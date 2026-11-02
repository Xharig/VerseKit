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

//! Verse-Kit's user interface on egui.
//!
//! At startup call [`install`] (fonts and image loaders). [`apply`] puts a
//! color scheme into an egui context; call it again after switching and the
//! next frame is drawn in the new colors, no restart.
//!
//! What egui's styling does not cover (toggle switch, slider, scroll bar
//! colors, square boxes with corner brackets, filled accent buttons, accent
//! headings, heat map, field with suggestions) is in [`widgets`]; which
//! entries such a field suggests: [`suggest`]. Lucide icons: [`icons`]. Lato:
//! [`fonts`].

pub mod axes_page;
pub mod axes_plot;
pub mod basetool_page;
pub mod blickwinkel_page;
pub mod blueprint_list;
pub mod blueprint_progress;
pub mod channel_question;
pub mod close_writes;
pub mod collection_tools;
pub mod contested_page;
pub mod contract_log;
pub mod crafting_page;
pub mod crafting_shop;
pub mod data_folder;
pub mod demand_page;
pub mod detection_page;
pub mod diagnose_page;
pub mod dismantle_view;
pub mod equipment_box;
pub mod extension;
pub mod farm_page;
pub mod file_dialog;
pub mod first_backup;
pub mod fleet_analysis_view;
pub mod fonts;
pub mod gallery_page;
pub mod game_picture;
pub mod game_texts_page;
pub mod grafik_page;
pub mod handle_page;
pub mod handle_pictures;
pub mod handle_search;
pub mod hangar_page;
pub mod head_sync;
pub mod highlight;
pub mod icons;
pub mod import_page;
pub mod info_bar;
pub mod joysticks_bind;
pub mod joysticks_list;
pub mod joysticks_page;
pub mod joysticks_tools;
pub mod layout;
pub mod mining_page;
pub mod modules_page;
pub mod news_page;
pub mod notes_page;
pub mod org_stock_page;
pub mod overlay;
pub mod refineries_page;
pub mod renderer;
pub mod routes_page;
pub mod salvage_page;
pub mod sample;
pub mod scanner_ui;
pub mod patch_changes_page;
pub mod selling_page;
pub mod serverstatus_page;
pub mod settings;
pub mod shader_page;
pub mod ship_names;
pub mod shopping_page;
pub mod shops_page;
pub mod slot_sync;
pub mod startprograms_page;
pub mod statistics_page;
pub mod stock_page;
pub mod stock_share;
pub mod streamer;
pub mod suggest;
pub mod thanks_page;
pub mod tiles;
pub mod trade;
pub mod update_page;
pub mod viewport_zoom;
pub mod widgets;
pub mod window_look;

use egui::{Color32, CornerRadius, Stroke, Theme, Visuals};
use versekit_core::font_size;
use versekit_core::theme::{Color, Scheme};

/// Everything the user interface needs once at startup: Lato, the SVG
/// loaders for the icons and the filter of the streamer mode.
pub fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    icons::install(ctx);
    streamer::install(ctx);
}

/// Sets the size of the whole interface (text, controls, icons, spacing)
/// from the size setting in points (0 … 7, `normal` = 1), capped to what the
/// screen can hold ([`max_size`]). Returns the size in effect. Takes effect
/// with the next frame; call [`fit_window`] every frame.
pub fn apply_size(ctx: &egui::Context, points: i64) -> i64 {
    let points = points.min(max_size(ctx));
    ctx.set_zoom_factor(font_size::zoom_factor(points));
    points
}

/// The zoom of a window of its own (overlay, info bar) that is `percent` as large as the main window at `points`.
pub fn window_zoom(points: i64, percent: i64) -> f32 {
    font_size::zoom_factor(points) * percent.max(1) as f32 / 100.0
}

/// [`apply_size`] for a window of its own that is `percent` as large as the main window. Returns the size in effect.
pub fn apply_window_size(ctx: &egui::Context, points: i64, percent: i64) -> i64 {
    let points = points.min(max_size(ctx));
    ctx.set_zoom_factor(window_zoom(points, percent));
    points
}

/// The screen in screen points, independent of the interface size; `None`
/// before the window knows its monitor. (egui reports it in interface
/// points, already divided by the zoom.)
pub fn screen_points(ctx: &egui::Context) -> Option<egui::Vec2> {
    ctx.input(|i| i.viewport().monitor_size).map(|size| size * ctx.zoom_factor())
}

/// Screen height in pixels, for the size preset `auto`.
pub fn screen_height(ctx: &egui::Context) -> Option<i64> {
    let size = ctx.input(|i| i.viewport().monitor_size)?;
    Some((size.y * ctx.pixels_per_point()).round() as i64)
}

/// The largest interface size this screen allows: the smallest window at
/// that size still fits on it. Unknown screen: no limit.
pub fn max_size(ctx: &egui::Context) -> i64 {
    screen_points(ctx).map_or(font_size::POINTS_RANGE.1, |s| font_size::max_points_for_screen(s.x, s.y))
}

/// Keeps the window at least as large as the smallest window of the current
/// size (it grows with the size) and no larger than the screen. Call every
/// frame; it only acts when the size changed.
///
/// The smallest window is the same in interface points at every size
/// ([`font_size::BASE_MIN_SIZE`]), so it grows on screen with the zoom.
///
/// The window size egui reports in the first frame after a size change is
/// still measured with the old zoom, so the window is fitted one frame later,
/// only ever enlarged, and never in full screen or while maximized.
pub fn fit_window(ctx: &egui::Context) {
    let id = egui::Id::unique("versekit-fit-window");
    let zoom = ctx.zoom_factor();
    // (zoom, fitted): the zoom seen last and whether the window was fitted to it.
    let seen = ctx.data(|d| d.get_temp::<(f32, bool)>(id));
    if seen != Some((zoom, false)) {
        if seen.is_none_or(|(z, _)| z != zoom) {
            ctx.data_mut(|d| d.insert_temp(id, (zoom, false)));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(base_size()));
            ctx.request_repaint();
        }
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, (zoom, true)));
    let (inner, monitor, fills_screen) = ctx.input(|i| {
        let v = i.viewport();
        (v.inner_rect, v.monitor_size, v.fullscreen == Some(true) || v.maximized == Some(true))
    });
    let Some(inner) = inner else { return };
    if let Some(wanted) = grown_size(inner.size(), base_size(), monitor, fills_screen) {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(wanted));
    }
}

/// The smallest window in interface points (the same at every size).
fn base_size() -> egui::Vec2 {
    egui::vec2(font_size::BASE_MIN_SIZE.0, font_size::BASE_MIN_SIZE.1)
}

/// The size to ask for so the window holds the smallest window `base`: only
/// ever larger than `inner`, at most the screen share of `monitor`; `None`
/// when nothing is to do or the window fills the screen (full screen,
/// maximized). All sizes in interface points.
pub fn grown_size(
    inner: egui::Vec2,
    base: egui::Vec2,
    monitor: Option<egui::Vec2>,
    fills_screen: bool,
) -> Option<egui::Vec2> {
    if fills_screen {
        return None;
    }
    let mut wanted = inner.max(base);
    if let Some(monitor) = monitor {
        wanted = wanted.min(monitor * font_size::SCREEN_SHARE).max(inner);
    }
    ((wanted - inner).length() > 0.5).then_some(wanted)
}

/// Radius of buttons and choice fields in rounded schemes.
pub const CONTROL_RADIUS: u8 = 5;
/// Radius of windows and text fields in rounded schemes.
pub const BOX_RADIUS: u8 = 8;
/// Radius of buttons and list panels in square schemes (design system
/// `--radius-sm`); boxes stay square there.
pub const SQUARE_CONTROL_RADIUS: u8 = 4;

pub fn color(c: Color) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

/// Rounding of buttons, choice fields and menus.
pub fn control_radius(scheme: &Scheme) -> CornerRadius {
    CornerRadius::same(if scheme.square { SQUARE_CONTROL_RADIUS } else { CONTROL_RADIUS })
}

/// Rounding of boxes and windows; none in square schemes (corner brackets).
pub fn box_radius(scheme: &Scheme) -> CornerRadius {
    CornerRadius::same(if scheme.square { 0 } else { BOX_RADIUS })
}

/// The egui visuals of a scheme.
pub fn visuals(scheme: &Scheme) -> Visuals {
    let mut v = Visuals::dark();
    let control = control_radius(scheme);

    v.panel_fill = color(scheme.bg);
    v.window_fill = color(scheme.surface);
    v.window_stroke = Stroke::new(1.0, color(scheme.line));
    v.window_corner_radius = box_radius(scheme);
    v.menu_corner_radius = control;
    v.faint_bg_color = color(scheme.bar);
    v.extreme_bg_color = color(scheme.field);
    v.text_edit_bg_color = Some(color(scheme.field));
    v.code_bg_color = color(scheme.field);
    v.weak_text_color = Some(color(scheme.sub));
    v.hyperlink_color = color(scheme.accent);
    v.warn_fg_color = color(scheme.yellow);
    v.error_fg_color = color(scheme.red);
    v.selection.bg_fill = color(scheme.accent_dark);
    // Also the border of a field while typing in it: 2 points.
    v.selection.stroke = Stroke::new(2.0, color(scheme.accent));

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = color(scheme.surface);
    w.noninteractive.weak_bg_fill = color(scheme.surface);
    w.noninteractive.bg_stroke = Stroke::new(1.0, color(scheme.line));
    w.noninteractive.fg_stroke = Stroke::new(1.0, color(scheme.fg));
    w.noninteractive.corner_radius = control;

    w.inactive.bg_fill = color(scheme.control());
    w.inactive.weak_bg_fill = color(scheme.control());
    w.inactive.bg_stroke = Stroke::new(1.0, color(scheme.control_border()));
    w.inactive.fg_stroke = Stroke::new(1.0, color(scheme.fg));
    w.inactive.corner_radius = control;

    w.hovered.bg_fill = color(scheme.hover);
    w.hovered.weak_bg_fill = color(scheme.hover);
    w.hovered.bg_stroke = Stroke::new(1.0, color(scheme.accent));
    w.hovered.fg_stroke = Stroke::new(1.5, color(scheme.hover_fg()));
    w.hovered.corner_radius = control;

    w.active.bg_fill = color(scheme.accent_dark);
    w.active.weak_bg_fill = color(scheme.accent_dark);
    w.active.bg_stroke = Stroke::new(1.0, color(scheme.accent));
    w.active.fg_stroke = Stroke::new(2.0, color(scheme.hover_fg()));
    w.active.corner_radius = control;

    w.open.bg_fill = color(scheme.selected);
    w.open.weak_bg_fill = color(scheme.selected);
    w.open.bg_stroke = Stroke::new(1.0, color(scheme.accent));
    w.open.fg_stroke = Stroke::new(1.0, color(scheme.fg));
    w.open.corner_radius = control;
    v
}

/// Puts a scheme into the context, for light and dark system mode alike (all
/// schemes are dark). Takes effect with the next frame.
pub fn apply(ctx: &egui::Context, scheme: &Scheme) {
    let v = visuals(scheme);
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
}

/// Tests that switch the language, or compare a shown text with the one the
/// language file gives, hold this lock: the language is one for the whole
/// process, and tests run side by side.
#[cfg(test)]
pub(crate) fn language_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::theme;

    #[test]
    fn switches_without_restart() {
        let ctx = egui::Context::default();
        for scheme in theme::schemes() {
            apply(&ctx, scheme);
            let mut seen = None;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = ui.button("Test");
                seen = Some((ui.visuals().panel_fill, ui.visuals().widgets.inactive.bg_stroke.color));
            });
            // Headless: no renderer takes the font texture.
            output.textures_delta.clear();
            assert_eq!(seen, Some((color(scheme.bg), color(scheme.control_border()))), "{}", scheme.id);
        }
    }

    #[test]
    fn the_window_only_ever_grows_and_never_in_full_screen() {
        let base = egui::vec2(1160.0, 380.0);
        let monitor = Some(egui::vec2(2000.0, 1300.0));
        // Too small: grows to the smallest window.
        assert_eq!(grown_size(egui::vec2(1000.0, 380.0), base, monitor, false), Some(base));
        // Larger than needed: stays, never shrinks.
        assert_eq!(grown_size(egui::vec2(1600.0, 1000.0), base, monitor, false), None);
        // Larger than the screen share: stays too.
        assert_eq!(grown_size(egui::vec2(1990.0, 1290.0), base, monitor, false), None);
        // Growing stops at the screen share.
        let small_screen = Some(egui::vec2(1100.0, 800.0));
        assert_eq!(grown_size(egui::vec2(900.0, 380.0), base, small_screen, false), Some(egui::vec2(1045.0, 380.0)));
        // Full screen or maximized: nothing.
        assert_eq!(grown_size(egui::vec2(1000.0, 300.0), base, monitor, true), None);
    }

    /// One frame with the window reported at `inner` (interface points);
    /// returns the size commands it sent.
    fn fit_frame(ctx: &egui::Context, inner: egui::Vec2, fullscreen: bool) -> Vec<egui::Vec2> {
        let mut input = egui::RawInput::default();
        let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
        info.inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, inner));
        info.monitor_size = Some(egui::vec2(2560.0, 1440.0) / ctx.zoom_factor());
        info.fullscreen = Some(fullscreen);
        let mut output = ctx.run_ui(input, |_| fit_window(ctx));
        output.textures_delta.clear();
        let commands = output.viewport_output.remove(&egui::ViewportId::ROOT).map(|v| v.commands).unwrap_or_default();
        commands
            .into_iter()
            .filter_map(|c| match c {
                egui::ViewportCommand::InnerSize(size) => Some(size),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn going_back_to_a_smaller_size_does_not_shrink_the_window() {
        // Full screen, WQHD back to Full HD: the window must not shrink.
        for fullscreen in [true, false] {
            let ctx = egui::Context::default();
            let physical = egui::vec2(2560.0, 1400.0);
            ctx.set_zoom_factor(1.27);
            for _ in 0..3 {
                fit_frame(&ctx, physical / 1.27, fullscreen);
            }
            ctx.set_zoom_factor(1.0);
            let mut sent = Vec::new();
            // First frame after the change still reports the old points.
            sent.extend(fit_frame(&ctx, physical / 1.27, fullscreen));
            for _ in 0..3 {
                sent.extend(fit_frame(&ctx, physical, fullscreen));
            }
            assert!(sent.is_empty(), "full screen {fullscreen}: asked for {sent:?}");
        }
    }

    #[test]
    fn a_window_smaller_than_the_smallest_grows_after_a_size_change() {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            fit_frame(&ctx, egui::vec2(1000.0, 500.0), false);
        }
        // Larger size: the same window is now 1000 / 1.27 points wide.
        ctx.set_zoom_factor(1.27);
        let mut sent = Vec::new();
        sent.extend(fit_frame(&ctx, egui::vec2(1000.0, 500.0), false));
        for _ in 0..3 {
            sent.extend(fit_frame(&ctx, egui::vec2(1000.0, 500.0) / 1.27, false));
        }
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!((sent[0].x - font_size::BASE_MIN_SIZE.0).abs() < 0.5, "{sent:?}");
    }

    #[test]
    fn square_schemes_round_only_controls() {
        let krt = theme::by_id("krt").expect("krt exists");
        assert_eq!(visuals(krt).widgets.inactive.corner_radius, CornerRadius::same(SQUARE_CONTROL_RADIUS));
        assert_eq!(visuals(krt).window_corner_radius, CornerRadius::ZERO);
        let original = theme::by_id("original").expect("original exists");
        assert_eq!(visuals(original).widgets.inactive.corner_radius, CornerRadius::same(CONTROL_RADIUS));
    }
}
