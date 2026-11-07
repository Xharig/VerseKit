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

//! Writes, for every player ship of the game data, the slots `ships_file` makes and every port that holds a part
//! at the factory, to compare both.
//! Run with `VERSEKIT_P4K=<Data.p4k> VERSEKIT_PROBE_OUT=<folder> cargo test -p versekit-gamedata --release --test
//! fleet_probe -- --ignored --nocapture`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::SystemTime;

use p4kx_archive::Archive;
use p4kx_export::{CatalogOptions, Filter};
use serde_json::Value;
use versekit_gamedata::ship_slots::{Catalogs, FILTERS, ships_file};

fn walk(ports: &[Value], path: &str, ship: &str, items: &HashMap<String, (String, String)>, out: &mut String) {
    for port in ports {
        let Some(name) = port["name"].as_str().filter(|n| !n.is_empty()) else { continue };
        let full = if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
        if let Some(guid) = port["itemGuid"].as_str() {
            let (class, kind) = items.get(guid).cloned().unwrap_or_else(|| (port["itemClassName"].as_str().unwrap_or("?").to_string(), String::new()));
            let flags: Vec<&str> = port["flags"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let _ = writeln!(out, "{ship}\t{full}\t{class}\t{kind}\t{}", flags.join(","));
        }
        if let Some(below) = port["ports"].as_array() {
            walk(below, &full, ship, items, out);
        }
    }
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn every_factory_part_and_every_slot() {
    let (Ok(path), Ok(out)) = (std::env::var("VERSEKIT_P4K"), std::env::var("VERSEKIT_PROBE_OUT")) else { return };
    let archive = Archive::open(&path).expect("a readable Data.p4k");
    let mut catalogs = Catalogs::default();
    let mut raw = Vec::new();
    for text in FILTERS {
        let filter = Filter::new(text);
        let options = CatalogOptions { filter: Some(&filter), ..CatalogOptions::new(SystemTime::UNIX_EPOCH) };
        match p4kx_export::build_catalog(&archive, &options) {
            Ok(built) => {
                let catalog: Value = serde_json::from_slice(&built.into_parts().0).expect("json");
                catalogs.add(&catalog);
                raw.push(catalog);
            }
            Err(error) => println!("filter {text}: {error}"),
        }
    }
    let mut items = HashMap::new();
    let mut loadouts = HashMap::new();
    let mut ships = Vec::new();
    for catalog in &raw {
        for item in catalog["items"].as_array().into_iter().flatten() {
            if let Some(guid) = item["guid"].as_str() {
                let class = item["className"].as_str().unwrap_or_default().to_string();
                items.insert(guid.to_string(), (class, item["type"].as_str().unwrap_or_default().to_string()));
            }
        }
        for loadout in catalog["loadouts"].as_array().into_iter().flatten() {
            if let Some(guid) = loadout["entityGuid"].as_str() {
                loadouts.insert(guid.to_string(), loadout.clone());
            }
        }
        ships.extend(catalog["ships"].as_array().cloned().unwrap_or_default());
    }
    let mut parts = String::new();
    for ship in &ships {
        if ship["playability"].as_str() != Some("player") {
            continue;
        }
        let (Some(class), Some(guid)) = (ship["className"].as_str(), ship["guid"].as_str()) else { continue };
        let ports = loadouts.get(guid).and_then(|l| l["ports"].as_array()).cloned().unwrap_or_default();
        walk(&ports, "", class, &items, &mut parts);
    }
    std::fs::write(format!("{out}/werksteile.tsv"), parts).expect("written");
    std::fs::write(format!("{out}/schiffe-520.json"), versekit_core::jsonio::dumps(&ships_file(&catalogs), None, false)).expect("written");
    println!("{} ships", ships.len());
}
