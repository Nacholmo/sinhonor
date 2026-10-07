//! Dev tool: find AnimSequences by name in a package and dump the start of their native data.
use upk::{lookup, ObjRef, Package, Value};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    for i in pkg.exports_of_class("AnimSequence") {
        let Ok((p, tail)) = pkg.properties_and_tail(i) else { continue };
        let Some(Value::Name(n)) = lookup(&p, "SequenceName") else { continue };
        if !n.contains(&a[1]) { continue }
        let d = pkg.export_data(i);
        println!("{} = {n}: data {} tail {tail} len {:?} frames {:?}", pkg.object_path(ObjRef::Export(i)), d.len(), lookup(&p, "SequenceLength"), lookup(&p, "NumFrames"));
        println!("  {:02x?}", &d[tail..(tail + 64).min(d.len())]);
        let t = &d[tail..];
        if let Some(pos) = t.windows(4).position(|w| w == b"50AE") { println!("  edge tag at tail+{pos}"); }
    }
}
