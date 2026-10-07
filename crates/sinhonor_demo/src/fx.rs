//! Renders the game's Cascade particle effects (simulated by the `cascade` crate) as batched,
//! per-material billboard meshes, and attaches lens effects to the camera.

use crate::to_bevy;
use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::NoFrustumCulling;
use cascade::{Alignment, Blend, Instance};
use dis_data::effects::{Effects, Fx};
use dis_motion::Vec3 as UVec3;
use glam::{Affine3A, Mat3};
use std::collections::HashMap;

const SCALE: f32 = crate::SCALE;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Attach {
    World,
    /// Placed `lens_distance` in front of the camera every frame.
    Camera,
}

pub struct Live {
    pub id: u64,
    pub inst: Instance,
    pub attach: Attach,
}

#[derive(Resource)]
pub struct FxWorld {
    pub effects: Effects,
    pub live: Vec<Live>,
    next_id: u64,
    materials: HashMap<String, Handle<StandardMaterial>>,
    batches: HashMap<String, Handle<Mesh>>,
    /// Camera basis (Unreal space), updated each frame for lens effects.
    pub camera: Affine3A,
}

impl FxWorld {
    pub fn new(effects: Effects) -> Self {
        Self { effects, live: Vec::new(), next_id: 1, materials: HashMap::new(), batches: HashMap::new(), camera: Affine3A::IDENTITY }
    }

    /// Starts an effect; returns its id (0 if the effect isn't available).
    pub fn spawn(&mut self, fx: Fx, transform: Affine3A, attach: Attach) -> u64 {
        let Some(def) = self.effects.systems.get(&fx).cloned() else { return 0 };
        let id = self.next_id;
        self.next_id += 1;
        let t = if attach == Attach::Camera { self.lens_transform() } else { transform };
        let distance = (t.translation - self.camera.translation).length();
        self.live.push(Live { id, inst: Instance::new_at_distance(def, t, id.wrapping_mul(0x9E37_79B9_7F4A_7C15), distance), attach });
        id
    }

    pub fn set_transform(&mut self, id: u64, transform: Affine3A) {
        if let Some(l) = self.live.iter_mut().find(|l| l.id == id) {
            l.inst.transform = transform;
        }
    }

    pub fn stop(&mut self, id: u64) {
        if let Some(l) = self.live.iter_mut().find(|l| l.id == id) {
            l.inst.deactivate();
        }
    }

    fn lens_transform(&self) -> Affine3A {
        let fwd = self.camera.matrix3.x_axis;
        Affine3A { matrix3: self.camera.matrix3, translation: self.camera.translation + fwd * self.effects.lens_distance }
    }
}

/// Unreal-space transform at `pos` facing `yaw`.
pub fn placed(pos: UVec3, yaw: f32) -> Affine3A {
    Affine3A::from_mat3_translation(Mat3::from_rotation_z(yaw), pos)
}

/// Unreal rotator (pitch -90, yaw): local X points straight down, Z along the yaw direction.
pub fn pitched_down(pos: UVec3, yaw: f32) -> Affine3A {
    let (s, c) = yaw.sin_cos();
    let x = UVec3::new(0.0, 0.0, -1.0);
    let y = UVec3::new(-s, c, 0.0);
    let z = UVec3::new(c, s, 0.0);
    Affine3A::from_mat3_translation(Mat3::from_cols(x, y, z), pos)
}

/// Camera basis in Unreal space: X forward, Y right, Z up.
pub fn camera_basis(eye: UVec3, forward: UVec3) -> Affine3A {
    let f = forward.normalize_or_zero();
    let r = UVec3::Z.cross(f).normalize_or(UVec3::Y);
    let u = f.cross(r);
    Affine3A::from_mat3_translation(Mat3::from_cols(f, r, u), eye)
}

fn image_from(m: &cascade::SpriteMaterial) -> Image {
    Image::new(
        Extent3d { width: m.width, height: m.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        m.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Advances every live effect and rebuilds the per-material meshes.
pub fn update_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FxWorld>,
    cams: Query<&GlobalTransform, With<crate::PlayerCam>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let dt = time.delta_secs().min(0.1);
    let lens = fx.lens_transform();
    let eye = fx.camera.translation;
    for l in fx.live.iter_mut() {
        if l.attach == Attach::Camera {
            l.inst.transform = lens;
        }
        // Distance-based LOD, as the engine does (`LODDistances`).
        l.inst.set_camera_distance((l.inst.transform.translation - eye).length());
        l.inst.update(dt);
    }
    fx.live.retain(|l| !l.inst.is_finished());

    let Ok(cam) = cams.single() else { return };
    let (right, up, fwd) = (cam.right().as_vec3(), cam.up().as_vec3(), cam.forward().as_vec3());

    struct Batch {
        pos: Vec<[f32; 3]>,
        uv: Vec<[f32; 2]>,
        col: Vec<[f32; 4]>,
        nrm: Vec<[f32; 3]>,
        idx: Vec<u32>,
    }
    let mut batches: HashMap<String, Batch> = HashMap::new();
    let mut new_materials = Vec::new();
    for l in &fx.live {
        let defs = l.inst.emitter_defs();
        for p in l.inst.particles() {
            let e = &defs[p.emitter];
            if e.is_mesh {
                continue;
            }
            let Some(mat) = &e.material else { continue };
            if !fx.materials.contains_key(&mat.name) {
                new_materials.push(mat.clone());
            }
            let c = to_bevy(p.pos);
            let (sx, sy) = (p.size.x.abs() * SCALE * 0.5, p.size.y.abs() * SCALE * 0.5);
            let (a, b) = match e.alignment {
                Some(Alignment::Velocity) => {
                    // UE3 velocity alignment: Y along the velocity, X across it.
                    let v = to_bevy(p.velocity);
                    let d = (v - fwd * v.dot(fwd)).normalize_or(up);
                    let side = d.cross(fwd).normalize_or(right);
                    (side * sx, d * sy)
                }
                Some(Alignment::Axis(ax)) => {
                    let n = to_bevy(l.inst.transform.transform_vector3(ax.vec())).normalize_or(Vec3::Y);
                    let t0 = if n.y.abs() < 0.9 { Vec3::Y.cross(n).normalize() } else { Vec3::X.cross(n).normalize() };
                    let t1 = n.cross(t0);
                    let (s, co) = p.rotation.sin_cos();
                    ((t0 * co + t1 * s) * sx, (t1 * co - t0 * s) * sy)
                }
                _ => {
                    let (s, co) = p.rotation.sin_cos();
                    ((right * co + up * s) * sx, (up * co - right * s) * sy)
                }
            };
            let col = [p.color[0].max(0.0), p.color[1].max(0.0), p.color[2].max(0.0), p.color[3].clamp(0.0, 1.0)];
            let b_ = batches.entry(mat.name.clone()).or_insert_with(|| Batch { pos: vec![], uv: vec![], col: vec![], nrm: vec![], idx: vec![] });
            let base = b_.pos.len() as u32;
            for (corner, uv) in [(-a - b, [0.0, 1.0]), (a - b, [1.0, 1.0]), (a + b, [1.0, 0.0]), (-a + b, [0.0, 0.0])] {
                b_.pos.push((c + corner).into());
                b_.uv.push(uv);
                b_.col.push(col);
                b_.nrm.push((-fwd).into());
            }
            b_.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    for m in new_materials {
        if fx.materials.contains_key(&m.name) {
            continue;
        }
        let image = images.add(image_from(&m));
        // Tint alpha: an intensity for additive materials, an opacity for translucent ones.
        let t = m.tint;
        let (k, alpha) = if m.blend == Blend::Additive { (t[3].max(0.05), 1.0) } else { (1.0, t[3].clamp(0.0, 1.0)) };
        let handle = materials.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::new(t[0] * k, t[1] * k, t[2] * k, alpha)),
            base_color_texture: Some(image),
            unlit: true,
            alpha_mode: if m.blend == Blend::Additive { AlphaMode::Add } else { AlphaMode::Blend },
            cull_mode: None,
            double_sided: true,
            fog_enabled: false,
            ..default()
        });
        fx.materials.insert(m.name.clone(), handle);
    }
    // Upload batches; materials with no particles this frame get an empty (degenerate) mesh.
    let names: Vec<String> = fx.materials.keys().cloned().collect();
    for name in names {
        let b = batches.remove(&name).unwrap_or(Batch { pos: vec![[0.0; 3]; 3], uv: vec![[0.0; 2]; 3], col: vec![[0.0; 4]; 3], nrm: vec![[0.0, 1.0, 0.0]; 3], idx: vec![0, 1, 2] });
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, b.nrm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.col);
        mesh.insert_indices(Indices::U32(b.idx));
        match fx.batches.get(&name) {
            Some(h) => {
                if let Some(m) = meshes.get_mut(h) {
                    *m = mesh;
                }
            }
            None => {
                let h = meshes.add(mesh);
                let mat = fx.materials[&name].clone();
                commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(mat), Transform::IDENTITY, NoFrustumCulling, NotShadowCaster, NotShadowReceiver));
                fx.batches.insert(name, h);
            }
        }
    }
}
