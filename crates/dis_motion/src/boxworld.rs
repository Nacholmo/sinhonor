//! A simple [`World`] made of axis-aligned boxes, water volumes and ladders. Good for tests,
//! prototypes and blockout levels; real hosts implement [`World`] over their own physics.

use crate::{Hit, Ladder, Vec3, Water, World};

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
}

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
}
