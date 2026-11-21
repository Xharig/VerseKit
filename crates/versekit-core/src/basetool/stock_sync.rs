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

//! The two stocks and the Basetool, the part without the net
//! (`exchange_stock.py`). The Basetool keeps the personal stock as **lots**: a
//! material or an item at a place, with quality and "stolen", and an amount.
//! VerseKit has two stocks that both fit it: the raw material stock (a
//! material with its quality) and the trade stock (goods under quality 0,
//! items in whole pieces).
//!
//! The rules:
//! 1. **The first sync only adds.** What is missing here comes in, what is
//!    missing there goes out (expected amount 0). A lot on **both** sides with
//!    different amounts is for the player to decide, never for VerseKit.
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
pub const NO_PLACE: &str = "\u{2014}";

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
    /// `{bt, name}`, for an item with its class `scRecord` where it is known.
    pub material: Value,
    /// The Basetool keeps it as an item.
    pub item: bool,
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
        let item = items.get(&bt).copied().unwrap_or(false);
        let (unit, quality) = if item {
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
            item,
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
///
/// ⚠ An item goes out by `bt` and `scRecord` alone, without `name`: the Basetool asks its material catalogue
/// first while a name stands there, and a near material name makes the change `AMBIGUOUS` (basetool#3058).
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
        let (material, location, quality, stolen, unit, item) = match (local.get(key), server.get(key)) {
            (Some(l), _) => (&l.material, &l.location, l.quality, l.stolen, l.unit.as_str(), l.item),
            (None, Some(s)) => (&s.material, &s.location, s.quality, s.stolen, s.unit.as_str(), s.item),
            (None, None) => continue,
        };
        let material_fields: &[&str] = if item { &["bt", "scRecord"] } else { &["bt", "name"] };
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
        let reference = match keep(material, material_fields) {
            Value::Object(fields) if fields.is_empty() => keep(material, &["bt", "name"]),
            fields => fields,
        };
        map_set(&mut op, "material", reference);
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
    in_material_sets(&keys, batch)
        .into_iter()
        .map(|at| {
            let ops: Vec<Value> = at.iter().map(|i| ops[*i].clone()).collect();
            let keys: Vec<String> = at.iter().map(|i| keys[*i].clone()).collect();
            (object(vec![("ops", Value::Array(ops))]), keys)
        })
        .collect()
}

/// The material of an identifier (`material|place|quality|stolen`).
pub fn material_of(key: &str) -> &str {
    key.rsplitn(4, '|').last().unwrap_or(key)
}

/// The identifier with another material in front.
fn with_material(key: &str, material: &str) -> String {
    match key.split_once('|') {
        Some((_, rest)) => format!("{}|{rest}", material.to_lowercase()),
        None => material.to_lowercase(),
    }
}

/// A material or item the Basetool folded into another: the `bt` and the name that take its place.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    pub bt: String,
    pub name: String,
    /// The Basetool keeps it as an item.
    pub item: bool,
}

/// A lot of the feed, as far as a merge is told by it.
struct Seen {
    bt: String,
    name: String,
    spot: (String, i64, bool),
    amount: f64,
    unit: String,
    item: bool,
}

fn seen(lot: &Value) -> Option<Seen> {
    let material = field(lot, "material")?;
    let bt = field(material, "bt").filter(|b| truthy(b)).map(crate::basetool::error::py_str)?;
    let name = field(material, "name").filter(|n| truthy(n)).map(crate::basetool::error::py_str).unwrap_or_default();
    let place = field(lot, "location")
        .and_then(|l| field(l, "name"))
        .filter(|n| truthy(n))
        .map(crate::basetool::error::py_str)
        .unwrap_or_default();
    let quantity = field(lot, "quantity");
    let unit = match quantity.and_then(|q| field(q, "unit")) {
        Some(Value::Str(u)) if !u.is_empty() => u.clone(),
        _ => SCU.to_string(),
    };
    Some(Seen {
        bt,
        name,
        spot: (
            crate::text::py_strip(&place).to_lowercase(),
            int_or_zero(field(lot, "quality")),
            field(lot, "stolen").is_some_and(truthy),
        ),
        amount: float_or_zero(quantity.and_then(|q| field(q, "amount"))),
        unit,
        item: is_item(lot),
    })
}

/// Merges the feed shows: a lot that is gone (its material has no lot left over there) while a lot of another
/// material at the same place, quality and "stolen" came or grew by exactly its amount. `before` and `after` are
/// the lots over there by key before and after fetching. Gives the old `bt` (as the Basetool wrote it) and what
/// takes its place; a material that points to two different ones is left out.
///
/// ⚠ This is only a guess from amounts. Whoever takes it over asks the catalog first whether the old `bt` is
/// really gone or resolves to the new one.
pub fn merge_candidates(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> BTreeMap<String, Merge> {
    let left: BTreeSet<String> = after.values().filter_map(seen).map(|s| s.bt.to_lowercase()).collect();
    let mut gone: BTreeMap<(String, i64, bool), Vec<Seen>> = BTreeMap::new();
    for (key, lot) in before {
        if after.contains_key(key) {
            continue;
        }
        if let Some(lot) = seen(lot).filter(|s| !left.contains(&s.bt.to_lowercase())) {
            gone.entry(lot.spot.clone()).or_default().push(lot);
        }
    }
    let mut found: BTreeMap<String, Vec<Merge>> = BTreeMap::new();
    for (spot, old) in &gone {
        let old_bts: BTreeSet<String> = old.iter().map(|s| s.bt.to_lowercase()).collect();
        let had = |bt: &str| -> f64 {
            before.values().filter_map(seen).filter(|s| s.spot == *spot && s.bt.to_lowercase() == bt).map(|s| s.amount).sum()
        };
        let fresh: Vec<Seen> = after
            .iter()
            .filter(|(key, lot)| match (seen(lot), before.get(*key).and_then(seen)) {
                (Some(now), Some(then)) => now.amount != then.amount || now.bt != then.bt,
                (Some(_), None) => true,
                _ => false,
            })
            .filter_map(|(_, lot)| seen(lot))
            .filter(|s| s.spot == *spot && !old_bts.contains(&s.bt.to_lowercase()))
            .collect();
        let grew_by = |new: &Seen, by: f64| num(by + had(&new.bt.to_lowercase()), &new.unit) == num(new.amount, &new.unit);
        let all: f64 = old.iter().map(|s| s.amount).sum();
        let together: Vec<&Seen> = fresh.iter().filter(|new| grew_by(new, all)).collect();
        for gone_lot in old {
            let target = match together.as_slice() {
                [one] => Some(*one),
                _ => match fresh.iter().filter(|new| grew_by(new, gone_lot.amount)).collect::<Vec<_>>().as_slice() {
                    [one] => Some(*one),
                    _ => None,
                },
            };
            if let Some(new) = target {
                found.entry(gone_lot.bt.clone()).or_default().push(Merge {
                    bt: new.bt.clone(),
                    name: new.name.clone(),
                    item: new.item,
                });
            }
        }
    }
    found
        .into_iter()
        .filter_map(|(old, targets)| {
            let first = targets.first()?.clone();
            targets.iter().all(|t| t.bt == first.bt).then_some((old, first))
        })
        .collect()
}

/// The key of a row as the resolver knows it: its UEX key, else its name in lower case.
pub fn lookup_key(row: &Value, store: &str) -> String {
    if store == "trade"
        && let Some(key) = uex_key(row)
    {
        return key;
    }
    let name = text_field(row, if store == "raw" { "material" } else { "ware" });
    crate::text::py_strip(&name).to_lowercase()
}

fn add_amounts(a: Option<&Value>, b: Option<&Value>) -> Value {
    match (a, b) {
        (Some(Value::Int(x)), Some(Value::Int(y))) => {
            Value::int(x.parse::<i64>().unwrap_or(0) + y.parse::<i64>().unwrap_or(0))
        }
        _ => Value::Float(py_round(float_or_zero(a) + float_or_zero(b), 3)),
    }
}

/// Moves the rows of a material the Basetool folded into another (`renames`: lookup key of the row → the new
/// name): they take the new name, and rows that stand alike afterwards (same name, place, quality, "stolen") are
/// put together into the first of them, so that no second row stands for one lot. Rows that stood alike before and
/// were not moved stay as they are. Gives both stocks and how many rows moved.
pub fn follow_merges(raw: &[Value], trade: &[Value], renames: &BTreeMap<String, String>) -> (Vec<Value>, Vec<Value>, usize) {
    let mut moved = 0;
    let mut fold = |rows: &[Value], store: &'static str| -> Vec<Value> {
        let name_field = if store == "raw" { "material" } else { "ware" };
        let mut rows: Vec<Value> = rows.to_vec();
        let mut touched: BTreeSet<usize> = BTreeSet::new();
        for (index, row) in rows.iter_mut().enumerate() {
            let Some(new) = renames.get(&lookup_key(row, store)) else { continue };
            if text_field(row, name_field) == *new {
                continue;
            }
            if let Value::Object(map) = row {
                map_set(map, name_field, Value::str(new));
                touched.insert(index);
            }
        }
        moved += touched.len();
        if touched.is_empty() {
            return rows;
        }
        let alike = |row: &Value| -> String {
            let quality = match field(row, "qualitaet") {
                Some(Value::Null) | None => 0,
                Some(v) => py_float_value(v).map_or(0, |q| q.round_ties_even() as i64),
            };
            format!(
                "{}|{}|{}|{}|{}|{}",
                lookup_key(row, store),
                crate::text::py_strip(&text_field(row, "ort")).to_lowercase(),
                quality,
                field(row, "gestohlen").is_some_and(truthy),
                field(row, "stueck").is_some_and(truthy),
                field(row, "uex").map(crate::basetool::error::py_str).unwrap_or_default(),
            )
        };
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            groups.entry(alike(row)).or_default().push(index);
        }
        let mut drop: BTreeSet<usize> = BTreeSet::new();
        for members in groups.values().filter(|m| m.len() > 1 && m.iter().any(|i| touched.contains(i))) {
            let first = members[0];
            for other in &members[1..] {
                let sum = add_amounts(field(&rows[first], "menge"), field(&rows[*other], "menge"));
                if let Value::Object(map) = &mut rows[first] {
                    map_set(map, "menge", sum);
                }
                drop.insert(*other);
            }
        }
        rows.into_iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, r)| r).collect()
    };
    let raw = fold(raw, "raw");
    let trade = fold(trade, "trade");
    (raw, trade, moved)
}

/// The amounts of the last stand under the new material where one was folded into another
/// (`merged`: old `bt` in lower case → new `bt`): the lots of both are one now, so their amounts add up.
pub fn rekey_amounts(stand: &BTreeMap<String, f64>, merged: &BTreeMap<String, String>) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for (key, amount) in stand {
        let key = match merged.get(material_of(key)) {
            Some(new) => with_material(key, new),
            None => key.clone(),
        };
        *out.entry(key).or_insert(0.0) += amount;
    }
    out
}

/// The keys of a decision or an open conflict under the new material; one that meets an entry of the new material
/// already gives way to it.
pub fn rekey_keys<V: Clone>(entries: &BTreeMap<String, V>, merged: &BTreeMap<String, String>) -> BTreeMap<String, V> {
    let mut out: BTreeMap<String, V> = entries
        .iter()
        .filter(|(key, _)| !merged.contains_key(material_of(key)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (key, value) in entries {
        if let Some(new) = merged.get(material_of(key)) {
            out.entry(with_material(key, new)).or_insert_with(|| value.clone());
        }
    }
    out
}

/// The reasons the Basetool gives for a material or item it no longer takes in.
pub const RETIRED: [&str; 2] = ["MATERIAL_RETIRED", "ITEM_RETIRED"];

/// Leaves out what may not go to the Basetool any more: an addition of a retired material (`retired`: `bt` in lower
/// case → reason). Reductions still go. A move (a reduction at one place and an addition at another in the same
/// send) still goes as a whole as long as no more is added than taken away. Gives the plan and the identifiers left
/// out, with the reason.
pub fn hold_retired(plan_result: &StockPlan, retired: &BTreeMap<String, String>) -> (StockPlan, BTreeMap<String, String>) {
    if retired.is_empty() {
        return (plan_result.clone(), BTreeMap::new());
    }
    let mut change: BTreeMap<&str, f64> = BTreeMap::new();
    for (key, here, expected) in &plan_result.push {
        let material = material_of(key);
        if retired.contains_key(material) {
            *change.entry(material).or_insert(0.0) += here - expected;
        }
    }
    let mut held = BTreeMap::new();
    let push = plan_result
        .push
        .iter()
        .filter(|(key, here, expected)| {
            let material = material_of(key);
            let keep = here <= expected || change.get(material).is_none_or(|sum| *sum <= 1e-9);
            if !keep {
                held.insert(key.clone(), retired[material].clone());
            }
            keep
        })
        .cloned()
        .collect();
    (StockPlan { push, take: plan_result.take.clone(), conflicts: plan_result.conflicts.clone() }, held)
}

/// Which instructions go into which change set, by their place in `keys`: all lots of one material in the
/// same set, in the order they came; a set is closed between two materials, never inside one.
///
/// ⚠ The Basetool holds back a change set in which a lot goes to 0 without an equal addition of the same
/// material in the same set (basetool#3420): a move to another place is the reduction at the old place and
/// the addition at the new one. Cut by count (`chunks(500)`) a move over the cut left a set of reductions
/// alone, and the lot stayed held back. A single material above `batch` lots cannot stay whole (the Basetool
/// refuses a send of more than 500 instructions altogether), so only that one is cut, at `batch`.
fn in_material_sets(keys: &[String], batch: usize) -> Vec<Vec<usize>> {
    let batch = batch.max(1);
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    for (at, key) in keys.iter().enumerate() {
        let material = material_of(key);
        match groups.iter_mut().find(|(name, _)| *name == material) {
            Some((_, members)) => members.push(at),
            None => groups.push((material, vec![at])),
        }
    }
    let mut sets: Vec<Vec<usize>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (_, members) in groups {
        if !open.is_empty() && open.len() + members.len() > batch {
            sets.push(std::mem::take(&mut open));
        }
        if members.len() > batch {
            sets.extend(members.chunks(batch).map(<[usize]>::to_vec));
        } else {
            open.extend(members);
        }
    }
    if !open.is_empty() {
        sets.push(open);
    }
    sets
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
    fn an_item_goes_out_by_its_keys_without_the_name_and_a_material_keeps_its_name() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        let items: BTreeMap<String, bool> = [("bt-coverall".to_string(), true)].into_iter().collect();
        let trade = [Value::Object(vec![
            ("ware".into(), Value::str("CoverAll")),
            ("menge".into(), Value::int(1)),
            ("ort".into(), Value::str("Orbituary")),
            ("stueck".into(), Value::Bool(true)),
        ])];
        let (mut local, _) = local_lots(&[row("Iron", 3.0, 500)], &trade, &resolve, &places(), &|_| false, &items);
        let shield = local.get_mut("bt-coverall|orbituary|0|0").expect("the item");
        assert!(shield.item);
        if let Value::Object(material) = &mut shield.material {
            map_set(material, "scRecord", Value::str("SHLD_GODI_S02_CoverAll_SCItem"));
        }
        let result = plan(&local, &BTreeMap::new(), None, &BTreeMap::new(), &BTreeSet::new());
        let sets = change_sets(&result, &local, &BTreeMap::new(), 500, &BTreeSet::new());
        let Some(Value::Array(ops)) = sets[0].0.get("ops") else { panic!("ops") };
        let material_of = |bt: &str| {
            ops.iter()
                .find_map(|op| op.get("material").filter(|m| m.get("bt") == Some(&Value::str(bt))).cloned())
                .expect(bt)
        };
        assert_eq!(
            material_of("bt-coverall"),
            Value::Object(vec![
                ("bt".into(), Value::str("bt-coverall")),
                ("scRecord".into(), Value::str("SHLD_GODI_S02_CoverAll_SCItem")),
            ])
        );
        assert_eq!(material_of("bt-iron").get("name"), Some(&Value::str("Iron")), "a raw material stays as it is");
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

    fn row_at(material: &str, amount: f64, quality: i64, place: &str) -> Value {
        let mut row = row(material, amount, quality);
        if let Value::Object(map) = &mut row {
            map_set(map, "ort", Value::str(place));
        }
        row
    }

    fn two_places() -> Places {
        let mut places = places();
        places.insert("levski".to_string(), Value::Object(vec![("name".into(), Value::str("Levski"))]));
        places
    }

    fn ops_of(set: &(Value, Vec<String>)) -> Vec<&Value> {
        match set.0.get("ops") {
            Some(Value::Array(ops)) => ops.iter().collect(),
            other => panic!("{other:?}"),
        }
    }

    /// The Basetool holds back a set in which a lot goes to 0 without an equal addition of the same material
    /// in the same set (basetool#3420): a move needs both halves in one set, wherever the count falls.
    #[test]
    fn a_move_of_a_material_stays_in_one_set_whatever_the_count() {
        let resolve = |name: &str| Some(format!("bt-{}", name.to_lowercase()));
        // 499 other materials, sorted ahead of Iron, so that the cut at 500 falls between the two lots of Iron.
        let mut rows: Vec<Value> = (0..499).map(|n| row_at(&format!("A{n:03}"), 1.0, 0, "Orbituary")).collect();
        rows.push(row_at("Iron", 2.0, 500, "Levski"));
        let (local, skipped) = local_lots(&rows, &[], &resolve, &two_places(), &|_| false, &BTreeMap::new());
        assert!(skipped.material.is_empty() && skipped.location.is_empty());
        let old = lot("bt-iron", "Orbituary", 500, 2.0, SCU, false);
        let server = server_lots(&[&old]);
        let base: BTreeMap<String, f64> = [("bt-iron|orbituary|500|0".to_string(), 2.0)].into_iter().collect();
        let result = plan(&local, &server, Some(&base), &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(result.push.len(), 501, "499 new lots, the reduction at the old place, the addition at the new");

        let sets = change_sets(&result, &local, &server, 500, &BTreeSet::new());
        assert_eq!(sets.iter().map(|s| s.1.len()).sum::<usize>(), 501, "nothing lost");
        assert!(sets.iter().all(|s| s.1.len() <= 500), "no set above the limit of the Basetool");
        let with_iron: Vec<&(Value, Vec<String>)> =
            sets.iter().filter(|s| s.1.iter().any(|k| k.starts_with("bt-iron|"))).collect();
        assert_eq!(with_iron.len(), 1, "both lots of the move go in one set: {:?}", sets.iter().map(|s| &s.1).collect::<Vec<_>>());
        assert_eq!(with_iron[0].1, ["bt-iron|levski|500|0", "bt-iron|orbituary|500|0"], "in the order they came");
        let amounts: Vec<Option<&Value>> =
            ops_of(with_iron[0]).iter().map(|op| op.get("quantity").and_then(|q| q.get("amount"))).collect();
        assert_eq!(amounts, [Some(&Value::Float(2.0)), Some(&Value::Float(0.0))], "the addition and the reduction");
        for (n, set) in sets.iter().enumerate() {
            assert_eq!(set.1.len(), ops_of(set).len(), "set {n}: one identifier for each instruction");
            // Each identifier belongs to the instruction at its place: it names that material, place and quality.
            for (op, key) in ops_of(set).into_iter().zip(&set.1) {
                let parts: Vec<&str> = key.split('|').collect();
                let place = op.get("location").and_then(|l| l.get("name")).and_then(Value::as_str).unwrap_or_default();
                assert_eq!(op.get("material").and_then(|m| m.get("bt")), Some(&Value::str(parts[0])), "set {n}: {key}");
                assert_eq!(place.to_lowercase(), parts[1], "set {n}: {key}");
                assert_eq!(op.get("quality"), Some(&Value::int(parts[2].parse().expect("quality"))), "set {n}: {key}");
            }
        }
    }

    #[test]
    fn sets_close_between_materials_and_only_a_material_above_the_limit_is_cut() {
        let key = |material: &str, quality: i64| format!("bt-{material}|orbituary|{quality}|0");
        let mut keys: Vec<String> = Vec::new();
        keys.extend((0..3).map(|q| key("a", q)));
        keys.extend((0..4).map(|q| key("b", q)));
        keys.extend((0..2).map(|q| key("c", q)));
        // Limit 5: a (3) | b (4) | c (2); b does not fit after a, c does not fit after b.
        assert_eq!(in_material_sets(&keys, 5), [vec![0, 1, 2], vec![3, 4, 5, 6], vec![7, 8]]);
        // Limit 7: a and b together, c after them.
        assert_eq!(in_material_sets(&keys, 7), [vec![0, 1, 2, 3, 4, 5, 6], vec![7, 8]]);
        // Limit 3: b alone is above the limit and is cut at it, a and c stay whole.
        assert_eq!(in_material_sets(&keys, 3), [vec![0, 1, 2], vec![3, 4, 5], vec![6], vec![7, 8]]);
        assert_eq!(in_material_sets(&keys, 100), [(0..9).collect::<Vec<_>>()]);
        assert!(in_material_sets(&[], 5).is_empty());
        // Materials that come apart in the plan are still one group; the first appearance sets the order.
        let apart = [key("a", 0), key("b", 0), key("a", 1)];
        assert_eq!(in_material_sets(&apart, 5), [vec![0, 2, 1]]);
        assert_eq!(material_of("bt-iron|levski|500|0"), "bt-iron");
    }

    fn keyed(lots: &[Value]) -> BTreeMap<String, Value> {
        lots.iter().map(|l| (l.get("key").and_then(Value::as_str).expect("key").to_string(), l.clone())).collect()
    }

    fn with_key(mut lot: Value, key: &str) -> Value {
        if let Value::Object(map) = &mut lot {
            map_set(map, "key", Value::str(key));
        }
        lot
    }

    #[test]
    fn a_lot_that_went_while_another_grew_by_its_amount_is_a_merge_and_anything_else_is_not() {
        let party = with_key(lot("bt-party", "Orbituary", 0, 10.0, SCU, false), "L1");
        let fire = with_key(lot("bt-fire", "Orbituary", 0, 5.0, SCU, false), "L2");
        let iron = with_key(lot("bt-iron", "Orbituary", 0, 1.0, SCU, false), "L3");
        let before = keyed(&[party.clone(), fire.clone(), iron.clone()]);
        // The Basetool folds party into fire: L1 goes, L2 holds both amounts.
        let grown = with_key(lot("bt-fire", "Orbituary", 0, 15.0, SCU, false), "L2");
        let after = keyed(&[grown, iron.clone()]);
        let found = merge_candidates(&before, &after);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found["bt-party"].bt, "bt-fire");
        // A new lot under a new key counts as well, where none of the new material stood before.
        let before_alone = keyed(&[party.clone(), iron.clone()]);
        let new_lot = with_key(lot("bt-fire", "Orbituary", 0, 10.0, SCU, false), "L9");
        assert_eq!(merge_candidates(&before_alone, &keyed(&[new_lot, iron.clone()]))["bt-party"].bt, "bt-fire");
        // Counter-checks: an amount that does not add up, another place, or the old material still somewhere.
        let wrong = with_key(lot("bt-fire", "Orbituary", 0, 14.0, SCU, false), "L2");
        assert!(merge_candidates(&before, &keyed(&[wrong, iron.clone()])).is_empty(), "amount");
        let elsewhere = with_key(lot("bt-fire", "Lorville", 0, 10.0, SCU, false), "L9");
        assert!(merge_candidates(&before_alone, &keyed(&[elsewhere, iron.clone()])).is_empty(), "place");
        let still = with_key(lot("bt-party", "Lorville", 0, 1.0, SCU, false), "L8");
        let grown = with_key(lot("bt-fire", "Orbituary", 0, 15.0, SCU, false), "L2");
        assert!(merge_candidates(&before, &keyed(&[grown, iron.clone(), still])).is_empty(), "old one still there");
        // A lot only taken away is no merge.
        assert!(merge_candidates(&before, &keyed(&[fire, iron])).is_empty(), "removed");
    }

    #[test]
    fn rows_of_a_folded_material_take_the_new_name_and_become_one_row_with_the_sum() {
        let renames: BTreeMap<String, String> = [("party favors".to_string(), "Fireworks".to_string())].into();
        let mut stolen = row("Party Favors", 1.0, 0);
        if let Value::Object(map) = &mut stolen {
            map_set(map, "gestohlen", Value::Bool(true));
        }
        let raw = [
            row("Fireworks", 5.0, 0),
            row("Iron", 2.0, 0),
            row("Party Favors", 10.0, 0),
            stolen,
            row("Party Favors", 3.0, 500),
        ];
        let (raw, trade, moved) = follow_merges(&raw, &[], &renames);
        assert_eq!(moved, 3);
        assert!(trade.is_empty());
        let names: Vec<(String, f64, i64)> = raw
            .iter()
            .map(|r| (text_field(r, "material"), float_or_zero(r.get("menge")), int_or_zero(r.get("qualitaet"))))
            .collect();
        assert_eq!(
            names,
            [
                ("Fireworks".to_string(), 15.0, 0),
                ("Iron".to_string(), 2.0, 0),
                ("Fireworks".to_string(), 1.0, 0),
                ("Fireworks".to_string(), 3.0, 500),
            ],
            "the same lot is one row, a stolen one and another quality stay rows of their own"
        );
        assert_eq!(raw[2].get("gestohlen"), Some(&Value::Bool(true)));
        // Rows alike that no merge touched stay as they are.
        let (same, _, none) = follow_merges(&[row("Iron", 1.0, 0), row("Iron", 2.0, 0)], &[], &renames);
        assert_eq!((same.len(), none), (2, 0));
    }

    #[test]
    fn the_stand_of_a_folded_material_adds_up_under_the_new_one() {
        let merged: BTreeMap<String, String> = [("bt-party".to_string(), "bt-fire".to_string())].into();
        let stand: BTreeMap<String, f64> =
            [("bt-party|orbituary|0|0".to_string(), 10.0), ("bt-fire|orbituary|0|0".to_string(), 5.0)].into();
        assert_eq!(rekey_amounts(&stand, &merged), [("bt-fire|orbituary|0|0".to_string(), 15.0)].into());
        let decisions: BTreeMap<String, String> = [
            ("bt-party|orbituary|0|0".to_string(), "mine".to_string()),
            ("bt-party|lorville|0|0".to_string(), "theirs".to_string()),
            ("bt-fire|orbituary|0|0".to_string(), "theirs".to_string()),
        ]
        .into();
        let moved = rekey_keys(&decisions, &merged);
        assert_eq!(moved.get("bt-fire|orbituary|0|0").map(String::as_str), Some("theirs"), "the new one stays");
        assert_eq!(moved.get("bt-fire|lorville|0|0").map(String::as_str), Some("theirs"));
        assert_eq!(moved.len(), 2);
    }

    #[test]
    fn a_retired_material_goes_out_only_as_a_reduction_or_as_a_move() {
        let retired: BTreeMap<String, String> = [("bt-boron".to_string(), "MATERIAL_RETIRED".to_string())].into();
        let key = |place: &str| format!("bt-boron|{place}|0|0");
        let plan_of = |push: Vec<(String, f64, f64)>| StockPlan { push, ..StockPlan::default() };
        // An addition stays back, a reduction goes, other materials go.
        let (out, held) = hold_retired(
            &plan_of(vec![(key("a"), 5.0, 3.0), ("bt-iron|a|0|0".into(), 2.0, 1.0)]),
            &retired,
        );
        assert_eq!(out.push, [("bt-iron|a|0|0".to_string(), 2.0, 1.0)]);
        assert_eq!(held.get(&key("a")).map(String::as_str), Some("MATERIAL_RETIRED"));
        let (out, held) = hold_retired(&plan_of(vec![(key("a"), 1.0, 3.0)]), &retired);
        assert_eq!((out.push.len(), held.len()), (1, 0));
        // A move: as much away at one place as added at the other, both go.
        let (out, held) = hold_retired(&plan_of(vec![(key("a"), 0.0, 3.0), (key("b"), 3.0, 0.0)]), &retired);
        assert_eq!((out.push.len(), held.len()), (2, 0));
        // More added than taken away: the reduction goes, the addition stays back.
        let (out, held) = hold_retired(&plan_of(vec![(key("a"), 0.0, 3.0), (key("b"), 5.0, 0.0)]), &retired);
        assert_eq!(out.push, [(key("a"), 0.0, 3.0)]);
        assert!(held.contains_key(&key("b")));
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
