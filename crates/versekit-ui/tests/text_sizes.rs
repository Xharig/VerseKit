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

//! The sizes of text the pages really draw, measured without a window (egui_kittest without a renderer): every
//! page of the main window with the sample data. The size is the one of the font in the drawn text, in interface
//! points, so it does not change with the size of the interface. Every text has one of the four sizes of the
//! interface (small, body, card title, heading); `-- --nocapture` prints how many lines each size has.

use std::collections::BTreeMap;

use egui::epaint::Shape;
use egui::vec2;
use egui_kittest::Harness;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::theme;
use versekit_ui::sample::SamplePage;
use versekit_ui::widgets::CARD_TITLE_SCALE;
use versekit_ui::{BODY_SIZE, HEADING_SIZE, SMALL_SIZE};

/// Frames a page gets to settle before it is measured.
const SETTLE_FRAMES: usize = 6;

/// A window like the usual one, in interface points.
const WINDOW: egui::Vec2 = vec2(1280.0, 820.0);

/// Text that has a size of its own on purpose, each with its page and its reason.
const OWN_SIZES: [(&str, &str); 1] = [(
    "hf_overlay",
    "the copy of the pill of the scanner in the game: its text is as high as the pill it copies",
)];

/// One size of text on one page: how many lines have it and one example.
#[derive(Default)]
struct Seen {
    lines: usize,
    example: String,
}

/// The sizes of text found, by page and size in tenths of a point.
type Sizes = BTreeMap<(String, i32), Seen>;

fn tenths(size: f32) -> i32 {
    (size * 10.0).round() as i32
}

/// The sizes of text the interface sets: small, body, title of a card, heading.
fn steps() -> [i32; 4] {
    [tenths(SMALL_SIZE), tenths(BODY_SIZE), tenths(BODY_SIZE * CARD_TITLE_SCALE), tenths(HEADING_SIZE)]
}

/// Adds the text of `shape`, by size.
fn add_sizes(shape: &Shape, page: &str, into: &mut Sizes) {
    match shape {
        Shape::Text(text) => {
            for section in &text.galley.job.sections {
                let range = section.byte_range.start.0..section.byte_range.end.0;
                let part = text.galley.job.text.get(range).unwrap_or_default().trim();
                if part.is_empty() {
                    continue;
                }
                let seen = into.entry((page.to_string(), tenths(section.format.font_id.size))).or_default();
                seen.lines += 1;
                if seen.example.is_empty() {
                    seen.example = part.chars().take(50).collect();
                }
            }
        }
        Shape::Vec(shapes) => shapes.iter().for_each(|inner| add_sizes(inner, page, into)),
        _ => {}
    }
}

/// Every size of text every page of the main window draws.
fn measure() -> Sizes {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let mut found = Sizes::new();
    for key in SamplePage::page_keys() {
        let mut page = SamplePage::preview();
        page.go_to(key);
        let mut harness = Harness::builder()
            .with_size(WINDOW)
            .with_context_setup(versekit_ui::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, page: &mut SamplePage| {
                    let _ = page.show(ui);
                },
                page,
            );
        for _ in 0..SETTLE_FRAMES {
            harness.step();
        }
        let name = if key.is_empty() { "(overview)" } else { key };
        for clipped in &harness.output().shapes {
            add_sizes(&clipped.shape, name, &mut found);
        }
    }
    set_thread_language(Choice::Auto);
    found
}

/// The texts of `found` that have none of the sizes of the interface and no reason of their own.
fn outside_the_steps(found: &Sizes) -> Vec<String> {
    found
        .iter()
        .filter(|((page, size), _)| !steps().contains(size) && !OWN_SIZES.iter().any(|(own, _)| own == page))
        .map(|((page, size), seen)| {
            format!("{page}: {:.1} Punkte, {} Zeilen, z. B. {:?}", *size as f32 / 10.0, seen.lines, seen.example)
        })
        .collect()
}

#[test]
fn every_text_of_every_page_has_one_of_the_sizes_of_the_interface() {
    let found = measure();
    let mut lines: BTreeMap<i32, usize> = BTreeMap::new();
    for ((_, size), seen) in &found {
        *lines.entry(*size).or_default() += seen.lines;
    }
    for (size, count) in &lines {
        println!("{:>5.1} Punkte: {count:>5} Zeilen", *size as f32 / 10.0);
    }
    let outside = outside_the_steps(&found);
    assert!(outside.is_empty(), "Text in einer Größe ohne Stufe:\n{}", outside.join("\n"));
    for (page, reason) in OWN_SIZES {
        assert!(found.keys().any(|(shown, _)| *shown == page), "{page} ({reason}) is not drawn any more");
    }
}

/// The scan sees a text of a size of its own: a label drawn at 12 points is none of the four.
#[test]
fn the_size_scan_finds_a_size_of_its_own() {
    let mut harness = Harness::builder().with_accessibility_check(false).allow_missing_glyphs().build_ui(|ui| {
        versekit_ui::install(ui.ctx());
        ui.label(egui::RichText::new("zwölf Punkte").size(12.0));
        ui.label("normal");
    });
    harness.step();
    let mut found = Sizes::new();
    for clipped in &harness.output().shapes {
        add_sizes(&clipped.shape, "x", &mut found);
    }
    let outside = outside_the_steps(&found);
    assert_eq!(outside.len(), 1, "{outside:?}");
    assert!(outside[0].contains("12.0"), "{outside:?}");
}
