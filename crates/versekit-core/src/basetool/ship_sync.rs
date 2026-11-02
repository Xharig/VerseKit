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

//! The hangar and the Basetool, the part without the net
//! (`exchange_ships.py`): what the hangar says about its ships, what is to be
//! done to make both sides agree, and the change sets that carry it out.
//! Nothing here asks the Basetool for anything; the answers it gave come in as
//! arguments, so the whole of it is checked against 4.12 with made-up ones
//! (`tools/parity_shipsync.py`).
//!
//! ⚠⚠ **VerseKit keeps ONE entry per ship type** (`hangar::merge_duplicates`),
//! the Basetool every ship on its own. Exactly one ship per type is linked; a
//! second one of the same type over there is left alone and never removed.
//!
//! The identifier of an entry (`externalId`) comes from maker and name, so
//! `hangar.json` needs no new field and the identifier stays the same over any
//! import.
//!
//! The rules (`resources/ships.md`, the sync guide):
//! 1. **Link first, then create.** A type that exists over there is hung on a
//!    ship there (`link`); only what is missing is created.
//! 2. **Every change sends the current name and location along**, because an
//!    `upsert` deletes what is missing. The location is unknown here and goes
//!    back as it came. The name is the own name of the Fleet Manager where the
//!    ship maps to a game name. `fitted` is the tick "ready for use"; left out,
//!    the server keeps its value.
//! 3. **Never purchase data.** Price, bought, package, warbond stay here.
//! 4. **Removing needs approval.** A ship missing here (say a melted pledge)
//!    is shown to the player.
//! 5. **What was removed elsewhere does not come back silently**, only with
//!    `override` after consent.
//!
//! ⚠ Not wired to a sync worker yet and no request is made: P6 hands over the
//! pure half. The worker puts [`plan`] and [`change_sets`] on top of the feed.

use std::collections::{BTreeMap, BTreeSet};

use crate::asop::MAX_LENGTH;
use crate::basetool::error::py_str;
use crate::hangar::{self, get, text, truthy};
use crate::jsonio::{Map, Value, map_set, py_eq, py_eq_value};
use crate::paths::name_key;
use crate::text::{py_strip, sha1_hex};

pub const LTI: &str = "LTI";
pub const MONTHS: &str = "MONTHS";

/// How far a name of the hangar goes into the reference of the ship type.
const REF_LENGTH: usize = 200;
/// How far an own name goes (the Fleet Manager's limit).
const OWN_NAME_LENGTH: usize = 255;
/// The most insurance months the contract takes.
const MAX_MONTHS: i128 = 120;

/// The first `count` characters (Python slices by code point).
fn head(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

/// The fixed identifier of a hangar entry, from maker and name.
pub fn external_id(entry: &Map) -> String {
    let base = format!("{}|{}", name_key(text(entry, "hersteller")), name_key(text(entry, "name")));
    format!("vk-{}", &sha1_hex(base.as_bytes())[..24])
}

/// Python's `int(value)` for the values a hangar file can hold; `None` where
/// Python raises `TypeError` or `ValueError`. (Only ASCII digits are read from
/// a text; an infinite number, which Python refuses in another way, counts as
/// unreadable too.)
fn py_int(value: Option<&Value>) -> Option<i128> {
    match value? {
        Value::Bool(b) => Some(i128::from(*b)),
        Value::Int(digits) => {
            Some(digits.parse().unwrap_or(if digits.starts_with('-') { i128::MIN } else { i128::MAX }))
        }
        Value::Float(x) if x.is_finite() => Some(*x as i128),
        Value::Str(s) => {
            let s = py_strip(s);
            let (negative, digits) = match s.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, s.strip_prefix('+').unwrap_or(s)),
            };
            // `1_0` is ten, but `_1`, `1_` and `1__0` are no numbers.
            let ok = !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit() || c == '_')
                && !digits.starts_with('_')
                && !digits.ends_with('_')
                && !digits.contains("__");
            if !ok {
                return None;
            }
            let number = digits
                .chars()
                .filter_map(|c| c.to_digit(10))
                .fold(0i128, |n, d| n.saturating_mul(10).saturating_add(i128::from(d)));
            Some(if negative { -number } else { number })
        }
        _ => None,
    }
}

/// The hangar's data to the `insurance` of the contract.
pub fn insurance(entry: &Map) -> Value {
    if truthy(get(entry, "lti")) {
        return Value::Object(vec![("kind".to_string(), Value::str(LTI))]);
    }
    let months = py_int(get(entry, "versicherung")).unwrap_or(0).clamp(0, MAX_MONTHS);
    Value::Object(vec![("kind".to_string(), Value::str(MONTHS)), ("months".to_string(), Value::int(months as i64))])
}

/// One ship of the hangar as the sync sees it. Purchase data is not part of it,
/// so none can go out by mistake.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalShip {
    pub name: String,
    /// Where the entry came from (`herkunft`), if it says.
    pub origin: Option<String>,
    /// The reference of the ship type: `name`, and `scRecord` when known.
    pub reference: Map,
    pub insurance: Value,
    /// The own name in the Fleet Manager; `None` when the ship maps to no game
    /// name (then it is neither sent nor fetched).
    pub own_name: Option<String>,
    pub asop_key: Option<String>,
    /// `None` as long as the tick was never set.
    pub fitted: Option<bool>,
    /// `None` = never chosen, `""` = expressly no unit.
    pub unit: Option<String>,
}

/// The ships of the hangar by identifier.
///
/// `own_names` is `{hangar name: (key, own name)}` for the ships that map to a
/// game name. Boats of the catalogue are left out: they stand in no pledge and
/// the Basetool keeps possession.
pub fn local_ships(entries: &[&Map], own_names: &BTreeMap<String, (String, String)>) -> BTreeMap<String, LocalShip> {
    let mut out = BTreeMap::new();
    for entry in entries {
        let name = py_strip(text(entry, "name")).to_string();
        let origin = get(entry, "herkunft").and_then(Value::as_str);
        if name.is_empty() || origin == Some(hangar::CATALOG) {
            continue;
        }
        let mut reference: Map = vec![("name".to_string(), Value::str(head(&name, REF_LENGTH)))];
        if truthy(get(entry, "kurz")) {
            reference.push(("scRecord".to_string(), Value::str(head(text(entry, "kurz"), REF_LENGTH))));
        }
        let own = own_names.get(&name);
        let fitted = get(entry, "einsatzbereit").filter(|v| !matches!(v, Value::Null)).map(|v| truthy(Some(v)));
        let unit = get(entry, "einheit").filter(|v| !matches!(v, Value::Null)).map(py_str);
        out.insert(
            external_id(entry),
            LocalShip {
                name,
                origin: origin.map(str::to_string),
                reference,
                insurance: insurance(entry),
                own_name: own.map(|(_, own)| head(own, OWN_NAME_LENGTH)),
                asop_key: own.map(|(key, _)| key.clone()),
                fitted,
                unit,
            },
        );
    }
    out
}

/// The identifier of a ship's unit in the Basetool; `""` without one.
pub fn server_unit(ship: &Value) -> String {
    match ship.get("orgUnit") {
        Some(unit) if truthy(Some(unit)) => match unit.get("id") {
            Some(id) if truthy(Some(id)) => py_str(id),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Push,
    Pull,
}

/// A value of the last sync that is really there (a stored `null` is none).
fn known(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !matches!(v, Value::Null))
}

/// Push, pull or nothing for the unit. ⚠ In a contradiction the Basetool wins:
/// officers and admins change the unit there too and VerseKit must never undo
/// that silently. Never chosen means: take the unit from over there.
fn unit_side(mine: Option<&str>, theirs: &str, before: Option<&Value>) -> Option<Side> {
    let Some(mine) = mine else { return (!theirs.is_empty()).then_some(Side::Pull) };
    if mine == theirs {
        return None;
    }
    // ⚠ Without the state of the last sync nobody can say who changed it, so
    // the Basetool counts.
    if before.is_some_and(|b| py_eq_value(&Value::str(theirs), b)) { Some(Side::Push) } else { Some(Side::Pull) }
}

/// Push, pull or nothing for the name. Without a state of the last sync: a
/// missing name comes from the other side; two different ones, ours goes out.
fn name_side(mine: &str, theirs: &str, before: Option<&str>) -> Option<Side> {
    if mine == theirs {
        return None;
    }
    let Some(before) = before else {
        return Some(if theirs.is_empty() || !mine.is_empty() { Side::Push } else { Side::Pull });
    };
    if theirs == before {
        Some(Side::Push)
    } else if mine == before {
        Some(Side::Pull)
    } else {
        Some(Side::Push)
    }
}

/// Push, pull or nothing for `fitted`. A tick never set takes only a set
/// `fitted` from over there: a `false` there is the same as nothing.
fn fitted_side(mine: Option<bool>, theirs: bool, before: Option<&Value>) -> Option<Side> {
    let Some(mine) = mine else { return theirs.then_some(Side::Pull) };
    if mine == theirs {
        return None;
    }
    if before.is_none_or(|b| py_eq_value(&Value::Bool(theirs), b)) {
        Some(Side::Push)
    } else if py_eq_value(&Value::Bool(mine), before.unwrap_or(&Value::Null)) {
        Some(Side::Pull)
    } else {
        Some(Side::Push)
    }
}

/// Does this insurance carry a statement? `MONTHS` with 0 does not: that is how
/// VerseKit sends a ship without insurance.
pub fn covered(cover: Option<&Value>) -> bool {
    let Some(Value::Object(cover)) = cover else { return false };
    matches!(get(cover, "kind"), Some(Value::Str(kind)) if kind == LTI) || truthy(get(cover, "months"))
}

/// Push, pull or nothing for the insurance. ⚠ Without the state of the last
/// sync the insurance of the Basetool counts; ours goes out only where theirs
/// says nothing. Otherwise the next sync would overwrite an insurance kept over
/// there.
fn insurance_side(ours: &Value, theirs: Option<&Value>, before: Option<&Value>) -> Option<Side> {
    if py_eq(Some(ours), theirs) {
        return None;
    }
    let Some(before) = before else {
        return if covered(theirs) { Some(Side::Pull) } else { covered(Some(ours)).then_some(Side::Push) };
    };
    if py_eq(theirs, Some(before)) {
        Some(Side::Push)
    } else if py_eq_value(ours, before) {
        Some(Side::Pull)
    } else {
        None
    }
}

/// What the last sync left behind, per identifier.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShipState {
    /// externalId to shipId.
    pub links: BTreeMap<String, String>,
    pub baseline: BTreeMap<String, Value>,
    pub baseline_name: BTreeMap<String, String>,
    pub baseline_fitted: BTreeMap<String, Value>,
    pub baseline_unit: BTreeMap<String, Value>,
    pub removed_elsewhere: BTreeSet<String>,
    pub pending_removals: BTreeSet<String>,
    pub overrides: BTreeSet<String>,
    /// "Keep in the Basetool".
    pub kept: BTreeSet<String>,
}

impl ShipState {
    /// Reads the `ships` part of the sync state; what is not there is empty.
    pub fn from_value(state: &Value) -> Self {
        fn map_of(state: &Value, key: &str) -> BTreeMap<String, Value> {
            match state.get(key) {
                Some(Value::Object(items)) => items.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => BTreeMap::new(),
            }
        }
        fn set_of(state: &Value, key: &str) -> BTreeSet<String> {
            match state.get(key) {
                Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
                _ => BTreeSet::new(),
            }
        }
        let strings = |key: &str| -> BTreeMap<String, String> {
            map_of(state, key).into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string()))).collect()
        };
        Self {
            links: strings("links"),
            baseline: map_of(state, "baseline"),
            baseline_name: strings("baseline_name"),
            baseline_fitted: map_of(state, "baseline_fitted"),
            baseline_unit: map_of(state, "baseline_unit"),
            removed_elsewhere: set_of(state, "removed_elsewhere"),
            pending_removals: set_of(state, "pending_removals"),
            overrides: set_of(state, "overrides"),
            kept: set_of(state, "kept"),
        }
    }
}

const FIELD_INSURANCE: &str = "insurance";
const FIELD_NAME: &str = "name";
const FIELD_FITTED: &str = "fitted";
const FIELD_UNIT: &str = "orgUnit";

/// Per field, whether it goes out or comes in: `(push, pull)`.
fn sides(
    mine: &LocalShip,
    ship: &Value,
    ext: &str,
    state: &ShipState,
) -> (BTreeSet<&'static str>, BTreeSet<&'static str>) {
    let mut push = BTreeSet::new();
    let mut pull = BTreeSet::new();
    let mut put = |field: &'static str, side: Option<Side>| match side {
        Some(Side::Push) => {
            push.insert(field);
        }
        Some(Side::Pull) => {
            pull.insert(field);
        }
        None => {}
    };
    put(FIELD_INSURANCE, insurance_side(&mine.insurance, ship.get("insurance"), known(state.baseline.get(ext))));
    if let Some(own) = &mine.own_name {
        // ⚠ Compared at the length the Fleet Manager holds: a longer name from
        // over there would come in cut and go out cut at the next sync.
        let theirs = match ship.get("name") {
            Some(name) if truthy(Some(name)) => head(&py_str(name), MAX_LENGTH),
            _ => String::new(),
        };
        let before = state.baseline_name.get(ext).map(|b| head(b, MAX_LENGTH));
        put(FIELD_NAME, name_side(&head(own, MAX_LENGTH), &theirs, before.as_deref()));
    }
    put(FIELD_FITTED, fitted_side(mine.fitted, truthy(ship.get("fitted")), known(state.baseline_fitted.get(ext))));
    put(FIELD_UNIT, unit_side(mine.unit.as_deref(), &server_unit(ship), known(state.baseline_unit.get(ext))));
    (push, pull)
}

/// What is to be done, without doing anything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Hang a hangar ship on a ship over there: (externalId, shipId).
    pub link: Vec<(String, String)>,
    pub create: Vec<String>,
    pub update: Vec<(String, String)>,
    pub pull: Vec<(String, String)>,
    pub remove: Vec<(String, String)>,
    pub push_fields: BTreeMap<String, Vec<String>>,
    pub pull_fields: BTreeMap<String, Vec<String>>,
    /// Ships over there of a type the hangar lacks: take them in.
    pub take: Vec<String>,
    /// Ships gone over there that came from there: leave them out of the hangar.
    pub drop: Vec<String>,
    pub local_removed: BTreeMap<String, String>,
    pub removed_elsewhere: Vec<String>,
    pub links: BTreeMap<String, String>,
    /// Ships whose unit was changed on both sides (the Basetool won).
    pub unit_conflicts: Vec<String>,
    /// The pulls that only hold if their `link` is accepted.
    pub link_pull: Vec<String>,
    /// The creations the player has agreed to again.
    pub override_: Vec<String>,
}

/// The `bt` of a ship over there, as a key for comparing: a text stays itself,
/// anything else cannot equal a text.
fn type_key(bt: Option<&Value>) -> Option<String> {
    match bt {
        Some(value) if truthy(Some(value)) => Some(match value {
            Value::Str(s) => s.clone(),
            other => format!("\u{0}{}", py_str(other)),
        }),
        _ => None,
    }
}

fn server_type(ship: &Value) -> Option<String> {
    type_key(ship.get("shipType").filter(|t| truthy(Some(t))).and_then(|t| t.get("bt")))
}

/// What is to be done, without doing it.
///
/// `local` comes from [`local_ships`], `resolved` maps the identifier to the
/// `bt` of the ship type, `server` holds the ships over there (shipId to ship)
/// after fetching, `state` what the last sync left.
pub fn plan(
    local: &BTreeMap<String, LocalShip>,
    resolved: &BTreeMap<String, String>,
    server: &BTreeMap<String, Value>,
    state: &ShipState,
) -> Plan {
    let mut links = state.links.clone();
    // What the server already knows as our link counts.
    for (ship_id, ship) in server {
        if let Some(Value::Str(ext)) = ship.get("externalId").filter(|e| truthy(Some(e))) {
            links.insert(ext.clone(), ship_id.clone());
        }
    }
    let mut result = Plan::default();
    let mut taken: BTreeSet<&str> = BTreeSet::new();
    let resolved_bt = |ext: &str| resolved.get(ext).filter(|bt| !bt.is_empty());

    // 1) The linked: still there? changed here?
    for (ext, ship_id) in &links {
        let ship = server.get(ship_id);
        let Some(mine) = local.get(ext) else {
            if ship.is_some() {
                if state.kept.contains(ext) {
                    taken.insert(ship_id);
                } else if !state.pending_removals.contains(ext) {
                    result.local_removed.insert(ext.clone(), ship_id.clone());
                } else {
                    result.remove.push((ext.clone(), ship_id.clone()));
                }
            }
            continue;
        };
        let Some(ship) = ship else {
            // Gone over there and not by us: never create it again silently.
            if mine.origin.as_deref() == Some(hangar::BASETOOL) {
                result.drop.push(ext.clone());
            } else if state.overrides.contains(ext) {
                result.create.push(ext.clone());
            } else {
                result.removed_elsewhere.push(ext.clone());
            }
            continue;
        };
        taken.insert(ship_id);
        result.links.insert(ext.clone(), ship_id.clone());
        let (push, pull) = sides(mine, ship, ext, state);
        if !push.is_empty() {
            result.update.push((ext.clone(), ship_id.clone()));
            result.push_fields.insert(ext.clone(), push.iter().map(|f| (*f).to_string()).collect());
        }
        if !pull.is_empty() {
            result.pull.push((ext.clone(), ship_id.clone()));
            result.pull_fields.insert(ext.clone(), pull.iter().map(|f| (*f).to_string()).collect());
            // Both sides changed and the Basetool wins: that belongs in the
            // message of the sync.
            let before = known(state.baseline_unit.get(ext));
            if pull.contains(FIELD_UNIT)
                && let (Some(unit), Some(before)) = (&mine.unit, before)
                && !py_eq_value(&Value::str(unit.as_str()), before)
            {
                result.unit_conflicts.push(ext.clone());
            }
        }
    }

    // 2) The not yet linked: first hang on a ship that is there. ⚠ A type that
    // a linked ship here already holds is neither created again nor linked a
    // second time: there is one entry per type here, a second one is a double
    // the name did not give away.
    let mut linked_types: BTreeSet<String> = result.links.keys().filter_map(|ext| resolved_bt(ext).cloned()).collect();
    for ext in local.keys() {
        if links.get(ext).is_some_and(|id| server.contains_key(id)) {
            continue;
        }
        // Just left out (`drop`) means: deleted over there, do not create again.
        if result.removed_elsewhere.contains(ext) || result.create.contains(ext) || result.drop.contains(ext) {
            continue;
        }
        let Some(bt) = resolved_bt(ext) else { continue };
        if linked_types.contains(bt) {
            continue;
        }
        let found = server.iter().find(|(sid, ship)| {
            !taken.contains(sid.as_str())
                && !truthy(ship.get("externalId"))
                && server_type(ship).as_deref() == Some(bt.as_str())
        });
        if let Some((match_id, ship)) = found {
            result.link.push((ext.clone(), match_id.clone()));
            result.links.insert(ext.clone(), match_id.clone());
            taken.insert(match_id);
            linked_types.insert(bt.clone());
            // Without the state of the last sync an insurance from over there
            // comes in right away (`insurance_side`); `link_pull` names it so
            // it only counts when the `link` is accepted too.
            if insurance_side(&local[ext].insurance, ship.get("insurance"), None) == Some(Side::Pull) {
                result.pull.push((ext.clone(), match_id.clone()));
                result.pull_fields.insert(ext.clone(), vec![FIELD_INSURANCE.to_string()]);
                result.link_pull.push(ext.clone());
            }
        } else if !state.removed_elsewhere.contains(ext) || state.overrides.contains(ext) {
            result.create.push(ext.clone());
            linked_types.insert(bt.clone());
        }
    }

    // 3) Ships over there of a type that is not here: take them in.
    let mut local_types: BTreeSet<String> = local.keys().filter_map(|ext| resolved_bt(ext).cloned()).collect();
    for (ship_id, ship) in server {
        if taken.contains(ship_id.as_str()) || truthy(ship.get("externalId")) {
            continue;
        }
        if let Some(bt) = server_type(ship)
            && !local_types.contains(&bt)
        {
            result.take.push(ship_id.clone());
            local_types.insert(bt); // each type is taken in once
        }
    }
    result.override_ = result.create.iter().filter(|ext| state.overrides.contains(*ext)).cloned().collect();
    result
}

/// The value of `version` (or `orgUnitVersion`), `0` where the ship has none.
fn counter(ship: &Value, key: &str) -> Value {
    ship.get(key).cloned().unwrap_or_else(|| Value::int(0))
}

/// The entries of an object whose value says something, in the order given.
fn kept_entries(value: Option<&Value>, keys: &[&str]) -> Value {
    let items = match value {
        Some(Value::Object(items)) => items.as_slice(),
        _ => &[],
    };
    Value::Object(items.iter().filter(|(k, v)| keys.contains(&k.as_str()) && truthy(Some(v))).cloned().collect())
}

/// The shipments: `(change set, identifiers)` per batch.
///
/// `new_unit` is the unit for new ships (a setting), `None` means "as before"
/// and the Basetool decides. `known_units` are the identifiers of the member's
/// units (`None` = list not fetched): on creating, a unit goes out only if it
/// is in there, otherwise the Basetool would refuse the whole creation, at
/// every sync again.
///
/// ⚠ `link` always comes BEFORE creating: the guide wants it so and the server
/// decides in order.
pub fn change_sets(
    plan: &Plan,
    local: &BTreeMap<String, LocalShip>,
    resolved: &BTreeMap<String, String>,
    server: &BTreeMap<String, Value>,
    batch: usize,
    new_unit: Option<&str>,
    known_units: Option<&BTreeSet<String>>,
) -> Vec<(Value, Vec<String>)> {
    let mut ops: Vec<Map> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let ship_type = |ext: &str| -> Value {
        let mut reference = local[ext].reference.clone();
        if let Some(bt) = resolved.get(ext).filter(|bt| !bt.is_empty()) {
            map_set(&mut reference, "bt", Value::str(bt.as_str()));
        }
        Value::Object(reference)
    };
    let text_field = |key: &str, value: &str| (key.to_string(), Value::str(value));

    for (ext, ship_id) in &plan.link {
        ops.push(vec![text_field("op", "link"), text_field("externalId", ext), text_field("shipId", ship_id)]);
        keys.push(ext.clone());
    }
    for ext in &plan.create {
        let mine = &local[ext];
        let mut op = vec![
            text_field("op", "upsert"),
            text_field("externalId", ext),
            ("shipType".to_string(), ship_type(ext)),
            ("insurance".to_string(), mine.insurance.clone()),
        ];
        if let Some(own) = mine.own_name.as_deref().filter(|own| !own.is_empty()) {
            op.push(text_field("name", own));
        }
        if let Some(fitted) = mine.fitted {
            op.push(("fitted".to_string(), Value::Bool(fitted)));
        }
        // On creating without a counter. A unit chosen on the card goes before
        // the setting; `""` (no unit) goes out as `null`.
        let unit = mine.unit.as_deref().or(new_unit);
        if let (Some(unit), Some(known)) = (unit, known_units)
            && (unit.is_empty() || known.contains(unit))
        {
            op.push(("orgUnit".to_string(), if unit.is_empty() { Value::Null } else { Value::str(unit) }));
        }
        if plan.override_.contains(ext) {
            op.push(("override".to_string(), Value::Bool(true)));
        }
        ops.push(op);
        keys.push(ext.clone());
    }
    for (ext, ship_id) in &plan.update {
        let ship = &server[ship_id];
        let mine = &local[ext];
        let fields: BTreeSet<&str> = match plan.push_fields.get(ext) {
            Some(fields) => fields.iter().map(String::as_str).collect(),
            None => BTreeSet::from([FIELD_INSURANCE]),
        };
        // ⚠ What does not go out goes back with the value of the Basetool,
        // otherwise the `upsert` deletes name and location (rule 2).
        let insurance = if fields.contains(FIELD_INSURANCE) {
            mine.insurance.clone()
        } else {
            match ship.get("insurance") {
                Some(theirs) if truthy(Some(theirs)) => theirs.clone(),
                _ => mine.insurance.clone(),
            }
        };
        let mut op = vec![
            text_field("op", "upsert"),
            text_field("externalId", ext),
            text_field("shipId", ship_id),
            ("version".to_string(), counter(ship, "version")),
            ("shipType".to_string(), kept_entries(ship.get("shipType"), &["bt", "name"])),
            ("insurance".to_string(), insurance),
        ];
        let name = if fields.contains(FIELD_NAME) {
            mine.own_name.as_deref().filter(|own| !own.is_empty()).map(Value::str)
        } else {
            ship.get("name").filter(|n| truthy(Some(n))).cloned()
        };
        if let Some(name) = name {
            op.push(("name".to_string(), name));
        }
        if fields.contains(FIELD_FITTED) {
            op.push(("fitted".to_string(), Value::Bool(mine.fitted.unwrap_or(false))));
        }
        // ⚠ Without `orgUnit` the Basetool keeps the unit: so only when it was
        // changed here, and then with its own counter.
        if fields.contains(FIELD_UNIT) {
            let unit = mine.unit.as_deref().filter(|u| !u.is_empty());
            op.push(("orgUnit".to_string(), unit.map_or(Value::Null, Value::str)));
            op.push(("orgUnitVersion".to_string(), counter(ship, "orgUnitVersion")));
        }
        if truthy(ship.get("location")) {
            op.push(("location".to_string(), kept_entries(ship.get("location"), &["name", "uex"])));
        }
        ops.push(op);
        keys.push(ext.clone());
    }
    for (ext, ship_id) in &plan.remove {
        ops.push(vec![
            text_field("op", "remove"),
            text_field("shipId", ship_id),
            ("version".to_string(), counter(&server[ship_id], "version")),
        ]);
        keys.push(ext.clone());
    }
    for (n, op) in ops.iter_mut().enumerate() {
        op.push(("opId".to_string(), Value::str(format!("h{}", n + 1))));
    }
    let size = batch.max(1);
    ops.chunks(size)
        .zip(keys.chunks(size))
        .map(|(ops, keys)| {
            let list = ops.iter().map(|op| Value::Object(op.clone())).collect();
            (Value::Object(vec![("ops".to_string(), Value::Array(list))]), keys.to_vec())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn value(text: &str) -> Value {
        loads(text).unwrap()
    }

    fn entry(text: &str) -> Map {
        value(text).as_object().unwrap().clone()
    }

    #[test]
    fn sha1_is_the_known_one_and_the_identifier_the_one_of_4_12() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        let long = "a".repeat(1000);
        assert_eq!(sha1_hex(long.as_bytes()), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
        let id = external_id(&entry(r#"{"hersteller": "Aegis Dynamics", "name": "Avenger Titan"}"#));
        assert_eq!(id, "vk-6d408fe079641bdc20845993");
        assert_eq!(external_id(&entry("{}")), "vk-3eb416223e9e69e6bb8ee197");
    }

    #[test]
    fn insurance_is_lti_or_months_from_0_to_120() {
        let kind = |text: &str| insurance(&entry(text));
        assert_eq!(kind(r#"{"lti": true, "versicherung": 5}"#), value(r#"{"kind": "LTI"}"#));
        assert_eq!(kind(r#"{"versicherung": 6}"#), value(r#"{"kind": "MONTHS", "months": 6}"#));
        assert_eq!(kind(r#"{"versicherung": "1_0"}"#), value(r#"{"kind": "MONTHS", "months": 10}"#));
        assert_eq!(kind(r#"{"versicherung": 999}"#), value(r#"{"kind": "MONTHS", "months": 120}"#));
        assert_eq!(kind(r#"{"versicherung": -4}"#), value(r#"{"kind": "MONTHS", "months": 0}"#));
        assert_eq!(kind(r#"{"versicherung": "abc"}"#), value(r#"{"kind": "MONTHS", "months": 0}"#));
        assert_eq!(kind(r#"{"versicherung": 3.9}"#), value(r#"{"kind": "MONTHS", "months": 3}"#));
        assert_eq!(kind("{}"), value(r#"{"kind": "MONTHS", "months": 0}"#));
    }

    fn ship(id: &str, extra: &str) -> (String, Value) {
        (id.to_string(), value(&format!(r#"{{"shipType": {{"bt": "bt-a", "name": "A"}}, "version": 3{extra}}}"#)))
    }

    fn one_local(text: &str) -> BTreeMap<String, LocalShip> {
        let entry = entry(text);
        local_ships(&[&entry], &BTreeMap::new())
    }

    #[test]
    fn a_type_is_linked_before_it_is_created_and_a_second_of_the_type_is_left_alone() {
        let local =
            one_local(r#"{"name": "Avenger Titan", "hersteller": "Aegis Dynamics", "kurz": "AEGS_Avenger_Titan"}"#);
        let ext = local.keys().next().unwrap().clone();
        let resolved = BTreeMap::from([(ext.clone(), "bt-a".to_string())]);
        let server: BTreeMap<_, _> = [ship("s1", ""), ship("s2", "")].into();
        let plan = plan(&local, &resolved, &server, &ShipState::default());
        assert_eq!(plan.link, vec![(ext.clone(), "s1".to_string())]);
        assert!(plan.create.is_empty(), "linked, not created");
        assert!(plan.take.is_empty(), "the second ship of the type is not taken in either");
        let sets = change_sets(&plan, &local, &resolved, &server, 500, None, None);
        assert_eq!(sets.len(), 1);
        let ops = sets[0].0.get("ops").unwrap();
        assert_eq!(ops, &value(&format!(r#"[{{"op": "link", "externalId": "{ext}", "shipId": "s1", "opId": "h1"}}]"#)));
    }

    #[test]
    fn a_ship_gone_over_there_is_not_created_again_without_consent() {
        let local = one_local(r#"{"name": "Avenger Titan", "hersteller": "Aegis Dynamics"}"#);
        let ext = local.keys().next().unwrap().clone();
        let resolved = BTreeMap::from([(ext.clone(), "bt-a".to_string())]);
        let mut state = ShipState::default();
        state.links.insert(ext.clone(), "gone".to_string());
        let server = BTreeMap::new();
        assert_eq!(plan(&local, &resolved, &server, &state).removed_elsewhere, vec![ext.clone()]);
        state.overrides.insert(ext.clone());
        let again = plan(&local, &resolved, &server, &state);
        assert_eq!(again.create, vec![ext.clone()]);
        assert_eq!(again.override_, vec![ext.clone()]);
        let sets = change_sets(&again, &local, &resolved, &server, 500, None, None);
        let op = match sets[0].0.get("ops") {
            Some(Value::Array(ops)) => ops[0].clone(),
            _ => panic!("ops"),
        };
        assert_eq!(op.get("override"), Some(&Value::Bool(true)));
        assert_eq!(op.get("shipType").and_then(|t| t.get("bt")), Some(&Value::str("bt-a")));
    }

    #[test]
    fn what_does_not_go_out_goes_back_with_the_value_of_the_basetool() {
        let local = one_local(r#"{"name": "Avenger Titan", "hersteller": "Aegis Dynamics", "lti": true}"#);
        let ext = local.keys().next().unwrap().clone();
        let resolved = BTreeMap::from([(ext.clone(), "bt-a".to_string())]);
        let server: BTreeMap<_, _> = [ship(
            "s1",
            &format!(r#", "externalId": "{ext}", "name": "Bob", "location": {{"name": "Area18", "other": 1}}, "insurance": {{"kind": "MONTHS", "months": 3}}"#),
        )]
        .into();
        let state = ShipState::from_value(&value(&format!(
            r#"{{"links": {{"{ext}": "s1"}}, "baseline": {{"{ext}": {{"kind": "MONTHS", "months": 3}}}}}}"#
        )));
        let plan = plan(&local, &resolved, &server, &state);
        assert_eq!(plan.push_fields[&ext], vec!["insurance".to_string()]);
        let sets = change_sets(&plan, &local, &resolved, &server, 500, None, None);
        let ops = match sets[0].0.get("ops") {
            Some(Value::Array(ops)) => ops.clone(),
            _ => panic!("ops"),
        };
        assert_eq!(ops[0].get("name"), Some(&Value::str("Bob")), "the name of the Basetool goes back");
        assert_eq!(ops[0].get("location"), Some(&value(r#"{"name": "Area18"}"#)), "only name and uex of the location");
        assert_eq!(ops[0].get("insurance"), Some(&value(r#"{"kind": "LTI"}"#)));
        assert_eq!(ops[0].get("version"), Some(&Value::int(3)));
    }
}
