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

//! Streamer mode in the user interface: one filter on everything that is drawn.
//!
//! [`install`] adds a plug-in to the egui context. Right before the output goes to the screen the
//! plug-in goes through every text that was laid out (labels, buttons, tooltips, notices, the text
//! in fields, the lines of the overlay) and through the names the screen reader gets, and lets
//! `versekit_core::streamer::cover` hide what is private. No page asks for it and none can forget
//! it: a text that is drawn at all passes here. While the mode is off the plug-in does nothing.

use std::sync::Arc;

use egui::epaint::Shape;
use egui::text::{ByteIndex, LayoutJob, LayoutSection};
use versekit_core::streamer;

/// The filter on the output of a context.
struct Filter;

impl egui::Plugin for Filter {
    fn debug_name(&self) -> &'static str {
        "streamer-filter"
    }

    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        if !streamer::active() {
            return;
        }
        for clipped in &mut output.shapes {
            cover_shape(ctx, &mut clipped.shape);
        }
        if let Some(update) = output.platform_output.accesskit_update.as_mut() {
            for (_, node) in &mut update.nodes {
                cover_node(node);
            }
        }
    }
}

/// Adds the filter to the context (once; a second call changes nothing).
pub fn install(ctx: &egui::Context) {
    ctx.add_plugin(Filter);
}

fn cover_shape(ctx: &egui::Context, shape: &mut Shape) {
    match shape {
        Shape::Vec(shapes) => {
            for inner in shapes {
                cover_shape(ctx, inner);
            }
        }
        Shape::Text(text) => {
            if let Some(job) = covered_job(&text.galley.job) {
                text.galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            }
        }
        _ => {}
    }
}

/// The job with everything private hidden; `None` when the text has nothing private.
fn covered_job(job: &Arc<LayoutJob>) -> Option<LayoutJob> {
    if !streamer::needs_cover(&job.text) {
        return None;
    }
    let mut covered = LayoutJob { text: String::new(), sections: Vec::new(), ..(**job).clone() };
    for section in &job.sections {
        let piece = &job.text[section.byte_range.start.0..section.byte_range.end.0];
        let start = covered.text.len();
        covered.text.push_str(&streamer::cover(piece));
        covered.sections.push(LayoutSection {
            leading_space: section.leading_space,
            byte_range: ByteIndex(start)..ByteIndex(covered.text.len()),
            format: section.format.clone(),
        });
    }
    // A value that stood across two sections is still there: then one section with the first look.
    if streamer::needs_cover(&covered.text) {
        let format = job.sections.first().map(|s| s.format.clone()).unwrap_or_default();
        let text = streamer::cover(&job.text);
        covered.sections =
            vec![LayoutSection { leading_space: 0.0, byte_range: ByteIndex(0)..ByteIndex(text.len()), format }];
        covered.text = text;
    }
    Some(covered)
}

/// The name, the value and the hints a screen reader gets.
fn cover_node(node: &mut egui::accesskit::Node) {
    macro_rules! property {
        ($get:ident, $set:ident) => {
            if let Some(text) = node.$get()
                && streamer::needs_cover(text)
            {
                let text = streamer::cover(text);
                node.$set(text);
            }
        };
    }
    property!(label, set_label);
    property!(description, set_description);
    property!(value, set_value);
    property!(placeholder, set_placeholder);
    property!(tooltip, set_tooltip);
}

#[cfg(test)]
mod tests {
    use egui::epaint::ClippedShape;

    use super::*;

    fn drawn(ctx: &egui::Context, add: impl FnMut(&mut egui::Ui)) -> Vec<String> {
        let mut output = ctx.run_ui(egui::RawInput::default(), add);
        output.textures_delta.clear();
        let mut texts = Vec::new();
        for ClippedShape { shape, .. } in &output.shapes {
            collect(shape, &mut texts);
        }
        texts
    }

    fn collect(shape: &Shape, texts: &mut Vec<String>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, texts)),
            Shape::Text(text) => texts.push(text.galley.text().to_string()),
            _ => {}
        }
    }

    #[test]
    fn the_filter_covers_what_is_drawn_and_only_while_the_mode_is_on() {
        let ctx = egui::Context::default();
        install(&ctx);
        streamer::learn("Waldmeister");
        streamer::set_thread_active(Some(false));
        let plain = drawn(&ctx, |ui| {
            ui.label("Als Waldmeister in C:\\Users\\Anna\\Doku");
        });
        assert!(plain.iter().any(|t| t.contains("Waldmeister")), "off: unchanged, {plain:?}");

        streamer::set_thread_active(Some(true));
        let hidden = drawn(&ctx, |ui| {
            ui.label("Als Waldmeister in C:\\Users\\Anna\\Doku");
        });
        streamer::set_thread_active(None);
        let all = hidden.join(" ");
        assert!(!all.contains("Waldmeister") && !all.contains("Anna"), "on: covered, {hidden:?}");
        assert!(all.contains("[Gesch") || all.contains("[Redacted]"), "{hidden:?}");
    }
}
