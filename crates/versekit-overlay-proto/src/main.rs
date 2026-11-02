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

//! Throwaway prototype for stress-testing the egui overlay.
//!
//! Usage:
//! - no switch: overlay (wgpu renderer)
//! - `--glow`: overlay with OpenGL instead of wgpu
//! - `--table`: window with 10,000 rows instead of the overlay
//! - `--probe <datei>`: runs through all toggles by itself, writes the
//!   result to the file and exits (for the invisible desktop)
//! - `--no-guard`: focus guard off (only for the negative control of the self-check)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod table;
#[cfg(windows)]
mod win;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;
use egui::{Color32, RichText, ViewportCommand, WindowLevel};

pub const GREEN: Color32 = Color32::from_rgb(0x9c, 0xe4, 0x30);
const RED: Color32 = Color32::from_rgb(0xff, 0x5a, 0x4a);
const GREY: Color32 = Color32::from_rgb(0x9a, 0xa4, 0x96);
const BACKGROUND: Color32 = Color32::from_rgb(0x10, 0x14, 0x10);
const OPACITY_STEPS: [u8; 4] = [255, 204, 153, 102];

struct Args {
    glow: bool,
    table: bool,
    guard: bool,
    probe: Option<PathBuf>,
}

fn args() -> Args {
    let mut out = Args { glow: false, table: false, guard: true, probe: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--glow" => out.glow = true,
            "--table" => out.table = true,
            "--no-guard" => out.guard = false,
            "--probe" => out.probe = it.next().map(PathBuf::from),
            _ => {}
        }
    }
    out
}

/// Collects check lines and measured values and writes them to a text file at the end.
pub struct Probe {
    path: PathBuf,
    lines: Vec<String>,
    failed: usize,
}

impl Probe {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lines: Vec::new(), failed: 0 }
    }

    pub fn check(&mut self, name: &str, ok: bool) {
        if !ok {
            self.failed += 1;
        }
        self.lines.push(format!("{} {name}", if ok { "OK  " } else { "FEHL" }));
    }

    pub fn value(&mut self, name: &str, value: String) {
        self.lines.push(format!("WERT {name} = {value}"));
    }

    pub fn finish(&mut self) {
        let mut text = String::new();
        for line in &self.lines {
            let _ = writeln!(text, "{line}");
        }
        let _ = writeln!(text, "ERGEBNIS {}", if self.failed == 0 { "gruen" } else { "rot" });
        let _ = std::fs::write(&self.path, text);
    }
}

// The window handling below is Windows-only (`cfg(windows)`); elsewhere the
// prototype only shows the window.
#[cfg_attr(not(windows), allow(dead_code))]
struct OverlayApp {
    hwnd: isize,
    renderer: &'static str,
    guard: bool,
    passthrough: bool,
    on_top: bool,
    opacity_step: usize,
    clicks: u32,
    hotkeys: Option<mpsc::Receiver<HotkeyEvent>>,
    hotkeys_ok: bool,
    /// Offset from mouse pointer to the window's top-left corner while dragging, in screen points.
    drag_grab: Option<(i32, i32)>,
    probe: Option<Probe>,
    frame: u64,
    frame_ms: f32,
    guard_installed: bool,
    last_topmost: Instant,
}

#[cfg(windows)]
type HotkeyEvent = win::Hotkey;
#[cfg(not(windows))]
type HotkeyEvent = ();

#[cfg_attr(not(windows), allow(dead_code))]
impl OverlayApp {
    fn set_passthrough(&mut self, ctx: &egui::Context, on: bool) {
        self.passthrough = on;
        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(on));
    }

    fn set_on_top(&mut self, ctx: &egui::Context, on: bool) {
        self.on_top = on;
        let level = if on { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
    }

    fn set_opacity_step(&mut self, step: usize) {
        self.opacity_step = step % OPACITY_STEPS.len();
        #[cfg(windows)]
        win::set_opacity(self.hwnd, OPACITY_STEPS[self.opacity_step]);
    }

    #[cfg(windows)]
    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.hotkeys.as_ref() else { return };
        let keys: Vec<_> = rx.try_iter().collect();
        for key in keys {
            match key {
                win::Hotkey::Passthrough => self.set_passthrough(ctx, !self.passthrough),
                win::Hotkey::Opacity => self.set_opacity_step(self.opacity_step + 1),
                win::Hotkey::OnTop => self.set_on_top(ctx, !self.on_top),
                win::Hotkey::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            }
        }
    }

    #[cfg(not(windows))]
    fn handle_hotkeys(&mut self, _ctx: &egui::Context) {}

    /// The probe run: toggle each setting once, then inspect the style bits.
    #[cfg(windows)]
    fn probe_step(&mut self, ctx: &egui::Context) {
        if self.probe.is_none() {
            return;
        }
        let hwnd = self.hwnd;
        let style = win::ex_style(hwnd);
        let alpha = win::layered_alpha(hwnd);
        let guard = self.guard;
        let renderer = self.renderer;
        let installed = self.guard_installed;
        let frame = self.frame;
        let check = |app: &mut Self, name: &str, ok: bool| app.probe.as_mut().unwrap().check(name, ok);
        match frame {
            10 => {
                let probe = self.probe.as_mut().unwrap();
                probe.value("renderer", renderer.to_string());
                probe.value("schutz", guard.to_string());
                check(self, "schutz_eingehaengt", installed);
                check(self, "noactivate_beim_start", win::has_noactivate(style));
                check(self, "immer_oben_beim_start", win::has_topmost(style));
            }
            // Click-through at full opacity: winit layers the window itself.
            12 => self.set_passthrough(ctx, true),
            20 => {
                check(self, "noactivate_nach_durchklick_an_100", win::has_noactivate(style));
                check(self, "durchklick_an_100", win::has_transparent(style));
                // A layered window without an opacity set would be invisible.
                check(self, "sichtbar_nach_durchklick_an_100", !win::has_layered(style) || alpha == Some(255));
            }
            22 => self.set_passthrough(ctx, false),
            30 => {
                check(self, "noactivate_nach_durchklick_aus_100", win::has_noactivate(style));
                check(self, "durchklick_aus_100", !win::has_transparent(style));
            }
            32 => self.set_opacity_step(2),
            40 => {
                check(self, "geschichtet_bei_60", win::has_layered(style));
                check(self, "deckkraft_153", alpha == Some(153));
            }
            42 => self.set_passthrough(ctx, true),
            50 => {
                check(self, "noactivate_nach_durchklick_an_60", win::has_noactivate(style));
                check(self, "durchklick_an_60", win::has_transparent(style));
                check(self, "deckkraft_153_nach_durchklick_an", alpha == Some(153));
            }
            // Change the opacity while click-through is on.
            52 => self.set_opacity_step(3),
            58 => check(self, "deckkraft_102_bei_durchklick", alpha == Some(102)),
            60 => self.set_passthrough(ctx, false),
            68 => {
                check(self, "noactivate_nach_durchklick_aus_40", win::has_noactivate(style));
                check(self, "durchklick_aus_40", !win::has_transparent(style));
                check(self, "geschichtet_nach_durchklick_aus_40", win::has_layered(style));
                check(self, "deckkraft_102_nach_durchklick_aus", alpha == Some(102));
            }
            70 => self.set_opacity_step(0),
            78 => check(self, "zurueck_auf_100", !win::has_layered(style) || alpha == Some(255)),
            80 => self.set_on_top(ctx, false),
            88 => {
                check(self, "noactivate_nach_immer_oben_aus", win::has_noactivate(style));
                check(self, "immer_oben_aus", !win::has_topmost(style));
            }
            90 => self.set_on_top(ctx, true),
            98 => {
                check(self, "noactivate_nach_immer_oben_an", win::has_noactivate(style));
                check(self, "immer_oben_an", win::has_topmost(style));
            }
            100 => {
                let answer = win::ask_mouse_activate(hwnd);
                let probe = self.probe.as_mut().unwrap();
                probe.value("antwort_auf_wm_mouseactivate", answer.to_string());
                probe.check("klick_aktiviert_nicht", win::is_ma_noactivate(answer));
            }
            101 => win::move_to(hwnd, 321, 234),
            102 => {
                check(self, "verschieben_ohne_aktivieren", win::window_pos(hwnd) == (321, 234));
                let repairs = win::style_repairs();
                self.probe.as_mut().unwrap().value("stil_reparaturen", repairs.to_string());
                check(self, "schutz_hat_repariert", repairs > 0);
                check(self, "nie_aktiviert", win::activations() == 0);
                self.probe.as_mut().unwrap().finish();
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            _ => {}
        }
        ctx.request_repaint();
    }

    #[cfg(not(windows))]
    fn probe_step(&mut self, _ctx: &egui::Context) {}

    fn status_row(ui: &mut egui::Ui, name: &str, value: RichText) {
        ui.label(RichText::new(name).color(GREY));
        ui.label(value);
        ui.end_row();
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BACKGROUND.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        self.frame += 1;
        let ctx = ui.ctx().clone();
        // Dragging over labels should move the window, not select text.
        ui.style_mut().interaction.selectable_labels = false;
        self.handle_hotkeys(&ctx);
        self.probe_step(&ctx);
        #[cfg(windows)]
        if self.on_top && self.probe.is_none() {
            if self.last_topmost.elapsed().as_secs_f32() >= 1.0 {
                win::reassert_topmost(self.hwnd);
                self.last_topmost = Instant::now();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(false);
        #[cfg(windows)]
        let (activations, repairs) = (win::activations(), win::style_repairs());
        #[cfg(not(windows))]
        let (activations, repairs) = (0, 0);

        egui::Frame::NONE
            .fill(BACKGROUND)
            .stroke(egui::Stroke::new(1.0, GREEN))
            .corner_radius(8.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                #[cfg_attr(not(windows), allow(unused_variables))]
                let drag = ui.add(
                    egui::Label::new(RichText::new("Verse-Kit Overlay-Test").strong().color(GREEN))
                        .sense(egui::Sense::drag()),
                );
                // Custom moving instead of StartDrag: the Windows route via the
                // title bar brings the window to the foreground. The position is
                // computed in screen points directly from the mouse position so
                // the window does not lag behind the pointer.
                #[cfg(windows)]
                {
                    if drag.drag_started() {
                        let (cx, cy) = win::cursor_pos();
                        let (wx, wy) = win::window_pos(self.hwnd);
                        self.drag_grab = Some((cx - wx, cy - wy));
                    }
                    if let Some((gx, gy)) = self.drag_grab {
                        if ctx.input(|i| i.pointer.primary_down()) {
                            let (cx, cy) = win::cursor_pos();
                            win::move_to(self.hwnd, cx - gx, cy - gy);
                            ctx.request_repaint();
                        } else {
                            self.drag_grab = None;
                        }
                    }
                }
                ui.label(RichText::new("Zum Verschieben an der Überschrift ziehen").small().color(GREY));
                ui.add_space(6.0);

                let on_off = |on: bool| if on { "an" } else { "aus" };
                egui::Grid::new("status").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    Self::status_row(ui, "Renderer", RichText::new(self.renderer));
                    Self::status_row(ui, "Durchklicken (Strg+Alt+D)", RichText::new(on_off(self.passthrough)));
                    let percent = OPACITY_STEPS[self.opacity_step] as u32 * 100 / 255;
                    Self::status_row(ui, "Deckkraft (Strg+Alt+O)", RichText::new(format!("{percent} %")));
                    Self::status_row(ui, "Immer oben (Strg+Alt+T)", RichText::new(on_off(self.on_top)));
                    let focus_text = if focused {
                        RichText::new("JA – Schutz versagt").color(RED).strong()
                    } else {
                        RichText::new("nein (richtig)").color(GREEN)
                    };
                    Self::status_row(ui, "Overlay hat den Fokus", focus_text);
                    let act_color = if activations == 0 { GREEN } else { RED };
                    Self::status_row(ui, "Aktivierungen", RichText::new(activations.to_string()).color(act_color));
                    Self::status_row(ui, "Stil repariert", RichText::new(repairs.to_string()));
                    Self::status_row(ui, "Aufbau je Bild", RichText::new(format!("{:.2} ms", self.frame_ms)));
                });

                if !self.guard_installed {
                    ui.colored_label(RED, "Fokusschutz konnte nicht eingehängt werden");
                }
                if !self.hotkeys_ok && self.probe.is_none() {
                    ui.colored_label(RED, "Tastenkürzel konnten nicht angemeldet werden");
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("Klick-Test: {}", self.clicks)).clicked() {
                        self.clicks += 1;
                    }
                    if ui.button("Beenden").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Beenden auch mit Strg+Alt+X").small().color(GREY));
            });

        self.frame_ms = started.elapsed().as_secs_f32() * 1000.0;
    }
}

fn hwnd_of(window: &impl raw_window_handle::HasWindowHandle) -> isize {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

fn run_overlay(args: Args) -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Verse-Kit Overlay-Test")
        .with_app_id("versekit-overlay-proto")
        .with_inner_size([400.0, 330.0])
        .with_position([80.0, 80.0])
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top()
        .with_active(false)
        .with_taskbar(false);
    let renderer = if args.glow { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    let options = eframe::NativeOptions { viewport, renderer, ..Default::default() };
    let renderer_name = if args.glow { "glow (OpenGL)" } else { "wgpu" };

    eframe::run_native(
        "versekit-overlay-proto",
        options,
        Box::new(move |cc| {
            let hwnd = hwnd_of(cc);
            #[cfg_attr(not(windows), allow(unused_mut))]
            let mut app = OverlayApp {
                hwnd,
                renderer: renderer_name,
                guard: args.guard,
                passthrough: false,
                on_top: true,
                opacity_step: 0,
                clicks: 0,
                hotkeys: None,
                hotkeys_ok: false,
                drag_grab: None,
                probe: args.probe.map(Probe::new),
                frame: 0,
                frame_ms: 0.0,
                guard_installed: !args.guard,
                last_topmost: Instant::now(),
            };
            #[cfg(windows)]
            {
                app.guard_installed = hwnd != 0 && win::install(hwnd, args.guard);
                if app.probe.is_none() {
                    let (tx, rx) = mpsc::channel();
                    let ctx = cc.egui_ctx.clone();
                    app.hotkeys_ok = win::start_hotkeys(tx, move || ctx.request_repaint());
                    app.hotkeys = Some(rx);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

fn run_table(args: Args) -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Verse-Kit Tabellen-Test")
        .with_app_id("versekit-table-proto")
        .with_inner_size([900.0, 640.0])
        .with_active(args.probe.is_none());
    let renderer = if args.glow { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    let options = eframe::NativeOptions { viewport, renderer, ..Default::default() };
    eframe::run_native(
        "versekit-table-proto",
        options,
        Box::new(move |_cc| Ok(Box::new(table::TableApp::new(args.probe)))),
    )
}

fn main() -> eframe::Result {
    let args = args();
    if args.table { run_table(args) } else { run_overlay(args) }
}
