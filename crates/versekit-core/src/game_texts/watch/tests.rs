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

//! The watch against stand-ins for the net, the archive, scmdb and the game, in a throwaway game folder.

use std::cell::Cell;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::super::tests::{FakeArchive, Setup};
use super::*;
use crate::net::Fake;
use crate::{injection, language};

thread_local! {
    /// Whether the stand-in game runs, for the watch of this thread.
    static GAME: Cell<bool> = const { Cell::new(false) };
}

fn game(_: &Paths) -> bool {
    GAME.with(Cell::get)
}

/// scmdb that counts the looks and has no recipes (or fails, as told).
#[derive(Default)]
struct CountingScmdb {
    asked: Mutex<usize>,
}

impl recipe_file::Supply for CountingScmdb {
    fn file(&self, name: &str) -> Result<Vec<u8>, String> {
        if name == "versions.json" {
            *self.asked.lock().unwrap() += 1;
        }
        Err(format!("{name}: missing"))
    }
}

fn watch(scmdb: Arc<CountingScmdb>) -> Watch {
    GAME.with(|g| g.set(false));
    let mut watch = Watch::new(Arc::new(Fake::default()), Arc::new(FakeArchive::default()), scmdb);
    watch.game_running = game;
    watch
}

/// Ticks at `now` until the run has ended, at most a minute; what it told.
fn settle(watch: &mut Watch, paths: &Arc<Paths>, now: f64) -> Vec<Notice> {
    let start = Instant::now();
    let mut told = watch.tick(paths, now);
    while watch.busy() {
        assert!(start.elapsed() < Duration::from_secs(60), "the run ends");
        std::thread::sleep(Duration::from_millis(5));
        told.extend(watch.tick(paths, now));
    }
    told
}

fn applied(told: &[Notice]) -> bool {
    told.iter().any(|notice| matches!(notice, Notice::Applied(_)))
}

/// The answer of a run that only looked at scmdb, with what came of the recipe file.
fn looked(recipes: Result<Updated, String>) -> Done {
    Done {
        ran: None,
        looked: Some(Looked { recipes: Some(recipes), craft: false }),
        catalog: None,
        refreshed: None,
        denied: Vec::new(),
    }
}

/// The only build the stand-in for scmdb has mining data for.
const MINING_BUILD: &str = "4.3.2-live.1";

/// scmdb that names a build and has the mining data of [`MINING_BUILD`]; it counts what was asked for, by name.
struct BuildScmdb {
    build: Mutex<String>,
    asked: Mutex<Vec<String>>,
}

impl BuildScmdb {
    fn new(build: &str) -> Arc<BuildScmdb> {
        Arc::new(BuildScmdb { build: Mutex::new(build.to_string()), asked: Mutex::default() })
    }

    fn asked_for(&self, part: &str) -> usize {
        self.asked.lock().unwrap().iter().filter(|name| name.contains(part)).count()
    }
}

impl recipe_file::Supply for BuildScmdb {
    fn file(&self, name: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(name.to_string());
        let build = self.build.lock().unwrap().clone();
        if name == "versions.json" {
            return Ok(format!(r#"[{{"version": "{build}"}}]"#).into_bytes());
        }
        if name == format!("mining_data-{MINING_BUILD}.json") {
            return Ok(br#"{"locations": [{"name": "Aberdeen"}], "refineries": [], "mineableElements": {}}"#.to_vec());
        }
        Err(format!("{name}: missing"))
    }
}

fn watch_with(scmdb: Arc<BuildScmdb>) -> Watch {
    GAME.with(|g| g.set(false));
    let mut watch = Watch::new(Arc::new(Fake::default()), Arc::new(FakeArchive::default()), scmdb);
    watch.game_running = game;
    watch
}

/// While the game runs the translation is still looked at every five minutes: a game session holds nothing back.
#[test]
fn while_the_game_runs_the_translation_is_looked_at_every_five_minutes() {
    let s = Setup::new("wache-spiel");
    s.set_up_german();
    let release = "https://api.github.com/repos/Dymerz/StarCitizen-Localization/releases/latest";
    let net = Arc::new(Fake::default());
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = Watch::new(net.clone(), Arc::new(FakeArchive::default()), Arc::new(CountingScmdb::default()));
    watch.game_running = game;
    GAME.with(|g| g.set(true));
    let looks = || net.asked().iter().filter(|url| *url == release).count();
    settle(&mut watch, &paths, 0.0);
    assert_eq!(looks(), 1, "looked at the start");
    for now in [60.0, 120.0, 299.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(looks(), 1, "not before five minutes");
    settle(&mut watch, &paths, 300.0);
    assert_eq!(looks(), 2, "five minutes later, with the game running");
    settle(&mut watch, &paths, 600.0);
    assert_eq!(looks(), 3, "and again");
    GAME.with(|g| g.set(false));
}

#[test]
fn the_start_repairs_the_language_once_and_writes_the_notes_at_once() {
    let s = Setup::new("wache-start");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    let told = settle(&mut watch, &paths, 0.0);
    let repaired = told.iter().filter(|notice| matches!(notice, Notice::LanguageRepaired(_))).count();
    assert_eq!(repaired, 1, "{told:?}");
    assert!(applied(&told), "{told:?}");
    assert!(injection::is_applied(&s.ini()));
    // Nothing changed: the next ticks stay quiet, also after the clock of the collection.
    for now in [1.0, 2.0, 40.0] {
        assert!(settle(&mut watch, &paths, now).is_empty(), "{now}");
    }
}

#[test]
fn a_running_run_starts_no_second_one() {
    let s = Setup::new("wache-einmal");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    watch.tick(&paths, 0.0);
    assert!(watch.busy());
    let flight = watch.in_flight.as_ref().map(|f| &f.done as *const _);
    watch.tick(&paths, 400.0);
    assert_eq!(watch.in_flight.as_ref().map(|f| &f.done as *const _), flight, "the same run, no second");
    settle(&mut watch, &paths, 400.0);
}

#[test]
fn the_end_of_a_game_session_looks_at_once() {
    let s = Setup::new("wache-spielende");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    GAME.with(|g| g.set(true));
    settle(&mut watch, &paths, 0.0);
    // Another tool rewrites nothing; the five minutes are far away. The game ends: the next look after it runs.
    assert!(!settle(&mut watch, &paths, 25.0).iter().any(|_| true));
    GAME.with(|g| g.set(false));
    settle(&mut watch, &paths, 50.0);
    assert_eq!(watch.upkeep.texts_next, 50.0 + super::super::TEXTS_POLL_SECONDS, "a timed run came at 50 s");
}

#[test]
fn a_thread_that_does_not_start_leaves_the_clocks_free() {
    let s = Setup::new("wache-faden");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    watch.fail_spawn = true;
    watch.tick(&paths, 0.0);
    assert!(!watch.busy());
    assert!(!watch.upkeep.running, "the upkeep is not left waiting for a run that never comes");
    assert_eq!(watch.recipes_next, RECIPES_RETRY_SECONDS);
    watch.fail_spawn = false;
    assert!(applied(&settle(&mut watch, &paths, 400.0)), "the next due run comes");
}

#[test]
fn another_writer_of_the_file_brings_one_look_and_then_quiet() {
    let s = Setup::new("wache-fremd");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    settle(&mut watch, &paths, 0.0);
    // A step of the main window writes the file outside the watch.
    injection::remove_texts(&paths, &s.ini(), "german_(germany)");
    assert!(!injection::is_applied(&s.ini()));
    let told = settle(&mut watch, &paths, 6.0);
    assert!(applied(&told), "the notes are back: {told:?}");
    for now in [12.0, 18.0] {
        assert!(settle(&mut watch, &paths, now).is_empty(), "its own writing counts not as another's: {now}");
    }
}

#[test]
fn the_recipe_file_is_looked_at_on_start_then_every_six_hours_and_soon_after_a_failure() {
    let s = Setup::new("wache-rezepte");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let scmdb = Arc::new(CountingScmdb::default());
    let mut watch = watch(Arc::clone(&scmdb));
    settle(&mut watch, &paths, 0.0);
    assert_eq!(*scmdb.asked.lock().unwrap(), 1, "at the start, also with nothing else set up");
    settle(&mut watch, &paths, 100.0);
    assert_eq!(*scmdb.asked.lock().unwrap(), 1, "not again before the retry");
    settle(&mut watch, &paths, 301.0);
    assert_eq!(*scmdb.asked.lock().unwrap(), 2, "a failed look is repeated after five minutes");
    // A look that found the file current waits six hours.
    watch.recipes_next = 400.0 + RECIPES_POLL_SECONDS;
    settle(&mut watch, &paths, 400.0 + RECIPES_POLL_SECONDS - 1.0);
    assert_eq!(*scmdb.asked.lock().unwrap(), 2);
}

/// The way of success: a look that found the file current comes again after six hours, not after the retry.
#[test]
fn a_current_recipe_file_is_looked_at_again_after_six_hours() {
    let s = Setup::new("wache-rezepte-aktuell");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    let (sender, receiver) = mpsc::channel();
    sender.send(looked(Ok(Updated::Current(2)))).unwrap();
    watch.in_flight = Some(InFlight { job: false, catalog: false, refresh: false, done: receiver });
    watch.started = true;
    watch.tick(&paths, 10.0);
    assert_eq!(watch.recipes_next, 10.0 + RECIPES_POLL_SECONDS);
    assert_ne!(paths.setting(SETTING_STOCK_MARK), Some(String::new()), "a current file leaves the notes as they are");
    settle(&mut watch, &paths, 10.0);
}

#[test]
fn the_collection_is_read_again_only_when_it_changed() {
    let s = Setup::new("wache-sammlung");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    settle(&mut watch, &paths, 0.0);
    let reads = watch.owned_reads;
    assert!(reads >= 1);
    for now in [1.0, 2.0, 3.0, 4.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(watch.owned_reads, reads, "an unchanged collection is not read every second");
    let file = paths.app_file(sources::COLLECTION_FILE);
    std::fs::write(&file, r#"{"bauplaene": {"neu_eins": {}, "neu_zwei": {}}}"#).unwrap();
    settle(&mut watch, &paths, 5.0);
    assert_eq!(watch.owned_reads, reads + 1, "a changed one is read at the next tick");
    assert!(watch.owned.as_ref().is_some_and(|known| known.keys.contains("neu_eins")));
}

#[test]
fn new_recipes_have_the_notes_written_anew() {
    let s = Setup::new("wache-neue-rezepte");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    settle(&mut watch, &paths, 0.0);
    assert!(!paths.setting(SETTING_STOCK_MARK).unwrap_or_default().is_empty());
    for (now, updated) in [(2.0, Updated::Written(3)), (3.0, Updated::Corrected(3))] {
        let (sender, receiver) = mpsc::channel();
        sender.send(looked(Ok(updated.clone()))).unwrap();
        watch.in_flight = Some(InFlight { job: false, catalog: false, refresh: false, done: receiver });
        let told = settle(&mut watch, &paths, now);
        assert!(applied(&told), "the notes come again with {updated:?}: {told:?}");
        assert_eq!(watch.recipes_next, now + RECIPES_POLL_SECONDS);
    }
}

#[test]
fn on_a_copy_of_the_data_the_run_writes_nothing_and_its_refused_writes_come_back() {
    let s = Setup::new("wache-kopie");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mut watch = watch(Arc::default());
    crate::writeguard::deny(true);
    settle(&mut watch, &paths, 0.0);
    let denied = watch.take_denied();
    crate::writeguard::deny(false);
    let _ = crate::writeguard::take_denied();
    assert!(!injection::is_applied(&s.ini()), "the lock of the caller holds on the thread of the run");
    assert!(denied.iter().any(|write| write.contains("global.ini")), "{denied:?}");
    assert!(watch.take_denied().is_empty(), "told once");
}

/// A build of the catalog that counts its runs and writes a file into the data folder, as the real one writes its
/// cache.
fn counting_catalog() -> (CatalogBuild, Arc<Mutex<usize>>) {
    failing_catalog(0)
}

/// Like [`counting_catalog`], whose first `fails` builds give no catalog.
fn failing_catalog(fails: usize) -> (CatalogBuild, Arc<Mutex<usize>>) {
    let built = Arc::new(Mutex::new(0));
    let count = Arc::clone(&built);
    let build: CatalogBuild = Arc::new(move |paths: &Paths| {
        let mut count = count.lock().unwrap();
        *count += 1;
        if *count <= fails {
            return None;
        }
        let _ =
            crate::jsonio::write_text_atomic(&paths.app_file("katalog-test.json"), "{}", crate::jsonio::Newline::Keep);
        Some(CatalogBuilt { version: "4.3.2-live.1".to_string(), blueprints: 7 })
    });
    (build, built)
}

thread_local! {
    /// Whether the stand-in archive may be read (the game is closed), for the watch of this thread.
    static FREE: Cell<bool> = const { Cell::new(true) };
}

fn archive_free(_: &Paths) -> bool {
    FREE.with(Cell::get)
}

#[test]
fn a_catalog_behind_at_the_start_is_built_once_the_game_is_closed_and_the_notes_are_written_anew() {
    let s = Setup::new("wache-katalog-rueckstand");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog_refresh(Arc::new(|_: &Paths| true), build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(false));
    assert!(applied(&settle(&mut watch, &paths, 0.0)), "the start writes the notes it has");
    for now in [1.0, 10.0, 60.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*built.lock().unwrap(), 0, "no build while the game runs");
    FREE.with(|free| free.set(true));
    settle(&mut watch, &paths, 70.0);
    assert_eq!(*built.lock().unwrap(), 1, "built once the game is closed, without a page open");
    let told = settle(&mut watch, &paths, 71.0);
    assert!(applied(&told), "the notes are written anew from the new catalog: {told:?}");
    for now in [72.0, 200.0, 1000.0] {
        assert!(!applied(&settle(&mut watch, &paths, now)), "once: {now}");
    }
    assert_eq!(*built.lock().unwrap(), 1, "built once");
}

/// A patch while the watch runs, with the game closed: the `Data.p4k` of the throwaway game folder changes. The
/// stand-in catalog lags behind the archive it was not built from, as the real one does, and its build writes the
/// catalog of the game data (`katalog-spieldaten.json`) that names the new contract of the patch. The look of the
/// notes at a patch is left out (the stand-in archive has no stamp), as it is when the notes are not kept current.
#[test]
fn a_patch_while_the_watch_runs_builds_the_catalog_from_the_game_data_and_writes_the_notes_from_it() {
    use super::super::tests::{TWO_CONTRACTS, own_catalog};
    let s = Setup::new("wache-patch-katalog");
    s.set_up_german();
    std::fs::write(s.ini(), TWO_CONTRACTS).unwrap();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let archive = paths.archive_file().expect("the archive of the game folder");
    let built_from = paths.app_file("gebaut-aus.txt");
    std::fs::write(&archive, "4.10.2-live.1").unwrap();
    std::fs::write(&built_from, "4.10.2-live.1").unwrap();
    std::fs::write(paths.app_file(sources::CATALOG_V5), own_catalog("4.10.2-live.1", &["Bounty_title_001"])).unwrap();
    let read = |file: &str| std::fs::read_to_string(file).unwrap_or_default();
    let (archive_of, from_of) = (archive.clone(), built_from.clone());
    let behind: CatalogCheck = Arc::new(move |_: &Paths| read(&archive_of) != read(&from_of));
    let count = Arc::new(Mutex::new(0));
    let (counted, archive_of) = (Arc::clone(&count), archive.clone());
    let build: CatalogBuild = Arc::new(move |paths: &Paths| {
        *counted.lock().unwrap() += 1;
        let version = read(&archive_of);
        let keys: &[&str] = if version.starts_with("4.10.3") {
            &["Bounty_title_001", "Bounty_title_002"]
        } else {
            &["Bounty_title_001"]
        };
        std::fs::write(paths.app_file(sources::CATALOG_V5), own_catalog(&version, keys)).ok()?;
        std::fs::write(paths.app_file("gebaut-aus.txt"), &version).ok()?;
        Some(CatalogBuilt { version, blueprints: 1 })
    });
    let mut watch = watch(Arc::default()).with_catalog_refresh(behind, build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(true));
    settle(&mut watch, &paths, 0.0);
    let before = std::fs::read_to_string(s.ini()).unwrap();
    assert!(before.contains("Kopfgeld <EM4>[BP]</EM4>") && !before.contains("Neues Kopfgeld <EM4>"), "{before}");
    assert_eq!(*count.lock().unwrap(), 0, "up to date at the start");
    // The launcher writes the new patch into the archive.
    std::fs::write(&archive, "4.10.3-live.2 patched").unwrap();
    for step in 1..=40 {
        settle(&mut watch, &paths, f64::from(step) * 5.0);
    }
    assert_eq!(*count.lock().unwrap(), 1, "built from the new archive, without a page open and without a restart");
    let after = std::fs::read_to_string(s.ini()).unwrap();
    assert!(after.contains("Neues Kopfgeld <EM4>[BP]</EM4>"), "the notes name the new contract: {after}");
    for now in [400.0, 1000.0, 4000.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*count.lock().unwrap(), 1, "once for that patch");
}

/// A build of the catalog that writes the catalog of the game data with both contracts of [`TWO_CONTRACTS`], and
/// counts its runs.
fn catalog_of_the_patch() -> (CatalogBuild, Arc<Mutex<usize>>) {
    use super::super::tests::own_catalog;
    let count = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&count);
    let build: CatalogBuild = Arc::new(move |paths: &Paths| {
        *counted.lock().unwrap() += 1;
        let catalog = own_catalog("4.10.3-live.2", &["Bounty_title_001", "Bounty_title_002"]);
        std::fs::write(paths.app_file(sources::CATALOG_V5), catalog).ok()?;
        Some(CatalogBuilt { version: "4.10.3-live.2".to_string(), blueprints: 1 })
    });
    (build, count)
}

#[test]
fn a_catalog_built_in_a_run_without_the_notes_has_them_written_with_the_next_tick() {
    use super::super::tests::{TWO_CONTRACTS, own_catalog};
    let s = Setup::new("wache-katalog-angaben");
    s.set_up_german();
    std::fs::write(s.ini(), TWO_CONTRACTS).unwrap();
    std::fs::write(s.paths.app_file(sources::CATALOG_V5), own_catalog("4.10.2-live.1", &["Bounty_title_001"])).unwrap();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, count) = catalog_of_the_patch();
    let mut watch = watch(Arc::default()).with_catalog(build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(true));
    assert!(applied(&settle(&mut watch, &paths, 0.0)));
    // A build that is due (a retry after a patch) in a run of its own, long before the next timed look at the texts.
    watch.catalog_due = true;
    settle(&mut watch, &paths, 10.0);
    assert_eq!(*count.lock().unwrap(), 1);
    let told = settle(&mut watch, &paths, 11.0);
    assert!(applied(&told), "{told:?}");
    assert!(std::fs::read_to_string(s.ini()).unwrap().contains("Neues Kopfgeld <EM4>[BP]</EM4>"));
}

#[test]
fn a_patch_and_a_catalog_behind_in_the_same_run_build_it_once() {
    let s = Setup::new("wache-katalog-einmal");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, count) = catalog_of_the_patch();
    let mut watch =
        watch(Arc::default()).with_catalog(Arc::clone(&build)).with_catalog_refresh(Arc::new(|_: &Paths| false), build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(true));
    settle(&mut watch, &paths, 0.0);
    watch.refresh_due = true;
    watch.catalog_due = true;
    settle(&mut watch, &paths, 10.0);
    assert_eq!(*count.lock().unwrap(), 1, "the catalog brought up to date serves the patch as well");
    assert!(!watch.catalog_due && !watch.refresh_due);
}

#[test]
fn a_catalog_up_to_date_at_the_start_is_not_built_anew() {
    let s = Setup::new("wache-katalog-aktuell");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog_refresh(Arc::new(|_: &Paths| false), build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(true));
    for now in [0.0, 1.0, 70.0, 1000.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*built.lock().unwrap(), 0);
}

#[test]
fn without_keeping_the_texts_current_the_catalog_is_built_but_the_notes_are_left_alone() {
    let s = Setup::new("wache-katalog-ohne-auto");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    paths.set_setting(super::super::SETTING_AUTO, Value::Bool(false));
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog_refresh(Arc::new(|_: &Paths| true), build);
    watch.archive_free = archive_free;
    FREE.with(|free| free.set(true));
    let mut told = Vec::new();
    for now in [0.0, 1.0, 2.0, 70.0] {
        told.extend(settle(&mut watch, &paths, now));
    }
    assert_eq!(*built.lock().unwrap(), 1, "the catalog is brought up to date all the same");
    assert!(!applied(&told), "{told:?}");
    assert!(!injection::is_applied(&s.ini()));
}

#[test]
fn a_catalog_build_that_failed_is_tried_again_after_five_minutes() {
    let s = Setup::new("wache-katalog-nochmal");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = failing_catalog(1);
    let mut watch = watch(Arc::default()).with_catalog(build);
    settle(&mut watch, &paths, 0.0);
    watch.upkeep.patch_at = 100.0;
    settle(&mut watch, &paths, 100.0);
    assert_eq!(*built.lock().unwrap(), 1, "the patch builds it, and it fails");
    for now in [101.0, 200.0, 100.0 + CATALOG_RETRY_SECONDS - 1.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*built.lock().unwrap(), 1, "not again before five minutes");
    settle(&mut watch, &paths, 100.0 + CATALOG_RETRY_SECONDS);
    assert_eq!(*built.lock().unwrap(), 2, "tried again after five minutes");
    assert!(std::path::Path::new(&paths.app_file("katalog-test.json")).is_file());
    for now in [500.0, 900.0, 2000.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*built.lock().unwrap(), 2, "a build that worked is not repeated");
}

#[test]
fn new_craft_data_at_scmdb_build_the_catalog_and_the_status_line_says_it_at_most_every_six_hours() {
    let s = Setup::new("wache-craftdaten");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let scmdb = BuildScmdb::new("4.3.2-live.1");
    let (build, built) = counting_catalog();
    let behind_of = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&behind_of);
    // The catalog lags behind every build it was not built for; it is built for the one named last.
    let current = Arc::new(Mutex::new(String::new()));
    let catalog_build = Arc::clone(&current);
    let behind: CraftCheck = Arc::new(move |_: &Paths, build: &str| {
        seen.lock().unwrap().push(build.to_string());
        *catalog_build.lock().unwrap() != build
    });
    let mark = Arc::clone(&current);
    let counted = Arc::clone(&build);
    let build: CatalogBuild = Arc::new(move |paths: &Paths| {
        let built = counted(paths)?;
        *mark.lock().unwrap() = built.version.clone();
        Some(built)
    });
    let mut watch = watch_with(Arc::clone(&scmdb)).with_catalog(build).with_craft_check(behind);
    let told = settle(&mut watch, &paths, 0.0);
    assert_eq!(*built.lock().unwrap(), 1, "the first look finds the catalog behind");
    let said: Vec<&Notice> = told.iter().filter(|n| matches!(n, Notice::CraftData { .. })).collect();
    assert_eq!(said, [&Notice::CraftData { build: "4.3.2-live.1".to_string(), items: 7 }], "{told:?}");
    language::set_thread_language(language::Choice::De);
    assert_eq!(said[0].text(), "scmdb-Craftdaten aktualisiert (4.3.2-live.1, 7 Gegenstände)");
    language::set_thread_language(language::Choice::Auto);
    // Within six hours the craft data are not looked at again, also when scmdb has a new build by now (the recipe
    // file, which this scmdb lacks, is asked for again after five minutes).
    *scmdb.build.lock().unwrap() = "4.3.3-live.2".to_string();
    for now in [400.0, 3600.0, RECIPES_POLL_SECONDS - 1.0] {
        assert!(!settle(&mut watch, &paths, now).iter().any(|n| matches!(n, Notice::CraftData { .. })));
    }
    assert!(scmdb.asked_for("versions.json") > 1, "the recipes were looked at again");
    assert_eq!(behind_of.lock().unwrap().len(), 1, "the craft data once");
    assert_eq!(*built.lock().unwrap(), 1);
    // After six hours: the new build is seen, the catalog built for it and told.
    let told = settle(&mut watch, &paths, RECIPES_POLL_SECONDS);
    assert_eq!(behind_of.lock().unwrap().len(), 2);
    assert_eq!(*built.lock().unwrap(), 2);
    assert!(told.iter().any(|n| matches!(n, Notice::CraftData { .. })), "{told:?}");
    assert_eq!(behind_of.lock().unwrap().last().map(String::as_str), Some("4.3.3-live.2"));
}

#[test]
fn current_craft_data_build_nothing_and_say_nothing() {
    let s = Setup::new("wache-craftdaten-aktuell");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let scmdb = BuildScmdb::new("4.3.2-live.1");
    let (build, built) = counting_catalog();
    let mut watch = watch_with(scmdb).with_catalog(build).with_craft_check(Arc::new(|_: &Paths, _: &str| false));
    let told = settle(&mut watch, &paths, 0.0);
    assert_eq!(*built.lock().unwrap(), 0);
    assert!(!told.iter().any(|n| matches!(n, Notice::CraftData { .. })), "{told:?}");
}

#[test]
fn a_failed_build_for_new_craft_data_is_told_when_the_retry_works() {
    let s = Setup::new("wache-craftdaten-nochmal");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = failing_catalog(1);
    let mut watch = watch_with(BuildScmdb::new("4.3.2-live.1"))
        .with_catalog(build)
        .with_craft_check(Arc::new(|_: &Paths, _: &str| true));
    let told = settle(&mut watch, &paths, 0.0);
    assert_eq!(*built.lock().unwrap(), 1);
    assert!(!told.iter().any(|n| matches!(n, Notice::CraftData { .. })), "no catalog, nothing to tell");
    let told = settle(&mut watch, &paths, CATALOG_RETRY_SECONDS);
    assert_eq!(*built.lock().unwrap(), 2);
    assert!(told.iter().any(|n| matches!(n, Notice::CraftData { .. })), "{told:?}");
}

/// A patch while VerseKit was closed leaves the mining data of the old build: the first look at scmdb brings those of
/// the build it names, without a patch seen by the running process and without a page open.
#[test]
fn the_mining_data_of_an_older_build_are_fetched_at_the_start_without_a_patch_seen() {
    let s = Setup::new("wache-bergbau");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let mining_file = paths.app_file(crate::stock_data::MINING_FILE);
    let old =
        r#"{"format": 1, "build": "4.2.0-live.0", "locations": [{"name": "Old"}], "refineries": [], "elemente": {}}"#;
    std::fs::write(&mining_file, old).unwrap();
    assert!(!crate::stock_fetch::mining_current(&paths, MINING_BUILD));
    let scmdb = BuildScmdb::new(MINING_BUILD);
    let mut watch = watch_with(Arc::clone(&scmdb));
    settle(&mut watch, &paths, 0.0);
    assert_eq!(scmdb.asked_for("mining_data-4.3.2-live.1"), 1, "the start fetches those of the build");
    assert!(crate::stock_fetch::mining_current(&paths, MINING_BUILD), "and keeps them");
    for now in [1.0, 400.0, RECIPES_POLL_SECONDS, 2.0 * RECIPES_POLL_SECONDS] {
        settle(&mut watch, &paths, now);
    }
    assert!(scmdb.asked_for("versions.json") >= 2, "scmdb was looked at again");
    assert_eq!(scmdb.asked_for("mining_data-"), 1, "current data are not fetched again");
}

/// The build at scmdb changes while the watch runs and no patch is seen (the game is patched while VerseKit is
/// closed, or the archive could not be read): the next look still brings the mining data of the new build.
#[test]
fn a_new_build_at_scmdb_brings_its_mining_data_with_the_next_look_whether_or_not_a_patch_was_seen() {
    let s = Setup::new("wache-bergbau-neuer-stand");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let scmdb = BuildScmdb::new("4.2.0-live.0");
    let mut watch = watch_with(Arc::clone(&scmdb));
    settle(&mut watch, &paths, 0.0);
    assert_eq!(scmdb.asked_for("mining_data-"), 1, "scmdb has none for that build");
    *scmdb.build.lock().unwrap() = MINING_BUILD.to_string();
    settle(&mut watch, &paths, RECIPES_POLL_SECONDS);
    assert_eq!(scmdb.asked_for("mining_data-4.3.2-live.1"), 1);
    assert!(crate::stock_fetch::mining_current(&paths, MINING_BUILD));
}

#[test]
fn mining_data_that_could_not_be_had_are_asked_for_with_the_next_look() {
    let s = Setup::new("wache-bergbau-nochmal");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let scmdb = BuildScmdb::new("4.3.2-live.1");
    let mut watch = watch_with(Arc::clone(&scmdb));
    settle(&mut watch, &paths, 0.0);
    // scmdb names a build it has no mining data for.
    *scmdb.build.lock().unwrap() = "9.9.9-live.9".to_string();
    watch.upkeep.patch_at = 100.0;
    settle(&mut watch, &paths, 100.0);
    assert_eq!(scmdb.asked_for("mining_data-9.9.9"), 1);
    *scmdb.build.lock().unwrap() = "4.3.2-live.1".to_string();
    settle(&mut watch, &paths, 100.0 + RECIPES_POLL_SECONDS);
    assert_eq!(scmdb.asked_for("mining_data-4.3.2-live.1"), 1, "the next look fetches them");
}

#[test]
fn the_catalog_is_built_anew_once_after_each_finished_patch_and_never_without_one() {
    let s = Setup::new("wache-katalog");
    s.set_up_german();
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog(build);
    settle(&mut watch, &paths, 0.0);
    settle(&mut watch, &paths, 400.0);
    assert_eq!(*built.lock().unwrap(), 0, "the start and the timed runs build no catalog");
    watch.upkeep.patch_at = 500.0;
    settle(&mut watch, &paths, 500.0);
    assert_eq!(*built.lock().unwrap(), 1, "the finished patch builds it");
    assert!(std::path::Path::new(&paths.app_file("katalog-test.json")).is_file());
    for now in [501.0, 900.0, 1300.0] {
        settle(&mut watch, &paths, now);
    }
    assert_eq!(*built.lock().unwrap(), 1, "once for that patch, also in the runs every five minutes after it");
    watch.upkeep.patch_at = 2000.0;
    settle(&mut watch, &paths, 2000.0);
    assert_eq!(*built.lock().unwrap(), 2, "the next patch builds it again");
}

#[test]
fn a_catalog_whose_thread_did_not_start_is_built_with_the_next_run() {
    let s = Setup::new("wache-katalog-faden");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog(build);
    settle(&mut watch, &paths, 0.0);
    watch.upkeep.patch_at = 100.0;
    watch.fail_spawn = true;
    watch.tick(&paths, 100.0);
    assert_eq!(*built.lock().unwrap(), 0);
    watch.fail_spawn = false;
    settle(&mut watch, &paths, 101.0);
    assert_eq!(*built.lock().unwrap(), 1, "still due after the thread failed");
}

#[test]
fn on_a_copy_of_the_data_the_catalog_writes_nothing_and_its_refused_writes_come_back() {
    let s = Setup::new("wache-katalog-kopie");
    let paths = Arc::new(Paths::new(s.paths.host.clone()));
    let target = paths.app_file("katalog-test.json");
    let (build, built) = counting_catalog();
    let mut watch = watch(Arc::default()).with_catalog(build);
    settle(&mut watch, &paths, 0.0);
    watch.upkeep.patch_at = 100.0;
    crate::writeguard::deny(true);
    settle(&mut watch, &paths, 100.0);
    let denied = watch.take_denied();
    crate::writeguard::deny(false);
    let _ = crate::writeguard::take_denied();
    assert_eq!(*built.lock().unwrap(), 1);
    assert!(!std::path::Path::new(&target).exists(), "the lock of the caller holds for the catalog");
    assert!(denied.iter().any(|write| write.contains("katalog-test.json")), "{denied:?}");
}

#[test]
fn without_the_net_nothing_is_fetched_and_nothing_runs() {
    let s = Setup::new("wache-offline");
    s.set_up_german();
    let host = {
        let mut host = s.paths.host.clone();
        host.set_var("SC_BP_NO_NET", "1");
        host
    };
    let paths = Arc::new(Paths::new(host));
    let scmdb = Arc::new(CountingScmdb::default());
    let mut watch = watch(Arc::clone(&scmdb));
    settle(&mut watch, &paths, 0.0);
    assert!(!watch.busy());
    assert_eq!(*scmdb.asked.lock().unwrap(), 0);
    assert!(!injection::is_applied(&s.ini()));
}
