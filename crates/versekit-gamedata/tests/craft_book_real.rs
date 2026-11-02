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

//! The craft book of a real build: what can be crafted, how it is sorted and
//! the table of product values of the M5A Cannon.
//!
//! Run with `cargo test -p versekit-gamedata --release --test craft_book_real --
//! --ignored --nocapture` and `VERSEKIT_DATAFORGE` and `VERSEKIT_GLOBAL_INI`
//! (see `values_real`) or `VERSEKIT_CHANNEL_FOLDER`.

use std::collections::HashSet;

use versekit_gamedata::craft_book::{CraftBook, Have};
use versekit_gamedata::product_table::{self, Line};
use versekit_gamedata::values::GameValues;
use versekit_gamedata::{GameData, Language, Localization, quality, values_from_dataforge};

fn values() -> GameValues {
    if let (Some(dcb), Some(ini)) = (std::env::var_os("VERSEKIT_DATAFORGE"), std::env::var_os("VERSEKIT_GLOBAL_INI")) {
        let texts = Localization::from_files(Language::English, &std::fs::read(ini).unwrap(), None).unwrap();
        return values_from_dataforge(&std::fs::read(dcb).unwrap(), &texts, None).unwrap();
    }
    let game = match std::env::var_os("VERSEKIT_CHANNEL_FOLDER") {
        Some(folder) => GameData::open(folder).unwrap(),
        None => GameData::open_installed().unwrap(),
    };
    let texts = game.localization(Language::English).unwrap();
    game.values(&texts).unwrap()
}

#[test]
#[ignore = "reads the game data"]
fn the_book_lists_what_can_be_crafted_and_sorts_it() {
    let values = values();
    let book = CraftBook::build(&values);
    println!("{} items for {} recipes", book.items.len(), values.recipes.len());
    assert!(book.items.len() > 1500 && book.items.len() <= values.recipes.len());
    // Every item is sorted into a category of the filter; nothing is left without one.
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for item in &book.items {
        *kinds.entry(book.category(item).0).or_default() += 1;
    }
    println!("{kinds:?}");
    assert!(kinds.get("ruestung").copied().unwrap_or(0) > 800);
    assert!(kinds.get("schiffswaffe").copied().unwrap_or(0) > 80);
    assert!(kinds.get("fpswaffe").copied().unwrap_or(0) > 150);
    // Holding the names of half of them: nothing is unclear except the shared names.
    let have: HashSet<String> =
        book.items.iter().step_by(2).map(|i| versekit_core::paths::name_key(&i.basis)).collect();
    let marks = book.with_collection(&have);
    let (sure, all, unclear) = book.counts(&have);
    println!("{sure} sure of {all}, {unclear} unclear");
    assert_eq!(all, book.items.len());
    assert_eq!(sure, marks.iter().filter(|m| **m == Have::Yes).count());
}

/// The kind and subkind of a recipe from the folder of its blueprint are the
/// ones scmdb gives for the same build: the folders of the data and the
/// sorting of scmdb name the same things.
#[test]
#[ignore = "reads the game data and the recipe file of 4.12"]
fn the_kind_from_the_folder_is_the_one_scmdb_gives() {
    let Some(file) = std::env::var_os("VERSEKIT_RECIPES") else {
        panic!("VERSEKIT_RECIPES names crafting-blueprints.json")
    };
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let mut scmdb = std::collections::BTreeMap::<String, (String, String)>::new();
    for blueprint in kept["blueprints"].as_array().unwrap() {
        scmdb.insert(
            blueprint["tag"].as_str().unwrap().to_ascii_lowercase(),
            (
                blueprint["type"].as_str().unwrap_or_default().to_string(),
                blueprint["subtype"].as_str().unwrap_or_default().to_string(),
            ),
        );
    }
    let values = values();
    let (mut compared, mut missing) = (0, 0);
    let mut kinds_differ = Vec::new();
    // What counts is the category the page sorts a recipe into: with the kind and subkind of the
    // folder and with those of scmdb it must be the same one. (scmdb gives a size as subkind where
    // the folders name none, and a few weapons stand in another folder than their kind; the tag
    // decides there.)
    let mut sorted_differently = Vec::new();
    for recipe in &values.recipes {
        let Some((kind, sub)) = scmdb.get(&recipe.tag.to_ascii_lowercase()) else {
            missing += 1;
            continue;
        };
        compared += 1;
        // A folder that names no kind (ore pods and mining modules) leaves it empty; the sorting
        // takes the kind of the product then. A kind the folder names must be scmdb's.
        if !recipe.kind.is_empty() && *kind != recipe.kind {
            kinds_differ.push(format!("{}: scmdb {kind} against {}", recipe.tag, recipe.kind));
        }
        let item_kind = values.product(recipe).and_then(|i| i.cg_item_type.clone()).unwrap_or_default();
        let own = versekit_core::blueprints::classify_recipe(&item_kind, &recipe.tag, &recipe.subtype, &recipe.kind);
        let theirs = versekit_core::blueprints::classify_recipe(&item_kind, &recipe.tag, sub, kind);
        if own != theirs {
            sorted_differently.push(format!("{}: {own:?} against {theirs:?}", recipe.tag));
        }
    }
    println!(
        "{compared} compared, {missing} not in scmdb, kinds differ {}, sorted differently {}",
        kinds_differ.len(),
        sorted_differently.len()
    );
    assert!(compared > 1500, "{compared} recipes compared");
    assert!(kinds_differ.is_empty(), "{kinds_differ:?}");
    assert!(sorted_differently.is_empty(), "{sorted_differently:?}");
}

/// What the own reading lacks comes from scmdb's values of the same build
/// (here the file 4.12 kept, `VERSEKIT_RECIPES`): then a cooler, a power
/// plant and a piece of armour have a table as well.
#[test]
#[ignore = "reads the game data and the recipe file of 4.12"]
fn scmdb_fills_the_tables_the_own_reading_leaves_empty() {
    let Some(file) = std::env::var_os("VERSEKIT_RECIPES") else {
        panic!("VERSEKIT_RECIPES names crafting-blueprints.json")
    };
    let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let theirs = versekit_gamedata::scmdb::values(kept["build"].as_str(), &kept, &kept["products"]);
    let mut own = values();
    let book = CraftBook::build(&own);
    let rows_of = |values: &GameValues, kind: &str| -> usize {
        let item = book.items.iter().find(|i| i.catalog_kind == kind).unwrap();
        let recipe = book.recipe_of(values, item).unwrap();
        let product = values.product(recipe).unwrap();
        let factors = quality::combined(recipe, |_, _| Some(500.0));
        product_table::table(product, &factors).iter().filter(|l| matches!(l, Line::Row(_))).count()
    };
    let before: Vec<usize> =
        ["Cooler", "PowerPlant", "Char_Armor_Helmet", "WeaponMining"].iter().map(|k| rows_of(&own, k)).collect();
    versekit_gamedata::value_source::fill_gaps(&mut own, &theirs);
    let after: Vec<usize> =
        ["Cooler", "PowerPlant", "Char_Armor_Helmet", "WeaponMining"].iter().map(|k| rows_of(&own, k)).collect();
    println!("rows before {before:?}, after {after:?}");
    assert!(after.iter().all(|n| *n > 0), "{after:?}");
    assert!(after.iter().zip(&before).all(|(a, b)| a >= b));
}

/// The values read last as the crafting page keeps them (`werte-spieldaten.json`, named by
/// `VERSEKIT_VALUES_CACHE`), or else those of the game.
fn kept_or_read() -> GameValues {
    match std::env::var_os("VERSEKIT_VALUES_CACHE") {
        Some(file) => {
            let kept: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            serde_json::from_value(kept["values"].clone()).unwrap()
        }
        None => values(),
    }
}

/// Every craftable item stands in exactly one category of the crafting terminal; the ones that
/// stand in none or in two are named.
#[test]
#[ignore = "reads the game data or the values kept by the crafting page"]
fn every_craftable_item_lands_in_exactly_one_category_of_the_terminal() {
    let values = kept_or_read();
    let book = CraftBook::build(&values);
    assert!(book.items.len() > 1500, "{} items", book.items.len());
    let mut wrong = Vec::new();
    let mut per_place = std::collections::BTreeMap::<String, usize>::new();
    for item in &book.items {
        let found = versekit_core::fabricator::places(&item.catalog_kind, &item.kind, &item.subtype);
        if found.len() != 1 {
            wrong.push(format!(
                "{} ({}/{}/{}): {} places {found:?}",
                item.name,
                item.catalog_kind,
                item.kind,
                item.subtype,
                found.len()
            ));
            continue;
        }
        let place = found[0];
        let key = versekit_core::fabricator::CATEGORIES[place.category].key;
        *per_place.entry(format!("{key}/{}", place.sub.unwrap_or("-"))).or_default() += 1;
    }
    println!("{} items: {per_place:?}", book.items.len());
    assert!(wrong.is_empty(), "{} items without exactly one place:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
#[ignore = "reads the game data"]
fn the_table_of_the_m5a_cannon_shows_damage_and_dps_that_quality_changes() {
    let values = values();
    let book = CraftBook::build(&values);
    let item = book.items.iter().find(|i| i.name == "M5A Cannon").expect("the M5A Cannon");
    let recipe = book.recipe_of(&values, item).unwrap();
    let product = values.product(recipe).unwrap();
    let rows = |q: f64| -> Vec<(String, String, String)> {
        let factors = quality::combined(recipe, |_, _| Some(q));
        product_table::table(product, &factors)
            .into_iter()
            .filter_map(|line| match line {
                Line::Row(row) => {
                    let shown = product_table::formatted(&row);
                    Some((row.label, shown.base, shown.made))
                }
                _ => None,
            })
            .collect()
    };
    let base = rows(500.0);
    let best = rows(1000.0);
    println!("{base:?}\n{best:?}");
    assert!(base.iter().any(|(label, _, _)| label == "DPS"), "{base:?}");
    // At the neutral quality nothing changes; at the best one DPS and health rise.
    assert!(base.iter().all(|(_, _, made)| made == "—"), "{base:?}");
    let dps = best.iter().find(|(label, _, _)| label == "DPS").unwrap();
    assert_ne!(dps.2, "—");
}
