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

//! Rust side of the check for the changed game values: the patch files the Python side wrote, read from its
//! scratch folder, and the value pairs it drew, written as rows. With `--gegenprobe` the first row of the
//! pairs gets one character more, and the comparison must report it.

use versekit_core::jsonio::{self, Value};
use versekit_core::language::{self, Choice};
use versekit_core::patch_changes::{self, Fate, FieldChange, Offered, Summary, Tone};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(value: &Value, key: &str) -> String {
    get(value, key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn tone(tone: &Tone) -> &'static str {
    match tone {
        Tone::Up => "up",
        Tone::Down => "down",
        Tone::Gone => "gone",
        Tone::Same => "same",
        Tone::Plain => "plain",
    }
}

fn row(change: &FieldChange) -> Value {
    let (written, shade) = patch_changes::field_text(change);
    Value::Array(vec![Value::str(written), Value::str(tone(&shade))])
}

fn summary_value(summary: &Summary) -> Value {
    Value::Array(vec![Value::int(summary.added), Value::int(summary.removed), Value::int(summary.modified)])
}

pub fn run(input: &str, out: &str, counter: bool) -> Result<(), String> {
    let python = jsonio::read_json(input).map_err(|e| format!("{input}: {e:?}"))?;
    let home = text(&python, "home");
    let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
    let offered: Vec<Offered> = match get(&python, "offered") {
        Some(Value::Array(list)) => list
            .iter()
            .map(|entry| Offered {
                version: text(entry, "version"),
                date: text(entry, "datum"),
                summary: Summary::of(get(entry, "summary")),
                path: text(entry, "path"),
            })
            .collect(),
        _ => Vec::new(),
    };
    let cases: Vec<FieldChange> = match get(&python, "cases") {
        Some(Value::Array(list)) => list
            .iter()
            .map(|case| FieldChange {
                path: text(case, "path"),
                old: get(case, "oldValue").cloned(),
                new: get(case, "newValue").cloned(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let versions: Vec<String> =
        match get(&python, "languages").and_then(|l| get(l, "de")).and_then(|d| get(d, "patches")) {
            Some(Value::Object(map)) => map.iter().map(|(k, _)| k.clone()).collect(),
            _ => Vec::new(),
        };
    let mut languages = Vec::new();
    for (code, choice) in [("de", Choice::De), ("en", Choice::En)] {
        language::set_language(choice);
        let overview = patch_changes::overview(&paths, &offered)
            .iter()
            .map(|p| {
                Value::Object(vec![
                    ("version".into(), Value::str(&p.version)),
                    ("kurz".into(), Value::str(&p.short)),
                    ("datum".into(), Value::str(&p.date)),
                    ("leer".into(), Value::Bool(p.empty)),
                    ("bei_erkul".into(), Value::Bool(p.at_source)),
                    ("abgelegt".into(), Value::Bool(p.kept)),
                    ("summary".into(), summary_value(&p.summary)),
                ])
            })
            .collect();
        let mut patches = Vec::new();
        for version in &versions {
            let items = patch_changes::changes(&paths, version, None);
            let categories = patch_changes::categories(&items)
                .into_iter()
                .map(|(kind, n)| Value::Array(vec![Value::str(kind), Value::int(n as i64)]))
                .collect();
            let listed = items
                .iter()
                .map(|item| {
                    let fate = match item.fate {
                        Fate::New => "neu",
                        Fate::Gone => "weg",
                        Fate::Changed => "geaendert",
                    };
                    Value::Object(vec![
                        ("art".into(), Value::str(&item.kind)),
                        ("zustand".into(), Value::str(fate)),
                        ("id".into(), Value::str(&item.id)),
                        ("name".into(), Value::str(&item.name)),
                        ("groesse".into(), item.size.clone().unwrap_or(Value::Null)),
                        ("felder".into(), Value::Array(item.fields.iter().map(row).collect())),
                    ])
                })
                .collect();
            let weapons = patch_changes::changes(&paths, version, Some("weapons")).len();
            patches.push((
                version.clone(),
                Value::Object(vec![
                    ("categories".into(), Value::Array(categories)),
                    ("weapons".into(), Value::int(weapons as i64)),
                    ("items".into(), Value::Array(listed)),
                ]),
            ));
        }
        let mut rows: Vec<Value> = cases.iter().map(row).collect();
        if counter && let Some(Value::Array(first)) = rows.first_mut() {
            first[0] = Value::str(format!("{}!", first[0].as_str().unwrap_or("")));
        }
        languages.push((
            code.to_string(),
            Value::Object(vec![
                ("stored".into(), Value::Array(patch_changes::stored(&paths).iter().map(Value::str).collect())),
                ("overview".into(), Value::Array(overview)),
                ("patches".into(), Value::Object(patches)),
                ("rows".into(), Value::Array(rows)),
            ]),
        ));
    }
    let data = Value::Object(vec![("languages".into(), Value::Object(languages))]);
    std::fs::write(out, jsonio::dumps(&data, Some(1), false)).map_err(|e| format!("{out}: {e}"))?;
    println!("{} Wertepaare, {} Patches", cases.len(), versions.len());
    Ok(())
}
