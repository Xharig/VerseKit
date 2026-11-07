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

//! Rust side of the check for the crafting page
//! (`tools/parity_craftbook.py`): the list of everything craftable, the
//! categories, what a recipe asks of the stock and the table of product
//! values, from the same recipe file and the same made-up stock.

use std::collections::HashSet;
use std::fs;

use serde_json::{Value, json};
use versekit_core::blueprints::classify_recipe;
use versekit_core::jsonio;
use versekit_core::language::{self, Lang};
use versekit_core::stock::number_of;
use versekit_core::stock_data::StockData;
use versekit_core::stock_use::{self, Ingredient, Order};
use versekit_gamedata::craft_book::{self, CraftBook, Have};
use versekit_gamedata::product_table::{self, Better, Line, Rating};
use versekit_gamedata::quality;
use versekit_gamedata::scmdb;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn mixed_quality(material: &str) -> f64 {
    let sum: u64 = material.bytes().map(u64::from).sum();
    ((sum * 7) % 1001) as f64
}

/// The fingerprint of a stock, the same text and hash as the Python side.
fn digest(entries: &[jsonio::Value]) -> String {
    let text: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "{}|{:.6}|{:.6}",
                e.get("material").and_then(jsonio::Value::as_str).unwrap_or_default(),
                number_of(e, "qualitaet"),
                number_of(e, "menge")
            )
        })
        .collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.join(";").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn have_json(have: Have) -> Value {
    match have {
        Have::Yes => json!(true),
        Have::No => json!(false),
        Have::Unclear => Value::Null,
    }
}

fn better_text(better: Better) -> &'static str {
    match better {
        Better::High => "high",
        Better::Low => "low",
        Better::Absolute => "abs",
        Better::Dps => "dps",
    }
}

fn lines_json(lines: &[Line]) -> Vec<Value> {
    lines
        .iter()
        .map(|line| match line {
            Line::Info(text) => json!(["info", text]),
            Line::Section(title) => json!(["section", title]),
            Line::Row(row) => {
                let shown = product_table::formatted(row);
                let rating = match shown.rating {
                    Rating::Good => "good",
                    Rating::Bad => "bad",
                    Rating::Neutral => "neutral",
                };
                json!([
                    "row",
                    row.label,
                    row.base,
                    row.made,
                    row.unit,
                    row.decimals,
                    better_text(row.better),
                    [shown.base, shown.made, shown.change, rating]
                ])
            }
        })
        .collect()
}

fn ingredient_json(i: &Ingredient) -> Value {
    json!([i.slot, i.material, i.amount, i.quality])
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let kept = read(input["recipes_file"].as_str().ok_or("recipes_file fehlt")?)?;
    let build = kept["build"].as_str().map(str::to_string);
    let values = scmdb::values(build.as_deref(), &kept, &kept["products"]);
    let mut data = StockData::default();
    data.add_recipe_materials(craft_book::material_lines(&values));
    let have: HashSet<String> =
        input["have"].as_array().ok_or("have fehlt")?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let mut result = json!({});
    language::with_language(Lang::De, || -> Result<(), String> {
        let book = CraftBook::build(&values);
        let marks = book.with_collection(&have);
        result["book"] = json!(
            book.items
                .iter()
                .zip(&marks)
                .map(|(item, mark)| json!({
                    "basis": item.basis, "name": item.name, "maker": item.manufacturer, "type": item.kind,
                    "subtype": item.subtype, "tiers": item.tiers, "tag": item.tag, "tags": item.tags,
                    "entity": item.entity, "have": have_json(*mark),
                }))
                .collect::<Vec<_>>()
        );
        let (sure, all, unclear) = book.counts(&have);
        result["counts"] = json!([sure, all, unclear]);
        let mut classes = serde_json::Map::new();
        for recipe in &values.recipes {
            let kind = values.product(recipe).and_then(|i| i.cg_item_type.clone()).unwrap_or_default();
            let (top, sub) = classify_recipe(&kind, &recipe.tag, &recipe.subtype, &recipe.kind);
            classes.insert(recipe.tag.clone(), json!([top, sub]));
        }
        result["classes"] = Value::Object(classes);
        let mut recipes = serde_json::Map::new();
        for item in &book.items {
            let Some(recipe) = book.find(&values, &item.tag) else {
                recipes.insert(item.tag.clone(), Value::Null);
                continue;
            };
            let tiers = craft_book::ingredients(recipe);
            recipes.insert(
                item.tag.clone(),
                json!({
                    "time": recipe.tiers.iter().map(|t| t.craft_time_seconds.unwrap_or(0)).collect::<Vec<_>>(),
                    "ingredients": tiers.iter().map(|t| t.iter().map(ingredient_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
                }),
            );
        }
        result["recipes"] = Value::Object(recipes);
        result["materials"] = json!(data.material_names());
        let mut with = serde_json::Map::new();
        for name in data.material_names() {
            with.insert(name.clone(), json!(craft_book::blueprints_with(&values, name)));
        }
        result["with"] = Value::Object(with);
        let (lost, efficiency) = craft_book::dismantle_lost(&values);
        result["dismantle"] = json!([lost.iter().collect::<Vec<_>>(), efficiency, values.dismantle.seconds]);

        let stock_text = serde_json::to_string(&input["stock"]).map_err(|e| e.to_string())?;
        let stock = match jsonio::loads(&stock_text).map_err(|_| "stock".to_string())? {
            jsonio::Value::Array(items) => items,
            _ => return Err("stock fehlt".into()),
        };
        let mut cases = Vec::new();
        for item in &book.items {
            let Some(recipe) = book.find(&values, &item.tag) else { continue };
            let tiers = craft_book::ingredients(recipe);
            let Some(ingredients) = tiers.first().filter(|t| !t.is_empty()) else { continue };
            for count in [1_i64, 3] {
                let check: Vec<Value> = stock_use::check(&stock, &data, ingredients, count)
                    .iter()
                    .map(|s| json!([s.material, s.needed, s.there, s.missing, s.too_low, s.quality]))
                    .collect();
                let mut case = json!({ "tag": item.tag, "count": count, "check": check });
                for (key, order) in [("none", None), ("best", Some(Order::Best)), ("worst", Some(Order::Worst))] {
                    let wanted: Vec<(String, Order)> = order
                        .map(|o| ingredients.iter().map(|i| (i.material.clone(), o)).collect())
                        .unwrap_or_default();
                    let mut work = stock.clone();
                    let outcome = stock_use::deduct(&mut work, &data, ingredients, count, &wanted);
                    case[format!("deduct_{key}")] = match outcome {
                        Ok(()) => json!({ "ok": true, "missing": [], "left": digest(&work) }),
                        Err(missing) => json!({
                            "ok": false,
                            "missing": missing.iter().map(|(n, s)| json!([n, s])).collect::<Vec<_>>(),
                            "left": Value::Null,
                        }),
                    };
                }
                let mut quality_map = serde_json::Map::new();
                for i in ingredients {
                    let needed = i.amount * count as f64;
                    let qf = |best: bool| {
                        stock_use::quality_for(&stock, &data, &i.material, needed, i.quality, best, Some(i.amount))
                    };
                    let (with_good, with_low) = stock_use::amount_with_quality(&stock, &i.material, i.quality);
                    quality_map.insert(
                        i.material.clone(),
                        json!({
                            "best": qf(true), "worst": qf(false),
                            "best_all": stock_use::best_quality(&stock, &i.material, i.quality),
                            "worst_all": stock_use::worst_quality(&stock, &i.material, i.quality),
                            "with": [with_good, with_low],
                        }),
                    );
                }
                case["quality"] = Value::Object(quality_map);
                cases.push(case);
            }
        }
        result["stock_cases"] = json!(cases);

        let mut tables = serde_json::Map::new();
        for recipe in &values.recipes {
            let product = values.product(recipe);
            for scenario in ["default", "q1000", "q0", "mixed"] {
                let factors = quality::combined(recipe, |_, slot| match scenario {
                    "q1000" => Some(1000.0),
                    "q0" => Some(0.0),
                    "mixed" => Some(mixed_quality(slot.material())),
                    _ => None,
                });
                let lines = product.map(|item| product_table::table(item, &factors)).unwrap_or_default();
                tables.insert(format!("{}/{scenario}", recipe.tag), json!(lines_json(&lines)));
            }
        }
        result["tables"] = Value::Object(tables);
        Ok(())
    })?;
    if gegenprobe {
        // One name, one count, one deducted fingerprint and one table value
        // changed: the comparison must report all four.
        result["book"][0]["name"] = json!("Gegenprobe");
        result["counts"][0] = json!(result["counts"][0].as_u64().unwrap_or(0) + 1);
        if let Some(case) = result["stock_cases"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["deduct_best"]["ok"] == json!(true)))
        {
            case["deduct_best"]["left"] = json!("0000000000000000");
        }
        if let Some(row) = result["tables"]
            .as_object_mut()
            .into_iter()
            .flat_map(|t| t.values_mut())
            .flat_map(|lines| lines.as_array_mut().into_iter().flatten())
            .find(|l| l[0] == json!("row"))
        {
            row[2] = json!(row[2].as_f64().unwrap_or(0.0) + 1.0);
        }
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
