//! Prints the tuning loaded from your install: cargo run -p dis_data --example dump_tuning -- [install]
fn main() {
    let arg = std::env::args().nth(1).map(std::path::PathBuf::from);
    let install = dis_data::find_install(arg.as_deref()).expect("Dishonored install not found (pass a path or set DISHONORED_DIR)");
    let t = std::time::Instant::now();
    let data = dis_data::load(&install, Default::default()).expect("load");
    println!("loaded from {} in {:?}", install.display(), t.elapsed());
    println!("{:#?}\n{:#?}\n{:#?}\n{:#?}", data.player, data.blink, data.drop_assassinate, data.melee);
    for w in &data.warnings {
        println!("warning: {w}");
    }
    let mut anims: Vec<_> = data.anim_lengths.iter().filter(|(k, _)| k.contains("Mantle") || k.contains("Blink") || k.contains("Slide") || k.contains("Land") || k.contains("Jump")).collect();
    anims.sort_by(|a, b| a.0.cmp(b.0));
    println!("{} player anims; motion-related: {anims:?}", data.anim_lengths.len());
    if std::env::var_os("ALL_ANIMS").is_some() {
        let mut all: Vec<_> = data.anim_lengths.keys().collect();
        all.sort();
        println!("ALL {}", all.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
    }
    println!("blink sound events: {:?}", data.blink.sound_events);
    let t = std::time::Instant::now();
    let snd = dis_data::sounds::load_sounds(&install, &data.blink.sound_events);
    let bytes: usize = snd.ogg.values().map(Vec::len).sum();
    println!("sounds: {} cues, {} clips, {} KiB ogg, in {:?}", snd.cues.len(), snd.ogg.len(), bytes / 1024, t.elapsed());
    let t = std::time::Instant::now();
    let fx = dis_data::effects::load_effects(&install);
    let mut names: Vec<_> = fx.systems.iter().map(|(k, v)| format!("{k:?}={}e", v.emitters.len())).collect();
    names.sort();
    println!("effects in {:?}: {} (lens distance {}): {}", t.elapsed(), fx.systems.len(), fx.lens_distance, names.join(" "));
    for w in &fx.warnings {
        println!("effect warning: {w}");
    }
    let t = std::time::Instant::now();
    match dis_data::viewmodel::load_viewmodel(&install) {
        Ok(vm) => {
            println!(
            "viewmodel in {:?}: arms {} verts (diffuse {:?}, normal {:?}), sword {:?}, {} anims, warnings {:?}",
            t.elapsed(),
            vm.arms.mesh.vertices.len(),
            vm.arms.diffuse.as_ref().map(|t| (t.width, t.height)),
            vm.arms.normal.as_ref().map(|t| (t.width, t.height)),
            vm.sword.as_ref().map(|s| (s.mesh.vertices.len(), s.diffuse.is_some(), s.normal.is_some())),
            vm.anims.len(),
            vm.warnings
        );
        let mut seqs: Vec<_> = vm.particle_notifies.iter().collect();
        seqs.sort_by_key(|(k, _)| k.as_str());
        for (seq, list) in seqs {
            for n in list {
                println!("  {seq} @ {:.2}s -> {} at {:?}/{:?}", n.time, n.system.name, n.socket, n.bone);
            }
        }
        }
        Err(e) => println!("viewmodel error: {e}"),
    }
    for w in &snd.warnings {
        println!("sound warning: {w}");
    }
    for w in &data.warnings {
        println!("warning: {w}");
    }
}
