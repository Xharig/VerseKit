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

//! The column of controls of the settings rows, without a screen: on every page of the program the controls of a
//! row begin at one left edge, that edge is the same in every row of a card, the column is one width in the whole
//! program, a drop-down fills it, and nothing a row draws reaches beyond it.

use egui::Rect;
use versekit_core::language::{self, Choice};
use versekit_core::{font_size, theme};

use super::docking::tests::draw;
use super::real_data::real_page;
use super::{NAV, SamplePage};
use crate::settings::{self, CONTROL_WIDTH, column_log};

/// Two edges are the same when they differ by less than this (points).
const SAME: f32 = 0.6;
/// The most a line of a hint is high: one line of small text with its symbol (as high as a control line); two lines
/// of small text are well above it.
const ONE_HINT_LINE: f32 = crate::SMALL_SIZE * 2.0;

/// The pages of the window with settings rows: every page of the navigation and the ones only a jump leads to.
fn pages() -> Vec<&'static str> {
    let mut pages = vec![""];
    pages.extend(NAV.iter().flat_map(|(_, pages)| pages.iter().map(|(page, _)| *page)));
    pages.extend(["hf_wasistneu", "hf_diagnose"]);
    pages
}

/// What is wrong with the rows and drop-downs `rows` and `choices` drew: one text each.
fn faults(rows: &[column_log::Row], choices: &[Rect]) -> Vec<String> {
    let mut found = Vec::new();
    for row in rows {
        let wanted = CONTROL_WIDTH.min((row.row.width() - crate::layout::GAP - 80.0).max(0.0));
        if (row.column.width() - wanted).abs() > SAME {
            found.push(format!("column {:.1} wide instead of {wanted:.1}", row.column.width()));
        }
        // The column stands right after the names, at the same place in every card of that width.
        let left = row.row.min.x + crate::widgets::name_column(row.row.width(), wanted) + crate::layout::GAP;
        if (row.column.min.x - left).abs() > SAME {
            found.push(format!(
                "column at {:.1} instead of {left:.1}, right after the names (the row ends at {:.1})",
                row.column.min.x, row.row.max.x
            ));
        }
        // The hint is one line at most: what is longer stands on hover.
        if row.hint.is_positive() && row.hint.height() > ONE_HINT_LINE {
            found.push(format!("a hint {:.0} high, more than one line", row.hint.height()));
        }
        if (row.used.min.x - row.column.min.x).abs() > SAME {
            found.push(format!(
                "controls begin at {:.1}, the column at {:.1} (the row ends at {:.1})",
                row.used.min.x, row.column.min.x, row.row.max.x
            ));
        }
        if row.used.max.x > row.column.max.x + SAME {
            found.push(format!("controls reach {:.1} beyond the column", row.used.max.x - row.column.max.x));
        }
        // A row is a few lines at most; a column that was given all the height of the page would show here.
        if row.row.height() > 300.0 {
            found.push(format!("a row {:.0} high", row.row.height()));
        }
        if row.text.max.x > row.column.min.x - 1.0 {
            found.push(format!("the name reaches {:.1} into the column", row.text.max.x - row.column.min.x));
        }
    }
    // One left edge in a card: rows that begin and end at the same place begin their column at the same place.
    for (i, a) in rows.iter().enumerate() {
        for b in &rows[i + 1..] {
            let same_card = (a.row.max.x - b.row.max.x).abs() < SAME && (a.row.min.x - b.row.min.x).abs() < SAME;
            let wider = (a.column.min.x - b.column.min.x).abs() > SAME;
            if same_card && wider && (a.column.width() - b.column.width()).abs() < SAME {
                found.push(format!("two rows of a card: columns at {:.1} and {:.1}", a.column.min.x, b.column.min.x));
            }
        }
    }
    // A drop-down fills the column it stands in.
    for field in choices {
        let column = rows.iter().find(|row| row.column.contains(field.left_center()));
        match column {
            Some(row) if (field.width() - row.column.width()).abs() > SAME => {
                found.push(format!("drop-down {:.1} wide in a column of {:.1}", field.width(), row.column.width()));
            }
            Some(_) => {}
            None => found.push(format!("a drop-down outside every column at {:.1}", field.min.x)),
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Draws `key` twice (the first draw lets the layout settle) and gives what the second drew.
fn rows_of(page: &mut SamplePage, key: &str, width: f32, height: f32) -> (f32, Vec<column_log::Row>, Vec<Rect>) {
    page.go_to(key);
    draw(page, width, height);
    column_log::take();
    let (overflow, _) = draw(page, width, height);
    let (rows, choices) = column_log::take();
    (overflow, rows, choices)
}

#[test]
fn the_controls_of_every_settings_row_stand_in_one_column_on_every_page() {
    let _language = crate::language_lock();
    let mut findings = Vec::new();
    let mut rows_seen = 0;
    let mut choices_seen = 0;
    for real in [false, true] {
        let (mut page, dir) = if real {
            real_page("bedienspalte", "", &[])
        } else {
            theme::set_current("original");
            (SamplePage::preview(), std::env::temp_dir())
        };
        for scheme in ["original", "krt"] {
            theme::set_thread_current(scheme);
            for points in [font_size::POINTS_RANGE.0, 3, font_size::POINTS_RANGE.1] {
                page.size = points;
                // A tall window: a page that is taller than the window gets a bar at its side and is narrower.
                let (width, _) = font_size::min_window(points);
                let height = 4000.0;
                for english in [false, true] {
                    language::set_thread_language(if english { Choice::En } else { Choice::De });
                    for key in pages() {
                        let (overflow, rows, choices) = rows_of(&mut page, key, width, height);
                        rows_seen += rows.len();
                        choices_seen += choices.len();
                        let which = format!("{key} real {real} {scheme} size {points} english {english}");
                        // The joysticks page draws no settings row; its width is the business of its own checks.
                        if overflow > 0.5 && key != "hf_joysticks" {
                            findings.push(format!("{which}: {overflow:.0} too wide"));
                        }
                        findings.extend(faults(&rows, &choices).into_iter().map(|f| format!("{which}: {f}")));
                    }
                }
            }
        }
        if real {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    theme::set_current("original");
    language::set_thread_language(Choice::De);
    // The check really looked at rows and at drop-downs.
    assert!(rows_seen > 100 && choices_seen > 20, "{rows_seen} rows, {choices_seen} drop-downs");
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(15)]);
}

/// The row of a switch, one of a drop-down with a text far too long for it, one of a slider, one of a button: made
/// by hand, so that the long text and every kind of control are certainly there.
#[test]
fn a_long_choice_is_cut_inside_the_column_and_every_control_begins_at_its_edge() {
    let _language = crate::language_lock();
    theme::set_thread_current("original");
    language::set_thread_language(Choice::De);
    let scheme = theme::current();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let long = "Convolutional Neural Network, und noch ein langer Zusatz dahinter";
    let mut on = true;
    let mut value = 40;
    column_log::take();
    for _ in 0..3 {
        column_log::take();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 500.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default_margins().show(ui, |ui| {
                settings::setting_row(ui, scheme, "Schalter", "", false, |row| {
                    row.add(crate::widgets::Toggle::new(&mut on, scheme));
                });
                settings::setting_row(ui, scheme, "Modell", "Ein Hinweis", false, |row| {
                    settings::choice_box(row, "modell", "Modell", long, |list| {
                        let _ = list.selectable_label(true, long);
                    });
                });
                settings::setting_row(ui, scheme, "Regler", "", false, |row| {
                    settings::slider_with_value(row, scheme, "Regler", &mut value, (0, 100), false, |n| {
                        format!("{n} %")
                    });
                });
                settings::setting_row(ui, scheme, "Knopf", "", false, |row| {
                    row.add(crate::widgets::Button::new("Knopf", crate::widgets::ButtonKind::Normal, scheme));
                });
            });
        });
        output.textures_delta.clear();
    }
    let (rows, choices) = column_log::take();
    assert_eq!(rows.len(), 4);
    assert_eq!(choices.len(), 1);
    assert!(faults(&rows, &choices).is_empty(), "{:?}", faults(&rows, &choices));
    let edge = rows[0].column.min.x;
    for row in &rows {
        assert!((row.used.min.x - edge).abs() < SAME, "all controls begin at {edge}: {:?}", row.used);
    }
    let field = choices[0];
    assert!((field.width() - CONTROL_WIDTH).abs() < SAME, "the drop-down fills the column: {field:?}");
    // One line each: a text that was not cut but wrapped would make the field and the row taller.
    assert!(field.height() < 30.0, "the long text is cut, not wrapped: {field:?}");
    for row in &rows {
        assert!(row.used.height() < 40.0, "every control stands in one line: {:?}", row.used);
    }
    assert!(field.min.x >= edge - SAME, "the long text does not grow to the left over the edge");
}

#[test]
fn the_slider_leaves_a_place_for_its_number_that_does_not_move() {
    let _language = crate::language_lock();
    let scheme = theme::current();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut seen = Vec::new();
    for mut value in [9, 99, 100] {
        column_log::take();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 300.0))),
            ..Default::default()
        };
        let mut slider = None;
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default_margins().show(ui, |ui| {
                settings::setting_row(ui, scheme, "Regler", "", false, |row| {
                    slider =
                        Some(settings::slider_with_value(row, scheme, "Regler", &mut value, (0, 100), false, |n| {
                            format!("{n} %")
                        }));
                });
            });
        });
        output.textures_delta.clear();
        let (rows, _) = column_log::take();
        seen.push((slider.expect("drawn").rect.max.x, rows[0]));
    }
    // The slider stays, and the number takes its place of fixed width at the end of the column, in one line.
    assert!(seen.windows(2).all(|w| (w[0].0 - w[1].0).abs() < SAME), "the slider stays: {seen:?}");
    for (slider_end, row) in &seen {
        assert!((row.used.max.x - row.column.max.x).abs() < SAME, "the number ends at the column: {row:?}");
        assert!(*slider_end <= row.column.max.x - settings::VALUE_WIDTH + SAME, "the number has its place: {row:?}");
        assert!(row.used.height() < 40.0, "slider and number stand in one line: {:?}", row.used);
    }
}

/// The check finds what it is for: rows made wrong by hand are reported.
#[test]
fn the_check_finds_a_column_that_is_not_one() {
    let rect = |x0: f32, x1: f32| Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x1, 30.0));
    let hint = Rect::from_min_max(egui::pos2(0.0, 30.0), egui::pos2(300.0, 44.0));
    let row = |column: Rect, used: Rect, text: Rect| column_log::Row { row: rect(0.0, 700.0), column, used, text, hint };
    // A row 700 wide: names 360, the gap, the column of 232 right after them.
    let good = row(rect(368.0, 600.0), rect(368.0, 400.0), rect(0.0, 300.0));
    assert!(faults(&[good], &[]).is_empty(), "{:?}", faults(&[good], &[]));
    // Controls at the right end of the column.
    assert_eq!(faults(&[row(rect(368.0, 600.0), rect(540.0, 600.0), rect(0.0, 300.0))], &[]).len(), 1);
    // A column of another width.
    assert!(!faults(&[row(rect(368.0, 650.0), rect(368.0, 400.0), rect(0.0, 300.0))], &[]).is_empty());
    // A column at the far edge of the row, as before: not right after the names.
    assert_eq!(faults(&[row(rect(468.0, 700.0), rect(468.0, 500.0), rect(0.0, 300.0))], &[]).len(), 1);
    // Controls beyond the column, a name into the column.
    assert_eq!(faults(&[row(rect(368.0, 600.0), rect(368.0, 640.0), rect(0.0, 300.0))], &[]).len(), 1);
    assert_eq!(faults(&[row(rect(368.0, 600.0), rect(368.0, 400.0), rect(0.0, 420.0))], &[]).len(), 1);
    // A hint in two lines.
    let tall = column_log::Row { hint: Rect::from_min_max(egui::pos2(0.0, 30.0), egui::pos2(300.0, 60.0)), ..good };
    assert_eq!(faults(&[tall], &[]).len(), 1, "{:?}", faults(&[tall], &[]));
    // A drop-down narrower than its column, and two rows of one card with different edges.
    assert_eq!(faults(&[good], &[rect(368.0, 500.0)]).len(), 1);
    let other = row(rect(340.0, 572.0), rect(340.0, 360.0), rect(0.0, 300.0));
    assert!(!faults(&[good, other], &[]).is_empty());
}

/// The text of the graphics page that was too long for its field fits whole in the column, in both languages.
#[test]
fn the_longest_choice_of_the_graphics_page_fits_the_column_whole() {
    let _language = crate::language_lock();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    for choice in [Choice::De, Choice::En] {
        language::set_thread_language(choice);
        let text = language::t("s_gr_modell_0");
        let mut width = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0))),
                ..Default::default()
            },
            |ui| {
                let font = egui::TextStyle::Button.resolve(ui.style());
                width = ui.fonts_mut(|f| f.layout_no_wrap(text.clone(), font, egui::Color32::WHITE).size().x);
            },
        );
        output.textures_delta.clear();
        // The arrow, the padding and the gap take about this much of the column.
        assert!(width <= CONTROL_WIDTH - 36.0, "{text} is {width} wide in a column of {CONTROL_WIDTH}");
    }
    language::set_thread_language(Choice::De);
}

/// The two languages stand in the order of the setup assistant: German first.
#[test]
fn the_languages_stand_german_first() {
    let _language = crate::language_lock();
    let mut page = SamplePage::preview();
    page.go_to("hf_allgemein");
    let place = |texts: &[(String, egui::Pos2)], text: &str| {
        texts.iter().find(|(t, _)| t == text).map(|(_, at)| *at).unwrap_or_else(|| panic!("{text} is not drawn"))
    };
    let texts = draw(&mut page, 1500.0, 4000.0).1;
    let (german, english) = (place(&texts, "Deutsch"), place(&texts, "English"));
    assert!((german.y - english.y).abs() < 3.0 && german.x < english.x, "{german:?} {english:?}");
    // The language of the system is the third choice, on a line of its own below the two.
    let system = place(&texts, &versekit_core::language::t("e_sprache_auto"));
    assert!(system.y > german.y + 3.0, "{system:?} below {german:?}");
}
