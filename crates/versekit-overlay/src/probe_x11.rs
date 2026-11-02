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

//! The self-check of the real window under X11 (`--probe <file>`), meant for a virtual screen (`xvfb-run`) or a
//! desktop session: it goes through the states by itself, reads back from the X server what the window holds,
//! writes one line per check and closes.
//!
//! Checked: there is an X11 window and it is mapped; it never takes the keyboard (`WM_HINTS.input` false,
//! `--gegenprobe fokus` leaves the hint as winit makes it) and neither the window manager nor the X server gave it
//! the keyboard; it is kept above the others (`_NET_WM_STATE_ABOVE`, only where a window manager runs); with
//! the lock closed its input shape is the lock alone (`--gegenprobe schloss` draws no lock to keep) and without
//! the lock it takes clicks everywhere; folded, its bounding shape is the bar and a
//! point beside the bar lies outside it (the X server hands no click there to the window; `--gegenprobe
//! ohne_region` sets no shape), open it is the open overlay; the window has 32 bits per pixel, the condition for
//! its opacity below 100 %.
//!
//! Not checked here: whether a compositor really lets the window shine through (the opacity is painted into the
//! pixels; a virtual screen has no compositor), the photos of the content and the frame-by-frame filming of
//! folding (the Windows self-check reads the pixels of the screen), the icon next to the clock and the run with
//! data. Under Wayland without X11 there is no window to read: the run says so and ends.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use std::fmt::Write as _;
use std::path::PathBuf;

use versekit_core::overlay::Rect;

/// The deliberate faults this self-check knows.
pub const COUNTERS: [&str; 3] = ["fokus", "ohne_region", "schloss"];

/// The box around `rects`; `None` without one.
pub fn bounds(rects: &[Rect]) -> Option<Rect> {
    let mut real = rects.iter().filter(|r| r.w > 0 && r.h > 0);
    let first = *real.next()?;
    Some(real.fold(first, |all, r| all.union(*r)))
}

/// Whether the point lies in one of `rects`.
pub fn holds(rects: &[Rect], x: i32, y: i32) -> bool {
    rects.iter().any(|r| r.contains(x, y))
}

/// Whether the shape `rects` (window pixels) of the window at `at` (screen pixels) is the part `part` (screen
/// pixels): its box is the part to the pixel.
pub fn shape_is(rects: &[Rect], at: (i32, i32), part: Rect) -> bool {
    bounds(rects).is_some_and(|b| b.relative_to(-at.0, -at.1) == part)
}

/// Folded: the middle of the open overlay `open` lies beside the bar `bar` and outside the shape `rects` of the
/// window at `at` (a click there reaches the window below), the middle of the bar inside it.
pub fn clicks_pass_beside(rects: &[Rect], at: (i32, i32), bar: Rect, open: Rect) -> bool {
    let beside = (open.x + open.w / 2, open.y + open.h / 2);
    let on_bar = (bar.x + bar.w / 2, bar.y + bar.h / 2);
    !bar.contains(beside.0, beside.1)
        && !holds(rects, beside.0 - at.0, beside.1 - at.1)
        && holds(rects, on_bar.0 - at.0, on_bar.1 - at.1)
}

/// Whether an input shape takes no click at all.
pub fn passes_every_click(input: &[Rect]) -> bool {
    bounds(input).is_none()
}

/// With the lock closed: the input shape (window pixels) of the window at `at` (screen pixels) is the lock `lock`
/// (screen pixels) and nothing else. The lock takes clicks, every other click passes.
pub fn only_the_lock_takes_clicks(input: &[Rect], at: (i32, i32), lock: Rect) -> bool {
    lock.w > 0 && lock.h > 0 && shape_is(input, at, lock)
}

/// Whether the window `window` is without the keyboard: neither the focus of the X server (`focus`) nor the
/// window the window manager names active (`active`).
pub fn keyboard_elsewhere(window: u32, focus: u32, active: Option<u32>) -> bool {
    focus != window && active != Some(window)
}

pub struct Probe {
    path: PathBuf,
    counter: Option<String>,
    guard_ok: bool,
    lines: Vec<String>,
    failed: usize,
    stage: usize,
    since: f64,
    #[cfg(all(unix, not(target_os = "macos")))]
    link: Option<system::Link>,
}

impl Probe {
    pub fn new(path: PathBuf, guard_ok: bool, counter: Option<String>) -> Probe {
        Probe {
            path,
            counter,
            guard_ok,
            lines: Vec::new(),
            failed: 0,
            stage: 0,
            since: 0.0,
            #[cfg(all(unix, not(target_os = "macos")))]
            link: system::Link::connect(),
        }
    }

    fn check(&mut self, name: &str, ok: bool) {
        if !ok {
            self.failed += 1;
        }
        self.lines.push(format!("{} {name}", if ok { "OK  " } else { "FEHL" }));
    }

    fn value(&mut self, name: &str, value: impl std::fmt::Display) {
        self.lines.push(format!("WERT {name} = {value}"));
    }

    /// The text of the result: one line per check and value, the verdict last.
    fn text(&mut self) -> String {
        // A misspelt counter check would run as a plain one and pass.
        if let Some(counter) = self.counter.clone() {
            self.value("gegenprobe", &counter);
            self.check("gegenprobe_bekannt", COUNTERS.contains(&counter.as_str()));
        }
        let mut text = String::new();
        for line in &self.lines {
            let _ = writeln!(text, "{line}");
        }
        let _ = writeln!(text, "ERGEBNIS {}", if self.failed == 0 { "gruen" } else { "rot" });
        text
    }

    fn finish(&mut self) {
        let text = self.text();
        let _ = std::fs::write(&self.path, text);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use system::step;

#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use eframe::egui;
    use egui::ViewportCommand;
    use versekit_core::overlay::{self, Mode, Rect, Row, Shown};
    use x11rb::connection::Connection;
    use x11rb::properties::WmHints;
    use x11rb::protocol::shape::{ConnectionExt as _, SK};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MapState, Window};
    use x11rb::rust_connection::RustConnection;

    use super::{
        Probe, bounds, clicks_pass_beside, keyboard_elsewhere, only_the_lock_takes_clicks, passes_every_click, shape_is,
    };
    use crate::app::OverlayApp;

    /// A connection of the self-check of its own: it only reads.
    pub struct Link {
        conn: RustConnection,
        root: Window,
    }

    impl Link {
        pub fn connect() -> Option<Link> {
            let (conn, number) = RustConnection::connect(None).ok()?;
            let root = conn.setup().roots.get(number)?.root;
            Some(Link { conn, root })
        }

        fn atom(&self, name: &str) -> Option<u32> {
            let atom = self.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom;
            (atom != x11rb::NONE).then_some(atom)
        }

        fn values(&self, window: Window, property: &str, kind: impl Into<u32>) -> Vec<u32> {
            let Some(property) = self.atom(property) else { return Vec::new() };
            let reply = self.conn.get_property(false, window, property, kind, 0, 64).ok().and_then(|c| c.reply().ok());
            reply.and_then(|r| r.value32().map(Iterator::collect)).unwrap_or_default()
        }

        fn mapped(&self, window: Window) -> Option<bool> {
            let attributes = self.conn.get_window_attributes(window).ok()?.reply().ok()?;
            Some(attributes.map_state == MapState::VIEWABLE)
        }

        fn depth(&self, window: Window) -> Option<u8> {
            Some(self.conn.get_geometry(window).ok()?.reply().ok()?.depth)
        }

        fn at(&self, window: Window) -> Option<(i32, i32)> {
            let at = self.conn.translate_coordinates(window, self.root, 0, 0).ok()?.reply().ok()?;
            Some((i32::from(at.dst_x), i32::from(at.dst_y)))
        }

        fn takes_keyboard(&self, window: Window) -> Option<bool> {
            let hints = WmHints::get(&self.conn, window).ok()?.reply().ok()?;
            Some(hints.and_then(|h| h.input) != Some(false))
        }

        fn keyboard_elsewhere(&self, window: Window) -> Option<bool> {
            let focus = self.conn.get_input_focus().ok()?.reply().ok()?.focus;
            let active = self.values(self.root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW).first().copied();
            Some(keyboard_elsewhere(window, focus, active))
        }

        fn above(&self, window: Window) -> bool {
            let above = self.atom("_NET_WM_STATE_ABOVE");
            self.values(window, "_NET_WM_STATE", AtomEnum::ATOM).iter().any(|a| Some(*a) == above)
        }

        /// The name of the window manager, `None` when none runs (`_NET_SUPPORTING_WM_CHECK`).
        fn window_manager(&self) -> Option<String> {
            let check = *self.values(self.root, "_NET_SUPPORTING_WM_CHECK", AtomEnum::WINDOW).first()?;
            let (name, utf8) = (self.atom("_NET_WM_NAME")?, self.atom("UTF8_STRING")?);
            let reply = self.conn.get_property(false, check, name, utf8, 0, 64).ok()?.reply().ok()?;
            Some(String::from_utf8_lossy(&reply.value).into_owned())
        }

        fn shape(&self, window: Window, kind: SK) -> Option<Vec<Rect>> {
            let reply = self.conn.shape_get_rectangles(window, kind).ok()?.reply().ok()?;
            Some(
                reply
                    .rectangles
                    .iter()
                    .map(|r| Rect::new(i32::from(r.x), i32::from(r.y), i32::from(r.width), i32::from(r.height)))
                    .collect(),
            )
        }
    }

    fn window(app: &OverlayApp) -> Option<Window> {
        u32::try_from(app.hwnd).ok().filter(|&w| w != 0)
    }

    /// The bounding shape of the window is `part` (screen pixels), checked as `name`.
    fn shape_check(probe: &mut Probe, window: Window, part: Rect, name: &str) -> Option<Vec<Rect>> {
        let link = probe.link.as_ref()?;
        let (rects, at) = (link.shape(window, SK::BOUNDING), link.at(window));
        let seen = rects.as_deref().and_then(bounds).zip(at).map(|(b, at)| b.relative_to(-at.0, -at.1));
        probe.value(&format!("{name}_sichtbar"), seen.map_or("?".to_string(), overlay::geometry_text));
        probe.check(name, rects.as_deref().zip(at).is_some_and(|(r, at)| shape_is(r, at, part)));
        rects
    }

    /// The keyboard is not with the window, checked as `name`.
    fn keyboard_check(probe: &mut Probe, window: Window, name: &str) {
        let elsewhere = probe.link.as_ref().and_then(|l| l.keyboard_elsewhere(window));
        probe.check(name, elsewhere == Some(true));
    }

    pub fn step(app: &mut OverlayApp, ctx: &egui::Context, now: f64) {
        let Some(mut probe) = app.probe.take() else { return };
        ctx.request_repaint();
        let waited = now - probe.since;
        let next = |probe: &mut Probe| {
            probe.stage += 1;
            probe.since = now;
        };
        match (probe.stage, window(app)) {
            (0, _) if waited < 1.0 => {}
            (0, None) => {
                probe.check("x11_fenster_da", false);
                probe.value("x11_fenster", "keines (Wayland ohne X11?): nichts zu messen");
                finish(&mut probe, ctx);
            }
            (_, None) => {}
            (0, Some(window)) => {
                app.vis.set_game(true);
                app.settings.popup_seconds = 2;
                // The pointer of the screen must not decide anything.
                app.settings.auto_through = false;
                let Some(link) = probe.link.as_ref() else {
                    probe.check("x11_verbindung", false);
                    finish(&mut probe, ctx);
                    app.probe = Some(probe);
                    return;
                };
                let (mapped, keyboard, manager) =
                    (link.mapped(window), link.takes_keyboard(window), link.window_manager());
                let (depth, above) = (link.depth(window), link.above(window));
                probe.check("x11_fenster_da", mapped.is_some());
                probe.check("fenster_sichtbar", mapped == Some(true));
                probe.check("schutz_eingehaengt", probe.guard_ok);
                probe.check("nimmt_keine_tastatur", keyboard == Some(false));
                keyboard_check(&mut probe, window, "tastatur_nicht_genommen_beim_start");
                probe.value("fenstermanager", manager.as_deref().unwrap_or("keiner"));
                match manager {
                    Some(_) => probe.check("immer_oben_beim_start", above),
                    None => probe.value("immer_oben_beim_start", "kein Fenstermanager, entfällt"),
                }
                probe.value("bits_je_punkt", depth.map_or("?".to_string(), |d| d.to_string()));
                probe.check("deckkraft_moeglich_32_bit", depth == Some(32));
                // Enough finds to fill the open overlay.
                for n in 4..=12 {
                    let row = Row::Found {
                        name: format!("Bauplan {n}"),
                        kind: "Kühler".into(),
                        code: "S1 · A".into(),
                        time: "21:00:00".into(),
                        caught_up: false,
                    };
                    app.rows.push(row, 20);
                }
                next(&mut probe);
            }
            (1, Some(window)) if waited >= 1.0 => {
                probe.check("offen_mit_spiel", app.vis.shown(now) == Shown::Open);
                probe.check("zeilen_gezeichnet", app.drawn.len() >= 3);
                shape_check(&mut probe, window, app.current, "bereich_offen_wie_das_overlay");
                app.settings.mode = Mode::Always;
                app.vis.set_mode(Mode::Always);
                app.settings.click_through = true;
                next(&mut probe);
            }
            (2, Some(window)) if waited >= 1.0 => {
                let input = probe.link.as_ref().and_then(|l| l.shape(window, SK::INPUT));
                let at = probe.link.as_ref().and_then(|l| l.at(window));
                let lock = app.lock_on_screen(ctx.pixels_per_point());
                probe.value("eingabeform_bei_durchklick", format!("{input:?}"));
                probe.value("schloss", lock.map_or("?".to_string(), overlay::geometry_text));
                let only_lock = match (input.as_deref(), at, lock) {
                    (Some(input), Some(at), Some(lock)) => only_the_lock_takes_clicks(input, at, lock),
                    _ => false,
                };
                probe.check("durchklick_an", app.passing && only_lock);
                let keyboard = probe.link.as_ref().and_then(|l| l.takes_keyboard(window));
                probe.check("nimmt_keine_tastatur_nach_durchklick_an", keyboard == Some(false));
                app.settings.click_through = false;
                next(&mut probe);
            }
            (3, Some(window)) if waited >= 1.0 => {
                let input = probe.link.as_ref().and_then(|l| l.shape(window, SK::INPUT));
                probe.check("durchklick_aus", !app.passing && input.as_deref().is_some_and(|i| !passes_every_click(i)));
                keyboard_check(&mut probe, window, "tastatur_nicht_genommen_nach_durchklick");
                app.vis.toggle(now, 2.0);
                next(&mut probe);
            }
            (4, Some(window)) if waited >= 1.0 => {
                let (bar, open) = (app.current, app.last_open);
                probe.value("eingeklappt", overlay::geometry_text(bar));
                let rects = shape_check(&mut probe, window, bar, "bereich_eingeklappt_wie_die_leiste");
                let at = probe.link.as_ref().and_then(|l| l.at(window));
                let passes = rects.zip(at).is_some_and(|(r, at)| clicks_pass_beside(&r, at, bar, open));
                probe.check("neben_der_leiste_klickt_durch", passes);
                app.vis.toggle(now, 2.0);
                next(&mut probe);
            }
            (5, Some(window)) if waited >= 1.0 => {
                shape_check(&mut probe, window, app.current, "bereich_offen_nach_dem_aufklappen");
                keyboard_check(&mut probe, window, "tastatur_nicht_genommen_am_ende");
                finish(&mut probe, ctx);
            }
            _ => {}
        }
        app.probe = Some(probe);
    }

    fn finish(probe: &mut Probe, ctx: &egui::Context) {
        probe.finish();
        probe.stage = usize::MAX;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_is_the_part_when_its_box_lies_on_it_to_the_pixel() {
        let rounded = [Rect::new(3, 0, 94, 1), Rect::new(0, 1, 100, 18), Rect::new(3, 19, 94, 1)];
        assert!(shape_is(&rounded, (500, 200), Rect::new(500, 200, 100, 20)));
        assert!(!shape_is(&rounded, (500, 200), Rect::new(500, 200, 100, 21)), "a pixel off");
        assert!(!shape_is(&rounded, (501, 200), Rect::new(500, 200, 100, 20)), "the window elsewhere");
        assert!(!shape_is(&[], (0, 0), Rect::new(0, 0, 1, 1)), "no shape, no part");
    }

    /// Folded with the bar on top: the middle of the open overlay is outside the shape of the bar, the bar's
    /// middle inside; without a shape (the whole window) the click beside does not pass.
    #[test]
    fn beside_the_bar_a_click_passes_only_when_the_shape_is_the_bar() {
        let open = Rect::new(100, 50, 400, 250);
        let bar = Rect::new(100, 50, 400, 30);
        let at = (100, 50);
        let bar_shape = [Rect::new(0, 0, 400, 30)];
        assert!(clicks_pass_beside(&bar_shape, at, bar, open));
        let whole = [Rect::new(0, 0, 400, 250)];
        assert!(!clicks_pass_beside(&whole, at, bar, open), "a window without a shape takes the click");
        assert!(!clicks_pass_beside(&[], at, bar, open), "nothing at all: not even the bar");
    }

    /// ⚠ An empty input shape passed this check before and locked the player out: the lock must take clicks.
    #[test]
    fn with_the_lock_closed_the_lock_alone_takes_clicks() {
        let lock = Rect::new(580, 54, 24, 24);
        let at = (500, 50);
        assert!(only_the_lock_takes_clicks(&[Rect::new(80, 4, 24, 24)], at, lock));
        assert!(!only_the_lock_takes_clicks(&[], at, lock), "nothing takes clicks: no way back");
        assert!(!only_the_lock_takes_clicks(&[Rect::new(0, 0, 400, 250)], at, lock), "the whole window catches them");
        assert!(!only_the_lock_takes_clicks(&[Rect::new(81, 4, 24, 24)], at, lock), "a pixel off");
        assert!(!only_the_lock_takes_clicks(&[], at, Rect::new(580, 54, 0, 0)), "no lock drawn");
    }

    #[test]
    fn clicks_pass_only_through_an_empty_input_shape() {
        assert!(passes_every_click(&[]));
        assert!(passes_every_click(&[Rect::new(0, 0, 0, 0)]));
        assert!(!passes_every_click(&[Rect::new(0, 0, 65535, 65535)]));
    }

    #[test]
    fn the_keyboard_counts_as_taken_by_the_focus_or_by_the_window_manager() {
        assert!(keyboard_elsewhere(7, 1, Some(9)));
        assert!(keyboard_elsewhere(7, 1, None));
        assert!(!keyboard_elsewhere(7, 7, None), "the focus of the X server");
        assert!(!keyboard_elsewhere(7, 1, Some(7)), "active for the window manager");
    }

    #[test]
    fn a_misspelt_counter_check_makes_the_run_red() {
        let mut probe = Probe::new(PathBuf::from("unbenutzt.txt"), true, Some("fokuss".into()));
        probe.check("etwas", true);
        let text = probe.text();
        assert!(text.contains("FEHL gegenprobe_bekannt") && text.ends_with("ERGEBNIS rot\n"), "{text}");
        let mut known = Probe::new(PathBuf::from("unbenutzt.txt"), true, Some("ohne_region".into()));
        assert!(known.text().ends_with("ERGEBNIS gruen\n"));
    }
}
