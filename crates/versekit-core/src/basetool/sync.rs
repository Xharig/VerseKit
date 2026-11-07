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

//! The sync in the background (`basetool_sync.tick`, `run`, `_run`), the
//! state it publishes, jobs of the pages and the device login of the
//! settings page (`basetool_page._start_login`).
//!
//! **Pages keep their state.** An area publishes its data as a `Shared`
//! value with a generation number. A page compares the number with the one
//! it last drew and takes the new data in; its own state (open form,
//! filters, search, scroll position) stays. Nothing here rebuilds a page.
//!
//! **One pass at a time.** If the timer and a button met, one would read the
//! feed while the other already sends, and a deletion mark would get lost.
//!
//! **Order of a pass:** service document (installation id, rights, minimum
//! version), name of the installation, account check, then every
//! switched-on area with its rights. An area whose errors must not stop the
//! others says so (`isolated`).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::basetool::connection::{Clock, Connection, DeviceLogin, Poll, get, get_str};
use crate::basetool::error::{Action, ApiError, jitter};
use crate::basetool::feed::Locations;
use crate::basetool::hold::{self, Confirmations};
use crate::basetool::schedule::{MIN_WAIT, Schedule};
use crate::basetool::scopes::{self, AREAS, SETTING_LABEL, Switches, all_in};
use crate::basetool::state;
use crate::jsonio::{Map, Value, map_set};
use crate::logsource::ALL_ACCOUNTS;
use crate::paths::Paths;
use crate::writeguard;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The syncs with a worker, by the folder of their own data: a write of the own stock files, the
/// hangar or the own ship names in that folder starts their wait for a pass.
static SAVED: Mutex<Vec<(String, std::sync::Weak<Engine>)>> = Mutex::new(Vec::new());

fn data_folder(paths: &Paths) -> String {
    crate::pypath::Flavor::native().dirname(&paths.app_file(crate::hangar::FILE))
}

/// Called after a write of the own stock files, the hangar or `asop.json`
/// (`basetool_sync.store_saved`): with stock or hangar switched on the next
/// pass follows soon, collected. Writes of the sync itself do not count.
pub fn store_saved(file: &str) {
    let folder = crate::pypath::Flavor::native().dirname(file);
    let engines: Vec<Arc<Engine>> = {
        let mut all = lock(&SAVED);
        all.retain(|(_, engine)| engine.strong_count() > 0);
        all.iter().filter(|(own, _)| *own == folder).filter_map(|(_, engine)| engine.upgrade()).collect()
    };
    for engine in engines {
        let switches: &dyn Switches = engine.paths.as_ref();
        if switches.on(scopes::SETTING_STOCK) || switches.on(scopes::SETTING_SHIPS) {
            engine.note_local_change();
        }
    }
}

/// Data published for pages: a value and the generation it came with.
pub struct Shared<T> {
    slot: Arc<Mutex<(u64, Option<Arc<T>>)>>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Shared<T> {
        Shared { slot: self.slot.clone() }
    }
}

impl<T> Default for Shared<T> {
    fn default() -> Shared<T> {
        Shared { slot: Arc::new(Mutex::new((0, None))) }
    }
}

impl<T> Shared<T> {
    /// A new value; the generation grows by one.
    pub fn publish(&self, value: T) -> u64 {
        let mut slot = lock(&self.slot);
        slot.0 += 1;
        slot.1 = Some(Arc::new(value));
        slot.0
    }

    /// No value any more (disconnected, switched off, right missing).
    pub fn clear(&self) -> u64 {
        let mut slot = lock(&self.slot);
        slot.0 += 1;
        slot.1 = None;
        slot.0
    }

    pub fn generation(&self) -> u64 {
        lock(&self.slot).0
    }

    /// Generation and value.
    pub fn get(&self) -> (u64, Option<Arc<T>>) {
        let slot = lock(&self.slot);
        (slot.0, slot.1.clone())
    }

    /// The value, if its generation is newer than `seen`.
    pub fn newer_than(&self, seen: u64) -> Option<(u64, Option<Arc<T>>)> {
        let slot = lock(&self.slot);
        (slot.0 > seen).then(|| (slot.0, slot.1.clone()))
    }
}

/// Where a pass stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Running,
    Ok,
    Error,
    /// Stopped on purpose (account check).
    Stopped,
}

/// What the settings page and the sync buttons show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub phase: Phase,
    /// Error or stop code, otherwise empty (`SCOPE_MISSING` with `Ok` when
    /// a new login would grant a missing right).
    pub code: String,
    /// Seconds since 1970 of the last pass that went through.
    pub last_sync: Option<f64>,
    pub counts: Map,
    pub installation_id: Option<String>,
    pub capabilities: Vec<String>,
    pub min_version: Option<String>,
    /// Switched-on areas whose rights are missing.
    pub missing: Vec<String>,
    /// Result of the account check (`match`, `mismatch`, `confirmed` …).
    pub account: Option<String>,
}

/// What a pass hands an area.
pub struct Pass<'a> {
    pub conn: &'a Connection,
    pub paths: &'a Paths,
    /// The sync state of this installation; saved after each area.
    pub state: &'a mut Map,
    pub capabilities: &'a [String],
    pub now: f64,
    /// Routes whose expired hold a sync button released for this pass.
    pub release_now: &'a HashSet<String>,
    pub confirmations: &'a Confirmations,
    pub locations: &'a mut Locations,
    /// Who the own game account is (blueprints of other accounts do not go out).
    pub own_account: &'a OwnAccount,
    applying: &'a AtomicBool,
}

impl Pass<'_> {
    /// Whether the Basetool granted all of `scopes`.
    pub fn granted(&self, scopes: &[&str]) -> bool {
        all_in(scopes, self.capabilities)
    }

    /// Writes into the own stores; this does not count as an own change
    /// that would ask for another sync.
    pub fn apply_local<R>(&self, work: impl FnOnce() -> R) -> R {
        raised(self.applying, work)
    }
}

/// Runs `work` with `flag` set; the flag is put back also when `work` panics.
fn raised<R>(flag: &AtomicBool, work: impl FnOnce() -> R) -> R {
    struct Reset<'f>(&'f AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    flag.store(true, Ordering::SeqCst);
    let _reset = Reset(flag);
    work()
}

/// One part of the sync (unit storage, unit list, later blueprints, stock,
/// hangar, demand).
pub trait Area: Send {
    /// A name for the counts and the error log.
    fn name(&self) -> &'static str;
    /// Whether it runs in this pass; `ready` are the switched-on areas with
    /// all rights granted.
    fn runs(&self, ready: &[&'static str], capabilities: &[String]) -> bool;
    /// An error here does not stop the other areas; the counts carry it.
    fn isolated(&self) -> bool {
        false
    }
    /// Whether an isolated area throws its data away after an error with
    /// this action. Default: unless it is worth trying again later.
    fn forget_after(&self, action: Action) -> bool {
        action != Action::Retry
    }
    /// One pass; returns the counts.
    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError>;
    /// Throw everything away: switched off, right missing, disconnected.
    fn forget(&mut self);
}

/// Who the own game account is: `None` unknown, `*` all accounts.
pub type OwnAccount = Arc<dyn Fn() -> Option<String> + Send + Sync>;
/// Called when there is something new to draw.
pub type Waker = Arc<dyn Fn() + Send + Sync>;
/// A change of the state from a page (a decision), applicable repeatedly.
pub type Change = Arc<dyn Fn(&mut Map) + Send + Sync>;

struct Engine {
    conn: Arc<Connection>,
    paths: Arc<Paths>,
    clock: Clock,
    version: String,
    own_account: OwnAccount,
    waker: Waker,
    schedule: Mutex<Schedule>,
    wake: Condvar,
    status: Shared<Status>,
    areas: Mutex<Vec<Box<dyn Area>>>,
    /// The names of the areas, readable while a pass holds them.
    area_names: Vec<&'static str>,
    /// Whether a token was stored, and when that was looked at.
    linked: Mutex<Option<(std::time::Instant, bool)>>,
    inbox: Mutex<Vec<Change>>,
    release_requested: Mutex<HashSet<String>>,
    confirmations: Confirmations,
    locations: Mutex<Locations>,
    applying: AtomicBool,
    stop: AtomicBool,
    guard: writeguard::State,
    /// Held during a pass: never two at once.
    passing: Mutex<()>,
    /// Time and size of `bestand.json` at the last look; `None` before the first.
    collection_seen: Mutex<Option<Option<FileStamp>>>,
}

/// When a file was written and how long it is.
type FileStamp = (std::time::SystemTime, u64);

/// How often the worker looks whether another process (the overlay with a find) wrote the collection.
pub const COLLECTION_EVERY: f64 = 5.0;

impl Engine {
    fn now(&self) -> f64 {
        (self.clock)()
    }

    fn collection_stamp(&self) -> Option<FileStamp> {
        let meta = std::fs::metadata(self.paths.app_file(crate::collection::FILE_NAME)).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    /// The overlay writes a find into the collection in a process of its own: with the blueprints switched on a
    /// collection that changed since the last look is an own change, synced after [`AFTER_CHANGE`] seconds.
    ///
    /// [`AFTER_CHANGE`]: crate::basetool::schedule::AFTER_CHANGE
    fn look_at_collection(&self) {
        let now = self.collection_stamp();
        let before = lock(&self.collection_seen).replace(now);
        let switches: &dyn Switches = self.paths.as_ref();
        if before.is_some_and(|before| before != now) && switches.on(scopes::SETTING_BLUEPRINTS) {
            self.note_local_change();
        }
    }

    /// A pass wrote the collection itself: that is no own change.
    fn collection_taken(&self) {
        let now = self.collection_stamp();
        *lock(&self.collection_seen) = Some(now);
    }

    fn set_status(&self, change: impl FnOnce(&mut Status)) {
        *lock(&self.linked) = None;
        let mut status = self.status.get().1.map(|s| (*s).clone()).unwrap_or_default();
        change(&mut status);
        self.status.publish(status);
        (self.waker)();
    }

    /// The own data changed: sync soon, collected; not while the sync itself writes.
    fn note_local_change(&self) {
        if scopes::enabled(self.paths.as_ref()) && !self.applying.load(Ordering::SeqCst) {
            let now = self.now();
            lock(&self.schedule).local_changed(now);
            self.wake.notify_all();
        }
    }

    fn label(&self) -> String {
        // The connection name is private for the streamer mode.
        crate::streamer::private(
            self.paths
                .setting(SETTING_LABEL)
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| crate::testmode::basetool_default_label(&self.paths).to_string()),
        )
    }

    fn save(&self, state: &mut Map) -> Result<(), ApiError> {
        for change in lock(&self.inbox).iter() {
            change(state);
        }
        state::save_state(&self.paths, state).map_err(|e| ApiError::internal("INTERNAL", &e.to_string()))
    }

    /// Account check before the first sync of an account: `None` goes on,
    /// otherwise the reason to stop.
    fn account_stop(&self, state: &mut Map) -> Result<Option<String>, ApiError> {
        let handle = (self.own_account)().filter(|h| !h.is_empty());
        let Some(handle) = handle else { return Ok(Some("account_none".into())) };
        if handle == ALL_ACCOUNTS {
            return Ok(Some("account_all".into()));
        }
        let lowered = handle.to_lowercase();
        let mut accounts = get(state, "accounts").and_then(Value::as_object).cloned().unwrap_or_default();
        let result = match get_str(&accounts, &lowered) {
            Some(known) => known.to_string(),
            None => {
                let answer = self.conn.account_check(&handle)?;
                map_set(&mut accounts, &lowered, Value::str(&answer));
                map_set(state, "accounts", Value::Object(accounts));
                answer
            }
        };
        self.set_status(|s| s.account = Some(result.clone()));
        Ok(if matches!(result.as_str(), "match" | "confirmed") { None } else { Some(format!("account_{result}")) })
    }

    /// One whole pass.
    fn pass(&self) -> Result<(), ApiError> {
        // The test mode: no pass at all, so no area plans or writes anything.
        if self.conn.test_locked() {
            return Err(ApiError::new(crate::testmode::BASETOOL_LOCKED, 0));
        }
        self.set_status(|s| {
            s.phase = Phase::Running;
            s.code.clear();
        });
        let doc = self.conn.service_document()?;
        let installation_id = crate::text::py_strip(get_str(&doc, "installationId").unwrap_or_default()).to_string();
        if installation_id.is_empty() {
            // Without an id no own state file, and a shared fallback would
            // mix states of different installations.
            return Err(ApiError::new("INSTALLATION_ID_MISSING", 200));
        }
        let mut capabilities: Vec<String> = match get(&doc, "capabilities") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => Vec::new(),
        };
        capabilities.sort();
        capabilities.dedup();
        let minimum = get_str(&doc, "minClientVersion").filter(|m| !m.is_empty()).map(str::to_string);
        self.set_status(|s| {
            s.installation_id = Some(installation_id.clone());
            s.capabilities = capabilities.clone();
            s.min_version = minimum.clone();
        });
        if let Some(minimum) = &minimum
            && !self.version.is_empty()
            && crate::version::is_newer(minimum, &self.version)
        {
            return Err(ApiError::new("CLIENT_VERSION_UNSUPPORTED", 403));
        }
        let switches: &dyn Switches = self.paths.as_ref();
        let on: Vec<(&'static str, &'static [&'static str])> =
            AREAS.iter().copied().filter(|(setting, _)| switches.on(setting)).collect();
        let ready: Vec<&'static str> =
            on.iter().filter(|(_, scopes)| all_in(scopes, &capabilities)).map(|(setting, _)| *setting).collect();
        let missing: Vec<&'static str> =
            on.iter().filter(|(_, scopes)| !all_in(scopes, &capabilities)).map(|(setting, _)| *setting).collect();
        if ready.is_empty() {
            return Err(ApiError::new("SCOPE_MISSING", 403));
        }

        // What was decided until here is already in the file.
        lock(&self.inbox).clear();
        let release_now: HashSet<String> = std::mem::take(&mut *lock(&self.release_requested));
        let mut state = state::load_state(&self.paths, &installation_id);
        let label = self.label();
        if get_str(&state, "label_sent") != Some(label.as_str()) {
            self.conn.label_installation(&label)?;
            map_set(&mut state, "label_sent", Value::str(&label));
            self.save(&mut state)?;
        }
        let stop = self.account_stop(&mut state)?;
        self.save(&mut state)?;
        if let Some(code) = stop {
            self.set_status(|s| {
                s.phase = Phase::Stopped;
                s.code = code;
            });
            return Ok(());
        }

        let mut counts = Map::new();
        let mut locations = lock(&self.locations).clone();
        {
            let mut areas = lock(&self.areas);
            for area in areas.iter_mut() {
                if !area.runs(&ready, &capabilities) {
                    area.forget();
                    continue;
                }
                let mut pass = Pass {
                    conn: &self.conn,
                    paths: &self.paths,
                    state: &mut state,
                    capabilities: &capabilities,
                    now: self.now(),
                    release_now: &release_now,
                    confirmations: &self.confirmations,
                    locations: &mut locations,
                    own_account: &self.own_account,
                    applying: &self.applying,
                };
                match area.run(&mut pass) {
                    Ok(result) => map_set(&mut counts, area.name(), Value::Object(result)),
                    Err(error) if area.isolated() => {
                        if area.forget_after(error.action()) {
                            area.forget();
                        }
                        error.record(&format!("basetool_sync.{}", area.name()));
                        let failed = vec![("error".into(), Value::str(&error.code))];
                        map_set(&mut counts, area.name(), Value::Object(failed));
                    }
                    Err(error) => return Err(error),
                }
                self.save(&mut state)?;
                (self.waker)();
            }
        }
        *lock(&self.locations) = locations;

        // A reminder only where a new login helps: rights the Basetool does
        // not offer yet cannot be granted.
        let askable = on.iter().any(|(setting, scopes)| {
            missing.contains(setting) && {
                let wanted: Vec<String> = scopes.iter().map(|s| (*s).to_string()).collect();
                let not_offered = self.conn.not_offered(&wanted);
                wanted.iter().any(|s| !capabilities.contains(s) && !not_offered.contains(s))
            }
        });
        let now = self.now();
        self.set_status(|s| {
            s.phase = Phase::Ok;
            s.code = if askable { "SCOPE_MISSING".into() } else { String::new() };
            s.counts = counts;
            s.missing = missing.iter().map(|m| (*m).to_string()).collect();
            s.last_sync = Some(now);
        });
        Ok(())
    }

    /// Waits until the schedule says so, a button wakes it or it is told
    /// to stop. Due and stop are looked at under the lock, so no wake-up
    /// gets lost.
    fn pause(&self, enabled: bool) {
        let schedule = lock(&self.schedule);
        let now = self.now();
        if self.stop.load(Ordering::SeqCst) || (enabled && schedule.due(now)) {
            return;
        }
        let wait = if enabled { schedule.wait(now, COLLECTION_EVERY) } else { 60.0 };
        let _ = self.wake.wait_timeout(schedule, Duration::from_secs_f64(wait));
    }

    /// The worker: waits until a pass is due and runs it.
    fn work(self: Arc<Self>) {
        writeguard::adopt(&self.guard);
        while !self.stop.load(Ordering::SeqCst) {
            // Settings and secret store are read outside the lock: a slow
            // keyring must not block the buttons.
            let enabled = scopes::enabled(self.paths.as_ref());
            if enabled {
                self.look_at_collection();
            }
            if !enabled || !lock(&self.schedule).due(self.now()) {
                self.pause(enabled);
                continue;
            }
            if !self.conn.connected() {
                // Still due after an old own change: wait the short pause
                // anyway, or the check would run without end.
                let mut schedule = lock(&self.schedule);
                schedule.not_connected(self.now());
                if !self.stop.load(Ordering::SeqCst) {
                    let _ = self.wake.wait_timeout(schedule, Duration::from_secs_f64(MIN_WAIT));
                }
                continue;
            }
            {
                let mut schedule = lock(&self.schedule);
                if !schedule.due(self.now()) {
                    continue;
                }
                schedule.start();
            }
            let result = self.guarded_pass();
            self.collection_taken();
            let now = self.now();
            let pending = !lock(&self.inbox).is_empty();
            match result {
                Ok(()) => lock(&self.schedule).succeeded(now, pending),
                Err(error) => lock(&self.schedule).failed(now, error.action(), error.retry_after, jitter()),
            }
            (self.waker)();
        }
    }

    /// A pass, never two at once; a failure goes into the error log and the
    /// status.
    fn guarded_pass(&self) -> Result<(), ApiError> {
        let _passing = lock(&self.passing);
        // A failure inside a pass must not end the worker for good.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.pass()))
            .unwrap_or_else(|_| Err(ApiError::internal("INTERNAL", "panic in the sync pass")));
        if let Err(error) = &result {
            // Into the error log: the page shows only the code, and nobody
            // could tell which request it was. The lock of the test mode is
            // no error.
            error.record("basetool.sync");
            self.set_status(|s| {
                s.phase = Phase::Error;
                s.code = error.code.clone();
            });
        }
        result
    }
}

/// A job of a page (a booking, a member list) in its own thread.
pub struct Job<R> {
    answer: Receiver<R>,
    done: Option<R>,
    /// The thread ended (or never started) without an answer.
    lost: bool,
}

impl<R> Job<R> {
    /// The result, once it is there; it is handed out once.
    pub fn poll(&mut self) -> Option<R> {
        if self.done.is_none() {
            match self.answer.try_recv() {
                Ok(result) => self.done = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.lost = true,
            }
        }
        self.done.take()
    }

    /// Whether the job can never answer: its thread broke off or did not
    /// start. A page that waits for it lets go then.
    pub fn lost(&self) -> bool {
        self.lost
    }

    /// Waits for the result (for tests and tools, never in the window).
    pub fn wait(self, limit: Duration) -> Option<R> {
        self.answer.recv_timeout(limit).ok()
    }
}

/// Where a device login stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Login {
    Starting,
    /// Show the code and the address; the player confirms in the browser.
    Code {
        user_code: String,
        verification_uri: String,
    },
    Connected,
    Denied,
    Expired,
    /// An error code (`error_text`).
    Failed(String),
    Cancelled,
}

/// The handle the program and the pages hold.
#[derive(Clone)]
pub struct SyncHandle {
    engine: Arc<Engine>,
    login: Shared<Login>,
    login_round: Arc<Mutex<u64>>,
    worker: Arc<Mutex<Option<JoinHandle<()>>>>,
}

/// What the sync needs from the program.
pub struct Setup {
    pub conn: Arc<Connection>,
    pub paths: Arc<Paths>,
    pub clock: Clock,
    /// The own version, compared with the Basetool's minimum.
    pub version: String,
    pub own_account: OwnAccount,
    pub waker: Waker,
    pub areas: Vec<Box<dyn Area>>,
}

impl SyncHandle {
    /// Starts the worker thread.
    pub fn start(setup: Setup) -> SyncHandle {
        let handle = SyncHandle::without_worker(setup);
        handle.watch_saves();
        let worker = {
            let engine = handle.engine.clone();
            std::thread::Builder::new().name("basetool-sync".into()).spawn(move || engine.work()).ok()
        };
        *lock(&handle.worker) = worker;
        handle
    }

    /// From now on a write of the own stock files, the hangar or `asop.json` in the data folder
    /// of this sync starts its wait for a pass.
    fn watch_saves(&self) {
        lock(&SAVED).push((data_folder(&self.engine.paths), Arc::downgrade(&self.engine)));
    }

    /// Without a worker: passes only through `pass_now` (tools, tests).
    pub fn without_worker(setup: Setup) -> SyncHandle {
        let area_names = setup.areas.iter().map(|area| area.name()).collect();
        let engine = Arc::new(Engine {
            area_names,
            linked: Mutex::new(None),
            conn: setup.conn,
            paths: setup.paths,
            clock: setup.clock,
            version: setup.version,
            own_account: setup.own_account,
            waker: setup.waker,
            schedule: Mutex::new(Schedule::default()),
            wake: Condvar::new(),
            status: Shared::default(),
            areas: Mutex::new(setup.areas),
            inbox: Mutex::new(Vec::new()),
            release_requested: Mutex::new(HashSet::new()),
            confirmations: Confirmations::default(),
            locations: Mutex::new(Locations::default()),
            applying: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            guard: writeguard::state(),
            passing: Mutex::new(()),
            collection_seen: Mutex::new(None),
        });
        SyncHandle {
            engine,
            login: Shared::default(),
            login_round: Arc::new(Mutex::new(0)),
            worker: Arc::new(Mutex::new(None)),
        }
    }

    /// One pass right here, waiting for a running one.
    pub fn pass_now(&self) -> Result<(), ApiError> {
        self.engine.guarded_pass()
    }

    /// Ends the worker after the running pass.
    pub fn shutdown(&self) {
        self.stop();
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }

    /// Tells the worker to end after the running pass, without waiting for
    /// it (closing a window must not wait for the network).
    pub fn stop(&self) {
        let _schedule = lock(&self.engine.schedule);
        self.engine.stop.store(true, Ordering::SeqCst);
        self.engine.wake.notify_all();
    }

    pub fn connection(&self) -> &Arc<Connection> {
        &self.engine.conn
    }

    pub fn status(&self) -> Shared<Status> {
        self.engine.status.clone()
    }

    pub fn login_state(&self) -> Shared<Login> {
        self.login.clone()
    }

    /// Sync now, never in parallel; a running pass is followed directly by
    /// the next one.
    pub fn request_now(&self) {
        let mut schedule = lock(&self.engine.schedule);
        schedule.request_now();
        self.engine.wake.notify_all();
    }

    /// A sync button: sync now, and an expired hold on `routes` (all when
    /// empty) may send once in the next pass. A running hold stays.
    pub fn release_held(&self, routes: &[&str]) {
        {
            let mut requested = lock(&self.engine.release_requested);
            if routes.is_empty() {
                requested.extend(crate::basetool::scopes::SEND_ROUTES.iter().map(|r| (*r).to_string()));
            } else {
                requested.extend(routes.iter().map(|r| (*r).to_string()));
            }
        }
        self.request_now();
    }

    /// The own data changed: sync soon, collected. Writes of the sync
    /// itself do not count.
    pub fn local_changed(&self) {
        self.engine.note_local_change();
    }

    /// Looks whether the collection was written since the last look, as the worker does every
    /// [`COLLECTION_EVERY`] seconds (for a handle without a worker).
    pub fn look_at_collection(&self) {
        self.engine.look_at_collection();
    }

    /// Whether an own change waits for its pass.
    pub fn change_waits(&self) -> bool {
        lock(&self.engine.schedule).changed_at.is_some()
    }

    /// The confirmation page of a held change set while it is valid.
    pub fn confirm_link(&self, route: &str) -> Option<String> {
        self.engine.confirmations.link(route, self.engine.now())
    }

    /// Holds a change set of a page (a booking) the way the sync holds its
    /// own, so the button and the page show the same.
    pub fn note_confirmation(&self, route: &str, url: Option<String>) {
        let now = self.engine.now();
        self.engine.confirmations.set(route, url, now + hold::CONFIRM_LIFETIME);
        (self.engine.waker)();
    }

    /// A decision of a page about the state: saved at once, kept for a
    /// running pass (it saves its own copy at the end) and followed by a
    /// sync. False while there is no state yet.
    pub fn decide(&self, change: Change) -> bool {
        let Some(id) = self.engine.status.get().1.and_then(|s| s.installation_id.clone()) else { return false };
        // A state that is there but cannot be read now (locked for a moment)
        // is not replaced by an empty one.
        let Some(mut current) = state::try_load(&self.engine.paths, &id) else { return false };
        change(&mut current);
        if state::save_state(&self.engine.paths, &current).is_err() {
            return false;
        }
        lock(&self.engine.inbox).push(change);
        self.request_now();
        true
    }

    /// The player confirms on the page: this is my account.
    pub fn confirm_account(&self) -> bool {
        let handle = (self.engine.own_account)().filter(|h| !h.is_empty() && h != ALL_ACCOUNTS);
        let Some(handle) = handle else { return false };
        let lowered = handle.to_lowercase();
        self.decide(Arc::new(move |state: &mut Map| {
            let mut accounts = get(state, "accounts").and_then(Value::as_object).cloned().unwrap_or_default();
            map_set(&mut accounts, &lowered, Value::str("confirmed"));
            map_set(state, "accounts", Value::Object(accounts));
        }))
    }

    /// The settings the areas and the pages read their switches from.
    pub fn paths(&self) -> &Arc<Paths> {
        &self.engine.paths
    }

    /// Writes a setting under the write lock the sync was made with, so the
    /// switches of the page reach the file the sync reads whichever thread
    /// the window runs in.
    pub fn set_setting(&self, name: &str, value: Value) -> bool {
        /// Gives the caller's lock back, also when the write panics.
        struct Restore(writeguard::State);
        impl Drop for Restore {
            fn drop(&mut self) {
                writeguard::adopt(&self.0);
            }
        }
        let _restore = Restore(writeguard::state());
        writeguard::adopt(&self.engine.guard);
        self.engine.paths.set_setting(name, value)
    }

    /// Whether a token is stored, asked of the system's store at most once a
    /// second: the window asks every frame, and a keyring is a process or a
    /// decryption each time. A pass, a login, disconnecting and every change
    /// of the status ask again at once.
    pub fn linked(&self) -> bool {
        let mut seen = lock(&self.engine.linked);
        if let Some((at, linked)) = *seen
            && at.elapsed() < Duration::from_secs(1)
        {
            return linked;
        }
        let linked = self.engine.conn.connected();
        *seen = Some((std::time::Instant::now(), linked));
        linked
    }

    /// Whether the sync has an area of this name (`Area::name`); the
    /// switches of areas that are not built stay off.
    pub fn has_area(&self, name: &str) -> bool {
        self.engine.area_names.contains(&name)
    }

    /// The time of the sync (seconds since 1970).
    pub fn now(&self) -> f64 {
        self.engine.now()
    }

    /// Name of this installation in the Basetool.
    pub fn label(&self) -> String {
        self.engine.label()
    }

    /// The sync state of the installation the last pass synced with; `None`
    /// before the first pass.
    pub fn state(&self) -> Option<Map> {
        let id = self.engine.status.get().1.and_then(|s| s.installation_id.clone())?;
        Some(state::load_state(&self.engine.paths, &id))
    }

    /// The rights the Basetool granted: those of the last token and those the
    /// last pass saw.
    pub fn granted(&self) -> Vec<String> {
        let mut all = self.engine.conn.granted();
        if let Some(status) = self.engine.status.get().1 {
            all.extend(status.capabilities.iter().cloned());
        }
        all.sort();
        all.dedup();
        all
    }

    /// Whether the page of an area is there: its switch on, its rights
    /// granted and the connection standing.
    pub fn area_visible(&self, setting: &str) -> bool {
        let Some((_, scopes)) = AREAS.iter().find(|(s, _)| *s == setting) else { return false };
        self.engine.paths.setting_bool(setting, false) && all_in(scopes, &self.granted()) && self.linked()
    }

    /// An area was switched on or off: what the areas that no longer run hold
    /// is thrown away at once, and a pass follows. In a thread of its own,
    /// because a running pass holds the areas.
    pub fn switched(&self) {
        let handle = self.clone();
        let spawned = std::thread::Builder::new().name("basetool-switch".into()).spawn(move || {
            let engine = &handle.engine;
            writeguard::adopt(&engine.guard);
            let capabilities = engine.status.get().1.map(|s| s.capabilities.clone()).unwrap_or_default();
            let switches: &dyn Switches = engine.paths.as_ref();
            let ready: Vec<&'static str> = AREAS
                .iter()
                .filter(|(setting, scopes)| switches.on(setting) && all_in(scopes, &capabilities))
                .map(|(setting, _)| *setting)
                .collect();
            for area in lock(&engine.areas).iter_mut() {
                if !area.runs(&ready, &capabilities) {
                    area.forget();
                }
            }
            (engine.waker)();
            handle.request_now();
        });
        if spawned.is_err() {
            crate::errors::record("basetool_sync.switched", &"no thread");
            self.request_now();
        }
    }

    /// A job with the connection in its own thread; the window asks with
    /// `Job::poll` and is woken when it is done.
    pub fn job<R: Send + 'static>(&self, work: impl FnOnce(&Connection) -> R + Send + 'static) -> Job<R> {
        let (send, answer) = std::sync::mpsc::channel();
        let engine = self.engine.clone();
        let spawned = std::thread::Builder::new().name("basetool-job".into()).spawn(move || {
            writeguard::adopt(&engine.guard);
            let _ = send.send(work(&engine.conn));
            (engine.waker)();
        });
        if spawned.is_err() {
            crate::errors::record("basetool_sync.job", &"no thread");
        }
        Job { answer, done: None, lost: false }
    }

    /// Device login in the background with the rights of the switched-on
    /// areas; the code shows up in `login_state`.
    pub fn connect(&self) {
        let mut wanted = vec![scopes::SCOPE_CONNECT.to_string()];
        wanted.extend(scopes::wanted_scopes(self.engine.paths.as_ref()));
        let round = {
            let mut round = lock(&self.login_round);
            *round += 1;
            *round
        };
        self.login.publish(Login::Starting);
        (self.engine.waker)();
        let handle = self.clone();
        let spawned = std::thread::Builder::new().name("basetool-login".into()).spawn(move || {
            writeguard::adopt(&handle.engine.guard);
            // A failure inside must not leave "getting a code" on the page for good.
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.run_login(round, &wanted)));
            if ran.is_err() {
                crate::errors::record("basetool_page.login", &"panic in the login");
                if handle.current_round(round) {
                    handle.login.publish(Login::Failed("INTERNAL".into()));
                    (handle.engine.waker)();
                }
            }
        });
        if spawned.is_err() {
            self.login.publish(Login::Failed("INTERNAL".into()));
        }
    }

    /// Stops a running device login.
    pub fn cancel_login(&self) {
        *lock(&self.login_round) += 1;
        self.login.publish(Login::Cancelled);
        (self.engine.waker)();
    }

    fn current_round(&self, round: u64) -> bool {
        *lock(&self.login_round) == round
    }

    fn run_login(&self, round: u64, wanted: &[String]) {
        let conn = &self.engine.conn;
        let finish = |state: Login| {
            *lock(&self.engine.linked) = None;
            if self.current_round(round) {
                self.login.publish(state);
                (self.engine.waker)();
            }
        };
        let mut device: DeviceLogin = match conn.start_login(wanted) {
            Ok(device) => device,
            Err(error) => {
                error.record("basetool_page.login");
                return finish(Login::Failed(error.code));
            }
        };
        finish(Login::Code { user_code: device.user_code.clone(), verification_uri: device.verification_uri.clone() });
        loop {
            // Waits in small steps so a cancel ends it quickly.
            let mut waited = 0.0;
            while waited < device.interval as f64 {
                if !self.current_round(round) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
                waited += 0.1;
            }
            if !self.current_round(round) {
                return;
            }
            match conn.poll_login(&mut device) {
                Ok(Poll::Pending | Poll::SlowDown) => continue,
                Ok(Poll::Ok) => {
                    finish(Login::Connected);
                    self.request_now();
                }
                Ok(Poll::Denied) => finish(Login::Denied),
                Ok(Poll::Expired) => finish(Login::Expired),
                Err(error) => {
                    error.record("basetool_page.poll");
                    finish(Login::Failed(error.code));
                }
            }
            return;
        }
    }

    /// "Disconnect" in the background: revoke and delete token and key,
    /// throw away every area's data and the state of this installation.
    pub fn disconnect(&self) -> Job<()> {
        let engine = self.engine.clone();
        let login = self.login.clone();
        self.job(move |conn| {
            conn.disconnect();
            // A running pass ends first (its requests now fail); otherwise it
            // would write the state file again after it was deleted.
            let _passing = lock(&engine.passing);
            for area in lock(&engine.areas).iter_mut() {
                area.forget();
            }
            let id = engine.status.get().1.and_then(|s| s.installation_id.clone());
            if let Some(id) = id {
                state::forget_installation(&engine.paths, &id);
            }
            engine.confirmations.clear_all();
            *lock(&engine.linked) = None;
            engine.status.publish(Status::default());
            login.clear();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_of_an_own_write_is_up_only_while_it_runs_and_comes_down_after_a_panic() {
        let flag = AtomicBool::new(false);
        assert!(raised(&flag, || flag.load(Ordering::SeqCst)), "up while the work runs");
        assert!(!flag.load(Ordering::SeqCst), "down after it");
        let panicked =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| raised(&flag, || panic!("write failed"))));
        assert!(panicked.is_err());
        assert!(!flag.load(Ordering::SeqCst), "down after a panic as well, or no own change would ever count again");
    }

    fn watching(name: &str) -> (SyncHandle, Arc<Paths>, String) {
        let dir = std::env::temp_dir().join(format!("versekit-saved-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        let data = dir.to_string_lossy().replace('\\', "/");
        let host = crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, &data)],
            &data,
        );
        let paths = Arc::new(Paths::new(host));
        let clock = crate::basetool::connection::system_clock();
        let double = Arc::new(crate::basetool::exchange_double::ExchangeDouble::new());
        let handle = SyncHandle::without_worker(Setup {
            conn: Arc::new(double.connection(clock.clone())),
            paths: paths.clone(),
            clock,
            version: "5.0.0".into(),
            own_account: Arc::new(|| None),
            waker: Arc::new(|| {}),
            areas: Vec::new(),
        });
        handle.watch_saves();
        (handle, paths, data)
    }

    fn waiting(handle: &SyncHandle) -> bool {
        lock(&handle.engine.schedule).changed_at.is_some()
    }

    fn calm(handle: &SyncHandle) {
        lock(&handle.engine.schedule).changed_at = None;
    }

    #[test]
    fn a_write_of_the_own_stock_hangar_or_names_starts_the_wait_for_a_pass() {
        let (handle, paths, data) = watching("saved");
        let raw = paths.app_file(crate::stock::RAW_FILE);
        // Nothing switched on: nothing to sync.
        assert!(crate::stock::save(&raw, &[], "test"));
        assert!(!waiting(&handle), "no area on");
        // Only the blueprints on: the stock and the hangar do not count.
        assert!(paths.set_setting(scopes::SETTING_BLUEPRINTS, Value::Bool(true)));
        assert!(crate::stock::save(&raw, &[], "test"));
        assert!(!waiting(&handle), "the blueprints do not wait for the stock");
        assert!(paths.set_setting(scopes::SETTING_STOCK, Value::Bool(true)));
        assert!(crate::stock::save(&raw, &[], "test"));
        assert!(waiting(&handle), "the stock");
        calm(&handle);
        assert!(crate::asop::save(&paths.app_file(crate::asop::FILE), &crate::asop::empty()));
        assert!(waiting(&handle), "the own names go with the hangar");
        calm(&handle);
        assert!(crate::hangar::write(&paths.app_file(crate::hangar::FILE), &crate::hangar::empty()));
        assert!(waiting(&handle), "the hangar");
        calm(&handle);
        // The sync's own writing does not count.
        handle.engine.applying.store(true, Ordering::SeqCst);
        assert!(crate::stock::save(&raw, &[], "test"));
        handle.engine.applying.store(false, Ordering::SeqCst);
        assert!(!waiting(&handle), "a take-over is no own change");
        // A file of another folder is not of this sync.
        let other = format!("{data}/anderswo");
        std::fs::create_dir_all(&other).expect("folder");
        assert!(crate::stock::save(&format!("{other}/rohstoffe.json"), &[], "test"));
        assert!(!waiting(&handle), "another folder");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn shared_counts_generations() {
        let shared: Shared<Vec<u8>> = Shared::default();
        assert_eq!(shared.get().0, 0);
        assert_eq!(shared.publish(vec![1]), 1);
        let page_seen = 1;
        assert!(shared.newer_than(page_seen).is_none());
        shared.publish(vec![2]);
        let (generation, value) = shared.newer_than(page_seen).unwrap();
        assert_eq!((generation, value.as_deref()), (2, Some(&vec![2])));
        shared.clear();
        assert_eq!(shared.get(), (3, None));
    }
}
