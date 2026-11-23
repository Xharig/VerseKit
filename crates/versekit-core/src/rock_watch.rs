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

//! The watch that reads the panel of the scanned rock in the background ([`crate::rock_scan`]).
//!
//! | Rule | Why |
//! |---|---|
//! | Runs only while the signature watch runs | that one holds the consent to grab the screen |
//! | Searches the whole game picture only while mining and at most every [`SEARCH_EVERY_S`] | the text recognition over the whole picture costs computing time beside the game |
//! | After a find only the part of the panel ([`panel_area`]) | a few readings of a small part are cheap |
//! | The part is forgotten after [`LOST_BEATS`] beats without the panel | the panel stands elsewhere in another ship |
//!
//! The recognition comes in as a function ([`Reader`]): the checks give made-up words, the program the
//! recognition of Windows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::refinery_scan::{self as refinery, Job, Mode, ScanError, Word};
use crate::rock_scan::{self as scan, Panel, Read, Steady, Sure};
use crate::signature_scan::Gray;
use crate::signature_watch::Source;
use crate::win_ocr::Bitmap;

/// One beat of the watch, in seconds.
pub const INTERVAL_S: f64 = 1.0;
/// The whole game picture is searched at most this often.
pub const SEARCH_EVERY_S: f64 = 8.0;
/// So many beats without the panel in its part, then it is searched again.
pub const LOST_BEATS: u32 = 4;
/// The search enlarges the game picture so that it is about this high: the letters of the panel (about 15 points
/// on 1440 lines) then stand about 22 high, enough for the recognition, and a picture of 32:9 fits one job.
pub const SEARCH_HEIGHT: f64 = 2160.0;
/// The setting with the places the panel was found in, by size of the game window: `{"5120x1440": [x, y, w, h]}`.
pub const PLACES: &str = "stein_feld_lage";

/// The text recognition: for each job a list of words in pixels of the picture.
pub type Reader = Box<dyn FnMut(&Bitmap, &[Job]) -> Result<Vec<Vec<Word>>, ScanError> + Send>;

/// The recognition of the system.
pub fn system_reader() -> Reader {
    Box::new(|bitmap: &Bitmap, jobs: &[Job]| {
        use crate::refinery_scan::Ocr as _;
        crate::win_ocr::Recognition::new(bitmap).read(jobs)
    })
}

/// What the watch holds now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The values that stood in two pictures in a row.
    pub sure: Sure,
    /// The panel is in sight.
    pub seen: bool,
    /// Counts up at every change.
    pub version: u64,
    /// The place the panel was found in last, with the size of the game window (`5120x1440`): to be kept in the
    /// settings ([`PLACES`]), so the next start reads it there without a search.
    pub place: Option<(String, [i64; 4])>,
}

/// The places kept in the settings ([`PLACES`]), by size of the game window.
pub fn places_of(value: Option<&crate::jsonio::Value>) -> Vec<(String, [i64; 4])> {
    use crate::jsonio::Value;
    let Some(Value::Object(map)) = value else { return Vec::new() };
    map.iter()
        .filter_map(|(size, v)| {
            let Value::Array(list) = v else { return None };
            let numbers: Vec<i64> = list
                .iter()
                .filter_map(|n| match n {
                    Value::Int(text) => text.parse().ok(),
                    _ => None,
                })
                .collect();
            let place: [i64; 4] = numbers.try_into().ok()?;
            (place[2] > 0 && place[3] > 0 && place[0] >= 0 && place[1] >= 0).then(|| (size.clone(), place))
        })
        .collect()
}

/// The places as the settings keep them, with `place` for `size` set.
pub fn places_value(mut places: Vec<(String, [i64; 4])>, size: &str, place: [i64; 4]) -> crate::jsonio::Value {
    use crate::jsonio::Value;
    match places.iter_mut().find(|(k, _)| k == size) {
        Some(slot) => slot.1 = place,
        None => places.push((size.to_string(), place)),
    }
    Value::Object(
        places.into_iter().map(|(k, p)| (k, Value::Array(p.iter().map(|n| Value::int(*n)).collect()))).collect(),
    )
}

/// The key of a game window size: `5120x1440`.
pub fn size_key(size: (i64, i64)) -> String {
    format!("{}x{}", size.0, size.1)
}

/// The part of the panel in a picture of `size`, from the words of a search: from its head (`SCAN RESULTS`) or
/// its label of the mass. The panel is about 26 lines of its letters wide and 36 high (with a list of materials).
pub fn panel_area(words: &[Word], size: (i64, i64)) -> Option<refinery::Area> {
    let upper = |w: &Word| w.t.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_uppercase();
    let head = words.iter().find(|w| upper(w) == "SCAN" || upper(w) == "SCANERGEBNISSE");
    let mass = words.iter().find(|w| matches!(upper(w).as_str(), "MASS" | "MASSE"));
    let (x, top, line) = match (head, mass) {
        (Some(head), _) => (head.x, head.y, head.h),
        (None, Some(mass)) => (mass.x, mass.y - 7.0 * mass.h, mass.h),
        _ => return None,
    };
    let line = line.max(6.0);
    let left = (x - 2.0 * line).max(0.0) as i64;
    let top = (top - 2.0 * line).max(0.0) as i64;
    let w = ((26.0 * line) as i64).min(size.0 - left);
    let h = ((36.0 * line) as i64).min(size.1 - top);
    (w > 0 && h > 0).then_some((left, top, w, h))
}

/// The grey picture of a frame of the picture: the brightest channel (the HUD writes bright on dark), with one
/// point of margin.
pub fn crop(bitmap: &Bitmap, frame: (f64, f64, f64, f64)) -> Option<Gray> {
    let left = (frame.0.floor() as i64 - 1).max(0);
    let top = (frame.1.floor() as i64 - 1).max(0);
    let right = ((frame.0 + frame.2).ceil() as i64 + 1).min(bitmap.width);
    let bottom = ((frame.1 + frame.3).ceil() as i64 + 1).min(bitmap.height);
    if right - left < 3 || bottom - top < 5 {
        return None;
    }
    let (w, h) = ((right - left) as usize, (bottom - top) as usize);
    let mut data = Vec::with_capacity(w * h);
    for y in top..bottom {
        for x in left..right {
            let at = ((y * bitmap.width + x) * 4) as usize;
            let point = bitmap.raw.get(at..at + 3)?;
            data.push(point[0].max(point[1]).max(point[2]));
        }
    }
    Some(Gray::new(w, h, data))
}

/// The jobs of one reading of the part `area`.
pub fn reading_jobs(area: refinery::Area) -> Vec<Job> {
    scan::PASSES.iter().map(|(scale, mode)| Job { area, scale: *scale, mode: *mode }).collect()
}

/// One picture read: the readings merged, each 0 and 8 checked on the picture. `None` where no reading saw the
/// panel.
pub fn read_picture(bitmap: &Bitmap, area: refinery::Area, reader: &mut Reader) -> Result<Option<Sure>, ScanError> {
    let passes: Vec<Panel> = reader(bitmap, &reading_jobs(area))?.iter().map(|words| scan::parse(words)).collect();
    if !passes.iter().any(|p| p.found) {
        return Ok(None);
    }
    let merged = scan::merge(&passes);
    let check = |read: &Read| crop(bitmap, read.frame).is_some_and(|gray| scan::zero_or_eight(read, &gray));
    let mut sure = Sure::of(&merged, check);
    sure.with_head = merged.resistance.as_ref().and_then(|r| bright_colour(bitmap, r.frame)).is_some_and(scan::is_head_green);
    Ok(Some(sure))
}

/// The mean colour (red, green, blue) of the bright points of a frame of the picture: those of which the brightest
/// channel is above 150. `None` without any.
pub fn bright_colour(bitmap: &Bitmap, frame: (f64, f64, f64, f64)) -> Option<(f64, f64, f64)> {
    let left = frame.0.floor().max(0.0) as i64;
    let top = frame.1.floor().max(0.0) as i64;
    let right = ((frame.0 + frame.2).ceil() as i64).min(bitmap.width);
    let bottom = ((frame.1 + frame.3).ceil() as i64).min(bitmap.height);
    let (mut sum, mut count) = ((0.0, 0.0, 0.0), 0.0);
    for y in top..bottom {
        for x in left..right {
            let at = ((y * bitmap.width + x) * 4) as usize;
            let Some(point) = bitmap.raw.get(at..at + 3) else { continue };
            let (b, g, r) = (f64::from(point[0]), f64::from(point[1]), f64::from(point[2]));
            if r.max(g).max(b) > 150.0 {
                sum = (sum.0 + r, sum.1 + g, sum.2 + b);
                count += 1.0;
            }
        }
    }
    (count > 0.0).then(|| (sum.0 / count, sum.1 / count, sum.2 / count))
}

/// The jobs of a search over a picture of `width` × `height`: enlarged to [`SEARCH_HEIGHT`] (at least 1), in as
/// few parts side by side as the longest side the recognition takes allows, each overlapping the next by a tenth.
pub fn search_jobs(width: i64, height: i64) -> Vec<Job> {
    let scale = (SEARCH_HEIGHT / height.max(1) as f64).max(1.0);
    let most = ((refinery::OCR_LIMIT / scale) as i64).max(1);
    let parts = (width + most - 1) / most;
    let step = width / parts.max(1);
    let overlap = step / 10;
    (0..parts)
        .map(|i| {
            let x = (i * step - overlap).max(0);
            let w = (step + 2 * overlap).min(width - x).min(most);
            Job { area: (x, 0, w, height), scale, mode: Mode::Plain }
        })
        .collect()
}

/// The whole picture searched for the panel: its part, or `None`.
pub fn search(bitmap: &Bitmap, reader: &mut Reader) -> Result<Option<refinery::Area>, ScanError> {
    let words = refinery::merge_words(&reader(bitmap, &search_jobs(bitmap.width, bitmap.height))?);
    Ok(panel_area(&words, (bitmap.width, bitmap.height)))
}

/// A line of what the rock gives, as the window and the overlay show it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    /// The rock cracks: the line stands in the accent.
    pub good: bool,
}

/// The lines for what the watch holds: the answer whether the rock cracks with `loadout`, and its rough worth by
/// `price` (per SCU of a material). Nothing while no panel was seen; each line only where `show` (answer, worth)
/// asks for it; the worth only where it is known.
pub fn lines(state: &State, loadout: &crate::rock_crack::Loadout, price: impl Fn(&str) -> Option<f64>, show: (bool, bool)) -> Vec<Line> {
    use crate::rock_crack::{self as crack, Rock, Verdict};
    let mut out = Vec::new();
    // Nothing read (no panel, or a chunk not scanned yet): no lines.
    if state.sure.is_empty() {
        return out;
    }
    let lang = crate::language::current();
    if show.0 {
        let rock = Rock { mass: state.sure.mass, resistance: state.sure.resistance, instability: state.sure.instability };
        let verdict =
            if state.sure.with_head { crack::assess_with_head(&rock, loadout) } else { crack::assess(&rock, loadout) };
        let good = matches!(verdict, Verdict::Known(found) if found.band.cracks());
        out.push(Line { text: crack::verdict_line_in(lang, &verdict), good });
    }
    if show.1
        && let Some(scu) = state.sure.scu
        && let Some(worth) = crack::worth(scu, &state.sure.shares, price)
    {
        out.push(Line { text: crack::worth_line_in(lang, worth), good: false });
    }
    out
}

struct Shared {
    state: Mutex<State>,
    stop: AtomicBool,
    /// The player mines now (a signature was seen lately): only then is the whole picture searched.
    mining: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The thread.
pub struct Watch {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts the thread with the places kept in the settings ([`places_of`]); `wake` is called whenever what it
    /// holds changed.
    pub fn start(
        mut source: Box<dyn Source>,
        mut reader: Reader,
        known: Vec<(String, [i64; 4])>,
        wake: impl Fn() + Send + 'static,
    ) -> Option<Watch> {
        let shared =
            Arc::new(Shared { state: Mutex::new(State::default()), stop: AtomicBool::new(false), mining: AtomicBool::new(false) });
        let inner = shared.clone();
        let thread = std::thread::Builder::new()
            .name("stein-wache".into())
            .spawn(move || run_loop(&inner, source.as_mut(), &mut reader, known, &wake))
            .ok()?;
        Some(Watch { shared, thread: Some(thread) })
    }

    /// Whether the player mines now; the whole picture is searched only then.
    pub fn set_mining(&self, on: bool) {
        self.shared.mining.store(on, Ordering::Relaxed);
    }

    pub fn alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn state(&self) -> State {
        lock(&self.shared.state).clone()
    }

    /// Ends the thread and waits for it.
    pub fn stop(mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// What the loop keeps from beat to beat.
#[derive(Default)]
pub struct Beat {
    /// The part of the panel in the game window, and the size of the window it was found in.
    pub area: Option<(refinery::Area, (i64, i64))>,
    pub misses: u32,
    pub searched_at: Option<Instant>,
    pub steady: Steady,
    /// The places kept from earlier finds, by size of the game window ([`PLACES`]).
    pub known: Vec<(String, [i64; 4])>,
    /// The kept place was tried for this size already: once lost there, only a search finds the panel again.
    pub known_tried: Option<String>,
    /// A place a search found, to be kept.
    pub found: Option<(String, [i64; 4])>,
    /// The rock's own resistance read last (plain, not green), with the mass it was read with: the HUD shows the
    /// resistance with the head applied once the mining laser is armed, and the rock of the same mass keeps its own.
    pub raw: Option<(f64, f64)>,
}

impl Beat {
    /// The values that stand with the rock's own resistance where it is known: a plain one is kept, a green one of
    /// a rock of the same mass is replaced by the plain one kept.
    fn with_raw(&mut self, mut sure: Sure) -> Sure {
        match (sure.mass, sure.resistance) {
            (Some(mass), Some(resistance)) if !sure.with_head => self.raw = Some((mass, resistance)),
            (Some(mass), Some(_)) => {
                if let Some((_, own)) = self.raw.filter(|(kept, _)| *kept == mass) {
                    sure.resistance = Some(own);
                    sure.with_head = false;
                }
            }
            _ => {}
        }
        sure
    }
}

impl Beat {
    /// One beat over the game picture `bitmap`: the place kept for its size first, else a search (only `mining`,
    /// not too often); then read the part. Gives the values that stand and whether the panel was seen.
    pub fn step(&mut self, bitmap: &Bitmap, mining: bool, now: Instant, reader: &mut Reader) -> (Sure, bool) {
        let size = (bitmap.width, bitmap.height);
        let key = size_key(size);
        if self.area.is_some_and(|(_, known)| known != size) {
            self.area = None;
        }
        if self.area.is_none() && self.known_tried.as_deref() != Some(key.as_str()) {
            if let Some((_, [x, y, w, h])) = self.known.iter().find(|(k, _)| *k == key) {
                self.area = Some(((*x, *y, *w, *h), size));
                self.misses = 0;
            }
            self.known_tried = Some(key.clone());
        }
        if self.area.is_none() {
            let due = self.searched_at.is_none_or(|at| now.duration_since(at).as_secs_f64() >= SEARCH_EVERY_S);
            if !mining || !due {
                return (self.steady.step(None), false);
            }
            self.searched_at = Some(now);
            match search(bitmap, reader) {
                Ok(Some(area)) => {
                    self.area = Some((area, size));
                    self.misses = 0;
                    let place = [area.0, area.1, area.2, area.3];
                    self.found = Some((key.clone(), place));
                    match self.known.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = place,
                        None => self.known.push((key, place)),
                    }
                }
                _ => return (self.steady.step(None), false),
            }
        }
        let Some((area, _)) = self.area else { return (self.steady.step(None), false) };
        match read_picture(bitmap, area, reader) {
            Ok(Some(sure)) => {
                self.misses = 0;
                let stood = self.steady.step(Some(sure));
                (self.with_raw(stood), true)
            }
            _ => {
                self.misses += 1;
                if self.misses >= LOST_BEATS {
                    self.area = None;
                }
                (self.steady.step(None), false)
            }
        }
    }
}

fn run_loop(shared: &Shared, source: &mut dyn Source, reader: &mut Reader, known: Vec<(String, [i64; 4])>, wake: &dyn Fn()) {
    let mut beat = Beat { known, ..Beat::default() };
    while !shared.stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let (sure, seen) = match source.game() {
            Some(game) => match source.grab(game) {
                Ok(raw) => {
                    let bitmap = Bitmap { width: i64::from(game.2), height: i64::from(game.3), raw };
                    beat.step(&bitmap, shared.mining.load(Ordering::Relaxed), started, reader)
                }
                Err(_) => (beat.steady.step(None), false),
            },
            None => (beat.steady.step(None), false),
        };
        let found = beat.found.take();
        let changed = {
            let mut state = lock(&shared.state);
            let changed = state.sure != sure || state.seen != seen || found.is_some();
            if changed {
                state.sure = sure;
                state.seen = seen;
                if found.is_some() {
                    state.place = found;
                }
                state.version += 1;
            }
            changed
        };
        if changed {
            wake();
        }
        let rest = Duration::from_secs_f64(INTERVAL_S).saturating_sub(started.elapsed());
        let until = Instant::now() + rest;
        while !shared.stop.load(Ordering::Relaxed) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests;
