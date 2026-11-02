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

//! The two stocks and the Basetool, the part without the net
//! (`exchange_stock.py`). The Basetool keeps the personal stock as **lots**: a
//! material or an item at a place, with quality and "stolen", and an amount.
//! Verse-Kit has two stocks that both fit it: the raw material stock (a
//! material with its quality) and the trade stock (goods under quality 0,
//! items in whole pieces).
//!
//! The rules:
//! 1. **The first sync only adds.** What is missing here comes in, what is
//!    missing there goes out (expected amount 0). A lot on **both** sides with
//!    different amounts is for the player to decide, never for Verse-Kit.
//! 2. **After that the comparison with the last state counts** (`baseline`): a
//!    change only here goes out with the amount that stood there last as
//!    `expectedQuantity`; a change only there is taken here; a change on both
//!    sides is for the player again.
//! 3. **`VERSION_CONFLICT` is solved by fetching**, never by repeating blindly.
//! 4. **Only places of `catalog/locations`.** A place the Basetool does not
//!    know stays here and is shown; it is never made.
//! 5. SCU with at most three decimals, pieces whole.
//!
//! A lot is known here by its **identifier** `material|place|quality|stolen`
//! (all lower case, the material by the `bt` key of the Basetool). Several
//! entries of a stock with one identifier are added up, just as the Basetool
//! books them.
//!
//! Nothing here asks the Basetool for anything (`tools/parity_stocksync.py`).

use std::collections::{BTreeMap, BTreeSet};

use crate::basetool::error::truthy;
use crate::jsonio::{Map, Value, map_set};
use crate::stock::py_round;
use crate::stock_data::py_float_value;

pub const SCU: &str = "SCU";
pub const PIECE: &str = "PIECE";

/// Shown for a place without a name (the dash of 4.12).
const NO_PLACE: &str = "\u{2014}";

/// Python's `float(x or 0)` for a value of a file.
fn float_or_zero(value: Option<&Value>) -> f64 {
    match value {
        Some(v) if truthy(v) => py_float_value(v).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Whether every `menge` of the rows is a number or empty. Python's `float(...)` raises on anything else
/// and the pass ends; with `float_or_zero` such a row would count as 0 and could be sent away as "emptied".
pub fn amounts_readable(rows: &[Value]) -> bool {
    rows.iter().all(|row| match field(row, "menge") {
        Some(v) if truthy(v) => py_float_value(v).is_some(),
        _ => true,
    })
}

/// Python's `int(x or 0)` for a number of a file (floats cut towards zero).
fn int_or_zero(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::Str(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// `_num`: whole pieces or SCU with three decimals.
pub fn num(amount: f64, unit: &str) -> f64 {
    if unit == PIECE { amount.round_ties_even() } else { py_round(amount, 3) }
}

/// `_num` as the value that goes into a file or a send: an integer for pieces.
pub fn num_value(amount: f64, unit: &str) -> Value {
    if unit == PIECE { Value::int(amount.round_ties_even() as i64) } else { Value::Float(py_round(amount, 3)) }
}

fn field<'a>(row: &'a Value, key: &str) -> Option<&'a Value> {
    row.get(key)
}

fn text_field(row: &Value, key: &str) -> String {
    match field(row, key) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// `uex:<number>` for an item entered by hand with a UEX number, else `None`.
/// Under this key stands its `bt` in the resolver, not under the name a goods
/// of the same name may carry.
pub fn uex_key(row: &Value) -> Option<String> {
    let pieces = field(row, "stueck").is_some_and(truthy);
    match field(row, "uex") {
        Some(Value::Int(n)) if pieces => n.parse::<i64>().ok().filter(|n| *n > 0).map(|n| format!("uex:{n}")),
        _ => None,
    }
}

/// What a trade stock row is resolved with: the UEX key or the name.
pub fn trade_lookup(row: &Value) -> String {
    uex_key(row).unwrap_or_else(|| text_field(row, "ware"))
}

/// `material|place|quality|stolen`.
pub fn identity(material_key: &str, location: &str, quality: i64, stolen: bool) -> String {
    format!(
        "{}|{}|{}|{}",
        material_key.to_lowercase(),
        crate::text::py_strip(location).to_lowercase(),
        quality,
        i32::from(stolen)
    )
}

/// Is this lot of the Basetool an item (not a material)?
pub fn is_item(lot: &Value) -> bool {
    let kind = match field(lot, "kind") {
        Some(v) if truthy(v) => crate::basetool::error::py_str(v).to_uppercase(),
        _ => String::new(),
    };
    if kind == "MATERIAL" || kind == "ITEM" {
        return kind == "ITEM";
    }
    !matches!(field(lot, "materialKind"), Some(Value::Object(_)))
}

/// A lot of the Basetool.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerLot {
    pub key: Value,
    pub material: Value,
    pub location: Value,
    pub quality: i64,
    pub stolen: bool,
    pub amount: f64,
    pub unit: String,
    pub item: bool,
}

/// The lots of the Basetool by identifier. Several with one identifier (there
/// should be none) are added up.
pub fn server_lots(items: &[&Value]) -> BTreeMap<String, ServerLot> {
    let mut out: BTreeMap<String, ServerLot> = BTreeMap::new();
    for lot in items {
        let empty = Value::Object(Map::new());
        let material = match field(lot, "material") {
            Some(v) if truthy(v) => v,
            _ => &empty,
        };
        let location = match field(lot, "location") {
            Some(v) if truthy(v) => v,
            _ => &empty,
        };
        let material_key = match field(material, "bt") {
            Some(v) if truthy(v) => crate::basetool::error::py_str(v),
            _ => match field(material, "name") {
                Some(v) if truthy(v) => crate::basetool::error::py_str(v),
                _ => String::new(),
            },
        };
        let place = match field(location, "name") {
            Some(v) if truthy(v) => crate::basetool::error::py_str(v),
            _ => String::new(),
        };
        let quality = int_or_zero(field(lot, "quality"));
        let stolen = field(lot, "stolen").is_some_and(truthy);
        let key = identity(&material_key, &place, quality, stolen);
        let quantity = match field(lot, "quantity") {
            Some(v) if truthy(v) => v.clone(),
            _ => Value::Object(Map::new()),
        };
        let unit = match field(&quantity, "unit") {
            Some(Value::Str(u)) if !u.is_empty() => u.clone(),
            _ => SCU.to_string(),
        };
        let amount = float_or_zero(field(&quantity, "amount"));
        if let Some(known) = out.get_mut(&key) {
            known.amount = num(known.amount + amount, &unit);
            continue;
        }
        out.insert(
            key,
            ServerLot {
                key: field(lot, "key").cloned().unwrap_or(Value::Null),
                material: material.clone(),
                location: location.clone(),
                quality,
                stolen,
                amount: num(amount, &unit),
                unit,
                item: is_item(lot),
            },
        );
    }
    out
}

/// `{bt: true/false}`: which `bt` the Basetool keeps as an item. `known` is
/// what was learnt last time, so it does not vanish with the last lot.
pub fn item_flags(items: &[&Value], known: Option<&BTreeMap<String, bool>>) -> BTreeMap<String, bool> {
    let mut out = known.cloned().unwrap_or_default();
    for lot in items {
        if let Some(bt) = field(lot, "material").and_then(|m| field(m, "bt")).filter(|b| truthy(b)) {
            out.insert(crate::basetool::error::py_str(bt), is_item(lot));
        }
    }
    out
}

/// A lot of this side.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalLot {
    /// `{bt, name}`.
    pub material: Value,
    /// The place as `catalog/locations` has it (`name`, `uex`).
    pub location: Value,
    pub quality: i64,
    pub stolen: bool,
    pub amount: f64,
    pub unit: String,
    /// `raw` or `trade`.
    pub store: &'static str,
    /// The positions of the rows in their stock.
    pub rows: Vec<usize>,
}

/// What could not be matched.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skipped {
    /// Unknown material.
    pub material: Vec<String>,
    /// A place the Basetool does not know, or none.
    pub location: Vec<String>,
}

/// The places of `catalog/locations`: lower case name to the place.
pub type Places = BTreeMap<String, Value>;

fn head(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

/// Both stocks to `{identifier: lot}` and what could not be matched.
/// `resolve(name)` gives the `bt` or nothing; `is_piece(name)` says whether the
/// material counts in pieces; `items` (`bt` to true for items) puts items under
/// quality 0 in whole pieces, whatever the stock.
pub fn local_lots(
    raw: &[Value],
    trade: &[Value],
    resolve: &dyn Fn(&str) -> Option<String>,
    places: &Places,
    is_piece: &dyn Fn(&str) -> bool,
    items: &BTreeMap<String, bool>,
) -> (BTreeMap<String, LocalLot>, Skipped) {
    let mut out: BTreeMap<String, LocalLot> = BTreeMap::new();
    let mut skipped = Skipped::default();
    let mut put = |store: &'static str,
                   name: &str,
                   amount: f64,
                   location: &str,
                   quality: i64,
                   stolen: bool,
                   index: usize,
                   lookup: Option<&str>| {
        let Some(bt) = resolve(lookup.filter(|l| !l.is_empty()).unwrap_or(name)).filter(|b| !b.is_empty()) else {
            skipped.material.push(name.to_string());
            return;
        };
        let Some(place) = places.get(&crate::text::py_strip(location).to_lowercase()) else {
            skipped.location.push(if location.is_empty() { NO_PLACE.to_string() } else { location.to_string() });
            return;
        };
        let (unit, quality) = if items.get(&bt).copied().unwrap_or(false) {
            (PIECE, 0)
        } else {
            (if store == "raw" && is_piece(name) { PIECE } else { SCU }, quality)
        };
        let place_name = match place.get("name") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let key = identity(&bt, &place_name, quality, stolen);
        let entry = out.entry(key).or_insert_with(|| LocalLot {
            material: Value::Object(vec![
                ("bt".to_string(), Value::str(&bt)),
                ("name".to_string(), Value::str(head(name, 200))),
            ]),
            location: place.clone(),
            quality,
            stolen,
            amount: 0.0,
            unit: unit.to_string(),
            store,
            rows: Vec::new(),
        });
        entry.amount = num(entry.amount + amount, unit);
        entry.rows.push(index);
    };
    for (index, row) in raw.iter().enumerate() {
        let quality = match field(row, "qualitaet") {
            Some(Value::Null) | None => 0,
            Some(v) => py_float_value(v).map_or(0, |q| q.round_ties_even() as i64),
        };
        put(
            "raw",
            &text_field(row, "material"),
            float_or_zero(field(row, "menge")),
            &text_field(row, "ort"),
            quality,
            field(row, "gestohlen").is_some_and(truthy),
            index,
            None,
        );
    }
    for (index, row) in trade.iter().enumerate() {
        put(
            "trade",
            &text_field(row, "ware"),
            float_or_zero(field(row, "menge")),
            &text_field(row, "ort"),
            0,
            field(row, "gestohlen").is_some_and(truthy),
            index,
            Some(&trade_lookup(row)),
        );
    }
    (out, skipped)
}

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Pulls rows that sit in the wrong stock into the right one, only here and
/// without a send to the Basetool. Gives (raw, trade, how many were moved).
///
/// * An item in the raw material stock goes to the trade stock; its identifier
///   stays the same (quality 0).
/// * A material in the trade stock goes to the raw material stock if its lot
///   under quality 0 is not there but **exactly one** lot of the same material
///   at the same place, equally stolen and with the same amount carries a
///   quality: the row gets it. Without a clear hit it stays where it is.
pub fn relocate(
    raw: &[Value],
    trade: &[Value],
    resolve: &dyn Fn(&str) -> Option<String>,
    places: &Places,
    server: &BTreeMap<String, ServerLot>,
    items: &BTreeMap<String, bool>,
) -> (Vec<Value>, Vec<Value>, usize) {
    let mut keep_raw: Vec<Value> = Vec::new();
    let mut new_trade: Vec<Value> = Vec::new();
    let mut moved = 0;
    for row in raw {
        let bt = resolve(&text_field(row, "material")).filter(|b| !b.is_empty());
        if bt.as_ref().is_some_and(|bt| items.get(bt).copied().unwrap_or(false)) {
            new_trade.push(object(vec![
                ("ware", Value::str(text_field(row, "material"))),
                ("menge", num_value(float_or_zero(field(row, "menge")), PIECE)),
                ("ort", Value::str(text_field(row, "ort"))),
                ("gestohlen", Value::Bool(field(row, "gestohlen").is_some_and(truthy))),
                ("stueck", Value::Bool(true)),
            ]));
            moved += 1;
        } else {
            keep_raw.push(row.clone());
        }
    }

    let place_of = |row: &Value| places.get(&crate::text::py_strip(&text_field(row, "ort")).to_lowercase());
    let place_name = |place: &Value| match place.get("name") {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    };
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for row in &keep_raw {
        let bt = resolve(&text_field(row, "material")).filter(|b| !b.is_empty());
        if let (Some(bt), Some(place)) = (bt, place_of(row)) {
            let quality = match field(row, "qualitaet") {
                Some(Value::Null) | None => 0,
                Some(v) => py_float_value(v).map_or(0, |q| q.round_ties_even() as i64),
            };
            taken.insert(identity(&bt, &place_name(place), quality, field(row, "gestohlen").is_some_and(truthy)));
        }
    }

    struct Group {
        key: String,
        bt: String,
        place: String,
        stolen: bool,
        rows: Vec<usize>,
    }
    let mut groups: Vec<Group> = Vec::new();
    for (index, row) in trade.iter().enumerate() {
        let bt = resolve(&trade_lookup(row)).filter(|b| !b.is_empty());
        let place = place_of(row);
        let (Some(bt), Some(place)) = (bt, place) else { continue };
        // Items count as such (and an unknown one as well): only what the
        // Basetool is known to keep as a material can move.
        if items.get(&bt).copied().unwrap_or(true) {
            continue;
        }
        let stolen = field(row, "gestohlen").is_some_and(truthy);
        let place = place_name(place);
        let key = identity(&bt, &place, 0, stolen);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(group) => group.rows.push(index),
            None => groups.push(Group { key, bt: bt.to_lowercase(), place, stolen, rows: vec![index] }),
        }
    }

    let mut drop: BTreeSet<usize> = BTreeSet::new();
    let mut new_raw: Vec<Value> = Vec::new();
    for group in &groups {
        if server.contains_key(&group.key) {
            continue;
        }
        let first = &trade[group.rows[0]];
        let total: f64 = group.rows.iter().fold(0.0, |sum, i| sum + float_or_zero(field(&trade[*i], "menge")));
        let matches: Vec<&ServerLot> = server
            .iter()
            .filter(|(lot_key, lot)| {
                let bt = field(&lot.material, "bt").map(crate::basetool::error::py_str).unwrap_or_default();
                let place = field(&lot.location, "name").map(crate::basetool::error::py_str).unwrap_or_default();
                !lot.item
                    && lot.quality != 0
                    && !taken.contains(*lot_key)
                    && bt.to_lowercase() == group.bt
                    && crate::text::py_strip(&place).to_lowercase()
                        == crate::text::py_strip(&group.place).to_lowercase()
                    && lot.stolen == group.stolen
                    && num(total, &lot.unit) == lot.amount
            })
            .map(|(_, lot)| lot)
            .collect();
        if matches.len() != 1 {
            continue;
        }
        let lot = matches[0];
        let mut row = Map::new();
        map_set(&mut row, "material", Value::str(text_field(first, "ware")));
        map_set(&mut row, "menge", num_value(total, &lot.unit));
        map_set(&mut row, "qualitaet", Value::int(lot.quality));
        map_set(&mut row, "ort", Value::str(text_field(first, "ort")));
        if group.stolen {
            map_set(&mut row, "gestohlen", Value::Bool(true));
        }
        new_raw.push(Value::Object(row));
        drop.extend(group.rows.iter().copied());
        moved += group.rows.len();
    }

    let mut trade_out: Vec<Value> =
        trade.iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, r)| r.clone()).collect();
    let mut raw_out = keep_raw;
    raw_out.extend(new_raw);
    trade_out.extend(new_trade);
    (raw_out, trade_out, moved)
}

/// What to do: pushes `(identifier, amount here, amount expected there)`,
/// takes `(identifier, amount there)`, and the conflicts `(here, there)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StockPlan {
    pub push: Vec<(String, f64, f64)>,
    pub take: Vec<(String, f64)>,
    pub conflicts: BTreeMap<String, (f64, f64)>,
}

/// What is to be done, without doing it. `baseline` is `{identifier: amount}`
/// of the last sync, `None` at the first. `decisions` is what the player chose
/// at a conflict (`mine` or `theirs`); `open_conflicts` are conflicts nobody
/// has decided yet: they stay conflicts until the player decides or both sides
/// agree again.
pub fn plan(
    local: &BTreeMap<String, LocalLot>,
    server: &BTreeMap<String, ServerLot>,
    baseline: Option<&BTreeMap<String, f64>>,
    decisions: &BTreeMap<String, String>,
    open_conflicts: &BTreeSet<String>,
) -> StockPlan {
    let first = baseline.is_none();
    let empty = BTreeMap::new();
    let base = baseline.unwrap_or(&empty);
    let mut result = StockPlan::default();
    let keys: BTreeSet<&String> = local.keys().chain(server.keys()).collect();
    for key in keys {
        let here = local.get(key).map_or(0.0, |l| l.amount);
        let there = server.get(key).map_or(0.0, |l| l.amount);
        if here == there {
            continue;
        }
        match decisions.get(key).map(String::as_str) {
            Some("mine") => {
                result.push.push((key.clone(), here, there));
                continue;
            }
            Some("theirs") => {
                result.take.push((key.clone(), there));
                continue;
            }
            _ => {}
        }
        if open_conflicts.contains(key) {
            result.conflicts.insert(key.clone(), (here, there));
            continue;
        }
        if first {
            if there == 0.0 {
                result.push.push((key.clone(), here, 0.0));
            } else if here == 0.0 {
                result.take.push((key.clone(), there));
            } else {
                result.conflicts.insert(key.clone(), (here, there));
            }
            continue;
        }
        let before = base.get(key).copied().unwrap_or(0.0);
        if there == before {
            result.push.push((key.clone(), here, before));
        } else if here == before {
            result.take.push((key.clone(), there));
        } else {
            result.conflicts.insert(key.clone(), (here, there));
        }
    }
    result
}

/// Protects lots of the Basetool at a place that `local_lots` had to skip
/// because it is not in the catalog of places. Gives the plan and the
/// identifiers for which something was left out.
///
/// ⚠ Without the place all rows of that place are missing in `local`, and
/// `plan` takes the lots there for removed here: they would go out with amount
/// 0. So no reduction goes out for them, nothing is taken here (a new row would
/// stand next to the skipped one) and there is no conflict. Skipped materials
/// protect nothing: a name the Basetool keeps for a lot is resolved through that
/// lot, and a name never resolved cannot have made a lot.
pub fn keep_unmapped(
    plan_result: &StockPlan,
    local: &BTreeMap<String, LocalLot>,
    server: &BTreeMap<String, ServerLot>,
    skipped: &Skipped,
) -> (StockPlan, BTreeSet<String>) {
    let mut places: BTreeSet<String> =
        skipped.location.iter().map(|p| crate::text::py_strip(p).to_lowercase()).collect();
    places.remove("");
    let mut guarded = BTreeSet::new();
    for (key, lot) in server {
        let place = match lot.location.get("name") {
            Some(Value::Str(s)) => crate::text::py_strip(s).to_lowercase(),
            _ => String::new(),
        };
        if places.contains(&place) {
            guarded.insert(key.clone());
        }
    }
    if guarded.is_empty() {
        return (plan_result.clone(), BTreeSet::new());
    }
    let amount_here = |key: &str| local.get(key).map_or(0.0, |l| l.amount);
    let push: Vec<(String, f64, f64)> =
        plan_result.push.iter().filter(|e| !(guarded.contains(&e.0) && e.1 < e.2)).cloned().collect();
    let take: Vec<(String, f64)> =
        plan_result.take.iter().filter(|e| !(guarded.contains(&e.0) && !local.contains_key(&e.0))).cloned().collect();
    let conflicts: BTreeMap<String, (f64, f64)> = plan_result
        .conflicts
        .iter()
        .filter(|(key, value)| !(guarded.contains(*key) && amount_here(key) < value.1))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    touched.extend(plan_result.push.iter().map(|e| e.0.clone()).filter(|k| !push.iter().any(|e| &e.0 == k)));
    touched.extend(plan_result.take.iter().map(|e| e.0.clone()).filter(|k| !take.iter().any(|e| &e.0 == k)));
    touched.extend(plan_result.conflicts.keys().filter(|k| !conflicts.contains_key(*k)).cloned());
    (StockPlan { push, take, conflicts }, touched)
}

/// The sends: `(change set, identifiers)`, a `set-quantity` for each lot. The
/// identifiers in `overrides` are those where the player chose "my amount": they
/// go with `override`, since a lot emptied over there would be refused as
/// `REMOVED_ELSEWHERE` otherwise.
///
/// ⚠ A lot that is not here at all is set to 0 with the data of the Basetool:
/// material, place and unit only the server still knows.
pub fn change_sets(
    plan_result: &StockPlan,
    local: &BTreeMap<String, LocalLot>,
    server: &BTreeMap<String, ServerLot>,
    batch: usize,
    overrides: &BTreeSet<String>,
) -> Vec<(Value, Vec<String>)> {
    let mut ops: Vec<Value> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for (n, (key, amount, expected)) in plan_result.push.iter().enumerate() {
        let (material, location, quality, stolen, unit) = match (local.get(key), server.get(key)) {
            (Some(l), _) => (&l.material, &l.location, l.quality, l.stolen, l.unit.as_str()),
            (None, Some(s)) => (&s.material, &s.location, s.quality, s.stolen, s.unit.as_str()),
            (None, None) => continue,
        };
        let keep = |value: &Value, names: &[&str]| -> Value {
            match value {
                Value::Object(map) => Value::Object(
                    map.iter().filter(|(k, v)| names.contains(&k.as_str()) && truthy(v)).cloned().collect(),
                ),
                _ => Value::Object(Map::new()),
            }
        };
        let mut op = Map::new();
        map_set(&mut op, "opId", Value::str(format!("s{}", n + 1)));
        map_set(&mut op, "op", Value::str("set-quantity"));
        map_set(&mut op, "material", keep(material, &["bt", "name"]));
        map_set(&mut op, "location", keep(location, &["name", "uex"]));
        map_set(&mut op, "quality", Value::int(quality));
        map_set(&mut op, "stolen", Value::Bool(stolen));
        map_set(&mut op, "quantity", object(vec![("amount", num_value(*amount, unit)), ("unit", Value::str(unit))]));
        map_set(
            &mut op,
            "expectedQuantity",
            object(vec![("amount", num_value(*expected, unit)), ("unit", Value::str(unit))]),
        );
        if overrides.contains(key) {
            map_set(&mut op, "override", Value::Bool(true));
        }
        ops.push(Value::Object(op));
        keys.push(key.clone());
    }
    ops.chunks(batch)
        .zip(keys.chunks(batch))
        .map(|(ops, keys)| (object(vec![("ops", Value::Array(ops.to_vec()))]), keys.to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lot(bt: &str, place: &str, quality: i64, amount: f64, unit: &str, item: bool) -> Value {
        let mut lot = vec![
            ("key".to_string(), Value::str(format!("{bt}-{place}-{quality}"))),
            (
                "material".to_string(),
                Value::Object(vec![("bt".into(), Value::str(bt)), ("name".into(), Value::str(bt))]),
            ),
            ("location".to_string(), Value::Object(vec![("name".into(), Value::str(place))])),
            ("quality".to_string(), Value::int(quality)),
            ("stolen".to_string(), Value::Bool(false)),
            (
                "quantity".to_string(),
                Value::Object(vec![("amount".into(), Value::Float(amount)), ("unit".into(), Value::str(unit))]),
            ),
        ];
        // Without the field a lot counts as an item, so a material must say so.
        lot.push(("kind".to_string(), Value::str(if item { "ITEM" } else { "MATERIAL" })));
        Value::Object(lot)
    }

    fn places() -> Places {
        [("orbituary".to_string(), Value::Object(vec![("name".into(), Value::str("Orbituary"))]))].into_iter().collect()
    }

    fn row(material: &str, amount: f64, quality: i64) -> Value {
        Value::Object(vec![
            ("material".into(), Value::str(material)),
            ("menge".into(), Value::Float(amount)),
            ("qualitaet".into(), Value::int(quality)),
            ("ort".into(), Value::str("Orbituary")),
        ])
    }

    #[test]
    fn lots_of_the_basetool_are_added_up_by_their_identifier_and_items_are_told() {
        let a = lot("bt-iron", "Orbituary", 500, 1.2345, SCU, false);
        let b = lot("bt-iron", "Orbituary", 500, 1.0, SCU, false);
        let c = lot("bt-gun", "Orbituary", 0, 2.0, PIECE, true);
        let server = server_lots(&[&a, &b, &c]);
        assert_eq!(server.len(), 2);
        assert_eq!(server["bt-iron|orbituary|500|0"].amount, 2.234, "added up and rounded to three decimals");
        assert!(server["bt-gun|orbituary|0|0"].item);
        let flags = item_flags(&[&a, &c], None);
        assert_eq!(flags.get("bt-gun"), Some(&true));
        assert_eq!(flags.get("bt-iron"), Some(&false));
    }

    #[test]
    fn the_first_sync_only_adds_and_a_difference_on_both_sides_is_for_the_player() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        let (local, skipped) = local_lots(
            &[row("Iron", 3.0, 500), row("Gold", 1.0, 100)],
            &[],
            &resolve,
            &places(),
            &|_| false,
            &BTreeMap::new(),
        );
        assert!(skipped.material.is_empty() && skipped.location.is_empty());
        let there = lot("bt-iron", "Orbituary", 500, 2.0, SCU, false);
        let copper = lot("bt-copper", "Orbituary", 100, 4.0, SCU, false);
        let server = server_lots(&[&there, &copper]);
        let result = plan(&local, &server, None, &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.push, [("bt-gold|orbituary|100|0".to_string(), 1.0, 0.0)], "missing there goes out");
        assert_eq!(result.take, [("bt-copper|orbituary|100|0".to_string(), 4.0)], "missing here comes in");
        assert_eq!(result.conflicts.get("bt-iron|orbituary|500|0"), Some(&(3.0, 2.0)));
    }

    #[test]
    fn after_the_first_sync_the_last_state_decides_who_changed() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        let (local, _) = local_lots(&[row("Iron", 3.0, 500)], &[], &resolve, &places(), &|_| false, &BTreeMap::new());
        let there = lot("bt-iron", "Orbituary", 500, 2.0, SCU, false);
        let server = server_lots(&[&there]);
        let key = "bt-iron|orbituary|500|0".to_string();
        // Only here changed (there still the old 2): out, expecting 2.
        let base: BTreeMap<String, f64> = [(key.clone(), 2.0)].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.push, [(key.clone(), 3.0, 2.0)]);
        // Only there changed (here still the old 3): taken.
        let base: BTreeMap<String, f64> = [(key.clone(), 3.0)].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.take, [(key.clone(), 2.0)]);
        // Both changed: the player.
        let base: BTreeMap<String, f64> = [(key.clone(), 1.0)].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.conflicts.get(&key), Some(&(3.0, 2.0)));
        // The player chose "mine": out with override.
        let decisions: BTreeMap<String, String> = [(key.clone(), "mine".to_string())].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &decisions, &BTreeSet::new());
        let overrides: BTreeSet<String> = [key.clone()].into_iter().collect();
        let sets = change_sets(&result, &local, &server, 500, &overrides);
        assert_eq!(sets.len(), 1);
        let op = match sets[0].0.get("ops") {
            Some(Value::Array(ops)) => ops[0].clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(op.get("override"), Some(&Value::Bool(true)));
        assert_eq!(op.get("expectedQuantity").and_then(|q| q.get("amount")), Some(&Value::Float(2.0)));
    }

    #[test]
    fn a_place_the_basetool_does_not_know_protects_its_lots() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        let mut other = row("Iron", 3.0, 500);
        if let Value::Object(map) = &mut other {
            map_set(map, "ort", Value::str("Nirgendwo"));
        }
        let (local, skipped) = local_lots(&[other], &[], &resolve, &places(), &|_| false, &BTreeMap::new());
        assert!(local.is_empty());
        assert_eq!(skipped.location, ["Nirgendwo"]);
        // The lot over there stands at "Nirgendwo" as well (a place that is gone from the catalog).
        let there = lot("bt-iron", "Nirgendwo", 500, 2.0, SCU, false);
        let server = server_lots(&[&there]);
        let base: BTreeMap<String, f64> = [("bt-iron|nirgendwo|500|0".to_string(), 2.0)].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.push.len(), 1, "without the guard the lot would be set to 0 over there");
        let (guarded, touched) = keep_unmapped(&result, &local, &server, &skipped);
        assert!(guarded.push.is_empty() && !touched.is_empty());
    }

    #[test]
    fn an_item_in_the_raw_stock_moves_to_the_trade_stock() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        let items: BTreeMap<String, bool> = [("bt-gun".to_string(), true)].into_iter().collect();
        let (raw, trade, moved) =
            relocate(&[row("Gun", 2.0, 0), row("Iron", 1.0, 500)], &[], &resolve, &places(), &BTreeMap::new(), &items);
        assert_eq!(moved, 1);
        assert_eq!(raw.len(), 1);
        assert_eq!(trade.len(), 1);
        assert_eq!(trade[0].get("menge"), Some(&Value::int(2)));
        assert_eq!(trade[0].get("stueck"), Some(&Value::Bool(true)));
    }
}
