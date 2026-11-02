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

//! The curve of an axis (`curve_plot.py` of 4.12): what comes out at the back when the stick is deflected, with the
//! dead zone and the saturation as dim areas and the straight line as a comparison.
//!
//! Two views: the **quadrant** (0 to 1), which the game shows itself and in which something can be seen, and the
//! **whole** (−1 to 1), in which the curve runs through the zero point as a whole. The curve is point-symmetric, so
//! the quadrant is enough, and it makes every detail four times as large.
//!
//! ⚠ A curve is a picture. A screen reader gets the same thing as a sentence (dead zone, saturation, sensitivity).

use egui::{Align2, Context, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, vec2};
use versekit_core::axes;
use versekit_core::language::{Arg, t, tf};
use versekit_core::theme::Scheme;

use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How many points the line of the curve gets. Too few turn the bend at the dead zone into a soft round, and the
/// bend is the point.
pub const SUPPORT_POINTS: usize = 160;

/// The margins round the drawing, for the labels: left, top, right, bottom.
const MARGIN: (f32, f32, f32, f32) = (34.0, 10.0, 10.0, 24.0);

/// What the picture needs to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Values {
    /// `None` is "not set": no dead zone.
    pub deadzone: Option<f64>,
    /// `None` is "not set": the whole way.
    pub saturation: Option<f64>,
    /// `None` is the exponent 1.
    pub exponent: Option<f64>,
    /// The points of a curve of the game, if any; they win over the exponent.
    pub curve: Vec<(Option<f64>, Option<f64>)>,
}

impl Values {
    /// Without any setting: the straight line.
    pub fn straight() -> Values {
        Values { deadzone: None, saturation: None, exponent: None, curve: Vec::new() }
    }
}

/// The drawing area inside the picture, in points from its top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x0: f32,
    pub y0: f32,
    pub width: f32,
    pub height: f32,
}

/// The area of a picture of this size, with the margin for the labels (at least 10 points wide and high).
pub fn area(size: Vec2) -> Area {
    let (left, top, right, bottom) = MARGIN;
    Area { x0: left, y0: top, width: (size.x - left - right).max(10.0), height: (size.y - top - bottom).max(10.0) }
}

/// From values of the curve (−1 to 1 in the whole, 0 to 1 in the quadrant) to a point of the picture.
pub fn point(area: &Area, whole: bool, on: f64, off: f64) -> Pos2 {
    let (px, py) = if whole { ((on + 1.0) / 2.0, (off + 1.0) / 2.0) } else { (on, off) };
    Pos2::new(area.x0 + (px as f32) * area.width, area.y0 + (1.0 - py as f32) * area.height)
}

/// The dim areas where nothing happens, as `(from, to)` on the axis of the input: the dead zone (in the whole on
/// both sides) and the way beyond the saturation. A band that has no width is not there.
pub fn bands(deadzone: Option<f64>, saturation: Option<f64>, whole: bool) -> Vec<(f64, f64)> {
    let deadzone = deadzone.unwrap_or(0.0);
    let saturation = saturation.unwrap_or(1.0);
    let mut out = Vec::new();
    let mut band = |from: f64, to: f64| {
        if to > from {
            out.push((from, to));
        }
    };
    if deadzone > 0.0 {
        band(0.0, deadzone);
        if whole {
            band(-deadzone, 0.0);
        }
    }
    if saturation < 1.0 {
        band(saturation, 1.0);
        if whole {
            band(-1.0, -saturation);
        }
    }
    out
}

/// The line of the curve as points of the picture.
pub fn line(area: &Area, whole: bool, values: &Values) -> Vec<Pos2> {
    axes::progression(values.deadzone, values.saturation, values.exponent, &values.curve, SUPPORT_POINTS, whole)
        .into_iter()
        .map(|(on, off)| point(area, whole, on, off))
        .collect()
}

/// "Dead zone 0.099 · Saturation 0.85 · Sensitivity 1.5", the line above the large picture (6 significant digits
/// like `%g`; the sensitivity only where the picture has one).
pub fn values_line(values: &Values) -> String {
    let general = |value: Option<f64>| value.map_or_else(|| "—".to_string(), versekit_core::pyfloat::general);
    let line = format!(
        "{} {} · {} {}",
        t("s_kv_totzone"),
        general(values.deadzone),
        t("s_kv_saettigung"),
        general(values.saturation)
    );
    // The sensitivity is part of the curve: the big picture must say it as well.
    match values.exponent {
        Some(exponent) => format!("{line} · {} {}", t("s_ac_empf_kurz"), general(Some(exponent))),
        None => line,
    }
}

/// The sentence a screen reader gets for the picture.
pub fn description(values: &Values, whole: bool) -> String {
    tf(
        if whole { "s_ac_kurve_ganz" } else { "s_ac_kurve" },
        &[
            ("0", Arg::from(axes::two_decimals(values.deadzone).as_str())),
            ("1", Arg::from(axes::two_decimals(values.saturation).as_str())),
            ("2", Arg::from(axes::two_decimals(Some(values.exponent.unwrap_or(axes::DEFAULT_EXPONENT))).as_str())),
        ],
    )
}

/// Draws the picture in `size` points. `pointer` is the deflection measured right now, if one is (no page gives
/// one yet).
pub fn show(ui: &mut Ui, scheme: &Scheme, size: Vec2, whole: bool, values: &Values, pointer: Option<f64>) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let text = description(values, whole);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &text));
    let painter = ui.painter_at(rect);
    let radius = control_radius(scheme);
    painter.rect_filled(rect, radius, color(scheme.surface));
    painter.rect_stroke(rect, radius, Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);

    let a = area(rect.size());
    let origin = rect.min.to_vec2();
    let at = |p: Pos2| p + origin;
    let (left, top) = (rect.min.x + a.x0, rect.min.y + a.y0);
    let plot = Rect::from_min_size(Pos2::new(left, top), vec2(a.width, a.height));

    // 1. The areas where nothing happens, first: everything else lies over them.
    for (from, to) in bands(values.deadzone, values.saturation, whole) {
        let x_from = at(point(&a, whole, from, 0.0)).x;
        let x_to = at(point(&a, whole, to, 0.0)).x;
        painter.rect_filled(
            Rect::from_x_y_ranges(x_from..=x_to, plot.y_range()),
            0.0,
            color(scheme.sub).gamma_multiply(0.3),
        );
    }
    // 2. Quarter lines and frame (more would be noise, fewer give no hold).
    let grid = Stroke::new(1.0, color(scheme.sub).gamma_multiply(0.5));
    for share in [0.25_f32, 0.5, 0.75] {
        let x = plot.min.x + share * plot.width();
        let y = plot.min.y + share * plot.height();
        painter.line_segment([Pos2::new(x, plot.min.y), Pos2::new(x, plot.max.y)], grid);
        painter.line_segment([Pos2::new(plot.min.x, y), Pos2::new(plot.max.x, y)], grid);
    }
    painter.rect_stroke(plot, 0.0, grid, StrokeKind::Inside);
    if whole {
        // The zero lines stronger: in the whole they are what the curve is symmetric round.
        let strong = Stroke::new(1.0, color(scheme.sub));
        painter.line_segment([Pos2::new(plot.center().x, plot.min.y), Pos2::new(plot.center().x, plot.max.y)], strong);
        painter.line_segment([Pos2::new(plot.min.x, plot.center().y), Pos2::new(plot.max.x, plot.center().y)], strong);
    }
    // 3. The straight line as a comparison: without it nobody sees whether a curve is steep or flat.
    let (from, to) = if whole { (-1.0, -1.0) } else { (0.0, 0.0) };
    let ends = [at(point(&a, whole, from, to)), at(point(&a, whole, 1.0, 1.0))];
    painter.extend(Shape::dashed_line(&ends, Stroke::new(1.0, color(scheme.sub)), 3.0, 3.0));
    // 4. The curve.
    let points: Vec<Pos2> = line(&a, whole, values).into_iter().map(at).collect();
    if points.len() >= 2 {
        painter.add(Shape::line(points, Stroke::new(2.0, color(scheme.accent))));
    }
    // 5. The deflection measured right now.
    if let Some(deflection) = pointer {
        let off = axes::answer(deflection, values.deadzone, values.saturation, values.exponent, &values.curve);
        let (on, off) = if whole { (deflection, off) } else { (deflection.abs(), off.abs()) };
        let centre = at(point(&a, whole, on, off));
        painter.circle(centre, 4.0, color(scheme.gold), Stroke::new(1.0, color(scheme.bg)));
    }
    // 6. The labels: the ends of both axes and what is on which.
    let font = egui::TextStyle::Small.resolve(ui.style());
    let sub = color(scheme.sub);
    let low = if whole { "-1" } else { "0" };
    painter.text(Pos2::new(plot.min.x, plot.max.y + 12.0), Align2::LEFT_CENTER, low, font.clone(), sub);
    painter.text(Pos2::new(plot.max.x, plot.max.y + 12.0), Align2::RIGHT_CENTER, "1", font.clone(), sub);
    painter.text(Pos2::new(plot.min.x - 6.0, plot.max.y), Align2::RIGHT_CENTER, low, font.clone(), sub);
    painter.text(Pos2::new(plot.min.x - 6.0, plot.min.y), Align2::RIGHT_CENTER, "1", font.clone(), sub);
    painter.text(
        Pos2::new(plot.center().x, plot.max.y + 12.0),
        Align2::CENTER_CENTER,
        t("s_kv_achse_ein"),
        font.clone(),
        sub,
    );
    // The vertical one reads from the bottom to the top: turned a quarter against the clock round its start.
    let galley = painter.layout_no_wrap(t("s_kv_achse_aus"), font, sub);
    let centre = Pos2::new(plot.min.x - 20.0, plot.center().y);
    let start = Pos2::new(centre.x - galley.size().y / 2.0, centre.y + galley.size().x / 2.0);
    painter.add(egui::epaint::TextShape::new(start, galley, sub).with_angle(-std::f32::consts::FRAC_PI_2));
    response
}

/// The curve in a window of its own, for looking closely: on the page the picture is small, because the sliders
/// and the list stand beside it.
pub struct Large {
    /// What the window is called.
    pub title: String,
    pub values: Values,
    pub whole: bool,
}

/// How large the picture in the window is, in points.
pub const LARGE_SIDE: f32 = 560.0;

/// Draws the window; gives `false` when it is to close (Esc, a click beside it, the button).
pub fn show_large(ctx: &Context, scheme: &Scheme, large: &mut Large) -> bool {
    let side = LARGE_SIDE.min((ctx.content_rect().height() - 190.0).max(240.0)).min(ctx.content_rect().width() - 80.0);
    let side = side.max(240.0);
    let (close_by_button, close_beside) = {
        let mut button = false;
        let (_, beside) = widgets::dialog_wide(ctx, scheme, "axes-large", side + 24.0, |ui| {
            let heading = widgets::bold(ui, &large.title, egui::TextStyle::Body);
            ui.add(egui::Label::new(heading).wrap());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let label = t(if large.whole { "s_kv_quadrant" } else { "s_kv_ganz" });
                if ui.add(widgets::Button::new(&label, ButtonKind::Normal, scheme)).clicked() {
                    large.whole = !large.whole;
                }
                let line = values_line(&large.values);
                ui.add(egui::Label::new(egui::RichText::new(line).small().color(color(scheme.sub))).wrap());
            });
            ui.add_space(8.0);
            show(ui, scheme, vec2(side, side), large.whole, &large.values, None);
            ui.add_space(8.0);
            if ui.add(widgets::Button::new(&t("hf_schliessen"), ButtonKind::Normal, scheme)).clicked() {
                button = true;
            }
        });
        (button, beside)
    };
    !(close_by_button || close_beside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn the_area_keeps_the_margin_for_the_labels_and_a_minimum() {
        let a = area(vec2(240.0, 240.0));
        assert_eq!((a.x0, a.y0, a.width, a.height), (34.0, 10.0, 196.0, 206.0));
        let tiny = area(vec2(5.0, 5.0));
        assert_eq!((tiny.width, tiny.height), (10.0, 10.0));
    }

    #[test]
    fn values_land_on_the_picture_the_quadrant_from_corner_to_corner_and_the_whole_round_the_middle() {
        let a = area(vec2(240.0, 240.0));
        assert!(close(point(&a, false, 0.0, 0.0), Pos2::new(34.0, 216.0)), "bottom left");
        assert!(close(point(&a, false, 1.0, 1.0), Pos2::new(230.0, 10.0)), "top right");
        assert!(close(point(&a, true, -1.0, -1.0), Pos2::new(34.0, 216.0)));
        assert!(close(point(&a, true, 1.0, 1.0), Pos2::new(230.0, 10.0)));
        assert!(close(point(&a, true, 0.0, 0.0), Pos2::new(132.0, 113.0)), "the middle");
    }

    #[test]
    fn the_dim_areas_are_the_dead_zone_and_the_way_beyond_the_saturation() {
        assert!(bands(None, None, false).is_empty());
        assert!(bands(Some(0.0), Some(1.0), true).is_empty());
        assert_eq!(bands(Some(0.1), None, false), [(0.0, 0.1)]);
        assert_eq!(bands(Some(0.1), None, true), [(0.0, 0.1), (-0.1, 0.0)]);
        assert_eq!(bands(None, Some(0.8), false), [(0.8, 1.0)]);
        assert_eq!(bands(None, Some(0.8), true), [(0.8, 1.0), (-1.0, -0.8)]);
        assert_eq!(bands(Some(0.15), Some(0.85), false), [(0.0, 0.15), (0.85, 1.0)]);
        // A saturation of 1 or more has no way beyond it; a dead zone below nothing is none.
        assert!(bands(Some(-0.2), Some(1.3), true).is_empty());
    }

    #[test]
    fn the_line_has_the_points_of_the_drawing_and_bends_at_the_dead_zone() {
        let a = area(vec2(240.0, 240.0));
        let straight = line(&a, false, &Values::straight());
        assert_eq!(straight.len(), SUPPORT_POINTS + 1);
        assert!(close(straight[0], Pos2::new(34.0, 216.0)) && close(straight[SUPPORT_POINTS], Pos2::new(230.0, 10.0)));
        // With a dead zone the line lies on the floor first.
        let values = Values { deadzone: Some(0.5), ..Values::straight() };
        let bent = line(&a, false, &values);
        assert!(
            close(bent[SUPPORT_POINTS / 4], Pos2::new(34.0 + 196.0 * 0.25, 216.0)),
            "{:?}",
            bent[SUPPORT_POINTS / 4]
        );
        let whole = line(&a, true, &values);
        assert_eq!(whole.len(), SUPPORT_POINTS + 1);
        assert!(close(whole[SUPPORT_POINTS / 2], Pos2::new(132.0, 113.0)), "through the zero point");
    }

    #[test]
    fn the_line_above_the_large_picture_names_the_sensitivity_too() {
        let _guard = crate::language_lock();
        let values = Values { deadzone: Some(0.15), saturation: Some(0.85), exponent: Some(2.55), curve: Vec::new() };
        let line = values_line(&values);
        assert!(line.contains("2.55") && line.contains(&t("s_ac_empf_kurz")), "{line}");
        let other = Values { exponent: Some(1.5), ..values.clone() };
        assert_ne!(values_line(&other), line, "another sensitivity, another line");
        assert!(!values_line(&Values::straight()).contains(&t("s_ac_empf_kurz")), "no sensitivity: none named");
    }

    #[test]
    fn a_screen_reader_gets_the_three_numbers_as_a_sentence() {
        let values = Values { deadzone: Some(0.15), saturation: None, exponent: Some(1.5), curve: Vec::new() };
        let said = description(&values, false);
        assert!(said.contains("0.15") && said.contains("1.5"), "{said}");
        assert!(description(&values, true) != said, "the whole is told as the whole");
    }
}
