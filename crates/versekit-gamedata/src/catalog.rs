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

//! The blueprint catalog: every crafting blueprint with the item it makes
//! and the contracts that award it.

use std::collections::{HashMap, HashSet};

use p4kx_dataforge::{DataForge, Guid, RecordTree};
use p4kx_localization::is_placeholder_key;
use versekit_core::text::py_strip;

use crate::Localization;
use crate::contract_facts::ContractFacts;
use crate::corrections::{CatalogFixes, Report};
use crate::fields::{Trees, class_name, component, elements, integer, link, locale_key, structure, text};
use crate::sources::{self, Inputs};

/// The record path prefix of the blueprints that are recipes.
pub(crate) const BLUEPRINT_PREFIX: &str = "libs/foundry/records/crafting/blueprints/crafting/";

/// Words that mark a record as a development leftover when its record path or
/// name holds one of them as a whole word, ignoring ASCII case.
const LEFTOVER_WORDS: [&str; 6] = ["test", "template", "dummy", "debug", "deprecated", "dev"];

/// A localisation key with its text in the chosen language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Text {
    /// The key as the record holds it, e.g. `@item_NameBEHR_LMG`; `None` when
    /// the record holds none.
    pub key: Option<String>,
    /// The text of the key, `None` for a placeholder key or a key without text.
    pub text: Option<String>,
}

impl Text {
    pub(crate) fn resolve(key: Option<&str>, texts: &Localization) -> Self {
        let text = key.filter(|key| !is_placeholder_key(key)).and_then(|key| texts.get(key));
        Self { key: key.map(str::to_string), text }
    }

    /// The text, else the key, else an empty string; a placeholder key such
    /// as `@LOC_PLACEHOLDER` is never returned.
    pub fn shown(&self) -> &str {
        let key = self.key.as_deref().filter(|key| !is_placeholder_key(key));
        self.text.as_deref().or(key).unwrap_or_default()
    }
}

/// The manufacturer of an item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manufacturer {
    /// The record GUID.
    pub guid: String,
    /// The record name without its type, e.g. `BEHR`.
    pub class_name: String,
    /// The short code, e.g. `BEH`.
    pub code: Option<String>,
    /// The name.
    pub name: Text,
    /// Whether the name was corrected (the record's own name text is the
    /// one of another maker).
    pub corrected: bool,
}

/// The item a blueprint makes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// The record GUID.
    pub guid: String,
    /// The record name without its type, e.g. `behr_rifle_ballistic_01`.
    pub class_name: String,
    /// The record path, without `.xml`.
    pub record_path: String,
    /// The name.
    pub name: Text,
    /// The item type, e.g. `WeaponPersonal` or `Char_Armor_Torso`.
    pub kind: Option<String>,
    /// The item sub type, e.g. `Rifle`.
    pub sub_kind: Option<String>,
    /// The size.
    pub size: Option<i64>,
    /// The grade.
    pub grade: Option<i64>,
    /// The class from the `Class:` line of the English description, e.g.
    /// `Military`; `None` without that line.
    pub class: Option<String>,
    /// The grade the English description states (`Grade: C` is 3), from 1
    /// for A to 4 for D; `None` without one.
    pub described_grade: Option<i64>,
    /// The manufacturer the English description states (`Manufacturer: …`).
    pub described_manufacturer: Option<String>,
    /// The manufacturer.
    pub manufacturer: Option<Manufacturer>,
    /// The text key of the description the game shows for the item, without
    /// the leading `@`, e.g. `item_Desc_cds_heavy_armor_01_Shared`; several
    /// items can share one. `None` without one or for a placeholder key.
    pub description_key: Option<String>,
}

/// Where a source is defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// A contract of a contract generator.
    Contract,
    /// A reward tier of a scenario, reached with a number of points.
    Scenario,
}

/// One way to get a blueprint: a contract or a scenario tier whose reward
/// pool holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// Contract or scenario tier.
    pub kind: SourceKind,
    /// The debug name of the contract, or the record name of the scenario.
    pub id: String,
    /// The record path of the contract generator or scenario.
    pub record_path: String,
    /// The contract title, or the progression text of a scenario.
    pub mission: Text,
    /// The title as a list shows it: each `~mission(…)` placeholder the data
    /// fixes is filled in, one the game picks when it offers the contract is
    /// named in brackets (`[LOCATION]`, `[TARGET]`, `[SHIP]`); without
    /// leading and trailing white space.
    pub title: String,
    /// The mission type, e.g. `Bounty Hunter`.
    pub mission_type: Option<Text>,
    /// Who offers the contract: its `Contractor` text, else the name of the
    /// organisation it names as contractor.
    pub contractor: Option<Text>,
    /// The faction whose reputation the contract counts toward.
    pub faction: Option<Text>,
    /// The text key of the contract's description, without `@` (the parameter `Description`); `None` where the data
    /// name none.
    pub description_key: Option<String>,
    /// The star systems the contract is offered in: the stars among the places the data list (their map icon is a
    /// star), in stored order.
    pub systems: Vec<Text>,
    /// The places the contract is offered at: planets, moons, stations, in
    /// stored order; the systems are in [`Source::systems`].
    pub places: Vec<Text>,
    /// The lowest reputation rank that gets the contract.
    pub rank: Option<Text>,
    /// The reputation that rank starts at.
    pub min_reputation: Option<i64>,
    /// The highest reputation rank that still gets the contract: above it
    /// the contract is not offered any more.
    pub max_rank: Option<Text>,
    /// The reputation that rank starts at.
    pub max_reputation: Option<i64>,
    /// Whether the contract can be shared in a party; `None` where the data
    /// does not say.
    pub shareable: Option<bool>,
    /// The minutes until the contract can be taken again, where the player
    /// has a personal cooldown for it.
    pub cooldown: Option<i64>,
    /// The reputation the contract gives on success, where the data states
    /// fixed amounts: the sum of all its reputation results whose first
    /// mission result flag is set.
    pub reputation_gain: Option<i64>,
    /// The points a scenario tier needs.
    pub min_points: Option<i64>,
    /// The fixed reward in aUEC, where the data states one; computed rewards
    /// are `None`.
    pub reward_uec: Option<i64>,
    /// The chance that the reward pool is awarded, from 0 to 1.
    pub chance: Option<f64>,
    /// The record name of the reward pool.
    pub pool: String,
}

/// One crafting blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    /// The record GUID.
    pub guid: String,
    /// The record name without its type, e.g. `BP_CRAFT_behr_rifle_ballistic_01`.
    pub class_name: String,
    /// The record path, without `.xml`.
    pub record_path: String,
    /// The blueprint's own name; when it has no text, the item's name, and
    /// when that has none either, the class name as text.
    pub name: Text,
    /// Whether every player has the blueprint from the start
    /// (`CraftingGlobalParams.defaultBlueprintSelection`).
    pub default: bool,
    /// The item the blueprint makes; `None` when its record is missing.
    pub item: Option<Item>,
    /// The contracts and scenario tiers that award the blueprint, lowest
    /// reputation first.
    pub sources: Vec<Source>,
    /// The record names of the reward pools that list the blueprint, also
    /// those no released source awards; in record order.
    pub pools: Vec<String>,
}

/// The blueprints of one archive.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    /// The blueprints, ordered by class name ignoring ASCII case.
    pub blueprints: Vec<Blueprint>,
    /// The blueprint records left out as development leftovers.
    pub left_out: usize,
    /// The reward pools that hold blueprints.
    pub pools: usize,
    /// The blueprints any reward pool lists, also those no released source
    /// awards.
    pub in_pools: usize,
    /// The released contracts that award a reward pool.
    pub contracts: usize,
    /// The contracts that award a reward pool but are marked not for release
    /// or work in progress, themselves or their handler; they give no source.
    pub unreleased_contracts: usize,
    /// The scenario tiers that award a reward pool.
    pub scenario_tiers: usize,
    /// The records that could not be read: record path, GUID and reason.
    pub failed: Vec<String>,
    /// What became of the corrections the reading was given.
    pub corrections: Report,
    /// The keys every contract entry shows where they differ from those it names, by
    /// [`crate::sources::entry_key`].
    pub shown_keys: HashMap<String, crate::sources::ShownKeys>,
}

pub(crate) fn read(forge: &DataForge<'_>, texts: &Localization, fixes: CatalogFixes, facts: &ContractFacts) -> Catalog {
    let mut trees = Trees::new(forge);
    let mut inputs = Inputs::default();
    let mut blueprints = Vec::new();
    let mut globals = Vec::new();
    let mut left_out = 0;
    for record in forge.records() {
        let (Ok(type_name), Ok(path), Ok(name)) = (record.type_name(), record.record_path(), record.name()) else {
            continue;
        };
        match type_name {
            "CraftingBlueprintRecord" if starts_with_ignore_case(path, BLUEPRINT_PREFIX) => {
                if is_leftover(path) || is_leftover(name) {
                    left_out += 1;
                } else {
                    blueprints.push(record.guid());
                }
            }
            "CraftingGlobalParams" => globals.push(record.guid()),
            "BlueprintPoolRecord" => inputs.pools.push(record.guid()),
            "ContractGenerator" => inputs.generators.push(record.guid()),
            "ScenarioProgress" => inputs.scenarios.push(record.guid()),
            _ => {}
        }
    }
    let defaults: HashSet<Guid> = globals
        .into_iter()
        .filter_map(|guid| trees.get(guid))
        .flat_map(|tree| {
            let selection = structure(Some(tree.root()), "defaultBlueprintSelection");
            elements(selection, "blueprintRecords").iter().filter_map(link).collect::<Vec<_>>()
        })
        .collect();
    let found = sources::collect(&mut trees, &inputs, texts, &fixes, facts);
    let mut by_blueprint = found.by_blueprint;
    let mut pools_by_blueprint = found.pools_by_blueprint;
    let mut items = HashMap::new();
    let mut catalog = Catalog {
        left_out,
        pools: found.pools,
        in_pools: found.in_pools,
        contracts: found.contracts,
        unreleased_contracts: found.unreleased_contracts,
        scenario_tiers: found.scenario_tiers,
        ..Catalog::default()
    };
    for guid in blueprints {
        let Some(tree) = trees.get(guid) else { continue };
        let sources = by_blueprint.remove(&guid).unwrap_or_default();
        let mut entry = blueprint(&tree, sources, &mut trees, &mut items, texts, &fixes);
        entry.default = defaults.contains(&guid);
        entry.pools = pools_by_blueprint.remove(&guid).unwrap_or_default();
        catalog.blueprints.push(entry);
    }
    catalog.blueprints.sort_by_cached_key(|blueprint| blueprint.class_name.to_ascii_lowercase());
    catalog.shown_keys = sources::shown_keys(&mut trees, &inputs.generators, texts);
    catalog.failed = trees.into_failed();
    catalog.corrections = fixes.into_report();
    catalog
}

fn blueprint(
    tree: &RecordTree<'_>,
    sources: Vec<Source>,
    trees: &mut Trees<'_, '_>,
    items: &mut HashMap<Guid, Option<Item>>,
    texts: &Localization,
    fixes: &CatalogFixes,
) -> Blueprint {
    let definition = structure(Some(tree.root()), "blueprint");
    let creation = structure(definition, "processSpecificData");
    let item = crate::fields::reference(creation, "entityClass").and_then(|guid| {
        items.entry(guid).or_insert_with(|| trees.get(guid).map(|item| self::item(&item, trees, texts, fixes))).clone()
    });
    let mut name = Text::resolve(locale_key(definition, "blueprintName"), texts);
    if name.text.is_none()
        && let Some(item) = &item
        && item.name.text.is_some()
    {
        name = item.name.clone();
    }
    if name.text.is_none() {
        name.text = Some(class_name(tree).to_string());
    }
    // A label the data got wrong: the item is right, its name is not.
    if let Some(meant) = fixes.name(&tree.guid().to_string()) {
        name = Text { key: name.key, text: Some(meant.to_string()) };
    }
    Blueprint {
        guid: tree.guid().to_string(),
        class_name: class_name(tree).to_string(),
        record_path: tree.record_path().to_string(),
        name,
        default: false,
        item,
        sources,
        pools: Vec::new(),
    }
}

fn item(tree: &RecordTree<'_>, trees: &mut Trees<'_, '_>, texts: &Localization, fixes: &CatalogFixes) -> Item {
    let definition = structure(component(tree, "SAttachableComponentParams"), "AttachDef");
    let localization = structure(definition, "Localization");
    let description = locale_key(localization, "Description").and_then(|key| texts.english(key));
    let guid = tree.guid().to_string();
    let manufacturer = trees.follow(definition, "Manufacturer").map(|maker| {
        let record = maker.guid().to_string();
        let mut name = Text::resolve(locale_key(structure(Some(maker.root()), "Localization"), "Name"), texts);
        // A maker the data names wrongly: the record's own name text is the
        // one of another maker.
        let mut corrected = false;
        if let Some(meant) = fixes.maker(&guid, Some(&record), name.shown()) {
            name = Text { key: name.key, text: Some(meant.to_string()) };
            corrected = true;
        }
        Manufacturer {
            guid: record,
            class_name: class_name(&maker).to_string(),
            code: text(Some(maker.root()), "Code").map(str::to_string),
            name,
            corrected,
        }
    });
    Item {
        kind: fixes.item_kind(&guid).map(str::to_string).or_else(|| text(definition, "Type").map(str::to_string)),
        guid,
        class_name: class_name(tree).to_string(),
        record_path: tree.record_path().to_string(),
        name: Text::resolve(locale_key(localization, "Name"), texts),
        sub_kind: text(definition, "SubType").map(str::to_string),
        size: integer(definition, "Size"),
        grade: integer(definition, "Grade"),
        class: description.as_deref().and_then(class_line),
        described_grade: description.as_deref().and_then(|d| described(d, "Grade")).and_then(grade_number),
        described_manufacturer: description.as_deref().and_then(|d| described(d, "Manufacturer")),
        manufacturer,
        description_key: text_key(locale_key(localization, "Description")),
    }
}

/// A text key as the `global.ini` names it: without the leading `@`; `None`
/// for a placeholder key or an empty one.
pub(crate) fn text_key(key: Option<&str>) -> Option<String> {
    let key = key.filter(|key| !is_placeholder_key(key))?;
    let key = key.strip_prefix('@').unwrap_or(key);
    (!key.is_empty()).then(|| key.to_string())
}

/// The value after the first `<field>:` anywhere in `description`, up to the
/// end of its line, trimmed; `None` when it is missing or empty.
fn described(description: &str, field: &str) -> Option<String> {
    let at = description.find(&format!("{field}:"))? + field.len() + 1;
    let value = description[at..].lines().next().unwrap_or_default();
    Some(py_strip(value).to_string()).filter(|value| !value.is_empty())
}

/// The grade letter `A` to `D` as 1 to 4.
fn grade_number(letter: String) -> Option<i64> {
    match letter.to_ascii_uppercase().as_str() {
        "A" => Some(1),
        "B" => Some(2),
        "C" => Some(3),
        "D" => Some(4),
        _ => None,
    }
}

/// The value of the first line of `description` that starts with `Class:`
/// after leading white space, trimmed; `None` without such a line or for an
/// empty value.
fn class_line(description: &str) -> Option<String> {
    description
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("Class:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn starts_with_ignore_case(path: &str, prefix: &str) -> bool {
    path.len() >= prefix.len()
        && path.bytes().zip(prefix.bytes()).all(|(a, b)| {
            let a = if a == b'\\' { b'/' } else { a.to_ascii_lowercase() };
            a == b
        })
}

/// Whether `text` holds one of [`LEFTOVER_WORDS`] as a run of ASCII letters
/// and digits, ignoring case.
pub(crate) fn is_leftover(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| LEFTOVER_WORDS.iter().any(|leftover| word.eq_ignore_ascii_case(leftover)))
}

#[cfg(test)]
mod tests {
    use super::{Text, class_line, described, grade_number, is_leftover, starts_with_ignore_case, text_key};

    fn text(key: Option<&str>, text: Option<&str>) -> Text {
        Text { key: key.map(str::to_string), text: text.map(str::to_string) }
    }

    #[test]
    fn shown_never_returns_a_placeholder_key() {
        assert_eq!(text(Some("@LOC_PLACEHOLDER"), None).shown(), "");
        assert_eq!(text(Some("@loc_empty"), None).shown(), "");
        assert_eq!(text(Some("@item_Missing"), None).shown(), "@item_Missing");
        assert_eq!(text(Some("@LOC_EMPTY"), Some("Rifle")).shown(), "Rifle");
        assert_eq!(text(None, None).shown(), "");
    }

    #[test]
    fn leftover_words_match_whole_words_only() {
        assert!(is_leftover("libs/foundry/records/crafting/blueprints/crafting/test/bp_craft_x"));
        assert!(is_leftover("BP_CRAFT_paint_TEST_01"));
        assert!(!is_leftover("BP_CRAFT_contestant_armor"));
        assert!(!is_leftover("libs/foundry/records/crafting/blueprints/crafting/$templates/bp_craft_x"));
    }

    #[test]
    fn class_line_is_read_from_its_own_line() {
        assert_eq!(class_line("Manufacturer: X\n  Class: Military \nSize: 1").as_deref(), Some("Military"));
        assert_eq!(class_line("Manufacturer: X\nSize: 1"), None);
        assert_eq!(class_line("Class:   "), None);
    }

    #[test]
    fn grade_and_manufacturer_are_read_from_the_description() {
        let description = "Manufacturer: Wen/Cassel Propulsion \nItem Type: Cooler\nGrade: c\nSize: 1";
        assert_eq!(described(description, "Manufacturer").as_deref(), Some("Wen/Cassel Propulsion"));
        assert_eq!(described(description, "Grade").and_then(grade_number), Some(3));
        assert_eq!(described("Grade: Bespoke", "Grade").and_then(grade_number), None);
        assert_eq!(described("Size: 1\nGrade:", "Grade"), None);
        assert_eq!(described("Size: 1", "Manufacturer"), None);
    }

    #[test]
    fn record_path_prefix_ignores_case_and_separator() {
        let prefix = "libs/foundry/records/crafting/blueprints/crafting/";
        assert!(starts_with_ignore_case("Libs\\Foundry\\Records\\Crafting\\Blueprints\\Crafting\\x", prefix));
        assert!(!starts_with_ignore_case("libs/foundry/records/crafting/blueprints/dismantle/x", prefix));
    }

    #[test]
    fn the_description_key_loses_its_at_sign_and_placeholders_count_as_none() {
        assert_eq!(
            text_key(Some("@item_Desc_cds_heavy_armor_01_Shared")).as_deref(),
            Some("item_Desc_cds_heavy_armor_01_Shared")
        );
        assert_eq!(text_key(Some("item_DescCOOL_A")).as_deref(), Some("item_DescCOOL_A"));
        assert_eq!(text_key(Some("@LOC_EMPTY")), None);
        assert_eq!(text_key(Some("@LOC_PLACEHOLDER")), None);
        assert_eq!(text_key(Some("@")), None);
        assert_eq!(text_key(None), None);
    }
}
