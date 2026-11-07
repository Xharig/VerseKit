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

//! The writing of the recipe file against a stand-in for scmdb, in a throwaway data folder, and its three readers
//! on what it wrote.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::testdir::Dir;

const BUILD: &str = "4.3.2-live.123";

/// Two recipes: a rifle out of iron and a part, a cooler out of gold that names the product it was meant to make.
const BLUEPRINTS: &str = r#"{"blueprints": [
    {"tag": "BP_RIFLE", "productName": "Atzkav Sniper Rifle", "productEntityClass": "rifle_01", "guid": "g1",
     "tiers": [{"craftTimeSeconds": 60, "slots": [{"options": [
        {"type": "resource", "resourceName": "Iron", "quantity": 0.5, "minQuality": 100},
        {"type": "item", "itemName": "Barrel Part", "quantity": 1}]}]}]},
    {"tag": "BP_COOLER", "productName": "Frostline", "productEntityClass": "wrong_01",
     "suggestedProductEntityClass": "cooler_01", "guid": "g2",
     "tiers": [{"craftTimeSeconds": 30, "slots": [{"options": [
        {"type": "resource", "resourceName": "Gold", "quantity": 0.2}]}]}]}],
  "dismantle": {"Lindinium": false}}"#;

const ITEMS: &str = r#"{"items": [
    {"entityClass": "rifle_01", "itemType": "weapon", "tags": ["x"], "fireModesIndex": 1, "ammoIndex": 0},
    {"entityClass": "cooler_01", "itemType": "shipcomponent", "coolingRate": 12.5, "knownIssue": "y"},
    {"entityClass": "unrelated_01", "itemType": "weapon"}],
  "fireModesPools": [[{"name": "single"}], [{"name": "auto", "fireRate": 600}]],
  "ammoPools": [{"speed": 900}]}"#;

/// scmdb with the files of [`BUILD`] and what was asked for, by name.
#[derive(Default)]
struct FakeScmdb {
    build: Option<&'static str>,
    blueprints: Option<&'static str>,
    items: Option<&'static str>,
    issues: Option<&'static str>,
    asked: Mutex<Vec<String>>,
}

/// The known data errors of [`BUILD`]: the rifle's recipe makes another item under another name.
const ISSUES: &str = r#"{"blueprintRecords": {"_comment": "x",
    "G1": {"expected": {"entityClass": "rifle_02", "name": "Atzkav Sniper Rifle Mk2"}, "note": "swapped"}}}"#;

impl FakeScmdb {
    fn full() -> FakeScmdb {
        FakeScmdb { build: Some(BUILD), blueprints: Some(BLUEPRINTS), items: Some(ITEMS), ..FakeScmdb::default() }
    }

    fn asked_for(&self, part: &str) -> usize {
        self.asked.lock().unwrap().iter().filter(|name| name.contains(part)).count()
    }
}

impl Supply for FakeScmdb {
    fn file(&self, name: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(name.to_string());
        let found = if name == "versions.json" {
            self.build.map(|build| format!(r#"[{{"version": "4.4.0-ptu.9"}}, {{"version": "{build}"}}]"#))
        } else if name == format!("crafting_blueprints-{BUILD}.json") {
            self.blueprints.map(str::to_string)
        } else if name == format!("crafting_items-{BUILD}.json") {
            self.items.map(str::to_string)
        } else if name == issues_source(BUILD) {
            self.issues.map(str::to_string)
        } else {
            None
        };
        found.map(String::into_bytes).ok_or_else(|| format!("{name}: missing"))
    }
}

fn paths_in(dir: &Dir, offline: bool) -> Paths {
    let root = dir.path().to_string_lossy().into_owned();
    let data = format!("{root}/data");
    std::fs::create_dir_all(&data).unwrap();
    let mut vars = vec![("HOME", root.clone()), ("SC_BP_HOME", data)];
    if offline {
        vars.push(("SC_BP_NO_NET", "1".to_string()));
    }
    let vars: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Paths::new(Host::new(Flavor::Posix, &vars, &root))
}

fn written(paths: &Paths) -> Value {
    jsonio::read_json(&paths.app_file(FILE)).expect("the file is there")
}

#[test]
fn the_build_is_the_first_one_that_is_no_ptu() {
    assert_eq!(current_build(&FakeScmdb::full()), Ok(BUILD.to_string()));
    assert!(current_build(&FakeScmdb::default()).is_err(), "no list, no build");
}

#[test]
fn a_new_build_writes_recipes_dismantling_and_the_products_of_the_recipes() {
    let dir = Dir::new("rezepte-neu");
    let paths = paths_in(&dir, false);
    assert_eq!(update(&paths, &FakeScmdb::full()), Ok(Updated::Written(2)));
    let data = written(&paths);
    assert_eq!(data.get("format"), Some(&Value::int(FORMAT)));
    assert_eq!(data.get("build"), Some(&Value::str(BUILD)));
    assert_eq!(list(data.get("blueprints")).len(), 2);
    assert_eq!(data.get("dismantle").and_then(|d| d.get("Lindinium")), Some(&Value::Bool(false)));
    let products = data.get("products").and_then(Value::as_object).expect("products");
    let keys: Vec<&str> = products.iter().map(|(key, _)| key.as_str()).collect();
    // The product of the rifle and the one the cooler was meant to make; nothing that no recipe makes.
    assert_eq!(keys, ["rifle_01", "cooler_01"]);
    let rifle = data.get("products").and_then(|p| p.get("rifle_01")).expect("rifle");
    assert!(rifle.get("tags").is_none() && rifle.get("fireModesIndex").is_none(), "{rifle:?}");
    assert_eq!(
        rifle.get("fireModes").and_then(|m| list(Some(m)).first()).and_then(|m| m.get("name")),
        Some(&Value::str("auto"))
    );
    assert_eq!(rifle.get("ammo").and_then(|a| a.get("speed")), Some(&Value::int(900)));
    let cooler = data.get("products").and_then(|p| p.get("cooler_01")).expect("cooler");
    assert!(cooler.get("knownIssue").is_none(), "{cooler:?}");
}

#[test]
fn the_three_readers_take_the_written_file() {
    let dir = Dir::new("rezepte-leser");
    let paths = paths_in(&dir, false);
    update(&paths, &FakeScmdb::full()).unwrap();
    // The notes in the contract texts: the items with their recipe.
    let items = crate::injection::sources::items(&paths, &BTreeSet::new());
    let tags: Vec<&str> = items.iter().map(|item| item.tag.as_str()).collect();
    assert!(tags.contains(&"BP_RIFLE") && tags.contains(&"BP_COOLER"), "{tags:?}");
    assert!(items.iter().all(|item| item.first_tier.is_some()), "every item has its recipe");
    // The materials of the stock.
    let stock = crate::stock_data::StockData::load(|name| paths.app_file(name));
    assert_eq!(stock.material_names(), ["Barrel Part", "Gold", "Iron"]);
    assert!(stock.is_piece("Barrel Part"));
    // The tags for the Basetool.
    let unique: HashMap<String, String> = crate::export::unique_tags(&paths);
    assert!(!unique.is_empty(), "the tags come from the recipes");
}

#[test]
fn the_same_build_fetches_nothing_but_the_list_of_builds() {
    let dir = Dir::new("rezepte-gleich");
    let paths = paths_in(&dir, false);
    let scmdb = FakeScmdb::full();
    update(&paths, &scmdb).unwrap();
    assert_eq!(scmdb.asked_for("crafting_"), 2, "recipes and items once");
    assert_eq!(update(&paths, &scmdb), Ok(Updated::Current(2)));
    assert_eq!(scmdb.asked_for("crafting_"), 2, "nothing fetched again");
    assert_eq!(scmdb.asked_for("versions.json"), 2);
}

#[test]
fn a_file_without_dismantling_or_products_is_fetched_once_more() {
    let dir = Dir::new("rezepte-nachholen");
    let paths = paths_in(&dir, false);
    for older in [
        format!(r#"{{"format": 1, "build": "{BUILD}", "blueprints": [{{"tag": "A"}}]}}"#),
        format!(
            r#"{{"format": 1, "build": "{BUILD}", "blueprints": [{{"tag": "A"}}], "dismantle": {{}}, "products": {{}}}}"#
        ),
    ] {
        std::fs::write(paths.app_file(FILE), older).unwrap();
        assert_eq!(update(&paths, &FakeScmdb::full()), Ok(Updated::Written(2)));
    }
}

#[test]
fn a_build_without_products_counts_as_current_until_the_next_build() {
    let dir = Dir::new("rezepte-ohne-produkte");
    let paths = paths_in(&dir, false);
    let scmdb = FakeScmdb { items: None, ..FakeScmdb::full() };
    assert_eq!(update(&paths, &scmdb), Ok(Updated::Written(2)));
    assert_eq!(written(&paths).get("products"), Some(&Value::Object(Map::new())));
    assert_eq!(written(&paths).get(WITHOUT_PRODUCTS), Some(&Value::str(BUILD)));
    let file = std::fs::read(paths.app_file(FILE)).unwrap();
    // Every later look: nothing fetched, nothing written (the notes in the game's texts stay as they are).
    assert_eq!(update(&paths, &scmdb), Ok(Updated::Current(2)));
    assert_eq!(update(&paths, &FakeScmdb::full()), Ok(Updated::Current(2)));
    assert_eq!(scmdb.asked_for("crafting_"), 2, "recipes and items only once");
    assert_eq!(std::fs::read(paths.app_file(FILE)).unwrap(), file);
    // A mark of another build does not count: a new build is fetched whole.
    let other = r#"{"format": 1, "build": "4.0.0-live.1", "blueprints": [{"tag": "A"}], "dismantle": {}, "products": {}, "ohne_produkte": "4.0.0-live.1"}"#;
    std::fs::write(paths.app_file(FILE), other).unwrap();
    assert_eq!(update(&paths, &FakeScmdb::full()), Ok(Updated::Written(2)));
    assert!(written(&paths).get(WITHOUT_PRODUCTS).is_none(), "with products there is no mark");
}

#[test]
fn a_failed_fetch_of_the_products_keeps_those_of_the_older_build() {
    let dir = Dir::new("rezepte-produkte-behalten");
    let paths = paths_in(&dir, false);
    let older = r#"{"format": 1, "build": "4.0.0-live.1", "blueprints": [{"tag": "OLD"}], "dismantle": {}, "products": {"rifle_01": {"itemType": "weapon"}}}"#;
    std::fs::write(paths.app_file(FILE), older).unwrap();
    assert_eq!(update(&paths, &FakeScmdb { items: None, ..FakeScmdb::full() }), Ok(Updated::Written(2)));
    let data = written(&paths);
    assert_eq!(data.get("build"), Some(&Value::str(BUILD)), "the recipes of the new build");
    assert_eq!(
        data.get("products").and_then(|p| p.get("rifle_01")).and_then(|r| r.get("itemType")),
        Some(&Value::str("weapon")),
        "the products of the older build are not overwritten with nothing"
    );
}

#[test]
fn a_failed_fetch_leaves_the_file_as_it_was() {
    let dir = Dir::new("rezepte-fehler");
    let paths = paths_in(&dir, false);
    let before = r#"{"format": 1, "build": "4.0.0-live.1", "blueprints": [{"tag": "OLD"}]}"#;
    std::fs::write(paths.app_file(FILE), before).unwrap();
    for broken in [
        FakeScmdb { blueprints: None, ..FakeScmdb::full() },
        FakeScmdb { blueprints: Some(r#"{"blueprints": []}"#), ..FakeScmdb::full() },
        FakeScmdb { blueprints: Some("kein json"), ..FakeScmdb::full() },
        FakeScmdb::default(),
    ] {
        assert!(update(&paths, &broken).is_err());
        assert_eq!(std::fs::read_to_string(paths.app_file(FILE)).unwrap(), before);
    }
    let leftovers: Vec<_> = std::fs::read_dir(paths.app_folder())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "nothing half written: {leftovers:?}");
}

/// The item the notes name for the recipe `tag`: (product name, entity class).
fn item_of(paths: &Paths, tag: &str) -> Option<(String, String)> {
    crate::injection::sources::items(paths, &BTreeSet::new())
        .into_iter()
        .find(|item| item.tag == tag)
        .map(|item| (item.base, item.entity))
}

#[test]
fn the_list_of_data_errors_comes_with_the_recipes_and_the_notes_correct_with_it() {
    let dir = Dir::new("rezepte-datenfehler");
    let paths = paths_in(&dir, false);
    update(&paths, &FakeScmdb { issues: Some(ISSUES), ..FakeScmdb::full() }).unwrap();
    let kept = jsonio::read_json(&paths.app_file(ISSUES_FILE)).expect("the list is kept");
    assert_eq!(kept.get("format"), Some(&Value::int(ISSUES_FORMAT)));
    assert_eq!(kept.get("build"), Some(&Value::str(BUILD)));
    assert!(kept.get("daten").and_then(|d| d.get("blueprintRecords")).is_some(), "{kept:?}");
    assert_eq!(
        item_of(&paths, "BP_RIFLE"),
        Some(("Atzkav Sniper Rifle Mk2".to_string(), "rifle_02".to_string())),
        "the reader of the notes applies the correction"
    );
    // The same recipes without a list for the build: nothing is corrected.
    let plain = Dir::new("rezepte-ohne-datenfehler");
    let plain = paths_in(&plain, false);
    update(&plain, &FakeScmdb::full()).unwrap();
    assert_eq!(item_of(&plain, "BP_RIFLE"), Some(("Atzkav Sniper Rifle".to_string(), "rifle_01".to_string())));
}

#[test]
fn a_missing_list_is_asked_for_again_after_six_hours_and_a_new_one_corrects_current_recipes() {
    let dir = Dir::new("rezepte-datenfehler-spaeter");
    let paths = paths_in(&dir, false);
    let scmdb = FakeScmdb::full();
    let now = 1_800_000_000.0;
    assert!(!update_issues(&paths, &scmdb, BUILD, now), "no list: nothing changed");
    let kept = jsonio::read_json(&paths.app_file(ISSUES_FILE)).expect("an empty list is kept");
    assert_eq!(kept.get("daten"), Some(&Value::Object(Map::new())));
    assert!(!update_issues(&paths, &scmdb, BUILD, now + 3600.0));
    assert_eq!(scmdb.asked_for("cig_data_issues-"), 1, "not again within six hours");
    let later = FakeScmdb { issues: Some(ISSUES), ..FakeScmdb::full() };
    assert!(update_issues(&paths, &later, BUILD, now + ISSUES_RETRY_SECONDS), "the list came: a change");
    assert!(!update_issues(&paths, &later, BUILD, now + 3.0 * ISSUES_RETRY_SECONDS), "a list with entries stays");
    assert_eq!(later.asked_for("cig_data_issues-"), 1);
    // Recipes current, a new list: the readers correct anew.
    let fresh = Dir::new("rezepte-datenfehler-neu");
    let fresh = paths_in(&fresh, false);
    assert_eq!(update(&fresh, &FakeScmdb::full()), Ok(Updated::Written(2)));
    std::fs::write(
        fresh.app_file(ISSUES_FILE),
        format!(r#"{{"format": 1, "build": "{BUILD}", "geholt": 0, "daten": {{}}}}"#),
    )
    .unwrap();
    assert_eq!(update(&fresh, &FakeScmdb { issues: Some(ISSUES), ..FakeScmdb::full() }), Ok(Updated::Corrected(2)));
    assert_eq!(update(&fresh, &FakeScmdb { issues: Some(ISSUES), ..FakeScmdb::full() }), Ok(Updated::Current(2)));
}

#[test]
fn a_kept_list_stays_for_its_build_and_one_of_another_build_is_replaced() {
    let dir = Dir::new("rezepte-datenfehler-behalten");
    let paths = paths_in(&dir, false);
    let old = format!(r#"{{"format": 1, "build": "{BUILD}", "geholt": 0, "daten": {{"entities": {{}}}}}}"#);
    std::fs::write(paths.app_file(ISSUES_FILE), &old).unwrap();
    let scmdb = FakeScmdb { issues: Some(ISSUES), ..FakeScmdb::full() };
    assert!(!update_issues(&paths, &scmdb, BUILD, 1_800_000_000.0), "a list of the build stays as it is");
    assert_eq!(scmdb.asked_for("cig_data_issues-"), 0);
    assert_eq!(std::fs::read_to_string(paths.app_file(ISSUES_FILE)).unwrap(), old);
    // A list of another build does not count: the new build starts without entries.
    assert!(!update_issues(&paths, &FakeScmdb::full(), "4.4.0-live.1", 1_800_000_000.0));
    let other = jsonio::read_json(&paths.app_file(ISSUES_FILE)).unwrap();
    assert_eq!(other.get("build"), Some(&Value::str("4.4.0-live.1")));
    assert_eq!(other.get("daten"), Some(&Value::Object(Map::new())));
}

#[test]
fn without_the_net_nothing_is_asked() {
    let dir = Dir::new("rezepte-offline");
    let paths = paths_in(&dir, true);
    let scmdb = FakeScmdb::full();
    assert!(update(&paths, &scmdb).is_err());
    assert!(scmdb.asked.lock().unwrap().is_empty());
    assert!(!std::path::Path::new(&paths.app_file(FILE)).exists());
}
