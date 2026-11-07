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

//! The second hotkey of the overlay (Strg+Alt+R): it reads the refinery terminal while the game is in front
//! (`refinery_from_game` of 4.12). The reading runs in a thread of its own; the result waits for the page Mein
//! Lager (`refinery_scan::screen::hand_over`), and the overlay then opens the main window on it. Without the game
//! in front, or when the reading fails, a hint row says so.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use versekit_core::overlay::Hotkey;
use versekit_core::paths::Paths;
use versekit_core::refinery_scan::{Reason, screen};
use versekit_core::writeguard;

/// How a reading started by the hotkey ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Read and handed over: the main window is to open on Mein Lager.
    Read,
    /// Star Citizen was not in front: nothing was read (`rf_hk_kein_spiel`).
    NoGame,
    /// The reading failed (`rf_hk_fehler`).
    Failed,
}

impl Outcome {
    /// The text of the hint row, `None` when the main window shows the result.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Outcome::Read => None,
            Outcome::NoGame => Some("rf_hk_kein_spiel"),
            Outcome::Failed => Some("rf_hk_fehler"),
        }
    }
}

/// The hotkey of the refinery scan and the reading it started.
pub struct RefineryKey {
    /// The combination, read from the settings at every key.
    pub hotkey: Arc<Mutex<Option<Hotkey>>>,
    pub presses: Receiver<()>,
    done: (Sender<Outcome>, Receiver<Outcome>),
    running: bool,
}

impl RefineryKey {
    pub fn new(hotkey: Option<Hotkey>, presses: Receiver<()>) -> RefineryKey {
        RefineryKey { hotkey: Arc::new(Mutex::new(hotkey)), presses, done: mpsc::channel(), running: false }
    }

    /// The presses since the last look start a reading, one at a time; `start` runs it in a thread of its own
    /// (in the overlay: [`read_screen`]). Gives what came of a reading that ended since.
    pub fn poll(&mut self, start: impl FnOnce(Sender<Outcome>)) -> Option<Outcome> {
        let mut pressed = false;
        while self.presses.try_recv().is_ok() {
            pressed = true;
        }
        if pressed && !self.running {
            self.running = true;
            start(self.done.0.clone());
        }
        let outcome = self.done.1.try_recv().ok();
        if outcome.is_some() {
            self.running = false;
        }
        outcome
    }

    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.running
    }
}

/// The reading of the hotkey: in a thread of its own, under the lock of the writes of the overlay. A missing
/// language of the text recognition is no error of the program and is not recorded.
pub fn read_screen(paths: Paths, guard: writeguard::State, done: Sender<Outcome>, wake: impl Fn() + Send + 'static) {
    let refused = done.clone();
    let started = std::thread::Builder::new().name("raffinerie-hotkey".into()).spawn(move || {
        writeguard::adopt(&guard);
        let outcome = match screen::scan_from_game(&paths) {
            Ok(Some((jobs, kept))) => {
                if screen::hand_over(&paths, &jobs, kept) {
                    Outcome::Read
                } else {
                    Outcome::Failed
                }
            }
            Ok(None) => Outcome::NoGame,
            Err(error) => {
                if error.reason != Reason::NoLanguage {
                    versekit_core::errors::record("overlay.raffinerie_hotkey", &error);
                }
                Outcome::Failed
            }
        };
        let _ = done.send(outcome);
        wake();
    });
    if started.is_err() {
        let _ = refused.send(Outcome::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_starts_one_reading_at_a_time_and_its_end_is_told_once() {
        let (tx, rx) = mpsc::channel();
        let mut key = RefineryKey::new(Hotkey::parse("Strg+Alt+R"), rx);
        let mut started = Vec::new();
        assert_eq!(key.poll(|done| started.push(done)), None, "no press, no reading");
        assert!(started.is_empty());
        tx.send(()).unwrap();
        tx.send(()).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), None);
        assert_eq!(started.len(), 1, "two quick presses are one reading");
        tx.send(()).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), None);
        assert_eq!(started.len(), 1, "a press while it reads starts no second one");
        assert!(key.running());
        started[0].send(Outcome::NoGame).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), Some(Outcome::NoGame));
        assert!(!key.running());
        assert_eq!(key.poll(|done| started.push(done)), None, "told once");
        assert_eq!(Outcome::NoGame.hint(), Some("rf_hk_kein_spiel"));
        assert_eq!(Outcome::Failed.hint(), Some("rf_hk_fehler"));
        assert_eq!(Outcome::Read.hint(), None);
    }
}
