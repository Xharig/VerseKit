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

//! Rust side of the parity check for the blueprint sync and the stock sync
//! with the Basetool, the parts without the net (`tools/parity_exchange.py`):
//! the collection and the stocks in the form of the contract, the plan of a
//! pass and the change sets, from the cases of the Python output.
//!
//! `--gegenprobe` changes one value in every third case of each kind; the
//! comparison must report them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use versekit_core::basetool::blueprint_sync::{self, BlueprintState, LocalBlueprint, Plan};
use versekit_core::basetool::stock_sync::{self, LocalLot, Places, ServerLot, Skipped, StockPlan};
use versekit_core::jsonio::{self, Map, Value};

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("{key} fehlt"))
}

fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match field(v, key)? {
        Value::Array(items) => Ok(items),
        _ => Err(format!("{key} ist keine Liste")),
    }
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(Value::str).collect())
}

fn pairs(items: &[(String, String)]) -> Value {
    Value::Array(items.iter().map(|(a, b)| Value::Array(vec![Value::str(a), Value::str(b)])).collect())
}

fn pairs_of_map(items: &BTreeMap<String, String>) -> Value {
    Value::Array(items.iter().map(|(a, b)| Value::Array(vec![Value::str(a), Value::str(b)])).collect())
}

fn string_map(v: &Value) -> BTreeMap<String, String> {
    v.as_object().into_iter().flatten().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
}

fn string_set(v: &Value) -> BTreeSet<String> {
    match v {
        Value::Array(items) => items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
        _ => BTreeSet::new(),
    }
}

fn local_json(local: &BTreeMap<String, LocalBlueprint>) -> Value {
    Value::Object(
        local
            .iter()
            .map(|(key, b)| {
                (
                    key.clone(),
                    obj(vec![
                        ("name", Value::str(&b.name)),
                        ("tag", b.tag.as_ref().map_or(Value::Null, Value::str)),
                        ("source", Value::str(&b.source)),
                        ("time", b.time.clone().unwrap_or(Value::Null)),
                    ]),
                )
            })
            .collect(),
    )
}

fn plan_json(plan: &Plan) -> Value {
    obj(vec![
        ("add", strings(&plan.add)),
        ("override", strings(&plan.r#override)),
        ("remove", pairs(&plan.remove)),
        ("conflicts", pairs_of_map(&plan.conflicts)),
        ("local_add", strings(&plan.local_add)),
        ("local_remove", strings(&plan.local_remove)),
        ("local_removed", pairs_of_map(&plan.local_removed)),
        ("links", pairs_of_map(&plan.links)),
        ("unresolved", strings(&plan.unresolved)),
    ])
}

fn sets_json(sets: Vec<(Value, Vec<String>)>) -> Value {
    Value::Array(sets.into_iter().map(|(set, keys)| Value::Array(vec![set, strings(&keys)])).collect())
}

fn blueprint_case(given: &Value, wrong: bool) -> Result<Value, String> {
    let collection = field(given, "collection")?.as_object().cloned().unwrap_or_default();
    let tags: HashMap<String, String> = string_map(field(given, "tags")?).into_iter().collect();
    let foreign = string_set(field(given, "foreign")?);
    let state_value = field(given, "state")?;
    let state = BlueprintState::from_map(state_value.as_object().ok_or("state")?);
    let local = blueprint_sync::local_blueprints(&collection, &tags);
    let own = blueprint_sync::own_blueprints(&local, &foreign, &state.links);
    let resolved = string_map(field(given, "resolved")?);
    let server: BTreeMap<String, Value> =
        field(given, "server")?.as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.clone())).collect();
    let resync = matches!(field(given, "resync")?, Value::Bool(true));
    let mut plan = blueprint_sync::plan_sync(&own, &resolved, &server, array(given, "stones")?, &state, resync);
    if wrong {
        plan.unresolved.push("gegenprobe".to_string());
    }
    let sets = blueprint_sync::sync_change_sets(&plan, &own);
    Ok(obj(vec![
        ("local", local_json(&local)),
        ("own", local_json(&own)),
        ("plan", plan_json(&plan)),
        ("sets", sets_json(sets)),
    ]))
}

fn read_case(given: &Value, wrong: bool) -> Result<Value, String> {
    let result = field(given, "result")?.as_object().cloned().unwrap_or_default();
    let targets: Vec<String> = array(given, "targets")?.iter().filter_map(|t| t.as_str().map(str::to_string)).collect();
    let read = blueprint_sync::read_result(&result, &targets);
    let applied = if wrong { Value::int(999) } else { read.applied.clone() };
    Ok(obj(vec![
        ("applied", applied),
        ("conflicts", strings(&read.conflicts)),
        ("unmatched", strings(&read.unmatched)),
        ("ambiguous", strings(&read.ambiguous)),
        ("rejected", pairs(&read.rejected)),
        ("cursor", read.cursor.clone()),
    ]))
}

fn merge_case(given: &Value, wrong: bool) -> Result<Value, String> {
    let pages: Vec<Map> = array(given, "pages")?.iter().filter_map(|p| p.as_object().cloned()).collect();
    let (items, stones, cursor) = blueprint_sync::merge_pages(&pages);
    let cursor = if wrong { Some("gegenprobe".to_string()) } else { cursor };
    Ok(obj(vec![
        ("items", Value::Array(items)),
        ("stones", Value::Array(stones)),
        ("cursor", cursor.as_ref().map_or(Value::Null, Value::str)),
    ]))
}

fn server_json(server: &BTreeMap<String, ServerLot>) -> Value {
    Value::Object(
        server
            .iter()
            .map(|(id, l)| {
                (
                    id.clone(),
                    obj(vec![
                        ("key", l.key.clone()),
                        ("material", l.material.clone()),
                        ("location", l.location.clone()),
                        ("quality", Value::int(l.quality)),
                        ("stolen", Value::Bool(l.stolen)),
                        ("amount", Value::Float(l.amount)),
                        ("unit", Value::str(&l.unit)),
                        ("item", Value::Bool(l.item)),
                    ]),
                )
            })
            .collect(),
    )
}

fn local_lots_json(local: &BTreeMap<String, LocalLot>) -> Value {
    Value::Object(
        local
            .iter()
            .map(|(id, l)| {
                (
                    id.clone(),
                    obj(vec![
                        ("material", l.material.clone()),
                        ("location", l.location.clone()),
                        ("quality", Value::int(l.quality)),
                        ("stolen", Value::Bool(l.stolen)),
                        ("amount", Value::Float(l.amount)),
                        ("unit", Value::str(&l.unit)),
                        ("store", Value::str(l.store)),
                        ("rows", Value::Array(l.rows.iter().map(|r| Value::int(*r as i64)).collect())),
                    ]),
                )
            })
            .collect(),
    )
}

fn stock_plan_json(plan: &StockPlan) -> Value {
    obj(vec![
        (
            "push",
            Value::Array(
                plan.push
                    .iter()
                    .map(|(k, a, e)| Value::Array(vec![Value::str(k), Value::Float(*a), Value::Float(*e)]))
                    .collect(),
            ),
        ),
        (
            "take",
            Value::Array(plan.take.iter().map(|(k, a)| Value::Array(vec![Value::str(k), Value::Float(*a)])).collect()),
        ),
        (
            "conflicts",
            Value::Array(
                plan.conflicts
                    .iter()
                    .map(|(k, (h, t))| {
                        Value::Array(vec![Value::str(k), Value::Array(vec![Value::Float(*h), Value::Float(*t)])])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn skipped_json(skipped: &Skipped) -> Value {
    obj(vec![("material", strings(&skipped.material)), ("location", strings(&skipped.location))])
}

fn stock_case(given: &Value, wrong: bool) -> Result<Value, String> {
    let raw = array(given, "raw")?;
    let trade = array(given, "trade")?;
    let resolve = string_map(field(given, "resolve")?);
    let pieces = string_set(field(given, "pieces")?);
    let lots: Vec<&Value> = array(given, "lots")?.iter().collect();
    let known: BTreeMap<String, bool> = field(given, "known")?
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| if let Value::Bool(b) = v { Some((k.clone(), *b)) } else { None })
        .collect();
    let baseline: Option<BTreeMap<String, f64>> = match field(given, "baseline")? {
        Value::Object(map) => Some(
            map.iter()
                .map(|(k, v)| {
                    let n = match v {
                        Value::Int(n) => n.parse().unwrap_or(0.0),
                        Value::Float(x) => *x,
                        _ => 0.0,
                    };
                    (k.clone(), n)
                })
                .collect(),
        ),
        _ => None,
    };
    let decisions = string_map(field(given, "decisions")?);
    let open = string_set(field(given, "open")?);
    let batch = match field(given, "batch")? {
        Value::Int(n) => n.parse().map_err(|_| "batch")?,
        _ => return Err("batch ist keine Zahl".to_string()),
    };

    let places: Places = [
        ("orbituary", obj(vec![("name", Value::str("Orbituary")), ("uex", Value::int(7))])),
        ("lorville", obj(vec![("name", Value::str("Lorville"))])),
        ("area18", obj(vec![("name", Value::str("Area18")), ("uex", Value::int(3))])),
        ("grim hex", obj(vec![("name", Value::str("Grim HEX")), ("uex", Value::int(0))])),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let resolver = |name: &str| resolve.get(&versekit_core::text::py_strip(name).to_lowercase()).cloned();
    let is_piece = |name: &str| pieces.contains(name);
    let server = stock_sync::server_lots(&lots);
    let items = stock_sync::item_flags(&lots, Some(&known));
    let (raw_out, trade_out, moved) = stock_sync::relocate(raw, trade, &resolver, &places, &server, &items);
    let (local, skipped) = stock_sync::local_lots(&raw_out, &trade_out, &resolver, &places, &is_piece, &items);
    let result = stock_sync::plan(&local, &server, baseline.as_ref(), &decisions, &open);
    let (guarded_plan, guarded) = stock_sync::keep_unmapped(&result, &local, &server, &skipped);
    let overrides: BTreeSet<String> =
        decisions.iter().filter(|(_, v)| v.as_str() == "mine").map(|(k, _)| k.clone()).collect();
    let mut sets = stock_sync::change_sets(&guarded_plan, &local, &server, batch, &overrides);
    if wrong && let Some(first) = sets.first_mut() {
        first.1.push("gegenprobe".to_string());
    }
    Ok(obj(vec![
        ("server", server_json(&server)),
        ("items", Value::Object(items.iter().map(|(k, v)| (k.clone(), Value::Bool(*v))).collect())),
        (
            "relocated",
            obj(vec![
                ("raw", Value::Array(raw_out)),
                ("trade", Value::Array(trade_out)),
                ("moved", Value::int(if wrong { moved as i64 + 1 } else { moved as i64 })),
            ]),
        ),
        ("local", local_lots_json(&local)),
        ("skipped", skipped_json(&skipped)),
        ("plan", stock_plan_json(&result)),
        ("guarded_plan", stock_plan_json(&guarded_plan)),
        ("guarded", strings(&guarded.iter().cloned().collect::<Vec<_>>())),
        ("sets", sets_json(sets)),
    ]))
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut cases = Vec::new();
    for case in array(&input, "cases")? {
        let Value::Array(parts) = case else { return Err("ein Fall ist keine Liste".to_string()) };
        let kind = parts.first().and_then(Value::as_str).ok_or("Art fehlt")?;
        let given = parts.get(1).ok_or("Eingabe fehlt")?;
        let seen = counts.entry(kind.to_string()).or_insert(0);
        let wrong = gegenprobe && (*seen).is_multiple_of(3);
        *seen += 1;
        let made = match kind {
            "bp" => blueprint_case(given, wrong)?,
            "read" => read_case(given, wrong)?,
            "merge" => merge_case(given, wrong)?,
            "stock" => stock_case(given, wrong)?,
            other => return Err(format!("unbekannte Art {other}")),
        };
        cases.push(Value::Array(vec![Value::str(kind), given.clone(), made]));
    }
    let result = obj(vec![("cases", Value::Array(cases))]);
    std::fs::write(out, jsonio::dumps(&result, None, false)).map_err(|e| e.to_string())
}
