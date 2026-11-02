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

//! Rust side of the parity check for the hangar page
//! (`tools/parity_hangar_save.py`): reads every export file, replays the
//! steps of two pages on own files and looks up slots and UEX names, and
//! writes all of it in the same form.
//!
//! Hangar code may only write below `rust_folder` (write lock); after a
//! step in which Python merged, the file is written once with sorted keys,
//! as on the Python side.
//!
//! `--gegenprobe format|merge|schiffe`, each on its own: drop the last entry
//! of every read file, save without merging, count one slot more.

use std::collections::HashMap;
use std::fs;

use versekit_core::hangar::{self, Held, Rejected, ShipNames};
use versekit_core::hangar_formats::{self, ReadFail};
use versekit_core::jsonio::{self, Map, Newline, Value, map_set};
use versekit_core::ships::{ErkulShips, ShipId, UexShips};
use versekit_core::writeguard;

/// Which counter check is on.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counter {
    drop_last: bool,
    no_merge: bool,
    one_slot_more: bool,
}

impl Counter {
    /// From the word after `--gegenprobe`; none for the real run.
    pub fn parse(word: Option<&str>) -> Result<Counter, String> {
        let mut counter = Counter::default();
        match word {
            None => {}
            Some("format") => counter.drop_last = true,
            Some("merge") => counter.no_merge = true,
            Some("schiffe") => counter.one_slot_more = true,
            Some(other) => return Err(format!("unbekannte Gegenprobe {other}: format, merge oder schiffe")),
        }
        Ok(counter)
    }
}

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match value.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt")),
    }
}

fn flag(value: &Value, name: &str) -> bool {
    matches!(value.get(name), Some(Value::Bool(true)))
}

fn index(value: &Value) -> Result<usize, String> {
    match value.get("index") {
        Some(Value::Int(n)) => n.parse().map_err(|_| "index unlesbar".to_string()),
        _ => Err("index fehlt".into()),
    }
}

fn arr(items: Vec<Value>) -> Value {
    Value::Array(items)
}

/// Entries and error class, as `error_class(fleet.read(…))`.
fn read(path: &str, names: &ShipNames, counter: Counter) -> (&'static str, Vec<Map>) {
    match hangar_formats::read_file(path, names) {
        Ok(mut entries) => {
            if counter.drop_last {
                entries.pop();
            }
            ("", entries)
        }
        Err(ReadFail::Rejected(Rejected::Version)) => ("version", Vec::new()),
        Err(ReadFail::Rejected(Rejected::Transfer)) => ("uebergabe", Vec::new()),
        Err(ReadFail::Error(_)) => ("fehler", Vec::new()),
    }
}

fn save(target: &str, held: &mut Held, counter: Counter) -> bool {
    if counter.no_merge { hangar::save(target, &mut held.data) } else { hangar::save_held(target, held).ok() }
}

/// The file once more with sorted keys (`sort_keys` of the Python side).
fn sort_keys(target: &str) -> Result<(), String> {
    let data = jsonio::read_json(target).map_err(|e| e.to_string())?;
    jsonio::write_text_atomic(target, &jsonio::dumps(&data, Some(1), true), Newline::Native).map_err(|e| e.to_string())
}

/// One step as `run_step` of the Python side does it.
fn run_step(
    step: &Value,
    pages: &mut HashMap<String, Held>,
    target: &str,
    lists: &Value,
    names: &ShipNames,
    counter: Counter,
) -> Result<Value, String> {
    let op = text(step, "op")?;
    if op == "raw" {
        fs::write(target, text(step, "text")?).map_err(|e| e.to_string())?;
        return Ok(Value::Null);
    }
    let who = text(step, "who")?.to_string();
    if op == "load" {
        pages.insert(who, hangar::load_held(target));
        return Ok(Value::Null);
    }
    let held = pages.get_mut(&who).ok_or("Seite nicht geladen")?;
    if op == "changed" {
        return Ok(Value::Bool(hangar::changed_elsewhere(target, held)));
    }
    let count = hangar::ships(&held.data).len();
    if step.get("index").is_some() && index(step)? >= count {
        return Ok(Value::Null);
    }
    let saved = |held: &mut Held| Value::Bool(save(target, held, counter));
    Ok(match op {
        "add" => {
            let name = text(step, "name")?;
            let new = hangar::add_by_hand(&mut held.data, name, flag(step, "paid"), flag(step, "lti"));
            arr(vec![Value::Bool(new), if new { saved(held) } else { Value::Null }])
        }
        "remove" => {
            let entry = hangar::ships(&held.data)[index(step)?];
            let (name, maker) =
                (hangar::field_text(entry, "name").to_string(), hangar::field_text(entry, "hersteller").to_string());
            let gone = hangar::remove(&mut held.data, &name, &maker);
            arr(vec![Value::Bool(gone), if gone { saved(held) } else { Value::Null }])
        }
        "ready" => {
            let entry = hangar::ship_mut(&mut held.data, index(step)?).ok_or("kein Schiff")?;
            map_set(entry, "einsatzbereit", Value::Bool(flag(step, "on")));
            saved(held)
        }
        "melt" => {
            let entry = hangar::ship_mut(&mut held.data, index(step)?).ok_or("kein Schiff")?;
            if hangar::set_melt_choice(entry, text(step, "value")?) { saved(held) } else { Value::Null }
        }
        "slot" => {
            let entry = hangar::ship_mut(&mut held.data, index(step)?).ok_or("kein Schiff")?;
            let value = step.get("value").cloned().unwrap_or(Value::Null);
            match entry.iter_mut().find(|(k, _)| k == "belegung") {
                Some((_, Value::Object(slots))) => map_set(slots, text(step, "path")?, value),
                Some(_) => return Err("belegung ist kein Objekt".into()),
                None => map_set(entry, "belegung", Value::Object(vec![(text(step, "path")?.to_string(), value)])),
            }
            saved(held)
        }
        "wish" => {
            let new = hangar::wishlist_add(&mut held.data, text(step, "name")?, "");
            arr(vec![Value::Bool(new), saved(held)])
        }
        "top" => {
            map_set(&mut held.data, text(step, "key")?, step.get("value").cloned().unwrap_or(Value::Null));
            saved(held)
        }
        "import" => {
            let path = lists.get(text(step, "list")?).and_then(Value::as_str).ok_or("Liste fehlt")?;
            let (error, entries) = read(path, names, counter);
            if !error.is_empty() || entries.is_empty() {
                return Ok(arr(vec![Value::str(error), Value::int(entries.len() as i64)]));
            }
            let done = hangar::import_entries(&mut held.data, &entries);
            save(target, held, counter);
            arr(vec![
                Value::int(done.new as i64),
                Value::int(done.existing as i64),
                arr(done.removed.into_iter().map(Value::str).collect()),
            ])
        }
        other => return Err(format!("unbekannter Schritt {other}")),
    })
}

pub fn run(python_json: &str, out: &str, counter: Counter) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let folder = text(&input, "rust_folder")?.to_string();
    writeguard::deny(true);
    writeguard::allow_under(Some(&folder));
    let result = replay(&input, &folder, counter);
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    if !blocked.is_empty() {
        return Err(format!("Schreibsperre: außerhalb von {folder} geschrieben: {}", blocked.join(", ")));
    }
    fs::write(out, jsonio::dumps(&result?, Some(1), false)).map_err(|e| e.to_string())
}

/// Everything the Python side did, on own files below `folder`.
fn replay(input: &Value, folder: &str, counter: Counter) -> Result<Value, String> {
    let lists = input.get("lists").ok_or("lists fehlt")?;
    let catalog_file = text(input, "catalog_file")?;
    let names_for = |catalog: bool| if catalog { ShipNames::load(catalog_file) } else { ShipNames::default() };

    let mut formats = Vec::new();
    for case in list(input, "formats")? {
        let name = text(case, "list")?;
        let path = lists.get(name).and_then(Value::as_str).ok_or("Liste fehlt")?;
        let (error, entries) = read(path, &names_for(flag(case, "catalog")), counter);
        formats.push(Value::Object(vec![
            ("list".into(), Value::str(name)),
            ("catalog".into(), Value::Bool(flag(case, "catalog"))),
            ("error".into(), Value::str(error)),
            ("entries".into(), arr(entries.into_iter().map(Value::Object).collect())),
        ]));
    }

    let steps = list(input, "steps")?;
    let mut runs = Vec::new();
    for run in list(input, "runs")? {
        let name = text(run, "name")?;
        let names = names_for(flag(run, "catalog"));
        let merged = list(run, "merged")?;
        let base = format!("{folder}/{name}");
        jsonio::make_dirs(&base).map_err(|e| e.to_string())?;
        let target = format!("{base}/hangar.json");
        if let Some(start) = run.get("start").and_then(Value::as_str) {
            jsonio::write_text_atomic(&target, start, Newline::Native).map_err(|e| e.to_string())?;
        }
        let mut pages = HashMap::new();
        let (mut results, mut snaps) = (Vec::new(), Vec::new());
        for (i, step) in steps.iter().enumerate() {
            results.push(run_step(step, &mut pages, &target, lists, &names, counter)?);
            if std::path::Path::new(&target).exists() {
                let file = format!("{base}/stand-{i}.json");
                fs::copy(&target, &file).map_err(|e| e.to_string())?;
                snaps.push(Value::str(file));
                if matches!(merged.get(i), Some(Value::Bool(true))) {
                    sort_keys(&target)?;
                }
            } else {
                snaps.push(Value::str(""));
            }
        }
        runs.push(Value::Object(vec![
            ("name".into(), Value::str(name)),
            ("results".into(), arr(results)),
            ("snaps".into(), arr(snaps)),
        ]));
    }

    let erkul = ErkulShips::load(text(input, "erkul_file")?);
    let uex = UexShips::load(catalog_file);
    let python_ships = input.get("ships").ok_or("ships fehlt")?;
    let mut slots = Vec::new();
    for probe in list(python_ships, "slots")? {
        let ship = list(probe, "ship")?;
        let part = |i: usize| ship.get(i).and_then(Value::as_str).unwrap_or("");
        let id = ShipId { name: part(0), maker: part(1), short: part(2), maker_short: part(3) };
        let count = erkul.slot_count(id).map(|n| if counter.one_slot_more { n + 1 } else { n });
        slots.push(Value::Object(vec![
            ("ship".into(), arr(ship.to_vec())),
            ("slots".into(), count.map_or(Value::Null, Value::int)),
            ("concept".into(), Value::Bool(uex.is_concept(part(0)))),
            ("knows".into(), Value::Bool(uex.knows(part(0)))),
        ]));
    }
    let ships = Value::Object(vec![
        ("version".into(), Value::str(erkul.game_version())),
        ("all_names".into(), arr(uex.all_names().into_iter().map(Value::str).collect())),
        ("slots".into(), arr(slots)),
    ]);
    Ok(Value::Object(vec![("formats".into(), arr(formats)), ("runs".into(), arr(runs)), ("ships".into(), ships)]))
}
