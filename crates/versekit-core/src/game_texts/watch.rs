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

//! The background of the game's texts as one thing, for the process that keeps them (the overlay): the language
//! line at the start, the clocks of [`Upkeep`], the look whether the game still runs, the look at scmdb (the recipe
//! file with the list of the known data errors, the craft data of the catalog, the mining data after a patch), the
//! catalog after a patch, and the runs on a thread of their own, one at a time.
//!
//! [`Watch::tick`] is cheap (a few file stamps); fetching and writing happen on the thread, which takes over the
//! write lock of its caller (`writeguard`), so a run on a copy of the data writes only into the copy.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use super::{Archive, Notice, Ran, SETTING_STOCK_MARK, Upkeep, check_game_languages, run};
use crate::errors;
use crate::injection::sources;
use crate::jsonio::Value;
use crate::language::SpawnWithLanguage as _;
use crate::net::Net;
use crate::paths::Paths;
use crate::recipe_file::{self, Updated};
use crate::scmdb::FetchError;
use crate::stock_fetch::{self, Outcome};

/// How often the clocks are looked at.
pub const TICK_SECONDS: f64 = 1.0;
/// How often the watch asks whether the game runs.
pub const GAME_LOOK_SECONDS: f64 = 20.0;
/// How often scmdb is asked for a new build: the recipe file and the craft data (`SCMDB_POLL_SEC`).
pub const RECIPES_POLL_SECONDS: f64 = 6.0 * 3600.0;
/// How soon the recipe file is looked at again after a look that failed.
pub const RECIPES_RETRY_SECONDS: f64 = 300.0;
/// How soon a build of the catalog that gave no catalog is tried again.
pub const CATALOG_RETRY_SECONDS: f64 = 300.0;

/// What a build of the catalog gave: the build of the game it is for and the number of its blueprints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogBuilt {
    pub version: String,
    pub blueprints: usize,
}

/// Builds the catalog of the contract data anew from the game data, on the thread of a run (the crate of the game
/// data gives it: `versekit_gamedata::list::rebuild_now`); `None` when no catalog could be built.
pub type CatalogBuild = Arc<dyn Fn(&Paths) -> Option<CatalogBuilt> + Send + Sync>;

/// Whether the craft data of the catalog (kind, size, grade, class) lag behind the build scmdb names as current (the
/// crate of the game data gives it: `versekit_gamedata::list::craft_data_behind`).
pub type CraftCheck = Arc<dyn Fn(&Paths, &str) -> bool + Send + Sync>;

/// What the look at scmdb on the thread did.
struct Looked {
    /// The recipe file, when it was looked at (or the build could not be had).
    recipes: Option<Result<Updated, String>>,
    /// The mining data of the build, when they were due.
    mining: Option<Outcome>,
    /// The catalog was built because its craft data lagged behind.
    craft: bool,
}

/// What a run on the thread did.
struct Done {
    ran: Option<Ran>,
    looked: Option<Looked>,
    /// The build of the catalog, when one ran.
    catalog: Option<Option<CatalogBuilt>>,
    /// The writes the lock refused on the thread.
    denied: Vec<String>,
}

/// The run on the thread: what it carries, and the answer to come.
struct InFlight {
    job: bool,
    catalog: bool,
    mining: bool,
    done: Receiver<Done>,
}

/// The background of the game's texts.
pub struct Watch {
    upkeep: Upkeep,
    net: Arc<dyn Net>,
    archive: Arc<dyn Archive>,
    recipes: Arc<dyn recipe_file::Supply>,
    game_running: fn(&Paths) -> bool,
    started: bool,
    last_tick: f64,
    /// When the game was looked at, and whether it ran.
    game: Option<(f64, bool)>,
    recipes_next: f64,
    /// The patch the recipe file was looked at for.
    patch_seen: f64,
    /// The build of the catalog after a patch, if one is given.
    catalog: Option<CatalogBuild>,
    /// The look whether the craft data lag behind scmdb, if one is given.
    craft: Option<CraftCheck>,
    /// When the craft data are looked at next.
    craft_next: f64,
    /// A build of the catalog is due (after a patch, or again after one that failed).
    catalog_due: bool,
    /// Not before this time.
    catalog_at: f64,
    /// A build for new craft data failed: the next build that works is told.
    craft_pending: bool,
    /// A patch was finished and its mining data are not fetched yet.
    mining_due: bool,
    in_flight: Option<InFlight>,
    /// The writes the lock refused on the threads of the runs, not told yet.
    denied: Vec<String>,
    /// The own blueprints as last read, with the stamp of the collection file they were read at.
    owned: Option<Owned>,
    /// How often the collection was read.
    owned_reads: usize,
    #[cfg(test)]
    fail_spawn: bool,
}

/// The size and the time of change of a file, or `None` without one.
type Stamp = Option<(u64, Option<std::time::SystemTime>)>;

/// The keys of the own blueprints, read from the collection file `path` while it had `stamp`.
struct Owned {
    path: String,
    stamp: Stamp,
    keys: std::collections::BTreeSet<String>,
}

fn stamp_of(path: &str) -> Stamp {
    std::fs::metadata(path).ok().map(|meta| (meta.len(), meta.modified().ok()))
}

/// The own blueprints (`sources::owned`): read again only when the collection file changed (or another one is
/// meant), else those of the last read.
fn owned_of<'a>(
    memo: &'a mut Option<Owned>,
    reads: &mut usize,
    paths: &Paths,
) -> &'a std::collections::BTreeSet<String> {
    let path = paths.app_file(sources::COLLECTION_FILE);
    let stamp = stamp_of(&path);
    static NONE: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let same = memo.as_ref().is_some_and(|known| known.path == path && known.stamp == stamp);
    if !same {
        *reads += 1;
        *memo = Some(Owned { keys: sources::owned(paths), path, stamp });
    }
    memo.as_ref().map_or(&NONE, |known| &known.keys)
}

/// What the look at scmdb is to do.
struct Look {
    recipes: bool,
    mining: bool,
    /// The look whether the craft data lag behind, with the build of the catalog.
    craft: Option<(CraftCheck, CatalogBuild)>,
}

/// The look at scmdb on the thread: its current build, the recipe file of it with the list of the known data errors
/// when `look.recipes`, the mining data when `look.mining`, and the catalog when its craft data lag behind
/// (`look.craft`, unless `built` holds the build of this run already).
fn look_at_scmdb(
    paths: &Paths,
    supply: &dyn recipe_file::Supply,
    look: &Look,
    built: &mut Option<Option<CatalogBuilt>>,
) -> Looked {
    let build = match recipe_file::current_build(supply) {
        Ok(build) => build,
        Err(error) => {
            return Looked { recipes: look.recipes.then_some(Err(error)), mining: None, craft: false };
        }
    };
    let recipes = look.recipes.then(|| recipe_file::update_for(paths, supply, &build));
    let (mining, craft) = (look.mining, look.craft.as_ref());
    let fetch = |name: &str| supply.file(name).map_err(FetchError::Failed);
    let mining = mining.then(|| stock_fetch::update_mining_with(paths, &build, &fetch));
    let mut craft_built = false;
    if built.is_none()
        && let Some((behind, build_catalog)) = craft
        && behind(paths, &build)
    {
        *built = Some(build_catalog(paths));
        craft_built = true;
    }
    Looked { recipes, mining, craft: craft_built }
}

impl Watch {
    /// A watch that fetches over `net` and `recipes` and reads the game data through `archive`. The first tick
    /// checks the language line, looks for a run and at the recipe file at once.
    pub fn new(net: Arc<dyn Net>, archive: Arc<dyn Archive>, recipes: Arc<dyn recipe_file::Supply>) -> Watch {
        Watch {
            upkeep: Upkeep::default(),
            net,
            archive,
            recipes,
            game_running: crate::mission_log::game_running,
            started: false,
            last_tick: f64::NEG_INFINITY,
            game: None,
            recipes_next: 0.0,
            patch_seen: 0.0,
            catalog: None,
            craft: None,
            craft_next: 0.0,
            catalog_due: false,
            catalog_at: 0.0,
            craft_pending: false,
            mining_due: false,
            in_flight: None,
            denied: Vec::new(),
            owned: None,
            owned_reads: 0,
            #[cfg(test)]
            fail_spawn: false,
        }
    }

    /// The same watch, which builds the catalog of the contract data anew once after each finished patch, before
    /// the recipe file and the notes of that run, and again after [`CATALOG_RETRY_SECONDS`] when that gave none.
    pub fn with_catalog(mut self, build: CatalogBuild) -> Watch {
        self.catalog = Some(build);
        self
    }

    /// The same watch, which with each look at scmdb asks `behind` whether the craft data of the catalog lag behind
    /// scmdb's current build, builds the catalog then (with the build of [`Watch::with_catalog`]) and tells it.
    pub fn with_craft_check(mut self, behind: CraftCheck) -> Watch {
        self.craft = Some(behind);
        self
    }

    /// The writes the lock (`writeguard`) refused on the threads of the runs since the last call: on a copy of
    /// the data the caller tells them like those of its own thread.
    pub fn take_denied(&mut self) -> Vec<String> {
        std::mem::take(&mut self.denied)
    }

    /// Whether a run is going on now.
    pub fn busy(&self) -> bool {
        self.in_flight.is_some()
    }

    /// A game session ended: look at once.
    pub fn game_ended(&mut self) {
        self.upkeep.game_ended();
    }

    /// Once a second or more often: take in a finished run, look at the clocks and start the run that is due.
    /// `now` is in seconds from any fixed start. Gives what the player is to be told.
    pub fn tick(&mut self, paths: &Arc<Paths>, now: f64) -> Vec<Notice> {
        let mut notices = Vec::new();
        if !self.take_finished(paths, now, &mut notices) {
            return notices;
        }
        if !self.started {
            self.started = true;
            notices.extend(check_game_languages(paths));
        }
        if now - self.last_tick < TICK_SECONDS {
            return notices;
        }
        self.last_tick = now;
        self.look_at_the_game(paths, now);
        let owned = owned_of(&mut self.owned, &mut self.owned_reads, paths);
        let job = self.upkeep.tick(paths, self.archive.as_ref(), owned, now);
        // A finished patch brings a new build: the recipe file and the mining data are looked at with the same
        // run, and the catalog is built anew once for it.
        if self.upkeep.patch_at > 0.0 && self.upkeep.patch_at != self.patch_seen {
            self.patch_seen = self.upkeep.patch_at;
            self.recipes_next = 0.0;
            self.mining_due = true;
            if self.catalog.is_some() {
                self.catalog_due = true;
                self.catalog_at = 0.0;
            }
        }
        let recipes = now >= self.recipes_next;
        if recipes && crate::net::offline_in(&paths.host) {
            self.recipes_next = now + RECIPES_RETRY_SECONDS;
        }
        let offline = crate::net::offline_in(&paths.host);
        let recipes = recipes && !offline;
        let craft = match (&self.craft, &self.catalog) {
            (Some(behind), Some(build)) if now >= self.craft_next && !offline => {
                // At most every six hours, whatever the look brings (`scmdb_next` of 4.12).
                self.craft_next = now + RECIPES_POLL_SECONDS;
                Some((Arc::clone(behind), Arc::clone(build)))
            }
            _ => None,
        };
        let mining = (recipes || craft.is_some()) && self.mining_due;
        let look = Look { recipes, mining, craft };
        let catalog = self.catalog_due && now >= self.catalog_at;
        if job.is_none() && !look.recipes && look.craft.is_none() && !catalog {
            return notices;
        }
        self.start(paths, job, look, catalog, now);
        notices
    }

    /// Takes in the run that ended; whether no run is going on any longer.
    fn take_finished(&mut self, paths: &Paths, now: f64, notices: &mut Vec<Notice>) -> bool {
        let Some(flight) = &self.in_flight else { return true };
        let done = match flight.done.try_recv() {
            Ok(done) => Some(done),
            Err(TryRecvError::Empty) => return false,
            // A thread that broke off must not leave the watch waiting for ever.
            Err(TryRecvError::Disconnected) => None,
        };
        let (job, had_catalog, had_mining) = (flight.job, flight.catalog, flight.mining);
        self.in_flight = None;
        let (ran, looked, catalog) = match done {
            Some(done) => {
                self.denied.extend(done.denied);
                (done.ran, done.looked, done.catalog)
            }
            None => {
                let broken = Looked { recipes: Some(Err("broken off".to_string())), mining: None, craft: false };
                (None, Some(broken), had_catalog.then_some(None))
            }
        };
        if job {
            self.upkeep.finished(paths, ran.as_ref().is_some_and(|ran| ran.renewed));
        }
        if let Some(ran) = ran {
            notices.extend(ran.notices);
        }
        let craft = looked.as_ref().is_some_and(|looked| looked.craft);
        match catalog {
            Some(Some(built)) => {
                if craft || self.craft_pending {
                    notices.push(Notice::CraftData { build: built.version, items: built.blueprints });
                }
                self.craft_pending = false;
            }
            Some(None) => {
                // No catalog came: tried again in a while, and new craft data are told when it works.
                errors::record("watcher.katalog", &"the catalog build gave no catalog");
                self.catalog_due = true;
                self.catalog_at = now + CATALOG_RETRY_SECONDS;
                self.craft_pending |= craft;
            }
            None => {}
        }
        let Some(looked) = looked else { return true };
        // Mining data that could not be had are asked for again with the next look.
        if (had_mining && looked.mining.is_none()) || matches!(looked.mining, Some(Outcome::Failed(_))) {
            self.mining_due = true;
        }
        match looked.recipes {
            Some(Ok(updated)) => {
                self.recipes_next = now + RECIPES_POLL_SECONDS;
                if matches!(updated, Updated::Written(_) | Updated::Corrected(_)) {
                    // New recipes or corrections change the details in the notes: the next look writes them anew.
                    paths.set_setting(SETTING_STOCK_MARK, Value::str(""));
                    self.upkeep.stock_next = 0.0;
                }
            }
            Some(Err(error)) => {
                errors::record("watcher.rezepte", &error);
                self.recipes_next = now + RECIPES_RETRY_SECONDS;
            }
            None => {}
        }
        true
    }

    /// Every [`GAME_LOOK_SECONDS`]: a game that ran and runs no longer ended its session.
    fn look_at_the_game(&mut self, paths: &Paths, now: f64) {
        if self.game.is_some_and(|(at, _)| now - at < GAME_LOOK_SECONDS) {
            return;
        }
        let running = (self.game_running)(paths);
        if self.game.is_some_and(|(_, was)| was) && !running {
            self.upkeep.game_ended();
        }
        self.game = Some((now, running));
    }

    fn start(&mut self, paths: &Arc<Paths>, job: Option<super::Job>, look: Look, catalog_now: bool, now: f64) {
        let has_job = job.is_some();
        let version = if has_job { crate::stock_fetch::live_build(paths) } else { String::new() };
        let (sender, receiver) = mpsc::channel();
        let (paths_now, net, archive, supply) =
            (Arc::clone(paths), Arc::clone(&self.net), Arc::clone(&self.archive), Arc::clone(&self.recipes));
        let catalog = if catalog_now { self.catalog.clone() } else { None };
        let has_catalog = catalog.is_some();
        if has_catalog {
            self.catalog_due = false;
        }
        let (recipes, mining, craft) = (look.recipes, look.mining, look.craft.is_some());
        self.mining_due &= !mining;
        let scmdb = (look.recipes || look.craft.is_some()).then_some(look);
        let lock = crate::writeguard::state();
        let work = move || {
            crate::writeguard::adopt(&lock);
            // The catalog and the recipes first: the notes of the same run are written with them.
            let mut built = catalog.map(|build| build(&paths_now));
            let looked = scmdb.map(|look| look_at_scmdb(&paths_now, supply.as_ref(), &look, &mut built));
            let ran = job.map(|job| run(&job, &paths_now, net.as_ref(), archive.as_ref(), &version));
            let _ = sender.send(Done { ran, looked, catalog: built, denied: crate::writeguard::take_denied() });
        };
        #[cfg(test)]
        let started =
            !self.fail_spawn && std::thread::Builder::new().name("spieltexte".into()).spawn_with_language(work).is_ok();
        #[cfg(not(test))]
        let started = std::thread::Builder::new().name("spieltexte".into()).spawn_with_language(work).is_ok();
        if started {
            self.in_flight = Some(InFlight { job: has_job, catalog: has_catalog, mining, done: receiver });
            return;
        }
        // Without a thread no run comes: the clocks must not wait for it.
        errors::record("watcher.spieltexte", &"thread did not start");
        if has_job {
            self.upkeep.running = false;
        }
        if recipes {
            self.recipes_next = now + RECIPES_RETRY_SECONDS;
        }
        if craft {
            self.craft_next = now + RECIPES_RETRY_SECONDS;
        }
        // The catalog and the mining data stay due for the next run.
        self.catalog_due |= has_catalog;
        self.mining_due |= mining;
    }
}

#[cfg(test)]
mod tests;
