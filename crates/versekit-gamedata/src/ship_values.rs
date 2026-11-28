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

//! The base values of the ships the hangar page shows on hover: size, SCM speed and cargo. They go into the ships
//! file of the game data as the field `werte` (see [`crate::ship_slots::ships_file`]).
//!
//! Where each comes from, read without the key (checked against LIVE 4.10.2, 12913470):
//! - size: `VehicleComponentParams.maxBoundingBoxSize` of the ship Record. Its axes are not the same on every ship
//!   (Gladius `y` 21, `x` 17.5, `z` 5.5 m; Caterpillar `y` 13.4, `x` 39.5, `z` 111.5 m; C2 Hercules `y` 70, `x` 94,
//!   `z` 23 m), so the longest side counts as the length and the shortest as the height: 16 of 18 ships of a
//!   sample match UEX and starcitizen.tools that way, 7 of 18 by the axes. A box many makers share is a stand-in
//!   ([`real_sizes`]).
//! - SCM speed: `IFCSParams.scmSpeed` of the flight controller the ship has at the factory
//!   (`Controller_Flight_AEGS_Gladius`: 226 m/s).
//! - cargo: `cargoScu` of the ship in the catalog p4kx builds; left out where p4kx could not read every cargo grid
//!   (`cargoGridsWithIssue` above 0), since the sum is then too low.

use std::collections::HashMap;

use p4kx_dataforge::{DataForge, TreeBuilder};
use serde_json::Value as Json;
use versekit_core::jsonio::{Map, Value};

use crate::fields::{component, number, structure};

/// What the DataForge says about the ships beyond the catalog of p4kx.
#[derive(Clone, Debug, Default)]
pub struct ShipValues {
    /// Ship identifier (lower case) → length, width and height in metres.
    pub sizes: HashMap<String, [f64; 3]>,
    /// Flight controller identifier (lower case) → SCM speed in m/s.
    pub scm: HashMap<String, f64>,
    /// Mining pod identifier (lower case) → what the pod holds in SCU (`ResourceContainer.capacity`). The pods of the
    /// Mole, the Prospector and the Golem are no cargo grids, so the cargo of the catalog leaves them out.
    pub pods: HashMap<String, f64>,
}

impl ShipValues {
    /// Reads the vehicles (`entities/spaceships`, `entities/groundvehicles`) and the flight controllers
    /// (`entities/scitem/…/controller/…`) of the DataForge.
    pub fn read(forge: &DataForge<'_>) -> ShipValues {
        let mut builder = TreeBuilder::new();
        let mut values = ShipValues::default();
        let mut boxes: Vec<(String, String, [f64; 3])> = Vec::new();
        for record in forge.records() {
            let (Ok("EntityClassDefinition"), Ok(path)) = (record.type_name(), record.record_path()) else { continue };
            let path = path.to_ascii_lowercase();
            let vehicle = path.contains("entities/spaceships/") || path.contains("entities/groundvehicles/");
            let controller = path.contains("entities/scitem/") && path.contains("/controller/");
            let pod = path.contains("entities/scitem/ships/utility/mining/miningpods/");
            if !vehicle && !controller && !pod {
                continue;
            }
            let Ok(tree) = builder.build(&record) else { continue };
            let guid = tree.guid().to_string().to_ascii_lowercase();
            if pod {
                if let Some(scu) = container_scu(structure(component(&tree, "ResourceContainer"), "capacity")) {
                    values.pods.insert(guid, scu);
                }
            } else if vehicle {
                let size = structure(component(&tree, "VehicleComponentParams"), "maxBoundingBoxSize");
                let sides = ["x", "y", "z"].map(|axis| number(size, axis).filter(|v| *v > 0.0));
                if let [Some(a), Some(b), Some(c)] = sides {
                    // The maker is the first word of the file name (`tmbl_cyclone` → `tmbl`).
                    let file = path.rsplit('/').next().unwrap_or_default();
                    let maker = file.split('_').next().unwrap_or_default().to_string();
                    boxes.push((guid, maker, longest_first([a, b, c])));
                }
            } else if let Some(scm) = number(component(&tree, "IFCSParams"), "scmSpeed").filter(|v| *v > 0.0) {
                values.scm.insert(guid, scm);
            }
        }
        values.sizes = real_sizes(boxes);
        values
    }

    /// The field `werte` of a ship of the catalog: `scm` (m/s), `masse` (length, width, height in metres), `fracht`
    /// (SCU of the cargo grids) and `erz` (SCU of the mining pods, only for a ship that has any), each only where the
    /// data have it; `None` when they have none. The ore of the pods is no cargo: the MOLE carries 32 SCU of cargo and
    /// 96 SCU of ore, as the game and the shops count it. `controller` is the identifier of the flight controller the
    /// ship has at the factory. `outside` is the cargo space of the ship beyond its cargo grids that was counted
    /// (`ship_slots::cargo_outside_grids`: the mining pods), `Some(0.0)` for a ship without any, and `None` for a ship
    /// with cargo space that cannot be counted: no `fracht` then. `fabricator_grids` is how many of its cargo grids
    /// sit at a fabricator port; they are neither cargo space nor unreadable grids.
    pub fn of(&self, ship: &Json, controller: Option<&str>, outside: Option<f64>, fabricator_grids: u64) -> Option<Value> {
        let mut out: Map = Vec::new();
        if let Some(scm) = controller.and_then(|guid| self.scm.get(&guid.to_ascii_lowercase())) {
            out.push(("scm".to_string(), Value::Float(*scm)));
        }
        let guid = ship["guid"].as_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(size) = self.sizes.get(&guid) {
            out.push(("masse".to_string(), Value::Array(size.iter().map(|v| Value::Float(*v)).collect())));
        }
        let unreadable = ship["cargoGridsWithIssue"].as_u64().unwrap_or(0).saturating_sub(fabricator_grids);
        let size = self.sizes.get(&guid);
        let fits = |scu: f64| size.is_none_or(|size| cargo_fits(scu, size));
        let readable = unreadable == 0 && outside.is_some();
        let ore = outside.unwrap_or(0.0);
        let known = known_cargo(ship["className"].as_str().unwrap_or_default());
        // The data win once they count as much themselves.
        let grids = ship["cargoScu"].as_f64().filter(|v| v.is_finite() && *v >= 0.0).map(|v| known.map_or(v, |k| v.max(k)));
        // Both together must fit the box of the ship; each is shown on its own.
        if let Some(scu) = grids.filter(|v| readable && fits(*v + ore)) {
            out.push(("fracht".to_string(), Value::Float(scu)));
            if ore > 0.0 {
                out.push(("erz".to_string(), Value::Float(ore)));
            }
            // The most it holds with cargo modules, where that is more than the factory loadout holds.
            if let Some(modules) = module_cargo(ship["className"].as_str().unwrap_or_default()).filter(|m| *m > scu) {
                out.push(("fracht_module".to_string(), Value::Float(modules)));
            }
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }
}

/// Ships whose hold the DataForge does not hold as cargo grids, with their cargo as the game shows it: the
/// beginning of the class name and the SCU. The 890 Jump (LIVE 4.10.3, `ORIG_890Jump`) has six grids on its hangar
/// deck in its loadout (2 × 30 + 4 × 10 = 100 SCU) and nothing else that holds a capacity, while its hold behind the
/// cargo lift carries the rest: 388 SCU in all (starcitizen.tools, UEX 4.8.3).
const KNOWN_CARGO: [(&str, f64); 1] = [("orig_890jump", 388.0)];

/// Ships whose cargo depends on the modules they carry, with the most cargo they hold with cargo modules: the class
/// name and the SCU. Their factory loadout has modules without cargo, so `fracht` is the cargo without them.
/// - Retaliator (`AEGS_Retaliator`, LIVE 4.10.3): `AEGS_Retaliator_Module_Front_Base` and `…_Rear_Base` hold no grid
///   (0 SCU); the cargo modules hold the grids `AEGS_Retaliator_Front_CargoGrid_Main` 3.75 × 6.25 × 2.5 m (30 SCU),
///   `…_Left` and `…_Right` 2.5 × 1.25 × 2.5 m (4 SCU each) and `AEGS_Retaliator_Rear_CargoGrid` 3.75 × 7.5 × 2.5 m
///   (36 SCU): 38 + 36 = 74 SCU, as starcitizen.tools (front 38, rear 36) and UEX say.
/// - Aurora Mk II (`RSI_Aurora_Mk2`): 2 SCU in its rack; the cargo module (TS) adds 6 SCU, 8 SCU in all
///   (starcitizen.tools; its grid `RSI_Aurora_Mk2_Module_CargoGrid` is 1.25 × 3.75 × 1.25 m, 3 SCU, on both sides).
const MODULE_CARGO: [(&str, f64); 2] = [("aegs_retaliator", 74.0), ("rsi_aurora_mk2", 8.0)];

/// The cargo with cargo modules of [`MODULE_CARGO`] for a ship class (the whole name, in any case); `None` for any
/// other.
fn module_cargo(class: &str) -> Option<f64> {
    MODULE_CARGO.iter().find(|(name, _)| class.eq_ignore_ascii_case(name)).map(|(_, scu)| *scu)
}

/// The cargo of [`KNOWN_CARGO`] for a ship class, by the beginning of its name in any case; `None` for any other.
fn known_cargo(class: &str) -> Option<f64> {
    let class = class.to_ascii_lowercase();
    KNOWN_CARGO.iter().find(|(start, _)| class.starts_with(start)).map(|(_, scu)| *scu)
}

/// What a `ResourceContainer` capacity holds in SCU: in standard (`standardCargoUnits`), centi (`centiSCU`) or micro
/// (`microSCU`) cargo units, as the cargo grids of p4kx are read; `None` for anything else.
fn container_scu(capacity: Option<&p4kx_dataforge::StructValue<'_>>) -> Option<f64> {
    let (field, divisor) = match capacity?.type_name() {
        "SStandardCargoUnit" => ("standardCargoUnits", 1.0),
        "SCentiCargoUnit" => ("centiSCU", 100.0),
        "SMicroCargoUnit" => ("microSCU", 1_000_000.0),
        _ => return None,
    };
    number(capacity, field).map(|value| value / divisor).filter(|scu| scu.is_finite() && *scu >= 0.0)
}

/// The volume of one SCU in cubic metres: a cube of 1.25 m on a side.
const SCU_VOLUME: f64 = 1.25 * 1.25 * 1.25;

/// Whether the cargo can lie in the box of the ship at all: its volume is less than the volume of the box. A ship of
/// the data with more cargo than room has the cargo or the size of another ship; in LIVE 4.10.3 the fullest hold is
/// the Golem Ox's (64 SCU, a quarter of its box), the next the Hull C's (a tenth).
pub fn cargo_fits(scu: f64, size: &[f64; 3]) -> bool {
    scu * SCU_VOLUME <= size.iter().product::<f64>()
}

/// The three sides of a box as length, width and height: the longest first, the shortest last.
fn longest_first(mut sides: [f64; 3]) -> [f64; 3] {
    sides.sort_by(|a, b| b.total_cmp(a));
    sides
}

/// How many makers may share a box to the centimetre before it counts as a stand-in. In LIVE 4.10.2 two makers
/// share a hull (Cutlass and Shiv, Sabre and Defender, Nox and HoverQuad); the ground vehicles of five makers
/// (Cyclone, Ursa, Mule, STV, CSV and others) all have 6 × 8.75 × 3.5 m, which starcitizen.tools does not confirm
/// for any of them (Mule 4.2 × 3 × 2.7 m, Cyclone 6 × 4 × 2.5 m).
const SHARED_BY_AT_MOST: usize = 2;

/// The boxes that are a size of their ship: `(identifier, maker, box)` → identifier → box; a box of more makers
/// than [`SHARED_BY_AT_MOST`] is a stand-in, no size.
fn real_sizes(boxes: Vec<(String, String, [f64; 3])>) -> HashMap<String, [f64; 3]> {
    let key = |size: &[f64; 3]| size.map(|v| (v * 100.0).round() as i64);
    let mut makers: HashMap<[i64; 3], Vec<&str>> = HashMap::new();
    for (_, maker, size) in &boxes {
        let list = makers.entry(key(size)).or_default();
        if !list.contains(&maker.as_str()) {
            list.push(maker);
        }
    }
    let shared: Vec<[i64; 3]> =
        makers.into_iter().filter(|(_, list)| list.len() > SHARED_BY_AT_MOST).map(|(size, _)| size).collect();
    boxes.into_iter().filter(|(_, _, size)| !shared.contains(&key(size))).map(|(guid, _, size)| (guid, size)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(guid: &str, cargo: Json, issues: u64) -> Json {
        serde_json::json!({"guid": guid, "cargoScu": cargo, "cargoGridsWithIssue": issues})
    }

    fn values() -> ShipValues {
        ShipValues {
            sizes: HashMap::from([("aa".to_string(), [21.0, 17.5, 5.5])]),
            scm: HashMap::from([("cc".to_string(), 226.0)]),
            pods: HashMap::new(),
        }
    }

    #[test]
    fn a_ship_gets_the_values_the_data_have_and_nothing_else() {
        let all = values().of(&ship("AA", serde_json::json!(46.0), 0), Some("CC"), Some(0.0), 0).expect("values");
        assert_eq!(all.get("scm"), Some(&Value::Float(226.0)));
        assert_eq!(
            all.get("masse"),
            Some(&Value::Array(vec![Value::Float(21.0), Value::Float(17.5), Value::Float(5.5)]))
        );
        assert_eq!(all.get("fracht"), Some(&Value::Float(46.0)));
        // No controller, no size, no cargo: no field of its own, and no values at all.
        let none = values().of(&ship("bb", Json::Null, 0), None, Some(0.0), 0);
        assert_eq!(none, None);
        let only_size = values().of(&ship("aa", Json::Null, 0), Some("unknown"), Some(0.0), 0).expect("size");
        assert_eq!(only_size.get("scm"), None);
        assert_eq!(only_size.get("fracht"), None);
        // A ship without a cargo grid has 0 SCU, which is a value.
        assert_eq!(
            values().of(&ship("bb", serde_json::json!(0.0), 0), None, Some(0.0), 0).and_then(|v| v.get("fracht").cloned()),
            Some(Value::Float(0.0))
        );
    }

    #[test]
    fn a_cargo_grid_of_a_fabricator_is_no_unreadable_grid() {
        let with = |issues: u64, fabricators: u64| {
            values().of(&ship("aa", serde_json::json!(456.0), issues), None, Some(0.0), fabricators)
                .and_then(|v| v.get("fracht").cloned())
        };
        assert_eq!(with(1, 1), Some(Value::Float(456.0)), "its only unreadable grid is the fabricator's");
        assert_eq!(with(2, 1), None, "another grid stays unreadable");
        assert_eq!(with(1, 0), None);
        assert_eq!(with(0, 1), Some(Value::Float(456.0)));
    }

    #[test]
    fn the_longest_side_is_the_length_and_the_shortest_the_height_however_the_axes_lie() {
        // Gladius `x` 17.5, `y` 21, `z` 5.5; Caterpillar `x` 39.5, `y` 13.4, `z` 111.5; X1 `x` 1.55, `y` 5.19, `z` 1.87.
        assert_eq!(longest_first([17.5, 21.0, 5.5]), [21.0, 17.5, 5.5]);
        assert_eq!(longest_first([39.5, 13.4, 111.5]), [111.5, 39.5, 13.4]);
        assert_eq!(longest_first([1.55, 5.19, 1.87]), [5.19, 1.87, 1.55]);
    }

    #[test]
    fn a_box_three_makers_share_is_a_stand_in_and_one_of_one_or_two_makers_a_size() {
        let entry = |id: &str, maker: &str, size: [f64; 3]| (id.to_string(), maker.to_string(), size);
        let boxes = vec![
            entry("cyclone", "tmbl", [8.75, 6.0, 3.5]),
            entry("ursa", "rsi", [8.75, 6.0, 3.5]),
            entry("mule", "drak", [8.75, 6.0, 3.5]),
            entry("cutlass", "drak", [37.5, 26.5, 11.5]),
            entry("cutlassred", "drak", [37.5, 26.5, 11.5]),
            entry("shiv", "glsn", [37.5, 26.5, 11.5]),
        ];
        let sizes = real_sizes(boxes);
        for stand_in in ["cyclone", "ursa", "mule"] {
            assert_eq!(sizes.get(stand_in), None, "{stand_in}");
        }
        // The variants of one ship and a hull of two makers share their box: that is their size.
        for hull in ["cutlass", "cutlassred", "shiv"] {
            assert_eq!(sizes.get(hull), Some(&[37.5, 26.5, 11.5]), "{hull}");
        }
    }

    #[test]
    fn cargo_that_does_not_fit_the_box_of_the_ship_is_left_out() {
        // More cargo than room: the box of a Constellation (62 × 26.6 × 16.3 m) is 26,882 m³.
        let small = ShipValues {
            sizes: HashMap::from([("aa".to_string(), [62.0, 26.6, 16.3])]),
            scm: HashMap::new(),
            pods: HashMap::new(),
        };
        let overfull = small.of(&ship("aa", serde_json::json!(14_000.0), 0), None, Some(0.0), 0).expect("size");
        assert_eq!(overfull.get("fracht"), None, "14,000 SCU are 27,300 m³ in a box of 26,880 m³");
        assert!(overfull.get("masse").is_some(), "the size stays");
        // Counter-check: the hold of a real Constellation, and the fullest hold of the game (Golem Ox 64 SCU in
        // 15.5 × 7 × 5 m), fit.
        let real = small.of(&ship("aa", serde_json::json!(96.0), 0), None, Some(0.0), 0).expect("values");
        assert_eq!(real.get("fracht"), Some(&Value::Float(96.0)));
        assert!(cargo_fits(64.0, &[15.5, 7.0, 5.0]));
        // Big freighters stay: Liberator 400 SCU (119 × 67 × 25 m), Hull C 4,608 (91.17 × 36.05 × 26.28), C2 696
        // (94 × 70 × 23), Idris-M 1,326 (243 × 127 × 57.5), Galaxy 64 (110 × 60 × 25).
        for (scu, size) in [
            (400.0, [119.0, 67.0, 25.0]),
            (4_608.0, [91.17, 36.05, 26.28]),
            (696.0, [94.0, 70.0, 23.0]),
            (1_326.0, [243.0, 127.0, 57.5]),
            (64.0, [110.0, 60.0, 25.0]),
        ] {
            assert!(cargo_fits(scu, &size), "{scu} SCU in {size:?}");
        }
        assert!(!cargo_fits(1_326.0, &[15.5, 7.0, 5.0]));
        // No size, no check.
        let unsized_ship = values().of(&ship("bb", serde_json::json!(1_326.0), 0), None, Some(0.0), 0).expect("cargo");
        assert_eq!(unsized_ship.get("fracht"), Some(&Value::Float(1_326.0)));
    }

    #[test]
    fn cargo_with_an_unreadable_grid_is_left_out() {
        let partial = values().of(&ship("aa", serde_json::json!(32.0), 1), None, Some(0.0), 0).expect("size");
        assert_eq!(partial.get("fracht"), None);
    }

    #[test]
    fn cargo_of_a_ship_with_space_that_cannot_be_counted_is_left_out() {
        // The 135c: its cargo area holds no capacity of its own in the data (LIVE 4.10.3), so its grids give 0 and the
        // ship has no cargo value at all.
        let area = values().of(&ship("aa", serde_json::json!(0.0), 0), None, None, 0).expect("size");
        assert_eq!(area.get("fracht"), None);
        assert!(area.get("masse").is_some(), "the size stays");
        // Counter-check: the same ship with all its cargo in grids keeps it.
        let counted = values().of(&ship("aa", serde_json::json!(32.0), 0), None, Some(0.0), 0).expect("values");
        assert_eq!(counted.get("fracht"), Some(&Value::Float(32.0)));
    }

    #[test]
    fn the_hold_of_the_890_jump_that_the_data_lack_is_counted_and_the_data_win_once_they_have_more() {
        let jump = |class: &str, grids: f64| {
            values()
                .of(&serde_json::json!({"guid": "bb", "className": class, "cargoScu": grids, "cargoGridsWithIssue": 0}), None, Some(0.0), 0)
                .and_then(|v| v.get("fracht").cloned())
        };
        // LIVE 4.10.3: the six grids of the hangar deck give 100 SCU; the game shows 388.
        assert_eq!(jump("ORIG_890Jump", 100.0), Some(Value::Float(388.0)));
        assert_eq!(jump("ORIG_890Jump_Drug_01", 100.0), Some(Value::Float(388.0)), "its variants too");
        assert_eq!(jump("ORIG_890Jump", 400.0), Some(Value::Float(400.0)), "the data count more themselves");
        // Counter-check: another ship keeps the sum of its grids.
        assert_eq!(jump("ORIG_600i_Explorer", 44.0), Some(Value::Float(44.0)));
    }

    #[test]
    fn a_ship_whose_cargo_hangs_on_its_modules_names_what_it_holds_with_cargo_modules() {
        let of = |class: &str, grids: f64| {
            values()
                .of(&serde_json::json!({"guid": "bb", "className": class, "cargoScu": grids, "cargoGridsWithIssue": 0}), None, Some(0.0), 0)
                .expect("values")
        };
        let retaliator = of("AEGS_Retaliator", 0.0);
        assert_eq!(
            (retaliator.get("fracht"), retaliator.get("fracht_module")),
            (Some(&Value::Float(0.0)), Some(&Value::Float(74.0))),
            "the factory modules hold nothing, the cargo modules 38 + 36"
        );
        let aurora = of("RSI_Aurora_Mk2", 2.0);
        assert_eq!((aurora.get("fracht"), aurora.get("fracht_module")), (Some(&Value::Float(2.0)), Some(&Value::Float(8.0))));
        // Counter-check: a ship without cargo modules, and another class that only begins like one, have no line.
        assert_eq!(of("AEGS_Hammerhead_GS", 64.0).get("fracht_module"), None);
        assert_eq!(of("RSI_Aurora_Mk2_MissileModule_TEMP", 2.0).get("fracht_module"), None);
    }

    #[test]
    fn the_ore_of_the_mining_pods_stands_beside_the_cargo_not_in_it() {
        // The Mole in LIVE 4.10.3: eight pods of 12 SCU and the rear lift of 32 SCU (the wiki and UEX: 32 SCU cargo,
        // 96 SCU ore); the Prospector: four pods of 8 SCU and no grid at all.
        let mole = values().of(&ship("aa", serde_json::json!(32.0), 0), None, Some(96.0), 0).expect("values");
        assert_eq!((mole.get("fracht"), mole.get("erz")), (Some(&Value::Float(32.0)), Some(&Value::Float(96.0))));
        let prospector = values().of(&ship("bb", serde_json::json!(0.0), 0), None, Some(32.0), 0).expect("values");
        assert_eq!((prospector.get("fracht"), prospector.get("erz")), (Some(&Value::Float(0.0)), Some(&Value::Float(32.0))));
        // A ship without pods has no ore line at all.
        let plain = values().of(&ship("bb", serde_json::json!(46.0), 0), None, Some(0.0), 0).expect("values");
        assert_eq!((plain.get("fracht"), plain.get("erz")), (Some(&Value::Float(46.0)), None));
        // Counter-check: pods that do not fit into the box of the ship together with the grids are neither.
        let small = ShipValues {
            sizes: HashMap::from([("aa".to_string(), [4.0, 3.0, 2.0])]),
            scm: HashMap::new(),
            pods: HashMap::new(),
        };
        let overfull = small.of(&ship("aa", serde_json::json!(0.0), 0), None, Some(96.0), 0).expect("size");
        assert_eq!((overfull.get("fracht"), overfull.get("erz")), (None, None));
    }
}
