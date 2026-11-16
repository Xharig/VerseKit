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

//! What the Basetool catalog of p4kx states for each contract entry
//! (`docs/basetool-catalog.md` of p4kx, "Contracts", ADR-0273): the blueprint
//! pools a contract or sub contract hands out, the faction it is given for,
//! every place it is offered at and whether it can be shared.
//!
//! The catalog is built for the contract generators only (a Filter on their
//! record path), so p4kx reads no starmap, shop or loot data for it.

use std::collections::HashMap;
use std::time::SystemTime;

use p4kx_archive::Archive;
use p4kx_dataforge::Guid;
use p4kx_export::{CatalogOptions, Filter};
use serde::Deserialize;

use crate::location_targets::LocationTargets;

/// The record path part that selects the contract generators.
pub(crate) const CONTRACT_GENERATORS: &str = "records/contracts/contractgenerator/";

/// The record path part that selects the location templates the location queries of the contracts search.
pub(crate) const LOCATION_TEMPLATES: &str = "records/missiondata/pu_locations/";

/// The `condition` of a location query that searches templates by their tags.
const TAG_SEARCH: &str = "tagSearch";

/// The `source` of a blueprint reward that a mission modifier lends.
const TEMPORARY: &str = "temporaryModifier";

/// The facts of one contract entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Facts {
    /// The blueprint pools the entry hands out, each with its chance as
    /// stored (`blueprintRewards`).
    pub(crate) rewards: Vec<(Guid, Option<f64>)>,
    /// The faction the contract is given for (`giverFaction`).
    pub(crate) giver: Option<Guid>,
    /// The `MissionLocality` records it is offered in (`offeredAt.localityGuids`).
    pub(crate) localities: Vec<Guid>,
    /// The starmap objects it is offered at (`offeredAt.locations`), the
    /// location of its broker entry when no place is stated.
    pub(crate) locations: Vec<Guid>,
    /// Whether it can be shared (`canBeShared`).
    pub(crate) shareable: Option<bool>,
    /// The location queries it searches places with (`locationQueries[].queryGuid`, lower case). They are no
    /// archive GUIDs: they name entries of [`ContractFacts`], not records.
    pub(crate) queries: Vec<String>,
}

/// Which tags of a location template a [`TagSearch`] looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TagType {
    /// The tags the template carries (`tags`).
    #[default]
    General,
    /// The tags of what the place produces (`producesTags`).
    Produces,
    /// The tags of what the place consumes (`consumesTags`).
    Consumes,
}

/// One way a tag search accepts a template: it carries every positive tag and none of the negative ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Term {
    pub(crate) positive: Vec<String>,
    pub(crate) negative: Vec<String>,
}

/// A condition of a location query that searches templates by their tags: one of its terms has to accept the template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TagSearch {
    pub(crate) kind: TagType,
    pub(crate) terms: Vec<Term>,
}

/// A location query of a contract entry: the places it names are the templates that satisfy every one of its tag
/// searches. Conditions of another kind (exclusions, distances) are not read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Query {
    pub(crate) searches: Vec<TagSearch>,
}

/// The facts of every contract entry, by the `id` of the contract and of the
/// sub contract, and the location queries they refer to by GUID text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContractFacts {
    entries: HashMap<(Guid, Option<Guid>), Facts>,
    queries: HashMap<String, Query>,
    targets: LocationTargets,
}

impl ContractFacts {
    /// Reads the contract entries of a Basetool catalog in JSON, e.g. one
    /// `p4kx catalog` wrote or [`catalog_json`] built.
    ///
    /// # Errors
    ///
    /// When the bytes are no catalog JSON.
    pub fn from_catalog(json: &[u8]) -> Result<Self, serde_json::Error> {
        let catalog: RawCatalog = serde_json::from_slice(json)?;
        let mut entries = HashMap::new();
        for entry in catalog.contracts {
            let Some(contract) = guid(&entry.guid) else { continue };
            let sub = match entry.sub_contract_guid.as_deref() {
                Some(text) => match guid(text) {
                    Some(sub) => Some(sub),
                    None => continue,
                },
                None => None,
            };
            let offered = entry.offered_at.unwrap_or_default();
            let facts = Facts {
                // A blueprint a mission lends while it runs
                // (`temporaryModifier`) is no reward: left out.
                rewards: entry
                    .blueprint_rewards
                    .iter()
                    .filter(|reward| reward.source.as_deref() != Some(TEMPORARY))
                    .filter_map(|reward| Some((guid(&reward.pool_guid)?, reward.chance)))
                    .collect(),
                giver: entry.giver_faction.and_then(|giver| guid(&giver.faction.guid)),
                localities: offered.locality_guids.iter().filter_map(|text| guid(text)).collect(),
                locations: offered.locations.iter().filter_map(|place| guid(&place.starmap_object_guid)).collect(),
                shareable: entry.can_be_shared.and_then(|fact| fact.value),
                queries: entry.location_queries.iter().map(|query| query.query_guid.to_ascii_lowercase()).collect(),
            };
            entries.entry((contract, sub)).or_insert(facts);
        }
        let queries = catalog.contract_location_queries.into_iter().map(RawQuery::read).collect();
        Ok(Self { entries, queries, targets: LocationTargets::default() })
    }

    /// The same facts with the location templates of a Basetool catalog in JSON, e.g. one [`catalog_json`] built for
    /// the record path `LOCATION_TEMPLATES`; the places the location queries lead to are read from them.
    ///
    /// # Errors
    ///
    /// When the bytes are no catalog JSON.
    pub fn with_locations(mut self, json: &[u8]) -> Result<Self, serde_json::Error> {
        self.targets = LocationTargets::from_catalog(json)?;
        Ok(self)
    }

    /// The templates the location queries search.
    pub(crate) fn targets(&self) -> &LocationTargets {
        &self.targets
    }

    /// The number of location templates the facts hold, and of the location queries that search them.
    pub fn location_counts(&self) -> (usize, usize) {
        (self.targets.len(), self.queries.len())
    }

    /// The location query with the GUID text `guid` (lower case).
    pub(crate) fn query(&self, guid: &str) -> Option<&Query> {
        self.queries.get(guid)
    }

    /// The facts of the contract `contract`, or of its sub contract `sub`;
    /// `None` for an entry the catalog leaves out (no `id`, or a development
    /// leftover).
    pub(crate) fn get(&self, contract: Guid, sub: Option<Guid>) -> Option<&Facts> {
        self.entries.get(&(contract, sub))
    }

    /// Every contract entry with the `id` of its contract and of its sub contract.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&(Guid, Option<Guid>), &Facts)> {
        self.entries.iter()
    }

    /// The number of contract entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there is no contract entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The Basetool catalog of the records of `archive` whose path matches `filter` (all of them without a filter), as
/// JSON.
///
/// # Errors
///
/// When p4kx cannot read the DataForge or the texts of the archive.
pub fn catalog_json(archive: &Archive, filter: Option<&str>) -> Result<Vec<u8>, p4kx_export::CatalogError> {
    let filter = filter.map(Filter::new);
    let options = CatalogOptions { filter: filter.as_ref(), ..CatalogOptions::new(SystemTime::UNIX_EPOCH) };
    p4kx_export::build_catalog(archive, &options).map(|built| built.into_parts().0)
}

fn guid(text: &str) -> Option<Guid> {
    text.parse().ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    contracts: Vec<RawEntry>,
    #[serde(default)]
    contract_location_queries: Vec<RawQuery>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawQuery {
    guid: String,
    #[serde(default)]
    conditions: Vec<RawCondition>,
}

impl RawQuery {
    /// The query under its GUID text in lower case; only the conditions that search tags are kept.
    fn read(self) -> (String, Query) {
        let searches = self
            .conditions
            .into_iter()
            .filter(|condition| condition.condition.as_deref() == Some(TAG_SEARCH))
            .map(|condition| TagSearch {
                kind: match condition.tag_type.as_deref() {
                    Some("Produces") => TagType::Produces,
                    Some("Consumes") => TagType::Consumes,
                    _ => TagType::General,
                },
                terms: condition
                    .terms
                    .into_iter()
                    .map(|term| Term { positive: term.positive_tags, negative: term.negative_tags })
                    .collect(),
            })
            .collect();
        (self.guid.to_ascii_lowercase(), Query { searches })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCondition {
    condition: Option<String>,
    tag_type: Option<String>,
    #[serde(default)]
    terms: Vec<RawTerm>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTerm {
    #[serde(default)]
    positive_tags: Vec<String>,
    #[serde(default)]
    negative_tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawQueryRef {
    query_guid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    guid: String,
    sub_contract_guid: Option<String>,
    #[serde(default)]
    offered_at: Option<RawOffered>,
    #[serde(default)]
    giver_faction: Option<RawGiver>,
    #[serde(default)]
    blueprint_rewards: Vec<RawReward>,
    #[serde(default)]
    can_be_shared: Option<RawFlag>,
    #[serde(default)]
    location_queries: Vec<RawQueryRef>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOffered {
    #[serde(default)]
    locality_guids: Vec<String>,
    #[serde(default)]
    locations: Vec<RawPlace>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPlace {
    starmap_object_guid: String,
}

#[derive(Deserialize)]
struct RawGiver {
    faction: RawFaction,
}

#[derive(Deserialize)]
struct RawFaction {
    guid: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReward {
    pool_guid: String,
    source: Option<String>,
    chance: Option<f64>,
}

#[derive(Deserialize)]
struct RawFlag {
    value: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::{ContractFacts, TagSearch, TagType, Term};

    #[test]
    fn the_facts_of_each_entry_are_read_by_contract_and_sub_contract() {
        let json = br#"{"schemaVersion": 2, "contracts": [
            {"guid": "10000000-0000-4000-8000-000000000001", "subContractGuid": null,
             "offeredAt": {"localityGuids": ["10000000-0000-4000-8000-000000000011"],
                           "locations": [{"starmapObjectGuid": "10000000-0000-4000-8000-000000000012",
                                          "objectType": null, "place": null}],
                           "brokerFallback": false},
             "giverFaction": {"source": "reputationResult",
                              "faction": {"guid": "10000000-0000-4000-8000-000000000021", "name": "f", "displayName": null}},
             "blueprintRewards": [{"poolGuid": "10000000-0000-4000-8000-000000000031", "source": "result", "chance": 0.3},
                                  {"poolGuid": "10000000-0000-4000-8000-000000000032", "source": "temporaryModifier", "chance": null}],
             "canBeShared": {"value": false, "layer": "template"}},
            {"guid": "10000000-0000-4000-8000-000000000001", "subContractGuid": "10000000-0000-4000-8000-000000000002",
             "offeredAt": {"localityGuids": [], "locations": [], "brokerFallback": false},
             "giverFaction": null, "blueprintRewards": [], "canBeShared": null},
            {"guid": "10000000-0000-4000-8000-000000000001", "subContractGuid": null,
             "blueprintRewards": [], "canBeShared": {"value": true, "layer": "contract"}}
        ]}"#;
        let facts = ContractFacts::from_catalog(json).unwrap();
        assert_eq!(facts.len(), 2, "of two entries with equal ids the first is kept");
        let guid = |last: u8| format!("10000000-0000-4000-8000-0000000000{last:02x}").parse().unwrap();
        let contract = facts.get(guid(0x01), None).unwrap();
        assert_eq!(contract.rewards, [(guid(0x31), Some(0.3))], "a lent blueprint is no reward");
        assert_eq!(contract.giver, Some(guid(0x21)));
        assert_eq!((contract.localities.as_slice(), contract.locations.as_slice()), (&[guid(0x11)][..], &[guid(0x12)][..]));
        assert_eq!(contract.shareable, Some(false));
        let sub = facts.get(guid(0x01), Some(guid(0x02))).unwrap();
        assert!(sub.rewards.is_empty() && sub.giver.is_none() && sub.shareable.is_none());
        assert!(facts.get(guid(0x02), None).is_none());
    }

    #[test]
    fn the_location_queries_of_an_entry_are_read_with_their_tag_searches_only() {
        let json = br#"{"contracts": [
            {"guid": "10000000-0000-4000-8000-000000000001", "subContractGuid": null,
             "locationQueries": [{"variable": "Drop", "token": "Destination", "level": "handler", "queryGuid": "AAAAAAAA-0000-4000-8000-000000000001"},
                                 {"variable": "Pick", "token": "Location", "level": "contract", "queryGuid": "aaaaaaaa-0000-4000-8000-000000000002"}]},
            {"guid": "10000000-0000-4000-8000-000000000002", "subContractGuid": null}
        ], "contractLocationQueries": [
            {"guid": "AAAAAAAA-0000-4000-8000-000000000001", "count": "one", "conditions": [
                {"condition": "tagSearch", "tagType": "General", "terms": [
                    {"positiveTags": ["Missions.A", "Missions.B"], "negativeTags": ["Missions.C"]},
                    {"positiveTags": ["Missions.D"], "negativeTags": []}]},
                {"condition": "tagSearch", "tagType": "Consumes", "terms": [{"positiveTags": ["Missions.E"], "negativeTags": []}]},
                {"condition": "tagSearch", "tagType": "Produces", "terms": []},
                {"condition": "tagSearch", "tagType": null, "terms": [{"positiveTags": ["Missions.F"], "negativeTags": []}]},
                {"condition": "excludeProperty", "excludedProperty": "x"},
                {"condition": "excludeNearbyLocations", "locationProperty": "y", "rangeKm": 10}]},
            {"guid": "aaaaaaaa-0000-4000-8000-000000000002", "conditions": []}
        ]}"#;
        let facts = ContractFacts::from_catalog(json).unwrap();
        let guid = |last: u8| format!("10000000-0000-4000-8000-0000000000{last:02x}").parse().unwrap();
        let entry = facts.get(guid(0x01), None).unwrap();
        assert_eq!(
            entry.queries,
            ["aaaaaaaa-0000-4000-8000-000000000001", "aaaaaaaa-0000-4000-8000-000000000002"],
            "the GUID texts are kept in lower case, in order"
        );
        assert!(facts.get(guid(0x02), None).unwrap().queries.is_empty());
        let term = |positive: &[&str], negative: &[&str]| Term {
            positive: positive.iter().map(|tag| (*tag).to_string()).collect(),
            negative: negative.iter().map(|tag| (*tag).to_string()).collect(),
        };
        let query = facts.query(&entry.queries[0]).unwrap();
        assert_eq!(
            query.searches,
            [
                TagSearch {
                    kind: TagType::General,
                    terms: vec![term(&["Missions.A", "Missions.B"], &["Missions.C"]), term(&["Missions.D"], &[])]
                },
                TagSearch { kind: TagType::Consumes, terms: vec![term(&["Missions.E"], &[])] },
                TagSearch { kind: TagType::Produces, terms: Vec::new() },
                TagSearch { kind: TagType::General, terms: vec![term(&["Missions.F"], &[])] },
            ],
            "exclusions and distances are not read; a missing tag type is the general one"
        );
        assert!(facts.query(&entry.queries[1]).unwrap().searches.is_empty());
        assert!(facts.query("aaaaaaaa-0000-4000-8000-0000000000ff").is_none());
    }
}
