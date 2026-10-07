//! Dev tool: per emitter, print each LOD level's spawn rate table and the system's LOD distances.
use upk::{lookup, ObjRef, Package, Value};
fn table(p: &[upk::Property], f: &str) -> String {
    match lookup(p, f) {
        Some(Value::Struct { fields, .. }) => match lookup(fields, "LookupTable") {
            Some(Value::Array { raw, .. }) => format!("{:?}", raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>()),
            _ => "default".into(),
        },
        _ => "-".into(),
    }
}
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    let ps = pkg.find_export(&a[1]).unwrap();
    let p = pkg.properties(ps).unwrap();
    if let Some(Value::Array { raw, .. }) = lookup(&p, "LODDistances") {
        println!("LODDistances {:?}", raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>());
    }
    for p2 in &p { if p2.name.contains("LOD") || p2.name.contains("Method") { println!("  {} = {:?}", p2.name, p2.value); } }
    let Some(Value::Array { raw, .. }) = lookup(&p, "Emitters") else { return };
    for em in Package::object_array(raw) {
        let ObjRef::Export(e) = em else { continue };
        let ep = pkg.properties(e).unwrap();
        let Some(Value::Array { raw: lods, .. }) = lookup(&ep, "LODLevels") else { continue };
        for (n, l) in Package::object_array(lods).into_iter().enumerate() {
            let ObjRef::Export(li) = l else { continue };
            let lp = pkg.properties(li).unwrap();
            let enabled = lookup(&lp, "bEnabled").and_then(Value::as_bool);
            if let Some(Value::Object(s)) = lookup(&lp, "SpawnModule") {
                if let ObjRef::Export(si) = ObjRef::from_index(*s) {
                    let sp = pkg.properties(si).unwrap();
                    let bursts = matches!(lookup(&sp, "BurstList"), Some(Value::Array { count, .. }) if *count > 0);
                    println!("{} LOD{n} enabled {:?}: rate {} scale {} bursts {bursts}", pkg.object_path(em).rsplit('.').next().unwrap(), enabled, table(&sp, "Rate"), table(&sp, "RateScale"));
                }
            }
        }
    }
}
