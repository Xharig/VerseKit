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

//! Whom a contract credits reputation, of which kind, read from the p4kx catalog of the player's `Data.p4k`: the
//! `reputationGains` of each contract entry with the English names of its faction (`factionReputations`) and scope
//! (`reputationScopes`). The rows have the form of `auftraege` in `auftragsruf.json`, as
//! [`crate::contract_info::reputation`] builds them from scmdb's file.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value as Json;
use versekit_core::injection::reputation::{key as reputation_key, merge_parties};
use versekit_core::injection::terms::Terms;
use versekit_core::jsonio::{self, Map, Value};

use crate::Offered;
use crate::scmdb_catalog::Fixes;
use crate::sources::{ShownKeys, entry_key};

/// The record path part that selects the faction reputation records.
pub(crate) const FACTION_RECORDS: &str = "libs/foundry/records/factions/factionreputation/";

/// The amounts of the reputation gains of every contract entry as the reward records state them, by
/// [`crate::sources::entry_key`] ([`crate::sources::reward_amounts`]): what fills the gains whose amount the p4kx
/// catalog does not have (`amount: null`).
pub type Amounts = HashMap<String, Vec<Option<i64>>>;

/// What the game data say of the contracts: whom each credits reputation ([`parties`]) and the terms of each title
/// (`versekit_core::injection::terms::summarize` over [`terms`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contracts {
    pub parties: Map,
    pub terms: Map,
    /// Where the contracts of each title play ([`regions`]); empty until the places of the entries are known.
    pub regions: Map,
}

/// [`Contracts`] from the catalogs as JSON: of the contract generators, the faction reputation records
/// ([`FACTION_RECORDS`]) and the reputation records (`reputation_ranks::REPUTATION_RECORDS`); `corrections` are the
/// known errors, applied when made for `build` or for every build.
///
/// # Errors
///
/// When one of them is no catalog JSON.
pub fn from_catalogs(
    contracts: &[u8],
    keys: &HashMap<String, ShownKeys>,
    factions: &[u8],
    reputation: &[u8],
    corrections: &[crate::corrections::Correction],
    build: &str,
    amounts: &Amounts,
) -> Result<Contracts, serde_json::Error> {
    let factions = names(&serde_json::from_slice(factions)?, "factionReputations");
    let reputation: Json = serde_json::from_slice(reputation)?;
    let scopes = names(&reputation, "reputationScopes");
    let contracts: Json = serde_json::from_slice(contracts)?;
    Ok(Contracts {
        parties: parties(&contracts, keys, &factions, &scopes, &Fixes::new(corrections, build), amounts),
        terms: versekit_core::injection::terms::summarize(&terms(&contracts, keys, &standings(&reputation), &factions)),
        regions: Map::new(),
    })
}

/// A region as [`regions`] writes it read back: system → its planets, empty for the whole system.
pub fn parse_region(region: &str) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut found = std::collections::BTreeMap::new();
    let mut rest = region.trim();
    while !rest.is_empty() {
        let (part, after) = match (rest.find(','), rest.find('(')) {
            (Some(comma), Some(open)) if open < comma => {
                let close = rest[open..].find(')').map_or(rest.len(), |at| open + at + 1);
                (&rest[..close], rest[close..].trim_start_matches(',').trim_start())
            }
            (Some(comma), _) => (&rest[..comma], rest[comma + 1..].trim_start()),
            (None, _) => (rest, ""),
        };
        let (system, planets) = match part.split_once('(') {
            Some((system, planets)) => (
                system.trim(),
                planets.trim_end_matches(')').split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
            ),
            None => (part.trim(), BTreeSet::new()),
        };
        if !system.is_empty() {
            found.insert(system.to_string(), planets);
        }
        rest = after;
    }
    found
}

/// `own` with what `theirs` adds (another region of the same title, e.g. scmdb's, which also names where a contract
/// leads): every system of either in sorted order; a system either names whole stays whole, else the planets of
/// both, the whole system above [`crate::contract_info::MAX_PLANETS`].
pub fn join_regions(own: &str, theirs: &str) -> String {
    let mut joined = parse_region(own);
    for (system, planets) in parse_region(theirs) {
        match joined.get_mut(&system) {
            Some(known) if known.is_empty() => {}
            Some(known) if planets.is_empty() => known.clear(),
            Some(known) => known.extend(planets),
            None => {
                joined.insert(system, planets);
            }
        }
    }
    joined
        .into_iter()
        .map(|(system, planets)| {
            if planets.is_empty() || planets.len() > crate::contract_info::MAX_PLANETS {
                system
            } else {
                format!("{system} ({})", planets.into_iter().collect::<Vec<_>>().join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `{key: 'Stanton (Crusader, microTech), Pyro'}` over the released contract entries of `contracts` (a catalog of the
/// contract generators), the key being the text key of each title the entry shows ([`shown`]) without `@` in lower
/// case, the places those of the star map ([`Offered`], by [`entry_key`]). As [`crate::contract_info`] reads them
/// from scmdb's file (`reputation.prepare_regions` of 4.12): the systems of all entries of a title in sorted order,
/// each with its planets in brackets; only the system where the title plays at every planet of it any contract is
/// offered at, or at more than [`crate::contract_info::MAX_PLANETS`].
pub fn regions(contracts: &Json, keys: &HashMap<String, ShownKeys>, offered: &HashMap<String, Offered>) -> Map {
    let mut everywhere: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    let mut gathered: Vec<(String, Offered)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for contract in entries(contracts) {
        let id = contract.get("guid").and_then(Json::as_str).unwrap_or_default();
        let sub = contract.get("subContractGuid").and_then(Json::as_str);
        let Some(places) = offered.get(&entry_key(id, sub)) else { continue };
        for (system, planets) in &places.planets {
            everywhere.entry(system).or_default().extend(planets.iter().map(String::as_str));
        }
        for title in shown(contract, keys).0.into_iter().map(reputation_key).filter(|key| !key.is_empty()) {
            let at = *index.entry(title.clone()).or_insert_with(|| {
                gathered.push((title, Offered::default()));
                gathered.len() - 1
            });
            let slot = &mut gathered[at].1;
            slot.systems.extend(places.systems.iter().cloned());
            for (system, planets) in &places.planets {
                slot.planets.entry(system.clone()).or_default().extend(planets.iter().cloned());
            }
        }
    }
    let mut out = Map::new();
    for (key, slot) in gathered {
        let parts: Vec<String> = slot
            .systems
            .iter()
            .map(|system| {
                let planets = slot.planets.get(system).cloned().unwrap_or_default();
                let all = everywhere.get(system.as_str());
                let at_every = all.is_some_and(|all| !all.is_empty() && all.iter().all(|p| planets.contains(*p)));
                if !at_every && !planets.is_empty() && planets.len() <= crate::contract_info::MAX_PLANETS {
                    format!("{system} ({})", planets.into_iter().collect::<Vec<_>>().join(", "))
                } else {
                    system.clone()
                }
            })
            .collect();
        if !parts.is_empty() {
            out.push((key, Value::Str(parts.join(", "))));
        }
    }
    out
}

/// GUID (lower case) → English name and `minReputation` of the standings (`reputationStandings`) of a catalog; a
/// standing without an English name goes by its record name.
pub fn standings(catalog: &Json) -> HashMap<String, (String, i64)> {
    let entries = catalog.get("reputationStandings").and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default();
    entries
        .iter()
        .filter_map(|entry| {
            let guid = entry.get("guid")?.as_str()?.to_ascii_lowercase();
            let english = entry.get("displayName").and_then(|d| d.get("en")).and_then(Json::as_str);
            let name = english.filter(|n| !n.is_empty()).or_else(|| entry.get("className")?.as_str())?;
            Some((guid, (name.to_string(), entry.get("minReputation")?.as_i64()?)))
        })
        .collect()
}

/// GUID (lower case) → English display name of the entries of `kind` in a catalog.
pub fn names(catalog: &Json, kind: &str) -> HashMap<String, String> {
    let entries = catalog.get(kind).and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default();
    entries
        .iter()
        .filter_map(|entry| {
            let guid = entry.get("guid")?.as_str()?.to_ascii_lowercase();
            let name = entry.get("displayName")?.get("en")?.as_str().filter(|name| !name.is_empty())?;
            Some((guid, name.to_string()))
        })
        .collect()
}

/// The released contract entries of a catalog of the contract generators: neither `notForRelease` nor
/// `workInProgress` is set, as for the blueprint catalog.
pub fn entries(contracts: &Json) -> impl Iterator<Item = &Json> {
    let entries = contracts.get("contracts").and_then(Json::as_array).map(Vec::as_slice).unwrap_or_default();
    entries.iter().filter(|entry| {
        ["notForRelease", "workInProgress"].iter().all(|flag| entry.get(*flag).and_then(Json::as_bool) != Some(true))
    })
}

/// The keys of the titles and the descriptions an entry shows: those of [`ShownKeys`] where the game puts another
/// text in place of the one it names (one of them, chosen by the game), else the one it names.
pub fn shown<'a>(contract: &'a Json, keys: &'a HashMap<String, ShownKeys>) -> (Vec<&'a str>, Vec<&'a str>) {
    let named = |field: &str| contract.get(field).and_then(|text| text.get("key")).and_then(Json::as_str);
    let id = contract.get("guid").and_then(Json::as_str).unwrap_or_default();
    let sub = contract.get("subContractGuid").and_then(Json::as_str);
    let found = keys.get(&entry_key(id, sub));
    let pick = |list: Option<&'a Vec<String>>, field: &str| -> Vec<&'a str> {
        match list.filter(|list| !list.is_empty()) {
            Some(list) => list.iter().map(String::as_str).collect(),
            None => named(field).into_iter().collect(),
        }
    };
    (pick(found.map(|k| &k.title), "title"), pick(found.map(|k| &k.description), "description"))
}

/// `{key: [{wer, was, wieviel}, ...]}` over the contract entries of `contracts` (a catalog of the contract
/// generators), the key being the text key of the title the entry shows ([`shown`]) without `@` in lower case. The
/// entries of one title are joined (`versekit_core::injection::reputation::merge_parties`). `factions` and `scopes` are
/// the names from [`names`]; `fixes` move the reputation of a contract whose faction the error list corrects, as for
/// scmdb's file.
pub(crate) fn parties(
    contracts: &Json,
    keys: &HashMap<String, ShownKeys>,
    factions: &HashMap<String, String>,
    scopes: &HashMap<String, String>,
    fixes: &Fixes,
    amounts: &Amounts,
) -> Map {
    let name = |table: &HashMap<String, String>, guid: Option<&str>| {
        guid.and_then(|guid| table.get(&guid.to_ascii_lowercase())).cloned()
    };
    let mut out: Vec<(String, Vec<Map>)> = Vec::new();
    for contract in entries(contracts) {
        let titles: Vec<String> =
            shown(contract, keys).0.into_iter().map(reputation_key).filter(|key| !key.is_empty()).collect();
        if titles.is_empty() {
            continue;
        }
        let debug_name = contract.get("debugName").and_then(Json::as_str).unwrap_or_default();
        let fix = fixes.faction_fix(debug_name);
        let own = giver(contract).unwrap_or_default().to_ascii_lowercase();
        let wrong = fix.filter(|(meant, _)| own != meant.to_ascii_lowercase()).map(|_| own);
        let mut rows = Vec::new();
        let gains: Vec<&Json> = contract.get("reputationGains").and_then(Json::as_array).into_iter().flatten().collect();
        // The amounts read from the reward records, where the catalog has none (p4kx reads only some folders).
        let read = contract
            .get("guid")
            .and_then(Json::as_str)
            .and_then(|id| amounts.get(&entry_key(id, contract.get("subContractGuid").and_then(Json::as_str))))
            .filter(|read| read.len() == gains.len());
        for (at, gain) in gains.into_iter().enumerate() {
            let amount = gain.get("amount").and_then(Json::as_i64).or_else(|| read.and_then(|read| read[at]));
            // A gain whose amount is nowhere to be read stays a row without an amount: it is shown as unknown, never
            // left out. An amount of zero is no gain.
            if amount == Some(0) {
                continue;
            }
            let guid = gain.get("factionReputationGuid").and_then(Json::as_str);
            let faction = name(factions, guid);
            let kind = name(scopes, gain.get("scopeGuid").and_then(Json::as_str));
            if faction.is_none() && kind.is_none() {
                continue;
            }
            let mut row: Map = vec![
                ("wer".into(), Value::Str(faction.clone().unwrap_or_default())),
                ("was".into(), Value::Str(kind.unwrap_or_default())),
                ("wieviel".into(), amount.map_or(Value::Null, Value::int)),
            ];
            if let (Some(wrong), Some((meant, note))) = (&wrong, fix)
                && guid.unwrap_or_default().to_ascii_lowercase() == *wrong
                && let Some(right) = name(factions, Some(meant))
            {
                jsonio::map_set(&mut row, "wer", Value::Str(right));
                let was: Map =
                    vec![("statt".into(), Value::Str(faction.unwrap_or_default())), ("note".into(), Value::str(note))];
                row.push(("datenfehler".into(), Value::Object(was)));
            }
            rows.push(row);
        }
        out.extend(titles.into_iter().map(|title| (title, rows.clone())));
    }
    merge_parties(&out)
}

/// The terms of every contract entry of `contracts` (a catalog of the contract generators) with the key of its title:
/// the cooldowns as stored (minutes, rounded), the buy-in, the fixed payout ([`fixed_reward`]), and the facts
/// `canBeShared`, `canReacceptAfterAbandoning` and `canReacceptAfterFailing`. The personal cooldown counts only where
/// `hasPersonalCooldown` is set.
/// `standings` are the ranks of the reputation catalog ([`standings`]) the contracts name as `minStandingGuid` and
/// `maxStandingGuid`; `factions` the English names of the faction reputation records ([`names`]), for the faction
/// that gives the contract ([`giver_name`]).
pub fn terms(
    contracts: &Json,
    keys: &HashMap<String, ShownKeys>,
    standings: &HashMap<String, (String, i64)>,
    factions: &HashMap<String, String>,
) -> Vec<(String, Terms)> {
    let rank = |contract: &Json, field: &str| {
        contract.get(field).and_then(Json::as_str).and_then(|guid| standings.get(&guid.to_ascii_lowercase())).cloned()
    };
    let whole = |value: Option<&Json>| value.and_then(Json::as_f64).filter(|x| x.is_finite()).map(|x| x.round() as i64);
    let fact =
        |contract: &Json, name: &str| contract.get(name).and_then(|fact| fact.get("value")).and_then(Json::as_bool);
    entries(contracts)
        .flat_map(|contract| {
            let (titles, descriptions) = shown(contract, keys);
            let personal = contract.get("hasPersonalCooldown").and_then(Json::as_bool) == Some(true);
            let terms = Terms {
                cooldown: personal.then(|| whole(contract.get("personalCooldownTime"))).flatten(),
                cooldown_spread: personal.then(|| whole(contract.get("personalCooldownTimeVariation"))).flatten(),
                abandoned: whole(contract.get("abandonedCooldownTime")),
                abandoned_spread: whole(contract.get("abandonedCooldownTimeVariation")),
                buy_in: whole(contract.get("buyInAmount")),
                shareable: fact(contract, "canBeShared"),
                again_abandoned: fact(contract, "canReacceptAfterAbandoning"),
                again_failed: fact(contract, "canReacceptAfterFailing"),
                description: None,
                lowest: rank(contract, "minStandingGuid"),
                highest: rank(contract, "maxStandingGuid"),
                reward: fixed_reward(contract),
                reward_base: None,
                giver: giver_name(contract, factions),
            };
            // Every title the game may show, with every description it may show.
            let descriptions: Vec<Option<String>> = if descriptions.is_empty() {
                vec![None]
            } else {
                descriptions.into_iter().map(|d| Some(d.to_string())).collect()
            };
            titles
                .into_iter()
                .flat_map(|title| {
                    let terms = &terms;
                    descriptions.iter().map(move |description| {
                        (title.to_string(), Terms { description: description.clone(), ..terms.clone() })
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The payout in aUEC a contract states as a fixed amount (`fixedRewards` of the catalog, the `ContractResult_Reward`
/// results that fire on success, currency `UEC`), the sum when it states several; `None` for a payout the game
/// computes, which the data give no amount for.
fn fixed_reward(contract: &Json) -> Option<i64> {
    let amounts: Vec<i64> = contract
        .get("fixedRewards")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter(|reward| reward.get("currency").and_then(Json::as_str).is_some_and(|c| c.eq_ignore_ascii_case("UEC")))
        .filter_map(|reward| reward.get("amount").and_then(Json::as_i64))
        .collect();
    (!amounts.is_empty()).then(|| amounts.iter().sum())
}

/// The English name of the faction that gives a contract (`giverFaction` of the catalog): the name of its faction
/// reputation record in `factions`, else the English display name the entry carries; `None` where the catalog names
/// no giver or the giver has no English name.
fn giver_name(contract: &Json, factions: &HashMap<String, String>) -> Option<String> {
    let faction = contract.get("giverFaction").filter(|giver| !giver.is_null())?.get("faction")?;
    let by_record = faction.get("guid").and_then(Json::as_str).and_then(|guid| factions.get(&guid.to_ascii_lowercase()));
    let shown = || faction.get("displayName")?.get("en")?.as_str().filter(|name| !name.is_empty()).map(str::to_string);
    by_record.cloned().or_else(shown)
}

/// The faction a correction of the contract's faction looks at: the one its first positive reputation gain is
/// credited to (where the error lists find the wrong faction), else its handler's, else the one the catalog names.
fn giver(contract: &Json) -> Option<&str> {
    let of = |field: &str| contract.get(field).filter(|value| !value.is_null());
    let gained = of("reputationGains")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .find(|gain| gain.get("amount").and_then(Json::as_i64).is_some_and(|amount| amount > 0))
        .and_then(|gain| gain.get("factionReputationGuid"));
    gained
        .or_else(|| of("faction").and_then(|faction| faction.get("guid")))
        .or_else(|| of("giverFaction").and_then(|giver| giver.get("faction")).and_then(|faction| faction.get("guid")))
        .and_then(Json::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::corrections::{Correction, Field, Section};

    fn catalog(contracts: Json) -> Json {
        json!({"contracts": contracts})
    }

    fn entry(debug: &str, title: &str, gains: Json) -> Json {
        json!({
            "debugName": debug,
            "title": {"key": title, "en": "x", "de": null},
            "faction": {"guid": "F1", "name": "f1", "displayName": null},
            "reputationGains": gains,
        })
    }

    fn gain(faction: &str, scope: &str, amount: i64) -> Json {
        json!({"factionReputationGuid": faction, "factionReputationName": "x", "scopeGuid": scope, "scopeName": "y", "amount": amount})
    }

    fn tables() -> (HashMap<String, String>, HashMap<String, String>) {
        let factions = names(
            &json!({"factionReputations": [
                {"guid": "f1", "displayName": {"key": "@a", "en": "Alpha", "de": null}},
                {"guid": "F2", "displayName": {"key": "@b", "en": "Beta", "de": null}},
                {"guid": "f3", "displayName": {"key": "@c", "en": null, "de": null}}]}),
            "factionReputations",
        );
        let scopes = names(
            &json!({"reputationScopes": [{"guid": "s1", "displayName": {"key": "@s", "en": "Standing", "de": null}}]}),
            "reputationScopes",
        );
        (factions, scopes)
    }

    #[test]
    fn the_gains_become_rows_by_title_key() {
        let (factions, scopes) = tables();
        let all = catalog(json!([
            entry(
                "A",
                "@Some_Title_001",
                json!([gain("F1", "S1", 50), gain("f2", "unknown", 25), gain("f3", "s9", 7), gain("f1", "s1", 0)])
            ),
            // No title, no gain: nothing.
            entry("B", "", json!([gain("f1", "s1", 5)])),
            entry("C", "@Other", json!([])),
            // The entries of one title are joined.
            entry("D", "@Twice", json!([gain("f1", "s1", 1)])),
            entry("E", "@twice", json!([gain("f2", "s1", 2), gain("f1", "s1", 4)])),
        ]));
        let found = parties(&all, &HashMap::new(), &factions, &scopes, &Fixes::new(&[], "t"), &Amounts::new());
        assert_eq!(
            jsonio::dumps(&Value::Object(found), None, false),
            concat!(
                r#"{"some_title_001": [{"wer": "Alpha", "was": "Standing", "wieviel": 50}, {"wer": "Beta", "was": "", "wieviel": 25}], "#,
                r#""twice": [{"wer": "Alpha", "was": "Standing", "wieviel": [1, 4]}]}"#
            )
        );
    }

    /// A gain whose amount the catalog does not have (`amount: null`, the reward record is in a folder p4kx does not
    /// read) takes the amount read from the reward record; where that is not known either, the row stays, without an
    /// amount, and is never left out.
    #[test]
    fn a_gain_without_an_amount_takes_the_one_of_its_reward_record_or_stays_unknown() {
        let (factions, scopes) = tables();
        let unset = |faction: &str| {
            json!({"factionReputationGuid": faction, "factionReputationName": "x", "scopeGuid": "s1", "scopeName": "y", "amount": null})
        };
        let mut read = entry("A", "@read", json!([unset("f1"), gain("f2", "s1", 7)]));
        read["guid"] = json!("AAAA");
        let mut unread = entry("B", "@unread", json!([unset("f1")]));
        unread["guid"] = json!("BBBB");
        let mut other = entry("C", "@other", json!([unset("f1"), unset("f2")]));
        other["guid"] = json!("CCCC");
        let mut sub = entry("D", "@sub", json!([unset("f2")]));
        sub["guid"] = json!("DDDD");
        sub["subContractGuid"] = json!("EEEE");
        let amounts: Amounts = [
            (entry_key("aaaa", None), vec![Some(200), Some(7)]),
            (entry_key("bbbb", None), vec![None]),
            // Two gains in the catalog, one read: the order cannot be trusted, nothing is taken over.
            (entry_key("cccc", None), vec![Some(5)]),
            (entry_key("dddd", Some("eeee")), vec![Some(100)]),
        ]
        .into_iter()
        .collect();
        let all = catalog(json!([read, unread, other, sub]));
        let found = parties(&all, &HashMap::new(), &factions, &scopes, &Fixes::new(&[], "t"), &amounts);
        assert_eq!(
            jsonio::dumps(&Value::Object(found), None, false),
            concat!(
                r#"{"read": [{"wer": "Alpha", "was": "Standing", "wieviel": 200}, {"wer": "Beta", "was": "Standing", "wieviel": 7}], "#,
                r#""unread": [{"wer": "Alpha", "was": "Standing", "wieviel": null}], "#,
                r#""other": [{"wer": "Alpha", "was": "Standing", "wieviel": null}, {"wer": "Beta", "was": "Standing", "wieviel": null}], "#,
                r#""sub": [{"wer": "Beta", "was": "Standing", "wieviel": 100}]}"#
            )
        );
        // What the catalog states is not touched by what was read.
        let mut stated = entry("E", "@stated", json!([gain("f1", "s1", 50)]));
        stated["guid"] = json!("FFFF");
        let wrong: Amounts = [(entry_key("ffff", None), vec![Some(999)])].into_iter().collect();
        let found = parties(&catalog(json!([stated])), &HashMap::new(), &factions, &scopes, &Fixes::new(&[], "t"), &wrong);
        assert_eq!(jsonio::dumps(&Value::Object(found), None, false), r#"{"stated": [{"wer": "Alpha", "was": "Standing", "wieviel": 50}]}"#);
    }

    /// A fixed payout in aUEC is the reward; one in another currency, or none (a computed payout), is no reward.
    #[test]
    fn only_a_fixed_payout_in_auec_is_the_reward() {
        let mut fixed = entry("A", "@fixed", json!([]));
        fixed["fixedRewards"] = json!([{"currency": "UEC", "amount": 134500, "max": 0, "plusBonuses": false}]);
        let mut merits = entry("B", "@merits", json!([]));
        merits["fixedRewards"] = json!([{"currency": "MER", "amount": 3000, "max": 5000, "plusBonuses": false}]);
        let mut computed = entry("C", "@computed", json!([]));
        computed["fixedRewards"] = json!([]);
        let found = terms(&catalog(json!([fixed, merits, computed])), &HashMap::new(), &HashMap::new(), &HashMap::new());
        let rewards: Vec<(&str, Option<i64>)> = found.iter().map(|(title, t)| (title.as_str(), t.reward)).collect();
        assert_eq!(rewards, [("@fixed", Some(134_500)), ("@merits", None), ("@computed", None)]);
    }

    /// The giver is the faction `giverFaction` names, by the name of its record, else by the name the entry carries;
    /// an entry without one, or with a giver without an English name, has none.
    #[test]
    fn the_giver_is_the_faction_the_catalog_names() {
        let (factions, _) = tables();
        let given = |debug: &str, title: &str, faction: Json| {
            let mut one = entry(debug, title, json!([]));
            one["giverFaction"] = faction;
            one
        };
        let all = catalog(json!([
            given("A", "@by_record", json!({"source": "handler", "faction": {"guid": "F2", "name": "b", "displayName": null}})),
            given(
                "B",
                "@by_name",
                json!({"source": "reputationResult", "faction": {"guid": "f9", "name": "z", "displayName": {"key": "@z", "en": "Zulu", "de": null}}})
            ),
            given("C", "@nameless", json!({"source": "handler", "faction": {"guid": "f3", "name": "c", "displayName": null}})),
            given("D", "@none", Json::Null),
            entry("E", "@missing", json!([])),
        ]));
        let found = terms(&all, &HashMap::new(), &HashMap::new(), &factions);
        let givers: Vec<(&str, Option<&str>)> = found.iter().map(|(title, t)| (title.as_str(), t.giver.as_deref())).collect();
        assert_eq!(
            givers,
            [("@by_record", Some("Beta")), ("@by_name", Some("Zulu")), ("@nameless", None), ("@none", None), ("@missing", None)]
        );
    }

    /// An entry whose title the game takes from the contractor goes by the key it shows.
    #[test]
    fn an_entry_goes_by_the_title_it_shows() {
        let (factions, scopes) = tables();
        let mut one = entry("H", "@blackbox_recover_title_E_001", json!([gain("f1", "s1", 5)]));
        one["guid"] = json!("10000000-0000-4000-8000-000000000001");
        let keys: HashMap<String, ShownKeys> = [(
            entry_key("10000000-0000-4000-8000-000000000001", None),
            ShownKeys {
                title: vec!["@Hockrow_BlackBox_space_E_Title_001".into(), "@Hockrow_BlackBox_space_E_Title_002".into()],
                description: Vec::new(),
            },
        )]
        .into_iter()
        .collect();
        let found = parties(&catalog(json!([one])), &keys, &factions, &scopes, &Fixes::new(&[], "t"), &Amounts::new());
        let titles: Vec<&str> = found.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(titles, ["hockrow_blackbox_space_e_title_001", "hockrow_blackbox_space_e_title_002"]);
    }

    /// The regions read like scmdb's: every system of the title's entries in sorted order, its planets in brackets
    /// unless the title plays at every planet any contract of the system is offered at or at more than four.
    #[test]
    fn the_regions_name_the_systems_and_the_planets_of_a_title() {
        let titled = |guid: &str, title: &str| {
            let mut one = entry(guid, title, json!([]));
            one["guid"] = json!(guid);
            one
        };
        let places = |list: &[(&str, &[&str])]| {
            let mut offered = Offered::default();
            for (system, planets) in list {
                offered.systems.insert((*system).to_string());
                if !planets.is_empty() {
                    offered.planets.insert((*system).to_string(), planets.iter().map(|p| p.to_string()).collect());
                }
            }
            offered
        };
        let stanton = ["ArcCorp", "Crusader", "Hurston", "microTech"];
        let all = catalog(json!([
            titled("g1", "@Two_Title"),
            titled("g2", "@Every_Title"),
            titled("g3", "@Wide_Title"),
            titled("g4", "@Wide_Title"),
            titled("g5", "@recovery_title"),
            titled("g6", "@Nowhere_Title"),
        ]));
        let offered: HashMap<String, Offered> = [
            ("g1", places(&[("Stanton", &["microTech", "Crusader"])])),
            ("g2", places(&[("Stanton", &stanton)])),
            ("g3", places(&[("Stanton", &["Hurston"])])),
            ("g4", places(&[("Pyro", &[]), ("Nyx", &["Nyx I"])])),
            ("g5", places(&[("Stanton", &["Hurston"])])),
        ]
        .into_iter()
        .map(|(guid, places)| (entry_key(guid, None), places))
        .collect();
        let keys: HashMap<String, ShownKeys> = [(
            entry_key("g5", None),
            ShownKeys { title: vec!["@BitZeros_RecoverItem_Generic_Title_001".into()], description: Vec::new() },
        )]
        .into_iter()
        .collect();
        let found = regions(&all, &keys, &offered);
        assert_eq!(
            jsonio::dumps(&Value::Object(found), None, false),
            concat!(
                r#"{"two_title": "Stanton (Crusader, microTech)", "every_title": "Stanton", "#,
                // Nyx I is the only planet of Nyx any contract is offered at: the whole system.
                r#""wide_title": "Nyx, Pyro, Stanton (Hurston)", "#,
                r#""bitzeros_recoveritem_generic_title_001": "Stanton (Hurston)"}"#
            ),
            "a title without places has no region"
        );
    }

    #[test]
    fn a_region_reads_back_and_joins_another() {
        let parsed = parse_region("Nyx, Pyro (Bloom, Pyro IV), Stanton");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed["Pyro"].iter().collect::<Vec<_>>(), ["Bloom", "Pyro IV"]);
        assert!(parsed["Nyx"].is_empty() && parsed["Stanton"].is_empty());
        assert!(parse_region("").is_empty());
        assert_eq!(join_regions("Stanton (Hurston)", "Stanton (microTech), Nyx"), "Nyx, Stanton (Hurston, microTech)");
        assert_eq!(join_regions("Stanton (Hurston)", "Stanton"), "Stanton", "a whole system stays whole");
        assert_eq!(join_regions("Stanton", "Stanton (Hurston)"), "Stanton");
        assert_eq!(join_regions("Pyro (Bloom, Pyro I)", "Pyro (Monox, Pyro V, Pyro IV)"), "Pyro", "more than four");
        assert_eq!(join_regions("Pyro (Bloom)", ""), "Pyro (Bloom)");
    }

    #[test]
    fn the_correction_of_a_faction_moves_the_reputation_and_notes_the_old_one() {
        let (factions, scopes) = tables();
        let all = catalog(json!([entry("Debug_mm", "@mm", json!([gain("f1", "s1", 50), gain("f2", "s1", 3)]))]));
        let fix = Correction {
            section: Section::Contract,
            record: "debug_MM".into(),
            field: Field::Faction,
            raw: None,
            expected: Some("f2".into()),
            missing_target: false,
            reason: "wrong faction".into(),
            build: None,
            source: "test",
        };
        let found = parties(&all, &HashMap::new(), &factions, &scopes, &Fixes::new(&[fix], "t"), &Amounts::new());
        assert_eq!(
            jsonio::dumps(&Value::Object(found), None, false),
            concat!(
                r#"{"mm": [{"wer": "Beta", "was": "Standing", "wieviel": 50, "datenfehler": {"statt": "Alpha", "note": "wrong faction"}}, "#,
                r#"{"wer": "Beta", "was": "Standing", "wieviel": 3}]}"#
            )
        );
    }
}
