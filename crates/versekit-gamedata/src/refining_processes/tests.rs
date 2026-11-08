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

use versekit_core::refining::{METHODS, Methods, Origin};

use super::*;

#[test]
fn a_line_of_the_terminal_reads_as_yield_speed_and_cost_advantage() {
    assert_eq!(levels_of("High Yield // High Cost // Slightly Fast"), Some([3, 3, 1]));
    assert_eq!(levels_of("High Yield // Low Cost // Slowest"), Some([3, 0, 3]));
    assert_eq!(levels_of("Moderate Yield // Moderate Cost // Moderate Speed\n"), Some([2, 2, 2]), "a line feed");
    assert_eq!(levels_of("Low Yield // Low Cost // Slightly Slow "), Some([1, 2, 3]));
    assert_eq!(levels_of("Low Yield // Moderate Cost // Very Fast"), Some([1, 3, 2]));
    assert_eq!(levels_of("Fastest // Low Yield // High Cost"), Some([1, 3, 1]), "in any order");
    for (word, speed) in [("Very Slow", 1), ("Slow", 1), ("Fast", 3)] {
        assert_eq!(levels_of(&format!("High Yield // High Cost // {word}")), Some([3, speed, 1]), "{word}");
    }
    assert_eq!(levels_of("High Yield // High Cost"), None, "two parts");
    assert_eq!(levels_of("High Yield // High Cost // Fast // Fast"), None, "four parts");
    assert_eq!(levels_of("Huge Yield // High Cost // Fast"), None, "an unknown size");
    assert_eq!(levels_of("High Yield // High Cost // Warp"), None, "an unknown speed");
    assert_eq!(levels_of("High Yield // Low Yield // Fast"), None, "no costs");
}

#[test]
fn the_text_of_the_file_is_read_back_by_the_mining_page_with_the_build_and_the_channel() {
    // The nine methods of the table, by their names and levels: the file the reading writes for them.
    let processes: Vec<Process> =
        METHODS.iter().map(|(_, name, levels)| Process { name: name.to_string(), levels: *levels }).collect();
    let read = Methods::from_game_text(&text(&processes, Some("4.10.2-LIVE.12913470"), "LIVE")).expect("all nine");
    assert_eq!(read.origin, Origin::Game { build: "4.10.2-LIVE.12913470".into(), channel: "LIVE".into() });
    assert!(read.differing_from_table().is_empty());
    // Eight are not enough: the page keeps its table.
    assert!(Methods::from_game_text(&text(&processes[..8], None, "PTU")).is_err());
    assert_eq!(channel_of(Path::new("C:/Games/StarCitizen/PTU/Data.p4k")), "PTU");
    assert_eq!(channel_of(Path::new("/games/StarCitizen/live/Data.p4k")), "LIVE");
}

#[test]
fn the_records_found_are_put_together_each_method_once_and_a_doubtful_one_left_out() {
    let found = |list: &[(&str, Option<[u8; 3]>)]| list.iter().map(|(n, l)| (n.to_string(), *l)).collect();
    let read = gather(found(&[
        ("Kazen Winnowing", Some([1, 2, 3])),
        ("Cormack Method", Some([1, 3, 2])),
        // The second record of the same method with the same levels counts once.
        ("Cormack Method", Some([1, 3, 2])),
        ("XCR Reaction", None),
        ("Gaskin Process", Some([2, 3, 1])),
        ("Gaskin Process", Some([3, 1, 2])),
        // A third one after two that disagree does not bring the name back.
        ("Gaskin Process", Some([2, 3, 1])),
    ]));
    let names: Vec<&str> = read.processes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Cormack Method", "Kazen Winnowing"], "ordered by name");
    assert_eq!(read.left_out.len(), 2, "{:?}", read.left_out);
    assert!(read.left_out[0].starts_with("XCR Reaction") && read.left_out[1].starts_with("Gaskin Process"));
}

#[test]
fn a_reading_without_all_nine_never_replaces_the_file() {
    let dir = versekit_core::testdir::Dir::new("raffinerie-ganz");
    let file = dir.path().join(FILE).to_string_lossy().into_owned();
    let all: Vec<Process> =
        METHODS.iter().map(|(_, name, levels)| Process { name: name.to_string(), levels: *levels }).collect();
    let whole = Read { processes: all.clone(), left_out: Vec::new() };
    assert!(matches!(keep(&file, &whole, Some("4.10.2"), "LIVE"), Kept::Written));
    let before = std::fs::read_to_string(&file).unwrap();
    let part = Read { processes: all[..8].to_vec(), left_out: vec!["XCR Reaction: no line of three levels".into()] };
    let Kept::Incomplete(why) = keep(&file, &part, Some("4.10.3"), "LIVE") else { panic!("written") };
    assert!(why.contains("missing") && why.contains("XCR Reaction"), "{why}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before, "the file of all nine stays");
}

#[test]
fn the_file_is_written_whole_and_replaced() {
    let dir = versekit_core::testdir::Dir::new("raffinerie-verfahren");
    let file = dir.path().join(FILE).to_string_lossy().into_owned();
    let one = [Process { name: "Pyrometric Chromalysis".into(), levels: [3, 3, 1] }];
    write(&file, &one, Some("4.10.2"), "LIVE").expect("written");
    write(&file, &one, Some("4.10.3"), "LIVE").expect("replaced");
    let read = versekit_core::jsonio::read_json(&file).expect("JSON");
    assert_eq!(read.get("build").and_then(Json::as_str), Some("4.10.3"));
    assert_eq!(read.get("verfahren").map(|list| matches!(list, Json::Array(items) if items.len() == 1)), Some(true));
}
