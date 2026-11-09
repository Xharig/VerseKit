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

//! The mission log: which contract was played when and how it ended, as the
//! saved `auftragslog.json` holds it. Reading, the title clean-up of older
//! files, merging, the search, and which blueprints a contract title gives.
//! Like `mission_log.py`, `contracts.clean`/`key_for` and
//! `catalog.blueprints_for_contract`, and writing the file back (`save`). The
//! reading of the game logs is a building block of its own (`mission_scan`).
//!
//! ⚠ Entries are kept as the JSON objects of the file, so merging behaves
//! like Python's dictionaries (key order, `update`, truthiness).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;

use crate::catalog_cache::CatalogCache;
use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::paths::{Paths, name_key};
use crate::text::{decode_ignore, lines_universal, py_isspace, py_strip};

/// The log file, in the subfolder `Intern`.
pub const FILE: &str = "auftragslog.json";
/// ⚠ A file of another format is not read at all (format 1 held contracts
/// that stayed „running“ for ever); the reading of the logs rebuilds it.
pub const FORMAT: i64 = 2;

pub const COMPLETED: &str = "abgeschlossen";
pub const ABORTED: &str = "abgebrochen";
pub const RUNNING: &str = "laeuft";
/// No end in the log, but surely no longer open; why it ended is not known.
pub const EXPIRED: &str = "verfallen";
/// Failed is not completed: shown in pale red, not green.
pub const FAILED: &str = "fehlgeschlagen";

/// The game counts as running while its `Game.log` was written this recently.
pub const GAME_IDLE_SEC: f64 = 300.0;

/// Marks in a title, ours and those of other translation tools: the whole
/// inserted block, or only the bubble with `[BP]`/`[Bauplan]` inside `<EM4>`.
static MARKS: LazyLock<Regex> = LazyLock::new(|| {
    let (open, close) = crate::legacy::TITLE_MARK;
    let block = format!("{}.*?{}", regex::escape(open), regex::escape(close));
    Regex::new(&format!(r"{block}|<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>")).expect("marks")
});
/// A placeholder the game fills in only when showing the title.
pub(crate) static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"~mission\([^)]*\)").expect("placeholder"));
/// A text variant behind a key of the `global.ini` (`…_Title,P`).
static VARIANT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",[A-Za-z]{1,3}$").expect("variant"));

/// Python's `' '.join(text.split())`.
pub(crate) fn squeeze(text: &str) -> String {
    text.split(py_isspace).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// A title without the blueprint marks and without doubled spaces
/// (`contracts.clean`).
pub fn clean(title: &str) -> String {
    squeeze(&MARKS.replace_all(title, " "))
}

/// Python's truthiness of a JSON value.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(text) => !text.trim_start_matches('-').trim_start_matches('0').is_empty(),
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

pub(crate) fn get<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// `e.get(key) or ''` for a text field; other types count as empty.
pub(crate) fn text_of<'a>(map: &'a Map, key: &str) -> &'a str {
    match get(map, key) {
        Some(Value::Str(s)) => s,
        _ => "",
    }
}

/// Python's `str()` of a name that is not a text; `None` for lists and objects.
fn name_text(value: &Value) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.clone()),
        Value::Int(text) => Some(text.clone()),
        Value::Float(x) => Some(jsonio::float_text(*x)),
        Value::Bool(true) => Some("True".into()),
        Value::Bool(false) => Some("False".into()),
        _ => None,
    }
}

/// What makes one run of a contract unique: name and start time (`_key`).
pub(crate) fn key_of(map: &Map) -> (String, String) {
    let name = match get(map, "name") {
        Some(v) if truthy(v) => name_text(v).unwrap_or_else(|| format!("{v:?}")),
        _ => String::new(),
    };
    (name, text_of(map, "wann").to_string())
}

fn sorted_newest_first(mut entries: Vec<Map>) -> Vec<Map> {
    // ⚠ Stable and descending, like Python's `sorted(…, reverse=True)`:
    // entries with the same time keep their order.
    entries.sort_by(|a, b| text_of(b, "wann").cmp(text_of(a, "wann")));
    entries
}

/// Saved and freshly read together without losing anything (`merge`).
///
/// ⚠ The newer state only wins when it knows more: an entry already
/// completed must not fall back to running because a log still holds only
/// its start; an end found only now completes the old entry.
pub fn merge(old: &[Map], new: &[Map]) -> Vec<Map> {
    let mut merged: Vec<Map> = Vec::new();
    let mut at: HashMap<(String, String), usize> = HashMap::new();
    let not_empty = |v: &Value| !matches!(v, Value::Null) && v != &Value::Str(String::new());
    for e in old.iter().chain(new) {
        let key = key_of(e);
        let Some(&i) = at.get(&key) else {
            at.insert(key, merged.len());
            merged.push(e.clone());
            continue;
        };
        let before = &mut merged[i];
        let state_of = |m: &Map| get(m, "zustand").cloned();
        let running = Some(Value::str(RUNNING));
        if state_of(before) == running && state_of(e) != running {
            for (k, v) in e.iter().filter(|(_, v)| not_empty(v)) {
                jsonio::map_set(before, k, v.clone());
            }
        } else if state_of(e) == running {
            for (k, v) in e {
                if k != "zustand" && !get(before, k).is_some_and(truthy) && truthy(v) {
                    jsonio::map_set(before, k, v.clone());
                }
            }
        } else {
            for (k, v) in e.iter().filter(|(_, v)| not_empty(v)) {
                jsonio::map_set(before, k, v.clone());
            }
        }
    }
    sorted_newest_first(merged)
}

/// Marks out of titles saved before the clean-up existed (`_clean_titles`).
///
/// ⚠ A changed list goes through [`merge`]: two entries can carry the same
/// key after cleaning, and they belong together.
pub fn clean_titles(entries: Vec<Map>) -> Vec<Map> {
    let mut changed = false;
    let mut cleaned = Vec::with_capacity(entries.len());
    for mut e in entries {
        let name = match get(&e, "name") {
            Some(v) if truthy(v) => name_text(v),
            _ => Some(String::new()),
        };
        if let Some(name) = name {
            let plain = clean(&name);
            let same = get(&e, "name") == Some(&Value::Str(plain.clone()));
            if !plain.is_empty() && !same {
                jsonio::map_set(&mut e, "name", Value::Str(plain));
                changed = true;
            }
        }
        cleaned.push(e);
    }
    if changed { merge(&cleaned, &[]) } else { cleaned }
}

/// Does the file say format 2 (`2.0` too, like Python's `==`)?
pub(crate) fn format_is_current(map: &Map) -> bool {
    match get(map, "format") {
        Some(Value::Int(text)) => text == &FORMAT.to_string(),
        Some(Value::Float(x)) => *x == FORMAT as f64,
        _ => false,
    }
}

/// The saved log holds something Python's `load` cannot work with: an
/// `auftraege` that is no list, or an entry that is no object. Python raises
/// there; the page shows an empty log and a reading that would write the
/// file back must not.
#[derive(Debug, PartialEq, Eq)]
pub struct Unreadable;

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protokoll hat einen unlesbaren Aufbau")
    }
}

/// The entries of a parsed log file: only format 2, titles cleaned.
pub fn entries_checked(data: &Value) -> Result<Vec<Map>, Unreadable> {
    let Value::Object(map) = data else { return Ok(Vec::new()) };
    if map.is_empty() || !format_is_current(map) {
        return Ok(Vec::new());
    }
    let items = match get(map, "auftraege") {
        Some(Value::Array(items)) => items,
        Some(v) if truthy(v) => return Err(Unreadable),
        _ => return Ok(Vec::new()),
    };
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::Object(e) => entries.push(e.clone()),
            _ => return Err(Unreadable),
        }
    }
    Ok(clean_titles(entries))
}

/// The entries of a parsed log file: only format 2, titles cleaned.
///
/// ⚠ An entry that is no object makes the whole log unreadable (the page
/// shows it as empty), and so does a list that is no list.
pub fn entries_of(data: &Value) -> Vec<Map> {
    entries_checked(data).unwrap_or_default()
}

/// The saved log of the data folder (`mission_log.load`); a broken file is
/// set aside and its previous version read, as everywhere.
pub fn load(paths: &Paths) -> Vec<Map> {
    load_checked(paths).unwrap_or_default()
}

/// [`load`], but a log Python would raise on is an error.
pub fn load_checked(paths: &Paths) -> Result<Vec<Map>, Unreadable> {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "mission_log.load") {
        Some(data) => entries_checked(&data),
        None => Ok(Vec::new()),
    }
}

/// Writes the log (`mission_log.save`); reports a failure instead of
/// swallowing it. The previous version stays as `.bak.json`: once the logs
/// are gone, an emptied log cannot be rebuilt.
pub fn save(paths: &Paths, entries: &[Map]) -> bool {
    let data = Value::Object(vec![
        ("format".into(), Value::int(FORMAT)),
        ("auftraege".into(), Value::Array(entries.iter().cloned().map(Value::Object).collect())),
    ]);
    match jsonio::save_json(&paths.app_file(FILE), &data, 1, false) {
        Ok(()) => true,
        Err(e) => {
            errors::record("mission_log.save", &e);
            false
        }
    }
}

/// The state of an entry; without one it counts as running.
pub fn state_of(entry: &Map) -> &str {
    match get(entry, "zustand") {
        Some(Value::Str(s)) if !s.is_empty() => s,
        _ => RUNNING,
    }
}

/// Where the name holds the text, ignoring case (`search`), as positions.
pub fn search_at(entries: &[Map], text: &str) -> Vec<usize> {
    let text = py_strip(text).to_lowercase();
    (0..entries.len())
        .filter(|&i| text.is_empty() || text_of(&entries[i], "name").to_lowercase().contains(&text))
        .collect()
}

/// Filter by name, ignoring case (`search`).
pub fn search<'a>(entries: &'a [Map], text: &str) -> Vec<&'a Map> {
    search_at(entries, text).into_iter().map(|i| &entries[i]).collect()
}

/// First the state, then the search: the counter counts what is shown. As
/// positions; `None` is every state.
pub fn shown_at(entries: &[Map], state: Option<&str>, text: &str) -> Vec<usize> {
    search_at(entries, text).into_iter().filter(|&i| state.is_none_or(|s| state_of(&entries[i]) == s)).collect()
}

/// How often each contract was played: name → (total, completed), in the
/// order of first appearance (`summarize`).
pub fn summarize(entries: &[Map]) -> Vec<(String, (usize, usize))> {
    let mut counter: Vec<(String, (usize, usize))> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for e in entries {
        let name = text_of(e, "name").to_string();
        let i = *at.entry(name.clone()).or_insert_with(|| {
            counter.push((name, (0, 0)));
            counter.len() - 1
        });
        counter[i].1.0 += 1;
        if state_of(e) == COMPLETED && get(e, "zustand").is_some() {
            counter[i].1.1 += 1;
        }
    }
    counter
}

/// The language key of the state word. Running says „läuft“ only while the
/// game writes, „noch offen“ otherwise: logging out ends no contract.
pub fn state_word(state: &str, game_running: bool) -> &'static str {
    match state {
        RUNNING => {
            if game_running {
                "s_al_laeuft"
            } else {
                "s_al_offen"
            }
        }
        COMPLETED => "s_al_fertig",
        ABORTED => "s_al_abbruch",
        FAILED => "s_al_fehl",
        EXPIRED => "s_al_verfallen",
        _ => "s_al_laeuft",
    }
}

/// Is the game writing its log right now (`paths.game_running`)?
pub fn game_running(paths: &Paths) -> bool {
    let Some(log) = paths.game_log(None) else { return false };
    let Ok(modified) = std::fs::metadata(&log).and_then(|m| m.modified()) else { return false };
    let age = SystemTime::now().duration_since(modified).map_or(0.0, |d| d.as_secs_f64());
    age < GAME_IDLE_SEC
}

/// One entry as the page shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mission {
    pub name: String,
    /// Accepted (`wann`), ended (`bis`), as `YYYY-MM-DDThh:mm:ss`.
    pub when: String,
    pub until: String,
    /// The state; without one, running.
    pub state: String,
    /// Objectives done and in total, only known for a running contract.
    pub goals_done: i64,
    pub goals_total: i64,
    pub blueprints: Vec<String>,
    /// The log file it was read from.
    pub source: String,
}

fn number(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

impl Mission {
    pub fn of(entry: &Map) -> Mission {
        Mission {
            name: text_of(entry, "name").to_string(),
            when: text_of(entry, "wann").to_string(),
            until: text_of(entry, "bis").to_string(),
            state: state_of(entry).to_string(),
            goals_done: number(get(entry, "ziele_fertig")),
            goals_total: number(get(entry, "ziele_gesamt")),
            blueprints: match get(entry, "bauplaene") {
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            },
            source: text_of(entry, "quelle").to_string(),
        }
    }

    /// The date part of the start (`wann[:10]`).
    pub fn day(&self) -> String {
        self.when.chars().take(10).collect()
    }

    /// „N of M objectives“ belongs only to a running contract.
    pub fn shows_goals(&self) -> bool {
        self.state == RUNNING && self.goals_total != 0
    }
}

// ---------------------------------------------------------------------------
// Which blueprints a contract title gives
// ---------------------------------------------------------------------------

/// Does a pattern collapse to a mere prefix (placeholder at the end)?
pub(crate) fn weak(parts: &[String]) -> bool {
    parts.last().is_some_and(String::is_empty) && parts.len() > 1
}

/// How much literal text a pattern holds; more is more exact.
pub(crate) fn weight(parts: &[String]) -> usize {
    parts.iter().map(|p| p.chars().filter(|&c| c != '\\').count()).sum()
}

/// The pieces of a title around its placeholders.
pub(crate) fn pieces(title: &str) -> Vec<String> {
    PLACEHOLDER.split(title).map(str::to_string).collect()
}

/// All unpacked `global.ini` of the installation, by language folder.
pub fn ini_files(paths: &Paths) -> Vec<PathBuf> {
    let Some(game) = paths.game_folder() else { return Vec::new() };
    let base = Path::new(&game).join("data").join("Localization");
    let Ok(read) = std::fs::read_dir(&base) else { return Vec::new() };
    let mut names: Vec<String> =
        read.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names.into_iter().map(|n| base.join(n).join("global.ini")).filter(|p| p.is_file()).collect()
}

/// Displayed title → mission key, from the `global.ini` and the missions the
/// catalog knows (`contracts._build_index` and `key_for`).
#[derive(Default)]
pub struct ContractIndex {
    exact: HashMap<String, String>,
    /// Every key of an exact title; only filled by [`ContractIndex::build_strict`].
    twins: HashMap<String, Vec<String>>,
    /// Pattern, key, weak, weight.
    patterns: Vec<(Regex, String, bool, usize)>,
}

impl ContractIndex {
    /// ⚠ The key can carry a text variant (`…_Title,P`); without stripping it
    /// the contract is not found, and a neighbour with a weaker pattern takes
    /// its place and shows the blueprints of another mission.
    pub fn build(ini_files: &[PathBuf], known: &HashSet<String>) -> ContractIndex {
        ContractIndex::build_with(ini_files, known, false)
    }

    /// Like [`ContractIndex::build`] over every key of the language files, in lower case: a title that stands for a
    /// key outside a caller's list too is seen as such. An exact title of two keys gives none from
    /// [`ContractIndex::key_for`], and [`ContractIndex::candidates`] gives all of them.
    pub fn build_strict(ini_files: &[PathBuf]) -> ContractIndex {
        ContractIndex::build_with(ini_files, &HashSet::new(), true)
    }

    fn build_with(ini_files: &[PathBuf], known: &HashSet<String>, strict: bool) -> ContractIndex {
        let mut index = ContractIndex::default();
        if known.is_empty() && !strict {
            return index;
        }
        for path in ini_files {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let text = decode_ignore(&bytes);
            for line in lines_universal(&text) {
                let Some(sep) = line.find('=').filter(|&s| s >= 1) else { continue };
                let folded = if strict { line[..sep].trim_start_matches('\u{feff}').to_lowercase() } else { String::new() };
                let mut key = if strict { folded.as_str() } else { &line[..sep] };
                if strict {
                    key = VARIANT.find(key).map_or(key, |m| &key[..m.start()]);
                } else if !known.contains(key) {
                    key = VARIANT.find(key).map_or(key, |m| &key[..m.start()]);
                    if !known.contains(key) {
                        continue;
                    }
                }
                let title = clean(&line[sep + 1..]);
                if title.is_empty() {
                    continue;
                }
                if title.contains("~mission(") {
                    let parts = pieces(&title);
                    let raw = format!("^{}$", parts.iter().map(|p| regex::escape(p)).collect::<Vec<_>>().join(".+"));
                    if let Ok(rx) = Regex::new(&raw) {
                        index.patterns.push((rx, key.to_string(), weak(&parts), weight(&parts)));
                    }
                } else {
                    index.exact.entry(title.to_lowercase()).or_insert_with(|| key.to_string());
                    if strict {
                        let keys = index.twins.entry(title.to_lowercase()).or_default();
                        if !keys.iter().any(|k| k == key) {
                            keys.push(key.to_string());
                        }
                    }
                }
            }
        }
        index
    }

    /// The mission key of a displayed title, or `None`.
    ///
    /// ⚠ Equally good patterns pointing at different missions give nothing:
    /// a made-up blueprint answer is worse than none.
    pub fn key_for(&self, title: &str) -> Option<String> {
        match self.candidates(title).as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }

    /// Every key a displayed title may stand for: the keys of the exact title, or else the keys of the best
    /// patterns it matches; empty for an unknown title.
    pub fn candidates(&self, title: &str) -> Vec<String> {
        let plain = clean(title);
        let lower = plain.to_lowercase();
        if let Some(keys) = self.twins.get(&lower) {
            return keys.clone();
        }
        if let Some(hit) = self.exact.get(&lower) {
            return vec![hit.clone()];
        }
        let matching: Vec<(&String, (bool, usize))> = self
            .patterns
            .iter()
            .filter(|(rx, ..)| rx.is_match(&plain))
            .map(|(_, key, weak, weight)| (key, (!weak, *weight)))
            .collect();
        let Some(rank) = matching.iter().map(|(_, r)| *r).max() else { return Vec::new() };
        let mut best: Vec<String> = Vec::new();
        for (key, _) in matching.iter().filter(|(_, r)| *r == rank) {
            if !best.contains(key) {
                best.push((*key).clone());
            }
        }
        best
    }
}

/// The missions of the catalog file with the names of their blueprints
/// (`missionen`, key → `bp`).
pub fn mission_blueprints(paths: &Paths) -> HashMap<String, Vec<String>> {
    let Ok(data) = jsonio::read_json(&crate::catalog_cache::current_file(paths)) else {
        return HashMap::new();
    };
    missions_of(&data)
}

/// [`mission_blueprints`] of a parsed catalog file.
pub fn missions_of(data: &Value) -> HashMap<String, Vec<String>> {
    let Some(Value::Object(missions)) = data.get("missionen") else { return HashMap::new() };
    missions
        .iter()
        .map(|(key, m)| {
            let names = match m.get("bp") {
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            (key.clone(), names)
        })
        .collect()
}

/// The contracts of the catalog file with the names of their blueprints
/// (`vertraege`, id → `bp`); empty for a catalog before that section existed.
pub fn contract_definitions(paths: &Paths) -> HashMap<String, Vec<String>> {
    let Ok(data) = jsonio::read_json(&crate::catalog_cache::current_file(paths)) else {
        return HashMap::new();
    };
    definitions_of(&data)
}

/// [`contract_definitions`] of a parsed catalog file.
pub fn definitions_of(data: &Value) -> HashMap<String, Vec<String>> {
    let Some(Value::Object(contracts)) = data.get("vertraege") else { return HashMap::new() };
    contracts
        .iter()
        .map(|(id, c)| {
            let names = match c.get("bp") {
                Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            (id.clone(), names)
        })
        .collect()
}

/// Everything needed to answer which blueprints a contract gives.
#[derive(Default)]
pub struct ContractBlueprints {
    pub index: ContractIndex,
    pub missions: HashMap<String, Vec<String>>,
    /// The contracts by their id, with their own list of blueprints.
    pub definitions: HashMap<String, Vec<String>>,
}

impl ContractBlueprints {
    /// Index and missions from the installation and the data folder.
    pub fn load(paths: &Paths) -> ContractBlueprints {
        ContractBlueprints::with_ini(paths, &ini_files(paths))
    }

    /// The same with the given language files instead of the installation's.
    pub fn with_ini(paths: &Paths, ini_files: &[PathBuf]) -> ContractBlueprints {
        let missions = mission_blueprints(paths);
        let known: HashSet<String> = missions.keys().cloned().collect();
        ContractBlueprints {
            index: ContractIndex::build(ini_files, &known),
            missions,
            definitions: contract_definitions(paths),
        }
    }

    /// What does this contract give, and what of it is still missing?
    /// `None` if the contract is unknown or gives no blueprints, else
    /// `(total, missing names)` (`contracts.check`).
    ///
    /// ⭐ The contract id beats the title: it hits exactly one contract with
    /// its own list instead of the one summed up over all regions. The title
    /// stays the fallback, not the replacement.
    pub fn check(
        &self,
        title: &str,
        has: &dyn Fn(&str) -> bool,
        contract_id: Option<&str>,
    ) -> Option<(usize, Vec<String>)> {
        let counted = |names: &[String]| -> Option<(usize, Vec<String>)> {
            let names: Vec<&String> = names.iter().filter(|n| !n.is_empty()).collect();
            if names.is_empty() {
                return None;
            }
            let missing = names.iter().filter(|n| !has(n.as_str())).map(|n| (*n).clone()).collect();
            Some((names.len(), missing))
        };
        if let Some(found) =
            contract_id.filter(|c| !c.is_empty()).and_then(|id| self.definitions.get(id)).and_then(|n| counted(n))
        {
            return Some(found);
        }
        let key = self.index.key_for(title)?;
        counted(self.missions.get(&key)?)
    }

    /// The keys of the blueprints this contract gives (`blueprints_for_contract`).
    ///
    /// Two ways, in this order: the title as the catalog's ways name it (the
    /// click in the own list), else over the mission key, which resolves
    /// placeholders („Stop Rival Attack at [LOCATION]“ in the catalog,
    /// „… at Asteroiden Bergbaubasis“ in the game).
    pub fn for_contract(&self, catalog: &CatalogCache, title: &str) -> BTreeSet<String> {
        let title = py_strip(title);
        if title.is_empty() {
            return BTreeSet::new();
        }
        let literal = literal_blueprints(catalog, title);
        if !literal.is_empty() {
            return literal;
        }
        let Some(key) = self.index.key_for(title) else { return BTreeSet::new() };
        let wanted: HashSet<String> =
            self.missions.get(&key).into_iter().flatten().filter(|n| !n.is_empty()).map(|n| name_key(n)).collect();
        catalog.blueprints.iter().filter(|b| wanted.contains(&b.key)).map(|b| b.key.clone()).collect()
    }
}

/// The keys of the blueprints whose ways name exactly this contract title.
pub fn literal_blueprints(catalog: &CatalogCache, title: &str) -> BTreeSet<String> {
    let title = py_strip(title);
    catalog
        .blueprints
        .iter()
        .filter(|b| b.ways.iter().any(|w| py_strip(&w.contract) == title))
        .map(|b| b.key.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pairs: &[(&str, Value)]) -> Map {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn s(text: &str) -> Value {
        Value::str(text)
    }

    #[test]
    fn clean_takes_out_every_form_of_the_mark() {
        assert_eq!(clean("Retake  Platforms <EM4>[BP!]</EM4>"), "Retake Platforms");
        assert_eq!(clean("<EM4>[BP]?</EM4> Rescue"), "Rescue");
        assert_eq!(clean("Hunt <EM4>[150 Rep] [BP]*</EM4>"), "Hunt");
        assert_eq!(clean("[SCBPW] 3 Baupläne [/SCBPW]Defend"), "Defend");
        assert_eq!(clean("Hilfe <EM4>[Bauplan]</EM4>"), "Hilfe");
        // Only a bubble with a blueprint mark goes; other emphasis stays.
        assert_eq!(clean("<EM4>[800 Rep]</EM4> Job"), "<EM4>[800 Rep]</EM4> Job");
        assert_eq!(clean(" a\u{1c}b\t c "), "a b c");
    }

    #[test]
    fn a_completed_run_never_falls_back_to_running() {
        let done = entry(&[
            ("name", s("X")),
            ("wann", s("2026-10-01T10:00:00")),
            ("zustand", s(COMPLETED)),
            ("bis", s("2026-10-01T11:00:00")),
        ]);
        let running = entry(&[
            ("name", s("X")),
            ("wann", s("2026-10-01T10:00:00")),
            ("zustand", s(RUNNING)),
            ("bis", s("")),
            ("quelle", s("b.log")),
        ]);
        let merged = merge(std::slice::from_ref(&done), &[running]);
        assert_eq!(merged.len(), 1);
        assert_eq!(state_of(&merged[0]), COMPLETED);
        assert_eq!(text_of(&merged[0], "bis"), "2026-10-01T11:00:00");
        // A missing field is filled in, behind the others.
        assert_eq!(text_of(&merged[0], "quelle"), "b.log");
        assert_eq!(merged[0].last().map(|(k, _)| k.as_str()), Some("quelle"));
        // An end found later completes the running entry.
        let running = entry(&[("name", s("X")), ("wann", s("2026-10-01T10:00:00")), ("zustand", s(RUNNING))]);
        let merged = merge(&[running], &[done]);
        assert_eq!(state_of(&merged[0]), COMPLETED);
    }

    #[test]
    fn merge_keeps_zero_and_empty_lists_like_python() {
        // `v not in (None, '')`: 0 and [] are copied over, None and '' are not.
        let a = entry(&[
            ("name", s("Y")),
            ("wann", s("t")),
            ("zustand", s(ABORTED)),
            ("ziele_fertig", Value::int(3)),
            ("bauplaene", Value::Array(vec![s("Z")])),
        ]);
        let b = entry(&[
            ("name", s("Y")),
            ("wann", s("t")),
            ("zustand", s(FAILED)),
            ("ziele_fertig", Value::int(0)),
            ("bauplaene", Value::Array(vec![])),
            ("bis", Value::Null),
            ("quelle", s("")),
        ]);
        let merged = merge(&[a], &[b]);
        assert_eq!(state_of(&merged[0]), FAILED);
        assert_eq!(get(&merged[0], "ziele_fertig"), Some(&Value::int(0)));
        assert_eq!(get(&merged[0], "bauplaene"), Some(&Value::Array(vec![])));
        assert!(get(&merged[0], "bis").is_none() && get(&merged[0], "quelle").is_none());
    }

    #[test]
    fn newest_first_and_equal_times_keep_their_order() {
        let e = |n: &str, w: &str| entry(&[("name", s(n)), ("wann", s(w))]);
        let merged = merge(&[e("a", "2026-01-01"), e("b", "2026-02-01"), e("c", "2026-01-01")], &[]);
        let names: Vec<&str> = merged.iter().map(|m| text_of(m, "name")).collect();
        assert_eq!(names, ["b", "a", "c"]);
    }

    #[test]
    fn cleaning_joins_a_title_saved_with_and_without_mark() {
        let marked = entry(&[("name", s("Z <EM4>[BP]</EM4>")), ("wann", s("t")), ("zustand", s(COMPLETED))]);
        let plain = entry(&[("name", s("Z")), ("wann", s("t")), ("zustand", s(RUNNING))]);
        let out = clean_titles(vec![plain, marked]);
        assert_eq!(out.len(), 1);
        assert_eq!(state_of(&out[0]), COMPLETED);
        // Nothing to clean: the list comes back untouched, not re-sorted.
        let untouched =
            vec![entry(&[("name", s("a")), ("wann", s("1"))]), entry(&[("name", s("b")), ("wann", s("2"))])];
        assert_eq!(clean_titles(untouched.clone()), untouched);
    }

    #[test]
    fn only_format_two_and_only_objects() {
        let file =
            |format: Value, items: Value| Value::Object(vec![("format".into(), format), ("auftraege".into(), items)]);
        let one = Value::Array(vec![Value::Object(entry(&[("name", s("a"))]))]);
        assert_eq!(entries_of(&file(Value::int(2), one.clone())).len(), 1);
        assert_eq!(entries_of(&file(Value::Float(2.0), one.clone())).len(), 1);
        assert!(entries_of(&file(Value::int(1), one.clone())).is_empty());
        assert!(entries_of(&file(s("2"), one)).is_empty());
        // One entry that is no object: Python's `e.get` raises, the page shows nothing.
        let broken = Value::Array(vec![Value::Object(entry(&[("name", s("a"))])), s("kaputt")]);
        assert!(entries_of(&file(Value::int(2), broken)).is_empty());
    }

    #[test]
    fn state_first_then_search_and_no_state_counts_as_running() {
        let entries = vec![
            entry(&[("name", s("Recover Vanduul Corpses")), ("zustand", s(COMPLETED))]),
            entry(&[("name", s("Recover Cargo"))]),
            entry(&[("name", s("Deliver"))]),
        ];
        assert_eq!(shown_at(&entries, None, "  RECOVER "), [0, 1]);
        assert_eq!(shown_at(&entries, Some(RUNNING), "recover"), [1]);
        assert_eq!(shown_at(&entries, Some(RUNNING), ""), [1, 2]);
        assert_eq!(shown_at(&entries, None, " ").len(), 3);
        assert_eq!(search(&entries, "deliver").len(), 1);
    }

    #[test]
    fn running_says_still_open_when_the_game_is_closed() {
        assert_eq!(state_word(RUNNING, true), "s_al_laeuft");
        assert_eq!(state_word(RUNNING, false), "s_al_offen");
        assert_eq!(state_word(EXPIRED, false), "s_al_verfallen");
        assert_eq!(state_word("unbekannt", false), "s_al_laeuft");
    }

    #[test]
    fn goals_only_for_running_contracts() {
        let m =
            Mission::of(&entry(&[("name", s("a")), ("ziele_gesamt", Value::int(5)), ("ziele_fertig", Value::int(2))]));
        assert!(m.shows_goals());
        let m = Mission::of(&entry(&[("zustand", s(COMPLETED)), ("ziele_gesamt", Value::int(5))]));
        assert!(!m.shows_goals());
    }

    #[test]
    fn the_exactest_pattern_wins_and_a_tie_says_nothing() {
        let dir = std::env::temp_dir().join(format!("versekit-auftraege-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(dir.join("english")).expect("folder");
        let ini = dir.join("english").join("global.ini");
        std::fs::write(
            &ini,
            "\u{feff}A_Title=Stop Rival Attack at ~mission(Location)\r\n\
             B_Title,P=Orange Lvl. Contract: ~mission(Item)\n\
             C_Title=Orange Lvl. Contract: Protect ~mission(Item) Now\n\
             D_Title=Plain Job <EM4>[BP]</EM4>\n\
             E_Title=Twin ~mission(X)\nF_Title=Twin ~mission(Y)\nG_Title=Unknown Job\n=leer\n",
        )
        .expect("ini");
        let known: HashSet<String> =
            ["A_Title", "B_Title", "C_Title", "D_Title", "E_Title", "F_Title"].iter().map(|s| s.to_string()).collect();
        let index = ContractIndex::build(std::slice::from_ref(&ini), &known);
        // ⚠ A byte order mark stays in front of the first key, which is then unknown.
        assert_eq!(index.key_for("Stop Rival Attack at Asteroiden Bergbaubasis"), None);
        let text = std::fs::read_to_string(&ini).expect("ini");
        std::fs::write(&ini, text.trim_start_matches('\u{feff}')).expect("ini");
        let index = ContractIndex::build(&[ini], &known);
        assert_eq!(index.key_for("Stop Rival Attack at Asteroiden Bergbaubasis").as_deref(), Some("A_Title"));
        // A placeholder in the middle beats a mere prefix.
        assert_eq!(index.key_for("Orange Lvl. Contract: Protect Tanks Now").as_deref(), Some("C_Title"));
        assert_eq!(index.key_for("Orange Lvl. Contract: Escort").as_deref(), Some("B_Title"));
        assert_eq!(index.key_for("PLAIN job").as_deref(), Some("D_Title"));
        assert_eq!(index.key_for("Twin Peaks"), None);
        assert_eq!(index.key_for("Unknown Job"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn weak_and_weight_as_python_counts_them() {
        assert!(weak(&pieces("Contract: ~mission(A)")));
        assert!(!weak(&pieces("~mission(A) at base")));
        assert!(!weak(&pieces("No placeholder")));
        assert_eq!(weight(&pieces("Ab ~mission(A) c\\d")), 3 + 3);
    }

    #[test]
    fn summarize_counts_runs_and_completions() {
        let entries = vec![
            entry(&[("name", s("a")), ("zustand", s(COMPLETED))]),
            entry(&[("name", s("b")), ("zustand", s(ABORTED))]),
            entry(&[("name", s("a")), ("zustand", s(FAILED))]),
        ];
        assert_eq!(summarize(&entries), vec![("a".to_string(), (2, 1)), ("b".to_string(), (1, 0))]);
    }
}
