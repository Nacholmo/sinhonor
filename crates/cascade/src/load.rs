//! Loading `ParticleSystem` exports (and their materials) from a cooked package.

use crate::{Alignment, Blend, Burst, Cylinder, Dist, EmitterDef, SpriteMaterial, SystemDef, Vec3Axis};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use upk::{lookup, ObjRef, Package, Property, Value};

/// Largest texture edge kept for sprites.
const MAX_TEXTURE: u32 = 256;

/// Builds sprite materials from material instances, caching by object path.
/// Also supplies module class defaults: UE3 only serializes values that differ from the class
/// default object, so omitted distributions come from `Engine.upk`'s `Default__ParticleModule*`.
pub struct MaterialLoader {
    tfc_dir: Option<PathBuf>,
    cache: Mutex<HashMap<String, Arc<SpriteMaterial>>>,
    engine: Option<Package>,
    class_defaults: Mutex<HashMap<String, Vec<Property>>>,
}

impl MaterialLoader {
    pub fn new(tfc_dir: Option<PathBuf>) -> Self {
        Self { tfc_dir, cache: Mutex::new(HashMap::new()), engine: None, class_defaults: Mutex::new(HashMap::new()) }
    }

    /// Uses `engine` (the install's `Engine.upk`) for module class defaults.
    pub fn with_engine_defaults(mut self, engine: Package) -> Self {
        self.engine = Some(engine);
        self
    }

    fn class_default(&self, class: &str) -> Vec<Property> {
        if let Some(p) = self.class_defaults.lock().unwrap().get(class) {
            return p.clone();
        }
        let props = self
            .engine
            .as_ref()
            .and_then(|e| e.find_export(&format!("Default__{class}")).and_then(|i| e.properties(i).ok()))
            .unwrap_or_default();
        self.class_defaults.lock().unwrap().insert(class.to_string(), props.clone());
        props
    }

    fn get(&self, pkg: &Package, r: ObjRef) -> Option<Arc<SpriteMaterial>> {
        let path = pkg.object_path(r);
        if let Some(m) = self.cache.lock().unwrap().get(&path) {
            return Some(m.clone());
        }
        let m = Arc::new(self.build(pkg, r, &path));
        self.cache.lock().unwrap().insert(path, m.clone());
        Some(m)
    }

    /// Material model: the parent material decides blending; the first diffuse/shape texture
    /// gives colour and the opacity (or shape) texture's red channel gives coverage. Unknown
    /// parents fall back to a soft round sprite. See NOTES.md.
    fn build(&self, pkg: &Package, r: ObjRef, path: &str) -> SpriteMaterial {
        let props = match r {
            ObjRef::Export(e) => pkg.properties(e).unwrap_or_default(),
            _ => Vec::new(),
        };
        let parent = match lookup(&props, "Parent") {
            Some(Value::Object(i)) => pkg.object_path(ObjRef::from_index(*i)),
            _ => String::new(),
        };
        let p = parent.to_ascii_lowercase();
        let blend = if p.contains("translu") || p.contains("watersplash") || p.contains("smoke") { Blend::Translucent } else { Blend::Additive };
        let glow = p.contains("glow");

        let mut textures: HashMap<String, ObjRef> = HashMap::new();
        if let Some(Value::Array { count, raw }) = lookup(&props, "TextureParameterValues") {
            for e in pkg.struct_array(*count, raw).unwrap_or_default() {
                if let (Some(Value::Name(n)), Some(Value::Object(i))) = (lookup(&e, "ParameterName"), e.iter().find(|q| q.name == "ParameterValue").map(|q| &q.value)) {
                    if *i != 0 {
                        textures.insert(n.to_ascii_lowercase(), ObjRef::from_index(*i));
                    }
                }
            }
        }
        let mut tint = [1.0f32; 4];
        if let Some(Value::Array { count, raw }) = lookup(&props, "VectorParameterValues") {
            for e in pkg.struct_array(*count, raw).unwrap_or_default() {
                let name = match lookup(&e, "ParameterName") {
                    Some(Value::Name(n)) => n.to_ascii_lowercase(),
                    _ => continue,
                };
                if name == "color" || name == "c_color" || (name == "g_glowcolor" && tint == [1.0; 4]) {
                    if let Some(Value::Raw { raw, .. }) = e.iter().find(|q| q.name == "ParameterValue").map(|q| &q.value) {
                        if raw.len() == 16 {
                            let f: Vec<f32> = raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                            tint = [f[0], f[1], f[2], f[3].clamp(0.0, 4.0)];
                        }
                    }
                }
            }
        }

        let tex = |keys: &[&str]| -> Option<upk::texture::Rgba> {
            keys.iter().find_map(|k| {
                let (_, r) = textures.iter().find(|(n, _)| n.contains(k))?;
                match r {
                    ObjRef::Export(e) => pkg.texture_rgba(*e, MAX_TEXTURE, self.tfc_dir.as_deref()).ok(),
                    _ => None,
                }
            })
        };
        let diffuse = tex(&["diffuse", "d_"]);
        let opacity = tex(&["opacity", "shape", "o_"]);
        let size = 64u32;
        let (w, h) = diffuse.as_ref().or(opacity.as_ref()).map_or((size, size), |t| (t.width, t.height));
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let radial = {
                    let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
                    (1.0 - d).clamp(0.0, 1.0).powf(1.5)
                };
                let sample = |t: &upk::texture::Rgba| {
                    let (tx, ty) = ((u * t.width as f32) as u32 % t.width, (v * t.height as f32) as u32 % t.height);
                    let i = ((ty * t.width + tx) * 4) as usize;
                    [t.pixels[i], t.pixels[i + 1], t.pixels[i + 2], t.pixels[i + 3]]
                };
                let rgb = diffuse.as_ref().map(&sample).map_or([255, 255, 255], |c| [c[0], c[1], c[2]]);
                let coverage = match (&opacity, &diffuse) {
                    (Some(t), _) => sample(t)[0] as f32 / 255.0,
                    (None, Some(t)) => {
                        let c = sample(t);
                        if c[3] < 255 { c[3] as f32 / 255.0 } else { (c[0].max(c[1]).max(c[2]) as f32 / 255.0) * radial.sqrt() }
                    }
                    (None, None) => radial,
                };
                // Glow materials shape a soft radial glow, modulated by their noise texture.
                let coverage = if glow { radial * (0.35 + 0.65 * coverage) } else { coverage };
                rgba[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], (coverage * 255.0) as u8]);
            }
        }
        SpriteMaterial { name: path.to_string(), blend, width: w, height: h, rgba, tint }
    }
}

fn dist(props: &[Property], field: &str, components: usize) -> Option<Dist> {
    let fields = match lookup(props, field)? {
        Value::Struct { fields, .. } => fields,
        _ => return None,
    };
    let values: Vec<f32> = match lookup(fields, "LookupTable") {
        Some(Value::Array { raw, .. }) => raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect(),
        _ => return None,
    };
    let byte = |k: &str| match lookup(fields, k) {
        Some(Value::Byte(b)) => Some(*b as usize),
        _ => None,
    };
    let f = |k: &str| lookup(fields, k).and_then(Value::as_f32);
    let elements = byte("LookupTableNumElements").unwrap_or(1);
    Some(Dist {
        values,
        elements,
        chunk: byte("LookupTableChunkSize").unwrap_or(components * elements),
        time_scale: f("LookupTableTimeScale").unwrap_or(0.0),
        start_time: f("LookupTableStartTime").unwrap_or(0.0),
    })
}

/// Overlays `over` on `base`, merging struct fields so partially serialized structs keep the
/// default's lookup tables.
fn merge(base: &[Property], over: &[Property]) -> Vec<Property> {
    let mut out = base.to_vec();
    for p in over {
        match out.iter_mut().find(|q| q.name == p.name && q.array_index == p.array_index) {
            Some(q) => {
                if let (Value::Struct { fields: bf, .. }, Value::Struct { name, fields: of }) = (&q.value, &p.value) {
                    q.value = Value::Struct { name: name.clone(), fields: merge(bf, of) };
                } else {
                    *q = p.clone();
                }
            }
            None => out.push(p.clone()),
        }
    }
    out
}

fn obj_props(pkg: &Package, i: i32) -> Option<(usize, Vec<Property>)> {
    match ObjRef::from_index(i) {
        ObjRef::Export(e) => pkg.properties(e).ok().map(|p| (e, p)),
        _ => None,
    }
}

fn axis(name: &str) -> Option<Vec3Axis> {
    Some(match name {
        "EPAL_X" => Vec3Axis::X,
        "EPAL_Y" => Vec3Axis::Y,
        "EPAL_Z" => Vec3Axis::Z,
        "EPAL_NEGATIVE_X" => Vec3Axis::NegX,
        "EPAL_NEGATIVE_Y" => Vec3Axis::NegY,
        "EPAL_NEGATIVE_Z" => Vec3Axis::NegZ,
        _ => return None,
    })
}

/// Loads a particle system by object path (e.g. `Vfx_GamePlay.Blink.Blink_Ground_01`).
pub fn load_system(pkg: &Package, path: &str, materials: &MaterialLoader) -> Option<SystemDef> {
    let ps = pkg.find_export(path)?;
    let props = pkg.properties(ps).ok()?;
    let mut sys = SystemDef { name: path.to_string(), warmup_time: lookup(&props, "WarmupTime").and_then(Value::as_f32).unwrap_or(0.0), ..Default::default() };
    let Some(Value::Array { raw, .. }) = lookup(&props, "Emitters") else { return Some(sys) };
    for em in Package::object_array(raw) {
        let ObjRef::Export(e) = em else { continue };
        let Ok(ep) = pkg.properties(e) else { continue };
        let Some(Value::Array { raw: lods, .. }) = lookup(&ep, "LODLevels") else { continue };
        let Some(ObjRef::Export(lod0)) = Package::object_array(lods).first().copied() else { continue };
        let Ok(lp) = pkg.properties(lod0) else { continue };
        if lookup(&lp, "bEnabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let mut def = EmitterDef { name: pkg.object_path(em), duration: 1.0, ..Default::default() };
        let mut modules = Vec::new();
        for k in ["RequiredModule", "SpawnModule", "TypeDataModule"] {
            if let Some(Value::Object(i)) = lookup(&lp, k) {
                modules.push(*i);
            }
        }
        if let Some(Value::Array { raw, .. }) = lookup(&lp, "Modules") {
            modules.extend(raw.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())));
        }
        for m in modules {
            let Some((mi, mp)) = obj_props(pkg, m) else { continue };
            let class = pkg.export_class_name(mi);
            let mp = merge(&materials.class_default(&class), &mp);
            if lookup(&mp, "bEnabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            match class.as_str() {
                "ParticleModuleRequired" => {
                    if let Some(Value::Object(i)) = lookup(&mp, "Material") {
                        if *i != 0 {
                            def.material = materials.get(pkg, ObjRef::from_index(*i));
                        }
                    }
                    def.local_space = lookup(&mp, "bUseLocalSpace").and_then(Value::as_bool).unwrap_or(false);
                    def.kill_on_deactivate = lookup(&mp, "bKillOnDeactivate").and_then(Value::as_bool).unwrap_or(false);
                    def.duration = lookup(&mp, "EmitterDuration").and_then(Value::as_f32).unwrap_or(1.0);
                    def.loops = lookup(&mp, "EmitterLoops").and_then(Value::as_f32).unwrap_or(0.0) as u32;
                    def.delay = lookup(&mp, "EmitterDelay").and_then(Value::as_f32).unwrap_or(0.0);
                    if let Some(Value::Enum(a)) = lookup(&mp, "ScreenAlignment") {
                        if a == "PSA_Velocity" {
                            def.alignment = Some(Alignment::Velocity);
                        }
                    }
                }
                "ParticleModuleSpawn" => {
                    def.spawn_rate = dist(&mp, "Rate", 1).unwrap_or_default();
                    def.spawn_rate_scale = dist(&mp, "RateScale", 1);
                    if let Some(Value::Array { count, raw }) = lookup(&mp, "BurstList") {
                        for b in pkg.struct_array(*count, raw).unwrap_or_default() {
                            let count = lookup(&b, "Count").and_then(Value::as_f32).unwrap_or(0.0) as u32;
                            let low = lookup(&b, "CountLow").and_then(Value::as_f32).filter(|v| *v >= 0.0).map(|v| v as u32);
                            let time = lookup(&b, "Time").and_then(Value::as_f32).unwrap_or(0.0);
                            def.bursts.push(Burst { count, count_low: low, time });
                        }
                    }
                }
                "ParticleModuleLifetime" => def.lifetime = dist(&mp, "Lifetime", 1).unwrap_or_default(),
                "ParticleModuleSize" => def.start_size = dist(&mp, "StartSize", 3).unwrap_or_default(),
                "ParticleModuleSizeMultiplyLife" => def.size_mult_life = dist(&mp, "LifeMultiplier", 3),
                "ParticleModuleSizeMultiplyVelocity" => def.size_mult_velocity = dist(&mp, "VelocityMultiplier", 3),
                "ParticleModuleColor" => {
                    def.start_color = dist(&mp, "StartColor", 3);
                    def.start_alpha = dist(&mp, "StartAlpha", 1);
                }
                "ParticleModuleColorOverLife" => {
                    def.color_over_life = dist(&mp, "ColorOverLife", 3);
                    def.alpha_over_life = dist(&mp, "AlphaOverLife", 1);
                }
                "ParticleModuleColorScaleOverLife" => {
                    def.color_scale_over_life = dist(&mp, "ColorScaleOverLife", 3);
                    def.alpha_scale_over_life = dist(&mp, "AlphaScaleOverLife", 1);
                }
                "ParticleModuleVelocity" => {
                    def.start_velocity = dist(&mp, "StartVelocity", 3).unwrap_or_default();
                    def.start_velocity_radial = dist(&mp, "StartVelocityRadial", 1).unwrap_or_default();
                    def.velocity_world_space = lookup(&mp, "bInWorldSpace").and_then(Value::as_bool).unwrap_or(false);
                }
                "ParticleModuleVelocityOverLifetime" => def.velocity_over_life = dist(&mp, "VelOverLife", 3),
                "ParticleModuleAcceleration" => def.acceleration = dist(&mp, "Acceleration", 3).unwrap_or_default(),
                "ParticleModuleLocation" => def.start_location = dist(&mp, "StartLocation", 3).unwrap_or_default(),
                "ParticleModuleLocationPrimitiveCylinder" => {
                    def.cylinder = Some(Cylinder {
                        radius: dist(&mp, "StartRadius", 1).unwrap_or_default(),
                        height: dist(&mp, "StartHeight", 1).unwrap_or_default(),
                        surface_only: lookup(&mp, "SurfaceOnly").and_then(Value::as_bool).unwrap_or(false),
                        velocity: lookup(&mp, "Velocity").and_then(Value::as_bool).unwrap_or(false),
                        velocity_scale: dist(&mp, "VelocityScale", 1).unwrap_or_default(),
                    })
                }
                "ParticleModuleRotation" => def.start_rotation = dist(&mp, "StartRotation", 1).unwrap_or_default(),
                "ParticleModuleRotationRate" => def.rotation_rate = dist(&mp, "StartRotationRate", 1).unwrap_or_default(),
                "ParticleModuleRotationRateMultiplyLife" => def.rotation_rate_mult_life = dist(&mp, "LifeMultiplier", 1),
                "ParticleModuleOrientationAxisLock" => {
                    if let Some(Value::Enum(a)) = lookup(&mp, "LockAxisFlags") {
                        if let Some(ax) = axis(a) {
                            def.alignment = Some(Alignment::Axis(ax));
                        }
                    }
                }
                "ParticleModuleTypeDataMesh" => def.is_mesh = true,
                _ => {}
            }
        }
        if def.lifetime.is_empty() {
            continue;
        }
        sys.emitters.push(def);
    }
    Some(sys)
}
