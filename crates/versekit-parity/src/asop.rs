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

//! Rust side of the parity check for the own ship names
//! (`tools/parity_asop.py`): the same parts on the same inputs, written in
//! the same form. Writing happens only in the Rust folder.
//!
//! `--gegenprobe` drops the last match of every list, flips the star of the
//! first step of every run and drops one key of every scenario table.

use std::fs;

use versekit_core::asop::{self, Table};
use versekit_core::game_ini;
use versekit_core::jsonio::{self, Map, Newline, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::writeguard;

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match value.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt")),
    }
}

fn s(text: &str) -> Value {
    Value::str(text)
}

fn pairs<'a>(items: impl IntoIterator<Item = (&'a String, &'a String)>, sort: bool) -> Value {
    let mut items: Vec<(&String, &String)> = items.into_iter().collect();
    if sort {
        items.sort();
    }
    Value::Array(items.into_iter().map(|(k, v)| Value::Array(vec![s(k), s(v)])).collect())
}

fn table_value(table: &Table) -> Value {
    pairs(table.iter(), true)
}

fn matches(ships: &[Value], table: &Table, gegenprobe: bool) -> Value {
    let maps: Vec<&Map> = ships.iter().filter_map(Value::as_object).collect();
    let mut found: Vec<Value> = asop::match_ships(&maps, table)
        .into_iter()
        .map(|m| Value::Array(vec![s(&m.name), s(&m.short), s(&m.key), s(&m.factory), s(m.way)]))
        .collect();
    if gegenprobe {
        found.pop();
    }
    Value::Array(found)
}

fn built<S: AsRef<str>>(lines: &[S], data: &Map) -> Value {
    Value::Array(
        asop::build_table(lines, data)
            .into_iter()
            .map(|(k, (name, star))| Value::Array(vec![s(&k), Value::Array(vec![s(&name), Value::Bool(star)])]))
            .collect(),
    )
}

fn object(value: &Value) -> Map {
    value.as_object().cloned().unwrap_or_default()
}

fn lines_of(path: &str) -> Result<Vec<String>, String> {
    game_ini::read_lines(path).ok_or_else(|| format!("{path} unlesbar"))
}

fn ship_lists(input: &Value) -> Vec<(String, Vec<Value>)> {
    match input.get("ships") {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(name, v)| (name.clone(), if let Value::Array(items) = v { items.clone() } else { Vec::new() }))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let folder = text(&input, "rust_folder")?.to_string();
    writeguard::deny(true);
    writeguard::allow_under(Some(&folder));
    let result = sides(&input, &folder, gegenprobe);
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    if !blocked.is_empty() {
        return Err(format!("Schreibversuche außerhalb des Rust-Ordners: {blocked:?}"));
    }
    fs::write(out, jsonio::dumps(&Value::Object(result?), Some(1), false)).map_err(|e| e.to_string())
}

fn sides(input: &Value, folder: &str, gegenprobe: bool) -> Result<Map, String> {
    let german = lines_of(text(input, "ini_de")?)?;
    let english = lines_of(text(input, "ini_en")?)?;
    let rein_in = input.get("rein").ok_or("rein fehlt")?;
    let keys_de = asop::read_keys(&german);
    let origtext = object(rein_in.get("origtext").ok_or("origtext fehlt")?);
    let factory = asop::factory_names(&keys_de, &origtext);
    let missing = |a: &[String], b: &[String]| {
        let found = asop::missing_names(a, b);
        pairs(found.iter().map(|(k, v)| (k, v)), false)
    };
    let display: Vec<Value> = list(rein_in, "display")?
        .iter()
        .filter_map(|row| match row {
            Value::Array(cells) if cells.len() >= 3 => {
                let (w, o) = (cells[0].as_str().unwrap_or(""), cells[1].as_str().unwrap_or(""));
                let star = matches!(cells[2], Value::Bool(true));
                Some(Value::Array(vec![s(w), s(o), Value::Bool(star), s(&asop::display_name(w, o, star))]))
            }
            _ => None,
        })
        .collect();
    let build_data: Vec<Map> = list(rein_in, "build_data")?.iter().map(object).collect();
    let rein: Map = vec![
        ("read_keys_de".into(), table_value(&keys_de)),
        ("read_keys_en".into(), table_value(&asop::read_keys(&english))),
        ("missing".into(), missing(&german, &english)),
        ("missing_reverse".into(), missing(&english, &german)),
        ("factory".into(), table_value(&factory)),
        ("factory_none".into(), table_value(&asop::factory_names(&keys_de, &Map::new()))),
        ("display".into(), Value::Array(display)),
        ("build".into(), Value::Array(build_data.iter().map(|d| built(&german, d)).collect())),
    ];

    let ships = ship_lists(input);
    let mut tables: Vec<(&str, Table)> = vec![("test", factory.clone())];
    let names_file = text(input, "names_file")?;
    if !names_file.is_empty() {
        let saved = object(&jsonio::read_json(names_file).map_err(|e| e.to_string())?);
        let lines = asop::saved_name_lines(&saved);
        tables.push(("namen", asop::factory_names(&asop::read_keys(&lines), &Map::new())));
    }
    let zuordnung: Map = tables
        .iter()
        .map(|(name, table)| {
            let per: Map = ships.iter().map(|(n, list)| (n.clone(), matches(list, table, gegenprobe))).collect();
            (name.to_string(), Value::Object(per))
        })
        .collect();

    let mut runs = Vec::new();
    let mut real_data: Option<Map> = None;
    for run in list(input, "ablaeufe")? {
        let name = text(run, "name")?;
        let home = format!("{folder}/ablauf-{name}");
        jsonio::make_dirs(&home).map_err(|e| e.to_string())?;
        let target = format!("{home}/{}", asop::FILE);
        if let Some(start) = run.get("start").and_then(Value::as_str) {
            jsonio::write_text_atomic(&target, start, Newline::Native).map_err(|e| e.to_string())?;
            if let Ok(Value::Object(map)) = jsonio::loads(start) {
                real_data = Some(map);
            }
        }
        let mut data = asop::load(&target);
        let (mut snaps, mut entries) = (Vec::new(), Vec::new());
        for (i, step) in list(run, "steps")?.iter().enumerate() {
            let Value::Array(cells) = step else { return Err("Schritt unlesbar".into()) };
            let key = cells.first().and_then(Value::as_str).ok_or("Schlüssel fehlt")?;
            let name_text = cells.get(1).and_then(Value::as_str).ok_or("Name fehlt")?;
            let star = matches!(cells.get(2), Some(Value::Bool(true))) != (gegenprobe && i == 0);
            asop::set_name(&mut data, key, name_text, star);
            if !asop::save(&target, &data) {
                return Err(format!("{target} nicht gespeichert"));
            }
            let file = format!("{home}/stand-{i}.json");
            fs::copy(&target, &file).map_err(|e| e.to_string())?;
            snaps.push(s(&file));
            let (own, starred) = asop::entry(&data, key);
            entries.push(Value::Array(vec![s(&own), Value::Bool(starred), Value::int(asop::count(&data) as i64)]));
        }
        runs.push(Value::Object(vec![
            ("name".into(), s(name)),
            ("snaps".into(), Value::Array(snaps)),
            ("entries".into(), Value::Array(entries)),
            ("build".into(), built(&german, &data)),
        ]));
    }

    let mut loads = Vec::new();
    for case in list(input, "laden")? {
        let data = asop::load(text(case, "file")?);
        loads.push(Value::Object(vec![
            ("name".into(), s(text(case, "name")?)),
            ("result".into(), s(&jsonio::dumps(&Value::Object(data), None, false))),
        ]));
    }

    let real_data = real_data.unwrap_or_else(|| build_data.get(2).cloned().unwrap_or_default());
    let mut scenarios = Vec::new();
    for scen in list(input, "szenarien")? {
        let name = text(scen, "name")?;
        let home = format!("{folder}/szenario-{name}");
        jsonio::make_dirs(&home).map_err(|e| e.to_string())?;
        for (file, content) in scen.get("files").and_then(Value::as_object).into_iter().flatten() {
            let content = content.as_str().ok_or("Dateiinhalt fehlt")?;
            jsonio::write_new(&format!("{home}/{file}"), content.as_bytes()).map_err(|e| e.to_string())?;
        }
        let mut host = Host::current();
        host.registry = false;
        host.set_var("SC_BP_HOME", &home);
        match text(scen, "game")? {
            "" => host.remove_var("SC_INSTALL_DIR"),
            game => host.set_var("SC_INSTALL_DIR", game),
        }
        let paths = Paths::new(host);
        let ini = game_ini::ini_file(&paths);
        let lines = asop::ini_lines(&paths, ini.as_deref());
        let mut table = asop::factory_table(&paths);
        if gegenprobe && let Some(key) = table.keys().min().cloned() {
            table.remove(&key);
        }
        let per: Map = ships.iter().map(|(n, list)| (n.clone(), matches(list, &table, false))).collect();
        scenarios.push(Value::Object(vec![
            ("name".into(), s(name)),
            ("ini".into(), ini.map_or(Value::Null, Value::Str)),
            ("lines".into(), Value::int(lines.len() as i64)),
            ("table".into(), table_value(&table)),
            ("matches".into(), Value::Object(per)),
            ("build".into(), built(&lines, &real_data)),
        ]));
    }

    Ok(vec![
        ("rein".into(), Value::Object(rein)),
        ("zuordnung".into(), Value::Object(zuordnung)),
        ("ablaeufe".into(), Value::Array(runs)),
        ("laden".into(), Value::Array(loads)),
        ("szenarien".into(), Value::Array(scenarios)),
    ])
}
