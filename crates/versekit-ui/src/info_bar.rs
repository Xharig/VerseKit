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

//! What the info bar draws: one narrow line in a window of its own over the game. On the left the shard, in the
//! middle the room for the search of handles, on the right the scanned material while the scanner reads one.
//!
//! [`show`] draws the bar from the left edge of the `Ui` and says how wide it is: the window takes that width, so
//! the bar grows to the right when the material comes in. While the lock is open the whole bar moves the window.

use egui::{CursorIcon, FontId, Rect, Role, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2};
use versekit_core::language::t;
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::handle_search::{self, HandleSearch};
use crate::widgets::{corner_marks, rounded_corner_marks};
use crate::{box_radius, color};

/// Height of the bar in points: lower than the bar of the overlay, so that in the middle it ends above the compass
/// of the game.
pub const HEIGHT: f32 = 24.0;
/// Space above and below the search field inside the bar.
const FIELD_INSET: f32 = 2.0;
const PAD: f32 = 8.0;
const ICON: f32 = 16.0;
/// Space between an icon and its text.
const ICON_GAP: f32 = 5.0;
/// Space on each side of the line between two parts.
const PART_GAP: f32 = 9.0;

/// The scanned material as the bar shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mining<'a> {
    /// The material behind the number; empty where there is none.
    pub material: &'a str,
    /// The material is a name the data know: it stands in the accent.
    pub known: bool,
    /// The number as the HUD writes it.
    pub number: &'a str,
}

/// Everything one frame shows.
pub struct View<'a> {
    pub scheme: &'a Scheme,
    /// The frame rates of the game and the server, while they are read; they stand left of the shard.
    pub fps: Option<&'a str>,
    /// The players on the shard and on the own server, while they are read; between the frame rates and the shard.
    pub players: Option<&'a str>,
    /// The shard in short; `None` while there is none (no game, no join yet).
    pub shard: Option<&'a str>,
    /// The scanned material, only while the scanner is on and reads one.
    pub mining: Option<Mining<'a>>,
    /// Background opacity, 0.3 … 1.0.
    pub opacity: f32,
    /// The lock of the overlay is open: the bar can be dragged.
    pub movable: bool,
}

/// What one frame learned.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// The width the bar needs, in points; with the card or the log below it the wider of the two.
    pub width: f32,
    /// The height it needs: the bar, and the card and the log lines below it.
    pub height: f32,
    /// A drag began on the bar this frame.
    pub drag: bool,
    /// The field of the search has the cursor: the window needs the keyboard.
    pub typing: bool,
    /// The texts of the three parts in their order.
    pub drawn: Vec<String>,
}

/// The size of the window for a bar `width` points wide and `height` points high (at least the bar).
pub fn size(width: f32, height: f32) -> Vec2 {
    vec2(width.ceil(), height.max(HEIGHT).ceil())
}

/// Space between the bar and the card below it.
const BELOW_GAP: f32 = 4.0;

fn with_alpha(c: Color, opacity: f32) -> egui::Color32 {
    let base = color(c);
    egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// One part of the bar: its icon and its texts with their fonts and colours; `field` is the search field instead.
struct Part {
    icon: &'static str,
    texts: Vec<(String, FontId, Color)>,
    field: bool,
}

fn parts(ui: &Ui, view: &View, field: bool) -> Vec<Part> {
    let s = view.scheme;
    let body = FontId::proportional(TextStyle::Body.resolve(ui.style()).size);
    let small = FontId::proportional(TextStyle::Small.resolve(ui.style()).size);
    let search = if field {
        Part { icon: "search", texts: Vec::new(), field: true }
    } else {
        Part { icon: "search", texts: vec![(t("ib_suche_bald"), small.clone(), s.sub)], field: false }
    };
    let read = |icon: &'static str, text: Option<&str>| {
        text.map(|text| Part { icon, texts: vec![(text.to_string(), body.clone(), s.fg)], field: false })
    };
    let mut parts: Vec<Part> = read("activity", view.fps).into_iter().chain(read("users", view.players)).collect();
    parts.extend([
        Part {
            icon: "server",
            texts: vec![match view.shard {
                Some(shard) => (shard.to_string(), body.clone(), s.fg),
                None => (t("ib_kein_shard"), small.clone(), s.sub),
            }],
            field: false,
        },
        search,
    ]);
    if let Some(mining) = view.mining {
        let bold = crate::fonts::bold(body.size);
        let texts = if mining.material.is_empty() {
            vec![(mining.number.to_string(), bold, s.fg)]
        } else {
            vec![
                (mining.material.to_string(), bold, if mining.known { s.accent } else { s.fg }),
                (mining.number.to_string(), small, s.sub),
            ]
        };
        parts.push(Part { icon: "pickaxe", texts, field: false });
    }
    parts
}

/// Draws the bar at the top left of `ui` and says what happened; the middle holds a note that the search follows.
pub fn show(ui: &mut Ui, view: &View) -> Output {
    show_with(ui, view, None)
}

/// Draws the bar at the top left of `ui`, with `search` in the middle when given: its field in the bar, its card
/// and the lines of its log monitor below the bar. Says what happened.
pub fn show_with(ui: &mut Ui, view: &View, mut search: Option<&mut HandleSearch>) -> Output {
    let s = view.scheme;
    let mut out = Output::default();
    ui.style_mut().interaction.selectable_labels = false;
    let painter = ui.painter().clone();
    let laid: Vec<(&'static str, Vec<std::sync::Arc<egui::Galley>>, bool)> = parts(ui, view, search.is_some())
        .into_iter()
        .map(|part| {
            let galleys = part.texts.into_iter().map(|(text, font, c)| painter.layout_no_wrap(text, font, color(c)));
            (part.icon, galleys.collect(), part.field)
        })
        .collect();
    let text_width = |galleys: &[std::sync::Arc<egui::Galley>]| {
        galleys.iter().map(|g| g.size().x).sum::<f32>() + ICON_GAP * galleys.len().saturating_sub(1) as f32
    };
    let field_width = handle_search::field_width(ui);
    let part_width = |galleys: &[std::sync::Arc<egui::Galley>], field: bool| {
        if field { field_width } else { ICON + ICON_GAP + text_width(galleys) }
    };
    let width = PAD
        + laid.iter().map(|(_, galleys, field)| part_width(galleys, *field)).sum::<f32>()
        + 2.0 * PART_GAP * laid.len().saturating_sub(1) as f32
        + PAD;
    out.width = width;
    out.height = HEIGHT;
    let rect = Rect::from_min_size(ui.max_rect().min, vec2(width.ceil(), HEIGHT));

    let radius = box_radius(s);
    painter.rect(rect, radius, with_alpha(s.bar, view.opacity), Stroke::new(1.0, color(s.line)), StrokeKind::Inside);
    // The area of the bar comes first: the field laid over it later takes its own clicks.
    let sense = if view.movable { Sense::click_and_drag() } else { Sense::hover() };
    let area = ui.interact(rect, ui.make_persistent_id("infoleiste"), sense);
    let y = rect.center().y;
    let mut x = rect.min.x + PAD;
    for (i, (icon, galleys, field)) in laid.iter().enumerate() {
        if i > 0 {
            x += PART_GAP;
            painter
                .line_segment([pos2(x, rect.min.y + 6.0), pos2(x, rect.max.y - 6.0)], Stroke::new(1.0, color(s.line)));
            x += PART_GAP;
        }
        if *field {
            let place =
                Rect::from_min_max(pos2(x, rect.min.y + FIELD_INSET), pos2(x + field_width, rect.max.y - FIELD_INSET));
            let builder =
                egui::UiBuilder::new().max_rect(place).layout(egui::Layout::left_to_right(egui::Align::Center));
            let mut inner = ui.new_child(builder);
            if let Some(search) = search.as_deref_mut() {
                search.field(&mut inner, s);
                let query = search.query.trim();
                out.drawn.push(if query.is_empty() { t("s_hq_feld_hinweis") } else { query.to_string() });
            }
            x += field_width;
            continue;
        }
        if let Some(image) = crate::icons::image(icon, s.icon(IconTint::Normal), ICON) {
            image.paint_at(ui, Rect::from_center_size(pos2(x + ICON / 2.0, y), Vec2::splat(ICON)));
        }
        x += ICON + ICON_GAP;
        let mut words = Vec::new();
        for (j, galley) in galleys.iter().enumerate() {
            if j > 0 {
                x += ICON_GAP;
            }
            painter.galley(pos2(x, y - galley.size().y / 2.0), galley.clone(), color(s.fg));
            x += galley.size().x;
            words.push(galley.text().to_string());
        }
        out.drawn.push(words.join(" "));
    }

    let stroke = Stroke::new(2.0, color(s.accent));
    if s.square {
        for mark in corner_marks(rect) {
            painter.line(mark.to_vec(), stroke);
        }
    } else {
        for mark in rounded_corner_marks(rect, f32::from(radius.nw)) {
            painter.line(mark, stroke);
        }
    }

    let name = out.drawn.join(" · ");
    area.widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
    let drag_help = view.movable && area.hovered();
    if view.movable {
        out.drag = area.drag_started();
        area.on_hover_cursor(CursorIcon::Grab);
    }
    below(ui, view, search.as_deref_mut(), rect, drag_help, &mut out);
    if let Some(search) = search {
        out.typing = search.typing(ui.ctx());
    }
    out
}

/// The short help, the card of the search and the lines of its log monitor below the bar `bar`: what they need
/// goes into `out`. The window grows to that size in the next frame; until then the part outside it is cut off.
fn below(
    ui: &mut Ui,
    view: &View,
    mut search: Option<&mut HandleSearch>,
    bar: Rect,
    drag_help: bool,
    out: &mut Output,
) {
    let s = view.scheme;
    let now = handle_search::now();
    let card = search.as_deref().is_some_and(|search| search.has_card());
    let log = search.as_deref().is_some_and(|search| search.has_log_lines(now));
    // A short help stands below the bar while no card shows: the window of the bar is only as high as the bar, a
    // tooltip would be cut off or cover it. On the field it says what the keys do, elsewhere that dragging moves.
    let help = if card {
        None
    } else if search.as_deref().is_some_and(|search| search.field_hovered()) {
        Some(t("s_hq_feld_tipp"))
    } else if drag_help {
        Some(t("hinweis_ziehen"))
    } else {
        None
    };
    if !card && !log && help.is_none() {
        return;
    }
    let width = handle_search::CARD_WIDTH;
    let place = Rect::from_min_size(pos2(bar.min.x, bar.max.y + BELOW_GAP), vec2(width, 4000.0));
    let builder = egui::UiBuilder::new().max_rect(place).layout(egui::Layout::top_down(egui::Align::Min));
    let mut inner = ui.new_child(builder);
    inner.set_max_width(width);
    if let Some(help) = help {
        egui::Frame::new()
            .fill(with_alpha(s.bar, view.opacity))
            .stroke(Stroke::new(1.0, color(s.line)))
            .corner_radius(box_radius(s))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(&mut inner, |ui| {
                ui.label(egui::RichText::new(help).small().color(color(s.sub)));
            });
    }
    if let Some(search) = search.as_deref_mut() {
        search.card(&mut inner, s);
    }
    if let Some(search) = search.filter(|_| log) {
        if card {
            inner.add_space(BELOW_GAP);
        }
        egui::Frame::new()
            .fill(with_alpha(s.bar, view.opacity))
            .stroke(Stroke::new(1.0, color(s.line)))
            .corner_radius(box_radius(s))
            .inner_margin(egui::Margin::same(6))
            .show(&mut inner, |ui| {
                ui.set_width(width - 14.0);
                search.log_lines(ui, s, now);
            });
    }
    let used = inner.min_rect();
    out.width = out.width.max(used.max.x - bar.min.x);
    out.height = out.height.max(used.max.y - bar.min.y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;
    use versekit_core::{language, streamer, theme};

    /// A drawn text: what it says, its left and right end.
    #[derive(Debug)]
    struct Drawn {
        text: String,
        left: f32,
        right: f32,
    }

    /// Draws `view` in a window 900 points wide in the text size `points` with `events`, twice (the second frame
    /// is the one looked at), and returns its output and the texts painted.
    fn frame(view: &View, points: i64, events: Vec<egui::Event>) -> (Output, Vec<Drawn>) {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, view.scheme);
        crate::apply_size(&ctx, points);
        let (mut out, mut drawn) = (Output::default(), Vec::new());
        for pass in 0..2 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 60.0))),
                ..Default::default()
            };
            if pass == 1 {
                input.events = events.clone();
            }
            let mut output = ctx.run_ui(input, |ui| out = show(ui, view));
            output.textures_delta.clear();
            drawn = output
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(text) => Some(Drawn {
                        text: text.galley.text().to_string(),
                        left: text.pos.x + text.galley.rect.min.x,
                        right: text.pos.x + text.galley.rect.max.x,
                    }),
                    _ => None,
                })
                .collect();
        }
        (out, drawn)
    }

    fn view<'a>(scheme: &'a Scheme, shard: Option<&'a str>, mining: Option<Mining<'a>>) -> View<'a> {
        View { scheme, fps: None, players: None, shard, mining, opacity: 0.93, movable: false }
    }

    /// The frame rates stand left of the shard while they are read, inside the width; without them the bar is
    /// as before.
    #[test]
    fn the_frame_rates_stand_left_of_the_shard() {
        let _language = crate::language_lock();
        let scheme = theme::by_id("original").expect("scheme");
        let (low, high) = versekit_core::font_size::POINTS_RANGE;
        for points in [low, high] {
            let rates = "60 FPS · Server 29.8";
            let with = View { fps: Some(rates), ..view(scheme, Some("euw1b · 420"), None) };
            let (out, drawn) = frame(&with, points, Vec::new());
            assert_eq!(out.drawn.first().map(String::as_str), Some(rates), "Stufe {points}");
            let at = |text: &str| drawn.iter().find(|d| d.text == text).unwrap_or_else(|| panic!("{text}"));
            assert!(at(rates).right < at("euw1b · 420").left, "Stufe {points}: left of the shard");
            assert!(at(rates).left >= PAD - 0.5, "Stufe {points}: inside the bar");
            let (without, _) = frame(&view(scheme, Some("euw1b · 420"), None), points, Vec::new());
            assert_eq!(without.drawn.len(), 2, "Stufe {points}: no rates, no part");
            // The players stand on their own between the rates and the shard.
            let both = View { players: Some("Shard 482 · Server 37"), ..with };
            let (out, drawn) = frame(&both, points, Vec::new());
            let at = |text: &str| drawn.iter().find(|d| d.text == text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(out.drawn.len(), 4, "Stufe {points}: rates, players, shard, search");
            assert!(at(rates).right < at("Shard 482 · Server 37").left, "Stufe {points}");
            assert!(at("Shard 482 · Server 37").right < at("euw1b · 420").left, "Stufe {points}");
        }
    }

    const GOLD: Mining<'static> = Mining { material: "Gold", known: true, number: "12,680" };

    /// The three parts stand in their order: shard, the room for the search, the material; the material only while
    /// it is given. Every text lies inside the width the bar asks for, in every text size.
    #[test]
    fn the_shard_the_search_and_the_material_stand_in_order_inside_the_width() {
        let _language = crate::language_lock();
        let scheme = theme::by_id("original").expect("scheme");
        let (low, high) = versekit_core::font_size::POINTS_RANGE;
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            for points in low..=high {
                let what = format!("{lang:?}, Stufe {points}");
                let (out, drawn) = frame(&view(scheme, Some("euw1b · 420"), Some(GOLD)), points, Vec::new());
                assert_eq!(
                    out.drawn,
                    ["euw1b · 420".to_string(), t("ib_suche_bald"), "Gold 12,680".to_string()],
                    "{what}"
                );
                let at = |text: &str| drawn.iter().find(|d| d.text == text).unwrap_or_else(|| panic!("{what}: {text}"));
                let (shard, search, gold, number) =
                    (at("euw1b · 420"), at(&t("ib_suche_bald")), at("Gold"), at("12,680"));
                assert!(shard.right < search.left && search.right < gold.left && gold.right < number.left, "{what}");
                assert!(shard.left >= PAD - 0.5, "{what}: the shard starts inside the bar");
                assert!(number.right <= out.width - PAD + 0.5, "{what}: the number ends inside the bar: {number:?}");

                let (without, drawn) = frame(&view(scheme, Some("euw1b · 420"), None), points, Vec::new());
                assert_eq!(without.drawn.len(), 2, "{what}: no material without the scanner");
                assert!(!drawn.iter().any(|d| d.text == "Gold"), "{what}");
                assert!(without.width < out.width, "{what}: the bar grows with the material, to the right");
            }
        }
        language::set_thread_language(language::Choice::Auto);
    }

    /// Without a shard the bar says so, in both languages; the material without a name shows its number.
    #[test]
    fn without_a_shard_the_bar_says_so_and_a_number_without_material_stands_alone() {
        let _language = crate::language_lock();
        let scheme = theme::by_id("original").expect("scheme");
        for lang in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(lang);
            let bare = Mining { material: "", known: false, number: "6,340" };
            let (out, _) = frame(&view(scheme, None, Some(bare)), 1, Vec::new());
            assert_eq!(out.drawn, [t("ib_kein_shard"), t("ib_suche_bald"), "6,340".to_string()], "{lang:?}");
            assert!(!t("ib_kein_shard").starts_with("ib_") && !t("ib_suche_bald").starts_with("ib_"), "{lang:?}");
        }
        language::set_thread_language(language::Choice::Auto);
    }

    /// A drag begins on the bar only while the lock is open; with the lock closed a press does nothing.
    #[test]
    fn a_drag_begins_on_the_bar_only_while_the_lock_is_open() {
        let scheme = theme::by_id("original").expect("scheme");
        let press = |movable: bool| {
            let mut shown = view(scheme, Some("euw1b · 420"), None);
            shown.movable = movable;
            let at = Pos2::new(20.0, HEIGHT / 2.0);
            let ctx = egui::Context::default();
            crate::install(&ctx);
            crate::apply(&ctx, scheme);
            let mut began = false;
            let steps = [
                vec![egui::Event::PointerMoved(at)],
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                vec![egui::Event::PointerMoved(at + vec2(30.0, 0.0))],
                vec![egui::Event::PointerMoved(at + vec2(60.0, 0.0))],
            ];
            for events in steps {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 60.0))),
                    events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| began |= show(ui, &shown).drag);
                output.textures_delta.clear();
            }
            began
        };
        assert!(press(true), "open lock: the bar moves");
        assert!(!press(false), "closed lock: the bar stays");
    }

    /// With the search, its field stands in the middle in place of the note; a click into it puts the cursor there
    /// and starts no drag, also with the lock open; a card below makes the bar higher and at least as wide.
    #[test]
    fn the_search_field_stands_in_the_middle_and_its_card_below() {
        let _language = crate::language_lock();
        language::set_thread_language(language::Choice::De);
        let scheme = theme::by_id("original").expect("scheme");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        crate::apply(&ctx, scheme);
        let mut search = HandleSearch::empty();
        let mut shown = view(scheme, Some("euw1b · 420"), None);
        shown.movable = true;
        let run = |search: &mut HandleSearch, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut out = Output::default();
            let mut output = ctx.run_ui(input, |ui| out = show_with(ui, &shown, Some(search)));
            output.textures_delta.clear();
            out
        };
        let first = run(&mut search, Vec::new());
        assert_eq!(first.drawn, ["euw1b · 420".to_string(), t("s_hq_feld_hinweis")]);
        assert!(!first.drawn.contains(&t("ib_suche_bald")), "the note gives way to the field");
        assert_eq!(first.height, HEIGHT, "nothing below the bar yet");
        assert!(!first.typing);
        // Counter-check: without the search the note stands there.
        let mut plain = Output::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| plain = show(ui, &shown));
        output.textures_delta.clear();
        assert_eq!(plain.drawn[1], t("ib_suche_bald"));
        // A click into the middle of the field: the cursor, no drag.
        let shard_end = PAD + ICON + ICON_GAP + 60.0;
        let at = Pos2::new(shard_end + 2.0 * PART_GAP + handle_search::FIELD_WIDTH / 2.0, HEIGHT / 2.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut search, vec![egui::Event::PointerMoved(at)]);
        let down = run(&mut search, vec![press(true)]);
        let up = run(&mut search, vec![press(false)]);
        assert!(!down.drag && !up.drag, "a click into the field moves nothing");
        let typed = run(&mut search, Vec::new());
        assert!(typed.typing, "the field has the cursor");
        // A search with an answer shows its card below the bar: the bar grows down and to the card's width.
        search.search(&ctx, "a/b", false);
        run(&mut search, Vec::new());
        let with_card = run(&mut search, Vec::new());
        assert!(with_card.height > HEIGHT + 20.0, "{}", with_card.height);
        assert!(with_card.width >= handle_search::CARD_WIDTH - 1.0, "{}", with_card.width);
        search.close();
        // The pointer still on the field: the short help stands below the bar, not over it.
        let help = run(&mut search, Vec::new());
        assert!(search.field_hovered(), "the pointer is on the field");
        assert!(help.height > HEIGHT + 4.0, "the help below: {}", help.height);
        // The pointer leaves: only the bar.
        run(&mut search, vec![egui::Event::PointerGone]);
        let closed = run(&mut search, Vec::new());
        assert_eq!(closed.height, HEIGHT, "closed: only the bar");
        // The pointer on the free bar: the drag help stands below it, no tooltip in the window of the bar.
        let free = Pos2::new(PAD + 4.0, HEIGHT / 2.0);
        run(&mut search, vec![egui::Event::PointerMoved(free)]);
        let drag = run(&mut search, Vec::new());
        assert!(!search.field_hovered(), "the pointer is beside the field");
        assert!(drag.height > HEIGHT + 4.0, "the drag help below: {}", drag.height);
        run(&mut search, vec![egui::Event::PointerGone]);
        assert_eq!(run(&mut search, Vec::new()).height, HEIGHT, "gone again");
        language::set_thread_language(language::Choice::Auto);
    }

    /// Streamer mode hides a learned value in the bar too.
    #[test]
    fn the_bar_hides_private_values_in_streamer_mode() {
        let scheme = theme::by_id("original").expect("scheme");
        let secret = "Leistenpilot4711"; // privacy-ok: made-up test value
        streamer::learn(secret);
        let shard = format!("euw1b · {secret}");
        let shown = view(scheme, Some(&shard), None);
        streamer::set_thread_active(Some(false));
        let (_, open) = frame(&shown, 1, Vec::new());
        streamer::set_thread_active(Some(true));
        let (_, covered) = frame(&shown, 1, Vec::new());
        streamer::set_thread_active(None);
        assert!(open.iter().any(|d| d.text.contains(secret)), "{open:?}");
        assert!(!covered.iter().any(|d| d.text.contains(secret)), "{covered:?}");
    }
}
