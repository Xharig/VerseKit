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
//! The field names and word parts are matched in every known language at
//! once (German, English, French); a spelling a file does not use cannot
//! give a false hit. Space before the colon counts too (French writes
//! `Taille : 1`). Where the description of a file gives no tag (a language
//! whose words are not known here), the tag of the English original stands
//! in for it, by the same key ([`ORIGINAL_FILE`], [`fill_from_original`]).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::text::{PY_SPACE, py_strip};

/// Field names per field (`specs.FIELDS`), in the order they are tried.
const SIZE: [&str; 4] = ["Größe", "Grösse", "Size", "Taille"];
const GRADE: [&str; 8] = ["Gütegrad", "Guetegrad", "Güteklasse", "Güte", "Grade", "Grad", "Qualité", "Catégorie"];
const CLASS: [&str; 3] = ["Klasse", "Class", "Classe"];
const GUIDANCE_FIELD: [&str; 5] =
    ["Verfolgungssignal", "Tracking Signal", "Signature Type", "Signature", "Signal de suivi"];
const MAKER: [&str; 3] = ["Hersteller", "Manufacturer", "Fabricant"];

/// Word parts and their tags (`specs.CLASSES`), lower case.
const CLASSES: [(&[&str], &str); 15] = [
    (&["civilian", "zivil", "civil"], "Civ"),
    (&["military", "militär", "militaer", "militaire"], "Mil"),
    (&["industrial", "industrie"], "Ind"),
    (&["stealth", "tarnung", "furtiv"], "Sth"),
    (&["competition", "wettkampf", "wettbewerb", "compétition"], "Cmp"),
    (&["laser"], "Las"),
    (&["elektron", "electron", "électron"], "Ele"),
    (&["plasma"], "Pla"),
    (&["distortion", "verzerrung", "distorsion"], "Dis"),
    (&["microwave", "mikrowelle", "micro-onde"], "Mic"),
    (&["ballist", "balisti"], "Bal"),
    (&["melee", "nahkampf", "mêlée"], "Nah"),
    (&["mining", "bergbau", "minage"], "Min"),
    (&["salvage", "verwertung", "récupération"], "Slv"),
    (&["medical", "medizin", "médical"], "Med"),
];

/// Tags that name a kind of weapon (`specs.WEAPON_CLASSES`).
const WEAPON_CLASSES: [&str; 10] = ["Las", "Ele", "Pla", "Dis", "Mic", "Bal", "Nah", "Min", "Slv", "Med"];

/// The seeker of a missile (`specs.GUIDANCE`).
const GUIDANCE: [(&[&str], &str); 3] = [
    (&["infrarot", "infrared", "infrarouge"], "IR"),
    (&["elektromagnet", "electromagnet", "électromagn"], "EM"),
    (&["querschnitt", "cross", "section"], "CS"),
];

/// A tag already at the end of a name (`specs.PRESENT_RE`).
static PRESENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"{s}*\((?:[A-Za-z]{{2,4}}|–|-)/(?:\d{{1,2}}|–|-)/(?:[A-Z]|–|-)\){s}*$|{s}*\((?:IR|EM|CS)\d{{1,2}}\){s}*$",
        s = PY_SPACE
    ))
    .expect("pattern")
});

static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^S?(\d{1,2})$").expect("pattern"));

/// One pattern per field spelling, built once. A field starts its line (`Size:`, never the end of `Magazine Size:`
/// or `Battery Size:`, which would give a weapon the size of its magazine).
static FIELD_PATTERNS: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
    SIZE.iter()
        .chain(&GRADE)
        .chain(&CLASS)
        .chain(&GUIDANCE_FIELD)
        .chain(&MAKER)
        .map(|spelling| {
            let pattern =
                format!(r"(?:^|\\n){PY_SPACE}*{}{PY_SPACE}*:{PY_SPACE}*([^\\]+?)(?:\\n|$)", regex::escape(spelling));
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

/// The tags of the English original, read from the archive with its names: `{name key in lower case: tag}`.
pub const ORIGINAL_FILE: &str = "kurzzeichen-original.json";

/// Stores the tags of the English original `lines` ([`build_table`]) in [`ORIGINAL_FILE`], none as well (the
/// original was read); gives how many.
pub fn save_original<S: AsRef<str>>(paths: &Paths, lines: &[S]) -> usize {
    let mut tags: Vec<(String, String)> =
        build_table(lines).into_iter().map(|(key, tag)| (key.to_lowercase(), tag)).collect();
    tags.sort();
    let value = Value::Object(tags.iter().map(|(key, tag)| (key.clone(), Value::str(tag.as_str()))).collect());
    match jsonio::write_text_atomic(&paths.app_file(ORIGINAL_FILE), &jsonio::dumps(&value, None, false), jsonio::Newline::Keep)
    {
        Ok(()) => tags.len(),
        Err(error) => {
            crate::errors::record("injection.kurzzeichen", &error);
            0
        }
    }
}

/// Whether the tags of the English original are stored.
pub fn original_stored(paths: &Paths) -> bool {
    std::path::Path::new(&paths.app_file(ORIGINAL_FILE)).is_file()
}

/// The stored tags of the English original ([`save_original`]); empty without them.
pub fn load_original(paths: &Paths) -> HashMap<String, String> {
    match jsonio::read_json(&paths.app_file(ORIGINAL_FILE)) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(key, value)| match value {
                Value::Str(tag) => Some((key, tag)),
                _ => None,
            })
            .collect(),
        _ => HashMap::new(),
    }
}

/// Every name key of `lines` takes the tag of the English `original` (keys in lower case) under the same key where
/// `table` gives none or another one, unless the original's has more gaps (`–`): the key is the same in every
/// language, so a language whose words are not known, or known only in part, gets its tags, and a translation that
/// tells a value wrong does not win over the game. (The French text of `item_DescSHLD_ASAS_S01_Veil` says
/// `Taille : 2`, the English one `Size: 1`, as does the other description of the Veil.)
pub fn fill_from_original<S: AsRef<str>>(
    table: &mut HashMap<String, String>,
    lines: &[S],
    original: &HashMap<String, String>,
) {
    if original.is_empty() {
        return;
    }
    for line in lines {
        let line = line.as_ref();
        let Some(sep) = line.find('=').filter(|sep| *sep > 0) else { continue };
        let key = line[..sep].split(',').next().unwrap_or("");
        let lower = key.to_lowercase();
        if after_prefix(&lower, "item_name").is_none() {
            continue;
        }
        let gaps = |tag: &str| tag.matches('–').count();
        let Some(tag) = original.get(&lower) else { continue };
        if table.get(key).is_none_or(|own| own != tag && gaps(tag) <= gaps(own)) {
            table.insert(key.to_string(), tag.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// French writes a space before the colon and its own words: every field and class is read.
    #[test]
    fn french_descriptions_give_their_tags() {
        assert_eq!(from_description("Taille : 1\\nGrade : C\\nClasse : Compétition").as_deref(), Some("(Cmp/1/C)"));
        assert_eq!(
            from_description("Fabricant : Behring\\nTaille : 3\\nGrade : B\\nClasse : Militaire\\n").as_deref(),
            Some("(Mil/3/B)")
        );
        assert_eq!(from_description("Classe : Énergie (Électron)").as_deref(), Some("(Ele)"));
        assert_eq!(from_description("Classe : Balistique").as_deref(), Some("(Bal)"));
        assert_eq!(from_description("Taille : 2\\nGrade : A\\nClasse : Furtivité").as_deref(), Some("(Sth/2/A)"));
        assert_eq!(from_description("Taille : 2\\nGrade : D\\nClasse : Civile").as_deref(), Some("(Civ/2/D)"));
        assert_eq!(from_description("Signal de suivi : Infrarouge\\nTaille : 1").as_deref(), Some("(IR1)"));
        assert_eq!(from_description("Signal de suivi : Électromagnétique\\nTaille : 2").as_deref(), Some("(EM2)"));
        assert_eq!(from_description("Signal de suivi : Section transversale\\nTaille : 3").as_deref(), Some("(CS3)"));
        // Another field that starts with the same word is no size.
        assert_eq!(from_description("Taille du chargeur : 25\\nClasse : Balistique").as_deref(), Some("(Bal)"));
        assert_eq!(from_description("Taille : 0\\nQualité : B\\nClasse : Civile").as_deref(), Some("(Civ/0/B)"));
        assert_eq!(from_description("Taille : 1\\nCatégorie : C\\nClasse : Civil").as_deref(), Some("(Civ/1/C)"));
    }

    /// A field that only ends with the name of another is not that field: a weapon never gets the size of its
    /// magazine or battery.
    #[test]
    fn the_size_of_a_magazine_is_no_size() {
        let rifle = "Manufacturer: Behring\\nItem Type: Assault Rifle\\nClass: Ballistic\\n\\nMagazine Size: 45\\nRate Of Fire: 800 rpm";
        assert_eq!(from_description(rifle).as_deref(), Some("(Bal)"));
        assert_eq!(from_description("Class: Energy (Laser)\\nBattery Size: 60").as_deref(), Some("(Las)"));
        assert_eq!(from_description("Size: 2\\nGrade: B\\nClass: Military").as_deref(), Some("(Mil/2/B)"));
    }

    /// A language whose words are not known: the tag of the English original by the same key; a tag of the file
    /// itself that tells other values gives way to the original's.
    #[test]
    fn a_language_without_known_words_takes_the_tags_of_the_english_original() {
        let english = [
            "item_DescPOWR_A=Size: 3\\nGrade: B\\nClass: Military",
            "item_NamePOWR_A=Ginzel",
            "item_DescCOOL_B=Size: 1\\nGrade: A\\nClass: Civilian",
            "item_NameCOOL_B=Glacier",
        ];
        let original: HashMap<String, String> =
            build_table(&english).into_iter().map(|(key, tag)| (key.to_lowercase(), tag)).collect();
        let other = [
            "item_DescPOWR_A=Tamaño: 3\\nGrado: B\\nClase: Militar",
            "item_NamePOWR_A=Ginzel",
            "item_DescCOOL_B=Größe: 2\\nGütegrad: C\\nKlasse: Industrie",
            "item_NameCOOL_B=Glacier",
            "item_NameNONE=Nothing",
        ];
        let mut table = build_table(&other);
        assert_eq!(table.len(), 1, "only the known words give a tag: {table:?}");
        fill_from_original(&mut table, &other, &original);
        assert_eq!(table.get("item_NamePOWR_A").map(String::as_str), Some("(Mil/3/B)"));
        assert_eq!(table.get("item_NameCOOL_B").map(String::as_str), Some("(Civ/1/A)"), "the game wins");
        assert_eq!(table.len(), 2);

        // Known in part (Dutch: class and grade read, the size not): the gap is filled from the original.
        let dutch = ["item_DescPOWR_A=Grootte: 3\\nGrade: B\\nClass: Military", "item_NamePOWR_A=Ginzel"];
        let mut table = build_table(&dutch);
        assert_eq!(table.get("item_NamePOWR_A").map(String::as_str), Some("(Mil/–/B)"), "the own tag has a gap");
        fill_from_original(&mut table, &dutch, &original);
        assert_eq!(table.get("item_NamePOWR_A").map(String::as_str), Some("(Mil/3/B)"));
    }

    /// The French translation has two descriptions of the Veil, `Taille : 2` under `item_DescSHLD_ASAS_S01_Veil` and
    /// `Taille : 1` under `item_Desc_SHLD_ASAS_S01_Veil`; the English original says `Size: 1` for both (lines of
    /// LIVE 4.10.3). Both names of the Veil get size 1.
    #[test]
    fn the_veil_is_size_one_in_french_as_in_the_game() {
        let english = [
            "item_DescSHLD_ASAS_S01_Veil=Item Type: Shield Generator\\nManufacturer: Ascension Astro\\nSize: 1\\nGrade: B\\nClass: Stealth",
            "item_Desc_SHLD_ASAS_S01_Veil=Item Type: Shield Generator\\nManufacturer: Ascension Astro\\nSize: 1\\nGrade: B\\nClass: Stealth\\n\\nAn advanced cooling system",
            "item_NameSHLD_ASAS_S01_Veil=Veil",
            "item_Name_SHLD_ASAS_S01_Veil=Veil",
        ];
        let original: HashMap<String, String> =
            build_table(&english).into_iter().map(|(key, tag)| (key.to_lowercase(), tag)).collect();
        let french = [
            "item_DescSHLD_ASAS_S01_Veil=Type d'objet : Générateur de bouclier\\nFabricant : Ascension Astro\\nTaille : 2\\nGrade : B\\nClasse : Furtivité",
            "item_Desc_SHLD_ASAS_S01_Veil=Type d'objet : Générateur de bouclier\\nFabricant : Ascension Astro\\nTaille : 1\\nGrade : B\\nClasse : Furtivité\\n\\nUn système de refroidissement",
            "item_NameSHLD_ASAS_S01_Veil=Veil",
            "item_Name_SHLD_ASAS_S01_Veil=Veil",
        ];
        let mut table = build_table(&french);
        assert_eq!(table.get("item_NameSHLD_ASAS_S01_Veil").map(String::as_str), Some("(Sth/2/B)"), "as written");
        fill_from_original(&mut table, &french, &original);
        for key in ["item_NameSHLD_ASAS_S01_Veil", "item_Name_SHLD_ASAS_S01_Veil"] {
            assert_eq!(table.get(key).map(String::as_str), Some("(Sth/1/B)"), "{key}");
        }
        // An original with a gap does not take a full tag of the translation away.
        let gap = HashMap::from([("item_name_shld_asas_s01_veil".to_string(), "(Sth/–/B)".to_string())]);
        let mut table = build_table(&french);
        fill_from_original(&mut table, &french, &gap);
        assert_eq!(table.get("item_Name_SHLD_ASAS_S01_Veil").map(String::as_str), Some("(Sth/1/B)"));
    }

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
