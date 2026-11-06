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

//! The time stamps of the Game.log and the facts every reading of a whole
//! log needs (arrived in the game, first and last stamp).
//!
//! ⚠ Python reads a log line by line (`for line in f`, universal newlines)
//! and looks at the first stamp of each line. The same answers are given
//! here from the whole text at once: the first stamp of the text is the first
//! stamp of its first stamped line, the last one is taken from the last
//! stamped line (its first stamp, not the last one on it).

use std::sync::LazyLock;

use regex::Regex;

/// The line that shows the player has really arrived in the game.
pub const SPAWN_MARKER: &str = "OnClientSpawned";

/// The stamp at the start of a line: `<2026-08-29T16:02:14.792Z>`.
static TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)").expect("fester Ausdruck"));

/// What `time.strptime(stamp[:19], '%Y-%m-%dT%H:%M:%S')` accepts: the same
/// alternatives per field as the regular expression of Python's `_strptime`.
static STRPTIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^([0-9]{4})-(1[0-2]|0[1-9]|[1-9])-(3[01]|[12][0-9]|0[1-9]|[1-9]| [1-9])T(2[0-3]|[01][0-9]|[0-9]):([0-5][0-9]|[0-9]):(6[01]|[0-5][0-9]|[0-9])$",
    )
    .expect("fester Ausdruck")
});

/// `_time_of`: the stamp of the first one in the line, or the empty text.
pub fn time_of(line: &str) -> &str {
    TIME.find(line).map_or("", |m| &m.as_str()[1..])
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a calendar day.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `_seconds`: a stamp as seconds since 1970 (`calendar.timegm` of the
/// `strptime` result), `None` when Python's parsing would raise.
///
/// ⚠ Only the digits 0 to 9 are read; Python's `\d` also accepts the digits
/// of other scripts.
pub fn seconds(stamp: &str) -> Option<i64> {
    let head: String = stamp.chars().take(19).collect();
    let caps = STRPTIME.captures(&head)?;
    let field = |i: usize| caps[i].trim().parse::<i64>().ok();
    let (year, month, day) = (field(1)?, field(2)?, field(3)?);
    let (hour, minute, second) = (field(4)?, field(5)?, field(6)?);
    if year < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// The first and last stamp of a whole log and whether the player arrived.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub spawn: bool,
    pub first: Option<String>,
    pub last: Option<String>,
    /// The stamp of the first line that tells the player arrived ([`SPAWN_MARKER`]).
    pub spawned: Option<String>,
}

/// The first stamp in a text.
pub fn first_stamp(text: &str) -> Option<&str> {
    TIME.find(text).map(|m| &m.as_str()[1..])
}

/// The stamp of the last line that carries one: backwards line by line until one does.
pub fn last_stamp(text: &str) -> Option<&str> {
    text.rsplit(['\n', '\r']).find_map(|line| TIME.find(line).map(|m| &m.as_str()[1..]))
}

impl Facts {
    pub fn of(text: &str) -> Facts {
        let first = first_stamp(text).map(str::to_string);
        let last = first.as_ref().and_then(|_| last_stamp(text).map(str::to_string));
        let spawned = text.find(SPAWN_MARKER).and_then(|at| {
            let start = text[..at].rfind(['\n', '\r']).map_or(0, |i| i + 1);
            let line = text[start..].split(['\n', '\r']).next().unwrap_or_default();
            TIME.find(line).map(|m| m.as_str()[1..].to_string())
        });
        Facts { spawn: text.contains(SPAWN_MARKER), first, last, spawned }
    }

    /// Seconds from the first arrival of the player to the last stamp; 0 unless both parse, the stamps are not zero
    /// and the player arrived.
    pub fn after_spawn(&self) -> i64 {
        let spawned = seconds(self.spawned.as_deref().unwrap_or(""));
        let last = seconds(self.last.as_deref().unwrap_or(""));
        match (spawned, last) {
            (Some(a), Some(b)) if a != 0 && b != 0 => (b - a).max(0),
            _ => 0,
        }
    }

    /// Seconds from the first to the last stamp; 0 unless both parse and are not zero.
    pub fn duration(&self) -> i64 {
        let first = seconds(self.first.as_deref().unwrap_or(""));
        let last = seconds(self.last.as_deref().unwrap_or(""));
        match (first, last) {
            (Some(a), Some(b)) if a != 0 && b != 0 => b - a,
            _ => 0,
        }
    }
}

/// The lines of a text that contain a match of `markers`, in order, each
/// once. Lines end at `\n` or `\r`, like Python's universal newlines.
pub fn candidate_lines<'a>(text: &'a str, markers: &'a Regex) -> impl Iterator<Item = &'a str> + 'a {
    let mut end_of_last = 0;
    let mut hits = markers.find_iter(text);
    std::iter::from_fn(move || {
        loop {
            let hit = hits.next()?;
            if hit.start() < end_of_last {
                continue;
            }
            let start = text[..hit.start()].rfind(['\n', '\r']).map_or(0, |i| i + 1);
            let end = text[hit.start()..].find(['\n', '\r']).map_or(text.len(), |i| hit.start() + i);
            end_of_last = end;
            return Some(&text[start..end]);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_like_strptime() {
        assert_eq!(seconds("2026-10-18T16:31:30"), Some(1_792_341_090));
        assert_eq!(seconds("1970-01-01T00:00:00"), Some(0));
        // The leap second fields Python's strptime allows.
        assert_eq!(seconds("2026-10-18T16:31:61"), Some(1_792_341_121));
        // What it refuses: month 0 or 13, day 32, 30 February, hour 24, minute 60, second 62.
        for bad in [
            "",
            "2026-00-10T00:00:00",
            "2026-13-10T00:00:00",
            "2026-10-32T00:00:00",
            "2026-02-30T00:00:00",
            "2026-10-10T24:00:00",
            "2026-10-10T10:60:00",
            "2026-10-10T10:10:62",
            "0000-01-01T00:00:00",
            "kaputt",
        ] {
            assert_eq!(seconds(bad), None, "{bad}");
        }
        // 29 February only in a leap year; the cut at 19 characters.
        assert!(seconds("2028-02-29T00:00:00").is_some());
        assert_eq!(seconds("2026-02-29T00:00:00"), None);
        assert_eq!(seconds("2026-10-18T16:31:30.792Z"), Some(1_792_341_090));
    }

    #[test]
    fn the_last_stamp_is_the_first_one_of_the_last_stamped_line() {
        let text = "<2026-01-01T10:00:00.1Z> eins <2026-09-09T09:09:09>\nohne\r<2026-01-01T12:00:00Z> zwei <2026-02-02T02:02:02>\nSchluss ohne";
        let facts = Facts::of(text);
        assert_eq!(facts.first.as_deref(), Some("2026-01-01T10:00:00"));
        assert_eq!(facts.last.as_deref(), Some("2026-01-01T12:00:00"));
        assert!(!facts.spawn);
        assert!(Facts::of("x OnClientSpawned y").spawn);
        assert_eq!(Facts::of("kein Stempel"), Facts::default());
        assert_eq!(facts.duration(), 7200);
    }

    #[test]
    fn the_time_since_the_arrival_counts_from_the_first_spawn_line() {
        let text = "<2026-11-06T08:11:17.443Z> Log started\n<2026-11-06T08:11:42.663Z> [CSessionManager::OnClientSpawned] Spawned!\n<2026-11-06T08:22:12.241Z> [CSessionManager::OnClientSpawned] Spawned!\n<2026-11-06T09:13:11.000Z> ende\n";
        let facts = Facts::of(text);
        assert_eq!(facts.spawned.as_deref(), Some("2026-11-06T08:11:42"));
        assert_eq!(facts.after_spawn(), 61 * 60 + 29);
        assert_eq!(facts.duration(), 61 * 60 + 54);
        // Without an arrival, or with an arrival without a stamp, nothing is measured.
        assert_eq!(Facts::of("<2026-11-06T08:11:17.443Z> a\n<2026-11-06T09:13:11.000Z> b\n").after_spawn(), 0);
        assert_eq!(Facts::of("<2026-11-06T08:11:17.443Z> a\nOnClientSpawned ohne Stempel\n<2026-11-06T09:13:11.000Z> b\n").after_spawn(), 0);
    }

    #[test]
    fn candidate_lines_come_once_and_in_order() {
        let markers = Regex::new("AAA|BBB").expect("fester Ausdruck");
        let text = "AAA BBB\nx\r\ny BBB\rzzz\nAAA";
        let lines: Vec<&str> = candidate_lines(text, &markers).collect();
        assert_eq!(lines, ["AAA BBB", "y BBB", "AAA"]);
    }
}
