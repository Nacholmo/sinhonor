//! Engine-agnostic Dishonored-style player motion and Blink.
//!
//! The host game owns rendering, input devices and the world; this crate owns the player's
//! movement state. Hosts implement [`World`] (box sweeps plus optional water and ladder
//! queries), feed an [`Input`] each frame, and read back position, view and the camera-feel
//! values. Coordinates follow Unreal conventions (Z up, X forward at yaw 0, Y right, units ~cm)
//! so tuning read from the game applies unchanged; hosts convert at the boundary.
//!
//! Behaviour sources are documented in `NOTES.md`: Blink follows the behaviour specified in
//! §5, the drop assassination §5b, the sword attack §5c and the ground assassination §5d; walking/falling follow UE3's character physics
//! fed with Dishonored's tuning; mantle, slide, lean, swim and ladder are modelled from the
//! game's tuning and animation timings.

mod blink;
pub mod boxworld;
mod camera;
mod collide;
mod controller;
mod melee;
mod takedown;
mod tuning;

pub use blink::{Blink, BlinkEvent, BlinkFx, BlinkMode, BlinkTarget};
pub use camera::CameraFeel;
pub use controller::{Input, MantleKind, Motion, MotionState, StepEvents};
pub use glam::{Vec2, Vec3};
pub use melee::{Melee, MeleeEvent, MeleeTuning, SwingAnim, SwingKind, SwingRun, SwingSet, CROSSHAIR_HALF_HEIGHT, SWEEP_HALF_HEIGHT};
pub use takedown::{landing, side_of, victim_placement, AssassinateTuning, Assassination, Awareness, DropAssassinateTuning, DropSide, PawnInfo, Reach, Side, Takedown, TakedownKind, TakedownRun, DIVE_SPEED_SCALE};
pub use tuning::*;

/// Result of a swept box query.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Fraction of the sweep travelled before contact, in `0..=1`.
    pub time: f32,
    /// Box centre at contact.
    pub location: Vec3,
    /// Surface normal at contact, pointing away from the surface.
    pub normal: Vec3,
    /// The box already overlapped something at the start of the sweep.
    pub start_penetrating: bool,
    /// Host-defined id of what was hit (0 = static world).
    pub actor: u32,
    /// What was hit is a character (Blink stops at characters and can knock them back).
    pub is_pawn: bool,
}

/// Water at a point: the surface height above it.
#[derive(Clone, Copy, Debug)]
pub struct Water {
    pub surface_z: f32,
}

/// A climbable ladder the player overlaps.
#[derive(Clone, Copy, Debug)]
pub struct Ladder {
    /// Horizontal normal pointing from the ladder towards where the climber stands.
    pub normal: Vec3,
    pub bottom_z: f32,
    pub top_z: f32,
}

/// The host's collision world.
pub trait World {
    /// Sweeps an axis-aligned box with half-extents `half` from `start` to `end` and returns
    /// the first blocking hit. A zero `half` is a line trace.
    fn sweep(&self, start: Vec3, end: Vec3, half: Vec3) -> Option<Hit>;

    /// True if a box at `center` overlaps blocking geometry.
    fn overlaps(&self, center: Vec3, half: Vec3) -> bool {
        self.sweep(center, center, half).is_some_and(|h| h.start_penetrating)
    }

    /// Water containing `point`, if any.
    fn water(&self, _point: Vec3) -> Option<Water> {
        None
    }

    /// A ladder volume overlapping the box, if any.
    fn ladder(&self, _center: Vec3, _half: Vec3) -> Option<Ladder> {
        None
    }

    /// True if Blink may not end inside this point (the game's blink-blocking volumes).
    fn blink_blocked(&self, _point: Vec3) -> bool {
        false
    }

    /// The character a pawn hit reported as `actor`, if it can be fought or taken down (alive,
    /// not ragdolled). Without it, drop assassinations find no one and no blow is a killing blow.
    fn pawn(&self, _actor: u32) -> Option<PawnInfo> {
        None
    }
}

/// Forward unit vector for a yaw/pitch in radians (Unreal axes).
pub fn view_dir(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), pitch.sin())
}

/// Horizontal forward and right unit vectors for a yaw.
pub fn yaw_axes(yaw: f32) -> (Vec3, Vec3) {
    (Vec3::new(yaw.cos(), yaw.sin(), 0.0), Vec3::new(-yaw.sin(), yaw.cos(), 0.0))
}
