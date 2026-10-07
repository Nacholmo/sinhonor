//! Prints the tuning loaded from your install: cargo run -p dis_data --example dump_tuning -- [install]
fn main() {
    let arg = std::env::args().nth(1).map(std::path::PathBuf::from);
    let install = dis_data::find_install(arg.as_deref()).expect("Dishonored install not found (pass a path or set DISHONORED_DIR)");
    let t = std::time::Instant::now();
    let data = dis_data::load(&install, Default::default()).expect("load");
    println!("loaded from {} in {:?}", install.display(), t.elapsed());
    println!("{:#?}\n{:#?}", data.player, data.blink);
    let mut anims: Vec<_> = data.anim_lengths.iter().filter(|(k, _)| k.contains("Mantle") || k.contains("Blink") || k.contains("Slide") || k.contains("Land") || k.contains("Jump")).collect();
    anims.sort_by(|a, b| a.0.cmp(b.0));
    println!("{} player anims; motion-related: {anims:?}", data.anim_lengths.len());
    println!("blink sound events: {:?}", data.blink.sound_events);
    let t = std::time::Instant::now();
    let snd = dis_data::sounds::load_sounds(&install, &data.blink.sound_events);
    let bytes: usize = snd.ogg.values().map(Vec::len).sum();
    println!("sounds: {} cues, {} clips, {} KiB ogg, in {:?}", snd.cues.len(), snd.ogg.len(), bytes / 1024, t.elapsed());
    for w in &snd.warnings {
        println!("sound warning: {w}");
    }
    for w in &data.warnings {
        println!("warning: {w}");
    }
}
