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

//! Reads the installed game's `Data.p4k`, prints counts and checks
//! properties every blueprint and source must have.
//!
//! Run with `cargo test -p versekit-gamedata --release --test real_archive --
//! --ignored --nocapture`. Without `VERSEKIT_CHANNEL_FOLDER` the game folder
//! comes from `GameData::open_installed`; with it, that channel folder is read.

use std::collections::BTreeMap;
use std::time::Instant;

use versekit_gamedata::{Blueprint, GameData, Language, SourceKind, Text};

fn open() -> GameData {
    match std::env::var_os("VERSEKIT_CHANNEL_FOLDER") {
        Some(folder) => GameData::open(folder).unwrap(),
        None => GameData::open_installed().unwrap(),
    }
}

/// Every text of a blueprint with where it stands.
fn texts(blueprint: &Blueprint) -> Vec<(&'static str, &Text)> {
    let mut all = vec![("blueprint name", &blueprint.name)];
    if let Some(item) = &blueprint.item {
        all.push(("item name", &item.name));
        if let Some(maker) = &item.manufacturer {
            all.push(("manufacturer", &maker.name));
        }
    }
    for source in &blueprint.sources {
        all.push(("mission", &source.mission));
        all.extend(source.contractor.iter().map(|text| ("contractor", text)));
        all.extend(source.faction.iter().map(|text| ("faction", text)));
        all.extend(source.rank.iter().map(|text| ("rank", text)));
        all.extend(source.places.iter().map(|text| ("place", text)));
    }
    all
}

#[test]
#[ignore = "reads the installed game"]
fn installed_archive_counts() {
    let started = Instant::now();
    let game = open();
    println!("archive: {} ({:?}, {:?})", game.archive_path().display(), game.branch(), game.version());
    let english = game.localization(Language::English).unwrap();
    let german = game.localization(Language::German).unwrap();
    println!("texts: en {} keys, de {} keys (own file: {})", english.len(), german.len(), german.has_own_texts());
    let catalog = game.blueprints(&german).unwrap();
    println!("read in {:.1} s", started.elapsed().as_secs_f64());

    let blueprints = &catalog.blueprints;
    let count = |test: &dyn Fn(&Blueprint) -> bool| blueprints.iter().filter(|b| test(b)).count();
    println!("blueprints: {} (left out as leftovers: {})", blueprints.len(), catalog.left_out);
    println!("  with item: {}", count(&|b| b.item.is_some()));
    println!("  with item name text: {}", count(&|b| b.item.as_ref().is_some_and(|i| i.name.text.is_some())));
    println!("  with size: {}", count(&|b| b.item.as_ref().is_some_and(|i| i.size.is_some())));
    println!("  with grade: {}", count(&|b| b.item.as_ref().is_some_and(|i| i.grade.is_some())));
    println!("  with class: {}", count(&|b| b.item.as_ref().is_some_and(|i| i.class.is_some())));
    println!("  with manufacturer: {}", count(&|b| b.item.as_ref().is_some_and(|i| i.manufacturer.is_some())));
    println!("  with sources: {}", count(&|b| !b.sources.is_empty()));
    println!("  with contract sources: {}", count(&|b| b.sources.iter().any(|s| s.kind == SourceKind::Contract)));
    println!("  with scenario sources: {}", count(&|b| b.sources.iter().any(|s| s.kind == SourceKind::Scenario)));
    println!("  with a rank: {}", count(&|b| b.sources.iter().any(|s| s.rank.is_some())));
    println!("  with a place: {}", count(&|b| b.sources.iter().any(|s| !s.places.is_empty())));
    println!("  with a fixed aUEC reward: {}", count(&|b| b.sources.iter().any(|s| s.reward_uec.is_some())));
    println!(
        "pools: {} listing {} blueprints; contracts: {} released, {} unreleased; scenario tiers: {}",
        catalog.pools, catalog.in_pools, catalog.contracts, catalog.unreleased_contracts, catalog.scenario_tiers
    );
    let sources: Vec<_> = blueprints.iter().flat_map(|b| &b.sources).collect();
    println!("sources over all blueprints: {}", sources.len());
    let most_places = sources.iter().map(|s| s.places.len()).max().unwrap_or(0);
    let many_places = sources.iter().filter(|s| s.places.len() > 4).count();
    println!("places per source: at most {most_places}, more than 4 in {many_places} sources");
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for blueprint in blueprints {
        let kind = blueprint.item.as_ref().and_then(|item| item.kind.as_deref()).unwrap_or("-");
        *kinds.entry(kind).or_default() += 1;
    }
    println!("item kinds: {kinds:?}");
    println!("failed records: {}", catalog.failed.len());
    for failure in catalog.failed.iter().take(10) {
        println!("  {failure}");
    }
    if let Some(sample) = blueprints.iter().find(|b| b.sources.iter().any(|s| s.rank.is_some() && !s.places.is_empty()))
    {
        println!("sample: {} = {}", sample.class_name, sample.name.shown());
        for source in sample.sources.iter().take(3) {
            println!(
                "  {} | {} | {} | rank {} ({:?}) | +{:?} rep | {:?} aUEC | {}",
                source.mission.shown(),
                source.faction.as_ref().map_or("-", Text::shown),
                source.places.iter().map(Text::shown).collect::<Vec<_>>().join(", "),
                source.rank.as_ref().map_or("-", Text::shown),
                source.min_reputation,
                source.reputation_gain,
                source.reward_uec,
                source.pool,
            );
        }
    }

    assert!(blueprints.len() > 1000, "a release build has more than 1000 blueprints");
    let placeholders: Vec<String> = blueprints
        .iter()
        .flat_map(|b| texts(b).into_iter().map(move |(field, text)| (b, field, text)))
        .filter(|(_, _, text)| text.shown().to_ascii_uppercase().starts_with("@LOC_"))
        .map(|(b, field, text)| format!("{} {field}: {}", b.class_name, text.shown()))
        .collect();
    assert!(
        placeholders.is_empty(),
        "{} placeholder keys shown, e.g. {:?}",
        placeholders.len(),
        &placeholders[..placeholders.len().min(5)]
    );
    let unnamed = blueprints.iter().filter(|b| b.name.shown().is_empty()).count();
    assert_eq!(unnamed, 0, "every blueprint shows a name");
    let empty_places = sources.iter().flat_map(|s| &s.places).filter(|p| p.shown().is_empty()).count();
    assert_eq!(empty_places, 0, "every place shows a name or key");
    let without_pool = sources.iter().filter(|s| s.pool.is_empty()).count();
    assert_eq!(without_pool, 0, "every source names its reward pool");
}

/// The built-in corrections give the three Foxwell hauling contracts their
/// faction, which the data name only in an excluding prerequisite.
#[test]
#[ignore = "reads the installed game"]
fn installed_archive_foxwell_hauling_has_its_faction() {
    let game = open();
    let english = game.localization(Language::English).unwrap();
    let catalog = game.blueprints_corrected(&english, &versekit_gamedata::corrections::built_in()).unwrap();
    for contract in versekit_gamedata::corrections::FOXWELL_HAULING {
        let sources: Vec<_> = catalog.blueprints.iter().flat_map(|b| &b.sources).filter(|s| s.id == contract).collect();
        assert!(!sources.is_empty(), "{contract} awards a blueprint");
        for source in sources {
            assert_eq!(source.faction.as_ref().map(Text::shown), Some("Foxwell Enforcement"), "{contract}");
        }
    }
}

/// Writes every source of every blueprint, one line each, into the file
/// `VERSEKIT_SOURCES_DUMP` names, so that two readings can be compared.
#[test]
#[ignore = "reads the installed game"]
fn installed_archive_source_dump() {
    let Some(out) = std::env::var_os("VERSEKIT_SOURCES_DUMP") else { return };
    let started = Instant::now();
    let game = open();
    let english = game.localization(Language::English).unwrap();
    let catalog = game.blueprints(&english).unwrap();
    println!("read in {:.1} s", started.elapsed().as_secs_f64());
    let shown = |text: &Option<Text>| text.as_ref().map_or("-", Text::shown).to_string();
    let mut lines = vec![format!(
        "# blueprints {} pools {} in_pools {} contracts {} unreleased {} scenario_tiers {} failed {}",
        catalog.blueprints.len(),
        catalog.pools,
        catalog.in_pools,
        catalog.contracts,
        catalog.unreleased_contracts,
        catalog.scenario_tiers,
        catalog.failed.len()
    )];
    for blueprint in &catalog.blueprints {
        for s in &blueprint.sources {
            lines.push(
                [
                    blueprint.class_name.clone(),
                    format!("{:?}", s.kind),
                    s.id.clone(),
                    s.record_path.clone(),
                    s.pool.clone(),
                    format!("{:?}", s.chance),
                    s.title.clone(),
                    shown(&s.mission_type),
                    shown(&s.contractor),
                    shown(&s.faction),
                    s.places.iter().map(Text::shown).collect::<Vec<_>>().join(" | "),
                    format!("{:?}", s.shareable),
                    shown(&s.rank),
                    format!("{:?}", s.min_reputation),
                    shown(&s.max_rank),
                    format!("{:?}", s.max_reputation),
                    format!("{:?}", s.cooldown),
                    format!("{:?}", s.reputation_gain),
                    format!("{:?}", s.min_points),
                    format!("{:?}", s.reward_uec),
                ]
                .join("\t"),
            );
        }
    }
    std::fs::write(out, lines.join("\n") + "\n").unwrap();
    println!("{} lines", lines.len());
}
