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

//! Rust side of the parity check for backing up and reading back the stock
//! (`tools/parity_stock_backup.py`): the same file texts through
//! `versekit_core::stock`, the same pairs through `restore_trade`.
//!
//! `--gegenprobe` writes the tables with a decimal point and puts the items
//! before the goods when reading in.

use std::fs;

use versekit_core::jsonio::{self, Value};
use versekit_core::stock::{self, Backup, Raises};

fn failed() -> Value {
    Value::Object(vec![("fehler".to_string(), Value::Bool(true))])
}

/// A list result: the list, `null` for „does not fit“, `{"fehler": true}` for „raises“.
fn listed(result: Result<Option<Vec<Value>>, Raises>) -> Value {
    match result {
        Ok(Some(entries)) => Value::Array(entries),
        Ok(None) => Value::Null,
        Err(Raises) => failed(),
    }
}

fn table(text: Option<String>, gegenprobe: bool) -> Value {
    match text {
        Some(t) if gegenprobe => Value::Str(t.replace(',', ".")),
        Some(t) => Value::Str(t),
        None => failed(),
    }
}

/// The raw entry list of a text, as the Python side takes it.
fn entries_of(text: &str) -> Option<Vec<Value>> {
    match jsonio::loads(text).ok()?.get("posten") {
        Some(Value::Array(entries)) => Some(entries.clone()),
        _ => None,
    }
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("{key} fehlt"))
}

fn text_field(v: &Value, key: &str) -> Result<String, String> {
    match field(v, key)? {
        Value::Str(s) => Ok(s.clone()),
        _ => Err(format!("{key} ist kein Text")),
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let Value::Array(cases) = field(&input, "cases")? else { return Err("cases ist keine Liste".into()) };
    let mut results = Vec::new();
    let mut by_name = Vec::new();
    for case in cases {
        let name = text_field(case, "name")?;
        let text = text_field(case, "text")?;
        let detected = match stock::try_detect_backup(&text) {
            Ok(Some(Backup::Raw(entries))) => Value::Array(vec![Value::str("raw"), Value::Array(entries)]),
            Ok(Some(Backup::Trade(entries))) => Value::Array(vec![Value::str("trade"), Value::Array(entries)]),
            Ok(None) => Value::Null,
            Err(Raises) => failed(),
        };
        let entries = entries_of(&text);
        let trade = stock::try_trade_from_json(&text);
        by_name.push((name.clone(), text.clone(), trade.clone()));
        results.push(Value::Object(vec![
            ("name".into(), Value::Str(name)),
            ("raw_from_json".into(), listed(stock::try_raw_from_json(&text))),
            ("trade_from_json".into(), listed(trade)),
            ("detect".into(), detected),
            ("raw_csv".into(), entries.as_ref().map_or(Value::Null, |e| table(stock::raw_csv(e), gegenprobe))),
            ("trade_csv".into(), entries.as_ref().map_or(Value::Null, |e| table(stock::trade_csv(e), gegenprobe))),
            ("as_json".into(), entries.as_ref().map_or(Value::Null, |e| Value::Str(stock::as_json(e)))),
        ]));
    }
    let goods: Vec<_> = by_name.iter().filter_map(|(n, t, f)| Some((n, t, f.clone().ok()??))).collect();
    let mut restores = Vec::new();
    for (here, here_text, _) in &goods {
        for (there, _, found) in &goods {
            let current = entries_of(here_text).ok_or("Liste fehlt")?;
            let (counts, result) = match stock::restore_trade(&current, found) {
                Some((mut all, goods_count, added)) => {
                    if gegenprobe {
                        all.reverse();
                    }
                    (Value::Array(vec![Value::int(goods_count as i64), Value::int(added as i64)]), all)
                }
                None => (failed(), current),
            };
            restores.push(Value::Object(vec![
                ("here".into(), Value::str(here.as_str())),
                ("there".into(), Value::str(there.as_str())),
                ("counts".into(), counts),
                ("result".into(), Value::Array(result)),
            ]));
        }
    }
    let result =
        Value::Object(vec![("cases".into(), Value::Array(results)), ("restores".into(), Value::Array(restores))]);
    fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())
}
