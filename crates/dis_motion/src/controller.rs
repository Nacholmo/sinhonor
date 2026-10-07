//! The player movement state machine: walk/run/sprint/crouch, jump and fall with air control,
//! slide, mantle (ledge finder), lean, swim, ladder, and Blink travel.

use crate::blink::{BlinkEvent, PawnView};
use crate::camera::{CameraFeel, CameraInput};
use crate::collide::{find_floor, move_slide, step_up, SKIN};
use crate::{yaw_axes, Blink, MantleTuning, MotionTuning, Vec2, Vec3, World};

/// Longest physics substep; larger frames are split.
const MAX_SUBSTEP: f32 = 1.0 / 90.0;
/// Extra distance below the feet that still counts as standing (lets the pawn follow stairs down).
const FLOOR_SNAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionState {
    Walking,
    Falling,
    Sliding,
    Mantling,
    Swimming,
    Ladder,
    Blinking,
}

/// One frame of player intent. `look` is the frame's yaw/pitch delta in radians.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// x = right, y = forward, each in `-1..=1`.
    pub move_axis: Vec2,
    pub look: Vec2,
    pub jump: bool,
    /// Crouch button (toggle on press, like the game's default binding).
    pub crouch: bool,
    pub sprint: bool,
    /// Slow walk modifier.
    pub walk: bool,
    /// -1 = lean left, 1 = lean right.
    pub lean: f32,
    /// Blink power button (hold to aim, release to go).
    pub blink: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MantleKind {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Default)]
pub struct StepEvents {
    pub jumped: bool,
    /// Downward speed at touchdown.
    pub landed: Option<f32>,
    pub fall_damage: Option<f32>,
    pub mantled: Option<MantleKind>,
    pub slid: bool,
    pub blink: Vec<BlinkEvent>,
}

#[derive(Clone, Copy, Debug)]
struct Ledge {
    dest: Vec3,
    edge_height: f32,
    crouch: bool,
    kind: MantleKind,
}

#[derive(Clone, Copy, Debug, Default)]
struct MantleRun {
    from: Vec3,
    to: Vec3,
    t: f32,
    duration: f32,
    crouch: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct SlideRun {
    t: f32,
    dir: Vec3,
    speed0: f32,
}

pub struct Motion {
    pub tuning: MotionTuning,
    /// Centre of the collision box.
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub state: MotionState,
    pub crouched: bool,
    pub sprinting: bool,
    pub blink: Blink,
    pub camera: CameraFeel,
    pub floor_normal: Option<Vec3>,
    pub last_mantle: Option<MantleKind>,
    crouch_wanted: bool,
    auto_crouched: bool,
    prev: Input,
    mantle: MantleRun,
    slide: SlideRun,
    swim_stroke: f32,
    fall_peak_speed: f32,
}

impl Motion {
    /// `feet` is the floor position the player stands on.
    pub fn new(tuning: MotionTuning, feet: Vec3, yaw: f32) -> Self {
        let pos = feet + Vec3::Z * (tuning.half_height + SKIN);
        Self {
            pos,
            vel: Vec3::ZERO,
            yaw,
            pitch: 0.0,
            state: MotionState::Falling,
            crouched: false,
            sprinting: false,
            blink: Blink::default(),
            camera: CameraFeel::default(),
            floor_normal: None,
            last_mantle: None,
            crouch_wanted: false,
            auto_crouched: false,
            prev: Input::default(),
            mantle: MantleRun::default(),
            slide: SlideRun::default(),
            swim_stroke: 0.0,
            fall_peak_speed: 0.0,
            tuning,
        }
    }

    pub fn half(&self) -> Vec3 {
        Self::half_for(&self.tuning, self.crouched)
    }

    fn half_for(t: &MotionTuning, crouched: bool) -> Vec3 {
        if crouched {
            Vec3::new(t.crouch_radius, t.crouch_radius, t.crouch_half_height)
        } else {
            Vec3::new(t.radius, t.radius, t.half_height)
        }
    }

    pub fn feet(&self) -> Vec3 {
        self.pos - Vec3::Z * self.half().z
    }

    /// Eye height above the box centre for the current stance (same feet-relative ratio as standing).
    fn eye_height_for(&self, crouched: bool) -> f32 {
        let t = &self.tuning;
        let stand_eye_from_feet = t.half_height + t.base_eye_height;
        let ratio = stand_eye_from_feet / (2.0 * t.half_height);
        let h = if crouched { t.crouch_half_height } else { t.half_height };
        2.0 * h * ratio - h
    }

    pub fn view_dir(&self) -> Vec3 {
        crate::view_dir(self.yaw, self.pitch)
    }

    pub fn speed_2d(&self) -> f32 {
        self.vel.truncate().length()
    }

    pub fn update(&mut self, world: &dyn World, input: &Input, dt: f32) -> StepEvents {
        let mut ev = StepEvents::default();
        let t = &self.tuning;
        self.yaw = (self.yaw + input.look.x).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + input.look.y).clamp(t.min_pitch_deg.to_radians(), t.max_pitch_deg.to_radians());

        let steps = (dt / MAX_SUBSTEP).ceil().clamp(1.0, 8.0) as usize;
        let h = dt / steps as f32;
        for i in 0..steps {
            // Button edges only on the first substep.
            let edge = if i == 0 { self.prev } else { *input };
            self.substep(world, input, &edge, h, &mut ev);
        }
        self.prev = *input;

        let feet_z = self.feet().z;
        let leaning = if self.state == MotionState::Walking && self.speed_2d() < 50.0 { input.lean } else { 0.0 };
        let cam_in = CameraInput {
            pos: self.pos,
            target_eye_height: self.eye_height_for(self.crouched),
            yaw: self.yaw,
            speed_2d: self.speed_2d(),
            run_speed: self.tuning.run_speed,
            grounded: matches!(self.state, MotionState::Walking | MotionState::Sliding),
            lean_axis: leaning,
            crouched: self.crouched,
            feet_z,
            blink_distortion: self.blink.fx.distortion,
        };
        self.camera.update(&self.tuning, world, &cam_in, dt);
        ev
    }

    fn pressed(now: bool, before: bool) -> bool {
        now && !before
    }

    fn substep(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        // --- Blink runs alongside every state; travelling takes over movement. ---
        let half = self.half();
        let can_blink = !matches!(self.state, MotionState::Mantling);
        let mut pawn = PawnView {
            pos: self.pos,
            half,
            eye: self.pos + Vec3::Z * self.eye_height_for(self.crouched),
            aim: self.view_dir(),
            yaw: self.yaw,
            velocity: self.vel,
            can_blink,
        };
        let blink_events = self.blink.tick(&self.tuning.blink, world, &mut pawn, input.blink, dt);
        for e in &blink_events {
            match e {
                BlinkEvent::Released => {
                    // The game switches to flying physics and crouches so low gaps fit.
                    if !self.crouched {
                        self.set_crouched(world, true);
                    }
                    self.pos = pawn.pos - Vec3::Z * (self.tuning.half_height - self.tuning.crouch_half_height);
                    pawn.pos = self.pos;
                    pawn.half = self.half();
                    self.state = MotionState::Blinking;
                }
                BlinkEvent::Ended => {
                    self.pos = pawn.pos;
                    self.vel = pawn.velocity;
                    self.after_blink(world, ev);
                }
                _ => {}
            }
        }
        if self.state == MotionState::Blinking {
            self.pos = pawn.pos;
            self.vel = pawn.velocity;
        }
        ev.blink.extend(blink_events);
        if self.state == MotionState::Blinking {
            return;
        }

        // --- Crouch toggle ---
        if Self::pressed(input.crouch, prev.crouch) && self.state != MotionState::Sliding {
            let sprinting_fast = self.state == MotionState::Walking && self.sprinting && self.speed_2d() > self.tuning.run_speed * 0.9;
            if sprinting_fast && !self.crouched {
                self.start_slide(world);
                ev.slid = true;
            } else {
                self.crouch_wanted = !self.crouched;
                self.auto_crouched = false;
            }
        }
        if self.state != MotionState::Sliding && self.crouch_wanted != self.crouched && !self.auto_crouched {
            self.set_crouched(world, self.crouch_wanted);
        }

        match self.state {
            MotionState::Walking => self.walk(world, input, prev, dt, ev),
            MotionState::Falling => self.fall(world, input, dt, ev),
            MotionState::Sliding => self.slide_tick(world, input, prev, dt, ev),
            MotionState::Mantling => self.mantle_tick(dt),
            MotionState::Swimming => self.swim(world, input, prev, dt, ev),
            MotionState::Ladder => self.ladder(world, input, prev, dt, ev),
            MotionState::Blinking => {}
        }

        // Enter water from any surface state.
        if matches!(self.state, MotionState::Walking | MotionState::Falling | MotionState::Sliding) {
            if let Some(w) = world.water(self.pos) {
                if w.surface_z > self.pos.z {
                    self.state = MotionState::Swimming;
                    self.set_crouched(world, false);
                    self.vel *= 0.5;
                }
            }
        }
    }

    /// Changes stance, keeping the feet in place. Standing up only happens if there's room.
    fn set_crouched(&mut self, world: &dyn World, crouch: bool) -> bool {
        if crouch == self.crouched {
            return true;
        }
        let t = &self.tuning;
        let dh = t.half_height - t.crouch_half_height;
        if crouch {
            self.pos.z -= dh;
            self.crouched = true;
            true
        } else {
            let stand = self.pos + Vec3::Z * dh;
            if world.overlaps(stand, Self::half_for(t, false)) {
                return false;
            }
            self.pos = stand;
            self.crouched = false;
            true
        }
    }

    // ----------------------------------------------------------------- walking

    fn wish(&self, input: &Input) -> (Vec3, f32) {
        let t = &self.tuning;
        let mut axis = input.move_axis;
        if axis.length() > 1.0 {
            axis = axis.normalize();
        }
        let (fwd, right) = yaw_axes(self.yaw);
        let dir = (fwd * axis.y + right * axis.x).normalize_or_zero();
        let (base, strafe, back) = if self.crouched {
            (t.crouch_speed, t.strafe_mult_sneak, t.backward_mult_sneak)
        } else if input.walk {
            (t.walk_speed, t.strafe_mult_run, t.backward_mult_run)
        } else if self.sprinting {
            (t.sprint_speed, t.strafe_mult_sprint, t.backward_mult_sprint)
        } else {
            (t.run_speed, t.strafe_mult_run, t.backward_mult_run)
        };
        // Direction-dependent max speed: an ellipse through forward (1), side (strafe) and back.
        let a = axis.normalize_or_zero();
        let along = if a.y >= 0.0 { 1.0 } else { back };
        let mult = if a == Vec2::ZERO { 0.0 } else { 1.0 / ((a.x / strafe).powi(2) + (a.y / along).powi(2)).sqrt() };
        (dir, base * mult * axis.length())
    }

    /// UE3 `CalcVelocity`: friction toward the wished direction, acceleration, speed cap.
    fn calc_velocity(&mut self, dir: Vec3, max_speed: f32, accel_scale: f32, friction: f32, dt: f32) {
        let t = &self.tuning;
        let mut v = self.vel.truncate();
        let d = dir.truncate();
        if d == Vec2::ZERO || max_speed <= 0.0 {
            let speed = v.length();
            let new = (speed - speed * 2.0 * friction * dt).max(0.0);
            v = if speed > 0.0 { v * (new / speed) } else { v };
        } else {
            let speed = v.length();
            v -= (v - d * speed) * (dt * friction).min(1.0);
            v += d * t.accel_rate * accel_scale * dt;
            let cap = max_speed.max(if accel_scale < 1.0 { speed } else { 0.0 });
            if v.length() > cap {
                v = v.normalize() * cap;
            }
        }
        self.vel.x = v.x;
        self.vel.y = v.y;
    }

    fn walk(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        self.sprinting = input.sprint && input.move_axis.y > 0.1 && !self.crouched;
        let leaning = input.lean != 0.0 && self.speed_2d() < 50.0;
        let (dir, max_speed) = if leaning { (Vec3::ZERO, 0.0) } else { self.wish(input) };

        // Ladder: walking into one starts climbing.
        if let Some(l) = world.ladder(self.pos, self.half()) {
            if dir.dot(-l.normal) > 0.5 {
                self.state = MotionState::Ladder;
                self.vel = Vec3::ZERO;
                return;
            }
        }

        // Jump (or mantle, if a ledge is in reach).
        if Self::pressed(input.jump, prev.jump) {
            if let Some(ledge) = self.find_ledge(world, &t.mantle, false) {
                self.start_mantle(ledge, ev);
                return;
            }
            if self.crouched && !self.set_crouched(world, false) {
                // No room to stand: no jump.
            } else {
                self.crouch_wanted = false;
                self.vel.z = t.jump_z;
                self.state = MotionState::Falling;
                self.fall_peak_speed = 0.0;
                ev.jumped = true;
                return;
            }
        }

        self.auto_crouch(world, dir);
        self.calc_velocity(dir, max_speed, 1.0, t.ground_friction, dt);
        self.vel.z = 0.0;
        self.ground_move(world, dt);
    }

    /// Crouches automatically in front of gaps only a crouched player fits through, and stands
    /// back up once the way ahead no longer needs it.
    fn auto_crouch(&mut self, world: &dyn World, dir: Vec3) {
        let t = &self.tuning;
        let stand_half = Self::half_for(t, false);
        let crouch_half = Self::half_for(t, true);
        let feet = self.feet();
        let stand_center = feet + Vec3::Z * (stand_half.z + 0.01);
        let crouch_center = feet + Vec3::Z * (crouch_half.z + 0.01);
        let look = if dir == Vec3::ZERO { yaw_axes(self.yaw).0 } else { dir };
        let probe = look * t.auto_crouch_test_distance;
        let needs_crouch = world.sweep(stand_center, stand_center + probe, stand_half).is_some_and(|h| h.normal.z.abs() < 0.3)
            && world.sweep(crouch_center, crouch_center + probe, crouch_half).is_none();
        if self.crouched {
            if self.auto_crouched && !self.crouch_wanted && !needs_crouch && self.set_crouched(world, false) {
                self.auto_crouched = false;
            }
        } else if needs_crouch && dir != Vec3::ZERO {
            self.set_crouched(world, true);
            self.auto_crouched = true;
        }
    }

    /// Horizontal move with step-up and floor following; switches to falling off ledges.
    fn ground_move(&mut self, world: &dyn World, dt: f32) {
        let t = self.tuning.clone();
        let half = self.half();
        let delta = Vec3::new(self.vel.x, self.vel.y, 0.0) * dt;
        let mut p = self.pos;
        let hits = move_slide(world, &mut p, delta, half);
        let blocked = hits.iter().any(|h| h.normal.z < t.walkable_floor_z);
        if blocked {
            let mut q = self.pos;
            if step_up(world, &mut q, delta, half, t.max_step_height, t.walkable_floor_z) && (q - self.pos).truncate().length() > (p - self.pos).truncate().length() + 0.1 {
                p = q;
            } else {
                for h in &hits {
                    let n = Vec3::new(h.normal.x, h.normal.y, 0.0).normalize_or_zero();
                    let into = self.vel.dot(n);
                    if into < 0.0 {
                        self.vel -= n * into;
                    }
                }
            }
        }
        self.pos = p;
        match find_floor(world, self.pos, half, t.max_step_height + FLOOR_SNAP, t.walkable_floor_z) {
            Some(f) => {
                if f.time * (t.max_step_height + FLOOR_SNAP) > SKIN + 0.05 {
                    self.pos = f.location + Vec3::Z * SKIN;
                }
                self.floor_normal = Some(f.normal);
            }
            None => {
                self.floor_normal = None;
                if self.state == MotionState::Sliding {
                    self.end_slide(world);
                }
                self.state = MotionState::Falling;
                self.fall_peak_speed = 0.0;
            }
        }
    }

    // ----------------------------------------------------------------- falling

    fn fall(&mut self, world: &dyn World, input: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        let (dir, max_speed) = self.wish(input);
        // Air control: a fraction of ground acceleration, never adding speed past the cap.
        let before = self.speed_2d();
        if dir != Vec3::ZERO {
            let mut v = self.vel.truncate() + dir.truncate() * t.accel_rate * t.air_control * dt;
            let cap = before.max(max_speed);
            if v.length() > cap {
                v = v.normalize() * cap;
            }
            self.vel.x = v.x;
            self.vel.y = v.y;
        }
        self.vel.z = (self.vel.z + t.gravity_z * dt).max(-t.max_fall_speed);
        self.fall_peak_speed = self.fall_peak_speed.max(-self.vel.z);

        // Ledge catch: pressing toward a ledge (or holding jump) while airborne.
        if input.move_axis.y > 0.1 || input.jump {
            if let Some(ledge) = self.find_ledge(world, &t.mantle, true) {
                self.start_mantle(ledge, ev);
                return;
            }
        }

        let half = self.half();
        let mut p = self.pos;
        let hits = move_slide(world, &mut p, self.vel * dt, half);
        self.pos = p;
        for h in &hits {
            if h.normal.z >= t.walkable_floor_z && self.vel.z <= 0.0 {
                let impact = -self.vel.z;
                self.vel.z = 0.0;
                self.state = MotionState::Walking;
                self.floor_normal = Some(h.normal);
                ev.landed = Some(impact);
                if impact > t.fall_damage_speed {
                    ev.fall_damage = Some(impact);
                }
                self.camera.land(impact, &t);
                return;
            }
            let into = self.vel.dot(h.normal);
            if into < 0.0 {
                self.vel -= h.normal * into;
            }
        }
        // Touching down without a blocking hit this frame (e.g. after a step).
        if self.vel.z <= 0.0 {
            if let Some(f) = find_floor(world, self.pos, half, 1.0, t.walkable_floor_z) {
                let impact = -self.vel.z;
                self.vel.z = 0.0;
                self.state = MotionState::Walking;
                self.floor_normal = Some(f.normal);
                ev.landed = Some(impact);
                self.camera.land(impact, &t);
            }
        }
    }

    // ----------------------------------------------------------------- slide

    fn start_slide(&mut self, world: &dyn World) {
        let dir = self.vel.truncate().normalize_or_zero().extend(0.0);
        self.slide = SlideRun { t: 0.0, dir, speed0: self.speed_2d() };
        self.set_crouched(world, true);
        self.crouch_wanted = true;
        self.state = MotionState::Sliding;
    }

    fn end_slide(&mut self, world: &dyn World) {
        if self.tuning.slide_allow_return_to_sprint && self.prev.sprint && self.set_crouched(world, false) {
            self.crouch_wanted = false;
        }
    }

    fn slide_tick(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        self.slide.t += dt;
        let frac = (self.slide.t / t.slide_time.max(1e-3)).min(1.0);
        let cancelable = frac >= t.slide_not_cancelable_pct;
        if cancelable && Self::pressed(input.jump, prev.jump) && self.set_crouched(world, false) {
            self.crouch_wanted = false;
            self.vel.z = t.jump_z;
            self.state = MotionState::Falling;
            ev.jumped = true;
            return;
        }
        if cancelable && Self::pressed(input.crouch, prev.crouch) && self.set_crouched(world, false) {
            self.crouch_wanted = false;
            self.state = MotionState::Walking;
            return;
        }
        // Speed bleeds from the entry speed down to crouch speed over the slide.
        let speed = self.slide.speed0 + (t.crouch_speed - self.slide.speed0) * frac;
        self.vel = self.slide.dir * speed;
        self.ground_move(world, dt);
        if self.state == MotionState::Sliding && frac >= 1.0 {
            self.state = MotionState::Walking;
            self.end_slide(world);
        }
    }

    // ----------------------------------------------------------------- mantle

    /// Probes for a mantleable ledge in front of the player.
    fn find_ledge(&self, world: &dyn World, m: &MantleTuning, airborne: bool) -> Option<Ledge> {
        let t = &self.tuning;
        let half = self.half();
        let feet = self.pos.z - half.z;
        let (fwd, _) = yaw_axes(self.yaw);
        let tiny = Vec3::splat(1.0);
        let reach = half.x + m.edge_search_dist;
        if airborne && self.vel.z < -m.max_fall_speed_for_mantle {
            return None;
        }

        // 1. Scan up the wall in front for its top edge.
        let max_face_z = m.max_vertical_angle_edge_face_deg.to_radians().sin();
        let max_face_h = m.max_horizontal_angle_edge_face_deg.to_radians().cos();
        let mut wall = None;
        let mut edge_found = false;
        let mut h = if airborne { m.line_check_step } else { m.min_edge_height };
        while h <= m.max_edge_height + m.line_check_step {
            let s = Vec3::new(self.pos.x, self.pos.y, feet + h);
            match world.sweep(s, s + fwd * reach, tiny) {
                Some(hit) if !hit.start_penetrating && hit.normal.z.abs() <= max_face_z && (-fwd).dot(hit.normal.with_z(0.0).normalize_or_zero()) >= max_face_h => {
                    wall = Some(hit);
                }
                Some(_) => {
                    if wall.is_some() {
                        return None;
                    }
                }
                None => {
                    if wall.is_some() {
                        edge_found = true;
                        break;
                    }
                }
            }
            h += m.line_check_step;
        }
        let wall = wall?;
        if !edge_found {
            return None;
        }

        // 2. Find the top surface just past the edge.
        let over = wall.location + fwd * (m.forward_move_amount + 1.0);
        let top_start = Vec3::new(over.x, over.y, feet + m.max_edge_height + 2.0);
        let top_end = Vec3::new(over.x, over.y, feet + m.min_edge_height.min(h) - 2.0);
        let top = world.sweep(top_start, top_end, tiny)?;
        if top.start_penetrating || top.normal.z < m.max_slope_angle_edge_top_deg.to_radians().cos() {
            return None;
        }
        let surface_z = top.location.z - tiny.z;
        let edge_height = surface_z - feet;
        let min_h = if airborne { m.line_check_step } else { m.min_edge_height };
        if edge_height < min_h || edge_height > m.max_edge_height {
            return None;
        }
        if airborne && -self.vel.z > m.fall_speed_for_ledge_grab && edge_height < m.ledge_grab_min_edge_height {
            return None;
        }

        // 3. Room on top: standing if possible, else crouched.
        let xy = Vec3::new(wall.location.x, wall.location.y, 0.0) + fwd * (t.radius + m.forward_move_amount);
        let stand_half = Self::half_for(t, false);
        let crouch_half = Self::half_for(t, true);
        let stand = Vec3::new(xy.x, xy.y, surface_z + stand_half.z + SKIN);
        let crouch_dest = Vec3::new(xy.x, xy.y, surface_z + crouch_half.z + SKIN);
        let (dest, crouch, dhalf) = if !world.overlaps(stand, stand_half) {
            (stand, false, stand_half)
        } else if !world.overlaps(crouch_dest, crouch_half) {
            (crouch_dest, true, crouch_half)
        } else {
            return None;
        };

        // 4. Clear path: straight up to the ledge height, then over it.
        let up_to = Vec3::new(self.pos.x, self.pos.y, dest.z.max(self.pos.z));
        let probe_half = Vec3::new(dhalf.x.min(half.x), dhalf.y.min(half.y), dhalf.z) - Vec3::splat(1.0);
        let start = Vec3::new(self.pos.x, self.pos.y, self.pos.z - half.z + probe_half.z + 1.0);
        if world.sweep(start, up_to, probe_half).is_some_and(|h| h.time < 0.999) {
            return None;
        }
        if world.sweep(up_to, dest, probe_half).is_some_and(|h| h.time < 0.999) {
            return None;
        }
        let kind = if edge_height <= m.low_max_edge_height {
            MantleKind::Low
        } else if edge_height <= m.medium_max_edge_height {
            MantleKind::Medium
        } else {
            MantleKind::High
        };
        Some(Ledge { dest, edge_height, crouch, kind })
    }

    fn start_mantle(&mut self, ledge: Ledge, ev: &mut StepEvents) {
        let t = &self.tuning;
        let a = &t.anim;
        let anim = match (ledge.kind, ledge.crouch || self.crouched) {
            (MantleKind::Low, false) => a.mantle_low,
            (MantleKind::Medium, false) => a.mantle_medium,
            (MantleKind::High, false) => a.mantle_high,
            (MantleKind::Low, true) => a.crouch_mantle_low,
            (MantleKind::Medium, true) => a.crouch_mantle_medium,
            (MantleKind::High, true) => a.crouch_mantle_high,
        };
        let mut duration = anim / t.mantle.anim_rate.max(0.01);
        if ledge.kind == MantleKind::Low && t.mantle.low_uses_step_up {
            // Low ledges use a quick step-up blend instead of the full climb.
            duration = (t.mantle.low_step_up_blend_time * 2.0).min(duration);
        }
        // The pawn's stance switches up front so the box matches the destination.
        let feet = self.feet();
        self.crouched = ledge.crouch;
        self.crouch_wanted = ledge.crouch;
        let from = feet + Vec3::Z * self.half().z;
        self.mantle = MantleRun { from, to: ledge.dest, t: 0.0, duration: duration.max(0.05), crouch: ledge.crouch };
        self.pos = from;
        self.vel = Vec3::ZERO;
        self.state = MotionState::Mantling;
        self.last_mantle = Some(ledge.kind);
        ev.mantled = Some(ledge.kind);
        let _ = ledge.edge_height;
    }

    fn mantle_tick(&mut self, dt: f32) {
        let m = &mut self.mantle;
        m.t += dt;
        let u = (m.t / m.duration).min(1.0);
        // Rise first (ease-out), then move over the edge.
        let rise = (u / 0.65).min(1.0);
        let rise = 1.0 - (1.0 - rise) * (1.0 - rise);
        let over = ((u - 0.45) / 0.55).clamp(0.0, 1.0);
        let over = over * over * (3.0 - 2.0 * over);
        let z = m.from.z + (m.to.z - m.from.z) * rise;
        let xy = m.from.truncate() + (m.to.truncate() - m.from.truncate()) * over;
        self.pos = Vec3::new(xy.x, xy.y, z);
        if u >= 1.0 {
            self.pos = m.to;
            self.crouched = m.crouch;
            self.state = MotionState::Walking;
        }
    }

    /// After a blink: restore velocity (done by Blink), then ledge check, then try to stand.
    fn after_blink(&mut self, world: &dyn World, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        if let Some(ledge) = self.find_ledge(world, &t.mantle_blink, true) {
            self.start_mantle(ledge, ev);
            return;
        }
        if self.crouched && !self.crouch_wanted {
            self.set_crouched(world, false);
        }
        let half = self.half();
        if let Some(w) = world.water(self.pos) {
            if w.surface_z > self.pos.z {
                self.state = MotionState::Swimming;
                return;
            }
        }
        self.state = if find_floor(world, self.pos, half, FLOOR_SNAP, t.walkable_floor_z).is_some() && self.vel.z <= 0.0 {
            MotionState::Walking
        } else {
            MotionState::Falling
        };
        self.fall_peak_speed = 0.0;
    }

    // ----------------------------------------------------------------- swim

    fn swim(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        let Some(water) = world.water(self.pos - Vec3::Z * (self.half().z * 0.5)) else {
            self.state = MotionState::Falling;
            return;
        };
        let eye_z = self.pos.z + self.eye_height_for(false);
        let at_surface = eye_z >= water.surface_z - 10.0;
        let mut axis = input.move_axis;
        if axis.length() > 1.0 {
            axis = axis.normalize();
        }
        let (_, right) = yaw_axes(self.yaw);
        let look = self.view_dir();
        let mut wish = look * axis.y + right * axis.x;
        if at_surface && wish.z > 0.0 {
            wish.z = 0.0;
        }
        if input.jump {
            wish.z += 1.0;
        }
        let wish = wish.normalize_or_zero();
        // Strokes: strong acceleration for a stroke, then the weaker glide acceleration.
        if wish != Vec3::ZERO && self.swim_stroke <= 0.0 {
            self.swim_stroke = t.swim.stroke_time;
        }
        let accel = if self.swim_stroke > 0.0 { t.swim.max_accel } else { t.swim.min_accel };
        self.swim_stroke -= dt;
        self.vel += wish * accel * dt;
        self.vel *= (1.0 - t.ground_friction * 0.3 * dt).max(0.0);
        let cap = if wish == Vec3::ZERO { t.swim.max_speed_no_stroke } else { t.water_speed };
        if self.vel.length() > cap {
            self.vel = self.vel.normalize() * cap;
        }
        // Buoyancy holds the eyes at the surface when not diving.
        if axis == Vec2::ZERO || at_surface {
            let target = water.surface_z - self.eye_height_for(false) + 4.0;
            self.vel.z += (target - self.pos.z) * 6.0 * dt;
        }
        // Climb out onto a ledge.
        if at_surface && (Self::pressed(input.jump, prev.jump) || axis.y > 0.1) {
            if let Some(ledge) = self.find_ledge(world, &t.mantle, true) {
                self.start_mantle(ledge, ev);
                return;
            }
        }
        let half = self.half();
        let mut p = self.pos;
        move_slide(world, &mut p, self.vel * dt, half);
        self.pos = p;
        if world.water(self.pos).is_none_or(|w| w.surface_z < self.pos.z - half.z * 0.6) {
            self.state = MotionState::Falling;
        }
    }

    // ----------------------------------------------------------------- ladder

    fn ladder(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        let half = self.half();
        let Some(l) = world.ladder(self.pos, half + Vec3::new(4.0, 4.0, 0.0)) else {
            self.state = MotionState::Falling;
            return;
        };
        if Self::pressed(input.jump, prev.jump) {
            self.vel = l.normal * 250.0 + Vec3::Z * 200.0;
            self.state = MotionState::Falling;
            ev.jumped = true;
            return;
        }
        // Forward climbs toward where you look (up unless looking clearly down).
        let up = if self.pitch < -0.35 { -1.0 } else { 1.0 };
        self.vel = Vec3::Z * (input.move_axis.y * up * t.ladder_speed);
        let mut p = self.pos;
        move_slide(world, &mut p, self.vel * dt - l.normal * 2.0 * dt, half);
        self.pos = p;
        let feet = self.pos.z - half.z;
        if self.vel.z < 0.0 && find_floor(world, self.pos, half, 2.0, t.walkable_floor_z).is_some() {
            self.state = MotionState::Walking;
        } else if feet >= l.top_z - t.mantle.low_max_edge_height.min(60.0) && self.vel.z > 0.0 {
            // Near the top: climb off onto the landing behind the ladder.
            if let Some(ledge) = self.find_ledge(world, &t.mantle, true) {
                self.start_mantle(ledge, ev);
                return;
            }
            let stand_half = Self::half_for(&t, false);
            let into = Vec3::new(-l.normal.x, -l.normal.y, 0.0) * (t.radius * 2.0 + t.mantle.forward_move_amount);
            let dest = Vec3::new(self.pos.x + into.x, self.pos.y + into.y, l.top_z + stand_half.z + SKIN);
            if !world.overlaps(dest, stand_half) {
                let ledge = Ledge { dest, edge_height: l.top_z - feet, crouch: false, kind: MantleKind::Low };
                self.start_mantle(ledge, ev);
            }
        }
    }
}
