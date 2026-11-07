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

//! The block "user.cfg" at the foot of the page „Spiel“ (`pages._user_cfg_section` of 4.12): the own lines of
//! the player in the `user.cfg` of a channel, to edit and save, and the whole file for reference beside them (under
//! them where the page is narrow).
//!
//! Unlike 4.12 this block does not write while the game runs (or cannot be told to), keeps what was typed when
//! the writing is refused, never takes a file that cannot be read for an empty one, and does not write over a
//! file that changed since it was read (the page Grafik writes into the same file): an untouched box follows the
//! file, an edited one asks for a reload instead of undoing what the other page wrote.

use egui::{RichText, TextStyle, Ui};
use versekit_core::game_files::WriteError;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::usercfg::{self, SetError};

use super::{GameTextsState, hint};
use crate::color;
use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind};

/// The smallest and the largest height of the boxes under each other, in lines (more lines scroll).
pub(super) const ROWS: (usize, usize) = (4, 18);
/// The same for the boxes side by side: they use the height the second box takes when they stand under each other.
pub(super) const SIDE_ROWS: (usize, usize) = (24, 32);
/// Narrowest a column of the block gets before the boxes stand under each other.
pub(super) const COLUMN_MIN: f32 = 340.0;
/// How many numbers of invalid lines the message names.
const NAMED_LINES: usize = 5;

/// What the block remembers: the channel shown, the text in the box and what was said last.
#[derive(Default)]
pub(super) struct UserCfgBlock {
    /// The folder of the channel shown; empty before the first look (then the first channel counts).
    pub(super) folder: String,
    /// The folder `own` and `whole` were read for; `None` before the first reading.
    read_for: Option<String>,
    /// The own lines as they stand in the box.
    pub(super) own: String,
    /// The own lines as the file gave them: the box is untouched while it still reads so.
    loaded_own: String,
    /// The whole file; `None` where this installation has none (or it cannot be read, see `unreadable`).
    pub(super) whole: Option<String>,
    /// Why the file could not be read, if that is so: it is not "no file", and nothing is saved then.
    pub(super) unreadable: Option<String>,
    /// When the file was last compared with the box (seconds of the window).
    pub(super) checked_at: Option<f64>,
    /// The last thing said, and whether it was a refusal.
    pub(super) said: Option<(String, bool)>,
    /// Where the two boxes (own lines, whole file) were drawn last, for the tests of the layout.
    #[cfg(test)]
    pub(super) drawn: [Option<egui::Rect>; 2],
}

/// How often the file is compared with what the box was read from (seconds).
const CHECK_SECONDS: f64 = 1.0;

impl UserCfgBlock {
    /// Reads the file of the folder anew; what was typed is gone.
    fn reload(&mut self) {
        match usercfg::read_checked(&self.folder) {
            Ok(whole) => {
                self.whole = whole;
                self.unreadable = None;
            }
            Err(error) => {
                self.whole = None;
                self.unreadable = Some(error.to_string());
            }
        }
        self.own = self.whole.as_deref().map(usercfg::own_lines).unwrap_or_default().join("\n");
        self.loaded_own.clone_from(&self.own);
        self.read_for = Some(self.folder.clone());
    }

    fn say(&mut self, text: String, bad: bool) {
        self.said = Some((text, bad));
    }

    /// Is the file as it was when it was read? (`false` also where it cannot be read now.)
    fn file_unchanged(&self) -> bool {
        match usercfg::read_checked(&self.folder) {
            Ok(now) => self.unreadable.is_none() && now == self.whole,
            Err(_) => false,
        }
    }

    /// Looks at the file now and then. A box nobody touched follows a change (the page Grafik writes into the
    /// same file); a box with typing in it keeps it and says that the file changed.
    fn follow_file(&mut self, now: f64) {
        if self.checked_at.is_some_and(|at| now - at < CHECK_SECONDS) {
            return;
        }
        self.checked_at = Some(now);
        if self.file_unchanged() {
            return;
        }
        if self.own == self.loaded_own {
            self.reload();
        } else if self.said.as_ref().is_none_or(|(text, _)| *text != t("s_uc_geaendert")) {
            self.say(t("s_uc_geaendert"), true);
        }
    }

    /// "Save": the lines of the box go into the file when all of them are valid, the game is quiet and the file is
    /// still the one the box was read from.
    fn save(&mut self, paths: &Paths) {
        if let Some(why) = &self.unreadable {
            let said = tf("s_uc_nicht_lesbar", &[("0", Arg::from(why.as_str()))]);
            self.say(said, true);
            return;
        }
        if !self.file_unchanged() {
            self.say(t("s_uc_geaendert"), true);
            return;
        }
        let lines: Vec<String> = self.own.lines().map(str::to_string).collect();
        match usercfg::write_own(paths, &self.folder, &lines) {
            Ok(_) => {
                self.reload();
                self.say(t("s_uc_gespeichert"), false);
            }
            Err(SetError::Bad(bad)) => {
                let numbers: Vec<String> = bad.iter().take(NAMED_LINES).map(|(number, _)| number.to_string()).collect();
                self.say(tf("s_uc_fehler", &[("0", Arg::from(numbers.join(", ")))]), true);
            }
            Err(SetError::Write(WriteError::Failed(_))) => self.say(t("s_uc_nicht"), true),
            Err(SetError::Write(other)) => self.say(other.reason(), true),
        }
    }
}

/// A box of the block: monospaced, as high as its text (but between `rows`), scrolling beyond that.
fn lines_box(
    ui: &mut Ui,
    salt: &str,
    text: &mut dyn egui::TextBuffer,
    lines: usize,
    rows: (usize, usize),
    tone: Option<egui::Color32>,
) -> egui::Rect {
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.fonts_mut(|f| f.row_height(&font));
    let height = row_height * rows.1 as f32 + 16.0;
    // A column has no height of its own to offer (its room is open downwards): the area asks for the rows
    // it wants instead of the 64 points a scroll area takes then.
    let wanted = row_height * lines.clamp(rows.0, rows.1) as f32 + 16.0;
    egui::ScrollArea::vertical()
        .id_salt((salt, "rollen"))
        .min_scrolled_height(wanted)
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut edit = egui::TextEdit::multiline(text)
                .id_salt(salt)
                .font(TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(lines.clamp(rows.0, rows.1));
            if let Some(tone) = tone {
                edit = edit.text_color(tone);
            }
            ui.add(edit);
        })
        .inner_rect
}

/// The block. Nothing is shown of the file before the first frame needs it; it is read when the channel changes
/// and on "Reload", not at every frame.
pub(super) fn show(ui: &mut Ui, scheme: &Scheme, state: &mut GameTextsState) {
    let Some(paths) = state.paths.clone() else { return };
    crate::settings::setting_group(ui, scheme, &t("s_uc_titel"));
    hint(ui, scheme, &t("s_uc_lead"));
    ui.add_space(4.0);
    // Only installations with a Data.p4k: a folder without it is no game.
    let channels: Vec<(String, String)> =
        paths.installed_channels().into_iter().filter(|(_, _, installed)| *installed).map(|(n, f, _)| (n, f)).collect();
    let block = &mut state.user_cfg;
    if channels.is_empty() {
        hint(ui, scheme, &t("s_tq_kein_spiel"));
        return;
    }
    if !channels.iter().any(|(_, folder)| *folder == block.folder) {
        block.folder = channels[0].1.clone();
    }
    // Every channel of the one list, in its order; one that is not installed stands grey with the reason.
    let missing = t("s_uc_nicht_installiert");
    ui.horizontal_wrapped(|row| {
        row.label(t("s_uc_kanal"));
        for name in versekit_core::paths::CHANNELS {
            let found = channels.iter().find(|(channel, _)| channel == name);
            let chosen = found.is_some_and(|(_, folder)| *folder == block.folder);
            let kind = if chosen { ButtonKind::Strong } else { ButtonKind::Normal };
            let button = widgets::Button::new(name, kind, scheme).blocked(found.is_none().then_some(missing.as_str()));
            if row.add(button).clicked()
                && let Some((_, folder)) = found
                && *folder != block.folder
            {
                block.folder.clone_from(folder);
                block.said = None;
            }
        }
    });
    if block.read_for.as_deref() != Some(block.folder.as_str()) {
        block.reload();
    }
    let now = ui.input(|i| i.time);
    block.follow_file(now);
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(CHECK_SECONDS));

    ui.add_space(8.0);
    let unreadable = block.unreadable.as_ref().map(|why| tf("s_uc_nicht_lesbar", &[("0", Arg::from(why.as_str()))]));
    // A file that cannot be read is not "no file": the box says what is wrong instead.
    let whole = unreadable.clone().or_else(|| block.whole.clone()).unwrap_or_else(|| t("s_uc_keine_datei"));
    let file = std::path::Path::new(&block.folder).join(usercfg::FILE).to_string_lossy().into_owned();
    let side_by_side = ui.available_width() >= 2.0 * COLUMN_MIN + GAP;
    // Side by side both boxes are equally tall and use the height the second box does not need below the first.
    let (rows, lines) =
        if side_by_side { (SIDE_ROWS, block.own.lines().count().max(whole.lines().count())) } else { (ROWS, 0) };
    // The heads of the columns are of different length: the taller one sets where both boxes begin.
    let head_key = ui.make_persistent_id("uc-kopf");
    let head_tallest: f32 = if side_by_side { ui.ctx().data(|d| d.get_temp(head_key)).unwrap_or(0.0) } else { 0.0 };
    let mut head_measured: f32 = 0.0;
    let head = |ui: &mut Ui, title: String, help: String, measured: &mut f32| {
        let top = ui.cursor().min.y;
        ui.label(RichText::new(title).strong());
        hint(ui, scheme, &help);
        let used = ui.cursor().min.y - top;
        *measured = measured.max(used);
        ui.add_space((head_tallest - used).max(0.0));
    };
    layout::columns_or_stack(ui, COLUMN_MIN, 2, |ui, column| {
        if column == 0 {
            head(ui, t("s_uc_eigene"), t("s_uc_eigene_h"), &mut head_measured);
            let own_lines = if side_by_side { lines } else { block.own.lines().count() };
            let _drawn = lines_box(ui, "uc-eigene", &mut block.own, own_lines, rows, None);
            #[cfg(test)]
            {
                block.drawn[0] = Some(_drawn);
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|row| {
                if row.add(widgets::Button::new(&t("s_uc_speichern"), ButtonKind::Strong, scheme)).clicked() {
                    block.save(&paths);
                }
                if row.add(widgets::Button::new(&t("s_uc_neu"), ButtonKind::Normal, scheme)).clicked() {
                    block.reload();
                    block.said = None;
                }
            });
            if let Some((text, bad)) = &block.said {
                let tone = if *bad { scheme.red } else { scheme.sub };
                ui.add(egui::Label::new(RichText::new(text).color(color(tone))).wrap());
            }
            if let Some(text) = &unreadable {
                ui.add(egui::Label::new(RichText::new(text).color(color(scheme.red))).wrap());
            }
        } else {
            if !side_by_side {
                ui.add_space(12.0 - GAP);
            }
            head(ui, t("s_uc_ganz"), tf("s_uc_ganz_h", &[("0", Arg::from(file.as_str()))]), &mut head_measured);
            let whole_lines = if side_by_side { lines } else { whole.lines().count() };
            let mut shown: &str = &whole;
            let _drawn = lines_box(ui, "uc-ganz", &mut shown, whole_lines, rows, Some(color(scheme.sub)));
            #[cfg(test)]
            {
                block.drawn[1] = Some(_drawn);
            }
        }
    });
    if side_by_side && (head_measured - head_tallest).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(head_key, head_measured));
        ui.ctx().request_repaint();
    }
}
