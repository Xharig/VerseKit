// Verse-Kit — companion for Star Citizen.
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

//! Everything VerseKit reads of one game channel, written as files for a comparison of two channels (a patch
//! against the one before): the game values, the full Basetool catalog p4kx builds, the names of the places, the
//! English texts and how many records of each type the DataForge holds. Only reads the archive, and only while the
//! game is not running: before every reading the process list is asked, and the run stops when the game started.
//!
//! Run with `VERSEKIT_CHANNEL_FOLDER=<…\LIVE> VERSEKIT_DUMP_OUT=<folder> cargo test -p versekit-gamedata --release
//! --test channel_dump_real -- --ignored --nocapture`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use p4kx_archive::Archive;
use p4kx_export::CatalogOptions;
use versekit_gamedata::{GameData, Language};

/// Stops the run when the game is running (or the process list says so).
fn game_closed(step: &str) {
    assert!(
        versekit_core::game_start::game_process() != Some(true),
        "Star Citizen runs: stopped before {step}, nothing more is read"
    );
}

fn write(out: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(out.join(name), bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
    println!("  {name}: {} KB", bytes.len() / 1024);
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn everything_read_of_a_channel_is_written_for_a_comparison() {
    let (Some(folder), Some(out)) = (std::env::var_os("VERSEKIT_CHANNEL_FOLDER"), std::env::var_os("VERSEKIT_DUMP_OUT")) else {
        return;
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("the output folder");
    game_closed("opening the archive");
    let game = GameData::open(&folder).expect("the archive");
    println!("{}: {:?} {:?} scmdb {:?}", game.archive_path().display(), game.branch(), game.version(), game.scmdb_build());
    let started = Instant::now();

    game_closed("the texts");
    let ini = game.english_ini_bytes().expect("the English texts");
    write(&out, "global.ini", &ini);
    let texts = game.localization(Language::English).expect("the texts");

    game_closed("the values");
    let values = game.values(&texts).expect("the game values");
    write(&out, "values.json", serde_json::to_string(&values).expect("values as JSON").as_bytes());

    game_closed("the DataForge");
    let bytes = game.dataforge_bytes().expect("the DataForge");
    let forge = p4kx_dataforge::DataForge::open(&bytes).expect("a readable DataForge");
    let mut types: BTreeMap<String, usize> = BTreeMap::new();
    let mut unreadable = 0;
    for record in forge.records() {
        match record.type_name() {
            Ok(kind) => *types.entry(kind.to_string()).or_default() += 1,
            Err(_) => unreadable += 1,
        }
    }
    println!("  {} record types, {unreadable} records without a readable type", types.len());
    write(&out, "types.json", serde_json::to_string(&types).expect("types").as_bytes());
    let names = versekit_gamedata::place_names::names_from_dataforge(&bytes, &ini).expect("the names of the places");
    write(&out, "places.json", serde_json::to_string(&names).expect("places").as_bytes());
    drop(forge);
    drop(bytes);

    game_closed("the catalog");
    let archive = Archive::open(game.archive_path()).expect("the archive for p4kx");
    let built = p4kx_export::build_catalog(&archive, &CatalogOptions::new(SystemTime::UNIX_EPOCH)).expect("the catalog");
    let (json, summary) = built.into_parts();
    write(&out, "catalog.json", &json);
    write(&out, "catalog-summary.txt", format!("{summary:#?}").as_bytes());
    game_closed("the end");
    println!("done in {:?}", started.elapsed());
}
