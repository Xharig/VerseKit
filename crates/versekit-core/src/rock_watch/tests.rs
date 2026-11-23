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

use super::*;

fn word(t: &str, x: f64, y: f64) -> Word {
    Word::new(t, x, y, 50.0, 10.0)
}

fn panel_words(mass: &str) -> Vec<Word> {
    vec![
        word("SCAN", 400.0, 300.0),
        word("RESULTS", 460.0, 300.0),
        word("MASS:", 400.0, 340.0),
        word(mass, 520.0, 340.0),
        word("RESISTANCE:", 400.0, 360.0),
        word("25%", 520.0, 360.0),
    ]
}

/// A dark picture without any 0 or 8 to check in the values used here.
fn picture() -> Bitmap {
    Bitmap { width: 1000, height: 800, raw: vec![0; 1000 * 800 * 4] }
}

/// A recognition that gives the panel words to every reading job, and counts the searches (jobs over tiles).
fn reader(mass: &'static str, searches: Arc<Mutex<u32>>, panel: Arc<AtomicBool>) -> Reader {
    Box::new(move |_: &Bitmap, jobs: &[Job]| {
        if jobs.len() != scan::PASSES.len() || jobs[0].scale != scan::PASSES[0].0 {
            *searches.lock().unwrap() += 1;
        }
        let words = if panel.load(Ordering::Relaxed) { panel_words(mass) } else { Vec::new() };
        Ok(jobs.iter().map(|_| words.clone()).collect())
    })
}

#[test]
fn the_part_of_the_panel_reaches_from_its_head_down_and_right() {
    let area = panel_area(&panel_words("5123"), (1000, 800)).unwrap();
    assert_eq!(area, (380, 280, 260, 360));
    let near_edge = panel_area(&[word("SCAN", 900.0, 700.0)], (1000, 800)).unwrap();
    assert_eq!(near_edge, (880, 680, 120, 120), "cut at the picture");
    let by_mass = panel_area(&[word("MASS:", 400.0, 340.0)], (1000, 800)).unwrap();
    assert_eq!(by_mass.1, 250);
    assert_eq!(panel_area(&[word("OPTIMAL", 1.0, 1.0)], (1000, 800)), None);
}

#[test]
fn the_crop_is_the_brightest_channel_of_the_frame_with_a_margin() {
    let mut bitmap = Bitmap { width: 8, height: 8, raw: vec![0; 8 * 8 * 4] };
    // The point (2, 2): blue 10, green 200, red 30.
    bitmap.raw[((2 * 8 + 2) * 4)..((2 * 8 + 2) * 4 + 3)].copy_from_slice(&[10, 200, 30]);
    let gray = crop(&bitmap, (2.0, 2.0, 2.0, 4.0)).unwrap();
    assert_eq!((gray.w, gray.h), (4, 6), "one point of margin all round");
    assert_eq!(gray.at(1, 1), 200);
    assert_eq!(gray.at(0, 0), 0);
    assert_eq!(crop(&bitmap, (0.0, 0.0, 0.5, 0.5)), None, "too small");
}

#[test]
fn the_panel_is_searched_only_while_mining_then_read_in_its_part_and_values_stand_after_two_pictures() {
    let searches = Arc::new(Mutex::new(0));
    let panel = Arc::new(AtomicBool::new(true));
    let mut read = reader("5123", searches.clone(), panel.clone());
    let mut beat = Beat::default();
    let start = Instant::now();
    let (sure, seen) = beat.step(&picture(), false, start, &mut read);
    assert!(sure.is_empty() && !seen);
    assert_eq!(*searches.lock().unwrap(), 0, "no search without mining");
    let (sure, seen) = beat.step(&picture(), true, start, &mut read);
    assert!(seen && sure.is_empty(), "first picture");
    assert_eq!(*searches.lock().unwrap(), 1);
    let (sure, _) = beat.step(&picture(), true, start + Duration::from_secs(1), &mut read);
    assert_eq!((sure.mass, sure.resistance), (Some(5123.0), Some(25.0)));
    assert_eq!(*searches.lock().unwrap(), 1, "the part is read, the picture not searched again");
    panel.store(false, Ordering::Relaxed);
    for beat_no in 0..LOST_BEATS {
        let (sure, seen) = beat.step(&picture(), true, start + Duration::from_secs(2), &mut read);
        assert!(sure.is_empty() && !seen, "beat {beat_no}");
    }
    assert!(beat.area.is_none(), "the part is forgotten");
    beat.step(&picture(), true, start + Duration::from_secs(3), &mut read);
    assert_eq!(*searches.lock().unwrap(), 1, "the next search waits its time");
    beat.step(&picture(), true, start + Duration::from_secs(10), &mut read);
    assert_eq!(*searches.lock().unwrap(), 2);
}

#[test]
fn a_value_with_a_zero_the_picture_cannot_confirm_stays_unknown() {
    let searches = Arc::new(Mutex::new(0));
    let mut read = reader("5300", searches, Arc::new(AtomicBool::new(true)));
    let mut beat = Beat::default();
    let start = Instant::now();
    beat.step(&picture(), true, start, &mut read);
    let (sure, seen) = beat.step(&picture(), true, start, &mut read);
    assert!(seen);
    assert_eq!(sure.mass, None, "the dark picture shows no digits");
    assert_eq!(sure.resistance, Some(25.0), "no 0 and no 8: nothing to confirm");
}

#[test]
fn the_lines_follow_the_panel_the_loadout_and_the_switches() {
    use crate::language::{Lang, with_language};
    use crate::rock_crack::{Factors, Laser, Loadout};
    let loadout = Loadout {
        lasers: vec![Laser { power: 3900.0, min_share: 0.0, head: Factors::NONE, modules: vec![] }],
        ..Loadout::default()
    };
    let sure = Sure {
        mass: Some(5000.0),
        resistance: Some(0.0),
        instability: Some(5.0),
        scu: Some(10.0),
        shares: vec![("QUANTAINIUM (RAW)".into(), 30.0), ("INERT MATERIALS".into(), 70.0)],
        qualities: vec![Some(597.0), Some(0.0)],
        verdict: None,
        with_head: false,
    };
    let state = State { sure, seen: true, version: 1, place: None };
    let price = |name: &str| crate::stock_data::norm_material(name).eq("quantainium").then_some(1000.0);
    let texts = |show| with_language(Lang::De, || lines(&state, &loadout, price, show));
    let both = texts((true, true));
    assert_eq!(both.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["Aufbrechbar · leicht · Gas 28 %", "Wert ca. 3.000 aUEC"]);
    assert!(both[0].good && !both[1].good);
    assert_eq!(texts((false, true)).len(), 1);
    assert!(texts((false, false)).is_empty());
    let empty = State { sure: Sure::default(), seen: true, version: 2, place: None };
    assert!(lines(&empty, &loadout, price, (true, true)).is_empty(), "an empty panel says nothing");
    let unsure = State { sure: Sure { instability: Some(5.0), ..Sure::default() }, seen: true, version: 2, place: None };
    let said = with_language(Lang::De, || lines(&unsure, &loadout, price, (true, true)));
    assert_eq!(said.len(), 1, "no worth without the SCU");
    assert_eq!(said[0].text, "Aufbrechbar: Scan nicht sicher gelesen");
    assert!(lines(&State::default(), &loadout, price, (true, true)).is_empty(), "no panel, no lines");
}

#[test]
fn the_search_of_a_wide_picture_is_one_job_at_a_small_enlargement() {
    let wide = search_jobs(5120, 1440);
    assert_eq!(wide.len(), 1);
    assert_eq!((wide[0].area, wide[0].scale), ((0, 0, 5120, 1440), 1.5));
    let four_k = search_jobs(7680, 2160);
    assert_eq!((four_k.len(), four_k[0].scale), (1, 1.0), "never made smaller");
    let small = search_jobs(1280, 720);
    assert_eq!(small[0].scale, 3.0);
    // Too wide for one job: parts side by side that overlap.
    let parts = search_jobs(15360, 1440);
    assert!(parts.len() >= 3, "{parts:?}");
    assert!(parts.iter().all(|j| j.area.2 as f64 * j.scale <= crate::refinery_scan::OCR_LIMIT));
    assert!(parts.windows(2).all(|p| p[1].area.0 < p[0].area.0 + p[0].area.2), "overlap");
    assert_eq!(parts.last().map(|j| j.area.0 + j.area.2), Some(15360), "to the right edge");
}

#[test]
fn a_place_found_is_kept_by_size_and_read_at_the_next_start_without_a_search() {
    let searches = Arc::new(Mutex::new(0));
    let mut read = reader("5123", searches.clone(), Arc::new(AtomicBool::new(true)));
    let mut first = Beat::default();
    first.step(&picture(), true, Instant::now(), &mut read);
    let found = first.found.clone().expect("a place found by the search");
    assert_eq!(found, ("1000x800".to_string(), [380, 280, 260, 360]));
    let kept = places_value(vec![("2560x1440".into(), [1, 2, 3, 4])], &found.0, found.1);
    let back = places_of(Some(&kept));
    assert_eq!(back, vec![("2560x1440".to_string(), [1, 2, 3, 4]), found.clone()]);
    let mut next = Beat { known: back, ..Beat::default() };
    next.step(&picture(), false, Instant::now(), &mut read);
    assert_eq!(*searches.lock().unwrap(), 1, "the kept place is read without searching, even while not mining");
    assert!(next.found.is_none(), "nothing new to keep");
    assert!(places_of(None).is_empty());
}

#[test]
fn the_colour_of_a_frame_is_the_mean_of_its_bright_points() {
    let mut bitmap = Bitmap { width: 4, height: 1, raw: vec![0; 16] };
    // BGRA: a green point, a white point, two dark ones.
    bitmap.raw[0..4].copy_from_slice(&[86, 232, 106, 255]);
    bitmap.raw[4..8].copy_from_slice(&[86, 232, 106, 255]);
    bitmap.raw[8..12].copy_from_slice(&[20, 30, 20, 255]);
    let green = bright_colour(&bitmap, (0.0, 0.0, 4.0, 1.0)).unwrap();
    assert_eq!(green, (106.0, 232.0, 86.0));
    assert!(scan::is_head_green(green));
    assert!(!scan::is_head_green((195.0, 237.0, 212.0)), "the plain values are near white");
    assert_eq!(bright_colour(&bitmap, (2.0, 0.0, 2.0, 1.0)), None, "only dark points");
}

#[test]
fn a_green_resistance_of_the_same_rock_takes_the_plain_one_read_before() {
    let mut beat = Beat::default();
    let plain = Sure { mass: Some(32975.0), resistance: Some(61.0), ..Sure::default() };
    assert_eq!(beat.with_raw(plain).resistance, Some(61.0));
    let green = Sure { mass: Some(32975.0), resistance: Some(42.0), with_head: true, ..Sure::default() };
    let back = beat.with_raw(green);
    assert_eq!((back.resistance, back.with_head), (Some(61.0), false), "the rock's own, not twice the head");
    let other = Sure { mass: Some(16365.0), resistance: Some(38.0), with_head: true, ..Sure::default() };
    let kept = beat.with_raw(other);
    assert_eq!((kept.resistance, kept.with_head), (Some(38.0), true), "another rock keeps the green value");
}

#[test]
fn another_size_of_the_game_window_forgets_the_part() {
    let searches = Arc::new(Mutex::new(0));
    let mut read = reader("5123", searches.clone(), Arc::new(AtomicBool::new(true)));
    let mut beat = Beat::default();
    let start = Instant::now();
    beat.step(&picture(), true, start, &mut read);
    let smaller = Bitmap { width: 800, height: 600, raw: vec![0; 800 * 600 * 4] };
    beat.step(&smaller, true, start + Duration::from_secs(9), &mut read);
    assert_eq!(*searches.lock().unwrap(), 2);
}
