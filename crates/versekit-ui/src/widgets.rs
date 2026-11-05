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

//! Verse-Kit's own controls, drawn in the colors of a scheme (what egui's
//! styling does not cover): toggle switch, scroll bars, boxes with the KRT
//! corner brackets, buttons (filled accent buttons in KRT) and headings.
//!
//! Sizes and color rules follow the Python version (`main_window.py`:
//! `toggle_switch`, `round_scrollbar`, `round_frame`, `_corner_marks`;
//! `pages.py`: `_button`, `_heading`).

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Color32, CornerRadius, CursorIcon, FontId, FontSelection, Frame, InnerResponse, Layout, Margin, Pos2, Rect,
    Response, RichText, Role, ScrollArea, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2, Widget, WidgetInfo,
    WidgetText, pos2, scroll_area::ScrollAreaOutput, style::ScrollStyle, vec2,
};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::{box_radius, color, control_radius};

// ---------------------------------------------------------------------------
// Keyboard focus
// ---------------------------------------------------------------------------

/// The frame that shows where Tab has put the focus: 2 points in the accent
/// around the control. egui gives own controls the focus only by keyboard,
/// so a click never shows it.
pub fn focus_ring(ui: &Ui, response: &Response, scheme: &Scheme, radius: CornerRadius) {
    if response.has_focus() {
        let stroke = Stroke::new(2.0, color(scheme.accent));
        ui.painter().rect_stroke(response.rect.expand(2.0), radius, stroke, StrokeKind::Outside);
    }
}

// ---------------------------------------------------------------------------
// Toggle switch
// ---------------------------------------------------------------------------

pub const TOGGLE_SIZE: Vec2 = vec2(44.0, 24.0);

/// Capsule and dot of a toggle switch placed at `rect` (44 × 24).
pub fn toggle_geometry(rect: Rect, on: bool) -> (Rect, Pos2) {
    let capsule = Rect::from_min_max(rect.min + vec2(2.0, 3.0), rect.max - vec2(2.0, 3.0));
    let x = if on { rect.width() - 24.0 } else { 0.0 };
    let dot = rect.min + vec2(12.0 + x, 13.0);
    (capsule, dot)
}

/// Round switch, on or off. Stays round in square schemes.
pub struct Toggle<'a> {
    on: &'a mut bool,
    scheme: &'a Scheme,
    knob: Option<versekit_core::theme::Color>,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool, scheme: &'a Scheme) -> Self {
        Toggle { on, scheme, knob: None }
    }

    /// The knob in another colour of the scheme than the accent (on) or the muted one (off): a switch that
    /// shows a state.
    pub fn knob(mut self, knob: Option<versekit_core::theme::Color>) -> Self {
        self.knob = knob;
        self
    }
}

impl Widget for Toggle<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(TOGGLE_SIZE, Sense::click());
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let (capsule, dot) = toggle_geometry(rect, *self.on);
            let (track, knob) = if *self.on {
                (self.scheme.track_on, self.knob.unwrap_or(self.scheme.accent))
            } else {
                (self.scheme.track, self.knob.unwrap_or(self.scheme.sub))
            };
            let painter = ui.painter();
            painter.rect_filled(capsule, CornerRadius::same(9), color(track));
            painter.circle_filled(dot, 7.0, color(knob));
            // The mouse over it: a ring in the accent around the capsule.
            if response.hovered() {
                let ring = Stroke::new(1.5, color(self.scheme.accent));
                painter.rect_stroke(capsule.expand(1.5), CornerRadius::same(10), ring, StrokeKind::Outside);
            }
        }
        focus_ring(ui, &response, self.scheme, CornerRadius::same(12));
        response.on_hover_cursor(CursorIcon::PointingHand)
    }
}

// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

/// Default width of a slider (`slider(…, width=190)`).
pub const SLIDER_WIDTH: f32 = 190.0;
const SLIDER_HEIGHT: f32 = 26.0;
const KNOB_RADIUS: f32 = 8.0;

/// Where the knob sits for `value`, in points from the left edge.
pub fn slider_x(width: f32, minimum: i64, maximum: i64, value: i64) -> f32 {
    let span = (maximum - minimum).max(1) as f32;
    let fraction = ((value - minimum) as f32 / span).clamp(0.0, 1.0);
    KNOB_RADIUS + fraction * (width - 2.0 * KNOB_RADIUS)
}

/// The value at `x` points from the left edge, rounded like Python's
/// `round()` (ties to even).
pub fn slider_value(width: f32, minimum: i64, maximum: i64, x: f32) -> i64 {
    let span = (maximum - minimum).max(1) as f64;
    let fraction = (f64::from(x - KNOB_RADIUS) / f64::from(width - 2.0 * KNOB_RADIUS)).clamp(0.0, 1.0);
    (minimum as f64 + fraction * span).round_ties_even() as i64
}

/// A slider in Verse-Kit's style: thin track, filled part and knob in the
/// accent. Click or drag sets the value.
pub struct Slider<'a> {
    value: &'a mut i64,
    minimum: i64,
    maximum: i64,
    width: f32,
    scheme: &'a Scheme,
    on_release: bool,
}

impl<'a> Slider<'a> {
    pub fn new(value: &'a mut i64, minimum: i64, maximum: i64, scheme: &'a Scheme) -> Self {
        Slider { value, minimum, maximum, width: SLIDER_WIDTH, scheme, on_release: false }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(2.0 * KNOB_RADIUS + 1.0);
        self
    }

    /// While the mouse button is down, the knob only shows the new value
    /// ([`Slider::shown`]); the value changes on release. For sliders whose
    /// change moves the slider itself (the interface size), otherwise the
    /// slider slips away under the mouse and the size jumps back and forth.
    pub fn on_release(mut self) -> Self {
        self.on_release = true;
        self
    }

    /// The value a slider shows right now: while dragging an
    /// [`on_release`](Slider::on_release) slider the value under the mouse,
    /// otherwise `None` (it shows its value).
    pub fn shown(ui: &Ui, response: &Response) -> Option<i64> {
        ui.data(|d| d.get_temp::<i64>(response.id.with("shown")))
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(vec2(self.width, SLIDER_HEIGHT), Sense::click_and_drag());
        let shown_id = response.id.with("shown");
        let mut value = *self.value;
        if let Some(pos) = response.interact_pointer_pos() {
            value = slider_value(self.width, self.minimum, self.maximum, pos.x - rect.min.x);
        }
        let holding = self.on_release && response.is_pointer_button_down_on();
        if holding {
            ui.data_mut(|d| d.insert_temp(shown_id, value));
        } else {
            ui.data_mut(|d| d.remove_temp::<i64>(shown_id));
        }
        if response.has_focus() {
            // Arrow keys move the value by one instead of moving the focus; up and down do the same as right and
            // left, page up and down move by a tenth of the way, home and end go to the ends.
            let filter = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() };
            ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
            let page = ((self.maximum - self.minimum) / 10).max(1);
            let none = egui::Modifiers::NONE;
            let (steps, ends) = ui.input_mut(|i| {
                let mut count = |key| i.count_and_consume_key(none, key) as i64;
                let steps = count(egui::Key::ArrowRight) + count(egui::Key::ArrowUp)
                    - count(egui::Key::ArrowLeft)
                    - count(egui::Key::ArrowDown)
                    + page * (count(egui::Key::PageUp) - count(egui::Key::PageDown));
                let ends = (count(egui::Key::Home) > 0, count(egui::Key::End) > 0);
                (steps, ends)
            });
            value = (value + steps).clamp(self.minimum, self.maximum);
            if ends.0 {
                value = self.minimum;
            } else if ends.1 {
                value = self.maximum;
            }
        }
        if !holding && value != *self.value {
            *self.value = value;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let y = rect.center().y;
            let x = rect.min.x + slider_x(self.width, self.minimum, self.maximum, value);
            let painter = ui.painter();
            let track = Rect::from_min_max(pos2(rect.min.x, y - 3.0), pos2(rect.max.x, y + 3.0));
            painter.rect_filled(track, CornerRadius::same(3), color(self.scheme.track));
            let filled = Rect::from_min_max(track.min, pos2(x, track.max.y));
            painter.rect_filled(filled, CornerRadius::same(3), color(self.scheme.accent));
            painter.circle_filled(pos2(x, y), KNOB_RADIUS, color(self.scheme.accent));
            // The mouse over it (or holding it): a ring around the knob, as the switch has one around its capsule.
            if response.hovered() || response.dragged() {
                painter.circle_stroke(pos2(x, y), KNOB_RADIUS + 3.0, Stroke::new(1.5, color(self.scheme.accent)));
            }
        }
        focus_ring(ui, &response, self.scheme, CornerRadius::same(4));
        response.on_hover_cursor(CursorIcon::PointingHand)
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Kind of button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Normal,
    /// The main action of a page; filled with the accent in KRT.
    Strong,
    /// Deletes or resets something; red from the start, not only on hover.
    Danger,
    /// Leads to another service, in its color (`theme::KRT_ORANGE`).
    Brand(Color),
}

/// Colors of a button in one state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonColors {
    pub text: Color,
    pub fill: Color,
    pub border: Color,
}

fn filled(scheme: &Scheme, kind: ButtonKind) -> bool {
    kind == ButtonKind::Strong && scheme.filled_buttons
}

/// How much of its own color a colored button takes into its fill on hover.
pub const HOVER_TINT: f64 = 0.25;

/// How far the fill of a brand button goes from the box to the ground, so the
/// brand color on it reaches 4.5:1 in the lighter schemes too.
const BRAND_FILL_DEPTH: f64 = 0.5;

/// The colors of `_button` in `pages.py`, with a visible hover for every kind.
pub fn button_colors(scheme: &Scheme, kind: ButtonKind, hovered: bool) -> ButtonColors {
    let mut colors = match kind {
        ButtonKind::Danger => ButtonColors { text: scheme.red, fill: scheme.danger_fill, border: scheme.red },
        ButtonKind::Brand(brand) => {
            ButtonColors { text: brand, fill: scheme.surface.mix(scheme.bg, BRAND_FILL_DEPTH), border: brand }
        }
        ButtonKind::Strong if filled(scheme, kind) => {
            ButtonColors { text: scheme.bg, fill: scheme.accent, border: scheme.accent }
        }
        ButtonKind::Strong => ButtonColors { text: scheme.accent, fill: scheme.accent_dark, border: scheme.accent },
        ButtonKind::Normal => ButtonColors { text: scheme.fg, fill: scheme.control(), border: scheme.control_border() },
    };
    if hovered {
        // Every kind changes visibly under the mouse (Python left strong and
        // danger buttons almost unchanged: their border and text already had
        // the hover color).
        match kind {
            // Design system `.btn:hover`: the light accent fill, text stays dark.
            _ if filled(scheme, kind) => colors.fill = scheme.hover_fg(),
            // The tinted fill takes contrast from text in the same color (down to
            // 3.2:1), so the text turns light as well (7.7:1 and more).
            ButtonKind::Strong => {
                colors.fill = colors.fill.mix(scheme.accent, HOVER_TINT);
                (colors.border, colors.text) = (scheme.fg, scheme.fg);
            }
            ButtonKind::Danger => {
                colors.fill = colors.fill.mix(scheme.red, HOVER_TINT);
                (colors.border, colors.text) = (scheme.fg, scheme.fg);
            }
            ButtonKind::Brand(brand) => {
                colors.fill = colors.fill.mix(brand, HOVER_TINT);
                (colors.border, colors.text) = (scheme.fg, scheme.fg);
            }
            ButtonKind::Normal => {
                colors.border = scheme.accent;
                colors.text = scheme.hover_fg();
            }
        }
    }
    colors
}

/// The label as the scheme writes it: strong buttons in capitals where they
/// are filled.
pub fn button_label(scheme: &Scheme, kind: ButtonKind, text: &str) -> String {
    if filled(scheme, kind) { text.to_uppercase() } else { text.to_string() }
}

/// A button in Verse-Kit's style: border, color on hover.
pub struct Button<'a> {
    text: &'a str,
    kind: ButtonKind,
    scheme: &'a Scheme,
    blocked: Option<&'a str>,
    wide: bool,
    wrap: bool,
    icon: Option<&'a str>,
    flag: Option<&'a str>,
    spoken: Option<&'a str>,
    tint: Option<Color>,
}

/// Size of the icon left of a button's label, and the gap after it.
pub const BUTTON_ICON_SIZE: f32 = 16.0;
const BUTTON_ICON_GAP: f32 = 6.0;

impl<'a> Button<'a> {
    pub fn new(text: &'a str, kind: ButtonKind, scheme: &'a Scheme) -> Self {
        Button {
            text,
            kind,
            scheme,
            blocked: None,
            wide: false,
            wrap: false,
            icon: None,
            flag: None,
            spoken: None,
            tint: None,
        }
    }

    /// The label (and icon) of a blocked button in `tint` instead of the muted text, e.g. a countdown that ripens.
    pub fn tint(mut self, tint: Color) -> Self {
        self.tint = Some(tint);
        self
    }

    /// The name a screen reader says, where the label alone is the same on several buttons (one "Apply" for
    /// each set of a list): the label with what it belongs to.
    pub fn spoken(mut self, name: &'a str) -> Self {
        self.spoken = Some(name);
        self
    }

    /// A flag left of the label (a translation of that country, as in 4.12);
    /// a country without a flag file gets none.
    pub fn flag(mut self, code: &'a str) -> Self {
        self.flag = Some(code);
        self
    }

    /// The button takes the whole width it has, the label in the middle.
    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    /// The label wraps where the row has no more room (a name that is not ours to shorten, such as the name of
    /// a device), instead of pushing the page wider than the window.
    pub fn wrapping(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// An icon left of the label, in the label's color.
    pub fn icon(mut self, name: &'a str) -> Self {
        self.icon = Some(name);
        self
    }

    /// While `reason` is given the button does nothing: greyed out, cursor
    /// not allowed, and on hover the reason and what to do.
    pub fn blocked(mut self, reason: Option<&'a str>) -> Self {
        self.blocked = reason;
        self
    }
}

/// Colors of a button that does nothing right now.
pub fn blocked_colors(scheme: &Scheme) -> ButtonColors {
    ButtonColors { text: scheme.sub, fill: scheme.surface, border: scheme.line }
}

/// The label of a [`Button`] laid out as it is drawn: capitals and bold where
/// the scheme fills the button.
fn button_galley(ui: &Ui, scheme: &Scheme, kind: ButtonKind, text: &str) -> std::sync::Arc<egui::Galley> {
    button_galley_in(ui, scheme, kind, text, None)
}

/// [`button_galley`], wrapped at `wrap_at` points where that is given.
fn button_galley_in(
    ui: &Ui,
    scheme: &Scheme,
    kind: ButtonKind,
    text: &str,
    wrap_at: Option<f32>,
) -> std::sync::Arc<egui::Galley> {
    let label = button_label(scheme, kind, text);
    // Filled buttons in bold, like the design system's `.btn`.
    let font = if filled(scheme, kind) {
        FontSelection::FontId(crate::fonts::bold(TextStyle::Button.resolve(ui.style()).size))
    } else {
        FontSelection::Style(TextStyle::Button)
    };
    match wrap_at {
        Some(width) => WidgetText::from(label).into_galley(ui, Some(egui::TextWrapMode::Wrap), width, font),
        None => WidgetText::from(label).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, font),
    }
}

/// Size a [`Button`] takes, for placing it before it is drawn.
pub fn button_size(ui: &Ui, scheme: &Scheme, kind: ButtonKind, text: &str) -> Vec2 {
    button_galley(ui, scheme, kind, text).size() + vec2(30.0, 16.0)
}

/// Size of a button with an icon left of its label ([`Button::icon`]).
pub fn icon_button_size(ui: &Ui, scheme: &Scheme, kind: ButtonKind, text: &str) -> Vec2 {
    let galley = button_galley(ui, scheme, kind, text).size();
    vec2(galley.x + BUTTON_ICON_SIZE + BUTTON_ICON_GAP, galley.y.max(BUTTON_ICON_SIZE)) + vec2(30.0, 16.0)
}

impl Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        // Room for the label: the row less the frame of the button (and the icon, which is added to it later).
        let wrap_at = self.wrap.then(|| (ui.available_width() - 30.0 - BUTTON_ICON_SIZE - BUTTON_ICON_GAP).max(40.0));
        let galley = button_galley_in(ui, self.scheme, self.kind, self.text, wrap_at);
        let picture = self.flag.and_then(|code| flag(ui, code));
        let lead = match (&picture, self.icon) {
            (Some((_, size)), _) => *size,
            (None, Some(_)) => Vec2::splat(BUTTON_ICON_SIZE),
            (None, None) => Vec2::ZERO,
        };
        let icon_width = if lead.x > 0.0 { lead.x + BUTTON_ICON_GAP } else { 0.0 };
        // Icon (or flag) and label as one block, centered in the button.
        let content = vec2(galley.size().x + icon_width, galley.size().y.max(lead.y.max(BUTTON_ICON_SIZE)));
        let mut size = content + vec2(30.0, 16.0);
        if self.wide {
            size.x = size.x.max(ui.available_width());
        }
        let place = |ui: &Ui, rect: Rect, galley: std::sync::Arc<egui::Galley>, text: Color| {
            let start = rect.center() - content / 2.0;
            if let Some((image, size)) = &picture {
                let at = egui::pos2(start.x, rect.center().y - size.y / 2.0);
                image.paint_at(ui, Rect::from_min_size(at, *size));
            } else if let Some(name) = self.icon
                && let Some(image) = crate::icons::image(name, text, BUTTON_ICON_SIZE)
            {
                let at = egui::pos2(start.x, rect.center().y - BUTTON_ICON_SIZE / 2.0);
                image.paint_at(ui, Rect::from_min_size(at, Vec2::splat(BUTTON_ICON_SIZE)));
            }
            let at = egui::pos2(start.x + icon_width, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(at, galley, color(text));
        };
        if let Some(reason) = self.blocked {
            let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
            if ui.is_rect_visible(rect) {
                let colors = blocked_colors(self.scheme);
                let stroke = Stroke::new(1.0, color(colors.border));
                let radius = control_radius(self.scheme);
                ui.painter().rect(rect.shrink(1.0), radius, color(colors.fill), stroke, StrokeKind::Inside);
                place(ui, rect, galley, self.tint.unwrap_or(colors.text));
            }
            response.widget_info(|| WidgetInfo::labeled(Role::Button, false, self.spoken.unwrap_or(self.text)));
            crate::highlight::mark(ui, self.scheme, rect, &[self.text]);
            return response.on_hover_cursor(CursorIcon::NotAllowed).on_hover_text(reason);
        }
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, self.spoken.unwrap_or(self.text)));
        if ui.is_rect_visible(rect) {
            let colors = button_colors(self.scheme, self.kind, response.hovered());
            let radius = control_radius(self.scheme);
            ui.painter().rect(
                rect.shrink(1.0),
                radius,
                color(colors.fill),
                Stroke::new(1.0, color(colors.border)),
                StrokeKind::Inside,
            );
            place(ui, rect, galley, colors.text);
        }
        focus_ring(ui, &response, self.scheme, control_radius(self.scheme));
        crate::highlight::mark(ui, self.scheme, rect, &[self.text]);
        response.on_hover_cursor(CursorIcon::PointingHand)
    }
}

// ---------------------------------------------------------------------------
// Heat map
// ---------------------------------------------------------------------------

/// Row height of the heat map (`STATS_CELL_HEIGHT`).
pub const HEAT_ROW_HEIGHT: f32 = 22.0;

/// A heat map like `stats_pages.heatmap`: one row per label, cells colored
/// from the scheme's `heat_low` to its accent; empty cells in the line color.
/// The label column is as wide as the longest label.
pub fn heat_map(ui: &mut Ui, scheme: &Scheme, labels: &[&str], rows: &[Vec<f64>]) -> Response {
    heat_map_hours(ui, scheme, labels, rows, 0)
}

/// [`heat_map`] with the number of the column (the hour, two digits) under
/// every `hour_step`th column; `0` for none. Every third hour is dense enough
/// to hit a cell without counting and still fits (`stats_pages.heatmap`).
pub fn heat_map_hours(ui: &mut Ui, scheme: &Scheme, labels: &[&str], rows: &[Vec<f64>], hour_step: usize) -> Response {
    heat_map_tipped(ui, scheme, labels, rows, hour_step, None)
}

/// What the heat map says about itself: its name for a reader of the screen
/// and the text of a cell (row, column, value) beside the pointer.
pub struct HeatTips<'a> {
    pub title: &'a str,
    pub cell: &'a dyn Fn(usize, usize, f64) -> String,
}

/// [`heat_map_hours`] that also names itself and shows the value of the cell
/// under the pointer.
pub fn heat_map_tipped(
    ui: &mut Ui,
    scheme: &Scheme,
    labels: &[&str],
    rows: &[Vec<f64>],
    hour_step: usize,
    tips: Option<&HeatTips<'_>>,
) -> Response {
    let highest = rows.iter().flatten().copied().fold(0.0, f64::max);
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let font = egui::FontId::proportional(13.0);
    let label_width = labels
        .iter()
        .map(|l| ui.painter().layout_no_wrap((*l).to_string(), font.clone(), color(scheme.sub)).size().x + 10.0)
        .fold(34.0, f32::max);
    let width = ui.available_width().max(label_width + columns as f32 * 8.0);
    // ⚠ The height of the hour line comes from the font, not guessed: with a
    // fixed 22 the label at the bottom was cut off in 4.12.
    let hours_height = if hour_step > 0 {
        4.0 + ui.painter().layout_no_wrap("00".into(), font.clone(), color(scheme.sub)).size().y + 4.0
    } else {
        0.0
    };
    let size = vec2(width, rows.len() as f32 * HEAT_ROW_HEIGHT + hours_height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let cell = (width - label_width) / columns as f32;
        if hour_step > 0 {
            for column in (0..columns).step_by(hour_step) {
                painter.text(
                    pos2(
                        rect.min.x + label_width + column as f32 * cell,
                        rect.min.y + rows.len() as f32 * HEAT_ROW_HEIGHT + 4.0,
                    ),
                    egui::Align2::LEFT_TOP,
                    format!("{column:02}"),
                    font.clone(),
                    color(scheme.sub),
                );
            }
        }
        for (row, values) in rows.iter().enumerate() {
            let y = rect.min.y + row as f32 * HEAT_ROW_HEIGHT;
            if let Some(label) = labels.get(row) {
                painter.text(
                    pos2(rect.min.x, y + HEAT_ROW_HEIGHT / 2.0),
                    egui::Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    color(scheme.sub),
                );
            }
            for (column, value) in values.iter().enumerate() {
                let x = rect.min.x + label_width + column as f32 * cell;
                let cell_rect =
                    Rect::from_min_max(pos2(x + 1.0, y + 1.0), pos2(x + cell - 1.0, y + HEAT_ROW_HEIGHT - 1.0));
                painter.rect_filled(cell_rect, CornerRadius::ZERO, color(scheme.heat_cell(*value, highest)));
            }
        }
    }
    let Some(tips) = tips else { return response };
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, tips.title));
    let cell = (width - label_width) / columns as f32;
    let under_pointer = response.hover_pos().and_then(|at| {
        let (x, y) = (at.x - rect.min.x - label_width, at.y - rect.min.y);
        let (column, row) = ((x / cell).floor(), (y / HEAT_ROW_HEIGHT).floor());
        (x >= 0.0 && y >= 0.0 && (row as usize) < rows.len()).then_some((row as usize, column as usize))
    });
    match under_pointer.and_then(|(row, column)| Some((row, column, *rows.get(row)?.get(column)?))) {
        Some((row, column, value)) => response.on_hover_text((tips.cell)(row, column, value)),
        None => response,
    }
}

// ---------------------------------------------------------------------------
// Input fields
// ---------------------------------------------------------------------------

/// A one-line input field whose hint stands in the field as muted text while
/// it is empty, so nothing has to be deleted before typing. The hint says what
/// goes in, not just that it is a search.
pub fn text_field<'a>(text: &'a mut String, hint: &str, scheme: &Scheme) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text).hint_text(RichText::new(hint).color(color(scheme.sub)))
}

/// The multi-line counterpart of [`text_field`].
pub fn text_area<'a>(text: &'a mut String, hint: &str, scheme: &Scheme) -> egui::TextEdit<'a> {
    egui::TextEdit::multiline(text).hint_text(RichText::new(hint).color(color(scheme.sub)))
}

/// Size of the X that empties a field.
pub const CLEAR_SIZE: f32 = 14.0;

/// Inner margin of a field with room for the X at the right edge.
fn clearable_margin() -> Margin {
    Margin { left: 4, right: 4 + CLEAR_SIZE as i8 + 4, top: 2, bottom: 2 }
}

/// Draws the X inside `field` while it holds text; a click empties the field
/// and leaves the cursor in it. Returns whether it emptied the field.
fn clear_button(ui: &mut Ui, field: &Response, text: &mut String, scheme: &Scheme) -> bool {
    clear_button_at(ui, field, text, scheme, 0.0)
}

/// [`clear_button`] with `inset` more room to its right (for the arrow of a select field).
fn clear_button_at(ui: &mut Ui, field: &Response, text: &mut String, scheme: &Scheme, inset: f32) -> bool {
    if text.is_empty() {
        return false;
    }
    let center = pos2(field.rect.right() - 4.0 - inset - CLEAR_SIZE / 2.0, field.rect.center().y);
    let rect = Rect::from_center_size(center, Vec2::splat(CLEAR_SIZE));
    let response = ui.interact(rect, field.id.with("clear"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
    if let Some(image) = crate::icons::themed("x", scheme, tint, CLEAR_SIZE) {
        image.paint_at(ui, rect);
    }
    if !response.clicked() {
        return false;
    }
    text.clear();
    ui.memory_mut(|m| m.request_focus(field.id));
    true
}

/// A one-line field with an X inside that empties it while it holds text.
/// `width` is the whole field including the X.
///
/// The field's id comes from its hint, not from its place: egui numbers
/// unnamed widgets by order, so when something above the field disappears
/// while typing (the menu filter hides an entry), the field would get a new
/// id and lose the cursor.
pub fn clearable_field(ui: &mut Ui, text: &mut String, hint: &str, scheme: &Scheme, width: f32) -> Response {
    let edit = text_field(text, hint, scheme).id_salt(("field", hint)).margin(clearable_margin()).desired_width(width);
    let mut response = ui.add(edit);
    if clear_button(ui, &response, text, scheme) {
        response.mark_changed();
    }
    response
}

/// Size of the magnifier at the left edge of a search line.
pub const SEARCH_ICON_SIZE: f32 = 14.0;

/// Inner margin of a search line: room for the magnifier at the left and the X at the right.
fn search_margin() -> Margin {
    Margin { left: 4 + SEARCH_ICON_SIZE as i8 + 4, ..clearable_margin() }
}

/// The visuals a search line draws with, to be put back with [`SearchLook::end`].
struct SearchLook {
    border: Stroke,
    ground: Option<Color32>,
}

impl SearchLook {
    /// Gives the fields drawn from now on the muted border and the lighter ground of a search line. The
    /// border at focus stays as it is for every field: the full accent, 2 points.
    fn begin(ui: &mut Ui, scheme: &Scheme) -> SearchLook {
        let visuals = ui.visuals_mut();
        let saved = SearchLook { border: visuals.widgets.inactive.bg_stroke, ground: visuals.text_edit_bg_color };
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, color(scheme.search_border()));
        visuals.text_edit_bg_color = Some(color(scheme.search_ground()));
        saved
    }

    fn end(self, ui: &mut Ui) {
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_stroke = self.border;
        visuals.text_edit_bg_color = self.ground;
    }
}

/// Draws the magnifier inside `field` at the left edge: in the accent while the field has the cursor.
fn search_icon(ui: &mut Ui, field: &Response, scheme: &Scheme) {
    let tint = if field.has_focus() { IconTint::Highlight } else { IconTint::Normal };
    let rect = Rect::from_center_size(
        pos2(field.rect.left() + 4.0 + SEARCH_ICON_SIZE / 2.0, field.rect.center().y),
        Vec2::splat(SEARCH_ICON_SIZE),
    );
    if let Some(image) = crate::icons::themed("search", scheme, tint, SEARCH_ICON_SIZE) {
        image.paint_at(ui, rect);
    }
}

/// A search line: a [`clearable_field`] that stands out from the other fields by a muted border in the accent
/// color (the full accent while it has the cursor), a magnifier at the left and a slightly lighter ground.
/// Every search of a page is drawn with it, never with a plain field.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str, scheme: &Scheme, width: f32) -> Response {
    let look = SearchLook::begin(ui, scheme);
    let edit = text_field(text, hint, scheme).id_salt(("field", hint)).margin(search_margin()).desired_width(width);
    let mut response = ui.add(edit);
    look.end(ui);
    search_icon(ui, &response, scheme);
    if clear_button(ui, &response, text, scheme) {
        response.mark_changed();
    }
    response
}

/// What a [`search_field_with_target`] did this frame.
pub struct SearchTarget {
    pub response: Response,
    /// Enter was pressed on a field with text: that text. The field is empty again.
    pub entered: Option<String>,
}

/// A [`search_field`] with a target for Enter (the menu filter opens its first hit): Enter hands the text over
/// and empties the field, Escape empties it. The caller still holds the text of this frame from before the field.
pub fn search_field_with_target(ui: &mut Ui, text: &mut String, hint: &str, scheme: &Scheme, width: f32) -> SearchTarget {
    let mut response = search_field(ui, text, hint, scheme, width);
    let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    let mut entered = None;
    if response.lost_focus() && enter && !text.trim().is_empty() {
        entered = Some(std::mem::take(text));
        response.mark_changed();
    } else if escape && (response.has_focus() || response.lost_focus()) && !text.is_empty() {
        text.clear();
        response.mark_changed();
    }
    SearchTarget { response, entered }
}

// ---------------------------------------------------------------------------
// Field with suggestions
// ---------------------------------------------------------------------------

/// What [`SuggestField::show`] returns.
pub struct SuggestOutput {
    pub response: Response,
    /// The entry picked this frame (Enter, Tab or a click), as its position
    /// in the entries.
    pub picked: Option<usize>,
    /// Are the suggestions visible?
    pub is_open: bool,
}

#[derive(Clone, Default)]
struct SuggestState {
    selected: usize,
    /// The input the selection belongs to; typing starts again at the first.
    query: String,
    open: bool,
    /// The input for which the list was closed (Esc or a pick); it stays
    /// closed until the input changes.
    closed_for: Option<String>,
}

/// Takes presses of `key` without modifiers out of the input, so the text
/// field does not see them; returns how many there were.
fn take_key(input: &mut egui::InputState, key: egui::Key) -> usize {
    let mut count = 0;
    input.events.retain(|event| {
        let hit = matches!(event, egui::Event::Key { key: k, pressed: true, modifiers, .. }
            if *k == key && modifiers.is_none());
        count += usize::from(hit);
        !hit
    });
    count
}

/// A one-line field that suggests matching entries while typing (order in
/// [`crate::suggest`]): at most eight, the typed part
/// highlighted, below them how many more match. Arrow keys choose, Enter,
/// Tab or a click picks, Esc closes; the X empties the field.
///
/// Built after egui's `CompletionPopup`, which has no line for further
/// matches and no highlighting.
pub struct SuggestField<'a> {
    id_salt: &'a str,
    hint: &'a str,
    scheme: &'a Scheme,
    width: f32,
    attention: bool,
    searching: bool,
    more_label: &'a dyn Fn(usize) -> String,
}

impl<'a> SuggestField<'a> {
    /// `more_label` writes the line below the suggestions for the number of
    /// further matches.
    pub fn new(id_salt: &'a str, hint: &'a str, scheme: &'a Scheme, more_label: &'a dyn Fn(usize) -> String) -> Self {
        SuggestField { id_salt, hint, scheme, width: f32::INFINITY, attention: false, searching: false, more_label }
    }

    /// Draws a ring in the accent around the field while `on`: the field is what the page waits for.
    pub fn attention(mut self, on: bool) -> Self {
        self.attention = on;
        self
    }

    /// Draws the field as a search line (see [`search_field`]): the field is there to search, not to pick one
    /// of a few known entries.
    pub fn searching(mut self) -> Self {
        self.searching = true;
        self
    }

    /// Width of the field; default: all the room there is.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show<S: AsRef<str>>(self, ui: &mut Ui, text: &mut String, entries: &[S]) -> SuggestOutput {
        let scheme = self.scheme;
        let id = ui.make_persistent_id(self.id_salt);
        let mut state: SuggestState = ui.data(|d| d.get_temp(id)).unwrap_or_default();

        // Keys for the list are taken before the text field sees them.
        let (mut down, mut up, mut accept, mut dismiss) = (0, 0, false, false);
        if state.open {
            ui.input_mut(|input| {
                down = take_key(input, egui::Key::ArrowDown);
                up = take_key(input, egui::Key::ArrowUp);
                accept = take_key(input, egui::Key::Enter) + take_key(input, egui::Key::Tab) > 0;
                dismiss = take_key(input, egui::Key::Escape) > 0;
            });
        }

        // A click into the list must not take the cursor out of the field.
        let popup_id = id.with("suggestions");
        let popup_layer = egui::LayerId::new(egui::PopupKind::Popup.order(), popup_id);
        let ctx = ui.ctx().clone();
        let on_popup =
            state.open && ctx.pointer_interact_pos().and_then(|pos| ctx.layer_id_at(pos)) == Some(popup_layer);
        let surrender = ctx.options(|o| o.input_options.surrender_focus_on);
        if on_popup {
            ctx.options_mut(|o| o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never);
        }
        let margin = if self.searching { search_margin() } else { clearable_margin() };
        let edit = text_field(text, self.hint, scheme).id(id).margin(margin).desired_width(self.width);
        // While the list is open, Tab and Esc act on the list, not on the focus.
        let filter = edit.get_event_filter();
        let now = egui::EventFilter { tab: filter.tab || state.open, escape: filter.escape || state.open, ..filter };
        let look = self.searching.then(|| SearchLook::begin(ui, scheme));
        let mut response = ui.add(edit.event_filter(now));
        if let Some(look) = look {
            look.end(ui);
            search_icon(ui, &response, scheme);
        }
        ctx.options_mut(|o| o.input_options.surrender_focus_on = surrender);
        if self.attention {
            let ring = Stroke::new(2.0, color(scheme.accent));
            ui.painter().rect_stroke(response.rect.expand(2.0), control_radius(scheme), ring, StrokeKind::Outside);
        }
        if clear_button(ui, &response, text, scheme) {
            response.mark_changed();
            state.closed_for = None;
        }

        if dismiss {
            state.closed_for = Some(text.clone());
        }
        let found = if state.closed_for.as_deref() == Some(text.as_str()) {
            crate::suggest::Suggestions::default()
        } else {
            crate::suggest::suggest(text, entries)
        };
        if state.query != *text {
            state.selected = 0;
            state.query = text.clone();
        }
        let count = found.hits.len();
        if count > 0 {
            state.selected = (state.selected + down + count - up % count) % count;
        }
        let mut picked_row = (accept && count > 0).then_some(state.selected);
        let is_open = response.has_focus() && count > 0 && picked_row.is_none();
        if response.has_focus() {
            // egui reads the filter at the start of the next frame, so a list
            // that opened in this frame already keeps Esc in the field.
            let next = egui::EventFilter { tab: filter.tab || is_open, escape: filter.escape || is_open, ..filter };
            ui.memory_mut(|m| m.set_focus_lock_filter(id, next));
        }

        let frame = Frame::new()
            .fill(color(scheme.field))
            .stroke(Stroke::new(1.0, color(scheme.accent)))
            .corner_radius(control_radius(scheme))
            .inner_margin(Margin::same(4));
        let mut clicked = None;
        let selected = &mut state.selected;
        egui::Popup::from_response(&response)
            .id(popup_id)
            .kind(egui::PopupKind::Popup)
            .open(is_open)
            .align(egui::RectAlign::BOTTOM_START)
            .align_alternatives(&[egui::RectAlign::TOP_START])
            .width(response.rect.width() - 10.0)
            .frame(frame)
            .show(|ui| {
                let moved = ui.input(|i| i.pointer.is_moving());
                for (row, hit) in found.hits.iter().enumerate() {
                    let entry = entries[hit.index].as_ref();
                    let line = suggestion_row(ui, scheme, entry, &hit.matched, row == *selected);
                    if line.hovered() && moved {
                        *selected = row;
                    }
                    if line.clicked() {
                        clicked = Some(row);
                    }
                }
                if found.more > 0 {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new((self.more_label)(found.more)).color(color(scheme.sub)));
                    });
                }
            });
        if clicked.is_some() {
            picked_row = clicked;
        }

        let picked = picked_row.and_then(|row| found.hits.get(row)).map(|hit| hit.index);
        if let Some(index) = picked {
            *text = entries[index].as_ref().to_string();
            state.closed_for = Some(text.clone());
            state.query = text.clone();
            state.selected = 0;
            let mut edit = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
            let end = egui::text::CCursor::new(text.chars().count());
            edit.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            edit.store(&ctx, id);
            response.mark_changed();
        }
        state.open = is_open;
        ui.data_mut(|d| d.insert_temp(id, state));
        SuggestOutput { response, picked, is_open }
    }
}

/// One suggestion: the typed part in bold, in the accent color unless the
/// row is chosen (then the chosen-row color carries it).
fn suggestion_row(ui: &mut Ui, scheme: &Scheme, entry: &str, matched: &Range<usize>, chosen: bool) -> Response {
    let size = TextStyle::Body.resolve(ui.style()).size;
    let width = ui.available_width();
    let plain = TextFormat::simple(FontId::proportional(size), color(scheme.fg));
    let strong = TextFormat::simple(crate::fonts::bold(size), color(if chosen { scheme.fg } else { scheme.accent }));
    let (start, end) = (matched.start.min(entry.len()), matched.end.min(entry.len()));
    let mut job = LayoutJob::default();
    job.wrap.max_width = (width - 12.0).max(1.0);
    job.append(&entry[..start], 0.0, plain.clone());
    job.append(&entry[start..end], 0.0, strong);
    job.append(&entry[end..], 0.0, plain);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(vec2(width, galley.size().y + 8.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if chosen {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.accent_dark));
        }
        let pos = pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color(scheme.fg));
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Searchable select field
// ---------------------------------------------------------------------------

/// What [`SelectField::show`] returns.
pub struct SelectOutput {
    pub response: Response,
    /// The entry picked this frame (Enter or a click), as its position in the entries.
    pub picked: Option<usize>,
    /// The X emptied the field this frame.
    pub cleared: bool,
    /// Is the list visible?
    pub is_open: bool,
}

#[derive(Clone, Default)]
struct SelectState {
    open: bool,
    selected: usize,
    /// The text when the list opened; closing it without a pick puts it back.
    before: String,
    /// The text of the last frame: a change opens the list or starts it at the first entry again.
    last: String,
    /// Something was typed since the list opened: only then the list is filtered.
    typed: bool,
}

/// Size of the arrow at the right edge of a select field.
pub const SELECT_ARROW: f32 = 14.0;

/// Inner margin of a select field: room for the X and the arrow at the right edge.
fn select_margin() -> Margin {
    Margin { left: 4, right: (4.0 + CLEAR_SIZE + 2.0 + SELECT_ARROW + 4.0) as i8, top: 2, bottom: 2 }
}

/// The entries that hold every word of `query`, ignoring case, in the order given.
pub fn select_matches<S: AsRef<str>>(query: &str, entries: &[S]) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let entry = entry.as_ref().to_lowercase();
            words.iter().all(|word| entry.contains(word.as_str()))
        })
        .map(|(index, _)| index)
        .collect()
}

/// A field that is a drop-down and a search in one: a click, the arrow at the right edge or
/// ↓ opens the whole list in the order given (what comes first, comes first: the stock part,
/// say); typing narrows it to the entries that hold every typed word. ↑ ↓ choose, Enter or a
/// click picks, Esc or a click elsewhere closes and puts the value back; the X empties the
/// field. The field is a combo box with `name` for screen readers, its rows are options.
pub struct SelectField<'a> {
    id_salt: &'a str,
    name: &'a str,
    hint: &'a str,
    empty: &'a str,
    scheme: &'a Scheme,
    width: f32,
}

impl<'a> SelectField<'a> {
    /// `name` is read out for the field, `hint` stands in it while it is empty, `empty` in the
    /// list when nothing matches.
    pub fn new(id_salt: &'a str, name: &'a str, hint: &'a str, empty: &'a str, scheme: &'a Scheme) -> Self {
        SelectField { id_salt, name, hint, empty, scheme, width: f32::INFINITY }
    }

    /// Width of the field; default: all the room there is.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show<S: AsRef<str>>(self, ui: &mut Ui, text: &mut String, entries: &[S]) -> SelectOutput {
        let scheme = self.scheme;
        let id = ui.make_persistent_id(self.id_salt);
        let mut state: SelectState = ui.data(|d| d.get_temp(id)).unwrap_or_default();

        // Keys for the list are taken before the text field sees them.
        let (mut down, mut up, mut accept, mut dismiss) = (0, 0, false, false);
        if state.open {
            ui.input_mut(|input| {
                down = take_key(input, egui::Key::ArrowDown);
                up = take_key(input, egui::Key::ArrowUp);
                accept = take_key(input, egui::Key::Enter) > 0;
                dismiss = take_key(input, egui::Key::Escape) > 0;
            });
        }

        // A click into the list must not take the cursor out of the field.
        let popup_id = id.with("list");
        let popup_layer = egui::LayerId::new(egui::PopupKind::Popup.order(), popup_id);
        let ctx = ui.ctx().clone();
        let on_popup =
            state.open && ctx.pointer_interact_pos().and_then(|pos| ctx.layer_id_at(pos)) == Some(popup_layer);
        let surrender = ctx.options(|o| o.input_options.surrender_focus_on);
        if on_popup {
            ctx.options_mut(|o| o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never);
        }
        let edit = text_field(text, self.hint, scheme).id(id).margin(select_margin()).desired_width(self.width);
        let filter = edit.get_event_filter();
        let now = egui::EventFilter { escape: filter.escape || state.open, ..filter };
        let mut response = ui.add(edit.event_filter(now));
        ctx.options_mut(|o| o.input_options.surrender_focus_on = surrender);

        // The arrow at the right edge, the X left of it.
        let arrow_rect = Rect::from_center_size(
            pos2(response.rect.right() - 4.0 - SELECT_ARROW / 2.0, response.rect.center().y),
            Vec2::splat(SELECT_ARROW),
        );
        let open_list =
            versekit_core::language::t(if state.open { "s_mu_liste_schliessen" } else { "s_mu_liste_oeffnen" });
        let arrow = ui
            .interact(arrow_rect, id.with("arrow"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(&open_list);
        let arrow_name = format!("{}: {open_list}", self.name);
        arrow.widget_info(|| WidgetInfo::labeled(Role::Button, true, &arrow_name));
        let tint = if arrow.hovered() { IconTint::Hover } else { IconTint::Normal };
        if let Some(image) = crate::icons::themed("chevron-down", scheme, tint, SELECT_ARROW) {
            image.paint_at(ui, arrow_rect);
        }
        let cleared = clear_button_at(ui, &response, text, scheme, SELECT_ARROW + 2.0);

        let pressed_down = !state.open && response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let typed_closed = !state.open && *text != state.last && response.has_focus();
        let open_now =
            !state.open && (response.clicked() || arrow.clicked() || pressed_down || typed_closed || cleared);
        if open_now {
            state.open = true;
            state.before = if typed_closed || cleared { state.last.clone() } else { text.clone() };
            state.typed = typed_closed || cleared;
            state.selected = entries.iter().position(|e| e.as_ref() == text.as_str()).unwrap_or(0);
            if !response.has_focus() {
                ui.memory_mut(|m| m.request_focus(id));
            }
        } else if state.open && arrow.clicked() {
            dismiss = true;
        }
        if state.open && *text != state.last {
            state.typed = true;
            state.selected = 0;
        }
        if cleared {
            response.mark_changed();
        }

        let shown: Vec<usize> = if state.typed { select_matches(text, entries) } else { (0..entries.len()).collect() };
        let count = shown.len();
        let moved = down > 0 || up > 0;
        if count > 0 {
            state.selected = (state.selected % count + down + count - up % count) % count;
        }
        let mut picked_row = (state.open && accept && count > 0).then_some(state.selected);
        // Leaving the field (Tab, a click elsewhere) closes the list like Esc.
        let left = state.open && !open_now && !response.has_focus() && !on_popup && !arrow.clicked();
        if (dismiss || left) && picked_row.is_none() {
            if *text != state.before {
                text.clone_from(&state.before);
                response.mark_changed();
            }
            state.open = false;
        }
        let is_open = state.open && picked_row.is_none();
        if response.has_focus() {
            let next = egui::EventFilter { escape: filter.escape || is_open, ..filter };
            ui.memory_mut(|m| m.set_focus_lock_filter(id, next));
        }

        let frame = Frame::new()
            .fill(color(scheme.field))
            .stroke(Stroke::new(1.0, color(scheme.accent)))
            .corner_radius(control_radius(scheme))
            .inner_margin(Margin::same(4));
        let mut clicked = None;
        let selected = &mut state.selected;
        let empty = self.empty;
        egui::Popup::from_response(&response)
            .id(popup_id)
            .kind(egui::PopupKind::Popup)
            .open(is_open)
            .align(egui::RectAlign::BOTTOM_START)
            .align_alternatives(&[egui::RectAlign::TOP_START])
            .width(response.rect.width() - 10.0)
            .frame(frame)
            .show(|ui| {
                if shown.is_empty() {
                    ui.label(RichText::new(empty).color(color(scheme.sub)));
                    return;
                }
                let pointer_moved = ui.input(|i| i.pointer.is_moving());
                let list = ScrollArea::vertical().id_salt(popup_id.with("scroll"));
                list.max_height(220.0).min_scrolled_height(220.0).show(ui, |ui| {
                    for (row, index) in shown.iter().enumerate() {
                        let line = select_row(ui, scheme, entries[*index].as_ref(), row == *selected);
                        if row == *selected && moved {
                            line.scroll_to_me(None);
                        }
                        if line.hovered() && pointer_moved {
                            *selected = row;
                        }
                        if line.clicked() {
                            clicked = Some(row);
                        }
                    }
                });
            });
        if clicked.is_some() {
            picked_row = clicked;
        }

        let picked = picked_row.and_then(|row| shown.get(row).copied());
        if let Some(index) = picked {
            *text = entries[index].as_ref().to_string();
            state.open = false;
            let mut edit = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
            let end = egui::text::CCursor::new(text.chars().count());
            edit.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            edit.store(&ctx, id);
            response.mark_changed();
        }
        state.last.clone_from(text);
        let (enabled, name, value) = (response.enabled(), self.name.to_string(), text.clone());
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::ComboBox, enabled, &name);
            info.current_text_value = Some(value.clone());
            info
        });
        // A value wider than the field shows in full on hover.
        let margin = select_margin();
        let room = response.rect.width() - f32::from(margin.left) - f32::from(margin.right);
        if !is_open && line_galley(ui, text, TextStyle::Body).size().x > room {
            response = response.on_hover_text(text.clone());
        }
        ui.data_mut(|d| d.insert_temp(id, state));
        SelectOutput { response, picked, cleared, is_open: is_open && picked.is_none() }
    }
}

/// One row of a [`SelectField`]: the chosen one filled with the chosen-row color.
fn select_row(ui: &mut Ui, scheme: &Scheme, entry: &str, chosen: bool) -> Response {
    let size = TextStyle::Body.resolve(ui.style()).size;
    let width = ui.available_width();
    let mut job = LayoutJob::default();
    job.wrap.max_width = (width - 12.0).max(1.0);
    job.append(entry, 0.0, TextFormat::simple(FontId::proportional(size), color(scheme.fg)));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(vec2(width, galley.size().y + 8.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if chosen {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.accent_dark));
        }
        let pos = pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color(scheme.fg));
    }
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::selected(Role::ListBoxOption, enabled, chosen, entry));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Lists, empty pages, dialogs
// ---------------------------------------------------------------------------

/// One line of text laid out in the body style without wrapping.
fn line_galley(ui: &Ui, text: &str, style: TextStyle) -> std::sync::Arc<egui::Galley> {
    WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, FontSelection::Style(style))
}

/// A round choice (one of several): ring in the border color; chosen or
/// hovered ring and dot in the accent.
pub fn radio(ui: &mut Ui, scheme: &Scheme, on: bool, text: &str) -> Response {
    radio_with(ui, scheme, on, text, None)
}

/// The flags of `assets/flaggen` (from „flag-icons“, MIT, see
/// `tools/flaggen-vorlagen/`), one file per line size as in 4.12.
const FLAG_SIZES: [u32; 4] = [12, 14, 16, 18];

/// The countries with a flag file: the languages of the translations.
#[cfg(test)]
const FLAG_CODES: [&str; 9] = ["br", "de", "es", "fr", "gb", "it", "nl", "pt", "tr"];

fn flag_png(code: &str, size: u32) -> Option<&'static [u8]> {
    macro_rules! flags {
        ($($code:literal),*) => {
            match (code, size) {
                $(
                    ($code, 12) => include_bytes!(concat!("../../../assets/flaggen/", $code, "-12.png")).as_slice(),
                    ($code, 14) => include_bytes!(concat!("../../../assets/flaggen/", $code, "-14.png")).as_slice(),
                    ($code, 16) => include_bytes!(concat!("../../../assets/flaggen/", $code, "-16.png")).as_slice(),
                    ($code, 18) => include_bytes!(concat!("../../../assets/flaggen/", $code, "-18.png")).as_slice(),
                )*
                _ => return None,
            }
        };
    }
    Some(flags!("br", "de", "es", "fr", "gb", "it", "nl", "pt", "tr"))
}

/// A flag in the size of the line (4:3), or `None` for a country without one.
/// The file is picked by the pixels the line has on the screen (as 4.12 with
/// `dpi.px`), so a zoomed or sharp screen gets a larger file, not a blurred one.
fn flag(ui: &Ui, code: &str) -> Option<(egui::Image<'static>, Vec2)> {
    let scale = ui.ctx().pixels_per_point();
    let text = TextStyle::Body.resolve(ui.style()).size * scale;
    let size = FLAG_SIZES.into_iter().min_by(|a, b| (*a as f32 - text).abs().total_cmp(&(*b as f32 - text).abs()))?;
    let bytes = flag_png(code, size)?;
    let pixels = size as f32;
    let image = egui::Image::from_bytes(format!("bytes://flaggen/{code}-{size}.png"), bytes);
    Some((image, vec2(pixels * 4.0 / 3.0, pixels) / scale))
}

/// [`radio`] with a flag before the text: the language choice, as in 4.12.
/// The flags are the only pictures besides the Lucide symbols; `code` names
/// the country (`de`, `gb`).
pub fn radio_flag(ui: &mut Ui, scheme: &Scheme, on: bool, text: &str, code: &str) -> Response {
    let picture = flag(ui, code);
    radio_with(ui, scheme, on, text, picture)
}

fn radio_with(
    ui: &mut Ui,
    scheme: &Scheme,
    on: bool,
    text: &str,
    picture: Option<(egui::Image<'static>, Vec2)>,
) -> Response {
    let galley = line_galley(ui, text, TextStyle::Body);
    let lead = picture.as_ref().map_or(0.0, |(_, size)| size.x + 6.0);
    let size = vec2(24.0 + lead + galley.size().x, galley.size().y.max(18.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let center = pos2(rect.min.x + 9.0, rect.center().y);
        let hovered = response.hovered();
        let ring = if on || hovered { scheme.accent } else { scheme.control_border() };
        // The mouse over it shows on a chosen one too: a thicker ring.
        let width = if hovered { 2.5 } else { 1.5 };
        ui.painter().circle(center, 8.0, color(scheme.control()), Stroke::new(width, color(ring)));
        if on {
            ui.painter().circle_filled(center, 4.0, color(scheme.accent));
        }
        if let Some((image, size)) = picture {
            let at = pos2(rect.min.x + 24.0, rect.center().y - size.y / 2.0);
            image.paint_at(ui, Rect::from_min_size(at, size));
        }
        let pos = pos2(rect.min.x + 24.0 + lead, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color(scheme.fg));
    }
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, on, text));
    focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Gives a drop-down without a label of its own a name for screen readers;
/// egui only tells them the picked `value`.
pub fn name_combo(response: &Response, name: &str, value: &str) {
    let enabled = response.enabled();
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::ComboBox, enabled, name);
        info.current_text_value = Some(value.to_owned());
        info
    });
}

/// A row with a label and a switch at the right edge (on/off is always a
/// switch, never a check box).
pub fn switch_row(ui: &mut Ui, scheme: &Scheme, label: &str, on: &mut bool) -> Response {
    switch_row_tinted(ui, scheme, label, on, None)
}

/// [`switch_row`] whose knob shows a state in `knob` (a colour of the scheme).
pub fn switch_row_tinted(
    ui: &mut Ui,
    scheme: &Scheme,
    label: &str,
    on: &mut bool,
    knob: Option<versekit_core::theme::Color>,
) -> Response {
    let (switch, row) = switch_line(ui, scheme, label, on, knob);
    crate::highlight::mark(ui, scheme, row, &[label]);
    switch
}

/// The line of a switch row: the switch response and the rectangle of the whole line. It does not frame
/// itself after a menu filter jump; the caller marks it together with what belongs to it.
pub fn switch_line(
    ui: &mut Ui,
    scheme: &Scheme,
    label: &str,
    on: &mut bool,
    knob: Option<versekit_core::theme::Color>,
) -> (Response, Rect) {
    let line = ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(Toggle::new(on, scheme).knob(knob))
        })
        .inner
    });
    let response = line.inner;
    let (enabled, now) = (response.enabled(), *on);
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, now, label));
    (response, line.response.rect)
}

/// [`switch_row`] with its help text in small muted text below: after a jump from the menu filter to the
/// label or to the help text the line and the help are framed together.
pub fn switch_row_help(ui: &mut Ui, scheme: &Scheme, label: &str, help: &str, on: &mut bool) -> Response {
    let (switch, row) = switch_line(ui, scheme, label, on, None);
    let note = hint_label(ui, scheme, help);
    crate::highlight::mark(ui, scheme, row.union(note.rect), &[label, help]);
    switch
}

/// A short explanation in small muted text, wrapped; after a jump from the menu filter to its text it frames
/// itself.
pub fn hint(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let note = hint_label(ui, scheme, text);
    crate::highlight::mark(ui, scheme, note.rect, &[text]);
    note
}

/// [`hint`] without the frame of a menu filter jump, for a caller that frames it together with more.
pub fn hint_label(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap())
}

/// A row of tabs: text, the chosen one in the accent with a line below,
/// a thin line under the whole row. Returns whether the choice changed.
pub fn tab_bar(ui: &mut Ui, scheme: &Scheme, tabs: &[&str], chosen: &mut usize) -> bool {
    let mut changed = false;
    let row = ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, text) in tabs.iter().enumerate() {
            if tab(ui, scheme, *chosen == i, text).clicked() && *chosen != i {
                *chosen = i;
                changed = true;
            }
        }
    });
    let rect = row.response.rect;
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, color(scheme.line)));
    changed
}

/// A rare action of a page (back up, read in, clear): shown with icon and
/// text at the right of the tab row; `on` while its panel is open.
pub struct PageAction<'a> {
    pub icon: &'a str,
    pub text: &'a str,
    pub on: bool,
}

/// A [`tab_bar`] with the page's rare actions at its right, always in
/// sight. Too narrow for
/// one line: the actions go to their own line below, still at the right,
/// and wrap when even that line is too narrow. Returns (tab changed, index
/// of the action clicked).
pub fn tab_bar_with_actions(
    ui: &mut Ui,
    scheme: &Scheme,
    tabs: &[&str],
    chosen: &mut usize,
    actions: &[PageAction<'_>],
) -> (bool, Option<usize>) {
    const GAP_X: f32 = 4.0;
    let tab_width: f32 = tabs.iter().map(|t| line_galley(ui, t, TextStyle::Button).size().x + 16.0 + GAP_X).sum();
    let kind_of = |a: &PageAction<'_>| if a.on { ButtonKind::Strong } else { ButtonKind::Normal };
    let action_width: f32 = actions.iter().map(|a| icon_button_size(ui, scheme, kind_of(a), a.text).x + GAP_X).sum();
    let one_line = tab_width + action_width + 16.0 <= ui.available_width();
    let wraps = action_width > ui.available_width();
    let mut changed = false;
    let mut clicked = None;
    let mut draw_actions = |ui: &mut Ui| {
        let mut add = |ui: &mut Ui, i: usize, a: &PageAction<'_>| {
            let response = ui.add(Button::new(a.text, kind_of(a), scheme).icon(a.icon));
            let enabled = ui.is_enabled();
            response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, a.on, a.text));
            if response.clicked() {
                clicked = Some(i);
            }
        };
        if wraps {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = GAP_X;
                for (i, a) in actions.iter().enumerate() {
                    add(ui, i, a);
                }
            });
        } else {
            ui.with_layout(Layout::right_to_left(Align::Center), |right| {
                right.spacing_mut().item_spacing.x = GAP_X;
                for (i, a) in actions.iter().enumerate().rev() {
                    add(right, i, a);
                }
            });
        }
    };
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP_X;
        for (i, text) in tabs.iter().enumerate() {
            if tab(ui, scheme, *chosen == i, text).clicked() && *chosen != i {
                *chosen = i;
                changed = true;
            }
        }
        if one_line {
            draw_actions(ui);
        }
    });
    let rect = row.response.rect;
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, color(scheme.line)));
    if !one_line {
        ui.add_space(4.0);
        if wraps {
            draw_actions(ui);
        } else {
            ui.horizontal(|ui| draw_actions(ui));
        }
    }
    (changed, clicked)
}

/// One tab of a [`tab_bar`].
pub fn tab(ui: &mut Ui, scheme: &Scheme, on: bool, text: &str) -> Response {
    let galley = line_galley(ui, text, TextStyle::Button);
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(16.0, 12.0), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::Tab, enabled, on, text));
    if ui.is_rect_visible(rect) {
        let text_color = if on {
            scheme.accent
        } else if response.hovered() {
            scheme.fg
        } else {
            scheme.sub
        };
        let pos = pos2(rect.center().x - galley.size().x / 2.0, rect.min.y + 4.0);
        ui.painter().galley(pos, galley, color(text_color));
        if on || response.hovered() {
            let line = if on { scheme.accent } else { scheme.line };
            ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(2.0, color(line)));
        }
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    crate::highlight::mark(ui, scheme, rect, &[text]);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Width of the accent bar at the left of a chosen list row.
pub const LIST_BAR_WIDTH: f32 = 3.0;

/// A list row over the whole width: chosen = dark accent area with an accent
/// bar on the left, hovered = hover area. Long texts end with an ellipsis.
pub fn list_row(ui: &mut Ui, scheme: &Scheme, on: bool, text: &str) -> Response {
    let galley = WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (ui.available_width() - 16.0).max(1.0),
        FontSelection::Style(TextStyle::Body),
    );
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y + 8.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if on {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.accent_dark));
            let bar = Rect::from_min_size(rect.min, vec2(LIST_BAR_WIDTH, rect.height()));
            ui.painter().rect_filled(bar, CornerRadius::ZERO, color(scheme.accent));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
        }
        let pos = pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color(scheme.fg));
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Height of the progress track.
pub const PROGRESS_HEIGHT: f32 = 8.0;

/// Progress as a thin track in the switch color, the done part in the
/// accent, the percentage next to it. `width` is the whole widget.
pub fn progress(ui: &mut Ui, scheme: &Scheme, fraction: f32, width: f32) -> Response {
    let fraction = fraction.clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        let label = format!("{} %", (fraction * 100.0).round() as i64);
        let label_width = line_galley(ui, "100 %", TextStyle::Body).size().x;
        let track_width = (width.min(ui.available_width()) - label_width - ui.spacing().item_spacing.x).max(20.0);
        let (rect, response) = ui.allocate_exact_size(vec2(track_width, PROGRESS_HEIGHT), Sense::hover());
        let radius = CornerRadius::same(if scheme.square { 0 } else { (PROGRESS_HEIGHT / 2.0) as u8 });
        ui.painter().rect_filled(rect, radius, color(scheme.track));
        let mut done = rect;
        done.set_width(rect.width() * fraction);
        ui.painter().rect_filled(done, radius, color(scheme.accent));
        ui.label(label);
        response
    })
    .inner
}

/// A collapsing section whose arrow is a Lucide chevron (right = closed,
/// down = open) instead of egui's triangle.
pub fn collapsing(title: &str) -> egui::CollapsingHeader {
    egui::CollapsingHeader::new(title).icon(|ui, openness, response| {
        let name = if openness > 0.5 { "chevron-down" } else { "chevron-right" };
        let size = response.rect.height().max(12.0) + 4.0;
        let scheme = versekit_core::theme::current();
        let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
        if let Some(image) = crate::icons::themed(name, scheme, tint, size) {
            image.paint_at(ui, Rect::from_center_size(response.rect.center(), Vec2::splat(size)));
        }
    })
}

/// A field whose input is wrong: red border, below it one sentence in red
/// that says what would be right. Without an error a normal field with the X.
pub fn checked_field(
    ui: &mut Ui,
    scheme: &Scheme,
    text: &mut String,
    hint: &str,
    width: f32,
    error: Option<&str>,
) -> Response {
    let response = ui
        .scope(|ui| {
            if error.is_some() {
                let red = Stroke::new(1.0, color(scheme.red));
                let w = &mut ui.visuals_mut().widgets;
                (w.inactive.bg_stroke, w.hovered.bg_stroke) = (red, red);
                ui.visuals_mut().selection.stroke.color = color(scheme.red);
            }
            clearable_field(ui, text, hint, scheme, width)
        })
        .inner;
    if let Some(error) = error {
        ui.label(RichText::new(error).color(color(scheme.red)));
    }
    response
}

/// A column head of a table: bold, clickable; the sorted column shows a
/// Lucide arrow for the direction. Number columns are right-aligned.
/// Returns whether it was clicked.
pub fn sort_head(ui: &mut Ui, scheme: &Scheme, text: &str, sorted: Option<bool>, numbers: bool) -> bool {
    let add = |ui: &mut Ui| {
        let label = ui.add(egui::Label::new(RichText::new(text).strong()).sense(Sense::click()));
        crate::highlight::mark(ui, scheme, label.rect, &[text]);
        if let Some(ascending) = sorted {
            let arrow = if ascending { "chevron-up" } else { "chevron-down" };
            if let Some(image) = crate::icons::themed(arrow, scheme, IconTint::Highlight, 14.0) {
                ui.add(image);
            }
        }
        label.on_hover_cursor(CursorIcon::PointingHand).clicked()
    };
    if numbers { ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner } else { add(ui) }
}

/// A table cell; numbers stand right-aligned.
pub fn cell(ui: &mut Ui, text: &str, numbers: bool) {
    if numbers {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(text));
    } else {
        ui.label(text);
    }
}

/// How long a notice stays at the top right.
pub const NOTICE_SECONDS: f64 = 6.0;

/// What [`notice`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    /// Is it still shown?
    pub shown: bool,
    /// Was its action button clicked this frame?
    pub acted: bool,
}

/// A notice at the top right of the window in a box with an icon, for
/// [`NOTICE_SECONDS`] after `since` (`ui.input(|i| i.time)` when it came),
/// optionally with one button (after deleting: undo).
pub fn notice(
    ctx: &egui::Context,
    scheme: &Scheme,
    since: f64,
    icon: &str,
    text: &str,
    action: Option<&str>,
) -> Notice {
    let age = ctx.input(|i| i.time) - since;
    if !(0.0..NOTICE_SECONDS).contains(&age) {
        return Notice { shown: false, acted: false };
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(NOTICE_SECONDS - age));
    let mut acted = false;
    egui::Area::new(egui::Id::unique(("versekit-notice", text)))
        .anchor(egui::Align2::RIGHT_TOP, vec2(-16.0, 16.0))
        .order(egui::Order::Foreground)
        .interactable(action.is_some())
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            card(ui, scheme, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some(image) = crate::icons::themed(icon, scheme, IconTint::Highlight, 18.0) {
                        ui.add(image);
                    }
                    ui.label(text);
                    if let Some(label) = action {
                        acted = ui.add(Button::new(label, ButtonKind::Normal, scheme)).clicked();
                    }
                });
            });
        });
    Notice { shown: !acted, acted }
}

/// Seconds between two tries when loading failed.
pub const RETRY_SECONDS: f64 = 30.0;

/// Seconds until the next try, counted from the failure at `failed_at`
/// (`ui.input(|i| i.time)`); 0 means: try now. Never below 0, never above
/// [`RETRY_SECONDS`].
pub fn retry_in(failed_at: f64, now: f64) -> f64 {
    (RETRY_SECONDS - (now - failed_at)).clamp(0.0, RETRY_SECONDS)
}

/// When loading failed: a box with a warning icon, the reason and when the
/// program tries again by itself (no button); `status` is the countdown text.
/// Repaints so the count goes on.
pub fn load_failed(ui: &mut Ui, scheme: &Scheme, reason: &str, status: &str) -> Response {
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    list_frame(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            if let Some(image) = crate::icons::image("shield-alert", scheme.yellow, 24.0) {
                ui.add(image);
            }
            ui.label(reason);
            ui.label(RichText::new(status).color(color(scheme.sub)));
            ui.add_space(6.0);
        });
    })
    .response
}

/// Side of a button that only shows an icon.
pub const ICON_BUTTON_SIZE: f32 = 26.0;

/// A square button with only an icon; what it does always shows on hover.
pub fn icon_button(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::click());
    // Screen readers name the button by what it does.
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, meaning));
    if ui.is_rect_visible(rect) {
        let colors = button_colors(scheme, ButtonKind::Normal, response.hovered());
        let stroke = Stroke::new(1.0, color(colors.border));
        ui.painter().rect(rect.shrink(1.0), control_radius(scheme), color(colors.fill), stroke, StrokeKind::Inside);
        let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
        if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
        }
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(meaning)
}

/// The pin next to a page heading (like 4.12): pins the page to the
/// overview or loosens it again. While pinned it shows `pin-off` in the
/// accent. `hints` are (pin, loosen).
pub fn pin_toggle(ui: &mut Ui, scheme: &Scheme, pinned: bool, hints: (&str, &str)) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::click());
    if ui.is_rect_visible(rect) {
        let icon = if pinned { "pin-off" } else { "pin" };
        let tint = if pinned {
            scheme.accent
        } else if response.hovered() {
            scheme.icon(IconTint::Hover)
        } else {
            scheme.icon(IconTint::Normal)
        };
        if let Some(image) = crate::icons::image(icon, tint, 18.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.0)));
        }
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    let hint = if pinned { hints.1 } else { hints.0 };
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint)
}

/// The grip at the left of a row that can be dragged to another place (`grip-vertical`, the hand that grabs
/// while the mouse is over it and closes while it drags). The response tells `dragged()` and `drag_stopped()`;
/// the order is changed by the caller, who also offers the same by buttons.
pub fn drag_handle(ui: &mut Ui, scheme: &Scheme, meaning: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::drag());
    if ui.is_rect_visible(rect) {
        let active = response.hovered() || response.dragged();
        let tint = if active { scheme.icon(IconTint::Hover) } else { scheme.icon(IconTint::Normal) };
        if let Some(image) = crate::icons::image("grip-vertical", tint, 16.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
        }
    }
    if response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    response.on_hover_cursor(CursorIcon::Grab).on_hover_text(meaning)
}

/// A row with a text and, while the mouse is over the row, icon buttons at
/// its right edge instead of a right-click menu.
/// `actions` are (icon, meaning); returns the index of the one clicked.
pub fn action_row(ui: &mut Ui, scheme: &Scheme, text: &str, actions: &[(&str, &str)]) -> Option<usize> {
    let height = ICON_BUTTON_SIZE + 4.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
    }
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 2.0))));
    row.horizontal_centered(|ui| {
        ui.add(egui::Label::new(text).truncate());
    });
    if !hovered {
        return None;
    }
    let mut clicked = None;
    let mut buttons = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(4.0, 2.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    for (i, (icon, meaning)) in actions.iter().enumerate().rev() {
        if icon_button(&mut buttons, scheme, icon, meaning).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// While something loads: a spinner in the accent and a short text saying
/// what loads.
pub fn loading(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().color(color(scheme.accent)));
        ui.label(text);
    })
    .response
}

// ---------------------------------------------------------------------------
// Window frame: navigation and status bar
// ---------------------------------------------------------------------------

/// Height of one navigation entry.
pub const NAV_ROW_HEIGHT: f32 = 28.0;

/// Height of a group head in the navigation: small text, little room.
pub const NAV_GROUP_HEIGHT: f32 = 18.0;

fn nav_section_id(title: &str) -> egui::Id {
    egui::Id::unique(("versekit-nav-section", title))
}

/// Opens or folds a group of the navigation from elsewhere: a tile on the
/// overview folds the group of its page, so only that page shows in it.
pub fn set_nav_section_open(ctx: &egui::Context, title: &str, open: bool) {
    ctx.data_mut(|d| d.insert_temp(nav_section_id(title), open));
}

/// Whether the group `title` of the navigation is open (open at first).
pub fn nav_section_open(ctx: &egui::Context, title: &str) -> bool {
    ctx.data(|d| d.get_temp::<bool>(nav_section_id(title))).unwrap_or(true)
}

/// A group of the navigation that folds (like 4.x): a click on the name
/// opens or closes it in [`REVEAL_SECONDS`]; a Lucide arrow at the right
/// shows which. Open at first. `filtering`: the menu filter is in use, the
/// group shows its matches even when folded. `holds_page`: the open page is
/// in this group, so a folded group still shows that one entry. `add` gets whether the group is fully open and then draws
/// all entries, otherwise only the open page. Returns its result while
/// anything shows.
pub fn nav_section<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    title: &str,
    filtering: bool,
    holds_page: bool,
    add: impl FnOnce(&mut Ui, bool) -> R,
) -> NavSection<R> {
    let id = nav_section_id(title);
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    ui.add_space(2.0);
    // Click folds; dragging the head moves the whole group (click and drag,
    // so a click never starts a drag).
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), NAV_GROUP_HEIGHT), Sense::click_and_drag());
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    if ui.is_rect_visible(rect) {
        let text_color = if response.hovered() { scheme.fg } else { scheme.sub };
        let galley =
            ui.painter().layout_no_wrap(title.to_uppercase(), TextStyle::Small.resolve(ui.style()), color(text_color));
        ui.painter().galley(pos2(rect.min.x + 4.0, rect.center().y - galley.size().y / 2.0), galley, color(text_color));
        let arrow = if open || filtering { "chevron-down" } else { "chevron-right" };
        let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
        let icon_rect = Rect::from_center_size(pos2(rect.right() - 12.0, rect.center().y), Vec2::splat(12.0));
        if let Some(image) = crate::icons::themed(arrow, scheme, tint, 12.0) {
            image.paint_at(ui, icon_rect);
        }
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    let cursor = if response.dragged() { CursorIcon::Grabbing } else { CursorIcon::PointingHand };
    let head = response.on_hover_cursor(cursor);
    let full = open || filtering;
    let inner = reveal(ui, &format!("nav-section-{title}"), full || holds_page, |ui| add(ui, full));
    NavSection { head, inner }
}

/// What [`nav_section`] returns: the head (for dragging the group) and the
/// result of `add` while the group shows.
pub struct NavSection<R> {
    pub head: Response,
    pub inner: Option<R>,
}

/// Moves the item at `from` so that it stands before the item that was at
/// `before` (`before` = the length: to the end), for lists one reorders by
/// dragging, like the groups of the navigation.
pub fn move_before<T>(list: &mut Vec<T>, from: usize, before: usize) {
    if from >= list.len() || before > list.len() || before == from || before == from + 1 {
        return;
    }
    let item = list.remove(from);
    let at = if before > from { before - 1 } else { before };
    list.insert(at, item);
}

/// One page in the navigation at the left: icon and name; the chosen page
/// like a chosen list row. `new` is the text of the new sign while the page
/// has not been opened yet.
pub fn nav_entry(ui: &mut Ui, scheme: &Scheme, icon: &str, page: &str, on: bool, new: Option<&str>) -> Response {
    let tint = if on { IconTint::Highlight } else { IconTint::Normal };
    nav_row(ui, scheme, page, on, new, (0.0, 0.0), |ui, rect| {
        if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
            image.paint_at(ui, rect);
        }
    })
}

/// A page nested under a fixed entry of the navigation: like [`nav_entry`],
/// pushed in from the left.
pub fn nav_sub_entry(ui: &mut Ui, scheme: &Scheme, icon: &str, page: &str, on: bool) -> Response {
    let tint = if on { IconTint::Highlight } else { IconTint::Normal };
    nav_row(ui, scheme, page, on, None, (NAV_SUB_INDENT, 0.0), |ui, rect| {
        if let Some(image) = crate::icons::themed(icon, scheme, tint, 16.0) {
            image.paint_at(ui, rect);
        }
    })
}

/// A fixed entry of the navigation with a picture instead of an icon and a
/// chevron at the right that shows whether its pages are open.
pub fn nav_picture_entry(
    ui: &mut Ui,
    scheme: &Scheme,
    picture: egui::Image<'static>,
    name: &str,
    on: bool,
    open: bool,
) -> Response {
    // The name stops before the chevron at the right.
    let response = nav_row(ui, scheme, name, on, None, (0.0, NAV_CHEVRON + 16.0), |ui, rect| {
        // A picture needs more room than an icon: it is drawn around the same center.
        let wanted = Rect::from_center_size(rect.center(), Vec2::splat(NAV_PICTURE));
        picture.fit_to_exact_size(wanted.size()).paint_at(ui, wanted);
    });
    let chevron = if open { "chevron-down" } else { "chevron-right" };
    let spot =
        Rect::from_center_size(pos2(response.rect.right() - 14.0, response.rect.center().y), Vec2::splat(NAV_CHEVRON));
    if ui.is_rect_visible(response.rect)
        && let Some(image) = crate::icons::themed(chevron, scheme, IconTint::Normal, NAV_CHEVRON)
    {
        image.paint_at(ui, spot);
    }
    response
}

/// Indent of a page nested under a fixed entry of the navigation.
const NAV_SUB_INDENT: f32 = 14.0;
/// Size of the chevron of a fixed entry.
const NAV_CHEVRON: f32 = 14.0;
/// Size of the picture of a fixed entry.
const NAV_PICTURE: f32 = 22.0;

/// One row of the navigation: the chosen look, the hover and the label; the
/// icon (a square 16 points wide, pushed in by `indent`) is painted by `paint_icon`.
fn nav_row(
    ui: &mut Ui,
    scheme: &Scheme,
    page: &str,
    on: bool,
    new: Option<&str>,
    (indent, reserve): (f32, f32),
    paint_icon: impl FnOnce(&Ui, Rect),
) -> Response {
    // A name too long for one line (a hit of the menu filter, "Page › Setting") wraps and the row grows: nothing
    // is cut. With the new sign the name keeps one line and is cut, whole on hover.
    let room = ui.available_width() - (10.0 + indent + 16.0 + 8.0) - (6.0 + reserve);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let wrapped = ui.fonts_mut(|f| f.layout(page.to_string(), font, color(scheme.fg), room.max(20.0)));
    let wraps = new.is_none() && wrapped.rows.len() > 1;
    let height = if wraps { NAV_ROW_HEIGHT.max(wrapped.size().y + 10.0) } else { NAV_ROW_HEIGHT };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, on, page));
    if ui.is_rect_visible(rect) {
        if on {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.accent_dark));
            let bar = Rect::from_min_size(rect.min, vec2(LIST_BAR_WIDTH, rect.height()));
            ui.painter().rect_filled(bar, CornerRadius::ZERO, color(scheme.accent));
        } else if response.hovered() {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
        }
        let icon_rect = Rect::from_min_size(pos2(rect.min.x + 10.0 + indent, rect.center().y - 8.0), Vec2::splat(16.0));
        paint_icon(ui, icon_rect);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(icon_rect.right() + 8.0, rect.top()),
                    rect.max - vec2(6.0 + reserve, 0.0),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let label = egui::Label::new(RichText::new(page).color(color(scheme.fg))).selectable(false);
        row.add(if wraps { label.wrap() } else { label.truncate() });
        if let Some(text) = new {
            new_badge(&mut row, scheme, text);
        }
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let cut = new.is_some() && wrapped.rows.len() > 1;
    if cut { response.on_hover_text(page) } else { response }
}

/// The new sign: small, filled with the accent,
/// bold dark text. It goes once the area has been opened.
pub fn new_badge(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), crate::fonts::bold(10.0), color(scheme.bg));
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
    let radius = CornerRadius::same(if scheme.square { 0 } else { 7 });
    ui.painter().rect_filled(rect, radius, color(scheme.accent));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color(scheme.bg));
    response
}

/// The pages visited, for going back and forward (like 4.x). The same page twice in a row counts once; a new page
/// after going back drops what lay ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    pages: Vec<String>,
    at: usize,
}

impl History {
    /// A history that starts at `page`.
    pub fn new(page: &str) -> Self {
        History { pages: vec![page.to_string()], at: 0 }
    }

    pub fn current(&self) -> &str {
        &self.pages[self.at]
    }

    /// Notes that `page` was opened.
    pub fn visit(&mut self, page: &str) {
        if self.current() == page {
            return;
        }
        self.pages.truncate(self.at + 1);
        self.pages.push(page.to_string());
        self.at += 1;
    }

    pub fn can_go_back(&self) -> bool {
        self.at > 0
    }

    /// The page a step back would show.
    pub fn previous(&self) -> Option<&str> {
        self.can_go_back().then(|| self.pages[self.at - 1].as_str())
    }

    pub fn can_go_forward(&self) -> bool {
        self.at + 1 < self.pages.len()
    }

    /// Goes one page back; returns the page to show.
    pub fn back(&mut self) -> Option<&str> {
        self.can_go_back().then(|| {
            self.at -= 1;
            self.pages[self.at].as_str()
        })
    }

    /// Goes one page forward; returns the page to show.
    pub fn forward(&mut self) -> Option<&str> {
        self.can_go_forward().then(|| {
            self.at += 1;
            self.pages[self.at].as_str()
        })
    }
}

/// Texts of the back and forward arrows: (back, forward, nothing back,
/// nothing forward), from the language file.
pub struct HistoryTexts<'a> {
    pub back: &'a str,
    pub forward: &'a str,
    pub no_back: &'a str,
    pub no_forward: &'a str,
}

/// The back and forward arrows at the left of a page head, and the same by
/// Alt+← / Alt+→ (not while typing in a field, where on the Mac Alt+← moves
/// by word) and the side buttons of the mouse. Returns the page to show
/// when one of them was used.
pub fn history_buttons(ui: &mut Ui, scheme: &Scheme, history: &mut History, texts: &HistoryTexts) -> Option<String> {
    match history_buttons_inner(ui, scheme, history, texts, None) {
        Some(HistoryStep::Page(page)) => Some(page),
        _ => None,
    }
}

/// Where a step of [`history_buttons_inner`] leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryStep {
    /// To this page.
    Page(String),
    /// Back inside the page: what it shows on top (a detail view) closes.
    Inner,
}

/// [`history_buttons`] for a page that shows something on top of its list (`inner` names it for the hint):
/// a step back closes that first and the history stays where it is.
pub fn history_buttons_inner(
    ui: &mut Ui,
    scheme: &Scheme,
    history: &mut History,
    texts: &HistoryTexts,
    inner: Option<&str>,
) -> Option<HistoryStep> {
    let typing = ui.ctx().text_edit_focused();
    let (key_back, key_forward, mouse_back, mouse_forward) = ui.input_mut(|i| {
        let alt = egui::Modifiers::ALT;
        (
            !typing && i.consume_key(alt, egui::Key::ArrowLeft),
            !typing && i.consume_key(alt, egui::Key::ArrowRight),
            i.pointer.button_pressed(egui::PointerButton::Extra1),
            i.pointer.button_pressed(egui::PointerButton::Extra2),
        )
    });
    let mut back = key_back || mouse_back;
    let mut forward = key_forward || mouse_forward;
    let can = (history.can_go_back() || inner.is_some(), history.can_go_forward());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let hint_back = inner.unwrap_or(if can.0 { texts.back } else { texts.no_back });
        back |= arrow_button(ui, scheme, "arrow-left", hint_back, can.0).clicked();
        let hint_forward = if can.1 { texts.forward } else { texts.no_forward };
        forward |= arrow_button(ui, scheme, "arrow-right", hint_forward, can.1).clicked();
    });
    if back {
        if inner.is_some() {
            return Some(HistoryStep::Inner);
        }
        return history.back().map(|page| HistoryStep::Page(page.to_string()));
    }
    if forward {
        return history.forward().map(|page| HistoryStep::Page(page.to_string()));
    }
    None
}

/// An icon button that may be greyed out; the hint explains either way.
fn arrow_button(ui: &mut Ui, scheme: &Scheme, icon: &str, hint: &str, enabled: bool) -> Response {
    if enabled {
        return icon_button(ui, scheme, icon, hint);
    }
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON_SIZE), Sense::hover());
    let colors = blocked_colors(scheme);
    let stroke = Stroke::new(1.0, color(colors.border));
    ui.painter().rect(rect.shrink(1.0), control_radius(scheme), color(colors.fill), stroke, StrokeKind::Inside);
    if let Some(image) = crate::icons::image(icon, scheme.line, 16.0) {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
    }
    response.on_hover_cursor(CursorIcon::NotAllowed).on_hover_text(hint)
}

/// Height of the head and the foot bar of the window.
pub const BAR_HEIGHT: f32 = 30.0;
/// Size of the symbol in a [`bar_action`].
const BAR_ICON: f32 = 14.0;

/// How a [`bar_action`] calls for attention: not at all; pale red while
/// nothing is wrong (so the report button is still found at once); strong
/// red once there is something to report; or in the accent while something
/// new waits (a new version, like 4.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alarm {
    No,
    Quiet,
    Loud,
    News,
}

impl From<bool> for Alarm {
    fn from(alarm: bool) -> Self {
        if alarm { Alarm::Loud } else { Alarm::No }
    }
}

/// The pale red of [`Alarm::Quiet`]: halfway between red and the muted text.
pub fn quiet_red(scheme: &Scheme) -> Color {
    scheme.red.mix(scheme.sub, 0.5)
}

/// A symbol with its word in the head or foot bar, like 4.12 (a symbol alone
/// is not self-explanatory): muted at rest, in the accent under the mouse;
/// `alarm` keeps it red, pale or strong. `hint` shows on hover.
pub fn bar_action(
    ui: &mut Ui,
    scheme: &Scheme,
    icon: &str,
    text: &str,
    hint: &str,
    alarm: impl Into<Alarm>,
) -> Response {
    let alarm = alarm.into();
    let font = TextStyle::Small.resolve(ui.style());
    let word = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size();
    let size = vec2(6.0 + BAR_ICON + 5.0 + word.x + 6.0, BAR_HEIGHT - 8.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let tone = if alarm == Alarm::Loud {
            scheme.red
        } else if alarm == Alarm::Quiet && !response.hovered() {
            quiet_red(scheme)
        } else if response.hovered() || alarm == Alarm::News {
            scheme.accent
        } else {
            scheme.sub
        };
        if response.hovered() {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
        }
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 6.0 + BAR_ICON / 2.0, rect.center().y), Vec2::splat(BAR_ICON));
        if let Some(image) = crate::icons::image(icon, tone, BAR_ICON) {
            image.paint_at(ui, icon_rect);
        }
        let galley = ui.painter().layout_no_wrap(text.to_string(), font, color(tone));
        ui.painter().galley(
            pos2(icon_rect.right() + 5.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color(tone),
        );
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint)
}

/// Something loading right now, for the foot bar: what (with its source,
/// source) and how far, 0 … 1 (`None` while unknown).
pub struct Activity<'a> {
    pub text: &'a str,
    pub done: Option<f32>,
}

/// Width of the progress track in the foot bar.
const FOOT_TRACK: f32 = 90.0;

/// The foot bar of the window (like 4.12). At the left what
/// is going on: a running download or data being loaded, with its progress,
/// otherwise `idle` (game state, short replies of the pages). At the right
/// the `actions`, drawn right to left. Returns the bar and where the status
/// at the left ended.
pub fn foot_bar(
    ui: &mut Ui,
    scheme: &Scheme,
    idle: &str,
    activity: Option<&Activity>,
    actions: impl FnOnce(&mut Ui),
) -> (Response, Rect) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, color(scheme.bar));
    let inner = rect.shrink2(vec2(8.0, 0.0));
    let mut right = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center)));
    actions(&mut right);
    let used = right.min_rect();
    let left_end = if used.width() > 0.0 { used.left() - 12.0 } else { inner.right() };
    let left_rect = Rect::from_min_max(inner.min, pos2(left_end.max(inner.left()), inner.bottom()));
    let mut left =
        ui.new_child(egui::UiBuilder::new().max_rect(left_rect).layout(Layout::left_to_right(Align::Center)));
    left.set_clip_rect(left_rect.intersect(ui.clip_rect()));
    match activity {
        Some(activity) => {
            if let Some(image) = crate::icons::image("download", scheme.accent, BAR_ICON) {
                left.add(image);
            }
            if let Some(done) = activity.done {
                let done = done.clamp(0.0, 1.0);
                let (track, _) = left.allocate_exact_size(vec2(FOOT_TRACK, 4.0), Sense::hover());
                let radius = CornerRadius::same(if scheme.square { 0 } else { 2 });
                left.painter().rect_filled(track, radius, color(scheme.track));
                let mut filled = track;
                filled.set_width(track.width() * done);
                left.painter().rect_filled(filled, radius, color(scheme.accent));
                left.label(
                    RichText::new(format!("{} %", (done * 100.0).round() as i64)).small().color(color(scheme.fg)),
                );
            }
            left.add(egui::Label::new(RichText::new(activity.text).small().color(color(scheme.fg))).truncate());
        }
        None => {
            left.add(egui::Label::new(RichText::new(idle).small().color(color(scheme.sub))).truncate());
        }
    }
    (response, left.min_rect())
}

// ---------------------------------------------------------------------------
// Saved, charts, choosing several, motion
// ---------------------------------------------------------------------------

/// How long the saved mark stays next to a setting.
pub const SAVED_SECONDS: f64 = 2.0;

/// Settings take effect at once; after a change a
/// check mark and `text` show for [`SAVED_SECONDS`].
/// `saved_at` is `ui.input(|i| i.time)` of the change. Returns whether it shows.
pub fn saved_hint(ui: &mut Ui, scheme: &Scheme, saved_at: Option<f64>, text: &str) -> bool {
    let now = ui.input(|i| i.time);
    let Some(at) = saved_at.filter(|at| (0.0..SAVED_SECONDS).contains(&(now - at))) else { return false };
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SAVED_SECONDS - (now - at)));
    ui.horizontal(|ui| {
        if let Some(image) = crate::icons::themed("circle-check", scheme, IconTint::Highlight, 14.0) {
            ui.add(image);
        }
        ui.label(RichText::new(text).color(color(scheme.sub)));
    });
    true
}

/// Usual height of a line chart including the labels.
pub const CHART_HEIGHT: f32 = 130.0;
/// Lowest height at which a line chart is still readable.
pub const CHART_MIN_HEIGHT: f32 = 70.0;

/// A course as a line with points in the accent, faint
/// guide lines, muted axes; the mouse on a point shows `value(v)`. `points`
/// are (label below, value).
///
/// `size` is the room it takes: the width at most what is there, the height
/// as given; in a tile the room that is left ([`CHART_MIN_HEIGHT`] at least,
/// below that a chart is not readable and the caller leaves it out).
pub fn line_chart(
    ui: &mut Ui,
    scheme: &Scheme,
    points: &[(&str, f32)],
    size: Vec2,
    value: &dyn Fn(f32) -> String,
) -> Response {
    let size = vec2(size.x.min(ui.available_width()), size.y.max(CHART_MIN_HEIGHT));
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let plot = Rect::from_min_max(rect.min + vec2(34.0, 6.0), rect.max - vec2(6.0, 20.0));
    let highest = points.iter().map(|(_, v)| *v).fold(f32::EPSILON, f32::max);
    let painter = ui.painter().clone();
    let small = egui::FontId::proportional(11.0);
    for step in 0..=2 {
        let y = plot.bottom() - plot.height() * step as f32 / 2.0;
        let line = if step == 0 { scheme.sub } else { scheme.line };
        painter.hline(plot.x_range(), y, Stroke::new(1.0, color(line)));
        let label = value(highest * step as f32 / 2.0);
        painter.text(pos2(plot.left() - 4.0, y), egui::Align2::RIGHT_CENTER, label, small.clone(), color(scheme.sub));
    }
    let slot = plot.width() / points.len().max(1) as f32;
    let mut line = Vec::with_capacity(points.len());
    let mut hovered_point = None;
    for (i, (label, v)) in points.iter().enumerate() {
        let x = plot.left() + slot * (i as f32 + 0.5);
        let y = plot.bottom() - plot.height() * v / highest;
        painter.text(pos2(x, plot.bottom() + 4.0), egui::Align2::CENTER_TOP, *label, small.clone(), color(scheme.sub));
        let hit = Rect::from_min_max(pos2(x - slot / 2.0, plot.top()), pos2(x + slot / 2.0, plot.bottom()));
        let point = ui.interact(hit, response.id.with(i), Sense::hover());
        if point.hovered() {
            hovered_point = Some(i);
        }
        point.on_hover_text(format!("{label}: {}", value(*v)));
        line.push(pos2(x, y));
    }
    painter.line(line.clone(), Stroke::new(2.0, color(scheme.accent)));
    for (i, p) in line.into_iter().enumerate() {
        let radius = if hovered_point == Some(i) { 5.0 } else { 3.5 };
        painter.circle_filled(p, radius, color(scheme.accent));
    }
    response
}

/// Which rows of a list are chosen: a click chooses one
/// row, Ctrl (Mac: Cmd) adds or removes one, Shift chooses the range from
/// the last click.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    chosen: Vec<bool>,
    last: Option<usize>,
}

impl Selection {
    pub fn new(rows: usize) -> Self {
        Selection { chosen: vec![false; rows], last: None }
    }

    pub fn click(&mut self, row: usize, modifiers: egui::Modifiers) {
        if row >= self.chosen.len() {
            return;
        }
        if modifiers.shift
            && let Some(last) = self.last
        {
            let range = last.min(row)..=last.max(row);
            self.chosen.iter_mut().enumerate().for_each(|(k, c)| *c = range.contains(&k));
            return;
        }
        if modifiers.command || modifiers.ctrl {
            self.chosen[row] = !self.chosen[row];
        } else {
            self.chosen.iter_mut().enumerate().for_each(|(k, c)| *c = k == row);
        }
        self.last = Some(row);
    }

    pub fn is_chosen(&self, row: usize) -> bool {
        self.chosen.get(row).copied().unwrap_or(false)
    }

    pub fn count(&self) -> usize {
        self.chosen.iter().filter(|c| **c).count()
    }

    pub fn clear(&mut self) {
        self.chosen.iter_mut().for_each(|c| *c = false);
        self.last = None;
    }
}

/// How long showing and hiding takes.
pub const REVEAL_SECONDS: f32 = 0.25;

/// Shows `add` while `open`, fading in and growing to its height in
/// [`REVEAL_SECONDS`], and shrinking and fading out when closed.
/// Returns the result while anything shows.
///
/// egui's own collapsing animation takes its duration from the whole
/// window's style, so it cannot be set for this alone.
pub fn reveal<R>(ui: &mut Ui, id_salt: &str, open: bool, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    reveal_for(ui, id_salt, open, REVEAL_SECONDS, add)
}

/// [`reveal`] with its own duration in seconds.
pub fn reveal_for<R>(
    ui: &mut Ui,
    id_salt: &str,
    open: bool,
    seconds: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let id = ui.make_persistent_id(id_salt);
    let shown = ui.ctx().animate_bool_with_time(id, open, seconds);
    if shown <= 0.0 {
        return None;
    }
    let height_id = id.with("height");
    let full: Option<f32> = ui.data(|d| d.get_temp(height_id));
    // The full height is known only after the first drawing: until then
    // nothing shows, so the box never jumps to full size.
    let visible = match full {
        None => 0.0,
        Some(height) if shown < 1.0 => height * shown,
        Some(_) => f32::INFINITY,
    };
    let top = ui.cursor().min;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap()));
    child.multiply_opacity(shown);
    let mut clip = child.clip_rect();
    clip.max.y = clip.max.y.min(top.y + visible);
    child.set_clip_rect(clip);
    let inner = add(&mut child);
    let used = child.min_rect();
    ui.data_mut(|d| d.insert_temp(height_id, used.height()));
    let height = used.height().min(visible);
    ui.advance_cursor_after_rect(Rect::from_min_size(top, vec2(used.width(), height)));
    Some(inner)
}

/// Margin and border of [`list_frame`] on one side.
const LIST_FRAME_SIDE: i8 = 4;
const LIST_FRAME_STROKE: f32 = 1.0;
/// What [`list_frame`] adds to the height of its content (both sides).
pub const LIST_FRAME_EXTRA: f32 = 2.0 * (LIST_FRAME_SIDE as f32 + LIST_FRAME_STROKE);

/// The frame around a list: field color with the border of the controls, so
/// one sees where the list ends.
pub fn list_frame<R>(ui: &mut Ui, scheme: &Scheme, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    Frame::new()
        .fill(color(scheme.field))
        .stroke(Stroke::new(LIST_FRAME_STROKE, color(scheme.control_border())))
        .corner_radius(control_radius(scheme))
        .inner_margin(Margin::same(LIST_FRAME_SIDE))
        .show(ui, add_contents)
}

/// What an empty list or page shows: a muted icon and one sentence that says
/// what to do so that something appears.
pub fn empty_state(ui: &mut Ui, scheme: &Scheme, icon: &str, text: &str) -> Response {
    let state = ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        if let Some(image) = crate::icons::themed(icon, scheme, IconTint::Normal, 28.0) {
            ui.add(image);
        }
        ui.add_space(4.0);
        ui.label(RichText::new(text).color(color(scheme.sub)));
        ui.add_space(8.0);
    });
    crate::highlight::mark(ui, scheme, state.response.rect, &[text]);
    state.response
}

/// Widest a dialog gets.
pub const DIALOG_WIDTH: f32 = 420.0;

/// A dialog in the middle of the window over the dimmed page, in
/// a box with corner brackets. Returns the result of `add_contents` and
/// whether it should close (Esc or a click beside it).
pub fn dialog<R>(
    ctx: &egui::Context,
    scheme: &Scheme,
    id_salt: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, bool) {
    dialog_wide(ctx, scheme, id_salt, DIALOG_WIDTH, add_contents)
}

/// [`dialog`] of a width of its own (a picture needs more than a question does).
pub fn dialog_wide<R>(
    ctx: &egui::Context,
    scheme: &Scheme,
    id_salt: &str,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, bool) {
    let modal = egui::Modal::new(egui::Id::unique(id_salt)).frame(Frame::NONE).show(ctx, |ui| {
        ui.set_max_width(width);
        card(ui, scheme, add_contents).inner
    });
    let close = modal.should_close();
    (modal.inner, close)
}

// ---------------------------------------------------------------------------
// Headings
// ---------------------------------------------------------------------------

/// Heading color: the accent in KRT, otherwise the text color.
pub fn heading_color(scheme: &Scheme) -> Color {
    if scheme.accent_headings { scheme.accent } else { scheme.fg }
}

/// A page heading as the scheme writes it, in Lato Bold.
pub fn heading(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let font = crate::fonts::bold(TextStyle::Heading.resolve(ui.style()).size);
    let shown = ui.label(RichText::new(scheme.heading(text)).font(font).color(color(heading_color(scheme))));
    crate::highlight::mark(ui, scheme, shown.rect, &[text]);
    shown
}

/// The color of the title of a card ([`card_title`], [`crate::settings::setting_group`]): the text color of the
/// scheme.
pub fn section_title_color(scheme: &Scheme) -> Color {
    scheme.accent
}

/// How much larger the title of a card is than the text of its rows.
pub const CARD_TITLE_SCALE: f32 = 1.15;

/// The title of a card: bold, one step larger than the text of its rows, in the accent of the scheme, written as
/// given, never underlined. After a jump from the menu filter to its text it frames itself.
pub fn card_title(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let shown = card_title_label(ui, scheme, text);
    crate::highlight::mark(ui, scheme, shown.rect, &[text]);
    shown
}

/// [`card_title`] without the frame of a menu filter jump, for a caller that frames the whole card.
pub fn card_title_label(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let size = TextStyle::Body.resolve(ui.style()).size * CARD_TITLE_SCALE;
    let title = RichText::new(text).font(crate::fonts::bold(size)).color(color(section_title_color(scheme)));
    ui.add(egui::Label::new(title).wrap())
}

// ---------------------------------------------------------------------------
// Boxes
// ---------------------------------------------------------------------------

/// Length of the KRT corner brackets.
pub const CORNER_LENGTH: f32 = 10.0;

/// The two corner brackets of a box at `rect`: top left and bottom right
/// (`_corner_marks`), each as three points.
pub fn corner_marks(rect: Rect) -> [[Pos2; 3]; 2] {
    let (left, top) = (rect.min.x + 1.0, rect.min.y + 1.0);
    let (right, bottom) = (rect.max.x - 1.0, rect.max.y - 1.0);
    [
        [pos2(left, rect.min.y + CORNER_LENGTH), pos2(left, top), pos2(rect.min.x + CORNER_LENGTH, top)],
        [pos2(right - CORNER_LENGTH, bottom), pos2(right, bottom), pos2(right, bottom - CORNER_LENGTH)],
    ]
}

/// The two corner brackets of a rounded box: they follow the rounding of
/// the corner (`radius`) and run on straight for a few points.
pub fn rounded_corner_marks(rect: Rect, radius: f32) -> [Vec<Pos2>; 2] {
    const STEPS: usize = 8;
    let r = (radius - 1.0).max(0.0);
    let leg = r + CORNER_LENGTH - 4.0;
    let (left, top) = (rect.min.x + 1.0, rect.min.y + 1.0);
    let (right, bottom) = (rect.max.x - 1.0, rect.max.y - 1.0);
    let arc = |center: Pos2, from: f32| {
        (0..=STEPS).map(move |i| {
            let angle = from + std::f32::consts::FRAC_PI_2 * i as f32 / STEPS as f32;
            pos2(center.x + r * angle.cos(), center.y + r * angle.sin())
        })
    };
    let mut top_left = vec![pos2(left, top + leg)];
    top_left.extend(arc(pos2(left + r, top + r), std::f32::consts::PI));
    top_left.push(pos2(left + leg, top));
    // From the bottom edge round to the right edge: the quarter from 90° back to 0°.
    let mut bottom_right = vec![pos2(right - leg, bottom)];
    bottom_right.extend(arc(pos2(right - r, bottom - r), 0.0).collect::<Vec<_>>().into_iter().rev());
    bottom_right.push(pos2(right, bottom - leg));
    [top_left, bottom_right]
}

/// The cards a run of the tests drew, with their places: the checks of the widths and of the layout of the
/// cards of a page read them (there is no other way to see a card from outside once it is drawn). Outside
/// the unit tests the cards are only noted between [`record`]`(true)` and `record(false)` of a thread, so the
/// program itself never collects them.
pub mod drawn_cards {
    use std::cell::{Cell, RefCell};

    thread_local! {
        /// Place, clipping area and whether the card floats above the page.
        static CARDS: RefCell<Vec<(egui::Rect, egui::Rect, bool)>> = const { RefCell::new(Vec::new()) };
        static ON: Cell<bool> = const { Cell::new(cfg!(test)) };
    }

    pub(super) fn note(rect: egui::Rect, clip: egui::Rect, floating: bool) {
        if ON.with(Cell::get) {
            CARDS.with(|cards| cards.borrow_mut().push((rect, clip, floating)));
        }
    }

    /// Starts or stops noting the cards drawn by this thread; stopping forgets the ones noted.
    pub fn record(on: bool) {
        ON.with(|flag| flag.set(on));
        if !on {
            take();
        }
    }

    /// The cards drawn since the last call (or the start), in the order of drawing.
    pub fn take() -> Vec<egui::Rect> {
        CARDS.with(|cards| std::mem::take(&mut *cards.borrow_mut())).into_iter().map(|(rect, ..)| rect).collect()
    }

    /// Like [`take`], but only the cards of the page (not a notice or a dialog that floats above it), each
    /// with the area it was drawn into (the part of it that can be seen).
    pub fn take_clipped() -> Vec<(egui::Rect, egui::Rect)> {
        take_split().0
    }

    /// The cards drawn since the last call: those of the page as [`take_clipped`] gives them, and the places of
    /// the cards that float above it.
    pub fn take_split() -> (Vec<(egui::Rect, egui::Rect)>, Vec<egui::Rect>) {
        let (floating, page): (Vec<_>, Vec<_>) = CARDS
            .with(|cards| std::mem::take(&mut *cards.borrow_mut()))
            .into_iter()
            .partition(|(.., floating)| *floating);
        (
            page.into_iter().map(|(rect, clip, _)| (rect, clip)).collect(),
            floating.into_iter().map(|(rect, ..)| rect).collect(),
        )
    }
}

/// The columns of a row of cards (`layout::columns_or_stack`) keep the cards of the row equally tall: a column
/// tells the cards in it by how much its last card is to be taller, and the card pads itself by that.
pub(crate) mod row_cards {
    use std::cell::RefCell;

    struct Column {
        last: usize,
        pad: f32,
        drawn: usize,
        applied: f32,
        inside: bool,
    }

    thread_local! {
        static COLUMNS: RefCell<Vec<Column>> = const { RefCell::new(Vec::new()) };
    }

    /// A column begins: `last` is the number of cards it drew in the frame before, `pad` the height its last
    /// card is to be taller by.
    pub(crate) fn enter(last: usize, pad: f32) {
        COLUMNS.with(|columns| columns.borrow_mut().push(Column { last, pad, drawn: 0, applied: 0.0, inside: false }));
    }

    /// The column ends: the number of cards drawn in it and the height added to its last one.
    pub(crate) fn leave() -> (usize, f32) {
        COLUMNS.with(|columns| columns.borrow_mut().pop().map_or((0, 0.0), |column| (column.drawn, column.applied)))
    }

    /// A card begins: `Some(height to add)` for a card of the column itself, `None` for a card inside a card or
    /// outside any column.
    pub(super) fn begin_card() -> Option<f32> {
        COLUMNS.with(|columns| {
            let mut columns = columns.borrow_mut();
            let column = columns.last_mut().filter(|column| !column.inside)?;
            column.inside = true;
            column.drawn += 1;
            Some(if column.drawn == column.last { column.pad } else { 0.0 })
        })
    }

    pub(super) fn end_card(added: f32) {
        COLUMNS.with(|columns| {
            if let Some(column) = columns.borrow_mut().last_mut() {
                column.inside = false;
                column.applied += added;
            }
        });
    }
}

/// A box on the surface color with a thin border and two corner brackets in
/// the accent color (top left, bottom right), in every scheme.
/// Square in KRT, otherwise rounded with brackets that follow
/// the rounding.
pub fn card<R>(ui: &mut Ui, scheme: &Scheme, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let radius = box_radius(scheme);
    let pad = row_cards::begin_card();
    let shown = Frame::new()
        .fill(color(scheme.surface))
        .stroke(Stroke::new(1.0, color(scheme.line)))
        .corner_radius(radius)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            // One below the other the cards of a page are all the same width: a frame is as wide as its content,
            // and content that does not spread over the width itself (a label, a list) would leave the card
            // narrow, with its corner marks away from those of the cards around it. Cards that stand side by
            // side (a horizontal layout) keep the width of their content, and so do the ones that float
            // above the page (a notice, a dialog, a tile that is dragged): there the width is the content's.
            if ui.layout().main_dir().is_vertical() && ui.layer_id().order == egui::Order::Background {
                ui.set_min_width(ui.available_width());
            }
            let inner = add_contents(ui);
            // The last card of a column of a row is as tall as the tallest column asks for.
            if let Some(added) = pad.filter(|added| *added > 0.5) {
                ui.allocate_space(vec2(0.0, added));
            }
            inner
        });
    if let Some(added) = pad {
        row_cards::end_card(added);
    }
    let stroke = Stroke::new(2.0, color(scheme.accent));
    let rect = shown.response.rect;
    drawn_cards::note(rect, ui.clip_rect(), ui.layer_id().order != egui::Order::Background);
    if scheme.square {
        for mark in corner_marks(rect) {
            ui.painter().line(mark.to_vec(), stroke);
        }
    } else {
        for mark in rounded_corner_marks(rect, f32::from(radius.nw)) {
            ui.painter().line(mark, stroke);
        }
    }
    shown
}

// ---------------------------------------------------------------------------
// Scroll bars
// ---------------------------------------------------------------------------

/// Width of a scroll bar (`round_scrollbar`, `width=10`).
pub const SCROLL_WIDTH: f32 = 10.0;

/// A solid bar with a visible groove; the handle stays at least 2.4 widths
/// long so it can still be grabbed in a list of 700 entries.
pub fn scroll_style() -> ScrollStyle {
    ScrollStyle {
        bar_width: SCROLL_WIDTH,
        handle_min_length: SCROLL_WIDTH * 2.4,
        bar_inner_margin: 2.0,
        bar_outer_margin: 0.0,
        ..ScrollStyle::solid()
    }
}

/// Shows a scroll area whose bar uses the scheme's scroll colors: groove,
/// grip, light grip while hovered or dragged. The content keeps the normal
/// colors.
pub fn scroll_area<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    area: ScrollArea,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> ScrollAreaOutput<R> {
    let content_visuals = ui.visuals().clone();
    let radius = CornerRadius::same(if scheme.square { 0 } else { (SCROLL_WIDTH / 2.0) as u8 });
    ui.scope(|ui| {
        ui.spacing_mut().scroll = scroll_style();
        let v = ui.visuals_mut();
        v.extreme_bg_color = color(scheme.scroll_groove);
        v.widgets.inactive.bg_fill = color(scheme.scroll_grip);
        v.widgets.hovered.bg_fill = color(scheme.scroll_grip_light);
        v.widgets.active.bg_fill = color(scheme.scroll_grip_light);
        for state in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
            state.corner_radius = radius;
        }
        area.show(ui, |ui| {
            *ui.visuals_mut() = content_visuals;
            add_contents(ui)
        })
    })
    .inner
}

// ---------------------------------------------------------------------------
// Version, data age, status, gallery, community mark
// ---------------------------------------------------------------------------

/// The version next to the program name: a stable version in the
/// accent; a test build as a yellow label in the
/// window, not in the window title. `test_label` is that label's word.
pub fn version_mark(ui: &mut Ui, scheme: &Scheme, version: &str, test_label: Option<&str>) -> Response {
    let Some(word) = test_label else {
        return ui.label(RichText::new(format!("v{version}")).small().strong().color(color(scheme.accent)));
    };
    Frame::new()
        .stroke(Stroke::new(1.0, color(scheme.yellow)))
        .corner_radius(control_radius(scheme))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} {version}", word.to_uppercase()))
                    .small()
                    .strong()
                    .color(color(scheme.yellow)),
            )
        })
        .response
}

/// A text in the bold font at the size of `style` (`RichText::strong` only changes the color).
pub fn bold(ui: &Ui, text: impl Into<String>, style: TextStyle) -> RichText {
    RichText::new(text.into()).font(crate::fonts::bold(style.resolve(ui.style()).size))
}

/// How old the shown data is, as muted text; date and time on hover.
pub fn data_age(ui: &mut Ui, scheme: &Scheme, seconds: f64, exact: &str) -> Response {
    use versekit_core::language::{Arg, relative_age, tf};
    let text = tf("s_st_zuletzt", &[("0", Arg::Text(relative_age(seconds)))]);
    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).truncate()).on_hover_text(exact)
}

/// A lasting state on a page, e.g. game folder found: a box with
/// a bar in `tone` at the left, an icon, a bold sentence and a muted line.
pub fn status_box(ui: &mut Ui, scheme: &Scheme, tone: Color, icon: &str, head: &str, line: &str) -> Response {
    let inner = Frame::new()
        .fill(color(scheme.surface))
        .stroke(Stroke::new(1.0, color(scheme.line)))
        .corner_radius(control_radius(scheme))
        .inner_margin(Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if let Some(image) = crate::icons::image(icon, tone, 18.0) {
                    ui.add(image);
                }
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(head).strong()).truncate());
                    if !line.is_empty() {
                        ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).truncate());
                    }
                });
            });
        });
    let rect = inner.response.rect;
    let bar = Rect::from_min_size(rect.min, vec2(LIST_BAR_WIDTH, rect.height()));
    ui.painter().rect_filled(bar, 0.0, color(tone));
    crate::highlight::mark(ui, scheme, rect, &[head, line]);
    inner.response
}

/// One highlight of the what's-new gallery: icon, head, a sentence, the page it shows.
pub struct NewsItem<'a> {
    pub icon: &'a str,
    pub head: &'a str,
    pub text: &'a str,
    pub page: &'a str,
}

/// Words of the gallery: `title` and `of` make the counter
/// (version and position, e.g. 2 of 3), the rest label the buttons.
pub struct GalleryTexts<'a> {
    pub title: &'a str,
    pub of: &'a str,
    pub back: &'a str,
    pub next: &'a str,
    pub open: &'a str,
    pub close: &'a str,
}

/// What the user did in the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryAction {
    /// Open the page of the highlight with this index.
    Open(usize),
    Close,
}

/// The what's-new gallery after an update: one highlight at a time with its
/// icon, the arrows (and ← →, while no field has the keyboard) page through,
/// the open button opens its page; not modal. `index` is the highlight shown.
pub fn news_gallery(
    ui: &mut Ui,
    scheme: &Scheme,
    items: &[NewsItem],
    index: &mut usize,
    texts: &GalleryTexts,
) -> Option<GalleryAction> {
    if items.is_empty() {
        return Some(GalleryAction::Close);
    }
    *index = (*index).min(items.len() - 1);
    let step = |index: &mut usize, forward: bool| {
        *index = if forward { (*index + 1) % items.len() } else { (*index + items.len() - 1) % items.len() };
    };
    let keys_free = ui.memory(|m| m.focused().is_none());
    if keys_free {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            step(index, true);
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            step(index, false);
        }
    }
    let mut action = None;
    card(ui, scheme, |ui| {
        let item = &items[*index];
        let count = format!("{} · {} {} {}", texts.title, *index + 1, texts.of, items.len());
        ui.label(RichText::new(count).small().color(color(scheme.sub)));
        ui.horizontal(|ui| {
            if let Some(image) = crate::icons::themed(item.icon, scheme, IconTint::Highlight, 28.0) {
                ui.add(image);
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(item.head).strong());
                ui.label(item.text);
            });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if icon_button(ui, scheme, "arrow-left", texts.back).clicked() {
                step(index, false);
            }
            if icon_button(ui, scheme, "arrow-right", texts.next).clicked() {
                step(index, true);
            }
            if ui.add(Button::new(texts.open, ButtonKind::Strong, scheme)).clicked() {
                action = Some(GalleryAction::Open(*index));
            }
            if ui.add(Button::new(texts.close, ButtonKind::Normal, scheme)).clicked() {
                action = Some(GalleryAction::Close);
            }
        });
    });
    action
}

/// A row of drop-down filters over a list; `filters[f]` are the
/// choices of filter `f`, the first one meaning all, `chosen[f]` the one
/// picked. The reset button is not part of it: it stays at the top
/// right of the page head.
/// `id_salt` tells this bar from every other one in the same page: bars
/// built in one `Ui` share its scope, so with the same salt their
/// drop-downs would share one id and shut each other's list at once.
/// Returns whether a filter changed.
pub fn filter_bar(ui: &mut Ui, id_salt: &str, filters: &[&[&str]], chosen: &mut [usize]) -> bool {
    let before = chosen.to_vec();
    ui.horizontal_wrapped(|ui| {
        for (f, options) in filters.iter().enumerate() {
            let picked = options.get(chosen[f]).copied().unwrap_or_default();
            let combo = egui::ComboBox::from_id_salt(("filterleiste", id_salt, f))
                .selected_text(picked)
                .width(120.0)
                .show_ui(ui, |list| {
                    for (o, option) in options.iter().enumerate() {
                        list.selectable_value(&mut chosen[f], o, *option);
                    }
                });
            name_combo(&combo.response, options.first().copied().unwrap_or_default(), picked);
        }
    });
    before != chosen
}

/// A link that opens outside Verse-Kit: the text in the accent and the
/// `external-link` symbol after it, both clickable; underlined on hover.
pub fn external_link(ui: &mut Ui, scheme: &Scheme, text: &str) -> Response {
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let link = ui.add(egui::Link::new(RichText::new(text).color(color(scheme.accent))));
        let icon = crate::icons::themed("external-link", scheme, IconTint::Highlight, 14.0).map(|image| {
            let response = ui.add(image.sense(Sense::click()));
            // The symbol clicks like the text, so it carries the same name.
            response.widget_info(|| WidgetInfo::labeled(Role::Link, true, text));
            response
        });
        icon.map_or(link.clone(), |icon| link | icon)
    });
    crate::highlight::mark(ui, scheme, inner.response.rect, &[text]);
    inner.inner.on_hover_cursor(CursorIcon::PointingHand)
}

/// A whole number with − and + and the field between them to type into.
/// Typing is taken on Enter or when the field is left, kept inside
/// `range`; the unit follows the field. Returns whether the value changed.
pub fn number_field(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    value: &mut i64,
    range: std::ops::RangeInclusive<i64>,
    unit: &str,
) -> bool {
    number_field_labelled(ui, scheme, id_salt, value, range, unit, None)
}

/// [`number_field`] whose field is named for screen readers by the label with the id `label`.
pub fn number_field_labelled(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    value: &mut i64,
    range: std::ops::RangeInclusive<i64>,
    unit: &str,
    label: Option<egui::Id>,
) -> bool {
    let before = *value;
    let id = egui::Id::unique(("versekit-zahlenfeld", id_salt));
    // In a row laid out from the right (the control of a setting row) the
    // parts come in reverse, so on screen it always reads − field + unit.
    let order: [u8; 4] = if ui.layout().prefer_right_to_left() { [3, 2, 1, 0] } else { [0, 1, 2, 3] };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for part in order {
            match part {
                0 => {
                    if icon_button(ui, scheme, "minus", &versekit_core::language::t("s_ff_weniger")).clicked() {
                        *value = (*value - 1).max(*range.start());
                    }
                }
                1 => {
                    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| value.to_string());
                    let field =
                        text_field(&mut text, "", scheme).desired_width(56.0).align(egui::Align2::RIGHT_CENTER).id(id);
                    let response = ui.add(field);
                    let response = match label {
                        Some(label) => response.labelled_by(label),
                        None => response,
                    };
                    if response.has_focus() {
                        ui.data_mut(|d| d.insert_temp(id, text.clone()));
                    } else {
                        if response.lost_focus() {
                            *value = text.trim().parse::<i64>().unwrap_or(*value).clamp(*range.start(), *range.end());
                        }
                        ui.data_mut(|d| d.remove::<String>(id));
                    }
                }
                2 => {
                    if icon_button(ui, scheme, "plus", &versekit_core::language::t("s_ff_mehr")).clicked() {
                        *value = (*value + 1).min(*range.end());
                    }
                }
                _ => {
                    if !unit.is_empty() {
                        ui.label(unit);
                    }
                }
            }
        }
    });
    *value != before
}

/// One line of a ranking: the bar behind the line, as long as the
/// share of the largest entry, name at the left, number at the right.
pub fn ranking_row(ui: &mut Ui, scheme: &Scheme, name: &str, number: &str, share: f32) -> Response {
    ranking_row_tone(ui, scheme, name, number, share, RankTone::Accent)
}

/// What a ranking is about: the usual (accent), a loss (red) or equipment
/// (gold), as the statistics pages of 4.12 colored their bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RankTone {
    Accent,
    Red,
    Gold,
}

/// The bar color of a ranking line: dark enough for the text on top of it
/// (the contrast test measures `fg` on it in every scheme).
pub fn rank_bar(scheme: &Scheme, tone: RankTone) -> Color {
    match tone {
        RankTone::Accent => scheme.accent_dark,
        RankTone::Red => scheme.danger_fill,
        RankTone::Gold => scheme.gold.mix(scheme.bg, 0.72),
    }
}

/// The edge of a ranking bar: the tone in full, so the length shows against
/// the page (3:1, measured in `tests/contrast.rs`).
pub fn rank_edge(scheme: &Scheme, tone: RankTone) -> Color {
    match tone {
        RankTone::Accent => scheme.accent,
        RankTone::Red => scheme.red,
        RankTone::Gold => scheme.gold,
    }
}

/// [`ranking_row`] with the tone of its bar.
pub fn ranking_row_tone(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    number: &str,
    share: f32,
    tone: RankTone,
) -> Response {
    let height = ui.text_style_height(&TextStyle::Body) + 10.0;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    // A reader of the screen hears the name and the number, though the line is painted.
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{name}, {number}")));
    let mut elided = false;
    if ui.is_rect_visible(rect) {
        let bar = Rect::from_min_size(rect.min, vec2(rect.width() * share.clamp(0.0, 1.0), rect.height()));
        let radius = control_radius(scheme);
        ui.painter().rect_filled(bar, radius, color(rank_bar(scheme, tone)));
        // The fill is dark for the text on it; the edge in the color of the tone
        // is what shows the length against the page.
        ui.painter().rect_stroke(bar, radius, Stroke::new(1.0, color(rank_edge(scheme, tone))), StrokeKind::Inside);
        let font = TextStyle::Body.resolve(ui.style());
        let text = color(scheme.fg);
        let painter = ui.painter();
        let number_galley = painter.layout_no_wrap(number.to_string(), font.clone(), text);
        // A long name is cut with "…" before the number, never over it.
        let room = (rect.width() - number_galley.size().x - 3.0 * 8.0).max(20.0);
        let mut job = LayoutJob::simple_singleline(name.to_string(), font, text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let name_galley = painter.layout_job(job);
        elided = name_galley.elided;
        painter.galley(pos2(rect.min.x + 8.0, rect.center().y - name_galley.size().y / 2.0), name_galley, text);
        painter.galley(
            pos2(rect.max.x - 8.0 - number_galley.size().x, rect.center().y - number_galley.size().y / 2.0),
            number_galley,
            text,
        );
    }
    // What was cut can be read whole when the pointer rests on the line.
    if elided { response.on_hover_text(name) } else { response }
}

/// The color schemes as small pictures of the window in THEIR colors (like
/// 4.12): head bar with an accent dot, two lines, an accent bar; square
/// schemes with a corner bracket. Under each its name, the chosen one in
/// the accent with a check mark. `names[i]` belongs to `schemes[i]`.
/// Returns the id of a clicked scheme.
pub fn scheme_cards(ui: &mut Ui, current: &Scheme, schemes: &[Scheme], names: &[String]) -> Option<&'static str> {
    const CARD: Vec2 = vec2(150.0, 70.0);
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 10.0);
        for (s, name) in schemes.iter().zip(names) {
            let on = s.id == current.id;
            // A fixed size known before drawing, so the row wraps before a card
            // that would not fit (an open-ended `vertical` made it grow instead).
            let label_height = ui.text_style_height(&TextStyle::Small).max(14.0) + 4.0;
            let size = vec2(CARD.x, CARD.y + ui.spacing().item_spacing.y + label_height);
            let card = ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                ui.set_width(CARD.x);
                let (rect, response) = ui.allocate_exact_size(CARD, Sense::click());
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    let f = CARD.y / 78.0;
                    let at = |x: f32, y: f32| rect.min + vec2(x, y * f);
                    let border = if on || response.hovered() { current.accent } else { s.line };
                    p.rect(rect, 0.0, color(s.bg), Stroke::new(2.0, color(border)), StrokeKind::Inside);
                    p.rect_filled(
                        Rect::from_min_max(at(3.0, 3.0), rect.right_top() + vec2(-3.0, 18.0 * f)),
                        0.0,
                        color(s.bar),
                    );
                    p.circle_filled(at(13.0 * f, 11.0), 3.0 * f, color(s.accent));
                    p.rect_filled(
                        Rect::from_min_max(at(12.0 * f, 28.0), at(CARD.x - 12.0 * f, 34.0)),
                        0.0,
                        color(s.surface),
                    );
                    p.rect_filled(
                        Rect::from_min_max(at(12.0 * f, 40.0), at(CARD.x - 50.0 * f, 46.0)),
                        0.0,
                        color(s.surface),
                    );
                    p.rect_filled(
                        Rect::from_min_max(at(12.0 * f, 56.0), at(CARD.x - 30.0 * f, 64.0)),
                        0.0,
                        color(s.accent),
                    );
                    if s.square {
                        let points = vec![at(6.0, 30.0), at(6.0, 24.0), at(12.0 * f, 24.0)];
                        p.add(egui::Shape::line(points, Stroke::new(2.0, color(s.accent))));
                    }
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if on && let Some(image) = crate::icons::themed("check", current, IconTint::Highlight, 14.0) {
                        ui.add(image);
                    }
                    let tone = if on { current.accent } else { current.sub };
                    ui.add(egui::Label::new(RichText::new(name).small().color(color(tone))).truncate());
                });
                response
            });
            let response = card.inner.on_hover_cursor(CursorIcon::PointingHand);
            if response.clicked() {
                picked = Some(s.id);
            }
        }
    });
    picked
}

/// A small label with a border in `tone`.
pub fn badge(ui: &mut Ui, scheme: &Scheme, tone: Color, text: &str) -> Response {
    Frame::new()
        .stroke(Stroke::new(1.0, color(tone)))
        .corner_radius(control_radius(scheme))
        .inner_margin(Margin::symmetric(5, 0))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(color(tone))))
        .response
}

/// The badges of a row: at most two, the rest as a +N badge with all of them
/// on hover. On screen always in the order given, the +N badge last, also in
/// a row laid out from the right.
pub fn badges(ui: &mut Ui, scheme: &Scheme, marks: &[(&str, Color)]) {
    let rest: Vec<&str> = marks.iter().skip(2).map(|(text, _)| *text).collect();
    let rest_label = format!("+{}", rest.len());
    let mut parts: Vec<(&str, Color, bool)> = marks.iter().take(2).map(|(text, tone)| (*text, *tone, false)).collect();
    if !rest.is_empty() {
        parts.push((&rest_label, scheme.sub, true));
    }
    if ui.layout().prefer_right_to_left() {
        parts.reverse();
    }
    for (text, tone, is_rest) in parts {
        let shown = badge(ui, scheme, tone, text);
        if is_rest {
            shown.on_hover_text(rest.join(" · "));
        }
    }
}

/// The short code of an item: class written out, size, grade, always
/// in this order and form, `Militär · S2 · A`, as a quiet label after the
/// name. Items without a class show only size and grade.
pub fn class_badge(ui: &mut Ui, scheme: &Scheme, class: Option<&str>, size: u8, grade: char) -> Response {
    let text = match class {
        Some(class) => format!("{class} · S{size} · {grade}"),
        None => format!("S{size} · {grade}"),
    };
    badge(ui, scheme, scheme.sub, &text)
}

/// The color that says better or worse: blue and yellow, so it also
/// works for people who tell red from green badly. Whether more is better
/// is the caller's business.
pub fn change_color(scheme: &Scheme, better: bool) -> Color {
    if better { scheme.blue } else { scheme.yellow }
}

/// The state symbols at the start of a row: always there, in the
/// accent when on and pale when off, each a click to switch; `states` are
/// (Lucide icon, on, meaning). Returns the index of a switched state.
pub fn state_icons(ui: &mut Ui, scheme: &Scheme, states: &mut [(&str, bool, &str)]) -> Option<usize> {
    let mut switched = None;
    for (i, (icon, on, meaning)) in states.iter_mut().enumerate() {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
        let tone = if *on {
            scheme.accent
        } else if response.hovered() {
            scheme.sub
        } else {
            scheme.line
        };
        if let Some(image) = crate::icons::image(icon, tone, 16.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(*meaning).clicked() {
            *on = !*on;
            switched = Some(i);
        }
    }
    switched
}

/// A toggle row with counters: one button per choice with its count,
/// the chosen one filled. Returns whether the choice changed.
pub fn count_toggles(ui: &mut Ui, scheme: &Scheme, choices: &[(&str, usize)], chosen: &mut usize) -> bool {
    let before = *chosen;
    ui.horizontal_wrapped(|ui| {
        for (i, (word, count)) in choices.iter().enumerate() {
            let kind = if *chosen == i { ButtonKind::Strong } else { ButtonKind::Normal };
            if ui.add(Button::new(&format!("{word} {count}"), kind, scheme)).clicked() {
                *chosen = i;
            }
        }
    });
    *chosen != before
}

/// The filter a jump brought along, as a label with ✕ to loosen it.
/// Returns true when ✕ was clicked.
pub fn jump_filter(ui: &mut Ui, scheme: &Scheme, text: &str) -> bool {
    let mut loosen = false;
    Frame::new()
        .fill(color(scheme.accent_dark))
        .stroke(Stroke::new(1.0, color(scheme.accent)))
        .corner_radius(control_radius(scheme))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(text).small());
                loosen = icon_button(ui, scheme, "x", &versekit_core::language::t("s_ff_filter_loesen")).clicked();
            });
        });
    loosen
}

/// One version in the what's-new history: number, date, what came.
pub struct VersionNote<'a> {
    pub version: &'a str,
    pub date: &'a str,
    pub lines: &'a [&'a str],
}

/// The version history, newest first: each version a card that folds;
/// open are the newest one and every version newer than `seen` (the one the
/// user ran before), so whoever skipped versions reads what came between.
pub fn version_history(ui: &mut Ui, scheme: &Scheme, notes: &[VersionNote], seen: &str) {
    let seen_at = notes.iter().position(|n| n.version == seen).unwrap_or(notes.len());
    for (i, note) in notes.iter().enumerate() {
        let open = i == 0 || i < seen_at;
        card(ui, scheme, |ui| {
            ui.set_width(ui.available_width());
            collapsing(&format!("VerseKit {} · {}", note.version, note.date))
                .id_salt(("versionen", note.version))
                .default_open(open)
                .show(ui, |ui| {
                    for line in note.lines {
                        ui.label(format!("· {line}"));
                    }
                });
        });
        ui.add_space(4.0);
    }
}

/// A key combination that also works in the game: the modifier keys as
/// buttons that stay pressed, the key from a list. `modifiers` are
/// (word, pressed), `key_name` names the key list for screen readers.
/// Returns whether the combination changed.
pub fn hotkey_picker(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    modifiers: &mut [(&str, bool)],
    keys: &[&str],
    key: &mut usize,
    key_name: &str,
) -> bool {
    let before = (modifiers.iter().map(|(_, on)| *on).collect::<Vec<_>>(), *key);
    ui.horizontal_wrapped(|ui| {
        for (word, on) in modifiers.iter_mut() {
            let kind = if *on { ButtonKind::Strong } else { ButtonKind::Normal };
            if ui.add(Button::new(word, kind, scheme)).clicked() {
                *on = !*on;
            }
        }
        ui.label("+");
        let picked = keys.get(*key).copied().unwrap_or_default();
        let combo = egui::ComboBox::from_id_salt(("tastenkuerzel", id_salt)).selected_text(picked).width(80.0).show_ui(
            ui,
            |list| {
                for (k, name) in keys.iter().enumerate() {
                    list.selectable_value(key, k, *name);
                }
            },
        );
        name_combo(&combo.response, key_name, picked);
    });
    before != (modifiers.iter().map(|(_, on)| *on).collect::<Vec<_>>(), *key)
}

/// What the user did at a folder in one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FolderEvents {
    /// The field was left or Enter was pressed: what stands in it is meant.
    pub typed: bool,
    pub browse: bool,
    pub open: bool,
}

/// What the user did at a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderAction {
    Browse,
    Open,
}

/// A folder to choose: the path as a field to type into, below it
/// the browse button (the system dialog) and the open button (in the file manager).
/// `words` are (browse, open).
pub fn folder_field(ui: &mut Ui, scheme: &Scheme, path: &mut String, words: (&str, &str)) -> Option<FolderAction> {
    let events = folder_field_events(ui, scheme, path, words);
    if events.browse {
        Some(FolderAction::Browse)
    } else if events.open {
        Some(FolderAction::Open)
    } else {
        None
    }
}

/// [`folder_field`] with everything that happened in the frame. The caller takes the typed
/// path only when `typed` says the field was left, not with every key.
pub fn folder_field_events(ui: &mut Ui, scheme: &Scheme, path: &mut String, words: (&str, &str)) -> FolderEvents {
    let placeholder = versekit_core::language::t("s_ff_ordner_ph");
    let typed = ui.add(text_field(path, &placeholder, scheme).desired_width(f32::INFINITY)).lost_focus();
    let mut events = FolderEvents { typed, ..FolderEvents::default() };
    ui.horizontal(|ui| {
        events.browse = ui.add(Button::new(words.0, ButtonKind::Normal, scheme)).clicked();
        events.open = ui.add(Button::new(words.1, ButtonKind::Normal, scheme).icon("folder")).clicked();
    });
    events
}

/// Heights the community mark is prepared in
/// (`assets/community/<n>.png`, white on clear, from the CIG fan kit).
pub const COMMUNITY_SIZES: [u32; 7] = [64, 80, 96, 112, 128, 160, 200];

/// The biggest prepared size in screen pixels that is not taller than
/// `pixels` (the smallest when none fits): the mark is never scaled up.
pub fn community_size(pixels: f32) -> u32 {
    COMMUNITY_SIZES.iter().rev().copied().find(|&s| s as f32 <= pixels).unwrap_or(COMMUNITY_SIZES[0])
}

/// Height in points the mark takes for at most `height` points: chosen in
/// screen pixels, so with Windows scaling above 100 % it stays sharp.
pub fn community_height(height: f32, pixels_per_point: f32) -> f32 {
    community_size(height * pixels_per_point) as f32 / pixels_per_point
}

/// The community mark: white, in the accent on hover,
/// clickable (the caller opens the link), at most `height` points tall. It
/// is drawn pixel for pixel on the pixel grid; a prepared size is never
/// stretched, else it blurs at high resolutions.
pub fn community_mark(ui: &mut Ui, scheme: &Scheme, height: f32) -> Response {
    use egui::emath::GuiRounding as _;
    let pixels_per_point = ui.ctx().pixels_per_point();
    let size = community_size(height * pixels_per_point);
    let bytes: &'static [u8] = match size {
        64 => include_bytes!("../../../assets/community/64.png"),
        80 => include_bytes!("../../../assets/community/80.png"),
        96 => include_bytes!("../../../assets/community/96.png"),
        112 => include_bytes!("../../../assets/community/112.png"),
        128 => include_bytes!("../../../assets/community/128.png"),
        160 => include_bytes!("../../../assets/community/160.png"),
        _ => include_bytes!("../../../assets/community/200.png"),
    };
    let points = Vec2::splat(size as f32 / pixels_per_point);
    let (rect, response) = ui.allocate_exact_size(points, Sense::click());
    if ui.is_rect_visible(rect) {
        let exact = Rect::from_min_size(rect.min.round_to_pixels(pixels_per_point), points);
        let tint = if response.hovered() { color(scheme.accent) } else { Color32::WHITE };
        egui::Image::from_bytes(format!("bytes://community/{size}.png"), bytes).tint(tint).paint_at(ui, exact);
    }
    focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::theme::{self, KRT_ORANGE};

    fn scheme(id: &str) -> &'static Scheme {
        theme::by_id(id).expect("scheme exists")
    }

    #[test]
    fn toggle_like_python() {
        let rect = Rect::from_min_size(pos2(100.0, 50.0), TOGGLE_SIZE);
        let (capsule, off) = toggle_geometry(rect, false);
        assert_eq!(capsule, Rect::from_min_max(pos2(102.0, 53.0), pos2(142.0, 71.0)));
        // Python: oval 5..19 × 6..20, moved by width - 24 when on.
        assert_eq!(off, pos2(112.0, 63.0));
        assert_eq!(toggle_geometry(rect, true).1, pos2(132.0, 63.0));
    }

    #[test]
    fn button_colors_like_python() {
        let original = scheme("original");
        let krt = scheme("krt");
        let normal = button_colors(original, ButtonKind::Normal, false);
        assert_eq!(normal, ButtonColors { text: original.fg, fill: original.surface, border: original.line });
        let hover = button_colors(original, ButtonKind::Normal, true);
        assert_eq!((hover.text, hover.border), (original.accent, original.accent));
        // KRT: controls with their own fill and border, hover text light accent.
        let normal = button_colors(krt, ButtonKind::Normal, true);
        assert_eq!((normal.fill, normal.text), (krt.control(), krt.hover_fg()));
        assert_eq!(button_colors(krt, ButtonKind::Normal, false).border, krt.control_border());
        // Strong: filled in KRT, dark accent area elsewhere.
        let strong = button_colors(krt, ButtonKind::Strong, false);
        assert_eq!(strong, ButtonColors { text: krt.bg, fill: krt.accent, border: krt.accent });
        // Filled strong button on hover: light accent fill, dark text (`.btn:hover`).
        let strong_hover = button_colors(krt, ButtonKind::Strong, true);
        assert_eq!((strong_hover.fill, strong_hover.text), (krt.hover_fg(), krt.bg));
        assert_eq!(button_colors(original, ButtonKind::Strong, false).fill, original.accent_dark);
        assert_eq!(button_label(krt, ButtonKind::Strong, "Speichern"), "SPEICHERN");
        assert_eq!(button_label(original, ButtonKind::Strong, "Speichern"), "Speichern");
        // Danger is red at rest; on hover red fill, light border and light
        // text (red text on the red fill would fall to 3.2:1).
        let danger = button_colors(original, ButtonKind::Danger, false);
        assert_eq!(danger, ButtonColors { text: original.red, fill: original.danger_fill, border: original.red });
        let danger_hover = button_colors(original, ButtonKind::Danger, true);
        assert_eq!((danger_hover.text, danger_hover.border), (original.fg, original.fg));
        assert_eq!(danger_hover.fill, original.danger_fill.mix(original.red, HOVER_TINT));
        assert_eq!(button_colors(original, ButtonKind::Brand(KRT_ORANGE), false).text, KRT_ORANGE);
        let brand = button_colors(original, ButtonKind::Brand(KRT_ORANGE), true);
        assert_eq!((brand.text, brand.border), (original.fg, original.fg));
    }

    #[test]
    fn every_button_changes_visibly_on_hover() {
        for s in theme::schemes() {
            for kind in [ButtonKind::Normal, ButtonKind::Strong, ButtonKind::Danger, ButtonKind::Brand(KRT_ORANGE)] {
                let rest = button_colors(s, kind, false);
                let hover = button_colors(s, kind, true);
                let changed = [rest.fill != hover.fill, rest.border != hover.border, rest.text != hover.text];
                assert!(changed.iter().filter(|c| **c).count() >= 1, "{} {kind:?}: no hover change", s.id);
                assert!(
                    rest.border != hover.border || rest.fill != hover.fill,
                    "{} {kind:?}: only the text changes",
                    s.id
                );
            }
        }
    }

    /// A switch draws something else with the mouse over it, off and on, in
    /// every scheme.
    #[test]
    fn a_switch_shows_the_mouse_over_it() {
        for s in theme::schemes() {
            for start in [false, true] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                let (at, mut on) = (pos2(60.0, 60.0), start);
                let center = at + TOGGLE_SIZE / 2.0;
                let mut drawn = |events: Vec<egui::Event>| {
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0))),
                        events,
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        ui.put(Rect::from_min_size(at, TOGGLE_SIZE), Toggle::new(&mut on, s));
                    });
                    output.textures_delta.clear();
                    format!("{:?}", output.shapes)
                };
                drawn(vec![]);
                let rest = drawn(vec![]);
                drawn(vec![egui::Event::PointerMoved(center)]);
                let over = drawn(vec![egui::Event::PointerMoved(center)]);
                assert_ne!(rest, over, "{} {start}: no change with the mouse over the switch", s.id);
            }
        }
    }

    #[test]
    fn corners_and_headings() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 80.0));
        let [top_left, bottom_right] = corner_marks(rect);
        assert_eq!(top_left, [pos2(1.0, 10.0), pos2(1.0, 1.0), pos2(10.0, 1.0)]);
        assert_eq!(bottom_right, [pos2(189.0, 79.0), pos2(199.0, 79.0), pos2(199.0, 69.0)]);
        assert_eq!(heading_color(scheme("krt")), scheme("krt").accent);
        assert_eq!(heading_color(scheme("eis")), scheme("eis").fg);
    }

    #[test]
    fn rounded_brackets_follow_the_corner() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 80.0));
        let [top_left, bottom_right] = rounded_corner_marks(rect, 8.0);
        let close = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;
        // Leg down the left side, arc with radius 7 around (8, 8), leg along the top.
        assert!(close(top_left[0], pos2(1.0, 14.0)));
        assert!(close(top_left[1], pos2(1.0, 8.0)));
        assert!(close(top_left[top_left.len() - 2], pos2(8.0, 1.0)));
        assert!(close(*top_left.last().expect("points"), pos2(14.0, 1.0)));
        for p in &top_left[1..top_left.len() - 1] {
            assert!(((*p - pos2(8.0, 8.0)).length() - 7.0).abs() < 1e-3, "{p:?} off the arc");
        }
        // Leg along the bottom, arc around (192, 72), leg up the right side.
        assert!(close(bottom_right[0], pos2(186.0, 79.0)));
        assert!(close(bottom_right[1], pos2(192.0, 79.0)));
        assert!(close(bottom_right[bottom_right.len() - 2], pos2(199.0, 72.0)));
        assert!(close(*bottom_right.last().expect("points"), pos2(199.0, 66.0)));
        for p in &bottom_right[1..bottom_right.len() - 1] {
            assert!(((*p - pos2(192.0, 72.0)).length() - 7.0).abs() < 1e-3, "{p:?} off the arc");
            assert!(p.x >= 192.0 - 1e-3 && p.y >= 72.0 - 1e-3, "{p:?} in the wrong quarter");
        }
    }

    #[test]
    fn slider_like_python() {
        // Python: x = 8 + fraction * (width - 16); value = round(min + fraction * span).
        assert!((slider_x(190.0, 0, 100, 40) - 77.6).abs() < 1e-4);
        assert_eq!(slider_x(190.0, 0, 100, 150), 182.0);
        assert_eq!(slider_value(190.0, 0, 100, 77.6), 40);
        assert_eq!(slider_value(190.0, 0, 100, -50.0), 0);
        assert_eq!(slider_value(190.0, 0, 100, 500.0), 100);
        // Ties go to the even number, like Python's round().
        assert_eq!(slider_value(20.0, 0, 2, 9.0), 0);
        assert_eq!(slider_value(20.0, 0, 2, 11.0), 2);
    }

    #[test]
    fn slider_follows_click() {
        let ctx = egui::Context::default();
        let original = scheme("original");
        let mut value = 40;
        let mut place = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            place = ui.add(Slider::new(&mut value, 0, 100, original)).rect
        });
        let target = pos2(place.min.x + slider_x(SLIDER_WIDTH, 0, 100, 75), place.center().y);
        for input in click_at(target) {
            frame(&ctx, input, &mut |ui| {
                ui.add(Slider::new(&mut value, 0, 100, original));
            });
        }
        assert_eq!(value, 75, "slider at {place:?}");
    }

    fn frame(ctx: &egui::Context, input: egui::RawInput, ui_fn: &mut dyn FnMut(&mut Ui)) {
        let mut output = ctx.run_ui(input, |ui| ui_fn(ui));
        output.textures_delta.clear();
    }

    /// Every rectangle and the number of pictures (rectangles with a texture) of a frame; the letters are
    /// neither.
    fn marks(shapes: &[egui::epaint::ClippedShape]) -> (Vec<egui::epaint::RectShape>, usize) {
        fn collect(shape: &egui::epaint::Shape, rects: &mut Vec<egui::epaint::RectShape>, pictures: &mut usize) {
            match shape {
                egui::epaint::Shape::Rect(rect) if rect.brush.is_some() => *pictures += 1,
                egui::epaint::Shape::Rect(rect) => rects.push(rect.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, rects, pictures)),
                _ => {}
            }
        }
        let (mut rects, mut pictures) = (Vec::new(), 0);
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut rects, &mut pictures));
        (rects, pictures)
    }

    /// The search line and the plain field side by side, `frames` frames, the first one with the cursor in the
    /// search line when `focus`: what the last frame drew.
    fn lines(scheme: &Scheme, focus: bool, frames: usize) -> (Vec<egui::epaint::RectShape>, usize) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.set_visuals(crate::visuals(scheme));
        let (mut search, mut plain) = (String::new(), String::new());
        let mut drawn = (Vec::new(), 0);
        for n in 0..frames {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.horizontal(|row| {
                    let line = search_field(row, &mut search, "Suche", scheme, 160.0);
                    clearable_field(row, &mut plain, "Name", scheme, 160.0);
                    if focus && n == 0 {
                        line.request_focus();
                    }
                });
            });
            output.textures_delta.clear();
            drawn = marks(&output.shapes);
        }
        drawn
    }

    /// The search line has a muted border in the accent, a lighter ground and the magnifier, in every scheme;
    /// the plain field next to it has none of the three.
    #[test]
    fn a_search_line_stands_out_from_a_plain_field() {
        for scheme in theme::schemes() {
            let (rects, pictures) = lines(scheme, false, 4);
            let ground = color(scheme.search_ground());
            let border = color(scheme.search_border());
            let search = rects.iter().filter(|r| r.fill == ground && r.stroke.color == border).count();
            assert_eq!(search, 1, "{}: the muted border and the lighter ground: {rects:?}", scheme.id);
            let plain = rects.iter().filter(|r| r.fill == color(scheme.field) && r.stroke.color != border).count();
            assert!(plain >= 1, "{}: the plain field keeps its look: {rects:?}", scheme.id);
            assert_ne!(scheme.search_border(), scheme.accent, "{}: muted", scheme.id);
            assert_ne!(scheme.search_ground(), scheme.field, "{}: lighter", scheme.id);
            assert_eq!(pictures, 1, "{}: the magnifier and nothing else", scheme.id);
        }
    }

    /// With the cursor in it the search line has the full accent as its border (2 points).
    #[test]
    fn a_search_line_with_the_cursor_has_the_full_accent_as_its_border() {
        let scheme = theme::current();
        assert_eq!(theme::by_id("original").map(|s| s.accent.to_hex()).as_deref(), Some("#9ce430"));
        let (rects, pictures) = lines(scheme, true, 4);
        let full: Vec<_> = rects
            .iter()
            .filter(|r| r.fill == color(scheme.search_ground()) && r.stroke.color == color(scheme.accent))
            .collect();
        assert_eq!(full.len(), 1, "{rects:?}");
        assert_eq!(full[0].stroke.width, 2.0);
        assert_eq!(pictures, 1);
    }

    fn click_at(pos: Pos2) -> Vec<egui::RawInput> {
        let event = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![
            egui::RawInput { events: vec![egui::Event::PointerMoved(pos)], ..Default::default() },
            egui::RawInput { events: vec![event(true)], ..Default::default() },
            egui::RawInput { events: vec![event(false)], ..Default::default() },
        ]
    }

    #[test]
    fn toggle_flips_on_click() {
        let ctx = egui::Context::default();
        let krt = scheme("krt");
        let mut on = false;
        let mut place = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| place = ui.add(Toggle::new(&mut on, krt)).rect);
        for input in click_at(place.center()) {
            frame(&ctx, input, &mut |ui| {
                ui.add(Toggle::new(&mut on, krt));
            });
        }
        assert!(on, "toggle at {place:?} did not switch on");
    }

    #[test]
    fn every_widget_draws_in_every_scheme() {
        let ctx = egui::Context::default();
        for s in theme::schemes() {
            crate::apply(&ctx, s);
            let mut on = true;
            frame(&ctx, egui::RawInput::default(), &mut |ui| {
                heading(ui, s, "Überschrift");
                card(ui, s, |ui| {
                    ui.add(Toggle::new(&mut on, s));
                    for kind in
                        [ButtonKind::Normal, ButtonKind::Strong, ButtonKind::Danger, ButtonKind::Brand(KRT_ORANGE)]
                    {
                        ui.add(Button::new("Knopf", kind, s));
                    }
                });
                scroll_area(ui, s, ScrollArea::vertical().max_height(40.0), |ui| {
                    for i in 0..30 {
                        ui.label(format!("Zeile {i}"));
                    }
                });
                heat_map(ui, s, &["Mo", "Di"], &[vec![0.0, 10.0, 60.0], vec![30.0]]);
            });
        }
    }

    /// Clicks at `pos` over the frames of [`click_at`]; returns whether `add`
    /// reported a click in any of them.
    fn clicked_at(ctx: &egui::Context, pos: Pos2, add: &mut dyn FnMut(&mut Ui) -> bool) -> bool {
        let mut clicked = false;
        for input in click_at(pos) {
            frame(ctx, input, &mut |ui| clicked |= add(ui));
        }
        clicked
    }

    #[test]
    fn new_controls_draw_in_every_scheme() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for s in theme::schemes() {
            let (mut tab_chosen, mut on, mut text) = (1, true, "1200".to_string());
            frame(&ctx, egui::RawInput::default(), &mut |ui| {
                radio(ui, s, true, "Name");
                switch_row(ui, s, "nur fehlende", &mut on);
                tab_bar(ui, s, &["A", "B"], &mut tab_chosen);
                list_row(ui, s, true, "Arclight Pistole");
                progress(ui, s, 0.4, 200.0);
                collapsing("Fundorte").show(ui, |ui| ui.label("Daymar"));
                checked_field(ui, s, &mut text, "Qualität", 200.0, Some("Qualität geht nur von 0 bis 1000."));
                loading(ui, s, "Lädt …");
                notice(ui.ctx(), s, ui.input(|i| i.time), "circle-check", "Fertig.", Some("Rückgängig"));
                load_failed(ui, s, "Basetool nicht erreichbar.", "Nächster Versuch in 30 s");
                icon_button(ui, s, "pencil", "Bearbeiten");
                action_row(ui, s, "Arclight Pistole", &[("bookmark", "Merken")]);
                ui.add(Button::new("Eintragen", ButtonKind::Strong, s).blocked(Some("Erst ausfüllen.")));
            });
        }
    }

    #[test]
    fn foot_bar_actions_sit_at_the_right_and_answer_a_click() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        let long = "Spiel läuft · Stanton · 4.3.1 · ".repeat(20);
        let loading = Activity { text: &long, done: Some(0.5) };
        // (bar, report, news, clicked, status) of the last frame.
        let state = std::cell::RefCell::new((Rect::NOTHING, Rect::NOTHING, Rect::NOTHING, false, Rect::NOTHING));
        let mut draw = |ui: &mut Ui| {
            let mut st = state.borrow_mut();
            let (mut report, mut news, mut clicked) = (Rect::NOTHING, Rect::NOTHING, st.3);
            let (bar, status) = foot_bar(ui, s, &long, Some(&loading), |row| {
                let action = bar_action(row, s, "bug", "Fehler melden", "Diagnose", true);
                clicked |= action.clicked();
                report = action.rect;
                news = bar_action(row, s, "sparkles", "Was ist neu", "Änderungen", false).rect;
            });
            *st = (bar.rect, report, news, clicked, status);
        };
        // A real window width: the long text has to reach the actions.
        let sized = |mut input: egui::RawInput| {
            input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)));
            input
        };
        frame(&ctx, sized(egui::RawInput::default()), &mut draw);
        frame(&ctx, sized(egui::RawInput::default()), &mut draw);
        let (bar, report, news, _, status) = *state.borrow();
        assert!(bar.contains_rect(report) && bar.contains_rect(news), "actions inside the bar");
        assert!(status.right() < news.left(), "the long text at the left runs under the actions: {status:?} {news:?}");
        assert!(bar.right() - report.right() <= 10.0, "„Fehler melden“ at the right edge");
        assert!(news.right() <= report.left(), "the next action left of it");
        for input in click_at(report.center()) {
            frame(&ctx, sized(input), &mut draw);
        }
        assert!(state.borrow().3, "a click on „Fehler melden“ reaches it, even with a long text at the left");
    }

    #[test]
    fn window_parts_draw_in_every_scheme() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for s in theme::schemes() {
            frame(&ctx, egui::RawInput::default(), &mut |ui| {
                nav_section(ui, s, "Baupläne", false, false, |ui, _| {
                    nav_entry(ui, s, "factory", "Herstellung", false, Some("NEU"));
                    nav_entry(ui, s, "star", "Wunschliste", true, None);
                });
                let loading = Activity { text: "Basetool-Daten werden geladen …", done: Some(0.42) };
                for activity in [None, Some(&loading)] {
                    foot_bar(ui, s, "Spiel läuft", activity, |ui| {
                        bar_action(ui, s, "bug", "Fehler melden", "Öffnet die Diagnose", true);
                        bar_action(ui, s, "sparkles", "Was ist neu", "Die Änderungen", false);
                    });
                }
                pin_toggle(ui, s, false, ("Anheften", "Lösen"));
                pin_toggle(ui, s, true, ("Anheften", "Lösen"));
                saved_hint(ui, s, Some(ui.input(|i| i.time)), "Gespeichert");
                let chart = vec2(300.0, CHART_HEIGHT);
                line_chart(ui, s, &[("Mo", 1.0), ("Di", 0.0), ("Mi", 3.0)], chart, &|v| format!("{v} h"));
                line_chart(ui, s, &[], chart, &|v| format!("{v} h"));
            });
        }
    }

    /// A hit of the menu filter too long for the side bar wraps: every word is drawn, nothing is cut with „…“,
    /// the row grows and stays inside the side bar. A short name keeps the height of a row.
    #[test]
    fn a_long_entry_of_the_side_bar_wraps_instead_of_being_cut() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let long = "Verbindung & Abgleich › Welcher Account wird abgeglichen?";
        let mut rows = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(220.0, 400.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            rows.push(nav_entry(ui, s, "settings", "Allgemein", false, None).rect);
            rows.push(nav_entry(ui, s, "settings", long, false, None).rect);
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        fn collect(shape: &egui::epaint::Shape, into: &mut Vec<(String, Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    // What the rows show, not the text asked for: a cut row ends in „…“.
                    let shown: Vec<String> =
                        text.galley.rows.iter().map(|placed| placed.row.text().to_string()).collect();
                    let shown = shown.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                    into.push((shown, text.galley.rect.translate(text.pos.to_vec2())))
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        let drawn = texts.iter().find(|(text, _)| text.starts_with("Verbindung")).expect("the long entry");
        assert_eq!(drawn.0, long, "nothing cut: {texts:?}");
        assert!(drawn.1.right() <= rows[1].right() + 0.5, "inside the side bar: {drawn:?} {:?}", rows[1]);
        assert!(drawn.1.bottom() <= rows[1].bottom() + 0.5, "inside its row: {drawn:?} {:?}", rows[1]);
        assert_eq!(rows[0].height(), NAV_ROW_HEIGHT, "a short name keeps the height of a row");
        assert!(rows[1].height() > NAV_ROW_HEIGHT, "the long one grows: {:?}", rows[1]);
    }

    #[test]
    fn a_click_on_a_group_name_folds_the_group() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        // Height of the group at time `t`, and where its head is.
        let run_filtered = |input: egui::RawInput, head: &mut Rect, filtering: bool| -> f32 {
            let mut height = 0.0;
            frame(&ctx, input, &mut |ui| {
                let top = ui.cursor().min.y;
                nav_section(ui, s, "Lager", filtering, false, |ui, _| {
                    nav_entry(ui, s, "warehouse", "Mein Lager", false, None);
                    nav_entry(ui, s, "factory", "Herstellung", false, None);
                });
                height = ui.cursor().min.y - top;
                *head = Rect::from_min_size(pos2(0.0, top + 2.0), vec2(200.0, NAV_GROUP_HEIGHT));
            });
            height
        };
        let run = |input: egui::RawInput, head: &mut Rect| run_filtered(input, head, false);
        let at = |t: f64, events: Vec<egui::Event>| egui::RawInput { time: Some(t), events, ..Default::default() };
        let mut head = Rect::NOTHING;
        let mut open = 0.0;
        for t in [0.0, 0.1, 0.2, 0.3, 0.4] {
            open = run(at(t, vec![]), &mut head);
        }
        assert!(open > 2.0 * NAV_ROW_HEIGHT, "open with both entries: {open}");
        let pos = head.center();
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(at(0.5, vec![egui::Event::PointerMoved(pos)]), &mut head);
        run(at(0.55, vec![button(true)]), &mut head);
        run(at(0.6, vec![button(false)]), &mut head);
        let mut closed = open;
        for t in [0.7, 0.8, 0.9, 1.0, 1.2] {
            closed = run(at(t, vec![]), &mut head);
        }
        assert!(closed < NAV_GROUP_HEIGHT + 10.0, "only the head is left: {closed} (open {open})");
        // While the menu filter is used, a folded group shows its matches.
        let mut filtered = closed;
        for t in [1.3, 1.4, 1.5, 1.6, 1.8] {
            filtered = run_filtered(at(t, vec![]), &mut head, true);
        }
        assert!(filtered > 2.0 * NAV_ROW_HEIGHT, "open while filtering: {filtered}");
    }

    #[test]
    fn a_field_keeps_the_cursor_when_something_above_it_disappears() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut text = String::new();
        let mut id = egui::Id::NULL;
        // Like the menu filter: the entry above shows only while it matches.
        let step = |input: egui::RawInput, text: &mut String, id: &mut egui::Id| {
            frame(&ctx, input, &mut |ui| {
                if text.is_empty() || "übersicht".contains(text.as_str()) {
                    ui.label("Übersicht");
                }
                *id = clearable_field(ui, text, "Menü filtern …", s, 200.0).id;
            });
        };
        step(egui::RawInput::default(), &mut text, &mut id);
        ctx.memory_mut(|m| m.request_focus(id));
        step(egui::RawInput::default(), &mut text, &mut id);
        step(typed("raff"), &mut text, &mut id);
        step(egui::RawInput::default(), &mut text, &mut id);
        assert!(ctx.memory(|m| m.has_focus(id)), "the field keeps the cursor");
        step(typed("i"), &mut text, &mut id);
        assert_eq!(text, "raffi", "typing goes on in the same field");
    }

    #[test]
    fn a_folded_group_shows_only_the_open_page() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        set_nav_section_open(&ctx, "Abbau", false);
        let mut height = 0.0;
        for t in [0.0, 0.1, 0.2, 0.3, 0.4] {
            let input = egui::RawInput { time: Some(t), ..Default::default() };
            frame(&ctx, input, &mut |ui| {
                let top = ui.cursor().min.y;
                nav_section(ui, s, "Abbau", false, true, |ui, full| {
                    assert!(!full);
                    nav_entry(ui, s, "factory", "Raffinerien", true, None);
                });
                height = ui.cursor().min.y - top;
            });
        }
        assert!(height > NAV_GROUP_HEIGHT + NAV_ROW_HEIGHT - 1.0, "head and the open page: {height}");
        assert!(height < NAV_GROUP_HEIGHT + 2.0 * NAV_ROW_HEIGHT, "not more: {height}");
    }

    #[test]
    fn history_goes_back_and_forward_like_a_browser() {
        let mut h = History::new("start");
        assert!(!h.can_go_back() && !h.can_go_forward());
        h.visit("lager");
        h.visit("lager");
        h.visit("raffinerie");
        assert_eq!(h.back(), Some("lager"), "the same page twice counts once");
        assert_eq!(h.back(), Some("start"));
        assert_eq!(h.back(), None);
        assert_eq!(h.forward(), Some("lager"));
        h.visit("routen");
        assert!(!h.can_go_forward(), "a new page drops what lay ahead");
        assert_eq!(h.back(), Some("lager"));
    }

    const HISTORY_TEXTS: HistoryTexts<'static> =
        HistoryTexts { back: "Zurück", forward: "Vor", no_back: "Keine vorige", no_forward: "Keine nächste" };

    #[test]
    fn alt_arrows_and_the_mouse_side_buttons_walk_the_history() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut history = History::new("start");
        history.visit("lager");
        history.visit("routen");
        let mut field = String::new();
        let mut field_id = egui::Id::NULL;
        let step = |input: egui::RawInput, history: &mut History, field: &mut String, id: &mut egui::Id| {
            let mut went = None;
            frame(&ctx, input, &mut |ui| {
                went = history_buttons(ui, s, history, &HISTORY_TEXTS);
                *id = clearable_field(ui, field, "Suche", s, 200.0).id;
            });
            went
        };
        let alt_left = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            ..Default::default()
        };
        step(egui::RawInput::default(), &mut history, &mut field, &mut field_id);
        assert_eq!(step(alt_left.clone(), &mut history, &mut field, &mut field_id), Some("lager".to_string()));
        let side_button = egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos: pos2(300.0, 300.0),
                button: egui::PointerButton::Extra1,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        assert_eq!(step(side_button, &mut history, &mut field, &mut field_id), Some("start".to_string()));
        // While typing in a field, Alt+← belongs to the field.
        history.visit("routen");
        ctx.memory_mut(|m| m.request_focus(field_id));
        step(egui::RawInput::default(), &mut history, &mut field, &mut field_id);
        assert_eq!(step(alt_left, &mut history, &mut field, &mut field_id), None);
        assert_eq!(history.current(), "routen");
    }

    #[test]
    fn a_step_back_closes_what_lies_on_top_before_it_leaves_the_page() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut history = History::new("start");
        history.visit("laeden");
        let alt_left = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            ..Default::default()
        };
        let step = |input: egui::RawInput, history: &mut History, inner: Option<&str>| {
            let mut went = None;
            frame(&ctx, input, &mut |ui| went = history_buttons_inner(ui, s, history, &HISTORY_TEXTS, inner));
            went
        };
        step(egui::RawInput::default(), &mut history, None);
        // A part lies open on the page: the step closes it, the page stays.
        assert_eq!(step(alt_left.clone(), &mut history, Some("Zurück zur Liste")), Some(HistoryStep::Inner));
        assert_eq!(history.current(), "laeden");
        // Nothing open: the step leaves the page.
        assert_eq!(step(alt_left.clone(), &mut history, None), Some(HistoryStep::Page("start".into())));
        // At the start of the history, with something open, the arrow still closes it.
        assert_eq!(step(alt_left, &mut history, Some("Zurück zur Liste")), Some(HistoryStep::Inner));
    }

    #[test]
    fn a_dragged_group_lands_before_the_one_under_the_line() {
        let mut order = vec!["baupläne", "logistik", "herstellung", "abbau", "handel"];
        // The miner: `Abbau` before `Baupläne`.
        move_before(&mut order, 3, 0);
        assert_eq!(order, ["abbau", "baupläne", "logistik", "herstellung", "handel"]);
        // Down: `abbau` before `handel` (index 4).
        move_before(&mut order, 0, 4);
        assert_eq!(order, ["baupläne", "logistik", "herstellung", "abbau", "handel"]);
        // To the end.
        move_before(&mut order, 0, 5);
        assert_eq!(order, ["logistik", "herstellung", "abbau", "handel", "baupläne"]);
        // On its own place, or out of range: nothing.
        let before = order.clone();
        move_before(&mut order, 2, 2);
        move_before(&mut order, 2, 3);
        move_before(&mut order, 9, 0);
        move_before(&mut order, 0, 9);
        assert_eq!(order, before);
    }

    #[test]
    fn choosing_several_rows_with_ctrl_and_shift() {
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        let shift = egui::Modifiers { shift: true, ..Default::default() };
        let mut rows = Selection::new(5);
        rows.click(1, none);
        assert_eq!((rows.count(), rows.is_chosen(1)), (1, true));
        rows.click(3, ctrl);
        assert_eq!(rows.count(), 2, "Ctrl adds");
        rows.click(3, ctrl);
        assert_eq!((rows.count(), rows.is_chosen(3)), (1, false), "Ctrl again removes");
        // Shift counts from the row clicked last.
        rows.click(1, none);
        rows.click(4, shift);
        assert_eq!((1..=4).filter(|r| rows.is_chosen(*r)).count(), 4, "Shift chooses 1 to 4");
        assert!(!rows.is_chosen(0));
        rows.click(2, none);
        assert_eq!(rows.count(), 1, "a plain click chooses one");
        rows.click(9, none);
        assert_eq!(rows.count(), 1, "a row that does not exist changes nothing");
        rows.clear();
        assert_eq!(rows.count(), 0);
    }

    /// Height `reveal` takes up at the time `t` (seconds) after opening at 0.
    fn revealed_height(ctx: &egui::Context, t: f64, open: bool) -> f32 {
        let mut height = 0.0;
        let input = egui::RawInput { time: Some(t), ..Default::default() };
        frame(ctx, input, &mut |ui| {
            let top = ui.cursor().min.y;
            reveal(ui, "test", open, |ui| {
                ui.add_space(100.0);
            });
            height = ui.cursor().min.y - top;
        });
        height
    }

    #[test]
    fn reveal_grows_and_shrinks_over_a_quarter_second() {
        let ctx = egui::Context::default();
        assert_eq!(revealed_height(&ctx, 0.0, false), 0.0);
        let heights: Vec<f32> =
            [0.0, 0.05, 0.1, 0.15, 0.2, 0.3, 0.5].iter().map(|t| revealed_height(&ctx, *t, true)).collect();
        assert!(heights.windows(2).all(|w| w[1] >= w[0]), "only grows: {heights:?}");
        assert!(heights.iter().any(|h| *h > 5.0 && *h < 95.0), "a moment half open: {heights:?}");
        assert!(*heights.last().unwrap_or(&0.0) >= 100.0, "fully open in the end: {heights:?}");
        let closing: Vec<f32> = [0.6, 0.65, 0.7, 1.0].iter().map(|t| revealed_height(&ctx, *t, false)).collect();
        assert!(closing.iter().any(|h| *h > 5.0 && *h < 95.0), "closes gradually: {closing:?}");
        assert_eq!(*closing.last().unwrap_or(&1.0), 0.0);
    }

    #[test]
    fn a_blocked_button_does_nothing_and_looks_greyed() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("krt");
        let mut place = Rect::NOTHING;
        let blocked = |ui: &mut Ui| ui.add(Button::new("Eintragen", ButtonKind::Strong, s).blocked(Some("Grund.")));
        frame(&ctx, egui::RawInput::default(), &mut |ui| place = blocked(ui).rect);
        assert!(!clicked_at(&ctx, place.center(), &mut |ui| blocked(ui).clicked()));
        let free = |ui: &mut Ui| ui.add(Button::new("Eintragen", ButtonKind::Strong, s).blocked(None));
        assert!(clicked_at(&ctx, place.center(), &mut |ui| free(ui).clicked()), "without a reason it works");
        assert_eq!(blocked_colors(s), ButtonColors { text: s.sub, fill: s.surface, border: s.line });
    }

    #[test]
    fn retry_counts_down_from_thirty_seconds() {
        assert_eq!(retry_in(100.0, 100.0), 30.0);
        assert_eq!(retry_in(100.0, 112.5), 17.5);
        assert_eq!(retry_in(100.0, 131.0), 0.0);
        assert_eq!(retry_in(100.0, 90.0), 30.0, "a clock running backwards does not count up");
    }

    #[test]
    fn row_buttons_appear_only_under_the_mouse_and_report_which() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let actions = [("bookmark", "Merken"), ("trash-2", "Löschen")];
        let mut place = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            let top = ui.cursor().min;
            assert_eq!(action_row(ui, s, "Zeile", &actions), None);
            place = Rect::from_min_max(top, pos2(top.x + ui.available_width(), ui.cursor().min.y));
        });
        // ⚠ Proves only that a click on a shown button reports its action,
        // not that the buttons stay hidden without the mouse.
        let delete_at = pos2(place.right() - 4.0 - ICON_BUTTON_SIZE / 2.0, place.center().y);
        let mut hit = None;
        for input in click_at(delete_at) {
            frame(&ctx, input, &mut |ui| {
                if let Some(i) = action_row(ui, s, "Zeile", &actions) {
                    hit = Some(i);
                }
            });
        }
        assert_eq!(hit, Some(1), "the rightmost button is the last action");
    }

    #[test]
    fn the_undo_button_of_a_notice_reports_its_click() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut since = 0.0;
        let mut button = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| since = ui.input(|i| i.time));
        // Find the button: the notice sits at the top right; its last widget is the button.
        for _ in 0..2 {
            frame(&ctx, egui::RawInput::default(), &mut |ui| {
                notice(ui.ctx(), s, since, "trash-2", "Gelöscht.", Some("Rückgängig"));
            });
        }
        ctx.memory(|m| {
            if let Some(rect) = m.area_rect(egui::Id::unique(("versekit-notice", "Gelöscht."))) {
                button = Rect::from_min_max(pos2(rect.right() - 110.0, rect.top()), rect.max);
            }
        });
        assert!(button.is_positive(), "the notice is on screen");
        let mut acted = false;
        let target = pos2(button.right() - 60.0, button.center().y);
        for input in click_at(target) {
            frame(&ctx, input, &mut |ui| {
                acted |= notice(ui.ctx(), s, since, "trash-2", "Gelöscht.", Some("Rückgängig")).acted;
            });
        }
        assert!(acted);
        let gone = notice(&ctx, s, since - NOTICE_SECONDS - 1.0, "trash-2", "Gelöscht.", None);
        assert_eq!(gone, Notice { shown: false, acted: false }, "after six seconds it is gone");
    }

    /// The tab row with actions: at every width, in every scheme and with any
    /// panel open, no action overlaps a tab or leaves the window, and the
    /// open action tells screen readers it is on.
    #[test]
    fn page_actions_keep_clear_of_the_tabs() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        let names = ["Sichern", "Einlesen", "Leeren"];
        let overlap = |a: &egui::accesskit::Rect, b: &egui::accesskit::Rect| {
            a.x0 < b.x1 - 0.5 && b.x0 < a.x1 - 0.5 && a.y0 < b.y1 - 0.5 && b.y0 < a.y1 - 0.5
        };
        for id in ["original", "krt"] {
            let s = scheme(id);
            for width in (240..=700).step_by(5) {
                for open in [None, Some(0), Some(2)] {
                    let actions: Vec<PageAction<'_>> = names
                        .iter()
                        .enumerate()
                        .map(|(i, text)| PageAction { icon: "download", text, on: open == Some(i) })
                        .collect();
                    let input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width as f32, 400.0))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        let mut chosen = 0;
                        tab_bar_with_actions(ui, s, &["Material (1234)", "Items (4040)"], &mut chosen, &actions);
                    });
                    output.textures_delta.clear();
                    let nodes = output.platform_output.accesskit_update.expect("accesskit is on").nodes;
                    let tabs: Vec<_> =
                        nodes.iter().filter(|(_, n)| n.role() == Role::Tab).filter_map(|(_, n)| n.bounds()).collect();
                    assert_eq!(tabs.len(), 2, "{id} {width}");
                    for (i, name) in names.iter().enumerate() {
                        let (_, node) = nodes
                            .iter()
                            .find(|(_, n)| n.role() == Role::Button && n.label() == Some(*name))
                            .unwrap_or_else(|| panic!("{name} fehlt bei {id} {width}"));
                        let b = node.bounds().expect("bounds");
                        assert!(
                            b.x0 >= -0.5 && b.x1 <= f64::from(width) + 0.5,
                            "{name} ragt hinaus: {id} {width} {b:?}"
                        );
                        for t in &tabs {
                            assert!(!overlap(&b, t), "{name} überdeckt einen Reiter: {id} {width} {open:?}");
                        }
                        let on = node.toggled() == Some(egui::accesskit::Toggled::True);
                        assert_eq!(on, open == Some(i), "{name} meldet offen falsch: {id} {width}");
                    }
                }
            }
        }
    }

    /// What a screen reader gets for one element: role, name, value, chosen.
    type ReaderNode = (Role, Option<String>, Option<String>, Option<bool>);

    /// The nodes a screen reader gets for one frame.
    fn screen_reader_nodes(ctx: &egui::Context, ui_fn: &mut dyn FnMut(&mut Ui)) -> Vec<ReaderNode> {
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| ui_fn(ui));
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("accesskit is on");
        update
            .nodes
            .iter()
            .map(|(_, node)| {
                let chosen = node.toggled().map(|t| t == egui::accesskit::Toggled::True);
                (node.role(), node.label().map(str::to_owned), node.value().map(str::to_owned), chosen)
            })
            .collect()
    }

    /// The language choice carries its flag before the name, as in 4.12; every
    /// flag file has the height of its line size.
    #[test]
    fn a_language_choice_shows_its_flag() {
        for code in FLAG_CODES {
            for size in FLAG_SIZES {
                let png = flag_png(code, size).expect("flag file");
                let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
                assert_eq!(height, size, "{code}-{size}.png");
            }
        }
        // Every translation's flag has its files; the original texts show the English one.
        use versekit_core::translation;
        for source in translation::SOURCES.iter() {
            let code = translation::flag_of(source.id).expect(source.id);
            assert!(FLAG_CODES.contains(&code), "{}: {code}", source.id);
        }
        assert_eq!(translation::flag_of(translation::ORIGINAL), Some("gb"));
        assert_eq!(translation::flag_of(translation::CUSTOM), None);
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let (mut plain, mut flagged, mut pictures) = (0.0, 0.0, 0);
        let (mut button, mut button_flagged, mut lead) = (0.0, 0.0, Vec2::ZERO);
        // The pictures are decoded next to the frames: frames until they are there (at most ten
        // seconds, the whole test suite may be running beside it).
        for _ in 0..500 {
            if pictures >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                plain = radio(ui, s, true, "Deutsch").rect.width();
                flagged = radio_flag(ui, s, true, "Deutsch", "de").rect.width();
                button = ui.add(Button::new("Français", ButtonKind::Normal, s)).rect.width();
                button_flagged = ui.add(Button::new("Français", ButtonKind::Normal, s).flag("fr")).rect.width();
                lead = flag(ui, "fr").expect("a flag").1;
            });
            output.textures_delta.clear();
            pictures = output
                .shapes
                .iter()
                .filter(|c| {
                    !matches!(c.shape, egui::Shape::Text(_)) && c.shape.texture_id() != egui::TextureId::default()
                })
                .count();
        }
        assert!((flagged - plain - lead.x - 6.0).abs() < 0.5, "{plain} {flagged} {lead:?}");
        assert!((button_flagged - button - lead.x - 6.0).abs() < 0.5, "{button} {button_flagged} {lead:?}");
        assert!(pictures >= 2, "both flags are drawn: {pictures}");

        // The file fits the pixels of the line: at 100 % and zoomed to 125 % a file of the line's
        // height in pixels, drawn at the line's height in points.
        for scale in [1.0, 1.25] {
            ctx.set_pixels_per_point(scale);
            let (mut size, mut line) = (Vec2::ZERO, 0.0);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                size = flag(ui, "de").expect("a flag").1;
                line = TextStyle::Body.resolve(ui.style()).size;
            });
            output.textures_delta.clear();
            let pixels = size.y * scale;
            assert!(FLAG_SIZES.iter().any(|f| (*f as f32 - pixels).abs() < 0.01), "{scale}: {pixels} px is no file");
            assert!((pixels - line * scale).abs() <= 1.0, "{scale}: {pixels} px for a line of {} px", line * scale);
        }
    }

    #[test]
    fn radio_and_drop_downs_have_a_name_for_screen_readers() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut chosen = [0, 2];
        let mut words = [("Strg", true), ("Alt", false)];
        let mut key = 1;
        let nodes = screen_reader_nodes(&ctx, &mut |ui| {
            radio(ui, s, true, "Deutsch");
            radio(ui, s, false, "English");
            filter_bar(ui, "test", &[&["Alle Arten", "Waffe"], &["Alle Größen", "S1", "S2"]], &mut chosen);
            hotkey_picker(ui, s, "test", &mut words, &["F7", "F8"], &mut key, "Overlay ein- und ausblenden");
            ui.add(Button::new("Bestätigen", ButtonKind::Strong, s));
            ui.add(Button::new("Speichern", ButtonKind::Normal, s).blocked(Some("Erst etwas ändern.")));
            nav_entry(ui, s, "box", "Mein Lager", true, None);
        });
        let find = |role: Role, name: &str| {
            nodes.iter().find(|(r, n, _, _)| *r == role && n.as_deref() == Some(name)).cloned()
        };
        assert_eq!(find(Role::RadioButton, "Deutsch").map(|n| n.3), Some(Some(true)));
        assert_eq!(find(Role::RadioButton, "English").map(|n| n.3), Some(Some(false)));
        assert_eq!(find(Role::ComboBox, "Alle Arten").and_then(|n| n.2).as_deref(), Some("Alle Arten"));
        assert_eq!(find(Role::ComboBox, "Alle Größen").and_then(|n| n.2).as_deref(), Some("S2"));
        assert_eq!(find(Role::ComboBox, "Overlay ein- und ausblenden").and_then(|n| n.2).as_deref(), Some("F8"));
        assert!(find(Role::Button, "Bestätigen").is_some(), "button named by its label");
        assert!(find(Role::Button, "Speichern").is_some(), "a blocked button keeps its name");
        assert_eq!(find(Role::Button, "Mein Lager").map(|n| n.3), Some(Some(true)), "open page is chosen");

        // Counter check: a plain egui drop-down has no name, and the test sees that.
        let plain = screen_reader_nodes(&ctx, &mut |ui| {
            egui::ComboBox::from_id_salt("ohne-namen").selected_text("S2").show_ui(ui, |_| {});
        });
        let combo = plain.iter().find(|(r, _, _, _)| *r == Role::ComboBox).expect("drop-down is there");
        assert_eq!(combo.1, None, "egui gives a drop-down no name of its own");
    }

    #[test]
    fn radio_tab_and_list_row_react_to_clicks() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut place = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| place = radio(ui, s, false, "Name").rect);
        assert!(clicked_at(&ctx, place.center(), &mut |ui| radio(ui, s, false, "Name").clicked()));
        frame(&ctx, egui::RawInput::default(), &mut |ui| place = list_row(ui, s, false, "Zeile").rect);
        assert!(clicked_at(&ctx, place.center(), &mut |ui| list_row(ui, s, false, "Zeile").clicked()));

        let mut chosen = 0;
        let mut second = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            ui.horizontal(|ui| {
                tab(ui, s, true, "Übersicht");
                second = tab(ui, s, false, "Lager").rect;
            });
        });
        let mut changed = false;
        for input in click_at(second.center()) {
            frame(&ctx, input, &mut |ui| changed |= tab_bar(ui, s, &["Übersicht", "Lager"], &mut chosen));
        }
        assert!(changed);
        assert_eq!(chosen, 1);
    }

    #[test]
    fn the_switch_of_a_switch_row_sits_at_the_right_edge() {
        let ctx = egui::Context::default();
        let s = scheme("krt");
        let mut on = false;
        let mut row = Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            ui.set_width(400.0);
            row = switch_row(ui, s, "nur fehlende", &mut on).rect;
        });
        assert!((row.right() - 400.0).abs() < 1.0, "switch at {row:?}");
        for input in click_at(row.center()) {
            frame(&ctx, input, &mut |ui| {
                ui.set_width(400.0);
                switch_row(ui, s, "nur fehlende", &mut on);
            });
        }
        assert!(on);
    }

    #[test]
    fn focus_comes_by_tab_not_by_click() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = scheme("original");
        let mut button = None;
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            button = Some(ui.add(Button::new("Knopf", ButtonKind::Normal, s)));
        });
        let place = button.as_ref().map_or(Rect::NOTHING, |b| b.rect);
        for input in click_at(place.center()) {
            frame(&ctx, input, &mut |ui| button = Some(ui.add(Button::new("Knopf", ButtonKind::Normal, s))));
        }
        assert!(!button.as_ref().is_some_and(Response::has_focus), "a click shows no focus frame");
        frame(&ctx, keys(&[egui::Key::Tab]), &mut |ui| {
            button = Some(ui.add(Button::new("Knopf", ButtonKind::Normal, s)));
        });
        frame(&ctx, egui::RawInput::default(), &mut |ui| {
            button = Some(ui.add(Button::new("Knopf", ButtonKind::Normal, s)));
        });
        assert!(button.as_ref().is_some_and(Response::has_focus), "Tab puts the focus on the button");
    }

    #[test]
    fn an_on_release_slider_changes_its_value_only_on_release() {
        let ctx = egui::Context::default();
        let s = scheme("original");
        let mut value = 1;
        let mut place = Rect::NOTHING;
        let mut shown = None;
        let step = |input: egui::RawInput, value: &mut i64, place: &mut Rect, shown: &mut Option<i64>| {
            frame(&ctx, input, &mut |ui| {
                let response = ui.add(Slider::new(value, 0, 7, s).width(140.0).on_release());
                (*place, *shown) = (response.rect, Slider::shown(ui, &response));
            });
        };
        step(egui::RawInput::default(), &mut value, &mut place, &mut shown);
        let (start, end) = (pos2(place.left() + 25.0, place.center().y), pos2(place.right() - 2.0, place.center().y));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let input = |events| egui::RawInput { events, ..Default::default() };
        step(input(vec![egui::Event::PointerMoved(start)]), &mut value, &mut place, &mut shown);
        step(input(vec![button(start, true)]), &mut value, &mut place, &mut shown);
        step(input(vec![egui::Event::PointerMoved(end)]), &mut value, &mut place, &mut shown);
        step(input(vec![egui::Event::PointerMoved(end)]), &mut value, &mut place, &mut shown);
        assert_eq!((value, shown), (1, Some(7)), "while holding: unchanged, but the knob shows 7");
        step(input(vec![button(end, false)]), &mut value, &mut place, &mut shown);
        assert_eq!((value, shown), (7, None), "on release the value is taken");
    }

    #[test]
    fn arrow_keys_move_a_focused_slider() {
        let ctx = egui::Context::default();
        let s = scheme("original");
        // A second slider to the right must keep its value. ⚠ This does not
        // prove the key filter: in this headless setup egui did not move the
        // focus by arrow keys even without it.
        let (mut value, mut other) = (40, 10);
        let mut id = egui::Id::NULL;
        let two = |input: egui::RawInput, value: &mut i64, other: &mut i64, id: &mut egui::Id| {
            frame(&ctx, input, &mut |ui| {
                ui.horizontal(|ui| {
                    *id = ui.add(Slider::new(value, 0, 100, s)).id;
                    ui.add(Slider::new(other, 0, 100, s));
                });
            });
        };
        two(egui::RawInput::default(), &mut value, &mut other, &mut id);
        ctx.memory_mut(|m| m.request_focus(id));
        two(egui::RawInput::default(), &mut value, &mut other, &mut id);
        for key in [egui::Key::ArrowRight, egui::Key::ArrowRight, egui::Key::ArrowLeft] {
            two(keys(&[key]), &mut value, &mut other, &mut id);
        }
        assert_eq!((value, other), (41, 10));
        assert!(ctx.memory(|m| m.has_focus(id)), "the arrows did not move the focus away");
    }

    #[test]
    fn up_down_page_home_and_end_move_a_focused_slider_too() {
        let ctx = egui::Context::default();
        let s = scheme("original");
        let mut value = 40;
        let mut id = egui::Id::NULL;
        let one = |input: egui::RawInput, value: &mut i64, id: &mut egui::Id| {
            frame(&ctx, input, &mut |ui| {
                *id = ui.add(Slider::new(value, 0, 100, s)).id;
            });
        };
        one(egui::RawInput::default(), &mut value, &mut id);
        ctx.memory_mut(|m| m.request_focus(id));
        one(egui::RawInput::default(), &mut value, &mut id);
        one(keys(&[egui::Key::ArrowUp]), &mut value, &mut id);
        assert_eq!(value, 41, "up is like right");
        one(keys(&[egui::Key::ArrowDown]), &mut value, &mut id);
        one(keys(&[egui::Key::ArrowDown]), &mut value, &mut id);
        assert_eq!(value, 39, "down is like left");
        one(keys(&[egui::Key::PageUp]), &mut value, &mut id);
        assert_eq!(value, 49, "page up is a tenth of the way");
        one(keys(&[egui::Key::PageDown]), &mut value, &mut id);
        one(keys(&[egui::Key::PageDown]), &mut value, &mut id);
        assert_eq!(value, 29);
        one(keys(&[egui::Key::End]), &mut value, &mut id);
        assert_eq!(value, 100);
        one(keys(&[egui::Key::Home]), &mut value, &mut id);
        assert_eq!(value, 0);
        assert!(ctx.memory(|m| m.has_focus(id)));
    }

    fn keys(keys: &[egui::Key]) -> egui::RawInput {
        let events = keys
            .iter()
            .map(|&key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect();
        egui::RawInput { events, ..Default::default() }
    }

    fn typed(text: &str) -> egui::RawInput {
        egui::RawInput { events: vec![egui::Event::Text(text.to_string())], ..Default::default() }
    }

    /// One field with suggestions over several frames.
    struct SuggestRig {
        ctx: egui::Context,
        text: String,
        rect: Rect,
        id: egui::Id,
        picked: Option<usize>,
        open: bool,
    }

    const SHIPS: [&str; 3] = ["Buccaneer", "Buzzard", "Super Hornet"];

    impl SuggestRig {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let mut rig = SuggestRig {
                ctx,
                text: String::new(),
                rect: Rect::NOTHING,
                id: egui::Id::NULL,
                picked: None,
                open: false,
            };
            rig.step(egui::RawInput::default());
            rig.ctx.memory_mut(|m| m.request_focus(rig.id));
            rig.step(egui::RawInput::default());
            rig
        }

        fn step(&mut self, input: egui::RawInput) {
            let more = |n: usize| format!("+{n}");
            let ctx = self.ctx.clone();
            let (text, rect, id, picked, open) =
                (&mut self.text, &mut self.rect, &mut self.id, &mut self.picked, &mut self.open);
            frame(&ctx, input, &mut |ui| {
                ui.add_space(20.0);
                let shown = SuggestField::new("feld", "Schiff, z. B. Buzzard", scheme("original"), &more)
                    .width(300.0)
                    .show(ui, text, &SHIPS);
                (*rect, *id, *picked, *open) = (shown.response.rect, shown.response.id, shown.picked, shown.is_open);
            });
        }

        fn has_focus(&self) -> bool {
            self.ctx.memory(|m| m.has_focus(self.id))
        }
    }

    #[test]
    fn suggest_field_picks_with_arrow_and_enter() {
        let mut rig = SuggestRig::new();
        rig.step(typed("bu"));
        assert!(rig.open, "after “bu” the suggestions are open");
        // Order: Buzzard (shorter), Buccaneer; one down picks Buccaneer.
        rig.step(keys(&[egui::Key::ArrowDown, egui::Key::Enter]));
        assert_eq!((rig.picked, rig.text.as_str()), (Some(0), "Buccaneer"));
        assert!(!rig.open);
        rig.step(egui::RawInput::default());
        assert!(!rig.open, "the picked name does not open the list again");
        assert!(rig.has_focus(), "the cursor stays in the field");
    }

    #[test]
    fn suggest_field_closes_with_escape_until_the_input_changes() {
        let mut rig = SuggestRig::new();
        rig.step(typed("bu"));
        rig.step(keys(&[egui::Key::Escape]));
        rig.step(egui::RawInput::default());
        assert!(!rig.open);
        assert!(rig.has_focus(), "Esc closes the list, not the field");
        rig.step(typed("z"));
        assert!(rig.open);
    }

    #[test]
    fn suggest_field_click_on_a_suggestion_picks_it() {
        let mut rig = SuggestRig::new();
        rig.step(typed("bu"));
        rig.step(egui::RawInput::default());
        // The first row lies right below the field.
        let first_row = pos2(rig.rect.left() + 40.0, rig.rect.bottom() + 16.0);
        for input in click_at(first_row) {
            rig.step(input);
            if rig.picked.is_some() {
                break;
            }
        }
        assert_eq!((rig.picked, rig.text.as_str()), (Some(1), "Buzzard"));
    }

    #[test]
    fn the_x_empties_the_field_and_keeps_the_cursor() {
        let mut rig = SuggestRig::new();
        rig.step(typed("Buzz"));
        let x = pos2(rig.rect.right() - 4.0 - CLEAR_SIZE / 2.0, rig.rect.center().y);
        for input in click_at(x) {
            rig.step(input);
        }
        assert_eq!(rig.text, "");
        assert!(rig.has_focus());
    }

    /// One select field over several frames, holding "Buzzard" at the start.
    struct SelectRig {
        ctx: egui::Context,
        text: String,
        rect: Rect,
        id: egui::Id,
        picked: Option<usize>,
        cleared: bool,
        open: bool,
    }

    const PARTS: [&str; 4] = ["Werksteil: Buzzard", "Buccaneer", "Super Hornet", "Hornet Tracker"];

    impl SelectRig {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let mut rig = SelectRig {
                ctx,
                text: "Buccaneer".to_string(),
                rect: Rect::NOTHING,
                id: egui::Id::NULL,
                picked: None,
                cleared: false,
                open: false,
            };
            rig.step(egui::RawInput::default());
            rig
        }

        fn step(&mut self, input: egui::RawInput) {
            let ctx = self.ctx.clone();
            let (text, rect, id, picked, cleared, open) =
                (&mut self.text, &mut self.rect, &mut self.id, &mut self.picked, &mut self.cleared, &mut self.open);
            frame(&ctx, input, &mut |ui| {
                ui.add_space(20.0);
                let shown = SelectField::new("wahl", "Teil", "Teil suchen", "Nichts gefunden", scheme("original"))
                    .width(300.0)
                    .show(ui, text, &PARTS);
                (*rect, *id, *picked, *cleared, *open) =
                    (shown.response.rect, shown.response.id, shown.picked, shown.cleared, shown.is_open);
            });
        }

        fn click(&mut self, at: Pos2) {
            for input in click_at(at) {
                self.step(input);
            }
        }

        fn arrow(&self) -> Pos2 {
            pos2(self.rect.right() - 4.0 - SELECT_ARROW / 2.0, self.rect.center().y)
        }
    }

    #[test]
    fn select_matches_keeps_the_order_and_needs_every_word() {
        assert_eq!(select_matches("", &PARTS), vec![0, 1, 2, 3]);
        assert_eq!(select_matches("hornet", &PARTS), vec![2, 3]);
        assert_eq!(select_matches("HORNET super", &PARTS), vec![2]);
        assert!(select_matches("x-wing", &PARTS).is_empty());
    }

    #[test]
    fn the_arrow_opens_the_whole_list_and_the_keys_pick() {
        let mut rig = SelectRig::new();
        assert!(!rig.open);
        rig.click(rig.arrow());
        assert!(rig.open, "the arrow opens the list");
        // The list starts at the value it holds (Buccaneer); two down is Hornet Tracker.
        rig.step(keys(&[egui::Key::ArrowDown, egui::Key::ArrowDown, egui::Key::Enter]));
        assert_eq!((rig.picked, rig.text.as_str()), (Some(3), "Hornet Tracker"));
        assert!(!rig.open);
        rig.step(egui::RawInput::default());
        assert!(!rig.open, "the picked value does not open the list again");
    }

    #[test]
    fn typing_narrows_the_list_and_esc_puts_the_value_back() {
        let mut rig = SelectRig::new();
        rig.click(rig.rect.center());
        assert!(rig.open, "a click into the field opens the list");
        rig.step(keys(&[egui::Key::End]));
        for _ in 0..9 {
            rig.step(keys(&[egui::Key::Backspace]));
        }
        rig.step(typed("tracker"));
        rig.step(keys(&[egui::Key::Enter]));
        assert_eq!((rig.picked, rig.text.as_str()), (Some(3), "Hornet Tracker"), "only the match is left");
        rig.click(rig.rect.center());
        rig.step(typed(" xyz"));
        assert!(rig.open);
        rig.step(keys(&[egui::Key::Escape]));
        assert_eq!(rig.text, "Hornet Tracker", "Esc puts the value back");
        assert!(!rig.open);
    }

    #[test]
    fn a_click_on_a_row_picks_and_a_click_elsewhere_puts_the_value_back() {
        let mut rig = SelectRig::new();
        rig.click(rig.arrow());
        rig.step(egui::RawInput::default());
        // The first row lies right below the field: the stock part.
        rig.click(pos2(rig.rect.left() + 40.0, rig.rect.bottom() + 16.0));
        assert_eq!(rig.text, "Werksteil: Buzzard");
        rig.click(rig.arrow());
        rig.step(typed("x"));
        rig.click(pos2(550.0, 380.0));
        assert_eq!(rig.text, "Werksteil: Buzzard", "leaving without a pick keeps the value");
        assert!(!rig.open);
    }

    #[test]
    fn the_x_empties_the_field_and_shows_every_entry() {
        let mut rig = SelectRig::new();
        let x = pos2(rig.rect.right() - 4.0 - SELECT_ARROW - 2.0 - CLEAR_SIZE / 2.0, rig.rect.center().y);
        rig.click(x);
        assert_eq!(rig.text, "");
        assert!(rig.open, "the emptied field shows the list");
        assert!(rig.ctx.memory(|m| m.has_focus(rig.id)), "the cursor stays in the field");
    }

    #[test]
    fn a_click_into_empty_space_ends_the_input() {
        let mut rig = SuggestRig::new();
        rig.step(typed("Bu"));
        assert!(rig.has_focus());
        for input in click_at(pos2(550.0, 380.0)) {
            rig.step(input);
        }
        assert!(!rig.has_focus());
    }
}
