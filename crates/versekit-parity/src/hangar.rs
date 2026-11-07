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

//! Rust side of the parity check for the ship list `hangar-schiffe`
//! (`tools/parity_hangar.py`): replays every scenario on its own files and
//! writes entries, counts and file states in the same form.
//!
//! `--gegenprobe` drops the last read entry of every successful import.

use std::fs;

use versekit_core::hangar::{self, Rejected, ShipNames};
use versekit_core::hangar_inbox as inbox;
use versekit_core::jsonio::{self, Map, Newline, Value};

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match value.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt")),
    }
}

/// `fleet.read` for a file holding a JSON object: entries or an error class.
fn read_list(path: &str, names: &ShipNames) -> Result<Vec<Map>, &'static str> {
    let bytes = fs::read(path).map_err(|_| "fehler")?;
    let text = String::from_utf8(bytes).map_err(|_| "fehler")?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let raw = jsonio::loads(text).map_err(|_| "fehler")?;
    hangar::read_object(&raw, names).map_err(|r| match r {
        Rejected::Version => "version",
        Rejected::Transfer => "uebergabe",
    })
}

/// `versekit-parity einlieferung <hangar.json> <port> <token> [--aus]`: the
/// receiver on its own, to put the same requests to it as to the Python
/// test receiver. Runs until it is stopped.
pub fn serve(hangar_file: &str, port: &str, token: &str, off: bool) -> Result<(), String> {
    let port: u16 = port.parse().map_err(|_| "Port unlesbar".to_string())?;
    let listener = inbox::bind(port).map_err(|e| e.to_string())?;
    let gate = inbox::Gate { enabled: !off, token: token.to_string(), ready: true };
    let files = inbox::Files { hangar: hangar_file.to_string(), ship_catalog: String::new() };
    println!("Lauscht auf http://127.0.0.1:{port}{}", inbox::PATH);
    inbox::serve(listener, inbox::Inbox::new(gate), files, versekit_core::writeguard::state(), || {});
    Ok(())
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let folder = text(&input, "rust_folder")?.to_string();
    let lists = input.get("lists").ok_or("lists fehlt")?;
    let catalog_file = text(&input, "catalog_file")?;
    let mut scenarios = Vec::new();
    for scenario in list(&input, "scenarios")? {
        let name = text(scenario, "name")?;
        let names = if matches!(scenario.get("catalog"), Some(Value::Bool(true))) {
            ShipNames::load(catalog_file)
        } else {
            ShipNames::default()
        };
        let base = format!("{folder}/{name}");
        jsonio::make_dirs(&base).map_err(|e| e.to_string())?;
        let target = format!("{base}/hangar.json");
        if let Some(start) = scenario.get("start").and_then(Value::as_str) {
            jsonio::write_text_atomic(&target, start, Newline::Native).map_err(|e| e.to_string())?;
        }
        let mut data = hangar::load(&target);
        let mut snaps = Vec::new();
        let snap = |snaps: &mut Vec<Value>| -> Result<(), String> {
            let file = format!("{base}/stand-{}.json", snaps.len());
            if std::path::Path::new(&target).exists() {
                fs::copy(&target, &file).map_err(|e| e.to_string())?;
                snaps.push(Value::str(file));
            } else {
                snaps.push(Value::str(""));
            }
            Ok(())
        };
        snap(&mut snaps)?;
        let mut steps = Vec::new();
        for step in list(scenario, "steps")? {
            let list_name = text(step, "list")?;
            let path = lists.get(list_name).and_then(Value::as_str).ok_or("Liste fehlt")?;
            let (error, entries) = match read_list(path, &names) {
                Ok(entries) => ("", entries),
                Err(mark) => (mark, Vec::new()),
            };
            let mut result = Value::Null;
            if error.is_empty() && !entries.is_empty() {
                let mut taken = entries.clone();
                if gegenprobe {
                    taken.pop();
                }
                let done = hangar::import_entries(&mut data, &taken);
                if !hangar::save(&target, &mut data) {
                    return Err(format!("{target} nicht gespeichert"));
                }
                result = Value::Array(vec![
                    Value::int(done.new as i64),
                    Value::int(done.existing as i64),
                    Value::Array(done.removed.into_iter().map(Value::str).collect()),
                ]);
            }
            let step_out: Map = vec![
                ("list".into(), Value::str(list_name)),
                ("error".into(), Value::str(error)),
                ("entries".into(), Value::Array(entries.into_iter().map(Value::Object).collect())),
                ("result".into(), result),
            ];
            steps.push(Value::Object(step_out));
            snap(&mut snaps)?;
        }
        let scenario_out: Map = vec![
            ("name".into(), Value::str(name)),
            ("steps".into(), Value::Array(steps)),
            ("snaps".into(), Value::Array(snaps)),
        ];
        scenarios.push(Value::Object(scenario_out));
    }
    let result: Map = vec![("scenarios".into(), Value::Array(scenarios))];
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
