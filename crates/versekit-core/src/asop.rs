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

//! Own ship names at the retrieval terminal (ASOP), port of `asop.py`.
//!
//! The names live in `asop.json` (`{"format": 1, "namen": {key: {"name",
//! "stern"}}}`), keyed by the `vehicle_Name…` keys of the game's
//! `global.ini`. A key belongs to the model, not to one ship. Which key a
//! hangar ship has is found by `match_ships`: exact steps first, fuzzy ones
//! last, and every step only counts with exactly one candidate.
//!
//! Writing the names into the game's `global.ini` is not part of this
//! module (`build_table` and `display_name` prepare it).

use std::collections::HashMap;

use crate::errors;
use crate::game_ini;
use crate::jsonio::{self, Map, Newline, Value, map_set};
use crate::paths::Paths;
use crate::pypath::Flavor;
use crate::text::{py_isspace, py_strip};

pub const FILE: &str = "asop.json";
pub const FORMAT: i64 = 1;
pub const PREFIX: &str = "vehicle_Name";
pub const SHORT_SUFFIX: &str = "_short";
/// The mark before a name.
pub const STAR: char = '*';
/// Longest own name, in characters.
pub const MAX_LENGTH: usize = 40;
/// The note of the original texts (`injection.ORIGTEXT_FILE`).
pub const ORIGTEXT_FILE: &str = "injektion-urtext.json";
/// The mark in the original texts for a line that was added
/// (`injection.ADDED`).
pub const ADDED: &str = "\u{0}versekit-ergaenzt";
/// The stored original names from the `Data.p4k` (`gametext.NAMES_FILE`).
pub const NAMES_FILE: &str = "namen-original.json";

/// `vehicle_Name` key → name, as read from a `global.ini`.
pub type Table = HashMap<String, String>;

/// An empty collection.
pub fn empty() -> Map {
    vec![("format".into(), Value::int(FORMAT)), ("namen".into(), Value::Object(Map::new()))]
}

fn get<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Python's truth value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

fn names(data: &Map) -> Option<&Map> {
    get(data, "namen").and_then(Value::as_object)
}

/// The own names, or an empty collection. Only `format` 1 with an object
/// `namen` counts; an unreadable file is reported.
pub fn load(path: &str) -> Map {
    match jsonio::read_json(path) {
        Ok(Value::Object(data)) => {
            if jsonio::py_eq(get(&data, "format"), Some(&Value::int(FORMAT))) && names(&data).is_some() {
                return data;
            }
        }
        Ok(_) => errors::record("asop.load", &"kein Objekt"),
        Err(jsonio::ReadError::NotFound) => {}
        Err(e) => errors::record("asop.load", &e),
    }
    empty()
}

/// What `json.dump(data, f, ensure_ascii=False, indent=1)` writes.
pub fn file_text(data: &Map) -> String {
    jsonio::dumps(&Value::Object(data.clone()), Some(1), false)
}

/// Store atomically; false (reported) when it failed.
pub fn save(path: &str, data: &Map) -> bool {
    let result = (|| {
        let folder = Flavor::native().dirname(path);
        if !folder.is_empty() {
            jsonio::make_dirs(&folder)?;
        }
        jsonio::write_text_atomic(path, &file_text(data), Newline::Native)
    })();
    match result {
        Ok(()) => {
            // The names go to the Basetool with the hangar.
            crate::basetool::sync::store_saved(path);
            true
        }
        Err(e) => {
            errors::record("asop.save", &e);
            false
        }
    }
}

/// What `store` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    Written,
    /// The entry was already so; nothing was written.
    Unchanged,
    /// A file that is no collection of names was set aside first.
    SetAside,
    Failed,
}

/// What is at the path before a name is stored.
enum Found {
    /// A collection of names, or no file.
    Usable,
    /// Read, but no collection of names (`load` would start empty and the
    /// next save would write over it).
    Unusable,
    /// Could not be read at all (locked, no access).
    Unreadable,
}

fn inspect(path: &str) -> Found {
    match jsonio::read_json(path) {
        Ok(Value::Object(data))
            if jsonio::py_eq(get(&data, "format"), Some(&Value::int(FORMAT))) && names(&data).is_some() =>
        {
            Found::Usable
        }
        Err(jsonio::ReadError::NotFound) => Found::Usable,
        Err(jsonio::ReadError::Os(_)) => Found::Unreadable,
        _ => Found::Unusable,
    }
}

/// Move an unusable file to a free `asop.kaputt-<time>.json`.
fn set_aside(path: &str) -> Result<(), String> {
    let stamp = jsonio::time_stamp();
    let name = |n: usize| {
        let tail = if n == 0 { String::new() } else { format!("-{n}") };
        match path.strip_suffix(".json") {
            Some(stem) => format!("{stem}.kaputt-{stamp}{tail}.json"),
            None => format!("{path}.kaputt-{stamp}{tail}"),
        }
    };
    let aside = (0..).map(name).find(|n| !std::path::Path::new(n).exists()).unwrap_or_else(|| name(0));
    jsonio::os_replace(path, &aside).map_err(|e| e.to_string())?;
    errors::record("asop.save", &format!("keine Namensliste, beiseitegelegt: {aside}"));
    Ok(())
}

static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// One name from the page: read the file as it is now, change only this
/// key and write it back. Returns what happened and the collection as it
/// is in the file now; `None` when nothing was written.
///
/// ⚠ 4.12 writes the collection it read when the page was built, over
/// what another place wrote since, and over a file that is no collection.
pub fn store(path: &str, key: &str, name: &str, star: bool) -> (Stored, Option<Map>) {
    let _lock = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let set_aside_first = match inspect(path) {
        Found::Usable => false,
        Found::Unusable => true,
        Found::Unreadable => {
            errors::record("asop.save", &"asop.json unlesbar");
            return (Stored::Failed, None);
        }
    };
    if set_aside_first && let Err(e) = set_aside(path) {
        errors::record("asop.save", &e);
        return (Stored::Failed, None);
    }
    let mut data = load(path);
    let before = data.clone();
    set_name(&mut data, key, name, star);
    if data == before && !set_aside_first {
        return (Stored::Unchanged, Some(data));
    }
    match (save(path, &data), set_aside_first) {
        (false, _) => (Stored::Failed, None),
        (true, true) => (Stored::SetAside, Some(data)),
        (true, false) => (Stored::Written, Some(data)),
    }
}

/// Enter an own name, or remove it: an empty name without the star
/// removes the entry. The name is stripped and cut to `MAX_LENGTH`.
pub fn set_name(data: &mut Map, key: &str, name: &str, star: bool) {
    let name: String = py_strip(name).chars().take(MAX_LENGTH).collect();
    if get(data, "namen").is_none() {
        data.push(("namen".into(), Value::Object(Map::new())));
    }
    let Some((_, Value::Object(names))) = data.iter_mut().find(|(k, _)| k == "namen") else { return };
    if name.is_empty() && !star {
        names.retain(|(k, _)| k != key);
    } else {
        let entry: Map = vec![("name".into(), Value::str(name)), ("stern".into(), Value::Bool(star))];
        map_set(names, key, Value::Object(entry));
    }
}

/// The own name and the star of a key; `("", false)` without an entry.
pub fn entry(data: &Map, key: &str) -> (String, bool) {
    let Some(found) = names(data).and_then(|n| get(n, key)) else { return (String::new(), false) };
    let name = match found.get("name") {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    };
    (name, truthy(found.get("stern")))
}

/// How many keys have an entry.
pub fn count(data: &Map) -> usize {
    names(data).map_or(0, Vec::len)
}

// ------------------------------------------------------------ matching

/// `re.sub(r'[^a-z0-9]', '', text.lower())`.
pub fn slim(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// The text after the first `PREFIX.len()` characters, and whether those
/// spell `vehicle_Name` in any case.
fn split_prefix(text: &str) -> (bool, &str) {
    let cut = text.char_indices().nth(PREFIX.len()).map_or(text.len(), |(i, _)| i);
    let head = &text[..cut];
    (head.to_lowercase() == PREFIX.to_lowercase(), &text[cut..])
}

/// The vehicle names of `global.ini` lines: `{key: value}` for the long
/// names. `…_short` keys and keys with an addition (`,P`) stay out; the
/// start of the key counts in any case.
pub fn read_keys<S: AsRef<str>>(lines: &[S]) -> Table {
    let mut table = Table::new();
    for line in lines {
        let line = line.as_ref();
        let (named, _) = split_prefix(line);
        if !named {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.contains(',') || split_prefix(key).1.to_lowercase().ends_with(SHORT_SUFFIX) {
            continue;
        }
        table.insert(key.to_string(), value.trim_end_matches(['\r', '\n']).to_string());
    }
    table
}

/// The table with the factory names: where the running file carries an
/// own name, the stored original text counts. A star in front is cut.
///
/// ⚠ A line marked as added (`ADDED`) keeps the value of the table; 4.12
/// takes the mark itself as the factory name.
pub fn factory_names(table: &Table, origtext: &Map) -> Table {
    table
        .iter()
        .map(|(k, v)| {
            let base = match get(origtext, k) {
                Some(Value::Str(s)) if !s.is_empty() && s != ADDED => s,
                _ => v,
            };
            (k.clone(), base.trim_start_matches(STAR).to_string())
        })
        .collect()
}

/// Vehicle names (long and `…_short`) the reference knows and `lines` do
/// not, in the order of the reference. Keys compare in any case; a star in
/// front of the value is cut.
pub fn missing_names<S: AsRef<str>, R: AsRef<str>>(lines: &[S], reference: &[R]) -> Vec<(String, String)> {
    let mut present = std::collections::HashSet::new();
    for line in lines {
        let line = line.as_ref();
        if split_prefix(line).0
            && let Some((key, _)) = line.split_once('=')
        {
            present.insert(key.split(',').next().unwrap_or("").to_lowercase());
        }
    }
    let mut result: Vec<(String, String)> = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for line in reference {
        let line = line.as_ref();
        if !split_prefix(line).0 {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.contains(',') || present.contains(&key.to_lowercase()) || taken.contains(key) {
            continue;
        }
        taken.insert(key.to_string());
        result.push((key.to_string(), value.trim_end_matches(['\r', '\n']).trim_start_matches(STAR).to_string()));
    }
    result
}

/// Which key a hangar ship has, and how it was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// The name in the hangar, stripped.
    pub name: String,
    /// The short name of the hangar entry, stripped.
    pub short: String,
    /// The `vehicle_Name` key; empty without a match.
    pub key: String,
    /// The value of the key in the table.
    pub factory: String,
    /// The step that found it: `kurz`, `name`, `wertende`, `kurzanfang`,
    /// `imwert`, `gekuerzt`; without a key `mehrdeutig` or `nichts`.
    pub way: &'static str,
}

/// A table entry with its slimmed key part and value.
struct Row<'a> {
    key: &'a str,
    key_slim: String,
    value_slim: String,
}

/// The key of each ship (`{"name", "kurz", …}` entries), sorted by name
/// without case. Ships without a name are left out.
pub fn match_ships(ships: &[&Map], table: &Table) -> Vec<Match> {
    let rows: Vec<Row> =
        table.iter().map(|(k, v)| Row { key: k, key_slim: slim(split_prefix(k).1), value_slim: slim(v) }).collect();
    let mut by_key: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut by_value: HashMap<&str, Vec<&str>> = HashMap::new();
    for row in &rows {
        by_key.entry(&row.key_slim).or_default().push(row.key);
        by_value.entry(&row.value_slim).or_default().push(row.key);
    }
    let field = |ship: &Map, key: &str| match get(ship, key) {
        Some(Value::Str(s)) => py_strip(s).to_string(),
        _ => String::new(),
    };
    let mut result: Vec<(String, Match)> = Vec::new();
    for ship in ships {
        let name = field(ship, "name");
        if name.is_empty() {
            continue;
        }
        let short = field(ship, "kurz");
        let (key, way) = ladder(&name, &short, &rows, &by_key, &by_value);
        let factory = key.and_then(|k| table.get(k)).cloned().unwrap_or_default();
        result.push((name.to_lowercase(), Match { key: key.unwrap_or("").to_string(), name, short, factory, way }));
    }
    // Stable, like Python's `sort`.
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result.into_iter().map(|(_, m)| m).collect()
}

/// How the hangar stands for own names: `(renamable, all, without a match)`. Renamable is a ship with a key in
/// the language file; the names without one stay in the order of [`match_ships`].
pub fn naming_summary(matches: &[Match]) -> (usize, usize, Vec<String>) {
    let without: Vec<String> = matches.iter().filter(|m| m.key.is_empty()).map(|m| m.name.clone()).collect();
    (matches.len() - without.len(), matches.len(), without)
}

/// `_ladder`: exact first, fuzzy last, and never with several candidates.
/// A step with several candidates hands on; when nothing is left, it makes
/// the answer `mehrdeutig` instead of `nichts`.
fn ladder<'a>(
    name: &str,
    short: &str,
    rows: &[Row<'a>],
    by_key: &HashMap<&str, Vec<&'a str>>,
    by_value: &HashMap<&str, Vec<&'a str>>,
) -> (Option<&'a str>, &'static str) {
    let (n, k) = (slim(name), slim(short));
    let mut ambiguous = false;
    let mut step = |hits: Vec<&'a str>, way: &'static str| -> Option<(Option<&'a str>, &'static str)> {
        if hits.len() == 1 {
            return Some((Some(hits[0]), way));
        }
        ambiguous |= hits.len() > 1;
        None
    };
    if !k.is_empty()
        && let Some(found) = step(by_key.get(k.as_str()).cloned().unwrap_or_default(), "kurz")
    {
        return found;
    }
    if !n.is_empty() {
        if let Some(found) = step(by_value.get(n.as_str()).cloned().unwrap_or_default(), "name") {
            return found;
        }
        let hits = rows.iter().filter(|r| r.value_slim.ends_with(&n)).map(|r| r.key).collect();
        if let Some(found) = step(hits, "wertende") {
            return found;
        }
    }
    if !k.is_empty() {
        let hits = rows.iter().filter(|r| r.key_slim.starts_with(&k)).map(|r| r.key).collect();
        if let Some(found) = step(hits, "kurzanfang") {
            return found;
        }
    }
    if !n.is_empty() {
        let hits = rows.iter().filter(|r| r.value_slim.contains(&n)).map(|r| r.key).collect();
        if let Some(found) = step(hits, "imwert") {
            return found;
        }
    }
    // A package name with a paint behind it: cut word by word from the
    // back, never below two words.
    let words: Vec<&str> = name.split(py_isspace).filter(|w| !w.is_empty()).collect();
    if !ambiguous {
        for end in (2..words.len()).rev() {
            let cut = slim(&words[..end].join(" "));
            let hits: Vec<&str> = rows.iter().filter(|r| r.value_slim.ends_with(&cut)).map(|r| r.key).collect();
            if hits.len() == 1 {
                return (Some(hits[0]), "gekuerzt");
            }
            if hits.len() > 1 {
                ambiguous = true;
                break;
            }
        }
    }
    (None, if ambiguous { "mehrdeutig" } else { "nichts" })
}

// ------------------------------------------------------------ for the game

/// How the name should read in the game: the own name or the factory name,
/// with the star in front when set. A star already there is cut first.
pub fn display_name(factory: &str, own: &str, star: bool) -> String {
    let own = py_strip(own);
    let base = if own.is_empty() { factory } else { own };
    let base = py_strip(base.trim_start_matches(STAR));
    if star { format!("{STAR}{base}") } else { base.to_string() }
}

/// `{key: (own name, star)}` for the keys the lines know, in the order of
/// `asop.json`; empty when nothing is named. A key the file no longer
/// knows stays in `asop.json` and is skipped here.
pub fn build_table<S: AsRef<str>>(lines: &[S], data: &Map) -> Vec<(String, (String, bool))> {
    let Some(names) = names(data).filter(|n| !n.is_empty()) else { return Vec::new() };
    let present = read_keys(lines);
    let mut done = Vec::new();
    for (key, e) in names {
        if !present.contains_key(key) {
            continue;
        }
        let name = match e.get("name") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let star = truthy(e.get("stern"));
        if !name.is_empty() || star {
            done.push((key.clone(), (name, star)));
        }
    }
    done
}

// ------------------------------------------------------------ the table

/// The stored original texts (`injection.load_origtext`), or empty.
pub fn load_origtext(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(ORIGTEXT_FILE)) {
        Ok(Value::Object(file)) => match get(&file, "texte") {
            Some(Value::Object(texts)) => texts.clone(),
            _ => Map::new(),
        },
        _ => Map::new(),
    }
}

/// A JSON value as `'%s' % value` writes it, for names that are no text.
fn py_str(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.clone(),
        Value::Float(x) => jsonio::float_text(*x),
        other => jsonio::dumps(other, None, false),
    }
}

/// The stored original names as `key=name` lines.
pub fn saved_name_lines(names: &Map) -> Vec<String> {
    names.iter().map(|(k, v)| format!("{k}={}", py_str(v))).collect()
}

/// `injection._added_ship_names`: vehicle names of the English file that
/// this file lacks. Next to it the English `global.ini`; without one the
/// stored original names (`namen-original.json`). Lines added last time
/// count as missing.
///
/// 4.12 reads the names from the `Data.p4k` here when none are stored or
/// the archive changed; this only takes the stored ones, so the callers
/// store them first (`game_texts::prepare` through `Archive::refresh_names`,
/// the ship names page before its table).
pub fn added_ship_names(paths: &Paths, ini: &str, lines: &[String], origtext: &Map) -> Vec<(String, String)> {
    let flavor = paths.host.flavor;
    let folder = flavor.dirname(ini);
    if flavor.basename(&folder).to_lowercase() == "english" {
        return Vec::new();
    }
    let english = flavor.join_all(&flavor.dirname(&folder), &["english", "global.ini"]);
    let reference: Vec<String> = if game_ini::is_file(&english) {
        game_ini::read_lines(&english).unwrap_or_default()
    } else {
        match jsonio::read_json(&paths.app_file(NAMES_FILE)) {
            Ok(Value::Object(names)) => saved_name_lines(&names),
            _ => Vec::new(),
        }
    };
    if reference.is_empty() {
        return Vec::new();
    }
    let own: Vec<&String> = lines
        .iter()
        .filter(|line| {
            let key = line.split('=').next().unwrap_or("");
            !matches!(get(origtext, key), Some(Value::Str(s)) if s == ADDED)
        })
        .collect();
    missing_names(&own, &reference)
}

/// `asop._ini_lines`: the lines of the `global.ini` that counts, with the
/// added ship names behind them; empty without a file.
pub fn ini_lines(paths: &Paths, ini: Option<&str>) -> Vec<String> {
    let Some(ini) = ini.filter(|p| game_ini::is_file(p)) else { return Vec::new() };
    let Some(mut lines) = game_ini::read_lines(ini) else { return Vec::new() };
    let extra = added_ship_names(paths, ini, &lines, &load_origtext(paths));
    lines.extend(extra.into_iter().map(|(k, v)| format!("{k}={v}")));
    lines
}

/// `asop.factory_table`: key → factory name from the `global.ini` that
/// counts; empty without one.
pub fn factory_table(paths: &Paths) -> Table {
    let ini = game_ini::ini_file(paths);
    let lines = ini_lines(paths, ini.as_deref());
    if lines.is_empty() {
        return Table::new();
    }
    factory_names(&read_keys(&lines), &load_origtext(paths))
}

/// Path, modification time and size of each source of the table, to
/// notice a change (`asop._stamp`).
pub fn table_stamp(paths: &Paths) -> Vec<(String, Option<(std::time::SystemTime, u64)>)> {
    let files = [game_ini::ini_file(paths).unwrap_or_default(), paths.app_file(ORIGTEXT_FILE)];
    files
        .into_iter()
        .map(|file| {
            let info = std::fs::metadata(&file).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
            (file, info)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(name: &str, short: &str) -> Map {
        vec![("name".into(), Value::str(name)), ("kurz".into(), Value::str(short))]
    }

    fn table(lines: &[&str]) -> Table {
        read_keys(lines)
    }

    fn one(name: &str, short: &str, t: &Table) -> (String, &'static str) {
        let s = ship(name, short);
        let found = match_ships(&[&s], t);
        (found[0].key.clone(), found[0].way)
    }

    const INI: &[&str] = &[
        "vehicle_NameANVL_Hornet_F7C_M_Mk2=F7C-M Super Hornet Mk II",
        "vehicle_NameANVL_F7C_M_Super_Hornet_Mk_I=F7C-M Super Hornet Mk I",
        "vehicle_NameANVL_F7C_M_Super_Hornet_Mk_I_short=Super Hornet",
        "vehicle_NameRSI_URSA_Medivac=Ursa Medivac",
        "vehicle_NameRSI_Ursa_Rover=Ursa Rover",
        "vehicle_NameANVL_C8R_Pisces_Rescue=C8R Pisces Rescue",
        "vehicle_NameARGO_ATLS_IKTI=Argo ATLS IKTI",
        "vehicle_NameARGO_ATLS=Argo ATLS",
        "vehicle_nameRSI_Paladin=Paladin",
        "vehicle_NameAEGS_Gladius,P=Gladius",
        "vehicle_NameDRAK_Cutlass_Black=Cutlass Black",
        "vehicle_NameDRAK_Cutlass_Black_PIR=Cutlass Black",
    ];

    #[test]
    fn keys_skip_short_and_additions_and_ignore_case_of_the_start() {
        let t = table(INI);
        assert!(t.contains_key("vehicle_nameRSI_Paladin"));
        assert!(!t.keys().any(|k| k.ends_with("_short")));
        assert!(!t.keys().any(|k| k.contains(',')));
    }

    #[test]
    fn the_name_saves_a_wrong_short_name() {
        // The Mk II carries the short name of the Mk I.
        let t = table(INI);
        assert_eq!(one("F7C-M Super Hornet Mk II", "", &t).0, "vehicle_NameANVL_Hornet_F7C_M_Mk2");
        assert_eq!(
            one("F7C-M Super Hornet Mk I", "ANVL_F7C_M_Super_Hornet_Mk_I", &t),
            ("vehicle_NameANVL_F7C_M_Super_Hornet_Mk_I".to_string(), "kurz")
        );
    }

    #[test]
    fn the_summary_counts_the_ships_with_a_key_and_names_the_others() {
        let t = table(INI);
        let ships = [ship("Paladin", ""), ship("Idris", ""), ship("Cutlass Black", ""), ship("Ursa Rover", "")];
        let refs: Vec<&Map> = ships.iter().collect();
        let (renamable, all, without) = naming_summary(&match_ships(&refs, &t));
        assert_eq!((renamable, all), (2, 4));
        assert_eq!(without, ["Cutlass Black", "Idris"], "sorted as the matches, ambiguous ones among them");
        assert_eq!(naming_summary(&[]), (0, 0, Vec::<String>::new()));
    }

    #[test]
    fn several_candidates_give_nothing() {
        let t = table(INI);
        // Two keys with the same factory name.
        assert_eq!(one("Cutlass Black", "", &t), (String::new(), "mehrdeutig"));
        assert_eq!(one("Idris", "", &t), (String::new(), "nichts"));
    }

    #[test]
    fn steps_of_the_ladder() {
        let t = table(INI);
        assert_eq!(one("Ursa Medivac", "RSI_Ursa", &t).1, "name");
        assert_eq!(one("C8R Pisces", "ANVL_C8R_Pisces", &t).1, "kurzanfang");
        assert_eq!(one("ATLS IKTI Akuma", "", &t), ("vehicle_NameARGO_ATLS_IKTI".to_string(), "gekuerzt"));
        assert_eq!(one("Medivac", "", &t).1, "wertende");
    }

    #[test]
    fn set_name_strips_cuts_and_removes() {
        let mut data = empty();
        set_name(&mut data, "k", "  Nachtfalke  ", false);
        assert_eq!(entry(&data, "k"), ("Nachtfalke".to_string(), false));
        set_name(&mut data, "k", &"x".repeat(50), true);
        assert_eq!(entry(&data, "k").0.chars().count(), MAX_LENGTH);
        set_name(&mut data, "k", "", true);
        assert_eq!(entry(&data, "k"), (String::new(), true));
        set_name(&mut data, "k", "  ", false);
        assert_eq!(count(&data), 0);
    }

    #[test]
    fn display_name_never_stacks_stars() {
        assert_eq!(display_name("Gladius", "", true), "*Gladius");
        assert_eq!(display_name("*Gladius", "", true), "*Gladius");
        assert_eq!(display_name("Gladius", " Falke ", false), "Falke");
        assert_eq!(display_name("Gladius", "**Falke", true), "*Falke");
    }

    #[test]
    fn build_table_only_for_known_keys() {
        let mut data = empty();
        set_name(&mut data, "vehicle_NameRSI_Ursa_Rover", "Rover", false);
        set_name(&mut data, "vehicle_NameGIBT_ES_NICHT", "Weg", true);
        let built = build_table(INI, &data);
        assert_eq!(built, vec![("vehicle_NameRSI_Ursa_Rover".to_string(), ("Rover".to_string(), false))]);
    }

    #[test]
    fn an_added_line_keeps_its_name() {
        let t = table(&["vehicle_NameRSI_Ursa_Rover=*Rover"]);
        let origtext: Map = vec![("vehicle_NameRSI_Ursa_Rover".into(), Value::str(ADDED))];
        assert_eq!(factory_names(&t, &origtext)["vehicle_NameRSI_Ursa_Rover"], "Rover");
        let origtext: Map = vec![("vehicle_NameRSI_Ursa_Rover".into(), Value::str("Ursa Rover"))];
        assert_eq!(factory_names(&t, &origtext)["vehicle_NameRSI_Ursa_Rover"], "Ursa Rover");
    }

    fn scratch(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("versekit-asop-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join(FILE).to_string_lossy().into_owned();
        (dir, file)
    }

    #[test]
    fn store_keeps_what_another_place_wrote() {
        let (dir, file) = scratch("fremd");
        assert_eq!(store(&file, "a", "Erster", false).0, Stored::Written);
        // Meanwhile another place adds a name.
        let mut other = load(&file);
        set_name(&mut other, "b", "Fremd", true);
        assert!(save(&file, &other));
        let (done, data) = store(&file, "a", "Zweiter", false);
        assert_eq!(done, Stored::Written);
        assert_eq!(entry(&data.unwrap_or_default(), "b"), ("Fremd".to_string(), true));
        assert_eq!(entry(&load(&file), "a").0, "Zweiter");
        assert_eq!(store(&file, "a", " Zweiter ", false).0, Stored::Unchanged);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_sets_aside_a_file_that_is_no_collection() {
        let (dir, file) = scratch("kaputt");
        std::fs::write(&file, r#"{"format": 2, "namen": {"a": {"name": "neu"}}}"#).expect("file");
        let (done, data) = store(&file, "k", "Falke", false);
        assert_eq!(done, Stored::SetAside);
        assert_eq!(count(&data.unwrap_or_default()), 1);
        let aside: Vec<String> = std::fs::read_dir(&dir)
            .expect("folder")
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .filter(|n| n.starts_with("asop.kaputt-"))
            .collect();
        assert_eq!(aside.len(), 1, "{aside:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_never_sets_aside_a_file_it_cannot_read() {
        let (dir, file) = scratch("unlesbar");
        // A folder in place of the file: reading fails with an OS error.
        std::fs::create_dir_all(&file).expect("folder");
        assert_eq!(store(&file, "k", "Falke", false), (Stored::Failed, None));
        assert!(std::path::Path::new(&file).is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_names_in_reference_order() {
        let own = ["vehicle_nameRSI_Ursa_Rover=Rover", "vehicle_NameX,P=x"];
        let reference = ["vehicle_NameRSI_Ursa_Rover=Ursa Rover", "vehicle_NameB=*Bee", "vehicle_NameA=A", "x=y"];
        assert_eq!(
            missing_names(&own, &reference),
            vec![("vehicle_NameB".to_string(), "Bee".to_string()), ("vehicle_NameA".to_string(), "A".to_string())]
        );
    }
}
