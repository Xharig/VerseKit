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

//! Rust side of the check for starting the game (`tools/parity_start.py`): the same texts split by
//! `game_start::shlex_split` in both modes.

use std::fs;

use serde_json::{Value, json};
use versekit_core::game_start;

/// `gegenprobe`: a quote that is not closed is not an error but the rest of the text as a piece.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let mut done = Vec::with_capacity(cases.len());
    for case in cases {
        let (text, posix) = (case[0].as_str().ok_or("Text fehlt")?, case[1].as_bool().ok_or("posix fehlt")?);
        let mut pieces = game_start::shlex_split(text, posix);
        if gegenprobe && pieces.is_none() {
            pieces = Some(vec![text.to_string()]);
        }
        done.push(json!([text, posix, pieces]));
    }
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}
