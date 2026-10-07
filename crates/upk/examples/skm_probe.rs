//! Dev tool: summarize a SkeletalMesh from your own install.
use upk::{ObjRef, Package};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    let i = pkg.find_export(&a[1]).expect("mesh");
    let m = pkg.skeletal_mesh(i).expect("parse");
    println!("{} bones, {} verts, {} indices, {} sections, edge skeleton {} bytes", m.bones.len(), m.vertices.len(), m.indices.len(), m.sections.len(), m.edge_skeleton.len());
    println!("mesh origin {:?} rot origin {:?}", m.mesh_origin, m.rot_origin);
    for (k, mat) in m.materials.iter().enumerate() { println!("  material {k}: {}", pkg.object_path(*mat)); }
    for s in &m.sections { println!("  section mat {} first {} tris {}", s.material, s.first_index, s.num_triangles); }
    for (k, b) in m.bones.iter().enumerate().take(80) { println!("  bone {k:2} {:28} parent {:2} pos {:?}", b.name, b.parent, b.position); }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &m.vertices { for c in 0..3 { lo[c] = lo[c].min(v.position[c]); hi[c] = hi[c].max(v.position[c]); } }
    println!("bounds {lo:?} .. {hi:?}; v0 {:?}", m.vertices.first());
    let _ = ObjRef::None;
}
