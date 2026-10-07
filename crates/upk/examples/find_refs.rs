//! Dev tool: list exports whose tagged properties reference an object (top-level and in structs).
//! usage: cargo run -p upk --example find_refs -- <package.upk> <Object.Path>
use upk::{ObjRef, Package, Property, Value};

fn refs(props: &[Property], target: i32, path: &mut Vec<String>, out: &mut Vec<String>) {
    for p in props {
        match &p.value {
            Value::Object(i) if *i == target => out.push(format!("{}{}", path.join("."), p.name)),
            Value::Struct { fields, .. } => {
                path.push(format!("{}.", p.name));
                refs(fields, target, path, out);
                path.pop();
            }
            Value::Array { raw, .. } => {
                if Package::object_array(raw).iter().any(|r| matches!((r, ObjRef::from_index(target)), (ObjRef::Export(a), ObjRef::Export(b)) if *a == b)) {
                    out.push(format!("{}{}[]", path.join("."), p.name));
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).expect("open");
    let e = pkg.find_export(&a[1]).expect("object") ;
    let target = e as i32 + 1;
    for i in 0..pkg.exports.len() {
        let Ok(props) = pkg.properties(i) else { continue };
        let mut out = Vec::new();
        refs(&props, target, &mut Vec::new(), &mut out);
        for f in out {
            println!("{} ({}) .{f}", pkg.object_path(ObjRef::Export(i)), pkg.export_class_name(i));
        }
    }
}
