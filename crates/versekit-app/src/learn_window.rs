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

//! The small window for teaching the digits of the signature scanner (`--anlernen`): the overlay opens it above
//! the game. It shows the picture kept for an unsure number (or the last one the scanner saw), asks for the right
//! number, teaches it and closes a moment after. Nothing opens it by itself.

use versekit_core::mining_data::{self, MiningData};
use versekit_core::paths::{Host, Paths};
use versekit_core::{font_size, language, theme, writeguard};
use versekit_ui::scanner_ui::{ScannerState, show_teaching};

/// How long the answer stays before the window closes, in seconds.
const CLOSE_AFTER: f64 = 3.0;
/// The first size of the window, in points.
const SIZE: [f32; 2] = [500.0, 420.0];

struct LearnWindow {
    state: ScannerState,
    mining: MiningData,
    /// When the number was taught (the time of the frame).
    taught_at: Option<f64>,
    /// The title bar of the system, dark or light with the scheme.
    title_bar: versekit_ui::window_look::TitleBar,
}

impl eframe::App for LearnWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.title_bar.follow(ui.ctx(), theme::current());
        self.show(ui);
    }
}

impl LearnWindow {
    fn show(&mut self, ui: &mut egui::Ui) {
        let scheme = theme::current();
        let now = ui.input(|i| i.time);
        egui::Frame::NONE.fill(versekit_ui::color(scheme.bg)).inner_margin(12.0).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            show_teaching(ui, scheme, &mut self.state, &self.mining);
        });
        if self.state.taught && self.taught_at.is_none() {
            self.taught_at = Some(now);
        }
        match self.taught_at {
            Some(at) if now - at >= CLOSE_AFTER => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            _ => ui.ctx().request_repaint_after(std::time::Duration::from_millis(250)),
        }
        // Escape closes it too.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// The window as it opens: small, above the others, with the class every window of the program carries.
pub(crate) fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(language::window_title(&language::t("scan_titel")))
        .with_app_id(versekit_core::paths::WM_CLASS)
        .with_inner_size(SIZE)
        .with_always_on_top()
}

/// Runs the window on the data given (`--daten <folder>`, written inside it only) or the player's own.
pub fn run(args: &[String]) -> eframe::Result {
    let root = args.iter().position(|a| a == "--daten").and_then(|at| args.get(at + 1)).cloned();
    let paths = match root.as_deref() {
        Some(root) => {
            writeguard::deny(true);
            writeguard::allow_under(Some(root));
            Paths::for_copy(Host::current(), root)
        }
        None => Some(Paths::new(Host::current())),
    };
    let Some(paths) = paths else { return Ok(()) };
    paths.share_saves();
    let data = paths.settings();
    theme::set_current(theme::choice_of(&data).id);
    let points = font_size::font_points(&data, &paths.host);
    language::set_language(language::choice_of(&data));
    let state = ScannerState::with_files(&paths);
    let mining = MiningData::load(&paths.app_file(mining_data::FILE));
    let options = crate::main_window::native_options(viewport());
    versekit_ui::renderer::run_native(
        "versekit-anlernen",
        options,
        versekit_ui::renderer::Look::Opaque,
        Box::new(move |cc| {
            versekit_ui::install(&cc.egui_ctx);
            versekit_ui::apply(&cc.egui_ctx, theme::current());
            versekit_ui::apply_size(&cc.egui_ctx, points);
            Ok(Box::new(LearnWindow { state, mining, taught_at: None, title_bar: Default::default() }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_small_stays_above_the_game_and_carries_the_class_of_the_program() {
        let viewport = viewport();
        assert_eq!(viewport.app_id.as_deref(), Some("Versekit"));
        assert_eq!(viewport.inner_size, Some(egui::vec2(SIZE[0], SIZE[1])));
        assert_eq!(viewport.window_level, Some(egui::WindowLevel::AlwaysOnTop));
    }

    #[test]
    fn the_window_closes_a_few_seconds_after_the_number_was_taught_and_not_before() {
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut window = LearnWindow {
            state: ScannerState::default(),
            mining: MiningData::default(),
            taught_at: None,
            title_bar: Default::default(),
        };
        let closed_at = |window: &mut LearnWindow, time: f64| -> bool {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(440.0, 400.0))),
                    time: Some(time),
                    ..Default::default()
                },
                |ui| window.show(ui),
            );
            out.textures_delta.clear();
            out.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)))
        };
        assert!(!closed_at(&mut window, 0.0));
        assert!(!closed_at(&mut window, 10.0), "nothing taught: the window waits for a picture");
        window.state.taught = true;
        assert!(!closed_at(&mut window, 11.0));
        assert!(!closed_at(&mut window, 13.0), "under three seconds after");
        assert!(closed_at(&mut window, 14.5));
    }
}
