//! Dev tool: dump the Edge skeleton header of a SkeletalMesh.
use upk::Package;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    let m = pkg.skeletal_mesh(pkg.find_export(&a[1]).unwrap()).unwrap();
    let s = &m.edge_skeleton;
    let u32s: Vec<u32> = s[..64].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    println!("u32: {:08x?}", u32s);
    let u16s: Vec<u16> = s[16..32].chunks_exact(2).map(|c| u16::from_le_bytes(c.try_into().unwrap())).collect();
    println!("u16 @16: {:?}", u16s);
    for (k, v) in u32s.iter().enumerate() {
        let field = k * 4;
        let target = field + *v as usize;
        if *v > 0 && target < s.len() { println!("  field +{field:2}: self-rel -> {target}"); }
    }
}
