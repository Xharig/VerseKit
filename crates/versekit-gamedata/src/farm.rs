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

//! What is still to be farmed: the material for all items that are to be
//! crafted, against the own stock (`cart.farm_list` of 4.12), and where to
//! find it (`pages.farm_locations`).
//!
//! ⚠ Everything is added up across all items, not item by item: two items
//! that take 2 Iron each against 3 Iron in the stock are 4 against 3, and one
//! is missing. The lowest quality of a recipe counts: the stock is handed out
//! from the most demanding requirement downwards, each portion from one crate
//! that holds it whole (crates are not put together); pieces are counted
//! over all entries. An item whose recipe cannot be read is named, never
//! counted as needing nothing.
//!
//! ⚠ The planned items are the whole bill (`cart.invoice`): the parts on
//! "build" of the ships in the hangar and on the wish list, then the farm
//! notes ([`planned_posts`]). A ship whose slots are unknown brings no parts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use versekit_core::cart;
use versekit_core::jsonio::{Map, Value};
use versekit_core::mining_data::{MiningData, Ore};
use versekit_core::notepad::Note;
use versekit_core::raw_prices::RawPrices;
use versekit_core::ship_market::Market;
use versekit_core::shops::Shops;
use versekit_core::stock_data::{StockData, lower, norm_material};
use versekit_core::stock_use::{amount_of, pick_crate, quality_of};
use versekit_core::text::py_sum;

use crate::cart_data::{CartData, SlotProvider};
use crate::craft_book::{self, CraftBook};
use crate::values::{GameValues, Recipe};

/// One item that is to be crafted.
#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub name: String,
    /// The identifier of the recipe data, empty where there is none.
    pub reference: String,
    /// How often it is to be crafted.
    pub pieces: i64,
}

/// What one material at one lowest quality comes to.
#[derive(Clone, Debug, PartialEq)]
pub struct Need {
    pub material: String,
    pub needed: f64,
    /// The part of the stock handed out to this need.
    pub have: f64,
    pub missing: f64,
    pub min_quality: f64,
    /// What is in the stock but below the quality: a hint, not stock.
    pub too_low: f64,
}

/// The result of [`farm_list`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FarmList {
    /// What is missing, the largest gap first.
    pub missing: Vec<Need>,
    /// What the stock covers, alphabetical.
    pub complete: Vec<Need>,
    /// How many pieces are to be crafted.
    pub posts: i64,
    /// Items whose recipe could not be read.
    pub without_recipe: Vec<String>,
}

/// The recipe of a post: by the identifier of its product where it has one,
/// else by its name; a recipe without tiers is none.
pub fn recipe_of<'a>(values: &'a GameValues, book: &CraftBook, post: &Post) -> Option<&'a Recipe> {
    let blueprint = blueprint_of(book, &post.reference, &post.name);
    if blueprint.is_empty() {
        return None;
    }
    book.find(values, blueprint).filter(|recipe| !recipe.tiers.is_empty())
}

/// The name of the blueprint that makes this identifier (`cart._blueprint_index`), else `name`.
pub fn blueprint_of<'a>(book: &'a CraftBook, reference: &str, name: &'a str) -> &'a str {
    let reference = reference.to_lowercase();
    book.items
        .iter()
        .find(|item| !item.entity.is_empty() && item.entity == reference)
        .map_or(name, |item| if item.basis.is_empty() { name } else { item.basis.as_str() })
}

/// The posts of the notes: the ones that are to be crafted and not ticked
/// off, of which the recipe is known (a note without one counts as bought, as
/// in the shopping list of 4.12: the way falls back to buying).
pub fn posts_of(values: &GameValues, book: &CraftBook, notes: &[Note]) -> Vec<Post> {
    notes
        .iter()
        .filter(|n| matches!(n.way.as_str(), "" | "bauen") && !n.done)
        .map(|n| Post { name: n.name.clone(), reference: n.reference.clone(), pieces: n.count })
        .filter(|post| recipe_of(values, book, post).is_some())
        .collect()
}

/// The posts of the whole plan in `data` (the fleet file), in the order of the
/// bill (`cart.farm_list` takes them from `cart.invoice`): every part on
/// "build" that is not ticked off, of the hangar, the wish list and the
/// notes. The way of a part without a recipe falls back to buying there, so
/// only parts with a recipe come. `slots` gives the slots of a ship; without
/// it no ship brings parts.
pub fn planned_posts(data: &Map, values: &GameValues, book: &CraftBook, slots: Option<&SlotProvider<'_>>) -> Vec<Post> {
    posts_of_plan(&plan_lines(data, values, book, slots))
}

/// Where a planned part comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanSource {
    /// The farm notes: the player plans the part by hand.
    Notepad,
    /// A slot of a ship in the hangar.
    Hangar,
    /// A slot of a ship on the wish list.
    Wishlist,
}

/// One part of the plan: what [`planned_posts`] counts, with where it comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanLine {
    pub name: String,
    /// The identifier of the part; empty for a note.
    pub reference: String,
    /// How many, as the farm list counts them: at least one and at most 9999.
    pub count: i64,
    pub source: PlanSource,
    /// The ship the part belongs to; empty for a note.
    pub ship: String,
    /// Where on the ship (`Cooler S2`); empty for a note.
    pub position: String,
}

/// The whole plan (`cart.invoice`): every part on "build" that is not ticked off and has a recipe, of the notes, the
/// hangar and the wish list, in the order of the bill. The posts of the farm list are made of exactly these lines.
pub fn plan_lines(
    data: &Map,
    values: &GameValues,
    book: &CraftBook,
    slots: Option<&SlotProvider<'_>>,
) -> Vec<PlanLine> {
    // Prices play no part in the way: the stores of prices stay empty.
    let (shops, raw, market) = (Shops::default(), RawPrices::default(), Market::default());
    let sources = CartData { values, book, shops: &shops, raw: &raw, market: &market, slots };
    cart::invoice(data, &sources)
        .lines
        .iter()
        .filter(|l| l.way == cart::CRAFT && !l.done)
        .map(|l| PlanLine {
            name: l.name.clone(),
            reference: l.reference.clone(),
            count: l.count.clamp(1, 9999),
            source: match l.origin {
                cart::Origin::Hangar => PlanSource::Hangar,
                cart::Origin::Wishlist => PlanSource::Wishlist,
                cart::Origin::Notepad | cart::Origin::Basket => PlanSource::Notepad,
            },
            ship: l.ship.clone(),
            position: l.position.clone(),
        })
        .collect()
}

/// The posts of the farm list for the lines of the plan.
pub fn posts_of_plan(lines: &[PlanLine]) -> Vec<Post> {
    lines
        .iter()
        .map(|l| Post { name: l.name.clone(), reference: l.reference.clone(), pieces: l.count })
        .collect()
}

/// How often one item of the craft book is planned: by the notes and by the ships (hangar and wish list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanCount {
    pub notes: i64,
    pub ships: i64,
}

impl PlanCount {
    pub fn total(&self) -> i64 {
        self.notes + self.ships
    }
}

/// The count of every item of the book (by its index) that the lines plan. A line finds its item by the identifier
/// of the part, else by the name the list shows, else by the name of the product.
pub fn plan_counts(book: &CraftBook, lines: &[PlanLine]) -> HashMap<usize, PlanCount> {
    let mut by_entity: HashMap<&str, usize> = HashMap::new();
    let (mut by_name, mut by_basis): (HashMap<String, usize>, HashMap<String, usize>) = (HashMap::new(), HashMap::new());
    for (at, item) in book.items.iter().enumerate() {
        if !item.entity.is_empty() {
            by_entity.entry(item.entity.as_str()).or_insert(at);
        }
        by_name.entry(lower(&item.name)).or_insert(at);
        by_basis.entry(lower(&item.basis)).or_insert(at);
    }
    let mut counts: HashMap<usize, PlanCount> = HashMap::new();
    for line in lines {
        let found = (!line.reference.is_empty())
            .then(|| by_entity.get(line.reference.to_lowercase().as_str()).copied())
            .flatten()
            .or_else(|| by_name.get(&lower(&line.name)).copied())
            .or_else(|| by_basis.get(&lower(&line.name)).copied());
        let Some(at) = found else { continue };
        let entry = counts.entry(at).or_default();
        match line.source {
            PlanSource::Notepad => entry.notes += line.count,
            PlanSource::Hangar | PlanSource::Wishlist => entry.ships += line.count,
        }
    }
    counts
}

/// The material for all `posts` against the stock.
pub fn farm_list(values: &GameValues, book: &CraftBook, posts: &[Post], stock: &[Value], data: &StockData) -> FarmList {
    struct Group {
        name: String,
        amount: f64,
        portions: Vec<f64>,
    }
    // (comparable name, quality as bits) in the order they appear.
    let mut order: Vec<(String, u64)> = Vec::new();
    let mut needed: HashMap<(String, u64), Group> = HashMap::new();
    let mut without_recipe = Vec::new();
    let mut crafted = 0;
    for post in posts {
        // A count from a hand-edited file is capped like the field of the crafting page: one portion per piece is kept.
        let pieces = post.pieces.clamp(1, 9999);
        crafted += pieces;
        let Some(recipe) = recipe_of(values, book, post) else {
            without_recipe.push(post.name.clone());
            continue;
        };
        for tier in craft_book::ingredients(recipe) {
            for ingredient in tier {
                let key = (norm_material(&ingredient.material), ingredient.quality.to_bits());
                let group = needed.entry(key.clone()).or_insert_with(|| {
                    order.push(key);
                    Group { name: ingredient.material.clone(), amount: 0.0, portions: Vec::new() }
                });
                group.amount += ingredient.amount * pieces as f64;
                group.portions.extend(std::iter::repeat_n(ingredient.amount, pieces as usize));
            }
        }
    }
    // By material, the groups of one material in the order they appeared.
    let mut by_material: Vec<(String, Vec<(f64, Group)>)> = Vec::new();
    for key in order {
        let group = needed.remove(&key).expect("every key of the order has a group");
        let grade = f64::from_bits(key.1);
        match by_material.iter_mut().find(|(norm, _)| *norm == key.0) {
            Some((_, groups)) => groups.push((grade, group)),
            None => by_material.push((key.0, vec![(grade, group)])),
        }
    }
    let (mut missing, mut complete) = (Vec::new(), Vec::new());
    for (norm, mut groups) in by_material {
        // The most demanding first: it takes the ore that is just enough.
        groups.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut mine: Vec<Value> = stock
            .iter()
            .filter(|p| norm_material(p.get("material").and_then(Value::as_str).unwrap_or("")) == norm)
            .cloned()
            .collect();
        let pieces_ware = data.is_piece(&groups[0].1.name);
        for (grade, group) in groups {
            let mut taken = 0.0;
            if pieces_ware {
                let mut usable: Vec<usize> =
                    (0..mine.len()).filter(|&i| quality_of(&mine[i]) >= grade && amount_of(&mine[i]) > 0.0).collect();
                usable.sort_by(|&a, &b| quality_of(&mine[a]).total_cmp(&quality_of(&mine[b])));
                for i in usable {
                    if taken >= group.amount {
                        break;
                    }
                    let have = amount_of(&mine[i]);
                    let take = have.min(group.amount - taken);
                    set_amount(&mut mine[i], have - take);
                    taken += take;
                }
            } else {
                let mut portions = group.portions.clone();
                portions.sort_by(|a, b| b.total_cmp(a));
                for portion in portions {
                    let Some(i) = pick_crate(&mine, &norm, grade, portion, None) else { continue };
                    let left = amount_of(&mine[i]) - portion;
                    set_amount(&mut mine[i], left);
                    taken += portion;
                }
            }
            let too_low = py_sum(mine.iter().filter(|p| quality_of(p) < grade).map(amount_of));
            let need = Need {
                material: group.name,
                needed: group.amount,
                have: taken,
                missing: (group.amount - taken).max(0.0),
                min_quality: grade,
                too_low,
            };
            if need.missing > 0.0 { missing.push(need) } else { complete.push(need) }
        }
    }
    missing.sort_by(|a, b| b.missing.total_cmp(&a.missing).then_with(|| lower(&a.material).cmp(&lower(&b.material))));
    complete.sort_by_cached_key(|n| lower(&n.material));
    let without_recipe: BTreeSet<String> = without_recipe.into_iter().filter(|n| !n.is_empty()).collect();
    FarmList { missing, complete, posts: crafted, without_recipe: without_recipe.into_iter().collect() }
}

fn set_amount(entry: &mut Value, amount: f64) {
    if let Value::Object(map) = entry {
        versekit_core::jsonio::map_set(map, "menge", Value::Float(amount));
    }
}

/// A place where several of the missing materials lie.
#[derive(Clone, Debug, PartialEq)]
pub struct GatheringPlace {
    pub name: String,
    pub system: String,
    /// The missing materials that lie there, alphabetical.
    pub ores: Vec<String>,
}

/// Where the missing materials lie (`pages.farm_locations`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Locations {
    /// Per material up to `per_ore` places as (place, system, share).
    pub per_ore: BTreeMap<String, Vec<(String, String, f64)>>,
    /// Places that hold at least two of the missing materials, the best first.
    pub gathering: Vec<GatheringPlace>,
}

/// How many places per ore and how many gathering places are named.
pub const PER_ORE: usize = 3;
pub const PLACES: usize = 5;

/// The places of the `needed` materials: the richest three per material, and
/// the places that hold two or more of them.
pub fn locations(needed: &[String], mining: &MiningData) -> Locations {
    // An ore by its comparable name; of two with the same key the later one.
    let by_key: HashMap<String, &Ore> = mining.ores().iter().map(|o| (mining.material_key(&o.name), o)).collect();
    let mut per_ore: BTreeMap<String, Vec<(String, String, f64)>> = BTreeMap::new();
    // (place, system) with the materials that lie there and their share, in the order they appear.
    type Gathered = ((String, String), Vec<(String, f64)>);
    let mut collected: Vec<Gathered> = Vec::new();
    for material in needed {
        let Some(ore) = by_key.get(&mining.material_key(material)) else { continue };
        per_ore.insert(
            material.clone(),
            ore.places.iter().take(PER_ORE).map(|p| (p.name.clone(), p.system.clone(), p.share)).collect(),
        );
        for place in &ore.places {
            let key = (place.name.clone(), place.system.clone());
            let found = match collected.iter().position(|(k, _)| *k == key) {
                Some(i) => &mut collected[i].1,
                None => {
                    collected.push((key, Vec::new()));
                    &mut collected.last_mut().expect("just pushed").1
                }
            };
            match found.iter_mut().find(|(m, _)| m == material) {
                Some(entry) => entry.1 = place.share,
                None => found.push((material.clone(), place.share)),
            }
        }
    }
    let mut gathering: Vec<(f64, GatheringPlace)> = collected
        .into_iter()
        .filter(|(_, found)| found.len() >= 2)
        .map(|((name, system), found)| {
            let sum = py_sum(found.iter().map(|(_, share)| *share));
            let mut ores: Vec<String> = found.into_iter().map(|(m, _)| m).collect();
            ores.sort_by_cached_key(|m| lower(m));
            (sum, GatheringPlace { name, system, ores })
        })
        .collect();
    gathering.sort_by(|a, b| {
        b.1.ores
            .len()
            .cmp(&a.1.ores.len())
            .then(b.0.total_cmp(&a.0))
            .then_with(|| lower(&a.1.name).cmp(&lower(&b.1.name)))
    });
    gathering.truncate(PLACES);
    Locations { per_ore, gathering: gathering.into_iter().map(|(_, place)| place).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{ItemValues, MaterialKind, Recipe, RecipeTier, Slot, SlotOption};
    use versekit_core::jsonio::loads;

    fn recipe(tag: &str, product: &str, parts: &[(&str, f64, i64)]) -> Recipe {
        Recipe {
            guid: tag.to_lowercase(),
            tag: tag.into(),
            product: Some(product.into()),
            tiers: vec![RecipeTier {
                craft_time_seconds: Some(60),
                slots: parts
                    .iter()
                    .map(|(material, amount, quality)| Slot {
                        name: "Frame".into(),
                        options: vec![SlotOption {
                            kind: MaterialKind::Resource,
                            material: Some((*material).into()),
                            quantity: Some(*amount),
                            min_quality: Some(*quality),
                            ..SlotOption::default()
                        }],
                        modifiers: Vec::new(),
                    })
                    .collect(),
            }],
            ..Recipe::default()
        }
    }

    fn setup(recipes: Vec<Recipe>, names: &[&str]) -> (GameValues, CraftBook) {
        let mut values = GameValues::default();
        for (recipe, name) in recipes.iter().zip(names) {
            values.items.insert(
                recipe.product.clone().unwrap(),
                ItemValues {
                    guid: recipe.product.clone().unwrap(),
                    name: Some((*name).into()),
                    ..ItemValues::default()
                },
            );
        }
        values.recipes = recipes;
        let book = CraftBook::build(&values);
        (values, book)
    }

    fn stock(text: &str) -> Vec<Value> {
        match loads(text).unwrap() {
            Value::Array(items) => items,
            _ => panic!("array"),
        }
    }

    fn post(name: &str, pieces: i64) -> Post {
        Post { name: name.into(), reference: String::new(), pieces }
    }

    #[test]
    fn two_items_that_each_would_be_covered_are_not_covered_together() {
        let (values, book) = setup(
            vec![recipe("BP_CRAFT_a", "a", &[("Iron", 2.0, 0)]), recipe("BP_CRAFT_b", "b", &[("Iron", 2.0, 0)])],
            &["Alpha", "Beta"],
        );
        let data = StockData::default();
        let rows = stock(r#"[{"material":"Iron","menge":3.0,"qualitaet":700}]"#);
        let single = farm_list(&values, &book, &[post("Alpha", 1)], &rows, &data);
        assert!(single.missing.is_empty() && single.posts == 1);
        let both = farm_list(&values, &book, &[post("Alpha", 1), post("Beta", 1)], &rows, &data);
        // Each portion is 2 SCU, the one crate holds 3: one portion is covered, the other is not.
        assert_eq!(both.missing.len(), 1);
        assert_eq!((both.missing[0].needed, both.missing[0].have, both.missing[0].missing), (4.0, 2.0, 2.0));
    }

    #[test]
    fn the_most_demanding_quality_is_served_first_and_takes_the_lowest_crate_that_is_enough() {
        let (values, book) = setup(
            vec![recipe("BP_CRAFT_a", "a", &[("Iron", 1.0, 800)]), recipe("BP_CRAFT_b", "b", &[("Iron", 1.0, 0)])],
            &["Alpha", "Beta"],
        );
        let rows = stock(
            r#"[{"material":"Iron","menge":1.0,"qualitaet":900},{"material":"Iron","menge":1.0,"qualitaet":300}]"#,
        );
        let list = farm_list(&values, &book, &[post("Beta", 1), post("Alpha", 1)], &rows, &StockData::default());
        assert!(list.missing.is_empty(), "{list:?}");
        assert_eq!(list.complete.len(), 2);
    }

    #[test]
    fn a_count_multiplies_and_an_item_without_recipe_is_named_not_counted_as_nothing() {
        let (values, book) = setup(vec![recipe("BP_CRAFT_a", "a", &[("Iron", 0.5, 0)])], &["Alpha"]);
        let list = farm_list(&values, &book, &[post("Alpha", 3), post("Unknown", 2)], &[], &StockData::default());
        assert_eq!(list.posts, 5, "the unknown item counts as planned");
        assert_eq!(list.without_recipe, ["Unknown"]);
        assert_eq!((list.missing[0].needed, list.missing[0].missing), (1.5, 1.5));
    }

    #[test]
    fn only_notes_to_craft_that_are_not_ticked_off_and_have_a_recipe_make_posts() {
        let (values, book) = setup(vec![recipe("BP_CRAFT_a", "a", &[("Iron", 0.5, 0)])], &["Alpha"]);
        let note = |name: &str, way: &str, done: bool| Note {
            name: name.into(),
            reference: String::new(),
            count: 2,
            way: way.into(),
            done,
        };
        let notes = [
            note("Alpha", "bauen", false),
            note("Alpha", "", false),
            note("Alpha", "kaufen", false),
            note("Alpha", "bauen", true),
            note("Unknown", "bauen", false),
        ];
        let posts = posts_of(&values, &book, &notes);
        assert_eq!(posts.len(), 2, "{posts:?}");
        assert!(posts.iter().all(|p| p.name == "Alpha" && p.pieces == 2));
    }

    fn fleet(text: &str) -> Map {
        match loads(text).unwrap() {
            Value::Object(map) => map,
            _ => panic!("object"),
        }
    }

    #[test]
    fn the_parts_on_build_of_the_hangar_and_the_wish_list_are_planned_with_the_notes() {
        let (values, book) = setup(
            vec![
                recipe("BP_CRAFT_a", "a", &[("Iron", 1.0, 0)]),
                recipe("BP_CRAFT_b", "b", &[("Gold", 2.0, 0)]),
                recipe("BP_CRAFT_c", "c", &[("Iron", 0.5, 0)]),
            ],
            &["Alpha", "Beta", "Gamma"],
        );
        let data = fleet(
            r#"{"format": 1,
              "schiffe": [{"name": "Ship One", "belegung": {
                  "p1": {"ref": "a", "name": "Alpha", "weg": "bauen"},
                  "p2": {"ref": "b", "name": "Beta", "weg": "kaufen"},
                  "p3": {"ref": "c", "name": "Gamma", "weg": "bauen", "erledigt": true},
                  "p4": {"ref": "f", "name": "Factory part", "weg": "bauen"},
                  "gone": {"ref": "a", "name": "Alpha", "weg": "bauen"},
                  "p5": {"ref": "unknown", "name": "No recipe", "weg": "bauen"}}},
                {"name": "Ship Without Slots", "belegung": {"p1": {"ref": "a", "name": "Alpha", "weg": "bauen"}}}],
              "wunsch": [{"name": "Ship Two", "belegung": {"p1": {"ref": "b", "name": "Beta", "weg": "bauen"}}}],
              "merkzettel": [{"name": "Gamma", "anzahl": 3, "weg": "bauen"}]}"#,
        );
        let slot = |path: &str, factory: &str| cart::Slot {
            path: path.into(),
            kind: "Cooler".into(),
            size: Some(1),
            factory_ref: factory.into(),
            factory_name: String::new(),
        };
        let provider = |ship: &Map, _chosen: &Map| -> Vec<cart::Slot> {
            match ship.iter().find(|(k, _)| k == "name").and_then(|(_, v)| v.as_str()) {
                Some("Ship One" | "Ship Two") => {
                    vec![slot("p1", ""), slot("p2", ""), slot("p3", ""), slot("p4", "f"), slot("p5", "")]
                }
                _ => Vec::new(),
            }
        };
        let posts = planned_posts(&data, &values, &book, Some(&provider));
        let named: Vec<(&str, &str, i64)> =
            posts.iter().map(|p| (p.name.as_str(), p.reference.as_str(), p.pieces)).collect();
        // The note first (it has no ship), then the ships by name; the bought, the ticked off, the factory part,
        // the slot that is gone and the part without a recipe stay out.
        assert_eq!(named, vec![("Gamma", "", 3), ("Alpha", "a", 1), ("Beta", "b", 1)]);
        let list = farm_list(&values, &book, &posts, &[], &StockData::default());
        assert_eq!(list.posts, 5);
        let iron = list.missing.iter().find(|n| n.material == "Iron").expect("iron");
        assert_eq!(iron.needed, 2.5, "one Alpha of the hangar and three Gamma of the notes");
        // Without slots no ship brings parts: only the note is left.
        let notes_only = planned_posts(&data, &values, &book, None);
        assert_eq!(notes_only, vec![Post { name: "Gamma".into(), reference: String::new(), pieces: 3 }]);
    }

    /// A fleet with the notes, a ship of the hangar and one of the wish list that plan the same parts.
    fn planning_fleet() -> (GameValues, CraftBook, Map) {
        let (values, book) = setup(
            vec![
                recipe("BP_CRAFT_a", "a", &[("Iron", 1.0, 0)]),
                recipe("BP_CRAFT_b", "b", &[("Gold", 2.0, 0)]),
                recipe("BP_CRAFT_c", "c", &[("Iron", 0.5, 0)]),
            ],
            &["Alpha", "Beta", "Gamma"],
        );
        let data = fleet(
            r#"{"format": 1,
              "schiffe": [{"name": "Ship One", "belegung": {
                  "p1": {"ref": "a", "name": "Alpha", "weg": "bauen"},
                  "p2": {"ref": "b", "name": "Beta", "weg": "kaufen"},
                  "p3": {"ref": "c", "name": "Gamma", "weg": "bauen", "erledigt": true}}}],
              "wunsch": [{"name": "Ship Two", "belegung": {"p1": {"ref": "a", "name": "Alpha", "weg": "bauen"}}}],
              "merkzettel": [{"name": "Alpha", "anzahl": 3, "weg": "bauen"}, {"name": "Beta", "anzahl": 2, "weg": "bauen"}]}"#,
        );
        (values, book, data)
    }

    fn two_ships_slots(_ship: &Map, _chosen: &Map) -> Vec<cart::Slot> {
        ["p1", "p2", "p3"]
            .iter()
            .map(|path| cart::Slot {
                path: (*path).into(),
                kind: "Cooler".into(),
                size: Some(1),
                factory_ref: String::new(),
                factory_name: String::new(),
            })
            .collect()
    }

    #[test]
    fn the_plan_keeps_where_each_part_comes_from_and_adds_up_to_the_posts_of_the_farm_list() {
        let (values, book, data) = planning_fleet();
        let lines = plan_lines(&data, &values, &book, Some(&two_ships_slots));
        let sources: Vec<(&str, PlanSource, &str, i64)> =
            lines.iter().map(|l| (l.name.as_str(), l.source, l.ship.as_str(), l.count)).collect();
        assert_eq!(
            sources,
            vec![
                ("Alpha", PlanSource::Notepad, "", 3),
                ("Beta", PlanSource::Notepad, "", 2),
                ("Alpha", PlanSource::Hangar, "Ship One", 1),
                ("Alpha", PlanSource::Wishlist, "Ship Two", 1),
            ],
            "the bought and the ticked-off parts are not in the plan"
        );
        // The farm list counts exactly these parts.
        let posts = planned_posts(&data, &values, &book, Some(&two_ships_slots));
        assert_eq!(posts, posts_of_plan(&lines));
        let list = farm_list(&values, &book, &posts, &[], &StockData::default());
        assert_eq!(list.posts, lines.iter().map(|l| l.count).sum::<i64>());
    }

    #[test]
    fn an_item_counts_what_the_notes_and_what_the_ships_plan_apart_and_a_tick_or_a_purchase_counts_nothing() {
        let (values, book, data) = planning_fleet();
        let lines = plan_lines(&data, &values, &book, Some(&two_ships_slots));
        let counts = plan_counts(&book, &lines);
        let of = |name: &str| {
            let at = book.items.iter().position(|item| item.name == name).expect("item");
            counts.get(&at).copied().unwrap_or_default()
        };
        assert_eq!(of("Alpha"), PlanCount { notes: 3, ships: 2 }, "a note by its name, the ships by the identifier");
        assert_eq!(of("Beta"), PlanCount { notes: 2, ships: 0 }, "the part bought on the ship is not planned");
        assert_eq!(of("Gamma"), PlanCount::default(), "the ticked-off part is not planned");
        assert_eq!(of("Alpha").total(), 5);
        // The sums over all items are the sum over all lines: no part is lost on the way to its item.
        let all: i64 = counts.values().map(PlanCount::total).sum();
        assert_eq!(all, lines.iter().map(|l| l.count).sum::<i64>());
    }

    #[test]
    fn a_stock_below_the_quality_is_a_hint_and_not_stock() {
        let (values, book) = setup(vec![recipe("BP_CRAFT_a", "a", &[("Iron", 1.0, 500)])], &["Alpha"]);
        let rows = stock(r#"[{"material":"Iron","menge":4.0,"qualitaet":300}]"#);
        let list = farm_list(&values, &book, &[post("Alpha", 1)], &rows, &StockData::default());
        assert_eq!((list.missing[0].have, list.missing[0].too_low), (0.0, 4.0));
    }

    #[test]
    fn places_with_two_missing_materials_are_gathering_places() {
        let text = r#"{"format": 1, "locations": [
          {"locationName": "Daymar", "system": "Stanton", "groups": [{"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "c", "relativeProbability": 1}]}]},
          {"locationName": "Cellin", "system": "Stanton", "groups": [{"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "d", "relativeProbability": 1}]}]}],
         "compositions": {"c": {"parts": [{"elementName": "Iron", "probability": 1, "minPercent": 50, "maxPercent": 50}, {"elementName": "Gold", "probability": 1, "minPercent": 50, "maxPercent": 50}]},
                          "d": {"parts": [{"elementName": "Iron", "probability": 1, "minPercent": 100, "maxPercent": 100}]}}}"#;
        let mining = MiningData::of(&loads(text).unwrap());
        let found = locations(&["Iron".to_string(), "Gold".to_string()], &mining);
        assert_eq!(found.gathering.len(), 1);
        assert_eq!(
            (found.gathering[0].name.as_str(), found.gathering[0].ores.as_slice()),
            ("Daymar", ["Gold".to_string(), "Iron".to_string()].as_slice())
        );
        assert_eq!(found.per_ore["Iron"][0].0, "Cellin", "the richer place first");
    }
}
