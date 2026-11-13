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
            if !vehicle && !controller {
                continue;
            }
            let Ok(tree) = builder.build(&record) else { continue };
            let guid = tree.guid().to_string().to_ascii_lowercase();
            if vehicle {
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

    /// The field `werte` of a ship of the catalog: `scm` (m/s), `masse` (length, width, height in metres) and `fracht`
    /// (SCU), each only where the data have it; `None` when they have none. `controller` is the identifier of the
    /// flight controller the ship has at the factory. `cargo_counted` is false for a ship with cargo space the sum of
    /// its cargo grids leaves out (`ship_slots::cargo_outside_grids`): no `fracht` then.
    pub fn of(&self, ship: &Json, controller: Option<&str>, cargo_counted: bool) -> Option<Value> {
        let mut out: Map = Vec::new();
        if let Some(scm) = controller.and_then(|guid| self.scm.get(&guid.to_ascii_lowercase())) {
            out.push(("scm".to_string(), Value::Float(*scm)));
        }
        let guid = ship["guid"].as_str().unwrap_or_default().to_ascii_lowercase();
        if let Some(size) = self.sizes.get(&guid) {
            out.push(("masse".to_string(), Value::Array(size.iter().map(|v| Value::Float(*v)).collect())));
        }
        let unreadable = ship["cargoGridsWithIssue"].as_u64().unwrap_or(0);
        let size = self.sizes.get(&guid);
        let fits = |scu: f64| size.is_none_or(|size| cargo_fits(scu, size));
        let readable = unreadable == 0 && cargo_counted;
        if let Some(scu) = ship["cargoScu"].as_f64().filter(|v| v.is_finite() && *v >= 0.0 && readable && fits(*v)) {
            out.push(("fracht".to_string(), Value::Float(scu)));
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }
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
        }
    }

    #[test]
    fn a_ship_gets_the_values_the_data_have_and_nothing_else() {
        let all = values().of(&ship("AA", serde_json::json!(46.0), 0), Some("CC"), true).expect("values");
        assert_eq!(all.get("scm"), Some(&Value::Float(226.0)));
        assert_eq!(
            all.get("masse"),
            Some(&Value::Array(vec![Value::Float(21.0), Value::Float(17.5), Value::Float(5.5)]))
        );
        assert_eq!(all.get("fracht"), Some(&Value::Float(46.0)));
        // No controller, no size, no cargo: no field of its own, and no values at all.
        let none = values().of(&ship("bb", Json::Null, 0), None, true);
        assert_eq!(none, None);
        let only_size = values().of(&ship("aa", Json::Null, 0), Some("unknown"), true).expect("size");
        assert_eq!(only_size.get("scm"), None);
        assert_eq!(only_size.get("fracht"), None);
        // A ship without a cargo grid has 0 SCU, which is a value.
        assert_eq!(
            values().of(&ship("bb", serde_json::json!(0.0), 0), None, true).and_then(|v| v.get("fracht").cloned()),
            Some(Value::Float(0.0))
        );
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
        let small = ShipValues { sizes: HashMap::from([("aa".to_string(), [62.0, 26.6, 16.3])]), scm: HashMap::new() };
        let overfull = small.of(&ship("aa", serde_json::json!(14_000.0), 0), None, true).expect("size");
        assert_eq!(overfull.get("fracht"), None, "14,000 SCU are 27,300 m³ in a box of 26,880 m³");
        assert!(overfull.get("masse").is_some(), "the size stays");
        // Counter-check: the hold of a real Constellation, and the fullest hold of the game (Golem Ox 64 SCU in
        // 15.5 × 7 × 5 m), fit.
        let real = small.of(&ship("aa", serde_json::json!(96.0), 0), None, true).expect("values");
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
        let unsized_ship = values().of(&ship("bb", serde_json::json!(1_326.0), 0), None, true).expect("cargo");
        assert_eq!(unsized_ship.get("fracht"), Some(&Value::Float(1_326.0)));
    }

    #[test]
    fn cargo_with_an_unreadable_grid_is_left_out() {
        let partial = values().of(&ship("aa", serde_json::json!(32.0), 1), None, true).expect("size");
        assert_eq!(partial.get("fracht"), None);
    }

    #[test]
    fn cargo_of_a_ship_with_space_outside_its_grids_is_left_out() {
        // The Mole: 32 SCU of the rear lift are the only grid, the six mining pods are not (LIVE 4.10.3).
        let mole = values().of(&ship("aa", serde_json::json!(32.0), 0), None, false).expect("size");
        assert_eq!(mole.get("fracht"), None);
        assert!(mole.get("masse").is_some(), "the size stays");
        // Counter-check: the same ship with all its cargo in grids keeps it.
        let counted = values().of(&ship("aa", serde_json::json!(32.0), 0), None, true).expect("values");
        assert_eq!(counted.get("fracht"), Some(&Value::Float(32.0)));
    }
}
