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

//! Rust side of the check for mining and the refineries (`tools/parity_abbau.py`): the places and ores, the
//! scan signature both ways, the refinery table with the short names of its columns, the refineries per ore
//! and the processing methods, from the same file and the same typed values.

use std::fs;

use serde_json::{Value, json};
use versekit_core::jsonio;
use versekit_core::language::{Lang, with_language};
use versekit_core::mining_data::MiningData;
use versekit_core::{refining, salvage, ships};
use versekit_gamedata::craft_book::{self, CraftBook};
use versekit_gamedata::scmdb;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn texts(list: &Value, key: &str) -> Result<Vec<String>, String> {
    Ok(list
        .as_array()
        .ok_or(format!("{key} fehlt"))?
        .iter()
        .map(|case| case["query"].as_str().unwrap_or_default().to_string())
        .collect())
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let mut result = json!({});
    with_language(Lang::De, || -> Result<(), String> {
        let mining = MiningData::load(input["mining_file"].as_str().ok_or("mining_file fehlt")?);
        result = sections(&mining, &input)?;
        let made_up = MiningData::load(input["made_up_file"].as_str().ok_or("made_up_file fehlt")?);
        result["made_up"] = sections(&made_up, &input["made_up"])?;
        result["salvage"] = salvage_sections(&input["salvage"])?;
        Ok(())
    })?;
    if gegenprobe {
        // One share, one hit, one value of the table, one bonus and one method changed: all five must be reported.
        if let Some(share) = result["ores"][0]["places"][0].get_mut(3) {
            *share = json!(share.as_f64().unwrap_or(0.0) + 0.25);
        }
        if let Some(case) = result["find"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["hits"].as_array().is_some_and(|h| !h.is_empty())))
        {
            case["hits"][0][1] = json!(case["hits"][0][1].as_i64().unwrap_or(0) + 1);
        }
        if let Some(value) = result["made_up"]["matrix"]["rows"][0][1].get_mut(0) {
            *value = json!(value.as_i64().unwrap_or(0) + 1);
        }
        if let Some(case) = result["per_ore"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["rows"].as_array().is_some_and(|r| !r.is_empty())))
        {
            case["rows"][0][2] = json!(case["rows"][0][2].as_f64().unwrap_or(0.0) + 1.0);
        }
        result["recommend"][0]["best"] = json!("gegenprobe");
        // And one amount taken apart and one part of a loadout.
        if let Some(case) = result["salvage"]["dismantle"]
            .as_array_mut()
            .and_then(|c| c.iter_mut().find(|c| c["rows"].as_array().is_some_and(|r| !r.is_empty())))
        {
            case["rows"][0][1] = json!(case["rows"][0][1].as_f64().unwrap_or(0.0) + 1.0);
        }
        result["salvage"]["loadouts"][0]["parts"][0][5] = json!(99);
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The same value for the own JSON reader as the Python side wrote it.
fn own(value: &Value) -> Result<jsonio::Value, String> {
    jsonio::loads(&serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|_| "json".to_string())
}

/// The wrecks and taking apart: the loadout of the made-up ship files with its value, the code of erkul per
/// ship name and every craftable part taken apart, for the cases the Python side wrote.
fn salvage_sections(input: &Value) -> Result<Value, String> {
    let mut result = json!({});
    let prices = input["prices"].as_object().ok_or("prices fehlt")?;
    let mut loadouts = Vec::new();
    for case in input["loadouts"].as_array().ok_or("loadouts fehlt")? {
        let path = case["path"].as_str().unwrap_or_default();
        let raw = match input["ship_files"].get(path) {
            Some(file) => own(file)?,
            None => jsonio::Value::Null,
        };
        let parts = salvage::loadout(&raw);
        let (total, with, without) = salvage::value(&parts, |code| prices.get(code).and_then(Value::as_f64));
        let plain = |part: &salvage::Part| -> Result<Value, String> {
            let grade: Value =
                serde_json::from_str(&jsonio::dumps(&part.grade, None, false)).map_err(|e| e.to_string())?;
            Ok(json!([part.reference, part.name, part.kind, part.size, grade, part.count]))
        };
        loadouts.push(json!({
            "path": path,
            "parts": parts.iter().map(plain).collect::<Result<Vec<_>, _>>()?,
            "value": [total, with, without],
        }));
    }
    result["loadouts"] = json!(loadouts);
    let codes: Vec<String> =
        input["codes"].as_array().ok_or("codes fehlt")?.iter().filter_map(|c| c.as_str().map(str::to_string)).collect();
    result["codes"] = json!(codes);
    result["match"] = json!(
        input["match"]
            .as_array()
            .ok_or("match fehlt")?
            .iter()
            .map(|case| {
                let (name, maker) = (case[0].as_str().unwrap_or_default(), case[1].as_str().unwrap_or_default());
                json!([name, maker, ships::erkul_code(&codes, name, maker).unwrap_or_default()])
            })
            .collect::<Vec<_>>()
    );
    let kept = read(input["recipes_file"].as_str().ok_or("recipes_file fehlt")?)?;
    let build = kept["build"].as_str().map(str::to_string);
    let values = scmdb::values(build.as_deref(), &kept, &kept["products"]);
    let (share, seconds, blocked) = craft_book::dismantle_rules(&values);
    result["rules"] = json!([share, seconds, blocked]);
    let book = CraftBook::build(&values);
    let mut taken = Vec::new();
    for case in input["dismantle"].as_array().ok_or("dismantle fehlt")? {
        let item = case["item"].as_str().unwrap_or_default();
        let (rows, time) = match book.find(&values, item).filter(|r| !r.tiers.is_empty()) {
            Some(recipe) => (craft_book::dismantle(&values, recipe), seconds),
            None => (Vec::new(), 0),
        };
        taken.push(json!({
            "item": item,
            "seconds": time,
            "rows": rows.iter().map(|r| json!([r.material, r.inside, r.back, r.lost])).collect::<Vec<_>>(),
        }));
    }
    result["dismantle"] = json!(taken);
    Ok(result)
}

/// Everything compared, for one file of mining data and the cases the Python side wrote for it.
fn sections(mining: &MiningData, input: &Value) -> Result<Value, String> {
    let mut result = json!({});
    {
        result["places"] = json!(
            mining
                .places()
                .iter()
                .map(|p| json!({
                    "name": p.name, "vorlage": p.template, "system": p.system, "typ": p.kind,
                    "erze": p.ores.iter().map(|o| json!([o.name, o.kinds])).collect::<Vec<_>>(),
                    "anteile": p.ores.iter().map(|o| json!([o.name, o.share, o.level])).collect::<Vec<_>>(),
                    "je_geraet": p.per_tool.iter()
                        .map(|t| json!([t.tool, t.ores.iter().map(|(n, a, s)| json!([n, a, s])).collect::<Vec<_>>()]))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        result["ores"] = json!(
            mining
                .ores()
                .iter()
                .map(|ore| json!({
                    "name": ore.name,
                    "places": ore.places.iter()
                        .map(|p| json!([p.name, p.system, p.kinds, p.share, p.level,
                            p.per_tool.iter().map(|t| json!([t.tool, t.share, t.level, t.ores_there])).collect::<Vec<_>>()]))
                        .collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        result["find"] = json!(
            texts(&input["find"], "find")?
                .iter()
                .map(|q| json!({
                    "query": q,
                    "hits": mining.find_signature(q).iter()
                        .map(|h| json!([h.name, h.chunks, h.total, h.deviation])).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        result["signatures"] = json!(
            texts(&input["signatures"], "signatures")?
                .iter()
                .map(|q| json!({
                    "query": q,
                    "rows": mining.signatures_of(q).iter().map(|(n, s, m)| json!([n, s, m])).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        let (columns, rows) = refining::matrix(mining);
        result["matrix"] = json!({
            "columns": columns.iter().map(|c| json!([c.names, c.system])).collect::<Vec<_>>(),
            "rows": rows.iter().map(|r| json!([r.material, r.values, r.best])).collect::<Vec<_>>(),
        });
        result["short"] = json!(columns.iter().map(|c| refining::short_name(&c.names)).collect::<Vec<_>>());
        let mut heads: Vec<String> = columns.iter().map(|c| refining::head(&refining::short_name(&c.names))).collect();
        for extra in [
            "ARC-L1",
            "Checkmate",
            "Pyro-Gate",
            "Orbituary",
            "Stanton-Gate",
            "abcdefghijklmnop",
            "a b c d e f",
            "Ruin Station",
            "x",
        ] {
            heads.push(refining::head(extra));
        }
        result["head"] = json!(heads);
        result["groups"] = json!(refining::groups(&columns).iter().map(|(s, n)| json!([s, n])).collect::<Vec<_>>());
        let mut per_ore = Vec::new();
        for case in input["per_ore"].as_array().ok_or("per_ore fehlt")? {
            let ore = case["ore"].as_str().unwrap_or_default();
            let found = refining::for_ore(mining, ore);
            per_ore.push(json!({
                "ore": ore,
                "rows": found.iter().map(|(n, s, b)| json!([n, s, b])).collect::<Vec<_>>(),
                "box": found.iter().map(|(n, _, _)| { let (first, more) = refining::box_name(n); json!([first, more]) })
                    .collect::<Vec<_>>(),
            }));
        }
        result["per_ore"] = json!(per_ore);
        result["methods"] =
            json!(refining::METHODS.iter().map(|(key, name, levels)| json!([key, name, levels])).collect::<Vec<_>>());
        let mut recommended = Vec::new();
        for case in input["recommend"].as_array().ok_or("recommend fehlt")? {
            let (first, second) = (case["first"].as_str(), case["second"].as_str());
            let (best, all) = refining::recommend(first, second);
            recommended.push(json!({ "first": first, "second": second, "best": best, "all": all }));
        }
        result["recommend"] = json!(recommended);
        result["dominated"] = json!(refining::dominated().iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>());
    }
    Ok(result)
}
