//! Optional integration check against the user's install. No game data is shipped as fixtures.
use cascade::{load_system, MaterialLoader};
use std::{path::PathBuf, sync::Arc};
use upk::Package;

#[test]
fn hand_imports_share_real_glow_material_and_blink_meshes_load() {
    let install = std::env::var_os("DISHONORED_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Dishonored"));
    let dir = install.join("DishonoredGame/CookedPCConsole");
    if !dir.join("DishonoredGame.upk").is_file() {
        return;
    }
    let game = Package::open(dir.join("DishonoredGame.upk")).unwrap();
    let startup = Package::open(dir.join("Startup.upk")).unwrap();
    let engine = Package::open(dir.join("Engine.upk")).unwrap();
    let loader = MaterialLoader::new(Some(dir)).with_engine_defaults(engine).with_package(&game);
    // Load the import first: a white fallback cached under this name poisoned later effects.
    let hand = load_system(&startup, "Vfx_GamePlay.Powers.Ps_Tattoo_Glow_02", &loader).unwrap();
    let ground = load_system(&game, "Vfx_GamePlay.Blink.Blink_Ground_01", &loader).unwrap();
    let glow = |system: &cascade::SystemDef| {
        system
            .emitters
            .iter()
            .filter_map(|e| e.material.as_ref())
            .find(|m| m.name.ends_with("Blink_Glow_02_INST"))
            .unwrap()
            .clone()
    };
    let h = glow(&hand);
    let g = glow(&ground);
    assert!(Arc::ptr_eq(&h, &g));
    assert!(h.tint[0] < h.tint[2], "import must retain the original blue material tint");
    assert!(hand.emitters.iter().all(|e| e.material.is_some()), "smoke material must resolve too");
    assert!(ground.emitters.iter().filter(|e| e.is_mesh).all(|e| e.mesh.is_some()));
    assert!(ground.emitters.iter().filter_map(|e| e.mesh.as_ref()).any(|m| !m.vertices.is_empty() && !m.indices.is_empty()));
    assert!(hand.emitters.iter().filter(|e| e.alignment.is_some()).all(|e| e.square));
}
