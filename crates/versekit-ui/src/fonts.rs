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

//! Lato in every scheme (the typeface of the Profit Basetool), embedded.
//!
//! Source: github.com/google/fonts, `ofl/lato` (Lato 2.0), under the SIL Open
//! Font License 1.1 (`fonts/OFL.txt` ships with it). egui's own fonts stay as
//! fallbacks for characters Lato does not have (emoji, symbols).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const REGULAR: &str = "lato";
const BOLD_DATA: &str = "lato-bold";
/// Font family for headings and filled buttons.
pub const BOLD: &str = "lato-bold";

/// The license text of Lato, for the thanks and licenses page.
pub const LICENSE: &str = include_str!("../fonts/OFL.txt");

/// egui's font setup with Lato first.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(REGULAR.into(), Arc::new(FontData::from_static(include_bytes!("../fonts/Lato-Regular.ttf"))));
    fonts.font_data.insert(BOLD_DATA.into(), Arc::new(FontData::from_static(include_bytes!("../fonts/Lato-Bold.ttf"))));
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, REGULAR.into());
    let mut bold = vec![BOLD_DATA.to_string()];
    bold.extend(fallbacks);
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);
    fonts
}

/// Puts Lato into the context. Call once at startup.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

/// Lato Bold at `size` points.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lato_comes_first_with_fallbacks() {
        let fonts = definitions();
        let proportional = &fonts.families[&FontFamily::Proportional];
        assert_eq!(proportional[0], REGULAR);
        assert!(proportional.len() > 1, "egui fallbacks kept");
        assert_eq!(fonts.families[&FontFamily::Name(BOLD.into())][0], BOLD_DATA);
        assert!(LICENSE.contains("SIL Open Font License"));
    }

    #[test]
    fn text_is_laid_out_with_lato() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut width = 0.0;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            width = ui.label(egui::RichText::new("Überschrift ÄÖÜ ß").font(bold(18.0))).rect.width();
        });
        output.textures_delta.clear();
        assert!(width > 0.0);
    }
}
