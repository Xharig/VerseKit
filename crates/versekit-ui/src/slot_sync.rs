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

//! The one way to the slots of erkul.games (`pages._fetch_slots`): every page that wants the slots of the own
//! ships asks here, and here it is decided whether a fetch runs.
//!
//! * Only one fetch runs at a time. A request while it runs leaves at most one more run behind it, which then
//!   sees what changed meanwhile (a ship entered while the first run was on its way).
//! * Opening the hangar page and a blueprint without data ask only when a ship lacks its slots.
//! * Opening the hangar page fetches once per program run for one list of ships (hangar and wishlist) and one
//!   game build: a concept ship never gets slots and would make every entering fetch again.
//! * A blueprint without data asks at most once per program run: for a concept ship there is never anything to
//!   fetch, and a click on a blueprint must not start a fetch that brought nothing the last time. A new wish
//!   does not care about that lock: its ship has just come in.
//!
//! The fetch itself is [`crate::trade::fetch_slots`] on its own thread; the pages read the store again when the
//! file changed.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use versekit_core::erkul_fetch::{self, Feed, ShipRow};
use versekit_core::paths::Paths;

/// Why the slots are asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The quiet fetches at the start of the program.
    Start,
    /// A hangar export was imported.
    Import,
    /// A ship was entered by hand.
    ByHand,
    /// The hangar page was entered.
    PageOpen,
    /// A ship was put on the wishlist.
    Wish,
    /// The crafting page found no slot data for a blueprint.
    BlueprintNoData,
}

impl Trigger {
    /// Whether the trigger asks only when a ship lacks its slots.
    fn only_when_unknown(self) -> bool {
        matches!(self, Trigger::PageOpen | Trigger::BlueprintNoData)
    }
}

/// What came of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A fetch started now.
    Started,
    /// A fetch runs; one more run follows it.
    Queued,
    /// A fetch runs and one more is already waiting behind it: nothing added.
    AlreadyQueued,
    /// Nothing to do: every ship has its slots, or the blueprint lock stands.
    Skipped,
    /// No thread could be started.
    NotStarted,
}

/// Makes the feed of one run.
pub type FeedMaker = Arc<dyn Fn() -> Box<dyn Feed> + Send + Sync>;

/// The ships of the hangar and the wishlist, and the live game build.
type PageKey = (Vec<ShipRow>, String);

/// How the fetches stand.
#[derive(Default)]
struct Run {
    running: bool,
    again: bool,
    /// A blueprint without data has asked in this program run.
    blueprint_asked: bool,
    /// The list of ships and the game build opening the page fetched for last.
    page_fetched: Option<PageKey>,
    /// The data folder of the latest request: the next run reads and writes there.
    paths: Option<Arc<Paths>>,
}

struct Inner {
    feed: FeedMaker,
    run: Mutex<Run>,
    /// Signalled when the fetches end.
    idle: Condvar,
    /// Woken after each run, so a shown page reads the store again.
    waker: Mutex<Option<egui::Context>>,
    /// Runs done.
    runs: AtomicUsize,
    /// Requests per trigger, in the order of [`Trigger`].
    asked: [AtomicUsize; 6],
}

/// The fetches of the slots of one program run. Cloning shares them.
#[derive(Clone)]
pub struct SlotSync {
    inner: Arc<Inner>,
}

impl SlotSync {
    /// Fetches with feeds made by `feed`, one per run.
    pub fn new(feed: FeedMaker) -> SlotSync {
        SlotSync {
            inner: Arc::new(Inner {
                feed,
                run: Mutex::new(Run::default()),
                idle: Condvar::new(),
                waker: Mutex::new(None),
                runs: AtomicUsize::new(0),
                asked: Default::default(),
            }),
        }
    }

    /// The window to wake after a run.
    pub fn wake(&self, ctx: &egui::Context) {
        if let Ok(mut waker) = self.inner.waker.lock()
            && waker.is_none()
        {
            *waker = Some(ctx.clone());
        }
    }

    /// Asks for the slots of the ships in `paths` for the reason `why`.
    pub fn request(&self, paths: &Paths, why: Trigger) -> Answer {
        self.inner.asked[why as usize].fetch_add(1, Ordering::SeqCst);
        if why == Trigger::BlueprintNoData {
            let Ok(mut run) = self.inner.run.lock() else { return Answer::Skipped };
            // Set at the first asking, also when nothing is unknown: later frames of the page stay cheap.
            if std::mem::replace(&mut run.blueprint_asked, true) {
                return Answer::Skipped;
            }
        }
        let mut page_key = None;
        if why.only_when_unknown() {
            let rows = crate::trade::slot_rows(paths);
            if why == Trigger::PageOpen {
                let key = (rows.clone(), versekit_core::stock_fetch::live_build(paths));
                if self.inner.run.lock().is_ok_and(|run| run.page_fetched.as_ref() == Some(&key)) {
                    return Answer::Skipped;
                }
                page_key = Some(key);
            }
            if !erkul_fetch::any_unknown(paths, &rows, crate::trade::now()) {
                return Answer::Skipped;
            }
        }
        let Ok(mut run) = self.inner.run.lock() else { return Answer::Skipped };
        if page_key.is_some() {
            run.page_fetched = page_key;
        }
        run.paths = Some(crate::trade::job_paths(paths));
        if run.running {
            return if std::mem::replace(&mut run.again, true) { Answer::AlreadyQueued } else { Answer::Queued };
        }
        run.running = true;
        drop(run);
        let inner = Arc::clone(&self.inner);
        let started = std::thread::Builder::new().name("erkul-steckplaetze".to_string()).spawn_with_language(move || work(&inner));
        if started.is_err() {
            if let Ok(mut run) = self.inner.run.lock() {
                run.running = false;
                run.again = false;
            }
            self.inner.idle.notify_all();
            return Answer::NotStarted;
        }
        Answer::Started
    }

    /// Whether a fetch runs.
    pub fn running(&self) -> bool {
        self.inner.run.lock().is_ok_and(|run| run.running)
    }

    /// Waits until no fetch runs, at most `limit`. True when none runs.
    pub fn wait_idle(&self, limit: Duration) -> bool {
        let Ok(run) = self.inner.run.lock() else { return false };
        match self.inner.idle.wait_timeout_while(run, limit, |run| run.running) {
            Ok((run, _)) => !run.running,
            Err(_) => false,
        }
    }

    /// How many runs ended.
    pub fn runs(&self) -> usize {
        self.inner.runs.load(Ordering::SeqCst)
    }

    /// How often `why` asked.
    pub fn asked(&self, why: Trigger) -> usize {
        self.inner.asked[why as usize].load(Ordering::SeqCst)
    }
}

/// The runs on their thread: one, and another as long as one was asked for meanwhile.
fn work(inner: &Inner) {
    loop {
        let paths = inner.run.lock().ok().and_then(|run| run.paths.clone());
        if let Some(paths) = paths {
            let feed = (inner.feed)();
            crate::trade::fetch_slots(&paths, feed.as_ref());
        }
        inner.runs.fetch_add(1, Ordering::SeqCst);
        if let Ok(waker) = inner.waker.lock()
            && let Some(ctx) = waker.as_ref()
        {
            ctx.request_repaint();
        }
        let Ok(mut run) = inner.run.lock() else { return };
        if std::mem::replace(&mut run.again, false) {
            continue;
        }
        run.running = false;
        drop(run);
        inner.idle.notify_all();
        return;
    }
}

/// The fetches of the program, with the feed that asks erkul.games.
pub fn global() -> &'static SlotSync {
    static GLOBAL: OnceLock<SlotSync> = OnceLock::new();
    GLOBAL.get_or_init(|| SlotSync::new(Arc::new(|| Box::new(crate::trade::erkul_feed()))))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use versekit_core::erkul_fetch::{FnFeed, pack};
    use versekit_core::ships::{self, ErkulShips, ShipId};

    /// How long a test waits for the fetches.
    pub(crate) const WAIT: Duration = Duration::from_secs(20);

    /// erkul in small: the Arrow and the Gladius, at `version`.
    pub(crate) fn erkul(version: &str, path: &str) -> Option<Vec<u8>> {
        let ship = r#"{"vehicle": {"hardpoints": [{"name": "hp", "accepts": [{"type": "Cooler"}], "maxSize": 1}]}}"#;
        let catalog = format!(r#"{{"dataVersion": "{version}", "groups": [{{"indexPath": "i.bin"}}]}}"#);
        let text = match path {
            "LIVE/catalog.bin" => catalog,
            "LIVE/i.bin" => {
                r#"{"blobs": [{"id": "anvl_arrow", "path": "a.bin"}, {"id": "aegs_gladius", "path": "g.bin"}]}"#
                    .to_string()
            }
            "LIVE/a.bin" | "LIVE/g.bin" => ship.to_string(),
            _ => return None,
        };
        Some(pack(&text))
    }

    /// A gate the feed waits at, and every path it was asked for.
    #[derive(Default)]
    pub(crate) struct Counter {
        pub asked: Mutex<Vec<String>>,
        /// While true, the catalog waits.
        pub closed: Mutex<bool>,
        pub open: Condvar,
        pub version: Mutex<String>,
    }

    impl Counter {
        pub(crate) fn new(version: &str) -> Arc<Counter> {
            let counter = Arc::new(Counter::default());
            *counter.version.lock().unwrap() = version.to_string();
            counter
        }

        /// How often the catalog was asked for: one per run that reached the net.
        pub(crate) fn catalogs(&self) -> usize {
            self.asked.lock().unwrap().iter().filter(|p| p.ends_with("catalog.bin")).count()
        }

        pub(crate) fn close(&self) {
            *self.closed.lock().unwrap() = true;
        }

        pub(crate) fn release(&self) {
            *self.closed.lock().unwrap() = false;
            self.open.notify_all();
        }

        /// A sync whose feeds answer from [`erkul`] and write down what they were asked.
        pub(crate) fn sync(self: &Arc<Counter>) -> SlotSync {
            let counter = Arc::clone(self);
            SlotSync::new(Arc::new(move || {
                let counter = Arc::clone(&counter);
                Box::new(FnFeed(move |path: &str| {
                    counter.asked.lock().unwrap().push(path.to_string());
                    if path.ends_with("catalog.bin") {
                        let closed = counter.closed.lock().unwrap();
                        let _open = counter.open.wait_while(closed, |closed| *closed).unwrap();
                    }
                    let version = counter.version.lock().unwrap().clone();
                    erkul(&version, path)
                }))
            }))
        }
    }

    /// A data folder with `hangar` as `hangar.json`.
    pub(crate) fn folder(name: &str, hangar: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-slotsync-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(versekit_core::hangar::FILE), hangar).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &home)],
            &home,
        );
        (Paths::new(host), dir)
    }

    const ARROW: &str = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil", "hkurz": "ANVL"}]}"#;

    fn knows_arrow(paths: &Paths) -> bool {
        let erkul = ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
        erkul.knows(ShipId { name: "Arrow", maker: "Anvil", short: "", maker_short: "ANVL" })
    }

    #[test]
    fn every_trigger_fetches_and_the_ones_that_wait_for_a_gap_only_with_one() {
        let (paths, dir) = folder("trigger", ARROW);
        // Start, import, by hand and a wish always fetch.
        for why in [Trigger::Start, Trigger::Import, Trigger::ByHand, Trigger::Wish] {
            let counter = Counter::new("4.3");
            let sync = counter.sync();
            assert_eq!(sync.request(&paths, why), Answer::Started, "{why:?}");
            assert!(sync.wait_idle(WAIT));
            assert_eq!((sync.runs(), counter.catalogs()), (1, 1), "{why:?}");
            assert!(knows_arrow(&paths), "{why:?}");
        }
        // Every ship has its slots: opening the page and a blueprint without data ask nothing.
        for why in [Trigger::PageOpen, Trigger::BlueprintNoData] {
            let counter = Counter::new("4.3");
            let sync = counter.sync();
            assert_eq!(sync.request(&paths, why), Answer::Skipped, "{why:?}");
            assert_eq!((sync.runs(), counter.catalogs()), (0, 0), "{why:?}");
        }
        // A ship without slots: both fetch.
        std::fs::remove_file(paths.app_file(ships::ERKUL_FILE)).unwrap();
        for why in [Trigger::PageOpen, Trigger::BlueprintNoData] {
            let counter = Counter::new("4.3");
            let sync = counter.sync();
            assert_eq!(sync.request(&paths, why), Answer::Started, "{why:?}");
            assert!(sync.wait_idle(WAIT));
            assert_eq!((sync.runs(), counter.catalogs()), (1, 1), "{why:?}");
            std::fs::remove_file(paths.app_file(ships::ERKUL_FILE)).unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_run_at_a_time_and_at_most_one_after_it() {
        let (paths, dir) = folder("lock", ARROW);
        let counter = Counter::new("4.3");
        let sync = counter.sync();
        counter.close();
        assert_eq!(sync.request(&paths, Trigger::Import), Answer::Started);
        // The first run waits at the catalog; whatever comes now waits behind it, as one run.
        while counter.asked.lock().unwrap().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(sync.running());
        assert_eq!(sync.request(&paths, Trigger::ByHand), Answer::Queued);
        assert_eq!(sync.request(&paths, Trigger::Wish), Answer::AlreadyQueued);
        assert_eq!(sync.request(&paths, Trigger::Import), Answer::AlreadyQueued);
        counter.release();
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.runs(), 2, "the first run and one after it");
        assert_eq!(counter.catalogs(), 2);
        assert!(!sync.running());
        // Idle again: the next request starts a run of its own.
        assert_eq!(sync.request(&paths, Trigger::Import), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.runs(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_blueprint_asks_once_per_run_and_a_wish_goes_past_that() {
        let (paths, dir) = folder("blueprint", ARROW);
        let counter = Counter::new("4.3");
        let sync = counter.sync();
        assert_eq!(sync.request(&paths, Trigger::BlueprintNoData), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        // The slots are gone again (a concept ship looks like this): the blueprint does not ask a second time.
        std::fs::remove_file(paths.app_file(ships::ERKUL_FILE)).unwrap();
        assert_eq!(sync.request(&paths, Trigger::BlueprintNoData), Answer::Skipped);
        assert_eq!(sync.runs(), 1);
        // A wish goes past the lock, and so does opening the page.
        assert_eq!(sync.request(&paths, Trigger::Wish), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        std::fs::remove_file(paths.app_file(ships::ERKUL_FILE)).unwrap();
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!((sync.runs(), counter.catalogs()), (3, 3));
        assert_eq!(sync.asked(Trigger::BlueprintNoData), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_a_concept_ship_the_page_fetches_once_until_the_ships_or_the_build_change() {
        // The Kraken is a concept ship: erkul never has its slots.
        let hangar = r#"{"format": 1, "schiffe": [{"name": "Arrow", "hersteller": "Anvil", "hkurz": "ANVL"},
            {"name": "Kraken", "hersteller": "Drake", "hkurz": "DRAK"}]}"#;
        let (paths, dir) = folder("concept", hangar);
        let build = |live: &str| {
            std::fs::write(dir.join("spielstand.json"), format!(r#"{{"live":"{live}","ptu":"","format":1,"geholt":1.0}}"#))
                .unwrap();
        };
        build("4.3.0");
        let counter = Counter::new("4.3");
        let sync = counter.sync();
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert!(knows_arrow(&paths));
        // Entering again and again: the Kraken still lacks its slots, nothing is fetched.
        for _ in 0..3 {
            assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Skipped);
        }
        assert_eq!((sync.runs(), counter.catalogs()), (1, 1));
        // A wish changes the list of ships: the next entering fetches once.
        let wished = hangar.replace(r#""format": 1,"#, r#""format": 1, "wunsch": [{"name": "Gladius", "hersteller": "Aegis"}],"#);
        std::fs::write(dir.join(versekit_core::hangar::FILE), wished).unwrap();
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Skipped);
        // A new game build: once more, then quiet again.
        build("4.4.0");
        *counter.version.lock().unwrap() = "4.4".to_string();
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Skipped);
        assert_eq!((sync.runs(), counter.catalogs()), (3, 3));
        // The other ways in still fetch.
        assert_eq!(sync.request(&paths, Trigger::Import), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        // A program run of its own fetches once on entering.
        let fresh = Counter::new("4.4");
        let next = fresh.sync();
        assert_eq!(next.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(next.wait_idle(WAIT));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_game_build_makes_the_page_fetch_again_and_the_new_patch_is_stored() {
        let (paths, dir) = folder("patch", ARROW);
        let build = |live: &str| {
            std::fs::write(
                dir.join("spielstand.json"),
                format!(r#"{{"live":"{live}","ptu":"","format":1,"geholt":1.0}}"#),
            )
            .unwrap();
        };
        build("4.3.0");
        let counter = Counter::new("4.3");
        let sync = counter.sync();
        assert_eq!(sync.request(&paths, Trigger::Start), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Skipped, "the slots are there");
        // The patch: the stored slots count no more, opening the page asks, erkul has the new version.
        build("4.4.0");
        *counter.version.lock().unwrap() = "4.4".to_string();
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        let stored = ErkulShips::load(&paths.app_file(ships::ERKUL_FILE));
        assert_eq!(stored.game_version(), "4.4");
        assert!(knows_arrow(&paths));
        assert_eq!(sync.request(&paths, Trigger::PageOpen), Answer::Skipped, "stored again for the new build");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_the_net_a_run_ends_and_writes_nothing() {
        let (paths, dir) = folder("offline", ARROW);
        let sync = SlotSync::new(Arc::new(|| Box::new(FnFeed(|_: &str| None))));
        assert_eq!(sync.request(&paths, Trigger::Start), Answer::Started);
        assert!(sync.wait_idle(WAIT));
        assert_eq!(sync.runs(), 1);
        assert!(!std::path::Path::new(&paths.app_file(ships::ERKUL_FILE)).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
