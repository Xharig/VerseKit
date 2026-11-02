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

//! What the own unit still needs (`GET /me/org-demand`, right
//! `exchange.demand.read`; `exchange_demand.py` of 4.12): the open orders of
//! the units of the member as materials with a lowest quality and as items,
//! anonymous and read only.
//!
//! ⚠⚠ **In memory only.** The Basetool allows a copy for 7 days at most,
//! never in a backup, never in an error report, never passed on. The data
//! folder is backed up, so the demand lies nowhere on the disk; after a
//! restart it is fetched again with the next sync (at most every 5 minutes).
//! When the answer carries a `reason`, nothing is shown and the reason is
//! said.

use std::cmp::Reverse;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::basetool::connection::{Connection, get};
use crate::basetool::error::{ApiError, py_str, truthy};
use crate::basetool::feed::list;
use crate::basetool::orgstock::{float_of, int_of, text_of, whole};
use crate::basetool::scopes::SETTING_DEMAND;
use crate::basetool::sync::{Area, Pass};
use crate::jsonio::{Map, Value};
use crate::stock_data::{lower, norm_material};

pub const PATH: &str = "/me/org-demand";
/// Older data is thrown away.
pub const MAX_AGE: f64 = 7.0 * 24.0 * 3600.0;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A material the units still need.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// The ores it is made of, as far as the Basetool names them.
    pub raw: Vec<String>,
    pub min_quality: i64,
    pub amount: f64,
    pub unit: String,
    pub source: String,
}

/// An item the units still need.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub amount: f64,
    pub unit: String,
    /// The member has the blueprint of it.
    pub craftable: bool,
}

/// One line of an order.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderLine {
    pub name: String,
    pub amount: f64,
    pub unit: String,
    pub min_quality: i64,
    pub item: bool,
}

/// An order of the logistician unit of the member.
#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub id: String,
    pub number: Option<i64>,
    pub kind: String,
    pub status: String,
    pub priority: Option<i64>,
    pub unit: String,
    pub requesting: String,
    pub handle: String,
    pub comment: String,
    pub lines: Vec<OrderLine>,
}

/// The answer in a fixed form; what is unknown falls away.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Demand {
    pub materials: Vec<Material>,
    pub items: Vec<Item>,
    /// Only for a logistician with the right `exchange.demand.details`.
    pub orders: Option<Vec<Order>>,
    pub updated: String,
    pub reason: String,
}

/// `{'amount': 40, 'unit': 'SCU'}` → `(40.0, "SCU")`, else `(0.0, "")`.
fn amount_block(block: Option<&Value>) -> (f64, String) {
    let Some(map) = block.and_then(Value::as_object) else { return (0.0, String::new()) };
    (float_of(get(map, "amount")), text_value(get(map, "unit")))
}

/// `str(value or '')`.
fn text_value(value: Option<&Value>) -> String {
    match value {
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    }
}

/// The name of a block: `str(block.get('name') or '').strip()`, empty for anything but an object.
fn name_of(block: Option<&Value>) -> String {
    match block.and_then(Value::as_object) {
        Some(map) => text_of(get(map, "name"), usize::MAX),
        None => String::new(),
    }
}

fn parse_order(order: &Value) -> Option<Order> {
    let map = order.as_object()?;
    let id = text_of(get(map, "id"), usize::MAX);
    if id.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for line in list(map, "materials").iter().filter_map(Value::as_object) {
        let (amount, unit) = amount_block(get(line, "openQuantity"));
        let name = name_of(get(line, "material"));
        if name.is_empty() || amount <= 0.0 {
            continue;
        }
        lines.push(OrderLine { name, amount, unit, min_quality: int_of(get(line, "minQuality")), item: false });
    }
    for line in list(map, "items").iter().filter_map(Value::as_object) {
        let (amount, unit) = amount_block(get(line, "openQuantity"));
        let name = name_of(get(line, "item"));
        if name.is_empty() || amount <= 0.0 {
            continue;
        }
        let unit = if unit.is_empty() { "PIECE".to_string() } else { unit };
        lines.push(OrderLine { name, amount, unit, min_quality: 0, item: true });
    }
    Some(Order {
        id,
        number: whole(get(map, "number")),
        kind: text_value(get(map, "type")),
        status: text_value(get(map, "status")),
        priority: whole(get(map, "priority")),
        unit: name_of(get(map, "unit")),
        requesting: crate::streamer::private(name_of(get(map, "requestingUnit"))),
        handle: crate::streamer::private(text_of(get(map, "handle"), 200)),
        comment: text_of(get(map, "comment"), 2000),
        lines,
    })
}

/// The answer of the Basetool in a fixed form (`exchange_demand.parse`).
pub fn parse(answer: &Value) -> Demand {
    let empty = Map::new();
    let answer = answer.as_object().unwrap_or(&empty);
    let mut materials = Vec::new();
    for line in list(answer, "materials").iter().filter_map(Value::as_object) {
        let name = name_of(get(line, "material"));
        let (amount, unit) = amount_block(get(line, "openQuantity"));
        if name.is_empty() || amount <= 0.0 {
            continue;
        }
        let raw: Vec<String> = match get(line, "rawRefs") {
            Some(Value::Array(refs)) => {
                refs.iter().take(20).map(|r| name_of(Some(r))).filter(|r| !r.is_empty()).collect()
            }
            _ => Vec::new(),
        };
        materials.push(Material {
            name,
            raw,
            min_quality: int_of(get(line, "minQuality")),
            amount,
            unit,
            source: text_value(get(line, "source")),
        });
    }
    let mut items = Vec::new();
    for line in list(answer, "items").iter().filter_map(Value::as_object) {
        let name = name_of(get(line, "item"));
        let (amount, unit) = amount_block(get(line, "openQuantity"));
        if name.is_empty() || amount <= 0.0 {
            continue;
        }
        items.push(Item { name, amount, unit, craftable: get(line, "craftableByMe").is_some_and(truthy) });
    }
    materials.sort_by_cached_key(|m| (lower(&m.name), Reverse(m.min_quality)));
    items.sort_by_cached_key(|i| (!i.craftable, lower(&i.name)));
    let orders = match get(answer, "orders") {
        Some(Value::Array(orders)) => {
            let mut found: Vec<Order> = orders.iter().filter_map(parse_order).collect();
            found.sort_by_key(|o| (o.priority.is_none(), o.priority.unwrap_or(0), o.number.unwrap_or(0)));
            Some(found)
        }
        _ => None,
    };
    Demand {
        materials,
        items,
        orders,
        updated: text_value(get(answer, "updatedAt")),
        reason: text_value(get(answer, "reason")),
    }
}

#[derive(Default)]
struct Inner {
    data: Option<Arc<Demand>>,
    at: f64,
    reason: String,
    generation: u64,
}

/// The demand in memory; the sync fills it.
#[derive(Default)]
pub struct DemandStore {
    inner: Mutex<Inner>,
}

impl DemandStore {
    pub fn new() -> Arc<DemandStore> {
        Arc::new(DemandStore::default())
    }

    /// Fetches the demand and keeps it; returns the counts. A `reason`
    /// (`NOT_PERMITTED`) throws the state before away.
    pub fn refresh(&self, conn: &Connection, now: f64) -> Result<Map, ApiError> {
        let answer = conn.request("GET", PATH, None, &[], None)?;
        Ok(self.take(parse(&Value::Object(answer)), now))
    }

    /// Keeps a demand that was read; the counts for the sync.
    pub fn take(&self, demand: Demand, now: f64) -> Map {
        let counts = vec![
            ("materials".to_string(), Value::int(demand.materials.len() as i64)),
            ("items".to_string(), Value::int(demand.items.len() as i64)),
            ("reason".to_string(), Value::str(&demand.reason)),
        ];
        let mut inner = lock(&self.inner);
        inner.reason = demand.reason.clone();
        inner.data = if demand.reason.is_empty() { Some(Arc::new(demand)) } else { None };
        inner.at = now;
        inner.generation += 1;
        counts
    }

    /// The demand read last and its generation; `None` when nothing was
    /// read, a reason came or it is older than [`MAX_AGE`] (then it is
    /// thrown away).
    pub fn current(&self, now: f64) -> (u64, Option<Arc<Demand>>) {
        let mut inner = lock(&self.inner);
        if inner.data.is_some() && now - inner.at > MAX_AGE {
            inner.data = None;
            inner.at = 0.0;
            inner.generation += 1;
        }
        (inner.generation, inner.data.clone())
    }

    /// When the demand in memory was fetched (seconds since 1970); `None` without one.
    pub fn fetched_at(&self) -> Option<f64> {
        let inner = lock(&self.inner);
        inner.data.as_ref().map(|_| inner.at)
    }

    /// Why nothing is shown, empty when there is no reason.
    pub fn reason(&self) -> String {
        lock(&self.inner).reason.clone()
    }

    /// Throw everything away: switched off, disconnected, right missing.
    pub fn forget(&self) {
        let mut inner = lock(&self.inner);
        if inner.data.is_some() || !inner.reason.is_empty() {
            inner.generation += 1;
        }
        inner.data = None;
        inner.at = 0.0;
        inner.reason.clear();
    }

    /// The materials the demand asks for that fit one of `names`, as the
    /// material itself or as one of the ores it is made of. Empty without a
    /// demand.
    pub fn matching(&self, names: &[String], now: f64) -> Vec<Material> {
        let wanted: Vec<String> = names.iter().filter(|n| !n.is_empty()).map(|n| norm_material(n)).collect();
        let (_, data) = self.current(now);
        let Some(data) = data.filter(|_| !wanted.is_empty()) else { return Vec::new() };
        data.materials
            .iter()
            .filter(|line| {
                std::iter::once(norm_material(&line.name))
                    .chain(line.raw.iter().map(|r| norm_material(r)))
                    .any(|key| wanted.contains(&key))
            })
            .cloned()
            .collect()
    }
}

/// The area of the sync: runs while the demand is switched on and its right
/// granted. An error stops the pass, as in 4.12.
#[derive(Clone)]
pub struct DemandArea {
    pub store: Arc<DemandStore>,
}

impl Area for DemandArea {
    fn name(&self) -> &'static str {
        "demand"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_DEMAND)
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        self.store.refresh(pass.conn, pass.now)
    }

    fn forget(&mut self) {
        self.store.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn answer(text: &str) -> Value {
        loads(text).unwrap()
    }

    #[test]
    fn the_answer_is_brought_into_a_fixed_form_and_sorted() {
        let demand = parse(&answer(
            r#"{"materials": [
                {"material": {"name": "Titanium"}, "openQuantity": {"amount": 12.5, "unit": "SCU"}, "minQuality": 500,
                 "rawRefs": [{"name": "Titanium (Ore)"}, {"name": ""}, 5], "source": "JOB"},
                {"material": {"name": "agricium"}, "openQuantity": {"amount": 3, "unit": "SCU"}, "minQuality": "700"},
                {"material": {"name": "Agricium"}, "openQuantity": {"amount": 4, "unit": "SCU"}, "minQuality": 300},
                {"material": {"name": "Nothing"}, "openQuantity": {"amount": 0, "unit": "SCU"}},
                {"material": {"name": ""}, "openQuantity": {"amount": 9}},
                "no object"],
               "items": [
                {"item": {"name": "Helm"}, "openQuantity": {"amount": 2, "unit": "PIECE"}, "craftableByMe": false},
                {"item": {"name": "Zange"}, "openQuantity": {"amount": 1, "unit": "PIECE"}, "craftableByMe": true},
                {"item": {"name": "Ast"}, "openQuantity": {"amount": 1}, "craftableByMe": true}],
               "updatedAt": "2026-10-22T10:00:00Z"}"#,
        ));
        let names: Vec<(&str, i64)> = demand.materials.iter().map(|m| (m.name.as_str(), m.min_quality)).collect();
        assert_eq!(names, [("agricium", 700), ("Agricium", 300), ("Titanium", 500)]);
        assert_eq!(demand.materials[2].raw, ["Titanium (Ore)"]);
        assert_eq!(demand.materials[2].source, "JOB");
        let items: Vec<(&str, bool)> = demand.items.iter().map(|i| (i.name.as_str(), i.craftable)).collect();
        assert_eq!(items, [("Ast", true), ("Zange", true), ("Helm", false)], "what can be crafted first");
        assert_eq!(demand.items[0].unit, "", "a missing unit stays empty in the items");
        assert!(demand.orders.is_none(), "no orders without the right");
        assert_eq!(demand.updated, "2026-10-22T10:00:00Z");
    }

    #[test]
    fn orders_come_only_as_a_list_and_sort_by_priority_then_number() {
        let demand = parse(&answer(
            r#"{"orders": [
                {"id": "a", "number": 7, "status": "OPEN", "priority": 2, "unit": {"name": "Alpha"},
                 "requestingUnit": {"name": "Beta"}, "handle": " Name ", "comment": "  hallo  ",
                 "materials": [{"material": {"name": "Iron"}, "openQuantity": {"amount": 4, "unit": "SCU"}, "minQuality": 600}],
                 "items": [{"item": {"name": "Helm"}, "openQuantity": {"amount": 2}}]},
                {"id": "b", "number": 3, "status": "IN_PROGRESS", "priority": 2},
                {"id": "c", "status": "OPEN"},
                {"id": "  ", "number": 1},
                {"number": 2},
                "x"]}"#,
        ));
        let orders = demand.orders.expect("a list");
        let numbers: Vec<Option<i64>> = orders.iter().map(|o| o.number).collect();
        assert_eq!(numbers, [Some(3), Some(7), None]);
        let first = &orders[1];
        assert_eq!((first.handle.as_str(), first.comment.as_str(), first.unit.as_str()), ("Name", "hallo", "Alpha"));
        assert_eq!(first.requesting, "Beta");
        assert_eq!(first.lines.len(), 2);
        assert_eq!((first.lines[0].min_quality, first.lines[0].item), (600, false));
        assert_eq!((first.lines[1].unit.as_str(), first.lines[1].item), ("PIECE", true));
        assert!(parse(&answer(r#"{"orders": []}"#)).orders.is_some_and(|o| o.is_empty()));
        assert!(parse(&answer(r#"{"orders": "none"}"#)).orders.is_none());
    }

    #[test]
    fn something_that_is_no_answer_is_an_empty_demand() {
        assert_eq!(parse(&answer("[1, 2]")), Demand::default());
        assert_eq!(parse(&answer("null")), Demand::default());
        assert_eq!(parse(&answer(r#"{"reason": "NOT_PERMITTED"}"#)).reason, "NOT_PERMITTED");
    }

    #[test]
    fn a_reason_throws_the_state_away_and_old_data_is_forgotten() {
        let store = DemandStore::new();
        let demand =
            parse(&answer(r#"{"materials": [{"material": {"name": "Iron"}, "openQuantity": {"amount": 1}}]}"#));
        store.take(demand, 1000.0);
        assert!(store.current(1000.0).1.is_some());
        // Seven days written out, not the constant: a longer copy is against the rule of the Basetool.
        assert_eq!(MAX_AGE, 604_800.0);
        assert!(store.current(1000.0 + 604_800.0).1.is_some(), "on the seventh day it is still there");
        assert!(store.current(1000.0 + 604_800.0 + 1.0).1.is_none(), "after 7 days it is gone");
        store.take(
            parse(&answer(r#"{"materials": [{"material": {"name": "Iron"}, "openQuantity": {"amount": 1}}]}"#)),
            2000.0,
        );
        store.take(parse(&answer(r#"{"reason": "NOT_PERMITTED"}"#)), 2001.0);
        assert!(store.current(2001.0).1.is_none());
        assert_eq!(store.reason(), "NOT_PERMITTED");
        store.forget();
        assert_eq!(store.reason(), "");
    }

    #[test]
    fn the_ores_a_material_is_made_of_match_too() {
        let store = DemandStore::new();
        store.take(
            parse(&answer(
                r#"{"materials": [
                    {"material": {"name": "Titanium"}, "openQuantity": {"amount": 2}, "rawRefs": [{"name": "Titanium (Ore)"}]},
                    {"material": {"name": "Gold"}, "openQuantity": {"amount": 1}},
                    {"material": {"name": "Alloy X"}, "openQuantity": {"amount": 3}, "rawRefs": [{"name": "Quartz (Ore)"}]}]}"#,
            )),
            10.0,
        );
        // Only through the ore it is made of: the name itself says nothing of Quartz.
        let by_ore = store.matching(&["quartz".to_string()], 10.0);
        assert_eq!(by_ore.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Alloy X"]);
        let found = store.matching(&["titanium (raw)".to_string()], 10.0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Titanium");
        assert!(store.matching(&[], 10.0).is_empty());
        assert!(store.matching(&["Corundum".to_string()], 10.0).is_empty());
    }
}
