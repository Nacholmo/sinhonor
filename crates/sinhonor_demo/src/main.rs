//! sinhonor demo: Dishonored motion and Blink on a blockout course, with every tuning value
//! read at startup from your own Dishonored install.
//!
//! cargo run --release -p sinhonor_demo -- [--game <Dishonored dir>] [--difficulty easy|normal|hard|veryhard]

mod level;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorGrabMode, PrimaryWindow};
use dis_data::sounds::{Cue, Gait, Sounds, Surface};
use dis_motion::{BlinkEvent, BlinkMode, Input as MotionInput, MantleKind, Motion, MotionState, MotionTuning, StepEvents};
use std::collections::HashMap;
use level::{Kind, Level};
use std::path::PathBuf;

/// Unreal units (cm) to Bevy metres.
const SCALE: f32 = 0.01;
const MOUSE_SENS: f32 = 0.0022;

/// Unreal (X fwd, Y right, Z up; left-handed) to Bevy (Y up; right-handed): swap Y and Z.
fn to_bevy(v: dis_motion::Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y) * SCALE
}

#[derive(Resource)]
struct Sim {
    motion: Motion,
    level: Level,
    tuning: MotionTuning,
    source: String,
    warnings: usize,
    log: Vec<String>,
    look: Vec2,
    show_help: bool,
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
}

fn step(name: &'static str, secs: f32, input: MotionInput) -> Step {
    Step { name, secs, input, view: None, shot: false }
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
        step("flush", 0.5, idle),
    ]
}

#[derive(Component)]
struct Hud;

#[derive(Component)]
struct BlinkOverlay;

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
                autopilot.steps = autopilot_script();
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
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "sinhonor — Dishonored motion kit".into(), ..default() }),
            ..default()
        }))
        .insert_resource(sim)
        .insert_resource(autopilot)
        .insert_resource(sfx)
        .insert_resource(ClearColor(Color::srgb(0.55, 0.6, 0.66)))
        .insert_resource(AmbientLight { color: Color::WHITE, brightness: 400.0, ..default() })
        .add_systems(Startup, (setup_scene, load_audio, grab_cursor.run_if(|a: Res<Autopilot>| a.steps.is_empty())))
        .add_systems(Update, (cursor_toggle, gather_look, simulate, play_sounds, draw_blink, update_camera, update_hud).chain())
        .run();
}

fn color_for(kind: Kind) -> Color {
    match kind {
        Kind::Ground => Color::srgb(0.32, 0.33, 0.30),
        Kind::Wall => Color::srgb(0.45, 0.40, 0.36),
        Kind::Ledge => Color::srgb(0.62, 0.52, 0.38),
        Kind::Stairs => Color::srgb(0.55, 0.55, 0.50),
        Kind::Roof => Color::srgb(0.38, 0.30, 0.28),
        Kind::Guard => Color::srgb(0.65, 0.18, 0.15),
        Kind::Ladder => Color::srgb(0.75, 0.62, 0.20),
        Kind::Water => Color::srgba(0.15, 0.32, 0.45, 0.55),
    }
}

fn setup_scene(mut commands: Commands, sim: Res<Sim>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for p in &sim.level.pieces {
        let (a, b) = (to_bevy(p.min), to_bevy(p.max));
        let size = (b - a).abs();
        let center = (a + b) * 0.5;
        let transparent = p.kind == Kind::Water;
        let material = materials.add(StandardMaterial {
            base_color: color_for(p.kind),
            perceptual_roughness: 0.9,
            alpha_mode: if transparent { AlphaMode::Blend } else { AlphaMode::Opaque },
            ..default()
        });
        commands.spawn((Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))), MeshMaterial3d(material), Transform::from_translation(center)));
    }
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Projection::from(PerspectiveProjection { fov: sim.tuning.fov_deg.to_radians(), near: 0.05, ..default() }),
        DistanceFog { color: Color::srgb(0.55, 0.6, 0.66), falloff: FogFalloff::Linear { start: 40.0, end: 160.0 }, ..default() },
        Transform::default(),
        PlayerCam,
    ));
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
        BackgroundColor(Color::NONE),
        BlinkOverlay,
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
                "[autopilot] {:<18} state {:?} crouched {} pos ({:.0}, {:.0}, feet {:.0}) speed {:.0} blink {:?}",
                st.name, m.state, m.crouched, m.pos.x, m.pos.y, m.feet().z, m.speed_2d(), m.blink.mode
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

fn push_log(sim: &mut Sim, s: String) {
    sim.log.push(s);
    if sim.log.len() > 6 {
        sim.log.remove(0);
    }
}

fn draw_blink(sim: Res<Sim>, mut gizmos: Gizmos) {
    let b = &sim.motion.blink;
    if b.mode != BlinkMode::Targeting {
        return;
    }
    let Some(t) = b.target else { return };
    let half = sim.motion.half();
    let feet = t.point - dis_motion::Vec3::Z * half.z;
    let color = if t.stop_at_pawn { Color::srgb(1.0, 0.3, 0.2) } else { Color::srgb(0.3, 0.9, 1.0) };
    gizmos.sphere(Isometry3d::from_translation(to_bevy(t.point)), 0.12, color);
    gizmos.circle(Isometry3d::new(to_bevy(feet), Quat::from_rotation_arc(Vec3::Z, Vec3::Y)), half.x * SCALE, color);
    gizmos.line(to_bevy(feet), to_bevy(t.ground_point), color.with_alpha(0.5));
    gizmos.circle(Isometry3d::new(to_bevy(t.ground_point) + Vec3::Y * 0.01, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)), 0.25, color.with_alpha(0.6));
}

fn update_camera(sim: Res<Sim>, mut cams: Query<(&mut Transform, &mut Projection), With<PlayerCam>>, mut overlay: Query<&mut BackgroundColor, With<BlinkOverlay>>) {
    let Ok((mut tf, mut proj)) = cams.single_mut() else { return };
    let m = &sim.motion;
    let dir = m.view_dir();
    let eye = to_bevy(m.camera.eye);
    *tf = Transform::from_translation(eye).looking_to(Vec3::new(dir.x, dir.z, dir.y), Vec3::Y);
    tf.rotate_local_z(-m.camera.roll);
    if let Projection::Perspective(p) = &mut *proj {
        p.fov = m.camera.fov_deg.to_radians();
    }
    if let Ok(mut bg) = overlay.single_mut() {
        let fx = m.blink.fx;
        let a = (fx.distortion * 0.18 + fx.blur * 0.12).clamp(0.0, 0.45);
        bg.0 = Color::srgba(0.35, 0.55, 0.9, a);
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
            "\nWASD move  Mouse look  Space jump/mantle  Ctrl/C crouch (sprint+crouch = slide)\nShift sprint  Alt walk  Q/E lean  RMB/F hold Blink, release to go  1/2 Blink tier\nR reset  M mute  F1 help  Esc free cursor / quit\n\nTuning read from {}\n{} warnings; run speed {:.0}, sprint {:.0}, jump {:.0}, gravity {:.0}",
            sim.source, sim.warnings, sim.tuning.run_speed, sim.tuning.sprint_speed, sim.tuning.jump_z, sim.tuning.gravity_z
        ));
    }
    *text = Text::new(s);
}
