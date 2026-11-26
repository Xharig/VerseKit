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

//! VerseKit's user interface on egui.
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
pub mod catalog_progress;
pub mod channel_question;
pub mod close_writes;
pub mod collection_tools;
pub mod contested_page;
pub mod contract_finder_page;
pub mod contract_log;
pub mod crafting_page;
pub mod crafting_shop;
pub mod data_folder;
pub mod bank_page;
pub mod finance_page;
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
pub mod flugbuch_page;
pub mod fonts;
pub mod gallery_page;
pub mod game_texts_page;
pub mod grafik_page;
pub mod install_picker;
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
pub mod joysticks_order;
pub mod joysticks_list;
pub mod joysticks_page;
pub mod joysticks_tools;
pub mod layout;
pub mod mining_page;
pub mod modules_page;
pub mod news_page;
pub mod notes_page;
pub mod org_connect_page;
pub mod org_data;
pub mod org_data_orders_page;
pub mod org_data_stock_page;
pub mod org_share;
pub mod org_stock_page;
pub mod overlay;
pub mod refineries_page;
pub mod renderer;
pub mod rock_box;
pub mod routes_page;
pub mod salvage_page;
pub mod sample;
pub mod scanner_ui;
pub mod patch_changes_page;
pub mod selling_page;
mod selling_route;
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
pub mod streamdeck_page;
pub mod streamer;
pub mod suggest;
pub mod thanks_page;
pub mod tiles;
pub mod trade;
pub mod update_page;
pub mod viewport_zoom;
pub mod widgets;
pub mod window_look;
pub mod workbench_ui;

use egui::{Color32, CornerRadius, Stroke, Theme, Visuals};
use versekit_core::font_size;
use versekit_core::theme::{Color, Scheme};

/// Everything the user interface needs once at startup: Lato, the text sizes, the SVG
/// loaders for the icons and the filter of the streamer mode.
pub fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    ctx.all_styles_mut(|style| style.text_styles = text_styles());
    icons::install(ctx);
    streamer::install(ctx);
}

/// Size of the text of rows, buttons and fields at the size `normal`, in points.
pub const BODY_SIZE: f32 = 13.0;
/// Size of hints, notes under a setting and other small text: close below the body text, so it stays readable
/// at every size of the interface.
pub const SMALL_SIZE: f32 = 12.0;
/// Size of the title of a page and of big figures.
pub const HEADING_SIZE: f32 = 17.0;

/// The few sizes of text the interface uses: small, body (rows, buttons, numbers) and heading; the title of a card
/// lies between body and heading ([`widgets::CARD_TITLE_SCALE`]).
pub fn text_styles() -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
    use egui::{FontId, TextStyle};
    [
        (TextStyle::Small, FontId::proportional(SMALL_SIZE)),
        (TextStyle::Body, FontId::proportional(BODY_SIZE)),
        (TextStyle::Button, FontId::proportional(BODY_SIZE)),
        (TextStyle::Heading, FontId::proportional(HEADING_SIZE)),
        (TextStyle::Monospace, FontId::monospace(BODY_SIZE)),
    ]
    .into()
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
    CornerRadius::same(if scheme.square { SQUARE_CONTROL_RADIUS } else { scheme.radius_control })
}

/// Rounding of boxes and windows; none in square schemes (corner brackets).
pub fn box_radius(scheme: &Scheme) -> CornerRadius {
    CornerRadius::same(if scheme.square { 0 } else { scheme.radius_box })
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
    v.error_fg_color = color(scheme.red_text());
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

/// What a test that switches the language, or compares a shown text with the
/// one the language file gives, holds. The language and the scheme are one for
/// the whole process while tests run side by side, so the test gets a language
/// and a scheme of its own, for its thread only (German, the first scheme):
/// whatever it switches afterwards (`set_language`, `set_current`) changes only
/// those, and no test waits for another. Before this, one lock for all of them
/// made 600 tests queue up behind each other.
#[cfg(test)]
pub(crate) struct LanguageGuard;

/// Whether the tests go through every size and language (`VERSEKIT_TESTS_VOLL=1`, set for the check before a
/// release). Without it a fixed sample runs, see [`test_sizes`].
#[cfg(test)]
pub(crate) fn full_run() -> bool {
    std::env::var("VERSEKIT_TESTS_VOLL").is_ok_and(|value| value == "1")
}

/// A test that costs minutes (it goes through every page or every size) runs in the full run only. In any other run
/// it says so and ends at once, so the quick run before a commit does not wait for it.
#[cfg(test)]
pub(crate) fn skip_unless_full_run() -> bool {
    if full_run() {
        return false;
    }
    eprintln!("skipped: runs only with VERSEKIT_TESTS_VOLL=1 (the check before a release)");
    true
}

/// The sizes of the text a test goes through: every one in the full run, else both ends and one in between. The
/// one in between is fixed by the name of the test (the same at every run, different from test to test), so no size
/// is left out by all the tests.
#[cfg(test)]
pub(crate) fn test_sizes() -> Vec<i64> {
    let (low, high) = versekit_core::font_size::POINTS_RANGE;
    if full_run() {
        return (low..=high).collect();
    }
    let name = std::thread::current().name().unwrap_or("").bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    let between = low + 1 + (name % (high - low - 1).max(1) as u64) as i64;
    vec![low, between, high]
}

/// Which of `all` a test that goes through widths, schemes or the like takes: all of them in the full run, else the
/// first, the last and one in between. The one in between is fixed by the name of the test (the same at every run,
/// different from test to test), so the smallest and the largest are always there and no value is left out by all
/// the tests.
#[cfg(test)]
pub(crate) fn test_pick<T: Copy>(all: &[T]) -> Vec<T> {
    if full_run() || all.len() <= 3 {
        return all.to_vec();
    }
    let name = std::thread::current().name().unwrap_or("").bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    let between = 1 + (name % (all.len() as u64 - 2)) as usize;
    vec![all[0], all[between], all[all.len() - 1]]
}

/// The schemes a test of the layout goes through: all in the full run, else the first (square) and the last
/// (round), which differ in the corners.
#[cfg(test)]
pub(crate) fn test_schemes() -> Vec<&'static versekit_core::theme::Scheme> {
    let all = versekit_core::theme::schemes();
    if full_run() {
        return all.iter().collect();
    }
    vec![&all[0], &all[all.len() - 1]]
}

/// Gives the test its own language and scheme (see [`LanguageGuard`]).
#[cfg(test)]
pub(crate) fn language_lock() -> LanguageGuard {
    versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    versekit_core::theme::set_thread_current(versekit_core::theme::schemes()[0].id);
    LanguageGuard
}

/// The longest a test waits for something that happens in a thread of its own (seconds). A wait is over as soon as
/// its condition holds, so the limit only matters for a fault; `VERSEKIT_TESTS_WARTEN` shortens it for the counter-check.
#[cfg(test)]
fn wait_limit() -> std::time::Duration {
    let seconds = std::env::var("VERSEKIT_TESTS_WARTEN").ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(60);
    std::time::Duration::from_secs(seconds)
}

/// Runs `step` again and again, a few milliseconds apart, until it says it is done or [`wait_limit`] has passed;
/// whether it was done. The wait ends on the condition, not after a count of tries: a count runs out early when
/// the machine is busy and the thread it waits for is slow.
#[cfg(test)]
pub(crate) fn wait_for(step: impl FnMut() -> bool) -> bool {
    wait_for_within(wait_limit(), step)
}

#[cfg(test)]
fn wait_for_within(limit: std::time::Duration, mut step: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + limit;
    loop {
        if step() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::theme;

    #[test]
    fn the_sample_always_holds_the_smallest_the_largest_and_both_ends_of_the_schemes() {
        let widths = [330.0_f32, 480.0, 640.0, 900.0, 1400.0];
        let taken = test_pick(&widths);
        assert!(taken.contains(&330.0) && taken.contains(&1400.0), "{taken:?}");
        assert!(taken.len() == 3 || full_run());
        assert_eq!(test_pick(&widths), taken, "the same at every run");
        assert_eq!(test_pick(&[1, 2]), [1, 2], "a short list is taken whole");
        let schemes = test_schemes();
        let all = theme::schemes();
        assert!(schemes.iter().any(|s| s.id == all[0].id) && schemes.iter().any(|s| s.id == all[all.len() - 1].id));
    }

    #[test]
    fn a_wait_ends_when_the_thread_is_done_however_slow_it_is() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let setter = flag.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            setter.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let started = std::time::Instant::now();
        assert!(wait_for_within(std::time::Duration::from_secs(30), || flag.load(std::sync::atomic::Ordering::SeqCst)));
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "it ends on the condition, not at the limit");
        thread.join().expect("thread");
    }

    #[test]
    fn a_wait_for_something_that_never_happens_ends_at_the_limit_and_says_so() {
        let started = std::time::Instant::now();
        assert!(!wait_for_within(std::time::Duration::from_millis(300), || false));
        assert!(started.elapsed() >= std::time::Duration::from_millis(300));
    }

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

    /// The sizes of text a style resolves, small to large, with the title of a card.
    fn sizes_of(style: &egui::Style) -> Vec<f32> {
        let size = |text: egui::TextStyle| text.resolve(style).size;
        let body = size(egui::TextStyle::Body);
        let mut sizes = vec![
            size(egui::TextStyle::Small),
            body,
            size(egui::TextStyle::Button),
            size(egui::TextStyle::Monospace),
            body * widgets::CARD_TITLE_SCALE,
            size(egui::TextStyle::Heading),
        ];
        sizes.sort_by(f32::total_cmp);
        sizes.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        sizes
    }

    /// What is wrong with the sizes: too many of them, small text far below the body text, a heading far above it.
    fn size_faults(style: &egui::Style) -> Vec<String> {
        let sizes = sizes_of(style);
        let body = egui::TextStyle::Body.resolve(style).size;
        let mut faults = Vec::new();
        if sizes.len() > 4 {
            faults.push(format!("{} sizes of text: {sizes:?}", sizes.len()));
        }
        if sizes[0] < 0.85 * body {
            faults.push(format!("the smallest text {} is far below the body text {body}", sizes[0]));
        }
        if sizes[sizes.len() - 1] > 1.35 * body {
            faults.push(format!("the largest text {} is far above the body text {body}", sizes[sizes.len() - 1]));
        }
        faults
    }

    #[test]
    fn the_interface_uses_few_text_sizes_close_to_the_body_text() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut style = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| style = Some(ui.style().as_ref().clone()));
        output.textures_delta.clear();
        let style = style.expect("a frame ran");
        assert_eq!(egui::TextStyle::Small.resolve(&style).size, SMALL_SIZE, "install sets the sizes");
        assert_eq!(size_faults(&style), Vec::<String>::new());
    }

    #[test]
    fn the_size_check_sees_egui_s_own_sizes() {
        // Counter-check: egui's own sizes (small 9 under body 13, heading 18) are what the check is there to catch.
        let faults = size_faults(&egui::Style::default());
        assert!(faults.iter().any(|f| f.contains("smallest")), "{faults:?}");
        assert!(faults.iter().any(|f| f.contains("largest")), "{faults:?}");
    }

    #[test]
    fn krt_rounds_like_design_system_3_and_the_others_as_before() {
        let krt = theme::by_id("krt").expect("krt exists");
        assert_eq!(visuals(krt).widgets.inactive.corner_radius, CornerRadius::same(6));
        assert_eq!(visuals(krt).window_corner_radius, CornerRadius::same(10));
        let original = theme::by_id("original").expect("original exists");
        assert_eq!(visuals(original).widgets.inactive.corner_radius, CornerRadius::same(CONTROL_RADIUS));
    }
}
