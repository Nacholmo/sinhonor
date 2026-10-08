//! LOD 0 of Dishonored's cooked static meshes (801/30). Layout checked against the
//! install and the UE Viewer format reference; no mesh data is embedded here.
use crate::{reader::Reader, Error, Package, Result};

#[derive(Clone, Debug)]
pub struct StaticVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct StaticMesh {
    pub vertices: Vec<StaticVertex>,
    pub indices: Vec<u32>,
}

fn count(r: &mut Reader<'_>) -> Result<usize> {
    let n = r.u32()? as usize;
    if n > 1_000_000 {
        return Err(Error::Unsupported("static mesh array too large".into()));
    }
    Ok(n)
}

fn bulk<'a>(r: &mut Reader<'a>) -> Result<(usize, &'a [u8])> {
    let stride = count(r)?;
    let n = count(r)?;
    let bytes = stride.checked_mul(n).ok_or(Error::Truncated("static mesh array"))?;
    Ok((stride, r.bytes(bytes)?))
}

fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = ((h >> 10) & 31) as i32;
    let mantissa = (h & 1023) as f32;
    match exponent {
        0 => sign * mantissa * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15),
    }
}

impl Package {
    pub fn static_mesh(&self, export: usize) -> Result<StaticMesh> {
        let (_, tail) = self.properties_and_tail(export)?;
        read_lod0(self.export_data(export), tail)
    }
}

fn read_lod0(data: &[u8], tail: usize) -> Result<StaticMesh> {
    let mut r = Reader::new(data);
    r.seek(tail);
    r.skip(28 + 4)?; // bounds and BodySetup reference
    bulk(&mut r)?; // old-format kDOP nodes, also used in Dishonored 801
    bulk(&mut r)?; // collision triangles
    r.skip(4)?; // internal mesh version
    if count(&mut r)? == 0 {
        return Err(Error::Unsupported("static mesh has no LOD".into()));
    }
    let flags = r.u32()?;
    r.skip(4)?; // raw triangle element count
    let bytes = count(&mut r)?;
    r.skip(4)?; // bulk file offset
    if bytes > 0 {
        if flags & 1 != 0 {
            return Err(Error::Unsupported("external raw mesh triangles".into()));
        }
        r.skip(bytes)?;
    }
    let sections = count(&mut r)?;
    for _ in 0..sections {
        r.skip(36)?; // material, collision/shadow flags, index and vertex ranges
        let fragments = count(&mut r)?;
        r.skip(fragments * 8)?;
        if r.u8()? != 0 {
            return Err(Error::Unsupported("PS3 static mesh section".into()));
        }
    }
    let position_stride = count(&mut r)?;
    let n = count(&mut r)?;
    let (stride, positions) = bulk(&mut r)?;
    if stride != 12 || position_stride != 12 || positions.len() != n * 12 {
        return Err(Error::Unsupported("static mesh position stream".into()));
    }
    let texcoords = count(&mut r)?;
    let uv_stride = count(&mut r)?;
    let uv_count = count(&mut r)?;
    let full_uv = r.u32()? != 0;
    let (stride, attributes) = bulk(&mut r)?;
    let uv_bytes = if full_uv { 8 } else { 4 };
    if texcoords == 0 || stride != uv_stride || stride < 8 + texcoords * uv_bytes || uv_count != n || attributes.len() != n * stride {
        return Err(Error::Unsupported("static mesh UV stream".into()));
    }
    let mut vertices = Vec::with_capacity(n);
    for (p, a) in positions.chunks_exact(12).zip(attributes.chunks_exact(stride)) {
        let mut pr = Reader::new(p);
        let mut ar = Reader::new(&a[8..]); // tangent X and normal Z are packed into eight bytes
        let uv = if full_uv {
            [ar.f32()?, ar.f32()?]
        } else {
            [half(ar.u16()?), half(ar.u16()?)]
        };
        vertices.push(StaticVertex {
            position: [pr.f32()?, pr.f32()?, pr.f32()?],
            uv,
            color: [1.0; 4],
        });
    }
    r.skip(4)?; // color stride (zero for absent colors)
    let colors = count(&mut r)?;
    if colors != 0 {
        let (stride, data) = bulk(&mut r)?;
        if stride != 4 || colors != n || data.len() != n * 4 {
            return Err(Error::Unsupported("static mesh color stream".into()));
        }
        for (v, c) in vertices.iter_mut().zip(data.chunks_exact(4)) {
            v.color = [c[2] as f32 / 255.0, c[1] as f32 / 255.0, c[0] as f32 / 255.0, c[3] as f32 / 255.0];
        }
    }
    if count(&mut r)? != n {
        return Err(Error::Unsupported("static mesh vertex count mismatch".into()));
    }
    let (stride, data) = bulk(&mut r)?;
    if stride != 2 {
        return Err(Error::Unsupported("static mesh index width".into()));
    }
    let indices: Vec<u32> = data.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32).collect();
    if indices.len() % 3 != 0 || indices.iter().any(|&i| i as usize >= n) {
        return Err(Error::Unsupported("static mesh index out of range".into()));
    }
    if vertices.iter().any(|v| v.position.iter().chain(v.uv.iter()).any(|f| !f.is_finite())) {
        return Err(Error::Unsupported("non-finite static mesh vertex".into()));
    }
    Ok(StaticMesh { vertices, indices })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Original synthetic triangle, with two half-precision UV sets and BGRA colors.
    fn triangle() -> Vec<u8> {
        let mut d = vec![0; 32];
        let word = |d: &mut Vec<u8>, n: u32| d.extend_from_slice(&n.to_le_bytes());
        for n in [32, 0, 8, 0, 18, 1, 0, 0, 0, 0, 1] {
            word(&mut d, n);
        }
        d.extend_from_slice(&[0; 36]);
        word(&mut d, 0); // no fragments
        d.push(0); // no PS3 section
        for n in [12, 3, 12, 3] {
            word(&mut d, n);
        }
        for position in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for v in position {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        for n in [2, 16, 3, 0, 16, 3] {
            word(&mut d, n);
        }
        for _ in 0..3 {
            d.extend_from_slice(&[0; 8]); // packed tangents
            for uv in [0x3800u16, 0x3c00, 0, 0] {
                d.extend_from_slice(&uv.to_le_bytes());
            }
        }
        for n in [4, 3, 4, 3] {
            word(&mut d, n);
        }
        d.extend_from_slice(&[0, 128, 255, 255].repeat(3));
        for n in [3, 2, 3] {
            word(&mut d, n);
        }
        for i in [0u16, 1, 2] {
            d.extend_from_slice(&i.to_le_bytes());
        }
        d
    }

    #[test]
    fn reads_positions_half_uvs_colors_and_indices() {
        let mesh = read_lod0(&triangle(), 0).unwrap();
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert_eq!(mesh.vertices[1].position, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[0].uv, [0.5, 1.0]);
        assert_eq!(mesh.vertices[0].color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
    }

    #[test]
    fn rejects_truncation_large_arrays_and_invalid_indices() {
        let mut d = triangle();
        assert!(read_lod0(&d[..d.len() - 1], 0).is_err());
        let end = d.len();
        d[end - 2..].copy_from_slice(&99u16.to_le_bytes());
        assert!(read_lod0(&d, 0).is_err());
        d[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_lod0(&d, 0).is_err());
    }
}
