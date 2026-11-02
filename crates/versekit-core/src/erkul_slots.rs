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
use crate::ships::{ErkulShips, ShipId, stored};

pub const PARTS_FILE: &str = "erkul-teile.json";
pub const PARTS_FORMAT: i64 = 2;

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
        }
    }
}

impl ErkulParts {
    /// Reads the stored parts file; empty without a usable one.
    pub fn load(path: &str) -> ErkulParts {
        let root = stored(path, PARTS_FORMAT);
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
                });
            }
        }
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

    /// `erkul.compatible`: the stored parts that fit this slot.
    pub fn compatible(&self, slot: &SlotInfo) -> Vec<&Part> {
        let Some(fit) = &slot.fit else { return Vec::new() };
        self.parts.iter().filter(|p| fits_slot(fit, p)).collect()
    }
}

/// `erkul.fits_slot`: does the part go into the slot? Four conditions, all must hold: kind and subkind
/// are in the list of the slot; the size of the part lies between the smallest and largest of the slot
/// (not equal to the slot size: the bomb rack S3 sits in a slot of size 10); what the part requires, the
/// slot offers; what the slot requires, the part carries.
///
/// ⚠ Without the last two the torpedo slot of the Eclipse would offer the racks of every other ship:
/// 126 instead of 4.
pub fn fits_slot(fit: &Fit, part: &Part) -> bool {
    if !fit.types.iter().any(|(kind, subs)| *kind == part.kind && (subs.is_empty() || subs.contains(&part.subkind))) {
        return false;
    }
    if let Some(size) = part.size
        && (fit.min.is_some_and(|low| size < low) || fit.max.is_some_and(|high| size > high))
    {
        return false;
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
    if parts.is_empty() {
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
        out.push(slot);
        let pick = pick_of(&path);
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
    /// The two files in the data folder.
    pub fn files(paths: &Paths) -> [String; 2] {
        [paths.app_file(crate::ships::ERKUL_FILE), paths.app_file(PARTS_FILE)]
    }

    pub fn load(paths: &Paths) -> ErkulData {
        let [ships, parts] = ErkulData::files(paths);
        ErkulData { ships: ErkulShips::load(&ships), parts: ErkulParts::load(&parts) }
    }

    /// The name of a part for the plan (the identifier where erkul does not know it).
    pub fn part_name(&self, reference: &str) -> String {
        self.parts.get(reference).map_or_else(|| reference.to_string(), |p| p.name.clone())
    }

    /// The paths of all slots of a ship, as erkul spells them.
    pub fn slot_paths(&self, ship: &Map) -> Vec<String> {
        hardpoints(&self.ships, &self.parts, ship_id(ship), &Map::new()).into_iter().map(|s| s.path).collect()
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
    /// (`cart.line_items` reads `erkul.hardpoints(name, maker, short, maker short, chosen)`).
    pub fn cart_slots(&self, ship: &Map, chosen: &Map) -> Vec<crate::cart::Slot> {
        hardpoints(&self.ships, &self.parts, ship_id(ship), chosen)
            .into_iter()
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

/// `erkul.fits`: how many slots of this kind and size the ship has; 0 where none fits or the ship is unknown.
pub fn fits(ships: &ErkulShips, kind: &str, size: Option<i64>, id: ShipId) -> i64 {
    let Some(size) = size.filter(|_| !kind.is_empty()) else { return 0 };
    let Some(entry) = ships.entry(id) else { return 0 };
    let Some(Value::Array(counted)) = get(entry, "plaetze") else { return 0 };
    counted
        .iter()
        .filter_map(Value::as_object)
        .filter(|p| text(p, "art") == kind && py_int(get(p, "groesse")).unwrap_or(-1) == size)
        .map(|p| py_int(get(p, "anzahl")).unwrap_or(0))
        .sum()
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

/// The part (`kind`, `size` as the blueprint catalog has them) against the hangar.
pub fn fit_answer(ships: &ErkulShips, kind: &str, size: Option<i64>, hangar: &[&Map]) -> FitAnswer {
    if kind.is_empty() || size.is_none_or(|s| s == 0) || !INTERESTING.contains(&kind) {
        return FitAnswer::Silent;
    }
    if hangar.is_empty() {
        return FitAnswer::EmptyHangar;
    }
    if ships.is_empty() {
        return FitAnswer::NoData;
    }
    let found = matching_ships(ships, kind, size, hangar);
    if found.is_empty() { FitAnswer::Nowhere } else { FitAnswer::In(found) }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            fit_answer(&ships, "Cooler", Some(2), &both),
            FitAnswer::In(vec![("Eclipse".to_string(), 3), ("Gladius".to_string(), 1)]),
            "the most slots first"
        );
        assert_eq!(fit_answer(&ships, "Shield", Some(1), &both), FitAnswer::In(vec![("Eclipse".to_string(), 1)]));
        assert_eq!(fit_answer(&ships, "Cooler", Some(4), &both), FitAnswer::Nowhere);
        assert_eq!(
            fit_answer(&ships, "Cooler", Some(2), &[unknown]),
            FitAnswer::Nowhere,
            "a ship without data fits nothing"
        );
        // Nothing is said where there is nothing to say.
        assert_eq!(fit_answer(&ships, "Cooler", None, &both), FitAnswer::Silent, "no size");
        assert_eq!(fit_answer(&ships, "Cooler", Some(0), &both), FitAnswer::Silent, "size 0 is no size");
        assert_eq!(fit_answer(&ships, "Char_Armor_Helmet", Some(1), &both), FitAnswer::Silent, "armour");
        assert_eq!(fit_answer(&ships, "", Some(1), &both), FitAnswer::Silent);
        // An empty hangar is not "fits nowhere", and missing data is not either.
        assert_eq!(fit_answer(&ships, "Cooler", Some(2), &[]), FitAnswer::EmptyHangar);
        assert_eq!(fit_answer(&ErkulShips::default(), "Cooler", Some(2), &both), FitAnswer::NoData);
        assert_eq!(fit_answer(&ErkulShips::default(), "Cooler", Some(2), &[]), FitAnswer::EmptyHangar);
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
