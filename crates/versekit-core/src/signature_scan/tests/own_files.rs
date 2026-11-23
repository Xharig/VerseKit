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

//! The files of the digits taught: which image each pattern came from, and forgetting an image, a pattern or all.

use super::own::{self, FileError, Kind, OwnFile};
use super::*;

/// Files that do not exist yet and the folder of the images kept, in a folder of their own.
fn place(name: &str) -> (OwnFiles, String) {
    let folder = testdir::fresh(name);
    let files = OwnFiles::in_folder(|file| folder.join(file).to_string_lossy().into_owned());
    (files, folder.join("bilder").to_string_lossy().into_owned())
}

/// A real crop of the test pictures by its key.
fn crop(key: &str) -> Gray {
    images_of(PICTURES).into_iter().find(|(k, _)| k == key).map(|(_, g)| g).expect(key)
}

/// The same crop as both grey images.
fn both_images(gray: &Gray) -> Images {
    ByVariant::both(Some(gray.clone()))
}

/// All patterns (taught and derived) of a file.
fn count(path: &str) -> usize {
    OwnFile::open(path).expect("file").len()
}

/// Teaches `key` as `typed` from both grey images; gives the name of the image kept.
fn teach(files: &OwnFiles, samples: &str, key: &str, typed: &str) -> String {
    // The names of the images differ by the millisecond.
    std::thread::sleep(std::time::Duration::from_millis(3));
    let before = samples_in(samples);
    let known = templates_by_variant(SHIPPED, files);
    learn_variants(&both_images(&crop(key)), typed, Some(Variant::Green), &known, files, samples).expect(key);
    let after = samples_in(samples);
    after.into_iter().find(|n| !before.contains(n)).expect("a new image")
}

fn samples_in(folder: &str) -> Vec<String> {
    super::super::samples(folder)
}

#[test]
fn a_kept_image_reads_back_as_the_grey_image_it_was() {
    for (_, gray) in images_of(PICTURES).into_iter().take(6) {
        assert_eq!(own::gray_of_png(&png_bytes(&gray)), Some(gray));
    }
    // The images of another program: every filter of the format.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/signatur-golem-3440");
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let picture = image::open(entry.path()).unwrap().to_luma8();
        let gray = own::gray_of_png(&std::fs::read(entry.path()).unwrap()).expect("read");
        assert_eq!((gray.w as u32, gray.h as u32), picture.dimensions());
        assert_eq!(gray.data, picture.into_raw(), "{:?}", entry.file_name());
    }
    assert_eq!(own::gray_of_png(b"not a picture"), None);
    assert_eq!(own::gray_of_png(&png_bytes(&Gray::new(0, 0, Vec::new()))), None);
}

#[test]
fn every_pattern_taught_names_its_image_and_forgetting_the_image_takes_only_what_it_alone_gave() {
    let (files, samples) = place("eigene-herkunft");
    let first = teach(&files, &samples, "2,000", "2,000");
    let green = files.of(Variant::Green).to_string();
    let after_first = count(&green);
    assert!(after_first > 0);
    let file = OwnFile::open(&green).unwrap();
    assert!(file.taught().iter().all(|(_, _, names)| names == &vec![first.clone()]), "{:?}", file.taught());

    let second = teach(&files, &samples, "14,160", "14,160");
    assert_ne!(first, second);
    let both = count(&green);
    assert!(both > after_first);

    let gone = own::forget_sample(&files, &samples, &first).expect("forgotten");
    assert!(gone > 0);
    assert!(!samples_in(&samples).contains(&first), "the image is gone");
    assert!(samples_in(&samples).contains(&second), "the other image stays");
    let left = OwnFile::open(&green).unwrap();
    assert!(left.taught().iter().all(|(_, _, names)| !names.contains(&first)));
    assert!(!left.is_empty(), "what the second image gave stays");
    // The second image still reads with what it taught.
    let mut known = shipped();
    known.add_file(&green);
    assert_eq!(read(&crop("14,160"), &known, &[14_160, 2_000]).value, Some(14_160));

    own::forget_sample(&files, &samples, &second).expect("forgotten");
    assert_eq!(count(&green), 0, "nothing is left that the images gave");
}

#[test]
fn a_pattern_two_images_gave_stays_until_both_are_forgotten() {
    let (files, samples) = place("eigene-zweimal");
    let first = teach(&files, &samples, "2,000", "2,000");
    let green = files.of(Variant::Green).to_string();
    let once = OwnFile::open(&green).unwrap();
    // The same picture taught again is refused now; a second image with the same patterns is made by hand.
    let second = {
        std::thread::sleep(std::time::Duration::from_millis(3));
        let name = sample_name("2,000");
        for variant in VARIANTS {
            let known = templates_by_variant(SHIPPED, &files);
            learn_into(&crop("2,000"), "2,000", known.get(variant), files.of(variant), Some(&name)).unwrap();
        }
        assert!(save_sample_as(&samples, &crop("2,000"), &name));
        name
    };
    let twice = OwnFile::open(&green).unwrap();
    assert_eq!(once.len(), twice.len(), "the same image adds no pattern");
    assert!(twice.taught().iter().all(|(_, _, names)| names.len() == 2));
    own::forget_sample(&files, &samples, &first).unwrap();
    assert_eq!(count(&green), once.len(), "the second image still names them");
    assert!(OwnFile::open(&green).unwrap().taught().iter().all(|(_, _, names)| names == &vec![second.clone()]));
    own::forget_sample(&files, &samples, &second).unwrap();
    assert_eq!(count(&green), 0);
}

/// A file as the versions before named the images: the same patterns, no `herkunft`.
fn strip_sources(path: &str) {
    let Value::Object(fields) = jsonio::read_json(path).unwrap() else { panic!("object") };
    let kept: Vec<(String, Value)> = fields.into_iter().filter(|(k, _)| !k.starts_with("herkunft")).collect();
    jsonio::write_text_atomic(path, &jsonio::dumps(&Value::Object(kept), None, false), Newline::Keep).unwrap();
}

#[test]
fn patterns_taught_before_images_were_named_are_found_again_by_teaching_the_image_again() {
    let (files, samples) = place("eigene-alt");
    let old = teach(&files, &samples, "2,000", "2,000");
    let other = teach(&files, &samples, "14,160", "14,160");
    for variant in VARIANTS {
        strip_sources(files.of(variant));
    }
    let green = files.of(Variant::Green).to_string();
    let all = count(&green);
    let gone = own::forget_sample(&files, &samples, &old).unwrap();
    assert!(gone > 0, "the patterns of the old image are found");
    let left = count(&green);
    assert!(left > 0 && left < all, "{left} of {all}");
    // What the other image gave is untouched: teaching it again adds nothing new.
    let file = OwnFile::open(&green).unwrap();
    let raster = crop("14,160");
    let lesson = lesson(&raster, "14160", false).unwrap();
    let (taught, _) = lesson_patterns(&raster, &lesson);
    let have = file.taught();
    assert!(taught.iter().all(|(_, d, p, _)| have.iter().any(|(hd, hp, _)| hd == d && hp == p)));
    assert!(samples_in(&samples).contains(&other));
}

#[test]
fn forgetting_one_digit_takes_it_from_every_file_with_the_patterns_derived_from_it() {
    let (files, samples) = place("eigene-ziffer");
    teach(&files, &samples, "14,160", "14,160");
    let list = own::taught_digits(&files);
    let ones: Vec<&own::TaughtDigit> = list.iter().filter(|t| t.digit == '1').collect();
    assert!(!ones.is_empty(), "{list:?}");
    assert!(list.windows(2).all(|p| p[0].digit <= p[1].digit), "by digit");
    let target = ones[0].clone();
    let fours_before = list.iter().filter(|t| t.digit == '4').count();
    let before: Vec<usize> = VARIANTS.iter().map(|v| count(files.of(*v))).collect();
    let gone = own::forget_digit(&files, '1', &target.pattern).unwrap();
    assert!(gone >= 2, "the pattern in both files at least");
    let after = own::taught_digits(&files);
    assert!(!after.iter().any(|t| t.digit == '1' && t.pattern == target.pattern));
    assert_eq!(after.iter().filter(|t| t.digit == '4').count(), fours_before, "other digits stay");
    let now: Vec<usize> = VARIANTS.iter().map(|v| count(files.of(*v))).collect();
    assert!(now.iter().zip(&before).all(|(n, b)| n < b), "{now:?} {before:?}");
    // A pattern that is not there takes nothing.
    assert_eq!(own::forget_digit(&files, '1', &target.pattern), Ok(0));
}

#[test]
fn forgetting_everything_leaves_the_shipped_templates_and_the_images() {
    let (files, samples) = place("eigene-alles");
    teach(&files, &samples, "2,000", "2,000");
    std::fs::write(&files.shared, "{\"format\": 1, \"ziffern\": {}}").unwrap();
    own::forget_all(&files).unwrap();
    for (_, path) in files.all() {
        assert!(!std::path::Path::new(path).exists(), "{path}");
    }
    assert_eq!(samples_in(&samples).len(), 1, "the image stays");
    assert_eq!(templates_by_variant(SHIPPED, &files).green.learned_digits(), 10);
    // Nothing there: nothing to do.
    assert_eq!(own::forget_all(&files), Ok(()));
}

#[test]
fn a_file_that_cannot_be_read_is_left_as_it_is() {
    let (files, samples) = place("eigene-kaputt");
    let name = teach(&files, &samples, "2,000", "2,000");
    let green = files.of(Variant::Green).to_string();
    std::fs::write(&green, "kein JSON").unwrap();
    assert_eq!(own::forget_sample(&files, &samples, &name), Err(FileError::Read));
    assert_eq!(std::fs::read_to_string(&green).unwrap(), "kein JSON");
    assert_eq!(own::forget_digit(&files, '2', &[0; CELLS]), Err(FileError::Read));
}

#[test]
fn a_digit_in_several_files_is_listed_once_with_the_images_of_all() {
    let (files, samples) = place("eigene-einmal");
    let name = teach(&files, &samples, "2,000", "2,000");
    let twos: Vec<own::TaughtDigit> = own::taught_digits(&files).into_iter().filter(|t| t.digit == '2').collect();
    assert_eq!(twos.len(), 1, "taught into both files, listed once: {twos:?}");
    // The same pattern in the shared file of an older version, from another image.
    let mut shared = OwnFile::open(&files.shared).unwrap();
    shared.add(Kind::Taught, '2', &twos[0].pattern, Some("alt.png"), 24);
    shared.save(&files.shared).unwrap();
    let twos: Vec<own::TaughtDigit> = own::taught_digits(&files).into_iter().filter(|t| t.digit == '2').collect();
    assert_eq!(twos.len(), 1);
    assert_eq!(twos[0].sources, vec!["alt.png".to_string(), name]);
}

#[test]
fn the_number_of_a_kept_image_is_read_from_its_name() {
    assert_eq!(own::sample_number("17080_20261121-185805-143.png"), "17080");
    assert_eq!(own::sample_number("frei.png"), "");
}
