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

//! Ships: how much fits in, where one is for sale, what it costs
//! (`ships.update`, `with_cargo`, `catalog`, `buy_at`, `rent_at` of 4.12).
//!
//! The three UEX lists `vehicles`, `vehicles_purchases_prices` and
//! `vehicles_rentals_prices` all lie below the cut at 500 rows, so one
//! request each fetches the whole list. The price rows carry no ship name,
//! only `id_vehicle`; they are joined by that number, never by the name.
//! The file `schiffe.json` follows the patch. The names of the ships for the
//! hangar are read by [`crate::ships`].

use crate::jsonio::Value;
use crate::ordered::OrderedMap;
use crate::paths::Paths;
use crate::stock_data::{py_float_value, truthy};
use crate::stock_fetch::{self, Fetch, switched_off};
use crate::uex::{self, DAY, Store};

pub const SOURCE_SHIPS: &str = "https://api.uexcorp.uk/2.0/vehicles";
pub const SOURCE_BUY: &str = "https://api.uexcorp.uk/2.0/vehicles_purchases_prices";
pub const SOURCE_RENT: &str = "https://api.uexcorp.uk/2.0/vehicles_rentals_prices";
pub const CACHE: &str = "schiffe.json";
/// 5 holds the add-on mark.
pub const FORMAT: i64 = 5;
/// How long the stored list counts as fresh, in seconds: a day, like the price lists. A new game build
/// makes it stale at once, whatever its age.
pub const SHELF_LIFE: f64 = DAY;

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE).bound();

/// A ship of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Ship {
    /// The name with the maker, as UEX writes it (`name_full`).
    pub name: String,
    /// The cargo space in SCU.
    pub scu: i64,
    /// The yard (`werft`).
    pub maker: String,
    pub concept: bool,
    pub addon: bool,
}

/// Where a ship is for sale or rent.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// The terminal (`stelle`).
    pub spot: String,
    pub place: String,
    pub system: String,
    pub price: f64,
}

/// A ship with its cargo space for the lists of the pages.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub maker: String,
    pub scu: i64,
}

/// The stored list.
#[derive(Clone, Debug, Default)]
pub struct Market {
    pub ships: OrderedMap<Ship>,
    pub buy: OrderedMap<Vec<Offer>>,
    pub rent: OrderedMap<Vec<Offer>>,
}

fn string_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn offers_of(value: Option<&Value>) -> OrderedMap<Vec<Offer>> {
    let mut map = OrderedMap::new();
    if let Some(Value::Object(entries)) = value {
        for (ident, rows) in entries {
            let offers = match rows {
                Value::Array(rows) => rows
                    .iter()
                    .map(|z| Offer {
                        spot: string_of(z.get("stelle")),
                        place: string_of(z.get("ort")),
                        system: string_of(z.get("system")),
                        price: z.get("preis").and_then(py_float_value).unwrap_or(0.0),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            map.set(ident, offers);
        }
    }
    map
}

fn offers_value(map: &OrderedMap<Vec<Offer>>) -> Value {
    Value::Object(
        map.iter()
            .map(|(ident, offers)| {
                let rows = offers
                    .iter()
                    .map(|o| {
                        Value::Object(vec![
                            uex::field("stelle", Value::Str(o.spot.clone())),
                            uex::field("ort", Value::Str(o.place.clone())),
                            uex::field("system", Value::Str(o.system.clone())),
                            uex::field("preis", uex::float(o.price)),
                        ])
                    })
                    .collect();
                (ident.to_string(), Value::Array(rows))
            })
            .collect(),
    )
}

impl Market {
    pub fn load(paths: &Paths) -> Market {
        STORE.load(paths).map_or_else(Market::default, |data| Market::from_value(&data))
    }

    pub fn from_value(data: &Value) -> Market {
        let mut ships = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("schiffe") {
            for (ident, s) in entries {
                ships.set(
                    ident,
                    Ship {
                        name: string_of(s.get("name")),
                        scu: s.get("scu").and_then(stock_fetch::py_int).unwrap_or(0),
                        maker: string_of(s.get("werft")),
                        concept: truthy(s.get("konzept")),
                        addon: truthy(s.get("anbau")),
                    },
                );
            }
        }
        Market { ships, buy: offers_of(data.get("kauf")), rent: offers_of(data.get("miete")) }
    }

    /// Is any ship on the list?
    pub fn is_empty(&self) -> bool {
        self.ships.is_empty()
    }

    /// Every ship with cargo space, whether or not it is for sale: whoever
    /// plans a route flies the own ship.
    pub fn with_cargo(&self) -> Vec<Entry> {
        let mut result: Vec<Entry> = self
            .ships
            .values()
            .filter(|s| s.scu > 0 && !s.name.is_empty())
            .map(|s| Entry { name: s.name.clone(), maker: s.maker.clone(), scu: s.scu })
            .collect();
        result.sort_by_key(|e| crate::stock_data::lower(&e.name));
        result
    }

    /// Every ship that is for sale or rent somewhere.
    pub fn catalog(&self) -> Vec<Entry> {
        let mut result: Vec<Entry> = self
            .ships
            .iter()
            .filter(|(ident, s)| {
                (self.buy.get(ident).is_some() || self.rent.get(ident).is_some()) && !s.name.is_empty()
            })
            .map(|(_, s)| Entry { name: s.name.clone(), maker: s.maker.clone(), scu: s.scu })
            .collect();
        result.sort_by_key(|e| crate::stock_data::lower(&e.name));
        result
    }

    /// The cargo space of a ship by its whole name, or `0`.
    pub fn scu(&self, name: &str) -> i64 {
        let wanted = crate::stock_data::lower(crate::text::py_strip(name));
        self.ships.values().find(|s| crate::stock_data::lower(&s.name) == wanted).map_or(0, |s| s.scu)
    }

    fn places(&self, name: &str, rent: bool) -> Vec<Offer> {
        let wanted = crate::stock_data::lower(crate::text::py_strip(name));
        let Some((ident, _)) = self.ships.iter().find(|(_, s)| crate::stock_data::lower(&s.name) == wanted) else {
            return Vec::new();
        };
        let list = if rent { &self.rent } else { &self.buy };
        let mut offers = list.get(ident).cloned().unwrap_or_default();
        offers.sort_by(|a, b| a.price.total_cmp(&b.price));
        offers
    }

    /// Where the ship is for sale, the cheapest first.
    pub fn buy_at(&self, name: &str) -> Vec<Offer> {
        self.places(name, false)
    }

    /// Where the ship is for rent (per day), the cheapest first.
    pub fn rent_at(&self, name: &str) -> Vec<Offer> {
        self.places(name, true)
    }
}

/// A price list `{ship number: [offers]}` out of the rows of UEX.
fn collect_prices(rows: &[Value], field: &str) -> Option<OrderedMap<Vec<Offer>>> {
    let mut result: OrderedMap<Vec<Offer>> = OrderedMap::new();
    for x in rows {
        let ident = match x.get("id_vehicle").filter(|v| truthy(Some(v))) {
            Some(Value::Str(s)) => s.clone(),
            Some(Value::Int(n)) => n.clone(),
            Some(Value::Float(f)) => crate::jsonio::float_text(*f),
            _ => String::new(),
        };
        let price = uex::number(x, field)?;
        // A price of zero means: not available here.
        if ident.is_empty() || price <= 0.0 {
            continue;
        }
        result.entry_or_default(&ident).push(Offer {
            spot: uex::stripped(x, "terminal_name"),
            place: uex::first_set(x, &["space_station_name", "city_name", "outpost_name", "planet_name"]),
            system: uex::stripped(x, "star_system_name"),
            price,
        });
    }
    Some(result)
}

/// Fetches the three lists when they are missing or stale (`ships.update`).
/// `true` when a list is there afterwards. The price lists may fail: the
/// cargo spaces are worth keeping alone.
pub fn update(paths: &Paths, now: f64, fetch: Fetch<'_>) -> bool {
    if switched_off(&paths.host) {
        return false;
    }
    if !STORE.stale(paths, now) {
        return true;
    }
    let Some(data) = uex::fetch("schiffe", SOURCE_SHIPS, fetch) else { return false };
    if uex::rows(&data).is_empty() {
        return false;
    }
    let mut ships: OrderedMap<Ship> = OrderedMap::new();
    for x in uex::rows(&data) {
        let ident = match x.get("id").filter(|v| truthy(Some(v))) {
            Some(Value::Str(s)) => s.clone(),
            Some(Value::Int(n)) => n.clone(),
            Some(Value::Float(f)) => crate::jsonio::float_text(*f),
            _ => String::new(),
        };
        let name = uex::first_set(x, &["name_full", "name"]);
        if ident.is_empty() || name.is_empty() {
            continue;
        }
        let Some(scu) = uex::whole(x, "scu") else { return false };
        ships.set(
            &ident,
            Ship {
                name,
                scu,
                maker: uex::stripped(x, "company_name"),
                concept: truthy(x.get("is_concept")),
                addon: truthy(x.get("is_addon")),
            },
        );
    }
    let buy_data = uex::fetch("schiffe.kauf", SOURCE_BUY, fetch);
    let rent_data = uex::fetch("schiffe.miete", SOURCE_RENT, fetch);
    let Some(buy) = collect_prices(buy_data.as_ref().map_or(&[][..], uex::rows), "price_buy") else { return false };
    let Some(rent) = collect_prices(rent_data.as_ref().map_or(&[][..], uex::rows), "price_rent") else { return false };
    let ships_value = Value::Object(
        ships
            .iter()
            .map(|(ident, s)| {
                let mut fields = vec![
                    uex::field("name", Value::Str(s.name.clone())),
                    uex::field("scu", Value::int(s.scu)),
                    uex::field("werft", Value::Str(s.maker.clone())),
                ];
                if s.concept {
                    fields.push(uex::field("konzept", Value::int(1)));
                }
                if s.addon {
                    fields.push(uex::field("anbau", Value::int(1)));
                }
                (ident.to_string(), Value::Object(fields))
            })
            .collect(),
    );
    STORE
        .save(
            paths,
            vec![
                uex::field("schiffe", ships_value),
                uex::field("kauf", offers_value(&buy)),
                uex::field("miete", offers_value(&rent)),
            ],
            true,
            now,
        )
        .is_ok()
}

/// How old the stored list is in seconds, or `None`.
pub fn age(paths: &Paths, now: f64) -> Option<f64> {
    STORE.age(paths, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn market() -> Market {
        let data = crate::jsonio::loads(
            r#"{"schiffe": {"1": {"name": "MISC Freelancer MAX", "scu": 122, "werft": "MISC"},
                           "2": {"name": "Aegis Gladius", "scu": 0, "werft": "Aegis"},
                           "3": {"name": "Drake Caterpillar", "scu": 576, "werft": "Drake"}},
                "kauf": {"1": [{"stelle": "B", "ort": "Y", "system": "S", "preis": 300.0},
                               {"stelle": "A", "ort": "X", "system": "S", "preis": 200.0}]},
                "miete": {"2": [{"stelle": "C", "ort": "Z", "system": "S", "preis": 5.0}]}}"#,
        )
        .unwrap();
        Market::from_value(&data)
    }

    #[test]
    fn cargo_ships_are_sorted_and_the_ones_without_space_left_out() {
        let names: Vec<String> = market().with_cargo().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Drake Caterpillar", "MISC Freelancer MAX"]);
        assert_eq!(market().scu("misc freelancer max"), 122);
        assert_eq!(market().scu("nothing"), 0);
    }

    #[test]
    fn the_catalog_holds_what_is_for_sale_or_rent() {
        let names: Vec<String> = market().catalog().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Aegis Gladius", "MISC Freelancer MAX"]);
    }

    /// The list is fetched again after a day and when the game build changes, not before.
    #[test]
    fn the_stored_list_is_stale_after_a_day_and_after_a_new_game_build() {
        let folder = std::env::temp_dir().join(format!("versekit-ships-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &home)], &home);
        let paths = Paths::new(host);
        let build = |live: &str| {
            std::fs::write(
                folder.join("spielstand.json"),
                format!(r#"{{"live":"{live}","ptu":"","format":1,"geholt":1.0}}"#),
            )
            .unwrap();
        };
        let saved_at = 1_790_000_000.0;
        assert!(STORE.stale(&paths, saved_at), "without a file");
        build("4.10.2");
        STORE.save(&paths, Vec::new(), true, saved_at).unwrap();
        assert!(!STORE.stale(&paths, saved_at + 3_600.0), "an hour old stands");
        assert!(!STORE.stale(&paths, saved_at + DAY - 60.0), "just under a day stands");
        assert!(STORE.stale(&paths, saved_at + DAY + 60.0), "just over a day is fetched again");
        build("4.10.3");
        assert!(STORE.stale(&paths, saved_at + 3_600.0), "a new game build makes a fresh list stale");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_cheapest_place_comes_first() {
        let places = market().buy_at("MISC Freelancer MAX");
        assert_eq!(places[0].spot, "A");
        assert!(market().buy_at("Drake Caterpillar").is_empty());
    }
}
