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

//! The page "Refineries" (`pages._refineries` of 4.12): what every refinery makes of every material, bonus
//! and penalty, one row per material and one column per bundle of stations with the same profile in one
//! system. The table scrolls sideways in an area of its own; the page itself never does. The head of a
//! column names the stations of it in a tooltip.
//!
//! It reads the same mining data as the mining page and shares its state; unlike 4.12 it shows the data as
//! soon as they arrive and offers to fetch them where they are missing.

use egui::{Align, Layout, RichText, Ui, vec2};
use versekit_core::language::t;
use versekit_core::refining::{self, Column};
use versekit_core::theme::Scheme;

use crate::color;
use crate::mining_page::{self, MiningState};
use crate::widgets;

/// The width of a column of values, at least: the short names stand on two lines of up to six letters.
const VALUE_MIN: f32 = 46.0;
const GAP: f32 = 8.0;

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The width of the widest line of a text.
fn text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    text.lines().map(|line| crate::joysticks_page::text_width(ui, line, style.clone())).fold(0.0, f32::max)
}

/// A value of the table: the bonus with its sign, a dot for none.
fn value_text(value: i64) -> String {
    if value == 0 { "·".to_string() } else { format!("{value:+}") }
}

/// The short name of a column; a column of stations without a name shows a question mark.
pub(crate) fn short_name(column: &Column) -> String {
    let short = refining::short_name(&column.names);
    if short == "—" { "?".to_string() } else { short }
}

/// One row of the table: its stripe and its cells side by side.
fn stripe_row(ui: &mut Ui, scheme: &Scheme, k: usize, add: impl FnOnce(&mut Ui)) {
    let fill = if k % 2 == 1 { color(scheme.bar) } else { egui::Color32::TRANSPARENT };
    egui::Frame::new().fill(fill).show(ui, |stripe| {
        stripe.horizontal(add);
    });
}

/// A cell of exactly this size with its content at the bottom, at `side`: a taller text never makes its row
/// taller, so the materials and the values stay on the same rows. The response is the whole cell's.
fn cell(ui: &mut Ui, width: f32, height: f32, side: Align, text: RichText) -> egui::Response {
    let (rect, placed) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(Layout::bottom_up(side)));
    inner.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
    // Laid over the text: the pointer on the text belongs to the cell, so its tooltip shows there too.
    ui.interact(rect, placed.id.with("cell"), egui::Sense::hover())
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut MiningState) {
    state.follow(ui);
    state.poll_fetch();
    ui.label(sub(scheme, t("s_rf_lead")));
    ui.add_space(8.0);
    let (columns, rows) = refining::matrix(state.data());
    if columns.is_empty() || rows.is_empty() {
        mining_page::no_data(ui, scheme, state, ("factory", "s_rf_keine"));
        return;
    }
    let heads: Vec<String> = columns.iter().map(|c| refining::head(&short_name(c))).collect();
    let body = egui::TextStyle::Body;
    let line = ui.text_style_height(&body);
    let material_width = rows
        .iter()
        .map(|r| text_width(ui, &r.material, egui::TextStyle::Small))
        .chain([text_width(ui, &t("s_rf_material"), egui::TextStyle::Small)])
        .fold(0.0, f32::max)
        + GAP;
    let value_width = heads
        .iter()
        .map(|h| text_width(ui, h, body.clone()))
        .chain(rows.iter().flat_map(|r| r.values.iter().map(|v| text_width(ui, &value_text(*v), body.clone()))))
        .fold(VALUE_MIN, f32::max)
        + GAP;
    ui.horizontal_top(|table| {
        table.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // A row of cells is as high as its cells, not as high as a button.
        table.spacing_mut().interact_size.y = 0.0;
        // The materials stand still: who scrolls sideways keeps the row.
        table.vertical(|left| {
            cell(left, material_width, line, Align::Min, RichText::new(""));
            cell(left, material_width, line * 2.0, Align::Min, sub(scheme, t("s_rf_material")));
            for (k, item) in rows.iter().enumerate() {
                stripe_row(left, scheme, k, |row| {
                    cell(row, material_width, line, Align::Min, RichText::new(&item.material).small());
                });
            }
        });
        // A row of the page is one line high: the area of the values gets the height of the whole table.
        let height = line * (rows.len() as f32 + 3.0) + widgets::SCROLL_WIDTH;
        let room = vec2(table.available_width(), height);
        table.allocate_ui_with_layout(room, Layout::top_down(Align::Min), |values| {
            let area = egui::ScrollArea::horizontal().id_salt("raffinerie-tafel").auto_shrink([false, true]);
            values_table(values, scheme, area, (&columns, &rows, &heads), (value_width, line));
        });
    });
    ui.add_space(10.0);
    ui.label(sub(scheme, t("s_rf_quelle")));
    mining_page::stand(ui, scheme, state);
}

/// The values of the table in their area that scrolls sideways: the systems above their columns, the
/// short names, one row per material.
fn values_table(
    ui: &mut Ui,
    scheme: &Scheme,
    area: egui::ScrollArea,
    (columns, rows, heads): (&[Column], &[refining::Row], &[String]),
    (value_width, line): (f32, f32),
) {
    {
        widgets::scroll_area(ui, scheme, area, |right| {
            right.spacing_mut().item_spacing = vec2(0.0, 0.0);
            right.spacing_mut().interact_size.y = 0.0;
            // The systems above their columns: only runs of columns, never sorted anew.
            right.horizontal(|row| {
                for (system, count) in refining::groups(columns) {
                    let name = RichText::new(system).color(color(scheme.accent));
                    cell(row, value_width * count as f32, line, Align::Center, name);
                }
            });
            right.horizontal(|row| {
                // The head of a column names every station of it when the pointer rests on it.
                for (column, head) in columns.iter().zip(heads) {
                    cell(row, value_width, line * 2.0, Align::Max, RichText::new(head))
                        .on_hover_text(column.names.join("\n"));
                }
            });
            for (k, item) in rows.iter().enumerate() {
                stripe_row(right, scheme, k, |row| {
                    for (i, value) in item.values.iter().enumerate() {
                        // A 0 is no advice; the best value of a row stands out in bold.
                        let mut text = RichText::new(value_text(*value))
                            .color(color(mining_page::bonus_color(scheme, *value as f64)));
                        if item.best == Some(i) && *value > 0 {
                            text = widgets::bold(row, value_text(*value), egui::TextStyle::Body)
                                .color(color(mining_page::bonus_color(scheme, *value as f64)));
                        }
                        cell(row, value_width, line, Align::Max, text);
                    }
                });
            }
        });
    }
}
