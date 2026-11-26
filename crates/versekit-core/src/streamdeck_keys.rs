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
//!
//! ⚠ The game takes the key of some actions only held long enough (`delayed_press`), only let go soon enough (`tap`)
//! or only tapped twice (`double_tap`) ([`Timing`]). A tap on the deck is short and a press may be long, so the keys
//! keep to the timing of the action, not to the key of the deck: an up comes at the earliest after the threshold of
//! a held action plus [`MARGIN`], at the latest [`MARGIN`] before the limit of a tapped one (the later up of the deck
//! then sends nothing), and a double tap sends two taps by itself. [`GameKeys::watch`] sends what is planned; it runs
//! every [`WATCH_EVERY`].

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::anti_afk::{KeyEvent, SendError, key_events};
use crate::anti_afk_key::Key;
use crate::controls::Timing;

/// The longest a key is held.
pub const MAX_HOLD: Duration = Duration::from_secs(60);
/// The plugin asks every second; this long without a request, held keys are let go.
pub const SILENCE: Duration = Duration::from_secs(3);
/// The shortest a key stays down, so the game sees it in one of its frames.
pub const SHORTEST: Duration = Duration::from_millis(40);
/// How much longer than the game asks a key is held, or how much sooner it is let go.
pub const MARGIN: Duration = Duration::from_millis(50);
/// How often the planned keys are looked at.
pub const WATCH_EVERY: Duration = Duration::from_millis(20);

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
    timing: Timing,
    /// The keys planned to go up (`true`) or down, at these times, in order.
    plan: Vec<(Instant, bool)>,
}

/// What is planned for a key that just went down by `timing`: a tapped action comes up before its limit, a double
/// tap taps twice.
fn plan_at_down(timing: Timing, now: Instant) -> Vec<(Instant, bool)> {
    match timing {
        Timing::Within(ms) => {
            let limit = Duration::from_millis(u64::from(ms));
            vec![(now + limit.saturating_sub(MARGIN).max(SHORTEST), true)]
        }
        Timing::Twice => vec![(now + SHORTEST, true), (now + SHORTEST * 5 / 2, false), (now + SHORTEST * 7 / 2, true)],
        Timing::Follow | Timing::AtLeast(_) => Vec::new(),
    }
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

    /// Presses the keys of `action`, taken by the game as `timing` says. A second down of an action held already
    /// sends nothing.
    ///
    /// # Errors
    ///
    /// [`NotSent`].
    pub fn down(&self, action: &str, keys: &[Key], timing: Timing, sink: &dyn KeySink, now: Instant) -> Result<(), NotSent> {
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
        let plan = plan_at_down(timing, now);
        held.push(Held { action: action.to_string(), keys: keys.to_vec(), since: now, timing, plan });
        Ok(())
    }

    /// The key of the deck came up for `action`: its keys go up now, or later where the game must see them held
    /// longer (then [`GameKeys::watch`] lets them go). A double tap goes on by itself.
    pub fn up(&self, action: &str, sink: &dyn KeySink, now: Instant) {
        let mut held = self.held();
        let Some(at) = held.iter().position(|h| h.action == action) else { return };
        let item = &mut held[at];
        let when = match item.timing {
            Timing::Follow => now,
            Timing::Twice => return,
            Timing::AtLeast(ms) => item.since + Duration::from_millis(u64::from(ms)) + MARGIN,
            Timing::Within(_) => item.since + SHORTEST,
        };
        // A planned up that comes sooner stays.
        let planned = item.plan.iter().find(|(_, up)| *up).map(|(t, _)| *t);
        let when = planned.map_or(when, |p| p.min(when)).max(now);
        if when <= now {
            let gone = held.remove(at);
            let _ = sink.send(&key_events(&gone.keys, true));
        } else {
            item.plan = vec![(when, true)];
        }
    }

    /// Sends what is planned up to `now`; a key whose plan ended with an up is let go.
    fn send_planned(&self, sink: &dyn KeySink, now: Instant) {
        let mut held = self.held();
        let mut done = Vec::new();
        for (index, item) in held.iter_mut().enumerate() {
            while let Some(&(when, up)) = item.plan.first() {
                if when > now {
                    break;
                }
                item.plan.remove(0);
                let _ = sink.send(&key_events(&item.keys, up));
                if up && item.plan.is_empty() {
                    done.push(index);
                }
            }
        }
        for index in done.into_iter().rev() {
            held.remove(index);
        }
    }

    /// Lets go of everything held.
    pub fn release_all(&self, sink: &dyn KeySink) {
        let gone: Vec<Held> = std::mem::take(&mut *self.held());
        for held in gone {
            let _ = sink.send(&key_events(&held.keys, true));
        }
    }

    /// One look: sends what is planned up to `now`, lets go of what was held too long, everything when the game left
    /// the front or the plugin went silent (`contact`: how long ago it asked last).
    pub fn watch(&self, sink: &dyn KeySink, contact: Option<Duration>, now: Instant) {
        if self.held().is_empty() {
            return;
        }
        if !sink.game_in_front() || contact.is_none_or(|ago| ago >= SILENCE) {
            self.release_all(sink);
            return;
        }
        self.send_planned(sink, now);
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
        game.down("visor_wipe", &keys(), Timing::Follow, &fake, Instant::now()).expect("sent");
        game.up("visor_wipe", &fake, Instant::now());
        let sent = fake.sent.lock().unwrap().clone();
        let scans: Vec<(u16, bool)> = sent.iter().map(|e| (e.key.scan, e.up)).collect();
        assert_eq!(scans, vec![(0x38, false), (0x2D, false), (0x2D, true), (0x38, true)]);
        assert_eq!(game.count(), 0);
    }

    #[test]
    fn nothing_goes_to_another_window_than_the_game() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(game.down("v_eject", &keys(), Timing::Follow, &fake, Instant::now()), Err(NotSent::NotInFront));
        assert!(fake.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn an_up_without_a_down_and_a_second_down_send_nothing() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        game.up("v_eject", &fake, Instant::now());
        assert!(fake.sent.lock().unwrap().is_empty());
        game.down("v_eject", &keys(), Timing::Follow, &fake, Instant::now()).expect("sent");
        game.down("v_eject", &keys(), Timing::Follow, &fake, Instant::now()).expect("held already");
        assert_eq!(fake.downs(), 2, "one press of two keys");
    }

    #[test]
    fn a_key_held_too_long_is_let_go() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        let start = Instant::now();
        game.down("v_eject", &keys(), Timing::Follow, &fake, start).expect("sent");
        game.watch(&fake, Some(Duration::ZERO), start + MAX_HOLD - Duration::from_secs(1));
        assert_eq!(game.count(), 1, "not yet");
        game.watch(&fake, Some(Duration::ZERO), start + MAX_HOLD);
        assert_eq!((game.count(), fake.ups()), (0, 2));
    }

    #[test]
    fn everything_is_let_go_when_the_game_leaves_the_front_or_the_plugin_falls_silent() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        let now = Instant::now();
        game.down("a", &keys(), Timing::Follow, &fake, now).expect("sent");
        fake.front.store(false, Ordering::SeqCst);
        game.watch(&fake, Some(Duration::ZERO), now);
        assert_eq!(game.count(), 0, "the game left the front");
        fake.front.store(true, Ordering::SeqCst);
        game.down("b", &keys(), Timing::Follow, &fake, now).expect("sent");
        game.watch(&fake, Some(SILENCE - Duration::from_millis(1)), now);
        assert_eq!(game.count(), 1);
        game.watch(&fake, Some(SILENCE), now);
        assert_eq!(game.count(), 0, "the plugin fell silent");
        game.down("c", &keys(), Timing::Follow, &fake, now).expect("sent");
        game.watch(&fake, None, now);
        assert_eq!(game.count(), 0, "it never asked");
    }

    #[test]
    fn a_press_the_system_refused_lets_go_of_what_went_down() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        fake.fail.store(true, Ordering::SeqCst);
        assert_eq!(game.down("a", &keys(), Timing::Follow, &fake, Instant::now()), Err(NotSent::Failed));
        assert_eq!((game.count(), fake.ups()), (0, 2));
    }

    /// The times the keys went down and up, after `start`, in milliseconds: one entry per press of the main key
    /// (the last of the keys of an action).
    fn moments(fake: &Fake, times: &[(Instant, usize)], start: Instant) -> Vec<(u128, bool)> {
        let sent = fake.sent.lock().unwrap().clone();
        let main = keys().last().expect("a key").scan;
        let mut out = Vec::new();
        let mut seen = 0;
        for (at, upto) in times {
            for event in &sent[seen..*upto] {
                if event.key.scan == main {
                    out.push((at.duration_since(start).as_millis(), event.up));
                }
            }
            seen = *upto;
        }
        out
    }

    /// Plays a deck key down at 0 and up at `up_ms`, the watch running every 20 ms up to two seconds; gives when the
    /// main key went down and up.
    fn played(timing: Timing, up_ms: u64) -> Vec<(u128, bool)> {
        let (fake, game) = (Fake::new(), GameKeys::default());
        let start = Instant::now();
        let mut times = Vec::new();
        game.down("a", &keys(), timing, &fake, start).expect("sent");
        times.push((start, fake.sent.lock().unwrap().len()));
        let mut t = 0;
        while t <= 2000 {
            let now = start + Duration::from_millis(t);
            if t == up_ms {
                game.up("a", &fake, now);
            }
            game.watch(&fake, Some(Duration::ZERO), now);
            times.push((now, fake.sent.lock().unwrap().len()));
            t += WATCH_EVERY.as_millis() as u64;
        }
        assert_eq!(game.count(), 0, "{timing:?}: nothing left held");
        moments(&fake, &times, start)
    }

    #[test]
    fn a_key_the_game_takes_as_it_goes_follows_the_deck() {
        assert_eq!(played(Timing::Follow, 60), vec![(0, false), (60, true)]);
        assert_eq!(played(Timing::Follow, 800), vec![(0, false), (800, true)]);
    }

    #[test]
    fn a_key_the_game_takes_only_held_is_held_past_its_threshold_however_short_the_tap() {
        // delayed_press: 0.25 s; the deck tapped for 60 ms.
        assert_eq!(played(Timing::AtLeast(250), 60), vec![(0, false), (300, true)]);
        // delayed_press_medium (self destruct): 0.5 s.
        assert_eq!(played(Timing::AtLeast(500), 40), vec![(0, false), (560, true)]);
        // Held longer on the deck: it follows the deck.
        assert_eq!(played(Timing::AtLeast(250), 900), vec![(0, false), (900, true)]);
    }

    #[test]
    fn a_key_the_game_takes_only_tapped_comes_up_before_its_limit_however_long_the_press() {
        // tap: 0.25 s; the deck held for a second: up at 200 ms, the up of the deck sends nothing.
        assert_eq!(played(Timing::Within(250), 1000), vec![(0, false), (200, true)]);
        // tap_quicker: 0.15 s.
        assert_eq!(played(Timing::Within(150), 1000), vec![(0, false), (100, true)]);
        // A short tap: it follows the deck, but stays down at least the shortest time.
        assert_eq!(played(Timing::Within(250), 60), vec![(0, false), (60, true)]);
        assert_eq!(played(Timing::Within(250), 0), vec![(0, false), (40, true)]);
    }

    #[test]
    fn a_key_the_game_takes_only_tapped_twice_is_tapped_twice() {
        assert_eq!(played(Timing::Twice, 60), vec![(0, false), (40, true), (100, false), (140, true)]);
        assert_eq!(played(Timing::Twice, 1000), vec![(0, false), (40, true), (100, false), (140, true)]);
    }

    #[test]
    fn at_the_end_everything_held_comes_up() {
        let (fake, game) = (Fake::new(), GameKeys::default());
        game.down("a", &keys(), Timing::Follow, &fake, Instant::now()).expect("sent");
        game.down("b", &crate::anti_afk_key::parse("f").expect("f"), Timing::Follow, &fake, Instant::now()).expect("sent");
        game.release_all(&fake);
        assert_eq!((game.count(), fake.ups()), (0, 3));
    }
}
