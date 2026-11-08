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
//! The levels of the methods come from the game data where the reading of the values kept them ([`Methods`],
//! [`GAME_FILE`], read from the line the terminal shows below each method); otherwise from the table written down
//! by hand ([`METHODS`]), where for Pyrometric Chromalysis and Dinyx Solventation they are the ones the costs and
//! times of a run show. A bigger level is better for the player on every axis, also for the costs (3 is "low
//! costs").

use crate::jsonio::Value;
use crate::mining_data::MiningData;
use crate::stock_data::lower;

/// The game version the levels of the table were read in, and when; the page names them while it shows the table.
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

/// The name of a method in the game.
pub fn name(key: &str) -> &'static str {
    METHODS.iter().find(|(k, _, _)| *k == key).map_or("", |(_, name, _)| name)
}

/// The file the levels read from the game data are kept in, beside the cache of the values.
pub const GAME_FILE: &str = "raffinerie-verfahren-spieldaten.json";
/// The version of the layout of [`GAME_FILE`].
pub const GAME_FORMAT: i64 = 1;

/// Where the levels of the methods come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The table written down by hand ([`METHODS`], [`PATCH`], [`READ_ON`]).
    Table,
    /// The game data of `build`, read in the channel `channel` (`LIVE`, `PTU` …).
    Game { build: String, channel: String },
}

/// The nine methods with their levels (yield, speed, cost advantage, as in [`METHODS`]): read from the game data
/// where that worked, else the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Methods {
    levels: [(&'static str, [u8; 3]); 9],
    pub origin: Origin,
}

impl Methods {
    /// The table written down by hand.
    pub fn table() -> Methods {
        Methods { levels: METHODS.map(|(key, _, levels)| (key, levels)), origin: Origin::Table }
    }

    /// The levels of [`GAME_FILE`] as written by the reading of the game data. Every one of the nine methods of the
    /// table has to be there once, by its name in the game, with yield and costs 1 to 3 and speed 0 to 3.
    ///
    /// # Errors
    ///
    /// What does not fit, in words for the error log.
    pub fn from_game_text(text: &str) -> Result<Methods, String> {
        let read = crate::jsonio::loads(text).map_err(|error| format!("no JSON: {error}"))?;
        if read.get("format") != Some(&Value::int(GAME_FORMAT)) {
            return Err("another layout".into());
        }
        let word = |key: &str| read.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        let (build, channel) = (word("build"), word("kanal"));
        let Some(Value::Array(entries)) = read.get("verfahren") else { return Err("no methods".into()) };
        let mut levels = METHODS.map(|(key, _, _)| (key, None::<[u8; 3]>));
        for entry in entries {
            let name = entry.get("name").and_then(Value::as_str).unwrap_or_default();
            let Some(at) = METHODS.iter().position(|(_, known, _)| known.eq_ignore_ascii_case(name.trim())) else {
                return Err(format!("unknown method {name:?}"));
            };
            let number = |axis: &str, lowest: u8| match entry.get(axis) {
                Some(Value::Int(digits)) => digits.parse::<u8>().ok().filter(|n| (lowest..=3).contains(n)),
                _ => None,
            };
            let (Some(gain), Some(speed), Some(cost)) = (number("ertrag", 1), number("tempo", 0), number("kosten", 1))
            else {
                return Err(format!("levels of {name:?} out of range"));
            };
            if levels[at].1.replace([gain, speed, cost]).is_some() {
                return Err(format!("{name:?} twice"));
            }
        }
        let mut complete = [("", [0; 3]); 9];
        for (slot, (key, found)) in complete.iter_mut().zip(levels) {
            *slot = (key, found.ok_or_else(|| format!("{key} missing"))?);
        }
        Ok(Methods { levels: complete, origin: Origin::Game { build, channel } })
    }

    /// The levels of the game data kept in `file`; `None` when there is no such file, or it does not fit (then the
    /// error log says why).
    pub fn from_game_file(file: &str) -> Option<Methods> {
        let text = std::fs::read_to_string(file).ok()?;
        match Methods::from_game_text(&text) {
            Ok(methods) => Some(methods),
            Err(error) => {
                crate::errors::record("refining.game_file", &error);
                None
            }
        }
    }

    /// The level of a method on an axis; 0 for an unknown method or axis.
    pub fn level(&self, key: &str, axis: &str) -> u8 {
        match (self.levels.iter().find(|(k, _)| *k == key), position(axis)) {
            (Some((_, levels)), Some(at)) => levels[at],
            _ => 0,
        }
    }

    /// The methods whose levels differ from the table, in the order of the table.
    pub fn differing_from_table(&self) -> Vec<&'static str> {
        let table = Methods::table();
        self.levels
            .iter()
            .filter(|(key, levels)| table.levels.iter().any(|(k, l)| k == key && l != levels))
            .map(|(key, _)| *key)
            .collect()
    }

    /// The method that suits the axes the player chose, and all nine in that order. What was not chosen follows
    /// in the order yield, costs, speed, so the order is always the same; a second axis equal to the first adds
    /// nothing.
    pub fn recommend(&self, first: Option<&str>, second: Option<&str>) -> (&'static str, Vec<&'static str>) {
        let mut order: Vec<&str> = Vec::new();
        for axis in [first, second]
            .into_iter()
            .flatten()
            .filter(|a| position(a).is_some())
            .chain(["ertrag", "kosten", "tempo"])
        {
            if !order.contains(&axis) {
                order.push(axis);
            }
        }
        let mut all: Vec<&'static str> = self.levels.iter().map(|(k, _)| *k).collect();
        all.sort_by_key(|key| order.iter().map(|axis| std::cmp::Reverse(self.level(key, axis))).collect::<Vec<_>>());
        (all[0], all)
    }

    /// The methods another one beats in every respect: no axis worse, one better. Per method the first such
    /// other one. Computed, not entered: if a level changes, the advice stays true.
    pub fn dominated(&self) -> Vec<(&'static str, &'static str)> {
        let mut result = Vec::new();
        for (a, _) in self.levels {
            for (b, _) in self.levels {
                if a == b {
                    continue;
                }
                let better = AXES.iter().any(|axis| self.level(b, axis) > self.level(a, axis));
                let worse = AXES.iter().any(|axis| self.level(b, axis) < self.level(a, axis));
                if better && !worse {
                    result.push((a, b));
                    break;
                }
            }
        }
        result
    }
}

/// The level of a method of the table on an axis; 0 for an unknown method or axis.
pub fn level(key: &str, axis: &str) -> u8 {
    Methods::table().level(key, axis)
}

/// [`Methods::recommend`] for the table.
pub fn recommend(first: Option<&str>, second: Option<&str>) -> (&'static str, Vec<&'static str>) {
    Methods::table().recommend(first, second)
}

/// [`Methods::dominated`] for the table.
pub fn dominated() -> Vec<(&'static str, &'static str)> {
    Methods::table().dominated()
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

    /// The file the reading of the game data writes, for the methods of the table with `levels` per name.
    fn game_text(levels: &[(&str, [u8; 3])]) -> String {
        let entries: Vec<String> = levels
            .iter()
            .map(|(name, [gain, speed, cost])| {
                format!(r#"{{"name": "{name}", "ertrag": {gain}, "tempo": {speed}, "kosten": {cost}}}"#)
            })
            .collect();
        format!(r#"{{"format": 1, "build": "4.10.2-LIVE.1", "kanal": "PTU", "verfahren": [{}]}}"#, entries.join(", "))
    }

    fn table_levels() -> Vec<(&'static str, [u8; 3])> {
        METHODS.iter().map(|(_, name, levels)| (*name, *levels)).collect()
    }

    #[test]
    fn the_levels_read_from_the_game_take_the_place_of_the_table() {
        let mut read = table_levels();
        // As in the game data of 4.10.2: the levels of Ferron Exchange and Kazen Winnowing exchanged.
        let (ferron, kazen) = (read[3].1, read[4].1);
        (read[3].1, read[4].1) = (kazen, ferron);
        let methods = Methods::from_game_text(&game_text(&read)).expect("all nine");
        assert_eq!(methods.origin, Origin::Game { build: "4.10.2-LIVE.1".into(), channel: "PTU".into() });
        assert_eq!(methods.level("ferron", "ertrag"), kazen[0]);
        assert_eq!(methods.differing_from_table(), ["ferron", "kazen"]);
        assert_eq!(Methods::table().differing_from_table(), Vec::<&str>::new());
        // The order follows the levels read: of the moderate yields Ferron came first by its low costs, now Kazen.
        let place = |order: &[&str], key: &str| order.iter().position(|k| *k == key).expect(key);
        let (_, table_order) = recommend(None, None);
        let (_, read_order) = methods.recommend(None, None);
        assert!(place(&table_order, "ferron") < place(&table_order, "kazen"), "{table_order:?}");
        assert!(place(&read_order, "kazen") < place(&read_order, "ferron"), "{read_order:?}");
        // Names are found whatever their case and spaces.
        let loose: Vec<(String, [u8; 3])> =
            table_levels().into_iter().map(|(name, levels)| (format!(" {} ", name.to_uppercase()), levels)).collect();
        let loose: Vec<(&str, [u8; 3])> = loose.iter().map(|(name, levels)| (name.as_str(), *levels)).collect();
        assert!(Methods::from_game_text(&game_text(&loose)).is_ok());
    }

    #[test]
    fn a_file_of_the_game_that_does_not_fit_leaves_the_table() {
        let all = table_levels();
        assert!(Methods::from_game_text(&game_text(&all[..8])).unwrap_err().contains("missing"));
        let mut twice = all.clone();
        twice[8].0 = twice[0].0;
        assert!(Methods::from_game_text(&game_text(&twice)).unwrap_err().contains("twice"));
        let mut unknown = all.clone();
        unknown[0].0 = "Quantum Folding";
        assert!(Methods::from_game_text(&game_text(&unknown)).unwrap_err().contains("unknown"));
        for wrong in [[0, 3, 1], [3, 4, 1], [3, 3, 0], [3, 3, 4]] {
            let mut levels = all.clone();
            levels[0].1 = wrong;
            assert!(Methods::from_game_text(&game_text(&levels)).is_err(), "{wrong:?}");
        }
        let mut speed_zero = all.clone();
        speed_zero[0].1 = [3, 0, 1];
        assert!(Methods::from_game_text(&game_text(&speed_zero)).is_ok(), "the slowest speed is 0");
        let other_layout = game_text(&all).replace(r#""format": 1"#, r#""format": 2"#);
        assert!(Methods::from_game_text(&other_layout).is_err());
        assert!(Methods::from_game_text("{kaputt").is_err());
        let dir = crate::testdir::Dir::new("raffinerie-spieldaten");
        let file = dir.path().join(GAME_FILE).to_string_lossy().into_owned();
        assert_eq!(Methods::from_game_file(&file), None, "no file");
        std::fs::write(&file, "{kaputt").unwrap();
        assert_eq!(Methods::from_game_file(&file), None, "a broken file");
        std::fs::write(&file, game_text(&all)).unwrap();
        assert!(Methods::from_game_file(&file).is_some_and(|m| m.differing_from_table().is_empty()));
    }
}
