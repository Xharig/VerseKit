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

//! The keys of the game actions the plugin presses itself while VerseKit does not run: the table VerseKit gave last
//! (the player's own bindings, names in the language of VerseKit), kept in a file of its own
//! ([`versekit_core::streamdeck::KEY_TABLE_FILE`]); without one, the factory keys the plugin brings
//! (`werkstasten.json`, worked out from the game by `tests/deck_keys_real.rs`).

use std::collections::HashMap;

use serde_json::{Value, json};
use versekit_core::controls::Timing;
use versekit_core::language::Lang;

/// The factory keys of the game actions of the shipped profiles: `aktionen` → `{eingabe, halten}`.
const FACTORY: &str = include_str!("../../../streamdeck-plugin/werkstasten.json");

/// One game action as the plugin presses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The input as the game writes it (`rctrl+a`, `mouse4`).
    pub input: String,
    pub timing: Timing,
    /// The name of the game, in the language of the table; empty in the factory table.
    pub name: String,
    /// The `actionmap` of the action; empty in the factory table.
    pub map: String,
}

/// The keys of the game actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The language of VerseKit when it gave the table.
    pub lang: Lang,
    pub entries: HashMap<String, Entry>,
    /// Given by VerseKit (`true`) or the factory keys the plugin brings.
    pub from_verse: bool,
    /// The switch of VerseKit that shows the key on every game action key, as VerseKit said it last.
    pub keys_shown: bool,
}

fn lang_word(lang: Lang) -> &'static str {
    if lang == Lang::En { "en" } else { "de" }
}

impl Table {
    /// The table of the answer of VerseKit (`/v1/belegung`): every action with an input on the keyboard or mouse.
    pub fn from_bindings(value: &Value, lang: Lang) -> Table {
        let mut entries = HashMap::new();
        for item in value["aktionen"].as_array().into_iter().flatten() {
            let (Some(id), Some(input)) = (item["id"].as_str(), item["eingabe"].as_str().filter(|i| !i.is_empty())) else {
                continue;
            };
            let entry = Entry {
                input: input.to_string(),
                timing: item["halten"].as_str().and_then(Timing::parse).unwrap_or_default(),
                name: item["name"].as_str().unwrap_or("").to_string(),
                map: item["karte"].as_str().unwrap_or("").to_string(),
            };
            entries.insert(id.to_string(), entry);
        }
        Table { lang, entries, from_verse: true, keys_shown: false }
    }

    /// The factory keys the plugin brings.
    pub fn factory() -> Table {
        let mut table = Table::parse(FACTORY).unwrap_or(Table { lang: Lang::De, entries: HashMap::new(), from_verse: false, keys_shown: false });
        table.from_verse = false;
        table
    }

    pub fn to_text(&self) -> String {
        let entries: serde_json::Map<String, Value> = self
            .entries
            .iter()
            .map(|(id, e)| {
                (id.clone(), json!({"eingabe": e.input, "halten": e.timing.word(), "name": e.name, "karte": e.map}))
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "fassung": 1,
            "sprache": lang_word(self.lang),
            "von_versekit": self.from_verse,
            "tasten_anzeigen": self.keys_shown,
            "aktionen": entries
        }))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Option<Table> {
        let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        if value["fassung"].as_u64() != Some(1) {
            return None;
        }
        let lang = if value["sprache"] == "en" { Lang::En } else { Lang::De };
        let entries = value["aktionen"]
            .as_object()?
            .iter()
            .filter_map(|(id, e)| {
                let input = e["eingabe"].as_str().filter(|i| !i.is_empty())?.to_string();
                let timing = e["halten"].as_str().and_then(Timing::parse).unwrap_or_default();
                let text = |name: &str| e[name].as_str().unwrap_or("").to_string();
                Some((id.clone(), Entry { input, timing, name: text("name"), map: text("karte") }))
            })
            .collect();
        Some(Table {
            lang,
            entries,
            from_verse: value["von_versekit"].as_bool().unwrap_or(false),
            keys_shown: value["tasten_anzeigen"].as_bool().unwrap_or(false),
        })
    }

    /// The table of the file `path`; the factory keys where there is none or it cannot be read.
    pub fn load(path: Option<&std::path::Path>) -> Table {
        path.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| Table::parse(&t)).unwrap_or_else(Table::factory)
    }

    /// Keeps the table in the file `path` (whole or not at all).
    pub fn save(&self, path: &std::path::Path) {
        let _ = versekit_core::jsonio::write_text_atomic(&path.to_string_lossy(), &self.to_text(), versekit_core::jsonio::Newline::Keep);
    }

    /// The entry of a game action, also of a qualified name (`spaceship_view.v_view_zoom_in`) by its plain name.
    pub fn get(&self, action: &str) -> Option<&Entry> {
        self.entries.get(action).or_else(|| action.split_once('.').and_then(|(_, plain)| self.entries.get(plain)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_of_verse_kit_goes_out_and_comes_back_and_the_factory_table_is_there() {
        let answer = json!({"aktionen": [
            {"id": "v_toggle_guns_mode", "eingabe": "rctrl+np_4", "halten": "folgen", "name": "Waffenmodus", "karte": "seat_general"},
            {"id": "v_master_mode_cycle_long", "eingabe": "b", "halten": "mindestens:250", "name": "Modus", "karte": "spaceship_movement"},
            {"id": "v_on_stick", "eingabe": "", "name": "Nur Stick"},
        ]});
        let table = Table::from_bindings(&answer, Lang::De);
        assert_eq!(table.entries.len(), 2, "an action without an input is left out");
        assert_eq!(table.get("v_master_mode_cycle_long").map(|e| e.timing), Some(Timing::AtLeast(250)));
        assert_eq!(Table::parse(&table.to_text()), Some(table.clone()));
        let factory = Table::factory();
        assert!(!factory.from_verse);
        assert_eq!(factory.get("v_power_toggle_weapons").map(|e| e.input.as_str()), Some("p"), "the factory key");
        assert_eq!(table.get("spaceship_view.v_toggle_guns_mode"), table.get("v_toggle_guns_mode"), "a qualified name");
        let folder = std::env::temp_dir().join(format!("versekit-sd-tabelle-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let file = folder.join("streamdeck_tasten.json");
        assert_eq!(Table::load(Some(&file)), factory, "no file: the factory keys");
        table.save(&file);
        assert_eq!(Table::load(Some(&file)), table, "the kept table");
        let _ = std::fs::remove_dir_all(folder);
    }
}
