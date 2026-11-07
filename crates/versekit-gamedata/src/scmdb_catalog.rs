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

//! The origin part of the blueprint catalog, built from scmdb's file
//! `merged-<build>.json` like the 4.12 catalog does (`catalog._origin`,
//! `catalog._pickup_places`, `catalog._missions` and
//! `cig_issues.apply_merged`).
//!
//! What it fills per blueprint: the key, the name, the ways (the contracts
//! that give it, the easiest first) and the reward pool of a blueprint no
//! contract gives. Kind, size, grade, class and manufacturer come from the
//! game, the starter blueprints and the "no known way" mark from other
//! places; they stay empty here.
//!
//! The known errors in CIG's data ([`Correction`]) work on the raw file
//! before it is read, like `apply_merged` does: the name of a pool row, a
//! blueprint referenced twice in a pool, the faction of a contract. A
//! correction that does not fit the file changes nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::Value;
use versekit_core::catalog_cache::{self, Blueprint, CatalogCache, Mission, Place, Way};
use versekit_core::paths::name_key;
use versekit_core::scmdb::FetchError;

use crate::corrections::{Correction, Field, Section};
use crate::list::pool_readable;

/// How many ways a blueprint keeps (`SOURCES_PER_BP` of 4.12): the easiest.
pub const WAYS_PER_BLUEPRINT: usize = 12;

/// How many places of a contract are named; the rest is counted (`mehr`).
pub const PLACES_PER_CONTRACT: usize = 4;

/// How many reward pools are named for a blueprint without a way.
const POOLS_PER_BLUEPRINT: usize = 2;

/// Sorts a missing reputation behind every real one.
const NO_REPUTATION: f64 = 1e9;

/// The name of the file of `build` at scmdb.
pub fn merged_file(build: &str) -> String {
    format!("merged-{build}.json")
}

/// Fetches `merged-<build>.json` (see `versekit_core::scmdb::fetch_file`)
/// and builds the origin part of the catalog; `issues` are the corrections
/// of the build and `user_agent` names VerseKit.
///
/// # Errors
///
/// The fetch error, or [`FetchError::Failed`] when the file is no JSON.
pub fn fetch(build: &str, issues: &[Correction], user_agent: &str) -> Result<CatalogCache, FetchError> {
    let bytes = versekit_core::scmdb::fetch_file(&merged_file(build), user_agent)?;
    let merged: Value = serde_json::from_slice(&bytes).map_err(|error| FetchError::Failed(error.to_string()))?;
    Ok(from_merged(&merged, build, issues))
}

/// The origin part of the catalog from the parsed file `merged`: every
/// blueprint some reward pool holds, with its ways or the name of its pool,
/// and the contracts that give blueprints (`missions`).
///
/// The blueprints are in the order of their names. The missions are in the
/// order their text keys first occur in the file; a mission lists the
/// blueprints of every variant of the same text, its head values come from
/// the easiest variant that gives blueprints, and all the other values 4.12
/// keeps per contract text are in `extra` (see
/// [`crate::contract_info::missions`]). `fetched` stays empty.
pub fn from_merged(merged: &Value, build: &str, issues: &[Correction]) -> CatalogCache {
    let fixes = Fixes::new(issues, build);
    let pools = read_pools(merged, &fixes);
    let missions = crate::contract_info::missions_of(merged, &pools)
        .iter()
        .map(|(key, entry)| Mission {
            key: key.trim_start_matches('@').to_string(),
            ..catalog_cache::mission_from_value(key, entry)
        })
        .collect();
    CatalogCache {
        version: build.to_string(),
        fetched: String::new(),
        blueprints: blueprints(merged, &pools, &fixes),
        missions,
        reputation: Some(crate::contract_info::reputation(merged, build, issues, "")),
    }
}

// ---------------------------------------------------------------- corrections

/// One blueprint referenced twice in a pool.
struct Duplicate {
    /// The blueprint referenced twice (record, lower case).
    blueprint: String,
    /// The entry to change, `None` for the second occurrence.
    index: Option<usize>,
    /// The record meant instead, `None` to drop the entry.
    replacement: Option<String>,
}

/// The corrections that work on `merged-<build>.json`, by what they name.
#[derive(Default)]
pub(crate) struct Fixes {
    /// Blueprint record (lower case) to the name meant.
    names: HashMap<String, String>,
    /// Pool record (lower case) to its blueprints referenced twice.
    duplicates: HashMap<String, Vec<Duplicate>>,
    /// Debug name of a contract (lower case) to the faction record meant and
    /// the reason.
    factions: HashMap<String, (String, String)>,
}

impl Fixes {
    pub(crate) fn new(issues: &[Correction], build: &str) -> Self {
        let mut fixes = Self::default();
        for issue in issues.iter().filter(|c| c.build.as_deref().is_none_or(|own| own == build)) {
            let expected = issue.expected.as_deref().filter(|text| !text.is_empty());
            match (issue.section, &issue.field) {
                (Section::Blueprint, Field::Name) => {
                    if let Some(name) = expected {
                        fixes.names.insert(issue.record.to_ascii_lowercase(), name.to_string());
                    }
                }
                (Section::Pool, Field::PoolEntry { blueprint, index }) => {
                    fixes.duplicates.entry(issue.record.to_ascii_lowercase()).or_default().push(Duplicate {
                        blueprint: blueprint.to_ascii_lowercase(),
                        index: *index,
                        replacement: expected.map(str::to_ascii_lowercase),
                    });
                }
                (Section::Contract, Field::Faction) => {
                    if let Some(faction) = expected {
                        fixes.factions.insert(
                            issue.record.to_ascii_lowercase(),
                            (faction.to_ascii_lowercase(), issue.reason.clone()),
                        );
                    }
                }
                // The entity a blueprint makes does not change its name in a
                // pool, and the other fields belong to other data.
                _ => {}
            }
        }
        fixes
    }

    /// The faction record of `contract`: the corrected one, else the raw one.
    fn faction_of(&self, contract: &Value) -> Option<String> {
        let debug_name = contract.get("debugName").and_then(Value::as_str).unwrap_or_default();
        if let Some((meant, _)) = self.factions.get(&debug_name.to_ascii_lowercase()) {
            return Some(meant.clone());
        }
        contract.get("factionGuid").and_then(Value::as_str).map(str::to_string)
    }

    /// `cig_issues.faction_fix` without the name of the faction: the record
    /// meant (lower case) and the reason for the contract with this debug
    /// name.
    pub(crate) fn faction_fix(&self, debug_name: &str) -> Option<(&str, &str)> {
        self.factions.get(&debug_name.to_ascii_lowercase()).map(|(guid, note)| (guid.as_str(), note.as_str()))
    }
}

// ----------------------------------------------------------------- the pools

/// One blueprint row of a reward pool.
struct Row {
    /// The blueprint record, lower case.
    record: String,
    name: Option<String>,
}

/// A reward pool: its name and the blueprints it holds.
pub(crate) struct Pool {
    name: String,
    rows: Vec<Row>,
    /// Names the file itself gives to the records meant instead of a
    /// blueprint referenced twice (`knownIssues`), by record in lower case.
    meant: HashMap<String, String>,
}

/// The names scmdb notes in a pool for the records meant in place of a
/// blueprint referenced twice.
fn meant_names(pool: &Value) -> HashMap<String, String> {
    let notes = items(pool.get("knownIssues").and_then(|issues| issues.get("duplicateReferences")));
    notes
        .iter()
        .filter_map(|note| {
            let expected = note.get("expected")?;
            let record = expected.get("blueprintRecord")?.as_str()?.trim().to_ascii_lowercase();
            Some((record, non_empty(expected.get("name"))?))
        })
        .collect()
}

impl Pool {
    /// The names of the rows that have one.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().filter_map(|row| row.name.as_deref())
    }
}

pub(crate) fn non_empty(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_string)
}

/// A number as a whole number, a fraction cut off like the cache does.
pub(crate) fn int_of(value: Option<&Value>) -> Option<i64> {
    let number = value?.as_number()?;
    number.as_i64().or_else(|| number.as_f64().filter(|x| x.is_finite()).map(|x| x as i64))
}

pub(crate) fn float_of(value: Option<&Value>) -> Option<f64> {
    value?.as_f64()
}

pub(crate) fn items(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// The pools with the corrections applied: the names of rows first, then
/// the blueprints referenced twice.
pub(crate) fn read_pools(merged: &Value, fixes: &Fixes) -> BTreeMap<String, Pool> {
    let mut pools = BTreeMap::new();
    for (guid, pool) in merged.get("blueprintPools").and_then(Value::as_object).into_iter().flatten() {
        if !pool.is_object() {
            continue;
        }
        let rows = items(pool.get("blueprints"))
            .iter()
            .map(|row| {
                let record = row.get("blueprintRecord").and_then(Value::as_str).unwrap_or_default();
                let record = record.trim().to_ascii_lowercase();
                let name = fixes.names.get(&record).cloned().or_else(|| non_empty(row.get("name")));
                Row { record, name }
            })
            .collect();
        let name = non_empty(pool.get("name")).unwrap_or_default();
        pools.insert(guid.clone(), Pool { name, rows, meant: meant_names(pool) });
    }
    if fixes.duplicates.is_empty() {
        return pools;
    }
    // A replacement row takes the name the pool notes for it, else the name
    // the record has in another pool.
    let mut known: HashMap<String, String> = HashMap::new();
    for row in pools.values().flat_map(|pool| &pool.rows) {
        if let Some(name) = &row.name {
            known.entry(row.record.clone()).or_insert_with(|| name.clone());
        }
    }
    for (guid, pool) in &mut pools {
        for duplicate in fixes.duplicates.get(&guid.to_ascii_lowercase()).into_iter().flatten() {
            let replacement = duplicate.replacement.as_deref();
            let name = replacement.and_then(|record| pool.meant.get(record).or_else(|| known.get(record)));
            fix_duplicate(&mut pool.rows, duplicate, name.cloned());
        }
    }
    pools
}

/// The second reference to a blueprint is replaced by the one meant, or
/// dropped when none is known.
fn fix_duplicate(rows: &mut Vec<Row>, duplicate: &Duplicate, name: Option<String>) {
    let places: Vec<usize> =
        rows.iter().enumerate().filter(|(_, row)| row.record == duplicate.blueprint).map(|(i, _)| i).collect();
    if places.len() < 2 {
        return;
    }
    let target = match duplicate.index {
        Some(index) => places.contains(&index).then_some(index),
        None => places.get(1).copied(),
    };
    let Some(target) = target else { return };
    match &duplicate.replacement {
        Some(record) => {
            let row = &mut rows[target];
            row.record.clone_from(record);
            if name.is_some() {
                row.name = name;
            }
        }
        None => {
            rows.remove(target);
        }
    }
}

// ---------------------------------------------------------------- the places

/// Where `contract` is taken: the system and the larger places, planets and
/// moons first (`catalog._pickup_places`); `None` when it names neither.
fn pickup_place(contract: &Value, places: Option<&Value>) -> Option<Place> {
    let (mut planets, mut other): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for ident in items(contract.get("locations")).iter().filter_map(Value::as_str) {
        let Some(place) = places.and_then(|pool| pool.get(ident)).filter(|place| place.is_object()) else { continue };
        let Some(name) = non_empty(place.get("name")) else { continue };
        let kind = place.get("type").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        match kind.as_str() {
            // The system stands next to it anyway.
            "star" => {}
            "planet" | "moon" => planets.push(name),
            _ => other.push(name),
        }
    }
    let names = unique(if planets.is_empty() { other } else { planets });
    let systems = systems_of(contract);
    if names.is_empty() && systems.is_empty() {
        return None;
    }
    Some(Place {
        system: systems.join(", "),
        more: i64::try_from(names.len().saturating_sub(PLACES_PER_CONTRACT)).unwrap_or(i64::MAX),
        places: names.into_iter().take(PLACES_PER_CONTRACT).collect(),
    })
}

fn unique(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names.into_iter().filter(|name| seen.insert(name.clone())).collect()
}

/// The systems a contract is offered in (`availableSystems`, else `systems`).
fn systems_of(contract: &Value) -> Vec<String> {
    let list = |key: &str| -> Vec<String> {
        items(contract.get(key)).iter().filter_map(Value::as_str).map(str::to_string).collect()
    };
    let available = list("availableSystems");
    if available.is_empty() { list("systems") } else { available }
}

// ------------------------------------------------------------------ the ways

/// The contracts of the file, the current ones first.
pub(crate) fn contracts(merged: &Value) -> impl Iterator<Item = &Value> {
    items(merged.get("contracts")).iter().chain(items(merged.get("legacyContracts")))
}

/// The reward pools of a contract that give blueprints.
pub(crate) fn reward_pools(contract: &Value) -> impl Iterator<Item = &str> {
    items(contract.get("blueprintRewards")).iter().filter_map(|reward| reward.get("blueprintPool")?.as_str())
}

/// The reputation a contract gives: the sum of its reward entries.
fn reputation_gain(contract: &Value, rewards: &[Value]) -> Option<i64> {
    let index = usize::try_from(contract.get("factionRewardsIndex")?.as_i64()?).ok()?;
    let entries = rewards.get(index)?.as_array()?;
    Some(entries.iter().map(|entry| int_of(entry.get("amount")).unwrap_or(0)).sum())
}

/// How hard a contract is to get into: the lowest reputation first, at a
/// tie the higher reward.
pub(crate) fn ease(contract: &Value) -> (f64, f64) {
    let reputation = contract.get("minStanding").and_then(|rank| float_of(rank.get("minReputation")));
    (reputation.unwrap_or(NO_REPUTATION), float_of(contract.get("rewardUEC")).unwrap_or(0.0))
}

pub(crate) fn easier(a: (f64, f64), b: (f64, f64)) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1))
}

/// A way as it is sorted and cleaned.
#[derive(Clone)]
struct Candidate {
    way: Way,
    /// Contract title and faction as the file has them, to drop the same
    /// contract reached through several pools.
    title: Option<String>,
    faction: Option<String>,
    ease: (f64, f64),
}

fn candidate(contract: &Value, merged: &Value, fixes: &Fixes) -> Candidate {
    let factions = merged.get("factions");
    let faction = fixes
        .faction_of(contract)
        .and_then(|guid| factions?.get(guid.as_str()))
        .and_then(|faction| non_empty(faction.get("name")));
    let rank = contract.get("minStanding").filter(|rank| rank.is_object());
    let rewards = items(merged.get("factionRewardsPools"));
    let title = non_empty(contract.get("title"));
    Candidate {
        way: Way {
            contract: title.clone().unwrap_or_default(),
            kind: non_empty(contract.get("missionType")).unwrap_or_default(),
            faction: faction.clone().unwrap_or_default(),
            reward: int_of(contract.get("rewardUEC")),
            reputation_gain: reputation_gain(contract, rewards),
            rank: rank.and_then(|rank| non_empty(rank.get("name"))),
            rank_reputation: rank.and_then(|rank| int_of(rank.get("minReputation"))),
            place: pickup_place(contract, merged.get("locationPools")),
        },
        title,
        faction,
        ease: ease(contract),
    }
}

/// The ways to each blueprint (by name key): the easiest first, the same
/// contract once, at most [`WAYS_PER_BLUEPRINT`].
fn ways(merged: &Value, pools: &BTreeMap<String, Pool>, fixes: &Fixes) -> HashMap<String, Vec<Way>> {
    let mut found: HashMap<String, Vec<Candidate>> = HashMap::new();
    for contract in contracts(merged) {
        if items(contract.get("blueprintRewards")).is_empty() {
            continue;
        }
        let way = candidate(contract, merged, fixes);
        for pool in reward_pools(contract).filter_map(|guid| pools.get(guid)) {
            for name in pool.names() {
                found.entry(name_key(name)).or_default().push(way.clone());
            }
        }
    }
    found
        .into_iter()
        .map(|(key, mut rows)| {
            rows.sort_by(|a, b| easier(a.ease, b.ease));
            let mut seen = HashSet::new();
            let ways = rows
                .into_iter()
                .filter(|row| seen.insert((row.title.clone(), row.faction.clone())))
                .take(WAYS_PER_BLUEPRINT)
                .map(|row| row.way)
                .collect();
            (key, ways)
        })
        .collect()
}

// ------------------------------------------------------------- the blueprints

/// The blueprints, in the order of their names. A name that gives the same
/// key as an earlier one replaces that entry in place.
fn blueprints(merged: &Value, pools: &BTreeMap<String, Pool>, fixes: &Fixes) -> Vec<Blueprint> {
    let ways = ways(merged, pools, fixes);
    let mut pool_names: HashMap<String, Vec<&str>> = HashMap::new();
    for pool in pools.values().filter(|pool| !pool.name.is_empty()) {
        for name in pool.names() {
            pool_names.entry(name_key(name)).or_default().push(&pool.name);
        }
    }
    let names: BTreeSet<&str> = pools.values().flat_map(Pool::names).collect();
    let mut found: Vec<Blueprint> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for name in names {
        let key = name_key(name);
        let way_list = ways.get(&key).cloned().unwrap_or_default();
        let pool = if way_list.is_empty() { pool_of(pool_names.get(&key)) } else { None };
        let blueprint =
            Blueprint { key: key.clone(), name: name.to_string(), ways: way_list, pool, ..Blueprint::default() };
        match index.get(&key) {
            Some(&position) => found[position] = blueprint,
            None => {
                index.insert(key, found.len());
                found.push(blueprint);
            }
        }
    }
    found
}

/// The pool a blueprint without a way comes from: the readable names of the
/// pools that hold it, the first two in order, joined.
fn pool_of(raw_names: Option<&Vec<&str>>) -> Option<String> {
    let readable: BTreeSet<String> = raw_names?.iter().map(|name| pool_readable(name)).collect();
    let joined = readable.into_iter().take(POOLS_PER_BLUEPRINT).collect::<Vec<_>>().join(" · ");
    (!joined.is_empty()).then_some(joined)
}

// The contracts that give blueprints (`missions`) are built in
// `contract_info`, which also builds the reputation table.

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn correction(section: Section, record: &str, field: Field, expected: Option<&str>) -> Correction {
        Correction {
            section,
            record: record.to_string(),
            field,
            raw: None,
            expected: expected.map(str::to_string),
            missing_target: false,
            reason: String::new(),
            build: None,
            source: "test",
        }
    }

    /// A contract that gives the blueprints of `pool`.
    fn contract(title: &str, text_key: &str, faction: &str, reputation: i64, reward: i64, pool: &str) -> Value {
        json!({
            "debugName": format!("Debug_{title}"),
            "title": title,
            "titleLocKey": text_key,
            "missionType": "Mercenary",
            "factionGuid": faction,
            "factionRewardsIndex": 0,
            "rewardUEC": reward,
            "minStanding": {"name": format!("Rank {reputation}"), "minReputation": reputation},
            "maxStanding": {"name": "Top", "minReputation": reputation + 1000},
            "canBeShared": true,
            "hasPersonalCooldown": false,
            "personalCooldownTime": 30,
            "locations": ["l1"],
            "availableSystems": ["Stanton"],
            "blueprintRewards": [{"blueprintPool": pool, "chance": 1}],
        })
    }

    fn merged(contracts: &[Value]) -> Value {
        json!({
            "version": "t-1",
            "factions": {"f1": {"name": "Alpha"}, "f2": {"name": "Beta"}},
            "factionRewardsPools": [[{"amount": 50}, {"amount": 25}]],
            "locationPools": {"l1": {"name": "Hurston", "type": "Planet"}},
            "blueprintPools": {
                "P1": {"name": "BP_MISSIONREWARD_Alpha_Easy", "blueprints": [
                    {"blueprintRecord": "B1", "name": "Gun A"},
                    {"blueprintRecord": "B2", "name": "Gun B"}]},
                "P2": {"name": "BP_REWARDS_RedWind_01_02", "blueprints": [
                    {"blueprintRecord": "B3", "name": "Odd Thing"}]},
            },
            "contracts": contracts,
            "legacyContracts": [],
        })
    }

    fn blueprint<'a>(catalog: &'a CatalogCache, key: &str) -> &'a Blueprint {
        catalog.blueprints.iter().find(|b| b.key == key).unwrap()
    }

    #[test]
    fn the_file_is_named_after_the_build() {
        assert_eq!(merged_file("4.10.2-live.12913470"), "merged-4.10.2-live.12913470.json");
    }

    #[test]
    fn places_name_planets_and_moons_first_at_most_four() {
        let pool = json!({
            "p1": {"name": "Hurston", "type": "Planet"}, "p2": {"name": "Aberdeen", "type": "Moon"},
            "p3": {"name": "Arial", "type": "Moon"}, "p4": {"name": "Magda", "type": "Moon"},
            "p5": {"name": "Ita", "type": "Moon"}, "p6": {"name": "Hurston", "type": "Planet"},
            "s1": {"name": "Stanton", "type": "Star"}, "o1": {"name": "HUR L1", "type": "Lagrange"},
            "o2": {"name": "Everus Harbor", "type": "Station"}, "bad": {"type": "Planet"},
        });
        let contract = json!({"locations": ["o1", "s1", "p1", "p2", "p3", "p4", "p5", "p6", "bad", "none"],
            "availableSystems": ["Stanton", "Nyx"]});
        let place = pickup_place(&contract, Some(&pool)).unwrap();
        assert_eq!(place.system, "Stanton, Nyx");
        assert_eq!(place.places, ["Hurston", "Aberdeen", "Arial", "Magda"]);
        assert_eq!(place.more, 1);
    }

    #[test]
    fn places_fall_back_to_the_rest_and_to_nothing() {
        let pool = json!({"o1": {"name": "HUR L1", "type": "Lagrange"}, "s1": {"name": "Stanton", "type": "Star"}});
        let contract = json!({"locations": ["o1", "s1"], "systems": ["Pyro"]});
        let place = pickup_place(&contract, Some(&pool)).unwrap();
        assert_eq!((place.system.as_str(), place.places, place.more), ("Pyro", vec!["HUR L1".to_string()], 0));
        // Only a system, no places.
        let only_system = pickup_place(&json!({"availableSystems": ["Nyx"]}), Some(&pool)).unwrap();
        assert_eq!((only_system.system.as_str(), only_system.places.len()), ("Nyx", 0));
        // Neither.
        assert_eq!(pickup_place(&json!({"locations": ["s1"]}), Some(&pool)), None);
        assert_eq!(pickup_place(&json!({}), None), None);
    }

    #[test]
    fn ways_come_easiest_first_without_the_same_contract_twice() {
        let contracts = [
            contract("Hard", "k_hard", "f1", 5000, 900, "P1"),
            contract("Easy Low Pay", "k_easy", "f1", 0, 100, "P1"),
            contract("Easy High Pay", "k_easy2", "f1", 0, 500, "P1"),
            contract("Easy High Pay", "k_easy3", "f1", 0, 500, "P1"),
            contract("Easy High Pay", "k_other", "f2", 0, 500, "P1"),
        ];
        let catalog = from_merged(&merged(&contracts), "t-1", &[]);
        let ways = &blueprint(&catalog, "gun a").ways;
        let titles: Vec<(&str, &str)> = ways.iter().map(|w| (w.contract.as_str(), w.faction.as_str())).collect();
        assert_eq!(
            titles,
            [("Easy High Pay", "Alpha"), ("Easy High Pay", "Beta"), ("Easy Low Pay", "Alpha"), ("Hard", "Alpha")]
        );
        let first = &ways[0];
        assert_eq!((first.kind.as_str(), first.reward, first.reputation_gain), ("Mercenary", Some(500), Some(75)));
        assert_eq!((first.rank.as_deref(), first.rank_reputation), (Some("Rank 0"), Some(0)));
        let place = first.place.as_ref().unwrap();
        assert_eq!((place.system.as_str(), place.places.len(), place.more), ("Stanton", 1, 0));
        assert_eq!(catalog.version, "t-1");
    }

    #[test]
    fn a_blueprint_without_a_way_names_its_pool() {
        let catalog = from_merged(&merged(&[contract("Only", "k", "f1", 0, 1, "P1")]), "t-1", &[]);
        assert_eq!(blueprint(&catalog, "odd thing").pool.as_deref(), Some("RedWind"));
        assert!(blueprint(&catalog, "odd thing").ways.is_empty());
        assert_eq!(blueprint(&catalog, "gun a").pool, None);
        let keys: Vec<&str> = catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["gun a", "gun b", "odd thing"]);
        assert!(catalog.blueprints.iter().all(|b| !b.no_way && !b.start && b.since.is_none() && b.kind.is_empty()));
    }

    #[test]
    fn a_mission_takes_its_head_values_from_the_easiest_variant_with_blueprints() {
        let mut easy = contract("Easy", "same_text", "f1", 0, 100, "P1");
        easy["hasPersonalCooldown"] = json!(true);
        easy["personalCooldownTime"] = json!(45);
        let hard = contract("Hard", "same_text", "f1", 9000, 900, "P2");
        // The easiest of all gives no blueprints and must not count.
        let mut empty = contract("Empty", "same_text", "f1", 0, 1, "P1");
        empty["blueprintRewards"] = json!([]);
        let other = contract("Other", "other_text", "f1", 200, 5, "P2");
        // In the order the text keys first occur in the file (as a Python
        // dict has them), not sorted: `same_text` first when `hard` is.
        let first = from_merged(&merged(&[hard.clone(), other.clone(), empty.clone(), easy.clone()]), "t-1", &[]);
        let keys: Vec<&str> = first.missions.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["same_text", "other_text"]);
        let catalog = from_merged(&merged(&[other, hard, empty, easy]), "t-1", &[]);
        assert_eq!(catalog.missions.len(), 2);
        assert_eq!(catalog.missions[0].key, "other_text");
        assert_eq!(catalog.missions[0].blueprints, ["odd thing"]);
        assert_eq!(catalog.missions[0].reputation_max, Some(1200));
        let mission = &catalog.missions[1];
        assert_eq!(mission.blueprints, ["gun a", "gun b", "odd thing"]);
        assert_eq!((mission.reputation_max, mission.rank_max.as_deref()), (Some(1000), Some("Top")));
        assert_eq!((mission.shareable, mission.cooldown), (Some(true), Some(45)));
    }

    #[test]
    fn a_name_correction_renames_the_row_and_changes_its_key() {
        let issues = [correction(Section::Blueprint, "b1", Field::Name, Some("Gun Renamed"))];
        let mut file = merged(&[contract("Only", "k", "f1", 0, 1, "P1")]);
        file["blueprintPools"]["P1"]["blueprints"][0]["blueprintRecord"] = json!("B1");
        let catalog = from_merged(&file, "t-1", &issues);
        assert_eq!(blueprint(&catalog, "gun renamed").name, "Gun Renamed");
        assert!(catalog.blueprints.iter().all(|b| b.key != "gun a"));
        assert_eq!(catalog.missions[0].blueprints, ["gun b", "gun renamed"]);
    }

    #[test]
    fn a_blueprint_referenced_twice_is_replaced_or_dropped() {
        let mut file = merged(&[contract("Only", "k", "f1", 0, 1, "P1")]);
        file["blueprintPools"]["P1"]["blueprints"] = json!([
            {"blueprintRecord": "B1", "name": "Gun A"},
            {"blueprintRecord": "B1", "name": "Gun A"},
        ]);
        file["blueprintPools"]["P2"]["blueprints"] = json!([
            {"blueprintRecord": "B3", "name": "Odd Thing"},
            {"blueprintRecord": "B3", "name": "Odd Thing"},
            {"blueprintRecord": "B4", "name": "Fourth"},
        ]);
        // The pool notes the name of the record meant.
        file["blueprintPools"]["P1"]["knownIssues"] = json!({"duplicateReferences": [
            {"blueprintRecord": "B1", "expected": {"blueprintRecord": "B9", "name": "Gun Nine"}}]});
        let dup = |blueprint: &str| Field::PoolEntry { blueprint: blueprint.to_string(), index: None };
        let issues =
            [correction(Section::Pool, "p1", dup("b1"), Some("b9")), correction(Section::Pool, "p2", dup("b3"), None)];
        let catalog = from_merged(&file, "t-1", &issues);
        let keys: Vec<&str> = catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["fourth", "gun a", "gun nine", "odd thing"]);
        // Without the corrections the pool keeps the twin.
        let raw = from_merged(&file, "t-1", &[]);
        assert!(raw.blueprints.iter().all(|b| b.key != "gun nine"));
    }

    #[test]
    fn a_replacement_takes_the_name_of_the_record_in_another_pool() {
        let mut file = merged(&[contract("Only", "k", "f1", 0, 1, "P1")]);
        file["blueprintPools"]["P1"]["blueprints"] = json!([
            {"blueprintRecord": "B1", "name": "Gun A"},
            {"blueprintRecord": "B1", "name": "Gun A"},
        ]);
        let field = Field::PoolEntry { blueprint: "b1".into(), index: None };
        let catalog = from_merged(&file, "t-1", &[correction(Section::Pool, "p1", field, Some("b3"))]);
        // The contract now gives the record of "Odd Thing" next to "Gun A".
        assert_eq!(catalog.missions[0].blueprints, ["gun a", "odd thing"]);
        assert_eq!(blueprint(&catalog, "odd thing").ways.len(), 1);
    }

    #[test]
    fn a_faction_correction_moves_the_contract_to_the_faction_meant() {
        let issues = [correction(Section::Contract, "Debug_Wrong", Field::Faction, Some("F2"))];
        let file = merged(&[contract("Wrong", "k", "f1", 0, 1, "P1"), contract("Right", "k2", "f1", 0, 1, "P1")]);
        let catalog = from_merged(&file, "t-1", &issues);
        let factions: BTreeSet<(String, String)> =
            blueprint(&catalog, "gun a").ways.iter().map(|w| (w.contract.clone(), w.faction.clone())).collect();
        let expected = [("Wrong", "Beta"), ("Right", "Alpha")].map(|(a, b)| (a.to_string(), b.to_string()));
        assert_eq!(factions, BTreeSet::from(expected));
    }

    #[test]
    fn a_correction_of_another_build_or_without_a_match_changes_nothing() {
        let file = merged(&[contract("Only", "k", "f1", 0, 1, "P1")]);
        let plain = from_merged(&file, "t-1", &[]);
        let mut elsewhere = correction(Section::Blueprint, "b1", Field::Name, Some("Other"));
        elsewhere.build = Some("t-2".into());
        let unknown = correction(Section::Contract, "Debug_Nothing", Field::Faction, Some("f2"));
        let unmatched = correction(Section::Pool, "p1", Field::PoolEntry { blueprint: "x".into(), index: None }, None);
        let missing_faction = correction(Section::Contract, "Debug_Only", Field::Faction, None);
        assert_eq!(from_merged(&file, "t-1", &[elsewhere, unknown, unmatched, missing_faction]), plain);
    }

    #[test]
    fn an_empty_or_odd_file_gives_an_empty_catalog() {
        for file in [json!({}), json!([]), json!({"blueprintPools": 3, "contracts": {"a": 1}})] {
            let catalog = from_merged(&file, "t-1", &[]);
            assert!(catalog.blueprints.is_empty() && catalog.missions.is_empty());
        }
    }
}
