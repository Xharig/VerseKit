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

//! Times, on the data of this computer (only read), the parts the tiles of the hangar and of the translation of
//! the overview read: run by hand with `cargo test -p versekit-core --release --test overview_timing -- --ignored
//! --nocapture`.

use std::time::Instant;

use versekit_core::paths::{Host, Paths};

fn timed<T>(what: &str, run: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = run();
    println!("{what}: {} ms", start.elapsed().as_millis());
    out
}

#[test]
#[ignore = "reads the data of this computer"]
fn the_parts_of_the_slow_tiles() {
    let paths = Paths::new(Host::current());
    for round in 0..2 {
        println!("-- round {round}");
        timed("settings", || paths.settings().len());
        timed("game_folder", || paths.game_folder());
        let channels = timed("installed_channels", || paths.installed_channels());
        println!("   {} channels", channels.len());
        timed("channel_sources", || versekit_core::translation::channel_sources(&paths).len());
        let source = paths.setting(versekit_core::translation::SETTING_SOURCE).unwrap_or_default();
        let today = versekit_core::translation::today();
        timed("status_text main", || {
            versekit_core::translation::status_text(&paths, &source, None, versekit_core::language::Lang::De, &today)
        });
        for (name, _, _) in &channels {
            timed(&format!("status_text {name}"), || {
                versekit_core::translation::status_text(&paths, &source, Some(name), versekit_core::language::Lang::De, &today)
            });
        }
        let file = std::env::var("VERSEKIT_HANGAR_FILE").unwrap_or_else(|_| paths.app_file(versekit_core::hangar::FILE));
        println!("   hangar file: {file}");
        let data = timed("hangar::read", || versekit_core::hangar::read(&file));
        println!("   hangar read: {}", data.is_some());
    }
}
