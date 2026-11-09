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

//! `versekit-parity auftraege_vollstaendig <auszug> <merged.json> [--weglassen <titel>]`: whether the terms of the
//! contracts (`angaben`, read from the extract `<auszug>` of `versekit-parity auszug` as the program reads them) name
//! every contract, compared contract by contract:
//!
//! 1. every released contract entry of the game data (contract generators with their handlers, sub contracts, legacy
//!    contracts) has an entry under the key of the title it shows; an entry without a title and an unreleased one are
//!    listed with that reason;
//! 2. every contract of scmdb's file of the same build (`contracts` and `legacyContracts`) has one, under its title or,
//!    found by its id, under the title the game data show (then listed with both);
//! 3. every key of the `global.ini` that belongs to the family of a contract title (same beginning before `_title`)
//!    and has no entry is listed with the reason; so is a title without a text and a description without a line;
//! 4. per entry, which terms it has and which it lacks, written to `auftraege-vollstaendig.tsv` in `<auszug>`.
//!
//! Points 1 and 2 decide: a contract without an entry is a gap and fails the check. `--weglassen` drops the entry of
//! one title first, to see the check fail and name it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value as Json;
use versekit_core::injection::form;
use versekit_core::injection::reputation::key;
use versekit_core::jsonio::Value;
use versekit_gamedata::contract_reputation;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn text(value: Option<&Json>) -> Option<&str> {
    value.and_then(Json::as_str).filter(|t| !t.is_empty())
}

/// A gap: the contract and why.
struct Gap {
    what: String,
    why: &'static str,
}

pub fn run(folder: &str, merged: &str, leave_out: Option<&str>) -> Result<(), String> {
    let folder = Path::new(folder);
    let ini = folder.join("global.ini");
    let contracts_bytes = read(&folder.join("contracts.json"))?;
    let keys = crate::extract::shown_keys(folder)?;
    let mut own = contract_reputation::from_catalogs(
        &contracts_bytes,
        &keys,
        &read(&folder.join("factions.json"))?,
        &read(&folder.join("reputation.json"))?,
        &[],
        "",
    )
    .map_err(|e| e.to_string())?;
    if let Some(title) = leave_out {
        let wanted = key(title);
        let before = own.terms.len();
        own.terms.retain(|(k, _)| *k != wanted);
        println!("Gegenprobe: '{wanted}' weggelassen ({} Eintrag)", before - own.terms.len());
    }
    let have: HashSet<&str> = own.terms.iter().map(|(k, _)| k.as_str()).collect();
    let catalog: Json = serde_json::from_slice(&contracts_bytes).map_err(|e| e.to_string())?;
    let merged: Json = serde_json::from_slice(&read(Path::new(merged))?).map_err(|e| format!("{merged}: {e}"))?;
    let ini_text = versekit_core::text::decode_ignore(&read(&ini)?);
    let ini_keys: BTreeSet<String> = ini_text
        .lines()
        .filter_map(form::split_line)
        .map(|(k, _, _)| k.trim_start_matches('\u{feff}').to_lowercase())
        .collect();

    // 1. Every contract entry of the game data.
    let mut gaps: Vec<Gap> = Vec::new();
    let mut untitled: Vec<String> = Vec::new();
    let entries = catalog.get("contracts").and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default();
    let released: Vec<&Json> = contract_reputation::entries(&catalog).collect();
    let unreleased: Vec<String> = entries
        .iter()
        .filter(|entry| !released.iter().any(|r| std::ptr::eq(*r, *entry)))
        .map(|entry| {
            let title = text(entry.get("title").and_then(|t| t.get("key"))).unwrap_or("(ohne Titel)");
            format!("{} {title}", text(entry.get("debugName")).unwrap_or("?"))
        })
        .collect();
    for entry in released.iter().copied() {
        let debug = text(entry.get("debugName")).unwrap_or("(ohne debugName)");
        let sub = text(entry.get("subContractGuid")).map_or(String::new(), |s| format!(" Untervertrag {s}"));
        let titles: Vec<String> =
            contract_reputation::shown(entry, &keys).0.into_iter().map(key).filter(|k| !k.is_empty()).collect();
        if titles.is_empty() {
            untitled.push(format!("{debug}{sub} ({})", text(entry.get("recordPath")).unwrap_or("?")));
        }
        for title in titles.iter().filter(|title| !have.contains(title.as_str())) {
            gaps.push(Gap { what: format!("{title} ({debug}{sub})"), why: "Auftrag der Spieldaten ohne Eintrag" });
        }
    }

    // 2. Every contract of scmdb's file, matched by its id with the entry of the game data; where both name the
    //    title differently, the title the game data show counts.
    let mut shown_by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in released.iter().copied() {
        let id = versekit_gamedata::entry_key(
            text(entry.get("guid")).unwrap_or_default(),
            text(entry.get("subContractGuid")),
        );
        shown_by_id.insert(id, contract_reputation::shown(entry, &keys).0.into_iter().map(key).collect());
    }
    let mut scmdb_count = 0;
    let mut other_title: Vec<String> = Vec::new();
    for list in ["contracts", "legacyContracts"] {
        for contract in merged.get(list).and_then(Json::as_array).into_iter().flatten() {
            scmdb_count += 1;
            let title = text(contract.get("titleLocKey")).or_else(|| text(contract.get("titleKey"))).map(key);
            let debug = text(contract.get("debugName")).unwrap_or("?");
            let id = match text(contract.get("parentId")) {
                Some(parent) => versekit_gamedata::entry_key(parent, text(contract.get("id"))),
                None => versekit_gamedata::entry_key(text(contract.get("id")).unwrap_or_default(), None),
            };
            let ours = shown_by_id
                .get(&id)
                .map(|titles| titles.iter().filter(|t| have.contains(t.as_str())).cloned().collect::<Vec<_>>())
                .filter(|titles| !titles.is_empty());
            match (title.filter(|t| !t.is_empty()), ours) {
                (Some(title), _) if have.contains(title.as_str()) => {}
                (Some(title), Some(ours)) => other_title.push(format!("{title} → {} ({debug})", ours.join(" / "))),
                (Some(title), None) => gaps.push(Gap {
                    what: format!("{title} ({debug}, scmdb {list}, „{}“)", text(contract.get("title")).unwrap_or("")),
                    why: "Auftrag von scmdb ohne Eintrag",
                }),
                (None, _) => untitled.push(format!("{debug} (scmdb {list})")),
            }
        }
    }

    // 3. The keys of the global.ini of the same families.
    let families: BTreeSet<String> = have.iter().map(|k| form::stem(k)).filter(|s| !s.is_empty()).collect();
    let ini_texts: BTreeMap<String, &str> = ini_text
        .lines()
        .filter_map(form::split_line)
        .map(|(k, _, value)| (k.trim_start_matches('\u{feff}').to_lowercase(), value))
        .collect();
    let (placeholders, not_offered): (Vec<&String>, Vec<&String>) = ini_keys
        .iter()
        .filter(|k| k.contains("_title") && !have.contains(k.as_str()) && families.contains(&form::stem(k)))
        .partition(|k| ini_texts.get(k.as_str()).is_some_and(|t| t.trim_start().starts_with("~mission(")));
    let no_text: Vec<&str> = have.iter().copied().filter(|k| !ini_keys.contains(*k)).collect();

    // 4. The terms of every entry.
    let parties: HashSet<&str> = own.parties.iter().map(|(k, _)| k.as_str()).collect();
    let fields = [
        "warte",
        "warte_abbruch",
        "buyin",
        "belohnung",
        "teilbar",
        "wieder_abbruch",
        "wieder_fehlschlag",
        "rang",
        "rang_max",
    ];
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut table = String::from("titel\tbeschreibung_in_ini\truf");
    for field in fields {
        let _ = write!(table, "\t{field}");
    }
    table.push('\n');
    let mut without_line = Vec::new();
    for (title, entry) in &own.terms {
        let entry = entry.as_object().cloned().unwrap_or_default();
        let has = |name: &str| entry.iter().any(|(k, _)| k == name);
        let texts: Vec<String> = match entry.iter().find(|(k, _)| k == "texte") {
            Some((_, Value::Array(list))) => list.iter().filter_map(Value::as_str).map(String::from).collect(),
            _ => Vec::new(),
        };
        let placed = texts.iter().any(|t| ini_keys.contains(t));
        if !placed {
            without_line.push(title.clone());
        }
        let _ = write!(table, "{title}\t{}\t{}", yes(placed), yes(parties.contains(title.as_str())));
        for field in fields {
            let present = has(field);
            if present {
                *counts.entry(field).or_default() += 1;
            }
            let _ = write!(table, "\t{}", yes(present));
        }
        table.push('\n');
    }
    let tsv = folder.join("auftraege-vollstaendig.tsv");
    std::fs::write(&tsv, table).map_err(|e| e.to_string())?;

    println!(
        "Spieldaten: {} Vertragseinträge, davon {} veröffentlicht; {} Titel in den Angaben; scmdb: {scmdb_count} Aufträge",
        entries.len(),
        released.len(),
        own.terms.len()
    );
    println!(
        "Felder: {}; mit Ruf: {} von {}",
        counts.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", "),
        own.terms.iter().filter(|(k, _)| parties.contains(k.as_str())).count(),
        own.terms.len()
    );
    println!("Je Eintrag: {}", tsv.display());
    list("Nicht veröffentlicht (notForRelease oder workInProgress gesetzt, im Spiel nicht angeboten)", &unreleased);
    list("Ohne Titel (kein Eintrag möglich: die Spieldaten nennen keinen Titelschlüssel)", &untitled);
    list(
        "Aufträge, die scmdb unter einem anderen Titel führt (scmdb → angezeigter Titel der Spieldaten)",
        &other_title,
    );
    list(
        "Titelschlüssel der global.ini derselben Familie ohne Eintrag, Platzhalter (~mission: das Spiel zeigt den Text des Auftraggebers)",
        &placeholders.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    list(
        "Titelschlüssel der global.ini derselben Familie ohne Eintrag (nicht angeboten: kein veröffentlichter Auftrag zeigt ihn)",
        &not_offered.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    list(
        "Einträge, deren Titel keinen Text in der global.ini hat (im Spiel nicht zu sehen)",
        &no_text.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    list("Einträge ohne Beschreibungszeile in der global.ini (kein Ort für den Block)", &without_line);
    let gap_lines: Vec<String> = gaps.iter().map(|g| format!("{} — {}", g.what, g.why)).collect();
    list("LÜCKEN", &gap_lines);
    if gaps.is_empty() {
        println!("ERGEBNIS vollständig");
        Ok(())
    } else {
        println!("ERGEBNIS LÜCKEN");
        Err(format!("{} Aufträge ohne Eintrag", gaps.len()))
    }
}

fn yes(flag: bool) -> &'static str {
    if flag { "ja" } else { "nein" }
}

fn list(title: &str, items: &[String]) {
    println!("{title}: {}", items.len());
    for item in items {
        println!("    {item}");
    }
}
