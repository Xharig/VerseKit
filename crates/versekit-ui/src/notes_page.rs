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
//! one can be taken back for a few seconds.

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
    pub(crate) fn typed(&mut self, text: &str, now: f64) {
        if self.memos.set_text(self.selected, text, versekit_core::export::now_unix()) {
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
    let mut text = open.text.clone();
    let hint = t("s_nz_schreib_hier");
    let name = t("hf_notizen");
    let edit = widgets::text_area(&mut text, &hint, scheme)
        .id(text_id(state.epoch))
        .desired_width(f32::INFINITY)
        .desired_rows(TEXT_ROWS)
        .min_size(egui::vec2(0.0, height));
    let response = ui.add(edit);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, &name);
        info.current_text_value = Some(text.clone());
        info
    });
    if std::mem::take(&mut state.focus_text) {
        response.request_focus();
    }
    if response.changed() {
        state.typed(&text, now);
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
