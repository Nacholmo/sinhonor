//! Motion and Blink particle effects, loaded from the install's cooked packages.
//!
//! The Blink markers and arrival lens effect are the ones `Twk_Blink` references; the rest are
//! the game's physical-material effects for footsteps, slides, landings and water.

use cascade::{load_system, MaterialLoader, SystemDef};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use upk::{lookup, ObjRef, Package, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    /// Targeting marker when the target is on the ground.
    BlinkGround,
    /// Targeting marker when the target is in the air (the game uses it for low and high drops).
    BlinkFall,
    /// Targeting marker when the blink ends on a ledge.
    BlinkMantle,
    /// Camera-lens effect played when a blink lands.
    BlinkArriveLens,
    SlideStone,
    SlideGeneric,
    LandDirt,
    StepGravel,
    StepDirt,
    StepWater,
    WaterSplash,
    Swimming,
    /// Camera-lens droplets after leaving water.
    CameraWater,
}

#[derive(Default)]
pub struct Effects {
    pub systems: HashMap<Fx, Arc<SystemDef>>,
    /// Distance in front of the camera for lens effects (`EmitterCameraLensEffectBase`).
    pub lens_distance: f32,
    pub warnings: Vec<String>,
}

pub fn load_effects(install: &Path) -> Effects {
    let mut out = Effects { lens_distance: 90.0, ..Default::default() };
    let dir = install.join("DishonoredGame/CookedPCConsole");
    let open = |n: &str| Package::open(dir.join(n));
    let (game, startup, engine) = match (open("DishonoredGame.upk"), open("Startup.upk"), open("Engine.upk")) {
        (Ok(g), Ok(s), Ok(e)) => (g, s, e),
        _ => {
            out.warnings.push("could not open the effect packages".into());
            return out;
        }
    };
    if let Some(d) = engine
        .find_export("Default__EmitterCameraLensEffectBase")
        .and_then(|i| engine.properties(i).ok())
        .and_then(|p| lookup(&p, "DistFromCamera").and_then(Value::as_f32))
    {
        out.lens_distance = d;
    }
    let loader = MaterialLoader::new(Some(dir.clone())).with_engine_defaults(engine);

    // Paths the Blink tweak references.
    let mut blink_paths: Vec<(Fx, String)> = Vec::new();
    if let Some(twk) = game.find_export("Twk_Powers.Blink.Twk_Blink").and_then(|i| game.properties(i).ok()) {
        let path_of = |field: &str| match lookup(&twk, field) {
            Some(Value::Object(i)) if *i != 0 => Some(game.object_path(ObjRef::from_index(*i))),
            _ => None,
        };
        for (fx, field) in [(Fx::BlinkGround, "m_pGroundPS"), (Fx::BlinkFall, "m_pLowFallPS"), (Fx::BlinkMantle, "m_pMantlePS")] {
            match path_of(field) {
                Some(p) => blink_paths.push((fx, p)),
                None => out.warnings.push(format!("Twk_Blink.{field} not found")),
            }
        }
        // m_pCooldownEffect -> DisTweaks_EmitterCameraLensEffect -> m_DefaultParticleSystem.
        if let Some(Value::Object(i)) = lookup(&twk, "m_pCooldownEffect") {
            if let ObjRef::Export(e) = ObjRef::from_index(*i) {
                if let Some(Value::Object(ps)) = game.properties(e).ok().as_deref().and_then(|p| lookup(p, "m_DefaultParticleSystem")) {
                    blink_paths.push((Fx::BlinkArriveLens, game.object_path(ObjRef::from_index(*ps))));
                }
            }
        }
    } else {
        out.warnings.push("Twk_Blink not found".into());
    }

    let mut wanted: Vec<(Fx, &Package, String)> = blink_paths.into_iter().map(|(f, p)| (f, &game, p)).collect();
    for (fx, path) in [
        (Fx::SlideStone, "Vfx_PhysMat.Slide.PS_Slide_Stone"),
        (Fx::SlideGeneric, "Vfx_PhysMat.Slide.PS_Slide_Generic"),
        (Fx::LandDirt, "Vfx_PhysMat.Dirt.Ps_JumpLand_Big_DirtGravel"),
        (Fx::StepGravel, "Vfx_PhysMat.FootSteps.Ps_fsteps_Gravel_RunPs_fsteps_Dirt_Run"),
        (Fx::StepDirt, "Vfx_PhysMat.FootSteps.Ps_fsteps_Dirt_RunPs_fsteps_Dirt_Run"),
        (Fx::StepWater, "Vfx_PhysMat.FootSteps.Ps_fsteps_Water_RunPs_fsteps_Dirt_Run"),
        (Fx::WaterSplash, "Vfx_PhysMat.Water.Pfx_WaterSplash"),
    ] {
        wanted.push((fx, &game, path.to_string()));
    }
    for (fx, path) in [(Fx::Swimming, "Vfx_Water.Effects.Player_Swimming"), (Fx::CameraWater, "Vfx_PhysMat.Water.ps_camera_water")] {
        wanted.push((fx, &startup, path.to_string()));
    }
    for (fx, pkg, path) in wanted {
        match load_system(pkg, &path, &loader) {
            Some(def) if !def.emitters.is_empty() => {
                out.systems.insert(fx, Arc::new(def));
            }
            _ => out.warnings.push(format!("particle system {path} not loaded")),
        }
    }
    out
}
