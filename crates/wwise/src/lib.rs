//! Minimal reader for the Wwise sound packages Dishonored ships (`*.pck`, AKPK v1 with
//! soundbank version 65): resolves an event name to what it plays and converts clips from
//! Wwise Vorbis to standard Ogg Vorbis (via the BSD-3 `ww2ogg` crate, a port of hcs's ww2ogg).
//!
//! Event ids are the FNV-1 32-bit hash of the lower-cased event name, as Wwise computes them.
//! Only what playback of one-shot events needs is decoded: Play actions, sounds and the
//! random/sequence, switch and layer containers. Container children are recovered by matching
//! the ids of sibling objects in the payload (skipping the parent field), which avoids
//! version-specific property parsing. Layout notes are in the repository's NOTES.md.

use std::{collections::HashMap, fmt, fs, io::Cursor, path::Path};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Format(String),
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Format(s) => write!(f, "format: {s}"),
            Error::Decode(s) => write!(f, "decode: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Wwise's id for a name: FNV-1 32-bit over the lower-cased bytes.
pub fn fnv1_32(name: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in name.to_ascii_lowercase().bytes() {
        h = h.wrapping_mul(16_777_619);
        h ^= b as u32;
    }
    h
}

/// What an event plays.
#[derive(Clone, Debug, PartialEq)]
pub enum PlayNode {
    /// One clip, by media id.
    Clip(u32),
    /// Pick one child (random/sequence and switch containers).
    OneOf(Vec<PlayNode>),
    /// Play every child together (layer containers, multiple Play actions).
    All(Vec<PlayNode>),
}

impl PlayNode {
    /// All media ids reachable from this node.
    pub fn clips(&self) -> Vec<u32> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }
    fn collect(&self, out: &mut Vec<u32>) {
        match self {
            PlayNode::Clip(id) => {
                if !out.contains(id) {
                    out.push(*id)
                }
            }
            PlayNode::OneOf(c) | PlayNode::All(c) => c.iter().for_each(|n| n.collect(out)),
        }
    }
    /// Media ids to play now, using `pick(n)` to choose among `n` alternatives.
    pub fn choose(&self, pick: &mut dyn FnMut(usize) -> usize) -> Vec<u32> {
        match self {
            PlayNode::Clip(id) => vec![*id],
            PlayNode::OneOf(c) if !c.is_empty() => c[pick(c.len()).min(c.len() - 1)].choose(pick),
            PlayNode::OneOf(_) => Vec::new(),
            PlayNode::All(c) => c.iter().flat_map(|n| n.choose(pick)).collect(),
        }
    }
}

const HIRC_SOUND: u8 = 2;
const HIRC_ACTION: u8 = 3;
const HIRC_EVENT: u8 = 4;
const HIRC_RANSEQ: u8 = 5;
const HIRC_SWITCH: u8 = 6;
const HIRC_LAYER: u8 = 9;

/// A loaded `.pck`: its soundbanks' objects and every clip it carries (embedded or streamed).
#[derive(Default)]
pub struct Package {
    objects: HashMap<u32, (u8, Vec<u8>)>,
    media: HashMap<u32, Vec<u8>>,
}

fn u32_at(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| Error::Format(format!("truncated at {o}")))
}

impl Package {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn from_bytes(d: &[u8]) -> Result<Self> {
        if d.get(..4) != Some(b"AKPK") {
            return Err(Error::Format("not an AKPK package".into()));
        }
        let lang_size = u32_at(d, 12)? as usize;
        let banks_size = u32_at(d, 16)? as usize;
        // This revision has one extra u32 before the language map.
        let banks_lut = 28 + lang_size;
        let streams_lut = banks_lut + banks_size;
        let lut = |o: usize| -> Result<Vec<(u32, usize, usize)>> {
            let n = u32_at(d, o)? as usize;
            (0..n)
                .map(|i| {
                    let e = o + 4 + 20 * i;
                    let (id, block, size, start) = (u32_at(d, e)?, u32_at(d, e + 4)?, u32_at(d, e + 8)?, u32_at(d, e + 12)?);
                    Ok((id, start as usize * block as usize, size as usize))
                })
                .collect()
        };
        let mut pkg = Package::default();
        for (_, off, size) in lut(banks_lut)? {
            let bank = d.get(off..off + size).ok_or_else(|| Error::Format("bank out of range".into()))?;
            pkg.add_bank(bank)?;
        }
        for (id, off, size) in lut(streams_lut)? {
            if let Some(b) = d.get(off..off + size) {
                pkg.media.entry(id).or_insert_with(|| b.to_vec());
            }
        }
        Ok(pkg)
    }

    fn add_bank(&mut self, b: &[u8]) -> Result<()> {
        let mut sections = HashMap::new();
        let mut o = 0;
        while o + 8 <= b.len() {
            let tag = [b[o], b[o + 1], b[o + 2], b[o + 3]];
            let len = u32_at(b, o + 4)? as usize;
            sections.insert(tag, (o + 8, len));
            o += 8 + len;
        }
        if let (Some(&(io, il)), Some(&(dofs, _))) = (sections.get(b"DIDX"), sections.get(b"DATA")) {
            for k in 0..il / 12 {
                let (id, mo, ms) = (u32_at(b, io + 12 * k)?, u32_at(b, io + 12 * k + 4)? as usize, u32_at(b, io + 12 * k + 8)? as usize);
                if let Some(m) = b.get(dofs + mo..dofs + mo + ms) {
                    self.media.insert(id, m.to_vec());
                }
            }
        }
        if let Some(&(ho, _)) = sections.get(b"HIRC") {
            let n = u32_at(b, ho)? as usize;
            let mut o = ho + 4;
            for _ in 0..n {
                let t = *b.get(o).ok_or_else(|| Error::Format("hirc truncated".into()))?;
                let len = u32_at(b, o + 1)? as usize;
                let id = u32_at(b, o + 5)?;
                let payload = b.get(o + 9..o + 5 + len).ok_or_else(|| Error::Format("hirc object out of range".into()))?;
                self.objects.insert(id, (t, payload.to_vec()));
                o += 5 + len;
            }
        }
        Ok(())
    }

    pub fn has_event(&self, name: &str) -> bool {
        self.objects.get(&fnv1_32(name)).is_some_and(|(t, _)| *t == HIRC_EVENT)
    }

    /// What the named event plays, or `None` if it isn't in this package.
    pub fn event(&self, name: &str) -> Option<PlayNode> {
        let id = fnv1_32(name);
        let (t, _) = self.objects.get(&id)?;
        if *t != HIRC_EVENT {
            return None;
        }
        self.node(id, 0).filter(|n| !n.clips().is_empty())
    }

    fn node(&self, id: u32, depth: usize) -> Option<PlayNode> {
        if depth > 12 {
            return None;
        }
        let (t, p) = self.objects.get(&id)?;
        match *t {
            HIRC_EVENT => {
                let n = u32_at(p, 0).ok()? as usize;
                let kids: Vec<_> = (0..n).filter_map(|k| self.node(u32_at(p, 4 + 4 * k).ok()?, depth + 1)).collect();
                Some(PlayNode::All(kids))
            }
            HIRC_ACTION => {
                let action_type = u16::from_le_bytes([*p.first()?, *p.get(1)?]);
                // 0x04xx = Play.
                (action_type >> 8 == 4).then(|| self.node(u32_at(p, 2).ok()?, depth + 1)).flatten()
            }
            HIRC_SOUND => {
                // Source data: plugin id, stream type, source (media) id.
                let media = u32_at(p, 8).ok()?;
                self.media.contains_key(&media).then_some(PlayNode::Clip(media))
            }
            HIRC_RANSEQ | HIRC_SWITCH | HIRC_LAYER => {
                // NodeBaseParams: override-fx flag, fx count, [fx...], bus id, parent id.
                let parent = (p.get(1) == Some(&0)).then(|| u32_at(p, 6).ok()).flatten();
                let mut kids = Vec::new();
                let mut seen = Vec::new();
                for k in 10..p.len().saturating_sub(3) {
                    let v = u32_at(p, k).ok()?;
                    if Some(v) == parent || v == id || seen.contains(&v) {
                        continue;
                    }
                    if let Some((ct, _)) = self.objects.get(&v) {
                        if matches!(*ct, HIRC_SOUND | HIRC_RANSEQ | HIRC_SWITCH | HIRC_LAYER) {
                            seen.push(v);
                            if let Some(n) = self.node(v, depth + 1) {
                                kids.push(n);
                            }
                        }
                    }
                }
                Some(if *t == HIRC_LAYER { PlayNode::All(kids) } else { PlayNode::OneOf(kids) })
            }
            _ => None,
        }
    }

    /// Raw Wwise media (a RIFF `.wem`).
    pub fn media(&self, id: u32) -> Option<&[u8]> {
        self.media.get(&id).map(Vec::as_slice)
    }

    /// Converts a clip to a standard Ogg Vorbis stream.
    pub fn ogg(&self, id: u32) -> Result<Vec<u8>> {
        let wem = self.media(id).ok_or_else(|| Error::Format(format!("no media {id:#x}")))?;
        wem_to_ogg(wem)
    }
}

/// Wwise Vorbis `.wem` to Ogg Vorbis.
///
/// Wwise strips Vorbis codebooks and references one of two packed libraries (standard or
/// aoTuV). Both are tried; each result is fully decoded and the one that decodes without
/// errors and with the least full-scale clipping wins. (`ww2ogg::validate` rejects loud
/// impact sounds that legitimately clip, so it isn't used as the arbiter.)
pub fn wem_to_ogg(wem: &[u8]) -> Result<Vec<u8>> {
    let mut best: Option<(f32, Vec<u8>)> = None;
    let mut last = String::new();
    for aotuv in [false, true] {
        let books = if aotuv { ww2ogg::CodebookLibrary::aotuv_codebooks() } else { ww2ogg::CodebookLibrary::default_codebooks() };
        let books = books.map_err(|e| Error::Decode(e.to_string()))?;
        let converted = ww2ogg::WwiseRiffVorbis::new(Cursor::new(wem), books).and_then(|mut c| {
            let mut out = Vec::new();
            c.generate_ogg(&mut out)?;
            Ok(out)
        });
        let ogg = match converted {
            Ok(o) => o,
            Err(e) => {
                last = e.to_string();
                continue;
            }
        };
        match clipping_ratio(&ogg) {
            Some(r) if best.as_ref().is_none_or(|(b, _)| r < *b) => best = Some((r, ogg)),
            Some(_) => {}
            None => last = "decoded stream is not valid Vorbis".into(),
        }
    }
    match best {
        Some((r, ogg)) if r < 0.2 => Ok(ogg),
        Some((r, _)) => Err(Error::Decode(format!("{:.0}% of samples clipped with either codebook", r * 100.0))),
        None => Err(Error::Decode(last)),
    }
}

/// Fraction of samples at 16-bit full scale, or `None` if the stream fails to decode.
fn clipping_ratio(ogg: &[u8]) -> Option<f32> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg)).ok()?;
    let (mut total, mut clipped) = (0usize, 0usize);
    loop {
        match r.read_dec_packet_itl() {
            Ok(Some(p)) => {
                total += p.len();
                clipped += p.iter().filter(|&&s| s == i16::MAX || s == i16::MIN).count();
            }
            Ok(None) => break,
            Err(_) => return None,
        }
    }
    (total > 0).then(|| clipped as f32 / total as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_matches_wwise() {
        // Known pair: Wwise hashes names case-insensitively with FNV-1.
        assert_eq!(fnv1_32("Play"), fnv1_32("play"));
        assert_eq!(fnv1_32(""), 2_166_136_261);
    }

    #[test]
    fn choose_respects_structure() {
        let n = PlayNode::All(vec![PlayNode::Clip(1), PlayNode::OneOf(vec![PlayNode::Clip(2), PlayNode::Clip(3)])]);
        assert_eq!(n.choose(&mut |_| 1), vec![1, 3]);
        assert_eq!(n.clips(), vec![1, 2, 3]);
    }
}
