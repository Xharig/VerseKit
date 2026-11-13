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

//! Which ship of the game data a name of the hangar or of the UEX list finds: the cargo of the ship found is set against
//! the cargo UEX lists for it, and every ship whose two differ is printed. Reads the data folder (`Intern`) of a
//! VerseKit data folder, never writes to it.
//!
//! Run with `VERSEKIT_INTERN=<…\VerseKit-Daten\Intern> cargo test -p versekit-core --release --test ship_lookup_real
//! -- --ignored --nocapture`.

use versekit_core::jsonio::Value;
use versekit_core::ships::{ErkulShips, ShipId};

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Float(x) => Some(*x),
        Value::Int(text) => text.parse().ok(),
        _ => None,
    }
}

fn read(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).expect("json")
}

/// What the lookup finds for one name: the stored key and the id of the ship, and its cargo, if any.
fn found(ships: &ErkulShips, id: ShipId) -> Option<(String, Option<f64>, Option<[f64; 3]>)> {
    let entry = ships.entry(id)?;
    let key = entry.iter().find(|(k, _)| k == "id").and_then(|(_, v)| v.as_str()).unwrap_or("?").to_string();
    let values = entry.iter().find(|(k, _)| k == "werte").and_then(|(_, v)| v.as_object());
    let cargo = values.and_then(|v| number(v.iter().find(|(k, _)| k == "fracht").map(|(_, v)| v)));
    let size = values.and_then(|v| match v.iter().find(|(k, _)| k == "masse").map(|(_, v)| v) {
        Some(Value::Array(a)) if a.len() == 3 => {
            Some([number(a.first())?, number(a.get(1))?, number(a.get(2))?])
        }
        _ => None,
    });
    Some((key, cargo, size))
}

#[test]
#[ignore = "reads the data folder of an installed VerseKit"]
fn names_find_the_ship_they_mean() {
    let folder = std::path::PathBuf::from(std::env::var("VERSEKIT_INTERN").expect("VERSEKIT_INTERN"));
    let game = ErkulShips::load(&folder.join("spiel-schiffe.json").to_string_lossy());
    let erkul = ErkulShips::load(&folder.join("erkul-schiffe.json").to_string_lossy());
    let ships = ErkulShips::merged(game, erkul);
    let uex = read(&folder.join("schiffe.json"));
    let mut lines = Vec::new();
    let mut wrong = 0;
    let mut total = 0;
    // The UEX list: name without the maker (as a player writes it in the hangar), the maker from `werft`.
    for (_, ship) in uex["schiffe"].as_object().expect("ships") {
        let (full, maker) = (ship["name"].as_str().unwrap_or_default(), ship["werft"].as_str().unwrap_or_default());
        let scu = ship["scu"].as_f64();
        let name = full.strip_prefix(maker).map(str::trim).unwrap_or_else(|| full.split_once(' ').map_or(full, |p| p.1));
        total += 1;
        let result = found(&ships, ShipId { name, maker, short: "", maker_short: "" });
        let verdict = match (&result, scu) {
            (Some((_, Some(cargo), _)), Some(scu)) if (cargo - scu).abs() > 0.5 => "CARGO DIFFERS",
            (None, _) => "not found",
            _ => "ok",
        };
        if verdict == "CARGO DIFFERS" {
            wrong += 1;
        }
        if verdict != "ok" {
            lines.push(format!("UEX\t{full}\t{maker}\tUEX {scu:?}\t{result:?}\t{verdict}"));
        }
    }
    // The hangar entries as they are stored.
    let hangar = read(&folder.join("hangar.json"));
    for entry in hangar["schiffe"].as_array().expect("hangar") {
        let field = |k: &str| entry[k].as_str().unwrap_or_default();
        let result = found(
            &ships,
            ShipId { name: field("name"), maker: field("hersteller"), short: field("kurz"), maker_short: field("hkurz") },
        );
        total += 1;
        lines.push(format!("HANGAR\t{}\t{}\t{}\t{result:?}", field("name"), field("hersteller"), field("kurz")));
    }
    // The price the hover finds for each hangar entry, as the hangar page asks for it.
    let uex_ships = versekit_core::ships::UexShips::load(&folder.join("schiffe.json").to_string_lossy());
    let market = versekit_core::ship_market::Market::from_value(
        &versekit_core::jsonio::read_json(&folder.join("schiffe.json").to_string_lossy()).expect("schiffe.json"),
    );
    for entry in hangar["schiffe"].as_array().expect("hangar") {
        let name = entry["name"].as_str().unwrap_or_default();
        let listed = uex_ships.name_of(name).or_else(|| uex_ships.name_continuing(name)).unwrap_or(name);
        let price = market.buy_at(listed).first().map(|offer| offer.price);
        lines.push(format!("PRICE\t{name}\t{listed}\t{price:?}"));
    }
    println!("{total} names, {wrong} cargo differs");
    for line in &lines {
        println!("{line}");
    }
    if let Ok(out) = std::env::var("VERSEKIT_LOOKUP_OUT") {
        std::fs::write(out, lines.join("\n")).expect("out");
    }
}
