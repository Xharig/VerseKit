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

//! `versekit-parity katalog_bau <daten> <kanalordner> <scmdb-ordner> [--alle]`:
//! the whole way of the blueprint list, as a new user without any cache of
//! the 4.12 version goes it, with the real `Data.p4k` in `<kanalordner>`.
//!
//! The catalog is built into a throwaway data folder that holds no 4.12
//! cache (`list::list_catalog_with`: archive, corrections, scmdb, patch
//! history). scmdb's files are not fetched but read from `<scmdb-ordner>`
//! (`merged-<build>.json`, `cig_data_issues-<build>.json`, as scmdb names
//! them). The result is compared with the 4.12 cache in `<daten>` (only
//! read), and these must hold or the run fails: the starter blueprints, the
//! reputation limits and the cooldowns come from the game data, the second
//! build finds the cache current, the patch history is fed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::writeguard;
use versekit_gamedata::corrections::{self, Correction};
use versekit_gamedata::list::{self, Supply};
use versekit_gamedata::scmdb_catalog;

/// scmdb's files of one build in a folder, standing for the network.
struct Files {
    folder: PathBuf,
}

impl Files {
    fn read(&self, name: &str) -> Result<serde_json::Value, String> {
        let bytes = std::fs::read(self.folder.join(name)).map_err(|e| format!("{name}: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{name}: {e}"))
    }
}

impl Supply for Files {
    fn current_build(&self) -> Result<String, String> {
        Err("keine Netzabfrage".into())
    }

    fn corrections(&self, build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        let mut list = corrections::built_in();
        let Some(build) = build else { return (list, Some("kein Build".into())) };
        match self.read(&corrections::scmdb_file(build)) {
            Ok(file) => {
                list.extend(corrections::from_scmdb(&file, build));
                (list, None)
            }
            Err(error) => (list, Some(error)),
        }
    }

    fn catalog(&self, build: &str, corrections: &[Correction]) -> Result<CatalogCache, String> {
        Ok(scmdb_catalog::from_merged(&self.read(&scmdb_catalog::merged_file(build))?, build, corrections))
    }
}

fn paths_for(data: &str) -> Result<Paths, String> {
    if Path::new(data).join(COPY_DATA_FOLDER).is_dir() {
        return Paths::for_copy(Host::current(), data).ok_or_else(|| format!("keine Datenkopie: {data}"));
    }
    Ok(Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, data)], data)))
}

fn check(ok: bool, what: &str, failures: &mut Vec<String>) {
    println!("{} {what}", if ok { "OK    " } else { "FEHLER" });
    if !ok {
        failures.push(what.to_string());
    }
}

pub fn run(data: &str, channel: &str, scmdb: &str, all: bool, upgrader: bool) -> Result<(), String> {
    let data_paths = paths_for(data)?;
    let old = catalog_cache::load(&data_paths).ok_or("kein 4.12-Katalog in den Daten")?;
    let scratch = std::env::temp_dir().join(format!("versekit-katalog-bau-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    // `VERSEKIT_KATALOG_BEHALTEN=<folder>` keeps the built cache and the
    // reputation table there, for the parity check of the notes.
    let keep = std::env::var("VERSEKIT_KATALOG_BEHALTEN").ok();
    let text = scratch.to_string_lossy().into_owned();
    let fresh = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text));
    writeguard::deny(true);
    writeguard::allow_under(Some(&text));
    let cache = scratch.join(list::CACHE_FILE).to_string_lossy().into_owned();
    let channel = Path::new(channel);
    let supply: Arc<dyn Supply> = Arc::new(Files { folder: PathBuf::from(scmdb) });

    let mut failures = Vec::new();
    if upgrader {
        // Someone who comes from 4.12: its cache, its list of everything seen and its history are there.
        for file in
            [catalog_cache::CACHE_FILE, versekit_core::patch_history::SEEN_FILE, versekit_core::patch_history::FILE]
        {
            let _ = std::fs::copy(data_paths.app_file(file), fresh.app_file(file));
        }
        check(
            Path::new(&fresh.app_file(versekit_core::patch_history::SEEN_FILE)).is_file(),
            "Aufrüster: Liste des Gesehenen aus 4.12 übernommen",
            &mut failures,
        );
    } else {
        check(
            !Path::new(&fresh.app_file(catalog_cache::CACHE_FILE)).exists(),
            "kein Zwischenspeicher der 4.12-Fassung im Datenordner",
            &mut failures,
        );
    }
    let started = std::time::Instant::now();
    let new = list::list_catalog_with(&fresh, Some(channel), &cache, Arc::clone(&supply));
    let took = started.elapsed();
    let new = new.ok_or("kein Katalog gebaut")?;
    println!(
        "gebaut in {:.1} s: {} Baupläne, {} Aufträge, Stand {}",
        took.as_secs_f64(),
        new.blueprints.len(),
        new.missions.len(),
        new.version
    );
    check(Path::new(&cache).is_file(), "Zwischenspeicher des Katalogs geschrieben", &mut failures);
    check(
        new.blueprints.iter().filter(|b| b.start).count() == 8,
        "acht Startbaupläne aus den Spieldaten",
        &mut failures,
    );
    check(
        new.missions.iter().any(|m| m.reputation_max.is_some_and(|max| max > 0)),
        "Ruf-Obergrenzen aus den Spieldaten",
        &mut failures,
    );
    check(
        new.missions.iter().any(|m| m.cooldown.is_some_and(|c| c > 0)),
        "Sperrzeiten aus den Spieldaten",
        &mut failures,
    );
    check(
        new.missions.iter().any(|m| m.shareable == Some(false)),
        "nicht teilbare Aufträge aus den Spieldaten",
        &mut failures,
    );
    check(
        new.blueprints.iter().flat_map(|b| &b.ways).any(|w| w.place.as_ref().is_some_and(|p| !p.system.is_empty())),
        "Orte mit System von scmdb ergänzt",
        &mut failures,
    );
    check(!versekit_core::patch_history::seen(&fresh).is_empty(), "alles bisher Gesehene festgehalten", &mut failures);
    let again = list::start_catalog_with(&fresh, Some(channel), &cache, supply);
    check(
        again.rebuild.is_none() && again.catalog.as_ref() == Some(&new),
        "der zweite Start findet den Katalog aktuell",
        &mut failures,
    );
    let denied = writeguard::take_denied();
    check(denied.is_empty(), "keine abgelehnten Schreibversuche", &mut failures);

    let with_since = new.blueprints.iter().filter(|b| b.since.is_some()).count();
    let old_since = old.blueprints.iter().filter(|b| b.since.is_some()).count();
    println!("Stempel „seit“: {with_since} (4.12: {old_since}; ein neuer Nutzer hat nur die mitgelieferte Historie)");
    if upgrader {
        // What the build wrote into the history as new for this version: with the list of everything seen
        // from 4.12 it must be next to nothing.
        let history = versekit_core::patch_history::load(&fresh);
        let added: Vec<&String> = history.get(&new.version).map(|p| p.new.iter().collect()).unwrap_or_default();
        println!(
            "Aufrüster: {} Zugänge für {} in der Historie: {:?}",
            added.len(),
            new.version,
            added.iter().take(10).collect::<Vec<_>>()
        );
    }
    crate::catalog_gap::print_differences(&old, &new, all);
    writeguard::deny(false);
    writeguard::allow_under(None);
    if let Some(folder) = keep.as_deref() {
        let _ = std::fs::create_dir_all(folder);
        for file in [list::CACHE_FILE, versekit_core::injection::reputation::CACHE_FILE] {
            let kept = std::fs::copy(fresh.app_file(file), Path::new(folder).join(file));
            println!("behalten: {file} {}", if kept.is_ok() { "ja" } else { "fehlt" });
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} Prüfungen fehlgeschlagen: {failures:?}", failures.len()))
    }
}
