//! Takedowns. The drop assassination: attacking while falling onto a character kills it from
//! above, following the behaviour specified in `NOTES.md` §5b. A target is looked for along the
//! fall the player is predicted to make; one found early is locked on to, and the player plunges
//! straight down at it until close enough, then lands at the side of the target they came from.
//! The ground assassination (§5d): attacking a character that hasn't noticed the player kills it
//! outright, with a paired kill for the side the player is on.
//!
//! Characters are the host's: sweeps report them as pawn hits, and [`World::pawn`] describes them.

use crate::{yaw_axes, Vec3, World};

/// Downward speed multiplier when locking on to a target that is still too far below (game constant).
pub const DIVE_SPEED_SCALE: f32 = 2.0;

/// A character the player can take down, as the host describes it.
#[derive(Clone, Copy, Debug)]
pub struct PawnInfo {
    /// Collision centre; line-of-sight traces aim here.
    pub center: Vec3,
    /// Height of the floor the character stands on.
    pub floor_z: f32,
    /// Height of the character's torso, the point the drop distance is measured to.
    pub torso_z: f32,
    /// Facing (radians, Unreal yaw: 0 = +X, +90° = +Y).
    pub yaw: f32,
    /// Health left; a sword blow that leaves none is a killing blow.
    pub health: f32,
    pub awareness: Awareness,
    /// Running (assassinations can be refused for runners).
    pub running: bool,
}

/// A character's awareness of the player, in the game's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Awareness {
    #[default]
    Unaware,
    AwareOfPlayer,
    Surprised,
    Suspicious,
    Fearful,
    InCombat,
    Begging,
    Choked,
}

/// The side of the target the player strikes from, as the target sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Front,
    Left,
    Right,
    Back,
}

impl Side {
    /// In the game's cardinal-direction order.
    pub const ALL: [Side; 4] = [Side::Front, Side::Left, Side::Right, Side::Back];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// One side's paired kill.
#[derive(Clone, Debug)]
pub struct DropSide {
    /// Where the player's feet go, in the target's frame (x forward, y right, z up, from its feet).
    pub anchor: Vec3,
    /// How long the player is held (seconds).
    pub duration: f32,
    /// The player's kill animation.
    pub anim: String,
}

/// The ground assassination.
#[derive(Clone, Debug)]
pub struct AssassinateTuning {
    /// Reach (scaled by forward speed like a sword swing, with its own share of the bonus).
    pub range: f32,
    pub ray_scale_percent: f32,
    /// Half-extents of the box swept from the player to the target's torso; if the world is in
    /// the way, the plain (unpaired) kill is used.
    pub probe_extent: Vec3,
    /// Which awareness states allow it, indexed like [`Awareness`].
    pub on_awareness: [bool; 8],
    pub can_assassinate_runners: bool,
    /// How many fast kills (a random count in this range) and how long (a random time in this
    /// range) before the slow kill again.
    pub finishers_before_slow: (u32, u32),
    pub time_before_slow: (f32, f32),
    /// Paired kills by side ([`Side::index`]): the slow ones, the fast ones. The anchor is where
    /// the player stands in the victim's frame; the victim is moved to put it there.
    pub slow: [DropSide; 4],
    pub fast: [DropSide; 4],
    /// The plain kill, when there's no room for a paired one or the player is falling.
    pub generic: DropSide,
}

#[derive(Clone, Debug)]
pub struct DropAssassinateTuning {
    /// How far ahead (seconds) the fall is predicted when looking for a target.
    pub hit_window: f32,
    /// The player's feet must be more than this above the target's torso.
    pub min_drop_dist: f32,
    /// ...and at most this, for the kill to start; higher up, the player locks on and dives.
    pub max_drop_dist: f32,
    /// Vertical speed must be below this (so not while rising).
    pub max_drop_jump_vel: f32,
    /// The prediction assumes at least this downward speed.
    pub min_drop_down_vel: f32,
    /// Indexed by [`Side::index`].
    pub sides: [DropSide; 4],
}

/// The player as the drop checks see them.
pub(crate) struct Faller {
    pub pos: Vec3,
    /// Collision half-extents; the checks sweep the player's own box.
    pub half: Vec3,
    pub feet_z: f32,
    pub vel: Vec3,
    pub gravity_z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// A target is below, but still too far: attacking locks on and dives.
    Track,
    /// Close enough: attacking kills now.
    InRange,
}

/// Where the fall takes the player over the hit window: current velocity (at least
/// `min_drop_down_vel` downward) under gravity, with no horizontal acceleration.
fn predicted_drop(t: &DropAssassinateTuning, me: &Faller) -> Vec3 {
    let w = t.hit_window;
    let vz = me.vel.z.min(-t.min_drop_down_vel);
    Vec3::new(me.vel.x * w, me.vel.y * w, vz * w + 0.5 * me.gravity_z * w * w)
}

/// Whether `target` can be dropped on from here, and whether it is already in range.
pub(crate) fn check(world: &dyn World, t: &DropAssassinateTuning, me: &Faller, target: u32) -> Option<Reach> {
    let info = world.pawn(target)?;
    if me.vel.z >= t.max_drop_jump_vel {
        return None;
    }
    if -predicted_drop(t, me).z <= t.min_drop_dist {
        return None;
    }
    let height = me.feet_z - info.torso_z;
    if height <= t.min_drop_dist {
        return None;
    }
    // The player's box must be able to reach the target without hitting anything else first.
    let hit = world.sweep(me.pos, info.center, me.half)?;
    if !hit.is_pawn || hit.actor != target {
        return None;
    }
    Some(if height <= t.max_drop_dist { Reach::InRange } else { Reach::Track })
}

/// Sweeps the player's box along the predicted fall, then straight down, for a target.
pub(crate) fn search(world: &dyn World, t: &DropAssassinateTuning, me: &Faller) -> Option<(u32, Reach)> {
    let drop = predicted_drop(t, me);
    let along = world.sweep(me.pos, me.pos + drop, me.half);
    let hit = along.or_else(|| world.sweep(me.pos, me.pos + Vec3::Z * drop.z, me.half))?;
    if !hit.is_pawn || hit.actor == 0 {
        return None;
    }
    check(world, t, me, hit.actor).map(|r| (hit.actor, r))
}

/// The side of `target` that `pos` is on: whichever of its forward and right axes `pos` lies
/// further along decides front/back or left/right.
pub fn side_of(target: &PawnInfo, pos: Vec3) -> Side {
    let (fwd, right) = yaw_axes(target.yaw);
    let d = pos - target.center;
    let (x, y) = (d.dot(fwd), d.dot(right));
    if x.abs() <= y.abs() {
        if y > 0.0 {
            Side::Right
        } else {
            Side::Left
        }
    } else if x >= 0.0 {
        Side::Front
    } else {
        Side::Back
    }
}

/// Where the player's feet go for a kill from `side`, and the yaw that faces the target.
pub fn landing(target: &PawnInfo, side: &DropSide) -> (Vec3, f32) {
    let (fwd, right) = yaw_axes(target.yaw);
    let base = Vec3::new(target.center.x, target.center.y, target.floor_z);
    let feet = base + fwd * side.anchor.x + right * side.anchor.y + Vec3::Z * side.anchor.z;
    let to = base - feet;
    let yaw = if to.truncate().length() > 1.0 { to.y.atan2(to.x) } else { target.yaw };
    (feet, yaw)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakedownKind {
    Drop,
    Assassination { fast: bool, generic: bool },
}

/// A ground assassination that started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Assassination {
    pub target: u32,
    pub side: Side,
    pub fast: bool,
    /// The plain kill: the victim stays where it is.
    pub generic: bool,
    /// Where the host should put the victim (feet) and which way it should face, for the paired kill.
    pub victim_feet: Vec3,
    pub victim_yaw: f32,
}

/// A takedown under way.
#[derive(Clone, Debug)]
pub struct TakedownRun {
    pub kind: TakedownKind,
    /// The player's animation.
    pub anim: String,
    pub target: u32,
    pub side: Side,
    /// Seconds since the kill started, and its length.
    pub t: f32,
    pub duration: f32,
    /// The eye position just before the player was moved to the target's side, so hosts can ease
    /// the camera across.
    pub from_eye: Vec3,
}

/// Takedown state the host can read (prompts, lock-on, the kill).
#[derive(Clone, Debug, Default)]
pub struct Takedown {
    /// A drop-assassination target attacking would take this frame, and whether it is in range.
    pub prompt: Option<(u32, Reach)>,
    /// A character attacking would assassinate this frame.
    pub assassinate: Option<u32>,
    /// Locked on, plunging toward this target.
    pub diving: Option<u32>,
    pub run: Option<TakedownRun>,
    pacing: Pacing,
}

/// The slow kill is the special one: after it come a few fast ones, and some time must pass.
#[derive(Clone, Debug, Default)]
struct Pacing {
    clock: f32,
    fast_left: i32,
    slow_timer: f32,
    last_slow: f32,
    seen_fast: bool,
    rng: u32,
}

impl Takedown {
    pub(crate) fn tick(&mut self, dt: f32) {
        self.pacing.clock += dt;
    }

    /// Whether this assassination is the slow one, and moves the pacing on.
    pub(crate) fn next_is_slow(&mut self, t: &AssassinateTuning) -> bool {
        let p = &mut self.pacing;
        if p.rng == 0 {
            p.rng = 0x2545_F491;
        }
        let slow = p.fast_left <= 0 || (p.clock - p.last_slow >= p.slow_timer && p.seen_fast);
        let mut roll = || {
            p.rng ^= p.rng << 13;
            p.rng ^= p.rng >> 17;
            p.rng ^= p.rng << 5;
            p.rng as f32 / u32::MAX as f32
        };
        if slow {
            let (lo, hi) = t.finishers_before_slow;
            let count = lo as f32 + roll() * (hi.saturating_sub(lo)) as f32;
            let time = t.time_before_slow.0 + roll() * (t.time_before_slow.1 - t.time_before_slow.0);
            p.fast_left = count.round() as i32;
            p.slow_timer = time;
            p.seen_fast = false;
            p.last_slow = p.clock;
        } else {
            p.fast_left -= 1;
            p.seen_fast = true;
        }
        slow
    }
}

/// Whether `info` may be assassinated: its awareness allows it, and it isn't running (unless
/// runners are allowed).
pub(crate) fn can_assassinate(t: &AssassinateTuning, info: &PawnInfo) -> bool {
    t.on_awareness[info.awareness as usize] && (!info.running || t.can_assassinate_runners)
}

/// Where the victim of a paired ground kill goes: in front of the player at the anchor's
/// distance, turned so that the anchor (the player's place in its frame) lies on the player.
pub fn victim_placement(player_feet: Vec3, player_yaw: f32, kill: &DropSide) -> (Vec3, f32) {
    let (fwd, _) = yaw_axes(player_yaw);
    let a = kill.anchor;
    let dist = a.truncate().length();
    let yaw = (-fwd.y).atan2(-fwd.x) - a.y.atan2(a.x);
    (player_feet + fwd * dist - Vec3::Z * a.z, yaw)
}
