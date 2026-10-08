//! Dev tool: print a StaticMesh's bounds (first native field after its properties).
use upk::{ObjRef, Package};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    let i = pkg.find_export(&a[1]).expect("mesh");
    match pkg.static_mesh(i) { Ok(m) => println!("LOD0: {} vertices, {} triangles", m.vertices.len(), m.indices.len()/3), Err(e) => println!("LOD0: {e}") };
    let (props, tail) = pkg.properties_and_tail(i).unwrap();
    for p in &props { println!("  {} = {:?}", p.name, p.value); }
    let d = pkg.export_data(i);
    let f: Vec<f32> = d[tail..tail + 28].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    println!("{} ({}): bounds origin {:?} extent {:?} radius {}", pkg.object_path(ObjRef::Export(i)), d.len(), &f[0..3], &f[3..6], f[6]);
    println!("next bytes {:02x?}", &d[tail + 28..(tail + 120).min(d.len())]);
}
