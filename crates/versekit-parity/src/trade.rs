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

//! Rust side of the parity check for trade (`tools/parity_trade.py`): the
//! same queries on copies of the real stored UEX lists, and the same
//! updates on the same made-up answers, with the clock at the value of the
//! Python side. Nothing leaves the process; every request is answered from
//! the table of the Python output.
//!
//! `--gegenprobe` reverses the list of goods and changes one price in the
//! made-up answers, so the comparison must report both.

use std::fs;
use std::time::{Duration, UNIX_EPOCH};

use versekit_core::commodity_prices::{self, Prices};
use versekit_core::jsonio::{self, Value};
use versekit_core::legacy::ENV_HOME;
use versekit_core::paths::{Host, Paths};
use versekit_core::places;
use versekit_core::routes::{self, ChainOptions, Leg, Routes};
use versekit_core::scmdb::FetchError;
use versekit_core::selling::{self, Selling, Update};
use versekit_core::ship_market::{self, Market};
use versekit_core::shops::{self, Catalog, Shops};
use versekit_core::stock_fetch;
use versekit_core::writeguard;

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn strs(items: &[String]) -> Value {
    Value::Array(items.iter().cloned().map(Value::Str).collect())
}

fn float(x: f64) -> Value {
    Value::Float(x)
}

fn opt_float(x: Option<f64>) -> Value {
    x.map_or(Value::Null, Value::Float)
}

fn int(n: i64) -> Value {
    Value::int(n)
}

fn pair_text(p: Option<(&'static str, bool)>) -> Value {
    p.map_or(Value::Null, |(key, flag)| Value::Array(vec![Value::str(key), Value::Bool(flag)]))
}

fn leg_value(leg: &Leg, start: bool) -> Value {
    let t = &leg.trip;
    let mut fields = vec![
        ("ware", Value::str(&t.good)),
        ("ziel", Value::str(&t.dest)),
        ("zielname", Value::str(&t.dest_name)),
        ("zielort", Value::str(&t.dest_place)),
        ("zielsystem", Value::str(&t.dest_system)),
        ("ek", float(t.buy)),
        ("vk", float(t.sell)),
        ("gewinn_scu", float(t.profit_scu)),
        ("strecke", float(t.distance)),
        ("vorrat", int(t.stock)),
        ("bedarf", int(t.demand)),
        ("menge", int(leg.amount)),
        ("gewinn", float(leg.profit)),
        ("grenze", Value::str(leg.limit)),
    ];
    if start {
        fields.push(("startname", Value::str(&leg.start_name)));
    }
    obj(fields)
}

fn legs(list: &[Leg], start: bool) -> Value {
    Value::Array(list.iter().map(|leg| leg_value(leg, start)).collect())
}

fn chain_value(found: &[(f64, Vec<Leg>)]) -> Value {
    Value::Array(found.iter().map(|(profit, way)| Value::Array(vec![float(*profit), legs(way, false)])).collect())
}

fn offer_values(offers: &[ship_market::Offer]) -> Value {
    Value::Array(
        offers
            .iter()
            .map(|o| {
                obj(vec![
                    ("stelle", Value::str(&o.spot)),
                    ("ort", Value::str(&o.place)),
                    ("system", Value::str(&o.system)),
                    ("preis", float(o.price)),
                ])
            })
            .collect(),
    )
}

fn entry_values(entries: &[ship_market::Entry]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|e| obj(vec![("name", Value::str(&e.name)), ("werft", Value::str(&e.maker)), ("scu", int(e.scu))]))
            .collect(),
    )
}

fn place_value(p: &selling::Place) -> Value {
    obj(vec![
        ("terminal", Value::str(&p.terminal)),
        ("ort", Value::str(&p.place)),
        ("system", Value::str(&p.system)),
        ("nqa", Value::Bool(p.nqa)),
        (
            "treffer",
            Value::Array(
                p.hits
                    .iter()
                    .map(|h| {
                        obj(vec![
                            ("ware", Value::str(&h.good)),
                            ("preis", float(h.price)),
                            ("kisten", Value::str(&h.boxes)),
                            ("alter", opt_float(h.age)),
                            ("fuellstand", pair_text(h.fill)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("anzahl", int(p.count as i64)),
        ("summe", float(p.sum)),
        ("alter", opt_float(p.age)),
    ])
}

fn buy_place_value(p: &selling::BuyPlace) -> Value {
    obj(vec![
        ("terminal", Value::str(&p.terminal)),
        ("ort", Value::str(&p.place)),
        ("system", Value::str(&p.system)),
        ("nqa", Value::Bool(p.nqa)),
        ("preis", float(p.price)),
        ("kisten", Value::str(&p.boxes)),
        ("vorrat", int(p.stock)),
        (
            "vorratsstufe",
            p.level.map_or(Value::Null, |(key, grade)| Value::Array(vec![Value::str(key), Value::str(grade)])),
        ),
        ("alter", opt_float(p.age)),
    ])
}

fn item_value(item: &shops::CatalogItem) -> Value {
    obj(vec![
        ("name", Value::str(&item.name)),
        ("kennung", Value::str(&item.code)),
        ("abschnitt", Value::str(&item.section)),
        ("kategorie", Value::str(&item.group)),
        ("hersteller", Value::str(&item.maker)),
        ("groesse", Value::str(&item.size)),
        ("klasse", Value::str(&item.class)),
        ("guete", Value::str(&item.grade)),
        ("orte", strs(&item.places)),
    ])
}

fn rows_value(rows: Option<&[shops::ShopRow]>) -> Value {
    rows.map_or(Value::Null, |rows| {
        Value::Array(
            rows.iter()
                .map(|r| {
                    obj(vec![
                        ("laden", Value::str(&r.shop)),
                        ("ort", Value::str(&r.place)),
                        ("system", Value::str(&r.system)),
                        ("preis", float(r.price)),
                        ("zustand", int(r.condition)),
                    ])
                })
                .collect(),
        )
    })
}

fn cheapest_value(hit: Option<(f64, String, String)>) -> Value {
    hit.map_or(Value::Null, |(price, shop, place)| {
        Value::Array(vec![float(price), Value::Str(shop), Value::Str(place)])
    })
}

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn files_of(folder: &str, names: &[String]) -> Value {
    let mut out = Vec::new();
    for name in names {
        if let Ok(bytes) = fs::read(format!("{folder}/{name}")) {
            out.push((name.clone(), Value::Str(String::from_utf8_lossy(&bytes).into_owned())));
        }
    }
    Value::Object(out)
}

fn flag(found: bool) -> Value {
    Value::Bool(found)
}

fn update_value(update: Update) -> Value {
    Value::Array(vec![flag(update == Update::Done), Value::str(update.code())])
}

fn system_time(at: f64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs_f64(at)
}

fn first<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    items.iter().take(n).cloned().collect()
}

fn last<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    items[items.len().saturating_sub(n)..].to_vec()
}

const HOLDS: [(i64, f64); 5] = [(120, 500_000.0), (32, 100_000.0), (0, 1_000.0), (2_000, 1e9), (96, 12_345.5)];

#[allow(clippy::too_many_lines)]
fn queries(paths: &Paths, now: f64, gegenprobe: bool, cargo: &[Value]) -> Value {
    let selling = Selling::load(paths);
    let mut goods = selling.goods();
    if gegenprobe {
        goods.reverse();
    }
    let buy_goods = selling.buy_goods();
    let mut out: Vec<(&str, Value)> = Vec::new();
    out.push(("goods", strs(&goods)));
    out.push(("buy_goods", strs(&buy_goods)));
    let mut names: Vec<String> = goods.clone();
    names.extend(["Nichts".to_string(), "gold".to_string()]);
    out.push((
        "best",
        Value::Array(
            names
                .iter()
                .map(|g| {
                    Value::Array(vec![
                        Value::str(g),
                        float(selling.best_price(g, false)),
                        float(selling.best_price(g, true)),
                        flag(selling.known(g)),
                        flag(selling::in_top_list(g)),
                    ])
                })
                .collect(),
        ),
    ));
    let mut buy_names = buy_goods.clone();
    buy_names.push("Nichts".to_string());
    out.push((
        "cheapest",
        Value::Array(
            buy_names
                .iter()
                .map(|g| {
                    Value::Array(vec![
                        Value::str(g),
                        float(selling.cheapest_price(g, false)),
                        float(selling.cheapest_price(g, true)),
                        flag(selling.buy_known(g)),
                    ])
                })
                .collect(),
        ),
    ));
    let sorted_goods = selling.goods();
    let mut subsets: Vec<Vec<String>> = sorted_goods.iter().take(12).map(|g| vec![g.clone()]).collect();
    subsets.push(first(&sorted_goods, 3));
    subsets.push(first(&sorted_goods, 8));
    let mut mixed = vec!["Nichts".to_string()];
    mixed.extend(first(&sorted_goods, 2));
    subsets.push(mixed);
    subsets.push(vec!["Nichts".to_string()]);
    subsets.push(Vec::new());
    let mut places_for = Vec::new();
    for names in &subsets {
        for nqa in [false, true] {
            let found: Vec<Value> = selling.places_for(names, nqa, now).iter().map(place_value).collect();
            places_for.push(Value::Array(vec![strs(names), flag(nqa), Value::Array(found)]));
        }
    }
    out.push(("places_for", Value::Array(places_for)));
    out.push((
        "places_all",
        Value::Array(
            [false, true]
                .iter()
                .map(|nqa| {
                    let found = selling.places_for(&sorted_goods, *nqa, now);
                    Value::Array(vec![flag(*nqa), Value::Array(found.iter().take(25).map(place_value).collect())])
                })
                .collect(),
        ),
    ));
    let sorted_buy = selling.buy_goods();
    let mut buy_places = Vec::new();
    for g in sorted_buy.iter().take(15) {
        for nqa in [false, true] {
            let found: Vec<Value> = selling.buy_places(g, nqa, now).iter().map(buy_place_value).collect();
            buy_places.push(Value::Array(vec![Value::str(g), flag(nqa), Value::Array(found)]));
        }
    }
    out.push(("buy_places", Value::Array(buy_places)));
    let level_offer = |level: i64| selling::Offer { level, ..selling::Offer::default() };
    out.push((
        "levels",
        Value::Array(
            [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5), Some(6), Some(7), Some(8)]
                .iter()
                .map(|lv: &Option<i64>| {
                    let offer = level_offer(lv.unwrap_or(0));
                    Value::Array(vec![
                        lv.map_or(Value::Null, int),
                        pair_text(selling::fill_level(Some(&offer))),
                        selling::stock_level(Some(&offer))
                            .map_or(Value::Null, |(k, g)| Value::Array(vec![Value::str(k), Value::str(g)])),
                    ])
                })
                .collect(),
        ),
    ));
    out.push((
        "levels_none",
        Value::Array(vec![
            pair_text(selling::fill_level(None)),
            selling::stock_level(None).map_or(Value::Null, |(k, g)| Value::Array(vec![Value::str(k), Value::str(g)])),
            pair_text(selling::fill_level(Some(&selling::Offer::default()))),
        ]),
    ));
    out.push((
        "wait",
        Value::Array(
            [now, now + 1000.0, now + 5000.0]
                .iter()
                .map(|m| Value::Array(vec![float(*m), opt_float(selling.age(*m)), int(selling::wait_time(paths, *m))]))
                .collect(),
        ),
    ));

    // --- the own cargo
    let sums = |map: &versekit_core::ordered::OrderedMap<f64>| {
        Value::Object(map.iter().map(|(name, value)| (name.to_string(), float(*value))).collect())
    };
    out.push((
        "cargo_sums",
        Value::Array(
            [None, Some(true), Some(false)]
                .iter()
                .map(|only| {
                    Value::Array(vec![
                        only.map_or(Value::Null, Value::Bool),
                        sums(&selling::cargo_amounts(cargo, *only)),
                        sums(&selling::cargo_costs(cargo, *only)),
                        sums(&selling::cargo_costed_amounts(cargo, *only)),
                        strs(&selling::goods_in_stock(cargo, *only)),
                    ])
                })
                .collect(),
        ),
    ));
    out.push(("cargo_stolen", flag(selling::has_stolen(cargo))));
    // --- routes
    let routes = Routes::load(paths);
    let all_starts: Vec<String> = routes.starts.iter().map(|(ident, _)| ident.to_string()).collect();
    let posts = routes::trade_posts(&selling);
    out.push((
        "posts",
        Value::Array(posts.iter().map(|(a, b)| Value::Array(vec![Value::str(a), Value::str(b)])).collect()),
    ));
    out.push(("known_starts", int(routes.known_starts(&posts) as i64)));
    let mut starts: Vec<String> = first(&all_starts, 6);
    starts.push("nicht-da".to_string());
    out.push((
        "route_age",
        Value::Array(starts.iter().map(|s| Value::Array(vec![Value::str(s), opt_float(routes.age(s, now))])).collect()),
    ));
    out.push((
        "route_trips",
        Value::Array(
            starts
                .iter()
                .map(|s| {
                    Value::Array(vec![Value::str(s), routes.trips(s).map_or(Value::Null, |t| int(t.len() as i64))])
                })
                .collect(),
        ),
    ));
    let mut single = Vec::new();
    for s in &starts {
        for (scu, money) in HOLDS {
            let found = routes::single_trips(&routes, s, scu, money, 8);
            single.push(Value::Array(vec![Value::str(s), int(scu), money_value(money), legs(&found, false)]));
        }
    }
    out.push(("single", Value::Array(single)));
    let mut book = Routes::load(paths);
    let mut chains = Vec::new();
    for s in first(&all_starts, 4) {
        for stops in [2, 3] {
            for round in [false, true] {
                for short in [false, true] {
                    let found = routes::chain(
                        &mut book,
                        &s,
                        120,
                        500_000.0,
                        ChainOptions { short, most: 5, stops, round_trip: round },
                        &mut |_, _| false,
                    );
                    chains.push(Value::Array(vec![
                        Value::str(&s),
                        int(stops),
                        flag(round),
                        flag(short),
                        chain_value(&found),
                    ]));
                }
            }
        }
    }
    out.push(("chains", Value::Array(chains)));
    out.push((
        "anywhere",
        Value::Array(
            [(120, 500_000.0), (32, 20_000.0)]
                .iter()
                .map(|(scu, money)| {
                    let found = routes::best_anywhere(&routes, &posts, *scu, *money, 15);
                    Value::Array(vec![int(*scu), money_value(*money), legs(&found, true)])
                })
                .collect(),
        ),
    ));
    let mut anywhere = Vec::new();
    for (stops, round, short) in [(2, false, false), (3, true, false), (2, false, true)] {
        let found = routes::best_chains_anywhere(
            &mut book,
            &posts,
            120,
            500_000.0,
            ChainOptions { short, most: 15, stops, round_trip: round },
        );
        let rows: Vec<Value> = found
            .iter()
            .map(|(profit, name, way)| Value::Array(vec![float(*profit), Value::str(name), legs(way, false)]))
            .collect();
        anywhere.push(Value::Array(vec![int(stops), flag(round), flag(short), Value::Array(rows)]));
    }
    out.push(("chains_anywhere", Value::Array(anywhere)));

    // --- ships, places, prices, shops
    let market = Market::load(paths);
    out.push(("cargo", entry_values(&market.with_cargo())));
    out.push(("ship_catalog", entry_values(&market.catalog())));
    let cargo_names: Vec<String> = first(&market.with_cargo(), 12).into_iter().map(|e| e.name).collect();
    let mut ship_names = cargo_names;
    ship_names.extend(["Nichts".to_string(), " ".to_string()]);
    out.push((
        "ship_scu",
        Value::Array(
            ship_names
                .iter()
                .map(|n| {
                    Value::Array(vec![
                        Value::str(n),
                        int(market.scu(n)),
                        int(market.scu(&n.to_lowercase())),
                        offer_values(&market.buy_at(n)),
                        offer_values(&market.rent_at(n)),
                    ])
                })
                .collect(),
        ),
    ));
    let catalog_places = read_places(paths);
    out.push(("places", strs(&catalog_places)));
    let probes = ["pyro", "lorvile", "checkmate", "  ", "zzzz", "Port", "Area18", "olisar"];
    out.push((
        "similar",
        Value::Array(
            probes
                .iter()
                .map(|p| {
                    Value::Array(vec![
                        Value::str(*p),
                        strs(&places::similar(&catalog_places, p, 4)),
                        strs(&places::similar(&catalog_places, p, 2)),
                    ])
                })
                .collect(),
        ),
    ));
    let prices = Prices::load(paths);
    let materials = [
        "Iron",
        "Borase",
        "Quantainium",
        "Gold",
        "Aluminium",
        "Aluminum",
        "Raw Ouratite",
        "Nichts",
        "",
        "Titanium",
        "Agricium (Ore)",
    ];
    out.push((
        "price",
        Value::Array(
            materials
                .iter()
                .map(|m| {
                    let hit = prices
                        .price(m)
                        .map_or(Value::Null, |p| Value::Array(vec![float(p.buy), float(p.sell), Value::Str(p.form)]));
                    Value::Array(vec![Value::str(*m), hit])
                })
                .collect(),
        ),
    ));
    let catalog = Catalog::load(paths);
    let items = catalog.items();
    out.push((
        "catalog",
        obj(vec![
            ("count", int(items.len() as i64)),
            ("first", Value::Array(first(&items, 6).iter().map(item_value).collect())),
            ("last", Value::Array(last(&items, 6).iter().map(item_value).collect())),
            ("ready", flag(catalog.ready())),
            ("places", strs(&catalog.places())),
            ("fetched", opt_float(catalog.fetched)),
        ]),
    ));
    out.push(("cheapest_by_entity", cheapest_summary(&catalog)));
    let shops_book = Shops::load(paths);
    let stored = stored_codes(paths);
    let mut codes: Vec<String> = first(&stored, 25);
    codes.extend(["nicht-da".to_string(), String::new()]);
    out.push((
        "shop_rows",
        Value::Array(
            codes
                .iter()
                .map(|k| {
                    Value::Array(vec![
                        Value::str(k),
                        flag(shops_book.known(k)),
                        opt_float(shops_book.age(k, now)),
                        rows_value(shops_book.shops_for(k)),
                        cheapest_value(shops_book.cheapest(k)),
                    ])
                })
                .collect(),
        ),
    ));
    obj(out)
}

fn money_value(money: f64) -> Value {
    if money.fract() == 0.0 && money.abs() < 1e15 { Value::int(money as i64) } else { Value::Float(money) }
}

fn read_places(paths: &Paths) -> Vec<String> {
    match jsonio::read_json(&paths.app_file(places::CACHE)) {
        Ok(data) => match data.get("orte") {
            Some(Value::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

fn stored_codes(paths: &Paths) -> Vec<String> {
    match jsonio::read_json(&paths.app_file(shops::CACHE)) {
        Ok(data) => match data.get("teile") {
            Some(Value::Object(items)) => items.iter().map(|(k, _)| k.clone()).collect(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

fn cheapest_summary(catalog: &Catalog) -> Value {
    let cheapest = catalog.cheapest();
    let mut by_code: Vec<(String, Value)> = catalog
        .parts
        .iter()
        .filter(|p| !p.code.is_empty())
        .filter_map(|p| {
            p.cheapest
                .as_ref()
                .map(|(price, place)| (p.code.clone(), Value::Array(vec![price.clone(), Value::str(place)])))
        })
        .collect();
    by_code.sort_by(|a, b| a.0.cmp(&b.0));
    by_code.dedup_by(|a, b| a.0 == b.0);
    let mut by_name: Vec<(String, Value)> = catalog
        .price_names
        .iter()
        .map(|(n, (price, place))| (n.to_string(), Value::Array(vec![price.clone(), Value::str(place)])))
        .collect();
    by_name.sort_by(|a, b| a.0.cmp(&b.0));
    let pairs = |list: Vec<(String, Value)>| {
        Value::Array(list.into_iter().take(30).map(|(k, v)| Value::Array(vec![Value::Str(k), v])).collect())
    };
    let mut codes: Vec<String> = cheapest.known_codes.iter().cloned().collect();
    codes.sort();
    let mut names: Vec<String> = cheapest.known_names.iter().cloned().collect();
    names.sort();
    obj(vec![
        ("kennung", pairs(by_code)),
        ("name", pairs(by_name)),
        (
            "counts",
            Value::Array(vec![
                int(cheapest.by_code.len() as i64),
                int(cheapest.by_name.len() as i64),
                int(cheapest.known_codes.len() as i64),
                int(cheapest.known_names.len() as i64),
            ]),
        ),
        ("kennungen", strs(&first(&codes, 10))),
        ("namen", strs(&first(&names, 10))),
    ])
}

fn sorted_code_prices(catalog: &Catalog) -> Value {
    let mut list: Vec<(String, Value)> = catalog
        .parts
        .iter()
        .filter(|p| !p.code.is_empty())
        .filter_map(|p| {
            p.cheapest
                .as_ref()
                .map(|(price, place)| (p.code.clone(), Value::Array(vec![price.clone(), Value::str(place)])))
        })
        .collect();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Array(list.into_iter().map(|(k, v)| Value::Array(vec![Value::Str(k), v])).collect())
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    if std::env::var(versekit_core::legacy::ENV_NO_NET).is_ok_and(|v| !v.is_empty() && v != "0") {
        return Err("SC_BP_NO_NET ist gesetzt: der Lauf braucht die Abrufe der Fassung".into());
    }
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let base = text(&input, "rust_folder")?;
    let now = match input.get("now") {
        Some(Value::Float(x)) => *x,
        _ => return Err("now fehlt".into()),
    };
    let folder = (1..1000)
        .map(|n| format!("{base}/lauf-{n}"))
        .find(|f| !std::path::Path::new(f).exists())
        .ok_or("kein freier Laufordner")?;
    jsonio::make_dirs(&folder).map_err(|e| e.to_string())?;
    writeguard::deny(true);
    writeguard::allow_under(Some(&folder));
    let result = run_in(&input, &folder, now, gegenprobe, out);
    writeguard::deny(false);
    writeguard::allow_under(None);
    result
}

fn run_in(input: &Value, folder: &str, now: f64, gegenprobe: bool, out: &str) -> Result<(), String> {
    let mut host = Host::current();
    host.registry = false;
    host.set_var(ENV_HOME, folder);
    let paths = Paths::new(host);
    let Some(Value::Object(copies)) = input.get("copies") else { return Err("copies fehlt".into()) };
    for (name, source) in copies {
        if let Some(source) = source.as_str() {
            fs::copy(source, format!("{folder}/{name}")).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    let files: Vec<String> = match input.get("files") {
        Some(Value::Array(items)) => items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
        _ => return Err("files fehlt".into()),
    };
    let cargo = match input.get("cargo").and_then(|c| c.get("posten")) {
        Some(Value::Array(entries)) => entries.clone(),
        _ => return Err("cargo fehlt".into()),
    };
    let queries = queries(&paths, now, gegenprobe, &cargo);

    // --- the updates on made-up answers
    let Some(Value::Object(table)) = input.get("fixtures") else { return Err("fixtures fehlt".into()) };
    let mut table: Vec<(String, String)> =
        table.iter().filter_map(|(url, body)| body.as_str().map(|b| (url.clone(), b.to_string()))).collect();
    if gegenprobe && let Some(row) = table.iter_mut().find(|(url, _)| url.ends_with("commodities_prices_all")) {
        row.1 = row.1.replacen("33000", "33001", 1);
    }
    for name in &files {
        let _ = fs::remove_file(format!("{folder}/{name}"));
    }
    selling::forget_failure();
    let mut steps: Vec<Value> = Vec::new();
    let host_table = std::sync::Mutex::new(table);
    let fetch = |url: &str| -> Result<Vec<u8>, FetchError> {
        let table = host_table.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .find(|(known, _)| known == url)
            .map(|(_, body)| body.clone().into_bytes())
            .ok_or_else(|| FetchError::Failed(format!("not in the table: {url}")))
    };
    let mut record = |name: &str, at: f64, result: Value| {
        steps.push(obj(vec![
            ("name", Value::str(name)),
            ("at", float(at)),
            ("result", result),
            ("files", files_of(folder, &files)),
        ]));
    };
    let ok =
        |outcome: stock_fetch::Outcome| matches!(outcome, stock_fetch::Outcome::Fresh | stock_fetch::Outcome::Current);
    let mut at = now;
    record("build", at, flag(ok(stock_fetch::update_build_with(&paths, system_time(at), &fetch))));
    at = now + 60.0;
    record("build-fresh", at, flag(ok(stock_fetch::update_build_with(&paths, system_time(at), &fetch))));
    at = now;
    record("selling-first", at, update_value(selling::update(&paths, at, false, &fetch)));
    at = now + 100.0;
    record("selling-locked", at, update_value(selling::update(&paths, at, true, &fetch)));
    record("selling-wait", at, int(selling::wait_time(&paths, at)));
    at = now + 200.0;
    record("selling-fresh", at, update_value(selling::update(&paths, at, false, &fetch)));
    at = now + 4000.0;
    record("selling-forced", at, update_value(selling::update(&paths, at, true, &fetch)));
    let written = Selling::load(&paths);
    let best: Vec<Value> = ["Gold", "Edge Good", "Near Good", "Cheap Edge", "Cheap Near", "Luminalia Gift", "Copper"]
        .iter()
        .map(|g| {
            Value::Array(vec![
                Value::str(*g),
                float(written.best_price(g, false)),
                float(written.best_price(g, true)),
                float(written.cheapest_price(g, false)),
                float(written.cheapest_price(g, true)),
            ])
        })
        .collect();
    record("selling-best", at, Value::Array(best));
    record("places-first", at, flag(places::update(&paths, at, &fetch)));
    at = now + 4100.0;
    record("places-fresh", at, flag(places::update(&paths, at, &fetch)));
    at = now + 4000.0;
    record("prices-first", at, Value::Array(vec![flag(commodity_prices::update(&paths, at, &fetch)), Value::str("")]));
    record("ships-first", at, flag(ship_market::update(&paths, at, &fetch)));
    for start in ["10", "11", "12", "13", "99", "20"] {
        let mut book = Routes::load(&paths);
        let found = routes::fetch_start(&paths, &mut book, start, false, at, &fetch);
        record(&format!("routes-fetch-{start}"), at, flag(found));
    }
    at = now + 4100.0;
    let mut book = Routes::load(&paths);
    record("routes-fresh", at, flag(routes::fetch_start(&paths, &mut book, "10", false, at, &fetch)));
    at = now + 4000.0 + 7.0 * 3600.0;
    let mut book = Routes::load(&paths);
    record("routes-stale", at, flag(routes::fetch_start(&paths, &mut book, "10", false, at, &fetch)));
    let mut book = Routes::load(&paths);
    record("routes-force", at, flag(routes::fetch_start(&paths, &mut book, "11", true, at, &fetch)));
    let run_chain = |stops: i64, round_trip: bool| {
        let mut book = Routes::load(&paths);
        let found = routes::chain(
            &mut book,
            "10",
            120,
            500_000.0,
            ChainOptions { short: false, most: 5, stops, round_trip },
            &mut |book, place| routes::fetch_start(&paths, book, place, false, at, &fetch),
        );
        chain_value(&found)
    };
    record("routes-chain", at, run_chain(3, false));
    record("routes-chain-round", at, run_chain(2, true));
    let book = Routes::load(&paths);
    let single = routes::single_trips(&book, "10", 120, 500_000.0, 20);
    record("routes-single", at, legs(&single, false));
    record("routes-floor", at, legs(&routes::single_trips(&book, "20", 100, 1.0, 20), false));
    record("routes-tie", at, legs(&routes::single_trips(&book, "20", 10, 1000.0, 20), false));
    let selling_book = Selling::load(&paths);
    let posts = routes::trade_posts(&selling_book);
    let mut book = Routes::load(&paths);
    let done = routes::fetch_all(&paths, &mut book, &posts, at, &fetch, &mut |_, _| {}, &|| false);
    record("routes-fetch-all", at, int(done as i64));
    let book = Routes::load(&paths);
    let found = routes::best_anywhere(&book, &posts, 120, 500_000.0, 15);
    record("routes-anywhere", at, legs(&found, true));
    at = now + 5000.0;
    record("catalog-build", at, flag(shops::build_catalog(&paths, at, None, &fetch, &mut |_, _| {})));
    at = now + 5100.0;
    record("catalog-fresh", at, flag(shops::build_catalog(&paths, at, None, &fetch, &mut |_, _| {})));
    let catalog = Catalog::load(&paths);
    record("catalog-items", at, Value::Array(catalog.items().iter().map(item_value).collect()));
    record("catalog-cheapest", at, sorted_code_prices(&catalog));
    at = now + 5200.0;
    let fetch_part = |code: &str, name: &str, at: f64| {
        let mut book = Shops::load(&paths);
        let mut catalog = Catalog::load(&paths);
        flag(shops::fetch_part(&paths, &mut book, &mut catalog, (code, name), false, at, &fetch))
    };
    record("shop-code", at, fetch_part("u-101", "", at));
    record("shop-id", at, fetch_part("id:102", "", at));
    record("shop-name", at, fetch_part("u-zzz", "Gamma", at));
    record("shop-empty", at, fetch_part("u-empty", "Nothing here", at));
    record("shop-space", at, fetch_part("Bauplan Name", "", at));
    record("shop-quote", at, fetch_part("a\"b", "", at));
    let book = Shops::load(&paths);
    record(
        "shop-cheapest",
        at,
        Value::Array(vec![
            cheapest_value(book.cheapest("u-101")),
            cheapest_value(book.cheapest("nicht-da")),
            rows_value(book.shops_for("u-101")),
            rows_value(book.shops_for("x")),
            opt_float(book.age("u-101", at)),
        ]),
    );
    at = now + 5200.0 + 31.0 * 86400.0;
    record("shop-stale", at, fetch_part("u-101", "", at));
    host_table.lock().unwrap_or_else(|e| e.into_inner()).retain(|(url, _)| !url.ends_with("commodities_prices_all"));
    at = now + 9000.0;
    record("selling-network-down", at, update_value(selling::update(&paths, at, true, &fetch)));
    at = now + 9010.0;
    record("selling-wait-after-error", at, int(selling::wait_time(&paths, at)));
    at = now + 9100.0;
    record("selling-after-wait", at, update_value(selling::update(&paths, at, true, &fetch)));
    host_table
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((format!("{}commodities_prices_all", "https://api.uexcorp.uk/2.0/"), "{\"data\": []}".to_string()));
    at = now + 20000.0;
    record("selling-empty", at, update_value(selling::update(&paths, at, true, &fetch)));
    host_table.lock().unwrap_or_else(|e| e.into_inner()).retain(|(url, _)| !url.ends_with("commodities_prices_all"));
    host_table.lock().unwrap_or_else(|e| e.into_inner()).push((
        "https://api.uexcorp.uk/2.0/commodities_prices_all".to_string(),
        "{\"data\": [{\"commodity_name\": \"X\", \"price_sell\": 0}]}".to_string(),
    ));
    at = now + 30000.0;
    record("selling-no-prices", at, update_value(selling::update(&paths, at, true, &fetch)));
    let result = obj(vec![("queries", queries), ("steps", Value::Array(steps))]);
    fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())?;
    Ok(())
}
