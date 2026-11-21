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

//! Reads test pictures of the crafting terminal with the text recognition of Windows, as the scan in the game
//! will, and counts the outcome against the name each picture should give. The pictures lie as raw BGRA in a folder
//! of their own (`tools/bauplan_pruefbild.ps1` makes them from screenshots), beside a list `pruefbilder.json`:
//!
//! ```json
//! [{"datei": "sedulity_5120x1440.bgra", "zeiger": [1630, 702], "erwartet": "Sedulity"},
//!  {"datei": "leer_5120x1440.bgra", "zeiger": [2400, 700], "erwartet": null}]
//! ```
//!
//! `zeiger` is where the mouse pointer stood, `erwartet` the catalog name the hint shows (`null`: none). The
//! pictures and the catalog are only read.
//!
//! Run with `VERSEKIT_BLUEPRINT_PICTURES=<folder> VERSEKIT_CATALOG=<katalog-spieldaten.json> cargo test -p
//! versekit-core --release --test blueprint_scan_real -- --ignored --nocapture`; with `VERSEKIT_INI_EN` and
//! `VERSEKIT_INI_GAME` (two `global.ini`) also the names in the language of the game.

#![cfg(windows)]

use std::path::Path;

use versekit_core::blueprint_scan::{Names, Tally, read_at, translation_pairs};
use versekit_core::catalog_cache::parse;
use versekit_core::jsonio::{Value, read_json};
use versekit_core::win_ocr::{Bitmap, Recognition};

/// Width and height from a name `…_<width>x<height>.bgra`.
fn size_of(name: &str) -> Option<(i64, i64)> {
    let stem = name.strip_suffix(".bgra")?;
    let (w, h) = stem.rsplit('_').next()?.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn field<'a>(item: &'a Value, key: &str) -> Option<&'a Value> {
    match item {
        Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

/// Reads every picture of a folder whole (in the tiles of the refinery search) and prints each line that matches
/// the catalog: what the recognition makes of the hints, without knowing where the pointer stood. Run with
/// `VERSEKIT_BLUEPRINT_SURVEY=<folder of .bgra>` and `VERSEKIT_CATALOG`.
#[test]
#[ignore = "reads test pictures of the game"]
fn survey_of_whole_pictures() {
    use versekit_core::blueprint_scan::{Certainty, lines_of, match_line, with_joined};
    use versekit_core::refinery_scan::{Ocr, merge_words, search_jobs};
    let (Some(folder), Ok(catalog)) =
        (std::env::var_os("VERSEKIT_BLUEPRINT_SURVEY"), std::env::var("VERSEKIT_CATALOG"))
    else {
        return;
    };
    let names = Names::of_catalog(&parse(&read_json(&catalog).expect("the catalog")));
    let mut files: Vec<String> = std::fs::read_dir(&folder)
        .expect("the folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bgra"))
        .collect();
    files.sort();
    for file in files {
        let (width, height) = size_of(&file).expect("width and height in the name");
        let raw = std::fs::read(Path::new(&folder).join(&file)).expect("the picture");
        let bitmap = Bitmap { width, height, raw };
        let mut ocr = Recognition::new(&bitmap);
        let words = merge_words(&ocr.read(&search_jobs(width, height)).expect("read"));
        println!("== {file}");
        for line in with_joined(&lines_of(&words)) {
            let reading = match_line(&line, &names);
            if reading.certainty > Certainty::None {
                let offered: Vec<&str> = reading.candidates.iter().take(3).map(|(n, _)| n.as_str()).collect();
                println!("  {:?} {:?} {:?} <- {line:?} {offered:?}", reading.certainty, reading.how, reading.name);
            }
        }
    }
}

/// The reading at the pointer over every picture of a folder, the pointer put where the hint begins: a reading
/// of the whole picture finds the hint (the matched line not in capitals), the pointer is set a little above and
/// left of its first word, and [`read_at`] reads there as the scan in the game will. Counted against the name the
/// whole reading found. Same variables as [`survey_of_whole_pictures`].
#[test]
#[ignore = "reads test pictures of the game"]
fn reading_at_the_pointer_where_the_hint_stands() {
    use versekit_core::blueprint_scan::{Certainty, in_capitals, match_line};
    use versekit_core::refinery_scan::{Ocr, merge_words, rows, search_jobs};
    let (Some(folder), Ok(catalog)) =
        (std::env::var_os("VERSEKIT_BLUEPRINT_SURVEY"), std::env::var("VERSEKIT_CATALOG"))
    else {
        return;
    };
    let names = Names::of_catalog(&parse(&read_json(&catalog).expect("the catalog")));
    let mut files: Vec<String> = std::fs::read_dir(&folder)
        .expect("the folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bgra"))
        .collect();
    files.sort();
    let mut tally = Tally::default();
    let mut wrong = Vec::new();
    let mut unsure_reference = 0;
    for file in files {
        let (width, height) = size_of(&file).expect("width and height in the name");
        let raw = std::fs::read(Path::new(&folder).join(&file)).expect("the picture");
        let bitmap = Bitmap { width, height, raw };
        let words = merge_words(&Recognition::new(&bitmap).read(&search_jobs(width, height)).expect("read"));
        // The hint: the first line read sure or likely and not in capitals, with the frame of its first word.
        let hint = rows(&words).into_iter().find_map(|row| {
            let line = row.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
            let reading = match_line(&line, &names);
            (reading.certainty >= Certainty::Likely && !in_capitals(&line))
                .then(|| (reading.name.clone(), reading.certainty == Certainty::Sure, row[0].x, row[0].y))
        });
        let Some((expected, sure, x, y)) = hint else { continue };
        let step = height as f64 / 1440.0;
        let pointer = ((x - 10.0 * step) as i64, (y - 10.0 * step) as i64);
        let started = std::time::Instant::now();
        let outcome = read_at((width, height), pointer, &mut Recognition::new(&bitmap), &names).expect("read");
        println!(
            "== {file}: pointer {pointer:?} → {:?} {:?} (whole picture: {expected:?}) {:?} in {} ms, lines {:?}",
            outcome.certainty,
            outcome.name,
            outcome.candidates,
            started.elapsed().as_millis(),
            outcome.lines
        );
        // Only a sure reading of the whole picture is a name to count against; an unsure one may be the beginning of
        // the name the hint shows.
        if !sure {
            unsure_reference += 1;
            continue;
        }
        let before = tally.wrong;
        tally.count(&outcome, expected.as_deref());
        if tally.wrong > before {
            wrong.push(file.clone());
        }
    }
    println!("{tally}; {unsure_reference} more against an unsure reading of the whole picture (shown, not counted)");
    assert!(wrong.is_empty(), "sure and wrong: {wrong:?}");
}

#[test]
#[ignore = "reads test pictures of the game"]
fn every_test_picture_gives_its_blueprint_or_asks() {
    let (Some(folder), Ok(catalog)) =
        (std::env::var_os("VERSEKIT_BLUEPRINT_PICTURES"), std::env::var("VERSEKIT_CATALOG"))
    else {
        return;
    };
    let folder = Path::new(&folder);
    let mut names = Names::of_catalog(&parse(&read_json(&catalog).expect("the catalog")));
    if let (Ok(english), Ok(game)) = (std::env::var("VERSEKIT_INI_EN"), std::env::var("VERSEKIT_INI_GAME")) {
        let read = |p: &str| String::from_utf8_lossy(&std::fs::read(p).expect("global.ini")).into_owned();
        let taken = names.add_translations(&translation_pairs(&read(&english), &read(&game)));
        println!("{taken} names of the language of the game");
    }
    let list = read_json(&folder.join("pruefbilder.json").to_string_lossy()).expect("pruefbilder.json");
    let Value::Array(items) = list else { panic!("pruefbilder.json is no list") };
    assert!(!items.is_empty(), "no pictures");
    let mut tally = Tally::default();
    let mut wrong = Vec::new();
    for item in &items {
        let Some(Value::Str(file)) = field(item, "datei") else { panic!("datei missing: {item:?}") };
        let pointer = match field(item, "zeiger") {
            Some(Value::Array(xy)) if xy.len() == 2 => {
                let n = |v: &Value| match v {
                    Value::Int(i) => i.parse().expect("pointer"),
                    Value::Float(f) => *f as i64,
                    other => panic!("zeiger: {other:?}"),
                };
                (n(&xy[0]), n(&xy[1]))
            }
            other => panic!("zeiger of {file}: {other:?}"),
        };
        let expected = match field(item, "erwartet") {
            Some(Value::Str(name)) => Some(name.as_str()),
            _ => None,
        };
        let (width, height) = size_of(file).expect("width and height in the name");
        let raw = std::fs::read(folder.join(file)).expect("the picture");
        assert_eq!(raw.len() as i64, width * height * 4, "{file}");
        let bitmap = Bitmap { width, height, raw };
        let mut ocr = Recognition::new(&bitmap);
        let outcome = read_at((width, height), pointer, &mut ocr, &names).expect("read");
        println!(
            "== {file}: {:?} {:?} (expected {expected:?}), offered {:?}, lines {:?}",
            outcome.certainty, outcome.name, outcome.candidates, outcome.lines
        );
        let before = tally.wrong;
        tally.count(&outcome, expected);
        if tally.wrong > before {
            wrong.push(file.clone());
        }
    }
    println!("{tally}");
    assert!(wrong.is_empty(), "sure and wrong: {wrong:?}");
}
