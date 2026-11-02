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

//! The four buttons above the list of the bindings (`werkzeug_zeichnen` of 4.12): save as a **profile**, save the
//! binding as a file, as a list (CSV), and play a binding back.
//!
//! * **Profile.** A profile lies in the folder of the mappings of the game and loads there under its name
//!   (`pp_rebindkeys load <name>`): the way most want, since it lives in the game. A name that is taken is not
//!   replaced without asking: a whole evening of bindings can stand behind it. Over `joysticks_write` and the
//!   guard of the game: a copy of a replaced profile, nothing while the game runs.
//! * **Save / list.** Copies to a place the player chooses; nothing of the game is changed, so no guard.
//! * **Play back.** From one of the own profiles (the player knows his binding by its **name**, not by a path) or
//!   from a file somebody sent. Asks first: the present binding is replaced; a copy is made before.

use egui::{Context, RichText, Ui};
use versekit_core::joysticks;
use versekit_core::joysticks_write::{self, Failure};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::file_dialog::{self, Filters, Pending};
use crate::settings::Said;
use crate::widgets::{self, ButtonKind};

/// The system dialog to choose a file to open or to save to (a test puts an answer in its place).
pub type OpenFn = fn(&Context, &str, Filters, Option<&str>) -> Pending;
/// The system dialog to choose where to save (the last argument is the suggested name).
pub type SaveFn = fn(&Context, &str, Filters, Option<&str>, &str) -> Pending;

/// The height of the list of profiles in the dialog, in points.
const LIST_ROOM: f32 = 220.0;

/// What a dialog of the tools is at.
enum Dialog {
    /// The name of a new profile is typed; the profiles there are, to take a name from.
    Name { name: String, existing: Vec<String>, error: Option<String>, focus: bool },
    /// The name is taken: replace?
    Replace { name: String },
    /// A text to read (how to load the profile in the game).
    Result { text: String },
    /// Play back: from where?
    Source { profiles: Vec<String> },
    /// Play back: which profile?
    Choose { profiles: Vec<String> },
    /// Play back: sure? The source is a path.
    Confirm { source: String },
}

/// A system dialog that is open, and what it is for.
enum Picking {
    /// Where to save the binding (`true`: as a list).
    Save { csv: bool, pending: Pending },
    /// A file to play back.
    Open(Pending),
}

/// What the tools keep.
pub struct ToolsState {
    dialog: Option<Dialog>,
    picking: Option<Picking>,
    notice: Option<Said>,
    open: OpenFn,
    save: SaveFn,
}

impl Default for ToolsState {
    fn default() -> Self {
        ToolsState {
            dialog: None,
            picking: None,
            notice: None,
            open: file_dialog::open_file,
            save: file_dialog::save_file,
        }
    }
}

/// What the tools need from the page.
pub struct ToolsInfo<'a> {
    pub paths: &'a Paths,
    /// Why nothing may be written into the files of the game now.
    pub quiet: Option<&'a str>,
    /// The list as the text of a file, when it is asked for.
    pub csv: &'a dyn Fn() -> String,
}

impl ToolsState {
    /// Uses these dialogs instead of the system ones (the tests).
    #[cfg(test)]
    pub(crate) fn with_dialogs(open: OpenFn, save: SaveFn) -> ToolsState {
        ToolsState { open, save, ..ToolsState::default() }
    }

    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Is a dialog of the tools open (the keys then belong to it)?
    pub fn is_open(&self) -> bool {
        self.dialog.is_some()
    }

    fn told(&mut self, text: String, fault: bool) {
        self.notice = Some(if fault { Said::fault(text) } else { Said::note(text) });
    }

    /// The four buttons; gives `true` when the files of the game were changed (the page reads them again).
    pub fn show(&mut self, ui: &mut Ui, scheme: &Scheme, info: &ToolsInfo<'_>) -> bool {
        let mut changed = self.answer(ui.ctx(), info);
        ui.horizontal_wrapped(|ui| {
            // Before "save": the profile is the way most want; it lies in the game and loads there under its name.
            let (profile, save, list, play) =
                (t("s_js_profil"), t("s_js_ausgeben"), t("s_js_ausgeben_csv"), t("s_js_einlesen"));
            if ui.add(widgets::Button::new(&profile, ButtonKind::Normal, scheme).blocked(info.quiet)).clicked() {
                self.dialog = Some(Dialog::Name {
                    name: String::new(),
                    existing: joysticks::profiles(info.paths, None),
                    error: None,
                    focus: true,
                });
            }
            if ui.add(widgets::Button::new(&save, ButtonKind::Normal, scheme)).clicked() {
                let pending = (self.save)(ui.ctx(), &save, &[("XML", &["xml"])], None, "actionmaps.xml");
                self.picking = Some(Picking::Save { csv: false, pending });
            }
            if ui.add(widgets::Button::new(&list, ButtonKind::Normal, scheme)).clicked() {
                let pending = (self.save)(ui.ctx(), &list, &[("CSV", &["csv"])], None, "actionmaps.csv");
                self.picking = Some(Picking::Save { csv: true, pending });
            }
            if ui.add(widgets::Button::new(&play, ButtonKind::Normal, scheme).blocked(info.quiet)).clicked() {
                let profiles = joysticks::profiles(info.paths, None);
                if profiles.is_empty() {
                    // No profile of his own: only a file is left, so no question about where from.
                    let pending = (self.open)(ui.ctx(), &play, &[("XML", &["xml"])], None);
                    self.picking = Some(Picking::Open(pending));
                } else {
                    self.dialog = Some(Dialog::Source { profiles });
                }
            }
        });
        changed |= self.dialogs(ui.ctx(), scheme, info);
        changed
    }

    /// Takes the answer of a system dialog that closed.
    fn answer(&mut self, _ctx: &Context, info: &ToolsInfo<'_>) -> bool {
        let Some(picking) = self.picking.take() else { return false };
        match picking {
            Picking::Save { csv, pending } => match pending.take() {
                None => {
                    self.picking = Some(Picking::Save { csv, pending });
                    false
                }
                Some(None) => false,
                Some(Some(target)) => {
                    let result = if csv {
                        joysticks_write::export_csv(info.paths, &target, &(info.csv)(), None, None)
                    } else {
                        joysticks_write::export_xml(info.paths, &target, None, None)
                    };
                    match result {
                        Ok(()) => self.told(tf("s_js_ausgabe_ok", &[("0", Arg::from(target.as_str()))]), false),
                        Err(failure) => {
                            self.told(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))]), true);
                        }
                    }
                    false
                }
            },
            Picking::Open(pending) => match pending.take() {
                None => {
                    self.picking = Some(Picking::Open(pending));
                    false
                }
                Some(None) => false,
                Some(Some(source)) => {
                    self.dialog = Some(Dialog::Confirm { source });
                    false
                }
            },
        }
    }

    /// Draws the open dialog; gives `true` when a change was made.
    fn dialogs(&mut self, ctx: &Context, scheme: &Scheme, info: &ToolsInfo<'_>) -> bool {
        let Some(dialog) = self.dialog.take() else { return false };
        let mut changed = false;
        let mut keep: Option<Dialog> = None;
        match dialog {
            Dialog::Name { mut name, existing, mut error, mut focus } => {
                let (mut ok, mut cancel) = (false, false);
                let (_, close) = widgets::dialog(ctx, scheme, "steuerung-profil", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_profil"));
                    ui.add(egui::Label::new(t("s_js_profil_frage")).wrap());
                    ui.add_space(8.0);
                    let width = ui.available_width();
                    let response = widgets::clearable_field(ui, &mut name, &t("s_pl_profilname"), scheme, width);
                    if focus {
                        response.request_focus();
                        focus = false;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        ok = true;
                    }
                    if response.changed() {
                        error = None;
                    }
                    if let Some(text) = &error {
                        ui.add(egui::Label::new(RichText::new(text).small().color(crate::color(scheme.red))).wrap());
                    }
                    if !existing.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(t("s_js_profil_liste")).small().color(crate::color(scheme.sub)));
                        ui.horizontal_wrapped(|ui| {
                            for entry in &existing {
                                if ui.add(widgets::Button::new(entry, ButtonKind::Normal, scheme)).clicked() {
                                    name.clone_from(entry);
                                    error = None;
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ok |= ui.add(widgets::Button::new(&t("s_js_profil"), ButtonKind::Strong, scheme)).clicked();
                        cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                    });
                });
                if cancel || close {
                    return false;
                }
                // The question of the name is over when it was taken (the next dialog is set) or the profile saved.
                let mut over = false;
                if ok {
                    let wanted = versekit_core::text::py_strip(&name).to_string();
                    match joysticks::check_name(&wanted) {
                        Err(key) => error = Some(t(key)),
                        Ok(()) if existing.contains(&wanted) => {
                            keep = Some(Dialog::Replace { name: wanted });
                            over = true;
                        }
                        Ok(()) => {
                            changed = self.save_profile(info, &wanted, false);
                            over = true;
                        }
                    }
                }
                if !over {
                    keep = Some(Dialog::Name { name, existing, error, focus });
                }
            }
            Dialog::Replace { name } => {
                let (mut yes, mut no) = (false, false);
                let (_, close) = widgets::dialog(ctx, scheme, "steuerung-profil-ersetzen", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_profil"));
                    ui.add(egui::Label::new(tf("s_js_profil_ersetzen", &[("0", Arg::from(name.as_str()))])).wrap());
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        no = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                        yes =
                            ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Danger, scheme)).clicked();
                    });
                });
                if yes {
                    changed = self.save_profile(info, &name, true);
                } else if !(no || close) {
                    keep = Some(Dialog::Replace { name });
                }
            }
            Dialog::Result { text } => {
                let (ok, close) = widgets::dialog(ctx, scheme, "steuerung-profil-ok", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_profil"));
                    ui.label(&text);
                    ui.add_space(8.0);
                    ui.add(widgets::Button::new(&t("e_ok"), ButtonKind::Strong, scheme)).clicked()
                });
                if !(ok || close) {
                    keep = Some(Dialog::Result { text });
                }
            }
            Dialog::Source { profiles } => {
                let (mut mine, mut file, mut cancel) = (false, false, false);
                let (_, close) = widgets::dialog(ctx, scheme, "steuerung-einlesen", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_einlesen"));
                    ui.add(egui::Label::new(t("s_js_einlesen_woher")).wrap());
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        mine = ui
                            .add(widgets::Button::new(&t("s_js_einlesen_profil"), ButtonKind::Strong, scheme))
                            .clicked();
                        file = ui
                            .add(widgets::Button::new(&t("s_js_einlesen_datei"), ButtonKind::Normal, scheme))
                            .clicked();
                        cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                    });
                });
                if mine {
                    keep = Some(Dialog::Choose { profiles });
                } else if file {
                    let pending = (self.open)(ctx, &t("s_js_einlesen"), &[("XML", &["xml"])], None);
                    self.picking = Some(Picking::Open(pending));
                } else if !(cancel || close) {
                    keep = Some(Dialog::Source { profiles });
                }
            }
            Dialog::Choose { profiles } => {
                let mut chosen: Option<String> = None;
                let mut cancel = false;
                let (_, close) = widgets::dialog(ctx, scheme, "steuerung-einlesen-waehlen", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_einlesen"));
                    ui.add(egui::Label::new(t("s_js_einlesen_waehlen")).wrap());
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().max_height(LIST_ROOM).min_scrolled_height(LIST_ROOM).show(ui, |ui| {
                        for entry in &profiles {
                            if widgets::list_row(ui, scheme, false, entry).clicked() {
                                chosen = Some(entry.clone());
                            }
                        }
                    });
                    ui.add_space(8.0);
                    cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                });
                if let Some(name) = chosen {
                    match joysticks::profile_file(info.paths, &name, None) {
                        Some(source) => keep = Some(Dialog::Confirm { source }),
                        None => self.told(tf("s_js_schief", &[("0", Arg::from(t("s_js_f_datei").as_str()))]), true),
                    }
                } else if !(cancel || close) {
                    keep = Some(Dialog::Choose { profiles });
                }
            }
            Dialog::Confirm { source } => {
                let (mut yes, mut no) = (false, false);
                let (_, close) = widgets::dialog(ctx, scheme, "steuerung-einlesen-frage", |ui| {
                    widgets::heading(ui, scheme, &t("s_js_einlesen"));
                    ui.add(egui::Label::new(t("s_js_einlesen_frage")).wrap());
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        no = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                        yes =
                            ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), ButtonKind::Danger, scheme)).clicked();
                    });
                });
                if yes {
                    changed = self.play_back(info, &source);
                } else if !(no || close) {
                    keep = Some(Dialog::Confirm { source });
                }
            }
        }
        if let Some(next) = keep {
            self.dialog = Some(next);
        }
        changed
    }

    fn save_profile(&mut self, info: &ToolsInfo<'_>, name: &str, overwrite: bool) -> bool {
        match joysticks_write::save_profile(info.paths, name, overwrite, None, None) {
            Ok(_) => {
                let text = tf("s_js_profil_ok", &[("0", Arg::from(name)), ("1", Arg::from(name))]);
                self.dialog = Some(Dialog::Result { text });
                true
            }
            Err(Failure::NameTaken) => {
                self.dialog = Some(Dialog::Replace { name: name.to_string() });
                false
            }
            Err(failure) => {
                self.told(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))]), true);
                false
            }
        }
    }

    fn play_back(&mut self, info: &ToolsInfo<'_>, source: &str) -> bool {
        match joysticks_write::import_file(info.paths, source, None, None) {
            Ok(done) => {
                let copy = done.backup.as_deref().map(crate::joysticks_page::base_name).unwrap_or_default();
                let key = if done.count == 1 { "s_js_einlesen_ok_eins" } else { "s_js_einlesen_ok" };
                let count = i64::try_from(done.count).unwrap_or(i64::MAX);
                self.told(tf(key, &[("0", Arg::from(count)), ("1", Arg::from(copy.as_str()))]), false);
                true
            }
            Err(failure) => {
                self.told(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))]), true);
                false
            }
        }
    }
}
