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

//! The game values of a real build: the M5A Cannon S3 as Erkul shows it,
//! and the corrections VerseKit adds.
//!
//! Run with `cargo test -p versekit-gamedata --release --test values_real --
//! --ignored --nocapture`. Reading the archive over the network is slow:
//! with `VERSEKIT_DATAFORGE` and `VERSEKIT_GLOBAL_INI` the DataForge and the
//! English `global.ini` kept from it are read instead
//! (`GameData::dataforge_bytes`, `GameData::english_ini_bytes`); otherwise
//! `VERSEKIT_CHANNEL_FOLDER` or the installed game is opened. Written for
//! build 4.10.2-live.12913470 (4.10.197).

use versekit_gamedata::corrections::{self, Outcome};
use versekit_gamedata::values::{GameValues, Stat};
use versekit_gamedata::{GameData, Language, Localization, quality, value_source, values_from_dataforge};

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

fn close(value: Option<f64>, expected: f64) -> bool {
    value.is_some_and(|value| (value - expected).abs() < 1e-6 * expected.abs().max(1.0))
}

#[test]
#[ignore = "reads the game data"]
fn the_m5a_cannon_matches_erkul() {
    let values = values();
    println!("{} recipes, {} items, {} not readable", values.recipes.len(), values.items.len(), values.failed.len());
    let recipe = values.recipe("BP_CRAFT_BEHR_LaserCannon_S3").unwrap();
    let slots = &recipe.tiers[0].slots;
    let names: Vec<&str> = slots.iter().map(|slot| slot.name.as_str()).collect();
    assert_eq!(names, ["Frame", "Emitter", "Aperture Iris"]);
    assert_eq!(slots[0].modifiers[0].property_key, "health_maxhealth");
    assert_eq!((slots[0].modifiers[0].at_start, slots[0].modifiers[0].at_end), (0.9, 1.1));
    assert_eq!(slots[1].modifiers[0].property_key, "weapon_damage");
    assert_eq!((slots[2].modifiers[0].at_start, slots[2].modifiers[0].at_end), (0.95, 1.05));
    let item = values.product(recipe).unwrap();
    assert_eq!(item.name.as_deref(), Some("M5A Cannon"));
    let base = quality::summary(item);
    assert!(close(base.alpha, 410.184), "{base:?}");
    assert!(close(base.fire_rate, 100.0));
    assert!(close(base.range, 1184.0 * 2.03));
    assert!(close(base.health, 1024.0));
    let best = quality::summary(&quality::craft(item, recipe, |_, _| Some(1000.0)).made);
    assert!(close(best.alpha, 410.184 * 1.1));
    assert!(close(best.dps, 100.0 / 60.0 * 410.184 * 1.1));
    assert!(close(best.health, 1024.0 * 1.1));
    assert!(close(best.fire_rate, 100.0));
    assert!(close(best.range, 1184.0 * 2.03));
    // Bought parts and the factory loadout are read as well.
    let shields = values.items.values().filter(|item| item.stat(Stat::ShieldHealth).is_some()).count();
    let guns = values.items.values().filter(|item| item.attach_type.as_deref() == Some("WeaponGun")).count();
    println!("shields {shields}, ship weapons {guns}");
    assert!(shields > 61 && guns > 96);
}

#[test]
#[ignore = "reads the game data"]
fn the_corrections_of_versekit_apply() {
    let mut values = values();
    let report = corrections::apply(&mut values, &corrections::built_in());
    assert_eq!(report.outcomes[0].1, Outcome::Applied, "Polaris shield");
    assert_eq!(report.outcomes[1].1, Outcome::Marked, "Pioneer cooler");
    let polaris = values.recipe("BP_CRAFT_SHLD_RSI_S04_Polaris_SCItem").expect("the Polaris shield blueprint");
    let shield = values.product(polaris).unwrap_or_else(|| {
        panic!("the meant product {:?} of the Polaris blueprint is no item of the game data", polaris.product)
    });
    assert_eq!(shield.class_name, "SHLD_RSI_S04_Polaris_SCItem");
    assert!(shield.stat(Stat::ShieldHealth).is_some());
    let pioneer = values.recipe("BP_CRAFT_COOL_S04_CNOU_Pioneer").unwrap();
    assert!(pioneer.product_missing);
    assert_eq!(pioneer.corrected[0].raw, "3cf6753c-d69c-4980-a45e-724151407096");
}

#[test]
#[ignore = "reads the game channel and fetches from the network"]
fn the_archive_comes_first_and_scmdb_checks_it() {
    let folder = std::env::var_os("VERSEKIT_CHANNEL_FOLDER").expect("VERSEKIT_CHANNEL_FOLDER");
    let loaded = value_source::load(folder, "VerseKit/5.0.0-dev (+https://versekit.xharig.com)").unwrap();
    assert_eq!(loaded.source, value_source::Source::Archive);
    assert!(loaded.scmdb_error.is_none(), "{:?}", loaded.scmdb_error);
    let check = loaded.cross_check.unwrap();
    println!("{} products compared, {} differences: {}", check.products, check.differences(), check.summary());
    assert!(check.products > 1600);
    assert_eq!(check.differences(), 0, "{}", check.summary());
}

#[test]
#[ignore = "fetches from the network"]
fn the_scmdb_error_list_is_fetched() {
    let list =
        corrections::fetch_scmdb("4.10.2-live.12913470", "VerseKit/5.0.0-dev (+https://versekit.xharig.com)").unwrap();
    println!("{} corrections", list.len());
    assert!(list.len() >= 30);
    assert!(list.iter().all(|correction| correction.source == "scmdb"));
}
