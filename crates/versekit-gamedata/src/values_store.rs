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

//! The game values for a window: the last ones read at once, the new ones
//! read in a thread of their own.
//!
//! Reading the values takes seconds (the archive, then scmdb's files for the
//! cross-check). [`start`] answers at once with the values kept in
//! [`CACHE_FILE`] and hands over the [`Rebuild`] that is still due: when the
//! archive changed (path, size, time), or scmdb could not be had the last
//! time and the pause [`RETRY_SECONDS`] is over. [`Pending::poll`] brings
//! the new values back.
//!
//! The source order is the one of [`crate::value_source`]: the archive
//! first, scmdb as fallback, cross-check and filler of what the own
//! reading does not read; the corrections apply to whichever source was used.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use versekit_core::jsonio::{self, Newline};
use versekit_core::writeguard;

use crate::corrections::{self, Correction};
use crate::value_source::{self, Loaded, Source};
use crate::values::GameValues;
use crate::{ARCHIVE_NAME, Error, GameData, Language, scmdb};

/// The cache of the values read from the game data, in the subfolder `Intern`.
pub const CACHE_FILE: &str = "werte-spieldaten.json";

/// The version of the cache layout; a cache of another version is read again.
const CACHE_FORMAT: i64 = 2;

/// Seconds until scmdb is asked again after it could not be had.
pub const RETRY_SECONDS: u64 = 6 * 3600;

/// What the values take from the network: scmdb's files of the same build and
/// the known errors in the data.
pub trait Supply: Send + Sync {
    /// scmdb's values of `build`.
    ///
    /// # Errors
    ///
    /// Why they could not be had.
    fn scmdb_values(&self, build: &str) -> Result<GameValues, String>;

    /// scmdb's current LIVE build, for values without an archive.
    ///
    /// # Errors
    ///
    /// Why it could not be had.
    fn current_build(&self) -> Result<String, String>;

    /// The corrections for `build` (the own ones first), and why scmdb's list
    /// is missing.
    fn corrections(&self, build: Option<&str>) -> (Vec<Correction>, Option<String>);

    /// Whether the network is switched off, so that asking again is pointless.
    fn is_off(&self) -> bool {
        false
    }

    /// Whether a window may read the archive of the installed game with this
    /// supply. A supply made for tests says no: they must not read a game
    /// that happens to be installed.
    fn reads_archive(&self) -> bool {
        true
    }

    /// Whether the game is certainly closed, so that the slots of the ships may be read from the archive: that
    /// reading waits while the game runs, as p4kx asks. A supply made for tests says yes.
    fn game_closed(&self) -> bool {
        true
    }
}

/// The supply at run time: scmdb's mirror, then scmdb.net (`SC_BP_NO_NET`
/// switches it off: then it asks nothing and gives only the own corrections).
#[derive(Clone, Debug)]
pub struct Runtime {
    user_agent: String,
    /// The network is switched off; fixed when the supply is made.
    off: bool,
    /// The computer whose game is asked whether it runs; `None` for the one of this process.
    host: Option<versekit_core::paths::Host>,
}

impl Runtime {
    /// A supply that names VerseKit in its requests, switched off by
    /// `SC_BP_NO_NET` of the running process.
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self { user_agent: user_agent.into(), off: versekit_core::net::offline(), host: None }
    }

    /// The supply for `host`: switched off by `SC_BP_NO_NET` of the environment
    /// given, not of the shell a check runs in. Switched on, the requests
    /// themselves still pass the door of the process (`scmdb`), which refuses
    /// them when the process has the switch.
    pub fn for_host(host: &versekit_core::paths::Host) -> Self {
        Self { off: versekit_core::net::offline_in(host), host: Some(host.clone()), ..Self::default() }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new(versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION")))
    }
}

impl Supply for Runtime {
    fn scmdb_values(&self, build: &str) -> Result<GameValues, String> {
        if self.off {
            return Err("offline".into());
        }
        scmdb::fetch_values(build, &self.user_agent).map_err(|error| error.to_string())
    }

    fn current_build(&self) -> Result<String, String> {
        if self.off {
            return Err("offline".into());
        }
        scmdb::current_build(&self.user_agent).map_err(|error| error.to_string())
    }

    fn corrections(&self, build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        if self.off {
            return (corrections::built_in(), Some("offline".into()));
        }
        let (list, error) = corrections::collect(build, &self.user_agent);
        (list, error.map(|error| error.to_string()))
    }

    fn is_off(&self) -> bool {
        self.off
    }

    /// Asks the process list and the logs of every channel (`game_start::run_state`); what cannot be told
    /// counts as running.
    fn game_closed(&self) -> bool {
        use versekit_core::game_start::run_state;
        let state = match &self.host {
            Some(host) => run_state(&versekit_core::paths::Paths::new(host.clone())),
            None => run_state(versekit_core::paths::global()),
        };
        !state.blocks_writing()
    }
}

/// A supply without network, for tests and for a switched-off network.
#[derive(Clone, Copy, Debug, Default)]
pub struct Offline;

impl Supply for Offline {
    fn scmdb_values(&self, _build: &str) -> Result<GameValues, String> {
        Err("offline".into())
    }

    fn current_build(&self) -> Result<String, String> {
        Err("offline".into())
    }

    fn corrections(&self, _build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        (corrections::built_in(), Some("offline".into()))
    }

    fn is_off(&self) -> bool {
        true
    }

    fn reads_archive(&self) -> bool {
        false
    }
}

/// What is kept with the values: where they come from and when scmdb was
/// asked last.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stamp {
    format: i64,
    /// The path of the archive, empty without one.
    archive: String,
    /// Size and modification time of the archive.
    size: u64,
    modified: u128,
    /// Whether scmdb's values filled the gaps.
    filled: bool,
    /// The seconds since 1970 scmdb was asked for the last time, 0 for never.
    attempt: u64,
    /// `archive` or `scmdb`.
    source: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    stamp: Stamp,
    values: GameValues,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn archive_stamp(channel_folder: Option<&Path>) -> (String, u64, u128) {
    let Some(path) = channel_folder.map(|folder| folder.join(ARCHIVE_NAME)) else { return (String::new(), 0, 0) };
    let Ok(meta) = fs::metadata(&path) else { return (String::new(), 0, 0) };
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    (path.to_string_lossy().into_owned(), meta.len(), modified)
}

fn read_cache(file: &str) -> Option<Stored> {
    let text = fs::read_to_string(file).ok()?;
    let stored: Stored = serde_json::from_str(&text).ok()?;
    (stored.stamp.format == CACHE_FORMAT).then_some(stored)
}

/// The values kept in `cache_file`, read without looking at the archive or asking anybody; `None` without
/// a usable cache.
pub fn kept(cache_file: &str) -> Option<GameValues> {
    read_cache(cache_file).map(|stored| stored.values)
}

fn write_cache(file: &str, stored: &Stored) -> std::io::Result<()> {
    let text = serde_json::to_string(stored).map_err(std::io::Error::other)?;
    jsonio::write_text_atomic(file, &text, Newline::Native)
}

/// The values to show at once, and the rebuild still due.
pub struct Start {
    /// The values kept; `None` when there are none.
    pub values: Option<Arc<GameValues>>,
    /// Where the kept values come from: `archive`, `scmdb` or empty.
    pub source: String,
    /// What is still to be read; `None` when the kept values are current.
    pub rebuild: Option<Rebuild>,
}

/// The reading that is still due.
pub struct Rebuild {
    channel_folder: Option<PathBuf>,
    cache_file: String,
    supply: Arc<dyn Supply>,
}

/// Answers at once with the kept values and says what is still due. `cache_file`
/// is the full path of [`CACHE_FILE`]; `channel_folder` the folder of the
/// game channel, `None` without a game.
pub fn start(channel_folder: Option<&Path>, cache_file: &str, supply: Arc<dyn Supply>) -> Start {
    let stored = read_cache(cache_file);
    let (archive, size, modified) = archive_stamp(channel_folder);
    let due = match &stored {
        None => true,
        Some(stored) => {
            let stamp = &stored.stamp;
            let archive_changed = stamp.archive != archive || stamp.size != size || stamp.modified != modified;
            let scmdb_missing =
                !stamp.filled && !supply.is_off() && now().saturating_sub(stamp.attempt) > RETRY_SECONDS;
            // The slots of the ships came with 5.2.0: a cache from before has none beside it yet.
            // Only an archive gives them, and only while the game is closed.
            let slots_missing = stamp.source == "archive"
                && !archive.is_empty()
                && supply.game_closed()
                && Path::new(cache_file).parent().is_some_and(|folder| !folder.join(crate::ship_slots::SHIPS_FILE).exists());
            archive_changed || scmdb_missing || slots_missing
        }
    };
    let source = stored.as_ref().map(|s| s.stamp.source.clone()).unwrap_or_default();
    let values = stored.map(|stored| Arc::new(stored.values));
    let rebuild = due.then(|| Rebuild {
        channel_folder: channel_folder.map(Path::to_path_buf),
        cache_file: cache_file.to_string(),
        supply,
    });
    Start { values, source, rebuild }
}

/// The values the reading gave.
pub struct Built {
    pub values: Arc<GameValues>,
    pub source: Source,
    /// Why the own reading failed, when scmdb stands in.
    pub archive_error: Option<String>,
    /// How many values differ from scmdb's at the same build, `None` when
    /// they were not compared.
    pub differences: Option<usize>,
}

fn note(label: &str, key: &str, detail: &str) {
    let text = format!("{} ({detail})", versekit_core::language::t(key));
    versekit_core::errors::record(label, &text);
}

impl Rebuild {
    /// Reads the values and keeps them in the cache file; `None` when neither
    /// the archive nor scmdb gives values. A refused or failed write only
    /// costs the next start the reading.
    pub fn run(&self) -> Option<Built> {
        let folder = self.channel_folder.as_deref();
        let topic = versekit_core::archive_lock::reading_for("values");
        let game = folder.map(GameData::open);
        drop(topic);
        let build = game.as_ref().and_then(|g| g.as_ref().ok()).and_then(GameData::scmdb_build);
        let archive: Result<GameValues, Error> = match game {
            Some(game) => game.and_then(|game| {
                let texts = game.localization(Language::English)?;
                let values = game.values(&texts)?;
                self.keep_signatures(&game, &texts, build.as_deref());
                self.keep_slots(&game);
                Ok(values)
            }),
            None => Err(Error::NoGameFolder),
        };
        let read = archive.is_ok();
        let supply = &self.supply;
        let loaded = value_source::choose(archive, || {
            if supply.is_off() {
                return Err("offline".to_string());
            }
            let build = match build.clone() {
                Some(build) => build,
                None if read => return Err("the build of the channel is unknown".to_string()),
                None => supply.current_build()?,
            };
            supply.scmdb_values(&build)
        });
        let Loaded { mut values, source, archive_error, cross_check, scmdb_error } = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                versekit_core::errors::record("values.load", &error);
                return None;
            }
        };
        if let Some(error) = &archive_error {
            let detail = error.to_string();
            note("gamedata.fallback", "s_p3_rueckfall", &detail);
        }
        let differences = cross_check.as_ref().map(crate::cross_check::CrossCheck::differences);
        if let Some(check) = cross_check.as_ref().filter(|check| check.differences() > 0) {
            let detail = format!("{} Abweichungen; {}", check.differences(), check.summary());
            note("gamedata.crosscheck", "s_p3_gegenprobe", &detail);
        }
        let (list, _error) = self.supply.corrections(values.build.as_deref().or(build.as_deref()));
        corrections::apply(&mut values, &list);
        let (archive_path, size, modified) = archive_stamp(folder);
        let filled = source == Source::Scmdb || scmdb_error.is_none() && cross_check.is_some();
        let asked = !self.supply.is_off();
        let stored = Stored {
            stamp: Stamp {
                format: CACHE_FORMAT,
                archive: archive_path,
                size,
                modified,
                filled,
                attempt: if asked { now() } else { 0 },
                source: if source == Source::Archive { "archive" } else { "scmdb" }.into(),
            },
            values,
        };
        if let Err(error) = write_cache(&self.cache_file, &stored)
            && !writeguard::refused(&error)
        {
            versekit_core::errors::record("values.cache_write", &error);
        }
        Some(Built {
            values: Arc::new(stored.values),
            source,
            archive_error: archive_error.map(|e| e.to_string()),
            differences,
        })
    }

    /// Keeps the scan signatures of the rocks of the archive beside the cache, where the mining data find them. A
    /// reading or a write that fails only leaves the signatures of the mining data as they were.
    fn keep_signatures(&self, game: &GameData, texts: &crate::Localization, build: Option<&str>) {
        let Some(folder) = Path::new(&self.cache_file).parent() else { return };
        let file = folder.join(crate::mineables::SIGNATURES_FILE).to_string_lossy().into_owned();
        let rocks = game.dataforge_bytes().and_then(|bytes| crate::mineables::rocks_from_dataforge(&bytes, texts));
        match rocks {
            Ok(rocks) => {
                let stamp = crate::mineables::ArchiveStamp::of(game.archive_path());
                if let Err(error) = crate::mineables::write_signatures(&file, &rocks, build, stamp)
                    && !writeguard::refused(&error)
                {
                    versekit_core::errors::record("values.signatures_write", &error);
                }
            }
            Err(error) => versekit_core::errors::record("values.signatures", &error),
        }
    }

    /// Keeps the slots of the ships and the parts with slots of the archive beside the cache, where the hangar
    /// finds them before erkul's ([`crate::ship_slots`]). A reading or a write that fails leaves the files as they
    /// were: the hangar then reads erkul's.
    fn keep_slots(&self, game: &GameData) {
        let Some(folder) = Path::new(&self.cache_file).parent() else { return };
        // While the game runs the slots wait for the next start with the game closed: the files stay missing
        // or as they were, and the hangar reads what it has.
        if !self.supply.game_closed() {
            return;
        }
        match game.ship_slots() {
            Ok((ships, parts)) => {
                for (name, value) in [(crate::ship_slots::SHIPS_FILE, ships), (crate::ship_slots::PARTS_FILE, parts)] {
                    let file = folder.join(name).to_string_lossy().into_owned();
                    let text = jsonio::dumps(&value, None, false);
                    if let Err(error) = jsonio::write_text_atomic(&file, &text, Newline::Native)
                        && !writeguard::refused(&error)
                    {
                        versekit_core::errors::record("values.slots_write", &error);
                    }
                }
            }
            Err(error) => versekit_core::errors::record("values.slots", &error),
        }
    }

    /// Runs the reading in a thread of its own: see [`Pending::spawn`].
    pub fn spawn(self) -> Option<Pending> {
        let pending = Pending::spawn(move || self.run());
        if pending.is_none() {
            versekit_core::errors::record("values.thread", &"no thread for reading the game values");
        }
        pending
    }
}

/// Called by the reading thread when it is done.
type Wake = Box<dyn Fn() + Send + Sync>;

/// A reading running in a thread of its own.
pub struct Pending {
    receiver: Receiver<Option<Built>>,
    wake: Arc<OnceLock<Wake>>,
    denied: Arc<Mutex<Vec<String>>>,
}

/// How far a reading stands.
pub enum Progress {
    Waiting,
    /// Done: the values, or `None` when none could be had.
    Done(Option<Built>),
}

impl Pending {
    /// Runs `work` in a thread of its own, with the write lock of the calling
    /// thread; `None` when the thread cannot be started. A reading that dies
    /// counts as one without values.
    pub fn spawn(work: impl FnOnce() -> Option<Built> + Send + 'static) -> Option<Pending> {
        let (sender, receiver) = mpsc::channel();
        let wake: Arc<OnceLock<Wake>> = Arc::default();
        let denied: Arc<Mutex<Vec<String>>> = Arc::default();
        let (lock, thread_wake, thread_denied) = (writeguard::state(), Arc::clone(&wake), Arc::clone(&denied));
        std::thread::Builder::new()
            .name("values-read".into())
            .spawn(move || {
                writeguard::adopt(&lock);
                let built = catch_unwind(AssertUnwindSafe(work)).unwrap_or(None);
                thread_denied.lock().unwrap_or_else(|e| e.into_inner()).extend(writeguard::take_denied());
                let _ = sender.send(built);
                if let Some(wake) = thread_wake.get() {
                    wake();
                }
            })
            .ok()?;
        Some(Pending { receiver, wake, denied })
    }

    /// What the thread calls when it is done (the window asks for a new
    /// frame); only the first call counts.
    pub fn set_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = self.wake.set(Box::new(wake));
    }

    /// The writes the write lock refused in the thread, once it is done.
    pub fn take_denied(&self) -> Vec<String> {
        std::mem::take(&mut *self.denied.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The reading when it is done. The result is handed over once.
    pub fn poll(&self) -> Progress {
        match self.receiver.try_recv() {
            Ok(built) => Progress::Done(built),
            Err(TryRecvError::Empty) => Progress::Waiting,
            Err(TryRecvError::Disconnected) => Progress::Done(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vk-values-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn without_a_cache_the_reading_is_due_and_there_are_no_values() {
        let dir = folder("none");
        let file = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        let started = start(None, &file, Arc::new(Offline));
        assert!(started.values.is_none());
        assert!(started.rebuild.is_some());
    }

    #[test]
    fn a_cache_of_the_same_archive_is_current_and_one_of_another_is_due() {
        let dir = folder("cache");
        let file = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        let archive = dir.join(ARCHIVE_NAME);
        fs::write(&archive, b"archive").unwrap();
        let (path, size, modified) = archive_stamp(Some(&dir));
        let stamp = Stamp {
            format: CACHE_FORMAT,
            archive: path,
            size,
            modified,
            filled: true,
            attempt: 1,
            source: "archive".into(),
        };
        write_cache(&file, &Stored { stamp, values: GameValues::default() }).unwrap();
        // A cache from before 5.2.0 has no slots of the ships beside it: they are read once.
        assert!(start(Some(&dir), &file, Arc::new(Offline)).rebuild.is_some(), "the slots of the ships are missing");
        fs::write(dir.join(crate::ship_slots::SHIPS_FILE), b"{}").unwrap();
        let current = start(Some(&dir), &file, Arc::new(Offline));
        assert!(current.values.is_some());
        assert!(current.rebuild.is_none(), "same archive, scmdb filled, slots there");
        fs::write(&archive, b"archive of a new patch").unwrap();
        let changed = start(Some(&dir), &file, Arc::new(Offline));
        assert!(changed.values.is_some(), "the old values are shown while the new ones are read");
        assert!(changed.rebuild.is_some());
    }

    /// A supply without network whose game runs.
    struct Playing;

    impl Supply for Playing {
        fn scmdb_values(&self, _build: &str) -> Result<GameValues, String> {
            Err("offline".into())
        }
        fn current_build(&self) -> Result<String, String> {
            Err("offline".into())
        }
        fn corrections(&self, _build: Option<&str>) -> (Vec<Correction>, Option<String>) {
            (Vec::new(), None)
        }
        fn is_off(&self) -> bool {
            true
        }
        fn game_closed(&self) -> bool {
            false
        }
    }

    #[test]
    fn the_slots_of_the_ships_wait_while_the_game_runs() {
        let dir = folder("playing");
        let file = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        fs::write(dir.join(ARCHIVE_NAME), b"archive").unwrap();
        let (path, size, modified) = archive_stamp(Some(&dir));
        let stamp = Stamp { format: CACHE_FORMAT, archive: path, size, modified, filled: true, attempt: 1, source: "archive".into() };
        write_cache(&file, &Stored { stamp, values: GameValues::default() }).unwrap();
        // The slot file is missing, but the game runs: nothing is read now.
        assert!(start(Some(&dir), &file, Arc::new(Playing)).rebuild.is_none(), "the game runs");
        // Counter-check: with the game closed the same cache is due.
        assert!(start(Some(&dir), &file, Arc::new(Offline)).rebuild.is_some(), "the game is closed");
    }

    #[test]
    fn a_cache_that_scmdb_did_not_fill_is_read_again_after_the_pause_only_with_network() {
        let dir = folder("retry");
        let file = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        let stamp = Stamp {
            format: CACHE_FORMAT,
            archive: String::new(),
            size: 0,
            modified: 0,
            filled: false,
            attempt: now().saturating_sub(RETRY_SECONDS + 10),
            source: "archive".into(),
        };
        write_cache(&file, &Stored { stamp, values: GameValues::default() }).unwrap();
        assert!(start(None, &file, Arc::new(Offline)).rebuild.is_none(), "no network: asking again is pointless");
        let online = Runtime::for_host(&host(""));
        assert!(start(None, &file, Arc::new(online)).rebuild.is_some());
        let off = Runtime::for_host(&host("1"));
        assert!(start(None, &file, Arc::new(off)).rebuild.is_none(), "switched off in the environment given");
    }

    /// A made-up computer with `SC_BP_NO_NET` set to `no_net`.
    fn host(no_net: &str) -> versekit_core::paths::Host {
        versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_NO_NET, no_net)],
            "/",
        )
    }

    /// The switch of the environment given holds for every request, whatever the shell of the
    /// check has; switched off, the own corrections still come.
    #[test]
    fn the_runtime_supply_follows_the_switch_of_the_environment_given() {
        let off = Runtime::for_host(&host("1"));
        assert!(off.is_off());
        assert_eq!(off.current_build(), Err("offline".to_string()));
        assert_eq!(off.scmdb_values("4.3.1-live.1").map(|_| ()), Err("offline".to_string()));
        let (list, why) = off.corrections(Some("4.3.1-live.1"));
        assert_eq!((list.len(), why.as_deref()), (corrections::built_in().len(), Some("offline")));
        assert!(!Runtime::for_host(&host("")).is_off());
        assert!(!Runtime::for_host(&host("0")).is_off());
    }

    #[test]
    fn a_cache_of_another_layout_is_not_used() {
        let dir = folder("format");
        let file = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        fs::write(&file, "{\"stamp\": {}, \"values\": {}}").unwrap();
        assert!(start(None, &file, Arc::new(Offline)).values.is_none());
    }
}
