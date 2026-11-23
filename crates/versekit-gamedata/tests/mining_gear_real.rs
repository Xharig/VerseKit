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

//! The mining equipment of the installed game (file `spiel-bergbau.json`): laser heads with power, every module and
//! gadget, each with the values the game data hold, and the parts of a common MOLE fitting among them. Every part is
//! printed with its values, one per line.
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> cargo test -p versekit-gamedata --release --test mining_gear_real -- --ignored
//! --nocapture`; `VERSEKIT_MINING_FILE` names a file to write the result into (never the data of the program).

use versekit_core::jsonio::Value;
use versekit_gamedata::GameData;

/// Parts a MOLE flies with (heads Helix II and Hofstede-S2, modules Focus III, Rieger-C3 and FLTR-XL), by a piece of
/// their name.
const MOLE_PARTS: [(&str, &str); 5] =
    [("koepfe", "Helix II"), ("koepfe", "Hofstede-S2"), ("module", "Focus III"), ("module", "Rieger-C3"), ("module", "FLTR-XL")];

fn list<'a>(file: &'a Value, key: &str) -> &'a [Value] {
    match file.get(key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn line(entry: &Value) -> String {
    let Value::Object(fields) = entry else { return String::new() };
    fields
        .iter()
        .filter(|(k, _)| k != "ref")
        .map(|(k, v)| format!("{k}={}", versekit_core::jsonio::dumps(v, None, false)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn heads_modules_and_gadgets_come_with_their_values() {
    let path = std::env::var("VERSEKIT_P4K")
        .expect("VERSEKIT_P4K names no Data.p4k: this check reads the game data and proves nothing without them");
    let game = GameData::open_file(&path).expect("a readable Data.p4k");
    let forge = game.dataforge_bytes().expect("the DataForge");
    let (_, _, mining) = game.slots_and_mining_with(&forge).expect("the mining file");
    if let Ok(file) = std::env::var("VERSEKIT_MINING_FILE") {
        std::fs::write(file, versekit_core::jsonio::dumps(&mining, Some(2), false)).expect("the mining file");
    }
    println!("build {:?}", mining.get("spielversion"));
    for key in ["koepfe", "module", "gadgets"] {
        println!("== {key}: {}", list(&mining, key).len());
        for entry in list(&mining, key) {
            println!("{}", line(entry));
        }
    }
    let heads = list(&mining, "koepfe");
    assert!(heads.len() >= 10, "{} heads", heads.len());
    assert!(list(&mining, "module").len() >= 10, "modules");
    assert!(list(&mining, "gadgets").len() >= 5, "gadgets");
    let power = |e: &Value| match e.get("leistung") {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(t)) => t.parse().unwrap_or(0.0),
        _ => 0.0,
    };
    assert!(heads.iter().all(|h| power(h) > 0.0), "a head without power");
    let mut missing = Vec::new();
    for (key, piece) in MOLE_PARTS {
        let found = list(&mining, key)
            .iter()
            .any(|e| e.get("name").and_then(Value::as_str).is_some_and(|n| n.contains(piece)));
        if !found {
            missing.push(format!("{key}: {piece}"));
        }
    }
    assert!(missing.is_empty(), "missing: {}", missing.join(", "));
}
