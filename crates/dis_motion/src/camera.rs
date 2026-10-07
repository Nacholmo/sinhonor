//! First-person camera feel: smoothed eye height (crouch/mantle), head bob and roll, landing
//! dip, lean (spring-damped, collision-clamped) and FOV including the Blink lens punch.
//!
//! The game drives bob from a camera animation tree (`Ply_Nav_LocoCamera_at`); until the Edge
//! animation decoder exists this is procedural, scaled by the game's `m_BobAmount` and
//! `m_RollAmount`. Lean uses the game's spring constants, max angles, tilt and height ratio.

use crate::{MotionTuning, Vec3, World};

/// Camera collision half-extent (the game's camera cylinder is 9 x 9).
const CAMERA_HALF: f32 = 9.0;

#[derive(Clone, Debug, Default)]
pub struct CameraFeel {
    /// Eye position in world space (after lean and effects).
    pub eye: Vec3,
    pub roll: f32,
    pub fov_deg: f32,
    eye_height: f32,
    bob_time: f32,
    dip: f32,
    dip_vel: f32,
    pub lean_angle: f32,
    lean_vel: f32,
    initialised: bool,
}

pub(crate) struct CameraInput {
    pub pos: Vec3,
    pub target_eye_height: f32,
    pub yaw: f32,
    pub speed_2d: f32,
    pub run_speed: f32,
    pub grounded: bool,
    pub lean_axis: f32,
    pub crouched: bool,
    pub feet_z: f32,
    pub blink_distortion: f32,
}

impl CameraFeel {
    pub(crate) fn update(&mut self, t: &MotionTuning, world: &dyn World, c: &CameraInput, dt: f32) {
        if !self.initialised {
            self.eye_height = c.target_eye_height;
            self.fov_deg = t.fov_deg;
            self.initialised = true;
        }
        // Eye height eases toward its target (crouch, stand, mantle).
        let k = 1.0 - (-12.0 * dt).exp();
        self.eye_height += (c.target_eye_height - self.eye_height) * k;

        // Head bob: phase advances with ground speed, vertical at twice the lateral rate.
        let speed_ratio = if c.grounded { (c.speed_2d / c.run_speed.max(1.0)).min(1.6) } else { 0.0 };
        self.bob_time += dt * (0.3 + 0.7 * speed_ratio) * 2.0;
        let bob_z = t.bob_amount * 3.0 * speed_ratio * (self.bob_time * std::f32::consts::TAU).sin().abs();
        let bob_roll = t.roll_amount * 0.6_f32.to_radians() * speed_ratio * (self.bob_time * std::f32::consts::PI).sin();

        // Landing dip: critically-damped spring pulled back to zero.
        let (s, d) = (120.0, 22.0);
        self.dip_vel += (-s * self.dip - d * self.dip_vel) * dt;
        self.dip += self.dip_vel * dt;

        // Lean: spring toward the target angle with the game's springiness/damping.
        let max = if c.crouched { t.lean.max_angle_crouched_deg } else { t.lean.max_angle_deg }.to_radians();
        let target = c.lean_axis.clamp(-1.0, 1.0) * max;
        self.lean_vel += (t.lean.springiness * (target - self.lean_angle) - t.lean.damping * self.lean_vel) * dt;
        self.lean_angle += self.lean_vel * dt;

        let base_eye = Vec3::new(c.pos.x, c.pos.y, c.pos.z + self.eye_height);
        // Lean pivots at the feet; the lever is the eye height scaled by the game's height ratio.
        let lever = (base_eye.z - c.feet_z) * t.lean.height_pct * 0.5;
        let (_, right) = crate::yaw_axes(c.yaw);
        let lean_off = right * (self.lean_angle.sin() * lever) - Vec3::Z * (lever * (1.0 - self.lean_angle.cos()));
        let mut eye = base_eye + Vec3::Z * (bob_z + self.dip);
        let half = Vec3::splat(CAMERA_HALF);
        if lean_off.length_squared() > 1e-4 {
            match world.sweep(eye, eye + lean_off, half) {
                Some(h) => {
                    let allowed = (h.time * lean_off.length() - 1.0).max(0.0);
                    eye += lean_off.normalize() * allowed;
                    // Don't keep pushing into the wall.
                    let ratio = allowed / lean_off.length();
                    self.lean_angle *= ratio.max(0.0).sqrt().max(0.0);
                }
                None => eye += lean_off,
            }
        }
        self.eye = eye;
        self.roll = self.lean_angle * t.lean.camera_tilt_pct + bob_roll;

        let fov_target = t.fov_deg + c.blink_distortion * 6.0;
        self.fov_deg += (fov_target - self.fov_deg) * (1.0 - (-t.fov_blend_speed * dt).exp());
    }

    /// Kicks the landing dip; `impact` is the downward speed at touchdown.
    pub(crate) fn land(&mut self, impact: f32, t: &MotionTuning) {
        let strength = (impact / t.fall_damage_speed.max(1.0)).clamp(0.0, 1.0);
        self.dip_vel -= 120.0 * strength + 20.0;
    }
}
