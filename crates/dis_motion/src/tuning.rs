//! Tuning consumed by the motion core. Filled from the game install by the `dishonored-data`
//! feature ([`MotionTuning::from_game`]) or by the host from any other source.

use crate::{AssassinateTuning, DropAssassinateTuning, MeleeTuning, Vec3};

#[derive(Clone, Debug)]
pub struct MotionTuning {
    pub gravity_z: f32,
    pub radius: f32,
    pub half_height: f32,
    pub crouch_radius: f32,
    pub crouch_half_height: f32,
    pub max_step_height: f32,
    /// Eye height above the cylinder centre when standing.
    pub base_eye_height: f32,
    pub walkable_floor_z: f32,
    pub max_fall_speed: f32,
    pub ladder_speed: f32,

    pub run_speed: f32,
    pub sprint_speed: f32,
    pub crouch_speed: f32,
    pub walk_speed: f32,
    pub water_speed: f32,
    pub accel_rate: f32,
    pub ground_friction: f32,
    pub braking: f32,
    pub strafe_mult_run: f32,
    pub strafe_mult_sneak: f32,
    pub strafe_mult_sprint: f32,
    pub backward_mult_run: f32,
    pub backward_mult_sneak: f32,
    pub backward_mult_sprint: f32,
    pub jump_z: f32,
    /// Agility's power jump; all zero without it.
    pub power_jump: PowerJumpTuning,
    pub air_control: f32,
    pub fall_damage_speed: f32,
    pub fall_death_speed: f32,

    pub slide_time: f32,
    pub slide_not_cancelable_pct: f32,
    pub slide_allow_return_to_sprint: bool,
    /// The slide stops dead when something turns it further than this.
    pub slide_deactivate_angle_deg: f32,
    /// Camera shake when it does (the game's strength value).
    pub slide_impact_shake: f32,
    pub auto_crouch_test_distance: f32,

    pub min_pitch_deg: f32,
    pub max_pitch_deg: f32,
    pub fov_deg: f32,
    pub fov_blend_speed: f32,
    pub bob_amount: f32,
    pub roll_amount: f32,

    pub lean: LeanTuning,
    pub swim: SwimTuning,
    pub mantle: MantleTuning,
    pub mantle_blink: MantleTuning,
    pub anim: AnimTimes,
    pub blink: BlinkTuning,
    pub drop_assassinate: DropAssassinateTuning,
    pub melee: MeleeTuning,
    pub assassinate: AssassinateTuning,
}

/// How the jump button can lift a jump further, in the game's order of jump styles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JumpStyle {
    /// Still holding jump at the top of a jump kicks the player up again (the game's setting).
    #[default]
    FixedPower,
    /// A higher jump from the start; letting go of jump cuts the climb.
    HeldPowerFullStop,
    /// Holding jump keeps pushing the player up for a while.
    ContinuousPower,
}

/// The power jump (`NOTES.md` §5e). With every speed at zero there is none, as without Agility.
#[derive(Clone, Debug, Default)]
pub struct PowerJumpTuning {
    pub style: JumpStyle,
    /// Fixed style: the upward speed the player is given at the top of a held jump.
    pub jump_z: f32,
    /// Held style: the take-off speed, cut to `extra_stop_vel` when jump is let go. Continuous
    /// style: above zero, the push is available.
    pub full_stop_z: f32,
    pub extra_stop_vel: f32,
    /// Continuous style: how long holding jump keeps pushing, and how hard (uu/s²).
    pub held_time: f32,
    pub held_accel: f32,
}

/// The lean (`NOTES.md` §5g).
#[derive(Clone, Debug)]
pub struct LeanTuning {
    /// How fast the head moves while lean is held (uu/s).
    pub lean_speed: f32,
    /// The lean lasts this long after letting go (the view stays limited).
    pub release_time: f32,
    /// View limits while leaning, around the view when the lean started.
    pub min_pitch_deg: f32,
    pub max_pitch_deg: f32,
    pub min_yaw_deg: f32,
    pub max_yaw_deg: f32,
    pub max_angle_deg: f32,
    pub max_angle_crouched_deg: f32,
    /// How far, and how fast, the angle limit gives when held against it.
    pub max_soften_angle_deg: f32,
    pub max_soften_speed: f32,
    pub camera_tilt_pct: f32,
    /// The pivot is this many collision heights below the head.
    pub height_pct: f32,
    pub springiness: f32,
    pub damping: f32,
    /// The camera's fixed step.
    pub fixed_time_step: f32,
}

#[derive(Clone, Debug)]
pub struct SwimTuning {
    pub min_accel: f32,
    pub max_accel: f32,
    pub max_speed_no_stroke: f32,
    pub stroke_time: f32,
}

#[derive(Clone, Debug)]
pub struct MantleTuning {
    pub line_check_step: f32,
    pub min_edge_height: f32,
    pub max_edge_height: f32,
    pub low_max_edge_height: f32,
    pub medium_max_edge_height: f32,
    pub low_uses_step_up: bool,
    pub low_step_up_blend_time: f32,
    pub max_vertical_angle_edge_face_deg: f32,
    pub max_horizontal_angle_edge_face_deg: f32,
    pub max_slope_angle_edge_top_deg: f32,
    pub edge_search_dist: f32,
    pub forward_move_amount: f32,
    pub fall_speed_for_ledge_grab: f32,
    pub max_fall_speed_for_mantle: f32,
    pub ledge_grab_min_edge_height: f32,
    pub anim_rate: f32,
}

/// How a mantle animation moves the player (`NOTES.md` §5f).
#[derive(Clone, Debug, Default)]
pub struct MantleClip {
    /// When the mantle ends (seconds, before the mantle animation rate).
    pub exit: f32,
    /// The animation root's path: (seconds, rise, forward). The rise is scaled to the edge height;
    /// with no forward travel the player moves over the edge by the finder's forward step. Empty:
    /// a straight rise over `exit`.
    pub root: Vec<Vec3>,
}

/// Durations (seconds) of the first-person animations that pace motion.
#[derive(Clone, Debug)]
pub struct AnimTimes {
    /// Low, medium and high mantles, then the crouched ones.
    pub mantle: [MantleClip; 6],
    pub land_small: f32,
    pub land_big: f32,
    pub slide_in: f32,
    pub slide_out: f32,
}

#[derive(Clone, Debug)]
pub struct BlinkLevel {
    pub distance: f32,
    pub horiz_distance: f32,
    pub vert_distance: f32,
    pub step_distance: f32,
    pub step_interval: f32,
    pub warmup_time: f32,
    pub cooldown_time: f32,
    pub warmup_wobble_max: f32,
    pub warmup_wobble_per_second: f32,
    pub warmup_distortion_min: f32,
    pub move_distortion_max: f32,
    pub move_blur_max: f32,
    pub move_reach_max_at_pct: f32,
    pub cooldown_wobble_count: i32,
}

#[derive(Clone, Debug)]
pub struct BlinkTuning {
    pub levels: Vec<BlinkLevel>,
    pub target_extent: [f32; 3],
    pub close_collision_distance: f32,
    pub close_collision_offset_step: f32,
    pub limit_vertical_from_ground: bool,
}

#[cfg(feature = "dishonored-data")]
impl MotionTuning {
    /// Builds the tuning from data loaded out of the user's Dishonored install.
    pub fn from_game(g: &dis_data::GameData) -> Self {
        let p = &g.player;
        let anim = |name: &str, fallback: f32| g.anim(name).unwrap_or(fallback);
        let mantle = |m: &dis_data::MantleTuning| MantleTuning {
            line_check_step: m.line_check_step,
            min_edge_height: m.min_edge_height,
            max_edge_height: m.max_edge_height,
            low_max_edge_height: m.low_max_edge_height,
            medium_max_edge_height: m.medium_max_edge_height,
            low_uses_step_up: m.low_uses_step_up,
            low_step_up_blend_time: m.low_step_up_blend_time,
            max_vertical_angle_edge_face_deg: m.max_vertical_angle_edge_face,
            max_horizontal_angle_edge_face_deg: m.max_horizontal_angle_edge_face,
            max_slope_angle_edge_top_deg: m.max_slope_angle_edge_top,
            edge_search_dist: m.edge_search_dist,
            forward_move_amount: m.forward_move_amount,
            fall_speed_for_ledge_grab: m.fall_speed_for_ledge_grab,
            max_fall_speed_for_mantle: m.max_fall_speed_for_mantle,
            ledge_grab_min_edge_height: m.ledge_grab_min_edge_height,
            anim_rate: p.mantle_anim_rate,
        };
        let side = |s: &dis_data::takedown::DropSide| crate::DropSide { anchor: crate::Vec3::from(s.anchor), duration: s.duration, anim: s.anim.clone() };
        MotionTuning {
            gravity_z: p.gravity_z,
            radius: p.collision_radius,
            half_height: p.collision_half_height,
            crouch_radius: p.crouch_radius,
            crouch_half_height: p.crouch_half_height,
            max_step_height: p.max_step_height,
            base_eye_height: p.base_eye_height,
            walkable_floor_z: p.walkable_floor_z,
            max_fall_speed: p.max_fall_speed,
            ladder_speed: p.ladder_speed,
            run_speed: p.ground_speed,
            sprint_speed: p.ground_speed_sprint,
            crouch_speed: p.ground_speed_crouch,
            walk_speed: p.ground_speed_walk,
            water_speed: p.water_speed,
            accel_rate: p.accel_rate,
            ground_friction: p.ground_friction,
            braking: p.ground_friction,
            strafe_mult_run: p.strafe_mult_run,
            strafe_mult_sneak: p.strafe_mult_sneak,
            strafe_mult_sprint: p.strafe_mult_sprint,
            backward_mult_run: p.backward_mult_run,
            backward_mult_sneak: p.backward_mult_sneak,
            backward_mult_sprint: p.backward_mult_sprint,
            jump_z: p.jump_z_attribute,
            power_jump: PowerJumpTuning {
                style: match p.jump_style {
                    dis_data::JumpStyle::FixedPower => JumpStyle::FixedPower,
                    dis_data::JumpStyle::HeldPowerFullStop => JumpStyle::HeldPowerFullStop,
                    dis_data::JumpStyle::ContinuousPower => JumpStyle::ContinuousPower,
                },
                jump_z: p.jump_z_power_jump,
                full_stop_z: p.power_jump_full_stop,
                extra_stop_vel: p.full_stop_extra_stop_vel,
                held_time: p.held_power_jump_button_time,
                held_accel: p.held_power_jump_accel,
            },
            air_control: p.air_control,
            fall_damage_speed: p.max_speed_before_fall_damage,
            fall_death_speed: p.max_speed_before_fall_death,
            slide_time: p.slide_time,
            slide_not_cancelable_pct: p.slide_percent_not_cancelable,
            slide_allow_return_to_sprint: p.slide_allow_return_to_sprint,
            slide_deactivate_angle_deg: p.slide_velocity_deactivate_angle,
            slide_impact_shake: p.slide_impact_camera_shake,
            auto_crouch_test_distance: p.auto_crouch_test_distance,
            min_pitch_deg: p.min_view_pitch,
            max_pitch_deg: p.max_view_pitch,
            fov_deg: p.default_fov,
            fov_blend_speed: p.fov_blend_speed,
            bob_amount: p.bob_amount,
            roll_amount: p.roll_amount,
            lean: LeanTuning {
                lean_speed: p.lean.lean_speed,
                release_time: p.lean.release_time,
                min_pitch_deg: p.lean.min_pitch,
                max_pitch_deg: p.lean.max_pitch,
                min_yaw_deg: p.lean.min_yaw,
                max_yaw_deg: p.lean.max_yaw,
                max_soften_angle_deg: p.lean.max_soften_angle,
                max_soften_speed: p.lean.max_soften_speed,
                fixed_time_step: p.lean.fixed_time_step,
                max_angle_deg: p.lean.max_angle,
                max_angle_crouched_deg: p.lean.max_angle_crouched,
                camera_tilt_pct: p.lean.camera_tilt_percent,
                height_pct: p.lean.height_pct,
                springiness: p.lean.springiness,
                damping: p.lean.damping,
            },
            swim: SwimTuning {
                min_accel: p.swim.min_accel,
                max_accel: p.swim.max_accel,
                max_speed_no_stroke: p.swim.max_speed_no_stroke,
                stroke_time: p.swim.stroke_time,
            },
            mantle: mantle(&p.mantle),
            mantle_blink: mantle(&p.mantle_blink),
            anim: AnimTimes {
                mantle: std::array::from_fn(|i| {
                    let c = &g.mantle_clips[i];
                    MantleClip { exit: c.exit, root: c.root.iter().map(|r| Vec3::from(*r)).collect() }
                }),
                land_small: anim("Empty_JumpLandSmall", 0.5) / p.land_anim_rate.max(0.01),
                land_big: anim("Generic_JumpLandBig", 1.0) / p.land_anim_rate.max(0.01),
                slide_in: anim("Empty_SlideIn", 0.3),
                slide_out: anim("Empty_SlideOutSneak", 0.3),
            },
            blink: BlinkTuning {
                levels: g
                    .blink
                    .levels
                    .iter()
                    .map(|l| BlinkLevel {
                        distance: l.distance,
                        horiz_distance: l.horiz_distance,
                        vert_distance: l.vert_distance,
                        step_distance: l.step_distance,
                        step_interval: l.step_interval,
                        warmup_time: l.warmup_time,
                        cooldown_time: l.cooldown_time,
                        warmup_wobble_max: l.warmup_wobble_max,
                        warmup_wobble_per_second: l.warmup_wobble_per_second,
                        warmup_distortion_min: l.warmup_distortion_min,
                        move_distortion_max: l.move_distortion_max,
                        move_blur_max: l.move_blur_max,
                        move_reach_max_at_pct: l.move_reach_max_at_pct,
                        cooldown_wobble_count: l.cooldown_wobble_count,
                    })
                    .collect(),
                target_extent: g.blink.target_test_extent,
                close_collision_distance: g.blink.close_collision_distance,
                close_collision_offset_step: g.blink.close_collision_offset_step,
                limit_vertical_from_ground: g.blink.limit_vertical_from_ground,
            },
            drop_assassinate: {
                let d = &g.drop_assassinate;
                DropAssassinateTuning {
                    hit_window: d.hit_window,
                    min_drop_dist: d.min_drop_dist,
                    max_drop_dist: d.max_drop_dist,
                    max_drop_jump_vel: d.max_drop_jump_vel,
                    min_drop_down_vel: d.min_drop_down_vel,
                    sides: std::array::from_fn(|i| side(&d.sides[i])),
                }
            },
            assassinate: {
                let a = &g.assassinate;
                AssassinateTuning {
                    range: a.range,
                    ray_scale_percent: a.ray_scale_percent,
                    probe_extent: crate::Vec3::from(a.probe_extent),
                    on_awareness: a.on_awareness,
                    can_assassinate_runners: a.can_assassinate_runners,
                    finishers_before_slow: a.finishers_before_slow,
                    time_before_slow: a.time_before_slow,
                    slow: std::array::from_fn(|i| side(&a.slow[i])),
                    fast: std::array::from_fn(|i| side(&a.fast[i])),
                    generic: side(&a.generic),
                }
            },
            melee: {
                let m = &g.melee;
                let anim = |a: &dis_data::melee::SwingAnim| crate::SwingAnim {
                    name: a.name.clone(),
                    zone: a.zone,
                    chain_input: a.chain_input,
                    interruptible: a.interruptible,
                    exit: a.exit,
                };
                MeleeTuning {
                    range: m.range,
                    ray_scale_percent: m.ray_scale_percent,
                    ray_speed_scale: m.ray_speed_scale,
                    ray_speed_scale_max: m.ray_speed_scale_max,
                    min_speed_ray_scale: m.min_speed_ray_scale,
                    sweep_size: m.sweep_size,
                    crosshair_size: m.crosshair_size,
                    chain_time: m.chain_time,
                    damage: m.damage,
                    env_hit_shake: m.env_hit_shake,
                    swings: std::array::from_fn(|i| {
                        m.swings[i]
                            .iter()
                            .map(|s| crate::SwingSet { swing: anim(&s.swing), env_hit: s.env_hit.as_ref().map(anim), env_hit_chain: s.env_hit_chain.as_ref().map(anim) })
                            .collect()
                    }),
                }
            },
        }
    }
}
