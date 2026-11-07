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

//! No button is greyed out without saying why. Every page is drawn without a window, in both languages, and
//! every button a screen reader knows as disabled must carry a description: the hover text of
//! `widgets::Button::blocked` (and every `on_hover_text`) becomes the description of its node.
//!
//! ⚠ `on_disabled_hover_text` of egui shows a tooltip but gives the node no description; a button blocked that
//! way is reported as well. Use `widgets::Button::blocked` or add the reason with `on_hover_text`.

use versekit_core::language::{self, Choice};
use versekit_core::paths::Paths;
use versekit_core::theme;

use super::SamplePage;
use super::english::pages;
use super::real_data::{folder, host_in};

/// The names of the buttons that are disabled and say nothing about it, in the nodes of one frame.
fn silent(update: &egui::accesskit::TreeUpdate) -> Vec<String> {
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == egui::accesskit::Role::Button && node.is_disabled())
        .filter(|(_, node)| node.description().is_none_or(|d| d.trim().is_empty()))
        .map(|(_, node)| node.label().unwrap_or("(ohne Namen)").to_string())
        .collect()
}

/// The silent buttons of `page` open at `key` at one size.
fn silent_on(page: &mut SamplePage, key: &str, width: f32, height: f32) -> Vec<String> {
    page.go_to(key);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx.enable_accesskit();
    let mut found = Vec::new();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        output.textures_delta.clear();
        if let Some(update) = &output.platform_output.accesskit_update {
            found = silent(update);
        }
    }
    found
}

/// Every silent button of every page, as `case, page: name`.
fn findings() -> Vec<String> {
    let mut out = Vec::new();
    let mut look = |case: &str, page: &mut SamplePage| {
        for key in pages() {
            for name in silent_on(page, key, 1280.0, 900.0) {
                let line = format!("{case}, {}: {name}", if key.is_empty() { "overview" } else { key });
                if !out.contains(&line) {
                    out.push(line);
                }
            }
        }
    };
    for choice in [Choice::De, Choice::En] {
        language::set_thread_language(choice);
        theme::set_current("original");
        let mut sample = SamplePage::preview();
        look(&format!("sample {choice:?}"), &mut sample);
        let dir = folder("stumme-knoepfe");
        let mut real = SamplePage::with_settings(Paths::new(host_in(&dir, &[])));
        look(&format!("real data {choice:?}"), &mut real);
        let _ = std::fs::remove_dir_all(dir);
    }
    language::set_thread_language(Choice::Auto);
    out
}

#[test]
fn no_page_has_a_blocked_button_without_a_reason() {
    let found = findings();
    assert!(found.is_empty(), "{} button(s) blocked without a reason:\n{}", found.len(), found.join("\n"));
}

/// The check sees a button greyed out without a word and lets one with a reason pass.
#[test]
fn the_check_finds_a_blocked_button_without_a_reason() {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    ctx.enable_accesskit();
    let scheme = theme::by_id("original").expect("original");
    let mut found = Vec::new();
    for _ in 0..2 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add_enabled(false, egui::Button::new("Stumm"));
            ui.add_enabled(false, egui::Button::new("Nur Tooltip")).on_disabled_hover_text("Grund");
            ui.add(
                crate::widgets::Button::new("Mit Grund", crate::widgets::ButtonKind::Normal, scheme)
                    .blocked(Some("Grund")),
            );
            ui.add(crate::widgets::Button::new("Frei", crate::widgets::ButtonKind::Normal, scheme));
        });
        output.textures_delta.clear();
        if let Some(update) = &output.platform_output.accesskit_update {
            found = silent(update);
        }
    }
    found.sort();
    assert_eq!(found, ["Nur Tooltip", "Stumm"]);
}
