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

//! The page "Notes" (new in v5): memo pages of free text. The list of the
//! memos stands at the left (above the text where the window is narrow), the
//! text of the chosen one in a large field next to it. A memo is named by its
//! first line, saves a moment after typing and when it is left, and a deleted
//! one can be taken back for a few seconds. A bar above the text gives it its
//! look (see [`rich`]).

mod rich;

use egui::{RichText, Ui};
use versekit_core::language::{Arg, t, tf};
use versekit_core::memo::{self, Memo, Memos};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::color;
use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind};

/// Seconds after the last keystroke until the text is written.
const SAVE_DELAY: f64 = 0.8;
/// From this width on the list stands beside the text.
const TWO_COLUMNS_FROM: f32 = 560.0;
/// Width of the list beside the text.
const LIST_WIDTH: f32 = 220.0;
/// Height of a row of the list, for the room it gets.
const ROW_HEIGHT: f32 = 28.0;
/// Rows of the list above the text where the window is narrow.
const STACKED_ROWS: f32 = 5.0;
/// Fewest lines the field of the text has.
const TEXT_ROWS: usize = 8;

/// Where a test finds the rectangles of the list and the text field of the last frame.
const BOXES_ID: &str = "notizen-kaesten";

/// What the page keeps: the memos, which one is open, and what is not written yet.
pub struct NotesState {
    /// Where the memos are kept; without it (the preview) they live in the window only.
    paths: Option<Paths>,
    memos: Memos,
    selected: usize,
    /// Counts every change of the open memo, so that the field of a memo starts without the cursor and the
    /// undo steps of the one before.
    epoch: u64,
    /// When the text changed that is not written yet (the clock of the window).
    changed_at: Option<f64>,
    /// When the memos were written last (the clock of the window), for the note "Saved".
    saved_at: Option<f64>,
    failed: bool,
    /// A memo deleted a moment ago: when, its place and the memo, to be taken back.
    deleted: Option<(f64, usize, Memo)>,
    /// The text field gets the cursor in the next frame.
    focus_text: bool,
    /// Where the cursor of the field stood, and the styles chosen for the next letters.
    caret: rich::Caret,
    /// The letters to mark again in the frame after a click on the bar (the field drops the marking when it
    /// gives up its cursor for the click).
    reselect: Option<std::ops::Range<usize>>,
}

impl Default for NotesState {
    fn default() -> Self {
        NotesState::kept_in(None, Memos::default())
    }
}

impl Drop for NotesState {
    fn drop(&mut self) {
        self.flush(None);
    }
}

fn text_id(epoch: u64) -> egui::Id {
    egui::Id::unique(("notizen-text", epoch))
}

impl NotesState {
    /// The memos of the data folder.
    pub fn with_files(paths: &Paths) -> NotesState {
        NotesState::kept_in(Some(Paths::new(paths.host.clone())), memo::load(paths))
    }

    fn kept_in(paths: Option<Paths>, memos: Memos) -> NotesState {
        NotesState {
            paths,
            memos,
            selected: 0,
            epoch: 0,
            changed_at: None,
            saved_at: None,
            failed: false,
            deleted: None,
            focus_text: false,
            caret: rich::Caret::default(),
            reselect: None,
        }
    }

    /// The memos as they stand in the window.
    pub fn memos(&self) -> &Memos {
        &self.memos
    }

    /// Writes what is typed and not written yet.
    pub fn flush(&mut self, now: Option<f64>) {
        if self.changed_at.is_some() {
            self.write(now);
        }
    }

    fn write(&mut self, now: Option<f64>) {
        self.changed_at = None;
        let Some(paths) = &self.paths else { return };
        match memo::save(paths, &self.memos) {
            Ok(()) => {
                self.failed = false;
                self.saved_at = now;
            }
            Err(error) => {
                versekit_core::errors::record("notes.save", &error);
                self.failed = true;
            }
        }
    }

    /// Writes the typed text once the pause after the last keystroke is over, and asks for a frame at that moment.
    fn tick(&mut self, ui: &Ui) {
        let Some(since) = self.changed_at else { return };
        let now = ui.input(|i| i.time);
        if now - since >= SAVE_DELAY {
            self.write(Some(now));
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SAVE_DELAY - (now - since)));
        }
    }

    fn open(&mut self, at: usize) {
        self.selected = at;
        self.epoch += 1;
        self.caret = rich::Caret::default();
        self.reselect = None;
    }

    fn select(&mut self, at: usize, now: f64) {
        if at != self.selected {
            self.flush(Some(now));
            self.open(at);
        }
    }

    pub(crate) fn add(&mut self, now: f64) {
        self.flush(Some(now));
        let at = self.memos.add(versekit_core::export::now_unix());
        self.open(at);
        self.focus_text = true;
        self.write(Some(now));
    }

    fn delete_selected(&mut self, now: f64) {
        self.flush(Some(now));
        let Some(gone) = self.memos.remove(self.selected) else { return };
        self.deleted = Some((now, self.selected, gone));
        self.open(self.selected.min(self.memos.items.len().saturating_sub(1)));
        self.write(Some(now));
    }

    fn take_back(&mut self, at: usize, memo: Memo, now: f64) {
        self.flush(Some(now));
        let at = self.memos.restore(at, memo);
        self.open(at);
        self.write(Some(now));
    }

    /// The text of the open memo was typed into.
    #[cfg(test)]
    pub(crate) fn typed(&mut self, text: &str, now: f64) {
        self.typed_at(text, None, now);
    }

    /// [`NotesState::typed`] with where the cursor stands after it (in letters); new letters get the styles
    /// chosen for them.
    fn typed_at(&mut self, text: &str, cursor: Option<usize>, now: f64) {
        let typing = self.caret.pending.take();
        if self.memos.edit(self.selected, text, cursor, typing, versekit_core::export::now_unix()) {
            self.changed_at = Some(now);
        }
    }

    /// Takes what a button or a key did into the open memo; the cursor goes where it says.
    fn take(&mut self, outcome: rich::Outcome, now: f64) {
        match outcome {
            rich::Outcome::Typing => {}
            rich::Outcome::Look(look) => self.restyled(look, now),
            rich::Outcome::Moved(at) => self.caret.selection = at..at,
            rich::Outcome::Rebuilt(text, look, at) => {
                if self.memos.rebuild(self.selected, text, look, versekit_core::export::now_unix()) {
                    self.changed_at = Some(now);
                }
                self.caret.selection = at..at;
            }
        }
    }

    /// The open memo got a new look.
    fn restyled(&mut self, look: versekit_core::memo::format::Formatting, now: f64) {
        if self.memos.set_look(self.selected, look, versekit_core::export::now_unix()) {
            self.changed_at = Some(now);
        }
    }
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The name of a memo in the list.
fn name_of(memo: &Memo) -> String {
    match memo.title() {
        "" => t("s_nz_ohne_titel"),
        title => title.to_string(),
    }
}

/// Where the title of `text` ends (see [`Memo::title`]): the end of its first line that holds something,
/// without the line break; 0 for a text without one.
fn title_end(text: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if !line.trim().is_empty() {
            return at + line.trim_end_matches(['\n', '\r']).len();
        }
        at += line.len();
    }
    0
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut NotesState) {
    state.tick(ui);
    let now = ui.input(|i| i.time);
    ui.label(sub(scheme, t("s_nz_lead")));
    ui.add_space(6.0);
    if state.failed {
        ui.label(RichText::new(t("s_nz_nicht_gespeichert")).color(color(scheme.red)));
        ui.add_space(4.0);
    }
    ui.horizontal_wrapped(|row| {
        let new_word = t("s_nz_neu");
        if row.add(widgets::Button::new(&new_word, ButtonKind::Strong, scheme).icon("plus")).clicked() {
            state.add(now);
        }
        let delete_word = t("s_nz_loeschen");
        let delete = widgets::Button::new(&delete_word, ButtonKind::Normal, scheme).icon("trash-2");
        if row.add_enabled(!state.memos.items.is_empty(), delete).clicked() {
            state.delete_selected(now);
        }
        widgets::saved_hint(row, scheme, state.saved_at, &t("s_nz_gespeichert"));
    });
    ui.add_space(6.0);

    let room = layout::list_room(ui, ROW_HEIGHT);
    let (list_rect, editor_rect);
    if ui.available_width() >= TWO_COLUMNS_FROM {
        let mut rects = (egui::Rect::NOTHING, None);
        ui.horizontal_top(|row| {
            row.spacing_mut().item_spacing.x = GAP;
            let column = egui::Layout::top_down(egui::Align::Min);
            row.allocate_ui_with_layout(egui::vec2(LIST_WIDTH, 0.0), column, |list| {
                list.set_width(LIST_WIDTH);
                rects.0 = list_block(list, scheme, state, room, true, now);
            });
            let rest = row.available_width();
            row.allocate_ui_with_layout(egui::vec2(rest, 0.0), column, |text| {
                text.set_width(rest);
                rects.1 = editor(text, scheme, state, room, now);
            });
        });
        (list_rect, editor_rect) = rects;
    } else {
        let list_height = (STACKED_ROWS * ROW_HEIGHT).min(room);
        list_rect = list_block(ui, scheme, state, list_height, false, now);
        ui.add_space(GAP);
        editor_rect = editor(ui, scheme, state, (room - list_height - GAP).max(0.0), now);
    }
    if cfg!(test) {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(BOXES_ID), (list_rect, editor_rect)));
    }

    // A deleted memo can be taken back for a few seconds, to the place where it stood.
    if let Some((since, at, memo)) = state.deleted.take() {
        let said = tf("s_nz_geloescht", &[("0", Arg::from(name_of(&memo)))]);
        let word = t("s_mlg_rueckgaengig");
        let shown = widgets::notice(ui.ctx(), scheme, since, "trash-2", &said, Some(&word));
        if shown.acted {
            state.take_back(at, memo, now);
            ui.ctx().request_repaint();
        } else if shown.shown {
            state.deleted = Some((since, at, memo));
        }
    }
}

/// The list of the memos by their titles, `height` high with its frame. It stands that high even with few
/// memos where it `fills` (beside the text), else it ends with the last row; more rows scroll.
fn list_block(ui: &mut Ui, scheme: &Scheme, state: &mut NotesState, height: f32, fills: bool, now: f64) -> egui::Rect {
    let inner = (height - widgets::LIST_FRAME_EXTRA).max(0.0);
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        if state.memos.items.is_empty() {
            if fills {
                list.set_min_height(inner);
            }
            list.label(sub(scheme, t("s_nz_liste_leer")));
            return;
        }
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .id_salt("notizen-liste")
            .max_height(inner)
            .min_scrolled_height(inner)
            .auto_shrink([false, !fills])
            .show(list, |rows| {
                for (at, memo) in state.memos.items.iter().enumerate() {
                    let row = widgets::list_row(rows, scheme, at == state.selected, &name_of(memo));
                    if row.clicked() {
                        chosen = Some(at);
                    }
                }
            });
        if let Some(at) = chosen {
            state.select(at, now);
        }
    })
    .response
    .rect
}

/// The large field for the text of the open memo, at least `height` high; its rectangle, without a memo
/// none.
fn editor(ui: &mut Ui, scheme: &Scheme, state: &mut NotesState, height: f32, now: f64) -> Option<egui::Rect> {
    let Some(open) = state.memos.items.get(state.selected) else {
        widgets::empty_state(ui, scheme, "sticky-note", &t("s_nz_nichts_offen"));
        return None;
    };
    let id = text_id(state.epoch);
    let (before, look_before) = (open.text.clone(), open.look.clone());

    // The bar and the keys act on the text marked when the field last had the cursor.
    let bar_top = ui.cursor().top();
    let mut action = rich::toolbar(ui, scheme, &before, &look_before, &state.caret);
    ui.add_space(4.0);
    let height = (height - (ui.cursor().top() - bar_top)).max(0.0);
    let mut outcome = None;
    if ui.memory(|memory| memory.has_focus(id)) {
        action = action.or_else(|| rich::shortcut(ui));
        if let Some(look) = rich::backspace_leaves_list(ui, &before, &look_before, &state.caret) {
            outcome = Some(rich::Outcome::Look(look));
        }
        outcome = outcome.or_else(|| rich::table_keys(ui, &before, &look_before, &state.caret));
    }
    let acted = action.is_some() || outcome.is_some();
    if let Some(action) = action {
        outcome = rich::apply(action, &before, &look_before, &mut state.caret);
    }
    if let Some(outcome) = outcome {
        state.take(outcome, now);
    }
    if acted {
        // A click on the bar gives the field its cursor back, with the same letters marked. The field gives
        // up both when it sees the click elsewhere, so they are given again in the next frame.
        rich::restore_selection(ui.ctx(), id, &state.caret.selection);
        state.reselect = Some(state.caret.selection.clone());
    } else if let Some(marked) = state.reselect.take() {
        rich::restore_selection(ui.ctx(), id, &marked);
    }

    let (before, look) = {
        let open = &state.memos.items[state.selected];
        (open.text.clone(), open.look.clone())
    };
    let mut text = before.clone();
    let in_table = state.caret.in_table(&before, &look);
    let hint = t("s_nz_schreib_hier");
    let name = t("hf_notizen");
    // The line that names the memo stands as a heading: Lato Bold in the size and colour of a heading.
    let body = egui::TextStyle::Body.resolve(ui.style());
    let fonts = rich::Fonts {
        title: crate::fonts::bold(egui::TextStyle::Heading.resolve(ui.style()).size),
        bold: crate::fonts::bold(body.size),
        body,
        title_color: color(widgets::heading_color(scheme)),
        body_color: color(scheme.fg),
        grid: color(scheme.line),
    };
    let tables = std::cell::RefCell::new(Vec::new());
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
        let all = buffer.as_str();
        rich::layout(ui, all, &before, &look, title_end(all), &fonts, wrap, &tables)
    };
    let edit = widgets::text_area(&mut text, &hint, scheme);
    // In a table Tab goes from cell to cell instead of to the next control.
    let filter = egui::EventFilter { tab: in_table, ..edit.get_event_filter() };
    let edit = edit
        .event_filter(filter)
        .layouter(&mut layouter)
        .id(id)
        .desired_width(f32::INFINITY)
        .desired_rows(TEXT_ROWS)
        .min_size(egui::vec2(0.0, height));
    let output = edit.show(ui);
    let response = output.response.response.clone();
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, &name);
        info.current_text_value = Some(text.clone());
        info
    });
    if std::mem::take(&mut state.focus_text) {
        response.request_focus();
    }
    let cursor = output.cursor_range.map(|range| range.primary.index.0);
    if let Some(range) = output.cursor_range {
        let marked = range.as_sorted_char_range();
        let marked = marked.start.0..marked.end.0;
        // Moving the cursor forgets styles chosen for letters not typed yet.
        if !response.changed() && marked != state.caret.selection {
            state.caret.pending = None;
        }
        state.caret.selection = marked;
    }
    if response.changed() {
        match look.break_ends_list(&before, &text, cursor) {
            // Enter in an empty point ends the list there, the text stays as it was.
            Some(line) => {
                let mut ended = look.clone();
                ended.set_lines(line..line + 1, versekit_core::memo::format::LineKind::Plain);
                state.restyled(ended, now);
                let at = cursor.unwrap_or(1).saturating_sub(1);
                state.caret.selection = at..at;
                rich::restore_selection(ui.ctx(), id, &state.caret.selection);
                ui.ctx().request_repaint();
            }
            None => state.typed_at(&text, cursor, now),
        }
    }
    let shown = &state.memos.items[state.selected];
    if shown.text == text {
        let (galley, at, clip) = (&output.galley, output.galley_pos, output.text_clip_rect);
        rich::paint_markers(ui, galley, at, clip, &shown.look, &fonts);
        rich::paint_tables(ui, galley, at, clip, &tables.borrow(), &fonts);
    }
    if response.lost_focus() {
        state.flush(Some(now));
    }
    Some(response.rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Rect, vec2};
    use versekit_core::language::{Choice, set_language};

    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    /// The page at `time` seconds with `events` typed or clicked in the first of two frames (a notice is drawn
    /// from the second on): the texts and how far it sticks out.
    fn frame(
        ctx: &egui::Context,
        state: &mut NotesState,
        width: f32,
        time: f64,
        events: Vec<Event>,
    ) -> (Vec<String>, f32) {
        frame_in(ctx, state, vec2(width, 900.0), time, events)
    }

    /// [`frame`] in a window of `size`.
    fn frame_in(
        ctx: &egui::Context,
        state: &mut NotesState,
        size: egui::Vec2,
        time: f64,
        events: Vec<Event>,
    ) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let mut over = 0.0;
        let mut shown_texts = Vec::new();
        for events in [events, Vec::new()] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    let shown = layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
                    over = shown.overflow;
                },
            );
            output.textures_delta.clear();
            shown_texts = texts(&output.shapes);
        }
        (shown_texts, over)
    }

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("versekit-notizen-seite-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let vars = [(versekit_core::legacy::ENV_HOME, home.as_str()), (versekit_core::legacy::ENV_NO_NET, "1")];
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &home);
        (Paths::new(host), dir)
    }

    fn type_text(text: &str) -> Vec<Event> {
        vec![Event::Text(text.to_string())]
    }

    /// The rectangles of the list and of the text field in the last frame.
    fn boxes(ctx: &egui::Context) -> (Rect, Option<Rect>) {
        ctx.data(|d| d.get_temp(egui::Id::unique(BOXES_ID))).expect("the page drew")
    }

    /// A page with `count` notes, the first one open.
    fn state_with(count: usize) -> NotesState {
        let mut state = NotesState::default();
        for n in 0..count {
            state.add(0.0);
            state.memos.set_text(n, &format!("Zettel {n}"), 1);
        }
        state.open(0);
        state
    }

    #[test]
    fn the_title_ends_with_the_first_line_that_holds_something() {
        for (text, title) in [
            ("", ""),
            ("Titanium holen", "Titanium holen"),
            ("Titanium holen\nLorville", "Titanium holen"),
            ("\n  \nTitanium\r\nLorville", "\n  \nTitanium"),
            ("   \n", ""),
        ] {
            assert_eq!(&text[..title_end(text)], title, "{text:?}");
            let memo = Memo { text: text.to_string(), ..Default::default() };
            assert_eq!(text[..title_end(text)].trim(), memo.title(), "the same line as the name in the list: {text:?}");
        }
    }

    #[test]
    fn the_first_line_of_the_field_stands_as_a_heading_and_the_rest_as_text() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let mut state = state_with(1);
        state.memos.set_text(0, "Titanium holen\nLorville\nArea18", 1);
        state.epoch += 1;
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, &mut state));
            },
        );
        output.textures_delta.clear();
        fn find(shape: &egui::epaint::Shape) -> Option<std::sync::Arc<egui::Galley>> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text().contains("Lorville") => Some(text.galley.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(find),
                _ => None,
            }
        }
        let galley = output.shapes.iter().find_map(|c| find(&c.shape)).expect("the field is drawn");
        let sections = &galley.job.sections;
        let bold = egui::FontFamily::Name(crate::fonts::BOLD.into());
        let range = |s: &egui::text::LayoutSection| (s.byte_range.start.0, s.byte_range.end.0);
        let title = sections.iter().find(|s| range(s) == (0, 14)).expect("a section for the first line");
        assert_eq!(title.format.font_id.family, bold, "the first line in Lato Bold");
        let rest = sections.iter().find(|s| range(s).0 == 14).expect("a section for the rest");
        assert_ne!(rest.format.font_id.family, bold, "the rest as usual text");
        assert!(title.format.font_id.size > rest.format.font_id.size, "the first line larger");
    }

    #[test]
    fn a_note_made_typed_saved_and_loaded_again_is_there() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("rundweg");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        let (shown, over) = frame(&ctx, &mut state, 900.0, 0.0, Vec::new());
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_nz_liste_leer")) && shown.contains(&t("s_nz_nichts_offen")), "{shown:?}");

        // "New note" makes one, writes it at once and puts the cursor into the field.
        state.add(1.0);
        assert_eq!(state.memos().items.len(), 1);
        assert_eq!(memo::load(&paths).items.len(), 1, "an empty note is kept as well");
        frame(&ctx, &mut state, 900.0, 1.0, Vec::new());
        // The text arrives in the focused field; it is not written before the pause is over.
        frame(&ctx, &mut state, 900.0, 1.1, type_text("Titanium holen\nLorville\nArea18"));
        assert_eq!(state.memos().items[0].text, "Titanium holen\nLorville\nArea18");
        assert!(memo::load(&paths).items[0].text.is_empty(), "not written while one is still typing");
        frame(&ctx, &mut state, 900.0, 1.5, Vec::new());
        assert!(memo::load(&paths).items[0].text.is_empty(), "the pause is not over");
        let (shown, _) = frame(&ctx, &mut state, 900.0, 2.0, Vec::new());
        assert_eq!(memo::load(&paths).items[0].text, "Titanium holen\nLorville\nArea18");
        assert!(shown.contains(&"Titanium holen".to_string()), "the title stands in the list: {shown:?}");
        assert!(shown.contains(&t("s_nz_gespeichert")), "{shown:?}");

        // A new window with the same folder finds the note again, in its field.
        let mut again = NotesState::with_files(&paths);
        let (shown, over) = frame(&ctx, &mut again, 900.0, 3.0, Vec::new());
        assert_eq!(over, 0.0);
        assert!(shown.iter().any(|s| s.contains("Lorville")), "{shown:?}");
        assert!(!shown.contains(&t("s_nz_nichts_offen")), "{shown:?}");
        drop(state);
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The list of the notes reaches the bottom of the window as the text field does, with the height of the
    /// window, whether there are few notes or many; many scroll inside it.
    #[test]
    fn the_list_goes_down_to_the_edge_of_the_window_like_the_text_field_at_every_height() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let mut last_height = 0.0;
        for window in [480.0, 640.0, 900.0, 1300.0] {
            let size = vec2(900.0, window);
            let mut list_height = 0.0;
            for count in [0, 1, 3, 80] {
                let ctx = context();
                let mut state = state_with(count);
                let (shown, over) = frame_in(&ctx, &mut state, size, 0.0, Vec::new());
                assert_eq!(over, 0.0);
                let (list, editor) = boxes(&ctx);
                list_height = list.height();
                assert!(
                    (list.bottom() - (window - 12.0)).abs() <= 1.0,
                    "window {window}, {count} notes: the list ends at {} of {}",
                    list.bottom(),
                    window - 12.0
                );
                if let Some(editor) = editor {
                    assert!(
                        (list.bottom() - editor.bottom()).abs() <= 1.0,
                        "window {window}, {count} notes: list {} and text {}",
                        list.bottom(),
                        editor.bottom()
                    );
                    assert!(count > 0);
                }
                if count == 80 {
                    // The rows beyond the frame are not drawn; the wheel brings them.
                    assert!(!shown.contains(&"Zettel 79".to_string()), "{shown:?}");
                    let at = egui::pos2(100.0, list.center().y);
                    let wheel = Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: vec2(0.0, -10_000.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: egui::Modifiers::NONE,
                    };
                    frame_in(&ctx, &mut state, size, 0.1, vec![Event::PointerMoved(at), wheel]);
                    let (shown, _) = frame_in(&ctx, &mut state, size, 0.2, Vec::new());
                    assert!(shown.contains(&"Zettel 79".to_string()), "window {window}: {shown:?}");
                    assert_eq!(boxes(&ctx).0.height(), list.height(), "scrolling does not change the list");
                }
            }
            assert!(list_height > last_height, "the list grows with the window: {list_height} at {window}");
            last_height = list_height;
        }
    }

    /// Where the window is narrow the list stands above the text and stays short.
    #[test]
    fn in_a_narrow_window_the_list_above_the_text_ends_with_its_rows() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let ctx = context();
        let mut state = state_with(2);
        let size = vec2(400.0, 900.0);
        frame_in(&ctx, &mut state, size, 0.0, Vec::new());
        let (list, editor) = boxes(&ctx);
        assert!(list.height() < STACKED_ROWS * ROW_HEIGHT, "{}", list.height());
        assert!(editor.is_some_and(|editor| editor.top() >= list.bottom()), "{editor:?} {list:?}");
    }

    #[test]
    fn the_note_is_not_there_when_nothing_was_saved() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("gegenprobe");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        state.add(1.0);
        frame(&ctx, &mut state, 900.0, 1.0, Vec::new());
        frame(&ctx, &mut state, 900.0, 1.1, type_text("Nur im Fenster"));
        // Before the pause is over the file holds nothing, so a window opened now finds an empty note.
        let early = NotesState::with_files(&paths);
        assert_eq!(early.memos().items[0].text, "");
        assert!(early.memos().latest().is_some_and(|m| m.title().is_empty()));
        // The memos of another folder are no part of it.
        let (other, other_dir) = paths_in("gegenprobe-anderer");
        assert!(NotesState::with_files(&other).memos().items.is_empty());
        // Leaving the page writes what waits: a flush, and the end of the window too.
        state.flush(Some(1.2));
        assert_eq!(NotesState::with_files(&paths).memos().items[0].text, "Nur im Fenster");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other_dir);
    }

    #[test]
    fn closing_the_window_writes_the_last_words() {
        let (paths, dir) = paths_in("schliessen");
        let ctx = context();
        {
            let mut state = NotesState::with_files(&paths);
            state.add(1.0);
            frame(&ctx, &mut state, 900.0, 1.0, Vec::new());
            frame(&ctx, &mut state, 900.0, 1.1, type_text("Kurz vor Schluss"));
        }
        assert_eq!(memo::load(&paths).items[0].text, "Kurz vor Schluss");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_note_comes_back_to_its_place_and_goes_after_a_while() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("loeschen");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        for (n, text) in ["eins", "zwei", "drei"].into_iter().enumerate() {
            state.add(n as f64);
            state.typed(text, n as f64);
            state.flush(Some(n as f64));
        }
        let titles = |state: &NotesState| state.memos().items.iter().map(|m| m.title().to_string()).collect::<Vec<_>>();
        assert_eq!(titles(&state), ["drei", "zwei", "eins"]);
        state.select(1, 5.0);
        state.delete_selected(5.0);
        assert_eq!(titles(&state), ["drei", "eins"]);
        assert_eq!(titles(&NotesState::with_files(&paths)), ["drei", "eins"], "deleted in the file as well");
        let (shown, _) = frame(&ctx, &mut state, 900.0, 5.5, Vec::new());
        assert!(shown.contains(&t("s_mlg_rueckgaengig")), "{shown:?}");
        assert!(shown.contains(&tf("s_nz_geloescht", &[("0", Arg::from("zwei"))])), "{shown:?}");
        let (since, at, memo) = state.deleted.take().unwrap();
        state.take_back(at, memo, since + 1.0);
        assert_eq!(titles(&state), ["drei", "zwei", "eins"]);
        assert_eq!(state.selected, 1, "the note it was is open again");
        assert_eq!(titles(&NotesState::with_files(&paths)), ["drei", "zwei", "eins"]);
        // Left alone, the offer is gone after its seconds.
        state.select(0, 6.0);
        state.delete_selected(6.0);
        let (shown, _) = frame(&ctx, &mut state, 900.0, 6.0 + widgets::NOTICE_SECONDS + 1.0, Vec::new());
        assert!(!shown.contains(&t("s_mlg_rueckgaengig")), "{shown:?}");
        assert!(state.deleted.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_locked_data_folder_says_so_and_keeps_the_note_in_the_window() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("gesperrt");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        versekit_core::writeguard::deny(true);
        state.add(1.0);
        state.typed("Bleibt im Fenster", 1.0);
        state.flush(Some(1.0));
        versekit_core::writeguard::deny(false);
        assert!(state.failed);
        let (shown, _) = frame(&ctx, &mut state, 900.0, 2.0, Vec::new());
        assert!(shown.contains(&t("s_nz_nicht_gespeichert")), "{shown:?}");
        assert_eq!(state.memos().items[0].text, "Bleibt im Fenster");
        state.write(Some(3.0));
        assert!(!state.failed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use versekit_core::memo::format::{BOLD, ITALIC, LineKind, STRIKE, UNDERLINE};

    /// Marks the letters `range` of the open note and gives the field the cursor, as dragging over them
    /// would; one frame later the page knows the marked letters.
    fn mark(ctx: &egui::Context, state: &mut NotesState, range: std::ops::Range<usize>, time: f64) {
        rich::restore_selection(ctx, text_id(state.epoch), &range);
        frame(ctx, state, 900.0, time, Vec::new());
    }

    /// Clicks the button `at` of the bar (in the order of [`rich::BUTTONS`]).
    fn click_button(ctx: &egui::Context, state: &mut NotesState, at: usize, time: f64) {
        let rects: Vec<Rect> = ctx.data(|d| d.get_temp(egui::Id::unique(rich::BAR_ID))).expect("the bar drew");
        let centre = rects[at].center();
        let press = |pressed| Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(ctx, state, 900.0, time, vec![Event::PointerMoved(centre), press(true)]);
        frame(ctx, state, 900.0, time + 0.05, vec![press(false)]);
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// The sections of the drawn field: (text, bold, italics, underlined, struck through).
    fn field_sections(ctx: &egui::Context, state: &mut NotesState, needle: &str) -> Vec<(String, bool, bool, bool, bool)> {
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(50.0),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
            },
        );
        output.textures_delta.clear();
        fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<std::sync::Arc<egui::Galley>> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text().contains(needle) => Some(text.galley.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, needle)),
                _ => None,
            }
        }
        let galley = output.shapes.iter().find_map(|c| find(&c.shape, needle)).expect("the field is drawn");
        let bold = egui::FontFamily::Name(crate::fonts::BOLD.into());
        let job = &galley.job;
        job.sections
            .iter()
            .map(|s| {
                let f = &s.format;
                (
                    job.text[s.byte_range.start.0..s.byte_range.end.0].to_string(),
                    f.font_id.family == bold,
                    f.italics,
                    f.underline.width > 0.0,
                    f.strikethrough.width > 0.0,
                )
            })
            .collect()
    }

    /// The circles drawn on the page (the bullets of a list).
    fn circles(shapes: &[egui::epaint::ClippedShape]) -> usize {
        fn count(shape: &egui::epaint::Shape) -> usize {
            match shape {
                egui::epaint::Shape::Circle(_) => 1,
                egui::epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
                _ => 0,
            }
        }
        shapes.iter().map(|c| count(&c.shape)).sum()
    }

    /// A page with one open note holding `text`, the field with the cursor.
    fn page_with(text: &str) -> (egui::Context, NotesState) {
        let ctx = context();
        let mut state = NotesState::default();
        state.add(0.0);
        state.typed(text, 0.0);
        state.focus_text = true;
        frame(&ctx, &mut state, 900.0, 0.0, Vec::new());
        (ctx, state)
    }

    #[test]
    fn the_buttons_give_the_marked_letters_their_look_and_take_it_again() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Einkauf\nTitanium und Gold");
        for (button, style) in [(0, BOLD), (1, ITALIC), (2, UNDERLINE), (3, STRIKE)] {
            // The time of the frames only goes forward.
            let at = 1.0 + button as f64;
            mark(&ctx, &mut state, 8..16, at);
            click_button(&ctx, &mut state, button, at + 0.1);
            let look = &state.memos().items[0].look;
            assert!(look.all_have(8..16, style), "button {button} gives \"Titanium\" its style");
            assert_eq!(look.style_at(7) & style, 0, "the line break before is not touched");
            assert_eq!(look.style_at(16) & style, 0, "the blank after is not touched");
            assert_eq!(state.caret.selection, 8..16, "the letters stay marked after the click");
        }
        let sections = field_sections(&ctx, &mut state, "Titanium");
        assert!(sections.contains(&("Titanium".to_string(), true, true, true, true)), "{sections:?}");
        assert!(sections.iter().any(|s| s.0 == " und Gold" && !s.1 && !s.2 && !s.3 && !s.4), "{sections:?}");
        // A second click takes the style away again (after the frame of `field_sections`).
        mark(&ctx, &mut state, 8..16, 51.0);
        click_button(&ctx, &mut state, 0, 51.1);
        assert_eq!(state.memos().items[0].look.style_at(8) & BOLD, 0);
        assert_eq!(state.memos().items[0].text, "Einkauf\nTitanium und Gold", "the text has no signs in it");
        // After a click on the bar the field keeps the cursor: what is typed next goes in, in the new style.
        mark(&ctx, &mut state, 25..25, 52.0);
        click_button(&ctx, &mut state, 0, 52.1);
        frame(&ctx, &mut state, 900.0, 52.2, Vec::new());
        frame(&ctx, &mut state, 900.0, 52.3, type_text(" Erz"));
        let memo = &state.memos().items[0];
        assert_eq!(memo.text, "Einkauf\nTitanium und Gold Erz");
        assert!(memo.look.all_have(25..29, BOLD), "typed bold after the click");
    }

    /// The bar stands inside the field's column at every size, in both languages and every width.
    #[test]
    fn the_bar_fits_at_every_size_language_and_width() {
        let _language = crate::language_lock();
        let mut findings = Vec::new();
        for language in [Choice::De, Choice::En] {
            set_language(language);
            for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
                for (physical, in_table) in [260.0f32, 330.0, 400.0, 559.0, 560.0, 700.0, 900.0]
                    .into_iter()
                    .flat_map(|width| [(width, false), (width, true)])
                {
                    let ctx = context();
                    crate::apply_size(&ctx, points);
                    let zoom = versekit_core::font_size::zoom_factor(points);
                    let width = physical / zoom;
                    let mut state = NotesState::default();
                    state.add(0.0);
                    state.typed("Ein Zettel mit einem sehr langen Titel\nErz\tMenge", 0.0);
                    if in_table {
                        // The cursor in the table: the buttons for its rows and columns stand in the bar too.
                        let mut look = state.memos().items[0].look.clone();
                        look.set_lines(1..2, LineKind::Table);
                        state.restyled(look, 0.0);
                        state.caret.selection = 40..40;
                    }
                    let mut over = 0.0;
                    for _ in 0..3 {
                        over = frame_in(&ctx, &mut state, vec2(width, 900.0 / zoom), 0.0, Vec::new()).1;
                    }
                    let rects: Vec<Rect> = ctx.data(|d| d.get_temp(egui::Id::unique(rich::BAR_ID))).unwrap();
                    let buttons = rich::BUTTONS.len() + if in_table { rich::TABLE_BUTTONS.len() } else { 0 };
                    assert_eq!(rects.len(), buttons, "table {in_table}");
                    let editor = boxes(&ctx).1.unwrap();
                    let right = rects.iter().map(|r| r.right()).fold(0.0, f32::max);
                    let left = rects.iter().map(|r| r.left()).fold(f32::MAX, f32::min);
                    if over > 0.5 || right > width + 0.5 || left < editor.left() - 0.5 || right > editor.right() + 0.5
                    {
                        findings.push(format!(
                            "{language:?} size {points} width {physical}: over {over}, bar {left}..{right}, field {}..{}",
                            editor.left(),
                            editor.right()
                        ));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    #[test]
    fn ctrl_b_i_u_act_on_the_marked_letters_and_ctrl_u_deletes_nothing() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Einkauf\nTitanium und Gold");
        mark(&ctx, &mut state, 8..16, 1.0);
        let command = egui::Modifiers::COMMAND;
        frame(&ctx, &mut state, 900.0, 1.1, vec![key(egui::Key::B, command)]);
        frame(&ctx, &mut state, 900.0, 1.2, vec![key(egui::Key::I, command)]);
        frame(&ctx, &mut state, 900.0, 1.3, vec![key(egui::Key::U, command)]);
        let memo = &state.memos().items[0];
        assert_eq!(memo.text, "Einkauf\nTitanium und Gold", "Ctrl+U does not delete to the start of the line");
        assert!(memo.look.all_have(8..16, BOLD | ITALIC | UNDERLINE));
        assert!(memo.look.all_have(8..16, BOLD) && memo.look.all_have(8..16, ITALIC));
        assert!(memo.look.all_have(8..16, UNDERLINE));
        // With nothing marked, Ctrl+B makes the next letters bold, and the button shows it.
        mark(&ctx, &mut state, 25..25, 2.0);
        frame(&ctx, &mut state, 900.0, 2.1, vec![key(egui::Key::B, command)]);
        assert!(state.caret.is_active(rich::Action::Style(BOLD), &state.memos().items[0].text, &state.memos().items[0].look));
        frame(&ctx, &mut state, 900.0, 2.2, type_text(" fett"));
        let memo = &state.memos().items[0];
        assert_eq!(memo.text, "Einkauf\nTitanium und Gold fett");
        assert!(memo.look.all_have(25..30, BOLD), "the letters typed after Ctrl+B are bold");
        assert!(!memo.look.all_have(17..25, BOLD));
    }

    #[test]
    fn the_list_buttons_make_points_with_bullets_and_numbers_and_no_signs_in_the_text() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Einkauf\nTitanium\nGold\nQuantanium");
        mark(&ctx, &mut state, 8..20, 1.0);
        click_button(&ctx, &mut state, 4, 1.1);
        let look = state.memos().items[0].look.clone();
        assert_eq!((0..4).map(|l| look.line_kind(l)).collect::<Vec<_>>(), [
            LineKind::Plain,
            LineKind::Bullet,
            LineKind::Bullet,
            LineKind::Plain
        ]);
        let scheme = versekit_core::theme::current();
        // The time of the frames only goes forward.
        let shapes = |ctx: &egui::Context, state: &mut NotesState, time: f64| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
                },
            );
            output.textures_delta.clear();
            output.shapes
        };
        let before = circles(&shapes(&ctx, &mut state, 1.5));
        assert!(before >= 2, "a bullet in front of each point: {before}");
        // The numbered list takes the same lines over and counts them.
        mark(&ctx, &mut state, 8..30, 2.0);
        click_button(&ctx, &mut state, 5, 2.1);
        let look = state.memos().items[0].look.clone();
        assert_eq!((1..4).map(|l| look.number_of(l)).collect::<Vec<_>>(), [Some(1), Some(2), Some(3)]);
        let shown = texts(&shapes(&ctx, &mut state, 2.5));
        for number in ["1.", "2.", "3."] {
            assert!(shown.contains(&number.to_string()), "{number} in front of its point: {shown:?}");
        }
        assert_eq!(circles(&shapes(&ctx, &mut state, 2.6)), before - 2, "no bullets left");
        let text = &state.memos().items[0].text;
        assert_eq!(text, "Einkauf\nTitanium\nGold\nQuantanium");
        assert!(!text.contains(['•', '-', '*', '|', '#']), "{text:?}");
        // The same button again takes the lines out of the list.
        mark(&ctx, &mut state, 8..30, 3.0);
        click_button(&ctx, &mut state, 5, 3.1);
        assert!(state.memos().items[0].look.is_plain());
    }

    #[test]
    fn enter_in_an_empty_point_ends_the_list_and_backspace_takes_a_point_out() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Einkauf\nTitanium");
        mark(&ctx, &mut state, 8..8, 1.0);
        click_button(&ctx, &mut state, 4, 1.1);
        // Enter at the end of the point makes the next point.
        mark(&ctx, &mut state, 16..16, 2.0);
        frame(&ctx, &mut state, 900.0, 2.1, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(state.memos().items[0].text, "Einkauf\nTitanium\n");
        assert_eq!(state.memos().items[0].look.line_kind(2), LineKind::Bullet);
        // Enter in that empty point ends the list and adds no line.
        frame(&ctx, &mut state, 900.0, 2.2, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        frame(&ctx, &mut state, 900.0, 2.3, Vec::new());
        assert_eq!(state.memos().items[0].text, "Einkauf\nTitanium\n");
        assert_eq!(state.memos().items[0].look.line_kind(2), LineKind::Plain);
        assert_eq!(state.memos().items[0].look.line_kind(1), LineKind::Bullet);
        // Backspace at the start of a point takes it out of the list and keeps the text.
        mark(&ctx, &mut state, 8..8, 3.0);
        frame(&ctx, &mut state, 900.0, 3.1, vec![key(egui::Key::Backspace, egui::Modifiers::NONE)]);
        assert_eq!(state.memos().items[0].text, "Einkauf\nTitanium\n");
        assert!(state.memos().items[0].look.is_plain());
        // On a plain line Backspace deletes as always.
        frame(&ctx, &mut state, 900.0, 3.2, vec![key(egui::Key::Backspace, egui::Modifiers::NONE)]);
        assert_eq!(state.memos().items[0].text, "EinkaufTitanium\n");
    }

    #[test]
    fn a_formatted_note_is_saved_and_opens_again_in_its_look() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("aussehen");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        state.add(0.0);
        state.focus_text = true;
        frame(&ctx, &mut state, 900.0, 0.0, Vec::new());
        frame(&ctx, &mut state, 900.0, 0.1, type_text("Einkauf\nTitanium und Gold"));
        mark(&ctx, &mut state, 8..16, 0.2);
        click_button(&ctx, &mut state, 0, 0.3);
        mark(&ctx, &mut state, 8..8, 0.4);
        click_button(&ctx, &mut state, 5, 0.5);
        frame(&ctx, &mut state, 900.0, 2.0, Vec::new());
        let read = memo::load(&paths);
        assert_eq!(read.items[0].text, "Einkauf\nTitanium und Gold");
        assert!(read.items[0].look.all_have(8..16, BOLD));
        assert_eq!(read.items[0].look.line_kind(1), LineKind::Number);
        // A new window shows it in that look.
        let mut again = NotesState::with_files(&paths);
        let sections = field_sections(&ctx, &mut again, "Titanium");
        assert!(sections.iter().any(|s| s.0 == "Titanium" && s.1), "{sections:?}");
        drop(state);
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A note as 5.1.0 saved it opens with its text as it was: signs that look like formatting stay letters,
    /// the first line stands as the heading, and saving it again changes nothing in the file.
    #[test]
    fn a_note_of_5_1_0_opens_as_it_was_and_stays_the_same_in_the_file() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("alt-5-1-0");
        let old = "{\n \"format\": 1,\n \"zettel\": [\n  {\n   \"text\": \"Titanium holen\\n- Lorville\\n**Area18**\",\n   \"bearbeitet\": 1762300000\n  }\n ]\n}";
        let file = paths.app_file(memo::FILE);
        std::fs::create_dir_all(std::path::Path::new(&file).parent().unwrap()).unwrap();
        std::fs::write(&file, old).unwrap();
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        let sections = field_sections(&ctx, &mut state, "Lorville");
        assert_eq!(sections[0], ("Titanium holen".to_string(), true, false, false, false), "{sections:?}");
        let rest: String = sections[1..].iter().map(|s| s.0.as_str()).collect();
        assert_eq!(rest, "\n- Lorville\n**Area18**");
        assert!(sections[1..].iter().all(|s| !s.1 && !s.2 && !s.3 && !s.4), "{sections:?}");
        state.flush(Some(1.0));
        state.write(Some(1.0));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), old);
        drop(state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_button_says_what_it_does_in_both_languages() {
        let _language = crate::language_lock();
        for (choice, words) in [(Choice::De, ["Fett (Strg+B)", "Kursiv (Strg+I)", "Unterstrichen (Strg+U)"]), (
            Choice::En,
            ["Bold (Ctrl+B)", "Italic (Ctrl+I)", "Underline (Ctrl+U)"],
        )] {
            set_language(choice);
            let meanings: Vec<String> =
                rich::BUTTONS.iter().map(|(_, _, key, letter)| rich::meaning(key, *letter)).collect();
            if !cfg!(target_os = "macos") {
                assert_eq!(meanings[..3], words);
            }
            assert!(meanings.iter().all(|m| !m.is_empty() && !m.starts_with("s_nz_")), "{meanings:?}");
            for (_, icon, key, letter) in rich::BUTTONS.iter().chain(&rich::TABLE_BUTTONS) {
                assert!(crate::icons::image(icon, versekit_core::theme::current().fg, 16.0).is_some(), "{icon}");
                let meaning = rich::meaning(key, *letter);
                assert!(!meaning.is_empty() && !meaning.starts_with("s_nz_"), "{key}: {meaning:?}");
            }
        }
    }

    /// The field as drawn: its galley.
    fn field_galley(ctx: &egui::Context, state: &mut NotesState, needle: &str) -> std::sync::Arc<egui::Galley> {
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(40.0),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
            },
        );
        output.textures_delta.clear();
        fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<std::sync::Arc<egui::Galley>> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text().contains(needle) => Some(text.galley.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, needle)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|c| find(&c.shape, needle)).expect("the field is drawn")
    }

    /// Where the letter `letter` of `row` of the galley starts (from the left of the galley).
    fn letter_x(galley: &egui::Galley, row: usize, letter: char) -> f32 {
        let row = &galley.rows[row];
        row.pos.x + row.glyphs.iter().find(|g| g.chr == letter).expect("the letter is drawn").pos.x
    }

    /// The straight lines drawn on the page (the lines of a table and the frames of other boxes).
    fn segments(ctx: &egui::Context, state: &mut NotesState) -> usize {
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(41.0),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
            },
        );
        output.textures_delta.clear();
        let shapes = output.shapes;
        fn count(shape: &egui::epaint::Shape) -> usize {
            match shape {
                egui::epaint::Shape::LineSegment { .. } => 1,
                egui::epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
                _ => 0,
            }
        }
        shapes.iter().map(|c| count(&c.shape)).sum()
    }

    fn bar_buttons(ctx: &egui::Context) -> usize {
        ctx.data(|d| d.get_temp::<Vec<Rect>>(egui::Id::unique(rich::BAR_ID))).map_or(0, |r| r.len())
    }

    const TABLE: usize = 6;
    const ADD_ROW: usize = 7;
    const REMOVE_ROW: usize = 8;
    const ADD_COLUMN: usize = 9;
    const REMOVE_COLUMN: usize = 10;

    #[test]
    fn a_table_is_put_in_filled_with_tab_and_its_columns_stand_in_line() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Erze");
        let lines_before = segments(&ctx, &mut state);
        mark(&ctx, &mut state, 4..4, 1.0);
        assert_eq!(bar_buttons(&ctx), rich::BUTTONS.len(), "no buttons for rows outside a table");
        click_button(&ctx, &mut state, TABLE, 1.1);
        assert_eq!(state.memos().items[0].text, "Erze\n\t\n\t");
        assert_eq!(state.caret.selection, 5..5, "the cursor in the first cell");
        frame(&ctx, &mut state, 900.0, 1.2, Vec::new());
        assert_eq!(bar_buttons(&ctx), rich::BUTTONS.len() + rich::TABLE_BUTTONS.len());
        let tab = key(egui::Key::Tab, egui::Modifiers::NONE);
        let mut time = 1.3;
        for typed in ["Erz", "Menge", "Gold", "12"] {
            frame(&ctx, &mut state, 900.0, time, type_text(typed));
            frame(&ctx, &mut state, 900.0, time + 0.05, vec![tab.clone()]);
            time += 0.1;
        }
        // Tab after the last cell added a row.
        assert_eq!(state.memos().items[0].text, "Erze\nErz\tMenge\nGold\t12\n\t");
        // In the very frame Tab is pressed the field keeps the cursor: it does not go on to the next control.
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events: vec![tab.clone()],
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, &mut state));
            },
        );
        output.textures_delta.clear();
        assert_eq!(ctx.memory(|m| m.focused()), Some(text_id(state.epoch)), "Tab stays in the table");
        assert_eq!(state.caret.selection, 24..24, "the cursor in the next cell");
        let look = &state.memos().items[0].look;
        assert_eq!((1..4).map(|l| look.line_kind(l)).collect::<Vec<_>>(), [LineKind::Table; 3]);
        // The second column starts at the same place in every row, behind the widest word of the first.
        let galley = field_galley(&ctx, &mut state, "Menge");
        let (menge, twelve) = (letter_x(&galley, 1, 'M'), letter_x(&galley, 2, '1'));
        assert!((menge - twelve).abs() < 0.5, "Menge at {menge}, 12 at {twelve}");
        let gold_end = galley.rows[2].pos.x + galley.rows[2].glyphs.iter().find(|g| g.chr == 'd').unwrap().max_x();
        assert!(twelve > gold_end + 4.0, "room between the columns: {gold_end} {twelve}");
        // Three rows and two columns: four lines across, three down.
        assert!(segments(&ctx, &mut state) >= lines_before + 7, "the lines of the table are drawn");
    }

    /// The lines of the table in `lines` (the numbers of its lines in the text) as the page draws them, with the
    /// letters of each cell.
    fn drawn_table(
        ctx: &egui::Context,
        state: &mut NotesState,
        width: f32,
        lines: std::ops::Range<usize>,
    ) -> rich::tests::Drawn {
        let scheme = versekit_core::theme::current();
        let mut shapes = Vec::new();
        for time in [50.0, 50.1] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 900.0))),
                    ..Default::default()
                },
                |ui| {
                    layout::page(ui, scheme, "notes-test", |ui| show_page(ui, scheme, state));
                },
            );
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        let body = ctx.global_style().text_styles[&egui::TextStyle::Body].size;
        let pad = (body * 0.8).round();
        fn texts(shape: &egui::epaint::Shape, out: &mut Vec<egui::epaint::TextShape>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.clone()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut all = Vec::new();
        shapes.iter().for_each(|c| texts(&c.shape, &mut all));
        let field = all.iter().find(|t| t.galley.text().contains('\t')).expect("the field is drawn");
        rich::tests::measure(&shapes, &field.galley, field.pos, lines, (pad, color(scheme.line)))
    }

    #[test]
    fn the_table_on_the_page_is_closed_all_round_and_every_cell_keeps_the_same_room_to_its_lines() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        for (scale, width) in [(1.0, 900.0), (1.25, 900.0), (1.5, 560.0), (2.0, 400.0), (1.0, 260.0)] {
            let (ctx, mut state) = page_with("Erze\nLindinium\t12\nSavrilium\t7");
            ctx.set_pixels_per_point(scale);
            let mut look = state.memos().items[0].look.clone();
            look.set_lines(1..3, LineKind::Table);
            state.restyled(look, 0.0);
            // The cursor stands in a cell: the buttons for rows and columns are there.
            mark(&ctx, &mut state, 6..6, 1.0);
            assert_eq!(bar_buttons(&ctx), rich::BUTTONS.len() + rich::TABLE_BUTTONS.len());
            let drawn = drawn_table(&ctx, &mut state, width, 1..3);
            let problems = rich::tests::check(&drawn, 2);
            let seen = format!("scale {scale}, {width} wide: {:?} {:?}", drawn.verticals, drawn.cells);
            assert_eq!(problems, Vec::<String>::new(), "{seen}");
        }
    }

    #[test]
    fn a_table_typed_in_by_hand_keeps_its_frame_and_its_room_at_every_step() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Erze");
        mark(&ctx, &mut state, 4..4, 1.0);
        click_button(&ctx, &mut state, TABLE, 1.1);
        frame(&ctx, &mut state, 900.0, 1.2, Vec::new());
        let tab = key(egui::Key::Tab, egui::Modifiers::NONE);
        let mut time = 1.3;
        let mut problems = Vec::new();
        for typed in ["Lindinium", "12", "Savrilium", "7"] {
            frame(&ctx, &mut state, 900.0, time, type_text(typed));
            for (step, rows) in [("typed", 1..3), ("again", 1..3)] {
                let table = state.memos().items[0].text.clone();
                let lines = table.split('\n').count();
                let columns = table.split('\n').skip(1).map(|l| l.split('\t').count()).max().unwrap_or(1);
                let rows = rows.start..lines.min(rows.end.max(lines));
                let drawn = drawn_table(&ctx, &mut state, 900.0, rows);
                for problem in rich::tests::check(&drawn, columns) {
                    problems.push(format!("after {typed:?} ({step}) in {table:?}: {problem}"));
                }
            }
            frame(&ctx, &mut state, 900.0, time + 0.05, vec![tab.clone()]);
            time += 0.1;
        }
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn the_row_and_column_buttons_change_the_table_and_the_table_button_takes_it_out() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (ctx, mut state) = page_with("Erze\nErz\tMenge\nGold\t12\nEnde");
        let mut look = state.memos().items[0].look.clone();
        look.set_lines(1..3, LineKind::Table);
        state.restyled(look, 0.0);
        mark(&ctx, &mut state, 6..6, 1.0);
        click_button(&ctx, &mut state, ADD_COLUMN, 1.1);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\t\tMenge\nGold\t\t12\nEnde");
        click_button(&ctx, &mut state, ADD_ROW, 1.2);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\t\tMenge\n\t\t\nGold\t\t12\nEnde");
        click_button(&ctx, &mut state, REMOVE_ROW, 1.3);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\t\tMenge\nGold\t\t12\nEnde");
        mark(&ctx, &mut state, 9..9, 2.0);
        click_button(&ctx, &mut state, REMOVE_COLUMN, 2.1);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\tMenge\nGold\t12\nEnde");
        // Enter in a row adds the next one; Backspace at the start of a cell joins nothing.
        mark(&ctx, &mut state, 9..9, 3.0);
        frame(&ctx, &mut state, 900.0, 3.1, vec![key(egui::Key::Backspace, egui::Modifiers::NONE)]);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\tMenge\nGold\t12\nEnde");
        frame(&ctx, &mut state, 900.0, 3.2, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(state.memos().items[0].text, "Erze\nErz\tMenge\n\t\nGold\t12\nEnde");
        // The table button in a table makes its rows plain lines again; the words stay.
        mark(&ctx, &mut state, 6..6, 4.0);
        click_button(&ctx, &mut state, TABLE, 4.1);
        assert!(state.memos().items[0].look.is_plain());
        assert_eq!(state.memos().items[0].text, "Erze\nErz\tMenge\n\t\nGold\t12\nEnde");
    }

    #[test]
    fn a_table_is_saved_as_tab_separated_lines_and_opens_again_as_a_table() {
        let _language = crate::language_lock();
        set_language(Choice::De);
        let (paths, dir) = paths_in("tabelle");
        let ctx = context();
        let mut state = NotesState::with_files(&paths);
        state.add(0.0);
        state.focus_text = true;
        frame(&ctx, &mut state, 900.0, 0.0, Vec::new());
        frame(&ctx, &mut state, 900.0, 0.1, type_text("Erze"));
        click_button(&ctx, &mut state, TABLE, 0.2);
        frame(&ctx, &mut state, 900.0, 0.3, type_text("Gold"));
        frame(&ctx, &mut state, 900.0, 2.0, Vec::new());
        let read = memo::load(&paths);
        assert_eq!(read.items[0].text, "Erze\nGold\t\n\t", "the plain text an older version shows");
        assert_eq!(read.items[0].look.line_kind(1), LineKind::Table);
        let mut again = NotesState::with_files(&paths);
        frame(&ctx, &mut again, 900.0, 3.0, Vec::new());
        // The same words without the table drawn fewer lines.
        let mut plain = NotesState::default();
        plain.add(0.0);
        plain.typed("Erze\nGold\t\n\t", 0.0);
        frame(&ctx, &mut plain, 900.0, 3.0, Vec::new());
        assert!(segments(&ctx, &mut again) > segments(&ctx, &mut plain), "drawn as a table");
        drop(state);
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_sticks_out_at_any_width_and_the_list_goes_above_the_text_where_narrow() {
        let _language = crate::language_lock();
        for choice in [Choice::De, Choice::En] {
            set_language(choice);
            let ctx = context();
            let mut state = NotesState::default();
            for n in 0..4 {
                state.add(f64::from(n));
                state.typed(
                    &format!(
                        "Ein Zettel mit einem sehr langen Titel, der nirgends abgeschnitten werden darf {n}\nZeile 2"
                    ),
                    0.0,
                );
            }
            for width in [260.0, 400.0, 559.0, 560.0, 900.0] {
                let (shown, over) = frame(&ctx, &mut state, width, 0.0, Vec::new());
                assert_eq!(over, 0.0, "{width} wide");
                assert!(shown.contains(&t("s_nz_neu")) && shown.contains(&t("s_nz_loeschen")), "{shown:?}");
            }
        }
    }
}
