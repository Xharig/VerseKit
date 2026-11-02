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

//! The recipes of the crafting blueprints with the quality modifiers of
//! each slot, from the crafting data of the p4k-extractor
//! (`p4kx_export::read_crafting`, krt-profit/p4k-extractor#318).
//!
//! A tier's mandatory cost is a selection whose options are the slots, each
//! a selection itself with its name, the modifiers of its context and the
//! materials (a resource in SCU, an item as a count), which may carry
//! modifiers of their own. A modifier names a `CraftingGameplayPropertyDef`
//! record, whose record name without `gpp_` is the property key. The names
//! of slots, materials and properties are looked up here in English.

use std::collections::HashMap;

use p4kx_dataforge::{Guid, RecordTree, StructValue, Value, ValueError};
use p4kx_export::{BlueprintTiers, Cost, CostContext, GameplayProperty, Tier};
use versekit_core::text::py_strip;

use crate::Localization;
use crate::fields::{Trees, locale_key, structure};
use crate::values::{MaterialKind, Modifier, ModifierKind, Recipe, RecipeTier, Slot, SlotOption};

/// The English text of the key in the field `name`, without white space
/// around it; `None` without one.
pub(crate) fn english(parent: Option<&StructValue<'_>>, name: &str, texts: &Localization) -> Option<String> {
    english_key(locale_key(parent, name), texts)
}

/// The English text of `key`, without white space around it; `None`
/// without one.
fn english_key(key: Option<&str>, texts: &Localization) -> Option<String> {
    let text = texts.english(key?)?;
    Some(py_strip(&text).to_string()).filter(|text| !text.is_empty())
}

/// A debug name such as `APERTURE IRIS` as a title: `Aperture Iris`.
fn title(debug_name: &str) -> String {
    debug_name
        .split(' ')
        .map(|word| {
            let lower = word.to_lowercase();
            let mut chars = lower.chars();
            chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// A 32-bit float as the shortest decimal that gives it back (`0.95`, not
/// `0.949999988079071`).
fn decimal(value: Option<f32>) -> Option<f64> {
    value.filter(|value| value.is_finite()).and_then(|value| value.to_string().parse().ok())
}

/// The property key and English name of each gameplay property.
pub(crate) struct Properties {
    by_guid: HashMap<Guid, (String, Option<String>)>,
}

impl Properties {
    pub(crate) fn new(properties: &[GameplayProperty<'_>], texts: &Localization) -> Self {
        let by_guid = properties
            .iter()
            .map(|property| {
                let path = property.record_path;
                let last = path.rsplit(['/', '\\']).next().unwrap_or(path).to_ascii_lowercase();
                let key = last.strip_prefix("gpp_").map(str::to_string).unwrap_or(last);
                (property.guid, (key, english_key(property.name_key, texts)))
            })
            .collect();
        Self { by_guid }
    }
}

/// The modifiers in `context`, one entry per range.
fn modifiers(context: &CostContext<'_>, properties: &Properties) -> Vec<Modifier> {
    let mut found = Vec::new();
    for modifier in &context.modifiers {
        let (key, name) = modifier.property.and_then(|guid| properties.by_guid.get(&guid)).cloned().unwrap_or_default();
        for range in &modifier.ranges {
            let kind = match range.kind {
                p4kx_export::ModifierKind::Additive => ModifierKind::Additive,
                _ => ModifierKind::Multiplier,
            };
            found.push(Modifier {
                property_key: key.clone(),
                property_name: name.clone(),
                kind,
                start_quality: range.start_quality.map_or(0.0, |q| q as f64),
                end_quality: range.end_quality.map_or(0.0, |q| q as f64),
                at_start: decimal(range.at_start).unwrap_or(1.0),
                at_end: decimal(range.at_end).unwrap_or(1.0),
                corrected: Vec::new(),
            });
        }
    }
    found
}

/// The English name of the item `tree` describes.
fn item_name(tree: &RecordTree<'_>, texts: &Localization) -> Option<String> {
    let definition = structure(crate::fields::component(tree, "SAttachableComponentParams"), "AttachDef");
    english(structure(definition, "Localization"), "Name", texts)
}

/// A material option: a resource or an item line.
fn option(
    cost: &Cost<'_>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
    properties: &Properties,
) -> Option<SlotOption> {
    let (kind, guid, quantity, min_quality, context) = match cost {
        Cost::Resource(line) => {
            (MaterialKind::Resource, line.resource, decimal(line.quantity_scu), line.min_quality, &line.context)
        }
        Cost::Item(line) => {
            (MaterialKind::Item, line.item, line.quantity.map(|q| q as f64), line.min_quality, &line.context)
        }
        _ => return None,
    };
    let material = guid.and_then(|guid| trees.get(guid)).and_then(|tree| match kind {
        MaterialKind::Resource => english(Some(tree.root()), "displayName", texts),
        MaterialKind::Item => item_name(&tree, texts),
    });
    Some(SlotOption {
        kind,
        material,
        material_guid: guid.map(|guid| guid.to_string()),
        quantity,
        min_quality,
        modifiers: modifiers(context, properties),
    })
}

/// A slot: a selection with its name, materials and modifiers, or a single
/// material line.
fn slot(cost: &Cost<'_>, trees: &mut Trees<'_, '_>, texts: &Localization, properties: &Properties) -> Option<Slot> {
    let Cost::Select(selection) = cost else {
        let option = option(cost, trees, texts, properties)?;
        return Some(Slot { name: String::new(), options: vec![option], modifiers: Vec::new() });
    };
    // A slot without a name of its own (`@LOC_PLACEHOLDER`) is named after
    // its debug name; scmdb shows the placeholder text there.
    let name = english_key(selection.slot_key, texts).or_else(|| selection.slot.map(title)).unwrap_or_default();
    let options =
        selection.options.iter().filter_map(|option| self::option(option, trees, texts, properties)).collect();
    Some(Slot { name, options, modifiers: modifiers(&selection.context, properties) })
}

fn tier(tier: &Tier<'_>, trees: &mut Trees<'_, '_>, texts: &Localization, properties: &Properties) -> RecipeTier {
    let mut slots = Vec::new();
    match &tier.mandatory {
        Some(Cost::Select(selection)) => {
            for option in &selection.options {
                slots.extend(slot(option, trees, texts, properties));
            }
        }
        Some(cost) => slots.extend(slot(cost, trees, texts, properties)),
        None => {}
    }
    RecipeTier { craft_time_seconds: tier.craft_time_seconds, slots }
}

/// The GUID a reference to a record that does not exist names: the
/// extractor leaves such a product out, the blueprint record keeps it as an
/// error at its place.
fn dangling_product(blueprint: &BlueprintTiers<'_>, trees: &mut Trees<'_, '_>) -> Option<Guid> {
    let tree = trees.once(blueprint.guid)?;
    let creation = structure(structure(Some(tree.root()), "blueprint"), "processSpecificData");
    match creation.and_then(|creation| creation.field("entityClass")) {
        Some(Value::Error(ValueError::DanglingReference { guid })) => Some(*guid),
        _ => None,
    }
}

/// The folder under the crafting blueprints names what a blueprint makes:
/// `fpsgear/weapons/pistol` is a pistol, `fpsgear/armour/combat/heavy` armour
/// of the role combat, `vehiclegear/weapons/laser/cannon` a laser weapon,
/// `vehiclegear/cooler` a cooler. Returns scmdb's (kind, subtype); a folder
/// of templates counts as the folder below it.
pub(crate) fn kind_of_path(record_path: &str) -> (String, String) {
    const ROOT: &str = "libs/foundry/records/crafting/blueprints/crafting/";
    let lower = record_path.to_ascii_lowercase().replace('\\', "/");
    let rest = lower.strip_prefix(ROOT).unwrap_or(&lower);
    let mut folders: Vec<&str> = rest.split('/').collect();
    folders.pop();
    folders.retain(|folder| *folder != "$templates");
    let at = |i: usize| folders.get(i).copied().unwrap_or_default().to_string();
    match folders.first().copied() {
        Some("fpsgear") => match folders.get(1).copied() {
            Some("ammo") => ("ammo".into(), at(2)),
            Some("armour") => ("armour".into(), at(2)),
            Some("weapons") => ("weapons".into(), at(2)),
            _ => (String::new(), String::new()),
        },
        Some("vehiclegear") => match folders.get(1).copied() {
            Some("weapons") => ("weapons".into(), at(2)),
            Some(kind) => (kind.to_string(), at(2)),
            None => (String::new(), String::new()),
        },
        _ => (String::new(), String::new()),
    }
}

/// The recipe of one blueprint of the crafting data.
pub(crate) fn read(
    blueprint: &BlueprintTiers<'_>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
    properties: &Properties,
) -> Recipe {
    let dangling = if blueprint.produced_item.is_none() { dangling_product(blueprint, trees) } else { None };
    let product_missing = dangling.is_some() || blueprint.produced_item.is_some_and(|guid| !trees.exists(guid));
    let (kind, subtype) = kind_of_path(blueprint.record_path);
    Recipe {
        kind,
        subtype,
        guid: blueprint.guid.to_string(),
        tag: blueprint.class_name.to_string(),
        product: blueprint.produced_item.or(dangling).map(|guid| guid.to_string()),
        product_missing,
        tiers: blueprint.tiers.iter().map(|value| tier(value, trees, texts, properties)).collect(),
        corrected: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::kind_of_path;

    fn kinds(path: &str) -> (String, String) {
        kind_of_path(&format!("libs/foundry/records/crafting/blueprints/crafting/{path}"))
    }

    fn pair(kind: &str, sub: &str) -> (String, String) {
        (kind.to_string(), sub.to_string())
    }

    #[test]
    fn the_folder_names_what_a_blueprint_makes() {
        assert_eq!(kinds("fpsgear/weapons/pistol/bp.xml"), pair("weapons", "pistol"));
        assert_eq!(kinds("fpsgear/armour/combat/heavy/bp.xml"), pair("armour", "combat"));
        assert_eq!(kinds("fpsgear/ammo/rifle/bp.xml"), pair("ammo", "rifle"));
        assert_eq!(kinds("vehiclegear/weapons/laser/cannon/bp.xml"), pair("weapons", "laser"));
        assert_eq!(kinds("vehiclegear/cooler/bp.xml"), pair("cooler", ""));
        assert_eq!(kinds("vehiclegear/$templates/cooler/bp.xml"), pair("cooler", ""));
        assert_eq!(kinds("something/else/bp.xml"), pair("", ""));
    }

    #[test]
    fn a_path_with_backslashes_gives_the_same_kind() {
        let path = r"Libs\Foundry\Records\Crafting\Blueprints\Crafting\fpsgear\weapons\pistol\bp.xml";
        assert_eq!(kind_of_path(path), pair("weapons", "pistol"));
    }
}
