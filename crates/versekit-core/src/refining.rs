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

//! The refineries: which of the nine processing methods suits what the player wants (`refinery.py` of 4.12),
//! and what every refinery makes of an ore (`mining.refinery_matrix`, `refineries_for` and the short names
//! of the columns of `pages.py`).
//!
//! The terminal shows one line per method and no numbers, so the levels are written down by hand. For
//! Pyrometric Chromalysis and Dinyx Solventation they are the ones the costs and times of a run show, not the
//! ones the text suggests. A bigger level is better for the player on every axis, also for the costs (3 is
//! "low costs").

use crate::jsonio::Value;
use crate::mining_data::MiningData;
use crate::stock_data::lower;

/// The game version the levels were read in, and when.
pub const PATCH: &str = "4.10";
pub const READ_ON: &str = "2026-09-03";

/// The three axes, in the order of the choice on the page.
pub const AXES: [&str; 3] = ["ertrag", "kosten", "tempo"];

/// Per method: the key, the name in the game (English in both languages: the terminal shows it so), and the
/// levels of yield, speed and cost advantage.
pub const METHODS: [(&str, &str, [u8; 3]); 9] = [
    ("pyrometric", "Pyrometric Chromalysis", [3, 3, 1]),
    ("gaskin", "Gaskin Process", [3, 1, 2]),
    ("dinyx", "Dinyx Solventation", [3, 0, 3]),
    ("ferron", "Ferron Exchange", [2, 1, 3]),
    ("kazen", "Kazen Winnowing", [2, 2, 2]),
    ("thermonatic", "Thermonatic Deposition", [2, 3, 1]),
    ("electro", "Electrostarolysis", [1, 2, 3]),
    ("cormack", "Cormack Method", [1, 3, 2]),
    ("xcr", "XCR Reaction", [1, 3, 1]),
];

/// Where an axis stands in the levels.
fn position(axis: &str) -> Option<usize> {
    match axis {
        "ertrag" => Some(0),
        "tempo" => Some(1),
        "kosten" => Some(2),
        _ => None,
    }
}

/// The level of a method on an axis; 0 for an unknown method or axis.
pub fn level(key: &str, axis: &str) -> u8 {
    match (METHODS.iter().find(|(k, _, _)| *k == key), position(axis)) {
        (Some((_, _, levels)), Some(at)) => levels[at],
        _ => 0,
    }
}

/// The name of a method in the game.
pub fn name(key: &str) -> &'static str {
    METHODS.iter().find(|(k, _, _)| *k == key).map_or("", |(_, name, _)| name)
}

/// The method that suits the axes the player chose, and all nine in that order. What was not chosen follows
/// in the order yield, costs, speed, so the order is always the same; a second axis equal to the first adds
/// nothing.
pub fn recommend(first: Option<&str>, second: Option<&str>) -> (&'static str, Vec<&'static str>) {
    let mut order: Vec<&str> = Vec::new();
    for axis in
        [first, second].into_iter().flatten().filter(|a| position(a).is_some()).chain(["ertrag", "kosten", "tempo"])
    {
        if !order.contains(&axis) {
            order.push(axis);
        }
    }
    let mut all: Vec<&'static str> = METHODS.iter().map(|(k, _, _)| *k).collect();
    all.sort_by_key(|key| order.iter().map(|axis| std::cmp::Reverse(level(key, axis))).collect::<Vec<_>>());
    (all[0], all)
}

/// The methods another one beats in every respect: no axis worse, one better. Per method the first such
/// other one. Computed, not entered: if a level changes, the advice stays true.
pub fn dominated() -> Vec<(&'static str, &'static str)> {
    let mut result = Vec::new();
    for (a, _, _) in METHODS {
        for (b, _, _) in METHODS {
            if a == b {
                continue;
            }
            let better = AXES.iter().any(|axis| level(b, axis) > level(a, axis));
            let worse = AXES.iter().any(|axis| level(b, axis) < level(a, axis));
            if better && !worse {
                result.push((a, b));
                break;
            }
        }
    }
    result
}

// ------------------------------------------------------------ the refineries

/// One column of the table: the stations with the same profile in one system.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// The names of the stations, sorted.
    pub names: Vec<String>,
    pub system: String,
}

/// One row of the table: a material, its bonus per column and the column of the best value, where the values
/// are not all the same.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub material: String,
    pub values: Vec<i64>,
    pub best: Option<usize>,
}

/// `int()` of Python on a value of the data; 0 where it raises.
fn whole(value: &Value) -> i64 {
    match value {
        Value::Int(n) => n.parse().unwrap_or(0),
        Value::Float(x) if x.is_finite() => x.trunc() as i64,
        Value::Bool(b) => i64::from(*b),
        Value::Str(s) => crate::text::py_strip(s).parse().unwrap_or(0),
        _ => 0,
    }
}

/// A bonus as a number, for comparing and showing.
fn bonus_of(value: &Value) -> f64 {
    match value {
        Value::Int(n) => n.parse().unwrap_or(0.0),
        Value::Float(x) => *x,
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// The stations bundled per (system, profile), in the order they first appear; only stations whose profile
/// stands in the data. A bundle never reaches over a system: "Nyx, Pyro, Stanton" answers no question.
fn bundles(mining: &MiningData) -> Vec<((String, String), Vec<String>)> {
    let mut bundled: Vec<((String, String), Vec<String>)> = Vec::new();
    for refinery in mining.refineries() {
        if !mining.profiles().iter().any(|(id, _)| *id == refinery.profile) {
            continue;
        }
        let key = (refinery.system.clone(), refinery.profile.clone());
        match bundled.iter_mut().find(|(k, _)| *k == key) {
            Some((_, names)) => names.push(refinery.name.clone()),
            None => bundled.push((key, vec![refinery.name.clone()])),
        }
    }
    bundled
}

/// Every refinery side by side (`mining.refinery_matrix`): the columns by system, the biggest bundles first,
/// and one row per material with the bonus per column. What a profile does not name is 0 %.
pub fn matrix(mining: &MiningData) -> (Vec<Column>, Vec<Row>) {
    if mining.profiles().is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut order = bundles(mining);
    if order.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let sorted_names = |names: &[String]| {
        let mut names = names.to_vec();
        names.sort();
        names
    };
    order.sort_by_cached_key(|((system, _), names)| {
        (system.clone(), std::cmp::Reverse(names.len()), sorted_names(names))
    });
    let columns = order
        .iter()
        .map(|((system, _), names)| Column { names: sorted_names(names), system: system.clone() })
        .collect();
    let mut materials: Vec<String> = Vec::new();
    for (_, values) in mining.profiles() {
        for (material, _) in values {
            if !materials.contains(material) {
                materials.push(material.clone());
            }
        }
    }
    // Ores and raw materials no profile names still get a row (all 0 %), so a new ore is not missing.
    for element in mining.elements() {
        let refinable = element.name.ends_with(" (Ore)") || element.name.ends_with(" (Raw)");
        if refinable && !materials.contains(&element.name) {
            materials.push(element.name.clone());
        }
    }
    materials.sort_by_cached_key(|m| lower(m));
    let rows = materials
        .into_iter()
        .map(|material| {
            let values: Vec<i64> = order
                .iter()
                .map(|((_, profile), _)| {
                    mining
                        .profiles()
                        .iter()
                        .find(|(id, _)| id == profile)
                        .and_then(|(_, values)| values.iter().find(|(m, _)| *m == material))
                        .map_or(0, |(_, v)| whole(v))
                })
                .collect();
            let best = match values.iter().max() {
                Some(top) if values.iter().any(|v| v != top) => values.iter().position(|v| v == top),
                _ => None,
            };
            Row { material, values, best }
        })
        .collect();
    (columns, rows)
}

/// Which refinery makes the most of this ore (`mining.refineries_for`): per bundle of stations the bonus,
/// the best first. The materials are compared by their key, since the profiles, the recipes and the mining
/// data write them differently.
pub fn for_ore(mining: &MiningData, material: &str) -> Vec<(Vec<String>, String, f64)> {
    if mining.profiles().is_empty() {
        return Vec::new();
    }
    let wanted = mining.material_key(material);
    let bonus = |profile: &str| {
        mining
            .profiles()
            .iter()
            .find(|(id, _)| id == profile)
            .and_then(|(_, values)| values.iter().find(|(m, _)| mining.material_key(m) == wanted))
            .map_or(0.0, |(_, v)| bonus_of(v))
    };
    let mut result: Vec<(Vec<String>, String, f64)> = bundles(mining)
        .into_iter()
        .map(|((system, profile), mut names)| {
            names.sort();
            (names, system, bonus(&profile))
        })
        .collect();
    result.sort_by(|a, b| {
        b.2.total_cmp(&a.2)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.first().map_or("", String::as_str).cmp(b.0.first().map_or("", String::as_str)))
    });
    result
}

/// The short name of a column (`pages._refinery_short`): the first word of the first station; a gateway gets
/// "-Gate", since "Pyro Gateway" stands in Nyx and "Pyro" alone would say the wrong system.
pub fn short_name(names: &[String]) -> String {
    let mut shorts: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| !n.is_empty()) {
        let mut word = name.split(' ').next().unwrap_or_default().to_string();
        if name.contains("Gateway") {
            word.push_str("-Gate");
        }
        if !shorts.contains(&word) {
            shorts.push(word);
        }
    }
    shorts.into_iter().next().unwrap_or_else(|| "—".to_string())
}

/// The short name of the first station in the box of an ore (`pages.py`, the box below an ore): the first
/// word, without "-Gate", and how many other short names there are.
pub fn box_name(names: &[String]) -> (String, usize) {
    let mut shorts: Vec<&str> = Vec::new();
    for name in names {
        let word = name.split(' ').next().unwrap_or_default();
        if !shorts.contains(&word) {
            shorts.push(word);
        }
    }
    match shorts.first() {
        Some(first) => (first.to_string(), shorts.len() - 1),
        None => (String::new(), 0),
    }
}

/// The widest short name that stands on one line above a column.
pub const HEAD_WIDTH: usize = 6;

/// A short name for the head of a column on two lines (`pages._refinery_head`): up to six letters on one; a
/// longer one is broken after a hyphen or space near the middle, otherwise in the middle.
pub fn head(short: &str) -> String {
    let chars: Vec<char> = short.chars().collect();
    let length = chars.len();
    if length <= HEAD_WIDTH {
        return short.to_string();
    }
    let middle = length.div_ceil(2);
    let mut cut = None;
    'search: for offset in 0..HEAD_WIDTH {
        for at in [middle + offset, middle.wrapping_sub(offset)] {
            if (1..length).contains(&at) && matches!(chars[at - 1], '-' | ' ') {
                cut = Some(at);
                break 'search;
            }
        }
    }
    let at = cut.unwrap_or(middle);
    let (mut top, mut bottom): (String, String) = (chars[..at].iter().collect(), chars[at..].iter().collect());
    top = top.trim_end().to_string();
    if top.chars().count() > HEAD_WIDTH || bottom.chars().count() > HEAD_WIDTH {
        top = chars[..middle].iter().collect();
        bottom = chars[middle..].iter().collect();
    }
    format!("{top}\n{bottom}")
}

/// The columns grouped by system, without sorting anew: per run of columns of one system its name and how
/// many columns it spans.
pub fn groups(columns: &[Column]) -> Vec<(String, usize)> {
    let mut result: Vec<(String, usize)> = Vec::new();
    for column in columns {
        match result.last_mut() {
            Some((system, count)) if *system == column.system => *count += 1,
            _ => result.push((column.system.clone(), 1)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio;

    #[test]
    fn without_a_wish_the_method_with_the_highest_yield_and_lowest_costs_comes_first() {
        let (best, all) = recommend(None, None);
        assert_eq!(best, "dinyx");
        assert_eq!(
            all,
            ["dinyx", "gaskin", "pyrometric", "ferron", "kazen", "thermonatic", "electro", "cormack", "xcr"]
        );
        assert_eq!(recommend(Some("tempo"), None).0, "pyrometric");
        assert_eq!(recommend(Some("tempo"), Some("kosten")).0, "cormack");
        assert_eq!(recommend(Some("kosten"), Some("tempo")).0, "electro");
        // A second axis equal to the first adds nothing; "then" alone leads.
        assert_eq!(recommend(Some("tempo"), Some("tempo")), recommend(Some("tempo"), None));
        assert_eq!(recommend(None, Some("tempo")), recommend(Some("tempo"), None));
        assert_eq!(recommend(Some("unknown"), None), recommend(None, None));
    }

    #[test]
    fn the_costs_come_first_never_pyrometric_and_the_measured_order_of_the_two_stands() {
        // Dinyx is the cheap and slow method, Pyrometric the dear and fast one.
        assert_ne!(recommend(Some("kosten"), None).0, "pyrometric");
        assert_ne!(recommend(Some("kosten"), Some("tempo")).0, "pyrometric");
        assert_eq!(recommend(Some("kosten"), None).0, "dinyx");
        assert!(level("dinyx", "kosten") > level("pyrometric", "kosten"));
        assert!(level("pyrometric", "tempo") > level("dinyx", "tempo"));
        assert_eq!(level("pyrometric", "ertrag"), level("dinyx", "ertrag"));
    }

    #[test]
    fn a_method_beaten_on_every_axis_is_named_with_the_one_that_beats_it() {
        assert_eq!(dominated(), [("thermonatic", "pyrometric"), ("xcr", "pyrometric")]);
        // Yield and costs make nine different cells: every pair names one method.
        let mut cells: Vec<(u8, u8)> =
            METHODS.iter().map(|(k, _, _)| (level(k, "ertrag"), level(k, "kosten"))).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 9);
        assert_eq!(level("nothing", "ertrag"), 0);
        assert_eq!(name("xcr"), "XCR Reaction");
    }

    fn data() -> MiningData {
        let text = r#"{"format": 1, "locations": [],
          "refineries": [
            {"name": "ARC-L1 Wide Forest Station", "system": "Stanton", "profileId": "a"},
            {"name": "CRU-L1 Ambitious Dream Station", "system": "Stanton", "profileId": "a"},
            {"name": "Pyro Gateway", "system": "Nyx", "profileId": "a"},
            {"name": "Levski", "system": "Nyx", "profileId": "b"},
            {"name": "Nowhere", "system": "Stanton", "profileId": "x"},
            {"name": "Checkmate", "system": "Pyro", "profileId": "b"}],
          "refineryProfiles": {
            "a": {"Aluminum (Ore)": 3, "Gold": -2.7},
            "b": {"Gold": 5, "Aluminum (Ore)": 3}},
          "elemente": {
            "r": {"name": "Riccite (Ore)"},
            "h": {"name": "Hadanite"},
            "a": {"name": "Aluminum (Ore)"}}}"#;
        MiningData::of(&jsonio::loads(text).unwrap())
    }

    #[test]
    fn an_ore_no_refinery_names_still_gets_a_row_of_zeros() {
        let (_, rows) = matrix(&data());
        let riccite = rows.iter().find(|r| r.material == "Riccite (Ore)").expect("Riccite has a row");
        assert_eq!(riccite.values, [0, 0, 0, 0]);
        assert_eq!(riccite.best, None);
        assert!(!rows.iter().any(|r| r.material == "Hadanite"), "a gem is not refined");
        assert_eq!(rows.iter().filter(|r| r.material.starts_with("Alu")).count(), 1, "no second row");
    }

    #[test]
    fn the_table_bundles_per_system_and_marks_only_a_real_best() {
        let (columns, rows) = matrix(&data());
        let shown: Vec<(&str, usize)> = columns.iter().map(|c| (c.system.as_str(), c.names.len())).collect();
        assert_eq!(shown, [("Nyx", 1), ("Nyx", 1), ("Pyro", 1), ("Stanton", 2)]);
        assert_eq!(columns[0].names, ["Levski"], "same size: by the names");
        let gold = rows.iter().find(|r| r.material == "Gold").unwrap();
        assert_eq!(gold.values, [5, -2, 5, -2], "int() cuts toward zero");
        assert_eq!(gold.best, Some(0));
        let aluminum = rows.iter().find(|r| r.material.starts_with("Alu")).unwrap();
        assert_eq!(aluminum.best, None, "all the same: nothing to mark");
        assert_eq!(groups(&columns), [("Nyx".to_string(), 2), ("Pyro".to_string(), 1), ("Stanton".to_string(), 1)]);
    }

    #[test]
    fn the_refineries_for_an_ore_find_it_by_its_key_the_best_first() {
        let found = for_ore(&data(), "Aluminium");
        assert!(found.iter().all(|(_, _, bonus)| *bonus == 3.0), "{found:?}");
        let gold = for_ore(&data(), "Gold");
        assert_eq!(gold[0].2, 5.0);
        assert_eq!(gold.last().unwrap().2, -2.7);
        assert_eq!((gold[0].1.as_str(), gold[0].0[0].as_str()), ("Nyx", "Levski"));
        assert!(for_ore(&data(), "Quantainium").iter().all(|(_, _, bonus)| *bonus == 0.0));
    }

    #[test]
    fn the_short_names_of_the_columns_and_their_heads() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(short_name(&names(&["Pyro Gateway (Nyx)"])), "Pyro-Gate");
        assert_eq!(short_name(&names(&["ARC-L1 Wide", "CRU-L1 Dream"])), "ARC-L1");
        assert_eq!(short_name(&[]), "—");
        assert_eq!(box_name(&names(&["Pyro Gateway", "Pyro Gateway (Nyx)", "Levski"])), ("Pyro".to_string(), 1));
        assert_eq!(head("ARC-L1"), "ARC-L1");
        assert_eq!(head("Checkmate"), "Check\nmate");
        assert_eq!(head("Pyro-Gate"), "Pyro-\nGate");
        assert_eq!(head("Orbituary"), "Orbit\nuary");
    }
}
