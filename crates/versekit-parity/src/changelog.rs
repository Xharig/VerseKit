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

//! Rust side of the check for the history of the versions (`tools/parity_changelog.py`): the built-in change
//! logs in both languages together with the releases the Python side gives.

use std::fs;

use serde_json::{Value, json};
use versekit_core::changelog::{self, Entry, Kind};
use versekit_core::language::Lang;
use versekit_core::updater::Release;

fn releases(python: &Value) -> Vec<Release> {
    python["releases"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|r| Release {
                    version: r["version"].as_str().unwrap_or_default().to_string(),
                    date: r["datum"].as_str().unwrap_or_default().to_string(),
                    text: r["text"].as_str().unwrap_or_default().to_string(),
                    prerelease: r["vorab"].as_bool().unwrap_or(false),
                    ..Release::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn tag(kind: Kind) -> &'static str {
    match kind {
        Kind::New => "neu",
        Kind::Better => "bess",
        Kind::Fix => "fix",
    }
}

fn entry_json(entry: &Entry) -> Value {
    let points: Vec<Value> =
        changelog::points_by_kind(&entry.text).iter().map(|(kind, line)| json!([tag(*kind), line])).collect();
    json!({
        "version": entry.version,
        "datum": entry.date,
        "text": entry.text,
        "vorspann": changelog::intro(&entry.text),
        "punkte": points,
        "quelle": if entry.built_in { "changelog" } else { "github" },
    })
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let python: Value =
        serde_json::from_slice(&fs::read(python_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let releases = releases(&python);
    let mut result = serde_json::Map::new();
    for (code, lang) in [("de", Lang::De), ("en", Lang::En)] {
        let mut entries: Vec<Value> = changelog::grouped(lang, &releases).iter().map(entry_json).collect();
        if gegenprobe && code == "de" {
            // A version dropped and a kind swapped.
            entries.remove(2);
            if let Some(points) = entries[0]["punkte"].as_array_mut()
                && let Some(first) = points.first_mut()
            {
                first[0] = json!("fix");
            }
        }
        result.insert(code.to_string(), Value::Array(entries));
    }
    fs::write(out, serde_json::to_vec_pretty(&Value::Object(result)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
