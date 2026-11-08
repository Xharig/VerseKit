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

//! Reads the slots of the ships from the installed game's `Data.p4k` and checks that the blade slot of every ship
//! offers only the flight blades of that ship: the F8C Lightning those with the tag of the Lightning, the Gladius
//! those with the tag of the Gladius, and every ship its factory blade. Only reads; the reading of the archive waits
//! while the game runs. The slot files go into a folder of the temp directory, never into the data folder.
//!
//! Run with `cargo test -p versekit-gamedata --release --test blades_real -- --ignored --nocapture`. Without
//! `VERSEKIT_CHANNEL_FOLDER` the game folder comes from `GameData::open_installed`; with it, that channel folder is
//! read.

use std::collections::BTreeSet;

use versekit_core::erkul_slots::{ErkulParts, Part, SlotInfo, hardpoints};
use versekit_core::jsonio::{Map, Value};
use versekit_core::ships::{ErkulShips, ShipId};
use versekit_gamedata::{GameData, ship_slots};

fn open() -> GameData {
    match std::env::var_os("VERSEKIT_CHANNEL_FOLDER") {
        Some(folder) => GameData::open(folder).unwrap(),
        None => GameData::open_installed().unwrap(),
    }
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn tags(part: &Part) -> BTreeSet<String> {
    part.tags.iter().map(|t| t.to_lowercase()).collect()
}

/// The blade slot of a ship and the blades it offers.
fn blades<'p>(ships: &ErkulShips, parts: &'p ErkulParts, name: &str, class: &str) -> Option<(SlotInfo, Vec<&'p Part>)> {
    let id = ShipId { name, maker: "", short: class, maker_short: "" };
    let slot = hardpoints(ships, parts, id, &Map::new()).into_iter().find(|s| s.kind == "FlightController")?;
    let offered = parts.compatible(&slot);
    Some((slot, offered))
}

#[test]
#[ignore = "reads the installed game"]
fn the_blade_slot_of_every_ship_offers_only_the_blades_of_that_ship() {
    versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    let game = open();
    println!("archive: {} ({:?})", game.archive_path().display(), game.scmdb_build());
    let (ships_value, parts_value) = game.ship_slots().expect("the slots");
    let dir = std::env::temp_dir().join(format!("versekit-blades-real-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("folder");
    let write = |name: &str, value: &Value| {
        let file = dir.join(name).to_string_lossy().into_owned();
        std::fs::write(&file, versekit_core::jsonio::dumps(value, None, false)).expect("written");
        file
    };
    let ships = ErkulShips::load(&write(ship_slots::SHIPS_FILE, &ships_value));
    let parts = ErkulParts::load(&write(ship_slots::PARTS_FILE, &parts_value));
    let all_blades = parts.all().iter().filter(|p| p.kind == "FlightController").count();
    println!("{all_blades} flight blades in the parts");
    // The rule compares the tags of a blade with those of the factory blade as a whole: that holds as long as no
    // blade carries more than the one tag of its ship.
    let several: Vec<String> = parts
        .all()
        .iter()
        .filter(|p| p.kind == "FlightController" && p.tags.len() > 1)
        .map(|p| format!("{} {:?}", p.class, p.tags))
        .collect();
    assert!(several.is_empty(), "blades with more than one tag: {several:?}");

    // The F8C Lightning: the blades with the tag of the Lightning, never those of other ships.
    let (slot, offered) = blades(&ships, &parts, "F8C Lightning", "ANVL_Lightning_F8C").expect("the F8C has a blade slot");
    let names: Vec<String> = offered.iter().map(|p| format!("{} ({})", p.name, p.class)).collect();
    println!("F8C: factory {:?}, offered {names:?}", slot.factory);
    assert!(offered.iter().all(|p| p.tags.iter().any(|t| t.eq_ignore_ascii_case("ANVL_Lighting_Blade"))), "{names:?}");
    assert!(offered.len() < 20, "not the blades of every ship: {}", offered.len());

    // The Gladius: its own blades with the tag of the Gladius.
    let (slot, offered) = blades(&ships, &parts, "Gladius", "AEGS_Gladius").expect("the Gladius has a blade slot");
    let names: Vec<String> = offered.iter().map(|p| format!("{} ({})", p.name, p.class)).collect();
    println!("Gladius: factory {:?}, offered {names:?}", slot.factory);
    assert!(offered.iter().all(|p| p.tags.iter().any(|t| t.eq_ignore_ascii_case("AEGS_Gladius_Blade"))), "{names:?}");
    assert!(offered.len() >= 3, "the factory blade and the two to buy: {names:?}");

    // A factory blade without tags: the Constellation Andromeda (Mk IV) gets its own two by the class name.
    if let Some((_, offered)) = blades(&ships, &parts, "Constellation Andromeda", "RSI_Constellation_Andromeda") {
        let names: Vec<String> = offered.iter().map(|p| format!("{} ({})", p.name, p.class)).collect();
        println!("Constellation Andromeda: offered {names:?}");
        assert!(offered.iter().all(|p| p.tags.is_empty()), "{names:?}");
    }

    // Every ship: its factory blade is offered, and every other blade has its tags, or with none its class family.
    let Some(Value::Object(listed)) = (match &ships_value {
        Value::Object(root) => field(root, "schiffe").cloned(),
        _ => None,
    }) else {
        panic!("the ships file has ships")
    };
    let (mut checked, mut most) = (0, (0, String::new()));
    for (_, entry) in &listed {
        let Some(entry) = entry.as_object() else { continue };
        let name = field(entry, "name").and_then(Value::as_str).unwrap_or_default().to_string();
        let class = field(entry, "id").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some((slot, offered)) = blades(&ships, &parts, &name, &class) else { continue };
        let Some(stock) = slot.factory.as_ref().and_then(|(reference, _)| parts.get(reference)) else { continue };
        assert!(offered.iter().any(|p| p.reference == stock.reference), "{class}: the factory blade is offered");
        for part in &offered {
            let family = format!("{}_", stock.class.to_lowercase());
            let same = if stock.tags.is_empty() {
                part.reference == stock.reference
                    || part.tags.is_empty()
                        && part.class.to_lowercase().strip_prefix(&family).is_some_and(|rest| rest.contains("blade"))
            } else {
                tags(part) == tags(stock)
            };
            assert!(same, "{class}: {} ({}) belongs to another ship", part.name, part.class);
        }
        checked += 1;
        if offered.len() > most.0 {
            most = (offered.len(), class.clone());
        }
    }
    println!("{checked} ships checked, the most blades: {} at {}", most.0, most.1);
    assert!(checked > 100, "the player ships");
    assert!(most.0 < 20, "no ship gets the blades of every ship");
    let _ = std::fs::remove_dir_all(&dir);
}
