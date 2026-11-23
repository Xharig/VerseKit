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

//! The item names the game data ask for and the English `global.ini` of the same archive does not have.
//!
//! Every item states the text key of its name (`AttachDef.Localization.Name`) and of its short name (`ShortName`).
//! Where the English file has no line for such a key, the game shows the bare key (`＠item_Name…`). The keys are read
//! anew with every build of the catalog, so a name CIG adds later drops out of the list by itself.
//!
//! An item counts only when it is a real one: the record path and name hold no word of a development leftover (test,
//! template, dummy, debug, deprecated, dev, temp, tmp, wip, unused). Only keys that begin with `item_Name` are kept:
//! they are the ones a name can be formed for from the names of related items.

use p4kx_dataforge::DataForge;

use versekit_core::injection::names::{FILE_KEY, field};
use versekit_core::jsonio::{Map, Value};

use crate::Localization;
use crate::catalog::{is_leftover, starts_with_ignore_case, text_key};
use crate::fields::{Trees, class_name, component, integer, locale_key, structure, text};

/// The record path prefix of the items.
const ITEM_PREFIX: &str = "libs/foundry/records/entities/scitem/";

/// Further words of a leftover that the blueprint catalog does not know.
const MORE_LEFTOVER_WORDS: [&str; 4] = ["temp", "tmp", "wip", "unused"];

/// Whether `text` holds one of the leftover words as a whole word, ignoring case.
fn leftover(text: &str) -> bool {
    is_leftover(text)
        || text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| MORE_LEFTOVER_WORDS.iter().any(|leftover| word.eq_ignore_ascii_case(leftover)))
}

/// One key the English file lacks, with what is known of the items that ask for it.
#[derive(Default)]
struct Need {
    /// For a short name key: the key of the name of the same item.
    short_of: Option<String>,
    /// `Type/SubType` of the item.
    kind: String,
    /// The capacity of a magazine.
    capacity: Option<i64>,
}

/// The name keys the DataForge asks for and `texts` (the English file) lacks, in the form the catalog file keeps
/// them ([`FILE_KEY`]): key → `{"von": …, "art": …, "kap": …}`, in the order of the records.
pub fn read(forge: &DataForge<'_>, texts: &Localization) -> Map {
    let mut trees = Trees::new(forge);
    let items: Vec<_> = forge
        .records()
        .filter(|record| {
            matches!(record.type_name(), Ok("EntityClassDefinition"))
                && record.record_path().is_ok_and(|path| starts_with_ignore_case(path, ITEM_PREFIX) && !leftover(path))
        })
        .map(|record| record.guid())
        .collect();
    let mut found: Vec<(String, Need)> = Vec::new();
    for guid in items {
        let Some(tree) = trees.once(guid) else { continue };
        if leftover(class_name(&tree)) {
            continue;
        }
        let attach = component(&tree, "SAttachableComponentParams");
        let definition = structure(attach, "AttachDef");
        let localization = structure(definition, "Localization");
        let kind = format!(
            "{}/{}",
            text(definition, "Type").unwrap_or_default(),
            text(definition, "SubType").unwrap_or_default()
        );
        let capacity = integer(component(&tree, "SAmmoContainerComponentParams"), "maxAmmoCount");
        let name = locale_key(localization, "Name");
        let short = locale_key(localization, "ShortName");
        let mut lacking = |raw: Option<&str>, short_of: Option<&str>| {
            let Some(raw) = raw else { return };
            if texts.english(raw).is_some() {
                return;
            }
            let Some(key) = text_key(Some(raw)).filter(|key| key.to_ascii_lowercase().starts_with("item_name")) else {
                return;
            };
            if found.iter().any(|(known, _)| known.eq_ignore_ascii_case(&key)) {
                return;
            }
            found.push((key, Need { short_of: short_of.map(str::to_string), kind: kind.clone(), capacity }));
        };
        let name_key = name.and_then(|raw| text_key(Some(raw)));
        lacking(name, None);
        lacking(short, name_key.as_deref());
    }
    found
        .into_iter()
        .map(|(key, need)| {
            let mut entry = Map::new();
            if let Some(of) = need.short_of {
                entry.push((field::SHORT_OF.into(), Value::str(of)));
            }
            entry.push((field::KIND.into(), Value::str(need.kind)));
            if let Some(capacity) = need.capacity {
                entry.push((field::CAPACITY.into(), Value::int(capacity)));
            }
            (key, Value::Object(entry))
        })
        .collect()
}

/// The key of the list in the catalog file.
pub const CATALOG_KEY: &str = FILE_KEY;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test and placeholder items are no items of the game: whole words of the record path or name only.
    #[test]
    fn leftover_words_are_found_as_whole_words() {
        assert!(leftover("scitem/ships/paints/paint_redeemer_template"));
        assert!(leftover("RADR_GNRP_S03_Idris_TEMP"));
        assert!(leftover("Flair_Hanging_Template"));
        assert!(leftover("weapon_test_01"));
        assert!(leftover("Usable_Puzzle_Valve_TEMPLATE"));
        assert!(leftover("item_wip"));
        assert!(!leftover("scitem/weapons/fps_weapons/ksar_lmg_energy_01"));
        assert!(!leftover("scitem/ships/paints/paint_cutter_black_silver_stripe"));
        assert!(!leftover("Attestation_Helmet"), "a word inside another word is no leftover word");
        assert!(!leftover("Contempt_Rifle"));
    }
}
