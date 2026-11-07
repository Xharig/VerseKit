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

//! `versekit-parity katalog_start <daten> <kanalordner> [--gegenprobe]`:
//! how long the blueprint list waits for its catalog on the first start
//! after a patch, with the real `Data.p4k` in `<kanalordner>`.
//!
//! `<daten>` is a copy of the user's data (holding `VerseKit-Daten`) or a
//! data folder of its own; it is only read. The cache of the built catalog
//! goes into a throwaway folder, and a patch is made there by marking the
//! cache as built from another stand of the archive.
//!
//! The run checks that the window may draw within [`FIRST_IMAGE_LIMIT`] of
//! asking for its catalog (the stale cache of the archive, or without one
//! the 4.12 cache) and has the build running in a thread of its own, that it
//! keeps answering while the build runs, that the build brings the same
//! catalog as a build that is waited for, that the thread wrote nothing the
//! write lock refused, and that the next start finds the cache current.
//!
//! `--gegenprobe` does the build before it is handed to the thread. The run
//! then succeeds only when the first image exceeds the limit
//! (`GEGENPROBE GREIFT`).

use std::path::Path;
use std::time::{Duration, Instant};

use versekit_core::catalog_cache::{self, CatalogCache};
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::writeguard;
use versekit_gamedata::list::{self, Progress};

/// The measurement runs without the network.
fn offline() -> std::sync::Arc<list::Offline> {
    std::sync::Arc::new(list::Offline)
}

/// The longest the list may take until it has a catalog to show.
const FIRST_IMAGE_LIMIT: Duration = Duration::from_millis(100);

/// The longest gap between two looks of the window while the build runs; a
/// window that waits for the build misses its looks for the whole build.
const GAP_LIMIT: Duration = Duration::from_millis(250);

/// How often a start is repeated; the median counts.
const RUNS: usize = 5;

/// How often the waiting loop looks at the build, standing for the frames of the window.
const FRAME: Duration = Duration::from_millis(5);

fn ms(time: Duration) -> String {
    format!("{:.1} ms", time.as_secs_f64() * 1000.0)
}

fn paths_for(data: &str) -> Result<Paths, String> {
    if Path::new(data).join(COPY_DATA_FOLDER).is_dir() {
        return Paths::for_copy(Host::current(), data).ok_or_else(|| format!("keine Datenkopie: {data}"));
    }
    Ok(Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, data)], data)))
}

fn median<T>(mut run: impl FnMut() -> T) -> (Duration, T) {
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..RUNS {
        let started = Instant::now();
        last = Some(run());
        times.push(started.elapsed());
    }
    times.sort();
    (times[times.len() / 2], last.expect("at least one run"))
}

/// What a patch does to the cache: it was built from another stand of the archive.
fn make_stale(cache: &str) -> Result<(), String> {
    let Value::Object(mut data) = jsonio::read_json(cache).map_err(|e| format!("Zwischenspeicher: {e:?}"))? else {
        return Err("der Zwischenspeicher ist kein Objekt".into());
    };
    let Some((_, Value::Object(source))) = data.iter_mut().find(|(k, _)| k == "quelle") else {
        return Err("der Zwischenspeicher hat keinen Stempel".into());
    };
    jsonio::map_set(source, "archiv_stand", Value::Null);
    let text = jsonio::dumps(&Value::Object(data), None, false);
    jsonio::write_text_atomic(cache, &text, jsonio::Newline::Native).map_err(|e| e.to_string())
}

/// What the window does at the start after a patch: ask for the catalog to
/// show and have the build run in the background. Returns the time it took
/// until the window may draw, the catalog to show and the running build.
/// With `sabotage` the build is done before it is handed over.
fn begin(
    paths: &Paths,
    channel: &Path,
    cache: &str,
    sabotage: bool,
) -> Result<(Duration, Option<CatalogCache>, list::Pending), String> {
    let started = Instant::now();
    let start = list::start_catalog_with(paths, Some(channel), cache, offline());
    let rebuild = start.rebuild.ok_or("nach dem Patch ist kein Neubau fällig")?;
    let pending = if sabotage {
        let built = rebuild.run();
        list::Pending::spawn(move || built)
    } else {
        rebuild.spawn()
    }
    .ok_or("kein Faden")?;
    Ok((started.elapsed(), start.catalog, pending))
}

pub fn run(data: &str, channel: &str, sabotage: bool) -> Result<(), String> {
    let channel = Path::new(channel);
    if !channel.join(versekit_gamedata::ARCHIVE_NAME).is_file() {
        return Err(format!("keine Data.p4k in {}", channel.display()));
    }
    let scratch = std::env::temp_dir().join(format!("versekit-katalog-start-{}", versekit_core::testdir::run_tag()));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let cache = scratch.join(list::CACHE_FILE).to_string_lossy().into_owned();
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch.to_string_lossy()));
    let mut denied = Vec::new();
    let result = paths_for(data).and_then(|paths| measure(&paths, channel, &cache, sabotage, &mut denied));
    denied.extend(writeguard::take_denied());
    let _ = std::fs::remove_dir_all(&scratch);
    if !denied.is_empty() {
        return Err(format!("Schreibversuche in den Daten: {denied:?}"));
    }
    result
}

fn measure(paths: &Paths, channel: &Path, cache: &str, sabotage: bool, denied: &mut Vec<String>) -> Result<(), String> {
    let legacy = catalog_cache::load(paths).ok_or("kein 4.12-Katalog in den Daten")?;

    // The build that is waited for: the way the first start after a patch went before.
    let started = Instant::now();
    let waited =
        list::list_catalog_with(paths, Some(channel), cache, offline()).ok_or("kein Katalog aus der Data.p4k")?;
    let wait_time = started.elapsed();
    if !Path::new(cache).is_file() {
        return Err(format!("nicht aus der Data.p4k gelesen: {}", channel.display()));
    }
    println!("Neubau, auf den gewartet wird (Archiv lesen + speichern): {}", ms(wait_time));

    // A patch: the cache now belongs to another stand of the archive. The
    // start alone, over and over, as the median of the first image.
    make_stale(cache)?;
    let (start_time, shown) = median(|| list::start_catalog_with(paths, Some(channel), cache, offline()).catalog);
    println!("Start allein, sofort ohne das Archiv: {} (Median aus {RUNS})", ms(start_time));
    if shown.as_ref() != Some(&waited) {
        return Err("der alte Stand ist nicht der zuletzt gebaute".into());
    }

    // The way the window goes: start and hand the build to its thread.
    let (first_time, shown, pending) = begin(paths, channel, cache, sabotage)?;
    println!(
        "erstes Bild nach dem Patch (Start + Übergabe an den Faden{}): {} (Grenze {})",
        if sabotage { ", Gegenprobe: Bau vorher im Vordergrund" } else { "" },
        ms(first_time),
        ms(FIRST_IMAGE_LIMIT)
    );
    if sabotage {
        let late = first_time > FIRST_IMAGE_LIMIT;
        // The thread of the sabotage did nothing; it is over at once.
        let _ = pending.poll();
        return if late {
            println!("GEGENPROBE GREIFT: {} über der Grenze von {}", ms(first_time), ms(FIRST_IMAGE_LIMIT));
            Ok(())
        } else {
            Err(format!("GEGENPROBE WIRKUNGSLOS: {} unter der Grenze von {}", ms(first_time), ms(FIRST_IMAGE_LIMIT)))
        };
    }
    if shown.as_ref() != Some(&waited) {
        return Err("der alte Stand ist nicht der zuletzt gebaute".into());
    }
    if first_time > FIRST_IMAGE_LIMIT || start_time > FIRST_IMAGE_LIMIT {
        return Err(format!("ZU LANGSAM: {} bis zum ersten Bild, Grenze {}", ms(first_time), ms(FIRST_IMAGE_LIMIT)));
    }

    // The build in the background while the "window" keeps answering.
    let (started, mut frames, mut longest, mut last) = (Instant::now(), 0_u32, Duration::ZERO, Instant::now());
    let built = loop {
        match pending.poll() {
            Progress::Done(built) => break built.ok_or("der Neubau im Hintergrund scheiterte")?,
            Progress::Waiting => {
                std::thread::sleep(FRAME);
                frames += 1;
                longest = longest.max(last.elapsed());
                last = Instant::now();
            }
        }
        if started.elapsed() > Duration::from_secs(60) {
            return Err("der Neubau im Hintergrund wurde nicht fertig".into());
        }
    };
    println!(
        "Neubau im Hintergrund: {} · währenddessen {frames} Bilder, längste Lücke {}",
        ms(started.elapsed()),
        ms(longest)
    );
    denied.extend(pending.take_denied());
    if frames == 0 {
        return Err("der Neubau war schon fertig, bevor das erste Bild kam: er lief nicht im Hintergrund".into());
    }
    if longest > GAP_LIMIT {
        return Err(format!("das Fenster hing während des Neubaus: längste Lücke {}", ms(longest)));
    }
    if built != waited {
        return Err("der Neubau im Hintergrund bringt einen anderen Katalog als der, auf den gewartet wird".into());
    }

    // The next start finds the cache current and has nothing to build.
    let (next_time, next) = median(|| list::start_catalog_with(paths, Some(channel), cache, offline()));
    if next.rebuild.is_some() || next.catalog.as_ref() != Some(&built) {
        return Err("der nächste Start findet den Zwischenspeicher nicht aktuell".into());
    }
    println!("nächster Start (Zwischenspeicher): {} (Median aus {RUNS})", ms(next_time));

    // The very first start, with the game installed but no cache of its own: the 4.12 cache stands in.
    std::fs::remove_file(cache).map_err(|e| e.to_string())?;
    let (bare_time, bare) = median(|| list::start_catalog_with(paths, Some(channel), cache, offline()));
    if bare.catalog.as_ref() != Some(&legacy) || bare.rebuild.is_none() {
        return Err("ohne eigenen Zwischenspeicher kommt nicht der 4.12-Katalog mit fälligem Neubau".into());
    }
    println!("erster Start ohne eigenen Zwischenspeicher (4.12-Cache): {} (Median aus {RUNS})", ms(bare_time));
    if bare_time > FIRST_IMAGE_LIMIT {
        return Err(format!("ZU LANGSAM: {} ohne eigenen Zwischenspeicher", ms(bare_time)));
    }
    println!("GLEICH");
    Ok(())
}
