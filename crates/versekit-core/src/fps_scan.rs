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

//! The frame rates the game writes into its picture with `r_DisplayInfo` (1 to 4), read from the screen: the own
//! ones (`FPS 40.6 [26..47], - 25.7ms`) and the ones of the server (`Server FPS 19.5 [18.4..20.5]`). The block
//! stands right-aligned at the top right of the picture, in yellow and orange letters on the dark picture.
//!
//! | Rule | Why |
//! |---|---|
//! | Off by default | grabbing the screen without consent would be a breach of trust |
//! | Only while Star Citizen is in front | otherwise it photographs the desktop |
//! | One fixed part at the top right, every few seconds | the reading runs beside the game and takes its computing time |
//! | The recognition only where lines of yellow or orange writing stand | without the block nothing is read and nothing is said |
//!
//! The way of the refinery scanner: the picture comes from [`crate::screen_grab`], the text recognition of Windows
//! ([`crate::win_ocr`]) reads it, [`crate::refinery_scan::rows`] puts the words into lines.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::refinery_scan::{Job, Mode, Ocr, ScanError, Word, rows};
use crate::signature_watch::Source;
use crate::win_ocr::{Bitmap, Recognition};

/// How often the part is looked at.
pub const EVERY: Duration = Duration::from_secs(3);
/// How long a reading stands without a new one (the game in the background, a reading that failed).
pub const FRESH: Duration = Duration::from_secs(10);
/// The longest one reading of the recognition may take.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The part that is grabbed: from this share of the width to the right edge, from the top edge this share down.
const AREA_LEFT: f64 = 0.5;
const AREA_HEIGHT: f64 = 0.4;
/// Points of writing a row needs to count as a row of a line, and the largest share of the width it may fill
/// (a yellow sky is no writing).
const ROW_MIN_POINTS: usize = 4;
const ROW_MAX_SHARE: f64 = 0.35;
/// A line is at least so high and at most so high, in points.
const LINE_MIN_HEIGHT: i64 = 4;
const LINE_MAX_HEIGHT: i64 = 80;
/// The block is right-aligned: a line of it ends in this share of the width on the right.
const LINE_RIGHT_SHARE: f64 = 0.8;
/// So many lines make a block; the reading takes at most so many from the top.
const MIN_LINES: usize = 2;
const MAX_LINES: usize = 16;
/// The letters are enlarged to about this height for the recognition.
const LETTER_HEIGHT: f64 = 36.0;

/// What the block says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// The frames per second of the own game.
    pub fps: f64,
    /// The frames per second of the server, where the block names them.
    pub server: Option<f64>,
    /// The players on the shard and on the own server, from `pop. [shard 482, srv 37]` (level 3 and up).
    pub players: Option<Players>,
}

/// How many players the block names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Players {
    pub shard: u32,
    /// On the server the player is on; `None` where the line was cut before it.
    pub server: Option<u32>,
}

/// Can this system read the block at all (Windows with its text recognition)?
pub fn supported() -> bool {
    crate::screen_grab::supported() && crate::win_ocr::supported()
}

/// The part of the game window that is grabbed: the upper right corner.
pub fn area(game: crate::screen_grab::Area) -> crate::screen_grab::Area {
    let (x, y, w, h) = game;
    let left = x + (f64::from(w) * AREA_LEFT) as i32;
    (left, y, x + w - left, ((f64::from(h) * AREA_HEIGHT) as i32).max(1))
}

/// Is the point (blue, green, red) yellow or orange writing? Red strong, green at least half way, blue weak.
pub fn is_writing(b: u8, g: u8, r: u8) -> bool {
    r >= 160 && g >= 80 && i32::from(r) - i32::from(b) >= 100
}

/// One line of writing in the picture, in points: top and bottom row (bottom inclusive), left and right end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub top: i64,
    pub bottom: i64,
    pub left: i64,
    pub right: i64,
}

impl Line {
    fn height(&self) -> i64 {
        self.bottom - self.top + 1
    }
}

/// The lines of yellow or orange writing that end at the right edge, top to bottom. A row with a gap of one row
/// in between still belongs to the line (the dot over an i).
pub fn lines(picture: &Bitmap) -> Vec<Line> {
    let (width, height) = (picture.width.max(0) as usize, picture.height.max(0) as usize);
    if picture.raw.len() < width * height * 4 || width == 0 {
        return Vec::new();
    }
    let most = (width as f64 * ROW_MAX_SHARE) as usize;
    let mut found: Vec<Line> = Vec::new();
    let mut open: Option<(Line, i64)> = None;
    for y in 0..height {
        let row = &picture.raw[y * width * 4..(y + 1) * width * 4];
        let (mut count, mut left, mut right) = (0usize, usize::MAX, 0usize);
        for (x, point) in row.as_chunks::<4>().0.iter().enumerate() {
            if is_writing(point[0], point[1], point[2]) {
                count += 1;
                left = left.min(x);
                right = right.max(x);
            }
        }
        let y = y as i64;
        if (ROW_MIN_POINTS..=most).contains(&count) {
            open = Some(match open {
                Some((line, last)) if y - last <= 2 => (
                    Line { bottom: y, left: line.left.min(left as i64), right: line.right.max(right as i64), ..line },
                    y,
                ),
                before => {
                    found.extend(before.map(|(line, _)| line));
                    (Line { top: y, bottom: y, left: left as i64, right: right as i64 }, y)
                }
            });
        }
    }
    found.extend(open.map(|(line, _)| line));
    let edge = (width as f64 * LINE_RIGHT_SHARE) as i64;
    found.retain(|line| (LINE_MIN_HEIGHT..=LINE_MAX_HEIGHT).contains(&line.height()) && line.right >= edge);
    found
}

/// The reading the recognition is asked for: the first lines of the block, enlarged so that the letters are about
/// [`LETTER_HEIGHT`] high, dark writing on a bright ground. `None` without a block.
pub fn job(found: &[Line], width: i64) -> Option<Job> {
    if found.len() < MIN_LINES {
        return None;
    }
    let taken = &found[..found.len().min(MAX_LINES)];
    let mut heights: Vec<i64> = taken.iter().map(Line::height).collect();
    heights.sort_unstable();
    let letter = heights[heights.len() / 2];
    let pad = letter;
    let left = (taken.iter().map(|l| l.left).min()? - pad).max(0);
    let top = (taken[0].top - pad).max(0);
    let bottom = taken[taken.len() - 1].bottom + pad;
    let scale = (LETTER_HEIGHT / letter as f64).clamp(1.0, 4.0);
    Some(Job { area: (left, top, (width - left).max(1), bottom - top + 1), scale, mode: Mode::Dark })
}

/// The text lines of the words, top to bottom.
pub fn text_lines(words: &[Word]) -> Vec<String> {
    rows(words).iter().map(|row| row.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ")).collect()
}

/// `Server FPS 18.4`; the recognition drops the `FPS` at times (`Server 17.4`), a `Server-Time` is no rate.
static SERVER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)server\s*(?:fps)?\s*:?\s+([0-9OoIl]{1,4}(?:[.,][0-9OoIl]{1,2})?)").expect("pattern")
});
/// `pop. [shard 482, srv 37]` in the line `Server: …`, with the letters the recognition mistakes for digits: the
/// number after `shard`, then the number right before the closing bracket, however `, srv` was read.
static PLAYERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)pop.{0,8}?shard\s*:?\s*([0-9OoIl]{1,4})(?:[^\])0-9]{1,12}([0-9][0-9OoIl]{0,4})\s*([\])|])?)?")
        .expect("pattern")
});
static OWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z])fps\s*:?\s*([0-9OoIl]{1,4}(?:[.,][0-9OoIl]{1,2})?)").expect("pattern")
});

/// A number the recognition read, with the letters it mistakes for digits put right; `None` without a real digit.
fn number(text: &str) -> Option<f64> {
    if !text.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let plain: String = text
        .chars()
        .map(|c| match c {
            'O' | 'o' => '0',
            'I' | 'l' => '1',
            ',' => '.',
            c => c,
        })
        .collect();
    plain.parse().ok()
}

/// The frame rates of the lines: the own ones from the line with `FPS` and without `Server`, the ones of the server
/// from the line `Server FPS`; the players from `pop. [shard …, srv …]`. Values no game reaches do not count.
/// `None` without the own frame rates.
pub fn parse(lines: &[String]) -> Option<Reading> {
    let (mut fps, mut server) = (None, None);
    // The line may come split in two by the recognition: the counts are looked for in all lines together.
    let all = lines.join(" ");
    let players = PLAYERS.captures(&all).and_then(|c| {
        let count = |text: &str| number(text).filter(|v| *v < 10_000.0);
        let shard = count(c.get(1)?.as_str()).filter(|v| *v >= 1.0)? as u32;
        // The closing bracket is read as a 1 at times (`srv 461` for `srv 46]`): without a bracket after it, a
        // last 1 of a longer number is the bracket.
        let on_server = c.get(2).map(|m| {
            let text = m.as_str();
            match text.char_indices().last() {
                Some((at, '1' | 'l' | 'I')) if c.get(3).is_none() && at > 0 => &text[..at],
                _ => text,
            }
        });
        Some(Players { shard, server: on_server.and_then(count).map(|v| v as u32).filter(|v| *v <= shard) })
    });
    for line in lines {
        let lower = line.to_lowercase();
        if lower.contains("server") {
            if server.is_none() {
                server = SERVER.captures(line).and_then(|c| number(&c[1])).filter(|v| *v > 0.0 && *v < 1000.0);
            }
        } else if fps.is_none() {
            fps = OWN.captures(line).and_then(|c| number(&c[1])).filter(|v| *v >= 1.0 && *v < 1000.0);
        }
    }
    Some(Reading { fps: fps?, server, players })
}

/// What one look at the part found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    /// No block of writing there: nothing is shown.
    Nothing,
    /// A block, but the frame rates could not be read from it.
    Unread,
    Read(Reading),
}

/// Reads the words of a picture for the jobs; the text recognition of Windows, or a made-up one in the checks.
pub trait Reader: Send {
    fn read(&mut self, picture: &Bitmap, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError>;
}

/// The text recognition of Windows.
pub struct System;

impl Reader for System {
    fn read(&mut self, picture: &Bitmap, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        Recognition { bitmap: picture, timeout: READ_TIMEOUT }.read(jobs)
    }
}

/// One look at a grabbed part: the recognition is asked only where a block of writing stands.
pub fn look(picture: &Bitmap, reader: &mut dyn Reader) -> Look {
    look_lines(picture, reader).0
}

/// [`look`], with the text lines the recognition gave.
fn look_lines(picture: &Bitmap, reader: &mut dyn Reader) -> (Look, Vec<String>) {
    let Some(job) = job(&lines(picture), picture.width) else { return (Look::Nothing, Vec::new()) };
    let words = match reader.read(picture, &[job]) {
        Ok(words) => words.into_iter().flatten().collect::<Vec<_>>(),
        Err(_) => return (Look::Unread, Vec::new()),
    };
    let text = text_lines(&words);
    (parse(&text).map_or(Look::Unread, Look::Read), text)
}

/// Names a file that gets the text lines of every look, for finding out what the recognition reads (a test run
/// sets it; nothing is written without it).
pub const ENV_LINES_FILE: &str = "VERSEKIT_FPS_ZEILEN";

// ------------------------------------------------------------ the thread

struct Shared {
    last: Mutex<Option<(Reading, Instant)>>,
    stop: AtomicBool,
}

/// The parts of a reading the recognition misses now and then (the rate of the server, the players), kept from the
/// last look that had them while they are fresh, so that they do not flicker.
#[derive(Default)]
struct Kept {
    server: Option<(f64, Instant)>,
    players: Option<(Players, Instant)>,
}

impl Kept {
    fn fill(&mut self, mut reading: Reading, now: Instant) -> Reading {
        let fresh = |at: Instant| now.duration_since(at) <= FRESH;
        match reading.server {
            Some(server) => self.server = Some((server, now)),
            None => reading.server = self.server.filter(|(_, at)| fresh(*at)).map(|(server, _)| server),
        }
        let before = self.players.filter(|(_, at)| fresh(*at)).map(|(players, _)| players);
        let read = reading.players;
        reading.players = match (read, before) {
            // The count of the server read before stays with the count of the shard read now.
            (Some(now_read), Some(before)) if now_read.server.is_none() => Some(Players {
                server: before.server.filter(|on_server| *on_server <= now_read.shard),
                ..now_read
            }),
            (Some(now_read), _) => Some(now_read),
            (None, before) => before,
        };
        // Only what this look read counts as fresh.
        if let (Some(_), Some(players)) = (read, reading.players) {
            self.players = Some((players, now));
        }
        reading
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The running watch.
pub struct Watch {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    /// Starts the thread: every `every` one look, while the game is in front. `wake` is called when what is shown
    /// changes.
    pub fn start(
        mut source: Box<dyn Source>,
        mut reader: Box<dyn Reader>,
        every: Duration,
        wake: impl Fn() + Send + 'static,
    ) -> Option<Watch> {
        let shared = Arc::new(Shared { last: Mutex::new(None), stop: AtomicBool::new(false) });
        let inner = shared.clone();
        let thread = std::thread::Builder::new()
            .name("fps-wache".into())
            .spawn(move || run_loop(&inner, source.as_mut(), reader.as_mut(), every, &wake))
            .ok()?;
        Some(Watch { shared, thread: Some(thread) })
    }

    /// The last reading while it is fresh.
    pub fn reading(&self) -> Option<Reading> {
        lock(&self.shared.last).filter(|(_, at)| at.elapsed() <= FRESH).map(|(reading, _)| reading)
    }

    pub fn alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
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

fn run_loop(shared: &Shared, source: &mut dyn Source, reader: &mut dyn Reader, every: Duration, wake: &dyn Fn()) {
    let lines_file = std::env::var(ENV_LINES_FILE).ok().filter(|p| !p.is_empty());
    let mut kept = Kept::default();
    while !shared.stop.load(Ordering::Relaxed) {
        let began = Instant::now();
        if let Some(game) = source.game() {
            let part = area(game);
            if let Ok(raw) = source.grab(part) {
                let picture = Bitmap { width: i64::from(part.2), height: i64::from(part.3), raw };
                let before = lock(&shared.last).map(|(reading, _)| reading);
                let (seen, text) = look_lines(&picture, reader);
                if let Some(file) = &lines_file
                    && !text.is_empty()
                {
                    let _ = std::fs::write(file, text.join("\n"));
                }
                match seen {
                    Look::Nothing => *lock(&shared.last) = None,
                    Look::Unread => {}
                    Look::Read(reading) => {
                        let now = Instant::now();
                        *lock(&shared.last) = Some((kept.fill(reading, now), now));
                    }
                }
                if lock(&shared.last).map(|(reading, _)| reading) != before {
                    wake();
                }
            }
        }
        while began.elapsed() < every && !shared.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50).min(every));
        }
    }
}

#[cfg(test)]
mod tests;
