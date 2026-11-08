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

//! How long was played: every session found in a log is written down once
//! (`spielzeit.json`) and counted for ever, because the game keeps only a
//! handful of logs. `playtime.py`: the catching up of the file, the sums of
//! its periods and the session running now.
//!
//! ⚠ A session counts when the player really arrived in the game. Short
//! ones count too: drawing a line would be a claim about real playing.
//! ⚠ One longer than 24 hours is no session (a changed clock, or a log with
//! two runs): better to drop one than to spoil the total.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::errors;
use crate::jsonio::{self, Map, Value, map_set};
use crate::logread;
use crate::logtime::{Facts, FactsScan, seconds};
use crate::mission_log::get;
use crate::ordered::parallel_map;
use crate::paths::Paths;
use crate::pypath::Flavor;
use crate::text::decode_replace;

pub const FILE: &str = "spielzeit.json";
pub const FORMAT: i64 = 1;
/// A session longer than this is none.
pub const MAX_SPAN_SEC: i64 = 24 * 3600;

/// From and to of a log's text in seconds, or `None` when the player never
/// arrived or the stamps make no session (`span_from_log`).
pub fn span_of_text(text: &str) -> Option<(i64, i64)> {
    span_of_facts(&Facts::of(text))
}

fn span_of_facts(facts: &Facts) -> Option<(i64, i64)> {
    if !facts.spawn {
        return None;
    }
    let start = seconds(facts.first.as_deref().unwrap_or(""))?;
    let end = seconds(facts.last.as_deref().unwrap_or(""))?;
    if start == 0 || end == 0 || end < start || end - start > MAX_SPAN_SEC {
        return None;
    }
    Some((start, end))
}

/// [`span_of_text`] of a log file; an unreadable file has none.
///
/// ⚠ The file is read a piece at a time, so a log of gigabytes never stands in memory whole.
pub fn span_from_log(path: &str) -> Option<(i64, i64)> {
    let mut scan = FactsScan::default();
    logread::read_all(path, |bytes| scan.piece(&decode_replace(bytes))).ok()?;
    span_of_facts(scan.facts())
}

/// The saved sessions: `{'format':…, 'sitzungen':[…]}`. A broken file is set
/// aside and reported (`load_json`).
pub fn load(paths: &Paths) -> Map {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "playtime.load") {
        Some(Value::Object(map)) if matches!(get(&map, "sitzungen"), Some(Value::Array(_))) => map,
        _ => vec![("format".into(), Value::int(FORMAT)), ("sitzungen".into(), Value::Array(Vec::new()))],
    }
}

/// Writes first beside (`.neu`), then renames: a crash in between must not
/// leave half a file, and with it the whole recorded past. Sets the format
/// and reports a failure under `label`.
pub(crate) fn write_beside(paths: &Paths, file: &str, data: &mut Map, format: i64, label: &str) -> bool {
    map_set(data, "format", Value::int(format));
    let target = paths.app_file(file);
    let result = (|| {
        let folder = Flavor::native().dirname(&target);
        if !folder.is_empty() && !Path::new(&folder).is_dir() {
            jsonio::make_dirs(&folder)?;
        }
        let temp = format!("{target}.neu");
        jsonio::write_new(&temp, jsonio::dumps(&Value::Object(data.clone()), None, false).as_bytes())?;
        jsonio::os_replace(&temp, &target)
    })();
    match result {
        Ok(()) => true,
        Err(e) => {
            errors::record(label, &e);
            false
        }
    }
}

/// Writes the sessions; reports when it fails, or the time would be gone silently.
pub fn save(paths: &Paths, data: &mut Map) -> bool {
    write_beside(paths, FILE, data, FORMAT, "playtime.save")
}

/// A number as Python compares it (`True` is 1).
pub(crate) fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

fn von_key(entry: &Value) -> String {
    jsonio::dumps(entry.get("von").unwrap_or(&Value::Null), None, false)
}

/// Reads the logs and writes the sessions on; returns the new ones.
///
/// All logs are handed over, not only the fresh ones: this file has its own
/// read position (name and size). A session is known by its start; the
/// file name is no good, the running `Game.log` becomes a backup at the next
/// start. A longer session with a known start extends the known one: the
/// running log still grows.
pub fn catch_up(paths: &Paths, files: &[String]) -> usize {
    let known_marks = match get(&load(paths), "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    // Asked before reading: all the logs together are many megabytes.
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut to_read: Vec<&String> = Vec::new();
    for path in files {
        let Ok(meta) = fs::metadata(path) else { continue };
        let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let name = Flavor::native().basename(path);
        let same = number(get(&known_marks, &name)).is_some_and(|stored| stored == size as f64);
        if same {
            continue;
        }
        marks.push((name, size));
        to_read.push(path);
    }
    let found = parallel_map(&to_read, |path| span_from_log(path));
    // ⚠ Written anew under the lock, from the file as it is now: the page of
    // the statistics and the reading of the contract log both come here.
    let _guard = jsonio::lock_saves();
    let mut data = load(paths);
    let mut sessions: Vec<Value> = match get(&data, "sitzungen") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let mut known: HashMap<String, usize> = HashMap::new();
    for (i, entry) in sessions.iter().enumerate() {
        known.insert(von_key(entry), i);
    }
    let mut read: Map = match get(&data, "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    for (name, size) in &marks {
        map_set(&mut read, name, Value::int(*size));
    }
    let mut added = 0;
    for span in found {
        let Some((start, end)) = span else { continue };
        let key = jsonio::dumps(&Value::int(start), None, false);
        match known.get(&key) {
            None => {
                sessions.push(Value::Object(vec![("von".into(), Value::int(start)), ("bis".into(), Value::int(end))]));
                known.insert(key, sessions.len() - 1);
                added += 1;
            }
            Some(&i) => {
                if let Value::Object(entry) = &mut sessions[i]
                    && (end as f64) > number(get(entry, "bis")).unwrap_or(0.0)
                {
                    map_set(entry, "bis", Value::int(end));
                }
            }
        }
    }
    let von = |e: &Value| number(e.get("von")).unwrap_or(0.0);
    sessions.sort_by(|a, b| von(a).total_cmp(&von(b)));
    map_set(&mut data, "sitzungen", Value::Array(sessions));
    map_set(&mut data, "gelesen", Value::Object(read));
    save(paths, &mut data);
    added
}

/// The saved sessions as (from, to) in seconds, as the file lists them; an
/// entry without both ends is none.
pub fn spans(paths: &Paths) -> Vec<(i64, i64)> {
    let data = load(paths);
    let Some(Value::Array(list)) = get(&data, "sitzungen") else { return Vec::new() };
    let whole = |value: Option<&Value>| number(value).map(|n| n as i64).filter(|n| *n != 0);
    list.iter().filter_map(|entry| Some((whole(entry.get("von"))?, whole(entry.get("bis"))?))).collect()
}

/// Overlapping periods melt into one, or the time would count twice
/// (`_merge_spans`). ⚠ In the real data there was exactly one such case,
/// with 7.9 hours of overlap.
pub fn merge_spans(spans: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut sorted = spans.to_vec();
    sorted.sort_unstable();
    let mut out: Vec<(i64, i64)> = Vec::new();
    for (start, end) in sorted {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// What is remembered of the running log between two questions: the file,
/// its size and where the session began.
static RUNNING_START: std::sync::Mutex<Option<(String, u64, Option<i64>)>> = std::sync::Mutex::new(None);

/// From and to of the session running now, or `None` when the game does not
/// run.
///
/// ⚠ The end comes from the write time of the file, not from the clock: who
/// closes the game and leaves the program open would otherwise see a session
/// that goes on while nobody plays. ⚠ The start is not searched on every call:
/// the `Game.log` has megabytes. It is kept as long as the same file is the
/// same session; when the game starts anew the file gets **shorter**.
pub fn running_span(paths: &Paths) -> Option<(i64, i64)> {
    let file = paths.game_log(None)?;
    if !crate::mission_log::game_running(paths) {
        return None;
    }
    let meta = fs::metadata(&file).ok()?;
    let size = meta.len();
    let start = {
        let mut kept = RUNNING_START.lock().unwrap_or_else(|e| e.into_inner());
        let known = kept.as_ref().filter(|(name, was, start)| *name == file && *was <= size && start.is_some());
        let start = match known {
            Some((_, _, start)) => *start,
            None => span_from_log(&file).map(|span| span.0),
        };
        *kept = Some((file, size, start));
        start?
    };
    let end = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    (0..=MAX_SPAN_SEC).contains(&(end - start)).then_some((start, end))
}

/// The session running now in seconds, 0 when the game does not run (`session_now`).
pub fn session_now(paths: &Paths) -> i64 {
    session_length(running_span(paths))
}

/// The length of a running session `span`, 0 without one.
pub fn session_length(span: Option<(i64, i64)>) -> i64 {
    span.map_or(0, |(start, end)| end - start)
}

/// Seconds as `3 h 14 min`, short enough for a head line (`as_text`).
///
/// ⚠ No seconds: they change all the time, draw the eye and say nothing about
/// play time. Under a minute it says `0 min`: a number stays a number.
pub fn as_text(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
    if hours > 0 { format!("{hours} h {minutes:02} min") } else { format!("{minutes} min") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy;
    use crate::paths::Host;

    #[test]
    fn spans_melt_when_they_overlap_and_text_stays_short() {
        assert_eq!(merge_spans(&[(50, 80), (10, 20), (15, 30), (30, 40)]), vec![(10, 40), (50, 80)]);
        // One inside another does not shorten the outer one.
        assert_eq!(merge_spans(&[(10, 100), (20, 30)]), vec![(10, 100)]);
        assert!(merge_spans(&[]).is_empty());
        assert_eq!(as_text(3 * 3600 + 5 * 60 + 59), "3 h 05 min");
        assert_eq!(as_text(59), "0 min");
        assert_eq!(as_text(-5), "0 min");
        assert_eq!(as_text(125 * 60), "2 h 05 min");
    }

    #[test]
    fn the_running_session_is_its_length_and_nothing_without_a_game() {
        assert_eq!(session_length(Some((1_000, 1_000 + 42 * 60))), 42 * 60);
        assert_eq!(session_length(None), 0);
        let root = std::env::temp_dir().join(format!("versekit-playtime-sitzung-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("Ordner");
        let home = root.to_string_lossy().into_owned();
        let gone = root.join("kein-spiel").to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(
            crate::pypath::Flavor::native(),
            &[(legacy::ENV_HOME, home.as_str()), (crate::paths::ENV_INSTALL_DIR, gone.as_str())],
            &home,
        ));
        assert_eq!(session_now(&paths), 0, "no game folder, no session");
        let _ = fs::remove_dir_all(&root);
    }

    fn stamped(first: &str, last: &str, spawn: bool) -> String {
        let marker = if spawn { "OnClientSpawned" } else { "" };
        format!("<{first}.000Z> start\n<{last}.000Z> ende {marker}\n")
    }

    #[test]
    fn a_session_needs_the_spawn_and_a_plausible_length() {
        let ok = stamped("2026-10-01T10:00:00", "2026-10-01T11:00:00", true);
        assert_eq!(span_of_text(&ok), seconds("2026-10-01T10:00:00").zip(seconds("2026-10-01T11:00:00")));
        // Never arrived in the game: a loading screen is no playing time.
        assert_eq!(span_of_text(&stamped("2026-10-01T10:00:00", "2026-10-01T11:00:00", false)), None);
        // 24 hours exactly counts, one second more does not; backwards neither.
        assert!(span_of_text(&stamped("2026-10-01T10:00:00", "2026-10-02T10:00:00", true)).is_some());
        assert_eq!(span_of_text(&stamped("2026-10-01T10:00:00", "2026-10-02T10:00:01", true)), None);
        assert_eq!(span_of_text(&stamped("2026-10-01T11:00:00", "2026-10-01T10:00:00", true)), None);
        assert_eq!(span_of_text("OnClientSpawned ohne Stempel"), None);
        assert!(span_of_text(&stamped("2026-10-01T10:00:00", "2026-10-01T10:00:00", true)).is_some());
    }

    #[test]
    fn catching_up_counts_once_extends_and_keeps_the_order() {
        let root = std::env::temp_dir().join(format!("versekit-playtime-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        fs::create_dir_all(&home).expect("Ordner");
        let logs = root.join("logs");
        fs::create_dir_all(&logs).expect("Ordner");
        let host = Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, home.to_str().expect("Pfad"))],
            root.to_str().expect("Pfad"),
        );
        let paths = Paths::new(host);
        let a = logs.join("a.log");
        let b = logs.join("b.log");
        fs::write(&a, stamped("2026-10-02T10:00:00", "2026-10-02T11:00:00", true)).expect("Datei");
        fs::write(&b, stamped("2026-10-01T10:00:00", "2026-10-01T10:30:00", true)).expect("Datei");
        let files = vec![a.to_str().expect("Pfad").to_string(), b.to_str().expect("Pfad").to_string()];
        assert_eq!(catch_up(&paths, &files), 2);
        // Known by name and size: nothing is read again.
        assert_eq!(catch_up(&paths, &files), 0);
        // The same start, a longer log (the running one grows): the session grows.
        // (Longer by a few letters: the read position is name and size.)
        fs::write(&a, format!("{}mehr\n", stamped("2026-10-02T10:00:00", "2026-10-02T12:00:00", true))).expect("Datei");
        assert_eq!(catch_up(&paths, &files), 0);
        let data = load(&paths);
        let Some(Value::Array(sessions)) = get(&data, "sitzungen") else { panic!("Sitzungen") };
        let vons: Vec<f64> = sessions.iter().filter_map(|s| number(s.get("von"))).collect();
        assert_eq!(vons.len(), 2);
        assert!(vons[0] < vons[1], "oldest first");
        assert_eq!(number(sessions[1].get("bis")), seconds("2026-10-02T12:00:00").map(|s| s as f64));
        let keys: Vec<&str> = data.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["format", "sitzungen", "gelesen"]);
        let _ = fs::remove_dir_all(&root);
    }
}
