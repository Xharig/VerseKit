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

//! The fourth hotkey of the overlay (Alt+Umschalt+B): it takes the picture of the game for the signature window at
//! once, while the game is in front (`versekit_core::game_picture::take_now`). The grab runs in a thread of its own
//! and keeps the picture in the folder `Intern`, where the main window finds it; a hint row says what came of it.

use std::sync::mpsc::Sender;

use versekit_core::game_picture::{self, Fetched};
use versekit_core::paths::Paths;
use versekit_core::signature_watch::Screen;
use versekit_core::writeguard;

/// The hotkey of the picture of the game and the grab it started.
pub type PictureKey = crate::refinery::KeyJob<Outcome>;

/// How a grab started by the hotkey ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Taken and kept (`ov_hk_bild_ok`).
    Taken,
    /// Star Citizen was not in front: nothing was grabbed (`ov_hk_bild_kein_spiel`).
    NoGame,
    /// The grab or the file failed (`ov_hk_bild_fehler`).
    Failed,
}

impl Outcome {
    /// What a fetch came to, as the hotkey tells it.
    pub fn of(fetched: &Fetched) -> Outcome {
        match fetched {
            Fetched::Got(_) => Outcome::Taken,
            Fetched::NoGame | Fetched::Stopped => Outcome::NoGame,
            Fetched::Failed(_) => Outcome::Failed,
        }
    }

    /// The text of the hint row.
    pub fn hint(self) -> &'static str {
        match self {
            Outcome::Taken => "ov_hk_bild_ok",
            Outcome::NoGame => "ov_hk_bild_kein_spiel",
            Outcome::Failed => "ov_hk_bild_fehler",
        }
    }
}

/// The grab of the hotkey: in a thread of its own, under the lock of the writes of the overlay.
pub fn take(paths: Paths, guard: writeguard::State, done: Sender<Outcome>, wake: impl Fn() + Send + 'static) {
    let refused = done.clone();
    let started = std::thread::Builder::new().name("spielbild-hotkey".into()).spawn(move || {
        writeguard::adopt(&guard);
        let fetched = game_picture::take_now(&mut Screen, &paths.app_file(game_picture::FILE));
        let _ = done.send(Outcome::of(&fetched));
        wake();
    });
    if started.is_err() {
        let _ = refused.send(Outcome::Failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use versekit_core::game_picture::GamePicture;
    use versekit_core::overlay::Hotkey;
    use versekit_core::screen_grab::GrabError;

    #[test]
    fn each_end_of_the_grab_has_its_hint() {
        let picture = GamePicture { w: 1, h: 1, game: (1, 1), rgb: vec![0, 0, 0] };
        assert_eq!(Outcome::of(&Fetched::Got(picture)), Outcome::Taken);
        assert_eq!(Outcome::of(&Fetched::NoGame), Outcome::NoGame);
        assert_eq!(Outcome::of(&Fetched::Stopped), Outcome::NoGame);
        assert_eq!(Outcome::of(&Fetched::Failed(GrabError::Failed)), Outcome::Failed);
        assert_eq!(Outcome::Taken.hint(), "ov_hk_bild_ok");
        assert_eq!(Outcome::NoGame.hint(), "ov_hk_bild_kein_spiel");
        assert_eq!(Outcome::Failed.hint(), "ov_hk_bild_fehler");
    }

    #[test]
    fn a_press_starts_one_grab_at_a_time() {
        let (tx, rx) = mpsc::channel();
        let mut key = PictureKey::new(Hotkey::parse(versekit_core::overlay::DEFAULT_PICTURE_HOTKEY), rx);
        let mut started = Vec::new();
        tx.send(()).unwrap();
        tx.send(()).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), None);
        assert_eq!(started.len(), 1, "two quick presses are one grab");
        // A press while the grab runs starts no second one.
        tx.send(()).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), None);
        assert_eq!(started.len(), 1, "a press while it grabs starts no second one");
        started[0].send(Outcome::Taken).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), Some(Outcome::Taken));
        assert_eq!(key.poll(|done| started.push(done)), None, "told once");
        // Once it ended, the next press starts the next grab.
        tx.send(()).unwrap();
        assert_eq!(key.poll(|done| started.push(done)), None);
        assert_eq!(started.len(), 2, "the next press after the end");
    }
}
