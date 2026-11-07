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

//! Rust side of the check for the controls (`tools/parity_joysticks.py`): log texts, `actionmaps.xml` and
//! scratch game folders through `joysticks::{devices_from_text, assignment, bindings, compare, devices}`.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use versekit_core::device_hub::{self, Status, Step};
use versekit_core::input_device::SystemDevice;
use versekit_core::joysticks::{self, Assigned, Device};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

/// `gegenprobe`: the bindings are sorted by their text instead of axes first and buttons by number, and a
/// replacement is also named where several devices are missing or new.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let folder = std::env::temp_dir().join(format!("versekit-parity-joysticks-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let home = folder.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &home)], &home));
    let mut done = Vec::with_capacity(cases.len());
    let result = (|| -> Result<(), String> {
        for (number, case) in cases.iter().enumerate() {
            let kind = case[0].as_str().ok_or("Art fehlt")?;
            let answer = match kind {
                "geraete_text" => pairs(&joysticks::devices_from_text(case[1].as_str().ok_or("Text fehlt")?)),
                "zuordnung" => {
                    let file = write_file(&folder, case[1].as_str().ok_or("Text fehlt")?)?;
                    stored(&joysticks::assignment(&paths, Some(&file), None))
                }
                "belegung" => {
                    let file = write_file(&folder, case[1].as_str().ok_or("Text fehlt")?)?;
                    bindings(&joysticks::bindings(&paths, Some(&file), None), gegenprobe)
                }
                "vergleich" => compare(&paths, &folder, number, &case[1], gegenprobe)?,
                "hub" => hub(&paths, &folder, number, &case[1], gegenprobe)?,
                "kopf" => {
                    let game = folder.join(format!("k{number}"));
                    put(&game.join("Game.log"), case[1].as_str().ok_or("Text fehlt")?)?;
                    pairs(&joysticks::devices(&paths, Some(&game.to_string_lossy())))
                }
                other => return Err(format!("unbekannte Art {other}")),
            };
            let mut row = case.as_array().cloned().unwrap_or_default();
            let last = row.len() - 1;
            row[last] = answer;
            done.push(Value::Array(row));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&folder);
    result?;
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}

fn put(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_file(folder: &Path, text: &str) -> Result<String, String> {
    let path = folder.join("actionmaps.xml");
    put(&path, text)?;
    Ok(path.to_string_lossy().into_owned())
}

/// The hub over a scratch game folder with the sticks of the system put in by hand.
fn hub(paths: &Paths, folder: &Path, number: usize, case: &Value, gegenprobe: bool) -> Result<Value, String> {
    let game = make_game(folder, &format!("h{number}"), case)?;
    let system: Vec<SystemDevice> = case["system"]
        .as_array()
        .ok_or("System fehlt")?
        .iter()
        .map(|d| SystemDevice {
            path: d[0].as_str().unwrap_or_default().to_string(),
            name: d[1].as_str().unwrap_or_default().to_string(),
            id: d[2].as_str().unwrap_or_default().to_string(),
        })
        .collect();
    let result = device_hub::summary(paths, &system, Some(&game.to_string_lossy()), None);
    let mut steps: Vec<(String, String, String)> = device_hub::suggestions_of(&result.devices)
        .iter()
        .map(|s| {
            let art = match s.step {
                Step::Swap => "tausch",
                Step::Start => "starten",
                Step::PlugIn => "anstecken",
            };
            (art.to_string(), s.device.id.clone(), s.old.as_ref().map_or(String::new(), |o| o.id.clone()))
        })
        .collect();
    if gegenprobe {
        // Guess: one missing and any number of new ones gives a swap with the first of them.
        let missing: Vec<_> = result.devices.iter().filter(|d| d.status == Status::Unplugged).collect();
        let fresh: Vec<_> =
            result.devices.iter().filter(|d| matches!(d.status, Status::NoNumber | Status::Unknown)).collect();
        if missing.len() == 1 && !fresh.is_empty() {
            steps = vec![("tausch".to_string(), fresh[0].id.clone(), missing[0].id.clone())];
        }
    }
    steps.sort();
    let steps: Vec<Value> = steps.into_iter().map(|(art, id, old)| json!([art, id, old])).collect();
    let mut rows: Vec<&device_hub::Entry> = result.devices.iter().collect();
    rows.sort_by_key(|d| (d.number.is_none(), d.number.unwrap_or(0), d.name.to_lowercase(), d.id.clone()));
    let rows: Vec<Value> = rows
        .iter()
        .map(|d| {
            json!([
                d.id,
                d.short,
                d.name,
                d.status.word(),
                d.number,
                d.system_path,
                d.system_name,
                d.connected,
                d.in_game
            ])
        })
        .collect();
    Ok(json!([rows, [result.ready, result.no_number, result.unplugged, result.unknown], result.all_well, steps]))
}

fn pairs(devices: &[Device]) -> Value {
    Value::Array(devices.iter().map(|d| json!([d.slot, d.name, d.id])).collect())
}

fn stored(items: &[Assigned]) -> Value {
    Value::Array(items.iter().map(|z| json!([z.number, z.name, z.id])).collect())
}

fn bindings(all: &[(String, Vec<joysticks::Binding>)], gegenprobe: bool) -> Value {
    Value::Array(
        all.iter()
            .map(|(id, entries)| {
                let mut entries = entries.clone();
                if gegenprobe {
                    entries.sort_by(|a, b| a.input.cmp(&b.input));
                }
                let rows: Vec<Value> =
                    entries.iter().map(|e| json!([e.input, e.action, e.section, e.kind.word()])).collect();
                json!([id, rows])
            })
            .collect(),
    )
}

/// The scratch game folder of a case: `Game.log`, kept logs, `actionmaps.xml`.
fn make_game(folder: &Path, tag: &str, case: &Value) -> Result<std::path::PathBuf, String> {
    let game = folder.join(tag);
    if let Some(log) = case["log"].as_str() {
        put(&game.join("Game.log"), log)?;
    }
    for (index, text) in case["sicherungen"].as_array().ok_or("Sicherungen fehlen")?.iter().enumerate() {
        let path = game.join("logbackups").join(format!("Game Build({index}).log"));
        put(&path, text.as_str().ok_or("Text fehlt")?)?;
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + index as u64 * 100);
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(at))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if let Some(xml) = case["xml"].as_str() {
        put(&game.join("USER").join("Client").join("0").join("Profiles").join("default").join("actionmaps.xml"), xml)?;
    }
    Ok(game)
}

fn compare(paths: &Paths, folder: &Path, number: usize, case: &Value, gegenprobe: bool) -> Result<Value, String> {
    let game = make_game(folder, &format!("g{number}"), case)?;
    let result = joysticks::compare(paths, Some(&game.to_string_lossy()), None);
    let replacement = match (&result.replacement, gegenprobe) {
        (Some(pair), _) => Some(pair.clone()),
        (None, true) if !result.missing.is_empty() && !result.new.is_empty() => {
            Some((result.missing[0].clone(), result.new[0].clone()))
        }
        _ => None,
    };
    Ok(json!([
        result.state.word(),
        pairs(&result.devices),
        stored(&result.stored),
        Value::Array(result.missing.iter().map(|z| json!([z.number, z.id])).collect()),
        pairs(&result.new),
        Value::Array(replacement.iter().map(|(a, n)| json!([[a.number, a.id], [n.slot, n.id]])).collect()),
    ]))
}
