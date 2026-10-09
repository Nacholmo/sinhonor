//! Minimal reader for UE3 config files (`Config/Default*.ini`).

use std::{collections::HashMap, fs, path::Path};

#[derive(Default, Debug)]
pub struct Ini {
    /// section (lower-case) -> key (lower-case) -> values in file order (`+Key=` lines append).
    sections: HashMap<String, HashMap<String, Vec<String>>>,
}

impl Ini {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        Ok(Self::parse(&fs::read_to_string(path)?))
    }

    pub fn parse(text: &str) -> Self {
        let mut ini = Ini::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = s.to_ascii_lowercase();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim_start_matches(['+', '.', '-', '!']).trim().to_ascii_lowercase();
                ini.sections.entry(section.clone()).or_default().entry(k).or_default().push(v.trim().to_string());
            }
        }
        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_ascii_lowercase())?
            .get(&key.to_ascii_lowercase())?
            .last()
            .map(String::as_str)
    }

    /// Every value of a repeated key, in file order.
    pub fn all(&self, section: &str, key: &str) -> &[String] {
        self.sections
            .get(&section.to_ascii_lowercase())
            .and_then(|s| s.get(&key.to_ascii_lowercase()))
            .map_or(&[], Vec::as_slice)
    }

    pub fn f32(&self, section: &str, key: &str) -> Option<f32> {
        self.get(section, key)?.parse().ok()
    }

    pub fn merge(&mut self, other: Ini) {
        for (s, keys) in other.sections {
            let dst = self.sections.entry(s).or_default();
            for (k, v) in keys {
                dst.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_keys() {
        let ini = Ini::parse("[A.B]\nX=1.5\n; c\n+Y=a\n+Y=b\n[c]\nz = 2\n");
        assert_eq!(ini.f32("a.b", "x"), Some(1.5));
        assert_eq!(ini.get("A.B", "Y"), Some("b"));
        assert_eq!(ini.f32("C", "Z"), Some(2.0));
    }
}
