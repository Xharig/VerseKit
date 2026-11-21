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

//! The button "Sicherung" of the window (`main_window._backup` of 4.12): everything of one's own into one
//! file, or such a file back.
//!
//! ⚠ **Two ways behind one button.** Writing is the common case, restoring the rare one (change of the
//! computer, set up anew), but that is what a backup is for. A button that can only write solves half the
//! problem.
//!
//! The order of a restore is the one of 4.12: look first, then ask, then write. Whoever picks the wrong file
//! learns that BEFORE the inventory is gone. The work (reading, writing, restoring) runs in a thread of its own
//! so that the window keeps drawing; the program starts anew afterwards (`take_restart`), because the modules
//! hold their data in memory and would write it over the files just restored.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::RichText;
use versekit_core::backup::{self, BindingsError, Change, Found, PreviewItem, RestoreError, Restored, WriteError};
use versekit_core::backup_auto::{self, Trigger};
use versekit_core::backup_game::{self, ChannelContents, ChannelPreview, GameRestored};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;
use versekit_core::{game_ini, game_start, writeguard};

use super::SamplePage;
use crate::color;
use crate::file_dialog::{self, Pending};
use crate::widgets::{self, ButtonKind};

/// The controls of a backup that the player is asked about after the data are back: what the backup holds of
/// them, and what was answered so far.
struct ControlsAsk {
    source: String,
    /// What the restore of the data said (what was switched off, how many files).
    restored: Restored,
    /// Is the active binding in the backup (the old way, straight under `Steuerung/`)?
    active: bool,
    /// The names of the saved profiles in the backup (the old way).
    profiles: Vec<String>,
    /// The files of the game by channel; where there are any, they count instead of `active` and `profiles`.
    channels: Vec<ChannelContents>,
    /// The saved profiles are wanted (the first question was answered with yes).
    profiles_wanted: bool,
}

impl ControlsAsk {
    /// Does the backup hold the files of the game by channel?
    fn by_channel(&self) -> bool {
        !self.channels.is_empty()
    }

    /// Is there something that only adds (saved profiles, character templates)?
    fn has_additive(&self) -> bool {
        if self.by_channel() { self.channels.iter().any(ChannelContents::has_additive) } else { !self.profiles.is_empty() }
    }

    /// Is there something the game works with now (active binding; by channel also game settings and `user.cfg`)?
    fn has_active(&self) -> bool {
        if self.by_channel() { self.channels.iter().any(ChannelContents::has_active) } else { self.active }
    }

    /// The first question.
    fn additive_question(&self) -> String {
        if !self.by_channel() {
            return tf("sich_belegung_frage", &[("0", Arg::from(self.profiles.join(", ")))]);
        }
        let lines: Vec<String> = self
            .channels
            .iter()
            .filter(|c| c.has_additive())
            .map(|c| {
                tf(
                    "sich_spiel_zeile",
                    &[
                        ("0", Arg::from(c.channel.clone())),
                        ("1", Arg::from(c.profiles.len())),
                        ("2", Arg::from(c.templates.len())),
                    ],
                )
            })
            .collect();
        tf("sich_spiel_frage", &[("0", Arg::from(lines.join("\n")))])
    }

    /// The second question.
    fn active_question(&self) -> String {
        if self.by_channel() { t("sich_spiel_aktiv") } else { t("sich_belegung_aktiv") }
    }
}

/// What a restore would do, looked at before the question: the files of the data folder and of the game.
#[derive(Default)]
struct Look {
    data: Vec<PreviewItem>,
    game: Vec<ChannelPreview>,
}

/// How many of `items` would be replaced, be new, stay the same.
fn counted<'a>(items: impl Iterator<Item = &'a PreviewItem>) -> (usize, usize, usize) {
    items.fold((0, 0, 0), |(replaced, new, same), item| match item.change {
        Change::Replaced => (replaced + 1, new, same),
        Change::New => (replaced, new + 1, same),
        Change::Same => (replaced, new, same + 1),
        Change::NoPlace => (replaced, new, same),
    })
}

/// The word for a change in the list of the look.
fn change_word(change: Change) -> String {
    t(match change {
        Change::New => "sich_aend_neu",
        Change::Replaced => "sich_aend_ersetzt",
        Change::Same => "sich_aend_gleich",
        Change::NoPlace => "sich_aend_kein_ort",
    })
}

/// The lines of the look: one for the data folder, one for each channel of the game.
fn look_lines(look: &Look) -> Vec<String> {
    let (replaced, new, same) = counted(look.data.iter());
    let mut lines = vec![tf(
        "sich_vorschau",
        &[("0", Arg::from(replaced)), ("1", Arg::from(new)), ("2", Arg::from(same))],
    )];
    for channel in &look.game {
        if channel.installed {
            let (replaced, new, same) = counted(channel.additive.iter().chain(&channel.active));
            lines.push(tf(
                "sich_vorschau_kanal",
                &[
                    ("0", Arg::from(channel.channel.clone())),
                    ("1", Arg::from(replaced)),
                    ("2", Arg::from(new)),
                    ("3", Arg::from(same)),
                ],
            ));
        } else {
            lines.push(tf("sich_vorschau_fehlt", &[("0", Arg::from(channel.channel.clone()))]));
        }
    }
    lines
}

/// The files of the look that would change something (new, replaced, no place), by name.
fn look_changes(look: &Look) -> Vec<(Change, String)> {
    let mut out: Vec<(Change, String)> =
        look.data.iter().filter(|i| i.change != Change::Same).map(|i| (i.change, i.name.clone())).collect();
    for channel in &look.game {
        for item in channel.additive.iter().chain(&channel.active).filter(|i| i.change != Change::Same) {
            out.push((item.change, format!("{}: {}", channel.channel, item.name)));
        }
    }
    out
}

/// Where the flow is.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The choice: write or restore.
    Choose,
    /// The dialog for the place of the new file is open.
    Saving(Pending),
    /// The dialog for the file to restore is open.
    Opening(Pending),
    /// A thread works; `.1` is the text it is shown under.
    Working(Receiver<Done>, &'static str),
    /// The look into the file is done: is the player sure?
    Asking { source: String, found: Found, look: Look },
    /// The data are back and the backup holds saved profiles: put them back too? (They only lie there until the
    /// player loads one.)
    AskProfiles(ControlsAsk),
    /// Replace the ACTIVE binding as well? A question of its own with No as the way of least harm: whoever
    /// slips sits in a ship that answers nothing.
    AskActive(ControlsAsk),
    /// The restore is done and there is something to tell before the program starts anew.
    Told { text: String, count: usize },
}

/// What a thread ended with.
enum Done {
    Written {
        target: String,
        outcome: Result<backup::Written, WriteError>,
    },
    Checked {
        source: String,
        found: Option<Found>,
        look: Look,
    },
    Restored {
        source: String,
        outcome: Result<Restored, RestoreError>,
        controls: (bool, Vec<String>),
        channels: Vec<ChannelContents>,
    },
    /// The controls were put back (or why not); the restore of the data is told with them.
    Controls {
        restored: Restored,
        outcome: Result<GameRestored, BindingsError>,
        by_channel: bool,
    },
}

/// The state of the flow, in the page.
#[derive(Default)]
pub(super) struct BackupFlow {
    phase: Phase,
    /// The program can start itself anew (the window of the user's own data does).
    can_restart: bool,
    /// A restore is done and the program should start anew.
    restart: bool,
    /// The program is starting anew: no second backup is begun meanwhile.
    locked: bool,
}

/// What the dialogs of the backup show.
const ZIP_FILES: file_dialog::Filters = &[("ZIP", &["zip"])];

/// The archive suffix: a name without `.zip` gets it.
fn with_suffix(path: String) -> String {
    if path.to_lowercase().ends_with(".zip") { path } else { format!("{path}.zip") }
}

impl SamplePage {
    /// The program can start itself anew after a restore (see [`SamplePage::take_restart`]); without it the
    /// player is asked to do it.
    pub fn offer_restart(&mut self, can: bool) {
        self.backup.can_restart = can;
    }

    /// A restore is done and the program should start anew; true once.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.backup.restart)
    }

    /// May a backup be begun now: real data are open, no backup is under way and the program is not starting anew.
    fn backup_free(&self) -> bool {
        self.store.is_some() && !self.backup.locked && matches!(self.backup.phase, Phase::Idle)
    }

    /// Opens the choice "create or restore a backup" (the button in the foot of the window).
    pub(super) fn open_backup(&mut self) {
        if self.backup_free() {
            self.backup.phase = Phase::Choose;
        }
    }

    /// Goes straight to the dialog for the place of a new backup (the button of the page and the link of the
    /// tile: the way is clear there, no choice first).
    pub(super) fn open_backup_write(&mut self, ctx: &egui::Context) {
        if self.backup_free() {
            let pending = file_dialog::save_file(ctx, &t("sich_schreiben"), ZIP_FILES, None, &backup::suggestion());
            self.backup.phase = Phase::Saving(pending);
        }
    }

    /// Goes straight to the dialog for the backup to restore (the button of the page).
    pub(super) fn open_backup_read(&mut self, ctx: &egui::Context) {
        if self.backup_free() {
            self.backup.phase = Phase::Opening(file_dialog::open_file(ctx, &t("sich_lesen"), ZIP_FILES, None));
        }
    }

    /// The block "Sicherung" at the top of the page "Sichern & Zurücksetzen" (4.12: `_backup_section`): what a
    /// backup is, and both ways side by side. ⚠ The buttons go **straight** to the dialog of the file; the choice
    /// between the two ways is only in the foot of the window, where one button has to serve both.
    pub(super) fn backup_section(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        crate::settings::setting_group(ui, scheme, &t("s_si_sicherung"));
        ui.add(egui::Label::new(t("sich_lead")).wrap());
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(t("sich_was")).small().color(color(scheme.sub))).wrap());
        ui.add_space(6.0);
        let (mut write, mut read) = (false, false);
        let (write_label, read_label) = (t("sich_schreiben"), t("sich_lesen"));
        ui.horizontal_wrapped(|row| {
            let button = widgets::Button::new(&write_label, ButtonKind::Strong, scheme).icon("download");
            write = row.add(button).clicked();
            let button = widgets::Button::new(&read_label, ButtonKind::Normal, scheme).icon("archive-restore");
            read = row.add(button).clicked();
        });
        if write {
            self.open_backup_write(ui.ctx());
        } else if read {
            self.open_backup_read(ui.ctx());
        }
        self.auto_backup_section(ui, scheme);
    }

    /// The backups the program writes by itself: when (start, end of the game), how many stay, and the way to put one
    /// back (the dialog opens in their folder).
    fn auto_backup_section(&mut self, ui: &mut egui::Ui, scheme: &Scheme) {
        let Some(store) = &self.store else { return };
        let paths = &store.paths;
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(t("sich_auto_titel")).strong()).wrap());
        for (trigger, key) in [(Trigger::Start, "sich_auto_start"), (Trigger::GameEnd, "sich_auto_spielende")] {
            let mut on = backup_auto::enabled(paths, trigger);
            if widgets::switch_row(ui, scheme, &t(key), &mut on).changed() {
                backup_auto::set_enabled(paths, trigger, on);
            }
        }
        let mut keep = i64::try_from(backup_auto::keep(paths)).unwrap_or(backup_auto::KEEP_DEFAULT);
        let label = tf("sich_auto_behalten", &[("0", Arg::from(keep))]);
        ui.horizontal_wrapped(|row| {
            row.add(egui::Label::new(&label));
            let slider = widgets::Slider::new(&mut keep, backup_auto::KEEP_MIN, backup_auto::KEEP_MAX, scheme).on_release();
            if row.add(slider).changed() {
                backup_auto::set_keep(paths, keep);
            }
        });
        ui.add(egui::Label::new(RichText::new(t("sich_auto_hinweis")).small().color(color(scheme.sub))).wrap());
        ui.add_space(4.0);
        let folder = backup_auto::folder(paths);
        let read = ui
            .add(widgets::Button::new(&t("sich_auto_lesen"), ButtonKind::Normal, scheme).icon("clock"))
            .clicked();
        if read && self.backup_free() {
            let start = std::path::Path::new(&folder).is_dir().then_some(folder.as_str());
            let pending = file_dialog::open_file(ui.ctx(), &t("sich_auto_lesen"), ZIP_FILES, start);
            self.backup.phase = Phase::Opening(pending);
        }
    }

    /// Is a backup dialog open (or a thread at work)?
    pub fn backup_open(&self) -> bool {
        !matches!(self.backup.phase, Phase::Idle)
    }

    /// Does a thread work on the backup right now? Then the window must not close: a file would be cut.
    pub fn backup_working(&self) -> bool {
        matches!(self.backup.phase, Phase::Working(..))
    }

    /// Says something in the line of the window; a fault shows with the warning symbol.
    fn say_backup(&mut self, ctx: &egui::Context, text: String, fault: bool) {
        self.pin_fault = fault.then(|| text.clone());
        self.pin_notice = Some((ctx.input(|i| i.time), text));
    }

    /// Starts `work` on a copy of the paths in a thread of its own, under the same lock of writing.
    fn start_backup_work<T: Send + 'static>(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        work: impl FnOnce(&Paths) -> T + Send + 'static,
        done: impl FnOnce(T) -> Done + Send + 'static,
    ) {
        let Some(store) = &self.store else { return };
        let paths = store.paths.working_copy();
        let guard = writeguard::state();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let started = std::thread::Builder::new().name("sicherung".into()).spawn_with_language(move || {
            writeguard::adopt(&guard);
            // An update waits while this mark lies there: the installer would end the program in the middle.
            let _mark = versekit_core::backup::RunningMark::begin(&paths, versekit_core::updater::now());
            let _ = tx.send(done(work(&paths)));
            ctx.request_repaint();
        });
        self.backup.phase = if started.is_ok() { Phase::Working(rx, label) } else { Phase::Idle };
    }

    /// Draws what the flow needs and moves it on (nothing while it is idle).
    pub(super) fn show_backup(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        match std::mem::take(&mut self.backup.phase) {
            Phase::Idle => {}
            Phase::Choose => self.backup_choose(ctx, scheme),
            Phase::Saving(pending) => match pending.take() {
                None => self.backup.phase = Phase::Saving(pending),
                Some(None) => {}
                Some(Some(path)) => {
                    let (target, version) = (with_suffix(path), env!("CARGO_PKG_VERSION"));
                    let on = target.clone();
                    self.start_backup_work(
                        ctx,
                        "sich_arbeitet",
                        // The files of the game of every installed channel come along whenever there are any.
                        move |paths| backup::write_reporting(paths, &on, version, &backup_game::bindings_all(paths)),
                        move |outcome| Done::Written { target, outcome },
                    );
                }
            },
            Phase::Opening(pending) => match pending.take() {
                None => self.backup.phase = Phase::Opening(pending),
                Some(None) => {}
                Some(Some(source)) => {
                    let on = source.clone();
                    self.start_backup_work(
                        ctx,
                        "sich_arbeitet",
                        // The look at what a restore would do is taken with the check, before the question.
                        move |paths| {
                            let found = backup::check(&on);
                            let look = match &found {
                                Some(_) => Look {
                                    data: backup::preview_data(paths, &on).unwrap_or_default(),
                                    game: backup_game::preview(paths, &on),
                                },
                                None => Look::default(),
                            };
                            (found, look)
                        },
                        move |(found, look)| Done::Checked { source, found, look },
                    );
                }
            },
            Phase::Working(rx, label) => {
                match rx.try_recv() {
                    Ok(done) => self.backup_done(ctx, done),
                    Err(TryRecvError::Empty) => {
                        self.backup.phase = Phase::Working(rx, label);
                        ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    }
                    // The thread ended without an answer: said, not swallowed.
                    Err(TryRecvError::Disconnected) => {
                        self.say_backup(ctx, tf("sich_fehler", &[("0", Arg::from("?"))]), true);
                    }
                }
                if matches!(self.backup.phase, Phase::Working(..)) {
                    Self::backup_waiting(ctx, scheme, label);
                }
            }
            Phase::Asking { source, found, look } => self.backup_ask(ctx, scheme, source, found, look),
            Phase::AskProfiles(ask) => self.backup_ask_profiles(ctx, scheme, ask),
            Phase::AskActive(ask) => self.backup_ask_active(ctx, scheme, ask),
            Phase::Told { text, count } => self.backup_told(ctx, scheme, text, count),
        }
    }

    fn backup_choose(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let ((write, read, cancel), close) = widgets::dialog(ctx, scheme, "sicherung", |ui| {
            widgets::heading(ui, scheme, &t("sich_titel"));
            ui.add(egui::Label::new(t("sich_lead")).wrap());
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(t("sich_was")).small().color(color(scheme.sub))).wrap());
            ui.add_space(10.0);
            let write = ui
                .add(widgets::Button::new(&t("sich_schreiben"), ButtonKind::Strong, scheme).icon("download").wide())
                .clicked();
            let read = ui
                .add(widgets::Button::new(&t("sich_lesen"), ButtonKind::Normal, scheme).icon("archive-restore").wide())
                .clicked();
            ui.add_space(4.0);
            let cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
            (write, read, cancel)
        });
        if write {
            let pending = file_dialog::save_file(ctx, &t("sich_schreiben"), ZIP_FILES, None, &backup::suggestion());
            self.backup.phase = Phase::Saving(pending);
        } else if read {
            self.backup.phase = Phase::Opening(file_dialog::open_file(ctx, &t("sich_lesen"), ZIP_FILES, None));
        } else if !(cancel || close) {
            self.backup.phase = Phase::Choose;
        }
    }

    fn backup_waiting(ctx: &egui::Context, scheme: &Scheme, label: &str) {
        widgets::dialog(ctx, scheme, "sicherung-arbeit", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(color(scheme.accent)));
                ui.add(egui::Label::new(t(label)).wrap());
            });
        });
    }

    /// A thread ended: the next step.
    fn backup_done(&mut self, ctx: &egui::Context, done: Done) {
        // The next dialog (the question, the note) is drawn in the next frame: ask for it.
        ctx.request_repaint();
        match done {
            Done::Written { target, outcome } => {
                let name = std::path::Path::new(&target)
                    .file_name()
                    .map_or_else(|| target.clone(), |n| n.to_string_lossy().into_owned());
                match outcome {
                    Ok(written) => {
                        let mut said =
                            tf("sich_fertig", &[("0", Arg::from(written.count)), ("1", Arg::from(name))]);
                        // ⚠ A backup with files missing is no plain success: said as a warning, with the names.
                        let left = written.left_out.len();
                        if left > 0 {
                            let mut names = written.left_out.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
                            if left > 5 {
                                names.push_str(", …");
                            }
                            said = format!(
                                "{said}\n{}",
                                tf("sich_ausgelassen", &[("0", Arg::from(names)), ("1", Arg::from(left))])
                            );
                        }
                        self.say_backup(ctx, said, left > 0);
                        // The overview counts from the time of the last backup: take the counts again.
                        self.overview = None;
                    }
                    Err(WriteError::Empty) => self.say_backup(ctx, t("sich_leer"), true),
                    Err(WriteError::Failed(why)) => {
                        self.say_backup(ctx, tf("sich_fehler", &[("0", Arg::from(why))]), true);
                    }
                }
            }
            Done::Checked { source, found, look } => match found {
                None => self.say_backup(ctx, t("sich_ungueltig"), true),
                Some(found) => self.backup.phase = Phase::Asking { source, found, look },
            },
            Done::Restored { source, outcome, controls: (active, profiles), channels } => {
                // The settings were written by another handle: read them anew.
                if let Some(store) = &self.store {
                    store.paths.forget_settings();
                }
                match outcome {
                    Err(RestoreError::Invalid) => self.say_backup(ctx, t("sich_ungueltig"), true),
                    Err(RestoreError::Failed(why)) => {
                        self.say_backup(ctx, tf("sich_fehler", &[("0", Arg::from(why))]), true);
                    }
                    Ok(restored) => {
                        // ⚠⚠ The controls come apart from the data and only on request (4.12): they lie in the game
                        // folder, not in the data folder. First the saved profiles (they only lie there until one is
                        // loaded), then, separately, the active binding.
                        let ask = ControlsAsk { source, restored, active, profiles, channels, profiles_wanted: false };
                        if ask.has_additive() {
                            self.backup.phase = Phase::AskProfiles(ask);
                        } else if ask.has_active() {
                            self.backup.phase = Phase::AskActive(ask);
                        } else {
                            self.backup_controls_over(ctx, &ask.restored, None);
                        }
                    }
                }
            }
            Done::Controls { restored, outcome, by_channel } => {
                let said = match outcome {
                    Ok(done) => {
                        let mut parts = Vec::new();
                        if done.count > 0 {
                            let key = if by_channel { "sich_spiel_ok" } else { "sich_belegung_ok" };
                            parts.push(tf(key, &[("0", Arg::from(done.count))]));
                        }
                        if !done.missing.is_empty() {
                            parts.push(tf("sich_spiel_fehlt", &[("0", Arg::from(done.missing.join(", ")))]));
                        }
                        if parts.is_empty() {
                            parts.push(t("sich_belegung_ohne_ort"));
                        }
                        parts.join("\n")
                    }
                    Err(BindingsError::GameNotQuiet(reason)) => tf("sich_belegung_spiel", &[("0", Arg::from(reason))]),
                    Err(BindingsError::Failed(why)) => tf("sich_belegung_fehler", &[("0", Arg::from(why))]),
                    Err(BindingsError::Invalid) => t("sich_ungueltig"),
                };
                // ⚠ A fault here does not undo the restore: the data are back already (4.12).
                self.backup_controls_over(ctx, &restored, Some(said));
            }
        }
    }

    /// The questions about the controls are over (answered, or none to ask): what is to be told goes before the
    /// program starts anew, or it starts at once.
    fn backup_controls_over(&mut self, ctx: &egui::Context, restored: &Restored, controls: Option<String>) {
        let text = told_after_restore(restored, controls.as_deref());
        if text.is_empty() {
            self.backup_finished(ctx, restored.count);
        } else {
            self.backup.phase = Phase::Told { text, count: restored.count };
        }
    }

    /// Puts the controls back in a thread of its own: the saved profiles, and the active binding with
    /// `with_active`.
    fn backup_restore_controls(&mut self, ctx: &egui::Context, ask: ControlsAsk, with_active: bool) {
        let by_channel = ask.by_channel();
        let ControlsAsk { source, restored, .. } = ask;
        self.start_backup_work(
            ctx,
            "sich_arbeitet",
            move |paths| backup_game::restore(paths, &source, with_active),
            move |outcome| Done::Controls { restored, outcome, by_channel },
        );
    }

    /// "The backup also holds your controls: …. Put the saved profiles back?"
    fn backup_ask_profiles(&mut self, ctx: &egui::Context, scheme: &Scheme, mut ask: ControlsAsk) {
        let ((no, yes), close) = widgets::dialog(ctx, scheme, "sicherung-profile", |ui| {
            widgets::heading(ui, scheme, &t("sich_titel"));
            ui.add(egui::Label::new(ask.additive_question()).wrap());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let no = ui.add(widgets::Button::new(&t("e_nein"), ButtonKind::Normal, scheme)).clicked();
                let yes = ui.add(widgets::Button::new(&t("e_ja"), ButtonKind::Strong, scheme)).clicked();
                (no, yes)
            })
            .inner
        });
        if yes {
            ask.profiles_wanted = true;
            if ask.has_active() {
                self.backup.phase = Phase::AskActive(ask);
            } else {
                self.backup_restore_controls(ctx, ask, false);
            }
        } else if no || close {
            // No to the profiles is no to the controls: the active binding is not even asked about.
            self.backup_controls_over(ctx, &ask.restored, None);
        } else {
            self.backup.phase = Phase::AskProfiles(ask);
        }
    }

    /// "Replace the ACTIVE bindings as well?" No is the way of least harm and stands first and strong.
    fn backup_ask_active(&mut self, ctx: &egui::Context, scheme: &Scheme, ask: ControlsAsk) {
        let ((no, yes), close) = widgets::dialog(ctx, scheme, "sicherung-aktiv", |ui| {
            widgets::heading(ui, scheme, &t("sich_titel"));
            ui.add(egui::Label::new(ask.active_question()).wrap());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let no = ui.add(widgets::Button::new(&t("e_nein"), ButtonKind::Strong, scheme)).clicked();
                let yes = ui.add(widgets::Button::new(&t("e_ja"), ButtonKind::Danger, scheme)).clicked();
                (no, yes)
            })
            .inner
        });
        if yes {
            self.backup_restore_controls(ctx, ask, true);
        } else if no || close {
            if ask.profiles_wanted {
                self.backup_restore_controls(ctx, ask, false);
            } else {
                self.backup_controls_over(ctx, &ask.restored, None);
            }
        } else {
            self.backup.phase = Phase::AskActive(ask);
        }
    }

    /// The restore is over and told: the program starts anew, or the player is asked to.
    fn backup_finished(&mut self, ctx: &egui::Context, count: usize) {
        if self.backup.can_restart {
            let said = tf("sich_zurueck_ok", &[("0", Arg::from(count))]);
            self.say_backup(ctx, said, false);
            self.backup.restart = true;
            self.backup.locked = true;
        } else {
            self.say_backup(ctx, t("sich_neustart_selbst"), false);
        }
    }

    /// "Restore the backup of … with N files?": the state of before is put aside, and the program starts anew.
    fn backup_ask(&mut self, ctx: &egui::Context, scheme: &Scheme, source: String, found: Found, look: Look) {
        let ((cancel, restore), close) = widgets::dialog(ctx, scheme, "sicherung-frage", |ui| {
            widgets::heading(ui, scheme, &t("sich_titel"));
            let when = if found.when.is_empty() { "?".to_string() } else { found.when.clone() };
            let ask = tf("sich_frage", &[("0", Arg::from(when)), ("1", Arg::from(found.count))]);
            ui.add(egui::Label::new(ask).wrap());
            ui.add_space(6.0);
            // ⚠ What will be replaced is shown BEFORE the yes: counted, and file by file on request.
            for line in look_lines(&look) {
                ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.sub))).wrap());
            }
            let changes = look_changes(&look);
            if !changes.is_empty() {
                egui::CollapsingHeader::new(t("sich_vorschau_liste")).id_salt("sicherung-vorschau").show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("sicherung-vorschau-liste").max_height(180.0).show(ui, |ui| {
                        for (change, name) in &changes {
                            let line = format!("{}: {name}", change_word(*change));
                            ui.add(egui::Label::new(RichText::new(line).small()).wrap());
                        }
                    });
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let cancel = ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked();
                let restore =
                    ui.add(widgets::Button::new(&t("s_mu_rest_einspielen"), ButtonKind::Danger, scheme)).clicked();
                (cancel, restore)
            })
            .inner
        });
        if restore {
            let on = source.clone();
            self.start_backup_work(
                ctx,
                "sich_arbeitet",
                // The copy of before carries the player's files of the game too: a slip costs nothing.
                move |paths| backup::restore(paths, &on, &backup_game::bindings_all(paths)),
                move |outcome| {
                    let controls = backup::bindings_in_archive(&source);
                    let channels = backup_game::in_archive(&source);
                    Done::Restored { source, outcome, controls, channels }
                },
            );
        } else if !(cancel || close) {
            self.backup.phase = Phase::Asking { source, found, look };
        }
    }

    fn backup_told(&mut self, ctx: &egui::Context, scheme: &Scheme, text: String, count: usize) {
        // Only the button closes it: the program starts anew with it, and a slip beside the box must not do that
        // before the text was read.
        let (done, _) = widgets::dialog(ctx, scheme, "sicherung-hinweis", |ui| {
            widgets::heading(ui, scheme, &t("sich_titel"));
            ui.add(egui::Label::new(&text).wrap());
            ui.add_space(8.0);
            ui.add(widgets::Button::new(&t("hf_schliessen"), ButtonKind::Strong, scheme)).clicked()
        });
        if done {
            self.backup_finished(ctx, count);
        } else {
            self.backup.phase = Phase::Told { text, count };
        }
    }
}

/// What the player is told after a restore: what became of the controls (where they were asked about), then
/// what was switched off or not taken over. Nothing when there is nothing to tell.
fn told_after_restore(done: &Restored, controls: Option<&str>) -> String {
    let mut parts = Vec::new();
    parts.extend(controls.map(str::to_string));
    if !done.disarmed.is_empty() {
        let names: Vec<String> = done
            .disarmed
            .iter()
            .map(|name| {
                let shown = match name.as_str() {
                    game_start::STARTER_SETTING => t("sich_startbefehl"),
                    game_ini::SETTING_CUSTOM => t("sich_eigene_quelle"),
                    other => other.to_string(),
                };
                format!("• {shown}")
            })
            .collect();
        parts.push(tf("sich_befehle_aus", &[("0", Arg::from(names.join("\n")))]));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use egui::Pos2;
    use versekit_core::language;

    use super::super::controls::tests::Bench;
    use super::super::real_data::{folder, host_in};
    use super::*;

    fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    /// What a binding file of the game says: a real root, and a mark that tells the files apart.
    fn xml(mark: &str) -> String {
        format!(
            "<ActionMaps>\n <ActionProfiles profileName=\"default\"><actionmap name=\"{mark}\"/></ActionProfiles>\n</ActionMaps>\n"
        )
    }

    /// The game folder of a throwaway folder: `<folder>/spiel/LIVE`, with the active binding, the settings of the game
    /// and two saved profiles (all marked `mark`), and a `Game.log` last written `age` seconds ago.
    fn made_up_game(folder: &Path, mark: &str, age: u64) -> PathBuf {
        let game = folder.join("spiel").join("LIVE");
        let zero = game.join("USER").join("Client").join("0");
        std::fs::create_dir_all(zero.join("Profiles").join("default")).unwrap();
        std::fs::create_dir_all(zero.join("controls").join("mappings")).unwrap();
        let put = |rel: &str, text: String| std::fs::write(zero.join(rel), text).unwrap();
        put("Profiles/default/actionmaps.xml", xml(&format!("aktiv-{mark}")));
        put("Profiles/default/attributes.xml", format!("<Attributes><Attr name=\"{mark}\"/></Attributes>"));
        put("controls/mappings/Kampf.xml", xml(&format!("kampf-{mark}")));
        put("controls/mappings/Bergbau.xml", xml(&format!("bergbau-{mark}")));
        std::fs::write(game.join("Game.log"), "x").unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(game.join("Game.log"))
            .and_then(|file| file.set_modified(at))
            .unwrap();
        game
    }

    fn control(game: &Path, rel: &str) -> String {
        std::fs::read_to_string(game.join("USER").join("Client").join("0").join(rel)).unwrap_or_default()
    }

    /// A page on real data in a throwaway folder: `<folder>/daten` is the data folder, the copy of before lies
    /// next to it in `<folder>`; the game folder is `<folder>/spiel/LIVE` with controls marked `mark` (the game has
    /// been quiet for two hours). ⚠ A test never reads the controls of the real game folder: even the copy of
    /// before takes them along.
    fn controls_page(name: &str, mark: &str, age: u64) -> (SamplePage, PathBuf, PathBuf, PathBuf) {
        let folder = folder(name);
        let data = folder.join("daten");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("einstellungen.json"), "{}\n").unwrap();
        let game = made_up_game(&folder, mark, age);
        let live = game.to_string_lossy().into_owned();
        let vars = [(versekit_core::paths::ENV_INSTALL_DIR, live.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        versekit_core::theme::set_current("original");
        language::set_thread_language(language::Choice::De);
        let mut page = SamplePage::with_settings(Paths::new(host_in(&data, &vars)));
        page.offer_restart(true);
        (page, folder, data, game)
    }

    /// [`controls_page`] for the tests that do not care about the controls.
    fn backup_page(name: &str) -> (SamplePage, PathBuf, PathBuf) {
        let (page, folder, data, _game) = controls_page(name, "x", 7200);
        (page, folder, data)
    }

    /// Frames until `done` says so, at most a minute: the steps run in threads of their own and take as long as a
    /// busy processor lets them. A step that never ends is told by the caller's asserts, not by a count of frames.
    fn frames_until(bench: &mut Bench, done: impl Fn(&Bench) -> bool) -> Vec<(String, Pos2)> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut texts = bench.settle();
        while !done(bench) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            texts = bench.settle();
        }
        texts
    }

    /// Frames until no dialog and no thread is left (the threads need a moment).
    fn finish(bench: &mut Bench) -> Vec<(String, Pos2)> {
        frames_until(bench, |bench| !bench.page.backup_open())
    }

    fn files_named(folder: &Path, start: &str) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(start))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn the_button_in_the_foot_opens_the_choice_with_both_ways_and_nothing_else_happens() {
        let _language = crate::language_lock();
        let (page, folder, data) = backup_page("sicherung-knopf");
        let mut bench = Bench::new(page);
        let before = bench.settle();
        assert!(!bench.page.backup_open());
        // The overview has a tile of the same name; the button is the one in the foot, the lowest of them.
        let label = language::t("hf_sicherung");
        let button = before.iter().filter(|(t, _)| *t == label).map(|(_, at)| *at).max_by(|a, b| a.y.total_cmp(&b.y));
        bench.click_at(button.expect("the button in the foot"));
        let texts = bench.settle();
        assert!(bench.page.backup_open(), "the choice is open");
        assert!(
            shows(&texts, &language::t("sich_schreiben")) && shows(&texts, &language::t("sich_lesen")),
            "{texts:?}"
        );
        assert!(shows(&texts, &language::t("sich_lead")));
        assert_eq!(files_named(&folder, ""), ["daten", "spiel"], "only the two folders: nothing was written");
        assert!(data.join("einstellungen.json").exists());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_backup_is_written_where_the_player_picked_with_the_ending_and_told_in_the_line() {
        let _language = crate::language_lock();
        let (page, folder, data) = backup_page("sicherung-schreiben");
        std::fs::create_dir_all(data.join("Bestand")).unwrap();
        std::fs::write(data.join("Bestand").join("x.json"), "[1]").unwrap();
        let mut bench = Bench::new(page);
        bench.page.backup.phase =
            Phase::Saving(Pending::answered(Some(folder.join("meine").to_string_lossy().into_owned())));
        finish(&mut bench);
        let target = folder.join("meine.zip");
        assert!(target.exists(), "the ending is added: {:?}", files_named(&folder, ""));
        let found = backup::check(&target.to_string_lossy()).expect("a backup of this program");
        let bytes = std::fs::read(&target).unwrap();
        let names: Vec<String> =
            versekit_core::zip::entries(&bytes).unwrap().into_iter().map(|entry| entry.name).collect();
        assert!(names.contains(&"Bestand/x.json".to_string()), "{names:?}");
        assert_eq!(found.count, names.len() - 1, "the attachment is not counted");
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone()).unwrap_or_default();
        assert!(said.contains("meine.zip") && said.contains(&found.count.to_string()), "{said}");
        assert!(bench.page.pin_fault.is_none());
        assert!(!bench.page.take_restart(), "writing restarts nothing");
        // Cancelling the dialog of the place writes nothing.
        bench.page.backup.phase = Phase::Saving(Pending::answered(None));
        finish(&mut bench);
        assert_eq!(files_named(&folder, "meine"), ["meine.zip"]);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_backup_with_a_file_left_out_is_told_as_a_warning_with_the_name() {
        let _language = crate::language_lock();
        let (page, folder, data) = backup_page("sicherung-ausgelassen");
        std::fs::create_dir_all(data.join("Bestand")).unwrap();
        std::fs::write(data.join("Bestand").join("x.json"), "[1]").unwrap();
        // A file over the limit of the reader; a hole in the file system, nothing is read of it.
        std::fs::create_dir_all(data.join("Intern")).unwrap();
        let big = std::fs::File::create(data.join("Intern").join("riesig.json")).unwrap();
        big.set_len(versekit_core::zip::MAX_ENTRY as u64 + 1).unwrap();
        drop(big);
        let mut bench = Bench::new(page);
        bench.page.backup.phase =
            Phase::Saving(Pending::answered(Some(folder.join("meine").to_string_lossy().into_owned())));
        finish(&mut bench);
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone()).unwrap_or_default();
        assert!(said.contains("riesig.json"), "the missing file is named: {said}");
        assert!(bench.page.pin_fault.is_some(), "a backup with a hole is not a plain success");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_file_that_is_no_backup_is_refused_before_the_player_is_asked() {
        let _language = crate::language_lock();
        let (page, folder, data) = backup_page("sicherung-ungueltig");
        let wrong = folder.join("kein.zip");
        std::fs::write(&wrong, "kein zip").unwrap();
        let mut bench = Bench::new(page);
        // (The page writes its own settings file at the start: what counts is that the refusal changes nothing.)
        bench.settle();
        let before = std::fs::read_to_string(data.join("einstellungen.json")).unwrap();
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(wrong.to_string_lossy().into_owned())));
        let texts = finish(&mut bench);
        assert!(!shows(&texts, &language::t("s_mu_rest_einspielen")), "no question for a wrong file");
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone()).unwrap_or_default();
        assert_eq!(said, language::t("sich_ungueltig"));
        assert!(bench.page.pin_fault.is_some(), "a refusal shows as a fault");
        assert!(!bench.page.take_restart());
        assert_eq!(std::fs::read_to_string(data.join("einstellungen.json")).unwrap(), before, "nothing touched");
        assert!(files_named(&folder, "VerseKit-vorher").is_empty(), "no copy of before for a refusal");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_backup_is_restored_only_when_the_player_says_yes_and_the_state_of_before_is_put_aside() {
        let _language = crate::language_lock();
        let (page, folder, data) = backup_page("sicherung-einspielen");
        std::fs::create_dir_all(data.join("Bestand")).unwrap();
        std::fs::write(data.join("Bestand").join("x.json"), "alt").unwrap();
        let zip = folder.join("stand.zip").to_string_lossy().into_owned();
        assert!(backup::write(&page.store.as_ref().unwrap().paths, &zip, "5.0", &[]).is_ok());
        std::fs::write(data.join("Bestand").join("x.json"), "neu").unwrap();
        let mut bench = Bench::new(page);
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip.clone())));
        let texts = finish_until_asked(&mut bench);
        // The question names the date and the number of files, and the file is still the new one.
        assert!(shows(&texts, &language::t("s_mu_rest_einspielen")), "{texts:?}");
        assert!(shows(&texts, &language::t("abbrechen")));
        assert_eq!(std::fs::read_to_string(data.join("Bestand").join("x.json")).unwrap(), "neu");
        // No: nothing happens.
        bench.click(&language::t("abbrechen"));
        finish(&mut bench);
        assert_eq!(std::fs::read_to_string(data.join("Bestand").join("x.json")).unwrap(), "neu");
        assert!(files_named(&folder, "VerseKit-vorher").is_empty());
        assert!(!bench.page.take_restart());
        // Yes: the old state is back, the new one lies aside, and the program is asked to start anew.
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip)));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        finish(&mut bench);
        assert_eq!(std::fs::read_to_string(data.join("Bestand").join("x.json")).unwrap(), "alt");
        let aside = files_named(&folder, "VerseKit-vorher");
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert!(bench.page.take_restart(), "the program starts anew");
        assert!(!bench.page.take_restart(), "once");
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone()).unwrap_or_default();
        assert!(said.contains("Dateien eingespielt"), "{said}");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn every_dialog_of_the_backup_fits_the_window_in_both_languages() {
        use language::Choice;
        // The visible part of the lines: what a box clips is not seen.
        fn rows(shape: &egui::epaint::Shape, clip: egui::Rect, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    for placed in &text.galley.rows {
                        let line = placed.row.text().to_string();
                        if !line.trim().is_empty() {
                            let rect =
                                placed.rect_without_leading_space().translate(text.pos.to_vec2()).intersect(clip);
                            if rect.is_positive() {
                                out.push((line, rect));
                            }
                        }
                    }
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| rows(s, clip, out)),
                _ => {}
            }
        }
        let _language = crate::language_lock();
        let (mut page, folder, _data) = backup_page("sicherung-mass");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let draw = |page: &mut SamplePage, width: f32| -> Vec<(String, egui::Rect)> {
            let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 900.0));
            let mut lines = Vec::new();
            for _ in 0..4 {
                let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                    page.show(ui);
                });
                out.textures_delta.clear();
                lines.clear();
                out.shapes.iter().for_each(|c| rows(&c.shape, c.clip_rect, &mut lines));
            }
            lines
        };
        let (_keep, rx) = mpsc::channel::<Done>();
        let mut rx = Some(rx);
        let mut findings = Vec::new();
        for lang in [Choice::De, Choice::En] {
            for width in [900.0_f32, 1300.0] {
                language::set_thread_language(lang);
                page.backup.phase = Phase::Idle;
                let behind = draw(&mut page, width);
                let restored = || Restored {
                    count: 9,
                    fallback: None,
                    disarmed: vec![
                        "Ein Startprogramm mit einem recht langen Namen (C:\\Programme\\Werkzeug\\werkzeug.exe)"
                            .to_string(),
                        game_start::STARTER_SETTING.to_string(),
                        game_ini::SETTING_CUSTOM.to_string(),
                    ],
                };
                // The longest things the dialogs hold: the sentence about the game, and many names of profiles.
                let said = tf("sich_belegung_spiel", &[("0", Arg::from(t("s_gf_unklar")))]);
                let told = told_after_restore(&restored(), Some(&said));
                let ask = || ControlsAsk {
                    source: "x.zip".to_string(),
                    restored: restored(),
                    active: true,
                    profiles: [
                        "Kampf",
                        "Bergbau",
                        "Frachtflug",
                        "Rennen",
                        "Handel",
                        "Erkundung",
                        "Bergung",
                        "Piraterie",
                    ]
                    .map(String::from)
                    .to_vec(),
                    channels: Vec::new(),
                    profiles_wanted: true,
                };
                // By channel: every channel with many profiles and templates.
                let by_channel = || ControlsAsk {
                    channels: ["LIVE", "HOTFIX", "PTU"]
                        .map(|channel| ChannelContents {
                            channel: channel.to_string(),
                            active: true,
                            game_settings: true,
                            user_cfg: true,
                            profiles: (0..40).map(|n| format!("Profil {n}")).collect(),
                            templates: (0..12).map(|n| format!("Vorlage {n}")).collect(),
                        })
                        .to_vec(),
                    ..ask()
                };
                // The longest look: many files of the data, every channel, one of them not installed here.
                let look = || {
                    let item = |name: String, change| PreviewItem { name, change };
                    let changes = [Change::New, Change::Replaced, Change::Same];
                    let data = (0..60)
                        .map(|n| item(format!("Bauplaene/ein-recht-langer-name-einer-datei-{n}.json"), changes[n % 3]))
                        .collect();
                    let game = ["LIVE", "PTU"]
                        .map(|channel| ChannelPreview {
                            channel: channel.to_string(),
                            installed: channel == "LIVE",
                            additive: (0..30)
                                .map(|n| item(format!("mappings/profil-{n}.xml"), Change::New))
                                .collect(),
                            active: vec![item("user.cfg".to_string(), Change::Replaced)],
                        })
                        .to_vec();
                    Look { data, game }
                };
                for which in 0..8 {
                    page.backup.phase = match which {
                        0 => Phase::Choose,
                        1 => Phase::Working(rx.take().unwrap_or_else(|| mpsc::channel().1), "sich_arbeitet"),
                        2 => Phase::Asking {
                            source: "x.zip".to_string(),
                            found: Found { count: 1234, when: "01.02.2026 10:11".to_string() },
                            look: look(),
                        },
                        3 => Phase::AskProfiles(ask()),
                        4 => Phase::AskActive(ask()),
                        5 => Phase::AskProfiles(by_channel()),
                        6 => Phase::AskActive(by_channel()),
                        _ => Phase::Told { text: told.clone(), count: 9 },
                    };
                    let lines = draw(&mut page, width);
                    let mine: Vec<&(String, egui::Rect)> = lines
                        .iter()
                        .filter(|(text, rect)| {
                            !behind.iter().any(|(t, r)| t == text && (r.min - rect.min).length() < 1.0)
                        })
                        .collect();
                    let at = format!("{lang:?} Breite {width} Fenster {which}");
                    if mine.is_empty() {
                        findings.push(format!("{at}: nothing drawn"));
                    }
                    for (text, rect) in &mine {
                        if rect.right() > width + 0.5 || rect.bottom() > 900.5 {
                            findings.push(format!("{at}: {text:?} leaves the window"));
                        }
                    }
                    'pairs: for (i, (a, ra)) in mine.iter().enumerate() {
                        for (b, rb) in &mine[i + 1..] {
                            let both = ra.intersect(*rb);
                            if both.width() > 1.0 && both.height() > 1.0 {
                                findings.push(format!("{at}: {a:?} {ra:?} lies on {b:?} {rb:?}"));
                                break 'pairs;
                            }
                        }
                    }
                    if which == 1
                        && let Phase::Working(back, _) = std::mem::take(&mut page.backup.phase)
                    {
                        rx = Some(back);
                    }
                }
            }
        }
        language::set_thread_language(Choice::De);
        let _ = std::fs::remove_dir_all(folder);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Frames until the question is there (the look into the file runs in a thread).
    fn finish_until_asked(bench: &mut Bench) -> Vec<(String, Pos2)> {
        // The texts of the last frames are returned, not those of the frame before the question came: the
        // question can arrive in the last of the four frames of a settle.
        frames_until(bench, |bench| matches!(bench.page.backup.phase, Phase::Asking { .. }));
        bench.settle()
    }

    #[test]
    fn a_window_that_cannot_start_itself_anew_asks_the_player_to_do_it() {
        let _language = crate::language_lock();
        let (mut page, folder, data) = backup_page("sicherung-ohne-neustart");
        page.offer_restart(false);
        let zip = folder.join("stand.zip").to_string_lossy().into_owned();
        std::fs::write(data.join("a.json"), "1").unwrap();
        assert!(backup::write(&page.store.as_ref().unwrap().paths, &zip, "", &[]).is_ok());
        let mut bench = Bench::new(page);
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip)));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        finish(&mut bench);
        assert!(!bench.page.take_restart());
        let said = bench.page.pin_notice.as_ref().map(|(_, text)| text.clone()).unwrap_or_default();
        assert_eq!(said, language::t("sich_neustart_selbst"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn what_was_not_taken_over_is_told_before_the_program_starts_anew() {
        let _language = crate::language_lock();
        let (source, folder_a, data_a) = backup_page("sicherung-quelle");
        std::fs::write(
            data_a.join("einstellungen.json"),
            "{\"startprogramme\": [{\"name\": \"Tool\", \"datei\": \"C:\\\\x\\\\tool.exe\", \"an\": true}]}\n",
        )
        .unwrap();
        let zip = folder_a.join("quelle.zip").to_string_lossy().into_owned();
        assert!(backup::write(&source.store.as_ref().unwrap().paths, &zip, "5.0", &[]).is_ok());
        let (target, folder_b, data_b) = backup_page("sicherung-ziel");
        let mut bench = Bench::new(target);
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip)));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        let mut texts = frames_until(&mut bench, |bench| matches!(bench.page.backup.phase, Phase::Told { .. }));
        texts.extend(bench.settle());
        assert!(shows(&texts, "Tool (C:\\x\\tool.exe)"), "the switched off start program is named: {texts:?}");
        assert!(!bench.page.take_restart(), "not before the player has read it");
        // A slip beside the box does not close it (and so does not start the program anew).
        bench.click_at(Pos2::new(4.0, 4.0));
        bench.settle();
        assert!(matches!(bench.page.backup.phase, Phase::Told { .. }), "the note stays");
        assert!(!bench.page.take_restart(), "and nothing starts anew");
        bench.click(&language::t("hf_schliessen"));
        finish(&mut bench);
        assert!(bench.page.take_restart());
        // The program is starting anew: no second backup is begun meanwhile.
        bench.page.open_backup();
        assert!(!bench.page.backup_open(), "locked while the program starts anew");
        let file = std::fs::read_to_string(data_b.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"an\": false"), "the start program arrived switched off: {file}");
        for folder in [folder_a, folder_b] {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    #[test]
    fn a_file_name_gets_the_ending_of_an_archive() {
        assert_eq!(with_suffix("C:/x/meine".to_string()), "C:/x/meine.zip");
        assert_eq!(with_suffix("C:/x/meine.ZIP".to_string()), "C:/x/meine.ZIP");
        assert_eq!(with_suffix("meine.zip.txt".to_string()), "meine.zip.txt.zip");
    }

    #[test]
    fn the_player_is_told_what_became_of_the_controls_and_what_was_switched_off() {
        let _language = crate::language_lock();
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let done = Restored {
            count: 3,
            fallback: None,
            disarmed: vec!["Tool (C:\\x\\tool.exe)".to_string(), game_start::STARTER_SETTING.to_string()],
        };
        let text = told_after_restore(&done, Some("Steuerung zurückgelegt: 2 Dateien."));
        assert!(text.contains("• Tool (C:\\x\\tool.exe)") && text.contains("• Startbefehl für Star Citizen"), "{text}");
        assert!(text.starts_with("Steuerung zurückgelegt: 2 Dateien."), "the controls come first: {text}");
        assert!(!text.contains("noch nicht zurückgelegt"), "the controls are no longer 'later': {text}");
        let nothing = Restored { count: 1, fallback: None, disarmed: Vec::new() };
        assert_eq!(told_after_restore(&nothing, None), "", "nothing to tell, nothing shown");
    }

    /// A zip archive of these members.
    fn archive_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = versekit_core::zip::Builder::new((2026, 1, 1, 0, 0, 0));
        for (name, content) in members {
            archive.add(name, content).unwrap();
        }
        archive.finish().unwrap()
    }

    /// The names of the files below the `Steuerung/` of an archive, the old way (not by channel).
    fn controls_in(archive: &Path) -> Vec<String> {
        all_controls_in(archive).into_iter().filter(|name| !name.starts_with(backup_game::CHANNELS_PREFIX)).collect()
    }

    /// The names of all files below the `Steuerung/` of an archive, also by channel.
    fn all_controls_in(archive: &Path) -> Vec<String> {
        let data = std::fs::read(archive).unwrap();
        let mut names: Vec<String> = versekit_core::zip::entries(&data)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .filter(|name| name.starts_with("Steuerung/"))
            .collect();
        names.sort();
        names
    }

    const ALL_CONTROLS: [&str; 4] = [
        "Steuerung/actionmaps.xml",
        "Steuerung/attributes.xml",
        "Steuerung/mappings/Bergbau.xml",
        "Steuerung/mappings/Kampf.xml",
    ];

    /// The same files of the made-up game by channel: the game folder `spiel/LIVE` is the channel LIVE.
    const ALL_BY_CHANNEL: [&str; 4] = [
        "Steuerung/Kanaele/LIVE/actionmaps.xml",
        "Steuerung/Kanaele/LIVE/attributes.xml",
        "Steuerung/Kanaele/LIVE/mappings/Bergbau.xml",
        "Steuerung/Kanaele/LIVE/mappings/Kampf.xml",
    ];

    /// The files of the game by channel in an archive.
    fn by_channel_in(archive: &Path) -> Vec<String> {
        all_controls_in(archive).into_iter().filter(|name| name.starts_with(backup_game::CHANNELS_PREFIX)).collect()
    }

    /// The state of a restore of a backup made from a game marked `quelle` into a page whose game is marked `ziel`:
    /// the page with the question for the file already answered.
    fn restoring_page(name: &str, age: u64) -> (Bench, PathBuf, PathBuf, String) {
        let (source, folder_a, _data_a, _game_a) = controls_page(&format!("{name}-quelle"), "quelle", 7200);
        let paths = &source.store.as_ref().unwrap().paths;
        let zip = folder_a.join("quelle.zip").to_string_lossy().into_owned();
        assert!(backup::write(paths, &zip, "5.0", &backup::bindings(paths, None)).is_ok());
        let (target, folder_b, _data_b, game_b) = controls_page(&format!("{name}-ziel"), "ziel", age);
        let mut bench = Bench::new(target);
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip.clone())));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        let _ = folder_a;
        (bench, folder_b, game_b, zip)
    }

    /// Frames until a dialog of this phase is up.
    fn until_phase(bench: &mut Bench, phase: fn(&Phase) -> bool) -> Vec<(String, Pos2)> {
        frames_until(bench, |bench| phase(&bench.page.backup.phase));
        let texts = bench.settle();
        assert!(phase(&bench.page.backup.phase), "the dialog never came: {texts:?}");
        texts
    }

    fn is_profiles(phase: &Phase) -> bool {
        matches!(phase, Phase::AskProfiles(_))
    }

    fn is_active(phase: &Phase) -> bool {
        matches!(phase, Phase::AskActive(_))
    }

    fn is_told(phase: &Phase) -> bool {
        matches!(phase, Phase::Told { .. })
    }

    #[test]
    fn a_backup_made_from_the_page_takes_the_controls_of_the_game_folder_along() {
        let _language = crate::language_lock();
        let (page, folder, data, _game) = controls_page("sicherung-steuerung", "a", 7200);
        std::fs::create_dir_all(data.join("Bestand")).unwrap();
        std::fs::write(data.join("Bestand").join("x.json"), "[1]").unwrap();
        let mut bench = Bench::new(page);
        bench.page.backup.phase =
            Phase::Saving(Pending::answered(Some(folder.join("mit").to_string_lossy().into_owned())));
        finish(&mut bench);
        let target = folder.join("mit.zip");
        assert_eq!(controls_in(&target), ALL_CONTROLS, "the active binding, the game settings and both profiles");
        assert_eq!(by_channel_in(&target), ALL_BY_CHANNEL, "and the same by channel");
        let found = backup::check(&target.to_string_lossy()).expect("a backup of this program");
        let said = bench.notice().unwrap_or_default();
        assert!(said.contains(&found.count.to_string()), "the count in the line says the controls too: {said}");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_copy_of_before_takes_the_controls_along_too() {
        let _language = crate::language_lock();
        let (mut bench, folder, _game, _zip) = restoring_page("sicherung-vorher", 7200);
        // No: only the data are restored, the copy of before is there nonetheless.
        until_phase(&mut bench, is_profiles);
        bench.click(&language::t("e_nein"));
        finish(&mut bench);
        let copies = files_named(&folder, "VerseKit-vorher");
        assert_eq!(copies.len(), 1, "{copies:?}");
        assert_eq!(controls_in(&folder.join(&copies[0])), ALL_CONTROLS, "the controls of before are in the copy");
        assert_eq!(by_channel_in(&folder.join(&copies[0])), ALL_BY_CHANNEL, "by channel too");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_saved_profiles_come_back_on_yes_and_the_active_binding_only_on_a_second_yes() {
        let _language = crate::language_lock();
        let (mut bench, folder, game, _zip) = restoring_page("sicherung-profile", 7200);
        let texts = until_phase(&mut bench, is_profiles);
        let question = language::tf("sich_belegung_frage", &[("0", language::Arg::from("Bergbau, Kampf"))]);
        assert!(
            texts.iter().any(|(text, _)| text.contains("Bergbau, Kampf")),
            "the names are in the question: {texts:?}"
        );
        assert!(shows(&texts, question.lines().next().unwrap_or("")), "{texts:?}");
        assert!(!bench.page.take_restart(), "nothing starts anew before the questions are answered");
        // Yes to the profiles: the second question comes, with No in front.
        bench.click(&language::t("e_ja"));
        let texts = until_phase(&mut bench, is_active);
        assert!(shows(&texts, language::t("sich_belegung_aktiv").lines().next().unwrap_or("")), "{texts:?}");
        assert!(control(&game, "controls/mappings/Kampf.xml").contains("kampf-ziel"), "still nothing written");
        // No to the active binding: the profiles come, the active binding and the settings of the game stay.
        bench.click(&language::t("e_nein"));
        let texts = until_phase(&mut bench, is_told);
        assert!(control(&game, "controls/mappings/Kampf.xml").contains("kampf-quelle"));
        assert!(control(&game, "controls/mappings/Bergbau.xml").contains("bergbau-quelle"));
        assert!(control(&game, "Profiles/default/actionmaps.xml").contains("aktiv-ziel"), "the active one stays");
        assert!(control(&game, "Profiles/default/attributes.xml").contains("\"ziel\""), "and so do the game settings");
        let said = language::tf("sich_belegung_ok", &[("0", language::Arg::from(2))]);
        assert!(shows(&texts, &said), "the player is told how many files: {texts:?}");
        assert!(!shows(&texts, "noch nicht zurückgelegt"), "no 'later' any more: {texts:?}");
        assert!(!bench.page.take_restart(), "not before the note was read");
        bench.click(&language::t("hf_schliessen"));
        finish(&mut bench);
        assert!(bench.page.take_restart(), "then the program starts anew");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_active_binding_and_the_game_settings_come_back_on_the_second_yes_with_a_copy_of_the_old_one() {
        let _language = crate::language_lock();
        let (mut bench, folder, game, _zip) = restoring_page("sicherung-aktiv", 7200);
        let old_active = control(&game, "Profiles/default/actionmaps.xml");
        until_phase(&mut bench, is_profiles);
        bench.click(&language::t("e_ja"));
        until_phase(&mut bench, is_active);
        bench.click(&language::t("e_ja"));
        let texts = until_phase(&mut bench, is_told);
        assert!(control(&game, "Profiles/default/actionmaps.xml").contains("aktiv-quelle"));
        assert!(control(&game, "Profiles/default/attributes.xml").contains("\"quelle\""));
        assert!(control(&game, "controls/mappings/Kampf.xml").contains("kampf-quelle"));
        assert!(shows(&texts, &language::tf("sich_belegung_ok", &[("0", language::Arg::from(4))])), "{texts:?}");
        let zero = game.join("USER").join("Client").join("0").join("Profiles").join("default");
        let copies = files_named(&zero, "actionmaps.xml.");
        assert_eq!(copies.len(), 1, "the old active binding was put aside: {copies:?}");
        assert_eq!(std::fs::read_to_string(zero.join(&copies[0])).unwrap(), old_active);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn no_to_the_profiles_puts_nothing_of_the_controls_back_and_does_not_ask_about_the_active_binding() {
        let _language = crate::language_lock();
        let (mut bench, folder, game, _zip) = restoring_page("sicherung-nein", 7200);
        until_phase(&mut bench, is_profiles);
        bench.click(&language::t("e_nein"));
        finish(&mut bench);
        assert!(!matches!(bench.page.backup.phase, Phase::AskActive(_)), "no second question");
        for (rel, mark) in [
            ("controls/mappings/Kampf.xml", "kampf-ziel"),
            ("controls/mappings/Bergbau.xml", "bergbau-ziel"),
            ("Profiles/default/actionmaps.xml", "aktiv-ziel"),
            ("Profiles/default/attributes.xml", "\"ziel\""),
        ] {
            assert!(control(&game, rel).contains(mark), "{rel} stays");
        }
        assert!(bench.page.take_restart(), "the data are back: the program starts anew");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn while_the_game_runs_the_controls_are_not_written_and_the_player_is_told_why() {
        let _language = crate::language_lock();
        // The log was written a second ago: the game runs.
        let (mut bench, folder, game, _zip) = restoring_page("sicherung-spiel", 0);
        until_phase(&mut bench, is_profiles);
        bench.click(&language::t("e_ja"));
        until_phase(&mut bench, is_active);
        bench.click(&language::t("e_ja"));
        let texts = until_phase(&mut bench, is_told);
        let reason = language::t("s_gf_spiel");
        assert!(texts.iter().any(|(text, _)| text.contains(&reason)), "the reason stands in the note: {texts:?}");
        assert!(shows(&texts, &language::t("sich_belegung_spiel").replace("{0}", "")), "{texts:?}");
        assert!(control(&game, "Profiles/default/actionmaps.xml").contains("aktiv-ziel"));
        assert!(control(&game, "controls/mappings/Kampf.xml").contains("kampf-ziel"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_backup_with_the_active_binding_alone_asks_only_about_that_and_one_without_controls_asks_nothing() {
        let _language = crate::language_lock();
        let (page, folder, _data, game) = controls_page("sicherung-nur-aktiv", "ziel", 7200);
        let info = format!("{}\r\n\r\nErstellt am 01.02.2026 10:11 mit VerseKit 5.\r\n", backup::MARKER);
        let only_active = folder.join("aktiv.zip");
        std::fs::write(
            &only_active,
            archive_of(&[
                ("Steuerung/actionmaps.xml", xml("aktiv-quelle").as_bytes()),
                ("Bestand/x.json", b"[1]"),
                (backup::INFO_FILE, info.as_bytes()),
            ]),
        )
        .unwrap();
        let no_controls = folder.join("ohne.zip");
        std::fs::write(
            &no_controls,
            archive_of(&[("Bestand/x.json", b"[2]"), (backup::INFO_FILE, info.as_bytes())]),
        )
        .unwrap();
        let mut bench = Bench::new(page);
        // No controls in the file: no question at all, the program starts anew.
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(no_controls.to_string_lossy().into_owned())));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        finish(&mut bench);
        assert!(bench.page.take_restart(), "nothing to ask about");
        bench.page.backup.locked = false;
        // Only the active binding: no question about profiles, straight to the second one.
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(only_active.to_string_lossy().into_owned())));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        until_phase(&mut bench, is_active);
        bench.click(&language::t("e_nein"));
        finish(&mut bench);
        assert!(control(&game, "Profiles/default/actionmaps.xml").contains("aktiv-ziel"), "No: the active one stays");
        assert!(bench.page.take_restart());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_page_backup_and_reset_has_both_buttons_and_they_go_straight_to_the_dialogs() {
        let _language = crate::language_lock();
        let (page, folder, _data, _game) = controls_page("sicherung-seite", "a", 7200);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_sichern");
        // ⚠ No system dialog on the screen of whoever runs the test: the stand-in notes what was asked for.
        file_dialog::stand_in::start();
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("sich_lead")), "what a backup is: {texts:?}");
        assert!(shows(&texts, &language::t("sich_was")), "and what comes along");
        assert!(shows(&texts, &language::t("sich_schreiben")) && shows(&texts, &language::t("sich_lesen")), "{texts:?}");
        assert!(!bench.page.backup_open(), "nothing is open before a click");
        bench.click(&language::t("sich_schreiben"));
        bench.settle();
        assert!(matches!(bench.page.backup.phase, Phase::Saving(_)), "the dialog of the place, without the choice first");
        assert_eq!(file_dialog::stand_in::stop(), ["save"]);
        bench.page.backup.phase = Phase::Idle;
        file_dialog::stand_in::start();
        bench.click(&language::t("sich_lesen"));
        bench.settle();
        assert!(matches!(bench.page.backup.phase, Phase::Opening(_)), "the dialog of the file, without the choice first");
        assert_eq!(file_dialog::stand_in::stop(), ["open"]);
        assert!(bench.page.page_key() == "hf_sichern", "the page stays");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_tile_of_the_backup_has_the_link_back_up_now_that_makes_the_backup_and_not_the_page() {
        let _language = crate::language_lock();
        let (page, folder, _data, _game) = controls_page("sicherung-kachel", "a", 7200);
        let mut bench = Bench::new(page);
        let link = language::t("s_uv_sicherung_jetzt");
        let never = language::t("s_uv_sicherung_nie");
        // The counts of the overview are taken on the first frames.
        let mut texts = bench.settle();
        for _ in 0..50 {
            if shows(&texts, &never) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            texts = bench.settle();
        }
        assert!(shows(&texts, &never), "the tile of the backup is on the overview and says no backup yet: {texts:?}");
        let links = texts.iter().filter(|(text, _)| *text == link).count();
        assert_eq!(links, 1, "the link stands on the tile of the backup and on no other: {texts:?}");
        // A click on the link: the dialog of the place opens, the page does not change.
        file_dialog::stand_in::start();
        bench.click(&link);
        bench.settle();
        assert!(matches!(bench.page.backup.phase, Phase::Saving(_)), "the backup is begun");
        assert_eq!(file_dialog::stand_in::stop(), ["save"]);
        assert_eq!(bench.page.page_key(), "", "the page behind the tile did not open");
        bench.page.backup.phase = Phase::Idle;
        // A click beside the link, on the line of the tile, opens the page as before.
        file_dialog::stand_in::start();
        bench.click(&never);
        bench.settle();
        assert_eq!(bench.page.page_key(), "hf_sichern", "the tile itself leads to the page");
        assert!(!bench.page.backup_open(), "and starts no backup");
        assert!(file_dialog::stand_in::stop().is_empty(), "no dialog was asked for");
        // After a backup that worked the tile says when: the counts are taken again.
        bench.page.open_overview();
        bench.page.backup.phase =
            Phase::Saving(Pending::answered(Some(folder.join("kachel").to_string_lossy().into_owned())));
        finish(&mut bench);
        let texts = bench.settle();
        let last = language::t("s_uv_sicherung_zuletzt").replace("{0}", "");
        assert!(shows(&texts, &last), "the tile shows the time of the last backup: {texts:?}");
        assert!(!shows(&texts, &never), "and no longer 'no backup yet'");
        assert_eq!(texts.iter().filter(|(text, _)| *text == link).count(), 1, "the link stays");
        let _ = std::fs::remove_dir_all(folder);
    }

    /// A backup by channel: the look is shown before the yes, the question names the channel, the character template
    /// comes back on the first yes, the `user.cfg` only on the second, with the language line of this computer.
    #[test]
    fn a_backup_by_channel_shows_the_look_first_and_brings_the_user_cfg_back_only_on_the_second_yes() {
        let _language = crate::language_lock();
        let (source, folder_a, _data_a, game_a) = controls_page("sicherung-kanal-quelle", "quelle", 7200);
        std::fs::write(game_a.join("user.cfg"), "g_language = german_(germany)\nsys_maxfps = 90\n").unwrap();
        let templates_a = game_a.join("user").join("client").join("0").join("CustomCharacters");
        std::fs::create_dir_all(&templates_a).unwrap();
        std::fs::write(templates_a.join("Pilotin.chf"), b"CHF-quelle").unwrap();
        let mut bench = Bench::new(source);
        let zip = folder_a.join("kanal").to_string_lossy().into_owned();
        bench.page.backup.phase = Phase::Saving(Pending::answered(Some(zip.clone())));
        finish(&mut bench);
        let zip = format!("{zip}.zip");
        assert!(by_channel_in(Path::new(&zip)).contains(&"Steuerung/Kanaele/LIVE/user.cfg".to_string()));

        let (target, folder_b, _data_b, game_b) = controls_page("sicherung-kanal-ziel", "ziel", 7200);
        std::fs::write(game_b.join("user.cfg"), "g_language = english\nsys_maxfps = 30\n").unwrap();
        let mut bench = Bench::new(target);
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip.clone())));
        let texts = finish_until_asked(&mut bench);
        let channel_line = language::tf(
            "sich_vorschau_kanal",
            &[
                ("0", language::Arg::from("LIVE")),
                // Replaced: the active binding, the game settings, both profiles and the user.cfg; new: the template.
                ("1", language::Arg::from(5)),
                ("2", language::Arg::from(1)),
                ("3", language::Arg::from(0)),
            ],
        );
        assert!(shows(&texts, &channel_line), "the look says what changes in the channel: {texts:?}");
        assert!(control(&game_b, "controls/mappings/Kampf.xml").contains("kampf-ziel"), "the look writes nothing");
        bench.click(&language::t("s_mu_rest_einspielen"));
        let texts = until_phase(&mut bench, is_profiles);
        assert!(shows(&texts, "LIVE: 2"), "the question names the channel: {texts:?}");
        bench.click(&language::t("e_ja"));
        let texts = until_phase(&mut bench, is_active);
        assert!(shows(&texts, language::t("sich_spiel_aktiv").lines().next().unwrap_or("")), "{texts:?}");
        let template_b = game_b.join("USER").join("Client").join("0").join("CustomCharacters").join("Pilotin.chf");
        assert!(!template_b.exists(), "nothing before the answers");
        bench.click(&language::t("e_nein"));
        until_phase(&mut bench, is_told);
        assert_eq!(std::fs::read(&template_b).unwrap_or_default(), b"CHF-quelle", "the template came back");
        assert!(control(&game_b, "controls/mappings/Kampf.xml").contains("kampf-quelle"));
        let cfg = std::fs::read_to_string(game_b.join("user.cfg")).unwrap();
        assert!(cfg.contains("sys_maxfps = 30"), "the user.cfg stays without the second yes: {cfg}");
        bench.click(&language::t("hf_schliessen"));
        finish(&mut bench);

        // Once more with yes to both: the user.cfg comes back, the language line of this computer stays.
        let mut bench = Bench::new(controls_page_again(&folder_b));
        bench.page.backup.phase = Phase::Opening(Pending::answered(Some(zip)));
        finish_until_asked(&mut bench);
        bench.click(&language::t("s_mu_rest_einspielen"));
        until_phase(&mut bench, is_profiles);
        bench.click(&language::t("e_ja"));
        until_phase(&mut bench, is_active);
        bench.click(&language::t("e_ja"));
        until_phase(&mut bench, is_told);
        let cfg = std::fs::read_to_string(game_b.join("user.cfg")).unwrap();
        assert!(cfg.contains("sys_maxfps = 90"), "{cfg}");
        assert!(cfg.contains("g_language = english") && !cfg.contains("german"), "the language stays: {cfg}");
        assert!(control(&game_b, "Profiles/default/actionmaps.xml").contains("aktiv-quelle"));
        let _ = std::fs::remove_dir_all(folder_a);
        let _ = std::fs::remove_dir_all(folder_b);
    }

    /// A new page on the throwaway folder of [`controls_page`] that is there already (the game is quiet).
    fn controls_page_again(folder: &Path) -> SamplePage {
        let data = folder.join("daten");
        let live = folder.join("spiel").join("LIVE").to_string_lossy().into_owned();
        let vars = [(versekit_core::paths::ENV_INSTALL_DIR, live.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        let mut page = SamplePage::with_settings(Paths::new(host_in(&data, &vars)));
        page.offer_restart(true);
        page
    }
}
