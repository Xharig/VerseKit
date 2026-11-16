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

//! Where the raw materials lie, read from the game data: which place has which deposit, and which raw material a
//! deposit holds.
//!
//! The chain, all by GUID: a raw material (`ResourceType`) is held by a mineable element, the elements make up the
//! compositions of the deposits, a provider preset lists its deposits in groups (a way of mining each), and a
//! preset is tied to a place either by its name (`HPP_<class of the starmap object>`: moons and planets) or by
//! the containers that name it (clusters, with the starmap object each hangs below). A raw material reaches the
//! refined good a blueprint asks for by `ResourceType.refinedVersion`, so no list of names is needed.
//!
//! The records come from the whole Basetool catalog of p4kx (the places of the containers need the starmap and the
//! presets together), the link to the refined good and the English names from the game database. The result is kept
//! as `fundorte-spieldaten.json` in the data folder, once per state of the `Data.p4k`, where the mining data read it
//! (`versekit_core::mining_data`). Everything of the reading is in this module, so a change of the catalog (p4kx
//! 0.5, schema 3) touches only it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use p4kx_dataforge::{DataForge, Value};
use serde::Deserialize;
use versekit_core::jsonio::{self, Newline, Value as Json};

use crate::Error;
use crate::quality_goods::{Keep, archive_stamp, parse_ini};

/// The file in the data folder.
pub use versekit_core::mining_data::PLACES_FILE as FILE;
/// The layout of [`FILE`].
pub use versekit_core::mining_data::PLACES_FORMAT as FORMAT;
/// The type of the records that are raw materials and goods.
const RESOURCE_TYPE: &str = "ResourceType";
/// Fewer places mean the catalog was not understood: then nothing is kept.
pub const MINIMUM: usize = 20;
/// The groups of a provider preset that are a way of mining raw materials: by ship, by ground vehicle, by hand.
/// Salvage and the groups of loose goods are none.
const MINING_GROUPS: [&str; 3] = ["SpaceShip_Mineables", "GroundVehicle_Mineables", "FPS_Mineables"];
/// How many points of a cluster the name of its place writes out; the rest is counted.
const MOST_NAMES: usize = 6;
/// The first part of the name of the preset of a moon or a planet.
const BODY_PREFIX: &str = "hpp_";
/// Between the points of a cluster.
const JOIN: &str = " · ";
/// How far up the parents of a starmap object are followed to find its star.
const MOST_PARENTS: usize = 16;
/// The type of the starmap object of a star.
const STAR: &str = "Star";

/// A `ResourceType` of the game database: a raw material or another good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Good {
    /// The GUID in lower case.
    pub guid: String,
    /// The English name.
    pub name: String,
    /// The GUID of the refined good (`refinedVersion`), where there is one.
    pub refined: Option<String>,
}

/// The goods of the DataForge `forge` with an English name from the `global.ini` `raw_ini`, in the order of the
/// records; placeholders and goods without a text stay out.
pub fn goods(forge: &DataForge<'_>, raw_ini: &[u8]) -> Vec<Good> {
    let texts = parse_ini(raw_ini);
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut found: Vec<Good> = Vec::new();
    for record in forge.records() {
        if record.type_name().ok() != Some(RESOURCE_TYPE) {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let key = match tree.field("displayName") {
            Some(Value::Locale(text) | Value::String(text)) => text.trim_start_matches('@').to_lowercase(),
            _ => String::new(),
        };
        let Some(name) = texts.get(&key).filter(|value| !value.is_empty() && !value.starts_with('@')) else {
            continue;
        };
        let refined = match tree.field("refinedVersion") {
            Some(Value::Reference(reference)) => {
                let guid = reference.guid().to_string().to_lowercase();
                (!guid.chars().all(|c| c == '0' || c == '-')).then_some(guid)
            }
            _ => None,
        };
        let guid = record.guid().to_string().to_lowercase();
        if !found.iter().any(|good| good.guid == guid) {
            found.push(Good { guid, name: name.clone(), refined });
        }
    }
    found
}

/// The goods of the DataForge `bytes`, named from the English `global.ini` `ini`.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn goods_from_dataforge(bytes: &[u8], ini: &[u8]) -> Result<Vec<Good>, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: "Data\\Game2.dcb".to_string(), source: Box::new(source) })?;
    Ok(goods(&forge, ini))
}

// ------------------------------------------------------------ the catalog

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    mineable_elements: Vec<RawElement>,
    #[serde(default)]
    mineable_compositions: Vec<RawComposition>,
    #[serde(default)]
    harvestable_provider_presets: Vec<RawPreset>,
    #[serde(default)]
    harvestable_locations: Vec<RawLocation>,
    #[serde(default)]
    starmap_objects: Vec<RawObject>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawElement {
    guid: String,
    #[serde(default)]
    class_name: String,
    resource_type_guid: Option<String>,
}

#[derive(Deserialize)]
struct RawText {
    en: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComposition {
    guid: String,
    #[serde(default)]
    class_name: String,
    deposit_name: Option<RawText>,
    #[serde(default)]
    parts: Vec<RawPart>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPart {
    mineable_element_guid: Option<String>,
    min_percentage: Option<f64>,
    max_percentage: Option<f64>,
    probability: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPreset {
    guid: String,
    class_name: String,
    #[serde(default)]
    groups: Vec<RawGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGroup {
    group_name: Option<String>,
    group_probability: Option<f64>,
    #[serde(default)]
    harvestables: Vec<RawHarvestable>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHarvestable {
    harvestable: Option<RawSpawn>,
    relative_probability: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSpawn {
    mineable_composition_guid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLocation {
    preset_guid: String,
    #[serde(default)]
    system: String,
    starmap_object_guid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawObject {
    guid: String,
    #[serde(default)]
    class_name: String,
    name: Option<RawText>,
    #[serde(rename = "type")]
    kind: Option<String>,
    parent_guid: Option<String>,
    system: Option<String>,
}

impl RawObject {
    /// The English name the player finds; `None` for a placeholder.
    fn english(&self) -> Option<&str> {
        self.name.as_ref().and_then(|name| name.en.as_deref()).map(str::trim).filter(|name| !name.is_empty())
    }
}

// ------------------------------------------------------------ the result

/// One deposit of a group: the composition and its share of the group, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Deposit {
    pub composition: String,
    pub share: f64,
}

/// A way of mining at a place.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    pub probability: f64,
    pub deposits: Vec<Deposit>,
}

/// A place with its ways of mining.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// The name the player finds: of a moon or a planet, or the points of a cluster.
    pub name: String,
    pub system: String,
    /// The class name of the provider preset.
    pub template: String,
    /// `Moon`, `Planet` … for a body, `Cluster` for the points of the containers.
    pub kind: String,
    /// How many points of a cluster the name stands for (one for a body).
    pub points: usize,
    pub groups: Vec<Group>,
}

/// One part of a composition: the raw material with its share and its chance.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub element: String,
    /// The English name of the raw material.
    pub ore: String,
    pub min: f64,
    pub max: f64,
    pub probability: f64,
}

/// A deposit: what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    pub name: String,
    pub parts: Vec<Part>,
}

/// Where the raw materials lie.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Places {
    pub places: Vec<Place>,
    /// By GUID: only the compositions a place uses.
    pub compositions: BTreeMap<String, Composition>,
    /// The raw materials with the English name of the refined good they become.
    pub ores: Vec<(String, Option<String>)>,
}

fn lower(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// The name of a point of a cluster as the player reads it: a side cluster (`MIC L4-A`) is the point it hangs below
/// (`MIC L4`).
fn folded(object: &RawObject, objects: &HashMap<String, &RawObject>) -> Option<String> {
    let name = object.english()?;
    let parent = object.parent_guid.as_deref().and_then(|guid| objects.get(&lower(guid))).and_then(|parent| parent.english());
    if let Some(parent) = parent
        && let Some(rest) = name.strip_prefix(parent)
        && let Some(suffix) = rest.strip_prefix('-')
        && !suffix.is_empty()
        && suffix.len() <= 2
        && suffix.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Some(parent.to_string());
    }
    Some(name.to_string())
}

/// The name of a star system: its folder in capitals at the start where no star states it.
fn title(folder: &str) -> String {
    let mut chars = folder.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The groups of a preset that are a way of mining, each name once, each with a deposit at least.
fn mining_groups(preset: &RawPreset) -> Vec<Group> {
    let mut seen = BTreeSet::new();
    let mut groups = Vec::new();
    for group in &preset.groups {
        let Some(name) = group.group_name.as_deref().filter(|name| MINING_GROUPS.contains(name)) else { continue };
        if !seen.insert(name) {
            continue;
        }
        let deposits: Vec<Deposit> = group
            .harvestables
            .iter()
            .filter_map(|entry| {
                let composition = entry.harvestable.as_ref()?.mineable_composition_guid.as_deref()?;
                Some(Deposit { composition: lower(composition), share: entry.relative_probability.unwrap_or(0.0) })
            })
            .collect();
        if !deposits.is_empty() {
            groups.push(Group { name: name.to_string(), probability: group.group_probability.unwrap_or(1.0), deposits });
        }
    }
    groups
}

/// Where the raw materials lie, from the Basetool catalog `catalog` (JSON) and the `goods` of the game database.
///
/// # Errors
///
/// When the bytes are no catalog JSON.
pub fn build(catalog: &[u8], goods: &[Good]) -> Result<Places, serde_json::Error> {
    let raw: RawCatalog = serde_json::from_slice(catalog)?;
    Ok(places_of(&raw, goods))
}

fn places_of(raw: &RawCatalog, goods: &[Good]) -> Places {
    let objects: HashMap<String, &RawObject> = raw.starmap_objects.iter().map(|o| (lower(&o.guid), o)).collect();
    let mut by_class: HashMap<String, &RawObject> = HashMap::new();
    for object in &raw.starmap_objects {
        by_class.entry(lower(&object.class_name)).or_insert(object);
    }
    // The star systems: the folder of a system (`stanton`) and the name of its star.
    let stars: HashMap<String, String> = raw
        .starmap_objects
        .iter()
        .filter(|o| o.kind.as_deref() == Some(STAR))
        .filter_map(|o| Some((lower(o.system.as_deref()?), o.english()?.to_string())))
        .collect();
    let system_of_folder = |folder: &str| stars.get(&lower(folder)).cloned().unwrap_or_else(|| title(folder));
    let system_of_object = |object: &RawObject| {
        let mut at = object;
        for _ in 0..MOST_PARENTS {
            if at.kind.as_deref() == Some(STAR) {
                return at.english().map(str::to_string).unwrap_or_default();
            }
            match at.parent_guid.as_deref().and_then(|guid| objects.get(&lower(guid))) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        object.system.as_deref().map(system_of_folder).unwrap_or_default()
    };
    let mut rows: HashMap<String, Vec<&RawLocation>> = HashMap::new();
    for row in &raw.harvestable_locations {
        rows.entry(lower(&row.preset_guid)).or_default().push(row);
    }

    let mut places: Vec<Place> = Vec::new();
    for preset in &raw.harvestable_provider_presets {
        let groups = mining_groups(preset);
        if groups.is_empty() {
            continue;
        }
        let class = preset.class_name.as_str();
        // A moon or a planet: the preset is named after its starmap object.
        if class.get(..BODY_PREFIX.len()).is_some_and(|start| start.eq_ignore_ascii_case(BODY_PREFIX))
            && let Some(body) = by_class.get(&lower(&class[BODY_PREFIX.len()..]))
            && let Some(name) = body.english()
        {
            places.push(Place {
                name: name.to_string(),
                system: system_of_object(body),
                template: class.to_string(),
                kind: body.kind.clone().unwrap_or_else(|| "Body".to_string()),
                points: 1,
                groups: groups.clone(),
            });
        }
        // The points of a cluster: the containers that name the preset, per system, the points by name.
        let mut points: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for row in rows.get(&lower(&preset.guid)).into_iter().flatten() {
            let object = row.starmap_object_guid.as_deref().and_then(|guid| objects.get(&lower(guid)));
            if let Some(name) = object.and_then(|object| folded(object, &objects)) {
                points.entry(row.system.clone()).or_default().insert(name);
            }
        }
        for (folder, names) in points {
            let count = names.len();
            let mut written: Vec<String> = names.into_iter().take(MOST_NAMES).collect();
            if count > MOST_NAMES {
                written.push(format!("… +{}", count - MOST_NAMES));
            }
            places.push(Place {
                name: written.join(JOIN),
                system: system_of_folder(&folder),
                template: class.to_string(),
                kind: "Cluster".to_string(),
                points: count,
                groups: groups.clone(),
            });
        }
    }

    // The compositions the places use, and the raw materials in them.
    let elements: HashMap<String, &RawElement> = raw.mineable_elements.iter().map(|e| (lower(&e.guid), e)).collect();
    let by_guid: HashMap<&str, &Good> = goods.iter().map(|good| (good.guid.as_str(), good)).collect();
    let used: BTreeSet<String> = places
        .iter()
        .flat_map(|place| place.groups.iter())
        .flat_map(|group| group.deposits.iter())
        .map(|deposit| deposit.composition.clone())
        .collect();
    let mut ores: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut compositions = BTreeMap::new();
    for composition in &raw.mineable_compositions {
        let guid = lower(&composition.guid);
        if !used.contains(&guid) {
            continue;
        }
        let mut parts = Vec::new();
        for part in &composition.parts {
            let Some(element) = part.mineable_element_guid.as_deref().and_then(|guid| elements.get(&lower(guid))) else {
                continue;
            };
            let good = element.resource_type_guid.as_deref().and_then(|guid| by_guid.get(lower(guid).as_str()));
            let ore = good.map_or_else(|| element.class_name.clone(), |good| good.name.clone());
            let refined = good.and_then(|good| good.refined.as_deref()).and_then(|guid| by_guid.get(guid)).map(|g| g.name.clone());
            ores.entry(ore.clone()).or_insert(refined);
            parts.push(Part {
                element: lower(&element.guid),
                ore,
                min: part.min_percentage.unwrap_or(0.0),
                max: part.max_percentage.unwrap_or(0.0),
                probability: part.probability.unwrap_or(1.0),
            });
        }
        let name = composition
            .deposit_name
            .as_ref()
            .and_then(|name| name.en.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| composition.class_name.clone());
        compositions.insert(guid, Composition { name, parts });
    }
    Places { places, compositions, ores: ores.into_iter().collect() }
}

// ------------------------------------------------------------ the file

fn number(value: f64) -> Json {
    Json::Float(value)
}

/// The text of [`FILE`] for the places read from an archive with `stamp`.
pub fn file_text(stamp: &str, places: &Places) -> String {
    let place_rows = places
        .places
        .iter()
        .map(|place| {
            let groups = place
                .groups
                .iter()
                .map(|group| {
                    let deposits = group
                        .deposits
                        .iter()
                        .map(|deposit| {
                            Json::Object(vec![
                                ("mischung".into(), Json::str(&deposit.composition)),
                                ("anteil".into(), number(deposit.share)),
                            ])
                        })
                        .collect();
                    Json::Object(vec![
                        ("name".into(), Json::str(&group.name)),
                        ("wahrscheinlichkeit".into(), number(group.probability)),
                        ("lagerstaetten".into(), Json::Array(deposits)),
                    ])
                })
                .collect();
            Json::Object(vec![
                ("name".into(), Json::str(&place.name)),
                ("system".into(), Json::str(&place.system)),
                ("vorlage".into(), Json::str(&place.template)),
                ("art".into(), Json::str(&place.kind)),
                ("punkte".into(), Json::int(i64::try_from(place.points).unwrap_or(i64::MAX))),
                ("gruppen".into(), Json::Array(groups)),
            ])
        })
        .collect();
    let compositions = places
        .compositions
        .iter()
        .map(|(guid, composition)| {
            let parts = composition
                .parts
                .iter()
                .map(|part| {
                    Json::Object(vec![
                        ("element".into(), Json::str(&part.element)),
                        ("erz".into(), Json::str(&part.ore)),
                        ("min".into(), number(part.min)),
                        ("max".into(), number(part.max)),
                        ("wahrscheinlichkeit".into(), number(part.probability)),
                    ])
                })
                .collect();
            (
                guid.clone(),
                Json::Object(vec![("name".into(), Json::str(&composition.name)), ("teile".into(), Json::Array(parts))]),
            )
        })
        .collect();
    let ores = places
        .ores
        .iter()
        .map(|(name, refined)| {
            Json::Object(vec![
                ("name".into(), Json::str(name)),
                ("veredelt".into(), refined.as_ref().map_or(Json::Null, Json::str)),
            ])
        })
        .collect();
    let data = Json::Object(vec![
        ("format".into(), Json::int(FORMAT)),
        ("stand".into(), Json::str(stamp)),
        ("orte".into(), Json::Array(place_rows)),
        ("mischungen".into(), Json::Object(compositions)),
        ("erze".into(), Json::Array(ores)),
    ]);
    jsonio::dumps(&data, None, false)
}

/// Whether `file` was read from an archive with `stamp`, in the current layout.
pub fn is_current(file: &str, stamp: &str) -> bool {
    let Ok(read) = jsonio::read_json(file) else { return false };
    read.get("stand").and_then(Json::as_str) == Some(stamp) && read.get("format") == Some(&Json::int(FORMAT))
}

/// Reads the places with `read` and writes `file` when it does not belong to the archive at `archive` as it is now.
pub fn keep_current_with(archive: &Path, file: &str, read: impl FnOnce() -> Result<Places, Error>) -> Keep {
    let Some(stamp) = archive_stamp(archive) else { return Keep::NoArchive };
    if is_current(file, &stamp) {
        return Keep::Current;
    }
    let places = match read() {
        Ok(places) => places,
        Err(error) => return Keep::Failed(error.to_string()),
    };
    if places.places.len() < MINIMUM {
        return Keep::TooFew(places.places.len());
    }
    match jsonio::write_text_atomic(file, &file_text(&stamp, &places), Newline::Keep) {
        Ok(()) => Keep::Written,
        Err(error) if versekit_core::writeguard::refused(&error) => Keep::Refused,
        Err(error) => Keep::Failed(error.to_string()),
    }
}

/// Keeps `file` current for the archive at `archive`: reads the whole catalog (half a minute and about 210 MB) only
/// after a patch.
pub fn keep_current(archive: &Path, file: &str) -> Keep {
    keep_current_with(archive, file, || {
        let game = crate::GameData::open_file(archive)?;
        let catalog = game.catalog_all()?;
        let goods = goods_from_dataforge(&game.dataforge_bytes()?, &game.english_ini_bytes()?)?;
        build(&catalog, &goods).map_err(|source| Error::Places { source: Box::new(source) })
    })
}

/// Keeps the places of the data folder of `paths` current for its archive and notes what went wrong: a failure in
/// the error log, too few places in the start trail. Without a game nothing happens.
pub fn keep(paths: &versekit_core::paths::Paths) -> Option<Keep> {
    let archive = paths.archive_file()?;
    let _topic = versekit_core::archive_lock::reading_for("fundorte");
    let outcome = keep_current(Path::new(&archive), &paths.app_file(FILE));
    match &outcome {
        Keep::Failed(reason) => versekit_core::errors::record("places.refresh", reason),
        Keep::TooFew(count) => {
            versekit_core::trail::step(&format!("Fundorte: nur {count} erkannt, nicht abgelegt"));
        }
        _ => {}
    }
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use p4kx_testkit::{DataForgeBuilder, TestRecord, TestStruct, TestType, TestValue};

    use super::*;

    // ---- the goods and the link to the refined good

    fn good(guid: u128, name: &str, key: &str, refined: Option<u128>) -> TestRecord {
        let record = TestRecord::new(
            guid,
            RESOURCE_TYPE,
            format!("{RESOURCE_TYPE}.{name}"),
            "libs/foundry/records/resourcetypedatabase/db.xml",
        )
        .value("displayName", TestValue::text(key));
        match refined {
            Some(target) => record.value("refinedVersion", TestValue::Reference(target)),
            None => record,
        }
    }

    fn forge() -> Vec<u8> {
        DataForgeBuilder::new()
            .version(8)
            .structure(
                TestStruct::new(RESOURCE_TYPE)
                    .field("displayName", TestType::Locale)
                    .field("refinedVersion", TestType::Reference),
            )
            .record(good(0x10, "Ore_Iron", "@items_commodities_iron_ore", Some(0x11)))
            .record(good(0x11, "Iron", "@items_commodities_iron", None))
            .record(good(0x12, "Ore_Ice", "@items_commodities_ice_raw", Some(0x13)))
            .record(good(0x13, "PressurizedIce", "@items_commodities_ice", None))
            .record(good(0x14, "Hadanite", "@items_commodities_hadanite", None))
            .record(good(0x15, "NoText", "@no_such_key", None))
            .build()
    }

    fn ini() -> Vec<u8> {
        "\u{feff}items_commodities_iron_ore=Iron (Ore)\r\nitems_commodities_iron=Iron\r\n\
         items_commodities_ice_raw=Raw Ice\r\nitems_commodities_ice=Pressurized Ice\r\n\
         items_commodities_hadanite=Hadanite\r\n"
            .as_bytes()
            .to_vec()
    }

    #[test]
    fn a_raw_material_leads_to_its_refined_good_by_the_link_of_the_game_database() {
        let goods = goods_from_dataforge(&forge(), &ini()).expect("goods");
        let by_name = |name: &str| goods.iter().find(|good| good.name == name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(goods.len(), 5, "a good without a text stays out");
        assert_eq!(by_name("Iron (Ore)").refined.as_deref(), Some(by_name("Iron").guid.as_str()));
        assert_eq!(
            by_name("Raw Ice").refined.as_deref(),
            Some(by_name("Pressurized Ice").guid.as_str()),
            "a raw material that is called differently still reaches its refined good"
        );
        assert_eq!(by_name("Hadanite").refined, None, "an ore without a refined version has none");
        assert_eq!(by_name("Iron").refined, None);
    }

    // ---- the places

    /// A small catalog: Stanton with a moon (`HPP_Stanton2b` ↔ Daymar), a cluster preset with containers below
    /// points of two Lagrange points (one with a side cluster), a preset for salvage, and one without a place.
    fn catalog() -> String {
        r#"{
          "starmapObjects": [
            {"guid": "00000000-0000-4000-8000-0000000000a0", "className": "Stanton1_Star", "name": {"en": "Stanton"}, "type": "Star", "system": "stanton"},
            {"guid": "00000000-0000-4000-8000-0000000000a1", "className": "Pyro1_Star", "name": {"en": "Pyro"}, "type": "Star", "system": "pyro"},
            {"guid": "00000000-0000-4000-8000-0000000000b0", "className": "STANTON2B", "name": {"en": "Daymar"}, "type": "Moon", "parentGuid": "00000000-0000-4000-8000-0000000000a0", "system": null},
            {"guid": "00000000-0000-4000-8000-0000000000c0", "className": "MIC_L4", "name": {"en": "MIC L4"}, "type": "Manmade", "parentGuid": "00000000-0000-4000-8000-0000000000a0"},
            {"guid": "00000000-0000-4000-8000-0000000000c1", "className": "MIC_L4_A", "name": {"en": "MIC L4-A"}, "type": "Asteroid_ValidQT", "parentGuid": "00000000-0000-4000-8000-0000000000c0"},
            {"guid": "00000000-0000-4000-8000-0000000000c2", "className": "MIC_L5", "name": {"en": "MIC L5"}, "type": "Manmade", "parentGuid": "00000000-0000-4000-8000-0000000000a0"},
            {"guid": "00000000-0000-4000-8000-0000000000c3", "className": "Blank", "name": {"key": "@generic_locations_blank", "en": null}, "type": "Asteroid_ValidQT"}
          ],
          "mineableElements": [
            {"guid": "00000000-0000-4000-8000-0000000000e1", "className": "Iron_Ore", "resourceTypeGuid": "00000000-0000-0000-0000-000000000010"},
            {"guid": "00000000-0000-4000-8000-0000000000e2", "className": "Ice_Ore", "resourceTypeGuid": "00000000-0000-0000-0000-000000000012"},
            {"guid": "00000000-0000-4000-8000-0000000000e3", "className": "Unnamed_Ore", "resourceTypeGuid": "00000000-0000-0000-0000-0000000000ff"}
          ],
          "mineableCompositions": [
            {"guid": "00000000-0000-4000-8000-0000000000d1", "className": "IronRock", "depositName": {"en": "Iron rock"}, "parts": [
              {"mineableElementGuid": "00000000-0000-4000-8000-0000000000e1", "minPercentage": 40.0, "maxPercentage": 60.0, "probability": 1.0}]},
            {"guid": "00000000-0000-4000-8000-0000000000d2", "className": "IceRock", "depositName": {"en": null}, "parts": [
              {"mineableElementGuid": "00000000-0000-4000-8000-0000000000e2", "minPercentage": 10.0, "maxPercentage": 20.0, "probability": 0.5},
              {"mineableElementGuid": "00000000-0000-4000-8000-0000000000e3", "minPercentage": 1.0, "maxPercentage": 3.0}]},
            {"guid": "00000000-0000-4000-8000-0000000000d3", "className": "NotUsed", "parts": []}
          ],
          "harvestableProviderPresets": [
            {"guid": "00000000-0000-4000-8000-0000000000f1", "className": "HPP_Stanton2b", "groups": [
              {"groupName": "SpaceShip_Mineables", "groupProbability": 1.0, "harvestables": [
                {"relativeProbability": 3.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000D1"}},
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d2"}},
                {"relativeProbability": 9.0, "harvestable": {"mineableCompositionGuid": null}}]},
              {"groupName": "Salvage_Panels", "groupProbability": 1.0, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]},
              {"groupName": "Havestables", "groupProbability": 1.0, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]},
              {"groupName": "FPS_Mineables", "groupProbability": 0.5, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]},
              {"groupName": "FPS_Mineables", "groupProbability": 0.9, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d2"}}]}]},
            {"guid": "00000000-0000-4000-8000-0000000000f2", "className": "HPP_Lagrange_A", "groups": [
              {"groupName": "GroundVehicle_Mineables", "groupProbability": 2.0, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]}]},
            {"guid": "00000000-0000-4000-8000-0000000000f3", "className": "HPP_Nowhere", "groups": [
              {"groupName": "SpaceShip_Mineables", "groupProbability": 1.0, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]}]},
            {"guid": "00000000-0000-4000-8000-0000000000f4", "className": "HPP_OnlySalvage", "groups": [
              {"groupName": "Salvage_Hulls", "groupProbability": 1.0, "harvestables": [
                {"relativeProbability": 1.0, "harvestable": {"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}]}]}
          ],
          "harvestableLocations": [
            {"presetGuid": "00000000-0000-4000-8000-0000000000F2", "system": "stanton", "container": "a", "starmapObjectGuid": "00000000-0000-4000-8000-0000000000c1"},
            {"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "stanton", "container": "b", "starmapObjectGuid": "00000000-0000-4000-8000-0000000000c0"},
            {"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "stanton", "container": "c", "starmapObjectGuid": "00000000-0000-4000-8000-0000000000c2"},
            {"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "stanton", "container": "d", "starmapObjectGuid": null},
            {"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "stanton", "container": "e", "starmapObjectGuid": "00000000-0000-4000-8000-0000000000c3"},
            {"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "pyro", "container": "f", "starmapObjectGuid": "00000000-0000-4000-8000-0000000000c2"}
          ]
        }"#
        .to_string()
    }

    fn goods_of_forge() -> Vec<Good> {
        goods_from_dataforge(&forge(), &ini()).expect("goods")
    }

    fn built() -> Places {
        build(catalog().as_bytes(), &goods_of_forge()).expect("places")
    }

    fn place<'a>(places: &'a Places, name: &str) -> &'a Place {
        places.places.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("{name}: {:?}", places.places))
    }

    #[test]
    fn a_moon_is_found_by_the_name_of_its_preset_whatever_the_case_and_a_preset_without_a_place_is_left_out() {
        let places = built();
        let daymar = place(&places, "Daymar");
        assert_eq!((daymar.template.as_str(), daymar.kind.as_str()), ("HPP_Stanton2b", "Moon"));
        assert_eq!(daymar.system, "Stanton", "the system comes from the star above the moon");
        assert_eq!(daymar.points, 1);
        assert!(places.places.iter().all(|p| p.template != "HPP_Nowhere"), "a preset no object and no container names");
        assert!(places.places.iter().all(|p| p.template != "HPP_OnlySalvage"), "a preset with no way of mining");
    }

    #[test]
    fn the_points_of_a_cluster_become_one_place_and_a_side_cluster_is_its_main_point() {
        let places = built();
        let cluster = places.places.iter().find(|p| p.template == "HPP_Lagrange_A" && p.system == "Stanton").unwrap();
        assert_eq!(cluster.name, "MIC L4 · MIC L5", "MIC L4-A and MIC L4 are one point; null and unnamed rows count none");
        assert_eq!((cluster.kind.as_str(), cluster.points), ("Cluster", 2));
        let other = places.places.iter().find(|p| p.template == "HPP_Lagrange_A" && p.system == "Pyro").unwrap();
        assert_eq!(other.name, "MIC L5", "per system, with the name of the star");
    }

    #[test]
    fn a_cluster_with_many_points_writes_out_a_few_and_counts_the_rest() {
        let objects: String = (0..9)
            .map(|n| {
                format!(
                    r#"{{"guid": "00000000-0000-4000-8000-00000000{n:04x}", "className": "P{n}", "name": {{"en": "Point {n}"}}, "type": "Manmade"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let rows: String = (0..9)
            .map(|n| {
                format!(
                    r#"{{"presetGuid": "00000000-0000-4000-8000-0000000000f2", "system": "nyx", "container": "c{n}", "starmapObjectGuid": "00000000-0000-4000-8000-00000000{n:04x}"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let text = format!(
            r#"{{"starmapObjects": [{objects}],
               "harvestableProviderPresets": [{{"guid": "00000000-0000-4000-8000-0000000000f2", "className": "HPP_Nyx", "groups": [
                 {{"groupName": "SpaceShip_Mineables", "groups": [], "harvestables": [{{"relativeProbability": 1.0, "harvestable": {{"mineableCompositionGuid": "x"}}}}]}}]}}],
               "harvestableLocations": [{rows}]}}"#
        );
        let places = build(text.as_bytes(), &[]).unwrap();
        let nyx = &places.places[0];
        assert_eq!(nyx.name, "Point 0 · Point 1 · Point 2 · Point 3 · Point 4 · Point 5 · … +3");
        assert_eq!(nyx.points, 9);
        assert_eq!(nyx.system, "Nyx", "a system no star names is its folder with a capital");
    }

    #[test]
    fn only_the_three_ways_of_mining_count_and_each_group_once() {
        let places = built();
        let daymar = place(&places, "Daymar");
        let names: Vec<&str> = daymar.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["SpaceShip_Mineables", "FPS_Mineables"], "no salvage, no loose goods, no second FPS group");
        let ship = &daymar.groups[0];
        assert_eq!(ship.probability, 1.0);
        let deposits: Vec<(&str, f64)> = ship.deposits.iter().map(|d| (d.composition.as_str(), d.share)).collect();
        assert_eq!(
            deposits,
            [("00000000-0000-4000-8000-0000000000d1", 3.0), ("00000000-0000-4000-8000-0000000000d2", 1.0)],
            "GUIDs in lower case, a harvestable without a composition is no deposit"
        );
        assert_eq!(daymar.groups[1].probability, 0.5, "the hand-mined group is the first of its name");
        let cluster = places.places.iter().find(|p| p.template == "HPP_Lagrange_A").unwrap();
        assert_eq!(cluster.groups[0].name, "GroundVehicle_Mineables");
    }

    #[test]
    fn the_compositions_name_their_raw_materials_by_the_game_database_and_the_ores_their_refined_goods() {
        let places = built();
        assert_eq!(places.compositions.len(), 2, "only what a place uses");
        let iron = &places.compositions["00000000-0000-4000-8000-0000000000d1"];
        assert_eq!(iron.name, "Iron rock");
        assert_eq!(
            iron.parts,
            [Part {
                element: "00000000-0000-4000-8000-0000000000e1".to_string(),
                ore: "Iron (Ore)".to_string(),
                min: 40.0,
                max: 60.0,
                probability: 1.0
            }]
        );
        let ice = &places.compositions["00000000-0000-4000-8000-0000000000d2"];
        assert_eq!(ice.name, "IceRock", "the class name where the deposit has no name");
        assert_eq!(ice.parts[0].ore, "Raw Ice");
        assert_eq!(ice.parts[1].ore, "Unnamed_Ore", "an element whose good has no name keeps its class name");
        assert_eq!(ice.parts[1].probability, 1.0, "no chance stated is a sure one");
        assert_eq!(
            places.ores,
            [
                ("Iron (Ore)".to_string(), Some("Iron".to_string())),
                ("Raw Ice".to_string(), Some("Pressurized Ice".to_string())),
                ("Unnamed_Ore".to_string(), None),
            ]
        );
    }

    // ---- the file and its keeping

    fn big_catalog(places: usize) -> String {
        let objects: String = (0..places)
            .map(|n| format!(r#"{{"guid": "00000000-0000-4000-8000-00000001{n:04x}", "className": "Body{n}", "name": {{"en": "Body {n}"}}, "type": "Moon"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let presets: String = (0..places)
            .map(|n| {
                format!(
                    r#"{{"guid": "00000000-0000-4000-8000-00000002{n:04x}", "className": "HPP_Body{n}", "groups": [{{"groupName": "SpaceShip_Mineables", "groupProbability": 1.0, "harvestables": [{{"relativeProbability": 1.0, "harvestable": {{"mineableCompositionGuid": "00000000-0000-4000-8000-0000000000d1"}}}}]}}]}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let tail = catalog();
        let elements_and_compositions = &tail[tail.find("\"mineableElements\"").unwrap()..tail.find("\"harvestableProviderPresets\"").unwrap()];
        format!(r#"{{"starmapObjects": [{objects}], {elements_and_compositions} "harvestableProviderPresets": [{presets}]}}"#)
    }

    #[test]
    fn the_file_is_written_once_per_state_of_the_archive_and_not_with_too_few_places() {
        let dir = versekit_core::testdir::Dir::new("fundorte");
        let archive = dir.path().join("Data.p4k");
        let home = dir.path().to_string_lossy().into_owned();
        let paths = versekit_core::paths::Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[("VERSEKIT_HOME", home.as_str())],
            &home,
        ));
        let file = paths.app_file(FILE);
        assert_eq!(keep_current_with(&archive, &file, || unreachable!("no archive, no reading")), Keep::NoArchive);
        std::fs::write(&archive, b"p4k").unwrap();
        let many = || build(big_catalog(MINIMUM).as_bytes(), &goods_of_forge()).map_err(|s| Error::Places { source: Box::new(s) });
        assert_eq!(keep_current_with(&archive, &file, many), Keep::Written);
        let kept = jsonio::read_json(&file).expect("the file");
        assert_eq!(kept.get("format"), Some(&Json::int(FORMAT)));
        assert_eq!(kept.get("stand").and_then(Json::as_str), archive_stamp(&archive).as_deref());
        assert_eq!(kept.get("orte").and_then(|o| match o { Json::Array(list) => Some(list.len()), _ => None }), Some(MINIMUM));
        assert_eq!(
            keep_current_with(&archive, &file, || unreachable!("the same archive is not read again")),
            Keep::Current
        );
        // A patch: read again; too few places leave the file as it was; a broken catalog is a failure.
        std::fs::write(&archive, b"patched p4k").unwrap();
        let few = || build(big_catalog(3).as_bytes(), &goods_of_forge()).map_err(|s| Error::Places { source: Box::new(s) });
        assert_eq!(keep_current_with(&archive, &file, few), Keep::TooFew(3));
        assert_eq!(jsonio::read_json(&file).unwrap(), kept);
        let broken = || build(b"{kaputt", &[]).map_err(|s| Error::Places { source: Box::new(s) });
        assert!(matches!(keep_current_with(&archive, &file, broken), Keep::Failed(_)));
        assert_eq!(jsonio::read_json(&file).unwrap(), kept, "a broken catalog leaves the file");
        assert_eq!(keep_current_with(&archive, &file, many), Keep::Written);
    }

    #[test]
    fn the_places_of_the_file_give_the_shares_by_hand_the_mining_data_work_out() {
        let dir = versekit_core::testdir::Dir::new("fundorte-anteile");
        let file = dir.path().join(FILE).to_string_lossy().into_owned();
        // Daymar of `catalog()` by ship: Iron rock (3) and Ice rock (1): Iron 3/4 * 50 = 37.5 against
        // Raw Ice 1/4 * 0.5 * 15 = 1.875 and Unnamed_Ore 1/4 * 1 * 2 = 0.5.
        let text = file_text("stamp", &built());
        std::fs::write(&file, text).unwrap();
        let mining = versekit_core::mining_data::MiningData::load(
            &dir.path().join(versekit_core::mining_data::FILE).to_string_lossy(),
        );
        assert!(mining.places_from_game_data());
        let iron = mining.ores().iter().find(|o| o.name == "Iron (Ore)").expect("iron");
        let at_daymar = iron.places.iter().find(|p| p.name == "Daymar").expect("Daymar");
        let by_ship = at_daymar.per_tool.iter().find(|t| t.tool == "schiff").expect("by ship");
        assert!((by_ship.share - 37.5 / (37.5 + 1.875 + 0.5)).abs() < 1e-9, "{by_ship:?}");
        assert_eq!(mining.material_key("Raw Ice"), mining.material_key("Pressurized Ice"));
    }

    /// Reads the places of the archive in `VERSEKIT_P4K` into a folder of its own and prints how long it took.
    #[test]
    #[ignore = "needs a real Data.p4k in VERSEKIT_P4K; writes into a temporary folder"]
    fn the_real_archive_gives_the_places_and_says_how_long_it_took() {
        let Ok(archive) = std::env::var("VERSEKIT_P4K") else { return };
        let dir = versekit_core::testdir::Dir::new("fundorte-echt");
        let file = dir.path().join(FILE).to_string_lossy().into_owned();
        let started = std::time::Instant::now();
        let outcome = keep_current(Path::new(&archive), &file);
        eprintln!("places of the archive: {outcome:?} in {:?}", started.elapsed());
        assert_eq!(outcome, Keep::Written);
        let again = std::time::Instant::now();
        assert_eq!(keep_current(Path::new(&archive), &file), Keep::Current);
        eprintln!("the same archive again: {:?}", again.elapsed());
        let places = jsonio::read_json(&file).expect("the file");
        let count = places.get("orte").and_then(|o| match o { Json::Array(list) => Some(list.len()), _ => None });
        eprintln!("places in the file: {count:?}");
        assert!(count.is_some_and(|n| n >= MINIMUM));
    }
}
