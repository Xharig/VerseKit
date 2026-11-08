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

//! The page "Shader-Cache" (`_shader_cache` of 4.12): empties the shader caches of Star Citizen and of
//! the graphics drivers, **only while the game does not run**.
//!
//! Two ways start it: "Nur alte Shader löschen" (the recommended one: the version folders of the game that are
//! older than the installed version, nothing else) and "Alles löschen" (every version, the drivers, the crash
//! reports). Both look first (in a thread of its own) and show what was found as one switch per place with its
//! size; the player picks, a yes empties (in a thread of its own), a window tells for each place what was freed.
//! Searching and deleting can take seconds with many thousand files, so neither happens in the frame.
//!
//! ⚠ This is deleting, final and without a copy. Nothing is deleted when the game runs **or when nobody can
//! say** (no Game.log known): the safe reading decided by the main session. The check is made at the click
//! and once more after the yes. "Nur alte" lists only version folders, offers a switch only at those called old,
//! and asks the core again right before deleting: the installed version is never touched, and neither is a
//! folder whose version cannot be told.

use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Role, Ui, WidgetInfo};
use versekit_core::game_start::{self, RunState};
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::shader_cache::{self, Age, Branches, Cleared, Place, PlaceKind};
use versekit_core::theme::Scheme;

use crate::color;
use crate::layout;
use crate::settings::Said;
use crate::widgets::{self, ButtonKind};

/// What the player asked to empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// The version folders older than the installed version.
    OnlyOld,
    /// Every version, the drivers and the crash reports.
    Everything,
}

/// The answer of the search.
struct Found {
    places: Vec<Place>,
    /// The installed versions were read for sure (only then something can be old).
    sure: bool,
}

/// What is asked: the places and which of them are switched on.
struct Asked {
    scope: Scope,
    places: Vec<Place>,
    chosen: Vec<bool>,
}

impl Asked {
    /// The indices of the places switched on.
    fn picked(&self) -> Vec<usize> {
        (0..self.places.len()).filter(|&i| self.chosen[i]).collect()
    }

    fn picked_bytes(&self) -> u64 {
        self.picked().into_iter().map(|i| self.places[i].bytes).sum()
    }
}

/// Where the page is.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The caches are being looked for.
    Searching(Scope, Receiver<Found>),
    /// Found; the player is asked.
    Asking(Asked),
    /// The emptying runs; the places are kept to name the result.
    Clearing(Vec<Place>, Receiver<Vec<(usize, Cleared)>>),
}

/// What the page keeps.
#[derive(Default)]
pub struct ShaderState {
    phase: Phase,
    /// A window with a text to read (the game runs, or the result), and its title.
    result: Option<String>,
    /// The line for the foot of the window (a notice).
    notice: Option<Said>,
    /// The size of all caches found, in bytes, as last measured.
    size: Option<u64>,
    /// The measuring under way.
    size_job: Option<Receiver<u64>>,
    /// When the size was last measured (the time of the frame); `None`: due.
    measured_at: Option<f64>,
    /// Why nothing may be emptied now (the game runs, or nobody can say), as last looked, and when.
    quiet: Option<String>,
    quiet_at: Option<f64>,
}

/// How often the page looks whether the game runs, in seconds (not in every frame: it reads the process list).
const QUIET_LOOK: f64 = 2.0;

/// Why the two ways to start are grey now: the game runs (say to quit it), or nobody can say.
fn start_blocked(paths: &Paths) -> Option<String> {
    match game_start::run_state(paths) {
        RunState::Idle => None,
        RunState::Running => Some(t("s_gf_spiel")),
        RunState::Unknown => Some(t("s_sc_unklar")),
    }
}

/// The setting with the moment the caches were last emptied through VerseKit (seconds since 1970).
pub const SETTING_CLEARED: &str = "shader_zuletzt_geleert";
/// The size is measured again after this long on the page, in seconds.
const MEASURE_AGAIN: f64 = 60.0;

/// When the caches were last emptied through VerseKit.
fn cleared_at(paths: &Paths) -> Option<i64> {
    match paths.settings().into_iter().find(|(k, _)| k == SETTING_CLEARED).map(|(_, v)| v)? {
        Value::Int(n) => n.parse().ok(),
        _ => None,
    }
}

/// When the game was last patched: the time its archive `Data.p4k` was last written.
fn patched_at(paths: &Paths) -> Option<i64> {
    let archive = std::path::Path::new(&paths.game_folder()?).join("Data.p4k");
    let written = std::fs::metadata(archive).ok()?.modified().ok()?;
    Some(i64::try_from(written.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()).unwrap_or(i64::MAX))
}

/// The line under the lead: the size of the caches and when they were last emptied; and, where they were emptied
/// before the last patch of the game, a sentence that says so.
fn state_lines(ui: &mut Ui, scheme: &Scheme, state: &ShaderState, paths: &Paths) {
    let size = match state.size {
        Some(bytes) => tf("s_sc_groesse", &[("0", Arg::from(size_text(bytes).as_str()))]),
        None => t("s_sc_misst"),
    };
    let cleared = cleared_at(paths);
    let when = match cleared {
        Some(seconds) => {
            let day = versekit_core::stats_report::local_day(versekit_core::language::current(), seconds);
            tf("s_sc_geleert_am", &[("0", Arg::from(day.as_str()))])
        }
        None => t("s_sc_nie_geleert"),
    };
    ui.label(format!("{size} · {when}"));
    if let (Some(cleared), Some(patched)) = (cleared, patched_at(paths))
        && cleared < patched
    {
        ui.label(RichText::new(t("s_sc_seit_patch")).color(color(scheme.gold)));
    }
    ui.add_space(8.0);
}

impl ShaderState {
    /// Measures the size of the caches in a thread of its own: when the card is first drawn, a while later again,
    /// and after emptying. Never while a search or an emptying runs.
    fn measure(&mut self, ui: &Ui, paths: &Paths) {
        if let Some(job) = &self.size_job {
            match job.try_recv() {
                Ok(bytes) => {
                    self.size = Some(bytes);
                    self.size_job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(mpsc::TryRecvError::Disconnected) => self.size_job = None,
            }
            return;
        }
        let now = ui.input(|i| i.time);
        let due = self.measured_at.is_none_or(|at| !(0.0..MEASURE_AGAIN).contains(&(now - at)));
        if !due || self.busy() {
            return;
        }
        self.measured_at = Some(now);
        let (sender, receiver) = mpsc::channel();
        let copy = paths.working_copy();
        versekit_core::language::spawn_with_language(move || {
            let branches = Branches::read(&copy);
            let places = shader_cache::find_places(
                &shader_cache::sc_cache_roots(&copy),
                &shader_cache::gpu_cache_dirs(&copy),
                &branches,
            );
            let _ = sender.send(places.iter().map(|place| place.bytes).sum());
        });
        self.size_job = Some(receiver);
    }

    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Is a job under way (the buttons wait)?
    fn busy(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Is the page at rest (the tests ask)?
    #[cfg(test)]
    fn phase_is_idle(&self) -> bool {
        !self.busy()
    }
}

/// The reason nothing is deleted now, if there is one.
fn blocked(paths: &Paths) -> Option<String> {
    match game_start::run_state(paths) {
        RunState::Idle => None,
        RunState::Running => Some(t("s_sc_spiel")),
        RunState::Unknown => Some(t("s_sc_unklar")),
    }
}

/// The decimal sign of the language now.
fn decimal() -> char {
    if versekit_core::language::current() == versekit_core::language::Lang::De { ',' } else { '.' }
}

fn size_text(bytes: u64) -> String {
    shader_cache::format_size(bytes, decimal())
}

/// The name of a place as the list shows it.
fn place_name(place: &Place) -> String {
    let key = match place.kind {
        PlaceKind::Game => {
            let version = Arg::from(place.version());
            return if place.suffix.is_empty() {
                tf("s_sc_ort_sc", &[("0", version)])
            } else {
                tf("s_sc_ort_sc_n", &[("0", version), ("1", Arg::from(place.suffix.as_str()))])
            };
        }
        PlaceKind::Crash => "s_sc_ort_crash",
        PlaceKind::Nvidia => "s_sc_ort_nvidia",
        PlaceKind::Amd => "s_sc_ort_amd",
        PlaceKind::Intel => "s_sc_ort_intel",
        PlaceKind::DirectX => "s_sc_ort_dx",
        PlaceKind::Mesa => "s_sc_ort_mesa",
        PlaceKind::OtherDriver => "s_sc_ort_gpu",
    };
    t(key)
}

/// The words after the size of a place: what it is to the installed version, or that it is for all games.
fn place_tag(place: &Place, scope: Scope) -> Option<String> {
    let key = match (place.kind, place.age, scope) {
        (PlaceKind::Game, Age::Old, _) => "s_sc_tag_alt",
        (PlaceKind::Game, Age::Current, Scope::OnlyOld) => "s_sc_bleibt_aktuell",
        (PlaceKind::Game, Age::Current, Scope::Everything) => "s_sc_tag_aktuell",
        (PlaceKind::Game, Age::Unclear, Scope::OnlyOld) => "s_sc_bleibt_unklar",
        (PlaceKind::Game, Age::Unclear, Scope::Everything) => "s_sc_tag_unklar",
        (PlaceKind::Crash, ..) => return None,
        _ => "s_sc_tag_alle",
    };
    Some(t(key))
}

/// Can the player switch this place on in this list? In "nur alte" only an old version folder can be.
fn switchable(place: &Place, scope: Scope) -> bool {
    scope == Scope::Everything || (place.kind == PlaceKind::Game && place.age == Age::Old)
}

/// What the search found, turned into the question or into a notice that there is nothing to ask.
fn question_from(scope: Scope, found: Found) -> Result<Asked, String> {
    let mut places = found.places;
    places.retain(|p| p.bytes > 0);
    match scope {
        Scope::Everything => {
            if places.is_empty() {
                return Err(t("s_sc_leer"));
            }
            let chosen = vec![true; places.len()];
            Ok(Asked { scope, places, chosen })
        }
        Scope::OnlyOld => {
            places.retain(|p| p.kind == PlaceKind::Game);
            if places.is_empty() {
                return Err(t("s_sc_leer"));
            }
            if !places.iter().any(|p| p.age == Age::Old) {
                return Err(t(if found.sure { "s_sc_alt_keine" } else { "s_sc_alt_unklar" }));
            }
            let chosen = places.iter().map(|p| p.age == Age::Old).collect();
            Ok(Asked { scope, places, chosen })
        }
    }
}

/// The text of the result window: the total, then a line for each place.
fn result_text(places: &[Place], done: &[(usize, Cleared)]) -> String {
    let freed: u64 = done.iter().map(|(_, c)| c.freed).sum();
    let skipped: u64 = done.iter().map(|(_, c)| c.skipped).sum();
    let mut text = tf("s_sc_fertig", &[("0", Arg::from(size_text(freed).as_str()))]);
    text.push_str("\n\n");
    text.push_str(&t("s_sc_erg_kopf"));
    for (at, cleared) in done {
        let Some(place) = places.get(*at) else { continue };
        let name = place_name(place);
        let name = Arg::from(name.as_str());
        let size = size_text(cleared.freed);
        let size = Arg::from(size.as_str());
        let line = if cleared.kept {
            tf("s_sc_erg_blieb", &[("0", name)])
        } else if cleared.skipped == 0 {
            tf("s_sc_erg_zeile", &[("0", name), ("1", size)])
        } else if cleared.skipped == 1 {
            tf("s_sc_erg_rest_eins", &[("0", name), ("1", size)])
        } else {
            let count = Arg::from(i64::try_from(cleared.skipped).unwrap_or(i64::MAX));
            tf("s_sc_erg_rest", &[("0", name), ("1", size), ("2", count)])
        };
        text.push('\n');
        text.push_str(&line);
    }
    if skipped > 0 {
        text.push_str("\n\n");
        let key = if skipped == 1 { "s_sc_rest_eins" } else { "s_sc_rest" };
        text.push_str(&tf(key, &[("0", Arg::from(i64::try_from(skipped).unwrap_or(i64::MAX)))]));
    }
    text
}

/// One place of the list: its name, below it the size and what it is; a switch where the player may pick it.
fn place_row(ui: &mut Ui, scheme: &Scheme, place: &Place, scope: Scope, on: &mut bool) {
    let name = place_name(place);
    let size = size_text(place.bytes);
    let hint = match place_tag(place, scope) {
        Some(tag) => tf("s_sc_zeile", &[("0", Arg::from(size.as_str())), ("1", Arg::from(tag.as_str()))]),
        None => size,
    };
    let can = switchable(place, scope);
    // The size (and whether it stays) is what the row is about: it stands in the line, after the switch.
    crate::settings::setting_row(ui, scheme, &name, "", false, |row| {
        if can {
            let response = row.add(widgets::Toggle::new(on, scheme));
            let (enabled, now) = (response.enabled(), *on);
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, now, &name));
        }
        row.label(egui::RichText::new(&hint).small().color(crate::color(scheme.sub)));
    });
}

/// The card on its own, with its title (its tests draw it so; the window draws it as the last card of "Grafik").
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ShaderState, paths: &Paths) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_sc_titel"));
        show_body(ui, scheme, state, paths);
    });
}

/// What the card of the shader cache holds below its title, and moves the jobs on: what it is for, the two ways to
/// start, the question with the places found, the answer.
pub fn show_body(ui: &mut Ui, scheme: &Scheme, state: &mut ShaderState, paths: &Paths) {
    widgets::reading(ui, RichText::new(t("s_sc_lead")).small().color(color(scheme.sub)));
    ui.add_space(4.0);
    state.measure(ui, paths);
    state_lines(ui, scheme, state, paths);
    // A job that finished.
    match std::mem::take(&mut state.phase) {
        Phase::Searching(scope, receiver) => match receiver.try_recv() {
            Ok(found) => match question_from(scope, found) {
                Ok(asked) => state.phase = Phase::Asking(asked),
                Err(text) => state.notice = Some(Said::note(text)),
            },
            Err(mpsc::TryRecvError::Empty) => {
                state.phase = Phase::Searching(scope, receiver);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            // The thread is gone without an answer: say so, do not fall back to rest in silence.
            Err(mpsc::TryRecvError::Disconnected) => state.notice = Some(Said::fault(t("s_hl_fehler"))),
        },
        Phase::Clearing(places, receiver) => match receiver.try_recv() {
            Ok(done) => {
                state.result = Some(result_text(&places, &done));
                // Emptied now: the moment is kept, the size measured anew.
                paths.set_setting(SETTING_CLEARED, Value::int(versekit_core::export::now_unix()));
                state.measured_at = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                state.phase = Phase::Clearing(places, receiver);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            Err(mpsc::TryRecvError::Disconnected) => state.notice = Some(Said::fault(t("s_hl_fehler"))),
        },
        other => state.phase = other,
    }

    let mut start: Option<Scope> = None;
    let (mut cancelled, mut confirmed) = (false, false);
    // Whether the game runs, looked at every few seconds: while it runs the two ways to start are grey.
    let now = ui.input(|i| i.time);
    if state.quiet_at.is_none_or(|at| !(0.0..QUIET_LOOK).contains(&(now - at))) {
        state.quiet = start_blocked(paths);
        state.quiet_at = Some(now);
    }
    if state.quiet.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(QUIET_LOOK));
    }
    let busy_reason = state.busy().then(|| t("s_sc_laeuft_schon")).or_else(|| state.quiet.clone());
    {
        // The two ways side by side in a grid of equal cells, under each other when the window is narrow.
        layout::columns_or_stack(ui, START_CELL, 2, |ui, cell| {
            let (scope, name, hint, kind) = if cell == 0 {
                (Scope::OnlyOld, t("s_sc_knopf_alt"), t("s_sc_knopf_alt_h"), ButtonKind::Strong)
            } else {
                (Scope::Everything, t("s_sc_knopf"), t("s_sc_knopf_h"), ButtonKind::Danger)
            };
            let button = widgets::Button::new(&name, kind, scheme).wide().blocked(busy_reason.as_deref());
            if ui.add(button).clicked() {
                start = Some(scope);
            }
            widgets::hint(ui, scheme, &hint);
        });
        // A job under way: the wheel and what is done, as long as it takes.
        match &mut state.phase {
            Phase::Searching(..) => {
                ui.add_space(8.0);
                widgets::loading(ui, scheme, &t("s_sc_sucht"));
            }
            Phase::Clearing(..) => {
                ui.add_space(8.0);
                widgets::loading(ui, scheme, &t("s_sc_loescht"));
            }
            // The question stands in the card, no window of its own: what was found with a switch for each place,
            // then "Bestätigen".
            Phase::Asking(asked) => {
                ui.add_space(8.0);
                ui.label(t(if asked.scope == Scope::OnlyOld { "s_sc_alt_kopf" } else { "s_sc_frage_kopf" }));
                ui.add_space(6.0);
                let scope = asked.scope;
                for (place, on) in asked.places.iter().zip(asked.chosen.iter_mut()) {
                    place_row(ui, scheme, place, scope, on);
                }
                let picked = asked.picked();
                let chosen = tf("s_sc_gewaehlt", &[("0", Arg::from(size_text(asked.picked_bytes()).as_str()))]);
                ui.label(RichText::new(chosen).small().color(color(scheme.sub)));
                ui.add_space(4.0);
                ui.label(t("s_sc_frage"));
                ui.add_space(6.0);
                let nothing = picked.is_empty().then(|| t("s_sc_nichts_gewaehlt"));
                let (cancel_text, confirm_text) = (t("abbrechen"), t("s_mlg_bestaetigen"));
                layout::columns_or_stack(ui, ASK_CELL, 2, |ui, cell| {
                    if cell == 0 {
                        let button = widgets::Button::new(&cancel_text, ButtonKind::Normal, scheme).wide();
                        cancelled = ui.add(button).clicked();
                    } else {
                        let button = widgets::Button::new(&confirm_text, ButtonKind::Danger, scheme)
                            .wide()
                            .blocked(nothing.as_deref());
                        confirmed = ui.add(button).clicked();
                    }
                });
            }
            Phase::Idle => {}
        }
    }
    if let Some(scope) = start
        && !state.busy()
    {
        match blocked(paths) {
            Some(reason) => state.result = Some(reason),
            None => {
                let (sender, receiver) = mpsc::channel();
                let copy = paths.working_copy();
                versekit_core::language::spawn_with_language(move || {
                    let branches = Branches::read(&copy);
                    let places = shader_cache::find_places(
                        &shader_cache::sc_cache_roots(&copy),
                        &shader_cache::gpu_cache_dirs(&copy),
                        &branches,
                    );
                    let _ = sender.send(Found { places, sure: branches.is_sure() });
                });
                state.phase = Phase::Searching(scope, receiver);
            }
        }
    }
    if confirmed {
        // Once more at the yes: the game may have started while the question stood there.
        match blocked(paths) {
            Some(reason) => {
                state.result = Some(reason);
                state.phase = Phase::Idle;
            }
            None => {
                let Phase::Asking(asked) = std::mem::take(&mut state.phase) else { return };
                let (sender, receiver) = mpsc::channel();
                let copy = paths.working_copy();
                let (places, picked, scope) = (asked.places.clone(), asked.picked(), asked.scope);
                versekit_core::language::spawn_with_language(move || {
                    // "Nur alte": the installed versions are read again, the core empties only what is old now.
                    let branches = (scope == Scope::OnlyOld).then(|| Branches::read(&copy));
                    let _ = sender.send(shader_cache::clear_places(&asked.places, &picked, branches.as_ref()));
                });
                state.phase = Phase::Clearing(places, receiver);
            }
        }
    } else if cancelled {
        state.phase = Phase::Idle;
    }

    // A text that must not be overlooked: the footer shows it for four seconds only.
    if let Some(text) = state.result.clone() {
        let (ok, close) = widgets::dialog(ui.ctx(), scheme, "shader-ergebnis", |ui| {
            widgets::heading(ui, scheme, &t("s_sc_titel"));
            ui.label(&text);
            ui.add_space(8.0);
            ui.add(widgets::Button::new(&t("e_ok"), ButtonKind::Strong, scheme)).clicked()
        });
        if ok || close {
            state.result = None;
        }
    }
}

/// The least width of a cell of the two ways to start; narrower than two of them, they stand under each other.
const START_CELL: f32 = 220.0;
/// The same for "Abbrechen" and "Bestätigen".
const ASK_CELL: f32 = 140.0;

#[cfg(test)]
mod tests;
