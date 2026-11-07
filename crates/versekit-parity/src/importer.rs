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

//! Rust side of the check for reading in a stand: the files the Python side wrote, each read, previewed against
//! the same catalog and merged into the same starting collection, in a data folder of its own beside the Python
//! one, with the clock standing still. With `--gegenprobe` the first entry of the first file with entries gets
//! another time, and the comparison must report it.

use versekit_core::collection::{Catalog, Collection, Known};
use versekit_core::export::{self, FileExports};
use versekit_core::importer;
use versekit_core::jsonio::{self, Value};
use versekit_core::logsource::LogSource;
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{BackRoute, NoArchive};
use versekit_core::pypath::Flavor;

/// No catalog: the scratch folder of the Python side has none either.
struct NoCatalog;

impl Catalog for NoCatalog {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::new(std::iter::empty()))
    }
}

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(value: &Value, key: &str) -> String {
    get(value, key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn texts(list: &[String]) -> Value {
    Value::Array(list.iter().map(Value::str).collect())
}

fn fixed() -> String {
    "2026-10-30 12:00:00".to_string()
}

pub fn run(input: &str, out: &str, counter: bool) -> Result<(), String> {
    let python = jsonio::read_json(input).map_err(|e| format!("{input}: {e:?}"))?;
    let scratch = text(&python, "scratch");
    let flavor = Flavor::native();
    let home = flavor.join(&scratch, "rust-daten");
    let game = flavor.join(&scratch, "spiel");
    std::fs::create_dir_all(&home).map_err(|e| format!("{home}: {e}"))?;
    let vars = [
        (versekit_core::legacy::ENV_HOME, home.as_str()),
        (versekit_core::paths::ENV_INSTALL_DIR, game.as_str()),
        ("APPDATA", scratch.as_str()),
        ("LOCALAPPDATA", scratch.as_str()),
        (versekit_core::legacy::ENV_NO_NET, "1"),
    ];
    let paths = Paths::new(Host::new(flavor, &vars, &home));
    let catalog_names: Vec<String> = match get(&python, "catalog") {
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let back = BackRoute { paths: &paths, archive: &NoArchive };
    let exports = FileExports { paths: &paths, version: "5.0.0", clock: export::now_unix };
    let collection = Collection { paths: &paths, catalog: &NoCatalog, names: &back, exports: &exports, clock: fixed };
    let mut start = collection.load();
    collection.add(&mut start, "Karna Rifle", "log", Some("2026-01-01 00:00:00"));
    let sources = LogSource::sources_of(&paths);
    let mut cases = Vec::new();
    let mut changed = !counter;
    for case in match get(&python, "cases") {
        Some(Value::Array(list)) => list.as_slice(),
        _ => &[],
    } {
        let (kind, mut entries) = importer::read(&text(case, "file"), &|key| sources.resolve_key(key));
        if !changed && let Some(first) = entries.first_mut() {
            first.time = Some("1999-01-01 00:00:00".to_string());
            changed = true;
        }
        let mut data = start.clone();
        let view = importer::preview(&entries, &data, &catalog_names);
        let added = importer::merge(&collection, &entries, &mut data, false);
        let listed = entries
            .iter()
            .map(|e| Value::Array(vec![Value::str(&e.name), e.time.as_deref().map_or(Value::Null, Value::str)]))
            .collect();
        cases.push(Value::Object(vec![
            ("name".into(), Value::str(text(case, "name"))),
            ("kind".into(), kind.map_or(Value::Null, |k| Value::str(k.code()))),
            ("entries".into(), Value::Array(listed)),
            (
                "preview".into(),
                Value::Object(vec![
                    ("neu".into(), texts(&view.new)),
                    ("schon_da".into(), texts(&view.present)),
                    ("unbekannt".into(), texts(&view.unknown)),
                    ("gesamt".into(), Value::int(view.total as i64)),
                ]),
            ),
            ("added".into(), Value::int(added as i64)),
            (
                "bauplaene".into(),
                data.iter().find(|(k, _)| k == "bauplaene").map(|(_, v)| v.clone()).unwrap_or(Value::Null),
            ),
        ]));
    }
    let count = cases.len();
    let data = Value::Object(vec![("cases".into(), Value::Array(cases))]);
    std::fs::write(out, jsonio::dumps(&data, Some(1), false)).map_err(|e| format!("{out}: {e}"))?;
    println!("{count} Dateien");
    Ok(())
}
