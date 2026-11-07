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

//! What the game data give for the slots of the ships: p4kx builds the Basetool catalog of the
//! `Data.p4k` (only read) with the Records whose path holds `VERSEKIT_FILTER` (default
//! `entities/spaceships`), and the test counts its loadouts: the ports of every ship and of every
//! item with ports, each with the sizes and kinds it takes, the part the Default Loadout puts in
//! and the ports below that part. The catalog is written to `VERSEKIT_LOADOUT_OUT` when that is
//! set, for a closer look.
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> cargo test -p versekit-gamedata --release --test
//! loadouts_real -- --ignored --nocapture`.

use std::time::SystemTime;

use p4kx_archive::Archive;
use p4kx_export::{CatalogOptions, Filter};
use serde_json::Value;

/// Counts the ports of a tree, and those that hold a part, all the way down.
fn count(ports: &[Value], all: &mut usize, nested: &mut usize, depth: usize) {
    for port in ports {
        *all += 1;
        if depth > 0 {
            *nested += 1;
        }
        if let Some(below) = port["ports"].as_array() {
            count(below, all, nested, depth + 1);
        }
    }
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn the_game_data_give_the_ports_of_every_ship_with_what_hangs_below() {
    let Ok(path) = std::env::var("VERSEKIT_P4K") else { return };
    let archive = Archive::open(&path).expect("a readable Data.p4k");
    let text = std::env::var("VERSEKIT_FILTER").unwrap_or_else(|_| "entities/spaceships".to_string());
    let filter = Filter::new(&text);
    let options = CatalogOptions { filter: Some(&filter), ..CatalogOptions::new(SystemTime::UNIX_EPOCH) };
    let json = p4kx_export::build_catalog(&archive, &options).expect("the catalog").into_parts().0;
    if let Ok(out) = std::env::var("VERSEKIT_LOADOUT_OUT") {
        std::fs::write(out, &json).expect("written");
    }
    let catalog: Value = serde_json::from_slice(&json).expect("json");
    let loadouts = catalog["loadouts"].as_array().cloned().unwrap_or_default();
    let ships = catalog["ships"].as_array().map_or(0, Vec::len);
    let (mut all, mut nested) = (0, 0);
    for loadout in &loadouts {
        count(loadout["ports"].as_array().map_or(&[][..], Vec::as_slice), &mut all, &mut nested, 0);
    }
    println!("{ships} ships, {} loadouts, {all} ports, {nested} of them below a part", loadouts.len());
    assert!(!loadouts.is_empty(), "the catalog has loadouts");
}

/// The slot files VerseKit keeps beside the game values, written into the folder `VERSEKIT_SLOTS_OUT` for the
/// checks over all ships (`jeder_platz_aller_schiffe`, `rundlauf_alle_schiffe`).
#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn the_slots_of_the_game_data_are_written_in_the_form_of_the_erkul_files() {
    let (Ok(path), Ok(out)) = (std::env::var("VERSEKIT_P4K"), std::env::var("VERSEKIT_SLOTS_OUT")) else { return };
    let game = versekit_gamedata::GameData::open_file(&path).expect("a readable Data.p4k");
    let started = std::time::Instant::now();
    let (ships, parts) = game.ship_slots().expect("the slots");
    let count = |value: &versekit_core::jsonio::Value, key: &str| match value {
        versekit_core::jsonio::Value::Object(root) => {
            root.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_object()).map_or(0, Vec::len)
        }
        _ => 0,
    };
    println!("{} ships, {} parts in {:?}", count(&ships, "schiffe"), count(&parts, "teile"), started.elapsed());
    for (name, value) in [(versekit_gamedata::ship_slots::SHIPS_FILE, &ships), (versekit_gamedata::ship_slots::PARTS_FILE, &parts)] {
        std::fs::write(std::path::Path::new(&out).join(name), versekit_core::jsonio::dumps(value, None, false)).expect("written");
    }
    assert!(count(&ships, "schiffe") > 100, "the player ships");
}

/// Where the vehicle implementations lie in the archive: the files the top ports of a ship take
/// their kinds and sizes from.
#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn the_archive_holds_the_vehicle_implementations() {
    let Ok(path) = std::env::var("VERSEKIT_P4K") else { return };
    let archive = Archive::open(&path).expect("a readable Data.p4k");
    let word = std::env::var("VERSEKIT_FILTER").unwrap_or_else(|_| "hornet_f7cm".to_string()).to_lowercase();
    let mut implementations = 0;
    for entry in archive.entries() {
        let name = entry.path().replace('\\', "/").to_lowercase();
        if name.contains("vehicles/implementations") {
            implementations += 1;
        }
        if name.contains("vehicles/") && name.contains(&word) {
            println!("{}", entry.path());
        }
    }
    println!("{implementations} entries below vehicles/implementations");
}
