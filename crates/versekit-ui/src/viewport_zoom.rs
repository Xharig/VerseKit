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

//! A zoom of its own for one viewport of a context: the interface in that viewport is drawn `factor` times as
//! large as in the others, which keep the zoom of the context.
//!
//! egui knows one zoom per context: `pixels_per_point` is that zoom times the scale the system gives the window.
//! The plugin makes the scale of the viewport `factor` times larger before each of its passes, so it is drawn at
//! the larger size. The window system still counts in points of the context's zoom (the pointer positions and the
//! size of the window egui is given), so those are divided by `factor` in the same step.
//!
//! What the viewport asks of the window system (its size and place) stays in points of the context's zoom: a size
//! of `n` own points is asked for as `n * factor`.

use egui::{Context, Event, MouseWheelUnit, RawInput, ViewportId};

/// The zoom of each viewport that has one of its own, relative to the zoom of the context.
#[derive(Debug, Default)]
pub struct ViewportZoom {
    factors: Vec<(ViewportId, f32)>,
}

impl ViewportZoom {
    fn factor(&self, id: ViewportId) -> Option<f32> {
        self.factors.iter().find(|(v, _)| *v == id).map(|(_, f)| *f)
    }
}

impl egui::Plugin for ViewportZoom {
    fn debug_name(&self) -> &'static str {
        "versekit_viewport_zoom"
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut RawInput) {
        if let Some(factor) = self.factor(input.viewport_id) {
            scale_input(input, factor);
        }
    }
}

/// Gives the viewport `id` the zoom `factor` relative to the context; 1 takes it away again. Takes effect with the
/// next pass of that viewport.
pub fn set(ctx: &Context, id: ViewportId, factor: f32) {
    let factor = if factor.is_finite() && factor > 0.0 { factor } else { 1.0 };
    if factor_of(ctx, id) == factor {
        return;
    }
    let plugin = ctx.plugin_or_default::<ViewportZoom>();
    let mut zoom = plugin.lock();
    zoom.factors.retain(|(v, _)| *v != id);
    if factor != 1.0 {
        zoom.factors.push((id, factor));
    }
}

/// The zoom the viewport `id` has relative to the context; 1 without one of its own.
pub fn factor_of(ctx: &Context, id: ViewportId) -> f32 {
    ctx.plugin_opt::<ViewportZoom>().and_then(|p| p.lock().factor(id)).unwrap_or(1.0)
}

/// The input of one pass as the viewport sees it at `factor`: a larger scale, and the positions and sizes the
/// window system gave in points of the context's zoom turned into the smaller own points.
pub fn scale_input(input: &mut RawInput, factor: f32) {
    let shrink = |p: egui::Pos2| (p.to_vec2() / factor).to_pos2();
    let shrink_rect = |r: egui::Rect| egui::Rect::from_min_max(shrink(r.min), shrink(r.max));
    if let Some(info) = input.viewports.get_mut(&input.viewport_id) {
        info.native_pixels_per_point = Some(info.native_pixels_per_point.unwrap_or(1.0) * factor);
        info.inner_rect = info.inner_rect.map(shrink_rect);
        info.outer_rect = info.outer_rect.map(shrink_rect);
        info.monitor_size = info.monitor_size.map(|s| s / factor);
    }
    input.screen_rect = input.screen_rect.map(shrink_rect);
    for event in &mut input.events {
        match event {
            Event::PointerMoved(pos)
            | Event::PointerButton { pos, .. }
            | Event::Touch { pos, .. }
            | Event::MiddleClickPaste { pos, .. } => *pos = shrink(*pos),
            Event::MouseWheel { unit: MouseWheelUnit::Point, delta, .. } => *delta /= factor,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect, ViewportInfo, pos2, vec2};

    /// One pass of `id` on a window of 1000 x 100 pixels at the system scale 1, the pointer at (200, 50) pixels.
    fn pass(ctx: &Context, id: ViewportId) -> (f32, Rect, Option<Pos2>) {
        let info = ViewportInfo { native_pixels_per_point: Some(1.0), ..Default::default() };
        let input = RawInput {
            viewport_id: id,
            viewports: std::iter::once((id, info)).collect(),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 100.0))),
            events: vec![Event::PointerMoved(pos2(200.0, 50.0))],
            ..Default::default()
        };
        let mut seen = (0.0, Rect::NOTHING, None);
        let mut output = ctx.run_ui(input, |ui| {
            seen = (ui.ctx().pixels_per_point(), ui.ctx().content_rect(), ui.ctx().pointer_latest_pos());
        });
        output.textures_delta.clear();
        seen
    }

    /// The viewport with a zoom of its own is drawn twice as large and lays out in half the points, the pointer where
    /// it is on the window; the other viewport of the same context stays as it was, and 1 takes the zoom away.
    #[test]
    fn only_the_viewport_with_a_zoom_of_its_own_is_drawn_larger() {
        let ctx = Context::default();
        let bar = ViewportId::from_hash_of("bar");
        assert_eq!(pass(&ctx, bar).0, 1.0, "no zoom of its own yet");
        set(&ctx, bar, 2.0);
        let (ppp, rect, pointer) = pass(&ctx, bar);
        assert_eq!(ppp, 2.0, "drawn at twice the scale");
        assert_eq!(rect.size(), vec2(500.0, 50.0), "the same window in half the points");
        assert_eq!(pointer, Some(pos2(100.0, 25.0)), "the pointer on the same spot of the window");
        let (ppp, rect, _) = pass(&ctx, ViewportId::ROOT);
        assert_eq!((ppp, rect.size()), (1.0, vec2(1000.0, 100.0)), "the other viewport keeps the zoom of the context");
        set(&ctx, bar, 1.0);
        assert_eq!(pass(&ctx, bar).0, 1.0, "taken away again");
    }

    /// Only positions in points of the window change: a wheel in lines stays as it was.
    #[test]
    fn a_wheel_in_points_is_shrunk_and_one_in_lines_is_not() {
        let wheel = |unit| Event::MouseWheel {
            unit,
            delta: vec2(0.0, 30.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        };
        let mut input = RawInput { events: vec![wheel(MouseWheelUnit::Point), wheel(MouseWheelUnit::Line)], ..Default::default() };
        scale_input(&mut input, 1.5);
        let deltas: Vec<f32> = input
            .events
            .iter()
            .filter_map(|e| match e {
                Event::MouseWheel { delta, .. } => Some(delta.y),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, [20.0, 30.0]);
    }
}
