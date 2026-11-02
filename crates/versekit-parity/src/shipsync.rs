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

//! Rust side of the parity check for the hangar sync with the Basetool, the
//! part without the net (`tools/parity_shipsync.py`): the identifier, the
//! insurance, the ships of the hangar as the sync sees them, the plan and the
//! change sets, all from the cases of the Python output. Nothing is fetched.
//!
//! `--gegenprobe` changes one value in each section; the comparison must
//! report all of them.

use std::collections::{BTreeMap, BTreeSet};

use versekit_core::basetool::ship_sync::{self, LocalShip, Plan, ShipState};
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

fn entries(list: &[Value]) -> Result<Vec<&Map>, String> {
    list.iter().map(|v| v.as_object().ok_or_else(|| "Eintrag ist kein Objekt".to_string())).collect()
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(Value::str).collect())
}

fn pairs(items: &[(String, String)]) -> Value {
    Value::Array(items.iter().map(|(a, b)| Value::Array(vec![Value::str(a), Value::str(b)])).collect())
}

fn map_of_strings(items: &BTreeMap<String, String>) -> Value {
    Value::Object(items.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect())
}

fn field_lists(items: &BTreeMap<String, Vec<String>>) -> Value {
    Value::Object(items.iter().map(|(k, v)| (k.clone(), strings(v))).collect())
}

/// `{hangar name: [key, own name or null]}` to what `local_ships` takes.
fn own_names(v: &Value) -> BTreeMap<String, (String, String)> {
    let mut out = BTreeMap::new();
    for (name, pair) in v.as_object().into_iter().flatten() {
        if let Value::Array(pair) = pair {
            let key = pair.first().and_then(Value::as_str).unwrap_or_default().to_string();
            let own = pair.get(1).and_then(Value::as_str).unwrap_or_default().to_string();
            out.insert(name.clone(), (key, own));
        }
    }
    out
}

fn local_json(local: &BTreeMap<String, LocalShip>, shout: bool) -> Value {
    Value::Object(
        local
            .iter()
            .map(|(ext, s)| {
                let name = if shout { s.name.to_uppercase() } else { s.name.clone() };
                let text = |t: &Option<String>| t.as_ref().map_or(Value::Null, Value::str);
                (
                    ext.clone(),
                    obj(vec![
                        ("name", Value::str(name)),
                        ("origin", text(&s.origin)),
                        ("ref", Value::Object(s.reference.clone())),
                        ("insurance", s.insurance.clone()),
                        ("own_name", text(&s.own_name)),
                        ("asop_key", text(&s.asop_key)),
                        ("fitted", s.fitted.map_or(Value::Null, Value::Bool)),
                        ("unit", text(&s.unit)),
                    ]),
                )
            })
            .collect(),
    )
}

fn plan_json(plan: &Plan, wrong: bool) -> Value {
    let mut create = plan.create.clone();
    if wrong {
        create.push("gegenprobe".to_string());
    }
    obj(vec![
        ("link", pairs(&plan.link)),
        ("create", strings(&create)),
        ("update", pairs(&plan.update)),
        ("pull", pairs(&plan.pull)),
        ("remove", pairs(&plan.remove)),
        ("push_fields", field_lists(&plan.push_fields)),
        ("pull_fields", field_lists(&plan.pull_fields)),
        ("take", strings(&plan.take)),
        ("drop", strings(&plan.drop)),
        ("local_removed", map_of_strings(&plan.local_removed)),
        ("removed_elsewhere", strings(&plan.removed_elsewhere)),
        ("links", map_of_strings(&plan.links)),
        ("unit_conflicts", strings(&plan.unit_conflicts)),
        ("link_pull", strings(&plan.link_pull)),
        ("override", strings(&plan.override_)),
    ])
}

/// The id of a made-up request in the answer of the first case.
fn wrong_op_id(set: Value) -> Value {
    let Value::Object(mut map) = set else { return set };
    if let Some((_, Value::Array(ops))) = map.iter_mut().find(|(k, _)| k == "ops")
        && let Some(Value::Object(op)) = ops.first_mut()
    {
        jsonio::map_set(op, "opId", Value::str("gegenprobe"));
    }
    Value::Object(map)
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;

    let mut ids = Vec::new();
    for (i, item) in array(&input, "ids")?.iter().enumerate() {
        let entry = field(item, "entry")?.as_object().ok_or("entry")?;
        let mut id = ship_sync::external_id(entry);
        if gegenprobe && i == 0 {
            id = id.chars().rev().collect();
        }
        ids.push(obj(vec![("entry", field(item, "entry")?.clone()), ("id", Value::str(id))]));
    }

    let mut insurances = Vec::new();
    for (i, item) in array(&input, "insurance")?.iter().enumerate() {
        let entry = field(item, "entry")?.as_object().ok_or("entry")?;
        let mut cover = ship_sync::insurance(entry);
        if gegenprobe
            && i == 2
            && let Value::Object(map) = &mut cover
        {
            jsonio::map_set(map, "months", Value::int(99));
        }
        insurances.push(obj(vec![("entry", field(item, "entry")?.clone()), ("insurance", cover)]));
    }

    let first = field(field(&input, "local")?, "input")?;
    let fleet = entries(array(first, "fleet")?)?;
    let local = ship_sync::local_ships(&fleet, &own_names(field(first, "own")?));
    let local_block = obj(vec![("input", first.clone()), ("ships", local_json(&local, gegenprobe))]);

    let mut cases = Vec::new();
    for (index, case) in array(&input, "cases")?.iter().enumerate() {
        let given = field(case, "input")?;
        let fleet = entries(array(given, "fleet")?)?;
        let local = ship_sync::local_ships(&fleet, &own_names(field(given, "own")?));
        let resolved: BTreeMap<String, String> = field(given, "resolved")?
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect();
        let server: BTreeMap<String, Value> =
            field(given, "server")?.as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.clone())).collect();
        let state = ShipState::from_value(field(given, "state")?);
        let plan = ship_sync::plan(&local, &resolved, &server, &state);

        let mut changes = Vec::new();
        for (n, set) in array(case, "changes")?.iter().enumerate() {
            let option = field(set, "option")?;
            let batch = match field(option, "batch")? {
                Value::Int(n) => n.parse().map_err(|_| "batch")?,
                _ => return Err("batch ist keine Zahl".to_string()),
            };
            let new_unit = field(option, "new_unit")?.as_str();
            let known: Option<BTreeSet<String>> = match field(option, "known_units")? {
                Value::Array(units) => Some(units.iter().filter_map(|u| u.as_str().map(str::to_string)).collect()),
                _ => None,
            };
            let made = ship_sync::change_sets(&plan, &local, &resolved, &server, batch, new_unit, known.as_ref());
            let sets = made
                .into_iter()
                .map(|(set, keys)| {
                    let set = if gegenprobe && index == 1 && n == 0 { wrong_op_id(set) } else { set };
                    obj(vec![("set", set), ("keys", strings(&keys))])
                })
                .collect();
            changes.push(obj(vec![("option", option.clone()), ("sets", Value::Array(sets))]));
        }
        cases.push(obj(vec![
            ("input", given.clone()),
            ("local", local_json(&local, false)),
            ("plan", plan_json(&plan, gegenprobe && index == 0)),
            ("changes", Value::Array(changes)),
        ]));
    }

    let result = obj(vec![
        ("ids", Value::Array(ids)),
        ("insurance", Value::Array(insurances)),
        ("local", local_block),
        ("cases", Value::Array(cases)),
    ]);
    std::fs::write(out, jsonio::dumps(&result, None, false)).map_err(|e| e.to_string())
}
