//! Reader for Unreal Engine 3 packages as shipped with Dishonored (file version 801, licensee 30).
//!
//! Scope: what sinhonor needs to pull tuning data out of the user's own install at runtime:
//! the package summary, LZO-compressed chunk flattening, the name/import/export tables, and
//! tagged-property decoding of exported objects. Formats were learned from public references
//! (UELib by Eliot van Uytfanghe, MIT; UE Viewer by Gildor, MIT; deadYokai's ue3-tools docs)
//! and checked against the installed files. No game data is embedded here.

mod props;
mod reader;

pub use props::{lookup, Property, Value};

use reader::Reader;
use std::{fmt, fs, path::Path};

pub const PACKAGE_TAG: u32 = 0x9E2A83C1;
const PKG_STORE_COMPRESSED: u32 = 0x0200_0000;
const COMPRESS_LZO: u32 = 2;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    BadTag(u32),
    Truncated(&'static str),
    Unsupported(String),
    Lzo(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::BadTag(t) => write!(f, "not a UE3 package (tag {t:#x})"),
            Error::Truncated(what) => write!(f, "truncated while reading {what}"),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
            Error::Lzo(s) => write!(f, "lzo: {s}"),
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

/// An FName as stored in tables: index into the name table plus an instance number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameRef {
    pub index: i32,
    pub number: i32,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub class_package: NameRef,
    pub class_name: NameRef,
    pub outer: i32,
    pub name: NameRef,
}

#[derive(Clone, Debug)]
pub struct Export {
    pub class: i32,
    pub super_: i32,
    pub outer: i32,
    pub name: NameRef,
    pub archetype: i32,
    pub flags: u64,
    pub serial_size: i32,
    pub serial_offset: i32,
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub file_version: u16,
    pub licensee_version: u16,
    pub package_flags: u32,
    pub engine_version: i32,
    pub cooker_version: i32,
    pub compression_flags: u32,
}

/// A loaded (and, if needed, decompressed) package.
pub struct Package {
    data: Vec<u8>,
    pub summary: Summary,
    pub names: Vec<String>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
}

/// Object references inside a package: `> 0` is export `i - 1`, `< 0` is import `-i - 1`, 0 is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjRef {
    None,
    Export(usize),
    Import(usize),
}

impl ObjRef {
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => ObjRef::None,
            i if i > 0 => ObjRef::Export((i - 1) as usize),
            i => ObjRef::Import((-i - 1) as usize),
        }
    }
}

impl Package {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(fs::read(path)?)
    }

    pub fn from_bytes(raw: Vec<u8>) -> Result<Self> {
        let data = flatten(raw)?;
        let mut r = Reader::new(&data);
        let header = read_header(&mut r)?;
        let mut pkg = Package {
            summary: header.summary,
            names: Vec::with_capacity(header.name_count),
            imports: Vec::with_capacity(header.import_count),
            exports: Vec::with_capacity(header.export_count),
            data: Vec::new(),
        };

        r.seek(header.name_offset);
        for _ in 0..header.name_count {
            pkg.names.push(r.fstring()?);
            r.u64()?; // object flags of the name entry
        }

        r.seek(header.import_offset);
        for _ in 0..header.import_count {
            pkg.imports.push(Import {
                class_package: r.name_ref()?,
                class_name: r.name_ref()?,
                outer: r.i32()?,
                name: r.name_ref()?,
            });
        }

        r.seek(header.export_offset);
        for _ in 0..header.export_count {
            let class = r.i32()?;
            let super_ = r.i32()?;
            let outer = r.i32()?;
            let name = r.name_ref()?;
            let archetype = r.i32()?;
            let flags = r.u64()?;
            let serial_size = r.i32()?;
            let serial_offset = r.i32()?;
            r.u32()?; // export flags
            let net_objects = r.i32()?;
            r.skip(net_objects.max(0) as usize * 4)?;
            r.skip(16)?; // package guid
            r.u32()?; // package flags
            pkg.exports.push(Export { class, super_, outer, name, archetype, flags, serial_size, serial_offset });
        }

        pkg.data = data;
        Ok(pkg)
    }

    pub fn name(&self, n: NameRef) -> String {
        let base = self.names.get(n.index as usize).map(String::as_str).unwrap_or("<bad-name>");
        if n.number > 0 {
            format!("{base}_{}", n.number - 1)
        } else {
            base.to_string()
        }
    }

    fn object_name_and_outer(&self, r: ObjRef) -> Option<(String, i32)> {
        match r {
            ObjRef::None => None,
            ObjRef::Export(i) => self.exports.get(i).map(|e| (self.name(e.name), e.outer)),
            ObjRef::Import(i) => self.imports.get(i).map(|e| (self.name(e.name), e.outer)),
        }
    }

    /// Dotted path without the class, e.g. `Twk_Powers.Blink.Twk_Blink`.
    pub fn object_path(&self, r: ObjRef) -> String {
        let mut parts = Vec::new();
        let mut cur = r;
        while let Some((name, outer)) = self.object_name_and_outer(cur) {
            parts.push(name);
            cur = ObjRef::from_index(outer);
        }
        parts.reverse();
        parts.join(".")
    }

    /// Name of the class of an export (`Class` for class objects themselves).
    pub fn export_class_name(&self, i: usize) -> String {
        match ObjRef::from_index(self.exports[i].class) {
            ObjRef::None => "Class".to_string(),
            r => self.object_name_and_outer(r).map(|(n, _)| n).unwrap_or_default(),
        }
    }

    pub fn find_export(&self, path: &str) -> Option<usize> {
        (0..self.exports.len()).find(|&i| self.object_path(ObjRef::Export(i)).eq_ignore_ascii_case(path))
    }

    /// All exports whose class name matches.
    pub fn exports_of_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = usize> + 'a {
        (0..self.exports.len()).filter(move |&i| self.export_class_name(i) == class)
    }

    pub fn export_data(&self, i: usize) -> &[u8] {
        let e = &self.exports[i];
        let start = e.serial_offset.max(0) as usize;
        let end = (start + e.serial_size.max(0) as usize).min(self.data.len());
        &self.data[start.min(end)..end]
    }

    /// Decodes the tagged properties of a plain (non-class) exported object.
    pub fn properties(&self, i: usize) -> Result<Vec<Property>> {
        let data = self.export_data(i);
        // Plain UObjects start with their NetIndex; components and templates may carry extra
        // fields first. Find the first offset where a well-formed tag stream begins.
        for start in [4usize, 0, 8, 12, 16, 20, 24] {
            if start >= data.len() {
                break;
            }
            let mut r = Reader::new(data);
            r.seek(start);
            if let Ok(props) = props::read_tagged(&mut r, self) {
                if !props.is_empty() || data.len() <= start + 8 {
                    return Ok(props);
                }
            }
        }
        Err(Error::Unsupported(format!("no property stream in {}", self.object_path(ObjRef::Export(i)))))
    }

    /// Default values of a `ScriptStruct` export. Cooked structs end with their defaults as a
    /// tagged stream, so this takes the earliest offset whose stream ends exactly at the end of
    /// the export's data.
    pub fn struct_defaults(&self, i: usize) -> Result<Vec<Property>> {
        let data = self.export_data(i);
        for start in 0..data.len().saturating_sub(8) {
            let mut r = Reader::new(data);
            r.seek(start);
            if let Ok(props) = props::read_tagged(&mut r, self) {
                if r.remaining() == 0 && !props.is_empty() {
                    return Ok(props);
                }
            }
        }
        Err(Error::Unsupported(format!("no struct defaults in {}", self.object_path(ObjRef::Export(i)))))
    }
}

struct Header {
    summary: Summary,
    name_count: usize,
    name_offset: usize,
    export_count: usize,
    export_offset: usize,
    import_count: usize,
    import_offset: usize,
}

fn read_header(r: &mut Reader) -> Result<Header> {
    let tag = r.u32()?;
    if tag != PACKAGE_TAG {
        return Err(Error::BadTag(tag));
    }
    let file_version = r.u16()?;
    let licensee_version = r.u16()?;
    let ver = file_version as u32;
    r.i32()?; // header size
    r.fstring()?; // folder name
    let package_flags = r.u32()?;
    let name_count = r.i32()? as usize;
    let name_offset = r.i32()? as usize;
    let export_count = r.i32()? as usize;
    let export_offset = r.i32()? as usize;
    let import_count = r.i32()? as usize;
    let import_offset = r.i32()? as usize;
    if ver >= 415 {
        r.i32()?; // depends offset
    }
    if ver >= 623 {
        r.skip(12)?; // import/export guids offset + counts
    }
    if ver >= 584 {
        r.i32()?; // thumbnail table offset
    }
    r.skip(16)?; // guid
    let generations = r.i32()?.max(0) as usize;
    r.skip(generations * if ver >= 322 { 12 } else { 8 })?;
    let engine_version = r.i32()?;
    let cooker_version = r.i32()?;
    let compression_flags = r.u32()?;
    Ok(Header {
        summary: Summary { file_version, licensee_version, package_flags, engine_version, cooker_version, compression_flags },
        name_count,
        name_offset,
        export_count,
        export_offset,
        import_count,
        import_offset,
    })
}

/// Returns an uncompressed image of the package. A fully compressed UE3 package keeps its
/// summary raw and stores everything else as LZO chunks mapped to their uncompressed offsets.
/// The summary is rewritten without chunks (absolute offsets stay valid; the gap is zeroed).
pub fn flatten(src: Vec<u8>) -> Result<Vec<u8>> {
    let mut r = Reader::new(&src);
    let tag = r.u32()?;
    if tag != PACKAGE_TAG {
        return Err(Error::BadTag(tag));
    }
    let ver = r.u32()? & 0xFFFF;
    r.i32()?;
    r.fstring()?;
    let flags_pos = r.pos();
    let pkg_flags = r.u32()?;
    r.skip(24)?;
    if ver >= 415 {
        r.skip(4)?;
    }
    if ver >= 623 {
        r.skip(12)?;
    }
    if ver >= 584 {
        r.skip(4)?;
    }
    r.skip(16)?;
    let gens = r.i32()?.max(0) as usize;
    r.skip(gens * if ver >= 322 { 12 } else { 8 })?;
    r.skip(8)?;
    let comp_flags_pos = r.pos();
    let comp_flags = r.u32()?;
    let nchunks = r.i32()?.max(0) as usize;
    let chunks_pos = r.pos();
    let mut chunks = Vec::with_capacity(nchunks);
    for _ in 0..nchunks {
        chunks.push((r.u32()? as usize, r.u32()? as usize, r.u32()? as usize, r.u32()? as usize));
    }
    let after_chunks = r.pos();
    if comp_flags == 0 || chunks.is_empty() {
        return Ok(src);
    }
    if comp_flags != COMPRESS_LZO {
        return Err(Error::Unsupported(format!("compression flags {comp_flags}")));
    }

    let summary_end = chunks.iter().map(|c| c.2).min().unwrap_or(after_chunks);
    let total = chunks.iter().map(|c| c.0 + c.1).max().unwrap_or(0);
    let first_uncomp = chunks.iter().map(|c| c.0).min().unwrap_or(0);
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&src[..chunks_pos]);
    out.extend_from_slice(src.get(after_chunks..summary_end).ok_or(Error::Truncated("summary"))?);
    out[flags_pos..flags_pos + 4].copy_from_slice(&(pkg_flags & !PKG_STORE_COMPRESSED).to_le_bytes());
    out[comp_flags_pos..comp_flags_pos + 8].fill(0);
    if out.len() > first_uncomp {
        return Err(Error::Unsupported("summary overlaps first chunk".into()));
    }
    out.resize(total, 0);

    for (uoff, usize_, coff, _) in chunks {
        let mut c = Reader::new(&src);
        c.seek(coff);
        if c.u32()? != PACKAGE_TAG {
            return Err(Error::Unsupported(format!("bad chunk tag at {coff}")));
        }
        c.u32()?; // block size
        c.u32()?; // compressed size
        let total_u = c.u32()? as usize;
        let mut blocks = Vec::new();
        let mut got = 0;
        while got < total_u {
            let (bc, bu) = (c.u32()? as usize, c.u32()? as usize);
            blocks.push((bc, bu));
            got += bu;
        }
        if total_u != usize_ {
            return Err(Error::Unsupported(format!("chunk size mismatch at {coff}")));
        }
        let mut dpos = c.pos();
        let mut o = uoff;
        for (bc, bu) in blocks {
            let block = src.get(dpos..dpos + bc).ok_or(Error::Truncated("lzo block"))?;
            let dec = lzokay_native::decompress_all(block, Some(bu)).map_err(|e| Error::Lzo(format!("{e:?}")))?;
            if dec.len() != bu || o + bu > out.len() {
                return Err(Error::Lzo(format!("block size mismatch at {dpos}")));
            }
            out[o..o + bu].copy_from_slice(&dec);
            o += bu;
            dpos += bc;
        }
    }
    Ok(out)
}
