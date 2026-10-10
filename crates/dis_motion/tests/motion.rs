//! Behaviour tests on a box world with neutral, made-up tuning (no game data).
use dis_motion::boxworld::BoxWorld;
use dis_motion::*;

/// A made-up mantle clip: rises over the first two thirds, moves forward (if `forward`) after.
fn mantle_clip(exit: f32, forward: bool) -> MantleClip {
    let f = if forward { 30.0 } else { 0.0 };
    MantleClip { exit, root: vec![Vec3::ZERO, Vec3::new(exit / 3.0, 6.0, 0.0), Vec3::new(exit * 2.0 / 3.0, 10.0, f * 0.5), Vec3::new(exit, 10.0, f)] }
}

fn tuning() -> MotionTuning {
    let mantle = MantleTuning {
        line_check_step: 10.0,
        min_edge_height: 50.0,
        max_edge_height: 200.0,
        low_max_edge_height: 100.0,
        medium_max_edge_height: 150.0,
        low_uses_step_up: true,
        low_step_up_blend_time: 0.2,
        max_vertical_angle_edge_face_deg: 20.0,
        max_horizontal_angle_edge_face_deg: 45.0,
        max_slope_angle_edge_top_deg: 45.0,
        edge_search_dist: 50.0,
        forward_move_amount: 20.0,
        fall_speed_for_ledge_grab: 1000.0,
        max_fall_speed_for_mantle: 2000.0,
        ledge_grab_min_edge_height: 150.0,
        anim_rate: 1.0,
    };
    MotionTuning {
        gravity_z: -1000.0,
        radius: 30.0,
        half_height: 80.0,
        crouch_radius: 30.0,
        crouch_half_height: 55.0,
        crawl_half_height: 30.0,
        max_step_height: 30.0,
        base_eye_height: 70.0,
        walkable_floor_z: 0.7,
        terminal_velocity: 2500.0,
        ladder_speed: 200.0,
        run_speed: 400.0,
        sprint_speed: 600.0,
        crouch_speed: 200.0,
        walk_speed: 150.0,
        slow_walk_speed: 80.0,
        sword_speed_factor: 1.0,
        empty_hand_speed_factor: 1.0,
        speed_blend_down: 10.0,
        gait: GaitTuning {
            slow_walk_threshold: 0.4,
            walk_threshold: 0.8,
            stop_sprint_threshold: 0.7,
            strafe_angle_forward_deg: 25.0,
            strafe_angle_back_deg: 50.0,
            backwards_angle_deg: 35.0,
        },
        water_speed: 300.0,
        accel_rate: 2000.0,
        ground_friction: 8.0,
        braking: 8.0,
        fluid_friction: 2.0,
        strafe_mult_run: 0.8,
        strafe_mult_sneak: 0.8,
        strafe_mult_sprint: 0.5,
        backward_mult_run: 0.7,
        backward_mult_sneak: 0.7,
        backward_mult_sprint: 0.5,
        jump_z: 500.0,
        power_jump: PowerJumpTuning::default(),
        air_control: 0.2,
        fall_damage_speed: 1500.0,
        fall_death_speed: 2500.0,
        slide_time: 1.0,
        slide_not_cancelable_pct: 0.5,
        slide_allow_return_to_sprint: false,
        slide_deactivate_angle_deg: 25.0,
        slide_impact_shake: 1.0,
        auto_crouch_test_distance: 100.0,
        min_pitch_deg: -85.0,
        max_pitch_deg: 85.0,
        fov_deg: 75.0,
        fov_blend_speed: 5.0,
        bob_amount: 0.5,
        roll_amount: 0.5,
        lean: LeanTuning {
            lean_speed: 800.0,
            release_time: 0.2,
            min_pitch_deg: -30.0,
            max_pitch_deg: 30.0,
            min_yaw_deg: -50.0,
            max_yaw_deg: 50.0,
            max_angle_deg: 15.0,
            max_angle_crouched_deg: 15.0,
            max_soften_angle_deg: 4.0,
            max_soften_speed: 4.0,
            camera_tilt_pct: 0.5,
            height_pct: 1.5,
            springiness: 80.0,
            damping: 12.0,
            fixed_time_step: 1.0 / 60.0,
        },
        swim: SwimTuning { min_accel: 500.0, max_accel: 2000.0, max_speed_no_stroke: 300.0, stroke_time: 0.3 },
        mantle_blink: mantle.clone(),
        mantle,
        anim: AnimTimes {
            mantle: std::array::from_fn(|i| mantle_clip(0.5 + 0.1 * (i % 3) as f32, i < 3)),
            land_small: 0.5,
            land_big: 1.0,
            slide_in: 0.3,
            slide_out: 0.3,
        },
        blink: BlinkTuning {
            levels: vec![BlinkLevel {
                distance: 1500.0,
                horiz_distance: 1000.0,
                vert_distance: 400.0,
                step_distance: 100.0,
                step_interval: 0.01,
                warmup_time: 0.5,
                cooldown_time: 0.5,
                warmup_wobble_max: 0.2,
                warmup_wobble_per_second: 0.4,
                warmup_distortion_min: 0.2,
                move_distortion_max: 1.0,
                move_blur_max: 1.0,
                move_reach_max_at_pct: 0.8,
                cooldown_wobble_count: 5,
            }],
            target_extent: [20.0; 3],
            close_collision_distance: 300.0,
            close_collision_offset_step: 30.0,
            limit_vertical_from_ground: false,
        },
        drop_assassinate: DropAssassinateTuning {
            hit_window: 3.0,
            min_drop_dist: 0.0,
            max_drop_dist: 300.0,
            max_drop_jump_vel: 0.0,
            min_drop_down_vel: 120.0,
            sides: [
                DropSide { anchor: Vec3::new(55.0, 0.0, 0.0), duration: 2.0, anim: "Drop".into() },
                DropSide { anchor: Vec3::new(0.0, -75.0, 0.0), duration: 2.0, anim: "Drop".into() },
                DropSide { anchor: Vec3::new(0.0, 75.0, 0.0), duration: 2.0, anim: "Drop".into() },
                DropSide { anchor: Vec3::new(-65.0, 0.0, 0.0), duration: 2.0, anim: "Drop".into() },
            ],
        },
        melee: melee_tuning(),
        assassinate: assassinate_tuning(),
    }
}

fn assassinate_tuning() -> AssassinateTuning {
    let kill = |anim: &str, x: f32, y: f32, duration: f32| DropSide { anchor: Vec3::new(x, y, 0.0), duration, anim: anim.into() };
    AssassinateTuning {
        range: 260.0,
        ray_scale_percent: 1.0,
        probe_extent: Vec3::new(15.0, 15.0, 30.0),
        on_awareness: [true, false, true, true, false, false, false, false],
        can_assassinate_runners: false,
        finishers_before_slow: (2, 2),
        time_before_slow: (60.0, 60.0),
        slow: [kill("SlowFront", 110.0, 0.0, 1.5), kill("SlowLeft", 0.0, -110.0, 1.5), kill("SlowRight", 0.0, 110.0, 1.5), kill("SlowBack", -110.0, 0.0, 1.5)],
        fast: [kill("FastFront", 115.0, 0.0, 0.8), kill("FastLeft", 0.0, -115.0, 0.8), kill("FastRight", 0.0, 115.0, 0.8), kill("FastBack", -115.0, 0.0, 0.8)],
        generic: kill("Generic", 0.0, 0.0, 0.3),
    }
}

fn swing_anim(name: &str, zone: Option<(f32, f32)>) -> SwingAnim {
    SwingAnim { name: name.into(), zone, chain_input: zone.map(|_| 0.25), interruptible: 0.6, exit: 0.8 }
}

fn melee_tuning() -> MeleeTuning {
    let set = |name: &str| SwingSet {
        swing: swing_anim(name, Some((0.15, 0.05))),
        env_hit: Some(SwingAnim { exit: 0.5, interruptible: 0.35, ..swing_anim(&format!("{name}_Recoil"), None) }),
        env_hit_chain: None,
    };
    MeleeTuning {
        range: 200.0,
        ray_scale_percent: 0.5,
        ray_speed_scale: 0.004,
        ray_speed_scale_max: 1.2,
        min_speed_ray_scale: 350.0,
        sweep_size: 50.0,
        crosshair_size: 12.0,
        chain_time: 0.9,
        damage: 12.0,
        env_hit_shake: 150.0,
        swings: [vec![set("Forehand")], vec![set("Backhand")], vec![set("Sneak")], vec![set("KillForehand")], vec![set("KillBackhand")]],
    }
}

fn floor_world() -> BoxWorld {
    let mut w = BoxWorld::default();
    w.add_box(Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 0.0));
    w
}

fn run(m: &mut Motion, w: &BoxWorld, input: Input, secs: f32) -> Vec<StepEvents> {
    let dt = 1.0 / 60.0;
    let mut evs = Vec::new();
    let mut t = 0.0;
    while t < secs {
        evs.push(m.update(w, &input, dt));
        t += dt;
    }
    evs
}

fn fwd() -> Input {
    Input { move_axis: Vec2::new(0.0, 1.0), ..Default::default() }
}

#[test]
fn settles_on_floor_and_runs_at_run_speed() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::new(0.0, 0.0, 50.0), 0.0);
    run(&mut m, &w, Input::default(), 1.0);
    assert_eq!(m.state, MotionState::Walking);
    assert!((m.feet().z).abs() < 1.0, "feet at {}", m.feet().z);
    run(&mut m, &w, fwd(), 1.0);
    assert!((m.speed_2d() - 400.0).abs() < 5.0, "speed {}", m.speed_2d());
    let sprint = Input { sprint: true, ..fwd() };
    run(&mut m, &w, sprint, 1.0);
    assert!((m.speed_2d() - 600.0).abs() < 5.0, "sprint {}", m.speed_2d());
    let back = Input { move_axis: Vec2::new(0.0, -1.0), ..Default::default() };
    run(&mut m, &w, back, 1.5);
    assert!((m.speed_2d() - 280.0).abs() < 5.0, "backward {}", m.speed_2d());
}

#[test]
fn jump_apex_matches_ballistics() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let ground = m.feet().z;
    let mut peak: f32 = 0.0;
    let jump = Input { jump: true, ..Default::default() };
    let idle = Input::default();
    let mut landed = false;
    for i in 0..120 {
        let e = m.update(&w, if i == 0 { &jump } else { &idle }, 1.0 / 60.0);
        peak = peak.max(m.feet().z - ground);
        landed |= e.landed.is_some();
    }
    let expected = 500.0f32 * 500.0 / (2.0 * 1000.0);
    assert!((peak - expected).abs() < 8.0, "apex {peak} vs {expected}");
    assert!(landed);
}

/// Jumps from the floor, holding jump for `hold` seconds; returns the highest the feet got and
/// how many power jumps there were.
fn jump_peak(t: MotionTuning, hold: f32) -> (f32, usize) {
    let w = floor_world();
    let mut m = Motion::new(t, Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let ground = m.feet().z;
    let (mut peak, mut power): (f32, usize) = (0.0, 0);
    let dt = 1.0 / 60.0;
    for i in 0..240 {
        let input = Input { jump: (i as f32) * dt < hold, ..Default::default() };
        let e = m.update(&w, &input, dt);
        peak = peak.max(m.feet().z - ground);
        power += usize::from(e.power_jumped);
    }
    (peak, power)
}

#[test]
fn power_jump_kicks_in_at_the_top_of_a_held_jump() {
    let mut t = tuning();
    t.power_jump = PowerJumpTuning { style: JumpStyle::FixedPower, jump_z: 600.0, ..Default::default() };
    let normal = 500.0f32 * 500.0 / 2000.0;
    // Tapped: an ordinary jump.
    let (peak, power) = jump_peak(t.clone(), 0.02);
    assert!((peak - normal).abs() < 8.0 && power == 0, "tap: {peak} {power}");
    // Let go before the top: still ordinary.
    let (peak, power) = jump_peak(t.clone(), 0.3);
    assert!((peak - normal).abs() < 8.0 && power == 0, "early release: {peak} {power}");
    // Held through the top: kicked up again, once.
    let (peak, power) = jump_peak(t.clone(), 3.0);
    let expected = normal + 600.0 * 600.0 / 2000.0;
    assert!((peak - expected).abs() < 10.0 && power == 1, "held: {peak} vs {expected}, {power}");
    // Without the power (Agility not owned) holding changes nothing.
    let (peak, power) = jump_peak(tuning(), 3.0);
    assert!((peak - normal).abs() < 8.0 && power == 0, "no power: {peak} {power}");
}

#[test]
fn held_power_jump_is_cut_short_by_letting_go() {
    let mut t = tuning();
    t.power_jump = PowerJumpTuning { style: JumpStyle::HeldPowerFullStop, full_stop_z: 700.0, extra_stop_vel: 300.0, held_time: 0.1, ..Default::default() };
    let (full, power) = jump_peak(t.clone(), 3.0);
    assert!((full - 700.0 * 700.0 / 2000.0).abs() < 10.0 && power == 1, "held: {full}");
    let (cut, _) = jump_peak(t, 0.05);
    assert!(cut < 80.0, "let go early: {cut}");
}

#[test]
fn continuous_power_jump_pushes_while_held() {
    let mut t = tuning();
    t.power_jump = PowerJumpTuning { style: JumpStyle::ContinuousPower, full_stop_z: 1.0, held_time: 0.2, held_accel: 1500.0, ..Default::default() };
    let normal = 500.0f32 * 500.0 / 2000.0;
    let (tapped, _) = jump_peak(t.clone(), 0.02);
    let (held, power) = jump_peak(t, 3.0);
    assert!(tapped < normal + 30.0, "tapped: {tapped}");
    assert!(held > normal + 60.0 && power == 1, "held: {held} {power}");
}

#[test]
fn steps_up_small_ledges_and_mantles_tall_ones() {
    let mut w = floor_world();
    w.add_box(Vec3::new(200.0, -500.0, 0.0), Vec3::new(400.0, 500.0, 20.0)); // step
    w.add_box(Vec3::new(600.0, -500.0, 0.0), Vec3::new(900.0, 500.0, 140.0)); // ledge
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, fwd(), 1.0);
    assert!((m.feet().z - 20.0).abs() < 2.0, "should be on the step, feet {}", m.feet().z);
    // Walk into the ledge and press jump: mantle up.
    run(&mut m, &w, fwd(), 0.6);
    let jump = Input { jump: true, ..fwd() };
    let mut mantled = None;
    for e in run(&mut m, &w, jump, 0.05) {
        mantled = mantled.or(e.mantled);
    }
    assert!(mantled.is_some(), "expected a mantle, state {:?} pos {:?}", m.state, m.pos);
    run(&mut m, &w, Input::default(), 1.5);
    assert_eq!(m.state, MotionState::Walking);
    assert!((m.feet().z - 140.0).abs() < 2.0, "on top of the ledge, feet {}", m.feet().z);
}

#[test]
fn slide_from_sprint_bleeds_to_crouch_speed() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input { sprint: true, ..fwd() }, 1.0);
    let evs = run(&mut m, &w, Input { sprint: true, crouch: true, ..fwd() }, 0.05);
    assert!(evs.iter().any(|e| e.slid));
    assert_eq!(m.state, MotionState::Sliding);
    assert!(m.crouched);
    run(&mut m, &w, fwd(), 1.2);
    assert_eq!(m.state, MotionState::Walking);
    assert!(m.crouched);
}

#[test]
fn auto_crouch_under_low_gap() {
    let mut w = floor_world();
    w.add_box(Vec3::new(300.0, -500.0, 100.0), Vec3::new(700.0, 500.0, 400.0)); // beam at 100
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, fwd(), 4.5);
    assert!(m.pos.x > 700.0, "got through the gap: x {}", m.pos.x);
}

#[test]
fn blink_travels_to_wall_and_keeps_momentum() {
    let mut w = floor_world();
    w.add_box(Vec3::new(800.0, -500.0, 0.0), Vec3::new(900.0, 500.0, 600.0)); // wall
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let start_x = m.pos.x;
    run(&mut m, &w, Input { blink: true, ..Default::default() }, 0.2);
    let target = m.blink.target.expect("targeting");
    assert!(target.point.x < 800.0 - 29.0 && target.point.x > 600.0, "target in front of the wall: {:?}", target.point);
    let mut ended = false;
    for e in run(&mut m, &w, Input::default(), 0.5) {
        ended |= e.blink.contains(&BlinkEvent::Ended);
    }
    assert!(ended);
    assert!(m.pos.x > start_x + 500.0 && m.pos.x < 800.0, "pos {:?}", m.pos);
}

#[test]
fn blink_while_crouched_stays_on_the_floor() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    run(&mut m, &w, Input { crouch: true, ..Default::default() }, 0.05);
    run(&mut m, &w, Input::default(), 0.2);
    assert!(m.crouched);
    m.pitch = -0.4; // aim at the floor a few metres ahead
    run(&mut m, &w, Input { blink: true, ..Default::default() }, 0.3);
    let evs = run(&mut m, &w, Input::default(), 1.0);
    assert!(evs.iter().any(|e| e.blink.contains(&BlinkEvent::Ended)));
    assert!(m.feet().z.abs() < 1.0, "feet {}", m.feet().z);
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn blink_range_is_squashed_sphere() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    m.pitch = 0.5; // looking up
    run(&mut m, &w, Input { blink: true, ..Default::default() }, 0.1);
    let up = m.blink.target.unwrap().point - m.pos;
    assert!(up.z <= 400.0 + 120.0, "vertical reach capped: {up:?}");
}

#[test]
fn swims_at_surface() {
    let mut w = floor_world();
    w.add_water(Vec3::new(-2000.0, -2000.0, -100.0), Vec3::new(2000.0, 2000.0, 400.0));
    let mut m = Motion::new(tuning(), Vec3::new(0.0, 0.0, 500.0), 0.0);
    run(&mut m, &w, Input::default(), 3.0);
    assert_eq!(m.state, MotionState::Swimming);
    let eye = m.pos.z + 70.0;
    assert!((eye - 400.0).abs() < 30.0, "eyes near surface: {eye}");
}

#[test]
fn climbs_ladder() {
    let mut w = floor_world();
    w.add_box(Vec3::new(300.0, -200.0, 0.0), Vec3::new(400.0, 200.0, 300.0));
    w.add_ladder(Vec3::new(280.0, -50.0, 0.0), Vec3::new(300.0, 50.0, 300.0), Vec3::new(-1.0, 0.0, 0.0));
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    let mut saw_ladder = false;
    let mut on_top = false;
    for _ in 0..200 {
        m.update(&w, &fwd(), 1.0 / 60.0);
        saw_ladder |= m.state == MotionState::Ladder;
        on_top |= m.state == MotionState::Walking && (m.feet().z - 300.0).abs() < 2.0;
    }
    assert!(saw_ladder);
    assert!(on_top, "climbed off onto the top, feet {}", m.feet().z);
}

/// Uses the real tuning when a Dishonored install is available (skipped otherwise).
#[cfg(feature = "dishonored-data")]
#[test]
fn real_install_tuning_drives_motion() {
    let Some(install) = dis_data::find_install(None) else {
        eprintln!("no Dishonored install found; skipping");
        return;
    };
    let data = dis_data::load(&install, Default::default()).expect("load install");
    let t = MotionTuning::from_game(&data);
    assert!(t.run_speed > 0.0 && t.jump_z > 0.0 && !t.blink.levels.is_empty());
    let w = floor_world();
    let mut m = Motion::new(t.clone(), Vec3::ZERO, 0.0);
    run(&mut m, &w, fwd(), 1.0);
    // Corvo runs with his sword out.
    assert!((m.speed_2d() - t.run_speed * t.sword_speed_factor).abs() < 5.0, "speed {}", m.speed_2d());
    run(&mut m, &w, Input::default(), 1.0);
    let x0 = m.pos.x;
    run(&mut m, &w, Input { blink: true, ..Default::default() }, 0.2);
    let evs = run(&mut m, &w, Input::default(), 1.0);
    assert!(evs.iter().any(|e| e.blink.contains(&BlinkEvent::Ended)));
    let travelled = m.pos.x - x0;
    let reach = t.blink.levels[0].horiz_distance;
    assert!(travelled > reach * 0.8 && travelled < reach + 50.0, "travelled {travelled} of {reach}");

    // Agility: no power jump without it; with either level, a held jump goes higher than a tap.
    assert_eq!(t.power_jump.jump_z, 0.0);
    for level in [1, 2] {
        let agile = MotionTuning::from_game(&data.with_power(dis_data::powers::AGILITY, level));
        assert!(agile.power_jump.jump_z > 0.0, "Agility {level}");
        let (tap, _) = jump_peak(agile.clone(), 0.02);
        let (held, power) = jump_peak(agile, 3.0);
        assert!(held > tap * 1.5 && power == 1, "Agility {level}: held {held}, tap {tap}");
    }
}

/// A floor with one 175 cm character at the origin, facing +X.
fn guard_world() -> BoxWorld {
    let mut w = floor_world();
    w.add_character(Vec3::new(0.0, 0.0, 87.5), Vec3::new(31.0, 31.0, 87.5), 7, 0.0, 30.0);
    w
}

fn attack() -> Input {
    Input { attack: true, ..Default::default() }
}

#[test]
fn drop_assassination_in_range_lands_on_the_side_it_came_from() {
    let w = guard_world();
    // Feet 3 m up, just behind the guard: the torso is 1.2 m up, so this is within 3 m.
    let mut m = Motion::new(tuning(), Vec3::new(-20.0, 0.0, 300.0), 0.0);
    run(&mut m, &w, Input::default(), 0.1);
    assert_eq!(m.state, MotionState::Falling);
    assert!(matches!(m.takedown.prompt, Some((7, Reach::InRange))), "prompt {:?}", m.takedown.prompt);
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    assert_eq!(ev.drop_assassination, Some((7, Side::Back)));
    assert_eq!(m.state, MotionState::Takedown);
    // Placed 65 cm behind the guard on its floor, facing it.
    assert!((m.pos.x + 65.0).abs() < 1.0 && m.pos.y.abs() < 1.0, "pos {:?}", m.pos);
    assert!(m.feet().z.abs() < 1.0, "feet {}", m.feet().z);
    assert!(m.yaw.abs() < 0.01 || (m.yaw - std::f32::consts::TAU).abs() < 0.01, "yaw {}", m.yaw);
    // Held for the kill, then free again.
    let look = Input { look: Vec2::new(1.0, 0.0), ..Default::default() };
    run(&mut m, &w, look, 1.0);
    assert_eq!(m.state, MotionState::Takedown);
    assert!(m.yaw.abs() < 0.01 || (m.yaw - std::f32::consts::TAU).abs() < 0.01, "view held, yaw {}", m.yaw);
    run(&mut m, &w, Input::default(), 1.1);
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn drop_assassination_too_high_locks_on_and_dives() {
    let w = guard_world();
    let mut m = Motion::new(tuning(), Vec3::new(10.0, 20.0, 1200.0), 0.0);
    m.vel = Vec3::new(30.0, 0.0, 0.0);
    run(&mut m, &w, Input::default(), 0.1);
    assert!(matches!(m.takedown.prompt, Some((7, Reach::Track))), "prompt {:?}", m.takedown.prompt);
    let vz = m.vel.z;
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    assert_eq!(ev.dove_at, Some(7));
    assert_eq!(m.takedown.diving, Some(7));
    assert_eq!(m.vel.truncate(), Vec2::ZERO);
    assert!(m.vel.z < vz * 1.9, "dive speed {} from {vz}", m.vel.z);
    // Steering is ignored on the way down; the kill starts once in range.
    let mut kill = None;
    for e in run(&mut m, &w, fwd(), 1.5) {
        kill = kill.or(e.drop_assassination);
    }
    // Directly above and slightly front-right: |x| < |y|, so from the right.
    assert_eq!(kill, Some((7, Side::Right)));
    assert_eq!(m.state, MotionState::Takedown);
}

#[test]
fn drop_assassination_needs_a_clear_line_and_a_fall() {
    // A slab between the player and the guard hides it.
    let mut w = guard_world();
    w.add_box(Vec3::new(-200.0, -200.0, 250.0), Vec3::new(200.0, 200.0, 260.0));
    let mut m = Motion::new(tuning(), Vec3::new(0.0, 0.0, 400.0), 0.0);
    run(&mut m, &w, Input::default(), 0.05);
    assert!(m.takedown.prompt.is_none());
    // Rising past someone doesn't count.
    let w = guard_world();
    let mut m = Motion::new(tuning(), Vec3::new(0.0, 0.0, 250.0), 0.0);
    m.vel.z = 600.0;
    m.update(&w, &Input::default(), 1.0 / 60.0);
    assert!(m.takedown.prompt.is_none());
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    assert!(ev.drop_assassination.is_none() && ev.dove_at.is_none());
    // On the ground, attacking a guard that has seen the player is no takedown.
    let mut w = guard_world();
    w.set_awareness(7, Awareness::InCombat);
    let mut m = Motion::new(tuning(), Vec3::new(-100.0, 0.0, 0.0), 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    assert!(ev.drop_assassination.is_none() && ev.assassination.is_none());
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn drop_assassination_side_follows_the_target_facing() {
    let guard = PawnInfo { center: Vec3::new(0.0, 0.0, 90.0), floor_z: 0.0, torso_z: 120.0, yaw: std::f32::consts::FRAC_PI_2, health: 25.0, awareness: Awareness::Unaware, running: false };
    // Facing +Y: +Y is its front, +X its left.
    assert_eq!(side_of(&guard, Vec3::new(0.0, 100.0, 300.0)), Side::Front);
    assert_eq!(side_of(&guard, Vec3::new(0.0, -100.0, 300.0)), Side::Back);
    assert_eq!(side_of(&guard, Vec3::new(100.0, 10.0, 300.0)), Side::Left);
    assert_eq!(side_of(&guard, Vec3::new(-100.0, -10.0, 300.0)), Side::Right);
    let (feet, yaw) = landing(&guard, &DropSide { anchor: Vec3::new(0.0, -75.0, 0.0), duration: 1.0, anim: "Drop".into() });
    assert!((feet - Vec3::new(75.0, 0.0, 0.0)).length() < 0.01, "left anchor {feet:?}");
    assert!(yaw.cos() < -0.999, "faces the guard, yaw {yaw}");
}

#[test]
fn drop_assassination_sweeps_the_players_box() {
    // Not quite over the guard (its edge is at x = 31), but the player's box overlaps it.
    let w = guard_world();
    let mut m = Motion::new(tuning(), Vec3::new(55.0, 0.0, 300.0), 0.0);
    run(&mut m, &w, Input::default(), 0.05);
    assert!(matches!(m.takedown.prompt, Some((7, Reach::InRange))), "prompt {:?}", m.takedown.prompt);
    // Clear of it, nothing.
    let mut m = Motion::new(tuning(), Vec3::new(120.0, 0.0, 300.0), 0.0);
    run(&mut m, &w, Input::default(), 0.05);
    assert!(m.takedown.prompt.is_none());
}

fn swings(evs: &[StepEvents]) -> Vec<MeleeEvent> {
    evs.iter().flat_map(|e| e.melee.clone()).collect()
}

/// A guard that has seen the player (so attacks are sword blows, not assassinations).
fn alert_guard_world() -> BoxWorld {
    let mut w = guard_world();
    w.set_awareness(7, Awareness::InCombat);
    w
}

/// Standing 1.5 m from the guard at the origin, facing it.
fn facing_guard(w: &BoxWorld) -> Motion {
    let mut m = Motion::new(tuning(), Vec3::new(-150.0, 0.0, 0.0), 0.0);
    run(&mut m, w, Input::default(), 0.3);
    m
}

#[test]
fn sword_swings_alternate_and_chain() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let kinds = |evs: &[StepEvents]| swings(evs).into_iter().filter_map(|e| if let MeleeEvent::Swing { kind, .. } = e { Some(kind) } else { None }).collect::<Vec<_>>();
    // Press, then press again during the chain window: forehand, then backhand once it may start.
    let mut evs = run(&mut m, &w, attack(), 0.05);
    evs.extend(run(&mut m, &w, Input::default(), 0.25));
    evs.extend(run(&mut m, &w, attack(), 0.05));
    evs.extend(run(&mut m, &w, Input::default(), 0.5));
    assert_eq!(kinds(&evs), vec![SwingKind::Forehand, SwingKind::Backhand]);
    // A third, in the backhand's chain window: forehand again.
    run(&mut m, &w, Input::default(), 0.1);
    let evs = [run(&mut m, &w, attack(), 0.05), run(&mut m, &w, Input::default(), 1.0)].concat();
    assert_eq!(kinds(&evs), vec![SwingKind::Forehand]);
    // Long after the last swing the chain starts over with a forehand.
    run(&mut m, &w, Input::default(), 1.5);
    let evs = run(&mut m, &w, attack(), 0.05);
    assert_eq!(kinds(&evs), vec![SwingKind::Forehand]);
    assert!(m.melee.run.is_some());
    // A press too early in the swing is ignored.
    let evs = run(&mut m, &w, attack(), 0.02);
    assert!(kinds(&evs).is_empty());
}

#[test]
fn sword_hits_the_guard_in_reach_and_finishes_it() {
    let mut w = alert_guard_world();
    let mut m = facing_guard(&w);
    assert_eq!(m.melee.target, Some(7));
    let evs = [run(&mut m, &w, attack(), 0.05), run(&mut m, &w, Input::default(), 0.3)].concat();
    let hits: Vec<_> = swings(&evs).into_iter().filter(|e| matches!(e, MeleeEvent::Hit { .. })).collect();
    assert_eq!(hits, vec![MeleeEvent::Hit { target: 7, damage: 12.0, killing: false }]);
    // Down to 12 health: the next swing is a killing blow.
    w.set_health(7, 12.0);
    run(&mut m, &w, Input::default(), 1.5);
    let evs = [run(&mut m, &w, attack(), 0.05), run(&mut m, &w, Input::default(), 0.4)].concat();
    let ev = swings(&evs);
    assert!(matches!(ev[0], MeleeEvent::Swing { kind: SwingKind::KillingForehand, .. }), "{ev:?}");
    assert!(ev.contains(&MeleeEvent::Hit { target: 7, damage: 12.0, killing: true }), "{ev:?}");
}

#[test]
fn sword_recoils_off_walls_and_misses_out_of_reach() {
    let mut w = floor_world();
    w.add_box(Vec3::new(100.0, -300.0, 0.0), Vec3::new(140.0, 300.0, 300.0));
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let evs = [run(&mut m, &w, attack(), 0.05), run(&mut m, &w, Input::default(), 0.3)].concat();
    let env: Vec<_> = swings(&evs).into_iter().filter(|e| matches!(e, MeleeEvent::EnvHit { .. })).collect();
    assert_eq!(env.len(), 1, "{env:?}");
    assert!(matches!(&env[0], MeleeEvent::EnvHit { anim: Some(a), shake, .. } if a == "Forehand_Recoil" && *shake == 150.0));
    assert!(m.melee.run.as_ref().is_some_and(|r| r.recoiled));
    // A guard 3.5 m away is out of reach (2 m, plus the blade box's 50 cm).
    let w = alert_guard_world();
    let mut m = Motion::new(tuning(), Vec3::new(-350.0, 0.0, 0.0), 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    assert_eq!(m.melee.target, None);
    let evs = [run(&mut m, &w, attack(), 0.05), run(&mut m, &w, Input::default(), 0.4)].concat();
    assert!(swings(&evs).iter().all(|e| matches!(e, MeleeEvent::Swing { .. })));
}

#[test]
fn sword_reach_grows_with_forward_speed_and_sneak_when_crouched() {
    let t = melee_tuning();
    assert_eq!(t.reach(Vec3::new(350.0, 0.0, 0.0), Vec3::X), 200.0);
    // At 6 m/s: 1 + (600 - 350) * 0.004 = 2, and half of the bonus applies.
    assert!((t.reach(Vec3::new(600.0, 0.0, 0.0), Vec3::X) - 300.0).abs() < 0.01);
    assert!((t.reach(Vec3::new(5000.0, 0.0, 0.0), Vec3::X) - 200.0 * 1.6).abs() < 0.01);
    let w = alert_guard_world();
    let mut m = facing_guard(&w);
    run(&mut m, &w, Input { crouch: true, ..Default::default() }, 0.05);
    run(&mut m, &w, Input::default(), 0.3);
    assert!(m.crouched);
    let evs = run(&mut m, &w, attack(), 0.05);
    assert!(matches!(swings(&evs)[0], MeleeEvent::Swing { kind: SwingKind::Sneak, .. }));
}

#[test]
fn assassination_kills_an_unaware_guard_and_places_it() {
    let w = guard_world();
    // Behind the guard (it faces +X), 1.5 m back, facing it.
    let mut m = facing_guard(&w);
    assert_eq!(m.takedown.assassinate, Some(7));
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    let a = ev.assassination.expect("assassination");
    assert_eq!((a.target, a.side, a.fast, a.generic), (7, Side::Back, false, false));
    assert!(ev.melee.is_empty(), "no swing as well");
    assert_eq!(m.state, MotionState::Takedown);
    assert_eq!(m.takedown.run.as_ref().map(|r| r.anim.as_str()), Some("SlowBack"));
    // The player stays; the victim goes 1.1 m in front of them, turned so they're behind it.
    assert!((m.pos.x + 150.0).abs() < 1.0, "player stays, x {}", m.pos.x);
    assert!((a.victim_feet - Vec3::new(-40.0, 0.0, 0.0)).length() < 1.0, "victim at {:?}", a.victim_feet);
    assert!(a.victim_yaw.cos() > 0.999, "victim faces away, yaw {}", a.victim_yaw);
    run(&mut m, &w, Input::default(), 1.6);
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn assassination_needs_an_unaware_guard_and_paces_slow_and_fast_kills() {
    // A guard in combat is fought, not assassinated.
    let w = alert_guard_world();
    let m = facing_guard(&w);
    assert_eq!(m.takedown.assassinate, None);
    // Suspicious still counts; running doesn't.
    let mut w = guard_world();
    w.set_awareness(7, Awareness::Suspicious);
    let m = facing_guard(&w);
    assert_eq!(m.takedown.assassinate, Some(7));
    w.characters[0].1.running = true;
    let m = facing_guard(&w);
    assert_eq!(m.takedown.assassinate, None);
    // Slow first, then two fast ones, then slow again (the time limit doesn't apply when the
    // fast ones run out).
    let w = guard_world();
    let mut m = facing_guard(&w);
    let mut paces = Vec::new();
    for _ in 0..4 {
        let ev = m.update(&w, &attack(), 1.0 / 60.0);
        paces.push(ev.assassination.expect("assassination").fast);
        run(&mut m, &w, Input::default(), 2.0);
    }
    assert_eq!(paces, vec![false, true, true, false]);
}

#[test]
fn assassination_is_plain_when_the_world_is_in_the_way() {
    // A low wall between the player and the guard: room for the blade, not for the pairing.
    let mut w = guard_world();
    w.add_box(Vec3::new(-80.0, -100.0, 0.0), Vec3::new(-70.0, 100.0, 140.0));
    let mut m = facing_guard(&w);
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    let a = ev.assassination.expect("assassination");
    assert!(a.generic);
    assert_eq!(a.victim_feet, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(m.takedown.run.as_ref().map(|r| r.anim.as_str()), Some("Generic"));
}

/// A floor with a wall of the given height across +X at x = 100.
fn wall_world(height: f32) -> BoxWorld {
    let mut w = floor_world();
    w.add_box(Vec3::new(100.0, -500.0, 0.0), Vec3::new(500.0, 500.0, height));
    w
}

/// Faces `yaw`, walks into the wall and presses jump; returns the events of the press.
fn mantle_at(w: &BoxWorld, yaw: f32, crouched: bool) -> (Motion, Vec<StepEvents>) {
    let mut m = Motion::new(tuning(), Vec3::ZERO, yaw);
    run(&mut m, w, Input::default(), 0.3);
    if crouched {
        run(&mut m, w, Input { crouch: true, ..Default::default() }, 0.02);
        run(&mut m, w, Input::default(), 0.1);
    }
    run(&mut m, w, fwd(), 0.6);
    let evs = run(&mut m, w, Input { jump: true, ..fwd() }, 0.02);
    (m, evs)
}

#[test]
fn low_edges_step_up_at_once_and_higher_ones_climb() {
    // Low: stepped up the same frame, no climb.
    let w = wall_world(90.0);
    let (m, evs) = mantle_at(&w, 0.0, false);
    assert_eq!(evs.iter().find_map(|e| e.mantled), Some(MantleKind::Low));
    assert_eq!(m.state, MotionState::Walking);
    assert!(m.feet().z >= 90.0 && m.feet().z < 101.0, "stepped up to {}", m.feet().z);
    assert!(m.pos.x > 85.0, "over the edge, x {}", m.pos.x);

    // High, approached at an angle: the climb turns the player to face the edge.
    let w = wall_world(190.0);
    let (mut m, evs) = mantle_at(&w, 0.5, false);
    assert_eq!(evs.iter().find_map(|e| e.mantled), Some(MantleKind::High));
    assert_eq!(m.state, MotionState::Mantling);
    run(&mut m, &w, Input::default(), 1.0);
    assert_eq!(m.state, MotionState::Walking);
    assert!((m.feet().z - 190.0).abs() < 2.0, "on top, feet {}", m.feet().z);
    assert!(m.yaw.min(std::f32::consts::TAU - m.yaw) < 0.05, "faces the edge, yaw {}", m.yaw);
}

#[test]
fn mantle_needs_a_facing_wall_and_crouches_under_a_low_roof() {
    // Too sideways to the wall: no mantle.
    let w = wall_world(140.0);
    let (_, evs) = mantle_at(&w, 1.0, false);
    assert!(evs.iter().all(|e| e.mantled.is_none()), "no mantle at a glancing angle");

    // A roof over the top leaves only crouching room: a crouched player finds the edge, and
    // stays crouched on top.
    let mut w = wall_world(120.0);
    w.add_box(Vec3::new(100.0, -500.0, 260.0), Vec3::new(500.0, 500.0, 340.0));
    let (mut m, evs) = mantle_at(&w, 0.0, true);
    assert_eq!(evs.iter().find_map(|e| e.mantled), Some(MantleKind::Medium));
    run(&mut m, &w, Input::default(), 1.0);
    assert!(m.crouched && m.state == MotionState::Walking, "crouched on top: {:?} {}", m.state, m.crouched);
    assert!((m.feet().z - 120.0).abs() < 2.0, "on top, feet {}", m.feet().z);
}

#[test]
fn falling_fast_catches_edges_only_once_well_past_them() {
    // Falls past the top of a tall wall, holding forward; returns how far the edge was above the
    // feet when it was caught, and whether it was a hard catch.
    let fall = |speed: f32| {
        let w = wall_world(400.0);
        let mut m = Motion::new(tuning(), Vec3::new(69.0, 0.0, 380.0), 0.0);
        m.vel = Vec3::new(0.0, 0.0, -speed);
        for _ in 0..60 {
            let e = m.update(&w, &fwd(), 1.0 / 60.0);
            if e.mantled.is_some() {
                return Some((400.0 - m.feet().z, e.mantle_impact));
            }
        }
        None
    };
    let (slow, impact) = fall(300.0).expect("caught falling slowly");
    assert!(!impact && slow < 70.0, "slow: edge {slow} above the feet");
    let (fast, impact) = fall(1200.0).expect("caught falling fast");
    assert!(impact && fast >= 140.0, "fast: edge {fast} above the feet");
}

/// Sprints along +X in `w`, then starts a slide; returns the motion mid-slide.
fn sliding(w: &BoxWorld, yaw: f32) -> Motion {
    let mut m = Motion::new(tuning(), Vec3::ZERO, yaw);
    run(&mut m, w, Input { sprint: true, ..fwd() }, 1.0);
    run(&mut m, w, Input { sprint: true, crouch: true, ..fwd() }, 1.0 / 60.0);
    assert_eq!(m.state, MotionState::Sliding);
    m
}

#[test]
fn slide_eases_to_crouch_speed_and_cancels_only_after_its_start() {
    let w = floor_world();
    let mut m = sliding(&w, 0.0);
    let start = m.speed_2d();
    // A quarter of the way in, the cosine ease has barely begun.
    run(&mut m, &w, fwd(), 0.25);
    let eased = 1.0 - (1.0 - (0.25f32 * std::f32::consts::PI).cos()) * 0.5;
    let expected = 200.0 + (start - 200.0) * eased;
    assert!((m.speed_2d() - expected).abs() < 15.0, "speed {} vs {expected}", m.speed_2d());
    // Pulling back does nothing in the first half...
    run(&mut m, &w, Input { move_axis: Vec2::new(0.0, -1.0), ..Default::default() }, 1.0 / 60.0);
    assert_eq!(m.state, MotionState::Sliding);
    // ...and ends the slide after it.
    run(&mut m, &w, fwd(), 0.4);
    run(&mut m, &w, Input { move_axis: Vec2::new(0.0, -1.0), ..Default::default() }, 1.0 / 60.0);
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn slide_stops_dead_when_turned_aside() {
    // Sliding diagonally into a wall: it turns the slide 45 degrees, past the 25 allowed.
    let mut w = floor_world();
    w.add_box(Vec3::new(560.0, -2000.0, 0.0), Vec3::new(900.0, 2000.0, 200.0));
    let mut m = sliding(&w, std::f32::consts::FRAC_PI_4);
    let impact = (0..60).find_map(|_| m.update(&w, &fwd(), 1.0 / 60.0).slide_impact);
    assert_eq!(impact, Some(1.0), "impact reported");
    assert_eq!(m.state, MotionState::Walking);
    // (The rest of that frame is ordinary walking again.)
    assert!(m.speed_2d() < 40.0, "stopped dead, speed {}", m.speed_2d());
}

#[test]
fn lean_swings_the_head_out_on_its_lever_and_holds_the_view() {
    let w = floor_world();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.5);
    let rest = m.camera.eye;
    let lean = Input { lean: 1.0, ..Default::default() };
    run(&mut m, &w, lean, 1.0);
    // Lever 1.5 x 2 half-heights; out to the angle limit, plus at most the soften angle.
    let lever = 1.5 * 80.0 * 2.0;
    let side = m.camera.eye.y - rest.y;
    let (lo, hi) = (lever * 15f32.to_radians().sin(), lever * 19f32.to_radians().sin());
    assert!(side > lo - 2.0 && side < hi + 2.0, "leaned {side}, expected {lo}..{hi}");
    assert!(m.camera.eye.z < rest.z, "the head drops as it swings out");
    assert!(m.camera.roll > 0.0);
    // The view can't be turned far while leaning.
    run(&mut m, &w, Input { look: Vec2::new(2.0, 0.0), ..lean }, 1.0 / 60.0);
    assert!((m.yaw - 50f32.to_radians()).abs() < 1e-3, "yaw held at {}", m.yaw);
    // Let go: the spring brings the head home.
    run(&mut m, &w, Input::default(), 1.5);
    assert!((m.camera.eye.y - rest.y).abs() < 1.0, "back to {}", m.camera.eye.y - rest.y);
}

/// Top speed reached holding `input` for a while on open floor.
fn top_speed(t: MotionTuning, input: Input) -> f32 {
    let w = floor_world();
    let mut m = Motion::new(t, Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.2);
    run(&mut m, &w, input, 1.5);
    m.speed_2d()
}

#[test]
fn gait_follows_the_input_angle_and_strength() {
    let t = tuning();
    let at = |x: f32, y: f32| top_speed(tuning(), Input { move_axis: Vec2::new(x, y), ..Default::default() });
    let d = std::f32::consts::FRAC_1_SQRT_2;
    // Straight ahead, and the forward diagonal (outside the forward strafe angle): full run speed.
    assert!((at(0.0, 1.0) - t.run_speed).abs() < 2.0);
    assert!((at(d, d) - t.run_speed).abs() < 2.0, "forward diagonal {}", at(d, d));
    // Sideways, and the back diagonal (inside the backward strafe angle): the strafe factor.
    assert!((at(1.0, 0.0) - t.run_speed * t.strafe_mult_run).abs() < 2.0);
    assert!((at(d, -d) - t.run_speed * t.strafe_mult_run).abs() < 2.0, "back diagonal {}", at(d, -d));
    // Straight back: the backward factor.
    assert!((at(0.0, -1.0) - t.run_speed * t.backward_mult_run).abs() < 2.0);
    // A light push walks, a lighter one slow-walks, whatever the direction.
    assert!((at(0.0, 0.6) - t.walk_speed).abs() < 2.0);
    assert!((at(-0.3, 0.0) - t.slow_walk_speed).abs() < 2.0);
    // Sprinting backwards uses the sprint's own factor.
    let back_sprint = top_speed(tuning(), Input { move_axis: Vec2::new(0.0, -1.0), sprint: true, ..Default::default() });
    assert!((back_sprint - t.sprint_speed * t.backward_mult_sprint).abs() < 2.0, "back sprint {back_sprint}");
    // The sword in hand slows every gait.
    let mut slow = tuning();
    slow.sword_speed_factor = 0.75;
    assert!((top_speed(slow, fwd()) - t.run_speed * 0.75).abs() < 2.0);
}

#[test]
fn top_speed_falls_smoothly_and_braking_stops_dead() {
    let w = floor_world();
    let t = tuning();
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input { sprint: true, ..fwd() }, 1.5);
    assert!((m.speed_2d() - t.sprint_speed).abs() < 2.0);
    // Letting go of sprint: the cap comes down over a few frames, not at once.
    run(&mut m, &w, fwd(), 0.05);
    assert!(m.speed_2d() > t.run_speed + 100.0, "still fast: {}", m.speed_2d());
    run(&mut m, &w, fwd(), 1.0);
    assert!((m.speed_2d() - t.run_speed).abs() < 2.0);
    // No input: braking slows it over a few frames, then stops it dead.
    run(&mut m, &w, Input::default(), 1.0 / 60.0);
    assert!(m.speed_2d() > t.run_speed * 0.5 && m.speed_2d() < t.run_speed, "braking: {}", m.speed_2d());
    run(&mut m, &w, Input::default(), 0.5);
    assert_eq!(m.speed_2d(), 0.0);
}

#[test]
fn crawls_under_gaps_too_low_to_sneak_through() {
    // A slab 80 above the floor: too low for the sneaking box, fine for the crawling one.
    let mut w = floor_world();
    w.add_box(Vec3::new(200.0, -500.0, 80.0), Vec3::new(600.0, 500.0, 400.0));
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.2);
    run(&mut m, &w, fwd(), 1.0);
    assert!(m.crouched && m.crawling, "crawling under the slab at x {}", m.pos.x);
    assert!(m.pos.x > 230.0, "made it under, x {}", m.pos.x);
    run(&mut m, &w, fwd(), 2.0);
    assert!(!m.crouched && !m.crawling, "up again past it, x {}", m.pos.x);
}

#[test]
fn air_control_steers_but_never_adds_speed_past_ground_speed() {
    let w = floor_world();
    let t = tuning();
    // A standing jump, pushing forward: the push is the air-control share of the acceleration.
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    run(&mut m, &w, Input { jump: true, ..Default::default() }, 1.0 / 60.0);
    run(&mut m, &w, fwd(), 0.3);
    let expected = t.accel_rate * t.air_control * 0.3;
    assert!(m.state == MotionState::Falling && (m.speed_2d() - expected).abs() < 25.0, "air push {} vs {expected}", m.speed_2d());
    // A sprinting jump (faster than ground speed): pushing on can steer, but not speed up.
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input { sprint: true, ..fwd() }, 1.0);
    let launch = m.speed_2d();
    run(&mut m, &w, Input { jump: true, sprint: true, ..fwd() }, 1.0 / 60.0);
    run(&mut m, &w, Input { sprint: true, ..fwd() }, 0.3);
    assert!(m.speed_2d() <= launch + 0.5, "{} after launching at {launch}", m.speed_2d());
}

#[test]
fn falls_no_faster_than_terminal_velocity() {
    let mut w = BoxWorld::default();
    w.add_box(Vec3::new(-500.0, -500.0, -20000.0), Vec3::new(500.0, 500.0, -19900.0));
    let mut m = Motion::new(tuning(), Vec3::ZERO, 0.0);
    run(&mut m, &w, Input::default(), 4.0);
    assert_eq!(m.state, MotionState::Falling);
    assert!((m.vel.length() - tuning().terminal_velocity).abs() < 1.0, "falling at {}", m.vel.length());
}

#[test]
fn swims_at_water_speed_with_the_sword_put_away() {
    let mut w = floor_world();
    w.add_water(Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 400.0));
    let mut t = tuning();
    t.sword_speed_factor = 0.5;
    let water_speed = t.water_speed;
    let mut m = Motion::new(t, Vec3::new(0.0, 0.0, 500.0), 0.0);
    run(&mut m, &w, Input::default(), 2.0);
    assert_eq!(m.state, MotionState::Swimming);
    run(&mut m, &w, fwd(), 3.0);
    assert!((m.speed_2d() - water_speed).abs() < 2.0, "swimming at {}", m.speed_2d());
    // Letting go: the water slows the swimmer gradually (no braking).
    let before = m.speed_2d();
    run(&mut m, &w, Input::default(), 0.1);
    assert!(m.speed_2d() < before && m.speed_2d() > before * 0.5, "glides: {}", m.speed_2d());
}
