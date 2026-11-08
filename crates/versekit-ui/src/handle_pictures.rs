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

//! The pictures of the card of the handle search as textures. A thread of its own fetches and decodes them one
//! after the other (`versekit_core::handle_query::pictures` keeps them on disk); the frame only takes what came.
//! Until a picture is there, or when there is none, [`Pictures::show`] draws a quiet box of the same size.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use egui::{Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2};
use versekit_core::handle_query::pictures;
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::{box_radius, color};

/// The longest side of a picture kept as a texture, in pixels.
pub const SIDE_MAX: u32 = 160;
/// The most textures held; past that the ones not waiting are let go and asked again when shown.
const SLOTS_MAX: usize = 96;

/// Where a picture stands.
enum Slot {
    Waiting,
    Ready(TextureHandle),
    Missing,
}

/// The pictures asked for and the thread that brings them.
pub struct Pictures {
    paths: Option<Paths>,
    net: Option<Arc<dyn Net>>,
    slots: HashMap<String, Slot>,
    asks: Option<Sender<String>>,
    answers: Option<Receiver<(String, Option<egui::ColorImage>)>>,
}

/// Decodes a PNG, JPEG or WebP picture and makes it at most [`SIDE_MAX`] pixels on its longest side.
pub fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode().ok()?;
    let picture = if picture.width().max(picture.height()) > SIDE_MAX {
        picture.thumbnail(SIDE_MAX, SIDE_MAX)
    } else {
        picture
    };
    let rgba = picture.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

impl Pictures {
    /// Pictures of the data folder of `paths`, fetched through `net`; without a net none are asked.
    pub fn new(paths: Option<&Paths>, net: Option<Arc<dyn Net>>) -> Pictures {
        Pictures {
            paths: paths.map(|p| Paths::new(p.host.clone())),
            net,
            slots: HashMap::new(),
            asks: None,
            answers: None,
        }
    }

    /// No pictures: the preview and the self-check.
    pub fn none() -> Pictures {
        Pictures::new(None, None)
    }

    /// Whether a picture is still on its way.
    pub fn waiting(&self) -> bool {
        self.slots.values().any(|s| matches!(s, Slot::Waiting))
    }

    /// Takes the pictures the thread brought.
    fn take(&mut self, ctx: &egui::Context) {
        let Some(answers) = &self.answers else { return };
        while let Ok((url, image)) = answers.try_recv() {
            let slot = match image {
                Some(image) => Slot::Ready(ctx.load_texture(format!("handle-bild {url}"), image, Default::default())),
                None => Slot::Missing,
            };
            self.slots.insert(url, slot);
        }
    }

    /// Starts the thread on the first picture asked.
    fn sender(&mut self, ctx: &egui::Context) -> Option<&Sender<String>> {
        if self.asks.is_none() {
            let (Some(paths), Some(net)) = (&self.paths, &self.net) else { return None };
            let (ask, asked) = std::sync::mpsc::channel::<String>();
            let (answer, answers) = std::sync::mpsc::channel();
            let own = Paths::new(paths.host.clone());
            let net = Arc::clone(net);
            let lock = versekit_core::writeguard::state();
            let wake = ctx.clone();
            let started = std::thread::Builder::new().name("handle-bilder".into()).spawn_with_language(move || {
                versekit_core::writeguard::adopt(&lock);
                for url in asked {
                    let image = pictures::get(&own, net.as_ref(), &url).and_then(|bytes| decode(&bytes));
                    if answer.send((url, image)).is_err() {
                        break;
                    }
                    wake.request_repaint();
                }
            });
            if started.is_err() {
                return None;
            }
            self.asks = Some(ask);
            self.answers = Some(answers);
        }
        self.asks.as_ref()
    }

    /// The texture of `url` once it is there; the first call asks for it.
    pub fn texture(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        self.take(ctx);
        if url.is_empty() {
            return None;
        }
        match self.slots.get(url) {
            Some(Slot::Ready(texture)) => return Some(texture.clone()),
            Some(_) => return None,
            None => {}
        }
        if self.slots.len() >= SLOTS_MAX {
            self.slots.retain(|_, s| matches!(s, Slot::Waiting));
        }
        let asked = self.sender(ctx).is_some_and(|s| s.send(url.to_string()).is_ok());
        self.slots.insert(url.to_string(), if asked { Slot::Waiting } else { Slot::Missing });
        None
    }

    /// The picture of `url` in a box `size` points large, or the quiet box while there is none. The place is the
    /// same either way.
    pub fn show(&mut self, ui: &mut Ui, scheme: &Scheme, url: &str, size: f32) -> egui::Response {
        let texture = self.texture(ui.ctx(), url);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        let radius = box_radius(scheme);
        match texture {
            Some(texture) => {
                let fit = fit(texture.size_vec2(), size);
                egui::Image::from_texture(egui::load::SizedTexture::new(texture.id(), fit))
                    .corner_radius(radius)
                    .paint_at(ui, Rect::from_center_size(rect.center(), fit));
            }
            None => {
                ui.painter().rect(rect, radius, color(scheme.field), Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
            }
        }
        response
    }
}

/// The size of a picture `size` fitted into a square `side` points large, in its own shape.
fn fit(size: Vec2, side: f32) -> Vec2 {
    let longest = size.x.max(size.y).max(1.0);
    size * (side / longest)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG of `w` x `h` red pixels.
    pub(crate) fn png(w: u32, h: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut out, image::ImageFormat::Png).expect("png");
        out.into_inner()
    }

    #[test]
    fn a_picture_is_decoded_and_made_small_and_noise_is_none() {
        let small = decode(&png(40, 20)).expect("decoded");
        assert_eq!(small.size, [40, 20]);
        let large = decode(&png(640, 320)).expect("decoded");
        assert_eq!(large.size, [SIDE_MAX as usize, SIDE_MAX as usize / 2]);
        assert!(decode(b"<html>kein Bild</html>").is_none());
        assert_eq!(fit(egui::vec2(200.0, 100.0), 36.0), egui::vec2(36.0, 18.0));
    }
}
