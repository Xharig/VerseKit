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

//! Rust side of the check for "Bedarf der Einheit"
//! (`tools/parity_demand.py`): the answer of the Basetool in its fixed form,
//! which materials fit a list of names and when the copy in memory is too old.

use std::fs;

use serde_json::{Value, json};
use versekit_core::basetool::demand::{self, Demand, DemandStore};
use versekit_core::jsonio;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn own(value: &Value) -> Result<jsonio::Value, String> {
    jsonio::loads(&serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|_| "json".to_string())
}

fn parsed_json(demand: &Demand) -> Value {
    json!({
        "materials": demand.materials.iter().map(|m| json!({
            "name": m.name, "raw": m.raw, "min_quality": m.min_quality, "amount": m.amount,
            "unit": m.unit, "source": m.source,
        })).collect::<Vec<_>>(),
        "items": demand.items.iter().map(|i| json!({
            "name": i.name, "amount": i.amount, "unit": i.unit, "craftable": i.craftable,
        })).collect::<Vec<_>>(),
        "orders": demand.orders.as_ref().map(|orders| orders.iter().map(|o| json!({
            "id": o.id, "number": o.number, "type": o.kind, "status": o.status, "priority": o.priority,
            "unit": o.unit, "requesting": o.requesting, "handle": o.handle, "comment": o.comment,
            "lines": o.lines.iter().map(|l| json!({
                "name": l.name, "amount": l.amount, "unit": l.unit, "min_quality": l.min_quality, "item": l.item,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>()),
        "updated": demand.updated,
        "reason": demand.reason,
    })
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let mut result = json!({});
    let mut cases = Vec::new();
    let mut big = Demand::default();
    for case in input["cases"].as_array().ok_or("cases fehlt")? {
        let demand = demand::parse(&own(&case["answer"])?);
        if case["name"] == json!("materials") {
            big = demand.clone();
        }
        cases.push(json!({ "name": case["name"], "answer": case["answer"], "parsed": parsed_json(&demand) }));
    }
    result["cases"] = json!(cases);
    let store = DemandStore::new();
    store.take(big.clone(), 1000.0);
    let mut matching = Vec::new();
    for case in input["matching"].as_array().ok_or("matching fehlt")? {
        let names: Vec<String> =
            case["names"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_string)).collect();
        let found: Vec<String> =
            store.matching(&names, 1000.0).iter().map(|m| format!("{}|{}", m.name, m.min_quality)).collect();
        matching.push(json!({ "names": case["names"], "found": found }));
    }
    result["matching"] = json!(matching);
    let mut age = Vec::new();
    for case in input["age"].as_array().ok_or("age fehlt")? {
        let delta = case["delta"].as_f64().ok_or("delta fehlt")?;
        let store = DemandStore::new();
        store.take(big.clone(), 1000.0);
        age.push(json!({ "delta": case["delta"], "there": store.current(1000.0 + delta).1.is_some() }));
    }
    result["age"] = json!(age);
    // The store: what is kept after a clean answer, after one with a reason, after a clean one again and after `forget`.
    let store = DemandStore::new();
    let mut steps = Vec::new();
    for answer in input["store"]["sequence"].as_array().ok_or("store fehlt")? {
        let counts = store.take(demand::parse(&own(answer)?), 5000.0);
        let count =
            |key: &str| counts.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or(jsonio::Value::Null);
        let number = |key: &str| match count(key) {
            jsonio::Value::Int(n) => n.parse::<i64>().unwrap_or(-1),
            _ => -1,
        };
        let data = store.current(5000.0).1;
        steps.push(json!({
            "counts": { "materials": number("materials"), "items": number("items"),
                        "reason": count("reason").as_str().unwrap_or_default() },
            "there": data.is_some(),
            "reason": store.reason(),
            "materials": data.map_or(0, |d| d.materials.len()),
        }));
    }
    store.forget();
    steps.push(json!({
        "counts": Value::Null, "there": store.current(5000.0).1.is_some(),
        "reason": store.reason(), "materials": 0,
    }));
    result["store"] = json!({ "sequence": input["store"]["sequence"], "steps": steps });
    if gegenprobe {
        // One name, one list of orders, one match and one age changed: the comparison must report all four.
        result["cases"][1]["parsed"]["materials"][0]["name"] = json!("Gegenprobe");
        result["cases"][3]["parsed"]["orders"] = Value::Null;
        result["matching"][0]["found"] = json!([]);
        result["age"][3]["there"] = json!(true);
        result["store"]["steps"][1]["there"] = json!(true);
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
