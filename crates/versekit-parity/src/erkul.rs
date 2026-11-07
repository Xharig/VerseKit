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

//! Rust side of the check for the slots of the ships
//! (`tools/parity_erkul.py`): the single slots, the change after a rack
//! swap, the rule which part fits a slot, the factory fitting, the count of
//! slots of a kind and size and the ships a part goes into, from the same
//! two stored files.
//!
//! ⚠ Wanted deviation from 4.12 (since 5.2.0): the program takes the slots of
//! the ships from the game data first (`spiel-schiffe.json`, `spiel-teile.json`,
//! built with p4kx from the `Data.p4k`: every player ship, turret → guns, mining
//! head → modules, rack → missiles) and erkul's only where those have nothing
//! (`ErkulData::load`, `ErkulShips::merged`). 4.12 knows only erkul. This check
//! compares the reading of erkul's two files alone, which stays as 4.12's; the
//! game data are checked by `ship_slots` in versekit-gamedata and the checks
//! over all ships (`jeder_platz_aller_schiffe`, `rundlauf_alle_schiffe`).

use std::fs;

use serde_json::{Value, json};
use versekit_core::erkul_fetch;
use versekit_core::erkul_slots::{self, ErkulParts, Fit, SlotInfo};
use versekit_core::hangar;
use versekit_core::jsonio;
use versekit_core::part_choices::{self, CraftEntry, ShopPart, Sources as ChoiceSources, Traits};
use versekit_core::ships::{ErkulShips, ShipId, UexShips};

fn plain(value: &jsonio::Value) -> Result<Value, String> {
    serde_json::from_str(&jsonio::dumps(value, None, false)).map_err(|e| e.to_string())
}

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn own_map(value: &Value) -> Result<jsonio::Map, String> {
    match jsonio::loads(&serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|_| "json".to_string())? {
        jsonio::Value::Object(map) => Ok(map),
        _ => Err("Objekt erwartet".into()),
    }
}

fn fit_json(fit: &Fit) -> Value {
    json!({
        "typen": fit.types.iter().map(|(t, s)| json!([t, s])).collect::<Vec<_>>(),
        "min": fit.min, "max": fit.max, "porttags": fit.port_tags, "verlangt": fit.required,
    })
}

fn slot_json(slot: &SlotInfo) -> Value {
    json!({
        "pfad": slot.path, "art": slot.kind, "groesse": slot.size,
        "passt": slot.fit.as_ref().map(fit_json),
        "werk": slot.factory.as_ref().map(|(r, n)| json!({ "ref": r, "name": n })),
    })
}

fn texts(value: &Value) -> [String; 4] {
    let at = |k: usize| value[k].as_str().unwrap_or("").to_string();
    [at(0), at(1), at(2), at(3)]
}

fn id_of(text: &[String; 4]) -> ShipId<'_> {
    ShipId { name: &text[0], maker: &text[1], short: &text[2], maker_short: &text[3] }
}

fn size_of(value: &Value) -> Option<i64> {
    value.as_i64()
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let home = std::env::temp_dir().join(format!("parity-erkul-rs-{}", versekit_core::testdir::run_tag()));
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let (ships_file, parts_file) = (home.join("erkul-schiffe.json"), home.join("erkul-teile.json"));
    // The files are written with the own reader, which keeps the order of the keys: the order of the parts matters.
    let raw = jsonio::loads(&fs::read_to_string(python_json).map_err(|e| e.to_string())?).map_err(|_| "json")?;
    let world = raw.get("world").ok_or("world fehlt")?;
    for (name, file) in [("schiffe", &ships_file), ("teile", &parts_file)] {
        let data = world.get(name).ok_or("Datei fehlt")?;
        fs::write(file, jsonio::dumps(data, None, false)).map_err(|e| e.to_string())?;
    }
    let ships = ErkulShips::load(&ships_file.to_string_lossy());
    let parts = ErkulParts::load(&parts_file.to_string_lossy());
    let no_choice = jsonio::Map::new();

    let mut result = json!({});
    let mut listed = Vec::new();
    for case in input["ships"].as_array().ok_or("ships fehlt")? {
        let text = texts(&case["id"]);
        let id = id_of(&text);
        listed.push(json!({
            "id": case["id"], "knows": ships.knows(id),
            "slots": erkul_slots::hardpoints(&ships, &parts, id, &no_choice).iter().map(slot_json).collect::<Vec<_>>(),
            "stock": erkul_slots::stock_loadout(&ships, &parts, id).iter().map(|s| json!({
                "ref": s.reference, "name": s.name, "art": s.kind, "groesse": s.size, "anzahl": s.count,
            })).collect::<Vec<_>>(),
        }));
    }
    result["ships"] = json!(listed);

    let mut choices = Vec::new();
    for case in input["choices"].as_array().ok_or("choices fehlt")? {
        let text = texts(&case["id"]);
        let chosen = own_map(&case["chosen"])?;
        choices.push(json!({
            "id": case["id"], "chosen": case["chosen"],
            "slots": erkul_slots::hardpoints(&ships, &parts, id_of(&text), &chosen).iter().map(slot_json).collect::<Vec<_>>(),
        }));
    }
    result["choices"] = json!(choices);

    let mut compatible = Vec::new();
    for case in input["compatible"].as_array().ok_or("compatible fehlt")? {
        let text = texts(&case["id"]);
        let slots = erkul_slots::hardpoints(&ships, &parts, id_of(&text), &no_choice);
        let slot = slots.iter().find(|s| Some(s.path.as_str()) == case["pfad"].as_str()).ok_or("Platz fehlt")?;
        compatible.push(json!({
            "id": case["id"], "pfad": case["pfad"],
            "refs": parts.compatible(slot).iter().map(|p| p.reference.clone()).collect::<Vec<_>>(),
        }));
    }
    result["compatible"] = json!(compatible);

    let mut fits = Vec::new();
    for case in input["fits"].as_array().ok_or("fits fehlt")? {
        let text = texts(&case["id"]);
        let kind = case["kind"].as_str().unwrap_or("");
        fits.push(json!({
            "kind": case["kind"], "size": case["size"], "id": case["id"],
            "count": erkul_slots::fits(&ships, kind, size_of(&case["size"]), id_of(&text)),
        }));
    }
    result["fits"] = json!(fits);

    let hangar_entries: Vec<jsonio::Map> =
        input["world"]["hangar"].as_array().ok_or("hangar fehlt")?.iter().map(own_map).collect::<Result<_, _>>()?;
    let entries: Vec<&jsonio::Map> = hangar_entries.iter().collect();
    let mut matching = Vec::new();
    for case in input["matching"].as_array().ok_or("matching fehlt")? {
        let kind = case["kind"].as_str().unwrap_or("");
        let found = erkul_slots::matching_ships(&ships, kind, size_of(&case["size"]), &entries);
        matching.push(json!({
            "kind": case["kind"], "size": case["size"], "found": found.iter().map(|(n, c)| json!([n, c])).collect::<Vec<_>>(),
        }));
    }
    result["matching"] = json!(matching);

    let eclipse_id: [String; 4] = ["Eclipse".into(), String::new(), String::new(), String::new()];
    let slots = erkul_slots::hardpoints(&ships, &parts, id_of(&eclipse_id), &no_choice);
    let mut rule = Vec::new();
    for case in input["rule"].as_array().ok_or("rule fehlt")? {
        let slot = slots.iter().find(|s| Some(s.path.as_str()) == case["pfad"].as_str()).ok_or("Platz fehlt")?;
        let mut accepted = Vec::new();
        if let Some(fit) = &slot.fit {
            for part in parts.all() {
                if erkul_slots::fits_slot(fit, part) {
                    accepted.push(part.reference.clone());
                }
            }
        }
        rule.push(json!({ "pfad": case["pfad"], "passt": slot.fit.is_some(), "parts": accepted }));
    }
    result["rule"] = json!(rule);
    // Which parts go into a slot.
    let text_of = |v: &Value, key: &str| v[key].as_str().unwrap_or("").to_string();
    let craft: Vec<CraftEntry> = input["world"]["craft"]
        .as_array()
        .ok_or("craft fehlt")?
        .iter()
        .map(|e| CraftEntry {
            entity: text_of(e, "entity"),
            basis: text_of(e, "basis"),
            name: text_of(e, "name"),
            maker: text_of(e, "hersteller"),
            recipe_kind: text_of(e, "art"),
            recipe_subtype: text_of(e, "unterart"),
        })
        .collect();
    let mut traits = std::collections::HashMap::new();
    for (name, t) in input["world"]["traits"].as_object().ok_or("traits fehlt")? {
        traits.insert(
            versekit_core::paths::name_key(name),
            Traits {
                kind: text_of(t, "a"),
                size: t["s"].as_i64(),
                maker: text_of(t, "m"),
                grade: t["g"].as_i64(),
                class: text_of(t, "c"),
            },
        );
    }
    let shop: Vec<ShopPart> = input["world"]["shop"]
        .as_array()
        .ok_or("shop fehlt")?
        .iter()
        .map(|e| ShopPart {
            code: text_of(e, "kennung"),
            name: text_of(e, "name"),
            group: text_of(e, "kategorie"),
            maker: text_of(e, "hersteller"),
            size: text_of(e, "groesse"),
            class: text_of(e, "klasse"),
            grade: text_of(e, "guete"),
        })
        .collect();
    let sources = ChoiceSources { craft: &craft, traits: &traits, shop: &shop, parts: &parts };
    let mut picked = Vec::new();
    for case in input["pick"].as_array().ok_or("pick fehlt")? {
        let kind = case["kind"].as_str().unwrap_or("");
        let slot = match (case["id"].as_array(), case["pfad"].as_str()) {
            (Some(_), Some(path)) => {
                let text = texts(&case["id"]);
                let all = erkul_slots::hardpoints(&ships, &parts, id_of(&text), &no_choice);
                Some(all.into_iter().find(|s| s.path == path).ok_or("Platz fehlt")?)
            }
            _ => None,
        };
        let found = part_choices::choices(&sources, kind, size_of(&case["size"]), slot.as_ref());
        picked.push(json!({
            "kind": case["kind"], "size": case["size"], "pfad": case["pfad"], "id": case["id"],
            "found": found.iter().map(|c| json!({
                "name": c.name, "kennung": c.reference, "hersteller": c.maker, "guete": c.grade,
                "klasse": c.class, "herkunft": c.origin.as_str(),
            })).collect::<Vec<_>>(),
        }));
    }
    result["pick"] = json!(picked);

    // The maker UEX has in the name, and the wish list.
    let uex_file = home.join("schiffe.json");
    fs::write(&uex_file, input["world"]["uex"].to_string()).map_err(|e| e.to_string())?;
    let uex = UexShips::load(&uex_file.to_string_lossy());
    result["makers"] = json!(
        input["makers"]
            .as_array()
            .ok_or("makers fehlt")?
            .iter()
            .map(|m| json!({ "name": m["name"], "maker": uex.manufacturer(m["name"].as_str().unwrap_or("")) }))
            .collect::<Vec<_>>()
    );
    let mut data = own_map(&input["wish"]["start"])?;
    let mut steps = Vec::new();
    for step in input["wish"]["steps"].as_array().ok_or("wish fehlt")? {
        let op = step["op"].as_array().ok_or("op fehlt")?;
        let arg = |k: usize| op.get(k).and_then(Value::as_str).unwrap_or("");
        let outcome = match arg(0) {
            "add" => json!(hangar::wishlist_add(&mut data, arg(1), arg(2))),
            "remove" => json!(hangar::wishlist_remove(&mut data, arg(1))),
            "contains" => json!(hangar::wishlist_contains(&data, arg(1))),
            "in_hangar" => json!(hangar::in_hangar(&data, arg(1), "", "", "")),
            _ => json!(
                hangar::wishlist(&data).iter().map(|w| hangar::field_text(w, "name").to_string()).collect::<Vec<_>>()
            ),
        };
        let wunsch = match data.iter().find(|(k, _)| k == "wunsch") {
            Some((_, v)) => plain(v)?,
            None => Value::Null,
        };
        steps.push(json!({
            "op": step["op"], "result": outcome, "wunsch": wunsch,
            "list": hangar::wishlist(&data).iter().map(|w| hangar::field_text(w, "name").to_string()).collect::<Vec<_>>(),
        }));
    }
    result["wish"] = json!({ "start": input["wish"]["start"], "steps": steps });
    let mut interesting: Vec<&str> = erkul_slots::INTERESTING.to_vec();
    interesting.sort_unstable();
    result["interesting"] = json!(interesting);
    // The ship files: the stored answer is packed as erkul packs it and handed over through a made-up feed, so
    // the way in (raw deflate, then JSON) is part of the check. Nothing leaves the process.
    let mut fetched = Vec::new();
    for case in input["fetch"].as_array().ok_or("fetch fehlt")? {
        let packed = erkul_fetch::pack(&case["raw"].to_string());
        let feed = erkul_fetch::FnFeed(move |_: &str| Some(packed.clone()));
        let id = case["id"].as_str().unwrap_or_default();
        let path = case["path"].as_str().unwrap_or_default();
        let reduced = match erkul_fetch::fetch_ship(&feed, id, path) {
            Some(value) => plain(&value)?,
            None => Value::Null,
        };
        fetched.push(json!({ "id": id, "path": path, "raw": case["raw"], "result": reduced }));
    }
    result["fetch"] = json!(fetched);
    let mut slim = Vec::new();
    for case in input["slim"].as_array().ok_or("slim fehlt")? {
        let raw = jsonio::loads(&case["raw"].to_string()).map_err(|e| e.to_string())?;
        let reduced = match erkul_fetch::slim_part(&raw) {
            Some(value) => plain(&value)?,
            None => Value::Null,
        };
        slim.push(json!({ "raw": case["raw"], "result": reduced }));
    }
    result["slim"] = json!(slim);
    let mut refreshed = Vec::new();
    for case in input["refresh"].as_array().ok_or("refresh fehlt")? {
        let files: Vec<(String, Vec<u8>)> = case["files"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(path, data)| (path.clone(), erkul_fetch::pack(&data.to_string())))
            .collect();
        let feed = erkul_fetch::FnFeed(move |path: &str| files.iter().find(|(p, _)| p == path).map(|(_, b)| b.clone()));
        let catalog = jsonio::loads(&case["catalog"].to_string()).map_err(|e| e.to_string())?;
        let saved = match erkul_fetch::refresh_parts(&feed, &catalog, case["stored"].as_str().unwrap_or_default()) {
            Some(value) => plain(&value)?,
            None => Value::Null,
        };
        refreshed.push(json!({
            "name": case["name"], "catalog": case["catalog"], "files": case["files"],
            "stored": case["stored"], "saved": saved,
        }));
    }
    result["refresh"] = json!(refreshed);
    let _ = fs::remove_dir_all(&home);
    if gegenprobe {
        // One value changed in each of the sections: the comparison must report every one of them.
        result["ships"][2]["slots"][0]["groesse"] = json!(99);
        result["ships"][2]["stock"][0]["anzahl"] = json!(99);
        result["choices"][1]["slots"] = json!([]);
        result["rule"][5]["parts"] = json!([]);
        result["fits"][0]["count"] = json!(99);
        result["pick"][0]["found"][0]["herkunft"] = json!("gegenprobe");
        result["makers"][0]["maker"] = json!("gegenprobe");
        result["wish"]["steps"][0]["result"] = json!("gegenprobe");
        result["interesting"][0] = json!("gegenprobe");
        result["fetch"][0]["result"]["plaetze"][0]["anzahl"] = json!(99);
        result["slim"][0]["result"]["name"] = json!("gegenprobe");
        result["refresh"][0]["saved"]["spielversion"] = json!("gegenprobe");
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
