//! Dev tool: inspect a material and its expression nodes in your own install.
//! usage: cargo run -p upk --example material_graph -- <package.upk> <Material.Path>
use upk::{ObjRef, Package, Property, Value};

fn show(pkg: &Package, props: &[Property], indent: usize) {
    for p in props {
        let pad = "  ".repeat(indent);
        match &p.value {
            Value::Struct { name, fields } => {
                println!("{pad}{}[{}] ({name})", p.name, p.array_index);
                show(pkg, fields, indent + 1);
            }
            Value::Array { count, raw } => println!("{pad}{}[{}] = array({count}, {} bytes)", p.name, p.array_index, raw.len()),
            Value::Raw { type_name, raw } => println!("{pad}{}[{}] = <{type_name}, {} bytes>", p.name, p.array_index, raw.len()),
            Value::Object(i) if *i != 0 => println!("{pad}{}[{}] = {} ({i})", p.name, p.array_index, pkg.object_path(ObjRef::from_index(*i))),
            v => println!("{pad}{}[{}] = {v:?}", p.name, p.array_index),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&args[0]).unwrap();
    let i = pkg.find_export(&args[1]).unwrap();
    let p = pkg.properties(i).unwrap();
    show(&pkg, &p, 0);
    if let Some(upk::Value::Array { raw, .. }) = upk::lookup(&p, "Expressions") {
        for r in Package::object_array(raw) {
            if let ObjRef::Export(i) = r {
                println!("=== {} : {}", pkg.object_path(r), pkg.export_class_name(i));
                show(&pkg, &pkg.properties(i).unwrap(), 1);
            }
        }
    }
}
