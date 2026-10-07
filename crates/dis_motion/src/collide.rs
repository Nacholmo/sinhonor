//! Kinematic collision helpers on top of [`World::sweep`]: move-and-slide, step-up and floor
//! finding, in the spirit of UE3's `MoveActor` / `stepUp` / floor checks.

use crate::{Hit, Vec3, World};

/// Distance kept from surfaces after a blocked move.
pub const SKIN: f32 = 0.25;
const MAX_SLIDES: usize = 4;

/// Moves a box by `delta`, sliding along anything it hits. Returns the hits in order.
pub fn move_slide(world: &dyn World, pos: &mut Vec3, delta: Vec3, half: Vec3) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut remaining = delta;
    let mut prev_normal: Option<Vec3> = None;
    for _ in 0..MAX_SLIDES {
        if remaining.length_squared() < 1e-6 {
            break;
        }
        match world.sweep(*pos, *pos + remaining, half) {
            None => {
                *pos += remaining;
                break;
            }
            Some(h) => {
                let len = remaining.length();
                let dir = remaining / len;
                if h.start_penetrating {
                    // Push out along the normal and stop this frame's move.
                    *pos += h.normal * SKIN;
                    hits.push(h);
                    break;
                }
                let travel = (len * h.time - SKIN).max(0.0);
                *pos += dir * travel;
                hits.push(h);
                remaining *= 1.0 - h.time;
                remaining -= h.normal * remaining.dot(h.normal);
                // Two opposing walls: slide along the crease between them.
                if let Some(n0) = prev_normal {
                    if n0.dot(h.normal) < 0.0 {
                        let crease = n0.cross(h.normal).normalize_or_zero();
                        remaining = crease * remaining.dot(crease);
                    }
                }
                prev_normal = Some(h.normal);
            }
        }
    }
    hits
}

/// Looks for a floor under the box within `dist`. Returns the hit if it's walkable.
pub fn find_floor(world: &dyn World, pos: Vec3, half: Vec3, dist: f32, walkable_z: f32) -> Option<Hit> {
    let h = world.sweep(pos, pos - Vec3::Z * dist, half)?;
    (h.normal.z >= walkable_z && !h.start_penetrating).then_some(h)
}

/// UE3-style step-up: lift by `step`, move horizontally, then settle down onto a walkable floor.
/// Returns true and updates `pos` if the step succeeded and made progress.
pub fn step_up(world: &dyn World, pos: &mut Vec3, horizontal: Vec3, half: Vec3, step: f32, walkable_z: f32) -> bool {
    if horizontal.length_squared() < 1e-4 {
        return false;
    }
    let start = *pos;
    let mut p = start;
    let up_hits = move_slide(world, &mut p, Vec3::Z * step, half);
    let lifted = p.z - start.z;
    if lifted < 1.0 && !up_hits.is_empty() {
        return false;
    }
    let fwd_hits = move_slide(world, &mut p, horizontal, half);
    if fwd_hits.iter().any(|h| h.normal.z < walkable_z && h.time < 1e-3) && (p - start).truncate().length() < 0.5 {
        return false;
    }
    match world.sweep(p, p - Vec3::Z * (lifted + step), half) {
        Some(h) if h.normal.z >= walkable_z && !h.start_penetrating => {
            p = h.location + Vec3::Z * SKIN;
        }
        _ => return false,
    }
    if (p - start).truncate().length() < 0.5 {
        return false;
    }
    *pos = p;
    true
}
