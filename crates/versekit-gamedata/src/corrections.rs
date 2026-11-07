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

//! Known errors in CIG's game data and what is meant there.
//!
//! The model follows krt-profit/basetool#2899: a correction names the record by GUID,
//! the field, where it is a list the index, the raw value CIG shipped where
//! it is known, the meant value and the reason. A reference whose record
//! does not exist is a case of its own; where the cause lies in another
//! record (a manufacturer the items point to), the correction names that
//! one.
//!
//! The starting stock is scmdb's list `cig_data_issues-<build>.json`,
//! fetched at run time and valid for exactly its build; VerseKit adds the
//! cases scmdb does not know yet ([`built_in`]). Applying them to the
//! [`GameValues`] marks every corrected value (`corrected`), so the user
//! interface can show that and why; the report says which corrections no
//! longer fit, because CIG fixed the data or the raw value changed.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Map, Value};
use versekit_core::scmdb::FetchError;

use crate::values::{Corrected, GameValues, Modifier};

/// The name of the error list of `build` at scmdb.
pub fn scmdb_file(build: &str) -> String {
    format!("cig_data_issues-{build}.json")
}

/// The kind of record a correction belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// A crafting blueprint record.
    Blueprint,
    /// A blueprint pool record.
    Pool,
    /// An entity (item) record.
    Entity,
    /// A contract, by its debug name.
    Contract,
    /// A manufacturer record: the cause of wrong manufacturer names.
    Manufacturer,
}

/// Which value of a modifier range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeValue {
    /// `startQuality`.
    StartQuality,
    /// `endQuality`.
    EndQuality,
    /// `modifierAtStart`.
    AtStart,
    /// `modifierAtEnd`.
    AtEnd,
}

impl RangeValue {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "startQuality" => Some(Self::StartQuality),
            "endQuality" => Some(Self::EndQuality),
            "modifierAtStart" => Some(Self::AtStart),
            "modifierAtEnd" => Some(Self::AtEnd),
            _ => None,
        }
    }

    /// The field name as scmdb and the marks name it.
    pub const fn key(self) -> &'static str {
        match self {
            Self::StartQuality => "startQuality",
            Self::EndQuality => "endQuality",
            Self::AtStart => "modifierAtStart",
            Self::AtEnd => "modifierAtEnd",
        }
    }

    fn of(self, range: &mut Modifier) -> &mut f64 {
        match self {
            Self::StartQuality => &mut range.start_quality,
            Self::EndQuality => &mut range.end_quality,
            Self::AtStart => &mut range.at_start,
            Self::AtEnd => &mut range.at_end,
        }
    }
}

/// The field a correction changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Field {
    /// The entity a blueprint makes.
    Product,
    /// The name a blueprint shows (the item is right, its label wrong).
    Name,
    /// One value of a modifier range in a slot of a blueprint.
    Modifier {
        /// The slot name, compared exactly.
        slot: String,
        /// The property key, e.g. `armor_damagemitigation`.
        property_key: String,
        /// The index of the range among those of the property in the slot;
        /// `None` when there must be exactly one.
        index: Option<usize>,
        /// Which value.
        value: RangeValue,
    },
    /// The attach type of an entity.
    AttachType,
    /// The crafting item type of an entity (scmdb `cgItemType`).
    CgItemType,
    /// The manufacturer name of an entity or of a manufacturer record.
    Manufacturer,
    /// A blueprint reference in a pool: the second of two equal ones.
    PoolEntry {
        /// The blueprint referenced twice.
        blueprint: String,
        /// The index of the entry in the pool, `None` for the second
        /// occurrence.
        index: Option<usize>,
    },
    /// The faction a contract counts toward.
    Faction,
}

/// One known error in CIG's data.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// The kind of record.
    pub section: Section,
    /// The GUID of the record in lower case; for a contract its debug name.
    pub record: String,
    /// The field.
    pub field: Field,
    /// The value CIG shipped, as text, where it is known.
    pub raw: Option<String>,
    /// The meant value, as text; `None` when no plausible one is known and
    /// the value is only marked (or, in a pool, the entry dropped).
    pub expected: Option<String>,
    /// Whether the raw value is a reference to a record that does not exist.
    pub missing_target: bool,
    /// The reason, in English.
    pub reason: String,
    /// The build the correction was made for; `None` for every build.
    pub build: Option<String>,
    /// Where it comes from: `scmdb` or `versekit`.
    pub source: &'static str,
}

/// What became of one correction when it was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The value was corrected and marked.
    Applied,
    /// The value was marked only: no meant value is known.
    Marked,
    /// The data already hold the meant value, or the missing target exists
    /// now: CIG fixed it.
    AlreadyRight,
    /// The value was corrected before (an earlier correction of the same
    /// field, e.g. the same case in two lists): nothing changed, no second
    /// mark.
    Repeated,
    /// The raw value differs from the known one: the data changed, the
    /// correction is held back until someone checks it.
    RawChanged,
    /// The record or the field was not found.
    NotFound,
    /// Several ranges fit and the correction names no index.
    Ambiguous,
    /// The correction belongs to data the values do not hold (names, pools,
    /// contracts); the catalog applies it.
    Elsewhere,
}

/// The outcome of every correction, in the order of the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// Correction and outcome.
    pub outcomes: Vec<(Correction, Outcome)>,
}

impl Report {
    /// How many corrections had `outcome`.
    pub fn count(&self, outcome: Outcome) -> usize {
        self.outcomes.iter().filter(|(_, found)| *found == outcome).count()
    }
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn note(entry: &Map<String, Value>) -> String {
    entry.get("note").and_then(Value::as_str).unwrap_or_default().to_string()
}

fn entries<'v>(file: &'v Value, section: &str) -> impl Iterator<Item = (String, &'v Map<String, Value>)> {
    file.get(section)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(key, value)| !key.starts_with('_') && value.is_object())
        .filter_map(|(key, value)| Some((key.clone(), value.as_object()?)))
}

/// The corrections of an scmdb error list (`cig_data_issues-<build>.json`)
/// for `build`.
pub fn from_scmdb(file: &Value, build: &str) -> Vec<Correction> {
    let new = |section, record: &str, field, expected: Option<String>, reason: &str| Correction {
        section,
        record: record.to_ascii_lowercase(),
        field,
        raw: None,
        expected,
        missing_target: false,
        reason: reason.to_string(),
        build: Some(build.to_string()),
        source: "scmdb",
    };
    let mut found = Vec::new();
    for (guid, entry) in entries(file, "blueprintRecords") {
        let reason = note(entry);
        let expected = entry.get("expected").and_then(Value::as_object);
        let slot = entry.get("slot").and_then(Value::as_str);
        let key = entry.get("propertyKey").and_then(Value::as_str);
        for (name, value) in expected.into_iter().flatten() {
            let field = match (slot, key) {
                (Some(slot), Some(key)) => match RangeValue::from_key(name) {
                    Some(value) => Field::Modifier { slot: slot.into(), property_key: key.into(), index: None, value },
                    None => continue,
                },
                _ => match name.as_str() {
                    "entityClass" => Field::Product,
                    "name" => Field::Name,
                    _ => continue,
                },
            };
            let expected =
                if field == Field::Product { text(value).map(|v| v.to_ascii_lowercase()) } else { text(value) };
            // A meant product or name of null names nothing to use (4.12
            // skips it as well); a range value always has one.
            if expected.is_none() {
                continue;
            }
            found.push(new(Section::Blueprint, &guid, field, expected, &reason));
        }
    }
    for (guid, entry) in entries(file, "blueprintPools") {
        for duplicate in entry.get("duplicateReferences").and_then(Value::as_array).into_iter().flatten() {
            let Some(duplicate) = duplicate.as_object() else { continue };
            let Some(blueprint) = duplicate.get("blueprintRecord").and_then(Value::as_str) else { continue };
            let expected = duplicate
                .get("expected")
                .and_then(|expected| expected.get("blueprintRecord"))
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase);
            let field = Field::PoolEntry { blueprint: blueprint.to_ascii_lowercase(), index: None };
            found.push(new(Section::Pool, &guid, field, expected, &note(duplicate)));
        }
    }
    for (guid, entry) in entries(file, "entities") {
        let reason = note(entry);
        for (name, value) in entry.get("expected").and_then(Value::as_object).into_iter().flatten() {
            let field = match name.as_str() {
                "attachType" => Field::AttachType,
                "cgItemType" => Field::CgItemType,
                "manufacturer" => Field::Manufacturer,
                _ => continue,
            };
            if let Some(expected) = text(value).filter(|text| !text.is_empty()) {
                found.push(new(Section::Entity, &guid, field, Some(expected), &reason));
            }
        }
    }
    for (name, entry) in entries(file, "contracts") {
        if entry.get("field").and_then(Value::as_str) != Some("factionGuid") {
            continue;
        }
        let expected = entry
            .get("expected")
            .and_then(|expected| expected.get("factionGuid"))
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase);
        let mut correction = new(Section::Contract, &name, Field::Faction, expected, &note(entry));
        correction.record = name;
        found.push(correction);
    }
    found
}

/// The corrections of scmdb's error list for `build`, fetched at run time
/// (see `versekit_core::scmdb::fetch_file`); `user_agent` names VerseKit.
///
/// # Errors
///
/// The fetch error, or [`FetchError::Failed`] when the file is no JSON.
pub fn fetch_scmdb(build: &str, user_agent: &str) -> Result<Vec<Correction>, FetchError> {
    let bytes = versekit_core::scmdb::fetch_file(&scmdb_file(build), user_agent)?;
    let file: Value = serde_json::from_slice(&bytes).map_err(|error| FetchError::Failed(error.to_string()))?;
    Ok(from_scmdb(&file, build))
}

/// The cases VerseKit knows beyond scmdb's list (found by Drachennarr with
/// an independent reader on build 4.10.197, krt-profit/basetool#2899). They carry the raw value,
/// so they hold back by themselves once CIG changes the data.
pub fn built_in() -> Vec<Correction> {
    vec![
        Correction {
            section: Section::Blueprint,
            record: "bb511044-6590-4e19-b077-a347cad104fa".into(),
            field: Field::Product,
            raw: Some("6fc982c0-2747-42b7-a628-7c76ecedf732".into()),
            expected: Some("d9870a05-1a66-4a9a-bec6-30eb67e3a3ce".into()),
            missing_target: false,
            reason: "BP_CRAFT_SHLD_RSI_S04_Polaris_SCItem makes the 890J shield (SHLD_ORIG_S04_890J_SCItem) \
                     instead of the Polaris shield (SHLD_RSI_S04_Polaris_SCItem); the Polaris shield has no \
                     blueprint, the 890J shield two."
                .into(),
            build: None,
            source: "versekit",
        },
        Correction {
            section: Section::Blueprint,
            record: "d7f3ad24-fca7-4393-b026-f55675cfe820".into(),
            field: Field::Product,
            raw: Some("3cf6753c-d69c-4980-a45e-724151407096".into()),
            expected: None,
            missing_target: true,
            reason: "BP_CRAFT_COOL_S04_CNOU_Pioneer refers to an entity that does not exist; no Pioneer cooler \
                     is defined in the game data."
                .into(),
            build: None,
            source: "versekit",
        },
        Correction {
            section: Section::Manufacturer,
            record: MXOX_RECORD.into(),
            field: Field::Manufacturer,
            raw: Some("Aegis Dynamics".into()),
            expected: Some("MaxOx".into()),
            missing_target: false,
            reason: "The manufacturer record scitemmanufacturer/mxox carries the code MXOX and the logo of MaxOx, \
                     but its name and description point to the Aegis Dynamics texts. Every item of that record \
                     shows the wrong maker; scmdb lists them one by one."
                .into(),
            build: None,
            source: "versekit",
        },
    ]
    .into_iter()
    .chain(FOXWELL_HAULING.iter().map(|contract| Correction {
        section: Section::Contract,
        record: (*contract).into(),
        field: Field::Faction,
        raw: None,
        expected: Some(FOXWELL_RECORD.into()),
        missing_target: false,
        reason: "The Foxwell hauling contract names no faction: its handler states none, it grants no \
                 reputation, and Foxwell Enforcement stands only in an excluding reputation prerequisite, \
                 which p4kx does not count as the giver."
            .into(),
        build: None,
        source: "versekit",
    }))
    .collect()
}

/// The GUID of the manufacturer record `scitemmanufacturer/mxox`, whose name
/// text is the one of Aegis Dynamics.
pub const MXOX_RECORD: &str = "de0d3594-9f18-42c0-8a90-6edd723d80f8";

/// The GUID of the faction reputation record of Foxwell Enforcement.
pub const FOXWELL_RECORD: &str = "016c2950-0292-468c-a408-a5283b52f158";

/// The Foxwell hauling contracts that name Foxwell Enforcement only in an
/// excluding reputation prerequisite.
pub const FOXWELL_HAULING: [&str; 3] = [
    "Foxwell_OLP_HaulCargo_AtoB_Small_Carbon",
    "Foxwell_PAF_HaulCargo_AtoB_Small_AgriculturalSupplies",
    "Foxwell_PAF_HaulCargo_Multi2ToSingle_Small_Aluminum",
];

/// The corrections for `build`, VerseKit's own first and then scmdb's list
/// of that build, fetched at run time; `error` says why scmdb's list is
/// missing. The own ones come first so that a cause named once (a
/// manufacturer record) is applied before the single items scmdb lists.
pub fn collect(build: Option<&str>, user_agent: &str) -> (Vec<Correction>, Option<FetchError>) {
    let mut list = built_in();
    let Some(build) = build else { return (list, Some(FetchError::Missing)) };
    match fetch_scmdb(build, user_agent) {
        Ok(theirs) => {
            list.extend(theirs);
            (list, None)
        }
        Err(error) => (list, Some(error)),
    }
}

/// The argument a hint text takes.
#[derive(Debug, Clone, PartialEq)]
pub enum HintArg {
    /// None.
    None,
    /// A number, for `{0:g}`.
    Number(f64),
    /// A text; for a product the GUID of the raw one, whose name the caller
    /// looks up.
    Text(String),
}

/// The text key of the hint at a corrected value and its argument: a
/// factor with its raw value (`s_cig_wert`), a product with the raw one
/// (`s_cig_produkt`), a reference without target (`s_p3_verweis_fehlt`),
/// any other value, a quality limit too, with its raw text
/// (`s_p3_wert_alt`).
pub fn hint(corrected: &Corrected) -> (&'static str, HintArg) {
    let raw = || HintArg::Text(corrected.raw.clone());
    match corrected.field.as_str() {
        "product" if corrected.value.is_empty() => ("s_p3_verweis_fehlt", HintArg::None),
        "product" => ("s_cig_produkt", raw()),
        "modifierAtStart" | "modifierAtEnd" => match parse_number(&corrected.raw) {
            Some(factor) => ("s_cig_wert", HintArg::Number(factor)),
            None => ("s_p3_wert_alt", raw()),
        },
        _ => ("s_p3_wert_alt", raw()),
    }
}

/// The text key naming where a correction comes from.
pub fn source_key(corrected: &Corrected) -> &'static str {
    if corrected.source == "versekit" { "s_p3_quelle_eigen" } else { "s_p3_quelle_scmdb" }
}

/// The text key of an outcome the user should hear about, `None` for the
/// others: a fixed error (`s_p3_behoben`) and a held-back correction
/// (`s_p3_ruht`).
pub const fn outcome_key(outcome: Outcome) -> Option<&'static str> {
    match outcome {
        Outcome::AlreadyRight => Some("s_p3_behoben"),
        Outcome::RawChanged => Some("s_p3_ruht"),
        _ => None,
    }
}

fn mark(field: &str, raw: impl Into<String>, value: impl Into<String>, correction: &Correction) -> Corrected {
    Corrected {
        field: field.to_string(),
        raw: raw.into(),
        value: value.into(),
        reason: correction.reason.clone(),
        source: correction.source.to_string(),
    }
}

fn parse_number(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Applies `corrections` to `values` and reports what became of each.
///
/// A correction made for another build than `values.build` is skipped as
/// [`Outcome::NotFound`]: after a patch the same GUIDs may hold other
/// values. Corrections without build apply to every build.
pub fn apply(values: &mut GameValues, corrections: &[Correction]) -> Report {
    let mut report = Report::default();
    for correction in corrections {
        let outcome = if correction.build.is_some() && correction.build != values.build {
            Outcome::NotFound
        } else {
            apply_one(values, correction)
        };
        report.outcomes.push((correction.clone(), outcome));
    }
    report
}

fn apply_one(values: &mut GameValues, correction: &Correction) -> Outcome {
    match (&correction.section, &correction.field) {
        (Section::Blueprint, Field::Product) => {
            // A reference without target that leads to a record now: fixed.
            let target_there = correction.missing_target
                && correction.raw.as_deref().is_some_and(|raw| values.items.contains_key(raw));
            let Some(recipe) = values.recipes.iter_mut().find(|recipe| recipe.guid == correction.record) else {
                return Outcome::NotFound;
            };
            if recipe.corrected.iter().any(|mark| mark.field == "product") {
                return Outcome::Repeated;
            }
            let current = recipe.product.clone().unwrap_or_default();
            if correction.expected.as_deref() == Some(current.as_str()) || target_there {
                return Outcome::AlreadyRight;
            }
            if correction.raw.as_deref().is_some_and(|raw| raw != current) {
                return Outcome::RawChanged;
            }
            match &correction.expected {
                Some(expected) => {
                    recipe.corrected.push(mark("product", current, expected.clone(), correction));
                    recipe.product = Some(expected.clone());
                    recipe.product_missing = false;
                    Outcome::Applied
                }
                None => {
                    recipe.corrected.push(mark("product", current, "", correction));
                    if correction.missing_target {
                        recipe.product_missing = true;
                    }
                    Outcome::Marked
                }
            }
        }
        (Section::Blueprint, Field::Modifier { slot, property_key, index, value }) => {
            let Some(recipe) = values.recipes.iter_mut().find(|recipe| recipe.guid == correction.record) else {
                return Outcome::NotFound;
            };
            let Some(expected) = correction.expected.as_deref().and_then(parse_number) else {
                return Outcome::NotFound;
            };
            let mut hits: Vec<&mut Modifier> = recipe
                .tiers
                .iter_mut()
                .flat_map(|tier| &mut tier.slots)
                .filter(|found| found.name == *slot)
                .flat_map(|found| found.modifiers.iter_mut())
                .filter(|range| range.property_key == *property_key)
                .collect();
            let range = match index {
                Some(index) if *index < hits.len() => hits.swap_remove(*index),
                Some(_) => return Outcome::NotFound,
                None if hits.len() == 1 => hits.swap_remove(0),
                None if hits.is_empty() => return Outcome::NotFound,
                None => return Outcome::Ambiguous,
            };
            if range.corrected.iter().any(|mark| mark.field == value.key()) {
                return Outcome::Repeated;
            }
            let place = value.of(range);
            let current = *place;
            if current == expected {
                return Outcome::AlreadyRight;
            }
            if correction.raw.as_deref().and_then(parse_number).is_some_and(|raw| raw != current) {
                return Outcome::RawChanged;
            }
            *place = expected;
            range.corrected.push(mark(value.key(), current.to_string(), expected.to_string(), correction));
            Outcome::Applied
        }
        (Section::Entity, Field::AttachType | Field::CgItemType | Field::Manufacturer) => {
            let Some(item) = values.items.get_mut(&correction.record) else { return Outcome::NotFound };
            let Some(expected) = correction.expected.clone() else { return Outcome::NotFound };
            let (name, place) = match correction.field {
                Field::AttachType => ("attachType", &mut item.attach_type),
                Field::CgItemType => ("cgItemType", &mut item.cg_item_type),
                _ => ("manufacturer", &mut item.manufacturer),
            };
            if item.corrected.iter().any(|mark| mark.field == name) {
                return Outcome::Repeated;
            }
            let current = place.clone().unwrap_or_default();
            if current == expected {
                return Outcome::AlreadyRight;
            }
            if correction.raw.as_deref().is_some_and(|raw| raw != current) {
                return Outcome::RawChanged;
            }
            *place = Some(expected.clone());
            item.corrected.push(mark(name, current, expected, correction));
            Outcome::Applied
        }
        (Section::Manufacturer, Field::Manufacturer) => {
            let Some(expected) = correction.expected.clone() else { return Outcome::NotFound };
            let mut seen = false;
            let mut changed = false;
            for item in values.items.values_mut() {
                if item.manufacturer_record.as_deref() != Some(correction.record.as_str()) {
                    continue;
                }
                let current = item.manufacturer.clone().unwrap_or_default();
                // The record is the cause: a raw name that is none of ours means the data changed.
                if correction.raw.as_deref().is_some_and(|raw| raw != current) && current != expected {
                    return Outcome::RawChanged;
                }
                seen = true;
                if item.corrected.iter().any(|mark| mark.field == "manufacturer") || current == expected {
                    continue;
                }
                item.manufacturer = Some(expected.clone());
                item.corrected.push(mark("manufacturer", current, expected.clone(), correction));
                changed = true;
            }
            match (seen, changed) {
                (false, _) => Outcome::NotFound,
                (true, true) => Outcome::Applied,
                (true, false) => Outcome::AlreadyRight,
            }
        }
        _ => Outcome::Elsewhere,
    }
}

/// The corrections the blueprint catalog applies while it reads the game
/// data: the names of blueprints, the entries of reward pools, the faction
/// of contracts, the kind of items and the maker of items and of a
/// manufacturer record. The others belong to the values ([`apply`]).
///
/// Built once from a list and handed to the reader; [`CatalogFixes::into_report`]
/// says what became of each correction.
#[derive(Debug, Default)]
pub struct CatalogFixes {
    names: HashMap<String, (usize, String)>,
    attach_types: HashMap<String, (usize, String)>,
    item_types: HashMap<String, (usize, String)>,
    pool_entries: HashMap<String, Vec<(usize, String, Option<String>)>>,
    factions: HashMap<String, (usize, String)>,
    item_makers: HashMap<String, (usize, String)>,
    /// Manufacturer record → (correction, meant name, the name CIG shipped).
    record_makers: HashMap<String, (usize, String, Option<String>)>,
    corrections: Vec<Correction>,
    used: RefCell<Vec<bool>>,
}

impl CatalogFixes {
    /// The fixes of `corrections` that fit `build`; a correction made for
    /// another build is left out (it counts as not found).
    pub fn new(corrections: &[Correction], build: Option<&str>) -> Self {
        let mut fixes = Self { corrections: corrections.to_vec(), ..Self::default() };
        fixes.used = RefCell::new(vec![false; corrections.len()]);
        for (i, correction) in corrections.iter().enumerate() {
            if correction.build.is_some() && correction.build.as_deref() != build {
                continue;
            }
            let record = correction.record.to_ascii_lowercase();
            let expected = correction.expected.clone();
            match (&correction.section, &correction.field, expected) {
                (Section::Blueprint, Field::Name, Some(name)) => {
                    fixes.names.insert(record, (i, name));
                }
                (Section::Entity, Field::AttachType, Some(kind)) => {
                    fixes.attach_types.insert(record, (i, kind));
                }
                (Section::Entity, Field::CgItemType, Some(kind)) => {
                    fixes.item_types.insert(record, (i, kind));
                }
                (Section::Entity, Field::Manufacturer, Some(maker)) => {
                    fixes.item_makers.insert(record, (i, maker));
                }
                (Section::Manufacturer, Field::Manufacturer, Some(maker)) => {
                    fixes.record_makers.insert(record, (i, maker, correction.raw.clone()));
                }
                (Section::Pool, Field::PoolEntry { blueprint, .. }, replacement) => {
                    fixes.pool_entries.entry(record).or_default().push((
                        i,
                        blueprint.to_ascii_lowercase(),
                        replacement,
                    ));
                }
                (Section::Contract, Field::Faction, Some(faction)) => {
                    fixes.factions.insert(record, (i, faction.to_ascii_lowercase()));
                }
                _ => {}
            }
        }
        fixes
    }

    fn hit(&self, index: usize) {
        if let Some(used) = self.used.borrow_mut().get_mut(index) {
            *used = true;
        }
    }

    fn find<'a>(&self, map: &'a HashMap<String, (usize, String)>, key: &str) -> Option<&'a str> {
        let (index, value) = map.get(&key.to_ascii_lowercase())?;
        self.hit(*index);
        Some(value)
    }

    /// The meant name of the blueprint record `guid`.
    pub fn name(&self, guid: &str) -> Option<&str> {
        self.find(&self.names, guid)
    }

    /// The meant kind of the entity `guid`: its attach type, else its crafting item type.
    pub fn item_kind(&self, guid: &str) -> Option<&str> {
        self.find(&self.attach_types, guid).or_else(|| self.find(&self.item_types, guid))
    }

    /// The meant maker of the items of the manufacturer record `maker_record` (the
    /// cause, named once), else of the entity `guid` alone. `current` is the
    /// name the data give now: when the correction knows the name CIG shipped
    /// and the data show another one (and not the meant one), the data changed
    /// and the correction is held back, as for the values.
    pub fn maker(&self, guid: &str, maker_record: Option<&str>, current: &str) -> Option<&str> {
        let by_record = maker_record.and_then(|record| {
            let (index, meant, raw) = self.record_makers.get(&record.to_ascii_lowercase())?;
            if raw.as_deref().is_some_and(|raw| raw != current) && current != meant {
                return None;
            }
            self.hit(*index);
            Some(meant.as_str())
        });
        by_record.or_else(|| self.find(&self.item_makers, guid))
    }

    /// The meant faction record of the contract named `debug_name`.
    pub fn faction(&self, debug_name: &str) -> Option<&str> {
        self.find(&self.factions, debug_name)
    }

    /// The entries of the pool `guid` to fix, each a blueprint listed twice
    /// and the blueprint that replaces its second place (none: drop it).
    pub fn pool_entries(&self, guid: &str) -> Vec<(usize, &str, Option<&str>)> {
        self.pool_entries
            .get(&guid.to_ascii_lowercase())
            .into_iter()
            .flatten()
            .map(|(index, blueprint, replacement)| (*index, blueprint.as_str(), replacement.as_deref()))
            .collect()
    }

    /// Notes that the pool correction `index` was applied.
    pub fn pool_entry_applied(&self, index: usize) {
        self.hit(index);
    }

    /// What became of each correction: applied (it found its record), held
    /// back for another build or not found, or left to the values.
    pub fn into_report(self) -> Report {
        let used = self.used.into_inner();
        let outcomes = self
            .corrections
            .into_iter()
            .zip(used)
            .map(|(correction, used)| {
                let for_catalog = matches!(
                    (&correction.section, &correction.field),
                    (Section::Blueprint, Field::Name)
                        | (Section::Entity, Field::AttachType | Field::CgItemType | Field::Manufacturer)
                        | (Section::Manufacturer, _)
                        | (Section::Pool, _)
                        | (Section::Contract, _)
                );
                let outcome = match (for_catalog, used) {
                    (false, _) => Outcome::Elsewhere,
                    (true, true) => Outcome::Applied,
                    (true, false) => Outcome::NotFound,
                };
                (correction, outcome)
            })
            .collect();
        Report { outcomes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{ItemValues, Recipe, RecipeTier, Slot};

    fn aztalan() -> GameValues {
        let range = Modifier {
            property_key: "armor_damagemitigation".into(),
            end_quality: 1000.0,
            at_start: 9.0,
            at_end: 1.1,
            ..Modifier::default()
        };
        GameValues {
            build: Some("4.10.2-live.12913470".into()),
            recipes: vec![Recipe {
                guid: "2663f762-5648-41d4-8746-c8dcc07e71b1".into(),
                tiers: vec![RecipeTier {
                    slots: vec![Slot { name: "Segment Paneling".into(), modifiers: vec![range], ..Slot::default() }],
                    ..RecipeTier::default()
                }],
                ..Recipe::default()
            }],
            ..GameValues::default()
        }
    }

    fn list() -> Value {
        serde_json::json!({
            "_comment": "…",
            "blueprintRecords": {
                "_comment": "…",
                "2663F762-5648-41d4-8746-c8dcc07e71b1": {"slot": "Segment Paneling",
                    "propertyKey": "armor_damagemitigation", "expected": {"modifierAtStart": 0.9},
                    "note": "9.0 where 0.9 fits"},
                "cdd236fa-56b9-4913-90cf-be7f4e6ca5a7": {"kind": "label", "expected": {"name": "Antium Arms Maroon"}}
            },
            "blueprintPools": {"ce014e33-cfcb-45e9-a12f-06f9baff9db2": {"duplicateReferences": [
                {"blueprintRecord": "9f11679b-027f-4a0b-903d-6655f2978e76",
                 "expected": {"blueprintRecord": "a193c4c6-87ed-4623-87db-588e59d47c04"}}]}},
            "entities": {"d5456c35-a30b-4e13-bae2-b5609a43909a": {
                "expected": {"attachType": "FuelNozzle", "cgItemType": "FuelNozzle"}}},
            "contracts": {"RedWind_Nyx_Medium_RecoverCargo": {"field": "factionGuid",
                "expected": {"factionGuid": "AA5F0BF8-f36f-423a-b9dd-73084139db23"}}}
        })
    }

    #[test]
    fn the_three_foxwell_hauling_contracts_are_given_for_foxwell_enforcement() {
        let fixes = CatalogFixes::new(&built_in(), None);
        for contract in [
            "Foxwell_OLP_HaulCargo_AtoB_Small_Carbon",
            "Foxwell_PAF_HaulCargo_AtoB_Small_AgriculturalSupplies",
            "Foxwell_PAF_HaulCargo_Multi2ToSingle_Small_Aluminum",
        ] {
            assert_eq!(fixes.faction(contract), Some("016c2950-0292-468c-a408-a5283b52f158"), "{contract}");
        }
        assert_eq!(fixes.faction("Foxwell_OLP_HaulCargo_AtoB_Small_Gold"), None, "no rule for other contracts");
        let factions = built_in().iter().filter(|c| c.field == Field::Faction).count();
        assert_eq!(factions, 3, "only these three");
    }

    #[test]
    fn the_scmdb_list_gives_one_correction_per_field() {
        let corrections = from_scmdb(&list(), "4.10.2-live.12913470");
        assert_eq!(corrections.len(), 6);
        assert_eq!(corrections[0].record, "2663f762-5648-41d4-8746-c8dcc07e71b1");
        assert_eq!(corrections[0].expected.as_deref(), Some("0.9"));
        assert!(corrections.iter().any(|c| c.field == Field::Faction && c.record == "RedWind_Nyx_Medium_RecoverCargo"));
    }

    #[test]
    fn a_modifier_is_corrected_and_marked() {
        let mut values = aztalan();
        let report = apply(&mut values, &from_scmdb(&list(), "4.10.2-live.12913470"));
        assert_eq!(report.count(Outcome::Applied), 1);
        let range = &values.recipes[0].tiers[0].slots[0].modifiers[0];
        assert_eq!(range.at_start, 0.9);
        assert_eq!(range.corrected[0].raw, "9");
        assert_eq!(range.corrected[0].field, "modifierAtStart");
        // Names, pools and contracts are for the catalog.
        assert_eq!(report.count(Outcome::Elsewhere), 3);
        // The item of the entity correction is not part of these values.
        assert_eq!(report.count(Outcome::NotFound), 2);
        // Applied twice: the second time nothing changes and no second mark.
        let again = apply(&mut values, &from_scmdb(&list(), "4.10.2-live.12913470"));
        assert_eq!(again.count(Outcome::Repeated), 1);
        assert_eq!(values.recipes[0].tiers[0].slots[0].modifiers[0].corrected.len(), 1);
    }

    #[test]
    fn a_list_of_another_build_does_not_apply() {
        let mut values = aztalan();
        let report = apply(&mut values, &from_scmdb(&list(), "4.10.1-live.1"));
        assert_eq!(report.count(Outcome::Applied), 0);
        assert_eq!(values.recipes[0].tiers[0].slots[0].modifiers[0].at_start, 9.0);
    }

    #[test]
    fn a_changed_raw_value_holds_the_correction_back() {
        let mut values = GameValues::default();
        values.recipes.push(Recipe {
            guid: "bb511044-6590-4e19-b077-a347cad104fa".into(),
            product: Some("something-else".into()),
            ..Recipe::default()
        });
        values.recipes.push(Recipe {
            guid: "d7f3ad24-fca7-4393-b026-f55675cfe820".into(),
            product: Some("3cf6753c-d69c-4980-a45e-724151407096".into()),
            ..Recipe::default()
        });
        let report = apply(&mut values, &built_in());
        assert_eq!(report.outcomes[0].1, Outcome::RawChanged);
        assert_eq!(values.recipes[0].product.as_deref(), Some("something-else"));
        assert_eq!(report.outcomes[1].1, Outcome::Marked);
        assert!(values.recipes[1].product_missing);
        assert!(values.recipes[1].corrected[0].value.is_empty());
        // Applied again: no second mark.
        assert_eq!(apply(&mut values, &built_in()).outcomes[1].1, Outcome::Repeated);
        assert_eq!(values.recipes[1].corrected.len(), 1);
    }

    #[test]
    fn a_missing_target_that_exists_now_is_fixed() {
        let mut values = GameValues::default();
        values.recipes.push(Recipe {
            guid: "d7f3ad24-fca7-4393-b026-f55675cfe820".into(),
            product: Some("3cf6753c-d69c-4980-a45e-724151407096".into()),
            ..Recipe::default()
        });
        values.items.insert("3cf6753c-d69c-4980-a45e-724151407096".into(), ItemValues::default());
        assert_eq!(apply(&mut values, &built_in()).outcomes[1].1, Outcome::AlreadyRight);
        assert!(!values.recipes[0].product_missing);
        assert!(values.recipes[0].corrected.is_empty());
    }

    #[test]
    fn a_null_meant_product_gives_no_correction() {
        let file =
            serde_json::json!({"blueprintRecords": {"b1": {"kind": "entity", "expected": {"entityClass": null}}}});
        assert!(from_scmdb(&file, "x").is_empty());
    }

    #[test]
    fn the_polaris_blueprint_makes_the_polaris_shield() {
        let mut values = GameValues::default();
        values.recipes.push(Recipe {
            guid: "bb511044-6590-4e19-b077-a347cad104fa".into(),
            product: Some("6fc982c0-2747-42b7-a628-7c76ecedf732".into()),
            ..Recipe::default()
        });
        let report = apply(&mut values, &built_in());
        assert_eq!(report.outcomes[0].1, Outcome::Applied);
        assert_eq!(values.recipes[0].product.as_deref(), Some("d9870a05-1a66-4a9a-bec6-30eb67e3a3ce"));
        assert_eq!(values.recipes[0].corrected[0].source, "versekit");
    }

    #[test]
    fn every_hint_has_a_text_in_both_languages() {
        use versekit_core::language::{Lang, text_in};
        let marks = [
            mark("modifierAtStart", "9", "0.9", &built_in()[0]),
            mark("product", "a", "b", &built_in()[0]),
            mark("product", "a", "", &built_in()[1]),
            mark("attachType", "DockingCollar", "FuelNozzle", &built_in()[0]),
        ];
        let mut keys: Vec<&str> = marks.iter().map(|m| hint(m).0).collect();
        keys.extend(marks.iter().map(source_key));
        keys.push("s_p3_berichtigt");
        keys.extend([Outcome::AlreadyRight, Outcome::RawChanged].into_iter().filter_map(outcome_key));
        assert_eq!(hint(&marks[2]), ("s_p3_verweis_fehlt", HintArg::None));
        assert_eq!(hint(&marks[0]), ("s_cig_wert", HintArg::Number(9.0)));
        assert_eq!(hint(&marks[3]).0, "s_p3_wert_alt");
        for key in keys {
            for lang in [Lang::De, Lang::En] {
                assert!(text_in(lang, key).is_some_and(|text| !text.is_empty() && !text.contains('—')), "{key}");
            }
        }
    }

    #[test]
    fn the_catalog_applies_names_pools_factions_kinds_and_makers() {
        let mut all = built_in();
        all.extend(from_scmdb(&list(), "4.10.2-live.12913470"));
        all.push(Correction {
            section: Section::Entity,
            record: "aaaa0000-0000-0000-0000-000000000001".into(),
            field: Field::Manufacturer,
            raw: None,
            expected: Some("maxOx".into()),
            missing_target: false,
            reason: String::new(),
            build: Some("4.10.2-live.12913470".into()),
            source: "scmdb",
        });
        let fixes = CatalogFixes::new(&all, Some("4.10.2-live.12913470"));
        assert_eq!(fixes.name("CDD236FA-56b9-4913-90cf-be7f4e6ca5a7"), Some("Antium Arms Maroon"));
        assert_eq!(fixes.name("00000000-0000-0000-0000-000000000000"), None);
        assert_eq!(fixes.item_kind("d5456c35-a30b-4e13-bae2-b5609a43909a"), Some("FuelNozzle"));
        assert_eq!(fixes.faction("redwind_nyx_medium_recoverCARGO"), Some("aa5f0bf8-f36f-423a-b9dd-73084139db23"));
        let pool = fixes.pool_entries("ce014e33-cfcb-45e9-a12f-06f9baff9db2");
        assert_eq!(pool.len(), 1);
        assert_eq!(
            (pool[0].1, pool[0].2),
            ("9f11679b-027f-4a0b-903d-6655f2978e76", Some("a193c4c6-87ed-4623-87db-588e59d47c04"))
        );
        fixes.pool_entry_applied(pool[0].0);
        // The cause is named once: the manufacturer record wins over the single item.
        let single = "aaaa0000-0000-0000-0000-000000000001";
        assert_eq!(fixes.maker(single, Some(MXOX_RECORD), "Aegis Dynamics"), Some("MaxOx"));
        assert_eq!(fixes.maker(single, None, "Aegis Dynamics"), Some("maxOx"));
        // The data changed (the record's name is neither the shipped nor the meant one): the record's
        // correction is held back, the single item's stays.
        assert_eq!(fixes.maker(single, Some(MXOX_RECORD), "Somebody Else"), Some("maxOx"));
        assert_eq!(fixes.maker("other", Some(MXOX_RECORD), "Somebody Else"), None);
        let report = fixes.into_report();
        // The pool, the name, the kind, the faction and both makers were asked for or applied.
        assert!(report.count(Outcome::Applied) >= 6, "{:?}", report.outcomes.iter().map(|o| o.1).collect::<Vec<_>>());
        // What belongs to the values stays with them.
        assert!(
            report.outcomes.iter().any(|(c, o)| matches!(c.field, Field::Modifier { .. }) && *o == Outcome::Elsewhere)
        );
    }

    #[test]
    fn a_correction_of_another_build_is_held_back_in_the_catalog() {
        let fixes = CatalogFixes::new(&from_scmdb(&list(), "4.10.1-live.1"), Some("4.10.2-live.12913470"));
        assert_eq!(fixes.name("cdd236fa-56b9-4913-90cf-be7f4e6ca5a7"), None);
    }

    #[test]
    fn a_manufacturer_record_corrects_its_items_once() {
        let item = |maker: &str| ItemValues {
            manufacturer: Some(maker.into()),
            manufacturer_record: Some(MXOX_RECORD.into()),
            ..ItemValues::default()
        };
        let mut values = GameValues::default();
        values.items.insert("a".into(), item("Aegis Dynamics"));
        values.items.insert("b".into(), item("Aegis Dynamics"));
        values
            .items
            .insert("c".into(), ItemValues { manufacturer: Some("Aegis Dynamics".into()), ..ItemValues::default() });
        let report = apply(&mut values, &built_in());
        assert_eq!(report.outcomes[2].1, Outcome::Applied);
        for key in ["a", "b"] {
            assert_eq!(values.items[key].manufacturer.as_deref(), Some("MaxOx"));
            assert_eq!(values.items[key].corrected.len(), 1);
        }
        assert_eq!(values.items["c"].manufacturer.as_deref(), Some("Aegis Dynamics"), "another record is left alone");
        // The single items of scmdb's list add no second mark.
        let mut single = from_scmdb(&list(), "x");
        single.push(Correction {
            section: Section::Entity,
            record: "a".into(),
            field: Field::Manufacturer,
            raw: None,
            expected: Some("maxOx".into()),
            missing_target: false,
            reason: String::new(),
            build: None,
            source: "scmdb",
        });
        let again = apply(&mut values, &single);
        assert_eq!(again.outcomes.last().map(|o| o.1), Some(Outcome::Repeated));
        assert_eq!(values.items["a"].corrected.len(), 1);
        // The data changed: the raw name is no longer the known one.
        let mut changed = GameValues::default();
        changed.items.insert("a".into(), item("Somebody Else"));
        assert_eq!(apply(&mut changed, &built_in()).outcomes[2].1, Outcome::RawChanged);
    }

    #[test]
    fn an_entity_type_is_corrected() {
        let mut values = GameValues::default();
        values.items.insert(
            "d5456c35-a30b-4e13-bae2-b5609a43909a".into(),
            ItemValues { attach_type: Some("DockingCollar".into()), ..ItemValues::default() },
        );
        values.build = Some("4.10.2-live.12913470".into());
        let report = apply(&mut values, &from_scmdb(&list(), "4.10.2-live.12913470"));
        assert_eq!(report.count(Outcome::Applied), 2);
        let item = &values.items["d5456c35-a30b-4e13-bae2-b5609a43909a"];
        assert_eq!(item.attach_type.as_deref(), Some("FuelNozzle"));
        assert_eq!(item.corrected.len(), 2);
    }
}
