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

//! Rust side of the parity check for the stock page
//! (`tools/parity_stock.py`): rebuilds the lists, number input and
//! refinery lines of both rounds and replays every file scenario on its own
//! files.
//!
//! `--gegenprobe` leaves out the last storable name and does not save the
//! first step of every scenario.

use std::collections::BTreeMap;
use std::fs;

use versekit_core::jsonio::{self, Map, Newline, Value};
use versekit_core::language::Lang;
use versekit_core::stock::{self, Cost, Refused, Row, Store};
use versekit_core::stock_data::StockData;

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match value.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt")),
    }
}

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn opt_float(x: Option<f64>) -> Value {
    x.map_or(Value::Null, Value::Float)
}

fn opt_str(x: Option<String>) -> Value {
    x.map_or(Value::Null, Value::Str)
}

fn strs(items: &[String]) -> Value {
    Value::Array(items.iter().cloned().map(Value::Str).collect())
}

fn number(v: &Value) -> Result<f64, String> {
    match v {
        Value::Int(n) => n.parse().map_err(|_| "Zahl unlesbar".to_string()),
        Value::Float(x) => Ok(*x),
        _ => Err("keine Zahl".into()),
    }
}

fn int_of(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => n.parse().ok(),
        _ => None,
    }
}

fn arg(args: &[Value], i: usize) -> Result<&Value, String> {
    args.get(i).ok_or_else(|| format!("Argument {i} fehlt"))
}

fn arg_str(args: &[Value], i: usize) -> Result<&str, String> {
    arg(args, i)?.as_str().ok_or_else(|| format!("Argument {i} ist kein Text"))
}

fn arg_bool(args: &[Value], i: usize) -> Result<bool, String> {
    match arg(args, i)? {
        Value::Bool(b) => Ok(*b),
        _ => Err(format!("Argument {i} ist kein Wahrheitswert")),
    }
}

fn arg_index(args: &[Value], i: usize) -> Result<usize, String> {
    int_of(arg(args, i)?).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| format!("Argument {i} ist keine Stelle"))
}

fn lists(data: &StockData, probes: &[Value], place_probes: &[Value], gegenprobe: bool) -> Value {
    let mut storable = data.storable().to_vec();
    if gegenprobe {
        storable.pop();
    }
    let mut names: Vec<String> = probes.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    for name in data.storable() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    let names: Vec<Value> = names
        .iter()
        .map(|n| {
            obj(vec![
                ("name", Value::str(n.clone())),
                ("piece", Value::Bool(data.is_piece(n))),
                ("storage", opt_str(data.storage_name(n))),
                ("kind", Value::str(data.material_kind(n, false).code())),
                ("kind_sale", Value::str(data.material_kind(n, true).code())),
                ("similar", strs(&data.similar_materials(n, 2))),
                ("portion0", opt_float(data.smallest_portion(n, 0.0))),
                ("portion500", opt_float(data.smallest_portion(n, 500.0))),
                ("first", Value::int(i64::from(data.piece_first(n)))),
            ])
        })
        .collect();
    let place_probes = place_probes
        .iter()
        .filter_map(Value::as_str)
        .map(|p| Value::Array(vec![Value::str(p), Value::Bool(data.knows_place(p)), opt_str(data.official_place(p))]))
        .collect();
    let goods = data.goods();
    let mut priced = goods.clone();
    priced.extend(["Nichts".to_string(), "gold".to_string()]);
    let prices = priced
        .iter()
        .map(|g| {
            Value::Array(vec![
                Value::str(g.clone()),
                Value::Float(data.best_price(g)),
                Value::Bool(data.knows_goods(g)),
            ])
        })
        .collect();
    let numbers: BTreeMap<&String, Value> =
        data.choices().iter().filter_map(|(k, c)| c.item.clone().map(|item| (k, item))).collect();
    obj(vec![
        ("storable", strs(&storable)),
        ("material_names", strs(data.material_names())),
        ("names", Value::Array(names)),
        ("places", strs(data.places())),
        ("place_probes", Value::Array(place_probes)),
        ("goods", strs(&goods)),
        ("prices", Value::Array(prices)),
        ("items", strs(data.item_names())),
        ("item_numbers", Value::Object(numbers.into_iter().map(|(k, v)| (k.clone(), v)).collect())),
        ("catalog_size", Value::int(data.choices().len() as i64)),
    ])
}

fn numbers(texts: &[Value]) -> Value {
    Value::Array(
        texts
            .iter()
            .filter_map(Value::as_str)
            .map(|t| {
                obj(vec![
                    ("text", Value::str(t)),
                    ("calc0", opt_float(stock::calculate(t, 0.0))),
                    ("calc1", opt_float(stock::calculate(t, 1.04))),
                    ("parse", opt_float(stock::parse_number(t))),
                    ("norm", Value::str(stock::normalize_separators(t, false))),
                    ("norm_int", Value::str(stock::normalize_separators(t, true))),
                ])
            })
            .collect(),
    )
}

fn amounts(values: &[Value]) -> Result<Value, String> {
    let mut out = Vec::new();
    for v in values {
        let a = number(v)?;
        out.push(Value::Array(vec![
            Value::Float(a),
            Value::str(stock::amount_text(a, false, ',')),
            Value::str(stock::amount_text(a, true, ',')),
            Value::str(stock::amount_text(a, false, '.')),
        ]));
    }
    Ok(Value::Array(out))
}

fn refinery(texts: &[Value], data: &StockData) -> Value {
    let mut out = Vec::new();
    for t in texts.iter().filter_map(Value::as_str) {
        for unit in ["cscu", "scu"] {
            let read = stock::refinery_lines(t, unit == "cscu", data);
            let entries = read
                .entries
                .iter()
                .map(|y| {
                    Value::Array(vec![Value::str(y.material.clone()), Value::Float(y.amount), Value::int(y.quality)])
                })
                .collect();
            let errors = read
                .errors
                .iter()
                .map(|(line, e)| Value::Array(vec![Value::str(line.clone()), Value::str(e.text_in(Lang::De))]))
                .collect();
            let hints = read
                .hints
                .iter()
                .map(|(line, key, amount)| {
                    Value::Array(vec![Value::str(line.clone()), Value::str(*key), Value::Float(*amount)])
                })
                .collect();
            out.push(obj(vec![
                ("text", Value::str(t)),
                ("unit", Value::str(unit)),
                ("entries", Value::Array(entries)),
                ("errors", Value::Array(errors)),
                ("hints", Value::Array(hints)),
            ]));
        }
    }
    Value::Array(out)
}

fn row_value(r: &Row) -> Value {
    obj(vec![
        ("store", Value::str(if r.store == Store::Raw { "raw" } else { "trade" })),
        ("index", Value::int(r.index as i64)),
        ("name", Value::str(r.name.clone())),
        ("amount", Value::Float(r.amount)),
        ("piece", Value::Bool(r.piece)),
        ("quality", r.quality.map_or(Value::Null, Value::int)),
        ("place", Value::str(r.place.clone())),
        ("stolen", Value::Bool(r.stolen)),
        ("cost", opt_float(r.cost)),
        ("item", Value::Bool(r.item)),
        ("leftover", Value::Bool(r.leftover)),
    ])
}

fn booked(result: Result<(), Refused>) -> Value {
    match result {
        Ok(()) => Value::Array(vec![Value::Bool(true), Value::str("")]),
        Err(r) => Value::Array(vec![Value::Bool(false), Value::str(r.code())]),
    }
}

struct Files {
    raw: String,
    trade: String,
}

/// One step as the Python side ran it; returns the result and whether a
/// file changed.
fn step(op: &str, args: &[Value], files: &Files, data: &StockData, save: bool) -> Result<Value, String> {
    let mut raw = stock::load(&files.raw, "materials.load");
    let mut trade = stock::load(&files.trade, "trade_cargo.load");
    let (result, raw_changed, trade_changed) = match op {
        "raw_add" => {
            let quality = int_of(arg(args, 2)?);
            stock::raw_add(&mut raw, data, arg_str(args, 0)?, number(arg(args, 1)?)?, quality, arg_str(args, 3)?);
            (Value::Null, true, false)
        }
        "raw_change" => {
            let quality = int_of(arg(args, 3)?);
            let index = arg_index(args, 0)?;
            let done = stock::raw_change(
                &mut raw,
                index,
                arg_str(args, 1)?,
                number(arg(args, 2)?)?,
                quality,
                arg_str(args, 4)?,
            );
            (Value::Bool(done), done, false)
        }
        "raw_remove" => {
            let index = arg_index(args, 0)?;
            let done = index < raw.len();
            if done {
                raw.remove(index);
            }
            (Value::Bool(done), done, false)
        }
        "raw_clear" => {
            raw.clear();
            (Value::Bool(true), true, false)
        }
        "trade_add" => {
            let result = stock::trade_add(
                &mut trade,
                arg_str(args, 0)?,
                arg_str(args, 1)?,
                arg_str(args, 2)?,
                arg_bool(args, 3)?,
                arg_str(args, 4)?,
            );
            let ok = result.is_ok();
            (booked(result), false, ok)
        }
        "item" => {
            let uex = int_of(arg(args, 4)?);
            let index = match arg(args, 5)? {
                Value::Null => None,
                _ => Some(arg_index(args, 5)?),
            };
            let result = stock::trade_add_item(
                &mut trade,
                arg_str(args, 0)?,
                arg_str(args, 1)?,
                arg_str(args, 2)?,
                arg_bool(args, 3)?,
                uex,
                index,
            );
            let ok = result.is_ok();
            (booked(result), false, ok)
        }
        "trade_change" => {
            let cost = match arg_str(args, 5)? {
                "KEEP" => Cost::Keep,
                text => Cost::Text(text),
            };
            let result = stock::trade_change(
                &mut trade,
                arg_index(args, 0)?,
                arg_str(args, 1)?,
                arg_str(args, 2)?,
                arg_str(args, 3)?,
                arg_bool(args, 4)?,
                cost,
            );
            let ok = result.is_ok();
            (booked(result), false, ok)
        }
        "trade_remove" => {
            let index = arg_index(args, 0)?;
            let done = index < trade.len();
            if done {
                trade.remove(index);
            }
            (Value::Bool(done), false, done)
        }
        "clear_goods" => (Value::int(stock::clear_goods(&mut trade) as i64), false, true),
        "clear_items" => (Value::int(stock::clear_items(&mut trade) as i64), false, true),
        "refinery" => {
            let read = stock::refinery_lines(arg_str(args, 0)?, arg_str(args, 1)? == "cscu", data);
            let place = arg_str(args, 2)?;
            for y in &read.entries {
                stock::raw_add(&mut raw, data, &y.material, y.amount, Some(y.quality), place);
            }
            let any = !read.entries.is_empty();
            (Value::int(read.entries.len() as i64), any, false)
        }
        other => return Err(format!("unbekannter Schritt {other}")),
    };
    if save && raw_changed && !stock::save(&files.raw, &raw, "materials.save") {
        return Err(format!("{} nicht gespeichert", files.raw));
    }
    if save && trade_changed && !stock::save(&files.trade, &trade, "trade_cargo.save") {
        return Err(format!("{} nicht gespeichert", files.trade));
    }
    Ok(result)
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let base_folder = text(&input, "rust_folder")?;
    // Every run gets a new folder of its own; nothing is deleted.
    let folder = (1..1000)
        .map(|n| format!("{base_folder}/lauf-{n}"))
        .find(|f| !std::path::Path::new(f).exists())
        .ok_or("kein freier Laufordner")?;
    let helpers = input.get("helpers").ok_or("helpers fehlt")?.clone();
    let probes = list(&input, "probes")?;
    let place_probes = list(&input, "place_probes")?;
    let mut rounds = Vec::new();
    for round in list(&input, "rounds")? {
        let with_data = matches!(round.get("data"), Some(Value::Bool(true)));
        let missing = format!("{folder}/fehlt");
        let data = StockData::load(|name| match helpers.get(name).and_then(Value::as_str) {
            Some(path) if with_data && !path.is_empty() => path.to_string(),
            _ => format!("{missing}/{name}"),
        });
        let mut scenarios = Vec::new();
        for scenario in list(round, "scenarios")? {
            let name = text(scenario, "name")?;
            let base = format!("{folder}/{name}");
            let snaps_folder = format!("{folder}/stand/{name}");
            jsonio::make_dirs(&snaps_folder).map_err(|e| e.to_string())?;
            let files =
                Files { raw: format!("{base}/{}", stock::RAW_FILE), trade: format!("{base}/{}", stock::TRADE_FILE) };
            jsonio::make_dirs(&base).map_err(|e| e.to_string())?;
            for (target, key) in [(&files.raw, "raw"), (&files.trade, "trade")] {
                if let Some(start) = scenario.get(key).and_then(Value::as_str) {
                    jsonio::write_text_atomic(target, start, Newline::Native).map_err(|e| e.to_string())?;
                }
            }
            let mut snaps: Vec<Value> = Vec::new();
            let snap = |snaps: &mut Vec<Value>| -> Result<(), String> {
                let mut pair = Vec::new();
                for target in [&files.raw, &files.trade] {
                    if std::path::Path::new(target).exists() {
                        let file_name = target.rsplit('/').next().unwrap_or("datei.json");
                        let file = format!("{snaps_folder}/{}-{file_name}", snaps.len());
                        fs::copy(target, &file).map_err(|e| e.to_string())?;
                        pair.push(Value::str(file));
                    } else {
                        pair.push(Value::str(""));
                    }
                }
                snaps.push(Value::Array(pair));
                Ok(())
            };
            snap(&mut snaps)?;
            let mut steps = Vec::new();
            for (i, s) in list(scenario, "steps")?.iter().enumerate() {
                let op = text(s, "op")?;
                let args = list(s, "args")?;
                let result = step(op, args, &files, &data, !(gegenprobe && i == 0))?;
                let raw = stock::load(&files.raw, "materials.load");
                let trade = stock::load(&files.trade, "trade_cargo.load");
                let rows = stock::rows(&raw, &trade, &data).iter().map(row_value).collect();
                let step_out: Map =
                    vec![("op".into(), Value::str(op)), ("result".into(), result), ("rows".into(), Value::Array(rows))];
                steps.push(Value::Object(step_out));
                snap(&mut snaps)?;
            }
            scenarios.push(obj(vec![
                ("name", Value::str(name)),
                ("steps", Value::Array(steps)),
                ("snaps", Value::Array(snaps)),
            ]));
        }
        rounds.push(obj(vec![
            ("data", Value::Bool(with_data)),
            ("lists", lists(&data, probes, place_probes, gegenprobe)),
            ("numbers", numbers(list(&input, "numbers")?)),
            ("amounts", amounts(list(&input, "amounts")?)?),
            ("refinery", refinery(list(&input, "refinery")?, &data)),
            ("scenarios", Value::Array(scenarios)),
        ]));
    }
    let result = obj(vec![("rounds", Value::Array(rounds))]);
    fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())
}
