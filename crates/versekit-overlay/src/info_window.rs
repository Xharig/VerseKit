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

//! The info bar over the game: a window of its own beside the overlay, a second viewport of the same egui context
//! (Windows and X11). It draws what `versekit_ui::info_bar` draws, never takes the keyboard (the guard of the
//! overlay is hooked into it once it is found), stays on top and follows the drag of the mouse. Where it was
//! dragged to is kept for the size of the main screen (`versekit_core::info_bar`).

use std::time::{Duration, Instant};

use eframe::egui;
use versekit_core::info_bar::{self, Corner, Places};
use versekit_core::overlay::Pointer;
use versekit_ui::handle_search::HandleSearch;
use versekit_ui::info_bar as view;

use super::{os, screens_now};

/// The width before the first frame measured the bar, in points.
const FIRST_WIDTH: f32 = 300.0;
/// How often the native window is looked for after it was made, before the search gives up.
const LOOKS: u32 = 20;
/// Names a file that gets one line for each step of the bar with the keyboard (showing, window found, press,
/// cursor in the field, keyboard asked for); unset, nothing is written.
const TRAIL_ENV: &str = "VERSEKIT_LEISTE_SPUR";

/// Appends `line` to the file of [`TRAIL_ENV`], with the seconds since 1970 in front.
fn trail(line: &str) {
    let Some(path) = std::env::var_os(TRAIL_ENV) else { return };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{:.3} {line}", versekit_core::updater::now());
    }
}

/// What a step found out for the overlay.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// A drag ended: the places with the new one, to be stored.
    pub store: Option<Places>,
}

pub struct InfoWindow {
    id: egui::ViewportId,
    /// The top left corner in screen pixels; `None` until the bar stands somewhere.
    at: Option<(i32, i32)>,
    /// While a drag runs: where the pointer holds the bar, from its top left corner.
    grab: Option<(i32, i32)>,
    places: Places,
    /// Where the bar starts while it was not dragged, and whether the settings gave one yet.
    corner: Corner,
    corner_taken: bool,
    /// The width and height the bar asked for in the last frame, in points, and whether a frame measured it yet.
    width: f32,
    height: f32,
    measured: bool,
    /// The bar's own size relative to the overlay's: its viewport is drawn this many times as large.
    zoom: f32,
    /// Where the window was put in the last frame, in screen pixels (read by the checks).
    #[cfg_attr(not(test), allow(dead_code))]
    placed: Option<(i32, i32)>,
    /// The window may take the keyboard: the field of the search has the cursor and the lock is open.
    typing: bool,
    /// The hotkey of the search asked for the cursor in the field, for the next frame.
    key_asked: bool,
    /// The hotkey put the cursor into the field: it keeps the keyboard with the lock closed too, until the field
    /// gives the cursor up (Enter, Escape).
    by_key: bool,
    /// The handle of the native window once found, and how often it was looked for.
    native: isize,
    looked: u32,
    last_topmost: Instant,
    shown: bool,
    /// Clicks pass through the bar: the lock of the overlay is closed (read by the checks).
    #[cfg_attr(not(test), allow(dead_code))]
    passing: bool,
}

impl InfoWindow {
    pub fn new(places: Places) -> InfoWindow {
        InfoWindow {
            id: egui::ViewportId::from_hash_of("versekit-infoleiste"),
            at: None,
            grab: None,
            places,
            corner: Corner::default(),
            corner_taken: false,
            width: FIRST_WIDTH,
            height: view::HEIGHT,
            measured: false,
            zoom: 1.0,
            placed: None,
            typing: false,
            key_asked: false,
            by_key: false,
            native: 0,
            looked: 0,
            last_topmost: Instant::now(),
            shown: false,
            passing: false,
        }
    }

    /// Do clicks pass through the bar?
    #[cfg(test)]
    pub fn passing(&self) -> bool {
        self.passing
    }

    /// The viewport of the bar.
    #[cfg(test)]
    pub fn id(&self) -> egui::ViewportId {
        self.id
    }

    /// Where the window stands in this frame, in screen pixels: the place kept, moved left so that the bar and its
    /// card end on the screen.
    #[cfg(test)]
    pub fn window_at(&self) -> Option<(i32, i32)> {
        self.placed.filter(|_| self.shown)
    }

    /// Is the bar shown?
    #[cfg(test)]
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// The top left corner in screen pixels, while the bar stands.
    #[cfg(test)]
    pub fn at(&self) -> Option<(i32, i32)> {
        self.at.filter(|_| self.shown)
    }

    /// May the bar take the keyboard?
    #[cfg(test)]
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// Waits a press of the hotkey of the search for the next frame?
    #[cfg(test)]
    pub fn key_asked(&self) -> bool {
        self.key_asked
    }

    /// The hotkey of the search was pressed: the next frame puts the cursor into the field, and the window takes the
    /// keyboard from the game, with the lock closed too.
    pub fn type_by_key(&mut self) {
        self.key_asked = true;
    }

    /// Lets the window take the keyboard while the field of the search has the cursor, and gives it back.
    fn allow_typing(&mut self, on: bool) {
        if on != self.typing {
            self.typing = on;
            // The field wants the keyboard but does not get it: a line in the error log, so that a report shows it.
            if self.native != 0 {
                let done = os::allow_typing(self.native, on);
                trail(&format!("keyboard {on} window {} done {done}", self.native));
                if on && !done {
                    versekit_core::errors::record("info_window.keyboard", &"the system refused the keyboard");
                }
            } else {
                trail(&format!("keyboard {on} without a window"));
                if on {
                    versekit_core::errors::record("info_window.keyboard", &"the field has the cursor, no window yet");
                }
            }
        }
    }

    /// Takes the places the settings hold now.
    pub fn set_places(&mut self, places: Places) {
        if places != self.places && self.grab.is_none() {
            let screen = screens_now().0;
            let key = info_bar::screen_key(screen);
            if places.place_for(&key) != self.places.place_for(&key) {
                self.at = None;
            }
            self.places = places;
        }
    }

    /// Takes the corner the settings hold now: a new one places the bar anew. True when the player chose another
    /// corner while the bar ran (not for the first one taken from the settings): the place dragged to is to be
    /// forgotten, or the choice would not show.
    pub fn set_corner(&mut self, corner: Corner) -> bool {
        let first = !std::mem::replace(&mut self.corner_taken, true);
        if corner == self.corner || self.grab.is_some() {
            return false;
        }
        self.corner = corner;
        self.at = None;
        self.measured = false;
        !first
    }

    /// Takes the bar's own size relative to the overlay's (`zoom`, 1 for the same size). Another size places the bar
    /// anew, so that at the right corner its right end stays on the edge.
    pub fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
        if zoom != self.zoom && self.grab.is_none() {
            self.zoom = zoom;
            self.at = None;
            self.measured = false;
        }
        versekit_ui::viewport_zoom::set(ctx, self.id, self.zoom);
    }

    /// The bar was not dragged on this size of the main screen: it stands at its corner.
    fn at_corner(&self) -> bool {
        self.places.place_for(&info_bar::screen_key(screens_now().0)).is_none()
    }

    /// The bar for this frame: shown while `on`, else gone (an immediate viewport that is not shown in a frame
    /// closes). `pointer` is where the system says the pointer is; `search` fills the middle of the bar.
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        on: bool,
        content: &view::View,
        pointer: Option<Pointer>,
        mut search: Option<&mut HandleSearch>,
    ) -> Step {
        let mut out = Step::default();
        if !on {
            self.allow_typing(false);
            (self.key_asked, self.by_key) = (false, false);
            if self.shown {
                trail(&format!("hidden, window {}", self.native));
                self.shown = false;
                self.native = 0;
                self.looked = 0;
                self.grab = None;
            }
            return out;
        }
        // The window system counts in points of the overlay (`ppp`); the bar's own points are `zoom` times as large.
        let ppp = ctx.pixels_per_point().max(0.01);
        let own = ppp * self.zoom;
        let width_px = (self.width * own).round() as i32;
        let (x, y) = *self.at.get_or_insert_with(|| {
            let (screen, work) = screens_now();
            info_bar::start_place(&self.places, screen, work, width_px, self.corner, versekit_core::screen::on_a_screen)
        });
        // A card wider than the room to the right of the bar moves the window left, never past the edge; where the
        // bar is kept stays as it was.
        let (x, y) = if self.grab.is_none() { (keep_inside(x, width_px, super::work_at(x, y)), y) } else { (x, y) };
        self.placed = Some((x, y));
        let size = view::size(self.width, self.height) * self.zoom;
        let builder = egui::ViewportBuilder::default()
            .with_title(versekit_core::language::window_title(&versekit_core::language::t("ib_fenstertitel")))
            .with_app_id(versekit_core::paths::WM_CLASS)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_active(false)
            .with_taskbar(false)
            .with_position([x as f32 / ppp, y as f32 / ppp])
            .with_inner_size(size)
            .with_mouse_passthrough(!content.movable)
            .with_visible(self.may_show());
        self.passing = !content.movable;
        let fresh = !self.shown;
        if fresh {
            trail(&format!("shown, window {} movable {}", self.native, content.movable));
        }
        // The cursor the hotkey asked for is put into the field once it is drawn, also with the lock closed.
        let asked = std::mem::take(&mut self.key_asked) && search.is_some();
        let by_key = self.by_key || asked;
        let (mut drawn, primary_down) = ctx.show_viewport_immediate(self.id, builder, |ui, class| {
            if ui.input(|i| i.pointer.any_pressed() || i.pointer.any_released()) {
                let (down, focused) = ui.input(|i| (i.pointer.primary_down(), i.focused));
                let typing = search.as_deref().is_some_and(|search| search.typing(ui.ctx()));
                let embedded = class == egui::ViewportClass::EmbeddedWindow;
                trail(&format!("press {down} focused {focused} cursor {typing} embedded {embedded}"));
            }
            let mut drawn = view::show_with(ui, content, search.as_deref_mut());
            // The lock closed while the field had the cursor, or the bar comes back with the cursor it had when it
            // went: the cursor goes (the focus is kept per window, also while the window is gone). The cursor the
            // hotkey put there stays.
            if drawn.typing
                && (!content.movable && !by_key || fresh)
                && let Some(search) = search.as_deref()
            {
                search.stop_typing(ui.ctx());
                drawn.typing = false;
            }
            if asked && let Some(search) = search.as_deref() {
                search.focus_field(ui.ctx());
                ui.ctx().request_repaint();
            }
            (drawn, ui.input(|i| i.pointer.primary_down()))
        });
        if asked {
            trail(&format!("hotkey, window {} cursor asked", self.native));
        }
        // The field gave the cursor up: the hotkey's hold on the keyboard ends with it.
        self.by_key = asked || self.by_key && drawn.typing;
        self.shown = true;
        if drawn.width > 0.0 && (drawn.width - self.width).abs() > 0.5 {
            self.width = drawn.width;
            // At the right corner the right end stays: the bar grows to the left.
            if self.corner == Corner::Right && self.grab.is_none() && self.at_corner() {
                self.measured = false;
            }
            ctx.request_repaint();
        }
        if drawn.height > 0.0 && (drawn.height - self.height).abs() > 0.5 {
            self.height = drawn.height;
            ctx.request_repaint();
        }
        // The keyboard only while the field has the cursor and the lock is open, or the hotkey put the cursor there.
        drawn.typing &= content.movable || self.by_key;
        self.allow_typing(drawn.typing);
        // Placed once more with the width the first frame measured: in the middle for the real width. After that
        // the left end stays and the bar grows to the right.
        if drawn.width > 0.0 && !std::mem::replace(&mut self.measured, true) && self.grab.is_none() {
            let (screen, work) = screens_now();
            let width_px = (drawn.width * own).round() as i32;
            self.at = Some(info_bar::start_place(
                &self.places,
                screen,
                work,
                width_px,
                self.corner,
                versekit_core::screen::on_a_screen,
            ));
        }
        self.find_native();
        // A drag starts from where the window stands.
        if drawn.drag && self.grab.is_none() {
            self.at = Some((x, y));
        }
        // The lock closed while the bar was held: the drag ends where the bar stands.
        let held = primary_down && content.movable;
        self.follow_drag(ctx, drawn.drag, held, pointer, own, &mut out);
        if self.native != 0 && self.last_topmost.elapsed() >= Duration::from_secs(1) {
            os::reassert_topmost(self.native);
            self.last_topmost = Instant::now();
        }
        if !self.may_show() {
            ctx.request_repaint();
        }
        out
    }

    /// May the bar be on the screen yet? On Windows only once the guard is in (or the search gave up): shown
    /// before, it would be a window that takes the focus for that moment. X11 lists a window only once it is
    /// shown, and it takes the hint against the keyboard later as well.
    fn may_show(&self) -> bool {
        !cfg!(windows) || self.native != 0 || self.looked >= LOOKS
    }

    /// Looks for the native window a few times after it was made and hooks the guard into it.
    fn find_native(&mut self) {
        if self.native != 0 || self.looked >= LOOKS {
            return;
        }
        self.looked += 1;
        let title = versekit_core::language::window_title(&versekit_core::language::t("ib_fenstertitel"));
        #[cfg(windows)]
        {
            self.native = crate::win::own_window_titled(&title);
            if self.native != 0 {
                crate::win::guard_also(self.native);
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.native = super::x11::own_window_named(&title);
            if self.native != 0 {
                super::x11::install(self.native);
            }
        }
        if self.native != 0 {
            trail(&format!("window found {} after {} looks, keyboard {}", self.native, self.looked, self.typing));
        }
        // The field took the cursor before the window was found: the window gets the keyboard now.
        if self.native != 0 && self.typing {
            os::allow_typing(self.native, true);
        }
        #[cfg(target_os = "macos")]
        let _ = title;
    }

    /// The bar follows the pointer while a drag runs; let go a few pixels before an edge of the screen under its
    /// middle, it goes onto that edge and wholly onto that screen (as the overlay does), and the place is kept for the
    /// size of the main screen. `own` is the scale of the bar (points to pixels).
    fn follow_drag(
        &mut self,
        ctx: &egui::Context,
        began: bool,
        primary_down: bool,
        pointer: Option<Pointer>,
        own: f32,
        out: &mut Step,
    ) {
        let Some((x, y)) = self.at else { return };
        if began && let Some(p) = pointer {
            self.grab = Some((p.x - x, p.y - y));
        }
        let Some((gx, gy)) = self.grab else { return };
        if primary_down {
            if let Some(p) = pointer {
                self.at = Some((p.x - gx, p.y - gy));
            }
            ctx.request_repaint();
            return;
        }
        self.grab = None;
        let (w, h) = ((self.width * own).round() as i32, (self.height * own).round() as i32);
        let (cx, cy) = (x + w / 2, y + h / 2);
        let screen = versekit_core::screen::monitor_area(cx, cy, false);
        let work = versekit_core::screen::monitor_area(cx, cy, true);
        let (x, y) = match (screen, work) {
            (Some(area), Some(work)) => {
                let bar = versekit_core::overlay::Rect::new(x, y, w, h);
                let area = versekit_core::overlay::Rect::from_area(area);
                let snapped = versekit_core::overlay::snapped(bar, area, versekit_core::overlay::Rect::from_area(work));
                (snapped.x, snapped.y)
            }
            _ => (x, y),
        };
        self.at = Some((x, y));
        let key = info_bar::screen_key(screens_now().0);
        self.places = self.places.with_place(&key, [x, y]);
        out.store = Some(self.places.clone());
        ctx.request_repaint();
    }
}

/// The left end of a window `width` pixels wide at `x`, moved left so that it ends inside `work`, but never past
/// its left edge.
fn keep_inside(x: i32, width: i32, work: versekit_core::overlay::Rect) -> i32 {
    x.min(work.x + work.w - width).max(work.x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_card_moves_the_window_left_inside_the_screen() {
        let work = versekit_core::overlay::Rect::new(0, 0, 2560, 1392);
        assert_eq!(keep_inside(2400, 356, work), 2560 - 356, "the right edge stays on the screen");
        assert_eq!(keep_inside(1000, 356, work), 1000, "room enough: it stays");
        assert_eq!(keep_inside(10, 3000, work), 0, "never past the left edge");
        let second = versekit_core::overlay::Rect::new(2560, 0, 1920, 1080);
        assert_eq!(keep_inside(4400, 356, second), 2560 + 1920 - 356);
    }
    use versekit_core::screen::{self, Monitor, Setup};
    use versekit_core::theme;

    /// A main screen of 2560 x 1440 pixels with a panel of 48 at the bottom.
    fn one_screen() -> Setup {
        Setup { scale: 1.0, primary: 0, monitors: vec![Monitor { area: (0, 0, 2560, 1440), work: (0, 0, 2560, 1392) }] }
    }

    fn pointer(x: i32, y: i32) -> Option<Pointer> {
        Some(Pointer { hidden: false, x, y })
    }

    /// One frame of the bar in a context without windows (the bar is drawn inside it).
    fn frame(info: &mut InfoWindow, ctx: &egui::Context, on: bool, movable: bool) -> Step {
        let content = view::View {
            scheme: theme::by_id("original").expect("scheme"),
            fps: None,
            players: None,
            shard: Some("euw1b · 420"),
            mining: None,
            opacity: 1.0,
            movable,
        };
        let mut step = Step::default();
        let mut output =
            ctx.run_ui(egui::RawInput::default(), |ui| step = info.step(ui.ctx(), on, &content, pointer(0, 0), None));
        output.textures_delta.clear();
        step
    }

    /// Switched on, the bar stands at the top in the middle of the main screen; switched off it is gone.
    #[test]
    fn the_bar_starts_at_the_top_in_the_middle_and_goes_when_switched_off() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut info = InfoWindow::new(Places::default());
        frame(&mut info, &ctx, false, true);
        assert!(!info.shown() && info.at().is_none(), "off: no bar");
        frame(&mut info, &ctx, true, true);
        frame(&mut info, &ctx, true, true);
        // In the middle for the width the first frame measured, at the top.
        assert!(info.width > 100.0 && info.width != FIRST_WIDTH, "measured: {}", info.width);
        let width_px = (info.width * ctx.pixels_per_point()).round() as i32;
        assert_eq!(info.at(), Some(((2560 - width_px) / 2, 0)));
        // A wider content keeps the left end: the bar grows to the right.
        let before = info.at();
        info.width += 80.0;
        frame(&mut info, &ctx, true, true);
        assert_eq!(info.at(), before, "the left end stays");
        frame(&mut info, &ctx, false, true);
        assert!(!info.shown(), "off again: gone");
    }

    /// At twice its size the bar is twice as wide on the screen: at the right corner its right end stays on the edge
    /// and it grows to the left, and dragged close to the right edge the window moves left so that the bar ends on
    /// the screen. The place kept stays as it was.
    #[test]
    fn a_larger_bar_grows_to_the_left_and_stays_on_the_screen() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let shown = |places: Places, corner: Corner, zoom: f32| {
            let mut info = InfoWindow::new(places);
            info.set_corner(corner);
            info.set_zoom(&ctx, zoom);
            for _ in 0..3 {
                frame(&mut info, &ctx, true, true);
            }
            let width_px = (info.width * ctx.pixels_per_point() * zoom).round() as i32;
            (info.window_at().expect("shown"), width_px, info.at().expect("kept"))
        };
        let ((x1, _), w1, _) = shown(Places::default(), Corner::Right, 1.0);
        let ((x2, y2), w2, _) = shown(Places::default(), Corner::Right, 2.0);
        assert_eq!(x1 + w1, 2560, "100 %: the right end on the edge");
        assert!((w2 - 2 * w1).abs() <= 1, "twice as wide: {w1} {w2}");
        assert_eq!((x2 + w2, y2), (2560, 0), "200 %: the right end stays on the edge, the bar at the top");
        assert!(x2 < x1, "it grew to the left: {x1} {x2}");
        let dragged = Places::default().with_place("2560x1440", [2560 - w1 - 10, 300]);
        let ((x3, y3), w3, kept) = shown(dragged, Corner::Middle, 2.0);
        assert!(x3 >= 0 && x3 + w3 <= 2560, "on the screen: {x3} + {w3}");
        assert_eq!((x3 + w3, y3), (2560, 300), "moved left just enough");
        assert_eq!(kept, (2560 - w1 - 10, 300), "the place kept stays");
        // Back at the same size the bar is where the corner puts it again.
        let ((x4, _), w4, _) = shown(Places::default(), Corner::Right, 1.0);
        assert_eq!((x4, w4), (x1, w1));
    }

    /// A drag moves the bar with the pointer; at its end the place is kept for the size of the main screen and
    /// handed on for storing, and the next start finds it there.
    #[test]
    fn a_drag_moves_the_bar_and_keeps_its_place_for_the_screen_size() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        let mut info = InfoWindow::new(Places::default());
        info.at = Some((1000, 0));
        let mut out = Step::default();
        info.follow_drag(&ctx, true, true, pointer(1010, 10), 1.0, &mut out);
        info.follow_drag(&ctx, false, true, pointer(410, 610), 1.0, &mut out);
        assert_eq!(info.at, Some((400, 600)), "follows the pointer, held where it was taken");
        assert_eq!(out.store, None, "nothing stored while held");
        info.follow_drag(&ctx, false, false, pointer(410, 610), 1.0, &mut out);
        let places = out.store.expect("stored at the end");
        assert_eq!(places.place_for("2560x1440"), Some([400, 600]));
        let again = InfoWindow::new(places);
        let (screen, work) = screens_now();
        assert_eq!(info_bar::start_place(&again.places, screen, work, 300, Corner::Middle, |_, _, _| true), (400, 600));
        // Without a drag nothing moves and nothing is stored.
        let mut quiet = Step::default();
        info.follow_drag(&ctx, false, false, pointer(5, 5), 1.0, &mut quiet);
        assert_eq!((info.at, quiet.store), (Some((400, 600)), None));
    }

    /// Let go a few pixels before an edge of the screen, the bar goes onto it and wholly onto the screen, like the
    /// overlay; further away it stays where it was let go.
    #[test]
    fn a_bar_let_go_near_an_edge_snaps_onto_it() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        let mut info = InfoWindow::new(Places::default());
        (info.width, info.height) = (300.0, 40.0);
        let let_go = |info: &mut InfoWindow, x: i32, y: i32| {
            info.at = Some((1000, 500));
            let mut out = Step::default();
            info.follow_drag(&ctx, true, true, pointer(1000, 500), 2.0, &mut out);
            info.follow_drag(&ctx, false, true, pointer(x, y), 2.0, &mut out);
            info.follow_drag(&ctx, false, false, pointer(x, y), 2.0, &mut out);
            (info.at, out.store.and_then(|p| p.place_for("2560x1440")))
        };
        // 600 x 80 pixels at scale 2: 10 pixels before the right edge and the top it snaps onto both.
        assert_eq!(let_go(&mut info, 2560 - 600 - 10, 10), (Some((1960, 0)), Some([1960, 0])));
        // Over the panel at the bottom (work area ends at 1392): onto the work area.
        assert_eq!(let_go(&mut info, 800, 1392 - 80 - 6).0, Some((800, 1312)));
        // In the middle nothing moves.
        assert_eq!(let_go(&mut info, 900, 700).0, Some((900, 700)));
    }

    /// The bar takes the keyboard only while the field of the search has the cursor and the lock is open; the
    /// lock closing takes the cursor out of the field and the keyboard back.
    #[test]
    fn the_bar_takes_the_keyboard_only_while_the_field_is_typed_in_with_the_lock_open() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut info = InfoWindow::new(Places::default());
        let mut search = HandleSearch::empty();
        let content = |movable| view::View {
            scheme: theme::by_id("original").expect("scheme"),
            fps: None,
            players: None,
            shard: None,
            mining: None,
            opacity: 1.0,
            movable,
        };
        let run = |info: &mut InfoWindow, search: &mut HandleSearch, movable: bool, focus: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                if focus {
                    search.focus_field(ui.ctx());
                }
                info.step(ui.ctx(), true, &content(movable), pointer(0, 0), Some(search));
            });
            output.textures_delta.clear();
        };
        run(&mut info, &mut search, true, false);
        assert!(!info.typing(), "no cursor in the field: no keyboard");
        run(&mut info, &mut search, true, true);
        run(&mut info, &mut search, true, false);
        assert!(info.typing(), "cursor in the field, lock open: the keyboard");
        run(&mut info, &mut search, false, false);
        assert!(!info.typing(), "lock closed: the keyboard goes back");
        run(&mut info, &mut search, false, false);
        assert!(!search.typing(&ctx), "and the cursor left the field");
        // Counter-check: with the lock closed the cursor asked for brings no keyboard.
        run(&mut info, &mut search, false, true);
        run(&mut info, &mut search, false, false);
        assert!(!info.typing());
    }

    /// The hotkey of the search puts the cursor into the field and gives the bar the keyboard, with the lock closed
    /// too; Escape takes the cursor out of the field and gives the keyboard back, and the closed lock holds again.
    #[test]
    fn the_hotkey_of_the_search_gives_the_field_the_cursor_and_the_bar_the_keyboard() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut info = InfoWindow::new(Places::default());
        let mut search = HandleSearch::empty();
        let content = view::View {
            scheme: theme::by_id("original").expect("scheme"),
            fps: None,
            players: None,
            shard: None,
            mining: None,
            opacity: 1.0,
            movable: false,
        };
        let run = |info: &mut InfoWindow, search: &mut HandleSearch, events: Vec<egui::Event>, focus: bool| {
            let input = egui::RawInput { events, ..Default::default() };
            let mut output = ctx.run_ui(input, |ui| {
                if focus {
                    search.focus_field(ui.ctx());
                }
                info.step(ui.ctx(), true, &content, pointer(0, 0), Some(search));
            });
            output.textures_delta.clear();
        };
        run(&mut info, &mut search, Vec::new(), false);
        assert!(!info.typing(), "lock closed, no hotkey: no keyboard");
        info.type_by_key();
        run(&mut info, &mut search, Vec::new(), false);
        run(&mut info, &mut search, Vec::new(), false);
        assert!(info.typing(), "the hotkey: the keyboard, with the lock closed");
        assert!(search.typing(&ctx), "and the cursor in the field");
        run(&mut info, &mut search, Vec::new(), false);
        assert!(info.typing(), "it stays while the field has the cursor");
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut info, &mut search, vec![escape], false);
        run(&mut info, &mut search, Vec::new(), false);
        assert!(!search.typing(&ctx), "Escape: the cursor left the field");
        assert!(!info.typing(), "and the keyboard went back");
        // Counter-check: the cursor put into the field otherwise is taken out again while the lock is closed.
        run(&mut info, &mut search, Vec::new(), true);
        run(&mut info, &mut search, Vec::new(), false);
        assert!(!info.typing() && !search.typing(&ctx));
        // The bar switched off forgets a hotkey press it did not show yet.
        info.type_by_key();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            info.step(ui.ctx(), false, &content, pointer(0, 0), Some(&mut search));
        });
        output.textures_delta.clear();
        assert!(!info.key_asked && !info.by_key);
    }

    /// The bar hidden while the field has the cursor and shown again: it comes back without the cursor, and a
    /// click into the field gives it the keyboard as on the first showing.
    #[test]
    fn the_bar_shown_again_starts_without_the_cursor_and_takes_the_keyboard_on_a_click() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut info = InfoWindow::new(Places::default());
        let mut search = HandleSearch::empty();
        let content = view::View {
            scheme: theme::by_id("original").expect("scheme"),
            fps: None,
            players: None,
            shard: None,
            mining: None,
            opacity: 1.0,
            movable: true,
        };
        let run = |info: &mut InfoWindow, search: &mut HandleSearch, on: bool, focus: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                if focus {
                    search.focus_field(ui.ctx());
                }
                info.step(ui.ctx(), on, &content, pointer(0, 0), Some(search));
            });
            output.textures_delta.clear();
        };
        run(&mut info, &mut search, true, false);
        run(&mut info, &mut search, true, true);
        run(&mut info, &mut search, true, false);
        assert!(info.typing(), "the field has the cursor");
        run(&mut info, &mut search, false, false);
        assert!(!info.typing() && !info.shown(), "hidden: no keyboard");
        run(&mut info, &mut search, true, false);
        run(&mut info, &mut search, true, false);
        assert!(!info.typing(), "shown again: the old cursor does not come back");
        assert!(!search.typing(&ctx), "and the field has none");
        run(&mut info, &mut search, true, true);
        run(&mut info, &mut search, true, false);
        assert!(info.typing(), "a click into the field: the keyboard again");
    }

    /// Closed lock: clicks pass through the bar; open lock: it takes them. A drag under way ends when the lock
    /// closes.
    #[test]
    fn the_closed_lock_lets_clicks_through_the_bar_and_ends_a_drag() {
        let _screens = screen::fixed(one_screen());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut info = InfoWindow::new(Places::default());
        frame(&mut info, &ctx, true, true);
        assert!(!info.passing(), "open lock: the bar takes the mouse");
        frame(&mut info, &ctx, true, false);
        assert!(info.passing(), "closed lock: clicks pass through");
        frame(&mut info, &ctx, true, true);
        assert!(!info.passing(), "open again");
        info.grab = Some((5, 5));
        let step = frame(&mut info, &ctx, true, false);
        assert_eq!(info.grab, None, "the drag ended");
        assert!(step.store.is_some(), "where it stood is kept");
    }
}
