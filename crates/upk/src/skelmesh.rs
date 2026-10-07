//! Cooked `SkeletalMesh` reading for Dishonored (file version 801).
//!
//! Layout learned from UE Viewer's UE3 notes (MIT), which include Dishonored's two extra
//! fields: a user-bounds record before the bounds, and an Edge (PS3 animation) skeleton blob
//! before the reference skeleton. Only what's needed to draw a skinned LOD 0 is kept.

use crate::{reader::Reader, Error, ObjRef, Package, Result};

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    /// Bind-pose rotation relative to the parent, as stored (x, y, z, w).
    pub orientation: [f32; 4],
    /// Bind-pose translation relative to the parent.
    pub position: [f32; 3],
    /// Index of the parent bone (the root points at itself).
    pub parent: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct SkinVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Tangent (x, y, z) and the binormal sign UE3 keeps in the normal's fourth byte.
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    /// Global bone indices.
    pub bones: [u16; 4],
    pub weights: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Section {
    pub material: usize,
    pub first_index: usize,
    pub num_triangles: usize,
}

#[derive(Clone, Debug)]
pub struct SkeletalMesh {
    pub materials: Vec<ObjRef>,
    pub bones: Vec<Bone>,
    pub sections: Vec<Section>,
    pub indices: Vec<u32>,
    pub vertices: Vec<SkinVertex>,
    /// Edge animation skeleton blob (joint i = bone i); needed to decode Edge animations.
    pub edge_skeleton: Vec<u8>,
    /// Mesh-level origin offset and rotation (UE3 `MeshOrigin`, `RotOrigin` pitch/yaw/roll).
    pub mesh_origin: [f32; 3],
    pub rot_origin: [i32; 3],
}

fn packed_normal(v: u32) -> [f32; 4] {
    let c = |s: u32| ((v >> s) & 0xFF) as f32 / 127.5 - 1.0;
    [c(0), c(8), c(16), c(24)]
}

fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    match e {
        0 => s * m * 2f32.powi(-24),
        31 => s * f32::INFINITY,
        _ => s * (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

struct Chunk {
    first_vertex: usize,
    bones: Vec<u16>,
    count: usize,
}

impl Package {
    pub fn skeletal_mesh(&self, export: usize) -> Result<SkeletalMesh> {
        let (props, tail) = self.properties_and_tail(export)?;
        let has_vertex_colors = crate::lookup(&props, "bHasVertexColors").and_then(crate::Value::as_bool).unwrap_or(false);
        let data = self.export_data(export);
        let mut r = Reader::new(data);
        r.seek(tail);
        // Dishonored user bounds: bone name, offset, radius.
        r.skip(8 + 12 + 4)?;
        r.skip(28)?; // FBoxSphereBounds
        let n = r.i32()?.max(0) as usize;
        let mut materials = Vec::with_capacity(n);
        for _ in 0..n {
            materials.push(ObjRef::from_index(r.i32()?));
        }
        let mesh_origin = [r.f32()?, r.f32()?, r.f32()?];
        let rot_origin = [r.i32()?, r.i32()?, r.i32()?];
        let n = r.i32()?.max(0) as usize;
        let edge_skeleton = r.bytes(n)?.to_vec();
        let n = r.i32()?;
        if !(0..2048).contains(&n) {
            return Err(Error::Unsupported("implausible bone count".into()));
        }
        let mut bones = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let name = self.name(r.name_ref()?);
            r.u32()?; // flags
            let orientation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
            let position = [r.f32()?, r.f32()?, r.f32()?];
            r.i32()?; // num children
            let parent = r.i32()?.max(0) as usize;
            r.u32()?; // bone colour
            bones.push(Bone { name, orientation, position, parent });
        }
        r.i32()?; // skeletal depth
        let lods = r.i32()?;
        if lods < 1 {
            return Err(Error::Unsupported("skeletal mesh without LODs".into()));
        }

        // --- LOD 0 ---
        let n = r.i32()?.max(0) as usize;
        let mut sections = Vec::with_capacity(n);
        for _ in 0..n {
            let material = r.u16()? as usize;
            r.u16()?; // chunk
            let first_index = r.i32()?.max(0) as usize;
            let num_triangles = r.u16()? as usize;
            r.u8()?;
            sections.push(Section { material, first_index, num_triangles });
        }
        let item = r.i32()?;
        let count = r.i32()?.max(0) as usize;
        if item != 2 {
            return Err(Error::Unsupported(format!("index size {item}")));
        }
        let indices: Vec<u32> = r.bytes(count * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as u32).collect();
        let n = r.i32()?.max(0) as usize;
        r.skip(n * 2)?; // used bones
        let n = r.i32()?.max(0) as usize;
        let mut chunks = Vec::with_capacity(n);
        for _ in 0..n {
            let first_vertex = r.i32()?.max(0) as usize;
            let rigid = r.i32()?.max(0) as usize;
            r.skip(rigid * 61)?;
            let soft = r.i32()?.max(0) as usize;
            r.skip(soft * 68)?;
            let nb = r.i32()?.max(0) as usize;
            let bones_map: Vec<u16> = (0..nb).map(|_| r.u16()).collect::<Result<_>>()?;
            let num_rigid = r.i32()?.max(0) as usize;
            let num_soft = r.i32()?.max(0) as usize;
            r.i32()?; // max influences
            chunks.push(Chunk { first_vertex, bones: bones_map, count: num_rigid + num_soft });
        }
        r.i32()?; // size
        r.i32()?; // num vertices
        let n = r.i32()?.max(0) as usize;
        r.skip(n)?; // required bones
        // Raw-points bulk data.
        let flags = r.u32()?;
        r.i32()?;
        let size = r.i32()?.max(0) as usize;
        r.i32()?;
        if flags & 0x21 == 0 {
            r.skip(size)?;
        }
        r.i32()?; // num UV sets
        // GPU skin vertex buffer.
        let uv_sets = r.i32()?.max(1) as usize;
        let full_uvs = r.i32()? != 0;
        r.i32()?; // packed position (unused on PC)
        r.skip(24)?; // extension, origin
        let elem = r.i32()?.max(0) as usize;
        let count = r.i32()?.max(0) as usize;
        let uv_size = if full_uvs { 8 } else { 4 };
        if elem != 8 + 8 + 12 + uv_size * uv_sets {
            return Err(Error::Unsupported(format!("vertex size {elem} for {uv_sets} UV sets")));
        }
        let raw = r.bytes(elem * count)?;
        let mut vertices = Vec::with_capacity(count);
        for v in raw.chunks_exact(elem) {
            let mut vr = Reader::new(v);
            let tangent = packed_normal(vr.u32()?);
            let normal = packed_normal(vr.u32()?);
            let bi = [vr.u8()?, vr.u8()?, vr.u8()?, vr.u8()?];
            let bw = [vr.u8()?, vr.u8()?, vr.u8()?, vr.u8()?];
            let position = [vr.f32()?, vr.f32()?, vr.f32()?];
            let uv = if full_uvs { [vr.f32()?, vr.f32()?] } else { [half(vr.u16()?), half(vr.u16()?)] };
            vertices.push(SkinVertex {
                position,
                normal: [normal[0], normal[1], normal[2]],
                tangent: [tangent[0], tangent[1], tangent[2], if normal[3] < 0.0 { -1.0 } else { 1.0 }],
                uv,
                bones: [bi[0] as u16, bi[1] as u16, bi[2] as u16, bi[3] as u16],
                weights: bw.map(|w| w as f32 / 255.0),
            });
        }
        let _ = has_vertex_colors;
        // Chunk-local bone indices -> global.
        for c in &chunks {
            for v in vertices.iter_mut().skip(c.first_vertex).take(c.count) {
                for b in v.bones.iter_mut() {
                    *b = c.bones.get(*b as usize).copied().unwrap_or(0);
                }
            }
        }
        Ok(SkeletalMesh { materials, bones, sections, indices, vertices, edge_skeleton, mesh_origin, rot_origin })
    }
}
