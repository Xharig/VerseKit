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

//! The block "Anti-AFK" on the page „Spiel“ (`pages._anti_afk_section` of 4.12): a switch with its explanation,
//! and the note of risk below it. The switch is off after every start and is never stored.
//!
//! In 4.12 the page switches the state of its own process. Here the keys are pressed by the overlay (a process of
//! its own, see `versekit_core::anti_afk`): the page *asks* it by signal and shows what the overlay wrote into the
//! data folder. Where the overlay does not run, the page says so and the switch stays as it is.
//!
//! The line under the switch says what the file says, and why it does not press right now ([`AfkBlock::status`]);
//! what a click said stands there only for a moment, so it never contradicts the switch.

use std::sync::Arc;
use std::time::Duration;

use egui::Ui;
use versekit_core::anti_afk::{self, Look, Reason, Snapshot};
use versekit_core::anti_afk_key::{self, Chord, Origin};
use versekit_core::language::{Arg, t, tf};
use versekit_core::stick_input::Source;
use versekit_core::theme::Scheme;
use versekit_core::window_signal::{self, Role};

use super::{GameTextsState, hint};
use crate::overlay::Tone;
use crate::widgets;

/// How long the page believes its own click before it believes the file again (seconds): the overlay takes the
/// signal in its next frame and writes the file then.
const BELIEVE_CLICK: f64 = 3.0;
/// How long the page shows that a click did not reach the overlay (seconds).
const SHOW_REFUSAL: f64 = 10.0;
/// How often the page looks at the file (seconds).
const LOOK_SECONDS: f64 = 1.0;

/// What came of asking the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// The signal went to the overlay.
    Sent,
    /// No overlay of this desktop runs.
    NoOverlay,
    /// An overlay runs but does not take the signal (it runs with higher rights than this window; Windows
    /// does not let a window of lower rights post to it).
    Refused,
}

/// Asks the overlay to switch (`true` = on).
pub(super) type Ask = Arc<dyn Fn(bool) -> Asked + Send + Sync>;

/// The real way: find the overlay, post the signal.
fn ask_overlay(on: bool) -> Asked {
    match window_signal::find(Role::Overlay) {
        None => Asked::NoOverlay,
        Some(hwnd) if window_signal::send(hwnd, anti_afk::signal_for(on)) => Asked::Sent,
        Some(_) => Asked::Refused,
    }
}

/// What the block remembers.
pub(super) struct AfkBlock {
    pub(super) snapshot: Snapshot,
    pub(super) looked_at: Option<f64>,
    /// The overlay of this desktop is there (looked at together with the file).
    overlay_there: bool,
    /// What the last click wished and until when the page shows it before the file confirms.
    pub(super) clicked: Option<(bool, f64)>,
    /// What the last click said (text, is it a refusal), and until when it stands under the switch.
    pub(super) said: Option<(String, bool)>,
    said_until: f64,
    ask: Ask,
    overlay_runs: Arc<dyn Fn() -> bool + Send + Sync>,
    supported: bool,
    /// The seconds of the wait as the setting holds them (read together with the file).
    pub(super) seconds: i64,
    /// The keys a press sends, from the binding of the game (read together with the file).
    pub(super) chord: Chord,
    chords: anti_afk_key::Cached,
    /// Do keys sent by a program count as input (the setting, read together with the file)?
    pub(super) count_injected: bool,
}

impl Default for AfkBlock {
    fn default() -> Self {
        AfkBlock {
            snapshot: Snapshot::default(),
            looked_at: None,
            overlay_there: false,
            clicked: None,
            said: None,
            said_until: 0.0,
            ask: Arc::new(ask_overlay),
            overlay_runs: Arc::new(|| window_signal::find(Role::Overlay).is_some()),
            supported: anti_afk::supported(),
            seconds: anti_afk::INTERVAL as i64,
            chord: Chord::default(),
            chords: anti_afk_key::Cached::default(),
            count_injected: true,
        }
    }
}

impl AfkBlock {
    /// With stand-ins for the overlay and for the system (tests).
    #[cfg(test)]
    pub(super) fn with_parts(ask: Ask, overlay_runs: Arc<dyn Fn() -> bool + Send + Sync>, supported: bool) -> AfkBlock {
        AfkBlock { ask, overlay_runs, supported, ..AfkBlock::default() }
    }

    /// The state the switch shows.
    pub(super) fn shown_on(&self, now: f64) -> bool {
        match self.clicked {
            Some((on, until)) if now < until => on,
            _ => self.snapshot.on && self.overlay_there,
        }
    }

    fn look(&mut self, paths: &versekit_core::paths::Paths, now: f64) {
        if self.looked_at.is_some_and(|at| now - at < LOOK_SECONDS) {
            return;
        }
        self.looked_at = Some(now);
        self.snapshot = Snapshot::read(paths);
        self.overlay_there = (self.overlay_runs)();
        self.seconds = anti_afk::interval_for(paths) as i64;
        self.chord = self.chords.get(paths);
        self.count_injected = anti_afk::injected_count(paths);
    }

    /// A click on the switch.
    fn clicked(&mut self, wish: bool, now: f64) {
        match (self.ask)(wish) {
            Asked::Sent => {
                self.clicked = Some((wish, now + BELIEVE_CLICK));
                // Look at the file again soon: the overlay writes it when it has switched.
                self.looked_at = None;
                let word = t(if wish { "e_an" } else { "e_aus" });
                self.said = Some((tf("aafk_sagen", &[("0", Arg::from(word))]), false));
                self.said_until = now + BELIEVE_CLICK;
            }
            Asked::NoOverlay => {
                self.clicked = None;
                self.said = Some((t("aafk_kein_overlay"), true));
                self.said_until = now + SHOW_REFUSAL;
            }
            Asked::Refused => {
                self.clicked = None;
                self.said = Some((t("aafk_abgelehnt"), true));
                self.said_until = now + SHOW_REFUSAL;
            }
        }
    }

    /// The state the switch shows at `now` (the time of the page) and `wall` (seconds since 1970): what a click
    /// wished, for a moment; off where no overlay runs; else what the overlay wrote.
    pub(super) fn state(&self, now: f64, wall: i64) -> Look {
        match self.clicked {
            Some((on, until)) if now < until => {
                if on {
                    Look::Waiting
                } else {
                    Look::Off
                }
            }
            _ if !self.overlay_there => Look::Off,
            _ => self.snapshot.look(wall),
        }
    }

    /// The line under the switch at `now` (the time of the page) and `wall` (seconds since 1970): what the last
    /// click said, for a moment; then what the overlay wrote. (Text, is it a fault.)
    pub(super) fn status(&self, now: f64, wall: i64) -> (String, bool) {
        if let Some(said) = self.said.as_ref().filter(|_| now < self.said_until) {
            return said.clone();
        }
        let said = |on: bool| tf("aafk_sagen", &[("0", Arg::from(t(if on { "e_an" } else { "e_aus" })))]);
        if !self.overlay_there {
            return (t("aafk_kein_overlay"), false);
        }
        let snapshot = &self.snapshot;
        if !snapshot.on {
            return match &snapshot.stop {
                Some(key) => (t(key), true),
                None => (said(false), false),
            };
        }
        let ago = |at: Option<i64>| Arg::from((wall - at.unwrap_or(wall)).max(0));
        let (reason, fault) = match snapshot.reason {
            None => return (said(true), false),
            Some(Reason::NotInFront) => (t("aafk_grund_vorne"), false),
            Some(Reason::Blind) => (t("aafk_grund_blind"), true),
            Some(Reason::Pressed) => (tf("aafk_grund_gedrueckt", &[("0", ago(snapshot.pressed_at))]), false),
            Some(Reason::Waiting) => match snapshot.input_source {
                Some(source) => (
                    tf("aafk_grund_eingabe", &[("0", ago(snapshot.input_at)), ("1", Arg::from(t(source_key(source))))]),
                    false,
                ),
                None => (tf("aafk_grund_still", &[("0", ago(snapshot.input_at))]), false),
            },
        };
        (tf("aafk_stand_an", &[("0", Arg::from(reason))]), fault)
    }
}

/// The text key of what an input came from.
fn source_key(source: Source) -> &'static str {
    source.text_key()
}

/// The text key of where the keys of a press come from.
fn origin_key(origin: Origin) -> &'static str {
    match origin {
        Origin::Own => "aafk_taste_eigene",
        Origin::Default => "aafk_taste_standard",
        Origin::Fallback => "aafk_taste_ersatz",
    }
}

/// Seconds since 1970, now.
fn wall_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// The block: the switch with its explanation (or, where the system cannot do it, the sentence that says so),
/// and the note of risk.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    let Some(paths) = state.paths.clone() else { return };
    let now = ui.input(|i| i.time);
    let block = &mut state.anti_afk;
    block.look(&paths, now);
    ui.ctx().request_repaint_after(Duration::from_secs_f64(LOOK_SECONDS));
    if block.supported {
        let wall = wall_now();
        // The switch and the line under it in the colour of the state; the hover says the state in words.
        let look = block.state(now, wall);
        let tone = Tone::of_anti_afk(look).tint(scheme, scheme.sub);
        let mut on = block.shown_on(now);
        let (title, help) = (t("aafk_titel"), t("aafk_h"));
        // The help on hover behind the symbol `info`, as in every row.
        let (switch, row) = widgets::switch_line_with(ui, scheme, &title, &help, &mut on, Some(tone));
        if switch.on_hover_text(t(look.hint_key())).changed() {
            block.clicked(on, now);
        }
        let (text, fault) = block.status(now, wall);
        let shade = if fault { scheme.red_text() } else { tone };
        let state_line = ui.add(egui::Label::new(egui::RichText::new(text).color(crate::color(shade))).wrap());
        // A jump from the menu filter to the switch or to its help frames it with its state.
        crate::highlight::mark(ui, scheme, row.union(state_line.rect), &[title.as_str(), help.as_str()]);
        ui.add_space(8.0);
        // The wait: the overlay reads the setting at its next beat.
        let mut seconds = block.seconds;
        let changed = crate::settings::setting_row(ui, scheme, &t("aafk_warten"), &t("aafk_warten_h"), false, |row| {
            widgets::number_field(
                row,
                scheme,
                "anti-afk-sekunden",
                &mut seconds,
                anti_afk::INTERVAL_RANGE,
                &t("sekunden"),
            )
        })
        .inner;
        if changed && paths.set_setting(anti_afk::SETTING_INTERVAL, versekit_core::jsonio::Value::int(seconds)) {
            block.seconds = seconds;
        }
        // The key a press sends: the binding of "wipe visor" in the game, and where it comes from.
        let shown =
            versekit_core::controls::input_readable(&block.chord.input, Some(versekit_core::joysticks::Kind::Keyboard));
        crate::settings::setting_row(ui, scheme, &t("aafk_taste"), &t(origin_key(block.chord.origin)), false, |row| {
            row.add(egui::Label::new(egui::RichText::new(shown).strong()))
        });
        // Whether keys sent by a program count as input: the overlay reads the setting at its next beat.
        let name = t("aafk_makro");
        let mut count = block.count_injected;
        crate::settings::setting_row(ui, scheme, &name, &t("aafk_makro_h"), false, |row| {
            let switch = row.add(widgets::Toggle::new(&mut count, scheme));
            let (enabled, on) = (switch.enabled(), count);
            switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, &name));
            if switch.changed()
                && paths.set_setting(anti_afk::SETTING_INJECTED, versekit_core::jsonio::Value::Bool(count))
            {
                block.count_injected = count;
            }
        });
        ui.add_space(8.0);
    } else {
        widgets::card_title(ui, scheme, &t("aafk_titel"));
        hint(ui, scheme, &t("aafk_h"));
        hint(ui, scheme, &t("aafk_nur_windows"));
        ui.add_space(8.0);
    }
    widgets::status_box(ui, scheme, scheme.sub, "info", &t("aafk_hinweis"), &t("aafk_hinweis_h"));
}
