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

//! Reads the processing methods of the refineries from the installed game's `Data.p4k` and compares them with the
//! table written down by hand (`versekit_core::refining::METHODS`). Only reads; the reading of the archive waits
//! while the game runs.
//!
//! Run with `cargo test -p versekit-gamedata --release --test refining_real -- --ignored --nocapture`. Without
//! `VERSEKIT_CHANNEL_FOLDER` the game folder comes from `GameData::open_installed`; with it, that channel folder is
//! read.

use versekit_core::refining::{METHODS, Methods};
use versekit_gamedata::{GameData, Language, refining_processes};

fn open() -> GameData {
    match std::env::var_os("VERSEKIT_CHANNEL_FOLDER") {
        Some(folder) => GameData::open(folder).unwrap(),
        None => GameData::open_installed().unwrap(),
    }
}

#[test]
#[ignore = "reads the installed game"]
fn the_methods_of_the_game_are_the_nine_of_the_table_with_its_levels() {
    let game = open();
    let texts = game.localization(Language::English).unwrap();
    let read = refining_processes::from_dataforge(&game.dataforge_bytes().unwrap(), &texts).unwrap();
    let processes = &read.processes;
    let channel = refining_processes::channel_of(game.archive_path());
    println!("archive: {} ({channel}, {:?})", game.archive_path().display(), game.scmdb_build());
    println!("left out: {:?}", read.left_out);
    for process in processes {
        println!("  {:<24} {:?}", process.name, process.levels);
    }
    let text = refining_processes::text(processes, game.scmdb_build().as_deref(), &channel);
    let methods = Methods::from_game_text(&text).expect("the nine methods of the table, by their names");
    // The nine levels as a whole are those of the table; which name carries which may differ.
    let mut read: Vec<[u8; 3]> = processes.iter().map(|process| process.levels).collect();
    let mut table: Vec<[u8; 3]> = METHODS.iter().map(|(_, _, levels)| *levels).collect();
    read.sort_unstable();
    table.sort_unstable();
    assert_eq!(read, table, "the nine levels of the table");
    println!("levels differing from the table by name: {:?}", methods.differing_from_table());
}
