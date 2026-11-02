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

//! The watch that reads the scan signature in the background.
//!
//! ⚠⚠ By itself and not on a button: to press a button the player has to leave Star Citizen, and then the game
//! switches the mining scanner off. The number would be gone before it is read.
//!
//! | Rule | Why |
//! |---|---|
//! | Only while Star Citizen is in front | otherwise it reads the desktop, a number in a browser would be a hit |
//! | Shown at two equal readings of the last three | the HUD flickers, a single reading can be wrong |
//! | A number that is no possible value needs four of the last five | it comes from the digits alone, nothing in the data backs it, and it also has to push out a possible value shown |
//! | Gone only after `CLEAR_S` without a number | turning the ship takes the pill away for moments |
//! | Off by default | grabbing the screen without consent would be a breach of trust |
//! | After a find only around the pill ([`Tracker`]) | the full search over half the game picture costs many times more |
//! | Without a find, full searches less often | the search runs beside the game and takes its computing time |

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::mining_data::MiningData;
use crate::screen_grab::{Area, GrabError};
use crate::signature_scan::{self as scan, ByVariant, Found, Gray, Images, OwnFiles, Templates, Variant};
use crate::signature_window::{self, Hint};

/// The setting that switches the watch on; at the same time the consent to grab the screen.
pub const SETTING: &str = "signatur_wache";
pub const INTERVAL_S: f64 = 0.5;
/// After this many seconds without a number the display goes: the grace against a reading that drops out for a
/// moment.
pub const CLEAR_S: f64 = 1.5;
/// Without a number for so long in the place known, the middle of the window is searched once.
pub const LOST_S: f64 = 20.0;
/// A picture of an unsure number is reported at most this often, in seconds.
const UNSURE_EVERY_S: f64 = 5.0;
/// After a number was read in the middle, it stays the area for so long.
const WIDE_HOLD_S: f64 = 3.0;
/// A possible value is shown at `NEEDED` equal readings of the last `VOTES`.
pub const VOTES: usize = 3;
pub const NEEDED: usize = 2;
/// A number that is no possible value is shown at `UNKNOWN_NEEDED` equal readings of the last `UNKNOWN_VOTES`.
pub const UNKNOWN_VOTES: usize = 5;
pub const UNKNOWN_NEEDED: usize = 4;
/// Templates and values are read anew this often.
pub const RELOAD_S: f64 = 60.0;

/// The window around the last found pill: so many pill widths and heights to each side, at least so many points.
const TRACK_MARGIN_W: f64 = 3.0;
const TRACK_MARGIN_MIN_X: i32 = 240;
const TRACK_MARGIN_H: f64 = 4.0;
const TRACK_MARGIN_MIN_Y: i32 = 120;
/// So many beats without a value, then the pill counts as gone.
const TRACK_MISSES: u32 = 2;
/// Even with a pill the whole area is searched this often: a more central one wins.
const RECHECK_S: f64 = 5.0;
/// Without a pill, but one seen lately: full search this often.
const FULL_BUSY_S: f64 = 1.0;
/// Long without a pill: full search this often.
const FULL_IDLE_S: f64 = 2.0;
/// So long after the last value counts as "lately".
const IDLE_AFTER_S: f64 = 8.0;

/// What a beat of the vote does with the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Leave it as it is.
    Keep,
    /// Show this number.
    Show(u64),
    /// Clear the display.
    Clear,
}

/// A value read, and whether it is one of the possible values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub value: u64,
    pub known: bool,
}

impl Seen {
    /// `value` read, `values` the possible values.
    pub fn of(value: u64, values: &[u64]) -> Seen {
        Seen { value, known: values.contains(&value) }
    }
}

/// The vote over the last readings (`signature_watch.step`): one beat at a time, without thread and screen.
#[derive(Clone, Debug)]
pub struct Voter {
    history: Vec<Option<u64>>,
    last_seen: f64,
    shown: Option<u64>,
    clear_after: f64,
}

impl Voter {
    pub fn new(clear_after: f64) -> Voter {
        Voter { history: Vec::new(), last_seen: 0.0, shown: None, clear_after }
    }

    /// The number shown now.
    pub fn shown(&self) -> Option<u64> {
        self.shown
    }

    /// One beat. `reading` is the value read, `None` for a beat that read nothing; `searched` is false for a
    /// beat without a search (no game in front, or a beat the search skipped): it casts no vote, only the time
    /// until the display clears runs on. A reading that is a possible value is voted over the last [`VOTES`]
    /// beats, any other over the last [`UNKNOWN_VOTES`], so it also needs the longer run to replace a number shown.
    pub fn step(&mut self, searched: bool, reading: Option<Seen>, now: f64) -> Change {
        if !searched {
            return self.fade(now);
        }
        self.history.push(reading.map(|seen| seen.value));
        let excess = self.history.len().saturating_sub(UNKNOWN_VOTES);
        self.history.drain(..excess);
        if let Some(Seen { value, known }) = reading {
            let (window, needed) = if known { (VOTES, NEEDED) } else { (UNKNOWN_VOTES, UNKNOWN_NEEDED) };
            let recent = &self.history[self.history.len().saturating_sub(window)..];
            if recent.iter().filter(|v| **v == Some(value)).count() >= needed {
                self.last_seen = now;
                if self.shown != Some(value) {
                    self.shown = Some(value);
                    return Change::Show(value);
                }
                return Change::Keep;
            }
        }
        self.fade(now)
    }

    fn fade(&mut self, now: f64) -> Change {
        if self.shown.is_some() && now - self.last_seen > self.clear_after {
            self.shown = None;
            return Change::Clear;
        }
        Change::Keep
    }

    /// The watch stops: the display clears at once.
    pub fn stop(&mut self) -> Change {
        self.history.clear();
        if self.shown.take().is_some() { Change::Clear } else { Change::Keep }
    }
}

/// The area to search in the window of the game (left, top, width, height), or `None`.
///
/// The middle of the game window: where the pill is searched as long as no place is known (the pill itself
/// stands in a fixed place of the ship HUD; where, depends on the ship and the size of the window).
pub fn search_area(game: Option<Area>) -> Option<Area> {
    let (x, y, w, h) = game?;
    let (fx, fy, fw, fh) = scan::SEARCH_AREA;
    let area = (
        x + (f64::from(w) * fx) as i32,
        y + (f64::from(h) * fy) as i32,
        (f64::from(w) * fw) as i32,
        (f64::from(h) * fh) as i32,
    );
    (area.2 > 0 && area.3 > 0).then_some(area)
}

/// What a look of the tracker found.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub found: Found,
    /// Where the crop around the pill lies on the screen.
    pub box_abs: Option<Area>,
}

/// Decides each beat WHERE to search (`signature_watch.Tracker`).
///
/// | Situation | Search |
/// |---|---|
/// | pill found last | only the window around it, the whole area every `RECHECK_S` |
/// | pill gone for `TRACK_MISSES` beats | the whole area, at most every `FULL_BUSY_S` |
/// | no value for `IDLE_AFTER_S` | the whole area, at most every `FULL_IDLE_S` |
#[derive(Debug, Default)]
pub struct Tracker {
    pub boxr: Option<Area>,
    misses: u32,
    last_full: Option<f64>,
    last_hit: Option<f64>,
    pub full_searches: u32,
    pub tracked_searches: u32,
}

impl Tracker {
    pub fn new() -> Tracker {
        Tracker::default()
    }

    /// A clock that jumps back does not hold the full search off.
    fn due(&self, now: f64, every: f64) -> bool {
        self.last_full.is_none_or(|last| now < last || now - last >= every)
    }

    /// The window around the last found pill, cut to `area`.
    fn window(&self, area: Area) -> Option<Area> {
        let (left, top, width, height) = self.boxr?;
        let margin_x = TRACK_MARGIN_MIN_X.max((f64::from(width) * TRACK_MARGIN_W) as i32);
        let margin_y = TRACK_MARGIN_MIN_Y.max((f64::from(height) * TRACK_MARGIN_H) as i32);
        let x0 = area.0.max(left - margin_x);
        let y0 = area.1.max(top - margin_y);
        let x1 = (area.0 + area.2).min(left + width + margin_x);
        let y1 = (area.1 + area.3).min(top + height + margin_y);
        (x1 - x0 >= 8 && y1 - y0 >= 8).then_some((x0, y0, x1 - x0, y1 - y0))
    }

    fn run(
        &mut self,
        rect: Area,
        now: f64,
        grab: &mut dyn FnMut(Area) -> Result<Vec<u8>, GrabError>,
        search: &mut dyn FnMut(&[u8], i32, i32) -> Found,
    ) -> Result<Look, GrabError> {
        let raw = grab(rect)?;
        let found = search(&raw, rect.2, rect.3);
        let box_abs = found.boxr.map(|b| (rect.0 + b.0, rect.1 + b.1, b.2, b.3));
        if found.reading.value.is_some() && box_abs.is_some() {
            self.boxr = box_abs;
            self.misses = 0;
            self.last_hit = Some(now);
        } else if self.boxr.is_some() {
            self.misses += 1;
            if self.misses >= TRACK_MISSES {
                self.boxr = None;
            }
        }
        Ok(Look { found, box_abs })
    }

    /// One beat: searches where the situation says, or not at all (`None`).
    pub fn look(
        &mut self,
        game: Option<Area>,
        now: f64,
        grab: &mut dyn FnMut(Area) -> Result<Vec<u8>, GrabError>,
        search: &mut dyn FnMut(&[u8], i32, i32) -> Found,
    ) -> Result<Option<Look>, GrabError> {
        self.look_in(search_area(game), now, grab, search)
    }

    /// As [`Tracker::look`] in an area given from outside (the place known, or the frame the player drew).
    pub fn look_in(
        &mut self,
        area: Option<Area>,
        now: f64,
        grab: &mut dyn FnMut(Area) -> Result<Vec<u8>, GrabError>,
        search: &mut dyn FnMut(&[u8], i32, i32) -> Found,
    ) -> Result<Option<Look>, GrabError> {
        let Some(area) = area else { return Ok(None) };
        let window = if self.boxr.is_some() { self.window(area) } else { None };
        if self.boxr.is_some() && window.is_none() {
            self.boxr = None;
        }
        if let Some(window) = window
            && !self.due(now, RECHECK_S)
        {
            self.tracked_searches += 1;
            let look = self.run(window, now, grab, search)?;
            // The pill is not in the window any more: it may have jumped, so look in the whole area at once if
            // there is time for it.
            if look.found.reading.value.is_some() || !self.due(now, FULL_BUSY_S) {
                return Ok(Some(look));
            }
        } else if window.is_none() {
            let recent = self.last_hit.is_some_and(|hit| now - hit < IDLE_AFTER_S);
            if !self.due(now, if recent { FULL_BUSY_S } else { FULL_IDLE_S }) {
                return Ok(None);
            }
        }
        self.full_searches += 1;
        self.last_full = Some(now);
        self.run(area, now, grab, search).map(Some)
    }
}

// ------------------------------------------------------------ the last picture, for teaching

/// The file the overlay leaves the last picture with a row of digits in, for the page that teaches. The
/// overlay and the main window are two programs.
pub const LAST_FRAME_FILE: &str = "signatur-letztes-bild.bin";
/// A frame with the grey points of the green grey image only.
const LAST_FRAME_MAGIC_GREEN: &[u8; 4] = b"VKS1";
/// A frame with the grey points of each grey image present.
const LAST_FRAME_MAGIC: &[u8; 4] = b"VKS2";
const NO_VARIANT: u8 = 255;

/// The bytes of a frame: magic, width, height, number, value (`u64::MAX` for none), the grey image shown (0
/// green, 1 bright, 255 none), a mask of the grey images that follow (bit 0 green, bit 1 bright), then the grey
/// points of each of them.
pub fn frame_bytes(frame: &Frame) -> Vec<u8> {
    let mut out = LAST_FRAME_MAGIC.to_vec();
    out.extend_from_slice(&(frame.image.w as u32).to_le_bytes());
    out.extend_from_slice(&(frame.image.h as u32).to_le_bytes());
    out.extend_from_slice(&frame.number.to_le_bytes());
    out.extend_from_slice(&frame.value.unwrap_or(u64::MAX).to_le_bytes());
    out.push(frame.variant.map_or(NO_VARIANT, |v| scan::VARIANTS.iter().position(|x| *x == v).unwrap_or(0) as u8));
    let stored: Vec<(usize, &Gray)> = scan::VARIANTS
        .into_iter()
        .enumerate()
        .filter_map(|(bit, v)| {
            let raster = frame.images.get(v).as_ref()?;
            (raster.w == frame.image.w && raster.h == frame.image.h).then_some((bit, raster))
        })
        .collect();
    out.push(stored.iter().fold(0u8, |mask, (bit, _)| mask | 1 << bit));
    for (_, raster) in stored {
        out.extend_from_slice(&raster.data);
    }
    out
}

/// A frame from its bytes; `None` for anything that is not one.
pub fn frame_of(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() < 28 {
        return None;
    }
    let magic = &bytes[..4];
    if magic != LAST_FRAME_MAGIC && magic != LAST_FRAME_MAGIC_GREEN {
        return None;
    }
    let number_at = |from: usize, size: usize| bytes.get(from..from + size);
    let w = u32::from_le_bytes(number_at(4, 4)?.try_into().ok()?) as usize;
    let h = u32::from_le_bytes(number_at(8, 4)?.try_into().ok()?) as usize;
    let number = u64::from_le_bytes(number_at(12, 8)?.try_into().ok()?);
    let value = u64::from_le_bytes(number_at(20, 8)?.try_into().ok()?);
    let value = (value != u64::MAX).then_some(value);
    let size = w.checked_mul(h)?;
    if w == 0 || h == 0 {
        return None;
    }
    if magic == LAST_FRAME_MAGIC_GREEN {
        let data = bytes.get(28..)?;
        if size != data.len() {
            return None;
        }
        return Some(Frame::new(Gray::new(w, h, data.to_vec()), value, number));
    }
    let shown = *bytes.get(28)?;
    let mask = *bytes.get(29)?;
    let mut images = scan::Images::default();
    let mut at = 30usize;
    for (bit, variant) in scan::VARIANTS.into_iter().enumerate() {
        if mask & (1 << bit) != 0 {
            let data = bytes.get(at..at.checked_add(size)?)?;
            *images.get_mut(variant) = Some(Gray::new(w, h, data.to_vec()));
            at += size;
        }
    }
    if at != bytes.len() {
        return None;
    }
    let variant = scan::VARIANTS.get(usize::from(shown)).copied().filter(|v| images.get(*v).is_some());
    let image = variant
        .and_then(|v| images.get(v).clone())
        .or_else(|| scan::VARIANTS.into_iter().find_map(|v| images.get(v).clone()))?;
    Some(Frame { image, variant, images, value, number })
}

/// Leaves the frame in the file, replacing the one before without ever leaving a half-written file.
pub fn write_last_frame(path: &str, frame: &Frame) -> bool {
    let temp = crate::jsonio::temp_name(path);
    crate::jsonio::write_new(&temp, &frame_bytes(frame)).is_ok() && crate::jsonio::replace_file(&temp, path).is_ok()
}

/// The frame the overlay left last, if there is one.
pub fn read_last_frame(path: &str) -> Option<Frame> {
    frame_of(&std::fs::read(path).ok()?)
}

/// The file the overlay leaves the counts of the grey images in, for the report: the report is made in the main
/// window, the watch runs in the overlay.
pub const COUNTS_FILE: &str = "signatur-graubilder.json";

/// Leaves the counts in the file, replacing the ones before without ever leaving a half-written file.
pub fn write_counts(path: &str, counts: &ByVariant<u64>) -> bool {
    let text = format!("{{\"format\": 1, \"gruen\": {}, \"hell\": {}}}", counts.green, counts.bright);
    let temp = crate::jsonio::temp_name(path);
    crate::jsonio::write_new(&temp, text.as_bytes()).is_ok() && crate::jsonio::replace_file(&temp, path).is_ok()
}

/// The counts the overlay left last; zeros when there is no file or it is no counts file.
pub fn read_counts(path: &str) -> ByVariant<u64> {
    let Ok(data) = crate::jsonio::read_json(path) else { return ByVariant::default() };
    if data.get("format") != Some(&crate::jsonio::Value::int(1)) {
        return ByVariant::default();
    }
    let count = |key: &str| match data.get(key) {
        Some(crate::jsonio::Value::Int(text)) => text.parse().unwrap_or(0),
        _ => 0,
    };
    ByVariant { green: count("gruen"), bright: count("hell") }
}

/// The folder of the pictures kept because the number was not sure: the player teaches them when he has time,
/// nothing opens by itself in the game.
pub const PENDING_FOLDER: &str = "signatur-gemerkt";
/// So many are kept; the oldest go first.
pub const PENDING_LIMIT: usize = 12;

/// The pictures kept, newest first, with the file each one lies in.
pub fn pending(folder: &str) -> Vec<(String, Frame)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".bin"))
        .collect();
    // The names are the time in milliseconds with leading zeros: the order of the names is the order of time.
    names.sort();
    names.reverse();
    names
        .into_iter()
        .filter_map(|name| {
            let path = format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
            read_last_frame(&path).map(|frame| (path, frame))
        })
        .collect()
}

/// Keeps a picture the number of which was not sure; a picture like the newest one kept is not kept again.
pub fn keep_pending(folder: &str, frame: &Frame) -> bool {
    if pending(folder).first().is_some_and(|(_, newest)| newest.image == frame.image) {
        return false;
    }
    if crate::jsonio::make_dirs(folder).is_err() {
        return false;
    }
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = format!("{}/{millis:016}.bin", folder.trim_end_matches(['/', '\\']));
    if !write_last_frame(&path, frame) {
        return false;
    }
    let all = pending(folder);
    for (old, _) in all.iter().skip(PENDING_LIMIT) {
        let _ = crate::jsonio::remove_file(old);
    }
    true
}

/// Lets go of a picture kept (it was taught, or the player does not want it).
pub fn drop_pending(path: &str) {
    let _ = crate::jsonio::remove_file(path);
}

// ------------------------------------------------------------ the thread

/// Where the watch gets its pictures from: the screen, or a made-up one in the checks.
pub trait Source: Send {
    /// The window of the game while it is in front.
    fn game(&mut self) -> Option<Area>;
    fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError>;
}

/// The real screen.
pub struct Screen;

impl Source for Screen {
    fn game(&mut self) -> Option<Area> {
        crate::screen_grab::game_rect()
    }
    fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError> {
        crate::screen_grab::grab_raw(area)
    }
}

/// Where the watch reads its templates and values from.
#[derive(Clone, Debug)]
pub struct Files {
    /// The text of the shipped templates.
    pub shipped: String,
    /// The files of the templates the player taught.
    pub own: OwnFiles,
    /// The mining data (`mining-data.json`).
    pub mining: String,
}

/// The last picture with a row of digits, for teaching.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The crop in the grey image the number was read from; the one of `variant` in `images`.
    pub image: Gray,
    /// The grey image the number was read from.
    pub variant: Option<Variant>,
    /// The crop in every grey image: teaching learns from each of them.
    pub images: Images,
    pub value: Option<u64>,
    /// Counts up with every new picture.
    pub number: u64,
}

impl Frame {
    /// A frame with the green grey image only.
    pub fn new(image: Gray, value: Option<u64>, number: u64) -> Frame {
        let images = Images { green: Some(image.clone()), bright: None };
        Frame { image, variant: Some(Variant::Green), images, value, number }
    }

    /// The frame of what the search found, or `None` when it has no crop.
    fn of(found: &Found, value: Option<u64>, number: u64) -> Option<Frame> {
        Some(Frame { image: found.image.clone()?, variant: found.variant, images: found.images.clone(), value, number })
    }
}

/// What the thread reports.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// The number shown now.
    pub shown: Option<u64>,
    /// Where the crop around the pill lies on the screen, while a number is shown.
    pub pill: Option<Area>,
    /// The window of the game at the last beat that read the number.
    pub game: Option<Area>,
    /// Counts up whenever `shown` or `pill` changed.
    pub version: u64,
    pub frame: Option<Frame>,
    /// Beats done so far.
    pub beats: u64,
    /// The place of the pill found for the first time or somewhere else: the size of the game window and
    /// the box from its corner; to be kept in the settings.
    pub place: Option<(String, [i32; 4])>,
    /// A row of digits was found that could not be read sure: the picture, to be kept (at most one in five
    /// seconds, and not twice the same).
    pub unsure: Option<Frame>,
    /// How often each grey image delivered the number read, over all beats since the start.
    pub variant_counts: ByVariant<u64>,
}

struct Shared {
    state: Mutex<State>,
    hint: Mutex<Hint>,
    stop: AtomicBool,
    reload: AtomicBool,
}

/// The data of a mutex also after a panic of the thread that held it: both are plain values that stay whole, and
/// a watch that fails at every beat after one panic would stay green and never read again.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The running watch.
pub struct Watch {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts the thread. `clear_after` is the grace in seconds before the display goes (`CLEAR_S`); `wake` is
    /// called whenever the number shown or the place of the pill changed.
    pub fn start(
        mut source: Box<dyn Source>,
        files: Files,
        times: Times,
        hint: Hint,
        wake: impl Fn() + Send + 'static,
    ) -> Option<Watch> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            hint: Mutex::new(hint),
            stop: AtomicBool::new(false),
            reload: AtomicBool::new(false),
        });
        let inner = shared.clone();
        let thread = std::thread::Builder::new()
            .name("signatur-wache".into())
            .spawn(move || run_loop(&inner, source.as_mut(), &files, times, &wake))
            .ok()?;
        Some(Watch { shared, thread: Some(thread) })
    }

    /// Where to read: the frame the player drew or the places found, taken at the next beat.
    pub fn set_hint(&self, hint: Hint) {
        *lock(&self.shared.hint) = hint;
    }

    /// Does the thread still run? Switched on is not the same as running: a thread that died on the way would
    /// leave a green eye.
    pub fn alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn state(&self) -> State {
        lock(&self.shared.state).clone()
    }

    /// After teaching: the templates are read at the next beat, not in a minute.
    pub fn reload_templates(&self) {
        self.shared.reload.store(true, Ordering::Relaxed);
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

struct Cache {
    at: Option<Instant>,
    known: ByVariant<Templates>,
    values: Vec<u64>,
}

fn load(files: &Files) -> (ByVariant<Templates>, Vec<u64>) {
    let known = scan::templates_by_variant(&files.shipped, &files.own);
    let values = scan::possible_values(&MiningData::load(&files.mining));
    (known, values)
}

/// The times of the watch, in seconds.
#[derive(Clone, Copy, Debug)]
pub struct Times {
    /// Between two beats.
    pub interval: f64,
    /// The grace before the display goes.
    pub clear_after: f64,
    /// Without a number for so long in the place known, the middle of the window is searched once.
    pub lost_after: f64,
}

impl Default for Times {
    fn default() -> Times {
        Times { interval: INTERVAL_S, clear_after: CLEAR_S, lost_after: LOST_S }
    }
}

/// Where the beats read, and what they remember for it: when a number was last read, when the middle of the
/// window was last searched beside the place known, and until when it is searched after a find there.
#[derive(Clone, Copy, Debug)]
struct Aim {
    last_value: f64,
    last_wide: f64,
    wide_until: f64,
}

impl Aim {
    fn new() -> Aim {
        Aim { last_value: 0.0, last_wide: f64::NEG_INFINITY, wide_until: f64::NEG_INFINITY }
    }

    /// The area to read in this beat and whether it is the middle beside a place known. The frame the player
    /// drew is all that is read: where it does not fit into the window (too small, outside) nothing is read.
    /// Else the place found before, and once in a while the middle of the window beside it, for a few seconds
    /// after a find there too, so that the vote of two equal readings can come about outside the place known;
    /// without a place the middle.
    fn pick(&self, hint: &Hint, game: Option<Area>, now: f64, lost_after: f64) -> (Option<Area>, bool) {
        match game.and_then(|g| hint.narrow(g)) {
            Some((area, true)) => (Some(area), false),
            Some((area, false)) => {
                if now < self.wide_until || (now - self.last_value >= lost_after && now - self.last_wide >= lost_after)
                {
                    (search_area(game), true)
                } else {
                    (Some(area), false)
                }
            }
            None if hint.frame.is_some() => (None, false),
            None => (search_area(game), false),
        }
    }

    /// After the beat: `wide` was asked for, `searched_whole` says the tracker did search the whole area (it may
    /// skip a beat), `read` that a number was read.
    fn done(&mut self, now: f64, wide: bool, searched_whole: bool, read: bool) {
        // A middle that was asked for and not searched stays due: the next beat asks again.
        if wide && searched_whole {
            self.last_wide = now;
        }
        if read {
            self.last_value = now;
            if wide && now >= self.wide_until {
                self.wide_until = now + WIDE_HOLD_S;
            }
        }
    }
}

/// One look of a beat: the area the aim picks, searched by the tracker, and what the aim remembers of it.
#[allow(clippy::too_many_arguments, reason = "the state of a beat, as it is")]
fn look_once(
    aim: &mut Aim,
    tracker: &mut Tracker,
    hint: &Hint,
    game: Option<Area>,
    now: f64,
    lost_after: f64,
    grab: &mut dyn FnMut(Area) -> Result<Vec<u8>, GrabError>,
    search: &mut dyn FnMut(&[u8], i32, i32) -> Found,
) -> Result<Option<Look>, GrabError> {
    let (area, wide) = aim.pick(hint, game, now, lost_after);
    let whole_before = tracker.full_searches;
    let looked = tracker.look_in(area, now, grab, search);
    let read = matches!(&looked, Ok(Some(look)) if look.found.reading.value.is_some());
    aim.done(now, wide, looked.is_ok() && tracker.full_searches > whole_before, read);
    looked
}

fn run_loop(shared: &Shared, source: &mut dyn Source, files: &Files, times: Times, wake: &dyn Fn()) {
    let Times { interval, clear_after, lost_after } = times;
    let start = Instant::now();
    let mut voter = Voter::new(clear_after);
    let mut tracker = Tracker::new();
    let mut aim = Aim::new();
    let mut last_unsure = f64::NEG_INFINITY;
    let mut cache = Cache { at: None, known: ByVariant::default(), values: Vec::new() };
    let mut failures = 0u32;
    let mut frame_number = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        let began = Instant::now();
        let now = start.elapsed().as_secs_f64();
        let beat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cache.at.is_none_or(|at| at.elapsed().as_secs_f64() > RELOAD_S)
                || shared.reload.swap(false, Ordering::Relaxed)
            {
                let (known, values) = load(files);
                cache = Cache { at: Some(Instant::now()), known, values };
            }
            let game = source.game();
            let (known, values) = (&cache.known, &cache.values);
            let hint = lock(&shared.hint).clone();
            // A grab that fails is a beat without a search: the display runs down instead of standing on.
            let (looked, grab_error) = match look_once(
                &mut aim,
                &mut tracker,
                &hint,
                game,
                now,
                lost_after,
                &mut |area| source.grab(area),
                &mut |raw, w, h| scan::search(raw, w, h, known, values, 12),
            ) {
                Ok(looked) => (looked, None),
                Err(error) => (None, Some(error)),
            };
            let reading = looked.as_ref().and_then(|l| l.found.reading.value).map(|value| Seen::of(value, values));
            let change = voter.step(looked.is_some(), reading, now);
            let mut state = lock(&shared.state);
            state.beats += 1;
            let mut changed = false;
            if let Some(look) = &looked
                && !look.found.reading.digits.is_empty()
                && let Some(image) = &look.found.image
                && scan::has_locator(image, &look.found.reading)
            {
                frame_number += 1;
                if let Some(frame) = Frame::of(&look.found, look.found.reading.value, frame_number) {
                    state.frame = Some(frame);
                }
            }
            if let Some(look) = &looked
                && look.found.reading.value.is_some()
                && let Some(variant) = look.found.variant
            {
                *state.variant_counts.get_mut(variant) += 1;
            }
            if let Some(look) = &looked
                && look.found.reading.reason == Some(scan::Reason::Unsure)
                && !look.found.reading.digits.is_empty()
                && now - last_unsure >= UNSURE_EVERY_S
                && let Some(image) = &look.found.image
                && scan::has_locator(image, &look.found.reading)
                && state.unsure.as_ref().is_none_or(|u| u.image != *image)
            {
                last_unsure = now;
                frame_number += 1;
                state.unsure = Frame::of(&look.found, None, frame_number);
                changed = true;
            }
            match change {
                Change::Show(_) | Change::Clear => {
                    state.shown = voter.shown();
                    changed = true;
                }
                Change::Keep => {}
            }
            // The first sure find, or a find elsewhere: the place is kept for this size of the window.
            if let (Change::Show(_), Some(look), Some(game)) = (change, &looked, game)
                && let Some(found) = look.box_abs
            {
                let key = signature_window::size_key(game);
                let place = [found.0 - game.0, found.1 - game.1, found.2, found.3];
                if hint.differs(&key, place) {
                    let mut slot = lock(&shared.hint);
                    *slot = slot.with_place(&key, place);
                    state.place = Some((key, place));
                    changed = true;
                }
            }
            // The place of the pill follows it while a number is shown.
            if state.shown.is_some() {
                if let Some(look) = &looked
                    && look.found.reading.value.is_some()
                    && look.box_abs.is_some()
                    && (look.box_abs != state.pill || game != state.game)
                {
                    state.pill = look.box_abs;
                    state.game = game;
                    changed = true;
                }
            } else if state.pill.is_some() {
                state.pill = None;
                changed = true;
            }
            if changed {
                state.version += 1;
                drop(state);
                wake();
            }
            grab_error.map_or(Ok(()), Err)
        }));
        match beat {
            Ok(Ok(())) => failures = 0,
            Ok(Err(_)) => failures += 1,
            Err(_) => {
                failures += 1;
                if failures == 1 {
                    crate::errors::record("signature_watch.loop", &"panic in the beat");
                }
            }
        }
        // After repeated failures slower, never off: a change to full screen can make the grab fail for a moment.
        let pause = if failures < 5 { interval } else { 2.0 };
        let wait = (pause - began.elapsed().as_secs_f64()).max(0.05);
        let end = Instant::now() + Duration::from_secs_f64(wait);
        while Instant::now() < end && !shared.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20).min(end.saturating_duration_since(Instant::now())));
        }
    }
    let mut state = lock(&shared.state);
    if voter.stop() == Change::Clear || state.shown.is_some() {
        state.shown = None;
        state.pill = None;
        state.version += 1;
        drop(state);
        wake();
    }
}

#[cfg(test)]
mod tests;
