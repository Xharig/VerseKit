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

//! Which contracts and scenario tiers award which blueprints.
//!
//! A `BlueprintPoolRecord` lists blueprints. A contract of a
//! `ContractGenerator` awards pools through `BlueprintRewards` results; a
//! tier of a `ScenarioProgress` awards pools once its points are reached.
//!
//! Each sub contract of a contract is a source of its own besides the
//! contract, with its own parameters and places laid over those of the
//! contract and its handler.
//!
//! Which pools a contract entry hands out, the faction it is given for, the
//! places it is offered at and whether it can be shared are the facts p4kx
//! states per entry ([`ContractFacts`]); the rest is read here.

use std::collections::{BTreeSet, HashMap, HashSet};

use p4kx_dataforge::{Guid, RecordTree, StructValue, Value};
use p4kx_localization::is_placeholder_key;
use versekit_core::text::py_strip;

use crate::contract_facts::{ContractFacts, Facts};
use crate::corrections::CatalogFixes;
use crate::fields::{
    Trees, as_struct, boolean, class_name, elements, integer, link, locale_key, number, reference, structs,
    structure, text,
};
use crate::location_targets::{Bodies, Place};
use crate::{Localization, Source, SourceKind, Text};

/// The records sources are read from.
#[derive(Debug, Default)]
pub(crate) struct Inputs {
    pub(crate) pools: Vec<Guid>,
    pub(crate) generators: Vec<Guid>,
    pub(crate) scenarios: Vec<Guid>,
    /// The star map objects whose record name ends in `Star` (`StantonStar`, `NyxStar`), the stars among them.
    pub(crate) stars: Vec<Guid>,
    /// Every star map object with its record name without the type, to find the bodies by their codes.
    pub(crate) star_map: Vec<(Guid, String)>,
}

/// The sources of every blueprint, with counts of what awarded them.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) by_blueprint: HashMap<Guid, Vec<Source>>,
    /// The record names of the reward pools that list each blueprint, each
    /// once, in the order the pools were read.
    pub(crate) pools_by_blueprint: HashMap<Guid, Vec<String>>,
    pub(crate) pools: usize,
    pub(crate) in_pools: usize,
    pub(crate) contracts: usize,
    pub(crate) unreleased_contracts: usize,
    pub(crate) scenario_tiers: usize,
}

/// A reward pool: its record name and the blueprints it lists.
struct Pool {
    name: String,
    blueprints: Vec<Guid>,
}

/// The contract lists of a generator handler, those p4kx reads.
const CONTRACT_LISTS: [&str; 4] = ["contracts", "introContracts", "legacyContracts", "PVPBountyContract"];

pub(crate) fn collect(
    trees: &mut Trees<'_, '_>,
    inputs: &Inputs,
    texts: &Localization,
    fixes: &CatalogFixes,
    facts: &ContractFacts,
) -> Found {
    let mut pools = HashMap::new();
    let mut pools_by_blueprint: HashMap<Guid, Vec<String>> = HashMap::new();
    for &guid in &inputs.pools {
        let Some(tree) = trees.get(guid) else { continue };
        let mut blueprints: Vec<Guid> = structs(Some(tree.root()), "blueprintRewards")
            .filter_map(|reward| reference(Some(reward), "blueprintRecord"))
            .collect();
        fix_duplicates(&mut blueprints, guid, fixes);
        if !blueprints.is_empty() {
            let name = class_name(&tree).to_string();
            for blueprint in &blueprints {
                let names = pools_by_blueprint.entry(*blueprint).or_default();
                if !names.contains(&name) {
                    names.push(name.clone());
                }
            }
            pools.insert(guid, Pool { name, blueprints });
        }
    }
    let in_pools: HashSet<Guid> = pools.values().flat_map(|pool| pool.blueprints.iter().copied()).collect();
    let mut found = Found { pools: pools.len(), in_pools: in_pools.len(), pools_by_blueprint, ..Found::default() };
    let count = versekit_core::build_progress::counter(inputs.generators.len());
    for (at, &guid) in inputs.generators.iter().enumerate() {
        count.at(at);
        let Some(tree) = trees.get(guid) else { continue };
        for handler in structs(Some(tree.root()), "generators") {
            for list in CONTRACT_LISTS {
                for contract in structs(Some(handler), list) {
                    let Some(id) = identity(contract) else { continue };
                    // Pools awarded only at another result than the completion: p4kx leaves them out; they count
                    // for every entry of the contract, as its own rewards do, marked as not sure.
                    let uncertain: Vec<_> =
                        uncertain_rewards(contract).into_iter().filter(|(pool, ..)| pools.contains_key(pool)).collect();
                    // The entries p4kx lists, the contract's and each sub
                    // contract's, that hand out a pool of blueprints.
                    let entries: Vec<_> = std::iter::once(None)
                        .chain(structs(Some(contract), "subContracts").map(Some))
                        .filter_map(|sub| {
                            let sub_id = match sub {
                                Some(sub) => Some(identity(sub)?),
                                None => None,
                            };
                            Some((sub, facts.get(id, sub_id)?))
                        })
                        .filter(|(_, facts)| {
                            !uncertain.is_empty() || facts.rewards.iter().any(|(pool, _)| pools.contains_key(pool))
                        })
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    if !is_released(handler) || !is_released(contract) {
                        found.unreleased_contracts += 1;
                        continue;
                    }
                    found.contracts += 1;
                    for (sub, facts) in entries {
                        let source = contract_source(&tree, handler, contract, sub, facts, trees, texts, fixes);
                        award(&mut found, &pools, &source, &facts.rewards);
                        for (pool, chance, at) in &uncertain {
                            if facts.rewards.iter().any(|(sure, _)| sure == pool) {
                                continue;
                            }
                            let unsure = Source { uncertain: at.clone(), ..source.clone() };
                            award(&mut found, &pools, &unsure, &[(*pool, *chance)]);
                        }
                    }
                }
            }
        }
    }
    drop(count);
    for &guid in &inputs.scenarios {
        let Some(tree) = trees.get(guid) else { continue };
        scenario_sources(&tree, &pools, &mut found, trees, texts);
    }
    for sources in found.by_blueprint.values_mut() {
        sources.sort_by(|a, b| {
            (a.min_reputation.is_none(), a.min_reputation, a.min_points, &a.title, &a.id).cmp(&(
                b.min_reputation.is_none(),
                b.min_reputation,
                b.min_points,
                &b.title,
                &b.id,
            ))
        });
    }
    found
}

/// The text keys a contract entry shows as title and description, where they differ from the keys it names: every
/// string of the organisation with the tag the placeholder asks for, of which the game shows one. Empty where the key
/// it names stays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShownKeys {
    pub title: Vec<String>,
    pub description: Vec<String>,
}

/// The key of a contract entry in [`shown_keys`]: the `id` of the contract and of the sub contract in lower case,
/// `<contract>|<sub>`, the sub empty for the contract itself.
pub fn entry_key(contract: &str, sub: Option<&str>) -> String {
    format!("{}|{}", contract.to_ascii_lowercase(), sub.unwrap_or_default().to_ascii_lowercase())
}

/// For every contract entry of every generator (no filter on rewards), the keys of its title and description whose
/// English text is no more than one placeholder `~mission(Token|Variant)`: the key of the text the game puts in its
/// place, e.g. the string variant of the contractor organisation (`@blackbox_recover_title_E_001`, whose text is
/// `~mission(Contractor|BlackBoxRecoverTitleEasy)`, shows `@Hockrow_BlackBox_space_E_Title_001`). An entry whose
/// keys stay as they are is left out.
pub(crate) fn shown_keys(
    trees: &mut Trees<'_, '_>,
    generators: &[Guid],
    texts: &Localization,
) -> HashMap<String, ShownKeys> {
    let mut found = HashMap::new();
    for &guid in generators {
        let Some(tree) = trees.get(guid) else { continue };
        for handler in structs(Some(tree.root()), "generators") {
            for list in CONTRACT_LISTS {
                for contract in structs(Some(handler), list) {
                    let Some(id) = identity(contract) else { continue };
                    let template = trees.follow(Some(contract), "template");
                    let template_root = template.as_deref().map(RecordTree::root);
                    let display = structure(template_root, "contractDisplayInfo");
                    for sub in std::iter::once(None).chain(structs(Some(contract), "subContracts").map(Some)) {
                        let sub_id = match sub {
                            Some(sub) => match identity(sub) {
                                Some(sub_id) => Some(sub_id),
                                None => continue,
                            },
                            None => None,
                        };
                        let layers = layers(handler, contract, sub);
                        let title = layered_param(&layers, "Title")
                            .or_else(|| elements(display, "displayString").first().and_then(key_value));
                        let description = layered_param(&layers, "Description");
                        let keys = entry_shown_keys(title, description, &layers, template_root, trees, texts);
                        if keys != ShownKeys::default() {
                            let sub_text = sub_id.map(|sub| sub.to_string());
                            found.entry(entry_key(&id.to_string(), sub_text.as_deref())).or_insert(keys);
                        }
                    }
                }
            }
        }
    }
    found
}

/// The amount of every reputation gain of every contract entry (by [`entry_key`]), read from the reward record each
/// gain links to (`reputationAmount`), in the order the gains stand in the p4kx catalog: the results of the sub
/// contract when it states any, else those of the contract, the ones that fire on success, each amount in stored
/// order. `None` where the link or the record cannot be read.
///
/// ⚠ p4kx 0.4.2 takes the amount only from the records under `missionrewards_reputation/`; since 5.0 the gains of
/// most contracts link to `factionreprewardvalues/` and `guildreprewardvalues/` (same record type, same field), so
/// the catalog has `amount: null` for them. This reads the linked record, wherever it is.
pub(crate) fn reward_amounts(trees: &mut Trees<'_, '_>, generators: &[Guid]) -> HashMap<String, Vec<Option<i64>>> {
    let mut found = HashMap::new();
    for &guid in generators {
        let Some(tree) = trees.get(guid) else { continue };
        for handler in structs(Some(tree.root()), "generators") {
            for list in CONTRACT_LISTS {
                for contract in structs(Some(handler), list) {
                    let Some(id) = identity(contract) else { continue };
                    for sub in std::iter::once(None).chain(structs(Some(contract), "subContracts").map(Some)) {
                        let sub_id = match sub {
                            Some(sub) => match identity(sub) {
                                Some(sub_id) => Some(sub_id),
                                None => continue,
                            },
                            None => None,
                        };
                        let block = sub
                            .and_then(|sub| structure(Some(sub), "contractResults"))
                            .filter(|block| !elements(Some(block), "contractResults").is_empty())
                            .or_else(|| structure(Some(contract), "contractResults"));
                        let mut amounts = Vec::new();
                        for result in structs(block, "contractResults")
                            .filter(|result| result.type_name() == LEGACY_REPUTATION && succeeds(result))
                        {
                            for amount in one_or_many(result, "contractResultReputationAmounts") {
                                let read = reference(Some(amount), "reward")
                                    .and_then(|reward| trees.get(reward))
                                    .and_then(|record| integer(Some(record.root()), "reputationAmount"));
                                amounts.push(read);
                            }
                        }
                        if !amounts.is_empty() {
                            let sub_text = sub_id.map(|sub| sub.to_string());
                            found.entry(entry_key(&id.to_string(), sub_text.as_deref())).or_insert(amounts);
                        }
                    }
                }
            }
        }
    }
    found
}

/// The result type that grants reputation.
const LEGACY_REPUTATION: &str = "ContractResult_LegacyReputation";

/// Whether a result fires on success: its first `missionResults` flag is set.
fn succeeds(result: &StructValue<'_>) -> bool {
    match elements(Some(result), "missionResults").first() {
        Some(Value::Bool(set)) => *set,
        Some(Value::Struct(flag)) => boolean(Some(flag), "value").unwrap_or(false),
        _ => false,
    }
}

/// The structs of the field `name`: the elements of an array, or the one struct the field holds.
fn one_or_many<'t, 'f>(parent: &'t StructValue<'f>, name: &str) -> Vec<&'t StructValue<'f>> {
    match parent.field(name) {
        Some(Value::Array(items)) => items.iter().filter_map(as_struct).collect(),
        Some(Value::Struct(one)) => vec![one],
        _ => Vec::new(),
    }
}

/// The keys an entry with the title key `title` and the description key `description` shows in their place
/// ([`shown_keys_of`]); a side whose key stays as it is, is empty.
fn entry_shown_keys(
    title: Option<&str>,
    description: Option<&str>,
    layers: &[&StructValue<'_>],
    template: Option<&StructValue<'_>>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) -> ShownKeys {
    let mut resolve = |key: Option<&str>| {
        let Some(key) = key else { return Vec::new() };
        let shown = shown_keys_of(key, layers, template, trees, texts);
        if shown.iter().all(|one| one == key) { Vec::new() } else { shown }
    };
    ShownKeys { title: resolve(title), description: resolve(description) }
}

/// The keys that stand in for `key` when its English text is one placeholder of an organisation's strings (every one
/// with the tag asked for) or of a fixed choice ([`placeholder`]); empty when it is something else.
fn shown_keys_of(
    key: &str,
    layers: &[&StructValue<'_>],
    template: Option<&StructValue<'_>>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) -> Vec<String> {
    let Some(text) = Text::resolve(Some(key), texts).text else { return Vec::new() };
    let Some(inner) = py_strip(&text).strip_prefix("~mission(").and_then(|rest| rest.strip_suffix(')')).map(String::from)
    else {
        return Vec::new();
    };
    if inner.contains('(') || inner.contains(')') {
        return Vec::new();
    }
    let (name, variant) = inner.split_once('|').map_or((inner.as_str(), None), |(name, variant)| (name, Some(variant)));
    let Some(value) = property(layers, template, name) else { return Vec::new() };
    match value.type_name() {
        "MissionPropertyValue_StringHash" => structs(Some(value), "options")
            .next()
            .and_then(|option| locale_key(Some(option), "textId"))
            .map(String::from)
            .into_iter()
            .collect(),
        "MissionPropertyValue_Organization" => organization_strings(value, variant.unwrap_or("Name"), trees),
        _ => Vec::new(),
    }
}

/// The localisation keys of every string variant tagged `variant` of the first organisation the property `value`
/// names, in stored order.
fn organization_strings(value: &StructValue<'_>, variant: &str, trees: &mut Trees<'_, '_>) -> Vec<String> {
    let Some(organization) = organization(value).and_then(|guid| trees.get(guid)) else { return Vec::new() };
    let mut found = Vec::new();
    for entry in structs(structure(Some(organization.root()), "stringVariants"), "variants") {
        let tag = trees.follow(Some(entry), "tag");
        if tag.as_deref().and_then(|tag| text(Some(tag.root()), "tagName")).is_some_and(|name| name == variant)
            && let Some(key) = locale_key(Some(entry), "string")
        {
            found.push(key.to_string());
        }
    }
    found
}

/// Corrects the pool `pool` where the data list one blueprint twice: the
/// second place takes the blueprint that was meant, or goes when none is
/// known.
fn fix_duplicates(blueprints: &mut Vec<Guid>, pool: Guid, fixes: &CatalogFixes) {
    for (index, duplicated, replacement) in fixes.pool_entries(&pool.to_string()) {
        let mut places = blueprints.iter().enumerate().filter(|(_, found)| found.to_string() == duplicated);
        let Some(second) = places.nth(1).map(|(place, _)| place) else { continue };
        match replacement.and_then(|meant| meant.parse::<Guid>().ok()) {
            Some(meant) => blueprints[second] = meant,
            None => {
                blueprints.remove(second);
            }
        }
        fixes.pool_entry_applied(index);
    }
}

/// Adds `source` to every blueprint of every pool in `rewards`, once per
/// blueprint and equal source.
fn award(found: &mut Found, pools: &HashMap<Guid, Pool>, source: &Source, rewards: &[(Guid, Option<f64>)]) {
    for (guid, chance) in rewards {
        let Some(pool) = pools.get(guid) else { continue };
        for blueprint in &pool.blueprints {
            let entry = Source { pool: pool.name.clone(), chance: *chance, ..source.clone() };
            let list = found.by_blueprint.entry(*blueprint).or_default();
            if !list.contains(&entry) {
                list.push(entry);
            }
        }
    }
}

/// Whether neither `notForRelease` nor `workInProgress` is set.
fn is_released(value: &StructValue<'_>) -> bool {
    boolean(Some(value), "notForRelease") != Some(true) && boolean(Some(value), "workInProgress") != Some(true)
}

/// The `id` of a contract or sub contract; a nil one is none.
fn identity(value: &StructValue<'_>) -> Option<Guid> {
    match value.field("id")? {
        Value::Guid(id) if *id != Guid::default() => Some(*id),
        _ => None,
    }
}

/// The results of a contract.
fn results<'t, 'f>(contract: &'t StructValue<'f>) -> impl Iterator<Item = &'t StructValue<'f>> {
    structs(structure(Some(contract), "contractResults"), "contractResults")
}

/// Whether a contract result applies when the contract succeeds: the first
/// of its `missionResults` flags is set. A result without flags applies.
fn on_success(result: &StructValue<'_>) -> bool {
    !matches!(elements(Some(result), "missionResults").first(), Some(Value::Bool(false)))
}

/// The value of the string parameter `name` in the parameter overrides
/// `params`.
fn string_param<'f>(params: Option<&StructValue<'f>>, name: &str) -> Option<&'f str> {
    structs(params, "stringParamOverrides")
        .find(|param| text(Some(param), "param") == Some(name))
        .and_then(|param| locale_key(Some(param), "value").or_else(|| text(Some(param), "value")))
}

/// The text of `key` for a placeholder: `None` for a placeholder key or an
/// empty text.
fn filled(key: &str, texts: &Localization) -> Option<String> {
    Text::resolve(Some(key), texts).text.filter(|text| !py_strip(text).is_empty())
}

/// The parameter sets of a contract, the one that counts first: the sub
/// contract's own overrides, the contract's, its handler's. Each is read
/// for `stringParamOverrides`, `propertyOverrides` and `missionTypeOverride`.
fn layers<'t, 'f>(
    handler: &'t StructValue<'f>,
    contract: &'t StructValue<'f>,
    sub: Option<&'t StructValue<'f>>,
) -> Vec<&'t StructValue<'f>> {
    [sub, structure(Some(contract), "paramOverrides"), structure(Some(handler), "contractParams")]
        .into_iter()
        .flatten()
        .collect()
}

/// The string parameter `name` of the first parameter set that has it.
fn layered_param<'f>(layers: &[&StructValue<'f>], name: &str) -> Option<&'f str> {
    layers.iter().find_map(|layer| string_param(Some(layer), name))
}

/// The boolean parameter `name` of the first parameter set that has it.
fn layered_bool(layers: &[&StructValue<'_>], name: &str) -> Option<bool> {
    layers.iter().find_map(|layer| {
        structs(Some(layer), "boolParamOverrides")
            .find(|param| text(Some(param), "param") == Some(name))
            .and_then(|param| boolean(Some(param), "value"))
    })
}

/// The integer parameter `name` of the first parameter set that has it.
fn layered_int(layers: &[&StructValue<'_>], name: &str) -> Option<i64> {
    layers.iter().find_map(|layer| {
        structs(Some(layer), "intParamOverrides")
            .find(|param| text(Some(param), "param") == Some(name))
            .and_then(|param| integer(Some(param), "value"))
    })
}

/// The prerequisites of the sub contract, then of the contract, then of its
/// handler's default availability.
fn prerequisites<'t, 'f>(
    handler: &'t StructValue<'f>,
    contract: &'t StructValue<'f>,
    sub: Option<&'t StructValue<'f>>,
) -> Vec<&'t StructValue<'f>> {
    let mut all: Vec<_> = structs(sub, "additionalPrerequisites").collect();
    all.extend(structs(Some(contract), "additionalPrerequisites"));
    all.extend(structs(structure(Some(handler), "defaultAvailability"), "prerequisites"));
    all
}

/// The value of the mission property whose text token is `token`: from the
/// first parameter set that overrides it, else from the template's
/// properties.
fn property<'t, 'f>(
    layers: &[&'t StructValue<'f>],
    template: Option<&'t StructValue<'f>>,
    token: &str,
) -> Option<&'t StructValue<'f>> {
    let matches = |property: &&'t StructValue<'f>| {
        text(Some(property), "extendedTextToken").is_some_and(|own| own.eq_ignore_ascii_case(token))
    };
    layers
        .iter()
        .find_map(|layer| structs(Some(layer), "propertyOverrides").find(matches))
        .or_else(|| structs(template, "contractProperties").find(matches))
        .and_then(|property| structure(Some(property), "value"))
}

/// The first organisation the organisation property `value` names.
fn organization(value: &StructValue<'_>) -> Option<Guid> {
    if value.type_name() != "MissionPropertyValue_Organization" {
        return None;
    }
    structs(Some(value), "matchConditions")
        .flat_map(|condition| elements(Some(condition), "organizations").iter().filter_map(link))
        .next()
}

/// The localisation key of the string variant tagged `variant` of the first
/// organisation the property `value` names.
fn organization_string(value: &StructValue<'_>, variant: &str, trees: &mut Trees<'_, '_>) -> Option<String> {
    let organization = trees.get(organization(value)?)?;
    for entry in structs(structure(Some(organization.root()), "stringVariants"), "variants") {
        let tag = trees.follow(Some(entry), "tag");
        if tag.as_deref().and_then(|tag| text(Some(tag.root()), "tagName")).is_some_and(|name| name == variant) {
            return locale_key(Some(entry), "string").map(str::to_string);
        }
    }
    None
}

/// What stands for the placeholder `~mission(token)` in a contract title:
/// the text of a fixed choice or of an organisation's string, a marker in
/// brackets for what the game picks when it offers the contract
/// (`[LOCATION]`, `[TARGET]`, `[SHIP]`); `None` when it is not known.
fn placeholder(
    token: &str,
    layers: &[&StructValue<'_>],
    template: Option<&StructValue<'_>>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) -> Option<String> {
    let (name, variant) = token.split_once('|').map_or((token, None), |(name, variant)| (name, Some(variant)));
    let Some(value) = property(layers, template, name) else {
        return match name {
            "Location" => Some("[LOCATION]".into()),
            "TargetName" => Some("[TARGET]".into()),
            "Ship" => Some("[SHIP]".into()),
            _ => None,
        };
    };
    match value.type_name() {
        "MissionPropertyValue_StringHash" => {
            let key = structs(Some(value), "options").next().and_then(|option| locale_key(Some(option), "textId"))?;
            filled(key, texts)
        }
        "MissionPropertyValue_Organization" => {
            organization_string(value, variant.unwrap_or("Name"), trees).and_then(|key| filled(&key, texts))
        }
        "MissionPropertyValue_Location" => Some("[LOCATION]".into()),
        "MissionPropertyValue_AIName" | "MissionPropertyValue_NPCSpawnDescriptions" => Some("[TARGET]".into()),
        "MissionPropertyValue_ShipSpawnDescriptions" => Some("[SHIP]".into()),
        _ => None,
    }
}

/// `text` with each `~mission(token)` replaced by what `fill` gives for the
/// token; a placeholder it gives nothing for stays. Leading and trailing
/// white space go.
fn fill_placeholders(text: &str, mut fill: impl FnMut(&str) -> Option<String>) -> String {
    const START: &str = "~mission(";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(START) {
        let after = &rest[at + START.len()..];
        let Some(end) = after.find(')') else { break };
        out.push_str(&rest[..at]);
        match fill(&after[..end]) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[at..at + START.len() + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    py_strip(&out).to_string()
}

/// The contractor `text` with its placeholders filled as in a title. `None` when it has no text, is empty after
/// the filling or still holds a placeholder.
fn filled_contractor(text: Text, fill: impl FnMut(&str) -> Option<String>) -> Option<Text> {
    let filled = fill_placeholders(text.text.as_deref()?, fill);
    (!filled.is_empty() && !filled.contains("~mission(")).then_some(Text { key: text.key, text: Some(filled) })
}

/// The localisation key `value` holds, when it is one.
fn key_value<'f>(value: &Value<'f>) -> Option<&'f str> {
    match *value {
        Value::Locale(key) if !key.is_empty() => Some(key),
        Value::String(text) if text.starts_with('@') && text.len() > 1 => Some(text),
        _ => None,
    }
}

#[expect(clippy::too_many_arguments, reason = "the levels of a contract entry, its facts and the readers")]
fn contract_source(
    tree: &RecordTree<'_>,
    handler: &StructValue<'_>,
    contract: &StructValue<'_>,
    sub: Option<&StructValue<'_>>,
    facts: &Facts,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
    fixes: &CatalogFixes,
) -> Source {
    let layers = layers(handler, contract, sub);
    let id = text(Some(contract), "debugName").unwrap_or_default();
    let template = trees.follow(Some(contract), "template");
    let template_root = template.as_deref().map(RecordTree::root);
    let display = structure(template_root, "contractDisplayInfo");
    let title_key =
        layered_param(&layers, "Title").or_else(|| elements(display, "displayString").first().and_then(key_value));
    let mission = Text::resolve(title_key, texts);
    let title = fill_placeholders(mission.shown(), |token| placeholder(token, &layers, template_root, trees, texts));
    let contractor = match layered_param(&layers, "Contractor") {
        Some(key) => Some(Text::resolve(Some(key), texts)),
        None => property(&layers, template_root, "Contractor")
            .and_then(|value| organization_string(value, "Name", trees))
            .map(|key| Text::resolve(Some(&key), texts)),
    }
    .and_then(|text| filled_contractor(text, |token| placeholder(token, &layers, template_root, trees, texts)));
    let mission_type = layers
        .iter()
        .find_map(|layer| reference(Some(layer), "missionTypeOverride"))
        .or_else(|| reference(display, "type"))
        .and_then(|guid| trees.get(guid))
        .map(|kind| Text::resolve(locale_key(Some(kind.root()), "LocalisedTypeName"), texts));
    let prerequisites = prerequisites(handler, contract, sub);
    let reputation_prerequisite = prerequisites.iter().find(|p| p.type_name() == "ContractPrerequisite_Reputation");
    let reputations: Vec<_> = results(contract)
        .filter(|result| result.type_name() == "ContractResult_LegacyReputation" && on_success(result))
        .filter_map(|result| structure(Some(result), "contractResultReputationAmounts"))
        .collect();
    let meant_faction = fixes.faction(id).and_then(|meant| meant.parse::<Guid>().ok());
    let faction = meant_faction
        .or(facts.giver)
        .and_then(|guid| trees.get(guid))
        .map(|faction| Text::resolve(locale_key(Some(faction.root()), "displayName"), texts));
    let standing = trees
        .follow(Some(contract), "minStanding")
        .or_else(|| reputation_prerequisite.and_then(|p| trees.follow(Some(p), "minStanding")));
    let gains: Vec<i64> = reputations
        .iter()
        .filter_map(|amounts| trees.follow(Some(amounts), "reward"))
        .filter_map(|amount| integer(Some(amount.root()), "reputationAmount"))
        .collect();
    let reputation_gain = (!gains.is_empty()).then(|| gains.iter().sum());
    let reward_uec = results(contract)
        .find(|result| result.type_name() == "ContractResult_Reward")
        .and_then(|result| integer(structure(Some(result), "contractReward"), "reward"));
    // Only the contract's own limit: a limit in the reputation prerequisite
    // is not one scmdb counts (71 more blueprints would differ).
    let highest = trees.follow(Some(contract), "maxStanding");
    let availability = structure(Some(handler), "defaultAvailability");
    // ⚠ The availability of the handler the contract stands in, and the
    // overrides of the contract and its sub contract; the overrides of the
    // handler do not count, as in scmdb: a few handlers carry a
    // `PersonalCooldownTime` parameter that differs from their availability
    // (30 against 15 minutes at the Ling family haulers), and which of the
    // two the game uses is not known. The availability holds floats (`15.0`).
    // (scmdb reads the availability of the generator's first handler for all
    // its contracts; the data states it per handler.)
    let own_layers: Vec<&StructValue<'_>> =
        [sub, structure(Some(contract), "paramOverrides")].into_iter().flatten().collect();
    let cooldown = layered_bool(&own_layers, "HasPersonalCooldown")
        .or_else(|| boolean(availability, "hasPersonalCooldown"))
        .filter(|&has| has)
        .and_then(|_| {
            layered_int(&own_layers, "PersonalCooldownTime")
                .or_else(|| number(availability, "personalCooldownTime").map(|minutes| minutes.round() as i64))
        });
    let (systems, places) = places(facts, trees, texts);
    let description_param = layered_param(&layers, "Description");
    let description_key = crate::catalog::text_key(description_param);
    let shown = entry_shown_keys(title_key, description_param, &layers, template_root, trees, texts);
    Source {
        kind: SourceKind::Contract,
        id: id.to_string(),
        record_path: tree.record_path().to_string(),
        mission,
        title,
        mission_type,
        contractor,
        faction,
        description_key,
        shown,
        systems,
        places,
        rank: standing.as_ref().map(|standing| Text::resolve(locale_key(Some(standing.root()), "displayName"), texts)),
        min_reputation: standing.as_ref().and_then(|standing| integer(Some(standing.root()), "minReputation")),
        max_rank: highest
            .as_ref()
            .map(|standing| Text::resolve(locale_key(Some(standing.root()), "displayName"), texts)),
        max_reputation: highest.as_ref().and_then(|standing| integer(Some(standing.root()), "minReputation")),
        shareable: facts.shareable,
        cooldown,
        reputation_gain,
        min_points: None,
        reward_uec,
        chance: None,
        pool: String::new(),
        uncertain: Vec::new(),
    }
}

/// The names of the flags of `missionResults`, in their order (`EMissionResult`).
const MISSION_RESULTS: [&str; 5] = ["Completed", "InProgress", "Failed", "Abandoned", "Withdrawn"];

/// The blueprint pools a contract awards only at results other than its completion: each `BlueprintRewards` result
/// whose first `missionResults` flag (Completed) is not set but another one is, with its chance and the names of
/// the results it is awarded at. p4kx leaves them out of its catalog (only rewards on completion count there).
fn uncertain_rewards(contract: &StructValue<'_>) -> Vec<(Guid, Option<f64>, Vec<String>)> {
    results(contract)
        .filter(|result| result.type_name() == "BlueprintRewards" && !on_success(result))
        .filter_map(|result| {
            let flags = elements(Some(result), "missionResults");
            let at: Vec<String> = flags
                .iter()
                .enumerate()
                .filter(|(_, flag)| matches!(flag, Value::Bool(true)))
                .filter_map(|(index, _)| MISSION_RESULTS.get(index).map(|name| (*name).to_string()))
                .collect();
            if at.is_empty() {
                return None;
            }
            Some((reference(Some(result), "blueprintPool")?, number(Some(result), "chance"), at))
        })
        .collect()
}

/// What a place of the star map stands for in a contract's list of places.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlaceKind {
    /// A star: it names its system.
    Star,
    /// The hidden object of a whole system (no parent, not shown on the map, e.g. `NyxSolarSystem`).
    System,
    /// A planet, moon, station or other place.
    Place,
}

/// The star systems and the names of the places p4kx states for an entry: the objects of each of its localities,
/// then its locations. A star (its map icon) names its system; the hidden object of a whole system names it only
/// where no star does, and is never a place. Each name once, places named only by a placeholder key left out.
fn places(facts: &Facts, trees: &mut Trees<'_, '_>, texts: &Localization) -> (Vec<Text>, Vec<Text>) {
    let mut objects: Vec<Guid> = Vec::new();
    for &locality in &facts.localities {
        let Some(locality) = trees.get(locality) else { continue };
        objects.extend(elements(Some(locality.root()), "availableLocations").iter().filter_map(link));
    }
    objects.extend(facts.locations.iter().copied());
    let mut keys: Vec<(String, PlaceKind)> = Vec::new();
    for object in objects {
        let Some(place) = trees.get(object) else { continue };
        let root = Some(place.root());
        let kind = if text(root, "navIcon") == Some("Star") {
            PlaceKind::Star
        } else if reference(root, "parent").is_none() && boolean(root, "hideInStarmap") == Some(true) {
            PlaceKind::System
        } else {
            PlaceKind::Place
        };
        let Some(key) = locale_key(root, "name").map(str::to_string) else { continue };
        if !is_placeholder_key(&key) && !keys.iter().any(|(known, _)| known.eq_ignore_ascii_case(&key)) {
            keys.push((key, kind));
        }
    }
    let of = |wanted: PlaceKind| -> Vec<Text> {
        keys.iter().filter(|(_, kind)| *kind == wanted).map(|(key, _)| Text::resolve(Some(key), texts)).collect()
    };
    let stars = of(PlaceKind::Star);
    let systems = if stars.is_empty() { of(PlaceKind::System) } else { stars };
    (systems, of(PlaceKind::Place))
}

/// Where every contract entry p4kx lists is offered ([`crate::Offered`]), by [`entry_key`]: the system and the planet
/// of each object of its localities and of each of its locations, read up the star map ([`whereabouts`]), and of
/// each place its location queries lead to ([`crate::LocationTargets`]). `stars` are the candidates for the stars
/// ([`Inputs::stars`]), `star_map` the objects the bodies are found among ([`Inputs::star_map`]). An entry with no
/// place in a known system is left out.
pub(crate) fn offered(
    trees: &mut Trees<'_, '_>,
    stars: &[Guid],
    star_map: &[(Guid, String)],
    facts: &ContractFacts,
    texts: &Localization,
) -> HashMap<String, crate::Offered> {
    // The folder of each system (`…/starmap/pu/system/nyx/`) → the name of its star.
    let mut systems: HashMap<String, String> = HashMap::new();
    for &star in stars {
        let Some(tree) = trees.get(star) else { continue };
        let root = Some(tree.root());
        if text(root, "navIcon") == Some("Star")
            && let (Some(folder), Some(name)) = (system_folder(tree.record_path()), english_name(root, texts))
        {
            systems.entry(folder).or_insert(name);
        }
    }
    let mut known: HashMap<Guid, Option<(String, Option<String>)>> = HashMap::new();
    // The systems are those of the stars (their folder names the code), the bodies those objects named by a code.
    let codes = systems.iter().filter_map(|(folder, name)| {
        Some((folder.trim_end_matches('/').rsplit('/').next()?.to_string(), name.clone()))
    });
    let mut bodies = Bodies::new(codes);
    let coded: Vec<(Guid, String)> =
        star_map.iter().filter_map(|(guid, name)| Some((*guid, bodies.body_code(name)?))).collect();
    for (object, code) in coded {
        let planet = known.entry(object).or_insert_with(|| whereabouts(object, &systems, trees, texts)).clone();
        bodies.add_body(code, planet.and_then(|(_, planet)| planet));
    }
    offered_by(facts, &bodies, |facts| {
        let mut objects: Vec<Guid> = Vec::new();
        for &locality in &facts.localities {
            let Some(locality) = trees.get(locality) else { continue };
            objects.extend(elements(Some(locality.root()), "availableLocations").iter().filter_map(link));
        }
        objects.extend(facts.locations.iter().copied());
        objects
            .into_iter()
            .filter_map(|object| {
                known.entry(object).or_insert_with(|| whereabouts(object, &systems, trees, texts)).clone()
            })
            .collect()
    })
}

/// [`offered`] with the places of an entry's own localities and locations given by `places_of`: each entry with a
/// place in a known system, by [`entry_key`]. The places its location queries lead to count as places of the entry
/// (placed by `bodies`), also where the entry names no place of its own.
fn offered_by(
    all: &ContractFacts,
    bodies: &Bodies,
    mut places_of: impl FnMut(&Facts) -> Vec<Place>,
) -> HashMap<String, crate::Offered> {
    let mut led_to: HashMap<&str, BTreeSet<Place>> = HashMap::new();
    let mut found = HashMap::new();
    for ((contract, sub), facts) in all.iter() {
        let mut places = places_of(facts);
        for query in &facts.queries {
            let at = led_to.entry(query).or_insert_with(|| {
                all.query(query).map(|query| all.targets().places(query, bodies)).unwrap_or_default()
            });
            places.extend(at.iter().cloned());
        }
        let mut offered = crate::Offered::default();
        for (system, planet) in places {
            offered.systems.insert(system.clone());
            if let Some(planet) = planet {
                offered.planets.entry(system).or_default().insert(planet);
            }
        }
        if !offered.systems.is_empty() {
            let sub = sub.map(|sub| sub.to_string());
            found.insert(entry_key(&contract.to_string(), sub.as_deref()), offered);
        }
    }
    found
}

/// The most steps [`whereabouts`] goes up the star map.
const STAR_MAP_DEPTH: usize = 16;

/// The folder of the system a star map record lies in, `libs/foundry/records/starmap/pu/system/nyx/` in lower case;
/// for the object of a whole system beside those folders (`…/starmap/pu/nyxsolarsystem`) the folder of its system.
/// `None` for anything else.
fn system_folder(record_path: &str) -> Option<String> {
    const MARK: &str = "/system/";
    let path = record_path.to_ascii_lowercase();
    if let Some(at) = path.find(MARK).map(|at| at + MARK.len()) {
        let end = at + path[at..].find('/')?;
        return Some(path[..=end].to_string());
    }
    let (parent, last) = path.rsplit_once('/')?;
    let name = last.strip_suffix("solarsystem").filter(|name| !name.is_empty())?;
    Some(format!("{parent}{MARK}{name}/"))
}

/// The English name of a star map object; `None` for a placeholder key or an empty text.
fn english_name(root: Option<&StructValue<'_>>, texts: &Localization) -> Option<String> {
    locale_key(root, "name")
        .filter(|key| !is_placeholder_key(key))
        .and_then(|key| filled(key, texts))
        .map(|name| py_strip(&name).to_string())
}

/// The English names of the system and of the planet a star map object lies in. The planet is the first object of
/// `navIcon` `Planet` going up its parents (the object itself included). The system is the star of the system folder
/// the object lies in (`systems`, folder → name; the star of Nyx has no child, and its places hang under a hidden
/// object of the whole system); else the first star going up; else that hidden object at the top. `None` where no
/// system is found or its name is a placeholder.
fn whereabouts(
    object: Guid,
    systems: &HashMap<String, String>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) -> Option<(String, Option<String>)> {
    let (mut planet, mut star, mut top) = (None, None, None);
    let mut folder = None;
    let mut current = object;
    for _ in 0..STAR_MAP_DEPTH {
        let tree = trees.get(current)?;
        let root = Some(tree.root());
        if folder.is_none() {
            folder = system_folder(tree.record_path()).and_then(|folder| systems.get(&folder).cloned());
        }
        match text(root, "navIcon") {
            Some("Planet") if planet.is_none() => planet = english_name(root, texts),
            Some("Star") if star.is_none() => star = english_name(root, texts),
            _ => {}
        }
        match reference(root, "parent") {
            Some(parent) => current = parent,
            None => {
                if boolean(root, "hideInStarmap") == Some(true) {
                    top = english_name(root, texts);
                }
                break;
            }
        }
    }
    Some((folder.or(star).or(top)?, planet))
}

fn scenario_sources(
    tree: &RecordTree<'_>,
    pools: &HashMap<Guid, Pool>,
    found: &mut Found,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) {
    for tiers in structs(Some(tree.root()), "factionRewardTiers") {
        let faction = trees.follow(Some(tiers), "faction").map(|faction| {
            let reputation = trees.follow(Some(faction.root()), "factionReputationRef");
            let key = reputation
                .as_deref()
                .and_then(|reputation| locale_key(Some(reputation.root()), "displayName"))
                .or_else(|| locale_key(Some(faction.root()), "name"));
            Text::resolve(key, texts)
        });
        for progression in structs(Some(tiers), "tierProgressions") {
            for tier in elements(Some(progression), "tierRewards").iter().filter_map(as_struct) {
                let rewards: Vec<(Guid, Option<f64>)> =
                    elements(Some(tier), "blueprintPool").iter().filter_map(link).map(|guid| (guid, None)).collect();
                if !rewards.iter().any(|(pool, _)| pools.contains_key(pool)) {
                    continue;
                }
                found.scenario_tiers += 1;
                let mission = Text::resolve(locale_key(Some(progression), "progressionText"), texts);
                let source = Source {
                    kind: SourceKind::Scenario,
                    id: class_name(tree).to_string(),
                    record_path: tree.record_path().to_string(),
                    title: py_strip(mission.shown()).to_string(),
                    mission,
                    mission_type: None,
                    contractor: None,
                    faction: faction.clone(),
                    description_key: None,
                    shown: ShownKeys::default(),
                    systems: Vec::new(),
                    places: Vec::new(),
                    rank: None,
                    min_reputation: None,
                    max_rank: None,
                    max_reputation: None,
                    shareable: None,
                    cooldown: None,
                    reputation_gain: None,
                    min_points: integer(Some(tier), "minPoints"),
                    reward_uec: None,
                    chance: None,
                    pool: String::new(),
                    uncertain: Vec::new(),
                };
                award(found, pools, &source, &rewards);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use p4kx_dataforge::Guid;

    use super::{entry_key, fill_placeholders, filled_contractor, fix_duplicates, offered_by, system_folder};
    use crate::catalog::Text;
    use crate::contract_facts::ContractFacts;
    use crate::location_targets::Bodies;

    #[test]
    fn a_star_map_record_lies_in_the_folder_of_its_system() {
        let folder = Some("libs/foundry/records/starmap/pu/system/nyx/".to_string());
        assert_eq!(system_folder("libs/foundry/records/starmap/pu/system/nyx/nyxstar"), folder);
        assert_eq!(system_folder("libs/foundry/records/starmap/pu/system/Nyx/delamar/starmapobject.delamar"), folder);
        assert_eq!(system_folder("libs/foundry/records/starmap/pu/NyxSolarSystem"), folder, "the whole system");
        assert_eq!(system_folder("libs/foundry/records/starmap/pu/solarsystem"), None);
        assert_eq!(system_folder("libs/foundry/records/starmap/pu/somewhere"), None);
    }
    use crate::corrections::{CatalogFixes, Correction, Field, Section};

    fn guid(last: u8) -> Guid {
        format!("10000000-0000-4000-8000-0000000000{last:02x}").parse().unwrap()
    }

    fn duplicate(pool: u8, twice: u8, replacement: Option<u8>) -> Correction {
        Correction {
            section: Section::Pool,
            record: guid(pool).to_string(),
            field: Field::PoolEntry { blueprint: guid(twice).to_string(), index: None },
            raw: None,
            expected: replacement.map(|r| guid(r).to_string()),
            missing_target: false,
            reason: String::new(),
            build: None,
            source: "scmdb",
        }
    }

    const OWN_GUID: &str = "10000000-0000-4000-8000-000000000002";
    const ONLY_TARGETS: &str = "10000000-0000-4000-8000-0000000000a1";
    const BROAD_GUID: &str = "10000000-0000-4000-8000-0000000000b1";
    const QUERY_COLONIAL: &str = "20000000-0000-4000-8000-000000000001";
    const QUERY_BROAD: &str = "20000000-0000-4000-8000-000000000002";
    const QUERY_NOWHERE: &str = "20000000-0000-4000-8000-000000000003";

    /// Entries that only search places (`ONLY_TARGETS`, `BROAD_GUID`) and one that is offered somewhere and searches
    /// another place besides (`OWN_GUID`), with the templates the queries find.
    fn facts_with_queries() -> ContractFacts {
        let entry = |guid: &str, queries: &[&str]| {
            let queries: Vec<String> = queries.iter().map(|query| format!("{{\"queryGuid\": \"{query}\"}}")).collect();
            format!(
                "{{\"guid\": \"{guid}\", \"subContractGuid\": null, \"locationQueries\": [{}]}}",
                queries.join(", ")
            )
        };
        let search = |guid: &str, tag: &str| {
            format!(
                "{{\"guid\": \"{guid}\", \"conditions\": [{{\"condition\": \"tagSearch\", \"tagType\": \"General\", \
                  \"terms\": [{{\"positiveTags\": [\"{tag}\"], \"negativeTags\": []}}]}}]}}"
            )
        };
        let template = |name: &str, tag: &str| {
            format!("{{\"className\": \"{name}\", \"disabled\": false, \"tags\": [\"{tag}\"]}}")
        };
        let mut templates = vec![
            template("Outpost_Stanton2c_Layout", "Missions.LocationType.Outpost.Colonial"),
            template("Outpost_Pyro5a", "Missions.LocationType.Outpost.Colonial"),
            template("Station_Nyx2", "Missions.LocationType.Station"),
        ];
        templates.extend(
            (0..=crate::location_targets::BROAD_QUERY)
                .map(|n| template(&format!("Anywhere{n}_Nyx"), "Missions.LocationType.Any")),
        );
        let catalog = format!(
            "{{\"contracts\": [{}, {}, {}], \"contractLocationQueries\": [{}, {}, {}]}}",
            entry(ONLY_TARGETS, &[QUERY_COLONIAL, QUERY_NOWHERE]),
            entry(OWN_GUID, &[QUERY_COLONIAL]),
            entry(BROAD_GUID, &[QUERY_BROAD]),
            search(QUERY_COLONIAL, "Missions.LocationType.Outpost"),
            search(QUERY_BROAD, "Missions.LocationType.Any"),
            search(QUERY_NOWHERE, "Missions.LocationType.Nothing"),
        );
        let templates = format!("{{\"missionLocationTemplates\": [{}]}}", templates.join(", "));
        ContractFacts::from_catalog(catalog.as_bytes()).unwrap().with_locations(templates.as_bytes()).unwrap()
    }

    fn bodies() -> Bodies {
        let mut bodies = Bodies::new([
            ("stanton".to_string(), "Stanton".to_string()),
            ("pyro".to_string(), "Pyro".to_string()),
            ("nyx".to_string(), "Nyx".to_string()),
        ]);
        for (record, planet) in [("Stanton2c", "Crusader"), ("Pyro5a", "Pyro V"), ("Nyx2", "Nyx II")] {
            let code = bodies.body_code(record).unwrap();
            bodies.add_body(code, Some(planet.to_string()));
        }
        bodies
    }

    fn systems_of(offered: &crate::Offered) -> Vec<&str> {
        offered.systems.iter().map(String::as_str).collect()
    }

    #[test]
    fn an_entry_that_only_searches_places_is_offered_where_its_queries_lead() {
        let found = offered_by(&facts_with_queries(), &bodies(), |_| Vec::new());
        let entry = &found[&entry_key(ONLY_TARGETS, None)];
        assert_eq!(systems_of(entry), ["Pyro", "Stanton"], "a template below the searched tag counts");
        assert_eq!(entry.planets["Stanton"].iter().collect::<Vec<_>>(), ["Crusader"], "Yela lies at Crusader");
        assert_eq!(entry.planets["Pyro"].iter().collect::<Vec<_>>(), ["Pyro V"]);
        assert!(!found.contains_key(&entry_key("10000000-0000-4000-8000-0000000000ff", None)), "unknown entry");
    }

    #[test]
    fn the_places_of_an_entry_and_those_its_queries_lead_to_are_joined() {
        let found = offered_by(&facts_with_queries(), &bodies(), |facts| {
            if facts.queries.is_empty() { Vec::new() } else { vec![("Nyx".to_string(), Some("Nyx II".to_string()))] }
        });
        let entry = &found[&entry_key(OWN_GUID, None)];
        assert_eq!(systems_of(entry), ["Nyx", "Pyro", "Stanton"]);
        assert_eq!(entry.planets["Nyx"].iter().collect::<Vec<_>>(), ["Nyx II"]);
    }

    #[test]
    fn a_query_that_finds_too_many_places_or_none_adds_nothing() {
        let found = offered_by(&facts_with_queries(), &bodies(), |_| Vec::new());
        assert!(!found.contains_key(&entry_key(BROAD_GUID, None)), "a search over every place names no place");
        let entry = &found[&entry_key(ONLY_TARGETS, None)];
        assert!(!entry.systems.contains("Nyx"), "the query that finds nothing leaves the entry as it is");
    }

    #[test]
    fn a_blueprint_listed_twice_is_replaced_or_dropped_at_its_second_place() {
        let mut found = vec![guid(1), guid(2), guid(1), guid(3)];
        let replace = CatalogFixes::new(&[duplicate(9, 1, Some(7))], None);
        fix_duplicates(&mut found, guid(9), &replace);
        assert_eq!(found, [guid(1), guid(2), guid(7), guid(3)]);
        assert_eq!(replace.into_report().outcomes[0].1, crate::corrections::Outcome::Applied);

        let mut found = vec![guid(1), guid(2), guid(1), guid(3)];
        fix_duplicates(&mut found, guid(9), &CatalogFixes::new(&[duplicate(9, 1, None)], None));
        assert_eq!(found, [guid(1), guid(2), guid(3)], "no meant blueprint is known: the second place goes");

        // Another pool, or a blueprint listed once: nothing to fix.
        let mut found = vec![guid(1), guid(2)];
        let fixes = CatalogFixes::new(&[duplicate(9, 1, Some(7)), duplicate(8, 1, Some(7))], None);
        fix_duplicates(&mut found, guid(9), &fixes);
        assert_eq!(found, [guid(1), guid(2)]);
    }

    #[test]
    fn a_contractor_is_filled_like_a_title_and_never_shown_raw() {
        let fill = |token: &str| (token == "Contractor|Known").then(|| "Acme Haulers".to_string());
        let text = |key: &str, text: Option<&str>| Text { key: Some(key.into()), text: text.map(str::to_string) };

        let plain = filled_contractor(text("@Acme_from", Some("Acme")), fill).unwrap();
        assert_eq!(plain.shown(), "Acme");

        let filled = filled_contractor(text("@Known_from", Some("~mission(Contractor|Known)")), fill).unwrap();
        assert_eq!(filled.shown(), "Acme Haulers");
        assert_eq!(filled.key.as_deref(), Some("@Known_from"));

        assert!(filled_contractor(text("@Other_from", Some("~mission(Contractor|Other)")), fill).is_none());
        assert!(filled_contractor(text("@Owner_from", Some("From ~mission(Owner)")), fill).is_none());
        assert!(filled_contractor(text("@Empty_from", None), fill).is_none(), "no text: not the key");
        assert!(filled_contractor(text("@Blank_from", Some("  ")), fill).is_none());
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_stay() {
        let fill = |token: &str| match token {
            "Location" | "Location|Address" => Some("[LOCATION]".to_string()),
            "Danger" => Some("High-Risk Target".to_string()),
            _ => None,
        };
        assert_eq!(fill_placeholders("Target at ~mission(Location) ", fill), "Target at [LOCATION]");
        assert_eq!(
            fill_placeholders("Verified Bounty: ~mission(TargetName) | ~mission(Danger)", fill),
            "Verified Bounty: ~mission(TargetName) | High-Risk Target"
        );
        assert_eq!(fill_placeholders("~mission(Location|Address) needs help", fill), "[LOCATION] needs help");
        assert_eq!(
            fill_placeholders(" Campaign to reclaim XenoThreat outposts ", fill),
            "Campaign to reclaim XenoThreat outposts"
        );
        assert_eq!(fill_placeholders("Broken ~mission(Location", fill), "Broken ~mission(Location");
        assert_eq!(fill_placeholders("", fill), "");
    }
}
