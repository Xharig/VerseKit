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

//! Reads kept pictures of the refinery terminal with the text recognition of Windows, as the key in the game
//! does. The pictures lie as raw BGRA in a folder of their own, named `<anything>_<width>x<height>.bgra`; the test
//! prints what each reading gives and asks that every picture yields a table with lines.
//!
//! Run with `VERSEKIT_REFINERY_RAW=<folder> cargo test -p versekit-core --release --test refinery_real -- --ignored
//! --nocapture`.

#![cfg(windows)]

use versekit_core::refinery_scan::{
    Index, Job, Ocr, PASSES, Word, digits::Digits, find_tables, merge_words, read_image, rows, search_jobs,
};
use versekit_core::win_ocr::{Bitmap, Recognition};

/// Width and height from a name `…_<width>x<height>.bgra`.
fn size_of(name: &str) -> Option<(i64, i64)> {
    let stem = name.strip_suffix(".bgra")?;
    let (w, h) = stem.rsplit('_').next()?.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[test]
#[ignore = "reads pictures kept from the game"]
fn every_kept_picture_of_the_terminal_gives_its_lines() {
    let Some(folder) = std::env::var_os("VERSEKIT_REFINERY_RAW") else { return };
    let materials: Vec<String> =
        ["Iron", "Quantainium", "Laranite", "Tungsten", "Corundum", "Titanium", "Quartz", "Ouratite", "Agricium"]
            .map(String::from)
            .to_vec();
    let places: Vec<String> =
        ["ARC-L1 Wide Forest Station", "CRU-L1 Ambitious Dream Station", "HUR-L1 Green Glade Station", "Levski"]
            .map(String::from)
            .to_vec();
    let index = Index::new(&materials, |_| false).with_places(&places);
    let learned = Digits::default();
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .expect("the folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bgra"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no pictures");
    let mut failed = Vec::new();
    for name in names {
        let (width, height) = size_of(&name).expect("width and height in the name");
        let raw = std::fs::read(std::path::Path::new(&folder).join(&name)).expect("the picture");
        assert_eq!(raw.len() as i64, width * height * 4, "{name}");
        let bitmap = Bitmap { width, height, raw };
        let mut ocr = Recognition::new(&bitmap);
        let (jobs, _) = read_image((width, height), Some(&bitmap), &mut ocr, &index, &learned, None).expect("read");
        println!("== {name}: {} job(s)", jobs.len());
        if std::env::var_os("VERSEKIT_REFINERY_DEBUG").is_some() {
            // Every table with the rows of each enlarged reading, and the rows of the search in its column.
            let mut ocr = Recognition::new(&bitmap);
            let search = merge_words(&ocr.read(&search_jobs(width, height)).expect("search"));
            // The rows around the head of the station's profile, where the name of the station stands.
            for row in rows(&search) {
                let line: Vec<String> = row.iter().map(|w| format!("{}@{:.0},{:.0}h{:.0}", w.t, w.x, w.y, w.h)).collect();
                let text = line.join(" ").to_uppercase();
                if ["PROF", "STATION", "RAFFINERIESYS", "REFINERY"].iter().any(|k| text.contains(k)) {
                    println!("  top: {}", line.join("  "));
                }
            }
            for table in find_tables(&search, width, height) {
                println!("  table {:?} total {:?}", table.area, table.total);
                let (x, _, w, _) = table.area;
                let column: Vec<Word> =
                    search.iter().filter(|s| (x as f64) <= s.x + s.w / 2.0 && s.x + s.w / 2.0 <= (x + w) as f64).cloned().collect();
                for row in rows(&column) {
                    println!("    search: {}", row.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" | "));
                }
                let jobs: Vec<Job> = PASSES.iter().map(|(scale, mode)| Job { area: table.area, scale: *scale, mode: *mode }).collect();
                for (n, pass) in ocr.read(&jobs).expect("passes").iter().enumerate() {
                    for row in rows(pass) {
                        let line: Vec<String> = row.iter().map(|w| format!("{}@{:.0}", w.t, w.x + w.w / 2.0)).collect();
                        println!("    pass {n}: {}", line.join(" "));
                    }
                }
            }
        }
        if jobs.is_empty() {
            // What the search over the whole picture read: the rows that hold a head.
            let mut ocr = Recognition::new(&bitmap);
            let search = ocr.read(&search_jobs(width, height)).expect("search");
            let words = merge_words(&search);
            for row in rows(&words) {
                let line: Vec<String> = row.iter().map(|w| format!("{}@{:.0},{:.0}h{:.0}", w.t, w.x, w.y, w.h)).collect();
                let text = line.join(" ").to_uppercase();
                if ["QUAL", "UALI", "AUSB", "GEWONN", "MATERIA", "AUSGEW"].iter().any(|k| text.contains(k)) {
                    println!("  {}", line.join("  "));
                }
            }
        }
        for job in &jobs {
            println!(
                "total {:?} state {:?} remaining {:?} unsure {:?} station {:?}\n{}",
                job.total, job.state, job.remaining, job.unsure, job.station, job.text
            );
        }
        if !jobs.iter().any(|j| !j.text.trim().is_empty()) {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "no line read: {failed:?}");
}
