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

//! Rust side of the check for the own blueprint corrections: for each scenario of the Python side the same file
//! is read and the same names are asked for size, grade and class, as `_size_grade_class` gives them without scmdb
//! data, and for the short code of 4.12 (`M/A/1`). With `--gegenprobe` the first answer with a size gets another
//! size, and the comparison must report it.

use serde_json::{Value, json};
use versekit_core::jsonio;
use versekit_core::overrides::Overrides;

/// The letter of a class in the short code of 4.12.
fn class_letter(class: &str) -> &str {
    match class {
        "Military" => "M",
        "Stealth" => "S",
        "Industrial" => "I",
        "Civilian" => "C",
        "Competition" => "K",
        _ => "–",
    }
}

fn answer(overrides: &Overrides, name: &str) -> Value {
    let (size, grade, class) = match overrides.complete(name) {
        Some((class, size, grade)) => (Some(size), Some(grade), Some(class)),
        None => (overrides.size(name), None, None),
    };
    let code = if size.is_none() && grade.is_none() && class.is_none() {
        String::new()
    } else {
        let or_dash = |part: Option<&str>| part.filter(|p| !p.is_empty()).unwrap_or("–").to_string();
        let letter = class.filter(|c| !c.is_empty()).map_or("–", class_letter);
        format!("{letter}/{}/{}", or_dash(grade), or_dash(size))
    };
    json!({"name": name, "size": size, "grade": grade, "class": class, "code": code})
}

pub fn run(input: &str, out: &str, counter: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
    let python: Value = serde_json::from_str(&text).map_err(|e| format!("{input}: {e}"))?;
    let mut scenarios = Vec::new();
    let mut changed = !counter;
    for scenario in python["scenarios"].as_array().ok_or("no scenarios")? {
        let file = scenario["file"].as_str().ok_or("no file")?;
        let overrides = jsonio::read_json(file).map(|data| Overrides::from_value(&data)).unwrap_or_default();
        let mut answers = Vec::new();
        for asked in scenario["answers"].as_array().ok_or("no answers")? {
            let mut one = answer(&overrides, asked["name"].as_str().ok_or("no name")?);
            if !changed && !one["size"].is_null() {
                one["size"] = json!("99");
                changed = true;
            }
            answers.push(one);
        }
        scenarios.push(json!({"file": file, "answers": answers}));
    }
    let data = json!({ "scenarios": scenarios });
    std::fs::write(out, serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Szenarien", scenarios.len());
    Ok(())
}
