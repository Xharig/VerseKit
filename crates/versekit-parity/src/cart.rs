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

//! Rust side of the check for the calculating half of the shopping list
//! (`tools/parity_cart.py`): the two ways of a part, the bill, the total, the
//! route, grouping and the edits of the equipment, on the made-up world of the
//! Python side. The shop prices and the prices of the raw materials are read
//! from the same two files by the real stores.

use std::collections::HashMap;
use std::fs;

use serde_json::{Value, json};
use versekit_core::cart::{
    self, BUY, BuyOption, CraftOption, Ingredient, Kind, Line, Offer, Origin, Recipe, Slot, Sources, Step,
};
use versekit_core::hangar::field_text;
use versekit_core::jsonio::{self, Map};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::raw_prices::RawPrices;
use versekit_core::shops::Shops;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn own(value: &Value) -> Result<jsonio::Value, String> {
    jsonio::loads(&serde_json::to_string(value).map_err(|e| e.to_string())?).map_err(|_| "json".to_string())
}

fn own_map(value: &Value) -> Result<Map, String> {
    match own(value)? {
        jsonio::Value::Object(map) => Ok(map),
        _ => Err("Objekt erwartet".into()),
    }
}

fn plain(value: &jsonio::Value) -> Result<Value, String> {
    serde_json::from_str(&jsonio::dumps(value, None, false)).map_err(|e| e.to_string())
}

/// The made-up world: the stores come from the two files, the rest from the JSON.
struct World {
    shops: Shops,
    raw: RawPrices,
    index: HashMap<String, String>,
    recipes: HashMap<String, Recipe>,
    slots: HashMap<String, Vec<Slot>>,
    ships: HashMap<String, Vec<Offer>>,
}

impl World {
    fn load(world: &Value, home: &str) -> Result<World, String> {
        fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home)], home));
        fs::write(paths.app_file(versekit_core::shops::CACHE), world["shops_file"].to_string())
            .map_err(|e| e.to_string())?;
        fs::write(paths.app_file(versekit_core::stock_fetch::PRICES_FILE), world["prices_file"].to_string())
            .map_err(|e| e.to_string())?;
        let index = world["index"]
            .as_object()
            .ok_or("index fehlt")?
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
            .collect();
        let mut recipes = HashMap::new();
        for (name, recipe) in world["recipes"].as_object().ok_or("recipes fehlt")? {
            let steps = recipe["stufen"]
                .as_array()
                .ok_or("stufen fehlt")?
                .iter()
                .map(|step| Step {
                    seconds: step["zeit"].as_i64().unwrap_or(0),
                    ingredients: step["zutaten"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|z| Ingredient {
                            material: z[1].as_str().unwrap_or_default().to_string(),
                            amount: z[2].as_f64().unwrap_or(0.0),
                        })
                        .collect(),
                })
                .collect();
            recipes.insert(name.clone(), Recipe { blueprint: name.clone(), steps });
        }
        let mut slots = HashMap::new();
        for (ship, listed) in world["slots"].as_object().ok_or("slots fehlt")? {
            let listed = listed
                .as_array()
                .ok_or("slots")?
                .iter()
                .map(|s| Slot {
                    path: s["pfad"].as_str().unwrap_or_default().to_string(),
                    kind: s["art"].as_str().unwrap_or_default().to_string(),
                    size: s["groesse"].as_i64(),
                    factory_ref: s["werk"]["ref"].as_str().unwrap_or_default().to_string(),
                    factory_name: s["werk"]["name"].as_str().unwrap_or_default().to_string(),
                })
                .collect();
            slots.insert(ship.clone(), listed);
        }
        let mut ships = HashMap::new();
        for (ship, offers) in world["ship_offers"].as_object().ok_or("ship_offers fehlt")? {
            let offers = offers
                .as_array()
                .ok_or("ship_offers")?
                .iter()
                .map(|o| Offer {
                    shop: o["stelle"].as_str().unwrap_or_default().to_string(),
                    place: o["ort"].as_str().unwrap_or_default().to_string(),
                    system: String::new(),
                    price: o["preis"].as_f64().unwrap_or(0.0),
                })
                .collect();
            ships.insert(ship.clone(), offers);
        }
        Ok(World { shops: Shops::load(&paths), raw: RawPrices::load(&paths), index, recipes, slots, ships })
    }
}

impl Sources for World {
    fn shop_known(&self, reference: &str) -> bool {
        self.shops.known(reference)
    }
    fn cheapest(&self, reference: &str) -> Option<(f64, String, String)> {
        self.shops.cheapest(reference)
    }
    fn shops_for(&self, reference: &str) -> Vec<Offer> {
        self.shops
            .shops_for(reference)
            .map(|rows| {
                rows.iter()
                    .map(|r| Offer {
                        shop: r.shop.clone(),
                        place: r.place.clone(),
                        system: r.system.clone(),
                        price: r.price,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn raw_buy_price(&self, material: &str) -> f64 {
        self.raw.price(material).map_or(0.0, |p| p.buy)
    }
    fn recipe_for(&self, reference: &str, name: &str) -> Option<Recipe> {
        let mut blueprint =
            if reference.is_empty() { String::new() } else { self.index.get(reference).cloned().unwrap_or_default() };
        if blueprint.is_empty() && !name.is_empty() {
            blueprint = name.to_string();
        }
        if blueprint.is_empty() {
            return None;
        }
        self.recipes.get(&blueprint).filter(|r| !r.steps.is_empty()).cloned()
    }
    fn ship_offers(&self, name: &str) -> Vec<Offer> {
        self.ships.get(name).cloned().unwrap_or_default()
    }
    fn slots(&self, ship: &Map, _chosen: &Map) -> Vec<Slot> {
        self.slots.get(field_text(ship, "name")).cloned().unwrap_or_default()
    }
}

fn buy_json(o: &BuyOption) -> Value {
    json!({ "zustand": o.condition.as_str(), "preis": o.price, "laden": o.shop, "ort": o.place })
}

fn craft_json(o: &CraftOption) -> Value {
    json!({
        "zustand": o.condition.as_str(), "material": o.material, "dauer": o.seconds,
        "bauplan": o.blueprint, "ohne_preis": o.unpriced,
    })
}

fn item_json(l: &Line) -> Value {
    json!({
        "sorte": l.kind.as_str(), "schiff": l.ship, "quelle": l.origin.as_str(), "position": l.position,
        "name": l.name, "ref": l.reference, "weg": l.way, "kauf": buy_json(&l.buy), "bau": craft_json(&l.craft),
        "erledigt": l.done, "anzahl": l.count, "pfad": l.path, "pfade": l.paths, "art": l.slot_kind,
        "groesse": l.size, "werk_ref": l.factory_ref, "werk_name": l.factory_name,
    })
}

fn total_json(t: &cart::Total) -> Value {
    json!({
        "gesamt": t.total, "kaufen": t.buy, "bauen": t.craft, "dauer": t.seconds, "offen": t.open,
        "unvollstaendig": t.incomplete,
    })
}

fn stop_json(s: &cart::Stop) -> Value {
    json!({
        "system": s.system, "ort": s.place, "laeden": s.shops, "summe": s.sum,
        "posten": s.entries.iter().map(|e| json!({
            "pfad": e.path, "schiff": e.ship, "name": e.name, "preis": e.price, "anzahl": e.count, "laden": e.shop,
        })).collect::<Vec<_>>(),
    })
}

fn variant(lines: &[Line], name: &str) -> Vec<Line> {
    let mut lines = lines.to_vec();
    match name {
        "all_buy" => lines.iter_mut().filter(|l| l.kind == Kind::Part).for_each(|l| l.way = BUY.to_string()),
        "half_done" => lines.iter_mut().enumerate().filter(|(i, _)| i % 3 == 0).for_each(|(_, l)| l.done = true),
        "dup_done" => {
            // Every second open part gets two copies on other slots, ticked off: equal in everything but the tick.
            let copies: Vec<Line> = lines
                .iter()
                .enumerate()
                .filter(|(i, l)| l.kind == Kind::Part && !l.path.is_empty() && i % 2 == 0 && !l.done)
                .flat_map(|(_, l)| {
                    (1..=2).map(|n| Line { done: true, path: format!("{}_dup{n}", l.path), ..l.clone() })
                })
                .collect();
            lines.extend(copies);
        }
        "cutlass_only" => lines.retain(|l| l.ship == "Cutlass Black"),
        "all_buy_half_done" => {
            for (i, l) in lines.iter_mut().enumerate() {
                if l.kind == Kind::Part {
                    l.way = BUY.to_string();
                }
                if i % 4 == 1 {
                    l.done = true;
                }
            }
        }
        _ => {}
    }
    lines
}

fn evaluate(lines: &[Line], world: &World) -> Value {
    let (stops, without) = cart::route(lines, world);
    let route_total = cart::route_total(&stops);
    json!({
        "items": lines.iter().map(item_json).collect::<Vec<_>>(),
        "total": total_json(&cart::total(lines)),
        "stops": stops.iter().map(stop_json).collect::<Vec<_>>(),
        "without": without,
        "route_total": {
            "gesamt": route_total.total, "stopps": route_total.stops, "laeden": route_total.shops,
            "systeme": route_total.systems,
        },
        "lines": stops.iter().map(|s| cart::route_lines(&s.entries).iter().map(|l| json!({
            "name": l.name, "laden": l.shop, "preis": l.price, "anzahl": l.count, "summe": l.sum,
        })).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "group": cart::group_items(lines).iter().map(item_json).collect::<Vec<_>>(),
        "without_lines": cart::without_lines(lines, world).iter().map(|(n, c)| json!([n, c])).collect::<Vec<_>>(),
        "missing": cart::missing_prices(lines).iter().map(|(r, n)| json!([r, n])).collect::<Vec<_>>(),
    })
}

/// The ticks of the shopping list: a ship is searched by name and origin, every slot named is ticked.
fn ticks(input: &Value) -> Result<Value, String> {
    let mut fleet = own_map(&input["ticks"]["start"])?;
    let mut steps = Vec::new();
    for step in input["ticks"]["steps"].as_array().ok_or("ticks fehlt")? {
        let op = step["op"].as_array().ok_or("op fehlt")?;
        let origin = if op[0].as_str() == Some("wunsch") { Origin::Wishlist } else { Origin::Hangar };
        let slots: Vec<String> =
            op[2].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect();
        let changed =
            cart::tick(&mut fleet, origin, op[1].as_str().unwrap_or(""), &slots, op[3].as_bool().unwrap_or(false));
        steps.push(
            json!({ "op": step["op"], "changed": changed, "fleet": plain(&jsonio::Value::Object(fleet.clone()))? }),
        );
    }
    Ok(json!({ "start": input["ticks"]["start"], "steps": steps }))
}

fn edits(input: &Value) -> Result<Value, String> {
    let mut result = Vec::new();
    for case in input["edits"].as_array().ok_or("edits fehlt")? {
        let mut entry = own_map(&case["start"])?;
        let mut steps = Vec::new();
        for step in case["steps"].as_array().ok_or("steps fehlt")? {
            let op: Vec<&str> =
                step["op"].as_array().ok_or("op fehlt")?.iter().map(|x| x.as_str().unwrap_or("")).collect();
            let flag = step["op"][2].as_bool().unwrap_or(false);
            let changed = match op[0] {
                "set_done" => cart::set_done(&mut entry, op[1], flag),
                "set_method" => cart::set_method(&mut entry, op[1], op[2]),
                "set_part" => cart::set_part(&mut entry, op[1], op[2], op[3], op[4]),
                "clear_part" => cart::clear_part(&mut entry, op[1]),
                other => return Err(format!("unbekannte Änderung {other}")),
            };
            let belegung = match entry.iter().find(|(k, _)| k == "belegung") {
                Some((_, v)) => plain(v)?,
                None => Value::Null,
            };
            steps.push(json!({ "op": step["op"], "changed": changed, "belegung": belegung }));
        }
        result.push(json!({ "name": case["name"], "start": case["start"], "steps": steps }));
    }
    Ok(json!(result))
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let home = std::env::temp_dir().join(format!("parity-cart-rs-{}", versekit_core::testdir::run_tag()));
    let world = World::load(&input["world"], &home.to_string_lossy())?;
    let mut result = json!({});
    let options: Vec<Value> = input["options"]
        .as_array()
        .ok_or("options fehlt")?
        .iter()
        .map(|o| {
            let (reference, name) = (o["ref"].as_str().unwrap_or(""), o["name"].as_str().unwrap_or(""));
            json!({
                "ref": reference, "name": name, "buy": buy_json(&cart::buy_option(reference, &world)),
                "craft": craft_json(&cart::craft_option(reference, name, &world)),
            })
        })
        .collect();
    result["options"] = json!(options);

    let fleet = own_map(&input["world"]["fleet"])?;
    let bill = cart::invoice(&fleet, &world);
    result["bill"] = json!({
        "items": bill.lines.iter().map(item_json).collect::<Vec<_>>(),
        "total": total_json(&bill.total), "ships": bill.ships, "without_slots": bill.without_slot_data,
    });
    let mut variants = serde_json::Map::new();
    for name in ["as_is", "all_buy", "half_done", "cutlass_only", "all_buy_half_done", "dup_done"] {
        variants.insert(name.to_string(), evaluate(&variant(&bill.lines, name), &world));
    }
    result["variants"] = Value::Object(variants);

    let (stops, _) = cart::route(&variant(&bill.lines, "all_buy"), &world);
    let mut ordering = Vec::new();
    for case in input["ordering"].as_array().ok_or("ordering fehlt")? {
        let saved: Vec<String> =
            case["saved"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(str::to_string)).collect();
        let ordered = cart::ordered_stops(&stops, &saved);
        let mut moves = Vec::new();
        for m in case["moves"].as_array().ok_or("moves fehlt")? {
            let (from, to) = (m["from"].as_u64().ok_or("from")? as usize, m["to"].as_u64().ok_or("to")? as usize);
            let (moved, after) = cart::move_stop(&ordered, from, to, &saved);
            moves.push(json!({
                "from": m["from"], "to": m["to"], "order": moved.iter().map(cart::stop_key).collect::<Vec<_>>(),
                "saved_after": after,
            }));
        }
        ordering.push(json!({
            "saved": saved, "ordered": ordered.iter().map(cart::stop_key).collect::<Vec<_>>(), "moves": moves,
        }));
    }
    result["ordering"] = json!(ordering);
    result["edits"] = edits(&input)?;
    result["ticks"] = ticks(&input)?;
    let _ = fs::remove_dir_all(&home);
    if gegenprobe {
        // One price, one name, one state, one order, one result of an edit, one sum and one tick changed: the comparison must report all seven.
        result["variants"]["all_buy"]["stops"][0]["posten"][0]["preis"] = json!(1.0);
        result["bill"]["items"][0]["name"] = json!("Gegenprobe");
        result["options"][0]["buy"]["zustand"] = json!("gegenprobe");
        result["ordering"][1]["ordered"] = json!(["Gegenprobe|"]);
        result["edits"][0]["steps"][1]["changed"] = json!(true);
        result["variants"]["half_done"]["total"]["gesamt"] = json!(1.0);
        result["ticks"]["steps"][0]["changed"] = json!(false);
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
