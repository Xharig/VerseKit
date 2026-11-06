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
/// A row grows with the window up to this many lines.
const ROW_GROW: f32 = 1.8;

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

/// A cell of exactly this size with its content at the bottom, `lift` above it, at `side`: a taller text never
/// makes its row taller, so the materials and the values stay on the same rows. The response is the whole cell's.
fn cell(ui: &mut Ui, (width, height): (f32, f32), lift: f32, side: Align, text: RichText) -> egui::Response {
    let (rect, placed) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let room = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - lift));
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(room).layout(Layout::bottom_up(side)));
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
    // The table stands in a list frame with the corner brackets; it is measured inside the frame.
    widgets::list_frame_marked(ui, scheme, |ui| table(ui, scheme, &columns, &rows));
    ui.add_space(10.0);
    ui.label(sub(scheme, t("s_rf_quelle")));
    mining_page::stand(ui, scheme, state);
}

/// The table of yields: the materials on the left, standing still, the values beside them, scrolling sideways.
fn table(ui: &mut Ui, scheme: &Scheme, columns: &[Column], rows: &[refining::Row]) {
    let heads: Vec<String> = columns.iter().map(|c| refining::head(&short_name(c))).collect();
    let body = egui::TextStyle::Body;
    let line = ui.text_style_height(&body);
    let material_width = rows
        .iter()
        .map(|r| text_width(ui, &r.material, egui::TextStyle::Body))
        .chain([text_width(ui, &t("s_rf_material"), egui::TextStyle::Small)])
        .fold(0.0, f32::max)
        + GAP;
    let value_width = heads
        .iter()
        .map(|h| text_width(ui, h, body.clone()))
        .chain(rows.iter().flat_map(|r| r.values.iter().map(|v| text_width(ui, &value_text(*v), body.clone()))))
        .fold(VALUE_MIN, f32::max)
        + GAP;
    let (value_width, row) = table_size(ui, (material_width, value_width), line, (columns.len(), rows.len()));
    // A row taller than a line keeps its text in the middle.
    let lift = ((row - line) / 2.0).floor();
    ui.horizontal_top(|table| {
        table.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // A row of cells is as high as its cells, not as high as a button.
        table.spacing_mut().interact_size.y = 0.0;
        // The materials stand still: who scrolls sideways keeps the row.
        table.vertical(|left| {
            cell(left, (material_width, line), 0.0, Align::Min, RichText::new(""));
            cell(left, (material_width, line * 2.0), 0.0, Align::Min, sub(scheme, t("s_rf_material")));
            for (k, item) in rows.iter().enumerate() {
                stripe_row(left, scheme, k, |cells| {
                    // As large as the values beside it: the names are read from a distance too.
                    let name = RichText::new(&item.material);
                    cell(cells, (material_width, row), lift, Align::Min, name);
                });
            }
        });
        // The area of the values gets the height of the whole table.
        let height = line * 3.0 + row * rows.len() as f32 + widgets::SCROLL_WIDTH;
        let room = vec2(table.available_width(), height);
        table.allocate_ui_with_layout(room, Layout::top_down(Align::Min), |values| {
            let area = egui::ScrollArea::horizontal().id_salt("raffinerie-tafel").auto_shrink([false, true]);
            values_table(values, scheme, area, (columns, rows, &heads), (value_width, line, row, lift));
        });
    });
}

/// The width of a column of values and the height of a row, grown with the window: the columns share the width
/// beside the materials (never narrower than their text, `value_width`), the rows share the height down to the
/// bottom of the window less the heads, the bar and the two lines below the table (one line at least,
/// [`ROW_GROW`] at most).
fn table_size(
    ui: &Ui,
    (material_width, value_width): (f32, f32),
    line: f32,
    (columns, rows): (usize, usize),
) -> (f32, f32) {
    let beside = ui.available_width() - material_width - widgets::SCROLL_WIDTH - crate::layout::SAFETY;
    let width = value_width.max((beside / columns.max(1) as f32).floor());
    let below = 10.0 + 2.0 * ui.text_style_height(&egui::TextStyle::Small) + crate::layout::SAFETY;
    let left = ui.clip_rect().bottom() - ui.cursor().min.y - line * 3.0 - widgets::SCROLL_WIDTH - below;
    let height = (left / rows.max(1) as f32).clamp(line, line * ROW_GROW).floor();    (width, height.max(line))
}

/// The values of the table in their area that scrolls sideways: the systems above their columns, the
/// short names, one row per material.
fn values_table(
    ui: &mut Ui,
    scheme: &Scheme,
    area: egui::ScrollArea,
    (columns, rows, heads): (&[Column], &[refining::Row], &[String]),
    (value_width, line, row_height, lift): (f32, f32, f32, f32),
) {
    {
        widgets::scroll_area(ui, scheme, area, |right| {
            right.spacing_mut().item_spacing = vec2(0.0, 0.0);
            right.spacing_mut().interact_size.y = 0.0;
            // The systems above their columns: only runs of columns, never sorted anew.
            right.horizontal(|row| {
                for (system, count) in refining::groups(columns) {
                    let name = RichText::new(system).color(color(scheme.accent));
                    cell(row, (value_width * count as f32, line), 0.0, Align::Center, name);
                }
            });
            right.horizontal(|row| {
                // The head of a column names every station of it when the pointer rests on it.
                for (column, head) in columns.iter().zip(heads) {
                    cell(row, (value_width, line * 2.0), 0.0, Align::Max, RichText::new(head))
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
                        cell(row, (value_width, row_height), lift, Align::Max, text);
                    }
                });
            }
        });
    }
}
