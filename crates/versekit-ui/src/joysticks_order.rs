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

//! The block "Reihenfolge der Sticks" of the page "Steuerung": the order the player remembered, what is shifted
//! against it, a preview of what would move, the button that puts it right and the way back. The logic is
//! [`versekit_core::joystick_order`]; this part only draws and says what the player asked for.
//!
//! Nothing is written without a click, and the click that writes into the game goes over a question that names
//! the number of lines. Writing down the order touches an own file of the player only, so it is not held back
//! while the game runs.

use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::joystick_order::{Applied, Check};
use versekit_core::joysticks::FileState;
use versekit_core::language::SpawnWithLanguage as _;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::joysticks_page::{GAP, Row, block_title, list_row, small, text_width};
use crate::widgets::{self, ButtonKind};
use crate::{blickwinkel_page, color};

/// What the player asked for with a click in the block.
pub(crate) enum OrderAct {
    /// Write down the order the sticks have now.
    Remember,
    /// Show the question before the correction.
    Ask,
    /// Take the question back.
    Cancel,
    /// The yes: correct.
    Confirm,
    /// Take the last correction back.
    Undo,
}

/// What the block needs to know to draw itself.
pub(crate) struct View<'a> {
    pub(crate) check: &'a Check,
    /// Why nothing may be written into the game now; `None` when it is quiet.
    pub(crate) quiet: Option<&'a str>,
    pub(crate) file_state: FileState,
    /// Whether the binding names a stick at all.
    pub(crate) has_sticks: bool,
    /// The question is on the screen.
    pub(crate) asking: bool,
    pub(crate) undo: Option<&'a Applied>,
}

/// The question under the button and its two answers.
fn question(ui: &mut Ui, scheme: &Scheme, text: &str) -> Option<OrderAct> {
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(GAP / 2.0);
    let mut answer = None;
    ui.horizontal_wrapped(|ui| {
        if ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked() {
            answer = Some(OrderAct::Cancel);
        }
        if ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Strong, scheme)).clicked() {
            answer = Some(OrderAct::Confirm);
        }
    });
    answer
}

fn bindings_text(count: usize) -> String {
    if count == 1 { t("s_jr_belegung_eins") } else { tf("s_jr_belegungen", &[("0", Arg::from(i64::try_from(count).unwrap_or(i64::MAX)))]) }
}

fn joined(names: impl Iterator<Item = String>) -> String {
    names.collect::<Vec<_>>().join(", ")
}

fn shown(name: &str, id: &str) -> String {
    if name.is_empty() { id.split('-').next().unwrap_or("").to_string() } else { name.to_string() }
}

/// Draws the block; gives what the player clicked.
pub(crate) fn show(ui: &mut Ui, scheme: &Scheme, view: &View) -> Option<OrderAct> {
    let mut act = None;
    block_title(ui, scheme, &t("s_jr_titel"));
    small(ui, scheme, &t("s_jr_lead"));
    ui.add_space(GAP);
    // Writing down needs a binding that names a stick; the reason is told beside the button.
    let no_sticks = match view.file_state {
        FileState::Unreadable => Some(t("s_js_datei_gesperrt")),
        FileState::Missing => Some(t("s_js_keine_datei")),
        FileState::Fine if !view.has_sticks => Some(t("s_js_keine_datei")),
        FileState::Fine => None,
    };
    let check = view.check;
    let Some(remembered) = &check.remembered else {
        small(ui, scheme, &t("s_jr_nichts"));
        ui.add_space(GAP / 2.0);
        let label = t("s_jr_merken");
        let button = widgets::Button::new(&label, ButtonKind::Strong, scheme).blocked(no_sticks.as_deref());
        if ui.add(button).clicked() {
            act = Some(OrderAct::Remember);
        }
        if let Some(reason) = &no_sticks {
            small(ui, scheme, reason);
        }
        return act;
    };
    let count = i64::try_from(remembered.slots.len()).unwrap_or(i64::MAX);
    small(ui, scheme, &tf("s_jr_gemerkt", &[("0", Arg::from(remembered.stand.as_str())), ("1", Arg::from(count))]));
    ui.add_space(GAP / 2.0);
    let plan = &check.plan;
    if plan.is_shifted() {
        blickwinkel_page::verdict_box(ui, scheme, scheme.gold, "info", &t("s_jr_anders"), "");
        ui.add_space(GAP / 2.0);
        // The preview: what is moved, with how many lines. It stands before the question and in it.
        let states: Vec<String> = plan.shifts.iter().map(|s| bindings_text(s.bindings)).collect();
        let number = text_width(ui, "js00 → js00", egui::TextStyle::Body) + 8.0;
        let widest = states.iter().map(|s| text_width(ui, s, egui::TextStyle::Small)).fold(0.0_f32, f32::max);
        let widths = (number, widest.min(ui.available_width() * 0.35));
        for (shift, state) in plan.shifts.iter().zip(&states) {
            let name = shown(&shift.name, &shift.id);
            let number_text = format!("js{} → js{}", shift.from, shift.to);
            let row = Row {
                tone: scheme.gold,
                spoken: tf(
                    "s_jr_zeile_gesprochen",
                    &[
                        ("0", Arg::from(name.as_str())),
                        ("1", Arg::from(shift.from)),
                        ("2", Arg::from(shift.to)),
                        ("3", Arg::from(state.as_str())),
                    ],
                ),
                number: number_text,
                number_bound: true,
                name,
                sub: shift.id.split('-').next().unwrap_or("").to_string(),
                state: state.clone(),
                help: String::new(),
                salt: format!("order-{}", shift.id),
            };
            list_row(ui, scheme, &row, widths);
        }
        ui.add_space(GAP / 2.0);
        if plan.conflict {
            small(ui, scheme, &t("s_jr_widerspruch"));
        } else if view.asking {
            let lines = i64::try_from(plan.lines()).unwrap_or(i64::MAX);
            act = question(ui, scheme, &tf("s_jr_frage", &[("0", Arg::from(lines))]));
        } else {
            let label = t("s_jr_berichtigen");
            let button = widgets::Button::new(&label, ButtonKind::Strong, scheme).blocked(view.quiet);
            if ui.add(button).clicked() {
                act = Some(OrderAct::Ask);
            }
            if let Some(reason) = view.quiet {
                small(ui, scheme, reason);
            }
        }
    } else if plan.conflict {
        small(ui, scheme, &t("s_jr_widerspruch"));
    } else if let Some(reason) = &no_sticks {
        small(ui, scheme, reason);
    } else {
        ui.add(egui::Label::new(RichText::new(t("s_jr_passt")).small().color(color(scheme.accent))).wrap());
    }
    if !plan.not_found.is_empty() {
        let names = joined(plan.not_found.iter().map(|slot| shown(&slot.name, &slot.id)));
        small(ui, scheme, &tf("s_jr_nicht_gefunden", &[("0", Arg::from(names.as_str()))]));
    }
    if !plan.twins.is_empty() {
        let names = joined(plan.twins.iter().map(|slot| shown(&slot.name, &slot.id)));
        small(ui, scheme, &tf("s_jr_zwillinge", &[("0", Arg::from(names.as_str()))]));
    }
    ui.add_space(GAP / 2.0);
    ui.horizontal_wrapped(|ui| {
        let label = t("s_jr_merken_neu");
        let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).blocked(no_sticks.as_deref());
        if ui.add(button).on_hover_text(t("s_jr_merken_hilfe")).clicked() {
            act = Some(OrderAct::Remember);
        }
        if view.undo.is_some() {
            let label = t("s_jr_rueckgaengig");
            let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).blocked(view.quiet);
            if ui.add(button).on_hover_text(t("s_jr_rueck_hilfe")).clicked() {
                act = Some(OrderAct::Undo);
            }
        }
    });
    if view.undo.is_some()
        && let Some(reason) = view.quiet
    {
        small(ui, scheme, reason);
    }
    act
}

// -------------------------------------------------------------- at the start

/// The look at the start of the program: is the order of the sticks as remembered? Done once, in a thread of its
/// own, and it only **says** so; nothing is ever rewritten by it.
#[derive(Default)]
pub struct StartHint {
    started: bool,
    job: Option<Receiver<bool>>,
}

impl StartHint {
    /// Starts the look the first time it is called with a place to look at, and gives the text for the foot when
    /// the look has found the sticks shifted. Cheap to call at every frame.
    pub fn poll(&mut self, paths: Option<&Paths>) -> Option<String> {
        if !self.started {
            let paths = paths?;
            self.started = true;
            let (sender, receiver) = mpsc::channel();
            let copy = paths.working_copy();
            let spawned = std::thread::Builder::new().name("joystick-order-start".into()).spawn_with_language(move || {
                let shifted = versekit_core::joystick_order::check(&copy, None, None).plan.is_shifted();
                let _ = sender.send(shifted);
            });
            if spawned.is_ok() {
                self.job = Some(receiver);
            }
        }
        let receiver = self.job.as_ref()?;
        match receiver.try_recv() {
            Ok(shifted) => {
                self.job = None;
                shifted.then(|| t("s_jr_hinweis_start"))
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                None
            }
        }
    }

    /// Is the look still on its way? (The window must be drawn again to take its answer.)
    pub fn waiting(&self) -> bool {
        self.job.is_some()
    }
}
