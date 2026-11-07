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

//! A jump from the menu filter frames the element it found, whatever page it is on: the accent frame
//! stands around exactly that element, the view scrolls to it once, and the frame is gone after
//! [`HIGHLIGHT_SECONDS`]. Every test clicks the hit in the menu like a player and reads the frames out of
//! the shapes of the frames drawn.
//!
//! Every test holds the language lock: it compares shown texts with the language file.

use egui::epaint::ClippedShape;
use egui::{Event, Pos2, Rect, vec2};
use versekit_core::language::{self, t};
use versekit_core::theme;

use super::SamplePage;
use super::real_data::real_page;
use crate::highlight::{HIGHLIGHT_SECONDS, probe};

/// The window of the tests: wide, and low enough that the end of a long page lies below the fold.
const SIZE: (f32, f32) = (1300.0, 640.0);

/// Where the page begins: right of the menu.
const PAGE_LEFT: f32 = 230.0;

/// A window without a screen, with a clock of its own: one frame is a sixtieth of a second.
struct Window {
    ctx: egui::Context,
    page: SamplePage,
    time: f64,
}

impl Window {
    fn new(page: SamplePage) -> Window {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        Window { ctx, page, time: 1.0 }
    }

    /// One frame after `step` seconds with `events`; the shapes it drew.
    fn frame(&mut self, events: Vec<Event>, step: f64) -> Vec<ClippedShape> {
        self.time += step;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(SIZE.0, SIZE.1))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let page = &mut self.page;
        let mut output = self.ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        output.textures_delta.clear();
        output.shapes
    }

    /// `count` frames one after the other; the shapes of the last.
    fn frames(&mut self, count: usize) -> Vec<ClippedShape> {
        let mut shapes = Vec::new();
        for _ in 0..count {
            shapes = self.frame(Vec::new(), 1.0 / 60.0);
        }
        shapes
    }

    /// A click on the text `label` of the menu as a player makes it.
    fn click_menu(&mut self, label: &str) {
        let shapes = self.frames(6);
        let rect = text_rect(&shapes, label, 0.0).unwrap_or_else(|| {
            let shown: Vec<String> = probe::texts(&shapes).into_iter().map(|(text, _)| text).collect();
            panic!("{label} is not drawn: {shown:?}")
        });
        let at = rect.left_top() + vec2(4.0, 4.0);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at)], 1.0 / 60.0);
        self.frame(vec![button(true)], 1.0 / 60.0);
        self.frame(vec![button(false)], 1.0 / 60.0);
    }

    /// Types `filter` in the menu filter and clicks the hit `hit`.
    fn jump(&mut self, filter: &str, hit: &str) {
        self.page.nav_filter = filter.to_string();
        self.click_menu(hit);
    }
}

/// The place of the first text drawn right of `left` that reads `text` (capitals do not count: a scheme may
/// write them).
fn text_rect(shapes: &[ClippedShape], text: &str, left: f32) -> Option<Rect> {
    probe::texts(shapes)
        .into_iter()
        // A long hint stands in one line: its first sentence or its beginning with „…“.
        .find(|(shown, rect)| rect.min.x >= left && crate::widgets::shown_as_hint(&shown.to_lowercase(), &text.to_lowercase()))
        .map(|(_, rect)| rect)
}

/// The first words of a text: what a player types to find a long text in the menu.
fn start_of(text: &str) -> String {
    text.split(' ').take(5).collect::<Vec<_>>().join(" ")
}

/// What a jump has to do on one page.
struct Case {
    /// The page on real data (its own folder) or the sample page.
    real: bool,
    /// The key of the page the hit leads to.
    page: &'static str,
    /// The text the filter finds, as the page shows it.
    found: String,
    /// What is typed in the filter to get the hit onto the menu.
    filter: String,
    /// Texts of the same page that stand right beside the element and must stay outside the frame.
    neighbours: Vec<String>,
    /// Texts that belong to the element and must stand inside the frame.
    inside: Vec<String>,
    /// Whether the element lies below the fold: the page has to scroll to it.
    below_fold: bool,
    /// The text of the element that stands on the page: the one found, or the name of its row when the filter
    /// found a hint (a hint stands on hover behind the symbol `info`).
    seen: String,
}

impl Case {
    fn new(real: bool, page: &'static str, found: &str) -> Case {
        Case {
            real,
            page,
            found: found.to_string(),
            filter: found.to_string(),
            neighbours: Vec::new(),
            inside: vec![found.to_string()],
            below_fold: false,
            seen: found.to_string(),
        }
    }

    /// The text found stands on hover only: `name`, the name of its row, stands on the page and in the frame.
    fn on_hover_of(mut self, name: &str) -> Case {
        self.inside.retain(|text| *text != self.found);
        self.inside.push(name.to_string());
        self.seen = name.to_string();
        self
    }

    fn typing(mut self, filter: &str) -> Case {
        self.filter = filter.to_string();
        self
    }

    fn beside(mut self, texts: &[String]) -> Case {
        self.neighbours = texts.to_vec();
        self
    }

    fn with(mut self, texts: &[String]) -> Case {
        self.inside.extend_from_slice(texts);
        self
    }

    fn below(mut self) -> Case {
        self.below_fold = true;
        self
    }
}

/// The window of a case: real data in a folder of its own, or the sample page.
fn window_for(case: &Case, name: &str) -> (Window, Option<std::path::PathBuf>) {
    theme::set_current("original");
    if case.real {
        let (page, dir) = real_page(name, "", &[]);
        (Window::new(page), Some(dir))
    } else {
        language::set_thread_language(language::Choice::De);
        (Window::new(SamplePage::preview()), None)
    }
}

/// Jumps to the element of `case` from the menu filter and checks what the page does about it.
fn check(case: &Case, name: &str) {
    let (mut window, dir) = window_for(case, name);
    let hit = format!("{} › {}", t(case.page), case.found);
    let scheme = theme::current();
    // The page as it is without a jump: no frame, and the element where the case says it is.
    window.page.go_to(case.page);
    let plain = window.frames(6);
    assert!(probe::frames(&plain, scheme).is_empty(), "{name}: nothing is framed before a jump");
    let seen_before = text_rect(&plain, &case.seen, PAGE_LEFT).is_some_and(|rect| rect.max.y <= SIZE.1);
    assert_eq!(
        seen_before,
        !case.below_fold,
        "{name}: the element {} the fold",
        if case.below_fold { "lies below" } else { "stands above" }
    );
    window.page.go_to("");
    window.frames(6);

    window.jump(&case.filter, &hit);
    assert_eq!(window.page.page, case.page, "{name}: the hit leads to the page");
    // Half a second on: the view has had time to scroll, the highlight still lasts.
    let lit = window.frames(30);
    let frames = probe::frames(&lit, scheme);
    assert_eq!(frames.len(), 1, "{name}: exactly one frame stands: {frames:?}");
    let probe::Frame { rect: frame, clip } = frames[0];
    // Everything that belongs to the element is on the screen and inside the frame, nothing beside it is.
    for text in &case.inside {
        let rect = text_rect(&lit, text, PAGE_LEFT).unwrap_or_else(|| panic!("{name}: “{text}” is not drawn"));
        assert!(clip.contains_rect(rect), "{name}: “{text}” at {rect:?} is outside the view {clip:?}");
        assert!(frame.contains_rect(rect), "{name}: the frame {frame:?} does not hold “{text}” at {rect:?}");
    }
    for text in &case.neighbours {
        if let Some(rect) = text_rect(&lit, text, PAGE_LEFT) {
            assert!(!frame.contains(rect.center()), "{name}: the frame {frame:?} holds “{text}” at {rect:?}");
        }
    }
    if case.below_fold {
        // The view went to the element: the frame itself is in view.
        assert!(clip.contains_rect(frame), "{name}: the frame {frame:?} is outside the view {clip:?}");
    }
    // The frame is the one of the element, not a big one around the page.
    assert!(frame.height() < SIZE.1 / 2.0, "{name}: the frame {frame:?} is too big for one element");

    // It fades after the time: the same page, a moment later, has no frame.
    let gone = window.frame(Vec::new(), HIGHLIGHT_SECONDS + 0.5);
    assert!(probe::frames(&gone, scheme).is_empty(), "{name}: the frame is gone after the time");
    if let Some(dir) = dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_jump_to_a_switch_frames_the_switch_with_its_help_and_nothing_else() {
    let _language = crate::language_lock();
    // Its help on hover: the frame holds the switch with its state line.
    let case =
        Case::new(true, "hf_spiel", &t("aafk_titel")).typing("afk").beside(&[t("aafk_warten"), t("aafk_hinweis")]).below();
    check(&case, "filter-jump-switch");
}

#[test]
fn a_jump_to_the_help_of_a_switch_frames_the_same_switch() {
    let _language = crate::language_lock();
    let case = Case::new(true, "hf_spiel", &t("aafk_h"))
        .typing(&start_of(&t("aafk_h")))
        .on_hover_of(&t("aafk_titel"))
        .beside(&[t("aafk_warten"), t("aafk_hinweis")])
        .below();
    check(&case, "filter-jump-switch-help");
}

#[test]
fn a_jump_to_the_note_of_risk_frames_the_box_of_the_note() {
    let _language = crate::language_lock();
    let case = Case::new(true, "hf_spiel", &t("aafk_hinweis"))
        .typing(&start_of(&t("aafk_hinweis")))
        .with(&[t("aafk_hinweis_h")])
        .beside(&[t("aafk_warten_h"), t("s_uc_titel")])
        .below();
    check(&case, "filter-jump-note");
}

#[test]
fn a_jump_to_the_line_of_the_note_of_risk_frames_the_same_box() {
    let _language = crate::language_lock();
    let case = Case::new(true, "hf_spiel", &t("aafk_hinweis_h"))
        .typing("afk")
        .with(&[t("aafk_hinweis")])
        .beside(&[t("aafk_warten_h"), t("s_uc_titel")])
        .below();
    check(&case, "filter-jump-note-line");
}

#[test]
fn a_jump_to_a_heading_frames_the_heading() {
    let _language = crate::language_lock();
    let case = Case::new(true, "hf_spiel", &t("s_uc_titel")).beside(&[t("s_uc_lead"), t("aafk_hinweis")]).below();
    check(&case, "filter-jump-heading");
}

#[test]
fn a_jump_to_a_hint_frames_the_hint() {
    let _language = crate::language_lock();
    let case = Case::new(true, "hf_spiel", &t("s_uc_lead")).typing("g_language").beside(&[t("s_uc_titel")]).below();
    check(&case, "filter-jump-hint");
}

#[test]
fn a_jump_to_a_setting_on_the_overlay_page_frames_its_row() {
    let _language = crate::language_lock();
    let rows = super::settings_of("hf_overlay");
    let name = super::label(rows[0].1[1].0);
    let other = super::label(rows[0].1[0].0);
    // One line each on this card: the hint stands on hover, not in the frame.
    let case = Case::new(false, "hf_overlay", &name).beside(&[other]);
    check(&case, "filter-jump-overlay-row");
}

#[test]
fn the_own_account_is_found_by_its_words_and_the_jump_frames_its_row_on_the_general_page() {
    let _language = crate::language_lock();
    // Its hint on hover: the frame holds the row by its name.
    let case = Case::new(true, "hf_allgemein", &t("s_er_acc")).typing("handl");
    check(&case, "filter-jump-account");
}

#[test]
fn the_account_words_find_the_own_account_and_not_the_card_the_connection_page_shows_only_now_and_then() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let general = format!("{} › {}", t("hf_allgemein"), t("s_er_acc"));
    for filter in ["acc", "handl", "konto", "spielername"] {
        let hits = super::nav_hits_in(filter, "hf_gruppe_einst", "hf_allgemein", false);
        assert!(hits.contains(&general), "{filter}: {hits:?}");
    }
    let connection = super::nav_hits_in("acc", "hf_gruppe_einst", "k_basetool_verbindung", false);
    assert!(!connection.iter().any(|h| h.contains(&t("s_btv_konto"))), "{connection:?}");
    // On the sample page the row of the made-up connection page stays findable.
    let sample = super::nav_hits_in("acc", "hf_gruppe_einst", "k_basetool_verbindung", true);
    assert!(sample.iter().any(|h| h.contains(&t("s_btv_konto"))), "{sample:?}");
    language::set_language(language::Choice::En);
    let hits = super::nav_hits_in("player name", "hf_gruppe_einst", "hf_allgemein", false);
    assert!(hits.contains(&format!("{} › {}", t("hf_allgemein"), t("s_er_acc"))), "{hits:?}");
    language::set_language(language::Choice::De);
}

/// While blueprints of another account did not count, the blueprint tile of the overview leads to the own account
/// on the page Allgemein and frames its row; without any it leads where it always leads.
#[test]
fn the_blueprint_tile_leads_to_the_own_account_while_blueprints_of_another_account_did_not_count() {
    let _language = crate::language_lock();
    let (page, dir) = real_page("kachel-fremd", "", &[]);
    let mut window = Window::new(page);
    let title = t("s_uv_bauplaene");
    let click_tile = |window: &mut Window| {
        window.page.go_to("");
        let shapes = window.frames(8);
        let rect = text_rect(&shapes, &title, PAGE_LEFT).unwrap_or_else(|| panic!("{title} is not drawn"));
        let at = rect.center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        window.frame(vec![Event::PointerMoved(at)], 1.0 / 60.0);
        window.frame(vec![button(true)], 1.0 / 60.0);
        window.frame(vec![button(false)], 1.0 / 60.0);
        window.frames(2);
    };
    click_tile(&mut window);
    assert_ne!(window.page.page, "hf_allgemein", "without blueprints left out: the usual page");
    let file = window.page.store.as_ref().expect("real data").paths.app_file(versekit_core::logsource::FOREIGN_FILE);
    let tally = r#"{"sitzungen": {"datei:a.log": {"datei": "a.log", "bauplaene": ["x"]}}}"#;
    std::fs::write(file, tally).unwrap();
    click_tile(&mut window);
    assert_eq!(window.page.page, "hf_allgemein", "to the own account");
    let scheme = theme::current();
    let lit = window.frames(30);
    let frames = probe::frames(&lit, scheme);
    let row = text_rect(&lit, &t("s_er_acc"), PAGE_LEFT).expect("the row of the account");
    assert!(frames.len() == 1 && frames[0].rect.contains_rect(row), "the row is framed: {frames:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Every setting row of every page on real data is found by its name in the menu filter: a row the filter does not
/// know cannot be found by anybody who looks for it.
#[test]
fn every_setting_row_of_every_page_is_found_by_its_name_in_the_menu_filter() {
    let _language = crate::language_lock();
    let (page, dir) = real_page("filter-alle-zeilen", "", &[]);
    let mut window = Window::new(page);
    let mut missing = Vec::new();
    for (group, pages) in super::NAV {
        for (key, _) in pages.iter() {
            window.page.go_to(key);
            let _ = crate::settings::column_log::take_names();
            window.frames(8);
            let mut names = crate::settings::column_log::take_names();
            names.sort();
            names.dedup();
            // The row at the top of the page Grafik tells what waits to be written: a state, not a setting.
            let state = t("s_gr_alles_drin");
            for name in names.into_iter().filter(|n| !n.trim().is_empty() && *n != state) {
                // Found: typing the name gives an entry that leads to this page.
                if super::nav_hits_in(&name, group, key, false).is_empty() {
                    missing.push(format!("{key}: {name}"));
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    assert!(missing.is_empty(), "{} rows the menu filter does not find: {missing:#?}", missing.len());
}

#[test]
fn a_jump_to_something_on_another_page_frames_it_too() {
    let _language = crate::language_lock();
    // A button on the page about the program, and the link below it.
    let button = Case::new(false, "hf_ueber", &t("s_up_sofort")).beside(&[t("s_mu_projektseite")]);
    check(&button, "filter-jump-button-about");
    let link = Case::new(false, "hf_ueber", &t("s_mu_projektseite")).beside(&[t("s_up_sofort")]);
    check(&link, "filter-jump-link-about");
}
