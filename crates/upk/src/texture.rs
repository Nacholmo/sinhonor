//! Cooked `Texture2D` reading (file version 801) and DXT decoding to RGBA8.
//!
//! After the tagged properties a cooked texture stores an (empty) `SourceArt` bulk record, then
//! its mips: a count, and per mip a bulk record (`flags, element count, size on disk, offset`),
//! the inline payload unless the mip lives in a texture file cache (`.tfc`, flag 0x1), then the
//! mip's width and height. Flag 0x10 marks LZO-compressed payloads, stored as the same chunk
//! format packages use.

use crate::{reader::Reader, Error, Package, Result, Value};
use std::path::Path;

const BULK_SEPARATE_FILE: u32 = 0x01;
const BULK_UNUSED: u32 = 0x20;
const BULK_LZO: u32 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Dxt1,
    Dxt3,
    Dxt5,
    A8R8G8B8,
    G8,
    Other,
}

/// A decoded texture mip, RGBA8, rows top to bottom.
#[derive(Clone, Debug)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

struct MipRef {
    width: u32,
    height: u32,
    flags: u32,
    size_on_disk: usize,
    offset: usize,
    inline: Option<Vec<u8>>,
}

impl Package {
    /// Decodes a `Texture2D` export to RGBA8, choosing the largest mip not wider than
    /// `max_size` that can be read (inline in the package, or from `tfc_dir/<cache>.tfc`).
    pub fn texture_rgba(&self, export: usize, max_size: u32, tfc_dir: Option<&Path>) -> Result<Rgba> {
        let (props, tail) = self.properties_and_tail(export)?;
        let format = match crate::lookup(&props, "Format") {
            Some(Value::Enum(e)) => match e.as_str() {
                "PF_DXT1" => PixelFormat::Dxt1,
                "PF_DXT3" => PixelFormat::Dxt3,
                "PF_DXT5" => PixelFormat::Dxt5,
                "PF_A8R8G8B8" => PixelFormat::A8R8G8B8,
                "PF_G8" => PixelFormat::G8,
                _ => PixelFormat::Other,
            },
            // A missing enum means the first value, PF_Unknown... except UE3 cooks DXT1 as non-default.
            _ => PixelFormat::A8R8G8B8,
        };
        if format == PixelFormat::Other {
            return Err(Error::Unsupported("texture pixel format".into()));
        }
        let cache = match crate::lookup(&props, "TextureFileCacheName") {
            Some(Value::Name(n)) => Some(n.clone()),
            _ => None,
        };
        let data = self.export_data(export);
        let mut r = Reader::new(data);
        r.seek(tail);
        // SourceArt bulk record (cooked: empty).
        let sa_flags = r.u32()?;
        r.i32()?;
        let sa_size = r.i32()?.max(0) as usize;
        r.i32()?;
        if sa_flags & (BULK_SEPARATE_FILE | BULK_UNUSED) == 0 {
            r.skip(sa_size)?;
        }
        let count = r.i32()?;
        if !(0..=16).contains(&count) {
            return Err(Error::Unsupported("implausible mip count".into()));
        }
        let mut mips = Vec::new();
        for _ in 0..count {
            let flags = r.u32()?;
            r.i32()?; // element count
            let size_on_disk = r.i32()?.max(0) as usize;
            let offset = r.i32()?.max(0) as usize;
            let inline = if flags & (BULK_SEPARATE_FILE | BULK_UNUSED) == 0 && size_on_disk > 0 {
                Some(r.bytes(size_on_disk)?.to_vec())
            } else {
                None
            };
            let width = r.i32()?.max(0) as u32;
            let height = r.i32()?.max(0) as u32;
            mips.push(MipRef { width, height, flags, size_on_disk, offset, inline });
        }
        let tfc_path = match (tfc_dir, &cache) {
            (Some(dir), Some(name)) => Some(dir.join(format!("{name}.tfc"))),
            _ => None,
        };
        for m in mips.iter().filter(|m| m.width <= max_size && m.width > 0) {
            let raw = match (&m.inline, &tfc_path) {
                (Some(d), _) => d.clone(),
                (None, Some(p)) if m.flags & BULK_SEPARATE_FILE != 0 => match read_range(p, m.offset, m.size_on_disk) {
                    Some(d) => d,
                    None => continue,
                },
                _ => continue,
            };
            let raw = if m.flags & BULK_LZO != 0 { decompress_chunk(&raw)? } else { raw };
            return decode(format, m.width, m.height, &raw);
        }
        Err(Error::Unsupported("no readable mip".into()))
    }
}

/// Reads just `len` bytes at `offset` (texture caches are hundreds of MB).
fn read_range(path: &Path, offset: usize, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// One compressed chunk: tag, block size, total sizes, block table, LZO blocks.
fn decompress_chunk(d: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(d);
    if r.u32()? != crate::PACKAGE_TAG {
        return Err(Error::Unsupported("bad bulk chunk tag".into()));
    }
    r.u32()?;
    r.u32()?;
    let total = r.u32()? as usize;
    let mut blocks = Vec::new();
    let mut got = 0;
    while got < total {
        let (c, u) = (r.u32()? as usize, r.u32()? as usize);
        blocks.push((c, u));
        got += u;
    }
    let mut out = Vec::with_capacity(total);
    for (c, u) in blocks {
        let b = r.bytes(c)?;
        let dec = lzokay_native::decompress_all(b, Some(u)).map_err(|e| Error::Lzo(format!("{e:?}")))?;
        out.extend_from_slice(&dec);
    }
    Ok(out)
}

fn decode(format: PixelFormat, w: u32, h: u32, d: &[u8]) -> Result<Rgba> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let need = |n: usize| if d.len() < n { Err(Error::Truncated("mip data")) } else { Ok(()) };
    match format {
        PixelFormat::A8R8G8B8 => {
            need((w * h * 4) as usize)?;
            for (o, i) in px.chunks_exact_mut(4).zip(d.chunks_exact(4)) {
                o.copy_from_slice(&[i[2], i[1], i[0], i[3]]);
            }
        }
        PixelFormat::G8 => {
            need((w * h) as usize)?;
            for (o, &g) in px.chunks_exact_mut(4).zip(d) {
                o.copy_from_slice(&[g, g, g, 255]);
            }
        }
        PixelFormat::Dxt1 | PixelFormat::Dxt3 | PixelFormat::Dxt5 => {
            let block = if format == PixelFormat::Dxt1 { 8 } else { 16 };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            need((bw * bh) as usize * block)?;
            for by in 0..bh {
                for bx in 0..bw {
                    let b = &d[((by * bw + bx) as usize) * block..][..block];
                    let texels = match format {
                        PixelFormat::Dxt1 => dxt_color(b, true, None),
                        PixelFormat::Dxt3 => dxt_color(&b[8..], false, Some(dxt3_alpha(&b[..8]))),
                        _ => dxt_color(&b[8..], false, Some(dxt5_alpha(&b[..8]))),
                    };
                    for (k, t) in texels.iter().enumerate() {
                        let (x, y) = (bx * 4 + (k as u32 % 4), by * 4 + (k as u32 / 4));
                        if x < w && y < h {
                            let o = ((y * w + x) * 4) as usize;
                            px[o..o + 4].copy_from_slice(t);
                        }
                    }
                }
            }
        }
        PixelFormat::Other => unreachable!(),
    }
    Ok(Rgba { width: w, height: h, pixels: px })
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

fn dxt_color(b: &[u8], dxt1: bool, alpha: Option<[u8; 16]>) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u32, wb: u32| ((a as u32 * wa + b as u32 * wb) / (wa + wb)) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !dxt1 {
        pal[2] = [mix(p0[0], p1[0], 2, 1), mix(p0[1], p1[1], 2, 1), mix(p0[2], p1[2], 2, 1), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2), mix(p0[1], p1[1], 1, 2), mix(p0[2], p1[2], 1, 2), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1), mix(p0[1], p1[1], 1, 1), mix(p0[2], p1[2], 1, 1), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for (k, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (2 * k)) & 3) as usize];
        if let Some(a) = alpha {
            o[3] = a[k];
        }
    }
    out
}

fn dxt3_alpha(b: &[u8]) -> [u8; 16] {
    let mut a = [0u8; 16];
    for (k, v) in a.iter_mut().enumerate() {
        let nib = (b[k / 2] >> (4 * (k % 2))) & 15;
        *v = nib * 17;
    }
    a
}

fn dxt5_alpha(b: &[u8]) -> [u8; 16] {
    let (a0, a1) = (b[0] as u32, b[1] as u32);
    let mut pal = [0u8; 8];
    pal[0] = a0 as u8;
    pal[1] = a1 as u8;
    if a0 > a1 {
        for i in 1..7 {
            pal[i + 1] = ((a0 * (7 - i as u32) + a1 * i as u32) / 7) as u8;
        }
    } else {
        for i in 1..5 {
            pal[i + 1] = ((a0 * (5 - i as u32) + a1 * i as u32) / 5) as u8;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let bits = b[2..8].iter().rev().fold(0u64, |acc, &x| (acc << 8) | x as u64);
    let mut a = [0u8; 16];
    for (k, v) in a.iter_mut().enumerate() {
        *v = pal[((bits >> (3 * k)) & 7) as usize];
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_solid_block() {
        // c0 = pure red (0xF800), all indices 0.
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let t = dxt_color(&block, true, None);
        assert_eq!(t[0], [255, 0, 0, 255]);
        assert_eq!(t[15], [255, 0, 0, 255]);
    }

    #[test]
    fn dxt5_alpha_endpoints() {
        let b = [255, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(dxt5_alpha(&b)[0], 255);
    }
}
