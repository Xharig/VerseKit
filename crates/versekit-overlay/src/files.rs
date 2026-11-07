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

//! The files of the overlay away from the drawing thread. One thread reads
//! the settings and the remembered place, once a second or at once when
//! asked, and writes settings and the place, one job after the other: a
//! read never overtakes a write asked for before it.
//!
//! ⚠ On a network share every access to a file can take from milliseconds
//! to seconds. Done in the drawing thread, the overlay stood still meanwhile
//! and could not be dragged.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use versekit_core::contested;
use versekit_core::jsonio::{Map, Value};
use versekit_core::overlay::{self, Rect, Remembered, Settings};
use versekit_core::paths::{Paths, Recovered};
use versekit_core::window_signal::{self, Role, Signal};
use versekit_core::writeguard;

/// How often the settings are read without being asked.
const READ_EVERY: Duration = Duration::from_secs(1);

/// Under `--probe --langsam <ms>`: how long each file job takes in addition.
static SLOW_MS: AtomicU64 = AtomicU64::new(0);

pub fn set_slow(ms: u64) {
    SLOW_MS.store(ms, Ordering::Relaxed);
}

fn slow() {
    let ms = SLOW_MS.load(Ordering::Relaxed);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// What the settings file and the place file say.
pub struct Snapshot {
    pub settings: Settings,
    /// The settings as stored, for the scheme, the language and the text size.
    pub data: Map,
    pub remembered: Option<Remembered>,
    /// How many writes were done when this was read.
    pub writes_done: u64,
    /// The settings file was damaged when it was read: what became of it. Told once.
    pub recovered: Option<Recovered>,
    /// The start of the hangar cycle the page Contested Zones keeps; read only while the line of the hangar is on.
    pub hangar_start: Option<i64>,
}

/// A job for the file thread.
pub enum Job {
    /// Read; `fresh` reads the settings file whatever its stamp says.
    Read { fresh: bool },
    /// Write a setting; the main window is told afterwards.
    Setting(String, Value),
    /// Remember the open place (points).
    Place(Rect),
}

/// Does one job; `Some` for a read.
pub fn run(paths: &Paths, job: Job, writes_done: &mut u64) -> Option<Snapshot> {
    slow();
    match job {
        Job::Read { fresh } => {
            if fresh {
                paths.forget_settings();
            }
            let settings = Settings::read(paths);
            let data = paths.settings();
            // This thread reads most often, so it is the first to meet a damaged file.
            let recovered = paths.settings_recovered();
            paths.clear_settings_recovered();
            let hangar_start = if settings.hangar_timer { contested::load(paths).start() } else { None };
            Some(Snapshot {
                hangar_start,
                settings,
                data,
                remembered: overlay::load_remembered(paths),
                writes_done: *writes_done,
                recovered,
            })
        }
        Job::Setting(key, value) => {
            if paths.set_setting(&key, value) {
                window_signal::send_to(Role::Main, Signal::Settings);
            }
            *writes_done += 1;
            None
        }
        Job::Place(place) => {
            overlay::save_place(paths, place);
            *writes_done += 1;
            None
        }
    }
}

/// The drawing thread's end of the file thread.
pub struct Files {
    jobs: Sender<Job>,
    snapshots: Receiver<Snapshot>,
    /// Writes asked for; a snapshot from before the last of them is old.
    pub writes_sent: u64,
    /// A damaged settings file that was met and not yet told.
    recovered: std::cell::Cell<Option<Recovered>>,
}

impl Files {
    pub fn ask(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn write(&mut self, job: Job) {
        self.writes_sent += 1;
        self.ask(job);
    }

    /// The newest snapshot that knows every write asked for, if one came.
    pub fn take(&self) -> Option<Snapshot> {
        let mut newest = None;
        while let Ok(snapshot) = self.snapshots.try_recv() {
            // The hint about a damaged file is told once, so it must not be lost with a snapshot
            // that is dropped as old: it waits for the next one that is taken.
            if snapshot.recovered.is_some() {
                self.recovered.set(snapshot.recovered);
            }
            newest = Some(snapshot);
        }
        let mut taken = newest.filter(|s| s.writes_done >= self.writes_sent)?;
        taken.recovered = self.recovered.take();
        Some(taken)
    }
}

/// Starts the file thread on its own `paths`; `wake` brings a frame.
pub fn start(paths: Paths, guard: writeguard::State, wake: impl Fn() + Send + 'static) -> Files {
    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
    let (snap_tx, snap_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("dateien".into())
        .spawn(move || {
            writeguard::adopt(&guard);
            let mut writes_done = 0;
            let mut last_read = Instant::now() - READ_EVERY;
            loop {
                let wait = READ_EVERY.saturating_sub(last_read.elapsed());
                let job = match jobs_rx.recv_timeout(wait) {
                    Ok(job) => job,
                    Err(RecvTimeoutError::Timeout) => Job::Read { fresh: false },
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                if matches!(job, Job::Read { .. }) {
                    last_read = Instant::now();
                }
                if let Some(snapshot) = run(&paths, job, &mut writes_done) {
                    if snap_tx.send(snapshot).is_err() {
                        return;
                    }
                    wake();
                }
            }
        })
        .expect("file thread");
    Files { jobs: jobs_tx, snapshots: snap_rx, writes_sent: 0, recovered: std::cell::Cell::new(None) }
}

/// Is a read by the clock due after one at `last`?
pub fn read_due(last: Instant) -> bool {
    last.elapsed() >= READ_EVERY
}
