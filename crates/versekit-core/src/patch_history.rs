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

//! Which game patch brought which blueprints (`patchhistory.py` of 4.12).
//!
//! Three files, kept apart on purpose, in the formats 4.12 writes:
//!
//! - `daten/patch-historie.json`, shipped with the program: the additions of
//!   the patches the project observed before this device did;
//! - `patch-historie.json` in the data folder: what this device observed
//!   itself ([`record`]);
//! - `bauplaene-gesehen.json` in the data folder: every blueprint this device
//!   ever saw in a catalog, as name keys ([`seen`], [`set_seen`]). That list,
//!   not the last catalog, is what a new catalog is compared with, so a
//!   source that loses entries for a while and brings them back does not
//!   make them „new“.
//!
//! Only the additions per game version are kept, never the whole catalog.
//! Names are stored as written and compared as name keys.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io;

use crate::formats;
use crate::jsonio::{self, Map, Newline, Value};
use crate::paths::{Paths, name_key};

/// The file of what this device observed, also the name of the shipped one.
pub const FILE: &str = "patch-historie.json";

/// Every blueprint ever seen.
pub const SEEN_FILE: &str = "bauplaene-gesehen.json";

const BUNDLED: &str = include_str!("../../../daten/patch-historie.json");

/// The blueprints one patch brought.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    /// The day the patch was observed, `YYYY-MM-DD`; the earlier of two.
    pub date: String,
    /// The names, as written, sorted.
    pub new: Vec<String>,
}

/// All patches by the full game version (`4.10.2-live.12913470`).
pub type History = BTreeMap<String, Patch>;

/// The sort key of a game version: `4.10.0` comes after `4.9.0`.
pub fn rank(version: &str) -> Vec<u64> {
    let short = version.split('-').next().unwrap_or_default();
    short
        .split('.')
        .map(|part| if part.bytes().all(|b| b.is_ascii_digit()) { part.parse().unwrap_or(0) } else { 0 })
        .collect()
}

/// The change number at the end of a version (`12913470` in
/// `4.10.2-live.12913470`), to order builds of one version by number and not
/// by text (`live.10` after `live.9`); 0 without one.
fn build_number(version: &str) -> u64 {
    version.rsplit('.').next().and_then(|tail| tail.parse().ok()).unwrap_or(0)
}

fn read_patches(root: &Value) -> History {
    let Some(Value::Object(patches)) = root.get("patches") else { return History::new() };
    patches
        .iter()
        .map(|(version, entry)| {
            let names = match entry.get("neu") {
                Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            let date = entry.get("datum").and_then(Value::as_str).unwrap_or_default().to_string();
            (version.clone(), Patch { date, new: names })
        })
        .collect()
}

/// Two entries of one game version as one: the union of the names (not a
/// replacement; what [`record`] writes is only the gain since the last
/// run), the earlier day.
fn merge(old: &Patch, new: &Patch) -> Patch {
    let mut names = old.new.clone();
    let mut known: HashSet<String> = names.iter().map(|n| name_key(n)).collect();
    for name in &new.new {
        if known.insert(name_key(name)) {
            names.push(name.clone());
        }
    }
    let date = [&old.date, &new.date].into_iter().filter(|d| !d.is_empty()).min().cloned().unwrap_or_default();
    names.sort();
    Patch { date, new: names }
}

fn merged(files: impl IntoIterator<Item = History>) -> History {
    let mut all = History::new();
    for file in files {
        for (version, patch) in file {
            let entry = match all.get(&version) {
                Some(old) => merge(old, &patch),
                None => patch,
            };
            all.insert(version, entry);
        }
    }
    all
}

fn shipped() -> History {
    jsonio::loads(BUNDLED).map(|root| read_patches(&root)).unwrap_or_default()
}

fn own(paths: &Paths) -> History {
    jsonio::read_json(&paths.app_file(FILE)).map(|root| read_patches(&root)).unwrap_or_default()
}

/// The whole history: the shipped part and this device's own findings;
/// patches of one version are united.
pub fn load(paths: &Paths) -> History {
    merged([shipped(), own(paths)])
}

fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0);
    formats::local_time(seconds).map(|t| formats::strftime("%Y-%m-%d", &t)).unwrap_or_default()
}

fn patches_value(patches: &History) -> Value {
    let entries: Map = patches
        .iter()
        .map(|(version, patch)| {
            let entry: Map = vec![
                ("datum".into(), Value::str(&patch.date)),
                ("neu".into(), Value::Array(patch.new.iter().map(Value::str).collect())),
            ];
            (version.clone(), Value::Object(entry))
        })
        .collect();
    Value::Object(vec![
        (
            "hinweis".into(),
            Value::str(
                "Welcher Patch welche Baupläne gebracht hat. Nur die Zugänge je Spielversion, nie der ganze Katalog.",
            ),
        ),
        ("quelle".into(), Value::str("eigene Beobachtung von Verse-Kit")),
        ("weitergabe".into(), Value::Bool(true)),
        ("patches".into(), Value::Object(entries)),
    ])
}

/// Writes a patch into this device's own history, added to what the version
/// holds already; the shipped file stays untouched. Nothing is written
/// without a version or without names. `date` is today when `None`.
///
/// # Errors
///
/// The error of writing the file; a refused write (write lock) is the
/// caller's to tell from a failed one (`writeguard::refused`).
pub fn record(paths: &Paths, version: &str, names: &[String], date: Option<&str>) -> io::Result<()> {
    if version.is_empty() || names.is_empty() {
        return Ok(());
    }
    let mut own = own(paths);
    let mut sorted = names.to_vec();
    sorted.sort();
    let gain = Patch { date: date.map_or_else(today, str::to_string), new: sorted };
    let entry = match own.get(version) {
        Some(old) => merge(old, &gain),
        None => gain,
    };
    own.insert(version.to_string(), entry);
    jsonio::write_text_atomic(
        &paths.app_file(FILE),
        &jsonio::dumps(&patches_value(&own), Some(1), false),
        Newline::Native,
    )
}

/// The name key of each blueprint to the game version it appeared in first.
/// A name that appears in several patches counts for the earliest.
pub fn version_per_blueprint(history: &History) -> HashMap<String, String> {
    let mut versions: Vec<(&String, &Patch)> = history.iter().collect();
    versions.sort_by_key(|(version, _)| (rank(version), build_number(version)));
    let mut found = HashMap::new();
    for (version, patch) in versions {
        for name in &patch.new {
            found.entry(name_key(name)).or_insert_with(|| version.clone());
        }
    }
    found
}

/// Every patch, newest first: the full version, the short one and how many
/// blueprints it brought.
pub fn patches(history: &History) -> Vec<(String, String, usize)> {
    let mut versions: Vec<&String> = history.keys().collect();
    versions.sort_by_key(|version| std::cmp::Reverse((rank(version), build_number(version))));
    versions
        .into_iter()
        .map(|version| {
            let short = version.split('-').next().filter(|s| !s.is_empty()).unwrap_or(version);
            (version.clone(), short.to_string(), history.get(version).map_or(0, |p| p.new.len()))
        })
        .collect()
}

/// Every blueprint this device ever saw, as name keys; empty when unknown.
pub fn seen(paths: &Paths) -> BTreeSet<String> {
    let Ok(root) = jsonio::read_json(&paths.app_file(SEEN_FILE)) else { return BTreeSet::new() };
    match root.get("namen") {
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => BTreeSet::new(),
    }
}

/// Replaces the list of everything ever seen.
///
/// # Errors
///
/// The error of writing the file (see [`record`]).
pub fn set_seen(paths: &Paths, keys: &BTreeSet<String>) -> io::Result<()> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0);
    let stand = formats::local_time(seconds).map(|t| formats::strftime("%Y-%m-%d %H:%M:%S", &t)).unwrap_or_default();
    let data = Value::Object(vec![
        ("stand".into(), Value::str(stand)),
        ("namen".into(), Value::Array(keys.iter().map(Value::str).collect())),
    ]);
    jsonio::write_text_atomic(&paths.app_file(SEEN_FILE), &jsonio::dumps(&data, None, false), Newline::Native)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(date: &str, names: &[&str]) -> Patch {
        Patch { date: date.into(), new: names.iter().map(|n| (*n).to_string()).collect() }
    }

    #[test]
    fn versions_sort_by_number_not_by_text() {
        assert!(rank("4.10.0-live.1") > rank("4.9.0-live.9"));
        assert_eq!(rank("4.10.2-live.12913470"), [4, 10, 2]);
        assert_eq!(rank(""), [0]);
    }

    #[test]
    fn patches_of_one_version_are_united_with_the_earlier_day() {
        let all = merged([
            History::from([("4.10.0".to_string(), patch("2026-08-28", &["B", "A"]))]),
            History::from([("4.10.0".to_string(), patch("2026-08-26", &["b", "C"]))]),
        ]);
        assert_eq!(all["4.10.0"], patch("2026-08-26", &["A", "B", "C"]));
    }

    #[test]
    fn a_blueprint_counts_for_its_earliest_version() {
        let history = History::from([
            ("4.10.0-live.2".to_string(), patch("d", &["Antium Helmet"])),
            ("4.9.0-live.1".to_string(), patch("d", &["ANTIUM HELMET", "Other"])),
        ]);
        let found = version_per_blueprint(&history);
        assert_eq!(found[&name_key("Antium Helmet")], "4.9.0-live.1");
        assert_eq!(found[&name_key("Other")], "4.9.0-live.1");
        assert_eq!(patches(&history)[0], ("4.10.0-live.2".to_string(), "4.10.0".to_string(), 1));
    }

    #[test]
    fn the_shipped_history_reads() {
        let history = shipped();
        assert!(!history.is_empty());
        assert!(history.values().all(|p| !p.new.is_empty() && !p.date.is_empty()));
    }
}
