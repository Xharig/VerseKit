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

//! Tiles that every user arranges for himself, on every page: fixed
//! grid sizes, moved by dragging the whole tile, hidden and shown again.
//! The column count follows the width, so nothing is cut off.

use egui::{CursorIcon, Key, Modifiers, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use versekit_core::theme::{Color, IconTint, Scheme};

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// Narrowest column of the grid.
pub const TILE_MIN_WIDTH: f32 = 220.0;
/// Height of one grid row; a row grows where a tile in it needs more for its text.
pub const TILE_ROW_HEIGHT: f32 = 110.0;

/// A step in the size of a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Wider,
    Narrower,
    Taller,
    Shorter,
}

/// The fixed sizes of a tile, in grid cells (columns × rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileSize {
    Small,
    Wide,
    Tall,
    Large,
}

impl TileSize {
    pub const ALL: [TileSize; 4] = [TileSize::Small, TileSize::Wide, TileSize::Tall, TileSize::Large];

    /// One grid step in a direction (the arrow keys): a column or a row more or less, within 1 × 1 to 2 × 2.
    pub fn step(self, direction: Step) -> TileSize {
        let (columns, rows) = self.span();
        let (columns, rows) = match direction {
            Step::Wider => (2, rows),
            Step::Narrower => (1, rows),
            Step::Taller => (columns, 2),
            Step::Shorter => (columns, 1),
        };
        match (columns, rows) {
            (1, 1) => TileSize::Small,
            (_, 1) => TileSize::Wide,
            (1, _) => TileSize::Tall,
            _ => TileSize::Large,
        }
    }

    /// Columns and rows the tile takes.
    pub fn span(self) -> (usize, usize) {
        match self {
            TileSize::Small => (1, 1),
            TileSize::Wide => (2, 1),
            TileSize::Tall => (1, 2),
            TileSize::Large => (2, 2),
        }
    }
}

/// One tile of a board: its key (what the page draws in it), size and
/// whether the user hid it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub key: String,
    pub size: TileSize,
    pub hidden: bool,
    /// Pinned by the user from a page: its X loosens it (the tile goes),
    /// the X of other tiles only hides them.
    pub pinned: bool,
}

impl Tile {
    pub fn new(key: &str, size: TileSize) -> Self {
        Tile { key: key.to_string(), size, hidden: false, pinned: false }
    }

    /// A page pinned to the overview.
    pub fn pinned(key: &str) -> Self {
        Tile { pinned: true, ..Tile::new(key, TileSize::Small) }
    }
}

/// Where each tile goes: (column, row) of its top left cell, in order,
/// each at the first free place from the top left. A tile wider than the
/// grid takes the whole width.
pub fn layout(spans: &[(usize, usize)], columns: usize) -> Vec<(usize, usize)> {
    let columns = columns.max(1);
    let mut taken: Vec<Vec<bool>> = Vec::new();
    let mut places = Vec::with_capacity(spans.len());
    for &(width, height) in spans {
        let (width, height) = (width.clamp(1, columns), height.max(1));
        let mut row = 0;
        let place = loop {
            while taken.len() < row + height {
                taken.push(vec![false; columns]);
            }
            let free = |col: usize| (row..row + height).all(|r| (col..col + width).all(|c| !taken[r][c]));
            if let Some(col) = (0..=columns - width).find(|&col| free(col)) {
                break (col, row);
            }
            row += 1;
        };
        for row in taken.iter_mut().skip(place.1).take(height) {
            row[place.0..place.0 + width].iter_mut().for_each(|cell| *cell = true);
        }
        places.push(place);
    }
    places
}

/// The tiles at `from` and `to` swap places; all others stay.
pub fn swap_tiles(tiles: &mut [Tile], from: usize, to: usize) {
    if from < tiles.len() && to < tiles.len() {
        tiles.swap(from, to);
    }
}

/// Moves the tile at `from` to the end (dropped on a free place).
pub fn move_to_end(tiles: &mut Vec<Tile>, from: usize) {
    if from < tiles.len() {
        let tile = tiles.remove(from);
        tiles.push(tile);
    }
}

/// Texts of a board, from the language file.
pub struct TileTexts<'a> {
    /// Hint of the corner that resizes.
    pub size: &'a str,
    /// Hint of the X of a pinned tile.
    pub unpin: &'a str,
    /// Hint of the hide button.
    pub hide: &'a str,
    /// Names of the sizes in the order of [`TileSize::ALL`], shown while resizing.
    pub sizes: [&'a str; 4],
    /// Label before the hidden tiles.
    pub hidden: &'a str,
    /// Label on the tile it would swap with.
    pub swap: &'a str,
    /// Hint of a tile itself: the keys that work on it.
    pub keys: &'a str,
}

/// The size a tile gets when its bottom right corner is dragged to
/// `pointer` (its top left at `min`): the nearest grid size, at most 2 × 2
/// and never wider than the grid.
pub fn snapped_size(min: egui::Pos2, pointer: egui::Pos2, cell: f32, columns: usize) -> TileSize {
    let span = |distance: f32, step: f32, most: usize| {
        (((distance + GAP) / (step + GAP)).round() as usize).clamp(1, most.max(1))
    };
    let width = span(pointer.x - min.x, cell, columns.min(2));
    let height = span(pointer.y - min.y, TILE_ROW_HEIGHT, 2);
    match (width, height) {
        (1, 1) => TileSize::Small,
        (_, 1) => TileSize::Wide,
        (1, _) => TileSize::Tall,
        _ => TileSize::Large,
    }
}

/// Side of the corner that resizes a tile.
pub const RESIZE_CORNER: f32 = 22.0;

/// Number of columns that fit `width`.
pub fn columns_for(width: f32) -> usize {
    (((width + GAP) / (TILE_MIN_WIDTH + GAP)).floor() as usize).max(1)
}

/// Draws the visible tiles of `tiles` in the grid; `add` draws a tile's
/// content, `title` names a hidden tile. The whole tile can be grabbed and
/// dragged onto another one; its bottom right corner is dragged to resize
/// it (like a window, snapping to the grid on release); over a tile its
/// hide button shows. Returns how far the content of any tile sticks out
/// of it (0 = everything fits; the edge check demands 0).
pub fn board(
    ui: &mut Ui,
    scheme: &Scheme,
    id_salt: &str,
    tiles: &mut Vec<Tile>,
    texts: &TileTexts,
    add: &mut dyn FnMut(&mut Ui, &str),
    title: &dyn Fn(&str) -> String,
) -> f32 {
    let width = ui.available_width();
    let columns = columns_for(width);
    let cell = (width - GAP * (columns - 1) as f32) / columns as f32;
    let visible: Vec<usize> = (0..tiles.len()).filter(|&i| !tiles[i].hidden).collect();
    let spans: Vec<_> = visible.iter().map(|&i| tiles[i].size.span()).collect();
    let places = layout(&spans, columns);
    let rows = places.iter().zip(&spans).map(|((_, r), (_, h))| r + h).max().unwrap_or(0);
    // A row is as high as the grid says or as its tiles needed last frame to show their text in full (4.12
    // lets the row of a tile grow too). Tiles of one row come first, so a taller one grows only its lowest row.
    let needs = ui.make_persistent_id((id_salt, "need"));
    let need_id = |key: &str| needs.with(key);
    let mut row_heights = vec![TILE_ROW_HEIGHT; rows];
    let mut by_height: Vec<usize> = (0..visible.len()).collect();
    by_height.sort_by_key(|&k| spans[k].1);
    for k in by_height {
        let need: f32 = ui.data(|d| d.get_temp(need_id(&tiles[visible[k]].key))).unwrap_or(0.0);
        let (row, h) = (places[k].1, spans[k].1);
        let have = row_heights[row..row + h].iter().sum::<f32>() + GAP * (h - 1) as f32;
        if need > have {
            row_heights[row + h - 1] += need - have;
        }
    }
    let tops: Vec<f32> = row_heights
        .iter()
        .scan(0.0, |top, height| {
            let at = *top;
            *top += height + GAP;
            Some(at)
        })
        .collect();
    let height = row_heights.iter().sum::<f32>() + rows.saturating_sub(1) as f32 * GAP;
    let (board_rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());

    let drag_id = ui.make_persistent_id((id_salt, "dragging"));
    let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
    let resize_id = ui.make_persistent_id((id_salt, "resizing"));
    let mut resizing: Option<usize> = ui.data(|d| d.get_temp(resize_id));
    let mut drop_on = None;
    let mut action = None;
    let mut overflow: f32 = 0.0;
    for (k, &i) in visible.iter().enumerate() {
        let (col, row) = places[k];
        let (w, h) = (spans[k].0.min(columns), spans[k].1);
        let min = board_rect.min + vec2(col as f32 * (cell + GAP), tops[row]);
        let tile_width = cell * w as f32 + GAP * (w - 1) as f32;
        let size = vec2(tile_width, row_heights[row..row + h].iter().sum::<f32>() + GAP * (h - 1) as f32);
        let rect = Rect::from_min_size(min, size);
        // The size the tile has on the grid: what its content lays itself out in.
        let base = vec2(tile_width, TILE_ROW_HEIGHT * h as f32 + GAP * (h - 1) as f32);
        let need_key = need_id(&tiles[i].key);
        // The whole tile is the handle; widgets inside it are added later and
        // so lie on top: a button stays a button.
        // Click and drag, not drag alone: a widget that senses only dragging
        // starts its drag on the press itself, which would turn a click on the
        // X into moving the tile.
        let grab = ui.interact(rect, ui.make_persistent_id((id_salt, "tile", &tiles[i].key)), Sense::click_and_drag());
        // The name says which tile and what size it has: a change of the size is heard with the next look.
        let name = format!(
            "{}, {}",
            title(&tiles[i].key),
            texts.sizes[TileSize::ALL.iter().position(|s| *s == tiles[i].size).unwrap_or(0)]
        );
        grab.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        // The hint of the tile names its keys; over the X and the corner their own hints show instead.
        let corner = Rect::from_min_max(rect.max - Vec2::splat(RESIZE_CORNER), rect.max);
        let hide_rect = Rect::from_min_size(pos2(rect.right() - 30.0, rect.top() + 6.0), Vec2::splat(24.0));
        let over_button = ui.ctx().pointer_hover_pos().is_some_and(|p| corner.contains(p) || hide_rect.contains(p));
        let grab =
            if over_button || dragging.is_some() || resizing.is_some() { grab } else { grab.on_hover_text(texts.keys) };
        // A click on the tile itself (not on a button in it) opens its page: the caller takes it with
        // [`clicked_tile`].
        if grab.clicked() && dragging.is_none() && resizing.is_none() {
            let key = tiles[i].key.clone();
            ui.data_mut(|d| d.insert_temp(ui.make_persistent_id((id_salt, "clicked")), key));
        }
        // With the keyboard: Tab selects a tile, Enter opens it (a click), Delete hides it (a pinned page is
        // loosened), Shift and the arrow keys change its size, Ctrl and the arrow keys left and right move it
        // one place (Alt and the arrow keys are the history of the window). The ring is painted after the box.
        if grab.has_focus() {
            let pressed = |modifiers: Modifiers, key: Key| ui.input_mut(|input| input.consume_key(modifiers, key));
            if pressed(Modifiers::NONE, Key::Delete) {
                action = Some(Action::Hide(i));
            }
            for (key, step) in [
                (Key::ArrowRight, Step::Wider),
                (Key::ArrowLeft, Step::Narrower),
                (Key::ArrowDown, Step::Taller),
                (Key::ArrowUp, Step::Shorter),
            ] {
                if pressed(Modifiers::SHIFT, key) {
                    let mut size = tiles[i].size.step(step);
                    // On one column nothing gets wider (the mouse snaps the same way).
                    if columns < 2 && size.span().0 > 1 {
                        size = size.step(Step::Narrower);
                    }
                    action = Some(Action::Resize(i, size));
                }
            }
            // The place in the order: the visible tile before or after it.
            for (key, other) in [(Key::ArrowLeft, k.checked_sub(1)), (Key::ArrowRight, Some(k + 1))] {
                if pressed(Modifiers::CTRL, key)
                    && let Some(&to) = other.and_then(|at| visible.get(at))
                {
                    action = Some(Action::Swap(i, to));
                }
            }
        }
        if grab.drag_started() {
            dragging = Some(i);
            // Where the tile was taken, so it hangs at the same spot of the mouse.
            // The press, not where the drag was recognised a few points later.
            let pressed = ui.input(|input| input.pointer.press_origin()).or(grab.interact_pointer_pos());
            let taken = pressed.map_or(rect.size() / 2.0, |p| p - rect.min);
            ui.data_mut(|d| d.insert_temp(drag_id.with("offset"), taken));
        }
        if dragging.is_some_and(|d| d != i) && ui.rect_contains_pointer(rect) {
            drop_on = Some(i);
        }
        if dragging == Some(i) {
            // While dragged, the tile itself hangs at the mouse, not just an
            // outline; where it was, a dashed gap stays.
            gap(ui, scheme, rect);
            let taken: Vec2 = ui.data(|d| d.get_temp(drag_id.with("offset"))).unwrap_or(rect.size() / 2.0);
            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                egui::Area::new(egui::Id::unique((id_salt, "dragged tile")))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pointer - taken)
                    .interactable(false)
                    .show(ui.ctx(), |area| {
                        let floating = Rect::from_min_size(area.cursor().min, rect.size());
                        let mut child = area.new_child(egui::UiBuilder::new().max_rect(floating));
                        child.set_clip_rect(floating.expand(4.0));
                        let _ = tile_card(&mut child, scheme, floating, base, &mut |inner| add(inner, &tiles[i].key));
                        let stroke = Stroke::new(2.0, color(scheme.accent));
                        area.painter().rect_stroke(floating, control_radius(scheme), stroke, StrokeKind::Outside);
                        area.allocate_rect(floating, Sense::hover());
                    });
            }
        } else {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            // Nothing paints beyond the tile, whatever its content wants.
            child.set_clip_rect(rect.intersect(ui.clip_rect()));
            let (over, need, _card) = tile_card(&mut child, scheme, rect, base, &mut |inner| add(inner, &tiles[i].key));
            #[cfg(test)]
            drawn_tiles::note(rect, ui.clip_rect(), _card);
            overflow = overflow.max(over);
            let before: Option<f32> = ui.data(|d| d.get_temp(need_key));
            if before.is_none_or(|b| (b - need).abs() > 0.5) {
                ui.data_mut(|d| d.insert_temp(need_key, need));
                // The rows follow in the next frame.
                ui.ctx().request_repaint();
            }
        }
        if drop_on == Some(i) {
            swap_mark(ui, scheme, rect, texts.swap);
        }
        // The ring of the tile that is selected with the keyboard, over its box (painted before it, the box
        // would cover it).
        if grab.has_focus() && dragging.is_none() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                control_radius(scheme),
                Stroke::new(2.0, color(scheme.accent)),
                StrokeKind::Inside,
            );
        }
        // The bottom right corner resizes, like a window: on top of the tile
        // and its content, so a press there never moves the tile.
        let handle = ui
            .interact(corner, ui.make_persistent_id((id_salt, "resize", &tiles[i].key)), Sense::drag())
            .on_hover_cursor(CursorIcon::ResizeNwSe)
            .on_hover_text(texts.size);
        if handle.drag_started() {
            resizing = Some(i);
        }
        let over = ui.rect_contains_pointer(rect);
        if (over && dragging.is_none()) || resizing == Some(i) {
            // Three short lines in the corner show where to take hold.
            let stroke = Stroke::new(1.5, color(scheme.accent));
            for step in [5.0, 9.0, 13.0] {
                let a = pos2(rect.right() - step, rect.bottom() - 3.0);
                let b = pos2(rect.right() - 3.0, rect.bottom() - step);
                ui.painter().line_segment([a, b], stroke);
            }
        }
        if resizing == Some(i)
            && let Some(pointer) = ui.ctx().pointer_interact_pos()
        {
            let target = snapped_size(rect.min, pointer, cell, columns);
            let (tw, th) = target.span();
            let preview = Rect::from_min_size(
                rect.min,
                vec2(cell * tw as f32 + GAP * (tw - 1) as f32, TILE_ROW_HEIGHT * th as f32 + GAP * (th - 1) as f32),
            );
            let stroke = Stroke::new(2.0, color(scheme.accent));
            ui.painter().rect_stroke(preview, control_radius(scheme), stroke, StrokeKind::Outside);
            let name = texts.sizes[TileSize::ALL.iter().position(|s| *s == target).unwrap_or(0)];
            ui.painter().text(
                preview.right_bottom() - vec2(8.0, 8.0),
                egui::Align2::RIGHT_BOTTOM,
                name,
                egui::TextStyle::Body.resolve(ui.style()),
                color(scheme.accent),
            );
            if !ui.input(|input| input.pointer.any_down()) {
                action = Some(Action::Resize(i, target));
                resizing = None;
            }
        }
        if over && dragging.is_none() && resizing.is_none() {
            let hint = if tiles[i].pinned { texts.unpin } else { texts.hide };
            if corner_button(ui, scheme, hide_rect, (id_salt, "hide", i), "x", hint).clicked() {
                action = Some(Action::Hide(i));
            }
        }
        if (grab.dragged() || grab.hovered()) && !handle.hovered() && resizing.is_none() {
            ui.ctx().set_cursor_icon(if grab.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
    }
    // Dropped on a free place of the board: the tile goes to the end.
    let to_end = dragging.is_some() && drop_on.is_none() && ui.rect_contains_pointer(board_rect);
    if dragging.is_some() && !ui.input(|i| i.pointer.any_down()) {
        match (dragging, drop_on) {
            (Some(from), Some(to)) => action = Some(Action::Swap(from, to)),
            (Some(from), None) if to_end => action = Some(Action::ToEnd(from)),
            _ => {}
        }
        dragging = None;
    }
    if resizing.is_some() && !ui.input(|i| i.pointer.any_down()) && action.is_none() {
        resizing = None;
    }
    ui.data_mut(|d| {
        for (id, state) in [(drag_id, dragging), (resize_id, resizing)] {
            match state {
                Some(i) => {
                    d.insert_temp(id, i);
                }
                None => {
                    d.remove_temp::<usize>(id);
                }
            }
        }
    });
    match action {
        Some(Action::Swap(from, to)) => swap_tiles(tiles, from, to),
        Some(Action::ToEnd(from)) => move_to_end(tiles, from),
        Some(Action::Resize(i, size)) => tiles[i].size = size,
        Some(Action::Hide(i)) if tiles[i].pinned => {
            tiles.remove(i);
        }
        Some(Action::Hide(i)) => tiles[i].hidden = true,
        None => {}
    }

    let hidden: Vec<usize> = (0..tiles.len()).filter(|&i| tiles[i].hidden).collect();
    if !hidden.is_empty() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(texts.hidden).color(color(scheme.sub)));
            for i in hidden {
                if ui.add(widgets::Button::new(&title(&tiles[i].key), ButtonKind::Normal, scheme)).clicked() {
                    tiles[i].hidden = false;
                }
            }
        });
    }
    overflow.max(0.0)
}

/// The tile that was clicked on in the last frame of the board `id_salt` drawn in `ui`, once.
pub fn clicked_tile(ui: &Ui, id_salt: &str) -> Option<String> {
    ui.data_mut(|d| d.remove_temp::<String>(ui.make_persistent_id((id_salt, "clicked"))))
}

/// Room the head of a tile keeps free at the right for the hide button that shows over it.
pub const HIDE_ROOM: f32 = 34.0;

/// Height of the bar of a share in a tile.
pub const BAR_HEIGHT: f32 = 6.0;

/// Room between two segments of a bar drawn in steps.
const STEP_GAP: f32 = 2.0;

/// The least contrast (WCAG 2.x) of the empty part of a bar against the ground of its tile.
pub const TRACK_CONTRAST: f64 = 1.5;

/// The color of the empty part of a bar on a tile: the track of the scheme, moved toward its muted text color in
/// twentieths until it stands apart from the ground of the tile ([`Scheme::surface`]) by [`TRACK_CONTRAST`].
pub fn bar_track(scheme: &Scheme) -> Color {
    (0..=20)
        .map(|step| scheme.track.mix(scheme.sub, f64::from(step) / 20.0))
        .find(|track| track.contrast(scheme.surface) >= TRACK_CONTRAST)
        .unwrap_or(scheme.sub)
}

fn bar_radius(scheme: &Scheme) -> egui::CornerRadius {
    egui::CornerRadius::same(if scheme.square { 0 } else { (BAR_HEIGHT / 2.0) as u8 })
}

/// A thin bar as wide as the room, filled to `share` (0 to 1): the blueprints owned, the watch list found. It is
/// no widget to operate; its figures stand in the text above it.
pub fn share_bar(ui: &mut Ui, scheme: &Scheme, share: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::hover());
    let radius = bar_radius(scheme);
    ui.painter().rect_filled(rect, radius, color(bar_track(scheme)));
    let mut done = rect;
    done.set_width((rect.width() * share.clamp(0.0, 1.0)).max(if share > 0.0 { BAR_HEIGHT } else { 0.0 }));
    ui.painter().rect_filled(done.intersect(rect), radius, color(scheme.accent));
}

/// How many of `steps` segments a share lights.
pub fn lit_steps(share: f32, steps: u8) -> usize {
    ((share.clamp(0.0, 1.0) * f32::from(steps)).round() as usize).min(usize::from(steps))
}

/// The color of segment `at` (from 0) of `steps`, of which `lit` are lit: from red over to the accent along the
/// lit ones, the empty part of a bar for the others.
pub fn step_color(scheme: &Scheme, at: usize, lit: usize, steps: u8) -> Color {
    if at >= lit {
        return bar_track(scheme);
    }
    let share = if steps > 1 { at as f64 / f64::from(steps - 1) } else { 1.0 };
    scheme.red.mix(scheme.accent, share)
}

/// The bar of a share in `steps` segments side by side (the class of the computer of five): the lit ones run
/// from red to the accent, the others have the color of the empty part of a bar.
pub fn step_bar(ui: &mut Ui, scheme: &Scheme, share: f32, steps: u8) {
    let steps = steps.max(1);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::hover());
    let lit = lit_steps(share, steps);
    let width = ((rect.width() - STEP_GAP * f32::from(steps - 1)) / f32::from(steps)).max(1.0);
    let radius = bar_radius(scheme);
    for at in 0..usize::from(steps) {
        let left = rect.left() + at as f32 * (width + STEP_GAP);
        let segment = Rect::from_min_size(pos2(left, rect.top()), vec2(width, BAR_HEIGHT));
        ui.painter().rect_filled(segment, radius, color(step_color(scheme, at, lit, steps)));
    }
}

/// The key line of a tile in `tint`, wrapped and never cut. With a key `figure` the figure stands large and bold
/// (4.12's big number): a line that begins with it keeps it in the same text, any other line follows the figure in
/// its row and wraps below it.
pub fn key_line(ui: &mut Ui, line: &str, figure: Option<&str>, tint: egui::Color32) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let big = crate::fonts::bold(egui::TextStyle::Heading.resolve(ui.style()).size);
    let section = |job: &mut egui::text::LayoutJob, text: &str, font: egui::FontId| {
        job.append(
            text,
            0.0,
            egui::TextFormat { font_id: font, color: tint, valign: egui::Align::BOTTOM, ..Default::default() },
        );
    };
    match figure.filter(|figure| !figure.is_empty()) {
        None => {
            ui.add(egui::Label::new(RichText::new(line).color(tint)).wrap());
        }
        Some(figure) if line.starts_with(figure) => {
            let mut job = egui::text::LayoutJob::default();
            section(&mut job, figure, big);
            section(&mut job, &line[figure.len()..], body);
            ui.add(egui::Label::new(job).wrap());
        }
        Some(figure) => {
            let mut job = egui::text::LayoutJob::default();
            section(&mut job, figure, big);
            ui.horizontal_wrapped(|row| {
                row.add(egui::Label::new(job).wrap());
                row.add(egui::Label::new(RichText::new(line).color(tint)).wrap());
            });
        }
    }
}

enum Action {
    Swap(usize, usize),
    ToEnd(usize),
    Resize(usize, TileSize),
    Hide(usize),
}

/// Room the box of a tile takes from it (margin and frame).
const CARD_INSET: f32 = 26.0;

/// A tile's box in `rect`; its content lays itself out in the room of the grid size `base` and may run
/// lower. Returns how far the content sticks out of `rect`, the height the tile needs for all of it and the place
/// of the box.
fn tile_card(ui: &mut Ui, scheme: &Scheme, rect: Rect, base: Vec2, add: &mut dyn FnMut(&mut Ui)) -> (f32, f32, Rect) {
    let (mut over, mut need): (f32, f32) = (0.0, 0.0);
    let shown = widgets::card(ui, scheme, |inner| {
        // Text in a tile is no text to select: a press on it takes the tile, to move it or to open it.
        inner.style_mut().interaction.selectable_labels = false;
        let room = (rect.size() - Vec2::splat(CARD_INSET)).max(Vec2::ZERO);
        inner.set_max_size(vec2(room.x, (base.y - CARD_INSET).max(0.0)));
        let allowed = Rect::from_min_size(inner.min_rect().min, room);
        add(inner);
        let used = inner.min_rect();
        over = (used.max.x - allowed.max.x).max(used.max.y - allowed.max.y);
        need = used.height() + CARD_INSET;
        // The box fills the tile, also where the row grew for another tile. `set_min_size` would not do: in a
        // top-down layout it measures the height from the cursor, below the content, and the box would run
        // that far past the bottom of the tile. Rounded to the grid of the layout like any widget place.
        inner.expand_to_include_rect(egui::emath::GuiRounding::round_ui(allowed));
    });
    (over.max(0.0), need, shown.response.rect)
}

/// The tiles a run of the tests drew: the place of each tile, the part of the page that shows (the clip of the
/// board) and the place of its box, so a check sees whether the box stands whole in its tile.
#[cfg(test)]
pub(crate) mod drawn_tiles {
    use std::cell::RefCell;

    /// One drawn tile: its place, the visible part of the page it lies in, the place of its box.
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Drawn {
        pub tile: egui::Rect,
        pub visible: egui::Rect,
        pub card: egui::Rect,
    }

    thread_local! {
        static TILES: RefCell<Vec<Drawn>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn note(tile: egui::Rect, visible: egui::Rect, card: egui::Rect) {
        TILES.with(|tiles| tiles.borrow_mut().push(Drawn { tile, visible, card }));
    }

    /// The tiles drawn since the last call (or the start), in the order of drawing.
    pub(crate) fn take() -> Vec<Drawn> {
        TILES.with(|tiles| std::mem::take(&mut *tiles.borrow_mut()))
    }
}

/// Marks the tile the one in hand would swap with: a thick accent frame,
/// a light accent tint and, in the middle, the swap icon with `text`.
fn swap_mark(ui: &Ui, scheme: &Scheme, rect: Rect, text: &str) {
    let painter = ui.painter();
    let tint = color(scheme.accent).gamma_multiply(0.15);
    painter.rect_filled(rect, control_radius(scheme), tint);
    let stroke = Stroke::new(3.0, color(scheme.accent));
    painter.rect_stroke(rect.expand(2.0), control_radius(scheme), stroke, StrokeKind::Outside);
    let font = crate::fonts::bold(egui::TextStyle::Body.resolve(ui.style()).size);
    let galley = painter.layout_no_wrap(text.to_string(), font, color(scheme.bg));
    let badge = Rect::from_center_size(rect.center(), galley.size() + vec2(40.0, 12.0));
    painter.rect_filled(badge, control_radius(scheme), color(scheme.accent));
    if let Some(image) = crate::icons::image("arrow-left-right", scheme.bg, 16.0) {
        let at = Rect::from_center_size(pos2(badge.min.x + 16.0, badge.center().y), Vec2::splat(16.0));
        image.paint_at(ui, at);
    }
    painter.galley(pos2(badge.min.x + 30.0, badge.center().y - galley.size().y / 2.0), galley, color(scheme.bg));
}

/// The dashed gap a dragged tile leaves at its place.
fn gap(ui: &Ui, scheme: &Scheme, rect: Rect) {
    let r = rect.shrink(1.0);
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    let stroke = Stroke::new(1.5, color(scheme.control_border()));
    ui.painter().extend(egui::Shape::dashed_line(&corners, stroke, 8.0, 6.0));
}

/// A small icon button in the corner of a tile, with its meaning on hover.
fn corner_button(
    ui: &mut Ui,
    scheme: &Scheme,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    icon: &str,
    hint: &str,
) -> Response {
    let response = ui.interact(rect, ui.make_persistent_id(id), Sense::click());
    let colors = widgets::button_colors(scheme, ButtonKind::Normal, response.hovered());
    let stroke = Stroke::new(1.0, color(colors.border));
    ui.painter().rect(rect, control_radius(scheme), color(colors.fill), stroke, StrokeKind::Inside);
    let tint = if response.hovered() { IconTint::Hover } else { IconTint::Normal };
    if let Some(image) = crate::icons::themed(icon, scheme, tint, 14.0) {
        image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(14.0)));
    }
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No two tiles share a cell and every tile lies inside the grid.
    fn check(spans: &[(usize, usize)], columns: usize) -> Vec<(usize, usize)> {
        let places = layout(spans, columns);
        let mut cells = std::collections::HashSet::new();
        for (&(col, row), &(w, h)) in places.iter().zip(spans) {
            let w = w.clamp(1, columns);
            assert!(col + w <= columns, "inside the grid");
            for r in row..row + h {
                for c in col..col + w {
                    assert!(cells.insert((c, r)), "cell {c},{r} twice");
                }
            }
        }
        places
    }

    #[test]
    fn tiles_fill_the_grid_from_the_top_left() {
        // small, wide, large, small, tall in four columns:
        //   row 0: S W W S
        //   row 1: L L T .
        //   row 2: L L T .
        let spans = [(1, 1), (2, 1), (2, 2), (1, 1), (1, 2)];
        let places = check(&spans, 4);
        assert_eq!(places, vec![(0, 0), (1, 0), (0, 1), (3, 0), (2, 1)]);
    }

    #[test]
    fn a_narrow_window_stacks_and_nothing_sticks_out() {
        let spans = [(2, 2), (2, 1), (1, 1)];
        let places = check(&spans, 1);
        assert_eq!(places, vec![(0, 0), (0, 2), (0, 3)]);
        for columns in 1..=6 {
            check(&[(1, 1), (2, 1), (1, 2), (2, 2), (1, 1), (2, 1), (1, 1)], columns);
        }
    }

    #[test]
    fn columns_follow_the_width() {
        assert_eq!(columns_for(100.0), 1);
        assert_eq!(columns_for(TILE_MIN_WIDTH * 2.0 + GAP), 2);
        assert_eq!(columns_for(TILE_MIN_WIDTH * 2.0 + GAP - 1.0), 1);
    }

    #[test]
    fn dragging_a_tile_by_its_body_moves_it() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Größe",
            hide: "Ausblenden",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles: Vec<Tile> = ["a", "b", "c"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        let step = |events: Vec<egui::Event>, tiles: &mut Vec<Tile>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let _ = board(
                    ui,
                    scheme,
                    "test",
                    tiles,
                    &texts,
                    &mut |ui, key| {
                        ui.label(key);
                    },
                    &|key| key.to_string(),
                );
            });
            output.textures_delta.clear();
            // Whether the swap partner shows the swap text.
            output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Tauschen"))
        };
        step(vec![], &mut tiles);
        // Four columns of 244 points: grab `a` low in its body, drop on `c`
        // in the third column.
        let (from, to) = (pos2(150.0, 90.0), pos2(620.0, 90.0));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        step(vec![egui::Event::PointerMoved(from)], &mut tiles);
        step(vec![button(from, true)], &mut tiles);
        let mut marked = false;
        for x in [200.0, 400.0, 600.0, 620.0] {
            marked = step(vec![egui::Event::PointerMoved(pos2(x, 90.0))], &mut tiles);
        }
        assert!(marked, "the tile it would swap with says so");
        // While dragged, the tile itself hangs at the mouse where it was taken.
        let floating = ctx.memory(|m| m.area_rect(egui::Id::unique(("test", "dragged tile"))));
        let floating = floating.expect("the dragged tile is drawn at the mouse");
        assert!((floating.min.x - (620.0 - 150.0)).abs() < 2.0, "follows the mouse: {floating:?}");
        assert!(floating.width() > 200.0, "full size, not a dot: {floating:?}");
        step(vec![button(to, false)], &mut tiles);
        step(vec![], &mut tiles);
        let order: Vec<_> = tiles.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(order, ["c", "b", "a"], "a and c swap, b stays");
    }

    #[test]
    fn the_corner_snaps_to_the_nearest_grid_size() {
        let min = pos2(0.0, 0.0);
        let cell = 200.0;
        let snap = |x: f32, y: f32, columns| snapped_size(min, pos2(x, y), cell, columns);
        assert_eq!(snap(200.0, 110.0, 4), TileSize::Small);
        assert_eq!(snap(380.0, 100.0, 4), TileSize::Wide, "most of a second column");
        assert_eq!(snap(250.0, 100.0, 4), TileSize::Small, "a little over is not enough");
        assert_eq!(snap(190.0, 210.0, 4), TileSize::Tall);
        assert_eq!(snap(900.0, 900.0, 4), TileSize::Large, "never more than 2 × 2");
        assert_eq!(snap(900.0, 100.0, 1), TileSize::Small, "one column: never wide");
        assert_eq!(snap(-50.0, -50.0, 4), TileSize::Small);
    }

    #[test]
    fn dragging_the_corner_resizes_and_does_not_move() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles: Vec<Tile> = ["a", "b", "c"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        let step = |events: Vec<egui::Event>, tiles: &mut Vec<Tile>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let _ = board(
                    ui,
                    scheme,
                    "test",
                    tiles,
                    &texts,
                    &mut |ui, key| {
                        ui.label(key);
                    },
                    &|key| key.to_string(),
                );
            });
            output.textures_delta.clear();
        };
        step(vec![], &mut tiles);
        // Tile `a`: 244 × 110 at the top left; its corner is at (244, 110).
        let corner = pos2(240.0, 106.0);
        let target = pos2(480.0, 100.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        step(vec![egui::Event::PointerMoved(corner)], &mut tiles);
        step(vec![button(corner, true)], &mut tiles);
        for x in [300.0, 400.0, 480.0] {
            step(vec![egui::Event::PointerMoved(pos2(x, 100.0))], &mut tiles);
        }
        assert_eq!(tiles[0].size, TileSize::Small, "nothing changes before release");
        step(vec![button(target, false)], &mut tiles);
        step(vec![], &mut tiles);
        assert_eq!(tiles[0].size, TileSize::Wide);
        let order: Vec<_> = tiles.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(order, ["a", "b", "c"], "resizing does not move the tile");
    }

    #[test]
    fn nothing_paints_beyond_its_tile() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles = vec![Tile::new("hoch", TileSize::Small)];
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut tile_bottom = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            let top = ui.cursor().min.y;
            let _ = board(
                ui,
                scheme,
                "test",
                &mut tiles,
                &texts,
                &mut |ui, _| {
                    // Far more content than a 1 × 1 tile holds.
                    for line in 0..30 {
                        ui.label(format!("Zeile {line}"));
                    }
                },
                &|key| key.to_string(),
            );
            tile_bottom = top + TILE_ROW_HEIGHT;
        });
        output.textures_delta.clear();
        let below: Vec<_> = output
            .shapes
            .iter()
            .filter(|s| s.shape.visual_bounding_rect().min.y > tile_bottom + 1.0)
            .filter(|s| s.clip_rect.max.y > tile_bottom + 0.5)
            .collect();
        assert!(below.is_empty(), "{} shapes paint below the tile", below.len());
    }

    #[test]
    fn the_x_hides_a_tile() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles: Vec<Tile> = ["a", "b"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        let step = |events: Vec<egui::Event>, tiles: &mut Vec<Tile>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let _ = board(
                    ui,
                    scheme,
                    "test",
                    tiles,
                    &texts,
                    &mut |ui, key| {
                        ui.label(key);
                    },
                    &|key| key.to_string(),
                );
            });
            output.textures_delta.clear();
        };
        step(vec![], &mut tiles);
        // The X of tile `a` (244 wide): 24 × 24 at right − 30, top + 6.
        let x = pos2(244.0 - 30.0 + 12.0, 6.0 + 12.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: x,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        step(vec![egui::Event::PointerMoved(x)], &mut tiles);
        step(vec![egui::Event::PointerMoved(x)], &mut tiles);
        step(vec![button(true)], &mut tiles);
        step(vec![button(false)], &mut tiles);
        step(vec![], &mut tiles);
        assert!(tiles[0].hidden, "a click on the X hides the tile");
    }

    #[test]
    fn the_keyboard_selects_a_tile_and_resizes_moves_and_hides_it() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles: Vec<Tile> = ["a", "b", "c"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        let press = |key: Key, modifiers: Modifiers, tiles: &mut Vec<Tile>| {
            let event = egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers };
            for events in [vec![event], vec![]] {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 600.0))),
                    events,
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    let _ = board(
                        ui,
                        scheme,
                        "tasten",
                        tiles,
                        &texts,
                        &mut |ui, key| {
                            ui.label(key);
                        },
                        &|key| key.to_string(),
                    );
                });
                output.textures_delta.clear();
            }
        };
        let (none, shift, alt) = (Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL);
        let size_of = |tiles: &[Tile], key: &str| tiles.iter().find(|t| t.key == key).map(|t| t.size);
        // Nothing is selected: the keys change nothing.
        press(Key::ArrowRight, shift, &mut tiles);
        assert_eq!(size_of(&tiles, "a"), Some(TileSize::Small));
        // Tab selects the first tile; Shift and the arrows change its size within the grid.
        press(Key::Tab, none, &mut tiles);
        press(Key::ArrowRight, shift, &mut tiles);
        assert_eq!(size_of(&tiles, "a"), Some(TileSize::Wide));
        press(Key::ArrowDown, shift, &mut tiles);
        assert_eq!(size_of(&tiles, "a"), Some(TileSize::Large));
        press(Key::ArrowLeft, shift, &mut tiles);
        assert_eq!(size_of(&tiles, "a"), Some(TileSize::Tall));
        press(Key::ArrowUp, shift, &mut tiles);
        assert_eq!(size_of(&tiles, "a"), Some(TileSize::Small));
        // Ctrl and the arrows move it one place (Alt and the arrows are the history of the window); the first
        // one cannot go before itself.
        press(Key::ArrowLeft, alt, &mut tiles);
        assert_eq!(tiles.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        press(Key::ArrowRight, alt, &mut tiles);
        assert_eq!(tiles.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(), ["b", "a", "c"]);
        // Delete hides the tile that is selected.
        press(Key::Delete, none, &mut tiles);
        assert!(tiles.iter().find(|t| t.key == "a").is_some_and(|t| t.hidden));
        assert!(tiles.iter().filter(|t| t.key != "a").all(|t| !t.hidden));
    }

    #[test]
    fn on_one_column_the_keys_never_widen_a_tile_and_the_ring_stands_over_the_box() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Aus",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten",
        };
        let mut tiles = vec![Tile::new("a", TileSize::Small)];
        // 200 points: one column.
        let frame = |events: Vec<egui::Event>, tiles: &mut Vec<Tile>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(200.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let _ = board(
                    ui,
                    scheme,
                    "eine-spalte",
                    tiles,
                    &texts,
                    &mut |ui, key| {
                        ui.label(key);
                    },
                    &|key| key.to_string(),
                );
            });
            output.textures_delta.clear();
            output
        };
        let key = |key: Key, modifiers: Modifiers| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(vec![], &mut tiles);
        frame(vec![key(Key::Tab, Modifiers::NONE)], &mut tiles);
        frame(vec![key(Key::ArrowRight, Modifiers::SHIFT)], &mut tiles);
        frame(vec![], &mut tiles);
        assert_eq!(tiles[0].size, TileSize::Small, "nothing gets wider on one column");
        frame(vec![key(Key::ArrowDown, Modifiers::SHIFT)], &mut tiles);
        let output = frame(vec![], &mut tiles);
        assert_eq!(tiles[0].size, TileSize::Tall);
        // The ring of the selected tile comes after the fill of its box in the order of painting.
        fn flatten<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
                other => out.push(other),
            }
        }
        let mut shapes = Vec::new();
        output.shapes.iter().for_each(|clipped| flatten(&clipped.shape, &mut shapes));
        let accent = color(scheme.accent);
        let ring = shapes.iter().rposition(|s| {
            matches!(s, egui::Shape::Rect(r) if r.stroke.width == 2.0 && r.stroke.color == accent && r.fill == egui::Color32::TRANSPARENT)
        });
        let card = shapes.iter().position(|s| matches!(s, egui::Shape::Rect(r) if r.fill == color(scheme.surface)));
        let (ring, card) = (ring.expect("the ring is drawn"), card.expect("the box is drawn"));
        assert!(ring > card, "the ring ({ring}) lies over the box ({card})");
    }

    /// The hint of a tile names its keys; over its X the hint of the X shows instead, not both.
    #[test]
    fn the_hint_of_a_tile_names_its_keys() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let scheme = versekit_core::theme::by_id("original").expect("scheme exists");
        let texts = TileTexts {
            swap: "Tauschen",
            unpin: "Lösen",
            size: "Ecke",
            hide: "Ausblenden",
            sizes: ["K", "B", "H", "G"],
            hidden: "Aus:",
            keys: "Tasten der Kachel",
        };
        let mut tiles: Vec<Tile> = ["a", "b"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        let mut time = 0.0;
        // Rests the mouse at `at` for a few seconds; the texts drawn at the end.
        let mut rest = |at: egui::Pos2, tiles: &mut Vec<Tile>| {
            let mut drawn = Vec::new();
            for step in 0..8 {
                time += 0.5;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, 600.0))),
                    time: Some(time),
                    events: if step == 0 { vec![egui::Event::PointerMoved(at)] } else { Vec::new() },
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    let _ = board(
                        ui,
                        scheme,
                        "hinweis",
                        tiles,
                        &texts,
                        &mut |ui, key| {
                            ui.label(key);
                        },
                        &|key| key.to_string(),
                    );
                });
                output.textures_delta.clear();
                fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                    match shape {
                        egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                        egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                        _ => {}
                    }
                }
                drawn.clear();
                output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut drawn));
            }
            drawn
        };
        // Tile `a`: 244 × 110 at the top left.
        let shown = rest(pos2(100.0, 70.0), &mut tiles);
        assert!(shown.iter().any(|t| t == "Tasten der Kachel"), "over the tile its keys show: {shown:?}");
        let shown = rest(pos2(244.0 - 30.0 + 12.0, 6.0 + 12.0), &mut tiles);
        assert!(shown.iter().any(|t| t == "Ausblenden"), "over the X its hint shows: {shown:?}");
        assert!(!shown.iter().any(|t| t == "Tasten der Kachel"), "and not the keys as well: {shown:?}");
    }

    /// Draws `add` in a frame `width` points wide; the shapes painted, flat.
    fn painted(width: f32, add: impl FnOnce(&mut Ui)) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 600.0))),
            ..Default::default()
        };
        let mut add = Some(add);
        let mut output = ctx.run_ui(input, |ui| {
            if let Some(add) = add.take() {
                add(ui);
            }
        });
        output.textures_delta.clear();
        fn flatten(shape: egui::Shape, out: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, out)),
                other => out.push(other),
            }
        }
        let mut shapes = Vec::new();
        output.shapes.into_iter().for_each(|clipped| flatten(clipped.shape, &mut shapes));
        shapes
    }

    /// The empty part of a bar on a tile stands apart from the ground of the tile by at least 1.5:1 in every
    /// scheme, measured after WCAG; where the track of the scheme does so already, it keeps its color. The bar as
    /// drawn has that color.
    #[test]
    fn the_track_of_a_bar_stands_apart_from_the_ground_of_its_tile_in_every_scheme() {
        let faint: Vec<String> = versekit_core::theme::schemes()
            .iter()
            .filter(|s| bar_track(s).contrast(s.surface) < TRACK_CONTRAST)
            .map(|s| format!("{}: {:.2}:1", s.id, bar_track(s).contrast(s.surface)))
            .collect();
        assert!(faint.is_empty(), "the track hardly shows on the tile: {faint:?}");
        for scheme in versekit_core::theme::schemes() {
            if scheme.track.contrast(scheme.surface) >= TRACK_CONTRAST {
                assert_eq!(bar_track(scheme), scheme.track, "{}: a track that shows keeps its color", scheme.id);
            }
            let shapes = painted(300.0, |ui| share_bar(ui, scheme, 0.5));
            let bars: Vec<&egui::epaint::RectShape> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Rect(rect) if (rect.rect.height() - BAR_HEIGHT).abs() < 0.1 => Some(rect),
                    _ => None,
                })
                .collect();
            let track = bars.iter().find(|r| r.fill == color(bar_track(scheme))).expect("the track is drawn");
            let filled = bars.iter().find(|r| r.fill == color(scheme.accent)).expect("the filled part is drawn");
            assert!((filled.rect.width() / track.rect.width() - 0.5).abs() < 0.02, "{}", scheme.id);
        }
    }

    /// A bar in steps lights as many segments as its share comes to, from red to the accent; the others have the
    /// color of the empty part of a bar.
    #[test]
    fn a_bar_in_steps_lights_its_segments_from_red_to_the_accent() {
        assert_eq!([0.0, 0.2, 0.6, 1.0, 1.5].map(|share| lit_steps(share, 5)), [0, 1, 3, 5, 5]);
        for scheme in versekit_core::theme::schemes() {
            assert_eq!(step_color(scheme, 0, 3, 5), scheme.red, "{}", scheme.id);
            assert_eq!(step_color(scheme, 4, 5, 5), scheme.accent, "{}", scheme.id);
            assert_eq!(step_color(scheme, 3, 3, 5), bar_track(scheme), "{}", scheme.id);
            let shapes = painted(300.0, |ui| step_bar(ui, scheme, 0.6, 5));
            let segments: Vec<(f32, egui::Color32)> = shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Rect(rect) if (rect.rect.height() - BAR_HEIGHT).abs() < 0.1 => {
                        Some((rect.rect.left(), rect.fill))
                    }
                    _ => None,
                })
                .collect();
            let expected: Vec<egui::Color32> = (0..5).map(|at| color(step_color(scheme, at, 3, 5))).collect();
            assert_eq!(segments.iter().map(|(_, fill)| *fill).collect::<Vec<_>>(), expected, "{}", scheme.id);
            assert!(segments.windows(2).all(|pair| pair[1].0 > pair[0].0), "side by side: {segments:?}");
        }
    }

    /// The key figure stands larger and bold at the start of the line it begins, in the same text, so the line
    /// reads as before; before a line that does not begin with it, it stands on its own; without one the line is
    /// plain. Nothing reaches out of a narrow tile.
    #[test]
    fn the_key_figure_stands_large_and_bold_and_the_line_reads_as_before() {
        let texts = |shapes: &[egui::Shape]| -> Vec<std::sync::Arc<egui::Galley>> {
            shapes
                .iter()
                .filter_map(|s| match s {
                    egui::Shape::Text(text) => Some(text.galley.clone()),
                    _ => None,
                })
                .collect()
        };
        let tint = egui::Color32::from_rgb(1, 2, 3);
        let line = "318 von 722 · 44 % der Baupläne, die es im Spiel gibt";
        let shapes = painted(160.0, |ui| key_line(ui, line, Some("318"), tint));
        let galleys = texts(&shapes);
        assert_eq!(galleys.len(), 1, "one text");
        assert_eq!(galleys[0].text(), line);
        let sections = &galleys[0].job.sections;
        assert_eq!(sections.len(), 2, "the figure and the rest of the line in their own type: {sections:?}");
        let (figure, rest) = (&sections[0].format.font_id, &sections[1].format.font_id);
        assert!(figure.size > rest.size, "{figure:?} beside {rest:?}");
        assert_eq!(figure.family, crate::fonts::bold(1.0).family, "bold");
        assert!(sections.iter().all(|s| s.format.color == tint));
        assert!(galleys[0].rows.len() > 1, "wraps in a narrow tile");
        assert!(galleys[0].size().x <= 160.0, "nothing reaches out: {}", galleys[0].size().x);

        let shapes = painted(160.0, |ui| key_line(ui, "Rohstoffe: 8 · Waren: 5 · Items: 4", Some("17"), tint));
        let galleys = texts(&shapes);
        assert_eq!(galleys.iter().map(|g| g.text()).collect::<Vec<_>>(), ["17", "Rohstoffe: 8 · Waren: 5 · Items: 4"]);
        assert!(galleys[0].job.sections[0].format.font_id.size > galleys[1].job.sections[0].format.font_id.size);
        assert!(galleys.iter().all(|g| g.size().x <= 160.0));

        let shapes = painted(160.0, |ui| key_line(ui, line, None, tint));
        let galleys = texts(&shapes);
        assert_eq!(galleys.iter().map(|g| g.text()).collect::<Vec<_>>(), [line]);
        assert_eq!(galleys[0].job.sections[0].format.font_id.family, egui::FontFamily::Proportional);
    }

    #[test]
    fn a_step_in_the_size_stays_within_the_grid() {
        assert_eq!(TileSize::Small.step(Step::Wider), TileSize::Wide);
        assert_eq!(TileSize::Wide.step(Step::Wider), TileSize::Wide);
        assert_eq!(TileSize::Large.step(Step::Narrower), TileSize::Tall);
        assert_eq!(TileSize::Large.step(Step::Shorter), TileSize::Wide);
        assert_eq!(TileSize::Tall.step(Step::Taller), TileSize::Tall);
        assert_eq!(TileSize::Small.step(Step::Narrower), TileSize::Small);
    }

    #[test]
    fn dropped_tiles_swap_and_a_free_place_takes_them_to_the_end() {
        let names = |tiles: &[Tile]| tiles.iter().map(|t| t.key.clone()).collect::<Vec<_>>().join(" ");
        let mut tiles: Vec<Tile> = ["a", "b", "c", "d"].iter().map(|k| Tile::new(k, TileSize::Small)).collect();
        swap_tiles(&mut tiles, 0, 2);
        assert_eq!(names(&tiles), "c b a d", "only the two change places");
        move_to_end(&mut tiles, 0);
        assert_eq!(names(&tiles), "b a d c");
        swap_tiles(&mut tiles, 9, 0);
        move_to_end(&mut tiles, 9);
        assert_eq!(names(&tiles), "b a d c", "a tile that does not exist changes nothing");
    }
}
