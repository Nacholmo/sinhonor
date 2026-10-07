//! Dev tool: print an object's tagged properties from a package in your own install.
//! usage: cargo run -p upk --example upk_dump -- <package.upk> <Object.Path | --list CLASS>
use upk::{ObjRef, Package, Property, Value};

fn show(props: &[Property], indent: usize) {
    for p in props {
        let pad = "  ".repeat(indent);
        match &p.value {
            Value::Struct { name, fields } => {
                println!("{pad}{}[{}] ({name})", p.name, p.array_index);
                show(fields, indent + 1);
            }
            Value::Array { count, raw } => println!("{pad}{}[{}] = array({count}, {} bytes)", p.name, p.array_index, raw.len()),
            Value::Raw { type_name, raw } => println!("{pad}{}[{}] = <{type_name}, {} bytes>", p.name, p.array_index, raw.len()),
            v => println!("{pad}{}[{}] = {v:?}", p.name, p.array_index),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Package::open(&args[0]).expect("open package");
    println!("{:?}  names={} imports={} exports={}", pkg.summary, pkg.names.len(), pkg.imports.len(), pkg.exports.len());
    if args.get(1).map(String::as_str) == Some("--list") {
        for i in pkg.exports_of_class(&args[2]) {
            println!("{}", pkg.object_path(ObjRef::Export(i)));
        }
        return;
    }
    let i = pkg.find_export(&args[1]).expect("export not found");
    println!("{} : {}", pkg.object_path(ObjRef::Export(i)), pkg.export_class_name(i));
    show(&pkg.properties(i).expect("properties"), 1);
}
