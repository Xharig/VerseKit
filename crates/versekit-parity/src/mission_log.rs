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

//! Rust side of the parity check for the mission log page
//! (`tools/parity_mission_log.py`): loads the log from the scratch folder,
//! replays the made-up files, merges, searches, and resolves every title to
//! its mission key, blueprints and Issue Council search.

use std::collections::HashSet;
use std::path::PathBuf;

use versekit_core::issue_council::{self, Originals, Search};
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::mission_log::{self as log, ContractBlueprints, ContractIndex};
use versekit_core::paths::{Host, Paths};
use versekit_core::{catalog_cache, legacy};

fn maps(value: Option<&Value>) -> Vec<Map> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_object().cloned()).collect(),
        _ => Vec::new(),
    }
}

fn list(entries: Vec<Map>) -> Value {
    Value::Array(entries.into_iter().map(Value::Object).collect())
}

fn text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, Value::str)
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let home = input.get("home").and_then(Value::as_str).ok_or("home fehlt")?;
    let mut host = Host::current();
    host.registry = false;
    host.set_var(legacy::ENV_HOME, home);
    let paths = Paths::new(host);

    let loaded = log::load(&paths);
    let files: Vec<Value> = match input.get("files") {
        Some(Value::Array(cases)) => {
            cases.iter().map(|c| list(c.get("file").map_or_else(Vec::new, log::entries_of))).collect()
        }
        _ => Vec::new(),
    };
    let merges: Vec<Value> = match input.get("merges") {
        Some(Value::Array(cases)) => {
            cases.iter().map(|c| list(log::merge(&maps(c.get("old")), &maps(c.get("new"))))).collect()
        }
        _ => Vec::new(),
    };
    let cleaned = log::clean_titles(maps(input.get("marked")));
    let searches: Vec<Value> = match input.get("searches") {
        Some(Value::Array(cases)) => cases
            .iter()
            .map(|c| {
                let wanted = c.get("text").and_then(Value::as_str).unwrap_or_default();
                let hits = log::search(&loaded, wanted);
                Value::Array(
                    hits.iter()
                        .filter_map(|h| loaded.iter().position(|e| std::ptr::eq(e, *h)))
                        .map(|i| Value::int(i as i64))
                        .collect(),
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    let summary: Vec<Value> = log::summarize(&loaded)
        .into_iter()
        .map(|(name, (total, done))| {
            Value::Array(vec![Value::str(name), Value::int(total as i64), Value::int(done as i64)])
        })
        .collect();

    // The same language files as the Python side, the copied catalog and titles.
    let inis: Vec<PathBuf> = match input.get("ini_files") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(PathBuf::from).collect(),
        _ => Vec::new(),
    };
    let missions = log::mission_blueprints(&paths);
    let known: HashSet<String> = missions.keys().cloned().collect();
    let resolver = ContractBlueprints {
        index: ContractIndex::build(&inis, &known),
        missions,
        definitions: log::contract_definitions(&paths),
    };
    let mut catalog = catalog_cache::load(&paths).unwrap_or_default();
    if !paths.setting_bool("alle_bauplaene", false) {
        catalog.blueprints.retain(|b| !b.no_way);
    }
    let search = Search::new(Originals::load(&paths), issue_council::local_texts(&inis));
    let titles: Vec<Value> = match input.get("titles") {
        Some(Value::Array(cases)) => cases
            .iter()
            .map(|c| {
                let title = c.get("title").and_then(Value::as_str).unwrap_or_default();
                let blueprints = resolver.for_contract(&catalog, title).into_iter().map(Value::str).collect();
                Value::Object(vec![
                    ("title".into(), Value::str(title)),
                    ("clean".into(), Value::str(log::clean(title))),
                    ("key".into(), text(resolver.index.key_for(title).as_deref())),
                    ("blueprints".into(), Value::Array(blueprints)),
                    ("ic_clean".into(), Value::str(issue_council::clean(title))),
                    ("search".into(), Value::str(search.search_text(title))),
                    ("url".into(), Value::str(search.url_for(title))),
                ])
            })
            .collect(),
        _ => Vec::new(),
    };

    let mut result = Value::Object(vec![
        ("log".into(), list(loaded)),
        ("files".into(), Value::Array(files)),
        ("merges".into(), Value::Array(merges)),
        ("cleaned".into(), list(cleaned)),
        ("searches".into(), Value::Array(searches)),
        ("summary".into(), Value::Array(summary)),
        ("titles".into(), Value::Array(titles)),
    ]);
    if gegenprobe {
        spoil(&mut result);
    }
    std::fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())
}

/// The value no part ever holds.
const FOREIGN: &str = "gegenprobe";

fn part<'a>(result: &'a mut Value, name: &str) -> Option<&'a mut Vec<Value>> {
    match result {
        Value::Object(map) => match map.iter_mut().find(|(k, _)| k == name) {
            Some((_, Value::Array(items))) => Some(items),
            _ => None,
        },
        _ => None,
    }
}

fn set_in_first(items: &mut [Value], key: &str) {
    if let Some(Value::Object(entry)) = items.first_mut() {
        jsonio::map_set(entry, key, Value::str(FOREIGN));
    }
}

/// The counter-check: every part spoilt on its own, so each has to show up
/// in the comparison.
fn spoil(result: &mut Value) {
    if let Some(items) = part(result, "log") {
        set_in_first(items, "zustand");
    }
    if let Some(cases) = part(result, "files")
        && let Some(Value::Array(entries)) = cases.iter_mut().find(|c| matches!(c, Value::Array(e) if !e.is_empty()))
    {
        set_in_first(entries, "zustand");
    }
    if let Some(cases) = part(result, "merges")
        && let Some(Value::Array(entries)) = cases.first_mut()
    {
        set_in_first(entries, "zustand");
    }
    if let Some(items) = part(result, "cleaned") {
        set_in_first(items, "name");
    }
    if let Some(cases) = part(result, "searches")
        && let Some(Value::Array(hits)) = cases.first_mut()
    {
        hits.push(Value::int(-1));
    }
    if let Some(items) = part(result, "summary")
        && let Some(Value::Array(row)) = items.first_mut()
        && let Some(name) = row.first_mut()
    {
        *name = Value::str(FOREIGN);
    }
    // One title with a mission key gets every field of its own spoilt.
    if let Some(titles) = part(result, "titles")
        && let Some(Value::Object(title)) =
            titles.iter_mut().find(|t| matches!(t.get("key"), Some(Value::Str(k)) if !k.is_empty()))
    {
        for key in ["clean", "key", "ic_clean", "search", "url"] {
            jsonio::map_set(title, key, Value::str(FOREIGN));
        }
        if let Some((_, Value::Array(blueprints))) = title.iter_mut().find(|(k, _)| k == "blueprints") {
            blueprints.push(Value::str(FOREIGN));
        }
    }
}
