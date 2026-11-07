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

//! The one lock for reading a `Data.p4k`: nothing reads the archive while the game runs. The reading waits and goes on
//! as soon as the game is closed, without an error; the pages say that it waits ([`waiting`]).
//!
//! Whether the game certainly is closed is told by [`game_start::run_state`] (the process list and the logs of every
//! channel); what cannot be told counts as running. A process that runs on a made-up machine of the checks
//! (`paths::ENV_TEST_MACHINE`) reads freely: there is no game of the player there.

use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::game_start::run_state;
use crate::paths::{CHANNELS, ENV_TEST_MACHINE, Paths};

/// How often a waiting reading looks whether the game is closed now.
pub const POLL: Duration = Duration::from_secs(5);

/// How many readings wait for the game to close.
static WAITING: AtomicUsize = AtomicUsize::new(0);

/// The topics of the readings that wait (a reading without a topic waits under the empty one).
static TOPICS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

thread_local! {
    /// What the reading of this thread is for, told by the one that starts it.
    static TOPIC: Cell<&'static str> = const { Cell::new("") };
}

/// What the readings of the thread are for while it lives: the page that shows their data says it waits.
pub struct Topic(&'static str);

impl Drop for Topic {
    fn drop(&mut self) {
        TOPIC.with(|topic| topic.set(self.0));
    }
}

/// Names what the readings of this thread are for (`texts`, `catalog`, `signatures`, `values`, `controls`) until the
/// returned value is dropped.
pub fn reading_for(topic: &'static str) -> Topic {
    Topic(TOPIC.with(|current| current.replace(topic)))
}

/// Whether some reading for `topic` waits for the game to close right now.
pub fn waiting_for(topic: &str) -> bool {
    TOPICS.lock().unwrap_or_else(PoisonError::into_inner).contains(&topic)
}

/// Whether the archive may be read now: the game is certainly closed, or the machine is a made-up one.
pub fn may_read(paths: &Paths) -> bool {
    paths.host.var(ENV_TEST_MACHINE).is_some() || !run_state(paths).blocks_writing()
}

/// Whether some reading waits for the game to close right now.
pub fn waiting() -> bool {
    WAITING.load(Ordering::Relaxed) > 0
}

/// A waiting reading, counted with its topic while it lives.
struct Waiter(&'static str);

impl Waiter {
    fn new() -> Waiter {
        WAITING.fetch_add(1, Ordering::Relaxed);
        let topic = TOPIC.with(Cell::get);
        TOPICS.lock().unwrap_or_else(PoisonError::into_inner).push(topic);
        Waiter(topic)
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        WAITING.fetch_sub(1, Ordering::Relaxed);
        let mut topics = TOPICS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = topics.iter().position(|topic| *topic == self.0) {
            topics.remove(at);
        }
    }
}

/// Waits until `closed` says yes, asking every `poll`; returns at once when it says yes first. Counts as
/// [`waiting`] meanwhile.
pub fn wait_with(closed: &dyn Fn() -> bool, poll: Duration) {
    if closed() {
        return;
    }
    let _waiter = Waiter::new();
    while !closed() {
        std::thread::sleep(poll);
    }
}

/// Waits until the archive may be read ([`may_read`]).
pub fn wait(paths: &Paths) {
    wait_with(&|| may_read(paths), POLL);
}

/// Whether `archive` lies in a channel of test builds (every channel of [`CHANNELS`] but `LIVE`, `HOTFIX` included,
/// whose archives are confidential), told by the name of the folder that holds it.
pub fn is_test_archive(archive: &Path) -> bool {
    archive
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().to_uppercase())
        .is_some_and(|name| name != "LIVE" && CHANNELS.contains(&name.as_str()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn machine(vars: &[(&str, &str)]) -> Paths {
        let home = std::env::temp_dir().join(format!("versekit-sperre-{}", crate::testdir::run_tag()));
        let text = home.to_string_lossy().into_owned();
        let mut all = vec![("HOME", text.as_str()), ("SC_BP_HOME", text.as_str())];
        all.extend_from_slice(vars);
        Paths::new(Host::new(Flavor::native(), &all, &text))
    }

    #[test]
    fn a_reading_goes_on_at_once_when_the_game_is_closed_and_waits_while_it_runs() {
        // Closed: no waiting, nothing counted.
        let before = WAITING.load(Ordering::Relaxed);
        wait_with(&|| true, Duration::from_millis(1));
        assert_eq!(WAITING.load(Ordering::Relaxed), before);
        // Running: the reading waits, is counted, and goes on after the game closed.
        let closed = Arc::new(AtomicBool::new(false));
        let reader = {
            let closed = Arc::clone(&closed);
            std::thread::spawn(move || {
                wait_with(&|| closed.load(Ordering::SeqCst), Duration::from_millis(5));
            })
        };
        let start = std::time::Instant::now();
        while !waiting() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(waiting(), "the reading waits while the game runs");
        assert!(!reader.is_finished(), "and does not go on");
        closed.store(true, Ordering::SeqCst);
        reader.join().expect("the reading ended");
        assert!(!waiting() || before > 0, "nothing waits any more");
    }

    #[test]
    fn what_cannot_be_told_counts_as_running_and_a_made_up_machine_reads_freely() {
        // Only a game that certainly is closed lets the archive be read.
        use crate::game_start::RunState;
        assert!(RunState::Unknown.blocks_writing() && RunState::Running.blocks_writing());
        assert!(!RunState::Idle.blocks_writing());
        // The made-up machine of the checks has no game of the player.
        let made_up = machine(&[(crate::game_start::ENV_NO_PROCESS, "1"), (ENV_TEST_MACHINE, "x")]);
        assert!(may_read(&made_up));
    }

    #[test]
    fn an_archive_of_a_test_channel_is_told_by_its_folder() {
        for channel in ["PTU", "ptu", "EPTU", "HOTFIX", "TECH-PREVIEW"] {
            assert!(is_test_archive(Path::new(&format!("C:/Games/StarCitizen/{channel}/Data.p4k"))), "{channel}");
        }
        for channel in ["LIVE", "live", "PTUX", "Anything"] {
            assert!(!is_test_archive(Path::new(&format!("C:/Games/StarCitizen/{channel}/Data.p4k"))), "{channel}");
        }
        assert!(!is_test_archive(Path::new("Data.p4k")));
    }
}
