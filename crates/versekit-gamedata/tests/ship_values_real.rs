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

//! The base values of the ships of the installed game (size, SCM speed, cargo, field `werte` of the ships file): most
//! player ships have each, every value lies in its range, the ships named in `VERSEKIT_SHIPS_SHOW` (comma separated
//! keys, default a few of every size) are printed, and a few values of LIVE 4.10.2 are checked as they stand.
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> cargo test -p versekit-gamedata --release --test ship_values_real -- --ignored
//! --nocapture`; on a machine where VerseKit cannot tell whether the game runs, with `VERSEKIT_TESTUMGEBUNG=1` as well.

use versekit_core::jsonio::Value;
use versekit_gamedata::GameData;

const SHOW: &str = "aegsgladius,drakcutlassblack,miscfreelancermax,anvlcarrack,rsiconstellationandromeda,orig300i,\
drakcaterpillar,anvlarrow,crusstarlifterc2,drakgolem,miscreliant,xiannox,origx1,drakmule";

/// The values of LIVE 4.10.2 (12913470) as the game data have them, compared with UEX and starcitizen.tools on
/// 12.11.2026: key, SCM speed, length, width, height, cargo. The Caterpillar lies on another axis in the data (its
/// length is `z`); the Cutlass Black shares its box with the Shiv of another maker. Another build may change them;
/// then the line says which differ.
const FIXED: [(&str, f64, [f64; 3], f64); 4] = [
    ("aegsgladius", 226.0, [21.0, 17.5, 5.5], 0.0),
    ("drakcutlassblack", 217.0, [37.5, 26.5, 11.5], 46.0),
    ("anvlcarrack", 140.0, [126.0, 74.0, 30.0], 456.0),
    ("drakcaterpillar", 130.0, [111.5, 39.5, 13.4], 576.0),
];

/// Vehicles whose box in LIVE 4.10.2 is the stand-in 6 × 8.75 × 3.5 m that vehicles of several makers share (the
/// Mule is 4.2 × 3 × 2.7 m, the Cyclone 6 × 4 × 2.5 m by starcitizen.tools): no size.
const STAND_IN: [&str; 3] = ["drakmule", "tmblcyclone", "rsiursarover"];

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Float(x) => Some(*x),
        Value::Int(text) => text.parse().ok(),
        _ => None,
    }
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn most_ships_get_size_scm_speed_and_cargo_from_the_game_data() {
    let path = std::env::var("VERSEKIT_P4K")
        .expect("VERSEKIT_P4K names no Data.p4k: this check reads the game data and proves nothing without them");
    let game = GameData::open_file(&path).expect("a readable Data.p4k");
    let started = std::time::Instant::now();
    let (ships, _) = game.ship_slots().expect("the ships");
    println!("ships file read in {:.1} s", started.elapsed().as_secs_f64());
    // The ships file as the program keeps it, into the file `VERSEKIT_SHIPS_FILE` names (for the pictures of the
    // tool `bildwerkzeug`).
    if let Ok(file) = std::env::var("VERSEKIT_SHIPS_FILE") {
        std::fs::write(file, versekit_core::jsonio::dumps(&ships, None, false)).expect("the ships file");
    }
    let Some(Value::Object(list)) = ships.get("schiffe") else { panic!("no ships") };
    assert!(list.len() > 150, "{} player ships", list.len());
    let (mut scm, mut size, mut cargo) = (0, 0, 0);
    let mut out_of_range = Vec::new();
    for (key, entry) in list {
        let Some(values) = entry.get("werte") else { continue };
        if let Some(v) = number(values.get("scm")) {
            scm += 1;
            if !(10.0..=1_500.0).contains(&v) {
                out_of_range.push(format!("{key}: SCM {v}"));
            }
        }
        if let Some(Value::Array(sides)) = values.get("masse") {
            size += 1;
            let sides: Vec<f64> = sides.iter().filter_map(|v| number(Some(v))).collect();
            // Three sides, the longest first (the length) and the shortest last (the height).
            if sides.len() != 3
                || sides.iter().any(|v| !(0.5..=1_500.0).contains(v))
                || sides.windows(2).any(|w| w[0] < w[1])
            {
                out_of_range.push(format!("{key}: Maße {sides:?}"));
            }
        }
        if let Some(v) = number(values.get("fracht")) {
            cargo += 1;
            if !(0.0..=100_000.0).contains(&v) {
                out_of_range.push(format!("{key}: Fracht {v}"));
            }
        }
    }
    let total = list.len();
    println!("{total} ships: SCM {scm}, size {size}, cargo {cargo}");
    // Every ship with its values, one per line, into the file `VERSEKIT_SHIPS_DUMP` names (for a comparison by hand).
    if let Ok(dump) = std::env::var("VERSEKIT_SHIPS_DUMP") {
        let lines: Vec<String> = list
            .iter()
            .map(|(key, entry)| {
                let name = entry.get("name").and_then(Value::as_str).unwrap_or("?");
                format!("{key}\t{name}\t{:?}", entry.get("werte"))
            })
            .collect();
        std::fs::write(dump, lines.join("\n")).expect("the dump");
    }
    assert!(out_of_range.is_empty(), "{}", out_of_range.join("\n"));
    // Ground vehicles and a few ships without a flight controller of their own have no SCM speed; most have each.
    for (name, count) in [("SCM", scm), ("size", size), ("cargo", cargo)] {
        assert!(count * 10 >= total * 7, "{name}: only {count} of {total}");
    }
    let show = std::env::var("VERSEKIT_SHIPS_SHOW").unwrap_or_else(|_| SHOW.to_string());
    for key in show.split(',').map(str::trim).filter(|k| !k.is_empty()) {
        let entry = list.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let name = entry.and_then(|e| e.get("name")).and_then(Value::as_str).unwrap_or("?");
        println!("{key} ({name}): {:?}", entry.and_then(|e| e.get("werte")));
    }
    let mut differ = Vec::new();
    for (key, speed, sides, scu) in FIXED {
        let values =
            list.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.get("werte")).unwrap_or_else(|| panic!("{key}"));
        let read_sides: Vec<f64> = match values.get("masse") {
            Some(Value::Array(items)) => items.iter().filter_map(|v| number(Some(v))).collect(),
            _ => Vec::new(),
        };
        let read = (number(values.get("scm")), read_sides, number(values.get("fracht")));
        if read != (Some(speed), sides.to_vec(), Some(scu)) {
            differ.push(format!("{key}: {read:?}, LIVE 4.10.2 had {speed}, {sides:?}, {scu}"));
        }
    }
    for key in STAND_IN {
        let values = list.iter().find(|(k, _)| k == key).unwrap_or_else(|| panic!("{key}")).1.get("werte");
        if values.and_then(|v| v.get("masse")).is_some() {
            differ.push(format!("{key}: the stand-in box is shown as its size"));
        }
    }
    // Ships whose cargo lies outside their cargo grids (mining pods, the cargo area of the 135c): no cargo value at
    // all, not the too low sum of the grids. And ships with a hold the check must keep: Liberator 400, Galaxy 64
    // (without module, as the fleet manager of the game shows), Hull C 4,608, C2 696.
    for key in ["argomole", "miscprospector", "drakgolem", "orig135c"] {
        let values = list.iter().find(|(k, _)| k == key).unwrap_or_else(|| panic!("{key}")).1.get("werte");
        if values.and_then(|v| v.get("fracht")).is_some() {
            differ.push(format!("{key}: shows a cargo value though its cargo space lies outside the cargo grids"));
        }
    }
    for (key, scu) in [("anvlliberator", 400.0), ("rsigalaxy", 64.0), ("mischullc", 4608.0), ("crusstarlifterc2", 696.0)] {
        let read = list
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.get("werte"))
            .and_then(|v| number(v.get("fracht")));
        if read != Some(scu) {
            differ.push(format!("{key}: cargo {read:?}, wanted {scu}"));
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}
