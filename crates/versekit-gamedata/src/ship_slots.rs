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

//! The slots of the ships from the game data: the loadouts of the Basetool catalog p4kx builds of the
//! `Data.p4k`, written in the form of the stored erkul files, so that every reader of those reads them too.
//!
//! - [`SHIPS_FILE`] has the form of `erkul-schiffe.json`: per player ship its slots with full path
//!   (`hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2`), kind, size, what fits and the factory
//!   part, all the way down: turret → guns, mining head → modules, rack → missiles, salvage head → modifiers.
//! - [`PARTS_FILE`] has the form of `erkul-teile.json`: the parts with their own slots and what sits in them at the
//!   factory, so that a turret or a rack of the player's choosing brings its slots; and the racks, missiles,
//!   bombs and paints the rule of a slot chooses from.
//!
//! ⚠ The ports right on a ship come without kind and size (they sit in the encrypted vehicle implementation, and
//! the archive is read without a key): kind and size are then those of the factory part, and a port empty at the factory gets
//! its kind from its name and no size. A port the game marks `uneditable` is no slot; the ports below it are.
//!
//! ⚠ The paints: p4kx fits a paint to a ship by the paint ports of the vehicle implementation, so without the key
//! it fits 12 of 1064. What the DataForge gives readably ([`PaintFacts`]): the tags a paint requires of its port
//! (`RequiredTags`, `Paint_Gladius`) and the paint ports of the ship Records (21 ships). Those go into the files;
//! every other paint port stays without a rule (see `versekit_core::erkul_slots::fits_like_stock`).

use std::collections::{BTreeMap, HashMap};

use p4kx_dataforge::{DataForge, TreeBuilder};
use serde_json::Value as Json;
use versekit_core::erkul_slots::SWAPPABLE;
use versekit_core::jsonio::{Map, Value};

use crate::fields::{component, structs, structure, text};
use crate::ship_values::ShipValues;

/// The slots of the ships, in the form of `erkul-schiffe.json`.
pub const SHIPS_FILE: &str = versekit_core::erkul_slots::GAME_SHIPS_FILE;
/// The parts with slots and the parts chosen by rule, in the form of `erkul-teile.json`.
pub const PARTS_FILE: &str = versekit_core::erkul_slots::GAME_PARTS_FILE;
/// The format of [`SHIPS_FILE`]: the one of `erkul-schiffe.json`.
pub const SHIPS_FORMAT: i64 = versekit_core::ships::ERKUL_FILE_FORMAT;
/// Which base values of the ships [`SHIPS_FILE`] holds (`werte_stand`, 1: size, SCM speed and cargo; 2: the cargo
/// counts the mining pods of the Mole, the Prospector and the Golem, and the 135c and the Nomad have none; 3: the
/// mining pods stand as ore (`erz`) beside the cargo, no longer in it; 4: ships whose cargo hangs on their modules
/// name what they hold with cargo modules, `fracht_module`). Its format is the one of erkul's file and stays; a file
/// with another mark is read again once (`values_store`).
pub const VALUES_MARK: i64 = 4;
/// The format of [`PARTS_FILE`]: the one of `erkul-teile.json` with the class name and the tags a paint
/// requires; a file of an older format is read again once (`values_store`).
pub const PARTS_FORMAT: i64 = versekit_core::erkul_slots::GAME_PARTS_FORMAT;

/// One paint port of a ship Record (`SItemPortDef` with the type `Paints`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordPort {
    pub name: String,
    /// What the port offers a paint: its `PortTags` and `RequiredPortTags` and the `AttachDef.Tags` of the ship, as
    /// p4kx counts them.
    pub offers: Vec<String>,
    /// What a paint has to carry: the `RequiredPortTags`.
    pub requires: Vec<String>,
}

/// What the DataForge says about paints beyond the catalog of p4kx, read without the key.
#[derive(Clone, Debug, Default)]
pub struct PaintFacts {
    /// Paint identifier → the tags it requires of its port (`AttachDef.RequiredTags`).
    pub required: HashMap<String, Vec<String>>,
    /// Ship identifier → the paint ports of its Record.
    pub ports: HashMap<String, Vec<RecordPort>>,
}

fn words(text: Option<&str>) -> Vec<String> {
    text.unwrap_or_default().split_whitespace().map(str::to_string).collect()
}

impl PaintFacts {
    /// Reads the paints (`…/paints/…`) and the vehicles (`entities/spaceships`, `entities/groundvehicles`) of the
    /// DataForge.
    pub fn read(forge: &DataForge<'_>) -> PaintFacts {
        let mut builder = TreeBuilder::new();
        let mut facts = PaintFacts::default();
        for record in forge.records() {
            let (Ok("EntityClassDefinition"), Ok(path)) = (record.type_name(), record.record_path()) else { continue };
            let path = path.to_ascii_lowercase();
            let vehicle = path.contains("entities/spaceships/") || path.contains("entities/groundvehicles/");
            let paint = path.contains("/paints/");
            if !vehicle && !paint {
                continue;
            }
            let Ok(tree) = builder.build(&record) else { continue };
            let definition = structure(component(&tree, "SAttachableComponentParams"), "AttachDef");
            let guid = tree.guid().to_string().to_ascii_lowercase();
            if paint {
                if text(definition, "Type").is_some_and(|kind| kind.eq_ignore_ascii_case("Paints")) {
                    let required = words(text(definition, "RequiredTags"));
                    if !required.is_empty() {
                        facts.required.insert(guid, required);
                    }
                }
                continue;
            }
            let own = words(text(definition, "Tags"));
            let ports: Vec<RecordPort> = structs(component(&tree, "SItemPortContainerComponentParams"), "Ports")
                .filter(|port| {
                    structs(Some(port), "Types")
                        .any(|kind| text(Some(kind), "Type").is_some_and(|k| k.eq_ignore_ascii_case("Paints")))
                })
                .map(|port| {
                    let requires = words(text(Some(port), "RequiredPortTags"));
                    let mut offers = words(text(Some(port), "PortTags"));
                    for tag in requires.iter().chain(&own) {
                        if !offers.contains(tag) {
                            offers.push(tag.clone());
                        }
                    }
                    RecordPort { name: text(Some(port), "Name").unwrap_or_default().to_string(), offers, requires }
                })
                .collect();
            if !ports.is_empty() {
                facts.ports.insert(guid, ports);
            }
        }
        facts
    }
}

/// The record paths p4kx builds the catalog of, one build each: the ships, the ground vehicles, the parts and the
/// makers.
pub const FILTERS: [&str; 4] = ["entities/spaceships", "entities/groundvehicles", "entities/scitem", "scitemmanufacturer"];

/// The kinds whose parts the rule of the slot chooses (`erkul.PART_KINDS`): those go into [`PARTS_FILE`] even
/// without slots of their own.
const RULE_KINDS: [&str; 6] = ["MissileLauncher", "BombLauncher", "Missile", "Bomb", "Paints", "FlightController"];

/// The kind of a port right on the ship that is empty at the factory, by a word of its name.
const KIND_BY_NAME: [(&str, &str); 18] = [
    ("paint", "Paints"),
    ("battery", "Battery"),
    ("avionics", "Avionics"),
    ("computer", "Avionics"),
    ("gravity", "GravityGenerator"),
    ("power_plant", "PowerPlant"),
    ("cooler", "Cooler"),
    ("shield", "Shield"),
    ("quantum_drive", "QuantumDrive"),
    ("jump_drive", "JumpDrive"),
    ("radar", "Radar"),
    ("missile", "MissileLauncher"),
    ("bomb", "BombLauncher"),
    ("tractor", "TractorBeam"),
    ("salvage", "SalvageHead"),
    ("mining_pod", "Container"),
    ("ore_pod", "Container"),
    ("turret", "Turret"),
];

/// What one catalog of p4kx holds that is needed here; several catalogs (one per filter) go into one.
#[derive(Default)]
pub struct Catalogs {
    ships: Vec<Json>,
    items: HashMap<String, Json>,
    loadouts: HashMap<String, Json>,
    makers: HashMap<String, Json>,
    build: String,
    /// What the DataForge says about paints ([`PaintFacts::read`]); empty where it was not read.
    pub paints: PaintFacts,
    /// Size and SCM speed of the ships ([`ShipValues::read`]); empty where it was not read.
    pub values: ShipValues,
}

impl Catalogs {
    /// Adds a catalog as p4kx writes it.
    pub fn add(&mut self, catalog: &Json) {
        let list = |key: &str| catalog.get(key).and_then(Json::as_array).cloned().unwrap_or_default();
        self.ships.extend(list("ships"));
        for item in list("items") {
            if let Some(guid) = item.get("guid").and_then(Json::as_str) {
                self.items.insert(guid.to_string(), item.clone());
            }
        }
        for loadout in list("loadouts") {
            if let Some(guid) = loadout.get("entityGuid").and_then(Json::as_str) {
                self.loadouts.insert(guid.to_string(), loadout.clone());
            }
        }
        for maker in list("manufacturers") {
            if let Some(guid) = maker.get("guid").and_then(Json::as_str) {
                self.makers.insert(guid.to_string(), maker.clone());
            }
        }
        if self.build.is_empty() {
            let build = &catalog["source"]["build"];
            let text = |key: &str| build.get(key).and_then(Json::as_str).unwrap_or_default().to_string();
            self.build = [text("branch"), text("changeNumber")].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(".");
        }
    }

    fn item(&self, guid: Option<&str>) -> Option<&Json> {
        self.items.get(guid?)
    }
}

/// A text of the catalog that may be a translated one (`{key, en, de}`): the English, else the plain text.
fn english(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Object(texts) => texts.get("en").and_then(Json::as_str).unwrap_or_default().to_string(),
        _ => String::new(),
    }
}

/// The name of a part: its English name, else its class name.
fn part_name(item: &Json) -> String {
    let name = english(&item["name"]);
    if name.is_empty() { item["className"].as_str().unwrap_or_default().to_string() } else { name }
}

/// The key of a ship as erkul writes it: lower case, letters and digits only (`anvlhornetf7cmmk2`).
fn slim(text: &str) -> String {
    text.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

fn strings(value: &Json) -> Vec<Value> {
    value.as_array().map(|items| items.iter().filter_map(Json::as_str).map(Value::str).collect()).unwrap_or_default()
}

fn int(value: &Json) -> Option<i64> {
    value.as_i64()
}

/// What a port takes, in the stored form (`passt`); `None` where the port names no kind.
fn fit_of(port: &Json) -> Option<Value> {
    let kinds: Vec<Value> = port["types"]
        .as_array()?
        .iter()
        .filter_map(|t| {
            let kind = t.get("type").and_then(Json::as_str)?;
            Some(Value::Array(vec![Value::str(kind), Value::Array(strings(&t["subTypes"]))]))
        })
        .collect();
    if kinds.is_empty() {
        return None;
    }
    let size = |key: &str| int(&port[key]).map_or(Value::Null, Value::int);
    Some(Value::Object(vec![
        ("typen".to_string(), Value::Array(kinds)),
        ("min".to_string(), size("minSize")),
        ("max".to_string(), size("maxSize")),
        ("porttags".to_string(), Value::Array(strings(&port["portTags"]))),
        ("verlangt".to_string(), Value::Array(strings(&port["requiredTags"]))),
    ]))
}

fn flagged(port: &Json, flag: &str) -> bool {
    port["flags"].as_array().is_some_and(|flags| flags.iter().any(|f| f.as_str().is_some_and(|f| f.eq_ignore_ascii_case(flag))))
}

/// The kind of a port: the first swappable kind it takes, else the kind of its factory part, else (right on the
/// ship, empty at the factory) the kind its name says.
fn kind_of(port: &Json, part: Option<&Json>, top: bool) -> Option<String> {
    let named = port["types"].as_array().into_iter().flatten().filter_map(|t| t.get("type").and_then(Json::as_str));
    if let Some(kind) = named.clone().find(|k| SWAPPABLE.contains(k)) {
        return Some(kind.to_string());
    }
    if named.count() > 0 {
        return None;
    }
    if let Some(kind) = part.and_then(|p| p["type"].as_str()) {
        return SWAPPABLE.contains(&kind).then(|| kind.to_string());
    }
    if !top {
        return None;
    }
    let name = port["name"].as_str().unwrap_or_default().to_lowercase();
    KIND_BY_NAME.iter().find(|(word, _)| name.contains(word)).map(|(_, kind)| (*kind).to_string())
}

/// The factory part of a port in the stored form (`werk`).
fn stock_of(port: &Json, part: Option<&Json>) -> Option<Value> {
    let guid = port["itemGuid"].as_str()?;
    let name = part.map(part_name).unwrap_or_else(|| port["itemClassName"].as_str().unwrap_or(guid).to_string());
    Some(Value::Object(vec![("ref".to_string(), Value::str(guid)), ("name".to_string(), Value::str(name))]))
}

/// Every slot of a port tree, all the way down, under its full path; `sizes` gets the kind and size of each.
fn collect(
    catalogs: &Catalogs,
    ports: &[Json],
    path: &str,
    top: bool,
    out: &mut Vec<Value>,
    sizes: &mut Vec<(String, Option<i64>)>,
) {
    for port in ports {
        let Some(name) = port["name"].as_str().filter(|n| !n.is_empty()) else { continue };
        let full = if path.is_empty() { name.to_string() } else { format!("{path}/{name}") };
        let part = catalogs.item(port["itemGuid"].as_str());
        // A port the game marks `uneditable` is a fixed slot: shown with its factory part, where it has one and
        // the player sees it, and never chosen anew.
        let fixed = flagged(port, "uneditable");
        let shown = !fixed || (port["itemGuid"].as_str().is_some() && !flagged(port, "invisible"));
        if shown
            && !port["dockedVehicle"].as_bool().unwrap_or(false)
            && let Some(kind) = kind_of(port, part, top)
        {
            let size = int(&port["maxSize"]).or_else(|| int(&port["minSize"])).or_else(|| part.and_then(|p| int(&p["size"])));
            let mut entry: Map = vec![
                ("pfad".to_string(), Value::str(&full)),
                ("art".to_string(), Value::str(&kind)),
                ("groesse".to_string(), size.map_or(Value::Null, Value::int)),
            ];
            if let Some(fit) = fit_of(port) {
                entry.push(("passt".to_string(), fit));
            }
            if let Some(stock) = stock_of(port, part) {
                entry.push(("werk".to_string(), stock));
            }
            if fixed {
                entry.push(("fest".to_string(), Value::Bool(true)));
            } else {
                // A fixed slot takes no blueprint part, so it is not counted.
                sizes.push((kind, size));
            }
            out.push(Value::Object(entry));
        }
        if let Some(below) = port["ports"].as_array() {
            collect(catalogs, below, &full, false, out, sizes);
        }
    }
}

/// How many slots of each kind and size (`plaetze`), in the order of kind and size; a slot without a size is
/// not counted.
fn counts(sizes: &[(String, Option<i64>)]) -> Vec<Value> {
    let mut counted: BTreeMap<(String, i64), i64> = BTreeMap::new();
    for (kind, size) in sizes {
        if let Some(size) = size {
            *counted.entry((kind.clone(), *size)).or_insert(0) += 1;
        }
    }
    counted
        .into_iter()
        .map(|((kind, size), n)| {
            Value::Object(vec![
                ("art".to_string(), Value::str(kind)),
                ("groesse".to_string(), Value::int(size)),
                ("anzahl".to_string(), Value::int(n)),
            ])
        })
        .collect()
}

/// Gives the paint slots right on a ship without a rule of their own the rule of the paint port of the ship
/// Record: the port of the same name, else the only one where the ship has one paint slot and one such port.
fn paint_rules(slots: &mut [Value], record_ports: &[RecordPort]) {
    let open = |slot: &Value| {
        slot.as_object().is_some_and(|fields| {
            let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            get("art").and_then(Value::as_str) == Some("Paints")
                && get("passt").is_none()
                && !get("pfad").and_then(Value::as_str).unwrap_or("/").contains('/')
        })
    };
    let paint_slots = slots.iter().filter(|slot| open(slot)).count();
    for slot in slots.iter_mut().filter(|slot| open(slot)) {
        let Value::Object(fields) = slot else { continue };
        let path = fields.iter().find(|(k, _)| k == "pfad").and_then(|(_, v)| v.as_str()).unwrap_or_default().to_string();
        let port = record_ports
            .iter()
            .find(|port| port.name.eq_ignore_ascii_case(&path))
            .or_else(|| (paint_slots == 1 && record_ports.len() == 1).then(|| &record_ports[0]));
        // A port that names no paint family (no `RequiredPortTags`, nothing of a paint offered) says nothing: it would
        // take the factory paint's family away from the slot.
        let Some(port) = port.filter(|port| {
            !port.requires.is_empty() || port.offers.iter().any(|tag| tag.to_ascii_lowercase().contains("paint"))
        }) else {
            continue;
        };
        let tags = |list: &[String]| Value::Array(list.iter().map(Value::str).collect());
        fields.push((
            "passt".to_string(),
            Value::Object(vec![
                ("typen".to_string(), Value::Array(vec![Value::Array(vec![Value::str("Paints"), Value::Array(Vec::new())])])),
                ("min".to_string(), Value::Null),
                ("max".to_string(), Value::Null),
                ("porttags".to_string(), tags(&port.offers)),
                ("verlangt".to_string(), tags(&port.requires)),
            ]),
        ));
    }
}

/// The maker of a ship: its name and the short code of its class name (`ANVL` of `ANVL_Hornet_F7CM_Mk2`).
fn maker_of(catalogs: &Catalogs, ship: &Json) -> (String, String) {
    let class = ship["className"].as_str().unwrap_or_default();
    let code = class.split('_').next().unwrap_or_default().to_lowercase();
    let name = ship["manufacturerGuid"].as_str().and_then(|g| catalogs.makers.get(g)).map(|m| english(&m["name"])).unwrap_or_default();
    (name, code)
}

/// The flight controller a port tree has at the factory: the identifier of the first part of the kind
/// `FlightController`, all the way down.
fn flight_controller<'c>(catalogs: &'c Catalogs, ports: &'c [Json]) -> Option<&'c str> {
    ports.iter().find_map(|port| {
        let guid = port["itemGuid"].as_str();
        let kind = catalogs.item(guid).and_then(|part| part["type"].as_str());
        if kind == Some("FlightController") {
            return guid;
        }
        flight_controller(catalogs, port["ports"].as_array().map(Vec::as_slice).unwrap_or_default())
    })
}

/// The cargo space of a port tree that p4kx does not count as a cargo grid, in SCU: the mining pods of the Mole, the
/// Prospector and the Golem (`Cargo_ShipMining_Pod_*`, containers that hold a capacity of their own, read by
/// [`ShipValues::read`]; the collapsed spare pods in the hull hold nothing). `Some(0.0)` for a tree without any. `None`
/// where there is space that cannot be counted: the cargo area of the 135c (`ORIG_135c_Cargo_Area`) and the cargo bed
/// of the Nomad (`Door_Ship_Exterior_CNOU_Nomad_Cargo_Bed`) hold no capacity in the data (LIVE 4.10.3: 0 SCU of
/// grids, while the ships carry 6 and 24 SCU), nor does a pod without one; the sum of the grids of such a ship is
/// too low, so it is no cargo value.
fn cargo_outside_grids(catalogs: &Catalogs, ports: &[Json]) -> Option<f64> {
    let mut scu = 0.0;
    for port in ports {
        let guid = port["itemGuid"].as_str();
        let class = catalogs.item(guid).and_then(|part| part["className"].as_str()).unwrap_or_default();
        let class = class.to_ascii_lowercase();
        if class.contains("_shipmining_pod_") {
            // A collapsed pod is a spare that is stored, not a hold.
            if !class.ends_with("_collapsed") {
                scu += guid.and_then(|guid| catalogs.values.pods.get(&guid.to_ascii_lowercase()))?;
            }
        } else if class.ends_with("_cargo_area") || class.ends_with("_cargo_bed") {
            return None;
        }
        scu += cargo_outside_grids(catalogs, port["ports"].as_array().map(Vec::as_slice).unwrap_or_default())?;
    }
    Some(scu)
}

/// How many cargo grids of a port tree belong to a fabricator: a part that is a cargo grid entity at a port whose
/// name says fabricator, or below one.
fn fabricator_grids(catalogs: &Catalogs, ports: &[Json]) -> u64 {
    fabricator_grids_below(catalogs, ports, false)
}

fn fabricator_grids_below(catalogs: &Catalogs, ports: &[Json], in_fabricator: bool) -> u64 {
    let mut count = 0;
    for port in ports {
        let class = catalogs
            .item(port["itemGuid"].as_str())
            .and_then(|part| part["className"].as_str())
            .or_else(|| port["itemClassName"].as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let name = port["name"].as_str().unwrap_or_default().to_ascii_lowercase();
        let in_fabricator = in_fabricator || name.contains("fabricator");
        if in_fabricator && class.contains("cargogrid") {
            count += 1;
        }
        count += fabricator_grids_below(
            catalogs,
            port["ports"].as_array().map(Vec::as_slice).unwrap_or_default(),
            in_fabricator,
        );
    }
    count
}

/// The slots of the player ships in the form of `erkul-schiffe.json`, each with its base values (`werte`, see
/// [`ShipValues::of`]) where the data have them.
pub fn ships_file(catalogs: &Catalogs) -> Value {
    let mut ships: Map = Vec::new();
    let mut makers: Map = Vec::new();
    for ship in &catalogs.ships {
        if ship["playability"].as_str() != Some("player") {
            continue;
        }
        let Some(class) = ship["className"].as_str() else { continue };
        let Some(guid) = ship["guid"].as_str() else { continue };
        let key = slim(class);
        if key.is_empty() || ships.iter().any(|(k, _)| *k == key) {
            continue;
        }
        let ports = catalogs.loadouts.get(guid).and_then(|l| l["ports"].as_array()).cloned().unwrap_or_default();
        let (mut slots, mut sizes) = (Vec::new(), Vec::new());
        collect(catalogs, &ports, "", true, &mut slots, &mut sizes);
        if let Some(record_ports) = catalogs.paints.ports.get(&guid.to_ascii_lowercase()) {
            paint_rules(&mut slots, record_ports);
        }
        let (maker, code) = maker_of(catalogs, ship);
        if !maker.is_empty() && !code.is_empty() && !makers.iter().any(|(k, _)| *k == slim(&maker)) {
            makers.push((slim(&maker), Value::str(&code)));
        }
        let own = english(&ship["name"]);
        let name = [maker.as_str(), own.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" ");
        let mut entry: Map = vec![
            ("name".to_string(), Value::str(if name.is_empty() { class } else { &name })),
            ("id".to_string(), Value::str(class)),
            ("plaetze".to_string(), Value::Array(counts(&sizes))),
            ("slots".to_string(), Value::Array(slots)),
        ];
        let outside = cargo_outside_grids(catalogs, &ports);
        let fabricators = fabricator_grids(catalogs, &ports);
        if let Some(values) = catalogs.values.of(ship, flight_controller(catalogs, &ports), outside, fabricators) {
            entry.push(("werte".to_string(), values));
        }
        ships.push((key, Value::Object(entry)));
    }
    Value::Object(vec![
        ("format".to_string(), Value::int(SHIPS_FORMAT)),
        // Right after the format, where `values_store` reads it from the first bytes.
        ("werte_stand".to_string(), Value::int(VALUES_MARK)),
        ("spielversion".to_string(), Value::str(&catalogs.build)),
        ("quelle".to_string(), Value::str("spieldaten")),
        ("schiffe".to_string(), Value::Object(ships)),
        ("hersteller".to_string(), Value::Object(makers)),
    ])
}

/// The parts in the form of `erkul-teile.json`: every part with slots of its own (with what sits in them at the
/// factory) and every part of a kind the rule of a slot chooses from.
pub fn parts_file(catalogs: &Catalogs) -> Value {
    let mut parts: Vec<(String, Value)> = Vec::new();
    let mut guids: Vec<&String> = catalogs.items.keys().collect();
    guids.sort();
    for guid in guids {
        let item = &catalogs.items[guid];
        let kind = item["type"].as_str().unwrap_or_default();
        let ports = catalogs.loadouts.get(guid).and_then(|l| l["ports"].as_array()).cloned().unwrap_or_default();
        let mut own: Vec<Value> = Vec::new();
        let mut stock: Map = Vec::new();
        for port in &ports {
            let Some(name) = port["name"].as_str() else { continue };
            if flagged(port, "uneditable") {
                continue;
            }
            let takes = port["types"].as_array().into_iter().flatten().filter_map(|t| t["type"].as_str()).any(|k| SWAPPABLE.contains(&k));
            let Some(fit) = fit_of(port).filter(|_| takes) else { continue };
            own.push(Value::Object(vec![("name".to_string(), Value::str(name)), ("passt".to_string(), fit)]));
            if let Some(werk) = stock_of(port, catalogs.item(port["itemGuid"].as_str())) {
                stock.push((name.to_string(), werk));
            }
        }
        if own.is_empty() && !RULE_KINDS.contains(&kind) {
            continue;
        }
        let mut part: Map = vec![
            ("name".to_string(), Value::str(part_name(item))),
            ("typ".to_string(), Value::str(kind)),
            ("untertyp".to_string(), Value::str(item["subType"].as_str().unwrap_or_default())),
            ("groesse".to_string(), int(&item["size"]).map_or(Value::Null, Value::int)),
            ("tags".to_string(), Value::Array(strings(&item["tags"]))),
            // What a paint requires of its port (`Paint_Gladius`): it decides where the paint goes.
            (
                "verlangt".to_string(),
                Value::Array(
                    catalogs.paints.required.get(&guid.to_ascii_lowercase()).into_iter().flatten().map(Value::str).collect(),
                ),
            ),
            ("guete".to_string(), int(&item["grade"]).map_or(Value::str(""), |g| Value::str(g.to_string()))),
            // The class name ties a flight blade without tags to the blade of its ship (`erkul_slots::fits_like_stock`).
            ("klasse".to_string(), Value::str(item["className"].as_str().unwrap_or_default())),
        ];
        if !own.is_empty() {
            part.push(("plaetze".to_string(), Value::Array(own)));
            part.push(("werk".to_string(), Value::Object(stock)));
        }
        parts.push((guid.clone(), Value::Object(part)));
    }
    Value::Object(vec![
        ("format".to_string(), Value::int(PARTS_FORMAT)),
        ("spielversion".to_string(), Value::str(&catalogs.build)),
        ("quelle".to_string(), Value::str("spieldaten")),
        ("teile".to_string(), Value::Object(parts)),
    ])
}

/// A number of the catalog as a value of the file; `null` stays `null`.
fn float(value: &Json) -> Value {
    value.as_f64().map_or(Value::Null, |x| Value::Float((x * 10_000.0).round() / 10_000.0))
}

/// A number of the catalog, or `fallback` where it is not set.
fn float_or(value: &Json, fallback: f64) -> Value {
    match float(value) {
        Value::Null => Value::Float(fallback),
        set => set,
    }
}

/// A change in percent of a block of mining modifiers (`resistance`, `instability`); not set counts as none.
fn percent(modifiers: &Json, key: &str) -> Value {
    float_or(&modifiers[key], 0.0)
}

/// The further changes of a block of mining modifiers and its filter, as the file keeps them; not set stays `null`.
fn more(modifiers: &Json, filter: &Json) -> Vec<(&'static str, Value)> {
    vec![
        ("fenster", float(&modifiers["optimalChargeWindowSize"])),
        ("laderate", float(&modifiers["optimalChargeWindowRate"])),
        ("ueberladung", float(&modifiers["catastrophicChargeWindowRate"])),
        ("splitter", float(&modifiers["shatterDamage"])),
        ("clustering", float(&modifiers["clusterFactor"])),
        ("filter", float(filter)),
    ]
}

/// The power factor a module gives the cracking beam: the multiplier of its first fire action, where it has one.
fn power_factor(block: &Json) -> Value {
    block["fireActionModifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["fireActionIndex"].as_i64().unwrap_or(0) == 0)
        .map_or(Value::Float(1.0), |m| float_or(&m["damageMultiplier"], 1.0))
}

/// The mining equipment of the parts ([`versekit_core::mining_gear::FILE`]): every laser head with the power of
/// its cracking beam (`ElectricArc`), its changes and its module slots, every module and every gadget with theirs.
pub fn mining_file(catalogs: &Catalogs) -> Value {
    let (mut heads, mut modules, mut gadgets) = (Vec::new(), Vec::new(), Vec::new());
    let mut guids: Vec<&String> = catalogs.items.keys().collect();
    guids.sort();
    for guid in guids {
        let item = &catalogs.items[guid];
        let stats = &item["stats"];
        let named = |extra: Vec<(&str, Value)>| {
            let mut entry: Map = vec![
                ("ref".to_string(), Value::str(guid)),
                ("name".to_string(), Value::str(part_name(item))),
                ("klasse".to_string(), Value::str(item["className"].as_str().unwrap_or_default())),
            ];
            entry.extend(extra.into_iter().map(|(k, v)| (k.to_string(), v)));
            Value::Object(entry)
        };
        let laser = &stats["miningLaser"];
        if laser.is_object() {
            let beam_of = |hit: &str| {
                stats["weapon"]["fireActions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|action| action["hitType"].as_str() == Some(hit))
            };
            let beam = beam_of("ElectricArc");
            // A tool without power (the multi-tool of the AI in 4.10) cracks nothing and is no head to choose.
            let Some(power) =
                beam.map(|b| float(&b["damagePerSecond"]["energy"])).filter(|p| matches!(p, Value::Float(x) if *x > 0.0))
            else {
                continue;
            };
            let beam = beam.unwrap_or(&Json::Null);
            let ports = catalogs.loadouts.get(guid).and_then(|l| l["ports"].as_array()).cloned().unwrap_or_default();
            let slots = ports
                .iter()
                .filter(|port| {
                    port["types"].as_array().into_iter().flatten().any(|t| t["type"].as_str() == Some("MiningModifier"))
                })
                .count();
            let modifiers = &laser["modifiers"];
            let mut values = vec![
                ("groesse", int(&item["size"]).map_or(Value::Null, Value::int)),
                ("leistung", power),
                ("drossel_min", float(&laser["throttleMinimum"])),
                ("extraktion", beam_of("Extraction").map_or(Value::Null, |b| float(&b["damagePerSecond"]["energy"]))),
                ("reichweite_voll", float(&beam["fullDamageRange"])),
                ("reichweite_null", float(&beam["zeroDamageRange"])),
                ("widerstand", percent(modifiers, "resistance")),
                ("instabilitaet", percent(modifiers, "instability")),
                ("plaetze", Value::int(slots as i64)),
            ];
            values.extend(more(modifiers, &laser["filterModifier"]));
            heads.push(named(values));
            continue;
        }
        let block = &stats["miningModifier"];
        if !block.is_object() {
            continue;
        }
        if block["rock"].is_object() {
            // A base entity without a name of its own (`..._MiningBase`) is no gadget a player holds.
            if english(&item["name"]).trim().is_empty() {
                continue;
            }
            let modifiers = &block["rock"]["modifiers"];
            let mut values =
                vec![("widerstand", percent(modifiers, "resistance")), ("instabilitaet", percent(modifiers, "instability"))];
            values.extend(more(modifiers, &block["filter"]["filterModifier"]));
            gadgets.push(named(values));
        } else if block["laser"].is_object() || block["fireActionModifiers"].as_array().is_some_and(|a| !a.is_empty()) {
            let modifiers = &block["laser"]["modifiers"];
            let active = block["activationMethod"].as_str() == Some("ActivateOnDemand");
            let mut values = vec![
                ("aktiv", Value::Bool(active)),
                ("ladungen", int(&block["charges"]).filter(|c| *c >= 0).map_or(Value::Null, Value::int)),
                ("dauer", float(&block["laser"]["lifetime"])),
                ("leistung", power_factor(block)),
                ("widerstand", percent(modifiers, "resistance")),
                ("instabilitaet", percent(modifiers, "instability")),
            ];
            values.extend(more(modifiers, &block["filter"]["filterModifier"]));
            modules.push(named(values));
        }
    }
    Value::Object(vec![
        ("format".to_string(), Value::int(versekit_core::mining_gear::FORMAT)),
        ("spielversion".to_string(), Value::str(&catalogs.build)),
        ("quelle".to_string(), Value::str("spieldaten")),
        ("koepfe".to_string(), Value::Array(heads)),
        ("module".to_string(), Value::Array(modules)),
        ("gadgets".to_string(), Value::Array(gadgets)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::erkul_slots::{ErkulParts, hardpoints};
    use versekit_core::ships::{ErkulShips, ShipId};

    /// A made-up catalog in the shape p4kx writes: the F7C-M Mk II with its ball turret (two gimbals with a gun
    /// each, a rack with two missiles), a power plant whose port names no kind, a paint port empty at the factory,
    /// an uneditable seat and a weapon attachment; the TMSB-5 turret as a part with two S4 gun ports.
    fn catalog() -> Json {
        serde_json::from_str(r#"{
            "source": {"build": {"branch": "sc-alpha-4.10.2", "changeNumber": "12913470"}},
            "manufacturers": [{"guid": "m1", "name": {"en": "Anvil Aerospace"}}],
            "ships": [
                {"guid": "s1", "className": "ANVL_Hornet_F7CM_Mk2", "name": {"en": "F7C-M Super Hornet Mk II"}, "manufacturerGuid": "m1", "playability": "player"},
                {"guid": "s2", "className": "ANVL_Hornet_F7CM_Mk2_PU_AI", "name": {"en": "F7C-M AI"}, "manufacturerGuid": "m1", "playability": "ai"}
            ],
            "items": [
                {"guid": "pp", "className": "POWR_S02", "name": {"en": "Maelstrom"}, "type": "PowerPlant", "size": 2},
                {"guid": "ball", "className": "Ball_Turret", "name": {"en": "Anvil F7C-M Mk II Ball Turret"}, "type": "Turret", "size": 5},
                {"guid": "puck", "className": "VariPuck_S3", "name": {"en": "VariPuck S3 Gimbal Mount"}, "type": "Turret", "size": 3},
                {"guid": "gun3", "className": "CF337", "name": {"en": "CF-337 Panther Repeater"}, "type": "WeaponGun", "size": 3},
                {"guid": "rack", "className": "MRCK_S04", "name": {"en": "Rack"}, "type": "MissileLauncher", "size": 4},
                {"guid": "msl", "className": "MSL_S01", "name": {"en": "Spark I Missile"}, "type": "Missile", "size": 1, "tags": ["Missile"]},
                {"guid": "tmsb", "className": "TMSB5", "name": {"en": "TMSB-5 Ball Turret"}, "type": "Turret", "size": 5},
                {"guid": "seat", "className": "Seat", "name": {"en": "Seat"}, "type": "Seat", "size": 1}
            ],
            "loadouts": [
                {"entityGuid": "s1", "entityKind": "ship", "issues": ["vehicle_implementation"], "ports": [
                    {"name": "hardpoint_paint", "types": [], "flags": [], "ports": []},
                    {"name": "hardpoint_power_plant", "types": [], "flags": [], "itemGuid": "pp", "ports": []},
                    {"name": "hardpoint_seat_pilot", "types": [], "flags": [], "itemGuid": "seat", "ports": []},
                    {"name": "hardpoint_weapon_center", "types": [], "flags": [], "itemGuid": "ball", "ports": [
                        {"name": "hardpoint_weapon_left", "types": [{"type": "Turret", "subTypes": []}], "minSize": 3, "maxSize": 3, "flags": ["editable"], "itemGuid": "puck", "ports": [
                            {"name": "hardpoint_class_2", "types": [{"type": "WeaponGun", "subTypes": ["Gun"]}], "minSize": 3, "maxSize": 3, "flags": ["editable"], "itemGuid": "gun3", "ports": [
                                {"name": "BAR1", "types": [{"type": "WeaponAttachment", "subTypes": []}], "minSize": 1, "maxSize": 1, "flags": ["uneditable"], "ports": []}
                            ]}
                        ]},
                        {"name": "hardpoint_missile_rack", "types": [{"type": "MissileLauncher", "subTypes": []}], "minSize": 4, "maxSize": 4, "flags": [], "itemGuid": "rack", "ports": [
                            {"name": "missile_01_attach", "types": [{"type": "Missile", "subTypes": []}], "minSize": 1, "maxSize": 1, "flags": [], "portTags": ["Missile"], "itemGuid": "msl", "ports": []},
                            {"name": "missile_02_attach", "types": [{"type": "Missile", "subTypes": []}], "minSize": 1, "maxSize": 1, "flags": ["uneditable"], "itemGuid": "msl", "ports": []}
                        ]}
                    ]}
                ]},
                {"entityGuid": "tmsb", "entityKind": "item", "issues": [], "ports": [
                    {"name": "hardpoint_weapon_left", "types": [{"type": "WeaponGun", "subTypes": []}, {"type": "Turret", "subTypes": []}], "minSize": 4, "maxSize": 4, "flags": ["editable"], "itemGuid": null, "ports": []},
                    {"name": "hardpoint_weapon_right", "types": [{"type": "WeaponGun", "subTypes": []}], "minSize": 4, "maxSize": 4, "flags": ["editable"], "itemGuid": null, "ports": []}
                ]}
            ]
        }"#)
        .expect("the made-up catalog")
    }

    fn written(value: &Value, name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-spielslots-{}", versekit_core::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join(name).to_string_lossy().into_owned();
        std::fs::write(&file, versekit_core::jsonio::dumps(value, None, false)).expect("written");
        file
    }

    #[test]
    fn the_slots_of_a_ship_come_from_its_loadout_all_the_way_down_in_the_form_of_the_erkul_file() {
        let mut catalogs = Catalogs::default();
        catalogs.add(&catalog());
        let ships = ErkulShips::load(&written(&ships_file(&catalogs), SHIPS_FILE));
        let parts = ErkulParts::load(&written(&parts_file(&catalogs), PARTS_FILE));
        let id = ShipId { name: "F7C-M Super Hornet Mk II", maker: "Anvil Aerospace", short: "ANVL_Hornet_F7CM_Mk2", maker_short: "" };
        assert!(ships.knows(id), "found by the class name of the hangar entry");
        let slots = hardpoints(&ships, &parts, id, &Map::new());
        let found: Vec<(String, String, Option<i64>)> = slots.iter().map(|s| (s.path.clone(), s.kind.clone(), s.size)).collect();
        assert_eq!(
            found,
            [
                ("hardpoint_paint".to_string(), "Paints".to_string(), None),
                ("hardpoint_power_plant".into(), "PowerPlant".into(), Some(2)),
                ("hardpoint_weapon_center".into(), "Turret".into(), Some(5)),
                ("hardpoint_weapon_center/hardpoint_weapon_left".into(), "Turret".into(), Some(3)),
                ("hardpoint_weapon_center/hardpoint_weapon_left/hardpoint_class_2".into(), "WeaponGun".into(), Some(3)),
                ("hardpoint_weapon_center/hardpoint_missile_rack".into(), "MissileLauncher".into(), Some(4)),
                ("hardpoint_weapon_center/hardpoint_missile_rack/missile_01_attach".into(), "Missile".into(), Some(1)),
                ("hardpoint_weapon_center/hardpoint_missile_rack/missile_02_attach".into(), "Missile".into(), Some(1)),
            ],
            "the seat and the empty attachment are no slots"
        );
        // The uneditable missile is a fixed slot with its factory part; the others are not.
        let fixed: Vec<&str> = slots.iter().filter(|s| s.fixed).map(|s| s.path.as_str()).collect();
        assert_eq!(fixed, ["hardpoint_weapon_center/hardpoint_missile_rack/missile_02_attach"]);
        assert_eq!(slots[7].factory, Some(("msl".to_string(), "Spark I Missile".to_string())));
        // A plan for the fixed slot changes nothing there.
        let planned = versekit_core::jsonio::loads(
            r#"{"hardpoint_weapon_center/hardpoint_missile_rack/missile_02_attach": {"ref": "other", "name": "Other"}}"#,
        )
        .ok()
        .and_then(|v| v.as_object().cloned())
        .expect("plan");
        assert_eq!(hardpoints(&ships, &parts, id, &planned), slots, "the fixed slot keeps its factory part");
        // Neither the cart nor the paths of the plan nor the count for blueprints know the fixed slot.
        let data = versekit_core::erkul_slots::ErkulData { ships: ships.clone(), parts: parts.clone() };
        let entry = versekit_core::jsonio::loads(r#"{"name": "F7C-M Super Hornet Mk II", "kurz": "ANVL_Hornet_F7CM_Mk2"}"#)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .expect("entry");
        let fixed_path = "hardpoint_weapon_center/hardpoint_missile_rack/missile_02_attach";
        assert!(!data.cart_slots(&entry, &planned).iter().any(|s| s.path == fixed_path));
        assert!(!data.slot_paths(&entry).iter().any(|p| p == fixed_path));
        assert_eq!(versekit_core::erkul_slots::fits(&ships, "Missile", Some(1), id), 1, "one missile slot takes a part");
        // The factory part with its identifier, and what fits where the port says so.
        assert_eq!(slots[1].factory, Some(("pp".to_string(), "Maelstrom".to_string())));
        assert!(slots[1].fit.is_none(), "the top port names no kind: the factory part gave it");
        assert_eq!(slots[4].fit.as_ref().map(|f| (f.min, f.max)), Some((Some(3), Some(3))));
        // Each part keeps its class name: it ties a flight blade without tags to its ship.
        assert_eq!(parts.get("tmsb").map(|p| p.class.as_str()), Some("TMSB5"));
        // The AI ship is left out.
        assert!(!ships.knows(ShipId { name: "F7C-M AI", maker: "", short: "ANVL_Hornet_F7CM_Mk2_PU_AI", maker_short: "" }));
        // Another turret brings its own slots: the TMSB-5 two S4 guns in place of the stock ones.
        let swapped = versekit_core::jsonio::loads(r#"{"hardpoint_weapon_center": {"ref": "tmsb", "name": "TMSB-5 Ball Turret"}}"#)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .expect("plan");
        let after: Vec<(String, Option<i64>)> =
            hardpoints(&ships, &parts, id, &swapped).iter().map(|s| (s.path.clone(), s.size)).collect();
        assert!(after.contains(&("hardpoint_weapon_center/hardpoint_weapon_left".to_string(), Some(4))), "{after:?}");
        assert!(after.contains(&("hardpoint_weapon_center/hardpoint_weapon_right".to_string(), Some(4))), "{after:?}");
        assert!(!after.iter().any(|(p, _)| p.contains("hardpoint_missile_rack")), "the stock rack went with the stock turret");
    }

    /// The paints: what a paint requires and the paint port of the ship Record go into the files; the paint slot
    /// then takes the paints of its family only, and the parts file has the format of 5.2.1.
    #[test]
    fn the_paint_slot_takes_the_rule_of_the_paint_port_of_the_ship_record() {
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let mut raw = catalog();
        let items = raw["items"].as_array_mut().expect("items");
        for (guid, class, tags) in [
            ("P1", "Paint_Hornet_Mk2_Black", r#"["Paint_Hornet_F7_Mk2", "@Black"]"#),
            ("p2", "Paint_Gladius_Pirate", r#"["Paint_Gladius", "@GladiusPirate"]"#),
        ] {
            let tags: Json = serde_json::from_str(tags).expect("tags");
            items.push(serde_json::json!({"guid": guid, "className": class, "name": {"en": class}, "type": "Paints", "size": 1, "tags": tags}));
        }
        let paints = PaintFacts {
            // The DataForge writes identifiers in lower case; the catalog may not.
            required: [("p1", "Paint_Hornet_F7_Mk2"), ("p2", "Paint_Gladius")]
                .into_iter()
                .map(|(guid, tag)| (guid.to_string(), vec![tag.to_string()]))
                .collect(),
            ports: [(
                "s1".to_string(),
                vec![RecordPort {
                    name: "Hardpoint_Paint".into(),
                    offers: vec!["Paint_Hornet_F7_Mk2".into(), "anvl_hornet_f7cm_mk2".into()],
                    requires: vec!["Paint_Hornet_F7_Mk2".into()],
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut catalogs = Catalogs { paints, ..Catalogs::default() };
        catalogs.add(&raw);
        let parts_value = parts_file(&catalogs);
        let Value::Object(root) = &parts_value else { panic!("an object") };
        assert_eq!(root.first().map(|(k, v)| (k.as_str(), v.clone())), Some(("format", Value::int(3))));
        // Files of their own: the other test of this module writes the same names into the same folder.
        let ships = ErkulShips::load(&written(&ships_file(&catalogs), "lack-schiffe.json"));
        let parts = ErkulParts::load(&written(&parts_value, "lack-teile.json"));
        assert_eq!(parts.get("P1").map(|p| p.required.clone()), Some(vec!["Paint_Hornet_F7_Mk2".to_string()]));
        let id = ShipId { name: "F7C-M Super Hornet Mk II", maker: "Anvil Aerospace", short: "ANVL_Hornet_F7CM_Mk2", maker_short: "" };
        let slots = hardpoints(&ships, &parts, id, &Map::new());
        let paint = slots.iter().find(|s| s.kind == "Paints").expect("the paint slot");
        let offered: Vec<&str> = parts.compatible(paint).iter().map(|p| p.reference.as_str()).collect();
        assert_eq!(offered, ["P1"], "the Hornet paint, not the Gladius paint");
        // Counter-check: without the Record port the slot has no rule, and no list.
        let required = PaintFacts { required: catalogs.paints.required.clone(), ..PaintFacts::default() };
        let mut bare = Catalogs { paints: required, ..Catalogs::default() };
        bare.add(&raw);
        let ships = ErkulShips::load(&written(&ships_file(&bare), "lack-schiffe-ohne-port.json"));
        let paint = hardpoints(&ships, &parts, id, &Map::new()).into_iter().find(|s| s.kind == "Paints").expect("slot");
        assert!(paint.fit.is_none());
        assert!(parts.compatible(&paint).is_empty());
        // A Record port that names no paint family gives no rule either.
        let silent = RecordPort { name: "hardpoint_paint".into(), offers: vec!["anvl_hornet_f7cm_mk2".into()], requires: Vec::new() };
        let paints = PaintFacts {
            required: catalogs.paints.required.clone(),
            ports: [("s1".to_string(), vec![silent])].into_iter().collect(),
        };
        let mut quiet = Catalogs { paints, ..Catalogs::default() };
        quiet.add(&raw);
        let ships = ErkulShips::load(&written(&ships_file(&quiet), "lack-schiffe-stummer-port.json"));
        let paint = hardpoints(&ships, &parts, id, &Map::new()).into_iter().find(|s| s.kind == "Paints").expect("slot");
        assert!(paint.fit.is_none(), "the silent port is no rule");
    }

    #[test]
    fn the_cargo_grid_of_a_fabricator_port_does_not_take_the_cargo_of_a_ship() {
        let mut catalogs = Catalogs::default();
        catalogs.add(&serde_json::json!({
            "source": {"build": {"branch": "b", "changeNumber": "1"}},
            "manufacturers": [],
            "ships": [
                {"guid": "s1", "className": "TEST_Freighter", "name": {"en": "Freighter"}, "playability": "player",
                 "cargoScu": 456.0, "cargoGridsWithIssue": 1},
                {"guid": "s2", "className": "TEST_Barge", "name": {"en": "Barge"}, "playability": "player",
                 "cargoScu": 20.0, "cargoGridsWithIssue": 1}
            ],
            "items": [
                {"guid": "bench", "className": "Bench_Fabricator", "type": "Container"},
                {"guid": "grid", "className": "Bench_CargoGridEntity_1SCU", "type": "Container"},
                {"guid": "other", "className": "Some_CargoGridEntity_1SCU", "type": "Container"}
            ],
            "loadouts": [
                {"entityGuid": "s1", "entityKind": "ship", "issues": [], "ports": [
                    {"name": "hardpoint_fabricator", "types": [], "flags": [], "itemGuid": "bench", "ports": [
                        {"name": "$slot_subitem_01", "types": [], "flags": [], "itemGuid": "grid", "ports": []}
                    ]}
                ]},
                {"entityGuid": "s2", "entityKind": "ship", "issues": [], "ports": [
                    {"name": "hardpoint_storage", "types": [], "flags": [], "itemGuid": "other", "ports": []}
                ]}
            ]
        }));
        let file = ships_file(&catalogs);
        let cargo = |key: &str| {
            file.get("schiffe")
                .and_then(|ships| ships.get(key))
                .and_then(|ship| ship.get("werte"))
                .and_then(|values| values.get("fracht"))
                .cloned()
        };
        assert_eq!(cargo("testfreighter"), Some(Value::Float(456.0)));
        assert_eq!(cargo("testbarge"), None, "a grid of another port stays unreadable");
    }

    #[test]
    fn the_mining_pods_of_a_ship_are_counted_and_a_cargo_area_without_capacity_cannot_be() {
        let mut catalogs = Catalogs::default();
        catalogs.add(&serde_json::json!({"items": [
            {"guid": "pod", "className": "Cargo_ShipMining_Pod_Mole"},
            {"guid": "spare", "className": "Cargo_ShipMining_Pod_Mole_Collapsed"},
            {"guid": "nameless", "className": "Cargo_ShipMining_Pod_Unknown"},
            {"guid": "area", "className": "ORIG_135c_Cargo_Area"},
            {"guid": "grid", "className": "ANVL_Pisces_CargoGrid"},
            {"guid": "door", "className": "Door_Ship_Exterior_AEGS_Reclaimer_Cargo_Door"},
            {"guid": "bed", "className": "Door_Ship_Exterior_CNOU_Nomad_Cargo_Bed"}
        ]}));
        catalogs.values.pods.insert("pod".to_string(), 12.0);
        let port = |guid: &str, below: Vec<Json>| serde_json::json!({"name": "p", "itemGuid": guid, "ports": below});
        // The Mole in LIVE 4.10.3: eight pods, sixteen collapsed spares, at the top and further down the tree.
        let mut mole: Vec<Json> = (0..8).map(|_| port("pod", vec![])).collect();
        mole.extend((0..16).map(|_| port("spare", vec![])));
        assert_eq!(cargo_outside_grids(&catalogs, &mole), Some(96.0), "the spares hold nothing");
        assert_eq!(cargo_outside_grids(&catalogs, &[port("grid", vec![port("pod", vec![])])]), Some(12.0));
        // Space whose capacity the data do not give cannot be counted: the cargo area, a pod without a capacity.
        assert_eq!(cargo_outside_grids(&catalogs, &[port("grid", vec![port("area", vec![])])]), None);
        assert_eq!(cargo_outside_grids(&catalogs, &[port("pod", vec![]), port("nameless", vec![])]), None);
        assert_eq!(cargo_outside_grids(&catalogs, &[port("bed", vec![])]), None, "the cargo bed of the Nomad");
        // Counter-check: a ship with cargo grids and a door named cargo has all its cargo in the grids.
        let plain = [port("grid", vec![]), port("door", vec![]), port("unknown", vec![])];
        assert_eq!(cargo_outside_grids(&catalogs, &plain), Some(0.0));
        assert_eq!(cargo_outside_grids(&catalogs, &[]), Some(0.0));
    }

    /// A made-up catalog with a laser head (two module ports, an extraction beam beside the cracking one), an active
    /// and a passive module, a gadget and an item without mining values.
    #[test]
    fn the_mining_file_holds_heads_modules_and_gadgets_with_their_values() {
        let catalog: Json = serde_json::from_str(r#"{
            "items": [
              {"guid": "head", "className": "Mining_Laser_S1", "name": {"en": "Helix I"}, "type": "WeaponMining", "size": 1,
               "stats": {"miningLaser": {"throttleMinimum": 0.2, "filterModifier": 40,
                                         "modifiers": {"resistance": -30, "instability": null, "optimalChargeWindowSize": -20}},
                         "weapon": {"fireActions": [
                           {"hitType": "Extraction", "damagePerSecond": {"energy": 1295}},
                           {"hitType": "ElectricArc", "damagePerSecond": {"energy": 3150},
                            "fullDamageRange": 40, "zeroDamageRange": 45}]}}},
              {"guid": "surge", "className": "mining_modules_active_surge", "name": {"en": "Surge"}, "type": "MiningModifier",
               "stats": {"miningModifier": {"activationMethod": "ActivateOnDemand", "charges": 5,
                         "laser": {"lifetime": 15, "modifiers": {"resistance": -15}},
                         "fireActionModifiers": [{"fireActionIndex": 0, "damageMultiplier": 1.5}]}}},
              {"guid": "focus", "className": "mining_modules_passive_focus", "name": {"en": "Focus III"}, "type": "MiningModifier",
               "stats": {"miningModifier": {"activationMethod": "ActivateOnAttach", "charges": -1,
                         "laser": {"lifetime": null, "modifiers": {"instability": 10}}, "fireActionModifiers": []}}},
              {"guid": "sabir", "className": "mining_gadget_sabir", "name": {"en": "Sabir"}, "type": "Gadget",
               "stats": {"miningModifier": {"rock": {"modifiers": {"resistance": -50, "instability": 15}}, "fireActionModifiers": []}}},
              {"guid": "base", "className": "behr_ltp_kinetic_01_MiningBase", "name": {"en": ""}, "type": "Gadget",
               "stats": {"miningModifier": {"rock": {"modifiers": {"resistance": -10}}, "fireActionModifiers": []}}},
              {"guid": "seat", "className": "Seat", "name": {"en": "Seat"}, "type": "Seat"},
              {"guid": "tool", "className": "multitool_cutter_AI", "name": {"en": "Multi-Tool"}, "type": "WeaponPersonal",
               "stats": {"miningLaser": {"throttleMinimum": 0.001},
                         "weapon": {"fireActions": [{"hitType": "ElectricArc", "damagePerSecond": {"energy": 0}}]}}}
            ],
            "loadouts": [{"entityGuid": "head", "ports": [
              {"name": "module_1", "types": [{"type": "MiningModifier"}]},
              {"name": "module_2", "types": [{"type": "MiningModifier"}]},
              {"name": "light", "types": [{"type": "Light"}]}]}]
        }"#).unwrap();
        let mut catalogs = Catalogs::default();
        catalogs.add(&catalog);
        let file = mining_file(&catalogs);
        let gear = versekit_core::mining_gear::Gear::from_values(Some(&file), None);
        let head = gear.head("head").expect("the head");
        assert_eq!((head.name.as_str(), head.size, head.power, head.slots), ("Helix I", 1, 3150.0, 2));
        assert_eq!((head.resistance, head.instability), (-30.0, 0.0), "not set counts as no change");
        assert_eq!((head.throttle_min, head.extraction), (Some(0.2), Some(1295.0)));
        assert_eq!((head.range_full, head.range_zero), (Some(40.0), Some(45.0)), "the ranges of the cracking beam");
        assert_eq!((head.more.window, head.more.filter, head.more.rate), (Some(-20.0), Some(40.0), None));
        let surge = gear.module("surge").expect("the active module");
        assert!(surge.active);
        assert_eq!((surge.charges, surge.lifetime, surge.power, surge.resistance), (Some(5), Some(15.0), 1.5, -15.0));
        let focus = gear.module("focus").expect("the passive module");
        assert_eq!((focus.active, focus.charges, focus.lifetime, focus.power), (false, None, None, 1.0));
        assert_eq!(gear.gadget("sabir").map(|g| (g.resistance, g.instability)), Some((-50.0, 15.0)));
        assert_eq!((gear.heads.len(), gear.modules.len(), gear.gadgets.len()), (1, 2, 1), "the seat is no mining part, a tool without power no head");
        // The reader skips a head without power as well: the file itself must not hold it.
        let written = match file.get("koepfe") {
            Some(versekit_core::jsonio::Value::Array(heads)) => heads.len(),
            _ => 0,
        };
        assert_eq!(written, 1, "a tool without power is not written as a head");
    }
}
