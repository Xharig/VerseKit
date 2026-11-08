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

//! The error log `fehler.json` in the data folder (as `errors.py` of 4.12 kept it):
//! time, version, location, kind and message of every error that was caught
//! and reported.
//!
//! The last 50 different errors are kept; equal ones (same version,
//! location, kind and message) are counted instead of added. Every text goes
//! through [`Redactor`] first. Written only locally, nothing is sent.
//!
//! This module never fails the program: every function swallows its own
//! errors and reports success as `bool`.

use std::sync::Mutex;

use crate::formats;
use crate::jsonio::{self, Map, Newline, Value};
use crate::paths::Paths;
use crate::redact::Redactor;

pub const FILE: &str = "fehler.json";
/// The setting that holds the time the player last looked at the errors (`%Y-%m-%d %H:%M:%S`).
pub const SEEN_SETTING: &str = "fehler_gesehen";
/// So many entries are kept.
pub const MAX_ENTRIES: usize = 50;

/// What went wrong, if it was an error and not just a note.
pub struct Problem<'a> {
    /// Type name (`ValueError`, `io::Error`).
    pub kind: &'a str,
    pub message: &'a str,
    /// Where it happened, as far as known.
    pub trace: Option<&'a str>,
}

/// The error log of one data folder.
pub struct ErrorLog<'a> {
    paths: &'a Paths,
    version: String,
    lock: Mutex<()>,
}

fn entries_of(value: &Value) -> Vec<Value> {
    match value.get("eintraege") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    }
}

fn falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Int(n) => n == "0",
        Value::Float(x) => *x == 0.0,
        Value::Str(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
    }
}

/// `int(old.get('anzahl') or 1)`; `None` where Python's `int()` would raise.
fn count_of(entry: &Value) -> Option<i128> {
    match entry.get("anzahl") {
        Some(value) if !falsy(value) => crate::paths::py_int(value),
        _ => Some(1),
    }
}

fn ident(entry: &Value) -> [Value; 4] {
    ["fassung", "stelle", "art", "meldung"].map(|k| entry.get(k).cloned().unwrap_or(Value::Null))
}

/// Appends `entry`, or counts an equal one up and moves it to the end.
/// `None` if an old count cannot be read (Python gives up there too).
fn merge(mut entries: Vec<Value>, entry: Map) -> Option<Vec<Value>> {
    let new = Value::Object(entry);
    let wanted = ident(&new);
    for index in (0..entries.len()).rev() {
        if matches!(entries[index], Value::Object(_)) && ident(&entries[index]) == wanted {
            let mut old = entries.remove(index);
            let count = count_of(&old)?;
            if let Value::Object(map) = &mut old {
                jsonio::map_set(map, "anzahl", Value::Int(count.saturating_add(1).to_string()));
                if let Some(time) = new.get("zeit") {
                    jsonio::map_set(map, "zuletzt", time.clone());
                }
                if let Some(trace) = new.get("spur").filter(|t| t.as_str().is_some_and(|s| !s.is_empty())) {
                    jsonio::map_set(map, "spur", trace.clone());
                }
            }
            entries.push(old);
            return Some(entries);
        }
    }
    entries.push(new);
    Some(entries)
}

impl<'a> ErrorLog<'a> {
    pub fn new(paths: &'a Paths, version: &str) -> ErrorLog<'a> {
        ErrorLog { paths, version: version.to_string(), lock: Mutex::new(()) }
    }

    fn path(&self) -> String {
        self.paths.app_file(FILE)
    }

    fn read(&self) -> Vec<Value> {
        jsonio::read_json(&self.path()).map(|v| entries_of(&v)).unwrap_or_default()
    }

    fn write(&self, entries: Vec<Value>) -> bool {
        let data = Value::Object(vec![("eintraege".into(), Value::Array(entries))]);
        jsonio::write_text_atomic(&self.path(), &jsonio::dumps(&data, Some(1), false), Newline::Native).is_ok()
    }

    fn now() -> String {
        formats::local_time(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
        )
        .map(|t| formats::strftime("%Y-%m-%d %H:%M:%S", &t))
        .unwrap_or_default()
    }

    /// Records an error (or, without `problem`, a note) under its location.
    pub fn record(&self, label: &str, problem: Option<Problem<'_>>, note: &str) -> bool {
        self.record_at(label, problem, note, &Self::now())
    }

    /// How many errors of this version were written since the player last looked (or sent a report): what the
    /// button "Fehler melden" counts. Old entries of earlier versions and notes are not new errors.
    pub fn unseen(&self) -> usize {
        let seen = self.paths.setting(SEEN_SETTING).unwrap_or_default();
        self.read()
            .iter()
            .filter(|entry| entry.get("fassung").and_then(Value::as_str) == Some(self.version.as_str()))
            .filter(|entry| entry.get("art").and_then(Value::as_str) != Some("Hinweis"))
            .filter(|entry| {
                let last = entry.get("zuletzt").or_else(|| entry.get("zeit")).and_then(Value::as_str).unwrap_or_default();
                last > seen.as_str()
            })
            .count()
    }

    /// The player looked at the errors: from now on only newer ones count ([`unseen`](Self::unseen)).
    pub fn mark_seen(&self) -> bool {
        self.mark_seen_at(&Self::now())
    }

    /// [`mark_seen`](Self::mark_seen) with a given time stamp (`%Y-%m-%d %H:%M:%S`).
    pub fn mark_seen_at(&self, now: &str) -> bool {
        self.paths.set_setting(SEEN_SETTING, Value::str(now))
    }

    /// [`record`](Self::record) with a given time stamp (`%Y-%m-%d %H:%M:%S`).
    pub fn record_at(&self, label: &str, problem: Option<Problem<'_>>, note: &str, now: &str) -> bool {
        // Built for each entry: it knows the handles of now, also one found since the start.
        let redactor = Redactor::new(self.paths);
        let mut entry: Map = vec![
            ("zeit".into(), Value::str(now)),
            ("fassung".into(), Value::str(self.version.as_str())),
            ("stelle".into(), Value::str(label)),
        ];
        match &problem {
            Some(p) => {
                entry.push(("art".into(), Value::str(p.kind)));
                entry.push(("meldung".into(), Value::str(redactor.redact(p.message))));
                if !note.is_empty() {
                    entry.push(("hinweis".into(), Value::str(redactor.redact(note))));
                }
                let trace = p.trace.map(|t| redactor.redact(crate::text::py_strip(t))).unwrap_or_default();
                entry.push(("spur".into(), Value::str(trace)));
            }
            None => {
                entry.push(("art".into(), Value::str("Hinweis")));
                entry.push(("meldung".into(), Value::str(redactor.redact(note))));
            }
        }
        let Ok(_guard) = self.lock.lock() else { return false };
        let Some(mut entries) = merge(self.read(), entry) else { return false };
        if entries.len() > MAX_ENTRIES {
            entries.drain(..entries.len() - MAX_ENTRIES);
        }
        self.write(entries)
    }

    /// The newest entries, newest first.
    pub fn latest(&self, count: usize) -> Vec<Value> {
        self.read().into_iter().rev().take(count).collect()
    }

    pub fn count(&self) -> usize {
        self.read().len()
    }

    /// Forgets everything.
    pub fn clear(&self) -> bool {
        let Ok(_guard) = self.lock.lock() else { return false };
        self.write(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn log_in(folder: &std::path::Path) -> Paths {
        let dir = folder.to_string_lossy().to_string();
        let home = [("SC_BP_HOME", dir.as_str()), ("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        Paths::new(Host::new(Flavor::native(), &home, "/"))
    }

    /// An account found after the log was opened is redacted in the next entry already.
    #[test]
    fn a_handle_found_later_is_redacted_when_written() {
        let dir = std::env::temp_dir().join(format!("versekit-fehler-spaet-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let paths = log_in(&dir);
        let log = ErrorLog::new(&paths, "5.0.0");
        // A first entry before the account is known: a redactor kept from it would not know it.
        let first = Some(Problem { kind: "IoError", message: "vorher", trace: None });
        assert!(log.record_at("a", first, "", "2026-10-20 00:59:00"));
        std::fs::write(dir.join("einstellungen.json"), r#"{"eigener_account": "Spaetpilot"}"#).unwrap();
        let problem = Some(Problem { kind: "IoError", message: "Spaetpilot fehlt", trace: None });
        assert!(log.record_at("a", problem, "", "2026-10-20 01:00:00"));
        let written = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(written.contains("<SPIELER> fehlt") && !written.contains("Spaetpilot"), "{written}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn counts_equal_errors_and_keeps_fifty() {
        let dir = std::env::temp_dir().join(format!("versekit-fehler-{}", crate::testdir::run_tag()));
        let _ = std::fs::create_dir_all(&dir);
        let paths = log_in(&dir);
        let log = ErrorLog::new(&paths, "5.0.0");
        assert!(log.clear());
        let problem = || Some(Problem { kind: "IoError", message: "/home/spieler/x fehlt", trace: None });
        assert!(log.record_at("a", problem(), "", "2026-10-20 01:00:00"));
        assert!(log.record_at("a", problem(), "", "2026-10-20 01:00:05"));
        let newest = log.latest(1);
        assert_eq!(newest[0].get("anzahl"), Some(&Value::int(2)));
        assert_eq!(newest[0].get("meldung"), Some(&Value::str("<heim>/x fehlt")));
        assert_eq!(newest[0].get("zuletzt"), Some(&Value::str("2026-10-20 01:00:05")));
        for i in 0..60 {
            log.record_at(&format!("stelle{i}"), None, "Hinweis", "2026-10-20 02:00:00");
        }
        assert_eq!(log.count(), MAX_ENTRIES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// "Fehler melden (n)" counts only the new errors of this version since the last look: not the old ones of
    /// the file, not those of an earlier version, not notes; looking sets it to nothing, and an old error that
    /// happens again counts as new.
    #[test]
    fn only_new_errors_since_the_last_look_are_counted() {
        let dir = std::env::temp_dir().join(format!("versekit-fehler-neu-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let paths = log_in(&dir);
        let problem = |message| Some(Problem { kind: "IoError", message, trace: None });
        // Written by the last version: 50 old entries and the same error again.
        let before = ErrorLog::new(&paths, "5.1.3");
        for i in 0..50 {
            before.record_at(&format!("alt{i}"), problem("alt"), "", "2026-11-01 10:00:00");
        }
        let log = ErrorLog::new(&paths, "5.1.4");
        assert_eq!(log.unseen(), 0, "the old entries of the earlier version are no new errors");
        log.record_at("neu", None, "nur ein Hinweis", "2026-11-06 10:00:00");
        assert_eq!(log.unseen(), 0, "a note is no error");
        log.record_at("neu", problem("kaputt"), "", "2026-11-06 10:00:01");
        log.record_at("neu2", problem("kaputt"), "", "2026-11-06 10:00:02");
        assert_eq!(log.unseen(), 2);
        assert!(log.mark_seen_at("2026-11-06 10:00:05"));
        assert_eq!(log.unseen(), 0, "after looking nothing is new");
        log.record_at("neu", problem("kaputt"), "", "2026-11-06 10:01:00");
        assert_eq!(log.unseen(), 1, "the same error again after the look counts once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
