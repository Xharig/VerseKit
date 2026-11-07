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

//! Lucide icons (ISC license), one set for all schemes, tinted at runtime.
//!
//! The templates are embedded from `tools/symbol-vorlagen/` at build time.
//! A new icon: copy its SVG from the full Lucide set into that folder.
//!
//! Lucide draws with `currentColor`, which the SVG renderer paints black, and
//! a black image cannot be tinted. The strokes are made white first; egui's
//! tint then multiplies white into exactly the scheme color.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use egui::{Image, vec2};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::color;

mod sources {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

/// Names of all embedded icons, sorted.
pub fn names() -> impl Iterator<Item = &'static str> {
    sources::SOURCES.iter().map(|(name, _)| *name)
}

/// The original SVG of an icon.
pub fn source(name: &str) -> Option<&'static str> {
    sources::SOURCES.iter().find(|(n, _)| *n == name).map(|(_, svg)| *svg)
}

/// The SVG with white strokes, ready for tinting.
pub fn white_svg(svg: &str) -> String {
    svg.replace("currentColor", "#ffffff")
}

fn prepared() -> &'static HashMap<&'static str, Arc<[u8]>> {
    static PREPARED: OnceLock<HashMap<&'static str, Arc<[u8]>>> = OnceLock::new();
    PREPARED.get_or_init(|| {
        sources::SOURCES.iter().map(|(name, svg)| (*name, Arc::from(white_svg(svg).into_bytes()))).collect()
    })
}

/// Registers the image loaders egui needs for SVG. Call once at startup.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

fn extra() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
    static EXTRA: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    EXTRA.get_or_init(Mutex::default)
}

/// Adds icons that a program brings along: `(name, SVG)` in the Lucide
/// style. A name the embedded set already has keeps the embedded icon.
pub fn register_extra(icons: &[(&str, &str)]) {
    let mut extra = extra().lock().unwrap_or_else(PoisonError::into_inner);
    for (name, svg) in icons {
        extra.entry((*name).to_string()).or_insert_with(|| Arc::from(white_svg(svg).into_bytes()));
    }
}

/// An icon in a color, `size` points square; `None` for an unknown name.
pub fn image(name: &str, tint: Color, size: f32) -> Option<Image<'static>> {
    let bytes = match prepared().get(name) {
        Some(bytes) => bytes.clone(),
        None => extra().lock().unwrap_or_else(PoisonError::into_inner).get(name)?.clone(),
    };
    Some(
        Image::from_bytes(format!("bytes://icons/{name}.svg"), bytes)
            .tint(color(tint))
            .fit_to_exact_size(vec2(size, size)),
    )
}

/// An icon in the scheme's color for a state (normal, highlighted, hovered).
pub fn themed(name: &str, scheme: &Scheme, tint: IconTint, size: f32) -> Option<Image<'static>> {
    image(name, scheme.icon(tint), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(svg: &str) -> egui::ColorImage {
        egui_extras::image::load_svg_bytes(svg.as_bytes(), &Default::default()).expect("svg renders")
    }

    #[test]
    fn all_templates_are_embedded_and_render() {
        let names: Vec<_> = names().collect();
        assert!(names.len() >= 90, "only {} icons", names.len());
        assert!(names.windows(2).all(|w| w[0] < w[1]));
        for name in &names {
            let image = render(&white_svg(source(name).unwrap_or_default()));
            assert_eq!(image.size, [24, 24], "{name}");
        }
        assert!(source("gibt-es-nicht").is_none());
        assert!(image("gibt-es-nicht", Color::hex("#ffffff"), 16.0).is_none());
    }

    #[test]
    fn strokes_are_white_so_tint_works() {
        let svg = source("activity").expect("activity exists");
        let strongest = |image: &egui::ColorImage| image.pixels.iter().copied().max_by_key(|p| p.a()).expect("pixels");
        let white = strongest(&render(&white_svg(svg)));
        assert!(white.a() > 200 && white.r() > 200 && white.g() > 200 && white.b() > 200, "{white:?}");
        // Counter-check: untouched, the strokes come out black.
        let black = strongest(&render(svg));
        assert!(black.a() > 200 && black.r() < 30, "{black:?}");
    }

    #[test]
    fn loads_through_egui() {
        let ctx = egui::Context::default();
        install(&ctx);
        let scheme = versekit_core::theme::by_id("krt").expect("krt exists");
        let mut shown = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(image) = themed("settings", scheme, IconTint::Highlight, 16.0) {
                shown = ui.add(image).rect.width() > 0.0;
            }
        });
        output.textures_delta.clear();
        assert!(shown);
    }
}
