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

//! The common ground of every list fetched from UEX (`uex.py` of 4.12): one
//! request, one stored file with its age and format number.
//!
//! What a list means stays with the module of the list; this module knows
//! only fetching and storing. All requests go through a function object
//! ([`Fetch`]), so tests and checks run without a network.
//!
//! * UEX answers 403 without a name of the program and cuts an answer silently
//!   at 500 rows; [`fetch`] reports the second to the error log.
//! * A stored file carries `format`, `geholt` (seconds since 1970) and, when
//!   the live game build is known, `spielstand`.
//! * A list bound to the patch (`patch_bound`) stays valid until a new game
//!   build is live; its `shelf_life` is only the emergency limit.

use crate::errors;
use crate::jsonio::{self, Newline, Value};
use crate::paths::Paths;
use crate::scmdb::FetchError;
use crate::stock_data::{format_is, py_float_value, text_of, truthy};
use crate::stock_fetch::{self, Fetch, compact, parse_uex};
use crate::writeguard;

/// The usual time between two fetches of the same list, in seconds.
pub const DAY: f64 = 24.0 * 60.0 * 60.0;
/// A week.
pub const WEEK: f64 = 7.0 * DAY;

/// UEX cuts an answer silently at this many rows.
pub const CAP: usize = 500;

/// Fetches an address and returns the `data` of the answer (`uex.fetch`).
///
/// `None` when the request fails: no network, a refused or an unreadable
/// answer. `label` names the list in the error log; a switched-off network
/// reports nothing. Names are freed of HTML characters.
pub fn fetch(label: &str, url: &str, fetch: Fetch<'_>) -> Option<Value> {
    match fetch(url) {
        Err(FetchError::Offline) => None,
        Err(error) => {
            errors::record(&format!("uex.fetch.{label}"), &error);
            None
        }
        Ok(bytes) => {
            let data = parse_uex(label, &bytes);
            if data.is_none() {
                errors::record(&format!("uex.fetch.{label}"), &"unexpected answer");
            }
            data
        }
    }
}

/// The rows of a list answer; an object or a scalar counts as no rows.
pub fn rows(data: &Value) -> &[Value] {
    match data {
        Value::Array(rows) => rows,
        _ => &[],
    }
}

/// Python's `float(x.get(key) or 0)` of a row: `0` where the field is absent
/// or empty, `None` where Python would raise.
pub fn number(row: &Value, key: &str) -> Option<f64> {
    match row.get(key) {
        value if !truthy(value) => Some(0.0),
        Some(value) => py_float_value(value),
        None => Some(0.0),
    }
}

/// Python's `int(x.get(key) or 0)` of a row.
pub fn whole(row: &Value, key: &str) -> Option<i64> {
    match row.get(key) {
        value if !truthy(value) => Some(0),
        Some(value) => stock_fetch::py_int(value),
        None => Some(0),
    }
}

/// `(x.get(key) or '').strip()` of a row.
pub fn stripped(row: &Value, key: &str) -> String {
    let value = row.get(key).filter(|value| truthy(Some(value)));
    match value {
        Some(Value::Str(text)) => crate::text::py_strip(text).to_string(),
        _ => String::new(),
    }
}

/// `x.get(a) or x.get(b) or …`, stripped: the first field that is set.
pub fn first_set(row: &Value, keys: &[&str]) -> String {
    for key in keys {
        let value = row.get(key).filter(|value| truthy(Some(value)));
        if let Some(Value::Str(text)) = value {
            return crate::text::py_strip(text).to_string();
        }
    }
    String::new()
}

/// A stored UEX list on the disk (`uex.Store`).
#[derive(Clone, Copy, Debug)]
pub struct Store {
    /// The file name in the data folder.
    pub file: &'static str,
    /// The format number; a file of another one counts as absent.
    pub format: i64,
    /// How long the file counts as fresh, in seconds.
    pub shelf_life: f64,
    /// Whether `save` writes the live game build into the file.
    pub stamp: bool,
    /// Whether a new game build makes the file stale at once.
    pub patch_bound: bool,
}

/// Why a file was not written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The write lock refused the write.
    Refused,
    /// The write failed.
    Failed(String),
}

impl Store {
    /// A store without a stamp of the game build and without patch binding.
    pub const fn new(file: &'static str, format: i64, shelf_life: f64) -> Store {
        Store { file, format, shelf_life, stamp: true, patch_bound: false }
    }

    /// The same store bound to the patch.
    pub const fn bound(self) -> Store {
        Store { patch_bound: true, ..self }
    }

    /// The same store without the stamp of the game build.
    pub const fn unstamped(self) -> Store {
        Store { stamp: false, ..self }
    }

    pub fn path(&self, paths: &Paths) -> String {
        paths.app_file(self.file)
    }

    /// The stored file, or `None` when none (of this format) is there.
    pub fn load(&self, paths: &Paths) -> Option<Value> {
        let value = jsonio::read_json(&self.path(paths)).ok()?;
        (matches!(value, Value::Object(_)) && format_is(&value, self.format)).then_some(value)
    }

    /// How old the file is in seconds, or `None` without one.
    pub fn age(&self, paths: &Paths, now: f64) -> Option<f64> {
        age_of(&self.load(paths)?, now)
    }

    /// Must the list be fetched again? Without a file: yes.
    pub fn stale(&self, paths: &Paths, now: f64) -> bool {
        let store = self.load(paths);
        self.stale_with(paths, store.as_ref(), now)
    }

    /// [`Store::stale`] for a file already read.
    pub fn stale_with(&self, paths: &Paths, store: Option<&Value>, now: f64) -> bool {
        let age = store.and_then(|store| age_of(store, now));
        if age.is_none_or(|age| age >= self.shelf_life) {
            return true;
        }
        self.patch_bound && store.is_some_and(|store| outdated(paths, store).0)
    }

    /// Writes the own fields, then `format`, `geholt` and the stamp. The write
    /// is atomic: the old or the new file is there, never half of one.
    pub fn save(
        &self,
        paths: &Paths,
        fields: Vec<(String, Value)>,
        compact_json: bool,
        now: f64,
    ) -> Result<(), SaveError> {
        let mut fields = fields;
        fields.push(("format".to_string(), Value::int(self.format)));
        fields.push(("geholt".to_string(), Value::Float(now)));
        if self.stamp {
            let build = stock_fetch::live_build(paths);
            if !build.is_empty() {
                fields.push(("spielstand".to_string(), Value::Str(build)));
            }
        }
        let data = Value::Object(fields);
        let text = if compact_json { compact(&data) } else { jsonio::dumps(&data, None, false) };
        match jsonio::write_text_atomic(&self.path(paths), &text, Newline::Keep) {
            Ok(()) => Ok(()),
            Err(error) if writeguard::refused(&error) => Err(SaveError::Refused),
            Err(error) => {
                errors::record(&format!("uex.save.{}", self.file), &error);
                Err(SaveError::Failed(error.to_string()))
            }
        }
    }
}

/// The age of a stored list: `geholt` against `now`.
pub fn age_of(store: &Value, now: f64) -> Option<f64> {
    let fetched = store.get("geholt").filter(|value| truthy(Some(value)))?;
    py_float_value(fetched).map(|then| now - then)
}

/// Is a patch between this list and the game? `(yes, build of the list,
/// live build)`. In doubt: no (`gamebuild.outdated`).
pub fn outdated(paths: &Paths, store: &Value) -> (bool, String, String) {
    let live = stock_fetch::live_build(paths);
    let before = text_of(store.get("spielstand")).to_string();
    if live.is_empty() || before.is_empty() {
        return (false, before, live);
    }
    (before != live, before, live)
}

/// A key-value pair for [`Value::Object`].
pub fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

/// A float field the way Python writes a `float`.
pub fn float(value: f64) -> Value {
    Value::Float(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_reads_like_python() {
        let row = jsonio::loads(r#"{"a": "12", "b": 3.9, "c": null, "d": "", "e": "x", "f": " Name "}"#).unwrap();
        assert_eq!(number(&row, "a"), Some(12.0));
        assert_eq!(number(&row, "b"), Some(3.9));
        assert_eq!(number(&row, "c"), Some(0.0));
        assert_eq!(number(&row, "d"), Some(0.0));
        assert_eq!(number(&row, "e"), None);
        assert_eq!(number(&row, "missing"), Some(0.0));
        assert_eq!(whole(&row, "b"), Some(3));
        assert_eq!(whole(&row, "a"), Some(12));
        assert_eq!(stripped(&row, "f"), "Name");
        assert_eq!(first_set(&row, &["c", "d", "f"]), "Name");
    }

    #[test]
    fn age_of_a_stored_list_is_counted_from_geholt() {
        let store = jsonio::loads(r#"{"geholt": 1000.0, "format": 1}"#).unwrap();
        assert_eq!(age_of(&store, 1500.0), Some(500.0));
        assert_eq!(age_of(&Value::Object(Vec::new()), 1500.0), None);
    }

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let folder = std::env::temp_dir().join(format!("versekit-uex-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &home)], &home);
        (Paths::new(host), folder)
    }

    #[test]
    fn a_saved_list_is_written_in_order_and_goes_stale_with_its_age() {
        let (paths, folder) = paths_in("save");
        let store = Store::new("liste.json", 3, 100.0);
        assert!(store.stale(&paths, 1000.0), "without a file");
        store.save(&paths, vec![field("a", Value::int(1))], true, 1000.0).unwrap();
        let text = std::fs::read_to_string(folder.join("liste.json")).unwrap();
        assert_eq!(text, r#"{"a":1,"format":3,"geholt":1000.0}"#);
        assert_eq!(store.age(&paths, 1050.0), Some(50.0));
        assert!(!store.stale(&paths, 1050.0));
        assert!(store.stale(&paths, 1100.0), "the shelf life is over");
        // A file of another format counts as absent.
        assert!(Store::new("liste.json", 4, 100.0).load(&paths).is_none());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_list_bound_to_the_patch_goes_stale_when_the_game_build_changes() {
        let (paths, folder) = paths_in("patch");
        let build = |live: &str| {
            std::fs::write(
                folder.join("spielstand.json"),
                format!(r#"{{"live":"{live}","ptu":"","format":1,"geholt":1.0}}"#),
            )
            .unwrap();
        };
        let bound = Store::new("gebunden.json", 1, 1e9).bound();
        let free = Store::new("frei.json", 1, 1e9);
        build("4.10.0");
        bound.save(&paths, vec![], true, 1000.0).unwrap();
        free.save(&paths, vec![], true, 1000.0).unwrap();
        let text = std::fs::read_to_string(folder.join("gebunden.json")).unwrap();
        assert!(text.contains(r#""spielstand":"4.10.0""#), "{text}");
        assert!(!bound.stale(&paths, 1001.0));
        build("4.11.0");
        assert!(bound.stale(&paths, 1001.0), "a new build makes it stale at once");
        assert!(!free.stale(&paths, 1001.0), "a list not bound to the patch stands");
        // In doubt it stands: no known live build, no warning.
        std::fs::remove_file(folder.join("spielstand.json")).unwrap();
        assert!(!bound.stale(&paths, 1001.0));
        let _ = std::fs::remove_dir_all(folder);
    }
}
