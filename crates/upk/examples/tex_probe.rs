//! Dev tool: show a material instance's parameters and probe its textures' serialized layout.
use upk::{lookup, ObjRef, Package, Value};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).unwrap();
    let mi = pkg.find_export(&a[1]).unwrap();
    let p = pkg.properties(mi).unwrap();
    for f in ["TextureParameterValues", "VectorParameterValues", "ScalarParameterValues"] {
        if let Some(Value::Array { count, raw }) = lookup(&p, f) {
            for e in pkg.struct_array(*count, raw).unwrap() {
                let name = match lookup(&e, "ParameterName") { Some(Value::Name(n)) => n.clone(), _ => "?".into() };
                let val = e.iter().find(|q| q.name == "ParameterValue").map(|q| q.value.clone());
                match val {
                    Some(Value::Object(i)) => {
                        let r = ObjRef::from_index(i);
                        println!("{f}: {name} = {} ({:?})", pkg.object_path(r), r);
                        if let ObjRef::Export(e) = r {
                            let tp = pkg.properties(e).unwrap();
                            for q in &tp { println!("    {} = {:?}", q.name, q.value); }
                            let d = pkg.export_data(e);
                            let (_, tail) = pkg.properties_and_tail(e).unwrap();
                            println!("    export bytes {}; tail at {tail}: {:02x?}", d.len(), &d[tail..(tail + 96).min(d.len())]);
                            let dir = std::path::Path::new(&a[0]).parent();
                            match pkg.texture_rgba(e, 4096, dir) {
                                Ok(t) => {
                                    let avg: Vec<u32> = (0..4).map(|c| t.pixels.iter().skip(c).step_by(4).map(|&v| v as u32).sum::<u32>() / (t.width * t.height)).collect();
                                    println!("    decoded {}x{} avg rgba {avg:?}", t.width, t.height);
                                    if let Some(out) = a.get(2) {
                                        // Write a PPM/PAM for viewing.
                                        let mut f = format!("P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", t.width, t.height).into_bytes();
                                        f.extend_from_slice(&t.pixels);
                                        std::fs::write(format!("{out}/{}.pam", pkg.object_path(r).replace('.', "_")), f).unwrap();
                                    }
                                }
                                Err(err) => println!("    decode failed: {err}"),
                            }
                        }
                    }
                    v => println!("{f}: {name} = {v:?}"),
                }
            }
        }
    }
    if let Some(Value::Object(i)) = lookup(&p, "Parent") { println!("Parent = {}", pkg.object_path(ObjRef::from_index(*i))); }
}
