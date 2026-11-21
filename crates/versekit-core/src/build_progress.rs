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

//! How far the build of the blueprint catalog from the game data has come.
//!
//! The build is a fixed row of [`Step`]s. The steps that scan the records of the archive know their total (the
//! archive library counts the records it checked); the others are short and count only when they are over. The
//! overall fraction is the weighted share of the steps done plus the known part of the running step, and it never
//! falls back.
//!
//! The build thread owns a [`Run`]; the window of the same process asks [`snapshot`]. A build in another process
//! (the overlay keeps the catalog current in the background) is seen through a small file in the data folder
//! ([`FILE`]) that the run keeps up while it lasts and removes at its end.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::jsonio::{self, Value};
use crate::paths::Paths;

/// The file of the run in the data folder, for the window of another process.
pub const FILE: &str = "katalog-neubau-stand.json";

/// A file not kept up for this long belongs to a process that ended without removing it.
pub const STALE_SECONDS: u64 = 180;

/// The least time between two writes of the file while only the fraction moves.
const WRITE_EVERY_SECONDS: f64 = 1.0;

/// One step of the build, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// The archive is opened (waits while the game runs or the launcher patches).
    Archive,
    /// The known data errors are asked for.
    Corrections,
    /// The texts of the game are read.
    Texts,
    /// The contracts are read from the records of the archive.
    Contracts,
    /// The places the contracts lead to are read from the records.
    Places,
    /// The blueprints and their values are read from the DataForge.
    Blueprints,
    /// The reputation the contracts give is read from the records.
    Reputation,
    /// scmdb's catalog of the same build is fetched.
    Scmdb,
    /// The two catalogs are compared and the result is kept.
    Save,
}

impl Step {
    /// All steps in the order of the build.
    pub const ALL: [Step; 9] = [
        Step::Archive,
        Step::Corrections,
        Step::Texts,
        Step::Contracts,
        Step::Places,
        Step::Blueprints,
        Step::Reputation,
        Step::Scmdb,
        Step::Save,
    ];

    /// The place in the row, from 1.
    pub fn number(self) -> usize {
        Step::ALL.iter().position(|step| *step == self).map_or(1, |at| at + 1)
    }

    /// How many steps there are.
    pub fn count() -> usize {
        Step::ALL.len()
    }

    /// The key of the text that names the step.
    pub fn key(self) -> &'static str {
        match self {
            Step::Archive => "kat_schritt_archiv",
            Step::Corrections => "kat_schritt_korrekturen",
            Step::Texts => "kat_schritt_texte",
            Step::Contracts => "kat_schritt_vertraege",
            Step::Places => "kat_schritt_orte",
            Step::Blueprints => "kat_schritt_bauplaene",
            Step::Reputation => "kat_schritt_ruf",
            Step::Scmdb => "kat_schritt_scmdb",
            Step::Save => "kat_schritt_speichern",
        }
    }

    /// Its share of the whole build, by the time it takes; the shares add up to one.
    fn weight(self) -> f64 {
        match self {
            Step::Archive => 0.05,
            Step::Corrections => 0.02,
            Step::Texts => 0.03,
            Step::Contracts => 0.09,
            Step::Places => 0.09,
            Step::Blueprints => 0.42,
            Step::Reputation => 0.24,
            Step::Scmdb => 0.05,
            Step::Save => 0.01,
        }
    }

    /// How many readings of the records the step makes (each counts from nothing to its total), 0 for a step that
    /// does not count.
    fn readings(self) -> usize {
        match self {
            Step::Contracts | Step::Places => 1,
            // The contracts of the generators, the blueprints, the items with values.
            Step::Blueprints => 3,
            Step::Reputation => 2,
            _ => 0,
        }
    }

    /// The share of the build done before the step begins.
    fn before(self) -> f64 {
        Step::ALL.iter().take_while(|step| **step != self).map(|step| step.weight()).sum()
    }

    fn from_number(number: usize) -> Option<Step> {
        number.checked_sub(1).and_then(|at| Step::ALL.get(at)).copied()
    }
}

/// How the archive library counts: the step it reports, from 0, and how far it came in it.
const PHASES: usize = 2;

/// What the window shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub step: Step,
    /// How far the running step came, when it knows its total.
    pub step_fraction: Option<f64>,
    /// How far the whole build came, 0 to 1, never 1 while it runs.
    pub fraction: f64,
    /// Seconds since the run began (and, after a wait, since the wait ended).
    pub elapsed: u64,
    /// The archive may not be read yet: nothing is worked on, and the fraction says nothing.
    pub waiting: bool,
}

impl Snapshot {
    /// The percent of the whole build, 0 to 99 while it runs.
    pub fn percent(&self) -> u32 {
        ((self.fraction * 100.0).floor() as u32).min(99)
    }
}

struct Inner {
    step: Step,
    /// The readings of the records the step made so far, and how far the running one came.
    readings_done: usize,
    reading: Option<f64>,
    best: f64,
    started: Instant,
    started_at: u64,
    file: Option<String>,
    last_write: Option<(Instant, f64, Step)>,
    ended: bool,
    /// The steps the run came to, in order.
    visited: Vec<Step>,
    /// The archive may not be read yet (the game runs, the launcher patches): nothing is being worked on.
    waiting: bool,
    /// Readings of the records ended, and counts reported, over the whole run.
    readings: usize,
    reports: usize,
}

impl Inner {
    fn step_fraction(&self) -> Option<f64> {
        let readings = self.step.readings();
        if readings == 0 {
            return None;
        }
        let running = self.reading.unwrap_or(0.0);
        Some(((self.readings_done as f64 + running) / readings as f64).clamp(0.0, 1.0))
    }

    /// The fraction of the whole build; held at the best it was, and below one.
    fn fraction(&mut self) -> f64 {
        let own = self.step.weight() * self.step_fraction().unwrap_or(0.0);
        let now = (self.step.before() + own).min(0.99);
        self.best = self.best.max(now);
        self.best
    }

    fn snapshot(&mut self) -> Snapshot {
        Snapshot {
            step: self.step,
            step_fraction: self.step_fraction(),
            fraction: self.fraction(),
            elapsed: self.started.elapsed().as_secs(),
            waiting: self.waiting,
        }
    }

    /// Keeps the file for the other processes up: at each new step and, at most once a second, with each move.
    fn write(&mut self, force: bool) {
        let Some(file) = self.file.clone() else { return };
        let fraction = self.fraction();
        let step = self.step;
        let due = force
            || self.last_write.is_none_or(|(at, before, was)| {
                was != step || (fraction > before && at.elapsed().as_secs_f64() >= WRITE_EVERY_SECONDS)
            });
        if !due {
            return;
        }
        self.last_write = Some((Instant::now(), fraction, step));
        let text = jsonio::dumps(
            &Value::Object(vec![
                ("pid".into(), Value::int(i64::from(std::process::id()))),
                ("begin".into(), Value::int(self.started_at as i64)),
                ("stand".into(), Value::int(unix_now() as i64)),
                ("schritt".into(), Value::int(step.number() as i64)),
                ("lesungen".into(), Value::int(self.readings_done as i64)),
                ("anteil".into(), Value::Float(self.reading.unwrap_or(0.0))),
                ("gesamt".into(), Value::Float(fraction)),
                ("wartet".into(), Value::Bool(self.waiting)),
            ]),
            None,
            false,
        );
        let _ = jsonio::write_text_atomic(&file, &text, jsonio::Newline::Keep);
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

type Shared = Arc<Mutex<Inner>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Inner> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// The run of the process that the window asks.
static LATEST: Mutex<Option<Shared>> = Mutex::new(None);

thread_local! {
    /// The run the build of this thread belongs to.
    static BOUND: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

/// How a run on this thread ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Ended {
    /// The steps it came to, in order.
    pub steps: Vec<Step>,
    /// The fraction it had reached.
    pub fraction: f64,
    /// How many readings of the records it made, and how many counts of checked records they reported.
    pub readings: usize,
    pub reports: usize,
    /// It was told to have worked ([`Run::succeeded`]); a run that is only dropped counts as failed.
    pub worked: bool,
}

thread_local! {
    /// How the last run of this thread ended.
    static ENDED: RefCell<Option<Ended>> = const { RefCell::new(None) };
}

/// How the last run of this thread ended; taken once.
pub fn take_ended() -> Option<Ended> {
    ENDED.with(|ended| ended.borrow_mut().take())
}

/// A running build. Dropping it ends the run: the file is removed and the window sees no build any more.
pub struct Run {
    shared: Shared,
    worked: bool,
    /// The run the thread had before, to put back.
    outer: Option<Shared>,
}

/// Begins a run on this thread. `file` is the path of [`FILE`] for the other processes, `None` for none.
pub fn begin(file: Option<String>) -> Run {
    let shared: Shared = Arc::new(Mutex::new(Inner {
        step: Step::Archive,
        readings_done: 0,
        reading: None,
        best: 0.0,
        started: Instant::now(),
        started_at: unix_now(),
        file,
        last_write: None,
        ended: false,
        visited: vec![Step::Archive],
        waiting: false,
        readings: 0,
        reports: 0,
    }));
    lock(&shared).write(true);
    let outer = BOUND.with(|bound| bound.borrow_mut().replace(Arc::clone(&shared)));
    *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&shared));
    Run { shared, worked: false, outer }
}

impl Run {
    /// The build gave its catalog: the run ends as one that worked.
    pub fn succeeded(&mut self) {
        self.worked = true;
    }

    /// What the run stands at.
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.shared).snapshot()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let file = {
            let mut inner = lock(&self.shared);
            inner.ended = true;
            let fraction = inner.fraction();
            let ended = Ended {
                steps: inner.visited.clone(),
                fraction,
                readings: inner.readings,
                reports: inner.reports,
                worked: self.worked,
            };
            ENDED.with(|slot| *slot.borrow_mut() = Some(ended));
            inner.file.take()
        };
        if let Some(file) = file {
            let _ = jsonio::remove_file(&file);
        }
        BOUND.with(|bound| *bound.borrow_mut() = self.outer.take());
        let mut latest = LATEST.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref().is_some_and(|shared| Arc::ptr_eq(shared, &self.shared)) {
            *latest = None;
        }
    }
}

/// The build of this thread is at `step` (a step that was left is done; steps are not gone back to).
pub fn step(step: Step) {
    BOUND.with(|bound| {
        if let Some(shared) = bound.borrow().as_ref() {
            let mut inner = lock(shared);
            if step > inner.step {
                inner.step = step;
                inner.visited.push(step);
                inner.readings_done = 0;
                inner.reading = None;
            }
            inner.write(false);
        }
    });
}

/// The build of this thread waits (or no longer waits) for the archive to be readable. The time of the run counts
/// from the end of the wait.
pub fn waiting(on: bool) {
    BOUND.with(|bound| {
        if let Some(shared) = bound.borrow().as_ref() {
            let mut inner = lock(shared);
            if inner.waiting != on {
                inner.waiting = on;
                if !on {
                    inner.started = Instant::now();
                    inner.started_at = unix_now();
                }
                inner.write(true);
            }
        }
    });
}

/// The way for the thread of the archive library to report: it may be called from any thread.
#[derive(Clone, Default)]
pub struct Reporter(Option<Shared>);

/// The reporter of the build on this thread; one that reports nowhere without a build.
pub fn reporter() -> Reporter {
    Reporter(BOUND.with(|bound| bound.borrow().clone()))
}

impl Reporter {
    /// A reading of the records begins in the running step: the one before it, if any, is done.
    pub fn reading_begins(&self) {
        let Some(shared) = &self.0 else { return };
        let mut inner = lock(shared);
        if inner.reading.is_some() {
            inner.readings_done += 1;
        }
        inner.reading = Some(0.0);
        inner.write(false);
    }

    /// The reading has checked `done` of `total` records in its phase `phase` (from 0, `PHASES` of them).
    /// Calls may arrive out of order: the reading only moves forward.
    pub fn records(&self, phase: usize, done: usize, total: usize) {
        let Some(shared) = &self.0 else { return };
        if total == 0 {
            return;
        }
        let within = (done.min(total) as f64 / total as f64).clamp(0.0, 1.0);
        let at = ((phase.min(PHASES - 1)) as f64 + within) / PHASES as f64;
        let mut inner = lock(shared);
        inner.reports += 1;
        if inner.reading.is_none_or(|before| at > before) {
            inner.reading = Some(at);
        }
        inner.write(false);
    }

    /// The reading has gone through `done` of `total` entries of its own list (a loop over a known number).
    pub fn counted(&self, done: usize, total: usize) {
        let Some(shared) = &self.0 else { return };
        if total == 0 {
            return;
        }
        let at = (done.min(total) as f64 / total as f64).clamp(0.0, 1.0);
        let mut inner = lock(shared);
        inner.reports += 1;
        if inner.reading.is_none_or(|before| at > before) {
            inner.reading = Some(at);
        }
        inner.write(false);
    }

    /// The reading ended.
    pub fn reading_ends(&self) {
        let Some(shared) = &self.0 else { return };
        let mut inner = lock(shared);
        if inner.reading.take().is_some() {
            inner.readings_done += 1;
            inner.readings += 1;
        }
        inner.write(false);
    }
}

/// A loop over a known number of entries that counts for the build of this thread: it begins a reading, each
/// [`Counter::at`] tells how far the loop came, and it ends the reading when it is dropped.
pub struct Counter {
    report: Reporter,
    total: usize,
    every: usize,
}

/// Begins a counted loop of `total` entries.
pub fn counter(total: usize) -> Counter {
    let report = reporter();
    report.reading_begins();
    Counter { report, total, every: (total / 200).max(1) }
}

impl Counter {
    /// The loop is about to do entry `done` (from 0); the build is told every few entries only.
    pub fn at(&self, done: usize) {
        if done.is_multiple_of(self.every) {
            self.report.counted(done, self.total);
        }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.report.reading_ends();
    }
}

/// The build of this process, when one runs.
pub fn snapshot() -> Option<Snapshot> {
    let shared = LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    let mut inner = lock(&shared);
    (!inner.ended).then(|| inner.snapshot())
}

/// The build of this process, else the one the file of another process tells (kept up within
/// [`STALE_SECONDS`]).
pub fn snapshot_in(paths: &Paths) -> Option<Snapshot> {
    snapshot().or_else(|| snapshot_of_file(&paths.app_file(FILE), unix_now()))
}

/// What the file `path` tells at the time `now` (seconds since 1970); nothing when it is stale, comes from this
/// process or cannot be read.
pub fn snapshot_of_file(path: &str, now: u64) -> Option<Snapshot> {
    let data = jsonio::read_json(path).ok()?;
    let number = |key: &str| match data.get(key) {
        Some(Value::Int(text)) => text.parse::<i64>().ok(),
        _ => None,
    };
    let real = |key: &str| match data.get(key) {
        Some(Value::Float(value)) => Some(*value),
        Some(Value::Int(text)) => text.parse().ok(),
        _ => None,
    };
    if number("pid")? == i64::from(std::process::id()) {
        return None;
    }
    let (stand, begin) = (number("stand")?.max(0) as u64, number("begin")?.max(0) as u64);
    if now.saturating_sub(stand) > STALE_SECONDS {
        return None;
    }
    let step = Step::from_number(number("schritt")?.max(0) as usize)?;
    let mut inner = Inner {
        step,
        readings_done: number("lesungen")?.max(0) as usize,
        reading: real("anteil").filter(|part| *part > 0.0),
        best: real("gesamt")?.clamp(0.0, 0.99),
        started: Instant::now(),
        started_at: begin,
        file: None,
        last_write: None,
        ended: false,
        visited: Vec::new(),
        waiting: matches!(data.get("wartet"), Some(Value::Bool(true))),
        readings: 0,
        reports: 0,
    };
    let mut seen = inner.snapshot();
    seen.elapsed = now.saturating_sub(begin);
    Some(seen)
}

#[cfg(test)]
mod tests;
