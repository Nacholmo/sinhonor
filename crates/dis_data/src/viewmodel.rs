//! Corvo's first-person arms and sword, with their textures and animations, loaded from the
//! install: `Engine.upk` (`Ply_Player.Skm_Player`, the arms and their 2048² textures),
//! `Startup.upk` (`Wpn_PlySwords.Wpn_PlySword01` and the `Ply_*` AnimSets, Edge-compressed).

use edge_anim::{Animation, Skeleton};
use std::collections::HashMap;
use std::path::Path;
use upk::skelmesh::SkeletalMesh;
use upk::texture::Rgba;
use upk::{lookup, ObjRef, Package, Value};

pub struct MeshPart {
    pub mesh: SkeletalMesh,
    pub diffuse: Option<Rgba>,
    pub normal: Option<Rgba>,
}

pub struct ViewModel {
    pub arms: MeshPart,
    pub sword: Option<MeshPart>,
    pub skeleton: Skeleton,
    /// Edge animations by sequence name (first-person `Ply_*` sets).
    pub anims: HashMap<String, Animation>,
    /// Arms mesh sockets by name: (bone, location, rotator pitch/yaw/roll).
    pub sockets: HashMap<String, (String, [f32; 3], [i32; 3])>,
    /// Rotation of the `RightHandWpn` socket, where the sword is held.
    pub sword_socket: Option<[i32; 3]>,
    pub warnings: Vec<String>,
}

/// Largest texture edge kept (the arms ship 2048²).
const MAX_TEXTURE: u32 = 1024;

fn texture(pkg: &Package, material: ObjRef, keys: &[&str], tfc: &Path) -> Option<Rgba> {
    let ObjRef::Export(e) = material else { return None };
    let props = pkg.properties(e).ok()?;
    let Some(Value::Array { count, raw }) = lookup(&props, "TextureParameterValues") else { return None };
    for p in pkg.struct_array(*count, raw).ok()? {
        let Some(Value::Name(n)) = lookup(&p, "ParameterName") else { continue };
        if !keys.iter().any(|k| n.eq_ignore_ascii_case(k)) {
            continue;
        }
        if let Some(Value::Object(i)) = p.iter().find(|q| q.name == "ParameterValue").map(|q| &q.value) {
            if let ObjRef::Export(t) = ObjRef::from_index(*i) {
                if let Ok(img) = pkg.texture_rgba(t, MAX_TEXTURE, Some(tfc)) {
                    return Some(img);
                }
            }
        }
    }
    None
}

fn part(pkg: &Package, path: &str, tfc: &Path) -> Result<MeshPart, String> {
    let i = pkg.find_export(path).ok_or_else(|| format!("{path} not found"))?;
    let mesh = pkg.skeletal_mesh(i).map_err(|e| format!("{path}: {e}"))?;
    let mat = mesh.materials.first().copied().unwrap_or(ObjRef::None);
    let diffuse = texture(pkg, mat, &["D_Diffuse"], tfc);
    let normal = texture(pkg, mat, &["N_Normals"], tfc);
    Ok(MeshPart { mesh, diffuse, normal })
}

pub fn load_viewmodel(install: &Path) -> Result<ViewModel, String> {
    let dir = install.join("DishonoredGame/CookedPCConsole");
    let engine = Package::open(dir.join("Engine.upk")).map_err(|e| e.to_string())?;
    let startup = Package::open(dir.join("Startup.upk")).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    let arms = part(&engine, "Ply_Player.Skm_Player", &dir)?;
    let skeleton = Skeleton::parse(&arms.mesh.edge_skeleton).map_err(|e| e.to_string())?;
    let sword = match part(&startup, "Wpn_PlySwords.Wpn_PlySword01", &dir) {
        Ok(p) => Some(p),
        Err(e) => {
            warnings.push(e);
            None
        }
    };
    let mut anims = HashMap::new();
    for i in startup.exports_of_class("AnimSequence") {
        if !startup.object_path(ObjRef::Export(i)).starts_with("Ply_") {
            continue;
        }
        let Ok((p, tail)) = startup.properties_and_tail(i) else { continue };
        let Some(Value::Name(name)) = lookup(&p, "SequenceName") else { continue };
        let d = startup.export_data(i);
        // RawAnimationData (empty when cooked), then CompressedByteStream.
        let Some(len) = d.get(tail + 4..tail + 8).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize) else { continue };
        let Some(blob) = d.get(tail + 8..tail + 8 + len) else { continue };
        match Animation::parse(blob) {
            Ok(a) => {
                anims.entry(name.clone()).or_insert(a);
            }
            Err(_) => continue, // the two non-Edge sequences
        }
    }
    let mut sockets = HashMap::new();
    if let Some(i) = engine.find_export("Ply_Player.Skm_Player") {
        if let Ok(p) = engine.properties(i) {
            if let Some(Value::Array { raw, .. }) = lookup(&p, "Sockets") {
                for s in Package::object_array(raw) {
                    let ObjRef::Export(si) = s else { continue };
                    let Ok(sp) = engine.properties(si) else { continue };
                    let (Some(Value::Name(name)), Some(Value::Name(bone))) = (lookup(&sp, "SocketName"), lookup(&sp, "BoneName")) else { continue };
                    let loc = match lookup(&sp, "RelativeLocation") {
                        Some(Value::Vector(v)) => *v,
                        _ => [0.0; 3],
                    };
                    let rot = match lookup(&sp, "RelativeRotation") {
                        Some(Value::Rotator(r)) => *r,
                        _ => [0; 3],
                    };
                    sockets.insert(name.clone(), (bone.clone(), loc, rot));
                }
            }
        }
    }
    let sword_socket = sockets.get("RightHandWpn").map(|s| s.2);
    if anims.is_empty() {
        warnings.push("no first-person animations decoded".into());
    }
    Ok(ViewModel { arms, sword, skeleton, anims, sockets, sword_socket, warnings })
}
