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

//! Pictures of the route view of the selling page over the sample prices: every size of the text in a usual
//! window, the smallest and largest also in the smallest window, in German or English.
//!
//! `cargo run -p versekit-ui --example route_bild -- <folder> [en]`
//!
//! The window is for the pictures only and has to run on an own desktop (`tools/eigener_desktop.py`), never on
//! the screen of whoever works. Nothing is read from or written to the data of the program.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{ColorImage, vec2};
use versekit_core::font_size;
use versekit_core::language::{self, Choice};
use versekit_core::theme;
use versekit_ui::sample::SamplePage;

/// Frames a page gets to settle before it is photographed.
const SETTLE: u32 = 14;

/// One picture: the size of the text, the window, and the name of the file.
struct Plan {
    points: i64,
    window: egui::Vec2,
    name: String,
}

struct Shots {
    page: SamplePage,
    plan: Vec<Plan>,
    index: usize,
    frames: u32,
    asked: bool,
    folder: PathBuf,
    send: Sender<std::sync::Arc<ColorImage>>,
    receive: Receiver<std::sync::Arc<ColorImage>>,
}

impl eframe::App for Shots {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let Some(plan) = self.plan.get(self.index) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.page.size = plan.points;
        ctx.set_zoom_factor(font_size::zoom_factor(plan.points));
        self.page.show(ui);
        self.frames += 1;
        if self.frames == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(plan.window));
        }
        if self.frames >= SETTLE && !self.asked {
            self.asked = true;
            let send = self.send.clone();
            let wake = ctx.clone();
            ctx.request_screenshot(move |image| {
                let _ = send.send(image);
                wake.request_repaint();
            });
        }
        if let Ok(image) = self.receive.try_recv() {
            let path = self.folder.join(format!("{}.png", plan.name));
            if let Err(error) = image::save_buffer(
                &path,
                image.as_raw(),
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("{}: {error}", path.display());
            }
            self.index += 1;
            self.frames = 0;
            self.asked = false;
        }
        ctx.request_repaint();
    }
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let folder = PathBuf::from(args.next().expect("folder for the pictures"));
    let english = args.next().is_some_and(|word| word == "en");
    std::fs::create_dir_all(&folder).expect("folder");
    theme::set_current(theme::DEFAULT);
    let (low, high) = font_size::POINTS_RANGE;
    // Tall, so that the whole route stands in the picture without turning the wheel.
    let usual = vec2(1280.0, 1500.0);
    let mut plan: Vec<Plan> = (low..=high)
        .map(|points| Plan { points, window: usual, name: format!("route_{}_gross{points}", if english { "en" } else { "de" }) })
        .collect();
    for points in [low, high] {
        let (width, height) = font_size::min_window(points);
        plan.push(Plan {
            points,
            window: vec2(width, height),
            name: format!("route_{}_klein{points}", if english { "en" } else { "de" }),
        });
    }
    let mut page = SamplePage::preview();
    page.go_to("hf_verkauf");
    page.choose_first_entry();
    let (send, receive) = channel();
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("route_bild").with_inner_size(usual),
        ..Default::default()
    };
    versekit_ui::renderer::apply(&mut options);
    let choice = if english { Choice::En } else { Choice::De };
    eframe::run_native(
        "versekit-route-bild",
        options,
        Box::new(move |cc| {
            language::set_language(choice);
            versekit_ui::install(&cc.egui_ctx);
            Ok(Box::new(Shots { page, plan, index: 0, frames: 0, asked: false, folder, send, receive }))
        }),
    )
}
