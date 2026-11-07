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

//! Cross-check of the mining data (`mining-data.json`, kept from scmdb) against the mining kinds of
//! the Basetool catalog that p4kx writes from the same build: the numbers the share of an ore is made
//! of (compositions, elements, the groups and deposits of a provider preset), and the processing
//! methods of the refineries.
//!
//! `versekit-parity abbau_katalog <catalog.json> <mining-data.json> [--gegenprobe]`. The catalog is
//! the file `p4kx catalog` writes (`mineableCompositions`, `mineableElements`,
//! `harvestableProviderPresets`, `refiningProcesses`). Names, systems, kinds of place, scan
//! signatures and the refinery bonuses are not in the catalog and not compared. With `--gegenprobe`
//! one part of a composition, one number of an element, one deposit and one group of the file are
//! changed, and the check must report each of the four.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;
use versekit_core::refining;

/// The numbers of an element: the name in the file and in the catalog.
const ELEMENT_FIELDS: [(&str, &str); 7] = [
    ("instability", "instability"),
    ("resistance", "resistance"),
    ("optimalWindowMidpoint", "optimalWindowMidpoint"),
    ("optimalWindowRandomness", "optimalWindowMidpointRandomness"),
    ("optimalWindowThinness", "optimalWindowThinness"),
    ("explosionMultiplier", "explosionMultiplier"),
    ("clusterFactor", "clusterFactor"),
];

/// How many of a kind agree and how many do not, with the first few cases.
#[derive(Debug, Default)]
struct Count {
    equal: usize,
    different: usize,
    examples: Vec<String>,
}

impl Count {
    fn add(&mut self, same: bool, example: impl FnOnce() -> String) {
        if same {
            self.equal += 1;
        } else {
            self.different += 1;
            if self.examples.len() < 3 {
                self.examples.push(example());
            }
        }
    }
}

/// Everything the check found.
#[derive(Debug, Default)]
pub struct Report {
    compositions: Count,
    composition_missing: Vec<String>,
    elements: Count,
    element_missing: Vec<String>,
    deposits: Count,
    groups: Count,
    preset_missing: Vec<String>,
    only_catalog: Vec<String>,
    methods_match: Vec<String>,
    methods_differ: Vec<String>,
}

impl Report {
    /// The cases in which file and catalog disagree. The processing methods are not counted: how they
    /// differ is told, not failed.
    pub fn differences(&self) -> usize {
        self.compositions.different
            + self.composition_missing.len()
            + self.elements.different
            + self.element_missing.len()
            + self.deposits.different
            + self.groups.different
    }

    fn text(&self) -> String {
        let mut out = String::new();
        let mut line = |name: &str, count: &Count| {
            let _ = writeln!(out, "{name}: {} gleich, {} verschieden", count.equal, count.different);
            for example in &count.examples {
                let _ = writeln!(out, "    {example}");
            }
        };
        line("Zusammensetzungen", &self.compositions);
        line("Elemente (sieben Zahlen)", &self.elements);
        line("Vorkommen je Gruppe (Gewicht und Zusammensetzung)", &self.deposits);
        line("Gruppenwahrscheinlichkeit", &self.groups);
        let _ = writeln!(out, "Zusammensetzung nicht im Katalog: {:?}", self.composition_missing);
        let _ = writeln!(out, "Element nicht im Katalog: {:?}", self.element_missing);
        let _ = writeln!(out, "Ort ohne Voreinstellung im Katalog: {:?}", self.preset_missing);
        let _ = writeln!(out, "Voreinstellung nur im Katalog: {:?}", self.only_catalog);
        let _ = writeln!(
            out,
            "Verfahren: {} stimmen mit der Tabelle der Raffinerie überein, {} nicht (nur Hinweis)",
            self.methods_match.len(),
            self.methods_differ.len()
        );
        for method in &self.methods_differ {
            let _ = writeln!(out, "    {method}");
        }
        let _ = writeln!(out, "Abweichungen gesamt: {}", self.differences());
        out
    }
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn by_guid(list: &[Value]) -> BTreeMap<&str, &Value> {
    list.iter().filter_map(|entry| Some((entry.get("guid")?.as_str()?, entry))).collect()
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_f64).map(f64::to_bits)
}

/// One part of a composition as both sides state it, with the numbers as bits (the check is exact).
type Part = (String, Option<u64>, Option<u64>, Option<u64>, Option<u64>, Option<u64>);

fn part_of_file(part: &Value) -> Part {
    (
        part["elementGuid"].as_str().unwrap_or_default().to_string(),
        number(part, "probability"),
        number(part, "minPercent"),
        number(part, "maxPercent"),
        number(part, "curveExponent"),
        number(part, "qualityScale"),
    )
}

fn part_of_catalog(part: &Value) -> Part {
    (
        part["mineableElementGuid"].as_str().unwrap_or_default().to_string(),
        number(part, "probability"),
        number(part, "minPercentage"),
        number(part, "maxPercentage"),
        number(part, "curveExponent"),
        number(part, "qualityScale"),
    )
}

/// The rock deposits of a group: the weight and the composition, sorted. A deposit without a
/// composition is no rock and does not count.
fn rock_deposits<'a>(deposits: impl Iterator<Item = (Option<u64>, Option<&'a str>)>) -> Vec<(u64, String)> {
    let mut rocks: Vec<(u64, String)> =
        deposits.filter_map(|(weight, composition)| Some((weight?, composition?.to_string()))).collect();
    rocks.sort();
    rocks
}

/// Compares the file with the catalog.
pub fn compare(catalog: &Value, mining: &Value) -> Report {
    let mut report = Report::default();
    let compositions = by_guid(items(catalog, "mineableCompositions"));
    if let Some(file) = mining.get("compositions").and_then(Value::as_object) {
        for (guid, entry) in file {
            let Some(own) = compositions.get(guid.as_str()) else {
                report.composition_missing.push(guid.clone());
                continue;
            };
            let theirs: Vec<Part> = items(entry, "parts").iter().map(part_of_file).collect();
            let ours: Vec<Part> = items(own, "parts").iter().map(part_of_catalog).collect();
            let same = theirs == ours && entry["minimumDistinctElements"] == own["minimumDistinctElements"];
            report.compositions.add(same, || format!("{guid}: {} gegen {} Teile", theirs.len(), ours.len()));
        }
    }
    let elements = by_guid(items(catalog, "mineableElements"));
    if let Some(file) = mining.get("elemente").and_then(Value::as_object) {
        for (guid, entry) in file {
            let Some(own) = elements.get(guid.as_str()) else {
                report.element_missing.push(guid.clone());
                continue;
            };
            let mut wrong: Vec<&str> = ELEMENT_FIELDS
                .iter()
                .filter(|(file, cat)| number(entry, file) != number(own, cat))
                .map(|(f, _)| *f)
                .collect();
            if entry["resourceType"] != own["resourceTypeGuid"] {
                wrong.push("resourceType");
            }
            report.elements.add(wrong.is_empty(), || format!("{guid}: {wrong:?}"));
        }
    }
    let presets = by_guid(items(catalog, "harvestableProviderPresets"));
    let mut seen: Vec<&str> = Vec::new();
    for location in items(mining, "locations") {
        let Some(guid) = location["presetGuid"].as_str() else { continue };
        let Some(preset) = presets.get(guid) else {
            report.preset_missing.push(location["presetFile"].as_str().unwrap_or(guid).to_string());
            continue;
        };
        seen.push(guid);
        let name = location["presetFile"].as_str().unwrap_or(guid);
        for group in items(location, "groups") {
            let group_name = group["groupName"].as_str().unwrap_or_default();
            let theirs = rock_deposits(
                items(group, "deposits")
                    .iter()
                    .map(|d| (number(d, "relativeProbability"), d.get("compositionGuid").and_then(Value::as_str))),
            );
            if theirs.is_empty() {
                continue;
            }
            let own = items(preset, "groups").iter().find(|g| g["groupName"].as_str() == Some(group_name));
            let ours = own.map(|g| {
                rock_deposits(items(g, "harvestables").iter().map(|h| {
                    (
                        number(h, "relativeProbability"),
                        h.get("harvestable").and_then(|v| v.get("mineableCompositionGuid")).and_then(Value::as_str),
                    )
                }))
            });
            report.deposits.add(ours.as_ref() == Some(&theirs), || {
                format!("{name} / {group_name}: {} gegen {:?} Vorkommen", theirs.len(), ours.as_ref().map(Vec::len))
            });
            report
                .groups
                .add(own.is_some_and(|g| number(g, "groupProbability") == number(group, "groupProbability")), || {
                    format!("{name} / {group_name}")
                });
        }
    }
    report.only_catalog = presets
        .iter()
        .filter(|(guid, _)| !seen.contains(guid))
        .map(|(_, preset)| preset["className"].as_str().unwrap_or_default().to_string())
        .collect();
    methods(catalog, &mut report);
    report
}

/// Whether the level of a method in the table of the refinery follows the speed and quality the
/// catalog states: the quality is the level of the yield (1 wasteful to 3 careful) and the speed the
/// level of the speed (1 slow to 3 fast, the slowest may be 0 in the table).
fn methods(catalog: &Value, report: &mut Report) {
    for process in items(catalog, "refiningProcesses") {
        let name = process["processName"]["en"].as_str().unwrap_or_default();
        let Some((key, _, levels)) = refining::METHODS.iter().find(|(_, shown, _)| *shown == name) else {
            report.methods_differ.push(format!("{name}: nicht in der Tabelle"));
            continue;
        };
        let quality = match process["refiningQuality"].as_str() {
            Some("Wasteful") => 1,
            Some("Normal") => 2,
            Some("Careful") => 3,
            _ => 0,
        };
        let speed = match process["refiningSpeed"].as_str() {
            Some("Slow") => 1,
            Some("Normal") => 2,
            Some("Fast") => 3,
            _ => 0,
        };
        let yield_agrees = levels[0] == quality;
        let speed_agrees = levels[1] == speed || (speed == 1 && levels[1] == 0);
        if yield_agrees && speed_agrees {
            report.methods_match.push((*key).to_string());
        } else {
            report.methods_differ.push(format!(
                "{name}: Katalog {}/{} (Ertrag {quality}, Tempo {speed}), Tabelle Ertrag {}, Tempo {}",
                process["refiningSpeed"].as_str().unwrap_or("?"),
                process["refiningQuality"].as_str().unwrap_or("?"),
                levels[0],
                levels[1]
            ));
        }
    }
}

/// Changes one thing of each of the four kinds in the file; false where the file has nothing to change.
fn spoil(mining: &mut Value) -> Result<(), String> {
    let part = mining["compositions"]
        .as_object_mut()
        .and_then(|c| c.values_mut().find_map(|c| c["parts"].get_mut(0)))
        .ok_or("Gegenprobe: keine Zusammensetzung")?;
    part["maxPercent"] = Value::from(part["maxPercent"].as_f64().unwrap_or(0.0) + 1.0);
    let element =
        mining["elemente"].as_object_mut().and_then(|e| e.values_mut().next()).ok_or("Gegenprobe: kein Element")?;
    element["instability"] = Value::from(element["instability"].as_f64().unwrap_or(0.0) + 1.0);
    let locations = mining["locations"].as_array_mut().ok_or("Gegenprobe: keine Orte")?;
    let deposit = locations
        .iter_mut()
        .filter(|l| l["presetGuid"].is_string())
        .flat_map(|l| l["groups"].as_array_mut().into_iter().flatten())
        .flat_map(|g| g["deposits"].as_array_mut().into_iter().flatten())
        .find(|d| d["compositionGuid"].is_string())
        .ok_or("Gegenprobe: kein Vorkommen")?;
    deposit["relativeProbability"] = Value::from(deposit["relativeProbability"].as_f64().unwrap_or(0.0) + 1.0);
    let group = locations
        .iter_mut()
        .filter(|l| l["presetGuid"].is_string())
        .flat_map(|l| l["groups"].as_array_mut().into_iter().flatten())
        .find(|g| g["deposits"].as_array().is_some_and(|d| d.iter().any(|d| d["compositionGuid"].is_string())))
        .ok_or("Gegenprobe: keine Gruppe")?;
    group["groupProbability"] = Value::from(group["groupProbability"].as_f64().unwrap_or(0.0) + 0.5);
    Ok(())
}

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|e| format!("{path}: {e}"))
}

pub fn run(catalog_file: &str, mining_file: &str, gegenprobe: bool) -> Result<(), String> {
    let catalog = read(catalog_file)?;
    let mut mining = read(mining_file)?;
    let build = |v: &Value| v["build"].as_str().map(str::to_string);
    println!("Katalog: Stand {:?}, Datei: Stand {:?}", catalog["source"]["build"]["version"].as_str(), build(&mining));
    if gegenprobe {
        spoil(&mut mining)?;
    }
    let report = compare(&catalog, &mining);
    print!("{}", report.text());
    if gegenprobe {
        let missed: Vec<&str> = [
            ("Zusammensetzungen", report.compositions.different),
            ("Elemente", report.elements.different),
            ("Vorkommen", report.deposits.different),
            ("Gruppen", report.groups.different),
        ]
        .into_iter()
        .filter(|(_, different)| *different == 0)
        .map(|(name, _)| name)
        .collect();
        if !missed.is_empty() {
            return Err(format!("Gegenprobe hat nicht angeschlagen bei: {}", missed.join(", ")));
        }
        return Ok(());
    }
    if report.differences() > 0 {
        return Err(format!("{} Abweichungen", report.differences()));
    }
    println!("ERGEBNIS gleich");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalog() -> Value {
        json!({
            "mineableCompositions": [{"guid": "c1", "minimumDistinctElements": 1,
                "parts": [{"mineableElementGuid": "e1", "probability": 1.0, "minPercentage": 10.0,
                           "maxPercentage": 20.0, "curveExponent": 1.0, "qualityScale": 0.5}]}],
            "mineableElements": [{"guid": "e1", "resourceTypeGuid": "r1", "instability": 5.0, "resistance": 0.5,
                "optimalWindowMidpoint": 0.5, "optimalWindowMidpointRandomness": 0.1, "optimalWindowThinness": 1.0,
                "explosionMultiplier": 2.0, "clusterFactor": 0.3}],
            "harvestableProviderPresets": [{"guid": "p1", "className": "HPP_A", "groups": [
                {"groupName": "SpaceShip_Mineables", "groupProbability": 0.5, "harvestables": [
                    {"relativeProbability": 3.0, "harvestable": {"mineableCompositionGuid": "c1"}},
                    {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": null}}]}]},
                {"guid": "p2", "className": "HPP_B", "groups": []}],
            "refiningProcesses": [{"processName": {"en": "Dinyx Solventation"}, "refiningSpeed": "Slow",
                                   "refiningQuality": "Careful"},
                                  {"processName": {"en": "Gaskin Process"}, "refiningSpeed": "Fast",
                                   "refiningQuality": "Normal"}]
        })
    }

    fn mining() -> Value {
        json!({
            "compositions": {"c1": {"minimumDistinctElements": 1, "parts": [{"elementGuid": "e1", "probability": 1.0,
                "minPercent": 10.0, "maxPercent": 20.0, "curveExponent": 1.0, "qualityScale": 0.5}]}},
            "elemente": {"e1": {"resourceType": "r1", "instability": 5.0, "resistance": 0.5,
                "optimalWindowMidpoint": 0.5, "optimalWindowRandomness": 0.1, "optimalWindowThinness": 1.0,
                "explosionMultiplier": 2.0, "clusterFactor": 0.3}},
            "locations": [
                {"presetFile": "hpp_a", "presetGuid": "p1", "groups": [
                    {"groupName": "SpaceShip_Mineables", "groupProbability": 0.5, "deposits": [
                        {"relativeProbability": 3.0, "compositionGuid": "c1"}, {"relativeProbability": 1.0}]}]},
                {"presetFile": "_special", "presetGuid": null, "groups": []}
            ]
        })
    }

    #[test]
    fn equal_data_has_no_difference_and_the_unplaced_preset_is_told() {
        let report = compare(&catalog(), &mining());
        assert_eq!(report.differences(), 0, "{}", report.text());
        assert_eq!((report.compositions.equal, report.elements.equal, report.deposits.equal), (1, 1, 1));
        assert_eq!(report.only_catalog, ["HPP_B"]);
    }

    #[test]
    fn each_kind_of_difference_is_found() {
        let mut spoiled = mining();
        spoil(&mut spoiled).unwrap();
        let report = compare(&catalog(), &spoiled);
        assert_eq!(
            (
                report.compositions.different,
                report.elements.different,
                report.deposits.different,
                report.groups.different
            ),
            (1, 1, 1, 1),
            "{}",
            report.text()
        );
    }

    #[test]
    fn an_unknown_composition_or_element_is_a_difference() {
        let mut other = mining();
        other["compositions"]["c9"] = other["compositions"]["c1"].clone();
        other["elemente"]["e9"] = other["elemente"]["e1"].clone();
        let report = compare(&catalog(), &other);
        assert_eq!((report.composition_missing.len(), report.element_missing.len()), (1, 1));
        assert_eq!(report.differences(), 2);
    }

    #[test]
    fn the_methods_are_compared_with_the_table_and_only_told() {
        let report = compare(&catalog(), &mining());
        // Dinyx: careful, slow = the table's 3 and 0; Gaskin: the catalog says fast and normal, the table 3 and 1.
        assert_eq!(report.methods_match, ["dinyx"]);
        assert_eq!(report.methods_differ.len(), 1, "{:?}", report.methods_differ);
        assert!(report.methods_differ[0].starts_with("Gaskin Process"));
        assert_eq!(report.differences(), 0, "the methods are not failures");
    }
}
