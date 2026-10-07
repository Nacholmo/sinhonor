//! Blink, following the reverse-engineered behaviour of Dishonored's
//! `DishonoredActivePowerComponent_Blink` (see NOTES.md §5): ellipsoidal reach, box-swept
//! targeting with close-wall nudging and pull-back, ground correction, fixed-interval stepped
//! travel with lookahead, velocity restore on arrival, then cooldown.

use crate::{BlinkLevel, BlinkTuning, Vec3, World};

/// Sub-step length used while travelling (native constant).
const SUB_STEP: f32 = 50.0;
/// Horizontal distance at which the target counts as reached (native constant).
const ARRIVE_DIST: f32 = 25.0;
/// A step that moves less than this means we are stuck (native constant).
const STUCK_DIST: f32 = 2.0;
/// Lookahead multiplier for the per-substep sweep (native constant).
const LOOKAHEAD: f32 = 3.0;
/// Length of the downward ground trace under the target (native constant).
const GROUND_TRACE: f32 = 20000.0;
/// Velocity given to the pawn while travelling when no step interval is set (native constant).
const DEFAULT_TRAVEL_SPEED: f32 = 1000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlinkMode {
    Idle,
    /// Held: aiming and warming up.
    Targeting,
    /// Released: stepping toward the target.
    Travelling,
    /// Finished normally; counting down the cooldown.
    Cooldown,
    /// Aborted (fizzle); counting down the cooldown.
    AbortCooldown,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BlinkTarget {
    /// Where the pawn's centre will end up.
    pub point: Vec3,
    /// Surface normal of what the aim hit (zero if nothing).
    pub goal_normal: Vec3,
    pub ground_point: Vec3,
    pub ground_normal: Vec3,
    pub hit_something: bool,
    pub stop_at_pawn: bool,
}

/// Screen-effect parameters, matching the material parameters the game drives.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlinkFx {
    pub targeting: bool,
    pub distance_pct: f32,
    pub step_progress: f32,
    pub steps_taken: u32,
    pub time_elapsed: f32,
    pub warmup_time: f32,
    pub cooldown_time: f32,
    /// Lens distortion strength (`BlinkLensIntensity`-style).
    pub distortion: f32,
    /// Radial blur strength.
    pub blur: f32,
    /// Direction of the last step in the pawn's local space.
    pub local_dir: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlinkEvent {
    StartedTargeting,
    Released,
    /// Arrived (or stopped early); the controller restores velocity and checks for a ledge.
    Ended,
    /// Cast refused or cancelled.
    Fizzled,
    Ready,
    TouchedPawn(u32),
}

/// What the controller tells Blink about the pawn each tick.
pub struct PawnView {
    pub pos: Vec3,
    pub half: Vec3,
    pub eye: Vec3,
    pub aim: Vec3,
    pub yaw: f32,
    pub velocity: Vec3,
    pub can_blink: bool,
}

#[derive(Clone, Debug)]
pub struct Blink {
    pub level: usize,
    pub mode: BlinkMode,
    pub target: Option<BlinkTarget>,
    pub fx: BlinkFx,
    origin: Vec3,
    orig_velocity: Vec3,
    max_dist_from_origin: f32,
    step_timer: f32,
    time_elapsed: f32,
    time_in_mode: f32,
    cooldown_timer: f32,
    steps: u32,
    reached: bool,
    touched_pawn: bool,
    last_local_dir: Vec3,
}

impl Default for Blink {
    fn default() -> Self {
        Self {
            level: 0,
            mode: BlinkMode::Idle,
            target: None,
            fx: BlinkFx::default(),
            origin: Vec3::ZERO,
            orig_velocity: Vec3::ZERO,
            max_dist_from_origin: 0.0,
            step_timer: 0.0,
            time_elapsed: 0.0,
            time_in_mode: 0.0,
            cooldown_timer: 0.0,
            steps: 0,
            reached: false,
            touched_pawn: false,
            last_local_dir: Vec3::ZERO,
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Blink {
    pub fn level<'a>(&self, t: &'a BlinkTuning) -> &'a BlinkLevel {
        &t.levels[self.level.min(t.levels.len() - 1)]
    }

    pub fn velocity_to_restore(&self) -> Vec3 {
        self.orig_velocity
    }

    /// Reach along `aim` (the game's "squashed sphere").
    pub fn range(&self, t: &BlinkTuning, world: &dyn World, pawn: &PawnView, aim: Vec3) -> f32 {
        let l = self.level(t);
        let (max_h, max_v) = (l.horiz_distance, l.vert_distance);
        if aim.z <= 0.0 {
            return max_h;
        }
        let mut height_above_ground = 0.0;
        if t.limit_vertical_from_ground {
            let down = pawn.pos - Vec3::Z * max_v;
            let time = world.sweep(pawn.pos, down, Vec3::ZERO).map_or(1.0, |h| h.time);
            height_above_ground = (max_v * time).max(0.0);
        }
        let s = (max_v / aim.z).min(max_h);
        let mut v = aim * s;
        if t.limit_vertical_from_ground {
            v.z = v.z.min(max_v - height_above_ground);
        }
        v.length()
    }

    /// Finds where a blink along the current aim would put the pawn.
    pub fn find_target(&self, t: &BlinkTuning, world: &dyn World, pawn: &PawnView) -> BlinkTarget {
        let ext = Vec3::from(t.target_extent);
        let aim = pawn.aim.normalize_or_zero();
        let range = self.range(t, world, pawn, aim);
        let mut start = pawn.eye;
        let mut end = start + aim * range;
        let mut hit = world.sweep(start, end, ext);
        if hit.is_some_and(|h| h.start_penetrating) {
            // Aim started inside geometry: retry from the pawn's centre line.
            start = Vec3::new(pawn.pos.x, pawn.pos.y, pawn.eye.z.min(pawn.pos.z + pawn.half.z - ext.z));
            end = start + aim * range;
            hit = world.sweep(start, end, ext);
        }
        if let Some(h) = hit {
            if (h.location - start).length() < t.close_collision_distance {
                // A wall right in front: nudge the ray up, then sideways, and keep the first
                // offset that gets further.
                let step = t.close_collision_offset_step;
                let (_, right) = crate::yaw_axes(pawn.yaw);
                for off in [Vec3::Z * step, right * step, -right * step] {
                    if world.sweep(start, start + off, ext).is_some() {
                        continue;
                    }
                    let h2 = world.sweep(start + off, end + off, ext);
                    let better = match h2 {
                        None => true,
                        Some(h2) => h2.time > h.time + 1e-3 && !h2.start_penetrating,
                    };
                    if better {
                        start += off;
                        end += off;
                        hit = h2;
                        break;
                    }
                }
            }
        }
        let point = hit.map_or(end, |h| h.location);
        let mut target = BlinkTarget {
            goal_normal: hit.map_or(Vec3::ZERO, |h| h.normal),
            hit_something: hit.is_some(),
            stop_at_pawn: hit.is_some_and(|h| h.is_pawn),
            ..Default::default()
        };

        // Pull back along the aim until the pawn's collision fits, never past the start.
        let max_back = (point - start).length();
        let step = (pawn.half.x * 0.5).max(1.0);
        let mut back = 0.0;
        while back < max_back {
            let p = point - aim * back;
            if !world.overlaps(p, pawn.half) {
                break;
            }
            back += step;
        }
        let mut p = point - aim * back.min(max_back);

        // Ground under the target; keep the pawn's full height above it.
        match world.sweep(p, p - Vec3::Z * GROUND_TRACE, ext) {
            Some(g) if !g.start_penetrating => {
                target.ground_point = g.location - Vec3::Z * ext.z;
                target.ground_normal = g.normal;
                let min_z = g.location.z + pawn.half.z - ext.z;
                if p.z < min_z {
                    p.z = min_z;
                }
            }
            _ => {
                target.ground_point = p - Vec3::Z * GROUND_TRACE;
                target.ground_normal = Vec3::Z;
            }
        }
        target.point = p;
        target
    }

    /// Advances Blink. `held` is the power button. Returns events for the controller.
    pub fn tick(&mut self, t: &BlinkTuning, world: &dyn World, pawn: &mut PawnView, held: bool, dt: f32) -> Vec<BlinkEvent> {
        let mut ev = Vec::new();
        self.time_elapsed += dt;
        self.time_in_mode += dt;
        let level = self.level(t).clone();
        match self.mode {
            BlinkMode::Idle => {
                if held {
                    if pawn.can_blink {
                        self.enter(BlinkMode::Targeting);
                        self.time_elapsed = 0.0;
                        self.steps = 0;
                        self.reached = false;
                        self.touched_pawn = false;
                        ev.push(BlinkEvent::StartedTargeting);
                    } else {
                        ev.push(BlinkEvent::Fizzled);
                        self.enter(BlinkMode::AbortCooldown);
                    }
                }
            }
            BlinkMode::Targeting => {
                self.target = Some(self.find_target(t, world, pawn));
                if !held {
                    let target = self.target.unwrap();
                    let too_close = (target.point - pawn.pos).length() <= pawn.half.x;
                    if !pawn.can_blink || too_close {
                        self.target = None;
                        ev.push(BlinkEvent::Fizzled);
                        self.enter(BlinkMode::AbortCooldown);
                    } else {
                        self.origin = pawn.pos;
                        self.orig_velocity = pawn.velocity;
                        self.max_dist_from_origin = (target.point - pawn.pos).length();
                        self.step_timer = level.step_interval;
                        self.enter(BlinkMode::Travelling);
                        ev.push(BlinkEvent::Released);
                    }
                }
            }
            BlinkMode::Travelling => {
                let target = self.target.unwrap_or_default();
                let mut done = false;
                self.step_timer -= dt;
                // Several steps can fall into one frame at the game's 10 ms interval.
                while self.step_timer <= 0.0 && !done {
                    self.step_timer += level.step_interval.max(1e-4);
                    let before = pawn.pos;
                    let to = target.point - pawn.pos;
                    let facing = crate::yaw_axes(pawn.yaw).0;
                    if facing.dot(to) <= 0.0 && to.truncate().length() > ARRIVE_DIST {
                        done = true;
                    } else {
                        let goal = if to.length() >= level.step_distance {
                            pawn.pos + to.normalize() * level.step_distance
                        } else {
                            target.point
                        };
                        done |= self.sub_move(world, pawn, goal, &level, target.stop_at_pawn, &mut ev);
                    }
                    let moved = pawn.pos - before;
                    let (f, r) = crate::yaw_axes(pawn.yaw);
                    self.last_local_dir = Vec3::new(moved.dot(f), moved.dot(r), moved.z).normalize_or_zero();
                    if moved.length() < STUCK_DIST {
                        done = true;
                    }
                    self.steps += 1;
                    if (target.point - pawn.pos).truncate().length() < ARRIVE_DIST {
                        self.reached = true;
                    }
                    if (pawn.pos - self.origin).length() > self.max_dist_from_origin + 1.0 || self.reached {
                        done = true;
                    }
                }
                if done {
                    pawn.velocity = self.orig_velocity;
                    self.enter(BlinkMode::Cooldown);
                    ev.push(BlinkEvent::Ended);
                }
            }
            BlinkMode::Cooldown | BlinkMode::AbortCooldown => {
                self.cooldown_timer += dt;
                if self.cooldown_timer >= level.cooldown_time {
                    self.enter(BlinkMode::Idle);
                    self.target = None;
                    ev.push(BlinkEvent::Ready);
                }
            }
        }
        self.update_fx(&level, pawn);
        ev
    }

    /// Moves toward `goal` in sub-steps, with a lookahead sweep for pawns and blocking volumes.
    /// Returns true when travel should stop.
    fn sub_move(&mut self, world: &dyn World, pawn: &mut PawnView, goal: Vec3, level: &BlinkLevel, stop_at_pawn: bool, ev: &mut Vec<BlinkEvent>) -> bool {
        self.touched_pawn = false;
        loop {
            let to = goal - pawn.pos;
            let dist = to.length();
            if dist < 1e-3 {
                return false;
            }
            let dir = to / dist;
            let sub = dist.min(SUB_STEP);
            let speed = if level.step_interval > 0.0 { level.step_distance / level.step_interval } else { DEFAULT_TRAVEL_SPEED };
            pawn.velocity = dir * speed;
            if let Some(h) = world.sweep(pawn.pos, pawn.pos + dir * sub * LOOKAHEAD, pawn.half) {
                if h.is_pawn {
                    self.touched_pawn = true;
                    ev.push(BlinkEvent::TouchedPawn(h.actor));
                    return true;
                }
            }
            if world.blink_blocked(goal) {
                return true;
            }
            let next = pawn.pos + dir * sub;
            // The game moves without a collision test here (targeting already validated the
            // path); hosts with looser geometry get a safety check instead of tunnelling.
            if world.overlaps(next, pawn.half) {
                return true;
            }
            pawn.pos = next;
            if (goal - pawn.pos).length() > dist || (self.touched_pawn && stop_at_pawn) {
                return true;
            }
        }
    }

    fn enter(&mut self, mode: BlinkMode) {
        self.mode = mode;
        self.time_in_mode = 0.0;
        if matches!(mode, BlinkMode::Cooldown | BlinkMode::AbortCooldown) {
            self.cooldown_timer = 0.0;
        }
    }

    fn update_fx(&mut self, l: &BlinkLevel, pawn: &PawnView) {
        let dist_pct = if l.distance > 0.0 { ((self.origin - pawn.pos).length() / l.distance).clamp(0.0, 1.0) } else { 0.0 };
        let two_pi = std::f32::consts::TAU;
        let half_pi = std::f32::consts::FRAC_PI_2;
        let warm = l.warmup_time.max(1e-3);
        let wobble = 0.5 * ((two_pi * l.warmup_wobble_per_second * self.time_elapsed / warm - half_pi).sin() + 1.0);
        let ramp = smooth(self.time_elapsed / warm / warm);
        let warm_dist = ((1.0 - l.warmup_distortion_min) * wobble + l.warmup_distortion_min - wobble) * ramp + wobble;
        let warm_strength = warm_dist * l.warmup_wobble_max * ramp;
        let move_t = smooth(dist_pct / l.move_reach_max_at_pct.max(1e-3));
        let (distortion, blur) = match self.mode {
            BlinkMode::Targeting => (warm_strength, 0.0),
            BlinkMode::Travelling => (((move_t - warm_strength) * move_t + warm_strength) * l.move_distortion_max, l.move_blur_max * dist_pct),
            BlinkMode::Cooldown => {
                let c = self.cooldown_timer / l.cooldown_time.max(1e-3);
                let decay = (1.0 - c).max(0.0).powi(2);
                let s = (l.cooldown_wobble_count as f32 * c * two_pi - half_pi).sin();
                (0.5 * (s + 1.0) * l.move_blur_max * decay * move_t.max(0.2), 0.0)
            }
            _ => (0.0, 0.0),
        };
        self.fx = BlinkFx {
            targeting: self.mode == BlinkMode::Targeting,
            distance_pct: dist_pct,
            step_progress: if l.step_interval > 0.0 { (1.0 - self.step_timer / l.step_interval).clamp(0.0, 1.0) } else { 1.0 },
            steps_taken: self.steps,
            time_elapsed: self.time_elapsed,
            warmup_time: l.warmup_time,
            cooldown_time: self.cooldown_timer,
            distortion,
            blur,
            local_dir: self.last_local_dir,
        };
    }
}
