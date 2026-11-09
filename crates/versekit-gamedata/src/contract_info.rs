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

//! What the blueprint notes in the game's text file need to know about a
//! contract, from scmdb's file `merged-<build>.json`:
//!
//! - [`missions`]: the contracts that give blueprints in the format of the
//!   4.12 cache entry `missionen` (`catalog._missions`, `_per_system` and
//!   `system_sort_key`): per contract text the blueprints, the sure and the
//!   best chance, the easiest variant's reputation, rank, reward and
//!   cooldown, from which rank a blueprint is to be had, the blueprints per
//!   system and how many variants give nothing;
//! - [`reputation`]: whom a contract credits reputation, of which kind, and
//!   where it plays, as the table `auftragsruf.json`
//!   (`reputation.prepare` and `prepare_regions`);
//! - [`fingerprint`]: the short sum over the known errors of the build
//!   (`cig_issues.fingerprint`), which the table keeps to notice a changed
//!   list.
//!
//! The values are Python values ([`Value`]): an empty field is left out,
//! `0` and `false` stay, a number keeps the form of the source (`1` and
//! `1.0` differ). The known errors ([`Correction`]) work as in 4.12: a pool
//! row renamed or replaced before the missions are built, the faction of a
//! contract before the reputation is written.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value as Json;
use versekit_core::injection::reputation::{Table, key as reputation_key};
use versekit_core::injection::terms::Terms;
use versekit_core::jsonio::{self, Map, Value};

use crate::corrections::Correction;
use crate::scmdb_catalog::{Fixes, Pool, contracts, ease, easier, items, non_empty, read_pools, reward_pools};

/// The systems in the order the notes name them; others follow by name.
pub const SYSTEM_ORDER: [&str; 3] = ["Stanton", "Pyro", "Nyx"];

/// How many planets are named behind a system at most; more, and only the
/// system stands there.
pub const MAX_PLANETS: usize = 4;

// ---------------------------------------------------------------- python values

/// A JSON value as `json.load` gives it: `1` and `1.0` stay what they were.
fn py(value: &Json) -> Value {
    match value {
        Json::Null => Value::Null,
        Json::Bool(flag) => Value::Bool(*flag),
        Json::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(n), _) => Value::int(n),
            (None, Some(n)) => Value::Int(n.to_string()),
            _ => Value::Float(number.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(text) => Value::Str(text.clone()),
        Json::Array(list) => Value::Array(list.iter().map(py).collect()),
        Json::Object(map) => Value::Object(map.iter().map(|(key, item)| (key.clone(), py(item))).collect()),
    }
}

/// `dict.get(key)` where `None` and a missing key are the same.
fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.get(key).filter(|item| !item.is_null())
}

/// Python's truth value; a missing value is false.
fn truthy(value: Option<&Json>) -> bool {
    match value {
        None | Some(Json::Null) => false,
        Some(Json::Bool(flag)) => *flag,
        Some(Json::Number(number)) => number.as_f64().is_some_and(|x| x != 0.0),
        Some(Json::String(text)) => !text.is_empty(),
        Some(Json::Array(list)) => !list.is_empty(),
        Some(Json::Object(map)) => !map.is_empty(),
    }
}

/// `value or {}` as an object: the object itself when it holds something.
fn object_of<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    field(value, key).filter(|item| item.is_object())
}

/// A number as Python compares it; `true` is `1`, anything else `0`.
fn number(value: Option<&Json>) -> f64 {
    match value {
        Some(Json::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Json::Bool(flag)) => f64::from(u8::from(*flag)),
        _ => 0.0,
    }
}

/// What makes two values the same member of a Python set: `800` and `800.0`
/// are one, a missing value is `None`.
fn identity(value: Option<&Json>) -> String {
    match value {
        None | Some(Json::Null) => "-".to_string(),
        Some(Json::Number(_) | Json::Bool(_)) => format!("n{:016x}", (number(value) + 0.0).to_bits()),
        Some(other) => format!("j{other}"),
    }
}

/// The elements of a value Python loops over: a list, or the characters of
/// a text; falsy elements are left out.
fn elements(value: Option<&Json>) -> Vec<String> {
    match value {
        Some(Json::Array(list)) => {
            list.iter().filter_map(Json::as_str).filter(|s| !s.is_empty()).map(String::from).collect()
        }
        Some(Json::String(text)) => text.chars().map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// An entry is dropped when it is `None`, `''` or `[]`; `0` and `false` stay.
fn kept(value: &Value) -> bool {
    !matches!(value, Value::Null)
        && !matches!(value, Value::Str(text) if text.is_empty())
        && !matches!(value, Value::Array(list) if list.is_empty())
}

// -------------------------------------------------------------------- the missions

/// `catalog.system_sort_key`: Stanton, Pyro, Nyx, then the others by name.
pub fn system_sort_key(name: &str) -> (u8, usize, String) {
    match SYSTEM_ORDER.iter().position(|known| *known == name) {
        Some(index) => (0, index, String::new()),
        None => (1, 0, name.to_string()),
    }
}

/// One variant of a contract text.
struct Variant<'a> {
    /// The blueprints its reward pools hold.
    names: BTreeSet<&'a str>,
    contract: &'a Json,
    /// `availableSystems`, else `systems`.
    systems: Vec<String>,
}

impl<'a> Variant<'a> {
    fn new(contract: &'a Json, pools: &'a BTreeMap<String, Pool>) -> Self {
        let names = reward_pools(contract).filter_map(|guid| pools.get(guid)).flat_map(Pool::names).collect();
        let listed = field(contract, "availableSystems").filter(|list| truthy(Some(list)));
        let systems = match listed.or_else(|| field(contract, "systems")) {
            Some(Json::Array(list)) => list
                .iter()
                .map(|item| match item {
                    Json::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect(),
            _ => Vec::new(),
        };
        Self { names, contract, systems }
    }

    fn stand(&self) -> Option<&'a Json> {
        object_of(self.contract, "minStanding")
    }

    /// The reputation the rank starts at.
    fn reputation(&self) -> Option<&'a Json> {
        self.stand().and_then(|rank| field(rank, "minReputation"))
    }

    fn rank(&self) -> Option<&'a Json> {
        self.stand().and_then(|rank| field(rank, "name"))
    }

    fn rewards(&self) -> &'a [Json] {
        items(self.contract.get("blueprintRewards"))
    }
}

/// The first of the easiest variants: the lowest reputation, at a tie the
/// higher reward.
fn easiest<'a, 'b>(list: impl IntoIterator<Item = &'b Variant<'a>>) -> Option<&'b Variant<'a>>
where
    'a: 'b,
{
    list.into_iter().reduce(|best, next| {
        if easier(ease(next.contract), ease(best.contract)) == std::cmp::Ordering::Less { next } else { best }
    })
}

/// The reputation a contract gives: the sum of the entries of its reward
/// list, as Python adds them (`int` until a `float` shows up).
fn reputation_gain(contract: &Json, rewards: &[Json]) -> Option<Value> {
    let index = usize::try_from(field(contract, "factionRewardsIndex")?.as_i64()?).ok()?;
    let entries = rewards.get(index)?.as_array()?;
    let mut whole: i128 = 0;
    let mut fraction: Option<f64> = None;
    for amount in entries.iter().map(|entry| entry.get("amount")) {
        match amount {
            Some(Json::Number(n)) if n.is_f64() => {
                fraction = Some(fraction.unwrap_or(whole as f64) + n.as_f64().unwrap_or(0.0));
            }
            Some(Json::Number(n)) => {
                let n = n.as_i64().map_or_else(|| n.as_u64().map_or(0, i128::from), i128::from);
                match fraction {
                    Some(sum) => fraction = Some(sum + n as f64),
                    None => whole += n,
                }
            }
            _ => {}
        }
    }
    Some(fraction.map_or_else(|| Value::Int(whole.to_string()), Value::Float))
}

/// `{System: [blueprints]}` over the variants of one contract text, or
/// nothing: when a variant names no system, when only one system occurs and
/// when every system gives the same list the shared list says the same.
fn per_system(variants: &[&Variant<'_>]) -> Option<Value> {
    let mut found: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for variant in variants {
        if variant.systems.is_empty() {
            return None;
        }
        for system in &variant.systems {
            found.entry(system).or_default().extend(variant.names.iter().copied());
        }
    }
    let lists: BTreeSet<&BTreeSet<&str>> = found.values().collect();
    if found.len() < 2 || lists.len() < 2 {
        return None;
    }
    let mut systems: Vec<&str> = found.keys().copied().collect();
    systems.sort_by_key(|system| system_sort_key(system));
    let list = |names: &BTreeSet<&str>| Value::Array(names.iter().map(|name| Value::str(*name)).collect());
    Some(Value::Object(systems.into_iter().map(|system| (system.to_string(), list(&found[system]))).collect()))
}

/// The contracts of `merged` that give blueprints, in the format of the 4.12
/// cache entry `missionen`: the key is the text key (`titleLocKey`, else
/// `descriptionLocKey`) as the file has it, the order that of its first
/// occurrence. `issues` are the known errors of `build`.
pub fn missions(merged: &Json, build: &str, issues: &[Correction]) -> Map {
    missions_of(merged, &read_pools(merged, &Fixes::new(issues, build)))
}

/// [`missions`] with the pools read (and corrected) already.
pub(crate) fn missions_of(merged: &Json, pools: &BTreeMap<String, Pool>) -> Map {
    let rewards = items(merged.get("factionRewardsPools"));
    // A text key shared by several contracts: they are one mission.
    let mut order: Vec<(String, Vec<&Json>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for contract in contracts(merged) {
        let Some(key) = non_empty(contract.get("titleLocKey")).or_else(|| non_empty(contract.get("descriptionLocKey")))
        else {
            continue;
        };
        let at = *index.entry(key.clone()).or_insert_with(|| {
            order.push((key, Vec::new()));
            order.len() - 1
        });
        order[at].1.push(contract);
    }
    let mut result = Map::new();
    for (key, variants) in order {
        let seen: Vec<Variant<'_>> = variants.into_iter().map(|contract| Variant::new(contract, pools)).collect();
        // Variants without blueprints stay in `seen`: it shows that there are
        // levels of this contract that give nothing.
        let with: Vec<&Variant<'_>> = seen.iter().filter(|variant| !variant.names.is_empty()).collect();
        let Some(easy) = easiest(with.iter().copied()) else { continue };
        let all: BTreeSet<&str> = with.iter().flat_map(|variant| variant.names.iter().copied()).collect();
        let distinct_reputations: BTreeSet<String> =
            with.iter().map(|variant| identity(variant.reputation())).collect();

        // From which rank a blueprint is to be had; only where the levels
        // differ, and only when that tells blueprints apart.
        let mut from_rank: Vec<(&str, Value)> = Vec::new();
        if distinct_reputations.len() > 1 {
            let mut pairs: BTreeSet<String> = BTreeSet::new();
            for name in &all {
                let Some(first) = easiest(with.iter().copied().filter(|variant| variant.names.contains(name))) else {
                    continue;
                };
                let (rank, reputation) = (first.rank(), first.reputation());
                if truthy(rank) && truthy(reputation) {
                    let pair = vec![
                        ("rang".to_string(), rank.map_or(Value::Null, py)),
                        ("rep".to_string(), reputation.map_or(Value::Null, py)),
                    ];
                    from_rank.push((name, Value::Object(pair)));
                    pairs.insert(format!("{}|{}", identity(rank), identity(reputation)));
                }
            }
            if pairs.len() < 2 {
                from_rank.clear();
            }
        }

        let contract = easy.contract;
        // Sure and chance over all variants: a blueprint one of them
        // guarantees can be reached.
        let all_rewards = || with.iter().flat_map(|variant| variant.rewards());
        let sure = all_rewards().any(|reward| number(reward.get("chance")) == 1.0);
        let mut chance: Option<(f64, Value)> = None;
        for reward in all_rewards() {
            let given = field(reward, "chance").filter(|item| truthy(Some(item)));
            let (size, value) = given.map_or((0.0, Value::int(0)), |item| (number(Some(item)), py(item)));
            if chance.as_ref().is_none_or(|(best, _)| size > *best) {
                chance = Some((size, value));
            }
        }
        let rank = object_of(contract, "minStanding");
        let highest = object_of(contract, "maxStanding");
        let part = |from: Option<&Json>, name: &str| from.and_then(|object| field(object, name)).map(py);
        let cooldown = truthy(contract.get("hasPersonalCooldown"))
            .then(|| field(contract, "personalCooldownTime").map(py))
            .flatten();
        let head = [
            ("bp", Some(Value::Array(all.iter().map(|name| Value::str(*name)).collect()))),
            ("sicher", Some(Value::Bool(sure))),
            ("chance", chance.map(|(_, value)| value)),
            ("rep", part(rank, "minReputation")),
            ("rang", part(rank, "name")),
            ("rep_max", part(highest, "minReputation")),
            ("rang_max", part(highest, "name")),
            ("uec", field(contract, "rewardUEC").map(py)),
            ("ruf", reputation_gain(contract, rewards)),
            ("teilbar", field(contract, "canBeShared").map(py)),
            ("cooldown", cooldown),
        ];
        let mut entry: Map = head
            .into_iter()
            .filter_map(|(name, value)| Some((name.to_string(), value?)))
            .filter(|(_, v)| kept(v))
            .collect();
        if !from_rank.is_empty() {
            entry.push(("ab".into(), Value::Object(from_rank.into_iter().map(|(n, v)| (n.to_string(), v)).collect())));
        }
        // Only when every level with blueprints has the same threshold, it
        // can stand in the title.
        if distinct_reputations.len() == 1 {
            entry.push(("rep_eindeutig".into(), Value::Bool(true)));
        }
        if let Some(systems) = per_system(&with) {
            entry.push(("je_system".into(), systems));
        }
        let empty = seen.len() - with.len();
        if empty > 0 {
            entry.push(("leer".into(), Value::int(i64::try_from(empty).unwrap_or(i64::MAX))));
            entry.push(("stufen".into(), Value::int(i64::try_from(seen.len()).unwrap_or(i64::MAX))));
        }
        for (name, from) in [("titel_key", "titleLocKey"), ("text_key", "descriptionLocKey")] {
            if let Some(text) = non_empty(contract.get(from)) {
                entry.push((name.into(), Value::Str(text)));
            }
        }
        jsonio::map_set(&mut result, &key, Value::Object(entry));
    }
    result
}

// -------------------------------------------------------------------- reputation

/// The table of `auftragsruf.json` from `merged`: whom a contract credits
/// reputation (`auftraege`), where it plays (`regionen`) and the sum over the
/// known errors it was made with (`fingerprint`, see [`fingerprint`]).
///
/// The version is the one of the file, else `build`. The faction of a
/// contract named by `issues` gets the reputation the raw file credits to the
/// wrong one; the name of that faction comes from the file (4.12 falls back
/// on the name in the error list when the file has none, which the
/// [`Correction`] does not keep).
pub fn reputation(merged: &Json, build: &str, issues: &[Correction], fingerprint: &str) -> Table {
    let version = non_empty(merged.get("version")).unwrap_or_else(|| build.to_string());
    Table {
        version,
        contracts: parties(merged, &Fixes::new(issues, build)),
        regions: regions(merged),
        fingerprint: fingerprint.to_string(),
        terms: versekit_core::injection::terms::summarize(&terms(merged)),
        terms_source: versekit_core::injection::reputation::TERMS_FROM_SCMDB.to_string(),
    }
}

/// The terms of every contract of `merged` with the key of its title, as
/// [`crate::contract_reputation::terms`] reads them from the game data.
pub fn terms(merged: &Json) -> Vec<(String, Terms)> {
    let whole = |value: Option<&Json>| value.and_then(Json::as_f64).filter(|x| x.is_finite()).map(|x| x.round() as i64);
    let flag = |contract: &Json, name: &str| contract.get(name).and_then(Json::as_bool);
    let rank = |contract: &Json, name: &str| {
        let standing = object_of(contract, name)?;
        Some((non_empty(standing.get("name"))?, standing.get("minReputation")?.as_i64()?))
    };
    items(merged.get("contracts"))
        .iter()
        .filter_map(|contract| {
            let title = non_empty(contract.get("titleLocKey")).or_else(|| non_empty(contract.get("titleKey")))?;
            let personal = flag(contract, "hasPersonalCooldown") == Some(true);
            let terms = Terms {
                cooldown: personal.then(|| whole(contract.get("personalCooldownTime"))).flatten(),
                // scmdb's file states no spread of the cooldowns.
                cooldown_spread: None,
                abandoned: whole(contract.get("abandonedCooldownTime")),
                abandoned_spread: None,
                buy_in: whole(contract.get("buyIn")),
                shareable: flag(contract, "canBeShared"),
                again_abandoned: flag(contract, "canReacceptAfterAbandoning"),
                again_failed: flag(contract, "canReacceptAfterFailing"),
                description: non_empty(contract.get("descriptionLocKey")),
                lowest: rank(contract, "minStanding"),
                highest: rank(contract, "maxStanding"),
                // scmdb's file does not tell a fixed payout from a computed one.
                reward: None,
            };
            Some((title, terms))
        })
        .collect()
}

/// `reputation.prepare`: `{key: [{wer, was, wieviel}, ...]}`, the key being
/// the text key without `@` in lower case; the contracts of one key are
/// joined (`reputation::merge_parties`), as the game data's are.
fn parties(merged: &Json, fixes: &Fixes) -> Map {
    let pools = items(merged.get("factionRewardsPools"));
    let name_of = |table: &str, guid: Option<&str>| {
        let item = guid.and_then(|guid| merged.get(table)?.get(guid));
        item.and_then(|item| non_empty(item.get(if table == "scopes" { "displayName" } else { "name" })))
    };
    let mut out: Vec<(String, Vec<Map>)> = Vec::new();
    for contract in items(merged.get("contracts")) {
        let raw = non_empty(contract.get("titleLocKey")).or_else(|| non_empty(contract.get("titleKey")));
        let Some(key) = raw.map(|raw| reputation_key(&raw)).filter(|key| !key.is_empty()) else { continue };
        let Some(index) = contract.get("factionRewardsIndex").and_then(Json::as_i64) else { continue };
        let Some(entries) = usize::try_from(index).ok().and_then(|index| pools.get(index)) else { continue };
        let debug_name = contract.get("debugName").and_then(Json::as_str).unwrap_or_default();
        let fix = fixes.faction_fix(debug_name);
        let own = contract.get("factionGuid").and_then(Json::as_str).unwrap_or_default().to_lowercase();
        let wrong = fix.filter(|(meant, _)| own != *meant).map(|_| own);
        let mut rows = Vec::new();
        for part in entries.as_array().into_iter().flatten().filter(|part| part.is_object()) {
            let amount = field(part, "amount").filter(|amount| truthy(Some(amount)));
            let Some(amount) = amount else { continue };
            let guid = part.get("factionGuid").and_then(Json::as_str);
            let faction = name_of("factions", guid);
            let kind = name_of("scopes", part.get("scopeGuid").and_then(Json::as_str));
            if faction.is_none() && kind.is_none() {
                continue;
            }
            let mut row: Map = vec![
                ("wer".into(), Value::Str(faction.clone().unwrap_or_default())),
                ("was".into(), Value::Str(kind.unwrap_or_default())),
                ("wieviel".into(), py(amount)),
            ];
            if let (Some(wrong), Some((meant, note))) = (&wrong, fix)
                && guid.unwrap_or_default().to_lowercase() == *wrong
                && let Some(right) = name_of("factions", Some(meant))
            {
                jsonio::map_set(&mut row, "wer", Value::Str(right));
                let was: Map =
                    vec![("statt".into(), Value::Str(faction.unwrap_or_default())), ("note".into(), Value::str(note))];
                row.push(("datenfehler".into(), Value::Object(was)));
            }
            rows.push(row);
        }
        out.push((key, rows));
    }
    versekit_core::injection::reputation::merge_parties(&out)
}

/// What the contracts with one text key say about where they play.
#[derive(Default)]
struct Where {
    systems: BTreeSet<String>,
    pyro: BTreeSet<String>,
    planets: BTreeMap<String, BTreeSet<String>>,
}

/// `reputation.prepare_regions`: `{key: 'Stanton (Crusader, microTech)'}`.
fn regions(merged: &Json) -> Map {
    let pools = merged.get("locationPools").and_then(Json::as_object);
    let text = |value: Option<&Json>| value.and_then(Json::as_str).filter(|text| !text.is_empty()).map(String::from);
    let mut pyro: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (letter, region) in merged.get("pyroRegions").and_then(Json::as_object).into_iter().flatten() {
        let names = items(region.get("locations"))
            .iter()
            .filter(|place| place.is_object() && place.get("navIcon").and_then(Json::as_str) == Some("Planet"))
            .filter_map(|place| text(place.get("name")))
            .collect();
        pyro.insert(letter.clone(), names);
    }
    // Every planet of a system: a contract that names them all plays
    // everywhere in it, and then only the system stands there.
    let mut everywhere: HashMap<String, BTreeSet<String>> = HashMap::new();
    for pool in pools.into_iter().flat_map(|pools| pools.values()) {
        if let (Some(planet), Some(system)) = (text(pool.get("planet")), text(pool.get("system"))) {
            everywhere.entry(system).or_default().insert(planet);
        }
    }
    for names in pyro.values() {
        everywhere.entry("Pyro".into()).or_default().extend(names.iter().cloned());
    }
    let mut gathered: Vec<(String, Where)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for contract in items(merged.get("contracts")) {
        let raw = non_empty(contract.get("titleLocKey")).or_else(|| non_empty(contract.get("titleKey")));
        let Some(key) = raw.map(|raw| reputation_key(&raw)).filter(|key| !key.is_empty()) else { continue };
        let at = *index.entry(key.clone()).or_insert_with(|| {
            gathered.push((key, Where::default()));
            gathered.len() - 1
        });
        let slot = &mut gathered[at].1;
        slot.systems.extend(elements(contract.get("systems")));
        slot.pyro.extend(elements(contract.get("pyroRegion")));
        for place in items(contract.get("locations")).iter().filter_map(Json::as_str) {
            let pool = pools.and_then(|pools| pools.get(place));
            if let (Some(planet), Some(system)) =
                (text(pool.and_then(|p| p.get("planet"))), text(pool.and_then(|p| p.get("system"))))
            {
                slot.planets.entry(system).or_default().insert(planet);
            }
        }
    }
    let mut out = Map::new();
    for (key, slot) in gathered {
        let mut parts = Vec::new();
        for system in &slot.systems {
            let mut planets = slot.planets.get(system).cloned().unwrap_or_default();
            if system == "Pyro" && !slot.pyro.is_empty() && slot.pyro.len() < pyro.len() {
                for letter in &slot.pyro {
                    planets.extend(pyro.get(letter).into_iter().flatten().cloned());
                }
            } else if system == "Pyro" && slot.pyro.len() >= pyro.len() && !pyro.is_empty() {
                planets.clear();
            }
            if everywhere.get(system).is_some_and(|all| !all.is_empty() && all.is_subset(&planets)) {
                planets.clear();
            }
            if !planets.is_empty() && planets.len() <= MAX_PLANETS {
                parts.push(format!("{system} ({})", planets.into_iter().collect::<Vec<_>>().join(", ")));
            } else {
                parts.push(system.clone());
            }
        }
        if !parts.is_empty() {
            out.push((key, Value::Str(parts.join(", "))));
        }
    }
    out
}

// -------------------------------------------------------------------- fingerprint

/// The sections of the error list the fingerprint covers.
const SECTIONS: [&str; 4] = ["blueprintRecords", "blueprintPools", "entities", "contracts"];

/// `cig_issues.fingerprint`: the first twelve hex digits of the SHA-1 over
/// the entries of the error list (`cig_data_issues-<build>.json` as the
/// 4.12 file `cig-datenfehler.json` keeps it, its part `daten`), `''` when
/// there are none.
///
/// The entries are those of the four sections without the keys that start
/// with `_` and the ones that are no objects; the keys of every section but
/// the contracts in lower case. They are written like Python's
/// `json.dumps(..., sort_keys=True, ensure_ascii=False)`.
pub fn fingerprint(issues: &Value) -> String {
    let mut sections = Map::new();
    let mut any = false;
    for section in SECTIONS {
        let mut entries = Map::new();
        if let Some(Value::Object(block)) = issues.get(section) {
            for (key, entry) in block.iter().filter(|(k, v)| !k.starts_with('_') && matches!(v, Value::Object(_))) {
                let key = if section == "contracts" { key.clone() } else { key.to_lowercase() };
                jsonio::map_set(&mut entries, &key, entry.clone());
            }
        }
        any |= !entries.is_empty();
        sections.push((section.to_string(), Value::Object(entries)));
    }
    if !any {
        return String::new();
    }
    let text = jsonio::dumps(&Value::Object(sections), None, true);
    sha1_hex(text.as_bytes())[..12].to_string()
}

/// The SHA-1 of `data` as 40 hex digits; lives in the core now (the hangar sync forms its identifiers with it too).
pub use versekit_core::text::sha1_hex;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::corrections::{Field, Section};

    /// A contract of the text `key`; `pools` are (pool, chance).
    fn contract(key: &str, reputation: i64, reward: i64, pools: &[(&str, Json)]) -> Json {
        let rewards: Vec<Json> =
            pools.iter().map(|(pool, chance)| json!({"blueprintPool": pool, "chance": chance})).collect();
        json!({
            "debugName": format!("Debug_{key}_{reputation}"),
            "titleLocKey": format!("{key}_title"),
            "descriptionLocKey": format!("{key}_desc"),
            "factionGuid": "F1",
            "factionRewardsIndex": 0,
            "rewardUEC": reward,
            "minStanding": {"name": format!("Rank {reputation}"), "minReputation": reputation},
            "maxStanding": {"name": "Top", "minReputation": 95250},
            "canBeShared": true,
            "hasPersonalCooldown": false,
            "personalCooldownTime": 45,
            "blueprintRewards": rewards,
        })
    }

    fn with(mut contract: Json, key: &str, value: Json) -> Json {
        contract[key] = value;
        contract
    }

    fn merged(contracts: Vec<Json>) -> Json {
        json!({
            "version": "t-1",
            "scopes": {"s1": {"displayName": "Standing"}, "s2": {"displayName": "Affinity"}},
            "factions": {"f1": {"name": "Alpha"}, "f2": {"name": "Beta"}},
            "factionRewardsPools": [
                [{"factionGuid": "f1", "scopeGuid": "s1", "amount": 50}, {"factionGuid": "f2", "scopeGuid": "s2", "amount": 25}],
                [{"factionGuid": "f1", "scopeGuid": "s1", "amount": 0}],
                [],
            ],
            "blueprintPools": {
                "P1": {"name": "BP_A", "blueprints": [
                    {"blueprintRecord": "B1", "name": "Gun A"}, {"blueprintRecord": "B2", "name": "Gun B"}]},
                "P2": {"name": "BP_B", "blueprints": [{"blueprintRecord": "B3", "name": "Gun C"}]},
                "P3": {"name": "BP_C", "blueprints": [{"blueprintRecord": "B4", "name": "Gun D"}]},
                "P4": {"name": "BP_D", "blueprints": [{"blueprintRecord": "B5", "name": "Gun E"}]},
            },
            "locationPools": {
                "l1": {"planet": "Hurston", "system": "Stanton"}, "l2": {"planet": "Crusader", "system": "Stanton"},
                "l3": {"planet": "microTech", "system": "Stanton"}, "l4": {"planet": "ArcCorp", "system": "Stanton"},
                "l5": {"planet": "Nyx I", "system": "Nyx"}, "l6": {"planet": "Nyx II", "system": "Nyx"},
                "l7": {"planet": "Pyro IV", "system": "Pyro"}, "l8": {"system": "Pyro"},
            },
            "pyroRegions": {
                "A": {"locations": [{"navIcon": "Planet", "name": "Pyro I"}, {"navIcon": "Moon", "name": "Ignis"}]},
                "B": {"locations": [{"navIcon": "Planet", "name": "Bloom"}]},
                "C": {"locations": [{"navIcon": "Planet", "name": "Terminus"}]},
                "D": {"locations": []},
            },
            "contracts": contracts,
            "legacyContracts": [],
        })
    }

    fn one(map: &Map, key: &str) -> Value {
        map.iter().find(|(k, _)| k == key).unwrap_or_else(|| panic!("no {key}")).1.clone()
    }

    fn keys(value: &Value) -> Vec<&str> {
        value.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect()
    }

    fn texts(value: &Value) -> Vec<&str> {
        match value {
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            _ => panic!("no list"),
        }
    }

    fn no_issues() -> Vec<Correction> {
        Vec::new()
    }

    #[test]
    fn variants_with_one_key_are_one_mission_in_the_order_they_first_occur() {
        let all = merged(vec![
            contract("zeta", 0, 100, &[("P1", json!(0.25))]),
            contract("alpha", 300, 50, &[("P3", json!(1))]),
            contract("zeta", 5000, 900, &[("P2", json!(1))]),
            // A level that gives nothing.
            contract("zeta", 0, 1, &[]),
        ]);
        let found = missions(&all, "t-1", &no_issues());
        let order: Vec<&str> = found.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(order, ["zeta_title", "alpha_title"]);

        let zeta = one(&found, "zeta_title");
        // The head comes from the easiest variant that gives blueprints, the
        // blueprints from all, by code point.
        assert_eq!(texts(&one(zeta.as_object().unwrap(), "bp")), ["Gun A", "Gun B", "Gun C"]);
        assert_eq!(zeta.get("sicher"), Some(&Value::Bool(true)));
        assert_eq!(zeta.get("chance"), Some(&Value::int(1)));
        assert_eq!((zeta.get("rep"), zeta.get("uec")), (Some(&Value::int(0)), Some(&Value::int(100))));
        assert_eq!((zeta.get("rep_max"), zeta.get("rang_max")), (Some(&Value::int(95250)), Some(&Value::str("Top"))));
        // 50 + 25 of the reward list; false and 0 stay.
        assert_eq!(zeta.get("ruf"), Some(&Value::int(75)));
        assert_eq!(zeta.get("teilbar"), Some(&Value::Bool(true)));
        assert_eq!(zeta.get("cooldown"), None);
        // Two reputations among the levels with blueprints: no title value.
        assert_eq!(zeta.get("rep_eindeutig"), None);
        // One level of three gives nothing.
        assert_eq!((zeta.get("leer"), zeta.get("stufen")), (Some(&Value::int(1)), Some(&Value::int(3))));
        assert_eq!(
            (zeta.get("titel_key"), zeta.get("text_key")),
            (Some(&Value::str("zeta_title")), Some(&Value::str("zeta_desc")))
        );

        let alpha = one(&found, "alpha_title");
        assert_eq!(alpha.get("chance"), Some(&Value::int(1)));
        assert_eq!(alpha.get("rep_eindeutig"), Some(&Value::Bool(true)));
        assert_eq!((alpha.get("leer"), alpha.get("stufen")), (None, None));
        assert_eq!(
            keys(&alpha),
            [
                "bp",
                "sicher",
                "chance",
                "rep",
                "rang",
                "rep_max",
                "rang_max",
                "uec",
                "ruf",
                "teilbar",
                "rep_eindeutig",
                "titel_key",
                "text_key"
            ]
        );
    }

    #[test]
    fn an_empty_field_is_left_out_and_a_number_keeps_its_form() {
        let mut bare = contract("bare", 0, 0, &[("P1", json!(0.5)), ("P2", json!(1.0))]);
        bare["rewardUEC"] = Json::Null;
        bare["minStanding"] = Json::Null;
        bare["maxStanding"] = json!({"minReputation": 7});
        bare["canBeShared"] = json!(false);
        bare["hasPersonalCooldown"] = json!(true);
        bare["personalCooldownTime"] = json!(0);
        bare["factionRewardsIndex"] = json!(1);
        let found = missions(&merged(vec![bare]), "t-1", &no_issues());
        let mission = one(&found, "bare_title");
        // `1.0` stays a float; the first of the greatest is taken.
        assert_eq!(mission.get("chance"), Some(&Value::Float(1.0)));
        assert_eq!(mission.get("sicher"), Some(&Value::Bool(true)));
        assert_eq!(
            keys(&mission),
            [
                "bp",
                "sicher",
                "chance",
                "rep_max",
                "ruf",
                "teilbar",
                "cooldown",
                "rep_eindeutig",
                "titel_key",
                "text_key"
            ]
        );
        assert_eq!(mission.get("rep_max"), Some(&Value::int(7)));
        assert_eq!(mission.get("ruf"), Some(&Value::int(0)));
        assert_eq!(mission.get("teilbar"), Some(&Value::Bool(false)));
        assert_eq!(mission.get("cooldown"), Some(&Value::int(0)));
        // No blueprint gives anything sure when the chance is not 1.
        let uncertain = missions(&merged(vec![contract("u", 0, 1, &[("P1", json!(0.25))])]), "t-1", &no_issues());
        let mission = one(&uncertain, "u_title");
        assert_eq!(
            (mission.get("sicher"), mission.get("chance")),
            (Some(&Value::Bool(false)), Some(&Value::Float(0.25)))
        );
        // The text key stands in for the title when there is none.
        let mut untitled = contract("d", 0, 1, &[("P1", json!(1))]);
        untitled["titleLocKey"] = Json::Null;
        let found = missions(&merged(vec![untitled]), "t-1", &no_issues());
        let mission = one(&found, "d_desc");
        assert_eq!((mission.get("titel_key"), mission.get("text_key")), (None, Some(&Value::str("d_desc"))));
    }

    #[test]
    fn the_rank_a_blueprint_needs_stands_only_where_it_tells_blueprints_apart() {
        // Gun A and Gun B from rank 800, Gun D only from 5000.
        let different = merged(vec![
            contract("mm", 800, 10, &[("P1", json!(1))]),
            contract("mm", 5000, 10, &[("P1", json!(1)), ("P3", json!(1))]),
        ]);
        let mission = one(&missions(&different, "t-1", &no_issues()), "mm_title");
        let ab = mission.get("ab").expect("ab");
        assert_eq!(keys(ab), ["Gun A", "Gun B", "Gun D"]);
        assert_eq!(ab.get("Gun D").and_then(|rank| rank.get("rang")), Some(&Value::str("Rank 5000")));
        assert_eq!(ab.get("Gun A").and_then(|rank| rank.get("rep")), Some(&Value::int(800)));

        // The levels differ, but every blueprint is there from the same rank.
        let same =
            merged(vec![contract("mm", 800, 10, &[("P1", json!(1))]), contract("mm", 800, 20, &[("P1", json!(1))])]);
        let mission = one(&missions(&same, "t-1", &no_issues()), "mm_title");
        assert_eq!((mission.get("ab"), mission.get("rep_eindeutig")), (None, Some(&Value::Bool(true))));

        // The levels differ, but the easiest one starts at reputation 0, which
        // names no rank: nothing to tell apart.
        let one_level =
            merged(vec![contract("mm", 0, 10, &[("P1", json!(1))]), contract("mm", 800, 10, &[("P1", json!(1))])]);
        let mission = one(&missions(&one_level, "t-1", &no_issues()), "mm_title");
        assert_eq!(mission.get("ab"), None);
    }

    #[test]
    fn the_blueprints_per_system_come_in_the_order_stanton_pyro_nyx_then_the_rest() {
        let at = |system: &str, pool: &str| {
            with(contract("s", 0, 1, &[(pool, json!(1))]), "availableSystems", json!([system]))
        };
        let all =
            merged(vec![at("Zeta", "P2"), at("Nyx", "P1"), at("Pyro", "P3"), at("Stanton", "P4"), at("Alpha", "P4")]);
        let mission = one(&missions(&all, "t-1", &no_issues()), "s_title");
        let systems = mission.get("je_system").expect("je_system");
        assert_eq!(keys(systems), ["Stanton", "Pyro", "Nyx", "Alpha", "Zeta"]);
        assert_eq!(texts(systems.get("Nyx").unwrap()), ["Gun A", "Gun B"]);
        assert_eq!(texts(systems.get("Alpha").unwrap()), ["Gun E"]);

        // `systems` stands in for `availableSystems` when that is empty.
        let mut legacy = at("Nyx", "P1");
        legacy["availableSystems"] = json!([]);
        legacy["systems"] = json!(["Pyro"]);
        let mixed = merged(vec![legacy, at("Stanton", "P2")]);
        let mission = one(&missions(&mixed, "t-1", &no_issues()), "s_title");
        assert_eq!(keys(mission.get("je_system").unwrap()), ["Stanton", "Pyro"]);

        // Nothing when a variant names no system, when one system occurs, or
        // when every system gives the same list.
        let nameless = with(contract("s", 0, 1, &[("P1", json!(1))]), "availableSystems", Json::Null);
        for list in [
            vec![at("Nyx", "P1"), nameless],
            vec![at("Nyx", "P1"), at("Nyx", "P2")],
            vec![at("Nyx", "P1"), at("Pyro", "P1")],
        ] {
            let mission = one(&missions(&merged(list), "t-1", &no_issues()), "s_title");
            assert_eq!(mission.get("je_system"), None);
        }
    }

    #[test]
    fn a_correction_of_the_pool_works_on_the_missions() {
        let mut all = merged(vec![contract("mm", 0, 1, &[("P1", json!(1))])]);
        all["blueprintPools"]["P1"]["blueprints"][1]["name"] = json!("Gun A");
        all["blueprintPools"]["P1"]["blueprints"][1]["blueprintRecord"] = json!("B1");
        let field = Field::PoolEntry { blueprint: "b1".into(), index: None };
        let fix = Correction {
            section: Section::Pool,
            record: "p1".into(),
            field,
            raw: None,
            expected: None,
            missing_target: false,
            reason: String::new(),
            build: None,
            source: "test",
        };
        // The second reference to Gun A is dropped; a twin stays one name.
        let found = missions(&all, "t-1", &[fix]);
        assert_eq!(texts(&one(one(&found, "mm_title").as_object().unwrap(), "bp")), ["Gun A"]);
    }

    fn faction_fix() -> Correction {
        Correction {
            section: Section::Contract,
            record: "Debug_mm_0".into(),
            field: Field::Faction,
            raw: None,
            expected: Some("f2".into()),
            missing_target: false,
            reason: "wrong faction".into(),
            build: None,
            source: "test",
        }
    }

    #[test]
    fn the_reputation_goes_to_every_party_of_a_contract() {
        let mut all = merged(vec![
            contract("mm", 0, 1, &[("P1", json!(1))]),
            // Index out of range, an index that is no integer, and a pool
            // that gives nothing: no entry.
            with(contract("far", 0, 1, &[]), "factionRewardsIndex", json!(9)),
            with(contract("float", 0, 1, &[]), "factionRewardsIndex", json!(1.0)),
            with(contract("zero", 0, 1, &[]), "factionRewardsIndex", json!(1)),
            with(contract("none", 0, 1, &[]), "factionRewardsIndex", json!(2)),
        ]);
        all["contracts"][0]["titleLocKey"] = json!("@Some_Title_001");
        let table = reputation(&all, "t-9", &no_issues(), "abc");
        assert_eq!((table.version.as_str(), table.fingerprint.as_str()), ("t-1", "abc"));
        assert_eq!(keys(&Value::Object(table.contracts.clone())), ["some_title_001"]);
        let rows = one(&table.contracts, "some_title_001");
        let text = jsonio::dumps(&rows, None, false);
        assert_eq!(
            text,
            r#"[{"wer": "Alpha", "was": "Standing", "wieviel": 50}, {"wer": "Beta", "was": "Affinity", "wieviel": 25}]"#
        );
        // The version falls back on the build.
        all["version"] = Json::Null;
        assert_eq!(reputation(&all, "t-9", &no_issues(), "").version, "t-9");
    }

    #[test]
    fn a_party_without_a_name_or_kind_is_left_out_and_one_with_half_of_it_stays() {
        let mut all = merged(vec![contract("mm", 0, 1, &[("P1", json!(1))])]);
        all["factionRewardsPools"][0] = json!([
            {"factionGuid": "unknown", "scopeGuid": "unknown", "amount": 5},
            {"factionGuid": "f1", "scopeGuid": "unknown", "amount": 6},
            {"factionGuid": "unknown", "scopeGuid": "s1", "amount": 7},
            {"factionGuid": "f1", "scopeGuid": "s1", "amount": 1.5},
        ]);
        let rows = one(&reputation(&all, "t", &no_issues(), "").contracts, "mm_title");
        assert_eq!(
            jsonio::dumps(&rows, None, false),
            r#"[{"wer": "Alpha", "was": "", "wieviel": 6}, {"wer": "", "was": "Standing", "wieviel": 7}, {"wer": "Alpha", "was": "Standing", "wieviel": 1.5}]"#
        );
    }

    #[test]
    fn the_correction_of_a_faction_moves_the_reputation_and_notes_the_old_one() {
        // The contract `Debug_mm_0` counts toward Alpha (`F1`) in the file; the
        // list says Beta. The reputation raw credited to Alpha is Beta's now.
        let all = merged(vec![contract("mm", 0, 1, &[("P1", json!(1))]), contract("n", 0, 1, &[("P1", json!(1))])]);
        let table = reputation(&all, "t", &[faction_fix()], "");
        assert_eq!(
            jsonio::dumps(&one(&table.contracts, "mm_title"), None, false),
            concat!(
                r#"[{"wer": "Beta", "was": "Standing", "wieviel": 50, "datenfehler": {"statt": "Alpha", "note": "wrong faction"}}, "#,
                r#"{"wer": "Beta", "was": "Affinity", "wieviel": 25}]"#
            )
        );
        // Another contract is not touched, and neither is one the list
        // names for the faction it has already.
        assert!(jsonio::dumps(&one(&table.contracts, "n_title"), None, false).contains("Alpha"));
        let mut right = faction_fix();
        right.expected = Some("F1".into());
        let table = reputation(&all, "t", &[right], "");
        assert!(!jsonio::dumps(&Value::Object(table.contracts), None, false).contains("datenfehler"));
    }

    #[test]
    fn the_region_names_the_planets_and_the_pyro_regions() {
        let with_places = |key: &str, system: &str, places: &[&str], pyro: Json| {
            let mut c = contract(key, 0, 1, &[]);
            c["systems"] = json!([system]);
            c["locations"] = json!(places);
            c["pyroRegion"] = pyro;
            c
        };
        let all = merged(vec![
            // Two variants of one text: the planets are joined.
            with_places("a", "Stanton", &["l2"], Json::Null),
            with_places("a", "Stanton", &["l1", "none"], Json::Null),
            // Every planet of the system: only the system.
            with_places("all", "Stanton", &["l1", "l2", "l3", "l4"], Json::Null),
            // Pyro: the planets of the regions it names.
            with_places("p", "Pyro", &[], json!(["A", "B"])),
            // All four regions: only the system.
            with_places("pall", "Pyro", &["l7"], json!(["A", "B", "C", "D"])),
            // A region without planets and a place without a planet.
            with_places("q", "Pyro", &["l8"], json!(["D"])),
            // Several systems, sorted, and no system at all.
            with_places("two", "Nyx", &["l5"], Json::Null),
            with_places("none", "", &[], Json::Null),
        ]);
        let mut all = all;
        all["contracts"][6]["systems"] = json!(["Stanton", "Nyx"]);
        all["contracts"][6]["locations"] = json!(["l5", "l1"]);
        let table = reputation(&all, "t", &no_issues(), "");
        let regions: Vec<(&str, &str)> = table.regions.iter().map(|(k, v)| (k.as_str(), v.as_str().unwrap())).collect();
        assert_eq!(
            regions,
            [
                ("a_title", "Stanton (Crusader, Hurston)"),
                ("all_title", "Stanton"),
                ("p_title", "Pyro (Bloom, Pyro I)"),
                ("pall_title", "Pyro"),
                ("q_title", "Pyro"),
                ("two_title", "Nyx (Nyx I), Stanton (Hurston)"),
            ]
        );
        // More than four planets: only the system.
        let mut many = merged(vec![]);
        many["locationPools"]["l9"] = json!({"planet": "Extra", "system": "Stanton"});
        many["contracts"] = json!([with_places("many", "Stanton", &["l1", "l2", "l3", "l9"], Json::Null)]);
        many["contracts"][0]["locations"] = json!(["l1", "l2", "l3", "l9"]);
        assert_eq!(
            one(&reputation(&many, "t", &no_issues(), "").regions, "many_title"),
            Value::str("Stanton (Crusader, Extra, Hurston, microTech)")
        );
        many["locationPools"]["l10"] = json!({"planet": "Sixth", "system": "Stanton"});
        many["contracts"][0]["locations"] = json!(["l1", "l2", "l3", "l9", "l10"]);
        assert_eq!(one(&reputation(&many, "t", &no_issues(), "").regions, "many_title"), Value::str("Stanton"));
    }

    #[test]
    fn sha1_gives_the_known_sums() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        let long = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha1_hex(long.as_bytes()), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
        assert_eq!(sha1_hex(&[b'a'; 1000]), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
    }

    #[test]
    fn the_fingerprint_follows_the_entries_of_four_sections() {
        let list = jsonio::loads(
            r#"{"_comment": "x", "blueprintRecords": {"_comment": "c", "AbC-1": {"expected": {"name": "Äb"}, "note": "n"}, "skip": 3},
                "blueprintPools": {}, "entities": {}, "other": {"a": {"b": 1}},
                "contracts": {"Debug_Name": {"field": "factionGuid", "expected": {"factionGuid": "F2", "factionName": "Beta"}, "note": "n"}}}"#,
        )
        .unwrap();
        assert_eq!(fingerprint(&list), FINGERPRINT);
        // The order of the keys, the case of a GUID, comments, entries that
        // are no objects and other sections change nothing.
        let shuffled = jsonio::loads(
            r#"{"contracts": {"Debug_Name": {"note": "n", "expected": {"factionName": "Beta", "factionGuid": "F2"}, "field": "factionGuid"}},
                "blueprintRecords": {"abc-1": {"note": "n", "expected": {"name": "Äb"}}, "_x": {"a": 1}}}"#,
        )
        .unwrap();
        assert_eq!(fingerprint(&shuffled), FINGERPRINT);
        // The case of a debug name does matter.
        let other = jsonio::loads(r#"{"contracts": {"debug_name": {"field": "factionGuid"}}}"#).unwrap();
        assert_ne!(fingerprint(&other), FINGERPRINT);
        // Nothing to note: nothing.
        for empty in ["{}", r#"{"_comment": "x", "contracts": {"_c": {"a": 1}, "b": 2}}"#, "[]", "null"] {
            assert_eq!(fingerprint(&jsonio::loads(empty).unwrap()), "", "{empty}");
        }
    }

    /// Computed with `cig_issues.fingerprint` of 4.12 on the list above.
    const FINGERPRINT: &str = "718bfc04e83f";
}
