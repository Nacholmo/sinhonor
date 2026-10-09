//! The player's passive powers and their upgrade levels, read from the install's
//! `DefaultPlayer.ini` (`[DishonoredGame.DishonoredPowersComponent] m_Powers`).
//!
//! Each level is a list of attribute modifiers. Owning a power at a level applies that level's
//! list only: the levels replace each other rather than stack. Agility is the power named
//! `Celerity`; its levels raise the power-jump kick, the fall-damage limits and, at the second
//! level, the sprint and swim speeds and the landing and mantle animation rates.

/// The power the game calls Agility.
pub const AGILITY: &str = "Celerity";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierKind {
    /// Adds the value.
    AddVal,
    /// Adds the value as a percentage of the unmodified attribute.
    AddBasePercent,
    /// Replaces the attribute with the value.
    SetVal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttributeModifier {
    /// The attribute's name without its `Attribute_` prefix (the attribute tweak field is `m_<name>`).
    pub attribute: String,
    pub kind: ModifierKind,
    pub value: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerLevel {
    pub rune_cost: i32,
    pub modifiers: Vec<AttributeModifier>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Power {
    pub name: String,
    pub levels: Vec<PowerLevel>,
}

/// An attribute's value after `mods`, applied in order to `base`.
pub fn modified<'a>(base: f32, mods: impl IntoIterator<Item = &'a AttributeModifier>) -> f32 {
    mods.into_iter().fold(base, |v, m| match m.kind {
        ModifierKind::AddVal => v + m.value,
        ModifierKind::AddBasePercent => v + m.value * 0.01 * base,
        ModifierKind::SetVal => m.value,
    })
}

/// A UE3 config struct value: `(Key=Value,...)`, nested, with quoted strings.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Text(String),
    List(Vec<(Option<String>, Node)>),
}

impl Node {
    fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::List(items) => items.iter().find(|(k, _)| k.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(key))).map(|(_, v)| v),
            Node::Text(_) => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Node::Text(t) => Some(t),
            Node::List(_) => None,
        }
    }

    /// The elements of a list value (an empty value is an empty list).
    fn items(&self) -> Vec<&Node> {
        match self {
            Node::List(items) => items.iter().map(|(_, v)| v).collect(),
            Node::Text(_) => Vec::new(),
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn value(&mut self) -> Node {
        if self.s.get(self.i) == Some(&b'(') {
            self.i += 1;
            let mut items = Vec::new();
            while self.i < self.s.len() && self.s[self.i] != b')' {
                let start = self.i;
                let mut key = None;
                // `Key=` before the value, if there is one.
                while self.i < self.s.len() && !matches!(self.s[self.i], b'=' | b',' | b'(' | b')' | b'"') {
                    self.i += 1;
                }
                if self.s.get(self.i) == Some(&b'=') {
                    key = Some(String::from_utf8_lossy(&self.s[start..self.i]).trim().to_string());
                    self.i += 1;
                } else {
                    self.i = start;
                }
                items.push((key, self.value()));
                if self.s.get(self.i) == Some(&b',') {
                    self.i += 1;
                }
            }
            self.i += 1;
            Node::List(items)
        } else if self.s.get(self.i) == Some(&b'"') {
            self.i += 1;
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != b'"' {
                self.i += 1;
            }
            let t = String::from_utf8_lossy(&self.s[start..self.i]).to_string();
            self.i += 1;
            Node::Text(t)
        } else {
            let start = self.i;
            while self.i < self.s.len() && !matches!(self.s[self.i], b',' | b')') {
                self.i += 1;
            }
            Node::Text(String::from_utf8_lossy(&self.s[start..self.i]).trim().to_string())
        }
    }
}

fn parse_node(text: &str) -> Node {
    Parser { s: text.trim().as_bytes(), i: 0 }.value()
}

/// Parses `m_Powers` values.
pub fn parse_powers(values: &[String]) -> Vec<Power> {
    values
        .iter()
        .filter_map(|v| {
            let n = parse_node(v);
            let name = n.get("m_Name")?.text()?.to_string();
            let levels = n
                .get("m_Levels")
                .map(Node::items)
                .unwrap_or_default()
                .into_iter()
                .map(|l| PowerLevel {
                    rune_cost: l.get("m_RuneCost").and_then(Node::text).and_then(|t| t.parse().ok()).unwrap_or(0),
                    modifiers: l.get("m_Modifiers").map(Node::items).unwrap_or_default().into_iter().filter_map(modifier).collect(),
                })
                .collect();
            Some(Power { name, levels })
        })
        .collect()
}

fn modifier(n: &Node) -> Option<AttributeModifier> {
    let name = n.get("m_AttributeName")?.text()?;
    let kind = match n.get("m_ModType")?.text()?.rsplit('_').next()? {
        "AddVal" => ModifierKind::AddVal,
        "AddBasePercent" => ModifierKind::AddBasePercent,
        "SetVal" => ModifierKind::SetVal,
        _ => return None,
    };
    Some(AttributeModifier {
        attribute: name.strip_prefix("Attribute_").unwrap_or(name).to_string(),
        kind,
        value: n.get("m_fModValue")?.text()?.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_levels_and_modifiers() {
        let line = r#"(m_Name="Hop",m_eUISelection=eX_Hop,m_Levels=((m_RuneCost=0,m_Modifiers=),(m_RuneCost=4,m_Modifiers=((m_AttributeName="Attribute_Spring",m_ModType=eDisAttributeModifierType_AddVal,m_fModValue=12.500000),(m_AttributeName="Attribute_Pace",m_ModType=eDisAttributeModifierType_AddBasePercent,m_fModValue=20.000000)))),m_CurrentLevel=-1)"#;
        let p = parse_powers(&[line.to_string()]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "Hop");
        assert_eq!(p[0].levels.len(), 2);
        assert!(p[0].levels[0].modifiers.is_empty());
        assert_eq!(p[0].levels[1].rune_cost, 4);
        assert_eq!(
            p[0].levels[1].modifiers,
            vec![
                AttributeModifier { attribute: "Spring".into(), kind: ModifierKind::AddVal, value: 12.5 },
                AttributeModifier { attribute: "Pace".into(), kind: ModifierKind::AddBasePercent, value: 20.0 },
            ]
        );
    }

    #[test]
    fn modifiers_apply_in_order() {
        let m = |kind, value| AttributeModifier { attribute: "X".into(), kind, value };
        assert_eq!(modified(10.0, &[m(ModifierKind::AddVal, 5.0), m(ModifierKind::AddBasePercent, 50.0)]), 20.0);
        assert_eq!(modified(10.0, &[m(ModifierKind::AddVal, 5.0), m(ModifierKind::SetVal, 3.0)]), 3.0);
        assert_eq!(modified(10.0, &[]), 10.0);
    }
}
