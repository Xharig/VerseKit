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

//! The stored files the writing reads, in the formats 4.12 left them: the
//! catalog's contracts, the collection, the shop prices, the recipes, the
//! original names. Each loader gives an empty result for a missing or
//! unusable file, never an error.

use std::collections::BTreeSet;

use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

use super::item_info::{self, Item, Shops};
use super::{Inputs, ranks, reputation};

/// The catalog of 4.12 (`katalog-cache.json`) and of v5 (`katalog-spieldaten.json`).
pub const CATALOG_412: &str = "katalog-cache.json";
pub const CATALOG_V5: &str = "katalog-spieldaten.json";
pub const COLLECTION_FILE: &str = crate::collection::FILE_NAME;
pub const SHOPS_FILE: &str = "laeden-katalog.json";
pub const SHOPS_FORMAT: i64 = 7;
pub const RECIPES_FILE: &str = crate::recipe_file::FILE;
pub const RECIPES_FORMAT: i64 = crate::recipe_file::FORMAT;
pub const ISSUES_FILE: &str = crate::recipe_file::ISSUES_FILE;
pub const NAMES_FILE: &str = crate::asop::NAMES_FILE;

fn read(path: &str) -> Option<Value> {
    jsonio::read_json(path).ok()
}

/// The contracts (`missionen`) of a catalog file, in file order.
pub fn missions(path: &str) -> Map {
    missions_of(read(path).as_ref())
}

fn missions_of(data: Option<&Value>) -> Map {
    match data {
        Some(data) if matches!(data.get("bauplaene"), Some(Value::Object(_))) => {
            data.get("missionen").and_then(Value::as_object).cloned().unwrap_or_default()
        }
        _ => Map::new(),
    }
}

/// The keys of the own blueprints (`collection.keys`) from `bestand.json`.
pub fn owned(paths: &Paths) -> BTreeSet<String> {
    match read(&paths.app_file(COLLECTION_FILE)).as_ref().and_then(|d| d.get("bauplaene")) {
        Some(Value::Object(map)) => map.iter().map(|(k, _)| k.clone()).collect(),
        _ => BTreeSet::new(),
    }
}

/// The stored English names (`gametext.saved_names`).
pub fn original_names(paths: &Paths) -> Vec<(String, String)> {
    match read(&paths.app_file(NAMES_FILE)) {
        Some(Value::Object(map)) => map.into_iter().map(|(k, v)| (k, super::texts::py_str(&v))).collect(),
        _ => Vec::new(),
    }
}

/// The cheapest shops (`shops.cheapest_by_entity`); empty without a
/// catalog of the right format.
pub fn shops(paths: &Paths) -> Shops {
    match read(&paths.app_file(SHOPS_FILE)) {
        Some(data) if data.get("format") == Some(&Value::int(SHOPS_FORMAT)) => item_info::shops_from_catalog(&data),
        _ => Shops::default(),
    }
}

/// The craftable items from the recipe file (`recipe_file`), with the
/// error list of the same build applied (`crafting.load`).
pub fn items(paths: &Paths, owned: &BTreeSet<String>) -> Vec<Item> {
    let Some(data) = read(&paths.app_file(RECIPES_FILE)) else { return Vec::new() };
    if data.get("format") != Some(&Value::int(RECIPES_FORMAT)) {
        return item_info::items_from_412(&Value::Object(Map::new()), None, owned);
    }
    let build = data.get("build").and_then(Value::as_str).unwrap_or("");
    let issues = read(&paths.app_file(ISSUES_FILE))
        .filter(|i| !build.is_empty() && i.get("build").and_then(Value::as_str) == Some(build))
        .and_then(|i| i.get("daten").cloned());
    item_info::items_from_412(&data, issues.as_ref(), owned)
}

/// Everything from the stored files, as 4.12 would read it, and the text keys
/// of the crafted items where the catalog holds them. `catalog` is the catalog
/// file to take the contracts from.
pub fn inputs(paths: &Paths, catalog: &str, version: &str) -> Inputs {
    let owned = owned(paths);
    let catalog = read(catalog);
    Inputs {
        missions: missions_of(catalog.as_ref()),
        item_texts: catalog.as_ref().map(item_info::item_texts_from_catalog).unwrap_or_default(),
        items: items(paths, &owned),
        owned,
        reputation: reputation::load(paths),
        ranks: ranks::load(paths),
        shops: shops(paths),
        original_names: original_names(paths),
        version: version.to_string(),
    }
}
