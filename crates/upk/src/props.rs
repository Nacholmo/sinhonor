//! UE3 tagged-property streams (file version 801).
//!
//! Tag: name, type, size, array index, then a type-specific extra (struct name for structs,
//! the value byte for bools, the enum name for bytes), then `size` bytes of value. A stream
//! ends at the name `None`. Structs are themselves tagged streams, except the engine's
//! "immutable" math structs which are stored as raw binary.

use crate::{reader::Reader, Error, Package, Result};

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub array_index: i32,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub enum Value {
    Int(i32),
    Float(f32),
    Bool(bool),
    Byte(u8),
    Enum(String),
    Name(String),
    Str(String),
    Object(i32),
    Vector([f32; 3]),
    Rotator([i32; 3]),
    Struct { name: String, fields: Vec<Property> },
    /// Arrays keep their raw payload; element types are not recorded in the tag.
    Array { count: i32, raw: Vec<u8> },
    Raw { type_name: String, raw: Vec<u8> },
}

impl Value {
    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::Float(f) => Some(f),
            Value::Int(i) => Some(i as f32),
            Value::Byte(b) => Some(b as f32),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }
    pub fn fields(&self) -> &[Property] {
        match self {
            Value::Struct { fields, .. } => fields,
            _ => &[],
        }
    }
}

/// Finds `name` (and optional `[index]`) among properties; `a.b.c` descends into structs.
pub fn lookup<'a>(props: &'a [Property], path: &str) -> Option<&'a Value> {
    let (head, rest) = match path.split_once('.') {
        Some((h, r)) => (h, Some(r)),
        None => (path, None),
    };
    let (name, index) = match head.split_once('[') {
        Some((n, i)) => (n, i.trim_end_matches(']').parse().ok()?),
        None => (head, 0),
    };
    let p = props.iter().find(|p| p.name.eq_ignore_ascii_case(name) && p.array_index == index)?;
    match rest {
        None => Some(&p.value),
        Some(r) => lookup(p.value.fields(), r),
    }
}

const MAX_STREAM: usize = 4096;

pub(crate) fn read_tagged(r: &mut Reader, pkg: &Package) -> Result<Vec<Property>> {
    let mut out = Vec::new();
    for _ in 0..MAX_STREAM {
        let name = name_at(r, pkg)?;
        if name == "None" {
            return Ok(out);
        }
        let type_name = name_at(r, pkg)?;
        if !type_name.ends_with("Property") {
            return Err(Error::Unsupported(format!("not a property type: {type_name}")));
        }
        let size = r.i32()?;
        let array_index = r.i32()?;
        if !(0..1 << 24).contains(&size) || !(0..4096).contains(&array_index) {
            return Err(Error::Unsupported("implausible property tag".into()));
        }
        let size = size as usize;
        let value = match type_name.as_str() {
            "BoolProperty" => Value::Bool(r.u8()? != 0),
            "StructProperty" => {
                let struct_name = name_at(r, pkg)?;
                let body = r.bytes(size)?;
                struct_value(struct_name, body, pkg)
            }
            "ByteProperty" => {
                let enum_name = name_at(r, pkg)?;
                let body = r.bytes(size)?;
                if size == 8 && enum_name != "None" {
                    let mut b = Reader::new(body);
                    Value::Enum(name_at(&mut b, pkg)?)
                } else {
                    Value::Byte(*body.first().unwrap_or(&0))
                }
            }
            other => {
                let body = r.bytes(size)?;
                let mut b = Reader::new(body);
                match other {
                    "IntProperty" => Value::Int(b.i32()?),
                    "FloatProperty" => Value::Float(b.f32()?),
                    "NameProperty" => Value::Name(name_at(&mut b, pkg)?),
                    "StrProperty" => Value::Str(b.fstring()?),
                    "ObjectProperty" | "ComponentProperty" | "ClassProperty" | "InterfaceProperty" => {
                        Value::Object(b.i32()?)
                    }
                    "ArrayProperty" => Value::Array { count: b.i32()?, raw: body[4.min(body.len())..].to_vec() },
                    _ => Value::Raw { type_name: other.to_string(), raw: body.to_vec() },
                }
            }
        };
        out.push(Property { name, array_index, value });
    }
    Err(Error::Unsupported("property stream did not terminate".into()))
}

fn struct_value(name: String, body: &[u8], pkg: &Package) -> Value {
    let mut b = Reader::new(body);
    match (name.as_str(), body.len()) {
        ("Vector", 12) => {
            return Value::Vector([b.f32().unwrap(), b.f32().unwrap(), b.f32().unwrap()]);
        }
        ("Rotator", 12) => {
            return Value::Rotator([b.i32().unwrap(), b.i32().unwrap(), b.i32().unwrap()]);
        }
        _ => {}
    }
    match read_tagged(&mut b, pkg) {
        Ok(fields) if b.remaining() == 0 => Value::Struct { name, fields },
        _ => Value::Raw { type_name: format!("struct {name}"), raw: body.to_vec() },
    }
}

fn name_at(r: &mut Reader, pkg: &Package) -> Result<String> {
    let n = r.name_ref()?;
    if n.index < 0 || n.index as usize >= pkg.names.len() || !(0..1 << 16).contains(&n.number) {
        return Err(Error::Unsupported("name index out of range".into()));
    }
    Ok(pkg.name(n))
}
