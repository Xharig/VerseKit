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

use std::collections::{BTreeMap, HashMap};

use serde_json::Value as Json;
use versekit_core::erkul_slots::SWAPPABLE;
use versekit_core::jsonio::{Map, Value};

/// The slots of the ships, in the form of `erkul-schiffe.json`.
pub const SHIPS_FILE: &str = versekit_core::erkul_slots::GAME_SHIPS_FILE;
/// The parts with slots and the parts chosen by rule, in the form of `erkul-teile.json`.
pub const PARTS_FILE: &str = versekit_core::erkul_slots::GAME_PARTS_FILE;
/// The format of [`SHIPS_FILE`]: the one of `erkul-schiffe.json`.
pub const SHIPS_FORMAT: i64 = versekit_core::ships::ERKUL_FILE_FORMAT;
/// The format of [`PARTS_FILE`]: the one of `erkul-teile.json`.
pub const PARTS_FORMAT: i64 = versekit_core::erkul_slots::PARTS_FORMAT;

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

/// The maker of a ship: its name and the short code of its class name (`ANVL` of `ANVL_Hornet_F7CM_Mk2`).
fn maker_of(catalogs: &Catalogs, ship: &Json) -> (String, String) {
    let class = ship["className"].as_str().unwrap_or_default();
    let code = class.split('_').next().unwrap_or_default().to_lowercase();
    let name = ship["manufacturerGuid"].as_str().and_then(|g| catalogs.makers.get(g)).map(|m| english(&m["name"])).unwrap_or_default();
    (name, code)
}

/// The slots of the player ships in the form of `erkul-schiffe.json`.
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
        let (maker, code) = maker_of(catalogs, ship);
        if !maker.is_empty() && !code.is_empty() && !makers.iter().any(|(k, _)| *k == slim(&maker)) {
            makers.push((slim(&maker), Value::str(&code)));
        }
        let own = english(&ship["name"]);
        let name = [maker.as_str(), own.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" ");
        ships.push((
            key,
            Value::Object(vec![
                ("name".to_string(), Value::str(if name.is_empty() { class } else { &name })),
                ("id".to_string(), Value::str(class)),
                ("plaetze".to_string(), Value::Array(counts(&sizes))),
                ("slots".to_string(), Value::Array(slots)),
            ]),
        ));
    }
    Value::Object(vec![
        ("format".to_string(), Value::int(SHIPS_FORMAT)),
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
            ("verlangt".to_string(), Value::Array(Vec::new())),
            ("guete".to_string(), int(&item["grade"]).map_or(Value::str(""), |g| Value::str(g.to_string()))),
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
}
