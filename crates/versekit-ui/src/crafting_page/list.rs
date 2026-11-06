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

//! The side of the crafting page to choose from, laid out like the crafting
//! terminal of the game: search, "clear filters" and sort, the ten
//! categories (each with a menu of its subcategories), the switches
//! "affordable" and "owned", the blueprint, material and ship filters and the
//! items as tiles or rows under the headings of their subcategories.
//!
//! The list draws only the lines in sight, so all items can stand in it at
//! once; it scrolls inside its own frame.

use egui::accesskit::Role;
use egui::text::{LayoutJob, TextWrapping};
use egui::{CursorIcon, Rect, RichText, Sense, StrokeKind, Ui, Vec2, WidgetInfo, vec2};
use versekit_core::fabricator::{CATEGORIES, Names, Place};
use versekit_core::language::{Arg, t, tf};
use versekit_core::stock_data::lower;
use versekit_core::theme::Scheme;
use versekit_gamedata::craft_book::{CraftItem, Have};

use super::{CraftingState, MaterialHit, Picks, ShipFit, sub, tn};
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius, icons};

/// The order of the items within a subcategory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Sort {
    #[default]
    NameUp,
    NameDown,
    Maker,
}

/// Tiles or rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    Grid,
    List,
}

impl View {
    /// How the setting stores it.
    pub(super) fn word(self) -> &'static str {
        match self {
            View::Grid => "raster",
            View::List => "liste",
        }
    }

    pub(super) fn from_word(word: Option<&str>) -> View {
        if word == Some("liste") { View::List } else { View::Grid }
    }
}

/// One line of the list: the heading of a subcategory or a row of items.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Line {
    Head(Place),
    Items(Vec<usize>),
}

/// The side of a category button.
const CATEGORY_BUTTON: f32 = 32.0;
/// The gap between the tiles.
const TILE_GAP: f32 = 6.0;

/// The width of a tile: room for a name of about fifteen letters on a line.
fn tile_width(ui: &Ui) -> f32 {
    (ui.text_style_height(&egui::TextStyle::Body) * 8.5).max(120.0)
}

/// The height of a tile: the tier line, two lines of name, the maker.
fn tile_height(ui: &Ui) -> f32 {
    let body = ui.text_style_height(&egui::TextStyle::Body);
    let small = ui.text_style_height(&egui::TextStyle::Small);
    2.0 * body + 2.0 * small + 18.0
}

/// The height of a row of the compact list.
pub(super) fn row_height(ui: &Ui) -> f32 {
    (ui.text_style_height(&egui::TextStyle::Body) + 8.0).max(super::ROW_HEIGHT)
}

fn head_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + 10.0
}

// ------------------------------------------------------------ what passes

/// Whether the item passes the picks (not the search text).
pub(super) fn passes(state: &mut CraftingState, index: usize, mark: Have) -> bool {
    let picks = state.picks.clone();
    let place = state.places[index];
    if picks.category.is_some_and(|c| c != place.category) {
        return false;
    }
    if picks.sub.is_some() && place.sub != picks.sub {
        return false;
    }
    // Unclear counts as neither held nor missing: nothing is claimed.
    if picks.state == "habe" && mark != Have::Yes || picks.state == "fehlt" && mark != Have::No {
        return false;
    }
    if !picks.material.is_empty() {
        let covered = state.covers(index);
        if (picks.material == "reicht") != covered {
            return false;
        }
    }
    match &state.ship_fit.0 {
        ShipFit::Off => true,
        ShipFit::NoData => false,
        ShipFit::Parts(parts) => parts.contains_key(&index),
    }
}

/// Whether the search text finds the item: its name, its maker, its kind, the
/// names of its category and subcategory, or a material of its recipe.
pub(super) fn found(
    item: &CraftItem,
    place: Place,
    names: &Names,
    text: &str,
    from_material: &std::collections::HashSet<&str>,
) -> bool {
    text.is_empty()
        || lower(&item.name).contains(text)
        || lower(&item.manufacturer).contains(text)
        || lower(&item.kind).contains(text)
        || lower(&names.path(place)).contains(text)
        || from_material.contains(item.name.as_str())
}

/// The items that pass search and filters in the order of the list (by
/// category and subcategory, within them by the sort) and the materials the
/// search names.
pub(super) fn matching(state: &mut CraftingState, marks: &[Have]) -> (Vec<usize>, Vec<MaterialHit>) {
    let text = state.search.trim().to_lowercase();
    // The search also finds the ingredient: all blueprints that need it.
    let material_hits: Vec<MaterialHit> = if text.is_empty() { Vec::new() } else { state.by_material(&text).to_vec() };
    let from_material: std::collections::HashSet<&str> =
        material_hits.iter().flat_map(|(_, names)| names.iter().map(String::as_str)).collect();
    let mut shown = Vec::new();
    for (index, mark) in marks.iter().enumerate() {
        if !passes(state, index, *mark) {
            continue;
        }
        if found(&state.book.items[index], state.places[index], &state.names, &text, &from_material) {
            shown.push(index);
        }
    }
    let items = &state.book.items;
    let places = &state.places;
    let sort = state.sort;
    // By size first, as every list of parts (`size_order`): an item without a size last.
    let size = |i: usize| {
        let size = state.sizes.get(i).copied().flatten();
        (size.is_none(), size.unwrap_or(0))
    };
    // The book is sorted by name already; the sort keeps that order where it does not decide.
    shown.sort_by(|&a, &b| {
        let (pa, pb) = (places[a], places[b]);
        let group = (pa.category, pa.sub_order()).cmp(&(pb.category, pb.sub_order()));
        group.then_with(|| match sort {
            Sort::NameUp => size(a).cmp(&size(b)).then(a.cmp(&b)),
            Sort::NameDown => b.cmp(&a),
            Sort::Maker => lower(&items[a].manufacturer).cmp(&lower(&items[b].manufacturer)).then(a.cmp(&b)),
        })
    });
    (shown, material_hits)
}

/// The lines of the list: a heading before each subcategory, then the items,
/// `per_line` to a line.
pub(super) fn lines(places: &[Place], shown: &[usize], per_line: usize) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current: Option<Place> = None;
    let mut row: Vec<usize> = Vec::new();
    for &index in shown {
        let place = places[index];
        if current != Some(place) {
            if !row.is_empty() {
                lines.push(Line::Items(std::mem::take(&mut row)));
            }
            lines.push(Line::Head(place));
            current = Some(place);
        }
        row.push(index);
        if row.len() == per_line.max(1) {
            lines.push(Line::Items(std::mem::take(&mut row)));
        }
    }
    if !row.is_empty() {
        lines.push(Line::Items(row));
    }
    lines
}

// ------------------------------------------------------------ the filters

/// A switch with its label in front of it, kept on one line so a wrapping
/// row moves both together; `on` is the state, the return value whether it
/// was flipped.
fn switch(ui: &mut Ui, scheme: &Scheme, label: &str, hint: &str, on: bool) -> bool {
    let mut now = on;
    // A wrapping row cannot break a pair that is laid out as one thing: it lets it run out of the row. So the
    // pair starts the next row itself when it does not fit in what is left of this one.
    let font = egui::TextStyle::Body.resolve(ui.style());
    let label_width = ui.painter().layout_no_wrap(label.to_string(), font, egui::Color32::PLACEHOLDER).size().x;
    let needed = label_width + ui.spacing().item_spacing.x + widgets::TOGGLE_SIZE.x;
    // ⚠ `available_width` of a wrapping row is the whole width, not what is left of the row: the cursor tells.
    let left = ui.max_rect().right() - ui.cursor().min.x;
    if left < needed && needed <= ui.max_rect().width() {
        ui.end_row();
    }
    ui.horizontal(|pair| {
        pair.label(label);
        let response = pair.add(widgets::Toggle::new(&mut now, scheme));
        let enabled = response.enabled();
        response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, now, label));
        response.on_hover_text(hint);
    });
    now != on
}

/// The frame of the menus that open under a button.
fn menu_frame(scheme: &Scheme) -> egui::Frame {
    egui::Frame::new()
        .fill(color(scheme.field))
        .stroke(egui::Stroke::new(1.0, color(scheme.accent)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin::same(4))
}

/// Room the window keeps free above and below a menu.
pub(super) const MENU_MARGIN: f32 = 24.0;
/// A menu is at least this high, however small the window.
const MENU_MIN: f32 = 120.0;

/// The entries of a menu: at most as high as the window, the rest scrolls.
pub(super) fn menu_entries(menu: &mut Ui, scheme: &Scheme, add: impl FnOnce(&mut Ui)) {
    let room = (menu.ctx().content_rect().height() - 2.0 * MENU_MARGIN).max(MENU_MIN);
    let area = egui::ScrollArea::vertical().id_salt("entries").max_height(room).auto_shrink([true, true]);
    widgets::scroll_area(menu, scheme, area, add);
}

/// One entry of a menu: a tick where it is chosen, the text, muted when
/// nothing stands under it.
fn menu_row(ui: &mut Ui, scheme: &Scheme, ticked: bool, text: &str, empty: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::TextStyle::Body.resolve(ui.style()),
        color(if empty { scheme.sub } else { scheme.fg }),
    );
    let width = (galley.size().x + 36.0).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, galley.size().y + 8.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, control_radius(scheme), color(scheme.hover));
        }
        if ticked && let Some(image) = icons::image("check", scheme.accent, 14.0) {
            image.paint_at(ui, Rect::from_center_size(rect.left_center() + vec2(12.0, 0.0), Vec2::splat(14.0)));
        }
        let pos = egui::pos2(rect.min.x + 26.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(pos, galley, color(scheme.fg));
    }
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::selected(Role::MenuItemCheckBox, enabled, ticked, text));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A choice in the menu of a category: a ticked entry chosen again takes the
/// filter away, any other one sets it.
pub(super) fn pick(picks: &mut Picks, category: usize, sub: Option<&'static str>) {
    if picks.category == Some(category) && picks.sub == sub {
        picks.category = None;
        picks.sub = None;
    } else {
        picks.category = Some(category);
        picks.sub = sub;
    }
}

/// A square button with the icon of a category; a small chevron in its corner
/// says that it opens a menu.
fn category_button(ui: &mut Ui, scheme: &Scheme, index: usize, name: &str, chosen: bool) -> egui::Response {
    let category = &CATEGORIES[index];
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CATEGORY_BUTTON), Sense::click());
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, chosen, name));
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let colors = widgets::button_colors(scheme, ButtonKind::Normal, hovered);
        let (fill, border, tint) = if chosen {
            (scheme.accent_dark, scheme.accent, scheme.accent)
        } else {
            let tint = if hovered {
                scheme.icon(versekit_core::theme::IconTint::Hover)
            } else {
                scheme.icon(versekit_core::theme::IconTint::Normal)
            };
            (colors.fill, colors.border, tint)
        };
        let stroke = egui::Stroke::new(1.0, color(border));
        ui.painter().rect(rect.shrink(1.0), control_radius(scheme), color(fill), stroke, StrokeKind::Inside);
        if let Some(image) = icons::image(category.icon, tint, 18.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center() - vec2(1.0, 1.0), Vec2::splat(18.0)));
        }
        if !category.subs.is_empty()
            && let Some(image) = icons::image("chevron-down", tint, 9.0)
        {
            image.paint_at(ui, Rect::from_center_size(rect.right_bottom() - vec2(7.0, 7.0), Vec2::splat(9.0)));
        }
    }
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(name.to_string())
}

/// The ten category buttons; a category with subcategories opens its menu
/// ("all categories" first, then the subcategories with their counts).
fn category_bar(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) {
    let mut counts: std::collections::HashMap<Place, usize> = std::collections::HashMap::new();
    let mut per_category = [0usize; CATEGORIES.len()];
    for place in &state.places {
        *counts.entry(*place).or_default() += 1;
        per_category[place.category] += 1;
    }
    let mut chosen: Option<(usize, Option<&'static str>)> = None;
    let names = &state.names;
    let picks = &state.picks;
    ui.horizontal_wrapped(|row| {
        row.spacing_mut().item_spacing.x = 4.0;
        for (index, category) in CATEGORIES.iter().enumerate() {
            let on = picks.category == Some(index);
            let response = category_button(row, scheme, index, &names.category(index), on);
            if category.subs.is_empty() {
                if response.clicked() {
                    chosen = Some((index, None));
                }
                continue;
            }
            let id = row.make_persistent_id(("herstellung-kategorie", index));
            egui::Popup::menu(&response).id(id).frame(menu_frame(scheme)).show(|menu| {
                menu.set_min_width(180.0);
                menu_entries(menu, scheme, |menu| {
                    let all = format!("{} ({})", names.all(), per_category[index]);
                    if menu_row(menu, scheme, on && picks.sub.is_none(), &all, per_category[index] == 0).clicked() {
                        chosen = Some((index, None));
                    }
                    for sub in category.subs {
                        let n = counts.get(&Place { category: index, sub: Some(sub.key) }).copied().unwrap_or(0);
                        let text = format!("{} ({n})", names.sub(sub.key));
                        if menu_row(menu, scheme, on && picks.sub == Some(sub.key), &text, n == 0).clicked() {
                            chosen = Some((index, Some(sub.key)));
                        }
                    }
                });
            });
        }
    });
    if let Some((category, sub)) = chosen {
        pick(&mut state.picks, category, sub);
    }
}

/// The button with the order of the list; it opens the menu of the orders.
fn sort_button(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) {
    let sorts =
        [(Sort::NameUp, "s_fb_sort_auf"), (Sort::NameDown, "s_fb_sort_ab"), (Sort::Maker, "s_fb_sort_hersteller")];
    let now = sorts.iter().find(|(s, _)| *s == state.sort).map_or("s_fb_sort_auf", |(_, key)| key);
    let word = t(now);
    let response = ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("arrow-down-up"));
    let response = response.on_hover_text(t("s_fb_sortieren"));
    let mut picked = None;
    egui::Popup::menu(&response).id(ui.make_persistent_id("herstellung-sortieren")).frame(menu_frame(scheme)).show(
        |menu| {
            menu.set_min_width(160.0);
            for (sort, key) in sorts {
                if menu_row(menu, scheme, state.sort == sort, &t(key), false).clicked() {
                    picked = Some(sort);
                }
            }
        },
    );
    if let Some(sort) = picked {
        state.sort = sort;
    }
}

/// The button of the ship filter with the chosen ship; it opens the menu of the hangar ships, "all ships"
/// first, or says that the hangar is empty.
fn ship_button(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) {
    let names = state.ship_names();
    let word = if state.picks.ship.is_empty() {
        t("s_he_schiff_knopf")
    } else {
        tf("s_he_schiff_fuer", &[("schiff", Arg::from(state.picks.ship.as_str()))])
    };
    let response = ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("rocket").wrapping());
    let response = response.on_hover_text(t("s_he_schiff_tip"));
    let mut picked: Option<String> = None;
    egui::Popup::menu(&response).id(ui.make_persistent_id("herstellung-schiff")).frame(menu_frame(scheme)).show(
        |menu| {
            menu.set_min_width(200.0);
            if names.is_empty() {
                menu.set_max_width(260.0);
                menu.add(egui::Label::new(sub(scheme, t("s_he_schiff_leer"))).wrap());
                return;
            }
            menu_entries(menu, scheme, |menu| {
                if menu_row(menu, scheme, state.picks.ship.is_empty(), &t("s_he_schiff_alle"), false).clicked() {
                    picked = Some(String::new());
                }
                for name in &names {
                    if menu_row(menu, scheme, state.picks.ship == *name, name, false).clicked() {
                        picked = Some(name.clone());
                    }
                }
            });
        },
    );
    if let Some(ship) = picked {
        state.picks.ship = ship;
    }
}

/// The line about the chosen ship: how many of the shown items fit it and how many of them one has the
/// blueprint of, or that its slots are not known yet.
fn ship_line(ui: &mut Ui, scheme: &Scheme, state: &CraftingState, marks: &[Have], shown: &[usize]) {
    let ship = Arg::from(state.picks.ship.as_str());
    match &state.ship_fit.0 {
        ShipFit::Off => {}
        ShipFit::NoData => {
            let text = tf("s_he_schiff_unbekannt", &[("schiff", ship)]);
            ui.add(egui::Label::new(widgets::bold(ui, &text, egui::TextStyle::Body).color(color(scheme.gold))).wrap());
        }
        ShipFit::Parts(_) => {
            let held = shown.iter().filter(|&&i| marks.get(i) == Some(&Have::Yes)).count();
            let text =
                tf("s_he_schiff_zeile", &[("schiff", ship), ("n", Arg::from(shown.len())), ("habe", Arg::from(held))]);
            ui.add(egui::Label::new(RichText::new(text).color(color(scheme.accent))).wrap());
        }
    }
}

/// Everything above the list: search, buttons, categories, switches and filters.
fn filters(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState) {
    let width = ui.available_width();
    widgets::search_field(ui, &mut state.search, &t("s_pl_herstellung"), scheme, width);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        if row
            .add(widgets::Button::new(&t("s_fb_filter_leeren"), ButtonKind::Normal, scheme).icon("funnel-x"))
            .clicked()
        {
            state.clear_filters();
        }
        sort_button(row, scheme, state);
        ship_button(row, scheme, state);
        let (icon, meaning, next) = match state.view {
            View::Grid => ("list", t("s_fb_liste"), View::List),
            View::List => ("layout-grid", t("s_fb_raster"), View::Grid),
        };
        let response = widgets::icon_button(row, scheme, icon, &meaning);
        let enabled = response.enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &meaning));
        if response.clicked() {
            state.view = next;
        }
    });
    ui.add_space(4.0);
    category_bar(ui, scheme, state);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, t("s_fb_anzeigen")));
        // Off shows everything; on only what the stock covers or what one has the blueprint of.
        let affordable = state.picks.material == "reicht";
        if switch(row, scheme, &t("s_fb_erschwinglich"), &t("s_fb_erschwinglich_tip"), affordable) {
            state.picks.material = if affordable { String::new() } else { "reicht".into() };
        }
        let owned = state.picks.state == "habe";
        if switch(row, scheme, &t("s_fb_besitz"), &t("s_fb_besitz_tip"), owned) {
            state.picks.state = if owned { String::new() } else { "habe".into() };
        }
    });
}

// ------------------------------------------------------------ the items

/// The status icon of an item, its colour and what it says.
fn mark_icon(scheme: &Scheme, mark: Have) -> (&'static str, versekit_core::theme::Color, &'static str) {
    match mark {
        Have::Yes => ("check", scheme.accent, "ff_zustand_habe"),
        Have::Unclear => ("circle-question-mark", scheme.gold, "s_he_unklar"),
        Have::No => ("circle", scheme.sub, "ff_zustand_fehlt"),
    }
}

/// What a screen reader says of an item and what its hover says.
fn spoken(item: &CraftItem, mark: Have) -> (String, String) {
    let (_, _, hint) = mark_icon(versekit_core::theme::current(), mark);
    let state_name = t(hint);
    let full = if item.manufacturer.is_empty() {
        item.name.clone()
    } else {
        format!("{}  ·  {}", item.name, item.manufacturer)
    };
    (format!("{full}, {state_name}"), format!("{full}\n{state_name}"))
}

/// The id of the tile or row of an item: its name, which no other item has, so it stays while the list scrolls.
fn item_id(ui: &Ui, item: &CraftItem) -> egui::Id {
    ui.make_persistent_id(("herstellung-teil", item.name.as_str()))
}

/// The tier mark of a tile: the first tier, of how many.
fn tier_mark(item: &CraftItem) -> String {
    tf("s_fb_stufe_kurz", &[("n", Arg::Int(1))])
        + &if item.tiers > 1 { format!("/{}", item.tiers) } else { String::new() }
}

/// A tile: tier mark with class, size and grade and the status at the top,
/// the name on up to two lines, the maker under it.
fn tile(
    ui: &mut Ui,
    scheme: &Scheme,
    rect: Rect,
    item: &CraftItem,
    spec: &str,
    mark: Have,
    chosen: bool,
) -> egui::Response {
    let response = ui.interact(rect, item_id(ui, item), Sense::click());
    let (access, hover) = spoken(item, mark);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &access));
    let painter = ui.painter();
    let (fill, border) = if chosen {
        (scheme.accent_dark, scheme.accent)
    } else if response.hovered() {
        (scheme.hover, scheme.line)
    } else {
        (scheme.surface, scheme.line)
    };
    painter.rect(rect, control_radius(scheme), color(fill), egui::Stroke::new(1.0, color(border)), StrokeKind::Inside);
    let small = egui::TextStyle::Small.resolve(ui.style());
    let body = egui::TextStyle::Body.resolve(ui.style());
    let inner = rect.shrink2(vec2(6.0, 5.0));
    let head = if spec.is_empty() { tier_mark(item) } else { format!("{} · {spec}", tier_mark(item)) };
    let mut job = LayoutJob::simple_singleline(head, small.clone(), color(scheme.sub));
    job.wrap = TextWrapping::truncate_at_width(inner.width() - 18.0);
    let tier = painter.layout_job(job);
    let tier_height = tier.size().y;
    painter.galley(inner.left_top(), tier, color(scheme.sub));
    let (icon, tone, _) = mark_icon(scheme, mark);
    if let Some(image) = icons::image(icon, tone, 14.0) {
        image.paint_at(ui, Rect::from_min_size(egui::pos2(inner.right() - 14.0, inner.top()), Vec2::splat(14.0)));
    }
    let name_color = if chosen { scheme.accent } else { scheme.fg };
    let mut job = LayoutJob::simple(item.name.clone(), body, color(name_color), inner.width());
    job.wrap = TextWrapping { max_width: inner.width(), max_rows: 2, ..Default::default() };
    let name = ui.painter().layout_job(job);
    let top = inner.top() + tier_height.max(14.0) + 2.0;
    ui.painter().galley(egui::pos2(inner.left(), top), name, color(name_color));
    if !item.manufacturer.is_empty() {
        let mut job = LayoutJob::simple_singleline(item.manufacturer.clone(), small, color(scheme.sub));
        job.wrap = TextWrapping::truncate_at_width(inner.width());
        let maker = ui.painter().layout_job(job);
        let y = inner.bottom() - maker.size().y;
        ui.painter().galley(egui::pos2(inner.left(), y), maker, color(scheme.sub));
    }
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hover)
}

/// A row of the compact list: status, name, class, size, grade and maker at the right.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    scheme: &Scheme,
    rect: Rect,
    item: &CraftItem,
    spec: &str,
    mark: Have,
    k: usize,
    chosen: bool,
) -> egui::Response {
    let response = ui.interact(rect, item_id(ui, item), Sense::click());
    let (access, hover) = spoken(item, mark);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &access));
    let painter = ui.painter();
    if chosen {
        // The chosen row, as in the lists of the other pages: dark accent area and an accent bar at the left.
        painter.rect_filled(rect, 0.0, color(scheme.accent_dark));
        let bar = Rect::from_min_size(rect.min, vec2(widgets::LIST_BAR_WIDTH, rect.height()));
        painter.rect_filled(bar, 0.0, color(scheme.accent));
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, color(scheme.hover));
    } else if k % 2 == 1 {
        painter.rect_filled(rect, 0.0, color(scheme.bar));
    }
    let (icon, tone, _) = mark_icon(scheme, mark);
    if let Some(image) = icons::image(icon, tone, 14.0) {
        image.paint_at(ui, Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), Vec2::splat(14.0)));
    }
    let name_color = if chosen { scheme.accent } else { scheme.fg };
    let mut right = rect.right() - 8.0;
    // The maker is cut first when the row is narrow; the name keeps at least half.
    let right_text =
        [spec, item.manufacturer.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
    if !right_text.is_empty() {
        let small = egui::TextStyle::Small.resolve(ui.style());
        let room = ((rect.width() - 40.0) / 2.0).max(40.0);
        let mut job = LayoutJob::simple_singleline(right_text, small, color(scheme.sub));
        job.wrap = TextWrapping::truncate_at_width(room);
        let galley = ui.painter().layout_job(job);
        right -= galley.size().x;
        ui.painter().galley(egui::pos2(right, rect.center().y - galley.size().y / 2.0), galley, color(scheme.sub));
        right -= 8.0;
    }
    let left = rect.left() + 32.0;
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = LayoutJob::simple_singleline(item.name.clone(), font, color(name_color));
    job.wrap = TextWrapping::truncate_at_width((right - left).max(20.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(left, rect.center().y - galley.size().y / 2.0), galley, color(name_color));
    widgets::focus_ring(ui, &response, scheme, egui::CornerRadius::ZERO);
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hover)
}

/// The heading of a subcategory.
fn heading(ui: &Ui, scheme: &Scheme, rect: Rect, title: String) {
    if let Some(image) = icons::image("diamond", scheme.accent, 10.0) {
        image.paint_at(ui, Rect::from_center_size(rect.left_center() + vec2(7.0, 0.0), Vec2::splat(10.0)));
    }
    let font = crate::fonts::bold(egui::TextStyle::Body.resolve(ui.style()).size);
    let mut job = LayoutJob::simple_singleline(title, font, color(scheme.fg));
    job.wrap = TextWrapping::truncate_at_width((rect.width() - 20.0).max(20.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color(scheme.fg),
    );
}

/// The list side of the page in `height` points: filters, the material lines
/// of the search and the list. A click on an item chooses it.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, marks: &[Have], height: f32) {
    let top = ui.cursor().min.y;
    filters(ui, scheme, state);
    let (shown, material_hits) = matching(state, marks);
    ui.horizontal_wrapped(|row| {
        let mut place_line = String::new();
        if let Some(category) = state.picks.category {
            place_line = state.names.path(Place { category, sub: state.picks.sub });
        }
        if !place_line.is_empty() {
            row.label(RichText::new(place_line).color(color(scheme.accent)));
        }
        row.label(sub(scheme, tf("s_fb_treffer", &[("n", Arg::from(shown.len()))])));
    });
    ship_line(ui, scheme, state, marks, &shown);
    for (material, names) in material_hits.iter().take(3) {
        let line = if names.is_empty() {
            tn("s_he_aus_keine", &[Arg::from(material.as_str())])
        } else if names.len() == 1 {
            tn("s_p9_aus_eins", &[Arg::from(material.as_str())])
        } else {
            tn("s_he_aus", &[Arg::from(material.as_str()), Arg::from(names.len())])
        };
        ui.label(sub(scheme, line));
    }
    let used = ui.cursor().min.y - top;
    let room = (height - used - 12.0).max(super::list_min(ui));
    if shown.is_empty() {
        // Without the slots of the ship its line says why the list is empty, not "nothing found".
        if material_hits.is_empty() && state.ship_fit.0 != ShipFit::NoData {
            widgets::empty_state(ui, scheme, "scan-search", &t("s_he_nichts"));
        }
        return;
    }
    list_area(ui, scheme, state, marks, &shown, room);
}

/// The scrolling list: only the lines in sight are drawn.
fn list_area(ui: &mut Ui, scheme: &Scheme, state: &mut CraftingState, marks: &[Have], shown: &[usize], room: f32) {
    let mut clicked = None;
    widgets::list_frame_marked(ui, scheme, |frame| {
        frame.set_width(frame.available_width());
        let width = frame.available_width() - widgets::SCROLL_WIDTH - 4.0;
        let (tile_w, tile_h, row_h, head_h) =
            (tile_width(frame), tile_height(frame), row_height(frame), head_height(frame));
        let per_line = match state.view {
            View::Grid => (((width + TILE_GAP) / (tile_w + TILE_GAP)).floor() as usize).max(1),
            View::List => 1,
        };
        // The tiles share the width of the line.
        let tile_w = match state.view {
            View::Grid => ((width - TILE_GAP * (per_line as f32 - 1.0)) / per_line as f32).floor(),
            View::List => width,
        };
        let all = lines(&state.places, shown, per_line);
        let height_of = |line: &Line| match (line, state.view) {
            (Line::Head(_), _) => head_h,
            (Line::Items(_), View::Grid) => tile_h + TILE_GAP,
            (Line::Items(_), View::List) => row_h,
        };
        let mut tops = Vec::with_capacity(all.len());
        let mut total = 0.0;
        for line in &all {
            tops.push(total);
            total += height_of(line);
        }
        let mut area = egui::ScrollArea::vertical()
            .id_salt("herstellung-liste")
            .max_height(room)
            .min_scrolled_height(room)
            .auto_shrink([false, false]);
        // A jump from another page brings the chosen item into sight once.
        if let Some(wanted) = state.scroll_to.take()
            && let Some(at) = all.iter().position(|l| matches!(l, Line::Items(items) if items.contains(&wanted)))
        {
            let head = if at > 0 && matches!(all[at - 1], Line::Head(_)) { head_h } else { 0.0 };
            area = area.vertical_scroll_offset((tops[at] - head).max(0.0));
        }
        let output = scroll_viewport(frame, scheme, area, |inner, viewport| {
            inner.set_height(total);
            inner.set_width(width);
            let origin = inner.max_rect().min;
            let mut striped = 0usize;
            for (n, line) in all.iter().enumerate() {
                let h = height_of(line);
                if let Line::Items(_) = line {
                    striped += 1;
                }
                if tops[n] + h < viewport.min.y || tops[n] > viewport.max.y {
                    continue;
                }
                let rect = Rect::from_min_size(origin + vec2(0.0, tops[n]), vec2(width, h));
                match line {
                    Line::Head(place) => heading(inner, scheme, rect, state.names.path(*place)),
                    Line::Items(items) => {
                        for (k, &index) in items.iter().enumerate() {
                            let item = &state.book.items[index];
                            let chosen = state.open == Some(index);
                            let spec = state.specs.get(index).map_or("", String::as_str);
                            let response = match state.view {
                                View::Grid => {
                                    let at = rect.min + vec2(k as f32 * (tile_w + TILE_GAP), 0.0);
                                    let place = Rect::from_min_size(at, vec2(tile_w, tile_h));
                                    tile(inner, scheme, place, item, spec, marks[index], chosen)
                                }
                                View::List => row(inner, scheme, rect, item, spec, marks[index], striped, chosen),
                            };
                            if response.clicked() {
                                clicked = Some(index);
                            }
                        }
                    }
                }
            }
        });
        // What sticks out of the list is cut by it: the edge check of the page cannot see that.
        state.list_overflow = state.list_overflow.max((output.content_size.x - output.inner_rect.width()).max(0.0));
    });
    if let Some(index) = clicked {
        state.choose(index);
    }
}

/// [`widgets::scroll_area`] for a list that draws only what is in sight: the
/// same scroll bar, the closure gets the part in sight as well.
fn scroll_viewport<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    area: egui::ScrollArea,
    add_contents: impl FnOnce(&mut Ui, Rect) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let content_visuals = ui.visuals().clone();
    let radius = egui::CornerRadius::same(if scheme.square { 0 } else { (widgets::SCROLL_WIDTH / 2.0) as u8 });
    ui.scope(|ui| {
        ui.spacing_mut().scroll = widgets::scroll_style();
        let v = ui.visuals_mut();
        v.extreme_bg_color = color(scheme.scroll_groove);
        v.widgets.inactive.bg_fill = color(scheme.scroll_grip);
        v.widgets.hovered.bg_fill = color(scheme.scroll_grip_light);
        v.widgets.active.bg_fill = color(scheme.scroll_grip_light);
        for state in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
            state.corner_radius = radius;
        }
        area.show_viewport(ui, |ui, viewport| {
            *ui.visuals_mut() = content_visuals;
            add_contents(ui, viewport)
        })
    })
    .inner
}
