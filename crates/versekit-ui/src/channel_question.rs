// Verse-Kit — companion for Star Citizen.
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

//! The question after the game folder moved (`_kanal_pruefen`, `ask_channel` of 4.12).
//!
//! ⚠⚠ What it is for: when CIG puts a patched build beside LIVE, hardly anybody downloads 100 GB
//! again; the LIVE folder is renamed to HOTFIX so the launcher fetches only the differences. The
//! folder entered is gone then, and the program would read from somewhere else without a word or
//! not find Star Citizen although a path stands in the settings. Both look like a broken program.
//!
//! ⚠ It is asked, not switched silently: only the player knows which channel is meant; whoever
//! tests the PTU with LIVE beside it would have the wrong blueprints counted otherwise. And it is
//! asked once for each start of the program: whoever says "not now" wants peace.
//!
//! The look at the folders runs in a thread of its own (the game may lie on a share); the answer
//! comes back to the drawing thread.

use std::sync::mpsc::{self, Receiver};

use egui::{Sense, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::{Channel, Paths};
use versekit_core::theme::Scheme;

use crate::color;
use crate::widgets::{self, ButtonKind};

/// How long after the start the look is taken (4.12: 1.2 seconds, before the check for a new
/// version: nobody wants two windows in a row).
pub const LOOK_AFTER: f64 = 1.2;

/// The folder entered is gone, and these channels are there (newest first).
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    pub entered: String,
    pub channels: Vec<Channel>,
}

/// The state of the question.
pub struct ChannelQuestion {
    /// Seconds after the start before the look is taken.
    delay: f64,
    started: bool,
    look: Option<Receiver<Option<Mismatch>>>,
    /// The question that is open.
    open: Option<Mismatch>,
    /// The text of the answer, shown until it is read ("From now on … is read").
    result: Option<String>,
}

impl Default for ChannelQuestion {
    fn default() -> Self {
        ChannelQuestion { delay: LOOK_AFTER, started: false, look: None, open: None, result: None }
    }
}

impl ChannelQuestion {
    /// A question that looks at once (the tests do not wait).
    pub fn at_once() -> ChannelQuestion {
        ChannelQuestion { delay: 0.0, ..ChannelQuestion::default() }
    }

    /// Puts a question up without the look (the tests).
    pub fn ask(&mut self, mismatch: Mismatch) {
        self.started = true;
        self.open = Some(mismatch);
    }

    /// Is the look still to come or under way? The window then draws again soon.
    pub fn waiting(&self) -> bool {
        !self.started || self.look.is_some()
    }

    /// Is a question or an answer on the screen?
    pub fn is_open(&self) -> bool {
        self.open.is_some() || self.result.is_some()
    }

    /// Takes the look once, `delay` seconds after the start (`now`: seconds of the window), and
    /// takes up its answer when it comes.
    pub fn poll(&mut self, paths: &Paths, now: f64) {
        if !self.started && now >= self.delay {
            self.started = true;
            let (sender, receiver) = mpsc::channel();
            let copy = paths.working_copy();
            std::thread::spawn(move || {
                let look = copy.channel_mismatch().map(|(entered, _used, channels)| Mismatch { entered, channels });
                let _ = sender.send(look);
            });
            self.look = Some(receiver);
        }
        if let Some(receiver) = &self.look
            && let Ok(look) = receiver.try_recv()
        {
            self.look = None;
            self.open = look;
        }
    }

    /// Draws the question or the answer; gives the folder the player chose, once.
    pub fn show(&mut self, ctx: &egui::Context, scheme: &Scheme) -> Option<String> {
        if let Some(text) = self.result.clone() {
            let (ok, close) = widgets::dialog(ctx, scheme, "kanal-ergebnis", |ui| {
                widgets::heading(ui, scheme, &t("s_kn_titel"));
                ui.label(&text);
                ui.add_space(8.0);
                ui.add(widgets::Button::new(&t("e_ok"), ButtonKind::Strong, scheme)).clicked()
            });
            if ok || close {
                self.result = None;
            }
            return None;
        }
        let mismatch = self.open.clone()?;
        let chosen = self.show_question(ctx, scheme, &mismatch);
        match chosen {
            Choice::Open => None,
            Choice::Later => {
                self.open = None;
                None
            }
            Choice::Folder(folder) => {
                self.open = None;
                let name = folder.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string();
                self.result = Some(tf("s_kn_umgestellt", &[("0", Arg::from(name.as_str()))]));
                Some(folder)
            }
        }
    }

    fn show_question(&self, ctx: &egui::Context, scheme: &Scheme, mismatch: &Mismatch) -> Choice {
        let entered = base_name(&mismatch.entered);
        let (choice, close) = widgets::dialog(ctx, scheme, "kanal-frage", |ui| {
            widgets::heading(ui, scheme, &t("s_kn_titel"));
            ui.label(tf("s_kn_weg", &[("0", Arg::from(entered.as_str()))]));
            let mut choice = Choice::Open;
            // The usual case is one channel beside it (LIVE became HOTFIX): a question with yes and no.
            if let [(name, folder, _)] = mismatch.channels.as_slice() {
                ui.label(tf("s_kn_da", &[("0", Arg::from(name.as_str()))]));
                ui.add_space(4.0);
                ui.label(t("s_kn_frage"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(widgets::Button::new(&t("e_ja"), ButtonKind::Strong, scheme)).clicked() {
                        choice = Choice::Folder(folder.clone());
                    }
                    if ui.add(widgets::Button::new(&t("s_kn_spaeter"), ButtonKind::Normal, scheme)).clicked() {
                        choice = Choice::Later;
                    }
                });
                return choice;
            }
            ui.add_space(4.0);
            ui.label(t("s_kn_mehrere"));
            for (name, folder, stamp) in &mismatch.channels {
                ui.add_space(6.0);
                if channel_box(ui, scheme, name, *stamp) {
                    choice = Choice::Folder(folder.clone());
                }
            }
            ui.add_space(10.0);
            if ui.add(widgets::Button::new(&t("s_kn_spaeter"), ButtonKind::Normal, scheme)).clicked() {
                choice = Choice::Later;
            }
            choice
        });
        // Esc or a click beside it counts as "not now".
        if close && matches!(choice, Choice::Open) { Choice::Later } else { choice }
    }
}

enum Choice {
    Open,
    Later,
    Folder(String),
}

/// The last part of a folder, as the question names it.
fn base_name(folder: &str) -> String {
    folder.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

/// A box for one channel that takes a click anywhere on it: the name and when it was played last.
fn channel_box(ui: &mut Ui, scheme: &Scheme, name: &str, stamp: f64) -> bool {
    let when = versekit_core::formats::date_time(versekit_core::language::current(), stamp);
    let frame = egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(crate::control_radius(scheme))
        .inner_margin(egui::Margin::symmetric(12, 8));
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(name).color(color(scheme.fg)));
        ui.label(
            egui::RichText::new(tf("s_kn_zuletzt", &[("0", Arg::from(when.as_str()))]))
                .small()
                .color(color(scheme.sub)),
        );
    });
    let response = ui.interact(shown.response.rect, ui.scope_id().with(("kanal", name)), Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(
            shown.response.rect,
            crate::control_radius(scheme),
            egui::Stroke::new(1.0, color(scheme.accent)),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2};
    use versekit_core::language::{self, Choice as Language};
    use versekit_core::theme;

    fn mismatch(channels: &[(&str, &str, f64)]) -> Mismatch {
        Mismatch {
            entered: "C:\\Games\\StarCitizen\\LIVE".into(),
            channels: channels.iter().map(|(n, f, s)| (n.to_string(), f.to_string(), *s)).collect(),
        }
    }

    /// Frames with input on a window without a screen: what was drawn, and what the question gave.
    struct Bench {
        ctx: egui::Context,
        question: ChannelQuestion,
        chosen: Vec<String>,
    }

    impl Bench {
        fn new(question: ChannelQuestion) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            Bench { ctx, question, chosen: Vec::new() }
        }

        fn frame(&mut self, events: Vec<Event>) -> Vec<(String, Pos2)> {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
                events,
                ..Default::default()
            };
            let question = &mut self.question;
            let chosen = &mut self.chosen;
            let mut output = self.ctx.run_ui(input, |ui| {
                if let Some(folder) = question.show(ui.ctx(), scheme) {
                    chosen.push(folder);
                }
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            texts
        }

        fn settle(&mut self) -> Vec<(String, Pos2)> {
            let mut texts = Vec::new();
            for _ in 0..4 {
                texts = self.frame(Vec::new());
            }
            texts
        }

        fn click(&mut self, label: &str) {
            let texts = self.settle();
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
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
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Language::De);
        guard
    }

    #[test]
    fn one_channel_beside_it_is_a_question_with_yes_and_not_now() {
        let _language = setup();
        let mut bench = Bench::new(ChannelQuestion::default());
        bench.question.ask(mismatch(&[("HOTFIX", "C:\\Games\\StarCitizen\\HOTFIX", 1_700_000_000.0)]));
        let texts = bench.settle();
        let shown = |part: &str| texts.iter().any(|(text, _)| text.contains(part));
        assert!(shown(&language::t("s_kn_titel")) && shown("LIVE") && shown("HOTFIX"), "{texts:?}");
        assert!(shown(&language::t("s_kn_frage")));
        // Not now: nothing changes, the question is gone.
        bench.click(&language::t("s_kn_spaeter"));
        assert!(bench.chosen.is_empty() && !bench.question.is_open());
        // Yes: the folder is given back once, and the answer stands there until it is read.
        bench.question.ask(mismatch(&[("HOTFIX", "C:\\Games\\StarCitizen\\HOTFIX", 1_700_000_000.0)]));
        bench.click(&language::t("e_ja"));
        assert_eq!(bench.chosen, ["C:\\Games\\StarCitizen\\HOTFIX"]);
        let texts = bench.settle();
        let said = tf("s_kn_umgestellt", &[("0", Arg::from("HOTFIX"))]);
        assert!(texts.iter().any(|(text, _)| *text == said), "{texts:?}");
        bench.click(&language::t("e_ok"));
        assert!(!bench.question.is_open(), "the answer is read");
        assert_eq!(bench.chosen.len(), 1, "given back once");
    }

    #[test]
    fn several_channels_are_boxes_with_the_time_they_were_played_and_a_click_chooses() {
        let _language = setup();
        let mut bench = Bench::new(ChannelQuestion::default());
        bench.question.ask(mismatch(&[
            ("PTU", "C:\\Games\\StarCitizen\\PTU", 1_700_100_000.0),
            ("HOTFIX", "C:\\Games\\StarCitizen\\HOTFIX", 1_700_000_000.0),
        ]));
        let texts = bench.settle();
        assert!(texts.iter().any(|(t, _)| *t == language::t("s_kn_mehrere")), "{texts:?}");
        let played =
            texts.iter().filter(|(t, _)| t.starts_with(&language::t("s_kn_zuletzt").replace("{0}", ""))).count();
        assert_eq!(played, 2, "a box for each channel with its time: {texts:?}");
        bench.click("HOTFIX");
        assert_eq!(bench.chosen, ["C:\\Games\\StarCitizen\\HOTFIX"], "the click on the box chooses that channel");
    }

    #[test]
    fn the_look_asks_once_for_each_start_of_the_program() {
        use versekit_core::jsonio::Value;
        let _language = setup();
        let dir = std::env::temp_dir().join(format!("versekit-kanal-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let games = dir.join("StarCitizen");
        std::fs::create_dir_all(games.join("HOTFIX")).expect("folder");
        std::fs::write(games.join("HOTFIX").join("Game.log"), "").expect("log");
        // Posix rules with slashes: the game folders of this computer are not looked at, only these.
        let slashes = |path: &std::path::Path| path.to_string_lossy().replace('\\', "/");
        let home = slashes(&dir);
        let host = versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::Posix,
            &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())],
            &home,
        );
        let paths = Paths::new(host);
        // The entered folder is gone: LIVE was renamed to HOTFIX.
        let entered = slashes(&games.join("LIVE"));
        assert!(paths.set_setting("spiel_ordner", Value::str(entered.as_str())));
        let mut question = ChannelQuestion::default();
        // Before the time is up nothing is looked at.
        question.poll(&paths, 0.5);
        assert!(!question.is_open() && question.look.is_none());
        question.poll(&paths, 2.0);
        for _ in 0..60 {
            question.poll(&paths, 2.0);
            if question.is_open() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let open = question.open.clone().expect("the neighbour channel is found");
        assert_eq!(open.entered, entered);
        assert_eq!(open.channels.len(), 1);
        assert!(open.channels[0].1.ends_with("HOTFIX"));
        // Not now: not asked again in this run of the program.
        question.open = None;
        question.poll(&paths, 100.0);
        std::thread::sleep(std::time::Duration::from_millis(200));
        question.poll(&paths, 100.0);
        assert!(!question.is_open(), "once for each start");
        // Where the entered folder is there, nothing is asked at all.
        let mut fresh = ChannelQuestion::at_once();
        assert!(paths.set_setting("spiel_ordner", Value::str(slashes(&games.join("HOTFIX")).as_str())));
        for _ in 0..10 {
            fresh.poll(&paths, 1.0);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!fresh.is_open(), "the entered folder carries");
        let _ = std::fs::remove_dir_all(dir);
    }
}
