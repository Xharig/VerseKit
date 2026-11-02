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

//! The origin part of the catalog built from scmdb's `merged-<build>.json`,
//! compared with the catalog the 4.12 version cached for the same build.
//!
//! Run with `cargo test -p versekit-gamedata --release --test
//! scmdb_catalog_real -- --ignored --nocapture`. The paths come from
//! `VERSEKIT_SCMDB_MERGED` (the merged file), `VERSEKIT_SCMDB_ISSUES` (the
//! error list `cig_data_issues-<build>.json` of the same build) and
//! `VERSEKIT_LEGACY_CATALOG` (the cached catalog of 4.12). The cached
//! catalog also holds values from the game that are not built here, so
//! only what the merged file decides is compared:
//!
//! - the ways of every blueprint both hold, field by field and in order,
//! - the pool of a blueprint the old catalog gives no way and no start mark,
//! - the missions as a multiset.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde_json::Value;
use versekit_core::catalog_cache::{self, Blueprint, CatalogCache, Mission, Way};
use versekit_core::jsonio;
use versekit_gamedata::corrections::{self, Correction};
use versekit_gamedata::scmdb_catalog::from_merged;

fn path_of(name: &str) -> PathBuf {
    std::env::var_os(name).map(PathBuf::from).unwrap_or_else(|| panic!("set {name}"))
}

fn json_of(name: &str) -> Value {
    let bytes = std::fs::read(path_of(name)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// The fields in which two ways differ.
fn differences(ours: &Way, old: &Way) -> Vec<&'static str> {
    let mut found = Vec::new();
    let mut check = |name, same: bool| {
        if !same {
            found.push(name);
        }
    };
    check("contract", ours.contract == old.contract);
    check("kind", ours.kind == old.kind);
    check("faction", ours.faction == old.faction);
    check("reward", ours.reward == old.reward);
    check("reputation_gain", ours.reputation_gain == old.reputation_gain);
    check("rank", ours.rank == old.rank);
    check("rank_reputation", ours.rank_reputation == old.rank_reputation);
    check("place", ours.place == old.place);
    found
}

type MissionKey = (Vec<String>, Option<i64>, Option<String>, Option<bool>, Option<i64>);

fn mission_key(mission: &Mission) -> MissionKey {
    let mut keys = mission.blueprints.clone();
    keys.sort();
    (keys, mission.reputation_max, mission.rank_max.clone(), mission.shareable, mission.cooldown)
}

fn multiset(missions: &[Mission]) -> BTreeMap<MissionKey, usize> {
    let mut count = BTreeMap::new();
    for mission in missions {
        *count.entry(mission_key(mission)).or_insert(0) += 1;
    }
    count
}

fn by_key(catalog: &CatalogCache) -> HashMap<&str, &Blueprint> {
    catalog.blueprints.iter().map(|b| (b.key.as_str(), b)).collect()
}

/// What the comparison found.
#[derive(Default)]
struct Report {
    only_here: Vec<String>,
    /// Only in 4.12 and neither marked without a way nor a starter.
    unexplained: Vec<String>,
    way_errors: Vec<String>,
    pool_errors: Vec<String>,
    mission_errors: Vec<String>,
}

impl Report {
    fn differences(&self) -> usize {
        self.only_here.len()
            + self.unexplained.len()
            + self.way_errors.len()
            + self.pool_errors.len()
            + self.mission_errors.len()
    }
}

fn show(title: &str, lines: &[String]) {
    println!("{title}: {}", lines.len());
    for line in lines.iter().take(10) {
        println!("  {line}");
    }
}

fn compare(ours: &CatalogCache, old: &CatalogCache) -> Report {
    let mut report = Report::default();
    println!("blueprints: {} here, {} in 4.12", ours.blueprints.len(), old.blueprints.len());
    let ours_by_key = by_key(ours);
    let old_by_key = by_key(old);
    report.only_here = ours_by_key.keys().filter(|k| !old_by_key.contains_key(*k)).map(|k| (*k).to_string()).collect();
    let only_old: Vec<&&Blueprint> =
        old_by_key.iter().filter(|(k, _)| !ours_by_key.contains_key(**k)).map(|(_, b)| b).collect();
    report.unexplained = only_old.iter().filter(|b| !b.no_way && !b.start).map(|b| b.key.clone()).collect();
    println!(
        "only in 4.12: {} (no way: {}, start: {}, neither: {})",
        only_old.len(),
        only_old.iter().filter(|b| b.no_way).count(),
        only_old.iter().filter(|b| b.start).count(),
        report.unexplained.len()
    );

    // (a) the ways and (b) the pool
    let (mut compared, mut ways_compared, mut pools_compared) = (0usize, 0usize, 0usize);
    for (key, theirs) in &old_by_key {
        let Some(own) = ours_by_key.get(key) else { continue };
        compared += 1;
        if own.ways.len() != theirs.ways.len() {
            report.way_errors.push(format!("{key}: {} ways here, {} in 4.12", own.ways.len(), theirs.ways.len()));
        }
        for (i, (a, b)) in own.ways.iter().zip(&theirs.ways).enumerate() {
            ways_compared += 1;
            let fields = differences(a, b);
            if !fields.is_empty() {
                report.way_errors.push(format!("{key} way {i}: {fields:?}: here {a:?} | 4.12 {b:?}"));
            }
        }
        if theirs.ways.is_empty() && !theirs.start {
            pools_compared += 1;
            if own.pool != theirs.pool {
                report.pool_errors.push(format!("{key}: pool here {:?}, in 4.12 {:?}", own.pool, theirs.pool));
            }
        }
    }
    println!("compared: {compared} blueprints, {ways_compared} ways, {pools_compared} pools");

    // (c) the missions
    let (here, there) = (multiset(&ours.missions), multiset(&old.missions));
    for (key, count) in &here {
        let other = there.get(key).copied().unwrap_or(0);
        if other != *count {
            report.mission_errors.push(format!("here {count}x, 4.12 {other}x: {key:?}"));
        }
    }
    for (key, count) in &there {
        if !here.contains_key(key) {
            report.mission_errors.push(format!("here 0x, 4.12 {count}x: {key:?}"));
        }
    }
    println!("missions: {} here, {} in 4.12", ours.missions.len(), old.missions.len());

    show("blueprints only here", &report.only_here);
    show("blueprints only in 4.12 without a mark", &report.unexplained);
    show("way differences", &report.way_errors);
    show("pool differences", &report.pool_errors);
    show("mission differences", &report.mission_errors);
    report
}

fn files() -> (Value, String, Vec<Correction>, CatalogCache) {
    let merged = json_of("VERSEKIT_SCMDB_MERGED");
    let build = merged["version"].as_str().unwrap().to_string();
    let issues = corrections::from_scmdb(&json_of("VERSEKIT_SCMDB_ISSUES"), &build);
    let old = catalog_cache::parse(
        &jsonio::loads(&std::fs::read_to_string(path_of("VERSEKIT_LEGACY_CATALOG")).unwrap()).unwrap(),
    );
    (merged, build, issues, old)
}

#[test]
#[ignore = "reads the real files of scmdb and the cached catalog of 4.12"]
fn the_origin_matches_the_catalog_of_4_12() {
    let (merged, build, issues, old) = files();
    println!("build {build}: {} corrections", issues.len());
    let ours = from_merged(&merged, &build, &issues);
    assert_eq!(ours.version, build);
    let report = compare(&ours, &old);
    assert_eq!(report.differences(), 0);
}

/// The comparison can fail: without the corrections the data differ.
#[test]
#[ignore = "reads the real files of scmdb and the cached catalog of 4.12"]
fn without_the_corrections_the_comparison_finds_differences() {
    let (merged, build, _, old) = files();
    let ours = from_merged(&merged, &build, &[]);
    let report = compare(&ours, &old);
    assert!(!report.way_errors.is_empty() || !report.pool_errors.is_empty() || !report.mission_errors.is_empty());
}
