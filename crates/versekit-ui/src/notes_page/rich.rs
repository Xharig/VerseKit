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

//! The look of a memo in its field, as in a word processor: a bar of buttons
//! above the field (bold, italic, underlined, struck through, bulleted and
//! numbered list, table) that act on the marked text, Ctrl+B / Ctrl+I /
//! Ctrl+U, and the text drawn in its look at once. The field holds the plain
//! text only; no sign of the look is ever typed into it (the cells of a table
//! are parted by tabs, which show as room between its columns).

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Align2, Color32, CursorIcon, FontId, Key, Modifiers, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use versekit_core::language::{Arg, t, tf};
use versekit_core::memo::format::{self, BOLD, Formatting, ITALIC, LineKind, STRIKE, UNDERLINE};
use versekit_core::memo::table;
use versekit_core::theme::{IconTint, Scheme};

use crate::color;
use crate::widgets::{self, ButtonKind};

/// What a button of the bar or a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Gives the marked letters a style, or takes it from them where all have it.
    Style(u8),
    /// Puts the marked lines into a list of that kind, or takes them out where all stand in it.
    Lines(LineKind),
    /// Changes the table at the cursor, or puts one there.
    Table(TableOp),
}

/// What the buttons of a table do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableOp {
    /// Puts a table below the cursor; in a table, makes its rows plain lines again (the words stay).
    Toggle,
    AddRow,
    RemoveRow,
    AddColumn,
    RemoveColumn,
}

/// A button of the bar: what it does, its icon, the key of its hover text and the letter that does the same
/// with Ctrl.
pub(crate) type BarButton = (Action, &'static str, &'static str, Option<Key>);

/// The buttons of the bar.
pub(crate) const BUTTONS: [BarButton; 7] = [
    (Action::Style(BOLD), "bold", "s_nz_fett", Some(Key::B)),
    (Action::Style(ITALIC), "italic", "s_nz_kursiv", Some(Key::I)),
    (Action::Style(UNDERLINE), "underline", "s_nz_unterstrichen", Some(Key::U)),
    (Action::Style(STRIKE), "strikethrough", "s_nz_durchgestrichen", None),
    (Action::Lines(LineKind::Bullet), "list", "s_nz_aufzaehlung", None),
    (Action::Lines(LineKind::Number), "list-ordered", "s_nz_nummerierung", None),
    (Action::Table(TableOp::Toggle), "table", "s_nz_tabelle", None),
];

/// The buttons that stand after [`BUTTONS`] while the cursor is in a table.
pub(crate) const TABLE_BUTTONS: [BarButton; 4] = [
    (Action::Table(TableOp::AddRow), "between-horizontal-end", "s_nz_zeile_neu", None),
    (Action::Table(TableOp::RemoveRow), "fold-vertical", "s_nz_zeile_weg", None),
    (Action::Table(TableOp::AddColumn), "between-vertical-end", "s_nz_spalte_neu", None),
    (Action::Table(TableOp::RemoveColumn), "fold-horizontal", "s_nz_spalte_weg", None),
];

/// What an action did to the open memo.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Only the styles of the next letters changed.
    Typing,
    /// The look changed, the text stayed.
    Look(Formatting),
    /// The cursor goes to this place, nothing else changed.
    Moved(usize),
    /// Text and look changed together, the cursor goes to the place given.
    Rebuilt(String, Formatting, usize),
}

/// Where the cursor of the field stood when it was last seen, and the styles chosen for the next letters.
#[derive(Clone, Debug, Default)]
pub(crate) struct Caret {
    /// The marked letters, from the first to after the last; empty where only the cursor stands.
    pub selection: Range<usize>,
    /// Styles chosen with nothing marked: the next letters typed get them.
    pub pending: Option<u8>,
}

impl Caret {
    /// The styles the next letter typed gets.
    pub fn typing(&self, look: &Formatting) -> u8 {
        self.pending.unwrap_or_else(|| look.style_before(self.selection.start))
    }

    /// Whether the button of `action` stands pressed: all marked letters (or the next one typed) have its
    /// style, all marked lines stand in its list.
    pub fn is_active(&self, action: Action, text: &str, look: &Formatting) -> bool {
        match action {
            Action::Style(style) if self.selection.is_empty() => self.typing(look) & style != 0,
            Action::Style(style) => look.all_have(self.selection.clone(), style),
            Action::Lines(kind) => look.all_lines(lines_of(text, &self.selection), kind),
            Action::Table(_) => self.in_table(text, look),
        }
    }

    /// Whether the cursor stands in a table.
    pub fn in_table(&self, text: &str, look: &Formatting) -> bool {
        table::in_table(look, text, self.selection.start)
    }
}

/// The lines the letters of `selection` stand on.
fn lines_of(text: &str, selection: &Range<usize>) -> Range<usize> {
    format::line_of(text, selection.start)..format::line_of(text, selection.end) + 1
}

/// Does `action` on the marked part of `text`; `None` where it does nothing there.
pub(crate) fn apply(action: Action, text: &str, look: &Formatting, caret: &mut Caret) -> Option<Outcome> {
    let mut changed = look.clone();
    match action {
        Action::Style(style) if caret.selection.is_empty() => {
            caret.pending = Some(caret.typing(look) ^ style);
            return Some(Outcome::Typing);
        }
        Action::Style(style) => {
            let on = !look.all_have(caret.selection.clone(), style);
            changed.set_style(caret.selection.clone(), style, on);
        }
        Action::Lines(kind) => {
            let lines = lines_of(text, &caret.selection);
            let kind = if look.all_lines(lines.clone(), kind) { LineKind::Plain } else { kind };
            changed.set_lines(lines, kind);
        }
        Action::Table(TableOp::Toggle) if caret.in_table(text, look) => {
            let rows = table::rows_around(look, text, format::line_of(text, caret.selection.start))?;
            changed.set_lines(rows, LineKind::Plain);
        }
        Action::Table(op) => {
            let mut text = text.to_string();
            let at = caret.selection.start;
            let cursor = match op {
                TableOp::Toggle => table::insert(&mut text, &mut changed, at),
                TableOp::AddRow => table::add_row(&mut text, &mut changed, at),
                TableOp::RemoveRow => table::remove_row(&mut text, &mut changed, at),
                TableOp::AddColumn => table::add_column(&mut text, &mut changed, at),
                TableOp::RemoveColumn => table::remove_column(&mut text, &mut changed, at),
            }?;
            return Some(Outcome::Rebuilt(text, changed, cursor));
        }
    }
    Some(Outcome::Look(changed))
}

/// The keys that act differently in a table, taken from the keys of this frame before the field sees them:
/// Tab and Shift+Tab go from cell to cell (Tab after the last cell adds a row), Enter adds a row below,
/// Backspace and Delete do not join two cells. `None` where none of them was pressed in a table.
pub(crate) fn table_keys(ui: &Ui, text: &str, look: &Formatting, caret: &Caret) -> Option<Outcome> {
    if !caret.selection.is_empty() || !caret.in_table(text, look) {
        return None;
    }
    let at = caret.selection.start;
    let (mut text, mut look) = (text.to_string(), look.clone());
    let pressed = |modifiers: Modifiers, key: Key| ui.input_mut(|input| input.consume_key(modifiers, key));
    if pressed(Modifiers::SHIFT, Key::Tab) {
        return table::next_cell(&mut text, &mut look, at, true).map(Outcome::Moved);
    }
    if pressed(Modifiers::NONE, Key::Tab) {
        let cursor = table::next_cell(&mut text, &mut look, at, false)?;
        return Some(Outcome::Rebuilt(text, look, cursor));
    }
    if pressed(Modifiers::NONE, Key::Enter) {
        let cursor = table::add_row(&mut text, &mut look, at)?;
        return Some(Outcome::Rebuilt(text, look, cursor));
    }
    if (table::backspace_joins_cells(&text, &look, at) && pressed(Modifiers::NONE, Key::Backspace))
        || (table::delete_joins_cells(&text, &look, at) && pressed(Modifiers::NONE, Key::Delete))
    {
        return Some(Outcome::Moved(at));
    }
    None
}

/// The modifier of the keys as the keyboard of this system names it.
fn command_name() -> String {
    if cfg!(target_os = "macos") { "Cmd".to_string() } else { t("s_ei_taste_strg") }
}

/// The hover text of a button: what it does and, where it has one, its key.
pub(crate) fn meaning(key: &str, letter: Option<Key>) -> String {
    match letter {
        Some(letter) => tf(key, &[("0", Arg::from(format!("{}+{}", command_name(), letter.name())))]),
        None => t(key),
    }
}

/// A square button of the bar with its icon; it stands pressed (in the accent) while `active`.
fn bar_button(ui: &mut Ui, scheme: &Scheme, icon: &str, meaning: &str, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(widgets::ICON_BUTTON_SIZE), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, meaning));
    if ui.is_rect_visible(rect) {
        let kind = if active { ButtonKind::Strong } else { ButtonKind::Normal };
        let colors = widgets::button_colors(scheme, kind, response.hovered());
        let stroke = Stroke::new(1.0, color(colors.border));
        let radius = crate::control_radius(scheme);
        ui.painter().rect(rect.shrink(1.0), radius, color(colors.fill), stroke, egui::StrokeKind::Inside);
        let tint = match (active, response.hovered()) {
            (true, _) => colors.text,
            (false, true) => scheme.icon(IconTint::Hover),
            (false, false) => scheme.icon(IconTint::Normal),
        };
        if let Some(image) = crate::icons::image(icon, tint, 16.0) {
            image.paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
        }
    }
    widgets::focus_ring(ui, &response, scheme, crate::control_radius(scheme));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(meaning)
}

/// The bar above the field; the action of the button clicked.
pub(crate) fn toolbar(ui: &mut Ui, scheme: &Scheme, text: &str, look: &Formatting, caret: &Caret) -> Option<Action> {
    let mut clicked = None;
    let mut rects = Vec::new();
    // The rows and columns of a table can be changed while the cursor stands in one.
    let in_table = caret.in_table(text, look);
    let shown: Vec<&BarButton> = BUTTONS.iter().chain(TABLE_BUTTONS.iter().filter(|_| in_table)).collect();
    // The groups (letters, lists, table, its rows and columns) stand apart and wrap as a whole.
    let group_of = |action: &Action| match action {
        Action::Style(_) => 0,
        Action::Lines(_) => 1,
        Action::Table(TableOp::Toggle) => 2,
        Action::Table(_) => 3,
    };
    const GAP: f32 = 4.0;
    const GROUP_GAP: f32 = 12.0;
    let width_of = |group: &[&BarButton]| group.len() as f32 * widgets::ICON_BUTTON_SIZE + (group.len() - 1) as f32 * GAP;
    // The groups go into rows by their widths, so that a group never breaks apart.
    let room = ui.available_width();
    let mut rows: Vec<Vec<&[&BarButton]>> = Vec::new();
    let mut used = 0.0;
    for group in shown.chunk_by(|a, b| group_of(&a.0) == group_of(&b.0)) {
        match rows.last_mut() {
            Some(row) if used + GROUP_GAP + width_of(group) <= room => {
                used += GROUP_GAP + width_of(group);
                row.push(group);
            }
            _ => {
                used = width_of(group);
                rows.push(vec![group]);
            }
        }
    }
    for row in rows {
        ui.horizontal(|line| {
            line.spacing_mut().item_spacing.x = GAP;
            for (at, group) in row.iter().enumerate() {
                if at > 0 {
                    line.add_space(GROUP_GAP - GAP);
                }
                for (action, icon, key, letter) in group.iter() {
                    let toggles = !matches!(action, Action::Table(op) if *op != TableOp::Toggle);
                    let active = toggles && caret.is_active(*action, text, look);
                    let button = bar_button(line, scheme, icon, &meaning(key, *letter), active);
                    rects.push(button.rect);
                    if button.clicked() {
                        clicked = Some(*action);
                    }
                }
            }
        });
    }
    if cfg!(test) {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(BAR_ID), rects));
    }
    clicked
}

/// Where a test finds the rectangles of the buttons of the bar of the last frame, in the order of [`BUTTONS`]
/// and [`TABLE_BUTTONS`].
pub(crate) const BAR_ID: &str = "notizen-leiste";

/// Takes Ctrl+B, Ctrl+I or Ctrl+U from the keys of this frame (before the field sees them: Ctrl+U would
/// delete to the start of the line there); the action of the one pressed.
pub(crate) fn shortcut(ui: &Ui) -> Option<Action> {
    ui.input_mut(|input| {
        BUTTONS.iter().find_map(|(action, _, _, letter)| {
            letter.filter(|letter| input.consume_key(Modifiers::COMMAND, *letter)).map(|_| *action)
        })
    })
}

/// Backspace at the start of a point of a list takes the line out of the list instead of joining it to the
/// line above (as in a word processor); the new look where it did.
pub(crate) fn backspace_leaves_list(ui: &Ui, text: &str, look: &Formatting, caret: &Caret) -> Option<Formatting> {
    let at = caret.selection.start;
    let line = format::line_of(text, at);
    let listed = matches!(look.line_kind(line), LineKind::Bullet | LineKind::Number);
    if !caret.selection.is_empty() || !format::starts_line(text, at) || !listed {
        return None;
    }
    if !ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Backspace)) {
        return None;
    }
    let mut changed = look.clone();
    changed.set_lines(line..line + 1, LineKind::Plain);
    Some(changed)
}

/// Marks `selection` in the field `id` and gives it the cursor, so that after a click on the bar the same
/// text stays marked and typing goes on in the field.
pub(crate) fn restore_selection(ctx: &egui::Context, id: egui::Id, selection: &Range<usize>) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let range = CCursorRange::two(CCursor::new(selection.start), CCursor::new(selection.end));
    state.cursor.set_char_range(Some(range));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

/// The fonts and colours of the field.
pub(crate) struct Fonts {
    /// The line that names the memo.
    pub title: FontId,
    pub body: FontId,
    pub bold: FontId,
    pub title_color: Color32,
    pub body_color: Color32,
    /// The lines of a table.
    pub grid: Color32,
}

impl Fonts {
    /// How far the points of a list stand in, with their bullet or number in front.
    pub fn indent(&self) -> f32 {
        (self.body.size * 1.8).round()
    }

    /// The room between the words of two cells of a table, and around them.
    fn pad(&self) -> f32 {
        (self.body.size * 0.8).round()
    }
}

/// A table as it was laid out: its lines and where the borders of its columns stand (from the left edge of
/// the text, the outer ones included).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TableShape {
    pub lines: Range<usize>,
    pub borders: Vec<f32>,
}

/// One line of the text: its bytes and the place of its first letter.
#[derive(Clone)]
struct TextLine {
    bytes: Range<usize>,
    letter: usize,
}

fn text_lines(text: &str) -> Vec<TextLine> {
    let (mut found, mut byte, mut letter) = (Vec::new(), 0, 0);
    for line in text.split('\n') {
        found.push(TextLine { bytes: byte..byte + line.len(), letter });
        byte += line.len() + 1;
        letter += line.chars().count() + 1;
    }
    found
}

/// The cells of a line of a table, as lines of their own.
fn cells(text: &str, line: &TextLine) -> Vec<TextLine> {
    let (mut found, mut byte, mut letter) = (Vec::new(), line.bytes.start, line.letter);
    for cell in text[line.bytes.clone()].split('\t') {
        found.push(TextLine { bytes: byte..byte + cell.len(), letter });
        byte += cell.len() + 1;
        letter += cell.chars().count() + 1;
    }
    found
}

/// Writes the sections of a text in its look into a layout job.
struct Writer<'a> {
    text: &'a str,
    look: &'a Formatting,
    title_end: usize,
    fonts: &'a Fonts,
    /// The height of a row of a table, a little taller than a line of text.
    table_row: f32,
}

impl Writer<'_> {
    fn format(&self, in_title: bool, style: u8, table: bool) -> TextFormat {
        let fonts = self.fonts;
        let (font, colour) = match (in_title, style & BOLD != 0) {
            (true, _) => (fonts.title.clone(), fonts.title_color),
            (false, true) => (fonts.bold.clone(), fonts.body_color),
            (false, false) => (fonts.body.clone(), fonts.body_color),
        };
        let mut format = TextFormat::simple(font, colour);
        format.italics = style & ITALIC != 0;
        if style & UNDERLINE != 0 {
            format.underline = Stroke::new(1.0, colour);
        }
        if style & STRIKE != 0 {
            format.strikethrough = Stroke::new(1.0, colour);
        }
        if table {
            format.line_height = Some(self.table_row);
            format.valign = egui::Align::Center;
        }
        format
    }

    /// The letters of `piece` in their styles, the first section with `lead` points of room before it.
    fn piece(&self, job: &mut LayoutJob, piece: &TextLine, lead: f32, table: bool) {
        let (look, title_end) = (self.look, self.title_end);
        if piece.bytes.is_empty() {
            if lead != 0.0 || table {
                let at = piece.bytes.start;
                job.append("", lead, self.format(at < title_end, look.style_at(piece.letter), table));
            }
            return;
        }
        let (mut from, mut from_letter, mut lead) = (piece.bytes.start, piece.letter, lead);
        for (letter, (offset, _)) in self.text[piece.bytes.clone()].char_indices().enumerate() {
            let (byte, letter) = (piece.bytes.start + offset, piece.letter + letter);
            if byte > from && (look.style_at(letter) != look.style_at(letter - 1) || byte == title_end) {
                let format = self.format(from < title_end, look.style_at(from_letter), table);
                job.append(&self.text[from..byte], lead, format);
                (from, from_letter, lead) = (byte, letter, 0.0);
            }
        }
        let format = self.format(from < title_end, look.style_at(from_letter), table);
        job.append(&self.text[from..piece.bytes.end], lead, format);
    }

    /// How wide `piece` is laid out on its own.
    fn width(&self, ui: &Ui, piece: &TextLine) -> f32 {
        let mut job = LayoutJob::default();
        self.piece(&mut job, piece, 0.0, true);
        ui.fonts_mut(|fonts| fonts.layout_job(job)).size().x
    }
}

/// The text laid out in its look: the title (the text up to `title_end`, in bytes) as a heading, the
/// letters in their styles, the points of a list moved in, the cells of a table in columns (their borders
/// go into `tables`). `look` belongs to `old`; where the field already holds the next text, the look
/// follows it for this frame.
#[expect(clippy::too_many_arguments, reason = "the layouter of the field hands over all of it every frame")]
pub(crate) fn layout(
    ui: &Ui,
    text: &str,
    old: &str,
    look: &Formatting,
    title_end: usize,
    fonts: &Fonts,
    wrap: f32,
    tables: &RefCell<Vec<TableShape>>,
) -> Arc<egui::Galley> {
    let followed;
    let look = if text == old {
        look
    } else {
        let mut next = look.clone();
        next.follow_edit(old, text, None, None);
        followed = next;
        &followed
    };
    let (indent, pad) = (fonts.indent(), fonts.pad());
    let table_row = (ui.fonts_mut(|f| f.row_height(&fonts.body)) * 1.5).round();
    let writer = Writer { text, look, title_end, fonts, table_row };
    let lines = text_lines(text);
    let plain = TextFormat::simple(fonts.body.clone(), fonts.body_color);
    let tab = {
        let mut job = LayoutJob::default();
        job.append("\t", 0.0, plain.clone());
        ui.fonts_mut(|f| f.layout_job(job)).size().x
    };

    // Where the cells of each table start (each column as wide as its widest cell), and how wide the cells
    // of each row are.
    let mut shapes = Vec::new();
    let mut rows_of: Vec<Option<(Vec<f32>, Vec<f32>)>> = vec![None; lines.len()];
    let mut line = 0;
    while line < lines.len() {
        if look.line_kind(line) != LineKind::Table {
            line += 1;
            continue;
        }
        let first = line;
        while line < lines.len() && look.line_kind(line) == LineKind::Table {
            line += 1;
        }
        let widths: Vec<Vec<f32>> =
            (first..line).map(|l| cells(text, &lines[l]).iter().map(|c| writer.width(ui, c)).collect()).collect();
        let columns = widths.iter().map(Vec::len).max().unwrap_or(1);
        let widest: Vec<f32> =
            (0..columns).map(|c| widths.iter().filter_map(|row| row.get(c)).fold(0.0, |a: f32, b| a.max(*b))).collect();
        let mut starts = vec![pad * 0.5];
        for c in 1..columns {
            starts.push(starts[c - 1] + widest[c - 1] + tab + pad);
        }
        let mut borders = vec![0.0];
        borders.extend(starts.iter().skip(1).map(|start| start - pad * 0.5));
        borders.push(starts[columns - 1] + widest[columns - 1] + pad * 0.5);
        shapes.push(TableShape { lines: first..line, borders });
        for (row, l) in (first..line).enumerate() {
            rows_of[l] = Some((starts.clone(), widths[row].clone()));
        }
    }

    let mut job = LayoutJob::default();
    for (number, text_line) in lines.iter().enumerate() {
        match (look.line_kind(number), &rows_of[number]) {
            (LineKind::Table, Some((starts, widths))) => {
                for (c, cell) in cells(text, text_line).iter().enumerate() {
                    // The room after the cell before reaches to the start of this column.
                    let lead = match c {
                        0 => pad * 0.5,
                        _ => starts[c] - (starts[c - 1] + widths[c - 1] + tab),
                    };
                    writer.piece(&mut job, cell, lead.max(0.0), true);
                    if cell.bytes.end < text_line.bytes.end {
                        let mut format = plain.clone();
                        format.line_height = Some(table_row);
                        job.append("\t", 0.0, format);
                    }
                }
            }
            (LineKind::Bullet | LineKind::Number, _) => writer.piece(&mut job, text_line, indent, false),
            _ => writer.piece(&mut job, text_line, 0.0, false),
        }
        if number + 1 < lines.len() {
            job.append("\n", 0.0, plain.clone());
        }
    }
    job.wrap.max_width = wrap;
    let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    *tables.borrow_mut() = shapes;

    // An empty point of a list or empty row of a table has no letter to stand in with, so its row is moved
    // in itself: the cursor then stands behind the bullet, not in front of it.
    let mut empty_points = Vec::new();
    let mut line = 0;
    for (at, row) in galley.rows.iter().enumerate() {
        let room = match look.line_kind(line) {
            LineKind::Plain => 0.0,
            LineKind::Bullet | LineKind::Number => indent,
            LineKind::Table => pad * 0.5,
        };
        if row.glyphs.is_empty() && room > 0.0 {
            empty_points.push((at, room));
        }
        if row.ends_with_newline {
            line += 1;
        }
    }
    if !empty_points.is_empty() {
        let galley = Arc::make_mut(&mut galley);
        for (at, room) in empty_points {
            galley.rows[at].pos.x += room;
        }
    }
    galley
}

/// Draws the lines of the tables of `galley`, which stands at `at`.
pub(crate) fn paint_tables(ui: &Ui, galley: &egui::Galley, at: Pos2, clip: Rect, tables: &[TableShape], fonts: &Fonts) {
    if tables.is_empty() {
        return;
    }
    // The top and bottom of each line.
    let mut spans: Vec<(f32, f32)> = Vec::new();
    let mut fresh = true;
    for row in &galley.rows {
        let (top, bottom) = (row.pos.y, row.pos.y + row.size.y);
        if fresh {
            spans.push((top, bottom));
        } else if let Some(last) = spans.last_mut() {
            last.1 = last.1.max(bottom);
        }
        fresh = row.ends_with_newline;
    }
    let painter = ui.painter_at(clip.expand(1.0));
    let stroke = Stroke::new(1.0, fonts.grid);
    for table in tables {
        let (Some(first), Some(last)) = (spans.get(table.lines.start), spans.get(table.lines.end - 1)) else {
            continue;
        };
        let (left, right) = (at.x + table.borders[0], at.x + table.borders.last().copied().unwrap_or(0.0));
        let (top, bottom) = (at.y + first.0, at.y + last.1);
        for line in table.lines.clone() {
            let y = at.y + spans[line].0;
            painter.hline(left..=right, y, stroke);
        }
        painter.hline(left..=right, bottom, stroke);
        for border in &table.borders {
            painter.vline(at.x + border, top..=bottom, stroke);
        }
    }
}

/// Draws the bullet or the number in front of each point of a list of `galley`, which stands at `at`.
pub(crate) fn paint_markers(ui: &Ui, galley: &egui::Galley, at: Pos2, clip: Rect, look: &Formatting, fonts: &Fonts) {
    let painter = ui.painter_at(clip.expand(1.0));
    let indent = fonts.indent();
    let (mut line, mut first_row) = (0, true);
    for row in &galley.rows {
        if first_row {
            let middle = at.y + row.pos.y + row.size.y * 0.5;
            match look.line_kind(line) {
                LineKind::Plain | LineKind::Table => {}
                LineKind::Bullet => {
                    let centre = Pos2::new(at.x + indent * 0.45, middle);
                    painter.circle_filled(centre, (fonts.body.size * 0.17).max(2.0), fonts.body_color);
                }
                LineKind::Number => {
                    let number = look.number_of(line).unwrap_or(1);
                    let right = Pos2::new(at.x + indent - fonts.body.size * 0.35, middle);
                    painter.text(right, Align2::RIGHT_CENTER, format!("{number}."), fonts.body.clone(), fonts.body_color);
                }
            }
        }
        first_row = row.ends_with_newline;
        if row.ends_with_newline {
            line += 1;
        }
    }
}
