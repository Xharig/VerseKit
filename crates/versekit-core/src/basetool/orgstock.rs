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

//! The storage of the own units (`GET /org-stock`, right
//! `exchange.orgstock.read`; `exchange_orgstock.py`).
//!
//! One **pool** per unit the member belongs to directly; in it one **lot**
//! per goods, location, quality and stolen mark, summed over all members.
//! With `exchange.orgstock.details` every lot of a pool in which the member
//! is a logistician carries its **entries**, one per owner with note and
//! reservations. With `exchange.orgstock.write` a logistician books in that
//! pool: one change set to `POST /org-stock/changes`; what it changed comes
//! back through the feed.
//!
//! ⚠⚠ **In memory only**: it is the stock of other members. The Basetool
//! allows at most 7 days and never in a backup, and the data folder is
//! backed up, so nothing of it lies on disk. After a restart the snapshot is
//! fetched again, then only the feed.
//!
//! ⚠ Ids are opaque: `key`, `id`, `ref`, `bt` are stored and sent back,
//! never taken apart. An owner is known by `ref`, not by name.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::basetool::connection::{Connection, get, get_str, py_int};
use crate::basetool::error::{ApiError, py_str, truthy};
use crate::basetool::feed::{self, Sent, list};
use crate::basetool::scopes::{
    SCOPES_ORGSTOCK_DETAILS, SCOPES_ORGSTOCK_WRITE, SETTING_ORGSTOCK, SETTING_ORGSTOCK_WRITE,
};
use crate::basetool::sync::{Area, Pass};
use crate::jsonio::{self, Map, Value};
use crate::stock::{py_round, py_round_int};
use crate::stock_data::{lower, py_float};
use crate::text::py_strip;

pub const PATH: &str = "/org-stock";
pub const CHANGES: &str = "/org-stock/changes";
pub const MEMBERS: &str = "/org-stock/members";
pub const TARGETS: &str = "/org-stock/targets";
pub const RESOLVE: &str = "/catalog/resolve";
/// Older data is thrown away.
pub const MAX_AGE: f64 = 7.0 * 24.0 * 3600.0;

pub const SCU: &str = "SCU";
pub const PIECE: &str = "PIECE";
pub const ITEM: &str = "ITEM";
pub const MATERIAL: &str = "MATERIAL";

/// Refusals per operation the page puts into words; any other shows its code.
pub const REASONS: [&str; 13] = [
    "NOT_PERMITTED",
    "HOLDER_UNKNOWN",
    "VERSION_CONFLICT",
    "INSUFFICIENT_STOCK",
    "STOCK_EARMARKED",
    "STOCK_OFFERED",
    "OVER_ALLOCATION",
    "EARMARK_TARGET_INVALID",
    "UNIT_MISMATCH",
    "LOCATION_UNKNOWN",
    "STOLEN_MARKING_DISABLED",
    "UNMATCHED",
    "AMBIGUOUS",
];

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ------------------------------------------------------------ reading

/// `str(value or '').strip()[:limit]`.
pub fn text_of(value: Option<&Value>, limit: usize) -> String {
    match value {
        Some(v) if truthy(v) => py_strip(&py_str(v)).chars().take(limit).collect(),
        _ => String::new(),
    }
}

/// `int(text)` of Python: spaces around, a sign, `_` between digits.
fn py_int_text(text: &str) -> Option<i64> {
    let t = py_strip(text);
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    if !clean.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(sign * clean.parse::<i64>().unwrap_or(i64::MAX))
}

/// `float(value or 0)`; anything Python cannot read is 0.
pub(crate) fn float_of(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Float(x)) => *x,
        Some(Value::Bool(true)) => 1.0,
        Some(Value::Str(s)) if !s.is_empty() => py_float(s).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `int(value or 0)`; anything Python cannot read is 0.
pub(crate) fn int_of(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(if n.starts_with('-') { i64::MIN } else { i64::MAX }),
        Some(Value::Float(x)) if x.is_finite() => x.trunc() as i64,
        Some(Value::Bool(true)) => 1,
        Some(Value::Str(s)) if !s.is_empty() => py_int_text(s).unwrap_or(0),
        _ => 0,
    }
}

/// `value if isinstance(value, int) else None` (a bool is an int there).
pub(crate) fn whole(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int(n)) => Some(n.parse().unwrap_or(if n.starts_with('-') { i64::MIN } else { i64::MAX })),
        Some(Value::Bool(b)) => Some(i64::from(*b)),
        _ => None,
    }
}

fn object(value: Option<&Value>) -> Option<&Map> {
    value.and_then(Value::as_object)
}

/// `{'amount': 40, 'unit': 'SCU'}` → `(40.0, "SCU")`.
pub fn amount_of(block: Option<&Value>) -> (f64, String) {
    let Some(map) = object(block) else { return (0.0, SCU.into()) };
    let value = float_of(get(map, "amount"));
    let unit = text_of(get(map, "unit"), 255);
    (value, if unit.is_empty() { SCU.into() } else { unit })
}

/// One reservation of an entry for an order.
#[derive(Clone, Debug, PartialEq)]
pub struct JobLine {
    pub id: String,
    pub number: Option<i64>,
    pub amount: f64,
    pub delivered: bool,
}

/// One reservation of an entry for a mission.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionLine {
    pub id: String,
    pub name: String,
    pub amount: f64,
}

/// One owner's entry of a lot.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub version: i64,
    pub withheld: bool,
    /// Empty when the owner hides the name.
    pub holder: String,
    pub holder_ref: String,
    pub amount: f64,
    pub unit: String,
    pub note: String,
    pub jobs: Vec<JobLine>,
    pub missions: Vec<MissionLine>,
    pub free_jobs: f64,
    pub free_missions: f64,
}

impl Entry {
    /// What is not reserved in either dimension.
    pub fn free(&self) -> f64 {
        self.free_jobs.min(self.free_missions)
    }
}

/// An entry in fixed form, or `None` without an `id`.
pub fn parse_entry(entry: &Value) -> Option<Entry> {
    let map = entry.as_object()?;
    let id = text_of(get(map, "id"), 128);
    if id.is_empty() {
        return None;
    }
    let holder = object(get(map, "holder"));
    let withheld = holder.and_then(|h| get(h, "withheld")).is_some_and(truthy);
    let (amount, unit) = amount_of(get(map, "quantity"));
    let lines = |key: &str| -> Vec<&Map> {
        match get(map, key) {
            Some(v) if truthy(v) => match v {
                Value::Array(items) => items.iter().filter_map(Value::as_object).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    };
    let jobs = lines("jobOrders")
        .into_iter()
        .filter(|job| !text_of(get(job, "id"), 128).is_empty())
        .map(|job| JobLine {
            id: text_of(get(job, "id"), 128),
            number: whole(get(job, "number")),
            amount: amount_of(get(job, "quantity")).0,
            delivered: get(job, "delivered").is_some_and(truthy),
        })
        .collect();
    let missions = lines("missions")
        .into_iter()
        .filter(|m| !text_of(get(m, "id"), 128).is_empty())
        .map(|m| MissionLine {
            id: text_of(get(m, "id"), 128),
            name: text_of(get(m, "name"), 255),
            amount: amount_of(get(m, "quantity")).0,
        })
        .collect();
    let free = object(get(map, "free"));
    let free_of = |key: &str| match free.and_then(|f| get(f, key)) {
        None | Some(Value::Null) => amount,
        some => amount_of(some).0,
    };
    Some(Entry {
        id,
        version: whole(get(map, "version")).unwrap_or(0),
        withheld,
        holder: if withheld {
            String::new()
        } else {
            crate::streamer::private(text_of(holder.and_then(|h| get(h, "name")), 200))
        },
        holder_ref: if withheld { String::new() } else { text_of(holder.and_then(|h| get(h, "ref")), 128) },
        amount,
        unit,
        note: if withheld { String::new() } else { text_of(get(map, "note"), 2000) },
        jobs,
        missions,
        free_jobs: free_of("jobOrders"),
        free_missions: free_of("missions"),
    })
}

/// Ship component data of an item, as stable codes (`component`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component {
    /// `SHIELD`, `WEAPON_GUN` …; empty when missing.
    pub kind: String,
    pub size: Option<i64>,
    pub grade: String,
    /// `MILITARY`, `CIVILIAN`, `INDUSTRIAL`, `STEALTH`, `COMPETITION`.
    pub class: String,
}

/// The component of a lot; `None` without one.
pub fn parse_component(value: Option<&Value>) -> Option<Component> {
    let map = object(value)?;
    let component = Component {
        kind: text_of(get(map, "componentType"), 64),
        size: whole(get(map, "size")).filter(|s| *s >= 0),
        grade: text_of(get(map, "grade"), 32),
        class: text_of(get(map, "itemClass"), 64),
    };
    (component != Component::default()).then_some(component)
}

/// One lot of a pool.
#[derive(Clone, Debug, PartialEq)]
pub struct Lot {
    pub key: String,
    pub unit_id: String,
    pub unit_name: String,
    /// `{bt?, name?}` as sent back in a booking.
    pub material_bt: String,
    pub material_name: String,
    /// The shown name: the goods name, else the Basetool id.
    pub name: String,
    /// `materialKind` of the Basetool; `None` for an item.
    pub kind: Option<Value>,
    pub component: Option<Component>,
    /// `{name?, uex?}` of the location.
    pub location: Map,
    pub place: String,
    pub quality: i64,
    pub stolen: bool,
    pub amount: f64,
    /// `SCU` or `PIECE`.
    pub unit_q: String,
    /// Reserved for orders and for missions.
    pub job: f64,
    pub mission: f64,
    /// `None` when the lot carries none (no logistician, no `details`).
    pub entries: Option<Vec<Entry>>,
}

impl Lot {
    /// An item carries no `materialKind`; only materials have one.
    pub fn is_item(&self) -> bool {
        self.kind.is_none()
    }

    pub fn piece(&self) -> bool {
        self.unit_q == PIECE
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.as_ref()?.iter().find(|e| e.id == id)
    }
}

/// A lot in fixed form, or `None` without key, unit or goods.
pub fn parse_lot(item: &Value) -> Option<Lot> {
    let map = item.as_object()?;
    let key = text_of(get(map, "key"), 128);
    let unit = object(get(map, "unit"));
    let unit_id = text_of(unit.and_then(|u| get(u, "id")), 128);
    let unit_name = text_of(unit.and_then(|u| get(u, "name")), 255);
    let material = object(get(map, "material"));
    let name = text_of(material.and_then(|m| get(m, "name")), 255);
    let bt = text_of(material.and_then(|m| get(m, "bt")), 128);
    if key.is_empty()
        || unit_id.is_empty()
        || !(!name.is_empty() || material.and_then(|m| get(m, "bt")).is_some_and(truthy))
    {
        return None;
    }
    let location = object(get(map, "location"));
    let kept: Map = location
        .map(|l| l.iter().filter(|(k, v)| (k == "name" || k == "uex") && truthy(v)).cloned().collect())
        .unwrap_or_default();
    let (amount, unit_q) = amount_of(get(map, "quantity"));
    let earmarked = object(get(map, "earmarked"));
    let entries = match get(map, "entries") {
        Some(Value::Array(items)) => Some(items.iter().filter_map(parse_entry).collect()),
        _ => None,
    };
    Some(Lot {
        key,
        unit_id,
        unit_name,
        name: if name.is_empty() { bt.clone() } else { name.clone() },
        material_bt: bt,
        material_name: name,
        kind: get(map, "materialKind").filter(|k| matches!(k, Value::Object(_))).cloned(),
        component: parse_component(get(map, "component")),
        location: kept,
        place: text_of(location.and_then(|l| get(l, "name")), 255),
        quality: int_of(get(map, "quality")).clamp(0, 1000),
        stolen: get(map, "stolen").is_some_and(truthy),
        amount,
        unit_q,
        job: amount_of(earmarked.and_then(|e| get(e, "jobOrders"))).0,
        mission: amount_of(earmarked.and_then(|e| get(e, "missions"))).0,
        entries,
    })
}

/// A unit of a page; `logistician` as the unit carries it, `None` without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: String,
    pub name: String,
    pub shorthand: String,
    pub kind: String,
    pub logistician: Option<bool>,
}

impl Unit {
    /// How a unit is named in a choice: `Staffel Alpha (ALP)`.
    pub fn label(&self) -> String {
        let name =
            [&self.name, &self.shorthand, &self.id].into_iter().find(|s| !s.is_empty()).cloned().unwrap_or_default();
        if !self.shorthand.is_empty() && self.shorthand != name { format!("{name} ({})", self.shorthand) } else { name }
    }
}

pub fn parse_units(units: &[Value]) -> Vec<Unit> {
    units
        .iter()
        .filter_map(Value::as_object)
        .filter(|u| !text_of(get(u, "id"), 128).is_empty())
        .map(|u| Unit {
            id: text_of(get(u, "id"), 128),
            name: crate::streamer::private(text_of(get(u, "name"), 255)),
            shorthand: crate::streamer::private(text_of(get(u, "shorthand"), 255)),
            kind: text_of(get(u, "kind"), 255),
            logistician: match get(u, "logistician") {
                Some(Value::Bool(b)) => Some(*b),
                _ => None,
            },
        })
        .collect()
}

/// Someone a logistician can book as owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub holder_ref: String,
    pub name: String,
}

/// Everything the page shows; one snapshot, never changed afterwards.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrgStockData {
    /// In the order the Basetool sent them.
    pub lots: Vec<Lot>,
    pub units: Vec<Unit>,
    /// Seconds since 1970 of the fetch.
    pub at: f64,
    pub details: bool,
    pub write: bool,
    pub logistician: BTreeSet<String>,
    pub rosters: BTreeMap<String, Vec<Member>>,
}

impl OrgStockData {
    pub fn lot(&self, key: &str) -> Option<&Lot> {
        self.lots.iter().find(|l| l.key == key)
    }

    /// The lot holding entry `id`, and the entry.
    pub fn entry(&self, id: &str) -> Option<(&Lot, &Entry)> {
        self.lots.iter().find_map(|lot| lot.entry(id).map(|e| (lot, e)))
    }

    /// Whether booking is allowed here: right `write` and logistician of the unit.
    pub fn can_write(&self, unit_id: &str) -> bool {
        self.write && self.logistician.contains(unit_id)
    }

    /// The lots of a unit (all with an empty id) whose name or place holds
    /// `search`, by name, place, quality downwards, not stolen first.
    pub fn lots_of(&self, unit_id: &str, search: &str) -> Vec<&Lot> {
        let needle = lower(py_strip(search));
        let mut out: Vec<&Lot> = self
            .lots
            .iter()
            .filter(|lot| unit_id.is_empty() || lot.unit_id == unit_id)
            .filter(|lot| {
                needle.is_empty() || lower(&lot.name).contains(&needle) || lower(&lot.place).contains(&needle)
            })
            .collect();
        out.sort_by(|a, b| {
            (lower(&a.name), lower(&a.place), -a.quality, a.stolen).cmp(&(
                lower(&b.name),
                lower(&b.place),
                -b.quality,
                b.stolen,
            ))
        });
        out
    }

    /// `{bt, name}` of goods that lie in a pool already and are counted in
    /// `unit` there; otherwise `None` (`_known_material`).
    pub fn known_material(&self, name: &str, unit: &str) -> Option<Found> {
        let wanted = lower(py_strip(name));
        self.lots
            .iter()
            .find(|lot| lower(&lot.name) == wanted && lot.unit_q == unit && !lot.material_bt.is_empty())
            .map(|lot| Found {
                bt: lot.material_bt.clone(),
                name: if lot.material_name.is_empty() { lot.name.clone() } else { lot.material_name.clone() },
                kind: String::new(),
            })
    }

    /// The units in which the member may book.
    pub fn writable_units(&self) -> Vec<&Unit> {
        self.units.iter().filter(|u| self.logistician.contains(&u.id)).collect()
    }
}

/// Lays pages on a state (`apply_pages`). The units of the last page that
/// names any count; lots of a unit no longer named drop out; without
/// `details` the entries drop out.
pub fn apply_pages(lots: Vec<Lot>, pages: &[Map], details: bool) -> (Vec<Lot>, Vec<Unit>) {
    let mut lots = lots;
    let mut units = None;
    for page in pages {
        for stone in list(page, "removed") {
            if let Value::Object(map) = stone {
                let key = text_of(get(map, "key"), 128);
                lots.retain(|l| l.key != key);
            }
        }
        for item in list(page, "items") {
            if let Some(lot) = parse_lot(item) {
                match lots.iter_mut().find(|l| l.key == lot.key) {
                    Some(slot) => *slot = lot,
                    None => lots.push(lot),
                }
            }
        }
        if let Some(Value::Array(found)) = get(page, "units") {
            units = Some(parse_units(found));
        }
    }
    let units = units.unwrap_or_default();
    lots.retain(|l| units.iter().any(|u| u.id == l.unit_id));
    if !details {
        for lot in &mut lots {
            lot.entries = None;
        }
    }
    (lots, units)
}

/// Who can be booked as owner in this unit's pool. `NOT_PERMITTED` for a
/// unit in which the member is no logistician.
pub fn members(conn: &Connection, unit_id: &str) -> Result<Vec<Member>, ApiError> {
    let answer = conn.request("GET", MEMBERS, None, &[("unit", unit_id)], None)?;
    Ok(parse_members(&answer))
}

pub fn parse_members(answer: &Map) -> Vec<Member> {
    list(answer, "items")
        .iter()
        .filter_map(Value::as_object)
        .filter(|m| !text_of(get(m, "ref"), 128).is_empty())
        .map(|m| Member {
            holder_ref: text_of(get(m, "ref"), 128),
            name: crate::streamer::private(text_of(get(m, "name"), 200)),
        })
        .collect()
}

/// An order or mission an entry can be reserved for.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: String,
    pub number: Option<i64>,
    pub name: String,
    pub status: String,
}

/// Orders and missions for reservations in this pool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub jobs: Vec<Target>,
    pub missions: Vec<Target>,
}

pub fn targets(conn: &Connection, unit_id: &str, material_bt: &str) -> Result<Targets, ApiError> {
    let answer = conn.request("GET", TARGETS, None, &[("unit", unit_id), ("material", material_bt)], None)?;
    Ok(parse_targets(&answer))
}

pub fn parse_targets(answer: &Map) -> Targets {
    let read = |key: &str, numbered: bool| -> Vec<Target> {
        list(answer, key)
            .iter()
            .filter_map(Value::as_object)
            .filter(|t| !text_of(get(t, "id"), 128).is_empty())
            .map(|t| Target {
                id: text_of(get(t, "id"), 128),
                number: if numbered { whole(get(t, "number")) } else { None },
                name: if numbered { String::new() } else { text_of(get(t, "name"), 255) },
                status: text_of(get(t, "status"), 255),
            })
            .collect()
    };
    Targets { jobs: read("jobOrders", true), missions: read("missions", false) }
}

/// A hit of `catalog/resolve`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub bt: String,
    pub name: String,
    /// `ITEM` or `MATERIAL`, the kind asked for; empty when not known.
    pub kind: String,
}

/// What looking up goods gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    One(Found),
    /// At most ten.
    Several(Vec<Found>),
    Unmatched,
}

fn catalog_ref(raw: Option<&Value>, kind: &str) -> Option<Found> {
    let map = object(raw)?;
    let bt = text_of(get(map, "bt"), 128);
    if bt.is_empty() {
        return None;
    }
    let name = text_of(get(map, "name"), 200);
    Some(Found { name: if name.is_empty() { bt.clone() } else { name }, bt, kind: kind.to_string() })
}

/// The answer of `catalog/resolve` for the first reference.
pub fn read_resolved(answer: &Map, kind: &str) -> Resolved {
    for result in list(answer, "results") {
        let Some(map) = result.as_object() else { continue };
        if !jsonio::py_eq(get(map, "index"), Some(&Value::int(0))) {
            continue;
        }
        match get_str(map, "status") {
            Some("resolved") => {
                if let Some(found) = catalog_ref(get(map, "ref"), kind) {
                    return Resolved::One(found);
                }
            }
            Some("ambiguous") => {
                let candidates: Vec<Found> = match get(map, "candidates") {
                    Some(Value::Array(items)) => items.iter().filter_map(|c| catalog_ref(Some(c), kind)).collect(),
                    _ => Vec::new(),
                };
                if !candidates.is_empty() {
                    return Resolved::Several(candidates.into_iter().take(10).collect());
                }
            }
            _ => {}
        }
    }
    Resolved::Unmatched
}

/// One reference through `POST /catalog/resolve`.
pub fn resolve(conn: &Connection, kind: &str, reference: &Value) -> Result<Resolved, ApiError> {
    let body =
        Value::Object(vec![("kind".into(), Value::str(kind)), ("refs".into(), Value::Array(vec![reference.clone()]))]);
    Ok(read_resolved(&conn.request("POST", RESOLVE, Some(&body), &[], None)?, kind))
}

/// Looks up the goods of a booking (`resolve_material`).
///
/// `known`: kind and reference from the goods list; then first through it.
/// Otherwise, and when it finds nothing, by name: in pieces first as ITEM,
/// then as MATERIAL, in SCU the other way round. A unique first kind wins.
/// An ambiguous one stays ambiguous even when the second kind is unique;
/// its hit then joins the candidates and stays in even at ten.
pub fn resolve_material(
    conn: &Connection,
    name: &str,
    known: Option<(&str, &Value)>,
    piece: bool,
) -> Result<Resolved, ApiError> {
    resolve_with(name, known, piece, |kind, reference| resolve(conn, kind, reference))
}

/// `resolve_material` with the lookup handed in.
pub fn resolve_with(
    name: &str,
    known: Option<(&str, &Value)>,
    piece: bool,
    mut ask: impl FnMut(&str, &Value) -> Result<Resolved, ApiError>,
) -> Result<Resolved, ApiError> {
    if let Some((kind, reference)) = known {
        let found = ask(kind, reference)?;
        if found != Resolved::Unmatched {
            return Ok(found);
        }
    }
    let reference =
        Value::Object(vec![("name".into(), Value::str(py_strip(name).chars().take(200).collect::<String>()))]);
    let order = if piece { [ITEM, MATERIAL] } else { [MATERIAL, ITEM] };
    let (mut candidates, mut sure): (Vec<Found>, Vec<Found>) = (Vec::new(), Vec::new());
    for kind in order {
        match ask(kind, &reference)? {
            Resolved::One(found) => {
                if candidates.is_empty() {
                    return Ok(Resolved::One(found));
                }
                sure.push(found);
            }
            Resolved::Several(found) => candidates.extend(found),
            Resolved::Unmatched => {}
        }
    }
    if candidates.is_empty() && sure.is_empty() {
        return Ok(Resolved::Unmatched);
    }
    let mut out: Vec<Found> = Vec::new();
    for candidate in sure.iter().chain(candidates.iter()) {
        if !out.iter().any(|c| c.bt == candidate.bt) {
            out.push(candidate.clone());
        }
    }
    let keep: Vec<Found> = out.iter().filter(|c| sure.contains(c)).cloned().collect();
    let rest: Vec<Found> = out.iter().filter(|c| !sure.contains(c)).cloned().collect();
    let mut chosen: Vec<Found> = rest.into_iter().take(10usize.saturating_sub(keep.len())).collect();
    chosen.extend(keep);
    Ok(Resolved::Several(chosen))
}

// ------------------------------------------------------------ the store

#[derive(Default)]
struct Inner {
    data: Option<Arc<OrgStockData>>,
    cursor: Option<String>,
    generation: u64,
    /// Grows with every `forget`: a fetch that ran meanwhile keeps nothing.
    forgotten: u64,
}

/// The unit storage in memory; the sync and the page's bookings fetch into it.
#[derive(Default)]
pub struct OrgStock {
    inner: Mutex<Inner>,
    /// One fetch at a time: else two would read the same feed and overwrite
    /// each other's cursor.
    fetching: Mutex<()>,
}

impl OrgStock {
    pub fn new() -> Arc<OrgStock> {
        Arc::new(OrgStock::default())
    }

    /// The snapshot first, then the feed (`refresh`). When the right
    /// `details` comes or goes, everything is fetched anew. Logistician is
    /// the member in units with `logistician: true` or with entries; with
    /// `write` the member list of every unit without `logistician: false` is
    /// asked, and `NOT_PERMITTED` there means no logistician. Returns the counts.
    pub fn refresh(&self, conn: &Connection, details: bool, write: bool, now: f64) -> Result<Map, ApiError> {
        let _fetching = lock(&self.fetching);
        let (cursor, lots, forgotten) = {
            let inner = lock(&self.inner);
            match &inner.data {
                Some(data) if data.details == details && data.at != 0.0 => {
                    (inner.cursor.clone(), data.lots.clone(), inner.forgotten)
                }
                _ => (None, Vec::new(), inner.forgotten),
            }
        };
        let (fetched, expired) = feed::fetch_resuming(conn, PATH, cursor.as_deref())?;
        let lots = if expired { Vec::new() } else { lots };
        let (lots, units) = apply_pages(lots, &fetched.pages, details);
        let mut logistician: BTreeSet<String> =
            lots.iter().filter(|l| l.entries.is_some()).map(|l| l.unit_id.clone()).collect();
        for unit in &units {
            match unit.logistician {
                Some(true) => {
                    logistician.insert(unit.id.clone());
                }
                Some(false) => {
                    logistician.remove(&unit.id);
                }
                None => {}
            }
        }
        let mut rosters = BTreeMap::new();
        if write {
            for unit in units.iter().filter(|u| u.logistician != Some(false)) {
                match members(conn, &unit.id) {
                    Ok(roster) => {
                        rosters.insert(unit.id.clone(), roster);
                        logistician.insert(unit.id.clone());
                    }
                    Err(error) if error.code == "NOT_PERMITTED" => {
                        logistician.remove(&unit.id);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        let counts = vec![
            ("units".into(), Value::int(units.len() as i64)),
            ("lots".into(), Value::int(lots.len() as i64)),
            ("logistician".into(), Value::int(logistician.len() as i64)),
        ];
        let data = OrgStockData { lots, units, at: now, details, write, logistician, rosters };
        let mut inner = lock(&self.inner);
        if inner.forgotten != forgotten {
            // Disconnected or switched off while fetching: nothing is kept.
            return Ok(counts);
        }
        inner.data = Some(Arc::new(data));
        inner.cursor = fetched.cursor;
        inner.generation += 1;
        Ok(counts)
    }

    /// Generation and data; `None` when nothing was fetched or it is older
    /// than [`MAX_AGE`] (then it is thrown away).
    pub fn current(&self, now: f64) -> (u64, Option<Arc<OrgStockData>>) {
        let mut inner = lock(&self.inner);
        if inner.data.as_ref().is_some_and(|d| now - d.at > MAX_AGE) {
            inner.data = None;
            inner.cursor = None;
            inner.generation += 1;
        }
        (inner.generation, inner.data.clone())
    }

    pub fn generation(&self) -> u64 {
        lock(&self.inner).generation
    }

    /// Throw everything away: switched off, disconnected, right missing.
    pub fn forget(&self) {
        let mut inner = lock(&self.inner);
        inner.forgotten += 1;
        if inner.data.is_some() || inner.cursor.is_some() {
            inner.data = None;
            inner.cursor = None;
            inner.generation += 1;
        }
    }
}

/// The area of the sync: runs while the unit storage is switched on and its
/// right granted. Its errors stop no other area; the page then shows the
/// last state.
#[derive(Clone)]
pub struct OrgStockArea {
    pub store: Arc<OrgStock>,
}

impl Area for OrgStockArea {
    fn name(&self) -> &'static str {
        "orgstock"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_ORGSTOCK)
    }

    fn isolated(&self) -> bool {
        true
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        let details = pass.granted(SCOPES_ORGSTOCK_DETAILS);
        let write = pass.paths.setting_bool(SETTING_ORGSTOCK_WRITE, false) && pass.granted(SCOPES_ORGSTOCK_WRITE);
        self.store.refresh(pass.conn, details, write, pass.now)
    }

    fn forget(&mut self) {
        self.store.forget();
    }
}

// ------------------------------------------------------------ writing

/// `{amount, unit}`: SCU to three places, pieces whole.
pub fn quantity(amount: f64, unit: &str) -> Value {
    if unit == PIECE {
        return Value::Object(vec![
            ("amount".into(), Value::int(py_round_int(amount))),
            ("unit".into(), Value::str(PIECE)),
        ]);
    }
    Value::Object(vec![
        ("amount".into(), Value::Float(py_round(amount, 3))),
        ("unit".into(), Value::str(if unit.is_empty() { SCU } else { unit })),
    ])
}

/// Amounts taken from reservations: id → amount; amounts of 0 drop out.
pub type Earmarks = BTreeMap<String, f64>;

fn earmark_lines(amounts: &Earmarks, key: &str, unit: &str) -> Vec<Value> {
    amounts
        .iter()
        .filter(|(_, a)| **a > 0.0)
        .map(|(id, a)| Value::Object(vec![(key.into(), Value::str(id)), ("quantity".into(), quantity(*a, unit))]))
        .collect()
}

fn set(op: &mut Map, key: &str, value: Value) {
    jsonio::map_set(op, key, value);
}

/// A new entry in the pool, held by `holder_ref` (from the member list).
/// `material` and `location` are `{bt?, name}` and `{name, uex?}`.
#[allow(clippy::too_many_arguments)]
pub fn op_book_in(
    unit_id: &str,
    holder_ref: &str,
    material: Value,
    location: Value,
    quality: i64,
    stolen: bool,
    amount: f64,
    unit: &str,
    note: &str,
    merge_scu: bool,
) -> Value {
    let mut op: Map = vec![
        ("op".into(), Value::str("book-in")),
        ("unit".into(), Value::str(unit_id)),
        ("holder".into(), Value::str(holder_ref)),
        ("material".into(), material),
        ("location".into(), location),
        ("quality".into(), Value::int(if unit == PIECE { 0 } else { quality })),
        ("stolen".into(), Value::Bool(stolen)),
        ("quantity".into(), quantity(amount, unit)),
    ];
    if !py_strip(note).is_empty() {
        set(&mut op, "note", Value::str(py_strip(note)));
    }
    if merge_scu && unit != PIECE {
        set(&mut op, "mergeScu", Value::Bool(true));
    }
    Value::Object(op)
}

fn taking(
    name: &str,
    unit_id: &str,
    entry: &Entry,
    amount: f64,
    unit: &str,
    jobs: &Earmarks,
    missions: &Earmarks,
) -> Map {
    let mut op: Map = vec![
        ("op".into(), Value::str(name)),
        ("unit".into(), Value::str(unit_id)),
        ("entry".into(), Value::str(&entry.id)),
        ("version".into(), Value::int(entry.version)),
        ("quantity".into(), quantity(amount, unit)),
    ];
    let from_jobs = earmark_lines(jobs, "jobOrder", unit);
    let from_missions = earmark_lines(missions, "mission", unit);
    if !from_jobs.is_empty() {
        set(&mut op, "fromJobOrders", Value::Array(from_jobs));
    }
    if !from_missions.is_empty() {
        set(&mut op, "fromMissions", Value::Array(from_missions));
    }
    op
}

pub fn op_book_out(
    unit_id: &str,
    entry: &Entry,
    amount: f64,
    unit: &str,
    jobs: &Earmarks,
    missions: &Earmarks,
) -> Value {
    Value::Object(taking("book-out", unit_id, entry, amount, unit, jobs, missions))
}

#[allow(clippy::too_many_arguments)]
pub fn op_sell(
    unit_id: &str,
    entry: &Entry,
    amount: f64,
    unit: &str,
    terminal: &str,
    proceeds: f64,
    jobs: &Earmarks,
    missions: &Earmarks,
) -> Value {
    let mut op = taking("sell", unit_id, entry, amount, unit, jobs, missions);
    set(&mut op, "terminal", Value::str(py_strip(terminal)));
    set(&mut op, "proceeds", Value::int(py_round_int(proceeds)));
    Value::Object(op)
}

#[allow(clippy::too_many_arguments)]
pub fn op_move_location(
    unit_id: &str,
    entry: &Entry,
    amount: f64,
    unit: &str,
    location: Value,
    jobs: &Earmarks,
    missions: &Earmarks,
    merge_scu: bool,
) -> Value {
    let mut op = taking("move-location", unit_id, entry, amount, unit, jobs, missions);
    set(&mut op, "location", location);
    if merge_scu && unit != PIECE {
        set(&mut op, "mergeScu", Value::Bool(true));
    }
    Value::Object(op)
}

#[allow(clippy::too_many_arguments)]
pub fn op_move_holder(
    unit_id: &str,
    entry: &Entry,
    amount: f64,
    unit: &str,
    to_unit: &str,
    holder_ref: &str,
    jobs: &Earmarks,
    missions: &Earmarks,
    merge_scu: bool,
) -> Value {
    let mut op = taking("move-holder", unit_id, entry, amount, unit, jobs, missions);
    set(&mut op, "toUnit", Value::str(to_unit));
    set(&mut op, "holder", Value::str(holder_ref));
    if merge_scu && unit != PIECE {
        set(&mut op, "mergeScu", Value::Bool(true));
    }
    Value::Object(op)
}

/// Without an amount the whole entry moves, reservations included.
pub fn op_move_pool(
    unit_id: &str,
    entry: &Entry,
    to_unit: &str,
    amount: Option<f64>,
    unit: &str,
    merge_scu: bool,
) -> Value {
    let mut op: Map = vec![
        ("op".into(), Value::str("move-pool")),
        ("unit".into(), Value::str(unit_id)),
        ("entry".into(), Value::str(&entry.id)),
        ("version".into(), Value::int(entry.version)),
        ("toUnit".into(), Value::str(to_unit)),
    ];
    if let Some(amount) = amount.filter(|a| *a > 0.0) {
        set(&mut op, "quantity", quantity(amount, unit));
    }
    if merge_scu && unit != PIECE {
        set(&mut op, "mergeScu", Value::Bool(true));
    }
    Value::Object(op)
}

/// Without text the note is deleted.
pub fn op_note(unit_id: &str, entry: &Entry, note: &str) -> Value {
    let mut op: Map = vec![
        ("op".into(), Value::str("note")),
        ("unit".into(), Value::str(unit_id)),
        ("entry".into(), Value::str(&entry.id)),
        ("version".into(), Value::int(entry.version)),
    ];
    if !py_strip(note).is_empty() {
        set(&mut op, "note", Value::str(py_strip(note)));
    }
    Value::Object(op)
}

/// Without an amount the mark holds for the whole entry.
pub fn op_stolen(unit_id: &str, entry: &Entry, stolen: bool, amount: Option<f64>, unit: &str) -> Value {
    let mut op: Map = vec![
        ("op".into(), Value::str("stolen")),
        ("unit".into(), Value::str(unit_id)),
        ("entry".into(), Value::str(&entry.id)),
        ("version".into(), Value::int(entry.version)),
        ("stolen".into(), Value::Bool(stolen)),
    ];
    if let Some(amount) = amount.filter(|a| *a > 0.0) {
        set(&mut op, "quantity", quantity(amount, unit));
    }
    Value::Object(op)
}

/// `kind`: `add`, `change` or `remove`; `target_kind`: `jobOrder` or
/// `mission`, never both.
pub fn op_allocation(
    kind: &str,
    unit_id: &str,
    entry: &Entry,
    target_kind: &str,
    target_id: &str,
    amount: f64,
    unit: &str,
) -> Value {
    let mut op: Map = vec![
        ("op".into(), Value::str(format!("allocation-{kind}"))),
        ("unit".into(), Value::str(unit_id)),
        ("entry".into(), Value::str(&entry.id)),
        ("version".into(), Value::int(entry.version)),
        (target_kind.into(), Value::str(target_id)),
    ];
    if kind != "remove" {
        set(&mut op, "quantity", quantity(amount, unit));
    }
    Value::Object(op)
}

/// What became of a change set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub applied: i64,
    pub unchanged: i64,
    /// (index, result, reason) of every operation not taken.
    pub failed: Vec<(Value, String, String)>,
    /// Offers on the material exchange made smaller or removed.
    pub offers: i64,
    /// The Basetool holds a mass change until it is confirmed there; then
    /// nothing is written.
    pub confirm_url: Option<String>,
    pub held: bool,
}

impl Outcome {
    /// The reasons of the refused operations, each once, sorted (the result
    /// when there is no reason).
    pub fn reasons(&self) -> Vec<String> {
        let found: BTreeSet<String> = self
            .failed
            .iter()
            .map(|(_, kind, reason)| if reason.is_empty() { kind.clone() } else { reason.clone() })
            .collect();
        found.into_iter().collect()
    }
}

/// The answer to a change set (`read_result`).
pub fn read_result(result: &Map) -> Outcome {
    let failed = list(result, "results")
        .iter()
        .filter_map(Value::as_object)
        .filter(|e| !matches!(get_str(e, "result"), Some("unchanged" | "applied")))
        .map(|e| {
            (
                get(e, "index").cloned().unwrap_or(Value::Null),
                get(e, "result").filter(|r| truthy(r)).map(py_str).unwrap_or_default(),
                text_of(get(e, "reason"), 64),
            )
        })
        .collect();
    Outcome {
        applied: py_int(get(result, "applied"), 0),
        unchanged: py_int(get(result, "unchanged"), 0),
        failed,
        offers: py_int(get(result, "offersReduced"), 0) + py_int(get(result, "offersRemoved"), 0),
        confirm_url: None,
        held: false,
    }
}

/// Sends one change set (`send`).
pub fn send(conn: &Connection, ops: Vec<Value>) -> Result<Outcome, ApiError> {
    let body = Value::Object(vec![("ops".into(), Value::Array(ops))]);
    match feed::send_change_set(conn, conn.config(), CHANGES, &body)? {
        Sent::Answer(answer) => Ok(read_result(&answer)),
        Sent::Held { confirm_url, .. } => Ok(Outcome { confirm_url, held: true, ..Outcome::default() }),
        // A booking is the player's own click: the write guard of a test says so instead of holding it.
        Sent::Guarded(error) => Err(error),
    }
}

// ------------------------------------------------------------ the page's helpers

/// A typed number: comma or point; `None` when it is none (`number`).
pub fn number(raw: &str) -> Option<f64> {
    let value = py_strip(raw).replace(' ', "").replace(',', ".");
    if value.is_empty() {
        return None;
    }
    py_float(&value)
}

/// Why an amount is refused: the text key and, for "at most", the limit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AmountError {
    Missing,
    Wrong,
    TooMuch(f64),
}

/// Checks a typed amount (`_amount_ok`). Empty is 0 when `optional`.
/// An amount that is no finite number (`nan`, `inf`) is wrong.
pub fn amount_ok(value: Option<f64>, piece: bool, maximum: Option<f64>, optional: bool) -> Result<f64, AmountError> {
    let Some(value) = value else {
        return if optional { Ok(0.0) } else { Err(AmountError::Missing) };
    };
    if !value.is_finite() || value <= 0.0 || (piece && value != value.trunc()) {
        return Err(AmountError::Wrong);
    }
    if let Some(maximum) = maximum
        && value > maximum + 1e-9
    {
        return Err(AmountError::TooMuch(maximum));
    }
    Ok(value)
}

/// The own entry of a member list: found by the game account, without
/// regard to case; empty when the account is unknown or missing there
/// (`own_member`).
pub fn own_member(roster: &[Member], account: Option<&str>) -> String {
    let fold = |s: &str| lower(py_strip(s)).replace('ß', "ss");
    let wanted = fold(account.unwrap_or_default());
    if wanted.is_empty() {
        return String::new();
    }
    roster.iter().find(|m| fold(&m.name) == wanted).map(|m| m.holder_ref.clone()).unwrap_or_default()
}

/// The tab of a stored view, the old four values included (`view_of`).
pub fn view_of(saved: Option<&str>) -> &'static str {
    match saved {
        Some("material" | "rohstoffe" | "handelsware") => "material",
        Some("items" | "komponenten") => "items",
        _ => "material",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basetool::connection::parse_answer;

    fn page(text: &str) -> Map {
        parse_answer(text.as_bytes())
    }

    #[test]
    fn lots_and_entries_like_python() {
        let snapshot = page(
            r#"{"items": [
                {"key": "k1", "unit": {"id": "u1", "name": "Alpha"}, "material": {"bt": "m1", "name": "Laranite"},
                 "materialKind": {"type": "REFINED", "commodity": true}, "location": {"name": " Area18 ", "uex": 7, "x": 1},
                 "quality": 1500, "stolen": false, "quantity": {"amount": 40, "unit": "SCU"},
                 "earmarked": {"jobOrders": {"amount": 4, "unit": "SCU"}, "missions": {"amount": 0, "unit": "SCU"}},
                 "entries": [{"id": "e1", "version": 3, "holder": {"withheld": false, "name": "Nova", "ref": "r1"},
                              "quantity": {"amount": "40", "unit": "SCU"}, "note": "Kiste 2",
                              "jobOrders": [{"id": "j1", "number": 12, "quantity": {"amount": 4}, "delivered": false}, {"x": 1}],
                              "missions": [], "free": {"jobOrders": {"amount": 36}}},
                             {"id": "e2", "version": 1, "holder": {"withheld": true, "name": "Geheim", "ref": "r2"},
                              "quantity": {"amount": 1}, "note": "weg"},
                             {"version": 2}]},
                {"key": "k2", "unit": {"id": "u1", "name": "Alpha"}, "material": {"bt": "m2", "name": "FR-66"},
                 "component": {"componentType": "SHIELD", "size": 2, "grade": "C", "itemClass": "MILITARY"},
                 "location": {"name": "Lorville"}, "quality": "12.5", "quantity": {"amount": 2, "unit": "PIECE"}},
                {"key": "k3", "unit": {"id": "u9"}, "material": {"name": "Gold"}},
                {"unit": {"id": "u1"}, "material": {"name": "Ohne Schlüssel"}}],
              "units": [{"id": "u1", "name": "Alpha", "shorthand": "ALP", "logistician": true}],
              "hasMore": false, "nextCursor": "c1"}"#,
        );
        let (lots, units) = apply_pages(Vec::new(), &[snapshot], true);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].label(), "Alpha (ALP)");
        assert_eq!(lots.len(), 2, "lot of an unknown unit and one without key drop out");
        let lot = &lots[0];
        assert_eq!((lot.quality, lot.place.as_str(), lot.job), (1000, "Area18", 4.0));
        assert_eq!(lot.location, vec![("name".into(), Value::str(" Area18 ")), ("uex".into(), Value::int(7))]);
        let entries = lot.entries.as_ref().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].amount, entries[0].free_jobs, entries[0].free_missions), (40.0, 36.0, 40.0));
        assert_eq!(entries[0].jobs.len(), 1);
        assert_eq!((entries[1].holder.as_str(), entries[1].note.as_str(), entries[1].withheld), ("", "", true));
        let item = &lots[1];
        assert!(item.is_item() && item.piece());
        assert_eq!(item.quality, 0, "int('12.5') fails in Python");
        assert_eq!(
            item.component,
            Some(Component { kind: "SHIELD".into(), size: Some(2), grade: "C".into(), class: "MILITARY".into() })
        );
        let (no_details, _) = apply_pages(lots.clone(), &[], false);
        assert!(no_details.is_empty(), "no page means no units: everything goes");
    }

    #[test]
    fn the_feed_replaces_removes_and_keeps_the_order() {
        let lot = |key: &str, name: &str| {
            format!(
                r#"{{"key": "{key}", "unit": {{"id": "u1"}}, "material": {{"name": "{name}"}}, "quantity": {{"amount": 1}}}}"#
            )
        };
        let units = r#""units": [{"id": "u1", "name": "A"}]"#;
        let first =
            page(&format!(r#"{{"items": [{}, {}, {}], {units}}}"#, lot("a", "A"), lot("b", "B"), lot("c", "C")));
        let (lots, _) = apply_pages(Vec::new(), &[first], false);
        let second = page(&format!(r#"{{"items": [{}], "removed": [{{"key": "a"}}, "x"], {units}}}"#, lot("b", "B2")));
        let (lots, _) = apply_pages(lots, &[second], false);
        let names: Vec<&str> = lots.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["B2", "C"]);
    }

    #[test]
    fn order_of_the_list_and_search() {
        let mut data = OrgStockData::default();
        for (key, name, place, quality, stolen) in [
            ("1", "gold", "b", 10, false),
            ("2", "Gold", "A", 5, true),
            ("3", "Gold", "a", 50, false),
            ("4", "Agricium", "x", 0, false),
        ] {
            data.lots.push(Lot {
                key: key.into(),
                unit_id: "u".into(),
                unit_name: String::new(),
                material_bt: String::new(),
                material_name: name.into(),
                name: name.into(),
                kind: None,
                component: None,
                location: Map::new(),
                place: place.into(),
                quality,
                stolen,
                amount: 1.0,
                unit_q: SCU.into(),
                job: 0.0,
                mission: 0.0,
                entries: None,
            });
        }
        let keys: Vec<&str> = data.lots_of("u", "").iter().map(|l| l.key.as_str()).collect();
        assert_eq!(keys, ["4", "3", "2", "1"]);
        let found: Vec<&str> = data.lots_of("u", " X ").iter().map(|l| l.key.as_str()).collect();
        assert_eq!(found, ["4"]);
        assert!(data.lots_of("andere", "").is_empty());
    }

    #[test]
    fn operations_like_python() {
        let entry =
            parse_entry(&jsonio::loads(r#"{"id": "e1", "version": 4, "quantity": {"amount": 5}}"#).unwrap()).unwrap();
        let jobs: Earmarks = [("j2".to_string(), 1.0), ("j1".to_string(), 0.0)].into();
        let op = op_book_out("u1", &entry, 2.00049, SCU, &jobs, &Earmarks::new());
        assert_eq!(
            jsonio::dumps(&op, None, false),
            r#"{"op": "book-out", "unit": "u1", "entry": "e1", "version": 4, "quantity": {"amount": 2.0, "unit": "SCU"}, "fromJobOrders": [{"jobOrder": "j2", "quantity": {"amount": 1.0, "unit": "SCU"}}]}"#
        );
        let book_in = op_book_in(
            "u1",
            "r1",
            Value::Object(vec![("bt".into(), Value::str("m1"))]),
            Value::Object(vec![("name".into(), Value::str("Area18"))]),
            700,
            false,
            2.5,
            PIECE,
            "  ",
            true,
        );
        assert_eq!(
            jsonio::dumps(&book_in, None, false),
            r#"{"op": "book-in", "unit": "u1", "holder": "r1", "material": {"bt": "m1"}, "location": {"name": "Area18"}, "quality": 0, "stolen": false, "quantity": {"amount": 2, "unit": "PIECE"}}"#
        );
        let pool = op_move_pool("u1", &entry, "u2", None, SCU, true);
        assert_eq!(
            jsonio::dumps(&pool, None, false),
            r#"{"op": "move-pool", "unit": "u1", "entry": "e1", "version": 4, "toUnit": "u2", "mergeScu": true}"#
        );
        let remove = op_allocation("remove", "u1", &entry, "mission", "m9", 3.0, SCU);
        assert!(!jsonio::dumps(&remove, None, false).contains("quantity"));
    }

    #[test]
    fn results_and_reasons() {
        let answer = page(
            r#"{"applied": 1, "unchanged": 0, "results": [{"index": 0, "result": "unchanged"},
                {"index": 1, "result": "rejected", "reason": "VERSION_CONFLICT"}, {"index": 2, "result": "unmatched"},
                {"index": 3, "result": "rejected", "reason": "VERSION_CONFLICT"}], "offersReduced": 2, "offersRemoved": 1}"#,
        );
        let outcome = read_result(&answer);
        assert_eq!((outcome.applied, outcome.offers, outcome.failed.len()), (1, 3, 3));
        assert_eq!(outcome.reasons(), ["VERSION_CONFLICT", "unmatched"]);
    }

    #[test]
    fn resolving_keeps_the_sure_hit_among_the_candidates() {
        let found = |bt: &str, kind: &str| Found { bt: bt.into(), name: bt.into(), kind: kind.into() };
        let many: Vec<Found> = (0..10).map(|i| found(&format!("m{i}"), MATERIAL)).collect();
        let answers = std::cell::RefCell::new(vec![Resolved::Several(many), Resolved::One(found("i1", ITEM))]);
        let asked = std::cell::RefCell::new(Vec::new());
        let result = resolve_with("  Golden Medmon ", None, false, |kind, reference| {
            asked.borrow_mut().push((kind.to_string(), jsonio::dumps(reference, None, false)));
            Ok(answers.borrow_mut().remove(0))
        })
        .unwrap();
        let Resolved::Several(hits) = result else { panic!("not ambiguous") };
        assert_eq!(hits.len(), 10);
        assert_eq!(hits.last().map(|h| h.bt.as_str()), Some("i1"));
        assert_eq!(asked.borrow()[0], ("MATERIAL".to_string(), r#"{"name": "Golden Medmon"}"#.to_string()));
        // A unique first kind wins; a known reference goes first.
        let reference = Value::Object(vec![("uexId".into(), Value::int(5))]);
        let first =
            resolve_with("x", Some((ITEM, &reference)), true, |_, _| Ok(Resolved::One(found("b", ITEM)))).unwrap();
        assert_eq!(first, Resolved::One(found("b", ITEM)));
        let none = resolve_with("x", None, true, |_, _| Ok(Resolved::Unmatched)).unwrap();
        assert_eq!(none, Resolved::Unmatched);
    }

    #[test]
    fn amounts_numbers_members_views() {
        assert_eq!(number(" 1,5 "), Some(1.5));
        assert_eq!(number("1 000"), Some(1000.0));
        assert_eq!(number(""), None);
        assert_eq!(number("abc"), None);
        assert_eq!(amount_ok(None, false, None, true), Ok(0.0));
        assert_eq!(amount_ok(None, false, None, false), Err(AmountError::Missing));
        assert_eq!(amount_ok(Some(1.5), true, None, false), Err(AmountError::Wrong));
        assert_eq!(amount_ok(Some(f64::NAN), false, None, false), Err(AmountError::Wrong));
        assert_eq!(amount_ok(Some(5.0), false, Some(4.0), false), Err(AmountError::TooMuch(4.0)));
        assert_eq!(amount_ok(Some(4.0), false, Some(4.0), false), Ok(4.0));
        let roster = [
            Member { holder_ref: "r1".into(), name: "Nova".into() },
            Member { holder_ref: "r2".into(), name: " Straße ".into() },
        ];
        assert_eq!(own_member(&roster, Some("STRASSE")), "r2");
        assert_eq!(own_member(&roster, Some("")), "");
        assert_eq!(own_member(&roster, None), "");
        assert_eq!(
            (view_of(Some("komponenten")), view_of(Some("handelsware")), view_of(None)),
            ("items", "material", "material")
        );
    }
}
