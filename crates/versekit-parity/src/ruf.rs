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

//! The reputation read from the game data against what VerseKit kept so far:
//!
//! - `versekit-parity rufstufen <reputation.json> <rufstufen.json>`: the rank thresholds from the p4kx catalog of
//!   the reputation records against the stored `rufstufen.json`, and the additions to the rank names they give in
//!   German and English;
//! - `versekit-parity auftragsruf <auszug> <merged.json> <auftragsruf.json>`: whom the contracts credit reputation
//!   and their terms, from the extract `<auszug>` (`versekit-parity auszug`: DataForge, English texts and the p4kx
//!   catalogs `contracts.json`, `factions.json`, `reputation.json`), against the table built from scmdb's file of the
//!   same build and against the stored `auftragsruf.json`; the regions are scmdb's in both. The rows of a title are
//!   compared without their order.
//!
//! The input is only read. Each fails when there is a difference.

use std::path::Path;

use versekit_core::injection::reputation::Table;
use versekit_core::injection::{ranks, reputation};
use versekit_core::jsonio::{self, Map, Value};
use versekit_gamedata::{contract_info, contract_reputation, corrections, reputation_ranks};

use crate::contract_info::{Differences, compare_table};

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The suffixes a table of thresholds gives in `lang`, as `build_table` forms them.
fn suffixes(table: &[(String, i64)], lang: &str) -> Vec<(String, String)> {
    table
        .iter()
        .filter(|(_, value)| *value >= ranks::MIN_SHOWN)
        .map(|(key, value)| (key.clone(), ranks::suffix(*value, lang)))
        .collect()
}

pub fn ranks(catalog: &str, stored: &str) -> Result<(), String> {
    let own = reputation_ranks::thresholds(&read(Path::new(catalog))?).map_err(|e| e.to_string())?;
    let data = jsonio::read_json(stored).map_err(|e| format!("{stored}: {e}"))?;
    let old: Vec<(String, i64)> = data
        .get("stufen")
        .and_then(Value::as_object)
        .ok_or("rufstufen.json hat keine Stufen")?
        .iter()
        .filter_map(|(key, value)| match value {
            Value::Int(number) => Some((key.clone(), number.parse().ok()?)),
            _ => None,
        })
        .collect();
    println!("Gespeichert: {} Stufen (Stand {}), Spieldaten: {}", old.len(), text(data.get("version")), own.len());
    let mut differences = 0;
    for lang in ["de", "en"] {
        let (mine, theirs) = (suffixes(&own, lang), suffixes(&old, lang));
        for (key, tail) in &theirs {
            match mine.iter().find(|(k, _)| k == key) {
                Some((_, own_tail)) if own_tail == tail => {}
                Some((_, own_tail)) => {
                    differences += 1;
                    println!("  {lang} {key}: eigen '{own_tail}' gegen gespeichert '{tail}'");
                }
                None => {
                    differences += 1;
                    println!("  {lang} {key}: fehlt in den Spieldaten (gespeichert '{tail}')");
                }
            }
        }
        let extra: Vec<String> = mine
            .iter()
            .filter(|(key, _)| !theirs.iter().any(|(k, _)| k == key))
            .map(|(key, tail)| format!("{key}{tail}"))
            .collect();
        println!("{lang}: {} gespeicherte Zusätze geprüft, {} neu aus den Spieldaten", theirs.len(), extra.len());
        for line in &extra {
            println!("    neu {line}");
        }
    }
    let zero: Vec<&str> = old
        .iter()
        .filter(|(key, value)| *value < ranks::MIN_SHOWN && !own.iter().any(|(k, v)| k == key && v == value))
        .map(|(key, _)| key.as_str())
        .collect();
    if !zero.is_empty() {
        println!("Ohne Anzeige, aber anders oder fehlend: {zero:?}");
    }
    println!("ERGEBNIS {}", if differences == 0 { "gleich" } else { "ABWEICHUNG" });
    if differences == 0 { Ok(()) } else { Err(format!("{differences} Abweichungen")) }
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The fields of a contract compared with scmdb contract by contract: the catalog field and scmdb's.
const CONTRACT_FIELDS: [(&str, &str); 6] = [
    ("hasPersonalCooldown", "hasPersonalCooldown"),
    ("personalCooldownTime", "personalCooldownTime"),
    ("abandonedCooldownTime", "abandonedCooldownTime"),
    ("canReacceptAfterAbandoning", "canReacceptAfterAbandoning"),
    ("canReacceptAfterFailing", "canReacceptAfterFailing"),
    ("canBeShared", "canBeShared"),
];

/// A value of the catalog as scmdb writes it: the `value` of a fact, a whole number without its fraction.
fn plain(value: Option<&serde_json::Value>) -> serde_json::Value {
    let value = match value {
        Some(serde_json::Value::Object(fact)) => fact.get("value").cloned().unwrap_or_default(),
        Some(other) => other.clone(),
        None => serde_json::Value::Null,
    };
    match value.as_f64() {
        Some(x) if x.fract() == 0.0 && !value.is_boolean() => serde_json::json!(x as i64),
        _ => value,
    }
}

/// What the record of a handler states for a field: the value that counts (the override of its `contractParams`,
/// else its `defaultAvailability`) and the one of its `defaultAvailability`.
type HandlerValues = std::collections::HashMap<(String, String, String), (serde_json::Value, serde_json::Value)>;

/// The values of every handler of every contract generator of the DataForge in `folder`, read from the records
/// themselves, by (record path in lower case, handler, field). To check the values of the catalog that come from the
/// handler on their own.
fn handler_values(folder: &Path) -> Result<HandlerValues, String> {
    use p4kx_dataforge::{DataForge, JsonLayout, RecordJson};
    let bytes = read(&folder.join("Game2.dcb"))?;
    let forge = DataForge::open(&bytes).map_err(|e| e.to_string())?;
    let mut writer = RecordJson::new(JsonLayout::Compact);
    let mut found = std::collections::HashMap::new();
    for record in forge.records() {
        let (Ok(kind), Ok(path)) = (record.type_name(), record.record_path()) else { continue };
        if kind != "ContractGenerator" {
            continue;
        }
        let Ok(tree) = record.tree() else { continue };
        let mut out = Vec::new();
        writer.write(&tree, &mut out).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_slice(&out).map_err(|e| e.to_string())?;
        for handler in json.get("generators").and_then(serde_json::Value::as_array).into_iter().flatten() {
            let name = handler.get("debugName").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let defaults = handler.get("defaultAvailability");
            let params = handler.get("contractParams");
            for (field, _) in CONTRACT_FIELDS {
                let overridden = ["intParamOverrides", "boolParamOverrides"].iter().find_map(|list| {
                    params?.get(*list)?.as_array()?.iter().find_map(|param| {
                        let named = param.get("param")?.as_str()?;
                        named.eq_ignore_ascii_case(field).then(|| param.get("value").cloned()).flatten()
                    })
                });
                let default = defaults.and_then(|d| d.get(field)).cloned();
                if let Some(value) = overridden.or_else(|| default.clone()) {
                    found.insert(
                        (path.to_ascii_lowercase(), name.clone(), field.to_string()),
                        (plain(Some(&value)), plain(default.as_ref())),
                    );
                }
            }
        }
    }
    Ok(found)
}

/// Where a field of the catalog comes from: the level of the contract facts or of the cooldowns.
fn layer<'a>(entry: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    entry
        .get(field)
        .and_then(|fact| fact.get("layer"))
        .or_else(|| entry.get("cooldownLayers").and_then(|layers| layers.get(field)))
        .and_then(serde_json::Value::as_str)
}

/// The terms of every contract scmdb knows against the same contract of the game data, found by its id, field by
/// field: equal, scmdb's value is the default of another handler of the same generator, scmdb states none, or else.
/// A value of the catalog that comes from the handler must be the one the record of the handler states (`raw`, see
/// [`handler_values`]); where it is not, the difference counts as unexplained whatever scmdb says.
fn compare_contracts(catalog: &serde_json::Value, merged: &serde_json::Value, raw: &HandlerValues) -> Differences {
    use std::collections::{HashMap, HashSet};
    let entries: Vec<&serde_json::Value> = contract_reputation::entries(catalog).collect();
    let text =
        |entry: &serde_json::Value, name: &str| entry.get(name).and_then(serde_json::Value::as_str).map(String::from);
    let by_id: HashMap<String, &serde_json::Value> = entries
        .iter()
        .map(|entry| {
            let id = versekit_gamedata::entry_key(
                &text(entry, "guid").unwrap_or_default(),
                text(entry, "subContractGuid").as_deref(),
            );
            (id, *entry)
        })
        .collect();
    // The values each handler gives where no contract states its own: (record, field) → handler → values.
    let mut defaults: HashMap<(String, &str), HashMap<String, HashSet<String>>> = HashMap::new();
    for entry in &entries {
        for (field, _) in CONTRACT_FIELDS {
            if layer(entry, field) == Some("handler") {
                let record = text(entry, "recordPath").unwrap_or_default();
                let handler = text(entry, "handler").unwrap_or_default();
                defaults
                    .entry((record, field))
                    .or_default()
                    .entry(handler)
                    .or_default()
                    .insert(plain(entry.get(field)).to_string());
            }
        }
    }
    let mut found = Differences::default();
    let mut equal = 0;
    for list in ["contracts", "legacyContracts"] {
        for contract in merged.get(list).and_then(serde_json::Value::as_array).into_iter().flatten() {
            let id = match text(contract, "parentId") {
                Some(parent) => versekit_gamedata::entry_key(&parent, text(contract, "id").as_deref()),
                None => versekit_gamedata::entry_key(&text(contract, "id").unwrap_or_default(), None),
            };
            let Some(entry) = by_id.get(&id) else { continue };
            let debug = text(contract, "debugName").unwrap_or_default();
            for (field, theirs_field) in CONTRACT_FIELDS {
                let (ours, theirs) = (plain(entry.get(field)), contract.get(theirs_field).cloned().unwrap_or_default());
                if ours == theirs {
                    equal += 1;
                    continue;
                }
                let record = text(entry, "recordPath").unwrap_or_default();
                let handler = text(entry, "handler").unwrap_or_default();
                let other = defaults.get(&(record, field)).is_some_and(|by_handler| {
                    by_handler.iter().any(|(h, values)| *h != handler && values.contains(&theirs.to_string()))
                });
                let no_template = entry.get("templateGuid").is_none_or(serde_json::Value::is_null);
                let no_personal = field == "personalCooldownTime" && plain(entry.get("hasPersonalCooldown")) == false;
                let record_key = (
                    text(entry, "recordPath").unwrap_or_default().to_ascii_lowercase(),
                    handler.clone(),
                    field.to_string(),
                );
                let from_handler = layer(entry, field) == Some("handler");
                let stated = raw.get(&record_key).filter(|_| from_handler);
                let off_record = stated.is_some_and(|(value, _)| *value != ours);
                let default_taken = stated.is_some_and(|(value, default)| *value != *default && *default == theirs);
                let kind = if off_record {
                    "ungeklärt"
                } else if default_taken {
                    "scmdb nimmt die Voreinstellung des Handlers statt seiner Überschreibung (contractParams)"
                } else if theirs.is_null() {
                    "scmdb ohne Wert"
                } else if no_personal {
                    "ohne Belang: der Auftrag hat keine persönliche Wartezeit"
                } else if other {
                    "scmdb nimmt den Wert eines anderen Handlers desselben Generators"
                } else if no_template && ours.is_null() {
                    "die Spieldaten nennen keinen Wert (Auftrag ohne Vorlage), scmdb setzt einen"
                } else if no_template && layer(entry, field) == Some("handler") {
                    "Auftrag ohne Vorlage: der Handler nennt den Wert, scmdb setzt einen anderen"
                } else {
                    "ungeklärt"
                };
                let name = format!("{field}: {kind}");
                let line = format!("{debug} ({handler}): eigen {ours} gegen scmdb {theirs}");
                if kind == "ungeklärt" {
                    found.add_all(&name, line);
                } else {
                    found.add(&name, || line);
                }
            }
        }
    }
    println!("Auftragsangaben je Auftrag (gleiche id): {equal} Felder gleich");
    found
}

/// The rows of every title in one order: two sources may list the same rows in another.
fn sorted_rows(contracts: &Map) -> Map {
    contracts
        .iter()
        .map(|(key, rows)| {
            let rows = match rows {
                Value::Array(list) => {
                    let mut list = list.clone();
                    list.sort_by_key(|row| jsonio::dumps(row, None, true));
                    Value::Array(list)
                }
                other => other.clone(),
            };
            (key.clone(), rows)
        })
        .collect()
}

/// The terms of the contracts, title by title and field by field.
fn compare_terms(mine: &Map, theirs: &Map) -> Differences {
    let mut found = Differences::default();
    let get = |map: &Map, key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    for (key, _) in mine.iter().filter(|(k, _)| get(theirs, k).is_none()) {
        found.add("Angaben nur eigen", || key.clone());
    }
    for (key, _) in theirs.iter().filter(|(k, _)| get(mine, k).is_none()) {
        found.add("Angaben nur scmdb", || key.clone());
    }
    for (key, entry) in mine {
        let (Some(entry), Some(Value::Object(other))) = (entry.as_object(), get(theirs, key)) else { continue };
        let mut fields: Vec<&String> = entry.iter().map(|(k, _)| k).collect();
        fields.extend(other.iter().map(|(k, _)| k).filter(|k| !entry.iter().any(|(own, _)| own == *k)));
        for field in fields {
            let text = |map: &Map| {
                map.iter()
                    .find(|(k, _)| k == field)
                    .map_or("(fehlt)".to_string(), |(_, v)| jsonio::dumps(v, None, false))
            };
            let (a, b) = (text(entry), text(&other));
            if a != b {
                found.add(&format!("Angabe {field}"), || format!("{key}: eigen {a} gegen scmdb {b}"));
            }
        }
    }
    found
}

/// The stored form of a table, as `reputation::save` writes it.
fn as_file(table: &Table) -> Value {
    Value::Object(vec![
        ("format".into(), Value::int(reputation::FORMAT)),
        ("version".into(), Value::str(&table.version)),
        ("auftraege".into(), Value::Object(table.contracts.clone())),
        ("regionen".into(), Value::Object(table.regions.clone())),
        ("datenfehler".into(), Value::str(&table.fingerprint)),
    ])
}

pub fn contracts(folder: &str, merged: &str, stored: &str) -> Result<(), String> {
    let folder = Path::new(folder);
    let merged_json: serde_json::Value =
        serde_json::from_slice(&read(Path::new(merged))?).map_err(|e| format!("{merged}: {e}"))?;
    let build = merged_json.get("version").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let user_agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
    let (list, error) = corrections::collect(Some(&build), &user_agent);
    if let Some(error) = error {
        return Err(format!("Fehlerliste von {build}: {error}"));
    }
    println!("Stand {build}, {} Berichtigungen", list.len());
    let keys = crate::extract::shown_keys(folder)?;
    println!("{} Vertragseinträge zeigen einen anderen Titel- oder Beschreibungsschlüssel als sie nennen", keys.len());
    let own = contract_reputation::from_catalogs(
        &read(&folder.join("contracts.json"))?,
        &keys,
        &read(&folder.join("factions.json"))?,
        &read(&folder.join("reputation.json"))?,
        &list,
        &build,
    )
    .map_err(|e| e.to_string())?;
    let theirs = contract_info::reputation(&merged_json, &build, &list, "");
    let mine = Table { contracts: sorted_rows(&own.parties), ..theirs.clone() };
    let theirs_sorted = Table { contracts: sorted_rows(&theirs.contracts), ..theirs.clone() };

    let (against_scmdb, compared) = compare_table(&mine, &as_file(&theirs_sorted));
    against_scmdb.print("Ruf: Spieldaten gegen scmdb (gleicher Stand)", compared);
    let file = jsonio::read_json(stored).map_err(|e| format!("{stored}: {e}"))?;
    let (against_file, compared) = compare_table(&mine, &file);
    // The stored table joined the contracts of a title otherwise (the last one won): shown, not counted.
    against_file.print("Zur Ansicht, nicht gezählt — Ruf: Spieldaten gegen gespeichertes auftragsruf.json", compared);
    let terms = compare_terms(&versekit_gamedata::list::comparable_terms(&own.terms), &theirs.terms);
    terms.print("Zur Ansicht, nicht gezählt — Auftragsangaben je Titel gegen scmdb", theirs.terms.len());
    let catalog: serde_json::Value =
        serde_json::from_slice(&read(&folder.join("contracts.json"))?).map_err(|e| e.to_string())?;
    let raw = handler_values(folder)?;
    let per_contract = compare_contracts(&catalog, &merged_json, &raw);
    per_contract.print("Auftragsangaben je Auftrag gegen scmdb", 0);
    let unexplained: usize =
        per_contract.count.iter().filter(|(name, _)| name.ends_with("ungeklärt")).map(|(_, n)| n).sum();
    println!("Ungeklärte Abweichungen der Auftragsangaben: {unexplained}");
    // Titles only the game data know and the order of the titles are no differences of the values.
    let ruf: usize = against_scmdb
        .count
        .iter()
        .filter(|(name, _)| !name.ends_with("nur eigen") && !name.ends_with("Reihenfolge") && !name.ends_with("nur 4.12"))
        .map(|(_, n)| n)
        .sum();
    // A title only scmdb credits reputation for: one the game data know, whose contracts do not all give it (the
    // rows stand only where every contract gives them), or one they lack.
    let has = |map: &Map, key: &str| map.iter().any(|(k, _)| k == key);
    let (disputed, missing): (Vec<&String>, Vec<&String>) = theirs
        .contracts
        .iter()
        .map(|(key, _)| key)
        .filter(|key| !has(&mine.contracts, key))
        .partition(|key| has(&own.terms, key));
    println!("Titel mit Ruf nur bei scmdb, bei uns vorhanden, aber nicht jede Variante gibt ihn: {}", disputed.len());
    for key in &disputed {
        println!("    {key}");
    }
    println!("Titel mit Ruf nur bei scmdb, die die Spieldaten nicht kennen: {}", missing.len());
    for key in &missing {
        println!("    {key}");
    }
    let ruf = ruf + missing.len();
    println!("Abweichungen des Rufs auf Titeln, die beide kennen, und Titel nur bei scmdb: {ruf}");
    let total = ruf + unexplained;
    println!(
        "{} Aufträge mit Ruf aus den Spieldaten, {} aus scmdb; Angaben zu {} Titeln aus den Spieldaten, {} aus scmdb",
        mine.contracts.len(),
        theirs.contracts.len(),
        own.terms.len(),
        theirs.terms.len()
    );
    println!("ERGEBNIS {}", if total == 0 { "gleich" } else { "ABWEICHUNG" });
    if total == 0 { Ok(()) } else { Err(format!("{total} Abweichungen")) }
}
