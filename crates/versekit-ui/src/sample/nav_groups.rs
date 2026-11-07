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

//! The folded groups of the menu are kept in the settings file under the names of 4.12 (`gruppe_zu_<id>`): a group
//! folded there starts folded, and a click on its head is written.

use egui::{Event, Pos2, Rect, vec2};
use versekit_core::language::t;

use super::SamplePage;
use super::real_data::real_page;

const SIZE: (f32, f32) = (1300.0, 820.0);

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

/// A few quiet frames (the fold takes its time); the texts of the last.
fn settle(ctx: &egui::Context, page: &mut SamplePage) -> Vec<(String, Pos2)> {
    let mut texts = Vec::new();
    for _ in 0..30 {
        texts = frame(ctx, page, Vec::new());
    }
    texts
}

fn click(ctx: &egui::Context, page: &mut SamplePage, label: &str) -> Vec<(String, Pos2)> {
    let texts = settle(ctx, page);
    let (_, at) = texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("{label:?} not drawn: {texts:?}"));
    let at = *at + vec2(4.0, 4.0);
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

fn shows(texts: &[(String, Pos2)], text: &str) -> bool {
    texts.iter().any(|(shown, _)| shown == text)
}

#[test]
fn a_group_folded_in_4_12_starts_folded_and_a_click_on_its_head_is_kept() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("gruppen", "{\n  \"gruppe_zu_logistik\": true\n}\n", &[]);
    page.open_overview();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let (head, entry, other) = (t("hf_gruppe_logistik").to_uppercase(), t("hf_hangar"), t("hf_liste"));
    let texts = settle(&ctx, &mut page);
    assert!(shows(&texts, &other), "another group is open: {texts:?}");
    assert!(!shows(&texts, &entry), "the group folded in 4.12 starts folded: {texts:?}");

    let texts = click(&ctx, &mut page, &head);
    assert!(shows(&texts, &entry), "a click opens it: {texts:?}");
    let file = || std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    assert!(file().contains("\"gruppe_zu_logistik\": false"), "{}", file());

    let texts = click(&ctx, &mut page, &head);
    assert!(!shows(&texts, &entry), "folded again");
    assert!(file().contains("\"gruppe_zu_logistik\": true"), "{}", file());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_order_of_the_groups_is_kept_and_starts_as_it_was() {
    let _language = crate::language_lock();
    let stored = "{\n  \"menue_gruppen_reihenfolge\": [\"info\", \"gibt_es_nicht\", \"handel\", \"info\"]\n}\n";
    let (mut page, dir) = real_page("gruppen-reihe", stored, &[]);
    // Info and Handel first, an unknown or doubled id left out, the rest in the usual order.
    let names: Vec<&str> = page.group_order.iter().map(|&g| super::NAV[g].0).collect();
    assert_eq!(names[..3], ["hf_gruppe_info", "hf_gruppe_handel", "hf_gruppe_bp"], "{names:?}");
    assert_eq!(page.group_order.len(), super::NAV.len(), "every group once");
    page.open_overview();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let texts = settle(&ctx, &mut page);
    let y = |key: &str| {
        let head = t(key).to_uppercase();
        texts.iter().find(|(text, _)| *text == head).map(|(_, at)| at.y).unwrap_or_else(|| panic!("{head}: {texts:?}"))
    };
    assert!(y("hf_gruppe_info") < y("hf_gruppe_handel") && y("hf_gruppe_handel") < y("hf_gruppe_bp"), "drawn in that order");

    // A new order (as a drag leaves it) is written with the ids of the groups.
    page.group_order.swap(0, 1);
    let _ = settle(&ctx, &mut page);
    let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    assert!(file.contains("\"handel\",") && file.find("\"handel\"") < file.find("\"info\""), "{file}");
    let _ = std::fs::remove_dir_all(dir);
}
