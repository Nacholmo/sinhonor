//! Behaviour tests on a box world with neutral, made-up tuning (no game data).
use dis_motion::boxworld::BoxWorld;
use dis_motion::*;

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
        crouch_half_height: 40.0,
        max_step_height: 30.0,
        base_eye_height: 70.0,
        walkable_floor_z: 0.7,
        max_fall_speed: 2000.0,
        ladder_speed: 200.0,
        run_speed: 400.0,
        sprint_speed: 600.0,
        crouch_speed: 200.0,
        walk_speed: 150.0,
        water_speed: 300.0,
        accel_rate: 2000.0,
        ground_friction: 8.0,
        braking: 8.0,
        strafe_mult_run: 0.8,
        strafe_mult_sneak: 0.8,
        strafe_mult_sprint: 0.5,
        backward_mult_run: 0.7,
        backward_mult_sneak: 0.7,
        backward_mult_sprint: 0.5,
        jump_z: 500.0,
        air_control: 0.2,
        fall_damage_speed: 1500.0,
        fall_death_speed: 2500.0,
        slide_time: 1.0,
        slide_not_cancelable_pct: 0.5,
        slide_allow_return_to_sprint: false,
        auto_crouch_test_distance: 100.0,
        min_pitch_deg: -85.0,
        max_pitch_deg: 85.0,
        fov_deg: 75.0,
        fov_blend_speed: 5.0,
        bob_amount: 0.5,
        roll_amount: 0.5,
        lean: LeanTuning { max_angle_deg: 15.0, max_angle_crouched_deg: 15.0, camera_tilt_pct: 0.5, height_pct: 1.0, springiness: 80.0, damping: 12.0 },
        swim: SwimTuning { min_accel: 500.0, max_accel: 2000.0, max_speed_no_stroke: 300.0, stroke_time: 0.3 },
        mantle_blink: mantle.clone(),
        mantle,
        anim: AnimTimes { mantle_low: 0.6, mantle_medium: 0.7, mantle_high: 1.0, crouch_mantle_low: 0.6, crouch_mantle_medium: 0.7, crouch_mantle_high: 1.0, land_small: 0.5, land_big: 1.0, slide_in: 0.3, slide_out: 0.3 },
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
                DropSide { anchor: Vec3::new(55.0, 0.0, 0.0), duration: 2.0 },
                DropSide { anchor: Vec3::new(0.0, -75.0, 0.0), duration: 2.0 },
                DropSide { anchor: Vec3::new(0.0, 75.0, 0.0), duration: 2.0 },
                DropSide { anchor: Vec3::new(-65.0, 0.0, 0.0), duration: 2.0 },
            ],
        },
        melee: melee_tuning(),
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
    run(&mut m, &w, Input { sprint: true, ..fwd() }, 1.2);
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
    assert!((m.speed_2d() - t.run_speed).abs() < 5.0);
    run(&mut m, &w, Input::default(), 1.0);
    let x0 = m.pos.x;
    run(&mut m, &w, Input { blink: true, ..Default::default() }, 0.2);
    let evs = run(&mut m, &w, Input::default(), 1.0);
    assert!(evs.iter().any(|e| e.blink.contains(&BlinkEvent::Ended)));
    let travelled = m.pos.x - x0;
    let reach = t.blink.levels[0].horiz_distance;
    assert!(travelled > reach * 0.8 && travelled < reach + 50.0, "travelled {travelled} of {reach}");
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
    // On the ground, attacking does nothing.
    let mut m = Motion::new(tuning(), Vec3::new(-100.0, 0.0, 0.0), 0.0);
    run(&mut m, &w, Input::default(), 0.3);
    let ev = m.update(&w, &attack(), 1.0 / 60.0);
    assert!(ev.drop_assassination.is_none());
    assert_eq!(m.state, MotionState::Walking);
}

#[test]
fn drop_assassination_side_follows_the_target_facing() {
    let guard = PawnInfo { center: Vec3::new(0.0, 0.0, 90.0), floor_z: 0.0, torso_z: 120.0, yaw: std::f32::consts::FRAC_PI_2, health: 25.0 };
    // Facing +Y: +Y is its front, +X its left.
    assert_eq!(side_of(&guard, Vec3::new(0.0, 100.0, 300.0)), Side::Front);
    assert_eq!(side_of(&guard, Vec3::new(0.0, -100.0, 300.0)), Side::Back);
    assert_eq!(side_of(&guard, Vec3::new(100.0, 10.0, 300.0)), Side::Left);
    assert_eq!(side_of(&guard, Vec3::new(-100.0, -10.0, 300.0)), Side::Right);
    let (feet, yaw) = landing(&guard, &DropSide { anchor: Vec3::new(0.0, -75.0, 0.0), duration: 1.0 });
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
    let mut w = guard_world();
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
    let w = guard_world();
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
    let w = guard_world();
    let mut m = facing_guard(&w);
    run(&mut m, &w, Input { crouch: true, ..Default::default() }, 0.05);
    run(&mut m, &w, Input::default(), 0.3);
    assert!(m.crouched);
    let evs = run(&mut m, &w, attack(), 0.05);
    assert!(matches!(swings(&evs)[0], MeleeEvent::Swing { kind: SwingKind::Sneak, .. }));
}
