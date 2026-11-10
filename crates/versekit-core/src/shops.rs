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

//! What a finished part costs in a shop, and where it stands (`shops.py` of
//! 4.12).
//!
//! * The catalog `laeden-katalog.json`: every part UEX sells in the sections
//!   [`SECTIONS`] with maker, size, class, grade and the places that sell it,
//!   and every other good of the shops in [`GOODS_SECTIONS`] with maker and
//!   places; the cheapest price of each, and the shops ([`Outlet`]) with what
//!   each sells at which price. It follows the patch and takes about two
//!   minutes to build (three requests per group of equipment, two per group of
//!   goods, one per group of goods nobody sells); it is built in the background
//!   and never shipped.
//! * The prices `laeden.json`: the shops of one part, fetched when somebody
//!   looks at it and kept for up to [`SHELF_LIFE`] (at most [`MAX_ITEMS`]
//!   parts).
//!
//! A part is looked up by the entity code of the game, which is exactly the
//! `uuid` of UEX, never by a part of its name. A part without a `uuid` is
//! led by its UEX number (`id:123`). Only where the code finds nothing, the
//! whole name is searched in the catalog (never a part of it); a name UEX
//! uses twice is not assigned.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::errors;
use crate::jsonio::{self, Value};
use crate::ordered::OrderedMap;
use crate::paths::Paths;
use crate::stock_data::{lower, py_float_value, truthy};
use crate::stock_fetch::{self, Fetch, switched_off};
use crate::text::py_strip;
use crate::uex::{self, DAY, Store};

pub const SOURCE: &str = "https://api.uexcorp.uk/2.0/items_prices?uuid=";
pub const SOURCE_BY_ID: &str = "https://api.uexcorp.uk/2.0/items_prices?id_item=";
pub const SOURCE_CATEGORIES: &str = "https://api.uexcorp.uk/2.0/categories";
pub const SOURCE_ITEMS: &str = "https://api.uexcorp.uk/2.0/items?id_category=";
pub const SOURCE_CATEGORY_PRICES: &str = "https://api.uexcorp.uk/2.0/items_prices?id_category=";
pub const SOURCE_ATTRIBUTES: &str = "https://api.uexcorp.uk/2.0/items_attributes?id_category=";
/// The file of the shop prices.
pub const CACHE: &str = "laeden.json";
/// The file of the catalog.
pub const CATALOG_CACHE: &str = "laeden-katalog.json";
pub const FORMAT: i64 = 1;
/// 7: the cheapest price with its place for the part texts in the game. The shops of each place came later as
/// fields of their own ([`OUTLETS_MARK`]), so that the readers of format 7 keep reading the file.
pub const FORMAT_CATALOG: i64 = 7;
/// The field that says the catalog holds the shops (`laeden`, `l`); a catalog without it is built anew once.
pub const OUTLETS_MARK: &str = "mit_laeden";
/// The field that says which sections the catalog holds: a catalog with a smaller number is built anew once and
/// stays readable until the new one is there.
pub const RANGE_MARK: &str = "warenumfang";
/// [`RANGE_MARK`] of a catalog with [`SECTIONS`] and [`GOODS_SECTIONS`].
pub const RANGE: i64 = 2;
/// A part without a code is led by its UEX number with this prefix.
pub const ID_PREFIX: &str = "id:";
/// The sections of UEX with equipment: with size, class and grade.
pub const SECTIONS: [&str; 8] =
    ["Systems", "Vehicle Weapons", "Utility", "Personal Weapons", "Armor", "Avionics", "Undersuits", "Propulsion"];
/// The sections of UEX with the other goods of the shops: food, drink, medicine, clothing, liveries and the like.
/// They have no size, class or grade worth a filter. Commodities and vehicles are left out (they have pages of their
/// own), and so are data and other (no shop sells them).
pub const GOODS_SECTIONS: [&str; 8] =
    ["Clothing", "Miscellaneous", "Consumable", "Liveries", "Technology", "Module", "Decorations", "Flair"];

/// Is `section` one of the sections the catalog holds?
pub fn held_section(section: &str) -> bool {
    SECTIONS.contains(&section) || GOODS_SECTIONS.contains(&section)
}
/// The emergency limit of the prices of a part, in seconds.
pub const SHELF_LIFE: f64 = 30.0 * DAY;
/// The emergency limit of the catalog, in seconds.
pub const CATALOG_LIFE: f64 = 90.0 * DAY;
/// The most parts the price file keeps; the oldest drops out.
pub const MAX_ITEMS: usize = 400;

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE).bound();
const CATALOG: Store = Store::new(CATALOG_CACHE, FORMAT_CATALOG, CATALOG_LIFE).bound();

/// Only one catalog build at a time.
static CATALOG_LOCK: Mutex<()> = Mutex::new(());

/// The text key of a section (English UEX name).
pub fn section_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "Armor" => "s_uk_armor",
        "Avionics" => "s_uk_avionics",
        "Personal Weapons" => "s_uk_personal_weapons_s",
        "Propulsion" => "s_uk_propulsion",
        "Systems" => "s_uk_systems",
        "Undersuits" => "s_uk_undersuits_s",
        "Utility" => "s_uk_utility",
        "Vehicle Weapons" => "s_uk_vehicle_weapons_s",
        "Clothing" => "s_uk_clothing_s",
        "Miscellaneous" => "s_uk_miscellaneous_s",
        "Consumable" => "s_uk_consumable_s",
        "Liveries" => "s_uk_liveries_s",
        "Technology" => "s_uk_technology",
        "Module" => "s_uk_module_s",
        "Decorations" => "s_uk_decorations_s",
        "Flair" => "s_uk_flair",
        _ => return None,
    })
}

/// The text key of a group (English UEX name).
pub fn group_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "Arms" => "s_uk_arms",
        "Attachments" => "s_uk_attachments",
        "Backpacks" => "s_uk_backpacks",
        "Batteries" => "s_uk_batteries",
        "Bomb Racks" => "s_uk_bomb_racks",
        "Bombs" => "s_uk_bombs",
        "Container" => "s_uk_container",
        "Coolers" => "s_uk_coolers",
        "Docking Collars" => "s_uk_docking_collars",
        "External Fuel Tanks" => "s_uk_external_fuel_tanks",
        "Fabricator" => "s_uk_fabricator",
        "Flight Blade" => "s_uk_flight_blade",
        "Fuel Nozzle" => "s_uk_fuel_nozzle",
        "Full Set" => "s_uk_full_set",
        "Gadgets" => "s_uk_gadgets",
        "Gravity Generator" => "s_uk_gravity_generator",
        "Guns" => "s_uk_guns",
        "Helmets" => "s_uk_helmets",
        "Jump Modules" => "s_uk_jump_modules",
        "Legs" => "s_uk_legs",
        "Life Support Generator" => "s_uk_life_support_generator",
        "Mining Laser Heads" => "s_uk_mining_laser_heads",
        "Mining Modules" => "s_uk_mining_modules",
        "Missile Racks" => "s_uk_missile_racks",
        "Missiles" => "s_uk_missiles",
        "Personal Weapons" => "s_uk_personal_weapons",
        "Point Defense Cannon" => "s_uk_point_defense_cannon",
        "Power Plants" => "s_uk_power_plants",
        "Quantum Drives" => "s_uk_quantum_drives",
        "Radar" => "s_uk_radar",
        "Salvage Beams" => "s_uk_salvage_beams",
        "Scraper Beams" => "s_uk_scraper_beams",
        "Shield Generators" => "s_uk_shield_generators",
        "Torpedo Tubes" => "s_uk_torpedo_tubes",
        "Torso" => "s_uk_torso",
        "Tractor Beams" => "s_uk_tractor_beams",
        "Turrets" => "s_uk_turrets",
        "Undersuits" => "s_uk_undersuits",
        "Footwear" => "s_uk_footwear",
        "Gloves" => "s_uk_gloves",
        "Hats" => "s_uk_hats",
        "Jackets" => "s_uk_jackets",
        "Jumpsuits" => "s_uk_jumpsuits",
        "Legwear" => "s_uk_legwear",
        "Shirts" => "s_uk_shirts",
        "Eyeware" => "s_uk_eyeware",
        "Dresses" => "s_uk_dresses",
        "Consumables" => "s_uk_consumables",
        "Consumable" => "s_uk_consumable",
        "Miscellaneous" => "s_uk_miscellaneous",
        "Drinks" => "s_uk_drinks",
        "Foods" => "s_uk_foods",
        "Liveries" => "s_uk_liveries",
        "Mobiglas" => "s_uk_mobiglas",
        "Module" => "s_uk_module",
        "Decorations" => "s_uk_decorations",
        "Surface" => "s_uk_surface",
        _ => return None,
    })
}

/// The readable name of a section: translated where it has a text key, else as UEX writes it.
pub fn section_label(name: &str) -> String {
    section_key(name).map_or_else(|| name.to_string(), crate::language::t)
}

/// The readable name of a group: translated where it has a text key; else the name of a blueprint kind
/// ([`blueprint_group`]), else as UEX writes it.
pub fn group_label(name: &str) -> String {
    if let Some(key) = group_key(name) {
        return crate::language::t(key);
    }
    match name {
        GROUP_SHIP_WEAPONS => crate::language::t("s_ld_art_schiffswaffen"),
        GROUP_FPS_WEAPONS => crate::language::t("s_ld_art_fpswaffen"),
        _ => {
            let key = format!("he_art_{}", lower(name));
            if !name.is_empty() && crate::language::text_in(crate::language::Lang::De, &key).is_some() {
                crate::language::t(&key)
            } else {
                name.to_string()
            }
        }
    }
}

/// The subkinds of the weapons of ships and of the ones carried by hand (`pages.SHIP_WEAPONS`,
/// `pages.FPS_WEAPONS` of 4.12).
pub const SHIP_WEAPONS: [&str; 5] = ["laser", "ballistic", "distortion", "neutron", "tachyon"];
pub const FPS_WEAPONS: [&str; 6] = ["pistol", "rifle", "sniper", "smg", "shotgun", "lmg"];
/// The two groups the weapons of the blueprint list are split into.
pub const GROUP_SHIP_WEAPONS: &str = "weapons_schiff";
pub const GROUP_FPS_WEAPONS: &str = "weapons_fps";

/// The group of a blueprint in the list of the shops without a catalog (`_art_von` in `pages._shops`): the
/// kind of its recipe; the weapons split by their subkind into those of ships and those carried by hand,
/// a weapon of neither subkind stays `weapons`.
pub fn blueprint_group(kind: &str, subtype: &str) -> String {
    let kind = py_strip(kind);
    if kind != "weapons" {
        return kind.to_string();
    }
    let subtype = lower(py_strip(subtype));
    if SHIP_WEAPONS.contains(&subtype.as_str()) {
        GROUP_SHIP_WEAPONS.to_string()
    } else if FPS_WEAPONS.contains(&subtype.as_str()) {
        GROUP_FPS_WEAPONS.to_string()
    } else {
        kind.to_string()
    }
}

/// What the search of the shops looks through for an entry: the name, the maker, the section and the
/// group, readable (`section_label`, `group_label`) and as UEX writes them, in lower case.
pub fn search_text(
    name: &str,
    maker: &str,
    section_label: &str,
    group_label: &str,
    section: &str,
    group: &str,
) -> String {
    format!(
        "{} {} {} {} {} {}",
        lower(name),
        lower(maker),
        lower(section_label),
        lower(group_label),
        lower(section),
        lower(group)
    )
}

/// Whether the search of the shops finds an entry with the search text `hay` (see [`search_text`]) for
/// `typed`: every entry for nothing typed, none for one letter, else those that hold the typed text
/// anywhere, without regard to case.
pub fn search_finds(hay: &str, typed: &str) -> bool {
    let needle = lower(typed.trim());
    if needle.is_empty() {
        return true;
    }
    needle.chars().count() >= 2 && hay.contains(&needle)
}

impl CatalogItem {
    /// What the search of the shops looks through for this part.
    pub fn search_text(&self) -> String {
        search_text(
            &self.name,
            &self.maker,
            &section_label(&self.section),
            &group_label(&self.group),
            &self.section,
            &self.group,
        )
    }
}

/// The place of a UEX price row: station, else city, else outpost.
pub fn place_of(row: &Value) -> String {
    uex::first_set(row, &["space_station_name", "city_name", "outpost_name"])
}

// ------------------------------------------------------------ the catalog

/// One part of the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    /// The entity code or `id:<number>` (`s`).
    pub code: String,
    /// The index of the group in the group list (`k`).
    pub group: usize,
    pub maker: String,
    pub size: String,
    pub class: String,
    pub grade: String,
    /// The places that sell it, sorted (`o`).
    pub places: Vec<String>,
    /// The cheapest price with its place (`p`); the price as the file holds it.
    pub cheapest: Option<(Value, String)>,
    /// The shops that sell it, each with its price, the cheapest first (`l`): the index in [`Catalog::outlets`].
    pub offers: Vec<(usize, f64)>,
}

/// A shop of the catalog: the terminal of UEX (`laeden`), at its place in its star system.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Outlet {
    pub name: String,
    pub place: String,
    pub system: String,
}

/// A part for the shops page.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogItem {
    pub name: String,
    pub code: String,
    /// The section, English as UEX writes it.
    pub section: String,
    /// The group, English as UEX writes it.
    pub group: String,
    pub maker: String,
    pub size: String,
    pub class: String,
    pub grade: String,
    pub places: Vec<String>,
}

/// The stored catalog.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    /// Lower-case name → the UEX number as the file holds it.
    pub names: OrderedMap<Value>,
    /// `[section, group]`.
    pub groups: Vec<(String, String)>,
    pub parts: Vec<Part>,
    /// Lower-case name → cheapest price and place.
    pub price_names: OrderedMap<(Value, String)>,
    pub codes: Vec<String>,
    /// The shops the parts name (`laeden`); empty in a catalog built before they were kept.
    pub outlets: Vec<Outlet>,
    /// Whether it was built with the shops ([`OUTLETS_MARK`]).
    pub with_outlets: bool,
    /// When it was fetched (`geholt`).
    pub fetched: Option<f64>,
}

/// The prices of the catalog without a network: by code, by name, and which
/// codes and names UEX knows. Two different answers: "UEX knows the part, but
/// no shop sells it" and "UEX does not know it at all".
#[derive(Clone, Debug, Default)]
pub struct Cheapest {
    pub by_code: std::collections::HashMap<String, (f64, String)>,
    pub by_name: std::collections::HashMap<String, (f64, String)>,
    pub known_codes: HashSet<String>,
    pub known_names: HashSet<String>,
}

fn string_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn pair_of(value: Option<&Value>) -> Option<(Value, String)> {
    match value {
        Some(Value::Array(items)) if items.len() == 2 => Some((items[0].clone(), string_of(items.get(1)))),
        _ => None,
    }
}

impl Catalog {
    /// Reads the stored file; without a usable one the catalog is empty.
    pub fn load(paths: &Paths) -> Catalog {
        CATALOG.load(paths).map_or_else(Catalog::default, |data| Catalog::from_value(&data))
    }

    pub fn from_value(data: &Value) -> Catalog {
        let mut names = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("namen") {
            for (name, ident) in entries {
                names.set(name, ident.clone());
            }
        }
        let groups = match data.get("kategorien") {
            Some(Value::Array(rows)) => rows
                .iter()
                .map(|row| match row {
                    Value::Array(pair) => (string_of(pair.first()), string_of(pair.get(1))),
                    _ => (String::new(), String::new()),
                })
                .collect(),
            _ => Vec::new(),
        };
        let parts = match data.get("teile") {
            Some(Value::Array(rows)) => rows
                .iter()
                .map(|x| Part {
                    name: string_of(x.get("n")),
                    code: string_of(x.get("s")),
                    group: x.get("k").and_then(stock_fetch::py_int).map_or(usize::MAX, |n| n.max(0) as usize),
                    maker: string_of(x.get("h")),
                    size: string_of(x.get("g")),
                    class: string_of(x.get("c")),
                    grade: string_of(x.get("q")),
                    places: match x.get("o") {
                        Some(Value::Array(places)) => places.iter().map(|p| string_of(Some(p))).collect(),
                        _ => Vec::new(),
                    },
                    cheapest: pair_of(x.get("p")),
                    offers: match x.get("l") {
                        Some(Value::Array(rows)) => rows
                            .iter()
                            .filter_map(|row| match row {
                                Value::Array(pair) if pair.len() == 2 => Some((
                                    usize::try_from(stock_fetch::py_int(&pair[0])?).ok()?,
                                    py_float_value(&pair[1])?,
                                )),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                })
                .collect(),
            _ => Vec::new(),
        };
        let outlets: Vec<Outlet> = match data.get("laeden") {
            Some(Value::Array(rows)) => rows
                .iter()
                .map(|row| match row {
                    Value::Array(fields) => Outlet {
                        name: string_of(fields.first()),
                        place: string_of(fields.get(1)),
                        system: string_of(fields.get(2)),
                    },
                    _ => Outlet { name: String::new(), place: String::new(), system: String::new() },
                })
                .collect(),
            _ => Vec::new(),
        };
        // An offer that names no shop of the table is left out.
        let parts = parts
            .into_iter()
            .map(|mut part: Part| {
                part.offers.retain(|(at, _)| *at < outlets.len());
                part
            })
            .collect();
        let mut price_names = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("preise_namen") {
            for (name, pair) in entries {
                if let Some(pair) = pair_of(Some(pair)) {
                    price_names.set(name, pair);
                }
            }
        }
        let codes = match data.get("kennungen") {
            Some(Value::Array(items)) => items.iter().map(|i| string_of(Some(i))).collect(),
            _ => Vec::new(),
        };
        Catalog {
            names,
            groups,
            parts,
            price_names,
            codes,
            outlets,
            with_outlets: truthy(data.get(OUTLETS_MARK)),
            fetched: data.get("geholt").filter(|v| truthy(Some(v))).and_then(py_float_value),
        }
    }

    /// Was the catalog built with the shops of its places? One built before is built anew ([`catalog_due`]).
    pub fn knows_outlets(&self) -> bool {
        self.with_outlets
    }

    /// The shops at `place` with how many parts each sells, by name without case.
    pub fn outlets_at(&self, place: &str) -> Vec<(usize, usize)> {
        let mut counts = vec![0usize; self.outlets.len()];
        for part in &self.parts {
            for (at, _) in &part.offers {
                counts[*at] += 1;
            }
        }
        let mut found: Vec<(usize, usize)> = self
            .outlets
            .iter()
            .enumerate()
            .filter(|(at, outlet)| outlet.place == place && counts[*at] > 0)
            .map(|(at, _)| (at, counts[at]))
            .collect();
        found.sort_by(|a, b| {
            let (x, y) = (&self.outlets[a.0].name, &self.outlets[b.0].name);
            lower(x).cmp(&lower(y)).then_with(|| x.cmp(y))
        });
        found
    }

    /// What the shop `outlet` sells: the code of each part with its price there, in the order of the parts.
    pub fn offers_of(&self, outlet: usize) -> Vec<(String, f64)> {
        self.parts
            .iter()
            .filter(|part| !part.code.is_empty())
            .filter_map(|part| part.offers.iter().find(|(at, _)| *at == outlet).map(|(_, price)| (part.code.clone(), *price)))
            .collect()
    }

    /// The shop that `typed` names (without case and outer blanks): its label ([`Catalog::outlet_label`]), that is
    /// its name where only one shop carries it, else its name and its place.
    pub fn outlet_named(&self, typed: &str) -> Option<usize> {
        let wanted = lower(typed.trim());
        if wanted.is_empty() {
            return None;
        }
        (0..self.outlets.len()).find(|at| lower(&self.outlet_label(*at)) == wanted)
    }

    /// How the shop `at` is named for a choice: its name, or `name · place` where other shops carry the same
    /// name (a chain at several places).
    pub fn outlet_label(&self, at: usize) -> String {
        let Some(outlet) = self.outlets.get(at) else { return String::new() };
        let shared = self.outlets.iter().filter(|o| lower(&o.name) == lower(&outlet.name)).count() > 1;
        if shared { format!("{} · {}", outlet.name, outlet.place) } else { outlet.name.clone() }
    }

    /// The labels of all shops that sell something ([`Catalog::outlet_label`]), once each, by label without case.
    pub fn outlet_names(&self) -> Vec<String> {
        let mut selling = vec![false; self.outlets.len()];
        for part in &self.parts {
            for (at, _) in &part.offers {
                selling[*at] = true;
            }
        }
        let mut names: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for at in (0..self.outlets.len()).filter(|at| selling[*at] && !self.outlets[*at].name.is_empty()) {
            let label = self.outlet_label(at);
            if seen.insert(lower(&label)) {
                names.push(label);
            }
        }
        names.sort_by(|a, b| lower(a).cmp(&lower(b)).then_with(|| a.cmp(b)));
        names
    }

    /// Is the catalog there? Without parts there is no shop list.
    pub fn ready(&self) -> bool {
        !self.parts.is_empty()
    }

    /// Every part the sections hold that a shop sells, in the order of the
    /// file (by name without case).
    pub fn items(&self) -> Vec<CatalogItem> {
        self.parts
            .iter()
            .map(|x| {
                let (section, group) = self.groups.get(x.group).cloned().unwrap_or_default();
                CatalogItem {
                    name: x.name.clone(),
                    code: x.code.clone(),
                    section,
                    group,
                    maker: x.maker.clone(),
                    size: x.size.clone(),
                    class: x.class.clone(),
                    grade: x.grade.clone(),
                    places: x.places.clone(),
                }
            })
            .collect()
    }

    /// All places that sell something, in the order of the alphabet without case.
    pub fn places(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for part in &self.parts {
            for place in &part.places {
                if seen.insert(place.clone()) {
                    found.push(place.clone());
                }
            }
        }
        found.sort_by(|a, b| lower(a).cmp(&lower(b)).then_with(|| a.cmp(b)));
        found
    }

    /// The prices of the catalog (`cheapest_by_entity`).
    pub fn cheapest(&self) -> Cheapest {
        let mut out = Cheapest::default();
        for part in &self.parts {
            if let Some((price, place)) = &part.cheapest
                && !part.code.is_empty()
            {
                out.by_code.insert(part.code.clone(), (py_float_value(price).unwrap_or(0.0), place.clone()));
            }
        }
        for (name, (price, place)) in self.price_names.iter() {
            out.by_name.insert(name.to_string(), (py_float_value(price).unwrap_or(0.0), place.clone()));
        }
        out.known_codes = self.codes.iter().cloned().collect();
        out.known_names = self.names.iter().map(|(name, _)| name.to_string()).collect();
        out
    }

    /// The UEX number of a name, or `None`. Only the whole name counts.
    pub fn uex_id(&self, name: &str) -> Option<Value> {
        self.names.get(&lower(py_strip(name))).cloned()
    }

    fn value(&self) -> Vec<(String, Value)> {
        let parts = self
            .parts
            .iter()
            .map(|x| {
                let mut fields = vec![
                    uex::field("n", Value::Str(x.name.clone())),
                    uex::field("s", Value::Str(x.code.clone())),
                    uex::field("k", Value::int(x.group as i64)),
                    uex::field("h", Value::Str(x.maker.clone())),
                    uex::field("g", Value::Str(x.size.clone())),
                    uex::field("c", Value::Str(x.class.clone())),
                    uex::field("q", Value::Str(x.grade.clone())),
                    uex::field("o", Value::Array(x.places.iter().cloned().map(Value::Str).collect())),
                ];
                if let Some((price, place)) = &x.cheapest {
                    fields.push(uex::field("p", Value::Array(vec![price.clone(), Value::Str(place.clone())])));
                }
                if !x.offers.is_empty() {
                    let offers = x
                        .offers
                        .iter()
                        .map(|(at, price)| Value::Array(vec![Value::int(*at as i64), uex::float(*price)]))
                        .collect();
                    fields.push(uex::field("l", Value::Array(offers)));
                }
                Value::Object(fields)
            })
            .collect();
        vec![
            uex::field("namen", Value::Object(self.names.iter().map(|(n, v)| (n.to_string(), v.clone())).collect())),
            uex::field(
                "kategorien",
                Value::Array(
                    self.groups
                        .iter()
                        .map(|(s, g)| Value::Array(vec![Value::Str(s.clone()), Value::Str(g.clone())]))
                        .collect(),
                ),
            ),
            uex::field("teile", Value::Array(parts)),
            uex::field(
                "preise_namen",
                Value::Object(
                    self.price_names
                        .iter()
                        .map(|(n, (p, o))| (n.to_string(), Value::Array(vec![p.clone(), Value::Str(o.clone())])))
                        .collect(),
                ),
            ),
            uex::field("kennungen", Value::Array(self.codes.iter().cloned().map(Value::Str).collect())),
            uex::field(OUTLETS_MARK, Value::Bool(self.with_outlets)),
            uex::field(RANGE_MARK, Value::int(RANGE)),
            uex::field(
                "laeden",
                Value::Array(
                    self.outlets
                        .iter()
                        .map(|o| {
                            Value::Array(vec![
                                Value::Str(o.name.clone()),
                                Value::Str(o.place.clone()),
                                Value::Str(o.system.clone()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]
    }
}

/// Is the catalog due: stale (patch, emergency limit), built before it kept the shops ([`OUTLETS_MARK`]) or all
/// the goods ([`RANGE_MARK`]), or older than `max_age` seconds? Reads only the file. Such a catalog stays readable
/// until the new one is there; a failed build keeps it.
pub fn catalog_due(paths: &Paths, now: f64, max_age: Option<f64>) -> bool {
    let data = CATALOG.load(paths);
    let complete = data.as_ref().is_some_and(|data| {
        truthy(data.get(OUTLETS_MARK)) && data.get(RANGE_MARK).and_then(stock_fetch::py_int).is_some_and(|n| n >= RANGE)
    });
    if CATALOG.stale_with(paths, data.as_ref(), now) || !complete {
        return true;
    }
    match max_age {
        None => false,
        Some(limit) => CATALOG.age(paths, now).is_none_or(|age| age >= limit),
    }
}

fn py_text(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Int(n) => n.clone(),
        Value::Float(x) => jsonio::float_text(*x),
        Value::Str(s) => s.clone(),
        other => jsonio::dumps(other, None, false),
    }
}

/// `str(x.get(key) or '')`.
fn text_or_empty(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(value) if truthy(Some(value)) => py_text(value),
        _ => String::new(),
    }
}

/// Builds the catalog (`_save_catalog`). `progress(done, total)` is called
/// after each group. `true` when it was written; a failed request keeps the
/// old file. With `max_age` the catalog is built again when it is older.
pub fn build_catalog(
    paths: &Paths,
    now: f64,
    max_age: Option<f64>,
    fetch: Fetch<'_>,
    progress: &mut dyn FnMut(usize, usize),
) -> bool {
    let _lock = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !catalog_due(paths, now, max_age) {
        return true;
    }
    if switched_off(&paths.host) {
        return false;
    }
    // First the game build, so that the catalog carries the right stamp.
    let _ =
        stock_fetch::update_build_with(paths, std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(now), fetch);
    let Some(categories) = uex::fetch("shops.categories", SOURCE_CATEGORIES, fetch) else { return false };
    if uex::rows(&categories).is_empty() {
        return false;
    }
    let chosen: Vec<&Value> = uex::rows(&categories)
        .iter()
        .filter(|k| k.get("section").and_then(Value::as_str).is_some_and(held_section))
        .collect();
    let mut names: OrderedMap<Value> = OrderedMap::new();
    let mut duplicates: Vec<String> = Vec::new();
    let mut id_to_uuid: OrderedMap<String> = OrderedMap::new();
    let mut groups: Vec<(String, String)> = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    let mut name_prices: OrderedMap<(Value, String)> = OrderedMap::new();
    let mut outlets: Vec<Outlet> = Vec::new();
    let mut outlet_index: std::collections::HashMap<Outlet, usize> = std::collections::HashMap::new();
    for (number, category) in chosen.iter().enumerate() {
        let section = string_of(category.get("section"));
        let goods = GOODS_SECTIONS.contains(&section.as_str());
        let Some(category_id) = category.get("id").map(py_text) else { return false };
        // A group of goods asks for its prices first: where nobody sells anything, its list is not asked for.
        let goods_prices = if goods {
            let Some(rows) = uex::fetch("shops.buyable", &format!("{SOURCE_CATEGORY_PRICES}{category_id}"), fetch)
            else {
                return false;
            };
            if !uex::rows(&rows).iter().any(|x| uex::number(x, "price_buy").is_some_and(|p| p > 0.0)) {
                progress(number + 1, chosen.len());
                continue;
            }
            Some(rows)
        } else {
            None
        };
        let cat_index = groups.len();
        groups.push((section, string_of(category.get("name"))));
        let Some(items) = uex::fetch("shops.catalog", &format!("{SOURCE_ITEMS}{category_id}"), fetch) else {
            return false;
        };
        let mut raw: OrderedMap<Part> = OrderedMap::new();
        for x in uex::rows(&items) {
            let Some(ident) = x.get("id").filter(|v| truthy(Some(v))) else { continue };
            let name = uex::stripped(x, "name");
            let uuid = uex::stripped(x, "uuid");
            if !uuid.is_empty() {
                id_to_uuid.set(&py_text(ident), uuid);
            }
            raw.set(
                &py_text(ident),
                Part {
                    name: name.clone(),
                    code: String::new(),
                    group: cat_index,
                    maker: uex::stripped(x, "company_name"),
                    // The size UEX gives a good (a jacket, a drink) means nothing in the shop.
                    size: if goods { String::new() } else { py_strip(&text_or_empty(x, "size")).to_string() },
                    class: String::new(),
                    grade: String::new(),
                    places: Vec::new(),
                    cheapest: None,
                    offers: Vec::new(),
                },
            );
            let key = lower(&name);
            if key.is_empty() {
                continue;
            }
            if let Some(known) = names.get(&key)
                && !jsonio::py_eq_value(known, ident)
                && !duplicates.contains(&key)
            {
                duplicates.push(key.clone());
            }
            names.set(&key, ident.clone());
        }
        // Class and grade of each part; the size here beats the one of the list. Goods have none of them.
        let mut traits: OrderedMap<Vec<(String, String)>> = OrderedMap::new();
        let attributes = if goods {
            None
        } else {
            uex::fetch("shops.attributes", &format!("{SOURCE_ATTRIBUTES}{category_id}"), fetch)
        };
        for x in attributes.as_ref().map_or(&[][..], uex::rows) {
            let item_no = text_or_empty(x, "id_item");
            let mut field = uex::stripped(x, "attribute_name");
            let value = py_strip(&text_or_empty(x, "value")).to_string();
            if field == "Grade" || field == "Grade Letter" {
                field = "Grade".to_string();
            }
            if item_no.is_empty() || value.is_empty() || !["Size", "Class", "Grade"].contains(&field.as_str()) {
                continue;
            }
            let found = traits.entry_or_default(&item_no);
            match found.iter_mut().find(|(name, _)| *name == field) {
                Some(slot) => slot.1 = value,
                None => found.push((field, value)),
            }
        }
        let mut raw_parts: Vec<(String, Part)> = raw.iter().map(|(k, p)| (k.to_string(), p.clone())).collect();
        for (item_no, found) in traits.iter() {
            let Some((_, part)) = raw_parts.iter_mut().find(|(key, _)| key == item_no) else { continue };
            for (field, value) in found {
                match field.as_str() {
                    "Class" => part.class = value.clone(),
                    "Grade" => part.grade = value.clone(),
                    _ => part.size = value.clone(),
                }
            }
        }
        let price_rows = match goods_prices {
            Some(rows) => rows,
            None => {
                let Some(rows) = uex::fetch("shops.buyable", &format!("{SOURCE_CATEGORY_PRICES}{category_id}"), fetch)
                else {
                    return false;
                };
                rows
            }
        };
        // Per part the places that sell it, the cheapest price with its place, and the shops with their price.
        let mut sold_at: OrderedMap<Vec<String>> = OrderedMap::new();
        let mut cheapest: OrderedMap<(Value, String)> = OrderedMap::new();
        let mut offered: OrderedMap<Vec<(usize, f64)>> = OrderedMap::new();
        for x in uex::rows(&price_rows) {
            let price = uex::number(x, "price_buy").unwrap_or(0.0);
            if price <= 0.0 {
                continue;
            }
            let item = text_or_empty(x, "id_item");
            if item.is_empty() || !raw_parts.iter().any(|(key, _)| *key == item) {
                continue;
            }
            let here = sold_at.entry_or_default(&item);
            let place = place_of(x);
            if !place.is_empty() {
                if !here.contains(&place) {
                    here.push(place.clone());
                }
                let terminal = uex::stripped(x, "terminal_name");
                if !terminal.is_empty() {
                    let outlet =
                        Outlet { name: terminal, place: place.clone(), system: uex::stripped(x, "star_system_name") };
                    let at = *outlet_index.entry(outlet.clone()).or_insert_with(|| {
                        outlets.push(outlet);
                        outlets.len() - 1
                    });
                    let offers = offered.entry_or_default(&item);
                    match offers.iter_mut().find(|(known, _)| *known == at) {
                        Some(slot) => slot.1 = slot.1.min(price),
                        None => offers.push((at, price)),
                    }
                }
                let better = cheapest.get(&item).is_none_or(|(old, _)| price < py_float_value(old).unwrap_or(0.0));
                if better {
                    cheapest.set(&item, (x.get("price_buy").cloned().unwrap_or(Value::Null), place));
                }
            }
        }
        for (item, here) in sold_at.iter() {
            let Some((_, entry)) = raw_parts.iter().find(|(key, _)| key == item) else { continue };
            let mut part = entry.clone();
            part.code = id_to_uuid.get(item).cloned().unwrap_or_else(|| format!("{ID_PREFIX}{item}"));
            part.places = {
                let mut places = here.clone();
                places.sort();
                places
            };
            if let Some(offers) = offered.get(item) {
                let mut offers = offers.clone();
                offers.sort_by(|a, b| a.1.total_cmp(&b.1));
                part.offers = offers;
            }
            if let Some(hit) = cheapest.get(item) {
                part.cheapest = Some(hit.clone());
                let key = lower(&part.name);
                let price = py_float_value(&hit.0).unwrap_or(0.0);
                if !key.is_empty()
                    && name_prices.get(&key).is_none_or(|(old, _)| price < py_float_value(old).unwrap_or(0.0))
                {
                    name_prices.set(&key, hit.clone());
                }
            }
            parts.push(part);
        }
        progress(number + 1, chosen.len());
    }
    let mut kept = OrderedMap::new();
    for (name, ident) in names.iter() {
        if !duplicates.iter().any(|d| d == name) {
            kept.set(name, ident.clone());
        }
    }
    if kept.is_empty() {
        return false;
    }
    parts.sort_by_key(|part| lower(&part.name));
    let mut codes: Vec<String> = id_to_uuid.values().cloned().collect::<HashSet<_>>().into_iter().collect();
    codes.sort();
    let mut price_names = OrderedMap::new();
    for (name, hit) in name_prices.iter() {
        if kept.get(name).is_some() {
            price_names.set(name, hit.clone());
        }
    }
    let catalog =
        Catalog { names: kept, groups, parts, price_names, codes, outlets, with_outlets: true, fetched: Some(now) };
    CATALOG.save(paths, catalog.value(), true, now).is_ok()
}

// ------------------------------------------------------------ the prices of a part

/// One shop of a part.
#[derive(Clone, Debug, PartialEq)]
pub struct ShopRow {
    pub shop: String,
    pub place: String,
    pub system: String,
    pub price: f64,
    /// `100` = new; used goods are cheaper and worth less. `0` unknown.
    pub condition: i64,
}

/// The prices of the looked-up parts.
#[derive(Clone, Debug, Default)]
pub struct Shops {
    entries: OrderedMap<(f64, Vec<ShopRow>)>,
}

impl Shops {
    pub fn load(paths: &Paths) -> Shops {
        let mut entries = OrderedMap::new();
        if let Some(data) = STORE.load(paths)
            && let Some(Value::Object(items)) = data.get("teile")
        {
            for (code, entry) in items {
                let rows = match entry.get("zeilen") {
                    Some(Value::Array(rows)) => rows
                        .iter()
                        .map(|z| ShopRow {
                            shop: string_of(z.get("laden")),
                            place: string_of(z.get("ort")),
                            system: string_of(z.get("system")),
                            price: z.get("preis").and_then(py_float_value).unwrap_or(0.0),
                            condition: z.get("zustand").and_then(stock_fetch::py_int).unwrap_or(0),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let fetched = entry.get("geholt").and_then(py_float_value).unwrap_or(0.0);
                entries.set(code, (fetched, rows));
            }
        }
        Shops { entries }
    }

    /// Is something stored for this code (an empty result too)?
    pub fn known(&self, code: &str) -> bool {
        !code.is_empty() && self.entries.get(code).is_some()
    }

    /// How old the prices of this part are; `None` without them.
    pub fn age(&self, code: &str, now: f64) -> Option<f64> {
        self.entries.get(code).map(|(fetched, _)| now - fetched)
    }

    /// All shops that carry the part, the cheapest first. An empty list means
    /// UEX does not know the part, `None` that nobody looked yet.
    pub fn shops_for(&self, code: &str) -> Option<&[ShopRow]> {
        self.entries.get(code).map(|(_, rows)| &rows[..])
    }

    /// The cheapest shop: `(price, shop, place)`.
    pub fn cheapest(&self, code: &str) -> Option<(f64, String, String)> {
        let rows = self.shops_for(code)?;
        let mut best: Option<&ShopRow> = None;
        for row in rows {
            if best.is_none_or(|b| row.price < b.price) {
                best = Some(row);
            }
        }
        best.map(|row| {
            (row.price, if row.shop.is_empty() { "?".to_string() } else { row.shop.clone() }, row.place.clone())
        })
    }
}

/// Looks up the shop prices of a part (`fetch`). `name` is the fallback for a
/// code UEX does not know: the whole name is searched in the catalog. `true`
/// when something is stored afterwards (an empty result too).
pub fn fetch_part(
    paths: &Paths,
    shops: &mut Shops,
    catalog: &mut Catalog,
    (code, name): (&str, &str),
    force: bool,
    now: f64,
    fetch: Fetch<'_>,
) -> bool {
    if code.is_empty() {
        return false;
    }
    // A name in the field of the code makes a broken address: the guard
    // stands before the switch, so it works with the network off too.
    if code.chars().any(char::is_whitespace) || code.contains('"') {
        errors::record("shops.fetch", &format!("no code but a name: {:?}", code.chars().take(60).collect::<String>()));
        return false;
    }
    if switched_off(&paths.host) {
        return false;
    }
    if !force && shops.age(code, now).is_some_and(|age| age < SHELF_LIFE) {
        return true;
    }
    let data = if let Some(number) = code.strip_prefix(ID_PREFIX) {
        uex::fetch("shops.id", &format!("{SOURCE_BY_ID}{number}"), fetch)
    } else {
        uex::fetch("shops", &format!("{SOURCE}{code}"), fetch)
    };
    let Some(mut data) = data else { return false };
    if uex::rows(&data).is_empty() && !name.is_empty() {
        if !catalog.ready() {
            build_catalog(paths, now, None, fetch, &mut |_, _| {});
            *catalog = Catalog::load(paths);
        }
        if let Some(number) = catalog.uex_id(name)
            && let Some(by_id) = uex::fetch("shops.name", &format!("{SOURCE_BY_ID}{}", py_text(&number)), fetch)
            && !uex::rows(&by_id).is_empty()
        {
            data = by_id;
        }
    }
    let mut rows = Vec::new();
    for x in uex::rows(&data) {
        let Some(price) = uex::number(x, "price_buy") else { return false };
        // A price of zero means: this terminal does not sell it.
        if price <= 0.0 {
            continue;
        }
        let Some(condition) = uex::whole(x, "durability") else { return false };
        rows.push(ShopRow {
            shop: uex::stripped(x, "terminal_name"),
            place: place_of(x),
            system: uex::stripped(x, "star_system_name"),
            price,
            condition,
        });
    }
    rows.sort_by(|a, b| a.price.total_cmp(&b.price));
    // The file is read again before the part joins it: another fetch may have written meanwhile.
    *shops = Shops::load(paths);
    shops.entries.set(code, (now, rows));
    trim(shops);
    save_shops(paths, shops, now)
}

fn trim(shops: &mut Shops) {
    let count = shops.entries.len();
    if count <= MAX_ITEMS {
        return;
    }
    let mut by_age: Vec<(String, f64)> =
        shops.entries.iter().map(|(code, (fetched, _))| (code.to_string(), *fetched)).collect();
    by_age.sort_by(|a, b| a.1.total_cmp(&b.1));
    let drop: HashSet<String> = by_age.into_iter().take(count - MAX_ITEMS).map(|(code, _)| code).collect();
    let mut kept = OrderedMap::new();
    for (code, entry) in shops.entries.iter() {
        if !drop.contains(code) {
            kept.set(code, entry.clone());
        }
    }
    shops.entries = kept;
}

fn save_shops(paths: &Paths, shops: &Shops, now: f64) -> bool {
    let items = Value::Object(
        shops
            .entries
            .iter()
            .map(|(code, (fetched, rows))| {
                let rows = rows
                    .iter()
                    .map(|row| {
                        Value::Object(vec![
                            uex::field("laden", Value::Str(row.shop.clone())),
                            uex::field("ort", Value::Str(row.place.clone())),
                            uex::field("system", Value::Str(row.system.clone())),
                            uex::field("preis", uex::float(row.price)),
                            uex::field("zustand", Value::int(row.condition)),
                        ])
                    })
                    .collect();
                (
                    code.to_string(),
                    Value::Object(vec![
                        uex::field("geholt", uex::float(*fetched)),
                        uex::field("zeilen", Value::Array(rows)),
                    ]),
                )
            })
            .collect(),
    );
    STORE.save(paths, vec![uex::field("teile", items)], true, now).is_ok()
}

/// Forgets everything looked up (for checks).
pub fn forget(paths: &Paths, now: f64) -> bool {
    STORE.save(paths, vec![uex::field("teile", Value::Object(Vec::new()))], true, now).is_ok()
}

/// How old the catalog is; `None` without one.
pub fn catalog_age(paths: &Paths, now: f64) -> Option<f64> {
    CATALOG.age(paths, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_of_the_shops_needs_two_letters_and_finds_names_makers_and_translated_groups() {
        use crate::language::{Choice, set_thread_language};
        set_thread_language(Choice::De);
        let part = CatalogItem {
            name: "Frostline Cooler".into(),
            code: "uuid-cooler".into(),
            section: "Systems".into(),
            group: "Coolers".into(),
            maker: "Wen".into(),
            size: String::new(),
            class: String::new(),
            grade: String::new(),
            places: Vec::new(),
        };
        let hay = part.search_text();
        assert!(search_finds(&hay, "") && search_finds(&hay, "  "), "nothing typed: everything");
        assert!(!search_finds(&hay, "k"), "one letter: nothing");
        for typed in ["FROST", " cooler ", "wen", "kühl", "Schiffskompo", "coolers"] {
            assert!(search_finds(&hay, typed), "{typed:?} in {hay:?}");
        }
        assert!(!search_finds(&hay, "helm"));
        set_thread_language(Choice::En);
        assert!(!search_finds(&part.search_text(), "kühl"), "the group in the language in force");
        set_thread_language(Choice::Auto);
    }

    /// `SC_BP_NO_NET` of the environment given stops the catalog and a part before anything is
    /// asked; a name in the field of the code is refused before the switch. Without it the
    /// catalog is asked for, whatever the shell of the check has.
    #[test]
    fn no_net_of_the_given_environment_asks_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("versekit-laeden-aus-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let paths = |no_net: &str| {
            let vars = [("SC_BP_HOME", data.as_str()), (crate::legacy::ENV_NO_NET, no_net)];
            Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, "/"))
        };
        let asked = AtomicUsize::new(0);
        let fetch = |_: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
            asked.fetch_add(1, Ordering::Relaxed);
            Err(crate::scmdb::FetchError::Missing)
        };
        let off = paths("1");
        assert!(!build_catalog(&off, 0.0, None, &fetch, &mut |_, _| {}));
        let (mut shops, mut catalog) = (Shops::default(), Catalog::default());
        assert!(!fetch_part(&off, &mut shops, &mut catalog, ("123", "Teil"), true, 0.0, &fetch));
        assert!(!fetch_part(&paths("0"), &mut shops, &mut catalog, ("ein Name", "Teil"), true, 0.0, &fetch));
        assert_eq!(asked.load(Ordering::Relaxed), 0);
        build_catalog(&paths("0"), 0.0, None, &fetch, &mut |_, _| {});
        assert!(asked.load(Ordering::Relaxed) > 0, "without the switch the catalog is asked for");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_catalog_is_due_after_a_patch_and_by_age_on_demand() {
        let folder = std::env::temp_dir().join(format!("versekit-katalog-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &home)], &home);
        let paths = Paths::new(host);
        let write = |name: &str, text: &str| std::fs::write(folder.join(name), text).unwrap();
        assert!(catalog_due(&paths, 1000.0, None), "no file: due");
        write("spielstand.json", r#"{"live":"4.10.0","ptu":"","format":1,"geholt":1.0}"#);
        let catalog = |mark: &str| {
            format!(
                r#"{{"namen":{{}},"kategorien":[],"teile":[],"preise_namen":{{}},"kennungen":[],{mark}"format":{FORMAT_CATALOG},"geholt":900.0,"spielstand":"4.10.0"}}"#
            )
        };
        write(CATALOG_CACHE, &catalog(""));
        assert!(catalog_due(&paths, 1000.0, None), "a catalog without the shops is built anew once");
        write(CATALOG_CACHE, &catalog(&format!(r#""{OUTLETS_MARK}":true,"#)));
        assert!(catalog_due(&paths, 1000.0, None), "a catalog without all the goods is built anew once");
        write(CATALOG_CACHE, &catalog(&format!(r#""{OUTLETS_MARK}":true,"{RANGE_MARK}":{},"#, RANGE - 1)));
        assert!(catalog_due(&paths, 1000.0, None), "a catalog of a smaller range is built anew once");
        assert!(Catalog::load(&paths).knows_outlets(), "and stays readable until then");
        write(CATALOG_CACHE, &catalog(&format!(r#""{OUTLETS_MARK}":true,"{RANGE_MARK}":{RANGE},"#)));
        assert!(!catalog_due(&paths, 1000.0, None), "same build, young: not due");
        assert!(catalog_due(&paths, 1000.0, Some(50.0)), "older than the asked age: due");
        assert!(!catalog_due(&paths, 1000.0, Some(500.0)));
        write("spielstand.json", r#"{"live":"4.11.0","ptu":"","format":1,"geholt":1.0}"#);
        assert!(catalog_due(&paths, 1000.0, None), "a new game build makes it due at once");
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The catalog keeps the shops of each place from the answers it asks for anyway, with what each sells at
    /// which price; it reads back the same, and a shop is found by its name. Made-up answers, no network.
    #[test]
    fn the_catalog_keeps_the_shops_of_each_place_with_what_they_sell() {
        let folder = std::env::temp_dir().join(format!("versekit-laeden-orte-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let vars = [(crate::legacy::ENV_HOME, home.as_str()), (crate::legacy::ENV_NO_NET, "0")];
        let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
        let answer = |url: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
            let data = if url == SOURCE_CATEGORIES {
                r#"[{"id": 1, "section": "Systems", "name": "Coolers"}, {"id": 2, "section": "Armor", "name": "Helmets"}]"#
            } else if url == format!("{SOURCE_ITEMS}1") {
                r#"[{"id": 10, "uuid": "u-frost", "name": "Frostline", "company_name": "Wen", "size": "1"},
                    {"id": 11, "uuid": "u-glacier", "name": "Glacier", "company_name": "Wen", "size": "2"}]"#
            } else if url == format!("{SOURCE_ITEMS}2") {
                r#"[{"id": 20, "uuid": "u-helm", "name": "Morozov Helm", "company_name": "Roberts"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}1") {
                r#"[{"id_item": 10, "price_buy": 500, "terminal_name": "Cool Corner", "city_name": "Area18", "star_system_name": "Stanton"},
                    {"id_item": 10, "price_buy": 450, "terminal_name": "Cool Corner", "city_name": "Area18", "star_system_name": "Stanton"},
                    {"id_item": 10, "price_buy": 480, "terminal_name": "Dumpers Depot", "space_station_name": "Seraphim", "star_system_name": "Stanton"},
                    {"id_item": 11, "price_buy": 900, "terminal_name": "Dumpers Depot", "space_station_name": "Seraphim", "star_system_name": "Stanton"},
                    {"id_item": 11, "price_buy": 0, "terminal_name": "Cool Corner", "city_name": "Area18", "star_system_name": "Stanton"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}2") {
                r#"[{"id_item": 20, "price_buy": 1200, "terminal_name": "CenterMass", "city_name": "Area18", "star_system_name": "Stanton"},
                    {"id_item": 20, "price_buy": 1300, "terminal_name": "Dumpers Depot", "space_station_name": "Seraphim", "star_system_name": "Stanton"}]"#
            } else if url.starts_with(SOURCE_ATTRIBUTES) {
                "[]"
            } else {
                return Err(crate::scmdb::FetchError::Missing);
            };
            Ok(format!(r#"{{"status": "ok", "data": {data}}}"#).into_bytes())
        };
        assert!(build_catalog(&paths, 1000.0, None, &answer, &mut |_, _| {}));
        let catalog = Catalog::load(&paths);
        assert!(catalog.knows_outlets(), "{:?}", catalog.outlets);
        let name = |at: usize| catalog.outlets[at].name.clone();
        let at_area18: Vec<(String, usize)> = catalog.outlets_at("Area18").into_iter().map(|(at, n)| (name(at), n)).collect();
        assert_eq!(at_area18, [("CenterMass".to_string(), 1), ("Cool Corner".to_string(), 1)], "by name, a price of 0 is no offer");
        let at_seraphim: Vec<(String, usize)> = catalog.outlets_at("Seraphim").into_iter().map(|(at, n)| (name(at), n)).collect();
        assert_eq!(at_seraphim, [("Dumpers Depot".to_string(), 3)]);
        assert!(catalog.outlets_at("Lorville").is_empty());
        let depot = catalog.outlet_named("  dumpers depot ").expect("found by its name without case");
        assert_eq!(catalog.outlets[depot].system, "Stanton");
        let mut sold = catalog.offers_of(depot);
        sold.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(sold, [("u-frost".to_string(), 480.0), ("u-glacier".to_string(), 900.0), ("u-helm".to_string(), 1300.0)]);
        let corner = catalog.outlet_named("Cool Corner").unwrap();
        assert_eq!(catalog.offers_of(corner), [("u-frost".to_string(), 450.0)], "the cheaper of two rows of one shop");
        assert_eq!(catalog.outlet_names(), ["CenterMass", "Cool Corner", "Dumpers Depot"]);
        assert_eq!(catalog.outlet_named("Nirgends"), None);
        let frost = catalog.parts.iter().find(|p| p.code == "u-frost").unwrap();
        assert_eq!(frost.offers.first().map(|(_, price)| *price), Some(450.0), "the cheapest shop first");
        assert_eq!(frost.places, ["Area18", "Seraphim"], "the places stay as before");
        // A catalog built before the shops were kept knows none and reads all the same; it is due once.
        let older = Catalog::from_value(&jsonio::loads(r#"{"teile": [{"n": "X", "s": "u-x", "o": ["Area18"]}]}"#).unwrap());
        assert!(older.ready() && !older.knows_outlets() && older.outlets_at("Area18").is_empty());
        assert!(!catalog_due(&paths, 1000.0, None), "the new one is not due");
        // The part texts in the game read the same file as before: the cheapest price of each part.
        let texts = crate::injection::sources::shops(&paths);
        let helm = texts.by_entity.get("u-helm").map(|(price, place)| (py_float_value(price), place.clone()));
        assert_eq!(helm, Some((Some(1200.0), Value::str("Area18"))), "{:?}", texts.by_entity);
        // A chain at two places is chosen by its name and its place.
        let mut chain = catalog.clone();
        chain.outlets.push(Outlet { name: "CenterMass".into(), place: "Lorville".into(), system: "Stanton".into() });
        chain.parts[0].offers.push((chain.outlets.len() - 1, 999.0));
        assert_eq!(chain.outlet_named("CenterMass"), None, "the name alone names two");
        assert_eq!(chain.outlet_named("centermass · lorville"), Some(chain.outlets.len() - 1));
        assert!(chain.outlet_names().contains(&"CenterMass · Area18".to_string()), "{:?}", chain.outlet_names());
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The catalog holds the goods of the shops too (a drink, a jacket) with their shops and prices but without size,
    /// class or grade; a group of goods nobody sells costs one request, a group of goods two, and commodities and
    /// vehicles stay out. Made-up answers, no network.
    #[test]
    fn the_catalog_holds_the_goods_of_the_shops_with_as_few_requests_as_needed() {
        use crate::language::{Choice, set_thread_language};
        let folder = std::env::temp_dir().join(format!("versekit-laeden-waren-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let vars = [(crate::legacy::ENV_HOME, home.as_str()), (crate::legacy::ENV_NO_NET, "0")];
        let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
        let asked = Mutex::new(Vec::<String>::new());
        let answer = |url: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
            asked.lock().unwrap().push(url.to_string());
            let data = if url == SOURCE_CATEGORIES {
                r#"[{"id": 19, "section": "Systems", "name": "Coolers"},
                    {"id": 62, "section": "Miscellaneous", "name": "Drinks"},
                    {"id": 11, "section": "Clothing", "name": "Jackets"},
                    {"id": 68, "section": "Clothing", "name": "Eyeware"},
                    {"id": 36, "section": "Commodities", "name": "Commodities"},
                    {"id": 102, "section": "Vehicles", "name": "Vehicles"}]"#
            } else if url == format!("{SOURCE_ITEMS}19") {
                r#"[{"id": 10, "uuid": "u-frost", "name": "Frostline", "company_name": "Wen", "size": "1"}]"#
            } else if url == format!("{SOURCE_ITEMS}62") {
                r#"[{"id": 198, "uuid": "u-vestal", "name": "Vestal Water", "company_name": null, "size": "1"}]"#
            } else if url == format!("{SOURCE_ITEMS}11") {
                r#"[{"id": 300, "uuid": "u-jacket", "name": "Adiva Jacket", "company_name": "Upsurge", "size": "1"},
                    {"id": 301, "uuid": "u-unsold", "name": "Nobody's Jacket", "company_name": "Upsurge"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}19") {
                r#"[{"id_item": 10, "price_buy": 500, "terminal_name": "Cool Corner", "city_name": "Area18", "star_system_name": "Stanton"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}62") {
                r#"[{"id_item": 198, "price_buy": 3, "terminal_name": "Ellroy's - New Babbage", "city_name": "New Babbage", "star_system_name": "Stanton"},
                    {"id_item": 198, "price_buy": 4, "terminal_name": "Bar - Dunboro", "outpost_name": "Dunboro", "star_system_name": "Stanton"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}11") {
                r#"[{"id_item": 300, "price_buy": 900, "terminal_name": "Aparelli - New Babbage", "city_name": "New Babbage", "star_system_name": "Stanton"},
                    {"id_item": 301, "price_buy": 0, "terminal_name": "Aparelli - New Babbage", "city_name": "New Babbage", "star_system_name": "Stanton"}]"#
            } else if url == format!("{SOURCE_CATEGORY_PRICES}68") {
                r#"[{"id_item": 400, "price_buy": 0, "terminal_name": "Aparelli - New Babbage", "city_name": "New Babbage"}]"#
            } else if url == format!("{SOURCE_ATTRIBUTES}19") {
                r#"[{"id_item": 10, "attribute_name": "Class", "value": "Military"}, {"id_item": 10, "attribute_name": "Grade", "value": "A"}]"#
            } else if url.starts_with(SOURCE_ATTRIBUTES) {
                r#"[{"id_item": 198, "attribute_name": "Size", "value": "3"}]"#
            } else if url.starts_with("https://api.uexcorp.uk/2.0/game_versions") {
                return Err(crate::scmdb::FetchError::Missing);
            } else {
                panic!("asked for {url}");
            };
            Ok(format!(r#"{{"status": "ok", "data": {data}}}"#).into_bytes())
        };
        let mut steps = Vec::new();
        assert!(build_catalog(&paths, 1000.0, None, &answer, &mut |step, of| steps.push((step, of))));
        assert_eq!(steps.last(), Some(&(4, 4)), "four groups, commodities and vehicles left out: {steps:?}");
        let catalog_asks: Vec<String> = asked
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.starts_with("https://api.uexcorp.uk/2.0/item"))
            .cloned()
            .collect();
        let expected = [
            format!("{SOURCE_ITEMS}19"),
            format!("{SOURCE_ATTRIBUTES}19"),
            format!("{SOURCE_CATEGORY_PRICES}19"),
            format!("{SOURCE_CATEGORY_PRICES}62"),
            format!("{SOURCE_ITEMS}62"),
            format!("{SOURCE_CATEGORY_PRICES}11"),
            format!("{SOURCE_ITEMS}11"),
            format!("{SOURCE_CATEGORY_PRICES}68"),
        ];
        assert_eq!(catalog_asks, expected, "three for equipment, two for goods, one for goods nobody sells");
        let catalog = Catalog::load(&paths);
        let items = catalog.items();
        let water = items.iter().find(|i| i.name == "Vestal Water").expect("the drink is in the catalog");
        assert_eq!((water.section.as_str(), water.group.as_str()), ("Miscellaneous", "Drinks"));
        assert_eq!(water.places, ["Dunboro", "New Babbage"]);
        assert_eq!(
            (water.size.as_str(), water.class.as_str(), water.grade.as_str()),
            ("", "", ""),
            "no size for goods"
        );
        let ellroy = catalog.outlet_named("Ellroy's - New Babbage").expect("the shop of the drink");
        assert_eq!(catalog.offers_of(ellroy), [("u-vestal".to_string(), 3.0)]);
        assert_eq!(
            catalog.parts.iter().find(|p| p.code == "u-vestal").and_then(|p| p.offers.first()).map(|o| o.1),
            Some(3.0)
        );
        let frost = items.iter().find(|i| i.name == "Frostline").unwrap();
        assert_eq!((frost.size.as_str(), frost.class.as_str(), frost.grade.as_str()), ("1", "Military", "A"));
        assert!(items.iter().any(|i| i.name == "Adiva Jacket" && i.size.is_empty()));
        assert!(!items.iter().any(|i| i.name == "Nobody's Jacket"), "a price of 0 is no offer");
        assert!(!catalog.groups.iter().any(|(_, g)| g == "Eyeware"), "a group nobody sells stays out");
        assert!(!catalog_due(&paths, 1000.0, None), "the new catalog is complete");
        for typed in ["vesta", "vestal", "VESTAL water"] {
            assert!(search_finds(&water.search_text(), typed), "{typed:?}");
        }
        set_thread_language(Choice::De);
        assert!(search_finds(&water.search_text(), "getränk"), "{}", water.search_text());
        set_thread_language(Choice::Auto);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_price_file_keeps_the_newest_four_hundred_parts() {
        let mut book = Shops::default();
        for i in 0..405 {
            book.entries.set(&format!("u-{i}"), (f64::from(i), Vec::new()));
        }
        trim(&mut book);
        assert_eq!(book.entries.len(), MAX_ITEMS);
        assert!(!book.known("u-4") && book.known("u-5") && book.known("u-404"));
    }

    #[test]
    fn names_of_sections_and_groups_have_text_keys() {
        for section in SECTIONS.into_iter().chain(GOODS_SECTIONS) {
            assert!(section_key(section).is_some(), "{section}");
        }
        assert!(!held_section("Commodities") && !held_section("Vehicles") && held_section("Clothing"));
        assert_eq!(group_key("Coolers"), Some("s_uk_coolers"));
        assert_eq!(group_key("Nothing"), None);
    }

    #[test]
    fn the_place_comes_from_the_first_field_that_is_set() {
        let row =
            jsonio::loads(r#"{"space_station_name": "", "city_name": " Lorville ", "outpost_name": "X"}"#).unwrap();
        assert_eq!(place_of(&row), "Lorville");
        // A field of blanks is set; the strip comes after the choice.
        let blank = jsonio::loads(r#"{"space_station_name": " ", "city_name": "Lorville"}"#).unwrap();
        assert_eq!(place_of(&blank), "");
    }
}
