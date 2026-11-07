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

//! Cross-check of the own game values against scmdb at the same build
//! (`versekit_gamedata::cross_check`, the same comparison VerseKit runs on
//! every read):
//! the recipes with their slots and modifiers, and of every product its
//! kind, name, maker, size, grade, integrity, shield values, fire modes and
//! ammunition, read from the `Data.p4k`, must equal those of scmdb's files.
//! Cooling, power, quantum, radar, tractor, salvage and armour values are not
//! read from the `Data.p4k` yet and not compared. What quality makes of the
//! values is checked against 4.12 (`versekit-parity crafting`).
//!
//! `versekit-parity werte <dataforge.dcb> <global.ini> <crafting-blueprints.json> [--gegenprobe]`
//! reads a DataForge and an English `global.ini` kept from the archive
//! (`GameData::dataforge_bytes`, `GameData::english_ini_bytes`), and the
//! recipe file 4.12 keeps from scmdb (with `products`). With `--gegenprobe`
//! one modifier, one fire rate and one shield strength of products are
//! changed (the first by tag), and the check must report each of the three.

use std::fmt::Write as _;
use std::time::Instant;

use serde_json::Value;
use versekit_gamedata::values::{GameValues, Stat};
use versekit_gamedata::{Language, Localization, cross_check, scmdb, values_from_dataforge};

pub fn run(dcb: &str, ini: &str, recipes_file: &str, gegenprobe: bool) -> Result<(), String> {
    let started = Instant::now();
    let english = std::fs::read(ini).map_err(|e| format!("{ini}: {e}"))?;
    let texts = Localization::from_files(Language::English, &english, None).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(dcb).map_err(|e| format!("{dcb}: {e}"))?;
    let kept: Value =
        serde_json::from_slice(&std::fs::read(recipes_file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let build = kept["build"].as_str().map(str::to_string);
    let read_started = Instant::now();
    let mut own: GameValues = values_from_dataforge(&bytes, &texts, build.clone()).map_err(|e| e.to_string())?;
    let read_seconds = read_started.elapsed().as_secs_f64();
    let theirs = scmdb::values(build.as_deref(), &kept, &kept["products"]);
    if gegenprobe {
        // The recipes are ordered by tag; the first that fits is changed.
        let recipe = own
            .recipes
            .iter_mut()
            .find(|r| r.tiers.first().is_some_and(|t| t.slots.iter().any(|s| !s.modifiers.is_empty())))
            .ok_or("Gegenprobe: kein Rezept mit Modifikator")?;
        let slot =
            recipe.tiers[0].slots.iter_mut().find(|s| !s.modifiers.is_empty()).ok_or("Gegenprobe: kein Platz")?;
        slot.modifiers[0].at_end += 0.01;
        let products: Vec<String> =
            theirs.recipes.iter().filter(|r| theirs.product(r).is_some()).filter_map(|r| r.product.clone()).collect();
        let gun = products
            .iter()
            .find(|guid| {
                own.items.get(*guid).and_then(|i| i.weapon.as_ref()).is_some_and(|w| {
                    w.fire_modes.first().is_some_and(|m| m.shown().fire_rate.is_some_and(|rate| rate > 0.0))
                })
            })
            .cloned()
            .ok_or("Gegenprobe: keine Waffe unter den Produkten")?;
        if let Some(mode) =
            own.items.get_mut(&gun).and_then(|i| i.weapon.as_mut()).and_then(|w| w.fire_modes.first_mut())
        {
            let action = if mode.is_sequence() { &mut mode.sequence[0] } else { &mut mode.action };
            action.fire_rate = action.fire_rate.map(|r| r + 1.0);
        }
        let shield = products
            .iter()
            .find(|guid| own.items.get(*guid).is_some_and(|i| i.stat(Stat::ShieldHealth).is_some()))
            .cloned()
            .ok_or("Gegenprobe: kein Schild unter den Produkten")?;
        if let Some(item) = own.items.get_mut(&shield) {
            item.stats.insert(Stat::ShieldHealth, item.stat(Stat::ShieldHealth).unwrap_or(0.0) + 1.0);
        }
    }
    let tally = cross_check::compare(&own, &theirs);
    let (only_scmdb, only_own, products) = (&tally.only_scmdb, &tally.only_own, tally.products);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Build {}: eigen {} Rezepte und {} Gegenstände in {:.1} s gelesen (gesamt {:.1} s), scmdb {} Rezepte",
        build.as_deref().unwrap_or("?"),
        own.recipes.len(),
        own.items.len(),
        read_seconds,
        started.elapsed().as_secs_f64(),
        theirs.recipes.len()
    );
    let _ = writeln!(out, "Nur bei scmdb: {} {:?}", only_scmdb.len(), only_scmdb.iter().take(5).collect::<Vec<_>>());
    let _ = writeln!(out, "Nur eigen: {} {:?}", only_own.len(), only_own.iter().take(5).collect::<Vec<_>>());
    let _ = writeln!(out, "Verglichene Produkte: {products}; nicht lesbare Datensätze: {}", own.failed.len());
    for (field, count) in &tally.fields {
        let _ = writeln!(out, "{field}: {} gleich, {} verschieden", count.equal, count.different);
        for example in &count.examples {
            let _ = writeln!(out, "    {example}");
        }
    }
    let _ = writeln!(out, "Abweichungen gesamt: {}", tally.differences());
    if let Some(build) = build.as_deref() {
        // The corrections on the own values: scmdb's list of the same build,
        // kept by 4.12 next to the recipes, and the cases VerseKit adds.
        let issues_file = std::path::Path::new(recipes_file).with_file_name("cig-datenfehler.json");
        let mut list = versekit_gamedata::corrections::built_in();
        if let Ok(bytes) = std::fs::read(&issues_file)
            && let Ok(kept) = serde_json::from_slice::<Value>(&bytes)
            && kept["build"].as_str() == Some(build)
        {
            list.extend(versekit_gamedata::corrections::from_scmdb(&kept["daten"], build));
        }
        let report = versekit_gamedata::corrections::apply(&mut own, &list);
        let _ = writeln!(out, "Berichtigungen auf den eigenen Werten: {}", list.len());
        for (correction, outcome) in &report.outcomes {
            let _ = writeln!(
                out,
                "    {:?} {:?} {} {:?} → {:?}",
                outcome, correction.section, correction.record, correction.field, correction.expected
            );
        }
    }
    print!("{out}");
    if gegenprobe {
        let missed: Vec<&str> = ["Platz: Modifikatoren", "Modus: Feuerrate", "Wert: Schildstärke"]
            .into_iter()
            .filter(|field| tally.fields.get(field).is_none_or(|count| count.different == 0))
            .collect();
        if !missed.is_empty() {
            return Err(format!("Gegenprobe hat nicht angeschlagen bei: {}", missed.join(", ")));
        }
    }
    if tally.differences() > 0 {
        return Err(format!("{} Abweichungen", tally.differences()));
    }
    Ok(())
}
