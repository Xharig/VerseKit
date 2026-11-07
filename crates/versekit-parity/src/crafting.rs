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

//! Rust side of the check for the values of crafted items
//! (`tools/parity_crafting.py`): the same recipe and error files, the same
//! qualities, the same output.

use std::collections::BTreeMap;
use std::fs;

use serde_json::{Map, Value, json};
use versekit_gamedata::corrections::{self, Field, Outcome};
use versekit_gamedata::quality::{self, Factors};
use versekit_gamedata::scmdb;
use versekit_gamedata::values::{ItemValues, Recipe};

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

/// The same formula as `mixed_quality` on the Python side.
fn mixed_quality(material: &str) -> f64 {
    let sum: u64 = material.bytes().map(u64::from).sum();
    ((sum * 7) % 1001) as f64
}

fn shown(item: Option<&ItemValues>) -> Value {
    let Some(item) = item else { return Value::Null };
    let weapon = item.weapon.as_ref();
    let (damage, detonation) = quality::damage(weapon.and_then(|w| w.ammo.as_ref()));
    let modes: Vec<Value> = weapon
        .map(|w| w.fire_modes.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|mode| {
            let entry = mode.shown();
            let rate = entry.fire_rate.filter(|v| *v != 0.0).or(entry.delay);
            let recoil = entry.recoil.unwrap_or_default();
            json!({ "rate": rate, "recoil": [recoil.yaw_max_deg, recoil.pitch_max_deg, recoil.smooth_time] })
        })
        .collect();
    let stats: Map<String, Value> =
        item.stats.iter().map(|(stat, value)| (stat.scmdb_field().to_string(), json!(value))).collect();
    let armor = item.armor.clone().unwrap_or_default();
    json!({
        "dps": quality::dps(item),
        "damage": damage.map(|d| d.total()),
        "detonation": damage.is_some() && detonation,
        "modes": modes,
        "stats": stats,
        "beams": item.beams,
        "resistance": armor.resistance,
        "temperature": armor.temperature.map(|(min, max)| [min, max]),
        "dissipation": armor.radiation_dissipation,
    })
}

fn factors_json(factors: &Factors) -> (Value, Value) {
    (json!(factors.mul), json!(factors.pips))
}

fn curves(recipe: &Recipe, qualities: &[f64]) -> Vec<Value> {
    let mut found = Vec::new();
    for slot in recipe.tiers.first().map(|tier| tier.slots.as_slice()).unwrap_or_default() {
        let mut groups: Vec<(&str, Vec<_>)> = Vec::new();
        for modifier in &slot.modifiers {
            match groups.iter_mut().find(|(key, _)| *key == modifier.property_key) {
                Some((_, ranges)) => ranges.push(modifier),
                None => groups.push((&modifier.property_key, vec![modifier])),
            }
        }
        for (key, ranges) in groups {
            let reach = quality::range_of(&ranges)
                .map(|r| json!([r.quality_from, r.quality_to, r.factor_from, r.factor_to, r.base]));
            found.push(json!({
                "slot": slot.name,
                "key": key,
                "factors": qualities.iter().map(|q| quality::factor(&ranges, *q)).collect::<Vec<_>>(),
                "higher": quality::higher_is_better(&ranges),
                "absolute": quality::is_absolute(&ranges),
                "reach": reach,
            }));
        }
    }
    found
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let recipes_file = input["recipes_file"].as_str().ok_or("recipes_file fehlt")?;
    let kept = read(recipes_file)?;
    let build = kept["build"].as_str().map(str::to_string);
    let mut values = scmdb::values(build.as_deref(), &kept, &kept["products"]);
    let mut product_fixes = BTreeMap::new();
    if input["issues_applied"].as_bool().unwrap_or(false)
        && let Some(issues_file) = input["issues_file"].as_str()
        && let Some(build) = build.as_deref()
    {
        let issues = read(issues_file)?;
        let list = corrections::from_scmdb(&issues["daten"], build);
        let report = corrections::apply(&mut values, &list);
        for (correction, outcome) in &report.outcomes {
            if *outcome == Outcome::Applied && correction.field == Field::Product {
                product_fixes.insert(correction.record.clone(), "entityClass");
            }
        }
    }
    let qualities: Vec<f64> =
        input["qualities"].as_array().ok_or("qualities fehlt")?.iter().filter_map(Value::as_f64).collect();
    let mut entities = Map::new();
    for (guid, item) in &values.items {
        if !item.corrected.is_empty() {
            let mut fields: Vec<&str> = item.corrected.iter().map(|c| c.field.as_str()).collect();
            fields.sort_unstable();
            entities.insert(guid.clone(), json!(fields));
        }
    }
    let mut blueprints = Vec::new();
    for recipe in &values.recipes {
        let item = values.product(recipe);
        let mut scenarios = Vec::new();
        for q in &qualities {
            let factors = quality::combined(recipe, |_, _| Some(*q));
            let (mul, pips) = factors_json(&factors);
            let made = item.map(|item| quality::crafted(item, &factors));
            scenarios.push(json!({ "q": q, "mul": mul, "pips": pips, "made": shown(made.as_ref()) }));
        }
        let factors = quality::combined(recipe, |_, slot| Some(mixed_quality(slot.material())));
        let (mul, pips) = factors_json(&factors);
        let made = item.map(|item| quality::crafted(item, &factors));
        scenarios.push(json!({ "q": "mixed", "mul": mul, "pips": pips, "made": shown(made.as_ref()) }));
        let mut corrected = Vec::new();
        if let Some(field) = product_fixes.get(&recipe.guid) {
            corrected.push(json!(["recipe", [field]]));
        }
        for slot in recipe.tiers.iter().flat_map(|tier| &tier.slots) {
            for modifier in slot.modifiers.iter().filter(|m| !m.corrected.is_empty()) {
                let raw: Map<String, Value> =
                    modifier.corrected.iter().map(|c| (c.field.clone(), json!(c.raw.parse::<f64>().ok()))).collect();
                corrected.push(json!([slot.name, modifier.property_key, raw]));
            }
        }
        blueprints.push(json!({
            "guid": recipe.guid,
            "tag": recipe.tag,
            "product": item.map(|item| item.guid.clone()),
            "base": shown(item),
            "scenarios": scenarios,
            "curves": curves(recipe, &qualities),
            "corrected": corrected,
        }));
    }
    if gegenprobe {
        // One factor changed, one scenario dropped, one correction lost and
        // one damage value changed: the comparison must report all four.
        let with_mul = blueprints
            .iter_mut()
            .skip(1)
            .find(|b| b["scenarios"][8]["mul"].as_object().is_some_and(|m| !m.is_empty()))
            .ok_or("Gegenprobe: kein Rezept mit Faktoren")?;
        if let Some(mul) = with_mul["scenarios"][8]["mul"].as_object_mut()
            && let Some(first) = mul.values_mut().next()
        {
            *first = json!(first.as_f64().unwrap_or(1.0) + 0.001);
        }
        blueprints[0]["scenarios"].as_array_mut().ok_or("Gegenprobe: keine Fälle")?.pop();
        let with_mark = blueprints
            .iter_mut()
            .find(|b| b["corrected"].as_array().is_some_and(|c| !c.is_empty()))
            .ok_or("Gegenprobe: keine Berichtigung")?;
        with_mark["corrected"] = json!([]);
        let with_damage = blueprints
            .iter_mut()
            .rev()
            .find(|b| b["base"]["damage"].as_f64().is_some_and(|damage| damage > 0.0))
            .ok_or("Gegenprobe: kein Schaden")?;
        with_damage["base"]["damage"] = json!(with_damage["base"]["damage"].as_f64().unwrap_or(0.0) * 1.01);
    }
    let result = json!({ "build": values.build, "entities": entities, "blueprints": blueprints });
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
