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

//! The sync button in the head of the pages blueprint list, My stock and
//! hangar (`pages._sync_head`, `SYNC_PAGES` of 4.12): "Sync now" with what the
//! last pass made of this area, next to the heading.
//!
//! The button is there only when the switch of the area is on, its rights are
//! granted and the connection stands. A click lets an expired hold of the route
//! of this page go once and asks for a pass; the line under the head tells
//! how the last pass ended (done, waiting for the browser, differing amounts, a
//! refusal). It is worked out from the status on every frame, so it follows the
//! language, and it wraps, because the texts are long.

use egui::{RichText, Ui};
use versekit_core::basetool::overview::{self, Verdict};
use versekit_core::basetool::scopes::{SETTING_BLUEPRINTS, SETTING_SHIPS, SETTING_STOCK};
use versekit_core::basetool::sync::{Phase, Status};
use versekit_core::jsonio::Map;
use versekit_core::language::t;
use versekit_core::theme::Scheme;

use crate::basetool_page::Wire;
use crate::color;
use crate::widgets::{self, ButtonKind};

/// The area a page syncs, if it is one of the pages with the button.
pub fn setting_of(page: &str) -> Option<&'static str> {
    match page {
        "hf_liste" => Some(SETTING_BLUEPRINTS),
        "hf_meinlager" => Some(SETTING_STOCK),
        "hf_hangar" => Some(SETTING_SHIPS),
        _ => None,
    }
}

/// What the buttons remember: the pass one of them asked for, and the status
/// the lines are worked out from (so a change of language shows at once).
#[derive(Default)]
pub struct HeadSync {
    /// The area and the generation of the status when the button was pressed.
    waiting: Option<(&'static str, u64)>,
    /// The status and the state of the sync at the generation given.
    seen: Option<(u64, std::sync::Arc<Status>, Map)>,
}

impl HeadSync {
    /// Whether the button of `setting` shows at all (switch, rights, connection).
    pub fn ready(wire: &Wire, setting: &str) -> bool {
        wire.sync.area_visible(setting)
    }

    /// The test mode with the switch of `setting` on: the button stands, does nothing and says why
    /// (no pass runs, so the rights are never known).
    pub fn locked(wire: &Wire, setting: &str) -> bool {
        wire.sync.connection().test_locked() && wire.sync.paths().setting_bool(setting, false)
    }

    /// Takes the newest status (the state is read only when the status changed) and ends the wait
    /// once the pass the button asked for is over.
    pub fn poll(&mut self, wire: &Wire) {
        let (generation, status) = wire.sync.status().get();
        let Some(status) = status else { return };
        if self.seen.as_ref().is_none_or(|(known, _, _)| *known != generation) {
            self.seen = Some((generation, status.clone(), wire.sync.state().unwrap_or_default()));
        }
        if let Some((_, since)) = self.waiting
            && generation > since
            && status.phase != Phase::Running
        {
            self.waiting = None;
        }
    }

    /// Whether a pass the button asked for, or any pass, is running for `setting`.
    fn running(&self, setting: &str) -> bool {
        self.waiting.is_some_and(|(asking, _)| asking == setting)
            || self.seen.as_ref().is_some_and(|(_, status, _)| status.phase == Phase::Running)
    }

    /// The button, for the right side of the head. It does nothing while a pass runs.
    pub fn button(&mut self, ui: &mut Ui, scheme: &Scheme, wire: &Wire, setting: &'static str) {
        self.poll(wire);
        let word = t("s_bt_jetzt");
        if Self::locked(wire, setting) {
            let reason = t("s_btv_testbetrieb");
            ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw").blocked(Some(&reason)));
            return;
        }
        if !Self::ready(wire, setting) {
            return;
        }
        let reason = t("s_btv_sync_laeuft");
        let blocked = self.running(setting).then_some(reason.as_str());
        let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rotate-cw").blocked(blocked);
        if ui.add(button).clicked() {
            // The generation is read before the pass is asked for: a pass that ends at once still counts as later.
            self.waiting = Some((setting, wire.sync.status().generation()));
            let routes: Vec<&str> = overview::route_of(setting).into_iter().collect();
            wire.sync.release_held(&routes);
        }
    }

    /// How the last pass ended, on a line of its own under the head where it can wrap.
    pub fn line(&mut self, ui: &mut Ui, scheme: &Scheme, wire: &Wire, setting: &'static str) {
        self.poll(wire);
        if Self::locked(wire, setting) {
            ui.add(egui::Label::new(RichText::new(t("s_btv_testbetrieb")).small().color(color(scheme.yellow))).wrap());
            return;
        }
        if !Self::ready(wire, setting) {
            return;
        }
        if self.running(setting) {
            widgets::loading(ui, scheme, &t("s_btv_sync_laeuft"));
            return;
        }
        let Some((_, status, state)) = self.seen.as_ref().filter(|(_, status, _)| status.phase != Phase::Idle) else {
            return;
        };
        let (text, verdict) = overview::sync_result(status, state, Some(setting), wire.sync.now());
        let tone = match verdict {
            Verdict::Done => scheme.sub,
            Verdict::Waiting => scheme.yellow,
            Verdict::Failed => scheme.red,
        };
        ui.add(egui::Label::new(RichText::new(text).small().color(color(tone))).wrap());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use versekit_core::basetool::exchange_double::ExchangeDouble;
    use versekit_core::jsonio::Value;
    use versekit_core::language::{self, Choice};

    use super::*;

    fn folder(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-kopf-sync-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().replace('\\', "/")
    }

    /// One frame with the button in a layout from the right; the texts and where they stand.
    fn frame(
        ctx: &egui::Context,
        head: &mut HeadSync,
        wire: &Wire,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 200.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            // A row of the height of a head, as on the page, so that the line has room under it.
            ui.allocate_ui_with_layout(
                egui::vec2(1000.0, 32.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| head.button(ui, scheme, wire, SETTING_BLUEPRINTS),
            );
            head.line(ui, scheme, wire, SETTING_BLUEPRINTS);
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        fn collect(shape: &egui::Shape, into: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        texts
    }

    fn click(ctx: &egui::Context, head: &mut HeadSync, wire: &Wire, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, head, wire, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, head, wire, vec![button(true)]);
        frame(ctx, head, wire, vec![button(false)]);
    }

    #[test]
    fn the_button_shows_only_with_the_switch_the_rights_and_the_connection() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let double = Arc::new(ExchangeDouble::new());
        let dir = folder("sichtbar");
        let wire = Wire::over_double(&double, &dir);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut head = HeadSync::default();
        let word = t("s_bt_jetzt");
        // Switch off: no button.
        assert!(!frame(&ctx, &mut head, &wire, Vec::new()).iter().any(|(text, _)| *text == word));
        // Switch on, but no pass yet: the rights are not known.
        assert!(wire.sync.set_setting(SETTING_BLUEPRINTS, Value::Bool(true)));
        assert!(!HeadSync::ready(&wire, SETTING_BLUEPRINTS));
        wire.sync.pass_now().expect("pass");
        assert!(HeadSync::ready(&wire, SETTING_BLUEPRINTS));
        assert!(frame(&ctx, &mut head, &wire, Vec::new()).iter().any(|(text, _)| *text == word));
        // Another area is not on: its page has no button.
        assert!(!HeadSync::ready(&wire, SETTING_STOCK));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_click_asks_for_a_pass_and_the_line_tells_how_it_ended() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let double = Arc::new(ExchangeDouble::new());
        let dir = folder("klick");
        let wire = Wire::over_double(&double, &dir);
        assert!(wire.sync.set_setting(SETTING_BLUEPRINTS, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut head = HeadSync::default();
        let word = t("s_bt_jetzt");
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        let at =
            texts.iter().find(|(text, _)| *text == word).map(|(_, pos)| *pos + egui::vec2(8.0, 6.0)).expect("button");
        click(&ctx, &mut head, &wire, at);
        assert!(head.waiting.is_some(), "the click asked for a pass");
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(texts.iter().any(|(text, _)| *text == t("s_btv_sync_laeuft")), "{texts:?}");
        // The pass is over: the line tells it.
        wire.sync.pass_now().expect("pass");
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(head.waiting.is_none());
        let status = wire.sync.status().get().1.expect("status");
        let said = overview::sync_result(
            &status,
            &wire.sync.state().unwrap_or_default(),
            Some(SETTING_BLUEPRINTS),
            wire.sync.now(),
        )
        .0;
        assert!(texts.iter().any(|(text, _)| *text == said), "{texts:?} / {said}");
        // A refusal is told as a refusal.
        double.fail("", 503, "SERVICE_UNAVAILABLE", Some(30), 1);
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        let at =
            texts.iter().find(|(text, _)| *text == word).map(|(_, pos)| *pos + egui::vec2(8.0, 6.0)).expect("button");
        click(&ctx, &mut head, &wire, at);
        let _ = wire.sync.pass_now();
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        // Maintenance is not told as a refusal and shows no code, it says that Verse-Kit tries again.
        let failed = t("s_btv_f_netz");
        assert!(texts.iter().any(|(text, _)| *text == failed), "{texts:?} / {failed}");
        assert!(!texts.iter().any(|(text, _)| text.contains("SERVICE_UNAVAILABLE")), "{texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_long_line_wraps_in_a_narrow_column_and_is_never_cut() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let double = Arc::new(ExchangeDouble::new());
        let dir = folder("schmal");
        let wire = Wire::over_double(&double, &dir);
        assert!(wire.sync.set_setting(SETTING_BLUEPRINTS, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        double.fail("", 503, "SERVICE_UNAVAILABLE", Some(30), 1);
        let _ = wire.sync.pass_now();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::current();
        let mut head = HeadSync::default();
        let mut rows = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(260.0, 400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| head.line(ui, scheme, &wire, SETTING_BLUEPRINTS));
            output.textures_delta.clear();
            rows.clear();
            fn collect(shape: &egui::Shape, into: &mut Vec<(usize, bool, String)>) {
                match shape {
                    egui::Shape::Text(text) => {
                        into.push((text.galley.rows.len(), text.galley.elided, text.galley.text().to_string()));
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut rows));
        }
        let long = t("s_btv_f_netz");
        let (count, elided, _) = rows.iter().find(|(_, _, text)| *text == long).expect("the line is drawn");
        assert!(*count >= 2 && !*elided, "{count} rows, cut: {elided}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_line_follows_the_language_and_a_refusal_stays_until_a_pass_goes_through() {
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let double = Arc::new(ExchangeDouble::new());
        let dir = folder("sprache");
        let wire = Wire::over_double(&double, &dir);
        assert!(wire.sync.set_setting(SETTING_BLUEPRINTS, Value::Bool(true)));
        wire.sync.pass_now().expect("pass");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut head = HeadSync::default();
        let said =
            |texts: &[(String, egui::Pos2)], prefix: &str| texts.iter().any(|(text, _)| text.starts_with(prefix));
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(said(&texts, "Mit dem Basetool abgeglichen"), "{texts:?}");
        // The same line in the other language, without another click.
        language::set_language(Choice::En);
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(said(&texts, "Synced with the Basetool"), "{texts:?}");
        // A refusal shows as it is now, and goes when the next pass went through.
        double.fail("", 503, "SERVICE_UNAVAILABLE", Some(30), 1);
        let _ = wire.sync.pass_now();
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(said(&texts, &t("s_btv_f_netz")), "{texts:?}");
        wire.sync.pass_now().expect("pass");
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(said(&texts, "Synced with the Basetool"), "{texts:?}");
        language::set_language(Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn on_a_copy_of_the_data_the_button_does_nothing_and_says_why() {
        use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_NO_NET, ENV_SECRETS, ENV_SECRETS_FILE};
        let _language = crate::language_lock();
        language::set_language(Choice::De);
        let root = folder("testbetrieb");
        std::fs::create_dir_all(std::path::Path::new(&root).join(versekit_core::paths::COPY_DATA_FOLDER)).unwrap();
        let secrets = format!("{root}/geheim");
        let vars = [
            (ENV_SECRETS, secrets.as_str()),
            (ENV_SECRETS_FILE, "1"),
            (ENV_NO_NET, "1"),
            (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
        ];
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &root);
        let paths = versekit_core::paths::Paths::for_copy(host, &root).expect("a copy");
        let wire = Wire::system(Arc::new(paths), Arc::new(|| Some("PlayerOne".to_string())), "5.0.0").expect("wire");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut head = HeadSync::default();
        let (word, reason) = (t("s_bt_jetzt"), t("s_btv_testbetrieb"));
        // Switch off: nothing, as on the player's own data.
        assert!(frame(&ctx, &mut head, &wire, Vec::new()).is_empty());
        // Switch on: the button and the line that says why it does nothing.
        assert!(wire.sync.set_setting(SETTING_BLUEPRINTS, Value::Bool(true)));
        assert!(HeadSync::locked(&wire, SETTING_BLUEPRINTS) && !HeadSync::ready(&wire, SETTING_BLUEPRINTS));
        let texts = frame(&ctx, &mut head, &wire, Vec::new());
        assert!(texts.iter().any(|(text, _)| *text == word) && texts.iter().any(|(text, _)| *text == reason));
        let at =
            texts.iter().find(|(text, _)| *text == word).map(|(_, pos)| *pos + egui::vec2(8.0, 6.0)).expect("button");
        click(&ctx, &mut head, &wire, at);
        assert!(head.waiting.is_none(), "a click asks for nothing");
        wire.sync.stop();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn three_pages_have_the_button_and_each_syncs_its_own_area() {
        assert_eq!(setting_of("hf_liste"), Some(SETTING_BLUEPRINTS));
        assert_eq!(setting_of("hf_meinlager"), Some(SETTING_STOCK));
        assert_eq!(setting_of("hf_hangar"), Some(SETTING_SHIPS));
        assert_eq!(setting_of("hf_staffellager"), None, "the unit storage has its own button");
        assert_eq!(setting_of("hf_joysticks"), None);
    }
}
