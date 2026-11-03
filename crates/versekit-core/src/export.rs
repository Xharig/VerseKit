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

//! The output files next to the collection, rewritten after every save: one
//! for the import of the KRT Profit Basetool (`profit-base.online`), one for
//! the import of scmdb.net, a full backup and the contract log. They lie in
//! the folder `export` of the data folder (or the setting `export_ordner`),
//! always under the same names; dated files of older versions move to
//! `Ältere/`.
//!
//! ```json
//! {"blueprints": [{"productName": "Manticore Helmet", "tag": "BP_CRAFT_…",
//!                  "receivedAt": "2026-08-02T01:49:03Z"}]}
//! ```
//!
//! ⚠ A failure while writing one file leaves the other files alone and is not
//! reported. A data error (an entry that is not an object, a name that is not
//! text) stops the whole run; the collection reports it. Missing inputs
//! (catalog, recipe data, contract log) give smaller files or none, never an
//! error. A collection without blueprints writes no blueprint files (those
//! of the last save stay); the contract log is still written.
//!
//! ⚠ Stored strings stay German: the file names, the keys of the full backup
//! (`werkzeug`, `erstellt`, `spielversion`, `anzahl`, `bauplaene`, `name`,
//! `quelle`, `zeit`, `art`, `klasse`, `hersteller`) and of the contract log
//! (`format`, `auftraege`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::catalog_cache;
use crate::collection::{Exports, py_str, truthy};
use crate::jsonio::{self, Map, ReadError, Value, map_set, py_eq};
use crate::language::{self, Lang};
use crate::paths::{Paths, name_key};
use crate::pypath::Flavor;
use crate::text::{py_isspace, py_strip};
use crate::{blueprints, errors, legacy};

/// The value of `werkzeug` and `productName` by which the import recognizes
/// its own files.
pub const TOOL_NAME: &str = "VerseKit";
/// The page of a blueprint at scmdb, the tag appended.
pub const SCMDB_URL: &str = "https://scmdb.net/?page=fab&fab=";
/// The folder in the data folder, unless `export_ordner` names another.
pub const FOLDER: &str = "export";
pub const FOLDER_SETTING: &str = "export_ordner";
/// Where dated files of older versions go.
pub const OLD_FOLDER: &str = "Ältere";
/// Blueprints without a known way count only with this setting.
pub const ALL_SETTING: &str = "alle_bauplaene";
/// The recipe data of scmdb, with the tag of every blueprint.
pub const CRAFTING_FILE: &str = crate::recipe_file::FILE;
const CRAFTING_FORMAT: i64 = crate::recipe_file::FORMAT;
/// Known errors in the game data, per build.
pub const ISSUES_FILE: &str = "cig-datenfehler.json";
const ISSUES_FORMAT: i64 = 1;
/// The contract log.
pub const CONTRACT_LOG_FILE: &str = "auftragslog.json";
const CONTRACT_LOG_FORMAT: i64 = 2;
/// State of a contract that has not ended yet.
const RUNNING: &str = "laeuft";

/// The four output files, in the order they are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Basetool,
    Scmdb,
    Full,
    Contracts,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Basetool, Kind::Scmdb, Kind::Full, Kind::Contracts];

    /// The file name without `.json`; older versions put `-<date>` after it.
    pub fn stem(self) -> &'static str {
        match self {
            Kind::Basetool => "SC-Blueprints-Basetool",
            Kind::Scmdb => "scmdb-import",
            Kind::Full => legacy::EXPORT_FULL,
            Kind::Contracts => legacy::EXPORT_CONTRACTS,
        }
    }

    pub fn file_name(self) -> String {
        format!("{}.json", self.stem())
    }
}

// ------------------------------------------------------------ Values as in Python

fn get<'m>(map: &'m Map, key: &str) -> Option<&'m Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The value if it is true in Python's sense (`value or …`).
fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| truthy(v))
}

/// An object, where Python calls `.get` on it.
fn object(value: &Value) -> Result<&Map, String> {
    value.as_object().ok_or_else(|| "Eintrag ist kein Objekt".to_string())
}

/// `for x in value or []`: false is empty, a list gives its items; anything
/// else makes Python raise.
fn items(value: Option<&Value>) -> Result<&[Value], String> {
    match present(value) {
        None => Ok(&[]),
        Some(Value::Array(list)) => Ok(list),
        Some(_) => Err("keine Liste".into()),
    }
}

/// `(value or '')` where Python goes on with a text method.
fn text_or_empty(value: Option<&Value>) -> Result<&str, String> {
    match present(value) {
        None => Ok(""),
        Some(v) => v.as_str().ok_or_else(|| "kein Text".to_string()),
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

/// The blueprints of the collection sorted by key (`sorted(….items())`).
fn sorted_blueprints(data: &Map) -> Result<Vec<&(String, Value)>, String> {
    let mut list: Vec<&(String, Value)> = match present(get(data, "bauplaene")) {
        None => Vec::new(),
        Some(Value::Object(map)) => map.iter().collect(),
        Some(_) => return Err("bauplaene ist kein Objekt".into()),
    };
    list.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(list)
}

// ------------------------------------------------------------ Time

/// `time.strptime(…, '%Y-%m-%d %H:%M:%S')` of Python 3.14: single digits are
/// allowed, a day and an hour may start with a space, any whitespace stands
/// between date and time, every Unicode decimal digit counts.
static LOCAL_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?P<Y>\d\d\d\d)-(?P<m>1[0-2]|0[1-9]|[1-9])-(?P<d>3[0-1]|[1-2]\d|0[1-9]|[1-9]| [1-9])",
        r"[\s\x1c-\x1f]+(?P<H>2[0-3]|[0-1]\d|\d| \d):(?P<M>[0-5]\d|\d):(?P<S>6[0-1]|[0-5]\d|\d)",
    ))
    .expect("fester Ausdruck")
});

static DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d$").expect("fester Ausdruck"));

fn is_digit(c: char) -> bool {
    let mut buffer = [0; 4];
    DIGIT.is_match(c.encode_utf8(&mut buffer))
}

/// Value of a decimal digit: Unicode keeps the digits of a script in runs
/// that start with 0.
fn digit_value(c: char) -> u32 {
    let mut start = c as u32;
    while let Some(before) = start.checked_sub(1).and_then(char::from_u32)
        && is_digit(before)
    {
        start -= 1;
    }
    (c as u32 - start) % 10
}

/// Like `int()` on a group of the time: spaces are skipped.
fn group_value(text: &str) -> i32 {
    text.chars().filter(|c| *c != ' ').fold(0, |n, c| n * 10 + digit_value(c) as i32)
}

fn days_in_month(year: i32, month: i32) -> i32 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Year, month, day, hour, minute, second of a local time from the
/// collection, `None` where Python's `strptime` raises (no match, text left
/// over, a day the month does not have, year 0).
pub fn parse_local(text: &str) -> Option<[i32; 6]> {
    let caps = LOCAL_TIME.captures(text)?;
    if caps.get(0)?.end() != text.len() {
        return None;
    }
    let value = |name: &str| caps.name(name).map_or(0, |m| group_value(m.as_str()));
    let t = [value("Y"), value("m"), value("d"), value("H"), value("M"), value("S")];
    (t[0] >= 1 && t[2] <= days_in_month(t[0], t[1])).then_some(t)
}

/// Local time to UTC seconds through the C library (`time.mktime`, summer
/// time decided by the library); `None` where Python raises `OverflowError`.
#[cfg(windows)]
fn mktime(t: [i32; 6]) -> Option<i64> {
    use std::ffi::c_int;
    #[repr(C)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
    }
    unsafe extern "C" {
        fn _mktime64(tm: *mut Tm) -> i64;
    }
    let mut tm = Tm {
        sec: t[5],
        min: t[4],
        hour: t[3],
        mday: t[2],
        mon: t[1] - 1,
        year: t[0] - 1900,
        wday: -1,
        yday: 0,
        isdst: -1,
    };
    // SAFETY: `_mktime64` reads and normalizes only the structure passed in.
    let stamp = unsafe { _mktime64(&mut tm) };
    (stamp != -1 || tm.wday != -1).then_some(stamp)
}

#[cfg(unix)]
fn mktime(t: [i32; 6]) -> Option<i64> {
    // SAFETY: `mktime` reads and normalizes only the structure passed in.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_sec = t[5];
        tm.tm_min = t[4];
        tm.tm_hour = t[3];
        tm.tm_mday = t[2];
        tm.tm_mon = t[1] - 1;
        tm.tm_year = t[0] - 1900;
        tm.tm_wday = -1;
        tm.tm_isdst = -1;
        let stamp = libc::mktime(&mut tm);
        (stamp != -1 || tm.tm_wday != -1).then_some(stamp as i64)
    }
}

/// The first 19 characters `YYYY-MM-DD HH:MM:SS` of a local time as seconds
/// (`time.mktime(time.strptime(text[:19], …))`); `None` when they are not one.
pub fn local_stamp(text: &str) -> Option<i64> {
    mktime(parse_local(text.get(..19)?)?)
}

/// UTC seconds as `YYYY-MM-DDTHH:MM:SSZ` (with `.000` before the `Z` if asked).
pub fn utc_text(stamp: i64, millis: bool) -> String {
    let (days, secs) = (stamp.div_euclid(86_400), stamp.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let fraction = if millis { ".000" } else { "" };
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// A time of the collection as UTC for the Basetool, `None` if it cannot be
/// read (the field is then left out rather than made up).
///
/// ⚠⚠ The collection keeps LOCAL time, `Z` means UTC: it is converted, not
/// just suffixed (two hours off in summer otherwise).
pub fn iso_time(value: Option<&Value>) -> Option<String> {
    let t = parse_local(&py_str(present(value)?))?;
    Some(utc_text(mktime(t)?, false))
}

/// Now in UTC seconds.
pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

// ------------------------------------------------------------ Tags from the recipe data

/// Blueprint name (comparison form) → tag, only where the name has exactly
/// one tag. For Basetool and scmdb.
///
/// ⚠⚠ A name with several tags gets none: the Basetool checks the tag before
/// the name and jumps straight to that item, so a guessed tag would land on
/// the wrong part. Without a tag it matches by name.
///
/// ⚠ Matched by `name_key`, so `7MA "Lorica"` meets `7MA 'Lorica'` and
/// `(16 Schuss)` meets `(16 cap)`. Blueprints with the same name and the same
/// recipe are one item; its first tag counts. Name corrections from the list
/// of known data errors apply. Anything in the recipe data that would make
/// Python raise leaves the table empty.
pub fn unique_tags(paths: &Paths) -> HashMap<String, String> {
    let stamp = |name: &str| {
        let path = paths.app_file(name);
        let meta = fs::metadata(&path).ok().map(|m| (m.modified().ok(), m.len()));
        (path, meta)
    };
    let current = (stamp(CRAFTING_FILE), stamp(ISSUES_FILE));
    let mut memo = TAGS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((seen, table)) = memo.as_ref()
        && *seen == current
        && current.0.1.is_some()
    {
        return table.clone();
    }
    let table = recipe_tags(paths).unwrap_or_default();
    *memo = Some((current, table.clone()));
    table
}

type FileStamp = (String, Option<(Option<SystemTime>, u64)>);
type TagMemo = ((FileStamp, FileStamp), HashMap<String, String>);

/// The last table with the state of the recipe data and the error list it came from.
static TAGS: Mutex<Option<TagMemo>> = Mutex::new(None);

fn recipe_tags(paths: &Paths) -> Result<HashMap<String, String>, String> {
    let list = recipe_blueprints(paths)?;
    // name key → [(recipe, first blueprint with it)]
    let mut groups: HashMap<String, Vec<(Vec<String>, &Map)>> = HashMap::new();
    for b in &list {
        let map = object(b)?;
        let name = match present(get(map, "productName")) {
            Some(v) => py_str(v),
            None => name_from_tag(text_or_empty(get(map, "tag"))?),
        };
        let recipe = recipe_key(map)?;
        let parts = groups.entry(name_key(&name)).or_default();
        if !parts.iter().any(|(r, _)| *r == recipe) {
            parts.push((recipe, map));
        }
    }
    let mut seen: HashMap<String, HashSet<String>> = HashMap::new();
    for parts in groups.values() {
        for (_, first) in parts {
            let name = match present(get(first, "productName")) {
                Some(Value::Str(s)) => s.clone(),
                Some(_) => return Err("Produktname ist kein Text".into()),
                None => name_from_tag(text_or_empty(get(first, "tag"))?),
            };
            let tag = py_strip(text_or_empty(get(first, "tag"))?);
            let base = name_key(&name);
            if !base.is_empty() && !tag.is_empty() {
                seen.entry(base).or_default().insert(tag.to_string());
            }
        }
    }
    Ok(seen
        .into_iter()
        .filter(|(_, tags)| tags.len() == 1)
        .filter_map(|(name, tags)| Some((name, tags.into_iter().next()?)))
        .collect())
}

/// A readable name from the tag where `productName` is missing:
/// `BP_CRAFT_RADR_GNRP_S03_Idris_TEMP` → `RADR GNRP S03 Idris`.
fn name_from_tag(tag: &str) -> String {
    let raw = tag.replace("BP_CRAFT_", "").replace("_SCItem", "").replace("_TEMP", "").replace('_', " ");
    let raw = py_strip(&raw);
    if raw.is_empty() { "?".into() } else { raw.to_string() }
}

/// What makes two blueprints of the same name one item: their ingredients
/// (`slot|material|amount`, sorted).
fn recipe_key(blueprint: &Map) -> Result<Vec<String>, String> {
    let mut keys = Vec::new();
    for tier in items(get(blueprint, "tiers"))? {
        let tier = object(tier)?;
        for slot in items(get(tier, "slots"))? {
            let slot = object(slot)?;
            for option in items(get(slot, "options"))? {
                if !truthy(option) {
                    continue;
                }
                let option = object(option)?;
                let Some(material) = present(get(option, "resourceName")).or(present(get(option, "itemName"))) else {
                    continue;
                };
                let slot_name = present(get(slot, "name")).map_or_else(String::new, py_str);
                let amount = present(get(option, "quantity")).map_or_else(|| "0".into(), py_str);
                keys.push(format!("{slot_name}|{}|{amount}", py_str(material)));
            }
        }
    }
    keys.sort();
    Ok(keys)
}

/// The recipe entries as `crafting.load()` hands them on: only the known
/// format, product names corrected by the list of data errors for this build.
fn recipe_blueprints(paths: &Paths) -> Result<Vec<Value>, String> {
    let data = jsonio::read_json(&paths.app_file(CRAFTING_FILE)).map_err(|e| e.to_string())?;
    let map = object(&data)?;
    if !py_eq(get(map, "format"), Some(&Value::int(CRAFTING_FORMAT))) {
        return Err("anderes Format".into());
    }
    let mut list = items(get(map, "blueprints"))?.to_vec();
    let issues = issues_for(paths, get(map, "build"));
    if !issues.is_empty() {
        let products = present(get(map, "products"));
        fix_names(&mut list, &issues, products)?;
        if products.is_some_and(|p| !matches!(p, Value::Object(_))) {
            return Err("products ist kein Objekt".into());
        }
    }
    Ok(list)
}

/// The data errors for exactly this build (`cig_issues.issues_for`), empty
/// if there is no list or it belongs to another build.
fn issues_for(paths: &Paths, build: Option<&Value>) -> Map {
    if present(build).is_none() {
        return Map::new();
    }
    let raw = match jsonio::read_json(&paths.app_file(ISSUES_FILE)) {
        Ok(Value::Object(map)) => map,
        Err(ReadError::TooDeep(message)) => {
            errors::record("cig_issues.load", &message);
            return Map::new();
        }
        _ => return Map::new(),
    };
    let Some(Value::Object(issues)) = get(&raw, "daten") else { return Map::new() };
    if !py_eq(get(&raw, "format"), Some(&Value::int(ISSUES_FORMAT))) || !py_eq(get(&raw, "build"), build) {
        return Map::new();
    }
    issues.clone()
}

/// The entries of a section of the error list, keys in lower case, without
/// comments (`_…`).
fn issue_entries<'a>(issues: &'a Map, section: &str) -> HashMap<String, &'a Map> {
    let Some(Value::Object(block)) = get(issues, section) else { return HashMap::new() };
    block
        .iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .filter_map(|(k, v)| Some((k.to_lowercase(), v.as_object()?)))
        .collect()
}

fn guid(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => py_strip(s).to_lowercase(),
        _ => String::new(),
    }
}

/// The product names of `cig_issues.apply_blueprints`; the other corrections
/// there (modifiers, manufacturer) do not touch the tags, only what in them
/// would make Python raise is checked.
fn fix_names(list: &mut [Value], issues: &Map, products: Option<&Value>) -> Result<(), String> {
    let records = issue_entries(issues, "blueprintRecords");
    if records.is_empty() {
        return Ok(());
    }
    for b in list {
        let Value::Object(map) = b else { return Err("Rezept ist kein Objekt".into()) };
        let Some(entry) = records.get(&guid(get(map, "guid"))).filter(|e| !e.is_empty()) else { continue };
        let Some(Value::Object(expected)) = get(entry, "expected") else { continue };
        if present(get(entry, "slot")).is_some() && present(get(entry, "propertyKey")).is_some() {
            check_modifier_slots(map, get(entry, "slot"))?;
            continue;
        }
        if let Some(entity) = present(get(expected, "entityClass"))
            && guid(get(map, "productEntityClass")) != guid(Some(entity))
        {
            if matches!(entity, Value::Array(_) | Value::Object(_)) {
                return Err("entityClass ist nicht hashbar".into());
            }
            match products {
                None => {}
                Some(Value::Object(table)) => {
                    let product = entity.as_str().and_then(|key| get(table, key));
                    if present(product).is_some_and(|p| !matches!(p, Value::Object(_))) {
                        return Err("Produkt ist kein Objekt".into());
                    }
                }
                Some(_) => return Err("products ist kein Objekt".into()),
            }
        }
        if let Some(name) = present(get(expected, "name"))
            && !py_eq(get(map, "productName"), Some(name))
        {
            let name = name.clone();
            map_set(map, "productName", name);
        }
    }
    Ok(())
}

/// The walk of `_apply_modifier` through tiers, slots and modifiers.
fn check_modifier_slots(blueprint: &Map, slot_name: Option<&Value>) -> Result<(), String> {
    for tier in items(get(blueprint, "tiers"))? {
        for slot in items(get(object(tier)?, "slots"))? {
            let slot = object(slot)?;
            if !py_eq(get(slot, "name"), slot_name) {
                continue;
            }
            for modifier in items(get(slot, "modifiers"))? {
                object(modifier)?;
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------ Catalog

/// The catalog as the full backup reads it: the game version and the
/// blueprints by comparison key.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogView {
    pub version: Value,
    pub blueprints: HashMap<String, Map>,
}

/// Like `catalog.load()`: keys rebuilt from the names where one does not fit,
/// blueprints without a known way only with `alle_bauplaene`. Missing or
/// unusable: empty.
pub fn catalog_view(paths: &Paths) -> CatalogView {
    read_catalog(paths).unwrap_or_else(|| CatalogView { version: Value::Null, blueprints: HashMap::new() })
}

fn read_catalog(paths: &Paths) -> Option<CatalogView> {
    let data = jsonio::read_json(&catalog_cache::current_file(paths)).ok()?;
    let Some(Value::Object(list)) = data.get("bauplaene") else { return None };
    let entries: Vec<(&String, &Map)> = list.iter().map(|(k, e)| Some((k, e.as_object()?))).collect::<Option<_>>()?;
    let renamed = |e: &Map| present(get(e, "n")).map(|n| name_key(&py_str(n)));
    let rebuild = entries.iter().any(|(k, e)| renamed(e).is_some_and(|n| n != **k));
    let mut blueprints = HashMap::new();
    for (key, entry) in entries {
        let key = if rebuild { renamed(entry).unwrap_or_else(|| key.clone()) } else { key.clone() };
        blueprints.insert(key, entry.clone());
    }
    if !paths.setting_bool(ALL_SETTING, false) {
        blueprints.retain(|_, e| present(get(e, "ohne_weg")).is_none());
    }
    Some(CatalogView { version: data.get("version").cloned().unwrap_or(Value::Null), blueprints })
}

/// The kind of a catalog entry as the full backup names it.
fn kind_value(raw: &Value) -> Result<Value, String> {
    match raw {
        Value::Str(s) => Ok(Value::Str(blueprints::kind_readable(s))),
        Value::Array(_) | Value::Object(_) => Err("Art ist nicht hashbar".into()),
        other => {
            let key = format!("art_{}", py_str(other));
            Ok(if language::text_in(Lang::De, &key).is_some() { Value::Str(language::t(&key)) } else { other.clone() })
        }
    }
}

// ------------------------------------------------------------ The documents

/// What the import of `profit-base.online` expects: `productName` (required),
/// `tag` where it is unique, `receivedAt` where the time can be read.
/// Entries without a name are left out.
pub fn for_basetool(data: &Map, tags: &HashMap<String, String>) -> Result<Value, String> {
    let mut entries = Vec::new();
    for (_, e) in sorted_blueprints(data)? {
        let e = object(e)?;
        let name = py_strip(text_or_empty(get(e, "name"))?);
        if name.is_empty() {
            continue;
        }
        let mut entry = vec![("productName".to_string(), Value::str(name))];
        if let Some(tag) = tags.get(&name_key(name)).filter(|t| !t.is_empty()) {
            entry.push(("tag".into(), Value::str(tag)));
        }
        if let Some(time) = iso_time(get(e, "zeit")) {
            entry.push(("receivedAt".into(), Value::Str(time)));
        }
        entries.push(Value::Object(entry));
    }
    Ok(Value::Object(vec![("blueprints".into(), Value::Array(entries))]))
}

/// What the import of scmdb.net reads (`Import Watcher History`).
///
/// ⛔ It rejects any file without `exportSchemaVersion` or the list
/// `missions` (which stays empty). Per blueprint the tag counts, otherwise
/// `productName`; `tag`, `name` and `url` of the older format stay for the own
/// import. No `channel`: any value except LIVE makes scmdb warn.
pub fn for_scmdb(data: &Map, tags: &HashMap<String, String>, version: &str, now: i64) -> Result<Value, String> {
    let mut entries = Vec::new();
    for (_, e) in sorted_blueprints(data)? {
        let e = object(e)?;
        let name = py_strip(text_or_empty(get(e, "name"))?);
        if name.is_empty() {
            continue;
        }
        let tag = tags.get(&name_key(name)).cloned().unwrap_or_default();
        let mut entry = vec![
            ("tag".to_string(), Value::str(&tag)),
            ("productName".into(), Value::str(name)),
            ("name".into(), Value::str(name)),
        ];
        if !tag.is_empty() {
            entry.push(("url".into(), Value::Str(format!("{SCMDB_URL}{tag}"))));
        }
        entries.push(Value::Object(entry));
    }
    Ok(Value::Object(vec![
        ("exportSchemaVersion".into(), Value::int(1)),
        ("productName".into(), Value::str(TOOL_NAME)),
        ("watcherVersion".into(), Value::str(version)),
        ("exportedAt".into(), Value::Str(utc_text(now, true))),
        ("missions".into(), Value::Array(Vec::new())),
        ("blueprints".into(), Value::Array(entries)),
    ]))
}

/// Everything known about the own blueprints: name, source, time, and from
/// the catalog kind, class, size, grade, manufacturer. Fields that are `null`
/// or empty text are left out; `0` and `false` stay.
pub fn complete(data: &Map, catalog: &CatalogView, now: i64) -> Result<Value, String> {
    let mut entries = Vec::new();
    for (key, e) in sorted_blueprints(data)? {
        let e = object(e)?;
        let known = catalog.blueprints.get(key);
        let from_catalog = |name: &str| known.and_then(|k| get(k, name)).cloned();
        let kind = match known.and_then(|k| present(get(k, "a"))) {
            Some(raw) => Some(kind_value(raw)?),
            None => None,
        };
        let fields = [
            ("name", get(e, "name").cloned()),
            ("quelle", get(e, "quelle").cloned()),
            ("zeit", get(e, "zeit").cloned()),
            ("art", kind),
            ("klasse", from_catalog("c")),
            ("size", from_catalog("s")),
            ("grade", from_catalog("g")),
            ("hersteller", from_catalog("m")),
        ];
        let entry: Map = fields
            .into_iter()
            .filter_map(|(k, v)| {
                v.filter(|v| !matches!(v, Value::Null) && !matches!(v, Value::Str(s) if s.is_empty()))
                    .map(|v| (k.to_string(), v))
            })
            .collect();
        entries.push(Value::Object(entry));
    }
    let version = present(Some(&catalog.version)).cloned().unwrap_or(Value::Null);
    Ok(Value::Object(vec![
        ("werkzeug".into(), Value::str(TOOL_NAME)),
        ("erstellt".into(), Value::Str(utc_text(now, false))),
        ("spielversion".into(), version),
        ("anzahl".into(), Value::Int(entries.len().to_string())),
        ("bauplaene".into(), Value::Array(entries)),
    ]))
}

// ------------------------------------------------------------ Contract log

/// Blueprint marks in contract titles: the block of the previous version and
/// the bubble that other tools set as well
/// (`<EM4>[BP]?</EM4>`, `<EM4>[150 Rep] [BP]*</EM4>`).
static TITLE_MARKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"{}|<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>", legacy::TITLE_BLOCK))
        .expect("fester Ausdruck")
});

/// A title without marks and without double spaces (`contracts.clean`).
pub fn clean_title(title: &str) -> String {
    let plain = TITLE_MARKS.replace_all(title, " ");
    plain.split(py_isspace).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The contract log as `mission_log.load()` returns it: the stored runs,
/// titles cleaned of marks (runs that then share name and start are merged).
/// A broken file is reported and its backup used; missing, empty or another
/// format gives an empty list. `Err` where Python raises.
pub fn contract_log(paths: &Paths) -> Result<Vec<Value>, String> {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    let Some(Value::Object(data)) =
        jsonio::load_json(&paths.app_file(CONTRACT_LOG_FILE), Some(&valid), "mission_log.load")
    else {
        return Ok(Vec::new());
    };
    if data.is_empty() || !py_eq(get(&data, "format"), Some(&Value::int(CONTRACT_LOG_FORMAT))) {
        return Ok(Vec::new());
    }
    let entries = items(get(&data, "auftraege"))?;
    let mut cleaned = Vec::with_capacity(entries.len());
    let mut changed = false;
    for e in entries {
        let map = object(e)?;
        let name = present(get(map, "name"));
        let plain = name.map_or_else(String::new, |n| clean_title(&py_str(n)));
        if !plain.is_empty() && name.and_then(Value::as_str) != Some(plain.as_str()) {
            let mut copy = map.clone();
            map_set(&mut copy, "name", Value::Str(plain));
            cleaned.push(copy);
            changed = true;
        } else {
            cleaned.push(map.clone());
        }
    }
    if !changed {
        return Ok(entries.to_vec());
    }
    merge_runs(cleaned)
}

/// The dictionary key of a field of a run (`e.get(…) or ''`).
fn run_key(value: Option<&Value>, nan: &mut usize) -> Result<String, String> {
    Ok(match present(value) {
        None => "s:".into(),
        Some(Value::Str(s)) => format!("s:{s}"),
        Some(Value::Bool(b)) => format!("n:{}", u8::from(*b)),
        Some(Value::Int(text)) => format!("n:{text}"),
        Some(Value::Float(x)) if x.is_nan() => {
            *nan += 1;
            format!("nan:{nan}")
        }
        Some(Value::Float(x)) if x.fract() == 0.0 && x.abs() < 1e18 => format!("n:{}", *x as i64),
        Some(Value::Float(x)) => format!("f:{}", jsonio::float_text(*x)),
        Some(_) => return Err("Schlüssel nicht hashbar".into()),
    })
}

/// `mission_log.merge(entries, [])`: one run per name and start; a run that
/// ended beats „laeuft“, a running one only fills gaps. Newest start first.
fn merge_runs(entries: Vec<Map>) -> Result<Vec<Value>, String> {
    let running = |m: &Map| get(m, "zustand").and_then(Value::as_str) == Some(RUNNING);
    let mut merged: Vec<Map> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut nan = 0;
    for e in entries {
        let key = (run_key(get(&e, "name"), &mut nan)?, run_key(get(&e, "wann"), &mut nan)?);
        let Some(&at) = index.get(&key) else {
            index.insert(key, merged.len());
            merged.push(e);
            continue;
        };
        let before = &mut merged[at];
        if running(&e) {
            for (k, v) in &e {
                if k != "zustand" && present(get(before, k)).is_none() && truthy(v) {
                    map_set(before, k, v.clone());
                }
            }
        } else {
            for (k, v) in &e {
                if !matches!(v, Value::Null) && !matches!(v, Value::Str(s) if s.is_empty()) {
                    map_set(before, k, v.clone());
                }
            }
        }
    }
    let keys: Vec<Value> =
        merged.iter().map(|m| present(get(m, "wann")).cloned().unwrap_or_else(|| Value::str(""))).collect();
    let texts = keys.iter().all(|k| matches!(k, Value::Str(_)));
    let numbers = keys.iter().all(|k| number(k).is_some());
    if keys.len() > 1 && !texts && !numbers {
        return Err("Zeitpunkte nicht vergleichbar".into());
    }
    let mut order: Vec<usize> = (0..merged.len()).collect();
    order.sort_by(|&a, &b| match (&keys[b], &keys[a]) {
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (x, y) => number(x).partial_cmp(&number(y)).unwrap_or(std::cmp::Ordering::Equal),
    });
    let mut slots: Vec<Option<Map>> = merged.into_iter().map(Some).collect();
    Ok(order.into_iter().filter_map(|i| slots[i].take().map(Value::Object)).collect())
}

/// The contract log as text for the backup: indent 2, newline at the end.
pub fn contract_log_json(entries: &[Value]) -> String {
    let doc = Value::Object(vec![
        ("format".into(), Value::int(CONTRACT_LOG_FORMAT)),
        ("auftraege".into(), Value::Array(entries.to_vec())),
    ]);
    jsonio::dumps(&doc, Some(2), false) + "\n"
}

// ------------------------------------------------------------ Writing

/// Write through `<file>.tmp` and rename; line ends stay `\n`.
fn write_file(target: &str, text: &str) -> io::Result<()> {
    let flavor = Flavor::native();
    let cwd = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    jsonio::make_dirs(&flavor.dirname(&flavor.abspath(target, &cwd)))?;
    let temp = format!("{target}.tmp");
    jsonio::write_new(&temp, text.as_bytes())?;
    jsonio::os_replace(&temp, target)
}

/// Move dated files of older versions into `Ältere/`.
///
/// ⚠ Only names of the four patterns with something between the stem and
/// `.json`; everything else in the folder stays where it is. If the same name
/// already lies in `Ältere/`, the file in the folder is removed. A failure
/// stops the moving and is reported.
pub fn tidy_old_files(folder: &str) {
    let Ok(listing) = fs::read_dir(folder) else { return };
    let flavor = Flavor::native();
    let mut moved = Vec::new();
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !fs::metadata(flavor.join(folder, &name)).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let dated = |kind: &Kind| {
            let stem = kind.stem();
            name.starts_with(stem) && name.ends_with(".json") && name.len() > stem.len() + ".json".len()
        };
        if Kind::ALL.iter().any(dated) {
            moved.push(name);
        }
    }
    if moved.is_empty() {
        return;
    }
    let old = flavor.join(folder, OLD_FOLDER);
    let result = (|| {
        jsonio::make_dirs(&old)?;
        for name in &moved {
            let (source, target) = (flavor.join(folder, name), flavor.join(&old, name));
            if Path::new(&target).exists() {
                jsonio::remove_file(&source)?;
            } else {
                jsonio::os_replace(&source, &target)?;
            }
        }
        io::Result::Ok(())
    })();
    if let Err(e) = result {
        errors::record("export._tidy_old_files", &e);
    }
}

/// What one run wrote: the folder and the names of the files written.
#[derive(Clone, Debug, PartialEq)]
pub struct Archived {
    pub folder: String,
    pub written: Vec<String>,
}

/// The output files of the data folder, as `Collection::save` writes them.
pub struct FileExports<'a> {
    pub paths: &'a Paths,
    /// The running version; scmdb shows it as „Watcher v…“.
    pub version: &'a str,
    /// UTC seconds for the time stamps in the files (`now_unix`).
    pub clock: fn() -> i64,
}

impl FileExports<'_> {
    /// The folder of the files, created if missing (a failure is ignored).
    pub fn folder(&self) -> String {
        let folder = match self.paths.setting(FOLDER_SETTING) {
            Some(own) => own,
            None => self.paths.host.flavor.join(&self.paths.app_folder(), FOLDER),
        };
        let _ = jsonio::make_dirs(&folder);
        folder
    }

    /// Write all four files. A file that cannot be written is left out of
    /// the list; `Err` for a data error, which stops the run.
    pub fn archive_files(&self, data: &Map) -> Result<Archived, String> {
        let folder = self.folder();
        tidy_old_files(&folder);
        let mut written = Vec::new();
        let mut tags = None;
        for kind in Kind::ALL {
            let name = kind.file_name();
            let Some(text) = self.text_of(kind, data, &mut tags)? else { continue };
            if write_file(&self.paths.host.flavor.join(&folder, &name), &text).is_ok() {
                written.push(name);
            }
        }
        Ok(Archived { folder, written })
    }

    /// The text of one file as [`Self::archive_files`] writes it, `None` if there is nothing to write.
    pub fn text(&self, kind: Kind, data: &Map) -> Result<Option<String>, String> {
        self.text_of(kind, data, &mut None)
    }

    /// The text of one file, `None` if there is nothing to write.
    fn text_of(
        &self,
        kind: Kind,
        data: &Map,
        tags: &mut Option<HashMap<String, String>>,
    ) -> Result<Option<String>, String> {
        let doc = match kind {
            Kind::Contracts => {
                let entries = contract_log(self.paths)?;
                return Ok((!entries.is_empty()).then(|| contract_log_json(&entries)));
            }
            Kind::Basetool => for_basetool(data, tags.get_or_insert_with(|| unique_tags(self.paths)))?,
            Kind::Scmdb => {
                let table = tags.get_or_insert_with(|| unique_tags(self.paths));
                for_scmdb(data, table, self.version, (self.clock)())?
            }
            Kind::Full => complete(data, &catalog_view(self.paths), (self.clock)())?,
        };
        let count = match doc.get("blueprints").filter(|v| truthy(v)).or(doc.get("bauplaene")) {
            Some(Value::Array(list)) => list.len(),
            _ => 0,
        };
        Ok((count > 0).then(|| jsonio::dumps(&doc, Some(1), false)))
    }
}

impl Exports for FileExports<'_> {
    fn archive(&self, data: &Map) -> Result<(), String> {
        self.archive_files(data).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;

    fn temp_home(tag: &str) -> (std::path::PathBuf, Paths) {
        let dir = std::env::temp_dir().join(format!("versekit-export-{tag}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &text)], &text);
        (dir, Paths::new(host))
    }

    fn collection(entries: &str) -> Map {
        match jsonio::loads(&format!(r#"{{"version": 3, "stand": "x", "bauplaene": {{{entries}}}}}"#)).unwrap() {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    #[test]
    fn local_time_is_read_like_strptime() {
        assert_eq!(parse_local("2026-08-24 07:57:59"), Some([2026, 8, 24, 7, 57, 59]));
        assert_eq!(parse_local("2026-8-4 7:5:9"), Some([2026, 8, 4, 7, 5, 9]));
        assert_eq!(parse_local("2026-08- 4  7:05:61"), Some([2026, 8, 4, 7, 5, 61]));
        assert_eq!(parse_local("2026-08-24\u{1c}\t07:57:59"), Some([2026, 8, 24, 7, 57, 59]));
        assert_eq!(parse_local("２０２６-08-24 07:57:59"), Some([2026, 8, 24, 7, 57, 59]));
        assert_eq!(parse_local("2024-02-29 10:00:00"), Some([2024, 2, 29, 10, 0, 0]));
        for wrong in [
            "2026-02-29 10:00:00",
            "0000-01-01 00:00:00",
            "2026-08-24 24:00:00",
            "2026-08-24 07:57:59 ",
            " 2026-08-24 07:57:59",
            "2026-08-24T07:57:59",
            "2026-08-24 07:57",
            "26-08-24 07:57:59",
        ] {
            assert_eq!(parse_local(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn utc_text_counts_the_calendar() {
        assert_eq!(utc_text(0, false), "1970-01-01T00:00:00Z");
        assert_eq!(utc_text(951_782_400, true), "2000-02-29T00:00:00.000Z");
        assert_eq!(utc_text(32_535_215_999, false), "3000-12-31T23:59:59Z");
    }

    #[test]
    fn iso_time_converts_and_never_makes_up() {
        let t = iso_time(Some(&Value::str("2026-08-24 07:57:59"))).unwrap();
        assert!(t.starts_with("2026-08-2") && t.ends_with('Z') && t.len() == 20, "{t}");
        for value in [Value::str(""), Value::Null, Value::int(5), Value::Bool(true), Value::str("gestern")] {
            assert_eq!(iso_time(Some(&value)), None, "{value:?}");
        }
        assert_eq!(iso_time(None), None);
    }

    #[test]
    fn a_name_with_two_tags_gets_none() {
        let (dir, paths) = temp_home("tags");
        let recipe = |amount: &str| {
            format!(
                r#"[{{"slots": [{{"name": "A", "options": [{{"resourceName": "Iron", "quantity": {amount}}}]}}]}}]"#
            )
        };
        let crafting = format!(
            r#"{{"format": 1, "build": "b1", "blueprints": [
                {{"productName": "Main Powerplant", "tag": "BP_IDRIS", "tiers": {r1}}},
                {{"productName": "Main Powerplant", "tag": "BP_RECLAIMER", "tiers": {r2}}},
                {{"productName": "Arden Helmet", "tag": "BP_ARDEN_1", "tiers": {r1}}},
                {{"productName": "Arden Helmet", "tag": "BP_ARDEN_2", "tiers": {r1}}},
                {{"productName": "Mag (16 cap)", "tag": "  BP_MAG ", "tiers": {r1}}},
                {{"productName": "7MA 'Lorica'", "tag": "BP_LORICA", "tiers": {r1}}},
                {{"tag": "BP_CRAFT_RADR_GNRP_S03_Idris_TEMP"}},
                {{"productName": "Falsch", "guid": "ABC", "tag": "BP_FIXED"}},
                {{"productName": "Zwei", "tag": "BP_Z1", "tiers": {r1}}},
                {{"productName": "Zwei", "tag": "BP_Z2", "tiers": {r3}}}
            ]}}"#,
            r1 = recipe("1"),
            r2 = recipe("2"),
            r3 = recipe("1.0"),
        );
        fs::write(dir.join(CRAFTING_FILE), crafting).unwrap();
        fs::write(
            dir.join(ISSUES_FILE),
            r#"{"format": 1, "build": "b1", "daten": {"blueprintRecords": {"abc": {"expected": {"name": "Richtig"}}}}}"#,
        )
        .unwrap();
        let tags = unique_tags(&paths);
        assert_eq!(tags.get("main powerplant"), None, "two items of the same name");
        assert_eq!(tags.get("arden helmet").map(String::as_str), Some("BP_ARDEN_1"), "same recipe: first tag");
        assert_eq!(tags.get(&name_key("Mag (16 Schuss)")).map(String::as_str), Some("BP_MAG"));
        assert_eq!(tags.get(&name_key("7MA \"Lorica\"")).map(String::as_str), Some("BP_LORICA"));
        assert_eq!(tags.get("radr gnrp s03 idris").map(String::as_str), Some("BP_CRAFT_RADR_GNRP_S03_Idris_TEMP"));
        assert_eq!(tags.get("richtig").map(String::as_str), Some("BP_FIXED"), "name from the error list");
        assert_eq!(tags.get("falsch"), None);
        assert_eq!(tags.get("zwei"), None, "1 and 1.0 are different amounts in Python's text");

        fs::write(dir.join(CRAFTING_FILE), r#"{"format": 1, "blueprints": [{"productName": "A", "tag": "T"}, "x"]}"#)
            .unwrap();
        assert!(recipe_tags(&paths).is_err() && unique_tags(&paths).is_empty(), "a broken entry empties the table");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn titles_lose_their_marks_and_runs_merge() {
        assert_eq!(clean_title("Hilfe <EM4>[BP]?</EM4>  für  X"), "Hilfe für X");
        let block = legacy::TITLE_BLOCK.replace('\\', "").replace(".*?", "alt");
        assert_eq!(clean_title(&format!("{block}Titel")), "Titel");
        assert_eq!(clean_title("<EM4>[150 Rep] [Bauplan]*</EM4>Y"), "Y");
        let (dir, paths) = temp_home("auftraege");
        let file = paths.app_file(CONTRACT_LOG_FILE);
        fs::write(
            &file,
            r#"{"format": 2, "auftraege": [
                {"name": "A <EM4>[BP]</EM4>", "wann": "2026-01-01", "zustand": "laeuft", "ziele": 1},
                {"name": "A", "wann": "2026-01-01", "zustand": "abgeschlossen", "bis": "2026-01-02", "ziele": ""},
                {"name": "B", "wann": "2026-02-01", "zustand": "laeuft"},
                {"name": "C", "zustand": "laeuft"}
            ]}"#,
        )
        .unwrap();
        let entries = contract_log(&paths).unwrap();
        let names: Vec<&str> = entries.iter().filter_map(|e| e.get("name").and_then(Value::as_str)).collect();
        assert_eq!(names, ["B", "A", "C"], "newest start first, the marked and unmarked run are one");
        assert_eq!(entries[1].get("zustand").and_then(Value::as_str), Some("abgeschlossen"));
        assert_eq!(entries[1].get("ziele"), Some(&Value::int(1)), "an empty value does not overwrite");
        assert!(contract_log_json(&entries).ends_with("}\n"));

        fs::write(&file, r#"{"format": 2, "auftraege": [{"name": "A"}, 5]}"#).unwrap();
        assert!(contract_log(&paths).is_err(), "an entry that is not an object");
        fs::write(&file, r#"{"format": 1, "auftraege": [{"name": "A"}]}"#).unwrap();
        assert_eq!(contract_log(&paths), Ok(Vec::new()), "another format");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_full_backup_keeps_zero_and_drops_empty() {
        let mut catalog = CatalogView { version: Value::str(""), blueprints: HashMap::new() };
        let entry = jsonio::loads(r#"{"a": "", "c": "", "s": 0, "g": false, "m": null}"#).unwrap();
        catalog.blueprints.insert("helm".into(), entry.as_object().unwrap().clone());
        let data = collection(r#""helm": {"name": "Helm", "quelle": "", "zeit": null}"#);
        let doc = complete(&data, &catalog, 0).unwrap();
        let text = jsonio::dumps(&doc, None, false);
        assert_eq!(
            text,
            r#"{"werkzeug": "VerseKit", "erstellt": "1970-01-01T00:00:00Z", "spielversion": null, "anzahl": 1, "bauplaene": [{"name": "Helm", "size": 0, "grade": false}]}"#
        );
    }

    #[test]
    fn dated_files_move_and_others_stay() {
        let (dir, _paths) = temp_home("aelter");
        let folder = dir.join(FOLDER);
        fs::create_dir_all(folder.join(OLD_FOLDER)).unwrap();
        let dated_contracts = format!("{}-2026-09-01.json", legacy::EXPORT_CONTRACTS);
        for name in [
            "SC-Blueprints-Basetool.json",
            "SC-Blueprints-Basetool-2026-08-27.json",
            "scmdb-import-2026-09-13.json",
            "scmdb-importX.json",
            &dated_contracts,
            "bauplaene-db-import.json",
            "scmdb-import-1.json.tmp",
        ] {
            fs::write(folder.join(name), name).unwrap();
        }
        fs::write(folder.join(OLD_FOLDER).join("scmdb-import-2026-09-13.json"), "alt").unwrap();
        tidy_old_files(&folder.to_string_lossy());
        let left: HashSet<String> =
            fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let expected: HashSet<String> =
            [OLD_FOLDER, "SC-Blueprints-Basetool.json", "bauplaene-db-import.json", "scmdb-import-1.json.tmp"]
                .into_iter()
                .map(String::from)
                .collect();
        assert_eq!(left, expected);
        let old = folder.join(OLD_FOLDER);
        assert_eq!(
            fs::read_to_string(old.join("scmdb-import-2026-09-13.json")).unwrap(),
            "alt",
            "the older copy stays"
        );
        assert!(old.join("scmdb-importX.json").exists() && old.join(&dated_contracts).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_writes_four_files_or_stops_on_a_data_error() {
        let (dir, paths) = temp_home("ablage");
        fs::write(dir.join(CONTRACT_LOG_FILE), r#"{"format": 2, "auftraege": [{"name": "A", "wann": "x"}]}"#).unwrap();
        let exports = FileExports { paths: &paths, version: "5.0.0", clock: || 0 };
        let empty = collection("");
        let done = exports.archive_files(&empty).unwrap();
        assert_eq!(done.written, [Kind::Contracts.file_name()], "no blueprints, no blueprint files");
        let data = collection(r#""helm": {"name": " Helm ", "zeit": "2026-08-24 07:57:59"}, "leer": {"name": ""}"#);
        let done = exports.archive_files(&data).unwrap();
        assert_eq!(done.written, Kind::ALL.map(Kind::file_name));
        let basetool = fs::read_to_string(dir.join(FOLDER).join(Kind::Basetool.file_name())).unwrap();
        assert!(
            basetool.starts_with(
                "{\n \"blueprints\": [\n  {\n   \"productName\": \"Helm\",\n   \"receivedAt\": \"2026-08-2"
            )
        );
        assert!(!basetool.contains('\r') && !basetool.ends_with('\n'));
        let scmdb = fs::read_to_string(dir.join(FOLDER).join(Kind::Scmdb.file_name())).unwrap();
        assert!(scmdb.contains("\"watcherVersion\": \"5.0.0\",\n \"exportedAt\": \"1970-01-01T00:00:00.000Z\""));

        let broken = collection(r#""helm": "kein objekt""#);
        assert!(exports.archive_files(&broken).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A folder where a file belongs: that file fails, its `.tmp` stays as in
    /// Python, the other three are written.
    #[test]
    fn one_file_failing_leaves_the_others() {
        let (dir, paths) = temp_home("ordner");
        fs::write(dir.join(CONTRACT_LOG_FILE), r#"{"format": 2, "auftraege": [{"name": "A"}]}"#).unwrap();
        let folder = dir.join(FOLDER);
        fs::create_dir_all(folder.join(Kind::Scmdb.file_name())).unwrap();
        let exports = FileExports { paths: &paths, version: "5.0.0", clock: || 0 };
        let data = collection(r#""helm": {"name": "Helm"}"#);
        let done = exports.archive_files(&data).unwrap();
        assert_eq!(done.written, [Kind::Basetool, Kind::Full, Kind::Contracts].map(Kind::file_name));
        assert!(folder.join(Kind::Scmdb.file_name()).is_dir());
        assert!(folder.join(format!("{}.tmp", Kind::Scmdb.file_name())).is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
