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

//! The data of the shopping list: the stores of the program behind the
//! [`Sources`] of `versekit_core::cart`.
//!
//! ⚠ The slots of a ship are not read here. They come with the equipment of
//! the ships (P6), which puts its provider in `slots`; until then no ship has
//! slots and the list says so ("no slot data") instead of claiming that
//! nothing is to be got.

use versekit_core::cart::{self, Ingredient, Offer, Recipe, Slot, Sources, Step};
use versekit_core::jsonio::Map;
use versekit_core::raw_prices::RawPrices;
use versekit_core::ship_market::Market;
use versekit_core::shops::{ShopRow, Shops};

use crate::craft_book::{self, CraftBook};
use crate::farm::{self, Post};
use crate::values::GameValues;

/// Gives the slots of a ship for the equipment chosen so far: `(ship, chosen)`.
pub type SlotProvider<'a> = dyn Fn(&Map, &Map) -> Vec<Slot> + 'a;

/// The stores the shopping list reads.
pub struct CartData<'a> {
    pub values: &'a GameValues,
    pub book: &'a CraftBook,
    pub shops: &'a Shops,
    pub raw: &'a RawPrices,
    pub market: &'a Market,
    /// The slots of a ship for the equipment chosen so far: `(ship, chosen)`.
    pub slots: Option<&'a SlotProvider<'a>>,
}

fn offer_of(row: &ShopRow) -> Offer {
    Offer { shop: row.shop.clone(), place: row.place.clone(), system: row.system.clone(), price: row.price }
}

impl Sources for CartData<'_> {
    fn shop_known(&self, reference: &str) -> bool {
        self.shops.known(reference)
    }

    fn cheapest(&self, reference: &str) -> Option<(f64, String, String)> {
        self.shops.cheapest(reference)
    }

    fn shops_for(&self, reference: &str) -> Vec<Offer> {
        self.shops.shops_for(reference).map(|rows| rows.iter().map(offer_of).collect()).unwrap_or_default()
    }

    fn raw_buy_price(&self, material: &str) -> f64 {
        self.raw.price(material).map_or(0.0, |price| price.buy)
    }

    fn recipe_for(&self, reference: &str, name: &str) -> Option<Recipe> {
        let post = Post { name: name.to_string(), reference: reference.to_string(), pieces: 1 };
        let recipe = farm::recipe_of(self.values, self.book, &post)?;
        let steps = recipe
            .tiers
            .iter()
            .zip(craft_book::ingredients(recipe))
            .map(|(tier, ingredients)| Step {
                seconds: tier.craft_time_seconds.unwrap_or(0),
                ingredients: ingredients
                    .into_iter()
                    .map(|i| Ingredient { material: i.material, amount: i.amount })
                    .collect(),
            })
            .collect();
        Some(Recipe { blueprint: farm::blueprint_of(self.book, reference, name).to_string(), steps })
    }

    fn ship_offers(&self, name: &str) -> Vec<Offer> {
        self.market
            .buy_at(name)
            .into_iter()
            .map(|o| Offer { shop: o.spot, place: o.place, system: o.system, price: o.price })
            .collect()
    }

    fn slots(&self, ship: &Map, chosen: &Map) -> Vec<Slot> {
        self.slots.map_or_else(Vec::new, |provider| provider(ship, chosen))
    }
}

/// The parts the bill wants built and not ticked off, as posts for the farm
/// list (`cart.farm_list` takes them from the bill, a note without a recipe has
/// fallen back to buying there already).
pub fn posts_of_bill(lines: &[cart::Line]) -> Vec<Post> {
    lines
        .iter()
        .filter(|l| l.way == cart::CRAFT && !l.done)
        .map(|l| Post { name: l.name.clone(), reference: l.reference.clone(), pieces: l.count })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(way: &str, done: bool, count: i64) -> cart::Line {
        cart::Line {
            kind: cart::Kind::Part,
            ship: String::new(),
            origin: cart::Origin::Notepad,
            position: String::new(),
            name: "Helm".into(),
            reference: "ref".into(),
            way: way.into(),
            buy: cart::BuyOption {
                condition: cart::Condition::NotChecked,
                price: None,
                shop: String::new(),
                place: String::new(),
            },
            craft: cart::CraftOption {
                condition: cart::Condition::NoRecipe,
                material: None,
                seconds: None,
                blueprint: String::new(),
                unpriced: Vec::new(),
            },
            done,
            count,
            path: String::new(),
            paths: Vec::new(),
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        }
    }

    #[test]
    fn only_what_is_built_and_not_ticked_off_is_farmed() {
        let posts = posts_of_bill(&[line("bauen", false, 2), line("kaufen", false, 1), line("bauen", true, 1)]);
        assert_eq!(posts, vec![Post { name: "Helm".into(), reference: "ref".into(), pieces: 2 }]);
    }
}
