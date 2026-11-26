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

//! The keys of game actions pressed for the Stream Deck: down when its key goes down, up when it comes up, and only
//! into the game while it is in front. What was pressed is let go in any case ([`GameKeys::watch`]): after
//! [`MAX_HOLD`], when the game leaves the front, when the plugin stops asking for [`SILENCE`], and when the program
//! ends ([`GameKeys::release_all`]). An up without a down sends nothing.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::anti_afk::{KeyEvent, SendError, key_events};
use crate::anti_afk_key::Key;

/// The longest a key is held.
pub const MAX_HOLD: Duration = Duration::from_secs(60);
/// The plugin asks every second; this long without a request, held keys are let go.
pub const SILENCE: Duration = Duration::from_secs(3);

/// What sends the keys and says whether the game is in front.
pub trait KeySink: Send + Sync {
    fn game_in_front(&self) -> bool;
    /// Sends the events.
    ///
    /// # Errors
    ///
    /// [`SendError`].
    fn send(&self, events: &[KeyEvent]) -> Result<(), SendError>;
}

/// The real system.
pub struct SystemKeys;

impl KeySink for SystemKeys {
    fn game_in_front(&self) -> bool {
        use crate::anti_afk::Platform;
        crate::anti_afk::System.game_in_front()
    }

    fn send(&self, events: &[KeyEvent]) -> Result<(), SendError> {
        crate::anti_afk::send_player_keys(events)
    }
}

/// Why a press was not sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotSent {
    /// The game is not the window in front.
    NotInFront,
    /// The system did not take the keys.
    Failed,
}

struct Held {
    action: String,
    keys: Vec<Key>,
    since: Instant,
}

/// The keys held down for the Stream Deck.
#[derive(Default)]
pub struct GameKeys {
    held: Mutex<Vec<Held>>,
}

impl GameKeys {
    fn held(&self) -> std::sync::MutexGuard<'_, Vec<Held>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many actions are held now.
    pub fn count(&self) -> usize {
        self.held().len()
    }

    /// Presses the keys of `action`. A second down of an action held already sends nothing.
    ///
    /// # Errors
    ///
    /// [`NotSent`].
    pub fn down(&self, action: &str, keys: &[Key], sink: &dyn KeySink, now: Instant) -> Result<(), NotSent> {
        if !sink.game_in_front() {
            return Err(NotSent::NotInFront);
        }
        let mut held = self.held();
        if held.iter().any(|h| h.action == action) {
            return Ok(());
        }
        if sink.send(&key_events(keys, false)).is_err() {
            // Whatever went down comes up again.
            let _ = sink.send(&key_events(keys, true));
            return Err(NotSent::Failed);
        }
        held.push(Held { action: action.to_string(), keys: keys.to_vec(), since: now });
        Ok(())
    }

    /// Lets go of `action`, if it is held.
    pub fn up(&self, action: &str, sink: &dyn KeySink) {
        let mut held = self.held();
        if let Some(at) = held.iter().position(|h| h.action == action) {
            let gone = held.remove(at);
            let _ = sink.send(&key_events(&gone.keys, true));
        }
    }

    /// Lets go of everything held.
    pub fn release_all(&self, sink: &dyn KeySink) {
        let gone: Vec<Held> = std::mem::take(&mut *self.held());
        for held in gone {
            let _ = sink.send(&key_events(&held.keys, true));
        }
    }

    /// One look: lets go of what was held too long, everything when the game left the front or the plugin went
    /// silent (`contact`: how long ago it asked last).
    pub fn watch(&self, sink: &dyn KeySink, contact: Option<Duration>, now: Instant) {
        if self.held().is_empty() {
            return;
        }
        if !sink.game_in_front() || contact.is_none_or(|ago| ago >= SILENCE) {
            self.release_all(sink);
            return;
        }
        let gone: Vec<Held> = {
            let mut held = self.held();
            let (old, keep): (Vec<Held>, Vec<Held>) =
                std::mem::take(&mut *held).into_iter().partition(|h| now.duration_since(h.since) >= MAX_HOLD);
            *held = keep;
            old
        };
        for held in gone {
            let _ = sink.send(&key_events(&held.keys, true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Fake {
        front: AtomicBool,
        fail: AtomicBool,
        sent: Mutex<Vec<KeyEvent>>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake { front: AtomicBool::new(true), fail: AtomicBool::new(false), sent: Mutex::new(Vec::new()) }
        }
        fn downs(&self) -> usize {
            self.sent.lock().unwrap().iter().filter(|e| !e.up).count()
        }
        fn ups(&self) -> usize {
            self.sent.lock().unwrap().iter().filter(|e| e.up).count()
        }
    }

    impl KeySink for Fake {
        fn game_in_front(&self) -> bool {
            self.front.load(Ordering::SeqCst)
        }
        fn send(&self, events: &[KeyEvent]) -> Result<(), SendError> {
            if self.fail.load(Ordering::SeqCst) && events.iter().any(|e| !e.up) {
                return Err(SendError("no".into()));
            }
            self.sent.lock().unwrap().extend_from_slice(events);
            Ok(())
        }
    }

    fn keys() -> Vec<Key> {
        crate::anti_afk_key::parse("lalt+x").expect("keys")
    }

    #[test]
    fn down_and_up_follow_the_key_with_the_modifier_first_and_last() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        game.down("visor_wipe", &keys(), &fake, Instant::now()).expect("sent");
        game.up("visor_wipe", &fake);
        let sent = fake.sent.lock().unwrap().clone();
        let scans: Vec<(u16, bool)> = sent.iter().map(|e| (e.key.scan, e.up)).collect();
        assert_eq!(scans, vec![(0x38, false), (0x2D, false), (0x2D, true), (0x38, true)]);
        assert_eq!(game.count(), 0);
    }

    #[test]
    fn nothing_goes_to_another_window_than_the_game() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(game.down("v_eject", &keys(), &fake, Instant::now()), Err(NotSent::NotInFront));
        assert!(fake.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn an_up_without_a_down_and_a_second_down_send_nothing() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        game.up("v_eject", &fake);
        assert!(fake.sent.lock().unwrap().is_empty());
        game.down("v_eject", &keys(), &fake, Instant::now()).expect("sent");
        game.down("v_eject", &keys(), &fake, Instant::now()).expect("held already");
        assert_eq!(fake.downs(), 2, "one press of two keys");
    }

    #[test]
    fn a_key_held_too_long_is_let_go() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        let start = Instant::now();
        game.down("v_eject", &keys(), &fake, start).expect("sent");
        game.watch(&fake, Some(Duration::ZERO), start + MAX_HOLD - Duration::from_secs(1));
        assert_eq!(game.count(), 1, "not yet");
        game.watch(&fake, Some(Duration::ZERO), start + MAX_HOLD);
        assert_eq!((game.count(), fake.ups()), (0, 2));
    }

    #[test]
    fn everything_is_let_go_when_the_game_leaves_the_front_or_the_plugin_falls_silent() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        let now = Instant::now();
        game.down("a", &keys(), &fake, now).expect("sent");
        fake.front.store(false, Ordering::SeqCst);
        game.watch(&fake, Some(Duration::ZERO), now);
        assert_eq!(game.count(), 0, "the game left the front");
        fake.front.store(true, Ordering::SeqCst);
        game.down("b", &keys(), &fake, now).expect("sent");
        game.watch(&fake, Some(SILENCE - Duration::from_millis(1)), now);
        assert_eq!(game.count(), 1);
        game.watch(&fake, Some(SILENCE), now);
        assert_eq!(game.count(), 0, "the plugin fell silent");
        game.down("c", &keys(), &fake, now).expect("sent");
        game.watch(&fake, None, now);
        assert_eq!(game.count(), 0, "it never asked");
    }

    #[test]
    fn a_press_the_system_refused_lets_go_of_what_went_down() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        fake.fail.store(true, Ordering::SeqCst);
        assert_eq!(game.down("a", &keys(), &fake, Instant::now()), Err(NotSent::Failed));
        assert_eq!((game.count(), fake.ups()), (0, 2));
    }

    #[test]
    fn at_the_end_everything_held_comes_up() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        game.down("a", &keys(), &fake, Instant::now()).expect("sent");
        game.down("b", &crate::anti_afk_key::parse("f").expect("f"), &fake, Instant::now()).expect("sent");
        game.release_all(&fake);
        assert_eq!((game.count(), fake.ups()), (0, 3));
    }
}
