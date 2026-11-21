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

//! Reads the stand of a real archive twice and compares the two: nothing may differ. Prints how long a reading
//! takes and how large the stand is, as numbers only.
//!
//! Run with `VERSEKIT_P4K=<channel folder of a copy, never the installed game> cargo test -p versekit-gamedata
//! --release --test patch_stand_real -- --ignored --nocapture`.

use versekit_core::stand_diff;
use versekit_core::stands;
use versekit_gamedata::{GameData, stand_read};

#[test]
#[ignore = "reads a real archive"]
fn a_real_archive_read_twice_gives_no_difference() {
    let Some(folder) = std::env::var_os("VERSEKIT_P4K") else { return };
    let archive = std::path::Path::new(&folder).join("Data.p4k");
    let game = GameData::open_file_after(&archive, &|| true, versekit_core::archive_lock::POLL).expect("the archive");
    let started = std::time::Instant::now();
    let first = stand_read::read_stand(&game, "2026-01-01").expect("a stand");
    let took = started.elapsed();
    let second = stand_read::read_stand(&game, "2026-01-01").expect("a stand");
    let size = versekit_core::patch_changes::compact(&first.to_value()).len();
    println!("reading took {:.1} s, entries {}, stand as text {} bytes", took.as_secs_f64(), first.entry_count(), size);
    for (name, area) in &first.areas {
        println!("  {name}: {} entries", area.entries.len());
    }
    println!("probe {:?}", first.probe);
    if let Some(forms) = first.areas.get(stands::AREA_FORMS) {
        let missing: Vec<&str> =
            stand_read::WATCHED_STRUCTS.iter().copied().filter(|name| !forms.entries.contains_key(*name)).collect();
        println!("watched structs {}, found {}, not in the archive: {missing:?}", stand_read::WATCHED_STRUCTS.len(), forms.entries.len());
    }
    let cmp = stand_diff::compare(&first, &second);
    assert!(cmp.is_empty(), "the same archive read twice differs: {} rows, {:?}", cmp.rows.len(), cmp.warnings);
    assert!(stands::built_in(&first.channel));
}
