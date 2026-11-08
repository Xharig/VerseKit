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

//! The single slots of a ship with the part fitted at the factory, and the
//! parts that fit a slot (`erkul.hardpoints`, `with_choices`, `fits_slot`,
//! `compatible`, `stock_loadout`, `fits`, `matching_ships` of 4.12), read from
//! the two stored files `erkul-schiffe.json` (the slots) and
//! `erkul-teile.json` (racks, missiles, bombs, paints, components).
//!
//! ⚠ Nothing is fetched here. A file that is missing or of another format
//! counts as empty: then a ship has no slots, and the pages say "no slot
//! data" and never "nothing to get".
//!
//! ⚠⚠ A rack brings its own slots: whoever swaps the torpedo rack of the
//! Eclipse for the bomb rack has twenty bomb slots afterwards and no torpedo
//! slots ([`with_choices`]). The display and the cart read the same list.

use std::collections::{BTreeSet, HashMap};

use crate::hangar::{get, py_int, text};
use crate::jsonio::{Map, Value};
use crate::paths::Paths;
use crate::ships::{ErkulShips, ShipId};

pub const PARTS_FILE: &str = "erkul-teile.json";
pub const PARTS_FORMAT: i64 = 2;
/// The slots of the player ships from the game data, in the form of [`crate::ships::ERKUL_FILE`]; written by
/// `versekit-gamedata` (`ship_slots`) and read before erkul's.
pub const GAME_SHIPS_FILE: &str = "spiel-schiffe.json";
/// The parts with slots of the game data, in the form of [`PARTS_FILE`].
pub const GAME_PARTS_FILE: &str = "spiel-teile.json";
/// The format of [`GAME_PARTS_FILE`]: that of [`PARTS_FILE`] with the class name of each part and the tags a paint
/// requires (since 5.2.1). A file of format [`PARTS_FORMAT`] is still read as it is; the game data read it again
/// from the archive at the next start with the game closed (`versekit-gamedata`, `values_store`).
pub const GAME_PARTS_FORMAT: i64 = 3;

/// The kinds of slot a player can change (`erkul.SWAPPABLE`): the ones a
/// blueprint can fill plus battery, avionics and gravity generator.
pub const SWAPPABLE: [&str; 29] = [
    "Cooler",
    "PowerPlant",
    "Shield",
    "QuantumDrive",
    "Radar",
    "JumpDrive",
    "WeaponGun",
    "Turret",
    "TurretBase",
    "MissileLauncher",
    "Missile",
    "BombLauncher",
    "Bomb",
    "MiningLaser",
    "WeaponMining",
    "SalvageHead",
    "TractorBeam",
    "TowingBeam",
    "QuantumInterdictionGenerator",
    "EMP",
    "FlightController",
    "Paints",
    "MiningModifier",
    "SalvageModifier",
    "Container",
    "Cargo",
    "Battery",
    "Avionics",
    "GravityGenerator",
];

/// What a slot takes (`erkul._constraints`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Fit {
    /// `(kind, subkinds)`; no subkinds means every one.
    pub types: Vec<(String, Vec<String>)>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// What the slot offers.
    pub port_tags: Vec<String>,
    /// What a part has to carry to go in.
    pub required: Vec<String>,
}

/// One slot of a ship.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotInfo {
    /// The full path through the racks and mounts, `hardpoint_turret/hardpoint_class_2`.
    pub path: String,
    pub kind: String,
    pub size: Option<i64>,
    pub fit: Option<Fit>,
    /// The part fitted at the factory: `(identifier, name)`.
    pub factory: Option<(String, String)>,
    /// Built in for good (`fest`): it shows its factory part and takes no other.
    pub fixed: bool,
}

/// One port of a part that holds other parts (a rack).
#[derive(Clone, Debug, PartialEq)]
pub struct PartPort {
    pub name: String,
    pub fit: Fit,
}

/// A part of the erkul families (`erkul._slim_part`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Part {
    pub reference: String,
    pub name: String,
    pub kind: String,
    pub subkind: String,
    pub size: Option<i64>,
    pub tags: Vec<String>,
    pub required: Vec<String>,
    pub grade: String,
    pub ports: Vec<PartPort>,
    /// Port name → what sits in it at the factory, in stored order.
    pub factory: Vec<(String, (String, String))>,
    /// The class name in the game data (`Controller_Flight_AEGS_Gladius`); empty where the file has none
    /// (erkul's, and the game files written before 5.2.1).
    pub class: String,
}

/// The stored parts.
#[derive(Clone, Debug, Default)]
pub struct ErkulParts {
    parts: Vec<Part>,
    index: HashMap<String, usize>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

/// `erkul._constraints` as stored (`passt`): `None` where there is none (an empty dictionary counts as none).
fn fit_of(value: Option<&Value>) -> Option<Fit> {
    let map = value?.as_object().filter(|m| !m.is_empty())?;
    let types = match get(map, "typen") {
        Some(Value::Array(pairs)) => pairs
            .iter()
            .filter_map(|pair| match pair {
                Value::Array(two) => Some((two.first()?.as_str()?.to_string(), strings(two.get(1)))),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(Fit {
        types,
        min: py_int(get(map, "min")),
        max: py_int(get(map, "max")),
        port_tags: strings(get(map, "porttags")),
        required: strings(get(map, "verlangt")),
    })
}

fn pair_of(value: Option<&Value>) -> Option<(String, String)> {
    let map = value?.as_object().filter(|m| !m.is_empty())?;
    Some((text(map, "ref").to_string(), text(map, "name").to_string()))
}

impl SlotInfo {
    fn of(map: &Map) -> SlotInfo {
        SlotInfo {
            path: text(map, "pfad").to_string(),
            kind: text(map, "art").to_string(),
            size: py_int(get(map, "groesse")),
            fit: fit_of(get(map, "passt")),
            factory: pair_of(get(map, "werk")),
            fixed: crate::hangar::truthy(get(map, "fest")),
        }
    }
}

impl ErkulParts {
    /// Reads the stored parts file, erkul's ([`PARTS_FORMAT`]) or the game's ([`GAME_PARTS_FORMAT`]); empty without a
    /// usable one.
    pub fn load(path: &str) -> ErkulParts {
        let root = match crate::jsonio::read_json(path) {
            Ok(Value::Object(root))
                if [PARTS_FORMAT, GAME_PARTS_FORMAT].iter().any(|format| {
                    crate::jsonio::py_eq(get(&root, "format"), Some(&Value::int(*format)))
                }) =>
            {
                root
            }
            _ => Map::new(),
        };
        let mut parts = Vec::new();
        if let Some(Value::Object(items)) = get(&root, "teile") {
            for (reference, raw) in items {
                let Some(raw) = raw.as_object() else { continue };
                let ports = match get(raw, "plaetze") {
                    Some(Value::Array(ports)) => ports
                        .iter()
                        .filter_map(Value::as_object)
                        .filter_map(|p| {
                            Some(PartPort { name: text(p, "name").to_string(), fit: fit_of(get(p, "passt"))? })
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let factory = match get(raw, "werk") {
                    Some(Value::Object(items)) => {
                        items.iter().filter_map(|(port, v)| Some((port.clone(), pair_of(Some(v))?))).collect()
                    }
                    _ => Vec::new(),
                };
                parts.push(Part {
                    reference: reference.clone(),
                    name: text(raw, "name").to_string(),
                    kind: text(raw, "typ").to_string(),
                    subkind: text(raw, "untertyp").to_string(),
                    size: py_int(get(raw, "groesse")),
                    tags: strings(get(raw, "tags")),
                    required: strings(get(raw, "verlangt")),
                    grade: text(raw, "guete").to_string(),
                    ports,
                    factory,
                    class: text(raw, "klasse").to_string(),
                });
            }
        }
        ErkulParts::from_parts(parts)
    }

    /// The parts of the game data, and of erkul's those the game data do not have (by identifier).
    pub fn merged(game: ErkulParts, erkul: ErkulParts) -> ErkulParts {
        if game.is_empty() {
            return erkul;
        }
        let mut parts = game.parts;
        let known: std::collections::HashSet<String> = parts.iter().map(|p| p.reference.clone()).collect();
        parts.extend(erkul.parts.into_iter().filter(|p| !known.contains(&p.reference)));
        ErkulParts::from_parts(parts)
    }

    /// Parts made up for a test or a page.
    pub fn from_parts(parts: Vec<Part>) -> ErkulParts {
        let mut index = HashMap::new();
        for (i, part) in parts.iter().enumerate() {
            index.insert(part.reference.clone(), i);
        }
        ErkulParts { parts, index }
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// All parts in the order they are stored.
    pub fn all(&self) -> &[Part] {
        &self.parts
    }

    pub fn get(&self, reference: &str) -> Option<&Part> {
        self.index.get(reference).map(|&i| &self.parts[i])
    }

    /// `erkul.compatible`: the stored parts that fit this slot. A ship-bound slot without its own `fit`
    /// takes the parts that belong to the same ship as its factory part ([`ErkulParts::bound_stock`]).
    pub fn compatible(&self, slot: &SlotInfo) -> Vec<&Part> {
        if let Some(fit) = &slot.fit {
            return self.parts.iter().filter(|p| fits_slot(fit, p)).collect();
        }
        match self.bound_stock(slot) {
            Some(stock) => self.parts.iter().filter(|p| fits_like_stock(stock, p)).collect(),
            None => Vec::new(),
        }
    }

    /// The factory part a slot of a [`SHIP_BOUND`] kind takes its rule from, where the slot has no `fit` of its
    /// own (a port right on the ship: its tags sit in the encrypted vehicle implementation) and the part is known.
    pub fn bound_stock(&self, slot: &SlotInfo) -> Option<&Part> {
        if slot.fit.is_some() || !SHIP_BOUND.contains(&slot.kind.as_str()) {
            return None;
        }
        let (reference, _) = slot.factory.as_ref()?;
        self.get(reference).filter(|stock| stock.kind == slot.kind)
    }
}

/// The kinds of slot whose parts belong to one ship each: the flight blades and the paints. A blade of the game
/// carries the tag of its ship and requires the same tag of the port (`Tags` = `RequiredTags`, e.g.
/// `AEGS_Gladius_Blade`), so it goes only where the factory blade of the ship goes. A paint requires the tag of its
/// ship family of the port (`RequiredTags` = `Paint_Gladius`), which the port offers.
///
/// ⚠⚠ Such a slot without a rule of its own and without a known factory part has no list at all: which paints a
/// ship takes sits in the encrypted vehicle implementation, and the names do not tell it (the MDC takes
/// `Paint_MTC`, the Ursa `Ursa_Paint`). No choice is better than the paints of every ship.
///
/// ⚠ Not the turrets and racks: their tags mix ship-bound ones with general ones (`gimbalMount`, `flightReady`),
/// and what the port of the ship requires sits in the encrypted vehicle implementation.
pub const SHIP_BOUND: [&str; 2] = ["FlightController", "Paints"];

/// Does `part` belong to the same ship as the factory part `stock` of a [`SHIP_BOUND`] slot? The factory part
/// itself always does.
///
/// A paint: one that requires of its port only what the factory paint requires (the port offers that), and not
/// nothing; without what the factory paint requires (a file of 5.2.0) none other.
///
/// A flight blade: with tags at the factory part exactly its tags (case aside). Without tags (the
/// Constellation Mk IV, the Fury, the 300 series): only an untagged blade whose class name continues the class
/// name of the factory part right with `_Blade` or `_Flight_Blade` (`Controller_Flight_RSI_Constellation_Andromeda`
/// → `…_Andromeda_Blade_HND`); without class names nothing else.
///
/// ⚠ Not the bare prefix: `Controller_Flight_MISC_Fury_LX` and `…_Fury_LX_Blade_HND` continue
/// `Controller_Flight_MISC_Fury` too, and belong to another ship.
pub fn fits_like_stock(stock: &Part, part: &Part) -> bool {
    if part.reference == stock.reference {
        return true;
    }
    if part.kind != stock.kind {
        return false;
    }
    if let (Some(own), Some(wanted)) = (part.size, stock.size)
        && own != wanted
    {
        return false;
    }
    let lowered = |tags: &[String]| tags.iter().map(|t| t.to_lowercase()).collect::<BTreeSet<String>>();
    if stock.kind == "Paints" {
        let offered = lowered(&stock.required);
        return !offered.is_empty() && !part.required.is_empty() && lowered(&part.required).is_subset(&offered);
    }
    if !stock.tags.is_empty() {
        return lowered(&part.tags) == lowered(&stock.tags);
    }
    let family = stock.class.to_lowercase();
    if !part.tags.is_empty() || family.is_empty() {
        return false;
    }
    part.class
        .to_lowercase()
        .strip_prefix(&format!("{family}_"))
        .is_some_and(|rest| rest.starts_with("blade") || rest.starts_with("flight_blade"))
}

/// `erkul.fits_slot`: does the part go into the slot? Four conditions, all must hold: kind and subkind
/// are in the list of the slot; the size of the part lies between the smallest and largest of the slot
/// (not equal to the slot size: the bomb rack S3 sits in a slot of size 10); what the part requires, the
/// slot offers; what the slot requires, the part carries.
///
/// ⚠ Without the last two the torpedo slot of the Eclipse would offer the racks of every other ship:
/// 126 instead of 4.
///
/// A paint compares its tags without case: what it requires comes from the DataForge (since 5.2.1), what the port
/// offers from erkul or the ship Record, and the game spells a family both ways (`Paint_Gladius`, `paint_gladius`).
pub fn fits_slot(fit: &Fit, part: &Part) -> bool {
    if !fit.types.iter().any(|(kind, subs)| *kind == part.kind && (subs.is_empty() || subs.contains(&part.subkind))) {
        return false;
    }
    if let Some(size) = part.size
        && (fit.min.is_some_and(|low| size < low) || fit.max.is_some_and(|high| size > high))
    {
        return false;
    }
    if part.kind == "Paints" {
        let lowered = |tags: &[String]| tags.iter().map(|t| t.to_lowercase()).collect::<BTreeSet<String>>();
        let (offered, carried) = (lowered(&fit.port_tags), lowered(&part.tags));
        return lowered(&part.required).is_subset(&offered) && lowered(&fit.required).is_subset(&carried);
    }
    let offered: BTreeSet<&String> = fit.port_tags.iter().collect();
    if !part.required.iter().all(|r| offered.contains(r)) {
        return false;
    }
    let carried: BTreeSet<&String> = part.tags.iter().collect();
    fit.required.iter().all(|r| carried.contains(r))
}

/// `erkul.with_choices`: the slots after the own rack changes. A different rack brings different slots:
/// the slots below it are replaced by those of the chosen rack with its factory fitting.
/// `chosen` is the equipment of the ship, path → `{ref, …}`.
pub fn with_choices(slots: Vec<SlotInfo>, chosen: &Map, parts: &ErkulParts) -> Vec<SlotInfo> {
    if parts.is_empty() && !chosen.iter().any(|(_, v)| v.as_object().is_some_and(crate::cart::is_emptied)) {
        return slots;
    }
    let pick_of = |path: &str| -> Option<String> {
        let part = get(chosen, path)?.as_object()?;
        let reference = text(part, "ref");
        (!reference.is_empty()).then(|| reference.to_string())
    };
    let mut out: Vec<SlotInfo> = Vec::new();
    let mut gone: Vec<String> = Vec::new();
    let mut racks: HashMap<String, &Part> = HashMap::new();
    for mut slot in slots {
        let path = slot.path.clone();
        if gone.iter().any(|prefix| path.starts_with(prefix)) {
            continue;
        }
        // The slots in the factory rack need `fit` too: erkul keeps them at the rack, not at the ship.
        let (parent, own) = match path.rfind('/') {
            Some(at) => (&path[..at], &path[at + 1..]),
            None => ("", path.as_str()),
        };
        if slot.fit.is_none()
            && let Some(rack) = racks.get(parent)
            && let Some(port) = rack.ports.iter().find(|p| p.name == own)
        {
            slot.fit = Some(port.fit.clone());
        }
        let stock = slot.factory.as_ref().map(|(reference, _)| reference.clone()).filter(|r| !r.is_empty());
        let fixed = slot.fixed;
        out.push(slot);
        // A slot left empty holds no rack, so nothing hangs below it; a fixed slot is never left empty.
        if !fixed && get(chosen, &path).and_then(Value::as_object).is_some_and(crate::cart::is_emptied) {
            gone.push(format!("{path}/"));
            continue;
        }
        let pick = if fixed { None } else { pick_of(&path) };
        let Some(rack) = pick.as_ref().or(stock.as_ref()).and_then(|reference| parts.get(reference)) else {
            continue;
        };
        if rack.ports.is_empty() {
            continue;
        }
        racks.insert(path.clone(), rack);
        if pick.is_none() || pick == stock {
            continue;
        }
        let prefix = format!("{path}/");
        gone.push(prefix.clone());
        for port in &rack.ports {
            let kind = port.fit.types.iter().map(|(t, _)| t.as_str()).find(|t| SWAPPABLE.contains(t)).unwrap_or("");
            out.push(SlotInfo {
                path: format!("{prefix}{}", port.name),
                kind: kind.to_string(),
                size: port.fit.max,
                fit: Some(port.fit.clone()),
                factory: rack.factory.iter().find(|(name, _)| *name == port.name).map(|(_, pair)| pair.clone()),
                fixed: false,
            });
        }
    }
    out
}

/// What the ship is called for the lookup: the fields of the hangar entry.
pub fn ship_id(entry: &Map) -> ShipId<'_> {
    ShipId {
        name: text(entry, "name"),
        maker: text(entry, "hersteller"),
        short: text(entry, "kurz"),
        maker_short: text(entry, "hkurz"),
    }
}

/// `erkul.hardpoints`: the single changeable slots of a ship, after the rack changes in `chosen`.
/// An empty list means "no data"; whether erkul knows the ship at all says [`ErkulShips::knows`].
pub fn hardpoints(ships: &ErkulShips, parts: &ErkulParts, id: ShipId, chosen: &Map) -> Vec<SlotInfo> {
    let Some(entry) = ships.entry(id) else { return Vec::new() };
    let slots = match get(entry, "slots") {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).map(SlotInfo::of).collect(),
        _ => Vec::new(),
    };
    with_choices(slots, chosen, parts)
}

/// The two stored files together: the slots of the ships and the parts.
#[derive(Clone, Debug, Default)]
pub struct ErkulData {
    pub ships: ErkulShips,
    pub parts: ErkulParts,
}

impl ErkulData {
    /// The four files in the data folder: the slots and parts of the game data first, then those of erkul.
    pub fn files(paths: &Paths) -> [String; 4] {
        [
            paths.app_file(GAME_SHIPS_FILE),
            paths.app_file(GAME_PARTS_FILE),
            paths.app_file(crate::ships::ERKUL_FILE),
            paths.app_file(PARTS_FILE),
        ]
    }

    /// The slots of the game data ([`GAME_SHIPS_FILE`], built from the `Data.p4k` with p4kx), and erkul's only
    /// for the ships the game data lack or where they leave a slot without what fits (see
    /// [`ErkulShips::merged`]); the parts the same way.
    pub fn load(paths: &Paths) -> ErkulData {
        let [game_ships, game_parts, ships, parts] = ErkulData::files(paths);
        ErkulData {
            ships: ErkulShips::merged(ErkulShips::load(&game_ships), ErkulShips::load(&ships)),
            parts: ErkulParts::merged(ErkulParts::load(&game_parts), ErkulParts::load(&parts)),
        }
    }

    /// The name of a part for the plan (the identifier where erkul does not know it).
    pub fn part_name(&self, reference: &str) -> String {
        self.parts.get(reference).map_or_else(|| reference.to_string(), |p| p.name.clone())
    }

    /// The paths of all slots of a ship that take a part of the player, as erkul spells them.
    pub fn slot_paths(&self, ship: &Map) -> Vec<String> {
        hardpoints(&self.ships, &self.parts, ship_id(ship), &Map::new()).into_iter().filter(|s| !s.fixed).map(|s| s.path).collect()
    }

    /// What is fitted at the factory, port by port: `(path, identifier of the part)`, for the stored equipment
    /// (only deviations from this are kept).
    pub fn stock_ports(&self, ship: &Map) -> Vec<(String, String)> {
        hardpoints(&self.ships, &self.parts, ship_id(ship), &Map::new())
            .into_iter()
            .filter_map(|slot| slot.factory.filter(|(r, _)| !r.is_empty()).map(|(r, _)| (slot.path, r)))
            .collect()
    }

    /// The slots of a hangar entry for the equipment chosen so far, as the shopping list wants them
    /// (`cart.line_items` reads `erkul.hardpoints(name, maker, short, maker short, chosen)`). A fixed slot is
    /// none: nothing in it is bought or built.
    pub fn cart_slots(&self, ship: &Map, chosen: &Map) -> Vec<crate::cart::Slot> {
        hardpoints(&self.ships, &self.parts, ship_id(ship), chosen)
            .into_iter()
            .filter(|slot| !slot.fixed)
            .map(|slot| {
                let (factory_ref, factory_name) = slot.factory.unwrap_or_default();
                crate::cart::Slot { path: slot.path, kind: slot.kind, size: slot.size, factory_ref, factory_name }
            })
            .collect()
    }
}

/// A part of the factory fitting, counted by its identifier.
#[derive(Clone, Debug, PartialEq)]
pub struct Stock {
    pub reference: String,
    pub name: String,
    pub kind: String,
    pub size: Option<i64>,
    pub count: i64,
}

/// `erkul.stock_loadout`: what is fitted at the factory, each part once with its count, the most common
/// first. Counted by the identifier, not the name.
pub fn stock_loadout(ships: &ErkulShips, parts: &ErkulParts, id: ShipId) -> Vec<Stock> {
    let mut counted: Vec<Stock> = Vec::new();
    for slot in hardpoints(ships, parts, id, &Map::new()) {
        let Some((reference, name)) = slot.factory.filter(|(r, _)| !r.is_empty()) else { continue };
        match counted.iter_mut().find(|s| s.reference == reference) {
            Some(found) => found.count += 1,
            None => counted.push(Stock { reference, name, kind: slot.kind, size: slot.size, count: 1 }),
        }
    }
    counted.sort_by_cached_key(|s| (-s.count, s.name.to_lowercase()));
    counted
}

/// The slots of a stored ship counted as `(kind, size, number)`: the counts of `plaetze`, and for each kind and size
/// the larger number where the fitted `slots` list more. Some ships keep their components (cooler, power plant,
/// shield, quantum drive) only in `slots`; without them no component would fit those ships.
fn slot_counts(entry: &Map) -> Vec<(String, i64, i64)> {
    let mut counted: Vec<(String, i64, i64)> = Vec::new();
    if let Some(Value::Array(stored)) = get(entry, "plaetze") {
        for p in stored.iter().filter_map(Value::as_object) {
            let key = (text(p, "art").to_string(), py_int(get(p, "groesse")).unwrap_or(-1));
            let n = py_int(get(p, "anzahl")).unwrap_or(0);
            match counted.iter_mut().find(|(k, s, _)| *k == key.0 && *s == key.1) {
                Some(found) => found.2 += n,
                None => counted.push((key.0, key.1, n)),
            }
        }
    }
    let mut fitted: Vec<(String, i64, i64)> = Vec::new();
    if let Some(Value::Array(slots)) = get(entry, "slots") {
        for slot in slots.iter().filter_map(Value::as_object) {
            let (kind, size) = (text(slot, "art"), py_int(get(slot, "groesse")).unwrap_or(-1));
            // A fixed slot takes no blueprint part.
            if kind.is_empty() || crate::hangar::truthy(get(slot, "fest")) {
                continue;
            }
            match fitted.iter_mut().find(|(k, s, _)| k == kind && *s == size) {
                Some(found) => found.2 += 1,
                None => fitted.push((kind.to_string(), size, 1)),
            }
        }
    }
    for (kind, size, n) in fitted {
        match counted.iter_mut().find(|(k, s, _)| *k == kind && *s == size) {
            Some(found) => found.2 = found.2.max(n),
            None => counted.push((kind, size, n)),
        }
    }
    counted
}

/// `erkul.fits`: how many slots of this kind and size the ship has; 0 where none fits or the ship is unknown.
pub fn fits(ships: &ErkulShips, kind: &str, size: Option<i64>, id: ShipId) -> i64 {
    let Some(size) = size.filter(|_| !kind.is_empty()) else { return 0 };
    let Some(entry) = ships.entry(id) else { return 0 };
    slot_counts(entry).iter().filter(|(k, s, _)| k == kind && *s == size).map(|(_, _, n)| *n).sum()
}

/// `erkul.matching_ships`: into which of these hangar ships a part of this kind and size goes; pairs
/// `(name, number of slots)`, the most slots first. An empty list means "fits nowhere", not "no data".
pub fn matching_ships(ships: &ErkulShips, kind: &str, size: Option<i64>, hangar: &[&Map]) -> Vec<(String, i64)> {
    if kind.is_empty() || size.is_none() {
        return Vec::new();
    }
    let mut found: Vec<(String, i64)> = hangar
        .iter()
        .filter_map(|entry| {
            let count = fits(ships, kind, size, ship_id(entry));
            (count != 0).then(|| (text(entry, "name").to_string(), count))
        })
        .collect();
    found.sort_by_cached_key(|(name, count)| (-count, name.to_lowercase()));
    found
}

/// `erkul.INTERESTING`: the kinds of part that go into a ship and for which "fits into your ship" is said at
/// all. ⚠ The shopping list does NOT use it: it also offers parts that have no entry here (battery, on-board
/// computer), and widening this list would change the counts of `fits` in the cards of the ships.
pub const INTERESTING: [&str; 26] = [
    "Cooler",
    "PowerPlant",
    "Shield",
    "QuantumDrive",
    "Radar",
    "JumpDrive",
    "WeaponGun",
    "Turret",
    "TurretBase",
    "MissileLauncher",
    "Missile",
    "BombLauncher",
    "Bomb",
    "MiningLaser",
    "WeaponMining",
    "SalvageHead",
    "TractorBeam",
    "TowingBeam",
    "QuantumInterdictionGenerator",
    "EMP",
    "FlightController",
    "Paints",
    "MiningModifier",
    "SalvageModifier",
    "Container",
    "Cargo",
];

/// The answer to "does this part fit into one of my ships?" (`pages._hangar_fit` of 4.12). The cases look alike in
/// code (an empty list) and mean the opposite of each other, so they are told apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FitAnswer {
    /// Nothing is said: the part has no size, or is no part of a ship (armour, hand weapons).
    Silent,
    /// The hangar has no ship: "enter your ships", NOT "fits nowhere".
    EmptyHangar,
    /// The slots of the ships are not known (the file is missing or was thrown away).
    NoData,
    /// Into these ships, with the number of slots, the most slots first.
    In(Vec<(String, i64)>),
    /// Into none of the ships.
    Nowhere,
}

/// Kinds of part that go only into ships of their own maker: erkul names their slots only by kind and size, but an
/// ore pod of Argo goes into the MOLE, one of MISC into the Prospector, one of Drake into the Golem.
pub const OWN_MAKER_KINDS: [&str; 1] = ["Container"];

/// The words of a maker's name, lower case, without the signs between them.
fn maker_words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Whether a part of this kind, maker and name may go into the hangar ship `ship` at all. Kinds outside
/// [`OWN_MAKER_KINDS`] always may; those inside only into a ship of the maker of the part, or of the brand the name
/// of the part begins with ("MISC Enhanced Ore Pod" is made by Argo and goes into the MOLE and the Prospector).
pub fn maker_allows(kind: &str, part_maker: Option<&str>, part_name: &str, ship: &Map) -> bool {
    if !OWN_MAKER_KINDS.contains(&kind) {
        return true;
    }
    let brand = part_name.split_whitespace().next().unwrap_or_default();
    same_maker(part_maker.unwrap_or_default(), ship) || same_maker(brand, ship)
}

/// Whether `maker` names the maker of the hangar ship `ship`: by the whole name, its first word ("Argo Astronautics"
/// and "ARGO") or its initials ("Musashi Industrial & Starflight Concern" and "MISC"), against the maker and the
/// maker code of the hangar. An empty name names nobody.
fn same_maker(maker: &str, ship: &Map) -> bool {
    let part = maker_words(maker);
    let Some(first) = part.first() else { return false };
    let initials: String = part.iter().filter_map(|w| w.chars().next()).collect();
    let ship_maker = maker_words(text(ship, "hersteller"));
    if !ship_maker.is_empty() && (ship_maker == part || ship_maker[0] == *first) {
        return true;
    }
    [text(ship, "hersteller"), text(ship, "hkurz")].iter().any(|name| {
        let name = name.trim().to_lowercase();
        !name.is_empty() && (name == initials || name == *first)
    })
}

/// The part (`kind`, `size`, `maker` and `name` as the blueprint catalog has them) against the hangar.
pub fn fit_answer(
    ships: &ErkulShips,
    kind: &str,
    size: Option<i64>,
    (maker, name): (Option<&str>, &str),
    hangar: &[&Map],
) -> FitAnswer {
    if kind.is_empty() || size.is_none_or(|s| s == 0) || !INTERESTING.contains(&kind) {
        return FitAnswer::Silent;
    }
    if hangar.is_empty() {
        return FitAnswer::EmptyHangar;
    }
    if ships.is_empty() {
        return FitAnswer::NoData;
    }
    let allowed: Vec<&Map> = hangar.iter().copied().filter(|ship| maker_allows(kind, maker, name, ship)).collect();
    let found = matching_ships(ships, kind, size, &allowed);
    if found.is_empty() { FitAnswer::Nowhere } else { FitAnswer::In(found) }
}

/// The slots of one ship counted by kind and size, read once, so that every part of a long list can be held
/// against the same ship without looking the ship up again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotTable {
    /// `(kind, size, number of slots)` as stored.
    counted: Vec<(String, i64, i64)>,
}

impl SlotTable {
    /// The stored counts of a ship; `None` where erkul does not know the ship or stores no slots for it.
    pub fn of(ships: &ErkulShips, id: ShipId) -> Option<SlotTable> {
        let counted = slot_counts(ships.entry(id)?);
        (!counted.is_empty()).then_some(SlotTable { counted })
    }

    /// How many slots of the ship take a part of this kind and size: the count of [`fits`], with the
    /// guard of [`fit_answer`] (a kind of [`INTERESTING`] and a size other than 0), so that "fits into
    /// your ship" and the parts of a ship never say different things.
    pub fn takes(&self, kind: &str, size: Option<i64>) -> i64 {
        let Some(size) = size.filter(|s| *s != 0) else { return 0 };
        if kind.is_empty() || !INTERESTING.contains(&kind) {
            return 0;
        }
        self.counted.iter().filter(|(k, s, _)| k == kind && *s == size).map(|(_, _, n)| *n).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_of_the_game_data_come_first_and_erkul_adds_the_ones_they_lack() {
        let part = |reference: &str, name: &str| Part { reference: reference.into(), name: name.into(), ..Part::default() };
        let game = ErkulParts::from_parts(vec![part("tmsb", "TMSB-5 (game)")]);
        let erkul = ErkulParts::from_parts(vec![part("tmsb", "TMSB-5 (erkul)"), part("rack", "Rack")]);
        let merged = ErkulParts::merged(game, erkul);
        assert_eq!(merged.get("tmsb").map(|p| p.name.as_str()), Some("TMSB-5 (game)"));
        assert_eq!(merged.get("rack").map(|p| p.name.as_str()), Some("Rack"), "what the game data lack comes from erkul");
        assert_eq!(merged.all().len(), 2);
        assert_eq!(ErkulParts::merged(ErkulParts::default(), ErkulParts::from_parts(vec![part("a", "A")])).all().len(), 1);
    }

    fn fit(kind: &str, low: i64, high: i64, offers: &[&str], needs: &[&str]) -> Fit {
        Fit {
            types: vec![(kind.to_string(), Vec::new())],
            min: Some(low),
            max: Some(high),
            port_tags: offers.iter().map(|s| (*s).to_string()).collect(),
            required: needs.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn part(reference: &str, kind: &str, size: i64, tags: &[&str], needs: &[&str]) -> Part {
        Part {
            reference: reference.into(),
            name: reference.into(),
            kind: kind.into(),
            size: Some(size),
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            required: needs.iter().map(|s| (*s).to_string()).collect(),
            ..Part::default()
        }
    }

    fn slot(path: &str, kind: &str, fit: Option<Fit>, factory: Option<&str>) -> SlotInfo {
        SlotInfo {
            path: path.into(),
            kind: kind.into(),
            size: Some(3),
            fit,
            factory: factory.map(|r| (r.to_string(), r.to_string())),
            fixed: false,
        }
    }

    #[test]
    fn a_part_fits_only_where_the_tags_agree_in_both_directions() {
        let slot_fit = fit("MissileLauncher", 3, 10, &["Eclipse_Rack"], &["Eclipse_Rack"]);
        // Size inside the range, tags in both directions: fits.
        assert!(fits_slot(&slot_fit, &part("a", "MissileLauncher", 3, &["Eclipse_Rack"], &["Eclipse_Rack"])));
        // The rack of another ship does not carry what the slot requires.
        assert!(!fits_slot(&slot_fit, &part("b", "MissileLauncher", 3, &["Other"], &["Other"])));
        // The part requires something the slot does not offer.
        assert!(!fits_slot(&slot_fit, &part("c", "MissileLauncher", 3, &["Eclipse_Rack"], &["Eclipse_Rack", "Extra"])));
        // Too big or the wrong kind.
        assert!(!fits_slot(&slot_fit, &part("d", "MissileLauncher", 11, &["Eclipse_Rack"], &[])));
        assert!(!fits_slot(&slot_fit, &part("e", "Cooler", 3, &["Eclipse_Rack"], &[])));
        // A part without a size is not cut by the range.
        let mut paint = part("f", "MissileLauncher", 0, &["Eclipse_Rack"], &[]);
        paint.size = None;
        assert!(fits_slot(&slot_fit, &paint));
    }

    /// Flight blades as the game data hold them: per ship a factory blade and blades to buy, each with the tag of
    /// its ship; the Constellation Andromeda (and its Master Modes copy) without tags, the Fury and Fury LX too.
    fn blades() -> ErkulParts {
        let blade = |reference: &str, name: &str, class: &str, tags: &[&str]| Part {
            reference: reference.into(),
            name: name.into(),
            kind: "FlightController".into(),
            subkind: "UNDEFINED".into(),
            size: Some(1),
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            class: class.into(),
            ..Part::default()
        };
        ErkulParts::from_parts(vec![
            blade("f8c", "Flight Blade", "Controller_Flight_ANVL_Lightning_F8C", &["ANVL_Lighting_Blade"]),
            blade("f8", "Flight Blade", "Controller_Flight_ANVL_Lightning_F8", &["ANVL_Lighting_Blade"]),
            blade("glad", "Gladius Standard Flight Blade", "Controller_Flight_AEGS_Gladius", &["AEGS_Gladius_Blade"]),
            blade("glad-hnd", "Gladius PHB Flight Blade", "Controller_Flight_AEGS_Gladius_Blade_HND", &["AEGS_Gladius_Blade"]),
            blade("glad-spd", "Gladius TSB Flight Blade", "Controller_Flight_AEGS_Gladius_Blade_SPD", &["aegs_gladius_blade"]),
            blade("arrow", "Arrow Standard Flight Blade", "Controller_Flight_ANVL_Arrow", &["ANVL_Arrow_Blade"]),
            blade("asgard", "Anvil Asgard Standard Flight Blade", "Controller_Flight_ANVL_Asgard", &["ANVL_Asgard_Blade"]),
            blade("andro", "Andromeda Standard", "Controller_Flight_RSI_Constellation_Andromeda", &[]),
            blade("andro-hnd", "Andromeda PHB", "Controller_Flight_RSI_Constellation_Andromeda_Blade_HND", &[]),
            blade("andro-mm", "Andromeda Standard", "Controller_Flight_RSI_Constellation_Andromeda_MM_322", &[]),
            blade("andro-gs-hnd", "Andromeda GS PHB", "Controller_Flight_RSI_Constellation_Andromeda_GS_Blade_HND", &["$RSI_Constellation_Andromeda_Blade"]),
            blade("fury", "Flight Blade", "Controller_Flight_MISC_Fury", &[]),
            blade("fury-lx", "Flight Blade", "Controller_Flight_MISC_Fury_LX", &[]),
            blade("fury-lx-hnd", "Fury LX PHB", "Controller_Flight_MISC_Fury_LX_Blade_HND", &[]),
            blade("fury-spd", "Fury TSB", "Controller_Flight_MISC_Fury_Flight_Blade_SPD", &[]),
            // A blade of the size of another family and a part of another kind with the same tag.
            Part { size: Some(2), ..blade("big", "Big Blade", "Controller_Flight_AEGS_Gladius_Blade_XL", &["AEGS_Gladius_Blade"]) },
            Part { kind: "Cooler".into(), ..blade("cool", "Cooler", "COOL_Gladius", &["AEGS_Gladius_Blade"]) },
        ])
    }

    fn refs(parts: &ErkulParts, slot: &SlotInfo) -> Vec<String> {
        let mut found: Vec<String> = parts.compatible(slot).iter().map(|p| p.reference.clone()).collect();
        found.sort();
        found
    }

    fn blade_slot(factory: Option<&str>) -> SlotInfo {
        SlotInfo { size: Some(1), ..slot("hardpoint_controller_flight", "FlightController", None, factory) }
    }

    #[test]
    fn a_flight_blade_slot_takes_only_the_blades_of_its_own_ship() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let parts = blades();
        // The F8C takes the blades with its tag: its own and the F8's, never the Gladius' or the Arrow's.
        assert_eq!(refs(&parts, &blade_slot(Some("f8c"))), ["f8", "f8c"]);
        // The Gladius takes its three, the tag compared without case; not the cooler with the same tag, not the
        // blade of another size.
        assert_eq!(refs(&parts, &blade_slot(Some("glad"))), ["glad", "glad-hnd", "glad-spd"]);
        // Without tags: the blades of the same class family only, not the Master Modes copy, not the Mk IV GS
        // (tagged), not the Fury LX nor its blade beside the Fury.
        assert_eq!(refs(&parts, &blade_slot(Some("andro"))), ["andro", "andro-hnd"]);
        assert_eq!(refs(&parts, &blade_slot(Some("fury"))), ["fury", "fury-spd"]);
        assert_eq!(refs(&parts, &blade_slot(Some("fury-lx"))), ["fury-lx", "fury-lx-hnd"]);
        // Without class names (a file written before 5.2.1) an untagged factory blade keeps to itself.
        let no_class = ErkulParts::from_parts(blades().parts.into_iter().map(|p| Part { class: String::new(), ..p }).collect());
        assert_eq!(refs(&no_class, &blade_slot(Some("andro"))), ["andro"]);
        // The factory part is always among them, even one the rule would not take.
        assert!(fits_like_stock(parts.get("fury").unwrap(), parts.get("fury").unwrap()));
        // A slot of its own rule (erkul's file with its tags) goes by that rule, not by the factory part.
        let own = SlotInfo { fit: Some(fit("FlightController", 1, 1, &[], &["ANVL_Arrow_Blade"])), ..blade_slot(Some("f8c")) };
        assert_eq!(refs(&parts, &own), ["arrow"]);
        // Without a known factory part there is no rule and no list; other kinds keep theirs.
        assert!(refs(&parts, &blade_slot(None)).is_empty());
        assert!(refs(&parts, &blade_slot(Some("unknown"))).is_empty());
        assert!(parts.bound_stock(&slot("hardpoint_cooler", "Cooler", None, Some("cool"))).is_none());
    }

    #[test]
    fn a_paint_slot_takes_only_the_paints_its_ship_family_requires() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let paint = |reference: &str, tags: &[&str], needs: &[&str]| part(reference, "Paints", 1, tags, needs);
        let parts = ErkulParts::from_parts(vec![
            paint("pirate", &["Paint_Gladius", "@GladiusPirate"], &["Paint_Gladius"]),
            paint("valiant", &["Paint_Gladius", "@GladiusValiant"], &["paint_gladius"]),
            paint("lightning", &["Paint_Lightning", "@LightningPlatinum"], &["Paint_Lightning"]),
            paint("free", &["Paint_Template"], &[]),
            paint("old", &["Paint_Gladius"], &[]),
        ]);
        let paint_slot = |factory: Option<&str>, fit: Option<Fit>| SlotInfo {
            size: None,
            ..slot("hardpoint_paint", "Paints", fit, factory)
        };
        // A Gladius with its factory paint: the paints that require the Gladius family, case aside; not the
        // Lightning's, not one that requires nothing.
        assert_eq!(refs(&parts, &paint_slot(Some("pirate"), None)), ["pirate", "valiant"]);
        // A factory paint whose requirement is unknown (a file of 5.2.0) keeps to itself.
        assert_eq!(refs(&parts, &paint_slot(Some("old"), None)), ["old"]);
        // No rule of its own and no factory paint: no list, not every paint.
        assert!(refs(&parts, &paint_slot(None, None)).is_empty());
        // The rule of the port (erkul's, or the ship Record's) decides where there is one: it offers and requires
        // the Lightning family, spelt in lower case.
        let lightning = vec!["paint_lightning".to_string()];
        let port = Fit {
            types: vec![("Paints".into(), Vec::new())],
            port_tags: lightning.clone(),
            required: lightning,
            ..Fit::default()
        };
        assert_eq!(refs(&parts, &paint_slot(None, Some(port))), ["lightning"]);
    }

    #[test]
    fn a_swapped_rack_brings_its_own_slots_and_drops_the_old_ones() {
        let bomb_port = PartPort { name: "bomb_01".into(), fit: fit("Bomb", 3, 3, &[], &[]) };
        let mut rack = part("rack-bomb", "BombLauncher", 3, &[], &[]);
        rack.ports = vec![bomb_port];
        rack.factory = vec![("bomb_01".into(), ("bomb".into(), "Bomb".into()))];
        let mut torpedo_rack = part("rack-torpedo", "MissileLauncher", 10, &[], &[]);
        torpedo_rack.ports = vec![PartPort { name: "missile_01".into(), fit: fit("Missile", 9, 9, &[], &[]) }];
        let parts = ErkulParts::from_parts(vec![rack, torpedo_rack]);
        let slots = vec![
            slot("rack", "MissileLauncher", None, Some("rack-torpedo")),
            slot("rack/missile_01", "Missile", None, Some("torp")),
            slot("other", "Cooler", None, Some("cooler")),
        ];
        // Nothing chosen: the slot in the factory rack gets its fit from the rack.
        let plain = with_choices(slots.clone(), &Map::new(), &parts);
        assert_eq!(plain.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["rack", "rack/missile_01", "other"]);
        assert_eq!(plain[1].fit, Some(fit("Missile", 9, 9, &[], &[])));
        // The bomb rack instead: its slot with its factory part, the old one gone, the neighbour kept.
        let chosen: Map = vec![("rack".to_string(), Value::Object(vec![("ref".to_string(), Value::str("rack-bomb"))]))];
        let swapped = with_choices(slots, &chosen, &parts);
        assert_eq!(swapped.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(), ["rack", "rack/bomb_01", "other"]);
        assert_eq!(swapped[1].factory, Some(("bomb".to_string(), "Bomb".to_string())));
        assert_eq!(swapped[1].kind, "Bomb");
        // Without any stored parts the list stays as it is.
        let bare = with_choices(vec![slot("rack", "Cooler", None, None)], &chosen, &ErkulParts::default());
        assert_eq!(bare.len(), 1);
    }

    /// [`fit_answer`] for a part without a maker.
    fn any_maker(ships: &ErkulShips, kind: &str, size: Option<i64>, hangar: &[&Map]) -> FitAnswer {
        fit_answer(ships, kind, size, (None, ""), hangar)
    }

    #[test]
    fn an_ore_pod_fits_only_the_ships_of_its_maker() {
        let file = std::env::temp_dir().join(format!("versekit-fit-pods-{}.json", crate::testdir::run_tag()));
        let stored = r#"{"format": 4, "spielversion": "4.1", "hersteller": {},
            "schiffe": {"argomole": {"id": "argo_mole", "name": "Argo MOLE",
                "plaetze": [{"art": "Container", "groesse": 1, "anzahl": 8}]},
              "miscprospector": {"id": "misc_prospector", "name": "MISC Prospector",
                "plaetze": [{"art": "Container", "groesse": 1, "anzahl": 2}]},
              "drakegolem": {"id": "drak_golem", "name": "Drake Golem",
                "plaetze": [{"art": "Container", "groesse": 1, "anzahl": 1}]}}}"#;
        std::fs::write(&file, stored).unwrap();
        let ships = ErkulShips::load(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        let entry = |text: &str| crate::jsonio::loads(text).unwrap().as_object().unwrap().clone();
        let mole = entry(r#"{"name": "MOLE", "hersteller": "ARGO Astronautics", "hkurz": "ARGO"}"#);
        let prospector = entry(r#"{"name": "Prospector", "hersteller": "MISC", "hkurz": "MISC"}"#);
        let golem = entry(r#"{"name": "Golem", "hersteller": "Drake Interplanetary", "hkurz": "DRAK"}"#);
        let hangar = [&mole, &prospector, &golem];
        let fits =
            |name: &str, maker: &str| match fit_answer(&ships, "Container", Some(1), (Some(maker), name), &hangar) {
                FitAnswer::In(found) => found.into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
                other => panic!("{name}: {other:?}"),
            };
        assert_eq!(fits("Argo Ore Pod", "Argo Astronautics"), ["MOLE"]);
        assert_eq!(fits("MISC Ore Pod", "Musashi Industrial & Starflight Concern"), ["Prospector"]);
        assert_eq!(fits("Drake Ore Pod", "Drake Interplanetary"), ["Golem"]);
        assert_eq!(fits("GOLEM MC-4 Ore Pod", "Drake Interplanetary"), ["Golem"]);
        // Made by Argo, named after MISC: both.
        assert_eq!(fits("MISC Enhanced Ore Pod", "Argo Astronautics"), ["MOLE", "Prospector"]);
        assert_eq!(fit_answer(&ships, "Container", Some(1), (None, ""), &hangar), FitAnswer::Nowhere, "no maker");
        // Other kinds keep fitting by kind and size alone.
        assert!(maker_allows("Cooler", Some("Aegis Dynamics"), "", &mole));
        assert!(!maker_allows("Container", Some("Aegis Dynamics"), "Aegis Pod", &mole));
    }

    #[test]
    fn the_answer_to_fits_my_ship_tells_the_four_cases_apart() {
        let file = std::env::temp_dir().join(format!("versekit-fit-answer-{}.json", crate::testdir::run_tag()));
        let stored = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"aegisdynamics": "aegs"},
            "schiffe": {"aegiseclipse": {"id": "aegs_eclipse", "name": "Eclipse",
                "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 3}, {"art": "Shield", "groesse": 1, "anzahl": 1}]},
              "aegisgladius": {"id": "aegs_gladius", "name": "Gladius",
                "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 1}]}}}"#;
        std::fs::write(&file, stored).unwrap();
        let ships = ErkulShips::load(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        let eclipse = crate::jsonio::loads(r#"{"name": "Eclipse", "hersteller": "Aegis Dynamics"}"#).unwrap();
        let gladius = crate::jsonio::loads(r#"{"name": "Gladius", "hersteller": "Aegis Dynamics"}"#).unwrap();
        let unknown = crate::jsonio::loads(r#"{"name": "Nobody", "hersteller": "Nowhere"}"#).unwrap();
        let (eclipse, gladius, unknown) =
            (eclipse.as_object().unwrap(), gladius.as_object().unwrap(), unknown.as_object().unwrap());

        let both = [eclipse, gladius];
        assert_eq!(
            any_maker(&ships, "Cooler", Some(2), &both),
            FitAnswer::In(vec![("Eclipse".to_string(), 3), ("Gladius".to_string(), 1)]),
            "the most slots first"
        );
        assert_eq!(any_maker(&ships, "Shield", Some(1), &both), FitAnswer::In(vec![("Eclipse".to_string(), 1)]));
        assert_eq!(any_maker(&ships, "Cooler", Some(4), &both), FitAnswer::Nowhere);
        assert_eq!(
            any_maker(&ships, "Cooler", Some(2), &[unknown]),
            FitAnswer::Nowhere,
            "a ship without data fits nothing"
        );
        // Nothing is said where there is nothing to say.
        assert_eq!(any_maker(&ships, "Cooler", None, &both), FitAnswer::Silent, "no size");
        assert_eq!(any_maker(&ships, "Cooler", Some(0), &both), FitAnswer::Silent, "size 0 is no size");
        assert_eq!(any_maker(&ships, "Char_Armor_Helmet", Some(1), &both), FitAnswer::Silent, "armour");
        assert_eq!(any_maker(&ships, "", Some(1), &both), FitAnswer::Silent);
        // An empty hangar is not "fits nowhere", and missing data is not either.
        assert_eq!(any_maker(&ships, "Cooler", Some(2), &[]), FitAnswer::EmptyHangar);
        assert_eq!(any_maker(&ErkulShips::default(), "Cooler", Some(2), &both), FitAnswer::NoData);
        assert_eq!(any_maker(&ErkulShips::default(), "Cooler", Some(2), &[]), FitAnswer::EmptyHangar);
    }

    #[test]
    fn the_slot_table_of_a_ship_counts_as_fits_does_and_keeps_silent_where_fit_answer_does() {
        let file = std::env::temp_dir().join(format!("versekit-slot-table-{}.json", crate::testdir::run_tag()));
        let stored = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"aegisdynamics": "aegs"},
            "schiffe": {"aegiseclipse": {"id": "aegs_eclipse", "name": "Eclipse",
                "plaetze": [{"art": "Cooler", "groesse": 2, "anzahl": 3}, {"art": "Shield", "groesse": 1, "anzahl": 1},
                            {"art": "Cooler", "groesse": 2, "anzahl": 1}, {"art": "Battery", "groesse": 1, "anzahl": 2}]},
              "aegisgladius": {"id": "aegs_gladius", "name": "Gladius", "plaetze": []}}}"#;
        std::fs::write(&file, stored).unwrap();
        let ships = ErkulShips::load(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        let entry = |json: &str| crate::jsonio::loads(json).unwrap().as_object().unwrap().clone();
        let eclipse = entry(r#"{"name": "Eclipse", "hersteller": "Aegis Dynamics"}"#);
        let gladius = entry(r#"{"name": "Gladius", "hersteller": "Aegis Dynamics"}"#);
        let unknown = entry(r#"{"name": "Nobody", "hersteller": "Nowhere"}"#);

        let table = SlotTable::of(&ships, ship_id(&eclipse)).expect("the Eclipse has slots");
        assert_eq!(table.takes("Cooler", Some(2)), 4, "two rows of the same kind and size add up");
        assert_eq!(table.takes("Shield", Some(1)), 1);
        assert_eq!(table.takes("Cooler", Some(1)), 0, "the size has to match exactly");
        assert_eq!(table.takes("Cooler", None), 0);
        assert_eq!(table.takes("Cooler", Some(0)), 0);
        assert_eq!(table.takes("Battery", Some(1)), 0, "no kind outside INTERESTING");
        // Every answer agrees with the one of `fits` behind "fits into your ship".
        for (kind, size) in [("Cooler", Some(2)), ("Shield", Some(1)), ("Cooler", Some(3)), ("Radar", Some(1))] {
            assert_eq!(table.takes(kind, size), fits(&ships, kind, size, ship_id(&eclipse)), "{kind} {size:?}");
        }
        // A ship with no counted slots and a ship erkul does not know have no table: no data, not "nothing fits".
        assert_eq!(SlotTable::of(&ships, ship_id(&gladius)), None);
        assert_eq!(SlotTable::of(&ships, ship_id(&unknown)), None);
        assert_eq!(SlotTable::of(&ErkulShips::default(), ship_id(&eclipse)), None);
    }

    /// Some ships keep their components only in the fitted slots, not in the counts (the F7C-M Super Hornet Mk II
    /// of the stored file): they count all the same, and where both list a kind the larger number counts.
    #[test]
    fn components_only_in_the_fitted_slots_count_too() {
        let file = std::env::temp_dir().join(format!("versekit-slot-fitted-{}.json", crate::testdir::run_tag()));
        let stored = r#"{"format": 4, "spielversion": "4.1", "hersteller": {"anvilaerospace": "anvl"},
            "schiffe": {"anvilhornet": {"id": "anvl_hornet", "name": "Hornet",
                "plaetze": [{"art": "WeaponGun", "groesse": 3, "anzahl": 2}, {"art": "Cooler", "groesse": 1, "anzahl": 3}],
                "slots": [{"art": "Cooler", "groesse": 1}, {"art": "Cooler", "groesse": 1},
                          {"art": "PowerPlant", "groesse": 2}, {"art": "Shield", "groesse": 1}, {"art": "Shield", "groesse": 1},
                          {"art": "QuantumDrive", "groesse": 1}, {"art": "WeaponGun", "groesse": 3}]}}}"#;
        std::fs::write(&file, stored).unwrap();
        let ships = ErkulShips::load(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        let hornet = crate::jsonio::loads(r#"{"name": "Hornet", "hersteller": "Anvil Aerospace"}"#)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        let table = SlotTable::of(&ships, ship_id(&hornet)).expect("the Hornet has slots");
        assert_eq!(table.takes("PowerPlant", Some(2)), 1, "only in the fitted slots");
        assert_eq!(table.takes("Shield", Some(1)), 2);
        assert_eq!(table.takes("QuantumDrive", Some(1)), 1);
        assert_eq!(table.takes("Cooler", Some(1)), 3, "both list it: the larger number, not the sum");
        assert_eq!(table.takes("WeaponGun", Some(3)), 2);
        assert_eq!(fits(&ships, "PowerPlant", Some(2), ship_id(&hornet)), 1, "fits into your ship says the same");
        let names: Vec<String> =
            matching_ships(&ships, "Shield", Some(1), &[&hornet]).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Hornet"]);
    }

    #[test]
    fn only_the_kinds_of_4_12_get_an_answer() {
        assert_eq!(INTERESTING.len(), 26);
        assert!(
            INTERESTING.contains(&"Container")
                && INTERESTING.contains(&"Cargo")
                && INTERESTING.contains(&"MiningModifier")
        );
        assert!(!INTERESTING.contains(&"Battery"), "the shopping list reads more kinds than this list");
    }
}
