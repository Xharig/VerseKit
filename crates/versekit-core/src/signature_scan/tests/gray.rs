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

//! The two grey images: which one reads, the templates of each, teaching from both.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

const REAL: &str = include_str!("../../../../../tools/pruefdaten/signatur-windows-klein.json");

/// The real recordings of the small type by their keys.
fn real() -> BTreeMap<String, Gray> {
    images_of(REAL).into_iter().collect()
}

/// Possible values for the real recordings: every number of theirs, plus `extra`.
fn real_values(extra: &[u64]) -> Vec<u64> {
    let mut all: Vec<u64> = real().keys().map(|key| number_of(key)).collect();
    all.extend_from_slice(extra);
    all.sort();
    all.dedup();
    all
}

/// Files that do not exist yet, in a folder of their own.
fn files_in(name: &str) -> OwnFiles {
    let folder = testdir::fresh(name);
    OwnFiles::in_folder(|file| folder.join(file).to_string_lossy().into_owned())
}

/// The folder for the images kept, next to the files.
fn samples_of(files: &OwnFiles) -> String {
    format!("{}/bilder", std::path::Path::new(&files.shared).parent().unwrap().display())
}

fn cut_of(gray: &Gray) -> u8 {
    otsu(&histogram(gray))
}

/// The two grey images of BGRA bytes.
fn grays_of(raw: &[u8], width: usize, height: usize) -> Images {
    ByVariant {
        green: Some(to_gray(raw, width as i32, height as i32, Variant::Green)),
        bright: Some(to_gray(raw, width as i32, height as i32, Variant::Bright)),
    }
}

/// Grey image → BGRA: the digits (above `cut`) white, `reach` points of orange-red outline around them, the ground
/// grey as in the grey image. With `cover` the outline lets the ground shine through by the rest.
fn outlined(gray: &Gray, cut: u8, reach: i32, cover: Option<f64>) -> (Vec<u8>, usize, usize) {
    let (w, h) = (gray.w as i32, gray.h as i32);
    let mut ring = vec![false; gray.w * gray.h];
    for y in 0..h {
        for x in 0..w {
            if gray.at(x, y) <= cut {
                continue;
            }
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (nx, ny) = (x + dx, y + dy);
                    if (0..w).contains(&nx) && (0..h).contains(&ny) && gray.at(nx, ny) <= cut {
                        ring[ny as usize * gray.w + nx as usize] = true;
                    }
                }
            }
        }
    }
    let mut raw = Vec::with_capacity(gray.w * gray.h * 4);
    for (i, &value) in gray.data.iter().enumerate() {
        if !ring[i] {
            raw.extend_from_slice(&[value, value, value, 255]);
            continue;
        }
        match cover {
            None => raw.extend_from_slice(&[0, 0x46, 0xff, 0xff]),
            Some(c) => {
                let rest = f64::from(value) * (1.0 - c);
                raw.extend_from_slice(&[rest as u8, (rest + 70.0 * c) as u8, (rest + 200.0 * c) as u8, 255]);
            }
        }
    }
    (raw, gray.w, gray.h)
}

/// Grey image → BGRA: blue and red carry the value, green stays at the middle grey everywhere: digits that cannot be
/// seen in the green channel.
fn without_green(gray: &Gray) -> (Vec<u8>, usize, usize) {
    let mut ordered = gray.data.clone();
    ordered.sort();
    let ground = ordered[ordered.len() / 2];
    let raw = gray.data.iter().flat_map(|&v| [v, ground, v, 255]).collect();
    (raw, gray.w, gray.h)
}

/// A frame of the game picture: dark ground with the BGRA crop pasted at (300, 110).
fn frame_with(raw: &[u8], w: usize, h: usize) -> (Vec<u8>, i32, i32) {
    let (frame_w, frame_h, left, top) = (640usize, 240usize, 300usize, 110usize);
    let mut frame: Vec<u8> = [0x10u8, 0x10, 0x10, 0xff].repeat(frame_w * frame_h);
    for y in 0..h {
        let at = ((top + y) * frame_w + left) * 4;
        frame[at..at + w * 4].copy_from_slice(&raw[y * w * 4..(y + 1) * w * 4]);
    }
    (frame, frame_w as i32, frame_h as i32)
}

fn read_alone(images: &Images, known: &ByVariant<Templates>, values: &[u64]) -> ByVariant<Reading> {
    known.map(|v, templates| read(images.get(v).as_ref().expect("image"), templates, values))
}

#[test]
fn the_green_grey_image_is_the_green_channel_and_the_bright_one_the_brightest_of_the_three() {
    let point = [10u8, 200, 30, 255, 250, 5, 7, 255, 1, 2, 240, 255];
    assert_eq!(to_gray(&point, 3, 1, Variant::Bright).data, [200, 250, 240]);
    assert_eq!(to_gray(&point, 3, 1, Variant::Green).data, [200, 5, 2]);
    assert_eq!(gray_crop(&point, 3, (1, 0, 2, 1), Variant::Bright).data, [250, 240]);
    assert_eq!(gray_crop(&point, 3, (1, 0, 2, 1), Variant::Green).data, [5, 2]);
    assert_eq!(VARIANTS, [Variant::Green, Variant::Bright]);
    assert_eq!(Variant::from_key("hell"), Some(Variant::Bright));
    assert_eq!(Variant::from_key("gruen"), Some(Variant::Green));
    assert_eq!(Variant::from_key("blau"), None);
}

#[test]
fn a_picture_only_the_green_image_reads_is_read_from_it_also_through_the_search() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-617"));
    let real = real();
    let gray = &real["16,000#s1"];
    let values = real_values(&[16_000]);
    let (raw, w, h) = outlined(gray, cut_of(gray), 1, None);
    let images = grays_of(&raw, w, h);

    let alone = read_alone(&images, &known, &values);
    assert_eq!(alone.green.value, Some(16_000), "the trap: green reads");
    assert_ne!(alone.bright.value, Some(16_000), "the trap: bright does not");

    let found = read_variants(&images, &known, &values);
    assert_eq!(found.reading.value, Some(16_000));
    assert_eq!(found.variant, Some(Variant::Green));
    assert_eq!(found.image, images.green);
    assert_eq!(found.images, images);

    let (frame, frame_w, frame_h) = frame_with(&raw, w, h);
    let seen = search(&frame, frame_w, frame_h, &known, &values, 12);
    assert_eq!((seen.reading.value, seen.variant), (Some(16_000), Some(Variant::Green)));
    assert!(seen.images.green.is_some() && seen.images.bright.is_some());

    let (mut right, mut green_right, mut wrong) = (0, 0, Vec::new());
    for (key, gray) in &real {
        let wanted = number_of(key);
        let values = real_values(&[wanted]);
        let (raw, w, h) = outlined(gray, cut_of(gray), 1, None);
        let images = grays_of(&raw, w, h);
        if read(images.green.as_ref().unwrap(), &known.green, &values).value == Some(wanted) {
            green_right += 1;
        }
        match read_variants(&images, &known, &values).reading.value {
            Some(value) if value == wanted => right += 1,
            Some(value) => wrong.push((key.clone(), value)),
            None => {}
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
    assert!(right >= green_right && right >= 20, "{right} right, green alone {green_right}");
}

#[test]
fn a_picture_only_the_bright_image_reads_is_read_from_it() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-618"));
    let real = real();
    let gray = &real["16,000#s1"];
    let values = real_values(&[16_000]);
    let (raw, w, h) = without_green(gray);
    let images = grays_of(&raw, w, h);

    let alone = read_alone(&images, &known, &values);
    assert_eq!(alone.bright.value, Some(16_000), "the trap: bright reads");
    assert_eq!(alone.green.value, None, "the trap: green does not");

    let found = read_variants(&images, &known, &values);
    assert_eq!((found.reading.value, found.variant), (Some(16_000), Some(Variant::Bright)));
    assert_eq!(found.image, images.bright);

    let (mut right, mut wrong) = (0, Vec::new());
    for (key, gray) in &real {
        let wanted = number_of(key);
        let (raw, w, h) = without_green(gray);
        let images = grays_of(&raw, w, h);
        match read_variants(&images, &known, &real_values(&[wanted])).reading.value {
            Some(value) if value == wanted => right += 1,
            Some(value) => wrong.push((key.clone(), value)),
            None => {}
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
    assert!(right >= 20, "{right}");
}

#[test]
fn the_search_reads_a_pill_the_green_channel_only_marks_from_the_bright_image() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-suche-hell"));
    let real = real();
    let gray = &real["16,000#s1"];
    let values = real_values(&[16_000]);
    // Green is a flat bright band over the middle rows (the pill is found as a nest, nothing can be read in it); blue
    // and red carry the digits.
    let raw: Vec<u8> = gray
        .data
        .iter()
        .enumerate()
        .flat_map(|(i, &v)| [v, if (7..31).contains(&(i / gray.w)) { 170 } else { 12 }, v, 255])
        .collect();
    let (frame, frame_w, frame_h) = frame_with(&raw, gray.w, gray.h);
    let found = search(&frame, frame_w, frame_h, &known, &values, 12);
    assert_eq!((found.reading.value, found.variant), (Some(16_000), Some(Variant::Bright)));
    assert_eq!(found.image, found.images.bright);
    assert!(found.boxr.is_some());
}

#[test]
fn read_equally_well_in_both_the_green_image_wins() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-gleich"));
    let gray = real()["2,000"].clone();
    let images = Images { green: Some(gray.clone()), bright: Some(gray) };
    let found = read_variants(&images, &known, &[2_000, 3_000, 4_000, 6_000, 8_000, 12_000, 21_350]);
    assert_eq!((found.reading.value, found.variant), (Some(2_000), Some(Variant::Green)));
}

#[test]
fn when_both_read_a_value_the_green_one_counts_though_the_bright_one_fits_better() {
    let files = files_in("graubild-genauer");
    let gray = real()["2,000"].clone();
    let values = [2_000, 3_000, 4_000, 6_000, 8_000, 12_000, 21_350];
    let images = Images { green: Some(gray.clone()), bright: Some(gray.clone()) };
    // Only the bright image learns this picture: it reads it more exactly than the green one.
    let before = templates_by_variant(SHIPPED, &files);
    learn(&gray, "2,000", &before.bright, files.of(Variant::Bright), &samples_of(&files)).expect("taught");
    let known = templates_by_variant(SHIPPED, &files);
    let alone = read_alone(&images, &known, &values);
    assert!(alone.bright.distance < alone.green.distance, "the trap: {alone:?}");
    assert!(alone.green.value.is_some() && alone.bright.value.is_some());

    let found = read_variants(&images, &known, &values);
    assert_eq!((found.reading.value, found.variant), (Some(2_000), Some(Variant::Green)));
    assert!(!std::path::Path::new(files.of(Variant::Green)).exists(), "what bright learns does not reach green");
}

#[test]
fn a_missing_grey_image_is_skipped_and_none_at_all_is_silent() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-fehlt"));
    let gray = real()["2,000"].clone();
    let only_bright = Images { green: None, bright: Some(gray.clone()) };
    let found = read_variants(&only_bright, &known, &[2_000, 3_000, 4_000]);
    assert_eq!((found.reading.value, found.variant), (Some(2_000), Some(Variant::Bright)));
    let none = read_variants(&Images::default(), &known, &[2_000]);
    assert_eq!((none.reading.value, none.variant, none.image), (None, None, None));
    assert_eq!(none.reading.reason, Some(Reason::NoText));
}

#[test]
fn when_green_reads_nothing_either_the_front_image_with_digits_counts() {
    let known = templates_by_variant(SHIPPED, &files_in("graubild-still"));
    let gray = real()["2,000"].clone();
    let images = Images { green: Some(gray.clone()), bright: Some(gray) };
    let found = read_variants(&images, &known, &[9_999, 7_777]);
    assert_eq!(found.reading.value, None);
    assert_eq!(found.reading.reason, Some(Reason::Unsure));
    assert_eq!(found.variant, Some(Variant::Green));
}

/// The picture 56,000 made of real digits: the 1 of `16,000#s1` replaced by the 5 of `25,800#s1`.
fn fifty_six_thousand() -> Gray {
    let real = real();
    let mut target = real["16,000#s1"].clone();
    let source = &real["25,800#s1"];
    let first_row = |gray: &Gray| -> Vec<Box4> {
        for threshold in thresholds(gray) {
            if let Some(row) = digit_rows(&components(gray, threshold), gray, threshold, false).into_iter().next() {
                return row;
            }
        }
        Vec::new()
    };
    let one = first_row(&target)[0];
    let five = first_row(source)[1];
    let ground = target.at((one[0] - 3).max(0), one[1]);
    for y in one[1]..=one[3] {
        for x in one[0]..=one[2] {
            target.data[y as usize * target.w + x as usize] = ground;
        }
    }
    let (width, height) = (five[2] - five[0] + 1, five[3] - five[1] + 1);
    for dy in 0..height {
        for dx in 0..width {
            let value = source.at(five[0] + dx, five[1] + dy);
            target.data[(one[3] - height + 1 + dy) as usize * target.w + (one[2] - width + 1 + dx) as usize] = value;
        }
    }
    target
}

#[test]
fn green_counts_before_a_more_exact_bright_and_56000_is_never_read_as_16000() {
    let files = files_in("graubild-620");
    let gray = fifty_six_thousand();
    let values = real_values(&[16_000, 56_000]);
    let plain = templates_by_variant(SHIPPED, &files);
    assert_eq!(read(&gray, &plain.green, &values).value, Some(56_000), "the rebuilt picture shows 56,000");

    let (full_raw, w, h) = outlined(&gray, cut_of(&gray), 2, Some(1.0));
    let full = grays_of(&full_raw, w, h);
    assert_eq!(read_variants(&full, &plain, &values).reading.value, Some(56_000), "full outline");

    // Half cover: the bright image is taught this picture as 16,000, and has no template for the 5, else 16,000 and
    // 56,000 fit it nearly equally well and it stays silent.
    let (raw, w, h) = outlined(&gray, cut_of(&gray), 2, Some(0.5));
    let images = grays_of(&raw, w, h);
    learn(images.bright.as_ref().unwrap(), "16,000", &plain.bright, files.of(Variant::Bright), &samples_of(&files))
        .expect("taught");
    let mut known = templates_by_variant(SHIPPED, &files);
    known.bright.digits.retain(|(digit, _)| *digit != '5');
    let alone = read_alone(&images, &known, &values);
    assert_eq!(
        (alone.green.value, alone.bright.value),
        (Some(56_000), Some(16_000)),
        "the trap: {:?} {:?}",
        alone.green,
        alone.bright
    );
    assert!(alone.bright.distance < alone.green.distance, "the trap: bright fits better");

    let found = read_variants(&images, &known, &values);
    assert_eq!((found.reading.value, found.variant), (Some(56_000), Some(Variant::Green)));

    let (frame, frame_w, frame_h) = frame_with(&raw, w, h);
    let seen = search(&frame, frame_w, frame_h, &known, &values, 12);
    assert_eq!(seen.reading.value, Some(56_000), "through the search of the pill");
}

fn set_written(path: &str, when: SystemTime) {
    std::fs::File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn old_shared_templates_from_the_brightest_channel_hold_good_for_the_bright_image_alone() {
    let files = files_in("graubild-621");
    let gray = fifty_six_thousand();
    let values = real_values(&[16_000, 56_000]);
    let (full_raw, w, h) = outlined(&gray, cut_of(&gray), 2, Some(1.0));
    let full = grays_of(&full_raw, w, h);
    let (half_raw, hw, hh) = outlined(&gray, cut_of(&gray), 2, Some(0.5));
    let half = grays_of(&half_raw, hw, hh);
    let plain = templates_by_variant(SHIPPED, &files);
    assert_eq!(read(full.green.as_ref().unwrap(), &plain.green, &values).value, Some(56_000));

    // The digits of the brightest channel, taught as 16,000, in the shared file.
    learn(half.bright.as_ref().unwrap(), "16,000", &plain.bright, &files.shared, &samples_of(&files)).expect("taught");
    let kept = fs::read(&files.shared).unwrap();
    let mut shared = Templates::default();
    shared.add_file(&files.shared);
    let first_one = shared.digits.iter().find(|(d, _)| *d == '1').unwrap().1[0].pattern;
    let holds = |known: &Templates| {
        known
            .digits
            .iter()
            .find(|(d, _)| *d == '1')
            .is_some_and(|(_, list)| list.iter().any(|e| e.pattern == first_one))
    };

    // Written after the cut-off: it holds good for both, and the trap is set: green no longer reads 56,000.
    set_written(&files.shared, at(GREEN_LEARNED_SINCE + 12 * 86_400));
    assert_eq!(shared_own_variants(&files.shared), ByVariant::both(true));
    let both = templates_by_variant(SHIPPED, &files);
    assert!(holds(&both.green) && holds(&both.bright));
    let green_new = read(full.green.as_ref().unwrap(), &both.green, &values).value;
    assert_ne!(green_new, Some(56_000), "the trap: green is thrown by the old digits");

    // Written before it: bright only.
    set_written(&files.shared, at(1_791_000_000));
    assert_eq!(shared_own_variants(&files.shared), ByVariant { green: false, bright: true });
    let split = templates_by_variant(SHIPPED, &files);
    assert!(holds(&split.bright) && !holds(&split.green));
    let found = read_variants(&full, &split, &values);
    assert_eq!((found.reading.value, found.variant), (Some(56_000), Some(Variant::Green)));

    let (raw, w, h) = outlined(&gray, cut_of(&gray), 2, Some(1.0));
    let (frame, frame_w, frame_h) = frame_with(&raw, w, h);
    assert_eq!(search(&frame, frame_w, frame_h, &split, &values, 12).reading.value, Some(56_000));
    // The templates without grey image (shipped and shared) still hold the shared file.
    let mut without = Templates::default();
    without.add_text(SHIPPED);
    without.add_file(&files.shared);
    assert!(holds(&without));
    assert_eq!(fs::read(&files.shared).unwrap(), kept, "the old file stays as it is");
}

#[test]
fn the_templates_of_a_grey_image_hold_the_digits_of_its_own_file_and_not_those_of_the_other() {
    let files = files_in("graubild-eigene-datei");
    let samples = samples_of(&files);
    let real = real();
    let plain = templates_by_variant(SHIPPED, &files);
    learn(&real["6,740#s1"], "6,740", &plain.bright, files.of(Variant::Bright), &samples).expect("taught");
    let mut taught = Templates::default();
    taught.add_file(files.of(Variant::Bright));
    assert!(!taught.is_empty());

    let known = templates_by_variant(SHIPPED, &files);
    assert_ne!(known.bright.digest(), plain.bright.digest(), "bright holds what it taught");
    assert_eq!(known.green.digest(), plain.green.digest(), "green is untouched by it");
    assert_eq!(templates_for(SHIPPED, &files, Variant::Bright).digest(), known.bright.digest());

    learn(&real["17,080#s1"], "17,080", &plain.green, files.of(Variant::Green), &samples).expect("taught");
    let both = templates_by_variant(SHIPPED, &files);
    assert_ne!(both.green.digest(), plain.green.digest());
    assert_eq!(both.bright.digest(), known.bright.digest(), "bright is untouched by what green taught");
}

#[test]
fn a_shared_file_that_is_not_there_holds_good_for_both() {
    let files = files_in("graubild-ohne-datei");
    assert_eq!(shared_own_variants(&files.shared), ByVariant::both(true));
}

#[test]
fn teaching_from_both_images_stores_each_in_its_file_and_leaves_the_shared_file_alone() {
    let files = files_in("graubild-anlernen");
    let samples = samples_of(&files);
    let real = real();
    // An older shared file of the player.
    let known = templates_by_variant(SHIPPED, &files);
    let early = &real["6,740#s1"];
    learn(early, "6,740", &known.green, &files.shared, &samples).expect("shared file taught");
    let before = fs::read(&files.shared).unwrap();
    let samples_before = samples_len(&samples);

    // White digits on a reddish ground: the two grey images differ.
    let source = &real["17,080#s1"];
    let reddish: Vec<u8> = source.data.iter().flat_map(|&v| [v, v, v.saturating_add(40), 255]).collect();
    let images = grays_of(&reddish, source.w, source.h);
    assert_ne!(images.green, images.bright, "the pictures differ");
    let known = templates_by_variant(SHIPPED, &files);
    let tally = learn_variants(&images, "17,080", Some(Variant::Bright), &known, &files, &samples).expect("taught");
    assert_eq!(tally.found, 5);
    assert_eq!(fs::read(&files.shared).unwrap(), before, "the shared file stays byte for byte");
    for variant in VARIANTS {
        let mut own = Templates::default();
        own.add_file(files.of(variant));
        assert!(own.digits.iter().any(|(d, list)| *d == '8' && !list.is_empty()), "{variant:?}");
    }
    assert_eq!(samples_len(&samples), samples_before + 1, "one image for the number, not one for each grey image");

    // The shared digits keep holding good for both grey images.
    let after = templates_by_variant(SHIPPED, &files);
    assert!(after.green.learned_digits() >= known.green.learned_digits());
    let mut shared = Templates::default();
    shared.add_file(&files.shared);
    for variant in VARIANTS {
        let have = after.get(variant);
        for (digit, list) in &shared.digits {
            let mine = &have.digits.iter().find(|(d, _)| d == digit).expect("digit").1;
            assert!(list.iter().all(|e| mine.iter().any(|m| m.pattern == e.pattern)), "{digit} in {variant:?}");
        }
    }
}

fn samples_len(folder: &str) -> usize {
    samples(folder).len()
}

#[test]
fn teaching_into_one_grey_image_leaves_the_other_one_untouched() {
    let files = files_in("graubild-eine");
    let samples = samples_of(&files);
    let gray = real()["2,000"].clone();
    let known = templates_by_variant(SHIPPED, &files);
    learn(&gray, "2,000", &known.bright, files.of(Variant::Bright), &samples).expect("taught");
    assert!(std::path::Path::new(files.of(Variant::Bright)).exists());
    assert!(!std::path::Path::new(files.of(Variant::Green)).exists());
    assert!(!std::path::Path::new(&files.shared).exists());
}

#[test]
fn teaching_from_the_front_image_that_works_keeps_its_image_and_a_failing_one_does_not_stop_the_other() {
    let files = files_in("graubild-reihenfolge");
    let samples = samples_of(&files);
    let gray = real()["2,000"].clone();
    let blank = Gray::new(gray.w, gray.h, vec![0; gray.w * gray.h]);
    let known = templates_by_variant(SHIPPED, &files);
    let images = Images { green: Some(blank.clone()), bright: Some(gray.clone()) };
    // Green shows nothing to learn from, bright teaches: the call worked and the image kept is the bright one.
    let tally = learn_variants(&images, "2,000", Some(Variant::Green), &known, &files, &samples).expect("taught");
    assert_eq!(tally.found, 4);
    assert!(std::path::Path::new(files.of(Variant::Bright)).exists());
    assert!(!std::path::Path::new(files.of(Variant::Green)).exists());
    let kept = samples_len(&samples);
    assert_eq!(kept, 1);
    let png = fs::read(format!("{samples}/{}", super::super::samples(&samples)[0])).unwrap();
    let bright_png = png_bytes(&gray);
    assert_eq!(png, bright_png, "the image kept is the one that taught");

    // Nothing worked anywhere: the first outcome is the answer and nothing is kept.
    let both_blank = Images { green: Some(blank.clone()), bright: Some(blank) };
    assert_eq!(learn_variants(&both_blank, "2,000", None, &known, &files, &samples), Err("anlernen_anzahl"));
    assert_eq!(samples_len(&samples), kept);
    assert_eq!(learn_variants(&Images::default(), "2,000", None, &known, &files, &samples), Err("anlernen_anzahl"));
}

#[test]
fn a_six_that_runs_shut_at_the_only_threshold_with_a_row_is_read_at_a_higher_one() {
    // A real 6,740: the location symbol capped at 160 so that the row stands at one threshold only, and two points
    // just above it close the upper opening of the 6, which then has two holes like an 8.
    let known = shipped();
    let values = real_values(&[8_740]);
    let original = real()["6,740#s2"].clone();
    let before = read(&original, &known, &values);
    assert_eq!((before.value, before.digits.len()), (Some(6_740), 4), "the recording reads 6,740");
    let left = before.digits[0][0] - 2;
    let mut gray = original.clone();
    for y in 0..gray.h {
        for x in 0..gray.w {
            if (x as i32) < left {
                let i = y * gray.w + x;
                gray.data[i] = gray.data[i].min(160);
            }
        }
    }
    let rows: Vec<u8> = thresholds(&gray)
        .into_iter()
        .filter(|&t| !digit_rows(&components(&gray, t), &gray, t, false).is_empty())
        .collect();
    assert_eq!(rows.len(), 1, "with the capped symbol the row stands at one threshold only: {rows:?}");
    let low = rows[0];
    for y in [15, 16] {
        gray.data[y * gray.w + 54] = low + 10;
    }
    let row = digit_rows(&components(&gray, low), &gray, low, false);
    assert_eq!(row.first().map(Vec::len), Some(4), "the row of four digits at {low}");
    let six = holes(&fill(&normalize(&gray, row[0][0], low)));
    assert!(six.0 == 2 && six.1.is_some_and(|top| top < 0.5), "the 6 has a second hole at the top: {six:?}");

    let read_now = read(&gray, &known, &values);
    assert_eq!(read_now.value, Some(6_740), "read at a higher threshold");
    assert!(read_now.threshold.is_some_and(|t| t > low), "from a threshold above {low}: {:?}", read_now.threshold);
    let without: Vec<u64> = values.iter().copied().filter(|&v| v != 6_740).collect();
    assert_eq!(read(&gray, &known, &without).value, None, "without 6,740 it stays silent, no 8,740");
}
