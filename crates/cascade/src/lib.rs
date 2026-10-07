//! A small runtime for Unreal Engine 3 "Cascade" particle systems, loaded from Dishonored's
//! cooked packages at runtime and simulated on the CPU, engine-agnostic.
//!
//! Cooked distributions are baked into `LookupTable` float arrays (`FRawDistribution`): two
//! header floats (overall min/max), then samples spaced `1/LookupTableTimeScale` apart in
//! normalised particle (or emitter) time. A sample holds `LookupTableNumElements` values
//! (2 = a min/max pair, picked at random per particle) of 1 (float) or 3 (vector) components,
//! `LookupTableChunkSize` floats in all. Only LOD 0 and the modules Dishonored's motion and Blink
//! effects use are implemented; see NOTES.md for the list and the material model.

mod load;
mod sim;

pub use load::{load_system, MaterialLoader};
pub use sim::{Instance, RenderParticle};

use glam::Vec3;
use std::sync::Arc;

/// A baked distribution (float or vector).
#[derive(Clone, Debug, Default)]
pub struct Dist {
    pub values: Vec<f32>,
    pub elements: usize,
    pub chunk: usize,
    pub time_scale: f32,
    pub start_time: f32,
}

impl Dist {
    pub fn is_empty(&self) -> bool {
        self.values.len() <= 2
    }

    fn samples(&self) -> usize {
        (self.values.len().saturating_sub(2)) / self.chunk.max(1)
    }

    /// Component `c` of `n`, at normalised time `t`, with per-particle random `r` in 0..1.
    fn component(&self, t: f32, r: f32, c: usize, n: usize) -> f32 {
        let count = self.samples();
        if count == 0 {
            return 0.0;
        }
        let at = |i: usize| {
            let base = 2 + i * self.chunk;
            let lo = self.values.get(base + c).copied().unwrap_or(0.0);
            if self.elements >= 2 {
                let hi = self.values.get(base + n + c).copied().unwrap_or(lo);
                lo + (hi - lo) * r
            } else {
                lo
            }
        };
        if self.time_scale <= 0.0 || count == 1 {
            return at(0);
        }
        let f = ((t - self.start_time) * self.time_scale).clamp(0.0, (count - 1) as f32);
        let i = f.floor() as usize;
        let j = (i + 1).min(count - 1);
        let a = f - i as f32;
        at(i) * (1.0 - a) + at(j) * a
    }

    pub fn float(&self, t: f32, r: f32) -> f32 {
        self.component(t, r, 0, 1)
    }

    pub fn vector(&self, t: f32, r: Vec3) -> Vec3 {
        Vec3::new(self.component(t, r.x, 0, 3), self.component(t, r.y, 1, 3), self.component(t, r.z, 2, 3))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Additive,
    Translucent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    /// Camera-facing square.
    Square,
    /// Stretched along the particle's velocity.
    Velocity,
    /// Facing a fixed axis (Cascade's axis lock), in emitter space.
    Axis(Vec3Axis),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vec3Axis {
    X,
    Y,
    Z,
    NegX,
    NegY,
    NegZ,
}

impl Vec3Axis {
    pub fn vec(self) -> Vec3 {
        match self {
            Vec3Axis::X => Vec3::X,
            Vec3Axis::Y => Vec3::Y,
            Vec3Axis::Z => Vec3::Z,
            Vec3Axis::NegX => -Vec3::X,
            Vec3Axis::NegY => -Vec3::Y,
            Vec3Axis::NegZ => -Vec3::Z,
        }
    }
}

/// A sprite material: an RGBA image (RGB colour, A coverage) plus how it blends.
#[derive(Clone, Debug)]
pub struct SpriteMaterial {
    pub name: String,
    pub blend: Blend,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Constant tint from the material instance (vector parameter), multiplied with particle colour.
    pub tint: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Burst {
    pub count: u32,
    pub count_low: Option<u32>,
    pub time: f32,
}

#[derive(Clone, Debug, Default)]
pub struct EmitterDef {
    pub name: String,
    pub material: Option<Arc<SpriteMaterial>>,
    pub alignment: Option<Alignment>,
    pub local_space: bool,
    pub kill_on_deactivate: bool,
    pub duration: f32,
    pub loops: u32,
    pub delay: f32,
    pub spawn_rate: Dist,
    pub spawn_rate_scale: Option<Dist>,
    pub bursts: Vec<Burst>,
    pub lifetime: Dist,
    pub start_size: Dist,
    pub size_mult_life: Option<Dist>,
    pub size_mult_velocity: Option<Dist>,
    pub start_color: Option<Dist>,
    pub start_alpha: Option<Dist>,
    pub color_over_life: Option<Dist>,
    pub alpha_over_life: Option<Dist>,
    pub color_scale_over_life: Option<Dist>,
    pub alpha_scale_over_life: Option<Dist>,
    pub start_velocity: Dist,
    pub start_velocity_radial: Dist,
    pub velocity_world_space: bool,
    pub velocity_over_life: Option<Dist>,
    pub acceleration: Dist,
    pub start_location: Dist,
    pub cylinder: Option<Cylinder>,
    pub start_rotation: Dist,
    pub rotation_rate: Dist,
    pub rotation_rate_mult_life: Option<Dist>,
    /// Mesh emitters are drawn as sprites (no mesh import yet).
    pub is_mesh: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Cylinder {
    pub radius: Dist,
    pub height: Dist,
    pub surface_only: bool,
    pub velocity: bool,
    pub velocity_scale: Dist,
}

#[derive(Clone, Debug, Default)]
pub struct SystemDef {
    pub name: String,
    /// Emitters at LOD 0 (the same as `lods[0]`).
    pub emitters: Vec<EmitterDef>,
    /// Emitters per LOD level; indices line up across levels.
    pub lods: Vec<Vec<EmitterDef>>,
    /// Camera distance at which each LOD level starts (`LODDistances`).
    pub lod_distances: Vec<f32>,
    pub warmup_time: f32,
}

impl SystemDef {
    /// The LOD level the engine would use at `distance` from the camera.
    pub fn lod_for_distance(&self, distance: f32) -> usize {
        let n = self.lods.len().max(1);
        self.lod_distances.iter().take(n).rposition(|&d| distance >= d).unwrap_or(0)
    }
}
