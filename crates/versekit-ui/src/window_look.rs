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

//! What the system draws around a window of the program: the icon of the window (title bar, task bar, Alt+Tab;
//! without it eframe shows its own) and, on Windows, the title bar in the brightness of the colour scheme, port of
//! `titlebar.py`. A dark window with a white bar of the system on top would be the look of a light system design.

use std::sync::{Arc, OnceLock};

use egui::{IconData, SystemTheme, ViewportCommand};
use versekit_core::theme::Scheme;

/// The icon of the program, the picture `assets/icon.png`, decoded once.
pub fn program_icon() -> Arc<IconData> {
    static ICON: OnceLock<Arc<IconData>> = OnceLock::new();
    Arc::clone(ICON.get_or_init(|| {
        let picture = include_bytes!("../../../assets/icon.png");
        let icon = image::load_from_memory_with_format(picture, image::ImageFormat::Png).map(|decoded| {
            let rgba = decoded.to_rgba8();
            let (width, height) = rgba.dimensions();
            IconData { rgba: rgba.into_raw(), width, height }
        });
        Arc::new(icon.unwrap_or_default())
    }))
}

/// The icon of the program as a texture of `ctx`, made once per window (the head bar shows it before the name).
pub fn program_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::unique("versekit-programmsymbol");
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let icon = program_icon();
    let picture = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    let texture = ctx.load_texture("versekit-programmsymbol", picture, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// The title bar that fits the scheme: dark on a dark background, light on a light one.
pub fn title_theme(scheme: &Scheme) -> SystemTheme {
    let bg = scheme.bg;
    // The relative brightness of the background (Rec. 709 weights, enough to tell dark from light).
    let light = 0.2126 * f32::from(bg.r) + 0.7152 * f32::from(bg.g) + 0.0722 * f32::from(bg.b);
    if light > 127.5 { SystemTheme::Light } else { SystemTheme::Dark }
}

/// The title bar of one window: set when the window draws its first frame and again whenever the scheme turns it
/// from dark to light or back. egui hands it to the system (`DWMWA_USE_IMMERSIVE_DARK_MODE` on Windows) and has the
/// frame drawn anew, which the system does not do by itself for a frame on screen.
#[derive(Debug, Default)]
pub struct TitleBar {
    set: Option<SystemTheme>,
}

impl TitleBar {
    /// Once a frame: the bar follows `scheme`. Gives the theme when it was sent now.
    pub fn follow(&mut self, ctx: &egui::Context, scheme: &Scheme) -> Option<SystemTheme> {
        let wanted = title_theme(scheme);
        if self.set == Some(wanted) {
            return None;
        }
        self.set = Some(wanted);
        ctx.send_viewport_cmd(ViewportCommand::SetTheme(wanted));
        Some(wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_gets_the_icon_of_the_program_and_not_the_one_of_eframe() {
        let icon = program_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(icon.rgba.chunks(4).any(|pixel| pixel[3] > 0), "the picture is not empty");
        assert!(Arc::ptr_eq(&icon, &program_icon()), "decoded once");
    }

    #[test]
    fn the_title_bar_is_dark_in_every_scheme_of_the_program_and_light_only_on_a_light_background() {
        // Every scheme of the program has a dark background.
        for scheme in versekit_core::theme::schemes() {
            assert_eq!(title_theme(scheme), SystemTheme::Dark, "{}", scheme.id);
        }
        let mut light = versekit_core::theme::schemes()[0].clone();
        light.bg = versekit_core::theme::Color { r: 240, g: 240, b: 236 };
        assert_eq!(title_theme(&light), SystemTheme::Light);
    }

    #[test]
    fn the_title_bar_is_set_at_the_first_frame_and_again_only_when_the_scheme_turns() {
        let ctx = egui::Context::default();
        let dark = versekit_core::theme::schemes()[0].clone();
        let mut light = dark.clone();
        light.bg = versekit_core::theme::Color { r: 250, g: 250, b: 250 };
        let mut bar = TitleBar::default();
        let mut sent = |scheme: &Scheme| {
            let mut out = None;
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| out = bar.follow(ui.ctx(), scheme));
            output.textures_delta.clear();
            let commands = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone());
            (out, commands.unwrap_or_default())
        };
        let (first, commands) = sent(&dark);
        assert_eq!(first, Some(SystemTheme::Dark));
        assert!(commands.contains(&ViewportCommand::SetTheme(SystemTheme::Dark)), "{commands:?}");
        assert_eq!(sent(&dark), (None, Vec::new()), "nothing again while it fits");
        assert_eq!(sent(&light).0, Some(SystemTheme::Light));
        assert_eq!(sent(&dark).0, Some(SystemTheme::Dark));
    }
}
