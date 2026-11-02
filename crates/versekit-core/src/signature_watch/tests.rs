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

use std::sync::atomic::AtomicUsize;

use super::*;
use crate::jsonio::{self, Value};
use crate::signature_scan::{Found, Images, OwnFiles, Reading, Variant};
use crate::signature_window::Hint;
use crate::testdir;

const PICTURES: &str = include_str!("../../tests/data/signatur-pruefbilder.json");
const REAL: &str = include_str!("../../../../tools/pruefdaten/signatur-windows-klein.json");

fn voter() -> Voter {
    Voter::new(4.0)
}

/// A reading that is a possible value.
fn known(value: u64) -> Option<Seen> {
    Some(Seen { value, known: true })
}

/// A reading that is no possible value.
fn unknown(value: u64) -> Option<Seen> {
    Some(Seen { value, known: false })
}

#[test]
fn a_number_is_shown_at_the_second_equal_reading_of_the_last_three() {
    let mut v = voter();
    assert_eq!(v.step(true, known(2000), 0.0), Change::Keep);
    assert_eq!(v.step(true, known(2000), 0.5), Change::Show(2000));
    assert_eq!(v.shown(), Some(2000));
    // The same again changes nothing.
    assert_eq!(v.step(true, known(2000), 1.0), Change::Keep);
}

#[test]
fn two_different_readings_show_nothing_and_a_stray_reading_does_not_replace_a_shown_number() {
    let mut v = voter();
    assert_eq!(v.step(true, known(2000), 0.0), Change::Keep);
    assert_eq!(v.step(true, known(7000), 0.5), Change::Keep);
    assert_eq!(v.shown(), None);
    assert_eq!(v.step(true, known(2000), 1.0), Change::Show(2000));
    // One 7000 amid 2000s: the vote still stands for 2000.
    assert_eq!(v.step(true, known(2000), 1.5), Change::Keep);
    assert_eq!(v.step(true, known(7000), 2.0), Change::Keep);
    assert_eq!(v.shown(), Some(2000));
    assert_eq!(v.step(true, known(7000), 2.5), Change::Show(7000));
}

#[test]
fn a_number_that_is_no_possible_value_is_shown_at_the_fourth_equal_reading_of_the_last_five() {
    let mut v = voter();
    for (beat, now) in [0.0, 0.5, 1.0].into_iter().enumerate() {
        assert_eq!(v.step(true, unknown(1700), now), Change::Keep, "reading {}", beat + 1);
    }
    assert_eq!(v.step(true, unknown(1700), 1.5), Change::Show(1700));
    assert_eq!(v.shown(), Some(1700));
    // One stray reading among five changes nothing for the number shown.
    assert_eq!(v.step(true, unknown(1710), 2.0), Change::Keep);
    assert_eq!(v.step(true, unknown(1700), 2.5), Change::Keep);
    assert_eq!(v.shown(), Some(1700));

    // One stray reading amid the run delays the show by a beat.
    let mut v = voter();
    for (reading, now) in [(1700, 0.0), (1700, 0.5), (1710, 1.0), (1700, 1.5)] {
        assert_eq!(v.step(true, unknown(reading), now), Change::Keep);
    }
    assert_eq!(v.step(true, unknown(1700), 2.0), Change::Show(1700));

    // Three equal readings in a row are not enough.
    let mut v = voter();
    for (reading, now) in [(1710, 0.0), (1720, 0.5), (1700, 1.0), (1700, 1.5), (1700, 2.0)] {
        assert_eq!(v.step(true, unknown(reading), now), Change::Keep);
    }
    assert_eq!(v.shown(), None);
}

#[test]
fn a_number_shown_that_is_a_possible_value_is_replaced_by_an_unknown_one_only_after_the_longer_run() {
    let mut v = voter();
    v.step(true, known(2000), 0.0);
    assert_eq!(v.step(true, known(2000), 0.5), Change::Show(2000));
    for now in [1.0, 1.5, 2.0] {
        assert_eq!(v.step(true, unknown(1700), now), Change::Keep);
    }
    assert_eq!(v.shown(), Some(2000));
    assert_eq!(v.step(true, unknown(1700), 2.5), Change::Show(1700));
    // A possible value replaces an unknown one as it always did: two of the last three.
    assert_eq!(v.step(true, known(2000), 3.0), Change::Keep);
    assert_eq!(v.step(true, known(2000), 3.5), Change::Show(2000));
}

#[test]
fn the_display_clears_only_after_the_grace_without_a_number() {
    let mut v = voter();
    v.step(true, known(2000), 0.0);
    v.step(true, known(2000), 0.5);
    assert_eq!(v.step(true, None, 1.0), Change::Keep);
    assert_eq!(v.step(true, None, 4.5), Change::Keep);
    assert_eq!(v.step(true, None, 4.6), Change::Clear);
    assert_eq!(v.shown(), None);
    assert_eq!(v.step(true, None, 5.0), Change::Keep);
}

#[test]
fn a_beat_without_a_search_casts_no_vote_and_only_runs_the_clock_down() {
    let mut v = voter();
    v.step(true, known(2000), 0.0);
    v.step(true, known(2000), 0.5);
    // Without the game in front the display stays for the grace and then clears; the history is untouched.
    assert_eq!(v.step(false, None, 3.0), Change::Keep);
    assert_eq!(v.step(false, None, 4.6), Change::Clear);
    // One reading after that is still the second of the last three: 2000 had been read before.
    assert_eq!(v.step(true, known(2000), 5.0), Change::Show(2000));
}

#[test]
fn stopping_clears_at_once_and_forgets_the_history() {
    let mut v = voter();
    v.step(true, known(2000), 0.0);
    v.step(true, known(2000), 0.5);
    assert_eq!(v.stop(), Change::Clear);
    assert_eq!(v.stop(), Change::Keep);
    assert_eq!(v.step(true, known(2000), 1.0), Change::Keep);
}

#[test]
fn a_reading_is_known_when_it_is_in_the_list_of_possible_values() {
    let values = [3170, 6340];
    assert_eq!(Seen::of(3170, &values), Seen { value: 3170, known: true });
    assert_eq!(Seen::of(1700, &values), Seen { value: 1700, known: false });
}

#[test]
fn the_search_area_is_the_middle_of_the_game_window() {
    assert_eq!(search_area(Some((0, 0, 1920, 1080))), Some((384, 108, 1152, 810)));
    assert_eq!(search_area(Some((100, 50, 1000, 700))), Some((300, 120, 600, 525)));
    assert_eq!(search_area(None), None);
    assert_eq!(search_area(Some((0, 0, 0, 0))), None);
}

fn found(value: Option<u64>, boxr: Option<(i32, i32, i32, i32)>) -> Found {
    let reading = Reading { value, reason: None, distance: 0.1, digits: Vec::new(), threshold: None };
    Found { reading, boxr, ..Found::silent() }
}

/// A tracker with made-up grab and search: the rectangles asked for are recorded, the search answers from a list.
struct Bench {
    asked: Vec<Area>,
    answers: Vec<Found>,
    at: usize,
}

impl Bench {
    fn new(answers: Vec<Found>) -> Bench {
        Bench { asked: Vec::new(), answers, at: 0 }
    }

    fn look(&mut self, tracker: &mut Tracker, game: Option<Area>, now: f64) -> Option<Look> {
        let (asked, answers, at) = (&mut self.asked, &self.answers, &mut self.at);
        let mut grab = |area: Area| {
            asked.push(area);
            Ok(vec![0u8; 4])
        };
        let mut search = |_: &[u8], _: i32, _: i32| {
            let answer = answers[(*at).min(answers.len() - 1)].clone();
            *at += 1;
            answer
        };
        tracker.look(game, now, &mut grab, &mut search).expect("look")
    }
}

const GAME: Area = (0, 0, 1920, 1080);

#[test]
fn the_first_beat_searches_the_whole_area_and_the_next_only_around_the_pill() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(Some(2000), Some((100, 50, 120, 60)))]);
    let look = bench.look(&mut tracker, Some(GAME), 0.0).expect("first look");
    assert_eq!(bench.asked[0], (384, 108, 1152, 810));
    // The place is the crop's place inside the area, moved to the screen.
    assert_eq!(look.box_abs, Some((484, 158, 120, 60)));
    assert_eq!(tracker.boxr, Some((484, 158, 120, 60)));

    bench.look(&mut tracker, Some(GAME), 0.5).expect("tracked");
    // Window: margins of at least 240 / 120 around the pill, cut to the area.
    assert_eq!(bench.asked[1], (384, 108, 580, 350));
    assert_eq!((tracker.full_searches, tracker.tracked_searches), (1, 1));
}

#[test]
fn the_whole_area_is_searched_again_every_five_seconds_even_with_a_pill() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(Some(2000), Some((100, 50, 120, 60)))]);
    bench.look(&mut tracker, Some(GAME), 0.0);
    bench.look(&mut tracker, Some(GAME), 4.9);
    assert_eq!(tracker.full_searches, 1);
    bench.look(&mut tracker, Some(GAME), 5.0);
    assert_eq!(tracker.full_searches, 2);
}

#[test]
fn a_pill_that_is_not_in_the_window_makes_the_search_look_in_the_whole_area_when_there_is_time() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![
        found(Some(2000), Some((100, 50, 120, 60))),
        found(None, None),
        found(None, None),
        found(Some(2000), Some((500, 300, 120, 60))),
    ]);
    bench.look(&mut tracker, Some(GAME), 0.0);
    // Shortly after: the window finds nothing, but the full search is not due yet (FULL_BUSY_S 1 s after 0.0).
    let look = bench.look(&mut tracker, Some(GAME), 0.5).expect("tracked");
    assert_eq!(look.found.reading.value, None);
    assert_eq!(tracker.full_searches, 1);
    // A second later the window finds nothing and the full search is due: it runs in the same beat.
    let look = bench.look(&mut tracker, Some(GAME), 1.2).expect("both");
    assert_eq!(look.found.reading.value, Some(2000));
    assert_eq!(tracker.full_searches, 2);
    assert_eq!(tracker.boxr, Some((884, 408, 120, 60)));
}

#[test]
fn the_pill_counts_as_gone_after_two_beats_without_a_value() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(Some(2000), Some((100, 50, 120, 60))), found(None, None)]);
    bench.look(&mut tracker, Some(GAME), 0.0);
    bench.look(&mut tracker, Some(GAME), 0.2);
    assert!(tracker.boxr.is_some());
    bench.look(&mut tracker, Some(GAME), 0.4);
    assert!(tracker.boxr.is_none());
}

#[test]
fn without_a_pill_the_full_search_runs_at_most_every_second_when_one_was_seen_lately_else_every_two() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(Some(2000), Some((100, 50, 120, 60))), found(None, None)]);
    bench.look(&mut tracker, Some(GAME), 0.0);
    bench.look(&mut tracker, Some(GAME), 0.2);
    bench.look(&mut tracker, Some(GAME), 0.4);
    assert!(tracker.boxr.is_none());
    let before = tracker.full_searches;
    assert!(bench.look(&mut tracker, Some(GAME), 0.6).is_none(), "not due yet");
    assert!(bench.look(&mut tracker, Some(GAME), 1.3).is_some(), "busy: a second after the last full search");
    assert_eq!(tracker.full_searches, before + 1);
    // Long after the last value: every two seconds.
    assert!(bench.look(&mut tracker, Some(GAME), 10.0).is_some());
    assert!(bench.look(&mut tracker, Some(GAME), 11.0).is_none());
    assert!(bench.look(&mut tracker, Some(GAME), 12.0).is_some());
}

#[test]
fn a_clock_that_jumps_back_does_not_hold_the_full_search_off() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(None, None)]);
    bench.look(&mut tracker, Some(GAME), 100.0);
    assert!(bench.look(&mut tracker, Some(GAME), 3.0).is_some());
}

#[test]
fn without_a_game_nothing_is_searched() {
    let mut tracker = Tracker::new();
    let mut bench = Bench::new(vec![found(None, None)]);
    assert!(bench.look(&mut tracker, None, 0.0).is_none());
    assert!(bench.asked.is_empty());
}

// ------------------------------------------------------------ the thread

/// A made-up screen: a picture with the crop of a signature at a place; the game window is the whole picture.
struct Canvas {
    width: i32,
    height: i32,
    with_pill: Arc<AtomicBool>,
    raw_full: Vec<u8>,
    raw_blank: Vec<u8>,
    grabs: Arc<AtomicUsize>,
    /// Every area that was grabbed.
    areas: Arc<Mutex<Vec<Area>>>,
}

impl Source for Canvas {
    fn game(&mut self) -> Option<Area> {
        Some((0, 0, self.width, self.height))
    }
    fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError> {
        self.grabs.fetch_add(1, Ordering::Relaxed);
        self.areas.lock().unwrap().push(area);
        let raw = if self.with_pill.load(Ordering::Relaxed) { &self.raw_full } else { &self.raw_blank };
        let mut out = Vec::new();
        for y in area.1..area.1 + area.3 {
            let start = ((y * self.width + area.0) * 4) as usize;
            out.extend_from_slice(&raw[start..start + area.2 as usize * 4]);
        }
        Ok(out)
    }
}

fn small_crop() -> (u64, Gray) {
    crop_at(0)
}

fn crop_at(index: usize) -> (u64, Gray) {
    crop_of(PICTURES, index)
}

/// The crop with this key in a test file, with its number.
fn crop_named(text: &str, name: &str) -> (u64, Gray) {
    let file = jsonio::loads(text).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    crop_of(text, images.iter().position(|(key, _)| key == name).expect("key"))
}

/// The crop at `index` of a test file with its number.
fn crop_of(text: &str, index: usize) -> (u64, Gray) {
    let file = jsonio::loads(text).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    let (key, raster) = &images[index];
    let number: String = key.split('#').next().unwrap().chars().filter(char::is_ascii_digit).collect();
    let Value::Array(rows) = raster else { panic!("rows") };
    let mut data = Vec::new();
    let mut w = 0;
    for row in rows {
        let Value::Array(row) = row else { panic!("row") };
        w = row.len();
        data.extend(row.iter().map(|v| if let Value::Int(v) = v { v.parse::<u8>().unwrap() } else { 0 }));
    }
    (number.parse().unwrap(), Gray::new(w, rows.len(), data))
}

#[test]
fn the_thread_shows_the_number_while_the_pill_is_there_follows_it_and_clears_when_it_is_gone() {
    let (number, crop) = small_crop();
    let (width, height) = (1000i32, 700i32);
    let mut full = vec![0u8; (width * height * 4) as usize];
    for p in full.as_chunks_mut::<4>().0 {
        *p = [5, 12, 7, 255];
    }
    let blank = full.clone();
    let (left, top) = (420usize, 300usize);
    for y in 0..crop.h {
        for x in 0..crop.w {
            let v = crop.data[y * crop.w + x];
            let at = ((top + y) * width as usize + left + x) * 4;
            full[at..at + 4].copy_from_slice(&[v.saturating_add(60), v, 255, 255]);
        }
    }
    let with_pill = Arc::new(AtomicBool::new(true));
    let grabs = Arc::new(AtomicUsize::new(0));
    let canvas = Canvas {
        width,
        height,
        with_pill: with_pill.clone(),
        raw_full: full,
        raw_blank: blank,
        grabs: grabs.clone(),
        areas: Arc::default(),
    };
    let folder = testdir::fresh("signatur-wache");
    let files = Files {
        shipped: scan::SHIPPED_TEMPLATES.to_string(),
        own: OwnFiles::in_folder(|name| folder.join(name).to_string_lossy().into_owned()),
        mining: folder.join("fehlt.json").to_string_lossy().into_owned(),
    };
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    let watch = Watch::start(Box::new(canvas), files, fast(), Hint::default(), move || {
        counter.fetch_add(1, Ordering::Relaxed);
    })
    .expect("thread");
    assert!(watch.alive());

    let wait_for = |what: &dyn Fn(&State) -> bool| -> State {
        for _ in 0..200 {
            let state = watch.state();
            if what(&state) {
                return state;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("not reached: {:?}", watch.state());
    };
    let shown = wait_for(&|s| s.shown.is_some());
    assert_eq!(shown.shown, Some(number));
    let pill = shown.pill.expect("place of the pill");
    assert!(pill.0 <= left as i32 + crop.w as i32 && pill.0 + pill.2 >= left as i32, "{pill:?}");
    assert!(pill.1 <= top as i32 + crop.h as i32 && pill.1 + pill.3 >= top as i32, "{pill:?}");
    assert_eq!(shown.game, Some((0, 0, width, height)));
    let frame = shown.frame.expect("picture for teaching");
    assert!(frame.image.w > 0 && frame.number >= 1);
    assert!(woken.load(Ordering::Relaxed) >= 1);

    with_pill.store(false, Ordering::Relaxed);
    let cleared = wait_for(&|s| s.shown.is_none());
    assert_eq!(cleared.pill, None);
    assert!(cleared.version > shown.version);
    // Around the pill the window is searched every beat.
    assert!(grabs.load(Ordering::Relaxed) >= 3);

    watch.stop();
}

#[test]
fn a_stopped_watch_has_cleared_its_number_and_a_dead_thread_is_not_alive() {
    struct Nothing;
    impl Source for Nothing {
        fn game(&mut self) -> Option<Area> {
            None
        }
        fn grab(&mut self, _: Area) -> Result<Vec<u8>, GrabError> {
            Err(GrabError::Failed)
        }
    }
    let folder = testdir::fresh("signatur-wache-leer");
    let files = Files {
        shipped: scan::SHIPPED_TEMPLATES.to_string(),
        own: OwnFiles::in_folder(|name| folder.join(name).to_string_lossy().into_owned()),
        mining: folder.join("fehlt.json").to_string_lossy().into_owned(),
    };
    let watch = Watch::start(Box::new(Nothing), files, fast(), Hint::default(), || {}).expect("thread");
    // Waits for the first look instead of a fixed time: under load a fixed wait can pass without one.
    assert!(wait_until(&watch, |s| s.beats >= 1).is_some(), "the watch has looked: {:?}", watch.state());
    assert!(watch.alive());
    assert_eq!(watch.state().shown, None);
    watch.stop();
}

#[test]
fn the_last_frame_goes_through_the_file_with_its_number_and_value() {
    let (number, crop) = small_crop();
    let folder = testdir::fresh("signatur-letztes-bild");
    let path = folder.join(LAST_FRAME_FILE).to_string_lossy().into_owned();
    assert_eq!(read_last_frame(&path), None, "no file yet");
    let frame = Frame::new(crop.clone(), Some(number), 7);
    assert!(write_last_frame(&path, &frame));
    assert_eq!(read_last_frame(&path), Some(frame));
    // A newer one replaces it; a frame without a value keeps that.
    let newer = Frame::new(crop, None, 8);
    assert!(write_last_frame(&path, &newer));
    assert_eq!(read_last_frame(&path), Some(newer));
}

#[test]
fn bytes_that_are_no_frame_are_refused() {
    assert_eq!(frame_of(b""), None);
    assert_eq!(frame_of(b"VKS1"), None);
    let (_, crop) = small_crop();
    let mut bytes = frame_bytes(&Frame::new(crop, None, 1));
    assert!(frame_of(&bytes).is_some());
    bytes.pop();
    assert_eq!(frame_of(&bytes), None, "the points do not fill the size");
    bytes[0] = b'X';
    assert_eq!(frame_of(&bytes), None, "wrong magic");
}

// ------------------------------------------------------------ where the watch reads

/// The times of a check: a beat every 50 ms, the display goes after 0.6 s, the middle is searched again after
/// `lost` seconds.
fn times(lost: f64) -> Times {
    Times { interval: 0.05, clear_after: 0.6, lost_after: lost }
}

fn fast() -> Times {
    times(100.0)
}

/// A game window of 1000 by 700 points with the crop of a signature at (420, 300); every grab is recorded.
fn scene() -> (Canvas, Arc<Mutex<Vec<Area>>>, u64) {
    scene_of(0)
}

fn scene_of(index: usize) -> (Canvas, Arc<Mutex<Vec<Area>>>, u64) {
    scene_with(crop_at(index))
}

/// The same scene with the location symbol of crop 5 (14,160) replaced by a bar taller than a digit: a row of
/// digits that is found, but stands in no signature box.
fn scene_without_symbol() -> (Canvas, Arc<Mutex<Vec<Area>>>, u64) {
    let (number, mut crop) = crop_at(5);
    for y in 0..crop.h {
        for x in 0..45 {
            crop.data[y * crop.w + x] = if (39..42).contains(&x) && (9..22).contains(&y) { 255 } else { 28 };
        }
    }
    scene_with((number, crop))
}

fn scene_with(crop: (u64, Gray)) -> (Canvas, Arc<Mutex<Vec<Area>>>, u64) {
    scene_pixels(crop, |_, _, v| [v.saturating_add(60), v, 255, 255])
}

/// The scene with the points of the crop made by `pixel(x, y, value)` as BGRA.
fn scene_pixels(
    (number, crop): (u64, Gray),
    pixel: impl Fn(usize, usize, u8) -> [u8; 4],
) -> (Canvas, Arc<Mutex<Vec<Area>>>, u64) {
    let (width, height) = (1000i32, 700i32);
    let mut full = vec![0u8; (width * height * 4) as usize];
    for p in full.as_chunks_mut::<4>().0 {
        *p = [5, 12, 7, 255];
    }
    let blank = full.clone();
    for y in 0..crop.h {
        for x in 0..crop.w {
            let v = crop.data[y * crop.w + x];
            let at = ((300 + y) * width as usize + 420 + x) * 4;
            full[at..at + 4].copy_from_slice(&pixel(x, y, v));
        }
    }
    let areas = Arc::new(Mutex::new(Vec::new()));
    let canvas = Canvas {
        width,
        height,
        with_pill: Arc::new(AtomicBool::new(true)),
        raw_full: full,
        raw_blank: blank,
        grabs: Arc::default(),
        areas: areas.clone(),
    };
    (canvas, areas, number)
}

fn files_in(name: &str) -> Files {
    let folder = testdir::fresh(name);
    Files {
        shipped: scan::SHIPPED_TEMPLATES.to_string(),
        own: OwnFiles::in_folder(|name| folder.join(name).to_string_lossy().into_owned()),
        mining: folder.join("fehlt.json").to_string_lossy().into_owned(),
    }
}

fn wait_until(watch: &Watch, what: impl Fn(&State) -> bool) -> Option<State> {
    for _ in 0..160 {
        let state = watch.state();
        if what(&state) {
            return Some(state);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

#[test]
fn the_watch_counts_which_grey_image_read_the_number_and_keeps_both_for_teaching() {
    // Digits in the green channel: the green grey image reads.
    let (canvas, _, number) = scene();
    let watch =
        Watch::start(Box::new(canvas), files_in("graubild-gruen"), fast(), Hint::default(), || {}).expect("thread");
    let shown = wait_until(&watch, |s| s.shown.is_some() && s.frame.is_some()).expect("shown");
    watch.stop();
    assert_eq!(shown.shown, Some(number));
    assert!(shown.variant_counts.green >= 1, "{:?}", shown.variant_counts);
    assert_eq!(shown.variant_counts.bright, 0);
    let frame = shown.frame.expect("frame");
    assert_eq!(frame.variant, Some(Variant::Green));
    assert_eq!(frame.images.green.as_ref(), Some(&frame.image));
    assert!(frame.images.bright.as_ref().is_some_and(|g| g.w == frame.image.w && g.h == frame.image.h));

    // Green only marks the pill, blue and red carry the digits: the bright grey image reads, and only it is counted.
    let (canvas, _, _) =
        scene_pixels(crop_named(REAL, "16,000#s1"), |_, y, v| [v, if (7..31).contains(&y) { 170 } else { 12 }, v, 255]);
    let watch =
        Watch::start(Box::new(canvas), files_in("graubild-hell"), fast(), Hint::default(), || {}).expect("thread");
    let counted = wait_until(&watch, |s| s.variant_counts.bright >= 1).expect("counted");
    watch.stop();
    assert_eq!(counted.variant_counts.green, 0, "{:?}", counted.variant_counts);
}

#[test]
fn the_counts_of_the_grey_images_go_through_their_file_and_a_missing_or_foreign_file_counts_zero() {
    let folder = testdir::fresh("graubild-zaehler");
    let path = folder.join(COUNTS_FILE).to_string_lossy().into_owned();
    assert_eq!(read_counts(&path), scan::ByVariant::default(), "no file yet");
    assert!(write_counts(&path, &scan::ByVariant { green: 41, bright: 2 }));
    assert_eq!(read_counts(&path), scan::ByVariant { green: 41, bright: 2 });
    assert!(write_counts(&path, &scan::ByVariant { green: 42, bright: 2 }), "replaces the file");
    assert_eq!(read_counts(&path).green, 42);
    for foreign in
        ["{kaputt", "[1, 2]", "{\"format\": 2, \"gruen\": 5, \"hell\": 6}", "{\"format\": 1, \"gruen\": \"x\"}"]
    {
        std::fs::write(&path, foreign).unwrap();
        assert_eq!(read_counts(&path), scan::ByVariant::default(), "{foreign}");
    }
}

#[test]
fn a_frame_with_both_grey_images_goes_through_the_file_and_an_old_one_with_the_green_image_still_reads() {
    let (number, crop) = small_crop();
    let bright = Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(9)).collect());
    let frame = Frame {
        image: bright.clone(),
        variant: Some(Variant::Bright),
        images: Images { green: Some(crop.clone()), bright: Some(bright.clone()) },
        value: Some(number),
        number: 3,
    };
    assert_eq!(frame_of(&frame_bytes(&frame)), Some(frame.clone()));
    // Only one grey image present.
    let single = Frame { images: Images { green: None, bright: Some(bright.clone()) }, ..frame.clone() };
    assert_eq!(frame_of(&frame_bytes(&single)), Some(single));
    // A grey image of another size is left out.
    let odd = Frame { images: Images { green: Some(Gray::new(2, 2, vec![1; 4])), bright: Some(bright) }, ..frame };
    let read = frame_of(&frame_bytes(&odd)).expect("frame");
    assert_eq!((read.images.green, read.variant), (None, Some(Variant::Bright)));

    // The first format held the green grey image alone.
    let mut old = b"VKS1".to_vec();
    old.extend_from_slice(&(crop.w as u32).to_le_bytes());
    old.extend_from_slice(&(crop.h as u32).to_le_bytes());
    old.extend_from_slice(&5u64.to_le_bytes());
    old.extend_from_slice(&number.to_le_bytes());
    old.extend_from_slice(&crop.data);
    assert_eq!(frame_of(&old), Some(Frame::new(crop, Some(number), 5)));
}

#[test]
fn the_first_sure_find_keeps_the_place_and_a_watch_with_the_place_reads_only_around_it() {
    let game = (0, 0, 1000, 700);
    // Without a place: the middle is read, the find is reported with the size of the window.
    let (canvas, areas, number) = scene();
    let watch = Watch::start(Box::new(canvas), files_in("lage-1"), fast(), Hint::default(), || {}).expect("thread");
    let state = wait_until(&watch, |s| s.place.is_some()).expect("a place is reported");
    watch.stop();
    assert_eq!(state.shown, Some(number));
    let (key, place) = state.place.expect("place");
    assert_eq!(key, "1000x700");
    let (_, crop) = small_crop();
    let (right, bottom) = (420 + crop.w as i32, 300 + crop.h as i32);
    assert!(
        place[0] < right && place[0] + place[2] > 420 && place[1] < bottom && place[1] + place[3] > 300,
        "{place:?}"
    );
    assert!(areas.lock().unwrap().iter().any(|a| a.2 > 400), "the middle was read at first");

    // With the place kept: only a narrow area around it is grabbed, never the middle.
    let (canvas, areas, number) = scene();
    let hint = Hint::default().with_place(&key, place);
    let narrow = hint.narrow(game).expect("narrow").0;
    let watch = Watch::start(Box::new(canvas), files_in("lage-2"), fast(), hint, || {}).expect("thread");
    let shown = wait_until(&watch, |s| s.shown.is_some()).expect("shown");
    assert_eq!(shown.shown, Some(number));
    std::thread::sleep(Duration::from_millis(300));
    watch.stop();
    for a in areas.lock().unwrap().iter() {
        assert!(
            a.0 >= narrow.0 && a.1 >= narrow.1 && a.0 + a.2 <= narrow.0 + narrow.2 && a.1 + a.3 <= narrow.1 + narrow.3,
            "{a:?} lies beyond {narrow:?}"
        );
    }
    // The place found again does not make a new report.
    assert_eq!(shown.place, None);
}

#[test]
fn a_frame_the_player_drew_is_the_only_place_that_is_read() {
    let game = (0, 0, 1000, 700);
    // The frame holds the pill (per mille: 400..700 of 1000, 380..620 of 700 points).
    let (canvas, areas, number) = scene();
    let hint = Hint { frame: Some([400, 380, 300, 300]), places: Vec::new() };
    let frame = hint.narrow(game).expect("frame");
    assert!(frame.1, "the player drew it");
    let watch = Watch::start(Box::new(canvas), files_in("rahmen-1"), times(0.2), hint, || {}).expect("thread");
    let shown = wait_until(&watch, |s| s.shown.is_some()).expect("shown");
    assert_eq!(shown.shown, Some(number));
    std::thread::sleep(Duration::from_millis(600));
    watch.stop();
    let inside = |a: &Area| {
        a.0 >= frame.0.0 && a.1 >= frame.0.1 && a.0 + a.2 <= frame.0.0 + frame.0.2 && a.1 + a.3 <= frame.0.1 + frame.0.3
    };
    assert!(areas.lock().unwrap().iter().all(inside), "{:?} against {:?}", areas.lock().unwrap(), frame.0);

    // A frame beside the pill: nothing is read of the pill, however long it takes, and the middle is never
    // searched although nothing comes (the time for it is short here).
    let (canvas, areas, _) = scene();
    let hint = Hint { frame: Some([0, 0, 200, 200]), places: Vec::new() };
    let frame = hint.narrow(game).expect("frame");
    let watch = Watch::start(Box::new(canvas), files_in("rahmen-2"), times(0.1), hint, || {}).expect("thread");
    assert!(wait_until(&watch, |s| s.shown.is_some()).is_none(), "the pill lies outside the frame");
    watch.stop();
    assert!(!areas.lock().unwrap().is_empty());
    assert!(areas.lock().unwrap().iter().all(inside_of(frame.0)));
}

fn inside_of(frame: Area) -> impl Fn(&Area) -> bool {
    move |a| a.0 >= frame.0 && a.1 >= frame.1 && a.0 + a.2 <= frame.0 + frame.2 && a.1 + a.3 <= frame.1 + frame.3
}

#[test]
fn a_place_that_no_longer_holds_the_pill_is_given_up_for_a_look_at_the_middle_once_the_time_is_up() {
    // The place kept lies elsewhere: after `lost_after` the middle is searched and the pill found again, and
    // the new place is reported.
    let (canvas, areas, number) = scene();
    let hint = Hint::default().with_place("1000x700", [10, 10, 120, 60]);
    let watch = Watch::start(Box::new(canvas), files_in("lage-3"), times(0.4), hint, || {}).expect("thread");
    let state = wait_until(&watch, |s| s.place.is_some()).expect("a new place is reported");
    watch.stop();
    assert_eq!(state.shown, Some(number));
    assert_eq!(state.place.unwrap().0, "1000x700");
    let wide = areas.lock().unwrap().iter().filter(|a| a.2 > 400).count();
    assert!(wide >= 1, "the middle was read");
}

/// The look at the middle beside a place that does not hold the pill any more must happen when the tracker
/// searches, not at a beat it skips. The beats come every 0.7 s, so that the 20 s are up at a beat the tracker
/// (a whole search every 2 s without a pill) skips.
#[test]
fn the_look_at_the_middle_beside_a_wrong_place_is_not_used_up_by_a_beat_the_search_skips() {
    let game = (0, 0, 1000, 700);
    let hint = Hint::default().with_place("1000x700", [10, 10, 120, 60]);
    let (mut aim, mut tracker) = (Aim::new(), Tracker::new());
    let grabbed = std::cell::Cell::new((0, 0, 0, 0));
    let mut now = 0.0;
    let mut found_at = None;
    while now < 60.0 && found_at.is_none() {
        let mut grab = |area: Area| {
            grabbed.set(area);
            Ok(vec![0u8; 4])
        };
        // The pill stands in the middle only.
        let mut search = |_: &[u8], _: i32, _: i32| {
            if grabbed.get().2 > 400 { found(Some(2000), Some((420, 300, 120, 60))) } else { found(None, None) }
        };
        let look = look_once(&mut aim, &mut tracker, &hint, Some(game), now, 20.0, &mut grab, &mut search);
        if look.expect("look").is_some_and(|l| l.found.reading.value.is_some()) {
            found_at = Some(now);
        }
        now += 0.7;
    }
    let found_at = found_at.expect("found in the middle");
    assert!((20.0..24.0).contains(&found_at), "found at {found_at} s, the middle is due at 20 s");
}

#[test]
fn a_frame_that_does_not_fit_the_window_reads_nothing_instead_of_the_middle() {
    let game = (0, 0, 1000, 700);
    let aim = Aim::new();
    // 10 per mille of 700 points are 7 points: too little to read, and not a frame to replace by the middle.
    let tiny = Hint { frame: Some([400, 380, 10, 10]), places: Vec::new() };
    assert!(tiny.narrow(game).is_none());
    assert_eq!(aim.pick(&tiny, Some(game), 100.0, 20.0), (None, false));
    // Neither frame nor place: the middle, as before.
    assert_eq!(aim.pick(&Hint::default(), Some(game), 0.0, 20.0), (search_area(Some(game)), false));
    // The same through the thread: no grab at all, not even of the middle, while the beats go on.
    let (canvas, areas, _) = scene();
    let watch = Watch::start(Box::new(canvas), files_in("rahmen-klein"), fast(), tiny, || {}).expect("thread");
    wait_until(&watch, |s| s.beats >= 4).expect("the beats go on");
    watch.stop();
    assert!(areas.lock().unwrap().is_empty(), "{:?}", areas.lock().unwrap());
}

/// A source whose grab fails once `fail` is set.
struct Failing {
    inner: Canvas,
    fail: Arc<AtomicBool>,
}

impl Source for Failing {
    fn game(&mut self) -> Option<Area> {
        self.inner.game()
    }
    fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError> {
        if self.fail.load(Ordering::Relaxed) { Err(GrabError::Failed) } else { self.inner.grab(area) }
    }
}

#[test]
fn a_grab_that_keeps_failing_lets_the_number_run_down_instead_of_standing_on() {
    let (canvas, _, number) = scene();
    let fail = Arc::new(AtomicBool::new(false));
    let source = Failing { inner: canvas, fail: fail.clone() };
    let watch =
        Watch::start(Box::new(source), files_in("grab-fehler"), fast(), Hint::default(), || {}).expect("thread");
    let shown = wait_until(&watch, |s| s.shown.is_some()).expect("shown");
    assert_eq!(shown.shown, Some(number));
    fail.store(true, Ordering::Relaxed);
    let cleared = wait_until(&watch, |s| s.shown.is_none());
    watch.stop();
    assert!(cleared.is_some(), "the number stays although nothing can be read any more");
}

#[test]
fn a_thread_that_panicked_with_the_state_locked_does_not_end_the_reading() {
    let (canvas, _, number) = scene();
    let watch = Watch::start(Box::new(canvas), files_in("vergiftet"), fast(), Hint::default(), || {}).expect("thread");
    let shared = watch.shared.clone();
    let poisoned = std::thread::spawn(move || {
        let _held = shared.state.lock().unwrap();
        let _held_hint = shared.hint.lock().unwrap();
        panic!("a panic with both locks held");
    })
    .join();
    assert!(poisoned.is_err());
    assert!(watch.shared.state.is_poisoned() && watch.shared.hint.is_poisoned());
    let state = wait_until(&watch, |s| s.shown.is_some());
    watch.stop();
    assert_eq!(state.and_then(|s| s.shown), Some(number), "the reading goes on");
}

// ------------------------------------------------------------ unsure pictures

#[test]
fn a_row_of_digits_that_cannot_be_read_sure_is_reported_once_with_its_picture() {
    // 15,420 is no possible value without raw materials in the data (the file is missing): the row is found,
    // the number is not.
    let (canvas, _areas, _) = scene_of(7);
    let watch = Watch::start(Box::new(canvas), files_in("unsicher-1"), fast(), Hint::default(), || {}).expect("thread");
    let state = wait_until(&watch, |s| s.unsure.is_some()).expect("an unsure picture is reported");
    let first = state.unsure.expect("picture");
    assert_eq!(first.value, None);
    assert!(first.image.w > 0 && first.number >= 1);
    // Never shown, never a value, and the same picture is not reported again.
    std::thread::sleep(Duration::from_millis(400));
    let later = watch.state();
    watch.stop();
    assert_eq!(later.shown, None);
    assert_eq!(later.unsure.map(|u| u.number), Some(first.number));
}

#[test]
fn a_row_of_digits_without_the_location_symbol_offers_no_picture_for_teaching() {
    // Digits without a signature box are read when the number is a known value, but neither the last picture
    // nor an unsure one is kept from them: the teaching never gets a label or a distance.
    let (canvas, _areas, number) = scene_without_symbol();
    let watch =
        Watch::start(Box::new(canvas), files_in("ohne-symbol"), fast(), Hint::default(), || {}).expect("thread");
    // Waits for three looks instead of a fixed time: under load a fixed wait can pass without a single look.
    let looked = wait_until(&watch, |s| s.beats >= 3);
    let state = watch.state();
    watch.stop();
    assert!(looked.is_some(), "the watch has looked: {state:?}");
    assert!(state.frame.is_none(), "{:?} (number {number}) {:?}", state.frame.map(|f| f.number), state.shown);
    assert!(state.unsure.is_none());
    // With the symbol in place the same scene gives the picture.
    let (canvas, _areas, _) = scene_of(5);
    let watch = Watch::start(Box::new(canvas), files_in("mit-symbol"), fast(), Hint::default(), || {}).expect("thread");
    let state = wait_until(&watch, |s| s.frame.is_some() || s.unsure.is_some());
    watch.stop();
    assert!(state.is_some(), "a picture with the symbol is kept");
}

#[test]
fn a_number_read_sure_is_not_reported_as_unsure() {
    let (canvas, _areas, number) = scene();
    let watch = Watch::start(Box::new(canvas), files_in("unsicher-2"), fast(), Hint::default(), || {}).expect("thread");
    let state = wait_until(&watch, |s| s.shown.is_some()).expect("shown");
    std::thread::sleep(Duration::from_millis(300));
    let later = watch.state();
    watch.stop();
    assert_eq!(state.shown, Some(number));
    assert!(later.unsure.is_none());
}

#[test]
fn the_pictures_kept_are_newest_first_not_twice_the_same_limited_and_can_be_let_go() {
    let (_, crop) = small_crop();
    let folder = testdir::fresh("gemerkt").to_string_lossy().into_owned();
    assert!(pending(&folder).is_empty(), "no folder yet");
    let picture = |shade: u8| {
        let image = Gray::new(crop.w, crop.h, crop.data.iter().map(|v| v.saturating_add(shade)).collect());
        Frame::new(image, None, u64::from(shade))
    };
    assert!(keep_pending(&folder, &picture(1)));
    assert!(!keep_pending(&folder, &picture(1)), "the same picture twice");
    std::thread::sleep(Duration::from_millis(3));
    assert!(keep_pending(&folder, &picture(2)));
    let all = pending(&folder);
    assert_eq!(all.iter().map(|(_, f)| f.number).collect::<Vec<_>>(), [2, 1], "newest first");
    drop_pending(&all[0].0);
    assert_eq!(pending(&folder).len(), 1);
    for shade in 3..(3 + PENDING_LIMIT as u8 + 4) {
        std::thread::sleep(Duration::from_millis(3));
        assert!(keep_pending(&folder, &picture(shade)));
    }
    let kept = pending(&folder);
    assert_eq!(kept.len(), PENDING_LIMIT);
    assert_eq!(kept[0].1.number, u64::from(3 + PENDING_LIMIT as u8 + 3), "the newest stays");
}
