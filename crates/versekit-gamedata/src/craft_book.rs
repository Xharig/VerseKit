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

//! Everything that can be crafted, as the crafting page lists it
//! (`crafting.all_items`, `with_collection`, `counts`, `recipe`,
//! `blueprints_with` of 4.12), built from the game values.
//!
//! One entry per item, not per blueprint: equal names with the same recipe
//! are one entry. Names that stay twice (the power plants of the Idris and
//! the Reclaimer, the BroadSpec in two sizes) are different items; they get
//! a distinguishing word from the tag and count as unclear when the
//! collection holds the name, because the collection knows only names.

use std::collections::{BTreeSet, HashMap, HashSet};

use versekit_core::blueprints::classify_recipe;
use versekit_core::paths::name_key;
use versekit_core::stock_data::{lower, py_split};
use versekit_core::stock_use::Ingredient;

use crate::values::{GameValues, Recipe};

/// The makers whose name the game data carries wrongly, by the code in the
/// tag (the data of MaxOx names Aegis Dynamics).
const MANUFACTURER_BY_CODE: [(&str, &str); 1] = [("MXOX", "MaxOx")];

/// The code of the maker in a blueprint tag (`BP_CRAFT_MXOX_…`), or empty.
pub fn manufacturer_code(tag: &str) -> String {
    let parts: Vec<&str> = tag.split('_').collect();
    if parts.len() > 2 && parts[0] == "BP" { parts[2].to_uppercase() } else { String::new() }
}

/// The name of the maker, corrected where the data names the wrong one.
pub fn manufacturer_name(name: &str, code: &str) -> String {
    let code = code.to_uppercase();
    MANUFACTURER_BY_CODE.iter().find(|(c, _)| *c == code).map_or_else(|| name.to_string(), |(_, n)| (*n).to_string())
}

/// The words of a tag that can name a maker: the third and the fourth part
/// (`BP_CRAFT_kap_…`, `BP_CRAFT_COOL_AEGS_…`), lower case, letters only.
fn maker_words(tag: &str) -> Vec<String> {
    tag.split('_')
        .skip(2)
        .take(2)
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 2 && w.chars().all(char::is_alphabetic))
        .collect()
}

/// A word counts as a maker only when nearly every recipe that holds it
/// names the same maker (`legacy` is in the tags of several).
const PURITY: f64 = 0.9;

/// A word with fewer recipes that name a maker proves nothing: one vote would
/// be pure by itself.
const MIN_VOTES: usize = 3;

/// Which maker a word of a tag stands for, learned from the recipes whose
/// product names its maker; the maker named most often wins.
fn makers_by_word(values: &GameValues) -> HashMap<String, String> {
    let mut votes: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for recipe in &values.recipes {
        let Some(maker) = values.product(recipe).and_then(|item| item.manufacturer.clone()).filter(|m| !m.is_empty())
        else {
            continue;
        };
        for word in maker_words(&recipe.tag) {
            *votes.entry(word).or_default().entry(maker.clone()).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .filter_map(|(word, makers)| {
            let total: usize = makers.values().sum();
            if total < MIN_VOTES {
                return None;
            }
            let best = makers.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;
            (best.1 as f64 >= PURITY * total as f64).then_some((word, best.0))
        })
        .collect()
}

/// A readable name when the product has none: `BP_CRAFT_RADR_GNRP_S03_Idris_TEMP`
/// becomes `RADR GNRP S03 Idris`.
pub fn name_from_tag(tag: &str) -> String {
    let raw = tag.replace("BP_CRAFT_", "").replace("_SCItem", "").replace("_TEMP", "").replace('_', " ");
    let raw = versekit_core::text::py_strip(&raw).to_string();
    if raw.is_empty() { "?".into() } else { raw }
}

/// What tells two items of one name apart: the words of the tag that the name
/// does not hold, the last two.
fn distinguisher(tag: &str, name: &str) -> String {
    let raw = name_from_tag(tag);
    let in_name: HashSet<String> = py_split(name).iter().map(|w| lower(w)).collect();
    let parts: Vec<&str> = py_split(&raw)
        .into_iter()
        .filter(|w| !in_name.contains(&lower(w)) && !matches!(w.to_uppercase().as_str(), "BP" | "CRAFT"))
        .collect();
    if parts.is_empty() { raw } else { parts[parts.len().saturating_sub(2)..].join(" ") }
}

/// One thing that can be crafted.
#[derive(Clone, Debug, PartialEq)]
pub struct CraftItem {
    /// The name of the product; what the collection and the recipe lookup
    /// compare.
    pub basis: String,
    /// The name as the list shows it: with a distinguishing word where the
    /// name stays twice.
    pub name: String,
    pub manufacturer: String,
    /// The kind the recipe folder names (`weapons`, `armour`, `cooler` …).
    pub kind: String,
    /// The subkind (`pistol`, the role of armour …).
    pub subtype: String,
    /// The kind of the product in the catalog (`Char_Armor_Helmet`).
    pub catalog_kind: String,
    pub tiers: usize,
    /// The tag of the first blueprint.
    pub tag: String,
    /// The tags of all blueprints the item stands for.
    pub tags: Vec<String>,
    /// The GUID of the product, lower case; empty without one.
    pub entity: String,
    /// The name the data gave as the product before it was corrected.
    pub raw_product: Option<String>,
    /// The index of the first blueprint in [`GameValues::recipes`].
    pub recipe: usize,
}

/// Whether one has the blueprint: yes, no, or not clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Have {
    Yes,
    No,
    /// The name belongs to several items and the collection knows only the name.
    Unclear,
}

/// The list of everything craftable.
#[derive(Clone, Debug, Default)]
pub struct CraftBook {
    pub items: Vec<CraftItem>,
    /// Product name and tag, lower case, to the index of the first blueprint.
    index: HashMap<String, usize>,
}

fn kind_of(recipe: &Recipe, cg: &str) -> String {
    if !recipe.kind.is_empty() {
        return recipe.kind.clone();
    }
    match cg {
        "Container" => "orepod".into(),
        "MiningModifier" => "miningmodule".into(),
        _ => String::new(),
    }
}

/// What tells recipes apart: (slot, material, amount as bits) of each ingredient.
type RecipeKey = Vec<(String, String, u64)>;

/// The key a recipe is told from another by: slot, material and amount of
/// every ingredient, sorted.
fn recipe_key(recipe: &Recipe) -> RecipeKey {
    let mut key: RecipeKey = recipe
        .tiers
        .iter()
        .flat_map(|tier| ingredients_of(&tier.slots))
        .map(|i| (i.slot, i.material, i.amount.to_bits()))
        .collect();
    key.sort();
    key
}

fn ingredients_of(slots: &[crate::values::Slot]) -> Vec<Ingredient> {
    let mut found = Vec::new();
    for slot in slots {
        for option in &slot.options {
            let material = option.material.clone().unwrap_or_default();
            if material.is_empty() {
                continue;
            }
            found.push(Ingredient {
                slot: slot.name.clone(),
                material,
                amount: option.quantity.unwrap_or(0.0),
                quality: option.min_quality.map_or(0.0, |q| q as f64),
            });
        }
    }
    found
}

/// The ingredients of every tier of a recipe, each as (slot, material, amount,
/// lowest quality).
pub fn ingredients(recipe: &Recipe) -> Vec<Vec<Ingredient>> {
    recipe.tiers.iter().map(|tier| ingredients_of(&tier.slots)).collect()
}

/// The materials of every recipe as the stock data wants them: (material,
/// counted in pieces, amount, lowest quality).
pub fn material_lines(values: &GameValues) -> Vec<(String, bool, f64, f64)> {
    let mut lines = Vec::new();
    for recipe in &values.recipes {
        for tier in &recipe.tiers {
            for slot in &tier.slots {
                for option in &slot.options {
                    let Some(material) = option.material.clone().filter(|m| !m.is_empty()) else { continue };
                    let piece = option.kind == crate::values::MaterialKind::Item;
                    lines.push((
                        material,
                        piece,
                        option.quantity.unwrap_or(0.0),
                        option.min_quality.map_or(0.0, |q| q as f64),
                    ));
                }
            }
        }
    }
    lines
}

impl CraftBook {
    /// The book of the recipes in `values`.
    pub fn build(values: &GameValues) -> CraftBook {
        let named = |recipe: &Recipe| -> String {
            values
                .product(recipe)
                .and_then(|item| item.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| name_from_tag(&recipe.tag))
        };
        // Groups by the comparable name, in the order the names first appear.
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, recipe) in values.recipes.iter().enumerate() {
            let key = name_key(&named(recipe));
            if !groups.contains_key(&key) {
                order.push(key.clone());
            }
            groups.entry(key).or_default().push(i);
        }
        let makers = makers_by_word(values);
        let mut items = Vec::new();
        for key in &order {
            // Equal recipes are one part, different ones stay apart.
            let mut parts: Vec<(RecipeKey, Vec<usize>)> = Vec::new();
            for &i in &groups[key] {
                let recipe_key = recipe_key(&values.recipes[i]);
                match parts.iter_mut().find(|(k, _)| *k == recipe_key) {
                    Some(part) => part.1.push(i),
                    None => parts.push((recipe_key, vec![i])),
                }
            }
            let several = parts.len() > 1;
            for (_, members) in &parts {
                let first = members[0];
                let recipe = &values.recipes[first];
                let product = values.product(recipe);
                let basis = named(recipe);
                let name =
                    if several { format!("{basis} ({})", distinguisher(&recipe.tag, &basis)) } else { basis.clone() };
                let catalog_kind = product.and_then(|item| item.cg_item_type.clone()).unwrap_or_default();
                // A product without a maker takes the one its tag names.
                let maker = product
                    .and_then(|item| item.manufacturer.clone())
                    .filter(|maker| !maker.is_empty())
                    .or_else(|| maker_words(&recipe.tag).iter().find_map(|word| makers.get(word).cloned()))
                    .unwrap_or_default();
                let raw_product = recipe
                    .corrected
                    .iter()
                    .find(|c| c.field == "product")
                    .and_then(|c| values.items.get(&c.raw))
                    .and_then(|item| item.name.clone());
                items.push(CraftItem {
                    basis,
                    name,
                    manufacturer: manufacturer_name(&maker, &manufacturer_code(&recipe.tag)),
                    kind: kind_of(recipe, &catalog_kind),
                    subtype: recipe.subtype.clone(),
                    catalog_kind,
                    tiers: recipe.tiers.len(),
                    tag: recipe.tag.clone(),
                    tags: members.iter().map(|&m| values.recipes[m].tag.clone()).collect(),
                    entity: recipe.product.clone().unwrap_or_default(),
                    raw_product,
                    recipe: first,
                });
            }
        }
        items.sort_by_cached_key(|item| lower(&item.name));
        let mut index = HashMap::new();
        for (i, recipe) in values.recipes.iter().enumerate() {
            let name = values.product(recipe).and_then(|item| item.name.clone()).unwrap_or_default();
            index.entry(lower(&name)).or_insert(i);
            index.entry(lower(&recipe.tag)).or_insert(i);
        }
        // The name as the list shows it (`Name (Idris)`) leads to its own recipe.
        for item in &items {
            index.entry(lower(&item.name)).or_insert(item.recipe);
        }
        CraftBook { items, index }
    }

    /// The recipe of an item (its first blueprint).
    pub fn recipe_of<'a>(&self, values: &'a GameValues, item: &CraftItem) -> Option<&'a Recipe> {
        values.recipes.get(item.recipe)
    }

    /// The first recipe with this product name or tag (`crafting.recipe`).
    pub fn find<'a>(&self, values: &'a GameValues, name_or_tag: &str) -> Option<&'a Recipe> {
        let wanted = lower(versekit_core::text::py_strip(name_or_tag));
        self.index.get(&wanted).and_then(|&i| values.recipes.get(i))
    }

    /// Per item whether the blueprint is held (`with_collection`); `have`
    /// are the comparable names of the collection.
    pub fn with_collection(&self, have: &HashSet<String>) -> Vec<Have> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        for item in &self.items {
            let key = name_key(&item.basis);
            if !seen.insert(key.clone()) {
                ambiguous.insert(key);
            }
        }
        self.items
            .iter()
            .map(|item| {
                let key = name_key(&item.basis);
                let owned = have.contains(&key);
                match (owned, ambiguous.contains(&key)) {
                    (true, true) => Have::Unclear,
                    (true, false) => Have::Yes,
                    _ => Have::No,
                }
            })
            .collect()
    }

    /// (sure, all, unclear) for the line above the list (`counts`): the
    /// unclear are counted by name, not by entry.
    pub fn counts(&self, have: &HashSet<String>) -> (usize, usize, usize) {
        let marks = self.with_collection(have);
        let sure = marks.iter().filter(|m| **m == Have::Yes).count();
        let unclear: HashSet<String> = self
            .items
            .iter()
            .zip(&marks)
            .filter(|(_, m)| **m == Have::Unclear)
            .map(|(item, _)| name_key(&item.basis))
            .collect();
        (sure, self.items.len(), unclear.len())
    }

    /// The category and subcategory of an item, as the list filters by them.
    pub fn category(&self, item: &CraftItem) -> (String, &'static str) {
        classify_recipe(&item.catalog_kind, &item.tag, &item.subtype, &item.kind)
    }
}

/// The names of the products whose recipe holds `material`, sorted without
/// case (`crafting.blueprints_with`). Items without a product name are left
/// out, as in 4.12.
pub fn blueprints_with(values: &GameValues, material: &str) -> Vec<String> {
    let wanted = versekit_core::stock_data::norm_material(material);
    let mut names: BTreeSet<String> = BTreeSet::new();
    for recipe in &values.recipes {
        let Some(name) = values.product(recipe).and_then(|item| item.name.clone()).filter(|n| !n.is_empty()) else {
            continue;
        };
        let uses = recipe.tiers.iter().flat_map(|t| &t.slots).flat_map(|s| &s.options).any(|o| {
            o.material
                .as_deref()
                .is_some_and(|m| !m.is_empty() && versekit_core::stock_data::norm_material(m) == wanted)
        });
        if uses {
            names.insert(name);
        }
    }
    let mut sorted: Vec<String> = names.into_iter().collect();
    sorted.sort_by_cached_key(|n| lower(n));
    sorted
}

/// The share of the material that comes back when taking apart, and the
/// materials that do not (`crafting.dismantle_block`).
pub fn dismantle_lost(values: &GameValues) -> (BTreeSet<String>, f64) {
    let mut names: BTreeSet<String> = values.dismantle.resources.iter().cloned().collect();
    names.extend(values.dismantle.entities.iter().cloned());
    (names, values.dismantle.efficiency)
}

/// The rules of the fabricator as the page "Dismantle & Salvage" reads them (`salvage.dismantle_rules`): the
/// share that comes back (half where the data say none), the seconds and the materials that never come back,
/// in lower case; a blocked item also without its `(…)`, since "Saldynium (Ore)" is the ore "Saldynium".
pub fn dismantle_rules(values: &GameValues) -> (f64, i64, BTreeSet<String>) {
    let clean = |name: &str| lower(versekit_core::text::py_strip(name));
    let mut blocked: BTreeSet<String> =
        values.dismantle.resources.iter().filter(|n| !n.is_empty()).map(|n| clean(n)).collect();
    for name in values.dismantle.entities.iter().filter(|n| !n.is_empty()) {
        blocked.insert(clean(name));
        let short = clean(name.split('(').next().unwrap_or_default());
        if !short.is_empty() {
            blocked.insert(short);
        }
    }
    let share = if values.dismantle.efficiency == 0.0 { 0.5 } else { values.dismantle.efficiency };
    (share, values.dismantle.seconds, blocked)
}

/// One raw material of an item taken apart: how much is in it, how much comes back, whether it is lost, and
/// whether it is counted in pieces (an item) rather than in SCU.
#[derive(Clone, Debug, PartialEq)]
pub struct Taken {
    pub material: String,
    pub inside: f64,
    pub back: f64,
    pub lost: bool,
    pub piece: bool,
}

/// What taking this item apart gives (`salvage.dismantle`): every raw material of every tier added up, by
/// name; a blocked one comes back not at all, the others with the share of the fabricator.
pub fn dismantle(values: &GameValues, recipe: &Recipe) -> Vec<Taken> {
    let (share, _, blocked) = dismantle_rules(values);
    let mut demand: Vec<(String, f64)> = Vec::new();
    for tier in ingredients(recipe) {
        for ingredient in tier {
            let name = versekit_core::text::py_strip(&ingredient.material).to_string();
            if name.is_empty() {
                continue;
            }
            match demand.iter_mut().find(|(n, _)| *n == name) {
                Some((_, amount)) => *amount += ingredient.amount,
                None => demand.push((name, ingredient.amount)),
            }
        }
    }
    demand.sort_by_cached_key(|(name, _)| lower(name));
    let pieces: BTreeSet<String> = recipe
        .tiers
        .iter()
        .flat_map(|tier| &tier.slots)
        .flat_map(|slot| &slot.options)
        .filter(|option| option.kind == crate::values::MaterialKind::Item)
        .filter_map(|option| option.material.as_deref().map(|m| versekit_core::text::py_strip(m).to_string()))
        .collect();
    demand
        .into_iter()
        .map(|(material, inside)| {
            let lost = blocked.contains(&lower(&material));
            let piece = pieces.contains(&material);
            Taken { back: if lost { 0.0 } else { inside * share }, material, inside, lost, piece }
        })
        .collect()
}

/// How many raw materials never come back, each counted once: "Saldynium (Ore)" and "saldynium" are one.
pub fn blocked_count(blocked: &BTreeSet<String>) -> usize {
    blocked
        .iter()
        .map(|name| versekit_core::text::py_strip(name.split('(').next().unwrap_or_default()).to_string())
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{ItemValues, MaterialKind, RecipeTier, Slot, SlotOption};

    fn recipe(tag: &str, product: &str, material: &str, amount: f64) -> Recipe {
        Recipe {
            guid: tag.to_lowercase(),
            tag: tag.into(),
            product: Some(product.into()),
            tiers: vec![RecipeTier {
                craft_time_seconds: Some(60),
                slots: vec![Slot {
                    name: "Frame".into(),
                    options: vec![SlotOption {
                        kind: MaterialKind::Resource,
                        material: Some(material.into()),
                        quantity: Some(amount),
                        min_quality: Some(0),
                        ..SlotOption::default()
                    }],
                    modifiers: Vec::new(),
                }],
            }],
            ..Recipe::default()
        }
    }

    #[test]
    fn taking_apart_gives_the_share_back_and_nothing_of_a_blocked_material() {
        let mut values = GameValues {
            dismantle: crate::values::Dismantle {
                efficiency: 0.5,
                seconds: 15,
                resources: vec![" Quantainium ".into()],
                entities: vec!["Saldynium (Ore)".into()],
            },
            ..GameValues::default()
        };
        let (share, seconds, blocked) = dismantle_rules(&values);
        assert_eq!((share, seconds), (0.5, 15));
        assert_eq!(blocked_count(&blocked), 2, "long and short form are one material");
        let blocked: Vec<&str> = blocked.iter().map(String::as_str).collect();
        assert_eq!(blocked, ["quantainium", "saldynium", "saldynium (ore)"]);
        // Two tiers: the same material adds up over both; the order is by name.
        let mut item = recipe("BP_X", "x", "Iron", 3.0);
        let mut second = item.tiers[0].clone();
        second.slots[0].options[0].material = Some("Quantainium".into());
        second.slots[0].options[0].quantity = Some(0.25);
        second.slots.push(second.slots[0].clone());
        second.slots[1].options[0].material = Some("iron".into());
        item.tiers.push(second);
        item.tiers[0].slots[0].options.push(SlotOption {
            material: Some("Iron".into()),
            quantity: Some(1.5),
            ..SlotOption::default()
        });
        item.tiers[0].slots[0].options.push(SlotOption {
            kind: MaterialKind::Item,
            material: Some("Gem".into()),
            quantity: Some(2.0),
            ..SlotOption::default()
        });
        let taken = dismantle(&values, &item);
        let shown: Vec<(&str, f64, f64, bool, bool)> =
            taken.iter().map(|t| (t.material.as_str(), t.inside, t.back, t.lost, t.piece)).collect();
        assert_eq!(
            shown,
            [
                ("Gem", 2.0, 1.0, false, true),
                ("Iron", 4.5, 2.25, false, false),
                ("iron", 0.25, 0.125, false, false),
                ("Quantainium", 0.25, 0.0, true, false)
            ]
        );
        // Without a share in the data half comes back.
        values.dismantle.efficiency = 0.0;
        assert_eq!(dismantle_rules(&values).0, 0.5);
    }

    fn values() -> GameValues {
        let mut values = GameValues::default();
        for (guid, name) in [("a", "Main Powerplant"), ("b", "Main Powerplant"), ("c", "Helm"), ("d", "Helm")] {
            values.items.insert(
                guid.into(),
                ItemValues {
                    guid: guid.into(),
                    name: Some(name.into()),
                    manufacturer: Some("Maker".into()),
                    ..ItemValues::default()
                },
            );
        }
        values.recipes = vec![
            recipe("BP_CRAFT_POWR_AEGS_S04_Idris_SCItem", "a", "Iron", 1.0),
            recipe("BP_CRAFT_POWR_AEGS_S04_Reclaimer_SCItem", "b", "Iron", 2.0),
            recipe("BP_CRAFT_helm_01_01_13", "c", "Iron", 3.0),
            recipe("BP_CRAFT_helm_01_01_15", "d", "Iron", 3.0),
        ];
        values
    }

    #[test]
    fn equal_names_with_equal_recipes_are_one_entry_and_different_recipes_stay_apart() {
        let book = CraftBook::build(&values());
        let names: Vec<&str> = book.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Helm", "Main Powerplant (S04 Idris)", "Main Powerplant (S04 Reclaimer)"]);
        assert_eq!(book.items[0].tags.len(), 2);
    }

    #[test]
    fn a_held_name_that_means_two_items_is_unclear_and_counted_once() {
        let book = CraftBook::build(&values());
        let have: HashSet<String> = ["main powerplant".to_string(), "helm".to_string()].into();
        let marks = book.with_collection(&have);
        assert_eq!(marks, [Have::Yes, Have::Unclear, Have::Unclear]);
        assert_eq!(book.counts(&have), (1, 3, 1));
    }

    #[test]
    fn a_product_without_name_is_named_after_its_tag() {
        assert_eq!(name_from_tag("BP_CRAFT_RADR_GNRP_S03_Idris_TEMP"), "RADR GNRP S03 Idris");
        assert_eq!(name_from_tag("BP_CRAFT_"), "?");
    }

    #[test]
    fn the_maker_of_maxox_is_corrected_by_the_code_in_the_tag() {
        assert_eq!(manufacturer_code("BP_CRAFT_MXOX_NeutronCannon_S1"), "MXOX");
        assert_eq!(manufacturer_name("Aegis Dynamics", "MXOX"), "MaxOx");
        assert_eq!(manufacturer_name("Behring", "BEHR"), "Behring");
    }

    #[test]
    fn the_materials_a_recipe_uses_are_found_by_comparable_name() {
        let found = blueprints_with(&values(), "iron");
        assert_eq!(found, ["Helm", "Main Powerplant"]);
        assert!(blueprints_with(&values(), "Gold").is_empty());
    }
}
