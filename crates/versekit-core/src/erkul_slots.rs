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
    /// The kind of parts the stored ones lack, by identifier: what the shops list (a gun has no slots of its own,
    /// so the game data do not keep it among the parts).
    kinds: HashMap<String, String>,
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
        ErkulParts { parts, index, kinds: HashMap::new() }
    }

    /// Adds the kinds of parts by identifier, `(identifier, kind)`, as the shops list them.
    pub fn add_kinds(&mut self, kinds: impl IntoIterator<Item = (String, String)>) {
        self.kinds.extend(kinds.into_iter().filter(|(reference, kind)| !reference.is_empty() && !kind.is_empty()));
    }

    /// The kind of a part: the stored part's, else the one added by [`ErkulParts::add_kinds`].
    pub fn kind_of(&self, reference: &str) -> Option<&str> {
        match self.get(reference) {
            Some(part) => Some(part.kind.as_str()),
            None => self.kinds.get(reference).map(String::as_str),
        }
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

/// How deep the slots a chosen part brings are followed (turret → gimbal mount → gun is three).
const MAX_DEPTH: usize = 8;

/// What the plan does to the slots below a slot.
enum Below<'p> {
    /// They stay as listed; `Some(part)` is the part they sit in, whose ports give them what fits.
    Listed(Option<&'p Part>),
    /// They go: the slot is left empty, or another part sits in it (whose own slots were added).
    Replaced,
}

/// The own choice at a path of the plan.
fn pick_of(chosen: &Map, path: &str) -> Option<String> {
    let part = get(chosen, path)?.as_object()?;
    let reference = text(part, "ref");
    (!reference.is_empty()).then(|| reference.to_string())
}

/// Puts `slot` into `out` and says what the plan does to the slots below it. Another part than the stock one
/// replaces them only where it is known what the chosen part holds: its own slots where the parts know it with
/// slots, none where it is known as a part without slots (a gun in place of a gimbal mount, also a gun the parts
/// do not know but the shops list as one). A gimbal mount the parts know without its gun port, or a part of
/// which nothing is known (a ware of the shops without an identifier of the game, `id:5566`), leaves the slots
/// below as listed. `trail` holds the parts the slot sits in, so that a part that holds itself ends there.
fn fill<'p>(
    slot: SlotInfo,
    chosen: &Map,
    parts: &'p ErkulParts,
    out: &mut Vec<SlotInfo>,
    trail: &mut Vec<String>,
) -> Below<'p> {
    let path = slot.path.clone();
    let stock = slot.factory.as_ref().map(|(reference, _)| reference.clone()).filter(|r| !r.is_empty());
    let fixed = slot.fixed;
    out.push(slot);
    // A slot left empty holds nothing, so nothing hangs below it; a fixed slot is never left empty.
    if !fixed && get(chosen, &path).and_then(Value::as_object).is_some_and(crate::cart::is_emptied) {
        return Below::Replaced;
    }
    let holder = stock.as_deref().and_then(|reference| parts.get(reference)).filter(|part| !part.ports.is_empty());
    let pick = if fixed { None } else { pick_of(chosen, &path) };
    let Some(pick) = pick.filter(|pick| Some(pick) != stock.as_ref()) else { return Below::Listed(holder) };
    match parts.get(&pick) {
        Some(part) if !part.ports.is_empty() => {
            add_ports(part, &path, chosen, parts, out, trail);
            Below::Replaced
        }
        Some(part) if !is_gimbal(part) => Below::Replaced,
        None if parts.kind_of(&pick) == Some("WeaponGun") => Below::Replaced,
        _ => Below::Listed(holder),
    }
}

/// The slots `part` brings below `path`, each with what sits in it at the factory (or by the plan), and below
/// those the slots their parts bring: a ball turret chosen brings its gimbal mounts, and each mount its gun. A part
/// already in `trail` (one that holds itself) brings nothing more, nor does one deeper than [`MAX_DEPTH`].
fn add_ports(part: &Part, path: &str, chosen: &Map, parts: &ErkulParts, out: &mut Vec<SlotInfo>, trail: &mut Vec<String>) {
    if trail.len() >= MAX_DEPTH || trail.contains(&part.reference) {
        return;
    }
    trail.push(part.reference.clone());
    for port in &part.ports {
        let kind = port.fit.types.iter().map(|(t, _)| t.as_str()).find(|t| SWAPPABLE.contains(t)).unwrap_or("");
        let slot = SlotInfo {
            path: format!("{path}/{}", port.name),
            kind: kind.to_string(),
            size: port.fit.max,
            fit: Some(port.fit.clone()),
            factory: part.factory.iter().find(|(name, _)| *name == port.name).map(|(_, pair)| pair.clone()),
            fixed: false,
        };
        let below = slot.path.clone();
        if let Below::Listed(Some(inner)) = fill(slot, chosen, parts, out, trail) {
            add_ports(inner, &below, chosen, parts, out, trail);
        }
    }
    trail.pop();
}

/// `erkul.with_choices`: the slots after the own choices. A different part brings different slots: the slots
/// below it are replaced by those of the chosen part with its factory fitting, all the way down (the gimbal
/// mounts of a turret and their guns), and a gun chosen in place of a gimbal mount has no slot below it.
/// `chosen` is the equipment of the ship, path → `{ref, …}`.
pub fn with_choices(slots: Vec<SlotInfo>, chosen: &Map, parts: &ErkulParts) -> Vec<SlotInfo> {
    if parts.is_empty() && !chosen.iter().any(|(_, v)| v.as_object().is_some_and(crate::cart::is_emptied)) {
        return slots;
    }
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
        match fill(slot, chosen, parts, &mut out, &mut Vec::new()) {
            Below::Listed(Some(rack)) => {
                racks.insert(path, rack);
            }
            Below::Listed(None) => {}
            Below::Replaced => gone.push(format!("{path}/")),
        }
    }
    out
}

/// Whether a part is a gimbal mount: a turret that holds one gun and goes where a gun goes (`gimbalMount`).
pub fn is_gimbal(part: &Part) -> bool {
    part.kind == "Turret" && part.tags.iter().any(|tag| tag.eq_ignore_ascii_case("gimbalMount"))
}

/// The kinds a slot takes besides its own, `(kind, subkinds)`: those its rule names (the port of a gimbal mount
/// takes a gimbal mount or a gun right away), and for a slot without a rule whose factory part is a gimbal mount,
/// a gun right away.
pub fn other_kinds(slot: &SlotInfo, parts: &ErkulParts) -> Vec<(String, Vec<String>)> {
    if let Some(fit) = &slot.fit {
        return fit.types.iter().filter(|(kind, _)| *kind != slot.kind && SWAPPABLE.contains(&kind.as_str())).cloned().collect();
    }
    let gimbal = slot.factory.as_ref().and_then(|(reference, _)| parts.get(reference)).is_some_and(is_gimbal);
    if gimbal && slot.kind == "Turret" { vec![("WeaponGun".to_string(), vec!["Gun".to_string()])] } else { Vec::new() }
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

    /// Puts the plan (`belegung`) of a hangar entry right against the slots of its ship. Where the slots of the
    /// ship are not known nothing is changed. True when the plan changed.
    ///
    /// - One part planned in a gimbal mount and in the gun port below it (what one line of 5.2.x put into both)
    ///   stays in the gun port only, in the plan and in the active equipment: the gimbal mount keeps its factory
    ///   part.
    /// - An entry whose slot the ship does not have, after the plan, is marked [`crate::cart::ORPHANED`] and kept;
    ///   one whose slot is there again loses the mark.
    pub fn tidy_plan(&self, entry: &mut Map) -> bool {
        let stock = hardpoints(&self.ships, &self.parts, ship_id(entry), &Map::new());
        if stock.is_empty() {
            return false;
        }
        let Some(Value::Object(plan)) = get(entry, "belegung") else { return false };
        let mut plan = plan.clone();
        let listed = |path: &str| stock.iter().any(|slot| slot.path == path);
        let reference_at = |plan: &Map, path: &str| pick_of(plan, path).unwrap_or_default();
        let doubled: Vec<String> = plan
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| {
                let reference = reference_at(&plan, path);
                let gimbal = stock.iter().find(|slot| slot.path == *path).and_then(|slot| slot.factory.as_ref()).is_some_and(
                    |(stock_ref, name)| self.parts.get(stock_ref).is_some_and(is_gimbal) || name.to_lowercase().contains("gimbal"),
                );
                let below = plan.iter().any(|(other, _)| {
                    other.strip_prefix(path.as_str()).is_some_and(|rest| rest.starts_with('/'))
                        && listed(other)
                        && reference_at(&plan, other) == reference
                });
                !reference.is_empty() && gimbal && below
            })
            .collect();
        let mut changed = !doubled.is_empty();
        plan.retain(|(path, _)| !doubled.contains(path));
        let slots: Vec<String> = hardpoints(&self.ships, &self.parts, ship_id(entry), &plan)
            .into_iter()
            .filter(|slot| !slot.fixed)
            .map(|slot| slot.path)
            .collect();
        for (path, part) in &mut plan {
            let Value::Object(fields) = part else { continue };
            if text(fields, "ref").is_empty() {
                continue;
            }
            let gone = !slots.contains(path);
            let marked = crate::hangar::truthy(get(fields, crate::cart::ORPHANED));
            if gone && !marked {
                crate::jsonio::map_set(fields, crate::cart::ORPHANED, Value::Bool(true));
                changed = true;
            } else if !gone && marked {
                fields.retain(|(key, _)| key != crate::cart::ORPHANED);
                changed = true;
            }
        }
        if changed {
            crate::jsonio::map_set(entry, "belegung", Value::Object(plan));
        }
        // The active equipment follows the plan without the doubled entries.
        if !doubled.is_empty() {
            let stock_ports = self.stock_ports(entry);
            crate::equipment::sync_entry(entry, &stock_ports);
        }
        changed
    }

    /// [`ErkulData::tidy_plan`] for every ship of the hangar and of the wish list in `data`; true when one changed.
    pub fn tidy_plans(&self, data: &mut Map) -> bool {
        let mut changed = false;
        for key in ["schiffe", "wunsch"] {
            let Some((_, Value::Array(items))) = data.iter_mut().find(|(k, _)| k == key) else { continue };
            for item in items.iter_mut() {
                if let Value::Object(entry) = item {
                    changed |= self.tidy_plan(entry);
                }
            }
        }
        changed
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

    /// A port of a gimbal mount as the game data give it: a gun or a gimbal mount of size `size`.
    fn gimbal_port(size: i64) -> Fit {
        Fit {
            types: vec![("WeaponGun".into(), vec!["Gun".into()]), ("Turret".into(), vec!["GunTurret".into()])],
            min: Some(size),
            max: Some(size),
            ..Fit::default()
        }
    }

    /// A gimbal mount of the game data: one gun port of its size, empty at the factory.
    fn gimbal(reference: &str, name: &str, size: i64) -> Part {
        Part {
            reference: reference.into(),
            name: name.into(),
            kind: "Turret".into(),
            subkind: "GunTurret".into(),
            size: Some(size),
            tags: vec!["gimbalMount".into(), "flightReady".into()],
            ports: vec![PartPort { name: "hardpoint_class_2".into(), fit: fit("WeaponGun", size, size, &[], &[]) }],
            ..Part::default()
        }
    }

    /// The remote turret of the Shiv as the game data give it: four gimbal mounts, each with one gun.
    fn shiv_turret() -> Vec<SlotInfo> {
        let mut slots = vec![SlotInfo { size: Some(3), ..slot("hardpoint_turret", "Turret", None, Some("remote")) }];
        for side in ["right", "left", "top_right", "top_left"] {
            let mount = format!("hardpoint_turret/hardpoint_weapon_{side}");
            slots.push(SlotInfo { fit: Some(gimbal_port(3)), ..slot(&mount, "WeaponGun", None, Some("varipuck3")) });
            let gun = format!("{mount}/hardpoint_class_2");
            slots.push(slot(&gun, "WeaponGun", Some(fit("WeaponGun", 3, 3, &[], &[])), Some("tormenter")));
        }
        slots
    }

    fn plan(json: &str) -> Map {
        crate::jsonio::loads(json).ok().and_then(|v| v.as_object().cloned()).expect("plan")
    }

    #[test]
    fn a_gun_chosen_in_place_of_a_gimbal_mount_has_no_slot_below_it_and_another_mount_brings_its_own() {
        let mut parts =
            ErkulParts::from_parts(vec![gimbal("varipuck3", "VariPuck S3 Gimbal Mount", 3), gimbal("other3", "Other S3 Gimbal", 3)]);
        // The shops list the M5A as a gun; the game data do not keep it among the parts.
        parts.add_kinds([("m5a".to_string(), "WeaponGun".to_string()), ("odd".to_string(), "Turret".to_string())]);
        let paths = |slots: &[SlotInfo]| slots.iter().map(|s| s.path.clone()).collect::<Vec<_>>();
        let stock = with_choices(shiv_turret(), &Map::new(), &parts);
        assert_eq!(stock.len(), 9, "the turret, four mounts and four guns: {:?}", paths(&stock));
        // A gun right in the left mount: the gun that sat in the gimbal mount goes, the others stay.
        let gun = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "m5a", "name": "M5A Cannon"}}"#);
        let after = paths(&with_choices(shiv_turret(), &gun, &parts));
        assert!(after.contains(&"hardpoint_turret/hardpoint_weapon_left".to_string()), "{after:?}");
        assert!(!after.contains(&"hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2".to_string()), "{after:?}");
        assert!(after.contains(&"hardpoint_turret/hardpoint_weapon_right/hardpoint_class_2".to_string()), "{after:?}");
        assert_eq!(after.len(), 8);
        // A part the parts know without slots of its own (a gun of erkul's list) takes the gun slot away too.
        let known = ErkulParts::from_parts(parts.all().iter().cloned().chain([part("cf337", "WeaponGun", 3, &[], &[])]).collect());
        let cf = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "cf337", "name": "CF-337"}}"#);
        assert_eq!(with_choices(shiv_turret(), &cf, &known).len(), 8);
        // Nothing says what an unknown part is, or it is no gun: the slots below stay.
        let unknown_pick = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "mystery", "name": "?"}}"#);
        assert_eq!(with_choices(shiv_turret(), &unknown_pick, &parts).len(), 9, "unknown and of no known kind");
        let odd = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "odd", "name": "Odd Turret"}}"#);
        assert_eq!(with_choices(shiv_turret(), &odd, &parts).len(), 9, "a turret the parts do not know");
        // A gimbal mount the parts know without its gun port (erkul's list): the gun slot stays.
        let bare = Part { ports: Vec::new(), ..gimbal("bare3", "Bare S3 Gimbal", 3) };
        let with_bare = ErkulParts::from_parts(parts.all().iter().cloned().chain([bare]).collect());
        let other_gimbal = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "bare3", "name": "Bare S3 Gimbal"}}"#);
        let kept = paths(&with_choices(shiv_turret(), &other_gimbal, &with_bare));
        assert!(kept.contains(&"hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2".to_string()), "{kept:?}");
        // Another gimbal mount brings its own gun slot, empty at the factory.
        let other = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "other3", "name": "Other S3 Gimbal"}}"#);
        let slots = with_choices(shiv_turret(), &other, &parts);
        let inner = slots.iter().find(|s| s.path == "hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2").expect("its gun slot");
        assert_eq!((inner.kind.as_str(), inner.size, inner.factory.clone()), ("WeaponGun", Some(3), None));
        // Neither part known (no game data): the slots below stay as listed.
        let unknown = ErkulParts::from_parts(vec![part("cooler", "Cooler", 1, &[], &[])]);
        assert_eq!(with_choices(shiv_turret(), &gun, &unknown).len(), 9);
        // A ware of the shops without an identifier of the game can be anything: the slots below stay.
        let ware = plan(r#"{"hardpoint_turret/hardpoint_weapon_left": {"ref": "id:5566", "name": "SNS-R7X"}}"#);
        assert_eq!(with_choices(shiv_turret(), &ware, &parts).len(), 9);
        // The port of the gimbal mount takes a gimbal mount besides a gun; a wing slot holding a gimbal mount a gun.
        assert_eq!(other_kinds(&shiv_turret()[1], &parts), [("Turret".to_string(), vec!["GunTurret".to_string()])]);
        let wing = SlotInfo { size: Some(3), ..slot("hardpoint_weapon_left_wing", "Turret", None, Some("varipuck3")) };
        assert_eq!(other_kinds(&wing, &parts), [("WeaponGun".to_string(), vec!["Gun".to_string()])]);
        assert!(other_kinds(&shiv_turret()[0], &parts).is_empty(), "the remote turret is no gimbal mount");
    }

    /// The slots of the Shiv's turret ([`shiv_turret`]) stored as the game data store them, with the parts.
    fn shiv_data(name: &str) -> ErkulData {
        let fit = |kinds: &str| format!(r#"{{"typen": {kinds}, "min": 3, "max": 3, "porttags": [], "verlangt": []}}"#);
        let mut slots = vec![r#"{"pfad": "hardpoint_turret", "art": "Turret", "groesse": 3, "werk": {"ref": "remote", "name": "Remote Turret"}}"#.to_string()];
        for side in ["right", "left", "top_right", "top_left"] {
            let mount = format!("hardpoint_turret/hardpoint_weapon_{side}");
            slots.push(format!(
                r#"{{"pfad": "{mount}", "art": "WeaponGun", "groesse": 3, "passt": {}, "werk": {{"ref": "varipuck3", "name": "VariPuck S3 Gimbal Mount"}}}}"#,
                fit(r#"[["WeaponGun", ["Gun"]], ["Turret", ["GunTurret"]]]"#)
            ));
            slots.push(format!(
                r#"{{"pfad": "{mount}/hardpoint_class_2", "art": "WeaponGun", "groesse": 3, "passt": {}, "werk": {{"ref": "tormenter", "name": "Tormenter S3 Repeater"}}}}"#,
                fit(r#"[["WeaponGun", ["Gun"]]]"#)
            ));
        }
        let stored = format!(
            r#"{{"format": 4, "spielversion": "4.1", "hersteller": {{}}, "schiffe": {{"glsnshiv": {{"id": "GLSN_Shiv", "name": "Shiv", "plaetze": [], "slots": [{}]}}}}}}"#,
            slots.join(", ")
        );
        let file = std::env::temp_dir().join(format!("versekit-shiv-{name}-{}.json", crate::testdir::run_tag()));
        std::fs::write(&file, stored).expect("written");
        let ships = ErkulShips::load(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        let mut parts = ErkulParts::from_parts(vec![gimbal("varipuck3", "VariPuck S3 Gimbal Mount", 3)]);
        parts.add_kinds([("m5a".to_string(), "WeaponGun".to_string()), ("cf337".to_string(), "WeaponGun".to_string())]);
        ErkulData { ships, parts }
    }

    /// The bill of one ship over the stored slots, with nothing priced.
    struct Bill<'a>(&'a ErkulData);

    impl crate::cart::Sources for Bill<'_> {
        fn shop_known(&self, _reference: &str) -> bool {
            false
        }
        fn cheapest(&self, _reference: &str) -> Option<(f64, String, String)> {
            None
        }
        fn shops_for(&self, _reference: &str) -> Vec<crate::cart::Offer> {
            Vec::new()
        }
        fn raw_buy_price(&self, _material: &str) -> f64 {
            0.0
        }
        fn recipe_for(&self, _reference: &str, _name: &str) -> Option<crate::cart::Recipe> {
            None
        }
        fn ship_offers(&self, _name: &str) -> Vec<crate::cart::Offer> {
            Vec::new()
        }
        fn slots(&self, ship: &Map, chosen: &Map) -> Vec<crate::cart::Slot> {
            self.0.cart_slots(ship, chosen)
        }
    }

    /// The paths of the plan of `entry`.
    fn plan_paths(entry: &Map) -> Vec<String> {
        match get(entry, "belegung") {
            Some(Value::Object(plan)) => plan.iter().map(|(path, _)| path.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// A ship saved by 5.2.4 with one gun in all eight lines of the Shiv's turret (one line of 5.2.x put it into the
    /// four gimbal mounts and the four gun ports below them): read against the slots, the gun stays in the gun ports
    /// only, in the plan and in the equipment, and the gimbal mounts keep their factory part. The hangar counts four
    /// parts to get, the bill has four, and all four ticked the ship is fitted. Without the slots nothing changes.
    #[test]
    fn a_plan_of_5_2_4_with_a_gun_in_the_mounts_and_below_them_keeps_the_gimbal_mounts_and_counts_four() {
        let data = shiv_data("alt");
        let mut planned = Vec::new();
        let mut slots = Vec::new();
        for side in ["right", "left", "top_right", "top_left"] {
            for path in [format!("hardpoint_turret/hardpoint_weapon_{side}"), format!("hardpoint_turret/hardpoint_weapon_{side}/hardpoint_class_2")] {
                planned.push(format!(r#""{path}": {{"ref": "m5a", "name": "M5A Cannon", "weg": "kaufen"}}"#));
                slots.push(format!(r#"{{"portPath": "{path}", "itemGuid": "m5a", "acquisition": "purchased"}}"#));
            }
        }
        let saved = format!(
            r#"{{"name": "Shiv", "kurz": "GLSN_Shiv", "belegung": {{{}}}, "ausstattungen": [{{"name": "Standard", "active": true, "slots": [{}]}}]}}"#,
            planned.join(", "),
            slots.join(", ")
        );
        let mut entry = plan(&saved);
        assert_eq!(crate::equipment::read_all(&entry, &[]).active().map(|e| e.slots.len()), Some(8), "it reads");
        // Without the slots of the ship nothing is touched.
        let mut untouched = entry.clone();
        assert!(!ErkulData::default().tidy_plan(&mut untouched));
        assert!(crate::jsonio::py_eq_value(&Value::Object(untouched), &Value::Object(entry.clone())));
        assert!(data.tidy_plan(&mut entry));
        let paths = plan_paths(&entry);
        assert_eq!(paths.len(), 4);
        assert!(paths.iter().all(|p| p.ends_with("/hardpoint_class_2")), "{paths:?}");
        assert_eq!(crate::equipment::read_all(&entry, &[]).active().map(|e| e.slots.len()), Some(4), "the equipment follows");
        // Shown: each gimbal mount with its factory part, the gun below it.
        let chosen = match get(&entry, "belegung") {
            Some(Value::Object(plan)) => plan.clone(),
            _ => panic!("the plan"),
        };
        let shown = hardpoints(&data.ships, &data.parts, ship_id(&entry), &chosen);
        assert_eq!(shown.len(), 9);
        let mount = shown.iter().find(|s| s.path == "hardpoint_turret/hardpoint_weapon_left").expect("the mount");
        assert_eq!(mount.factory.as_ref().map(|(_, n)| n.as_str()), Some("VariPuck S3 Gimbal Mount"));
        assert!(pick_of(&chosen, &mount.path).is_none(), "the mount keeps its factory part");
        assert_eq!(pick_of(&chosen, "hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2").as_deref(), Some("m5a"));
        // Counted: four to get, four on the bill; all ticked, the ship is fitted.
        assert_eq!(crate::hangar::open_count(&entry), 4);
        assert!(!crate::hangar::fully_fitted(&entry));
        assert_eq!(crate::cart::line_items(&entry, &Bill(&data)).1.len(), 4);
        assert_eq!(crate::hangar::tick_all(&mut entry), 4);
        assert!(crate::hangar::fully_fitted(&entry));
        assert!(!data.tidy_plan(&mut entry), "read once more, nothing changes");
    }

    /// An entry of the plan whose slot the ship does not have (a gun in a gun port below a mount that now holds a
    /// gun itself) is kept and marked: the hangar does not count it, nothing ticks it off, the bill passes it by.
    /// Where the slot is there again, the mark goes.
    #[test]
    fn an_entry_without_its_slot_is_kept_but_never_counted_as_open() {
        let data = shiv_data("verwaist");
        let mut entry = plan(
            r#"{"name": "Shiv", "kurz": "GLSN_Shiv", "belegung": {
                "hardpoint_turret/hardpoint_weapon_left": {"ref": "m5a", "name": "M5A Cannon", "weg": "kaufen"},
                "hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2": {"ref": "cf337", "name": "CF-337", "weg": "kaufen"}}}"#,
        );
        assert_eq!(crate::hangar::open_count(&entry), 2, "before the slots are read");
        assert!(data.tidy_plan(&mut entry));
        assert_eq!(plan_paths(&entry).len(), 2, "nothing thrown away");
        assert_eq!(crate::hangar::open_count(&entry), 1);
        assert_eq!(crate::cart::line_items(&entry, &Bill(&data)).1.len(), 1);
        assert_eq!(crate::hangar::tick_all(&mut entry), 1, "the entry without its slot is not ticked");
        assert!(crate::hangar::fully_fitted(&entry));
        // The mount back at the factory: the gun port is there again and its entry counts again.
        if let Some((_, Value::Object(plan))) = entry.iter_mut().find(|(k, _)| k == "belegung") {
            plan.retain(|(path, _)| path != "hardpoint_turret/hardpoint_weapon_left");
        }
        assert!(data.tidy_plan(&mut entry));
        assert_eq!(crate::hangar::open_count(&entry), 1);
        assert!(!crate::hangar::fully_fitted(&entry));
    }

    /// A part whose port holds the part itself at the factory brings its slots once, not without end.
    #[test]
    fn a_part_that_holds_itself_brings_its_slots_once() {
        let mut looped = part("loop", "Turret", 3, &[], &[]);
        looped.ports = vec![
            PartPort { name: "a".into(), fit: fit("Turret", 3, 3, &[], &[]) },
            PartPort { name: "b".into(), fit: fit("Turret", 3, 3, &[], &[]) },
        ];
        looped.factory = vec![("a".into(), ("loop".into(), "Loop".into())), ("b".into(), ("loop".into(), "Loop".into()))];
        let parts = ErkulParts::from_parts(vec![looped]);
        let chosen = plan(r#"{"top": {"ref": "loop", "name": "Loop"}}"#);
        let found = with_choices(vec![slot("top", "Turret", None, Some("stock"))], &chosen, &parts);
        let paths: Vec<&str> = found.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["top", "top/a", "top/b"]);
    }

    #[test]
    fn a_turret_chosen_brings_its_gimbal_mounts_and_the_gun_slot_in_each() {
        // The TMSB-5 of the game data: two S4 ports holding its own gimbal mount, which holds one S4 gun.
        let mut tmsb = part("tmsb", "Turret", 5, &[], &[]);
        tmsb.name = "TMSB-5 Ball Turret".into();
        tmsb.ports = ["hardpoint_weapon_left", "hardpoint_weapon_right"]
            .iter()
            .map(|name| PartPort { name: (*name).into(), fit: gimbal_port(4) })
            .collect();
        tmsb.factory = ["hardpoint_weapon_left", "hardpoint_weapon_right"]
            .iter()
            .map(|name| ((*name).to_string(), ("tmsb-gimbal".to_string(), "TMSB-5 Gimbal Mount".to_string())))
            .collect();
        let mut parts = ErkulParts::from_parts(vec![tmsb, gimbal("tmsb-gimbal", "TMSB-5 Gimbal Mount", 4)]);
        parts.add_kinds([("gun4".to_string(), "WeaponGun".to_string())]);
        let ball = vec![
            SlotInfo { size: Some(5), ..slot("hardpoint_weapon_center", "Turret", None, Some("ball")) },
            SlotInfo { fit: Some(gimbal_port(3)), ..slot("hardpoint_weapon_center/hardpoint_weapon_left", "WeaponGun", None, Some("varipuck3")) },
            slot("hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2", "WeaponGun", None, Some("cf337")),
        ];
        let chosen = plan(r#"{"hardpoint_weapon_center": {"ref": "tmsb", "name": "TMSB-5 Ball Turret"}}"#);
        let found: Vec<(String, String, Option<i64>, Option<String>)> = with_choices(ball.clone(), &chosen, &parts)
            .into_iter()
            .map(|s| (s.path, s.kind, s.size, s.factory.map(|(_, name)| name)))
            .collect();
        let row = |path: &str, size: i64, factory: Option<&str>| {
            (path.to_string(), "WeaponGun".to_string(), Some(size), factory.map(str::to_string))
        };
        assert_eq!(
            found[1..],
            [
                row("hardpoint_weapon_center/hardpoint_weapon_left", 4, Some("TMSB-5 Gimbal Mount")),
                row("hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2", 4, None),
                row("hardpoint_weapon_center/hardpoint_weapon_right", 4, Some("TMSB-5 Gimbal Mount")),
                row("hardpoint_weapon_center/hardpoint_weapon_right/hardpoint_class_2", 4, None),
            ]
        );
        // A gun right in a mount of the chosen turret: no gun slot below it, the other mount keeps its own.
        let deeper = plan(
            r#"{"hardpoint_weapon_center": {"ref": "tmsb", "name": "TMSB-5 Ball Turret"},
                "hardpoint_weapon_center/hardpoint_weapon_left": {"ref": "gun4", "name": "Gun S4"}}"#,
        );
        let paths: Vec<String> = with_choices(ball, &deeper, &parts).into_iter().map(|s| s.path).collect();
        assert!(!paths.contains(&"hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2".to_string()), "{paths:?}");
        assert!(paths.contains(&"hardpoint_weapon_center/hardpoint_weapon_right/hardpoint_class_2".to_string()), "{paths:?}");
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
