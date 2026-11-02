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

//! What a wreck has on board (`salvage.py` of 4.12): the parts a ship carries from the factory that can be
//! taken out, counted per part, from the ship file of erkul; their shop value; and the wrecks looked up
//! before, kept in `bergung.json` so that they stand there without the net.
//!
//! Unlike 4.12 a wreck is kept and found under one key, the slim name of the ship: 4.12 kept it under the
//! code of erkul and looked for it under another key, so it never found one again.

use crate::erkul_fetch::{self, Feed};
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

/// The file of the wrecks looked up before.
pub const FILE: &str = "bergung.json";
const FORMAT: i64 = 1;
/// So many wrecks are kept; the oldest go first.
pub const MAX_SHIPS: usize = 40;

/// What counts as loot: the kinds of erkul (`type` or `category`) of parts that can be taken out. Hull
/// armour, fuel tanks and life support cannot and stay out.
pub const REMOVABLE: [&str; 21] = [
    "PowerPlant",
    "Cooler",
    "Shield",
    "QuantumDrive",
    "Radar",
    "JumpDrive",
    "WeaponGun",
    "Turret",
    "MissileLauncher",
    "Missile",
    "BombLauncher",
    "Bomb",
    "MiningLaser",
    "WeaponMining",
    "SalvageHead",
    "TractorBeam",
    "QuantumInterdictionGenerator",
    "EMP",
    "MiningModifier",
    "SalvageModifier",
    "FlightController",
];

/// One part of the factory loadout with how many of it there are.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The code of the part; through it hangs the shop price.
    pub reference: String,
    pub name: String,
    /// The kind of erkul (`WeaponGun`).
    pub kind: String,
    pub size: Option<i64>,
    /// The grade as erkul writes it (a number or a letter); `Null` without one.
    pub grade: Value,
    pub count: i64,
}

impl Part {
    /// The grade as text; empty without one.
    pub fn grade_text(&self) -> String {
        text(Some(&self.grade))
    }
}

fn list(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn truthy(value: Option<&Value>) -> bool {
    crate::hangar::truthy(value)
}

/// The text of a value as Python's `str` writes it; empty for none.
fn text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(value) => crate::basetool::error::py_str(value),
    }
}

/// Every fitted slot of a ship, down through turrets and arms (`salvage._collect_parts`). A fixed slot is
/// no loot, but its children are walked: a fixed turret holds guns that can be swapped.
fn collect(node: Option<&Value>, out: &mut Vec<Part>) {
    for slot in list(node).iter().filter(|s| matches!(s, Value::Object(_))) {
        let item = slot.get("item").filter(|i| matches!(i, Value::Object(_)));
        if let Some(item) = item
            && truthy(item.get("ref"))
            && slot.get("kind").and_then(Value::as_str) != Some("fixed")
        {
            // `type` before `category`: a gun is `category: AssembledWeapon` and `type: WeaponGun`.
            let kind = [item.get("type"), item.get("category")]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|k| REMOVABLE.contains(k));
            if let Some(kind) = kind {
                let name = [item.get("i18n").and_then(|i| i.get("name")), item.get("className")]
                    .into_iter()
                    .find(|v| truthy(*v))
                    .map_or_else(|| "?".to_string(), text);
                out.push(Part {
                    reference: text(item.get("ref")),
                    name,
                    kind: kind.to_string(),
                    size: crate::hangar::py_int(item.get("size")),
                    grade: item.get("grade").cloned().unwrap_or(Value::Null),
                    count: 0,
                });
            }
        }
        for field in ["slots", "children", "ports", "hardpoints"] {
            collect(slot.get(field), out);
        }
        if let Some(item) = item {
            for field in ["slots", "ports", "hardpoints"] {
                collect(item.get(field), out);
            }
        }
    }
}

/// The parts of a ship file that can be taken out (`salvage.factory_loadout`): equal parts on one line with
/// their count, by kind, the biggest first, then by name.
pub fn loadout(raw: &Value) -> Vec<Part> {
    if !matches!(raw, Value::Object(_)) {
        return Vec::new();
    }
    let mut found = Vec::new();
    collect(raw.get("slots"), &mut found);
    let mut counted: Vec<Part> = Vec::new();
    for part in found {
        match counted.iter_mut().find(|p| p.reference == part.reference) {
            Some(known) => known.count += 1,
            None => counted.push(Part { count: 1, ..part }),
        }
    }
    counted.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| b.size.unwrap_or(0).cmp(&a.size.unwrap_or(0)))
            .then_with(|| a.name.cmp(&b.name))
    });
    counted
}

/// The shop value of the parts (`salvage.value`): the sum and how many pieces have a price and how many
/// have none. Nothing is guessed: what has no price is counted and named.
pub fn value(parts: &[Part], price_of: impl Fn(&str) -> Option<f64>) -> (f64, i64, i64) {
    let (mut total, mut with, mut without) = (0.0, 0, 0);
    for part in parts {
        let count = if part.count == 0 { 1 } else { part.count };
        match price_of(&part.reference).filter(|p| *p != 0.0) {
            Some(price) => {
                total += price * count as f64;
                with += count;
            }
            None => without += count,
        }
    }
    (total, with, without)
}

/// The files of erkul's index: per ship code the path of its file, in the order of the index; a code that
/// comes again takes the later path.
pub fn ship_files(feed: &dyn Feed, catalog: &Value) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    for group in list(catalog.get("groups")) {
        let Some(path) = group.get("indexPath").and_then(Value::as_str).filter(|p| !p.is_empty()) else { continue };
        let Some(index) = erkul_fetch::fetch(feed, &format!("{}/{path}", erkul_fetch::BRANCH)) else { continue };
        for blob in list(index.get("blobs")) {
            let (Some(id), Some(path)) =
                (blob.get("id").and_then(Value::as_str), blob.get("path").and_then(Value::as_str))
            else {
                continue;
            };
            match files.iter_mut().find(|(known, _)| known == id) {
                Some(entry) => entry.1 = path.to_string(),
                None => files.push((id.to_string(), path.to_string())),
            }
        }
    }
    files
}

/// Why a lookup brought no wreck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// erkul gave no catalog or no index: no net, or erkul is down. Trying again later may help.
    Unreachable,
    /// erkul answered, but no code fits the name: the source does not list the ship.
    Unknown,
}

/// Looks a wreck up at erkul: the catalog, its index, the file of the ship. The code of erkul and the parts
/// (`pages._load_salvage`), or why there are none.
///
/// # Errors
///
/// [`Missing::Unreachable`] without an answer of erkul, [`Missing::Unknown`] where no code fits the name.
pub fn look_up(feed: &dyn Feed, name: &str, maker: &str) -> Result<(String, Vec<Part>), Missing> {
    let catalog = erkul_fetch::ship_catalog(feed).ok_or(Missing::Unreachable)?;
    let files = ship_files(feed, &catalog);
    if files.is_empty() {
        return Err(Missing::Unreachable);
    }
    let codes: Vec<String> = files.iter().map(|(code, _)| code.clone()).collect();
    let code = crate::ships::erkul_code(&codes, name, maker).ok_or(Missing::Unknown)?;
    let path = files.iter().find(|(c, _)| *c == code).map(|(_, p)| p.clone()).unwrap_or_default();
    let raw = erkul_fetch::fetch(feed, &format!("{}/{path}", erkul_fetch::BRANCH)).ok_or(Missing::Unreachable)?;
    Ok((code, loadout(&raw)))
}

// ------------------------------------------------------------ the wrecks looked up before

/// One wreck looked up before.
#[derive(Clone, Debug, PartialEq)]
pub struct Wreck {
    pub name: String,
    pub parts: Vec<Part>,
    /// When it was looked up (seconds since 1970).
    pub when: f64,
    /// The game version of the slots it was looked up with.
    pub version: String,
}

fn part_value(part: &Part) -> Value {
    Value::Object(vec![
        ("ref".into(), Value::str(&part.reference)),
        ("name".into(), Value::str(&part.name)),
        ("art".into(), Value::str(&part.kind)),
        ("groesse".into(), part.size.map_or(Value::Null, Value::int)),
        ("guete".into(), part.grade.clone()),
        ("anzahl".into(), Value::int(part.count)),
    ])
}

fn part_of(value: &Value) -> Option<Part> {
    let reference = text(value.get("ref"));
    if reference.is_empty() {
        return None;
    }
    Some(Part {
        reference,
        name: text(value.get("name")),
        kind: text(value.get("art")),
        size: crate::hangar::py_int(value.get("groesse")),
        grade: value.get("guete").cloned().unwrap_or(Value::Null),
        count: crate::hangar::py_int(value.get("anzahl")).unwrap_or(1),
    })
}

/// The wrecks of the file, by their key; none where the file is missing or of another format.
pub fn load(paths: &Paths) -> Vec<(String, Wreck)> {
    let Ok(data) = jsonio::read_json(&paths.app_file(FILE)) else { return Vec::new() };
    if !crate::stock_data::format_is(&data, FORMAT) {
        return Vec::new();
    }
    let Some(Value::Object(ships)) = data.get("schiffe") else { return Vec::new() };
    ships
        .iter()
        .map(|(key, entry)| {
            let wreck = Wreck {
                name: text(entry.get("name")),
                parts: list(entry.get("teile")).iter().filter_map(part_of).collect(),
                when: entry.get("stand").and_then(crate::stock_data::py_float_value).unwrap_or(0.0),
                version: text(entry.get("spielversion")),
            };
            (key.clone(), wreck)
        })
        .collect()
}

fn save(paths: &Paths, wrecks: &[(String, Wreck)]) -> std::io::Result<()> {
    let ships: Map = wrecks
        .iter()
        .map(|(key, wreck)| {
            let entry = Value::Object(vec![
                ("name".into(), Value::str(&wreck.name)),
                ("teile".into(), Value::Array(wreck.parts.iter().map(part_value).collect())),
                ("stand".into(), Value::Float(wreck.when)),
                ("spielversion".into(), Value::str(&wreck.version)),
            ]);
            (key.clone(), entry)
        })
        .collect();
    let data = Value::Object(vec![("format".into(), Value::int(FORMAT)), ("schiffe".into(), Value::Object(ships))]);
    jsonio::save_json(&paths.app_file(FILE), &data, 1, false)
}

/// The key a wreck is kept under: the slim name of the ship.
pub fn key_of(name: &str) -> String {
    crate::ships::slim_name(name)
}

/// A wreck looked up before, or `None`; one of another game version counts as none: CIG changes the
/// loadouts with every patch.
pub fn remembered(paths: &Paths, name: &str, version: &str) -> Option<Wreck> {
    let key = key_of(name);
    load(paths).into_iter().find(|(k, _)| *k == key).map(|(_, w)| w).filter(|w| w.version == version)
}

/// Keeps a wreck looked up; beyond [`MAX_SHIPS`] the oldest go.
///
/// # Errors
///
/// When the file cannot be written.
pub fn remember(paths: &Paths, wreck: Wreck) -> std::io::Result<()> {
    let key = key_of(&wreck.name);
    let mut wrecks = load(paths);
    match wrecks.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = wreck,
        None => wrecks.push((key, wreck)),
    }
    if wrecks.len() > MAX_SHIPS {
        let mut by_age: Vec<(String, f64)> = wrecks.iter().map(|(k, w)| (k.clone(), w.when)).collect();
        by_age.sort_by(|a, b| a.1.total_cmp(&b.1));
        let gone: Vec<String> = by_age.into_iter().take(wrecks.len() - MAX_SHIPS).map(|(k, _)| k).collect();
        wrecks.retain(|(k, _)| !gone.contains(k));
    }
    save(paths, &wrecks)
}

/// Forgets every wreck; how many there were.
///
/// # Errors
///
/// When the file cannot be written.
pub fn forget(paths: &Paths) -> std::io::Result<usize> {
    let count = load(paths).len();
    save(paths, &[])?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship() -> Value {
        jsonio::loads(
            r#"{"slots": [
              {"kind": "swappable", "item": {"ref": "c1", "type": "Cooler", "size": 1, "grade": "B", "i18n": {"name": "Bracer"}}},
              {"kind": "swappable", "item": {"ref": "c1", "type": "Cooler", "size": 1, "grade": "B", "i18n": {"name": "Bracer"}}},
              {"kind": "fixed", "item": {"ref": "t1", "type": "Turret", "size": 3, "ports": [
                 {"kind": "swappable", "item": {"ref": "g1", "category": "AssembledWeapon", "type": "WeaponGun", "size": 3, "className": "CF-337"}}]}},
              {"kind": "swappable", "item": {"ref": "a1", "type": "Armor", "size": 2}},
              {"kind": "swappable", "item": {"type": "Shield", "size": 2}},
              {"kind": "swappable", "item": {"ref": "s1", "category": "Shield", "size": 2}, "children": [
                 {"item": {"ref": "q1", "type": "QuantumDrive", "size": 1}}]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn the_loadout_counts_what_can_be_taken_out_down_through_turrets() {
        let parts = loadout(&ship());
        let shown: Vec<(&str, &str, i64)> = parts.iter().map(|p| (p.kind.as_str(), p.name.as_str(), p.count)).collect();
        // The fixed turret is no loot, its gun is; armour is no loot; a part without a code is none.
        assert_eq!(
            shown,
            [("Cooler", "Bracer", 2), ("QuantumDrive", "?", 1), ("Shield", "?", 1), ("WeaponGun", "CF-337", 1)]
        );
        assert_eq!(parts[0].grade_text(), "B");
        assert!(loadout(&Value::Null).is_empty());
    }

    #[test]
    fn the_value_counts_pieces_and_guesses_nothing() {
        let parts = loadout(&ship());
        let price = |code: &str| match code {
            "c1" => Some(1000.0),
            "g1" => Some(0.0),
            _ => None,
        };
        assert_eq!(value(&parts, price), (2000.0, 2, 3));
    }

    #[test]
    fn a_wreck_is_kept_found_under_its_name_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("versekit-bergung-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, home.as_str())],
            &home,
        ));
        let wreck =
            |name: &str, when: f64| Wreck { name: name.into(), parts: loadout(&ship()), when, version: "4.3".into() };
        remember(&paths, wreck("Cutlass Black", 1.0)).unwrap();
        // The name as typed with other spelling finds it; another game version does not.
        let found = remembered(&paths, "cutlass-black", "4.3").expect("kept");
        assert_eq!(found.parts, loadout(&ship()));
        assert!(remembered(&paths, "Cutlass Black", "4.4").is_none());
        for n in 0..MAX_SHIPS {
            remember(&paths, wreck(&format!("Ship {n}"), 10.0 + n as f64)).unwrap();
        }
        assert_eq!(load(&paths).len(), MAX_SHIPS);
        assert!(remembered(&paths, "Cutlass Black", "4.3").is_none(), "the oldest went first");
        assert_eq!(forget(&paths).unwrap(), MAX_SHIPS);
        assert!(load(&paths).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wreck_is_looked_up_through_the_catalog_and_the_index() {
        let packed = |value: &str| erkul_fetch::pack(value);
        let catalog = r#"{"groups": [{"indexPath": "ships.idx.bin"}]}"#;
        let index = r#"{"blobs": [{"id": "drak_cutlass_black", "path": "old.bin"}, {"id": "anvl_arrow", "path": "arrow.bin"},
                                   {"id": "drak_cutlass_black", "path": "cutlass.bin"}]}"#;
        let ship_text = jsonio::dumps(&ship(), None, false);
        let feed = erkul_fetch::FnFeed(|path: &str| match path {
            "LIVE/catalog.bin" => Some(packed(catalog)),
            "LIVE/ships.idx.bin" => Some(packed(index)),
            "LIVE/cutlass.bin" => Some(packed(&ship_text)),
            _ => None,
        });
        let (code, parts) = look_up(&feed, "Cutlass Black", "Drake Interplanetary").expect("found");
        assert_eq!(code, "drak_cutlass_black");
        assert_eq!(parts.len(), 4, "the later path of a code counts");
        assert_eq!(look_up(&feed, "Idris", "Aegis"), Err(Missing::Unknown));
        let silent = erkul_fetch::FnFeed(|_: &str| None);
        assert_eq!(look_up(&silent, "Cutlass Black", "Drake"), Err(Missing::Unreachable));
    }
}
