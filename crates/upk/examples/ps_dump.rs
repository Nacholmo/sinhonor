//! Dev tool: walk a Cascade ParticleSystem (emitters -> LOD 0 -> modules) in your own install.
//! usage: cargo run -p upk --example ps_dump -- <package.upk> <ParticleSystem.Path>
use upk::{lookup, ObjRef, Package, Property, Value};

fn refs(v: Option<&Value>) -> Vec<i32> {
    match v {
        Some(Value::Array { raw, .. }) => raw.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect(),
        _ => Vec::new(),
    }
}

fn show(pkg: &Package, props: &[Property], indent: usize) {
    for p in props {
        let pad = "  ".repeat(indent);
        match &p.value {
            Value::Struct { name, fields } => {
                println!("{pad}{}[{}] ({name})", p.name, p.array_index);
                show(pkg, fields, indent + 1);
            }
            Value::Array { count, raw } if raw.len() == *count as usize * 4 && p.name.contains("LookupTable") => {
                let f: Vec<f32> = raw.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                println!("{pad}{} = {:?}", p.name, f);
            }
            Value::Object(i) if *i != 0 => println!("{pad}{} = {} ({})", p.name, pkg.object_path(ObjRef::from_index(*i)), i),
            v => println!("{pad}{}[{}] = {v:?}", p.name, p.array_index),
        }
    }
}

fn obj(pkg: &Package, i: i32) -> Option<(usize, Vec<Property>)> {
    match ObjRef::from_index(i) {
        ObjRef::Export(e) => pkg.properties(e).ok().map(|p| (e, p)),
        _ => None,
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&a[0]).expect("open");
    let ps = pkg.find_export(&a[1]).expect("not found");
    let props = pkg.properties(ps).unwrap();
    for (n, em) in refs(lookup(&props, "Emitters")).into_iter().enumerate() {
        let Some((e, ep)) = obj(&pkg, em) else { continue };
        println!("== emitter {n}: {} ({})", pkg.object_path(ObjRef::Export(e)), pkg.export_class_name(e));
        show(&pkg, &ep.iter().filter(|p| p.name != "LODLevels").cloned().collect::<Vec<_>>(), 1);
        let Some(&lod0) = refs(lookup(&ep, "LODLevels")).first() else { continue };
        let Some((_, lp)) = obj(&pkg, lod0) else { continue };
        let mut mods = vec![];
        for k in ["RequiredModule", "SpawnModule", "TypeDataModule"] {
            if let Some(Value::Object(i)) = lookup(&lp, k) {
                mods.push(*i);
            }
        }
        mods.extend(refs(lookup(&lp, "Modules")));
        for m in mods {
            if let Some((mi, mp)) = obj(&pkg, m) {
                println!("  -- {}", pkg.export_class_name(mi));
                show(&pkg, &mp, 2);
            }
        }
    }
}
