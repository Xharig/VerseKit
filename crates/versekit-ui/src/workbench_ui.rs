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

//! The jobs of the fabricator on the page Herstellung (`versekit_core::workbench_jobs`): a part that folds open, with
//! a line to enter what is made and its time, and the list of the jobs in the order of the queue with the time left,
//! each with a button that takes it out. VerseKit only keeps the book; the overlay tells when one is done.

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::workbench_jobs::{self, Job, Refused, State};

use crate::color;
use crate::widgets::{self, ButtonKind};

/// How often the file is read again while the part is open (the overlay marks jobs as told in it).
const READ_EVERY: f64 = 15.0;
/// The minutes a new job has before the player changes them.
const MINUTES: i64 = 30;

/// What the part holds between frames.
pub struct WorkbenchState {
    /// What the player types for the next job, and its minutes.
    pub name: String,
    pub minutes: i64,
    jobs: Vec<Job>,
    read_at: f64,
    /// Why the last job could not be entered.
    said: Option<String>,
    /// The time now in seconds since 1970 (the checks set their own clock).
    pub clock: fn() -> f64,
}

impl Default for WorkbenchState {
    fn default() -> Self {
        WorkbenchState {
            name: String::new(),
            minutes: MINUTES,
            jobs: Vec::new(),
            read_at: f64::NEG_INFINITY,
            said: None,
            clock: || {
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
            },
        }
    }
}

impl WorkbenchState {
    /// The jobs as the part shows them.
    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    /// Enters the job typed: the name and the minutes; what refuses it is said under the line.
    pub fn enter(&mut self, paths: &Paths) {
        let now = (self.clock)();
        match workbench_jobs::add(paths, &self.name, self.minutes * 60, now) {
            Ok(jobs) => {
                self.jobs = jobs;
                self.name.clear();
                self.said = None;
            }
            Err(refused) => {
                let limit = Arg::from((workbench_jobs::RUNNING + workbench_jobs::WAITING) as i64);
                let days = Arg::from(workbench_jobs::LONGEST / 86_400);
                self.said = Some(match refused {
                    Refused::Full => tf("s_wb_voll", &[("0", limit)]),
                    Refused::Time => tf("s_wb_zeit_falsch", &[("0", days)]),
                    Refused::NotSaved => t("s_wb_nicht_gespeichert"),
                });
            }
        }
    }
}

/// Draws the part under the line of the fabricator; nothing without data of its own.
pub fn show(ui: &mut Ui, scheme: &Scheme, state: &mut WorkbenchState, paths: Option<&Paths>) {
    let Some(paths) = paths else { return };
    let now = (state.clock)();
    if now - state.read_at >= READ_EVERY || now < state.read_at {
        state.jobs = workbench_jobs::load(paths);
        state.read_at = now;
    }
    let planned = workbench_jobs::plan(&state.jobs);
    let open = planned.iter().filter(|p| p.end > now).count();
    let title = if open > 0 { tf("s_wb_titel_offen", &[("0", Arg::from(open as i64))]) } else { t("s_wb_titel") };
    // Folded until the player opens it: the lists of the page below keep their room.
    widgets::collapsing(&title).id_salt("werkbank-auftraege").default_open(false).show(ui, |ui| {
        let running = Arg::from(workbench_jobs::RUNNING as i64);
        let hint = tf("s_wb_h", &[("0", running)]);
        ui.add(egui::Label::new(RichText::new(hint).small().color(color(scheme.sub))).wrap());
        ui.add_space(4.0);
        if entry_line(ui, scheme, state) {
            state.enter(paths);
        }
        if let Some(said) = &state.said {
            ui.add(egui::Label::new(RichText::new(said).color(color(scheme.red_text()))).wrap());
        }
        ui.add_space(4.0);
        let planned = workbench_jobs::plan(&state.jobs);
        if planned.is_empty() {
            ui.add(egui::Label::new(RichText::new(t("s_wb_leer")).color(color(scheme.sub))).wrap());
            return;
        }
        let mut take_out = None;
        // At most a few lines high: the history of ten scrolls.
        egui::ScrollArea::vertical().id_salt("werkbank-liste").max_height(LIST_HEIGHT).min_scrolled_height(LIST_HEIGHT).auto_shrink([false, true]).show(
            ui,
            |ui| {
                for job in &planned {
                    if job_row(ui, scheme, job, now) {
                        take_out = Some(job.job.id);
                    }
                }
            },
        );
        if let Some(id) = take_out {
            state.jobs = workbench_jobs::remove(paths, id, now);
        }
        if planned.iter().any(|p| p.end <= now)
            && ui.add(widgets::Button::new(&t("s_wb_fertige_weg"), ButtonKind::Normal, scheme)).clicked()
        {
            state.jobs = workbench_jobs::clear_done(paths, now);
        }
    });
    // The time left and the number of open jobs in the title count every second, folded too.
    if open > 0 {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    ui.add_space(6.0);
}

/// The most height the list of the jobs takes, in points.
const LIST_HEIGHT: f32 = 160.0;

/// The line to enter a job: what is made, its minutes and the button. Gives whether it is to be entered.
fn entry_line(ui: &mut Ui, scheme: &Scheme, state: &mut WorkbenchState) -> bool {
    let mut enter = false;
    ui.horizontal_wrapped(|row| {
        let field = widgets::text_field(&mut state.name, &t("s_wb_name"), scheme).desired_width(220.0);
        let response = row.add(field);
        enter |= response.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Enter));
        let label = row.label(t("s_wb_zeit"));
        let most = workbench_jobs::LONGEST / 60;
        let unit = t("s_wb_minuten");
        widgets::number_field_labelled(
            row,
            scheme,
            "werkbank-minuten",
            &mut state.minutes,
            1..=most,
            &unit,
            Some(label.id),
        );
        let word = t("s_wb_eintragen");
        let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("plus");
        enter |= row.add(button).clicked();
    });
    enter
}

/// One job: the button that takes it out, named with the job for screen readers, and its line. Gives whether the
/// button was clicked.
fn job_row(ui: &mut Ui, scheme: &Scheme, job: &workbench_jobs::Planned, now: f64) -> bool {
    let name = workbench_jobs::label(&job.job);
    let spoken = tf("s_wb_entfernen_name", &[("0", Arg::from(name.as_str()))]);
    let done = matches!(job.state(now), State::Done { .. });
    let tint = if done { scheme.accent } else { scheme.fg };
    let line = format!("{name} · {}", workbench_jobs::state_text(job, now));
    let word = t("s_wb_entfernen");
    ui.horizontal(|row| {
        let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("x").spoken(&spoken);
        let clicked = row.add(button).clicked();
        row.add(egui::Label::new(RichText::new(line).color(color(tint))).truncate());
        clicked
    })
    .inner
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, RawInput, Rect, vec2};
    use versekit_core::language::{Choice, set_thread_language};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::*;

    fn paths(name: &str) -> Paths {
        let dir = versekit_core::testdir::fresh(name);
        let home = dir.to_string_lossy().into_owned();
        Paths::new(Host::new(Flavor::Posix, &[("HOME", &home)], &home))
    }

    /// Draws the part once and gives the texts it shows with their places.
    fn draw(
        ctx: &egui::Context,
        state: &mut WorkbenchState,
        paths: &Paths,
        events: Vec<egui::Event>,
    ) -> Vec<(String, Pos2)> {
        let scheme = versekit_core::theme::current();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
            events,
            // Half a second a frame: the part that folds open ends its opening.
            time: Some(ctx.input(|i| i.time) + 0.5),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| show(ui, scheme, state, Some(paths)));
        out.textures_delta.clear();
        let mut texts = Vec::new();
        fn collect(shape: &egui::epaint::Shape, into: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::epaint::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::epaint::Shape::Vec(list) => list.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        texts
    }

    fn click(ctx: &egui::Context, state: &mut WorkbenchState, paths: &Paths, at: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw(ctx, state, paths, vec![egui::Event::PointerMoved(at)]);
        draw(ctx, state, paths, vec![button(true)]);
        draw(ctx, state, paths, vec![button(false)]);
    }

    fn place(texts: &[(String, Pos2)], wanted: &str) -> Pos2 {
        texts
            .iter()
            .find(|(text, _)| text == wanted)
            .map(|(_, at)| *at + vec2(3.0, 3.0))
            .unwrap_or_else(|| panic!("{wanted} in {texts:?}"))
    }

    /// Without own data the part is not there; with them it is closed while no job is open.
    #[test]
    fn without_own_data_there_is_no_part_and_without_jobs_it_is_closed() {
        set_thread_language(Choice::De);
        let ctx = egui::Context::default();
        let scheme = versekit_core::theme::current();
        let mut state = WorkbenchState::default();
        let mut out = ctx.run_ui(RawInput::default(), |ui| show(ui, scheme, &mut state, None));
        out.textures_delta.clear();
        assert!(out.shapes.is_empty(), "nothing without data of its own");
        let paths = paths("werkbank-ui-leer");
        let texts = draw(&ctx, &mut state, &paths, vec![]);
        assert!(texts.iter().any(|(text, _)| *text == t("s_wb_titel")), "{texts:?}");
        assert!(!texts.iter().any(|(text, _)| *text == t("s_wb_eintragen")), "closed while nothing is open");
    }

    /// Typed and entered: the job stands in the list with its time left and in the file; the button takes it out.
    #[test]
    fn a_job_is_entered_shown_with_its_time_left_and_taken_out() {
        set_thread_language(Choice::De);
        let ctx = egui::Context::default();
        let paths = paths("werkbank-ui-eintragen");
        let mut state = WorkbenchState { clock: || 1_000_000.0, ..WorkbenchState::default() };
        // Opened by hand, as the player does while it is still empty.
        let texts = draw(&ctx, &mut state, &paths, vec![]);
        click(&ctx, &mut state, &paths, place(&texts, &t("s_wb_titel")));
        state.name = "Helm".into();
        state.minutes = 45;
        let texts = draw(&ctx, &mut state, &paths, vec![]);
        click(&ctx, &mut state, &paths, place(&texts, &t("s_wb_eintragen")));
        assert_eq!(workbench_jobs::load(&paths).len(), 1, "kept in the file");
        assert!(state.name.is_empty(), "the field is empty for the next one");
        let texts = draw(&ctx, &mut state, &paths, vec![]);
        assert!(texts.iter().any(|(text, _)| text.starts_with("Helm · noch 45m 0s")), "{texts:?}");
        let title = tf("s_wb_titel_offen", &[("0", Arg::from(1i64))]);
        assert!(texts.iter().any(|(text, _)| *text == title), "{texts:?}");
        click(&ctx, &mut state, &paths, place(&texts, &t("s_wb_entfernen")));
        assert!(workbench_jobs::load(&paths).is_empty(), "taken out");
        assert!(state.jobs().is_empty());
    }

    /// Time passes: the job is done, says so, and the button takes the done ones out.
    #[test]
    fn a_done_job_says_so_and_goes_by_the_button() {
        set_thread_language(Choice::De);
        let ctx = egui::Context::default();
        let paths = paths("werkbank-ui-fertig");
        let mut state = WorkbenchState { clock: || 3_000_000.0, ..WorkbenchState::default() };
        state.name = "Gewehr".into();
        state.minutes = 1;
        state.enter(&paths);
        state.clock = || 3_000_120.0;
        let texts = draw(&ctx, &mut state, &paths, vec![]);
        click(&ctx, &mut state, &paths, place(&texts, &t("s_wb_titel")));
        // The part opens over a few frames, and the list is measured in the first of them.
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = draw(&ctx, &mut state, &paths, vec![]);
        }
        let done = format!("Gewehr · {}", t("s_uv_raff_fertig"));
        assert!(texts.iter().any(|(text, _)| *text == done), "{texts:?}");
        click(&ctx, &mut state, &paths, place(&texts, &t("s_wb_fertige_weg")));
        assert!(workbench_jobs::load(&paths).is_empty(), "the done one went");
    }

    /// A full fabricator refuses the next job and says why.
    #[test]
    fn a_full_fabricator_says_so() {
        set_thread_language(Choice::De);
        let paths = paths("werkbank-ui-voll");
        let mut state = WorkbenchState { clock: || 2_000_000.0, ..WorkbenchState::default() };
        for n in 0..(workbench_jobs::RUNNING + workbench_jobs::WAITING) {
            state.name = format!("Teil {n}");
            state.enter(&paths);
        }
        state.name = "Noch eins".into();
        state.enter(&paths);
        let limit = Arg::from((workbench_jobs::RUNNING + workbench_jobs::WAITING) as i64);
        assert_eq!(state.said, Some(tf("s_wb_voll", &[("0", limit)])));
        assert_eq!(state.name, "Noch eins", "the name stays for later");
    }
}
