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

//! The hotkey of the blueprint scan: the name of the blueprint beside the mouse pointer at the crafting terminal
//! read ([`versekit_core::blueprint_scan::read_game`]), written into the collection when it was read sure
//! ([`versekit_core::blueprint_scan::record`]), and a notice of what came of it.

use std::sync::mpsc::Sender;

use versekit_core::blueprint_scan::{self, Action};
use versekit_core::language;
use versekit_core::paths::Paths;
use versekit_core::refinery_scan::Reason;
use versekit_core::scan_notice::{Notice, NoticeKind};
use versekit_core::writeguard;

/// How a scan started by the hotkey ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Read: what was done with it.
    Done(Action),
    /// Star Citizen was not in front, or the pointer stood outside its window (`s_bpscan_kein_spiel`).
    NoGame,
    /// No catalog to compare with (`s_bpscan_kein_katalog`).
    NoCatalog,
    /// The reading or the save failed (`s_bpscan_fehler`).
    Failed,
}

impl Outcome {
    /// The notice of the outcome, in the language of the program.
    pub fn notice(&self) -> Notice {
        let lang = language::current();
        let failure = |key: &str| Notice::new(NoticeKind::Nothing, language::t_in(lang, key), "");
        match self {
            Outcome::Done(action) => blueprint_scan::notice_in(lang, action),
            Outcome::NoGame => Notice::game_away(language::t_in(lang, "s_bpscan_kein_spiel")),
            Outcome::NoCatalog => failure("s_bpscan_kein_katalog"),
            Outcome::Failed => failure("s_bpscan_fehler"),
        }
    }
}

/// The hotkey of the blueprint scan and the scan it started.
pub type BlueprintKey = crate::refinery::KeyJob<Outcome>;

/// The work of one press, without the thread: the catalog, the reading at `pointer`, the collection.
pub fn scan(paths: &Paths, pointer: Option<(i32, i32)>) -> Outcome {
    let Some(pointer) = pointer else { return Outcome::NoGame };
    let Some(names) = blueprint_scan::names_of(paths) else { return Outcome::NoCatalog };
    match blueprint_scan::read_game(pointer, &names) {
        Ok(Some(read)) => match blueprint_scan::record(paths, &read) {
            Ok(action) => Outcome::Done(action),
            Err(error) => {
                versekit_core::errors::record("overlay.bauplan_hotkey", &error);
                Outcome::Failed
            }
        },
        Ok(None) => Outcome::NoGame,
        Err(error) => {
            if error.reason != Reason::NoLanguage {
                versekit_core::errors::record("overlay.bauplan_hotkey", &error);
            }
            Outcome::Failed
        }
    }
}

/// How long the hint of the tile the pointer left before stays on the screen while it fades out.
pub const HINT_FADE: std::time::Duration = std::time::Duration::from_millis(300);

/// One scan in a thread of its own, under the lock of the writes of the overlay. `pointer` is where the mouse stood
/// when the key was pressed.
pub fn start(
    paths: Paths,
    pointer: Option<(i32, i32)>,
    guard: writeguard::State,
    done: Sender<Outcome>,
    wake: impl Fn() + Send + 'static,
) {
    let refused = done.clone();
    let started = std::thread::Builder::new().name("bauplan-hotkey".into()).spawn(move || {
        writeguard::adopt(&guard);
        // The hint of the tile left a moment ago fades out while the new one shows: read once it is gone.
        std::thread::sleep(HINT_FADE);
        let _ = done.send(scan(&paths, pointer));
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
    fn every_end_of_the_scan_has_its_notice() {
        for outcome in [Outcome::NoGame, Outcome::NoCatalog, Outcome::Failed] {
            let notice = outcome.notice();
            assert_eq!(notice.kind, NoticeKind::Nothing);
            assert!(!notice.title.starts_with("s_bpscan_"), "{outcome:?}: {notice:?}");
            assert_eq!(notice.away, outcome == Outcome::NoGame, "{outcome:?}");
        }
        let done = Outcome::Done(Action::Add("Sedulity".into())).notice();
        assert_eq!((done.kind, done.detail.as_str()), (NoticeKind::Done, "Sedulity"));
    }

    #[test]
    fn without_a_pointer_nothing_is_read() {
        let paths = Paths::new(versekit_core::paths::Host::current());
        assert_eq!(scan(&paths, None), Outcome::NoGame);
    }
}
