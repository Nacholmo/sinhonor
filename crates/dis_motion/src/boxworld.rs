//! A simple [`World`] made of axis-aligned boxes, water volumes and ladders. Good for tests,
//! prototypes and blockout levels; real hosts implement [`World`] over their own physics.

use crate::{Awareness, Hit, Ladder, PawnInfo, Vec3, Water, World};

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min: min.min(max), max: min.max(max) }
    }
    pub fn from_center(center: Vec3, half: Vec3) -> Self {
        Self::new(center - half, center + half)
    }
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.cmplt(o.max).all() && self.max.cmpgt(o.min).all()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub aabb: Aabb,
    pub actor: u32,
    pub is_pawn: bool,
}

#[derive(Clone, Debug, Default)]
pub struct BoxWorld {
    pub solids: Vec<Solid>,
    pub water: Vec<Aabb>,
    pub ladders: Vec<(Aabb, Vec3)>,
    pub blink_blockers: Vec<Aabb>,
    /// Characters that can be taken down, by actor id.
    pub characters: Vec<(u32, PawnInfo)>,
}

/// A box has no skeleton, so a box character's torso is taken at this fraction of its height.
pub const BOX_TORSO_HEIGHT: f32 = 0.7;

const EPS: f32 = 1e-4;

impl BoxWorld {
    pub fn add_box(&mut self, min: Vec3, max: Vec3) -> &mut Self {
        self.solids.push(Solid { aabb: Aabb::new(min, max), actor: 0, is_pawn: false });
        self
    }
    pub fn add_pawn(&mut self, center: Vec3, half: Vec3, actor: u32) -> &mut Self {
        self.solids.push(Solid { aabb: Aabb::from_center(center, half), actor, is_pawn: true });
        self
    }
    /// A pawn that can also be fought and taken down, facing `yaw` (radians).
    pub fn add_character(&mut self, center: Vec3, half: Vec3, actor: u32, yaw: f32, health: f32) -> &mut Self {
        let floor_z = center.z - half.z;
        let info = PawnInfo { center, floor_z, torso_z: floor_z + 2.0 * half.z * BOX_TORSO_HEIGHT, yaw, health, awareness: Awareness::Unaware, running: false };
        self.characters.push((actor, info));
        self.add_pawn(center, half, actor)
    }
    /// Sets a character's health (after the host applies damage).
    pub fn set_health(&mut self, actor: u32, health: f32) -> &mut Self {
        if let Some((_, p)) = self.characters.iter_mut().find(|(a, _)| *a == actor) {
            p.health = health;
        }
        self
    }
    /// Sets a character's awareness of the player.
    pub fn set_awareness(&mut self, actor: u32, awareness: Awareness) -> &mut Self {
        if let Some((_, p)) = self.characters.iter_mut().find(|(a, _)| *a == actor) {
            p.awareness = awareness;
        }
        self
    }
    /// Moves a character (its feet to `feet`, facing `yaw`).
    pub fn place_character(&mut self, actor: u32, feet: Vec3, yaw: f32) -> &mut Self {
        let Some((_, p)) = self.characters.iter_mut().find(|(a, _)| *a == actor) else { return self };
        let d = Vec3::new(feet.x - p.center.x, feet.y - p.center.y, feet.z - p.floor_z);
        p.center += d;
        p.floor_z += d.z;
        p.torso_z += d.z;
        p.yaw = yaw;
        for s in self.solids.iter_mut().filter(|s| s.actor == actor) {
            s.aabb.min += d;
            s.aabb.max += d;
        }
        self
    }
    /// Removes an actor's collision and character entry (e.g. once it is dead).
    pub fn remove_actor(&mut self, actor: u32) -> &mut Self {
        self.solids.retain(|s| s.actor != actor);
        self.characters.retain(|(a, _)| *a != actor);
        self
    }
    pub fn add_water(&mut self, min: Vec3, max: Vec3) -> &mut Self {
        self.water.push(Aabb::new(min, max));
        self
    }
    /// `normal` points from the ladder toward the climber.
    pub fn add_ladder(&mut self, min: Vec3, max: Vec3, normal: Vec3) -> &mut Self {
        self.ladders.push((Aabb::new(min, max), normal.normalize()));
        self
    }

    fn sweep_one(b: &Aabb, start: Vec3, end: Vec3, half: Vec3) -> Option<(f32, Vec3, bool)> {
        let min = b.min - half;
        let max = b.max + half;
        let inside = start.cmpgt(min + EPS).all() && start.cmplt(max - EPS).all();
        if inside {
            // Push-out normal along the axis of least penetration.
            let d_min = start - min;
            let d_max = max - start;
            let mut best = (f32::MAX, Vec3::ZERO);
            for (i, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                if d_min[i] < best.0 {
                    best = (d_min[i], -axis);
                }
                if d_max[i] < best.0 {
                    best = (d_max[i], axis);
                }
            }
            return Some((0.0, best.1, true));
        }
        let d = end - start;
        let mut t_enter = 0.0f32;
        let mut t_exit = 1.0f32;
        let mut normal = Vec3::ZERO;
        for i in 0..3 {
            if d[i].abs() < 1e-8 {
                if start[i] <= min[i] || start[i] >= max[i] {
                    return None;
                }
                continue;
            }
            let inv = 1.0 / d[i];
            let (mut t0, mut t1) = ((min[i] - start[i]) * inv, (max[i] - start[i]) * inv);
            let mut n = Vec3::ZERO;
            n[i] = -d[i].signum();
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            if t0 > t_enter {
                t_enter = t0;
                normal = n;
            }
            t_exit = t_exit.min(t1);
            if t_enter > t_exit {
                return None;
            }
        }
        if normal == Vec3::ZERO || t_enter > 1.0 {
            return None;
        }
        Some((t_enter, normal, false))
    }
}

impl World for BoxWorld {
    fn sweep(&self, start: Vec3, end: Vec3, half: Vec3) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for s in &self.solids {
            if let Some((time, normal, pen)) = Self::sweep_one(&s.aabb, start, end, half) {
                // Ignore surfaces we are moving away from (lets boxes slide off touching faces).
                if !pen && (end - start).dot(normal) >= 0.0 {
                    continue;
                }
                if best.is_none_or(|b| time < b.time) {
                    best = Some(Hit {
                        time,
                        location: start + (end - start) * time,
                        normal,
                        start_penetrating: pen,
                        actor: s.actor,
                        is_pawn: s.is_pawn,
                    });
                }
            }
        }
        best
    }

    fn overlaps(&self, center: Vec3, half: Vec3) -> bool {
        let b = Aabb::from_center(center, half - Vec3::splat(0.01));
        self.solids.iter().any(|s| s.aabb.overlaps(&b))
    }

    fn water(&self, point: Vec3) -> Option<Water> {
        self.water.iter().find(|w| w.contains(point)).map(|w| Water { surface_z: w.max.z })
    }

    fn ladder(&self, center: Vec3, half: Vec3) -> Option<Ladder> {
        let b = Aabb::from_center(center, half);
        self.ladders
            .iter()
            .find(|(a, _)| a.overlaps(&b))
            .map(|(a, n)| Ladder { normal: *n, bottom_z: a.min.z, top_z: a.max.z })
    }

    fn blink_blocked(&self, point: Vec3) -> bool {
        self.blink_blockers.iter().any(|b| b.contains(point))
    }

    fn pawn(&self, actor: u32) -> Option<PawnInfo> {
        self.characters.iter().find(|(a, _)| *a == actor).map(|(_, p)| *p)
    }
}
