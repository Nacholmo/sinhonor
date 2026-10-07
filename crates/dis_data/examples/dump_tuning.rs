//! Prints the tuning loaded from your install: cargo run -p dis_data --example dump_tuning -- [install]
fn main() {
    let arg = std::env::args().nth(1).map(std::path::PathBuf::from);
    let install = dis_data::find_install(arg.as_deref()).expect("Dishonored install not found (pass a path or set DISHONORED_DIR)");
    let t = std::time::Instant::now();
    let data = dis_data::load(&install, Default::default()).expect("load");
    println!("loaded from {} in {:?}", install.display(), t.elapsed());
    println!("{:#?}\n{:#?}", data.player, data.blink);
    for w in &data.warnings {
        println!("warning: {w}");
    }
}
