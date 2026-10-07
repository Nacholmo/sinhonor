//! Dev tool: load a particle system from your install and simulate it for a second.
//! usage: cargo run -p cascade --example ps_sim -- <package.upk> <System.Path>
use std::sync::Arc;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = upk::Package::open(&a[0]).unwrap();
    let dir = std::path::Path::new(&a[0]).parent().unwrap();
    let mats = cascade::MaterialLoader::new(Some(dir.to_path_buf())).with_engine_defaults(upk::Package::open(dir.join("Engine.upk")).unwrap());
    let def = cascade::load_system(&pkg, &a[1], &mats).expect("system");
    for e in &def.emitters {
        let m = e.material.as_ref().map(|m| format!("{} {:?} {}x{} tint {:?}", m.name, m.blend, m.width, m.height, m.tint)).unwrap_or_default();
        println!("emitter {} dur {} loops {} bursts {:?} mesh {} align {:?}\n    material {m}", e.name, e.duration, e.loops, e.bursts, e.is_mesh, e.alignment);
    }
    println!("LOD distances {:?}, {} levels", def.lod_distances, def.lods.len());
    let lod = std::env::args().nth(3).and_then(|d| d.parse::<f32>().ok());
    let mut inst = cascade::Instance::new(Arc::new(def), glam::Affine3A::IDENTITY, 1);
    if let Some(d) = lod {
        inst.set_camera_distance(d);
        println!("camera distance {d} -> LOD {}", inst.lod());
    }
    for step in 0..30 {
        inst.update(1.0 / 30.0);
        if step % 10 == 9 {
            let ps: Vec<_> = inst.particles().collect();
            let avg_alpha = ps.iter().map(|p| p.color[3]).sum::<f32>() / ps.len().max(1) as f32;
            println!("t={:.2}s particles {} avg alpha {avg_alpha:.2} first {:?}", (step + 1) as f32 / 30.0, ps.len(), ps.first().map(|p| (p.pos, p.size)));
        }
    }
}
