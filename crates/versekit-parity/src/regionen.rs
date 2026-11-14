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

//! `versekit-parity regionen <auszug> <merged.json> [--alle]`: what the game data of an extract (made by
//! `versekit-parity auszug`) give for the notes without scmdb, against scmdb's file of the same build: the region of
//! every contract title (`# Region`) and the titles that give blueprints (`[BP]`). Every difference is listed with
//! both values, the first 40 of a kind unless `--alle`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use versekit_core::jsonio::Map;
use versekit_gamedata::{
    Language, Localization, blueprints_from_dataforge, contract_info, contract_reputation, list, scmdb_catalog,
};

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn texts_of(map: &Map) -> BTreeMap<String, String> {
    map.iter()
        .map(|(key, value)| (key.to_lowercase(), value.as_str().unwrap_or_default().to_string()))
        .collect()
}

fn print_some(title: &str, lines: &[String], all: bool) {
    println!("{title}: {}", lines.len());
    let shown = if all { lines.len() } else { lines.len().min(40) };
    for line in &lines[..shown] {
        println!("    {line}");
    }
    if shown < lines.len() {
        println!("    … {} weitere (--alle)", lines.len() - shown);
    }
}

use versekit_gamedata::contract_reputation::parse_region as parsed;

/// Why the region of a title differs from scmdb's: `mine` with every entry, `main` of the contracts alone.
fn cause(mine: &str, main: Option<&str>, theirs: &str) -> &'static str {
    if main == Some(theirs) {
        return "Unterauftraege";
    }
    let (mine, theirs_parsed) = (parsed(main.unwrap_or(mine)), parsed(theirs));
    let systems_mine: BTreeSet<&String> = mine.keys().collect();
    let systems_theirs: BTreeSet<&String> = theirs_parsed.keys().collect();
    if systems_mine == systems_theirs {
        // The same systems: scmdb names planets the data do not offer the contract at (the target places of its
        // location query), or names the whole system where the data name planets.
        let wider = mine.iter().all(|(system, planets)| {
            let other = &theirs_parsed[system];
            planets.is_empty() || other.is_empty() || planets.is_subset(other)
        });
        return if wider { "Zielorte (Ortsabfrage)" } else { "Planeten anders" };
    }
    if systems_mine.is_superset(&systems_theirs) { "eigen mehr Systeme" } else { "scmdb mehr Systeme" }
}

pub fn run(folder: &str, merged: &str, all: bool) -> Result<(), String> {
    let folder = Path::new(folder);
    let merged: serde_json::Value =
        serde_json::from_slice(&read(Path::new(merged))?).map_err(|e| format!("{merged}: {e}"))?;
    let build = merged.get("version").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    let texts =
        Localization::from_files(Language::English, &read(&folder.join("global.ini"))?, None).map_err(|e| e.to_string())?;
    let facts = crate::extract::contract_facts(folder)?;
    let contracts = read(&folder.join(crate::extract::CONTRACTS_FILE))?;
    let catalog = blueprints_from_dataforge(&read(&folder.join("Game2.dcb"))?, &facts, &texts, &[], None)
        .map_err(|e| e.to_string())?;
    let entries: serde_json::Value = serde_json::from_slice(&contracts).map_err(|e| e.to_string())?;
    println!("Stand scmdb {build}; {} Vertragseinträge mit Orten aus der Sternkarte", catalog.offered.len());

    // The regions, and those of the contracts alone without their sub contracts (scmdb's file lists no sub
    // contract), to tell why a region differs.
    let own = texts_of(&contract_reputation::regions(&entries, &catalog.shown_keys, &catalog.offered));
    let main_only: std::collections::HashMap<String, versekit_gamedata::Offered> =
        catalog.offered.iter().filter(|(key, _)| key.ends_with('|')).map(|(k, v)| (k.clone(), v.clone())).collect();
    let own_main = texts_of(&contract_reputation::regions(&entries, &catalog.shown_keys, &main_only));
    let theirs = texts_of(&contract_info::reputation(&merged, &build, &[], "").regions);
    let mut differ = Vec::new();
    let mut only_theirs = Vec::new();
    let mut same = 0;
    let mut causes: BTreeMap<&str, usize> = BTreeMap::new();
    // Differing titles whose own region already holds all scmdb names (scmdb adds nothing).
    let mut own_holds = 0;
    for (key, region) in &theirs {
        match own.get(key) {
            Some(mine) if mine == region => same += 1,
            Some(mine) => {
                let cause = cause(mine, own_main.get(key).map(String::as_str), region);
                *causes.entry(cause).or_default() += 1;
                if parsed(&contract_reputation::join_regions(mine, region)) == parsed(mine) {
                    own_holds += 1;
                }
                differ.push(format!("{key}: [{cause}] eigen „{mine}“ — scmdb „{region}“"));
            }
            None => only_theirs.push(format!("{key}: scmdb „{region}“")),
        }
    }
    let only_own: Vec<String> =
        own.iter().filter(|(key, _)| !theirs.contains_key(*key)).map(|(k, v)| format!("{k}: eigen „{v}“")).collect();
    println!("Regionen: eigen {}, scmdb {}, gleich {same}", own.len(), theirs.len());
    println!("Regionen verschieden nach Ursache: {causes:?}");
    println!("  davon enthält die eigene Region schon alles, was scmdb nennt: {own_holds}");
    print_some("Regionen verschieden", &differ, all);
    print_some("Regionen nur scmdb", &only_theirs, all);
    print_some("Regionen nur eigen", &only_own, all);

    // The titles with blueprints.
    let mine = list::from_game(&catalog, &build, None);
    let unsure: Vec<String> = mine
        .missions
        .iter()
        .filter_map(|mission| {
            let (_, at) =
                mission.extra.iter().find(|(name, _)| name == versekit_core::injection::texts::UNCERTAIN)?;
            Some(format!("{}: {}", mission.key, versekit_core::jsonio::dumps(at, None, false)))
        })
        .collect();
    print_some("[BP]-Titel unsicher (Bauplan nur bei einem anderen Ergebnis als Completed)", &unsure, all);
    let scmdb = scmdb_catalog::from_merged(&merged, &build, &[]);
    let keys = |missions: &[versekit_core::catalog_cache::Mission]| -> BTreeSet<String> {
        missions
            .iter()
            .filter_map(|mission| {
                let title = mission.extra.iter().find(|(name, _)| name == "titel_key").and_then(|(_, v)| v.as_str());
                title.map(str::to_lowercase).or_else(|| Some(mission.key.to_lowercase()))
            })
            .collect()
    };
    let (mine, scmdb) = (keys(&mine.missions), keys(&scmdb.missions));
    println!("[BP]-Titel: eigen {}, scmdb {}, gemeinsam {}", mine.len(), scmdb.len(), mine.intersection(&scmdb).count());
    print_some("[BP]-Titel nur scmdb", &scmdb.difference(&mine).cloned().collect::<Vec<_>>(), all);
    print_some("[BP]-Titel nur eigen", &mine.difference(&scmdb).cloned().collect::<Vec<_>>(), all);
    Ok(())
}
