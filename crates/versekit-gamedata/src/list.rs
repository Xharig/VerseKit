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

//! The catalog the blueprint list shows, built from the game data.
//!
//! The list reads a [`CatalogCache`]. [`list_catalog`] builds one from the
//! installed `Data.p4k` and keeps it in [`CACHE_FILE`], so the archive is
//! read once per patch and not at every start. The stamp of that file names
//! the archive (path, size, time) and whether scmdb's catalog could be had;
//! when the archive changes, or the file is broken, it is built again.
//!
//! Where the catalog comes from (the same order of precedence as the game
//! values, see `value_source`):
//!
//! | Source | What for |
//! |---|---|
//! | the own corrections (`bp-overrides.json`, `versekit_core::overrides`) | class, size and grade; set on every catalog handed out, above all else; a variant without a size takes its base's |
//! | the user's `Data.p4k` | blueprints, items, contracts, starter blueprints, reputation limits, shareable, cooldown; always first |
//! | the known errors in the data (`corrections`) | applied while reading |
//! | scmdb's catalog of the same build, fetched at run time ([`Supply`]) | rewards and places the data do not state, and the standing cross-check; the whole catalog when the archive cannot be read |
//! | the 4.12 cache | shown while the first build runs, nothing else |
//!
//! A window does not wait for the build: [`start_catalog`] answers at once
//! with the best catalog at hand (the current cache; else the last one built
//! from the same archive; else the 4.12 cache) and hands over the [`Rebuild`]
//! that is still due. [`Rebuild::spawn`] runs it in a thread of its own, and
//! [`Pending::poll`] brings the result back.
//!
//! Names are read in English: they are the keys of the collection, the
//! watch list and the farm notes, which 4.12 wrote with the English names.
//!
//! The patch a blueprint came with (`since`) is not in the game data: it
//! comes from this device's own observation, see
//! `versekit_core::patch_history`.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use versekit_core::catalog_cache::{self, Blueprint as ListBlueprint, CatalogCache, Mission, Place, Way};
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::{Host, Paths, name_key};
use versekit_core::text::py_strip;
use versekit_core::{patch_history, writeguard};

use crate::corrections::{self, Correction};
use crate::{ARCHIVE_NAME, Catalog, GameData, Item, Language, Source, SourceKind, scmdb, scmdb_catalog};

/// The cache of the catalog built from the game data, in the subfolder `Intern`.
pub const CACHE_FILE: &str = "katalog-spieldaten.json";

/// The version of the cache layout; a cache of another version is built again.
/// 3: the contracts carry the values the notes in the game text need.
/// 4: the blueprints carry the tag, subtype and kind of their recipe.
/// 5: the file carries the text keys of every crafted item (`beschreibungen`).
pub const CACHE_FORMAT: i64 = 5;

/// Seconds until scmdb is asked again after it could not be had, as 4.12 does.
pub const RETRY_SECONDS: u64 = 6 * 3600;

/// So many ways per blueprint are kept, the easiest first (4.12 `SOURCES_PER_BP`).
const WAYS_PER_BLUEPRINT: usize = 12;

/// So many places are named per contract (4.12 `PLACES_PER_CONTRACT`).
const PLACES_PER_CONTRACT: usize = 4;

/// What the catalog takes from the network besides the archive: the known
/// errors in the data and scmdb's catalog. Nothing of it is shipped.
pub trait Supply: Send + Sync {
    /// scmdb's current LIVE build, for a catalog without an archive.
    ///
    /// # Errors
    ///
    /// Why it could not be had.
    fn current_build(&self) -> Result<String, String>;

    /// The corrections for `build`, and why scmdb's list is missing.
    fn corrections(&self, build: Option<&str>) -> (Vec<Correction>, Option<String>);

    /// scmdb's catalog of `build`, with the `corrections` applied to it.
    ///
    /// # Errors
    ///
    /// Why it could not be had.
    fn catalog(&self, build: &str, corrections: &[Correction]) -> Result<CatalogCache, String>;

    /// Whether the network is switched off, so that asking again is pointless.
    fn is_off(&self) -> bool {
        false
    }
}

/// The supply at run time: scmdb's mirror, then scmdb.net (`SC_BP_NO_NET`
/// switches it off).
#[derive(Clone, Debug)]
pub struct Runtime {
    user_agent: String,
}

impl Runtime {
    /// A supply that names VerseKit in its requests.
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self { user_agent: user_agent.into() }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new(versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION")))
    }
}

impl Supply for Runtime {
    fn current_build(&self) -> Result<String, String> {
        scmdb::current_build(&self.user_agent).map_err(|error| error.to_string())
    }

    fn corrections(&self, build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        let (list, error) = corrections::collect(build, &self.user_agent);
        (list, error.map(|error| error.to_string()))
    }

    fn catalog(&self, build: &str, corrections: &[Correction]) -> Result<CatalogCache, String> {
        scmdb_catalog::fetch(build, corrections, &self.user_agent).map_err(|error| error.to_string())
    }

    fn is_off(&self) -> bool {
        std::env::var(versekit_core::legacy::ENV_NO_NET).is_ok_and(|value| !value.is_empty() && value != "0")
    }
}

/// No supply: only VerseKit's own corrections and the archive.
#[derive(Clone, Copy, Debug, Default)]
pub struct Offline;

impl Supply for Offline {
    fn current_build(&self) -> Result<String, String> {
        Err("offline".into())
    }

    fn corrections(&self, _build: Option<&str>) -> (Vec<Correction>, Option<String>) {
        (corrections::built_in(), Some("offline".into()))
    }

    fn catalog(&self, _build: &str, _corrections: &[Correction]) -> Result<CatalogCache, String> {
        Err("offline".into())
    }

    fn is_off(&self) -> bool {
        true
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The catalog for the blueprint list: from the installed game, else from
/// scmdb; `None` when there is neither.
pub fn list_catalog(paths: &Paths) -> Option<CatalogCache> {
    let archive = paths.archive_file().map(PathBuf::from);
    list_catalog_file(paths, archive.as_deref(), &paths.app_file(CACHE_FILE))
}

/// Like [`list_catalog`], never asking the network: the cache, else the
/// archive alone. For a caller that must not wait for scmdb (the overlay at
/// its start); the window completes the catalog from scmdb later.
pub fn list_catalog_offline(paths: &Paths) -> Option<CatalogCache> {
    let archive = paths.archive_file().map(PathBuf::from);
    list_catalog_archive(paths, archive.as_deref(), &paths.app_file(CACHE_FILE), Arc::new(Offline))
}

/// Like [`list_catalog`], with the channel folder (`None`: no installation)
/// and the cache file given. Waits for the build when one is due; a window
/// takes [`start_catalog_in`] instead.
pub fn list_catalog_in(paths: &Paths, channel_folder: Option<&Path>, cache_file: &str) -> Option<CatalogCache> {
    list_catalog_with(paths, channel_folder, cache_file, Arc::new(Runtime::default()))
}

/// Like [`list_catalog_in`], with the archive file itself given (`None`: no
/// archive).
pub fn list_catalog_file(paths: &Paths, archive: Option<&Path>, cache_file: &str) -> Option<CatalogCache> {
    list_catalog_archive(paths, archive, cache_file, Arc::new(Runtime::default()))
}

/// The archive file in a channel folder.
fn archive_in(channel_folder: Option<&Path>) -> Option<PathBuf> {
    channel_folder.map(|folder| folder.join(ARCHIVE_NAME))
}

/// Like [`list_catalog_in`], with the `supply` given.
pub fn list_catalog_with(
    paths: &Paths,
    channel_folder: Option<&Path>,
    cache_file: &str,
    supply: Arc<dyn Supply>,
) -> Option<CatalogCache> {
    list_catalog_archive(paths, archive_in(channel_folder).as_deref(), cache_file, supply)
}

/// Like [`list_catalog_with`], with the archive file itself given.
pub fn list_catalog_archive(
    paths: &Paths,
    archive: Option<&Path>,
    cache_file: &str,
    supply: Arc<dyn Supply>,
) -> Option<CatalogCache> {
    let start = start_in(paths, archive, cache_file, false, Arc::clone(&supply), now());
    let Some(rebuild) = start.rebuild else { return start.catalog };
    // A build that fails (the archive cannot be read for a while): the last
    // build of the same archive stands in, only then the 4.12 cache.
    rebuild.run().or_else(|| start_in(paths, archive, cache_file, true, supply, now()).catalog)
}

/// What the list shows first, and what is still to do.
pub struct Start {
    /// The catalog to show at once: the cache of this archive; when that is
    /// out of date or broken, the last cache built from the same archive;
    /// else the 4.12 cache. `None` when there is none of them.
    pub catalog: Option<CatalogCache>,
    /// The build that brings the catalog up to date; `None` when `catalog` is
    /// up to date already.
    pub rebuild: Option<Rebuild>,
}

/// The catalog for the blueprint list without waiting for the build: see
/// [`start_catalog_in`].
pub fn start_catalog(paths: &Paths) -> Start {
    let archive = paths.archive_file().map(PathBuf::from);
    start_catalog_file(paths, archive.as_deref(), &paths.app_file(CACHE_FILE))
}

/// Like [`start_catalog`], with the channel folder (`None`: no installation)
/// and the cache file given. Reads the cache file and, at most, the 4.12
/// cache; never the archive and never the network.
pub fn start_catalog_in(paths: &Paths, channel_folder: Option<&Path>, cache_file: &str) -> Start {
    start_catalog_with(paths, channel_folder, cache_file, Arc::new(Runtime::default()))
}

/// Like [`start_catalog_in`], with the `supply` the build will use.
pub fn start_catalog_with(
    paths: &Paths,
    channel_folder: Option<&Path>,
    cache_file: &str,
    supply: Arc<dyn Supply>,
) -> Start {
    start_catalog_archive(paths, archive_in(channel_folder).as_deref(), cache_file, supply)
}

/// Like [`start_catalog_in`], with the archive file itself given (`None`: no
/// archive).
pub fn start_catalog_file(paths: &Paths, archive: Option<&Path>, cache_file: &str) -> Start {
    start_catalog_archive(paths, archive, cache_file, Arc::new(Runtime::default()))
}

/// Like [`start_catalog_with`], with the archive file itself given.
pub fn start_catalog_archive(
    paths: &Paths,
    archive: Option<&Path>,
    cache_file: &str,
    supply: Arc<dyn Supply>,
) -> Start {
    start_in(paths, archive, cache_file, true, supply, now())
}

/// A build **now**, whatever the cache says: the button "Jetzt neu holen" of the page Erkennung. The
/// archive is read again and scmdb is asked again; the cache file is written when it worked. Run it in a thread
/// of its own ([`Rebuild::spawn`]).
pub fn rebuild_now(paths: &Paths) -> Rebuild {
    let archive = paths.archive_file().map(PathBuf::from);
    rebuild_now_archive(paths, archive.as_deref(), &paths.app_file(CACHE_FILE), Arc::new(Runtime::default()))
}

/// [`rebuild_now`] with the archive, the cache file and the `supply` given.
pub fn rebuild_now_archive(
    paths: &Paths,
    archive: Option<&Path>,
    cache_file: &str,
    supply: Arc<dyn Supply>,
) -> Rebuild {
    let archive = archive.map(Path::to_path_buf).filter(|path| path.is_file());
    Rebuild {
        host: paths.host.clone(),
        stamp: stamp(archive.as_deref()),
        archive,
        cache_file: cache_file.to_string(),
        legacy: OnceLock::new(),
        supply,
    }
}

/// The seconds since 1970 the stored cache says scmdb was asked at, and
/// what it was asked for: the build it brought (empty: it could not be had,
/// `-`: it was not asked for).
fn scmdb_state(source: Option<&Value>) -> (String, u64) {
    let build = source.and_then(|s| s.get("scmdb")).and_then(Value::as_str).unwrap_or_default().to_string();
    let tried =
        source.and_then(|s| s.get("versuch")).and_then(Value::as_str).and_then(|text| text.parse().ok()).unwrap_or(0);
    (build, tried)
}

/// Whether the stored cache is up to date for `stamp`: the same archive in
/// the same layout and scmdb either brought or not due to be asked again.
/// Without an archive the catalog of scmdb is asked for again after
/// [`RETRY_SECONDS`], as a patch cannot be seen.
fn is_current(source: Option<&Value>, stamp: &Value, has_archive: bool, supply: &dyn Supply, now: u64) -> bool {
    let Some(source) = source else { return false };
    if !same_archive(source, stamp) {
        return false;
    }
    let (build, tried) = scmdb_state(Some(source));
    let waited = now.saturating_sub(tried);
    if !has_archive {
        return supply.is_off() || waited < RETRY_SECONDS;
    }
    // With an archive scmdb adds to it: asked again only when it is missing.
    !build.is_empty() || supply.is_off() || waited < RETRY_SECONDS
}

/// Whether the stored cache was built in the layout of `stamp` from the archive it names, as it is now.
fn same_archive(source: &Value, stamp: &Value) -> bool {
    let same = |key: &str| source.get(key) == stamp.get(key);
    same("format") && same("archiv") && same("archiv_stand")
}

/// Whether the craft data of the catalog (kind, size, grade, class) lag behind `scmdb_build`, the build scmdb names
/// as current (`_scmdb_tick` of 4.12): there is no catalog, or with an archive the cache is not up to date for it
/// (a patch, or scmdb's part missing for [`RETRY_SECONDS`]), or without an archive, where scmdb's catalog stands
/// in, the cache holds another build. Reads only the cache file, never the archive or the network.
pub fn craft_data_behind(paths: &Paths, scmdb_build: &str) -> bool {
    let archive = paths.archive_file().map(PathBuf::from);
    craft_data_behind_file(archive.as_deref(), &paths.app_file(CACHE_FILE), scmdb_build, &Runtime::default(), now())
}

/// [`craft_data_behind`] with the archive, the cache file, the supply and the time given.
fn craft_data_behind_file(
    archive: Option<&Path>,
    cache_file: &str,
    scmdb_build: &str,
    supply: &dyn Supply,
    now: u64,
) -> bool {
    let archive = archive.filter(|path| path.is_file());
    let stamp = stamp(archive);
    let stored = jsonio::read_json(cache_file).ok();
    let source = stored.as_ref().and_then(|data| data.get("quelle"));
    let Some(catalog) = stored.as_ref().and_then(non_empty) else { return true };
    if archive.is_some() {
        return !is_current(source, &stamp, true, supply, now);
    }
    !source.is_some_and(|source| same_archive(source, &stamp))
        || (!scmdb_build.is_empty() && catalog.version != scmdb_build)
}

/// [`start_catalog_in`]; without `stand_in` a build that is due comes with no
/// catalog to show meanwhile (for a caller that waits for the build anyway).
fn start_in(
    paths: &Paths,
    archive: Option<&Path>,
    cache_file: &str,
    stand_in: bool,
    supply: Arc<dyn Supply>,
    now: u64,
) -> Start {
    let archive = archive.map(Path::to_path_buf).filter(|path| path.is_file());
    let stamp = stamp(archive.as_deref());
    let stored = jsonio::read_json(cache_file).ok();
    let source = stored.as_ref().and_then(|data| data.get("quelle"));
    if is_current(source, &stamp, archive.is_some(), &*supply, now)
        && let Some(catalog) = stored.as_ref().and_then(non_empty)
    {
        return Start { catalog: Some(corrected(paths, catalog)), rebuild: None };
    }
    let rebuild = |legacy: OnceLock<Option<CatalogCache>>| Rebuild {
        host: paths.host.clone(),
        archive: archive.clone(),
        stamp: stamp.clone(),
        cache_file: cache_file.to_string(),
        legacy,
        supply: Arc::clone(&supply),
    };
    if !stand_in {
        return Start { catalog: None, rebuild: Some(rebuild(OnceLock::new())) };
    }
    // The last build in this layout stands in before the 4.12 cache, if it
    // was made from the same archive (or from none, as there is none).
    let same_archive =
        |old: &Value| old.get("format") == stamp.get("format") && old.get("archiv") == stamp.get("archiv");
    if let Some(data) = stored.as_ref().filter(|_| source.is_some_and(same_archive))
        && let Some(catalog) = non_empty(data)
    {
        return Start { catalog: Some(corrected(paths, catalog)), rebuild: Some(rebuild(OnceLock::new())) };
    }
    let legacy = catalog_cache::load(paths);
    Start {
        catalog: legacy.clone().map(|catalog| corrected(paths, catalog)),
        rebuild: Some(rebuild(OnceLock::from(legacy))),
    }
}

/// The catalog with the own corrections of `bp-overrides.json` set on it, which come before the game data and
/// scmdb. They are set on what is handed out, never kept in the cache file, so a changed file counts at the next
/// load.
fn corrected(paths: &Paths, mut catalog: CatalogCache) -> CatalogCache {
    versekit_core::overrides::Overrides::load(paths).apply(&mut catalog);
    catalog
}

/// The catalog a cache file holds; `None` when it holds no blueprint.
fn non_empty(data: &Value) -> Option<CatalogCache> {
    let catalog = catalog_cache::parse(data);
    (!catalog.blueprints.is_empty()).then_some(catalog)
}

/// A build of the catalog that is due. It reads the archive, completes the
/// catalog from scmdb where that can be had and keeps it in the cache file.
pub struct Rebuild {
    host: Host,
    archive: Option<PathBuf>,
    stamp: Value,
    cache_file: String,
    legacy: OnceLock<Option<CatalogCache>>,
    supply: Arc<dyn Supply>,
}

impl Rebuild {
    /// Builds the catalog and keeps it in the cache file; `None` when neither
    /// the archive nor scmdb gives blueprints. A refused or failed write only
    /// costs the next start the building.
    pub fn run(&self) -> Option<CatalogCache> {
        let paths = Paths::new(self.host.clone());
        let Some(built) = build(&paths, self.archive.as_deref(), &*self.supply, &self.cache_file) else {
            versekit_core::errors::record("list.rebuild", &"the game data and scmdb gave no blueprint catalog");
            return None;
        };
        // `scmdb` is the build whose catalog completed this one, `-` when scmdb
        // was not asked for (no build to ask for) and empty when it could not
        // be had; `versuch` is the time scmdb was asked at, 0 when it was
        // not asked (a supply without network does not count).
        let mut stamp = self.stamp.clone();
        if let Value::Object(map) = &mut stamp {
            let asked = built.asked && !self.supply.is_off();
            let marker = built.scmdb.as_deref().unwrap_or(if built.asked { "" } else { "-" });
            map.push(("scmdb".into(), Value::str(marker)));
            map.push(("versuch".into(), Value::str(if asked { now() } else { 0 }.to_string())));
        }
        if let Err(error) = write_cache(&self.cache_file, &stamp, &built.catalog, &built.item_texts)
            && !writeguard::refused(&error)
        {
            versekit_core::errors::record("list.cache_write", &error);
        }
        Some(corrected(&paths, built.catalog))
    }

    /// The 4.12 cache, read when first asked for.
    pub fn legacy(&self) -> &Option<CatalogCache> {
        self.legacy.get_or_init(|| catalog_cache::load(&Paths::new(self.host.clone())))
    }

    /// Runs the build in a thread of its own: see [`Pending::spawn`].
    pub fn spawn(self) -> Option<Pending> {
        let pending = Pending::spawn(move || self.run());
        if pending.is_none() {
            versekit_core::errors::record("list.thread", &"no thread for the catalog build");
        }
        pending
    }
}

/// Called by the build thread when it is done.
type Wake = Box<dyn Fn() + Send + Sync>;

/// A build running in a thread of its own.
pub struct Pending {
    receiver: Receiver<Option<CatalogCache>>,
    wake: Arc<OnceLock<Wake>>,
    /// The writes the write lock refused in the thread.
    denied: Arc<Mutex<Vec<String>>>,
}

/// How far a build stands.
#[derive(Debug, PartialEq)]
pub enum Progress {
    /// Still running.
    Waiting,
    /// Done: the catalog, or `None` when it could not be built.
    Done(Option<CatalogCache>),
}

impl Pending {
    /// Runs `build` in a thread of its own, with the write lock of the
    /// calling thread; `None` when the thread cannot be started. A build that
    /// dies counts as one without a catalog.
    pub fn spawn(build: impl FnOnce() -> Option<CatalogCache> + Send + 'static) -> Option<Pending> {
        let (sender, receiver) = mpsc::channel();
        let wake: Arc<OnceLock<Wake>> = Arc::default();
        let denied: Arc<Mutex<Vec<String>>> = Arc::default();
        let (lock, thread_wake, thread_denied) = (writeguard::state(), Arc::clone(&wake), Arc::clone(&denied));
        std::thread::Builder::new()
            .name("catalog-build".into())
            .spawn_with_language(move || {
                writeguard::adopt(&lock);
                let built = catch_unwind(AssertUnwindSafe(build)).unwrap_or(None);
                thread_denied.lock().unwrap_or_else(|e| e.into_inner()).extend(writeguard::take_denied());
                let _ = sender.send(built);
                if let Some(wake) = thread_wake.get() {
                    wake();
                }
            })
            .ok()?;
        Some(Pending { receiver, wake, denied })
    }

    /// The writes the write lock refused in the build thread, once it is done.
    pub fn take_denied(&self) -> Vec<String> {
        std::mem::take(&mut *self.denied.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Whether [`Pending::set_wake`] has been called.
    pub fn has_wake(&self) -> bool {
        self.wake.get().is_some()
    }

    /// What the thread calls when it is done (the window asks for a new
    /// frame); only the first call counts. Ask [`Pending::poll`] after it:
    /// a build that finished earlier has not called it.
    pub fn set_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = self.wake.set(Box::new(wake));
    }

    /// The build when it is done; a build that died counts as done without a
    /// catalog. The result is handed over once: ask until `Done`, not after.
    pub fn poll(&self) -> Progress {
        match self.receiver.try_recv() {
            Ok(built) => Progress::Done(built),
            Err(TryRecvError::Empty) => Progress::Waiting,
            Err(TryRecvError::Disconnected) => Progress::Done(None),
        }
    }
}

/// Size and modification time of a file, `Null` when it is missing.
fn file_stamp(path: &Path) -> Value {
    let Ok(meta) = fs::metadata(path) else { return Value::Null };
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    Value::Object(vec![
        ("groesse".into(), Value::Int(meta.len().to_string())),
        ("geaendert".into(), Value::Int(modified.to_string())),
    ])
}

fn stamp(archive: Option<&Path>) -> Value {
    Value::Object(vec![
        ("format".into(), Value::int(CACHE_FORMAT)),
        ("archiv".into(), archive.map_or(Value::Null, |a| Value::str(a.to_string_lossy()))),
        ("archiv_stand".into(), archive.map_or(Value::Null, file_stamp)),
    ])
}

fn write_cache(file: &str, stamp: &Value, catalog: &CatalogCache, item_texts: &Map) -> std::io::Result<()> {
    let mut data = catalog_cache::to_value(catalog);
    if let Value::Object(map) = &mut data {
        map.insert(0, ("quelle".into(), stamp.clone()));
        if !item_texts.is_empty() {
            map.push((catalog_cache::ITEM_TEXTS.into(), Value::Object(item_texts.clone())));
        }
    }
    jsonio::write_text_atomic(file, &jsonio::dumps(&data, None, false), jsonio::Newline::Native)
}

/// The text keys of every item a blueprint of `game` makes, as the catalog file keeps them
/// ([`catalog_cache::ITEM_TEXTS`]): record GUID in lower case → `{"name": …, "text": …}`, in the order of the
/// blueprints; an item without a description key is left out.
pub fn item_texts(game: &Catalog) -> Map {
    let mut found = Map::new();
    let mut seen = HashSet::new();
    for item in game.blueprints.iter().filter_map(|b| b.item.as_ref()) {
        let Some(text) = &item.description_key else { continue };
        let guid = item.guid.to_ascii_lowercase();
        if !seen.insert(guid.clone()) {
            continue;
        }
        let mut keys = Map::new();
        if let Some(name) = crate::catalog::text_key(item.name.key.as_deref()) {
            keys.push(("name".into(), Value::str(name)));
        }
        keys.push(("text".into(), Value::str(text)));
        found.push((guid, Value::Object(keys)));
    }
    found
}

/// A built catalog with what it was completed from.
struct Built {
    catalog: CatalogCache,
    /// The text keys of the crafted items ([`item_texts`]); empty without the game data.
    item_texts: Map,
    /// The scmdb build whose catalog completed this one; `None` when scmdb
    /// could not be had.
    scmdb: Option<String>,
    /// Whether scmdb was asked for at all: there was a build to ask for.
    asked: bool,
}

/// How a note is worded in the error log: the label and the text key.
fn note(label: &str, key: &str, detail: &str) {
    let text = format!("{} ({detail})", versekit_core::language::t(key));
    versekit_core::errors::record(label, &text);
}

/// Reads the archive and builds the catalog, completed and checked with
/// scmdb's; scmdb's alone when the archive cannot be read. `None` when
/// neither gives blueprints.
fn build(paths: &Paths, archive: Option<&Path>, supply: &dyn Supply, cache_file: &str) -> Option<Built> {
    let topic = versekit_core::archive_lock::reading_for("catalog");
    let game = archive.map(GameData::open_file);
    drop(topic);
    let archive_build = game.as_ref().and_then(|game| game.as_ref().ok()).and_then(GameData::scmdb_build);
    let version = game
        .as_ref()
        .and_then(|game| game.as_ref().ok())
        .and_then(|game| archive_build.clone().or_else(|| game.version().map(str::to_string)));
    // Without an archive, or for one whose build is unknown only when it
    // is the fallback, scmdb's current LIVE build stands in.
    let wanted = match (&game, &archive_build) {
        (_, Some(build)) => Some(build.clone()),
        (Some(Ok(_)), None) => None,
        _ => supply.current_build().ok(),
    };
    let (fixes, fixes_error) = supply.corrections(wanted.as_deref());
    let fixes_missing = fixes_error.is_some();
    let read = game.and_then(Result::ok).map(|game| {
        let catalog = game.localization(Language::English).and_then(|texts| game.blueprints_corrected(&texts, &fixes));
        if let Err(error) = &catalog {
            versekit_core::errors::record("list.archive", error);
        }
        catalog
    });
    let theirs = wanted.as_deref().map(|build| supply.catalog(build, &fixes));
    let (their_catalog, their_error) = match theirs {
        Some(Ok(mut catalog)) => {
            // Whom the contracts credit reputation, for the notes in the
            // game text; kept only when there is something (4.12 `refresh`).
            if let Some(table) = catalog.reputation.take().filter(|t| !t.contracts.is_empty()) {
                versekit_core::injection::reputation::save(paths, &table);
            }
            (Some(catalog), None)
        }
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let trouble = fixes_error.filter(|_| their_catalog.is_none()).or(their_error);
    if let Some(error) = trouble.filter(|_| !supply.is_off() && wanted.is_some()) {
        versekit_core::errors::record("list.scmdb", &error);
    }
    let own = read.as_ref().and_then(|read| read.as_ref().ok()).filter(|catalog| !catalog.blueprints.is_empty());
    let texts = own.map(item_texts).unwrap_or_default();
    // Counts as brought only with the list of known errors as well: without
    // it names, pools and factions stay uncorrected and scmdb is asked again.
    let scmdb_build = their_catalog.as_ref().filter(|_| !fixes_missing).and(wanted.clone());
    let mut catalog = match own {
        Some(game) => {
            let full = from_game(game, version.as_deref().unwrap_or_default(), their_catalog.as_ref());
            if let Some(theirs) = &their_catalog {
                let check = cross_check(&full, theirs);
                if check.differences() > 0 {
                    let build = wanted.as_deref().unwrap_or("?");
                    note(
                        "list.crosscheck",
                        "s_p5_gegenprobe",
                        &format!("{build}: {} differences; {}", check.differences(), check.summary()),
                    );
                }
            }
            full
        }
        None => {
            let mut theirs = their_catalog?;
            if !theirs.blueprints.is_empty() {
                let why = match read.as_ref() {
                    Some(Err(error)) => error.to_string(),
                    _ => "Data.p4k".to_string(),
                };
                note("list.fallback", "s_p5_rueckfall", &why);
                if let Some(build) = &wanted {
                    theirs.version.clone_from(build);
                }
                theirs
            } else {
                return None;
            }
        }
    };
    // A build without the network does not feed the history: its names are
    // not yet corrected by scmdb's list, and the window builds again anyway.
    stamp_since(paths, &mut catalog, cache_file, !supply.is_off());
    Some(Built { catalog, item_texts: texts, scmdb: scmdb_build, asked: wanted.is_some() })
}

/// Sets the patch each blueprint came with from this device's history and,
/// with `feed`, feeds the history with what is new since the last build.
///
/// What is new is compared with everything this device ever saw; where that
/// list is empty, with the last catalog built (or, failing that, the 4.12
/// cache), and a first build ever counts nothing as new, because there is
/// nothing to compare with. A blueprint without a way is not an addition and
/// is not kept as seen either, so that it counts when a patch gives it one.
fn stamp_since(paths: &Paths, catalog: &mut CatalogCache, cache_file: &str, feed: bool) {
    let mut known = patch_history::seen(paths);
    if known.is_empty() {
        let before =
            jsonio::read_json(cache_file).ok().and_then(|data| non_empty(&data)).or_else(|| catalog_cache::load(paths));
        known = before.iter().flat_map(|c| c.blueprints.iter().filter(|b| !b.no_way).map(|b| b.key.clone())).collect();
    }
    let keys: BTreeSet<String> = catalog.blueprints.iter().filter(|b| !b.no_way).map(|b| b.key.clone()).collect();
    let report = |error: &std::io::Error| {
        if !writeguard::refused(error) {
            versekit_core::errors::record("list.history_write", error);
        }
    };
    if feed && !known.is_empty() {
        let additions: Vec<String> = catalog
            .blueprints
            .iter()
            .filter(|b| !known.contains(&b.key) && !b.no_way)
            .map(|b| b.name.clone())
            .collect();
        if let Err(error) = patch_history::record(paths, &catalog.version, &additions, None) {
            report(&error);
        }
    }
    if feed {
        known.extend(keys);
        if let Err(error) = patch_history::set_seen(paths, &known) {
            report(&error);
        }
    }
    let since = patch_history::version_per_blueprint(&patch_history::load(paths));
    for blueprint in &mut catalog.blueprints {
        blueprint.since = since.get(&blueprint.key).cloned();
    }
}

/// The difference between the own reading and scmdb's catalog of the same
/// build.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrossCheck {
    /// Blueprints only the own reading names.
    pub only_own: Vec<String>,
    /// Blueprints only scmdb names.
    pub only_scmdb: Vec<String>,
    /// Blueprints whose ways differ (contract, faction, reputation needed).
    pub ways: Vec<String>,
    /// Blueprints whose contracts differ in the reputation limit or in
    /// shareable.
    pub missions: Vec<String>,
    /// Blueprints whose shortest cooldown differs. Not counted as a
    /// difference: scmdb reads the cooldown of a generator's first handler
    /// for all its contracts, the data state it per handler.
    pub cooldowns: Vec<String>,
}

impl CrossCheck {
    /// How many differences there are.
    pub fn differences(&self) -> usize {
        self.only_own.len() + self.only_scmdb.len() + self.ways.len() + self.missions.len()
    }

    /// The counts and the first example of each kind, in one line.
    pub fn summary(&self) -> String {
        let part = |what: &str, list: &[String]| {
            (!list.is_empty())
                .then(|| format!("{what} {} (e.g. {})", list.len(), list.first().cloned().unwrap_or_default()))
        };
        [
            part("only own", &self.only_own),
            part("only scmdb", &self.only_scmdb),
            part("ways", &self.ways),
            part("contracts", &self.missions),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("; ")
    }
}

/// What a way is, apart from what the data does not state: the contract,
/// the faction and the reputation needed.
fn way_id(way: &Way) -> (String, String, Option<i64>) {
    (way.contract.clone(), way.faction.clone(), way.rank_reputation)
}

/// Compares the catalog read from the archive (`own`, as [`from_game`] makes
/// it, so with the rewards of scmdb where they decide between variants of a
/// contract) with scmdb's `theirs`: the blueprints, what identifies their
/// ways (contract, faction, reputation needed) and the limits of the
/// contracts. Blueprints without any way in scmdb's (it lists only what a
/// reward pool holds) are not compared.
pub fn cross_check(own: &CatalogCache, theirs: &CatalogCache) -> CrossCheck {
    let mut check = CrossCheck::default();
    let ours: HashMap<&str, &ListBlueprint> = own.blueprints.iter().map(|b| (b.key.as_str(), b)).collect();
    let others: HashMap<&str, &ListBlueprint> = theirs.blueprints.iter().map(|b| (b.key.as_str(), b)).collect();
    for key in others.keys().filter(|key| !ours.contains_key(**key)) {
        check.only_scmdb.push((*key).to_string());
    }
    for (key, blueprint) in &ours {
        let Some(other) = others.get(key) else {
            if !blueprint.ways.is_empty() {
                check.only_own.push((*key).to_string());
            }
            continue;
        };
        let mut mine: Vec<_> = blueprint.ways.iter().map(way_id).collect();
        let mut theirs: Vec<_> = other.ways.iter().map(way_id).collect();
        mine.sort();
        theirs.sort();
        mine.dedup();
        theirs.dedup();
        if mine != theirs {
            check.ways.push((*key).to_string());
        }
    }
    // What the list shows of the contracts of a blueprint: the reputation
    // limit that makes it vanish, shareable, the shortest cooldown.
    let describe = |catalog: &CatalogCache| {
        let caps = versekit_core::blueprints::reward_caps(catalog);
        let mut found: BTreeMap<String, (String, Option<i64>)> = BTreeMap::new();
        for key in catalog.missions.iter().flat_map(|m| &m.blueprints) {
            let (shareable, lockout) = versekit_core::blueprints::contract_traits(catalog, key);
            found.insert(key.clone(), (format!("Grenze {:?} teilbar {shareable:?}", caps.get(key)), lockout));
        }
        found
    };
    let (mine, others) = (describe(own), describe(theirs));
    for key in mine.keys().chain(others.keys()).collect::<BTreeSet<_>>() {
        let (a, b) = (mine.get(key), others.get(key));
        if a.map(|x| &x.0) != b.map(|x| &x.0) {
            check.missions.push(format!("{key}: eigen {:?} gegen scmdb {:?}", a.map(|x| &x.0), b.map(|x| &x.0)));
        }
        if a.map(|x| x.1) != b.map(|x| x.1) {
            check.cooldowns.push(format!(
                "{key}: eigen {:?} gegen scmdb {:?}",
                a.and_then(|x| x.1),
                b.and_then(|x| x.1)
            ));
        }
    }
    check.only_own.sort();
    check.only_scmdb.sort();
    check.ways.sort();
    check
}

/// The list catalog from the game's blueprint catalog, completed from
/// scmdb's catalog `theirs` of the same build where the game data say
/// nothing: the reward of a contract, the reputation it gives and where it
/// is taken, grouped by system. Without `theirs` the game data stand alone.
///
/// The patch a blueprint came with (`since`) is not set here; see
/// `stamp_since`.
pub fn from_game(game: &Catalog, game_version: &str, theirs: Option<&CatalogCache>) -> CatalogCache {
    let before: HashMap<&str, &ListBlueprint> =
        theirs.map(|l| l.blueprints.iter().rev().map(|b| (b.key.as_str(), b)).collect()).unwrap_or_default();
    let mut entries: Vec<ListBlueprint> = Vec::new();
    let mut pools: Vec<Vec<String>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for blueprint in &game.blueprints {
        let name = blueprint.name.shown().trim();
        // A blueprint without any name text shows its class name; such a
        // blueprint is left out.
        if name.is_empty() || name == blueprint.class_name {
            continue;
        }
        let key = name_key(name);
        let old = before.get(key.as_str()).copied();
        let ways = blueprint.sources.iter().filter(|s| s.kind == SourceKind::Contract).map(|s| way(s, old));
        // Two records of the same name are one entry, as in 4.12.
        if let Some(&i) = index.get(&key) {
            entries[i].ways.extend(ways);
            entries[i].start |= blueprint.default;
            pools[i].extend(blueprint.pools.iter().cloned());
            continue;
        }
        let item = blueprint.item.as_ref();
        // The recipe of the first record of a name, as the recipes are looked up by name.
        let (recipe_kind, recipe_subtype) = crate::recipes::kind_of_path(&blueprint.record_path);
        index.insert(key.clone(), entries.len());
        pools.push(blueprint.pools.clone());
        entries.push(ListBlueprint {
            key,
            name: name.to_string(),
            kind: item.and_then(|i| i.kind.clone()).unwrap_or_default(),
            sub: item.and_then(|i| i.sub_kind.clone()).unwrap_or_default(),
            size: item.and_then(|i| i.size).filter(|&size| size != 0),
            grade: item.and_then(grade),
            class: item.and_then(|i| i.class.as_deref()).and_then(class),
            maker: item.and_then(maker),
            ways: ways.collect(),
            start: blueprint.default,
            pool: None,
            since: None,
            no_way: false,
            recipe_tag: blueprint.class_name.clone(),
            recipe_subtype,
            recipe_kind,
        });
    }
    for (entry, pools) in entries.iter_mut().zip(&pools) {
        entry.ways = easiest(std::mem::take(&mut entry.ways), before.get(entry.key.as_str()).copied());
        if entry.ways.is_empty() && !pools.is_empty() {
            let readable: BTreeSet<String> = pools.iter().map(|p| pool_readable(p)).collect();
            entry.pool = Some(readable.into_iter().take(2).collect::<Vec<_>>().join(" · ")).filter(|p| !p.is_empty());
        }
        entry.no_way = pools.is_empty() && !entry.start;
    }
    // As 4.12 writes them: first what a reward pool holds, then the rest,
    // each by name.
    entries.sort_by(|a, b| (a.no_way, &a.name).cmp(&(b.no_way, &b.name)));
    CatalogCache {
        // „New in the game“ compares `since` with this version, both in the
        // form scmdb names a build (`4.10.2-live.12913470`).
        version: game_version.into(),
        fetched: theirs.map(|c| c.fetched.clone()).unwrap_or_default(),
        blueprints: entries,
        missions: missions(game, theirs),
        reputation: None,
    }
}

/// The faction a way shows: the faction text, else the contractor's.
fn way_faction(source: &Source) -> String {
    let shown = |text: Option<&crate::Text>| text.map(|t| t.shown().to_string()).filter(|t| !t.is_empty());
    shown(source.faction.as_ref()).or_else(|| shown(source.contractor.as_ref())).unwrap_or_default()
}

/// The contracts that give blueprints, as the list needs them for the
/// reputation limit, „shareable“ and the cooldown: one per contract text,
/// its values from the easiest variant (lowest reputation needed, then the
/// higher reward), as 4.12 takes them.
///
/// Most contracts compute their reward in the game, so the data cannot
/// order variants of equal reputation by it; scmdb's ways (`theirs`) give
/// the reward of a contract by title and faction where they are at hand.
fn missions(game: &Catalog, theirs: Option<&CatalogCache>) -> Vec<Mission> {
    struct Variant<'a> {
        source: &'a Source,
        /// Key → name of the blueprints.
        blueprints: BTreeMap<String, String>,
    }
    let mut rewards: HashMap<(&str, &str), i64> = HashMap::new();
    for way in theirs.iter().flat_map(|c| &c.blueprints).flat_map(|b| &b.ways) {
        if let Some(reward) = way.reward {
            let known = rewards.entry((way.contract.as_str(), way.faction.as_str())).or_insert(reward);
            *known = (*known).max(reward);
        }
    }
    let reward_of = |source: &Source| {
        source.reward_uec.or_else(|| rewards.get(&(source.title.as_str(), way_faction(source).as_str())).copied())
    };
    // What scmdb knows of a contract text beyond that (chance, reputation,
    // reward, rank per blueprint, systems), found by the text key without
    // `@` in lower case.
    let extras: HashMap<String, &Map> = theirs
        .iter()
        .flat_map(|c| &c.missions)
        .filter(|m| !m.extra.is_empty())
        .map(|m| (m.key.trim_start_matches('@').to_lowercase(), &m.extra))
        .collect();
    let mut groups: BTreeMap<String, Vec<Variant<'_>>> = BTreeMap::new();
    for blueprint in &game.blueprints {
        let name = blueprint.name.shown().trim();
        if name.is_empty() || name == blueprint.class_name {
            continue;
        }
        for source in blueprint.sources.iter().filter(|s| s.kind == SourceKind::Contract) {
            let Some(key) = source.mission.key.as_deref().map(|key| key.trim_start_matches('@').to_lowercase()) else {
                continue;
            };
            let variants = groups.entry(key).or_default();
            let same = |v: &Variant<'_>| {
                v.source.id == source.id && v.source.record_path == source.record_path && v.source.title == source.title
            };
            let at = variants.iter().position(same).unwrap_or_else(|| {
                variants.push(Variant { source, blueprints: BTreeMap::new() });
                variants.len() - 1
            });
            variants[at].blueprints.insert(name_key(name), name.to_string());
        }
    }
    let mut found: Vec<Mission> = groups
        .into_values()
        .filter_map(|variants| {
            let easiest = variants.iter().min_by_key(|v| {
                (v.source.min_reputation.unwrap_or(1_000_000_000), std::cmp::Reverse(reward_of(v.source).unwrap_or(0)))
            })?;
            let all: BTreeMap<&String, &String> = variants.iter().flat_map(|v| &v.blueprints).collect();
            // In the order of their names, as 4.12 writes `sorted(all_names)`.
            let mut ordered: Vec<(&String, &String)> = all.into_iter().collect();
            ordered.sort_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)));
            let key = easiest.source.mission.key.as_deref().unwrap_or_default().trim_start_matches('@');
            Some(Mission {
                key: key.to_string(),
                blueprints: ordered.into_iter().map(|(key, _)| key.clone()).collect(),
                reputation_max: easiest.source.max_reputation,
                rank_max: easiest
                    .source
                    .max_rank
                    .as_ref()
                    .map(|rank| rank.shown().to_string())
                    .filter(|r| !r.is_empty()),
                shareable: easiest.source.shareable,
                cooldown: easiest.source.cooldown,
                extra: extras.get(&key.to_lowercase()).map(|extra| (*extra).clone()).unwrap_or_default(),
            })
        })
        .collect();
    // In the order of scmdb, as 4.12 keeps it: contracts that share a
    // description write the block of the last one, so the order decides
    // which one shows. A contract only scmdb knows (the archive gives it no
    // source) stays as scmdb has it: its text is in the game file, and 4.12
    // wrote its notes. Contracts only the archive knows follow by key.
    let mut by_key: HashMap<String, Mission> = found.drain(..).map(|m| (m.key.to_lowercase(), m)).collect();
    let mut ordered = Vec::with_capacity(by_key.len());
    for mission in theirs.iter().flat_map(|c| &c.missions) {
        let key = mission.key.trim_start_matches('@').to_lowercase();
        if mission.key.is_empty() {
            continue;
        }
        match by_key.remove(&key) {
            Some(own) => ordered.push(own),
            None if !ordered.iter().any(|m: &Mission| m.key.trim_start_matches('@').to_lowercase() == key) => {
                ordered.push(mission.clone());
            }
            None => {}
        }
    }
    let mut rest: Vec<Mission> = by_key.into_values().collect();
    rest.sort_by_key(|m| m.key.to_lowercase());
    ordered.extend(rest);
    ordered
}

/// The grade as 4.12 shows it: a component (one with a class) takes the
/// grade its description states, which can differ from the item record.
fn grade(item: &Item) -> Option<i64> {
    if item.class.is_some() { item.described_grade.or(item.grade) } else { item.grade }
}

/// The class as 4.12 shows it: `Energy (Laser)` is `Energy`.
fn class(class: &str) -> Option<String> {
    let plain = class.split_once('(').map_or(class, |(plain, _)| plain);
    Some(py_strip(plain).to_string()).filter(|plain| !plain.is_empty())
}

/// The manufacturer as 4.12 shows it: the name of the manufacturer record,
/// its record name when the name has no text; the manufacturer the
/// description states when that is another one, not just another spelling.
fn maker(item: &Item) -> Option<String> {
    // A maker the known errors correct stays as corrected, whatever the description says.
    if let Some(maker) = item.manufacturer.as_ref().filter(|maker| maker.corrected) {
        return Some(py_strip(maker.name.shown()).to_string()).filter(|name| !name.is_empty());
    }
    let record = item.manufacturer.as_ref().and_then(|maker| {
        let name = py_strip(maker.name.text.as_deref().unwrap_or_default());
        Some(if name.is_empty() { maker.class_name.as_str() } else { name }.to_string()).filter(|n| !n.is_empty())
    });
    match (record, item.described_manufacturer.as_deref()) {
        (Some(record), Some(described)) if !same_maker(&record, described) => Some(described.to_string()),
        (record, _) => record,
    }
}

/// Only the ASCII letters and digits of a manufacturer name, lower case,
/// `&` read as `and`.
fn maker_key(name: &str) -> String {
    name.to_lowercase().replace('&', "and").chars().filter(char::is_ascii_alphanumeric).collect()
}

/// Whether two manufacturer names mean the same one, like 4.12
/// `same_maker`: equal or one inside the other, a short form made of the
/// initials, or spelled nearly alike.
fn same_maker(a: &str, b: &str) -> bool {
    let (ka, kb) = (maker_key(a), maker_key(b));
    if ka.is_empty() || kb.is_empty() {
        return false;
    }
    if ka == kb || ka.contains(&kb) || kb.contains(&ka) {
        return true;
    }
    for (short, long) in [(a, b), (b, a)] {
        let initials: String = long
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter_map(|word| word.chars().next())
            .collect::<String>()
            .to_ascii_lowercase();
        let short = maker_key(short);
        if initials.len() >= 2 && short.len() >= 2 && short.starts_with(&initials) {
            return true;
        }
    }
    similarity(ka.as_bytes(), kb.as_bytes()) >= 0.85
}

/// The similarity of two texts as Python's `difflib.SequenceMatcher.ratio`
/// computes it for short texts: twice the matched characters over all
/// characters, matching the longest common block first, then on both sides.
pub(crate) fn similarity(a: &[u8], b: &[u8]) -> f64 {
    fn matched(a: &[u8], b: &[u8]) -> usize {
        let (mut best, mut at_a, mut at_b) = (0, 0, 0);
        for i in 0..a.len() {
            for j in 0..b.len() {
                let mut k = 0;
                while i + k < a.len() && j + k < b.len() && a[i + k] == b[j + k] {
                    k += 1;
                }
                if k > best {
                    (best, at_a, at_b) = (k, i, j);
                }
            }
        }
        if best == 0 {
            return 0;
        }
        best + matched(&a[..at_a], &b[..at_b]) + matched(&a[at_a + best..], &b[at_b + best..])
    }
    let total = a.len() + b.len();
    if total == 0 { 1.0 } else { 2.0 * matched(a, b) as f64 / total as f64 }
}

/// One way from a contract source; what the game data lacks is taken from
/// the same contract in the entry `old` of scmdb's catalog.
fn way(source: &Source, old: Option<&ListBlueprint>) -> Way {
    let contract = source.title.clone();
    // Without a faction text the contractor's text stands in.
    let faction = way_faction(source);
    let same = old.and_then(|b| {
        b.ways
            .iter()
            .find(|w| w.contract == contract && w.faction == faction)
            .or_else(|| b.ways.iter().find(|w| w.contract == contract))
    });
    Way {
        // scmdb stands in where the data names no mission type.
        kind: source
            .mission_type
            .as_ref()
            .map(|kind| kind.shown().to_string())
            .filter(|kind| !kind.is_empty())
            .or_else(|| same.map(|w| w.kind.clone()))
            .unwrap_or_default(),
        // From scmdb where the game data states no fixed reward (most
        // contracts compute it in the game).
        reward: source.reward_uec.or_else(|| same.and_then(|w| w.reward)),
        // From scmdb where the game data states no reputation gain.
        reputation_gain: source.reputation_gain.or_else(|| same.and_then(|w| w.reputation_gain)),
        rank: source.rank.as_ref().map(|r| r.shown().to_string()).filter(|r| !r.is_empty()),
        rank_reputation: source.min_reputation,
        // From scmdb: the places by system with planets first. A contract
        // scmdb does not know gets the places as the data lists them.
        place: match same {
            Some(w) => w.place.clone(),
            None => places(source),
        },
        contract,
        faction,
    }
}

fn places(source: &Source) -> Option<Place> {
    let mut names: Vec<String> = Vec::new();
    for name in source.places.iter().map(|p| p.shown().trim()).filter(|n| !n.is_empty()) {
        if !names.iter().any(|known| known == name) {
            names.push(name.to_string());
        }
    }
    if names.is_empty() {
        return None;
    }
    let more = names.len().saturating_sub(PLACES_PER_CONTRACT);
    names.truncate(PLACES_PER_CONTRACT);
    Some(Place { system: String::new(), places: names, more: i64::try_from(more).unwrap_or(i64::MAX) })
}

/// The easiest ways first, as 4.12 orders them: lowest reputation needed
/// (none last), then the higher reward, then by contract title ignoring
/// case; one per contract and faction, at most [`WAYS_PER_BLUEPRINT`].
///
/// Of several variants of one contract and faction that are equally easy,
/// the one with the rank and reputation gain of a way in the scmdb entry
/// `old` comes first and is kept.
fn easiest(mut ways: Vec<Way>, old: Option<&ListBlueprint>) -> Vec<Way> {
    let known = |w: &Way| {
        old.is_some_and(|old| {
            old.ways.iter().any(|o| {
                o.contract == w.contract
                    && o.faction == w.faction
                    && o.rank_reputation == w.rank_reputation
                    && o.reputation_gain == w.reputation_gain
            })
        })
    };
    ways.sort_by_cached_key(|w| {
        (
            w.rank_reputation.unwrap_or(1_000_000_000),
            std::cmp::Reverse(w.reward.unwrap_or(0)),
            w.contract.to_lowercase(),
            !known(w),
        )
    });
    let mut seen = std::collections::HashSet::new();
    ways.retain(|w| seen.insert((w.contract.clone(), w.faction.clone())));
    ways.truncate(WAYS_PER_BLUEPRINT);
    ways
}

/// One difference between two list catalogs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The blueprint key; empty for the catalog as a whole.
    pub key: String,
    /// What differs, e.g. `fehlt neu`, `Größe`, `Weg 2 Auftrag`.
    pub what: String,
    pub old: String,
    pub new: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} · {}: {} → {}", self.key, self.what, self.old, self.new)
    }
}

fn shown<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

/// Every difference of what the list shows between `old` and `new`: missing
/// and added blueprints, every field of each blueprint and of each way, and
/// the contracts; empty when the list shows the same.
pub fn differences(old: &CatalogCache, new: &CatalogCache) -> Vec<Difference> {
    let mut found = Vec::new();
    let mut add = |key: &str, what: &str, old: String, new: String| {
        found.push(Difference { key: key.to_string(), what: what.to_string(), old, new });
    };
    let by_key = |catalog: &CatalogCache| -> HashMap<String, ListBlueprint> {
        catalog.blueprints.iter().rev().map(|b| (b.key.clone(), b.clone())).collect()
    };
    let (before, after) = (by_key(old), by_key(new));
    for b in &old.blueprints {
        if !after.contains_key(&b.key) {
            add(&b.key, "fehlt neu", b.name.clone(), String::new());
        }
    }
    for b in &new.blueprints {
        let Some(a) = before.get(&b.key) else {
            add(&b.key, "neu dazu", String::new(), b.name.clone());
            continue;
        };
        let fields: [(&str, String, String); 11] = [
            ("Name", shown(&a.name), shown(&b.name)),
            ("Art", shown(&a.kind), shown(&b.kind)),
            ("Unterart", shown(&a.sub), shown(&b.sub)),
            ("Größe", shown(&a.size), shown(&b.size)),
            ("Grad", shown(&a.grade), shown(&b.grade)),
            ("Klasse", shown(&a.class), shown(&b.class)),
            ("Hersteller", shown(&a.maker), shown(&b.maker)),
            ("Startbauplan", shown(&a.start), shown(&b.start)),
            ("Sonderquelle", shown(&a.pool), shown(&b.pool)),
            ("seit", shown(&a.since), shown(&b.since)),
            ("ohne Weg", shown(&a.no_way), shown(&b.no_way)),
        ];
        for (what, x, y) in fields {
            if x != y {
                add(&b.key, what, x, y);
            }
        }
        if a.ways.len() != b.ways.len() {
            add(&b.key, "Wege", a.ways.len().to_string(), b.ways.len().to_string());
        }
        for (i, (x, y)) in a.ways.iter().zip(&b.ways).enumerate() {
            let parts: [(&str, String, String); 8] = [
                ("Auftrag", shown(&x.contract), shown(&y.contract)),
                ("Typ", shown(&x.kind), shown(&y.kind)),
                ("Fraktion", shown(&x.faction), shown(&y.faction)),
                ("aUEC", shown(&x.reward), shown(&y.reward)),
                ("Ruf", shown(&x.reputation_gain), shown(&y.reputation_gain)),
                ("Rang", shown(&x.rank), shown(&y.rank)),
                ("Rang-Ruf", shown(&x.rank_reputation), shown(&y.rank_reputation)),
                ("Ort", shown(&x.place), shown(&y.place)),
            ];
            for (what, x, y) in parts {
                if x != y {
                    add(&b.key, &format!("Weg {} {what}", i + 1), x, y);
                }
            }
        }
    }
    if old.version != new.version {
        add("", "Version", old.version.clone(), new.version.clone());
    }
    if old.missions != new.missions {
        add("", "Aufträge", old.missions.len().to_string(), new.missions.len().to_string());
    }
    found
}

/// Strips the prefix every reward pool carries: `BP_`, then `MISSION`
/// optionally, `REWARD`, `S` optionally, `_`; ignoring case.
fn strip_pool_prefix(raw: &str) -> &str {
    let upper = raw.to_ascii_uppercase();
    let Some(mut rest) = upper.strip_prefix("BP_") else { return raw };
    rest = rest.strip_prefix("MISSION").unwrap_or(rest);
    let Some(mut rest) = rest.strip_prefix("REWARD") else { return raw };
    rest = rest.strip_prefix('S').unwrap_or(rest);
    let Some(rest) = rest.strip_prefix('_') else { return raw };
    &raw[raw.len() - rest.len()..]
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// A reward pool name as the list shows it, like 4.12 `pool_readable`:
/// `BP_REWARDS_Xenothreat2_15_06` becomes `XenoThreat`; a pool named after
/// an item (all lower case) or with more than two words gives nothing.
pub fn pool_readable(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let core = strip_pool_prefix(raw);
    let plain =
        [("xenothreat", "XenoThreat"), ("rdc_boss", "RDC-Boss"), ("rdc boss", "RDC-Boss"), ("rdcboss", "RDC-Boss")]
            .into_iter()
            .chain([("superheavy", "Super-Heavy-Mission"), ("cds_", "CDS-Rüstung"), ("cds ", "CDS-Rüstung")]);
    for (prefix, readable) in plain {
        if starts_with_ignore_case(core, prefix) {
            return readable.to_string();
        }
    }
    if core == core.to_lowercase() {
        return String::new();
    }
    // The numbered endings (`_15_06`) are tiers of the same pool.
    let mut core = core;
    while let Some(position) = core.rfind('_') {
        let tail = &core[position + 1..];
        if tail.is_empty() || !tail.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        core = &core[..position];
    }
    let words: Vec<&str> = core.split(['_', '-']).filter(|w| !w.is_empty()).collect();
    let text = words.join(" ");
    let text = text.trim();
    if text.split_whitespace().count() > 2 { String::new() } else { text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Item, Manufacturer, Text};

    fn text(shown: &str) -> Text {
        Text { key: None, text: Some(shown.to_string()) }
    }

    fn contract(title: &str, faction: &str, reputation: Option<i64>, reward: Option<i64>) -> Source {
        Source {
            kind: SourceKind::Contract,
            id: title.to_string(),
            record_path: String::new(),
            mission: text(title),
            title: title.to_string(),
            mission_type: None,
            contractor: None,
            faction: Some(text(faction)),
            places: vec![text("Hurston")],
            rank: reputation.map(|_| text("Rank")),
            min_reputation: reputation,
            max_rank: None,
            max_reputation: None,
            shareable: None,
            cooldown: None,
            reputation_gain: None,
            min_points: None,
            reward_uec: reward,
            chance: None,
            pool: "BP_MISSIONREWARD_x".to_string(),
        }
    }

    fn blueprint(name: &str, sources: Vec<Source>, pools: &[&str]) -> crate::Blueprint {
        crate::Blueprint {
            guid: String::new(),
            class_name: format!("BP_CRAFT_{name}"),
            record_path: String::new(),
            name: text(name),
            default: false,
            item: Some(Item {
                kind: Some("Cooler".into()),
                sub_kind: Some("UNDEFINED".into()),
                size: Some(1),
                grade: Some(3),
                class: Some("Civilian".into()),
                manufacturer: Some(Manufacturer { name: text("Lightning Power Ltd."), ..Manufacturer::default() }),
                ..Item::default()
            }),
            sources,
            pools: pools.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[test]
    fn the_cache_keeps_the_text_keys_of_each_crafted_item_once() {
        let made = |guid: &str, name: Option<&str>, description: Option<&str>| {
            let mut b = blueprint("x", Vec::new(), &[]);
            let item = b.item.as_mut().unwrap();
            item.guid = guid.into();
            item.name = Text { key: name.map(str::to_string), text: None };
            item.description_key = description.map(str::to_string);
            b
        };
        let game = Catalog {
            blueprints: vec![
                made(
                    "BEF9CD69",
                    Some("@item_Name_cds_heavy_armor_01_arms"),
                    Some("item_Desc_cds_heavy_armor_01_Shared"),
                ),
                made("bef9cd69", Some("@item_Name_other"), Some("item_Desc_other")),
                made("80fd0b3b", None, Some("item_Desc_cds_heavy_armor_01_Shared")),
                made("8eabeef4", Some("@item_Name_cds_heavy_armor_01_core"), None),
            ],
            ..Catalog::default()
        };
        let texts = item_texts(&game);
        assert_eq!(
            jsonio::dumps(&Value::Object(texts.clone()), None, false),
            r#"{"bef9cd69": {"name": "item_Name_cds_heavy_armor_01_arms", "text": "item_Desc_cds_heavy_armor_01_Shared"}, "80fd0b3b": {"text": "item_Desc_cds_heavy_armor_01_Shared"}}"#
        );
        let dir = std::env::temp_dir().join(format!("versekit-itemtexts-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cache = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        write_cache(&cache, &stamp(None), &CatalogCache::default(), &texts).unwrap();
        let read = versekit_core::injection::item_info::item_texts_from_catalog(&jsonio::read_json(&cache).unwrap());
        assert_eq!(read["80fd0b3b"].text, "item_Desc_cds_heavy_armor_01_Shared");
        assert_eq!(read["bef9cd69"].name, "item_Name_cds_heavy_armor_01_arms");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pool_names_read_like_4_12() {
        assert_eq!(pool_readable("BP_REWARDS_Xenothreat2_15_06"), "XenoThreat");
        assert_eq!(pool_readable("BP_MISSIONREWARD_RDC_Boss_01"), "RDC-Boss");
        assert_eq!(pool_readable("BP_REWARDS_CDS_Armor"), "CDS-Rüstung");
        assert_eq!(pool_readable("BP_REWARDS_RedWind_02"), "RedWind");
        assert_eq!(pool_readable("BP_MISSIONREWARD_behr_rifle_ballistic_01_mr01"), "");
        assert_eq!(pool_readable("BP_REWARDS_Carryable_2H_FL_MissionItem"), "");
        assert_eq!(pool_readable("  "), "");
    }

    #[test]
    fn ways_are_easiest_first_once_per_contract_and_faction() {
        let ways = easiest(
            vec![
                Way { contract: "B".into(), faction: "F".into(), rank_reputation: Some(800), ..Way::default() },
                Way { contract: "A".into(), faction: "F".into(), rank_reputation: None, ..Way::default() },
                Way {
                    contract: "C".into(),
                    faction: "F".into(),
                    rank_reputation: Some(800),
                    reward: Some(9),
                    ..Way::default()
                },
                Way { contract: "B".into(), faction: "F".into(), rank_reputation: Some(0), ..Way::default() },
            ],
            None,
        );
        let order: Vec<&str> = ways.iter().map(|w| w.contract.as_str()).collect();
        assert_eq!(order, ["B", "C", "A"]);
        assert_eq!(ways[0].rank_reputation, Some(0));
    }

    /// Every blueprint carries the tag of its recipe and the kind and subtype its folder names; two records of one
    /// name keep the first one's.
    #[test]
    fn the_blueprints_carry_their_recipe() {
        const ROOT: &str = "libs/foundry/records/crafting/blueprints/crafting/";
        let mut pistol = blueprint("S-38 Pistol", Vec::new(), &["P"]);
        pistol.class_name = "BP_CRAFT_behr_pistol_ballistic_01".into();
        pistol.record_path = format!("{ROOT}fpsgear/weapons/pistol/bp_craft_behr_pistol_ballistic_01");
        let mut twin = blueprint("S-38 Pistol", Vec::new(), &["P"]);
        twin.class_name = "BP_CRAFT_behr_pistol_ballistic_02".into();
        twin.record_path = format!("{ROOT}fpsgear/weapons/rifle/bp_craft_behr_pistol_ballistic_02");
        let mut gatling = blueprint("Gatling", Vec::new(), &["P"]);
        gatling.class_name = "BP_CRAFT_APAR_BallisticGatling_S4".into();
        gatling.record_path = format!("{ROOT}vehiclegear/weapons/ballistic/bp_craft_apar_ballisticgatling_s4");
        let game = Catalog { blueprints: vec![pistol, twin, gatling], ..Catalog::default() };
        let list = from_game(&game, "4.12.0-live.1", None);
        let recipe = |key: &str| {
            let b = list.blueprints.iter().find(|b| b.key == key).expect("the blueprint");
            (b.recipe_tag.as_str(), b.recipe_subtype.as_str(), b.recipe_kind.as_str())
        };
        assert_eq!(recipe("s-38 pistol"), ("BP_CRAFT_behr_pistol_ballistic_01", "pistol", "weapons"));
        assert_eq!(recipe("gatling"), ("BP_CRAFT_APAR_BallisticGatling_S4", "ballistic", "weapons"));
    }

    #[test]
    fn scmdb_completes_what_the_game_data_do_not_state() {
        let theirs = catalog_cache::parse(
            &jsonio::loads(
                r#"{"version": "4.10.2-live.1", "bauplaene": {
                "arcticstorm": {"n": "ArcticStorm",
                    "q": [{"auftrag": "Protect Fuel Tanks", "typ": "Mercenary", "fraktion": "Foxwell", "uec": 91750,
                           "ruf": 100, "wo": {"system": "Nyx", "orte": ["Nyx I"], "mehr": 0}}]}}}"#,
            )
            .unwrap(),
        );
        let mut pistol = blueprint("S-38 Pistol", Vec::new(), &[]);
        pistol.default = true;
        let mut source = contract("Protect Fuel Tanks", "Foxwell", Some(2200), None);
        (source.max_reputation, source.shareable, source.cooldown) = (Some(95250), Some(true), Some(45));
        source.mission = Text { key: Some("@Protect_Title".into()), text: Some("Protect Fuel Tanks".into()) };
        let game = Catalog {
            blueprints: vec![
                blueprint("ArcticStorm", vec![source], &["P"]),
                pistol,
                blueprint("Lonely", Vec::new(), &[]),
                blueprint("Xeno", Vec::new(), &["BP_REWARDS_Xenothreat2_15_06"]),
            ],
            ..Catalog::default()
        };
        let list = from_game(&game, "4.10.2-live.1", Some(&theirs));
        let keys: Vec<&str> = list.blueprints.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["arcticstorm", "s-38 pistol", "xeno", "lonely"]);
        let cooler = &list.blueprints[0];
        assert_eq!((cooler.size, cooler.grade, cooler.class.as_deref()), (Some(1), Some(3), Some("Civilian")));
        assert_eq!(cooler.maker.as_deref(), Some("Lightning Power Ltd."));
        let way = &cooler.ways[0];
        assert_eq!((way.kind.as_str(), way.reward, way.reputation_gain), ("Mercenary", Some(91750), Some(100)));
        assert_eq!((way.rank.as_deref(), way.rank_reputation), (Some("Rank"), Some(2200)));
        assert_eq!(way.place.as_ref().map(|p| p.system.as_str()), Some("Nyx"));
        assert!(list.blueprints[1].start && !list.blueprints[1].no_way, "a default blueprint of the data is a starter");
        assert_eq!(list.blueprints[2].pool.as_deref(), Some("XenoThreat"));
        assert!(!list.blueprints[2].no_way);
        assert!(list.blueprints[3].no_way && list.blueprints[3].pool.is_none());
        assert_eq!(list.version, "4.10.2-live.1");
        assert!(list.blueprints.iter().all(|b| b.since.is_none()), "the patch comes from the history, not from here");
        // The limit, shareable and cooldown of the contract come from the data.
        let mission = &list.missions[0];
        assert_eq!(mission.blueprints, ["arcticstorm"]);
        assert_eq!((mission.reputation_max, mission.shareable, mission.cooldown), (Some(95250), Some(true), Some(45)));
    }

    #[test]
    fn item_values_read_like_4_12() {
        assert_eq!(class("Energy (Laser)").as_deref(), Some("Energy"));
        assert_eq!(class("Energy\u{a0}(Laser)").as_deref(), Some("Energy"));
        assert_eq!(class("Military").as_deref(), Some("Military"));
        assert_eq!(class(" (Laser)"), None);
        let item = |class: Option<&str>, grade, described_grade| Item {
            class: class.map(str::to_string),
            grade,
            described_grade,
            ..Item::default()
        };
        assert_eq!(grade(&item(Some("Civilian"), Some(1), Some(3))), Some(3), "a component takes the described grade");
        assert_eq!(grade(&item(None, Some(1), Some(3))), Some(1), "anything else keeps the record's");
        assert_eq!(grade(&item(Some("Civilian"), Some(2), None)), Some(2));
    }

    #[test]
    fn the_maker_reads_like_4_12() {
        let item = |name: Option<&str>, class_name: &str, described: Option<&str>| Item {
            manufacturer: Some(Manufacturer {
                class_name: class_name.into(),
                name: Text { key: Some("@manufacturer_Name".into()), text: name.map(str::to_string) },
                ..Manufacturer::default()
            }),
            described_manufacturer: described.map(str::to_string),
            ..Item::default()
        };
        assert_eq!(maker(&item(Some("Caldera "), "CLDA", None)).as_deref(), Some("Caldera"));
        assert_eq!(
            maker(&item(None, "LBCO", Some("Lightning Bolt Co."))).as_deref(),
            Some("LBCO"),
            "a name without text is the record name; the description names the same maker by its initials"
        );
        assert_eq!(
            maker(&item(Some("Klaus & Werner"), "KLWE", Some("Verified Offworld Laser Technologies"))).as_deref(),
            Some("Verified Offworld Laser Technologies"),
            "the description names another maker"
        );
        assert_eq!(
            maker(&item(Some("Lightning Power Ltd."), "LPLT", Some("Lighting Power Ltd."))).as_deref(),
            Some("Lightning Power Ltd."),
            "another spelling is the same maker"
        );
        assert_eq!(maker(&Item::default()), None);
        assert!((similarity(b"lightingpowerltd", b"lightningpowerltd") - 32.0 / 33.0).abs() < 1e-12);
        assert!(
            (similarity(b"greycatindustrial", b"musashiindustrialandstarflightconcern") - 22.0 / 54.0).abs() < 1e-12
        );
        assert!((similarity(b"abcd", b"bcda") - 0.75).abs() < 1e-12);
    }

    #[test]
    fn the_list_follows_4_12_where_the_data_says_more_or_less() {
        let legacy = catalog_cache::parse(
            &jsonio::loads(
                r#"{"version": "4.10.2-live.1", "bauplaene": {
                "sniper": {"n": "Sniper", "q": [
                    {"auftrag": "[TARGET] needs stomping", "typ": "Bounty Hunter", "fraktion": "Headhunters",
                     "uec": 43250, "rep": 800},
                    {"auftrag": "Target at [LOCATION]", "typ": "Bounty Hunter", "fraktion": "Headhunters",
                     "uec": 43250, "rep": 800},
                    {"auftrag": "Wanted: [TARGET]", "typ": "Mercenary", "fraktion": "Bounty Hunters Guild",
                     "uec": 113000, "ruf": 16000, "rep": 1}]}}}"#,
            )
            .unwrap(),
        );
        let wanted = |gain| Source {
            reputation_gain: Some(gain),
            mission_type: Some(text("Bounty Hunter")),
            places: Vec::new(),
            ..contract("Wanted: [TARGET]", "Bounty Hunters Guild", Some(1), None)
        };
        let mut nameless = blueprint("nameless", Vec::new(), &["P"]);
        nameless.name = text("BP_CRAFT_nameless");
        let placeholder = Text { key: Some("@LOC_PLACEHOLDER".into()), text: None };
        let haul = Source { faction: Some(placeholder), ..contract("Haul", "", Some(0), None) };
        let mut small = blueprint("Small", vec![haul], &["P"]);
        small.sources[0].contractor = Some(text("Hockrow Agency"));
        if let Some(item) = small.item.as_mut() {
            item.size = Some(0);
        }
        let game = Catalog {
            blueprints: vec![
                blueprint(
                    "Sniper",
                    vec![
                        contract("Target at [LOCATION]", "Headhunters", Some(800), None),
                        contract("[TARGET] needs stomping", "Headhunters", Some(800), None),
                        wanted(1000),
                        wanted(16000),
                    ],
                    &["P"],
                ),
                nameless,
                small,
            ],
            ..Catalog::default()
        };
        let list = from_game(&game, "4.10.197.1", Some(&legacy));
        let keys: Vec<&str> = list.blueprints.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(keys, ["small", "sniper"], "a blueprint without any name text is left out");
        let small = &list.blueprints[0];
        assert_eq!(small.size, None, "size 0 shows no size");
        assert_eq!(small.ways[0].faction, "Hockrow Agency", "without a faction text the contractor stands in");
        let ways = &list.blueprints[1].ways;
        let titles: Vec<&str> = ways.iter().map(|w| w.contract.as_str()).collect();
        assert_eq!(titles, ["Wanted: [TARGET]", "[TARGET] needs stomping", "Target at [LOCATION]"]);
        assert_eq!(ways[0].reputation_gain, Some(16000), "the variant of the 4.12 way is kept");
        assert_eq!(ways[0].reward, Some(113000));
        assert_eq!(ways[0].kind, "Bounty Hunter", "the mission type of the data comes first");
        assert_eq!(ways[1].kind, "Bounty Hunter", "without one scmdb's catalog stands in");
        assert_eq!(ways[1].place, None, "a way 4.12 knows takes its place, also none");
    }

    /// A supply with the network on that is never asked.
    struct Online;

    impl Supply for Online {
        fn current_build(&self) -> Result<String, String> {
            unreachable!("the look at the craft data asks nobody")
        }
        fn corrections(&self, _: Option<&str>) -> (Vec<Correction>, Option<String>) {
            unreachable!("the look at the craft data asks nobody")
        }
        fn catalog(&self, _: &str, _: &[Correction]) -> Result<CatalogCache, String> {
            unreachable!("the look at the craft data asks nobody")
        }
    }

    #[test]
    fn the_craft_data_lag_behind_a_new_build_only_where_scmdb_stands_in_for_the_archive() {
        let dir = std::env::temp_dir().join(format!("versekit-craftdaten-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cache = dir.join(CACHE_FILE).to_string_lossy().into_owned();
        let write = |archive: Option<&Path>, marker: &str, tried: u64, version: &str| {
            let mut source = stamp(archive);
            if let Value::Object(map) = &mut source {
                map.push(("scmdb".into(), Value::str(marker)));
                map.push(("versuch".into(), Value::str(tried.to_string())));
            }
            write_cache(&cache, &source, &keyed(&["Alpha"], version), &Map::new()).unwrap();
        };
        let now = 1_800_000_000;
        assert!(craft_data_behind_file(None, &cache, "4.10.2-live.1", &Online, now), "no catalog at all");
        // Without an archive scmdb's catalog stands in: a new build at scmdb is new craft data.
        write(None, "4.10.2-live.1", now, "4.10.2-live.1");
        assert!(!craft_data_behind_file(None, &cache, "4.10.2-live.1", &Online, now));
        assert!(!craft_data_behind_file(None, &cache, "4.10.2-live.1", &Online, now + 7 * 3600), "the build counts");
        assert!(craft_data_behind_file(None, &cache, "4.11.0-live.2", &Online, now));
        // With an archive the archive names the build: scmdb's newer one does not count, a patch does.
        let archive = dir.join(ARCHIVE_NAME);
        fs::write(&archive, b"p4k").unwrap();
        assert!(craft_data_behind_file(Some(&archive), &cache, "4.10.2-live.1", &Online, now), "built without it");
        write(Some(&archive), "4.10.2-live.1", now, "4.10.2-live.1");
        assert!(!craft_data_behind_file(Some(&archive), &cache, "4.11.0-live.2", &Online, now));
        // scmdb's part missing: asked for again after the retry time, not before.
        write(Some(&archive), "", now, "4.10.2-live.1");
        assert!(!craft_data_behind_file(Some(&archive), &cache, "4.10.2-live.1", &Online, now + 3600));
        assert!(craft_data_behind_file(Some(&archive), &cache, "4.10.2-live.1", &Online, now + RETRY_SECONDS));
        write(Some(&archive), "4.10.2-live.1", now, "4.10.2-live.1");
        fs::write(&archive, b"patched p4k").unwrap();
        assert!(craft_data_behind_file(Some(&archive), &cache, "4.10.2-live.1", &Online, now), "a patch");
        let _ = fs::remove_dir_all(dir);
    }

    fn keyed(names: &[&str], version: &str) -> CatalogCache {
        let blueprints = names
            .iter()
            .map(|name| ListBlueprint {
                key: name_key(name),
                name: (*name).to_string(),
                no_way: name.starts_with("Lost"),
                ..ListBlueprint::default()
            })
            .collect();
        CatalogCache { version: version.into(), blueprints, ..CatalogCache::default() }
    }

    #[test]
    fn the_patch_of_a_blueprint_is_remembered_from_the_second_build_on() {
        let dir = std::env::temp_dir().join(format!("versekit-since-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(versekit_core::pypath::Flavor::native(), &[("SC_BP_HOME", &text)], &text));
        let cache = paths.app_file(CACHE_FILE);

        // The first build ever has nothing to compare with: nothing is new.
        let mut first = keyed(&["Alpha", "Beta"], "4.12.0-live.1");
        stamp_since(&paths, &mut first, &cache, true);
        assert!(first.blueprints.iter().all(|b| b.since.is_none()));
        assert_eq!(patch_history::seen(&paths).len(), 2);

        // A build without the network reads the history but does not feed it.
        let mut quiet = keyed(&["Alpha", "Beta", "Omega"], "4.12.1-live.1");
        stamp_since(&paths, &mut quiet, &cache, false);
        assert_eq!(patch_history::seen(&paths).len(), 2);
        assert!(!Path::new(&paths.app_file(patch_history::FILE)).exists());

        // The next build names what came with its patch, but not what has no way yet.
        let mut second = keyed(&["Alpha", "Beta", "Gamma", "Lost Delta"], "4.12.1-live.2");
        stamp_since(&paths, &mut second, &cache, true);
        let since: Vec<Option<&str>> = second.blueprints.iter().map(|b| b.since.as_deref()).collect();
        assert_eq!(since, [None, None, Some("4.12.1-live.2"), None]);
        assert_eq!(patch_history::seen(&paths).len(), 3, "what has no way is not kept as seen");

        // A third build with the same blueprints brings nothing new and keeps the stamp.
        let mut third = keyed(&["Alpha", "Beta", "Gamma", "Lost Delta"], "4.12.1-live.3");
        stamp_since(&paths, &mut third, &cache, true);
        assert_eq!(third.blueprints[2].since.as_deref(), Some("4.12.1-live.2"));

        // A blueprint that had no way and gets one with a patch counts as new then.
        let mut fourth = keyed(&["Alpha", "Beta", "Gamma", "Delta"], "4.12.2-live.4");
        stamp_since(&paths, &mut fourth, &cache, true);
        assert_eq!(fourth.blueprints[3].since.as_deref(), Some("4.12.2-live.4"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn contracts_with_one_text_are_one_mission_with_the_easiest_variants_values() {
        let mut easy = contract("Hunt", "F", Some(0), Some(10));
        let mut hard = contract("Hunt", "F", Some(5000), Some(99));
        for (source, id) in [(&mut easy, "Easy"), (&mut hard, "Hard")] {
            source.mission = Text { key: Some("@Hunt_Title".into()), text: Some("Hunt".into()) };
            source.id = id.into();
        }
        (easy.max_reputation, easy.cooldown) = (Some(2200), Some(30));
        (hard.max_reputation, hard.cooldown) = (Some(95250), Some(0));
        let game = Catalog { blueprints: vec![blueprint("One", vec![hard, easy], &["P"])], ..Catalog::default() };
        let missions = missions(&game, None);
        assert_eq!(missions.len(), 1);
        assert_eq!(missions[0].blueprints, ["one"]);
        assert_eq!((missions[0].reputation_max, missions[0].cooldown), (Some(2200), Some(30)), "the lowest reputation");
    }

    #[test]
    fn the_details_scmdb_knows_of_a_contract_text_are_attached_by_its_key() {
        let mut hunt = contract("Hunt", "F", Some(0), Some(10));
        hunt.mission = Text { key: Some("@Hunt_Title".into()), text: Some("Hunt".into()) };
        let mut lone = contract("Lone", "F", Some(0), Some(10));
        lone.mission = Text { key: Some("Lone_Title".into()), text: Some("Lone".into()) };
        let game = Catalog { blueprints: vec![blueprint("One", vec![hunt, lone], &["P"])], ..Catalog::default() };
        // scmdb keeps the key in another case and the details as 4.12 does.
        let details: Map = vec![("sicher".into(), Value::Bool(true)), ("ruf".into(), Value::int(150))];
        let theirs = CatalogCache {
            missions: vec![
                Mission { key: "hunt_TITLE".into(), extra: details.clone(), ..Mission::default() },
                Mission {
                    key: "other".into(),
                    extra: vec![("sicher".into(), Value::Bool(false))],
                    ..Mission::default()
                },
            ],
            ..CatalogCache::default()
        };
        let found = missions(&game, Some(&theirs));
        let extra = |key: &str| found.iter().find(|m| m.key == key).map(|m| m.extra.clone());
        assert_eq!(extra("Hunt_Title"), Some(details));
        // No scmdb contract of that text: nothing attached.
        assert_eq!(extra("Lone_Title"), Some(Map::new()));
        // Without scmdb nothing is attached.
        assert!(missions(&game, None).iter().all(|m| m.extra.is_empty()));
        // A contract only scmdb knows is kept as scmdb has it, once.
        assert_eq!(found.iter().filter(|m| m.key == "other").count(), 1);
        assert_eq!(extra("other"), Some(vec![("sicher".into(), Value::Bool(false))]));
        assert_eq!(found.iter().filter(|m| m.key.eq_ignore_ascii_case("hunt_title")).count(), 1);
    }

    #[test]
    fn contracts_keep_the_order_of_scmdb_and_the_rest_follows_by_key() {
        let titled = |key: &str| {
            let mut source = contract(key, "F", Some(0), Some(10));
            source.mission = Text { key: Some(format!("@{key}")), text: Some(key.into()) };
            source.id = key.into();
            source
        };
        let game = Catalog {
            blueprints: vec![blueprint("One", vec![titled("A_Title"), titled("B_Title"), titled("C_Title")], &["P"])],
            ..Catalog::default()
        };
        // Contracts that share a description write the last one's block, so
        // the order must be the one 4.12 had.
        let theirs = CatalogCache {
            missions: vec![
                Mission { key: "B_TITLE".into(), ..Mission::default() },
                Mission { key: "Other".into(), ..Mission::default() },
                Mission { key: "A_Title".into(), ..Mission::default() },
            ],
            ..CatalogCache::default()
        };
        let keys: Vec<String> = missions(&game, Some(&theirs)).into_iter().map(|m| m.key).collect();
        assert_eq!(keys, ["B_Title", "Other", "A_Title", "C_Title"]);
    }

    #[test]
    fn the_cross_check_names_what_differs() {
        let mine = keyed(&["Alpha", "Beta"], "v");
        let mut theirs = keyed(&["Alpha", "Gamma"], "v");
        let way = |title: &str| Way { contract: title.into(), faction: "F".into(), ..Way::default() };
        let mut own = mine.clone();
        own.blueprints[0].ways = vec![way("A")];
        theirs.blueprints[0].ways = vec![way("B")];
        let check = cross_check(&own, &theirs);
        assert_eq!((check.only_own.len(), check.only_scmdb.len(), check.ways.len()), (0, 1, 1));
        assert_eq!(check.differences(), 2);
        assert!(check.summary().contains("only scmdb 1"));
        assert_eq!(cross_check(&theirs, &theirs).differences(), 0);

        // The limit and shareable count as differences, the cooldown is told apart and not counted.
        let mission = |cap, cooldown| Mission {
            blueprints: vec!["alpha".into()],
            reputation_max: cap,
            shareable: Some(true),
            cooldown,
            ..Mission::default()
        };
        let (mut a, mut b) = (keyed(&["Alpha"], "v"), keyed(&["Alpha"], "v"));
        a.missions = vec![mission(Some(5000), Some(45))];
        b.missions = vec![mission(Some(5000), None)];
        let check = cross_check(&a, &b);
        assert_eq!((check.differences(), check.cooldowns.len()), (0, 1), "only the cooldown differs");
        b.missions = vec![mission(Some(9000), Some(45))];
        let check = cross_check(&a, &b);
        assert_eq!((check.missions.len(), check.cooldowns.len()), (1, 0));
        assert!(check.summary().contains("contracts 1"), "{}", check.summary());
    }

    #[test]
    fn a_corrected_maker_stays_as_corrected_whatever_the_description_says() {
        let item = |corrected| Item {
            manufacturer: Some(Manufacturer { name: text("Meant Maker"), corrected, ..Manufacturer::default() }),
            described_manufacturer: Some("Somebody Else Entirely".into()),
            ..Item::default()
        };
        assert_eq!(maker(&item(true)).as_deref(), Some("Meant Maker"));
        assert_eq!(
            maker(&item(false)).as_deref(),
            Some("Somebody Else Entirely"),
            "else the description wins, as in 4.12"
        );
    }

    #[test]
    fn without_scmdb_the_game_data_stand_alone() {
        let game = Catalog {
            blueprints: vec![blueprint("ArcticStorm", vec![contract("Protect", "Foxwell", Some(0), Some(5))], &["P"])],
            ..Catalog::default()
        };
        let list = from_game(&game, "4.10.197.1", None);
        assert_eq!(list.version, "4.10.197.1");
        let way = &list.blueprints[0].ways[0];
        assert_eq!((way.kind.as_str(), way.reward), ("", Some(5)));
        let place = way.place.as_ref().unwrap();
        assert_eq!((place.system.as_str(), place.places.as_slice(), place.more), ("", &["Hurston".to_string()][..], 0));
    }

    /// The build thread reads the language of the thread that started it,
    /// whichever language the program reads.
    #[test]
    fn the_build_thread_reads_the_language_of_its_starter() {
        use std::time::Duration;
        use versekit_core::language::{Choice, Lang, current, set_thread_language, t, t_in};
        let program = std::thread::spawn(current).join().unwrap();
        let (other, choice) = if program == Lang::En { (Lang::De, Choice::De) } else { (Lang::En, Choice::En) };
        set_thread_language(choice);
        let seen = Arc::new(Mutex::new(None));
        let pending = {
            let seen = Arc::clone(&seen);
            Pending::spawn(move || {
                *seen.lock().unwrap() = Some((current(), t("bauplaene")));
                None
            })
            .unwrap()
        };
        let started = std::time::Instant::now();
        while matches!(pending.poll(), Progress::Waiting) && started.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(*seen.lock().unwrap(), Some((other, t_in(other, "bauplaene"))));
        set_thread_language(Choice::Auto);
    }
}
