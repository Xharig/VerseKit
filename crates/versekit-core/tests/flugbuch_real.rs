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

//! The flight log on real logs: a survey and a cross-check against the statistics, run by hand on a COPY of the
//! logs (never the folder of the game):
//!
//! ```text
//! VERSEKIT_FLUGBUCH_LOGS=<folder with copies of Game.log files> cargo test --release -p versekit-core \
//!     --test flugbuch_real -- --ignored --nocapture
//! ```
//!
//! Per log it says how many events of every kind the flight log found, and fails where it counts something
//! differently from `statistik.json` where both count the same thing: the choices of a target, the deaths and the
//! shards joined. A kind with no event in a big log is a warning sign (the game changed a line).

use std::path::Path;

use versekit_core::flugbuch::{self, Kind};
use versekit_core::injuries::Phrases;
use versekit_core::jsonio::Value;
use versekit_core::play_stats;

fn get<'a>(map: &'a versekit_core::jsonio::Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn logs_in(folder: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(folder)
        .expect("the folder of the logs")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("log")))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
}

fn length(map: &versekit_core::jsonio::Map, key: &str) -> usize {
    match get(map, key) {
        Some(Value::Array(items)) => items.len(),
        _ => 0,
    }
}

#[test]
#[ignore = "needs a folder of copies of real logs: VERSEKIT_FLUGBUCH_LOGS"]
fn the_flight_log_counts_what_the_statistics_count() {
    let Some(folder) = std::env::var_os("VERSEKIT_FLUGBUCH_LOGS") else {
        println!("VERSEKIT_FLUGBUCH_LOGS names no folder: nothing to read");
        return;
    };
    let phrases = Phrases::table();
    let mut totals = [0_usize; 13];
    let mut differences = Vec::new();
    for path in logs_in(Path::new(&folder)) {
        let (Some(book), Some(stats)) = (flugbuch::read_log(&path, None, &phrases), play_stats::read_log(&path, None))
        else {
            println!("{path}: no session");
            continue;
        };
        let counts: Vec<String> = Kind::ALL
            .iter()
            .enumerate()
            .map(|(i, kind)| {
                let n = book.count(*kind);
                totals[i] += n;
                format!("{} {n}", kind.stored())
            })
            .collect();
        println!("{path}: {}", counts.join(", "));
        let targets = match get(&stats, "zielwahlen") {
            Some(Value::Int(text)) => text.parse::<usize>().unwrap_or(0),
            _ => 0,
        };
        for (name, flight, statistics) in [
            ("targets", book.count(Kind::Target), targets),
            ("deaths", book.count(Kind::Death), length(&stats, "tode")),
            ("shards", book.count(Kind::Shard), length(&stats, "shards")),
        ] {
            if flight != statistics {
                differences.push(format!("{path}: {name}: flight log {flight}, statistics {statistics}"));
            }
        }
    }
    let summary: Vec<String> = Kind::ALL.iter().zip(totals).map(|(kind, n)| format!("{} {n}", kind.stored())).collect();
    println!("all together: {}", summary.join(", "));
    assert!(differences.is_empty(), "the two readings differ:\n{}", differences.join("\n"));
}
