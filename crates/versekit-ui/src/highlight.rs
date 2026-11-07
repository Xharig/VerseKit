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

//! The element the menu filter jumped to. The window publishes the found text and the time of the jump
//! in the egui context; every shared widget that shows a text compares it with its own texts and frames
//! itself in the accent while the highlight lasts, and scrolls itself into view once per jump.
//!
//! Only the first element of a pass whose text is the found one frames itself, so a text that stands twice
//! on a page lights one place.

use egui::{Context, Id, Rect, Stroke, StrokeKind, Ui};
use versekit_core::theme::Scheme;

use crate::{color, control_radius};

/// How long an element reached from the menu filter stays framed.
pub const HIGHLIGHT_SECONDS: f64 = 1.5;

/// How far the frame stands off the element.
const REACH: f32 = 4.0;

/// How much room around the element the scroll keeps free.
const SCROLL_ROOM: f32 = 8.0;

/// The found text and the time (seconds of the egui clock) of the jump.
#[derive(Clone, Debug, PartialEq)]
struct Pointer {
    text: String,
    since: f64,
}

/// What the widgets of the running pass have done about the pointer.
#[derive(Clone, Copy, Default)]
struct Progress {
    /// The pass in which an element framed itself.
    pass: u64,
    /// Whether an element framed itself in that pass.
    claimed: bool,
    /// The jump (bits of its time) the view was scrolled for.
    scrolled_for: Option<u64>,
}

fn pointer_id() -> Id {
    Id::unique("versekit-highlight-pointer")
}

fn progress_id() -> Id {
    Id::unique("versekit-highlight-progress")
}

/// Tells the widgets which text the last jump from the menu filter found, and when (`None`: nothing).
/// The window calls it every frame before it draws a page.
pub fn publish(ctx: &Context, found: Option<&(String, f64)>) {
    ctx.data_mut(|data| {
        if let Some((text, since)) = found {
            data.insert_temp(pointer_id(), Pointer { text: text.clone(), since: *since });
        } else {
            data.remove::<Pointer>(pointer_id());
        }
    });
}

/// Whether the highlight that began at `since` still shows.
fn shows(now: f64, since: f64) -> bool {
    (0.0..HIGHLIGHT_SECONDS).contains(&(now - since))
}

/// The pointer while its highlight lasts.
fn current(ui: &Ui) -> Option<Pointer> {
    let pointer = ui.ctx().data(|data| data.get_temp::<Pointer>(pointer_id()))?;
    shows(ui.input(|i| i.time), pointer.since).then_some(pointer)
}

/// Whether the found text is one of `texts` and the highlight still lasts.
pub fn pointed(ui: &Ui, texts: &[&str]) -> bool {
    current(ui).is_some_and(|pointer| texts.iter().any(|text| !text.is_empty() && *text == pointer.text))
}

/// Frames `rect` in the accent when the found text is one of `texts` and the highlight still lasts, and
/// scrolls it into view the first time. Returns whether it framed.
pub fn mark(ui: &Ui, scheme: &Scheme, rect: Rect, texts: &[&str]) -> bool {
    pointed(ui, texts) && frame(ui, scheme, rect)
}

/// Frames `rect` in the accent for the pointer that lasts (unless another element framed itself in this pass)
/// and scrolls it into view the first time. Returns whether it framed. Keeps the window repainting while the
/// frame stands.
pub fn frame(ui: &Ui, scheme: &Scheme, rect: Rect) -> bool {
    let ctx = ui.ctx();
    let Some(pointer) = current(ui) else {
        return false;
    };
    let pass = ctx.cumulative_pass_nr();
    let (taken, scroll) = ctx.data_mut(|data| {
        let progress = data.get_temp_mut_or_default::<Progress>(progress_id());
        let taken = progress.pass == pass && progress.claimed;
        progress.pass = pass;
        progress.claimed = true;
        let jump = pointer.since.to_bits();
        let scroll = !taken && progress.scrolled_for != Some(jump);
        if scroll {
            progress.scrolled_for = Some(jump);
        }
        (taken, scroll)
    });
    if taken {
        return false;
    }
    let stroke = Stroke::new(2.0, color(scheme.accent));
    ui.painter().rect_stroke(rect.expand(REACH), control_radius(scheme), stroke, StrokeKind::Outside);
    if scroll {
        ui.scroll_to_rect(rect.expand(SCROLL_ROOM), None);
    }
    ctx.request_repaint();
    true
}

#[cfg(test)]
pub(crate) mod probe {
    //! What a test needs to see the frames of a highlight in the shapes of a frame.

    use egui::epaint::{RectShape, Shape};
    use egui::{Color32, Rect, StrokeKind};
    use versekit_core::theme::Scheme;

    use crate::color;

    fn collect(shape: &Shape, scheme: &Scheme, clip: Rect, frames: &mut Vec<Frame>) {
        match shape {
            Shape::Rect(RectShape { rect, fill, stroke, stroke_kind: StrokeKind::Outside, blur_width, .. })
                if *fill == Color32::TRANSPARENT
                    && stroke.width == 2.0
                    && stroke.color == color(scheme.accent)
                    && *blur_width == 0.0 =>
            {
                frames.push(Frame { rect: *rect, clip });
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, scheme, clip, frames)),
            _ => {}
        }
    }

    /// An accent frame and the rectangle it is clipped to (the part of the window it can be seen in).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) struct Frame {
        pub(crate) rect: Rect,
        pub(crate) clip: Rect,
    }

    /// The accent frames of a highlight (two points wide, outside, unfilled) in `shapes`.
    pub(crate) fn frames(shapes: &[egui::epaint::ClippedShape], scheme: &Scheme) -> Vec<Frame> {
        let mut frames = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, scheme, clipped.clip_rect, &mut frames));
        frames
    }

    fn collect_texts(shape: &Shape, texts: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(text) => {
                texts.push((text.galley.text().to_string(), text.galley.rect.translate(text.pos.to_vec2())));
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect_texts(shape, texts)),
            _ => {}
        }
    }

    /// The texts drawn in `shapes` with the rectangle each one covers.
    pub(crate) fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect)> {
        let mut texts = Vec::new();
        shapes.iter().for_each(|clipped| collect_texts(&clipped.shape, &mut texts));
        texts
    }
}

#[cfg(test)]
mod tests {
    use egui::epaint::ClippedShape;
    use egui::{Pos2, ScrollArea, vec2};
    use versekit_core::theme;

    use super::*;
    use crate::widgets;

    fn context() -> Context {
        let ctx = Context::default();
        crate::install(&ctx);
        ctx
    }

    /// One frame at `time` (seconds) with `add` in a window; the shapes it drew.
    fn frame(ctx: &Context, time: f64, mut add: impl FnMut(&mut Ui)) -> Vec<ClippedShape> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
            time: Some(time),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| add(ui));
        output.textures_delta.clear();
        output.shapes
    }

    fn jump(ctx: &Context, text: &str, since: f64) {
        publish(ctx, Some(&(text.to_string(), since)));
    }

    #[test]
    fn an_element_frames_itself_for_its_own_text_while_the_highlight_lasts() {
        let scheme = theme::current();
        let ctx = context();
        let both = |ui: &mut Ui| {
            widgets::hint(ui, scheme, "Alpha");
            widgets::hint(ui, scheme, "Beta");
        };
        // Before any jump nothing stands.
        assert!(probe::frames(&frame(&ctx, 1.0, both), scheme).is_empty());
        jump(&ctx, "Alpha", 1.0);
        let shapes = frame(&ctx, 1.5, both);
        let frames = probe::frames(&shapes, scheme);
        assert_eq!(frames.len(), 1, "{frames:?}");
        let rect_of = |text: &str| probe::texts(&shapes).into_iter().find(|(shown, _)| shown == text).unwrap().1;
        assert!(frames[0].rect.contains_rect(rect_of("Alpha")), "the frame holds the element");
        assert!(!frames[0].rect.contains(rect_of("Beta").center()), "and nothing beside it");
        // Just before the end it still stands, at the end it is gone.
        assert_eq!(probe::frames(&frame(&ctx, 1.0 + HIGHLIGHT_SECONDS - 0.1, both), scheme).len(), 1);
        assert!(probe::frames(&frame(&ctx, 1.0 + HIGHLIGHT_SECONDS, both), scheme).is_empty());
        // The jump has not happened yet.
        jump(&ctx, "Alpha", 9.0);
        assert!(probe::frames(&frame(&ctx, 2.6, both), scheme).is_empty());
        // Nothing found: nothing stands.
        publish(&ctx, None);
        assert!(probe::frames(&frame(&ctx, 2.7, both), scheme).is_empty());
        // A text that is on no element frames nothing.
        jump(&ctx, "Gamma", 2.7);
        assert!(probe::frames(&frame(&ctx, 2.8, both), scheme).is_empty());
    }

    #[test]
    fn of_equal_texts_only_the_first_frames_itself() {
        let scheme = theme::current();
        let ctx = context();
        jump(&ctx, "Same", 1.0);
        let shapes = frame(&ctx, 1.2, |ui| {
            widgets::hint(ui, scheme, "Same");
            ui.add_space(30.0);
            widgets::hint(ui, scheme, "Same");
        });
        let frames = probe::frames(&shapes, scheme);
        assert_eq!(frames.len(), 1, "{frames:?}");
        let first = probe::texts(&shapes).into_iter().find(|(shown, _)| shown == "Same").unwrap().1;
        assert!(frames[0].rect.contains_rect(first), "it is the first one");
    }

    /// The offset of a scroll area with a tall space above the element `Alpha`, after `frames` frames from
    /// `from` on, one sixtieth of a second apart. `reset` puts the view back to the top in the first frame.
    fn scroll_offset(ctx: &Context, from: f64, frames: u32, reset: bool) -> f32 {
        let scheme = theme::current();
        let mut offset = 0.0;
        for i in 0..frames {
            let time = from + f64::from(i) / 60.0;
            frame(ctx, time, |ui| {
                let mut area = ScrollArea::vertical().id_salt("page").max_height(200.0);
                if reset && i == 0 {
                    area = area.vertical_scroll_offset(0.0);
                }
                let shown = area.show(ui, |ui| {
                    ui.add_space(1500.0);
                    widgets::hint(ui, scheme, "Alpha");
                    ui.add_space(300.0);
                });
                offset = shown.state.clamped_offset().y;
            });
        }
        offset
    }

    #[test]
    fn the_view_scrolls_to_the_element_once_per_jump() {
        let ctx = context();
        // Without a jump the view stays at the top.
        assert!(scroll_offset(&ctx, 1.0, 10, false) < 1.0);
        jump(&ctx, "Alpha", 2.0);
        // The element lies at 1500: the view goes there.
        assert!(scroll_offset(&ctx, 2.0, 40, false) > 1200.0, "it scrolled to the element");
        // The player scrolls back up while the frame still stands: the view stays where the player put it.
        assert!(scroll_offset(&ctx, 2.7, 20, true) < 1.0, "it does not scroll again for the same jump");
        // A new jump scrolls again.
        jump(&ctx, "Alpha", 10.0);
        assert!(scroll_offset(&ctx, 10.0, 40, false) > 1200.0, "a new jump scrolls again");
    }
}
