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

//! The game values from the scmdb.net files of a build: the fallback when
//! the own reading of the `Data.p4k` fails, and the standing cross-check
//! against it (see [`crate::value_source`]).
//!
//! Read are `crafting_blueprints-<build>.json` (the recipes) and
//! `crafting_items-<build>.json` (the base values of the products), raw from
//! scmdb or as 4.12 keeps them (`crafting-blueprints.json` with `products`,
//! the pools already resolved). Nothing of it is shipped: the files are
//! fetched at run time on the user's computer (scmdb is CC BY-NC-ND 4.0, the
//! raw data belong to CIG).

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};
use versekit_core::scmdb::FetchError;

use crate::values::{
    Ammo, Armor, BeamInfo, Corrected, DAMAGE_KINDS, Damage, Dismantle, FireAction, FireMode, GameValues, ItemExtra,
    ItemType, ItemValues, MaterialKind, Modifier, ModifierKind, Recipe, RecipeTier, Recoil, Slot, SlotOption, Stat,
    Weapon,
};

/// The name of the recipe file of `build` at scmdb.
pub fn blueprints_file(build: &str) -> String {
    format!("crafting_blueprints-{build}.json")
}

/// The name of the item file of `build` at scmdb.
pub fn items_file(build: &str) -> String {
    format!("crafting_items-{build}.json")
}

fn fetch_json(name: &str, user_agent: &str) -> Result<Value, FetchError> {
    let bytes = versekit_core::scmdb::fetch_file(name, user_agent)?;
    serde_json::from_slice(&bytes).map_err(|error| FetchError::Failed(format!("{name}: {error}")))
}

/// The game values of `build` from scmdb's recipe and item files, fetched at
/// run time (see `versekit_core::scmdb::fetch_file`).
///
/// # Errors
///
/// The fetch error, or [`FetchError::Failed`] when a file is no JSON.
pub fn fetch_values(build: &str, user_agent: &str) -> Result<GameValues, FetchError> {
    let blueprints = fetch_json(&blueprints_file(build), user_agent)?;
    let items = fetch_json(&items_file(build), user_agent)?;
    Ok(values(Some(build), &blueprints, &items))
}

/// scmdb's current build of the LIVE channel (the first entry of
/// `versions.json` that is no PTU), e.g. `4.10.2-live.12913470`.
///
/// # Errors
///
/// The fetch error, or [`FetchError::Missing`] when the list names none.
pub fn current_build(user_agent: &str) -> Result<String, FetchError> {
    let versions = fetch_json("versions.json", user_agent)?;
    list(Some(&versions))
        .iter()
        .filter_map(|entry| entry.get("version").and_then(Value::as_str))
        .find(|name| !name.is_empty() && !name.to_ascii_lowercase().contains("ptu"))
        .map(str::to_string)
        .ok_or(FetchError::Missing)
}

/// Reference field, shared table and the field the resolved entry goes to,
/// in the raw item file.
const POOLS: [(&str, &str, &str); 5] = [
    ("fireModesIndex", "fireModesPools", "fireModes"),
    ("ammoIndex", "ammoPools", "ammo"),
    ("magazineIndex", "magazinePools", "magazine"),
    ("damageResistanceIndex", "damageResistancePools", "damageResistance"),
    ("signaturesIndex", "signaturesPools", "signatures"),
];

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|value| value.is_finite())
}

fn field(object: &Map<String, Value>, name: &str) -> Option<f64> {
    number(object.get(name))
}

fn whole(object: &Map<String, Value>, name: &str) -> Option<i64> {
    object.get(name).and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|v| v as i64)))
}

fn string(object: &Map<String, Value>, name: &str) -> Option<String> {
    object.get(name).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_string)
}

fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn list(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// A damage table; an empty or missing one is none.
fn damage(value: Option<&Value>) -> Option<Damage> {
    let table = object(value).filter(|table| !table.is_empty())?;
    Some(Damage(DAMAGE_KINDS.map(|kind| field(table, kind).unwrap_or(0.0))))
}

fn action(entry: &Map<String, Value>) -> FireAction {
    let spread = object(entry.get("spread")).and_then(|spread| Some((field(spread, "min")?, field(spread, "max")?)));
    let recoil = object(entry.get("recoil")).filter(|recoil| !recoil.is_empty()).map(|recoil| Recoil {
        yaw_max_deg: field(recoil, "yawMaxDeg"),
        pitch_max_deg: field(recoil, "pitchMaxDeg"),
        smooth_time: field(recoil, "smoothTime"),
    });
    FireAction {
        name: string(entry, "name"),
        kind: string(entry, "type").unwrap_or_default(),
        fire_rate: field(entry, "fireRate"),
        delay: field(entry, "delay"),
        damage_multiplier: field(entry, "damageMultiplier"),
        pellet_count: whole(entry, "pelletCount"),
        heat_per_shot: field(entry, "heatPerShot"),
        spread,
        recoil,
    }
}

fn weapon(item: &Map<String, Value>) -> Weapon {
    let fire_modes = list(item.get("fireModes"))
        .iter()
        .filter_map(Value::as_object)
        .map(|mode| FireMode {
            action: action(mode),
            sequence: list(mode.get("sequenceEntries")).iter().filter_map(Value::as_object).map(action).collect(),
        })
        .collect();
    let ammo = object(item.get("ammo")).map(|ammo| Ammo {
        speed: field(ammo, "speed"),
        lifetime: field(ammo, "lifetime"),
        damage: damage(ammo.get("damage")),
        detonation: damage(ammo.get("detonationDamage")),
    });
    let magazine = object(item.get("magazine")).and_then(|magazine| whole(magazine, "ammoCount"));
    Weapon { fire_modes, ammo, magazine }
}

fn armor(item: &Map<String, Value>) -> Option<Armor> {
    let resistance = object(item.get("damageResistance"));
    let temperature = object(item.get("temperatureResistance")).filter(|temp| !temp.is_empty());
    let radiation = object(item.get("radiationResistance")).filter(|rad| !rad.is_empty());
    if resistance.is_none() && temperature.is_none() && radiation.is_none() {
        return None;
    }
    Some(Armor {
        resistance: DAMAGE_KINDS.map(|kind| {
            resistance.and_then(|table| object(table.get(kind))).and_then(|cell| field(cell, "multiplier"))
        }),
        temperature: temperature.map(|temp| (field(temp, "min").unwrap_or(0.0), field(temp, "max").unwrap_or(0.0))),
        radiation_capacity: radiation.and_then(|rad| field(rad, "capacity")),
        radiation_dissipation: radiation.and_then(|rad| field(rad, "dissipationRate")),
    })
}

/// One item of the item file; `pools` is the whole raw file, whose shared
/// tables the references point into, or `None` for an item whose tables are
/// resolved already.
pub fn item(entry: &Map<String, Value>, pools: Option<&Map<String, Value>>) -> ItemValues {
    let mut resolved;
    let mut entry = entry;
    if let Some(pools) = pools {
        resolved = entry.clone();
        for (index_key, pool_key, target) in POOLS {
            let index = resolved.remove(index_key).and_then(|index| index.as_u64());
            let pool = list(pools.get(pool_key));
            if let Some(found) = index.and_then(|index| usize::try_from(index).ok()).and_then(|index| pool.get(index)) {
                resolved.insert(target.to_string(), found.clone());
            }
        }
        entry = &resolved;
    }
    let item_type = match entry.get("itemType").and_then(Value::as_str) {
        Some("weapon") => ItemType::Weapon,
        Some("shipcomponent") => ItemType::ShipComponent,
        Some("armor") => ItemType::Armor,
        _ => ItemType::Other,
    };
    let stats: BTreeMap<Stat, f64> =
        Stat::ALL.into_iter().filter_map(|stat| Some((stat, field(entry, stat.scmdb_field())?))).collect();
    let has_weapon = item_type == ItemType::Weapon || entry.contains_key("fireModes");
    ItemValues {
        guid: string(entry, "entityClass").unwrap_or_default().to_ascii_lowercase(),
        class_name: string(entry, "className").unwrap_or_default(),
        name: string(entry, "name"),
        item_type,
        attach_type: string(entry, "attachType"),
        cg_item_type: string(entry, "cgItemType"),
        size: whole(entry, "size"),
        grade: whole(entry, "grade"),
        manufacturer: string(entry, "manufacturer"),
        manufacturer_record: None,
        weapon: has_weapon.then(|| weapon(entry)),
        stats,
        beams: list(entry.get("beams"))
            .iter()
            .filter_map(Value::as_object)
            .map(|beam| field(beam, "dps").unwrap_or(0.0))
            .collect(),
        armor: armor(entry),
        corrected: Vec::new(),
        extra: extra(entry),
    }
}

/// The scmdb number fields the table of product values shows besides the
/// quality stats.
const EXTRA_NUMBERS: [&str; 18] = [
    "mass",
    "powerDraw",
    "emSignature",
    "irSignature",
    "selfRepairTime",
    "sensitivity",
    "piercing",
    "pingCooldown",
    "aimAssistRange",
    "minForce",
    "minDistance",
    "cargoCapacitySCU",
    "charges",
    "fuelRate",
    "quantumFuelRate",
    "spoolUpTime",
    "cooldownTime",
    "stageOneAccelRate",
];

fn extra(entry: &Map<String, Value>) -> ItemExtra {
    let mut found = ItemExtra::default();
    for name in EXTRA_NUMBERS {
        if let Some(value) = field(entry, name) {
            found.numbers.insert(name.into(), value);
        }
    }
    if let Some(overheat) = object(entry.get("heat")).and_then(|heat| field(heat, "overheatTemperature")) {
        found.numbers.insert("overheatTemperature".into(), overheat);
    }
    for name in ["componentClass", "attachSubType", "manufacturerCode"] {
        if let Some(text) = string(entry, name) {
            found.texts.insert(name.into(), text);
        }
    }
    if let Some(category) = object(entry.get("combatRange")).and_then(|range| string(range, "category")) {
        found.texts.insert("combatRange".into(), category);
    }
    if let Some(modifiers) = object(entry.get("modifiers")) {
        for (name, value) in modifiers {
            if let Some(number) = value.as_f64() {
                found.modifiers.insert(name.clone(), number);
            }
        }
    }
    found.beams = list(entry.get("beams"))
        .iter()
        .filter_map(Value::as_object)
        .map(|beam| BeamInfo {
            hit_type: string(beam, "hitType").unwrap_or_default(),
            full_range: field(beam, "fullDamageRange"),
            zero_range: field(beam, "zeroDamageRange"),
        })
        .collect();
    found
}

fn modifier(entry: &Map<String, Value>) -> Modifier {
    Modifier {
        property_key: string(entry, "propertyKey").unwrap_or_default(),
        property_name: string(entry, "propertyName"),
        kind: if entry.get("additive").and_then(Value::as_bool).unwrap_or(false) {
            ModifierKind::Additive
        } else {
            ModifierKind::Multiplier
        },
        start_quality: field(entry, "startQuality").unwrap_or(0.0),
        end_quality: field(entry, "endQuality").unwrap_or(0.0),
        at_start: field(entry, "modifierAtStart").unwrap_or(1.0),
        at_end: field(entry, "modifierAtEnd").unwrap_or(1.0),
        corrected: Vec::new(),
    }
}

fn modifiers(value: Option<&Value>) -> Vec<Modifier> {
    list(value).iter().filter_map(Value::as_object).map(modifier).collect()
}

fn slot(entry: &Map<String, Value>) -> Slot {
    let options = list(entry.get("options"))
        .iter()
        .filter_map(Value::as_object)
        .map(|option| {
            let item = option.get("type").and_then(Value::as_str) == Some("item");
            SlotOption {
                kind: if item { MaterialKind::Item } else { MaterialKind::Resource },
                material: string(option, "resourceName").or_else(|| string(option, "itemName")),
                material_guid: None,
                quantity: field(option, "quantity"),
                min_quality: whole(option, "minQuality"),
                modifiers: modifiers(option.get("modifiers")),
            }
        })
        .collect();
    Slot { name: string(entry, "name").unwrap_or_default(), options, modifiers: modifiers(entry.get("modifiers")) }
}

/// One recipe of the recipe file, as scmdb gives it: the product is the one
/// the record names, even where scmdb marks it as a CIG data error.
pub fn recipe(entry: &Map<String, Value>) -> Recipe {
    Recipe {
        guid: string(entry, "guid").unwrap_or_default().to_ascii_lowercase(),
        tag: string(entry, "tag").unwrap_or_default(),
        product: string(entry, "productEntityClass").map(|guid| guid.to_ascii_lowercase()),
        product_missing: false,
        kind: string(entry, "type").unwrap_or_default(),
        subtype: string(entry, "subtype").unwrap_or_default(),
        tiers: list(entry.get("tiers"))
            .iter()
            .filter_map(Value::as_object)
            .map(|tier| RecipeTier {
                craft_time_seconds: whole(tier, "craftTimeSeconds"),
                slots: list(tier.get("slots")).iter().filter_map(Value::as_object).map(slot).collect(),
            })
            .collect(),
        corrected: Vec::new(),
    }
}

/// The items of an item file: the raw scmdb file (`items` with shared
/// tables) or the 4.12 keeping (`{entity GUID: item}`).
pub fn items(file: &Value) -> HashMap<String, ItemValues> {
    let mut found = HashMap::new();
    if let Some(raw) = file.as_object().filter(|raw| raw.contains_key("items")) {
        for entry in list(raw.get("items")).iter().filter_map(Value::as_object) {
            let item = item(entry, Some(raw));
            found.insert(item.guid.clone(), item);
        }
    } else if let Some(kept) = file.as_object() {
        for entry in kept.values().filter_map(Value::as_object) {
            let item = item(entry, None);
            found.insert(item.guid.clone(), item);
        }
    }
    found
}

/// The game values of the recipe file `blueprints` and the item file
/// `items` of `build`.
///
/// Where scmdb marks a recipe's product as a CIG data error and names the
/// meant one, the meant one is used when its values are known (as 4.12
/// `product_stats.product_for` does), with a correction mark.
pub fn values(build: Option<&str>, blueprints: &Value, items_file: &Value) -> GameValues {
    let items = items(items_file);
    let mut recipes = Vec::new();
    for entry in list(blueprints.get("blueprints")).iter().filter_map(Value::as_object) {
        let mut recipe = recipe(entry);
        let meant = string(entry, "suggestedProductEntityClass").map(|guid| guid.to_ascii_lowercase());
        if entry.get("cigDataError").and_then(Value::as_bool).unwrap_or(false)
            && let Some(meant) = meant.filter(|meant| items.contains_key(meant))
            && recipe.product.as_deref() != Some(meant.as_str())
        {
            recipe.corrected.push(Corrected {
                field: "product".into(),
                raw: recipe.product.clone().unwrap_or_default(),
                value: meant.clone(),
                reason: string(entry, "suggestedName").map(|name| format!("meant: {name}")).unwrap_or_default(),
                source: "scmdb".into(),
            });
            recipe.product = Some(meant);
        }
        recipes.push(recipe);
    }
    recipes.sort_by_cached_key(|recipe| recipe.tag.to_ascii_lowercase());
    GameValues {
        build: build.map(str::to_string),
        items,
        recipes,
        failed: Vec::new(),
        dismantle: dismantle(blueprints),
        crafter_limits: None,
    }
}

/// What taking an item apart gives back, from the `dismantle` block of the
/// recipe file; what it lacks stays at the default.
fn dismantle(blueprints: &Value) -> Dismantle {
    let mut found = Dismantle::default();
    let Some(block) = blueprints.get("dismantle").and_then(Value::as_object) else { return found };
    if let Some(efficiency) = field(block, "efficiency") {
        found.efficiency = efficiency;
    }
    if let Some(seconds) = whole(block, "dismantleTimeSeconds") {
        found.seconds = seconds;
    }
    let names = |key: &str| -> Vec<String> {
        list(block.get(key)).iter().filter_map(Value::as_object).filter_map(|entry| string(entry, "name")).collect()
    };
    found.resources = names("blacklistedResources");
    found.entities = names("blacklistedEntityClasses");
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_item_resolves_its_shared_tables() {
        let file = serde_json::json!({
            "items": [{"entityClass": "ABC", "itemType": "weapon", "fireModesIndex": 0, "ammoIndex": 1,
                       "health": 5.0}],
            "fireModesPools": [[{"name": "Rapid", "type": "rapid", "fireRate": 600.0}]],
            "ammoPools": [{}, {"speed": 1000.0, "lifetime": 2.0, "damage": {"physical": 10.0}}],
        });
        let items = items(&file);
        let item = &items["abc"];
        let weapon = item.weapon.as_ref().unwrap();
        assert_eq!(weapon.fire_modes[0].action.fire_rate, Some(600.0));
        assert_eq!(weapon.ammo.as_ref().unwrap().range(), Some(2000.0));
        assert_eq!(weapon.ammo.as_ref().unwrap().damage.unwrap().total(), 10.0);
        assert_eq!(item.stat(Stat::Health), Some(5.0));
    }

    #[test]
    fn the_meant_product_replaces_a_marked_one() {
        let blueprints = serde_json::json!({"blueprints": [
            {"guid": "B1", "tag": "BP_X", "productEntityClass": "wrong", "cigDataError": true,
             "suggestedProductEntityClass": "RIGHT", "suggestedName": "Right One", "tiers": []}]});
        let kept = serde_json::json!({"right": {"entityClass": "RIGHT", "itemType": "shipcomponent"}});
        let values = values(Some("4.10.2-live.1"), &blueprints, &kept);
        let recipe = &values.recipes[0];
        assert_eq!(recipe.product.as_deref(), Some("right"));
        assert_eq!(recipe.corrected[0].raw, "wrong");
        assert!(values.product(recipe).is_some());
    }
}
