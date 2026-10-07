//! Environment textures for dressing a test level, read from the install's `DishonoredGame.upk`
//! (street cobbles, modular rock, wood planks; diffuse and normal maps).

use std::path::Path;
use upk::texture::Rgba;
use upk::Package;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Cobbles,
    Rock,
    Planks,
}

pub struct SurfaceTexture {
    pub diffuse: Rgba,
    pub normal: Option<Rgba>,
}

#[derive(Default)]
pub struct Surfaces {
    pub textures: Vec<(Surface, SurfaceTexture)>,
    pub warnings: Vec<String>,
}

impl Surfaces {
    pub fn get(&self, s: Surface) -> Option<&SurfaceTexture> {
        self.textures.iter().find(|(k, _)| *k == s).map(|(_, t)| t)
    }
}

const MAX_SIZE: u32 = 1024;

pub fn load_surfaces(install: &Path) -> Surfaces {
    let mut out = Surfaces::default();
    let dir = install.join("DishonoredGame/CookedPCConsole");
    let pkg = match Package::open(dir.join("DishonoredGame.upk")) {
        Ok(p) => p,
        Err(e) => {
            out.warnings.push(format!("DishonoredGame.upk: {e}"));
            return out;
        }
    };
    let tex = |path: &str| pkg.find_export(path).and_then(|e| pkg.texture_rgba(e, MAX_SIZE, Some(&dir)).ok());
    for (s, d, n) in [
        (Surface::Cobbles, "grounds.street_cobbles_01_d", "grounds.street_cobbles_01_n"),
        (Surface::Rock, "modular_rocks.modular_rock_01_d", "modular_rocks.modular_rock_01_n"),
        (Surface::Planks, "wood_plank_01.wood_plank_01_d", "wood_plank_01.wood_plank_01_n"),
    ] {
        match tex(d) {
            Some(diffuse) => out.textures.push((s, SurfaceTexture { diffuse, normal: tex(n) })),
            None => out.warnings.push(format!("texture {d} not loaded")),
        }
    }
    out
}
