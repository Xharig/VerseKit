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

//! The previews of the setup assistant: what a setting does, drawn live from the code that draws the real
//! thing. No screenshot of the game is used anywhere: the game stands there as a plain dark placeholder
//! with the words "Dein Spiel", and the overlay is the real one (`overlay::show`) with made-up finds.
//!
//! A preview only shows. Its widgets answer no click: a button of the bar in a picture must not look
//! pressable, and a drag on it must not move anything.

use egui::{Align2, Color32, FontId, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use versekit_core::language::{Phrase, t};
use versekit_core::overlay::{BarSide, Message, Row, Rows, Shown};
use versekit_core::theme::Scheme;

use crate::{box_radius, color, overlay};

/// The height of the frame of a preview.
pub const FRAME_HEIGHT: f32 = 214.0;

/// The height of the overlay in the picture; below it stands the strip with the two words.
const OVERLAY_HEIGHT: f32 = 166.0;

/// The margin of what stands in the frame.
const MARGIN: f32 = 12.0;

/// The widest a frame gets: wider shows only more empty game.
const FRAME_MAX_WIDTH: f32 = 600.0;

/// The most rows the picture of the overlay shows.
const ROWS_SHOWN: usize = 2;

/// The look of the placeholder for the game: deep blue, a few dim stars. The same in every scheme, because the
/// game does not change with the colour scheme of the program.
const SKY_TOP: Color32 = Color32::from_rgb(0x0a, 0x10, 0x1c);
const SKY_BOTTOM: Color32 = Color32::from_rgb(0x1a, 0x28, 0x3c);
const STAR: Color32 = Color32::from_rgb(0x8a, 0x9a, 0xb4);
/// The words in the frame: at least 4.5 : 1 on the lower sky (a test checks it).
const GAME_WORD: Color32 = Color32::from_rgb(0x87, 0x95, 0xad);

/// Draws the frame of a preview (the game as a plain placeholder) and gives `content` the area inside it.
/// The frame has an accessible name, `name`, and swallows every click.
pub fn frame(ui: &mut Ui, scheme: &Scheme, name: &str, height: f32, content: impl FnOnce(&mut Ui, Rect)) {
    let width = ui.available_width().min(FRAME_MAX_WIDTH);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let radius = box_radius(scheme);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius, SKY_TOP);
    // A second, lighter band at the bottom makes the plain colour read as a place.
    let band = Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max);
    painter.rect_filled(band, egui::CornerRadius { nw: 0, ne: 0, sw: radius.sw, se: radius.se }, SKY_BOTTOM);
    for (x, y, size) in stars() {
        let at = pos2(rect.min.x + x * rect.width(), rect.min.y + y * rect.height());
        painter.circle_filled(at, size, STAR);
    }
    // Below the overlay: what the picture is (left) and what the dark ground is (right).
    let font = FontId::proportional(14.0);
    painter.text(
        pos2(rect.min.x + MARGIN, rect.max.y - 8.0),
        Align2::LEFT_BOTTOM,
        t("as_vorschau_marke"),
        font.clone(),
        GAME_WORD,
    );
    painter.text(
        pos2(rect.max.x - MARGIN, rect.max.y - 8.0),
        Align2::RIGHT_BOTTOM,
        t("as_vorschau_spiel"),
        font,
        GAME_WORD,
    );
    painter.rect_stroke(rect, radius, Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    inner.set_clip_rect(rect);
    content(&mut inner, rect);
    // On top of the picture: it answers no click and no drag, is not a stop of the Tab key, and says what it is.
    let cover = ui.interact(rect, response.id.with("cover"), Sense::CLICK | Sense::DRAG);
    cover.widget_info(|| WidgetInfo::labeled(Role::Image, false, name));
}

/// Stars at fixed places (a share of width and height, and a radius): the picture is the same every frame.
fn stars() -> impl Iterator<Item = (f32, f32, f32)> {
    (0..28_u32).map(|i| {
        let h = i.wrapping_mul(2_654_435_761).rotate_left(7) ^ i.wrapping_mul(40_503);
        let x = (h % 997) as f32 / 997.0;
        let y = ((h / 997) % 463) as f32 / 463.0 * 0.62;
        let size = if h % 5 == 0 { 1.4 } else { 0.8 };
        (x, y, size)
    })
}

/// The rows of the picture: two finds that no player has: made up, with a quiet line as the real ones have.
pub fn sample_rows() -> Rows {
    let mut rows = Rows::default();
    for (name, kind, code, time) in [
        ("Pembroke Helmet", "Char_Armor_Helmet", "S2", "20:41:07"),
        ("Arclight Pistol", "WeaponPersonal", "S1", "20:12:33"),
    ] {
        rows.push(
            Row::Found {
                name: name.into(),
                kind: versekit_core::blueprints::kind_readable(kind),
                code: code.into(),
                time: time.into(),
                caught_up: false,
            },
            ROWS_SHOWN,
        );
    }
    rows
}

/// The overlay as the player's settings draw it, inside a frame of the game. `opacity` is 0.3 … 1.0.
///
/// What the overlay learned in the frame comes back, for a test: in the picture it must always be nothing.
pub fn overlay_picture(
    ui: &mut Ui,
    scheme: &Scheme,
    opacity: f32,
    bar: BarSide,
) -> overlay::Output {
    let mut learned = overlay::Output::default();
    frame(ui, scheme, &t("as_vorschau_name"), FRAME_HEIGHT, |ui, rect| {
        let rows = sample_rows();
        let status = Message::Phrase(Phrase::new("ov_status").with("0", 128).with("1", "20:41:07"));
        let width = versekit_core::overlay::DEFAULT_WIDTH as f32;
        let size = vec2(width.min(rect.width() - 2.0 * MARGIN), OVERLAY_HEIGHT);
        let at = Rect::from_min_size(rect.min + vec2(MARGIN, MARGIN), size);
        let view = overlay::View {
            scheme,
            shown: Shown::Open,
            rows: &rows,
            contracts: &[],
            contracts_max: versekit_core::overlay::CONTRACTS_MAX_DEFAULT as usize,
            hidden: None,
            refinery: &[],
            launcher: false,
            status: &status,
            location: None,
            shard: None,
            shard_hint: None,
            signature: None,
            demand: None,
            update: None,
            game: true,
            locked: false,
            bar,
            anchored: (false, false),
            opacity,
            extras: &[],
            menu: true,
            drag_under_buttons: false,
            hangar: None,
            hints_outside: false,
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
        child.set_clip_rect(at);
        // Nothing of it can be focused or pressed (neither with the mouse nor with Tab and Enter), and it is not
        // dimmed for that: the picture shows the overlay as it is.
        child.visuals_mut().disabled_alpha = 1.0;
        child.disable();
        learned = overlay::show(&mut child, &view);
    });
    learned
}

/// The height of the picture of the taskbar.
pub const TASKBAR_FRAME_HEIGHT: f32 = 76.0;

/// The taskbar of the system: neutral dark, the same in every scheme (it is the system's, not ours).
const BAR_GROUND: Color32 = Color32::from_rgb(0x14, 0x18, 0x22);
const BAR_ITEM: Color32 = Color32::from_rgb(0x3a, 0x44, 0x58);

/// The corner of a taskbar with the clock, and the icon of Verse-Kit next to it when the player wants it there:
/// what the switch "Symbol in der Ablage neben der Uhr" does. `tray_on` draws the icon.
pub fn taskbar_picture(ui: &mut Ui, scheme: &Scheme, tray_on: bool) {
    let width = ui.available_width().min(FRAME_MAX_WIDTH);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TASKBAR_FRAME_HEIGHT), Sense::hover());
    let radius = box_radius(scheme);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius, SKY_BOTTOM);
    painter.text(
        pos2(rect.min.x + MARGIN, rect.min.y + 8.0),
        Align2::LEFT_TOP,
        t("as_vorschau_marke"),
        FontId::proportional(14.0),
        GAME_WORD,
    );
    // The taskbar along the bottom edge.
    let bar = Rect::from_min_max(pos2(rect.min.x, rect.max.y - 34.0), rect.max);
    painter.rect_filled(bar, egui::CornerRadius { nw: 0, ne: 0, sw: radius.sw, se: radius.se }, BAR_GROUND);
    // The system's own buttons at the left: a start button and three programs.
    for i in 0..4 {
        let item = Rect::from_min_size(pos2(bar.min.x + MARGIN + i as f32 * 30.0, bar.min.y + 9.0), vec2(18.0, 16.0));
        painter.rect_filled(item, 3.0, BAR_ITEM);
    }
    // The corner at the right: the arrow of the hidden icons, our icon, the clock.
    let mut x = bar.max.x - MARGIN;
    let clock =
        painter.text(pos2(x, bar.center().y), Align2::RIGHT_CENTER, "20:41", FontId::proportional(13.0), GAME_TEXT);
    x = clock.min.x - 14.0;
    if tray_on {
        let icon = Rect::from_center_size(pos2(x - 9.0, bar.center().y), vec2(18.0, 18.0));
        painter.rect_filled(icon, 4.0, color(scheme.accent_dark));
        painter.rect_stroke(icon, 4.0, Stroke::new(1.5, color(scheme.accent)), StrokeKind::Inside);
        painter.line(
            vec![
                pos2(icon.min.x + 4.5, icon.min.y + 5.0),
                pos2(icon.center().x, icon.max.y - 4.5),
                pos2(icon.max.x - 4.5, icon.min.y + 5.0),
            ],
            Stroke::new(1.8, color(scheme.accent)),
        );
        x = icon.min.x - 10.0;
    }
    // The arrow of the hidden icons: a mark drawn from two lines, not a letter.
    let chevron = pos2(x - 6.0, bar.center().y);
    let stroke = Stroke::new(1.5, GAME_WORD);
    painter.line(vec![chevron + vec2(-4.0, 2.5), chevron + vec2(0.0, -2.5), chevron + vec2(4.0, 2.5)], stroke);
    painter.rect_stroke(rect, radius, Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
    let cover = ui.interact(rect, response.id.with("cover"), Sense::CLICK | Sense::DRAG);
    cover.widget_info(|| WidgetInfo::labeled(Role::Image, false, t("as_vorschau_tray")));
}

/// The height of a strip of steps.
pub const FLOW_HEIGHT: f32 = 64.0;

/// A strip of steps from left to right: a box for each, an arrow between two. The words wrap inside their
/// box. The strip answers no click; it is named `name` for screen readers, and the steps are in the name.
pub fn flow_strip(ui: &mut Ui, scheme: &Scheme, name: &str, steps: &[String]) {
    let width = ui.available_width().min(FRAME_MAX_WIDTH);
    let (rect, response) = ui.allocate_exact_size(vec2(width, FLOW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let radius = box_radius(scheme);
    let arrow = 22.0;
    let count = steps.len().max(1) as f32;
    let box_width = (rect.width() - arrow * (count - 1.0)) / count;
    for (i, step) in steps.iter().enumerate() {
        let left = rect.min.x + i as f32 * (box_width + arrow);
        let area = Rect::from_min_size(pos2(left, rect.min.y), vec2(box_width, rect.height()));
        painter.rect_filled(area, radius, color(scheme.surface));
        painter.rect_stroke(area, radius, Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
        // The number of the step, in the house colour, then its words.
        painter.text(
            area.min + vec2(8.0, 6.0),
            Align2::LEFT_TOP,
            (i + 1).to_string(),
            crate::fonts::bold(13.0),
            color(scheme.accent),
        );
        let galley = painter.layout(step.clone(), FontId::proportional(12.5), color(scheme.fg), box_width - 28.0);
        painter.galley(area.min + vec2(24.0, 6.0), galley, color(scheme.fg));
        if i + 1 < steps.len() {
            let tip = pos2(area.max.x + arrow - 5.0, area.center().y);
            let tail = pos2(area.max.x + 5.0, area.center().y);
            let stroke = Stroke::new(1.5, color(scheme.sub));
            painter.line(vec![tail, tip], stroke);
            painter.line(vec![tip + vec2(-4.0, -4.0), tip, tip + vec2(-4.0, 4.0)], stroke);
        }
    }
    let spoken = steps.join(", ");
    response.widget_info(|| WidgetInfo::labeled(Role::Image, false, format!("{name}: {spoken}")));
}

/// The size of the tile of the start page in the picture.
pub const TILE_SIZE: egui::Vec2 = egui::vec2(236.0, 92.0);

/// A tile of the start page as it looks in the scheme in use: the title, a large number and a bar. The number
/// is the one of the player's blueprints now, so that the tile is not made up.
pub fn tile_picture(ui: &mut Ui, scheme: &Scheme, count: usize) {
    let (rect, response) = ui.allocate_exact_size(TILE_SIZE, Sense::hover());
    let radius = box_radius(scheme);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius, color(scheme.surface));
    painter.rect_stroke(rect, radius, Stroke::new(1.0, color(scheme.line)), StrokeKind::Inside);
    let head = crate::widgets::heading_color(scheme);
    painter.text(
        rect.min + vec2(MARGIN, 10.0),
        Align2::LEFT_TOP,
        scheme.heading(&t("bauplaene")),
        crate::fonts::bold(15.0),
        color(head),
    );
    painter.text(
        rect.min + vec2(MARGIN, 34.0),
        Align2::LEFT_TOP,
        count.to_string(),
        crate::fonts::bold(26.0),
        color(scheme.fg),
    );
    if scheme.square {
        for mark in crate::widgets::corner_marks(rect) {
            painter.line(mark.to_vec(), Stroke::new(1.5, color(scheme.accent)));
        }
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Image, false, t("as_vorschau_kachel")));
}

/// The size of a card of a colour scheme.
pub const SCHEME_CARD: egui::Vec2 = egui::vec2(168.0, 104.0);

/// A card that shows `candidate` as a small window drawn in its own colours (bar, a card with a heading, two
/// lines of text and a button), with its name below in the colours of the scheme in use, `current`. `selected`
/// marks the one in use. The answer is clicked when the player takes it; the card is a radio button for the
/// keyboard and for screen readers.
pub fn scheme_card(ui: &mut Ui, current: &Scheme, candidate: &Scheme, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(SCHEME_CARD, Sense::click());
    let name = t(candidate.label);
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, &name));
    let painter = ui.painter_at(rect);
    let radius = box_radius(candidate);
    // The picture of the window: all in the colours of the candidate.
    let window = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - 22.0));
    painter.rect_filled(window, radius, color(candidate.bg));
    let bar = Rect::from_min_size(window.min + vec2(1.0, 1.0), vec2(window.width() - 2.0, 16.0));
    painter.rect_filled(bar, egui::CornerRadius::ZERO, color(candidate.bar));
    painter.circle_filled(bar.left_center() + vec2(9.0, 0.0), 3.5, color(candidate.accent));
    painter.rect_filled(
        Rect::from_min_size(bar.left_center() + vec2(20.0, -2.0), vec2(44.0, 4.0)),
        2.0,
        color(candidate.sub),
    );
    let card = Rect::from_min_max(window.min + vec2(8.0, 24.0), window.max - vec2(8.0, 8.0));
    painter.rect_filled(card, radius, color(candidate.surface));
    painter.rect_stroke(card, radius, Stroke::new(1.0, color(candidate.line)), StrokeKind::Inside);
    let head = if candidate.accent_headings { candidate.accent } else { candidate.fg };
    painter.rect_filled(Rect::from_min_size(card.min + vec2(8.0, 7.0), vec2(56.0, 6.0)), 2.0, color(head));
    painter.rect_filled(
        Rect::from_min_size(card.min + vec2(8.0, 20.0), vec2(card.width() - 36.0, 4.0)),
        2.0,
        color(candidate.fg),
    );
    painter.rect_filled(
        Rect::from_min_size(card.min + vec2(8.0, 29.0), vec2(card.width() - 60.0, 4.0)),
        2.0,
        color(candidate.sub),
    );
    let button = Rect::from_min_size(pos2(card.max.x - 38.0, card.max.y - 16.0), vec2(30.0, 10.0));
    painter.rect_filled(button, radius, color(candidate.accent_dark));
    painter.rect_stroke(button, radius, Stroke::new(1.0, color(candidate.accent)), StrokeKind::Inside);
    // The frame: the house colour of the scheme in use for the chosen one, a hint of it when hovered.
    // (A thin line of the muted colour for the others: the line colour of the scheme is too faint on the field.)
    let stroke = match (selected, response.hovered()) {
        (true, true) => Stroke::new(3.0, color(current.accent)),
        (true, false) => Stroke::new(2.0, color(current.accent)),
        (false, true) => Stroke::new(1.5, color(current.accent)),
        (false, false) => Stroke::new(1.0, color(current.sub)),
    };
    painter.rect_stroke(window, radius, stroke, StrokeKind::Inside);
    // The ring of the keyboard lies outside the card: the painter of the whole area, not the one cut to the card.
    if response.has_focus() {
        ui.painter().rect_stroke(rect.expand(3.0), radius, Stroke::new(2.0, color(current.fg)), StrokeKind::Outside);
    }
    // The name below, in the colours of the window the player works in; a tick before the chosen one, so that
    // the choice is not told by the frame and the weight alone.
    let font = if selected { crate::fonts::bold(13.0) } else { FontId::proportional(13.0) };
    let mut x = rect.min.x + 2.0;
    if selected && let Some(image) = crate::icons::image("check", current.accent, 14.0) {
        let at = Rect::from_min_size(pos2(x, rect.max.y - 17.0), vec2(14.0, 14.0));
        image.paint_at(ui, at);
        x += 18.0;
    }
    painter.text(pos2(x, rect.max.y - 2.0), Align2::LEFT_BOTTOM, &name, font, color(current.fg));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The height of the frame with a contract title before and after.
pub const TITLE_FRAME_HEIGHT: f32 = 112.0;

/// The colour of a note in a title in the game (`<EM4>`): a clear blue, on every sky.
const NOTE_BLUE: Color32 = Color32::from_rgb(0x6c, 0xb0, 0xff);
/// The colour of the title itself in the game.
const GAME_TEXT: Color32 = Color32::from_rgb(0xdc, 0xe4, 0xf2);

/// A text of the game with its notes cut into pieces: `(text, is_note)`. The game marks a note with `<EM4>…</EM4>`.
pub fn note_spans(text: &str) -> Vec<(String, bool)> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<EM4>") {
        if start > 0 {
            spans.push((rest[..start].to_string(), false));
        }
        let after = &rest[start + "<EM4>".len()..];
        match after.find("</EM4>") {
            Some(end) => {
                spans.push((after[..end].to_string(), true));
                rest = &after[end + "</EM4>".len()..];
            }
            None => {
                spans.push((after.to_string(), true));
                rest = "";
            }
        }
    }
    if !rest.is_empty() {
        spans.push((rest.to_string(), false));
    }
    spans
}

/// The title of a made-up contract as the game shows it without and with the notes of Verse-Kit, as the
/// settings write them (`injection::title_suffix`): `with_notes` is the master switch of the notes, `rep_on` the
/// reputation in the title.
pub fn contract_title_picture(ui: &mut Ui, scheme: &Scheme, with_notes: bool, rep_on: bool) {
    let title = t("as_vorschau_auftrag");
    let words = versekit_core::injection::texts::words_for(
        if versekit_core::language::current() == versekit_core::language::Lang::De {
            "german_(germany)"
        } else {
            "english"
        },
        rep_on,
    );
    let entry: versekit_core::jsonio::Map = vec![
        ("rep".to_string(), versekit_core::jsonio::Value::int(10_000)),
        ("rep_eindeutig".to_string(), versekit_core::jsonio::Value::Bool(true)),
    ];
    let note = if with_notes { versekit_core::injection::texts::title_suffix(&entry, &words) } else { String::new() };
    let rows = [(t("as_vorher"), title.clone()), (t("as_nachher"), format!("{title}{note}"))];
    // The name for screen readers carries both lines, without the marks of the game.
    let plain = |text: &str| note_spans(text).into_iter().map(|(piece, _)| piece).collect::<String>();
    let name =
        format!("{}: {} {}; {} {}", t("as_vorschau_titel"), rows[0].0, plain(&rows[0].1), rows[1].0, plain(&rows[1].1));
    frame(ui, scheme, &name, TITLE_FRAME_HEIGHT, |ui, rect| {
        let painter = ui.painter_at(rect);
        let font = FontId::proportional(14.0);
        let label_width = 80.0;
        for (i, (label, text)) in rows.iter().enumerate() {
            let y = rect.min.y + MARGIN + i as f32 * 28.0;
            painter.text(pos2(rect.min.x + MARGIN, y), Align2::LEFT_TOP, label, font.clone(), GAME_WORD);
            let mut job = egui::text::LayoutJob::default();
            for (piece, is_note) in note_spans(text) {
                job.append(
                    &piece,
                    0.0,
                    egui::TextFormat::simple(font.clone(), if is_note { NOTE_BLUE } else { GAME_TEXT }),
                );
            }
            let galley = painter.layout_job(job);
            painter.galley(pos2(rect.min.x + MARGIN + label_width, y), galley, GAME_TEXT);
        }
    });
}

#[cfg(test)]
mod tests {
    use egui::epaint::Shape;
    use egui::{Context, Event, PointerButton, Pos2, RawInput};
    use versekit_core::theme;

    use super::overlay::Action;
    use super::*;

    /// Runs one frame of the picture in a window of 700 x 300, with `events`, and gives back the shapes.
    fn frame_of(ctx: &Context, opacity: f32, events: Vec<Event>) -> Vec<Shape> {
        let scheme = &theme::schemes()[0];
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let _ = overlay_picture(ui, scheme, opacity, BarSide::Top);
        });
        out.textures_delta.clear();
        out.shapes.into_iter().map(|c| c.shape).collect()
    }

    fn rect_fills(shapes: &[Shape], out: &mut Vec<Color32>) {
        for shape in shapes {
            match shape {
                Shape::Rect(rect) => out.push(rect.fill),
                Shape::Vec(inner) => rect_fills(inner, out),
                _ => {}
            }
        }
    }

    #[test]
    fn the_background_of_the_overlay_in_the_picture_takes_the_opacity_of_the_setting() {
        let scheme = &theme::schemes()[0];
        let ctx = Context::default();
        crate::install(&ctx);
        for opacity in [0.3_f32, 0.62, 1.0] {
            let mut fills = Vec::new();
            for _ in 0..3 {
                fills.clear();
                rect_fills(&frame_of(&ctx, opacity, Vec::new()), &mut fills);
            }
            let base = color(scheme.bg);
            let alpha = (opacity * 255.0).round() as u8;
            let wanted = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
            assert!(fills.contains(&wanted), "opacity {opacity}: no background {wanted:?} among {fills:?}");
        }
    }

    /// The contrast ratio of two colours as WCAG states it.
    fn ratio(a: Color32, b: Color32) -> f64 {
        fn channel(v: u8) -> f64 {
            let c = f64::from(v) / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        fn luminance(c: Color32) -> f64 {
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn the_words_and_the_stars_of_the_placeholder_are_readable_on_both_skies() {
        for sky in [SKY_TOP, SKY_BOTTOM] {
            assert!(ratio(GAME_WORD, sky) >= 4.5, "the words on {sky:?}: {}", ratio(GAME_WORD, sky));
            assert!(ratio(STAR, sky) >= 3.0, "the stars on {sky:?}: {}", ratio(STAR, sky));
        }
    }

    #[test]
    fn the_taskbar_shows_our_icon_only_while_the_tray_is_on() {
        let ctx = Context::default();
        crate::install(&ctx);
        let scheme = &theme::schemes()[0];
        let fills = |tray_on: bool| {
            let mut found = Vec::new();
            for _ in 0..2 {
                let input = RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 300.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| taskbar_picture(ui, scheme, tray_on));
                out.textures_delta.clear();
                found.clear();
                rect_fills(&out.shapes.into_iter().map(|c| c.shape).collect::<Vec<_>>(), &mut found);
            }
            found
        };
        let icon = color(scheme.accent_dark);
        assert!(!fills(false).contains(&icon), "no icon while the tray is off");
        assert!(fills(true).contains(&icon), "the icon next to the clock while it is on");
    }

    #[test]
    fn the_notes_of_a_game_text_are_cut_out_of_it() {
        assert_eq!(note_spans("plain"), vec![("plain".to_string(), false)]);
        assert_eq!(
            note_spans("Title <EM4>[BP 10.000 Rep]</EM4>"),
            vec![("Title ".to_string(), false), ("[BP 10.000 Rep]".to_string(), true)]
        );
        // An unclosed note is a note to the end; text after a note stays text.
        assert_eq!(note_spans("<EM4>open"), vec![("open".to_string(), true)]);
        assert_eq!(
            note_spans("<EM4>a</EM4> and <EM4>b</EM4>!"),
            vec![
                ("a".to_string(), true),
                (" and ".to_string(), false),
                ("b".to_string(), true),
                ("!".to_string(), false)
            ]
        );
    }

    #[test]
    fn the_note_blue_and_the_title_are_readable_on_the_sky() {
        for sky in [SKY_TOP, SKY_BOTTOM] {
            assert!(ratio(NOTE_BLUE, sky) >= 4.5, "the note on {sky:?}: {}", ratio(NOTE_BLUE, sky));
            assert!(ratio(GAME_TEXT, sky) >= 4.5, "the title on {sky:?}: {}", ratio(GAME_TEXT, sky));
        }
    }

    #[test]
    fn the_keyboard_finds_nothing_in_the_picture() {
        let ctx = Context::default();
        crate::install(&ctx);
        let scheme = &theme::schemes()[0];
        let tab = || Event::Key {
            key: egui::Key::Tab,
            physical_key: Some(egui::Key::Tab),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // Tab through the whole picture: the buttons of the bar and the grip must not be stops, and Enter
        // on a stop must not press anything.
        let mut learned = overlay::Output::default();
        for step in 0..20 {
            let events = if step < 2 { Vec::new() } else { vec![tab()] };
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut out =
                ctx.run_ui(input, |ui| learned = overlay_picture(ui, scheme, 0.9, BarSide::Top));
            out.textures_delta.clear();
            assert!(ctx.memory(|m| m.focused()).is_none(), "the picture took the focus at step {step}");
            assert!(learned.actions.is_empty() && learned.drag.is_none(), "step {step}: {learned:?}");
        }
    }

    #[test]
    fn a_click_or_drag_on_the_picture_reaches_nothing_below() {
        let ctx = Context::default();
        crate::install(&ctx);
        let scheme = &theme::schemes()[0];
        let click = |pos: Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let run = |events: Vec<Event>, learned: &mut overlay::Output| {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut out =
                ctx.run_ui(input, |ui| *learned = overlay_picture(ui, scheme, 0.9, BarSide::Top));
            out.textures_delta.clear();
        };
        // A few frames so that the buttons have their places.
        let mut learned = overlay::Output::default();
        for _ in 0..3 {
            run(Vec::new(), &mut learned);
        }
        assert!(learned.buttons.len() >= 5, "the bar has its buttons in the picture: {:?}", learned.buttons);
        // Every button of the bar is pressed and let go, and the bar is dragged: nothing is learned.
        let mut points: Vec<Pos2> = learned.buttons.iter().map(|(_, r)| r.center()).collect();
        points.push(pos2(learned.title_end - 6.0, learned.buttons[0].1.center().y));
        for at in points {
            let mut got = Vec::new();
            // A click: pressed and let go on the same spot.
            run(vec![Event::PointerMoved(at), click(at, true)], &mut learned);
            got.extend(learned.actions.clone());
            got.extend(learned.drag.map(|_| Action::Quit));
            run(vec![click(at, false)], &mut learned);
            got.extend(learned.actions.clone());
            // A drag: pressed, moved far, let go.
            let away = at + vec2(30.0, 12.0);
            run(vec![Event::PointerMoved(at), click(at, true)], &mut learned);
            run(vec![Event::PointerMoved(away)], &mut learned);
            got.extend(learned.drag.map(|_| Action::Quit));
            run(vec![click(away, false)], &mut learned);
            got.extend(learned.actions.clone());
            assert!(got.is_empty(), "the picture answered at {at:?}: {got:?}");
        }
    }
}
