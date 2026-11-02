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

//! Rust side of the check for learning the blueprint phrase (`tools/parity_phrases_learn.py`):
//! `phrases::find_self` on the same files and names, `Sources::remember` on the same `phrasen.json`
//! written anew in a folder of its own.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use versekit_core::phrases::{Sources, find_self};

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2).ok_or("Hex ungerade")?, 16).map_err(|e| e.to_string()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The file as the Python side writes it down: missing (`null`), a folder (`DIR`) or its bytes in hex.
fn state(path: &Path) -> Value {
    if path.is_dir() {
        return json!("DIR");
    }
    fs::read(path).map_or(Value::Null, |bytes| json!(hex(&bytes)))
}

/// `gegenprobe`: the first phrase found is given back changed, and the first phrase kept is not kept.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let mut finds = Vec::new();
    let mut changed = !gegenprobe;
    for case in input["find"].as_array().ok_or("find fehlt")? {
        let at_most = case["at_most"].as_u64().ok_or("at_most fehlt")? as usize;
        let mut result = find_self(&strings(&case["names"]), &strings(&case["files"]), at_most);
        if !changed && let Some(found) = &mut result {
            found.push_str(" gegenprobe");
            changed = true;
        }
        finds.push(json!({ "result": result }));
    }
    // A folder of its own for each run: a file left by an earlier one would change what is kept.
    let run_name = Path::new(out).file_stem().ok_or("Ausgabe ohne Namen")?;
    let scratch = Path::new(input["rust_scratch"].as_str().ok_or("rust_scratch fehlt")?).join(run_name);
    if scratch.exists() {
        return Err(format!("{} gibt es schon: frische Ausgabedatei nehmen", scratch.display()));
    }
    let mut kept = Vec::new();
    let mut skipped = !gegenprobe;
    for (number, case) in input["remember"].as_array().ok_or("remember fehlt")?.iter().enumerate() {
        let folder = scratch.join(format!("{number:03}"));
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let target = folder.join("phrasen.json");
        match &case["before"] {
            Value::Null => {}
            Value::String(s) if s == "DIR" => fs::create_dir_all(&target).map_err(|e| e.to_string())?,
            Value::String(s) => fs::write(&target, unhex(s)?).map_err(|e| e.to_string())?,
            other => return Err(format!("before: {other}")),
        }
        let phrase = case["phrase"].as_str().ok_or("phrase fehlt")?;
        let sources = Sources { phrases_file: Some(target.clone()), localization_folder: None };
        let result = if !skipped && case["result"].as_bool() == Some(true) {
            skipped = true;
            false
        } else {
            sources.remember(phrase)
        };
        kept.push(json!({ "result": result, "after": state(&target) }));
    }
    fs::write(out, serde_json::to_string(&json!({ "find": finds, "remember": kept })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("find_self {} Fälle, remember {} Fälle", finds.len(), kept.len());
    Ok(())
}
