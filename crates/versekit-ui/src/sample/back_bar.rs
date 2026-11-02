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

//! The way back after a jump: a jump out of a page with a row in the menu puts "Back to <page>" above the
//! page it led to, a click on it leads back, and a page picked in the menu has none.

use egui::{Event, Pos2, Rect, vec2};
use versekit_core::language::{self, Arg, t, tf};

use super::SamplePage;

const SIZE: (f32, f32) = (1300.0, 800.0);

/// One frame with `events`; the texts it drew with their places.
fn frame(ctx: &egui::Context, page: &mut SamplePage, events: Vec<Event>) -> Vec<(String, Pos2)> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(SIZE.0, SIZE.1))),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        page.show(ui);
    });
    output.textures_delta.clear();
    let mut texts = Vec::new();
    fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts);
    }
    texts
}

/// A few quiet frames; the texts of the last.
fn settle(ctx: &egui::Context, page: &mut SamplePage) -> Vec<(String, Pos2)> {
    let mut texts = Vec::new();
    for _ in 0..4 {
        texts = frame(ctx, page, Vec::new());
    }
    texts
}

/// A click on the text `label` as a player makes it; the texts after it settled.
fn click(ctx: &egui::Context, page: &mut SamplePage, label: &str) -> Vec<(String, Pos2)> {
    let texts = settle(ctx, page);
    let (_, at) = texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("{label:?} not drawn: {texts:?}"));
    let at = *at + vec2(4.0, 6.0);
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(ctx, page, vec![Event::PointerMoved(at)]);
    frame(ctx, page, vec![button(true)]);
    frame(ctx, page, vec![button(false)]);
    settle(ctx, page)
}

/// The jump of the blueprint list to the crafting page of its first blueprint, as its name does when clicked.
fn jump_from_the_list(ctx: &egui::Context, page: &mut SamplePage) -> Vec<(String, Pos2)> {
    page.go_to("hf_liste");
    settle(ctx, page);
    let name = page.blueprints.catalog.blueprints.first().expect("a blueprint in the sample").name.clone();
    page.blueprint_view.craft_jump = Some(name);
    let texts = settle(ctx, page);
    assert_eq!(page.page, "hf_herstellung");
    texts
}

#[test]
fn a_jump_out_of_a_page_without_a_row_in_the_menu_offers_no_way_back() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut page = SamplePage::preview();
    page.go_to("hf_wasistneu");
    let back = tf("hf_zurueck_seite", &[("0", Arg::from(t("hf_wasistneu").as_str()))]);
    // The gallery opens the page of a news item: a jump, but "Was ist neu" has no row in the menu.
    let texts = click(&ctx, &mut page, &t("s_mu_galerie_seite"));
    assert_ne!(page.page, "hf_wasistneu", "the gallery jumped");
    assert!(page.came_from.is_none(), "{:?}", page.came_from);
    assert!(!texts.iter().any(|(text, _)| *text == back), "{texts:?}");

    // The same look finds the bar after a page with a row in the menu.
    let mut page = SamplePage::preview();
    let back = tf("hf_zurueck_seite", &[("0", Arg::from(t("hf_liste").as_str()))]);
    let texts = jump_from_the_list(&ctx, &mut page);
    assert!(texts.iter().any(|(text, _)| *text == back), "{back:?} above crafting: {texts:?}");
}

#[test]
fn the_jump_from_the_blueprint_list_to_crafting_leads_back_to_the_list() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut page = SamplePage::preview();
    let back = tf("hf_zurueck_seite", &[("0", Arg::from(t("hf_liste").as_str()))]);
    let texts = jump_from_the_list(&ctx, &mut page);
    assert!(texts.iter().any(|(text, _)| *text == back), "{back:?} above crafting: {texts:?}");
    // The click leads back, and the bar is gone there.
    let texts = click(&ctx, &mut page, &back);
    assert_eq!(page.page, "hf_liste");
    assert!(!texts.iter().any(|(text, _)| *text == back), "{texts:?}");
    // A step back in the history: forward leads to the page of the jump again.
    assert!(page.history.can_go_forward(), "forward leads to crafting");
}

#[test]
fn the_page_rename_ships_stays_says_where_the_names_went_and_leads_to_the_hangar_without_a_way_back() {
    let _language = crate::language_lock();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    for choice in [language::Choice::De, language::Choice::En] {
        language::set_language(choice);
        let mut page = SamplePage::preview();
        page.go_to("hf_asop");
        let texts = settle(&ctx, &mut page);
        // The page itself, not the hangar in its place.
        assert_eq!(page.page, "hf_asop");
        for key in ["hf_asop", "s_as_umgezogen_2", "s_as_zum_hangar"] {
            assert!(texts.iter().any(|(text, _)| *text == t(key)), "{key}: {texts:?}");
        }
        let texts = click(&ctx, &mut page, &t("s_as_zum_hangar"));
        assert_eq!(page.page, "hf_hangar");
        let back = tf("hf_zurueck_seite", &[("0", Arg::from(t("hf_asop").as_str()))]);
        assert!(!texts.iter().any(|(text, _)| *text == back), "{texts:?}");
    }
    language::set_language(language::Choice::De);
}

#[test]
fn a_page_picked_otherwise_after_the_jump_drops_the_way_back() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut page = SamplePage::preview();
    let back = tf("hf_zurueck_seite", &[("0", Arg::from(t("hf_liste").as_str()))]);
    let texts = jump_from_the_list(&ctx, &mut page);
    assert!(texts.iter().any(|(text, _)| *text == back), "{texts:?}");
    let target = page.page.clone();
    // Another page, as from the menu: the bar goes, and it does not come back with the page of the jump.
    page.page = "hf_ueber".to_string();
    let texts = settle(&ctx, &mut page);
    assert!(!texts.iter().any(|(text, _)| *text == back), "{texts:?}");
    page.page = target;
    let texts = settle(&ctx, &mut page);
    assert!(!texts.iter().any(|(text, _)| *text == back), "{texts:?}");
}
