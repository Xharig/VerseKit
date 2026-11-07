// VerseKit — companion for Star Citizen.
// Copyright (C) 2026 Xharig
//
// SPDX-License-Identifier: GPL-3.0-only
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The own corrections of class, size and grade of blueprints (`bp-overrides.json`), and the size a skin or
//! special variant takes from its base (`load_meta`, `_size_grade_class` of `sc_bp_watcher.py`).
//!
//! Order of precedence for size, grade and class: the own corrections, then the game data, then scmdb (which the
//! catalog stands on only when the archive cannot be read). The file is optional and lies beside the collection;
//! [`ENV_FILE`] (or the old name [`crate::legacy::ENV_OVERRIDES`]) names another place.
//!
//! Layout: `{"overrides": {"<name in lower case>": {"class": "Mil", "size": 3, "grade": "A"}, …}}`. An entry with
//! class, size and grade replaces all three; an entry with a size only sets the size. The class may be written short
//! (`Civ`, `Mil`, `Ind`, `Sth`, `Cmp`) or in full. Other fields (`type`, `force`, `source`, `note`, `date`) are kept
//! by the tools that write the file and are not read here.

use std::collections::HashMap;

use crate::catalog_cache::CatalogCache;
use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::text::{py_isspace, py_strip};

/// The file in the data folder.
pub const FILE: &str = "bp-overrides.json";
/// Another place of the file, for a special case.
pub const ENV_FILE: &str = "VERSEKIT_OVERRIDES";

/// The short forms of the classes and their full names.
const CLASS_FULL: [(&str, &str); 5] =
    [("Civ", "Civilian"), ("Mil", "Military"), ("Ind", "Industrial"), ("Sth", "Stealth"), ("Cmp", "Competition")];

/// The full name of a class written short; any other text as it is.
pub fn class_full(class: &str) -> &str {
    CLASS_FULL.iter().find(|(short, _)| *short == class).map_or(class, |(_, full)| full)
}

/// The corrections of one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// Name → class (full name), size, grade: entries with all three.
    complete: HashMap<String, (String, String, String)>,
    /// Name → size: entries with a size only.
    sizes: HashMap<String, String>,
}

/// The name as the corrections are looked up by: lower case, a protected space or a replacement character as a
/// space, without spaces around it.
pub fn norm(name: &str) -> String {
    py_strip(&name.to_lowercase().replace(['\u{a0}', '\u{fffd}'], " ")).to_string()
}

/// The name of the base of a skin or special variant: without the part in `"…"`, spaces made single
/// (`p4-ar "rust society" rifle` → `p4-ar rifle`).
pub fn base_name(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(open) = rest.find('"') {
        let Some(close) = rest[open + 1..].find('"') else { break };
        out.push_str(rest[..open].trim_end_matches(py_isspace));
        out.push(' ');
        rest = rest[open + 1 + close + 1..].trim_start_matches(py_isspace);
    }
    out.push_str(rest);
    py_strip(&out.split(py_isspace).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")).to_string()
}

/// Whether the value counts as given: not null, not `false`, not zero, not empty.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Str(text)) => !text.is_empty(),
        Some(Value::Int(text)) => text != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        Some(_) => true,
    }
}

/// The value as text, as `str()` writes it.
fn as_text(value: &Value) -> String {
    match value {
        Value::Str(text) => text.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => jsonio::dumps(other, None, false),
    }
}

impl Overrides {
    /// The corrections a parsed file holds. What comes after an entry that is not an object is not read.
    pub fn from_value(data: &Value) -> Overrides {
        let mut found = Overrides::default();
        let Some(Value::Object(entries)) = data.get("overrides") else { return found };
        for (key, entry) in entries {
            let Value::Object(_) = entry else { break };
            let (class, size, grade) = (entry.get("class"), entry.get("size"), entry.get("grade"));
            if truthy(class) && truthy(size) && truthy(grade) {
                let (class, size, grade) = (class.map(as_text), size.map(as_text), grade.map(as_text));
                let class = class.unwrap_or_default();
                found.complete.insert(
                    key.clone(),
                    (class_full(&class).to_string(), size.unwrap_or_default(), grade.unwrap_or_default()),
                );
            } else if let Some(size) = size.filter(|size| !matches!(size, Value::Null)) {
                found.sizes.insert(key.clone(), as_text(size));
            }
        }
        found
    }

    /// The corrections of the file of `paths`; none when there is no file or it cannot be read.
    pub fn load(paths: &Paths) -> Overrides {
        jsonio::read_json(&file(paths)).map(|data| Overrides::from_value(&data)).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.complete.is_empty() && self.sizes.is_empty()
    }

    /// Class, size and grade of a blueprint corrected in full.
    pub fn complete(&self, name: &str) -> Option<(&str, &str, &str)> {
        self.complete.get(&norm(name)).map(|(class, size, grade)| (class.as_str(), size.as_str(), grade.as_str()))
    }

    /// The corrected size of a blueprint: its own (by the looked up name, an empty one passes on to the name in
    /// lower case), else the one of its base. Can be empty, which sets no size.
    pub fn size(&self, name: &str) -> Option<&str> {
        let key = norm(name);
        let own = match self.sizes.get(&key).filter(|size| !size.is_empty()) {
            Some(size) => Some(size),
            None => self.sizes.get(&name.to_lowercase()),
        };
        own.or_else(|| {
            let base = base_name(&key);
            (base != key).then(|| self.sizes.get(&base)).flatten()
        })
        .map(String::as_str)
    }

    /// Sets the corrections on the blueprints of `catalog`, and the size of the base on a variant that states none.
    pub fn apply(&self, catalog: &mut CatalogCache) {
        let sizes_by_name: HashMap<String, i64> =
            catalog.blueprints.iter().filter_map(|b| Some((norm(&b.name), b.size?))).collect();
        for blueprint in &mut catalog.blueprints {
            if let Some((class, size, grade)) = self.complete(&blueprint.name) {
                blueprint.class = Some(class.to_string());
                blueprint.size = number(size);
                blueprint.grade = grade_number(grade);
                continue;
            }
            if let Some(size) = self.size(&blueprint.name).and_then(number) {
                blueprint.size = Some(size);
            } else if blueprint.size.is_none() {
                let key = norm(&blueprint.name);
                let base = base_name(&key);
                if base != key {
                    blueprint.size = sizes_by_name.get(&base).copied();
                }
            }
        }
    }
}

/// A size written as a number (`3`, `3.0`).
fn number(text: &str) -> Option<i64> {
    let text = py_strip(text);
    text.parse().ok().or_else(|| text.parse::<f64>().ok().filter(|x| x.fract() == 0.0 && x.is_finite()).map(|x| x as i64))
}

/// The grade as the catalog keeps it: `A` = 1 … `D` = 4; a number as it is.
fn grade_number(text: &str) -> Option<i64> {
    let text = py_strip(text);
    match text.chars().collect::<Vec<_>>().as_slice() {
        [letter] if letter.is_ascii_alphabetic() => Some(i64::from(letter.to_ascii_uppercase() as u8 - b'A') + 1),
        _ => number(text),
    }
}

/// Where the file of `paths` lies: the place [`ENV_FILE`] names, else the old name, else the data folder.
pub fn file(paths: &Paths) -> String {
    paths
        .host
        .var_set(ENV_FILE)
        .or_else(|| paths.host.var_set(crate::legacy::ENV_OVERRIDES))
        .map_or_else(|| paths.app_file(FILE), str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_cache::Blueprint;

    fn parse(text: &str) -> Overrides {
        Overrides::from_value(&jsonio::loads(text).expect("json"))
    }

    fn blueprint(name: &str, size: Option<i64>, grade: Option<i64>, class: Option<&str>) -> Blueprint {
        Blueprint {
            key: crate::paths::name_key(name),
            name: name.into(),
            size,
            grade,
            class: class.map(str::to_string),
            ..Blueprint::default()
        }
    }

    #[test]
    fn a_complete_entry_replaces_all_three_and_a_size_only_the_size() {
        let overrides = parse(
            r#"{"overrides": {
                "elsen": {"class": "Mil", "size": 1, "grade": "B", "source": "log"},
                "cf-337 panther repeater": {"size": 3, "type": "Laser Repeater", "force": true},
                "half": {"class": "Civ", "grade": "C"}
            }}"#,
        );
        let mut catalog = CatalogCache {
            blueprints: vec![
                blueprint("Elsen", Some(2), Some(1), Some("Civilian")),
                blueprint("CF-337 Panther Repeater", Some(2), None, None),
                blueprint("Half", Some(2), Some(1), Some("Military")),
            ],
            ..CatalogCache::default()
        };
        overrides.apply(&mut catalog);
        let seen: Vec<_> =
            catalog.blueprints.iter().map(|b| (b.class.as_deref(), b.size, b.grade)).collect();
        assert_eq!(
            seen,
            [
                (Some("Military"), Some(1), Some(2)),
                (None, Some(3), None),
                (Some("Military"), Some(2), Some(1)),
            ],
            "complete: all three; size only: the size; without a size: nothing"
        );
    }

    #[test]
    fn a_variant_takes_the_size_of_its_base() {
        let overrides = parse(r#"{"overrides": {"omnisky vi cannon": {"size": 2}}}"#);
        assert_eq!(overrides.size("Omnisky VI \"Blackguard\" Cannon"), Some("2"), "the correction of the base");
        assert_eq!(base_name("p4-ar  \"rust society\"  rifle"), "p4-ar rifle");
        assert_eq!(base_name("\"Ghost\" helmet"), "helmet");
        let mut catalog = CatalogCache {
            blueprints: vec![
                blueprint("P4-AR Rifle", Some(1), None, None),
                blueprint("P4-AR \"Rust Society\" Rifle", None, None, None),
                blueprint("Lone \"Skin\" Knife", None, None, None),
            ],
            ..CatalogCache::default()
        };
        Overrides::default().apply(&mut catalog);
        assert_eq!(catalog.blueprints[1].size, Some(1), "the game data of the base");
        assert_eq!(catalog.blueprints[2].size, None, "no base: no size");
    }

    #[test]
    fn the_key_is_matched_as_written() {
        let overrides = parse(r#"{"overrides": {"Elsen": {"class": "Mil", "size": 1, "grade": "B"}}}"#);
        assert!(overrides.complete("Elsen").is_none(), "a key in capitals never matches, as in the Python version");
        let overrides = parse("{\"overrides\": {\"aegis\u{a0}x\": {\"size\": 4}}}");
        assert_eq!(overrides.size("AEGIS\u{a0}X"), Some("4"), "found by the name in lower case");
    }

    #[test]
    fn a_broken_file_gives_what_was_read_before() {
        let overrides = parse(r#"{"overrides": {"a": {"size": 1}, "b": 7, "c": {"size": 2}}}"#);
        assert_eq!((overrides.size("a"), overrides.size("c")), (Some("1"), None));
        assert!(parse(r#"{"overrides": []}"#).is_empty());
        assert!(parse("[1]").is_empty());
    }

    #[test]
    fn the_file_is_found_by_the_new_name_then_the_old() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let home = "/home/test";
        let at = |vars: &[(&str, &str)]| {
            let mut all = vec![(crate::legacy::ENV_HOME, home)];
            all.extend_from_slice(vars);
            file(&Paths::new(Host::new(Flavor::Posix, &all, home)))
        };
        assert!(at(&[]).ends_with("bp-overrides.json"));
        assert_eq!(at(&[(crate::legacy::ENV_OVERRIDES, "/x/old.json")]), "/x/old.json");
        assert_eq!(at(&[(ENV_FILE, "/x/new.json"), (crate::legacy::ENV_OVERRIDES, "/x/old.json")]), "/x/new.json");
        assert!(at(&[(ENV_FILE, ""), (crate::legacy::ENV_OVERRIDES, "")]).ends_with("bp-overrides.json"), "empty is none");
    }
}
