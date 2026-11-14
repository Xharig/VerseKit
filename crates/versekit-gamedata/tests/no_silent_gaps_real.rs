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

//! Gaps that a read without an error can leave in the data of a build: a contractor that is a raw placeholder
//! (`~mission(…)`), and the Carrack without its cargo value. Reads the archive `VERSEKIT_P4K` names and writes
//! nothing; nothing of its content is printed except counts and class names of sources.
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> cargo test -p versekit-gamedata --release --test no_silent_gaps_real --
//! --ignored --nocapture`; on a machine where VerseKit cannot tell whether the game runs, with
//! `VERSEKIT_TESTUMGEBUNG=1` as well.

use versekit_core::jsonio::Value;
use versekit_gamedata::{GameData, Language};

fn open() -> GameData {
    let path = std::env::var("VERSEKIT_P4K")
        .expect("VERSEKIT_P4K names no Data.p4k: this check reads the game data and proves nothing without them");
    GameData::open_file(&path).expect("a readable Data.p4k")
}

#[test]
#[ignore = "reads the Data.p4k named by VERSEKIT_P4K"]
fn no_contractor_is_a_raw_placeholder() {
    let game = open();
    let english = game.localization(Language::English).expect("the texts");
    let catalog = game.blueprints(&english).expect("the blueprints");
    let (mut sources, mut contractors) = (0_usize, 0_usize);
    let mut raw = Vec::new();
    for blueprint in &catalog.blueprints {
        for source in &blueprint.sources {
            sources += 1;
            let Some(contractor) = &source.contractor else { continue };
            contractors += 1;
            if contractor.shown().contains("~mission(") {
                raw.push(source.id.clone());
            }
        }
    }
    println!("{sources} sources, {contractors} with a contractor, {} raw", raw.len());
    assert!(contractors > 0, "no source names a contractor: the check proves nothing");
    assert!(raw.is_empty(), "{} sources show a placeholder as contractor, e.g. {:?}", raw.len(), &raw[..raw.len().min(5)]);
}

#[test]
#[ignore = "reads the Data.p4k named by VERSEKIT_P4K"]
fn the_carrack_has_a_cargo_value() {
    let game = open();
    let (ships, _) = game.ship_slots().expect("the ships");
    let Some(Value::Object(list)) = ships.get("schiffe") else { panic!("no ships") };
    let cargo = |key: &str| {
        list.iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, ship)| ship.get("werte"))
            .and_then(|values| values.get("fracht"))
            .cloned()
    };
    let carrack = cargo("anvlcarrack");
    println!("anvlcarrack fracht {carrack:?}");
    assert!(matches!(carrack, Some(Value::Float(scu)) if scu >= 400.0), "{carrack:?}");
}
