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

//! The hint of a button of the overlay in a small window of its own next to the pointer. A tooltip inside the
//! overlay is cut off where the window ends, and the folded overlay is only as high as its bar. This window never
//! takes the keyboard (the guard of the overlay is hooked into it once it is found), lets every click through and
//! stays on top.

use eframe::egui;
use versekit_core::overlay::{Pointer, Rect, hint_place};
use versekit_core::theme::Scheme;
use versekit_ui::color;

/// How often the native window is looked for after it was made, before the search gives up.
const LOOKS: u32 = 20;
/// The widest the window gets before the text wraps, in points.
const MAX_WIDTH: f32 = 360.0;

/// What the frame around the text adds to it, in points: the margins (8 left and right, 4 above and below) and the
/// stroke of the frame.
const FRAME: egui::Vec2 = egui::vec2(18.0, 10.0);

/// How many times in a row the window of a hint is placed again because it reported another scale than the one it
/// was placed with, before it is shown with the last one all the same: a scale that keeps jumping must not keep the
/// window invisible for good.
const MAX_REPLACES: u32 = 3;

/// What is known of the scale of the window of the hint: the scale it is placed with (`own`, 0 = not known yet),
/// whether the window said the same (`confirmed`: only then it is shown) and how many times in a row it said another
/// (`tries`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scale {
    own: f32,
    confirmed: bool,
    tries: u32,
}

impl Scale {
    /// The window stood placed with `used` and reported `reported`: the next state. The same scale confirms it; another
    /// is taken over and the window is placed again, hidden meanwhile, but only [`MAX_REPLACES`] times in a row, then
    /// it is shown with the last scale it reported, which it goes on following without hiding again.
    fn next(self, used: f32, reported: f32) -> Scale {
        if reported <= 0.0 || (reported - used).abs() <= 0.005 {
            return Scale { own: used, confirmed: true, tries: 0 };
        }
        let tries = self.tries.saturating_add(1);
        Scale { own: reported, confirmed: tries >= MAX_REPLACES, tries }
    }
}

pub struct HintWindow {
    id: egui::ViewportId,
    /// The size the text asks for, in points, measured in the same frame the window is placed in.
    size: egui::Vec2,
    /// The scale of the window of the hint itself (it can be made on another monitor than the overlay stands on).
    scale: Scale,
    /// The scales the window of the hint had on the screens it was on, by work area: the next hint starts with the
    /// one of the screen it is on, so it is not hidden for the frames the scale takes to be known.
    known: Vec<(Rect, f32)>,
    /// Where the window was put last, in screen pixels.
    placed: Option<Rect>,
    native: isize,
    looked: u32,
    shown: bool,
}

impl HintWindow {
    pub fn new() -> HintWindow {
        HintWindow {
            id: egui::ViewportId::from_hash_of("versekit-hinweis"),
            size: egui::vec2(120.0, 24.0),
            scale: Scale { own: 0.0, confirmed: false, tries: 0 },
            known: Vec::new(),
            placed: None,
            native: 0,
            looked: 0,
            shown: false,
        }
    }

    /// Is the window shown?
    #[cfg(test)]
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Where the window was put last, in screen pixels.
    #[cfg(test)]
    pub fn window(&self) -> Option<Rect> {
        self.placed
    }

    /// The size `text` needs in a window, in points, wrapped at `max_content` points: measured here, so the window
    /// is made at its size at once and never one frame late.
    fn measure(ctx: &egui::Context, text: &str, max_content: f32) -> egui::Vec2 {
        let font = egui::TextStyle::Body.resolve(&ctx.global_style());
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.layout(text.to_owned(), font, egui::Color32::WHITE, max_content).size() + FRAME
    }

    /// Shows `text` next to the pointer while both are there; otherwise the window goes (an immediate viewport
    /// that is not shown in a frame closes).
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        text: Option<&str>,
        pointer: Option<Pointer>,
        symbol: Option<Rect>,
        scheme: &Scheme,
    ) {
        let (Some(text), Some(pointer)) = (text, pointer) else {
            if self.shown {
                self.shown = false;
                self.native = 0;
                self.looked = 0;
                self.scale = Scale { own: 0.0, confirmed: false, tries: 0 };
                self.placed = None;
            }
            return;
        };
        let anchor = symbol.unwrap_or(Rect::new(pointer.x, pointer.y, 1, 1));
        let area = super::work_at(anchor.x.saturating_add(anchor.w / 2), anchor.y.saturating_add(anchor.h / 2));
        // The scale of the window of the hint decides the pixels: winit turns the points it is given into pixels
        // with the scale of the monitor the window is on, which is not always the one of the overlay. A new hint
        // starts with the scale this screen had last time, else with the one of the overlay.
        if self.scale.own <= 0.0 {
            let known = self.known.iter().find(|(seen, _)| *seen == area).map(|(_, ppp)| *ppp);
            self.scale.own = known.unwrap_or_else(|| ctx.pixels_per_point().max(0.01));
        }
        let ppp = self.scale.own;
        let window = self.place(ctx, text, ppp, (pointer.x, pointer.y), anchor, area);
        self.placed = Some(window);
        let at = egui::pos2(window.x as f32 / ppp, window.y as f32 / ppp);
        let inner = egui::vec2(window.w as f32 / ppp, window.h as f32 / ppp);
        let builder = egui::ViewportBuilder::default()
            .with_title(versekit_core::language::window_title(&versekit_core::language::t("ov_hinweis_fenstertitel")))
            .with_app_id(versekit_core::paths::WM_CLASS)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_active(false)
            .with_taskbar(false)
            .with_position(at)
            .with_inner_size(inner)
            .with_mouse_passthrough(true)
            .with_visible(self.may_show() && self.scale.confirmed);
        let own = ctx.show_viewport_immediate(self.id, builder, |ui, _class| {
            egui::Frame::new()
                .fill(color(scheme.bar))
                .stroke(egui::Stroke::new(1.0, color(scheme.line)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_max_width((window.w as f32 / ppp - FRAME.x).max(1.0));
                    ui.label(egui::RichText::new(text).color(color(scheme.fg)));
                });
            ui.ctx().pixels_per_point()
        });
        self.shown = true;
        // The window says its own scale: where it differs from the one used, it is placed again with it and stays
        // hidden meanwhile.
        let before = self.scale;
        self.scale = before.next(ppp, own);
        if self.scale != before {
            ctx.request_repaint();
        }
        if self.scale.confirmed && self.scale.tries == 0 {
            // Remember what this screen reported, once the window said the same twice (not a scale given up on).
            let seen = self.scale.own;
            match self.known.iter_mut().find(|(screen, _)| *screen == area) {
                Some(entry) => entry.1 = seen,
                None => self.known.push((area, seen)),
            }
        }
        self.find_native();
        if !self.may_show() {
            ctx.request_repaint();
        }
    }

    /// Where the window goes, in screen pixels and at its size: wrapped and cut to `area` where the text is wider
    /// or taller than the screen, on the screen of the symbol, clear of the symbol (`anchor`) and wholly in view,
    /// whichever edge the overlay stands at.
    fn place(&mut self, ctx: &egui::Context, text: &str, ppp: f32, near: (i32, i32), anchor: Rect, area: Rect) -> Rect {
        // The text wraps at the widest the screen has room for, with the strip kept free at both edges.
        let free = (area.w - 2 * versekit_core::overlay::HINT_RESERVE).max(1) as f32;
        let room = (free / ppp - FRAME.x).clamp(1.0, MAX_WIDTH);
        self.size = Self::measure(ctx, text, room);
        let px = |size: egui::Vec2| ((size.x * ppp).ceil() as i32, (size.y * ppp).ceil() as i32);
        let window = hint_place(anchor, near, px(self.size), area);
        if window.w >= px(self.size).0 {
            return window;
        }
        // The screen cut the width (the longest word does not fit): the height comes from the width there really
        // is, or the last line would be lost.
        self.size = Self::measure(ctx, text, (window.w as f32 / ppp - FRAME.x).max(1.0));
        hint_place(anchor, near, (window.w, px(self.size).1), area)
    }

    /// May the window be on the screen yet? On Windows only once the guard is in (or the search gave up): shown
    /// before, it would be a window that takes the focus for that moment.
    fn may_show(&self) -> bool {
        !cfg!(windows) || self.native != 0 || self.looked >= LOOKS
    }

    /// Looks for the native window a few times after it was made and hooks the guard into it.
    fn find_native(&mut self) {
        if self.native != 0 || self.looked >= LOOKS {
            return;
        }
        self.looked += 1;
        let title = versekit_core::language::window_title(&versekit_core::language::t("ov_hinweis_fenstertitel"));
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
        #[cfg(target_os = "macos")]
        let _ = title;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window of a hint stands wholly on the screen of the overlay and clear of the symbol, whether the overlay
    /// sits at an edge or in a corner of the screen, in both scales.
    #[test]
    fn the_window_stands_in_view_and_clear_of_the_symbol_at_every_edge_and_corner() {
        let area = super::super::work_at(0, 0);
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut hint = HintWindow::new();
        let (w, h) = (26, 26);
        let (left, top, right, bottom) = (area.x, area.y, area.right() - w, area.bottom() - h);
        let (mid_x, mid_y) = (area.x + area.w / 2, area.y + area.h / 2);
        let spots = [
            (mid_x, top),
            (mid_x, bottom),
            (left, mid_y),
            (right, mid_y),
            (left, top),
            (right, top),
            (left, bottom),
            (right, bottom),
        ];
        let long = "Anti-AFK aus. Klicken zum Einschalten (drückt nach der Wartezeit die Taste für „Visier abwischen“, \
                    solange das Spiel vorn ist und du nichts drückst)";
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            for ppp in [1.0, 1.25, 1.5, 2.0] {
                for text in ["Logs neu lesen", long] {
                    for (x, y) in spots {
                        let symbol = Rect::new(x, y, w, h);
                        let near = (x + w / 2, y + h / 2);
                        let window = hint.place(&ctx, text, ppp, near, symbol, area);
                        assert!(area.holds(window), "{window:?} is not on {area:?} for the symbol at {x},{y}");
                        assert!(!window.overlaps(symbol), "{window:?} covers the symbol {symbol:?}");
                    }
                }
            }
        });
        output.textures_delta.clear();
    }

    /// Text wider than the screen: the window is made at the cut size (the one asked for would run over the edge),
    /// and the text wraps inside it.
    #[test]
    fn a_text_wider_than_the_screen_gets_a_window_cut_to_it() {
        // So small that even the narrowest wrapped text (a longest word and the frame) is wider than the screen.
        let tiny = Rect::new(0, 0, 40, 30);
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut hint = HintWindow::new();
        let long =
            "Ein sehr langer Hinweis, der in ein so kleines Fenster nicht in einer Zeile passt und umbrechen muss";
        let symbol = Rect::new(15, 10, 10, 10);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            let window = hint.place(&ctx, long, 1.0, (20, 15), symbol, tiny);
            let asked = (hint.size.x.ceil() as i32, hint.size.y.ceil() as i32);
            assert!(asked.0 > tiny.w || asked.1 > tiny.h, "the premise: the text asks for more than the screen has");
            assert!(tiny.holds(window), "{window:?} is not on the small screen");
            assert!(
                window.w <= tiny.w - 2 && window.h <= tiny.h - 2,
                "{window:?} is cut to the screen, with its strip"
            );
            assert!(window.w < asked.0 || window.h < asked.1, "the window is smaller than the text asked: {window:?}");
        });
        output.textures_delta.clear();
    }

    /// The window has the size of its text in the very frame it is placed in, from the first hint to the change from
    /// a short to a long one: no size set by hand, no frame with the old size.
    #[test]
    fn the_window_has_the_size_of_its_text_at_once_when_the_text_changes() {
        let area = super::super::work_at(0, 0);
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme");
        let mut hint = HintWindow::new();
        let symbol = Rect::new(area.right() - 40, area.bottom() - 30, 26, 26);
        let pointer = Some(Pointer { hidden: false, x: area.right() - 27, y: area.bottom() - 17 });
        let run = |hint: &mut HintWindow, text: &str| {
            let mut output = ctx
                .run_ui(egui::RawInput::default(), |ui| hint.step(ui.ctx(), Some(text), pointer, Some(symbol), scheme));
            output.textures_delta.clear();
            hint.window().expect("placed in this frame")
        };
        let short = run(&mut hint, "x");
        let long = run(&mut hint, "Anti-AFK aus. Klicken zum Einschalten, drückt nach der Wartezeit eine Taste");
        let ppp = 1.0;
        assert!(long.w > short.w, "the long text has the wider window at once: {short:?} then {long:?}");
        assert!(long.w as f32 >= hint.size.x * ppp - 1.0, "the window is as wide as the text asked: {long:?}");
        for window in [short, long] {
            assert!(area.holds(window), "{window:?} on {area:?}");
            assert!(!window.overlaps(symbol), "{window:?} covers the symbol");
        }
        // And back to the short one: the window shrinks in the same frame.
        let again = run(&mut hint, "x");
        assert_eq!(again.w, short.w, "short again");
    }

    /// The window says its scale at once and the same again: settled at the second step.
    #[test]
    fn a_scale_reported_twice_settles_the_window() {
        let start = Scale { own: 1.0, confirmed: false, tries: 0 };
        let first = start.next(1.0, 2.0);
        assert_eq!(first, Scale { own: 2.0, confirmed: false, tries: 1 }, "another scale: taken over, still hidden");
        let second = first.next(2.0, 2.0);
        assert_eq!(second, Scale { own: 2.0, confirmed: true, tries: 0 }, "the same twice: settled");
        // A window that does not say its scale (nothing reported) is settled with the one it has.
        assert!(start.next(1.0, 0.0).confirmed);
    }

    /// A scale that keeps jumping (1, 2, 1, 2, ...) does not keep the window hidden: after at most MAX_REPLACES
    /// placements in a row it is shown with the last scale and goes on following without hiding again.
    #[test]
    fn a_scale_that_jumps_back_and_forth_is_shown_after_three_tries() {
        let mut scale = Scale { own: 1.0, confirmed: false, tries: 0 };
        let mut first_visible = None;
        for step in 1..=12 {
            let reported = if scale.own < 1.5 { 2.0 } else { 1.0 };
            scale = scale.next(scale.own, reported);
            if scale.confirmed && first_visible.is_none() {
                first_visible = Some(step);
            }
            if let Some(from) = first_visible {
                assert!(scale.confirmed, "step {step}: hidden again after it was shown at step {from}");
            }
        }
        let shown_at = first_visible.expect("shown after all");
        assert!(shown_at <= MAX_REPLACES, "shown only at the step {shown_at}");
    }

    /// A new hint starts with the scale its screen had last time, not with the one of the overlay.
    #[test]
    fn a_new_hint_starts_with_the_scale_its_screen_had() {
        let area = super::super::work_at(0, 0);
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme");
        let mut hint = HintWindow::new();
        hint.known.push((area, 2.0));
        let symbol = Rect::new(area.x + 100, area.y + 100, 26, 26);
        let pointer = Some(Pointer { hidden: false, x: area.x + 113, y: area.y + 113 });
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            hint.step(ui.ctx(), Some("Hinweis"), pointer, Some(symbol), scheme)
        });
        output.textures_delta.clear();
        // (The window of this test then reports the 1.0 of its context: the placement of the frame is what counts.)
        let window = hint.window().expect("placed");
        assert!(window.w as f32 >= hint.size.x * 2.0 - 1.0, "in pixels of that scale: {window:?}");
    }

    /// On a narrow screen the text wraps with the strip at the edges counted in, and the height is that of the text
    /// at the width the window really has: no line is lost.
    #[test]
    fn on_a_narrow_screen_the_height_fits_the_text_at_the_width_there_is() {
        let narrow = Rect::new(0, 0, 60, 600);
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut hint = HintWindow::new();
        let text = "Signatur anlernen: letztes Scan-Bild zeigen, Zahl tippen";
        let symbol = Rect::new(20, 300, 20, 20);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
            for ppp in [1.0, 1.5] {
                let window = hint.place(&ctx, text, ppp, (30, 310), symbol, narrow);
                assert!(window.w <= narrow.w - 2 * versekit_core::overlay::HINT_RESERVE, "{window:?} with the strip");
                let content = (window.w as f32 / ppp - FRAME.x).max(1.0);
                let needed = HintWindow::measure(&ctx, text, content);
                assert!(
                    window.h as f32 >= needed.y * ppp - 1.0,
                    "{window:?} is too low for the text at the width there is ({needed:?}), ppp {ppp}"
                );
            }
        });
        output.textures_delta.clear();
    }

    /// The window comes with a text and the pointer and goes without either.
    #[test]
    fn the_hint_shows_with_text_and_pointer_and_goes_without() {
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme");
        let mut hint = HintWindow::new();
        let pointer = Some(Pointer { hidden: false, x: 100, y: 100 });
        let run = |hint: &mut HintWindow, text: Option<&str>, pointer: Option<Pointer>| {
            let mut output =
                ctx.run_ui(egui::RawInput::default(), |ui| hint.step(ui.ctx(), text, pointer, None, scheme));
            output.textures_delta.clear();
        };
        run(&mut hint, None, pointer);
        assert!(!hint.shown(), "no text: no window");
        run(&mut hint, Some("Logs noch einmal lesen, falls ein Bauplan fehlt"), pointer);
        assert!(hint.shown(), "text and pointer: the window");
        run(&mut hint, Some("x"), None);
        assert!(!hint.shown(), "no pointer: gone");
    }
}
