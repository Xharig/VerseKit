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

//! The window of a backup at the first start that did not work (`versekit_core::first_backup`): why, and three
//! ways on. "Erneut versuchen" makes it again, "Ordner wählen" makes it in a folder the player picks, "Beenden"
//! ends the program. The program only goes on once the backup is made; closing the window ends it.
//!
//! The backup runs in a thread of its own under the lock of writing the run has (`writeguard`), so the window
//! keeps drawing.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::RichText;
use versekit_core::first_backup::{self, Failure, Outcome};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::{theme, writeguard};

use crate::color;
use crate::file_dialog::{self, Pending};
use crate::widgets::{self, ButtonKind};

/// How the window ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The backup is made: the program goes on.
    Go(Outcome),
    /// The player ended the program.
    Quit,
}

/// The state of the window.
pub struct FailedBackup {
    paths: Paths,
    version: String,
    /// The lock of writing the backup works under.
    guard: writeguard::State,
    failure: Failure,
    /// The folder the player picked; `None`: the one of the data folder.
    folder: Option<String>,
    picking: Option<Pending>,
    working: Option<Receiver<Result<Outcome, Failure>>>,
    answer: Option<Answer>,
}

impl FailedBackup {
    /// The window after `failure`, for the data of `paths`; the backup works under `guard`.
    pub fn new(paths: &Paths, version: &str, guard: writeguard::State, failure: Failure) -> FailedBackup {
        FailedBackup {
            paths: paths.working_copy(),
            version: version.to_string(),
            guard,
            failure,
            folder: None,
            picking: None,
            working: None,
            answer: None,
        }
    }

    /// How the window ended; `None` while it is open.
    pub fn answer(&self) -> Option<&Answer> {
        self.answer.as_ref()
    }

    /// Does a backup run right now?
    pub fn working(&self) -> bool {
        self.working.is_some()
    }

    /// Makes the backup again, in a thread of its own.
    fn start(&mut self, ctx: &egui::Context) {
        let (paths, version, folder, guard) =
            (self.paths.working_copy(), self.version.clone(), self.folder.clone(), self.guard.clone());
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name("erststart-sicherung".into()).spawn(move || {
            writeguard::adopt(&guard);
            let _ = tx.send(first_backup::ensure(&paths, &version, folder.as_deref()));
            ctx.request_repaint();
        });
        match started {
            Ok(_) => self.working = Some(rx),
            Err(error) => {
                self.failure = Failure::Failed { folder: self.failure.folder().to_string(), reason: error.to_string() }
            }
        }
    }

    /// What came of the backup, once it is done.
    fn follow(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.working else { return };
        let done = match rx.try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(TryRecvError::Disconnected) => {
                Err(Failure::Failed { folder: self.failure.folder().to_string(), reason: "?".into() })
            }
        };
        self.working = None;
        match done {
            Ok(outcome) => self.answer = Some(Answer::Go(outcome)),
            Err(failure) => self.failure = failure,
        }
    }

    /// Draws the window and moves it on; asks the window to close once it is answered.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let scheme = theme::current();
        if let Some(answer) = self.picking.as_ref().and_then(Pending::take) {
            self.picking = None;
            if let Some(folder) = answer {
                self.folder = Some(folder);
                self.start(&ctx);
            }
        }
        self.follow(&ctx);
        // The window of the system closes it: that ends the program, as "Beenden" does.
        if ui.input(|i| i.viewport().close_requested()) && self.answer.is_none() {
            if self.working() {
                // A backup half written is no backup: the window waits for it.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else {
                self.answer = Some(Answer::Quit);
            }
        }
        let (mut again, mut choose, mut quit) = (false, false, false);
        egui::Frame::NONE.fill(color(scheme.bg)).inner_margin(20.0).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            widgets::heading(ui, scheme, &t("erst_sich_titel"));
            ui.add_space(8.0);
            let text = tf("erst_sich_text", &[("0", Arg::from(self.failure.text()))]);
            ui.add(egui::Label::new(text).wrap());
            if let Some(folder) = &self.folder {
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(folder).small().color(color(scheme.sub))).wrap());
            }
            ui.add_space(14.0);
            if self.working() {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(color(scheme.accent)));
                    ui.add(egui::Label::new(t("erst_sich_arbeitet")).wrap());
                });
                return;
            }
            let free = self.picking.is_none();
            let labels = (t("erst_sich_erneut"), t("erst_sich_waehlen"), t("erst_sich_beenden"));
            ui.horizontal_wrapped(|row| {
                let button = widgets::Button::new(&labels.0, ButtonKind::Strong, scheme).icon("rotate-cw");
                again = row.add_enabled(free, button).clicked();
                let button = widgets::Button::new(&labels.1, ButtonKind::Normal, scheme).icon("folder");
                choose = row.add_enabled(free, button).clicked();
                let button = widgets::Button::new(&labels.2, ButtonKind::Normal, scheme).icon("door-open");
                quit = row.add(button).clicked();
            });
        });
        if quit {
            self.answer = Some(Answer::Quit);
        } else if again {
            self.start(&ctx);
        } else if choose {
            let start = self.folder.clone().unwrap_or_else(|| self.failure.folder().to_string());
            self.picking = Some(file_dialog::choose_folder(&ctx, &t("erst_sich_waehlen"), Some(&start)));
        }
        if self.answer.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use egui::{Event, Pos2, Shape};
    use versekit_core::language;
    use versekit_core::paths::Host;
    use versekit_core::pypath::Flavor;

    use super::*;

    struct Bench {
        ctx: egui::Context,
        window: FailedBackup,
        closed: bool,
    }

    impl Bench {
        fn new(window: FailedBackup) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Bench { ctx, window, closed: false }
        }

        fn frame(&mut self, events: Vec<Event>) -> Vec<(String, Pos2)> {
            fn collect(shape: &Shape, into: &mut Vec<(String, Pos2)>) {
                match shape {
                    Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                    _ => {}
                }
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(560.0, 420.0))),
                events,
                ..Default::default()
            };
            let window = &mut self.window;
            let mut output = self.ctx.run_ui(input, |ui| window.show(ui));
            output.textures_delta.clear();
            if output
                .viewport_output
                .values()
                .any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)))
            {
                self.closed = true;
            }
            let mut texts = Vec::new();
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            texts
        }

        fn settle(&mut self) -> Vec<(String, Pos2)> {
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts = self.frame(Vec::new());
            }
            texts
        }

        fn click(&mut self, label: &str) {
            let texts = self.settle();
            let at = texts.iter().find(|(text, _)| text == label).map(|(_, at)| *at + egui::vec2(3.0, 3.0));
            let at = at.unwrap_or_else(|| panic!("{label} is not shown: {texts:?}"));
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new());
        }

        /// Frames until the backup is done, at most a minute.
        fn until_done(&mut self) -> Vec<(String, Pos2)> {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            let mut texts = self.settle();
            while self.window.working() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
                texts = self.settle();
            }
            texts
        }
    }

    fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    /// A data folder of the older version in a throwaway home, and the failure of a folder that cannot be made
    /// (a file stands where it should be).
    fn failed(name: &str) -> (FailedBackup, versekit_core::testdir::Dir, String) {
        let dir = versekit_core::testdir::Dir::new(&format!("erststart-fenster-{name}"));
        let text = dir.path().to_string_lossy().into_owned();
        let nowhere = dir.path().join("kein-spiel").to_string_lossy().into_owned();
        let vars = [
            ("HOME", text.as_str()),
            ("USERPROFILE", text.as_str()),
            ("APPDATA", text.as_str()),
            (versekit_core::paths::ENV_INSTALL_DIR, nowhere.as_str()),
            (versekit_core::game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(Host::new(Flavor::native(), &vars, &text));
        let root = paths.app_folder();
        std::fs::create_dir_all(Path::new(&root).join("Bauplaene")).unwrap();
        std::fs::write(Path::new(&root).join("Bauplaene").join("bestand.json"), "[1]").unwrap();
        let blocked = dir.path().join("besetzt");
        std::fs::write(&blocked, "x").unwrap();
        let blocked = blocked.join("unter").to_string_lossy().into_owned();
        let failure = first_backup::ensure(&paths, "5.0.0", Some(&blocked)).expect_err("cannot be made there");
        theme::set_current("original");
        let window = FailedBackup::new(&paths, "5.0.0", writeguard::state(), failure);
        (window, dir, root)
    }

    #[test]
    fn the_window_says_why_and_offers_three_ways_in_both_languages() {
        let _language = crate::language_lock();
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_language(lang);
            let (window, _dir, _root) = failed("texte");
            let mut bench = Bench::new(window);
            let texts = bench.settle();
            for key in ["erst_sich_titel", "erst_sich_erneut", "erst_sich_waehlen", "erst_sich_beenden"] {
                assert!(shows(&texts, &language::t(key)), "{key}: {texts:?}");
            }
            assert!(shows(&texts, "besetzt"), "the folder is named: {texts:?}");
            assert!(bench.window.answer().is_none() && !bench.closed, "nothing goes on by itself");
        }
        language::set_language(language::Choice::De);
    }

    #[test]
    fn quitting_ends_without_a_backup_and_without_a_mark() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (window, _dir, root) = failed("beenden");
        let paths = window.paths.working_copy();
        let mut bench = Bench::new(window);
        bench.click(&language::t("erst_sich_beenden"));
        assert_eq!(bench.window.answer(), Some(&Answer::Quit));
        assert!(bench.closed);
        assert!(first_backup::needed(&paths), "no mark");
        assert!(!Path::new(&root).join(first_backup::FOLDER).exists(), "no backup");
    }

    #[test]
    fn closing_the_window_ends_the_program_too() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (window, _dir, _root) = failed("schliessen");
        let mut bench = Bench::new(window);
        bench.settle();
        let mut input = egui::RawInput::default();
        input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        let window = &mut bench.window;
        let mut output = bench.ctx.run_ui(input, |ui| window.show(ui));
        output.textures_delta.clear();
        assert_eq!(bench.window.answer(), Some(&Answer::Quit));
    }

    #[test]
    fn trying_again_makes_the_backup_once_the_way_is_free_and_then_goes_on() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (mut window, _dir, root) = failed("erneut");
        // Still blocked: the window stays and says it again.
        window.folder = Some(window.failure.folder().to_string());
        let mut bench = Bench::new(window);
        bench.click(&language::t("erst_sich_erneut"));
        bench.until_done();
        assert!(bench.window.answer().is_none() && !bench.closed, "a second failure keeps the window");
        // The default folder of the data folder works.
        bench.window.folder = None;
        bench.click(&language::t("erst_sich_erneut"));
        bench.until_done();
        let Some(Answer::Go(Outcome::Written { file, .. })) = bench.window.answer().cloned() else {
            panic!("the backup is made: {:?}", bench.window.answer());
        };
        assert!(Path::new(&file).starts_with(Path::new(&root).join(first_backup::FOLDER)), "{file}");
        assert!(bench.closed, "the program goes on");
    }

    #[test]
    fn a_folder_picked_by_the_player_takes_the_backup() {
        let _language = crate::language_lock();
        language::set_language(language::Choice::De);
        let (window, dir, _root) = failed("waehlen");
        let mut bench = Bench::new(window);
        file_dialog::stand_in::start();
        bench.click(&language::t("erst_sich_waehlen"));
        assert_eq!(file_dialog::stand_in::stop(), ["folder"], "the dialog of a folder");
        let chosen = dir.path().join("Stick").to_string_lossy().into_owned();
        bench.window.picking = Some(Pending::answered(Some(chosen.clone())));
        bench.until_done();
        let Some(Answer::Go(Outcome::Written { file, .. })) = bench.window.answer().cloned() else {
            panic!("the backup is made: {:?}", bench.window.answer());
        };
        assert!(Path::new(&file).starts_with(&chosen), "{file}");
        // A cancelled dialog does nothing.
        let (window, _dir, _root) = failed("abbrechen");
        let mut bench = Bench::new(window);
        bench.window.picking = Some(Pending::answered(None));
        bench.settle();
        assert!(bench.window.answer().is_none() && !bench.window.working());
    }
}
