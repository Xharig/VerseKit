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

//! The user's own blueprint collection, the list of "which ones do I have"
//! (`bestand.json`).
//!
//! Every blueprint that shows up in the Game.log is recorded permanently:
//!
//! ```json
//! {"version": 3, "stand": "2026-08-24 02:31:00",
//!  "bauplaene": {"7ca 'nargun'": {"name": "7CA 'Nargun'", "quelle": "log",
//!                                 "zeit": "2026-08-24 02:31:00"}}}
//! ```
//!
//! The key is `paths::name_key` of the name. Sources: `log`, `nachlese`
//! (log backup), `launcher`, `import`, `start` (starter blueprint), `hand`
//! (ticked off manually) and `basetool`.
//!
//! ⚠ Stored strings stay German because they are in every user's files:
//! `bestand.json`, `bestand.bak.json`, `hochwasser.json`, the keys `version`,
//! `stand`, `bauplaene`, `name`, `quelle`, `zeit`, `ordner`, the source values
//! and `unbekannt` in `by_source`.
//!
//! The collection stays a JSON object (`jsonio::Map`) so that unknown fields
//! and values are preserved unchanged when saving.
//!
//! Gaps for components not yet ported: the catalog (`Catalog`) and the
//! original names from the `Data.p4k` (`phrases::ArchiveText`). The output
//! files come from `export::FileExports`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::sync::{LazyLock, OnceLock};

use regex::Regex;

use crate::errors;
use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::{Paths, name_key, py_int};
use crate::phrases::EnglishNames;
use crate::text::{py_isspace, py_strip};

/// File format version. 3: `name_key` also normalizes the language of the
/// quantity label; older collections are migrated on load.
pub const FILE_VERSION: i64 = 3;

/// Ranking of the sources: an entry is only upgraded, never downgraded.
///
/// ⚠ The log is at the top: `restorable` counts by this field what comes back
/// on its own after a reset. `basetool` (no proof in the game) is at the very
/// bottom.
pub const RANK: [(&str, i32); 7] =
    [("basetool", 0), ("launcher", 1), ("import", 1), ("hand", 2), ("start", 3), ("log", 4), ("nachlese", 4)];

/// Verse-Kit rebuilds these sources on its own after a reset.
pub const RESTORABLE: [&str; 3] = ["log", "nachlese", "start"];

/// From this many words on, matching by word set is allowed.
pub const MIN_WORDS: usize = 2;

/// The collection file in the app folder.
pub const FILE_NAME: &str = "bestand.json";

/// The mark holding the highest blueprint count ever seen.
pub const HIGH_WATER_FILE: &str = "hochwasser.json";

/// A trailing parenthesis at the end of the name.
static SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\x1c-\x1f]*\([^()]*\)[\s\x1c-\x1f]*\z").expect("fester Ausdruck"));

/// Rank of a source; anything unknown counts as 0.
pub fn rank(source: Option<&Value>) -> i32 {
    match source {
        Some(Value::Str(s)) => RANK.iter().find(|(name, _)| name == s).map_or(0, |(_, r)| *r),
        _ => 0,
    }
}

/// Local time as `YYYY-MM-DD HH:MM:SS`.
pub fn now_text() -> String {
    let (y, mo, d, h, mi, s) = jsonio::local_time();
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

// ------------------------------------------------------------ Interfaces

/// The blueprints the catalog knows: comparison key → display name (`n`), if
/// the entry has one.
#[derive(Clone, Default)]
pub struct Known {
    entries: HashMap<String, Option<String>>,
    words: OnceLock<Vec<(String, HashSet<String>)>>,
}

impl Known {
    pub fn new(entries: impl IntoIterator<Item = (String, Option<String>)>) -> Known {
        Known { entries: entries.into_iter().collect(), words: OnceLock::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// The display name for a key: `None` if the key is missing;
    /// `Some(None)` if the entry carries no name.
    pub fn get(&self, key: &str) -> Option<Option<&str>> {
        self.entries.get(key).map(|n| n.as_deref().filter(|s| !s.is_empty()))
    }

    /// Each key with its word set, computed once.
    fn words(&self) -> &[(String, HashSet<String>)] {
        self.words.get_or_init(|| self.entries.keys().map(|k| (k.clone(), split_words(k))).collect())
    }
}

/// The catalog, counterpart to `catalog.load()['bauplaene']`.
///
/// ⚠ Gap: the catalog has not been ported yet. An error while loading means
/// "leave names unchanged".
pub trait Catalog {
    fn known(&self) -> Result<Known, String>;
}

/// No catalog: empty, like a missing catalog file.
pub struct NoCatalog;

impl Catalog for NoCatalog {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::default())
    }
}

/// The output files (Basetool, scmdb, full backup, contract log), after every
/// save (`export::FileExports`).
pub trait Exports {
    fn archive(&self, data: &Map) -> Result<(), String>;
}

/// No output.
pub struct NoExports;

impl Exports for NoExports {
    fn archive(&self, _data: &Map) -> Result<(), String> {
        Ok(())
    }
}

/// Fewer blueprints than ever before: now, high-water mark, folder from back then.
#[derive(Clone, Debug, PartialEq)]
pub struct Shrinkage {
    pub now: usize,
    pub highest: i128,
    pub folder: Value,
}

impl Shrinkage {
    /// The folder from back then as text (`str()`); `None` where it is empty or missing.
    pub fn folder_text(&self) -> Option<String> {
        truthy(&self.folder).then(|| crate::basetool::error::py_str(&self.folder))
    }
}

// ------------------------------------------------------------ JSON helpers

fn get<'m>(map: &'m Map, key: &str) -> Option<&'m Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn get_mut<'m>(map: &'m mut Map, key: &str) -> Option<&'m mut Value> {
    map.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Like `dict.pop(key, None)`.
fn pop(map: &mut Map, key: &str) -> Option<Value> {
    let pos = map.iter().position(|(k, _)| k == key)?;
    Some(map.remove(pos).1)
}

/// Truth value as in Python.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(text) => text.trim_start_matches('-').bytes().any(|b| b != b'0'),
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Like `str(value)` for the values that can occur in a collection.
///
/// ⚠ Python writes lists and objects with `repr`; here they appear as JSON.
pub(crate) fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(text) => text.clone(),
        Value::Float(x) if x.is_nan() => "nan".into(),
        Value::Float(x) if x.is_infinite() => if *x > 0.0 { "inf" } else { "-inf" }.into(),
        Value::Float(x) => jsonio::float_text(*x),
        Value::Str(s) => s.clone(),
        other => jsonio::dumps(other, None, false),
    }
}

/// Field of an entry; an entry that is not an object has no fields.
fn field<'v>(entry: &'v Value, key: &str) -> Option<&'v Value> {
    entry.as_object().and_then(|m| get(m, key))
}

/// `entry.get(key) or fallback` as text.
fn field_or(entry: &Value, key: &str, fallback: &str) -> String {
    field(entry, key).filter(|v| truthy(v)).map_or_else(|| fallback.to_string(), py_str)
}

/// Words like Python's `str.split()`.
fn split_words(text: &str) -> HashSet<String> {
    text.split(py_isspace).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// The blueprints of a collection; empty if the field is missing.
pub fn blueprints(data: &Map) -> &[(String, Value)] {
    match get(data, "bauplaene") {
        Some(Value::Object(map)) => map,
        _ => &[],
    }
}

/// The blueprints for modification; if the field is missing, it is created.
fn blueprints_mut(data: &mut Map) -> &mut Map {
    if !matches!(get(data, "bauplaene"), Some(Value::Object(_))) {
        map_set(data, "bauplaene", Value::Object(Map::new()));
    }
    match get_mut(data, "bauplaene") {
        Some(Value::Object(map)) => map,
        _ => unreachable!("eben angelegt"),
    }
}

/// `data.get('version', 1) < FILE_VERSION`; `None` where Python would abort
/// with `TypeError` (text, list, object, `null`).
fn version_below(version: Option<&Value>) -> Option<bool> {
    match version {
        None => Some(true),
        Some(Value::Bool(_)) => Some(true),
        Some(Value::Int(text)) => {
            Some(text.parse::<i128>().map_or(text.starts_with('-'), |n| n < i128::from(FILE_VERSION)))
        }
        Some(Value::Float(x)) => Some(*x < FILE_VERSION as f64),
        Some(_) => None,
    }
}

// ------------------------------------------------------------ Stateless reading

/// Run stored keys through `name_key` again.
///
/// ⚠ Older collections carry keys with a class suffix (`xl-1 (mil/2/a)`) or a
/// quantity word; the list would not find them. If two old keys map to the
/// same new one, the older find wins; without a time, the higher rank wins.
/// True if a key changed.
pub fn renew_keys(data: &mut Map) -> bool {
    let old: Map = match get(data, "bauplaene") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let mut fresh_map = Map::new();
    let mut changed = false;
    for (key, entry) in old {
        let entry = if matches!(entry, Value::Object(_)) { entry } else { Value::Object(Map::new()) };
        let fresh = name_key(&field_or(&entry, "name", &key));
        if fresh != key {
            changed = true;
        }
        let Some(present) = get_mut(&mut fresh_map, &fresh) else {
            fresh_map.push((fresh, entry));
            continue;
        };
        // Merge the duplicate
        let old_time = field(present, "zeit").filter(|v| truthy(v)).map(py_str).unwrap_or_default();
        let new_time = field(&entry, "zeit").filter(|v| truthy(v)).map(py_str).unwrap_or_default();
        if !new_time.is_empty() && (old_time.is_empty() || new_time < old_time) {
            let mut entry = entry;
            if let Value::Object(map) = &mut entry
                && get(map, "quelle").is_none()
            {
                map.push(("quelle".into(), field(present, "quelle").cloned().unwrap_or(Value::Null)));
            }
            *present = entry;
        } else if rank(field(&entry, "quelle")) > rank(field(present, "quelle")) {
            let source = field(&entry, "quelle").cloned().unwrap_or(Value::Null);
            if let Value::Object(map) = present {
                map_set(map, "quelle", source);
            }
        }
    }
    if changed {
        map_set(data, "bauplaene", Value::Object(fresh_map));
    }
    changed
}

/// Remove a blueprint from the collection (untick it). True if it was in there.
pub fn remove(data: &mut Map, name: &str) -> bool {
    pop(blueprints_mut(data), &name_key(name)).is_some_and(|v| !matches!(v, Value::Null))
}

pub fn contains(data: &Map, name: &str) -> bool {
    let key = name_key(name);
    blueprints(data).iter().any(|(k, _)| *k == key)
}

/// All names in comparison form.
pub fn keys(data: &Map) -> BTreeSet<String> {
    blueprints(data).iter().map(|(k, _)| k.clone()).collect()
}

/// Like `keys`, but without what came only from the Basetool (such a
/// blueprint counts as a find as soon as it shows up in the Game.log).
pub fn keys_seen_in_game(data: &Map) -> BTreeSet<String> {
    blueprints(data)
        .iter()
        .filter(|(_, e)| field(e, "quelle").and_then(Value::as_str) != Some("basetool"))
        .map(|(k, _)| k.clone())
        .collect()
}

/// The names in their original spelling as found, alphabetical.
pub fn names(data: &Map) -> Vec<String> {
    let mut out: Vec<String> = blueprints(data).iter().map(|(k, e)| field_or(e, "name", k)).collect();
    out.sort();
    out
}

pub fn count(data: &Map) -> usize {
    blueprints(data).len()
}

/// How many blueprints come from where, in order of first appearance.
pub fn by_source(data: &Map) -> Vec<(String, usize)> {
    let mut counter: Vec<(String, usize)> = Vec::new();
    for (_, entry) in blueprints(data) {
        let source = field_or(entry, "quelle", "unbekannt");
        match counter.iter_mut().find(|(s, _)| *s == source) {
            Some((_, n)) => *n += 1,
            None => counter.push((source, 1)),
        }
    }
    counter
}

/// How many blueprints come back on their own after a reset.
pub fn restorable(data: &Map) -> usize {
    blueprints(data)
        .iter()
        .filter(|(_, e)| field(e, "quelle").and_then(Value::as_str).is_some_and(|q| RESTORABLE.contains(&q)))
        .count()
}

/// An old name whose words are contained in exactly one catalog name,
/// otherwise the name unchanged.
///
/// ⚠ No guessing here: at least `MIN_WORDS` words, and exactly one catalog
/// entry must contain all of them. A wrongly matched blueprint is worse than
/// one openly listed as unmatched.
pub fn unique_match(name: &str, known: &Known) -> String {
    let words = split_words(&name_key(name));
    if words.len() < MIN_WORDS {
        return name.to_string();
    }
    let mut hits = known.words().iter().filter(|(_, w)| words.is_subset(w)).map(|(k, _)| k);
    match (hits.next(), hits.next()) {
        (Some(hit), None) => known.get(hit).flatten().unwrap_or(hit).to_string(),
        _ => name.to_string(),
    }
}

// ------------------------------------------------------------ Collection with storage

/// The collection with everything it needs from outside.
pub struct Collection<'a> {
    pub paths: &'a Paths,
    pub catalog: &'a dyn Catalog,
    pub names: &'a dyn EnglishNames,
    pub exports: &'a dyn Exports,
    /// Clock for `stand` and `zeit` (`now_text`, fixed in test runs).
    pub clock: fn() -> String,
}

impl Collection<'_> {
    pub fn path(&self) -> String {
        self.paths.app_file(FILE_NAME)
    }

    pub fn empty(&self) -> Map {
        vec![
            ("version".into(), Value::int(FILE_VERSION)),
            ("stand".into(), Value::Str((self.clock)())),
            ("bauplaene".into(), Value::Object(Map::new())),
        ]
    }

    /// Collection from disk. A broken file is reported and set aside, then
    /// the backup applies; if neither is usable, an empty collection (startup
    /// never fails because of this).
    ///
    /// ⚠ Migrate first, then raise the version, and write only if something
    /// actually changed. The check is against `FILE_VERSION`, not a fixed
    /// number.
    ///
    /// ⚠ If `version` holds text or a container, Python aborts with
    /// `TypeError`; here the collection stays unchanged in that case.
    pub fn load(&self) -> Map {
        let Some(mut data) = self.read_object() else {
            return self.empty();
        };
        if version_below(get(&data, "version")) == Some(true) {
            // The migration saves: it goes again from what is on disk under the write lock, so that a write of
            // another process in between is not overwritten.
            let _lock = jsonio::lock_saves();
            let Some(fresh) = self.read_object() else {
                return self.empty();
            };
            data = fresh;
            if version_below(get(&data, "version")) == Some(true) {
                let changed = renew_keys(&mut data);
                map_set(&mut data, "version", Value::int(FILE_VERSION));
                if changed {
                    self.save(&mut data);
                }
            }
        }
        if get(&data, "version").is_none() {
            data.push(("version".into(), Value::int(FILE_VERSION)));
        }
        data
    }

    /// The file as a map with a `bauplaene` object, or `None`.
    fn read_object(&self) -> Option<Map> {
        let valid = |d: &Value| matches!(d, Value::Object(_));
        let Some(Value::Object(data)) = jsonio::load_json(&self.path(), Some(&valid), "collection.load") else {
            return None;
        };
        matches!(get(&data, "bauplaene"), Some(Value::Object(_))).then_some(data)
    }

    /// Writes the collection with a backup (`bestand.bak.json`) and without
    /// leaving partial files. A failure is reported; the output files follow
    /// afterwards.
    pub fn save(&self, data: &mut Map) -> bool {
        map_set(data, "version", Value::int(FILE_VERSION));
        map_set(data, "stand", Value::Str((self.clock)()));
        let target = self.path();
        let value = Value::Object(std::mem::take(data));
        let result = jsonio::save_json(&target, &value, 1, true);
        if let Value::Object(map) = value {
            *data = map;
        }
        match result {
            Ok(()) => {
                self.update_exports(data);
                true
            }
            Err(e) => {
                errors::record("collection.save", &e);
                false
            }
        }
    }

    /// Bring the output files up to date, silently (the collection is
    /// already safe on disk by then). An error is only reported.
    fn update_exports(&self, data: &Map) {
        if let Err(e) = self.exports.archive(data) {
            errors::record("collection.update_exports", &e);
        }
    }

    /// Where the highest blueprint count ever seen is stored: next to the
    /// second pointer, not in the data folder.
    pub fn high_water_file(&self) -> String {
        let flavor = self.paths.host.flavor;
        flavor.join(&flavor.dirname(&self.paths.second_pointer()), HIGH_WATER_FILE)
    }

    /// The stored mark: (high-water mark, folder). `Err` where Python raises
    /// an exception (not JSON, not an object, not a number).
    fn read_high_water(&self, mark: &str) -> Result<(i128, Value), ()> {
        let stored = jsonio::read_json(mark).map_err(|_| ())?;
        let map = stored.as_object().ok_or(())?;
        let highest = match get(map, "bauplaene").filter(|v| truthy(v)) {
            None => 0,
            Some(v) => py_int(v).ok_or(())?,
        };
        Ok((highest, get(map, "ordner").cloned().unwrap_or(Value::Null)))
    }

    /// Are there suddenly fewer blueprints than ever before? Then `Some`.
    ///
    /// ⚠ The check is against the high-water mark, not the last run: a
    /// collection never shrinks on its own. If it does, something is wrong
    /// with its location. A new high-water mark is remembered along with the
    /// folder.
    pub fn check_shrinkage(&self, data: &Map) -> Option<Shrinkage> {
        let now = count(data);
        let mark = self.high_water_file();
        let (highest, folder) = if crate::paths::is_file(&mark) {
            self.read_high_water(&mark).unwrap_or((0, Value::Null))
        } else {
            (0, Value::Null)
        };
        if now as i128 >= highest {
            let flavor = self.paths.host.flavor;
            let _ = (|| -> io::Result<()> {
                jsonio::make_dirs(&flavor.dirname(&mark))?;
                let temp = format!("{mark}.tmp");
                let value = Value::Object(vec![
                    ("bauplaene".into(), Value::Int(now.to_string())),
                    ("ordner".into(), Value::Str(self.paths.app_folder())),
                    ("stand".into(), Value::Str((self.clock)())),
                ]);
                jsonio::write_new(&temp, &jsonio::file_bytes(&value, 2))?;
                jsonio::os_replace(&temp, &mark)
            })();
            return None;
        }
        Some(Shrinkage { now, highest, folder })
    }

    /// Like `check_shrinkage`, but without changing the mark.
    pub fn shrinkage_state(&self) -> Option<Shrinkage> {
        let data = self.load();
        let now = count(&data);
        let mark = self.high_water_file();
        if !crate::paths::is_file(&mark) {
            return None;
        }
        let (highest, folder) = self.read_high_water(&mark).ok()?;
        if now as i128 >= highest {
            return None;
        }
        Some(Shrinkage { now, highest, folder })
    }

    /// Delete the mark, after a deliberate reset.
    pub fn reset_high_water(&self) {
        let _ = jsonio::remove_file(&self.high_water_file());
    }

    /// Remove the collection from disk, along with the high-water mark.
    ///
    /// ⚠ A file that is already missing counts as success: `None`. Otherwise
    /// the error.
    pub fn reset(&self) -> Option<io::Error> {
        self.reset_high_water();
        match jsonio::remove_file(&self.path()) {
            Ok(()) => None,
            Err(e) if jsonio::is_not_found(&e) => None,
            Err(e) => Some(e),
        }
    }

    /// The name as the catalog knows it, without appended details.
    ///
    /// ⚠ After unlocking, the game often writes `Balandin (S3 B Military)` to
    /// the log, because tools append class and size to the names. The
    /// parenthesis is only cut off if the full name is unknown and the
    /// shortened one is known (catalog names sometimes carry one themselves).
    /// After that: word set (`unique_match`), then the reverse lookup from the
    /// translation.
    ///
    /// ⚠ Pass `known` through when many names are processed in a row; without
    /// it, the function fetches the catalog itself every time.
    pub fn catalog_name(&self, name: &str, known: Option<&Known>) -> String {
        if name.is_empty() {
            return String::new();
        }
        let loaded;
        let known = match known {
            Some(k) => k,
            None => match self.catalog.known() {
                Ok(k) => {
                    loaded = k;
                    &loaded
                }
                Err(_) => return name.to_string(),
            },
        };
        if known.is_empty() || known.contains(&name_key(name)) {
            return name.to_string();
        }
        let short = py_strip(&SUFFIX_RE.replace(name, "")).to_string();
        if !short.is_empty() && short != name && known.contains(&name_key(&short)) {
            return short;
        }
        let found = unique_match(name, known);
        if found != name {
            return found;
        }
        let back = self.translated_back(if short.is_empty() { name } else { &short }, known);
        back.filter(|b| !b.is_empty()).unwrap_or_else(|| name.to_string())
    }

    /// The catalog name for a translated item name, only if exactly one
    /// English name of the key is in the catalog.
    pub fn translated_back(&self, name: &str, known: &Known) -> Option<String> {
        let mut hits: Vec<(String, String)> = Vec::new();
        for english in self.names.english_names(name) {
            let key = name_key(&english);
            if let Some(n) = known.get(&key) {
                let value = n.map_or_else(|| english.clone(), String::from);
                match hits.iter_mut().find(|(k, _)| *k == key) {
                    Some(slot) => slot.1 = value,
                    None => hits.push((key, value)),
                }
            }
        }
        if hits.len() != 1 {
            return None;
        }
        hits.pop().map(|(_, v)| v)
    }

    /// Align stored names with the catalog. Number of corrected entries; 0
    /// means "nothing to do".
    ///
    /// ⚠ If the blueprint already exists under the correct name, the existing
    /// entry stays (earlier find).
    ///
    /// ⚠ A name that is not text is left as is; Python aborts there.
    pub fn align(&self, data: &mut Map) -> usize {
        let Ok(known) = self.catalog.known() else { return 0 };
        let bp = blueprints_mut(data);
        let snapshot: Vec<String> = bp.iter().map(|(k, _)| k.clone()).collect();
        let mut fixed = 0;
        for key in snapshot {
            let Some(entry) = get(bp, &key) else { continue };
            let old_name = match field(entry, "name").filter(|v| truthy(v)) {
                None => key.clone(),
                Some(Value::Str(s)) => s.clone(),
                Some(_) => continue,
            };
            let new_name = self.catalog_name(&old_name, Some(&known));
            if new_name == old_name {
                continue;
            }
            let Some(mut entry) = pop(bp, &key) else { continue };
            let new_key = name_key(&new_name);
            if !bp.iter().any(|(k, _)| *k == new_key) {
                if let Value::Object(map) = &mut entry {
                    map_set(map, "name", Value::Str(new_name));
                }
                bp.push((new_key, entry));
            }
            fixed += 1;
        }
        fixed
    }

    /// Add a blueprint. True if it was not in there before or so far came
    /// only from the Basetool (first proof in the game).
    ///
    /// A known blueprint is not added twice; if the new source ranks higher,
    /// it is recorded. The name goes through `catalog_name` first.
    pub fn add(&self, data: &mut Map, name: &str, source: &str, when: Option<&str>) -> bool {
        let name = self.catalog_name(name, None);
        let key = name_key(&name);
        if key.is_empty() {
            return false;
        }
        let bp = blueprints_mut(data);
        match get_mut(bp, &key).filter(|v| !matches!(v, Value::Null)) {
            None => {
                let when = when.filter(|w| !w.is_empty()).map_or_else(|| (self.clock)(), String::from);
                let entry = Value::Object(vec![
                    ("name".into(), Value::str(py_strip(&name))),
                    ("quelle".into(), Value::str(source)),
                    ("zeit".into(), Value::Str(when)),
                ]);
                map_set(bp, &key, entry);
                true
            }
            Some(entry) => {
                let first_proof =
                    field(entry, "quelle").and_then(Value::as_str) == Some("basetool") && source != "basetool";
                if rank(Some(&Value::str(source))) > rank(field(entry, "quelle"))
                    && let Value::Object(map) = entry
                {
                    map_set(map, "quelle", Value::str(source));
                }
                first_proof
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use std::cell::RefCell;
    use std::fs;

    const FIXED: &str = "2026-10-19 12:00:00";

    fn fixed() -> String {
        FIXED.to_string()
    }

    struct Names(Vec<(&'static str, Vec<&'static str>)>);

    impl EnglishNames for Names {
        fn english_names(&self, name: &str) -> Vec<String> {
            self.0
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        }
    }

    struct Fixed(Vec<(&'static str, Option<&'static str>)>);

    impl Catalog for Fixed {
        fn known(&self) -> Result<Known, String> {
            Ok(Known::new(self.0.iter().map(|(k, n)| (k.to_string(), n.map(String::from)))))
        }
    }

    struct Broken;

    impl Catalog for Broken {
        fn known(&self) -> Result<Known, String> {
            Err("kaputt".into())
        }
    }

    #[derive(Default)]
    struct Counting(RefCell<usize>);

    impl Exports for Counting {
        fn archive(&self, _data: &Map) -> Result<(), String> {
            *self.0.borrow_mut() += 1;
            Ok(())
        }
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-collection-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn paths_in(home: &str) -> Paths {
        let (appdata, config) = (Flavor::native().join(home, "appdata"), Flavor::native().join(home, "config"));
        let vars =
            [(crate::legacy::ENV_HOME, home), ("APPDATA", appdata.as_str()), ("XDG_CONFIG_HOME", config.as_str())];
        Paths::new(Host::new(Flavor::native(), &vars, home))
    }

    fn entry(name: &str, source: &str, time: &str) -> Value {
        Value::Object(vec![
            ("name".into(), Value::str(name)),
            ("quelle".into(), Value::str(source)),
            ("zeit".into(), Value::str(time)),
        ])
    }

    fn data_with(entries: Vec<(&str, Value)>) -> Map {
        vec![
            ("version".into(), Value::int(1)),
            ("bauplaene".into(), Value::Object(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())),
        ]
    }

    #[test]
    fn name_key_cases() {
        assert_eq!(name_key("7MA \u{201e}Lorica\u{201c}"), "7ma 'lorica'");
        assert_eq!(name_key("XL-1 (Mil/2/A)"), "xl-1");
        assert_eq!(name_key("Ind/2/B Citadel"), "citadel");
        assert_eq!(name_key("Ind/2/B "), "ind/2/b");
        assert_eq!(name_key("Ravager-212 Magazine (16 Schuss)"), "ravager-212 magazine (16)");
        assert_eq!(name_key("Singe Cannon (S2)"), "singe cannon (s2)");
        assert_eq!(name_key("A\u{a0}B\u{fffd}C"), "a b c");
        assert_eq!(name_key("Rakete (IR1)"), "rakete");
        assert_eq!(name_key("x (mil/2/a)\n"), "x");
        assert_eq!(name_key("\u{1c}Helm\u{1f}"), "helm");
    }

    #[test]
    fn renew_merges_duplicates_older_wins() {
        let mut data = data_with(vec![
            ("xl-1 (mil/2/a)", entry("XL-1 (Mil/2/A)", "launcher", "2026-08-02 10:00:00")),
            ("xl-1", entry("XL-1", "log", "2026-08-01 10:00:00")),
            ("mag (16 cap)", entry("Mag (16 cap)", "log", "")),
            (
                "mag (16 schuss)",
                Value::Object(vec![
                    ("name".into(), Value::str("Mag (16 Schuss)")),
                    ("zeit".into(), Value::str("2026-01-01 00:00:00")),
                ]),
            ),
            ("kaputt", Value::str("kein Objekt")),
        ]);
        assert!(renew_keys(&mut data));
        let bp = blueprints(&data);
        let keys: Vec<&str> = bp.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["xl-1", "mag (16)", "kaputt"]);
        // The older find replaces the first one but keeps its position.
        assert_eq!(field(&bp[0].1, "zeit"), Some(&Value::str("2026-08-01 10:00:00")));
        // Without its own source, the older entry inherits that of the existing one.
        assert_eq!(field(&bp[1].1, "quelle"), Some(&Value::str("log")));
        assert_eq!(bp[2].1, Value::Object(Map::new()));
    }

    #[test]
    fn renew_rank_without_time() {
        let mut data = data_with(vec![
            ("a (bal)", entry("A (Bal)", "launcher", "")),
            ("a", Value::Object(vec![("name".into(), Value::str("A")), ("quelle".into(), Value::str("log"))])),
        ]);
        assert!(renew_keys(&mut data));
        assert_eq!(field(&blueprints(&data)[0].1, "quelle"), Some(&Value::str("log")));
        let mut same = data_with(vec![("a", entry("A", "log", "x"))]);
        assert!(!renew_keys(&mut same));
    }

    #[test]
    fn version_compare() {
        assert_eq!(version_below(None), Some(true));
        assert_eq!(version_below(Some(&Value::int(3))), Some(false));
        assert_eq!(version_below(Some(&Value::int(2))), Some(true));
        assert_eq!(version_below(Some(&Value::Int("-99999999999999999999999999999999999999999".into()))), Some(true));
        assert_eq!(version_below(Some(&Value::Int("99999999999999999999999999999999999999999".into()))), Some(false));
        assert_eq!(version_below(Some(&Value::Float(2.5))), Some(true));
        assert_eq!(version_below(Some(&Value::Float(f64::NAN))), Some(false));
        assert_eq!(version_below(Some(&Value::Bool(true))), Some(true));
        assert_eq!(version_below(Some(&Value::str("3"))), None);
    }

    #[test]
    fn catalog_name_rules() {
        let home = scratch("katalog");
        let paths = paths_in(&home);
        let catalog = Fixed(vec![
            ("balandin", Some("Balandin")),
            ("a03 sniper rifle magazine (15)", Some("A03 Sniper Rifle Magazine (15 cap)")),
            ("neutrino racing flight suit blackfire", Some("Neutrino Racing Flight Suit BlackFire")),
            ("parallax a", None),
            ("parallax b", None),
            ("helix ii mining laser", Some("Helix II Mining Laser")),
        ]);
        let names = Names(vec![
            ("Helix II Bergbaulaser", vec!["Helix II Mining Laser"]),
            ("Doppelt", vec!["Parallax A", "Parallax B"]),
        ]);
        let c = Collection { paths: &paths, catalog: &catalog, names: &names, exports: &NoExports, clock: fixed };
        assert_eq!(c.catalog_name("Balandin (S3 B Military)", None), "Balandin");
        assert_eq!(
            c.catalog_name("A03 Sniper Rifle Magazine (15 Schuss)", None),
            "A03 Sniper Rifle Magazine (15 Schuss)"
        );
        assert_eq!(c.catalog_name("BlackFire Racing Flight Suit", None), "Neutrino Racing Flight Suit BlackFire");
        // One word is not enough, and neither are multiple hits.
        assert_eq!(c.catalog_name("Parallax", None), "Parallax");
        assert_eq!(c.catalog_name("Helix II Bergbaulaser", None), "Helix II Mining Laser");
        assert_eq!(c.catalog_name("Helix II Bergbaulaser (S1)", None), "Helix II Mining Laser");
        assert_eq!(c.catalog_name("Doppelt", None), "Doppelt");
        assert_eq!(c.catalog_name("", None), "");
        let broken = Collection { paths: &paths, catalog: &Broken, names: &names, exports: &NoExports, clock: fixed };
        assert_eq!(broken.catalog_name("Balandin (S3 B Military)", None), "Balandin (S3 B Military)");
        let empty = Collection { paths: &paths, catalog: &NoCatalog, names: &names, exports: &NoExports, clock: fixed };
        assert_eq!(empty.catalog_name("Balandin (S3 B Military)", None), "Balandin (S3 B Military)");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn unique_match_takes_name_of_entry() {
        let known = Known::new([("x y z".to_string(), None), ("q r".to_string(), Some("Q R".to_string()))]);
        assert_eq!(unique_match("Z Y", &known), "x y z");
        assert_eq!(unique_match("r q", &known), "Q R");
        assert_eq!(unique_match("Z", &known), "Z");
    }

    #[test]
    fn add_ranks_and_first_proof() {
        let home = scratch("add");
        let paths = paths_in(&home);
        let names = Names(Vec::new());
        let c = Collection { paths: &paths, catalog: &NoCatalog, names: &names, exports: &NoExports, clock: fixed };
        let mut data = c.empty();
        assert!(c.add(&mut data, " Helm ", "basetool", None));
        assert_eq!(blueprints(&data)[0].1, entry("Helm", "basetool", FIXED));
        assert!(c.add(&mut data, "helm", "launcher", Some("")));
        assert!(!c.add(&mut data, "HELM", "log", None));
        assert_eq!(field(&blueprints(&data)[0].1, "quelle"), Some(&Value::str("log")));
        assert!(!c.add(&mut data, "Helm", "hand", None));
        assert_eq!(field(&blueprints(&data)[0].1, "quelle"), Some(&Value::str("log")));
        assert!(!c.add(&mut data, "   ", "log", None));
        assert!(c.add(&mut data, "Weste", "log", Some("2026-01-01 00:00:00")));
        assert_eq!(count(&data), 2);
        assert!(contains(&data, "WESTE"));
        assert!(remove(&mut data, "weste"));
        assert!(!remove(&mut data, "weste"));
        assert_eq!(by_source(&data), vec![("log".to_string(), 1)]);
        assert_eq!(restorable(&data), 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn counting_helpers() {
        let data = data_with(vec![
            ("b", entry("B", "basetool", "")),
            ("a", Value::Object(vec![("name".into(), Value::str(""))])),
            ("c", entry("C", "start", "")),
        ]);
        assert_eq!(names(&data), ["B", "C", "a"]);
        assert_eq!(keys_seen_in_game(&data).into_iter().collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(by_source(&data), vec![("basetool".into(), 1), ("unbekannt".into(), 1), ("start".into(), 1)]);
        assert_eq!(restorable(&data), 1);
    }

    #[test]
    fn load_migrates_and_saves_once() {
        let home = scratch("laden");
        let paths = paths_in(&home);
        let names = Names(Vec::new());
        let exports = Counting::default();
        let c = Collection { paths: &paths, catalog: &NoCatalog, names: &names, exports: &exports, clock: fixed };
        // Missing file: empty collection, nothing written.
        assert_eq!(c.load(), c.empty());
        assert!(!fs::exists(c.path()).unwrap());
        let old = data_with(vec![("xl-1 (mil/2/a)", entry("XL-1 (Mil/2/A)", "log", "t"))]);
        fs::write(c.path(), jsonio::dumps(&Value::Object(old), Some(1), false)).unwrap();
        let data = c.load();
        assert_eq!(keys(&data).into_iter().collect::<Vec<_>>(), ["xl-1"]);
        assert_eq!(*exports.0.borrow(), 1);
        let written = fs::read_to_string(c.path()).unwrap();
        assert!(written.contains("\"version\": 3"), "{written}");
        // Already migrated: no second write.
        c.load();
        assert_eq!(*exports.0.borrow(), 1);
        // Old version without changes: version raised, not written.
        let plain = data_with(vec![("xl-1", entry("XL-1", "log", "t"))]);
        fs::write(c.path(), jsonio::dumps(&Value::Object(plain), Some(1), false)).unwrap();
        let data = c.load();
        assert_eq!(get(&data, "version"), Some(&Value::int(3)));
        assert_eq!(*exports.0.borrow(), 1);
        // `bauplaene` is not an object: empty.
        fs::write(c.path(), "{\"bauplaene\": []}").unwrap();
        assert_eq!(c.load(), c.empty());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn the_migration_goes_from_what_another_writer_wrote_while_it_waited_for_the_lock() {
        let home = scratch("laden-sperre");
        let paths = paths_in(&home);
        let names = Names(Vec::new());
        let c = Collection { paths: &paths, catalog: &NoCatalog, names: &names, exports: &NoExports, clock: fixed };
        let old = |entries: Vec<(&str, Value)>| jsonio::dumps(&Value::Object(data_with(entries)), Some(1), false);
        let path = c.path();
        fs::write(&path, old(vec![("xl-1 (mil/2/a)", entry("XL-1 (Mil/2/A)", "log", "t"))])).unwrap();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let second = old(vec![
            ("xl-1 (mil/2/a)", entry("XL-1 (Mil/2/A)", "log", "t")),
            ("zz-9", entry("ZZ-9", "hand", "t")),
        ]);
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || {
                let _lock = jsonio::lock_saves();
                held_tx.send(()).unwrap();
                // The loading reads the old file first, and waits for the lock before it migrates.
                std::thread::sleep(std::time::Duration::from_millis(400));
                fs::write(&path, second).unwrap();
            })
        };
        held_rx.recv().unwrap();
        let data = c.load();
        writer.join().unwrap();
        assert!(keys(&data).contains("zz-9"), "the entry written while it waited is lost: {:?}", keys(&data));
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("zz-9") && written.contains("\"version\": 3"), "{written}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn high_water_and_reset() {
        let home = scratch("hochwasser");
        let paths = paths_in(&home);
        let names = Names(Vec::new());
        let c = Collection { paths: &paths, catalog: &NoCatalog, names: &names, exports: &NoExports, clock: fixed };
        assert!(c.high_water_file().starts_with(&home));
        let mut data = c.empty();
        c.add(&mut data, "A", "log", None);
        c.add(&mut data, "B", "log", None);
        assert!(c.save(&mut data));
        assert_eq!(c.check_shrinkage(&data), None);
        assert!(fs::read_to_string(c.high_water_file()).unwrap().contains("\"bauplaene\": 2"));
        // A tie is not shrinkage.
        assert_eq!(c.check_shrinkage(&data), None);
        assert_eq!(c.shrinkage_state(), None);
        remove(&mut data, "a");
        let shrink = c.check_shrinkage(&data).expect("kleiner als zuvor");
        assert_eq!((shrink.now, shrink.highest), (1, 2));
        assert_eq!(shrink.folder, Value::str(paths.app_folder()));
        // The stored collection still has two: no shrinkage.
        assert_eq!(c.shrinkage_state(), None);
        assert!(c.save(&mut data));
        assert_eq!(c.shrinkage_state().map(|s| s.highest), Some(2));
        // A number as text counts as with `int()`; nonsense counts as 0.
        fs::write(c.high_water_file(), "{\"bauplaene\": \" 5 \", \"ordner\": 7}").unwrap();
        assert_eq!(c.shrinkage_state(), Some(Shrinkage { now: 1, highest: 5, folder: Value::int(7) }));
        fs::write(c.high_water_file(), "{\"bauplaene\": \"fünf\"}").unwrap();
        assert_eq!(c.shrinkage_state(), None);
        assert_eq!(c.check_shrinkage(&data), None);
        assert!(fs::read_to_string(c.high_water_file()).unwrap().contains("\"bauplaene\": 1"));
        // Reset: mark and file gone, no error even when done twice.
        assert!(c.reset().is_none());
        assert!(!fs::exists(c.high_water_file()).unwrap());
        assert!(!fs::exists(c.path()).unwrap());
        assert!(c.reset().is_none());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn the_folder_of_the_mark_reads_as_python_prints_it() {
        let of = |folder: Value| Shrinkage { now: 1, highest: 2, folder }.folder_text();
        assert_eq!(of(Value::str("C:\\Daten")).as_deref(), Some("C:\\Daten"));
        assert_eq!(of(Value::int(7)).as_deref(), Some("7"));
        assert_eq!(of(Value::Bool(true)).as_deref(), Some("True"));
        for empty in [Value::Null, Value::str(""), Value::int(0), Value::Bool(false), Value::Array(Vec::new())] {
            assert_eq!(of(empty.clone()), None, "{empty:?}");
        }
    }

    #[test]
    fn align_keeps_existing_entry() {
        let home = scratch("angleichen");
        let paths = paths_in(&home);
        let names = Names(Vec::new());
        let catalog = Fixed(vec![("balandin", Some("Balandin")), ("glacis", Some("Glacis"))]);
        let c = Collection { paths: &paths, catalog: &catalog, names: &names, exports: &NoExports, clock: fixed };
        let mut data = data_with(vec![
            ("balandin", entry("Balandin", "log", "1")),
            ("balandin (s3 b military)", entry("Balandin (S3 B Military)", "log", "2")),
            ("glacis (x y)", entry("Glacis (X Y)", "hand", "3")),
            ("zahl", Value::Object(vec![("name".into(), Value::int(5))])),
        ]);
        assert_eq!(c.align(&mut data), 2);
        let bp = blueprints(&data);
        assert_eq!(bp.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["balandin", "zahl", "glacis"]);
        assert_eq!(field(&bp[0].1, "zeit"), Some(&Value::str("1")));
        assert_eq!(field(&bp[2].1, "name"), Some(&Value::str("Glacis")));
        let broken = Collection { paths: &paths, catalog: &Broken, names: &names, exports: &NoExports, clock: fixed };
        assert_eq!(broken.align(&mut data), 0);
        let _ = fs::remove_dir_all(&home);
    }
}
