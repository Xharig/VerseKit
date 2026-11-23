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

//! The two hotkeys of the stock scan: the info card under the mouse pointer read and its amount set at the place
//! of the player, into the own stock ([`Target::Own`]) or into the stock of the unit ([`Target::Unit`]), and a
//! notice that names the stock it went to ([`versekit_core::stock_scan_book`]).

use std::sync::mpsc::Sender;

use versekit_core::logsource::LogSource;
use versekit_core::paths::Paths;
use versekit_core::refinery_scan::Reason;
use versekit_core::stock_scan;
use versekit_core::stock_scan_book::{self, Outcome, Reading, Run, Target, UnitStock};
use versekit_core::writeguard;

/// A hotkey of the stock scan and the scan it started.
pub type StockKey = crate::refinery::KeyJob<Outcome>;

/// Reads the info card under `pointer` in the window of the game ([`stock_scan::read_game`]) with the names of the
/// stock and the texts of the game of `paths`. `None` when the game is not in front or the reading failed (a
/// failure other than a missing language of the text recognition goes to the error log).
fn read_card(paths: &Paths, pointer: (i32, i32)) -> Option<Reading> {
    let context = stock_scan::scan_context(paths);
    match stock_scan::read_game(pointer, &context.names, context.texts.as_ref()) {
        Ok(read) => read.map(|outcome| outcome.reading()),
        Err(error) => {
            if error.reason != Reason::NoLanguage {
                versekit_core::errors::record("overlay.lager_hotkey", &error);
            }
            None
        }
    }
}

/// The work of one press, without the thread. The checks that need no screen come first: a scan into the stock
/// of the unit without the setting, and a place that is not known, stop before anything is read.
pub fn scan(paths: &Paths, target: Target, pointer: Option<(i32, i32)>, now: i64) -> Outcome {
    let Some(pointer) = pointer else { return Outcome::NoGame(target) };
    if let Some(stopped) = stock_scan_book::check(paths, target) {
        return stopped;
    }
    let Some((place, place_since)) = stock_scan_book::current_place_since(paths) else {
        return Outcome::NoPlace(target);
    };
    let reading = read_card(paths, pointer).unwrap_or(Reading::Nothing);
    let link = (target == Target::Unit && reading != Reading::Nothing).then(|| {
        let account = LogSource::new(paths, LogSource::sources_of(paths)).own_account(None);
        stock_scan_book::org::OrgLink::system(paths, account, env!("CARGO_PKG_VERSION"))
    });
    let unit: Option<&dyn UnitStock> = link.as_ref().and_then(Option::as_ref).map(|l| l as &dyn UnitStock);
    stock_scan_book::run(&Run { paths, target, place: Some(place), place_since, unit, now }, reading)
}

/// One scan in a thread of its own, under the lock of the writes of the overlay. `pointer` is where the mouse stood
/// when the key was pressed.
pub fn start(
    paths: Paths,
    target: Target,
    pointer: Option<(i32, i32)>,
    guard: writeguard::State,
    done: Sender<Outcome>,
    wake: impl Fn() + Send + 'static,
) {
    let refused = done.clone();
    let started = std::thread::Builder::new().name("lager-hotkey".into()).spawn(move || {
        writeguard::adopt(&guard);
        let now = versekit_core::export::now_unix();
        let _ = done.send(scan(&paths, target, pointer, now));
        wake();
    });
    if started.is_err() {
        let _ = refused.send(Outcome::Failed(target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        Paths::new(versekit_core::paths::Host::current())
    }

    #[test]
    fn without_a_pointer_nothing_is_read() {
        assert_eq!(scan(&paths(), Target::Own, None, 0), Outcome::NoGame(Target::Own));
        assert_eq!(scan(&paths(), Target::Unit, None, 0), Outcome::NoGame(Target::Unit));
    }
}
