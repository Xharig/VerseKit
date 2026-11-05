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

//! Building blocks of settings pages and the setup assistant: the setting
//! row with its hint below, the list of steps that one can jump through, the
//! bar for a new version and the block that places the overlay on the screen.

use egui::{
    Align, CornerRadius, CursorIcon, InnerResponse, Layout, Rect, RichText, Role, Sense, Stroke, StrokeKind, TextStyle,
    Ui, Vec2, WidgetInfo, pos2, vec2,
};
use versekit_core::theme::{IconTint, Scheme};

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

// ---------------------------------------------------------------------------
// Setting row
// ---------------------------------------------------------------------------

/// The width of the column of controls, the same in every settings row of the program: switches, drop-downs,
/// sliders, fields and buttons all begin at its left edge, and every drop-down fills it. A text too long for a
/// drop-down is cut with „…“ and stands whole on hover.
pub const CONTROL_WIDTH: f32 = 232.0;
/// The place of the number at a slider, at the right end of the column. Of one width, so that the slider does not
/// slip away under the mouse when the number goes from 99 to 100.
pub const VALUE_WIDTH: f32 = 52.0;

/// One setting: its name at the left, the control in the column of controls at the right, a short
/// hint below in small muted text. After a jump from the menu filter to its
/// name or its hint the row frames itself in the accent (see
/// [`crate::highlight`]); `lit` says that the jump leads to this row under
/// another name (the row shows a text other than the one the filter found).
///
/// The column is [`CONTROL_WIDTH`] wide and its left edge is the same in every row of a card; the controls are
/// laid out from there to the right (what does not fit goes to a second line) and stand at that edge, whatever
/// their own width. The hint sits right below the name and the control in the middle of the name line, whatever
/// its height: hints of rows side by side then stand on one line, with a switch, a button or radio buttons.
pub fn setting_row<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    lit: bool,
    control: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let top = ui.cursor().min;
    let width = ui.available_width();
    let line_height = widgets::TOGGLE_SIZE.y;
    // A row too narrow for the column and a name keeps a name of 80 points; the column takes the rest.
    let column_width = CONTROL_WIDTH.min((width - GAP - 80.0).max(0.0));
    let column = Rect::from_min_size(pos2(top.x + width - column_width, top.y), vec2(column_width, line_height));
    let mut controls = ui.new_child(
        egui::UiBuilder::new().max_rect(column).layout(Layout::left_to_right(Align::Center).with_main_wrap(true)),
    );
    let inner = control(&mut controls);
    let used = controls.min_rect();
    // The name and the hint take what the column leaves, and wrap there.
    let text_right = if used.width() > 0.0 { column.left() - GAP } else { top.x + width };
    let text_area = Rect::from_min_max(top, pos2(text_right.max(top.x + 80.0), top.y + f32::MAX / 4.0));
    let mut text = ui.new_child(egui::UiBuilder::new().max_rect(text_area).layout(Layout::top_down(Align::Min)));
    text.allocate_ui_with_layout(vec2(text_area.width(), line_height), Layout::left_to_right(Align::Center), |ui| {
        ui.add(egui::Label::new(name).wrap());
    });
    if !hint.is_empty() {
        text.add(egui::Label::new(RichText::new(hint).small().color(color(scheme.sub))).wrap());
    }
    let bottom = text.min_rect().bottom().max(used.bottom());
    let rect = Rect::from_min_max(top, pos2(top.x + width, bottom));
    #[cfg(test)]
    {
        if used.width() > 0.0 {
            column_log::row(rect, Rect::from_min_max(column.min, pos2(column.max.x, bottom)), used, text.min_rect());
        }
        column_log::named(name);
    }
    let response = ui.allocate_rect(rect, Sense::hover());
    if !crate::highlight::mark(ui, scheme, rect, &[name, hint]) && lit {
        crate::highlight::frame(ui, scheme, rect);
    }
    ui.add_space(6.0);
    InnerResponse::new(inner, response)
}

/// What the rows and the drop-downs drew, for the checks: where each row, its column and its controls stood.
#[cfg(test)]
pub(crate) mod column_log {
    use std::cell::RefCell;

    use egui::Rect;

    /// One row with a control: the whole row, its column of controls, and what the controls used of it.
    #[derive(Clone, Copy, Debug)]
    pub struct Row {
        pub row: Rect,
        pub column: Rect,
        pub used: Rect,
        /// What the name and the hint took.
        pub text: Rect,
    }

    thread_local! {
        static ROWS: RefCell<Vec<Row>> = const { RefCell::new(Vec::new()) };
        static CHOICES: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
        static NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// The name of every setting row drawn, with a control or without.
    pub fn named(name: &str) {
        NAMES.with(|names| names.borrow_mut().push(name.to_string()));
    }

    /// The names of the rows drawn since the last call.
    pub fn take_names() -> Vec<String> {
        NAMES.with(|names| std::mem::take(&mut *names.borrow_mut()))
    }

    pub fn row(row: Rect, column: Rect, used: Rect, text: Rect) {
        ROWS.with(|rows| rows.borrow_mut().push(Row { row, column, used, text }));
    }

    pub fn choice(field: Rect) {
        CHOICES.with(|choices| choices.borrow_mut().push(field));
    }

    /// Everything logged since the last call: the rows and the fields of the drop-downs.
    pub fn take() -> (Vec<Row>, Vec<Rect>) {
        (
            ROWS.with(|rows| std::mem::take(&mut *rows.borrow_mut())),
            CHOICES.with(|c| std::mem::take(&mut *c.borrow_mut())),
        )
    }
}

/// A drop-down for the column of controls: as wide as the column, a text too long cut with „…“ and whole on
/// hover. `add` draws the entries. Gives the response of the field, named for screen readers by `name`.
pub fn choice_box(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    selected: &str,
    add: impl FnOnce(&mut Ui),
) -> egui::Response {
    let room = ui.available_width();
    let font = TextStyle::Button.resolve(ui.style());
    let wide = ui.fonts_mut(|fonts| fonts.layout_no_wrap(selected.to_string(), font, egui::Color32::WHITE).size().x);
    let combo = egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected)
        .width(room)
        .truncate()
        .show_ui(ui, |list| add(list));
    widgets::name_combo(&combo.response, name, selected);
    #[cfg(test)]
    column_log::choice(combo.response.rect);
    // The arrow and the padding of the field take about this much of the width.
    if wide > room - 30.0 { combo.response.on_hover_text(selected) } else { combo.response }
}

/// A slider for the column of controls with its number at the right in a place of fixed width ([`VALUE_WIDTH`]);
/// the slider takes the rest of the column. `on_release`: the value changes when the pointer is let go (the number
/// follows the pointer meanwhile). `text` writes the number.
pub fn slider_with_value(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    value: &mut i64,
    range: (i64, i64),
    on_release: bool,
    text: impl FnOnce(i64) -> String,
) -> egui::Response {
    let room = (ui.available_width().min(CONTROL_WIDTH) - VALUE_WIDTH - ui.spacing().item_spacing.x).max(40.0);
    let slider = widgets::Slider::new(value, range.0, range.1, scheme).width(room);
    let slider = if on_release { slider.on_release() } else { slider };
    let response = ui.add(slider);
    let shown = if on_release { widgets::Slider::shown(ui, &response).unwrap_or(*value) } else { *value };
    // The name for screen readers: the row holds it only as a text.
    let (enabled, now) = (response.enabled(), shown as f64);
    response.widget_info(|| egui::WidgetInfo::slider(enabled, now, name));
    let text = text(shown);
    ui.allocate_ui_with_layout(
        vec2(VALUE_WIDTH, widgets::TOGGLE_SIZE.y),
        Layout::right_to_left(Align::Center),
        |cell| {
            cell.label(text);
        },
    );
    response
}

/// A line a page gives the window for the foot: its text, and whether it tells of a fault (the window
/// shows a fault with the warning symbol instead of the pin).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub text: String,
    pub fault: bool,
}

impl Said {
    pub fn note(text: impl Into<String>) -> Said {
        Said { text: text.into(), fault: false }
    }

    pub fn fault(text: impl Into<String>) -> Said {
        Said { text: text.into(), fault: true }
    }
}

/// The small name of a group of settings above its rows, in the accent of the scheme.
pub fn setting_group(ui: &mut Ui, scheme: &Scheme, title: &str) {
    let tone = color(widgets::section_title_color(scheme));
    let shown = ui.label(RichText::new(title.to_uppercase()).small().color(tone));
    crate::highlight::mark(ui, scheme, shown.rect, &[title]);
    ui.add_space(2.0);
}

/// Narrowest a card of settings gets before the cards stack.
pub const SETTINGS_COLUMN_MIN: f32 = 420.0;

/// A settings page: each group a card of its own, two cards side by side and
/// as tall as each other; if the window is too narrow for two, the cards
/// stack, each as tall as its content.
/// `groups` are (title, number of rows); `row(ui, group, row)` draws one row,
/// usually with [`setting_row`]. Returns the cards drawn, in order.
pub fn settings_page(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    groups: &[(&str, usize)],
    row: &mut dyn FnMut(&mut Ui, usize, usize),
) -> Vec<Rect> {
    settings_page_wide(ui, scheme, id_salt, groups, &[], row)
}

/// [`settings_page`] where the groups in `wide` (by number) get a line of their own and the whole width.
pub fn settings_page_wide(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    groups: &[(&str, usize)],
    wide: &[usize],
    row: &mut dyn FnMut(&mut Ui, usize, usize),
) -> Vec<Rect> {
    let mut cards = Vec::new();
    let side_by_side = ui.available_width() >= 2.0 * SETTINGS_COLUMN_MIN + GAP;
    // The groups by line: two side by side, a wide one alone.
    let mut lines: Vec<Vec<(usize, &(&str, usize))>> = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        match lines.last_mut() {
            Some(line) if line.len() < 2 && !wide.contains(&g) && !wide.contains(&line[0].0) => line.push((g, group)),
            _ => lines.push(vec![(g, group)]),
        }
    }
    for (p, pair) in lines.iter().enumerate() {
        if p > 0 {
            ui.add_space(GAP);
        }
        let alone = pair.len() == 1 && wide.contains(&pair[0].0);
        // Side by side both cards get the height of the taller one from the
        // frame before.
        let id = ui.make_persistent_id((id_salt, "pair", p));
        let tallest: f32 = if side_by_side { ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0) } else { 0.0 };
        let mut measured: f32 = 0.0;
        let mut card = |ui: &mut Ui, g: usize, (title, rows): &(&str, usize)| {
            let shown = widgets::card(ui, scheme, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(tallest);
                let top = ui.cursor().min.y;
                setting_group(ui, scheme, title);
                (0..*rows).for_each(|r| row(ui, g, r));
                measured = measured.max(ui.cursor().min.y - top);
            });
            cards.push(shown.response.rect);
        };
        if side_by_side && !alone {
            // Always two places, so a single last card keeps half the width.
            crate::layout::columns_or_stack(ui, SETTINGS_COLUMN_MIN, 2, |ui, i| {
                if let Some((g, group)) = pair.get(i) {
                    card(ui, *g, group);
                }
            });
        } else {
            for (i, (g, group)) in pair.iter().enumerate() {
                if i > 0 {
                    ui.add_space(GAP);
                }
                card(ui, *g, group);
            }
        }
        if side_by_side && (measured - tallest).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(id, measured));
            ui.ctx().request_repaint();
        }
    }
    cards
}

// ---------------------------------------------------------------------------
// Steps of the setup assistant
// ---------------------------------------------------------------------------

/// Height of one step in the list.
pub const STEP_HEIGHT: f32 = 30.0;

/// Progress of a setup assistant: the step shown and the steps the player
/// confirmed. A check mark comes only from confirming, never from choosing
/// or moving on, so a step passed by does not look finished. The last step
/// finishes only once every step before it is confirmed.
pub struct WizardState {
    pub step: usize,
    pub done: Vec<bool>,
}

impl WizardState {
    pub fn new(steps: usize) -> Self {
        WizardState { step: 0, done: vec![false; steps] }
    }

    /// Confirms the shown step and moves on to the next step still open
    /// (the last step when none is). On the last step it finishes, if nothing
    /// before it is open.
    pub fn confirm(&mut self) {
        let last = self.done.len() - 1;
        if self.step == last {
            if self.open().is_empty() {
                self.done[last] = true;
            }
            return;
        }
        self.done[self.step] = true;
        self.step = (self.step + 1..last).chain(0..self.step).find(|&k| !self.done[k]).unwrap_or(last);
    }

    /// Steps before the last one that are not yet confirmed.
    pub fn open(&self) -> Vec<usize> {
        (0..self.done.len() - 1).filter(|&k| !self.done[k]).collect()
    }

    pub fn finished(&self) -> bool {
        self.done.last().copied().unwrap_or(false)
    }
}

/// The steps of the setup assistant as a list at the left: every step
/// can be clicked to jump there, a check mark marks each confirmed step, the
/// current one stands out like a chosen row.
/// Returns the step clicked.
pub fn step_list(ui: &mut Ui, scheme: &Scheme, steps: &[&str], current: usize, done: &[bool]) -> Option<usize> {
    let mut clicked = None;
    for (k, name) in steps.iter().enumerate() {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), STEP_HEIGHT), Sense::click());
        // Screen readers get the name and whether the step is confirmed.
        let (enabled, confirmed) = (ui.is_enabled(), done.get(k).copied().unwrap_or(false));
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, confirmed, *name));
        if ui.is_rect_visible(rect) {
            if k == current {
                ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.accent_dark));
                let bar = Rect::from_min_size(rect.min, vec2(widgets::LIST_BAR_WIDTH, rect.height()));
                ui.painter().rect_filled(bar, CornerRadius::ZERO, color(scheme.accent));
            } else if response.hovered() {
                ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
            }
            let finished = done.get(k).copied().unwrap_or(false);
            let (icon, tint) =
                if finished { ("circle-check", IconTint::Highlight) } else { ("circle", IconTint::Normal) };
            let at = Rect::from_min_size(pos2(rect.min.x + 10.0, rect.center().y - 8.0), Vec2::splat(16.0));
            if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
                image.paint_at(ui, at);
            }
            let text_color = if k == current || finished { scheme.fg } else { scheme.sub };
            let font = if k == current {
                crate::fonts::bold(TextStyle::Body.resolve(ui.style()).size)
            } else {
                TextStyle::Body.resolve(ui.style())
            };
            let galley = ui.painter().layout_no_wrap((*name).to_string(), font, color(text_color));
            ui.painter().galley(
                pos2(at.right() + 8.0, rect.center().y - galley.size().y / 2.0),
                galley,
                color(text_color),
            );
        }
        widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            clicked = Some(k);
        }
    }
    clicked
}

// ---------------------------------------------------------------------------
// New version
// ---------------------------------------------------------------------------

/// Texts of the bar for a new version, from the language file.
pub struct UpdateTexts<'a> {
    /// The message that a new version is there.
    pub message: &'a str,
    pub update: &'a str,
    pub news: &'a str,
    /// Why the strong button cannot be used yet (its hover); `None`: it can.
    pub update_blocked: Option<&'a str>,
}

/// What the player chose in the bar for a new version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateChoice {
    Update,
    News,
}

/// The bar across the top of the window when a new version is there.
/// While Star Citizen is not running, Verse-Kit updates itself silently;
/// the bar is for the time the game runs. It has no close button: it
/// stays until the update is loaded.
pub fn update_bar(ui: &mut Ui, scheme: &Scheme, texts: &UpdateTexts) -> Option<UpdateChoice> {
    notice_bar(ui, scheme, "download", texts)
}

/// A setting changed that only works after a restart: the same bar as a new
/// version, at the top of the page, with the restart button (Update) and the
/// later button (News).
pub fn restart_bar(ui: &mut Ui, scheme: &Scheme, texts: &UpdateTexts) -> Option<UpdateChoice> {
    notice_bar(ui, scheme, "rotate-cw", texts)
}

/// The bar of [`update_bar`] and [`restart_bar`]: icon, message, a strong
/// and a plain button.
fn notice_bar(ui: &mut Ui, scheme: &Scheme, icon: &str, texts: &UpdateTexts) -> Option<UpdateChoice> {
    let mut choice = None;
    egui::Frame::new()
        .fill(color(scheme.accent_dark))
        .stroke(Stroke::new(1.0, color(scheme.accent)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if let Some(image) = crate::icons::themed(icon, scheme, IconTint::Highlight, 16.0) {
                    ui.add(image);
                }
                ui.label(texts.message);
                let strong =
                    widgets::Button::new(texts.update, ButtonKind::Strong, scheme).blocked(texts.update_blocked);
                if ui.add(strong).clicked() {
                    choice = Some(UpdateChoice::Update);
                }
                if ui.add(widgets::Button::new(texts.news, ButtonKind::Normal, scheme)).clicked() {
                    choice = Some(UpdateChoice::News);
                }
            });
        });
    choice
}

// ---------------------------------------------------------------------------
// Where the overlay sits
// ---------------------------------------------------------------------------

/// Where the overlay sits on the screen (like 4.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPlace {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Free,
}

/// On which side of the overlay its bar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarSide {
    Top,
    Bottom,
}

/// Texts of the overlay block, from the language file (4.12 `s_ov_ecke_*`,
/// `s_ov_leiste_*`).
pub struct OverlayTexts<'a> {
    /// Top left, top right, bottom left, bottom right.
    pub corners: [&'a str; 4],
    pub free: &'a str,
    pub bar_top: &'a str,
    pub bar_bottom: &'a str,
}

/// Side of the square: the cross and the four corners in its notches.
pub const PLACE_SQUARE: f32 = 184.0;
/// Side of one corner piece.
const PLACE_CORNER: f32 = 66.0;
/// Height of the two bar boxes below the square.
const BAR_ROW: f32 = 64.0;
/// Space between the pieces that together form the square.
const PLACE_SEAM: f32 = 2.0;

/// The block that places the overlay, compact and symmetric: free movement
/// is a cross over the whole height and width, the four corners sit in its
/// notches, together a square. Below it the
/// two bar boxes side by side, together as wide as the square: bar on top at
/// the left, at the bottom at the right. A corner at the bottom puts the bar
/// at the bottom, one at the top on top; the boxes change it afterwards.
/// Returns whether anything changed.
pub fn overlay_place(
    ui: &mut Ui,
    scheme: &Scheme,
    place: &mut OverlayPlace,
    bar: &mut BarSide,
    texts: &OverlayTexts,
) -> bool {
    let (block, _) = ui.allocate_exact_size(vec2(PLACE_SQUARE, PLACE_SQUARE + GAP + BAR_ROW), Sense::hover());
    let square = Rect::from_min_size(block.min, Vec2::splat(PLACE_SQUARE));
    let mut changed = false;
    // The cross first: the corners come after, lie on top and take the click
    // in their notches; everything else of the square is the cross.
    if free_cross(ui, scheme, square, texts.free, *place == OverlayPlace::Free) {
        *place = OverlayPlace::Free;
        changed = true;
    }
    for (k, (spot, align)) in CORNERS.into_iter().enumerate() {
        let chosen = *place == spot;
        // The chosen corner shows the bar as set, the others as they would start.
        let shown = if chosen { *bar } else { corner_bar(align) };
        if corner_piece(ui, scheme, corner_rect(square, align), texts.corners[k], align, shown, chosen) {
            *place = spot;
            *bar = corner_bar(align);
            changed = true;
        }
    }
    let row = Rect::from_min_size(pos2(block.min.x, square.max.y + GAP), vec2(PLACE_SQUARE, BAR_ROW));
    let half = (PLACE_SQUARE - PLACE_SEAM) / 2.0;
    for (k, (side, name)) in
        [(BarSide::Top, texts.bar_top), (BarSide::Bottom, texts.bar_bottom)].into_iter().enumerate()
    {
        let rect = Rect::from_min_size(row.min + vec2(k as f32 * (half + PLACE_SEAM), 0.0), vec2(half, BAR_ROW));
        if bar_box(ui, scheme, rect, name, side, *bar == side) {
            *bar = side;
            changed = true;
        }
    }
    changed
}

/// The corners in the order of [`OverlayTexts::corners`].
const CORNERS: [(OverlayPlace, egui::Align2); 4] = [
    (OverlayPlace::TopLeft, egui::Align2::LEFT_TOP),
    (OverlayPlace::TopRight, egui::Align2::RIGHT_TOP),
    (OverlayPlace::BottomLeft, egui::Align2::LEFT_BOTTOM),
    (OverlayPlace::BottomRight, egui::Align2::RIGHT_BOTTOM),
];

/// The bar a corner starts with: at the bottom for the bottom corners, so
/// the overlay grows away from the screen edge.
fn corner_bar(corner: egui::Align2) -> BarSide {
    if corner.y() == egui::Align::Max { BarSide::Bottom } else { BarSide::Top }
}

fn corner_rect(square: Rect, align: egui::Align2) -> Rect {
    align.align_size_within_rect(Vec2::splat(PLACE_CORNER), square)
}

/// The outline of the cross: the square without the corner pieces and the
/// seam around them, clockwise from the top of the upright bar.
fn cross_outline(square: Rect) -> Vec<egui::Pos2> {
    let (a, b, s) = (PLACE_CORNER + PLACE_SEAM, PLACE_SQUARE - PLACE_CORNER - PLACE_SEAM, PLACE_SQUARE);
    [(a, 0.0), (b, 0.0), (b, a), (s, a), (s, b), (b, b), (b, s), (a, s), (a, b), (0.0, b), (0.0, a), (a, a)]
        .into_iter()
        .map(|(x, y)| square.min + vec2(x, y))
        .collect()
}

/// The free-movement cross: a cross from edge to edge, the move symbol and the name
/// in one line across its middle; chosen = accent frame and dark accent area.
fn free_cross(ui: &mut Ui, scheme: &Scheme, square: Rect, text: &str, chosen: bool) -> bool {
    let response = ui.interact(square, ui.make_persistent_id(("overlay-place", "free")), Sense::click());
    let (fill, border, width) = piece_colors(scheme, chosen, response.hovered());
    let (a, b) = (PLACE_CORNER + PLACE_SEAM, PLACE_SQUARE - PLACE_CORNER - PLACE_SEAM);
    let upright = Rect::from_min_max(square.min + vec2(a, 0.0), square.min + vec2(b, PLACE_SQUARE));
    let across = Rect::from_min_max(square.min + vec2(0.0, a), square.min + vec2(PLACE_SQUARE, b));
    let painter = ui.painter();
    painter.rect_filled(upright, 0.0, color(fill));
    painter.rect_filled(across, 0.0, color(fill));
    painter.add(egui::epaint::PathShape::closed_line(cross_outline(square), Stroke::new(width, color(border))));
    let small = TextStyle::Small.resolve(ui.style());
    let galley = painter.layout_no_wrap(text.to_string(), small, color(scheme.fg));
    let icon = 16.0;
    let left = square.center().x - (icon + 4.0 + galley.size().x) / 2.0;
    let icon_rect = Rect::from_min_size(pos2(left, square.center().y - icon / 2.0), Vec2::splat(icon));
    painter.galley(pos2(icon_rect.max.x + 4.0, square.center().y - galley.size().y / 2.0), galley, color(scheme.fg));
    let tone = if chosen { scheme.accent } else { scheme.sub };
    if let Some(image) = crate::icons::image("move", tone, icon) {
        image.paint_at(ui, icon_rect);
    }
    // The focus frame follows the cross, not the square around it.
    if response.has_focus() {
        let ring = egui::epaint::PathShape::closed_line(cross_outline(square), Stroke::new(2.0, color(scheme.accent)));
        ui.painter().add(ring);
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Fill, frame and frame width of a piece, like the bar boxes.
fn piece_colors(
    scheme: &Scheme,
    chosen: bool,
    hovered: bool,
) -> (versekit_core::theme::Color, versekit_core::theme::Color, f32) {
    if chosen {
        (scheme.accent_dark, scheme.accent, 2.0)
    } else if hovered {
        (scheme.hover, scheme.accent, 1.0)
    } else {
        (scheme.surface, scheme.control_border(), 1.0)
    }
}

/// Where the small overlay sits in a corner piece: in the corner of the
/// screen it stands for.
fn corner_mark(rect: Rect, corner: egui::Align2) -> Rect {
    corner.align_size_within_rect(vec2(30.0, 20.0), rect.shrink(10.0))
}

/// A corner piece: a small overlay in that corner with its bar on `bar`'s
/// side; the name shows on hover.
fn corner_piece(
    ui: &mut Ui,
    scheme: &Scheme,
    rect: Rect,
    text: &str,
    corner: egui::Align2,
    bar: BarSide,
    chosen: bool,
) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(("overlay-place", text)), Sense::click());
    let (fill, border, width) = piece_colors(scheme, chosen, response.hovered());
    let radius = control_radius(scheme);
    let painter = ui.painter();
    painter.rect(rect, radius, color(fill), Stroke::new(width, color(border)), StrokeKind::Inside);
    let tone = if chosen { scheme.accent } else { scheme.sub };
    let mark = corner_mark(rect, corner);
    painter.rect(mark, 2.0, color(scheme.bg), Stroke::new(1.0, color(tone)), StrokeKind::Inside);
    let strip = match bar {
        BarSide::Top => Rect::from_min_size(mark.min, vec2(mark.width(), 5.0)),
        BarSide::Bottom => Rect::from_min_max(pos2(mark.min.x, mark.max.y - 5.0), mark.max),
    };
    painter.rect_filled(strip, 1.0, color(tone));
    widgets::focus_ring(ui, &response, scheme, radius);
    response.on_hover_text(text).on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// One of the two bar boxes: a small overlay with its bar on top or at the
/// bottom, the name below; chosen = accent frame and dark accent area.
fn bar_box(ui: &mut Ui, scheme: &Scheme, rect: Rect, name: &str, side: BarSide, chosen: bool) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(("overlay-bar", name)), Sense::click());
    let (fill, border, width) = if chosen {
        (scheme.accent_dark, scheme.accent, 2.0)
    } else if response.hovered() {
        (scheme.hover, scheme.accent, 1.0)
    } else {
        (scheme.surface, scheme.control_border(), 1.0)
    };
    let painter = ui.painter();
    painter.rect(rect, control_radius(scheme), color(fill), Stroke::new(width, color(border)), StrokeKind::Inside);
    // The small overlay, its bar in the accent.
    let overlay = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 22.0), vec2(40.0, 24.0));
    painter.rect(overlay, 3.0, color(scheme.bg), Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
    let strip = match side {
        BarSide::Top => Rect::from_min_size(overlay.min, vec2(overlay.width(), 6.0)),
        BarSide::Bottom => Rect::from_min_max(pos2(overlay.min.x, overlay.max.y - 6.0), overlay.max),
    };
    painter.rect_filled(strip, 2.0, color(scheme.accent));
    let galley = painter.layout_no_wrap(name.to_string(), TextStyle::Small.resolve(ui.style()), color(scheme.fg));
    painter.galley(
        pos2(rect.center().x - galley.size().x / 2.0, rect.max.y - galley.size().y - 8.0),
        galley,
        color(scheme.fg),
    );
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::theme;

    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, add: &mut dyn FnMut(&mut Ui)) {
        let input = egui::RawInput {
            // Wide enough for two cards of the least width side by side.
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(2.0 * SETTINGS_COLUMN_MIN + 100.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| add(ui));
        output.textures_delta.clear();
    }

    fn click(ctx: &egui::Context, pos: egui::Pos2, add: &mut dyn FnMut(&mut Ui)) {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, vec![egui::Event::PointerMoved(pos)], add);
        frame(ctx, vec![button(true)], add);
        frame(ctx, vec![button(false)], add);
    }

    const TEXTS: OverlayTexts<'static> = OverlayTexts {
        corners: ["Oben links", "Oben rechts", "Unten links", "Unten rechts"],
        free: "Frei verschiebbar",
        bar_top: "Leiste oben",
        bar_bottom: "Leiste unten",
    };

    /// The title of a group stands in capitals in the accent of every scheme, the color the contrast check measures.
    #[test]
    fn the_title_of_a_group_stands_in_the_accent_of_every_scheme() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for s in theme::schemes() {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 200.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| setting_group(ui, s, "Info-Leiste"));
            output.textures_delta.clear();
            let colors: Vec<egui::Color32> = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) if text.galley.text() == "INFO-LEISTE" => {
                        text.galley.job.sections.first().map(|section| section.format.color)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(colors, [color(widgets::section_title_color(s))], "{}", s.id);
            assert_eq!(widgets::section_title_color(s), s.accent, "{}", s.id);
        }
    }

    #[test]
    fn the_overlay_square_places_the_overlay_where_it_is_clicked() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let (mut place, mut bar) = (OverlayPlace::TopLeft, BarSide::Top);
        let mut top = egui::Pos2::ZERO;
        frame(&ctx, vec![], &mut |ui| {
            top = ui.cursor().min;
            overlay_place(ui, s, &mut place, &mut bar, &TEXTS);
        });
        let square = Rect::from_min_size(top, Vec2::splat(PLACE_SQUARE));
        let mut at = |pos| {
            click(&ctx, pos, &mut |ui| {
                overlay_place(ui, s, &mut place, &mut bar, &TEXTS);
            });
            (place, bar)
        };
        let boxes = square.max.y + GAP + BAR_ROW / 2.0;
        // A bottom corner puts the bar at the bottom first, a top corner on top.
        assert_eq!(at(square.right_bottom() - vec2(10.0, 10.0)), (OverlayPlace::BottomRight, BarSide::Bottom));
        assert_eq!(at(pos2(square.left() + 30.0, boxes)), (OverlayPlace::BottomRight, BarSide::Top), "box changes it");
        assert_eq!(at(square.left_top() + vec2(10.0, 10.0)), (OverlayPlace::TopLeft, BarSide::Top));
        let bottom_left = square.left_bottom() + vec2(PLACE_CORNER - 6.0, -(PLACE_CORNER - 6.0));
        assert_eq!(at(bottom_left), (OverlayPlace::BottomLeft, BarSide::Bottom), "inside the notch: the corner");
        // Free keeps the bar.
        assert_eq!(
            at(square.center()),
            (OverlayPlace::Free, BarSide::Bottom),
            "the cross has its middle in the middle"
        );
        // The cross reaches every edge: its ends belong to it, not to a corner.
        for end in [square.center_top() + vec2(0.0, 4.0), square.left_center() + vec2(4.0, 0.0)] {
            assert_eq!(at(square.right_top() + vec2(-10.0, 10.0)).0, OverlayPlace::TopRight);
            assert_eq!(at(end).0, OverlayPlace::Free, "end of the cross at {end:?}");
        }
        assert_eq!(at(pos2(square.right() - 30.0, boxes)).1, BarSide::Bottom);
    }

    #[test]
    fn the_cross_and_the_corners_make_one_square() {
        let square = Rect::from_min_size(pos2(0.0, 0.0), Vec2::splat(PLACE_SQUARE));
        let outline = cross_outline(square);
        // Every corner piece fills its notch up to the seam.
        for (_, align) in CORNERS {
            let rect = corner_rect(square, align);
            assert!(square.contains_rect(rect));
            let notch = outline.iter().filter(|p| rect.expand(PLACE_SEAM + 0.01).contains(**p)).count();
            assert_eq!(notch, 3, "{align:?}: the three points of the notch lie a seam from the piece");
        }
        // The cross ends flush with the square on all four sides.
        let ends = |f: fn(&egui::Pos2) -> bool| outline.iter().filter(|p| f(p)).count();
        assert_eq!(ends(|p| p.y == 0.0), 2);
        assert_eq!(ends(|p| p.y == PLACE_SQUARE), 2);
        assert_eq!(ends(|p| p.x == 0.0), 2);
        assert_eq!(ends(|p| p.x == PLACE_SQUARE), 2);
    }

    #[test]
    fn the_small_overlay_sits_in_the_corner_it_stands_for() {
        let square = Rect::from_min_size(pos2(0.0, 0.0), Vec2::splat(PLACE_SQUARE));
        for (spot, align) in CORNERS {
            let rect = corner_rect(square, align);
            let mark = corner_mark(rect, align);
            assert!(rect.shrink(4.0).contains_rect(mark), "{spot:?}: mark sticks out: {mark:?}");
            let corner = rect.min + vec2(rect.width() * align.x().to_factor(), rect.height() * align.y().to_factor());
            let nearest = mark.clamp(corner);
            assert!(nearest.distance(corner) < 16.0, "{spot:?}: mark not in its corner: {mark:?}");
        }
    }

    #[test]
    fn the_free_name_fits_the_arm_of_the_cross() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        frame(&ctx, vec![], &mut |_| {});
        frame(&ctx, vec![], &mut |ui| {
            let small = TextStyle::Small.resolve(ui.style());
            let size = ui.painter().layout_no_wrap(TEXTS.free.to_string(), small, egui::Color32::WHITE).size();
            let arm = PLACE_SQUARE - 2.0 * (PLACE_CORNER + PLACE_SEAM);
            assert!(16.0 + 4.0 + size.x < PLACE_SQUARE - 16.0, "„{}“ is too wide for the square", TEXTS.free);
            assert!(size.y < arm - 8.0, "„{}“ is too tall for the arm", TEXTS.free);
        });
    }

    #[test]
    fn group_cards_stand_side_by_side_and_equally_tall() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let groups = [("Start", 1), ("Töne", 3), ("Sprache", 2)];
        let mut on = [true; 6];
        let mut cards = Vec::new();
        for _ in 0..3 {
            frame(&ctx, vec![], &mut |ui| {
                cards = settings_page(ui, s, "test", &groups, &mut |ui, g, r| {
                    setting_row(ui, s, "Name", "Hinweis", false, |row| {
                        row.add(widgets::Toggle::new(&mut on[g * 2 + r], s));
                    });
                });
            });
        }
        assert_eq!(cards.len(), 3);
        let (a, b, c) = (cards[0], cards[1], cards[2]);
        assert_eq!(a.top(), b.top(), "the first two groups stand side by side");
        assert!(a.right() < b.left());
        assert!((a.height() - b.height()).abs() < 0.5, "one row and three rows, yet equally tall: {a:?} {b:?}");
        assert!(c.top() > a.bottom(), "the third group starts the next pair");
        assert!(c.width() < a.width() + 1.0, "a single last card keeps half the width");
    }

    #[test]
    fn a_wide_group_gets_a_line_and_the_whole_width_to_itself() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let groups = [("Start", 1), ("Töne", 1), ("Breit", 2), ("Sprache", 1)];
        let mut on = [true; 8];
        let mut cards = Vec::new();
        for _ in 0..3 {
            frame(&ctx, vec![], &mut |ui| {
                cards = settings_page_wide(ui, s, "breit", &groups, &[2], &mut |ui, g, r| {
                    setting_row(ui, s, "Name", "Hinweis", false, |row| {
                        row.add(widgets::Toggle::new(&mut on[g * 2 + r], s));
                    });
                });
            });
        }
        assert_eq!(cards.len(), 4);
        let (a, b, wide, last) = (cards[0], cards[1], cards[2], cards[3]);
        assert_eq!(a.top(), b.top(), "the first two stand side by side");
        assert!(wide.top() > a.bottom(), "the wide group starts a line of its own");
        assert!(wide.width() > a.width() * 1.9, "and takes the whole width: {wide:?} against {a:?}");
        assert!(last.top() > wide.bottom(), "the next group starts the line after it");
        assert!(last.width() < wide.width() * 0.6, "and is half as wide again");
    }

    #[test]
    fn a_row_is_as_tall_with_any_control() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let (mut on, mut heights) = (true, Vec::new());
        frame(&ctx, vec![], &mut |ui| {
            heights.clear();
            let rows: [&mut dyn FnMut(&mut Ui); 4] = [
                &mut |row| {
                    row.add(widgets::Toggle::new(&mut on, s));
                },
                &mut |row| {
                    widgets::radio(row, s, true, "Deutsch");
                },
                &mut |row| {
                    row.add(widgets::Button::new("Wie im Basetool", ButtonKind::Normal, s));
                },
                &mut |_| {},
            ];
            for add in rows {
                heights.push(setting_row(ui, s, "Name", "Hinweis", false, |row| add(row)).response.rect.height());
            }
        });
        assert!(heights.iter().all(|h| (h - heights[0]).abs() < 0.5), "rows differ in height: {heights:?}");
    }

    #[test]
    fn narrow_windows_stack_the_cards_each_as_tall_as_its_content() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let groups = [("Start", 1), ("Töne", 3)];
        let mut on = [true; 4];
        let mut cards = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(500.0, 900.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                cards = settings_page(ui, s, "schmal", &groups, &mut |ui, g, r| {
                    setting_row(ui, s, "Name", "Hinweis", false, |row| {
                        row.add(widgets::Toggle::new(&mut on[g + r], s));
                    });
                });
            });
            output.textures_delta.clear();
        }
        let (a, b) = (cards[0], cards[1]);
        assert!(b.top() > a.bottom(), "too narrow for two: the second card goes below");
        assert_eq!(a.left(), b.left());
        assert!(a.height() + 30.0 < b.height(), "stacked, one row stays smaller than three: {a:?} {b:?}");
    }

    #[test]
    fn only_confirming_marks_a_step_and_finishing_needs_every_step() {
        let mut w = WizardState::new(4);
        // Jumping ahead marks nothing.
        w.step = 2;
        assert_eq!(w.open(), vec![0, 1, 2]);
        w.confirm();
        assert_eq!(w.done, vec![false, false, true, false]);
        assert_eq!(w.step, 0, "after the last real step: back to the first open one");
        w.confirm();
        assert_eq!(w.step, 1);
        // On the last step with `Sprache` still open: no finishing.
        w.step = 3;
        w.confirm();
        assert!(!w.finished(), "finished although „Sprache“ was never confirmed");
        assert_eq!(w.open(), vec![1]);
        w.step = 1;
        w.confirm();
        assert_eq!(w.step, 3, "nothing open: on to the last step");
        w.confirm();
        assert!(w.finished());
    }

    #[test]
    fn a_step_can_be_clicked_and_done_steps_are_marked() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("krt").expect("scheme");
        let steps = ["Spielordner", "Sprache", "Overlay", "Fertig"];
        let mut clicked = None;
        let mut top = 0.0;
        frame(&ctx, vec![], &mut |ui| {
            top = ui.cursor().min.y;
            step_list(ui, s, &steps, 0, &[true, false, false, false]);
        });
        click(&ctx, pos2(60.0, top + STEP_HEIGHT * 2.5), &mut |ui| {
            if let Some(k) = step_list(ui, s, &steps, 0, &[true, false, false, false]) {
                clicked = Some(k);
            }
        });
        assert_eq!(clicked, Some(2), "a click on the third step jumps there");
    }

    #[test]
    fn settings_parts_draw_in_every_scheme() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let texts =
            UpdateTexts { message: "5.1 ist da.", update: "Aktualisieren", news: "Was ist neu", update_blocked: None };
        for s in theme::schemes() {
            let (mut place, mut bar, mut on) = (OverlayPlace::Free, BarSide::Bottom, true);
            frame(&ctx, vec![], &mut |ui| {
                setting_group(ui, s, "Overlay");
                setting_row(ui, s, "Durchklickbar", "Mausklicks gehen ins Spiel.", true, |ui| {
                    ui.add(widgets::Toggle::new(&mut on, s));
                });
                step_list(ui, s, &["A", "B"], 1, &[true, false]);
                update_bar(ui, s, &texts);
                overlay_place(ui, s, &mut place, &mut bar, &TEXTS);
            });
        }
    }
}
