//! CPU simulation of a [`SystemDef`].

use crate::{EmitterDef, SystemDef};
use glam::{Affine3A, Vec2, Vec3};
use std::f32::consts::TAU;
use std::sync::Arc;

const WARMUP_STEP: f32 = 1.0 / 30.0;

#[derive(Clone, Debug)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size0: Vec3,
    rot: f32,
    rot_rate: f32,
    color0: Vec3,
    alpha0: f32,
    r: [f32; 8],
}

#[derive(Clone, Debug, Default)]
struct EmitterState {
    time: f32,
    spawn_frac: f32,
    loops_done: u32,
    done: bool,
    bursts_fired: Vec<bool>,
    particles: Vec<Particle>,
}

/// A particle ready to draw, in world space.
#[derive(Clone, Copy, Debug)]
pub struct RenderParticle {
    pub emitter: usize,
    pub pos: Vec3,
    pub size: Vec2,
    /// Sprite roll in radians.
    pub rotation: f32,
    pub color: [f32; 4],
    pub velocity: Vec3,
}

/// A running particle system.
pub struct Instance {
    pub def: Arc<SystemDef>,
    /// Emitter space to world space.
    pub transform: Affine3A,
    emitters: Vec<EmitterState>,
    active: bool,
    rng: u64,
}

impl Instance {
    pub fn new(def: Arc<SystemDef>, transform: Affine3A, seed: u64) -> Self {
        let emitters = def.emitters.iter().map(|e| EmitterState { bursts_fired: vec![false; e.bursts.len()], ..Default::default() }).collect();
        let mut inst = Self { transform, emitters, active: true, rng: seed | 1, def };
        let mut t = inst.def.warmup_time;
        while t > 0.0 {
            inst.update(WARMUP_STEP.min(t));
            t -= WARMUP_STEP;
        }
        inst
    }

    /// Stops spawning; emitters flagged kill-on-deactivate drop their particles.
    pub fn deactivate(&mut self) {
        self.active = false;
        for (e, s) in self.def.emitters.iter().zip(&mut self.emitters) {
            if e.kill_on_deactivate {
                s.particles.clear();
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// True once nothing will ever be drawn again.
    pub fn is_finished(&self) -> bool {
        self.emitters.iter().all(|s| s.particles.is_empty()) && (!self.active || self.emitters.iter().all(|s| s.done))
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn update(&mut self, dt: f32) {
        let def = self.def.clone();
        for (k, e) in def.emitters.iter().enumerate() {
            self.update_emitter(k, e, dt);
        }
    }

    fn update_emitter(&mut self, k: usize, e: &EmitterDef, dt: f32) {
        // Age and move existing particles.
        let transform = self.transform;
        {
            let s = &mut self.emitters[k];
            s.particles.retain_mut(|p| {
                p.age += dt;
                if p.age >= p.life {
                    return false;
                }
                let t = p.age / p.life;
                let mut acc = e.acceleration.vector(t, Vec3::new(p.r[5], p.r[6], p.r[7]));
                if e.local_space {
                    // Acceleration is authored in emitter space either way.
                } else {
                    acc = transform.transform_vector3(acc);
                }
                p.vel += acc * dt;
                let vel = match &e.velocity_over_life {
                    Some(d) => p.vel * d.vector(t, Vec3::splat(p.r[4])),
                    None => p.vel,
                };
                p.pos += vel * dt;
                let rate_mult = e.rotation_rate_mult_life.as_ref().map_or(1.0, |d| d.float(t, p.r[3]));
                p.rot += p.rot_rate * rate_mult * dt;
                true
            });
        }

        // Spawning.
        if !self.active || self.emitters[k].done {
            return;
        }
        let duration = e.duration.max(1e-3);
        let prev = self.emitters[k].time;
        self.emitters[k].time += dt;
        let time = self.emitters[k].time - e.delay;
        if time < 0.0 {
            return;
        }
        let local_t = (time % duration) / duration;
        let mut spawn = 0usize;
        // Rate spawning.
        let scale = e.spawn_rate_scale.as_ref().filter(|d| !d.is_empty()).map_or(1.0, |d| d.float(local_t, 0.5));
        let rate = e.spawn_rate.float(local_t, 0.5) * scale;
        if rate > 0.0 {
            let s = &mut self.emitters[k];
            s.spawn_frac += rate * dt;
            spawn += s.spawn_frac.floor() as usize;
            s.spawn_frac = s.spawn_frac.fract();
        }
        // Bursts, at normalised emitter time.
        let prev_local = ((prev - e.delay).max(0.0) % duration) / duration;
        let wrapped = local_t < prev_local || prev - e.delay < 0.0;
        if wrapped && prev - e.delay >= 0.0 {
            let s = &mut self.emitters[k];
            s.loops_done += 1;
            s.bursts_fired.iter_mut().for_each(|b| *b = false);
            if e.loops > 0 && s.loops_done >= e.loops {
                s.done = true;
                return;
            }
        }
        for (b, burst) in e.bursts.iter().enumerate() {
            if !self.emitters[k].bursts_fired[b] && local_t >= burst.time {
                self.emitters[k].bursts_fired[b] = true;
                let n = match burst.count_low {
                    Some(low) if low < burst.count => low + (self.rand() * (burst.count - low + 1) as f32) as u32,
                    _ => burst.count,
                };
                spawn += n as usize;
            }
        }
        for _ in 0..spawn.min(256) {
            let p = self.spawn(e, local_t);
            self.emitters[k].particles.push(p);
        }
    }

    fn spawn(&mut self, e: &EmitterDef, t: f32) -> Particle {
        let mut r = [0.0; 8];
        for v in r.iter_mut() {
            *v = self.rand();
        }
        let rv = |a: usize| Vec3::new(r[a % 8], r[(a + 1) % 8], r[(a + 2) % 8]);
        let life = e.lifetime.float(t, r[0]).max(1e-3);
        let size0 = e.start_size.vector(t, rv(1));
        let mut loc = e.start_location.vector(t, rv(4));
        let mut vel = e.start_velocity.vector(t, rv(2));
        if let Some(c) = &e.cylinder {
            let angle = self.rand() * TAU;
            let radius = c.radius.float(t, r[3]);
            let rad = if c.surface_only { radius } else { radius * self.rand().sqrt() };
            let offset = Vec3::new(angle.cos() * rad, angle.sin() * rad, (self.rand() - 0.5) * c.height.float(t, r[6]));
            loc += offset;
            if c.velocity {
                vel += offset.normalize_or_zero() * c.velocity_scale.float(t, r[7]);
            }
        }
        let radial = e.start_velocity_radial.float(t, r[5]);
        if radial != 0.0 {
            vel += loc.normalize_or_zero() * radial;
        }
        let (pos, vel) = if e.local_space {
            (loc, vel)
        } else {
            let wv = if e.velocity_world_space { vel } else { self.transform.transform_vector3(vel) };
            (self.transform.transform_point3(loc), wv)
        };
        Particle {
            pos,
            vel,
            age: 0.0,
            life,
            size0,
            rot: e.start_rotation.float(t, r[3]) * TAU,
            rot_rate: e.rotation_rate.float(t, r[4]) * TAU,
            color0: e.start_color.as_ref().map_or(Vec3::ONE, |d| d.vector(t, rv(5))),
            alpha0: e.start_alpha.as_ref().map_or(1.0, |d| d.float(t, r[6])),
            r,
        }
    }

    /// Visible particles, in world space.
    pub fn particles(&self) -> impl Iterator<Item = RenderParticle> + '_ {
        self.def.emitters.iter().zip(&self.emitters).enumerate().flat_map(move |(k, (e, s))| {
            s.particles.iter().map(move |p| {
                let t = p.age / p.life;
                let mut size = p.size0;
                if let Some(d) = &e.size_mult_life {
                    size *= d.vector(t, Vec3::splat(p.r[2]));
                }
                if let Some(d) = &e.size_mult_velocity {
                    let m = d.vector(t, Vec3::splat(p.r[3])) * p.vel.length();
                    size *= m.max(Vec3::ONE);
                }
                let mut color = e.color_over_life.as_ref().map_or(p.color0, |d| d.vector(t, Vec3::splat(p.r[1])));
                let mut alpha = e.alpha_over_life.as_ref().map_or(p.alpha0, |d| d.float(t, p.r[1]) * p.alpha0);
                if let Some(d) = &e.color_scale_over_life {
                    color *= d.vector(t, Vec3::splat(p.r[1]));
                }
                if let Some(d) = &e.alpha_scale_over_life {
                    alpha *= d.float(t, p.r[1]);
                }
                let (pos, velocity) = if e.local_space {
                    (self.transform.transform_point3(p.pos), self.transform.transform_vector3(p.vel))
                } else {
                    (p.pos, p.vel)
                };
                RenderParticle { emitter: k, pos, size: Vec2::new(size.x, size.y), rotation: p.rot, color: [color.x, color.y, color.z, alpha], velocity }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Burst, Dist};

    fn constant(v: &[f32]) -> Dist {
        let mut values = vec![0.0, 0.0];
        values.extend_from_slice(v);
        values.extend_from_slice(v);
        Dist { values, elements: 1, chunk: v.len(), time_scale: 0.0, start_time: 0.0 }
    }

    #[test]
    fn rate_and_bursts_spawn() {
        let e = EmitterDef {
            duration: 1.0,
            spawn_rate: constant(&[10.0]),
            bursts: vec![Burst { count: 5, count_low: None, time: 0.0 }],
            lifetime: constant(&[2.0]),
            start_size: constant(&[10.0, 10.0, 10.0]),
            ..Default::default()
        };
        let def = Arc::new(SystemDef { emitters: vec![e], ..Default::default() });
        let mut i = Instance::new(def, Affine3A::IDENTITY, 7);
        for _ in 0..10 {
            i.update(0.05);
        }
        let n = i.particles().count();
        assert!((9..=11).contains(&n), "5 burst + ~5 rate, got {n}");
    }

    #[test]
    fn uniform_range_samples_between_min_and_max() {
        let d = Dist { values: vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0], elements: 2, chunk: 2, time_scale: 0.0, start_time: 0.0 };
        assert_eq!(d.float(0.0, 0.0), 1.0);
        assert_eq!(d.float(0.0, 1.0), 2.0);
    }
}
