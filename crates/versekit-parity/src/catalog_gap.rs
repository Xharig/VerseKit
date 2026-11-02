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

//! `versekit-parity katalog_luecke <daten> <auszug> [<scmdb-ordner>] [--alle]`:
//! the catalog the list builds without the 4.12 cache, against the 4.12
//! cache in `<daten>`, by kind of difference.
//!
//! `<auszug>` is a folder made by `versekit-parity auszug`. With
//! `<scmdb-ordner>` (holding `merged-<build>.json` and
//! `cig_data_issues-<build>.json` of the build in `<auszug>/build.txt`) the
//! catalog is completed from scmdb as at run time; without it the Data.p4k
//! stands alone. Without `--alle` only the first lines of each kind are
//! printed. The patch stamps (`seit`) come from the history in `<daten>`,
//! read only.

use std::path::Path;

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::patch_history;
use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_gamedata::corrections::{self, Correction};
use versekit_gamedata::{Language, Localization, blueprints_from_dataforge, list, scmdb_catalog};

fn paths_for(data: &str) -> Result<Paths, String> {
    if Path::new(data).join(COPY_DATA_FOLDER).is_dir() {
        return Paths::for_copy(Host::current(), data).ok_or_else(|| format!("keine Datenkopie: {data}"));
    }
    Ok(Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, data)], data)))
}

/// The catalog from the game data (and scmdb's files in `scmdb`, when given)
/// the way a build makes it, without the stamp of the patch.
pub fn catalog_from(extract: &str, scmdb: Option<&str>) -> Result<(CatalogCache, Option<CatalogCache>), String> {
    let read = |name: &str| std::fs::read(Path::new(extract).join(name)).map_err(|e| format!("{name}: {e}"));
    let build_file = String::from_utf8_lossy(&read("build.txt")?).into_owned();
    let mut lines = build_file.lines();
    let build = lines.next().unwrap_or_default().to_string();
    let version = if build.is_empty() { lines.next().unwrap_or_default().to_string() } else { build.clone() };
    let texts = Localization::from_files(Language::English, &read("global.ini")?, None).map_err(|e| e.to_string())?;
    let mut fixes: Vec<Correction> = corrections::built_in();
    let mut theirs = None;
    if let Some(folder) = scmdb {
        let issues =
            std::fs::read(Path::new(folder).join(corrections::scmdb_file(&build))).map_err(|e| e.to_string())?;
        let issues: serde_json::Value = serde_json::from_slice(&issues).map_err(|e| e.to_string())?;
        fixes.extend(corrections::from_scmdb(&issues, &build));
        let merged =
            std::fs::read(Path::new(folder).join(scmdb_catalog::merged_file(&build))).map_err(|e| e.to_string())?;
        let merged: serde_json::Value = serde_json::from_slice(&merged).map_err(|e| e.to_string())?;
        theirs = Some(scmdb_catalog::from_merged(&merged, &build, &fixes));
    }
    let facts = crate::extract::contract_facts(Path::new(extract))?;
    let game = blueprints_from_dataforge(&read("Game2.dcb")?, &facts, &texts, &fixes, Some(&build))
        .map_err(|e| e.to_string())?;
    let complete = list::from_game(&game, &version, theirs.as_ref());
    Ok((complete, theirs))
}

pub fn run(data: &str, extract: &str, scmdb: Option<&str>, all: bool) -> Result<(), String> {
    let paths = paths_for(data)?;
    let old = catalog_cache::load(&paths).ok_or("kein 4.12-Katalog in den Daten")?;
    let (mut new, theirs) = catalog_from(extract, scmdb)?;
    let since = patch_history::version_per_blueprint(&patch_history::load(&paths));
    for blueprint in &mut new.blueprints {
        blueprint.since = since.get(&blueprint.key).cloned();
    }
    println!(
        "alt (4.12): {} Baupläne · neu (Data.p4k{}): {} Baupläne",
        old.blueprints.len(),
        if scmdb.is_some() { " + scmdb" } else { "" },
        new.blueprints.len()
    );
    if let Some(theirs) = &theirs {
        let check = list::cross_check(&new, theirs);
        println!("Gegenprobe Data.p4k gegen scmdb: {} Abweichungen {}", check.differences(), check.summary());
        println!("Sperrzeit (nicht gezählt, scmdb liest den ersten Handler): {} Baupläne", check.cooldowns.len());
        for (what, list) in [
            ("nur eigen", &check.only_own),
            ("nur scmdb", &check.only_scmdb),
            ("Wege", &check.ways),
            ("Aufträge", &check.missions),
            ("Sperrzeit", &check.cooldowns),
        ] {
            for line in list.iter().take(if all { usize::MAX } else { 4 }) {
                println!("      {what}: {line}");
            }
        }
    }
    print_differences(&old, &new, all);
    Ok(())
}

/// Every difference between the 4.12 catalog `old` and `new`, by kind, with
/// the first lines of each kind (all with `all`).
pub fn print_differences(old: &CatalogCache, new: &CatalogCache, all: bool) {
    let differences = list::differences(old, new);
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for d in &differences {
        let what = d.what.split_once(' ').filter(|(w, _)| *w == "Weg").map_or(d.what.clone(), |(_, rest)| {
            format!("Weg {}", rest.split_once(' ').map_or(rest, |(_, field)| field))
        });
        match kinds.iter_mut().find(|(k, _)| *k == what) {
            Some(entry) => entry.1 += 1,
            None => kinds.push((what, 1)),
        }
    }
    println!("{} Unterschiede zum 4.12-Katalog", differences.len());
    for (what, count) in &kinds {
        println!("  {what}: {count}");
        let of_kind = differences.iter().filter(|d| d.what.ends_with(what.trim_start_matches("Weg ")));
        for d in of_kind.take(if all { usize::MAX } else { 3 }) {
            println!("      {d}");
        }
    }
}
