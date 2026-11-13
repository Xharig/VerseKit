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

//! Reads a small synthetic `Data.p4k` written under the target folder.

mod common;

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use common::channel;
use versekit_gamedata::corrections::{Correction, Field, Outcome, Section};
use versekit_gamedata::{Error, GameData, Language, Source, SourceKind, Text};

fn key_text(key: &str, text: &str) -> Text {
    Text { key: Some(key.to_string()), text: Some(text.to_string()) }
}

#[test]
fn blueprints_with_item_and_sources() {
    let folder = channel("blueprints");
    let game = GameData::open(&folder).unwrap();
    assert_eq!(game.version(), Some("4.12.150.1"));
    assert_eq!(game.branch(), Some("sc-alpha-4.12.0"));
    let texts = game.localization(Language::English).unwrap();
    let catalog = game.blueprints(&texts).unwrap();

    let names: Vec<&str> = catalog.blueprints.iter().map(|blueprint| blueprint.class_name.as_str()).collect();
    assert_eq!(
        names,
        ["BP_CRAFT_behr_rifle", "BP_CRAFT_lonely", "BP_CRAFT_s38_pistol"],
        "leftover and dismantle blueprints stay out"
    );
    let defaults: Vec<bool> = catalog.blueprints.iter().map(|blueprint| blueprint.default).collect();
    assert_eq!(defaults, [false, false, true], "the pistol is in the default blueprints of the crafting globals");
    assert_eq!(catalog.left_out, 1);
    assert_eq!((catalog.pools, catalog.in_pools, catalog.scenario_tiers), (1, 1, 1));
    assert_eq!((catalog.contracts, catalog.unreleased_contracts), (2, 1));
    assert!(catalog.failed.is_empty(), "{:?}", catalog.failed);

    let rifle = &catalog.blueprints[0];
    assert_eq!(rifle.name, key_text("@item_Namebehr_rifle", "P8-AR Rifle"), "item name stands in for a placeholder");
    let item = rifle.item.as_ref().expect("the rifle blueprint makes an item");
    assert_eq!(item.class_name, "behr_rifle");
    assert_eq!((item.kind.as_deref(), item.sub_kind.as_deref()), (Some("WeaponPersonal"), Some("Rifle")));
    assert_eq!((item.size, item.grade), (Some(3), Some(2)));
    assert_eq!(item.class.as_deref(), Some("Military"));
    let maker = item.manufacturer.as_ref().expect("the rifle has a manufacturer");
    assert_eq!((maker.code.as_deref(), maker.name.shown()), (Some("BEH"), "Behring"));

    // The title fills in the fixed choice and the organisation's name and
    // names the place the game picks; the reputation adds the faction and
    // its affinity on success, the loss on failure stays out.
    let contract = Source {
        kind: SourceKind::Contract,
        id: "Hunt_Easy".to_string(),
        record_path: "libs/foundry/records/contracts/contractgenerator/highpoint_hunt".to_string(),
        mission: key_text(
            "@Hunt_Title",
            "Hunt the ~mission(Creature) at ~mission(Location) for ~mission(Contractor|Name) ",
        ),
        title: "Hunt the Kopion at [LOCATION] for Highpoint Org".to_string(),
        mission_type: Some(key_text("@MissionType_Hunting", "Hunting")),
        contractor: Some(key_text("@Highpoint_from", "Highpoint")),
        faction: Some(key_text("@Highpoint_RepUI_Name", "Highpoint Wilderness Specialists")),
        description_key: None,
        systems: Vec::new(),
        places: vec![key_text("@Stanton1", "Hurston")],
        rank: Some(key_text("@Rank_Two", "Rank Two")),
        min_reputation: Some(2500),
        max_rank: Some(key_text("@Rank_Top", "Elite")),
        max_reputation: Some(95250),
        shareable: Some(true),
        cooldown: Some(45),
        reputation_gain: Some(200),
        min_points: None,
        reward_uec: Some(12000),
        chance: Some(0.5),
        pool: "BP_MISSIONREWARD_rifles".to_string(),
    };
    // The sub contract is a source of its own: its title, its mission type,
    // its own place instead of the handler's; a target without a property is
    // named in brackets, a placeholder nothing fills stays.
    let sub = Source {
        mission: key_text("@Hunt_Sub_Title", "Hunt ~mission(TargetName) in the ~mission(Missing)"),
        title: "Hunt [TARGET] in the ~mission(Missing)".to_string(),
        mission_type: Some(key_text("@MissionType_Special", "Special hunt")),
        places: vec![key_text("@Stanton2", "Crusader")],
        ..contract.clone()
    };
    // Without a title of its own the template's title stands in.
    let untitled = Source {
        id: "Hunt_Untitled".to_string(),
        mission: key_text("@Template_Title", "Template title"),
        title: "Template title".to_string(),
        reputation_gain: None,
        reward_uec: None,
        chance: Some(1.0),
        shareable: Some(false),
        ..contract.clone()
    };
    let scenario = Source {
        kind: SourceKind::Scenario,
        id: "xeno_scenarioprogress".to_string(),
        record_path: "libs/foundry/records/contracts/contractscenarios/xeno".to_string(),
        mission: key_text("@Scenario_Progress", "Total contribution"),
        title: "Total contribution".to_string(),
        mission_type: None,
        contractor: None,
        faction: Some(key_text("@Foxwell_Name", "Foxwell Enforcement")),
        description_key: None,
        systems: Vec::new(),
        places: Vec::new(),
        rank: None,
        min_reputation: None,
        max_rank: None,
        max_reputation: None,
        shareable: None,
        cooldown: None,
        reputation_gain: None,
        min_points: Some(18000),
        reward_uec: None,
        chance: None,
        pool: "BP_MISSIONREWARD_rifles".to_string(),
    };
    assert_eq!(rifle.sources, [sub, contract, untitled, scenario], "the unreleased contract stays out");

    let lonely = &catalog.blueprints[1];
    assert_eq!((lonely.item.as_ref(), lonely.sources.len()), (None, 0));
    assert_eq!(lonely.name.shown(), "BP_CRAFT_lonely", "a blueprint without any name shows its class name");
    assert_eq!(lonely.name.key.as_deref(), Some("@LOC_PLACEHOLDER"));
}

fn fix(section: Section, record: &str, field: Field, expected: &str) -> Correction {
    Correction {
        section,
        record: record.to_string(),
        field,
        raw: None,
        expected: Some(expected.to_string()),
        missing_target: false,
        reason: "test".to_string(),
        build: None,
        source: "scmdb",
    }
}

/// The known errors of the data apply while the catalog is read: the name
/// of a blueprint, the kind of an item, the maker of a manufacturer record
/// and the faction of a contract.
#[test]
fn corrections_apply_while_reading() {
    let folder = channel("corrected");
    let game = GameData::open(&folder).unwrap();
    let texts = game.localization(Language::English).unwrap();
    let corrections = [
        fix(Section::Blueprint, "10000000-0000-4000-8000-000000000003", Field::Name, "Meant Rifle"),
        fix(Section::Entity, "10000000-0000-4000-8000-000000000002", Field::AttachType, "WeaponGun"),
        fix(Section::Manufacturer, "10000000-0000-4000-8000-000000000001", Field::Manufacturer, "Meant Maker"),
        fix(Section::Contract, "Hunt_Easy", Field::Faction, "10000000-0000-4000-8000-000000000009"),
        fix(Section::Blueprint, "00000000-0000-4000-8000-000000000000", Field::Name, "Nobody"),
    ];
    let catalog = game.blueprints_corrected(&texts, &corrections).unwrap();
    let rifle = &catalog.blueprints[0];
    assert_eq!(rifle.name.shown(), "Meant Rifle");
    let item = rifle.item.as_ref().unwrap();
    assert_eq!(item.kind.as_deref(), Some("WeaponGun"));
    assert_eq!(item.manufacturer.as_ref().map(|maker| maker.name.shown()), Some("Meant Maker"));
    let hunt = rifle.sources.iter().find(|s| s.id == "Hunt_Easy" && s.mission.key.as_deref() == Some("@Hunt_Title"));
    assert_eq!(hunt.and_then(|s| s.faction.as_ref()).map(Text::shown), Some("Rank Two"));
    let applied: Vec<Outcome> = catalog.corrections.outcomes.iter().map(|(_, outcome)| *outcome).collect();
    assert_eq!(
        applied,
        [Outcome::Applied, Outcome::Applied, Outcome::Applied, Outcome::Applied, Outcome::NotFound],
        "the correction for a record that is not there is not found"
    );
    // The same archive read without them is as it was.
    let plain = game.blueprints(&texts).unwrap();
    assert_eq!(plain.blueprints[0].name.shown(), "P8-AR Rifle");
    assert_eq!(plain.blueprints[0].item.as_ref().unwrap().kind.as_deref(), Some("WeaponPersonal"));
}

#[test]
fn german_texts_fall_back_to_english() {
    let folder = channel("german");
    let game = GameData::open(&folder).unwrap();
    let texts = game.localization(Language::German).unwrap();
    assert!(texts.has_own_texts());
    assert_eq!(texts.get("@item_Namebehr_rifle").as_deref(), Some("P8-AR-Gewehr"));
    assert_eq!(texts.get("manufacturer_namebehr").as_deref(), Some("Behring"), "English fallback, any case");
    assert_eq!(texts.get("@LOC_PLACEHOLDER"), None);
    assert_eq!(texts.get("@nowhere"), None);

    let catalog = game.blueprints(&texts).unwrap();
    let rifle = &catalog.blueprints[0];
    let item = rifle.item.as_ref().unwrap();
    assert_eq!(item.name.shown(), "P8-AR-Gewehr");
    assert_eq!(item.class.as_deref(), Some("Military"), "the class comes from the English description");
    let hunt = rifle.sources.iter().find(|s| s.mission.key.as_deref() == Some("@Hunt_Title")).unwrap();
    assert_eq!(hunt.mission.shown(), "Jagd auf den Kopion");
    assert_eq!(hunt.title, "Jagd auf den Kopion");
    assert_eq!(hunt.rank.as_ref().map(Text::shown), Some("Rank Two"));

    let japanese = game.localization(Language::Japanese).unwrap();
    assert!(!japanese.has_own_texts());
    assert_eq!(japanese.get("Creature").as_deref(), Some("Kopion"));
}

#[test]
fn reading_leaves_the_channel_folder_unchanged() {
    let folder = channel("unchanged");
    let before = snapshot(&folder);
    let game = GameData::open(&folder).unwrap();
    let texts = game.localization(Language::German).unwrap();
    game.blueprints(&texts).unwrap();
    drop(game);
    assert_eq!(snapshot(&folder), before);
}

#[test]
fn missing_archive_is_an_error() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("versekit-gamedata").join("no-archive");
    fs::create_dir_all(&folder).unwrap();
    assert!(matches!(GameData::open(&folder), Err(Error::Archive { .. })));
}

fn snapshot(folder: &Path) -> Vec<(String, Vec<u8>, SystemTime)> {
    let mut files: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let modified = entry.metadata().unwrap().modified().unwrap();
            (entry.file_name().to_string_lossy().into_owned(), fs::read(entry.path()).unwrap(), modified)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}
