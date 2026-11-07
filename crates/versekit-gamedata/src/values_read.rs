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

//! Reading the game values out of a DataForge: every recipe, the item each
//! one makes, and every ship weapon, personal weapon and shield, crafted or
//! not (the hangar compares with the factory loadout and shows bought
//! parts at quality 500).

use std::collections::HashMap;

use p4kx_dataforge::{DataForge, Guid};

use crate::catalog::starts_with_ignore_case;
use crate::fields::{Trees, component, elements, integer, link, number, structure};
use crate::recipes::english;
use crate::values::{Dismantle, GameValues, ItemType};
use crate::{Localization, corrections, item_values, recipes};

/// The record path prefix of the items.
const ITEM_PREFIX: &str = "libs/foundry/records/entities/scitem/";

/// The attach types read for every item, crafted or not.
const ALWAYS: [&str; 3] = ["WeaponGun", "WeaponPersonal", "Shield"];

/// The record path prefix of the blueprint that says what taking apart gives back.
const DISMANTLE_PREFIX: &str = "libs/foundry/records/crafting/blueprints/dismantle/";

/// What taking an item apart gives back: the share and the time from the
/// dismantle blueprint, the resources and items that never come back from
/// the global crafting parameters.
fn dismantle(trees: &mut Trees<'_, '_>, globals: &[Guid], record: Option<Guid>, texts: &Localization) -> Dismantle {
    let mut found = Dismantle::default();
    if let Some(tree) = record.and_then(|guid| trees.once(guid)) {
        let process = structure(structure(Some(tree.root()), "blueprint"), "processSpecificData");
        if let Some(efficiency) = number(process, "efficiency") {
            found.efficiency = efficiency;
        }
        let time = structure(process, "dismantleTime");
        if time.is_some() {
            let part = |name: &str| integer(time, name).unwrap_or(0);
            found.seconds = part("days") * 86_400 + part("hours") * 3600 + part("minutes") * 60 + part("seconds");
        }
    }
    for guid in globals {
        let Some(tree) = trees.once(*guid) else { continue };
        for (field, resources) in [("dismantleBlacklistResources", true), ("dismantleBlacklistEntityClasses", false)] {
            for value in elements(Some(tree.root()), field) {
                let Some(target) = link(value).and_then(|guid| trees.get(guid)) else { continue };
                let name = if resources {
                    english(Some(target.root()), "displayName", texts)
                } else {
                    let definition = structure(component(&target, "SAttachableComponentParams"), "AttachDef");
                    english(structure(definition, "Localization"), "Name", texts)
                };
                if let Some(name) = name {
                    let list = if resources { &mut found.resources } else { &mut found.entities };
                    if !list.contains(&name) {
                        list.push(name);
                    }
                }
            }
        }
    }
    found
}

pub(crate) fn read(forge: &DataForge<'_>, texts: &Localization, build: Option<String>) -> GameValues {
    let mut trees = Trees::new(forge);
    let mut entities = Vec::new();
    let mut globals = Vec::new();
    let mut dismantle_record = None;
    for record in forge.records() {
        let (Ok(type_name), Ok(path)) = (record.type_name(), record.record_path()) else { continue };
        match type_name {
            "EntityClassDefinition" if starts_with_ignore_case(path, ITEM_PREFIX) => entities.push(record.guid()),
            "CraftingGlobalParams" => globals.push(record.guid()),
            "CraftingBlueprintRecord" if starts_with_ignore_case(path, DISMANTLE_PREFIX) => {
                dismantle_record.get_or_insert(record.guid());
            }
            _ => {}
        }
    }
    let mut values = GameValues { build, ..GameValues::default() };
    values.dismantle = dismantle(&mut trees, &globals, dismantle_record, texts);
    // The recipes come from the crafting data of the p4k-extractor, which
    // leaves the development leftovers out by the rule the catalog uses.
    match p4kx_export::read_crafting(forge, None, None) {
        Ok(crafting) => {
            let properties = recipes::Properties::new(&crafting.properties, texts);
            for blueprint in &crafting.blueprints {
                values.recipes.push(recipes::read(blueprint, &mut trees, texts, &properties));
            }
            values
                .failed
                .extend(crafting.failures.iter().map(|f| format!("{} ({}): {}", f.record_path, f.guid, f.reason)));
        }
        Err(error) => values.failed.push(format!("crafting data: {error}")),
    }
    values.recipes.sort_by_cached_key(|recipe| recipe.tag.to_ascii_lowercase());
    // The products, and the meant products of the corrections VerseKit knows.
    let mut wanted: Vec<Guid> = values
        .recipes
        .iter()
        .filter(|recipe| !recipe.product_missing)
        .filter_map(|recipe| recipe.product.as_deref())
        .chain(
            corrections::built_in()
                .iter()
                .filter(|c| c.field == corrections::Field::Product)
                .filter_map(|c| c.expected.as_deref()),
        )
        .filter_map(|guid| guid.parse().ok())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut items = HashMap::new();
    for guid in wanted {
        if let Some(tree) = trees.once(guid) {
            let item = item_values::read(&tree, &mut trees, texts);
            items.insert(item.guid.clone(), item);
        }
    }
    for guid in entities {
        if items.contains_key(&guid.to_string()) {
            continue;
        }
        let Some(tree) = trees.once(guid) else { continue };
        if !item_values::attach_type_is(&tree, &ALWAYS) {
            continue;
        }
        let item = item_values::read(&tree, &mut trees, texts);
        if matches!(item.item_type, ItemType::Weapon | ItemType::ShipComponent) {
            items.insert(item.guid.clone(), item);
        }
    }
    values.items = items;
    values.failed.extend(trees.into_failed());
    values
}
