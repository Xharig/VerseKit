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

//! Anti-AFK (`anti_afk.py` of 4.12): presses the key of "wipe visor" once every five minutes so the game does
//! not throw the player out for being idle. Windows only; off after every start, **never stored**. The key is
//! the keyboard binding of the player ([`crate::anti_afk_key`]); Left Alt + X where it cannot be told.
//!
//! It presses only while Star Citizen is the foreground window and the player himself has used neither
//! keyboard, mouse, joystick, HOTAS nor gamepad for that long. The keys go in with `SendInput` as scan codes:
//! no hook, no extra package, no process of its own.
//!
//! ⚠ **The wait is not measured with `GetLastInputInfo`.** Windows counts every report of a game controller as
//! input of the user, and a HOTAS at rest sends two a second: the last input is never older than a second, and
//! the keys would never be pressed. While the switch is on, the Raw Input listener of the process
//! ([`crate::stick_input`]) tells when keyboard or mouse were really used, and keeps the latest state of every
//! stick; a thread of its own compares two looks at the sticks every few seconds and notes only *whether*
//! something moved, nothing of what. A report without change is no movement.
//!
//! Whether the keys arrived: the listener sees the keys anti-AFK sends (they carry
//! [`crate::stick_input::OWN_MARK`]) and counts them apart from the player's. Keys that Windows does not let
//! through (the game runs as administrator, this program does not) never reach the input of the system and so
//! never come back. Where the keyboard of the process does not go to the listener, the old hint is all that is
//! left: `GetLastInputInfo` must drop after the press ([`ARRIVED_WITHIN`]).
//!
//! ⛔ **No winmm for the sticks.** `joyGetPosEx` runs DirectInput in the process; for empty slots it searches
//! the devices again, and in that path it can overrun its own heap block (`0xc0000374`, the process ends).
//! Asked every three seconds on all sixteen slots, that path is taken all the time. Not `joyGetDevCapsW`
//! alone goes that way.
//!
//! The logic ([`AntiAfk`]) knows no system: it asks a [`Platform`], so every path is tested with a stand-in
//! that sends nothing. [`Runner`] puts the logic, the system and the joystick thread together; it lives in the
//! process of the overlay, which also keeps the beat ([`TICK`]).
//!
//! ⚠ In 4.12 everything is one process; here the overlay and the main window are two. The overlay writes what
//! the main window needs for the switch and the error report into the data folder after every change
//! ([`Snapshot`], `anti-afk.json`) and writes it fresh at every start, so the file never says "on" for a run
//! that is over. The main window asks the overlay to switch with a signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::anti_afk_key::{self, Chord, Key, Origin};
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;
use crate::stick_input::{Source, StickStates};

/// The file name of the program of the game, in lower case.
pub const GAME_EXE: &str = "starcitizen.exe";
/// Seconds without any input until the keys are pressed.
pub const INTERVAL: f64 = 300.0;
/// The setting with the seconds without input until the keys are pressed; without it [`INTERVAL`].
pub const SETTING_INTERVAL: &str = "anti_afk_sekunden";
/// The seconds the setting may hold: the shortest fits two beats and a look at the sticks.
pub const INTERVAL_RANGE: std::ops::RangeInclusive<i64> = 15..=600;

/// The seconds without input until the keys are pressed, as the player set them.
pub fn interval_for(paths: &Paths) -> f64 {
    paths.setting_int(SETTING_INTERVAL, INTERVAL as i64, Some(*INTERVAL_RANGE.start()), Some(*INTERVAL_RANGE.end()))
        as f64
}
/// The setting whether keys without a device (sent by a program: macro software of a HOTAS, a remote desktop)
/// count as input of the player; without it they do.
pub const SETTING_INJECTED: &str = "anti_afk_makro_tasten";

/// Do keys sent by a program count as input of the player, as the player set it?
pub fn injected_count(paths: &Paths) -> bool {
    paths.setting_bool(SETTING_INJECTED, true)
}
/// The beat in which the overlay calls [`Runner::tick`].
pub const TICK: Duration = Duration::from_secs(5);
/// Seconds between pressing and letting go.
pub const HOLD: Duration = Duration::from_millis(80);
/// Seconds between two looks at the joysticks.
pub const JOY_POLL: Duration = Duration::from_secs(3);
/// Where the keyboard of the process does not go to the listener: after sending, the last input Windows knows
/// (`GetLastInputInfo`) must be younger than this (seconds); else the key did not arrive.
pub const ARRIVED_WITHIN: f64 = 2.0;
/// How long after pressing the own keys are waited for at the listener.
pub const ARRIVAL_WAIT: Duration = Duration::from_secs(1);
/// An axis counts as moved from this change on, on the scale 0..65535 every axis is brought to
/// ([`crate::stick_input::normalize`]): 1.5 % of its way, the share 4.12 used on the scale of winmm. Being a
/// share, it is the same for 8, 10, 12 and 16 bits. At rest an axis shakes by a few steps of its resolution;
/// one step is 257 on an 8-bit axis, 64 on a 10-bit one, 16 on a 12-bit one, so three steps of shaking stay
/// below it on every resolution, while a hand on the stick moves an axis by far more within three seconds.
pub const MOVE_THRESHOLD: u32 = 1000;
/// The file with what the main window needs to know, in the data folder.
pub const FILE: &str = "anti-afk.json";
/// The texts of the two reasons for a stop.
pub const ERR_ADMIN: &str = "aafk_err_admin";
pub const ERR_SEND: &str = "aafk_err_send";

/// A look at all sticks: device → its latest state.
pub type JoyStates = StickStates;

/// Can this system do it at all (Windows)? Nothing is asked of the system for the answer.
pub const fn supported() -> bool {
    cfg!(windows)
}

/// The signal with which the main window asks the overlay to switch (`true` = on).
pub fn signal_for(on: bool) -> crate::window_signal::Signal {
    if on { crate::window_signal::Signal::AntiAfkOn } else { crate::window_signal::Signal::AntiAfkOff }
}

/// What a signal asks of anti-AFK: `Some(true)` = switch on, `Some(false)` = off, `None` = a signal that is
/// none of its business.
pub fn wish_of(signal: crate::window_signal::Signal) -> Option<bool> {
    match signal {
        crate::window_signal::Signal::AntiAfkOn => Some(true),
        crate::window_signal::Signal::AntiAfkOff => Some(false),
        _ => None,
    }
}

/// Did something move between two looks? Buttons and hats count with every change, axes from
/// [`MOVE_THRESHOLD`] on. A stick that was plugged in (or sent its first report) between counts for nothing.
pub fn joystick_moved(before: &JoyStates, after: &JoyStates) -> bool {
    after.iter().any(|(device, now)| before.get(device).is_some_and(|old| now.moved_from(old, MOVE_THRESHOLD)))
}

/// What happened at one beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The switch is off.
    Off,
    /// This system cannot do it: the switch went off.
    Unsupported,
    /// The game is not the foreground window.
    NotInFront,
    /// The input of the player cannot be read (no listener): nothing is pressed.
    Blind,
    /// The player used something not long ago.
    Active,
    /// Pressed already within this interval.
    TooSoon,
    /// Windows does not let the key through (the game runs as administrator, this program does not): off.
    Blocked,
    /// The key could not be sent: off.
    Failed,
    /// Pressed.
    Sent,
}

/// Why it does not press right now, as the page shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Star Citizen is not the foreground window.
    NotInFront,
    /// The player used something within the wait.
    Waiting,
    /// It pressed within the wait.
    Pressed,
    /// The input of the player cannot be read.
    Blind,
}

impl Reason {
    /// The word in the file.
    pub fn key(self) -> &'static str {
        match self {
            Reason::NotInFront => "nicht_vorne",
            Reason::Waiting => "wartet",
            Reason::Pressed => "gedrueckt",
            Reason::Blind => "blind",
        }
    }

    pub fn from_key(text: &str) -> Option<Reason> {
        [Reason::NotInFront, Reason::Waiting, Reason::Pressed, Reason::Blind]
            .into_iter()
            .find(|reason| reason.key() == text)
    }
}

/// How long the button stands as "just pressed" after a press (seconds).
pub const FLASH: f64 = 1.5;

/// The state the button of the overlay and the switch of the page show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Off.
    Off,
    /// On, waiting for the player to be idle long enough.
    Waiting,
    /// On, pressed within [`FLASH`].
    Pressed,
    /// Off by a stop that holds until the next switching on (the key did not arrive or could not be sent).
    Stopped,
    /// On, but the input of the player cannot be read: it does not press.
    Blind,
}

impl Look {
    /// The state from the switch, the stop that holds, the reason of the last beat and the seconds since the
    /// last press.
    pub fn of(on: bool, stopped: bool, reason: Option<Reason>, since_press: Option<f64>) -> Look {
        match (on, reason) {
            (false, _) if stopped => Look::Stopped,
            (false, _) => Look::Off,
            (true, Some(Reason::Blind)) => Look::Blind,
            (true, _) if since_press.is_some_and(|since| (0.0..FLASH).contains(&since)) => Look::Pressed,
            (true, _) => Look::Waiting,
        }
    }

    /// The text key of the hover hint: the state in words and what a click does.
    pub fn hint_key(self) -> &'static str {
        match self {
            Look::Off => "hinweis_aafk_aus",
            Look::Waiting => "hinweis_aafk_an",
            Look::Pressed => "hinweis_aafk_gedrueckt",
            Look::Stopped => "hinweis_aafk_gestoppt",
            Look::Blind => "hinweis_aafk_blind",
        }
    }

    /// Does the button stand as switched on?
    pub fn on(self) -> bool {
        matches!(self, Look::Waiting | Look::Pressed | Look::Blind)
    }
}

/// A key event could not be sent.
#[derive(Debug)]
pub struct SendError(pub String);

/// A key event of [`press_with`]: the key, and down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub up: bool,
}

/// The events of one press: down in the order of `keys` (the modifiers first), up in the reverse order.
pub fn key_events(keys: &[Key], up: bool) -> Vec<KeyEvent> {
    let ordered: Vec<Key> = if up { keys.iter().rev().copied().collect() } else { keys.to_vec() };
    ordered.into_iter().map(|key| KeyEvent { key, up }).collect()
}

/// Presses the keys one after the other, holds for `hold`, lets go backwards. Letting go also happens when the
/// pressing failed: a hanging Alt key would stay in the system.
///
/// # Errors
///
/// The first [`SendError`] of `send`.
pub fn press_with(
    keys: &[Key],
    hold: Duration,
    send: &mut dyn FnMut(&[KeyEvent]) -> Result<(), SendError>,
) -> Result<(), SendError> {
    let down = send(&key_events(keys, false));
    if down.is_ok() {
        std::thread::sleep(hold);
    }
    let up = send(&key_events(keys, true));
    down.and(up)
}

/// What the system answers (and does).
pub trait Platform: Send + Sync {
    /// Can this system do it at all (Windows)?
    fn supported(&self) -> bool;
    /// Is the foreground window one of `StarCitizen.exe`?
    fn game_in_front(&self) -> bool;
    /// Seconds since the last input Windows knows (`GetLastInputInfo`); `None` where that cannot be told. Every
    /// report of a game controller counts there, so it is no measure of the player: only a hint whether a key
    /// arrived, where the listener cannot tell it.
    fn idle_seconds(&self) -> Option<f64>;
    /// Does the game in front run elevated while this program does not (Windows then drops every key without
    /// a word)? `None` where that cannot be told.
    fn game_needs_admin(&self) -> Option<bool>;
    /// Presses the keys, the modifiers first.
    ///
    /// # Errors
    ///
    /// [`SendError`] where the system took not all events.
    fn press(&self, keys: &[Key]) -> Result<(), SendError>;
    /// Starts following keyboard, mouse and sticks (while the switch is on); `None` where the system cannot.
    /// Dropping the watch ends the following.
    fn watch_sticks(&self) -> Option<Box<dyn StickWatch>>;
}

/// Keyboard, mouse and sticks followed while the switch is on ([`Platform::watch_sticks`]).
pub trait StickWatch: Send + Sync {
    /// The latest state of every stick; empty while none sent anything.
    fn states(&self) -> JoyStates;
    /// Is the following still running?
    fn running(&self) -> bool;
    /// Seconds since the last real input of keyboard or mouse, and which; without one since the following
    /// started, the seconds since then and no source. Keys sent by a program count only with `count_injected`.
    fn input_idle(&self, count_injected: bool) -> (f64, Option<Source>);
    /// How many of the keys anti-AFK sent came back so far.
    fn own_keys(&self) -> u32;
    /// Do the keys of this process go to the following (so the keys anti-AFK sends must come back)?
    fn keyboard_ours(&self) -> bool;
}

/// The state and the rules, without a system. (No `Debug`: it holds the last stand of the sticks.)
pub struct AntiAfk {
    on: bool,
    /// Seconds without input until the keys are pressed.
    interval: f64,
    threshold: f64,
    last_sent: Option<f64>,
    count: u32,
    /// The text key of the stop that holds until the next switching on.
    error: Option<&'static str>,
    /// The last stop since the start of the program: (text key, `YYYY-MM-DD HH:MM`).
    failure: Option<(&'static str, String)>,
    /// When a stick moved last (monotonic seconds).
    joy_last: Option<f64>,
    joy_before: Option<JoyStates>,
    /// Why the last beat did not press (or that it pressed); `None` while off and before the first beat.
    reason: Option<Reason>,
    /// When the last real input came (monotonic seconds) and from what; without a source, since when nothing
    /// came.
    input_at: Option<(f64, Option<Source>)>,
    /// The keys a press sends.
    chord: Chord,
    /// Do keys sent by a program count as input of the player?
    count_injected: bool,
    /// Since when the switch is on (monotonic seconds); `None` while off.
    on_since: Option<f64>,
}

impl Default for AntiAfk {
    fn default() -> Self {
        AntiAfk {
            on: false,
            interval: INTERVAL,
            threshold: INTERVAL,
            last_sent: None,
            count: 0,
            error: None,
            failure: None,
            joy_last: None,
            joy_before: None,
            reason: None,
            input_at: None,
            chord: Chord::default(),
            count_injected: true,
            on_since: None,
        }
    }
}

impl AntiAfk {
    pub fn enabled(&self) -> bool {
        self.on
    }

    /// The text key of the last stop; holds until the player switches on again.
    pub fn last_error(&self) -> Option<&'static str> {
        self.error
    }

    /// The last stop since the start of the program, also when the switch is on again.
    pub fn last_failure(&self) -> Option<(&'static str, &str)> {
        self.failure.as_ref().map(|(key, when)| (*key, when.as_str()))
    }

    pub fn sent_count(&self) -> u32 {
        self.count
    }

    /// Why the last beat did not press (or that it pressed).
    pub fn reason(&self) -> Option<Reason> {
        self.reason
    }

    /// When the last real input came (monotonic seconds) and from what, as the last beat saw it.
    pub fn input_at(&self) -> Option<(f64, Option<Source>)> {
        self.input_at
    }

    /// When it pressed last (monotonic seconds).
    pub fn last_sent(&self) -> Option<f64> {
        self.last_sent
    }

    /// Takes the seconds without input until the keys are pressed; a wait already running shortens or grows
    /// with it.
    pub fn set_interval(&mut self, seconds: f64) {
        self.interval = seconds;
        self.threshold = seconds;
    }

    /// Takes the keys the next press sends.
    pub fn set_chord(&mut self, chord: Chord) {
        self.chord = chord;
    }

    /// The keys a press sends.
    pub fn chord(&self) -> &Chord {
        &self.chord
    }

    /// Takes whether keys sent by a program count as input of the player (from the next beat on).
    pub fn set_count_injected(&mut self, count: bool) {
        self.count_injected = count;
    }

    /// Since when the switch is on (monotonic seconds).
    pub fn on_since(&self) -> Option<f64> {
        self.on_since.filter(|_| self.on)
    }

    /// Switches on or off at `now` (monotonic seconds). Where the system cannot do it the switch stays off.
    /// Gives the new state. Switching on forgets the stop that held; the look at the sticks starts over.
    pub fn set_enabled_at(&mut self, on: bool, now: f64, platform: &dyn Platform) -> bool {
        let on = on && platform.supported();
        self.on = on;
        self.last_sent = None;
        self.threshold = self.interval;
        self.joy_before = None;
        self.joy_last = None;
        self.reason = None;
        self.input_at = None;
        self.on_since = on.then_some(now);
        if on {
            self.error = None;
        }
        on
    }

    /// [`AntiAfk::set_enabled_at`] at the time 0.
    pub fn set_enabled(&mut self, on: bool, platform: &dyn Platform) -> bool {
        self.set_enabled_at(on, 0.0, platform)
    }

    /// A stop: the key of the reason, the time, the switch off.
    fn fail(&mut self, key: &'static str) {
        self.error = Some(key);
        self.failure = Some((key, local_minute()));
        self.on = false;
        self.reason = None;
    }

    /// One look at the sticks (made outside the lock that guards this state); notes the time when one moved. The
    /// first look only makes the stand to compare with. Gives whether something moved.
    pub fn joy_note(&mut self, now: f64, after: JoyStates) -> bool {
        let before = self.joy_before.replace(after.clone());
        if before.as_ref().is_some_and(|before| joystick_moved(before, &after)) {
            self.joy_last = Some(now);
            return true;
        }
        false
    }

    /// Seconds since the last input of keyboard, mouse or stick, and which: `keys` is what the following says of
    /// keyboard and mouse ([`StickWatch::input_idle`]), a stick that moved more recently wins.
    pub fn idle_all(&self, now: f64, keys: (f64, Option<Source>)) -> (f64, Option<Source>) {
        match self.joy_last {
            Some(last) if now - last < keys.0 => (now - last, Some(Source::Stick)),
            _ => keys,
        }
    }

    /// One look and, when everything fits, the keys. `watch` is the following of keyboard, mouse and sticks;
    /// without it (or when it ended) the input of the player cannot be read and nothing is pressed.
    pub fn tick(&mut self, now: f64, platform: &dyn Platform, watch: Option<&dyn StickWatch>) -> Outcome {
        if !self.on {
            self.reason = None;
            return Outcome::Off;
        }
        if !platform.supported() {
            self.on = false;
            self.reason = None;
            return Outcome::Unsupported;
        }
        if !platform.game_in_front() {
            self.reason = Some(Reason::NotInFront);
            return Outcome::NotInFront;
        }
        let Some(watch) = watch.filter(|watch| watch.running()) else {
            self.reason = Some(Reason::Blind);
            return Outcome::Blind;
        };
        let (idle, source) = self.idle_all(now, watch.input_idle(self.count_injected));
        self.input_at = Some((now - idle, source));
        if idle < self.threshold {
            self.reason = Some(Reason::Waiting);
            return Outcome::Active;
        }
        if self.last_sent.is_some_and(|last| now - last < self.threshold) {
            self.reason = Some(Reason::Pressed);
            return Outcome::TooSoon;
        }
        if platform.game_needs_admin() == Some(true) {
            self.fail(ERR_ADMIN);
            return Outcome::Blocked;
        }
        let before = watch.own_keys();
        if platform.press(&self.chord.keys).is_err() {
            self.fail(ERR_SEND);
            return Outcome::Failed;
        }
        if !arrived(watch, before, platform) {
            self.fail(ERR_ADMIN);
            return Outcome::Blocked;
        }
        self.last_sent = Some(now);
        self.count += 1;
        self.threshold = self.interval;
        self.reason = Some(Reason::Pressed);
        Outcome::Sent
    }
}

/// Did the keys of the press arrive? Where the keys of the process go to the following, the own keys must come
/// back to it within [`ARRIVAL_WAIT`] (`before` is its count before the press): keys Windows drops never come
/// back. Elsewhere only the hint is left: the last input Windows knows must have dropped.
fn arrived(watch: &dyn StickWatch, before: u32, platform: &dyn Platform) -> bool {
    if watch.keyboard_ours() {
        let deadline = Instant::now() + ARRIVAL_WAIT;
        while watch.own_keys() == before {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        return true;
    }
    platform.idle_seconds().is_none_or(|after| after < ARRIVED_WITHIN)
}

/// `YYYY-MM-DD HH:MM` of now, local time.
fn local_minute() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    crate::formats::local_time(seconds)
        .map_or_else(String::new, |time| crate::formats::strftime("%Y-%m-%d %H:%M", &time))
}

// ---------------------------------------------------------------------------
// What the main window needs to know
// ---------------------------------------------------------------------------

/// What the overlay tells the main window through the data folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub on: bool,
    pub sent: u32,
    /// Is the thread that looks at the sticks running?
    pub joystick: bool,
    /// The last stop since the start of the program: (text key, time).
    pub failure: Option<(String, String)>,
    /// Why it does not press right now (or that it pressed); `None` while off and before the first beat.
    pub reason: Option<Reason>,
    /// When the last real input came (seconds since 1970); with [`Snapshot::input_source`] `None`, since when
    /// nothing came.
    pub input_at: Option<i64>,
    pub input_source: Option<Source>,
    /// When it pressed last (seconds since 1970).
    pub pressed_at: Option<i64>,
    /// The text key of the stop that holds until the next switching on.
    pub stop: Option<String>,
    /// Since when the switch is on (seconds since 1970).
    pub on_since: Option<i64>,
    /// The keys a press sends (the input as the game writes it, `lalt+k`) and where they come from.
    pub key: Option<(String, Origin)>,
    /// How many sticks the following saw since the switching on.
    pub sticks: u32,
}

impl Snapshot {
    /// The state the switch of the page shows at `wall` (seconds since 1970).
    pub fn look(&self, wall: i64) -> Look {
        let since = self.pressed_at.map(|at| (wall - at) as f64);
        Look::of(self.on, self.stop.is_some(), self.reason, since)
    }
}

impl Snapshot {
    fn to_value(&self) -> Value {
        let mut map = Map::new();
        let number = |n: Option<i64>| n.map_or(Value::Null, Value::int);
        jsonio::map_set(&mut map, "an", Value::Bool(self.on));
        jsonio::map_set(&mut map, "gesendet", Value::int(i64::from(self.sent)));
        jsonio::map_set(&mut map, "joystick", Value::Bool(self.joystick));
        let failure = self
            .failure
            .as_ref()
            .map_or(Value::Null, |(key, when)| Value::Array(vec![Value::str(key.as_str()), Value::str(when.as_str())]));
        jsonio::map_set(&mut map, "abbruch", failure);
        jsonio::map_set(&mut map, "grund", self.reason.map_or(Value::Null, |reason| Value::str(reason.key())));
        jsonio::map_set(&mut map, "eingabe_um", number(self.input_at));
        jsonio::map_set(&mut map, "eingabe_art", self.input_source.map_or(Value::Null, |s| Value::str(s.key())));
        jsonio::map_set(&mut map, "gedrueckt_um", number(self.pressed_at));
        jsonio::map_set(&mut map, "stopp", self.stop.as_deref().map_or(Value::Null, Value::str));
        jsonio::map_set(&mut map, "an_seit", number(self.on_since));
        let (key, origin) = match &self.key {
            Some((input, origin)) => (Value::str(input.as_str()), Value::str(origin.key())),
            None => (Value::Null, Value::Null),
        };
        jsonio::map_set(&mut map, "taste", key);
        jsonio::map_set(&mut map, "taste_herkunft", origin);
        jsonio::map_set(&mut map, "sticks", Value::int(i64::from(self.sticks)));
        Value::Object(map)
    }

    fn from_value(value: &Value) -> Snapshot {
        let field = |name: &str| value.get(name);
        let failure = field("abbruch").and_then(|v| match v {
            Value::Array(items) => match (items.first(), items.get(1)) {
                (Some(Value::Str(key)), Some(Value::Str(when))) => Some((key.clone(), when.clone())),
                _ => None,
            },
            _ => None,
        });
        let number = |name: &str| match field(name) {
            Some(Value::Int(n)) => n.parse::<i64>().ok(),
            _ => None,
        };
        let word = |name: &str| match field(name) {
            Some(Value::Str(text)) => Some(text.clone()),
            _ => None,
        };
        Snapshot {
            on: matches!(field("an"), Some(Value::Bool(true))),
            sent: match field("gesendet") {
                Some(Value::Int(n)) => n.parse().unwrap_or(0),
                _ => 0,
            },
            joystick: matches!(field("joystick"), Some(Value::Bool(true))),
            failure,
            reason: word("grund").as_deref().and_then(Reason::from_key),
            input_at: number("eingabe_um"),
            input_source: word("eingabe_art").as_deref().and_then(Source::from_key),
            pressed_at: number("gedrueckt_um"),
            stop: word("stopp"),
            on_since: number("an_seit"),
            key: word("taste").zip(word("taste_herkunft").as_deref().and_then(Origin::from_key)),
            sticks: number("sticks").and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
        }
    }

    /// What the overlay wrote last; "off, nothing sent" where there is no file (no overlay of this data runs).
    pub fn read(paths: &Paths) -> Snapshot {
        jsonio::read_json(&paths.app_file(FILE)).ok().map(|value| Snapshot::from_value(&value)).unwrap_or_default()
    }

    /// Writes the snapshot into the data folder.
    ///
    /// # Errors
    ///
    /// Where the file cannot be written (a folder that is locked or full).
    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        let text = jsonio::dumps(&self.to_value(), Some(1), false);
        jsonio::write_text_atomic(&paths.app_file(FILE), &text, jsonio::Newline::Native)
    }
}

// ---------------------------------------------------------------------------
// The overlay's end: logic, system, thread and snapshot together
// ---------------------------------------------------------------------------

/// A thread that does one thing every so often, as long as the switch is on.
struct Worker {
    /// Dropping the sender ends the thread at its next wait.
    stop: Sender<()>,
    handle: std::thread::JoinHandle<()>,
}

impl Worker {
    fn running(&self) -> bool {
        !self.handle.is_finished()
    }
}

/// The two intervals of a run.
#[derive(Clone, Copy)]
struct Timing {
    /// Between two beats (looking and pressing).
    beat: Duration,
    /// Between two looks at the sticks.
    look: Duration,
}

struct Shared {
    logic: Mutex<AntiAfk>,
    /// Mirror of the switch for whoever asks every frame: never waits for a beat that is pressing.
    on: AtomicBool,
    platform: Arc<dyn Platform>,
    /// The following of the sticks while the switch is on.
    sticks: Mutex<Option<Box<dyn StickWatch>>>,
    /// The look at the sticks, in a thread of its own.
    joy: Mutex<Option<Worker>>,
    /// The beat, in a thread of its own: it does not depend on the frames of a window.
    beat: Mutex<Option<Worker>>,
    paths: Option<Paths>,
    start: Instant,
    /// The seconds since 1970 at `start`: the times of the snapshot are counted from it.
    wall_start: f64,
    timing: Timing,
    /// What was written last; a beat writes only what changed.
    published: Mutex<Option<Snapshot>>,
    /// Called when the run stopped by itself or pressed: the window shows the reason or the press.
    waker: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// The reason of a stop nobody took yet.
    stopped: Mutex<Option<&'static str>>,
    /// The snapshot could not be written: it is tried again at the next beat.
    unpublished: AtomicBool,
    /// The keys of the binding, read again when the `actionmaps.xml` changes.
    chord: Mutex<anti_afk_key::Cached>,
}

/// Switch, beat and the look at the sticks of one run of the overlay.
#[derive(Clone)]
pub struct Runner {
    shared: Arc<Shared>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Runner {
    /// On this system. With `paths` the state is written into the data folder after every change, and now: the
    /// file of an earlier run says "off" from this moment on.
    pub fn system(paths: Option<Paths>) -> Runner {
        Runner::with_platform(Arc::new(System), paths)
    }

    /// With a stand-in for the system (tests).
    pub fn with_platform(platform: Arc<dyn Platform>, paths: Option<Paths>) -> Runner {
        Runner::with_timing(platform, paths, TICK, JOY_POLL)
    }

    /// With other intervals (tests, which cannot wait five seconds for a beat).
    pub fn with_timing(platform: Arc<dyn Platform>, paths: Option<Paths>, beat: Duration, look: Duration) -> Runner {
        let runner = Runner {
            shared: Arc::new(Shared {
                logic: Mutex::new(AntiAfk::default()),
                on: AtomicBool::new(false),
                platform,
                sticks: Mutex::new(None),
                joy: Mutex::new(None),
                beat: Mutex::new(None),
                paths,
                start: Instant::now(),
                wall_start: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |since| since.as_secs_f64()),
                timing: Timing { beat, look },
                published: Mutex::new(None),
                waker: Mutex::new(None),
                stopped: Mutex::new(None),
                unpublished: AtomicBool::new(false),
                chord: Mutex::new(anti_afk_key::Cached::default()),
            }),
        };
        runner.publish();
        runner
    }

    fn now(&self) -> f64 {
        self.shared.start.elapsed().as_secs_f64()
    }

    /// Can this system do it?
    pub fn supported(&self) -> bool {
        self.shared.platform.supported()
    }

    /// Is the switch on? Never waits: a window may ask at every frame.
    pub fn enabled(&self) -> bool {
        self.shared.on.load(Ordering::Relaxed)
    }

    /// The text key of the stop that holds until the next switching on.
    pub fn last_error(&self) -> Option<&'static str> {
        locked(&self.shared.logic).last_error()
    }

    pub fn sent_count(&self) -> u32 {
        locked(&self.shared.logic).sent_count()
    }

    /// Are the sticks followed and looked at?
    pub fn joy_running(&self) -> bool {
        locked(&self.shared.joy).as_ref().is_some_and(Worker::running)
            && locked(&self.shared.sticks).as_ref().is_some_and(|watch| watch.running())
    }

    /// Is the thread of the beat running?
    pub fn beat_running(&self) -> bool {
        locked(&self.shared.beat).as_ref().is_some_and(Worker::running)
    }

    /// What the main window learns. The times are whole seconds since 1970, counted from the start of the
    /// runner, so the same moment gives the same number at every beat.
    pub fn snapshot(&self) -> Snapshot {
        let joystick = self.joy_running();
        let sticks = locked(&self.shared.sticks)
            .as_ref()
            .map_or(0, |watch| u32::try_from(watch.states().len()).unwrap_or(u32::MAX));
        let logic = locked(&self.shared.logic);
        let wall = |at: f64| (self.shared.wall_start + at).floor() as i64;
        let on = logic.enabled();
        let input = logic.input_at().filter(|_| on);
        let chord = logic.chord();
        Snapshot {
            on,
            sent: logic.sent_count(),
            joystick,
            failure: logic.last_failure().map(|(key, when)| (key.to_string(), when.to_string())),
            reason: logic.reason().filter(|_| on),
            input_at: input.map(|(at, _)| wall(at)),
            input_source: input.and_then(|(_, source)| source),
            pressed_at: logic.last_sent().map(wall),
            stop: logic.last_error().map(str::to_string),
            on_since: logic.on_since().map(wall),
            key: on.then(|| (chord.input.clone(), chord.origin)),
            sticks,
        }
    }

    /// The keys of the binding of the game of the paths (Left Alt + X without paths).
    fn current_chord(&self) -> Chord {
        self.shared.paths.as_ref().map_or_else(Chord::default, |paths| locked(&self.shared.chord).get(paths))
    }

    /// The state the button shows now.
    pub fn state(&self) -> Look {
        let now = self.now();
        let logic = locked(&self.shared.logic);
        Look::of(logic.enabled(), logic.last_error().is_some(), logic.reason(), logic.last_sent().map(|at| now - at))
    }

    /// Seconds until the button stops standing as "just pressed"; `None` when it does not.
    pub fn flash_left(&self) -> Option<f64> {
        let now = self.now();
        let logic = locked(&self.shared.logic);
        let left = FLASH - (now - logic.last_sent()?);
        (logic.enabled() && left > 0.0).then_some(left)
    }

    /// Writes the snapshot. A write that fails is recorded and tried again at the next beat: the page must not
    /// believe a file that says "off" while the keys are pressed.
    fn publish(&self) {
        let Some(paths) = &self.shared.paths else { return };
        let snapshot = self.snapshot();
        match snapshot.write(paths) {
            Ok(()) => {
                self.shared.unpublished.store(false, Ordering::Relaxed);
                *locked(&self.shared.published) = Some(snapshot);
            }
            Err(error) => {
                crate::errors::record("anti_afk.snapshot", &error);
                self.shared.unpublished.store(true, Ordering::Relaxed);
                *locked(&self.shared.published) = None;
            }
        }
    }

    /// Writes the snapshot when it says something else than the file (or the last write failed).
    fn publish_changed(&self) {
        if self.shared.paths.is_none() {
            return;
        }
        let changed = locked(&self.shared.published).as_ref() != Some(&self.snapshot());
        if changed || self.shared.unpublished.load(Ordering::Relaxed) {
            self.publish();
        }
    }

    /// Calls `wake` whenever the run stops by itself ([`Runner::take_stop`] then has the reason) and whenever
    /// it pressed (the button shows it).
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        *locked(&self.shared.waker) = Some(Box::new(wake));
    }

    /// The reason of the stop that happened since the last call, if one did (the text key).
    pub fn take_stop(&self) -> Option<&'static str> {
        locked(&self.shared.stopped).take()
    }

    /// Switches on or off: the one way for the page, the button of the overlay and the signal. Gives the new
    /// state.
    pub fn set_enabled(&self, on: bool) -> bool {
        let chord = self.current_chord();
        let now = self.now();
        let state = {
            let mut logic = locked(&self.shared.logic);
            logic.set_chord(chord);
            logic.set_enabled_at(on, now, self.shared.platform.as_ref())
        };
        self.shared.on.store(state, Ordering::Relaxed);
        if state {
            // A reason nobody took belongs to the run that was.
            locked(&self.shared.stopped).take();
        }
        self.stop_workers();
        if state {
            self.start_workers();
        }
        self.publish();
        state
    }

    /// One beat: looks and presses when everything fits. A stop (the switch goes off) is published and told.
    pub fn tick(&self) -> Outcome {
        let now = self.now();
        // The settings and the binding are read at every beat: a change counts from the next beat on.
        let interval = self.shared.paths.as_ref().map_or(INTERVAL, interval_for);
        let count_injected = self.shared.paths.as_ref().is_none_or(injected_count);
        let chord = self.current_chord();
        let outcome = {
            let mut logic = locked(&self.shared.logic);
            logic.set_interval(interval);
            logic.set_count_injected(count_injected);
            logic.set_chord(chord);
            let watch = locked(&self.shared.sticks);
            let outcome = logic.tick(now, self.shared.platform.as_ref(), watch.as_deref());
            drop(watch);
            // A stop: the reason is there before the switch shows "off", so whoever sees "off" (a window
            // that asks every frame, a test that waits for it) finds the reason when it asks for it.
            if matches!(outcome, Outcome::Blocked | Outcome::Failed)
                && let Some(reason) = logic.last_error()
            {
                *locked(&self.shared.stopped) = Some(reason);
            }
            #[cfg(test)]
            std::thread::sleep(Duration::from_millis(u64::from(tests::STOP_GAP_MS.load(Ordering::SeqCst))));
            self.shared.on.store(logic.enabled(), Ordering::Relaxed);
            outcome
        };
        match outcome {
            Outcome::Blocked | Outcome::Failed | Outcome::Unsupported => {
                self.stop_workers();
                self.publish();
                if let Some(wake) = &*locked(&self.shared.waker) {
                    wake();
                }
            }
            // A press: the window shows it at once.
            Outcome::Sent => {
                self.publish_changed();
                if let Some(wake) = &*locked(&self.shared.waker) {
                    wake();
                }
            }
            // The reason and its times are new with every beat that saw something else.
            _ => self.publish_changed(),
        }
        outcome
    }

    /// One look at the sticks: the latest states are taken outside the lock of the logic, the comparison is made
    /// inside.
    fn look(&self) {
        let Some(after) = locked(&self.shared.sticks).as_ref().map(|watch| watch.states()) else { return };
        let now = self.now();
        locked(&self.shared.logic).joy_note(now, after);
    }

    /// A thread that calls `work` every `every` until its sender is dropped or the runner is gone. It holds the
    /// runner only while it works, so a runner that is dropped ends it too.
    fn spawn(&self, name: &str, every: Duration, work: fn(&Runner)) -> Option<Worker> {
        let (stop, wait) = mpsc::channel::<()>();
        let shared = Arc::downgrade(&self.shared);
        let handle = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                // A dropped sender ends the wait at once, a timeout is the next round.
                while matches!(wait.recv_timeout(every), Err(RecvTimeoutError::Timeout)) {
                    let Some(shared) = shared.upgrade() else { break };
                    work(&Runner { shared });
                }
            })
            .ok()?;
        Some(Worker { stop, handle })
    }

    fn start_workers(&self) {
        let timing = self.shared.timing;
        let watch = self.shared.platform.watch_sticks();
        let following = watch.is_some();
        *locked(&self.shared.sticks) = watch;
        if following {
            *locked(&self.shared.joy) = self.spawn("anti-afk-joystick", timing.look, Runner::look);
        }
        *locked(&self.shared.beat) = self.spawn("anti-afk-takt", timing.beat, |runner| {
            runner.tick();
        });
    }

    /// Stops the threads without waiting for them (they end at their next wait) and ends the following of the
    /// sticks.
    fn stop_workers(&self) {
        for slot in [&self.shared.joy, &self.shared.beat] {
            if let Some(Worker { stop, handle }) = locked(slot).take() {
                drop(stop);
                drop(handle);
            }
        }
        let watch = locked(&self.shared.sticks).take();
        drop(watch);
    }
}

// ---------------------------------------------------------------------------
// The system
// ---------------------------------------------------------------------------

/// The real system: Windows does it all, elsewhere nothing is supported.
pub struct System;

#[cfg(not(windows))]
impl Platform for System {
    fn supported(&self) -> bool {
        false
    }
    fn game_in_front(&self) -> bool {
        false
    }
    fn idle_seconds(&self) -> Option<f64> {
        None
    }
    fn game_needs_admin(&self) -> Option<bool> {
        None
    }
    fn press(&self, _keys: &[Key]) -> Result<(), SendError> {
        Err(SendError("no api".into()))
    }
    fn watch_sticks(&self) -> Option<Box<dyn StickWatch>> {
        None
    }
}

#[cfg(windows)]
mod windows_system {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetLastInputInfo, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        KEYEVENTF_SCANCODE, LASTINPUTINFO, SendInput,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    use super::{GAME_EXE, HOLD, JoyStates, Key, KeyEvent, Platform, SendError, StickWatch, System, press_with};
    use crate::stick_input::{Listener, OWN_MARK, Source};

    impl StickWatch for Listener {
        fn states(&self) -> JoyStates {
            Listener::states(self)
        }
        fn running(&self) -> bool {
            Listener::running(self)
        }
        fn input_idle(&self, count_injected: bool) -> (f64, Option<Source>) {
            Listener::input_idle(self, count_injected)
        }
        fn own_keys(&self) -> u32 {
            Listener::own_keys(self)
        }
        fn keyboard_ours(&self) -> bool {
            Listener::keyboard_ours(self)
        }
    }

    /// The process number of the foreground window; 0 where there is none.
    fn foreground_pid() -> u32 {
        // SAFETY: plain calls without arguments the system owns; a handle of 0 is "no window".
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            return 0;
        }
        let mut pid = 0u32;
        // SAFETY: `pid` lives during the call.
        unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
        pid
    }

    /// The file name of the program behind the process, in lower case; empty where it cannot be read.
    fn process_name(pid: u32) -> String {
        if pid == 0 {
            return String::new();
        }
        // SAFETY: asks for a handle with the least rights; closed below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return String::new();
        }
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer is as large as `size` says and lives during the call.
        let read = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &raw mut size) } != 0;
        // SAFETY: the handle was opened above.
        unsafe { CloseHandle(handle) };
        if !read {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]).replace('/', "\\");
        path.rsplit('\\').next().unwrap_or_default().to_lowercase()
    }

    /// Does the process behind `handle` run with administrator rights? `None` where that is unclear.
    fn elevated(handle: HANDLE) -> Option<bool> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: `token` lives during the call; closed below.
        if unsafe { OpenProcessToken(handle, TOKEN_QUERY, &raw mut token) } == 0 {
            return None;
        }
        let mut value = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = 0u32;
        // SAFETY: the buffer is as large as stated and lives during the call.
        let asked = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                (&raw mut value).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &raw mut size,
            )
        } != 0;
        // SAFETY: the token was opened above.
        unsafe { CloseHandle(token) };
        asked.then_some(value.TokenIsElevated != 0)
    }

    /// One key event, marked as anti-AFK's own: Raw Input hands the mark back, so the listener does not take it
    /// for the player's input.
    fn keyboard_event(event: KeyEvent) -> INPUT {
        let flags = KEYEVENTF_SCANCODE
            | if event.up { KEYEVENTF_KEYUP } else { 0 }
            | if event.key.extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: event.key.scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: OWN_MARK as usize,
                },
            },
        }
    }

    fn send(events: &[KeyEvent]) -> Result<(), SendError> {
        let inputs: Vec<INPUT> = events.iter().copied().map(keyboard_event).collect();
        // SAFETY: the array lives during the call and the size is that of one element.
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            return Ok(());
        }
        // SAFETY: reads the last error of this thread.
        let error = unsafe { GetLastError() };
        Err(SendError(format!("SendInput {sent}/{}, error {error}", inputs.len())))
    }

    impl Platform for System {
        fn supported(&self) -> bool {
            true
        }

        fn game_in_front(&self) -> bool {
            process_name(foreground_pid()) == GAME_EXE
        }

        fn idle_seconds(&self) -> Option<f64> {
            let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            // SAFETY: `info` is as large as `cbSize` says and lives during the call.
            if unsafe { GetLastInputInfo(&raw mut info) } == 0 {
                return None;
            }
            // SAFETY: a call without arguments. The tick count wraps around after 49 days: the difference
            // wraps with it.
            let now = unsafe { GetTickCount() };
            Some(f64::from(now.wrapping_sub(info.dwTime)) / 1000.0)
        }

        fn game_needs_admin(&self) -> Option<bool> {
            // SAFETY: the pseudo handle of this process needs no closing.
            let own = elevated(unsafe { GetCurrentProcess() })?;
            let pid = foreground_pid();
            // SAFETY: asks for a handle with the least rights; closed below.
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if handle.is_null() {
                return None;
            }
            let game = elevated(handle);
            // SAFETY: the handle was opened above.
            unsafe { CloseHandle(handle) };
            Some(game? && !own)
        }

        fn press(&self, keys: &[Key]) -> Result<(), SendError> {
            let scans: Vec<u16> = keys.iter().map(|key| key.scan).collect();
            crate::stick_input::own_press_begins(&scans);
            press_with(keys, HOLD, &mut |events| send(events))
        }

        fn watch_sticks(&self) -> Option<Box<dyn StickWatch>> {
            Listener::start().map(|listener| Box::new(listener) as Box<dyn StickWatch>)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use super::*;

    /// A stand-in for the system that sends nothing: what the tests set is what it answers. Its watch reports
    /// what the test puts into `input` and `sticks`.
    #[derive(Default)]
    struct Fake {
        unsupported: AtomicBool,
        front: AtomicBool,
        /// Milliseconds since the last input Windows knows (`GetLastInputInfo`); `u32::MAX` = cannot be told.
        idle_ms: AtomicU32,
        /// 0 = unclear, 1 = needs administrator, 2 = does not.
        admin: AtomicU32,
        press_fails: AtomicBool,
        /// Do the keys arrive after pressing (they come back to the watch, and Windows' input time drops)?
        arrives: AtomicBool,
        pressed: AtomicU32,
        /// What the watch says of keyboard and mouse.
        input: Arc<Mutex<(f64, Option<Source>)>>,
        /// What it says instead while keys sent by a program do not count (the last input was one).
        injected_off: Arc<Mutex<f64>>,
        /// The keys of every press.
        keys: Mutex<Vec<Vec<Key>>>,
        /// The own keys that came back to the watch.
        own: Arc<AtomicU32>,
        /// Do the keys of the process go to the watch?
        ours: Arc<AtomicBool>,
        /// What the following of the sticks reports; shared with the watch it hands out.
        sticks: Arc<Mutex<JoyStates>>,
        /// The system cannot follow.
        no_sticks: AtomicBool,
        /// How many watches were handed out and how many of them are gone.
        watches: Arc<AtomicU32>,
        watches_gone: Arc<AtomicU32>,
    }

    /// The watch of the stand-in: reports what the test put into the fake.
    struct FakeWatch {
        sticks: Arc<Mutex<JoyStates>>,
        input: Arc<Mutex<(f64, Option<Source>)>>,
        /// The seconds without input the watch says while keys sent by a program do not count.
        injected_off: Arc<Mutex<f64>>,
        own: Arc<AtomicU32>,
        ours: Arc<AtomicBool>,
        gone: Arc<AtomicU32>,
    }

    impl StickWatch for FakeWatch {
        fn states(&self) -> JoyStates {
            self.sticks.lock().unwrap().clone()
        }
        fn running(&self) -> bool {
            true
        }
        fn input_idle(&self, count_injected: bool) -> (f64, Option<Source>) {
            let (seconds, source) = *self.input.lock().unwrap();
            if source == Some(Source::KeyboardInjected) && !count_injected {
                return (*self.injected_off.lock().unwrap(), None);
            }
            (seconds, source)
        }
        fn own_keys(&self) -> u32 {
            self.own.load(Ordering::SeqCst)
        }
        fn keyboard_ours(&self) -> bool {
            self.ours.load(Ordering::SeqCst)
        }
    }

    impl Drop for FakeWatch {
        fn drop(&mut self) {
            self.gone.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Fake {
        /// The game in front, the player away for 400 seconds, and Windows' input time at 0: a HOTAS at rest
        /// sends reports that Windows counts as input.
        fn ready() -> Arc<Fake> {
            let fake = Fake::default();
            fake.front.store(true, Ordering::SeqCst);
            fake.idle_ms.store(0, Ordering::SeqCst);
            fake.admin.store(2, Ordering::SeqCst);
            fake.arrives.store(true, Ordering::SeqCst);
            fake.ours.store(true, Ordering::SeqCst);
            fake.away(400.0);
            Arc::new(fake)
        }

        /// Keyboard and mouse untouched for `seconds` (since the following started).
        fn away(&self, seconds: f64) {
            *self.input.lock().unwrap() = (seconds, None);
        }

        /// The last input of keyboard or mouse `seconds` ago.
        fn used(&self, seconds: f64, source: Source) {
            *self.input.lock().unwrap() = (seconds, Some(source));
        }

        fn watch(&self) -> Box<dyn StickWatch> {
            self.watch_sticks().expect("a watch")
        }
    }

    impl Platform for Fake {
        fn supported(&self) -> bool {
            !self.unsupported.load(Ordering::SeqCst)
        }
        fn game_in_front(&self) -> bool {
            self.front.load(Ordering::SeqCst)
        }
        fn idle_seconds(&self) -> Option<f64> {
            match self.idle_ms.load(Ordering::SeqCst) {
                u32::MAX => None,
                ms => Some(f64::from(ms) / 1000.0),
            }
        }
        fn game_needs_admin(&self) -> Option<bool> {
            match self.admin.load(Ordering::SeqCst) {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            }
        }
        fn press(&self, keys: &[Key]) -> Result<(), SendError> {
            if self.press_fails.load(Ordering::SeqCst) {
                return Err(SendError("fake".into()));
            }
            self.keys.lock().unwrap().push(keys.to_vec());
            self.pressed.fetch_add(1, Ordering::SeqCst);
            if self.arrives.load(Ordering::SeqCst) {
                // Alt and X, down: the own keys come back; Windows' input time drops.
                self.own.fetch_add(2, Ordering::SeqCst);
                self.idle_ms.store(10, Ordering::SeqCst);
            }
            Ok(())
        }
        fn watch_sticks(&self) -> Option<Box<dyn StickWatch>> {
            if self.no_sticks.load(Ordering::SeqCst) {
                return None;
            }
            self.watches.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(FakeWatch {
                sticks: Arc::clone(&self.sticks),
                input: Arc::clone(&self.input),
                injected_off: Arc::clone(&self.injected_off),
                own: Arc::clone(&self.own),
                ours: Arc::clone(&self.ours),
                gone: Arc::clone(&self.watches_gone),
            }))
        }
    }

    /// A stick as the system describes it: X and Y (16 bits), a hat, eight buttons, all in report 0.
    fn layout() -> crate::stick_input::Layout {
        use crate::stick_input::{BUTTON_PAGE, ButtonCap, Layout, ValueCap};
        let mut layout = Layout::default();
        let value = |usage, index, bits, max| ValueCap {
            page: 0x01,
            report: 0,
            range: false,
            first_usage: usage,
            first_index: index,
            last_index: index,
            count: 1,
            bits,
            min: 0,
            max,
        };
        layout.add_values(value(0x30, 0, 16, 65_535));
        layout.add_values(value(0x31, 1, 16, 65_535));
        layout.add_values(value(0x39, 2, 4, 7));
        layout.add_buttons(ButtonCap { page: BUTTON_PAGE, report: 0, first_index: 3, last_index: 10 });
        layout
    }

    /// The state of that stick after a report with X at `axis`, Y in the middle, these buttons down and the hat.
    fn stick(axis: u32, buttons: u8, hat: u32) -> crate::stick_input::StickState {
        let mut data = vec![(0, axis), (1, 32_768), (2, hat)];
        data.extend((0..8u16).filter(|bit| buttons & (1 << bit) != 0).map(|bit| (3 + bit, 1)));
        let mut state = crate::stick_input::StickState::default();
        state.apply(&layout(), 0, &data);
        state
    }

    fn sticks(list: &[(u64, crate::stick_input::StickState)]) -> JoyStates {
        list.iter().cloned().collect()
    }

    fn on(fake: &Arc<Fake>) -> AntiAfk {
        let mut logic = AntiAfk::default();
        assert!(logic.set_enabled(true, fake.as_ref()));
        logic
    }

    fn paths_in(name: &str) -> (crate::testdir::Dir, Paths) {
        let folder = crate::testdir::Dir::new(name);
        let text = folder.path().to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::Posix, &[("SC_BP_HOME", &text)], &text);
        (folder, Paths::new(host))
    }

    #[test]
    fn it_starts_off_and_stays_off_where_the_system_cannot_do_it() {
        let fake = Fake::ready();
        let logic = AntiAfk::default();
        assert!(!logic.enabled(), "off after every start");
        assert_eq!(logic.last_error(), None);
        assert_eq!(logic.sent_count(), 0);
        fake.unsupported.store(true, Ordering::SeqCst);
        let mut logic = AntiAfk::default();
        assert!(!logic.set_enabled(true, fake.as_ref()), "the switch stays off");
        assert!(!logic.enabled());
    }

    #[test]
    fn a_beat_goes_through_its_checks_in_order_and_presses_only_at_the_end() {
        let fake = Fake::ready();
        let watch = fake.watch();
        let watch = Some(watch.as_ref());
        let mut logic = AntiAfk::default();
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Off, "off first");
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 0);
        let mut logic = on(&fake);
        // The system loses the ability: the switch goes off.
        fake.unsupported.store(true, Ordering::SeqCst);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Unsupported);
        assert!(!logic.enabled());
        fake.unsupported.store(false, Ordering::SeqCst);
        let mut logic = on(&fake);
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::NotInFront);
        assert_eq!(logic.reason(), Some(Reason::NotInFront));
        fake.front.store(true, Ordering::SeqCst);
        // Without the following the input of the player cannot be read: nothing is pressed.
        assert_eq!(logic.tick(1000.0, fake.as_ref(), None), Outcome::Blind);
        assert_eq!(logic.reason(), Some(Reason::Blind));
        // Used a moment ago: the player is there.
        fake.used(299.0, Source::Keyboard);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Active);
        assert_eq!(logic.reason(), Some(Reason::Waiting));
        assert_eq!(logic.input_at(), Some((701.0, Some(Source::Keyboard))));
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 0, "nothing pressed so far");
        // Idle for long enough: pressed, once, although Windows' input time says 0 seconds.
        fake.used(300.0, Source::Keyboard);
        assert_eq!(fake.idle_seconds(), Some(0.0));
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Sent);
        assert_eq!((fake.pressed.load(Ordering::SeqCst), logic.sent_count()), (1, 1));
        assert_eq!((logic.reason(), logic.last_sent()), (Some(Reason::Pressed), Some(1000.0)));
        // The own keys are no input of the player: the wait goes on, but not twice in one interval.
        fake.used(400.0, Source::Keyboard);
        assert_eq!(logic.tick(1100.0, fake.as_ref(), watch), Outcome::TooSoon);
        assert_eq!(logic.reason(), Some(Reason::Pressed));
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 1);
        // The next interval: again.
        assert_eq!(logic.tick(1300.0, fake.as_ref(), watch), Outcome::Sent);
        assert_eq!(logic.sent_count(), 2);
        assert!(logic.enabled(), "it goes on");
        // Switched off: no reason.
        logic.set_enabled(false, fake.as_ref());
        assert_eq!(logic.reason(), None);
    }

    #[test]
    fn a_game_that_runs_as_administrator_stops_it_before_a_key_is_sent_and_again_when_it_did_not_arrive() {
        let fake = Fake::ready();
        let watch = fake.watch();
        let watch = Some(watch.as_ref());
        fake.admin.store(1, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Blocked);
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 0, "nothing is sent into the void");
        assert!(!logic.enabled());
        assert_eq!(logic.last_error(), Some(ERR_ADMIN));
        let (key, when) = logic.last_failure().expect("the stop is kept");
        assert_eq!(key, ERR_ADMIN);
        assert_eq!(when.len(), "2026-10-23 12:00".len(), "{when}");
        // Not to be told: tried. The keys go out but never come back to the listener: they did not arrive,
        // although Windows' input time stands at 0 (the reports of the sticks).
        fake.admin.store(0, Ordering::SeqCst);
        fake.arrives.store(false, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Blocked);
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 1, "this time it was sent");
        assert_eq!(logic.sent_count(), 0, "and does not count");
        assert_eq!(logic.last_error(), Some(ERR_ADMIN));
        // They come back: arrived.
        fake.arrives.store(true, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Sent);
    }

    #[test]
    fn where_the_keys_of_the_process_go_elsewhere_the_input_time_of_windows_is_the_hint() {
        let fake = Fake::ready();
        let watch = fake.watch();
        let watch = Some(watch.as_ref());
        fake.ours.store(false, Ordering::SeqCst);
        fake.admin.store(0, Ordering::SeqCst);
        // The input time of Windows does not drop after the press: not arrived.
        fake.idle_ms.store(400_000, Ordering::SeqCst);
        fake.arrives.store(false, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Blocked);
        // It drops: arrived.
        fake.idle_ms.store(400_000, Ordering::SeqCst);
        fake.arrives.store(true, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), watch), Outcome::Sent);
    }

    #[test]
    fn a_key_that_cannot_be_sent_stops_it_with_its_own_reason_and_switching_on_forgets_the_reason_but_not_the_stop() {
        let fake = Fake::ready();
        let watch = fake.watch();
        fake.press_fails.store(true, Ordering::SeqCst);
        let mut logic = on(&fake);
        assert_eq!(logic.tick(1000.0, fake.as_ref(), Some(watch.as_ref())), Outcome::Failed);
        assert_eq!((logic.enabled(), logic.last_error()), (false, Some(ERR_SEND)));
        fake.press_fails.store(false, Ordering::SeqCst);
        assert!(logic.set_enabled(true, fake.as_ref()));
        assert_eq!(logic.last_error(), None, "the reason holds until the next switching on");
        assert_eq!(logic.last_failure().map(|f| f.0), Some(ERR_SEND), "the stop stays for the report");
    }

    #[test]
    fn the_keys_go_down_in_order_and_up_backwards_and_up_also_when_down_failed() {
        let keys = anti_afk_key::parse("lctrl+ralt+k").expect("keys");
        let [ctrl, alt, k] = [keys[0], keys[1], keys[2]];
        assert!(alt.extended && !ctrl.extended && !k.extended, "{keys:?}");
        assert_eq!(
            key_events(&keys, false),
            [KeyEvent { key: ctrl, up: false }, KeyEvent { key: alt, up: false }, KeyEvent { key: k, up: false }]
        );
        assert_eq!(
            key_events(&keys, true),
            [KeyEvent { key: k, up: true }, KeyEvent { key: alt, up: true }, KeyEvent { key: ctrl, up: true }]
        );
        let mut seen: Vec<Vec<KeyEvent>> = Vec::new();
        let result = press_with(&keys, Duration::ZERO, &mut |events| {
            seen.push(events.to_vec());
            Ok(())
        });
        assert!(result.is_ok());
        assert_eq!(seen, [key_events(&keys, false), key_events(&keys, true)]);
        // The pressing fails: the letting go still happens, and the error is the first one.
        let keys = anti_afk_key::FALLBACK;
        let mut seen: Vec<Vec<KeyEvent>> = Vec::new();
        let result = press_with(&keys, Duration::ZERO, &mut |events| {
            seen.push(events.to_vec());
            if seen.len() == 1 { Err(SendError("no".into())) } else { Ok(()) }
        });
        assert!(matches!(result, Err(SendError(ref why)) if why == "no"), "{result:?}");
        assert_eq!(seen.len(), 2, "the keys are let go");
        assert!(seen[1].iter().all(|event| event.up));
    }

    #[test]
    fn a_stick_that_moves_counts_as_input_but_flicker_a_new_device_and_the_first_look_do_not() {
        // The hat at rest says 8 (no direction), 2 is to the right.
        let rest = sticks(&[(0, stick(32768, 0, 8))]);
        assert!(!joystick_moved(&rest, &rest));
        assert!(!joystick_moved(&rest, &sticks(&[(0, stick(32768 + 999, 0, 8))])), "flicker");
        assert!(joystick_moved(&rest, &sticks(&[(0, stick(32768 + 1000, 0, 8))])), "an axis from 1000 on");
        assert!(joystick_moved(&rest, &sticks(&[(0, stick(32768, 1, 8))])), "a button");
        assert!(joystick_moved(&rest, &sticks(&[(0, stick(32768, 0, 2))])), "the hat");
        let mut other = sticks(&[(0, stick(32768, 0, 8))]);
        other.insert(1, stick(0, 0, 0));
        assert!(!joystick_moved(&rest, &other), "a stick plugged in between counts for nothing");
        assert!(!joystick_moved(&other, &rest), "nor one that is gone");
        let silent = sticks(&[(0, crate::stick_input::StickState::default())]);
        assert!(!joystick_moved(&silent, &rest), "a stick whose first report came between counts for nothing");

        let fake = Fake::ready();
        let watch = fake.watch();
        let watch = Some(watch.as_ref());
        let mut logic = on(&fake);
        let look = |logic: &mut AntiAfk, now: f64, states: &JoyStates| logic.joy_note(now, states.clone());
        assert!(!look(&mut logic, 10.0, &rest), "the first look makes the stand to compare with");
        assert!(!look(&mut logic, 13.0, &rest));
        assert!(look(&mut logic, 16.0, &sticks(&[(0, stick(32768, 4, 8))])));
        // The stick counts like keyboard and mouse: the younger of both ages.
        assert_eq!(logic.idle_all(20.0, (400.0, None)), (4.0, Some(Source::Stick)));
        assert_eq!(logic.idle_all(20.0, (2.0, Some(Source::Mouse))), (2.0, Some(Source::Mouse)));
        // A moved stick holds the press back.
        assert_eq!(logic.tick(100.0, fake.as_ref(), watch), Outcome::Active, "the stick moved 84 seconds ago");
        assert_eq!(logic.input_at(), Some((16.0, Some(Source::Stick))));
        assert_eq!(logic.tick(400.0, fake.as_ref(), watch), Outcome::Sent, "and 384 seconds ago");
        // The sticks are looked at again: a button was pressed and let go.
        look(&mut logic, 500.0, &sticks(&[(0, stick(32768, 4, 8))]));
        assert!(look(&mut logic, 600.0, &sticks(&[(0, stick(32768, 0, 8))])));
        assert_eq!(logic.tick(601.0, fake.as_ref(), watch), Outcome::Active, "the stick moved a second ago");
    }

    #[test]
    fn the_snapshot_survives_the_file_with_every_field_and_a_missing_file_says_off() {
        let (_folder, paths) = paths_in("antiafk-snapshot");
        assert_eq!(Snapshot::read(&paths), Snapshot::default(), "no file: off, nothing sent");
        let snapshot = Snapshot {
            on: true,
            sent: 7,
            joystick: true,
            failure: Some((ERR_ADMIN.to_string(), "2026-10-23 12:34".to_string())),
            reason: Some(Reason::Waiting),
            input_at: Some(1_793_000_000),
            input_source: Some(Source::Mouse),
            pressed_at: Some(1_792_999_000),
            stop: Some(ERR_SEND.to_string()),
            on_since: Some(1_792_998_000),
            key: Some(("lalt+k".to_string(), Origin::Own)),
            sticks: 3,
        };
        snapshot.write(&paths).unwrap();
        assert_eq!(Snapshot::read(&paths), snapshot);
        let text = std::fs::read_to_string(paths.app_file(FILE)).unwrap();
        for word in [
            "\"grund\": \"wartet\"",
            "\"eingabe_um\": 1793000000",
            "\"eingabe_art\": \"maus\"",
            "\"gedrueckt_um\"",
            "\"stopp\": \"aafk_err_send\"",
            "\"an_seit\": 1792998000",
            "\"taste\": \"lalt+k\"",
            "\"taste_herkunft\": \"eigene\"",
            "\"sticks\": 3",
        ] {
            assert!(text.contains(word), "{word}: {text}");
        }
        for reason in [Reason::NotInFront, Reason::Pressed, Reason::Blind] {
            let one = Snapshot { reason: Some(reason), input_source: None, ..snapshot.clone() };
            one.write(&paths).unwrap();
            assert_eq!(Snapshot::read(&paths), one);
        }
        let off = Snapshot { failure: None, ..Snapshot::default() };
        off.write(&paths).unwrap();
        assert_eq!(Snapshot::read(&paths), off);
        // A file of an overlay without the new fields still reads, and unknown words are no reason.
        std::fs::write(paths.app_file(FILE), "{\"an\": true, \"gesendet\": 2, \"joystick\": true, \"grund\": \"x\"}")
            .unwrap();
        assert_eq!(Snapshot::read(&paths), Snapshot { on: true, sent: 2, joystick: true, ..Snapshot::default() });
    }

    #[test]
    fn the_runner_switches_the_look_at_the_sticks_and_writes_the_snapshot_after_every_change() {
        let (_folder, paths) = paths_in("antiafk-runner");
        // The file of an earlier run says "on": the new run writes "off" at once.
        Snapshot { on: true, sent: 9, joystick: true, ..Snapshot::default() }.write(&paths).unwrap();
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert_eq!(Snapshot::read(&paths), Snapshot::default(), "off from the start");
        assert!(runner.supported() && !runner.enabled() && !runner.joy_running());
        assert!(runner.set_enabled(true));
        assert!(runner.enabled() && runner.joy_running(), "the look at the sticks runs while it is on");
        let switched = Snapshot::read(&paths);
        assert!(switched.on_since.is_some_and(|at| at > 1_700_000_000), "{switched:?}");
        assert_eq!(
            switched,
            Snapshot {
                on: true,
                joystick: true,
                on_since: switched.on_since,
                key: Some((anti_afk_key::DEFAULT_INPUT.to_string(), Origin::Default)),
                ..Snapshot::default()
            }
        );
        assert_eq!(runner.tick(), Outcome::Sent);
        let pressed = Snapshot::read(&paths);
        assert_eq!((pressed.sent, pressed.reason), (1, Some(Reason::Pressed)), "a press is published");
        assert!(pressed.pressed_at.is_some_and(|at| at > 1_700_000_000), "{pressed:?}");
        // A stop is published with its reason, the thread of the sticks ends.
        fake.admin.store(1, Ordering::SeqCst);
        // The interval has not passed since the press: the beat is "too soon" first.
        assert_eq!(runner.tick(), Outcome::TooSoon);
        assert!(!runner.set_enabled(false), "gives the new state");
        assert!(!runner.enabled());
        let after = Snapshot::read(&paths);
        assert!(!after.on && !after.joystick && after.sent == 1 && after.reason.is_none(), "{after:?}");
        // The stop comes through a runner of its own, whose interval has not run yet.
        let blocked = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        blocked.set_enabled(true);
        assert_eq!(blocked.tick(), Outcome::Blocked);
        let stop = Snapshot::read(&paths);
        assert!(!stop.on && !stop.joystick, "{stop:?}");
        assert_eq!(stop.failure.as_ref().map(|f| f.0.as_str()), Some(ERR_ADMIN));
        assert_eq!(blocked.last_error(), Some(ERR_ADMIN));
        // A system that cannot do it: no switch, no thread.
        fake.unsupported.store(true, Ordering::SeqCst);
        assert!(!runner.set_enabled(true));
        assert!(!runner.joy_running());
    }

    /// What the page needs to say why it does not press comes into the file at the beat that sees it.
    #[test]
    fn the_snapshot_says_why_it_does_not_press_with_the_time_and_the_source_of_the_last_input() {
        let (_folder, paths) = paths_in("antiafk-grund");
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.set_enabled(true));
        let now =
            || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(runner.tick(), Outcome::NotInFront);
        assert_eq!(Snapshot::read(&paths).reason, Some(Reason::NotInFront));
        fake.front.store(true, Ordering::SeqCst);
        fake.used(4.0, Source::Keyboard);
        assert_eq!(runner.tick(), Outcome::Active);
        let waiting = Snapshot::read(&paths);
        assert_eq!((waiting.reason, waiting.input_source), (Some(Reason::Waiting), Some(Source::Keyboard)));
        let ago = now() - waiting.input_at.expect("the time of the last input");
        assert!((3..=6).contains(&ago), "the last input four seconds ago: {ago}");
        fake.used(1.0, Source::Mouse);
        assert_eq!(runner.tick(), Outcome::Active);
        assert_eq!(Snapshot::read(&paths).input_source, Some(Source::Mouse));
        // Nothing came since the switching on: the time is that of the start, without a source.
        fake.away(7.0);
        assert_eq!(runner.tick(), Outcome::Active);
        let still = Snapshot::read(&paths);
        assert_eq!((still.reason, still.input_source), (Some(Reason::Waiting), None));
        runner.set_enabled(false);
        assert_eq!(Snapshot::read(&paths).reason, None, "off: no reason");
        // Without the following there is nothing to read: the file says so.
        fake.no_sticks.store(true, Ordering::SeqCst);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Blind);
        assert_eq!(Snapshot::read(&paths).reason, Some(Reason::Blind));
        runner.set_enabled(false);
    }

    /// Every state of the button: off, waiting, just pressed (for [`FLASH`] seconds), stopped, blind.
    #[test]
    fn every_state_has_its_look_and_the_flash_of_a_press_ends_after_its_time() {
        assert_eq!(Look::of(false, false, None, None), Look::Off);
        assert_eq!(Look::of(false, false, Some(Reason::Pressed), Some(0.1)), Look::Off, "off is off");
        assert_eq!(Look::of(false, true, None, None), Look::Stopped);
        assert_eq!(Look::of(true, false, None, None), Look::Waiting, "on before the first beat");
        assert_eq!(Look::of(true, false, Some(Reason::Waiting), None), Look::Waiting);
        assert_eq!(Look::of(true, false, Some(Reason::NotInFront), Some(60.0)), Look::Waiting);
        assert_eq!(Look::of(true, false, Some(Reason::Pressed), Some(0.0)), Look::Pressed);
        assert_eq!(Look::of(true, false, Some(Reason::Pressed), Some(FLASH - 0.01)), Look::Pressed);
        assert_eq!(Look::of(true, false, Some(Reason::Pressed), Some(FLASH)), Look::Waiting, "the flash is over");
        assert_eq!(Look::of(true, false, Some(Reason::Blind), Some(0.0)), Look::Blind);
        let keys: Vec<&str> =
            [Look::Off, Look::Waiting, Look::Pressed, Look::Stopped, Look::Blind].map(Look::hint_key).to_vec();
        let unique: std::collections::BTreeSet<&&str> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len(), "every state says itself: {keys:?}");
        for key in keys {
            assert_ne!(crate::language::t(key), key, "{key} has a text");
        }
        // The page reads the state from the file: the press of this second and the one before flash.
        let pressed =
            Snapshot { on: true, reason: Some(Reason::Pressed), pressed_at: Some(1000), ..Snapshot::default() };
        assert_eq!(
            (pressed.look(1000), pressed.look(1001), pressed.look(1002)),
            (Look::Pressed, Look::Pressed, Look::Waiting)
        );
        let stopped = Snapshot { stop: Some(ERR_ADMIN.into()), ..Snapshot::default() };
        assert_eq!(stopped.look(1000), Look::Stopped);
        // The runner: pressed, the flash, then waiting; a stop, until the next switching on.
        let fake = Fake::ready();
        let woken = Arc::new(AtomicU32::new(0));
        let runner = Runner::with_timing(fake.clone(), None, Duration::from_secs(3600), Duration::from_secs(3600));
        let counter = woken.clone();
        runner.set_waker(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(runner.state(), Look::Off);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.state(), Look::Waiting);
        assert_eq!(runner.tick(), Outcome::Sent);
        assert_eq!(woken.load(Ordering::SeqCst), 1, "a press wakes the window");
        assert_eq!(runner.state(), Look::Pressed);
        assert!(runner.flash_left().is_some_and(|left| left > 0.0 && left <= FLASH));
        assert!(wait_for(|| runner.state() == Look::Waiting), "the flash ends");
        assert_eq!(runner.flash_left(), None);
        fake.admin.store(1, Ordering::SeqCst);
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Blocked);
        assert_eq!(runner.state(), Look::Stopped);
        assert_eq!(runner.snapshot().stop.as_deref(), Some(ERR_ADMIN));
        runner.set_enabled(false);
        assert_eq!(runner.state(), Look::Stopped, "switched off, the stop holds");
        fake.admin.store(2, Ordering::SeqCst);
        fake.front.store(false, Ordering::SeqCst);
        assert!(runner.set_enabled(true));
        assert_eq!((runner.state(), runner.snapshot().stop), (Look::Waiting, None), "switched on: the stop is gone");
        fake.no_sticks.store(true, Ordering::SeqCst);
        fake.front.store(true, Ordering::SeqCst);
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Blind);
        assert_eq!(runner.state(), Look::Blind);
        runner.set_enabled(false);
    }

    /// The seconds the player set count from the next beat on, within their range; without the setting it is
    /// five minutes.
    #[test]
    fn the_runner_waits_as_long_as_the_player_set() {
        let (_folder, paths) = paths_in("antiafk-zeit");
        assert_eq!(interval_for(&paths), INTERVAL, "five minutes without the setting");
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.set_enabled(true));
        fake.away(21.0);
        assert_eq!(runner.tick(), Outcome::Active, "21 seconds are not five minutes");
        assert!(paths.set_setting(SETTING_INTERVAL, Value::int(20)));
        assert_eq!(runner.tick(), Outcome::Sent, "20 seconds set, 21 idle");
        fake.away(19.0);
        // Switched on again: the wait starts over.
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Active, "19 seconds idle are not 20");
        // Outside the range: the nearest end.
        assert!(paths.set_setting(SETTING_INTERVAL, Value::int(3)));
        assert_eq!(interval_for(&paths), *INTERVAL_RANGE.start() as f64);
        assert!(paths.set_setting(SETTING_INTERVAL, Value::int(100_000)));
        assert_eq!(interval_for(&paths), *INTERVAL_RANGE.end() as f64);
        runner.set_enabled(false);
    }

    /// A HOTAS at rest keeps sending the same report, and Windows counts every one as input: the runner presses
    /// all the same once the set seconds are over, and a stick that moves holds it back.
    #[test]
    fn the_runner_presses_after_the_set_seconds_while_only_the_reports_of_a_stick_at_rest_come_in() {
        let (_folder, paths) = paths_in("antiafk-herzschlag");
        assert!(paths.set_setting(SETTING_INTERVAL, Value::int(15)));
        let fake = Fake::ready();
        fake.away(16.0);
        let runner = Runner::with_timing(
            fake.clone(),
            Some(Paths::new(paths.host.clone())),
            Duration::from_secs(3600),
            Duration::from_millis(5),
        );
        let rest = sticks(&[(1, stick(31_000, 0, 8)), (2, stick(33_000, 0, 8)), (3, stick(32_768, 0, 8))]);
        *fake.sticks.lock().unwrap() = rest.clone();
        assert!(runner.set_enabled(true));
        // The reports at rest come in all the time, the look compares them every few milliseconds.
        let beating = Arc::new(AtomicBool::new(true));
        let heart = {
            let (sticks, beating, rest) = (Arc::clone(&fake.sticks), Arc::clone(&beating), rest.clone());
            std::thread::spawn(move || {
                while beating.load(Ordering::SeqCst) {
                    *sticks.lock().unwrap() = rest.clone();
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        };
        assert!(wait_for(|| locked(&runner.shared.logic).joy_before.is_some()), "a first look");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(fake.idle_seconds(), Some(0.0), "Windows counts the reports as input");
        assert_eq!(runner.tick(), Outcome::Sent, "the reports at rest are no input of the player");
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 1);
        beating.store(false, Ordering::SeqCst);
        heart.join().unwrap();
        // The other way: a hand on the stick between two looks holds the press back.
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert!(wait_for(|| locked(&runner.shared.logic).joy_before.is_some()), "a first look");
        *fake.sticks.lock().unwrap() = sticks(&[(1, stick(31_000 + 5000, 0, 8)), (2, stick(33_000, 0, 8))]);
        assert!(wait_for(|| locked(&runner.shared.logic).joy_last.is_some()), "the look saw the stick move");
        assert_eq!(runner.tick(), Outcome::Active);
        let snapshot = Snapshot::read(&paths);
        assert_eq!((snapshot.reason, snapshot.input_source), (Some(Reason::Waiting), Some(Source::Stick)));
        assert_eq!(snapshot.sticks, 2, "the sticks the following knows");
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 1);
        runner.set_enabled(false);
    }

    #[test]
    fn the_sticks_are_followed_while_the_switch_is_on_and_a_moved_stick_holds_the_press_back() {
        let fake = Fake::ready();
        let runner = Runner::with_timing(fake.clone(), None, Duration::from_secs(3600), Duration::from_millis(5));
        *fake.sticks.lock().unwrap() = sticks(&[(0, stick(32768, 0, 8))]);
        assert_eq!(fake.watches.load(Ordering::SeqCst), 0, "nothing is followed while it is off");
        assert!(runner.set_enabled(true));
        assert_eq!(fake.watches.load(Ordering::SeqCst), 1);
        assert!(runner.joy_running());
        // The first look makes the stand; then the stick moves, and the next look sees it.
        assert!(wait_for(|| locked(&runner.shared.logic).joy_before.is_some()), "a first look");
        *fake.sticks.lock().unwrap() = sticks(&[(0, stick(32768 + 5000, 0, 8))]);
        assert!(wait_for(|| locked(&runner.shared.logic).joy_last.is_some()), "the look saw the stick move");
        assert_eq!(runner.tick(), Outcome::Active, "keyboard and mouse are idle, the stick is not");
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 0);
        assert!(!runner.set_enabled(false));
        assert_eq!(fake.watches_gone.load(Ordering::SeqCst), 1, "switching off ends the following");
        // A system that cannot follow: the switch and the beat work, nothing is looked at and nothing pressed.
        fake.no_sticks.store(true, Ordering::SeqCst);
        assert!(runner.set_enabled(true));
        assert!(runner.beat_running() && !runner.joy_running());
        assert!(!runner.snapshot().joystick, "the page learns that nothing is followed");
        assert_eq!(runner.tick(), Outcome::Blind);
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 0);
        runner.set_enabled(false);
    }

    /// The keys of a press are the keyboard binding of "wipe visor" of the player, read again when the file
    /// changes; without one Left Alt + X. The page and the report learn them from the file.
    #[test]
    fn the_runner_presses_the_binding_of_the_player_and_alt_x_without_one() {
        let (paths, file, _folder) = anti_afk_key::test_world("antiafk-belegung");
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.set_enabled(true));
        assert_eq!(Snapshot::read(&paths).key, Some(("lalt+x".to_string(), Origin::Default)), "no file");
        assert_eq!(runner.tick(), Outcome::Sent);
        assert_eq!(fake.keys.lock().unwrap().last().map(Vec::as_slice), Some(anti_afk_key::FALLBACK.as_slice()));
        std::fs::write(&file, anti_afk_key::test_maps(r#"<rebind input="kb1_lalt+k"/>"#)).unwrap();
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert_eq!(Snapshot::read(&paths).key, Some(("lalt+k".to_string(), Origin::Own)), "switching on reads it");
        assert_eq!(runner.tick(), Outcome::Sent);
        let k = anti_afk_key::key_of("k").unwrap();
        let alt = anti_afk_key::key_of("lalt").unwrap();
        assert_eq!(fake.keys.lock().unwrap().last().cloned(), Some(vec![alt, k]), "Left Alt, then K");
        // Rebound while it is on: the next press sends the new binding.
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        std::fs::write(&file, anti_afk_key::test_maps(r#"<rebind input="kb1_lalt+lshift+v"/>"#)).unwrap();
        assert_eq!(runner.tick(), Outcome::Sent);
        assert_eq!(fake.keys.lock().unwrap().last().map(Vec::len), Some(3));
        assert_eq!(Snapshot::read(&paths).key.map(|k| k.0), Some("lalt+lshift+v".to_string()));
        // A file that cannot be read: Left Alt + X, and the snapshot says it is the fallback.
        std::fs::write(&file, "<ActionMaps><kaputt").unwrap();
        runner.set_enabled(false);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Sent);
        assert_eq!(fake.keys.lock().unwrap().last().map(Vec::as_slice), Some(anti_afk_key::FALLBACK.as_slice()));
        assert_eq!(Snapshot::read(&paths).key, Some(("lalt+x".to_string(), Origin::Fallback)));
        runner.set_enabled(false);
        assert_eq!(Snapshot::read(&paths).key, None, "off: no key");
    }

    /// Keys sent by a program (macro software) count as input unless the player switched that off; the setting
    /// counts from the next beat on.
    #[test]
    fn keys_of_macro_software_count_as_input_unless_the_player_says_no() {
        let (_folder, paths) = paths_in("antiafk-makro");
        assert!(injected_count(&paths), "they count without the setting");
        let fake = Fake::ready();
        *fake.input.lock().unwrap() = (2.0, Some(Source::KeyboardInjected));
        *fake.injected_off.lock().unwrap() = 400.0;
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.set_enabled(true));
        assert_eq!(runner.tick(), Outcome::Active, "a key of the macro software two seconds ago");
        assert_eq!(Snapshot::read(&paths).input_source, Some(Source::KeyboardInjected));
        assert!(paths.set_setting(SETTING_INJECTED, Value::Bool(false)));
        assert!(!injected_count(&paths));
        assert_eq!(runner.tick(), Outcome::Sent, "they no longer count: 400 seconds without input");
        runner.set_enabled(false);
    }

    /// A pause between the moment a stop knows its reason and the moment the switch shows "off" (milliseconds):
    /// a test sets it to make a stretch of time in which an order of the two steps would show.
    pub(super) static STOP_GAP_MS: AtomicU32 = AtomicU32::new(0);

    /// Waits for `done` for at most half a minute (the threads take their time on a busy processor).
    fn wait_for(done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done() {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    #[test]
    fn the_beat_runs_in_a_thread_of_its_own_presses_without_a_window_and_tells_a_stop() {
        let fake = Fake::ready();
        let woken = Arc::new(AtomicU32::new(0));
        let runner = Runner::with_timing(fake.clone(), None, Duration::from_millis(10), Duration::from_millis(10));
        let counter = woken.clone();
        runner.set_waker(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert!(!runner.beat_running() && !runner.joy_running(), "nothing runs while it is off");
        assert!(runner.set_enabled(true));
        assert!(runner.beat_running() && runner.joy_running());
        // Nobody calls `tick`: the thread does, and presses once (the interval of five minutes holds it back
        // from pressing again).
        assert!(wait_for(|| fake.pressed.load(Ordering::SeqCst) >= 1), "the beat pressed");
        assert_eq!(runner.sent_count(), 1);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(fake.pressed.load(Ordering::SeqCst), 1, "not twice in one interval");
        assert!(runner.enabled(), "and it goes on");
        assert_eq!(runner.take_stop(), None);
        // The game now runs as administrator and the interval is over for a runner of its own: a stop.
        fake.admin.store(1, Ordering::SeqCst);
        let blocked = Runner::with_timing(fake.clone(), None, Duration::from_millis(10), Duration::from_millis(10));
        let counter = woken.clone();
        blocked.set_waker(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert!(blocked.set_enabled(true));
        assert!(wait_for(|| !blocked.enabled()), "the beat stopped it");
        assert_eq!(blocked.take_stop(), Some(ERR_ADMIN));
        assert_eq!(blocked.take_stop(), None, "told once");
        assert!(woken.load(Ordering::SeqCst) >= 1, "the window is woken");
        assert!(wait_for(|| !blocked.beat_running() && !blocked.joy_running()), "the threads end with the stop");
        // Switching off ends the threads of the first runner.
        assert!(!runner.set_enabled(false));
        assert!(wait_for(|| !runner.beat_running() && !runner.joy_running()));
    }

    #[test]
    fn whoever_sees_the_switch_off_after_a_stop_finds_the_reason_at_once() {
        // Between the knowing of the reason and the showing of "off" there is a pause of a tenth of a second: a
        // reader that waits for "off" and then asks for the reason must not find none. (The pause is reset at
        // the end; other tests that stop go through the same pause and are only slowed down.)
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                STOP_GAP_MS.store(0, Ordering::SeqCst);
            }
        }
        let _reset = Reset;
        STOP_GAP_MS.store(100, Ordering::SeqCst);
        let fake = Fake::ready();
        fake.admin.store(1, Ordering::SeqCst);
        let runner = Runner::with_timing(fake.clone(), None, Duration::from_millis(5), Duration::from_millis(50));
        assert!(runner.set_enabled(true));
        assert!(wait_for(|| !runner.enabled()), "the beat stopped it");
        assert_eq!(runner.take_stop(), Some(ERR_ADMIN), "off and the reason are there together");
        assert_eq!(runner.last_error(), Some(ERR_ADMIN));
        // A switching on forgets a reason nobody took.
        fake.admin.store(2, Ordering::SeqCst);
        assert!(runner.set_enabled(true));
        assert_eq!(runner.take_stop(), None);
        runner.set_enabled(false);
    }

    #[test]
    fn a_runner_that_is_dropped_ends_its_threads() {
        let fake = Fake::ready();
        let runner = Runner::with_timing(fake.clone(), None, Duration::from_millis(10), Duration::from_millis(10));
        assert!(runner.set_enabled(true));
        let weak = Arc::downgrade(&runner.shared);
        drop(runner);
        assert!(wait_for(|| weak.upgrade().is_none()), "the threads hold the runner only while they work");
    }

    #[test]
    fn a_snapshot_that_cannot_be_written_is_tried_again_at_the_next_beat() {
        let (_folder, paths) = paths_in("antiafk-ungeschrieben");
        // A folder where the file should be: nothing can be written there.
        std::fs::create_dir_all(paths.app_file(FILE)).unwrap();
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.shared.unpublished.load(Ordering::Relaxed), "the first write failed and is remembered");
        assert!(runner.set_enabled(true));
        // The obstacle goes; the next beat writes the file.
        std::fs::remove_dir_all(paths.app_file(FILE)).unwrap();
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(runner.tick(), Outcome::NotInFront);
        assert!(!runner.shared.unpublished.load(Ordering::Relaxed));
        assert!(Snapshot::read(&paths).on, "the file says what is true");
    }

    #[test]
    fn a_beat_that_sees_nothing_new_writes_nothing() {
        let (_folder, paths) = paths_in("antiafk-unveraendert");
        let fake = Fake::ready();
        let runner = Runner::with_platform(fake.clone(), Some(Paths::new(paths.host.clone())));
        assert!(runner.set_enabled(true));
        fake.front.store(false, Ordering::SeqCst);
        assert_eq!(runner.tick(), Outcome::NotInFront);
        // The file goes; a beat that sees the same does not write it again, one that sees something else does.
        std::fs::remove_file(paths.app_file(FILE)).unwrap();
        assert_eq!(runner.tick(), Outcome::NotInFront);
        assert!(!std::path::Path::new(&paths.app_file(FILE)).exists(), "nothing new, nothing written");
        fake.front.store(true, Ordering::SeqCst);
        fake.used(2.0, Source::Keyboard);
        assert_eq!(runner.tick(), Outcome::Active);
        assert_eq!(Snapshot::read(&paths).reason, Some(Reason::Waiting));
        runner.set_enabled(false);
    }

    #[test]
    fn the_signals_map_to_the_wishes_and_back_and_no_other_signal_is_a_wish() {
        use crate::window_signal::Signal;
        assert_eq!(signal_for(true), Signal::AntiAfkOn);
        assert_eq!(signal_for(false), Signal::AntiAfkOff);
        for on in [true, false] {
            assert_eq!(wish_of(signal_for(on)), Some(on));
        }
        for other in [Signal::Overview, Signal::Settings, Signal::Close, Signal::OpenGeneral, Signal::OpenAbout] {
            assert_eq!(wish_of(other), None, "{other:?}");
        }
        for reason in [Reason::NotInFront, Reason::Waiting, Reason::Pressed, Reason::Blind] {
            assert_eq!(Reason::from_key(reason.key()), Some(reason));
        }
    }

    #[test]
    fn on_the_real_system_only_what_is_safe_is_asked() {
        // Nothing is pressed here: `press` is not called. The questions are plain reads, and what they answer
        // depends on the machine (a session without a desktop has no input time, a game may be in front), so
        // they are only asked.
        let system = System;
        #[cfg(windows)]
        {
            assert!(system.supported());
            if let Some(seconds) = system.idle_seconds() {
                assert!(seconds >= 0.0);
            }
            let _ = system.game_in_front();
            let _ = system.game_needs_admin();
            // Followed by Raw Input: a registration of this test process, nothing is sent.
            let _alone = crate::stick_input::exclusive();
            let watch = system.watch_sticks().expect("Raw Input for keyboard, mouse and the game devices");
            assert!(watch.running());
            assert!(watch.keyboard_ours(), "the keys of this process go to the listener");
            assert!(watch.input_idle(true).0 >= 0.0);
            let _ = (watch.states(), watch.own_keys());
        }
        #[cfg(not(windows))]
        {
            assert!(!system.supported());
            assert!(system.watch_sticks().is_none() && system.idle_seconds().is_none());
        }
    }
}
