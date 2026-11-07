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

//! The page "Contested Zones" (new in v5): the cycle of the Executive Hangar and the self timers of the
//! keycards and compboards.
//!
//! The hangar runs in a cycle of 11 100 seconds from a start `S` (Unix seconds): 7 200 s closed (five lamps
//! turn green one after the other, every 24 minutes), 3 600 s open (a lamp goes out every 12 minutes), 300 s
//! resetting. The remaining seconds are those of whole seconds, as contestedzonetimers.com counts them.
//!
//! `S` is not in the game files. The page `contestedzonetimers.com` publishes it as plain text at
//! [`START_URL`]; it is fetched at most every [`FETCH_EVERY`] seconds and kept in `contested.json` in the
//! data folder, together with a start typed by hand, which takes precedence.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::formats;
use crate::jsonio::{self, Value};
use crate::language::{self, Arg, Lang, tf};
use crate::net::{self, FetchError, Net};
use crate::paths::Paths;

/// The file of the start kept (in the folder `Intern`).
pub const FILE: &str = "contested.json";
/// The format written.
const FORMAT: i64 = 1;
/// The page that publishes the start, as plain text.
pub const START_URL: &str = "https://contestedzonetimers.com/lib/cfg.dat";
/// The page the start comes from, for the link beside it.
pub const SOURCE_URL: &str = "https://contestedzonetimers.com/";
/// The name of the source as shown.
pub const SOURCE_NAME: &str = "contestedzonetimers.com";
/// The operator of the source.
pub const SOURCE_OPERATOR: &str = "Appieman";
/// So many seconds pass at least between two fetches of the start.
pub const FETCH_EVERY: f64 = 6.0 * 3_600.0;
/// So many seconds pass at least between two tries after one that failed.
pub const RETRY_AFTER: f64 = 600.0;
/// The most a fetched answer may hold.
const ANSWER_LIMIT: u64 = 1_024;

/// Seconds of the whole cycle.
pub const CYCLE: i64 = 11_100;
/// Seconds the hangar stays closed at the start of the cycle.
pub const CLOSED: i64 = 7_200;
/// Seconds the hangar is open.
pub const OPEN: i64 = 3_600;
/// Seconds the hangar resets at the end of the cycle.
pub const RESET: i64 = 300;
/// The lamps over the hangar door.
pub const LAMPS: usize = 5;
/// Seconds between two lamps turning green while the hangar is closed.
const CLOSED_STEP: i64 = 1_440;
/// Seconds between two lamps going out while the hangar is open.
const OPEN_STEP: i64 = 720;

/// The earliest and the latest start taken for a start (1 Jan 2020, 1 Jan 2100).
const START_RANGE: std::ops::RangeInclusive<i64> = 1_577_836_800..=4_102_444_800;

/// One writer of the file at a time.
static LOCK: Mutex<()> = Mutex::new(());

// ------------------------------------------------------------ the cycle

/// Where the cycle stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Open,
    Resetting,
}

/// How a lamp shines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lamp {
    Red,
    Green,
    Dark,
}

/// The state of the hangar at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hangar {
    pub phase: Phase,
    /// Seconds to the next change of the phase.
    pub to_change: i64,
    /// Seconds to the end of the whole cycle.
    pub cycle_rest: i64,
    pub lamps: [Lamp; LAMPS],
    /// How many lamps shine green.
    pub green: usize,
    /// The next lamp to change (counted from 1, left to right) and the seconds until it does; none while the
    /// hangar resets.
    pub next_lamp: Option<(usize, i64)>,
    /// Seconds of the phase that have passed, and the length of the phase.
    pub phase_elapsed: i64,
    pub phase_length: i64,
}

impl Hangar {
    /// How far the phase has come, from 0 to 1.
    pub fn progress(&self) -> f32 {
        (self.phase_elapsed as f32 / self.phase_length as f32).clamp(0.0, 1.0)
    }
}

/// Whole seconds of `now`, rounded down.
fn whole(now: f64) -> i64 {
    formats::floor_seconds(now).unwrap_or(0)
}

/// The lamps with the first `changed` ones in the state `after` and the others in the state `before`.
fn lamps(changed: i64, before: Lamp, after: Lamp) -> [Lamp; LAMPS] {
    std::array::from_fn(|i| if (i as i64) < changed { after } else { before })
}

/// The hangar `now` (Unix seconds) for a cycle that started at `start`.
pub fn hangar_at(start: i64, now: f64) -> Hangar {
    let position = (whole(now) - start).rem_euclid(CYCLE);
    let cycle_rest = CYCLE - position;
    if position < CLOSED {
        let changed = position / CLOSED_STEP;
        Hangar {
            phase: Phase::Closed,
            to_change: CLOSED - position,
            cycle_rest,
            lamps: lamps(changed, Lamp::Red, Lamp::Green),
            green: changed as usize,
            next_lamp: Some((changed as usize + 1, (changed + 1) * CLOSED_STEP - position)),
            phase_elapsed: position,
            phase_length: CLOSED,
        }
    } else if position < CLOSED + OPEN {
        let since = position - CLOSED;
        let changed = since / OPEN_STEP;
        Hangar {
            phase: Phase::Open,
            to_change: CLOSED + OPEN - position,
            cycle_rest,
            lamps: lamps(changed, Lamp::Green, Lamp::Dark),
            green: LAMPS - changed as usize,
            next_lamp: Some((changed as usize + 1, (changed + 1) * OPEN_STEP - since)),
            phase_elapsed: since,
            phase_length: OPEN,
        }
    } else {
        Hangar {
            phase: Phase::Resetting,
            to_change: cycle_rest,
            cycle_rest,
            lamps: [Lamp::Dark; LAMPS],
            green: 0,
            next_lamp: None,
            phase_elapsed: position - CLOSED - OPEN,
            phase_length: RESET,
        }
    }
}

/// One time the hangar opens, and when it closes again (Unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opening {
    pub opens: i64,
    pub closes: i64,
}

/// The next `count` times the hangar opens after `now`.
pub fn next_openings(start: i64, now: f64, count: usize) -> Vec<Opening> {
    let first = (whole(now) - start - CLOSED).div_euclid(CYCLE) + 1;
    (first..)
        .take(count)
        .map(|k| Opening { opens: start + k * CYCLE + CLOSED, closes: start + k * CYCLE + CLOSED + OPEN })
        .collect()
}

// ------------------------------------------------------------ the start

/// Whether `seconds` can be the start of a cycle.
pub fn plausible_start(seconds: i64) -> bool {
    START_RANGE.contains(&seconds)
}

/// The start in an answer of the page: a whole number, blanks around it allowed.
pub fn parse_start(body: &[u8]) -> Option<i64> {
    let start: i64 = String::from_utf8_lossy(body).trim().parse().ok()?;
    plausible_start(start).then_some(start)
}

/// The start of a cycle in which the hangar opens at `now`: the player has just seen it open.
pub fn start_just_opened(now: i64) -> i64 {
    now - CLOSED
}

/// The start of a cycle in which the hangar starts to reset at `now`: the player has just seen it close.
pub fn start_just_reset(now: i64) -> i64 {
    now - CLOSED - OPEN
}

/// The latest moment at or before `now` that was `hour:minute` o'clock local time.
pub fn start_from_clock(now: i64, hour: u32, minute: u32) -> Option<i64> {
    if hour > 23 || minute > 59 {
        return None;
    }
    let local = formats::local_time(now)?;
    let since_midnight = i64::from(local.hour) * 3_600 + i64::from(local.minute) * 60 + i64::from(local.second);
    let start = now - since_midnight + i64::from(hour) * 3_600 + i64::from(minute) * 60;
    Some(if start > now { start - 86_400 } else { start })
}

/// A start typed by hand: Unix seconds, or a time of day like `14:32` for the moment the cycle began.
pub fn parse_input(text: &str, now: i64) -> Option<i64> {
    let text = text.trim();
    if let Some((hour, minute)) = text.split_once(':') {
        let digits = |part: &str| part.trim().chars().all(|c| c.is_ascii_digit()) && !part.trim().is_empty();
        if !digits(hour) || !digits(minute) {
            return None;
        }
        return start_from_clock(now, hour.trim().parse().ok()?, minute.trim().parse().ok()?);
    }
    let start: i64 = text.parse().ok()?;
    plausible_start(start).then_some(start)
}

/// Where the start in use comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartSource {
    Page,
    Hand,
}

/// What is kept of the start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kept {
    /// The start the page published, when it was last fetched.
    pub site: Option<i64>,
    /// When the page was fetched (Unix seconds); 0 for never.
    pub fetched: f64,
    /// The start typed by hand.
    pub hand: Option<i64>,
}

impl Kept {
    /// The start the times are counted from: the one typed by hand, else the one of the page.
    pub fn start(&self) -> Option<i64> {
        self.hand.or(self.site)
    }

    /// Where the start in use comes from.
    pub fn source(&self) -> Option<StartSource> {
        if self.hand.is_some() { Some(StartSource::Hand) } else { self.site.map(|_| StartSource::Page) }
    }

    /// Whether the page is to be fetched at `now`: it never was, or the last time is long enough ago.
    pub fn due(&self, now: f64) -> bool {
        self.site.is_none() || now - self.fetched >= FETCH_EVERY
    }
}

fn whole_number(value: &Value) -> Option<i64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(n) => formats::floor_seconds(*n),
        _ => None,
    }
}

/// What is kept in the data folder.
pub fn load(paths: &Paths) -> Kept {
    let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { return Kept::default() };
    let get = |key: &str| data.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let start = |key: &str| get(key).and_then(whole_number).filter(|s| plausible_start(*s));
    Kept {
        site: start("start"),
        fetched: get("geholt")
            .and_then(|v| match v {
                Value::Float(n) => Some(*n),
                other => whole_number(other).map(|n| n as f64),
            })
            .unwrap_or(0.0),
        hand: start("hand"),
    }
}

fn write(paths: &Paths, kept: &Kept) -> std::io::Result<()> {
    let number = |value: Option<i64>| value.map_or(Value::Null, Value::int);
    let data = Value::Object(vec![
        ("format".into(), Value::int(FORMAT)),
        ("start".into(), number(kept.site)),
        ("geholt".into(), Value::Float(kept.fetched)),
        ("hand".into(), number(kept.hand)),
    ]);
    let target = paths.app_file(FILE);
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() {
        jsonio::make_dirs(&folder)?;
    }
    jsonio::write_text_atomic(&target, &jsonio::dumps(&data, Some(1), false), jsonio::Newline::Keep)
}

/// Keeps the start typed by hand, or forgets it (`None`); the start of the page stays.
///
/// # Errors
///
/// When the file cannot be written.
pub fn set_hand(paths: &Paths, hand: Option<i64>) -> std::io::Result<Kept> {
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let kept = Kept { hand, ..load(paths) };
    write(paths, &kept)?;
    Ok(kept)
}

/// What a fetch did.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Fetched and kept.
    Fresh(Kept),
    /// Fetching is switched off (`SC_BP_NO_NET`); nothing was changed.
    Offline,
    /// The page could not be fetched or its answer is no start; what was kept is untouched.
    Failed(String),
}

/// Fetches the start from the page and keeps it, together with the time `now`.
pub fn refresh(paths: &Paths, net: &dyn Net, now: f64) -> Outcome {
    if net::offline_in(&paths.host) {
        return Outcome::Offline;
    }
    let body = match net.get(START_URL) {
        Ok(body) => body,
        Err(FetchError::Offline) => return Outcome::Offline,
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    let Some(start) = parse_start(&body) else { return Outcome::Failed("unexpected answer".into()) };
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let kept = Kept { site: Some(start), fetched: now, ..load(paths) };
    match write(paths, &kept) {
        Ok(()) => Outcome::Fresh(kept),
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// The real net for [`refresh`]: the name of the program, a short time limit, a small answer.
pub fn web(user_agent: &str) -> net::Web {
    net::Web::new(user_agent, std::time::Duration::from_secs(15), ANSWER_LIMIT)
}

/// The Unix time now, with the fraction of the second.
pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

// ------------------------------------------------------------ writing times

/// `HH:MM:SS` for a number of seconds (negative counts as 0).
pub fn hms(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}:{:02}", seconds / 3_600, seconds % 3_600 / 60, seconds % 60)
}

/// `MM:SS` for a number of seconds (negative counts as 0).
pub fn minutes_seconds(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// The local time of day of a moment as `HH:MM`.
pub fn clock(seconds: i64) -> String {
    formats::local_time(seconds).map(|time| formats::strftime("%H:%M", &time)).unwrap_or_default()
}

/// A moment in local time for a list: `today 22:15` on the day of `now`, else day and time (`24.10. 22:15`, in
/// English `10-24 22:15`).
pub fn when(lang: Lang, seconds: i64, now: i64) -> String {
    let (Some(at), Some(today)) = (formats::local_time(seconds), formats::local_time(now)) else {
        return String::new();
    };
    if (at.year, at.month, at.day) == (today.year, today.month, today.day) {
        return tf("s_sp_heute", &[("0", Arg::from(formats::strftime("%H:%M", &at)))]);
    }
    let format = match lang {
        Lang::De => "%d.%m. %H:%M",
        Lang::En => "%m-%d %H:%M",
    };
    formats::strftime(format, &at)
}

/// The compact line of the overlay: the short name, the lamps, and what the hangar does next.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayLine {
    pub label: String,
    /// The phase the text is coloured by; none while there is no start.
    pub phase: Option<Phase>,
    pub lamps: [Lamp; LAMPS],
    pub text: String,
}

/// The line of the overlay at `now` for the start the page keeps (none: there is no start yet).
pub fn overlay_line(start: Option<i64>, now: f64) -> OverlayLine {
    let label = language::t("ov_hangar_kurz");
    let Some(start) = start else {
        return OverlayLine { label, phase: None, lamps: [Lamp::Dark; LAMPS], text: language::t("ov_hangar_ohne") };
    };
    let hangar = hangar_at(start, now);
    let key = match hangar.phase {
        Phase::Closed => "ov_hangar_oeffnet",
        Phase::Open => "ov_hangar_schliesst",
        Phase::Resetting => "ov_hangar_reset",
    };
    OverlayLine {
        label,
        phase: Some(hangar.phase),
        lamps: hangar.lamps,
        text: tf(key, &[("0", Arg::from(countdown(hangar.to_change)))]),
    }
}

/// A countdown as the circle shows it: `H:MM:SS` from an hour on, else `MM:SS`.
pub fn countdown(seconds: i64) -> String {
    if seconds >= 3_600 {
        let seconds = seconds.max(0);
        format!("{}:{:02}:{:02}", seconds / 3_600, seconds % 3_600 / 60, seconds % 60)
    } else {
        minutes_seconds(seconds)
    }
}

/// Which day a moment lies on, seen from `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Day {
    Today,
    Tomorrow,
    /// Any other day, written as `24.10.` (in English `10-24`).
    Other(String),
}

/// Days since 1970-01-01 of a calendar date.
fn civil_days(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The local day of `seconds` seen from `now`; none when a time cannot be converted.
pub fn day(lang: Lang, seconds: i64, now: i64) -> Option<Day> {
    let (at, today) = (formats::local_time(seconds)?, formats::local_time(now)?);
    Some(match civil_days(at.year, at.month, at.day) - civil_days(today.year, today.month, today.day) {
        0 => Day::Today,
        1 => Day::Tomorrow,
        _ => Day::Other(formats::strftime(
            match lang {
                Lang::De => "%d.%m.",
                Lang::En => "%m-%d",
            },
            &at,
        )),
    })
}

/// The date and time of a moment in full, as the lists of the program write them.
pub fn stamp(lang: Lang, seconds: i64) -> String {
    formats::date_time(lang, seconds as f64)
}

// ------------------------------------------------------------ the self timers

/// One self timer: what it is called, and how long it runs.
#[derive(Clone, Copy, Debug)]
pub struct TimerSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub seconds: i64,
}

/// A row of timers under one heading.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub name: &'static str,
    pub timers: &'static [TimerSpec],
}

/// The timers of one place.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub name: &'static str,
    pub sections: &'static [Section],
}

const fn timer(id: &'static str, name: &'static str, minutes: i64) -> TimerSpec {
    TimerSpec { id, name, seconds: minutes * 60 }
}

/// The self timers by place, with the names the game uses.
pub const GROUPS: [Group; 4] = [
    Group {
        name: "Checkmate",
        sections: &[
            Section {
                name: "Blue Keycards",
                timers: &[
                    timer("checkmate-terminal1", "Terminal 1", 15),
                    timer("checkmate-terminal2", "Terminal 2", 15),
                    timer("checkmate-terminal3", "Terminal 3", 15),
                ],
            },
            Section {
                name: "Compboards",
                timers: &[
                    timer("checkmate-tablet1", "Tablet 1", 30),
                    timer("checkmate-tablet2", "Tablet 2", 30),
                    timer("checkmate-tablet3", "Tablet 3", 30),
                ],
            },
        ],
    },
    Group {
        name: "Orbituary",
        sections: &[
            Section {
                name: "Blue Keycards",
                timers: &[
                    timer("orbituary-terminal1", "Terminal 1", 15),
                    timer("orbituary-terminal2", "Terminal 2", 15),
                ],
            },
            Section {
                name: "Compboards",
                timers: &[timer("orbituary-tablet4", "Tablet 4", 30), timer("orbituary-tablet7", "Tablet 7", 30)],
            },
        ],
    },
    Group {
        name: "Ruin Station",
        sections: &[
            Section {
                name: "Keycards",
                timers: &[
                    timer("ruinstation-crypt", "The Crypt", 15),
                    timer("ruinstation-lastresort", "The Last Resort", 15),
                    timer("ruinstation-wasteland", "The Wasteland", 15),
                ],
            },
            Section {
                name: "Compboards",
                timers: &[timer("ruinstation-tablet5", "Tablet 5", 30), timer("ruinstation-tablet6", "Tablet 6", 30)],
            },
        ],
    },
    Group {
        name: "PYAM-SUPVISR",
        sections: &[Section {
            name: "Red Keycards",
            timers: &[timer("pyam-3-4", "-3-4", 30), timer("pyam-3-5", "-3-5", 30)],
        }],
    },
];

/// Seconds a timer that ran out stays highlighted.
pub const HIGHLIGHT: f64 = 6.0;

/// How a self timer stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    /// Not started, at its full time.
    Idle,
    Running,
    /// Stopped with time left, or set to another time.
    Paused,
    /// Ran out.
    Done,
}

/// A countdown that runs on the clock: `end` is the Unix time it ends at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfTimer {
    full: i64,
    /// The seconds left while it does not run.
    left: i64,
    end: Option<f64>,
    /// When it ran out, until it is reset or set to another time.
    done_at: Option<f64>,
}

impl SelfTimer {
    pub fn new(full: i64) -> Self {
        SelfTimer { full, left: full, end: None, done_at: None }
    }

    /// The full time, which a minute button cannot pass.
    pub fn full(&self) -> i64 {
        self.full
    }

    /// The seconds left at `now`, rounded up so that a timer just started shows its full time.
    pub fn left(&self, now: f64) -> i64 {
        match self.end {
            Some(end) => formats::floor_seconds((end - now).ceil()).unwrap_or(0).clamp(0, self.full),
            None => self.left,
        }
    }

    pub fn state(&self) -> TimerState {
        if self.done_at.is_some() {
            TimerState::Done
        } else if self.end.is_some() {
            TimerState::Running
        } else if self.left == self.full {
            TimerState::Idle
        } else {
            TimerState::Paused
        }
    }

    /// Starts, or goes on after a stop. A timer with no time left does not start.
    pub fn start(&mut self, now: f64) {
        if self.end.is_none() && self.left > 0 {
            self.end = Some(now + self.left as f64);
        }
    }

    /// Stops with the time left at `now`.
    pub fn stop(&mut self, now: f64) {
        if self.end.is_some() {
            self.left = self.left(now);
            self.end = None;
        }
    }

    /// Back to the full time, not running.
    pub fn reset(&mut self) {
        *self = SelfTimer::new(self.full);
    }

    /// Adds `minutes` (or takes them off) within 0 and the full time; a timer that ran out is alive again.
    pub fn adjust(&mut self, minutes: i64, now: f64) {
        let left = (self.left(now) + minutes * 60).clamp(0, self.full);
        match self.end {
            Some(_) => self.end = Some(now + left as f64),
            None => self.left = left,
        }
        if left > 0 {
            self.done_at = None;
        }
    }

    /// Notes that the timer ran out; true the one time it does.
    pub fn tick(&mut self, now: f64) -> bool {
        match self.end {
            Some(end) if now >= end => {
                self.end = None;
                self.left = 0;
                self.done_at = Some(end);
                true
            }
            _ => false,
        }
    }

    /// Whether the timer ran out a moment ago.
    pub fn highlighted(&self, now: f64) -> bool {
        self.done_at.is_some_and(|at| now - at < HIGHLIGHT)
    }

    /// Seconds until it runs out, while it runs.
    pub fn wake_in(&self, now: f64) -> Option<f64> {
        self.end.map(|end| (end - now).max(0.0))
    }
}

#[cfg(test)]
mod tests;
