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

//! The pictures a player taught in a Drake Golem at 3440 × 1440 (`tests/data/signatur-golem-3440.HERKUNFT.txt`):
//! eighteen signatures and four numbers that are none.

use super::*;

/// The crops of the set: (typed number, file name, crop).
fn golem_crops() -> Vec<(u64, String, Gray)> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/signatur-golem-3440");
    let mut out: Vec<(u64, String, Gray)> = std::fs::read_dir(&folder)
        .expect("folder")
        .flatten()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let picture = image::open(entry.path()).expect("picture").to_luma8();
            let (w, h) = (picture.width() as usize, picture.height() as usize);
            let number = name.split('_').next().unwrap().parse().unwrap();
            (number, name, Gray::new(w, h, picture.into_raw()))
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// The four numbers that are no signature: a heading in its ring, a distance, a speed.
fn is_signature(number: u64) -> bool {
    number >= 1_000
}

/// The possible values for the set: the test values and every signature of it.
fn golem_values() -> Vec<u64> {
    let mut all = values();
    all.extend(golem_crops().iter().map(|c| c.0).filter(|n| is_signature(*n)));
    all.sort();
    all.dedup();
    all
}

/// The report of 22.11.2026: a heading in its ring, a distance and a speed were offered for teaching as if they
/// were signatures, and taught. Beside none of them stands the location symbol: none is read, none offered, none
/// taught, even when its number is a possible value.
#[test]
fn a_heading_a_distance_and_a_speed_are_neither_read_nor_offered_nor_taught() {
    let known = shipped();
    let mut checked = 0;
    for (number, name, crop) in golem_crops().into_iter().filter(|c| !is_signature(c.0)) {
        checked += 1;
        let mut values = golem_values();
        values.push(number);
        let reading = read(&crop, &known, &values);
        assert_eq!(reading.value, None, "{name}: {reading:?}");
        assert!(!has_locator(&crop, &reading), "{name}: {reading:?}");
        assert!(!has_pill(&[&crop]), "{name}: no pill in the picture");
        let typed = name.split('_').next().unwrap();
        let folder = testdir::fresh(&format!("golem-falsch-{number}"));
        let file = folder.join("eigene.json").to_string_lossy().into_owned();
        let samples = folder.join("bilder").to_string_lossy().into_owned();
        let taught = learn(&crop, typed, &known, &file, &samples);
        assert!(matches!(taught, Err("anlernen_ortszeichen" | "anlernen_anzahl")), "{name}: {taught:?}");
        assert!(!std::path::Path::new(&file).exists(), "{name}: nothing stored");
        assert!(super::super::samples(&samples).is_empty(), "{name}: no image kept");
    }
    assert_eq!(checked, 4);
}

/// The eighteen signatures of the same set carry the location symbol, are read as before (never wrong) and can be
/// taught.
#[test]
fn the_signatures_of_the_golem_carry_their_symbol_and_are_read_and_taught() {
    let (known, values) = (shipped(), golem_values());
    let (mut read_right, mut checked) = (0, 0);
    for (number, name, crop) in golem_crops().into_iter().filter(|c| is_signature(c.0)) {
        checked += 1;
        let reading = read(&crop, &known, &values);
        assert!(reading.value.is_none_or(|v| v == number), "{name}: {reading:?}");
        read_right += usize::from(reading.value == Some(number));
        assert!(has_locator(&crop, &reading), "{name}: {reading:?}");
        assert!(has_pill(&[&crop]), "{name}");
        assert!(lesson(&crop, &number.to_string(), true).is_ok(), "{name}");
    }
    assert_eq!(checked, 18);
    // As many as before the symbol was asked for when reading.
    assert!(read_right >= 13, "{read_right}");
}
