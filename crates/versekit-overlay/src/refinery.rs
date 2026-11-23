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
//! Lager (`refinery_scan::screen::hand_over`), and the overlay then opens the main window on it. A notice over the
//! game says what came of it ([`Outcome::notice`]); without the game in front, or when the reading fails, a hint row
//! says so.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use versekit_core::language::{self, Arg, Lang};
use versekit_core::overlay::Hotkey;
use versekit_core::paths::Paths;
use versekit_core::refinery_scan::{Reason, ScannedJob, screen};
use versekit_core::scan_notice::{Notice, NoticeKind};
use versekit_core::writeguard;

/// What a reading found: the materials read, those without sure numbers, and whether every job of it was in the
/// list of the jobs already.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub materials: usize,
    pub unsure: usize,
    pub known: bool,
    /// The terminal was seen, but no table of a job on it.
    pub covered: bool,
}

impl Found {
    /// What the jobs of a reading hold; `known` from the list as it was before the reading.
    pub fn of(jobs: &[ScannedJob], known: bool) -> Found {
        Found {
            materials: jobs.iter().map(|job| job.materials.len()).sum(),
            unsure: jobs.iter().map(|job| job.unsure.len()).sum(),
            known,
            covered: jobs.iter().any(|job| job.covered),
        }
    }
}

/// How a reading started by the hotkey ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Read and handed over: the main window is to open on Mein Lager.
    Read(Found),
    /// Star Citizen was not in front: nothing was read (`rf_hk_kein_spiel`).
    NoGame,
    /// The reading failed (`rf_hk_fehler`).
    Failed,
}

impl Outcome {
    /// Whether the main window opens on the reading: only when something was read.
    pub fn opens_main(self) -> bool {
        matches!(self, Outcome::Read(found) if found.materials > 0)
    }

    /// The notice while the reading runs, in the language of the program.
    pub fn working() -> Notice {
        Notice::new(NoticeKind::Working, language::t("s_rfscan_t_liest"), "")
    }

    /// The notice over the game, in the language of the program.
    pub fn notice(self) -> Notice {
        self.notice_in(language::current())
    }

    /// The notice in `lang`: no material read, values to check, a job known already, or the materials taken.
    pub fn notice_in(self, lang: Lang) -> Notice {
        let count = |n: usize, one: &str, more: &str| {
            if n == 1 {
                language::t_in(lang, one)
            } else {
                language::tf_in(lang, more, &[("0", Arg::Int(n as i64))])
            }
        };
        match self {
            Outcome::Read(found) if found.materials == 0 && found.covered => {
                Notice::new(NoticeKind::Nothing, language::t_in(lang, "s_rfscan_t_verdeckt"), "")
            }
            Outcome::Read(found) if found.materials == 0 => {
                Notice::new(NoticeKind::Nothing, language::t_in(lang, "s_rfscan_t_keiner"), "")
            }
            Outcome::Read(found) if found.unsure > 0 => Notice::new(
                NoticeKind::Unsure,
                language::t_in(lang, "s_rfscan_t_unsicher"),
                count(found.unsure, "s_rfscan_ein_wert", "s_rfscan_werte"),
            ),
            Outcome::Read(found) if found.known => Notice::new(
                NoticeKind::AlreadyThere,
                language::t_in(lang, "s_rfscan_t_schon"),
                language::t_in(lang, "s_rfscan_dieser_auftrag"),
            ),
            Outcome::Read(found) => Notice::new(
                NoticeKind::Done,
                language::t_in(lang, "s_rfscan_t_erfasst"),
                count(found.materials, "s_rfscan_ein_material", "s_rfscan_materialien"),
            ),
            Outcome::NoGame => Notice::game_away(language::t_in(lang, "rf_hk_kein_spiel")),
            Outcome::Failed => Notice::new(NoticeKind::Nothing, language::t_in(lang, "rf_hk_fehler"), ""),
        }
    }
}

/// A hotkey that starts a job in a thread of its own, one at a time, and what the job came to: the refinery scan,
/// the picture of the game.
pub struct KeyJob<T> {
    /// The combination, read from the settings at every key.
    pub hotkey: Arc<Mutex<Option<Hotkey>>>,
    pub presses: Receiver<()>,
    done: (Sender<T>, Receiver<T>),
    running: bool,
}

/// The hotkey of the refinery scan and the reading it started.
pub type RefineryKey = KeyJob<Outcome>;

impl<T> KeyJob<T> {
    pub fn new(hotkey: Option<Hotkey>, presses: Receiver<()>) -> KeyJob<T> {
        KeyJob { hotkey: Arc::new(Mutex::new(hotkey)), presses, done: mpsc::channel(), running: false }
    }

    /// The presses since the last look start a job, one at a time; `start` runs it in a thread of its own (for the
    /// refinery: [`read_screen`]). Gives what came of a job that ended since.
    pub fn poll(&mut self, start: impl FnOnce(Sender<T>)) -> Option<T> {
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

    /// Whether a job started by this key has not ended yet.
    pub fn running(&self) -> bool {
        self.running
    }
}

/// The reading of the hotkey: in a thread of its own, under the lock of the writes of the overlay. A missing
/// language of the text recognition is no error of the program and is not recorded. `grabbed` is set once the
/// picture is taken: only then does the overlay show that the reading runs, so its notice is never in the picture.
pub fn read_screen(
    paths: Paths,
    guard: writeguard::State,
    done: Sender<Outcome>,
    grabbed: Arc<AtomicBool>,
    wake: impl Fn() + Send + 'static,
) {
    let refused = done.clone();
    let started = std::thread::Builder::new().name("raffinerie-hotkey".into()).spawn(move || {
        writeguard::adopt(&guard);
        // The list as it was before the reading keeps it: whether the jobs read were in it already.
        let before = versekit_core::refinery_jobs::load(&paths);
        let taken = || {
            grabbed.store(true, Ordering::SeqCst);
            wake();
        };
        let outcome = match screen::scan_from_game_then(&paths, &taken) {
            Ok(Some((jobs, kept))) => {
                if screen::hand_over(&paths, &jobs, kept) {
                    Outcome::Read(Found::of(&jobs, versekit_core::refinery_jobs::all_known(&before, &jobs)))
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
    }

    fn job(materials: &[&str], unsure: &[&str]) -> ScannedJob {
        ScannedJob {
            text: String::new(),
            unsure: unsure.iter().map(|m| m.to_string()).collect(),
            materials: materials.iter().map(|m| m.to_string()).collect(),
            total: "1".into(),
            state: versekit_core::refinery_scan::State::Running,
            remaining: Some(60),
            cells: Vec::new(),
            station: String::new(),
            covered: false,
        }
    }

    /// The terminal seen without a table: the notice says it is too small or covered, not that there is no job.
    #[test]
    fn a_covered_terminal_has_its_own_notice() {
        let mut seen = job(&[], &[]);
        seen.covered = true;
        let found = Found::of(&[seen], false);
        assert!(found.covered);
        let notice = Outcome::Read(found).notice_in(Lang::De);
        assert_eq!((notice.kind, notice.title.as_str()), (NoticeKind::Nothing, "Terminal zu klein oder verdeckt"));
        assert_eq!(Outcome::Read(found).notice_in(Lang::En).title, "Terminal too small or covered");
        assert!(!Outcome::Read(found).opens_main());
        // A table read beside a covered card counts as read.
        let mut both = Found::of(&[job(&["Gold"], &[])], false);
        both.covered = true;
        assert_eq!(Outcome::Read(both).notice_in(Lang::De).kind, NoticeKind::Done);
    }

    /// Without the game in front nothing is grabbed: the flag that lets the notice "wird gelesen" show stays down.
    #[test]
    fn without_a_grab_the_notice_of_the_running_reading_is_not_let_out() {
        let folder = versekit_core::testdir::fresh("overlay-raffinerie-abgriff");
        let home = folder.to_string_lossy().into_owned();
        let paths = Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::envvars::ENV_HOME, home.as_str())],
            &home,
        ));
        let (tx, rx) = mpsc::channel();
        let grabbed = Arc::new(AtomicBool::new(false));
        if versekit_core::screen_grab::game_rect().is_none() {
            read_screen(paths, writeguard::state(), tx, grabbed.clone(), || {});
            assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(20)), Ok(Outcome::NoGame));
            assert!(!grabbed.load(Ordering::SeqCst));
        }
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn every_end_of_the_reading_has_its_notice_in_words() {
        let jobs = [job(&["Gold", "Iron"], &[]), job(&["Tin"], &[])];
        let found = Found::of(&jobs, false);
        assert_eq!(found, Found { materials: 3, unsure: 0, known: false, covered: false });
        let notice = |found: Found, lang: Lang| {
            let n = Outcome::Read(found).notice_in(lang);
            (n.kind, n.title, n.detail)
        };
        assert_eq!(notice(found, Lang::De), (NoticeKind::Done, "Raffinerie erfasst".into(), "3 Materialien".into()));
        assert_eq!(notice(found, Lang::En), (NoticeKind::Done, "Refinery captured".into(), "3 materials".into()));
        let one = Found { materials: 1, ..found };
        assert_eq!(notice(one, Lang::De).2, "1 Material");
        // Known already: the job is not taken twice.
        let known = Found { known: true, ..found };
        assert_eq!(notice(known, Lang::De), (NoticeKind::AlreadyThere, "Schon erfasst".into(), "dieser Auftrag".into()));
        // Values to check come before known and taken.
        let unsure = Found::of(&[job(&["Gold", "Iron"], &["Gold", "Iron"])], true);
        assert_eq!(notice(unsure, Lang::De), (NoticeKind::Unsure, "Unsicher".into(), "2 Werte prüfen".into()));
        assert_eq!(notice(Found { unsure: 1, ..unsure }, Lang::De).2, "1 Wert prüfen");
        // Nothing read: no job.
        assert_eq!(notice(Found::of(&[job(&[], &[])], false), Lang::De).1, "Kein Raffinerie-Auftrag erkannt");
        assert_eq!(notice(Found::default(), Lang::De).0, NoticeKind::Nothing);
        for outcome in [Outcome::NoGame, Outcome::Failed] {
            let n = outcome.notice_in(Lang::De);
            assert_eq!(n.kind, NoticeKind::Nothing);
            assert!(!n.title.starts_with("rf_hk_"), "{n:?}");
            assert_eq!(n.away, outcome == Outcome::NoGame, "{n:?}");
        }
        assert!(!Outcome::Read(Found::default()).notice_in(Lang::De).away, "nothing read is not the game away");
    }

    /// Only a reading that found something opens the main window; nothing read, no game or a failure leave the game
    /// in front. While the reading runs a notice says so, without a sound, until the outcome takes its place.
    #[test]
    fn only_a_reading_with_materials_opens_the_main_window() {
        let found = Found::of(&[job(&["Gold"], &[])], false);
        assert!(Outcome::Read(found).opens_main());
        assert!(Outcome::Read(Found { known: true, ..found }).opens_main(), "known: shown again");
        assert!(!Outcome::Read(Found::default()).opens_main(), "nothing read");
        assert!(!Outcome::NoGame.opens_main() && !Outcome::Failed.opens_main());
        let working = Outcome::working();
        assert_eq!((working.kind.sound(), working.detail.as_str()), (None, ""));
        assert!(working.duration() > Outcome::Read(found).notice().duration());
    }
}
