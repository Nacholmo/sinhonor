//! Corvo's first-person arms and sword: the game's mesh, textures and Edge animations, posed
//! from the motion state, skinned on the CPU and drawn by a viewmodel camera on its own render
//! layer (so the arms never clip into walls).
//!
//! The game aligns the arms mesh so its `camera_jnt` bone is the view: in that bone's space +X is
//! up, +Y right and -Z forward, which maps straight onto a Bevy camera at the origin.

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::core_3d::Camera3d;
use bevy::prelude::*;
use bevy::render::camera::ClearColorConfig;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::{NoFrustumCulling, RenderLayers};
use dis_data::viewmodel::{MeshPart, ParticleNotify, ViewModel};
use dis_motion::{BlinkMode, MotionState};
use edge_anim::Joint;
use glam::{Mat4, Quat, Vec3 as GVec3};

pub const LAYER: usize = 1;
const SCALE: f32 = crate::SCALE;
/// Crossfade time between animations.
const BLEND: f32 = 0.18;

#[derive(Clone, PartialEq)]
struct Track {
    name: String,
    looping: bool,
}

#[derive(Default)]
struct Layer {
    current: Option<Track>,
    time: f32,
    previous: Option<(Track, f32)>,
    fade: f32,
}

impl Layer {
    fn play(&mut self, name: &str, looping: bool) {
        let t = Track { name: name.to_string(), looping };
        if self.current.as_ref() == Some(&t) {
            return;
        }
        if let Some(cur) = self.current.take() {
            self.previous = Some((cur, self.time));
            self.fade = BLEND;
        }
        self.current = Some(t);
        self.time = 0.0;
    }
    /// Advances the clock and returns the particle notifies of the current track passed in
    /// `[old time, new time)`, wrapping for loops.
    fn advance(&mut self, dt: f32, vm: &ViewModel) -> Vec<ParticleNotify> {
        let mut fired = Vec::new();
        if let Some(t) = &self.current {
            if let (Some(list), Some(a)) = (vm.particle_notifies.get(&t.name), vm.anims.get(&t.name)) {
                let (from, to) = (self.time, self.time + dt);
                for n in list {
                    let hit = if t.looping && a.duration > 0.0 {
                        let k = ((from - n.time) / a.duration).ceil();
                        n.time + k * a.duration < to
                    } else {
                        n.time >= from && n.time < to && n.time <= a.duration
                    };
                    if hit {
                        fired.push(n.clone());
                    }
                }
            }
        }
        self.time += dt;
        self.fade = (self.fade - dt).max(0.0);
        if let Some((_, t)) = self.previous.as_mut() {
            *t += dt;
        }
        if self.fade <= 0.0 {
            self.previous = None;
        }
        fired
    }
    /// Finished a one-shot?
    fn done(&self, vm: &ViewModel) -> bool {
        match &self.current {
            Some(t) if !t.looping => vm.anims.get(&t.name).is_none_or(|a| self.time >= a.duration),
            _ => false,
        }
    }
}

#[derive(Resource)]
pub struct Hands {
    vm: ViewModel,
    world_bind_inv: Vec<Mat4>,
    cam_joint: usize,
    attach_joint: usize,
    root_joint: usize,
    base: Layer,
    left: Layer,
    /// Joints the left-hand power animations drive (the left arm and fingers).
    left_joints: Vec<bool>,
    arms_mesh: Handle<Mesh>,
    arms_material: Handle<StandardMaterial>,
    tattoo_strength: f32,
    sword_mesh: Option<Handle<Mesh>>,
    prev_state: MotionState,
    prev_blink: BlinkMode,
    landing: f32,
    /// `RightHandWpn` socket offset on `handAttachment_R_jnt`.
    sword_socket: Mat4,
    /// Live animation effects that follow a socket or bone: (effect id, bone, offset).
    attached_fx: Vec<(u64, usize, Mat4)>,
    pub visible: bool,
}

#[derive(Component)]
pub struct ViewmodelLight;

#[derive(Component)]
pub struct ViewmodelCamera;

fn image(t: &upk::texture::Rgba, srgb: bool) -> Image {
    Image::new(
        Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        t.pixels.clone(),
        if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm },
        RenderAssetUsages::default(),
    )
}

fn world_pose(bones: &[upk::skelmesh::Bone], local: impl Fn(usize) -> Mat4) -> Vec<Mat4> {
    let mut w: Vec<Mat4> = Vec::with_capacity(bones.len());
    for (i, b) in bones.iter().enumerate() {
        let l = local(i);
        w.push(if i == 0 || b.parent == i { l } else { w[b.parent] * l });
    }
    w
}

fn static_mesh(part: &MeshPart) -> Mesh {
    let m = &part.mesh;
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; m.vertices.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; m.vertices.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; m.vertices.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
    mesh.insert_indices(Indices::U32(m.indices.clone()));
    mesh
}

pub fn setup_hands(
    mut commands: Commands,
    vm: Option<Res<HandsAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    sim: Res<crate::Sim>,
    world_camera: Query<Entity, With<crate::PlayerCam>>,
) {
    let Some(asset) = vm else { return };
    let Some(vm) = asset.0.lock().unwrap().take() else { return };
    // Composite both HDR layers before tonemapping once, on the foreground camera.
    // Keep the world's tonemapping when arms are unavailable and this camera is not spawned.
    for camera in &world_camera {
        commands.entity(camera).insert(bevy::core_pipeline::tonemapping::Tonemapping::None);
    }
    let asset_socket = vm.sword_socket;
    // The sword mesh's own RotOrigin/MeshOrigin apply before the socket.
    let sword_origin = vm.sword.as_ref().map_or(Mat4::IDENTITY, |s| {
        rotator(s.mesh.rot_origin) * Mat4::from_translation(-GVec3::from(s.mesh.mesh_origin))
    });
    let bones = &vm.arms.mesh.bones;
    let find = |n: &str| bones.iter().position(|b| b.name.eq_ignore_ascii_case(n)).unwrap_or(0);
    let bind = world_pose(bones, |i| {
        let b = &bones[i];
        Mat4::from_rotation_translation(Quat::from_array(b.orientation), GVec3::from(b.position))
    });
    let world_bind_inv = bind.iter().map(|m| m.inverse()).collect();
    let mut left_joints = vec![false; bones.len()];
    if let Some(a) = vm.anims.get("Powers_Idle") {
        for h in &a.joint_hashes {
            if let Some(j) = vm.skeleton.joint_by_hash(*h) {
                left_joints[j] = bones[j].name != "root0_jnt";
            }
        }
    }

    let material = |part: &MeshPart, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>| {
        materials.add(StandardMaterial {
            base_color_texture: part.diffuse.as_ref().map(|t| images.add(image(t, true))),
            normal_map_texture: part.normal.as_ref().map(|t| images.add(image(t, false))),
            emissive_texture: part.tattoo_emissive.as_ref().map(|t| images.add(image(t, false))),
            emissive: if part.tattoo_emissive.is_some() { LinearRgba::new(part.power_color[0] * 0.2, part.power_color[1] * 0.2, part.power_color[2] * 0.2, 1.0) } else { LinearRgba::BLACK },
            perceptual_roughness: 0.75,
            reflectance: 0.25,
            cull_mode: None,
            double_sided: true,
            ..default()
        })
    };
    let arms_mesh = meshes.add(static_mesh(&vm.arms));
    let arms_mat = material(&vm.arms, &mut materials, &mut images);
    let layer = RenderLayers::layer(LAYER);
    commands.spawn((Mesh3d(arms_mesh.clone()), MeshMaterial3d(arms_mat.clone()), Transform::IDENTITY, NoFrustumCulling, layer.clone()));
    let sword_mesh = vm.sword.as_ref().map(|s| {
        let h = meshes.add(static_mesh(s));
        let mat = material(s, &mut materials, &mut images);
        commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(mat), Transform::IDENTITY, NoFrustumCulling, layer.clone()));
        h
    });
    commands.spawn((
        Camera3d::default(),
        // Both cameras must share the HDR target for load-preserving compositing.
        Camera { order: 1, hdr: true, clear_color: ClearColorConfig::None, ..default() },
        bevy::core_pipeline::prepass::DepthPrepass,
        bevy::core_pipeline::tonemapping::Tonemapping::ReinhardLuminance,
        Projection::from(PerspectiveProjection { fov: sim.tuning.fov_deg.to_radians(), near: 0.01, ..default() }),
        crate::color_grading(),
        Transform::IDENTITY,
        layer.clone(),
        ViewmodelCamera,
    ));
    commands.spawn((DirectionalLight { illuminance: 3500.0, shadows_enabled: false, ..default() }, Transform::IDENTITY, layer, ViewmodelLight));

    commands.insert_resource(Hands {
        cam_joint: find("camera_jnt"),
        attach_joint: find("handAttachment_R_jnt"),
        root_joint: find("root0_jnt"),
        world_bind_inv,
        base: Layer::default(),
        left: Layer::default(),
        left_joints,
        arms_mesh,
        arms_material: arms_mat,
        tattoo_strength: 0.2,
        sword_mesh,
        prev_state: MotionState::Walking,
        prev_blink: BlinkMode::Idle,
        landing: 0.0,
        sword_socket: rotator(asset_socket.unwrap_or([0, 16384, 0])) * sword_origin,
        attached_fx: Vec::new(),
        visible: true,
        vm,
    });
}

/// UE3 rotator (pitch, yaw, roll in 65536ths of a turn) as a matrix in Unreal axes: roll about
/// X, then pitch about Y, then yaw about Z; pitch and roll are negated by the left-handed axes
/// (checked against the sword's attach: its blade then points forward, as in the game).
fn rotator(r: [i32; 3]) -> Mat4 {
    let a = |v: i32| v as f32 / 65536.0 * std::f32::consts::TAU;
    Mat4::from_quat(Quat::from_rotation_z(a(r[1])) * Quat::from_rotation_y(-a(r[0])) * Quat::from_rotation_x(-a(r[2])))
}

/// The loaded view model, handed from `main` to the setup system.
#[derive(Resource)]
pub struct HandsAsset(pub std::sync::Mutex<Option<ViewModel>>);

fn sample(vm: &ViewModel, track: &Track, time: f32) -> Option<Vec<Joint>> {
    let a = vm.anims.get(&track.name)?;
    let t = if track.looping && a.duration > 0.0 { time.rem_euclid(a.duration) } else { time.min(a.duration) };
    a.evaluate(&vm.skeleton, t, false).ok()
}

fn blend(a: &mut [Joint], b: &[Joint], w: f32, mask: Option<&[bool]>) {
    for (i, (ja, jb)) in a.iter_mut().zip(b).enumerate() {
        if mask.is_some_and(|m| !m[i]) {
            continue;
        }
        let rb = if ja.rotation.dot(jb.rotation) < 0.0 { -jb.rotation } else { jb.rotation };
        ja.rotation = ja.rotation.slerp(rb, w);
        ja.translation = ja.translation.lerp(jb.translation, w);
    }
}

fn layer_pose(vm: &ViewModel, l: &Layer) -> Option<Vec<Joint>> {
    let cur = l.current.as_ref()?;
    let mut pose = sample(vm, cur, l.time)?;
    if let Some((prev, t)) = &l.previous {
        if let Some(p) = sample(vm, prev, *t) {
            let w = (l.fade / BLEND).clamp(0.0, 1.0);
            let mut out = p;
            blend(&mut out, &pose, 1.0 - w, None);
            pose = out;
        }
    }
    Some(pose)
}

/// Picks animations from the motion state, poses the skeleton and skins the meshes.
pub fn update_hands(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<crate::Sim>,
    hands: Option<ResMut<Hands>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut lights: Query<&mut Transform, With<ViewmodelLight>>,
    mut cams: Query<&mut Projection, With<ViewmodelCamera>>,
    mut fx: ResMut<crate::fx::FxWorld>,
) {
    let Some(mut h) = hands else { return };
    if keys.just_pressed(KeyCode::KeyH) {
        h.visible = !h.visible;
    }
    let dt = time.delta_secs().min(0.1);
    let m = &sim.motion;
    let target_glow = match m.blink.mode {
        BlinkMode::Targeting => 0.65,
        BlinkMode::Travelling => 1.0,
        BlinkMode::Cooldown => 0.2 + 0.45 * (1.0 - m.blink.fx.cooldown_time / sim.tuning.blink.levels[m.blink.level].cooldown_time.max(0.001)).clamp(0.0, 1.0),
        _ => 0.2,
    };
    h.tattoo_strength += (target_glow - h.tattoo_strength) * (1.0 - (-dt * 14.0).exp());
    if let Some(material) = materials.get_mut(&h.arms_material).filter(|_| h.vm.arms.tattoo_emissive.is_some()) {
        let c = h.vm.arms.power_color;
        material.emissive = LinearRgba::new(c[0] * h.tattoo_strength, c[1] * h.tattoo_strength, c[2] * h.tattoo_strength, 1.0);
    }

    // --- choose the base (sword hand + body) and left-hand (power) animations ---
    let speed = m.speed_2d();
    let moving = speed > 40.0;
    if m.state == MotionState::Walking && matches!(h.prev_state, MotionState::Falling | MotionState::Blinking) {
        h.landing = 0.5;
    }
    h.landing = (h.landing - dt).max(0.0);
    let (base, looping) = match m.state {
        MotionState::Mantling => (match m.last_mantle {
            Some(dis_motion::MantleKind::Low) => "Sword_Ready_MantleLow",
            Some(dis_motion::MantleKind::Medium) => "Sword_Ready_MantleMedium",
            _ => "Sword_Ready_MantleHigh",
        }, false),
        MotionState::Sliding => ("Sword_SlideLoop", true),
        MotionState::Swimming => (if moving { "Empty_SwimN" } else { "Empty_SwimIdle" }, true),
        MotionState::Falling | MotionState::Ladder => ("Sword_Ready_Jump", false),
        MotionState::Blinking => ("Sword_Ready_Idle", true),
        MotionState::Walking if h.landing > 0.0 => ("Sword_Ready_JumpLandSmall", false),
        MotionState::Walking => match (m.crouched, moving, m.sprinting, speed < 250.0) {
            (true, false, _, _) => ("Sword_Sneak_Idle", true),
            (true, true, _, _) => ("Sword_Sneak_Walk", true),
            (false, false, _, _) => ("Sword_Ready_Idle", true),
            (false, true, true, _) => ("Sword_Ready_Sprint", true),
            (false, true, false, true) => ("Sword_Ready_Walk", true),
            (false, true, false, false) => ("Sword_Ready_Run", true),
        },
    };
    h.base.play(base, looping);

    let blink = m.blink.mode;
    let left_name = match blink {
        BlinkMode::Targeting => {
            if h.prev_blink != BlinkMode::Targeting {
                h.left.play("Powers_Cast_Blink_In", false);
            }
            if h.left.done(&h.vm) {
                Some(("Powers_Cast_Blink_Loop", true))
            } else {
                None
            }
        }
        BlinkMode::Travelling => Some(("Generic_Powers_Cast_Blink_Travel", false)).filter(|_| h.vm.anims.contains_key("Generic_Powers_Cast_Blink_Travel")).or(Some(("Powers_Cast_Blink_Out", false))),
        BlinkMode::Cooldown | BlinkMode::AbortCooldown if !h.left.done(&h.vm) => None,
        _ => Some(match m.state {
            MotionState::Falling => ("Powers_Jump", false),
            MotionState::Walking if m.sprinting && moving => ("Powers_Sprint", true),
            MotionState::Walking if moving => ("Powers_Walk", true),
            _ => ("Powers_Idle", true),
        }),
    };
    if blink == BlinkMode::Cooldown && h.prev_blink == BlinkMode::Travelling {
        h.left.play("Powers_Cast_Blink_Out", false);
    } else if let Some((n, l)) = left_name {
        h.left.play(n, l);
    }
    h.prev_blink = blink;
    h.prev_state = m.state;
    let mut fired = {
        let h = &mut *h;
        let mut f = h.base.advance(dt, &h.vm);
        f.extend(h.left.advance(dt, &h.vm));
        f
    };

    // --- pose: base layer everywhere, power layer on the left arm (not while the body is busy) ---
    let Some(mut pose) = layer_pose(&h.vm, &h.base) else { return };
    let left_on = !matches!(m.state, MotionState::Mantling | MotionState::Sliding | MotionState::Swimming);
    if !left_on {
        fired.retain(|n| n.socket.as_deref() != Some("Tattoo"));
    }
    if left_on {
        if let Some(lp) = layer_pose(&h.vm, &h.left) {
            blend(&mut pose, &lp, 1.0, Some(&h.left_joints));
        }
    }
    let bones = &h.vm.arms.mesh.bones;
    let world = world_pose(bones, |i| Mat4::from_rotation_translation(pose[i].rotation, pose[i].translation));
    // camera_jnt space (+X up, +Y right, -Z forward) -> Bevy view (x right, y up, z back), in metres.
    let to_view = Mat4::from_cols(GVec3::new(0.0, SCALE, 0.0).extend(0.0), GVec3::new(SCALE, 0.0, 0.0).extend(0.0), GVec3::new(0.0, 0.0, SCALE).extend(0.0), glam::Vec4::W);
    let view = to_view * world[h.cam_joint].inverse();
    let _ = h.root_joint;

    // --- animation particle effects, simulated in the viewmodel frame (see fx::Attach) ---
    // camera_jnt (+X up, +Y right, +Z back) -> effect frame (X right, Y back, Z up).
    let cam_to_fx = Mat4::from_cols(glam::Vec4::Z, glam::Vec4::X, glam::Vec4::Y, glam::Vec4::W) * world[h.cam_joint].inverse();
    let as_affine = |m: Mat4| glam::Affine3A::from_mat4(m);
    let mut follow = Vec::new();
    if h.visible {
        for n in fired {
            let (bone, offset) = match n.socket.as_ref().and_then(|s| h.vm.sockets.get(s)) {
                Some((b, loc, rot)) => (b.clone(), Mat4::from_translation(GVec3::from(*loc)) * rotator(*rot)),
                None => (n.bone.clone().unwrap_or_default(), Mat4::IDENTITY),
            };
            let Some(bi) = bones.iter().position(|b| b.name.eq_ignore_ascii_case(&bone)) else { continue };
            let id = fx.spawn_def(n.system.clone(), as_affine(cam_to_fx * world[bi] * offset), crate::fx::Attach::Viewmodel);
            if n.attached && id != 0 {
                follow.push((id, bi, offset));
            }
        }
    }
    h.attached_fx.extend(follow);
    h.attached_fx.retain(|(id, ..)| fx.is_live(*id));
    for (id, bi, offset) in &h.attached_fx {
        fx.set_transform(*id, as_affine(cam_to_fx * world[*bi] * *offset));
    }

    let skin = |part: &MeshPart, mats: &[Mat4], rigid: Option<Mat4>, mesh: &mut Mesh| {
        let mut pos = Vec::with_capacity(part.mesh.vertices.len());
        let mut nrm = Vec::with_capacity(part.mesh.vertices.len());
        let mut tan = Vec::with_capacity(part.mesh.vertices.len());
        for v in &part.mesh.vertices {
            let m = match rigid {
                Some(r) => r,
                None => {
                    let mut acc = Mat4::ZERO;
                    for k in 0..4 {
                        if v.weights[k] > 0.0 {
                            acc += mats[v.bones[k] as usize] * v.weights[k];
                        }
                    }
                    acc
                }
            };
            let p = m.transform_point3(GVec3::from(v.position));
            let n = m.transform_vector3(GVec3::from(v.normal)).normalize_or_zero();
            let t = m.transform_vector3(GVec3::new(v.tangent[0], v.tangent[1], v.tangent[2])).normalize_or_zero();
            pos.push(p.to_array());
            nrm.push(n.to_array());
            // The view mapping mirrors handedness, so the binormal sign flips.
            tan.push([t.x, t.y, t.z, -v.tangent[3]]);
        }
        if std::env::var_os("SINHONOR_DEBUG_HANDS").is_some() {
            let (mut lo, mut hi) = (GVec3::splat(f32::MAX), GVec3::splat(f32::MIN));
            for p in &pos {
                lo = lo.min(GVec3::from(*p));
                hi = hi.max(GVec3::from(*p));
            }
            eprintln!("[hands] {} verts view bounds {lo:?} .. {hi:?}", pos.len());
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);
    };
    let hidden = !h.visible;
    let skin_mats: Vec<Mat4> = world.iter().zip(&h.world_bind_inv).map(|(w, ib)| if hidden { Mat4::ZERO } else { view * *w * *ib }).collect();
    if let Some(mesh) = meshes.get_mut(&h.arms_mesh) {
        skin(&h.vm.arms, &skin_mats, None, mesh);
    }
    if let (Some(sword), Some(handle)) = (h.vm.sword.as_ref(), h.sword_mesh.as_ref()) {
        if let Some(mesh) = meshes.get_mut(handle) {
            // `Empty_*` animations are the unarmed set (swimming): the sword is put away.
            let unarmed = h.base.current.as_ref().is_some_and(|t| t.name.starts_with("Empty_"));
            let attach = if hidden || unarmed { Mat4::ZERO } else { view * world[h.attach_joint] * h.sword_socket };
            skin(sword, &[], Some(attach), mesh);
        }
    }

    // The viewmodel light follows the world sun as seen from the camera.
    if let Ok(mut lt) = lights.single_mut() {
        let sun_dir_world = (Vec3::ZERO - Vec3::new(30.0, 60.0, 20.0)).normalize();
        let cam_rot = crate::camera_rotation(m);
        let local = cam_rot.inverse() * sun_dir_world;
        *lt = Transform::IDENTITY.looking_to(local, Vec3::Y);
    }
    if let Ok(mut p) = cams.single_mut() {
        if let Projection::Perspective(pp) = &mut *p {
            pp.fov = crate::vertical_fov(m.camera.fov_deg, pp.aspect_ratio);
        }
    }
}
