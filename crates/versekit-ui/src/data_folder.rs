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

//! The data folder on the page "Installation & Pfade": the folder in use,
//! opening it, choosing another one and moving the data along
//! (`_folders` and `_move_storage` of the Python version).
//!
//! The decisions are in `versekit_core::storage_move`; this module asks the
//! questions, runs the move next to the window and says how it ended.
//!
//! A folder that was given from outside (`--daten`, an environment variable,
//! another part of the program) is shown with who decided, and can be neither chosen
//! nor moved: a move must never land in a copy of the data or in real data.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{RichText, Ui};
use versekit_core::browser;
use versekit_core::given::{Fixed, GivenBy};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::redact;
use versekit_core::storage_move::{Choice, Outcome, Plan};
use versekit_core::theme::{IconTint, Scheme};
use versekit_core::writeguard;

use crate::file_dialog::{self, Pending};
use crate::layout::GAP;
use crate::settings::{self, UpdateChoice, UpdateTexts};
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// A question that waits for the answer of the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Question {
    /// `own` files lie in the folder in use: take them along?
    Take { target: String, own: usize },
    /// A data folder lies at the target already (`files` files): use it?
    Occupied { target: String, files: usize },
}

/// How the last switch ended, as the page says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// It worked (a check mark), or not (a warning).
    pub good: bool,
    pub text: String,
}

/// Where a restart stands after a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    None,
    /// The bar at the top of the page asks for it.
    Wanted,
    /// The user said later; the bar is hidden until the next switch.
    Later,
}

type Slot = Arc<Mutex<Option<Outcome>>>;

/// What the page keeps between frames.
pub struct DataFolderState {
    dialog: Option<Pending>,
    question: Option<Question>,
    work: Option<Slot>,
    report: Option<Report>,
    restart: Restart,
    /// When the folder was last opened, and whether it worked.
    opened: Option<(f64, bool)>,
}

impl Default for DataFolderState {
    fn default() -> Self {
        Self::new()
    }
}

/// The folder of the sample page, shown while the preview has no data of its own.
const SAMPLE_FOLDER: &str = "C:\\Users\\Name\\Documents\\Verse-Kit";

impl DataFolderState {
    pub fn new() -> Self {
        DataFolderState { dialog: None, question: None, work: None, report: None, restart: Restart::None, opened: None }
    }

    /// The question that waits for an answer.
    pub fn question(&self) -> Option<&Question> {
        self.question.as_ref()
    }

    /// How the last switch ended.
    pub fn report(&self) -> Option<&Report> {
        self.report.as_ref()
    }

    pub fn restart(&self) -> Restart {
        self.restart
    }

    /// A finished switch without a move, for the edge check of the page.
    #[cfg(test)]
    pub(crate) fn pretend_switched(&mut self, outcome: &Outcome) {
        self.report = Some(report_of(outcome));
        self.restart = Restart::Wanted;
    }

    /// Whether a move is running right now.
    pub fn working(&self) -> bool {
        self.work.is_some()
    }

    /// A folder was chosen: look at it and ask or switch, as 4.12 does.
    pub fn chosen(&mut self, ctx: &egui::Context, paths: &Paths, target: &str) {
        self.report = None;
        match paths.plan_storage_move(target) {
            Plan::Given => {}
            Plan::Same => self.report = Some(Report { good: true, text: t("s_pf_schon") }),
            Plan::Nested => self.report = Some(Report { good: false, text: t("s_pf_verschachtelt") }),
            Plan::NotWritable { reason } => {
                let reason = redact::redact(&reason);
                self.report =
                    Some(Report { good: false, text: tf("s_pf_nicht_schreibbar", &[("0", Arg::from(reason))]) });
            }
            Plan::NothingToTake => self.start(ctx, paths, target, Choice::SwitchOnly),
            Plan::Occupied { files } => {
                self.question = Some(Question::Occupied { target: target.to_string(), files });
            }
            Plan::OfferMove { own } => self.question = Some(Question::Take { target: target.to_string(), own }),
        }
    }

    /// Runs the switch on a thread of its own: the window keeps drawing.
    fn start(&mut self, ctx: &egui::Context, paths: &Paths, target: &str, choice: Choice) {
        let slot: Slot = Arc::new(Mutex::new(None));
        let (copy, guard, target, ctx, out) =
            (paths.working_copy(), writeguard::state(), target.to_string(), ctx.clone(), Arc::clone(&slot));
        let started = std::thread::Builder::new().name("data-folder".into()).spawn_with_language(move || {
            writeguard::adopt(&guard);
            // A crash in the move must not leave the page waiting for ever.
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| copy.apply_storage_move(&target, choice)))
                    .unwrap_or(Outcome::NotSaved);
            *out.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
            ctx.request_repaint();
        });
        if started.is_ok() {
            self.work = Some(slot);
        } else {
            self.report = Some(Report { good: false, text: t("s_pf_faden_fehlt") });
        }
    }

    /// Takes in what a dialog or a move has answered since the last frame.
    fn poll(&mut self, ctx: &egui::Context, paths: Option<&Paths>) {
        if let Some(pending) = &self.dialog
            && let Some(answer) = pending.take()
        {
            self.dialog = None;
            if let (Some(folder), Some(paths)) = (answer, paths) {
                self.chosen(ctx, paths, &folder);
            }
        }
        let finished = self.work.as_ref().and_then(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).take());
        if let Some(outcome) = finished {
            self.work = None;
            let switched = matches!(outcome, Outcome::Switched | Outcome::UsedExisting | Outcome::Moved { .. });
            self.report = Some(report_of(&outcome));
            if switched {
                self.restart = Restart::Wanted;
            }
        }
    }

    /// The restart did not start: the bar asks again.
    pub fn restart_again(&mut self) {
        self.restart = Restart::Wanted;
    }

    /// The restart bar at the top of the page while a switch waits for a restart.
    /// True when the user asked to restart now.
    pub fn restart_bar(&mut self, ui: &mut Ui, scheme: &Scheme) -> bool {
        if self.restart != Restart::Wanted {
            return false;
        }
        let texts = UpdateTexts {
            message: &t("s_pf_neustart"),
            update: &t("s_da_neustart_knopf"),
            news: &t("hf_update_spaeter"),
            update_blocked: None,
            urgent: false,
        };
        let choice = settings::restart_bar(ui, scheme, &texts);
        ui.add_space(GAP);
        match choice {
            Some(UpdateChoice::Update) => {
                self.restart = Restart::None;
                true
            }
            Some(UpdateChoice::News) => {
                self.restart = Restart::Later;
                false
            }
            None => false,
        }
    }

    /// The row of the page: the folder in use, the buttons, who gave it, and
    /// how the last switch went. `paths` is `None` in the preview without data.
    pub fn show(&mut self, ui: &mut Ui, scheme: &Scheme, paths: Option<&Paths>) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx, paths);
        let folder = paths.map_or_else(|| SAMPLE_FOLDER.to_string(), Paths::app_folder);
        let given = paths.and_then(|p| p.given().data);
        let reason = blocked_reason(given.as_ref().map(|f| &f.by), paths.is_some(), self.working()).map(t);
        let clicks = folder_row(ui, scheme, "datenordner-pfad", &folder, reason.as_deref());
        if clicks.browse && self.dialog.is_none() {
            self.dialog = Some(file_dialog::choose_folder(&ctx, &t("s_eigene"), Some(&folder)));
        }
        // Without data of its own there is no real folder to open.
        if clicks.open && paths.is_some() {
            self.opened = Some((ui.input(|i| i.time), browser::show_folder(&folder)));
        }
        if let Some(fixed) = &given {
            given_line(ui, scheme, fixed);
        }
        if self.working() {
            widgets::loading(ui, scheme, &t("s_pf_zieht_um"));
        }
        if let Some(report) = &self.report {
            ui.add_space(4.0);
            report_box(ui, scheme, report);
        }
    }

    /// The questions and the notice of opening, drawn on the whole window.
    pub fn dialogs(&mut self, ctx: &egui::Context, scheme: &Scheme, paths: Option<&Paths>) {
        self.poll(ctx, paths);
        if let Some((since, worked)) = self.opened {
            let (icon, key) =
                if worked { ("check", "s_pf_ordner_offen") } else { ("triangle-alert", "s_pf_ordner_zu") };
            if !widgets::notice(ctx, scheme, since, icon, &t(key), None).shown {
                self.opened = None;
            }
        }
        let Some(question) = self.question.clone() else { return };
        let (text, take, target) = match &question {
            Question::Take { own, target } => (tf("s_pf_mitnehmen", &[("0", Arg::from(*own))]), true, target),
            Question::Occupied { files, target } => (tf("s_pf_belegt", &[("0", Arg::from(*files))]), false, target),
        };
        let (cancel_text, target_line) = (t("e_abbrechen"), tf("s_pf_ziel", &[("0", Arg::from(target.as_str()))]));
        let ((cancel, without, yes), close) = widgets::dialog(ctx, scheme, "datenordner-frage", |ui| {
            widgets::heading(ui, scheme, &t("s_pf_titel"));
            ui.add(egui::Label::new(RichText::new(target_line).small().color(color(scheme.sub))).wrap());
            ui.add_space(4.0);
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let cancel = ui.add(widgets::Button::new(&cancel_text, ButtonKind::Normal, scheme)).clicked();
                let without =
                    take && ui.add(widgets::Button::new(&t("s_pf_ohne"), ButtonKind::Normal, scheme)).clicked();
                let label = t(if take { "s_ab_mitnehmen_ja" } else { "s_ab_belegt_ja" });
                let yes = ui.add(widgets::Button::new(&label, ButtonKind::Strong, scheme)).clicked();
                (cancel, without, yes)
            })
            .inner
        });
        // Closing the question or Esc is "Abbrechen": nothing is switched.
        if cancel || close {
            self.question = None;
        } else if let (true, Some(paths)) = (without || yes, paths) {
            self.question = None;
            let (target, choice) = match question {
                Question::Take { target, .. } if yes => (target, Choice::TakeAlong),
                Question::Take { target, .. } => (target, Choice::SwitchOnly),
                Question::Occupied { target, .. } => (target, Choice::UseExisting),
            };
            self.start(ctx, paths, &target, choice);
        }
    }
}

/// The text key of why "Durchsuchen …" is blocked, `None` while it can be used: a folder given at start
/// (`--daten` its own words), the preview without data, a move that runs.
fn blocked_reason(given: Option<&GivenBy>, has_data: bool, working: bool) -> Option<&'static str> {
    match given {
        Some(GivenBy::CommandLine) => Some("s_pf_gesperrt_daten"),
        Some(_) => Some("s_pf_gesperrt"),
        None if !has_data => Some("s_pf_ohne_daten"),
        None if working => Some("s_pf_wartet"),
        None => None,
    }
}

/// What was clicked in a [`folder_row`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clicks {
    pub browse: bool,
    pub open: bool,
}

/// A folder as a path to read and copy, with the buttons "Durchsuchen …" and
/// "Öffnen" below. While `blocked` gives a reason, "Durchsuchen …" does
/// nothing and says why on hover.
pub fn folder_row(ui: &mut Ui, scheme: &Scheme, id_salt: &str, folder: &str, blocked: Option<&str>) -> Clicks {
    let mut view: &str = folder;
    let width = ui.available_width().min(widgets::FIELD_MAX_WIDTH);
    ui.add(egui::TextEdit::singleline(&mut view).id_salt(id_salt).desired_width(width)).on_hover_text(folder);
    let mut clicks = Clicks::default();
    ui.horizontal_wrapped(|ui| {
        let (browse_text, open_text) = (t("s_durchsuchen"), t("s_oeffnen"));
        let browse = widgets::Button::new(&browse_text, ButtonKind::Normal, scheme).blocked(blocked);
        clicks.browse = ui.add(browse).clicked();
        let open = widgets::Button::new(&open_text, ButtonKind::Normal, scheme).icon("folder");
        clicks.open = ui.add(open).clicked();
    });
    clicks
}

/// How the last switch went: a box with a bar in the color of the state
/// (the accent when it worked, the warning color when not), a symbol and the
/// sentence, wrapped.
fn report_box(ui: &mut Ui, scheme: &Scheme, report: &Report) {
    let (tone, icon) = if report.good { (scheme.accent, "circle-check") } else { (scheme.yellow, "triangle-alert") };
    let inner = egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(crate::control_radius(scheme))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if let Some(image) = icons::image(icon, tone, 18.0) {
                    ui.add(image);
                }
                ui.add(egui::Label::new(&report.text).wrap());
            });
        });
    let rect = inner.response.rect;
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(widgets::LIST_BAR_WIDTH, rect.height()));
    ui.painter().rect_filled(bar, 0.0, color(tone));
}

/// The line under the buttons: who gave the folder (also used for the game folder).
pub fn given_line(ui: &mut Ui, scheme: &Scheme, fixed: &Fixed) {
    let text = match &fixed.by {
        GivenBy::CommandLine => t("s_pf_vorgabe_start"),
        GivenBy::Environment(name) => tf("s_pf_vorgabe_umgebung", &[("0", Arg::from(name.as_str()))]),
        GivenBy::Named(name) => tf("s_pf_vorgabe_name", &[("0", Arg::from(name.as_str()))]),
    };
    ui.horizontal_top(|ui| {
        if let Some(image) = icons::themed("lock", scheme, IconTint::Normal, 14.0) {
            ui.add(image);
        }
        ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
    });
}

/// How an outcome is said on the page.
pub fn report_of(outcome: &Outcome) -> Report {
    let bad = |key: &str, args: &[(&str, Arg)]| Report { good: false, text: tf(key, args) };
    let good = |key: &str, args: &[(&str, Arg)]| Report { good: true, text: tf(key, args) };
    match outcome {
        Outcome::Given => bad("s_pf_gesperrt", &[]),
        Outcome::Nested => bad("s_pf_verschachtelt", &[]),
        Outcome::Switched => good("s_pf_umgestellt", &[]),
        Outcome::UsedExisting => good("s_pf_uebernommen", &[]),
        Outcome::Moved { copied, kept, .. } if kept.is_empty() => good("s_pf_fertig", &[("0", Arg::from(*copied))]),
        Outcome::Moved { copied, kept, old } => good(
            "s_pf_fertig_rest",
            &[("0", Arg::from(*copied)), ("1", Arg::from(kept.len())), ("2", Arg::from(redact::redact(old)))],
        ),
        Outcome::Failed { failed, copied } => {
            bad("s_pf_misslungen", &[("0", Arg::from(*failed)), ("1", Arg::from(*copied))])
        }
        Outcome::NotSaved => bad("s_pf_nicht_gespeichert", &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Role;
    use std::fs;
    use std::time::{Duration, Instant};
    use versekit_core::language::{Choice as LangChoice, set_thread_language};
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    fn german() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        set_thread_language(LangChoice::De);
        guard
    }

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-datenordner-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn join(a: &str, b: &str) -> String {
        Flavor::native().join(a, b)
    }

    /// A machine whose home, configuration and data folder lie in `root`.
    fn machine(root: &str) -> Paths {
        let config = join(root, "AppData");
        let vars = [("USERPROFILE", root), ("HOME", root), ("APPDATA", config.as_str()), ("XDG_CONFIG_HOME", &config)];
        Paths::new(Host::new(Flavor::native(), &vars, root))
    }

    /// The data folder of the machine with a few files in it.
    fn filled(paths: &Paths) -> String {
        let folder = paths.app_folder();
        for rel in ["Bauplaene/bestand.json", "Einstellungen/phrasen.json", "Intern/notiz.txt"] {
            let full = join(&folder, rel);
            fs::create_dir_all(Flavor::native().dirname(&full)).unwrap();
            fs::write(&full, rel).unwrap();
        }
        folder
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        ctx
    }

    type Nodes = Vec<(Role, String, egui::Pos2)>;

    /// One frame of the page row and its questions.
    fn frame(
        ctx: &egui::Context,
        state: &mut DataFolderState,
        paths: Option<&Paths>,
        events: Vec<egui::Event>,
    ) -> Nodes {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            state.show(ui, scheme, paths);
            state.dialogs(ui.ctx(), scheme, paths);
        });
        output.textures_delta.clear();
        let Some(update) = output.platform_output.accesskit_update else { return Vec::new() };
        update
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                let text = n.label().or(n.value())?.to_owned();
                let b = n.bounds()?;
                Some((n.role(), text, egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)))
            })
            .collect()
    }

    fn click(at: egui::Pos2) -> Vec<egui::Event> {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    fn escape() -> Vec<egui::Event> {
        let key = |pressed| egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        };
        vec![key(true), key(false)]
    }

    fn button(nodes: &Nodes, text: &str) -> egui::Pos2 {
        nodes
            .iter()
            .find(|(role, label, _)| *role == Role::Button && label == text)
            .map(|(_, _, at)| *at)
            .unwrap_or_else(|| {
                panic!("Knopf {text:?} fehlt, da sind: {:?}", nodes.iter().map(|n| &n.1).collect::<Vec<_>>())
            })
    }

    fn has(nodes: &Nodes, text: &str) -> bool {
        nodes.iter().any(|(_, label, _)| label.contains(text))
    }

    /// The nodes once the question is up (egui sizes a dialog in a frame of its own first).
    fn question_nodes(ctx: &egui::Context, state: &mut DataFolderState, paths: &Paths) -> Nodes {
        frame(ctx, state, Some(paths), Vec::new());
        frame(ctx, state, Some(paths), Vec::new())
    }

    /// Frames until the move has ended (it runs on a thread of its own).
    fn settle(ctx: &egui::Context, state: &mut DataFolderState, paths: &Paths) -> Nodes {
        let started = Instant::now();
        loop {
            frame(ctx, state, Some(paths), Vec::new());
            if !state.working() {
                // The frame that took the answer in has drawn the old page: one more.
                return frame(ctx, state, Some(paths), Vec::new());
            }
            assert!(started.elapsed() < Duration::from_secs(20), "the move does not end");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn the_row_shows_the_folder_in_use_and_both_buttons() {
        let _german = german();
        let root = scratch("row");
        let paths = machine(&root);
        let folder = filled(&paths);
        let (ctx, mut state) = (context(), DataFolderState::new());
        frame(&ctx, &mut state, Some(&paths), Vec::new());
        let nodes = frame(&ctx, &mut state, Some(&paths), Vec::new());
        assert!(nodes.iter().any(|(_, text, _)| *text == folder), "the path of the folder in use");
        button(&nodes, "Durchsuchen …");
        button(&nodes, "Öffnen");
        assert!(!has(&nodes, "Vorgegeben"), "nothing given: no line about it");
    }

    #[test]
    fn a_folder_given_from_outside_says_who_and_cannot_be_moved() {
        let _german = german();
        let root = scratch("given");
        let paths = machine(&root);
        let folder = filled(&paths);
        paths.set_given_data(&folder, GivenBy::Named("Beispiel".into()));
        let (ctx, mut state) = (context(), DataFolderState::new());
        frame(&ctx, &mut state, Some(&paths), Vec::new());
        let nodes = frame(&ctx, &mut state, Some(&paths), Vec::new());
        assert!(has(&nodes, "Vorgegeben beim Start (Beispiel)."), "{nodes:?}");
        // A click on the blocked button opens no dialog.
        frame(&ctx, &mut state, Some(&paths), click(button(&nodes, "Durchsuchen …")));
        assert!(state.dialog.is_none());
        // Even a folder that gets in by another way is refused by the core.
        let target = join(&root, "Neu");
        state.chosen(&ctx, &paths, &target);
        assert!(state.question().is_none() && !state.working() && state.report().is_none());
        assert!(!std::path::Path::new(&target).exists(), "nothing created at the target");
        assert_eq!(paths.app_folder(), folder);
    }

    #[test]
    fn taking_the_data_along_moves_them_and_asks_for_a_restart() {
        let _german = german();
        let root = scratch("take");
        let paths = machine(&root);
        let old = filled(&paths);
        let target = join(&root, "Neu");
        let (ctx, mut state) = (context(), DataFolderState::new());
        state.chosen(&ctx, &paths, &target);
        assert_eq!(state.question(), Some(&Question::Take { target: target.clone(), own: 3 }));
        let nodes = question_nodes(&ctx, &mut state, &paths);
        assert!(has(&nodes, "Dateien im bisherigen Ordner: 3."), "{nodes:?}");
        frame(&ctx, &mut state, Some(&paths), click(button(&nodes, "Mitnehmen")));
        let nodes = settle(&ctx, &mut state, &paths);
        assert!(state.report().is_some_and(|r| r.good), "{:?}", state.report());
        assert!(has(&nodes, "Umgezogen und geprüft: 3."), "{nodes:?}");
        assert_eq!(state.restart(), Restart::Wanted);
        assert_eq!(paths.app_folder(), target);
        assert_eq!(fs::read_to_string(join(&target, "Bauplaene/bestand.json")).unwrap(), "Bauplaene/bestand.json");
        assert!(!std::path::Path::new(&join(&old, "Bauplaene/bestand.json")).exists(), "cleared at the old place");
    }

    #[test]
    fn switching_without_the_data_leaves_them_where_they_are() {
        let _german = german();
        let root = scratch("without");
        let paths = machine(&root);
        let old = filled(&paths);
        let target = join(&root, "Neu");
        let (ctx, mut state) = (context(), DataFolderState::new());
        state.chosen(&ctx, &paths, &target);
        let nodes = question_nodes(&ctx, &mut state, &paths);
        frame(&ctx, &mut state, Some(&paths), click(button(&nodes, "Ohne Daten wechseln")));
        settle(&ctx, &mut state, &paths);
        assert_eq!(paths.app_folder(), target);
        assert!(std::path::Path::new(&join(&old, "Bauplaene/bestand.json")).exists());
        assert!(!std::path::Path::new(&join(&target, "Bauplaene")).exists());
    }

    #[test]
    fn cancelling_and_escape_switch_nothing() {
        let _german = german();
        let root = scratch("cancel");
        let paths = machine(&root);
        let old = filled(&paths);
        let target = join(&root, "Neu");
        for by_escape in [false, true] {
            let (ctx, mut state) = (context(), DataFolderState::new());
            state.chosen(&ctx, &paths, &target);
            let nodes = question_nodes(&ctx, &mut state, &paths);
            let events = if by_escape { escape() } else { click(button(&nodes, "Abbrechen")) };
            frame(&ctx, &mut state, Some(&paths), events);
            frame(&ctx, &mut state, Some(&paths), Vec::new());
            assert!(state.question().is_none() && !state.working(), "by_escape {by_escape}");
            assert_eq!(paths.app_folder(), old, "by_escape {by_escape}: still the old folder");
            assert_eq!(state.restart(), Restart::None);
        }
    }

    #[test]
    fn a_folder_with_data_is_used_without_touching_it() {
        let _german = german();
        let root = scratch("occupied");
        let paths = machine(&root);
        filled(&paths);
        let target = join(&root, "Fremd");
        let theirs = join(&target, "Bauplaene/bestand.json");
        fs::create_dir_all(Flavor::native().dirname(&theirs)).unwrap();
        fs::write(&theirs, "fremd").unwrap();
        let (ctx, mut state) = (context(), DataFolderState::new());
        state.chosen(&ctx, &paths, &target);
        assert_eq!(state.question(), Some(&Question::Occupied { target: target.clone(), files: 1 }));
        let nodes = question_nodes(&ctx, &mut state, &paths);
        assert!(has(&nodes, "Dort liegen schon Dateien (1)."), "{nodes:?}");
        frame(&ctx, &mut state, Some(&paths), click(button(&nodes, "Die dort benutzen")));
        settle(&ctx, &mut state, &paths);
        assert_eq!(paths.app_folder(), target);
        assert_eq!(fs::read_to_string(&theirs).unwrap(), "fremd", "their file is untouched");
        assert!(!std::path::Path::new(&join(&target, "Intern")).exists(), "and nothing was copied in");
    }

    #[test]
    fn a_target_that_takes_nothing_is_said_and_nothing_changes() {
        let _german = german();
        let root = scratch("readonly");
        let paths = machine(&root);
        let old = filled(&paths);
        let file = join(&root, "datei");
        fs::write(&file, "x").unwrap();
        let (ctx, mut state) = (context(), DataFolderState::new());
        state.chosen(&ctx, &paths, &join(&file, "unter"));
        let nodes = frame(&ctx, &mut state, Some(&paths), Vec::new());
        assert!(has(&nodes, "Dorthin lässt sich nichts schreiben"), "{nodes:?}");
        assert!(state.report().is_some_and(|r| !r.good) && state.question().is_none());
        assert_eq!(paths.app_folder(), old);
    }

    #[test]
    fn the_same_folder_and_a_nested_one_are_said_not_ignored() {
        let _german = german();
        let root = scratch("same");
        let paths = machine(&root);
        let old = filled(&paths);
        let (ctx, mut state) = (context(), DataFolderState::new());
        state.chosen(&ctx, &paths, &old);
        assert_eq!(state.report().map(|r| r.text.clone()), Some("Das ist schon der Datenordner.".to_string()));
        state.chosen(&ctx, &paths, &join(&old, "Unter"));
        assert!(state.report().is_some_and(|r| !r.good && r.text.starts_with("Der Ordner liegt im bisherigen")));
        assert!(state.question().is_none() && !state.working());
        assert_eq!(paths.app_folder(), old);
    }

    #[test]
    fn without_data_of_its_own_the_preview_moves_nothing() {
        let _german = german();
        let (ctx, mut state) = (context(), DataFolderState::new());
        frame(&ctx, &mut state, None, Vec::new());
        let nodes = frame(&ctx, &mut state, None, Vec::new());
        frame(&ctx, &mut state, None, click(button(&nodes, "Durchsuchen …")));
        assert!(state.dialog.is_none(), "a blocked button opens nothing");
    }

    #[test]
    fn a_blocked_browse_says_who_fixed_the_folder() {
        let by_start = Some(&GivenBy::CommandLine);
        assert_eq!(blocked_reason(by_start, true, false), Some("s_pf_gesperrt_daten"));
        assert_eq!(blocked_reason(by_start, true, true), Some("s_pf_gesperrt_daten"), "the start before the move");
        let by_variable = GivenBy::Environment("VERSEKIT_HOME".into());
        assert_eq!(blocked_reason(Some(&by_variable), true, false), Some("s_pf_gesperrt"));
        assert_eq!(blocked_reason(Some(&GivenBy::Named("x".into())), true, false), Some("s_pf_gesperrt"));
        assert_eq!(blocked_reason(None, false, false), Some("s_pf_ohne_daten"));
        assert_eq!(blocked_reason(None, true, true), Some("s_pf_wartet"));
        assert_eq!(blocked_reason(None, true, false), None);
        let _german = german();
        for lang in [LangChoice::De, LangChoice::En] {
            set_thread_language(lang);
            let text = t("s_pf_gesperrt_daten");
            assert!(text.contains("--daten") && !text.contains("s_pf_"), "{lang:?}: {text}");
        }
        set_thread_language(LangChoice::De);
        assert_eq!(t("s_pf_gesperrt_daten"), "Der Ordner ist beim Start mit --daten festgelegt.");
    }

    /// `--daten` on an empty folder: the row shows the data folder of that copy, not the one of the samples, and
    /// "Durchsuchen …" opens nothing.
    #[test]
    fn a_start_on_an_empty_folder_shows_that_folder() {
        let _german = german();
        let root = scratch("leer");
        writeguard::deny(true);
        writeguard::allow_under(Some(&root));
        let paths = Paths::for_copy(Host::new(Flavor::native(), &[], &root), &root).expect("an empty folder is a copy");
        let (ctx, mut state) = (context(), DataFolderState::new());
        frame(&ctx, &mut state, Some(&paths), Vec::new());
        let nodes = frame(&ctx, &mut state, Some(&paths), Vec::new());
        let folder = join(&root, versekit_core::paths::COPY_DATA_FOLDER);
        assert!(nodes.iter().any(|(_, text, _)| *text == folder), "the folder of the copy: {nodes:?}");
        assert!(!has(&nodes, SAMPLE_FOLDER), "not the folder of the samples");
        assert!(has(&nodes, "Vorgegeben beim Start."), "{nodes:?}");
        frame(&ctx, &mut state, Some(&paths), click(button(&nodes, "Durchsuchen …")));
        assert!(state.dialog.is_none(), "a blocked button opens nothing");
        writeguard::deny(false);
        assert!(writeguard::take_denied().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_outcome_has_its_words_in_both_languages() {
        let _german = german();
        let outcomes = [
            Outcome::Given,
            Outcome::Nested,
            Outcome::Switched,
            Outcome::UsedExisting,
            Outcome::Moved { copied: 4, kept: Vec::new(), old: "alt".into() },
            Outcome::Moved { copied: 4, kept: vec!["a".into(), "b".into()], old: "alt".into() },
            Outcome::Failed { failed: 2, copied: 1 },
            Outcome::NotSaved,
        ];
        for lang in [LangChoice::De, LangChoice::En] {
            set_thread_language(lang);
            for outcome in &outcomes {
                let text = report_of(outcome).text;
                assert!(!text.is_empty() && !text.contains('{') && !text.contains("s_pf_"), "{outcome:?}: {text}");
            }
        }
        set_thread_language(LangChoice::De);
        assert!(report_of(&outcomes[5]).text.starts_with("Umgezogen und geprüft: 4. Im alten Ordner blieben 2:"));
        assert!(!report_of(&outcomes[6]).good && report_of(&outcomes[4]).good);
    }
}
