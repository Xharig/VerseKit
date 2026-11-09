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

//! `versekit-parity auftragsangaben <merged-<build>.json> <katalog-cache.json>
//! <auftragsruf.json> [<cig-datenfehler.json>]`: builds the contract details
//! of the blueprint notes from scmdb's file (see
//! `versekit_gamedata::contract_info`) and compares them with what 4.12 kept:
//!
//! - the contracts against `missionen` of the 4.12 cache, once as the module
//!   builds them and once as the catalog the list caches them
//!   (`scmdb_catalog::from_merged` and `catalog_cache::to_value`);
//! - the reputation table against `auftragsruf.json` (parties, regions,
//!   version, fingerprint of the known errors).
//!
//! With the error list of the build the known errors work as in 4.12; without
//! it nothing is corrected. The input is only read. Counts the differences
//! per field and fails when there is one.

use std::collections::BTreeMap;

use versekit_core::catalog_cache;
use versekit_core::injection::reputation::Table;
use versekit_core::jsonio::{self, Map, Value};
use versekit_gamedata::contract_info;
use versekit_gamedata::corrections::{self, Correction};
use versekit_gamedata::scmdb_catalog;

/// The differences of one comparison, by name; the first ones of each name
/// are kept to be shown.
#[derive(Default)]
pub(crate) struct Differences {
    pub(crate) count: BTreeMap<String, usize>,
    shown: BTreeMap<String, Vec<String>>,
}

impl Differences {
    pub(crate) fn add(&mut self, name: &str, detail: impl FnOnce() -> String) {
        *self.count.entry(name.to_string()).or_default() += 1;
        let shown = self.shown.entry(name.to_string()).or_default();
        if shown.len() < 3 {
            shown.push(detail());
        }
    }

    /// Like [`Differences::add`], every one of the name shown.
    pub(crate) fn add_all(&mut self, name: &str, detail: String) {
        *self.count.entry(name.to_string()).or_default() += 1;
        self.shown.entry(name.to_string()).or_default().push(detail);
    }

    pub(crate) fn total(&self) -> usize {
        self.count.values().sum()
    }

    pub(crate) fn print(&self, title: &str, compared: usize) {
        println!("{title}: {compared} verglichen, {} Abweichungen", self.total());
        for (name, count) in &self.count {
            println!("  {name}: {count}");
            for line in self.shown.get(name).into_iter().flatten() {
                println!("      {line}");
            }
        }
    }
}

fn text(value: Option<&Value>, sort_keys: bool) -> String {
    value.map_or_else(|| "(fehlt)".to_string(), |v| jsonio::dumps(v, None, sort_keys))
}

fn cut(text: &str) -> String {
    if text.chars().count() > 160 {
        format!("{}...", text.chars().take(160).collect::<String>())
    } else {
        text.to_string()
    }
}

fn get<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The keys in the order of the map: of those in both, are they in the same
/// order?
fn same_order(a: &Map, b: &Map) -> bool {
    let in_both = |map: &Map, other: &Map| -> Vec<String> {
        map.iter().filter(|(k, _)| get(other, k).is_some()).map(|(k, _)| k.clone()).collect()
    };
    in_both(a, b) == in_both(b, a)
}

/// The keys only one side has, and the order of the shared ones.
fn compare_keys(name: &str, mine: &Map, theirs: &Map, found: &mut Differences) {
    for (key, _) in mine.iter().filter(|(k, _)| get(theirs, k).is_none()) {
        found.add(&format!("{name} nur eigen"), || key.clone());
    }
    for (key, _) in theirs.iter().filter(|(k, _)| get(mine, k).is_none()) {
        found.add(&format!("{name} nur 4.12"), || key.clone());
    }
    if !same_order(mine, theirs) {
        found.add(&format!("{name} Reihenfolge"), || "die gemeinsamen Schlüssel stehen anders".to_string());
    }
}

/// The contracts of `mine` against those of 4.12, field by field. `ab` is
/// compared without the order of its keys: 4.12 writes them in the order of a
/// Python set, which changes from run to run.
fn compare_missions(name: &str, mine: &Map, theirs: &Map) -> Differences {
    let mut found = Differences::default();
    compare_keys(name, mine, theirs, &mut found);
    for (key, mine) in mine {
        let (Some(mine), Some(theirs)) = (mine.as_object(), get(theirs, key).and_then(Value::as_object)) else {
            continue;
        };
        let keys_of = |map: &Map| map.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        if keys_of(mine) != keys_of(theirs) {
            found.add("Feldfolge", || format!("{key}: {:?} gegen {:?}", keys_of(mine), keys_of(theirs)));
        }
        let fields: Vec<&String> = mine.iter().map(|(k, _)| k).chain(theirs.iter().map(|(k, _)| k)).collect();
        let mut seen = Vec::new();
        for field in fields {
            if seen.contains(&field) {
                continue;
            }
            seen.push(field);
            let sort = field == "ab";
            let (a, b) = (text(get(mine, field), sort), text(get(theirs, field), sort));
            if a != b {
                found.add(&format!("Feld {field}"), || format!("{key}: eigen {} gegen 4.12 {}", cut(&a), cut(&b)));
            }
        }
    }
    found
}

/// The reputation table against the file of 4.12.
pub(crate) fn compare_table(mine: &Table, theirs: &Value) -> (Differences, usize) {
    let mut found = Differences::default();
    let their = |name: &str| theirs.get(name).and_then(Value::as_object).cloned().unwrap_or_default();
    let (contracts, regions) = (their("auftraege"), their("regionen"));
    compare_keys("Aufträge", &mine.contracts, &contracts, &mut found);
    compare_keys("Regionen", &mine.regions, &regions, &mut found);
    for (key, rows) in &mine.contracts {
        let Some(other) = get(&contracts, key) else { continue };
        match (rows, other) {
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (row, other_row) in a.iter().zip(b) {
                    for field in ["wer", "was", "wieviel", "datenfehler"] {
                        let (x, y) = (text(row.get(field), false), text(other_row.get(field), false));
                        if x != y {
                            found.add(&format!("Ruf {field}"), || {
                                format!("{key}: eigen {} gegen 4.12 {}", cut(&x), cut(&y))
                            });
                        }
                    }
                    let keys_of =
                        |row: &Value| row.as_object().map(|m| m.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>());
                    if keys_of(row) != keys_of(other_row) {
                        found.add("Ruf Feldfolge", || key.clone());
                    }
                }
            }
            _ => found.add("Ruf Zeilenzahl", || {
                format!("{key}: eigen {} gegen 4.12 {}", cut(&text(Some(rows), false)), cut(&text(Some(other), false)))
            }),
        }
    }
    for (key, region) in &mine.regions {
        if let Some(other) = get(&regions, key)
            && region != other
        {
            found.add("Region", || {
                format!("{key}: eigen {} gegen 4.12 {}", text(Some(region), false), text(Some(other), false))
            });
        }
    }
    let version = theirs.get("version").and_then(Value::as_str).unwrap_or_default();
    if mine.version != version {
        found.add("Version", || format!("eigen {} gegen 4.12 {version}", mine.version));
    }
    let print = theirs.get("datenfehler").and_then(Value::as_str).unwrap_or_default();
    if mine.fingerprint != print {
        found.add("Fingerabdruck datenfehler", || format!("eigen '{}' gegen 4.12 '{print}'", mine.fingerprint));
    }
    (found, contracts.len() + regions.len())
}

fn read_json(path: &str) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

fn read_py(path: &str) -> Result<Value, String> {
    jsonio::read_json(path).map_err(|e| format!("{path}: {e}"))
}

pub fn run(merged: &str, cache: &str, reputation: &str, issues: Option<&str>) -> Result<(), String> {
    let merged = read_json(merged)?;
    let cache = read_py(cache)?;
    let reputation = read_py(reputation)?;
    let mut build = merged.get("version").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let mut list: Vec<Correction> = Vec::new();
    let mut fingerprint = String::new();
    if let Some(path) = issues {
        let file = read_json(path)?;
        let own = read_py(path)?;
        build = file.get("build").and_then(serde_json::Value::as_str).unwrap_or(&build).to_string();
        let daten = file.get("daten").cloned().unwrap_or(serde_json::Value::Null);
        list = corrections::from_scmdb(&daten, &build);
        fingerprint = contract_info::fingerprint(own.get("daten").unwrap_or(&Value::Null));
        println!("Fehlerliste: Stand {build}, {} Berichtigungen, Fingerabdruck '{fingerprint}'", list.len());
    } else {
        println!("Keine Fehlerliste: nichts wird berichtigt");
    }
    let theirs = cache.get("missionen").and_then(Value::as_object).ok_or("der 4.12-Katalog hat keine Aufträge")?;
    if let Some(version) = cache.get("version").and_then(Value::as_str)
        && version != build
    {
        println!("Hinweis: der 4.12-Katalog ist von {version}, die Quelle von {build}");
    }

    let mine = contract_info::missions(&merged, &build, &list);
    let first = compare_missions("Aufträge", &mine, theirs);
    first.print("Aufträge (contract_info::missions)", theirs.len());

    // The way the list keeps them: through the catalog and its cache format.
    let catalog = scmdb_catalog::from_merged(&merged, &build, &list);
    let written = catalog_cache::to_value(&catalog);
    let cached = written.get("missionen").and_then(Value::as_object).cloned().unwrap_or_default();
    let second = compare_missions("Aufträge", &cached, theirs);
    second.print("Aufträge (Katalog und Zwischenspeicher)", theirs.len());
    // And back: what is read from the cache is what was written.
    let again = catalog_cache::parse(&written);
    let third_count = usize::from(again != catalog);
    println!("Zwischenspeicher lesen und schreiben: {}", if third_count == 0 { "gleich" } else { "ABWEICHUNG" });

    let table = contract_info::reputation(&merged, &build, &list, &fingerprint);
    let (ruf, compared) = compare_table(&table, &reputation);
    ruf.print("Auftragsruf", compared);

    let total = first.total() + second.total() + third_count + ruf.total();
    println!(
        "{} Aufträge, {} Rufeinträge, {} Regionen, zusammen {total} Abweichungen",
        mine.len(),
        table.contracts.len(),
        table.regions.len()
    );
    if total == 0 { Ok(()) } else { Err(format!("{total} Abweichungen")) }
}
