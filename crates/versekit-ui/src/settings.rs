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
    Align, CornerRadius, CursorIcon, InnerResponse, Layout, Rect, Role, Sense, Stroke, StrokeKind, TextStyle,
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

/// One setting in one line: its name at the left with the symbol `info` behind it, the control in the column of
/// controls right after the names; the hint stands on hover over the row, never as a grey line below (the house
/// rule, enforced here for every page). After a jump from the menu filter to its
/// name or its hint the row frames itself in the accent (see
/// [`crate::highlight`]); `lit` says that the jump leads to this row under
/// another name (the row shows a text other than the one the filter found).
///
/// The column is [`CONTROL_WIDTH`] wide and its left edge is the same in every row of a card; the controls are
/// laid out from there to the right (what does not fit goes to a second line) and stand at that edge, whatever
/// their own width.
pub fn setting_row<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    lit: bool,
    control: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    row(ui, scheme, name, hint, lit, CONTROL_WIDTH, control)
}

/// A small heading in a card: a name in small muted text, with its hint on hover behind the symbol `info`, over the
/// rows it is about. What would be a settings row without a control is one of these (the check of every page finds
/// such a row). Framed after a jump from the menu filter to its name or hint.
pub fn subheading(ui: &mut Ui, scheme: &Scheme, name: &str, hint: &str) {
    let line = ui.horizontal(|line| {
        line.label(egui::RichText::new(name).small().color(color(scheme.sub)));
        let icon = line.text_style_height(&TextStyle::Small);
        if !hint.is_empty()
            && let Some(image) = crate::icons::themed("info", scheme, IconTint::Normal, icon)
        {
            line.add(image);
        }
    });
    let rect = line.response.rect;
    if !hint.is_empty() {
        ui.interact(rect, ui.auto_id_with(("subheading", name)), Sense::hover()).on_hover_text(hint);
    }
    crate::highlight::mark(ui, scheme, rect, &[name, hint]);
    ui.add_space(4.0);
}

/// A setting in one line: the same as [`setting_row`] (every setting stands in one line, its hint on hover behind
/// the symbol `info` after the name, never as a grey line below).
pub fn setting_line<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    lit: bool,
    control: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    row(ui, scheme, name, hint, lit, CONTROL_WIDTH, control)
}

fn row<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    name: &str,
    hint: &str,
    lit: bool,
    wanted: f32,
    control: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let top = ui.cursor().min;
    let width = ui.available_width();
    // As tall as a button: every row is as tall, whatever its control, and its name stands in the middle of it.
    let line_height = widgets::button_size(ui, scheme, ButtonKind::Normal, "Ag").y.max(widgets::TOGGLE_SIZE.y);
    // A row too narrow for the column and a name keeps a name of 80 points; the column takes the rest.
    let column_width = wanted.min((width - GAP - 80.0).max(0.0));
    // The column stands right after the names, at the same place in every row of a card, not at its far edge.
    let names = widgets::name_column(width, column_width);
    let column = Rect::from_min_size(pos2(top.x + names + GAP, top.y), vec2(column_width, line_height));
    let mut controls = ui.new_child(
        egui::UiBuilder::new().max_rect(column).layout(Layout::left_to_right(Align::Center).with_main_wrap(true)),
    );
    let inner = control(&mut controls);
    let used = controls.min_rect();
    // The name takes the column of names (the whole row without a control) and wraps before the symbol `info`.
    let text_right = if used.width() > 0.0 { column.left() - GAP } else { top.x + width };
    let text_area = Rect::from_min_max(top, pos2(text_right.max(top.x + 80.0), top.y + f32::MAX / 4.0));
    let mut text = ui.new_child(egui::UiBuilder::new().max_rect(text_area).layout(Layout::top_down(Align::Min)));
    text.allocate_ui_with_layout(vec2(text_area.width(), line_height), Layout::left_to_right(Align::Center), |ui| {
        let icon = ui.text_style_height(&TextStyle::Small);
        let info = !hint.is_empty();
        // The symbol `info` keeps its place after the name: the name wraps before it.
        let room = ui.available_width() - if info { icon + ui.spacing().item_spacing.x } else { 0.0 };
        ui.scope(|ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(name).wrap());
        });
        if info && let Some(image) = crate::icons::themed("info", scheme, IconTint::Normal, icon) {
            ui.add(image);
        }
    });
    let bottom = text.min_rect().bottom().max(used.bottom());
    let rect = Rect::from_min_max(top, pos2(top.x + width, bottom));
    #[cfg(test)]
    {
        // A line with a column of its own width (choices and a number in one line) is not a row of the column.
        if used.width() > 0.0 && wanted == CONTROL_WIDTH {
            let column = Rect::from_min_max(column.min, pos2(column.max.x, bottom));
            column_log::row(rect, column, used, text.min_rect(), Rect::NOTHING);
        }
        column_log::named(name);
    }
    // The row as content for the checks: its names and its whole column of controls. A row without a control is no
    // setting: by the house rules it is a small heading over what follows, and the check finds it.
    widgets::drawn_cards::note_row(Rect::from_min_max(top, pos2(column.max.x, bottom)));
    if used.width() <= 0.0 {
        widgets::drawn_cards::note_bare_row(rect);
    }
    // The hint stands whole on the row.
    let response = ui.allocate_rect(rect, Sense::hover());
    let response = if hint.is_empty() { response } else { response.on_hover_text(hint) };
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
        /// The line of the hint (`Rect::NOTHING` without one).
        pub hint: Rect,
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

    pub fn row(row: Rect, column: Rect, used: Rect, text: Rect, hint: Rect) {
        ROWS.with(|rows| rows.borrow_mut().push(Row { row, column, used, text, hint }));
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

/// The name of a group of settings above its rows: the title of its card ([`widgets::card_title`]).
pub fn setting_group(ui: &mut Ui, scheme: &Scheme, title: &str) {
    widgets::card_title(ui, scheme, title);
    ui.add_space(2.0);
}

/// Narrowest a card of settings gets before the cards stack.
pub const SETTINGS_COLUMN_MIN: f32 = 420.0;
/// The width a card of setting rows asks for: the names, the column of controls and the margins of the card. A wide
/// window gets as many columns of cards as fit at this width (up to four), so that no card runs empty to the right.
pub const SETTINGS_CARD_WIDTH: f32 = crate::layout::ROW_CELL + 2.0 * GAP;
/// The most columns of cards a page gets.
const MOST_CARD_COLUMNS: usize = 4;

/// How many columns of cards a page `width` wide gets: one where two would be narrower than
/// [`SETTINGS_COLUMN_MIN`], else as many as fit at [`SETTINGS_CARD_WIDTH`], two at least, [`MOST_CARD_COLUMNS`] at
/// most.
pub fn card_columns(width: f32) -> usize {
    if width < 2.0 * SETTINGS_COLUMN_MIN + GAP {
        return 1;
    }
    crate::layout::columns_that_fit(width, SETTINGS_CARD_WIDTH, MOST_CARD_COLUMNS).max(2)
}

/// A settings page: each group a card of its own, as tall as its content, in columns like a wall of bricks (the
/// next card goes into the shortest column; two columns, more in a wide window, see [`card_columns`]); if the window
/// is too narrow for two, the cards stack. These rules hold for every page that lays its cards out here: a card is
/// never stretched, never wider than a column, and short cards stand side by side.
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

/// [`settings_page`] where the groups in `wide` (by number) get a line of their own and the whole width: only for a
/// content that needs it (a table, an editor), never for a card of rows.
pub fn settings_page_wide(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    groups: &[(&str, usize)],
    wide: &[usize],
    row: &mut dyn FnMut(&mut Ui, usize, usize),
) -> Vec<Rect> {
    let mut cards = Vec::new();
    let count = card_columns(ui.available_width());
    let side_by_side = count > 1;
    // One card of a group, as tall as its content; its height is kept for the next frame.
    let mut card = |ui: &mut Ui, g: usize| {
        let (title, rows) = groups[g];
        let shown = widgets::card(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            setting_group(ui, scheme, title);
            (0..rows).for_each(|r| row(ui, g, r));
        });
        let id = egui::Id::unique(("settings-card", id_salt, g));
        let height = shown.response.rect.height();
        if ui.ctx().data(|d| d.get_temp::<f32>(id)).is_none_or(|before| (before - height).abs() > 0.5) {
            ui.ctx().data_mut(|d| d.insert_temp(id, height));
            ui.ctx().request_repaint();
        }
        cards.push((g, shown.response.rect));
    };
    let mut g = 0;
    while g < groups.len() {
        if g > 0 {
            ui.add_space(GAP);
        }
        // A wide group, or a window too narrow for two: one card over the whole width.
        if !side_by_side || wide.contains(&g) {
            card(ui, g);
            g += 1;
            continue;
        }
        // The groups up to the next wide one in columns like a wall of bricks: each card goes under the one of the
        // shortest column (the heights of the frame before; of equal ones the left), none is stretched.
        let end = (g..groups.len()).find(|i| wide.contains(i)).unwrap_or(groups.len());
        let height = |ui: &Ui, i: usize| -> f32 {
            ui.ctx().data(|d| d.get_temp(egui::Id::unique(("settings-card", id_salt, i)))).unwrap_or(0.0)
        };
        // Always the columns of the page, also for fewer cards: a card keeps the width it asks for and does not run
        // empty over a wider column.
        let mut columns: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut filled = vec![0.0_f32; count];
        for i in g..end {
            let shortest = (0..count).fold(0, |best, c| if filled[c] + 0.5 < filled[best] { c } else { best });
            columns[shortest].push(i);
            filled[shortest] += height(ui, i) + GAP;
        }
        // Then a card goes over to the shortest column while that makes the tallest one shorter: the columns end about
        // level (a long card early on no longer leaves the other column empty below). The order of the groups stays
        // within each column.
        for _ in 0..(end - g) * count {
            let tall = (0..count).fold(0, |best, c| if filled[c] > filled[best] { c } else { best });
            let short = (0..count).fold(0, |best, c| if filled[c] < filled[best] { c } else { best });
            let gap = filled[tall] - filled[short];
            let best = columns[tall]
                .iter()
                .enumerate()
                .map(|(k, i)| (k, height(ui, *i) + GAP))
                .filter(|(_, taken)| *taken + 0.5 < gap)
                .min_by(|a, b| (gap - 2.0 * a.1).abs().total_cmp(&(gap - 2.0 * b.1).abs()));
            let Some((k, taken)) = best else { break };
            let moved = columns[tall].remove(k);
            columns[short].push(moved);
            columns[short].sort_unstable();
            filled[tall] -= taken;
            filled[short] += taken;
        }
        let top = ui.cursor().min;
        let width = ui.available_width();
        let column_width = ((width - (count - 1) as f32 * GAP) / count as f32).floor();
        let mut bottom = top.y;
        for (c, column) in columns.iter().enumerate() {
            let left = top.x + c as f32 * (column_width + GAP);
            let place = Rect::from_min_size(pos2(left, top.y), vec2(column_width, f32::MAX / 4.0));
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(place).layout(Layout::top_down(Align::Min)));
            for (k, i) in column.iter().enumerate() {
                if k > 0 {
                    inner.add_space(GAP);
                }
                card(&mut inner, *i);
            }
            bottom = bottom.max(inner.min_rect().bottom());
        }
        ui.advance_cursor_after_rect(Rect::from_min_max(top, pos2(top.x + width, bottom)));
        g = end;
    }
    // In the order of the groups, whatever column each stands in.
    cards.sort_by_key(|(g, _)| *g);
    cards.into_iter().map(|(_, rect)| rect).collect()
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

/// Width of the field of places: two corners side by side, the row of free movement as wide.
pub const PLACE_SQUARE: f32 = 180.0;
/// Height of a row of two corners.
const CORNER_HEIGHT: f32 = 60.0;
/// Height of the row of free movement between the two rows of corners.
const FREE_HEIGHT: f32 = 34.0;
/// Height of the whole field: corners, free movement, corners, a seam between each.
const PLACE_HEIGHT: f32 = 2.0 * CORNER_HEIGHT + FREE_HEIGHT + 2.0 * PLACE_SEAM;
/// About the width of the whole block: the field and the radio buttons of the bar beside it.
pub const PLACE_BLOCK_WIDTH: f32 = PLACE_SQUARE + GAP + 150.0;
/// Space between the pieces that together form the field.
const PLACE_SEAM: f32 = 2.0;
/// The name of the choice of the place, for the check that one piece stands out at most.
const PLACE_CHOICE: &str = "overlay-platz";

/// The block that places the overlay, small and symmetric, in three rows without gaps: the two top corners, free
/// movement over the whole width, the two bottom corners. No piece lies over another, and only the place chosen
/// stands out. Beside it the bar, on top or at the bottom, as two radio buttons (a choice of its own, which does not
/// stand out like a place). A corner at the bottom puts the bar at the bottom, one at the top on top; the radio
/// buttons change it afterwards. Returns whether anything changed.
pub fn overlay_place(
    ui: &mut Ui,
    scheme: &Scheme,
    place: &mut OverlayPlace,
    bar: &mut BarSide,
    texts: &OverlayTexts,
) -> bool {
    let mut changed = false;
    ui.horizontal_top(|ui| {
        let (square, _) = ui.allocate_exact_size(vec2(PLACE_SQUARE, PLACE_HEIGHT), Sense::hover());
        for (k, (spot, align)) in CORNERS.into_iter().enumerate() {
            let chosen = *place == spot;
            widgets::drawn_cards::note_choice(PLACE_CHOICE, chosen);
            // The chosen corner shows the bar as set, the others as they would start.
            let shown = if chosen { *bar } else { corner_bar(align) };
            if corner_piece(ui, scheme, corner_rect(square, align), texts.corners[k], align, shown, chosen) {
                *place = spot;
                *bar = corner_bar(align);
                changed = true;
            }
        }
        // Free movement: the row between the corners.
        let free = *place == OverlayPlace::Free;
        widgets::drawn_cards::note_choice(PLACE_CHOICE, free);
        if free_piece(ui, scheme, free_rect(square), texts.free, free) {
            *place = OverlayPlace::Free;
            changed = true;
        }
        ui.add_space(GAP);
        ui.vertical(|ui| {
            for (side, name) in [(BarSide::Top, texts.bar_top), (BarSide::Bottom, texts.bar_bottom)] {
                if widgets::radio(ui, scheme, *bar == side, name).clicked() && *bar != side {
                    *bar = side;
                    changed = true;
                }
            }
        });
    });
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

/// A corner piece: half of the top or the bottom row, a seam apart from the other.
fn corner_rect(square: Rect, align: egui::Align2) -> Rect {
    align.align_size_within_rect(vec2((PLACE_SQUARE - PLACE_SEAM) / 2.0, CORNER_HEIGHT), square)
}

/// The piece for free movement: the middle row over the whole width, a seam from the corners above and below.
fn free_rect(square: Rect) -> Rect {
    Rect::from_center_size(square.center(), vec2(PLACE_SQUARE, FREE_HEIGHT))
}

/// The piece for free movement: the move symbol and the name in one line; chosen = accent frame and dark accent
/// area, like a corner.
fn free_piece(ui: &mut Ui, scheme: &Scheme, rect: Rect, text: &str, chosen: bool) -> bool {
    let response = ui.interact(rect, ui.make_persistent_id(("overlay-place", "free")), Sense::click());
    let (fill, border, width) = piece_colors(scheme, chosen, response.hovered());
    let radius = control_radius(scheme);
    let painter = ui.painter();
    painter.rect(rect, radius, color(fill), Stroke::new(width, color(border)), StrokeKind::Inside);
    let small = TextStyle::Small.resolve(ui.style());
    let galley = painter.layout_no_wrap(text.to_string(), small, color(scheme.fg));
    let icon = 16.0;
    let left = rect.center().x - (icon + 4.0 + galley.size().x) / 2.0;
    let icon_rect = Rect::from_min_size(pos2(left, rect.center().y - icon / 2.0), Vec2::splat(icon));
    painter.galley(pos2(icon_rect.max.x + 4.0, rect.center().y - galley.size().y / 2.0), galley, color(scheme.fg));
    let tone = if chosen { scheme.accent } else { scheme.sub };
    if let Some(image) = crate::icons::image("move", tone, icon) {
        image.paint_at(ui, icon_rect);
    }
    response.widget_info(|| egui::WidgetInfo::selected(Role::RadioButton, true, chosen, text));
    widgets::focus_ring(ui, &response, scheme, radius);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Fill, frame and frame width of a piece.
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
    corner.align_size_within_rect(vec2(26.0, 17.0), rect.shrink(8.0))
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
    response.widget_info(|| egui::WidgetInfo::selected(Role::RadioButton, true, chosen, text));
    widgets::focus_ring(ui, &response, scheme, radius);
    response.on_hover_text(text).on_hover_cursor(CursorIcon::PointingHand).clicked()
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

    /// The title of a group stands as written in the accent of every scheme, the color the contrast check
    /// measures, and one step larger than the rows.
    #[test]
    fn the_title_of_a_group_stands_as_written_in_the_accent_of_every_scheme() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for s in theme::schemes() {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 200.0))),
                ..Default::default()
            };
            let mut body = 0.0;
            let mut output = ctx.run_ui(input, |ui| {
                body = egui::TextStyle::Body.resolve(ui.style()).size;
                setting_group(ui, s, "Info-Leiste");
            });
            output.textures_delta.clear();
            let colors: Vec<(egui::Color32, f32)> = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Info-Leiste" => {
                        text.galley.job.sections.first().map(|section| (section.format.color, section.format.font_id.size))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(colors.len(), 1, "{}", s.id);
            assert_eq!(colors[0].0, color(widgets::section_title_color(s)), "{}", s.id);
            assert_eq!(widgets::section_title_color(s), s.accent, "{}", s.id);
            assert!(colors[0].1 > body, "{}: title {} not larger than the rows {body}", s.id, colors[0].1);
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
        let square = Rect::from_min_size(top, vec2(PLACE_SQUARE, PLACE_HEIGHT));
        let mut at = |pos| {
            click(&ctx, pos, &mut |ui| {
                overlay_place(ui, s, &mut place, &mut bar, &TEXTS);
            });
            (place, bar)
        };
        // The radio buttons of the bar stand beside the square, the one on top above.
        let radios = square.max.x + GAP + 8.0;
        // A bottom corner puts the bar at the bottom first, a top corner on top.
        assert_eq!(at(square.right_bottom() - vec2(10.0, 10.0)), (OverlayPlace::BottomRight, BarSide::Bottom));
        assert_eq!(at(pos2(radios, square.top() + 8.0)), (OverlayPlace::BottomRight, BarSide::Top), "the bar changes");
        assert_eq!(at(square.left_top() + vec2(10.0, 10.0)), (OverlayPlace::TopLeft, BarSide::Top));
        assert_eq!(at(square.left_bottom() + vec2(10.0, -10.0)), (OverlayPlace::BottomLeft, BarSide::Bottom));
        assert_eq!(at(square.right_top() + vec2(-10.0, 10.0)).0, OverlayPlace::TopRight);
        // Free in the middle keeps the bar, also at the ends of its row.
        assert_eq!(at(square.center()), (OverlayPlace::Free, BarSide::Top), "the middle is free");
        assert_eq!(at(square.left_center() + vec2(4.0, 0.0)).0, OverlayPlace::Free, "the row reaches the edge");
        assert_eq!(at(pos2(radios, square.top() + 30.0)).1, BarSide::Bottom);
    }

    #[test]
    fn the_corners_and_free_movement_fill_the_field_without_lying_over_each_other() {
        let square = Rect::from_min_size(pos2(0.0, 0.0), vec2(PLACE_SQUARE, PLACE_HEIGHT));
        let corners: Vec<Rect> = CORNERS.iter().map(|(_, align)| corner_rect(square, *align)).collect();
        let free = free_rect(square);
        // Five pieces a seam apart: no gap but the seams, nothing outside the field.
        let area: f32 = corners.iter().map(|c| c.area()).sum::<f32>() + free.area();
        let seams = 2.0 * PLACE_SQUARE * PLACE_SEAM + 2.0 * CORNER_HEIGHT * PLACE_SEAM;
        assert!((square.area() - area - seams).abs() < 2.0, "{area} and seams of {}", square.area());
        assert!(corners.iter().chain([&free]).all(|c| square.contains_rect(*c)));
        assert_eq!(free.center(), square.center(), "symmetric");
        // No piece lies over another.
        for (k, a) in corners.iter().enumerate() {
            assert!(!a.intersects(free.shrink(0.5)), "corner {k} over free movement");
            for b in &corners[k + 1..] {
                assert!(!a.shrink(0.5).intersects(b.shrink(0.5)), "corners over each other");
            }
        }
    }

    #[test]
    fn one_place_stands_out_at_a_time() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let s = theme::by_id("original").expect("scheme");
        for chosen in [OverlayPlace::TopRight, OverlayPlace::Free] {
            let (mut place, mut bar) = (chosen, BarSide::Top);
            widgets::drawn_cards::take_choices();
            frame(&ctx, vec![], &mut |ui| {
                overlay_place(ui, s, &mut place, &mut bar, &TEXTS);
            });
            let pieces = widgets::drawn_cards::take_choices();
            assert_eq!(pieces.len(), 5, "four corners and free");
            assert_eq!(pieces.iter().filter(|(_, on)| *on).count(), 1, "{chosen:?}: {pieces:?}");
        }
    }

    #[test]
    fn the_small_overlay_sits_in_the_corner_it_stands_for() {
        let square = Rect::from_min_size(pos2(0.0, 0.0), vec2(PLACE_SQUARE, PLACE_HEIGHT));
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
    fn the_free_name_fits_its_piece() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        frame(&ctx, vec![], &mut |_| {});
        frame(&ctx, vec![], &mut |ui| {
            let small = TextStyle::Small.resolve(ui.style());
            let size = ui.painter().layout_no_wrap(TEXTS.free.to_string(), small, egui::Color32::WHITE).size();
            let piece = free_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(PLACE_SQUARE, PLACE_HEIGHT)));
            assert!(16.0 + 4.0 + size.x < piece.width() - 8.0, "„{}“ is too wide for its piece", TEXTS.free);
            assert!(size.y < piece.height() - 4.0, "„{}“ is too tall for its piece", TEXTS.free);
        });
    }

    #[test]
    fn group_cards_stand_like_a_wall_of_bricks_each_as_tall_as_its_content() {
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
        assert!(b.height() > a.height() + 20.0, "one row and three rows: each as tall as its content: {a:?} {b:?}");
        // The third goes under the shorter column, the one of the first group.
        assert!((c.left() - a.left()).abs() < 0.5 && c.top() > a.bottom(), "under the shorter card: {a:?} {c:?}");
        assert!(c.top() < b.bottom(), "right under it, not under the taller one: {b:?} {c:?}");
        assert!((c.width() - a.width()).abs() < 1.0, "a card keeps the width of its column");
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
