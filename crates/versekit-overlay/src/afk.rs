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

//! Anti-AFK in the overlay (`sc_bp_watcher.py` of 4.12: the button of the bar, the beat, the line of the stop).
//!
//! The overlay is the process that presses the keys. The beat runs in a thread of its own
//! ([`anti_afk::Runner`]), not in the frames of the window, so it goes on whatever the window does. The window
//! shows the button in the bar (only where the system can do it: Windows), takes the switching that the page of
//! the main window asks for by signal and shows the reason of a stop. What the main window needs to show comes
//! through the data folder ([`anti_afk::Snapshot`]). The switch is off after every start and is never stored.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use versekit_core::anti_afk::{self, Runner};
use versekit_core::language::{Phrase, t};
use versekit_core::window_signal::Signal;
use versekit_ui::overlay::{Action, Extra, Tone};

/// The id of the button in the bar.
pub const ID: &str = "anti_afk";
/// What the signal of the main window asked for: switch on / switch off.
pub const ASKED_ON: u8 = 1;
pub const ASKED_OFF: u8 = 2;

/// What the signal handler stores for a wish (`anti_afk::wish_of`).
pub fn asked(on: bool) -> u8 {
    if on { ASKED_ON } else { ASKED_OFF }
}

/// The handler of the window for a signal: a wish of anti-AFK is stored for the next frame. Gives whether the
/// signal was one (the handler then asks for a frame and has nothing more to do with it).
pub fn take_wish(signal: Signal, store: &AtomicU8) -> bool {
    let Some(on) = anti_afk::wish_of(signal) else { return false };
    store.store(asked(on), Ordering::Relaxed);
    true
}

/// What happened that the overlay shows.
#[derive(Debug, PartialEq, Eq)]
pub enum News {
    /// The signal of the main window switched; the state it has now.
    Switched(bool),
    /// A stop: the text key of the reason. It stands as a hint line in the overlay.
    Stopped(&'static str),
}

/// The state of anti-AFK in the overlay; without a runner (a system that cannot do it, a self-check) there is
/// no button and nothing happens.
pub struct Afk {
    runner: Option<Runner>,
    asked: Arc<AtomicU8>,
    /// The window is woken when the run stops by itself.
    attached: bool,
}

impl Afk {
    /// Nothing: no button, no beat.
    pub fn none(asked: Arc<AtomicU8>) -> Afk {
        Afk { runner: None, asked, attached: false }
    }

    /// With a runner (the real one, or a stand-in in the tests).
    pub fn with_runner(runner: Runner, asked: Arc<AtomicU8>) -> Afk {
        Afk { runner: Some(runner), asked, attached: false }
    }

    /// Is the switch on?
    #[cfg(test)]
    pub fn enabled(&self) -> bool {
        self.runner.as_ref().is_some_and(Runner::enabled)
    }

    /// The button of the bar in the state of anti-AFK: the timer in the accent while it waits, yellow for a
    /// moment after a press, red when it cannot press; crossed out and grey when off (red after a stop). Its
    /// hint says the state and what a click does.
    pub fn extra(&self) -> Option<Extra> {
        let look = self.runner.as_ref()?.state();
        Some(Extra {
            action: Action::Extra(ID),
            icon: if look.on() { "timer" } else { "timer-off" },
            meaning: t(look.hint_key()),
            id: ID,
            tone: Tone::of_anti_afk(look),
        })
    }

    /// Seconds until the yellow of a press ends; `None` while it does not stand yellow.
    pub fn flash_left(&self) -> Option<f64> {
        self.runner.as_ref()?.flash_left()
    }

    /// A click on the button: the other state. Gives the new one.
    pub fn toggle(&self) -> Option<bool> {
        let runner = self.runner.as_ref()?;
        Some(runner.set_enabled(!runner.enabled()))
    }

    /// The line for the status of the overlay: "Anti-AFK: an/aus".
    pub fn said(on: bool) -> Phrase {
        Phrase::new("aafk_sagen").with("0", Phrase::new(if on { "e_an" } else { "e_aus" }))
    }

    /// One look in a frame: takes the switching the main window asked for and the reason of a stop. `wake` is
    /// kept the first time and called when the run stops by itself, so that the frame comes.
    pub fn step(&mut self, wake: impl Fn() + Send + Sync + 'static) -> Vec<News> {
        let Some(runner) = &self.runner else { return Vec::new() };
        if !std::mem::replace(&mut self.attached, true) {
            runner.set_waker(wake);
        }
        let mut news = Vec::new();
        match self.asked.swap(0, Ordering::Relaxed) {
            ASKED_ON => news.push(News::Switched(runner.set_enabled(true))),
            ASKED_OFF => news.push(News::Switched(runner.set_enabled(false))),
            _ => {}
        }
        if let Some(key) = runner.take_stop() {
            news.push(News::Stopped(key));
        }
        news
    }

    /// The overlay closes: the switch goes off and the file says so.
    pub fn shutdown(&self) {
        if let Some(runner) = &self.runner {
            runner.set_enabled(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use versekit_core::anti_afk::{self, Platform, SendError, StickWatch};
    use versekit_core::language::Choice;

    use super::*;

    /// A system that sends nothing: the game in front, the player away for good.
    struct Quiet {
        admin: AtomicBool,
        pressed: AtomicU8,
        /// The own keys that came back to the watch; a press brings two, as keys that arrived do.
        own: Arc<std::sync::atomic::AtomicU32>,
        /// The input cannot be read (no watch).
        blind: AtomicBool,
    }

    /// The watch of the quiet system: keyboard and mouse untouched since long, no stick.
    struct QuietWatch {
        own: Arc<std::sync::atomic::AtomicU32>,
    }

    impl StickWatch for QuietWatch {
        fn states(&self) -> versekit_core::anti_afk::JoyStates {
            Default::default()
        }
        fn running(&self) -> bool {
            true
        }
        fn input_idle(&self, _count_injected: bool) -> (f64, Option<versekit_core::stick_input::Source>) {
            (1_000_000.0, None)
        }
        fn own_keys(&self) -> u32 {
            self.own.load(Ordering::SeqCst)
        }
        fn keyboard_ours(&self) -> bool {
            true
        }
    }

    impl Platform for Quiet {
        fn supported(&self) -> bool {
            true
        }
        fn game_in_front(&self) -> bool {
            true
        }
        fn idle_seconds(&self) -> Option<f64> {
            // Windows counts the reports of a stick at rest as input.
            Some(0.0)
        }
        fn game_needs_admin(&self) -> Option<bool> {
            Some(self.admin.load(Ordering::SeqCst))
        }
        fn press(&self, _keys: &[versekit_core::anti_afk_key::Key]) -> Result<(), SendError> {
            self.pressed.fetch_add(1, Ordering::SeqCst);
            self.own.fetch_add(2, Ordering::SeqCst);
            Ok(())
        }
        fn watch_sticks(&self) -> Option<Box<dyn StickWatch>> {
            if self.blind.load(Ordering::SeqCst) {
                return None;
            }
            Some(Box::new(QuietWatch { own: Arc::clone(&self.own) }))
        }
    }

    fn built(admin: bool) -> (Afk, Arc<AtomicU8>, Arc<Quiet>) {
        let (afk, asked, platform, _) = built_with(admin, std::time::Duration::from_millis(10));
        (afk, asked, platform)
    }

    /// With the beat every `beat`; the runner comes along for a test that beats by hand.
    fn built_with(admin: bool, beat: std::time::Duration) -> (Afk, Arc<AtomicU8>, Arc<Quiet>, Runner) {
        let platform = Arc::new(Quiet {
            admin: AtomicBool::new(admin),
            pressed: AtomicU8::new(0),
            own: Default::default(),
            blind: AtomicBool::new(false),
        });
        let asked = Arc::new(AtomicU8::new(0));
        let runner = Runner::with_timing(platform.clone(), None, beat, beat);
        (Afk::with_runner(runner.clone(), asked.clone()), asked, platform, runner)
    }

    /// Waits for `done` for at most half a minute (the threads take their time on a busy processor).
    fn wait_for(mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done() {
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        true
    }

    #[test]
    fn without_a_runner_there_is_no_button_and_nothing_happens() {
        let asked = Arc::new(AtomicU8::new(ASKED_ON));
        let mut none = Afk::none(asked.clone());
        assert!(none.extra().is_none() && none.toggle().is_none() && !none.enabled());
        assert!(none.step(|| {}).is_empty());
        assert_eq!(asked.load(Ordering::SeqCst), ASKED_ON, "the request stays: nobody to take it");
        none.shutdown();
    }

    #[test]
    fn the_button_follows_the_switch_with_its_icon_its_colour_and_its_hint() {
        let _language = language_de();
        let (afk, _, _, _) = built_with(false, std::time::Duration::from_secs(3600));
        let off = afk.extra().expect("the button");
        assert_eq!((off.icon, off.tone, off.id), ("timer-off", Tone::Normal, ID));
        assert_eq!(off.meaning, t("hinweis_aafk_aus"));
        assert_eq!(off.action, Action::Extra(ID));
        assert_eq!(afk.toggle(), Some(true));
        let on = afk.extra().expect("the button");
        assert_eq!((on.icon, on.tone), ("timer", Tone::Lit));
        assert_eq!(on.meaning, t("hinweis_aafk_an"));
        assert_eq!(afk.toggle(), Some(false));
        assert!(!afk.enabled());
        assert_eq!(Afk::said(true).to_string(), "Anti-AFK: an");
        assert_eq!(Afk::said(false).to_string(), "Anti-AFK: aus");
    }

    /// A press turns the button yellow for a moment, then it is green again; a stop and a run that cannot read
    /// the input turn it red. The hint says each state.
    #[test]
    fn the_button_flashes_yellow_after_a_press_and_turns_red_when_it_cannot_press() {
        let _language = language_de();
        let (mut afk, _, platform, runner) = built_with(false, std::time::Duration::from_secs(3600));
        let woken = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = woken.clone();
        afk.step(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(afk.toggle(), Some(true));
        assert_eq!(afk.flash_left(), None);
        assert_eq!(runner.tick(), anti_afk::Outcome::Sent);
        assert!(woken.load(Ordering::SeqCst) >= 1, "the press wakes the window");
        let pressed = afk.extra().expect("the button");
        assert_eq!((pressed.icon, pressed.tone), ("timer", Tone::Flash));
        assert_eq!(pressed.meaning, t("hinweis_aafk_gedrueckt"));
        let left = afk.flash_left().expect("the frame that ends the yellow");
        assert!(left > 0.0 && left <= anti_afk::FLASH, "{left}");
        assert!(wait_for(|| afk.extra().is_some_and(|e| e.tone == Tone::Lit)), "green again");
        assert_eq!(afk.flash_left(), None);
        // The game runs as administrator: stopped, red, crossed out.
        platform.admin.store(true, Ordering::SeqCst);
        afk.toggle();
        afk.toggle();
        assert_eq!(runner.tick(), anti_afk::Outcome::Blocked);
        let stopped = afk.extra().expect("the button");
        assert_eq!((stopped.icon, stopped.tone), ("timer-off", Tone::Fault));
        assert_eq!(stopped.meaning, t("hinweis_aafk_gestoppt"));
        // The input cannot be read: on, but red.
        platform.admin.store(false, Ordering::SeqCst);
        platform.blind.store(true, Ordering::SeqCst);
        assert_eq!(afk.toggle(), Some(true));
        assert_eq!(runner.tick(), anti_afk::Outcome::Blind);
        let blind = afk.extra().expect("the button");
        assert_eq!((blind.icon, blind.tone), ("timer", Tone::Fault));
        assert_eq!(blind.meaning, t("hinweis_aafk_blind"));
        afk.shutdown();
    }

    #[test]
    fn the_signal_of_the_main_window_switches_in_the_next_frame_and_is_taken_once() {
        let _language = language_de();
        let (mut afk, asked, _) = built(false);
        asked.store(ASKED_ON, Ordering::SeqCst);
        assert_eq!(afk.step(|| {}), [News::Switched(true)]);
        assert!(afk.enabled());
        assert_eq!(asked.load(Ordering::SeqCst), 0, "taken");
        assert!(afk.step(|| {}).iter().all(|news| !matches!(news, News::Switched(_))), "once");
        asked.store(ASKED_OFF, Ordering::SeqCst);
        assert_eq!(afk.step(|| {}), [News::Switched(false)]);
        assert!(!afk.enabled());
    }

    #[test]
    fn the_signal_handler_stores_the_wish_that_the_frame_takes() {
        // What the handler of the window stores for a signal is what the frame turns into a switching.
        let store = AtomicU8::new(0);
        for on in [true, false] {
            assert!(take_wish(anti_afk::signal_for(on), &store));
            assert_eq!(store.swap(0, Ordering::SeqCst), if on { ASKED_ON } else { ASKED_OFF });
        }
        assert_ne!(ASKED_ON, ASKED_OFF);
        for other in [Signal::Overview, Signal::Settings, Signal::Close, Signal::OpenGeneral] {
            assert!(!take_wish(other, &store), "{other:?}");
        }
        assert_eq!(store.load(Ordering::SeqCst), 0, "no other signal is a wish");
    }

    /// The whole way of the signal that the page of the main window sends: posted to a real window, taken by
    /// the hook of the window on its thread, turned into a wish by the handler of the overlay, taken by the
    /// frame and made a switching. A window of its own (a message window: nobody sees it), not the overlay that
    /// may run on this machine: the signal is sent to this window's handle, never found by its role.
    #[cfg(windows)]
    #[test]
    fn the_signal_of_the_main_window_arrives_through_the_window_messages_and_switches() {
        use versekit_core::window_signal::{self, Role};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW,
        };
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a message-only window of a system class, made and used in this thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null(), "the window");
        let (mut afk, asked_store, _) = built(false);
        let handler_store = asked_store.clone();
        assert!(window_signal::serve(hwnd as isize, Role::Overlay, move |signal| {
            take_wish(signal, &handler_store);
        }));
        let pump = || {
            // SAFETY: takes the messages of this thread and hands them to their windows.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&raw const message);
                }
            }
        };
        for wish in [true, false] {
            assert!(window_signal::send(hwnd as isize, anti_afk::signal_for(wish)), "posted");
            pump();
            assert_eq!(afk.step(|| {}), [News::Switched(wish)], "the frame turns the wish into a switching");
            assert_eq!(afk.enabled(), wish);
        }
        // A signal that is none of anti-AFK's business changes nothing.
        assert!(window_signal::send(hwnd as isize, Signal::Overview));
        pump();
        assert!(afk.step(|| {}).is_empty());
        // SAFETY: the window made above.
        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn the_beat_presses_on_its_own_and_a_stop_comes_as_news_with_its_reason_and_wakes_the_window() {
        let (mut afk, _, platform) = built(false);
        afk.toggle();
        // Nobody asks for a frame: the thread of the beat presses.
        assert!(wait_for(|| platform.pressed.load(Ordering::SeqCst) >= 1), "pressed");
        assert!(afk.step(|| {}).is_empty(), "pressed, nothing to tell");
        // The game runs as administrator: the beat stops with the reason.
        let (mut blocked, _, _) = built(true);
        let woken = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = woken.clone();
        assert!(
            blocked
                .step(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
                .is_empty()
        );
        blocked.toggle();
        assert!(wait_for(|| !blocked.enabled()), "the beat switched it off");
        assert!(woken.load(Ordering::SeqCst) >= 1, "the window is woken");
        assert_eq!(blocked.step(|| {}), [News::Stopped(anti_afk::ERR_ADMIN)]);
        assert!(blocked.step(|| {}).is_empty(), "told once");
        assert_eq!(blocked.extra().map(|e| e.icon), Some("timer-off"));
    }

    #[test]
    fn closing_switches_off() {
        let (afk, _, _) = built(false);
        afk.toggle();
        assert!(afk.enabled());
        afk.shutdown();
        assert!(!afk.enabled());
    }

    fn language_de() -> std::sync::MutexGuard<'static, ()> {
        // The language of the process is one: the texts compared here are the German ones.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        versekit_core::language::set_language(Choice::De);
        guard
    }
}
