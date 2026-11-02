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

//! Reading in an existing blueprint stand (`importer.py`).
//!
//! The player picks a file; the format is told from the content:
//!
//! | Format | Mark |
//! |---|---|
//! | own backup | `werkzeug` of Verse-Kit (or the old name), list `bauplaene` |
//! | scmdb.net (older) | `exportSchemaVersion` or `ts`, `blueprints[].productName` |
//! | scmdb.net (newer) | `blueprints[].tag` + `name`, only `completed: true` |
//! | KRT Profit Basetool | `blueprints[].productName` (+ `receivedAt`) |
//! | Basetool envelope (SC Extractor and others) | `format: "basetool.blueprints"`, `formatVersion` 1.x, `items[].ref` |
//! | Blueprint DB (Star Citizen Deutsch) | `blueprints[].key` with `isDone` or `isMarked`, only `isDone: true` |
//!
//! The format of the SC Deutsch Launcher (`blueprints[].key` alone) is not
//! read any more: there is nothing left to fetch from there.
//!
//! **Merging, never replacing.** What is there stays, new comes in. Names the
//! catalog does not know come in too and are marked: one entry too few is
//! worse than one too many.
//!
//! This module decides nothing alone: [`preview`] says what would happen;
//! only [`merge`] writes.

use std::collections::BTreeSet;

use crate::collection::Collection;
use crate::formats;
use crate::jsonio::{self, Map, Value};
use crate::paths::name_key;
use crate::text::py_strip;

/// The source of an imported blueprint in the collection.
pub const SOURCE: &str = "import";

/// The names the program's own export files carry; both stay for good so old backups still read.
pub const OWN_TOOL_NAMES: [&str; 2] = ["VerseKit", crate::legacy::EXPORT_TOOL_NAME];

pub const ENVELOPE_FORMAT: &str = "basetool.blueprints";

/// Which format a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Own,
    ScmdbOld,
    ScmdbNew,
    /// The Basetool: the older output and the envelope read to the player under one name.
    Basetool,
    BlueprintDb,
}

impl Kind {
    /// The identifier the pages take the name for the display from.
    pub fn code(self) -> &'static str {
        match self {
            Kind::Own => "eigen",
            Kind::ScmdbOld => "scmdb",
            Kind::ScmdbNew => "scmdb2",
            Kind::Basetool => "basetool",
            Kind::BlueprintDb => "bpdb",
        }
    }
}

/// A blueprint of a file: its name and, if the file has one, when it was got (local time).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub time: Option<String>,
}

fn list<'a>(data: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    match data.get(key) {
        Some(Value::Array(items)) => Some(items),
        _ => None,
    }
}

fn text<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

fn truthy(entry: &Value, key: &str) -> bool {
    entry.get(key).is_some_and(crate::basetool::error::truthy)
}

fn has(entry: &Value, key: &str) -> bool {
    entry.get(key).is_some()
}

/// The name without the bracket addition at the end, for the fallback match.
fn strip_suffix(name: &str) -> String {
    let name = py_strip(name);
    match name.rfind('(') {
        Some(open) if name.ends_with(')') && !name[open + 1..name.len() - 1].contains(['(', ')']) => {
            py_strip(&name[..open]).to_string()
        }
        _ => name.to_string(),
    }
}

/// Is this the envelope `basetool.blueprints` in version 1.x? ⚠ Another major version counts as
/// an unknown file: the Basetool itself reads only 1.x, and what changes in 2.x is written nowhere.
fn is_envelope(data: &Value) -> bool {
    if text(data, "format") != Some(ENVELOPE_FORMAT) {
        return false;
    }
    let major = text(data, "formatVersion").and_then(|v| v.split('.').next());
    major == Some("1") && list(data, "items").is_some()
}

/// Which format the loaded file is; `None` for an unknown one.
pub fn detect(data: &Value) -> Option<Kind> {
    if !matches!(data, Value::Object(_)) {
        return None;
    }
    if is_envelope(data) {
        return Some(Kind::Basetool);
    }
    if text(data, "werkzeug").is_some_and(|w| OWN_TOOL_NAMES.contains(&w)) || has(data, "bauplaene") {
        return Some(Kind::Own);
    }
    let items = list(data, "blueprints").filter(|items| !items.is_empty())?;
    let first = if matches!(items[0], Value::Object(_)) { &items[0] } else { &Value::Object(Map::new()) };
    if has(first, "key") {
        // ⚠ The Blueprint DB writes `isDone` ("have") and `isMarked` ("want") on every entry; its "all as
        // JSON" holds every blueprint of the database, most of them `isDone: false`. All entries are
        // looked at, not the first: which one stands first is the sorting of the page.
        let marked = items.iter().any(|e| matches!(e, Value::Object(_)) && (has(e, "isDone") || has(e, "isMarked")));
        return marked.then_some(Kind::BlueprintDb);
    }
    if has(data, "exportSchemaVersion") || has(first, "ts") {
        return Some(Kind::ScmdbOld);
    }
    if has(first, "productName") {
        return Some(Kind::Basetool);
    }
    // Told by `tag` + `name`, not by `version`: the number would be another one at the next change.
    (has(first, "tag") && has(first, "name")).then_some(Kind::ScmdbNew)
}

/// A time in our writing (local time `YYYY-MM-DD HH:MM:SS`) or nothing.
///
/// ⚠ A time with `Z` or `+02:00` is NOT local time: whoever only cuts the `Z` puts a Basetool file
/// (`receivedAt` in UTC) two hours too early into the stand in summer. So it is worked out with the zone;
/// without a zone the value stays as it is.
fn time_from(value: Option<&Value>) -> Option<String> {
    let local = |stamp: i64| formats::local_time(stamp).map(|t| formats::strftime("%Y-%m-%d %H:%M:%S", &t));
    match value? {
        Value::Int(n) => n.parse::<i64>().ok().filter(|n| *n > 0).and_then(local),
        Value::Float(x) if *x > 0.0 => formats::floor_seconds(*x).and_then(local),
        // `true` is the number 1 in Python: one second after 1970.
        Value::Bool(true) => local(1),
        Value::Str(raw) if !py_strip(raw).is_empty() => {
            let raw = py_strip(raw);
            match zoned(raw) {
                Zoned::At(stamp) => local(stamp),
                // A time with a zone whose date or clock cannot be is no time at all.
                Zoned::Impossible => None,
                Zoned::No => Some(raw.replace('T', " ").chars().take(19).collect()),
            }
        }
        _ => None,
    }
}

/// What a text with a zone says.
enum Zoned {
    /// Seconds since 1970 (UTC).
    At(i64),
    /// Written with a zone, but the date or the clock cannot be (`2026-02-31`, `25:00`).
    Impossible,
    /// No time with a zone.
    No,
}

/// `2026-10-21T07:28:00.123Z` or with `+02:00` as seconds since 1970 (UTC).
fn zoned(raw: &str) -> Zoned {
    static ZONED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^(\d{4})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:([Zz])|([+-])(\d{2}):?(\d{2}))$",
        )
        .expect("pattern")
    });
    let Some(c) = ZONED.captures(raw) else { return Zoned::No };
    let number = |at: usize| c.get(at).and_then(|m| m.as_str().parse::<i64>().ok()).unwrap_or(0);
    let (year, month, day) = (number(1), number(2), number(3));
    let (hour, minute, second) = (number(4), number(5), number(6));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    // As `time.strptime` takes them: seconds up to 61.
    if year < 1
        || !(1..=12).contains(&month)
        || !(1..=month_days).contains(&day)
        || hour > 23
        || minute > 59
        || second > 61
    {
        return Zoned::Impossible;
    }
    let mut stamp = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    if c.get(7).is_none() {
        let offset = number(9) * 3600 + number(10) * 60;
        stamp -= if c.get(8).map(|m| m.as_str()) == Some("+") { offset } else { -offset };
    }
    Zoned::At(stamp)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The entries of an envelope of the Basetool.
///
/// The name comes from `ref.name`, else from `ref.locKey` through `resolve_key` (the way of a find
/// in the log, so the same blueprint does not land under two names). A key nowhere translated is
/// made readable by `resolve_key`. ⚠ An entry that carries only `bt`, `scRecord`, `scGuid` or `uexId`
/// has no name without the net and stays out. Time: `acquiredAt`, else `provenance.observedAt`
/// (both UTC).
fn envelope_entries(items: &[Value], resolve_key: &dyn Fn(&str) -> String) -> Vec<Entry> {
    let mut out = Vec::new();
    for item in items {
        let Some(reference @ Value::Object(_)) = item.get("ref") else { continue };
        let mut name = text(reference, "name").map(|n| py_strip(n).to_string()).unwrap_or_default();
        if name.is_empty() {
            let key = text(reference, "locKey").map(|k| py_strip(k).trim_start_matches('@').to_string());
            if let Some(key) = key.filter(|k| !k.is_empty()) {
                name = resolve_key(&format!("@{key}"));
                // A key with characters outside the unresolved ones comes back unchanged: without the `@`.
                if name.starts_with('@') {
                    name = key;
                }
            }
        }
        if name.is_empty() {
            continue;
        }
        let observed = item.get("provenance").and_then(|p| p.get("observedAt"));
        let when = item.get("acquiredAt").filter(|v| crate::basetool::error::truthy(v)).or(observed);
        out.push(Entry { name, time: time_from(when) });
    }
    out
}

/// Reads a file: the format and its blueprints. An unreadable or unknown file gives `None` (the page
/// says so; no exception in the middle of a flow).
pub fn read(path: &str, resolve_key: &dyn Fn(&str) -> String) -> (Option<Kind>, Vec<Entry>) {
    let Ok(bytes) = std::fs::read(path) else { return (None, Vec::new()) };
    let Ok(raw) = String::from_utf8(bytes) else { return (None, Vec::new()) };
    let Ok(data) = jsonio::loads(raw.strip_prefix('\u{feff}').unwrap_or(&raw)) else { return (None, Vec::new()) };
    read_value(&data, resolve_key)
}

/// [`read`] for data already loaded.
pub fn read_value(data: &Value, resolve_key: &dyn Fn(&str) -> String) -> (Option<Kind>, Vec<Entry>) {
    let kind = detect(data);
    let mut entries = Vec::new();
    let blueprints = list(data, "blueprints").map(Vec::as_slice).unwrap_or_default();
    match kind {
        Some(Kind::Basetool) if is_envelope(data) => {
            entries = envelope_entries(list(data, "items").map(Vec::as_slice).unwrap_or_default(), resolve_key);
        }
        Some(Kind::Own) => {
            for e in list(data, "bauplaene").map(Vec::as_slice).unwrap_or_default() {
                if let Some(name) = text(e, "name").filter(|n| !n.is_empty()) {
                    entries.push(Entry { name: name.to_string(), time: time_from(e.get("zeit")) });
                }
            }
        }
        Some(Kind::ScmdbOld | Kind::Basetool) => {
            for e in blueprints {
                if let Some(name) = text(e, "productName").filter(|n| !n.is_empty()) {
                    let when = e.get("ts").filter(|v| crate::basetool::error::truthy(v)).or(e.get("receivedAt"));
                    entries.push(Entry { name: name.to_string(), time: time_from(when) });
                }
            }
        }
        // ⚠ Only what is marked done: the export holds also blueprints someone only watched; `completed`
        // is the field for owning. There is no time in this format: none is better than an invented one.
        Some(Kind::ScmdbNew) => {
            for e in blueprints {
                if let Some(name) = text(e, "name").filter(|n| !n.is_empty())
                    && truthy(e, "completed")
                {
                    entries.push(Entry { name: name.to_string(), time: None });
                }
            }
        }
        // ⚠ Only what is played: `isMarked` is a wish list, not owning, and `isDone: false` is "do not have".
        Some(Kind::BlueprintDb) => {
            for e in blueprints {
                if let Some(name) = text(e, "key").filter(|n| !n.is_empty())
                    && truthy(e, "isDone")
                {
                    entries.push(Entry { name: name.to_string(), time: None });
                }
            }
        }
        None => {}
    }
    (kind, entries)
}

/// What would happen; changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    pub new: Vec<String>,
    pub present: Vec<String>,
    /// New names the catalog does not know: they come in too, listed apart so the progress number stays explicable.
    pub unknown: Vec<String>,
    pub total: usize,
}

/// `preview(entries, data, catalog_names)`.
pub fn preview(entries: &[Entry], data: &Map, catalog_names: &[String]) -> Preview {
    let present: BTreeSet<String> = match data.iter().find(|(k, _)| k == "bauplaene") {
        Some((_, Value::Object(map))) => map.iter().map(|(k, _)| k.clone()).collect(),
        _ => BTreeSet::new(),
    };
    let known: BTreeSet<String> = catalog_names.iter().map(|n| name_key(n)).collect();
    let known_short: BTreeSet<String> = catalog_names.iter().map(|n| name_key(&strip_suffix(n))).collect();
    let mut out = Preview::default();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let key = name_key(&entry.name);
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if present.contains(&key) {
            out.present.push(entry.name.clone());
            continue;
        }
        out.new.push(entry.name.clone());
        if !known.is_empty() && !known.contains(&key) {
            // A second try without the bracket addition, but only where it is unambiguous: else
            // `Singe Cannon (S1)/(S2)/(S3)` would melt.
            let short = name_key(&strip_suffix(&entry.name));
            if !known_short.contains(&short) {
                out.unknown.push(entry.name.clone());
            }
        }
    }
    out.total = seen.len();
    out
}

/// Takes the entries into the stand; the number of new ones. What is there stays untouched; a time from
/// the file is taken over: it is more exact than "just read in".
pub fn merge(collection: &Collection<'_>, entries: &[Entry], data: &mut Map, save: bool) -> usize {
    let added = entries.iter().filter(|e| collection.add(data, &e.name, SOURCE, e.time.as_deref())).count();
    if save && added > 0 {
        collection.save(data);
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Value {
        jsonio::loads(text).expect("json")
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    /// The resolver of the tests: one key is translated, the others come back unchanged.
    fn plain(key: &str) -> String {
        if key == "@bp_x" { "Bauplan X".to_string() } else { key.to_string() }
    }

    #[test]
    fn the_envelope_of_the_basetool_is_read_by_name_then_by_key_and_by_its_times() {
        let data = value(
            r#"{"format": "basetool.blueprints", "formatVersion": "1.2", "items": [
              {"ref": {"name": " Karna Rifle "}, "acquiredAt": "2026-07-01T10:00:00Z"},
              {"ref": {"locKey": "@bp_x"}, "provenance": {"observedAt": "2026-07-02T10:00:00+02:00"}},
              {"ref": {"locKey": "bp_y"}},
              {"ref": {"bt": "only-a-key"}},
              {"ref": {"name": ""}},
              "no object", {"nothing": 1}]}"#,
        );
        let (kind, entries) = read_value(&data, &plain);
        assert_eq!(kind, Some(Kind::Basetool));
        // A translated key gives its name; one that comes back unchanged is made plain without the `@`.
        assert_eq!(names(&entries), ["Karna Rifle", "Bauplan X", "bp_y"]);
        // The times come out in local time, worked out with their zone.
        let local = |stamp| formats::local_time(stamp).map(|t| formats::strftime("%Y-%m-%d %H:%M:%S", &t));
        assert_eq!(entries[0].time, local(at("2026-07-01T10:00:00Z")));
        assert_eq!(entries[1].time, local(at("2026-07-02T08:00:00Z")), "+02:00 is two hours less");
        assert_eq!(entries[2].time, None);
    }

    fn at(raw: &str) -> i64 {
        match zoned(raw) {
            Zoned::At(stamp) => stamp,
            _ => panic!("no time: {raw}"),
        }
    }

    #[test]
    fn a_zoned_time_that_cannot_be_is_no_time_and_true_is_one_second() {
        let time = |raw: &str| time_from(Some(&Value::str(raw)));
        for impossible in
            ["2026-02-31T10:00:00Z", "2026-13-01T00:00:00Z", "2026-07-01T25:00:00+02:00", "2025-02-29T00:00:00Z"]
        {
            assert_eq!(time(impossible), None, "{impossible}");
        }
        assert!(time("2024-02-29T00:00:00Z").is_some(), "a leap day is a day");
        assert_eq!(time("2026-07-01T25:00:00"), Some("2026-07-01 25:00:00".to_string()), "without a zone: as written");
        let one = formats::local_time(1).map(|t| formats::strftime("%Y-%m-%d %H:%M:%S", &t));
        assert_eq!(time_from(Some(&Value::Bool(true))), one);
        assert_eq!(time_from(Some(&Value::Bool(false))), None);
    }

    #[test]
    fn a_key_that_comes_back_unchanged_loses_its_at_sign() {
        let data = value(r#"{"format": "basetool.blueprints", "formatVersion": "1.0", "items": [{"ref": {"locKey": "bp z"}}]}"#);
        let (_, entries) = read_value(&data, &|key: &str| key.to_string());
        assert_eq!(names(&entries), ["bp z"]);
    }

    #[test]
    fn another_major_version_of_the_envelope_is_an_unknown_file() {
        let data = value(r#"{"format": "basetool.blueprints", "formatVersion": "2.0", "items": [{"ref": {"name": "A"}}]}"#);
        assert_eq!(detect(&data), None);
        let data = value(r#"{"format": "basetool.blueprints", "formatVersion": 1, "items": []}"#);
        assert_eq!(detect(&data), None, "the version is a text");
        let data = value(r#"{"format": "basetool.blueprints", "formatVersion": "1.0"}"#);
        assert_eq!(detect(&data), None, "no list of items");
    }

    #[test]
    fn each_format_is_told_from_its_content_and_reads_only_what_is_owned() {
        let own = value(&format!(
            r#"{{"werkzeug": "{}", "bauplaene": [{{"name": "A", "zeit": "2026-01-02 03:04:05"}}, {{"zeit": "x"}}]}}"#,
            crate::legacy::EXPORT_TOOL_NAME
        ));
        let (kind, entries) = read_value(&own, &plain);
        assert_eq!((kind, names(&entries)), (Some(Kind::Own), vec!["A"]));
        assert_eq!(entries[0].time.as_deref(), Some("2026-01-02 03:04:05"), "no zone: as it is");

        let old = value(r#"{"exportSchemaVersion": 1, "blueprints": [{"productName": "B", "ts": 0}, {"productName": "C"}]}"#);
        let (kind, entries) = read_value(&old, &plain);
        assert_eq!((kind, names(&entries)), (Some(Kind::ScmdbOld), vec!["B", "C"]));
        assert_eq!(entries[0].time, None, "a zero time is no time");

        let basetool = value(r#"{"blueprints": [{"productName": "D", "receivedAt": "2026-03-04T05:06:07"}]}"#);
        let (kind, entries) = read_value(&basetool, &plain);
        assert_eq!((kind, names(&entries)), (Some(Kind::Basetool), vec!["D"]));
        assert_eq!(entries[0].time.as_deref(), Some("2026-03-04 05:06:07"));

        let new = value(r#"{"version": 3, "blueprints": [{"tag": "t", "name": "E", "completed": true}, {"tag": "u", "name": "F", "completed": false}]}"#);
        let (kind, entries) = read_value(&new, &plain);
        assert_eq!((kind, names(&entries)), (Some(Kind::ScmdbNew), vec!["E"]), "only what is completed");

        // ⚠ The DB: the first entry may carry neither switch, the others do; only `isDone` counts.
        let db = value(r#"{"blueprints": [{"key": "G"}, {"key": "H", "isDone": true}, {"key": "I", "isMarked": true}, {"key": "J", "isDone": false}]}"#);
        let (kind, entries) = read_value(&db, &plain);
        assert_eq!((kind, names(&entries)), (Some(Kind::BlueprintDb), vec!["H"]));

        // The launcher's format is not read any more, and neither is a list of anything else.
        let launcher = value(r#"{"blueprints": [{"key": "K"}]}"#);
        assert_eq!(read_value(&launcher, &plain), (None, Vec::new()));
        assert_eq!(read_value(&value("[1, 2]"), &plain), (None, Vec::new()));
        assert_eq!(read_value(&value(r#"{"blueprints": []}"#), &plain), (None, Vec::new()));
    }

    #[test]
    fn a_file_is_read_with_or_without_a_byte_order_mark_and_a_broken_one_is_unknown() {
        let dir = std::env::temp_dir().join(format!("versekit-importer-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join("bestand.json").to_string_lossy().into_owned();
        std::fs::write(&file, "\u{feff}{\"bauplaene\": [{\"name\": \"A\"}]}").expect("file");
        assert_eq!(read(&file, &plain).0, Some(Kind::Own));
        std::fs::write(&file, "{kaputt").expect("file");
        assert_eq!(read(&file, &plain), (None, Vec::new()));
        std::fs::write(&file, [0xff, 0xfe, 0x00]).expect("file");
        assert_eq!(read(&file, &plain), (None, Vec::new()));
        assert_eq!(read(&format!("{file}.fehlt"), &plain), (None, Vec::new()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_preview_splits_into_new_present_and_unknown_and_counts_a_name_once() {
        let data = value(r#"{"bauplaene": {"karna rifle": {"name": "Karna Rifle"}}}"#);
        let Value::Object(data) = data else { panic!("object") };
        let entries: Vec<Entry> = ["Karna Rifle", "KARNA RIFLE", "Singe Cannon (S2)", "Singe Cannon (S4)", "Typo Gun", ""]
            .iter()
            .map(|n| Entry { name: (*n).to_string(), time: None })
            .collect();
        let catalog = ["Karna Rifle", "Singe Cannon (S1)", "Singe Cannon (S2)", "Singe Cannon (S3)"].map(String::from);
        let got = preview(&entries, &data, &catalog);
        assert_eq!(got.present, ["Karna Rifle"]);
        assert_eq!(got.new, ["Singe Cannon (S2)", "Singe Cannon (S4)", "Typo Gun"]);
        // Without the bracket the name is ambiguous in the catalog, so it counts as unknown only where
        // the short form is unknown as well: `(S4)` has a short form the catalog knows, `Typo Gun` not.
        assert_eq!(got.unknown, ["Typo Gun"]);
        assert_eq!(got.total, 4);
        // Without a catalog nothing is unknown.
        assert!(preview(&entries, &data, &[]).unknown.is_empty());
    }

    #[test]
    fn merging_takes_in_what_is_new_with_the_time_of_the_file_and_leaves_what_is_there() {
        use crate::collection::{self, NoCatalog, NoExports};
        use crate::phrases::{BackRoute, NoArchive};
        let dir = std::env::temp_dir().join(format!("versekit-importer-merge-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let data_dir = dir.to_string_lossy().replace('\\', "/");
        let host = crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, &data_dir)],
            &data_dir,
        );
        let paths = crate::paths::Paths::new(host);
        let names = BackRoute { paths: &paths, archive: &NoArchive };
        let collection = Collection {
            paths: &paths,
            catalog: &NoCatalog,
            names: &names,
            exports: &NoExports,
            clock: collection::now_text,
        };
        let mut data = collection.load();
        assert!(collection.add(&mut data, "Old One", "log", Some("2026-01-01 00:00:00")));
        let entries = [
            Entry { name: "Old One".into(), time: Some("2026-02-02 02:02:02".into()) },
            Entry { name: "New One".into(), time: Some("2026-03-04 05:06:07".into()) },
        ];
        assert_eq!(merge(&collection, &entries, &mut data, true), 1);
        let stored = collection.load();
        let find = |name: &str| {
            collection::blueprints(&stored)
                .iter()
                .find(|(k, _)| *k == name_key(name))
                .map(|(_, e)| (text(e, "quelle").map(String::from), text(e, "zeit").map(String::from)))
        };
        assert_eq!(find("New One"), Some((Some("import".into()), Some("2026-03-04 05:06:07".into()))));
        assert_eq!(find("Old One"), Some((Some("log".into()), Some("2026-01-01 00:00:00".into()))), "untouched");
        // Nothing new: nothing written.
        let mut again = collection.load();
        assert_eq!(merge(&collection, &entries, &mut again, true), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bracket_addition_is_cut_only_at_the_end_and_only_one() {
        assert_eq!(strip_suffix("Singe Cannon (S2)"), "Singe Cannon");
        assert_eq!(strip_suffix("A (x) (y)"), "A (x)");
        assert_eq!(strip_suffix("A (b (c))"), "A (b (c))");
        assert_eq!(strip_suffix("A"), "A");
    }
}
