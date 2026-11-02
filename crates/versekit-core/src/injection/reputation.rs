// Verse-Kit — companion for Star Citizen.
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

//! Whom a contract credits reputation, of which kind, and where it plays:
//! the stored table of `reputation.py` (`auftragsruf.json`) and the lines
//! it gives for a contract text.
//!
//! The table is built from scmdb's contract file
//! (`versekit_gamedata::contract_info`); only the result is stored, bound to
//! the game version.

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

pub const CACHE_FILE: &str = "auftragsruf.json";
/// From 2 with regions; a table of format 1 counts as empty.
pub const FORMAT: i64 = 2;

/// `reputation._key`: scmdb writes the key with `@` and in other case.
pub fn key(raw: &str) -> String {
    raw.trim_start_matches('@').to_lowercase()
}

/// The stored table (`{'version', 'auftraege', 'regionen', 'datenfehler'}`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub version: String,
    pub contracts: Map,
    pub regions: Map,
    pub fingerprint: String,
}

/// `reputation.load`.
pub fn load(paths: &Paths) -> Table {
    let Ok(data) = jsonio::read_json(&paths.app_file(CACHE_FILE)) else { return Table::default() };
    if data.get("format") != Some(&Value::int(FORMAT)) {
        return Table::default();
    }
    let Some(Value::Object(contracts)) = data.get("auftraege") else { return Table::default() };
    Table {
        version: data.get("version").and_then(Value::as_str).unwrap_or("").to_string(),
        contracts: contracts.clone(),
        regions: data.get("regionen").and_then(Value::as_object).cloned().unwrap_or_default(),
        fingerprint: data.get("datenfehler").and_then(Value::as_str).unwrap_or("").to_string(),
    }
}

/// `reputation.save`: keys in the order 4.12 writes them.
pub fn save(paths: &Paths, table: &Table) -> bool {
    let data: Map = vec![
        ("format".into(), Value::int(FORMAT)),
        ("version".into(), Value::str(&table.version)),
        ("auftraege".into(), Value::Object(table.contracts.clone())),
        ("regionen".into(), Value::Object(table.regions.clone())),
        ("datenfehler".into(), Value::str(&table.fingerprint)),
    ];
    let text = jsonio::dumps(&Value::Object(data), None, false);
    match jsonio::write_text_atomic(&paths.app_file(CACHE_FILE), &text, jsonio::Newline::Keep) {
        Ok(()) => true,
        Err(error) => {
            errors::record("reputation.save", &error);
            false
        }
    }
}

/// `reputation.same_build`: does `4.10.1-live.12660092` belong to `4.10.1`?
pub fn same_build(full: &str, game_version: &str) -> bool {
    !full.is_empty()
        && !game_version.is_empty()
        && (full == game_version || full.starts_with(&format!("{game_version}-")))
}

fn lookup<'a>(map: &'a Map, raw_key: &str) -> Option<&'a Value> {
    let wanted = key(raw_key);
    map.iter().find(|(k, _)| *k == wanted).map(|(_, v)| v)
}

/// `reputation.line`: `# Ruf: Headhunters +50 Standing`, or empty.
pub fn line(table: &Table, raw_key: &str, word: &str) -> String {
    let mut parts = Vec::new();
    if let Some(Value::Array(entries)) = lookup(&table.contracts, raw_key) {
        for entry in entries {
            let amount = entry.get("wieviel").map(super::texts::py_str).unwrap_or_else(|| "None".into());
            let text =
                |name: &str| crate::text::py_strip(entry.get(name).and_then(Value::as_str).unwrap_or("")).to_string();
            let (who, what) = (text("wer"), text("was"));
            match (who.is_empty(), what.is_empty()) {
                (false, false) => parts.push(format!("{who} +{amount} {what}")),
                (false, true) => parts.push(format!("{who} +{amount}")),
                (true, false) => parts.push(format!("+{amount} {what}")),
                (true, true) => {}
            }
        }
    }
    if parts.is_empty() { String::new() } else { format!("# {word}: {}", parts.join(", ")) }
}

/// `reputation.region_line`: `# Region: Pyro (Bloom)`, or empty.
pub fn region_line(table: &Table, raw_key: &str, word: &str) -> String {
    match lookup(&table.regions, raw_key).and_then(Value::as_str).filter(|t| !t.is_empty()) {
        Some(text) => format!("# {word}: {text}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_from_the_table() {
        let table = Table {
            contracts: jsonio::loads(
                r#"{"k_title": [{"wer": "Headhunters", "was": "Standing", "wieviel": 50}, {"wer": "", "was": "Rep", "wieviel": 5}]}"#,
            )
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
            regions: vec![("k_title".into(), Value::str("Pyro (Bloom)"))],
            ..Table::default()
        };
        assert_eq!(line(&table, "@K_Title", "Ruf"), "# Ruf: Headhunters +50 Standing, +5 Rep");
        assert_eq!(region_line(&table, "K_TITLE", "Region"), "# Region: Pyro (Bloom)");
        assert_eq!(line(&table, "other", "Ruf"), "");
        assert!(same_build("4.10.1-live.1", "4.10.1"));
        assert!(!same_build("4.10.10-live.1", "4.10.1"));
    }
}
