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

//! Layout rules so that nothing is cut off, at any interface size:
//!
//! - no fixed widths: rows wrap (`horizontal_wrapped`), text breaks;
//! - sections stand side by side only while each gets [`columns_or_stack`]'s
//!   minimum width, otherwise under each other;
//! - every page scrolls vertically ([`page`]) and reports how far its content
//!   sticks out to the right; the edge check in the tests demands 0.

use egui::{ScrollArea, Ui};
use versekit_core::theme::Scheme;

use crate::widgets;

/// Gap between sections.
pub const GAP: f32 = 8.0;

/// How many columns of at least `min_width` fit in `width` (with [`GAP`] between them): one at least, `most` at most.
/// For cards or rows of one kind that stand side by side in a wide window instead of stretching over it.
pub fn columns_that_fit(width: f32, min_width: f32, most: usize) -> usize {
    (((width + GAP) / (min_width + GAP)).floor() as usize).clamp(1, most.max(1))
}

/// The width of a column of setting rows: the names, the gap and the column of controls, with a little room.
pub const ROW_CELL: f32 = crate::widgets::NAME_WIDTH + GAP + crate::settings::CONTROL_WIDTH + 2.0 * GAP;

/// `count` sections side by side when each gets at least `min_width`,
/// otherwise under each other.
pub fn columns_or_stack(ui: &mut Ui, min_width: f32, count: usize, add: impl FnMut(&mut Ui, usize)) {
    split(ui, min_width, count, true, add);
}

/// Like [`columns_or_stack`], but every card stays as tall as its content: for columns with a different number of
/// cards, where padding the last one would stretch a small card over an empty area.
pub fn columns_or_stack_loose(ui: &mut Ui, min_width: f32, count: usize, add: impl FnMut(&mut Ui, usize)) {
    split(ui, min_width, count, false, add);
}

fn split(ui: &mut Ui, min_width: f32, count: usize, equal: bool, mut add: impl FnMut(&mut Ui, usize)) {
    let gaps = count.saturating_sub(1) as f32 * GAP;
    let needed = count as f32 * min_width + gaps;
    if count > 1 && ui.available_width() >= needed {
        // Own split instead of `ui.columns`: its rounding let the row stick
        // out by about one point. Whole points, rounded down, always fit.
        let width = ((ui.available_width() - gaps) / count as f32).floor();
        // The cards of the columns are equally tall: what each column drew in the frame before (cards, height
        // of its content without the padding of its last card) says by how much each pads its last card.
        let key = ui.auto_id_with("columns-height");
        let before: Vec<(usize, f32)> = ui.data(|d| d.get_temp(key)).unwrap_or_default();
        let tallest = if equal { before.iter().map(|(_, height)| *height).fold(0.0, f32::max) } else { 0.0 };
        let mut now = Vec::with_capacity(count);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for i in 0..count {
                let (cards, own) = if equal { before.get(i).copied().unwrap_or((0, tallest)) } else { (0, 0.0) };
                ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(width);
                    widgets::row_cards::enter(cards, (tallest - own).max(0.0));
                    add(ui, i);
                    let (drawn, added) = widgets::row_cards::leave();
                    now.push((drawn, ui.min_rect().height() - added));
                });
            }
        });
        let same =
            now.len() == before.len() && now.iter().zip(&before).all(|(a, b)| a.0 == b.0 && (a.1 - b.1).abs() <= 0.5);
        if !same {
            ui.data_mut(|d| d.insert_temp(key, now));
            ui.ctx().request_repaint();
        }
    } else {
        for i in 0..count {
            if i > 0 {
                ui.add_space(GAP);
            }
            add(ui, i);
        }
    }
}

/// Share of the visible page a list below other content keeps at least, and
/// rows it shows at least, however much stands above it.
pub const LIST_SHARE: f32 = 0.6;
pub const LIST_MIN_ROWS: f32 = 6.0;

/// Height for a scrolling list from the cursor on: what is left on screen,
/// but at least [`LIST_SHARE`] of the visible page and [`LIST_MIN_ROWS`] rows
/// of `row_height`; the page scrolls for the rest.
pub fn list_room(ui: &Ui, row_height: f32) -> f32 {
    let left = ui.clip_rect().bottom() - ui.cursor().min.y - 12.0;
    room_for_list(left, ui.clip_rect().height(), row_height)
}

/// [`list_room`] from the room `left` on screen and the `visible` page height.
pub fn room_for_list(left: f32, visible: f32, row_height: f32) -> f32 {
    left.max(visible * LIST_SHARE).max(LIST_MIN_ROWS * row_height)
}

/// What a page returns.
pub struct PageOutput<R> {
    pub inner: R,
    /// How far the content sticks out to the right (0 = nothing cut off).
    pub overflow: f32,
}

/// Room kept free on the right of every page: at sizes other than 100 %
/// widgets are snapped to whole screen pixels, which can add a fraction of a
/// point, enough to clip a border line.
pub const SAFETY: f32 = 2.0;

/// A page that scrolls vertically and measures whether anything is wider than
/// the window.
pub fn page<R>(ui: &mut Ui, scheme: &Scheme, id: &str, add: impl FnOnce(&mut Ui) -> R) -> PageOutput<R> {
    let area = ScrollArea::vertical().id_salt(id).auto_shrink([false, false]);
    let shown = widgets::scroll_area(ui, scheme, area, |ui| {
        ui.set_max_width(ui.available_width() - SAFETY);
        add(ui)
    });
    let overflow = (shown.content_size.x - shown.inner_rect.width()).max(0.0);
    PageOutput { inner: shown.inner, overflow }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::theme;

    fn measure(width: f32, add: impl Fn(&mut Ui) + Copy) -> f32 {
        let ctx = egui::Context::default();
        let scheme = theme::by_id("original").expect("original exists");
        let mut overflow = 0.0;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| overflow = page(ui, scheme, "probe", add).overflow);
            output.textures_delta.clear();
        }
        overflow
    }

    #[test]
    fn overflow_is_measured() {
        // Counter-check: something wider than the window must be reported.
        let too_wide = measure(600.0, |ui| {
            ui.add_sized([900.0, 20.0], egui::Label::new("breit"));
        });
        assert!(too_wide > 250.0, "overflow {too_wide}");
        let wrapped = measure(600.0, |ui| {
            ui.horizontal_wrapped(|ui| {
                for i in 0..40 {
                    let _ = ui.button(format!("Knopf {i}"));
                }
            });
        });
        assert_eq!(wrapped, 0.0);
    }

    #[test]
    fn the_cards_of_columns_are_equally_tall_and_stay_so() {
        let ctx = egui::Context::default();
        let scheme = theme::by_id("original").expect("original exists");
        let mut heights = Vec::new();
        for _ in 0..12 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            widgets::drawn_cards::take();
            let mut output = ctx.run_ui(input, |ui| {
                columns_or_stack(ui, 300.0, 2, |ui, i| {
                    widgets::card(ui, scheme, |ui| {
                        for line in 0..(2 + 5 * i) {
                            ui.label(format!("Zeile {line}"));
                        }
                    });
                });
            });
            output.textures_delta.clear();
            heights = widgets::drawn_cards::take().iter().map(|card| card.height()).collect();
        }
        assert_eq!(heights.len(), 2);
        assert!((heights[0] - heights[1]).abs() < 0.5, "{heights:?}");
        // The shorter card did not just grow by the padding again every frame: a card of 7 lines is the measure.
        assert!(heights[0] < 250.0, "{heights:?}");
    }

    #[test]
    fn sections_stack_when_narrow() {
        let narrow = measure(500.0, |ui| {
            columns_or_stack(ui, 300.0, 2, |ui, _| {
                ui.add_sized([290.0, 20.0], egui::Label::new("Abschnitt"));
            });
        });
        assert_eq!(narrow, 0.0);
    }
}
