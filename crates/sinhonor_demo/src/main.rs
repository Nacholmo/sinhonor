//! sinhonor demo: Dishonored motion and Blink on a blockout course, with every tuning value
//! read at startup from your own Dishonored install.
//!
//! cargo run --release -p sinhonor_demo -- [--game <Dishonored dir>] [--difficulty easy|normal|hard|veryhard]

mod blink_post;
mod fx;
mod hands;
mod level;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::core_pipeline::bloom::Bloom;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, PrimaryWindow};
use dis_data::sounds::{Cue, Gait, Sounds, Surface};
use dis_data::surfaces::Surface as TexSurface;
use dis_motion::{BlinkEvent, BlinkMode, Input as MotionInput, MantleKind, Motion, MotionState, MotionTuning, StepEvents};
use glam::Affine3A as Affine3AId;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};
use level::{Kind, Level};
use std::path::PathBuf;

/// Unreal units (cm) to Bevy metres.
const SCALE: f32 = 0.01;
const MOUSE_SENS: f32 = 0.0022;

/// Unreal (X fwd, Y right, Z up; left-handed) to Bevy (Y up; right-handed): swap Y and Z.
fn to_bevy(v: dis_motion::Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y) * SCALE
}

/// Haze and horizon colour.
const HORIZON: Color = Color::srgb(0.58, 0.62, 0.66);

/// Environment textures from the install, for dressing the course.
#[derive(Resource)]
struct SurfacesRes(dis_data::surfaces::Surfaces);

#[derive(Resource)]
pub struct Sim {
    pub motion: Motion,
    level: Level,
    pub tuning: MotionTuning,
    source: String,
    warnings: usize,
    log: Vec<String>,
    look: Vec2,
    show_help: bool,
    debug_gizmos: bool,
    /// This frame's motion events and movement, for the audio system.
    frame_events: StepEvents,
    frame_move: dis_motion::Vec3,
}

/// Game sounds (decoded from the install) and the state that paces them.
#[derive(Resource)]
struct Sfx {
    sounds: Sounds,
    handles: HashMap<u32, Handle<AudioSource>>,
    rng: u64,
    muted: bool,
    step_accum: f32,
    climb_accum: f32,
    sprint_time: f32,
    breath_timer: f32,
    swim_timer: f32,
    prev_state: MotionState,
    prev_crouched: bool,
    warmup: Vec<Entity>,
    fall_wind: Vec<Entity>,
    /// Footsteps played this frame (surface, feet position) for the effects system.
    steps: Vec<(Surface, dis_motion::Vec3)>,
}

impl Sfx {
    fn rand(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n.max(1) as u64) as usize
    }

    fn play(&mut self, commands: &mut Commands, cue: Cue, volume: f32) -> Vec<Entity> {
        if self.muted {
            return Vec::new();
        }
        let Some(node) = self.sounds.get(cue).cloned() else { return Vec::new() };
        if std::env::var_os("SINHONOR_LOG_SFX").is_some() {
            println!("[sfx] {cue:?}");
        }
        let mut ids = node.choose(&mut |n| self.rand(n));
        ids.dedup();
        ids.into_iter()
            .filter_map(|id| self.handles.get(&id).cloned())
            .map(|h| commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)))).id())
            .collect()
    }
}

/// Scripted run for verification: `--autopilot <dir>` plays the steps, logs the motion state
/// at each checkpoint, saves screenshots into `<dir>` and exits.
#[derive(Resource, Default)]
struct Autopilot {
    steps: Vec<Step>,
    idx: usize,
    t: f32,
    out: Option<PathBuf>,
}

#[derive(Clone)]
struct Step {
    name: &'static str,
    secs: f32,
    input: MotionInput,
    view: Option<(f32, f32)>,
    shot: bool,
    /// Place the player's feet here first (Unreal units).
    teleport: Option<[f32; 3]>,
}

fn step(name: &'static str, secs: f32, input: MotionInput) -> Step {
    Step { name, secs, input, view: None, shot: false, teleport: None }
}

/// Ground and water effects: gravel and puddle footsteps, pool splash, swimming, climbing out.
fn autopilot_fx_script() -> Vec<Step> {
    let fwd = MotionInput { move_axis: Vec2::new(0.0, 1.0), ..default() };
    let idle = MotionInput::default();
    vec![
        Step { teleport: Some([-1500.0, 0.0, 0.0]), view: Some((0.0, -0.2)), ..step("to marker spot", 0.5, idle) },
        Step { shot: true, ..step("blink marker on floor", 1.2, MotionInput { blink: true, ..idle }) },
        Step { view: Some((0.0, -0.1)), shot: true, ..step("blink marker further", 0.6, MotionInput { blink: true, ..idle }) },
        Step { shot: true, ..step("blink travel", 0.06, idle) },
        step("release", 1.4, idle),
        Step { teleport: Some([200.0, -800.0, 0.0]), view: Some((-FRAC_PI_2, -0.6)), ..step("to gravel", 0.5, idle) },
        step("walk onto gravel", 1.1, fwd),
        Step { shot: true, ..step("gravel steps", 0.25, fwd) },
        Step { teleport: Some([200.0, 800.0, 0.0]), view: Some((FRAC_PI_2, -0.6)), ..step("to puddle", 0.5, idle) },
        step("walk into puddle", 0.9, fwd),
        Step { shot: true, ..step("puddle steps", 0.25, fwd) },
        Step { teleport: Some([-2000.0, -1250.0, 0.0]), view: Some((-FRAC_PI_2, -0.5)), ..step("to pool edge", 0.5, idle) },
        step("walk off into the pool", 0.45, fwd),
        Step { shot: true, ..step("splash", 0.35, idle) },
        step("settle", 1.5, idle),
        Step { view: Some((-FRAC_PI_2, -0.3)), shot: true, ..step("swimming", 1.2, fwd) },
        Step { view: Some((FRAC_PI_2, 0.0)), ..step("turn to edge", 0.1, idle) },
        step("swim to edge", 2.5, fwd),
        step("climb out", 1.2, MotionInput { jump: true, ..fwd }),
        Step { shot: true, ..step("out of water", 0.3, idle) },
        step("flush", 0.5, idle),
    ]
}

fn autopilot_script() -> Vec<Step> {
    let fwd = MotionInput { move_axis: Vec2::new(0.0, 1.0), ..default() };
    let idle = MotionInput::default();
    vec![
        Step { shot: true, ..step("start", 0.6, idle) },
        step("walk to ledge", 1.9, fwd),
        step("jump -> mantle", 0.1, MotionInput { jump: true, ..fwd }),
        Step { shot: true, ..step("mantled", 1.0, idle) },
        Step { view: Some((0.0, -0.08)), ..step("aim blink", 0.1, idle) },
        Step { shot: true, ..step("blink targeting", 0.5, MotionInput { blink: true, ..idle }) },
        step("release blink", 0.6, idle),
        Step { shot: true, ..step("after blink", 0.1, idle) },
        step("walk to ladder + climb", 4.2, fwd),
        Step { shot: true, ..step("on tower roof", 0.6, idle) },
        step("walk to roof edge", 0.9, fwd),
        Step { view: Some((0.0, 0.0)), shot: true, ..step("roof blink targeting", 0.4, MotionInput { blink: true, ..idle }) },
        step("roof blink release", 1.2, idle),
        Step { shot: true, ..step("next roof", 0.2, idle) },
        step("sprint on roof", 0.5, MotionInput { sprint: true, ..fwd }),
        step("slide", 0.05, MotionInput { sprint: true, crouch: true, ..fwd }),
        Step { shot: true, ..step("sliding", 0.3, MotionInput { sprint: true, ..fwd }) },
        step("slide out", 1.0, idle),
        Step { shot: true, ..step("end", 0.3, idle) },
        step("blink cooldown", 1.2, idle),
        Step { view: Some((PI, -0.35)), shot: true, ..step("near blink marker", 1.0, MotionInput { blink: true, ..idle }) },
        Step { shot: true, ..step("blink arrival lens", 0.12, idle) },
        Step { shot: true, ..step("blink arrival lens later", 0.25, idle) },
        step("flush", 0.5, idle),
    ]
}

#[derive(Component)]
struct Hud;

#[derive(Component)]
struct PlayerCam;

fn main() {
    let mut game: Option<PathBuf> = None;
    let mut difficulty = dis_data::Difficulty::Normal;
    let mut autopilot = Autopilot::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().map(PathBuf::from),
            "--difficulty" => {
                difficulty = match args.next().as_deref() {
                    Some("easy") => dis_data::Difficulty::Easy,
                    Some("hard") => dis_data::Difficulty::Hard,
                    Some("veryhard") => dis_data::Difficulty::VeryHard,
                    _ => dis_data::Difficulty::Normal,
                }
            }
            "--autopilot" => {
                autopilot.out = args.next().map(PathBuf::from);
                if autopilot.steps.is_empty() {
                    autopilot.steps = autopilot_script();
                }
            }
            "--route" => {
                // `--route fx` picks the ground/water effects route for --autopilot.
                if args.next().as_deref() == Some("fx") {
                    autopilot.steps = autopilot_fx_script();
                }
            }
            "-h" | "--help" => {
                println!("usage: sinhonor_demo [--game <Dishonored install dir>] [--difficulty easy|normal|hard|veryhard]");
                return;
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
    }
    let Some(install) = dis_data::find_install(game.as_deref()) else {
        eprintln!("Dishonored install not found. Pass --game <path to steamapps/common/Dishonored> or set DISHONORED_DIR.");
        std::process::exit(1);
    };
    let data = match dis_data::load(&install, difficulty) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to read the install: {e}");
            std::process::exit(1);
        }
    };
    for w in &data.warnings {
        eprintln!("warning: {w}");
    }
    let sounds = dis_data::sounds::load_sounds(&install, &data.blink.sound_events);
    for w in &sounds.warnings {
        eprintln!("sound warning: {w}");
    }
    let viewmodel = match dis_data::viewmodel::load_viewmodel(&install) {
        Ok(vm) => {
            for w in &vm.warnings {
                eprintln!("viewmodel warning: {w}");
            }
            Some(vm)
        }
        Err(e) => {
            eprintln!("viewmodel not loaded: {e}");
            None
        }
    };
    let effects = dis_data::effects::load_effects(&install);
    for w in &effects.warnings {
        eprintln!("effect warning: {w}");
    }
    let surfaces = dis_data::surfaces::load_surfaces(&install);
    for w in &surfaces.warnings {
        eprintln!("texture warning: {w}");
    }
    let tuning = MotionTuning::from_game(&data);
    let level = level::build();
    let motion = Motion::new(tuning.clone(), level.spawn, level.spawn_yaw);
    let sim = Sim {
        motion,
        level,
        tuning,
        source: format!("{} ({:?})", install.display(), difficulty),
        warnings: data.warnings.len(),
        log: Vec::new(),
        look: Vec2::ZERO,
        show_help: true,
        debug_gizmos: false,
        frame_events: StepEvents::default(),
        frame_move: dis_motion::Vec3::ZERO,
    };
    let sfx = Sfx {
        sounds,
        handles: HashMap::new(),
        rng: 0x9E37_79B9_7F4A_7C15,
        muted: false,
        step_accum: 0.0,
        climb_accum: 0.0,
        sprint_time: 0.0,
        breath_timer: 0.0,
        swim_timer: 0.0,
        prev_state: MotionState::Falling,
        prev_crouched: false,
        warmup: Vec::new(),
        fall_wind: Vec::new(),
        steps: Vec::new(),
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "sinhonor — Dishonored motion kit".into(),
                resolution: bevy::window::WindowResolution::new(1600.0, 900.0),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((blink_post::BlinkPostPlugin, fx::FxPlugin))
        .insert_resource(sim)
        .insert_resource(autopilot)
        .insert_resource(sfx)
        .insert_resource(fx::FxWorld::new(effects))
        .insert_resource(FxState::default())
        .insert_resource(hands::HandsAsset(std::sync::Mutex::new(viewmodel)))
        .insert_resource(SurfacesRes(surfaces))
        .insert_resource(ClearColor(HORIZON))
        .insert_resource(AmbientLight { color: Color::srgb(0.75, 0.8, 0.9), brightness: 600.0, ..default() })
        .add_systems(Startup, (setup_scene, load_audio, hands::setup_hands, grab_cursor.run_if(|a: Res<Autopilot>| a.steps.is_empty())).chain())
        .add_systems(Update, (cursor_toggle, gather_look, simulate, play_sounds, fx_triggers, update_camera, hands::update_hands, fx::update_fx, draw_blink, update_hud).chain())
        .run();
}

fn color_for(kind: Kind) -> Color {
    match kind {
        Kind::Ground => Color::srgb(0.16, 0.15, 0.13),
        Kind::Wall => Color::srgb(0.30, 0.27, 0.24),
        Kind::Ledge => Color::srgb(0.36, 0.30, 0.23),
        Kind::Stairs => Color::srgb(0.33, 0.32, 0.29),
        Kind::Roof => Color::srgb(0.24, 0.19, 0.18),
        Kind::Guard => Color::srgb(0.65, 0.18, 0.15),
        Kind::Ladder => Color::srgb(0.75, 0.62, 0.20),
        Kind::Water => Color::srgba(0.15, 0.32, 0.45, 0.55),
        Kind::Gravel => Color::srgb(0.24, 0.21, 0.17),
    }
}

/// The game texture a piece is dressed with, its tint and its tile size in metres.
fn surface_for(kind: Kind) -> Option<(TexSurface, Color, f32)> {
    match kind {
        Kind::Ground => Some((TexSurface::Cobbles, Color::srgb(0.85, 0.85, 0.85), 3.0)),
        Kind::Gravel => Some((TexSurface::Cobbles, Color::srgb(0.75, 0.68, 0.58), 1.2)),
        Kind::Wall | Kind::Ledge => Some((TexSurface::Rock, Color::linear_rgb(1.7, 1.65, 1.6), 4.0)),
        Kind::Roof => Some((TexSurface::Rock, Color::linear_rgb(1.4, 1.2, 1.1), 4.0)),
        Kind::Stairs | Kind::Ladder => Some((TexSurface::Planks, Color::WHITE, 1.5)),
        Kind::Guard | Kind::Water => None,
    }
}

/// An RGBA texture with a box-filtered mip chain, repeating, anisotropic.
fn tiled_image(t: &upk::texture::Rgba, srgb: bool, flip_green: bool) -> Image {
    use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let (w0, h0) = (t.width, t.height);
    let mut level = t.pixels.clone();
    if flip_green {
        // Unreal normal maps are Y-down; Bevy's are Y-up.
        level.chunks_exact_mut(4).for_each(|p| p[1] = 255 - p[1]);
    }
    let mut data = level.clone();
    let (mut w, mut h, mut mips) = (w0, h0, 1);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let at = |xx: u32, yy: u32| level[((yy.min(h - 1) * w + xx.min(w - 1)) * 4 + c) as usize] as u32;
                    let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                    next[((y * nw + x) * 4 + c) as usize] = (sum / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (w, h, level, mips) = (nw, nh, next, mips + 1);
    }
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    let mut img = Image::new(Extent3d { width: w0, height: h0, depth_or_array_layers: 1 }, TextureDimension::D2, t.pixels.clone(), format, bevy::asset::RenderAssetUsages::default());
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = mips;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    img
}

/// Axis-aligned box in Bevy space with world-space UVs (`tile` metres per repeat), so textures
/// keep their scale across pieces of any size.
fn box_mesh(min: Vec3, max: Vec3, tile: f32) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    let (c, half) = ((min + max) * 0.5, (max - min).abs() * 0.5);
    let (mut pos, mut nrm, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // (normal, u, v) with u x v = normal, so the corners below wind counter-clockwise.
    for (n, u, v) in [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
    ] {
        let face = c + n * half.dot(n.abs());
        let (hu, hv) = (u * half.dot(u.abs()), v * half.dot(v.abs()));
        let base = pos.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = face + hu * su + hv * sv;
            pos.push(p.to_array());
            nrm.push(n.to_array());
            uv.push([p.dot(u) / tile, -p.dot(v) / tile]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    let _ = mesh.generate_tangents();
    mesh
}

/// The look shared by the world and viewmodel cameras: Dunwall's cool, desaturated grade.
pub fn color_grading() -> bevy::render::view::ColorGrading {
    use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
    ColorGrading {
        // Grade the combined HDR image gently: contrast above one clips dim linear-light
        // values, erasing shadow detail in the arms and around the targeting glow.
        global: ColorGradingGlobal { exposure: -0.1, post_saturation: 0.72, ..default() },
        shadows: ColorGradingSection { saturation: 0.8, lift: 0.01, ..default() },
        midtones: ColorGradingSection { saturation: 0.85, ..default() },
        highlights: ColorGradingSection { saturation: 0.8, gain: 1.05, ..default() },
    }
}

fn setup_scene(
    mut commands: Commands,
    sim: Res<Sim>,
    surfaces: Res<SurfacesRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut textured: HashMap<TexSurface, (Handle<Image>, Option<Handle<Image>>)> = HashMap::new();
    for (s, t) in &surfaces.0.textures {
        let d = images.add(tiled_image(&t.diffuse, true, false));
        let n = t.normal.as_ref().map(|n| images.add(tiled_image(n, false, true)));
        textured.insert(*s, (d, n));
    }
    for p in &sim.level.pieces {
        let (a, b) = (to_bevy(p.min), to_bevy(p.max));
        let (lo, hi) = (a.min(b), a.max(b));
        let surface = surface_for(p.kind).and_then(|(s, tint, tile)| textured.get(&s).map(|t| (t.clone(), tint, tile)));
        let (material, tile) = match surface {
            Some(((d, n), tint, tile)) => (
                StandardMaterial { base_color: tint, base_color_texture: Some(d), normal_map_texture: n, perceptual_roughness: 0.85, reflectance: 0.3, ..default() },
                tile,
            ),
            None if p.kind == Kind::Water => (
                StandardMaterial { base_color: Color::srgba(0.06, 0.10, 0.11, 0.8), perceptual_roughness: 0.08, reflectance: 0.6, alpha_mode: AlphaMode::Blend, ..default() },
                1.0,
            ),
            None => (StandardMaterial { base_color: color_for(p.kind), perceptual_roughness: 0.9, ..default() }, 1.0),
        };
        commands.spawn((Mesh3d(meshes.add(box_mesh(lo, hi, tile))), MeshMaterial3d(materials.add(material)), Transform::IDENTITY));
    }
    // Overcast sky: pale at the horizon (the fog colour), greyer blue overhead.
    let mut sky = Sphere::new(600.0).mesh().uv(48, 24);
    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(p)) = sky.attribute(Mesh::ATTRIBUTE_POSITION) {
        let cols: Vec<[f32; 4]> = p
            .iter()
            .map(|v| {
                let t = (v[1] / 600.0).clamp(0.0, 1.0).powf(0.6);
                let (h, z) = (LinearRgba::from(HORIZON), LinearRgba::from(Color::srgb(0.36, 0.41, 0.48)));
                [h.red + (z.red - h.red) * t, h.green + (z.green - h.green) * t, h.blue + (z.blue - h.blue) * t, 1.0]
            })
            .collect();
        sky.insert_attribute(Mesh::ATTRIBUTE_COLOR, cols);
    }
    commands.spawn((
        Mesh3d(meshes.add(sky)),
        MeshMaterial3d(materials.add(StandardMaterial { unlit: true, fog_enabled: false, cull_mode: None, ..default() })),
        Transform::IDENTITY,
        bevy::pbr::NotShadowCaster,
    ));
    commands.spawn((
        DirectionalLight { illuminance: 4500.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Camera { hdr: true, ..default() },
        bevy::core_pipeline::prepass::DepthPrepass,
        bevy::core_pipeline::tonemapping::Tonemapping::ReinhardLuminance,
        Bloom { intensity: 0.08, low_frequency_boost: 0.25, ..Bloom::OLD_SCHOOL },
        blink_post::BlinkLens::default(),
        Projection::from(PerspectiveProjection { fov: sim.tuning.fov_deg.to_radians(), near: 0.05, ..default() }),
        DistanceFog { color: HORIZON, falloff: FogFalloff::Linear { start: 15.0, end: 110.0 }, ..default() },
        color_grading(),
        Transform::default(),
        PlayerCam,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 15.0, ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), top: Val::Px(10.0), ..default() },
        Hud,
    ));
    // Crosshair.
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), top: Val::Percent(50.0), width: Val::Px(4.0), height: Val::Px(4.0), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
    ));
}

/// Pointer lock where the windowing backend supports it (Wayland, macOS); X11 and Windows only
/// support confining the cursor. Look input uses raw mouse motion either way.
fn grab_mode() -> CursorGrabMode {
    let wayland = cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland || cfg!(target_os = "macos") {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::Confined
    }
}

fn set_grab(window: &mut Window, grab: bool) {
    window.cursor_options.grab_mode = if grab { grab_mode() } else { CursorGrabMode::None };
    window.cursor_options.visible = !grab;
    if grab && grab_mode() == CursorGrabMode::Confined {
        // Keep the (hidden) cursor centred so a confined grab never pins it at an edge.
        let center = Vec2::new(window.width(), window.height()) * 0.5;
        window.set_cursor_position(Some(center));
    }
}

fn grab_cursor(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut w) = windows.single_mut() {
        set_grab(&mut w, true);
    }
}

fn cursor_toggle(keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut exit: EventWriter<AppExit>) {
    let Ok(mut w) = windows.single_mut() else { return };
    if keys.just_pressed(KeyCode::Escape) {
        if w.cursor_options.grab_mode == CursorGrabMode::None {
            exit.write(AppExit::Success);
        } else {
            set_grab(&mut w, false);
        }
    }
    if mouse.just_pressed(MouseButton::Left) && w.cursor_options.grab_mode == CursorGrabMode::None {
        set_grab(&mut w, true);
    }
    // Losing focus (alt-tab) releases the grab; take it back when the window is focused and clicked.
    if !w.focused && w.cursor_options.grab_mode != CursorGrabMode::None {
        set_grab(&mut w, false);
    }
}

fn gather_look(motion: Res<AccumulatedMouseMotion>, windows: Query<&Window, With<PrimaryWindow>>, mut sim: ResMut<Sim>) {
    let grabbed = windows.single().is_ok_and(|w| w.cursor_options.grab_mode != CursorGrabMode::None);
    if grabbed {
        sim.look += Vec2::new(motion.delta.x, -motion.delta.y) * MOUSE_SENS;
    }
}

fn simulate(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut sim: ResMut<Sim>,
    mut auto: ResMut<Autopilot>,
    mut exit: EventWriter<AppExit>,
) {
    let dt = time.delta_secs().min(0.1);
    let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32;
    let mut input = MotionInput {
        move_axis: Vec2::new(axis(KeyCode::KeyD, KeyCode::KeyA), axis(KeyCode::KeyW, KeyCode::KeyS)),
        look: std::mem::take(&mut sim.look),
        jump: keys.pressed(KeyCode::Space),
        crouch: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::KeyC),
        sprint: keys.pressed(KeyCode::ShiftLeft),
        walk: keys.pressed(KeyCode::AltLeft),
        lean: axis(KeyCode::KeyE, KeyCode::KeyQ),
        blink: mouse.pressed(MouseButton::Right) || keys.pressed(KeyCode::KeyF),
    };
    if !auto.steps.is_empty() {
        // Give the window a moment to come up before starting the script.
        if time.elapsed_secs() < 1.5 {
            return;
        }
        if auto.idx >= auto.steps.len() {
            exit.write(AppExit::Success);
            return;
        }
        let st = auto.steps[auto.idx].clone();
        if auto.t == 0.0 {
            if let Some(p) = st.teleport {
                let (tuning, level) = (sim.tuning.clone(), sim.motion.blink.level);
                sim.motion = Motion::new(tuning, dis_motion::Vec3::from(p), 0.0);
                sim.motion.blink.level = level;
            }
            if let Some((yaw, pitch)) = st.view {
                sim.motion.yaw = yaw;
                sim.motion.pitch = pitch;
            }
        }
        input = st.input;
        auto.t += dt;
        if auto.t >= st.secs {
            let m = &sim.motion;
            println!(
                "[autopilot] {:<18} state {:?} crouched {} pos ({:.0}, {:.0}, feet {:.0}) speed {:.0} blink {:?} fx blur {:.2} dist {:.2}",
                st.name, m.state, m.crouched, m.pos.x, m.pos.y, m.feet().z, m.speed_2d(), m.blink.mode, m.blink.fx.blur, m.blink.fx.distortion
            );
            if st.shot {
                if let Some(dir) = &auto.out {
                    let path = dir.join(format!("{:02}_{}.png", auto.idx, st.name.replace(' ', "_")));
                    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
                }
            }
            auto.idx += 1;
            auto.t = 0.0;
        }
    }
    if keys.just_pressed(KeyCode::Digit1) || keys.just_pressed(KeyCode::Digit2) {
        let lvl = if keys.just_pressed(KeyCode::Digit1) { 0 } else { 1 };
        let n = sim.tuning.blink.levels.len();
        sim.motion.blink.level = lvl.min(n - 1);
        let tier = sim.motion.blink.level;
        let reach = sim.tuning.blink.levels[tier].horiz_distance;
        push_log(&mut sim, format!("Blink tier {} (reach {reach:.0})", tier + 1));
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let (spawn, yaw, tuning, level) = (sim.level.spawn, sim.level.spawn_yaw, sim.tuning.clone(), sim.motion.blink.level);
        sim.motion = Motion::new(tuning, spawn, yaw);
        sim.motion.blink.level = level;
        push_log(&mut sim, "reset".into());
    }
    if keys.just_pressed(KeyCode::F1) {
        sim.show_help = !sim.show_help;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        sim.debug_gizmos = !sim.debug_gizmos;
        let on = sim.debug_gizmos;
        push_log(&mut sim, format!("blink debug gizmos {}", if on { "on" } else { "off" }));
    }
    let Sim { motion, level, .. } = &mut *sim;
    let before = motion.pos;
    let ev = motion.update(&level.world, &input, dt);
    sim.frame_move = sim.motion.pos - before;
    sim.frame_events = ev.clone();
    if ev.jumped {
        push_log(&mut sim, "jump".into());
    }
    if let Some(k) = ev.mantled {
        push_log(&mut sim, format!("mantle {k:?}"));
    }
    if ev.slid {
        push_log(&mut sim, "slide".into());
    }
    if let Some(v) = ev.landed.filter(|v| *v > 400.0) {
        push_log(&mut sim, format!("landed at {v:.0} uu/s{}", if ev.fall_damage.is_some() { " (fall damage!)" } else { "" }));
    }
    for b in ev.blink {
        let msg = match b {
            BlinkEvent::Released => "blink!".to_string(),
            BlinkEvent::Fizzled => "blink fizzled".to_string(),
            BlinkEvent::TouchedPawn(id) => format!("blink stopped at guard {id}"),
            _ => continue,
        };
        push_log(&mut sim, msg);
    }
    if sim.motion.pos.z < -2000.0 {
        let (spawn, yaw, tuning) = (sim.level.spawn, sim.level.spawn_yaw, sim.tuning.clone());
        sim.motion = Motion::new(tuning, spawn, yaw);
    }
}

fn load_audio(mut sfx: ResMut<Sfx>, mut sources: ResMut<Assets<AudioSource>>) {
    let clips: Vec<(u32, Vec<u8>)> = sfx.sounds.ogg.iter().map(|(k, v)| (*k, v.clone())).collect();
    for (id, ogg) in clips {
        let h = sources.add(AudioSource { bytes: ogg.into() });
        sfx.handles.insert(id, h);
    }
}

/// Surface under a point, from the course piece it stands on.
fn surface_at(level: &Level, p: dis_motion::Vec3) -> Surface {
    if level.world.water.iter().any(|w| w.contains(p + dis_motion::Vec3::Z * 4.0)) {
        return Surface::Water;
    }
    let probe = p - dis_motion::Vec3::Z * 3.0;
    let piece = level.pieces.iter().filter(|pc| !matches!(pc.kind, Kind::Water | Kind::Ladder)).find(|pc| {
        probe.x >= pc.min.x - 1.0 && probe.x <= pc.max.x + 1.0 && probe.y >= pc.min.y - 1.0 && probe.y <= pc.max.y + 1.0 && probe.z >= pc.min.z - 1.0 && probe.z <= pc.max.z + 1.0
    });
    match piece.map(|pc| pc.kind) {
        Some(Kind::Stairs | Kind::Ledge) => Surface::Wood,
        Some(Kind::Gravel) => Surface::Gravel,
        Some(Kind::Roof) => Surface::Rooftile,
        Some(Kind::Ladder) => Surface::Metal,
        _ => Surface::Stone,
    }
}

fn play_sounds(mut commands: Commands, time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, sim: Res<Sim>, mut sfx: ResMut<Sfx>) {
    if keys.just_pressed(KeyCode::KeyM) {
        sfx.muted = !sfx.muted;
    }
    let dt = time.delta_secs();
    let m = &sim.motion;
    let ev = &sim.frame_events;
    let feet = m.feet();
    let surface = surface_at(&sim.level, feet);
    let gait = if m.crouched { Gait::Sneak } else if m.sprinting { Gait::Sprint } else { Gait::Run };

    // Footsteps by distance walked (not while blinking, mantling or teleporting).
    let moved = sim.frame_move.truncate().length();
    if matches!(m.state, MotionState::Walking) && moved < 100.0 {
        sfx.step_accum += moved;
        let stride = match gait {
            Gait::Sneak => 100.0,
            Gait::Run => 150.0,
            Gait::Sprint => 210.0,
        };
        if sfx.step_accum >= stride {
            sfx.step_accum = 0.0;
            let vol = if gait == Gait::Sneak { 0.35 } else { 0.6 };
            sfx.play(&mut commands, Cue::Footstep(surface, gait), vol);
            sfx.steps.push((surface, feet));
        }
    } else if m.state != MotionState::Walking {
        sfx.step_accum = 75.0; // first step lands soon after touching down
    }
    if m.state == MotionState::Ladder {
        sfx.climb_accum += sim.frame_move.z.abs();
        if sfx.climb_accum >= 45.0 {
            sfx.climb_accum = 0.0;
            sfx.play(&mut commands, Cue::Footstep(Surface::Metal, Gait::Sneak), 0.5);
        }
    }
    if m.state == MotionState::Swimming && moved > 0.5 {
        sfx.swim_timer -= dt;
        if sfx.swim_timer <= 0.0 {
            sfx.swim_timer = 1.1;
            sfx.play(&mut commands, Cue::Swim, 0.6);
        }
    }

    if ev.jumped {
        sfx.play(&mut commands, Cue::Jump, 0.5);
        sfx.play(&mut commands, Cue::Footstep(surface, Gait::Run), 0.5);
    }
    if let Some(impact) = ev.landed {
        let wind: Vec<Entity> = std::mem::take(&mut sfx.fall_wind);
        for e in wind {
            commands.entity(e).try_despawn();
        }
        if impact > 1100.0 {
            sfx.play(&mut commands, Cue::LandHigh(surface), 0.8);
        } else if impact > 250.0 {
            sfx.play(&mut commands, Cue::LandSmall(surface), 0.6);
        }
    }
    if m.state == MotionState::Falling && m.vel.z < -900.0 && sfx.fall_wind.is_empty() {
        let e = sfx.play(&mut commands, Cue::FallWind, 0.5);
        sfx.fall_wind = e;
    }
    if let Some(kind) = ev.mantled {
        let cue = match kind {
            MantleKind::Low => Cue::MantleLow,
            MantleKind::Medium => Cue::MantleMedium,
            MantleKind::High => Cue::MantleHigh,
        };
        sfx.play(&mut commands, cue, 0.7);
    }
    if sfx.prev_state == MotionState::Mantling && m.state == MotionState::Walking {
        sfx.play(&mut commands, Cue::MantleImpact, 0.6);
    }
    if ev.slid {
        sfx.play(&mut commands, Cue::Slide, 0.7);
    }
    if m.state == MotionState::Swimming && sfx.prev_state != MotionState::Swimming {
        sfx.play(&mut commands, Cue::WaterEnter, 0.7);
    }
    let crouch_by_player = matches!(m.state, MotionState::Walking) && matches!(sfx.prev_state, MotionState::Walking);
    if m.crouched != sfx.prev_crouched && crouch_by_player {
        sfx.play(&mut commands, if m.crouched { Cue::Crouch } else { Cue::Stand }, 0.45);
    }

    // Breathing after a long sprint.
    if m.sprinting && m.state == MotionState::Walking {
        sfx.sprint_time += dt;
        sfx.breath_timer -= dt;
        if sfx.sprint_time > 3.0 && sfx.breath_timer <= 0.0 {
            sfx.breath_timer = 4.0;
            sfx.play(&mut commands, Cue::SprintBreath, 0.5);
        }
    } else {
        sfx.sprint_time = 0.0;
    }

    for b in &ev.blink {
        match b {
            BlinkEvent::StartedTargeting => {
                let e = sfx.play(&mut commands, Cue::BlinkWarmup, 0.8);
                sfx.warmup = e;
            }
            BlinkEvent::Released | BlinkEvent::Fizzled => {
                let warm: Vec<Entity> = std::mem::take(&mut sfx.warmup);
                for e in warm {
                    commands.entity(e).try_despawn();
                }
                let cue = if *b == BlinkEvent::Released { Cue::BlinkRelease } else { Cue::BlinkFizzle };
                sfx.play(&mut commands, cue, 0.9);
            }
            _ => {}
        }
    }
    sfx.prev_state = m.state;
    sfx.prev_crouched = m.crouched;
}

/// Which effects are running for ongoing motion states.
#[derive(Resource, Default)]
struct FxState {
    marker: Option<u64>,
    fall_marker: Option<u64>,
    slide: Option<u64>,
    swim_timer: f32,
    prev_state: Option<MotionState>,
    /// Camera shake: remaining time and strength.
    shake: (f32, f32),
}

fn fx_triggers(time: Res<Time>, sim: Res<Sim>, mut sfx: ResMut<Sfx>, mut fxw: ResMut<fx::FxWorld>, mut st: ResMut<FxState>) {
    use dis_data::effects::Fx;
    let m = &sim.motion;
    let ev = &sim.frame_events;
    fxw.camera = fx::camera_basis(m.camera.eye, m.view_dir());
    let feet = m.feet();

    // Blink targeting display, placed as the game places it: the ground effect at the ground
    // point with the pawn's yaw and pitch -90 (its local X points down, so its streaks rise and
    // its cards lie flat), and the fall effect at the target point with no rotation, shown while
    // the target is more than 15 units above the ground point.
    match (m.blink.mode, m.blink.target) {
        (BlinkMode::Targeting, Some(t)) => {
            let ext_z = sim.tuning.blink.target_extent[2];
            let ground = t.ground_point + dis_motion::Vec3::Z * ext_z;
            let ground_xf = fx::pitched_down(ground, m.yaw);
            let fall_xf = Affine3AId::from_translation(t.point);
            match st.marker {
                Some(id) => fxw.set_transform(id, ground_xf),
                None => st.marker = Some(fxw.spawn(Fx::BlinkGround, ground_xf, fx::Attach::World)),
            }
            let show_fall = t.point.z - ground.z > 15.0;
            match (st.fall_marker, show_fall) {
                (Some(id), true) => fxw.set_transform(id, fall_xf),
                (None, true) => st.fall_marker = Some(fxw.spawn(Fx::BlinkFall, fall_xf, fx::Attach::World)),
                (Some(id), false) => {
                    fxw.stop(id);
                    st.fall_marker = None;
                }
                (None, false) => {}
            }
        }
        _ => {
            for id in [st.marker.take(), st.fall_marker.take()].into_iter().flatten() {
                fxw.stop(id);
            }
        }
    }
    if ev.blink.contains(&BlinkEvent::Ended) {
        fxw.spawn(Fx::BlinkArriveLens, Affine3AId::IDENTITY, fx::Attach::Camera);
    }

    // Slide trail at the feet while sliding.
    if m.state == MotionState::Sliding {
        let xf = fx::placed(feet, m.vel.y.atan2(m.vel.x));
        match st.slide {
            Some(id) => fxw.set_transform(id, xf),
            None => {
                let kind = if surface_at(&sim.level, feet) == Surface::Stone { Fx::SlideStone } else { Fx::SlideGeneric };
                st.slide = Some(fxw.spawn(kind, xf, fx::Attach::World));
            }
        }
    } else if let Some(id) = st.slide.take() {
        fxw.stop(id);
    }

    // Footstep puffs on loose and wet ground.
    for (surface, at) in std::mem::take(&mut sfx.steps) {
        let kind = match surface {
            Surface::Gravel => Fx::StepGravel,
            Surface::Water => Fx::StepWater,
            _ => continue,
        };
        fxw.spawn(kind, fx::placed(at, m.yaw), fx::Attach::World);
    }
    if let Some(impact) = ev.landed {
        let surface = surface_at(&sim.level, feet);
        if impact > 900.0 && matches!(surface, Surface::Gravel | Surface::Stone) {
            fxw.spawn(Fx::LandDirt, fx::placed(feet, m.yaw), fx::Attach::World);
        }
        if impact > 900.0 {
            st.shake = (0.35, (impact / sim.tuning.fall_damage_speed).clamp(0.3, 1.5));
        }
    }

    // Water: splash on entry, wake while swimming, droplets on the lens after climbing out.
    let surface_z = sim.level.world.water.iter().find(|w| w.contains(m.pos)).map(|w| w.max.z);
    if m.state == MotionState::Swimming {
        if st.prev_state != Some(MotionState::Swimming) {
            if let Some(z) = surface_z {
                fxw.spawn(Fx::WaterSplash, fx::placed(dis_motion::Vec3::new(m.pos.x, m.pos.y, z), m.yaw), fx::Attach::World);
            }
        }
        st.swim_timer -= time.delta_secs();
        if st.swim_timer <= 0.0 && m.speed_2d() > 30.0 {
            st.swim_timer = 0.45;
            if let Some(z) = surface_z {
                fxw.spawn(Fx::Swimming, fx::placed(dis_motion::Vec3::new(m.pos.x, m.pos.y, z), m.yaw), fx::Attach::World);
            }
        }
    } else if st.prev_state == Some(MotionState::Swimming) {
        fxw.spawn(Fx::CameraWater, Affine3AId::IDENTITY, fx::Attach::Camera);
    }
    st.prev_state = Some(m.state);
    st.shake.0 = (st.shake.0 - time.delta_secs()).max(0.0);
}

/// Debug overlay for Blink targeting (toggle with G): the pawn footprint at the target and a
/// line down to the ground point the targeting found.
fn draw_blink(sim: Res<Sim>, mut gizmos: Gizmos) {
    if !sim.debug_gizmos {
        return;
    }
    let b = &sim.motion.blink;
    if b.mode != BlinkMode::Targeting {
        return;
    }
    let Some(t) = b.target else { return };
    let half = sim.motion.half();
    let feet = t.point - dis_motion::Vec3::Z * half.z;
    let color = if t.stop_at_pawn { Color::srgb(1.0, 0.3, 0.2) } else { Color::srgb(0.3, 0.9, 1.0) };
    gizmos.circle(Isometry3d::new(to_bevy(feet), Quat::from_rotation_arc(Vec3::Z, Vec3::Y)), half.x * SCALE, color);
    gizmos.line(to_bevy(feet), to_bevy(t.ground_point), color.with_alpha(0.6));
    gizmos.sphere(Isometry3d::from_translation(to_bevy(t.point)), 0.08, color);
}

fn push_log(sim: &mut Sim, s: String) {
    sim.log.push(s);
    if sim.log.len() > 6 {
        sim.log.remove(0);
    }
}


/// The game's 75° is used as the vertical angle: measured against in-game screenshots, the
/// first-person hands land where the game draws them only with a vertical 75° (Hor+ at any
/// width), not with a horizontal one.
pub fn vertical_fov(fov_deg: f32, _aspect: f32) -> f32 {
    fov_deg.to_radians()
}

/// The player camera's rotation in Bevy space (view direction plus lean/bob roll).
pub fn camera_rotation(m: &Motion) -> Quat {
    let dir = m.view_dir();
    let mut t = Transform::IDENTITY.looking_to(Vec3::new(dir.x, dir.z, dir.y), Vec3::Y);
    t.rotate_local_z(-m.camera.roll);
    t.rotation
}

fn update_camera(
    time: Res<Time>,
    sim: Res<Sim>,
    st: Res<FxState>,
    mut cams: Query<(&mut Transform, &mut Projection, &mut blink_post::BlinkLens), With<PlayerCam>>,
) {
    let Ok((mut tf, mut proj, mut lens)) = cams.single_mut() else { return };
    let m = &sim.motion;
    let dir = m.view_dir();
    let eye = to_bevy(m.camera.eye);
    *tf = Transform::from_translation(eye).looking_to(Vec3::new(dir.x, dir.z, dir.y), Vec3::Y);
    tf.rotate_local_z(-m.camera.roll);
    let (left, strength) = st.shake;
    if left > 0.0 {
        let k = strength * (left / 0.35).powi(2) * 0.02;
        let t = time.elapsed_secs() * 40.0;
        tf.rotate_local_x(k * t.sin());
        tf.rotate_local_y(k * 0.6 * (t * 1.3).cos());
    }
    if let Projection::Perspective(p) = &mut *proj {
        p.fov = vertical_fov(m.camera.fov_deg, p.aspect_ratio);
    }
    let fx = m.blink.fx;
    lens.blur = fx.blur.max(0.0);
    lens.distortion = fx.distortion.max(0.0);
    lens.time = fx.time_elapsed;
    if let Projection::Perspective(p) = &*proj {
        lens.aspect = p.aspect_ratio;
    }
}

fn update_hud(sim: Res<Sim>, mut hud: Query<&mut Text, With<Hud>>) {
    let Ok(mut text) = hud.single_mut() else { return };
    let m = &sim.motion;
    let state = match m.state {
        MotionState::Walking if m.sprinting => "Sprinting",
        MotionState::Walking if m.crouched => "Crouched",
        MotionState::Walking => "Walking",
        MotionState::Falling => "Falling",
        MotionState::Sliding => "Sliding",
        MotionState::Mantling => "Mantling",
        MotionState::Swimming => "Swimming",
        MotionState::Ladder => "Ladder",
        MotionState::Blinking => "Blinking",
    };
    let lvl = &sim.tuning.blink.levels[m.blink.level.min(sim.tuning.blink.levels.len() - 1)];
    let mut s = format!(
        "{state}   speed {:>4.0} uu/s   vz {:>5.0}   feet z {:>5.0}\nBlink tier {} ({:?})  reach {:.0} x {:.0}\n",
        m.speed_2d(),
        m.vel.z,
        m.feet().z,
        m.blink.level + 1,
        m.blink.mode,
        lvl.horiz_distance,
        lvl.vert_distance,
    );
    for l in &sim.log {
        s.push_str(&format!("> {l}\n"));
    }
    if sim.show_help {
        s.push_str(&format!(
            "\nWASD move  Mouse look  Space jump/mantle  Ctrl/C crouch (sprint+crouch = slide)\nShift sprint  Alt walk  Q/E lean  RMB/F hold Blink, release to go  1/2 Blink tier\nR reset  M mute  G blink gizmos  H hands  F1 help  Esc free cursor / quit\n\nTuning read from {}\n{} warnings; run speed {:.0}, sprint {:.0}, jump {:.0}, gravity {:.0}",
            sim.source, sim.warnings, sim.tuning.run_speed, sim.tuning.sprint_speed, sim.tuning.jump_z, sim.tuning.gravity_z
        ));
    }
    *text = Text::new(s);
}
