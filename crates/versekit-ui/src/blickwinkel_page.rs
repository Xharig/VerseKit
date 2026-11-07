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

//! The page "Blickwinkel" (`_view_angle` and `fov_window.py` of 4.12): which field of view suits the screen, and
//! where the player would have to sit for the one set in the game. The maths is `versekit_core::fov`.
//!
//! **Measuring the screen by hand.** The player holds a bank card to the screen and drags a rectangle to its
//! size. The measuring is a mode of the whole window (it needs the whole screen): the window goes full screen
//! on the screen it stands on, draws the card, and "Passt" reads its own width in pixels, which is the pixel
//! width of that screen, not of some other one. If the window did not go to full size (nothing is saved, the
//! player is told): a window width taken for the screen would make the whole calculation wrong.
//!
//! Nothing is written to the game here: the page reads `attributes.xml` and keeps three numbers of its own in
//! the settings of the program.

use egui::{RichText, Ui, ViewportCommand};
use versekit_core::formats;
use versekit_core::fov::{self, Analysis, Calibration, Game, GameSetting, Grade, Verdict};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::settings::{self, Said};
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How often the settings and the file of the game are read again, in seconds.
const LOOK_SECONDS: f64 = 1.0;
/// The card is never drawn smaller than this many pixels wide.
const MIN_CARD_PX: i64 = 120;
/// The largest width of the slider in pixels (4.12: 900, too little for a card on a very dense screen).
const MAX_SLIDER_PX: i64 = 1400;
/// The width of the card the measuring starts with, where nothing was measured before.
const START_CARD_PX: i64 = 320;
/// How long the window may take to go to full screen before it is taken as it is, in seconds.
const FULLSCREEN_WAIT: f64 = 1.5;
/// The shortest the slider of the measuring gets on a small window, in points.
const MIN_SLIDER_WIDTH: f32 = 120.0;
/// The height of the bar at the bottom of the measuring with the slider and the buttons, in points.
const BAR_HEIGHT: f32 = 64.0;
/// The window must be as wide as the screen, give or take this many points, to count as full screen: the state
/// "full screen" is what was asked for (it is true at once), only the width says what was reached.
const FULL_TOLERANCE: f32 = 2.0;
/// The widest card the measuring starts with, whatever the calibration says.
const MAX_START_PX: i64 = 5000;
/// The widest card the keys and the mouse make (a screen is not wider than this in pixels).
const MAX_CARD_PX: i64 = 20_000;
/// The page counts as left when it was not drawn for this long, in seconds.
const LEFT_SECONDS: f64 = 2.0;

/// The measuring that is under way.
struct Measure {
    /// The width of the card as drawn, in physical pixels.
    card_px: i64,
    opened_at: f64,
}

/// What the page keeps.
#[derive(Default)]
pub struct FovState {
    calibration: Calibration,
    game: GameSetting,
    looked_at: Option<f64>,
    /// The distance as typed, in centimetres.
    distance_text: String,
    /// The typed text was taken from the settings once (what is typed later is not overwritten).
    distance_loaded: bool,
    measure: Option<Measure>,
    /// A text to read in a window (the result of the measuring).
    dialog: Option<String>,
    notice: Option<Said>,
    /// When the page was drawn last: a longer gap means it was left and is shown again.
    drawn_at: Option<f64>,
}

impl FovState {
    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Is the screen being measured (the whole window is the measuring then)?
    pub fn measuring(&self) -> bool {
        self.measure.is_some()
    }

    /// Reads the settings and the file again at the next look.
    pub fn invalidate(&mut self) {
        self.looked_at = None;
        self.distance_loaded = false;
    }

    fn refresh(&mut self, ui: &Ui, paths: &Paths) {
        let now = ui.input(|i| i.time);
        if self.looked_at.is_some_and(|at| now - at < LOOK_SECONDS) {
            return;
        }
        self.looked_at = Some(now);
        self.calibration = fov::stored(paths);
        self.game = fov::game_setting(paths, None);
        if !self.distance_loaded {
            self.distance_text = self.calibration.distance_mm.map(fov::centimetres_text).unwrap_or_default();
            self.distance_loaded = true;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(LOOK_SECONDS));
    }

    /// Starts the measuring: the window goes full screen, the card as wide as last measured. What was typed
    /// in the field of the distance and not left yet is kept first (the field is not drawn while measuring).
    fn start_measuring(&mut self, ui: &Ui, paths: &Paths) {
        self.commit_typed(paths);
        let now = ui.input(|i| i.time);
        // A calibration edited by hand (0, or tiny) must not give a card as wide as the number allows.
        let card_px = self
            .calibration
            .mm_per_pixel
            .map_or(START_CARD_PX, |mm| (fov::CARD_WIDTH_MM / mm).round() as i64)
            .clamp(MIN_CARD_PX, MAX_START_PX);
        self.measure = Some(Measure { card_px, opened_at: now });
        ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(true));
        ui.ctx().request_repaint();
    }

    fn stop_measuring(&mut self, ctx: &egui::Context) {
        self.measure = None;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        self.invalidate();
        ctx.request_repaint();
    }

    /// Ends the measuring without a word (the window is full screen and has nothing to draw it with).
    pub fn abort_measuring(&mut self, ctx: &egui::Context) {
        if self.measure.is_some() {
            self.stop_measuring(ctx);
        }
    }

    /// What is typed in the field and differs from what is kept is kept now (the field is left by other means than
    /// Enter: the button for measuring, another page, the end of the program).
    fn commit_typed(&mut self, paths: &Paths) {
        if !self.distance_loaded || self.distance_text.trim().is_empty() {
            return;
        }
        let kept = self.calibration.distance_mm.map(fov::centimetres_text).unwrap_or_default();
        if self.distance_text.trim() != kept {
            self.commit_distance(paths);
        }
    }

    /// Is what stands in the field no distance? (Seen while typing, so the field says so at once and the sentence
    /// goes as soon as it is right. An empty field is no mistake.)
    fn distance_invalid(&self) -> bool {
        let text = self.distance_text.trim();
        !text.is_empty() && fov::typed_distance_mm(text).is_none()
    }

    /// The distance was typed and left: kept when it is a number above 0. What is no number stays in the field with
    /// its red line, the kept distance holds. An emptied field shows the kept distance again.
    fn commit_distance(&mut self, paths: &Paths) {
        let text = self.distance_text.trim();
        if text.is_empty() {
            self.distance_text = self.calibration.distance_mm.map(fov::centimetres_text).unwrap_or_default();
            return;
        }
        if let Some(mm) = fov::typed_distance_mm(text) {
            if fov::remember(paths, None, None, Some(mm)) {
                self.calibration.distance_mm = Some(mm);
                self.distance_text = fov::centimetres_text(mm);
            } else {
                // Not kept: the field keeps what was typed, and the page says that it was not saved.
                self.notice = Some(Said::fault(t("s_fv_f_speichern")));
            }
        }
    }
}

fn lang() -> language::Lang {
    language::current()
}

/// A line of the page: the name and, below, its help at the left, the value in the column of controls in bold.
fn value_row(ui: &mut Ui, scheme: &Scheme, name: &str, value: &str, hint: &str, hue: Option<egui::Color32>) {
    let hue = hue.unwrap_or_else(|| color(scheme.fg));
    settings::setting_row(ui, scheme, name, hint, false, |row| {
        row.add(egui::Label::new(RichText::new(value).strong().color(hue)).wrap());
    });
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut FovState, paths: &Paths) {
    // Shown again after a stay elsewhere: what was typed and not left is kept before the settings are read anew.
    let now = ui.input(|i| i.time);
    if state.drawn_at.is_some_and(|at| now - at > LEFT_SECONDS) {
        state.commit_typed(paths);
        state.invalidate();
    }
    state.drawn_at = Some(now);
    state.refresh(ui, paths);
    ui.add(egui::Label::new(RichText::new(t("s_fv_lead")).small().color(color(scheme.sub))).wrap());
    ui.add_space(8.0);

    // One card for the screen: the explanation, the button for measuring, then the width and the distance (or the
    // sentence that nothing is measured yet).
    let width_mm = state.calibration.width_mm();
    let measure = widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_fv_messen_kopf"));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(t("s_fv_erklaerung")).small()).wrap());
        ui.add_space(10.0);
        let word = if width_mm.is_some() { t("s_fv_neu_messen") } else { t("s_fv_messen") };
        let kind = if width_mm.is_some() { ButtonKind::Normal } else { ButtonKind::Strong };
        if ui.add(widgets::Button::new(&word, kind, scheme)).clicked() {
            return true;
        }
        ui.add_space(8.0);
        let Some(width_mm) = width_mm else {
            ui.add(egui::Label::new(RichText::new(t("s_fv_nicht_gemessen")).small().color(color(scheme.sub))).wrap());
            return false;
        };
        value_row(
            ui,
            scheme,
            &t("s_fv_breite"),
            &format!("{} cm", formats::fixed(lang(), width_mm / 10.0, 1)),
            "",
            None,
        );

        // The distance of the eyes.
        let (name, hint) = (t("s_fv_abstand"), t("s_fv_abstand_hilfe"));
        let mut left = false;
        settings::setting_row(ui, scheme, &name, &hint, false, |row| {
            let response =
                widgets::clearable_field(row, &mut state.distance_text, &t("s_fv_abstand_platz"), scheme, 100.0);
            row.label(t("s_pl_abstand"));
            let (enabled, shown) = (response.enabled(), state.distance_text.clone());
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, &name);
                info.current_text_value = Some(shown.clone());
                info
            });
            // Kept on Enter and on leaving the field, not at every key: "6" on the way to "60" is no distance.
            left = response.lost_focus();
            // A wrong text: the edge of the field is red at once.
            if state.distance_invalid() {
                let red = egui::Stroke::new(1.5, color(scheme.red));
                row.painter().rect_stroke(response.rect.expand(1.0), 3.0, red, egui::StrokeKind::Outside);
            }
        });
        if left {
            state.commit_distance(paths);
        }
        if state.distance_invalid() {
            ui.label(RichText::new(t("s_fv_abstand_ungueltig")).small().color(color(scheme.red)));
        }
        false
    })
    .inner;
    if measure {
        state.start_measuring(ui, paths);
        return;
    }
    let Some(width_mm) = width_mm else {
        dialog(ui, scheme, state);
        return;
    };

    if let Some(distance_mm) = state.calibration.distance_mm
        && let Some(analysis) = fov::analyse(width_mm, distance_mm, &state.game)
    {
        ui.add_space(8.0);
        results(ui, scheme, &analysis);
    }
    dialog(ui, scheme, state);
}

/// The lines with what the numbers say, and the verdict in a box, all in one card.
fn results(ui: &mut Ui, scheme: &Scheme, analysis: &Analysis) {
    let accent = color(scheme.accent);
    let gold = color(scheme.gold);
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_fv_ergebnis_kopf"));
        ui.add_space(4.0);
        let neutral = format!("{}°", formats::fixed(lang(), analysis.neutral, 1));
        if analysis.unreachable {
            let narrowest = analysis.narrowest.unwrap_or(0.0);
            let hint = tf("s_fv_neutral_unerreichbar", &[("0", Arg::from(narrowest))]);
            value_row(ui, scheme, &t("s_fv_neutral"), &neutral, &hint, Some(gold));
        } else {
            value_row(ui, scheme, &t("s_fv_neutral"), &neutral, &t("s_fv_neutral_hilfe"), Some(accent));
        }
        let Game::Known { in_game, optimal, .. } = analysis.game else {
            ui.add(egui::Label::new(RichText::new(t("s_fv_kein_spielwert")).small().color(color(scheme.sub))).wrap());
            return;
        };
        value_row(ui, scheme, &t("s_fv_im_spiel"), &format!("{}°", formats::fixed(lang(), in_game, 0)), "", None);
        if let Some(optimal) = optimal.filter(|_| !analysis.unreachable) {
            let value = format!("{} cm", formats::fixed(lang(), optimal / 10.0, 0));
            value_row(ui, scheme, &t("s_fv_optimalpunkt"), &value, &t("s_fv_optimalpunkt_hilfe"), None);
        }
        let Game::Known { verdict: Some(verdict), .. } = analysis.game else { return };
        let (text, hue) = match verdict {
            Verdict::AtMinimum => (t("s_fv_am_minimum"), scheme.accent),
            Verdict::ToMinimum(narrowest) => (tf("s_fv_zum_minimum", &[("0", Arg::from(narrowest))]), scheme.gold),
            Verdict::Good => (t("s_fv_passt_gut"), scheme.accent),
            Verdict::TooFar(cm, grade) => (tf("s_fv_zu_weit", &[("0", Arg::from(cm))]), grade_color(scheme, grade)),
            Verdict::TooNear(cm, grade) => (tf("s_fv_zu_nah", &[("0", Arg::from(cm))]), grade_color(scheme, grade)),
        };
        ui.add_space(6.0);
        // The first sentence is the verdict, the rest says what it means (a muted line below).
        let (head, line) = text.split_once(". ").map_or((text.as_str(), ""), |(head, line)| (head, line));
        let head = if line.is_empty() { head.to_string() } else { format!("{head}.") };
        let icon =
            if matches!(verdict, Verdict::Good | Verdict::AtMinimum) { "circle-check" } else { "triangle-alert" };
        verdict_box(ui, scheme, hue, icon, &head, line);
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(t("s_fv_hinweis_deutung")).small().color(color(scheme.sub))).wrap());
    });
}

/// The box of a lasting state like `widgets::status_box` (frame, bar, symbol, bold sentence, muted line below),
/// but the texts wrap: a verdict cut short with dots would be no verdict.
pub(crate) fn verdict_box(ui: &mut Ui, scheme: &Scheme, tone: Color, icon: &str, head: &str, line: &str) {
    let hue = color(tone);
    let inner = egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if let Some(image) = crate::icons::image(icon, tone, 18.0) {
                    ui.add(image);
                }
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(head).strong().color(hue)).wrap());
                    if !line.is_empty() {
                        ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).wrap());
                    }
                });
            });
        });
    let rect = inner.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(widgets::LIST_BAR_WIDTH, rect.height())),
        0.0,
        hue,
    );
}

/// A text laid out in the style of the program, wrapped at `wrap` points or not at all.
fn painter_layout(
    ui: &Ui,
    text: String,
    style: egui::TextStyle,
    hue: egui::Color32,
    wrap: Option<f32>,
) -> std::sync::Arc<egui::Galley> {
    let font = style.resolve(ui.style());
    match wrap {
        Some(width) => ui.painter().layout(text, font, hue, width),
        None => ui.painter().layout_no_wrap(text, font, hue),
    }
}

fn grade_color(scheme: &Scheme, grade: Grade) -> Color {
    match grade {
        Grade::Green => scheme.accent,
        Grade::Yellow => scheme.gold,
        Grade::Red => scheme.red,
    }
}

/// A text that must not be overlooked (the result of the measuring), in a window.
fn dialog(ui: &mut Ui, scheme: &Scheme, state: &mut FovState) {
    let Some(text) = state.dialog.clone() else { return };
    let (ok, close) = widgets::dialog(ui.ctx(), scheme, "blickwinkel-ergebnis", |ui| {
        widgets::heading(ui, scheme, &t("hf_blickwinkel"));
        ui.label(&text);
        ui.add_space(8.0);
        ui.add(widgets::Button::new(&t("e_ok"), ButtonKind::Strong, scheme)).clicked()
    });
    if ok || close {
        state.dialog = None;
    }
}

/// The whole window as the measuring: the card to drag to the size of a real card.
pub fn show_measure(ui: &mut Ui, scheme: &Scheme, state: &mut FovState, paths: &Paths) {
    let Some(mut measure) = state.measure.take() else { return };
    let ctx = ui.ctx().clone();
    let now = ui.input(|i| i.time);
    let (fullscreen, monitor) = ctx.input(|i| {
        let v = i.viewport();
        (v.fullscreen == Some(true), v.monitor_size)
    });
    // The window needs a moment to go full screen: the card is drawn when it has, or when it will not.
    if !fullscreen && now - measure.opened_at < FULLSCREEN_WAIT {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
        state.measure = Some(measure);
        return;
    }
    let ppp = ctx.pixels_per_point();
    let area = ui.max_rect();
    ui.painter().rect_filled(area, 0.0, color(scheme.bg));
    let canvas_px = (area.width() * ppp).round();
    let center = egui::pos2(area.center().x, area.center().y - 30.0 / ppp);

    // The mouse: the width is twice the distance of the pointer from the middle, at the least the smallest.
    // Not over the bar at the bottom: a click on "Passt" must not move the card first.
    let above_bar = egui::Rect::from_min_max(area.min, egui::pos2(area.right(), area.bottom() - BAR_HEIGHT));
    let surface = ui
        .interact(above_bar, ui.scope_id().with("fov-surface"), egui::Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if surface.is_pointer_button_down_on()
        && let Some(pos) = surface.interact_pointer_pos()
    {
        measure.card_px = (((pos.x - center.x).abs() * 2.0 * ppp).round() as i64).max(MIN_CARD_PX);
    }
    // The keys: one pixel at a time, with Shift ten. The presses are **taken**: the slider, which keeps the
    // focus after a click on it, would count the same arrow a second time otherwise.
    let (steps, escape) = ui.input_mut(|i| {
        let mut count = |key: egui::Key| {
            i64::try_from(i.count_and_consume_key(egui::Modifiers::SHIFT, key)).unwrap_or(0) * 10
                + i64::try_from(i.count_and_consume_key(egui::Modifiers::NONE, key)).unwrap_or(0)
        };
        let steps = count(egui::Key::ArrowRight) - count(egui::Key::ArrowLeft);
        (steps, i.key_pressed(egui::Key::Escape))
    });
    measure.card_px = measure.card_px.saturating_add(steps).clamp(MIN_CARD_PX, MAX_CARD_PX);
    if escape {
        state.stop_measuring(&ctx);
        return;
    }

    // The card, with the lines through its middle.
    let width = measure.card_px as f32 / ppp;
    let height = width * (fov::CARD_HEIGHT_MM / fov::CARD_WIDTH_MM) as f32;
    let card = egui::Rect::from_center_size(center, egui::vec2(width, height));
    let painter = ui.painter();
    painter.rect_filled(card, 0.0, color(scheme.surface));
    // Lines of two and one physical pixels, at every scaling (the edge of the card is what is matched to the card).
    painter.rect_stroke(card, 0.0, egui::Stroke::new(2.0 / ppp, color(scheme.accent)), egui::StrokeKind::Middle);
    let line = egui::Stroke::new(1.0 / ppp, color(scheme.sub));
    painter.line_segment([egui::pos2(center.x, card.top()), egui::pos2(center.x, card.bottom())], line);
    painter.line_segment([egui::pos2(card.left(), center.y), egui::pos2(card.right(), center.y)], line);
    // The bar at the bottom: slider, what is measured now, and the two buttons.
    let bar = egui::Rect::from_min_max(egui::pos2(area.left(), area.bottom() - BAR_HEIGHT), area.right_bottom());
    // The words: the instruction above the card, the measures below it; both stay on the screen and above the bar
    // however large the card is (then they lie over it).
    let text_width = (area.width() - 80.0).min(900.0 / ppp);
    let instruction =
        painter_layout(ui, t("s_fv_anleitung"), egui::TextStyle::Body, color(scheme.fg), Some(text_width));
    let top = (card.top() - 60.0 / ppp - instruction.size().y).max(area.top() + 8.0);
    ui.painter().galley(egui::pos2(center.x - instruction.size().x / 2.0, top), instruction, color(scheme.fg));
    let measures = painter_layout(ui, t("s_fv_masse"), egui::TextStyle::Small, color(scheme.sub), None);
    let below = (card.bottom() + 40.0 / ppp).min(bar.top() - measures.size().y - 8.0);
    ui.painter().galley(egui::pos2(center.x - measures.size().x / 2.0, below), measures, color(scheme.sub));

    let mut done = false;
    let mut cancel = false;
    let mut bar_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::left_to_right(egui::Align::Center).with_main_justify(false)),
    );
    // What the bar holds besides the slider decides how long the slider may be: on the smallest window it gets
    // shorter, or the buttons would stand over the edge.
    let stand = |px: i64| {
        let cm = fov::mm_per_pixel(px as f64)
            .and_then(|mm| fov::screen_width_mm(f64::from(canvas_px), mm))
            .map_or(0.0, |mm| mm / 10.0);
        let shown = formats::fixed(lang(), cm, 1);
        tf("s_fv_stand", &[("0", Arg::from(px)), ("1", Arg::from(shown.as_str()))])
    };
    let (yes_word, no_word) = (t("s_fv_passt"), t("s_fv_abbrechen"));
    let spacing = bar_ui.spacing().item_spacing.x;
    let label_width =
        painter_layout(&bar_ui, stand(measure.card_px), egui::TextStyle::Body, color(scheme.fg), None).size().x;
    let buttons_width = widgets::button_size(&bar_ui, scheme, ButtonKind::Strong, &yes_word).x
        + widgets::button_size(&bar_ui, scheme, ButtonKind::Normal, &no_word).x;
    let slider_width =
        (bar.width() - 16.0 - label_width - buttons_width - 3.0 * spacing).clamp(MIN_SLIDER_WIDTH, 420.0);
    let group = slider_width + label_width + buttons_width + 3.0 * spacing;
    bar_ui.add_space(((bar.width() - group) / 2.0).max(8.0));
    let before = measure.card_px;
    let mut slider_value = measure.card_px.clamp(MIN_CARD_PX, MAX_SLIDER_PX);
    let slider =
        bar_ui.add(widgets::Slider::new(&mut slider_value, MIN_CARD_PX, MAX_SLIDER_PX, scheme).width(slider_width));
    let (enabled, shown_px) = (slider.enabled(), slider_value as f64);
    slider.widget_info(|| egui::WidgetInfo::slider(enabled, shown_px, t("s_fv_kartenbreite")));
    if slider.changed() {
        measure.card_px = slider_value;
    }
    bar_ui.label(stand(measure.card_px));
    done |= bar_ui.add(widgets::Button::new(&yes_word, ButtonKind::Strong, scheme)).clicked();
    cancel |= bar_ui.add(widgets::Button::new(&no_word, ButtonKind::Normal, scheme)).clicked();
    if measure.card_px != before {
        ctx.request_repaint();
    }

    if cancel {
        state.stop_measuring(&ctx);
        return;
    }
    if done {
        // Only a window that really took the screen says how wide the screen is: as wide as the screen. ("Full
        // screen" as a state is what was asked for and true at once.) Where the size of the screen is unknown, it
        // is not taken (4.12 did not either).
        let real = fullscreen && monitor.is_some_and(|m| (area.width() - m.x).abs() <= FULL_TOLERANCE);
        let text = match fov::mm_per_pixel(measure.card_px as f64) {
            Some(mm) if real => {
                if fov::remember(paths, Some(mm), Some(f64::from(canvas_px)), None) {
                    let cm = fov::screen_width_mm(f64::from(canvas_px), mm).unwrap_or(0.0) / 10.0;
                    // The answer is a line in the foot (with the pin), not a window to close.
                    let shown = formats::fixed(lang(), cm, 1);
                    state.notice = Some(Said::note(tf("s_fv_gespeichert", &[("0", Arg::from(shown.as_str()))])));
                    None
                } else {
                    state.notice = Some(Said::fault(t("s_fv_f_speichern")));
                    Some(t("s_fv_f_speichern"))
                }
            }
            Some(_) => Some(t("s_fv_kein_vollbild")),
            None => None,
        };
        state.stop_measuring(&ctx);
        state.dialog = text;
        return;
    }
    state.measure = Some(measure);
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers, Pos2, Rect};
    use versekit_core::language::Choice;
    use versekit_core::paths::{ENV_INSTALL_DIR, Host};
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    type Texts = Vec<(String, Pos2)>;

    const FILE: &str = "<Attributes>\n <Attr name=\"FOV\" value=\"54.7681\"/>\n <Attr name=\"Width\" value=\"2560\"/>\n <Attr name=\"Height\" value=\"1440\"/>\n</Attributes>\n";

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-fovpage-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn world(name: &str, file: Option<&str>) -> (Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        if let Some(file) = file {
            std::fs::write(format!("{profile}/attributes.xml"), file).expect("file");
        }
        std::fs::write(format!("{game}/Game.log"), "x").expect("log");
        let vars = [("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), (ENV_INSTALL_DIR, game.as_str())];
        (Paths::new(Host::new(Flavor::Posix, &vars, &home)), home)
    }

    struct Bench {
        ctx: egui::Context,
        state: FovState,
        paths: Paths,
        home: String,
        time: f64,
        /// What the window says about itself: (full screen, size of the monitor in points).
        window: (bool, Option<egui::Vec2>),
        commands: Vec<ViewportCommand>,
        /// The size of the window in points.
        screen: egui::Vec2,
        /// Every text of the last frame with the place it takes.
        rects: Vec<(String, Rect)>,
    }

    impl Bench {
        fn new(name: &str, file: Option<&str>) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let (paths, home) = world(name, file);
            Bench {
                ctx,
                state: FovState::default(),
                paths,
                home,
                time: 10.0,
                window: (false, None),
                commands: Vec::new(),
                screen: egui::vec2(1280.0, 800.0),
                rects: Vec::new(),
            }
        }

        fn frame(&mut self, events: Vec<Event>) -> Texts {
            let scheme = theme::current();
            self.time += 0.05;
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let info = egui::ViewportInfo {
                fullscreen: Some(self.window.0),
                monitor_size: self.window.1,
                ..Default::default()
            };
            input.viewports.insert(egui::ViewportId::ROOT, info);
            let (state, paths) = (&mut self.state, &self.paths);
            let mut output = self.ctx.run_ui(input, |ui| {
                if state.measuring() {
                    show_measure(ui, scheme, state, paths);
                } else {
                    show_page(ui, scheme, state, paths);
                }
            });
            output.textures_delta.clear();
            if let Some(root) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                self.commands.extend(root.commands.iter().cloned());
            }
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            let mut rects = Vec::new();
            fn collect_rects(shape: &egui::Shape, into: &mut Vec<(String, Rect)>) {
                match shape {
                    egui::Shape::Text(text) => {
                        into.push((text.galley.text().to_string(), Rect::from_min_size(text.pos, text.galley.size())))
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_rects(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect_rects(&clipped.shape, &mut rects));
            self.rects = rects;
            texts
        }

        fn settle(&mut self) -> Texts {
            let mut last = Vec::new();
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            last
        }

        fn press_at(&mut self, at: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        fn click(&mut self, label: &str) {
            let texts = self.settle();
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.press_at(at);
        }

        fn key(&mut self, key: Key, modifiers: Modifiers) {
            // The modifiers are held as well as named in the event.
            self.frame(vec![Event::ModifiersChanged(modifiers)]);
            for pressed in [true, false] {
                self.frame(vec![Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }]);
            }
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
        }

        fn fullscreen_asked(&self) -> Option<bool> {
            self.commands
                .iter()
                .rev()
                .find_map(|c| if let ViewportCommand::Fullscreen(on) = c { Some(*on) } else { None })
        }
    }

    fn shows(texts: &Texts, text: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(text))
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Choice::De);
        guard
    }

    #[test]
    fn before_the_first_measuring_the_page_has_the_button_and_one_sentence_and_nothing_to_calculate() {
        let _guard = setup();
        let mut bench = Bench::new("ungemessen", Some(FILE));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_messen")) && shows(&texts, &t("s_fv_nicht_gemessen")), "{texts:?}");
        assert!(!shows(&texts, &t("s_fv_abstand")) && !shows(&texts, &t("s_fv_breite")), "no distance yet: {texts:?}");
        assert!(!shows(&texts, &t("s_fv_neu_messen")));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_screen_the_neutral_angle_cannot_reach_says_so_and_rates_only_the_minimum() {
        let _guard = setup();
        // The smallest setting of the game at 16:9 (the file has 54.7681 vertical): 85.3 horizontal. A screen 596 mm
        // wide at 700 mm gives about 46.2 degrees: far below, so not reachable.
        let mut bench = Bench::new("unerreichbar", Some(FILE));
        fov::remember(&bench.paths, Some(0.2330), Some(2560.0), Some(700.0));
        let texts = bench.settle();
        assert!(shows(&texts, "59,6 cm"), "the width of the screen: {texts:?}");
        assert!(shows(&texts, "46,2°"), "the neutral angle: {texts:?}");
        // Why it cannot be reached stands on hover over its row (every hint does); the box below says the verdict.
        let reason = tf("s_fv_neutral_unerreichbar", &[("0", Arg::from(85.3))]);
        assert!(!shows(&texts, &reason), "on hover, not as a line under the row: {texts:?}");
        assert!(shows(&texts, "85°"), "the setting in the game: {texts:?}");
        assert!(!shows(&texts, &t("s_fv_optimalpunkt")), "no sweet spot where it cannot be set: {texts:?}");
        // The verdict stands in a box: the first sentence in bold, what it means below it.
        let (head, line) =
            t("s_fv_am_minimum").split_once(". ").map(|(a, b)| (format!("{a}."), b.to_string())).unwrap();
        assert!(shows(&texts, &head) && shows(&texts, &line), "the narrowest is set: {texts:?}");
        assert!(shows(&texts, &t("s_fv_hinweis_deutung")), "the note on vertical and horizontal: {texts:?}");
        assert!(shows(&texts, &t("s_fv_neu_messen")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_reachable_angle_gives_the_sweet_spot_and_says_how_far_off_the_seat_is() {
        let _guard = setup();
        // 60 degrees vertical at 16:9 is 91.5 horizontal; a screen 447 mm wide (0.2330 mm per pixel, 1920 wide) has
        // its sweet spot at 447 / (2 · tan(45.75°)) = 218 mm.
        let file = "<Attributes>\n <Attr name=\"FOV\" value=\"60\"/>\n <Attr name=\"Width\" value=\"1920\"/>\n <Attr name=\"Height\" value=\"1080\"/>\n</Attributes>\n";
        let mut bench = Bench::new("erreichbar", Some(file));
        // 12 % too far away: yellow, "3 cm too far away".
        fov::remember(&bench.paths, Some(0.2330), Some(1920.0), Some(244.0));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_optimalpunkt")) && shows(&texts, "22 cm"), "the sweet spot: {texts:?}");
        let (head, line) = tf("s_fv_zu_weit", &[("0", Arg::from(2.6))])
            .split_once(". ")
            .map(|(a, b)| (format!("{a}."), b.to_string()))
            .unwrap();
        assert!(shows(&texts, &head) && shows(&texts, &line), "{texts:?}");
        // The hint stands on hover behind the symbol `info`, never as a line under its name.
        let hint = t("s_fv_neutral_hilfe");
        let start = hint.split(' ').take(4).collect::<Vec<_>>().join(" ");
        assert!(!texts.iter().any(|(text, _)| text.starts_with(&start)), "{texts:?}");
        // At the sweet spot itself: good.
        fov::remember(&bench.paths, None, None, Some(218.0));
        bench.state.invalidate();
        bench.time += 2.0;
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_passt_gut")), "{texts:?}");
        // Much too close: red, too close.
        fov::remember(&bench.paths, None, None, Some(120.0));
        bench.state.invalidate();
        bench.time += 2.0;
        let texts = bench.settle();
        assert!(texts.iter().any(|(s, _)| s.starts_with("Du sitzt ") && s.contains("zu nah")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_distance_is_kept_when_the_field_is_left_and_a_wrong_one_is_said_at_the_field() {
        let _guard = setup();
        let mut bench = Bench::new("abstand", Some(FILE));
        fov::remember(&bench.paths, Some(0.2330), Some(2560.0), None);
        bench.settle();
        bench.click(&t("s_fv_abstand_platz"));
        bench.frame(vec![Event::Text("70".to_string())]);
        assert_eq!(fov::stored(&bench.paths).distance_mm, None, "not at every key");
        bench.key(Key::Enter, Modifiers::NONE);
        bench.settle();
        assert_eq!(fov::stored(&bench.paths).distance_mm, Some(700.0));
        assert!(!shows(&bench.settle(), &t("s_fv_abstand_ungueltig")));
        // A text that is no distance: said at the field, the kept one stays.
        bench.state.distance_text = "viel".to_string();
        bench.state.commit_distance(&bench.paths);
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_abstand_ungueltig")), "{texts:?}");
        assert_eq!(fov::stored(&bench.paths).distance_mm, Some(700.0));
        // A comma is a decimal point.
        bench.state.distance_text = "62,5".to_string();
        bench.state.commit_distance(&bench.paths);
        assert_eq!(fov::stored(&bench.paths).distance_mm, Some(625.0));
        assert_eq!(bench.state.distance_text, "62.5");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_button_takes_the_window_to_full_screen_and_the_card_is_drawn_when_it_is_there() {
        let _guard = setup();
        let mut bench = Bench::new("messen", Some(FILE));
        bench.click(&t("s_fv_messen"));
        assert!(bench.state.measuring());
        assert_eq!(bench.fullscreen_asked(), Some(true));
        // Not full screen yet and not for long: the card is not drawn yet.
        let texts = bench.frame(Vec::new());
        assert!(!shows(&texts, &t("s_fv_passt")), "{texts:?}");
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_passt")) && shows(&texts, &t("s_fv_abbrechen")), "{texts:?}");
        assert!(shows(&texts, &t("s_fv_masse")), "{texts:?}");
        assert!(shows(&texts, "320 px"), "the card starts at 320 pixels: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_keys_move_the_card_by_one_or_ten_pixels_and_passt_keeps_the_calibration_of_the_full_screen() {
        let _guard = setup();
        let mut bench = Bench::new("tasten", Some(FILE));
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_messen"));
        bench.settle();
        for _ in 0..3 {
            bench.key(Key::ArrowRight, Modifiers::NONE);
        }
        bench.key(Key::ArrowLeft, Modifiers::SHIFT);
        let card = bench.state.measure.as_ref().map(|m| m.card_px).expect("measuring");
        assert_eq!(card, 320 + 3 - 10);
        // Not below the smallest.
        for _ in 0..40 {
            bench.key(Key::ArrowLeft, Modifiers::SHIFT);
        }
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(MIN_CARD_PX));
        for _ in 0..22 {
            bench.key(Key::ArrowRight, Modifiers::SHIFT);
        }
        let card = bench.state.measure.as_ref().map(|m| m.card_px).unwrap();
        bench.click(&t("s_fv_passt"));
        assert!(!bench.state.measuring() && bench.fullscreen_asked() == Some(false));
        let kept = fov::stored(&bench.paths);
        assert!((kept.mm_per_pixel.unwrap() - fov::CARD_WIDTH_MM / card as f64).abs() < 1e-12, "{kept:?}");
        assert_eq!(kept.pixel_width, Some(1280.0), "the width of the screen the window took");
        // The answer is a line in the foot, not a window.
        let said = bench.state.take_notice().expect("the answer");
        assert!(!said.fault && said.text.starts_with("Ausgemessen:") && said.text.ends_with(" cm breit."), "{said:?}");
        assert!(bench.state.dialog.is_none());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_mouse_drags_the_edge_of_the_card_and_a_click_on_a_button_does_not_move_it() {
        let _guard = setup();
        let mut bench = Bench::new("maus", Some(FILE));
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_messen"));
        bench.settle();
        // The pointer 200 points right of the middle: the card is 400 wide (twice the distance).
        let at = Pos2::new(640.0 + 200.0, 300.0);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        bench.frame(vec![Event::PointerMoved(at)]);
        bench.frame(vec![button(true)]);
        bench.frame(vec![Event::PointerMoved(Pos2::new(640.0 + 200.0, 301.0))]);
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(400));
        bench.frame(vec![button(false)]);
        bench.settle();
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(400));
        // The smallest width holds for the mouse as well.
        let near = Pos2::new(640.0 + 5.0, 300.0);
        bench.frame(vec![Event::PointerMoved(near)]);
        bench.frame(vec![Event::PointerButton {
            pos: near,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(MIN_CARD_PX));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_window_that_did_not_go_full_screen_saves_nothing_and_says_so() {
        let _guard = setup();
        let mut bench = Bench::new("kein-vollbild", Some(FILE));
        bench.click(&t("s_fv_messen"));
        // The window never goes full screen: after the wait the card is drawn all the same.
        bench.time += 3.0;
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_passt")), "{texts:?}");
        bench.click(&t("s_fv_passt"));
        assert_eq!(fov::stored(&bench.paths), Calibration::default(), "nothing saved");
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_fv_kein_vollbild")), "{texts:?}");
        // A window much smaller than the monitor counts as no full screen as well.
        bench.state.dialog = None;
        bench.window = (true, Some(egui::vec2(5000.0, 3000.0)));
        bench.click(&t("s_fv_messen"));
        bench.time += 3.0;
        bench.settle();
        bench.click(&t("s_fv_passt"));
        assert_eq!(fov::stored(&bench.paths), Calibration::default());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn escape_and_cancel_end_the_measuring_without_saving() {
        let _guard = setup();
        for how in ["escape", "cancel"] {
            let mut bench = Bench::new(&format!("abbruch-{how}"), Some(FILE));
            bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
            bench.click(&t("s_fv_messen"));
            bench.settle();
            if how == "escape" {
                bench.key(Key::Escape, Modifiers::NONE);
            } else {
                bench.click(&t("s_fv_abbrechen"));
            }
            assert!(!bench.state.measuring(), "{how}");
            assert_eq!(bench.fullscreen_asked(), Some(false));
            assert_eq!(fov::stored(&bench.paths), Calibration::default());
            let _ = std::fs::remove_dir_all(&bench.home);
        }
    }

    #[test]
    fn the_measuring_holds_its_texts_inside_the_screen_at_every_screen_size_language_scaling_and_card() {
        let _guard = setup();
        let mut findings = Vec::new();
        for scheme in ["original", "krt"] {
            theme::set_thread_current(scheme);
            for english in [false, true] {
                language::set_language(if english { Choice::En } else { Choice::De });
                // The smallest window of the program, full HD, a wide screen.
                for (width, height) in [(926.0, 600.0), (1280.0, 800.0), (1920.0, 1080.0), (2560.0, 1440.0)] {
                    for ppp in [1.0, 1.5, 2.0] {
                        // Cards of the smallest, a usual and a very wide size (in pixels).
                        for card in [MIN_CARD_PX, 600, 1300] {
                            let mut bench = Bench::new("rand", Some(FILE));
                            bench.screen = egui::vec2(width, height);
                            bench.window = (true, Some(bench.screen));
                            bench.ctx.set_pixels_per_point(ppp);
                            bench.state.measure = Some(Measure { card_px: card, opened_at: 0.0 });
                            bench.settle();
                            let screen = Rect::from_min_size(Pos2::ZERO, bench.screen);
                            for (text, rect) in &bench.rects {
                                if text.trim().is_empty() {
                                    continue;
                                }
                                if rect.min.x < screen.min.x - 0.5
                                    || rect.min.y < screen.min.y - 0.5
                                    || rect.max.x > screen.max.x + 0.5
                                    || rect.max.y > screen.max.y + 0.5
                                {
                                    findings.push(format!(
                                        "{scheme} englisch {english} {width}x{height} ppp {ppp} Karte {card}: `{text}` {rect:?}"
                                    ));
                                }
                            }
                            let _ = std::fs::remove_dir_all(&bench.home);
                        }
                    }
                }
            }
        }
        theme::set_current("original");
        language::set_language(Choice::De);
        assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
    }

    #[test]
    fn only_a_window_as_wide_as_the_screen_counts_as_full_screen() {
        let _guard = setup();
        // The window says "full screen" at once, but is 1280 wide on a screen of 1920 (it was not given the screen):
        // no calibration. Where the size of the screen is not known, none either.
        for (name, monitor) in [("schmaler", Some(egui::vec2(1920.0, 1080.0))), ("unbekannt", None)] {
            let mut bench = Bench::new(&format!("vollbild-{name}"), Some(FILE));
            bench.window = (true, monitor);
            bench.click(&t("s_fv_messen"));
            bench.settle();
            bench.click(&t("s_fv_passt"));
            assert_eq!(fov::stored(&bench.paths), Calibration::default(), "{name}");
            let texts = bench.settle();
            assert!(shows(&texts, &t("s_fv_kein_vollbild")), "{name}: {texts:?}");
            let _ = std::fs::remove_dir_all(&bench.home);
        }
        // A window one point off (decorations, rounding) still counts.
        let mut bench = Bench::new("vollbild-fast", Some(FILE));
        bench.window = (true, Some(egui::vec2(1281.5, 800.0)));
        bench.click(&t("s_fv_messen"));
        bench.settle();
        bench.click(&t("s_fv_passt"));
        assert_eq!(fov::stored(&bench.paths).pixel_width, Some(1280.0));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_arrow_keys_count_once_also_when_the_slider_has_the_focus() {
        let _guard = setup();
        let mut bench = Bench::new("regler-fokus", Some(FILE));
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_messen"));
        bench.settle();
        // A click does not give the slider the focus in egui; the Tab key does: first onto the surface of the card,
        // then onto the slider. Without the focus it would not count the arrows at all, and the test would show
        // nothing.
        bench.key(Key::Tab, Modifiers::NONE);
        bench.key(Key::Tab, Modifiers::NONE);
        assert!(bench.ctx.memory(|m| m.focused()).is_some(), "something has the focus");
        let before = bench.state.measure.as_ref().map(|m| m.card_px).unwrap();
        bench.key(Key::ArrowRight, Modifiers::NONE);
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(before + 1), "one pixel, not two");
        bench.key(Key::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(before - 9), "ten, not eleven");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn at_another_scaling_the_card_is_in_pixels_and_the_screen_is_its_width_in_pixels() {
        let _guard = setup();
        let mut bench = Bench::new("skalierung", Some(FILE));
        bench.ctx.set_pixels_per_point(1.5);
        // 1280 x 800 points at 1.5 are 1920 x 1200 pixels.
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_messen"));
        bench.settle();
        bench.settle();
        let texts = bench.settle();
        assert!(shows(&texts, "320 px"), "{texts:?}");
        bench.click(&t("s_fv_passt"));
        let kept = fov::stored(&bench.paths);
        assert_eq!(kept.pixel_width, Some(1920.0), "physical pixels: {kept:?}");
        assert!((kept.mm_per_pixel.unwrap() - fov::CARD_WIDTH_MM / 320.0).abs() < 1e-12, "{kept:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn what_was_typed_and_not_left_is_kept_when_the_button_for_measuring_is_clicked() {
        let _guard = setup();
        let mut bench = Bench::new("tippen-messen", Some(FILE));
        fov::remember(&bench.paths, Some(0.2330), Some(2560.0), None);
        bench.settle();
        bench.click(&t("s_fv_abstand_platz"));
        bench.frame(vec![Event::Text("65".to_string())]);
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_neu_messen"));
        assert!(bench.state.measuring());
        assert_eq!(fov::stored(&bench.paths).distance_mm, Some(650.0));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_calibration_that_cannot_be_written_is_not_called_saved() {
        let _guard = setup();
        let mut bench = Bench::new("nicht-schreibbar", Some(FILE));
        fov::remember(&bench.paths, Some(0.2330), Some(2560.0), Some(700.0));
        bench.settle();
        // The settings file is a folder now: nothing can be written.
        let file = bench.paths.settings_file();
        std::fs::remove_file(&file).ok();
        std::fs::create_dir_all(&file).unwrap();
        bench.state.distance_text = "80".to_string();
        bench.state.commit_distance(&bench.paths);
        let said = bench.state.take_notice().expect("said");
        assert!(said.fault && said.text == t("s_fv_f_speichern"), "{said:?}");
        assert_ne!(bench.state.calibration.distance_mm, Some(800.0), "the page does not show it as kept");
        std::fs::remove_dir_all(&file).unwrap();
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_next_measuring_starts_with_the_card_as_last_measured() {
        let _guard = setup();
        let mut bench = Bench::new("weiter", Some(FILE));
        fov::remember(&bench.paths, Some(fov::CARD_WIDTH_MM / 410.0), Some(2560.0), Some(700.0));
        bench.window = (true, Some(egui::vec2(1280.0, 800.0)));
        bench.click(&t("s_fv_neu_messen"));
        assert_eq!(bench.state.measure.as_ref().map(|m| m.card_px), Some(410));
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
