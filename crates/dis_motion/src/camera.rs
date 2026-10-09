//! First-person camera feel: smoothed eye height (crouch/mantle), head bob and roll, landing
//! dip, lean and FOV including the Blink lens punch.
//!
//! The game drives bob from a camera animation tree (`Ply_Nav_LocoCamera_at`); this is procedural,
//! scaled by the game's `m_BobAmount` and `m_RollAmount`. The lean follows the game's lean camera
//! (`NOTES.md` §5g): a head point on a lever around a pivot below it, pushed by the lean and pulled
//! back by a spring, stepped at the camera's fixed rate.

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
    /// The lean's sideways angle (radians, right positive).
    pub lean_angle: f32,
    lean: LeanHead,
    /// A step-up mantle moves the player at once; the view starts where it was and catches up.
    step_offset: Vec3,
    step_time: f32,
    initialised: bool,
}

/// The lean camera's head point, in view space (x forward, y right, z up) from the resting eye.
#[derive(Clone, Debug, Default)]
struct LeanHead {
    pos: Vec3,
    vel: Vec3,
    prev: Vec3,
    /// The lever's length (the pivot is this far below the resting head).
    len: f32,
    /// The angle limit and how far it currently gives (degrees).
    limit: f32,
    soften: f32,
    softening: bool,
    acc: f32,
}

impl LeanHead {
    /// One fixed step: the lean pushes the head, or the spring pulls it home; then it is held on the
    /// lever and within the angle limit.
    fn step(&mut self, t: &MotionTuning, push: Option<Vec3>, half_height: f32, crouched: bool, h: f32) {
        let l = &t.lean;
        let before = self.pos;
        self.prev = self.pos;
        if let Some(p) = push {
            self.vel = p;
        }
        let a = if self.pos.length_squared() > 1e-4 || self.vel.length_squared() > 1.0 {
            -l.springiness * self.pos - l.damping * self.vel
        } else {
            -self.vel
        };
        let v = self.vel + a * h;
        self.pos += v * h;
        self.vel = push.unwrap_or(v);

        self.len += (l.height_pct * half_height * 2.0 - self.len) * h * LEAN_SMOOTHING;
        let pivot = Vec3::new(0.0, 0.0, -self.len);
        self.pos = pivot + (self.pos - pivot).normalize_or(Vec3::Z) * self.len;

        let max = if crouched { l.max_angle_crouched_deg } else { l.max_angle_deg };
        self.limit += (max - self.limit) * h * LEAN_SMOOTHING;
        let (pitch, roll) = self.angles();
        let lim = (self.limit + self.soften).to_radians();
        if pitch.abs() > lim || roll.abs() > lim {
            let (p, r) = (pitch.clamp(-lim, lim), roll.clamp(-lim, lim));
            self.pos = pivot + Vec3::new(p.sin(), p.cos() * r.sin(), p.cos() * r.cos()) * self.len;
            self.softening = true;
        } else if (self.pos - before).length_squared() > 25.0 {
            self.softening = false;
        }
        // Held against the limit, it gives a little, then settles back.
        if self.softening {
            self.soften += (l.max_soften_angle_deg - self.soften) * l.max_soften_speed * h;
            if self.soften > l.max_soften_angle_deg {
                self.soften = l.max_soften_angle_deg;
                self.softening = false;
            }
        } else {
            self.soften = (self.soften - l.max_soften_speed * self.soften * h).max(0.0);
        }
    }

    /// Forward and sideways angles of the lever (radians).
    fn angles(&self) -> (f32, f32) {
        let u = (self.pos + Vec3::Z * self.len) / self.len.max(1e-3);
        (u.x.atan2(u.y.hypot(u.z)), u.y.atan2(u.z))
    }
}

/// Rate the lean's lever length and angle limit catch up with their targets (per second).
const LEAN_SMOOTHING: f32 = 10.0;

pub(crate) struct CameraInput {
    pub pos: Vec3,
    pub target_eye_height: f32,
    pub yaw: f32,
    pub pitch: f32,
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

        // Lean: fixed steps of the head point, then the view between the last two.
        let half_height = (c.pos.z - c.feet_z).max(1.0);
        if self.lean.len <= 0.0 {
            self.lean.len = t.lean.height_pct * half_height * 2.0;
            self.lean.limit = if c.crouched { t.lean.max_angle_crouched_deg } else { t.lean.max_angle_deg };
        }
        let push = (c.lean_axis != 0.0).then(|| Vec3::new(0.0, c.lean_axis.signum() * t.lean.lean_speed, 0.0));
        let h = t.lean.fixed_time_step.max(1e-3);
        self.lean.acc = (self.lean.acc + dt).min(h * 30.0);
        while self.lean.acc >= h {
            self.lean.acc -= h;
            self.lean.step(t, push, half_height, c.crouched, h);
        }
        let head = self.lean.prev.lerp(self.lean.pos, self.lean.acc / h);
        self.lean_angle = self.lean.angles().1;

        let base_eye = Vec3::new(c.pos.x, c.pos.y, c.pos.z + self.eye_height);
        // The head's offset, turned into the view.
        let fwd = crate::view_dir(c.yaw, c.pitch);
        let (_, right) = crate::yaw_axes(c.yaw);
        let up = fwd.cross(right);
        let lean_off = fwd * head.x + right * head.y + up * head.z;
        if self.step_time > 0.0 {
            let left = (self.step_time - dt).max(0.0);
            self.step_offset *= left / self.step_time;
            self.step_time = left;
        }
        let mut eye = base_eye + Vec3::Z * (bob_z + self.dip) + self.step_offset;
        let half = Vec3::splat(CAMERA_HALF);
        if lean_off.length_squared() > 1e-4 {
            match world.sweep(eye, eye + lean_off, half) {
                Some(hit) => {
                    let allowed = (hit.time * lean_off.length() - 1.0).max(0.0);
                    eye += lean_off.normalize() * allowed;
                    // Don't keep pushing into the wall.
                    let ratio = (allowed / lean_off.length()).max(0.0);
                    self.lean.pos *= ratio;
                    self.lean.prev *= ratio;
                    self.lean.vel = Vec3::ZERO;
                    self.lean_angle *= ratio;
                }
                None => eye += lean_off,
            }
        }
        self.eye = eye;
        self.roll = self.lean_angle * t.lean.camera_tilt_pct + bob_roll;

        let fov_target = t.fov_deg + c.blink_distortion * 6.0;
        self.fov_deg += (fov_target - self.fov_deg) * (1.0 - (-t.fov_blend_speed * dt).exp());
    }

    /// The player was moved by `delta` at once: the view follows over `blend` seconds.
    pub(crate) fn step(&mut self, delta: Vec3, blend: f32) {
        if blend > 0.0 {
            self.step_offset -= delta;
            self.step_time = blend;
        }
    }

    /// Kicks the landing dip; `impact` is the downward speed at touchdown.
    pub(crate) fn land(&mut self, impact: f32, t: &MotionTuning) {
        let strength = (impact / t.fall_damage_speed.max(1.0)).clamp(0.0, 1.0);
        self.dip_vel -= 120.0 * strength + 20.0;
    }
}
