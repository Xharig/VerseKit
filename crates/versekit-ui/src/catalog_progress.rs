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

//! How the windows show the build of the blueprint catalogue: the foot bar of the main window, the empty state of the
//! list, and a strip over a list that is shown while it is rebuilt. The numbers come from
//! `versekit_core::build_progress`; a build of another process (the overlay keeps the catalogue current) is seen
//! through its file.

use std::time::Duration;

use egui::{Context, RichText, Ui};
use versekit_core::build_progress::{self, Snapshot, Step};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{IconTint, Scheme};

use crate::{color, icons, widgets};

/// How often the file of another process is looked at while a build runs, and while none does (seconds).
const BUSY_LOOK: f64 = 1.0;
const IDLE_LOOK: f64 = 4.0;

/// Width of the bar in the empty state and in the strip.
const BLOCK_BAR: f32 = 360.0;
const STRIP_BAR: f32 = 160.0;

/// Looks at the build of the catalogue: the own one every frame, one of another process by its file now and then.
#[derive(Default)]
pub struct BuildWatch {
    next: f64,
    seen: Option<Snapshot>,
    own: bool,
    /// A state given from outside; then neither this process's build nor the file of another replaces it.
    given: bool,
}

impl BuildWatch {
    /// Takes the state of the build; call it on every frame. Asks for a frame by itself while a build runs and, now
    /// and then, while none does (a build may begin in the overlay).
    pub fn poll(&mut self, ctx: &Context, paths: Option<&Paths>) {
        if self.given {
            return;
        }
        let now = ctx.input(|i| i.time);
        if let Some(own) = build_progress::snapshot() {
            self.seen = Some(own);
            self.own = true;
            self.next = now + BUSY_LOOK;
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        // A run of this process that just ended is gone at once, not after the next look at the file.
        if std::mem::take(&mut self.own) {
            self.seen = None;
            self.next = now;
        }
        if now >= self.next {
            self.seen = paths.and_then(build_progress::snapshot_in);
            self.next = now + if self.seen.is_some() { BUSY_LOOK } else { IDLE_LOOK };
        }
        let wait = if self.seen.is_some() { Duration::from_millis(500) } else { Duration::from_secs_f64(IDLE_LOOK) };
        ctx.request_repaint_after(wait);
    }

    /// The build that runs now, with what it has done.
    pub fn seen(&self) -> Option<&Snapshot> {
        self.seen.as_ref()
    }

    /// A state given from outside (a check, a picture).
    pub fn set(&mut self, seen: Option<Snapshot>) {
        self.seen = seen;
        self.next = f64::MAX;
        self.given = true;
    }
}

/// `3:07` for 187 seconds.
pub fn elapsed_text(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `Catalogue: step 4 of 9 · Reading the contracts`.
pub fn step_line(snapshot: &Snapshot) -> String {
    tf(
        "kat_fortschritt_zeile",
        &[
            ("0", Arg::from(snapshot.step.number())),
            ("1", Arg::from(Step::count())),
            ("2", Arg::from(t(snapshot.step.key()))),
        ],
    )
}

/// What the foot bar shows while the catalogue is built: the line and how far it is, 0 … 1. While the build only
/// waits for the archive to be readable, the line says what it waits for and there is no bar.
pub fn foot(snapshot: &Snapshot) -> (String, Option<f32>) {
    if snapshot.waiting {
        (t("kat_wartet_spiel"), None)
    } else {
        (step_line(snapshot), Some(snapshot.fraction as f32))
    }
}

/// The highlighted line that says the catalogue is read once the game is closed.
pub fn waiting_line(ui: &mut Ui, scheme: &Scheme, text: &str) {
    ui.horizontal_wrapped(|row| {
        if let Some(image) = icons::themed("clock", scheme, IconTint::Highlight, 14.0) {
            row.add(image);
        }
        row.label(RichText::new(text).strong().color(color(scheme.accent)));
    });
}

/// The bar with the percent, the step and the time, in the middle of an empty list.
pub fn block(ui: &mut Ui, scheme: &Scheme, snapshot: &Snapshot) {
    ui.add_space(4.0);
    ui.vertical_centered(|ui| {
        widgets::progress(ui, scheme, snapshot.fraction as f32, BLOCK_BAR, &t("kat_fortschritt_kopf"));
        ui.label(RichText::new(step_line(snapshot)).color(color(scheme.fg)));
        let since = tf("kat_fortschritt_dauer", &[("0", Arg::from(elapsed_text(snapshot.elapsed)))]);
        ui.label(RichText::new(since).small().color(color(scheme.sub)));
    });
    ui.add_space(4.0);
}

/// One line over a list that is shown while it is rebuilt: what happens, how far, which step.
pub fn strip(ui: &mut Ui, scheme: &Scheme, snapshot: &Snapshot) {
    ui.horizontal_wrapped(|row| {
        if let Some(image) = icons::themed("database", scheme, IconTint::Highlight, 14.0) {
            row.add(image);
        }
        row.label(RichText::new(t("kat_fortschritt_kopf")).color(color(scheme.sub)));
        widgets::progress(row, scheme, snapshot.fraction as f32, STRIP_BAR, &t("kat_fortschritt_kopf"));
        let line = format!(
            "{} · {}",
            step_line(snapshot),
            tf("kat_fortschritt_dauer", &[("0", Arg::from(elapsed_text(snapshot.elapsed)))])
        );
        row.label(RichText::new(line).small().color(color(scheme.sub)));
    });
}

/// How a build ended, for a few seconds under the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ended {
    /// The catalogue came; how many blueprints it holds.
    pub blueprints: Option<usize>,
    /// When it ended (`ctx` time).
    pub at: f64,
}

/// How long the end of a build stays on the page (seconds).
pub const ENDED_SECONDS: f64 = 12.0;

/// The end of a build as one line with a mark: the check mark for a catalogue, the warning for none. `false`
/// (and nothing drawn) once [`ENDED_SECONDS`] are over.
pub fn ended(ui: &mut Ui, scheme: &Scheme, ended: &Ended) -> bool {
    let now = ui.input(|i| i.time);
    let age = now - ended.at;
    if !(0.0..ENDED_SECONDS).contains(&age) {
        return false;
    }
    ui.ctx().request_repaint_after(Duration::from_secs_f64(ENDED_SECONDS - age));
    ui.horizontal_wrapped(|row| match ended.blueprints {
        Some(count) => {
            if let Some(image) = icons::themed("circle-check", scheme, IconTint::Highlight, 14.0) {
                row.add(image);
            }
            row.label(RichText::new(tf("kat_fertig", &[("0", Arg::from(count))])).color(color(scheme.sub)));
        }
        None => {
            if let Some(image) = icons::themed("triangle-alert", scheme, IconTint::Highlight, 14.0) {
                row.add(image);
            }
            row.label(RichText::new(t("kat_fehler")).color(color(scheme.red_text())));
        }
    });
    true
}

#[cfg(test)]
mod tests;
