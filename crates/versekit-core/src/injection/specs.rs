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

//! The tag behind an item name, `(Mil/3/A)` or `(IR1)`, read from the
//! item's description in the same `global.ini`: port of `specs.py`.
//!
//! The field names and word parts are matched in both languages at once;
//! a spelling a file does not use cannot give a false hit.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::text::{PY_SPACE, py_strip};

/// Field names per field (`specs.FIELDS`), in the order they are tried.
const SIZE: [&str; 3] = ["Größe", "Grösse", "Size"];
const GRADE: [&str; 6] = ["Gütegrad", "Guetegrad", "Güteklasse", "Güte", "Grade", "Grad"];
const CLASS: [&str; 2] = ["Klasse", "Class"];
const GUIDANCE_FIELD: [&str; 4] = ["Verfolgungssignal", "Tracking Signal", "Signature Type", "Signature"];
const MAKER: [&str; 2] = ["Hersteller", "Manufacturer"];

/// Word parts and their tags (`specs.CLASSES`), lower case.
const CLASSES: [(&[&str], &str); 15] = [
    (&["civilian", "zivil"], "Civ"),
    (&["military", "militär", "militaer"], "Mil"),
    (&["industrial", "industrie"], "Ind"),
    (&["stealth", "tarnung"], "Sth"),
    (&["competition", "wettkampf", "wettbewerb"], "Cmp"),
    (&["laser"], "Las"),
    (&["elektron", "electron"], "Ele"),
    (&["plasma"], "Pla"),
    (&["distortion", "verzerrung"], "Dis"),
    (&["microwave", "mikrowelle"], "Mic"),
    (&["ballist"], "Bal"),
    (&["melee", "nahkampf"], "Nah"),
    (&["mining", "bergbau"], "Min"),
    (&["salvage", "verwertung"], "Slv"),
    (&["medical", "medizin"], "Med"),
];

/// Tags that name a kind of weapon (`specs.WEAPON_CLASSES`).
const WEAPON_CLASSES: [&str; 10] = ["Las", "Ele", "Pla", "Dis", "Mic", "Bal", "Nah", "Min", "Slv", "Med"];

/// The seeker of a missile (`specs.GUIDANCE`).
const GUIDANCE: [(&[&str], &str); 3] =
    [(&["infrarot", "infrared"], "IR"), (&["elektromagnet", "electromagnet"], "EM"), (&["querschnitt", "cross"], "CS")];

/// A tag already at the end of a name (`specs.PRESENT_RE`).
static PRESENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"{s}*\((?:[A-Za-z]{{2,4}}|–|-)/(?:\d{{1,2}}|–|-)/(?:[A-Z]|–|-)\){s}*$|{s}*\((?:IR|EM|CS)\d{{1,2}}\){s}*$",
        s = PY_SPACE
    ))
    .expect("pattern")
});

static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^S?(\d{1,2})$").expect("pattern"));

/// One pattern per field spelling, built once.
static FIELD_PATTERNS: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
    SIZE.iter()
        .chain(&GRADE)
        .chain(&CLASS)
        .chain(&GUIDANCE_FIELD)
        .chain(&MAKER)
        .map(|spelling| {
            let pattern = format!(r"{}:{PY_SPACE}*([^\\]+?)(?:\\n|$)", regex::escape(spelling));
            (*spelling, Regex::new(&pattern).expect("pattern"))
        })
        .collect()
});

/// `specs._field`: the value of a field, or `None`.
fn field(text: &str, spellings: &[&str]) -> Option<String> {
    for spelling in spellings {
        if let Some(hit) = FIELD_PATTERNS[spelling].captures(text) {
            let value = py_strip(hit.get(1).map_or("", |m| m.as_str()));
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// `specs._tag`: word part comparison against a table.
fn tag(value: Option<&str>, table: &[(&[&str], &'static str)]) -> Option<&'static str> {
    let lower = value.filter(|v| !v.is_empty())?.to_lowercase();
    table.iter().find(|(parts, _)| parts.iter().any(|part| lower.contains(part))).map(|(_, short)| *short)
}

/// `specs._number`: `S3` or `3` as `3`.
fn number(size: Option<&str>) -> Option<String> {
    let size = size.filter(|s| !s.is_empty())?;
    NUMBER.captures(py_strip(size)).map(|c| c[1].to_string())
}

/// `specs._grade`: `A` to `D`, else `None`.
fn grade(value: Option<&str>) -> Option<String> {
    let short = py_strip(value.filter(|v| !v.is_empty())?).to_uppercase();
    ["A", "B", "C", "D"].contains(&short.as_str()).then_some(short)
}

/// `specs.from_description`: the tag for an item, or `None`.
pub fn from_description(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    if let Some(guidance) = tag(field(text, &GUIDANCE_FIELD).as_deref(), &GUIDANCE) {
        return Some(match number(field(text, &SIZE).as_deref()) {
            Some(size) => format!("({guidance}{size})"),
            None => format!("({guidance})"),
        });
    }
    tag_from_values(
        tag(field(text, &CLASS).as_deref(), &CLASSES),
        number(field(text, &SIZE).as_deref()).as_deref(),
        grade(field(text, &GRADE).as_deref()).as_deref(),
    )
}

/// `specs.class_short`: a class name (`Military`, `Zivil`, `Laser` …) as its tag, or `None`.
pub fn class_short(value: Option<&str>) -> Option<&'static str> {
    tag(value, &CLASSES)
}

/// `specs.tag_from_values`: the tag from values already taken apart.
pub fn tag_from_values(class: Option<&str>, size: Option<&str>, grade: Option<&str>) -> Option<String> {
    if class.is_some_and(|c| WEAPON_CLASSES.contains(&c)) && size.is_none() && grade.is_none() {
        return Some(format!("({})", class.unwrap_or_default()));
    }
    if [class, size, grade].iter().filter(|v| v.is_some()).count() < 2 {
        return None;
    }
    Some(format!("({}/{}/{})", class.unwrap_or("–"), size.unwrap_or("–"), grade.unwrap_or("–")))
}

/// `specs.strip_tag`: cut off a tag added earlier.
pub fn strip_tag(name: &str) -> String {
    PRESENT.replace(name, "").into_owned()
}

/// `specs._loose_stem`.
pub fn loose_stem(stem: &str) -> &str {
    let stem = stem.trim_start_matches('_');
    stem.strip_suffix("_scitem").unwrap_or(stem)
}

/// The first `len` characters of `key` lower case, compared like Python's
/// `lower.startswith(prefix)`; the rest of the lowered key after it.
pub(crate) fn after_prefix(lower: &str, prefix: &str) -> Option<String> {
    lower.strip_prefix(prefix).map(str::to_string)
}

/// `specs.build_table`: name key → tag, from the raw lines of a file.
pub fn build_table<S: AsRef<str>>(lines: &[S]) -> HashMap<String, String> {
    let mut descriptions: HashMap<String, String> = HashMap::new();
    let mut loose: HashMap<String, Vec<String>> = HashMap::new();
    // Name stems in file order; a later line with the same stem replaces
    // the key but keeps the place, like a Python dict.
    let mut name_stems: Vec<(String, String)> = Vec::new();
    let mut name_at: HashMap<String, usize> = HashMap::new();
    for line in lines {
        let line = line.as_ref();
        let Some(sep) = line.find('=') else { continue };
        if sep < 1 {
            continue;
        }
        let key = line[..sep].split(',').next().unwrap_or("");
        let lower = key.to_lowercase();
        let value = &line[sep + 1..];
        if let Some(stem) = after_prefix(&lower, "item_desc") {
            loose.entry(loose_stem(&stem).to_string()).or_default().push(value.to_string());
            descriptions.insert(stem, value.to_string());
        } else if let Some(stem) = after_prefix(&lower, "item_name") {
            match name_at.get(&stem) {
                Some(&at) => name_stems[at].1 = key.to_string(),
                None => {
                    name_at.insert(stem.clone(), name_stems.len());
                    name_stems.push((stem, key.to_string()));
                }
            }
        }
    }
    let mut table = HashMap::new();
    for (stem, key) in name_stems {
        let texts = descriptions.get(&stem).into_iter().chain(loose.get(loose_stem(&stem)).into_iter().flatten());
        if let Some(found) = texts.filter(|t| !t.is_empty()).find_map(|t| from_description(t)) {
            table.insert(key, found);
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_from_descriptions() {
        assert_eq!(
            from_description("Hersteller: X\\nGröße: S1\\nGütegrad: A\\nKlasse: Energie (Laser)\\n").as_deref(),
            Some("(Las/1/A)")
        );
        assert_eq!(from_description("Tracking Signal: Infrared\\nSize: 1").as_deref(), Some("(IR1)"));
        assert_eq!(from_description("Class: Ballistic").as_deref(), Some("(Bal)"));
        assert_eq!(from_description("Size: 3").as_deref(), None);
        assert_eq!(from_description("Size: 3\\nGrade: N/A\\nClass: Civilian").as_deref(), Some("(Civ/3/–)"));
    }

    #[test]
    fn present_tags_are_cut() {
        assert_eq!(strip_tag("Spark I-G Missile (CS1)"), "Spark I-G Missile");
        assert_eq!(strip_tag("Glacier (Mil/1/A) "), "Glacier");
        assert_eq!(strip_tag("Glacier (Military)"), "Glacier (Military)");
    }

    #[test]
    fn exact_stem_before_loose_stem() {
        let lines = [
            "item_DescPOWR_JUST_S03_Ginzel=Size: 3\\nGrade: B\\nClass: Military",
            "item_NamePOWR_JUST_S03_Ginzel_SCItem=Ginzel",
            "item_NameNothing=Nothing",
        ];
        let table = build_table(&lines);
        assert_eq!(table.get("item_NamePOWR_JUST_S03_Ginzel_SCItem").map(String::as_str), Some("(Mil/3/B)"));
        assert_eq!(table.len(), 1);
    }
}
