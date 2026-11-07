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

//! The equipment of a ship in the shape of the exchange contract of the
//! Basetool (#2821): several named equipments per ship, one of them `active`
//! (the one built into the ship in the game), per slot the part, how it was
//! got and the quality of what was built.
//!
//! ⚠ 4.12 and the pages of v5 work on one `belegung` per ship (`path → {ref,
//! name, weg, erledigt}`, see `cart.rs`): the plan of the active equipment.
//! The equipments are stored next to it (`ausstattungen`); after every change
//! of the plan the active one is written anew ([`sync_entry`]), and a switch
//! to another one makes its plan the plan of the pages ([`switch`]). 4.12 has
//! no such thing, so there is no parity check; the tests pin the rules of the
//! contract.
//!
//! The rules of the contract that this file holds:
//! - the slot is the `portPath` (port names joined with `/`), case not
//!   significant: it is stored in lower case;
//! - per slot an `itemGuid` (or empty) and the way it was got (`acquisition`:
//!   `purchased`, `crafted`, `pledge`, `looted`, `unknown`);
//! - the optional `blueprintGuid` of a built part;
//! - the `quality` per recipe slot is 0 to 1000; 500 is the base value of Erkul
//!   ("not built") and is not stored: a missing value means 500;
//! - only the deviations from the stock loadout are stored; the full view is
//!   worked out from the stock and the deviations ([`resolve`]);
//! - a port that does not fit after a patch is marked ([`Check`]), not thrown
//!   away;
//! - what was changed in a slot on the way in (`corrected`: the value as it
//!   came and why) is kept as it came;
//! - the paint (`paintItemGuid`) and the power block (modes, segments per
//!   port, gimbal and fire mode, in the form of erkul) belong to the
//!   equipment; the paint is kept as an identifier, the block as it came.

use std::collections::BTreeMap;

use serde_json::{Map as JsonMap, Value as Json, json};

use crate::cart::{BUY, CRAFT};
use crate::hangar::{get, text, truthy};
use crate::jsonio::{Map, Value};

/// The quality nobody stores: Erkul's base value.
pub const BASE_QUALITY: u16 = 500;
/// The top of the scale.
pub const MAX_QUALITY: u16 = 1000;

/// How a part was got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquisition {
    /// Bought in a shop.
    Purchased,
    /// Built from a blueprint.
    Crafted,
    /// Came with a pledge.
    Pledge,
    /// Found or taken in the game.
    Looted,
    /// Not known.
    Unknown,
}

impl Acquisition {
    /// The word of the contract.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Purchased => "purchased",
            Self::Crafted => "crafted",
            Self::Pledge => "pledge",
            Self::Looted => "looted",
            Self::Unknown => "unknown",
        }
    }

    /// Reads the word of the contract; anything else is `Unknown`, so a word
    /// of a later version does not stop the whole equipment.
    pub fn parse(word: &str) -> Self {
        match word {
            "purchased" => Self::Purchased,
            "crafted" => Self::Crafted,
            "pledge" => Self::Pledge,
            "looted" => Self::Looted,
            _ => Self::Unknown,
        }
    }

    /// The way of the shopping list (`kaufen`, `bauen`) this stands for, if any.
    pub fn way(self) -> Option<&'static str> {
        match self {
            Self::Purchased => Some(BUY),
            Self::Crafted => Some(CRAFT),
            _ => None,
        }
    }

    /// The word of the contract for a way of the shopping list.
    pub fn from_way(way: &str) -> Self {
        match way {
            BUY => Self::Purchased,
            CRAFT => Self::Crafted,
            _ => Self::Unknown,
        }
    }
}

/// A value of a slot that was changed on the way in: what came, and why it was changed (`corrected`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    /// The value as it came.
    pub raw: String,
    /// Why it was changed.
    pub reason: String,
}

/// One slot of an equipment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The port path, lower case.
    pub port_path: String,
    /// The part (the entity of the game); empty for "nothing in it".
    pub item_guid: String,
    /// How it was got.
    pub acquisition: Acquisition,
    /// The blueprint a built part came from.
    pub blueprint_guid: Option<String>,
    /// The quality per recipe slot, only the values that are not 500.
    pub quality: BTreeMap<String, u16>,
    /// What was changed in the slot on the way in, kept as it came.
    pub corrected: Option<Correction>,
}

impl Slot {
    /// A slot with a part and nothing else known.
    pub fn new(path: &str, item_guid: &str, acquisition: Acquisition) -> Self {
        Self {
            port_path: port_path(path),
            item_guid: item_guid.to_string(),
            acquisition,
            blueprint_guid: None,
            quality: BTreeMap::new(),
            corrected: None,
        }
    }

    /// The quality at a recipe slot: 500 when none is stored.
    pub fn quality_at(&self, recipe_slot: &str) -> u16 {
        self.quality.get(recipe_slot).copied().unwrap_or(BASE_QUALITY)
    }

    /// Sets the quality at a recipe slot; false when the value is out of 0 to
    /// 1000. 500 takes the entry away, because it is not stored.
    pub fn set_quality(&mut self, recipe_slot: &str, value: u16) -> bool {
        if value > MAX_QUALITY || recipe_slot.is_empty() {
            return false;
        }
        if value == BASE_QUALITY {
            self.quality.remove(recipe_slot);
        } else {
            self.quality.insert(recipe_slot.to_string(), value);
        }
        true
    }
}

/// The way a port path is written: trimmed, in lower case.
pub fn port_path(path: &str) -> String {
    path.trim().to_lowercase()
}

/// One named equipment of a ship.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Equipment {
    /// The name the player gave it.
    pub name: String,
    /// The one built into the ship in the game.
    pub active: bool,
    /// The slots that differ from the stock loadout.
    pub slots: Vec<Slot>,
    /// The paint (`paintItemGuid`); the contract checks it against the paints of the ship (`paints.shipGuids`).
    pub paint_item_guid: Option<String>,
    /// Power distribution and modes (SCM/NAV, segments per port, gimbal and fire mode): an optional block in the
    /// form of erkul. It is kept as it came and handed back as it is; nothing here reads into it.
    pub power: Option<Json>,
}

impl Equipment {
    /// The slot at a port path (case not significant).
    pub fn slot(&self, path: &str) -> Option<&Slot> {
        let path = port_path(path);
        self.slots.iter().find(|s| s.port_path == path)
    }

    /// Puts a slot in; one at the same port path is replaced.
    pub fn put(&mut self, slot: Slot) {
        match self.slots.iter_mut().find(|s| s.port_path == slot.port_path) {
            Some(old) => *old = slot,
            None => self.slots.push(slot),
        }
    }
}

/// All equipments of a ship; exactly one is active as soon as there is one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Equipments(pub Vec<Equipment>);

impl Equipments {
    /// The equipments of a list read from outside: names trimmed, a second one of the same name dropped, and
    /// exactly one active (the first that says so, else the first).
    pub fn new(list: Vec<Equipment>) -> Equipments {
        let mut all = Equipments::default();
        let mut chosen: Option<String> = None;
        for equipment in list {
            let (wants_active, name) = (equipment.active, equipment.name.trim().to_string());
            if all.add(Equipment { active: false, ..equipment }) && wants_active && chosen.is_none() {
                chosen = Some(name);
            }
        }
        if let Some(name) = chosen {
            all.activate(&name);
        }
        all
    }

    /// Makes the one named equipment the active one; false when none has the name.
    pub fn activate(&mut self, name: &str) -> bool {
        let name = name.trim();
        if !self.0.iter().any(|e| e.name == name) {
            return false;
        }
        for equipment in &mut self.0 {
            equipment.active = equipment.name == name;
        }
        true
    }

    /// The active equipment.
    pub fn active(&self) -> Option<&Equipment> {
        self.0.iter().find(|e| e.active)
    }

    /// Adds an equipment; a second one with the name is refused. The first
    /// equipment of a ship becomes the active one.
    pub fn add(&mut self, mut equipment: Equipment) -> bool {
        equipment.name = equipment.name.trim().to_string();
        if equipment.name.is_empty() || self.0.iter().any(|e| e.name == equipment.name) {
            return false;
        }
        equipment.active = equipment.active || self.0.is_empty();
        let name = equipment.name.clone();
        let wants_active = equipment.active;
        self.0.push(equipment);
        if wants_active {
            self.activate(&name);
        }
        true
    }

    /// Takes one away. The active one goes only when no other is left, so a
    /// ship with equipments always has an active one.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(index) = self.0.iter().position(|e| e.name == name) else { return false };
        if self.0[index].active && self.0.len() > 1 {
            return false;
        }
        self.0.remove(index);
        true
    }
}

/// The equipment the `belegung` of a ship stands for. The identifier is the
/// content; the name of the part and the tick are not part of the contract.
///
/// Only deviations are kept: a slot that holds the part the stock loadout has is left out, unless it is
/// built (`bauen`): a built part of the stock's kind is a deviation, because its quality hangs on the slot.
pub fn from_loadout(entry: &Map, name: &str, stock: &[(String, String)]) -> Equipment {
    let plan = match get(entry, "belegung") {
        Some(Value::Object(m)) => m.as_slice(),
        _ => &[],
    };
    let stock: BTreeMap<String, &str> = stock.iter().map(|(p, g)| (port_path(p), g.as_str())).collect();
    let mut slots = Vec::new();
    for (path, value) in plan {
        let Some(part) = value.as_object() else { continue };
        let reference = text(part, "ref");
        if reference.is_empty() {
            // A slot left empty is a deviation where the stock has a part there: the port is emptied.
            if crate::cart::is_emptied(part) && stock.contains_key(&port_path(path)) {
                slots.push(Slot::new(path, "", Acquisition::Unknown));
            }
            continue;
        }
        let slot = Slot::new(path, reference, Acquisition::from_way(text(part, "weg")));
        if stock.get(&slot.port_path).copied() == Some(reference) && slot.acquisition != Acquisition::Crafted {
            continue;
        }
        slots.push(slot);
    }
    Equipment { name: name.to_string(), active: true, slots, ..Equipment::default() }
}

/// The `belegung` of an equipment, for the pages: the name of a part comes from `name_of`.
///
/// ⚠ The pages look a slot up by the exact path of the erkul data (`hardpoint_Left_Pylon_03`), the equipment
/// holds it in lower case. `exact` lists those paths, a slot goes back under its exact spelling. `previous` is the
/// `belegung` as it was: the tick (`erledigt`) stays where the part stayed, and a slot got by pledge, loot or in
/// an unknown way keeps the way (`kaufen`/`bauen`) it had, and is bought when it had none.
pub fn to_loadout(equipment: &Equipment, name_of: &dyn Fn(&str) -> String, previous: &Map, exact: &[String]) -> Map {
    let before = |path: &str| -> Option<&Map> {
        previous.iter().find(|(k, _)| port_path(k) == path).and_then(|(_, v)| v.as_object())
    };
    let mut plan = Map::new();
    for slot in &equipment.slots {
        let key = exact.iter().find(|p| port_path(p) == slot.port_path).unwrap_or(&slot.port_path);
        if slot.item_guid.is_empty() {
            // An emptied port stays empty in the plan: without the mark the stock part would come back.
            plan.push((key.clone(), crate::cart::emptied_part()));
            continue;
        }
        let old = before(&slot.port_path);
        let way = slot
            .acquisition
            .way()
            .or_else(|| old.map(|o| text(o, "weg")).filter(|w| *w == BUY || *w == CRAFT))
            .unwrap_or(BUY);
        let mut part = vec![
            ("ref".to_string(), Value::str(slot.item_guid.as_str())),
            ("name".to_string(), Value::str(name_of(&slot.item_guid))),
            ("weg".to_string(), Value::str(way)),
        ];
        if old.is_some_and(|o| text(o, "ref") == slot.item_guid && truthy(get(o, "erledigt"))) {
            part.push(("erledigt".to_string(), Value::Bool(true)));
        }
        plan.push((key.clone(), Value::Object(part)));
    }
    plan
}

/// The full view: the stock loadout (`port path → part`) with the deviations of
/// the equipment put over it. A slot of the equipment without a part empties
/// the port.
pub fn resolve(stock: &[(String, String)], equipment: &Equipment) -> Vec<(String, String)> {
    let mut full: Vec<(String, String)> = stock.iter().map(|(p, g)| (port_path(p), g.clone())).collect();
    for slot in &equipment.slots {
        match full.iter_mut().find(|(p, _)| *p == slot.port_path) {
            Some((_, guid)) => guid.clone_from(&slot.item_guid),
            None => full.push((slot.port_path.clone(), slot.item_guid.clone())),
        }
    }
    full.retain(|(_, guid)| !guid.is_empty());
    full
}

/// What is stored of a full view (all slots with what is known of them): only the slots that differ from the
/// stock, and an emptied port of the stock as a slot without a part. A slot with the stock's part stays when it
/// was built, has a blueprint, a quality that is not 500 or a correction: those hang on the slot, not on the part.
pub fn deviations(stock: &[(String, String)], full: &[Slot]) -> Vec<Slot> {
    let stock: BTreeMap<String, &str> = stock.iter().map(|(p, g)| (port_path(p), g.as_str())).collect();
    let mut slots = Vec::new();
    for slot in full {
        let same = stock.get(&slot.port_path).copied() == Some(slot.item_guid.as_str());
        let more = slot.acquisition == Acquisition::Crafted
            || slot.blueprint_guid.is_some()
            || !slot.quality.is_empty()
            || slot.corrected.is_some();
        if !same || more {
            slots.push(slot.clone());
        }
    }
    for path in stock.keys() {
        if !full.iter().any(|s| s.port_path == *path) {
            slots.push(Slot::new(path, "", Acquisition::Unknown));
        }
    }
    slots
}

/// What a check of a slot found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The port is not in the ship any more (after a patch).
    PortGone,
    /// The part does not fit the port any more.
    NoFit,
    /// The blueprint does not produce this part.
    BlueprintMismatch,
}

/// Marks what does not hold after a patch: the port is gone, the part does not
/// fit it, or the blueprint is for another part. Nothing is dropped.
///
/// `has_port` says whether the ship has the port, `fits` whether the part fits
/// it, `produces` what a blueprint makes.
pub fn check(
    equipment: &Equipment,
    has_port: &dyn Fn(&str) -> bool,
    fits: &dyn Fn(&str, &str) -> bool,
    produces: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, Check)> {
    let mut found = Vec::new();
    for slot in &equipment.slots {
        if !has_port(&slot.port_path) {
            found.push((slot.port_path.clone(), Check::PortGone));
            continue;
        }
        if !slot.item_guid.is_empty() && !fits(&slot.port_path, &slot.item_guid) {
            found.push((slot.port_path.clone(), Check::NoFit));
            continue;
        }
        // A blueprint the catalog does not know cannot be checked: that is no mismatch.
        if let Some(blueprint) = &slot.blueprint_guid
            && produces(blueprint).is_some_and(|made| made != slot.item_guid)
        {
            found.push((slot.port_path.clone(), Check::BlueprintMismatch));
        }
    }
    found
}

/// The key in the entry of a ship (or a wish) that holds the equipments in the form of the exchange.
pub const KEY: &str = "ausstattungen";
/// The name of the equipment that is made from a plain `belegung`.
pub const DEFAULT_NAME: &str = "Standard";

fn as_json(value: &Value) -> Json {
    serde_json::from_str(&crate::jsonio::dumps(value, None, false)).unwrap_or(Json::Null)
}

fn as_value(json: &Json) -> Value {
    crate::jsonio::loads(&json.to_string()).unwrap_or(Value::Null)
}

/// An equipment written over its stored form: the keys the model does not know (version, origin, state of the
/// game) stay with it, and with each slot those of the slot at the same port path.
fn merged(equipment: &Equipment, before: &Value) -> Value {
    let mut written = as_value(&to_json(equipment));
    if let (Value::Object(new), Value::Object(before)) = (&mut written, before) {
        keep_unknown(new, before);
    }
    written
}

/// Puts the keys of `before` the model does not know into `new`, for the equipment and for each slot that still
/// holds the same part.
fn keep_unknown(new: &mut Map, before: &Map) {
    const SLOT_KEYS: [&str; 6] = ["portPath", "itemGuid", "acquisition", "blueprintGuid", "quality", "corrected"];
    for (key, value) in before {
        if !["name", "active", "slots", "paintItemGuid", "power"].contains(&key.as_str()) {
            new.push((key.clone(), value.clone()));
        }
    }
    let old_slots = match get(before, "slots") {
        Some(Value::Array(slots)) => slots.as_slice(),
        _ => &[],
    };
    let Some((_, Value::Array(slots))) = new.iter_mut().find(|(k, _)| k == "slots") else { return };
    for slot in slots.iter_mut() {
        let Value::Object(fields) = slot else { continue };
        // The keys belong to the part: another part at the port does not take them over.
        let (path, part) = (port_path(text(fields, "portPath")), text(fields, "itemGuid").to_string());
        let Some(Value::Object(old)) = old_slots.iter().find(
            |s| matches!(s, Value::Object(o) if port_path(text(o, "portPath")) == path && text(o, "itemGuid") == part),
        ) else {
            continue;
        };
        for (key, value) in old {
            if !SLOT_KEYS.contains(&key.as_str()) {
                fields.push((key.clone(), value.clone()));
            }
        }
    }
}

/// The equipments stored in the entry (`ausstattungen`); an entry that has none yet gives the one its old
/// `belegung` stands for, so a file of 4.12 is read as before. Nothing in the list that cannot be read is
/// let through: it is left out, not guessed.
pub fn read_all(entry: &Map, stock: &[(String, String)]) -> Equipments {
    let stored: Vec<Equipment> = match get(entry, KEY) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| from_json(&as_json(v)).ok()).collect(),
        _ => Vec::new(),
    };
    if !stored.is_empty() {
        return Equipments::new(stored);
    }
    let fresh = from_loadout(entry, DEFAULT_NAME, stock);
    if fresh.slots.is_empty() { Equipments::default() } else { Equipments::new(vec![fresh]) }
}

/// Stores the active equipment in the new form (`ausstattungen`) after the page changed the `belegung`, which
/// the pages still work on. What the `belegung` cannot say stays from the stored one when the slot still holds
/// the same part: the quality per recipe slot, the blueprint, the correction, and a way got by pledge, by loot or
/// not known (the plan says "buy" for those; [`set_acquisition`] changes the way), as do the paint
/// and the power block of the equipment. The other named equipments are not touched. True when the entry
/// changed. Nothing is written for a ship without any plan and without a stored equipment.
pub fn sync_entry(entry: &mut Map, stock: &[(String, String)]) -> bool {
    // ⚠ The other equipments, and any the model cannot read, are handed back as they are (raw): a slot with a
    // quality of 1001 in a named equipment must not make the whole equipment disappear. Only the active one is
    // written anew, and from it only what the plan can say; the keys the model does not know stay too.
    let raw_items: Vec<Value> = match get(entry, KEY) {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let parsed: Vec<Option<Equipment>> = raw_items.iter().map(|v| from_json(&as_json(v)).ok()).collect();
    let active_at = parsed
        .iter()
        .position(|p| p.as_ref().is_some_and(|e| e.active))
        .or_else(|| parsed.iter().position(Option::is_some));
    let old = active_at.and_then(|i| parsed[i].clone());
    let plan = get(entry, "belegung");
    // ⚠ No plan at all (the key is missing: a file written elsewhere) is not "everything back to the factory"
    // (an empty plan is): the stored slots stay until `restore_loadout` has made the plan from them.
    if !matches!(plan, Some(Value::Object(_))) && (old.as_ref().is_some_and(|o| !o.slots.is_empty()) || old.is_none()) {
        return false;
    }
    if old.is_none() && plan.is_none_or(|b| !truthy(Some(b))) {
        return false;
    }
    let name = old.as_ref().map_or(DEFAULT_NAME.to_string(), |o| o.name.clone());
    let mut fresh = from_loadout(entry, &name, stock);
    if let Some(old) = &old {
        for slot in &mut fresh.slots {
            let Some(before) = old.slot(&slot.port_path).filter(|b| b.item_guid == slot.item_guid) else { continue };
            // The quality and the blueprint belong to a built part: a slot turned to "buy" loses them.
            if slot.acquisition == Acquisition::Crafted {
                slot.quality.clone_from(&before.quality);
                slot.blueprint_guid.clone_from(&before.blueprint_guid);
            }
            // The plan knows only "buy" and "build": a way the player set besides those stays.
            if matches!(before.acquisition, Acquisition::Pledge | Acquisition::Looted | Acquisition::Unknown)
                && matches!(slot.acquisition, Acquisition::Purchased | Acquisition::Unknown)
            {
                slot.acquisition = before.acquisition;
            }
            slot.corrected.clone_from(&before.corrected);
        }
        // The stock part with a correction is no deviation the plan can say: it stays from the stored one.
        let stock_parts: BTreeMap<String, &str> = stock.iter().map(|(p, g)| (port_path(p), g.as_str())).collect();
        for before in &old.slots {
            if before.corrected.is_some()
                && fresh.slot(&before.port_path).is_none()
                && stock_parts.get(&before.port_path).copied() == Some(before.item_guid.as_str())
            {
                fresh.put(before.clone());
            }
        }
        fresh.paint_item_guid.clone_from(&old.paint_item_guid);
        fresh.power.clone_from(&old.power);
    }
    let mut items = raw_items;
    match active_at {
        Some(i) => items[i] = merged(&fresh, &items[i]),
        None => items.insert(0, as_value(&to_json(&fresh))),
    }
    // One active only: another one that says it is active is set back (its other keys stay).
    for (i, item) in items.iter_mut().enumerate() {
        if Some(i) == active_at || (active_at.is_none() && i == 0) {
            continue;
        }
        if let Value::Object(fields) = item
            && fields.iter().any(|(k, v)| k == "active" && matches!(v, Value::Bool(true)))
        {
            crate::jsonio::map_set(fields, "active", Value::Bool(false));
        }
    }
    let written = Value::Array(items);
    if get(entry, KEY).is_some_and(|before| crate::jsonio::py_eq_value(before, &written)) {
        return false;
    }
    crate::jsonio::map_set(entry, KEY, written);
    true
}

/// A file written by another place may hold the equipments and no `belegung`: the pages need it, so it is made
/// from the active equipment (the name of a part comes from `name_of`). True when it was made.
pub fn restore_loadout(entry: &mut Map, name_of: &dyn Fn(&str) -> String, exact: &[String]) -> bool {
    // Only where the key is missing: an empty plan is the player's "everything back to the factory".
    if matches!(get(entry, "belegung"), Some(Value::Object(_))) {
        return false;
    }
    let Some(active) = read_all(entry, &[]).active().cloned() else { return false };
    let plan = to_loadout(&active, name_of, &Map::new(), exact);
    if plan.is_empty() {
        return false;
    }
    crate::jsonio::map_set(entry, "belegung", Value::Object(plan));
    true
}

/// The equipments as stored, raw: what the model cannot read is handed on as it is.
fn stored(entry: &Map) -> Vec<Value> {
    match get(entry, KEY) {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// The name of a stored equipment, trimmed; empty when it has none.
fn item_name(item: &Value) -> String {
    match item {
        Value::Object(fields) => text(fields, "name").trim().to_string(),
        _ => String::new(),
    }
}

fn readable(item: &Value) -> bool {
    from_json(&as_json(item)).is_ok()
}

/// The stored equipment that counts as the active one: the first that says so, else the first the model reads.
fn active_index(items: &[Value]) -> Option<usize> {
    let parsed: Vec<Option<Equipment>> = items.iter().map(|v| from_json(&as_json(v)).ok()).collect();
    parsed.iter().position(|p| p.as_ref().is_some_and(|e| e.active)).or_else(|| parsed.iter().position(Option::is_some))
}

/// The key of a stored equipment that holds the port paths ticked off as fitted while another equipment is
/// active: the exchange has no word for the tick, which lives in the plan.
const FITTED_KEY: &str = "versekitFitted";

/// The port paths the plan ticks off as fitted.
fn fitted(entry: &Map) -> Vec<Value> {
    match get(entry, "belegung") {
        Some(Value::Object(plan)) => plan
            .iter()
            .filter(|(_, part)| part.as_object().is_some_and(|p| truthy(get(p, "erledigt"))))
            .map(|(path, _)| Value::str(port_path(path).as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// The ticks of the plan go with the stored equipment at `at`, the one the plan belongs to.
fn keep_fitted(entry: &Map, items: &mut [Value], at: usize) {
    if let Some(Value::Object(fields)) = items.get_mut(at) {
        crate::jsonio::map_set(fields, FITTED_KEY, Value::Array(fitted(entry)));
    }
}

/// The equipment at `at` becomes the active one and its plan the plan of the pages (see [`to_loadout`]), with
/// the ticks it had when it was active last. The ticks of the plan as it was go with the one active before.
fn activate_at(entry: &mut Map, mut items: Vec<Value>, at: usize, name_of: &dyn Fn(&str) -> String, exact: &[String]) {
    let flagged = |item: &Value| matches!(item, Value::Object(f) if f.iter().any(|(k, v)| k == "active" && matches!(v, Value::Bool(true))));
    if let Some(before) = (0..items.len()).find(|i| *i != at && flagged(&items[*i])) {
        keep_fitted(entry, &mut items, before);
    }
    for (i, item) in items.iter_mut().enumerate() {
        if let Value::Object(fields) = item {
            let on = fields.iter().any(|(k, v)| k == "active" && matches!(v, Value::Bool(true)));
            if i == at || on {
                crate::jsonio::map_set(fields, "active", Value::Bool(i == at));
            }
        }
    }
    let equipment = from_json(&as_json(&items[at])).unwrap_or_default();
    // The plan as it was belongs to the other equipment: neither its ways nor its ticks are this one's.
    let mut plan = to_loadout(&equipment, name_of, &Map::new(), exact);
    let ticked: Vec<String> = match &items[at] {
        Value::Object(fields) => match get(fields, FITTED_KEY) {
            Some(Value::Array(paths)) => paths.iter().filter_map(|p| p.as_str().map(port_path)).collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    for (path, part) in &mut plan {
        if let Value::Object(fields) = part
            && ticked.contains(&port_path(path))
        {
            crate::jsonio::map_set(fields, "erledigt", Value::Bool(true));
        }
    }
    crate::jsonio::map_set(entry, KEY, Value::Array(items));
    crate::jsonio::map_set(entry, "belegung", Value::Object(plan));
}

/// The names of the equipments the model reads, in their order, and the position of the active one.
pub fn names(entry: &Map, stock: &[(String, String)]) -> (Vec<String>, Option<usize>) {
    let all = read_all(entry, stock);
    (all.0.iter().map(|e| e.name.clone()).collect(), all.0.iter().position(|e| e.active))
}

/// Makes sure the active equipment is stored: from the plan, or under the default name for a ship without any
/// plan. True when the entry changed.
pub fn ensure_stored(entry: &mut Map, stock: &[(String, String)]) -> bool {
    let mut changed = sync_entry(entry, stock);
    let mut items = stored(entry);
    if !items.iter().any(readable) {
        items.insert(0, as_value(&to_json(&from_loadout(entry, DEFAULT_NAME, stock))));
        crate::jsonio::map_set(entry, KEY, Value::Array(items));
        changed = true;
    }
    changed
}

/// Gives an equipment another name. Refused (false) for an empty name, a name an equipment has already, or
/// when there is no equipment of the old name.
pub fn rename(entry: &mut Map, old: &str, new: &str) -> bool {
    let (old, new) = (old.trim(), new.trim());
    let mut items = stored(entry);
    if new.is_empty() || items.iter().any(|i| item_name(i) == new) {
        return false;
    }
    let Some(Value::Object(fields)) = items.iter_mut().find(|i| item_name(i) == old) else { return false };
    crate::jsonio::map_set(fields, "name", Value::str(new));
    crate::jsonio::map_set(entry, KEY, Value::Array(items));
    true
}

/// A new equipment under `name`, a copy of the active one (slots, paint, power), and the active one from now on.
/// The plan stays as it is: the copy holds the same. Refused for an empty name or one an equipment has already.
pub fn create(entry: &mut Map, name: &str, stock: &[(String, String)]) -> bool {
    let name = name.trim();
    if name.is_empty() || stored(entry).iter().any(|i| item_name(i) == name) {
        return false;
    }
    ensure_stored(entry, stock);
    let mut items = stored(entry);
    let before = active_index(&items);
    let mut copy = before.and_then(|at| from_json(&as_json(&items[at])).ok()).unwrap_or_default();
    if let Some(at) = before {
        keep_fitted(entry, &mut items, at);
    }
    copy.name = name.to_string();
    copy.active = true;
    items.push(as_value(&to_json(&copy)));
    let at = items.len() - 1;
    for (i, item) in items.iter_mut().enumerate() {
        if let Value::Object(fields) = item
            && i != at
            && fields.iter().any(|(k, v)| k == "active" && matches!(v, Value::Bool(true)))
        {
            crate::jsonio::map_set(fields, "active", Value::Bool(false));
        }
    }
    crate::jsonio::map_set(entry, KEY, Value::Array(items));
    true
}

/// Makes the named equipment the active one and its plan the plan of the pages; the plan as it was is stored in
/// the one that was active before. False when there is no such equipment or it is the active one already.
pub fn switch(
    entry: &mut Map,
    name: &str,
    stock: &[(String, String)],
    name_of: &dyn Fn(&str) -> String,
    exact: &[String],
) -> bool {
    sync_entry(entry, stock);
    let items = stored(entry);
    let Some(at) = items.iter().position(|i| item_name(i) == name.trim() && readable(i)) else { return false };
    if active_index(&items) == Some(at) {
        return false;
    }
    activate_at(entry, items, at, name_of, exact);
    true
}

/// What [`delete`] took away, to put it back with [`undo_delete`].
#[derive(Clone, Debug)]
pub struct Removed {
    /// Its place in the list.
    pub at: usize,
    /// The equipment as it was stored.
    pub item: Value,
    /// It was the active one.
    pub active: bool,
}

impl Removed {
    /// The name of the equipment taken away.
    pub fn name(&self) -> String {
        item_name(&self.item)
    }
}

/// Takes the named equipment away. When it was the active one, the first one left becomes active and its plan
/// the plan of the pages. The last equipment of a ship stays (`None`), as does a name no equipment has.
pub fn delete(
    entry: &mut Map,
    name: &str,
    stock: &[(String, String)],
    name_of: &dyn Fn(&str) -> String,
    exact: &[String],
) -> Option<Removed> {
    sync_entry(entry, stock);
    let mut items = stored(entry);
    let at = items.iter().position(|i| item_name(i) == name.trim() && readable(i))?;
    if items.iter().filter(|i| readable(i)).count() < 2 {
        return None;
    }
    let active = active_index(&items) == Some(at);
    if active {
        // Put back later, it brings its ticks along.
        keep_fitted(entry, &mut items, at);
    }
    let item = items.remove(at);
    if active {
        let next = items.iter().position(readable).unwrap_or(0);
        activate_at(entry, items, next, name_of, exact);
    } else {
        crate::jsonio::map_set(entry, KEY, Value::Array(items));
    }
    Some(Removed { at, item, active })
}

/// Puts back what [`delete`] took away, at its place; the active one is active again, with its plan. False when
/// another equipment has taken its name meanwhile.
pub fn undo_delete(
    entry: &mut Map,
    removed: &Removed,
    stock: &[(String, String)],
    name_of: &dyn Fn(&str) -> String,
    exact: &[String],
) -> bool {
    let name = item_name(&removed.item);
    if stored(entry).iter().any(|i| item_name(i) == name) {
        return false;
    }
    if removed.active {
        sync_entry(entry, stock);
    }
    let mut items = stored(entry);
    let at = removed.at.min(items.len());
    items.insert(at, removed.item.clone());
    if removed.active {
        activate_at(entry, items, at, name_of, exact);
    } else {
        crate::jsonio::map_set(entry, KEY, Value::Array(items));
    }
    true
}

/// The name a further equipment gets: `wanted` where no other one has it, else `wanted (2)`, `wanted (3)`, …;
/// an empty name stands for the default name.
pub fn unique_name(taken: &[String], wanted: &str) -> String {
    let wanted = wanted.trim();
    let base = if wanted.is_empty() { DEFAULT_NAME } else { wanted };
    if !taken.iter().any(|name| name == base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} ({n})")).find(|name| !taken.contains(name)).unwrap_or_default()
}

/// Puts `equipment` in as a further equipment of the ship, under a name no other one has, and makes it the
/// active one: its plan becomes the plan of the pages, the plan as it was stays with the one active before.
/// The name it got.
pub fn add_as_active(
    entry: &mut Map,
    equipment: &Equipment,
    stock: &[(String, String)],
    name_of: &dyn Fn(&str) -> String,
    exact: &[String],
) -> String {
    ensure_stored(entry, stock);
    let mut items = stored(entry);
    let taken: Vec<String> = items.iter().map(item_name).collect();
    let name = unique_name(&taken, &equipment.name);
    let copy = Equipment { name: name.clone(), active: false, ..equipment.clone() };
    items.push(as_value(&to_json(&copy)));
    crate::jsonio::map_set(entry, KEY, Value::Array(items));
    switch(entry, &name, stock, name_of, exact);
    name
}

/// Takes the equipments of another entry of the same ship in as further ones of `target`: inactive, under names
/// no other one has, with the ticks of the plan they had. The plan of `source` counts as its active equipment.
/// The active equipment and the plan of `target` stay. The number taken in.
pub fn take_in(target: &mut Map, source: &Map, stock: &[(String, String)]) -> usize {
    if read_all(source, stock).0.is_empty() {
        return 0;
    }
    let mut source = source.clone();
    ensure_stored(&mut source, stock);
    let mut brought = stored(&source);
    if let Some(at) = active_index(&brought) {
        keep_fitted(&source, &mut brought, at);
    }
    ensure_stored(target, stock);
    let mut items = stored(target);
    let mut count = 0;
    for item in brought {
        let Value::Object(mut fields) = item else { continue };
        if from_json(&as_json(&Value::Object(fields.clone()))).is_err() {
            continue;
        }
        let taken: Vec<String> = items.iter().map(item_name).collect();
        let name = unique_name(&taken, text(&fields, "name"));
        crate::jsonio::map_set(&mut fields, "name", Value::str(name));
        crate::jsonio::map_set(&mut fields, "active", Value::Bool(false));
        items.push(Value::Object(fields));
        count += 1;
    }
    crate::jsonio::map_set(target, KEY, Value::Array(items));
    count
}

/// Changes the slots of the active equipment at these paths; true when `edit` changed one.
fn edit_active(entry: &mut Map, paths: &[String], edit: &mut dyn FnMut(&mut Slot) -> bool) -> bool {
    let mut items = stored(entry);
    let Some(at) = active_index(&items) else { return false };
    let Ok(mut equipment) = from_json(&as_json(&items[at])) else { return false };
    let mut changed = false;
    for path in paths {
        let path = port_path(path);
        if let Some(slot) = equipment.slots.iter_mut().find(|s| s.port_path == path) {
            changed |= edit(slot);
        }
    }
    if changed {
        items[at] = merged(&equipment, &items[at]);
        crate::jsonio::map_set(entry, KEY, Value::Array(items));
    }
    changed
}

/// Sets how the parts in these slots were got. "Bought" and "built" are ways of the plan too and change it;
/// pledge, loot and "not known" are kept in the equipment, the plan says "buy" for them. A built part gets `blueprint` when it has none
/// yet and keeps its quality; a part got another way loses both. True when the entry changed.
pub fn set_acquisition(
    entry: &mut Map,
    paths: &[String],
    acquisition: Acquisition,
    blueprint: Option<&str>,
    stock: &[(String, String)],
) -> bool {
    // Pledge, loot and "not known" stand as "buy" in the plan: only then the stored way outlives a sync.
    let way = acquisition.way().unwrap_or(BUY);
    let mut changed = false;
    for path in paths {
        changed |= crate::cart::set_method(entry, path, way);
    }
    changed |= sync_entry(entry, stock);
    changed |= edit_active(entry, paths, &mut |slot| {
        let before = slot.clone();
        slot.acquisition = acquisition;
        if acquisition == Acquisition::Crafted {
            if slot.blueprint_guid.is_none() {
                slot.blueprint_guid = blueprint.filter(|b| !b.is_empty()).map(str::to_string);
            }
        } else {
            slot.blueprint_guid = None;
            slot.quality.clear();
        }
        *slot != before
    });
    changed
}

/// Sets the quality of the built parts in these slots at a recipe slot (0 to 1000; 500 is not stored). True
/// when the entry changed.
pub fn set_quality(
    entry: &mut Map,
    paths: &[String],
    recipe_slot: &str,
    value: u16,
    stock: &[(String, String)],
) -> bool {
    let synced = sync_entry(entry, stock);
    let edited = edit_active(entry, paths, &mut |slot| {
        slot.acquisition == Acquisition::Crafted
            && slot.quality_at(recipe_slot) != value
            && slot.set_quality(recipe_slot, value)
    });
    synced || edited
}

/// The equipment in the form of the exchange: camel case keys, the quality as
/// an object per recipe slot, left out when empty.
pub fn to_json(equipment: &Equipment) -> Json {
    let slots: Vec<Json> = equipment
        .slots
        .iter()
        .map(|slot| {
            let mut m = JsonMap::new();
            m.insert("portPath".into(), json!(slot.port_path));
            m.insert("itemGuid".into(), json!(slot.item_guid));
            m.insert("acquisition".into(), json!(slot.acquisition.as_str()));
            if let Some(blueprint) = &slot.blueprint_guid {
                m.insert("blueprintGuid".into(), json!(blueprint));
            }
            if !slot.quality.is_empty() {
                m.insert("quality".into(), json!(slot.quality));
            }
            if let Some(corrected) = &slot.corrected {
                m.insert("corrected".into(), json!({"raw": corrected.raw, "reason": corrected.reason}));
            }
            Json::Object(m)
        })
        .collect();
    let mut out = JsonMap::new();
    out.insert("name".into(), json!(equipment.name));
    out.insert("active".into(), json!(equipment.active));
    out.insert("slots".into(), Json::Array(slots));
    if let Some(paint) = &equipment.paint_item_guid {
        out.insert("paintItemGuid".into(), json!(paint));
    }
    if let Some(power) = &equipment.power {
        out.insert("power".into(), power.clone());
    }
    Json::Object(out)
}

/// Reads an equipment of the exchange. A port path is put into lower case, a
/// quality of 500 is dropped (it is not stored), a word of the way the contract
/// does not know becomes `unknown`. Refused (`Err` with the reason): no name,
/// a slot without a port path, a quality that is not a whole number from 0 to
/// 1000.
pub fn from_json(value: &Json) -> Result<Equipment, String> {
    let name = value["name"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err("name".to_string());
    }
    let mut equipment = Equipment {
        name,
        active: value["active"].as_bool().unwrap_or(false),
        paint_item_guid: value["paintItemGuid"].as_str().filter(|p| !p.is_empty()).map(str::to_string),
        power: value.get("power").filter(|p| p.is_object()).cloned(),
        ..Equipment::default()
    };
    for raw in value["slots"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let path = port_path(raw["portPath"].as_str().unwrap_or(""));
        if path.is_empty() {
            return Err("portPath".to_string());
        }
        let mut slot = Slot::new(
            &path,
            raw["itemGuid"].as_str().unwrap_or(""),
            Acquisition::parse(raw["acquisition"].as_str().unwrap_or("")),
        );
        slot.blueprint_guid = raw["blueprintGuid"].as_str().filter(|b| !b.is_empty()).map(str::to_string);
        if let Some(corrected) = raw["corrected"].as_object() {
            let word = |key: &str| corrected.get(key).and_then(Json::as_str).unwrap_or("").to_string();
            let (raw_value, reason) = (word("raw"), word("reason"));
            if !raw_value.is_empty() || !reason.is_empty() {
                slot.corrected = Some(Correction { raw: raw_value, reason });
            }
        }
        if let Some(quality) = raw["quality"].as_object() {
            for (recipe_slot, number) in quality {
                // A whole number, also written as `800.0`.
                let whole = number.as_u64().or_else(|| {
                    number
                        .as_f64()
                        .filter(|f| f.fract() == 0.0 && (0.0..=f64::from(u16::MAX)).contains(f))
                        .map(|f| f as u64)
                });
                let value =
                    whole.and_then(|n| u16::try_from(n).ok()).ok_or_else(|| format!("quality {recipe_slot}"))?;
                if !slot.set_quality(recipe_slot, value) {
                    return Err(format!("quality {recipe_slot}"));
                }
            }
        }
        equipment.put(slot);
    }
    Ok(equipment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn stock() -> Vec<(String, String)> {
        vec![
            ("hardpoint_power_plant".to_string(), "pp-stock".to_string()),
            ("Hardpoint_Cooler_Left".to_string(), "cool-stock".to_string()),
        ]
    }

    #[test]
    fn a_port_path_is_kept_in_lower_case_and_found_in_any_case() {
        let mut eq = Equipment { name: "Mining".into(), active: true, ..Equipment::default() };
        eq.put(Slot::new(" Hardpoint_Cooler/Missile_01 ", "m1", Acquisition::Purchased));
        assert_eq!(eq.slots[0].port_path, "hardpoint_cooler/missile_01");
        assert!(eq.slot("HARDPOINT_COOLER/MISSILE_01").is_some());
        eq.put(Slot::new("hardpoint_cooler/missile_01", "m2", Acquisition::Crafted));
        assert_eq!(eq.slots.len(), 1, "the same port path is one slot");
        assert_eq!(eq.slots[0].item_guid, "m2");
    }

    #[test]
    fn the_base_quality_is_not_stored_and_a_missing_one_counts_500() {
        let mut slot = Slot::new("p", "i", Acquisition::Crafted);
        assert_eq!(slot.quality_at("barrel"), 500);
        assert!(slot.set_quality("barrel", 730));
        assert_eq!(slot.quality_at("barrel"), 730);
        assert!(slot.set_quality("barrel", 500));
        assert!(slot.quality.is_empty(), "500 is not stored");
        assert!(!slot.set_quality("barrel", 1001), "above 1000 is refused");
        assert!(slot.set_quality("barrel", 0) && slot.set_quality("barrel", 1000));
    }

    #[test]
    fn exactly_one_equipment_is_active_and_the_last_one_may_go() {
        let mut all = Equipments::default();
        let named = |name: &str, active: bool| Equipment { name: name.into(), active, ..Equipment::default() };
        assert!(all.add(named("A", false)));
        assert!(all.active().is_some_and(|e| e.name == "A"), "the first one becomes active");
        assert!(!all.add(named("A", false)), "a name once");
        assert!(all.add(named("B", true)));
        assert_eq!(all.0.iter().filter(|e| e.active).count(), 1);
        assert!(all.active().is_some_and(|e| e.name == "B"));
        assert!(!all.remove("B"), "the active one stays while others are left");
        assert!(all.activate("A") && all.remove("B"));
        assert!(all.remove("A") && all.0.is_empty());
        assert!(!all.activate("A"));
    }

    #[test]
    fn only_the_deviations_from_the_stock_are_stored_and_the_full_view_comes_back() {
        let full = vec![
            Slot::new("hardpoint_power_plant", "pp-new", Acquisition::Purchased),
            Slot::new("hardpoint_shield", "sh-new", Acquisition::Purchased),
        ];
        let slots = deviations(&stock(), &full);
        let paths: Vec<_> = slots.iter().map(|s| (s.port_path.as_str(), s.item_guid.as_str())).collect();
        assert_eq!(
            paths,
            vec![("hardpoint_power_plant", "pp-new"), ("hardpoint_shield", "sh-new"), ("hardpoint_cooler_left", "")]
        );
        let eq = Equipment { name: "x".into(), active: true, slots, ..Equipment::default() };
        let back = resolve(&stock(), &eq);
        let mut want: Vec<(String, String)> =
            vec![("hardpoint_power_plant".into(), "pp-new".into()), ("hardpoint_shield".into(), "sh-new".into())];
        want.sort();
        let mut got = back;
        got.sort();
        assert_eq!(got, want, "an emptied port stays empty, a new one is there");

        // The stock's own part stays a deviation when it was built, has a blueprint or a quality: the quality
        // hangs on the slot. The same part bought is none.
        let mut built = Slot::new("hardpoint_power_plant", "pp-stock", Acquisition::Crafted);
        built.set_quality("core", 800);
        let bought = Slot::new("Hardpoint_Cooler_Left", "cool-stock", Acquisition::Purchased);
        let kept = deviations(&stock(), &[built, bought]);
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].port_path.as_str(), kept[0].quality_at("core")), ("hardpoint_power_plant", 800));
    }

    #[test]
    fn the_loadout_of_a_ship_becomes_an_equipment_and_back() {
        let entry = loads(
            r#"{"belegung": {"Cooler": {"ref": "c1", "name": "Cool", "weg": "bauen", "erledigt": true},
                 "shield": {"ref": "s1", "name": "Shield", "weg": "kaufen"}, "empty": {"ref": "", "name": "", "weg": "kaufen"}}}"#,
        )
        .unwrap();
        let entry = entry.as_object().unwrap();
        let eq = from_loadout(entry, "Active", &[]);
        assert_eq!(eq.slots.len(), 2, "a slot without a part is not a slot");
        assert_eq!(eq.slot("cooler").unwrap().acquisition, Acquisition::Crafted);
        assert_eq!(eq.slot("shield").unwrap().acquisition, Acquisition::Purchased);
        // Back: under the exact spelling of the erkul path, with the tick that was there.
        let previous = entry.iter().find(|(k, _)| k == "belegung").and_then(|(_, v)| v.as_object()).unwrap();
        let exact = vec!["Cooler".to_string()];
        let plan = to_loadout(&eq, &|guid| format!("name of {guid}"), previous, &exact);
        assert_eq!(text(plan[0].1.as_object().unwrap(), "name"), "name of c1");
        assert_eq!(text(plan[0].1.as_object().unwrap(), "weg"), "bauen");
        assert_eq!(plan[0].0, "Cooler", "the pages look the slot up by the exact path");
        assert_eq!(plan[1].0, "shield", "a path erkul does not list stays as it is");
        assert!(truthy(get(plan[0].1.as_object().unwrap(), "erledigt")), "the tick stays with the part");
        assert!(!truthy(get(plan[1].1.as_object().unwrap(), "erledigt")));
        // A slot got by pledge keeps the way it had; a part that changed loses the tick.
        let mut pledge = eq.clone();
        pledge.slots[0].acquisition = Acquisition::Pledge;
        pledge.slots[1].item_guid = "s2".into();
        let plan = to_loadout(&pledge, &|g| g.to_string(), previous, &exact);
        assert_eq!(text(plan[0].1.as_object().unwrap(), "weg"), "bauen");
        assert!(!truthy(get(plan[1].1.as_object().unwrap(), "erledigt")));
        // The stock's own part is no deviation.
        // ... but a built part of the stock's kind stays: its quality hangs on the slot.
        let stock = vec![("Shield".to_string(), "s1".to_string()), ("Cooler".to_string(), "c1".to_string())];
        let slim = from_loadout(entry, "Active", &stock);
        assert_eq!(slim.slots.len(), 1, "the bought stock part is left out");
        assert_eq!(slim.slots[0].port_path, "cooler", "the built one of the stock's kind stays");
    }

    #[test]
    fn equipments_read_from_outside_have_trimmed_names_and_exactly_one_active() {
        let named = |name: &str, active: bool| Equipment { name: name.into(), active, ..Equipment::default() };
        let all = Equipments::new(vec![named("A", false), named(" A ", true), named("B", true), named("C", true)]);
        assert_eq!(all.0.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
        assert_eq!(all.0.iter().filter(|e| e.active).count(), 1);
        assert_eq!(all.active().map(|e| e.name.as_str()), Some("B"), "the first that says so");
        let none = Equipments::new(vec![named("A", false), named("B", false)]);
        assert_eq!(none.active().map(|e| e.name.as_str()), Some("A"));
        let mut padded = Equipments::default();
        assert!(padded.add(named("X ", false)) && !padded.add(named("X", false)));
        assert!(padded.activate(" X"));
    }

    #[test]
    fn a_part_that_no_longer_fits_is_marked_and_not_thrown_away() {
        let mut crafted = Slot::new("a", "gun", Acquisition::Crafted);
        crafted.blueprint_guid = Some("bp-gun".to_string());
        let mut wrong = Slot::new("b", "gun", Acquisition::Crafted);
        wrong.blueprint_guid = Some("bp-other".to_string());
        let eq = Equipment {
            name: "x".into(),
            active: true,
            slots: vec![
                crafted,
                wrong,
                Slot::new("gone", "g", Acquisition::Purchased),
                Slot::new("c", "big", Acquisition::Purchased),
            ],
            ..Equipment::default()
        };
        let mut unknown = Slot::new("d", "gun", Acquisition::Crafted);
        unknown.blueprint_guid = Some("bp-not-in-the-catalog".to_string());
        let mut eq = eq;
        eq.slots.push(unknown);
        let found = check(&eq, &|p| p != "gone", &|_, g| g != "big", &|bp| match bp {
            "bp-gun" => Some("gun".to_string()),
            "bp-other" => Some("other".to_string()),
            _ => None,
        });
        assert_eq!(
            found,
            vec![
                ("b".to_string(), Check::BlueprintMismatch),
                ("gone".to_string(), Check::PortGone),
                ("c".to_string(), Check::NoFit)
            ]
        );
        assert_eq!(eq.slots.len(), 5, "nothing is dropped, and a blueprint the catalog lacks is not a mismatch");
    }

    #[test]
    fn the_exchange_form_round_trips_and_refuses_what_the_contract_refuses() {
        let raw = json!({
            "name": "Mining", "active": true,
            "slots": [
                {"portPath": "Hardpoint_Laser", "itemGuid": "l1", "acquisition": "crafted",
                 "blueprintGuid": "bp1", "quality": {"barrel": 800, "core": 500}},
                {"portPath": "hardpoint_x", "itemGuid": "", "acquisition": "somethingnew"}
            ]
        });
        let eq = from_json(&raw).unwrap();
        assert_eq!(eq.slots[0].port_path, "hardpoint_laser");
        assert_eq!(eq.slots[0].quality.len(), 1, "500 is not stored");
        assert_eq!(eq.slots[1].acquisition, Acquisition::Unknown);
        let again = to_json(&eq);
        assert_eq!(again["slots"][0]["quality"], json!({"barrel": 800}));
        assert_eq!(again["slots"][0]["acquisition"], json!("crafted"));
        assert!(again["slots"][1].get("quality").is_none() && again["slots"][1].get("blueprintGuid").is_none());
        assert_eq!(from_json(&again).unwrap(), eq);

        assert!(from_json(&json!({"name": " ", "slots": []})).is_err());
        assert!(from_json(&json!({"name": "x", "slots": [{"portPath": ""}]})).is_err());
        let too_high = json!({"name": "x", "slots": [{"portPath": "p", "quality": {"a": 1001}}]});
        assert_eq!(from_json(&too_high), Err("quality a".to_string()));
        let negative = json!({"name": "x", "slots": [{"portPath": "p", "quality": {"a": -1}}]});
        assert!(from_json(&negative).is_err());
    }

    fn entry_of(text: &str) -> Map {
        loads(text).unwrap().as_object().unwrap().clone()
    }

    #[test]
    fn a_file_of_4_12_is_read_as_one_equipment_and_the_change_of_the_plan_is_stored_in_the_new_form() {
        let stock = vec![("cooler_1".to_string(), "f-cooler".to_string())];
        // Only the old `belegung`: read as the one equipment; nothing is stored until something changes.
        let mut ship = entry_of(
            r#"{"name": "Test", "belegung": {"Cooler_1": {"ref": "c9", "name": "C9", "weg": "bauen", "erledigt": true},
                "shield_1": {"ref": "f-shield", "name": "S", "weg": "kaufen"}}}"#,
        );
        let read = read_all(&ship, &[("shield_1".to_string(), "f-shield".to_string())]);
        assert_eq!(read.0.len(), 1);
        assert_eq!(read.active().unwrap().name, DEFAULT_NAME);
        assert_eq!(read.active().unwrap().slots.len(), 1, "the stock's own bought part is no deviation");
        assert!(get(&ship, KEY).is_none());
        // The first change writes the new form, next to the old `belegung`, which stays.
        assert!(sync_entry(&mut ship, &stock));
        assert!(get(&ship, "belegung").is_some());
        let stored = as_json(get(&ship, KEY).unwrap());
        assert_eq!(stored[0]["slots"][0]["portPath"], json!("cooler_1"));
        assert_eq!(stored[0]["slots"][0]["acquisition"], json!("crafted"));
        assert_eq!(stored[0]["active"], json!(true));
        assert!(!sync_entry(&mut ship, &stock), "nothing changed, nothing written");
        // What the plan cannot say stays while the slot holds the same part; another part loses it.
        let mut with_quality = read_all(&ship, &stock);
        let active = with_quality.0.iter_mut().find(|e| e.active).unwrap();
        active.slots[0].set_quality("barrel", 800);
        active.slots[0].blueprint_guid = Some("bp-9".into());
        active.paint_item_guid = Some("paint-1".into());
        active.power = Some(json!({"modes": {"scm": true}}));
        crate::jsonio::map_set(
            &mut ship,
            KEY,
            Value::Array(with_quality.0.iter().map(|e| as_value(&to_json(e))).collect()),
        );
        crate::jsonio::map_set(&mut ship, "name", Value::str("Test renamed"));
        assert!(!sync_entry(&mut ship, &stock), "the plan did not change, so the quality stays and nothing is written");
        let kept = read_all(&ship, &stock);
        assert_eq!(kept.active().unwrap().slots[0].quality_at("barrel"), 800);
        assert_eq!(kept.active().unwrap().paint_item_guid.as_deref(), Some("paint-1"));
        assert!(kept.active().unwrap().power.is_some());
        if let Some((_, Value::Object(plan))) = ship.iter_mut().find(|(k, _)| k == "belegung") {
            crate::jsonio::map_set(plan, "Cooler_1", as_value(&json!({"ref": "other", "name": "O", "weg": "kaufen"})));
        }
        assert!(sync_entry(&mut ship, &stock));
        let changed = read_all(&ship, &stock);
        assert_eq!(changed.active().unwrap().slots[0].quality_at("barrel"), 500, "another part, the quality is gone");
        assert!(changed.active().unwrap().paint_item_guid.is_some(), "the paint belongs to the equipment");
    }

    #[test]
    fn other_named_equipments_are_not_touched_and_a_file_without_a_loadout_is_restored() {
        let mut ship = entry_of(
            r#"{"name": "T", "belegung": {"a": {"ref": "x", "name": "X", "weg": "kaufen"}},
                "ausstattungen": [
                  {"name": "Mining", "active": false, "slots": [{"portPath": "m", "itemGuid": "g", "acquisition": "pledge"}]},
                  {"name": "Combat", "active": true, "slots": []}]}"#,
        );
        assert!(sync_entry(&mut ship, &[]));
        let all = read_all(&ship, &[]);
        assert_eq!(all.0.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Mining", "Combat"]);
        assert_eq!(all.active().unwrap().slots.len(), 1, "the active one follows the plan");
        assert_eq!(all.0[0].slots[0].acquisition, Acquisition::Pledge, "the other one is as it was");
        // A file written elsewhere: equipments, no loadout. The pages get theirs back.
        let mut elsewhere = entry_of(
            r#"{"name": "T", "ausstattungen": [{"name": "A", "active": true,
                "slots": [{"portPath": "hardpoint_cooler_1", "itemGuid": "g1", "acquisition": "crafted"}]}]}"#,
        );
        let exact = vec!["hardpoint_Cooler_1".to_string()];
        assert!(restore_loadout(&mut elsewhere, &|g| format!("name {g}"), &exact));
        let plan = get(&elsewhere, "belegung").and_then(Value::as_object).unwrap();
        assert_eq!(plan[0].0, "hardpoint_Cooler_1");
        assert_eq!(text(plan[0].1.as_object().unwrap(), "weg"), "bauen");
        assert!(!restore_loadout(&mut elsewhere, &|g| g.to_string(), &exact), "there is a loadout now");
        // A ship without a plan and without equipments gets nothing written.
        let mut bare = entry_of(r#"{"name": "T"}"#);
        assert!(!sync_entry(&mut bare, &[]) && get(&bare, KEY).is_none());
    }

    #[test]
    fn an_equipment_the_model_cannot_read_survives_a_sync_and_a_missing_plan_is_not_a_cleared_one() {
        // A named equipment with a quality of 1001 and keys the model does not know: handed back as it is.
        let mut ship = entry_of(
            r#"{"name": "T", "belegung": {"a": {"ref": "x", "name": "X", "weg": "bauen"}},
                "ausstattungen": [
                  {"name": "Mining", "active": false, "version": 7, "slots": [{"portPath": "m", "itemGuid": "g", "quality": {"b": 1001}}]},
                  {"name": "Main", "active": true, "version": 3, "origin": "erkul", "slots": []}]}"#,
        );
        let before_mining = as_json(
            &get(&ship, KEY).and_then(|v| if let Value::Array(a) = v { a.first().cloned() } else { None }).unwrap(),
        );
        assert!(sync_entry(&mut ship, &[]));
        let items = match get(&ship, KEY) {
            Some(Value::Array(a)) => a.clone(),
            _ => panic!("list"),
        };
        assert_eq!(as_json(&items[0]), before_mining, "the unreadable one is as it was");
        let main = as_json(&items[1]);
        assert_eq!((main["version"].clone(), main["origin"].clone()), (json!(3), json!("erkul")), "unknown keys stay");
        assert_eq!(main["slots"][0]["portPath"], json!("a"));
        // A quality and a blueprint belong to a built part: the slot turned to buying loses them.
        let mut built = entry_of(r#"{"belegung": {"a": {"ref": "x", "name": "X", "weg": "bauen"}}}"#);
        assert!(sync_entry(&mut built, &[]));
        let mut with_quality = read_all(&built, &[]);
        with_quality.0[0].slots[0].set_quality("core", 800);
        crate::jsonio::map_set(
            &mut built,
            KEY,
            Value::Array(with_quality.0.iter().map(|e| as_value(&to_json(e))).collect()),
        );
        if let Some((_, Value::Object(plan))) = built.iter_mut().find(|(k, _)| k == "belegung") {
            crate::jsonio::map_set(plan, "a", as_value(&json!({"ref": "x", "name": "X", "weg": "kaufen"})));
        }
        assert!(sync_entry(&mut built, &[]));
        assert_eq!(read_all(&built, &[]).active().unwrap().slots[0].quality_at("core"), 500);
        // Stored slots and no plan at all (a file written elsewhere): nothing is wiped; a cleared plan is wiped.
        let mut elsewhere = entry_of(
            r#"{"ausstattungen": [{"name": "A", "active": true,
                "slots": [{"portPath": "p1", "itemGuid": "g1", "acquisition": "crafted", "quality": {"q": 700}}]}]}"#,
        );
        assert!(!sync_entry(&mut elsewhere, &[]), "no plan: nothing to say, nothing written");
        assert_eq!(read_all(&elsewhere, &[]).active().unwrap().slots[0].quality_at("q"), 700);
        crate::jsonio::map_set(&mut elsewhere, "belegung", Value::Object(Map::new()));
        assert!(sync_entry(&mut elsewhere, &[]), "an empty plan is the player's reset");
        assert!(read_all(&elsewhere, &[]).active().unwrap().slots.is_empty());
    }

    #[test]
    fn the_paint_and_the_power_block_travel_with_the_equipment_as_they_came() {
        let power = json!({"modes": {"scm": true}, "segments": {"hardpoint_laser": [1, 2, 3]}});
        let raw = json!({"name": "Mining", "active": true, "slots": [], "paintItemGuid": "paint-1", "power": power});
        let eq = from_json(&raw).unwrap();
        assert_eq!(eq.paint_item_guid.as_deref(), Some("paint-1"));
        assert_eq!(eq.power.as_ref(), Some(&power), "kept as it came");
        let again = to_json(&eq);
        assert_eq!(again["paintItemGuid"], json!("paint-1"));
        assert_eq!(again["power"], power);
        // Without them the form has neither key, and an empty paint or a power that is no block is not kept.
        let plain = from_json(&json!({"name": "x", "paintItemGuid": "", "power": 5})).unwrap();
        assert!(plain.paint_item_guid.is_none() && plain.power.is_none());
        assert!(to_json(&plain).get("paintItemGuid").is_none() && to_json(&plain).get("power").is_none());
    }

    #[test]
    fn a_correction_travels_with_the_slot_and_keeps_a_slot_of_the_stock() {
        let raw = json!({"name": "x", "active": true, "slots": [
            {"portPath": "hardpoint_shield", "itemGuid": "sh-stock", "acquisition": "purchased",
             "corrected": {"raw": "SH_OLD_NAME", "reason": "renamed in 4.1"}},
            {"portPath": "p2", "itemGuid": "a", "corrected": {"raw": "", "reason": ""}}]});
        let eq = from_json(&raw).unwrap();
        let fixed = Correction { raw: "SH_OLD_NAME".into(), reason: "renamed in 4.1".into() };
        assert_eq!(eq.slots[0].corrected.as_ref(), Some(&fixed));
        assert!(eq.slots[1].corrected.is_none(), "an empty correction is none");
        let again = to_json(&eq);
        assert_eq!(again["slots"][0]["corrected"], json!({"raw": "SH_OLD_NAME", "reason": "renamed in 4.1"}));
        assert!(again["slots"][1].get("corrected").is_none());
        assert_eq!(from_json(&again).unwrap(), eq);
        // The stock's own part with a correction stays a deviation: the correction hangs on the slot.
        let stock = vec![("hardpoint_shield".to_string(), "sh-stock".to_string())];
        assert_eq!(deviations(&stock, &eq.slots[..1]).len(), 1);
        assert!(deviations(&stock, &[Slot::new("hardpoint_shield", "sh-stock", Acquisition::Purchased)]).is_empty());
        // A change of the plan elsewhere keeps it.
        let mut ship = entry_of(
            r#"{"name": "T", "belegung": {"c": {"ref": "c9", "name": "C9", "weg": "kaufen"}}, "ausstattungen": [
                {"name": "Main", "active": true, "slots": [{"portPath": "c", "itemGuid": "c9", "acquisition": "purchased",
                 "corrected": {"raw": "C9_OLD", "reason": "why"}}]}]}"#,
        );
        assert!(crate::cart::set_part(&mut ship, "d", "d1", "D1", BUY));
        assert!(sync_entry(&mut ship, &[]));
        let kept = read_all(&ship, &[]);
        assert_eq!(
            kept.active().unwrap().slot("c").unwrap().corrected.as_ref().map(|c| c.raw.as_str()),
            Some("C9_OLD")
        );
    }

    /// A name for every part: its identifier in capitals.
    fn upper(guid: &str) -> String {
        guid.to_uppercase()
    }

    fn plan_ref(entry: &Map, path: &str) -> String {
        match get(entry, "belegung") {
            Some(Value::Object(plan)) => plan
                .iter()
                .find(|(k, _)| k == path)
                .and_then(|(_, v)| v.as_object())
                .map_or(String::new(), |part| text(part, "ref").to_string()),
            _ => String::new(),
        }
    }

    #[test]
    fn an_equipment_from_outside_is_added_under_a_free_name_and_becomes_the_active_one() {
        let mut ship = entry_of(
            r#"{"name": "T", "belegung": {"c": {"ref": "c1", "name": "C1", "weg": "kaufen", "erledigt": true}}}"#,
        );
        let mut came = Equipment { name: DEFAULT_NAME.into(), ..Equipment::default() };
        came.put(Slot::new("c", "c2", Acquisition::Unknown));
        came.put(Slot::new("w", "", Acquisition::Unknown));
        let name = add_as_active(&mut ship, &came, &[], &upper, &["C".to_string()]);
        assert_eq!(name, "Standard (2)", "the own one keeps its name");
        assert_eq!(names(&ship, &[]), (vec![DEFAULT_NAME.to_string(), name.clone()], Some(1)));
        assert_eq!(plan_ref(&ship, "C"), "c2", "the plan is the new one's, under the spelling of the slot data");
        assert!(plan_ref(&ship, "w").is_empty(), "an emptied port has no part in the plan");
        // Back to the own one: its tick is still there.
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        let ticked = match get(&ship, "belegung") {
            Some(Value::Object(plan)) => plan.iter().any(|(_, p)| truthy(p.as_object().and_then(|p| get(p, "erledigt")))),
            _ => false,
        };
        assert!(ticked);
        assert_eq!(unique_name(&["A".into(), "A (2)".into()], " A "), "A (3)");
        assert_eq!(unique_name(&[], ""), DEFAULT_NAME);
    }

    #[test]
    fn named_equipments_are_made_renamed_and_switched_and_the_plan_follows_the_active_one() {
        let mut ship = entry_of(r#"{"name": "T", "belegung": {"c": {"ref": "c1", "name": "C1", "weg": "kaufen"}}}"#);
        // The first new one: the plan as it is becomes the default one, the copy the active one.
        assert!(create(&mut ship, " Racing ", &[]));
        assert_eq!(names(&ship, &[]), (vec![DEFAULT_NAME.to_string(), "Racing".to_string()], Some(1)));
        assert_eq!(plan_ref(&ship, "c"), "c1", "the copy holds the same");
        assert!(!create(&mut ship, "Racing", &[]) && !create(&mut ship, "  ", &[]), "a name once, never empty");
        // The new one gets another part; the default one keeps its own.
        assert!(crate::cart::set_part(&mut ship, "c", "c2", "C2", BUY));
        assert!(sync_entry(&mut ship, &[]));
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        assert_eq!(plan_ref(&ship, "c"), "c1", "the plan is the one of the active equipment");
        assert!(!switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]), "active already");
        assert!(!switch(&mut ship, "Nothing", &[], &upper, &[]));
        assert!(switch(&mut ship, "Racing", &[], &upper, &[]));
        assert_eq!(plan_ref(&ship, "c"), "c2", "the change was stored with the one it was made on");
        // A change of the plan not yet stored goes with the switch into the one it was made on.
        assert!(crate::cart::set_part(&mut ship, "c", "c3", "C3", BUY));
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        assert!(switch(&mut ship, "Racing", &[], &upper, &[]));
        assert_eq!(plan_ref(&ship, "c"), "c3");
        // Renamed: not to an empty name, not to the name of another one, not one that is not there.
        assert!(!rename(&mut ship, "Racing", "") && !rename(&mut ship, "Racing", DEFAULT_NAME));
        assert!(!rename(&mut ship, "Nothing", "X"));
        assert!(rename(&mut ship, "Racing", "Race"));
        assert_eq!(names(&ship, &[]), (vec![DEFAULT_NAME.to_string(), "Race".to_string()], Some(1)));
        // Exactly one is marked active in the stored list.
        let flags: Vec<bool> = match get(&ship, KEY) {
            Some(Value::Array(items)) => items.iter().map(|i| truthy(get(i.as_object().unwrap(), "active"))).collect(),
            _ => Vec::new(),
        };
        assert_eq!(flags, [false, true]);
    }

    #[test]
    fn a_deleted_equipment_comes_back_with_undo_and_the_last_one_stays() {
        let mut ship = entry_of(r#"{"name": "T", "belegung": {"c": {"ref": "c1", "name": "C1", "weg": "kaufen"}}}"#);
        assert!(create(&mut ship, "Second", &[]));
        assert!(crate::cart::set_part(&mut ship, "c", "c2", "C2", BUY));
        assert!(sync_entry(&mut ship, &[]));
        // The active one goes: the first one left becomes active, with its plan.
        let removed = delete(&mut ship, "Second", &[], &upper, &[]).expect("deleted");
        assert!(removed.active && removed.at == 1);
        assert_eq!(names(&ship, &[]), (vec![DEFAULT_NAME.to_string()], Some(0)));
        assert_eq!(plan_ref(&ship, "c"), "c1");
        assert!(delete(&mut ship, DEFAULT_NAME, &[], &upper, &[]).is_none(), "the last one stays");
        assert!(delete(&mut ship, "Nothing", &[], &upper, &[]).is_none());
        // Back at its place, active again, with its plan.
        assert!(undo_delete(&mut ship, &removed, &[], &upper, &[]));
        assert_eq!(names(&ship, &[]), (vec![DEFAULT_NAME.to_string(), "Second".to_string()], Some(1)));
        assert_eq!(plan_ref(&ship, "c"), "c2");
        // An equipment that was not active comes back without taking over.
        let quiet = delete(&mut ship, DEFAULT_NAME, &[], &upper, &[]).expect("deleted");
        assert!(!quiet.active);
        assert_eq!(plan_ref(&ship, "c"), "c2");
        assert!(create(&mut ship, DEFAULT_NAME, &[]));
        assert!(!undo_delete(&mut ship, &quiet, &[], &upper, &[]), "its name was taken meanwhile");
    }

    #[test]
    fn the_way_a_part_was_got_is_set_per_slot_and_a_built_part_keeps_blueprint_and_quality() {
        let paths = vec!["c".to_string()];
        let mut ship = entry_of(r#"{"name": "T", "belegung": {"c": {"ref": "c1", "name": "C1", "weg": "kaufen"}}}"#);
        assert!(ensure_stored(&mut ship, &[]));
        let slot = |ship: &Map| read_all(ship, &[]).active().unwrap().slot("c").unwrap().clone();
        // Pledge: kept in the equipment, the plan keeps its way, and a later change of the plan keeps it too.
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Pledge, None, &[]));
        assert_eq!(slot(&ship).acquisition, Acquisition::Pledge);
        assert!(crate::cart::set_part(&mut ship, "d", "d1", "D1", BUY));
        assert!(sync_entry(&mut ship, &[]));
        assert_eq!(slot(&ship).acquisition, Acquisition::Pledge);
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Unknown, None, &[]));
        assert!(crate::cart::set_part(&mut ship, "d", "d2", "D2", BUY));
        sync_entry(&mut ship, &[]);
        assert_eq!(slot(&ship).acquisition, Acquisition::Unknown, "not known stays not known");
        // Built: the plan builds, the blueprint is set, the quality can be set.
        let way = |ship: &Map| match get(ship, "belegung") {
            Some(Value::Object(plan)) => plan
                .iter()
                .find(|(k, _)| k == "c")
                .and_then(|(_, v)| v.as_object())
                .map_or(String::new(), |part| text(part, "weg").to_string()),
            _ => String::new(),
        };
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Crafted, Some("bp-c1"), &[]));
        assert_eq!(way(&ship), CRAFT);
        assert_eq!(slot(&ship).blueprint_guid.as_deref(), Some("bp-c1"));
        assert!(set_quality(&mut ship, &paths, "Frame", 820, &[]));
        assert!(!set_quality(&mut ship, &paths, "Frame", 820, &[]), "the same again changes nothing");
        assert_eq!(slot(&ship).quality_at("Frame"), 820);
        assert_eq!(slot(&ship).quality_at("Core"), BASE_QUALITY);
        // Bought: the plan buys, a pledge before does not stick, blueprint and quality go.
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Purchased, None, &[]));
        let bought = slot(&ship);
        assert_eq!(
            (bought.acquisition, bought.blueprint_guid, bought.quality.len()),
            (Acquisition::Purchased, None, 0)
        );
        assert!(!set_quality(&mut ship, &paths, "Frame", 900, &[]), "only a built part has a quality");
        // Built, then pledge: the plan buys, and the pledge outlives the next change of the plan.
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Crafted, Some("bp-c1"), &[]));
        assert!(set_quality(&mut ship, &paths, "Frame", 820, &[]));
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Pledge, None, &[]));
        assert_eq!(way(&ship), BUY);
        let pledged = slot(&ship);
        assert_eq!((pledged.blueprint_guid, pledged.quality.len()), (None, 0), "a pledge was not built");
        assert!(crate::cart::set_part(&mut ship, "e", "e1", "E1", BUY));
        sync_entry(&mut ship, &[]);
        assert_eq!(slot(&ship).acquisition, Acquisition::Pledge);
    }

    #[test]
    fn the_ticks_of_an_equipment_come_back_with_it() {
        let done = |ship: &Map| crate::cart::is_done(ship, "c");
        let mut ship = entry_of(r#"{"name": "T", "belegung": {"c": {"ref": "c1", "name": "C1", "weg": "kaufen"}}}"#);
        assert!(crate::cart::set_done(&mut ship, "c", true));
        assert!(create(&mut ship, "Other", &[]));
        assert!(crate::cart::set_part(&mut ship, "c", "c2", "C2", BUY));
        sync_entry(&mut ship, &[]);
        assert!(!done(&ship), "another part is not fitted yet");
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        assert!(done(&ship), "back to the first: its part is fitted");
        assert!(switch(&mut ship, "Other", &[], &upper, &[]));
        assert!(!done(&ship));
        // The active one deleted and put back brings its ticks along.
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        let removed = delete(&mut ship, DEFAULT_NAME, &[], &upper, &[]).expect("deleted");
        assert!(!done(&ship));
        assert!(undo_delete(&mut ship, &removed, &[], &upper, &[]));
        assert!(done(&ship));
        // A tick made in the other one goes with it when it is left, and comes back with it.
        assert!(switch(&mut ship, "Other", &[], &upper, &[]));
        assert!(crate::cart::set_done(&mut ship, "c", true));
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        assert!(switch(&mut ship, "Other", &[], &upper, &[]));
        assert!(done(&ship), "the tick made on the other one comes back with it");
        // The same part in both: a tick taken back in one does not come back from the plan of the other.
        let mut twins = entry_of(r#"{"name": "T", "belegung": {"g": {"ref": "g1", "name": "G1", "weg": "kaufen"}}}"#);
        assert!(crate::cart::set_done(&mut twins, "g", true));
        assert!(create(&mut twins, "Twin", &[]));
        assert!(crate::cart::set_done(&mut twins, "g", false));
        assert!(switch(&mut twins, DEFAULT_NAME, &[], &upper, &[]));
        assert!(crate::cart::is_done(&twins, "g"));
        assert!(switch(&mut twins, "Twin", &[], &upper, &[]));
        assert!(!crate::cart::is_done(&twins, "g"), "taken back in the twin, it stays taken back");
    }

    #[test]
    fn a_switch_takes_neither_the_way_nor_the_keys_of_another_part_at_the_same_port() {
        let paths = vec!["c".to_string()];
        let mut ship = entry_of(r#"{"name": "T", "belegung": {"c": {"ref": "x", "name": "X", "weg": "bauen"}}}"#);
        assert!(create(&mut ship, "Two", &[]));
        assert!(crate::cart::set_part(&mut ship, "c", "y", "Y", BUY));
        assert!(set_acquisition(&mut ship, &paths, Acquisition::Pledge, None, &[]));
        assert!(switch(&mut ship, DEFAULT_NAME, &[], &upper, &[]));
        assert!(switch(&mut ship, "Two", &[], &upper, &[]));
        let part = match get(&ship, "belegung") {
            Some(Value::Object(plan)) => plan[0].1.as_object().cloned().unwrap_or_default(),
            _ => Map::new(),
        };
        assert_eq!(
            (text(&part, "ref"), text(&part, "weg")),
            ("y", BUY),
            "the way of the other part does not come along"
        );
        assert!(crate::cart::set_part(&mut ship, "d", "d1", "D1", BUY));
        sync_entry(&mut ship, &[]);
        let two = read_all(&ship, &[]);
        assert_eq!(two.active().unwrap().slot("c").unwrap().acquisition, Acquisition::Pledge);
        // A key of a slot the model does not know stays with its part, not with the port.
        let mut worn = entry_of(
            r#"{"name": "T", "belegung": {"c": {"ref": "c9", "name": "C9", "weg": "kaufen"}}, "ausstattungen": [
                {"name": "Main", "active": true, "slots": [
                 {"portPath": "c", "itemGuid": "c9", "acquisition": "purchased", "wear": 0.4}]}]}"#,
        );
        assert!(crate::cart::set_part(&mut worn, "c", "d9", "D9", BUY));
        assert!(sync_entry(&mut worn, &[]));
        let stored = as_json(get(&worn, KEY).unwrap());
        assert!(stored[0]["slots"][0].get("wear").is_none(), "{stored}");
    }

    #[test]
    fn a_sync_keeps_a_correction_at_the_stock_part_and_the_keys_of_a_slot_it_does_not_know() {
        let stock = vec![("shield".to_string(), "sh-stock".to_string())];
        let mut ship = entry_of(
            r#"{"name": "T", "belegung": {"c": {"ref": "c9", "name": "C9", "weg": "kaufen"}}, "ausstattungen": [
                {"name": "Main", "active": true, "slots": [
                 {"portPath": "shield", "itemGuid": "sh-stock", "acquisition": "purchased",
                  "corrected": {"raw": "SH_OLD", "reason": "renamed"}},
                 {"portPath": "c", "itemGuid": "c9", "acquisition": "purchased", "wear": 0.4}]}]}"#,
        );
        assert!(crate::cart::set_part(&mut ship, "d", "d1", "D1", BUY));
        assert!(sync_entry(&mut ship, &stock));
        let main = read_all(&ship, &stock);
        let shield = main.active().unwrap().slot("shield").cloned();
        assert_eq!(shield.and_then(|s| s.corrected).map(|c| c.raw), Some("SH_OLD".to_string()));
        let stored = as_json(get(&ship, KEY).unwrap());
        let c = stored[0]["slots"].as_array().unwrap().iter().find(|s| s["portPath"] == json!("c")).unwrap();
        assert_eq!(c["wear"], json!(0.4), "a key of the slot the model does not know stays");
    }
}
