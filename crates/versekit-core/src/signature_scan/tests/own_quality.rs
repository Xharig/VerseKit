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

//! Which patterns are taught at all (too wide, in two parts, nearer another digit) and which images count as the
//! same (taught twice).

use super::own::{self, Problem};
use super::*;

/// A pattern drawn from rows of text: `#` is ink.
fn drawn_rows(rows: &[&str]) -> Pattern {
    let mut pattern = [0u8; CELLS];
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            if c == '#' {
                pattern[y * NORM_W + x] = 1;
            }
        }
    }
    pattern
}

/// A crop as the table of the page "Signatur" teaches it: every place with its digit.
fn places_of(key: &str, raster: &Gray) -> Vec<(char, Pattern)> {
    let digits: String = number_of(key).to_string();
    let Ok(lesson) = lesson(raster, &digits, false) else { return Vec::new() };
    lesson.digits.iter().copied().zip(lesson.patterns.iter().copied()).collect()
}

#[test]
fn every_digit_of_the_real_crops_is_usable() {
    let shipped = shipped();
    let (mut total, mut bad) = (0, Vec::new());
    for (set, key, _, raster) in keyed_crops() {
        for (digit, pattern) in places_of(&key, &raster) {
            total += 1;
            if let Some(why) = own::problem(digit, &pattern, &shipped, false) {
                bad.push(format!("{set}/{key}: {digit} {why:?}"));
            }
        }
    }
    assert!(total > 400, "the crops give {total} digits");
    // Measured: a few crops of the sent-in set are broken or wide; at most 8 of 500 are refused.
    assert!(bad.len() <= 8, "{} of {total} real digits would be refused: {bad:?}", bad.len());
}

#[test]
fn a_block_and_two_digits_grown_together_are_too_wide() {
    let shipped = shipped();
    let block = drawn_rows(&["################"; 12]);
    assert_eq!(own::problem('1', &block, &shipped, true), Some(Problem::TooWide));
    // Two ones side by side: as wide as high.
    let rows: Vec<String> = (0..12).map(|_| "###.......###....".to_string()).collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    assert_eq!(own::problem('1', &drawn_rows(&rows), &shipped, true), Some(Problem::TooWide));
}

#[test]
fn a_digit_with_a_comma_beside_it_is_two_parts() {
    let shipped = shipped();
    let mut rows: Vec<String> = (0..18).map(|_| "......##........".to_string()).collect();
    rows[15] = "......##..##....".to_string();
    rows[16] = "......##..##....".to_string();
    rows[17] = "......##.##.....".to_string();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    assert_eq!(own::problem('1', &drawn_rows(&rows), &shipped, true), Some(Problem::TwoParts));
}

#[test]
fn a_shipped_five_taught_as_a_four_is_nearer_another_digit() {
    let known = shipped();
    let five = known.digits.iter().find(|(c, _)| *c == '5').and_then(|(_, e)| e.first()).expect("a five").pattern;
    assert_eq!(own::problem('4', &five, &known, true), Some(Problem::OtherDigit));
    assert_eq!(own::problem('4', &five, &known, false), Some(Problem::OtherDigit));
    assert_eq!(own::problem('5', &five, &known, true), None);
}

#[test]
fn teaching_leaves_out_the_places_that_are_no_good_and_says_which() {
    let known = shipped();
    let five = known.digits.iter().find(|(c, _)| *c == '5').and_then(|(_, e)| e.first()).expect("a five").pattern;
    let wide = drawn_rows(&["################"; 12]);
    let mut comma_rows: Vec<String> = (0..18).map(|_| "......##........".to_string()).collect();
    for row in &mut comma_rows[15..18] {
        *row = "......##..##....".to_string();
    }
    let comma_rows: Vec<&str> = comma_rows.iter().map(String::as_str).collect();
    let taught: Vec<Taught> = vec![
        (0, '5', five, Vec::new()),
        (1, '1', wide, Vec::new()),
        (2, '1', drawn_rows(&comma_rows), Vec::new()),
        (3, '4', five, Vec::new()),
    ];
    let (usable, unusable) = split_usable(taught, &known);
    assert_eq!(usable.iter().map(|t| t.0).collect::<Vec<_>>(), vec![0]);
    assert_eq!(unusable, vec![2, 3, 4], "places from 1: the block, the comma and the five taught as a four");
}

#[test]
fn an_image_nearly_like_one_kept_for_the_same_number_is_not_taught_again() {
    let known = shipped();
    let folder = testdir::fresh("signatur-doppelt");
    let files = OwnFiles::in_folder(|file| folder.join(file).to_string_lossy().into_owned());
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    let typed = number.to_string();
    let images = ByVariant::both(Some(crop.clone()));
    let by_variant = templates_by_variant(SHIPPED, &files);
    assert!(learn_variants(&images, &typed, Some(Variant::Green), &by_variant, &files, &samples_folder).is_ok());
    // The same crop with a point or two different: the same picture.
    let mut near = crop.clone();
    near.data[0] = near.data[0].saturating_add(40);
    let again = ByVariant::both(Some(near));
    assert_eq!(
        learn_variants(&again, &typed, Some(Variant::Green), &by_variant, &files, &samples_folder),
        Err("anlernen_schon")
    );
    assert_eq!(samples(&samples_folder).len(), 1);
    // Another number is another picture, even where the crop is the same.
    let other = if typed.ends_with('0') { format!("{}5", &typed[..typed.len() - 1]) } else { format!("{}0", &typed[..typed.len() - 1]) };
    let _ = learn_variants(&images, &other, Some(Variant::Green), &by_variant, &files, &samples_folder);
    assert!(known.learned_digits() > 0);
}

#[test]
fn pictures_kept_for_an_unsure_number_never_go_into_the_archive_of_a_report() {
    let folder = testdir::fresh("signatur-bericht-gemerkt");
    let files = OwnFiles::in_folder(|file| folder.join(file).to_string_lossy().into_owned());
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let refinery_folder = folder.join("raffinerie-bilder").to_string_lossy().into_owned();
    let pending_folder = folder.join(crate::signature_watch::PENDING_FOLDER).to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    let known = shipped();
    learn(&crop, &number.to_string(), &known, files.of(Variant::Green), &samples_folder).expect("taught");
    assert!(crate::signature_watch::keep_pending(&pending_folder, &crate::signature_watch::Frame::new(crop, None, 1)));
    assert_eq!(crate::signature_watch::pending(&pending_folder).len(), 1);
    let bytes = sample_archive("Max", &samples_folder, &files, &refinery_folder).expect("archive");
    let names: Vec<String> = crate::zip::entries(&bytes).unwrap().into_iter().map(|e| e.name).collect();
    assert!(names.iter().any(|n| n.contains("/bilder/")), "{names:?}");
    assert!(names.iter().all(|n| !n.contains("gemerkt") && !n.ends_with(".bin")), "{names:?}");
}

#[test]
fn removing_the_duplicates_leaves_one_image_per_number_and_the_newest() {
    let folder = testdir::fresh("signatur-doppelte-weg");
    let files = OwnFiles::in_folder(|file| folder.join(file).to_string_lossy().into_owned());
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    let typed = number.to_string();
    let mut names = Vec::new();
    // Taught with `learn` as before the check: three images of one number.
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(3));
        let known = templates_by_variant(SHIPPED, &files);
        let images = ByVariant::both(Some(crop.clone()));
        let name = sample_name(&typed);
        names.push(name.clone());
        let tally = learn_into(&crop, &typed, known.get(Variant::Green), files.of(Variant::Green), Some(&name));
        assert!(tally.is_ok());
        assert!(save_sample_as(&samples_folder, &images.get(Variant::Green).clone().unwrap(), &name));
    }
    let (_, removed) = own::forget_duplicates(&files, &samples_folder).expect("removed");
    assert_eq!(removed, 2);
    assert_eq!(samples(&samples_folder), vec![names[2].clone()]);
}
