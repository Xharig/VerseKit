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

//! Cross-check of the places where the raw materials lie, read from the game data (`versekit_gamedata::places`),
//! against the mining data of scmdb: which provider presets both name, whether their groups, deposits and
//! compositions agree (then the shares agree too: both go through the same working out), where a place of ours
//! is one of the hand table of the names of the points (`IN_GAME_NAMES`), and whether the refined good of an ore
//! by the link of the game database is the material scmdb names.
//!
//! `versekit-parity fundorte <dataforge.dcb> <global.ini> <catalog.json> <mining-data.json> [--gegenprobe]`, the
//! catalog being the whole one of p4kx. With `--gegenprobe` one share of a deposit, one part of a composition, one
//! probability of a group and one refined good are changed, and the check must report each of the four.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde_json::Value;
use versekit_core::mining_data::IN_GAME_NAMES;
use versekit_gamedata::places::{self, Place, Places};

/// How far two numbers may differ.
const EPSILON: f64 = 1e-9;
/// The ways of mining the game data know; scmdb adds a rare group of its own, which is left out.
const KINDS: [&str; 3] = ["SpaceShip_Mineables", "GroundVehicle_Mineables", "FPS_Mineables"];

fn read(file: &str) -> Result<Value, String> {
    serde_json::from_slice(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?)
        .map_err(|e| format!("{file}: {e}"))
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

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
            if self.examples.len() < 5 {
                self.examples.push(example());
            }
        }
    }
}

#[derive(Debug, Default)]
struct Report {
    groups: Count,
    deposits: Count,
    compositions: Count,
    points: Count,
    refined: Count,
    only_ours: Vec<String>,
    only_scmdb: Vec<String>,
    /// Places of ours with more points than the hand table names (the game data are a superset), and fewer.
    more_points: Vec<String>,
    fewer_points: Vec<String>,
    places: usize,
    compositions_used: usize,
    /// Raw materials the two sides name differently where the numbers are the same: (ours, scmdb). The texts of
    /// another build or another source, no fault of the data.
    names: Vec<(String, String)>,
}

impl Report {
    /// What differs in the data the shares are worked out from: groups, deposits, compositions, refined goods, and
    /// the presets only one side names.
    fn differences(&self) -> usize {
        self.groups.different
            + self.deposits.different
            + self.compositions.different
            + self.refined.different
            + self.only_ours.len()
            + self.only_scmdb.len()
    }

    fn text(&self) -> String {
        let mut out = String::new();
        let mut line = |name: &str, count: &Count| {
            let _ = writeln!(out, "{name}: {} gleich, {} verschieden", count.equal, count.different);
            for example in &count.examples {
                let _ = writeln!(out, "    {example}");
            }
        };
        line("Gruppen (Wahrscheinlichkeit)", &self.groups);
        line("Lagerstätten je Gruppe (Mischung und Anteil)", &self.deposits);
        line("Mischungen (Teile: Erz, Prozente, Wahrscheinlichkeit)", &self.compositions);
        line("Punkte der Haufen gegen die Handtabelle der Namen", &self.points);
        line("Veredelte Ware des Erzes gegen den Stoff von scmdb", &self.refined);
        let _ = writeln!(out, "Orte bei uns: {}, benutzte Mischungen: {}", self.places, self.compositions_used);
        let _ = writeln!(out, "Name des Erzes anders bei gleichen Zahlen (bei uns, bei scmdb): {:?}", self.names);
        let _ = writeln!(out, "Vorlage nur bei uns: {:?}", self.only_ours);
        let _ = writeln!(out, "Vorlage nur bei scmdb: {:?}", self.only_scmdb);
        let _ = writeln!(out, "Haufen mit mehr Punkten als die Handtabelle (Obermenge der Spieldaten): {:?}", self.more_points);
        let _ = writeln!(out, "Haufen mit weniger Punkten als die Handtabelle: {:?}", self.fewer_points);
        out
    }
}

type Groups = BTreeMap<String, (f64, Vec<(String, f64)>)>;

fn ours_groups(place: &Place) -> Groups {
    place
        .groups
        .iter()
        .map(|group| {
            let deposits = group.deposits.iter().map(|d| (d.composition.clone(), d.share)).collect();
            (group.name.clone(), (group.probability, deposits))
        })
        .collect()
}

fn theirs_groups(location: &Value) -> Groups {
    items(location, "groups")
        .iter()
        .filter_map(|group| {
            let name = group.get("groupName")?.as_str()?;
            KINDS.contains(&name).then_some(())?;
            let deposits = items(group, "deposits")
                .iter()
                .filter_map(|d| {
                    Some((
                        d.get("compositionGuid")?.as_str()?.to_ascii_lowercase(),
                        d.get("relativeProbability").and_then(Value::as_f64).unwrap_or(0.0),
                    ))
                })
                .collect();
            Some((name.to_string(), (group.get("groupProbability").and_then(Value::as_f64).unwrap_or(1.0), deposits)))
        })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPSILON
}

/// The points of a name of the hand table (`HUR L1 · HUR L4`), none for a name that stands for ranges.
fn hand_points(name: &str) -> Option<BTreeSet<String>> {
    (!name.contains('–')).then(|| name.split(" · ").map(str::to_string).collect())
}

fn compare(ours: &Places, theirs: &Value) -> Report {
    let mut report = Report { places: ours.places.len(), compositions_used: ours.compositions.len(), ..Report::default() };
    // The provider preset of a place is its template; per preset the first place states the groups.
    let mut by_template: BTreeMap<String, &Place> = BTreeMap::new();
    for place in &ours.places {
        by_template.entry(place.template.to_ascii_lowercase()).or_insert(place);
    }
    let mut their_by_preset: BTreeMap<String, &Value> = BTreeMap::new();
    for location in items(theirs, "locations") {
        let preset = location.get("presetFile").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        if !preset.is_empty() && !theirs_groups(location).is_empty() {
            their_by_preset.entry(preset).or_insert(location);
        }
    }
    report.only_ours = by_template.keys().filter(|key| !their_by_preset.contains_key(*key)).cloned().collect();
    report.only_scmdb = their_by_preset.keys().filter(|key| !by_template.contains_key(*key)).cloned().collect();
    let their_compositions = theirs.get("compositions");
    let mut seen_compositions: BTreeSet<String> = BTreeSet::new();
    let mut names_differ: BTreeSet<(String, String)> = BTreeSet::new();
    for (preset, place) in &by_template {
        let Some(location) = their_by_preset.get(preset) else { continue };
        let (ours_g, theirs_g) = (ours_groups(place), theirs_groups(location));
        for (name, (probability, deposits)) in &ours_g {
            let Some((their_probability, their_deposits)) = theirs_g.get(name) else {
                report.groups.add(false, || format!("{preset}: Gruppe {name} nur bei uns"));
                continue;
            };
            report.groups.add(near(*probability, *their_probability), || {
                format!("{preset}/{name}: {probability} gegen {their_probability}")
            });
            let same = deposits.len() == their_deposits.len()
                && deposits.iter().zip(their_deposits).all(|(a, b)| a.0 == b.0 && near(a.1, b.1));
            report.deposits.add(same, || {
                format!("{preset}/{name}: {} Lagerstätten gegen {}", deposits.len(), their_deposits.len())
            });
            seen_compositions.extend(deposits.iter().map(|d| d.0.clone()));
        }
        for name in theirs_g.keys().filter(|name| !ours_g.contains_key(*name)) {
            report.groups.add(false, || format!("{preset}: Gruppe {name} nur bei scmdb"));
        }
    }
    for guid in &seen_compositions {
        let (Some(ours_c), Some(theirs_c)) = (ours.compositions.get(guid), their_compositions.and_then(|c| c.get(guid)))
        else {
            report.compositions.add(false, || format!("{guid}: nur auf einer Seite"));
            continue;
        };
        let theirs_parts = items(theirs_c, "parts");
        let number = |part: &Value, key: &str| part.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let numbers_equal = |a: &places::Part, b: &Value| {
            near(a.min, number(b, "minPercent"))
                && near(a.max, number(b, "maxPercent"))
                && near(a.probability, b.get("probability").and_then(Value::as_f64).unwrap_or(1.0))
        };
        let name_of = |b: &Value| b.get("elementName").and_then(Value::as_str).unwrap_or("?").to_string();
        let numbers_same = ours_c.parts.len() == theirs_parts.len()
            && ours_c.parts.iter().zip(theirs_parts).all(|(a, b)| numbers_equal(a, b));
        // The numbers decide; a different name for the same raw material is told apart, and counted by itself.
        report.compositions.add(numbers_same, || {
            let (a, b) = (&ours_c.parts, theirs_parts);
            format!("{guid} ({}): {} Teile gegen {}; {:?}", ours_c.name, a.len(), b.len(), a.first().map(|p| (&p.ore, p.min, p.max)))
        });
        for (a, b) in ours_c.parts.iter().zip(theirs_parts).filter(|(a, b)| numbers_equal(a, b) && a.ore != name_of(b)) {
            names_differ.insert((a.ore.clone(), name_of(b)));
        }
    }
    // The clusters against the hand table of the names of the points.
    for place in ours.places.iter().filter(|place| place.kind == "Cluster" && place.points <= 6) {
        let Some((_, hand)) = IN_GAME_NAMES.iter().find(|(key, _)| key.eq_ignore_ascii_case(&place.template)) else {
            continue;
        };
        let Some(hand) = hand_points(hand) else { continue };
        let ours_set: BTreeSet<String> = place.name.split(" · ").map(str::to_string).collect();
        let label = format!("{}: {}", place.template, place.name);
        report.points.add(ours_set == hand, || label.clone());
        if ours_set.is_superset(&hand) && ours_set != hand {
            report.more_points.push(label);
        } else if hand.is_superset(&ours_set) && ours_set != hand {
            report.fewer_points.push(label);
        }
    }
    // The refined good of an ore (by the link of the game database) against the material scmdb names for the same
    // mineable element, found by its GUID.
    let refined_of: BTreeMap<&str, &Option<String>> = ours.ores.iter().map(|(ore, refined)| (ore.as_str(), refined)).collect();
    let mut element_ore: BTreeMap<&str, &str> = BTreeMap::new();
    for composition in ours.compositions.values() {
        for part in &composition.parts {
            element_ore.entry(part.element.as_str()).or_insert(part.ore.as_str());
        }
    }
    let their_elements = theirs.get("mineableElements").and_then(Value::as_object);
    for (element, ore) in &element_ore {
        let Some(material) = their_elements
            .and_then(|list| list.get(*element))
            .and_then(|e| e.get("materialName"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(Some(refined)) = refined_of.get(ore) else { continue };
        report.refined.add(refined.eq_ignore_ascii_case(material), || format!("{ore}: {refined} gegen {material}"));
    }
    report.names = names_differ.into_iter().collect();
    report
}

pub fn run(dcb: &str, ini: &str, catalog: &str, mining: &str, gegenprobe: bool) -> Result<(), String> {
    let bytes = std::fs::read(dcb).map_err(|e| format!("{dcb}: {e}"))?;
    let text = std::fs::read(ini).map_err(|e| format!("{ini}: {e}"))?;
    let goods = places::goods_from_dataforge(&bytes, &text).map_err(|e| e.to_string())?;
    let json = std::fs::read(catalog).map_err(|e| format!("{catalog}: {e}"))?;
    let ours = places::build(&json, &goods).map_err(|e| format!("{catalog}: {e}"))?;
    let theirs = read(mining)?;
    println!(
        "Gelesen: {} Güter, {} Orte, {} Mischungen, {} Erze",
        goods.len(),
        ours.places.len(),
        ours.compositions.len(),
        ours.ores.len()
    );
    let report = compare(&ours, &theirs);
    print!("{}", report.text());
    let known = report.only_ours.len() + report.only_scmdb.len();
    if report.differences() == known {
        println!("ERGEBNIS gleich (Vorlagen nur auf einer Seite: {known})");
    } else {
        println!("ERGEBNIS verschieden: {} Abweichungen in den Eingaben der Anteile", report.differences() - known);
    }
    if !gegenprobe {
        return Ok(());
    }
    // The counter-check: four faults, each of which the check has to report.
    let before = compare(&ours, &theirs);
    let mut missed = Vec::new();
    let mut fault = |name: &str, change: &dyn Fn(&mut Places) -> bool, count: &dyn Fn(&Report) -> usize| {
        let mut changed = ours.clone();
        if !change(&mut changed) {
            missed.push(format!("{name}: nichts zu ändern"));
            return;
        }
        let now = compare(&changed, &theirs);
        if count(&now) <= count(&before) {
            missed.push(format!("{name}: nicht gemeldet"));
        } else {
            println!("Gegenprobe {name}: gemeldet ({} gegen {})", count(&now), count(&before));
        }
    };
    fault(
        "Anteil einer Lagerstätte",
        &|p| p.places.iter_mut().flat_map(|pl| pl.groups.iter_mut()).flat_map(|g| g.deposits.iter_mut()).next().map(|d| d.share += 1.0).is_some(),
        &|r| r.deposits.different,
    );
    fault(
        "Teil einer Mischung",
        &|p| p.compositions.values_mut().flat_map(|c| c.parts.iter_mut()).next().map(|part| part.max += 1.0).is_some(),
        &|r| r.compositions.different,
    );
    fault(
        "Wahrscheinlichkeit einer Gruppe",
        &|p| p.places.iter_mut().flat_map(|pl| pl.groups.iter_mut()).next().map(|g| g.probability += 1.0).is_some(),
        &|r| r.groups.different,
    );
    fault(
        "veredelte Ware",
        &|p| p.ores.iter_mut().find(|(_, refined)| refined.is_some()).map(|(_, refined)| *refined = Some("Nichts".into())).is_some(),
        &|r| r.refined.different,
    );
    if missed.is_empty() {
        println!("Gegenprobe bestanden: alle vier Fehler gemeldet");
        Ok(())
    } else {
        Err(format!("Gegenprobe nicht bestanden: {missed:?}"))
    }
}
