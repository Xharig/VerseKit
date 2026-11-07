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

//! Shows the sample page (`versekit_ui::sample`) in a window: every control in
//! every color scheme and size, switched live.
//!
//! `cargo run -p versekit-ui --example farbschemata`
//!
//! Nothing is saved; the settings stay untouched.

use versekit_core::font_size;
use versekit_ui::sample::SamplePage;

struct Preview(SamplePage);

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.show(ui);
    }
}

fn main() -> eframe::Result {
    let (width, height) = font_size::BASE_MIN_SIZE;
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("VerseKit Farbschemata")
            .with_inner_size([width, height.max(900.0)])
            .with_min_inner_size([width, height]),
        ..Default::default()
    };
    versekit_ui::renderer::apply(&mut options);
    eframe::run_native(
        "versekit-farbschemata",
        options,
        Box::new(|cc| {
            // The sample texts are German; without this the names from the
            // language file follow the system language and the page mixes both.
            versekit_core::language::set_language(versekit_core::language::Choice::De);
            versekit_ui::install(&cc.egui_ctx);
            Ok(Box::new(Preview(SamplePage::new())))
        }),
    )
}
