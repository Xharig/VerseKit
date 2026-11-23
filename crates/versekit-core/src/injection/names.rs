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

//! Names the game data ask for and the English original does not have: the game shows `＠item_Name…` for them.
//!
//! The keys come from the catalog of the game data ([`FILE_KEY`]) and are worked out anew with every writing: a name
//! CIG adds later is in the file and needs nothing from us, and what we wrote before goes away by itself (the same
//! way every other addition is taken back, by the note of original texts).
//!
//! A name is formed only from names of the file itself, so it stands in the language of the translation, and only
//! where such a relative exists; an item with no relative stays without a name (better none than a wrong one):
//!
//! * a **base name** (`ksar_lmg_energy_01`) from the variants of the item that carry a quoted edition name
//!   (`Verdict "Derivation" LMG`): the name without the quoted part, when all such variants agree;
//! * a **magazine** (`<weapon>_mag`) from the name of its weapon and the word and unit the other magazines of the
//!   same kind of ammunition use in this language (`Gallant Rifle Battery (45 cap)`), with the capacity of the game
//!   data;
//! * a **short name** from the full name of the same item.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::Value;

/// The key of the list in the catalog file of the game data.
pub const FILE_KEY: &str = "namen_fehlen";

/// The field names of an entry of the list.
pub mod field {
    /// A short name key: the key of the name of the same item.
    pub const SHORT_OF: &str = "von";
    /// `Type/SubType` of the item.
    pub const KIND: &str = "art";
    /// The capacity of a magazine.
    pub const CAPACITY: &str = "kap";
}

/// The `Type/SubType` of a magazine or battery.
const MAGAZINE: &str = "WeaponAttachment/Magazine";

/// The beginning of every name key, in lower case.
const NAME_PREFIX: &str = "item_name";

/// A name key the game data ask for and the English file lacks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameNeed {
    pub key: String,
    /// For a short name key: the key of the name of the same item.
    pub short_of: Option<String>,
    /// `Type/SubType` of the item.
    pub kind: String,
    /// The capacity of a magazine.
    pub capacity: Option<i64>,
}

/// The list of the catalog file ([`FILE_KEY`]); empty for a catalog without it.
pub fn needs_from_catalog(data: &Value) -> Vec<NameNeed> {
    let Some(Value::Object(map)) = data.get(FILE_KEY) else { return Vec::new() };
    map.iter()
        .map(|(key, entry)| {
            let text = |name: &str| entry.get(name).and_then(Value::as_str).map(str::to_string);
            let capacity = match entry.get(field::CAPACITY) {
                Some(Value::Int(n)) => n.parse::<i64>().ok(),
                _ => None,
            };
            NameNeed {
                key: key.clone(),
                short_of: text(field::SHORT_OF),
                kind: text(field::KIND).unwrap_or_default(),
                capacity,
            }
        })
        .collect()
}

/// How a name was formed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// From the variants with a quoted edition name.
    Base,
    /// From the weapon and the other magazines.
    Magazine,
    /// From the full name of the same item.
    Short,
}

impl How {
    /// The code kept in the note of the added names.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Base => "basis",
            Self::Magazine => "magazin",
            Self::Short => "kurz",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Base, Self::Magazine, Self::Short].into_iter().find(|how| how.code() == code)
    }
}

/// A name formed for a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filled {
    pub key: String,
    pub text: String,
    pub how: How,
    /// The key of the item the name was formed from.
    pub from: String,
}

/// The pairs of quotation marks around an edition name.
const QUOTES: [(char, char); 6] = [('"', '"'), ('„', '“'), ('„', '”'), ('“', '”'), ('«', '»'), ('‹', '›')];

/// `text` without its one quoted part (`Verdict "Derivation" LMG` gives `Verdict LMG`); `None` for a text without
/// one, with several, or with nothing left.
pub fn without_quoted(text: &str) -> Option<String> {
    let mut found = None;
    for (open, close) in QUOTES {
        let places: Vec<(usize, char)> = text.char_indices().filter(|(_, c)| *c == open || *c == close).collect();
        let pair = if open == close {
            match places.as_slice() {
                [(a, _), (b, _)] => Some((*a, *b)),
                _ => None,
            }
        } else {
            match places.as_slice() {
                [(a, x), (b, y)] if *x == open && *y == close => Some((*a, *b)),
                _ => None,
            }
        };
        if let Some(pair) = pair {
            if found.is_some() {
                return None;
            }
            found = Some((pair, close));
        }
    }
    let ((start, end), close) = found?;
    let end = end + close.len_utf8();
    let before = text[..start].trim_end();
    let after = text[end..].trim_start();
    let joined = match (before.is_empty(), after.is_empty()) {
        (true, true) => return None,
        (false, true) => before.to_string(),
        (true, false) => after.to_string(),
        (false, false) => format!("{before} {after}"),
    };
    Some(joined)
}

/// A magazine name: the weapon, the word, the capacity and its unit.
static MAGAZINE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.+?\s+(\S+)\s+\(\s*\d+\s+([^()\d][^()]*?)\s*\)\s*$").expect("pattern"));

/// The kind of ammunition a weapon key stands for (`energy` in `ksar_lmg_energy_01_mag`): the third part of the key
/// after `item_Name`; `None` for a key with fewer parts.
fn ammunition(key: &str) -> Option<String> {
    let rest = key.get(NAME_PREFIX.len()..)?.to_lowercase();
    let parts: Vec<&str> = rest.split('_').collect();
    (parts.len() >= 4).then(|| parts[2].to_string())
}

/// The most frequent of `values`, the first in alphabetical order among equals.
fn most_frequent(values: Vec<String>) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    let mut counts: Vec<(String, usize)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts.into_iter().next().map(|(value, _)| value)
}

/// Forms the names of `needs` that no line of the file has. `names` are the names of the file by key in lower case
/// as `(key, text)`, the text without a tag of ours or a foreign one; `present` holds the keys of every line of the
/// file in lower case (a key with a line is never filled, whatever it says).
pub fn fill(names: &HashMap<String, (String, String)>, present: &HashSet<String>, needs: &[NameNeed]) -> Vec<Filled> {
    let open: Vec<&NameNeed> = needs
        .iter()
        .filter(|need| !need.key.is_empty() && !present.contains(&need.key.to_lowercase()))
        .collect();
    let mut formed: HashMap<String, Filled> = HashMap::new();

    // Base names, from the variants with a quoted edition name.
    for need in open.iter().filter(|need| need.short_of.is_none() && need.kind != MAGAZINE) {
        let lower = need.key.to_lowercase();
        let prefix = format!("{lower}_");
        let mut results: Vec<(String, &str)> = Vec::new();
        for (variant, (key, text)) in names {
            let Some(rest) = variant.strip_prefix(&prefix) else { continue };
            if rest.ends_with("_short") || rest.ends_with("_mag") || rest == "short" || rest == "mag" {
                continue;
            }
            if let Some(base) = without_quoted(text) {
                results.push((base, key));
            }
        }
        results.sort();
        if let Some((base, from)) = results.first().cloned()
            && results.iter().all(|(other, _)| *other == base)
        {
            formed.insert(
                lower,
                Filled { key: need.key.clone(), text: base, how: How::Base, from: from.to_string() },
            );
        }
    }

    // Magazines, from the weapon and the other magazines of the same kind of ammunition.
    for need in open.iter().filter(|need| need.kind == MAGAZINE) {
        let lower = need.key.to_lowercase();
        let Some(weapon) = lower.strip_suffix("_mag") else { continue };
        let weapon_name = names
            .get(weapon)
            .map(|(key, text)| (key.clone(), text.clone()))
            .or_else(|| formed.get(weapon).map(|f| (f.key.clone(), f.text.clone())));
        let Some((weapon_key, weapon_name)) = weapon_name else { continue };
        let Some(ammo) = ammunition(&need.key) else { continue };
        let mut words = Vec::new();
        let mut units = Vec::new();
        for (other, (other_key, text)) in names {
            if !other.ends_with("_mag") || ammunition(other_key).as_deref() != Some(ammo.as_str()) {
                continue;
            }
            if let Some(hit) = MAGAZINE_NAME.captures(text) {
                words.push(hit[1].to_string());
                units.push(hit[2].to_string());
            }
        }
        // Two magazines at least have to say how this language names them.
        if words.len() < 2 {
            continue;
        }
        let (Some(word), Some(unit)) = (most_frequent(words), most_frequent(units)) else { continue };
        let text = match need.capacity {
            Some(capacity) => format!("{weapon_name} {word} ({capacity} {unit})"),
            None => format!("{weapon_name} {word}"),
        };
        formed.insert(lower, Filled { key: need.key.clone(), text, how: How::Magazine, from: weapon_key });
    }

    // Short names, from the full name of the same item.
    for need in &open {
        let Some(full) = &need.short_of else { continue };
        let lower_full = full.to_lowercase();
        let name = names
            .get(&lower_full)
            .map(|(_, text)| text.clone())
            .or_else(|| formed.get(&lower_full).map(|f| f.text.clone()));
        if let Some(text) = name.filter(|text| !text.is_empty()) {
            formed.insert(
                need.key.to_lowercase(),
                Filled { key: need.key.clone(), text, how: How::Short, from: full.clone() },
            );
        }
    }

    open.iter().filter_map(|need| formed.remove(&need.key.to_lowercase())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(lines: &[(&str, &str)]) -> HashMap<String, (String, String)> {
        lines.iter().map(|(k, v)| (k.to_lowercase(), (k.to_string(), v.to_string()))).collect()
    }

    fn present(lines: &[(&str, &str)]) -> HashSet<String> {
        lines.iter().map(|(k, _)| k.to_lowercase()).collect()
    }

    fn need(key: &str) -> NameNeed {
        NameNeed { key: key.into(), ..NameNeed::default() }
    }

    fn mag(key: &str, capacity: Option<i64>) -> NameNeed {
        NameNeed { key: key.into(), kind: MAGAZINE.into(), capacity, ..NameNeed::default() }
    }

    fn short(key: &str, of: &str) -> NameNeed {
        NameNeed { key: key.into(), short_of: Some(of.into()), ..NameNeed::default() }
    }

    const FILE: [(&str, &str); 9] = [
        ("item_Nameksar_lmg_energy_01_store01", "Verdict \"Derivation\" LMG"),
        ("item_Nameklwe_lmg_energy_01_mag", "Demeco LMG Battery (100 cap)"),
        ("item_Namevolt_lmg_energy_01_mag", "Fresnel Energy LMG Battery (165 Cap)"),
        ("item_Namevolt_rifle_energy_01_mag", "Parallax Rifle Battery (80 Cap)"),
        ("item_Namebehr_rifle_ballistic_01_mag", "P4-AR Magazine (40 cap)"),
        ("item_Namebehr_pistol_ballistic_01_mag", "S-38 Magazine (20 cap)"),
        ("item_Nameglsn_shotgun_ballistic_01", "Salvation Shotgun"),
        ("item_Nameglsn_shotgun_ballistic_01_short", "Salvation Shotgun"),
        ("item_Namegmni_rifle_ballistic_01_mag", "S71 Rifle Magazine (30 cap)"),
    ];

    #[test]
    fn a_base_name_comes_from_the_quoted_variants_without_the_edition() {
        let formed = fill(&names(&FILE), &present(&FILE), &[need("item_Nameksar_lmg_energy_01")]);
        assert_eq!(formed.len(), 1, "{formed:?}");
        assert_eq!(formed[0].text, "Verdict LMG");
        assert_eq!(formed[0].how, How::Base);
        assert_eq!(formed[0].from, "item_Nameksar_lmg_energy_01_store01");
    }

    #[test]
    fn variants_that_disagree_give_no_name() {
        let file = [
            ("item_Nameaaa_rifle_energy_01_tint01", "Alpha \"One\" Rifle"),
            ("item_Nameaaa_rifle_energy_01_tint02", "Beta \"Two\" Rifle"),
        ];
        assert!(fill(&names(&file), &present(&file), &[need("item_Nameaaa_rifle_energy_01")]).is_empty());
        // A variant without a quoted part says nothing about the base.
        let plain = [("item_Nameaaa_rifle_energy_01_iron", "Alpha Iron Sight")];
        assert!(fill(&names(&plain), &present(&plain), &[need("item_Nameaaa_rifle_energy_01")]).is_empty());
    }

    #[test]
    fn a_magazine_takes_the_word_and_unit_of_its_kind_and_the_capacity_of_the_game_data() {
        let needs = [need("item_Nameksar_lmg_energy_01"), mag("item_Nameksar_lmg_energy_01_mag", Some(60))];
        let formed = fill(&names(&FILE), &present(&FILE), &needs);
        let magazine = formed.iter().find(|f| f.how == How::Magazine).expect("magazine");
        assert_eq!(magazine.text, "Verdict LMG Battery (60 Cap)", "the word of the energy weapons, their commonest unit");
        assert_eq!(magazine.from, "item_Nameksar_lmg_energy_01");
        // Ballistic magazines are no batteries.
        let formed = fill(&names(&FILE), &present(&FILE), &[mag("item_Nameglsn_shotgun_ballistic_01_mag", Some(12))]);
        assert_eq!(formed[0].text, "Salvation Shotgun Magazine (12 cap)");
        // Without a capacity the name has none.
        let formed = fill(&names(&FILE), &present(&FILE), &[mag("item_Nameglsn_shotgun_ballistic_01_mag", None)]);
        assert_eq!(formed[0].text, "Salvation Shotgun Magazine");
    }

    #[test]
    fn a_magazine_without_a_named_weapon_or_without_witnesses_stays_empty() {
        // The weapon has no name.
        assert!(fill(&names(&FILE), &present(&FILE), &[mag("item_Nameasad_rifle_energy_01_mag", Some(30))]).is_empty());
        // One magazine of the kind is no witness of how the language words it.
        let file = [("item_Nameabc_rifle_multi_01", "Rifle X"), ("item_Namexyz_rifle_multi_01_mag", "Rifle Y Magazine (5 cap)")];
        assert!(fill(&names(&file), &present(&file), &[mag("item_Nameabc_rifle_multi_01_mag", Some(30))]).is_empty());
    }

    #[test]
    fn a_short_name_is_the_full_name_of_the_same_item() {
        let file = [("item_Namebehr_smg_ballistic_01", "P8-SC SMG")];
        let formed = fill(&names(&file), &present(&file), &[short("item_Namebehr_smg_ballistic_01_short", "item_Namebehr_smg_ballistic_01")]);
        assert_eq!(formed.len(), 1);
        assert_eq!(formed[0].text, "P8-SC SMG");
        assert_eq!(formed[0].how, How::Short);
        // The short name of a base name that was formed itself.
        let needs = [need("item_Nameksar_lmg_energy_01"), short("item_Nameksar_lmg_energy_01_short", "item_Nameksar_lmg_energy_01")];
        let formed = fill(&names(&FILE), &present(&FILE), &needs);
        assert_eq!(formed.iter().find(|f| f.how == How::Short).map(|f| f.text.as_str()), Some("Verdict LMG"));
    }

    #[test]
    fn a_name_the_file_has_is_never_filled_and_a_short_name_of_nothing_stays_empty() {
        let file = [("item_Namekeep", "Mine")];
        let needs = [need("item_Namekeep"), short("item_Namegone_short", "item_Namegone")];
        assert!(fill(&names(&file), &present(&file), &needs).is_empty());
    }

    #[test]
    fn quoted_parts_in_the_quotes_of_several_languages() {
        assert_eq!(without_quoted("Verdict \"Derivation\" LMG").as_deref(), Some("Verdict LMG"));
        assert_eq!(without_quoted("Verdict „Derivation“ LMG").as_deref(), Some("Verdict LMG"));
        assert_eq!(without_quoted("Verdict «Derivation» LMG").as_deref(), Some("Verdict LMG"));
        assert_eq!(without_quoted("Verdict \"A\" LMG \"B\"").as_deref(), None);
        assert_eq!(without_quoted("Verdict LMG").as_deref(), None);
        assert_eq!(without_quoted("\"Only\"").as_deref(), None);
    }

    #[test]
    fn the_list_of_the_catalog_file_is_read() {
        let data = crate::jsonio::loads(
            r#"{"namen_fehlen": {"item_NameA_mag": {"art": "WeaponAttachment/Magazine", "kap": 30},
                "item_NameA_short": {"von": "item_NameA", "art": "WeaponPersonal/Small"}}}"#,
        )
        .unwrap();
        let needs = needs_from_catalog(&data);
        assert_eq!(needs.len(), 2);
        assert_eq!(needs[0].capacity, Some(30));
        assert_eq!(needs[0].kind, MAGAZINE);
        assert_eq!(needs[1].short_of.as_deref(), Some("item_NameA"));
        assert!(needs_from_catalog(&crate::jsonio::loads("{}").unwrap()).is_empty());
    }
}
