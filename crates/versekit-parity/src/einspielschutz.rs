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

//! Rust side of the check for the protection when a backup is restored (`tools/parity_einspielschutz.py`):
//! the same pairs (settings of the backup, settings here) through `start_programs::safe_import`.

use std::fs;

use serde_json::{Value as Json, json};
use versekit_core::jsonio::{self, Value};

/// `gegenprobe`: every entry of a backup stays as it came (nothing is switched off).
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Json = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let mut done = Vec::with_capacity(cases.len());
    for case in cases {
        let (incoming, before) = (object(&case[0])?, object(&case[1])?);
        let mut after = incoming.clone();
        let off = if gegenprobe { Vec::new() } else { versekit_core::start_programs::safe_import(&mut after, &before) };
        let after = serde_json::from_str::<Json>(&jsonio::dumps(&Value::Object(after), None, false))
            .map_err(|e| e.to_string())?;
        done.push(json!([case[0], case[1], [after, off]]));
    }
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}

/// A JSON object of the file as the program reads it.
fn object(json: &Json) -> Result<jsonio::Map, String> {
    match jsonio::loads(&json.to_string()).map_err(|e| format!("{e:?}"))? {
        Value::Object(map) => Ok(map),
        _ => Err("Einstellungen sind kein Objekt".to_string()),
    }
}
