//! Drop assassination: attacking while falling onto a character kills it from above, following
//! the behaviour specified in `NOTES.md` §5b. A target is looked for along the fall the player is
//! predicted to make; one found early is locked on to, and the player plunges straight down at it
//! until close enough, then lands at the side of the target they came from.
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
#[derive(Clone, Copy, Debug)]
pub struct DropSide {
    /// Where the player's feet go, in the target's frame (x forward, y right, z up, from its feet).
    pub anchor: Vec3,
    /// Length of the player's kill animation (seconds).
    pub duration: f32,
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

/// A drop assassination under way.
#[derive(Clone, Copy, Debug)]
pub struct TakedownRun {
    pub target: u32,
    pub side: Side,
    /// Seconds since the kill started, and its length.
    pub t: f32,
    pub duration: f32,
    /// The eye position just before the player was moved to the target's side, so hosts can ease
    /// the camera across.
    pub from_eye: Vec3,
}

/// Drop-assassination state the host can read (prompt, lock-on, the kill).
#[derive(Clone, Copy, Debug, Default)]
pub struct Takedown {
    /// A target attacking would take this frame, and whether it is already in range.
    pub prompt: Option<(u32, Reach)>,
    /// Locked on, plunging toward this target.
    pub diving: Option<u32>,
    pub run: Option<TakedownRun>,
}
