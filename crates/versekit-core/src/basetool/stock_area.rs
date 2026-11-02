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

//! The area "My stock" of the sync (`basetool_sync._sync_stock`,
//! `_apply_stock`, `_split_complete`, `resolve_uex_items`): the raw material
//! stock and the trade stock against the lots of the Basetool. The plan itself
//! is in [`stock_sync`](crate::basetool::stock_sync).
//!
//! Both stock files are written here after taking over (as the window writes
//! them for a booking), inside [`Pass::apply_local`], so that this is no own
//! change that would ask for another sync. The rows are looked for anew
//! before writing, never by their position from before: the player may have
//! changed something between fetching and writing.

use std::collections::{BTreeMap, BTreeSet};

use crate::basetool::area_support::{
    Posted, Stamp, pull_generic, put_object, send_unless_held, stamp_of, store_busy, store_ok, take_object, text_map,
};
use crate::basetool::connection::get;
use crate::basetool::error::{ApiError, truthy};
use crate::basetool::feed;
use crate::basetool::scopes::{ROUTE_STOCK, SETTING_STOCK};
use crate::basetool::stock_sync::{self, Places, ServerLot, num_value};
use crate::basetool::sync::{Area, Pass};
use crate::jsonio::{Map, Value, map_set};
use crate::paths::Paths;
use crate::stock::{self, RAW_FILE, TRADE_FILE};
use crate::stock_data::StockData;

/// At most this often the full stand when lots without `split` stand there.
pub const SPLIT_AGAIN: f64 = 24.0 * 3600.0;

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Float(x)) => *x,
        _ => 0.0,
    }
}

/// The keys of the lots that carry no `split` in the remembered stand.
fn without_split(server: &BTreeMap<String, Value>) -> Vec<String> {
    server
        .iter()
        .filter(|(_, lot)| !matches!(lot.get("split"), Some(Value::Object(_))))
        .map(|(k, _)| k.clone())
        .collect()
}

/// Lots without `split` in the remembered stand: once the full stand without a
/// cursor, at most every [`SPLIT_AGAIN`] seconds.
///
/// ⚠ The feed delivers only changed lots. A lot that last arrived without
/// `split` would stay so, and "My stock" would show it as entirely personal.
/// What still has none afterwards stands in `split_missing` (at most 50 keys).
fn split_complete(
    pass: &Pass<'_>,
    sub: &mut Map,
    server: BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, ApiError> {
    let mut server = server;
    let missing = without_split(&server);
    if !missing.is_empty() && pass.now - number(get(sub, "split_full_at")) >= SPLIT_AGAIN {
        map_set(sub, "split_full_at", Value::Float(pass.now));
        map_set(sub, "cursor", Value::Null);
        server = pull_generic(pass.conn, sub, "/me/stock", "key")?.0;
    }
    let missing = without_split(&server);
    map_set(sub, "split_missing", Value::Array(missing.into_iter().take(50).map(Value::Str).collect()));
    Ok(server)
}

/// The splits of the lots: key to `split`.
fn splits(server: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    server.iter().map(|(k, lot)| (k.clone(), lot.get("split").cloned().unwrap_or(Value::Null))).collect()
}

fn server_map(sub: &Map) -> BTreeMap<String, Value> {
    match get(sub, "server") {
        Some(Value::Object(inner)) => inner.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => BTreeMap::new(),
    }
}

fn names_text(items: &BTreeMap<String, Value>) -> Value {
    Value::Object(items.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn bools(sub: &Map, key: &str) -> BTreeMap<String, bool> {
    match get(sub, key) {
        Some(Value::Object(inner)) => inner
            .iter()
            .filter_map(|(k, v)| if let Value::Bool(b) = v { Some((k.clone(), *b)) } else { None })
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn row_text(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::Str(s)) => crate::text::py_strip(s).to_string(),
        _ => String::new(),
    }
}

/// Items entered by hand with a UEX number are resolved by that number, not by
/// the name: a name may fit several things. The `bt` per number stays in the
/// state (`resolved_uex`). Gives `uex:<number>` to `bt`, per row through its key,
/// never through the name: a goods of the same name stays a goods.
fn resolve_uex_items(pass: &Pass<'_>, trade: &[Value], sub: &mut Map) -> Result<BTreeMap<String, String>, ApiError> {
    let mut refs: Vec<(String, Value)> = Vec::new();
    for row in trade {
        let Some(key) = stock_sync::uex_key(row) else { continue };
        let name = row_text(row, "ware");
        if name.is_empty() {
            continue;
        }
        let uex = row.get("uex").cloned().unwrap_or(Value::Null);
        let mut reference = Map::new();
        map_set(&mut reference, "uexId", uex);
        map_set(&mut reference, "name", Value::str(name.chars().take(200).collect::<String>()));
        refs.push((key, Value::Object(reference)));
    }
    if refs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut cache = take_object(sub, "resolved_uex");
    let found = feed::resolve_refs(pass.conn, "ITEM", &refs, &mut cache, pass.now);
    put_object(sub, "resolved_uex", cache);
    let found: BTreeMap<String, String> = found?.into_iter().collect();
    Ok(refs.iter().filter_map(|(key, _)| found.get(key).map(|bt| (key.clone(), bt.clone()))).collect())
}

fn places_of(map: &Map) -> Places {
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// The area: raw material stock and trade stock against the lots.
pub struct StockArea;

impl StockArea {
    /// Takes over what changed over there and moves rows that sit in the wrong
    /// stock (`_apply_stock`). The rows are looked for anew.
    fn apply(
        pass: &Pass<'_>,
        takes: &[(String, f64)],
        server: &BTreeMap<String, ServerLot>,
        resolver: &dyn Fn(&str) -> Option<String>,
        (places, items): (&Places, &BTreeMap<String, bool>),
        data: &StockData,
        stamps: &(Stamp, Stamp),
    ) -> Result<(), ApiError> {
        let paths: &Paths = pass.paths;
        let (raw_path, trade_path) = (paths.app_file(RAW_FILE), paths.app_file(TRADE_FILE));
        pass.apply_local(|| {
            // Not while the page reads, changes and writes the files (it holds the lock for a frame).
            let _lock = stock::lock();
            // Read once, and only if the files are as they were when the plan was made: the player
            // or the watcher may have written them during the pass, and a row would be overwritten.
            if (stamp_of(&raw_path), stamp_of(&trade_path)) != *stamps {
                return Err(store_busy("stock changed during the pass"));
            }
            let (Some(raw), Some(trade)) = (
                stock::load_checked(&raw_path, "basetool_sync.rohstoffe"),
                stock::load_checked(&trade_path, "basetool_sync.handelslager"),
            ) else {
                return Err(store_busy("stock unreadable"));
            };
            let (mut raw, mut trade, _moved) = stock_sync::relocate(&raw, &trade, resolver, places, server, items);
            let (local, _skipped) =
                stock_sync::local_lots(&raw, &trade, resolver, places, &|n| data.is_piece(n), items);
            let (mut drop_raw, mut drop_trade): (BTreeSet<usize>, BTreeSet<usize>) = (BTreeSet::new(), BTreeSet::new());
            for (key, amount) in takes {
                if let Some(lot) = local.get(key) {
                    let (store, drop): (&mut Vec<Value>, &mut BTreeSet<usize>) =
                        if lot.store == "raw" { (&mut raw, &mut drop_raw) } else { (&mut trade, &mut drop_trade) };
                    let (first, rest) = (lot.rows[0], &lot.rows[1..]);
                    if *amount > 0.0 {
                        if let Value::Object(row) = &mut store[first] {
                            let unit = server.get(key).map_or(lot.unit.as_str(), |s| s.unit.as_str());
                            map_set(row, "menge", num_value(*amount, unit));
                        }
                    } else {
                        drop.insert(first);
                    }
                    drop.extend(rest.iter().copied());
                    continue;
                }
                let Some(source) = server.get(key).filter(|_| *amount > 0.0) else { continue };
                let name = match source.material.get("name") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let place = match source.location.get("name") {
                    Some(Value::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                let quantity = num_value(*amount, &source.unit);
                if source.item {
                    trade.push(Value::Object(vec![
                        ("ware".into(), Value::str(&name)),
                        ("menge".into(), quantity),
                        ("ort".into(), Value::str(&place)),
                        ("gestohlen".into(), Value::Bool(source.stolen)),
                        ("stueck".into(), Value::Bool(true)),
                    ]));
                } else {
                    let mut row = vec![
                        ("material".to_string(), Value::str(&name)),
                        ("menge".to_string(), quantity),
                        ("qualitaet".to_string(), Value::int(source.quality)),
                        ("ort".to_string(), Value::str(&place)),
                    ];
                    if source.stolen {
                        row.push(("gestohlen".to_string(), Value::Bool(true)));
                    }
                    raw.push(Value::Object(row));
                }
            }
            let keep = |rows: Vec<Value>, drop: &BTreeSet<usize>| -> Vec<Value> {
                rows.into_iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, r)| r).collect()
            };
            let raw_saved = stock::save(&raw_path, &keep(raw, &drop_raw), "basetool_sync.rohstoffe");
            let trade_saved = stock::save(&trade_path, &keep(trade, &drop_trade), "basetool_sync.handelslager");
            if raw_saved && trade_saved { Ok(()) } else { Err(store_busy("stock not written")) }
        })
    }

    fn sync(pass: &mut Pass<'_>, sub: &mut Map) -> Result<Map, ApiError> {
        let before = server_map(sub);
        let (server_raw, _resync) = pull_generic(pass.conn, sub, "/me/stock", "key")?;
        let mut server_raw = split_complete(pass, sub, server_raw)?;
        let locations = pass.locations.get(pass.conn, pass.now)?.clone();
        let places = places_of(&locations);
        let paths: &Paths = pass.paths;
        let (raw_path, trade_path) = (paths.app_file(RAW_FILE), paths.app_file(TRADE_FILE));
        let stamps = (stamp_of(&raw_path), stamp_of(&trade_path));
        // One read each, no check before: a file that cannot be read now must not count as an empty stock.
        let (Some(raw), Some(trade)) = (
            stock::load_checked(&raw_path, "basetool_sync.rohstoffe"),
            stock::load_checked(&trade_path, "basetool_sync.handelslager"),
        ) else {
            return Err(store_busy("stock unreadable"));
        };
        if !(stock_sync::amounts_readable(&raw) && stock_sync::amounts_readable(&trade)) {
            return Err(ApiError::internal("STOCK_AMOUNT_UNREADABLE", "an amount of the stock is no number"));
        }
        let data = StockData::load(|name| paths.app_file(name));
        let uex_bts = resolve_uex_items(pass, &trade, sub)?;
        // Items with a UEX number do not go into the search by name; a goods or a raw material of the same name does.
        let mut names: BTreeSet<String> = raw.iter().map(|r| row_text(r, "material")).collect();
        names.extend(trade.iter().filter(|r| stock_sync::uex_key(r).is_none()).map(|r| row_text(r, "ware")));
        names.retain(|n| !n.is_empty());
        let refs: Vec<(String, Value)> = names
            .iter()
            .map(|n| {
                let mut reference = Map::new();
                map_set(&mut reference, "name", Value::str(n.chars().take(200).collect::<String>()));
                (n.to_lowercase(), Value::Object(reference))
            })
            .collect();
        let mut cache = take_object(sub, "resolved");
        let found = feed::resolve_refs(pass.conn, "MATERIAL", &refs, &mut cache, pass.now);
        put_object(sub, "resolved", cache);
        let mut resolved: BTreeMap<String, String> = found?.into_iter().collect();
        resolved.extend(uex_bts.iter().map(|(k, v)| (k.clone(), v.clone())));
        let uex_items: BTreeSet<String> = uex_bts.values().cloned().collect();

        // What stands over there carries its `bt` itself, also an item that cannot be resolved as a MATERIAL.
        for lot in server_raw.values() {
            let material = lot.get("material");
            let name = material
                .and_then(|m| m.get("name"))
                .and_then(Value::as_str)
                .map(|n| crate::text::py_strip(n).to_lowercase());
            let bt = material.and_then(|m| m.get("bt")).filter(|b| truthy(b)).map(crate::basetool::error::py_str);
            if let (Some(name), Some(bt)) = (name, bt)
                && !name.is_empty()
                && !resolved.contains_key(&name)
            {
                resolved.insert(name, bt);
            }
        }
        let lots: Vec<&Value> = server_raw.values().collect();
        let mut items = stock_sync::item_flags(&lots, Some(&bools(sub, "items")));
        for bt in &uex_items {
            items.insert(bt.clone(), true);
        }
        let rest: Vec<&String> = names.iter().filter(|n| !resolved.contains_key(&n.to_lowercase())).collect();
        if !rest.is_empty() {
            let refs: Vec<(String, Value)> = rest
                .iter()
                .map(|n| {
                    let mut reference = Map::new();
                    map_set(&mut reference, "name", Value::str(n.chars().take(200).collect::<String>()));
                    (n.to_lowercase(), Value::Object(reference))
                })
                .collect();
            let mut cache = take_object(sub, "resolved_items");
            let found = feed::resolve_refs(pass.conn, "ITEM", &refs, &mut cache, pass.now);
            put_object(sub, "resolved_items", cache);
            for (name, bt) in found? {
                resolved.entry(name).or_insert_with(|| bt.clone());
                items.entry(bt).or_insert(true);
            }
        }
        map_set(sub, "items", Value::Object(items.iter().map(|(k, v)| (k.clone(), Value::Bool(*v))).collect()));
        sub.retain(|(k, _)| k != "commodity");

        let resolver = |name: &str| resolved.get(&crate::text::py_strip(name).to_lowercase()).cloned();
        let lots: Vec<&Value> = server_raw.values().collect();
        let mut server = stock_sync::server_lots(&lots);
        let (raw, trade, moved) = stock_sync::relocate(&raw, &trade, &resolver, &places, &server, &items);
        let (local, skipped) = stock_sync::local_lots(&raw, &trade, &resolver, &places, &|n| data.is_piece(n), &items);
        let decisions = text_map(sub, "decisions");
        let baseline: Option<BTreeMap<String, f64>> = match get(sub, "baseline") {
            Some(Value::Object(inner)) => Some(inner.iter().map(|(k, v)| (k.clone(), number(Some(v)))).collect()),
            _ => None,
        };
        let open: BTreeSet<String> = match get(sub, "conflicts") {
            Some(Value::Object(inner)) => inner.iter().map(|(k, _)| k.clone()).collect(),
            _ => BTreeSet::new(),
        };
        let result = stock_sync::plan(&local, &server, baseline.as_ref(), &decisions, &open);
        let (result, guarded) = stock_sync::keep_unmapped(&result, &local, &server, &skipped);
        let overrides: BTreeSet<String> =
            decisions.iter().filter(|(_, v)| v.as_str() == "mine").map(|(k, _)| k.clone()).collect();
        let sets = stock_sync::change_sets(&result, &local, &server, 500, &overrides);
        let Posted { failed, applied, posted, paused, offers_reduced, offers_removed, .. } =
            send_unless_held(pass, ROUTE_STOCK, &sets)?;
        let rejected: BTreeMap<String, String> = failed
            .iter()
            .map(|(key, kind, reason)| (key.clone(), if reason.is_empty() { kind.clone() } else { reason.clone() }))
            .collect();

        if applied > 0 {
            map_set(sub, "server", names_text(&server_raw));
            server_raw = pull_generic(pass.conn, sub, "/me/stock", "key")?.0;
            let lots: Vec<&Value> = server_raw.values().collect();
            server = stock_sync::server_lots(&lots);
        }

        if !result.take.is_empty() || moved > 0 {
            Self::apply(pass, &result.take, &server, &resolver, (&places, &items), &data, &stamps)?;
        }

        let mut conflicts = result.conflicts.clone();
        for (key, reason) in &rejected {
            if reason == "REMOVED_ELSEWHERE" {
                let here = local.get(key).map_or(0.0, |l| l.amount);
                let there = server.get(key).map_or(0.0, |l| l.amount);
                conflicts.insert(key.clone(), (here, there));
            }
        }
        // The stand: what stands over there now. ⚠ Open conflicts keep their OLD stand, or the next pass would
        // take the amount here for an own change and send it away silently.
        let old: BTreeMap<String, f64> = baseline.unwrap_or_default();
        let mut new_baseline: BTreeMap<String, f64> = server.iter().map(|(k, lot)| (k.clone(), lot.amount)).collect();
        for key in conflicts.keys() {
            match old.get(key) {
                Some(value) => {
                    new_baseline.insert(key.clone(), *value);
                }
                None => {
                    new_baseline.remove(key);
                }
            }
        }
        let names_of: Map = conflicts
            .keys()
            .map(|key| {
                let lot_name = |lot: Option<(&Value, &Value, i64)>| match lot {
                    Some((material, location, quality)) => {
                        let m = material.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or("?");
                        let l = location.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).unwrap_or("?");
                        format!("{m} @ {l} (Q{quality})")
                    }
                    None => "? @ ? (Q0)".to_string(),
                };
                let text = match (local.get(key), server.get(key)) {
                    (Some(l), _) => lot_name(Some((&l.material, &l.location, l.quality))),
                    (None, Some(s)) => lot_name(Some((&s.material, &s.location, s.quality))),
                    (None, None) => lot_name(None),
                };
                (key.clone(), Value::str(text))
            })
            .collect();
        let kept_decisions: Map = if paused {
            decisions.iter().filter(|(k, _)| !posted.contains(k)).map(|(k, v)| (k.clone(), Value::str(v))).collect()
        } else {
            Map::new()
        };
        let skipped_state = |list: &Vec<String>| -> Value {
            let mut unique: Vec<String> = list.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect();
            unique.truncate(20);
            Value::Array(unique.into_iter().map(Value::Str).collect())
        };
        map_set(sub, "server", names_text(&server_raw));
        map_set(
            sub,
            "baseline",
            Value::Object(new_baseline.iter().map(|(k, v)| (k.clone(), Value::Float(*v))).collect()),
        );
        map_set(
            sub,
            "conflicts",
            Value::Object(
                conflicts
                    .iter()
                    .map(|(k, (h, t))| (k.clone(), Value::Array(vec![Value::Float(*h), Value::Float(*t)])))
                    .collect(),
            ),
        );
        map_set(sub, "conflict_names", Value::Object(names_of));
        map_set(sub, "decisions", Value::Object(kept_decisions));
        map_set(sub, "rejected", Value::Object(rejected.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect()));
        map_set(
            sub,
            "skipped",
            Value::Object(vec![
                ("material".into(), skipped_state(&skipped.material)),
                ("location".into(), skipped_state(&skipped.location)),
            ]),
        );
        // Personal or common stands only over there (`split`). When that changes, "My stock" draws its list
        // anew. 4.12 left the flag in the state for the pass to pop; here it only goes out with the counts.
        let split_changed = splits(&server_raw) != splits(&before);

        let distinct = |list: &Vec<String>| list.iter().collect::<BTreeSet<_>>().len() as i64;
        let mut counts = Map::new();
        for (name, value) in [
            ("sent", applied),
            ("taken", result.take.len() as i64),
            ("moved", moved as i64),
            ("conflicts", conflicts.len() as i64),
            ("rejected", rejected.len() as i64),
            ("skipped_location", distinct(&skipped.location)),
            ("skipped_material", distinct(&skipped.material)),
            ("guarded", guarded.len() as i64),
            ("paused", i64::from(paused)),
            ("offers", offers_reduced + offers_removed),
            ("split_changed", i64::from(split_changed)),
        ] {
            map_set(&mut counts, name, Value::int(value));
        }
        Ok(counts)
    }
}

impl Area for StockArea {
    fn name(&self) -> &'static str {
        "stock"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_STOCK)
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        let paths: &Paths = pass.paths;
        if !(store_ok(&paths.app_file(RAW_FILE), "posten") && store_ok(&paths.app_file(TRADE_FILE), "posten")) {
            crate::errors::record("basetool_sync.lager", &"Lagerdatei unlesbar, Lager wird nicht abgeglichen");
            return Ok(vec![("unreadable".into(), Value::int(1))]);
        }
        let mut sub = take_object(pass.state, "stock");
        let result = Self::sync(pass, &mut sub);
        put_object(pass.state, "stock", sub);
        result
    }

    fn forget(&mut self) {}
}
