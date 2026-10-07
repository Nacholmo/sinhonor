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
