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

//! What the catalog reads out of the real DataForge besides the blueprints:
//! the starter blueprints, and the reputation limit, shareable and cooldown
//! of the contracts.
//!
//! Run with `cargo test -p versekit-gamedata --release --test catalog_real --
//! --ignored --nocapture` and `VERSEKIT_DATAFORGE` / `VERSEKIT_GLOBAL_INI` /
//! `VERSEKIT_CONTRACTS` (the copies `versekit-parity auszug` keeps). Written
//! for build 4.10.2-live.12913470.

use versekit_gamedata::{ContractFacts, Language, Localization, SourceKind, blueprints_from_dataforge, corrections};

#[test]
#[ignore = "reads the game data"]
fn the_catalog_holds_starters_limits_and_cooldowns() {
    let (Some(dcb), Some(ini), Some(contracts)) = (
        std::env::var_os("VERSEKIT_DATAFORGE"),
        std::env::var_os("VERSEKIT_GLOBAL_INI"),
        std::env::var_os("VERSEKIT_CONTRACTS"),
    ) else {
        return;
    };
    let texts = Localization::from_files(Language::English, &std::fs::read(ini).unwrap(), None).unwrap();
    let facts = ContractFacts::from_catalog(&std::fs::read(contracts).unwrap()).unwrap();
    let catalog =
        blueprints_from_dataforge(&std::fs::read(dcb).unwrap(), &facts, &texts, &corrections::built_in(), None)
            .unwrap();
    let defaults: Vec<&str> = catalog.blueprints.iter().filter(|b| b.default).map(|b| b.name.shown()).collect();
    println!("starter blueprints ({}): {defaults:?}", defaults.len());
    assert_eq!(defaults.len(), 8, "four suit parts, two weapons and two magazines");
    let contracts: Vec<_> =
        catalog.blueprints.iter().flat_map(|b| &b.sources).filter(|s| s.kind == SourceKind::Contract).collect();
    let count = |test: &dyn Fn(&versekit_gamedata::Source) -> bool| contracts.iter().filter(|s| test(s)).count();
    println!("contract sources: {}", contracts.len());
    println!("  with a limit: {}", count(&|s| s.max_reputation.is_some()));
    println!(
        "  with shareable: {} (not shareable: {})",
        count(&|s| s.shareable.is_some()),
        count(&|s| s.shareable == Some(false))
    );
    println!("  with a cooldown: {}", count(&|s| s.cooldown.is_some()));
    if let Some(name) = std::env::var_os("VERSEKIT_BLUEPRINT") {
        let name = name.to_string_lossy().to_lowercase();
        for b in catalog.blueprints.iter().filter(|b| b.name.shown().to_lowercase() == name) {
            for s in b.sources.iter().filter(|s| s.kind == SourceKind::Contract) {
                println!(
                    "{} | {} | cooldown {:?} shareable {:?} max {:?} rep {:?}",
                    s.id, s.record_path, s.cooldown, s.shareable, s.max_reputation, s.min_reputation
                );
            }
        }
    }
    if let Some(name) = std::env::var_os("VERSEKIT_CONTRACT") {
        let name = name.to_string_lossy().to_string();
        for s in contracts.iter().filter(|s| s.id == name).take(2) {
            println!(
                "{}: cooldown {:?} shareable {:?} max {:?} rep {:?}",
                s.id, s.cooldown, s.shareable, s.max_reputation, s.min_reputation
            );
        }
    }
    assert!(count(&|s| s.max_reputation.is_some()) > 0);
    assert!(count(&|s| s.shareable == Some(false)) > 0);
    assert!(count(&|s| s.cooldown.is_some()) > 0, "no cooldown read");
    println!(
        "{:?}",
        catalog
            .corrections
            .outcomes
            .iter()
            .map(|(c, o)| (c.reason.chars().take(30).collect::<String>(), *o))
            .collect::<Vec<_>>()
    );
}
