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

//! The stock values of every weapon and component of the installed game: per kind the values most of its items
//! must have and the ranges they must lie in, the longest line, how many descriptions of the English `global.ini`
//! get a block and how many an item without values keeps from one, the weapons with several pellets or a sequence,
//! and the block of the items named in `VERSEKIT_STATS_SHOW` (comma separated parts of keys, default one of each
//! kind) in both languages.
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> cargo test -p versekit-gamedata --release --test description_stats_real --
//! --ignored --nocapture`; on a machine where VerseKit cannot tell whether the game runs, with
//! `VERSEKIT_TESTUMGEBUNG=1` as well.

use std::collections::BTreeMap;

use versekit_core::injection::item_stats::{self, ENGLISH, GERMAN, ItemStats, MAX_LINE, kind, value};
use versekit_core::jsonio::Value;
use versekit_gamedata::{GameData, Language};

/// One of each kind, and the S4 weapons of the comparison with erkul.games (its table of S4 weapons, 4.10.2).
const SHOW: &str = "BEHR_LaserCannon_S3,GATS_BallisticGatling_S3,klwe_rifle_energy_01,SHLD_GODI_S01_AllStop,\
COOL_AEGS_S01_Glacier,POWR_AEGS_S02_Bolide,QDRV_RSI_S01_Eos,MISL_S02_IR_FSKI_Ignite,cds_heavy_armor_01_core,\
PRAR_DistortionScatterGun";

/// How a value of a kind is checked: most items of the kind have it, and where it is there it lies in a range.
struct Expect {
    name: &'static str,
    /// Whether a value of zero or below counts as there (a resistance, a spread or a temperature may be zero or
    /// below; a fire rate not, and the block shows none of those).
    zero_counts: bool,
    low: f64,
    high: f64,
    /// The share of the items of the kind that must have it.
    least: f64,
}

const fn some(name: &'static str, low: f64, high: f64) -> Expect {
    Expect { name, zero_counts: false, low, high, least: 0.5 }
}

const fn share(name: &'static str) -> Expect {
    Expect { name, zero_counts: true, low: 0.0, high: 1.0, least: 0.5 }
}

const fn any(name: &'static str, low: f64, high: f64) -> Expect {
    Expect { name, zero_counts: true, low, high, least: 0.5 }
}

/// The ranges are those of LIVE 4.10.2 with room: capital ship guns reach 145.833 DPS and 200.000 alpha, size 12
/// torpedoes 1.230.000 damage, a toy pistol shoots at 15 m/s, a sniper spreads by 33°.
fn expected() -> Vec<(&'static str, Vec<Expect>)> {
    vec![
        (
            kind::SHIP_WEAPON,
            vec![
                some(value::DPS, 1.0, 1_000_000.0),
                some(value::ALPHA, 0.1, 1_000_000.0),
                some(value::FIRE_RATE, 1.0, 6_000.0),
                some(value::RANGE, 10.0, 100_000.0),
                some(value::SPEED, 50.0, 20_000.0),
                some(value::PENETRATION, 0.001, 100.0),
                any(value::SPREAD, 0.0, 45.0),
                some(value::POWER, 0.01, 100.0),
                some(value::EM, 1.0, 1_000_000.0),
                // Energy weapons have no ammunition: about 4 of 10 have.
                Expect { least: 0.3, ..some(value::AMMO, 1.0, 100_000.0) },
                some(value::HEALTH, 1.0, 1_000_000.0),
            ],
        ),
        (
            kind::PERSONAL_WEAPON,
            vec![
                some(value::DPS, 1.0, 100_000.0),
                some(value::ALPHA, 0.1, 100_000.0),
                some(value::FIRE_RATE, 1.0, 6_000.0),
                some(value::SPEED, 5.0, 20_000.0),
                some(value::AMMO, 1.0, 100_000.0),
                any(value::SPREAD, 0.0, 45.0),
            ],
        ),
        (
            kind::SHIELD,
            vec![
                some(value::SHIELD_HP, 1.0, 10_000_000.0),
                some(value::REGEN, 0.1, 1_000_000.0),
                any(value::DAMAGED_DELAY, 0.0, 100.0),
                any(value::DOWNED_DELAY, 0.0, 100.0),
                share(value::RESIST_PHYSICAL),
                share(value::RESIST_ENERGY),
                share(value::RESIST_DISTORTION),
                share(value::ABSORB_PHYSICAL),
                share(value::ABSORB_ENERGY),
                share(value::ABSORB_DISTORTION),
            ],
        ),
        (
            kind::COOLER,
            vec![
                some(value::COOLING, 0.1, 100_000.0),
                some(value::POWER, 0.01, 100.0),
                some(value::IR, 1.0, 1_000_000.0),
                some(value::DISTORTION_HP, 1.0, 1_000_000.0),
                share(value::RESIST_PHYSICAL),
                share(value::RESIST_ENERGY),
                share(value::RESIST_DISTORTION),
            ],
        ),
        (
            kind::POWER_PLANT,
            vec![
                some(value::OUTPUT, 1.0, 1_000.0),
                some(value::EM, 1.0, 1_000_000.0),
                some(value::DISTORTION_HP, 1.0, 1_000_000.0),
                share(value::RESIST_PHYSICAL),
                share(value::RESIST_ENERGY),
                share(value::RESIST_DISTORTION),
            ],
        ),
        (
            kind::QUANTUM_DRIVE,
            vec![
                some(value::DRIVE_SPEED, 1_000_000.0, 10_000_000_000.0),
                some(value::FUEL, 0.000_01, 1.0),
                some(value::SPOOL, 0.1, 100.0),
                some(value::COOLDOWN, 0.1, 100.0),
                some(value::POWER, 0.01, 100.0),
                some(value::EM, 1.0, 1_000_000.0),
                some(value::HEALTH, 1.0, 1_000_000.0),
            ],
        ),
        (
            kind::MISSILE,
            vec![
                some(value::DAMAGE, 1.0, 10_000_000.0),
                some(value::RADIUS, 0.1, 1_000.0),
                some(value::LOCK_TIME, 0.01, 100.0),
                some(value::LOCK_RANGE, 10.0, 1_000_000.0),
                some(value::SPEED, 50.0, 20_000.0),
            ],
        ),
        (
            kind::ARMOR,
            vec![
                share(value::RESIST_PHYSICAL),
                share(value::RESIST_ENERGY),
                share(value::RESIST_DISTORTION),
                any(value::TEMP_MIN, -1_000.0, 1_000.0),
                any(value::TEMP_MAX, -1_000.0, 1_000.0),
                some(value::RADIATION, 1.0, 10_000_000.0),
            ],
        ),
    ]
}

fn catalog_of(read: &versekit_core::jsonio::Map) -> Vec<ItemStats> {
    item_stats::from_catalog(&Value::Object(vec![(item_stats::CATALOG_KEY.to_string(), Value::Object(read.clone()))]))
}

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn every_kind_gets_its_values_from_the_game_data() {
    let path = std::env::var("VERSEKIT_P4K")
        .expect("VERSEKIT_P4K names no Data.p4k: this check reads the game data and proves nothing without them");
    let game = GameData::open_file(&path).expect("a readable Data.p4k");
    let texts = game.localization(Language::English).expect("the English texts");
    let started = std::time::Instant::now();
    let read = game.description_stats(&texts).expect("the values");
    println!("{} items in {:?}", read.len(), started.elapsed());
    let items = catalog_of(&read);
    let mut per_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &items {
        *per_kind.entry(item.kind.as_str()).or_default() += 1;
    }
    println!("per kind (\"\": without values, on a description of an item with values): {per_kind:?}");

    let ini = String::from_utf8_lossy(&game.english_ini_bytes().expect("the English global.ini")).into_owned();
    let lines: Vec<&str> = ini.lines().collect();
    let blocks = item_stats::build(&lines, &items, &ENGLISH);
    let shown_keys: std::collections::HashSet<String> = blocks.keys().map(|key| key.to_lowercase()).collect();
    // Only what the game text shows is checked against the ranges: a description whose items read differently
    // gets no block (one item named like the BR-2 shotgun has 99.999 rounds and 142.500 damage per second).
    let in_game = |item: &ItemStats| shown_keys.contains(&item_stats::shown_text_key(&item.name, &item.text));

    let mut wrong = Vec::new();
    for (wanted, checks) in expected() {
        let of_kind: Vec<&ItemStats> = items.iter().filter(|i| i.kind == wanted).collect();
        assert!(!of_kind.is_empty(), "no item of the kind {wanted}");
        let with_lines = of_kind.iter().filter(|i| !item_stats::lines(i, &GERMAN).is_empty()).count();
        println!("\n{wanted}: {} items, {with_lines} with lines", of_kind.len());
        assert!(with_lines * 2 > of_kind.len(), "{wanted}: most items have lines");
        for check in &checks {
            let found: Vec<f64> = of_kind.iter().filter_map(|i| i.values.get(check.name).copied()).collect();
            // A value the block does not show (zero or below where that means none) is not checked.
            let shown = |v: &f64| check.zero_counts || *v > 0.0;
            let having = found.iter().filter(|v| shown(v)).count();
            let (low, high) = found.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
            println!("  {}: {having} of {}, from {low} to {high}", check.name, of_kind.len());
            if (having as f64) < check.least * of_kind.len() as f64 {
                wrong.push(format!("{wanted}: only {having} of {} have {}", of_kind.len(), check.name));
            }
            for item in of_kind.iter().filter(|item| in_game(item)) {
                if let Some(v) = item.values.get(check.name).filter(|v| shown(v))
                    && (*v < check.low || *v > check.high)
                {
                    wrong.push(format!(
                        "{wanted} {} ({}): {} = {v} outside {}..={}",
                        item.text, item.name, check.name, check.low, check.high
                    ));
                }
            }
        }
        for words in [&GERMAN, &ENGLISH] {
            let longest = of_kind
                .iter()
                .flat_map(|i| item_stats::lines(i, words))
                .max_by_key(|line| line.chars().count())
                .unwrap_or_default();
            println!("  longest line ({} characters): {longest}", longest.chars().count());
            if longest.chars().count() > MAX_LINE {
                wrong.push(format!("{wanted}: a line of {} characters: {longest}", longest.chars().count()));
            }
        }
    }

    let with_values: Vec<ItemStats> = items.iter().filter(|i| !i.kind.is_empty()).cloned().collect();
    let without_the_others = item_stats::build(&lines, &with_values, &ENGLISH);
    println!(
        "\n{} descriptions of the English global.ini get a block; {} more would without the items without values on them",
        blocks.len(),
        without_the_others.len() - blocks.len()
    );
    // LIVE 4.10.2 has 146 such items; without them in the catalog their descriptions would show numbers not theirs.
    if per_kind.get("").copied().unwrap_or_default() == 0 {
        wrong.push("no item without values on the description of an item with values: they are not kept".into());
    }
    if blocks.len() <= 500 {
        wrong.push(format!("only {} descriptions get a block", blocks.len()));
    }

    // The weapons with several pellets or a sequence, for a look at what they show.
    let values = game.values(&texts).expect("the base values");
    let mut odd: Vec<String> = Vec::new();
    for item in values.items.values() {
        let Some(weapon) = &item.weapon else { continue };
        let pellets = weapon.fire_modes.iter().filter_map(|m| m.shown().pellet_count).max().unwrap_or(1);
        let sequence = weapon.fire_modes.iter().any(versekit_gamedata::values::FireMode::is_sequence);
        if pellets > 1 || sequence {
            let entry = read.iter().find(|(guid, _)| *guid == item.guid.to_ascii_lowercase()).map(|(_, e)| e);
            let shown = entry.map(|e| catalog_of(&vec![(item.guid.clone(), e.clone())])).unwrap_or_default();
            let first = shown.first().map(|i| item_stats::lines(i, &ENGLISH).first().cloned().unwrap_or_default());
            odd.push(format!(
                "{} ({}): pellets {pellets}, sequence {sequence} → {}",
                item.name.as_deref().unwrap_or("?"),
                item.class_name,
                first.unwrap_or_default()
            ));
        }
    }
    odd.sort();
    println!("\n{} weapons with several pellets or a sequence:", odd.len());
    for line in odd.iter().take(40) {
        println!("  {line}");
    }

    // A scattergun counts all its pellets, as erkul.games does (Salvation, S4: 8 pellets × 60 = 480).
    let salvation = items.iter().find(|i| i.text.eq_ignore_ascii_case("item_DescPRAR_DistortionScatterGun_S4"));
    if let Some(salvation) = salvation {
        let alpha = salvation.values.get(value::ALPHA).copied().unwrap_or_default();
        println!("\nSalvation Distortion Scattergun: alpha {alpha}, DPS {:?}", salvation.values.get(value::DPS));
        if (alpha - 480.0).abs() > 0.5 {
            wrong.push(format!("the Salvation scattergun has alpha {alpha}, erkul.games shows 480"));
        }
    }

    // DPS, alpha and fire rate as erkul.games's table of S4 weapons shows them for 4.10.2-LIVE.12913470, rounded as
    // there. The C-788 loops over one entry (rate 50, delay 40 a minute) and fires at its own rate, 50.
    let erkul: [(&str, f64, f64, f64); 8] = [
        ("item_DescBEHR_BallisticCannon_S4", 813.0, 975.0, 50.0),
        ("item_DescBEHR_LaserCannon_S4", 1025.0, 615.0, 100.0),
        ("item_DescESPR_BallisticCannon_S4", 1017.0, 610.0, 100.0),
        ("item_DescGLSN_BallisticGatling_S4", 953.0, 52.0, 1100.0),
        ("item_DescAMRS_LaserCannon_S4", 820.0, 328.0, 150.0),
        ("item_DescESPR_LaserCannon_S4", 693.0, 166.0, 250.0),
        ("item_DescBEHR_BallisticGatling_S4", 1266.0, 84.0, 900.0),
        ("item_DescKLWE_LaserRepeater_S4", 818.0, 65.0, 750.0),
    ];
    for (text, dps, alpha, rate) in erkul {
        let Some(weapon) = items.iter().find(|i| i.text.eq_ignore_ascii_case(text)) else {
            wrong.push(format!("{text}: not read"));
            continue;
        };
        for (name, theirs) in [(value::DPS, dps), (value::ALPHA, alpha), (value::FIRE_RATE, rate)] {
            let own = weapon.values.get(name).copied().unwrap_or_default();
            if (own - theirs).abs() >= 1.0 {
                wrong.push(format!("{text}: {name} {own}, erkul.games {theirs}"));
            }
        }
    }

    // The S4 ship weapons in one line each, to set beside erkul.games's table of S4 weapons.
    let mut s4: Vec<String> = items
        .iter()
        .filter(|i| i.kind == kind::SHIP_WEAPON && i.text.to_lowercase().ends_with("_s4"))
        .map(|i| format!("{}: {}", i.text, item_stats::lines(i, &ENGLISH).join(" | ")))
        .collect();
    s4.sort();
    println!("\nS4 ship weapons:");
    for line in &s4 {
        println!("  {line}");
    }

    let show = std::env::var("VERSEKIT_STATS_SHOW").unwrap_or_else(|_| SHOW.to_string());
    for part in show.split(',').map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()) {
        let Some((guid, entry)) = read.iter().find(|(_, entry)| {
            entry.get("art").is_some()
                && (entry.get("text").and_then(Value::as_str).is_some_and(|t| t.to_lowercase().contains(&part))
                    || entry.get("name").and_then(Value::as_str).is_some_and(|n| n.to_lowercase().contains(&part)))
        }) else {
            println!("\n{part}: not found");
            continue;
        };
        let one = catalog_of(&vec![(guid.clone(), entry.clone())]);
        let item = &one[0];
        println!("\n{part} ({}, {}):", item.kind, item.text);
        println!("  DE: {}", item_stats::lines(item, &GERMAN).join("\n      "));
        println!("  EN: {}", item_stats::lines(item, &ENGLISH).join("\n      "));
        let shown = blocks.keys().any(|key| key.eq_ignore_ascii_case(&item.text));
        println!("  block in the English global.ini: {}", if shown { "yes" } else { "no" });
    }
    assert!(wrong.is_empty(), "{} findings:\n{}", wrong.len(), wrong.join("\n"));
}
