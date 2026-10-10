//! The player movement state machine: walk/run/sprint/crouch, jump and fall with air control,
//! Agility's power jump, slide, mantle (ledge finder), lean, swim, ladder, Blink travel and the
//! takedowns.

use crate::blink::{BlinkEvent, PawnView};
use crate::camera::{CameraFeel, CameraInput};
use crate::collide::{find_floor, move_slide, step_up, SKIN};
use crate::melee::{Melee, MeleeEvent, Swordsman};
use crate::takedown::{self, Assassination, Faller, Reach, Takedown, TakedownKind, TakedownRun, DIVE_SPEED_SCALE};
use crate::{yaw_axes, Blink, JumpStyle, MantleTuning, MotionTuning, Side, Vec2, Vec3, World};

/// Longest physics substep; larger frames are split.
const MAX_SUBSTEP: f32 = 1.0 / 90.0;
/// Extra distance below the feet that still counts as standing (lets the pawn follow stairs down).
const FLOOR_SNAP: f32 = 4.0;
/// How long jump must have been held, at the top of a jump, for the fixed power jump (game constant).
pub const POWER_JUMP_MIN_HOLD: f32 = 0.05;
/// Air control at or below this counts as none (engine constant).
const AIR_CONTROL_MIN: f32 = 0.05;
/// Below this horizontal speed, air control gets a boost to reach it (engine constant).
const AIR_CONTROL_MIN_SPEED: f32 = 10.0;
/// Braking is applied in steps of at most this long (engine constant).
const BRAKE_STEP: f32 = 0.03;
/// Braking below this speed stops dead (engine constant).
const BRAKE_STOP_SPEED: f32 = 12.7;
/// Pulling back further than this cancels a slide (game constant).
const SLIDE_CANCEL_BACK: f32 = -0.8;
/// How long the turn and slide to a mantle's start take (the game's default animation blend).
const MANTLE_INTRO_TIME: f32 = 0.2;
/// Upward speed at or below which a jump has reached its top (game constant).
const JUMP_TOP_SPEED: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionState {
    Walking,
    Falling,
    Sliding,
    Mantling,
    Swimming,
    Ladder,
    Blinking,
    /// A drop assassination: the player is held while the kill plays.
    Takedown,
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
    /// Attack button: a sword swing, or a drop assassination while falling onto someone.
    pub attack: bool,
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
    /// Agility's power jump kicked in.
    pub power_jumped: bool,
    /// Downward speed at touchdown.
    pub landed: Option<f32>,
    pub fall_damage: Option<f32>,
    pub mantled: Option<MantleKind>,
    /// The mantle caught an edge while falling fast (the game plays an impact animation).
    pub mantle_impact: bool,
    pub slid: bool,
    /// The slide was stopped dead by something in its way: the camera shake's strength.
    pub slide_impact: Option<f32>,
    pub blink: Vec<BlinkEvent>,
    /// Locked on to a drop-assassination target that is still too far below, and diving.
    pub dove_at: Option<u32>,
    /// A drop assassination started: the target (now dead) and the side it was taken from.
    pub drop_assassination: Option<(u32, Side)>,
    /// Sword swings, hits and recoils.
    pub melee: Vec<MeleeEvent>,
    /// A ground assassination started (the target is dead; place it as given for the paired kill).
    pub assassination: Option<Assassination>,
}

/// A jump on its way up. It ends at the top of the jump (or of the power jump).
#[derive(Clone, Copy, Debug, Default)]
pub struct JumpRun {
    /// How long jump has been held.
    pub held: f32,
    /// The power jump has been used (or, for the continuous style, the push is over).
    pub power: bool,
}

/// A place to climb from, as the edge finder reports it.
#[derive(Clone, Copy, Debug)]
struct MantleSpot {
    /// Where the climb starts: the player's feet, against the wall.
    start_feet: Vec3,
    /// Facing the edge.
    yaw: f32,
    edge_height: f32,
    /// The finder's last step forward, over the edge.
    over: Vec3,
    /// Caught while falling fast (the game plays an impact animation first).
    impact: bool,
    /// No room to stand on top.
    crouch: bool,
}

/// What the forward search found, for the last two checks.
struct MantleProbe {
    feet: Vec3,
    ext: Vec3,
    ext2: Vec3,
    clear: f32,
    origin: Vec3,
    h: f32,
    end: Vec3,
    over: Vec3,
    impact: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct MantleRun {
    from: Vec3,
    start: Vec3,
    /// How far the view turns to face the edge.
    yaw_turn: f32,
    rise: f32,
    over: Vec3,
    /// Index into `AnimTimes::mantle`.
    clip: usize,
    /// Animation time, and real time.
    t: f32,
    elapsed: f32,
    rate: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct SlideRun {
    t: f32,
    /// Velocity at the start, and the crouch-speed velocity it eases to.
    start: Vec3,
    target: Vec3,
    /// Something slowed the slide below its target: the easing stops.
    free: bool,
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
    /// Crouched low through a gap too low to sneak through (only while crouched).
    pub crawling: bool,
    pub sprinting: bool,
    /// The sword is in hand (the player moves a little slower); false for empty hands.
    pub sword_out: bool,
    /// The current top-speed factor, which falls smoothly and rises at once.
    pub speed_factor: f32,
    pub blink: Blink,
    pub camera: CameraFeel,
    pub floor_normal: Option<Vec3>,
    pub last_mantle: Option<MantleKind>,
    pub takedown: Takedown,
    pub melee: Melee,
    pub jump: Option<JumpRun>,
    /// Time left in the lean, and the view it started from (yaw, pitch).
    lean_hold: f32,
    lean_from: (f32, f32),
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
            crawling: false,
            sprinting: false,
            sword_out: true,
            speed_factor: 1.0,
            blink: Blink::default(),
            camera: CameraFeel::default(),
            floor_normal: None,
            last_mantle: None,
            takedown: Takedown::default(),
            melee: Melee::default(),
            jump: None,
            lean_hold: 0.0,
            lean_from: (0.0, 0.0),
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
        Self::half_of(&self.tuning, self.crouched, self.crawling)
    }

    fn half_for(t: &MotionTuning, crouched: bool) -> Vec3 {
        Self::half_of(t, crouched, false)
    }

    /// Standing, sneaking (crouched) or crawling (crouched low under something).
    fn half_of(t: &MotionTuning, crouched: bool, crawling: bool) -> Vec3 {
        if crouched && crawling {
            Vec3::new(t.crouch_radius, t.crouch_radius, t.crawl_half_height)
        } else if crouched {
            Vec3::new(t.crouch_radius, t.crouch_radius, t.crouch_half_height)
        } else {
            Vec3::new(t.radius, t.radius, t.half_height)
        }
    }

    pub fn feet(&self) -> Vec3 {
        self.pos - Vec3::Z * self.half().z
    }

    /// Eye height above the box centre for a stance (same feet-relative ratio as standing).
    fn eye_height_for(&self, crouched: bool) -> f32 {
        let t = &self.tuning;
        let stand_eye_from_feet = t.half_height + t.base_eye_height;
        let ratio = stand_eye_from_feet / (2.0 * t.half_height);
        let h = Self::half_of(t, crouched, crouched && self.crawling).z;
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
        // The kill animation owns the view.
        if self.state != MotionState::Takedown {
            self.yaw = (self.yaw + input.look.x).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch + input.look.y).clamp(t.min_pitch_deg.to_radians(), t.max_pitch_deg.to_radians());
        }
        // Leaning (and for a moment after) keeps the view near where the lean started.
        if self.state == MotionState::Walking && self.speed_2d() < 50.0 && input.lean != 0.0 {
            if self.lean_hold <= 0.0 {
                self.lean_from = (self.yaw, self.pitch);
            }
            self.lean_hold = t.lean.release_time;
        } else {
            self.lean_hold = (self.lean_hold - dt).max(0.0);
        }
        if self.lean_hold > 0.0 {
            let l = &t.lean;
            let (yaw0, pitch0) = self.lean_from;
            let turned = (self.yaw - yaw0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            self.yaw = (yaw0 + turned.clamp(l.min_yaw_deg.to_radians(), l.max_yaw_deg.to_radians())).rem_euclid(std::f32::consts::TAU);
            self.pitch = self.pitch.clamp(pitch0 + l.min_pitch_deg.to_radians(), pitch0 + l.max_pitch_deg.to_radians());
        }

        let steps = (dt / MAX_SUBSTEP).ceil().clamp(1.0, 8.0) as usize;
        let h = dt / steps as f32;
        for i in 0..steps {
            // Button edges only on the first substep.
            let edge = if i == 0 { self.prev } else { *input };
            self.substep(world, input, &edge, h, &mut ev);
        }
        self.prev = *input;
        let me = self.swordsman();
        self.melee.target = Melee::crosshair(&self.tuning.melee, world, &me, self.tuning.melee.reach(me.velocity, me.aim));
        self.takedown.tick(dt);
        self.takedown.assassinate = if matches!(self.state, MotionState::Walking | MotionState::Falling | MotionState::Sliding) && self.takedown.diving.is_none() {
            self.assassination_target(world)
        } else {
            None
        };
        self.takedown.prompt = match self.takedown.diving {
            Some(target) => takedown::check(world, &self.tuning.drop_assassinate, &self.faller(), target).map(|r| (target, r)),
            None if self.state == MotionState::Falling => takedown::search(world, &self.tuning.drop_assassinate, &self.faller()),
            None => None,
        };

        let feet_z = self.feet().z;
        let leaning = if self.state == MotionState::Walking && self.speed_2d() < 50.0 { input.lean } else { 0.0 };
        let cam_in = CameraInput {
            pos: self.pos,
            target_eye_height: self.eye_height_for(self.crouched),
            yaw: self.yaw,
            pitch: self.pitch,
            speed_2d: self.speed_2d(),
            run_speed: self.tuning.run_speed,
            grounded: matches!(self.state, MotionState::Walking | MotionState::Sliding | MotionState::Takedown),
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
        if self.state != MotionState::Falling {
            self.jump = None;
        }
        // --- Blink runs alongside every state; travelling takes over movement. ---
        let half = self.half();
        let can_blink = !matches!(self.state, MotionState::Mantling | MotionState::Takedown) && self.takedown.diving.is_none();
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
                    // The game switches to flying physics and crouches so low gaps fit. The target
                    // stays the pawn-centre point found while aiming, so a crouched body arrives
                    // with its feet higher, which is what lets blink reach ledges above you.
                    self.pos = pawn.pos;
                    if !self.crouched {
                        self.set_crouched(world, true);
                    }
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
        if self.state == MotionState::Takedown {
            self.takedown_tick(world, dt);
            return;
        }
        if self.state == MotionState::Falling && self.drop_attack(world, input, prev, ev) {
            self.melee.run = None;
            return;
        }
        // The sword: alongside walking, falling and sliding. An attack press that locked on to a
        // drop-assassination target is not also a swing.
        let can_fight = matches!(self.state, MotionState::Walking | MotionState::Falling | MotionState::Sliding) && self.takedown.diving.is_none();
        let swing_pressed = Self::pressed(input.attack, prev.attack) && ev.dove_at.is_none();
        // An attack on a character that may be assassinated is an assassination, not a swing.
        if swing_pressed && can_fight {
            if let Some(target) = self.assassination_target(world) {
                self.melee.run = None;
                self.start_assassination(world, target, ev);
                return;
            }
        }
        let me = Swordsman { attack: swing_pressed, can_fight, ..self.swordsman() };
        self.melee.tick(&self.tuning.melee, world, &me, dt, &mut ev.melee);
        if self.takedown.diving.is_some() && self.state != MotionState::Falling {
            self.takedown.diving = None;
        }
        // Locked on to a target: no steering, crouching or ledge grabs on the way down.
        let input = &if self.takedown.diving.is_some() { Input { look: input.look, ..Input::default() } } else { *input };
        let prev = &if self.takedown.diving.is_some() { Input::default() } else { *prev };

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
        // Stance changes wait for the moves that own the body to finish.
        let settled = !matches!(self.state, MotionState::Sliding | MotionState::Mantling | MotionState::Blinking | MotionState::Takedown);
        if settled && self.crouch_wanted != self.crouched && !self.auto_crouched {
            self.set_crouched(world, self.crouch_wanted);
        }

        match self.state {
            MotionState::Walking => self.walk(world, input, prev, dt, ev),
            MotionState::Falling => self.fall(world, input, dt, ev),
            MotionState::Sliding => self.slide_tick(world, input, prev, dt, ev),
            MotionState::Mantling => self.mantle_tick(world, dt),
            MotionState::Swimming => self.swim(world, input, prev, dt, ev),
            MotionState::Ladder => self.ladder(world, input, prev, dt, ev),
            MotionState::Blinking | MotionState::Takedown => {}
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
        if crouch == self.crouched && !(crouch && self.crawling) {
            return true;
        }
        self.set_stance(world, crouch, false) || (crouch && self.crouched)
    }

    /// Sets the stance if there's room for it, keeping the feet in place.
    fn set_stance(&mut self, world: &dyn World, crouched: bool, crawling: bool) -> bool {
        let from = self.half();
        let to = Self::half_of(&self.tuning, crouched, crawling);
        let centre = Vec3::new(self.pos.x, self.pos.y, self.pos.z - from.z + to.z);
        if to.z > from.z && world.overlaps(centre, to) {
            return false;
        }
        self.pos = centre;
        self.crouched = crouched;
        self.crawling = crouched && crawling;
        true
    }

    /// The wished direction and the gait's top-speed factor (of the run speed), as the game picks
    /// them (`NOTES.md` §6): walking for a light input, sprinting, sneaking, and the strafe and
    /// backward factors when the input points within their angles. The sword in hand slows it all.
    fn wish(&self, input: &Input) -> (Vec3, f32) {
        let t = &self.tuning;
        let g = &t.gait;
        let axis = input.move_axis;
        let mag = axis.length().min(1.0);
        let (fwd, right) = yaw_axes(self.yaw);
        let dir = (fwd * axis.y + right * axis.x).normalize_or_zero();
        if mag <= 1e-4 {
            return (Vec3::ZERO, 0.0);
        }
        let run = t.run_speed.max(1.0);
        let walking = !self.sprinting && (input.walk || mag <= g.walk_threshold);
        let walk_factor = if !input.walk && mag <= g.slow_walk_threshold { t.slow_walk_speed } else { t.walk_speed } / run;
        // The input's angle: within the strafe angle of sideways, or the backwards angle of back.
        let (side, ahead) = (axis.x.abs() / axis.length(), axis.y / axis.length());
        let backing = ahead < 0.0;
        let strafe_limit = if backing { g.strafe_angle_back_deg } else { g.strafe_angle_forward_deg };
        let strafing = !walking && side.clamp(-1.0, 1.0).acos().to_degrees() < strafe_limit;
        let backward = !walking && !strafing && backing && ahead.abs().clamp(-1.0, 1.0).acos().to_degrees() < g.backwards_angle_deg;
        let factor = if self.crouched {
            t.crouch_speed / run
        } else if self.sprinting {
            t.sprint_speed / run
                * if strafing {
                    t.strafe_mult_sprint
                } else if backward {
                    t.backward_mult_sprint
                } else {
                    1.0
                }
        } else if strafing {
            t.strafe_mult_run
        } else if backward {
            t.backward_mult_run
        } else if walking {
            walk_factor
        } else {
            1.0
        };
        let hands = if self.sword_out { t.sword_speed_factor } else { t.empty_hand_speed_factor };
        (dir, factor * hands)
    }

    /// Moves the top-speed factor toward `target`: down smoothly, up at once.
    fn blend_speed_factor(&mut self, target: f32, dt: f32) -> f32 {
        let k = (dt * self.tuning.speed_blend_down).clamp(0.0, 1.0);
        self.speed_factor = if target < self.speed_factor && self.speed_factor - target > 1e-4 {
            self.speed_factor + (target - self.speed_factor) * k
        } else {
            target
        };
        self.speed_factor
    }

    /// UE3 `CalcVelocity` on the ground: with no input, braking; otherwise friction turns the
    /// velocity toward the input while it accelerates; then the speed cap.
    fn calc_velocity(&mut self, dir: Vec3, max_speed: f32, max_accel: f32, friction: f32, dt: f32) {
        let mut v = self.vel.truncate();
        let d = dir.truncate();
        if d == Vec2::ZERO || max_accel <= 0.0 {
            v = Self::brake(v, friction, dt);
        } else {
            let speed = v.length();
            v -= (v - d * speed) * dt * friction;
            v += d * max_accel * dt;
        }
        if v.length() > max_speed {
            v = v.normalize_or_zero() * max_speed;
        }
        self.vel.x = v.x;
        self.vel.y = v.y;
    }

    /// UE3 braking: the velocity decays at twice the friction in steps of at most 0.03 s, and the
    /// result is the average over the frame; reversing or crawling to a near stop stops it dead.
    fn brake(v: Vec2, friction: f32, dt: f32) -> Vec2 {
        if v == Vec2::ZERO || dt <= 0.0 {
            return v;
        }
        let old = v;
        let (mut cur, mut avg, mut left) = (v, Vec2::ZERO, dt);
        while left > 0.0 {
            let step = left.min(BRAKE_STEP);
            cur -= cur * 2.0 * step * friction;
            left -= step;
            if cur.dot(old) > 0.0 {
                avg += cur * (step / dt);
            }
        }
        if avg.dot(old) < 0.0 || avg.length_squared() < BRAKE_STOP_SPEED * BRAKE_STOP_SPEED {
            Vec2::ZERO
        } else {
            avg
        }
    }

    fn walk(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        // Sprinting needs a firm push, in any direction; it ends sneaking.
        let firm = input.move_axis.length() > t.gait.stop_sprint_threshold;
        if input.sprint && firm && self.crouched && !self.crawling && !self.auto_crouched {
            self.crouch_wanted = false;
        }
        self.sprinting = input.sprint && firm && !self.crouched;
        let leaning = input.lean != 0.0 && self.speed_2d() < 50.0;
        let (dir, target) = if leaning { (Vec3::ZERO, 0.0) } else { self.wish(input) };
        let factor = self.blend_speed_factor(target, dt);

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
            if let Some(spot) = self.find_mantle_spot(world, &t.mantle) {
                self.start_mantle(world, spot, &t.mantle, ev);
                return;
            }
            if self.crouched && !self.set_crouched(world, false) {
                // No room to stand: no jump.
            } else {
                self.crouch_wanted = false;
                self.take_off(ev);
                return;
            }
        }

        self.auto_crouch(world, dir);
        self.calc_velocity(dir, t.run_speed * factor, t.accel_rate * factor, t.ground_friction, dt);
        self.vel.z = 0.0;
        self.ground_move(world, dt);
    }

    /// Crawls automatically into gaps too low to stand or sneak through (`m_fAutoCrouchTestDistance`
    /// ahead), and gets back up once the way ahead no longer needs it.
    fn auto_crouch(&mut self, world: &dyn World, dir: Vec3) {
        let t = &self.tuning;
        let want = Self::half_of(t, self.crouch_wanted, false);
        let crawl = Self::half_of(t, true, true);
        let feet = self.feet();
        let look = if dir == Vec3::ZERO { yaw_axes(self.yaw).0 } else { dir };
        let probe = look * t.auto_crouch_test_distance;
        let sweep = |h: Vec3| {
            let c = feet + Vec3::Z * (h.z + 0.01);
            world.sweep(c, c + probe, h)
        };
        let needs_crawl = sweep(want).is_some_and(|h| h.normal.z.abs() < 0.3) && sweep(crawl).is_none();
        if self.crawling {
            if !needs_crawl && self.set_stance(world, self.crouch_wanted, false) {
                self.auto_crouched = false;
            }
        } else if needs_crawl && dir != Vec3::ZERO && self.set_stance(world, true, true) {
            self.auto_crouched = true;
        }
    }

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

    /// Leaves the ground in a jump. With the held power-jump style, the jump is a power jump from
    /// the start.
    fn take_off(&mut self, ev: &mut StepEvents) {
        let p = &self.tuning.power_jump;
        let power = p.style == JumpStyle::HeldPowerFullStop && p.held_time > JUMP_TOP_SPEED;
        self.vel.z = if power { p.full_stop_z } else { self.tuning.jump_z };
        self.jump = Some(JumpRun { held: 0.0, power });
        self.state = MotionState::Falling;
        self.fall_peak_speed = 0.0;
        ev.jumped = true;
        ev.power_jumped = power;
    }

    /// Agility's power jump, on the way up a jump (`NOTES.md` §5e).
    fn jump_tick(&mut self, input: &Input, dt: f32, ev: &mut StepEvents) {
        let Some(mut run) = self.jump else { return };
        let p = &self.tuning.power_jump;
        match p.style {
            JumpStyle::FixedPower => {
                if self.vel.z <= JUMP_TOP_SPEED {
                    // At the top: still holding jump gives the power jump, once.
                    if run.power || run.held <= POWER_JUMP_MIN_HOLD || p.jump_z <= JUMP_TOP_SPEED {
                        self.jump = None;
                        return;
                    }
                    self.vel.z = p.jump_z;
                    run.power = true;
                    ev.power_jumped = true;
                }
                if !run.power {
                    // Letting go starts the count again.
                    run.held = if input.jump { run.held + dt } else { 0.0 };
                }
            }
            JumpStyle::HeldPowerFullStop => {
                if run.power && !input.jump {
                    self.vel.z = self.vel.z.min(p.extra_stop_vel);
                }
            }
            JumpStyle::ContinuousPower => {
                if !run.power && p.full_stop_z > JUMP_TOP_SPEED {
                    if !input.jump {
                        run.power = true;
                    } else {
                        run.held += dt;
                        if run.held > p.held_time {
                            run.power = true;
                            ev.power_jumped = true;
                        }
                    }
                    if !run.power {
                        self.vel.z += p.held_accel * dt;
                    }
                }
            }
        }
        if p.style != JumpStyle::FixedPower && self.vel.z <= JUMP_TOP_SPEED {
            self.jump = None;
            return;
        }
        self.jump = Some(run);
    }

    fn fall(&mut self, world: &dyn World, input: &Input, dt: f32, ev: &mut StepEvents) {
        self.jump_tick(input, dt, ev);
        let t = self.tuning.clone();
        let (dir, _) = self.wish(input);
        // Air control (UE3 falling, `NOTES.md` §5h): the input pushes at most the air-control share
        // of the acceleration, none if that push runs into something, and from ground speed up it
        // can steer but not add speed.
        let mut air = t.air_control;
        if air > AIR_CONTROL_MIN && dir != Vec3::ZERO {
            let test = (self.vel.with_z(0.0) + dir * t.accel_rate * air) * dt;
            if world.sweep(self.pos, self.pos + test.with_z(0.0), self.half()).is_some() {
                air = 0.0;
            }
        }
        let speed_2d = self.speed_2d();
        let mut max_accel = t.accel_rate * air;
        let mut limit_2d = None;
        if speed_2d < AIR_CONTROL_MIN_SPEED && air > 0.0 {
            max_accel += (AIR_CONTROL_MIN_SPEED - speed_2d) / dt.max(1e-4);
        } else if speed_2d >= t.run_speed {
            if air <= AIR_CONTROL_MIN {
                max_accel = 1.0;
            } else {
                limit_2d = Some(speed_2d);
            }
        }
        self.vel += dir * t.accel_rate.min(max_accel) * dt + Vec3::Z * t.gravity_z * dt;
        if let Some(l) = limit_2d.filter(|l| self.speed_2d() > *l) {
            let v = self.vel.truncate().normalize_or_zero() * l;
            self.vel = Vec3::new(v.x, v.y, self.vel.z);
        }
        if self.vel.length() > t.terminal_velocity {
            self.vel = self.vel.normalize_or_zero() * t.terminal_velocity;
        }
        self.fall_peak_speed = self.fall_peak_speed.max(-self.vel.z);

        // Ledge catch: pressing toward a ledge (or holding jump) while airborne.
        if input.move_axis.y > 0.1 || input.jump {
            if let Some(spot) = self.find_mantle_spot(world, &t.mantle) {
                self.start_mantle(world, spot, &t.mantle, ev);
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
        let start = self.vel.with_z(0.0);
        self.slide = SlideRun { t: 0.0, start, target: start.normalize_or_zero() * self.tuning.crouch_speed, free: false };
        self.set_crouched(world, true);
        self.crouch_wanted = true;
        self.state = MotionState::Sliding;
    }

    fn end_slide(&mut self, world: &dyn World) {
        if self.tuning.slide_allow_return_to_sprint && self.prev.sprint && self.set_crouched(world, false) {
            self.crouch_wanted = false;
        }
    }

    /// The slide (`NOTES.md` §5g): velocity eases from where it started to crouch speed along the
    /// same line over the slide time. It can be cancelled (crouch again, jump, or pull back) only
    /// after its first part, and stops dead if something turns it aside.
    fn slide_tick(&mut self, world: &dyn World, input: &Input, prev: &Input, dt: f32, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        self.slide.t += dt;
        let frac = (self.slide.t / t.slide_time.max(1e-3)).min(1.0);
        let cancelable = frac > t.slide_not_cancelable_pct;
        if cancelable && Self::pressed(input.jump, prev.jump) && self.set_crouched(world, false) {
            self.crouch_wanted = false;
            self.take_off(ev);
            return;
        }
        if cancelable && Self::pressed(input.crouch, prev.crouch) && self.set_crouched(world, false) {
            self.crouch_wanted = false;
            self.state = MotionState::Walking;
            return;
        }
        if cancelable && input.move_axis.y < SLIDE_CANCEL_BACK {
            self.state = MotionState::Walking;
            return;
        }
        let run = &self.slide;
        if run.free {
            self.calc_velocity(Vec3::ZERO, t.crouch_speed, 0.0, t.ground_friction, dt);
        } else {
            let ease = (1.0 - (frac * std::f32::consts::PI).cos()) * 0.5;
            let v = run.start + (run.target - run.start) * ease;
            self.vel = Vec3::new(v.x, v.y, self.vel.z);
        }
        self.ground_move(world, dt);
        if self.state != MotionState::Sliding {
            return;
        }
        // Turned aside by more than the deactivate angle: an impact, and the slide stops dead.
        let (from, now) = (self.slide.start.truncate().normalize_or_zero(), self.vel.truncate().normalize_or_zero());
        if from.dot(now) < t.slide_deactivate_angle_deg.to_radians().cos() {
            self.vel = Vec3::ZERO;
            self.state = MotionState::Walking;
            ev.slide_impact = Some(t.slide_impact_shake);
            return;
        }
        if self.vel.truncate().length() < self.slide.target.length() {
            self.slide.free = true;
        }
        if frac >= 1.0 {
            self.state = MotionState::Walking;
            self.end_slide(world);
        }
    }

    // ----------------------------------------------------------------- mantle

    /// The game's mantle edge finder (`NOTES.md` §5f): how much room there is above, then a box
    /// the player's size swept forward at rising heights until it meets a wall it can climb and
    /// then clears its top, then a look at the top's slope and whether there's room to stand.
    fn find_mantle_spot(&self, world: &dyn World, m: &MantleTuning) -> Option<MantleSpot> {
        let ext = self.half();
        let ext2 = Vec3::new(ext.x, ext.y, Self::half_for(&self.tuning, false).z);
        let feet = self.feet();
        let (facing, _) = yaw_axes(self.yaw);

        // Room above: the box swept up from where it stands.
        let centre = feet + Vec3::Z * ext.z;
        let up = m.max_edge_height - 2.0 * ext.z + 2.0 * ext2.z;
        let frac = if up > 0.0 { world.sweep(centre, centre + Vec3::Z * up, ext).map_or(1.0, |h| h.time) } else { 1.0 };
        let clear = frac * up + 2.0 * ext.z;

        // The lowest edge worth looking for depends on how fast the player is falling.
        if self.vel.z < -m.max_fall_speed_for_mantle {
            return None;
        }
        let step = m.line_check_step.max(1.0);
        let (min_edge, impact) = if self.vel.z < -m.fall_speed_for_ledge_grab { (m.ledge_grab_min_edge_height, true) } else { (m.min_edge_height, false) };
        let min_off = (min_edge - step).max(0.0) + ext.z;
        let max_off = (clear - ext.z).min(m.max_edge_height + ext.z);
        // Heights are the box centre's above the feet, stepped so the last lands on the highest.
        let range = max_off - min_off;
        let mut h = min_off + (range - (range / step).floor() * step) - 0.1;

        let max_tilt = m.max_vertical_angle_edge_face_deg.to_radians();
        let max_turn = m.max_horizontal_angle_edge_face_deg.to_radians();
        let (mut origin, mut dir, mut dist) = (feet, facing, m.edge_search_dist);
        let (mut wall, mut first) = (false, true);
        loop {
            let s = origin + Vec3::Z * h;
            let e = s + dir * dist;
            match world.sweep(s, e, ext) {
                // Characters are not ledges.
                Some(hit) if hit.is_pawn => return None,
                Some(hit) if !hit.start_penetrating => {
                    let n = hit.normal;
                    let tilt = std::f32::consts::FRAC_PI_2 - n.z.clamp(-1.0, 1.0).acos();
                    let turn = (-(facing.x * n.x + facing.y * n.y)).clamp(-1.0, 1.0).acos();
                    if tilt <= max_tilt && turn <= max_turn {
                        // A climbable face: carry on up it, pressed against it.
                        wall = true;
                        dir = Vec3::new(-n.x, -n.y, 0.0);
                        origin = Vec3::new(hit.location.x, hit.location.y, origin.z);
                        dist = if first { m.forward_move_amount } else { (1.0 - hit.time) * dist };
                        first = false;
                    }
                }
                Some(_) => {}
                None if wall => return self.check_mantle_spot(world, m, MantleProbe { feet, ext, ext2, clear, origin, h, end: e, over: dir * dist, impact }),
                None => {}
            }
            h += step;
            if h > max_off {
                return None;
            }
        }
    }

    /// The edge's top must not be too steep; then, is there room to stand on it?
    fn check_mantle_spot(&self, world: &dyn World, m: &MantleTuning, p: MantleProbe) -> Option<MantleSpot> {
        let step = m.line_check_step.max(1.0);
        if let Some(top) = world.sweep(p.end, p.end - Vec3::Z * (step + 1.0), p.ext) {
            if !top.start_penetrating && top.normal.z.clamp(-1.0, 1.0).acos() > m.max_slope_angle_edge_top_deg.to_radians() {
                return None;
            }
        }
        let stand = Vec3::new(p.origin.x, p.origin.y, p.origin.z + p.h - p.ext.z + p.ext2.z);
        let crouch = if p.ext2.z + stand.z <= p.clear + p.feet.z { world.sweep(stand, stand + p.over, p.ext2).is_some() } else { true };
        Some(MantleSpot {
            start_feet: Vec3::new(p.origin.x, p.origin.y, p.feet.z),
            yaw: p.over.y.atan2(p.over.x),
            edge_height: p.h - p.ext.z,
            over: p.over,
            impact: p.impact,
            crouch,
        })
    }

    /// Climbs onto `spot`: low edges with room to stand are stepped up at once; the rest play the
    /// mantle, moved by its animation.
    fn start_mantle(&mut self, world: &dyn World, spot: MantleSpot, m: &MantleTuning, ev: &mut StepEvents) {
        let kind = if spot.edge_height <= m.low_max_edge_height {
            MantleKind::Low
        } else if spot.edge_height <= m.medium_max_edge_height {
            MantleKind::Medium
        } else {
            MantleKind::High
        };
        self.last_mantle = Some(kind);
        ev.mantled = Some(kind);
        ev.mantle_impact = spot.impact;
        self.jump = None;
        let (fwd, _) = yaw_axes(spot.yaw);
        if kind == MantleKind::Low && m.low_uses_step_up && !spot.crouch {
            // Step-up: to the spot, up the edge, forward onto it, each move stopping at whatever
            // is in the way; the view catches up.
            let half = self.half();
            let before = self.pos;
            let mut p = self.pos;
            for d in [spot.start_feet + Vec3::Z * half.z - p, Vec3::Z * spot.edge_height, fwd * m.forward_move_amount] {
                p = match world.sweep(p, p + d, half) {
                    Some(h) if h.start_penetrating => p,
                    Some(h) => h.location,
                    None => p + d,
                };
            }
            self.pos = p;
            self.vel.z = 0.0;
            self.state = MotionState::Walking;
            self.camera.step(p - before, m.low_step_up_blend_time);
            return;
        }
        let feet = self.feet();
        if spot.crouch {
            self.crouched = true;
            self.crouch_wanted = true;
        }
        self.mantle = MantleRun {
            from: feet,
            start: spot.start_feet,
            yaw_turn: (spot.yaw - self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI,
            rise: spot.edge_height,
            over: spot.over,
            clip: kind as usize + if spot.crouch { 3 } else { 0 },
            t: 0.0,
            elapsed: 0.0,
            rate: m.anim_rate.max(0.01),
        };
        self.pos = feet + Vec3::Z * self.half().z;
        self.vel = Vec3::ZERO;
        self.state = MotionState::Mantling;
    }

    /// The mantle under way: a short turn and slide to the spot, then the animation's root path,
    /// its rise scaled to the edge height. The world doesn't block it.
    fn mantle_tick(&mut self, world: &dyn World, dt: f32) {
        let run = &mut self.mantle;
        let clip = &self.tuning.anim.mantle[run.clip];
        let intro_before = (run.elapsed / MANTLE_INTRO_TIME).min(1.0);
        run.elapsed += dt;
        run.t += dt * run.rate;
        let intro = (run.elapsed / MANTLE_INTRO_TIME).min(1.0);
        self.yaw = (self.yaw + run.yaw_turn * (intro - intro_before)).rem_euclid(std::f32::consts::TAU);

        let exit = clip.exit.max(0.05);
        // The rise and travel are measured from the clip's start to its end, as the game does.
        let (rise_total, fwd_total) = clip.root.last().map_or((0.0, 0.0), |s| (s.y, s.z));
        let (rise, fwd) = if rise_total < 1.0 {
            ((run.t / exit).min(1.0), (run.t / exit).min(1.0))
        } else {
            match clip.root.iter().position(|s| s.x >= run.t) {
                Some(0) => (0.0, 0.0),
                Some(k) => {
                    let (a, b) = (clip.root[k - 1], clip.root[k]);
                    let u = ((run.t - a.x) / (b.x - a.x).max(1e-6)).clamp(0.0, 1.0);
                    let s = a + (b - a) * u;
                    (s.y / rise_total, if fwd_total >= 1.0 { s.z / fwd_total } else { s.y / rise_total })
                }
                None => (1.0, 1.0),
            }
        };
        // Over the edge by the animation's travel, at least as far as the finder stepped.
        let over = run.over.with_z(0.0);
        let along = over.normalize_or_zero() * fwd_total.max(over.length());
        let feet = run.from + (run.start - run.from) * intro + Vec3::Z * (run.rise * rise) + along * fwd;
        self.pos = feet + Vec3::Z * Self::half_of(&self.tuning, self.crouched, self.crawling).z;

        if run.t >= exit {
            // Out of anything the climb ended inside, back toward the edge.
            let half = self.half();
            let back = along.normalize_or_zero();
            for _ in 0..8 {
                if !world.overlaps(self.pos, half) {
                    break;
                }
                self.pos -= back * 5.0;
            }
            self.state = MotionState::Walking;
        }
    }

    /// After a blink: restore velocity (done by Blink), then ledge check, then try to stand.
    fn after_blink(&mut self, world: &dyn World, ev: &mut StepEvents) {
        let t = self.tuning.clone();
        if let Some(spot) = self.find_mantle_spot(world, &t.mantle_blink) {
            self.start_mantle(world, spot, &t.mantle_blink, ev);
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

    // ----------------------------------------------------------------- drop assassination

    fn swordsman(&self) -> Swordsman {
        // Before the first camera update the eye isn't placed yet.
        let eye = if self.camera.eye == Vec3::ZERO { self.pos + Vec3::Z * self.eye_height_for(self.crouched) } else { self.camera.eye };
        Swordsman { eye, aim: self.view_dir(), velocity: self.vel, crouched: self.crouched, attack: false, can_fight: true }
    }

    fn faller(&self) -> Faller {
        Faller { pos: self.pos, half: self.half(), feet_z: self.feet().z, vel: self.vel, gravity_z: self.tuning.gravity_z }
    }

    /// While falling: follows a locked-on target, or answers an attack press. True if a kill started.
    fn drop_attack(&mut self, world: &dyn World, input: &Input, prev: &Input, ev: &mut StepEvents) -> bool {
        let t = self.tuning.drop_assassinate.clone();
        let me = self.faller();
        if let Some(target) = self.takedown.diving {
            match takedown::check(world, &t, &me, target) {
                Some(Reach::InRange) => return self.start_drop_kill(world, target, ev),
                Some(Reach::Track) => {}
                None => self.takedown.diving = None,
            }
            return false;
        }
        if !Self::pressed(input.attack, prev.attack) {
            return false;
        }
        match takedown::search(world, &t, &me) {
            Some((target, Reach::InRange)) => self.start_drop_kill(world, target, ev),
            Some((target, Reach::Track)) => {
                // Lock on: stop drifting and plunge straight down.
                self.takedown.diving = Some(target);
                self.vel = Vec3::new(0.0, 0.0, self.vel.z.min(0.0) * DIVE_SPEED_SCALE);
                self.jump = None;
                ev.dove_at = Some(target);
                false
            }
            None => false,
        }
    }

    /// Moves the player to the target's side for the kill (the target's animation places them),
    /// standing and facing it.
    fn start_drop_kill(&mut self, world: &dyn World, target: u32, ev: &mut StepEvents) -> bool {
        let Some(info) = world.pawn(target) else { return false };
        let side = takedown::side_of(&info, self.pos);
        let s = self.tuning.drop_assassinate.sides[side.index()].clone();
        let (feet, yaw) = takedown::landing(&info, &s);
        let from_eye = self.camera.eye;
        let half = Self::half_for(&self.tuning, false);
        let dest = feet + Vec3::Z * (half.z + SKIN);
        // Don't end up inside walls: stop where the way there is blocked (the target itself is
        // about to fall away, so it doesn't count).
        let pos = match world.sweep(self.pos, dest, half) {
            Some(h) if !(h.is_pawn && h.actor == target) && !h.start_penetrating => h.location,
            _ => dest,
        };
        self.pos = pos;
        self.crouched = false;
        self.crouch_wanted = false;
        self.auto_crouched = false;
        self.vel = Vec3::ZERO;
        self.yaw = yaw.rem_euclid(std::f32::consts::TAU);
        self.pitch = 0.0;
        self.fall_peak_speed = 0.0;
        self.state = MotionState::Takedown;
        self.takedown.diving = None;
        self.takedown.run = Some(TakedownRun { kind: TakedownKind::Drop, anim: s.anim, target, side, t: 0.0, duration: s.duration, from_eye });
        ev.drop_assassination = Some((target, side));
        true
    }

    /// The character under the crosshair, in the assassination's reach, that may be assassinated.
    fn assassination_target(&self, world: &dyn World) -> Option<u32> {
        let a = &self.tuning.assassinate;
        let me = self.swordsman();
        let reach = self.tuning.melee.reach_for(a.range, a.ray_scale_percent, me.velocity, me.aim);
        let target = Melee::crosshair(&self.tuning.melee, world, &me, reach)?;
        world.pawn(target).filter(|p| takedown::can_assassinate(a, p)).map(|_| target)
    }

    /// The ground assassination: the player stays (standing up), the victim is placed for the
    /// paired kill of the side the player is on, slow or fast as the pacing has it. Falling, or
    /// with the world between the player and the victim, it's the plain kill.
    fn start_assassination(&mut self, world: &dyn World, target: u32, ev: &mut StepEvents) {
        let Some(info) = world.pawn(target) else { return };
        let a = self.tuning.assassinate.clone();
        let side = takedown::side_of(&info, self.pos);
        let torso = Vec3::new(info.center.x, info.center.y, info.torso_z);
        let blocked = world.sweep(self.pos, torso, a.probe_extent).is_some_and(|h| !h.is_pawn);
        let generic = self.state == MotionState::Falling || blocked;
        let fast = !self.takedown.next_is_slow(&a);
        let kill = if generic {
            a.generic.clone()
        } else if fast {
            a.fast[side.index()].clone()
        } else {
            a.slow[side.index()].clone()
        };
        let from_eye = self.camera.eye;
        if self.crouched {
            self.set_crouched(world, false);
        }
        self.crouch_wanted = self.crouched;
        let (victim_feet, victim_yaw) = if generic {
            (Vec3::new(info.center.x, info.center.y, info.floor_z), info.yaw)
        } else {
            takedown::victim_placement(self.feet(), self.yaw, &kill)
        };
        self.vel = Vec3::ZERO;
        self.pitch = 0.0;
        self.state = MotionState::Takedown;
        let kind = TakedownKind::Assassination { fast, generic };
        self.takedown.run = Some(TakedownRun { kind, anim: kill.anim, target, side, t: 0.0, duration: kill.duration, from_eye });
        ev.assassination = Some(Assassination { target, side, fast, generic, victim_feet, victim_yaw });
    }

    fn takedown_tick(&mut self, world: &dyn World, dt: f32) {
        let Some(run) = self.takedown.run.as_mut() else {
            self.state = MotionState::Falling;
            return;
        };
        run.t += dt;
        self.vel = Vec3::ZERO;
        if run.t >= run.duration {
            self.takedown.run = None;
            let half = self.half();
            self.state = if find_floor(world, self.pos, half, FLOOR_SNAP, self.tuning.walkable_floor_z).is_some() {
                MotionState::Walking
            } else {
                MotionState::Falling
            };
        }
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
            if let Some(spot) = self.find_mantle_spot(world, &t.mantle) {
                self.start_mantle(world, spot, &t.mantle, ev);
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
            if let Some(spot) = self.find_mantle_spot(world, &t.mantle) {
                self.start_mantle(world, spot, &t.mantle, ev);
                return;
            }
            let stand_half = Self::half_for(&t, false);
            let into = Vec3::new(-l.normal.x, -l.normal.y, 0.0) * (t.radius * 2.0 + t.mantle.forward_move_amount);
            let dest = Vec3::new(self.pos.x + into.x, self.pos.y + into.y, l.top_z + stand_half.z + SKIN);
            if !world.overlaps(dest, stand_half) {
                let spot = MantleSpot { start_feet: self.feet(), yaw: into.y.atan2(into.x), edge_height: l.top_z - feet, over: into, impact: false, crouch: false };
                self.start_mantle(world, spot, &t.mantle, ev);
            }
        }
    }
}
