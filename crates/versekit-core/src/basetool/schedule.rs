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

//! When the next sync is due (`basetool_sync._schedule`, `due`,
//! `local_changed`, `request_now`, `_after_error`).
//!
//! At the start, after an own change (collected for 20 s), otherwise at most
//! every 5 minutes. After errors 5 s, doubled up to 5 minutes, with a random
//! part, never shorter than `Retry-After`. After a stop code (program
//! blocked, right missing …) there is no new attempt by itself, not even
//! after an own change; only a button (`request_now`) or the next start.

use crate::basetool::error::{Action, backoff};

/// At most this often without a reason.
pub const CADENCE: f64 = 300.0;
/// Own changes are collected this long, then synced once.
pub const AFTER_CHANGE: f64 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Schedule {
    /// Due from this time on (seconds since 1970); infinite when halted.
    pub next: f64,
    /// Failures in a row.
    pub attempt: u32,
    /// When the own data last changed, if not synced since.
    pub changed_at: Option<f64>,
    pub halted: bool,
    /// Asked for during a running pass: the next one follows directly.
    pub again: bool,
    pub running: bool,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule { next: 0.0, attempt: 0, changed_at: None, halted: false, again: false, running: false }
    }
}

impl Schedule {
    /// The own data changed: sync soon, collected.
    pub fn local_changed(&mut self, now: f64) {
        self.changed_at = Some(now);
    }

    /// Sync now, without waiting, but never in parallel.
    pub fn request_now(&mut self) {
        self.next = 0.0;
        self.attempt = 0;
        self.halted = false;
        self.again = true;
    }

    pub fn due(&self, now: f64) -> bool {
        if self.running || self.halted {
            return false;
        }
        if let Some(changed) = self.changed_at.filter(|c| *c != 0.0)
            && now - changed >= AFTER_CHANGE
            && self.attempt == 0
        {
            return true;
        }
        now >= self.next
    }

    /// Due, but not connected: try again after the cadence.
    pub fn not_connected(&mut self, now: f64) {
        self.next = now + CADENCE;
    }

    pub fn start(&mut self) {
        self.again = false;
        self.running = true;
    }

    /// A pass went through. `pending`: changes were queued during it.
    pub fn succeeded(&mut self, now: f64, pending: bool) {
        self.attempt = 0;
        self.next = if self.again || pending { 0.0 } else { now + CADENCE };
        self.changed_at = None;
        self.running = false;
    }

    /// A pass failed. A later attempt waits; anything else halts until a
    /// button or the next start.
    pub fn failed(&mut self, now: f64, action: Action, retry_after: Option<u64>, jitter: f64) {
        if action == Action::Retry {
            self.attempt += 1;
            self.next = now + backoff(self.attempt, retry_after, jitter);
        } else {
            self.attempt = 0;
            self.next = f64::INFINITY;
            self.halted = true;
        }
        self.running = false;
    }

    /// Seconds until the next check is worth it: at least `MIN_WAIT` (a
    /// worker never spins), at most `limit`.
    pub fn wait(&self, now: f64, limit: f64) -> f64 {
        let mut until = self.next;
        if let Some(changed) = self.changed_at
            && self.attempt == 0
            && !self.halted
        {
            until = until.min(changed + AFTER_CHANGE);
        }
        (until - now).clamp(MIN_WAIT, limit.max(MIN_WAIT))
    }
}

/// The shortest pause of the worker between two checks, as often as the
/// previous version looked (once per watcher tick).
pub const MIN_WAIT: f64 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_change_and_backoff() {
        let mut s = Schedule::default();
        assert!(s.due(0.0));
        s.start();
        assert!(!s.due(1.0));
        s.succeeded(10.0, false);
        assert!(!s.due(309.0) && s.due(310.0));
        s.local_changed(100.0);
        assert!(!s.due(119.0) && s.due(120.0));
        s.start();
        s.failed(120.0, Action::Retry, None, 0.0);
        assert_eq!((s.attempt, s.next), (1, 125.0));
        // After a failure the change no longer makes it due early.
        assert!(!s.due(124.0) && s.due(125.0));
        s.start();
        s.failed(125.0, Action::Retry, Some(3600), 0.0);
        assert_eq!(s.next, 3725.0);
        s.start();
        s.failed(130.0, Action::Stop, None, 0.0);
        assert!(s.halted && !s.due(1e12));
        s.local_changed(200.0);
        assert!(!s.due(1e12));
        s.request_now();
        assert!(s.due(0.0));
        s.start();
        s.request_now();
        s.succeeded(50.0, false);
        assert_eq!(s.next, 0.0);
        assert_eq!(Schedule { next: 400.0, ..Schedule::default() }.wait(100.0, 1000.0), 300.0);
        // Halted with an old change, or due: still a pause, never 0.
        let halted = Schedule { next: f64::INFINITY, halted: true, changed_at: Some(1.0), ..Schedule::default() };
        assert_eq!(halted.wait(1000.0, 60.0), 60.0);
        assert_eq!(Schedule::default().wait(1000.0, 60.0), MIN_WAIT);
    }
}
