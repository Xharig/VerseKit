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
use versekit_core::overlay::Pointer;
use versekit_core::theme::Scheme;
use versekit_ui::color;

/// How far the window stands right of and below the pointer, in screen pixels.
const OFFSET: (i32, i32) = (14, 22);
/// How often the native window is looked for after it was made, before the search gives up.
const LOOKS: u32 = 20;
/// The widest the window gets before the text wraps, in points.
const MAX_WIDTH: f32 = 360.0;

pub struct HintWindow {
    id: egui::ViewportId,
    /// The size the text asked for in the last frame, in points.
    size: egui::Vec2,
    native: isize,
    looked: u32,
    shown: bool,
}

impl HintWindow {
    pub fn new() -> HintWindow {
        HintWindow {
            id: egui::ViewportId::from_hash_of("versekit-hinweis"),
            size: egui::vec2(120.0, 24.0),
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

    /// Shows `text` next to the pointer while both are there; otherwise the window goes (an immediate viewport
    /// that is not shown in a frame closes).
    pub fn step(&mut self, ctx: &egui::Context, text: Option<&str>, pointer: Option<Pointer>, scheme: &Scheme) {
        let (Some(text), Some(pointer)) = (text, pointer) else {
            if self.shown {
                self.shown = false;
                self.native = 0;
                self.looked = 0;
            }
            return;
        };
        let ppp = ctx.pixels_per_point().max(0.01);
        let at = egui::pos2((pointer.x + OFFSET.0) as f32 / ppp, (pointer.y + OFFSET.1) as f32 / ppp);
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
            .with_inner_size(self.size)
            .with_mouse_passthrough(true)
            .with_visible(self.may_show());
        let wanted = ctx.show_viewport_immediate(self.id, builder, |ui, _class| {
            egui::Frame::new()
                .fill(color(scheme.bar))
                .stroke(egui::Stroke::new(1.0, color(scheme.line)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_max_width(MAX_WIDTH);
                    ui.label(egui::RichText::new(text).color(color(scheme.fg)));
                })
                .response
                .rect
                .size()
        });
        self.shown = true;
        if (wanted - self.size).length() > 0.5 {
            self.size = wanted;
            ctx.request_repaint();
        }
        self.find_native();
        if !self.may_show() {
            ctx.request_repaint();
        }
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
        let title =
            versekit_core::language::window_title(&versekit_core::language::t("ov_hinweis_fenstertitel"));
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

    /// The window comes with a text and the pointer and goes without either.
    #[test]
    fn the_hint_shows_with_text_and_pointer_and_goes_without() {
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme");
        let mut hint = HintWindow::new();
        let pointer = Some(Pointer { hidden: false, x: 100, y: 100 });
        let run = |hint: &mut HintWindow, text: Option<&str>, pointer: Option<Pointer>| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| hint.step(ui.ctx(), text, pointer, scheme));
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
