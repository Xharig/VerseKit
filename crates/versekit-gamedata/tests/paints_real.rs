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

//! Reads the slots of the ships from the installed game's `Data.p4k` and checks the paint slot of every ship: where
//! the ship Record has a paint port, the slot takes the paints whose requirement that port offers; where the ship
//! has a factory paint, the paints of its family; where neither says anything, no list instead of every paint.
//! Only reads; the reading of the archive waits while the game runs. The slot files go into a folder of the temp
//! directory (or `VERSEKIT_SLOTS_OUT`), never into the data folder.
//!
//! Run with `cargo test -p versekit-gamedata --release --test paints_real -- --ignored --nocapture`. Without
//! `VERSEKIT_CHANNEL_FOLDER` the game folder comes from `GameData::open_installed`; with it, that channel folder is
//! read.

use std::collections::BTreeSet;

use versekit_core::erkul_slots::{ErkulParts, hardpoints};
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

fn lowered(tags: &[String]) -> BTreeSet<String> {
    tags.iter().map(|t| t.to_lowercase()).collect()
}

#[test]
#[ignore = "reads the installed game"]
fn the_paint_slot_of_every_ship_offers_only_the_paints_of_that_ship() {
    versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    let game = open();
    println!("archive: {} ({:?})", game.archive_path().display(), game.scmdb_build());
    let (ships_value, parts_value) = game.ship_slots().expect("the slots");
    let own_dir = std::env::var_os("VERSEKIT_SLOTS_OUT").is_none();
    let dir = std::env::var_os("VERSEKIT_SLOTS_OUT")
        .map_or_else(|| std::env::temp_dir().join(format!("versekit-paints-real-{}", std::process::id())), Into::into);
    std::fs::create_dir_all(&dir).expect("folder");
    let write = |name: &str, value: &Value| {
        let file = dir.join(name).to_string_lossy().into_owned();
        std::fs::write(&file, versekit_core::jsonio::dumps(value, None, false)).expect("written");
        file
    };
    let ships = ErkulShips::load(&write(ship_slots::SHIPS_FILE, &ships_value));
    let parts = ErkulParts::load(&write(ship_slots::PARTS_FILE, &parts_value));
    let paints: Vec<_> = parts.all().iter().filter(|p| p.kind == "Paints").collect();
    let requiring = paints.iter().filter(|p| !p.required.is_empty()).count();
    println!("{} paints in the parts, {requiring} with what they require of their port", paints.len());
    assert!(requiring * 10 > paints.len() * 9, "nearly every paint says what it requires");

    let Some(Value::Object(listed)) = (match &ships_value {
        Value::Object(root) => field(root, "schiffe").cloned(),
        _ => None,
    }) else {
        panic!("the ships file has ships")
    };
    let (mut by_port, mut by_stock, mut none) = (Vec::new(), Vec::new(), Vec::new());
    for (_, entry) in &listed {
        let Some(entry) = entry.as_object() else { continue };
        let name = field(entry, "name").and_then(Value::as_str).unwrap_or_default().to_string();
        let class = field(entry, "id").and_then(Value::as_str).unwrap_or_default().to_string();
        let id = ShipId { name: &name, maker: "", short: &class, maker_short: "" };
        let Some(slot) = hardpoints(&ships, &parts, id, &Map::new()).into_iter().find(|s| s.kind == "Paints") else {
            continue;
        };
        let offered = parts.compatible(&slot);
        let names: Vec<String> = offered.iter().map(|p| format!("{} {:?}", p.name, p.required)).collect();
        if let Some(fit) = &slot.fit {
            // The port of the Record: each paint requires only what the port offers.
            let offers = lowered(&fit.port_tags);
            for part in &offered {
                assert!(lowered(&part.required).is_subset(&offers), "{class}: {} needs {:?}", part.name, part.required);
            }
            // And none of the family is missing: every paint that requires something the port offers, and carries
            // what the port requires.
            let wanted = lowered(&fit.required);
            for part in paints.iter().filter(|p| {
                !p.required.is_empty() && lowered(&p.required).is_subset(&offers) && wanted.is_subset(&lowered(&p.tags))
            }) {
                assert!(offered.iter().any(|o| o.reference == part.reference), "{class}: {} is missing", part.name);
            }
            by_port.push((class, offered.len(), names));
        } else if let Some(stock) = slot.factory.as_ref().and_then(|(reference, _)| parts.get(reference)) {
            let family = lowered(&stock.required);
            for part in offered.iter().filter(|p| p.reference != stock.reference) {
                assert!(lowered(&part.required).is_subset(&family), "{class}: {} is of another family", part.name);
            }
            by_stock.push((class, offered.len(), names));
        } else {
            assert!(offered.is_empty(), "{class}: no rule, no list");
            none.push(class);
        }
    }
    println!("by the port of the Record: {} ships", by_port.len());
    for (class, count, names) in by_port.iter().take(3) {
        println!("  {class}: {count} {:?}", &names[..names.len().min(4)]);
    }
    println!("by the factory paint: {} ships", by_stock.len());
    for (class, count, names) in by_stock.iter().take(3) {
        println!("  {class}: {count} {:?}", &names[..names.len().min(4)]);
    }
    println!("without a rule (no list): {} ships, e.g. {:?}", none.len(), &none[..none.len().min(8)]);
    // Two ships of each way, and the Gladius and the F8C without either: no list.
    assert!(by_port.len() >= 2 && by_port.iter().any(|(_, count, _)| *count > 1), "the ports of the Records give paints");
    assert!(by_stock.len() >= 2, "the factory paints give paints");
    assert!(none.iter().any(|c| c == "AEGS_Gladius") && none.iter().any(|c| c == "ANVL_Lightning_F8C"));
    if own_dir {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
