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

//! The own ships (`hangar.json`), as the Python version keeps them
//! (`fleet.py`, `included_vehicles.py`): reading the ship list
//! `hangar-schiffe` (the other export formats are in `hangar_formats`),
//! matching against the ships already stored, removing pledge ships an
//! export no longer names, the included vehicles from the bundled catalog,
//! and saving merged with what another place wrote meanwhile (`Held`).
//!
//! ⚠ Stored keys stay German (`schiffe`, `wunsch`, `merkzettel`, `herkunft`,
//! …) and everything the file holds besides is kept as it is: entries are
//! JSON objects, changed field by field, never rebuilt.
//!
//! ⚠ `pledgeId` is never taken over; the price (`preis`) is private and must
//! not reach an error report.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use regex::Regex;

use crate::errors;
use crate::jsonio::{self, Map, Newline, Value, map_set, py_eq, py_eq_value};
use crate::pypath::Flavor;
use crate::text::{py_isspace, py_strip};

pub const FILE: &str = "hangar.json";
pub const FORMAT: i64 = 1;

pub const PLEDGE: &str = "pledge";
pub const INGAME: &str = "ingame";
pub const BASETOOL: &str = "basetool";
pub const LIST: &str = "liste";
pub const CATALOG: &str = "katalog";
/// On an entry: its insurance came from the Basetool; no import changes it.
const BASETOOL_COVER: &str = "versicherung_basetool";
/// On an entry: it came from the Basetool and an export names it since.
const FROM_BASETOOL: &str = "aus_basetool";

pub const HANGAR_SHIPS_SCHEMA: &str = "hangar-schiffe";
/// The other output of the same extension: one transfer file per pledge.
pub const HANGAR_TRANSFER_SCHEMA: &str = "hangar-uebergabe";

/// The UEX ship list in the data folder, for names without a manufacturer.
pub const SHIP_CATALOG_FILE: &str = "schiffe.json";
const SHIP_CATALOG_FORMAT: i64 = 5;

const IMPORT_KEYS: [&str; 8] = ["kurz", "hkurz", "lti", "warbond", "paket", "gekauft", "preis", "versicherung"];
const CLASS_WORDS: [&str; 5] = ["frigate", "destroyer", "corvette", "carrier", "cruiser"];
const MATRIX_MATCHES: [&str; 2] = ["sicher", "naeherung"];

/// How the CSV of the Hangar Extension puts the maker before the ship name;
/// longest names first.
const CSV_MAKERS: [(&str, &str); 24] = [
    ("Roberts Space Industries", "RSI"),
    ("Consolidated Outland", "CNOU"),
    ("Musashi Industrial & Starflight Concern", "MISC"),
    ("Aegis", "AEGS"),
    ("Anvil", "ANVL"),
    ("Aopoa", "AOPO"),
    ("ARGO", "ARGO"),
    ("Argo", "ARGO"),
    ("Banu", "BANU"),
    ("Crusader", "CRUS"),
    ("CNOU", "CNOU"),
    ("Drake", "DRAK"),
    ("Esperia", "ESPR"),
    ("Gatac", "GAMA"),
    ("Greycat", "GRIN"),
    ("Kruger", "KRIG"),
    ("MISC", "MISC"),
    ("Mirai", "MRAI"),
    ("Origin", "ORIG"),
    ("RSI", "RSI"),
    ("Tumbril", "TMBL"),
    ("Vanduul", "VNCL"),
    ("Xi'an", "XIAN"),
    ("Xian", "XIAN"),
];

/// A file read on purpose not: the error mark of the Python version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// Another version than `hangar-schiffe` 1 (`version`).
    Version,
    /// A transfer file per pledge instead of the ship list (`uebergabe`).
    Transfer,
}

// ------------------------------------------------------------ dict helpers

pub(crate) fn get<'a>(m: &'a Map, key: &str) -> Option<&'a Value> {
    m.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The value as text, `""` when missing or not a string (`(x or '')`).
pub(crate) fn text<'a>(m: &'a Map, key: &str) -> &'a str {
    match get(m, key) {
        Some(Value::Str(s)) => s,
        _ => "",
    }
}

fn pop(m: &mut Map, key: &str) {
    m.retain(|(k, _)| k != key);
}

/// Python's truth value.
pub(crate) fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

fn is_true(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true)))
}

/// `value in (None, '')`.
fn none_or_empty(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null)) || matches!(v, Some(Value::Str(s)) if s.is_empty())
}

/// `value in (None, '', 0)`: `False` and `0.0` equal `0` in Python.
fn none_empty_or_zero(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => true,
        Some(Value::Str(s)) => s.is_empty(),
        Some(Value::Int(n)) => n == "0",
        Some(Value::Float(x)) => *x == 0.0,
        _ => false,
    }
}

/// `x is None`: missing or `null`.
fn is_none(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

/// Python's `int(value)`, `None` where it raises.
pub(crate) fn py_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Int(n) => Some(n.parse::<i64>().unwrap_or(if n.starts_with('-') { i64::MIN } else { i64::MAX })),
        Value::Float(x) if x.is_finite() => Some(x.trunc() as i64),
        Value::Str(s) => {
            let t: String = py_strip(s).chars().filter(|c| *c != '_').collect();
            t.parse::<i64>().ok()
        }
        _ => None,
    }
}

/// A stored field as text, `""` when missing or not a string.
pub fn field_text<'a>(entry: &'a Map, key: &str) -> &'a str {
    text(entry, key)
}

/// A stored field by Python's truth value.
pub fn field_true(entry: &Map, key: &str) -> bool {
    truthy(get(entry, key))
}

/// A stored field as `int(value or 0)`, `0` where that fails.
pub fn field_int(entry: &Map, key: &str) -> i64 {
    if truthy(get(entry, key)) { py_int(get(entry, key)).unwrap_or(0) } else { 0 }
}

/// `re.sub(r'[^a-z0-9]', '', (text or '').lower())`.
pub fn slim(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

fn slim_of(m: &Map, key: &str) -> String {
    slim(text(m, key))
}

pub(crate) fn strip_owned(s: &str) -> String {
    py_strip(s).to_string()
}

pub(crate) fn object(pairs: Vec<(&str, Value)>) -> Map {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// ------------------------------------------------------------ the file

/// An empty hangar.
pub fn empty() -> Map {
    object(vec![("format", Value::int(FORMAT)), ("schiffe", Value::Array(Vec::new()))])
}

/// One writer of `hangar.json` at a time in this program.
static HANGAR_LOCK: Mutex<()> = Mutex::new(());

/// Hold while reading, changing and writing the hangar as one step.
pub fn lock() -> MutexGuard<'static, ()> {
    HANGAR_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The file as it lies, if it has the expected structure.
pub fn read(path: &str) -> Option<Map> {
    let valid = |v: &Value| matches!(v, Value::Object(_));
    let data = jsonio::load_json(path, Some(&valid), "fleet.load")?;
    let Value::Object(map) = data else { return None };
    let format_ok = matches!(get(&map, "format"), Some(Value::Int(n)) if n == "1")
        || matches!(get(&map, "format"), Some(Value::Float(x)) if *x == 1.0);
    if format_ok && matches!(get(&map, "schiffe"), Some(Value::Array(_))) { Some(map) } else { None }
}

/// The own hangar, or an empty one. Duplicates are merged and saved.
pub fn load(path: &str) -> Map {
    match read(path) {
        Some(mut data) => {
            if merge_duplicates(&mut data) {
                let _ = save(path, &mut data);
            }
            data
        }
        None => empty(),
    }
}

/// Like [`load`], but a file that is there and cannot be read as a hangar gives
/// `None` instead of an empty one. For the sync, which must never take the
/// hangar for empty because it could not be read once.
pub fn load_checked(path: &str) -> Option<Map> {
    if !Path::new(path).exists() {
        return Some(empty());
    }
    let mut data = read(path)?;
    if merge_duplicates(&mut data) {
        let _ = save(path, &mut data);
    }
    Some(data)
}

/// Write atomically, after removing included vehicles that no longer belong.
pub fn save(path: &str, data: &mut Map) -> bool {
    settle(data);
    write(path, data)
}

/// What saving does to the data before writing: remove catalog vehicles
/// whose carrier is gone.
pub fn settle(data: &mut Map) {
    let mut ships = take_ships(data);
    prune(&mut ships, catalog_entries());
    put_ships(data, ships);
}

/// A file that is there but not a hangar (other `format`, `schiffe` no list,
/// not readable) is moved aside as `….kaputt-<time>.json`, so writing does
/// not overwrite it. `Ok(true)` when something was set aside (also when
/// `jsonio::load_json` did it while reading a broken file).
///
/// ⚠ Python writes over such a file; Verse-Kit keeps it.
fn set_aside_unusable(path: &str) -> Result<bool, String> {
    if !Path::new(path).exists() || read(path).is_some() {
        return Ok(false);
    }
    if !Path::new(path).exists() {
        return Ok(true);
    }
    // A free name: two files set aside in the same second must not meet.
    let stamp = jsonio::time_stamp();
    let name = |n: usize| {
        let tail = if n == 0 { String::new() } else { format!("-{n}") };
        match path.strip_suffix(".json") {
            Some(stem) => format!("{stem}.kaputt-{stamp}{tail}.json"),
            None => format!("{path}.kaputt-{stamp}{tail}"),
        }
    };
    let aside = (0..).map(name).find(|n| !Path::new(n).exists()).unwrap_or_else(|| name(0));
    jsonio::os_replace(path, &aside).map_err(|e| e.to_string())?;
    errors::record("fleet.save", &format!("kein Hangar, beiseitegelegt: {aside}"));
    Ok(true)
}

/// A copy as `hangar.bak.json` before something is removed; true when the
/// file is missing or the copy was made.
pub fn backup(path: &str) -> bool {
    if !Path::new(path).exists() {
        return true;
    }
    match jsonio::copy2(path, &jsonio::backup_name(path)) {
        Ok(()) => true,
        Err(e) => {
            errors::record("fleet.sicherung", &e);
            false
        }
    }
}

/// Write as it is, atomically (`.tmp` and replace); reported on failure.
/// A file there that is no hangar is set aside first.
pub fn write(path: &str, data: &Map) -> bool {
    if let Err(e) = set_aside_unusable(path) {
        errors::record("fleet.save", &e);
        return false;
    }
    let result = (|| {
        let folder = Flavor::native().dirname(path);
        if !folder.is_empty() {
            jsonio::make_dirs(&folder)?;
        }
        jsonio::write_text_atomic(path, &file_text(data), Newline::Native)
    })();
    match result {
        Ok(()) => {
            crate::basetool::sync::store_saved(path);
            true
        }
        Err(e) => {
            errors::record("fleet.save", &e);
            false
        }
    }
}

/// What `json.dump(data, f, ensure_ascii=False, indent=1)` writes.
pub fn file_text(data: &Map) -> String {
    jsonio::dumps(&Value::Object(data.clone()), Some(1), false)
}

fn take_ships(data: &mut Map) -> Vec<Map> {
    let ships = match data.iter_mut().find(|(k, _)| k == "schiffe") {
        Some((_, Value::Array(items))) => std::mem::take(items),
        _ => Vec::new(),
    };
    ships.into_iter().filter_map(|v| if let Value::Object(m) = v { Some(m) } else { None }).collect()
}

fn put_ships(data: &mut Map, ships: Vec<Map>) {
    map_set(data, "schiffe", Value::Array(ships.into_iter().map(Value::Object).collect()));
}

/// The ship entries of a hangar (objects only).
pub fn ships(data: &Map) -> Vec<&Map> {
    match get(data, "schiffe") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

/// The ship entry at `index` among the objects of `schiffe`, to change it.
pub fn ship_mut(data: &mut Map, index: usize) -> Option<&mut Map> {
    match data.iter_mut().find(|(k, _)| k == "schiffe") {
        Some((_, Value::Array(items))) => {
            items.iter_mut().filter_map(|v| if let Value::Object(m) = v { Some(m) } else { None }).nth(index)
        }
        _ => None,
    }
}

// ------------------------------------------------------------ Python's loose types

/// `(value or '')` where a string method follows: falsy values give `""`,
/// a truthy value that is no string is the error Python raises.
pub(crate) fn or_text(v: Option<&Value>) -> Result<&str, String> {
    match v {
        Some(Value::Str(s)) => Ok(s),
        v if !truthy(v) => Ok(""),
        _ => Err("'…' object has no attribute 'strip'".into()),
    }
}

/// `for x in (value or ())`: strings give their characters, objects their
/// keys; a truthy value that cannot be iterated is the error Python raises.
pub(crate) fn iter_values(v: Option<&Value>) -> Result<Vec<Value>, String> {
    match v {
        Some(Value::Array(items)) => Ok(items.clone()),
        Some(Value::Str(s)) => Ok(s.chars().map(|c| Value::Str(c.to_string())).collect()),
        Some(Value::Object(m)) => Ok(m.iter().map(|(k, _)| Value::Str(k.clone())).collect()),
        v if !truthy(v) => Ok(Vec::new()),
        _ => Err("object is not iterable".into()),
    }
}

/// `_slim(value)` where the value may be anything.
pub(crate) fn slim_value(v: Option<&Value>) -> Result<String, String> {
    or_text(v).map(slim)
}

// ------------------------------------------------------------ saving with merge

/// A hangar as a page holds it, with the state it last knew on disk
/// (`fleet._remember`).
///
/// ⚠ Several places hold their own hangar. Saving writes only what changed
/// in this one since it was loaded or saved (`fleet._merge`), so the changes
/// of the others stay.
#[derive(Clone, Debug)]
pub struct Held {
    pub data: Map,
    base: Option<Map>,
}

impl Held {
    /// Data that did not come from the disk: saved as it is.
    pub fn detached(data: Map) -> Held {
        Held { data, base: None }
    }
}

/// `fleet.load`: the own hangar, or an empty one; duplicates are merged and
/// saved.
pub fn load_held(path: &str) -> Held {
    let data = read(path).unwrap_or_else(empty);
    let mut held = Held { base: Some(data.clone()), data };
    if merge_duplicates(&mut held.data) {
        let _ = save_held(path, &mut held);
    }
    held
}

/// `a == b` for two objects as Python compares them.
fn same(a: &Map, b: &Map) -> bool {
    a.len() == b.len() && a.iter().all(|(k, v)| get(b, k).is_some_and(|w| py_eq_value(v, w)))
}

/// `fleet.changed_elsewhere`: did another place change the file since `held`
/// was loaded or saved?
pub fn changed_elsewhere(path: &str, held: &Held) -> bool {
    let Some(base) = &held.base else { return true };
    read(path).is_some_and(|disk| !same(&disk, base))
}

/// How saving went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// Written as it was held.
    Written,
    /// Another place had written meanwhile; both changes are in the file,
    /// so what is held is older than the file.
    Merged,
    /// The file there was no hangar; it lies beside as `….kaputt-…`.
    SetAside,
    /// Nothing written (reported).
    Failed,
}

impl Saved {
    pub fn ok(self) -> bool {
        self != Saved::Failed
    }
}

/// `fleet.save`: atomically, merged with the disk when another place wrote
/// in between; reported on failure.
pub fn save_held(path: &str, held: &mut Held) -> Saved {
    let aside = match set_aside_unusable(path) {
        Ok(aside) => aside,
        Err(e) => {
            errors::record("fleet.save", &e);
            return Saved::Failed;
        }
    };
    let disk = held.base.as_ref().and_then(|base| read(path).filter(|disk| !same(disk, base)));
    let saved = match (disk, &held.base) {
        (Some(disk), Some(base)) => match merge(base, &held.data, &disk) {
            Ok(mut out) => {
                settle(&mut out);
                settle(&mut held.data);
                if write(path, &out) { Saved::Merged } else { Saved::Failed }
            }
            Err(e) => {
                errors::record("fleet.save", &e);
                Saved::Failed
            }
        },
        _ => {
            settle(&mut held.data);
            match (write(path, &held.data), aside) {
                (false, _) => Saved::Failed,
                (true, true) => Saved::SetAside,
                (true, false) => Saved::Written,
            }
        }
    };
    if saved.ok() {
        held.base = Some(held.data.clone());
    }
    saved
}

/// `(maker, name, running number)` of a list entry.
type EntryKey = (String, String, usize);

/// `fleet._entry_keys`: `(maker, name, running number)` → index in the list.
fn entry_keys(list: Option<&Value>) -> Result<Vec<(EntryKey, usize)>, String> {
    let items = iter_values(list)?;
    let mut seen: Vec<((String, String), usize)> = Vec::new();
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Value::Object(e) = item else { continue };
        let base = (slim_value(get(e, "hersteller"))?, slim_value(get(e, "name"))?);
        let n = match seen.iter_mut().find(|(k, _)| *k == base) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((base.clone(), 1));
                1
            }
        };
        out.push(((base.0, base.1, n), index));
    }
    Ok(out)
}

fn key_index(keys: &[(EntryKey, usize)], key: &EntryKey) -> Option<usize> {
    keys.iter().find(|(k, _)| k == key).map(|(_, i)| *i)
}

/// The keys of two objects, those of the first first.
fn union_keys(a: &Map, b: &Map) -> Vec<String> {
    let mut keys: Vec<String> = a.iter().map(|(k, _)| k.clone()).collect();
    keys.extend(b.iter().map(|(k, _)| k.clone()).filter(|k| get(a, k).is_none()));
    keys
}

/// `fleet._merge_dict`: what changed from `base` to `mine`, into `target`.
fn merge_dict(base: &Map, mine: &Map, target: &mut Value) -> Result<(), String> {
    for key in union_keys(base, mine) {
        if py_eq(get(mine, &key), get(base, &key)) {
            continue;
        }
        let Value::Object(target) = target else { return Err("item assignment on a non-object".into()) };
        match get(mine, &key) {
            Some(value) => map_set(target, &key, value.clone()),
            None => pop(target, &key),
        }
    }
    Ok(())
}

/// The value of `key` in `m`, added as `default` when missing (`setdefault`).
fn set_default<'a>(m: &'a mut Map, key: &str, default: Value) -> &'a mut Value {
    let index = match m.iter().position(|(k, _)| k == key) {
        Some(i) => i,
        None => {
            m.push((key.to_string(), default));
            m.len() - 1
        }
    };
    &mut m[index].1
}

/// `fleet._merge`: the own changes (`base` → `mine`) on the disk state:
/// per ship field by field, in `belegung` slot by slot.
fn merge(base: &Map, mine: &Map, disk: &Map) -> Result<Map, String> {
    let mut result = disk.clone();
    for list_key in ["schiffe", "wunsch"] {
        if [base, mine, disk].iter().all(|m| get(m, list_key).is_none()) {
            continue;
        }
        let b = entry_keys(get(base, list_key))?;
        let m = entry_keys(get(mine, list_key))?;
        let base_list = match get(base, list_key) {
            Some(Value::Array(items)) => items.as_slice(),
            _ => &[],
        };
        let mine_list = match get(mine, list_key) {
            Some(Value::Array(items)) => items.as_slice(),
            _ => &[],
        };
        let target_list = set_default(&mut result, list_key, Value::Array(Vec::new()));
        let d = entry_keys(Some(target_list))?;
        for (key, mi) in &m {
            let entry = mine_list[*mi].as_object().cloned().unwrap_or_default();
            let Some(bi) = key_index(&b, key) else {
                if key_index(&d, key).is_none() {
                    match target_list {
                        Value::Array(items) => items.push(Value::Object(entry)),
                        _ => return Err("append on a non-list".into()),
                    }
                }
                continue;
            };
            let Some(ti) = key_index(&d, key) else { continue };
            let old = base_list[bi].as_object().cloned().unwrap_or_default();
            let Value::Array(items) = target_list else { continue };
            let Some(Value::Object(target)) = items.get_mut(ti) else { continue };
            for field in union_keys(&old, &entry) {
                if field == "belegung"
                    && let Some(Value::Object(slots)) = get(&entry, &field)
                {
                    let old_slots = match get(&old, &field) {
                        Some(Value::Object(o)) => o.clone(),
                        v if !truthy(v) => Map::new(),
                        _ => return Err("'…' object has no attribute 'get'".into()),
                    };
                    merge_dict(&old_slots, slots, set_default(target, &field, Value::Object(Map::new())))?;
                } else if !py_eq(get(&entry, &field), get(&old, &field)) {
                    match get(&entry, &field) {
                        Some(value) => map_set(target, &field, value.clone()),
                        None => pop(target, &field),
                    }
                }
            }
        }
        let gone: Vec<usize> =
            b.iter().filter(|(k, _)| key_index(&m, k).is_none()).filter_map(|(k, _)| key_index(&d, k)).collect();
        if !gone.is_empty()
            && let Value::Array(items) = target_list
        {
            let mut index = 0;
            items.retain(|_| {
                let keep = !gone.contains(&index);
                index += 1;
                keep
            });
        }
    }
    let without_lists =
        |m: &Map| -> Map { m.iter().filter(|(k, _)| k != "schiffe" && k != "wunsch").cloned().collect() };
    let mut top = Value::Object(result);
    merge_dict(&without_lists(base), &without_lists(mine), &mut top)?;
    match top {
        Value::Object(result) => Ok(result),
        _ => Err("not an object".into()),
    }
}

// ------------------------------------------------------------ changes from the page

/// `fleet.remove`: every entry with this maker and name; the included
/// vehicles of the catalog go along. True when one fell away.
pub fn remove(data: &mut Map, name: &str, manufacturer: &str) -> bool {
    let wanted = slim(manufacturer) + &slim(name);
    let mut ships = take_ships(data);
    let before = ships.len();
    ships.retain(|s| slim_of(s, "hersteller") + &slim_of(s, "name") != wanted);
    let removed = ships.len() != before;
    if removed {
        prune(&mut ships, catalog_entries());
    }
    put_ships(data, ships);
    removed
}

/// `fleet.add` from the page: bought with real money (`pledge`, with or
/// without LTI) or in the game. True when the ship is new.
pub fn add_by_hand(data: &mut Map, name: &str, real_money: bool, lti: bool) -> bool {
    let origin = if real_money { PLEDGE } else { INGAME };
    let rest = object(vec![("lti", Value::Bool(real_money && lti))]);
    let mut ships = take_ships(data);
    let new = add(&mut ships, name, "", origin, &rest, catalog_entries());
    put_ships(data, ships);
    new
}

pub const CHOICE_YES: &str = "ja";
pub const CHOICE_NO: &str = "nein";

/// `fleet_analysis.melt_applies`: only ships bought with real money.
pub fn melt_applies(entry: &Map) -> bool {
    let origin = get(entry, "herkunft");
    !truthy(origin) || matches!(origin, Some(Value::Str(s)) if s == PLEDGE)
}

/// `schmelz_wahl` as the page offers it: `ja`, `nein` or `""` (automatic).
pub fn melt_choice(entry: &Map) -> &str {
    match text(entry, "schmelz_wahl") {
        CHOICE_YES => CHOICE_YES,
        CHOICE_NO => CHOICE_NO,
        _ => "",
    }
}

/// `schmelzbar` from the import, when it is a truth value.
pub fn melt_imported(entry: &Map) -> Option<bool> {
    match get(entry, "schmelzbar") {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

/// Sets or clears `schmelz_wahl`; true when it changed.
pub fn set_melt_choice(entry: &mut Map, choice: &str) -> bool {
    let same = match get(entry, "schmelz_wahl") {
        v if !truthy(v) => choice.is_empty(),
        Some(Value::Str(s)) => s == choice,
        _ => false,
    };
    if same {
        return false;
    }
    if choice.is_empty() {
        pop(entry, "schmelz_wahl");
    } else {
        map_set(entry, "schmelz_wahl", Value::str(choice));
    }
    true
}

fn loadout(entry: &Map) -> &[(String, Value)] {
    match get(entry, "belegung") {
        Some(Value::Object(m)) => m,
        _ => &[],
    }
}

/// `cart.open_count`: planned parts not ticked off yet.
pub fn open_count(entry: &Map) -> usize {
    loadout(entry).iter().filter(|(_, part)| matches!(part, Value::Object(p) if !truthy(get(p, "erledigt")))).count()
}

/// `cart.fully_fitted`: something planned and all of it ticked off.
pub fn fully_fitted(entry: &Map) -> bool {
    let values = loadout(entry);
    !values.is_empty()
        && values.iter().all(|(_, part)| match part {
            Value::Object(p) => truthy(get(p, "erledigt")),
            _ => true,
        })
}

// ------------------------------------------------------------ same ship

fn names_compatible(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    long.starts_with(short) && CLASS_WORDS.contains(&&long[short.len()..])
}

fn from_basetool(entry: &Map) -> bool {
    text(entry, "herkunft") == BASETOOL || truthy(get(entry, FROM_BASETOOL))
}

/// Does this hangar entry mean the same ship? (`fleet._same_ship`)
pub fn same_ship(entry: &Map, name: &str, manufacturer: &str, kurz: &str, hkurz: &str) -> bool {
    let wanted = slim(name);
    let e_kurz = slim_of(entry, "kurz");
    let kurz = slim(kurz);
    if !e_kurz.is_empty() && !kurz.is_empty() && e_kurz == kurz && names_compatible(&slim_of(entry, "name"), &wanted) {
        return true;
    }
    let e_name = slim_of(entry, "name");
    if wanted.is_empty() {
        return false;
    }
    let e_hkurz = slim_of(entry, "hkurz");
    let hkurz = slim(hkurz);
    let both_codes = !e_hkurz.is_empty() && !hkurz.is_empty();
    if both_codes {
        let twins = (e_hkurz == "misc" && hkurz == "mrai") || (e_hkurz == "mrai" && hkurz == "misc");
        if e_hkurz == hkurz || twins {
            return names_compatible(&e_name, &wanted);
        }
    }
    let e_maker = slim_of(entry, "hersteller");
    if e_maker.is_empty() && !both_codes && from_basetool(entry) {
        return names_compatible(&e_name, &wanted);
    }
    if e_name != wanted {
        return false;
    }
    e_maker == slim(manufacturer)
}

fn find(ships: &[Map], name: &str, manufacturer: &str, kurz: &str, hkurz: &str) -> Option<usize> {
    ships.iter().position(|s| same_ship(s, name, manufacturer, kurz, hkurz))
}

/// Fill in what is missing, never touch what is there (`fleet._fill`).
fn fill(entry: &mut Map, rest: &Map) -> bool {
    let mut changed = false;
    for key in IMPORT_KEYS {
        let value = get(rest, key);
        if none_or_empty(value) {
            continue;
        }
        let value = value.cloned().unwrap_or(Value::Null);
        if (key == "lti" || key == "versicherung") && truthy(get(entry, BASETOOL_COVER)) {
            continue;
        }
        if key == "lti" || key == "warbond" {
            if value == Value::Bool(true) && !is_true(get(entry, key)) {
                map_set(entry, key, Value::Bool(true));
                changed = true;
            }
            continue;
        }
        if none_empty_or_zero(get(entry, key)) {
            map_set(entry, key, value);
            changed = true;
        }
    }
    changed
}

fn months(v: Option<&Value>) -> i64 {
    py_int(v).map_or(0, |n| n.max(0))
}

fn take_over(entry: &mut Map, origin: &str, source: Option<&Value>) {
    map_set(entry, "herkunft", Value::str(if origin.is_empty() { PLEDGE } else { origin }));
    map_set(entry, FROM_BASETOOL, Value::Bool(true));
    if let Some(source) = source.filter(|s| truthy(Some(s))) {
        map_set(entry, "quelle", source.clone());
    }
    if truthy(get(entry, "lti")) || months(get(entry, "versicherung")) != 0 {
        map_set(entry, BASETOOL_COVER, Value::Bool(true));
    }
}

fn promote(entry: &mut Map, name: &str, manufacturer: &str, origin: &str, rest: &Map) {
    map_set(entry, "herkunft", Value::str(if origin.is_empty() { PLEDGE } else { origin }));
    if !py_strip(name).is_empty() {
        map_set(entry, "name", Value::str(strip_owned(name)));
    }
    if !py_strip(manufacturer).is_empty() {
        map_set(entry, "hersteller", Value::str(strip_owned(manufacturer)));
    }
    if !py_strip(text(rest, "hkurz")).is_empty() {
        map_set(entry, "hkurz", Value::str(strip_owned(text(rest, "hkurz"))));
    }
    map_set(entry, "paket", Value::str(strip_owned(text(rest, "paket"))));
    match get(rest, "quelle").filter(|q| truthy(Some(q))) {
        Some(source) => map_set(entry, "quelle", source.clone()),
        None => pop(entry, "quelle"),
    }
}

/// Enter a ship; true when it is new (`fleet.add`).
fn add(ships: &mut Vec<Map>, name: &str, manufacturer: &str, origin: &str, rest: &Map, catalog: &[Map]) -> bool {
    if py_strip(name).is_empty() {
        return false;
    }
    let mut found = find(ships, name, manufacturer, text(rest, "kurz"), text(rest, "hkurz"));
    if found.is_none() && origin == BASETOOL && py_strip(manufacturer).is_empty() {
        let same: Vec<usize> =
            (0..ships.len()).filter(|&i| names_compatible(&slim_of(&ships[i], "name"), &slim(name))).collect();
        if same.len() == 1 {
            found = Some(same[0]);
        }
    }
    if found.is_none() && origin != CATALOG {
        found = catalog_entry(ships, name, text(rest, "hkurz"), catalog);
    }
    if let Some(i) = found {
        let herkunft = text(&ships[i], "herkunft").to_string();
        let promoted = herkunft == CATALOG && origin != CATALOG;
        if promoted {
            promote(&mut ships[i], name, manufacturer, origin, rest);
        } else if herkunft == BASETOOL && origin != BASETOOL && origin != CATALOG {
            take_over(&mut ships[i], origin, get(rest, "quelle"));
        }
        fill(&mut ships[i], rest);
        return promoted;
    }
    let mut entry = object(vec![
        ("name", Value::str(strip_owned(name))),
        ("hersteller", Value::str(strip_owned(manufacturer))),
        ("herkunft", Value::str(origin)),
        ("belegung", Value::Object(Map::new())),
    ]);
    for key in IMPORT_KEYS {
        if let Some(value) = get(rest, key).filter(|v| !none_or_empty(Some(v))) {
            map_set(&mut entry, key, value.clone());
        }
    }
    if let Some(source) = get(rest, "quelle").filter(|q| truthy(Some(q))) {
        map_set(&mut entry, "quelle", source.clone());
    }
    ships.push(entry);
    true
}

/// Works on the ship entries of a hangar as a list and puts them back.
pub fn edit_ships<R>(data: &mut Map, work: impl FnOnce(&mut Vec<Map>) -> R) -> R {
    let mut ships = take_ships(data);
    let result = work(&mut ships);
    put_ships(data, ships);
    result
}

/// Enter a ship in the data of the file; true when it is new (`fleet.add`).
/// The sync with the Basetool takes the ships it finds there in this way.
pub fn add_ship(data: &mut Map, name: &str, manufacturer: &str, origin: &str, rest: &Map) -> bool {
    let catalog = catalog_entries();
    let mut ships = take_ships(data);
    let new = add(&mut ships, name, manufacturer, origin, rest, catalog);
    put_ships(data, ships);
    new
}

/// Merge duplicate entries; the first stays (`fleet.merge_duplicates`).
pub fn merge_duplicates(data: &mut Map) -> bool {
    let ships = take_ships(data);
    let mut kept: Vec<Map> = Vec::new();
    let mut changed = false;
    for mut s in ships {
        let hit = kept.iter().position(|k| {
            same_ship(k, text(&s, "name"), text(&s, "hersteller"), text(&s, "kurz"), text(&s, "hkurz"))
                || same_ship(&s, text(k, "name"), text(k, "hersteller"), text(k, "kurz"), text(k, "hkurz"))
        });
        let Some(index) = hit else {
            kept.push(s);
            continue;
        };
        changed = true;
        if text(&s, "herkunft") == BASETOOL && text(&kept[index], "herkunft") != BASETOOL {
            std::mem::swap(&mut kept[index], &mut s);
        }
        let mut rest: Map = IMPORT_KEYS.iter().filter_map(|k| get(&s, k).map(|v| (k.to_string(), v.clone()))).collect();
        let target = &mut kept[index];
        let s_origin = text(&s, "herkunft");
        if text(target, "herkunft") == CATALOG && s_origin != CATALOG {
            let mut with_source = rest.clone();
            map_set(&mut with_source, "quelle", get(&s, "quelle").cloned().unwrap_or(Value::Null));
            promote(target, text(&s, "name"), text(&s, "hersteller"), s_origin, &with_source);
        } else if s_origin == CATALOG {
            map_set(&mut rest, "paket", Value::str(""));
        } else if text(target, "herkunft") == BASETOOL && s_origin != BASETOOL {
            take_over(target, s_origin, get(&s, "quelle"));
        }
        fill(target, &rest);
        if !truthy(get(target, "belegung")) && truthy(get(&s, "belegung")) {
            map_set(target, "belegung", get(&s, "belegung").cloned().unwrap_or(Value::Null));
        }
        for key in ["einsatzbereit", "einheit", "schmelzbar", "schmelz_wahl"] {
            if is_none(get(target, key)) && !is_none(get(&s, key)) {
                map_set(target, key, get(&s, key).cloned().unwrap_or(Value::Null));
            }
        }
    }
    put_ships(data, kept);
    changed
}

// ------------------------------------------------------------ wishlist

/// `fleet.wishlist_add`: true when the ship is new on the wishlist.
pub fn wishlist_add(data: &mut Map, name: &str, manufacturer: &str) -> bool {
    if py_strip(name).is_empty() {
        return false;
    }
    let wanted = slim(name);
    let list = match data.iter_mut().find(|(k, _)| k == "wunsch") {
        Some((_, Value::Array(items))) => items,
        _ => {
            map_set(data, "wunsch", Value::Array(Vec::new()));
            match data.iter_mut().find(|(k, _)| k == "wunsch") {
                Some((_, Value::Array(items))) => items,
                _ => return false,
            }
        }
    };
    if list.iter().any(|w| w.as_object().is_some_and(|w| slim_of(w, "name") == wanted)) {
        return false;
    }
    list.push(Value::Object(object(vec![
        ("name", Value::str(strip_owned(name))),
        ("hersteller", Value::str(strip_owned(manufacturer))),
        ("belegung", Value::Object(Map::new())),
    ])));
    true
}

/// `fleet.wishlist`: the ships one has in mind, alphabetical. A wish is no property: it never flows into
/// the answer whether a part fits the own ship. A missing field counts as an empty list.
pub fn wishlist(data: &Map) -> Vec<&Map> {
    let mut found: Vec<&Map> = match get(data, "wunsch") {
        Some(Value::Array(items)) => {
            items.iter().filter_map(Value::as_object).filter(|w| truthy(get(w, "name"))).collect()
        }
        _ => Vec::new(),
    };
    found.sort_by_cached_key(|w| text(w, "name").to_lowercase());
    found
}

/// `fleet.wishlist_contains`: is a ship of this name on the wishlist (compared as `slim` writes it)?
pub fn wishlist_contains(data: &Map, name: &str) -> bool {
    let wanted = slim(name);
    match get(data, "wunsch") {
        Some(Value::Array(items)) => items.iter().any(|w| w.as_object().is_some_and(|w| slim_of(w, "name") == wanted)),
        _ => false,
    }
}

/// `fleet.wishlist_remove`: strikes an entry; true when one fell away. The list stands in the data
/// afterwards, also where there was none.
pub fn wishlist_remove(data: &mut Map, name: &str) -> bool {
    let wanted = slim(name);
    let before = match get(data, "wunsch") {
        Some(Value::Array(items)) => items.len(),
        _ => 0,
    };
    let kept: Vec<Value> = match get(data, "wunsch") {
        Some(Value::Array(items)) => {
            items.iter().filter(|w| !w.as_object().is_some_and(|w| slim_of(w, "name") == wanted)).cloned().collect()
        }
        _ => Vec::new(),
    };
    let after = kept.len();
    map_set(data, "wunsch", Value::Array(kept));
    after != before
}

/// `fleet.contains` on the hangar: does a ship with this name stand in the hangar already?
pub fn in_hangar(data: &Map, name: &str, manufacturer: &str, kurz: &str, hkurz: &str) -> bool {
    ships(data).into_iter().any(|s| same_ship(s, name, manufacturer, kurz, hkurz))
}

/// Does the hangar hold the ship that is being put on the wish list? `maker` is what UEX has in front of the
/// name. ⚠ Deviation from 4.12 (`contains(data, name)` with no maker): there, an entry with a maker never
/// matched, so a ship one owns could still be wished for. A maker counts as the same when one spelling begins with
/// the other ("Anvil" and "Anvil Aerospace"), and a maker missing on either side counts as the same.
pub fn owns_wished(data: &Map, name: &str, maker: &str) -> bool {
    if in_hangar(data, name, maker, "", "") {
        return true;
    }
    let (wanted, maker) = (slim(name), slim(maker));
    !wanted.is_empty()
        && ships(data).into_iter().any(|entry| {
            let e_maker = slim_of(entry, "hersteller");
            slim_of(entry, "name") == wanted
                && (e_maker.is_empty()
                    || maker.is_empty()
                    || e_maker.starts_with(&maker)
                    || maker.starts_with(&e_maker))
        })
}

// ------------------------------------------------------------ between the hangar and the wish list

/// Why a ship was not moved between the own ships and the wish list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveRefused {
    /// The entry is not there (any more).
    Gone,
    /// A vehicle that comes with a package follows its carrier: it does not move on its own.
    Included,
}

/// How a ship came to its new list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moved {
    /// As an entry of its own.
    New,
    /// The list held the same ship already: its equipments joined that entry.
    Joined,
}

/// The own ship that a wish of this name and maker stands for, as [`owns_wished`] decides it.
fn owned_index(ships: &[Map], name: &str, maker: &str) -> Option<usize> {
    if let Some(at) = find(ships, name, maker, "", "") {
        return Some(at);
    }
    let (wanted, maker) = (slim(name), slim(maker));
    if wanted.is_empty() {
        return None;
    }
    ships.iter().position(|entry| {
        let e_maker = slim_of(entry, "hersteller");
        slim_of(entry, "name") == wanted
            && (e_maker.is_empty() || maker.is_empty() || e_maker.starts_with(&maker) || maker.starts_with(&e_maker))
    })
}

/// `source` joins `target`, an entry of the same ship. A target without any plan and without equipments takes the
/// plan and the equipments as they are; else the equipments of the source come in as further ones
/// ([`crate::equipment::take_in`]). A key the target lacks comes along.
fn join(target: &mut Map, source: &Map, stock: &[(String, String)]) {
    let bare = !truthy(get(target, "belegung"))
        && !matches!(get(target, crate::equipment::KEY), Some(Value::Array(items)) if !items.is_empty());
    if bare {
        for key in ["belegung", crate::equipment::KEY] {
            if let Some(value) = get(source, key) {
                map_set(target, key, value.clone());
            }
        }
    } else {
        crate::equipment::take_in(target, source, stock);
    }
    for (key, value) in source {
        if get(target, key).is_none() {
            map_set(target, key, value.clone());
        }
    }
}

/// A planned ship was bought: the whole entry goes into the own ships, with its plan, its equipments and whatever
/// else it holds; it counts as bought in the game where it says nothing else. Where the hangar holds the ship
/// already, the wish joins that entry. `stock` is the factory fitting of the ship (only deviations are stored).
pub fn wish_to_hangar(data: &mut Map, name: &str, stock: &[(String, String)]) -> Result<Moved, MoveRefused> {
    let wanted = slim(name);
    if wanted.is_empty() {
        return Err(MoveRefused::Gone);
    }
    let wish = match data.iter_mut().find(|(k, _)| k == "wunsch") {
        Some((_, Value::Array(items))) => {
            let at = items
                .iter()
                .position(|w| w.as_object().is_some_and(|w| slim_of(w, "name") == wanted))
                .ok_or(MoveRefused::Gone)?;
            match items.remove(at) {
                Value::Object(wish) => wish,
                _ => return Err(MoveRefused::Gone),
            }
        }
        _ => return Err(MoveRefused::Gone),
    };
    let mut ships = take_ships(data);
    let moved = match owned_index(&ships, text(&wish, "name"), text(&wish, "hersteller")) {
        Some(at) => {
            join(&mut ships[at], &wish, stock);
            Moved::Joined
        }
        None => {
            let mut entry = wish;
            if py_strip(text(&entry, "herkunft")).is_empty() {
                map_set(&mut entry, "herkunft", Value::str(INGAME));
            }
            if get(&entry, "belegung").is_none() {
                map_set(&mut entry, "belegung", Value::Object(Map::new()));
            }
            ships.push(entry);
            Moved::New
        }
    };
    put_ships(data, ships);
    Ok(moved)
}

/// An own ship is gone (a wipe): the entry at `index` of [`ships`] goes back onto the wish list, with its plan, its
/// equipments and whatever else it holds, so a move back brings all of it again. The vehicles of its package leave
/// the hangar with it, as when it is removed. Where the wish list holds the ship already, it joins that wish.
pub fn hangar_to_wish(data: &mut Map, index: usize, stock: &[(String, String)]) -> Result<Moved, MoveRefused> {
    let mut ships = take_ships(data);
    let refused = match ships.get(index) {
        None => Some(MoveRefused::Gone),
        Some(entry) if text(entry, "herkunft") == CATALOG => Some(MoveRefused::Included),
        Some(_) => None,
    };
    if let Some(refused) = refused {
        put_ships(data, ships);
        return Err(refused);
    }
    let entry = ships.remove(index);
    prune(&mut ships, catalog_entries());
    put_ships(data, ships);
    let wanted = slim_of(&entry, "name");
    if !matches!(get(data, "wunsch"), Some(Value::Array(_))) {
        map_set(data, "wunsch", Value::Array(Vec::new()));
    }
    let Some((_, Value::Array(wishes))) = data.iter_mut().find(|(k, _)| k == "wunsch") else {
        return Err(MoveRefused::Gone);
    };
    let same = wishes.iter_mut().find_map(|w| match w {
        Value::Object(w) if slim_of(w, "name") == wanted => Some(w),
        _ => None,
    });
    match same {
        Some(wish) => {
            join(wish, &entry, stock);
            Ok(Moved::Joined)
        }
        None => {
            wishes.push(Value::Object(entry));
            Ok(Moved::New)
        }
    }
}

// ------------------------------------------------------------ included vehicles

const BUNDLED_VEHICLES: &str = include_str!("../../../daten/beiboote.json");
const RSI_HOST: &str = "robertsspaceindustries.com";

/// The entries of the bundled vehicle catalog (`daten/beiboote.json`).
pub fn catalog_entries() -> &'static [Map] {
    static ENTRIES: OnceLock<Vec<Map>> = OnceLock::new();
    ENTRIES.get_or_init(|| parse_catalog(BUNDLED_VEHICLES))
}

fn parse_catalog(text_: &str) -> Vec<Map> {
    match jsonio::loads(text_) {
        Ok(Value::Object(root)) => match get(&root, "schiffe") {
            Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_object().cloned()).collect(),
            _ => Vec::new(),
        },
        Ok(_) => Vec::new(),
        Err(e) => {
            errors::record("included_vehicles.load", &e);
            Vec::new()
        }
    }
}

fn record_names(record: &Map) -> HashSet<String> {
    let main = match get(record, "schiff") {
        Some(v) if truthy(Some(v)) => v.as_str().unwrap_or(""),
        _ => text(record, "name"),
    };
    let mut result = HashSet::from([slim(main)]);
    if let Some(Value::Array(also)) = get(record, "auch") {
        result.extend(also.iter().filter_map(Value::as_str).map(slim));
    }
    result.remove("");
    result
}

/// The host of a URL like `urllib.parse.urlsplit(...).hostname`, and its scheme.
fn url_scheme_host(url: &str) -> (String, String) {
    let Some((scheme, rest)) = url.split_once(':') else { return (String::new(), String::new()) };
    let scheme_ok = scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    if !scheme_ok {
        return (String::new(), String::new());
    }
    let Some(rest) = rest.strip_prefix("//") else { return (scheme.to_lowercase(), String::new()) };
    let netloc = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = netloc.rsplit_once('@').map_or(netloc, |(_, h)| h);
    let host = if let Some(inner) = host.strip_prefix('[') {
        inner.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    (scheme.to_lowercase(), host.to_lowercase())
}

fn has_evidence(record: &Map) -> bool {
    let Some(Value::Object(proof)) = get(record, "beleg") else { return false };
    let address = match get(proof, "adresse") {
        Some(Value::Str(s)) => s.clone(),
        Some(v) if truthy(Some(v)) => jsonio::dumps(v, None, false),
        _ => String::new(),
    };
    let (scheme, host) = url_scheme_host(&address);
    let filled = |key: &str| match get(proof, key) {
        Some(Value::Str(s)) => !py_strip(s).is_empty(),
        Some(v) => truthy(Some(v)),
        None => false,
    };
    scheme == "https"
        && (host == RSI_HOST || host.ends_with(&format!(".{RSI_HOST}")))
        && filled("abgerufen")
        && filled("auszug")
}

fn record_for<'a>(entry: &Map, catalog: &'a [Map]) -> Option<&'a Map> {
    let wanted = slim_of(entry, "name");
    if wanted.is_empty() {
        return None;
    }
    let maker = slim_of(entry, "hkurz");
    catalog.iter().find(|record| {
        if !record_names(record).contains(&wanted) {
            return false;
        }
        let code = slim_of(record, "hkurz");
        !(!maker.is_empty() && !code.is_empty() && maker != code)
    })
}

fn vehicles_for(entry: &Map, catalog: &[Map]) -> Vec<Map> {
    let Some(record) = record_for(entry, catalog) else { return Vec::new() };
    if !has_evidence(record) {
        return Vec::new();
    }
    match get(record, "fahrzeuge") {
        Some(Value::Array(items)) => {
            items.iter().filter_map(Value::as_object).filter(|v| !slim_of(v, "name").is_empty()).cloned().collect()
        }
        _ => Vec::new(),
    }
}

fn present(ships: &[&Map], vehicle: &Map) -> Option<usize> {
    let wanted = record_names(vehicle);
    ships.iter().position(|s| {
        wanted.contains(&slim_of(s, "name"))
            || same_ship(s, text(vehicle, "name"), text(vehicle, "hersteller"), "", text(vehicle, "hkurz"))
    })
}

fn vehicle_named<'a>(name: &str, catalog: &'a [Map]) -> Option<&'a Map> {
    let wanted = slim(name);
    if wanted.is_empty() {
        return None;
    }
    catalog.iter().find_map(|record| match get(record, "fahrzeuge") {
        Some(Value::Array(items)) => {
            items.iter().filter_map(Value::as_object).find(|v| record_names(v).contains(&wanted))
        }
        _ => None,
    })
}

fn catalog_entry(ships: &[Map], name: &str, maker_code: &str, catalog: &[Map]) -> Option<usize> {
    let wanted = match vehicle_named(name, catalog) {
        Some(vehicle) => record_names(vehicle),
        None => HashSet::from([slim(name)]),
    };
    let maker = slim(maker_code);
    ships.iter().position(|s| {
        if text(s, "herkunft") != CATALOG || !wanted.contains(&slim_of(s, "name")) {
            return false;
        }
        let code = slim_of(s, "hkurz");
        !(!maker.is_empty() && !code.is_empty() && maker != code)
    })
}

fn absorb(target: &mut Map, entry: &Map) {
    if !truthy(get(target, "belegung")) && truthy(get(entry, "belegung")) {
        map_set(target, "belegung", get(entry, "belegung").cloned().unwrap_or(Value::Null));
    }
    for key in ["einsatzbereit", "einheit", "schmelz_wahl"] {
        if is_none(get(target, key)) && !is_none(get(entry, key)) {
            map_set(target, key, get(entry, key).cloned().unwrap_or(Value::Null));
        }
    }
}

/// `[(vehicle, carrier name)]`: what belongs to the hangar by the catalog.
fn wanted(ships: &[Map], catalog: &[Map]) -> Vec<(Map, String)> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for s in ships.iter().filter(|s| text(s, "herkunft") != CATALOG) {
        for vehicle in vehicles_for(s, catalog) {
            if seen.insert(slim_of(&vehicle, "name")) {
                result.push((vehicle, text(s, "name").to_string()));
            }
        }
    }
    result
}

/// Remove catalog vehicles that no longer belong; returns their names.
fn prune(ships: &mut Vec<Map>, catalog: &[Map]) -> Vec<String> {
    let keep: HashSet<String> = wanted(ships, catalog).iter().map(|(v, _)| slim_of(v, "name")).collect();
    let others: Vec<usize> = (0..ships.len()).filter(|&i| text(&ships[i], "herkunft") != CATALOG).collect();
    let mut gone = Vec::new();
    for i in 0..ships.len() {
        if text(&ships[i], "herkunft") != CATALOG {
            continue;
        }
        let vehicle = vehicle_named(text(&ships[i], "name"), catalog).cloned().unwrap_or_else(|| ships[i].clone());
        if !keep.contains(&slim_of(&vehicle, "name")) {
            gone.push(i);
            continue;
        }
        let other_refs: Vec<&Map> = others.iter().map(|&j| &ships[j]).collect();
        if let Some(pos) = present(&other_refs, &vehicle) {
            let twin = others[pos];
            let entry = ships[i].clone();
            absorb(&mut ships[twin], &entry);
            gone.push(i);
        }
    }
    let names = gone.iter().map(|&i| text(&ships[i], "name").to_string()).collect();
    if !gone.is_empty() {
        let mut index = 0;
        ships.retain(|_| {
            let keep = !gone.contains(&index);
            index += 1;
            keep
        });
    }
    names
}

const PLEDGE_KEYS: [&str; 9] =
    ["lti", "versicherung", "warbond", "gekauft", "preis", "schmelzbar", "quelle", "kurz", "versicherung_basetool"];

/// Removed ships that belong to a remaining carrier stay as catalog vehicles;
/// returns their indices in `gone`.
fn demote(kept: &[Map], gone: &mut [Map], catalog: &[Map]) -> Vec<usize> {
    let mut back: Vec<usize> = Vec::new();
    let kept_refs: Vec<&Map> = kept.iter().collect();
    for (vehicle, carrier) in wanted(kept, catalog) {
        if present(&kept_refs, &vehicle).is_some() {
            continue;
        }
        let free: Vec<usize> = (0..gone.len()).filter(|i| !back.contains(i)).collect();
        let free_refs: Vec<&Map> = free.iter().map(|&i| &gone[i]).collect();
        let Some(pos) = present(&free_refs, &vehicle) else { continue };
        let index = free[pos];
        let ship = &mut gone[index];
        map_set(ship, "herkunft", Value::str(CATALOG));
        map_set(ship, "paket", Value::str(carrier));
        for key in PLEDGE_KEYS {
            pop(ship, key);
        }
        back.push(index);
    }
    back
}

/// Add catalog vehicles and remove orphaned ones.
fn apply(ships: &mut Vec<Map>, catalog: &[Map]) {
    prune(ships, catalog);
    for (vehicle, carrier) in wanted(ships, catalog) {
        let refs: Vec<&Map> = ships.iter().collect();
        if let Some(i) = present(&refs, &vehicle) {
            if text(&ships[i], "herkunft") == CATALOG {
                map_set(&mut ships[i], "paket", Value::str(carrier));
            }
            continue;
        }
        ships.push(object(vec![
            ("name", Value::str(strip_owned(text(&vehicle, "name")))),
            ("hersteller", Value::str(strip_owned(text(&vehicle, "hersteller")))),
            ("hkurz", Value::str(strip_owned(text(&vehicle, "hkurz")))),
            ("herkunft", Value::str(CATALOG)),
            ("paket", Value::str(carrier)),
            ("belegung", Value::Object(Map::new())),
        ]));
    }
}

// ------------------------------------------------------------ ship catalog (UEX)

/// The names of the UEX ship list, for names that come without a maker.
#[derive(Clone, Debug, Default)]
pub struct ShipNames(pub Vec<String>);

impl ShipNames {
    /// From `schiffe.json` in the data folder; empty when missing or of
    /// another format.
    pub fn load(path: &str) -> ShipNames {
        let Ok(Value::Object(root)) = jsonio::read_json(path) else { return ShipNames::default() };
        if py_int(get(&root, "format")) != Some(SHIP_CATALOG_FORMAT)
            || matches!(get(&root, "format"), Some(Value::Bool(_)))
        {
            return ShipNames::default();
        }
        let names = match get(&root, "schiffe") {
            Some(Value::Object(ships)) => {
                ships.iter().filter_map(|(_, s)| s.as_object()).map(|s| text(s, "name").to_string()).collect()
            }
            _ => Vec::new(),
        };
        ShipNames(names)
    }

    /// `ships._find`: equal, else the only name that ends with it.
    fn find(&self, name: &str) -> Option<&str> {
        self.position(name).map(|i| self.0[i].as_str())
    }

    /// Where `find` finds the name.
    pub(crate) fn position(&self, name: &str) -> Option<usize> {
        let wanted = py_strip(name).to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        if let Some(hit) = self.0.iter().position(|n| n.to_lowercase() == wanted) {
            return Some(hit);
        }
        let suffix = format!(" {wanted}");
        let ends: Vec<usize> = (0..self.0.len()).filter(|&i| self.0[i].to_lowercase().ends_with(&suffix)).collect();
        if ends.len() == 1 { Some(ends[0]) } else { None }
    }
}

/// `aurora mr` → `Aurora MR`, only when the list writes everything in lower case.
pub(crate) fn nice(name: &str) -> String {
    let name = py_strip(name);
    if name.is_empty() || name != name.to_lowercase() {
        return name.to_string();
    }
    name.split(py_isspace)
        .filter(|w| !w.is_empty())
        .map(|word| {
            if word.chars().count() <= 2 && word.chars().all(char::is_alphabetic) {
                word.to_uppercase()
            } else {
                let mut chars = word.chars();
                let first: String = chars.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                first + chars.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn split_maker(full: &str) -> (String, String, String) {
    let full = py_strip(full);
    let lower = full.to_lowercase();
    for (maker, code) in CSV_MAKERS {
        if lower.starts_with(&format!("{} ", maker.to_lowercase())) {
            let rest: String = full.chars().skip(maker.chars().count()).collect();
            return (maker.to_string(), code.to_string(), py_strip(&rest).to_string());
        }
    }
    (String::new(), String::new(), full.to_string())
}

pub(crate) fn catalog_name(name: &str, ships: &ShipNames) -> (String, String, String) {
    let Some(full) = ships.find(name) else { return (nice(name), String::new(), String::new()) };
    let (maker, code, rest) = split_maker(full);
    if rest.is_empty() || maker.is_empty() {
        return (nice(name), String::new(), String::new());
    }
    (rest, maker, code)
}

// ------------------------------------------------------------ ship list `hangar-schiffe`

static EXT_INSURANCE: OnceLock<Option<Regex>> = OnceLock::new();

pub(crate) fn extension_insurance(entry: &Map) -> (bool, i64) {
    let mut lti = false;
    let mut months_ = 0i64;
    match get(entry, "insurance") {
        Some(Value::Bool(b)) => lti = *b,
        Some(Value::Str(value)) => {
            let t = py_strip(value);
            if t.to_lowercase() == "lti" {
                lti = true;
            } else {
                let re = EXT_INSURANCE.get_or_init(|| {
                    Regex::new(&format!(r"(?i)^{s}*(\d+){s}*(?:mi|mo|months?)?{s}*$", s = crate::text::PY_SPACE)).ok()
                });
                if let Some(c) = re.as_ref().and_then(|re| re.captures(t)) {
                    months_ = py_int(Some(&Value::Str(c[1].to_string()))).unwrap_or(i64::MAX);
                }
            }
        }
        _ => {}
    }
    let extra = get(entry, "insuranceMonths");
    if !truthy(extra) {
        months_ = months_.max(0);
    } else if let Some(n) = py_int(extra) {
        months_ = months_.max(n);
    }
    (lti, months_)
}

fn text_field(entry: &Map, key: &str) -> String {
    strip_owned(text(entry, key))
}

fn usd(value: Option<&Value>) -> String {
    let amount = match value {
        Some(Value::Int(n)) => n.parse::<f64>().ok(),
        Some(Value::Float(x)) => Some(*x),
        _ => None,
    };
    match amount {
        Some(x) if x >= 0.0 => format!("${x:.2} USD"),
        _ => String::new(),
    }
}

fn hangar_ships_matrix(entry: &Map) -> String {
    match get(entry, "matrixZuordnung") {
        Some(Value::Str(s)) if MATRIX_MATCHES.contains(&s.as_str()) => text_field(entry, "matrix"),
        _ => String::new(),
    }
}

fn hangar_ships_entry(entry: &Map, name_first: bool) -> Option<Map> {
    let empty_maker = Map::new();
    let maker = match get(entry, "manufacturer") {
        Some(Value::Object(m)) => m,
        _ => &empty_maker,
    };
    let matrix = hangar_ships_matrix(entry);
    let label = text_field(entry, "name");
    let name = if name_first {
        if label.is_empty() { matrix.clone() } else { label.clone() }
    } else if matrix.is_empty() {
        label.clone()
    } else {
        matrix.clone()
    };
    if name.is_empty() {
        return None;
    }
    let (lti, months_) = extension_insurance(entry);
    let code = text_field(entry, "code");
    let mut ship = object(vec![
        ("name", Value::str(name.clone())),
        ("hersteller", Value::str(text_field(maker, "name"))),
        ("kurz", Value::str(if code.is_empty() { text_field(entry, "rsiKennung") } else { code })),
        ("hkurz", Value::str(text_field(maker, "code"))),
        ("lti", Value::Bool(lti)),
        ("warbond", Value::Bool(is_true(get(entry, "warbond")))),
        ("versicherung", if months_ != 0 && !lti { Value::int(months_) } else { Value::Null }),
        ("paket", Value::str(text_field(entry, "includedWith"))),
        ("gekauft", Value::str(text_field(entry, "purchaseDate"))),
        ("preis", Value::str(usd(get(entry, "pledgePriceUsd")))),
    ]);
    if name == matrix
        && text(entry, "matrixZuordnung") == "naeherung"
        && !label.is_empty()
        && slim(&label) != slim(&matrix)
    {
        map_set(&mut ship, "naeherung", Value::str(label));
    }
    Some(ship)
}

/// What `_included_ships` sees of one export entry.
pub(crate) struct View {
    pub(crate) matrix: String,
    pub(crate) name: String,
    pub(crate) included: Vec<Value>,
}

pub(crate) fn included_ships(views: &[View], ships: &[Option<Map>], names: &ShipNames) -> Vec<Map> {
    let mut known: HashSet<String> = HashSet::new();
    for (view, ship) in views.iter().zip(ships) {
        if let Some(ship) = ship {
            for value in [text(ship, "name"), &view.matrix, &view.name] {
                if !slim(value).is_empty() {
                    known.insert(slim(value));
                }
            }
        }
    }
    let mut known_codes: HashSet<String> =
        ships.iter().flatten().map(|s| slim_of(s, "kurz")).filter(|c| !c.is_empty()).collect();
    let mut result = Vec::new();
    for (view, ship) in views.iter().zip(ships) {
        let Some(ship) = ship else { continue };
        for included in &view.included {
            let (label, code) = match included {
                Value::Object(m) => (get(m, "name").cloned(), get(m, "code").cloned()),
                other => (Some(other.clone()), None),
            };
            let code = match code {
                Some(Value::Str(c)) => strip_owned(&c),
                _ => String::new(),
            };
            let Some(Value::Str(label)) = label else { continue };
            if slim(&label).is_empty() {
                continue;
            }
            if known.contains(&slim(&label)) || (!code.is_empty() && known_codes.contains(&slim(&code))) {
                continue;
            }
            known.insert(slim(&label));
            if !code.is_empty() {
                known_codes.insert(slim(&code));
            }
            let (name, maker, maker_code) = catalog_name(py_strip(&label), names);
            result.push(object(vec![
                ("name", Value::str(name)),
                ("hersteller", Value::str(maker)),
                ("kurz", Value::str(code)),
                ("hkurz", Value::str(maker_code)),
                ("lti", get(ship, "lti").cloned().unwrap_or(Value::Null)),
                ("warbond", Value::Bool(false)),
                ("versicherung", get(ship, "versicherung").cloned().unwrap_or(Value::Null)),
                ("paket", Value::str(text(ship, "name"))),
                ("gekauft", Value::str("")),
                ("preis", Value::str("")),
                ("beilage", Value::Bool(true)),
            ]));
        }
    }
    result
}

/// Is the version `1` (and not a truth value)?
pub fn version_one(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Int(n)) => n == "1",
        Some(Value::Float(x)) => *x == 1.0,
        _ => false,
    }
}

/// The ship list `hangar-schiffe` → entries as read by the Python version.
pub fn from_hangar_ships(raw: &Map) -> Result<Vec<Map>, Rejected> {
    from_hangar_ships_with(raw, &ShipNames::default())
}

/// Like `from_hangar_ships`, with the UEX ship names for included vehicles.
pub fn from_hangar_ships_with(raw: &Map, names: &ShipNames) -> Result<Vec<Map>, Rejected> {
    if !version_one(get(raw, "version")) {
        return Err(Rejected::Version);
    }
    let entries: Vec<&Map> = match get(raw, "schiffe") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    };
    let mut codes_by_matrix: Vec<(String, HashSet<String>)> = Vec::new();
    for entry in &entries {
        let matrix = slim(&hangar_ships_matrix(entry));
        if matrix.is_empty() {
            continue;
        }
        let code = text_field(entry, "code");
        match codes_by_matrix.iter_mut().find(|(m, _)| *m == matrix) {
            Some((_, codes)) => {
                codes.insert(code);
            }
            None => codes_by_matrix.push((matrix, HashSet::from([code]))),
        }
    }
    let shared: HashSet<&String> = codes_by_matrix.iter().filter(|(_, c)| c.len() > 1).map(|(m, _)| m).collect();
    let ships: Vec<Option<Map>> =
        entries.iter().map(|e| hangar_ships_entry(e, shared.contains(&slim(&hangar_ships_matrix(e))))).collect();
    let views: Vec<View> = entries
        .iter()
        .map(|e| View {
            matrix: hangar_ships_matrix(e),
            name: text_field(e, "name"),
            included: match get(e, "includedShips") {
                Some(Value::Array(items)) => items.clone(),
                _ => Vec::new(),
            },
        })
        .collect();
    let mut result: Vec<Map> = ships.iter().flatten().cloned().collect();
    result.extend(included_ships(&views, &ships, names));
    Ok(result)
}

/// The JSON object of a file or delivery: which way it goes.
pub fn read_object(raw: &Value, names: &ShipNames) -> Result<Vec<Map>, Rejected> {
    let Value::Object(map) = raw else { return Ok(Vec::new()) };
    match get(map, "schema") {
        Some(Value::Str(s)) if s == HANGAR_SHIPS_SCHEMA => from_hangar_ships_with(map, names),
        Some(Value::Str(s)) if s == HANGAR_TRANSFER_SCHEMA => Err(Rejected::Transfer),
        _ => Ok(Vec::new()),
    }
}

// ------------------------------------------------------------ import

/// What an import did: new entries, entries already there, removed names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    pub new: usize,
    pub existing: usize,
    pub removed: Vec<String>,
}

/// Take read entries into the hangar (`fleet.import_entries`, without saving).
///
/// ⚠ A pledge export is the whole pledge hangar: a pledge ship it no longer
/// names is removed. Ships bought in the game stay.
pub fn import_entries(data: &mut Map, entries: &[Map]) -> Imported {
    let catalog = catalog_entries();
    let mut ships = take_ships(data);
    let mut new = 0usize;
    let origin_of = |e: &Map| -> String {
        let o = text(e, "herkunft");
        if truthy(get(e, "herkunft")) { o.to_string() } else { PLEDGE.to_string() }
    };
    let pledge_export = entries.iter().all(|e| origin_of(e) == PLEDGE);
    let wishes: Vec<&Map> = entries.iter().filter(|e| truthy(get(e, "wunsch"))).collect();
    let mut by_name: Vec<usize> = Vec::new();
    for e in entries {
        let origin = origin_of(e);
        if truthy(get(e, "wunsch")) {
            continue;
        }
        if (origin == LIST || truthy(get(e, "beilage"))) && !truthy(get(e, "hkurz")) && !truthy(get(e, "hersteller")) {
            let same: Vec<usize> =
                (0..ships.len()).filter(|&i| slim_of(&ships[i], "name") == slim_of(e, "name")).collect();
            if same.len() == 1 {
                let i = same[0];
                if text(&ships[i], "herkunft") == CATALOG {
                    promote(&mut ships[i], text(e, "name"), "", &origin, e);
                    new += 1;
                }
                if origin != LIST {
                    let mut rest = Map::new();
                    for key in ["paket", "lti", "versicherung"] {
                        map_set(&mut rest, key, get(e, key).cloned().unwrap_or(Value::Null));
                    }
                    if text(&ships[i], "herkunft") == BASETOOL {
                        take_over(&mut ships[i], &origin, get(e, "quelle"));
                    }
                    fill(&mut ships[i], &rest);
                }
                by_name.push(i);
                continue;
            }
        }
        let mut rest = Map::new();
        for key in IMPORT_KEYS.iter().chain(&["quelle"]) {
            map_set(&mut rest, key, get(e, key).cloned().unwrap_or(Value::Null));
        }
        if add(&mut ships, text(e, "name"), text(e, "hersteller"), &origin, &rest, catalog) {
            new += 1;
        }
    }
    apply_meltable(&mut ships, entries);
    put_ships(data, ships);
    for e in &wishes {
        let ships_now = take_ships(data);
        let listed = find(&ships_now, text(e, "name"), text(e, "hersteller"), "", text(e, "hkurz")).is_some()
            || ships_now.iter().any(|s| slim_of(s, "name") == slim_of(e, "name"))
            || catalog_entry(&ships_now, text(e, "name"), text(e, "hkurz"), catalog).is_some();
        put_ships(data, ships_now);
        if !listed {
            let _ = wishlist_add(data, text(e, "name"), text(e, "hersteller"));
        }
    }
    let mut ships = take_ships(data);
    let mut removed = Vec::new();
    if !entries.is_empty() && pledge_export {
        let mut kept = Vec::new();
        let mut gone = Vec::new();
        for (i, ship) in ships.into_iter().enumerate() {
            let in_export = by_name.contains(&i)
                || entries.iter().any(|e| {
                    same_ship(&ship, text(e, "name"), text(e, "hersteller"), text(e, "kurz"), text(e, "hkurz"))
                });
            if text(&ship, "herkunft") == PLEDGE && !in_export {
                gone.push(ship);
            } else {
                kept.push(ship);
            }
        }
        if !gone.is_empty() {
            let back = demote(&kept, &mut gone, catalog);
            removed =
                (0..gone.len()).filter(|i| !back.contains(i)).map(|i| text(&gone[i], "name").to_string()).collect();
            for i in back {
                kept.push(gone[i].clone());
            }
        }
        ships = kept;
    }
    apply(&mut ships, catalog);
    put_ships(data, ships);
    Imported { new, existing: entries.len() - new - wishes.len(), removed }
}

fn apply_meltable(ships: &mut [Map], entries: &[Map]) {
    for ship in ships.iter_mut() {
        let values: Vec<bool> = entries
            .iter()
            .filter(|e| same_ship(ship, text(e, "name"), text(e, "hersteller"), text(e, "kurz"), text(e, "hkurz")))
            .filter_map(|e| match get(e, "schmelzbar") {
                Some(Value::Bool(b)) => Some(*b),
                _ => None,
            })
            .collect();
        if !values.is_empty() {
            map_set(ship, "schmelzbar", Value::Bool(values.iter().any(|v| *v)));
        }
    }
}

/// The name of the hangar ship whose package holds `ships[index]`, or `""`
/// (`fleet.bundled_with`): only when `paket`, up to the first ` - `, names
/// another ship of the own hangar.
pub fn bundled_with(ships: &[&Map], index: usize) -> String {
    let entry = ships[index];
    let package = text(entry, "paket");
    let wanted = slim(package.split(" - ").next().unwrap_or(""));
    if wanted.is_empty() || wanted == slim_of(entry, "name") {
        return String::new();
    }
    ships
        .iter()
        .enumerate()
        .find(|(i, s)| *i != index && slim_of(s, "name") == wanted)
        .map(|(_, s)| text(s, "name").to_string())
        .unwrap_or_default()
}

/// The titles the ship list matched only approximately: `Name (Titel)`.
pub fn approximations(entries: &[Map]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| truthy(get(e, "naeherung")))
        .map(|e| format!("{} ({})", text(e, "name"), text(e, "naeherung")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text_: &str) -> Map {
        match jsonio::loads(text_).expect("JSON") {
            Value::Object(m) => m,
            _ => panic!("object"),
        }
    }

    /// `hangar.json` in a new, empty folder of its own.
    fn scratch(name: &str, start: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-hangar-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let path = dir.join(FILE).to_string_lossy().into_owned();
        std::fs::write(&path, start).expect("start");
        path
    }

    const TWO: &str = r#"{"format": 1, "schiffe": [
        {"name": "Carrack", "hersteller": "Anvil", "herkunft": "pledge", "belegung": {}},
        {"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge", "belegung": {}}]}"#;

    fn slot(held: &mut Held, index: usize, path: &str) {
        let entry = ship_mut(&mut held.data, index).expect("ship");
        let part = Value::Object(parse(r#"{"ref": "r", "name": "R", "weg": "kaufen"}"#));
        match entry.iter_mut().find(|(k, _)| k == "belegung") {
            Some((_, Value::Object(slots))) => map_set(slots, path, part),
            _ => map_set(entry, "belegung", Value::Object(vec![(path.to_string(), part)])),
        }
    }

    fn on_disk(path: &str) -> Map {
        read(path).expect("hangar on disk")
    }

    #[test]
    fn a_hangar_file_that_cannot_be_read_is_no_empty_hangar_for_the_sync() {
        let path = scratch("geprueft", TWO);
        let good = load_checked(&path).expect("a good file");
        assert_eq!(ships(&good).len(), 2);
        // No file: an empty hangar, and that is right.
        let missing = format!("{path}.fehlt");
        assert_eq!(ships(&load_checked(&missing).expect("empty")).len(), 0);
        // Damaged, another format, no list: nothing, where `load` would give an empty hangar.
        for broken in ["{kaputt", r#"{"format": 2, "schiffe": []}"#, r#"{"format": 1}"#, "[]"] {
            std::fs::write(&path, broken).expect("write");
            assert!(load_checked(&path).is_none(), "{broken}");
            assert_eq!(ships(&load(&path)).len(), 0, "{broken}");
        }
        let _ = std::fs::remove_dir_all(Path::new(&path).parent().expect("folder"));
    }

    #[test]
    fn two_pages_keep_each_others_changes() {
        let path = scratch("zwei", TWO);
        let mut a = load_held(&path);
        let mut b = load_held(&path);
        slot(&mut b, 0, "p1");
        assert_eq!(save_held(&path, &mut b), Saved::Written);
        assert!(changed_elsewhere(&path, &a));
        map_set(&mut a.data, "notiz", Value::int(1));
        map_set(ship_mut(&mut a.data, 1).expect("Gladius"), "einsatzbereit", Value::Bool(true));
        assert_eq!(save_held(&path, &mut a), Saved::Merged);
        let disk = on_disk(&path);
        assert!(file_text(&disk).contains("\"p1\""), "the slot of the other page stays");
        assert_eq!(get(&disk, "notiz"), Some(&Value::int(1)));
        assert_eq!(get(ships(&disk)[1], "einsatzbereit"), Some(&Value::Bool(true)));

        assert!(remove(&mut b.data, "Gladius", "Aegis"));
        assert_eq!(save_held(&path, &mut b), Saved::Merged);
        let disk = on_disk(&path);
        assert_eq!(ships(&disk).len(), 1, "own removal works");
        assert_eq!(get(&disk, "notiz"), Some(&Value::int(1)), "the other page's top-level key stays");

        // `a` still holds the Gladius; a change elsewhere must not bring it back.
        map_set(ship_mut(&mut a.data, 0).expect("Carrack"), "einsatzbereit", Value::Bool(false));
        assert_eq!(save_held(&path, &mut a), Saved::Merged);
        let disk = on_disk(&path);
        assert_eq!(ships(&disk).len(), 1, "a ship removed elsewhere does not come back");
        assert!(file_text(&disk).contains("\"p1\""));
        // After a merge the page holds less than the file, as in 4.12: it reloads.
        assert!(changed_elsewhere(&path, &a));
        let a = load_held(&path);
        assert!(!changed_elsewhere(&path, &a));
        assert_eq!(ships(&a.data).len(), 1);
    }

    #[test]
    fn a_merge_that_fails_writes_nothing() {
        for (case, disk) in [
            ("wunsch", TWO.replace(r#"{"format": 1,"#, r#"{"format": 1, "wunsch": 5,"#)),
            ("belegung", TWO.replacen(r#""belegung": {}"#, r#""belegung": [1]"#, 1)),
        ] {
            let path = scratch(case, TWO);
            let mut a = load_held(&path);
            std::fs::write(&path, &disk).expect("other place");
            slot(&mut a, 0, "p1");
            assert_eq!(save_held(&path, &mut a), Saved::Failed, "{case}");
            assert_eq!(std::fs::read_to_string(&path).expect("file"), disk, "{case}: nothing written");
        }
    }

    #[test]
    fn a_file_that_is_no_hangar_is_set_aside_not_overwritten() {
        let other = r#"{"format": 2, "schiffe": [{"name": "Polaris"}]}"#;
        let path = scratch("fremd", other);
        let mut a = load_held(&path);
        assert!(ships(&a.data).is_empty());
        assert!(add_by_hand(&mut a.data, "Arrow", false, false));
        assert_eq!(save_held(&path, &mut a), Saved::SetAside);
        assert_eq!(ships(&on_disk(&path)).len(), 1);
        let folder = Path::new(&path).parent().expect("folder");
        let kept: Vec<String> = std::fs::read_dir(folder)
            .expect("list")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("kaputt"))
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(std::fs::read_to_string(folder.join(&kept[0])).expect("aside"), other);

        // A second one in the same second gets a name of its own.
        std::fs::write(&path, other).expect("again");
        assert_eq!(save_held(&path, &mut a), Saved::SetAside);
        let count = std::fs::read_dir(folder)
            .expect("list")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("kaputt"));
        assert_eq!(count.count(), 2);
    }

    #[test]
    fn backup_before_removing() {
        let path = scratch("sicherung", TWO);
        assert!(backup(&path));
        assert_eq!(std::fs::read_to_string(jsonio::backup_name(&path)).expect("bak"), TWO);
        assert!(backup(&format!("{path}.fehlt")));
    }

    fn ship_list(ships: &str) -> Map {
        parse(&format!(r#"{{"schema": "hangar-schiffe", "version": 1, "schiffe": [{ships}]}}"#))
    }

    const PROWLER: &str = r#"{"pledgeId": 1, "matrix": "Prowler Utility", "matrixZuordnung": "sicher",
        "name": "Prowler Utility", "code": "ESPR_Prowler_Utility",
        "manufacturer": {"code": "ESPR", "name": "Esperia"}, "insurance": "LTI",
        "includedWith": null, "includedShips": [], "warbond": false,
        "purchaseDate": "2024-03-02", "pledgePriceUsd": 190}"#;

    #[test]
    fn entry_like_python() {
        let entries = from_hangar_ships(&ship_list(PROWLER)).unwrap();
        assert_eq!(
            jsonio::dumps(&Value::Object(entries[0].clone()), None, false),
            r#"{"name": "Prowler Utility", "hersteller": "Esperia", "kurz": "ESPR_Prowler_Utility", "hkurz": "ESPR", "lti": true, "warbond": false, "versicherung": null, "paket": "", "gekauft": "2024-03-02", "preis": "$190.00 USD"}"#
        );
    }

    #[test]
    fn other_version_and_transfer_are_rejected() {
        let mut raw = ship_list(PROWLER);
        map_set(&mut raw, "version", Value::int(2));
        assert_eq!(from_hangar_ships(&raw), Err(Rejected::Version));
        map_set(&mut raw, "version", Value::Bool(true));
        assert_eq!(from_hangar_ships(&raw), Err(Rejected::Version));
        let transfer = Value::Object(parse(r#"{"schema": "hangar-uebergabe", "version": 1}"#));
        assert_eq!(read_object(&transfer, &ShipNames::default()), Err(Rejected::Transfer));
    }

    #[test]
    fn insurance_months_and_zero() {
        let six = PROWLER.replace(r#""LTI""#, r#""6MI""#);
        let e = &from_hangar_ships(&ship_list(&six)).unwrap()[0];
        assert_eq!(get(e, "lti"), Some(&Value::Bool(false)));
        assert_eq!(get(e, "versicherung"), Some(&Value::int(6)));
        let zero = PROWLER.replace(r#""LTI""#, r#""0MI""#);
        let e = &from_hangar_ships(&ship_list(&zero)).unwrap()[0];
        assert_eq!(get(e, "versicherung"), Some(&Value::Null));
    }

    #[test]
    fn approximation_keeps_title_and_shared_matrix_keeps_name() {
        let approx = r#"{"matrix": "Freelancer MAX", "matrixZuordnung": "naeherung",
            "name": "Freelancer MAX Anniversary", "code": "MISC_A", "manufacturer": {"code": "MISC", "name": "M"}}"#;
        let e = &from_hangar_ships(&ship_list(approx)).unwrap()[0];
        assert_eq!(text(e, "name"), "Freelancer MAX");
        assert_eq!(text(e, "naeherung"), "Freelancer MAX Anniversary");
        let base = r#"{"matrix": "Freelancer MAX", "matrixZuordnung": "sicher", "name": "Freelancer MAX",
            "code": "MISC_B", "manufacturer": {"code": "MISC", "name": "M"}}"#;
        let both = from_hangar_ships(&ship_list(&format!("{approx}, {base}"))).unwrap();
        assert_eq!(text(&both[0], "name"), "Freelancer MAX Anniversary");
        assert!(get(&both[0], "naeherung").is_none());
    }

    #[test]
    fn included_ship_once_with_carrier_as_package() {
        let carrier = r#"{"matrix": "600i Explorer", "matrixZuordnung": "sicher", "name": "600i Explorer",
            "code": "ORIG_600i", "manufacturer": {"code": "ORIG", "name": "Origin Jumpworks"},
            "insurance": "LTI", "includedShips": [{"name": "G12", "code": "ORIG_G12"}, "G12"]}"#;
        let entries = from_hangar_ships(&ship_list(carrier)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(text(&entries[1], "name"), "G12");
        assert_eq!(text(&entries[1], "paket"), "600i Explorer");
        assert_eq!(get(&entries[1], "beilage"), Some(&Value::Bool(true)));
        let names = ShipNames(vec!["Origin G12".into()]);
        let raw = ship_list(carrier);
        let with = from_hangar_ships_with(&raw, &names).unwrap();
        assert_eq!(text(&with[1], "hersteller"), "Origin");
        assert_eq!(text(&with[1], "hkurz"), "ORIG");
    }

    #[test]
    fn import_twice_changes_nothing_and_removes_missing_pledges() {
        let mut data = empty();
        let first = from_hangar_ships(&ship_list(PROWLER)).unwrap();
        let done = import_entries(&mut data, &first);
        assert_eq!((done.new, done.existing), (1, 0));
        let before = file_text(&data);
        let again = import_entries(&mut data, &first);
        assert_eq!((again.new, again.existing), (0, 1));
        assert_eq!(file_text(&data), before);
        let cutlass = PROWLER.replace("Prowler Utility", "Cutlass Black").replace("ESPR", "DRAK");
        let next = from_hangar_ships(&ship_list(&cutlass)).unwrap();
        let swapped = import_entries(&mut data, &next);
        assert_eq!(swapped.removed, vec!["Prowler Utility".to_string()]);
        assert_eq!(ships(&data).len(), 1);
    }

    #[test]
    fn ingame_ships_wishlist_and_notes_stay() {
        let mut data = parse(
            r#"{"format": 1, "schiffe": [{"name": "Cutlass Red", "hersteller": "Drake", "herkunft": "ingame", "belegung": {}}],
              "wunsch": [{"name": "Polaris"}], "merkzettel": [{"name": "Helm", "anzahl": 2}], "eigenes": 7}"#,
        );
        let entries = from_hangar_ships(&ship_list(PROWLER)).unwrap();
        import_entries(&mut data, &entries);
        let out = file_text(&data);
        assert!(out.contains("Cutlass Red") && out.contains("Polaris") && out.contains("Helm"));
        assert!(out.contains("\"eigenes\": 7"));
        assert_eq!(ships(&data).len(), 2);
    }

    #[test]
    fn fill_keeps_existing_and_only_sets_lti() {
        let mut data = parse(
            r#"{"format": 1, "schiffe": [{"name": "Prowler Utility", "hersteller": "Esperia", "herkunft": "pledge",
              "belegung": {"a": 1}, "kurz": "ESPR_Prowler_Utility", "lti": false, "paket": "Alt"}]}"#,
        );
        let entries = from_hangar_ships(&ship_list(PROWLER)).unwrap();
        import_entries(&mut data, &entries);
        let s = ships(&data)[0];
        assert_eq!(get(s, "lti"), Some(&Value::Bool(true)));
        assert_eq!(text(s, "paket"), "Alt");
        assert_eq!(text(s, "gekauft"), "2024-03-02");
    }

    #[test]
    fn same_ship_ways() {
        let e = parse(r#"{"name": "Idris-P Frigate", "hkurz": "AEGS", "hersteller": "Aegis"}"#);
        assert!(same_ship(&e, "Idris-P", "", "", "AEGS"));
        let atls = parse(r#"{"name": "ATLS", "kurz": "ARGO_ATLS"}"#);
        assert!(!same_ship(&atls, "ATLS GEO", "", "ARGO_ATLS", ""));
        let razor = parse(r#"{"name": "Razor EX", "hkurz": "MISC"}"#);
        assert!(same_ship(&razor, "Razor EX", "", "", "MRAI"));
    }

    #[test]
    fn catalog_vehicle_follows_its_carrier() {
        let catalog = catalog_entries();
        let record = catalog
            .iter()
            .find(|r| has_evidence(r) && matches!(get(r, "fahrzeuge"), Some(Value::Array(v)) if !v.is_empty()))
            .expect("the bundled catalog holds a carrier with evidence and vehicles");
        let carrier = text(record, "schiff").to_string();
        let mut ships_ = vec![object(vec![
            ("name", Value::str(carrier.clone())),
            ("hersteller", Value::str("")),
            ("herkunft", Value::str(PLEDGE)),
        ])];
        apply(&mut ships_, catalog);
        assert!(ships_.len() > 1, "{carrier}");
        assert_eq!(text(&ships_[1], "herkunft"), CATALOG);
        ships_.remove(0);
        prune(&mut ships_, catalog);
        assert!(ships_.is_empty());
    }

    /// A wish with a plan, two equipments and a note of its own.
    const PLANNED: &str = r#"{"format": 1, "schiffe": [], "wunsch": [
        {"name": "Gladius", "hersteller": "Aegis Dynamics", "notiz": "für Rennen",
         "belegung": {"hardpoint_weapon_left": {"ref": "laser-3", "name": "Laser 3", "weg": "bauen", "erledigt": true}},
         "ausstattungen": [
            {"name": "PvP", "active": true, "slots": [{"portPath": "hardpoint_weapon_left", "itemGuid": "laser-3",
             "acquisition": "crafted", "blueprintGuid": "bp-laser-3", "quality": {"Frame": 800}}]},
            {"name": "Rennen", "active": false, "slots": [{"portPath": "hardpoint_cooler", "itemGuid": "cooler-2",
             "acquisition": "purchased"}]}]}]}"#;

    fn kept(entry: &Map) -> Vec<String> {
        ["belegung", crate::equipment::KEY, "notiz"].iter().map(|k| crate::jsonio::dumps(get(entry, k).unwrap_or(&Value::Null), None, false)).collect()
    }

    #[test]
    fn a_ship_moves_between_planned_and_the_hangar_both_ways_with_all_it_holds() {
        let stock = vec![("hardpoint_weapon_left".to_string(), "laser-1".to_string())];
        let mut data = parse(PLANNED);
        let before = kept(wishlist(&data)[0]);
        assert_eq!(wish_to_hangar(&mut data, " gladius ", &stock), Ok(Moved::New));
        assert!(wishlist(&data).is_empty(), "the wish is gone");
        let ship = ships(&data)[0].clone();
        assert_eq!(kept(&ship), before, "plan, equipments and note come along unchanged");
        assert_eq!(text(&ship, "herkunft"), INGAME, "bought in the game");
        assert_eq!(text(&ship, "hersteller"), "Aegis Dynamics");
        // A wipe: back onto the wish list, with all of it.
        map_set(ship_mut(&mut data, 0).expect("ship"), "einsatzbereit", Value::Bool(true));
        assert_eq!(hangar_to_wish(&mut data, 0, &stock), Ok(Moved::New));
        assert!(ships(&data).is_empty());
        let wish = wishlist(&data)[0].clone();
        assert_eq!(kept(&wish), before);
        assert!(truthy(get(&wish, "einsatzbereit")), "what the hangar knew waits for the way back");
        // And bought again: the origin it had stays.
        map_set(wish_mut(&mut data), "herkunft", Value::str(PLEDGE));
        assert_eq!(wish_to_hangar(&mut data, "Gladius", &stock), Ok(Moved::New));
        assert_eq!(text(ships(&data)[0], "herkunft"), PLEDGE);
        assert_eq!(kept(ships(&data)[0]), before);
    }

    fn wish_mut(data: &mut Map) -> &mut Map {
        match data.iter_mut().find(|(k, _)| k == "wunsch") {
            Some((_, Value::Array(items))) => match &mut items[0] {
                Value::Object(wish) => wish,
                _ => panic!("no wish"),
            },
            _ => panic!("no wish list"),
        }
    }

    #[test]
    fn a_move_onto_the_same_ship_adds_its_equipments_and_keeps_the_ones_there() {
        let stock: Vec<(String, String)> = Vec::new();
        // The hangar holds the Gladius with a plan of its own.
        let mut data = parse(PLANNED);
        let own = parse(
            r#"{"name": "Gladius", "hersteller": "Aegis", "herkunft": "pledge",
                "belegung": {"hardpoint_weapon_right": {"ref": "laser-9", "name": "Laser 9", "weg": "kaufen"}}}"#,
        );
        edit_ships(&mut data, |ships| ships.push(own));
        assert_eq!(wish_to_hangar(&mut data, "Gladius", &stock), Ok(Moved::Joined));
        assert_eq!(ships(&data).len(), 1, "no second entry");
        let ship = ships(&data)[0];
        let (names, active) = crate::equipment::names(ship, &stock);
        assert_eq!(names, ["Standard", "PvP", "Rennen"], "the own one first, the planned ones after it");
        assert_eq!(active, Some(0), "the own plan stays the active one");
        assert!(get(ship, "belegung").and_then(|b| b.get("hardpoint_weapon_right")).is_some());
        assert_eq!(text(ship, "notiz"), "für Rennen", "a key the ship lacked comes along");
        let pvp = crate::equipment::read_all(ship, &stock).0.into_iter().find(|e| e.name == "PvP").expect("PvP");
        assert_eq!(pvp.slot("hardpoint_weapon_left").map(|s| s.quality_at("Frame")), Some(800), "quality comes along");
        // An own entry without any plan takes the plan as it was planned.
        let mut data = parse(PLANNED);
        edit_ships(&mut data, |ships| ships.push(parse(r#"{"name": "Gladius", "hersteller": "Aegis", "belegung": {}}"#)));
        let before = kept(wishlist(&data)[0]);
        assert_eq!(wish_to_hangar(&mut data, "Gladius", &stock), Ok(Moved::Joined));
        assert_eq!(kept(ships(&data)[0]), before);
        // The other way: onto a wish of the same ship.
        let mut data = parse(PLANNED);
        edit_ships(&mut data, |ships| ships.push(parse(r#"{"name": "Gladius", "hersteller": "Aegis", "herkunft": "ingame",
            "belegung": {"hardpoint_weapon_right": {"ref": "laser-9", "name": "Laser 9", "weg": "kaufen"}}}"#)));
        assert_eq!(hangar_to_wish(&mut data, 0, &stock), Ok(Moved::Joined));
        assert_eq!(wishlist(&data).len(), 1);
        assert_eq!(crate::equipment::names(wishlist(&data)[0], &stock).0, ["PvP", "Rennen", "Standard"]);
    }

    #[test]
    fn a_move_that_cannot_be_done_changes_nothing() {
        let mut data = parse(TWO);
        let catalog_ship = parse(r#"{"name": "C8 Pisces", "hersteller": "Anvil", "herkunft": "katalog", "paket": "Carrack"}"#);
        edit_ships(&mut data, |ships| ships.push(catalog_ship));
        let before = data.clone();
        assert_eq!(hangar_to_wish(&mut data, 2, &[]), Err(MoveRefused::Included), "a vehicle of a package");
        assert_eq!(hangar_to_wish(&mut data, 9, &[]), Err(MoveRefused::Gone));
        assert_eq!(wish_to_hangar(&mut data, "Polaris", &[]), Err(MoveRefused::Gone));
        assert_eq!(wish_to_hangar(&mut data, "  ", &[]), Err(MoveRefused::Gone));
        assert!(py_eq_value(&Value::Object(data.clone()), &Value::Object(before)), "nothing changed");
        // Counter-check: the Gladius of the same file does move.
        assert_eq!(hangar_to_wish(&mut data, 1, &[]), Ok(Moved::New));
        assert!(wishlist_contains(&data, "Gladius"));
    }

    #[test]
    fn nice_and_split_maker() {
        assert_eq!(nice("aurora mr"), "Aurora MR");
        assert_eq!(nice("Aurora mr"), "Aurora mr");
        assert_eq!(split_maker("Aegis Idris-P"), ("Aegis".into(), "AEGS".into(), "Idris-P".into()));
        assert_eq!(usd(Some(&Value::int(475))), "$475.00 USD");
        assert_eq!(usd(Some(&Value::Bool(true))), "");
    }
}
