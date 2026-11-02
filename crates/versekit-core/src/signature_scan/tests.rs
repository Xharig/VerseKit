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

use super::*;
use crate::testdir;

const SHIPPED: &str = SHIPPED_TEMPLATES;
const PICTURES: &str = include_str!("../../tests/data/signatur-pruefbilder.json");

fn shipped() -> Templates {
    let mut known = Templates::default();
    known.add_text(SHIPPED);
    known
}

/// The same templates for both grey images.
fn both(known: &Templates) -> ByVariant<Templates> {
    ByVariant::both(known.clone())
}

/// The possible values the pictures need: the numbers of their keys and some others of the same length.
fn values() -> Vec<u64> {
    let mut values: Vec<u64> = pictures().iter().map(|(n, _)| *n).collect();
    values.extend([4_000, 8_000, 12_680, 16_980, 17_200, 2_010, 3_100, 19_375]);
    values.sort();
    values.dedup();
    values
}

mod gray;

/// The crops of a test file by their keys.
fn images_of(text: &str) -> Vec<(String, Gray)> {
    let file = jsonio::loads(text).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    images
        .iter()
        .map(|(key, raster)| {
            let Value::Array(rows) = raster else { panic!("rows") };
            let h = rows.len();
            let mut data = Vec::new();
            let mut w = 0;
            for row in rows {
                let Value::Array(row) = row else { panic!("row") };
                w = row.len();
                for v in row {
                    let Value::Int(v) = v else { panic!("int") };
                    data.push(v.parse::<u8>().unwrap());
                }
            }
            (key.clone(), Gray::new(w, h, data))
        })
        .collect()
}

/// The number a key of a test file stands for (`16,000#s1` is 16000).
fn number_of(key: &str) -> u64 {
    key.split('#').next().unwrap().chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap()
}

/// Real crops around the scan signature (green channel) with their right number: small and large type.
fn pictures() -> Vec<(u64, Gray)> {
    images_of(PICTURES).into_iter().map(|(key, raster)| (number_of(&key), raster)).collect()
}

/// BGRA bytes of a larger frame: noise ground, the crop in the green channel and blue and red as bright as can be
/// (so that the brightest channel would read it wrongly).
fn frame_of(raster: &Gray) -> (Vec<u8>, i32, i32) {
    let (fw, fh) = (320.max(raster.w + 120), 160.max(raster.h + 80));
    let mut raw = vec![0u8; fw * fh * 4];
    for y in 0..fh {
        for x in 0..fw {
            let g = (x * 31 + y * 17) % 37;
            let i = (y * fw + x) * 4;
            raw[i..i + 4].copy_from_slice(&[((g * 5 + 90) % 256) as u8, g as u8, ((g * 3 + 120) % 256) as u8, 255]);
        }
    }
    for ry in 0..raster.h {
        for rx in 0..raster.w {
            let v = raster.data[ry * raster.w + rx];
            let i = ((40 + ry) * fw + 60 + rx) * 4;
            raw[i] = v.saturating_add(60);
            raw[i + 1] = v;
            raw[i + 2] = 255;
        }
    }
    (raw, fw as i32, fh as i32)
}

#[test]
fn the_shipped_templates_cover_all_ten_digits() {
    let known = shipped();
    assert_eq!(known.learned_digits(), 10);
    assert!(!known.is_empty());
}

#[test]
fn the_grey_image_is_the_green_channel_and_the_outline_keeps_the_hole_of_a_zero_open() {
    // A zero of a white stroke with an orange-red outline two points wide, framed by black.
    let (frame_w, frame_h, left, top) = (20usize, 17usize, 4usize, 2usize);
    let ring: Vec<(usize, usize)> = (2..7usize)
        .flat_map(|x| (2..11usize).map(move |y| (x, y)))
        .filter(|&(x, y)| x == 2 || x == 6 || y == 2 || y == 10)
        .collect();
    let mut raw = vec![0u8; frame_w * frame_h * 4];
    let mut set = |x: usize, y: usize, bgra: [u8; 4]| {
        let at = ((top + y) * frame_w + left + x) * 4;
        raw[at..at + 4].copy_from_slice(&bgra);
    };
    for &(rx, ry) in &ring {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                set((rx as i32 + dx) as usize, (ry as i32 + dy) as usize, [0, 70, 255, 255]);
            }
        }
    }
    for &(x, y) in &ring {
        set(x, y, [255, 255, 255, 255]);
    }
    let holes_in = |g: &Gray| -> Option<usize> {
        let bright: Vec<(i32, i32)> = (0..g.h as i32)
            .flat_map(|y| (0..g.w as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| g.at(x, y) > 128)
            .collect();
        let min = |f: fn(&(i32, i32)) -> i32| bright.iter().map(f).min();
        let max = |f: fn(&(i32, i32)) -> i32| bright.iter().map(f).max();
        let boxr = [min(|p| p.0)?, min(|p| p.1)?, max(|p| p.0)?, max(|p| p.1)?];
        Some(holes(&normalize(g, boxr, 128)).0)
    };
    // The trap: in the brightest channel the zero has no opening.
    let brightest: Vec<u8> = raw.chunks(4).map(|p| p[0].max(p[1]).max(p[2])).collect();
    assert_eq!(holes_in(&Gray::new(frame_w, frame_h, brightest)), Some(0));

    let whole = to_gray(&raw, frame_w as i32, frame_h as i32, Variant::Green);
    assert_eq!(holes_in(&whole), Some(1));
    let crop = gray_crop(&raw, frame_w as i32, (left as i32, top as i32, 9, 13), Variant::Green);
    assert_eq!(holes_in(&crop), Some(1));
    for y in 0..13usize {
        for x in 0..9usize {
            assert_eq!(crop.data[y * 9 + x], whole.data[(top + y) * frame_w + left + x]);
        }
    }
    assert_eq!(whole.at((left + 2) as i32, (top + 2) as i32), 255);
    assert!(whole.at((left + 4) as i32, (top + 5) as i32) < 128);
    assert_eq!(whole.at(0, 0), 0);
    assert_eq!((whole.w, whole.h, crop.w, crop.h), (frame_w, frame_h, 9, 13));
}

#[test]
fn otsu_splits_two_levels_and_gives_128_for_nothing() {
    let mut counts = [0u64; 256];
    assert_eq!(otsu(&counts), 128);
    counts[20] = 900;
    counts[200] = 100;
    assert!((20..200).contains(&(otsu(&counts) as i32)));
}

#[test]
fn thresholds_stay_between_8_and_245_and_are_sorted_without_repeats() {
    let mut data = vec![30u8; 40 * 20];
    for v in data.iter_mut().take(60) {
        *v = 250;
    }
    for v in data.iter_mut().skip(60).take(80) {
        *v = 180;
    }
    let levels = thresholds(&Gray::new(40, 20, data));
    assert!(levels.windows(2).all(|p| p[0] < p[1]));
    assert!(levels.iter().all(|&v| (8..=245).contains(&v)));
    assert!(levels.len() >= 3);
}

#[test]
fn components_join_over_the_corner_and_drop_single_points() {
    // A diagonal stroke is one area; a single bright point and a flat dash are noise.
    let mut g = Gray::new(20, 12, vec![0; 240]);
    let mut set = |x: usize, y: usize| g.data[y * 20 + x] = 255;
    for i in 0..5 {
        set(2 + i, 1 + i);
    }
    set(3, 1);
    set(10, 3);
    for x in 14..19 {
        set(x, 8);
    }
    let boxes = components(&g, 100);
    assert_eq!(boxes, vec![[2, 1, 6, 5]]);
}

#[test]
fn a_fraction_of_half_rounds_to_the_even_part_count_like_python() {
    // Widths 25 and single 10: 2.5 parts is 2 (round half to even), not 3.
    let g = Gray::new(60, 14, vec![255; 60 * 14]);
    let boxes: Vec<Box4> = vec![[0, 0, 9, 13], [12, 0, 36, 13], [40, 0, 49, 13]];
    let cut = split_merged(&boxes, Some((&g, 100)));
    assert_eq!(cut.len(), 4);
    assert_eq!(cut.iter().filter(|b| b[0] >= 12 && b[2] <= 36).count(), 2);
}

#[test]
fn fill_stretches_a_pattern_over_the_grid_and_leaves_an_empty_one() {
    let empty = [0u8; CELLS];
    assert_eq!(fill(&empty), empty);
    let mut small = [0u8; CELLS];
    for y in 5..9 {
        for x in 6..9 {
            small[y * NORM_W + x] = 1;
        }
    }
    assert!(fill(&small).iter().all(|&v| v == 1));
}

#[test]
fn the_holes_of_digits_come_out_as_the_rules_for_teaching_say() {
    let known = shipped();
    for digit in "0123456789".chars() {
        let examples = &known.digits.iter().find(|(d, _)| *d == digit).unwrap().1;
        let fitting = examples.iter().filter(|e| plausible_template(digit, &e.pattern)).count();
        assert!(fitting * 10 >= examples.len() * 8, "digit {digit}: {fitting} of {}", examples.len());
    }
    // A zero is no six: the hole lies in the middle, not low.
    let zero = &known.digits.iter().find(|(d, _)| *d == '0').unwrap().1[0].pattern;
    assert!(plausible_template('0', zero));
    assert!(!plausible_template('6', zero));
    assert!(!plausible_template('x', zero));
}

/// A digit drawn over the whole grid: a frame three points wide, plus what `more` inks.
fn drawn_digit(more: impl Fn(usize, usize) -> bool, open: impl Fn(usize, usize) -> bool) -> Pattern {
    let mut pattern = [0u8; CELLS];
    for y in 0..NORM_H {
        for x in 0..NORM_W {
            let frame = !(3..NORM_W - 3).contains(&x) || !(3..NORM_H - 3).contains(&y);
            pattern[y * NORM_W + x] = u8::from((frame || more(x, y)) && !open(x, y));
        }
    }
    pattern
}

#[test]
fn a_slashed_zero_counts_as_a_zero_and_an_eight_or_a_six_does_not() {
    let never = |_: usize, _: usize| false;
    // The slash runs from the lower left to the upper right and leaves a hole above it and one below.
    let slashed = drawn_digit(|x, y| (x as f64 - (3.0 + (20 - y as i32) as f64 * 9.0 / 17.0)).abs() <= 1.5, never);
    let eight = drawn_digit(|_, y| (11..13).contains(&y), never);
    // The six: open on the right above its middle bar.
    let six = drawn_digit(|_, y| (11..13).contains(&y), |x, y| x >= NORM_W - 3 && (3..11).contains(&y));

    assert_eq!(holes(&slashed).0, 2);
    assert!(slashed_zero(&slashed));
    assert!(plausible_template('0', &slashed));
    assert!(!plausible_template('8', &slashed));
    assert!(!plausible_template('6', &slashed));

    assert_eq!(holes(&eight).0, 2);
    assert!(!slashed_zero(&eight));
    assert!(plausible_template('8', &eight));
    assert!(!plausible_template('0', &eight));

    assert_eq!(holes(&six).0, 1);
    assert!(!slashed_zero(&six));
    assert!(plausible_template('6', &six));
    assert!(!plausible_template('0', &six));
}

#[test]
fn the_hud_zero_with_its_slash_is_read_with_the_shipped_templates() {
    let known = shipped();
    let values = [10_665, 10_865, 18_665, 10_605, 7_110, 7_118, 7_140, 25_530, 25_538, 26_530, 25_580];
    let crops = images_of(include_str!("../../tests/data/signatur-nullen.json"));
    assert_eq!(crops.len(), 3);
    for (key, raster) in crops {
        assert_eq!(read(&raster, &known, &values).value, Some(number_of(&key)), "{key}");
    }
}

#[test]
fn a_six_shut_like_an_eight_is_not_read_as_an_eight() {
    // The 6 of this crop has two holes one above the other; by its points it looks more like a 6 than like an 8.
    let known = shipped();
    let crops = images_of(include_str!("../../../../tools/pruefdaten/signatur-windows-klein.json"));
    let raster = &crops.iter().find(|(key, _)| key == "16,960#s4").unwrap().1;
    assert_ne!(read(raster, &known, &[16_960, 16_980]).value, Some(16_980));
}

#[test]
fn a_small_closed_area_under_the_limit_is_no_hole() {
    // A block of 8 by 10 points with a hole of 2 points: below `MIN_HOLE` points, so no hole.
    let mut pattern = [0u8; CELLS];
    for y in 4..14 {
        for x in 4..12 {
            pattern[y * NORM_W + x] = 1;
        }
    }
    pattern[8 * NORM_W + 7] = 0;
    pattern[8 * NORM_W + 8] = 0;
    assert_eq!(holes(&pattern), (0, None));
}

#[test]
fn real_crops_of_both_type_sizes_are_read_with_their_right_number() {
    let known = shipped();
    let values = values();
    let all = pictures();
    assert!(all.len() >= 8);
    for (number, crop) in &all {
        let reading = read(crop, &known, &values);
        assert_eq!(reading.value, Some(*number), "{reading:?}");
        assert_eq!(reading.reason, None);
        assert!(reading.distance < 0.34 && reading.threshold.is_some() && reading.digits.len() >= 3);
    }
}

#[test]
fn the_search_finds_the_crop_again_inside_a_larger_frame() {
    let known = shipped();
    let values = values();
    // The search takes nests up to 40 points high and 320 wide: a crop of large type is not found in a frame;
    // what is found is read right.
    let mut found_again = 0;
    for (number, crop) in pictures() {
        let (raw, width, height) = frame_of(&crop);
        let found = search(&raw, width, height, &both(&known), &values, 12);
        if found.reading.value.is_none() {
            continue;
        }
        assert_eq!(found.reading.value, Some(number), "{:?}", found.reading);
        let (left, top, w, h) = found.boxr.expect("box");
        assert!(left <= 60 + crop.w as i32 && left + w >= 60 && top <= 40 + crop.h as i32 && top + h >= 40);
        assert!(found.image.is_some());
        found_again += 1;
    }
    assert!(found_again >= 4, "{found_again}");
}

#[test]
fn without_templates_the_reason_is_not_taught_and_without_values_no_values() {
    let (number, crop) = pictures().remove(0);
    let (raw, width, height) = frame_of(&crop);
    let found = search(&raw, width, height, &ByVariant::default(), &[number], 12);
    assert_eq!(found.reading.value, None);
    assert_eq!(found.reading.reason, Some(Reason::NotTaught));
    assert!(!found.reading.digits.is_empty());
    let found = search(&raw, width, height, &both(&shipped()), &[], 12);
    assert_eq!(found.reading.reason, Some(Reason::NoValues));
}

#[test]
fn a_number_that_matches_no_possible_value_stays_silent_as_unsure() {
    // 2,000 read against values that are all far from it in every digit.
    let (_, crop) = pictures().remove(0);
    let reading = read(&crop, &shipped(), &[9_999, 7_777, 5_555]);
    assert_eq!(reading.value, None);
    assert_eq!(reading.reason, Some(Reason::Unsure));
}

#[test]
fn an_empty_crop_and_a_blank_frame_say_no_text() {
    let known = shipped();
    assert_eq!(read(&Gray::default(), &known, &[1000]).reason, Some(Reason::NoText));
    let blank = Gray::new(60, 30, vec![40; 1800]);
    let reading = read(&blank, &known, &[1000]);
    assert_eq!((reading.value, reading.reason), (None, Some(Reason::NoText)));
}

#[test]
fn only_the_pill_nest_is_a_candidate_not_an_empty_frame() {
    let (_, crop) = pictures().remove(0);
    let (raw, width, height) = frame_of(&crop);
    assert!(!pill_candidates(&raw, width, height).is_empty());
    assert!(pill_candidates(&vec![0u8; 100 * 100 * 4], 100, 100).is_empty());
    assert!(pill_candidates(&[], 0, 0).is_empty());
}

#[test]
fn possible_values_are_every_signature_times_every_chunk_count_and_the_base_signatures() {
    let data = jsonio::loads(
        r#"{"format": 1, "elemente": {"g1": {"name": "Quantainium", "scanSignature": 3170, "rarity": "legendary"}}}"#,
    )
    .unwrap();
    let mining = MiningData::of(&data);
    let values = possible_values(&mining);
    assert!(values.contains(&3170) && values.contains(&6340) && !values.contains(&9510));
    assert!(values.contains(&4000) && values.contains(&28_000) && values.contains(&30_000));
    assert!(values.windows(2).all(|p| p[0] < p[1]));
}

/// The distances of one character: the digits named, every other digit far away.
fn column(near: &[(char, f64)]) -> Column {
    ('0'..='9').map(|d| (d, near.iter().find(|(c, _)| *c == d).map_or(0.9, |(_, x)| *x))).collect()
}

/// Four characters `1`, `7`, `0` sure and a last one of the distances given.
fn table_of_1_7_0(last: &[(char, f64)]) -> Vec<Column> {
    vec![column(&[('1', 0.02)]), column(&[('7', 0.02)]), column(&[('0', 0.02)]), column(last)]
}

#[test]
fn a_free_reading_that_does_not_end_in_0_or_5_gets_the_nearer_of_the_two_by_the_distances() {
    // The last character is a sure 3, the table says 0 is nearer (numerically 5 would be).
    let table = table_of_1_7_0(&[('3', 0.03), ('0', 0.10), ('5', 0.20)]);
    assert_eq!(free_reading(&table), Some(1703));
    assert_eq!(match_table(&table, &[1700]).0, Some(1700));
    // A sure 2 with 5 nearer by the table (numerically 0 would be): the sister is 1705, the value that fits.
    let table = table_of_1_7_0(&[('2', 0.03), ('5', 0.10), ('0', 0.20)]);
    assert_eq!(free_reading(&table), Some(1702));
    assert_eq!(match_table(&table, &[1700, 1705]).0, Some(1705));
}

#[test]
fn a_free_reading_that_ends_in_0_or_5_still_beats_the_snapped_value() {
    // `1,700` is no possible value, every digit is sure: it is shown, not snapped to `7,200`.
    let table = vec![
        column(&[('1', 0.02), ('7', 0.2)]),
        column(&[('7', 0.02), ('2', 0.2)]),
        column(&[('0', 0.02)]),
        column(&[('0', 0.02)]),
    ];
    assert_eq!(match_table(&table, &[7200]).0, Some(1700));
}

#[test]
fn the_ending_rule_is_off_when_a_possible_value_does_not_end_in_0_or_5() {
    let table = table_of_1_7_0(&[('3', 0.03), ('0', 0.10), ('2', 0.20)]);
    assert_eq!(match_table(&table, &[1700, 1702]).0, Some(1703));
    assert_eq!(match_table(&table, &[1700, 1705]).0, Some(1700));
}

#[test]
fn teaching_stores_the_digits_and_the_image_and_says_what_was_new() {
    let known = shipped();
    let folder = testdir::fresh("signatur-anlernen");
    let own = folder.join("eigene.json").to_string_lossy().into_owned();
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    let typed = number.to_string();

    assert_eq!(learn(&crop, "1", &known, &own, &samples_folder), Err("anlernen_leer"));
    assert_eq!(learn(&crop, "12345", &known, &own, &samples_folder), Err("anlernen_anzahl"));
    let tally = learn(&crop, &typed, &known, &own, &samples_folder).expect("taught");
    assert_eq!(tally.found, typed.len());
    assert_eq!(tally.new + tally.known + tally.unclear.len(), typed.len());
    assert!(tally.unclear.is_empty(), "{tally:?}");

    let taught = templates(SHIPPED, &own);
    assert!(taught.learned_digits() >= 10);
    let names = samples(&samples_folder);
    assert_eq!(names.len(), 1);
    assert!(names[0].starts_with(&format!("{typed}_")) && names[0].ends_with(".png"));
    let png = fs::read(format!("{samples_folder}/{}", names[0])).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    // A wrong typed number: a 6 typed over the first digit is not stored.
    let wrong = format!("6{}", &typed[1..]);
    let tally = learn(&crop, &wrong, &known, &own, &samples_folder).expect("taught");
    assert_eq!(tally.unclear, vec![1]);
}

#[test]
fn a_file_of_taught_digits_that_cannot_be_read_is_kept_and_nothing_is_taught() {
    let known = shipped();
    let folder = testdir::fresh("signatur-unlesbar");
    let own = folder.join("eigene.json").to_string_lossy().into_owned();
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    let typed = number.to_string();
    // Broken JSON, a JSON that is no object, and a folder in the place of the file.
    for (name, content) in [("kaputt", Some("{kaputt")), ("liste", Some("[1, 2]")), ("ordner", None)] {
        let _ = fs::remove_dir_all(&own);
        let _ = fs::remove_file(&own);
        match content {
            Some(text) => fs::write(&own, text).unwrap(),
            None => fs::create_dir_all(&own).unwrap(),
        }
        assert_eq!(learn(&crop, &typed, &known, &own, &samples_folder), Err("anlernen_lesen"), "{name}");
        match content {
            Some(text) => assert_eq!(fs::read_to_string(&own).unwrap(), text, "{name}: the file stays"),
            None => assert!(std::path::Path::new(&own).is_dir(), "{name}"),
        }
        assert!(samples(&samples_folder).is_empty(), "{name}: no sample image of a teaching that did not happen");
    }
    // A file that is not there starts empty, as before.
    let _ = fs::remove_dir_all(&own);
    assert!(learn(&crop, &typed, &known, &own, &samples_folder).is_ok());
}

#[test]
fn the_same_image_taught_again_and_again_adds_each_pattern_once() {
    let known = shipped();
    let folder = testdir::fresh("signatur-grenze");
    let own = folder.join("eigene.json").to_string_lossy().into_owned();
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let (number, crop) = pictures().remove(0);
    for _ in 0..30 {
        learn(&crop, &number.to_string(), &known, &own, &samples_folder).expect("taught");
    }
    let file = jsonio::read_json(&own).unwrap();
    let Some(Value::Object(digits)) = file.get("ziffern") else { panic!("ziffern") };
    let typed = number.to_string();
    for (digit, list) in digits {
        let Value::Array(list) = list else { panic!("list") };
        let times = typed.matches(digit.as_str()).count();
        assert!(!list.is_empty() && list.len() <= times, "digit {digit}: {} patterns for {times} places", list.len());
    }
}

#[test]
fn the_archive_holds_the_images_and_the_own_templates_under_the_name_of_the_reporter() {
    let known = shipped();
    let folder = testdir::fresh("signatur-archiv");
    let files = OwnFiles::in_folder(|name| folder.join(name).to_string_lossy().into_owned());
    let samples_folder = folder.join("bilder").to_string_lossy().into_owned();
    let refinery_folder = folder.join("raffinerie-bilder").to_string_lossy().into_owned();
    assert!(sample_archive("Max", &samples_folder, &files, &refinery_folder).is_none());
    // A kept picture of the refinery scanner alone is enough for an archive.
    std::fs::create_dir_all(&refinery_folder).unwrap();
    std::fs::write(format!("{refinery_folder}/raffinerie_20261030-010203-004_tabelle.png"), b"\x89PNG").unwrap();
    let only = sample_archive("", &samples_folder, &files, &refinery_folder).expect("archive of the refinery");
    let names: Vec<String> = crate::zip::entries(&only).unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["raffinerie/raffinerie_20261030-010203-004_tabelle.png"]);
    let (number, crop) = pictures().remove(0);
    for own in [&files.shared, &files.by_variant.green, &files.by_variant.bright] {
        learn(&crop, &number.to_string(), &known, own, &samples_folder).expect("taught");
    }
    let bytes = sample_archive("Max/Mü:ller ", &samples_folder, &files, &refinery_folder).expect("archive");
    let entries = crate::zip::entries(&bytes).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.iter().any(|n| n.starts_with(&format!("Max_Mü_ller/bilder/{number}_"))), "{names:?}");
    assert!(names.contains(&"Max_Mü_ller/raffinerie/raffinerie_20261030-010203-004_tabelle.png"), "{names:?}");
    for name in [OWN_TEMPLATE_FILE, OWN_GREEN_FILE, OWN_BRIGHT_FILE] {
        assert!(names.contains(&format!("Max_Mü_ller/{name}").as_str()), "{name} in {names:?}");
    }
    assert_eq!(archive_name("Max/Mü:ller "), "scan-bilder-Max_Mü_ller.zip");
    assert_eq!(archive_name("  "), "scan-bilder.zip");
    assert_eq!(archive_name("..."), "scan-bilder.zip");
}

/// The crop of a distance (23.4Gm) as the teaching window showed it: not a signature.
fn distance() -> Gray {
    let file = jsonio::loads(include_str!("../../tests/data/signatur-entfernung.json")).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    let Value::Array(rows) = &images[0].1 else { panic!("rows") };
    let (mut data, mut w) = (Vec::new(), 0);
    for row in rows {
        let Value::Array(row) = row else { panic!("row") };
        w = row.len();
        data.extend(row.iter().map(|v| if let Value::Int(v) = v { v.parse::<u8>().unwrap() } else { 0 }));
    }
    Gray::new(w, rows.len(), data)
}

#[test]
fn a_number_with_a_unit_is_no_signature_and_gives_no_row_of_digits_to_teach_from() {
    let known = shipped();
    // Every value the digits of such a picture could snap to is there; none may be read.
    let values: Vec<u64> = (1000..30000).step_by(5).collect();
    let crop = distance();
    let reading = read(&crop, &known, &values);
    assert_eq!(reading.value, None, "{reading:?}");
    assert!(reading.digits.is_empty(), "a distance offers no digits for teaching: {reading:?}");
    // The same in a larger frame through the search of the pill.
    let (raw, width, height) = frame_of(&crop);
    let found = search(&raw, width, height, &both(&known), &values, 12);
    assert_eq!(found.reading.value, None);
    assert!(found.reading.digits.is_empty(), "{:?} {:?}", found.reading, found.boxr);
}

/// A made-up picture: dark ground, bright filled boxes (left, top, right, bottom).
fn drawn(w: usize, h: usize, boxes: &[Box4]) -> Gray {
    let mut data = vec![20u8; w * h];
    for b in boxes {
        for y in b[1]..=b[3] {
            for x in b[0]..=b[2] {
                data[y as usize * w + x as usize] = 230;
            }
        }
    }
    Gray::new(w, h, data)
}

fn reading_of_digits(digits: Vec<Box4>) -> Reading {
    Reading { value: None, reason: Some(Reason::Unsure), distance: 1.0, digits, threshold: Some(128) }
}

#[test]
fn every_real_signature_crop_with_digits_carries_the_location_symbol() {
    let (known, values) = (shipped(), values());
    let mut checked = 0;
    for (number, crop) in pictures() {
        let reading = read(&crop, &known, &values);
        if reading.digits.is_empty() {
            continue;
        }
        checked += 1;
        assert!(has_locator(&crop, &reading), "{number}: {reading:?}");
    }
    assert!(checked >= 5, "{checked} crops with digits");
}

#[test]
fn a_real_crop_without_its_location_symbol_is_no_signature_box() {
    let (known, values) = (shipped(), values());
    let (_, crop) = pictures().into_iter().find(|(n, _)| *n == 14_160).expect("crop");
    let reading = read(&crop, &known, &values);
    let first = reading.digits.iter().map(|d| d[0]).min().expect("digits") as usize;
    assert!(has_locator(&crop, &reading));
    let mut bare = crop.clone();
    for y in 0..bare.h {
        for x in 0..first.saturating_sub(1) {
            bare.data[y * bare.w + x] = 28;
        }
    }
    assert!(!has_locator(&bare, &reading));
}

#[test]
fn words_of_letters_and_a_label_beside_a_row_are_no_signature_box() {
    let digits: Vec<Box4> = (0..5).map(|i| [60 + i * 8, 12, 64 + i * 8, 21]).collect();
    let reading = reading_of_digits(digits.clone());
    let mut boxes = digits.clone();
    // Three letters of the height of a digit, a gap, then the row: a word before the row.
    boxes.extend([[6, 12, 13, 21], [16, 12, 23, 21], [26, 12, 33, 21]]);
    assert!(!has_locator(&drawn(120, 34, &boxes), &reading), "letters");
    // A single letter does not make a symbol either.
    let mut one = digits.clone();
    one.push([46, 12, 53, 21]);
    assert!(!has_locator(&drawn(120, 34, &one), &reading), "one letter");
    // Nothing left of the row at all.
    assert!(!has_locator(&drawn(120, 34, &digits), &reading), "nothing");
    // A figure taller than a digit close to the row is the symbol.
    let mut symbol = digits;
    symbol.push([40, 8, 53, 24]);
    assert!(has_locator(&drawn(120, 34, &symbol), &reading), "symbol");
    // The same figure far from the row is not part of the box.
    let mut far = symbol.clone();
    far[5] = [2, 8, 15, 24];
    assert!(!has_locator(&drawn(120, 34, &far), &reading), "far away");
}

#[test]
fn a_row_of_digits_led_by_a_bar_instead_of_the_symbol_is_no_signature_box() {
    let (known, values) = (shipped(), values());
    let (_, mut crop) = pictures().into_iter().find(|(n, _)| *n == 14_160).expect("crop");
    for y in 0..crop.h {
        for x in 0..45 {
            crop.data[y * crop.w + x] = if (39..42).contains(&x) && (9..22).contains(&y) { 255 } else { 28 };
        }
    }
    // The row is found (a bar is not of the height of a digit), but the box has no location symbol.
    let reading = read(&crop, &known, &values);
    assert!(!reading.digits.is_empty(), "{reading:?}");
    assert!(!has_locator(&crop, &reading), "{reading:?}");
}

#[test]
fn a_reading_without_digits_or_threshold_has_no_symbol() {
    let raster = drawn(40, 20, &[[5, 5, 20, 15]]);
    assert!(!has_locator(&raster, &Reading { digits: Vec::new(), ..reading_of_digits(Vec::new()) }));
    assert!(!has_locator(&raster, &Reading { threshold: None, ..reading_of_digits(vec![[25, 5, 30, 14]]) }));
}

/// Every crop of the test sets with the number its key names: (set, key, number, crop).
fn keyed_crops() -> Vec<(&'static str, String, u64, Gray)> {
    let sets: [(&str, &str); 5] = [
        ("pruefbilder", PICTURES),
        ("nullen", include_str!("../../tests/data/signatur-nullen.json")),
        ("klein", include_str!("../../../../tools/pruefdaten/signatur-windows-klein.json")),
        ("gross", include_str!("../../../../tools/pruefdaten/signatur-windows-gross.json")),
        ("eingesandt", include_str!("../../../../tools/pruefdaten/signatur-eingesandt.json")),
    ];
    let mut all = Vec::new();
    for (name, text) in sets {
        for (key, raster) in images_of(text) {
            all.push((name, key.clone(), number_of(&key), raster));
        }
    }
    all
}

#[test]
fn no_crop_of_the_test_sets_is_read_wrong_with_the_numbers_of_all_keys_as_possible_values() {
    let known = shipped();
    let all = keyed_crops();
    assert_eq!(all.len(), 106);
    let mut values: Vec<u64> = all.iter().map(|c| c.2).collect();
    values.extend([4_000, 8_000, 12_680, 16_980, 17_200, 2_010, 3_100, 19_375]);
    values.sort();
    values.dedup();
    assert!(values.iter().all(|v| v.is_multiple_of(5)), "the ending rule is on");
    let (mut right, mut wrong) = (0, Vec::new());
    for (set, key, number, crop) in &all {
        match read(crop, &known, &values).value {
            Some(value) if value == *number => right += 1,
            Some(value) => wrong.push(format!("{set}/{key} read {value}")),
            None => {}
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
    assert!(right >= 91, "{right}");
}
