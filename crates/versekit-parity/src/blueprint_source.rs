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

//! `versekit-parity bauplan_quelle <daten> <kanalordner> [--gegenprobe [haken|stern|raute]]`:
//! the blueprint list's catalog from the 4.12 cache (old) against the one
//! from the `Data.p4k` in `<kanalordner>` (new), with the load times.
//!
//! `<daten>` is a copy of the user's data (holding `VerseKit-Daten`) or a
//! data folder of its own. Nothing is written there: writing is locked, and
//! the cache of the new catalog goes into a throwaway folder. Every
//! difference is listed, and every check, star and diamond that the new
//! catalog would lose. `--gegenprobe` drops one blueprint that carries a
//! check (`haken`, the default), a star (`stern`) or a diamond (`raute`)
//! from the new catalog. The run then succeeds only when the comparison
//! names exactly that mark as lost (`GEGENPROBE GREIFT`); otherwise, and
//! without a `Data.p4k`, it fails.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths, name_key};
use versekit_core::pypath::Flavor;
use versekit_core::{collection, writeguard};
use versekit_gamedata::list;

/// How often a load is repeated; the median counts.
const RUNS: usize = 5;

/// Which mark the counter check takes away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// No counter check.
    Off,
    /// A blueprint with a check.
    Check,
    /// A blueprint with a star.
    Star,
    /// A blueprint with a diamond.
    Diamond,
}

impl Counter {
    /// The mark named on the command line; `None` gives a check.
    pub fn parse(which: Option<&str>) -> Result<Self, String> {
        match which {
            None | Some("haken") => Ok(Self::Check),
            Some("stern") => Ok(Self::Star),
            Some("raute") => Ok(Self::Diamond),
            Some(other) => Err(format!("unbekannte Gegenprobe: {other} (haken, stern, raute)")),
        }
    }

    /// The position in [`marks`] of the mark this counter check takes away.
    fn mark(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Check => Some(0),
            Self::Star => Some(1),
            Self::Diamond => Some(2),
        }
    }
}

/// A name as the farm notes compare it (like the blueprint list).
fn slim(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

fn median<T>(mut run: impl FnMut() -> T) -> (Duration, T) {
    let started = Instant::now();
    let mut last = run();
    let mut times = vec![started.elapsed()];
    for _ in 1..RUNS {
        let started = Instant::now();
        last = run();
        times.push(started.elapsed());
    }
    times.sort();
    (times.get(times.len() / 2).copied().unwrap_or_default(), last)
}

fn ms(time: Duration) -> String {
    format!("{:.1} ms", time.as_secs_f64() * 1000.0)
}

fn paths_for(data: &str) -> Result<Paths, String> {
    if Path::new(data).join(COPY_DATA_FOLDER).is_dir() {
        return Paths::for_copy(Host::current(), data).ok_or_else(|| format!("keine Datenkopie: {data}"));
    }
    Ok(Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, data)], data)))
}

/// What the user marked, as keys of the catalog: checks, stars, diamonds.
fn marks(paths: &Paths) -> [(&'static str, HashSet<String>); 3] {
    let read = |file: &str| jsonio::read_json(&paths.app_file(file)).ok();
    let have = match read(collection::FILE_NAME) {
        Some(Value::Object(map)) => collection::keys(&map).into_iter().collect(),
        _ => HashSet::new(),
    };
    let watched = match read("watchlist.json").as_ref().and_then(|w| w.get("namen")) {
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(name_key).collect(),
        _ => HashSet::new(),
    };
    let noted = match read("hangar.json").as_ref().and_then(|h| h.get("merkzettel")) {
        Some(Value::Array(items)) => {
            items.iter().filter_map(|m| m.get("name").and_then(Value::as_str)).map(slim).collect()
        }
        _ => HashSet::new(),
    };
    [("Haken", have), ("Stern", watched), ("Raute", noted)]
}

/// What a mark of kind `what` is matched by: the slim name for a diamond,
/// else the catalog key.
fn mark_id(what: &str, blueprint: &catalog_cache::Blueprint) -> String {
    if what == "Raute" { slim(blueprint.name.trim()) } else { blueprint.key.clone() }
}

/// Takes the first blueprint out of `new` that carries the mark at
/// `which` of `marks`, stands in `old` too and shares the mark with no
/// other blueprint of `new`; the line [`lost`] must then report.
fn drop_marked(
    old: &CatalogCache,
    new: &mut CatalogCache,
    marks: &[(&'static str, HashSet<String>); 3],
    which: usize,
) -> Option<String> {
    let (what, keys) = &marks[which];
    let in_old: HashSet<String> = old.blueprints.iter().map(|b| mark_id(what, b)).collect();
    let ids: Vec<String> = new.blueprints.iter().map(|b| mark_id(what, b)).collect();
    let at = ids.iter().position(|id| {
        keys.contains(id) && in_old.contains(id) && ids.iter().filter(|other| *other == id).count() == 1
    })?;
    let dropped = new.blueprints.remove(at);
    let id = mark_id(what, &dropped);
    let name = old.blueprints.iter().find(|b| mark_id(what, b) == id).map_or(dropped.name, |b| b.name.clone());
    Some(format!("{what} verloren: {name}"))
}

/// The marks that stand at a blueprint of `old` and at none of `new`.
fn lost(old: &CatalogCache, new: &CatalogCache, marks: &[(&'static str, HashSet<String>); 3]) -> Vec<String> {
    let mut found = Vec::new();
    for (what, keys) in marks {
        let id = |b: &catalog_cache::Blueprint| mark_id(what, b);
        let in_new: HashSet<String> = new.blueprints.iter().map(id).collect();
        for b in &old.blueprints {
            let key = id(b);
            if keys.contains(&key) && !in_new.contains(&key) {
                found.push(format!("{what} verloren: {}", b.name));
            }
        }
    }
    found
}

/// Without a `Data.p4k` in `channel`: what the list loads the way the window
/// does (game folder from the data's settings) must be the 4.12 cache, and
/// how long each takes.
fn fallback_only(paths: &Paths, channel: &Path) -> Result<(), String> {
    println!("keine Data.p4k in {} — nur der Ersatzweg wird geprüft", channel.display());
    println!("Spielordner laut Daten: {:?}", paths.game_folder());
    let (old_time, old) = median(|| catalog_cache::load(paths));
    let (new_time, new) = median(|| list::list_catalog_offline(paths));
    let denied = writeguard::take_denied();
    let old = old.ok_or("kein 4.12-Katalog in den Daten")?;
    println!("alt (4.12-Cache): {} Baupläne · Ladezeit {} (Median aus {RUNS})", old.blueprints.len(), ms(old_time));
    let new = new.ok_or("die Liste lädt keinen Katalog")?;
    println!(
        "neu (wie das Fenster): {} Baupläne · Ladezeit {} (Median aus {RUNS})",
        new.blueprints.len(),
        ms(new_time)
    );
    let differences = list::differences(&old, &new);
    for line in &differences {
        println!("{line}");
    }
    if !denied.is_empty() {
        return Err(format!("Schreibversuche in den Daten: {denied:?}"));
    }
    if differences.is_empty() {
        println!("GLEICH");
        Ok(())
    } else {
        Err(format!("UNTERSCHIEDE: {}", differences.len()))
    }
}

pub fn run(data: &str, channel: &str, counter: Counter) -> Result<(), String> {
    let scratch = std::env::temp_dir().join(format!("versekit-bauplan-quelle-{}", versekit_core::testdir::run_tag()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let cache = scratch.join(list::CACHE_FILE).to_string_lossy().into_owned();
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch.to_string_lossy()));
    let paths = paths_for(data)?;
    let channel = Path::new(channel);
    if !channel.join(versekit_gamedata::ARCHIVE_NAME).is_file() {
        if counter != Counter::Off {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(format!("Gegenprobe nicht möglich: keine Data.p4k in {}", channel.display()));
        }
        return fallback_only(&paths, channel);
    }

    let (old_time, old) = median(|| catalog_cache::load(&paths));
    let old = old.ok_or("kein 4.12-Katalog in den Daten")?;
    let started = Instant::now();
    let offline = || std::sync::Arc::new(list::Offline);
    let built = list::list_catalog_with(&paths, Some(channel), &cache, offline());
    let build_time = started.elapsed();
    if !Path::new(&cache).is_file() {
        return Err(format!("nicht aus der Data.p4k gelesen: {}", channel.display()));
    }
    let (hit_time, new) = median(|| list::list_catalog_with(&paths, Some(channel), &cache, offline()));
    let (none_time, fallback) = median(|| list::list_catalog_with(&paths, None, &cache, offline()));
    let mut new = new.ok_or("kein neuer Katalog")?;
    let denied = writeguard::take_denied();
    let _ = std::fs::remove_dir_all(&scratch);

    if built.as_ref() != Some(&new) {
        return Err("der gespeicherte Katalog liest sich anders zurück, als er gebaut wurde".into());
    }
    let marks = marks(&paths);
    let expected = match counter.mark() {
        Some(which) => {
            let line = drop_marked(&old, &mut new, &marks, which)
                .ok_or_else(|| format!("Gegenprobe nicht möglich: kein {} in den Daten", marks[which].0))?;
            println!("Gegenprobe: erwartet „{line}“");
            Some(line)
        }
        None => None,
    };

    let visible = |c: &CatalogCache| c.blueprints.iter().filter(|b| !b.no_way).count();
    println!("alt (4.12-Cache): {} Baupläne, {} mit Weg sichtbar", old.blueprints.len(), visible(&old));
    println!("neu (Data.p4k):   {} Baupläne, {} mit Weg sichtbar", new.blueprints.len(), visible(&new));
    println!("Ladezeit alt: {} (Median aus {RUNS})", ms(old_time));
    println!("Ladezeit neu, erster Start nach dem Patch (Archiv lesen + speichern): {}", ms(build_time));
    println!("Ladezeit neu, jeder weitere Start (Zwischenspeicher): {} (Median aus {RUNS})", ms(hit_time));
    println!("Ladezeit ohne Installation (Ersatzweg 4.12-Cache): {} (Median aus {RUNS})", ms(none_time));
    if fallback.as_ref() != Some(&old) {
        return Err("ohne Installation kommt nicht der 4.12-Cache".into());
    }

    let differences = list::differences(&old, &new);
    let lost = lost(&old, &new, &marks);
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
    for (what, count) in &kinds {
        println!("  {what}: {count}");
    }
    for line in differences.iter().map(ToString::to_string).chain(lost.iter().cloned()) {
        println!("{line}");
    }
    if !denied.is_empty() {
        return Err(format!("Schreibversuche in den Daten: {denied:?}"));
    }
    if let Some(line) = expected {
        return if lost.contains(&line) {
            println!("GEGENPROBE GREIFT: „{line}“ gemeldet");
            Ok(())
        } else {
            Err(format!("GEGENPROBE WIRKUNGSLOS: „{line}“ nicht gemeldet"))
        };
    }
    if differences.is_empty() && lost.is_empty() {
        println!("GLEICH");
        Ok(())
    } else {
        Err(format!("UNTERSCHIEDE: {} · verlorene Markierungen: {}", differences.len(), lost.len()))
    }
}
