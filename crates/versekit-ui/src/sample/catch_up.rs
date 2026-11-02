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

//! Reading back the saved game logs for the setup assistant (`wizard._reread` of 4.12): the blueprints the
//! player received in earlier sessions go into the collection, and the assistant says how many and from how
//! many sessions. The overlay does the same quietly when it starts (`watcher::run`, the number in its status);
//! at the first start the overlay is not there yet, and the player should see the result before the last step.
//!
//! The read position is saved by the reading itself, so the overlay that starts afterwards finds the logs
//! known and reads nothing twice. For that reason nothing is read when the data folder cannot be written:
//! sessions marked as read whose finds were not saved would be lost for good.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use versekit_core::catalog_cache::CatalogCache;
use versekit_core::collection::{self, Catalog, Collection, Known};
use versekit_core::export::{self, FileExports};
use versekit_core::language::{Arg, Phrase};
use versekit_core::logsource::{LogSource, ReadState};
use versekit_core::paths::Paths;
use versekit_core::phrases::{self, BackRoute, NoArchive};
use versekit_core::{report_facts, writeguard};

/// The catalog the collection matches names against.
struct CacheCatalog<'a>(Option<&'a CatalogCache>);

impl Catalog for CacheCatalog<'_> {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::new(self.0.iter().flat_map(|c| c.blueprints.iter()).map(|b| (b.key.clone(), Some(b.name.clone())))))
    }
}

/// What the reading found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatchUp {
    /// Blueprints that were not in the collection yet.
    pub added: usize,
    /// Saved sessions read.
    pub files: usize,
    /// The hint when the kept logs have a gap (rendered where it is shown, in the language of that moment).
    pub gap: Option<Phrase>,
    /// Is everything that was found saved? False: nothing was read, or the collection could not be written.
    pub saved: bool,
    /// How many blueprints the collection holds after the reading.
    pub total: usize,
}

/// How many blueprints the collection of `paths` holds (the number on the last page of the assistant when
/// the reading step was not visited).
pub fn stock_count(paths: &Paths) -> usize {
    let names = BackRoute { paths, archive: &NoArchive };
    let exports = FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection = Collection {
        paths,
        catalog: &collection::NoCatalog,
        names: &names,
        exports: &exports,
        clock: collection::now_text,
    };
    collection::count(&collection.load())
}

/// The reading as the assistant holds it: started once per game folder and data folder when the step is first
/// shown.
#[derive(Default)]
pub struct Reading {
    /// The folders the reading was started for.
    key: Option<String>,
    work: Work,
}

#[derive(Default)]
enum Work {
    #[default]
    Idle,
    Running(Receiver<CatchUp>),
    Done(CatchUp),
}

impl Reading {
    /// Does a thread work right now?
    pub fn working(&self) -> bool {
        matches!(self.work, Work::Running(_))
    }

    /// Drops a finished result (the data folder may be another now); a thread that works is left alone.
    pub fn forget(&mut self) {
        if !self.working() {
            *self = Reading::default();
        }
    }

    /// The number of blueprints after a finished reading.
    pub fn total(&self) -> Option<usize> {
        match &self.work {
            Work::Done(done) if done.saved => Some(done.total),
            _ => None,
        }
    }

    /// Starts the reading for `game` (once per pair of folders, in a thread of its own under the lock of writing
    /// of this one; a thread that still works is waited for, never run next to a second one) and takes its
    /// result when it is there. `None` while it works.
    pub fn poll(&mut self, ctx: &egui::Context, paths: &Paths, game: &str) -> Option<&CatchUp> {
        let key = format!("{game}\n{}", paths.app_folder());
        if self.key.as_deref() != Some(key.as_str()) && !self.working() {
            self.key = Some(key.clone());
            self.work = Work::Idle;
        }
        if matches!(self.work, Work::Idle) {
            let own = paths.working_copy();
            let guard = writeguard::state();
            let (tx, rx) = mpsc::channel();
            let wake = ctx.clone();
            let started = std::thread::Builder::new().name("nachlese".into()).spawn(move || {
                writeguard::adopt(&guard);
                let _ = tx.send(catch_up(&own));
                wake.request_repaint();
            });
            self.work = if started.is_ok() { Work::Running(rx) } else { Work::Done(CatchUp::default()) };
        }
        if let Work::Running(rx) = &self.work {
            match rx.try_recv() {
                Ok(done) => self.work = Work::Done(done),
                // The thread ended without an answer: nothing is known, and that is said as "not saved".
                Err(TryRecvError::Disconnected) => self.work = Work::Done(CatchUp::default()),
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        match &self.work {
            // The result of another pair of folders is not shown for this one (the next call starts anew).
            Work::Done(done) if self.key.as_deref() == Some(key.as_str()) => Some(done),
            Work::Done(_) => {
                ctx.request_repaint();
                None
            }
            _ => None,
        }
    }
}

/// Reads the saved logs and the log of the running game into the collection of `paths`.
///
/// Slow (the catalog may have to be built from the archive of the game): for a thread of its own.
pub fn catch_up(paths: &Paths) -> CatchUp {
    // Nothing is marked as read where the finds cannot be saved.
    if report_facts::storage_status(&paths.app_folder()).is_err() {
        return CatchUp::default();
    }
    let catalog = versekit_gamedata::list::list_catalog_offline(paths);
    let cache = CacheCatalog(catalog.as_ref());
    let names = BackRoute { paths, archive: &NoArchive };
    let exports = FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection =
        Collection { paths, catalog: &cache, names: &names, exports: &exports, clock: collection::now_text };
    let sources = LogSource::sources_of(paths);
    let pattern = phrases::pattern(&sources.collect().0);
    let source = LogSource::new(paths, sources);
    // Reading takes seconds, in which the window may have written: the collection is loaded after it.
    let mut state = ReadState::open(paths);
    let (finds, report) = source.read_backlog(&mut state, &pattern, true, true);
    // Load, change and save under the write lock, so that neither a page of this window nor the overlay (another
    // process) writes the collection in between and loses a find.
    let lock = versekit_core::jsonio::lock_saves();
    let mut data = collection.load();
    let mut changed = collection.align(&mut data) > 0;
    // The blueprints everybody has from the start stand in no log.
    for blueprint in catalog.iter().flat_map(|c| c.blueprints.iter()).filter(|b| b.start) {
        changed |= collection.add(&mut data, &blueprint.name, "start", None);
    }
    let mut added = 0;
    for (name, _) in &finds {
        if collection.add(&mut data, name, "nachlese", None) {
            added += 1;
        }
    }
    let saved = !(changed || added > 0) || collection.save(&mut data);
    drop(lock);
    let gap = report.gap.as_ref().map(|gap| {
        gap.moments()
            .into_iter()
            .enumerate()
            .fold(Phrase::new(gap.key()), |phrase, (i, at)| phrase.with(&i.to_string(), Arg::moment(at)))
    });
    CatchUp { added, files: report.files, gap, saved, total: collection::count(&data) }
}

#[cfg(test)]
mod tests {
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::*;

    /// A throwaway data folder and a game folder with a log that holds two received blueprints.
    fn setup(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-nachlese-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let game = dir.join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        let line = |find: &str| {
            format!(
                "<2026-10-01T12:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \
                 \"Received Blueprint: {find}: \" [1]\n"
            )
        };
        std::fs::write(game.join("Game.log"), format!("{}{}", line("Arclight Pistol"), line("Pembroke Helmet")))
            .unwrap();
        let root = dir.to_string_lossy().replace('\\', "/");
        let data = format!("{root}/daten");
        std::fs::create_dir_all(&data).unwrap();
        let live = game.to_string_lossy().replace('\\', "/");
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &live)],
            &root,
        );
        (Paths::new(host), dir)
    }

    /// Frames until the thread of the reading has answered.
    fn answer(reading: &mut Reading, paths: &Paths, game: &str) -> CatchUp {
        let ctx = egui::Context::default();
        for _ in 0..500 {
            if let Some(done) = reading.poll(&ctx, paths, game) {
                return done.clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the reading did not answer");
    }

    #[test]
    fn the_logs_of_earlier_sessions_go_into_the_collection_and_are_counted_once() {
        let (paths, dir) = setup("zaehlt");
        let first = catch_up(&paths);
        assert_eq!((first.added, first.total, first.saved), (2, 2, true), "{first:?}");
        let stock = collection::Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &phrases::BackRoute { paths: &paths, archive: &NoArchive },
            exports: &FileExports { paths: &paths, version: "5.0.0", clock: export::now_unix },
            clock: collection::now_text,
        }
        .load();
        assert!(collection::contains(&stock, "Arclight Pistol") && collection::contains(&stock, "Pembroke Helmet"));
        // Again: nothing new.
        let again = catch_up(&paths);
        assert_eq!((again.added, again.total), (0, 2), "{again:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_log_nothing_is_added_and_nothing_breaks() {
        let (paths, dir) = setup("leer");
        std::fs::remove_file(dir.join("StarCitizen").join("LIVE").join("Game.log")).unwrap();
        let result = catch_up(&paths);
        assert_eq!((result.added, result.files, result.saved), (0, 0, true), "{result:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn where_nothing_can_be_saved_nothing_is_read_and_it_is_said() {
        let (paths, dir) = setup("gesperrt");
        // A file where the data folder should be: nothing can be written there.
        let data = paths.app_folder();
        std::fs::remove_dir_all(&data).unwrap();
        std::fs::write(&data, "x").unwrap();
        let result = catch_up(&paths);
        assert!(!result.saved && result.added == 0, "{result:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_thread_keeps_the_lock_of_writing_of_the_window() {
        let (paths, dir) = setup("sperre");
        let mut reading = Reading::default();
        // The window may not write at all: the thread has to find the same lock, so it saves nothing.
        writeguard::deny(true);
        let denied = answer(&mut reading, &paths, "game");
        writeguard::deny(false);
        let _ = writeguard::take_denied();
        assert!(!denied.saved, "{denied:?}");
        // Free again, for a new pair of folders: it reads and saves.
        let free = answer(&mut reading, &paths, "other");
        assert!(free.saved && free.added == 2 && reading.total() == Some(2), "{free:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_reading_waits_for_a_writer_of_the_lock_and_does_not_lose_its_entry() {
        use std::sync::mpsc::channel;
        let (paths, dir) = setup("schreiber");
        let (loaded_tx, loaded_rx) = channel();
        let (release_tx, release_rx) = channel::<()>();
        std::thread::scope(|scope| {
            // The other writer (another thread here, the overlay or a page in the program): loads the
            // collection under the lock, and writes its change only after the reading has had its chance.
            let paths = &paths;
            let writer = scope.spawn(move || {
                let _lock = versekit_core::jsonio::lock_saves();
                let stock = collection::Collection {
                    paths,
                    catalog: &collection::NoCatalog,
                    names: &phrases::BackRoute { paths, archive: &NoArchive },
                    exports: &FileExports { paths, version: "5.0.0", clock: export::now_unix },
                    clock: collection::now_text,
                };
                let mut data = stock.load();
                loaded_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                assert!(stock.add(&mut data, "Hand Visor", "hand", None));
                assert!(stock.save(&mut data));
            });
            loaded_rx.recv().unwrap();
            let (done_tx, done_rx) = channel();
            let reading = scope.spawn(move || done_tx.send(catch_up(paths)).unwrap());
            assert!(
                done_rx.recv_timeout(std::time::Duration::from_millis(500)).is_err(),
                "the reading waits for the one who holds the lock"
            );
            release_tx.send(()).unwrap();
            let result = done_rx.recv_timeout(std::time::Duration::from_secs(30)).expect("the reading goes on");
            assert!(result.saved && result.added == 2, "{result:?}");
            writer.join().unwrap();
            reading.join().unwrap();
        });
        let stock = collection::Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &phrases::BackRoute { paths: &paths, archive: &NoArchive },
            exports: &FileExports { paths: &paths, version: "5.0.0", clock: export::now_unix },
            clock: collection::now_text,
        }
        .load();
        for name in ["Hand Visor", "Arclight Pistol", "Pembroke Helmet"] {
            assert!(collection::contains(&stock, name), "{name} is lost");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_changed_game_folder_waits_for_the_thread_that_works_and_then_reads_anew() {
        let (paths, dir) = setup("wechsel");
        let ctx = egui::Context::default();
        let mut reading = Reading::default();
        assert!(reading.poll(&ctx, &paths, "one").is_none() || !reading.working());
        // Asked for another folder while the first may still work: never a result of the first for the second.
        for _ in 0..500 {
            if reading.poll(&ctx, &paths, "two").is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(reading.key.as_deref().map(|k| k.starts_with("two")), Some(true));
        assert!(!reading.working());
        let _ = std::fs::remove_dir_all(dir);
    }
}
