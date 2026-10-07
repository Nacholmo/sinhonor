//! Motion and Blink sound cues, loaded from the install's Wwise packages and converted to Ogg.
//!
//! Cue -> event-name mapping is ours (which game event best fits each motion moment); the Blink
//! events come from the Blink tweak itself. The audio is read from the user's install at runtime.

use std::collections::HashMap;
use std::path::Path;
use wwise::{Package, PlayNode};

/// Ground surface, matching the game's footstep event families (`FS_P_<Surface>_...`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Stone,
    Wood,
    Metal,
    Rooftile,
    Gravel,
    Water,
}

impl Surface {
    pub const ALL: [Surface; 6] = [Surface::Stone, Surface::Wood, Surface::Metal, Surface::Rooftile, Surface::Gravel, Surface::Water];
    fn event_name(self) -> &'static str {
        match self {
            Surface::Stone => "Stone",
            Surface::Wood => "Wood",
            Surface::Metal => "Metal",
            Surface::Rooftile => "Rooftile",
            Surface::Gravel => "Gravel",
            Surface::Water => "Water",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gait {
    Sneak,
    Run,
    Sprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Footstep(Surface, Gait),
    LandSmall(Surface),
    LandHigh(Surface),
    Jump,
    MantleLow,
    MantleMedium,
    MantleHigh,
    MantleImpact,
    Crouch,
    Stand,
    Slide,
    WaterEnter,
    Swim,
    FallWind,
    SprintBreath,
    BlinkWarmup,
    BlinkRelease,
    BlinkFizzle,
}

/// Game event names for the Blink cues, as referenced by `Twk_Blink`.
#[derive(Clone, Debug, Default)]
pub struct BlinkSoundEvents {
    pub warmup: Option<String>,
    pub blink: Option<String>,
    pub fizzle: Option<String>,
}

#[derive(Default)]
pub struct Sounds {
    pub cues: HashMap<Cue, PlayNode>,
    /// Ogg Vorbis bytes per media id.
    pub ogg: HashMap<u32, Vec<u8>>,
    pub warnings: Vec<String>,
}

impl Sounds {
    pub fn get(&self, cue: Cue) -> Option<&PlayNode> {
        self.cues.get(&cue)
    }
}

const PACKAGES: [&str; 4] = ["Bank_Footsteps.pck", "Bank_Player.pck", "Bank_Power_Player.pck", "Bank_UI_Ingame_Water.pck"];

pub fn load_sounds(install: &Path, blink: &BlinkSoundEvents) -> Sounds {
    let mut out = Sounds::default();
    let dir = install.join("DishonoredGame/CookedPCConsole");
    let mut packages = Vec::new();
    for p in PACKAGES {
        match Package::open(dir.join(p)) {
            Ok(pkg) => packages.push(pkg),
            Err(e) => out.warnings.push(format!("{p}: {e}")),
        }
    }

    let mut wanted: Vec<(Cue, String)> = Vec::new();
    for s in Surface::ALL {
        let n = s.event_name();
        wanted.push((Cue::Footstep(s, Gait::Sneak), format!("FS_P_{n}_Sn")));
        wanted.push((Cue::Footstep(s, Gait::Run), format!("FS_P_{n}_R")));
        wanted.push((Cue::Footstep(s, Gait::Sprint), format!("FS_P_{n}_Sp")));
        wanted.push((Cue::LandSmall(s), format!("FS_P_{n}_Fall_Small")));
        wanted.push((Cue::LandHigh(s), format!("FS_P_{n}_Fall_High")));
    }
    for (cue, name) in [
        (Cue::Jump, "Snd_P_Clothes_Mvt_05_Cue_ak"),
        (Cue::MantleLow, "Snd_P_Mantle_Low_cue_ak"),
        (Cue::MantleMedium, "Snd_P_Mantle_Medium_cue_ak"),
        (Cue::MantleHigh, "Snd_P_Mantle_High_cue_ak"),
        (Cue::MantleImpact, "Snd_P_Mantle_Impact_cue_ak"),
        (Cue::Crouch, "Snd_P_Crouching_01_Cue_ak"),
        (Cue::Stand, "Snd_P_Stand_Up_01_Cue_ak"),
        (Cue::Slide, "FS_P_Slide_Generic"),
        (Cue::WaterEnter, "FS_P_Water_Fall_High"),
        (Cue::Swim, "Snd_P_Swim"),
        (Cue::FallWind, "Snd_P_Fall_Wind"),
        (Cue::SprintBreath, "Snd_P_Sprint_Breath"),
    ] {
        wanted.push((cue, name.to_string()));
    }
    for (cue, name) in [(Cue::BlinkWarmup, &blink.warmup), (Cue::BlinkRelease, &blink.blink), (Cue::BlinkFizzle, &blink.fizzle)] {
        match name {
            Some(n) => wanted.push((cue, n.clone())),
            None => out.warnings.push(format!("{cue:?}: no event referenced by the Blink tweak")),
        }
    }

    for (cue, name) in wanted {
        let Some((pkg, node)) = packages.iter().find_map(|p| p.event(&name).map(|n| (p, n))) else {
            // Not every surface has every variant (e.g. no deep-water fall); that's expected.
            if !matches!(cue, Cue::Footstep(..) | Cue::LandSmall(..) | Cue::LandHigh(..)) {
                out.warnings.push(format!("sound event {name} not found"));
            }
            continue;
        };
        for id in node.clips() {
            if out.ogg.contains_key(&id) {
                continue;
            }
            match pkg.ogg(id) {
                Ok(o) => {
                    out.ogg.insert(id, o);
                }
                Err(e) => out.warnings.push(format!("{name}: clip {id:#x}: {e}")),
            }
        }
        out.cues.insert(cue, node);
    }
    out
}
