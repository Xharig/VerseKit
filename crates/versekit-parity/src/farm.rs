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

//! Rust side of the check for "What I have to farm"
//! (`tools/parity_farm.py`): the mining data, the material of the farm notes
//! and of the parts of the ships (with the made-up slots of the Python side)
//! against a made-up stock and the places of what is missing, from the same
//! files, notes and ships. `--gegenprobe` also leaves the slots out, so the
//! parts of the ships go missing.

use std::fs;

use serde_json::{Value, json};
use versekit_core::cart;
use versekit_core::jsonio;
use versekit_core::language::{Lang, with_language};
use versekit_core::mining_data::MiningData;
use versekit_core::raw_prices::RawPrices;
use versekit_core::ship_market::Market;
use versekit_core::shops::Shops;
use versekit_core::stock_data::StockData;
use versekit_gamedata::cart_data::{self, CartData};
use versekit_gamedata::craft_book::{self, CraftBook};
use versekit_gamedata::farm::{self, Need};
use versekit_gamedata::scmdb;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

/// The same value for the own JSON reader as the Python side wrote it.
fn own(value: &Value) -> Result<jsonio::Value, String> {
    jsonio::loads(&serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|_| "json".to_string())
}

fn need_json(need: &Need) -> Value {
    json!({
        "rohstoff": need.material, "benoetigt": need.needed, "vorhanden": need.have,
        "differenz": need.missing, "mindestguete": need.min_quality, "zu_gering": need.too_low,
    })
}

/// A made-up slot as the Python side gives it to `cart.line_items` (`pfad`, `art`, `groesse`, `werk`).
fn slot_of(slot: &Value) -> cart::Slot {
    let text = |value: &Value| value.as_str().unwrap_or("").to_string();
    cart::Slot {
        path: text(&slot["pfad"]),
        kind: text(&slot["art"]),
        size: slot["groesse"].as_i64(),
        factory_ref: text(&slot["werk"]["ref"]),
        factory_name: text(&slot["werk"]["name"]),
    }
}

fn locations_json(needed: &[String], mining: &MiningData) -> Value {
    let found = farm::locations(needed, mining);
    let per_material: serde_json::Map<String, Value> = found
        .per_ore
        .iter()
        .map(|(material, spots)| {
            (material.clone(), json!(spots.iter().map(|(p, s, a)| json!([p, s, a])).collect::<Vec<_>>()))
        })
        .collect();
    json!({
        "needed": needed,
        "per_material": per_material,
        "gathering": found.gathering.iter().map(|g| json!([g.name, g.system, g.ores])).collect::<Vec<_>>(),
    })
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let kept = read(input["recipes_file"].as_str().ok_or("recipes_file fehlt")?)?;
    let build = kept["build"].as_str().map(str::to_string);
    let values = scmdb::values(build.as_deref(), &kept, &kept["products"]);
    let mut data = StockData::default();
    data.add_recipe_materials(craft_book::material_lines(&values));
    let mining = MiningData::load(input["mining_file"].as_str().ok_or("mining_file fehlt")?);
    let mut result = json!({});
    with_language(Lang::De, || -> Result<(), String> {
        result["ores"] = json!(
            mining
                .ores()
                .iter()
                .map(|ore| json!({
                    "name": ore.name,
                    "places": ore.places.iter()
                        .map(|p| json!([p.name, p.system, p.kinds.iter().collect::<Vec<_>>(), p.share, p.level]))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        let mut keys = serde_json::Map::new();
        for name in input["keys"].as_object().ok_or("keys fehlt")?.keys() {
            keys.insert(name.clone(), json!(mining.material_key(name)));
        }
        result["keys"] = Value::Object(keys);

        let book = CraftBook::build(&values);
        // The fleet file of a case (ships, wish list, notes) with the made-up slots of its ships, by name.
        let list_json = |fleet: &Value, slots: &Value, stock: &[jsonio::Value]| -> Result<Value, String> {
            let held = match own(fleet)? {
                jsonio::Value::Object(map) => map,
                _ => return Err("Flotte fehlt".into()),
            };
            let of_ship = |ship: &jsonio::Map, _chosen: &jsonio::Map| -> Vec<cart::Slot> {
                let name = ship.iter().find(|(k, _)| k == "name").and_then(|(_, v)| v.as_str()).unwrap_or("");
                slots[name].as_array().map(|listed| listed.iter().map(slot_of).collect()).unwrap_or_default()
            };
            // The counter check leaves the slots out: the parts of the ships must be missing then.
            let provider: Option<&cart_data::SlotProvider> = if gegenprobe { None } else { Some(&of_ship) };
            let posts = farm::planned_posts(&held, &values, &book, provider);
            let list = farm::farm_list(&values, &book, &posts, stock, &data);
            Ok(json!({
                "fehlt": list.missing.iter().map(need_json).collect::<Vec<_>>(),
                "vollstaendig": list.complete.iter().map(need_json).collect::<Vec<_>>(),
                "posten": list.posts,
                "ohne_rezept": list.without_recipe,
            }))
        };
        // The shopping list reads the recipes through the same stores as the page, here without prices.
        let (shops, raw, market) = (Shops::default(), RawPrices::default(), Market::default());
        let sources = CartData { values: &values, book: &book, shops: &shops, raw: &raw, market: &market, slots: None };
        let mut singles = Vec::new();
        for single in input["singles"].as_array().ok_or("singles fehlt")? {
            let (reference, name) =
                (single["note"]["ref"].as_str().unwrap_or(""), single["note"]["name"].as_str().unwrap_or(""));
            let option = cart::craft_option(reference, name, &sources);
            let craft = json!({
                "zustand": option.condition.as_str(), "material": option.material, "dauer": option.seconds,
                "bauplan": option.blueprint, "ohne_preis": option.unpriced,
            });
            let fleet = json!({ "merkzettel": [single["note"]] });
            singles.push(json!({
                "note": single["note"], "list": list_json(&fleet, &json!({}), &[])?, "craft": craft,
            }));
        }
        result["singles"] = json!(singles);
        let mut cases = Vec::new();
        for case in input["cases"].as_array().ok_or("cases fehlt")? {
            let stock = match own(&input["stocks"][case["stock"].as_str().ok_or("stock fehlt")?])? {
                jsonio::Value::Array(items) => items,
                _ => return Err("stock fehlt".into()),
            };
            cases.push(json!({
                "scenario": case["scenario"], "plan": case["plan"], "stock": case["stock"], "data": case["data"],
                "slots": case["slots"], "list": list_json(&case["data"], &case["slots"], &stock)?,
            }));
        }
        result["cases"] = json!(cases);
        let mut places = Vec::new();
        for case in input["locations"].as_array().ok_or("locations fehlt")? {
            let needed: Vec<String> = case["needed"]
                .as_array()
                .ok_or("needed fehlt")?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            places.push(locations_json(&needed, &mining));
        }
        result["locations"] = json!(places);
        Ok(())
    })?;
    if gegenprobe {
        // One share, one amount, one place, one key and one count changed: the comparison must report all five.
        if let Some(share) = result["ores"][0]["places"][0].get_mut(3) {
            *share = json!(share.as_f64().unwrap_or(0.0) + 0.25);
        }
        if let Some(first) = result["cases"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["list"]["fehlt"].as_array().is_some_and(|rows| !rows.is_empty())))
        {
            first["list"]["fehlt"][0]["differenz"] =
                json!(first["list"]["fehlt"][0]["differenz"].as_f64().unwrap_or(0.0) + 1.0);
        }
        if let Some(case) = result["locations"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["gathering"].as_array().is_some_and(|g| !g.is_empty())))
        {
            case["gathering"][0][0] = json!("Gegenprobe");
        }
        if let Some(single) = result["singles"]
            .as_array_mut()
            .and_then(|s| s.iter_mut().find(|s| s["list"]["posten"].as_i64() == Some(1)))
        {
            single["list"]["posten"] = json!(2);
        }
        if let Some(entry) = result["keys"].as_object_mut().and_then(|k| k.values_mut().next()) {
            *entry = json!("gegenprobe");
        }
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
