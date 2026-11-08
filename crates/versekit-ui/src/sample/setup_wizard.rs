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

//! The setup assistant (`wizard.py` of 4.12) in the window: the steps at the left, each one clickable, a
//! check mark only for a confirmed step, finishing blocked while a step is open (the design of the model
//! page, `settings::WizardState`). Every choice is kept at once, as in 4.12; the assistant only decides which
//! steps apply (`versekit_core::setup`) and walks through them.
//!
//! The choices are made on the state the settings pages work on (the language, the size, the overlay, the
//! scheme, the game folder), so the pages and the assistant never disagree and the writing is the one of the
//! pages. What the pages have no field for (play time, range of blueprints, icon, the notes in the game) is
//! written straight to the settings file.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::{RichText, Ui};
use versekit_core::detection::{self, Accounts};
use versekit_core::given::GivenBy;
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::logsource::ALL_ACCOUNTS;
use versekit_core::paths::{Host, Paths};
use versekit_core::setup::{self, GameFolder, Step};
use versekit_core::theme::{self, Scheme};
use versekit_core::{desktop_entry, export, game_texts, injection, overlay, report_facts};

use super::catch_up::{self, Reading};
use super::preview;
use super::{SamplePage, size_presets, size_slider};
use crate::file_dialog::{self, Pending};
use crate::game_texts_page;
use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, settings};

/// How often the data folder is looked at for being writable while its step is shown (seconds).
const STORAGE_LOOK: f64 = 1.5;

/// What the card of the assistant needs besides the content of a step (margins, heads, the row of buttons):
/// the step may use the height of the window minus this, then it scrolls.
const STEP_FRAME: f32 = 200.0;

/// The space under a picture that stands above the area that scrolls.
const PICTURE_GAP: f32 = 8.0;

/// The height a step takes for its picture when the picture stands above the area that scrolls (0: the step has
/// none, or none that stays put).
fn picture_height(step: Step) -> f32 {
    let height = match step {
        Step::Overlay => preview::FRAME_HEIGHT,
        Step::Texts => preview::TITLE_FRAME_HEIGHT,
        Step::Start if cfg!(windows) => preview::TASKBAR_FRAME_HEIGHT,
        _ => return 0.0,
    };
    height + PICTURE_GAP
}

/// The least height the controls get beside a picture that stands above the area that scrolls; less, and the
/// picture scrolls with the step (a controls area of a hand's breadth is no use beside it).
const STEP_MIN_BESIDE_PICTURE: f32 = 200.0;

/// The least height the content of a step gets.
const STEP_MIN: f32 = 100.0;

/// The assistant while it is open.
pub(super) struct Wizard {
    /// The short setup `Willkommen in 5.0` for a program set up before 5.0 (the steps are those of
    /// [`setup::short_steps`], everything is filled in, and it can be skipped).
    short: bool,
    /// The steps that apply now.
    steps: Vec<Step>,
    /// The step shown and the steps confirmed.
    state: settings::WizardState,
    /// The game folder as typed, and what it means.
    game_text: String,
    game_seen: String,
    game_check: GameFolder,
    game_dialog: Option<Pending>,
    /// The places that were searched, for a player without the game found.
    searched: Vec<String>,
    /// Was a game folder found by itself when the assistant opened?
    found_by_itself: bool,
    /// Went on without a game folder.
    without_game: bool,
    /// The saved logs of the sessions kept so far.
    logs_kept: usize,
    /// The reading of those logs, started with the step.
    reading: Reading,
    /// The game folder the player confirmed in this run. The logs are read only for it, after the confirmation:
    /// the folder as typed may be the wrong channel, and reading writes into the collection.
    game_confirmed: Option<String>,
    /// The data folder the last frame saw (another one holds another collection).
    data_seen: String,
    /// The channels installed next to the game folder as typed, and the folder they were looked up for.
    channels_for: String,
    channels: Vec<String>,
    /// The picture of the step stands above the area that scrolls (set by the layout, read by the step).
    picture_pinned: bool,
    /// The blueprints in the collection, for the last page (`None`: not read yet).
    stock: Option<usize>,
    /// Play time in the head bar and the range of blueprints, read once and kept in the file.
    show_time: bool,
    all_blueprints: bool,
    tray: bool,
    notes: [bool; 4],
    /// Why nothing can be saved in the data folder (empty: it can), and when that was looked at last.
    storage_blocked: Option<String>,
    storage_at: f64,
    /// Why the assistant went away.
    closed: Option<Closed>,
    /// The last page offers an entry in the application menu (a system with such menus, none there yet), what
    /// writes it, and what came of the click: `(worked, said)`.
    menu_offer: bool,
    menu_make: MenuMaker,
    menu_said: Option<(bool, String)>,
    /// The accounts of the logs for the step "Dein Account", looked up in a thread when the step is first shown,
    /// and the one chosen there (an account, or all).
    account_search: Option<Receiver<Accounts>>,
    accounts: Option<Accounts>,
    account_choice: Option<String>,
}

/// Writes the entry in the application menu for the running program; gives its file or why it did not work.
type MenuMaker = fn(&Host) -> Result<std::path::PathBuf, String>;

/// Does the last page offer the entry in the application menu? Not in the short setup (it tells the way back and
/// nothing more), only on a system that has such menus, and only while there is no entry (that still points to
/// a program that is there).
fn menu_offered(short: bool, available: bool, present: bool) -> bool {
    !short && available && !present
}

/// How the assistant ended, for the program that opened the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupEnd {
    /// Run to its end: the program counts as set up.
    pub finished: bool,
    /// The player asked for the overview at the end.
    pub overview: bool,
}

/// How the assistant ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Closed {
    /// Run to its end (and "Übersicht öffnen" if the player wanted the overview).
    Finished { overview: bool },
    /// Closed before the end.
    Left,
}

impl SamplePage {
    /// Opens the assistant (the head bar, the settings, the first start).
    pub(super) fn open_setup(&mut self) {
        self.open_wizard(false);
    }

    /// Opens the short setup `Willkommen in 5.0` once for a program that was set up before 5.0: only in the main
    /// window (the caller), only where there are data (a copy of the data included), never while the game runs
    /// (a window above the game is no welcome; the process list and the logs of every channel say it, what cannot
    /// be told does not hold the page back), and only after the note that it was shown is saved (without it the
    /// page would come at every start). True when it opened.
    pub fn open_short_if_needed(&mut self) -> bool {
        if self.setup.is_some() || !self.real() {
            return false;
        }
        let Some(store) = &self.store else { return false };
        if !setup::short_needed(&store.paths)
            || versekit_core::game_start::run_state(&store.paths) == versekit_core::game_start::RunState::Running
        {
            return false;
        }
        if !setup::mark_version(&store.paths) {
            return false;
        }
        self.open_wizard(true);
        true
    }

    fn open_wizard(&mut self, short: bool) {
        self.game_texts.forget_leaving_alone();
        let paths = self.store.as_ref().map(|s| &s.paths);
        let found = paths.and_then(Paths::game_folder);
        // Whoever went on without the game once and has it now starts with all the steps, as in 4.12.
        let without_game = found.is_none() && paths.is_some_and(|p| p.setting_bool(setup::WITHOUT_GAME, false));
        let game_text = found.clone().unwrap_or_default();
        let game_check = paths.map_or(GameFolder::Empty, |p| setup::check_game_folder(p, &game_text));
        let steps =
            if short { setup::short_steps(without_game) } else { setup::steps(without_game, self.start_possible()) };
        let flag = |key: &str, default: bool| paths.map_or(default, |p| p.setting_bool(key, default));
        self.setup = Some(Wizard {
            short,
            state: settings::WizardState::new(steps.len()),
            steps,
            game_seen: game_text.clone(),
            game_text,
            game_check,
            game_dialog: None,
            searched: paths.map_or_else(Vec::new, |p| p.searched_game_locations(4)),
            found_by_itself: found.is_some(),
            without_game,
            logs_kept: paths.map_or(0, |p| p.log_backups(None).len()),
            reading: Reading::default(),
            game_confirmed: None,
            data_seen: paths.map(|p| p.app_folder()).unwrap_or_default(),
            channels_for: String::new(),
            channels: Vec::new(),
            picture_pinned: false,
            stock: None,
            show_time: flag("spielzeit_zeigen", false),
            all_blueprints: flag(export::ALL_SETTING, false),
            tray: flag(overlay::keys::TRAY, true),
            notes: [
                flag(game_texts::SETTING_ON, true),
                flag(game_texts::SETTING_AUTO, true),
                flag(injection::SETTING_DETAILS, true),
                flag(injection::ranks::SETTING, true),
            ],
            storage_blocked: None,
            storage_at: f64::NEG_INFINITY,
            closed: None,
            menu_offer: paths
                .is_some_and(|p| menu_offered(short, desktop_entry::available(), desktop_entry::present(&p.host))),
            menu_make: desktop_entry::create,
            menu_said: None,
            account_search: None,
            accounts: None,
            account_choice: None,
        });
    }

    /// Is the program set up (the proof is written, or a game folder was entered)? A window without data of its
    /// own is not.
    pub fn is_set_up(&self) -> bool {
        self.store.as_ref().is_some_and(|s| setup::is_configured(&s.paths))
    }

    /// Says something in the line of the window, as a fault.
    fn say_setup_fault(&mut self, now: f64, text: String) {
        self.pin_fault = Some(text.clone());
        self.pin_notice = Some((now, text));
    }

    /// The assistant ran to its end: written down that the program is set up. False, and said, when nothing
    /// could be saved (then the assistant stays, and the player is led to the data folder).
    fn finish_setup(&mut self, now: f64) -> bool {
        let saved = self.store.as_ref().is_none_or(|s| setup::finish(&s.paths));
        if !saved {
            self.say_setup_fault(now, t("einrichtung_nicht_gespeichert"));
        }
        saved
    }

    /// Where there is a start with the system or an icon next to the clock (Windows): the card for it exists.
    fn start_possible(&self) -> bool {
        self.autostart.possible() || cfg!(windows)
    }

    /// Opens the assistant when the program is not set up yet (or the game is missing, see
    /// [`versekit_core::setup::needed`]); true when it opened. Only on data of its own.
    pub fn open_setup_if_needed(&mut self) -> bool {
        let needed = self.store.as_ref().is_some_and(|s| setup::needed(&s.paths));
        if needed {
            self.open_setup();
        }
        needed
    }

    /// How the assistant ended since the last call (once); `None` while it is open or was not opened.
    pub fn take_setup_end(&mut self) -> Option<SetupEnd> {
        let closed = self.setup.as_ref()?.closed?;
        self.setup = None;
        Some(match closed {
            Closed::Finished { overview } => {
                // What the assistant changed behind the lists: the blueprints it read in, and the range of
                // the list (the switch is a setting of the file only).
                if let Some(store) = &self.store {
                    self.blueprints.follow_all_setting(&store.paths, &mut self.blueprint_view);
                }
                // "Übersicht öffnen" shows the overview, also when the assistant was opened on another page.
                if overview {
                    self.open_overview();
                }
                SetupEnd { finished: true, overview }
            }
            Closed::Left => SetupEnd { finished: false, overview: false },
        })
    }

    /// Is the assistant open?
    pub fn setup_open(&self) -> bool {
        self.setup.as_ref().is_some_and(|w| w.closed.is_none())
    }

    /// Writes a setting that the pages have no field for.
    fn put_setting(&self, key: &str, value: Value) {
        if let Some(store) = &self.store {
            store.paths.set_setting(key, value);
        }
    }

    /// Draws the assistant above the window (nothing while it is closed).
    pub(super) fn show_setup(&mut self, ctx: &egui::Context, scheme: &Scheme) {
        let Some(mut wizard) = self.setup.take() else { return };
        if let Some(answer) = wizard.game_dialog.as_ref().and_then(Pending::take) {
            wizard.game_dialog = None;
            if let Some(folder) = answer {
                wizard.game_text = folder;
            }
        }
        // What is typed is read as soon as it changes.
        if wizard.game_text != wizard.game_seen {
            wizard.game_seen.clone_from(&wizard.game_text);
            wizard.game_check = self
                .store
                .as_ref()
                .map_or(GameFolder::Empty, |s| setup::check_game_folder(&s.paths, &wizard.game_text));
        }
        // Read before the window takes the key itself.
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let width = (ctx.content_rect().width() - 48.0).clamp(420.0, 820.0);
        let modal = egui::Modal::new(egui::Id::unique("einrichtung")).frame(egui::Frame::NONE).show(ctx, |ui| {
            ui.set_width(width);
            widgets::card(ui, scheme, |ui| self.setup_body(ui, scheme, &mut wizard)).inner
        });
        // Esc leaves the assistant; a click beside it does not (a slip would end the first start).
        if wizard.closed.is_none() && modal.should_close() && escape {
            wizard.closed = Some(Closed::Left);
        }
        self.setup = Some(wizard);
    }

    fn setup_body(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        let names: Vec<String> = wizard.steps.iter().map(|s| t(step_title(*s))).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let step = wizard.state.step;
        ui.horizontal(|ui| {
            let title = if wizard.short {
                tf("as_willkommen_titel", &[("0", Arg::from(setup::VERSION))])
            } else {
                format!("{}: {}", t("hf_titel"), t("assistent"))
            };
            widgets::heading(ui, scheme, &title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = tf("schritt_von", &[("0", Arg::from(step + 1)), ("1", Arg::from(wizard.steps.len()))]);
                ui.label(RichText::new(count).small().color(color(scheme.sub)));
            });
        });
        ui.add_space(6.0);
        widgets::list_frame(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                // The list scrolls where the window is lower than the column of the step beside it (its heading,
                // the step and the row of buttons).
                let list_room = (ui.ctx().content_rect().height() - STEP_FRAME).max(STEP_MIN) + 60.0;
                let list = ui.allocate_ui_with_layout(
                    egui::vec2(236.0_f32.min(ui.available_width() * 0.36), list_room),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let area = egui::ScrollArea::vertical()
                            .id_salt("einrichtung-schritte")
                            .max_height(list_room)
                            .auto_shrink([false, true]);
                        widgets::scroll_area(ui, scheme, area, |ui| {
                            settings::step_list(ui, scheme, &name_refs, step, &wizard.state.done)
                        })
                        .inner
                    },
                );
                if let Some(clicked) = list.inner {
                    wizard.state.step = clicked;
                }
                ui.add_space(GAP);
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    let current = wizard.steps[wizard.state.step];
                    // (The welcome page has its title above already; the same word twice is no heading.)
                    if current != Step::Welcome {
                        widgets::heading(ui, scheme, &names[wizard.state.step]);
                        ui.add_space(4.0);
                    }
                    // The step scrolls where the window is low; the row of buttons below stays in sight.
                    let mut room = (ui.ctx().content_rect().height() - STEP_FRAME).max(STEP_MIN);
                    // The picture of a step stands above the area that scrolls, so that it stays in sight beside the
                    // controls it follows; where the window is too low for both, it scrolls with the step.
                    let pin = picture_height(current);
                    wizard.picture_pinned = pin > 0.0 && room - pin >= STEP_MIN_BESIDE_PICTURE;
                    if wizard.picture_pinned {
                        self.step_picture(ui, scheme, wizard, current);
                        ui.add_space(PICTURE_GAP);
                        room -= pin;
                    }
                    widgets::scroll_area(
                        ui,
                        scheme,
                        egui::ScrollArea::vertical()
                            .id_salt(("einrichtung-schritt", wizard.state.step))
                            .max_height(room)
                            .auto_shrink([false, true]),
                        |ui| {
                            ui.set_min_width(ui.available_width());
                            self.setup_step(ui, scheme, wizard, current);
                        },
                    );
                    ui.add_space(GAP);
                    self.setup_buttons(ui, scheme, wizard, current);
                });
            });
        });
    }

    fn paragraph(ui: &mut Ui, scheme: &Scheme, text: &str, muted: bool) {
        let tone = if muted { scheme.sub } else { scheme.fg };
        ui.add(egui::Label::new(RichText::new(text).color(color(tone))).wrap());
    }

    /// A switch with its name and what it does, as on the pages of the settings: name and hint at the left, the
    /// switch in the column of controls, and the name for screen readers. True when it was flipped.
    fn switch_row(ui: &mut Ui, scheme: &Scheme, name: &str, hint: &str, on: &mut bool) -> bool {
        ui.add_space(6.0);
        let mut changed = false;
        settings::setting_row(ui, scheme, name, hint, false, |row| {
            let switch = row.add(widgets::Toggle::new(on, scheme));
            let (enabled, now) = (switch.enabled(), *on);
            switch.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, now, name));
            changed = switch.changed();
        });
        changed
    }

    /// A line that tells a state: the icon in the colour of the state and the sentence (the state is not told by
    /// the colour alone).
    fn state_line(ui: &mut Ui, tone: versekit_core::theme::Color, icon: &str, text: &str) {
        ui.horizontal_top(|row| {
            if let Some(image) = crate::icons::image(icon, tone, 16.0) {
                row.add(image);
            }
            row.add(egui::Label::new(RichText::new(text).color(color(tone))).wrap());
        });
    }

    /// A setting row of the assistant: the name, what it does, and the control below.
    fn row(ui: &mut Ui, scheme: &Scheme, name: &str, hint: &str, control: impl FnOnce(&mut Ui)) {
        ui.add_space(6.0);
        ui.label(RichText::new(name).strong());
        ui.add(egui::Label::new(RichText::new(hint).small().color(color(scheme.sub))).wrap());
        ui.horizontal_wrapped(control);
    }

    fn setup_step(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard, step: Step) {
        match step {
            Step::Welcome => {
                Self::paragraph(ui, scheme, &t("as_willkommen_1"), false);
                ui.add_space(8.0);
                // What is new, in the words of the announcement (one place for the wording: the language files).
                ui.label(RichText::new(tf("as_neu_titel", &[("0", Arg::from(setup::VERSION))])).strong());
                for key in ["as_neu_1", "as_neu_2", "as_neu_3", "as_neu_4", "as_neu_5", "as_neu_6"] {
                    Self::paragraph(ui, scheme, &t(key), false);
                }
                ui.add_space(8.0);
                Self::paragraph(ui, scheme, &t("as_willkommen_2"), false);
                ui.add_space(6.0);
                Self::paragraph(ui, scheme, &t("as_willkommen_3"), true);
            }
            Step::Language => self.step_language(ui, scheme),
            Step::Storage => {
                Self::paragraph(ui, scheme, &t("schritt_ablage_text"), false);
                if cfg!(windows) {
                    ui.add_space(4.0);
                    Self::paragraph(ui, scheme, &t("schritt_ablage_hilfe"), true);
                }
                ui.add_space(8.0);
                let paths = self.store.as_ref().map(|s| &s.paths);
                self.data_folder.show(ui, scheme, paths);
                // Another data folder holds another collection and another game folder setting: what was read for
                // the first is not the answer for the second.
                let data_now = self.store.as_ref().map(|s| s.paths.app_folder()).unwrap_or_default();
                if data_now != wizard.data_seen {
                    wizard.data_seen = data_now;
                    wizard.stock = None;
                    wizard.game_confirmed = None;
                    wizard.reading.forget();
                }
                // Can anything be saved there? Looked at again every moment (a trial file is written).
                let now = ui.input(|i| i.time);
                if let Some(store) = &self.store
                    && now - wizard.storage_at > STORAGE_LOOK
                {
                    wizard.storage_at = now;
                    wizard.storage_blocked = report_facts::storage_status(&store.paths.app_folder()).err();
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(STORAGE_LOOK));
                if let Some(reason) = &wizard.storage_blocked {
                    ui.add_space(6.0);
                    let said = tf("ablage_gesperrt", &[("0", Arg::from(reason.as_str()))]);
                    Self::state_line(ui, scheme.red_text(), "circle-x", &said);
                } else if self.store.is_some() && wizard.storage_at.is_finite() {
                    // The trial write went through: the folder is said to be good.
                    ui.add_space(6.0);
                    Self::state_line(ui, scheme.accent, "circle-check", &t("ablage_ok"));
                }
            }
            Step::Game => self.step_game(ui, scheme, wizard),
            Step::Account => self.step_account(ui, scheme, wizard),
            Step::Overlay => self.step_overlay(ui, scheme, wizard),
            Step::Display => self.step_display(ui, scheme, wizard),
            Step::Start => self.step_start(ui, scheme, wizard),
            Step::Texts => self.step_texts(ui, scheme, wizard),
            Step::Connections => Self::step_connections(ui, scheme),
            Step::Done => {
                // Steps not confirmed yet are named first: "Fertig" waits for them (see the button).
                let open: Vec<String> =
                    wizard.state.open().into_iter().map(|k| t(step_title(wizard.steps[k]))).collect();
                if !open.is_empty() {
                    let said = tf("s_mu_wiz_offen", &[("steps", Arg::from(open.join(", ")))]);
                    ui.add(egui::Label::new(RichText::new(said).color(color(scheme.yellow))).wrap());
                    ui.add_space(8.0);
                }
                if wizard.short {
                    // The short setup ends with a word and the way back; the count, the modules and the privacy
                    // line were told at the first setup.
                    Self::paragraph(ui, scheme, &t("as_kurz_fertig"), false);
                    ui.add_space(6.0);
                    let again = tf("as_tipp_erneut", &[("0", Arg::from(t("hf_einrichtung")))]);
                    Self::paragraph(ui, scheme, &again, true);
                    return;
                }
                if wizard.without_game {
                    widgets::heading(ui, scheme, &t("ohne_spiel_titel"));
                    Self::paragraph(ui, scheme, &t("ohne_spiel_text"), false);
                    ui.add_space(4.0);
                    Self::paragraph(ui, scheme, &t("ohne_spiel_wo"), true);
                } else {
                    // What the collection holds now: the number the reading ended with, or (the reading step was not
                    // visited, or still works) the collection itself, read once and not every frame.
                    if wizard.reading.total().is_none() && wizard.stock.is_none() {
                        wizard.stock = self.store.as_ref().map(|s| catch_up::stock_count(&s.paths));
                    }
                    Self::paragraph(ui, scheme, &t("schritt_fertig_text"), false);
                    if let Some(count) = wizard.reading.total().or(wizard.stock) {
                        ui.add_space(8.0);
                        // The start page shows the number so, in the scheme chosen.
                        Self::paragraph(ui, scheme, &t("as_kachel_text"), true);
                        preview::tile_picture(ui, scheme, count);
                    }
                }
                ui.add_space(8.0);
                Self::paragraph(ui, scheme, &t("as_datenschutz"), true);
                self.done_modules(ui, scheme);
                ui.add_space(8.0);
                // The gear of the overlay opens the window with the list.
                Self::paragraph(ui, scheme, &t("tipp_liste"), true);
                let again = tf("as_tipp_erneut", &[("0", Arg::from(t("hf_einrichtung")))]);
                Self::paragraph(ui, scheme, &again, true);
                if !wizard.without_game {
                    self.done_menu_entry(ui, scheme, wizard);
                }
            }
        }
    }

    /// The offer of an entry in the application menu: where a system has such menus and has no entry yet. The
    /// answer stays below the button, the button stays too.
    fn done_menu_entry(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        if !wizard.menu_offer {
            return;
        }
        ui.add_space(12.0);
        Self::paragraph(ui, scheme, &t("as_menue_frage"), false);
        ui.add_space(6.0);
        let label = t("as_menue_knopf");
        let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).icon("app-window");
        if ui.add(button).clicked()
            && let Some(store) = &self.store
        {
            wizard.menu_said = Some(match (wizard.menu_make)(&store.paths.host) {
                Ok(file) => (true, tf("as_menue_da", &[("0", Arg::from(file.to_string_lossy().as_ref()))])),
                Err(why) => (false, tf("as_menue_nein", &[("0", Arg::from(why.as_str()))])),
            });
        }
        if let Some((worked, said)) = &wizard.menu_said {
            ui.add_space(6.0);
            if *worked {
                Self::state_line(ui, scheme.accent, "circle-check", said);
            } else {
                Self::state_line(ui, scheme.yellow, "triangle-alert", said);
            }
        }
    }

    /// The row "start the setup again" of the pages that can: the name and what it does at the left, the button at
    /// the right. Clicking opens the assistant.
    pub(super) fn repeat_setup_row(&mut self, ui: &mut Ui, scheme: &Scheme) {
        if Self::repeat_setup_line(ui, scheme) {
            self.open_setup();
        }
    }

    /// [`Self::repeat_setup_row`] for a caller that opens the assistant itself: whether the button was clicked.
    pub(super) fn repeat_setup_line(ui: &mut Ui, scheme: &Scheme) -> bool {
        let (name, hint, label) = (t("s_si_einrichtung"), t("s_si_einrichtung_h"), t("hf_einrichtung"));
        let found_by_name = crate::highlight::pointed(ui, &[&name]);
        let mut clicked = false;
        settings::setting_row(ui, scheme, &name, &hint, found_by_name, |row| {
            // The head bar has a button of the same label: the row's one is named by its row.
            let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).icon("wand-sparkles").spoken(&name);
            clicked = row.add(button).clicked();
        });
        clicked
    }

    fn step_language(&mut self, ui: &mut Ui, scheme: &Scheme) {
        Self::paragraph(ui, scheme, &t("schritt_sprache_text"), false);
        ui.add_space(8.0);
        let before = (self.language_auto, self.general_english);
        let host = self.store.as_ref().map(|s| s.paths.host.clone());
        ui.horizontal_wrapped(|ui| {
            if widgets::radio(ui, scheme, self.language_auto, &t("sprache_auto")).clicked() {
                self.language_auto = true;
                self.general_english =
                    host.as_ref().is_some_and(|h| language::system_language(h) == language::Lang::En);
            }
            if widgets::radio(ui, scheme, !self.language_auto && !self.general_english, "Deutsch").clicked() {
                (self.language_auto, self.general_english) = (false, false);
            }
            if widgets::radio(ui, scheme, !self.language_auto && self.general_english, "English").clicked() {
                (self.language_auto, self.general_english) = (false, true);
            }
        });
        if (self.language_auto, self.general_english) != before {
            language::set_language(if self.general_english { language::Choice::En } else { language::Choice::De });
            ui.ctx().request_repaint();
        }
    }

    fn step_game(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        Self::paragraph(ui, scheme, &t("schritt_spiel_text"), false);
        ui.add_space(4.0);
        Self::paragraph(ui, scheme, &t("schritt_spiel_hilfe"), true);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let room = (ui.available_width() - 120.0).max(120.0);
            widgets::clearable_field(ui, &mut wizard.game_text, &t("s_pl_spielordner"), scheme, room);
            if ui.add(widgets::Button::new(&t("durchsuchen"), ButtonKind::Normal, scheme)).clicked()
                && wizard.game_dialog.is_none()
            {
                let start = (!wizard.game_text.trim().is_empty()).then(|| wizard.game_text.clone());
                wizard.game_dialog =
                    Some(file_dialog::choose_folder(ui.ctx(), &t("s_ei_spielordner"), start.as_deref()));
            }
        });
        ui.add_space(6.0);
        match wizard.game_check.clone() {
            GameFolder::Empty => {}
            GameFolder::Found { folder, deduced } => {
                let mut said = t("log_gefunden");
                if deduced {
                    said.push('\n');
                    said.push_str(&tf("ordner_gedeutet", &[("0", Arg::from(folder.as_str()))]));
                }
                Self::state_line(ui, scheme.accent, "circle-check", &said);
                self.game_found_extras(ui, scheme, wizard, &folder);
            }
            GameFolder::NoLog => {
                Self::state_line(ui, scheme.yellow, "triangle-alert", &t("keine_log_darin"));
            }
        }
        if !wizard.found_by_itself {
            ui.add_space(10.0);
            Self::paragraph(ui, scheme, &t("gesucht_wurde_hier"), true);
            for place in &wizard.searched {
                ui.add(egui::Label::new(RichText::new(place).small().monospace().color(color(scheme.sub))).wrap());
            }
            // ⚠ The way out for whoever does not have Star Citizen on this computer: without it the assistant
            // would come at every start and the player could never get past this step.
            ui.add_space(10.0);
            let way_out = t("ohne_spiel");
            if ui.add(widgets::Button::new(&way_out, ButtonKind::Normal, scheme).icon("arrow-right")).clicked() {
                if let Some(store) = &self.store {
                    setup::go_on_without_game(&store.paths);
                }
                self.rebuild_steps(wizard, true);
                // The first three steps are settled (nothing to ask), the assistant goes on at the overlay.
                for (k, s) in wizard.steps.clone().iter().enumerate() {
                    if matches!(s, Step::Language | Step::Storage | Step::Game) {
                        wizard.state.done[k] = true;
                    }
                }
                wizard.state.step = wizard.steps.iter().position(|s| *s == Step::Overlay).unwrap_or(0);
            }
        }
    }

    /// Sets the steps anew for a game folder or none: the confirmations already given stay with their steps, the
    /// player stays at the step he is at (or the first one if that is gone).
    fn rebuild_steps(&self, wizard: &mut Wizard, without_game: bool) {
        let before: Vec<(Step, bool)> = wizard.steps.iter().copied().zip(wizard.state.done.iter().copied()).collect();
        let at = wizard.steps.get(wizard.state.step).copied();
        wizard.without_game = without_game;
        wizard.steps = setup::steps(without_game, self.start_possible());
        wizard.state = settings::WizardState::new(wizard.steps.len());
        for (k, step) in wizard.steps.iter().enumerate() {
            wizard.state.done[k] = before.iter().any(|(s, done)| s == step && *done) && k + 1 < wizard.steps.len();
        }
        wizard.state.step = at.and_then(|a| wizard.steps.iter().position(|s| *s == a)).unwrap_or(0);
    }

    /// What stands under a game folder that was found: the channels installed next to it, and what the logs
    /// of the earlier sessions gave. The reading starts only after the player confirmed THIS folder (reading
    /// writes into the collection and notes the account; a folder as typed may be a test channel or somebody
    /// else's logs, and nothing takes it back). Where the data folder cannot be written nothing is read.
    fn game_found_extras(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard, folder: &str) {
        let Some(store) = &self.store else { return };
        if wizard.channels_for != folder {
            // Only looked at, nothing written: the paths with this folder as the game folder.
            let scoped = store.paths.working_copy();
            scoped.set_given_game(folder, GivenBy::Named("Einrichtung".into()));
            wizard.channels_for = folder.to_string();
            wizard.channels = scoped.installed_channels().into_iter().map(|(name, ..)| name).collect();
        }
        if !wizard.channels.is_empty() {
            ui.add_space(4.0);
            let said = tf("as_kanaele", &[("0", Arg::from(wizard.channels.join(", ")))]);
            ui.add(egui::Label::new(RichText::new(said).small().color(color(scheme.sub))).wrap());
        }
        ui.add_space(10.0);
        Self::paragraph(ui, scheme, &t("schritt_lesen_text"), true);
        ui.add_space(6.0);
        if wizard.game_confirmed.as_deref() != Some(folder) {
            Self::paragraph(ui, scheme, &t("as_lesen_nach"), true);
            return;
        }
        // The number on the last page is read again after this (the reading changes it).
        wizard.stock = None;
        match wizard.reading.poll(ui.ctx(), &store.paths, folder).cloned() {
            Some(done) if !done.saved => {
                // Nothing was marked as read: the reading can be done again once the folder can be written.
                Self::state_line(ui, scheme.red_text(), "circle-x", &t("ov_bestand_fehler"));
            }
            Some(done) => {
                let line = tf("nachgelesen_gross", &[("0", Arg::from(done.added)), ("1", Arg::from(done.files))]);
                ui.label(line);
                if let Some(gap) = &done.gap {
                    ui.add_space(4.0);
                    let said = RichText::new(gap.render(language::current())).color(color(scheme.yellow));
                    ui.add(egui::Label::new(said).wrap());
                    Self::paragraph(ui, scheme, &t("nachtragen_hinweis"), true);
                }
            }
            None => {
                let line = if wizard.logs_kept > 0 {
                    tf("lese_logs_n", &[("0", Arg::from(wizard.logs_kept))])
                } else {
                    t("lese_logs")
                };
                widgets::loading(ui, scheme, &line);
            }
        }
    }

    /// The own account: the one found in the logs to confirm, or the choice when there are several (or "all" was
    /// chosen before). Looked up in a thread when the step is first shown, after the reading of the game step.
    fn step_account(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        Self::paragraph(ui, scheme, &t("as_acc_text"), false);
        ui.add_space(8.0);
        let Some(store) = &self.store else { return };
        if wizard.accounts.is_none() && wizard.account_search.is_none() && !wizard.reading.working() {
            let (tx, rx) = mpsc::channel();
            let copy = store.paths.working_copy();
            let wake = ui.ctx().clone();
            let started = std::thread::Builder::new().name("konten".into()).spawn_with_language(move || {
                let _ = tx.send(detection::accounts(&copy));
                wake.request_repaint();
            });
            if started.is_ok() {
                wizard.account_search = Some(rx);
            } else {
                wizard.accounts = Some(Accounts::default());
            }
        }
        if let Some(rx) = &wizard.account_search {
            match rx.try_recv() {
                Ok(found) => {
                    wizard.account_choice = found.own.clone().or_else(|| found.list.first().map(|(n, _)| n.clone()));
                    wizard.accounts = Some(found);
                    wizard.account_search = None;
                }
                Err(TryRecvError::Disconnected) => {
                    wizard.accounts = Some(Accounts::default());
                    wizard.account_search = None;
                }
                Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(150)),
            }
        }
        let Some(found) = wizard.accounts.clone() else {
            widgets::loading(ui, scheme, &t("s_er_acc_laedt"));
            return;
        };
        let all_before = found.own.as_deref() == Some(ALL_ACCOUNTS);
        match found.list.as_slice() {
            [] => Self::paragraph(ui, scheme, &t("s_er_acc_keiner"), true),
            [(name, count)] if !all_before => {
                let word = crate::detection_page::account_word(name, *count);
                Self::state_line(ui, scheme.accent, "circle-check", &tf("as_acc_einer", &[("0", Arg::from(word))]));
                ui.add_space(4.0);
                Self::paragraph(ui, scheme, &t("as_acc_einer_h"), true);
                wizard.account_choice = Some(name.clone());
            }
            list => {
                Self::paragraph(ui, scheme, &t("as_acc_mehrere"), false);
                ui.add_space(4.0);
                let mut entries: Vec<(String, String)> = list
                    .iter()
                    .map(|(name, count)| (name.clone(), crate::detection_page::account_word(name, *count)))
                    .collect();
                entries.push((ALL_ACCOUNTS.to_string(), t("s_er_acc_alle")));
                for (value, word) in entries {
                    let on = wizard.account_choice.as_deref() == Some(value.as_str());
                    if widgets::radio(ui, scheme, on, &word).clicked() {
                        wizard.account_choice = Some(value);
                    }
                }
            }
        }
        let tally = found.foreign;
        if tally.blueprints > 0 && !all_before {
            ui.add_space(8.0);
            let key = if tally.blueprints == 1 { "s_er_acc_fremd_1" } else { "s_er_acc_fremd_n" };
            let line = tf(key, &[("0", Arg::from(tally.blueprints))]);
            Self::state_line(ui, scheme.yellow, "triangle-alert", &line);
        }
    }

    /// What the program can be connected to: the way of the Hangar-Exporter as a strip of three steps, where to
    /// get it, and a word on the Basetool. Everything here is optional; the buttons only open the extension or its page in a browser.
    fn step_connections(ui: &mut Ui, scheme: &Scheme) {
        Self::paragraph(ui, scheme, &t("as_verb_text"), false);
        ui.add_space(8.0);
        let steps = [t("as_fluss_1"), t("as_fluss_2"), t("as_fluss_3")];
        preview::flow_strip(ui, scheme, &t("as_vorschau_fluss"), &steps);
        ui.add_space(8.0);
        crate::hangar_page::exporter_buttons(ui, scheme);
        ui.add_space(6.0);
        Self::paragraph(ui, scheme, &t("as_kopplung_wo"), true);
        ui.add_space(14.0);
        widgets::heading(ui, scheme, &t("k_basetool_titel"));
        Self::paragraph(ui, scheme, &t("as_basetool_text"), true);
    }

    /// The modules on the last page: each area of the program that can be hidden, with its switch (saved at once,
    /// as on the page Modules). Nothing is deleted by hiding.
    fn done_modules(&mut self, ui: &mut Ui, scheme: &Scheme) {
        if self.store.is_none() {
            return;
        }
        ui.add_space(10.0);
        widgets::heading(ui, scheme, &t("hf_module"));
        Self::paragraph(ui, scheme, &t("s_mo_lead"), true);
        let mut flipped = false;
        for (group, _) in versekit_core::modules::SWITCHABLE {
            let Some(store) = &self.store else { return };
            let title = t(versekit_core::modules::label(group));
            let hint = tf("s_mo_enthaelt", &[("0", Arg::from(crate::modules_page::pages_of(group).as_str()))]);
            let mut on = versekit_core::modules::enabled(&store.paths, group);
            if Self::switch_row(ui, scheme, &title, &hint, &mut on) {
                flipped |= versekit_core::modules::set_enabled(&store.paths, group, on);
            }
        }
        Self::paragraph(ui, scheme, &t("s_mo_hinweis"), true);
        // The menu follows at once, as with the page Modules.
        if flipped && let Some(store) = &self.store {
            self.module_hidden = versekit_core::modules::hidden_pages(&store.paths);
        }
    }

    /// The texts in the game: a title before and after (it follows the switches below it at once), the four
    /// switches of the notes, and the translation that carries them.
    fn step_texts(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        Self::paragraph(ui, scheme, &t("as_texte_text"), false);
        ui.add_space(6.0);
        if !wizard.picture_pinned {
            self.step_picture(ui, scheme, wizard, Step::Texts);
        }
        Self::paragraph(ui, scheme, &t("as_angaben_unten"), true);
        let keys = [
            (game_texts::SETTING_ON, "s_sp_an", "as_inj_an_h"),
            (game_texts::SETTING_AUTO, "s_sp_auto", "as_inj_auto_h"),
            (injection::SETTING_DETAILS, "s_sp_angaben", "as_angaben_h"),
            (injection::ranks::SETTING, "s_sp_rang", "as_rang_h"),
        ];
        for (k, (key, name, hint)) in keys.into_iter().enumerate() {
            // The same switches as on the page of the game: a note switched off takes the notes out of the text
            // file where they stand, instead of only changing the setting.
            if Self::switch_row(ui, scheme, &t(name), &t(hint), &mut wizard.notes[k]) {
                self.game_texts.switch_note(key, wizard.notes[k]);
            }
        }
        ui.add_space(12.0);
        Self::paragraph(ui, scheme, &t("inj_text"), false);
        ui.add_space(4.0);
        Self::paragraph(ui, scheme, &t("inj_wie"), true);
        ui.add_space(4.0);
        Self::paragraph(ui, scheme, &t("inj_fremd"), true);
        ui.add_space(8.0);
        // Nothing is chosen at first: the game is changed only on a click, and a source that replaces the text
        // file asks first (once per source). The card is the one of the page of the translation; "Keine Übersetzung"
        // asks nothing here and writes nothing, as in 4.12.
        game_texts_page::show_main_choice(ui, scheme, &mut self.game_texts);
    }

    /// The picture of a step, drawn from the state of the controls (the same whether it stands above the area
    /// that scrolls or in it).
    fn step_picture(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &Wizard, step: Step) {
        match step {
            Step::Overlay => {
                let bar = if self.bar_side == settings::BarSide::Bottom {
                    overlay::BarSide::Bottom
                } else {
                    overlay::BarSide::Top
                };
                // What the overlay of the picture learns (a click that did not happen) is nothing.
                let _ = preview::overlay_picture(ui, scheme, self.overlay_opacity as f32 / 100.0, bar);
            }
            Step::Texts => {
                let rep_on =
                    self.store.as_ref().is_none_or(|s| s.paths.setting_bool(injection::SETTING_TITLE_REP, true));
                preview::contract_title_picture(ui, scheme, wizard.notes[0], rep_on);
            }
            Step::Start if cfg!(windows) => preview::taskbar_picture(ui, scheme, wizard.tray),
            _ => {}
        }
    }

    /// The overlay in the game: the picture comes first and follows every control below it at once.
    fn step_overlay(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &Wizard) {
        Self::paragraph(ui, scheme, &t("as_spaeter"), true);
        Self::paragraph(ui, scheme, &t("as_overlay_text"), false);
        ui.add_space(6.0);
        if !wizard.picture_pinned {
            self.step_picture(ui, scheme, wizard, Step::Overlay);
        }
        Self::row(ui, scheme, &t("s_ov_modus"), &t("as_modus_h"), |row| {
            if widgets::radio(row, scheme, !self.overlay_popup, &t("s_ov_immer")).clicked() {
                self.overlay_popup = false;
            }
            if widgets::radio(row, scheme, self.overlay_popup, &t("s_ov_popup")).clicked() {
                self.overlay_popup = true;
            }
        });
        let name = t("e_deckkraft");
        Self::row(ui, scheme, &name, &t("as_deckkraft_h"), |row| {
            let slider = row.add(widgets::Slider::new(&mut self.overlay_opacity, 30, 100, scheme).width(150.0));
            let (enabled, now) = (slider.enabled(), self.overlay_opacity as f64);
            slider.widget_info(|| egui::WidgetInfo::slider(enabled, now, &name));
            row.label(format!("{} %", self.overlay_opacity));
        });
    }

    fn step_display(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        Self::paragraph(ui, scheme, &t("as_spaeter"), true);
        // The size is the one of the whole program, not of the overlay: it belongs here, not under the picture.
        Self::row(ui, scheme, &t("hf_schrift"), &t("as_schrift_h"), |row| {
            size_slider(row, scheme, &mut self.size);
        });
        size_presets(ui, scheme, &mut self.size);
        if Self::switch_row(ui, scheme, &t("s_zeit"), &t("as_zeit_h"), &mut wizard.show_time) {
            self.put_setting("spielzeit_zeigen", Value::Bool(wizard.show_time));
        }
        Self::row(ui, scheme, &t("s_da_schema"), &t("as_schema_h"), |row| {
            row.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            for s in theme::schemes() {
                if preview::scheme_card(row, scheme, s, s.id == scheme.id).clicked() {
                    theme::set_current(s.id);
                }
            }
        });
        Self::row(ui, scheme, &t("as_umfang"), &t("as_umfang_h"), |row| {
            let before = wizard.all_blueprints;
            if widgets::radio(row, scheme, !wizard.all_blueprints, &t("as_umfang_nur")).clicked() {
                wizard.all_blueprints = false;
            }
            if widgets::radio(row, scheme, wizard.all_blueprints, &t("as_umfang_alle")).clicked() {
                wizard.all_blueprints = true;
            }
            if wizard.all_blueprints != before {
                self.put_setting(export::ALL_SETTING, Value::Bool(wizard.all_blueprints));
            }
        });
    }

    fn step_start(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard) {
        Self::paragraph(ui, scheme, &t("as_spaeter"), true);
        // The icon next to the clock exists on Windows only; so does the picture of it.
        if cfg!(windows) && !wizard.picture_pinned {
            ui.add_space(4.0);
            self.step_picture(ui, scheme, wizard, Step::Start);
        }
        if self.autostart.possible() {
            let name = t(if cfg!(windows) { "autostart_win" } else { "autostart_linux" });
            let mut on = self.autostart_on;
            if Self::switch_row(ui, scheme, &name, &t("as_autostart_h"), &mut on) {
                self.autostart_on = on;
                let said = self.change_autostart(on);
                self.pin_notice = Some((ui.input(|i| i.time), said));
            }
        }
        // The icon next to the clock exists on Windows only; elsewhere not even shown.
        if cfg!(windows) && Self::switch_row(ui, scheme, &t("s_tray"), &t("as_tray_h"), &mut wizard.tray) {
            self.put_setting(overlay::keys::TRAY, Value::Bool(wizard.tray));
        }
    }

    fn setup_buttons(&mut self, ui: &mut Ui, scheme: &Scheme, wizard: &mut Wizard, step: Step) {
        let last = wizard.state.step + 1 == wizard.steps.len();
        // The game step cannot be confirmed before a folder with a log is meant, except on the way out.
        let game_blocked = step == Step::Game && !matches!(wizard.game_check, GameFolder::Found { .. });
        ui.horizontal(|ui| {
            let start = t("s_mu_wiz_anfang");
            let first = (wizard.state.step == 0).then_some(start.as_str());
            if ui.add(widgets::Button::new(&t("zurueck"), ButtonKind::Normal, scheme).blocked(first)).clicked() {
                wizard.state.step -= 1;
            }
            let (done, first_all) = (t("s_mu_wiz_schon"), t("s_mu_wiz_erst"));
            // Nothing typed is another reason than a folder without a log.
            let no_folder =
                t(if matches!(wizard.game_check, GameFolder::Empty) { "spielordner_erst" } else { "keine_log_darin" });
            let no_storage = t("ablage_erst_frei");
            // Where nothing can be saved, the data folder cannot be confirmed: the player picks another one.
            let storage_blocked = step == Step::Storage && wizard.storage_blocked.is_some();
            // The overlay would read next to the thread of the assistant: confirmed when it has answered. The
            // account is confirmed once the accounts of the logs are there.
            let reading_busy = (step == Step::Game && wizard.reading.working())
                || (step == Step::Account && wizard.account_search.is_some());
            let still_reading = t("lese_logs");
            let blocked = if last && wizard.state.finished() {
                Some(done.as_str())
            } else if last && !wizard.state.open().is_empty() {
                Some(first_all.as_str())
            } else if game_blocked {
                Some(no_folder.as_str())
            } else if storage_blocked {
                Some(no_storage.as_str())
            } else if reading_busy {
                Some(still_reading.as_str())
            } else {
                None
            };
            let name = t(if last { "fertig" } else { "s_mlg_bestaetigen" });
            // Enter goes on as in 4.12, unless a field is being typed in or a button has the focus (Enter would
            // press that one).
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !ui.ctx().egui_wants_keyboard_input()
                && ui.memory(|m| m.focused()).is_none();
            let pressed = ui.add(widgets::Button::new(&name, ButtonKind::Strong, scheme).blocked(blocked)).clicked()
                || (enter && blocked.is_none());
            if pressed {
                let now = ui.input(|i| i.time);
                let mut saved = true;
                // The first confirmation of a game folder keeps the player at the step: the logs are read now, for
                // this folder, and the second confirmation goes on.
                let mut stays = false;
                if step == Step::Game
                    && let GameFolder::Found { folder, .. } = wizard.game_check.clone()
                    && wizard.game_confirmed.as_deref() != Some(folder.as_str())
                {
                    saved = self.confirm_game(folder.clone());
                    if saved {
                        wizard.logs_kept = self.store.as_ref().map_or(0, |s| s.paths.log_backups(None).len());
                        wizard.game_confirmed = Some(folder);
                        // A game folder that is meant now ends "without game", also for the steps.
                        if wizard.without_game {
                            self.rebuild_steps(wizard, false);
                        }
                        stays = true;
                    }
                }
                // The account confirmed is the own one; the page Allgemein reads the accounts again.
                if step == Step::Account
                    && let (Some(choice), Some(store)) = (wizard.account_choice.as_deref(), &self.store)
                    && detection::set_account(&store.paths, choice)
                {
                    self.detection.invalidate();
                }
                // The last step is the end: written down first, and if that fails the assistant stays.
                let ends = last && wizard.state.open().is_empty();
                if ends && saved {
                    saved = self.finish_setup(now);
                } else if !saved {
                    self.say_setup_fault(now, t("einrichtung_nicht_gespeichert"));
                }
                if saved && !stays {
                    wizard.state.confirm();
                    if last && wizard.state.finished() {
                        wizard.closed = Some(Closed::Finished { overview: false });
                    }
                }
            }
            // The short setup is a courtesy: whoever wants to change nothing leaves it at any step. The note that
            // it was shown is saved already, so it does not come again. (Also on the last page while steps are
            // open: "Fertig" waits for them, skipping does not.)
            let can_end = last && wizard.state.open().is_empty();
            if wizard.short && !can_end {
                // At the right edge, away from the main button: a slip must not end the page.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let skip = ui
                        .add(widgets::Button::new(&t("as_ueberspringen"), ButtonKind::Normal, scheme))
                        .on_hover_text(t("as_ueberspringen_h"));
                    if skip.clicked() {
                        wizard.closed = Some(Closed::Left);
                    }
                });
            }
            if last && wizard.state.open().is_empty() {
                // At the end: the overview instead of just closing (the end is written down first, as with "Fertig").
                if ui.add(widgets::Button::new(&t("liste_oeffnen"), ButtonKind::Normal, scheme)).clicked() {
                    let now = ui.input(|i| i.time);
                    if self.finish_setup(now) {
                        wizard.state.confirm();
                        wizard.closed = Some(Closed::Finished { overview: true });
                    }
                }
            }
        });
    }

    /// The game folder meant is remembered: on the page and in the file.
    fn confirm_game(&mut self, folder: String) -> bool {
        self.live_folder_saved.clone_from(&folder);
        self.folders[0].clone_from(&folder);
        self.game_found = Some(folder.clone());
        self.store.as_ref().is_none_or(|store| setup::confirm_game_folder(&store.paths, &folder))
    }
}

/// The title of a step (the keys of 4.12).
fn step_title(step: Step) -> &'static str {
    match step {
        Step::Welcome => "schritt_willkommen",
        Step::Language => "schritt_sprache",
        Step::Storage => "schritt_ablage",
        Step::Game => "schritt_spiel",
        Step::Account => "schritt_konto",
        Step::Overlay => "schritt_overlay",
        Step::Display => "schritt_anzeige",
        Step::Start => "schritt_start",
        Step::Texts => "schritt_texte",
        Step::Connections => "schritt_verbindungen",
        Step::Done => "schritt_fertig",
    }
}

#[cfg(test)]
mod tests {
    use egui::Pos2;
    use versekit_core::language;

    use super::super::controls::tests::{Bench, preview};
    use super::super::real_data::{host_apart, host_in, real_page};
    use super::*;

    fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
        texts.iter().any(|(text, _)| text.contains(part))
    }

    /// A page on real data in a throwaway folder with a game folder of its own (a `Game.log` in `LIVE`).
    fn page_with_game(name: &str) -> (SamplePage, std::path::PathBuf) {
        let (page, dir) = real_page(name, "{}\n", &[]);
        drop(page);
        let host = host_apart(&dir);
        (SamplePage::with_settings(Paths::new(host)), dir)
    }

    #[test]
    fn the_button_of_the_head_bar_opens_the_assistant_with_its_ten_steps_and_the_count() {
        let _language = crate::language_lock();
        let mut bench = preview();
        bench.click(&language::t("hf_einrichtung"));
        let texts = bench.settle();
        assert!(bench.page.setup_open(), "the assistant is open");
        for key in [
            "schritt_sprache",
            "schritt_ablage",
            "schritt_spiel",
            "schritt_konto",
            "schritt_overlay",
            "schritt_anzeige",
            "schritt_texte",
            "schritt_verbindungen",
            "schritt_start",
            "schritt_fertig",
        ] {
            assert!(shows(&texts, &language::t(key)), "{key} stands in the list");
        }
        let count = tf("schritt_von", &[("0", Arg::from(1)), ("1", Arg::from(10))]);
        assert!(shows(&texts, &count), "{count}");
        assert!(shows(&texts, &language::t("sprache_auto")), "the first step: the language");
    }

    #[test]
    fn the_assistant_walks_through_confirms_and_remembers_that_the_program_is_set_up() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-lauf");
        let mut bench = Bench::new(page);
        assert!(bench.page.open_setup_if_needed(), "a new program needs the assistant");
        let confirm = language::t("s_mlg_bestaetigen");
        for _ in 0..200 {
            // The reading step waits for its thread.
            std::thread::sleep(std::time::Duration::from_millis(10));
            let texts = bench.settle();
            let at = if shows(&texts, &confirm) { confirm.clone() } else { String::new() };
            if at.is_empty() {
                break;
            }
            bench.click(&at);
        }
        // All steps before the last are confirmed now; the last one is "Fertig". (The window slides in for a dozen
        // frames; the button is where it stays only after that.)
        for _ in 0..4 {
            bench.settle();
        }
        let texts = bench.settle();
        let done = language::t("fertig");
        // "Fertig" is also the name of the last step in the list at the left: the button is the one furthest right.
        let button = texts.iter().filter(|(t, _)| *t == done).map(|(_, at)| *at).max_by(|a, b| a.x.total_cmp(&b.x));
        let button = button.expect("the end is reached");
        bench.click_at(button);
        bench.settle();
        let state = bench.page.setup.as_ref().map(|w| (w.state.step, w.state.done.clone(), w.closed));
        let end = bench.page.take_setup_end().unwrap_or_else(|| panic!("the assistant did not end: {state:?}"));
        assert_eq!(end, SetupEnd { finished: true, overview: false });
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"einrichtung_fertig\": true"), "{file}");
        assert!(file.contains("\"spiel_ordner\""), "the game folder that was found is confirmed: {file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A page on real data whose only game folder is the one of the throwaway folder from the start: no account
    /// of the logs of this computer gets into its settings (as with [`page_with_game`], whose first page looks for
    /// the game on the drives).
    fn page_apart(name: &str) -> (SamplePage, std::path::PathBuf) {
        let dir = super::super::real_data::folder(name);
        std::fs::write(dir.join("einstellungen.json"), "{}\n").unwrap();
        theme::set_current("original");
        language::set_thread_language(language::Choice::De);
        (SamplePage::with_settings(Paths::new(host_apart(&dir))), dir)
    }

    /// Writes logs of `accounts` (name, number of logs) into the backups of the game folder of `dir`.
    fn logs_of(dir: &std::path::Path, accounts: &[(&str, usize)]) {
        let backups = dir.join("LIVE").join("logbackups");
        std::fs::create_dir_all(&backups).unwrap();
        for (name, count) in accounts {
            for k in 0..*count {
                let text = format!("<Legacy login response> [CIG-net] User Login Success - Handle[{name}] - Time\n");
                std::fs::write(backups.join(format!("{name}-{k}.log")), text).unwrap();
            }
        }
    }

    /// Frames until `part` is drawn (the accounts are read in a thread), at most a minute.
    fn until_shown(bench: &mut Bench, part: &str) -> Vec<(String, Pos2)> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let texts = bench.settle();
            if shows(&texts, part) || std::time::Instant::now() > deadline {
                return texts;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn the_account_step_offers_the_accounts_of_the_logs_and_the_one_confirmed_is_the_own_one() {
        let _language = crate::language_lock();
        let (page, dir) = page_apart("einrichtung-konto-mehrere");
        logs_of(&dir, &[("Alice", 2), ("Bob", 1)]);
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Account).unwrap();
        }
        let texts = until_shown(&mut bench, &language::t("as_acc_mehrere"));
        assert!(shows(&texts, &language::t("as_acc_mehrere")), "several: the choice {texts:?}");
        let bob = crate::detection_page::account_word("Bob", 1);
        assert!(shows(&texts, &bob) && shows(&texts, &language::t("s_er_acc_alle")), "{texts:?}");
        bench.click(&bob);
        bench.click(&language::t("s_mlg_bestaetigen"));
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"eigener_account\": \"Bob\""), "the account confirmed is stored: {file}");
        let w = bench.page.setup.as_ref().expect("still open");
        assert_eq!(w.steps[w.state.step], Step::Overlay, "and the assistant goes on");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_account_step_shows_a_single_account_to_confirm() {
        let _language = crate::language_lock();
        let (page, dir) = page_apart("einrichtung-konto-einer");
        logs_of(&dir, &[("Alice", 1)]);
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Account).unwrap();
        }
        let found = tf("as_acc_einer", &[("0", Arg::from(crate::detection_page::account_word("Alice", 1)))]);
        let texts = until_shown(&mut bench, &found);
        assert!(shows(&texts, &found) && shows(&texts, &language::t("as_acc_einer_h")), "{texts:?}");
        assert!(!shows(&texts, &language::t("s_er_acc_alle")), "one account: no choice {texts:?}");
        bench.click(&language::t("s_mlg_bestaetigen"));
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"eigener_account\": \"Alice\""), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_game_step_waits_for_a_folder_with_a_log_and_has_the_way_out() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-spiel");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        // To the game step, then a folder that holds no log.
        for _ in 0..2 {
            bench.click(&language::t("s_mlg_bestaetigen"));
        }
        let empty = dir.join("leer");
        std::fs::create_dir_all(&empty).unwrap();
        if let Some(w) = bench.page.setup.as_mut() {
            w.game_text = empty.to_string_lossy().into_owned();
            w.found_by_itself = false;
        }
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("keine_log_darin")), "the folder has no log: said");
        assert!(shows(&texts, &language::t("ohne_spiel")), "and the way out stands there");
        let step = bench.page.setup.as_ref().map(|w| w.state.step);
        bench.click(&language::t("s_mlg_bestaetigen"));
        bench.settle();
        assert_eq!(bench.page.setup.as_ref().map(|w| w.state.step), step, "blocked: no step further");
        let game_at = bench.page.setup.as_ref().and_then(|w| w.steps.iter().position(|s| *s == Step::Game));
        assert!(!bench.page.setup.as_ref().is_some_and(|w| w.state.done[game_at.unwrap_or(0)]), "not confirmed");
        // The way out: the texts of the game are gone, the choice is kept in the file, the assistant goes on to
        // the overlay.
        bench.click(&language::t("ohne_spiel"));
        let texts = bench.settle();
        assert!(!shows(&texts, &language::t("schritt_texte")), "no texts without a game");
        let w = bench.page.setup.as_ref().expect("still open");
        assert_eq!(w.steps[w.state.step], Step::Overlay);
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"einrichtung_ohne_spiel\": true"), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_overlay_step_shows_the_overlay_in_a_frame_of_the_game_without_a_countdown() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-overlay");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Overlay).unwrap();
        }
        let texts = bench.settle();
        // The placeholder of the game, and the real overlay with its made-up finds.
        assert!(shows(&texts, &language::t("as_vorschau_spiel")), "the game is a placeholder");
        assert!(shows(&texts, "Pembroke Helmet") && shows(&texts, "Arclight Pistol"), "the finds of the picture");
        // The pop-up mode draws the same overlay: no line counts down to its folding away.
        bench.page.overlay_seconds = 9;
        bench.click(&language::t("s_ov_popup"));
        let texts = bench.settle();
        assert!(shows(&texts, "Pembroke Helmet"), "pop-up: the same picture");
        assert!(!texts.iter().any(|(text, _)| text.contains("9 s")), "pop-up: no countdown: {texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_overview_button_stands_on_the_last_page_before_the_end_and_ends_the_assistant() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-uebersicht-vor");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            // The way a player arrives: all steps before the last are confirmed, the last is not.
            let last = w.steps.len() - 1;
            for k in 0..last {
                w.state.done[k] = true;
            }
            w.state.step = last;
        }
        // The window slides in for a dozen frames; the buttons are where they stay only after that.
        for _ in 0..4 {
            bench.settle();
        }
        bench.click(&language::t("liste_oeffnen"));
        bench.settle();
        let end = bench.page.take_setup_end().expect("the assistant ended");
        assert_eq!(end, SetupEnd { finished: true, overview: true });
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(file.contains("\"einrichtung_fertig\": true"), "the end is written down: {file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_game_step_names_the_channels_reads_the_logs_and_says_how_many_blueprints_it_took_over() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-lesen");
        let line = |find: &str| {
            format!(
                "<2026-10-01T12:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \
                 \"Received Blueprint: {find}: \" [1]\n"
            )
        };
        std::fs::write(
            dir.join("LIVE").join("Game.log"),
            format!("{}{}", line("Arclight Pistol"), line("Pembroke Helmet")),
        )
        .unwrap();
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Game).unwrap();
        }
        // Before the confirmation nothing is read, and nothing is written: the folder as typed may be the wrong
        // channel, and reading puts blueprints into the collection.
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("as_lesen_nach")), "the reading waits for the confirmation");
        assert!(!shows(&texts, &language::t("lese_logs")), "no reading yet");
        assert!(!dir.join(versekit_core::collection::FILE_NAME).exists(), "nothing written before the answer");
        let before = bench.page.setup.as_ref().map(|w| w.state.step);
        // The first confirmation takes the folder and reads; the player stays at the step.
        bench.click(&language::t("s_mlg_bestaetigen"));
        assert_eq!(bench.page.setup.as_ref().map(|w| w.state.step), before, "stays while the logs are read");
        assert!(bench.page.setup.as_ref().is_some_and(|w| w.game_confirmed.is_some()));
        // The thread needs a moment: frames until the result stands there.
        let wanted = tf("nachgelesen_gross", &[("0", Arg::from(2)), ("1", Arg::from(0))]);
        let mut texts = bench.settle();
        for _ in 0..100 {
            if shows(&texts, &wanted) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            texts = bench.settle();
        }
        let seen: Vec<&str> = texts.iter().map(|(t, _)| t.as_str()).collect();
        assert!(shows(&texts, &wanted), "{wanted} / {seen:?}");
        // No stored session is kept, so the logs have a gap: said, with the way to tick off the rest by hand.
        assert!(shows(&texts, &language::t("nachtragen_hinweis")), "the hint for the older blueprints");
        assert!(!shows(&texts, &language::t("lese_logs")), "the waiting line is gone");
        let stock = std::fs::read_to_string(dir.join(versekit_core::collection::FILE_NAME)).unwrap_or_default();
        assert!(stock.contains("Arclight Pistol") && stock.contains("Pembroke Helmet"), "taken over: {stock}");
        // The second confirmation goes on.
        bench.click(&language::t("s_mlg_bestaetigen"));
        bench.settle();
        assert_ne!(bench.page.setup.as_ref().map(|w| w.state.step), before, "the second confirmation goes on");
        // The last page says how many blueprints the player has now, in the tile of the start page.
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.len() - 1;
        }
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("as_kachel_text")), "the tile is announced");
        assert!(texts.iter().any(|(t, _)| t == "2"), "the number of the tile: 2 / {texts:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_game_folder_confirmed_after_the_way_out_brings_the_steps_of_the_game_back() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-ausweg-zurueck");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        // The player went on without a game once; now a folder is typed and confirmed.
        if let Some(w) = bench.page.setup.as_mut() {
            w.found_by_itself = false;
            w.without_game = true;
        }
        let mut wizard = bench.page.setup.take().expect("open");
        bench.page.rebuild_steps(&mut wizard, true);
        wizard.state.step = wizard.steps.iter().position(|s| *s == Step::Game).unwrap();
        assert!(!wizard.steps.contains(&Step::Texts), "without a game: no texts");
        bench.page.setup = Some(wizard);
        bench.settle();
        bench.click(&language::t("s_mlg_bestaetigen"));
        bench.settle();
        let w = bench.page.setup.as_ref().expect("open");
        assert!(!w.without_game, "the folder meant now ends 'without game'");
        assert!(w.steps.contains(&Step::Texts), "and the texts of the game are a step again");
        assert_eq!(w.steps[w.state.step], Step::Game, "the player stays where he is");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_switch_of_the_notes_in_the_assistant_acts_as_on_the_page_of_the_game() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-schalter");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Texts).unwrap();
        }
        for _ in 0..4 {
            bench.settle();
        }
        let texts = bench.settle();
        let label = language::t("s_sp_an");
        let at = texts.iter().find(|(t, _)| *t == label).map(|(_, at)| *at).expect("the label of the master switch");
        // The switch stands in the column of controls of its row, like those of the settings pages.
        let said = tf("s_sp_an_sagen", &[("0", Arg::from(language::t("e_aus")))]);
        let mut flipped = false;
        for x in (0..60).map(|i| at.x + 200.0 + i as f32 * 12.0) {
            bench.click_at(Pos2::new(x, at.y + 8.0));
            bench.settle();
            if bench.page.game_texts.said().is_some_and(|(text, _)| *text == said) {
                flipped = true;
                break;
            }
        }
        assert!(flipped, "the page of the game says it was switched off, as with its own switch");
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(file.contains(&format!("\"{}\": false", game_texts::SETTING_ON)), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The settings file of a real installation of the 4.12 line (made anonymous), as a program set up before 5.0
    /// has it: set up (`einrichtung_fertig`), no version of the setup.
    const SETTINGS_OF_4_12: &str = include_str!("../../../../tools/pruefdaten/einstellungen-4-12-beispiel.json");

    /// A page on a copy of that file, with a game folder (a `Game.log` in `LIVE`) whose log is `age` seconds old.
    /// The opacity the copy has is not the default of the program (93), so that "filled in" can be told from "left
    /// at the default". The process list of the computer is not asked (the game may run on it).
    fn page_set_up_with_4_12(name: &str, age: u64) -> (SamplePage, std::path::PathBuf) {
        let settings = SETTINGS_OF_4_12.replace("\"deckkraft_prozent\": 93", "\"deckkraft_prozent\": 71");
        assert!(settings.contains("\"deckkraft_prozent\": 71"));
        let (page, dir) = real_page(name, &settings, &[]);
        drop(page);
        let live = dir.join("LIVE");
        let game = live.to_string_lossy().into_owned();
        let host = host_in(
            &dir,
            &[(versekit_core::paths::ENV_INSTALL_DIR, game.as_str()), (versekit_core::game_start::ENV_NO_PROCESS, "1")],
        );
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("Game.log"), "").unwrap();
        let log = dir.join("LIVE").join("Game.log");
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options().write(true).open(&log).unwrap().set_modified(when).unwrap();
        (SamplePage::with_settings(Paths::new(host)), dir)
    }

    #[test]
    fn the_short_setup_opens_once_for_a_program_set_up_with_4_12_filled_in_and_can_be_skipped() {
        let _language = crate::language_lock();
        let (mut page, dir) = page_set_up_with_4_12("kurz-4-12", 3600);
        assert!(!page.setup_open());
        assert!(page.open_short_if_needed(), "a program set up before 5.0 is welcomed");
        // The note that it was shown is in the file already (it comes once, also if the window is closed hard).
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"einrichtung_version\": 5"), "{file}");
        assert!(file.contains("\"einrichtung_fertig\": true"), "what the player had stays: {file}");
        assert!(file.contains("\"deckkraft_prozent\": 71"), "so do his settings: {file}");
        let mut bench = Bench::new(page);
        for _ in 0..4 {
            bench.settle();
        }
        let texts = bench.settle();
        let title = tf("as_willkommen_titel", &[("0", Arg::from(setup::VERSION))]);
        assert!(shows(&texts, &title), "the title: {title}");
        assert!(shows(&texts, &language::t("as_willkommen_1")) && shows(&texts, &language::t("as_ueberspringen")));
        // What is new in 5.0, the six points of the announcement.
        assert!(shows(&texts, &tf("as_neu_titel", &[("0", Arg::from(setup::VERSION))])), "the heading of the list");
        for key in ["as_neu_1", "as_neu_2", "as_neu_3", "as_neu_4", "as_neu_5", "as_neu_6"] {
            assert!(shows(&texts, &language::t(key)), "{key} stands on the welcome page");
        }
        // Short: the steps of the short setup, not those of the full one.
        let w = bench.page.setup.as_ref().expect("open");
        assert!(w.short && w.steps.first() == Some(&Step::Welcome) && !w.steps.contains(&Step::Language));
        // Filled in: the opacity he has is the one the step shows (after the welcome and the account, which waits
        // for the accounts of the logs).
        bench.click(&language::t("s_mlg_bestaetigen"));
        until_shown(&mut bench, &language::t("as_acc_text"));
        for _ in 0..200 {
            if bench.page.setup.as_ref().is_some_and(|w| w.steps[w.state.step] == Step::Overlay) {
                break;
            }
            bench.click(&language::t("s_mlg_bestaetigen"));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let texts = (0..4).map(|_| bench.settle()).last().unwrap_or_default();
        assert!(shows(&texts, "71 %"), "the overlay step shows the opacity of the file, not the default: {texts:?}");
        // Skipped: it ends at once, the file is the one it was (the note of the version apart), and it does not
        // come again.
        let before = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        bench.click(&language::t("as_ueberspringen"));
        bench.settle();
        let end = bench.page.take_setup_end().expect("the short setup ended");
        assert_eq!(end, SetupEnd { finished: false, overview: false });
        assert!(!bench.page.open_short_if_needed(), "once");
        let after = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert_eq!(before, after, "skipping changes nothing in the settings");
        assert!(!after.contains("spiel_ordner\": \"\"") && !after.contains("einrichtung_ohne_spiel"), "{after}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_short_setup_can_be_skipped_on_the_last_page_while_steps_are_open_and_runs_to_its_end() {
        let _language = crate::language_lock();
        let (mut page, dir) = page_set_up_with_4_12("kurz-letzte-seite", 3600);
        assert!(page.open_short_if_needed());
        let last = page.setup.as_ref().map(|w| w.steps.len() - 1).unwrap();
        page.setup.as_mut().unwrap().state.step = last;
        let mut bench = Bench::new(page);
        for _ in 0..4 {
            bench.settle();
        }
        // Steps are open on the last page: "Fertig" waits, "Überspringen" does not.
        bench.click(&language::t("as_ueberspringen"));
        bench.settle();
        assert_eq!(bench.page.take_setup_end(), Some(SetupEnd { finished: false, overview: false }));
        // Run to its end: every step confirmed, then "Fertig" writes the proof and ends it as finished.
        let mut page = SamplePage::with_settings(Paths::new(bench.page.store.as_ref().unwrap().paths.host.clone()));
        let paths = Paths::new(page.store.as_ref().unwrap().paths.host.clone());
        assert!(paths.set_setting(setup::VERSION_KEY, Value::int(0)), "as if it had not been shown");
        assert!(page.open_short_if_needed());
        let mut bench = Bench::new(page);
        let confirm = language::t("s_mlg_bestaetigen");
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let texts = bench.settle();
            if !shows(&texts, &confirm) {
                break;
            }
            bench.click(&confirm);
        }
        for _ in 0..4 {
            bench.settle();
        }
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("as_kurz_fertig")), "the end page of the short setup");
        let done = language::t("fertig");
        let button = texts.iter().filter(|(t, _)| *t == done).map(|(_, at)| *at).max_by(|a, b| a.x.total_cmp(&b.x));
        bench.click_at(button.expect("the button"));
        bench.settle();
        assert_eq!(bench.page.take_setup_end(), Some(SetupEnd { finished: true, overview: false }));
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"einrichtung_version\": 5"), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_short_setup_does_not_come_while_the_game_runs_in_another_channel() {
        let _language = crate::language_lock();
        // The log of the folder read is old; the game runs in the test channel next to it.
        let (mut page, dir) = page_set_up_with_4_12("kurz-anderer-kanal", 3600);
        let ptu = dir.join("PTU");
        std::fs::create_dir_all(&ptu).unwrap();
        std::fs::write(ptu.join("Game.log"), "").unwrap();
        assert!(!page.open_short_if_needed(), "the game runs, in another channel");
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(!file.contains("einrichtung_version"), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_short_setup_does_not_come_while_the_game_runs_and_comes_at_a_later_start() {
        let _language = crate::language_lock();
        // The log was written a moment ago: the game runs.
        let (mut page, dir) = page_set_up_with_4_12("kurz-spiel-laeuft", 5);
        assert!(!page.open_short_if_needed(), "not while the game runs");
        assert!(!page.setup_open());
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(!file.contains("einrichtung_version"), "and nothing is noted, so that it comes later: {file}");
        // The game is closed (the log is old): the next start brings it.
        let log = dir.join("LIVE").join("Game.log");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options().write(true).open(&log).unwrap().set_modified(old).unwrap();
        assert!(page.open_short_if_needed());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_short_setup_does_not_come_for_a_new_program_and_not_after_the_full_one() {
        let _language = crate::language_lock();
        // New: the full assistant has the field.
        let (mut page, dir) = page_with_game("kurz-neu");
        assert!(!page.open_short_if_needed(), "a new program is not welcomed back");
        // The full assistant, run to its end, writes the version: the short one never follows it.
        assert!(page.open_setup_if_needed());
        let paths = page.store.as_ref().map(|s| Paths::new(s.paths.host.clone())).unwrap();
        assert!(setup::finish(&paths));
        page.setup = None;
        assert!(!page.open_short_if_needed(), "nobody gets both");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn enter_confirms_a_step_when_nothing_has_the_focus_as_in_4_12() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-enter");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        bench.settle();
        let enter = |pressed: bool| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame(vec![enter(true)]);
        bench.frame(vec![enter(false)]);
        bench.settle();
        let w = bench.page.setup.as_ref().expect("open");
        assert!(w.state.done[0], "the first step is confirmed by Enter");
        assert_eq!(w.state.step, 1, "and the assistant went on");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_click_beside_the_assistant_does_not_end_it() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-klick");
        let mut bench = Bench::new(page);
        assert!(!bench.page.is_set_up(), "a new program");
        bench.page.open_setup();
        bench.settle();
        // A slip beside the card: the assistant stays (it would end the first start otherwise).
        bench.click_at(Pos2::new(4.0, 4.0));
        bench.settle();
        assert!(bench.page.setup_open(), "a click on the dimmed window does not leave");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_step_of_the_texts_offers_the_sources_and_asks_before_a_source_replaces_the_text_file() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-quellen");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Texts).unwrap();
        }
        bench.settle();
        // The step scrolls: the card of the translation stands at its end.
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        // The card of the page of the translation stands in the step: the list of languages, nothing chosen.
        let paths = bench.page.store.as_ref().map(|s| Paths::new(s.paths.host.clone())).unwrap();
        let sources: Vec<String> = versekit_core::translation::grouped_sources(&paths, None, true).concat();
        let foreign = sources.iter().find(|id| !id.is_empty() && *id != "original").expect("a foreign source");
        let name = versekit_core::translation::display_name(foreign);
        let (language_name, maker) =
            name.split_once(" (").map(|(l, m)| (l.to_string(), m.trim_end_matches(')').to_string())).unwrap();
        let nothing = language::t("s_tq_nichts");
        assert!(shows(&texts, &nothing), "„Keine Übersetzung“ stands in the list: {texts:?}");
        assert!(shows(&texts, &language::t("s_tq_nichts_gewaehlt")), "nothing is chosen at first");
        // The language from the list, then its sources come up.
        bench.click(&nothing);
        bench.settle();
        bench.click(&language_name);
        let texts = bench.settle();
        assert!(shows(&texts, &maker), "{maker} stands in the step: {texts:?}");
        // A foreign source asks first; nothing is written by the question.
        bench.click(&maker);
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("s_sp_warnung_titel")), "the question comes: {texts:?}");
        assert!(bench.page.setup_open(), "the assistant stays open behind it");
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(!file.contains("inj_quelle"), "no source is written before the answer: {file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Scrolls the step under the pointer by `dy` points (negative: down), in ten wheel clicks.
    fn scroll_step(bench: &mut Bench, dy: f32) {
        bench.frame(vec![egui::Event::PointerMoved(Pos2::new(800.0, 450.0))]);
        for _ in 0..10 {
            bench.frame(vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            }]);
        }
        bench.settle();
    }

    #[test]
    fn the_last_page_shows_the_count_the_tile_the_modules_and_the_privacy_line_and_a_module_is_saved_at_once() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-fertig");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.len() - 1;
        }
        let texts = bench.settle();
        // The tile of the start page with its title and the number, announced by a line.
        assert!(shows(&texts, &language::t("as_kachel_text")), "the tile is announced");
        let tile = texts.iter().filter(|(t, _)| t.to_lowercase() == language::t("bauplaene").to_lowercase()).count();
        assert!(tile >= 1, "the tile of the start page carries the title");
        // The rest stands further down in the step that scrolls.
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        assert!(shows(&texts, &language::t("as_datenschutz")), "the privacy line of the declaration");
        assert!(shows(&texts, &language::t("s_mo_hinweis")), "what cannot be hidden is said");
        for (group, _) in versekit_core::modules::SWITCHABLE {
            let label = language::t(versekit_core::modules::label(group));
            assert!(shows(&texts, &label), "{group} can be switched here");
        }
        // A switch (in the column of controls of its row, as on the page Modules) is in the file at once.
        let label = language::t(versekit_core::modules::label("handel"));
        let at = texts.iter().find(|(t, _)| *t == label).map(|(_, at)| *at).expect("the label of the module");
        let mut off = false;
        for x in (0..60).map(|i| at.x + 200.0 + i as f32 * 12.0) {
            bench.click_at(Pos2::new(x, at.y + 8.0));
            bench.settle();
            let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
            if file.contains("\"modul_handel\": false") {
                off = true;
                break;
            }
        }
        assert!(off, "the module is off in the file");
        // And the menu follows at once, not at the next start.
        assert!(bench.page.module_hidden.contains(&"hf_laeden"), "the pages of the module are hidden now");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_connections_step_shows_the_way_of_the_exporter_and_says_what_is_not_there_yet() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-verbindungen");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Connections).unwrap();
        }
        let texts = bench.settle();
        // The strip of the steps is a picture: its words are in the name for screen readers, and its boxes
        // carry the numbers and the words.
        for key in ["as_fluss_1", "as_fluss_2", "as_fluss_3"] {
            assert!(shows(&texts, &language::t(key)), "{key} stands in the strip");
        }
        // Three buttons alike, one per browser.
        for key in ["as_exporter_firefox", "as_exporter_chrome", "as_exporter_edge"] {
            assert!(shows(&texts, &language::t(key)), "{key} is a button");
        }
        assert!(shows(&texts, &language::t("as_basetool_text")), "the Basetool is named as optional");
        // The release page the buttons of Chrome and Edge open: the one of the Hangar-Exporter.
        let page = versekit_core::credits::HANGAR_EXPORTER;
        assert!(page.starts_with("https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/"));
        // Nothing here is written: the step only shows and opens a page.
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(!file.contains("hangar"), "nothing of the hangar written: {file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_appearance_step_shows_every_scheme_as_a_small_window_and_takes_the_one_clicked() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-schema");
        let before = theme::current().id;
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Display).unwrap();
        }
        bench.settle();
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        for s in theme::schemes() {
            assert!(shows(&texts, &language::t(s.label)), "{} stands under its card", s.label);
        }
        let other = theme::schemes().iter().find(|s| s.id != before).unwrap();
        bench.click(&language::t(other.label));
        bench.settle();
        let after = theme::current().id;
        theme::set_current(before);
        assert_eq!(after, other.id, "the card clicked is the scheme in use now");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_texts_step_shows_a_title_before_and_after_and_follows_the_switch_of_the_notes() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-titel");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Texts).unwrap();
        }
        let texts = bench.settle();
        let title = language::t("as_vorschau_auftrag");
        assert!(shows(&texts, &language::t("as_vorher")) && shows(&texts, &language::t("as_nachher")));
        // With the notes on: the title carries the mark of the notes in the second line, not in the first.
        let marked: Vec<&String> = texts.iter().map(|(t, _)| t).filter(|t| t.contains(&title)).collect();
        assert!(marked.iter().any(|t| t.contains("[BP")), "the title after: {marked:?}");
        assert!(marked.iter().any(|t| !t.contains("[BP")), "the title before: {marked:?}");
        // The master switch off: both lines are alike.
        if let Some(w) = bench.page.setup.as_mut() {
            w.notes[0] = false;
        }
        let texts = bench.settle();
        let marked: Vec<&String> = texts.iter().map(|(t, _)| t).filter(|t| t.contains(&title)).collect();
        assert!(marked.iter().all(|t| !t.contains("[BP")), "no notes: {marked:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_translation_in_the_assistant_asks_nothing_and_writes_nothing_as_in_4_12() {
        let _language = crate::language_lock();
        for (name, kept) in [("einrichtung-unberuehrt", false), ("einrichtung-unberuehrt-quelle", true)] {
            let (page, dir) = page_with_game(name);
            let mut bench = Bench::new(page);
            bench.page.open_setup();
            if let Some(w) = bench.page.setup.as_mut() {
                w.state.step = w.steps.iter().position(|s| *s == Step::Texts).unwrap();
            }
            if kept {
                // A source chosen on an earlier run stays when the player leaves the texts alone now.
                bench.page.put_setting(versekit_core::translation::SETTING_SOURCE, Value::str("starstrings"));
            }
            bench.settle();
            scroll_step(&mut bench, -400.0);
            // „Keine Übersetzung“ from the list of languages: the field opens it, its first entry chooses it.
            let texts = bench.settle();
            let shown = texts
                .iter()
                .map(|(text, _)| text.clone())
                .find(|text| *text == language::t("s_tq_nichts") || text.starts_with("English"))
                .unwrap_or_else(|| language::t("s_tq_nichts"));
            bench.click(&shown);
            bench.settle();
            bench.click_nth(&language::t("s_tq_nichts"), usize::from(shown == language::t("s_tq_nichts")));
            let texts = bench.settle();
            let question = language::t("s_tq_nichts_warnung");
            let first = question.split("\n\n").next().unwrap().to_string();
            assert!(!shows(&texts, &first), "no question comes: {first}");
            // Said in both cases: also over a source chosen before, which stays as it is.
            let said = language::t("s_tq_unberuehrt");
            let rest = said.split_once("}: ").map_or(said.as_str(), |(_, r)| r);
            assert!(shows(&texts, rest), "it is said that the file is left alone: {rest}");
            let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
            assert_eq!(file.contains("inj_quelle"), kept, "nothing written, nothing deleted: {file}");
            // The assistant opened anew starts with nothing chosen.
            bench.page.setup = None;
            bench.page.open_setup();
            if let Some(w) = bench.page.setup.as_mut() {
                w.state.step = w.steps.iter().position(|s| *s == Step::Texts).unwrap();
            }
            bench.settle();
            scroll_step(&mut bench, -400.0);
            let texts = bench.settle();
            assert!(!shows(&texts, rest), "not chosen again by itself: {rest}");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn the_last_step_does_not_end_the_assistant_while_other_steps_are_open() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-offen");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.len() - 1;
        }
        let texts = bench.settle();
        // The steps still open are named, so the player knows what "Fertig" waits for.
        let open: Vec<String> = bench
            .page
            .setup
            .as_ref()
            .map(|w| w.steps[..w.steps.len() - 1].iter().map(|s| language::t(step_title(*s))).collect())
            .unwrap();
        let said = tf("s_mu_wiz_offen", &[("steps", Arg::from(open.join(", ")))]);
        assert!(shows(&texts, &said), "{said}");
        let done = language::t("fertig");
        let button = texts.iter().filter(|(t, _)| *t == done).map(|(_, at)| *at).max_by(|a, b| a.x.total_cmp(&b.x));
        bench.click_at(button.expect("the button of the last step"));
        bench.settle();
        assert!(bench.page.setup_open(), "nothing ended");
        assert!(bench.page.take_setup_end().is_none());
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(!file.contains("einrichtung_fertig"), "nothing was written down: {file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_overview_opens_at_the_end_when_the_player_asks_for_it() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-uebersicht");
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_liste");
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.done = vec![true; w.steps.len()];
            w.state.step = w.steps.len() - 1;
        }
        for _ in 0..5 {
            bench.settle();
        }
        bench.click(&language::t("liste_oeffnen"));
        bench.settle();
        assert_eq!(bench.page.take_setup_end(), Some(SetupEnd { finished: true, overview: true }));
        assert_eq!(bench.page.page_key(), "", "the overview, not the page the assistant was opened on");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_data_folder_that_cannot_be_written_keeps_the_assistant_at_that_step_and_says_so() {
        let _language = crate::language_lock();
        // A file where the data folder should be: nothing can be saved.
        let dir = super::super::real_data::folder("einrichtung-gesperrt");
        let blocked = dir.join("kein-ordner");
        std::fs::write(&blocked, "ich bin eine Datei").unwrap();
        language::set_thread_language(language::Choice::De);
        let page = SamplePage::with_settings(Paths::new(super::super::real_data::host_in(&blocked, &[])));
        let mut bench = Bench::new(page);
        assert!(bench.page.open_setup_if_needed(), "nothing can be saved: the assistant comes");
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Storage).unwrap();
        }
        let texts = bench.settle();
        assert!(shows(&texts, "VerseKit darf hier nicht speichern"), "the reason is said: {texts:?}");
        assert!(!shows(&texts, &language::t("ablage_ok")), "a blocked folder is not called good: {texts:?}");
        bench.click(&language::t("s_mlg_bestaetigen"));
        bench.settle();
        let w = bench.page.setup.as_ref().expect("still open");
        assert_eq!(w.steps[w.state.step], Step::Storage, "blocked: no step further");
        assert!(!w.state.done[w.state.step], "not confirmed");
        // Finishing fails the same way: the assistant does not report itself done.
        let mut page = bench.page;
        assert!(!page.finish_setup(0.0), "nothing could be written");
        assert!(page.pin_fault.is_some());
        assert!(!page.is_set_up());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_data_folder_that_can_be_written_is_said_to_be_good_in_both_languages() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-ablage-gut");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Storage).unwrap();
        }
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            let texts = bench.settle();
            assert!(shows(&texts, &language::t("ablage_ok")), "{choice:?}: {texts:?}");
            let blocked = language::t("ablage_gesperrt");
            let head = blocked.split('(').next().unwrap_or_default().trim();
            assert!(!shows(&texts, head), "{choice:?}: not blocked {texts:?}");
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_last_page_names_the_gear_and_the_way_back_in_both_languages() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-zahnrad");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.done = vec![true; w.steps.len()];
            w.state.step = w.steps.len() - 1;
        }
        for choice in [language::Choice::De, language::Choice::En] {
            language::set_thread_language(choice);
            bench.settle();
            // The tips stand further down in the step that scrolls.
            scroll_step(&mut bench, -800.0);
            let texts = bench.settle();
            assert!(shows(&texts, &language::t("tipp_liste")), "{choice:?}: {texts:?}");
            let again = tf("as_tipp_erneut", &[("0", Arg::from(language::t("hf_einrichtung")))]);
            assert!(shows(&texts, &again), "{choice:?}: {texts:?}");
        }
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn leaving_the_assistant_before_its_end_keeps_the_program_unset_up() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-weg");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        bench.settle();
        // Esc closes the modal window.
        bench.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        bench.settle();
        let end = bench.page.take_setup_end();
        assert_eq!(end, Some(SetupEnd { finished: false, overview: false }));
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default();
        assert!(!file.contains("einrichtung_fertig"), "{file}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Writes the entry into the home of the throwaway folder, for the test program.
    fn make_in_the_throwaway_home(host: &Host) -> Result<std::path::PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        desktop_entry::create_in(&desktop_entry::data_home(host), &exe, &desktop_entry::run_arguments(host), None)
    }

    fn make_nothing(_: &Host) -> Result<std::path::PathBuf, String> {
        Err("no access".to_string())
    }

    /// The assistant on its last page with the offer of the menu on (the real offer stands where the system has
    /// such menus), and the page scrolled to its end.
    fn last_page_with_menu(bench: &mut Bench, make: MenuMaker) -> Vec<(String, Pos2)> {
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.len() - 1;
            w.menu_offer = true;
            w.menu_make = make;
        }
        bench.settle();
        scroll_step(bench, -400.0);
        bench.settle()
    }

    #[test]
    fn the_last_page_offers_the_entry_in_the_menu_and_writes_it_on_the_click_and_the_button_stays() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-menue");
        let mut bench = Bench::new(page);
        let texts = last_page_with_menu(&mut bench, make_in_the_throwaway_home);
        assert!(shows(&texts, &language::t("as_menue_frage")), "the question stands under the tips");
        assert!(shows(&texts, &language::t("as_menue_knopf")));
        let file = desktop_entry::target_file(&desktop_entry::data_home(&host_in(&dir, &[])));
        assert!(!file.exists(), "nothing is written before the click");
        bench.click(&language::t("as_menue_knopf"));
        // The answer stands under the button, a line further down.
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        assert!(file.is_file(), "the entry is written: {file:?}");
        let said = tf("as_menue_da", &[("0", Arg::from(file.to_string_lossy().as_ref()))]);
        assert!(shows(&texts, &said), "the answer names the file: {said}");
        let answer = bench.page.setup.as_ref().and_then(|w| w.menu_said.clone());
        assert_eq!(answer.map(|(worked, _)| worked), Some(true), "the answer is marked as a success");
        assert!(shows(&texts, &language::t("as_menue_knopf")), "the button stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn when_the_entry_cannot_be_written_the_answer_says_why_and_nothing_is_written() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-menue-fehler");
        let mut bench = Bench::new(page);
        last_page_with_menu(&mut bench, make_nothing);
        bench.click(&language::t("as_menue_knopf"));
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        let said = tf("as_menue_nein", &[("0", Arg::from("no access"))]);
        assert!(shows(&texts, &said), "{said}");
        let answer = bench.page.setup.as_ref().and_then(|w| w.menu_said.clone());
        assert_eq!(answer.map(|(worked, _)| worked), Some(false), "the answer is marked as a failure");
        assert!(!shows(&texts, &tf("as_menue_da", &[("0", Arg::from(""))])), "no word of success");
        let file = desktop_entry::target_file(&desktop_entry::data_home(&host_in(&dir, &[])));
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_menu_is_offered_on_the_page_of_a_game_only_where_the_system_has_menus_and_no_entry_yet() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-menue-angebot");
        let mut bench = Bench::new(page);
        // A fresh home: offered exactly where the system has such menus.
        bench.page.open_setup();
        assert_eq!(bench.page.setup.as_ref().map(|w| w.menu_offer), Some(desktop_entry::available()));
        // The short setup tells the way back and nothing more: no offer, also when it is forced.
        bench.page.open_wizard(true);
        assert_eq!(bench.page.setup.as_ref().map(|w| w.menu_offer), Some(false));
        if let Some(w) = bench.page.setup.as_mut() {
            w.menu_offer = true;
            w.state.step = w.steps.len() - 1;
        }
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        assert!(!shows(&texts, &language::t("as_menue_frage")), "the short setup asks nothing");
        // Without a game the last page is the short farewell of 4.12: no offer.
        bench.page.open_setup();
        if let Some(w) = bench.page.setup.as_mut() {
            w.without_game = true;
            w.menu_offer = true;
            w.state.step = w.steps.len() - 1;
        }
        scroll_step(&mut bench, -400.0);
        let texts = bench.settle();
        assert!(!shows(&texts, &language::t("as_menue_frage")), "without a game there is nothing to put in a menu");
        // An entry that stands (pointing to a program that is there): not offered again.
        let host = host_in(&dir, &[]);
        make_in_the_throwaway_home(&host).expect("written");
        bench.page.open_setup();
        assert_eq!(bench.page.setup.as_ref().map(|w| w.menu_offer), Some(false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_menu_is_offered_in_the_full_setup_on_a_system_with_menus_while_no_entry_stands() {
        let mut offering = Vec::new();
        for short in [false, true] {
            for available in [false, true] {
                for present in [false, true] {
                    if menu_offered(short, available, present) {
                        offering.push((short, available, present));
                    }
                }
            }
        }
        assert_eq!(offering, vec![(false, true, false)], "only the full setup, with menus, without an entry");
    }

    #[test]
    fn the_setup_starts_again_from_the_page_backup_and_reset_and_from_the_page_about_the_program() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("einrichtung-wieder", "{\"einrichtung_fertig\": true}\n", &[]);
        let mut bench = Bench::new(page);
        // The head bar has a button of that name too: on the page of the backup the row's one is the second.
        let cases =
            [("hf_sichern", language::t("hf_einrichtung"), 1), ("hf_ueber", language::t("s_ub_einrichtung"), 0)];
        for (key, button, nth) in cases {
            bench.page.go_to(key);
            let texts = bench.settle();
            assert!(!bench.page.setup_open(), "{key}: closed before the click");
            if key == "hf_sichern" {
                assert!(shows(&texts, &language::t("s_si_einrichtung")), "the row has its name");
                // What it does stands on hover behind the symbol `info`, not as a line.
                assert!(!shows(&texts, &language::t("s_si_einrichtung_h")), "and says what it does on hover");
            }
            bench.click_nth(&button, nth);
            bench.settle();
            assert!(bench.page.setup_open(), "{key}: the assistant opens");
            let w = bench.page.setup.as_ref().expect("open");
            assert_eq!((w.state.step, w.steps[0]), (0, Step::Language), "{key}: from the first step");
            bench.page.setup = None;
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn leaving_a_repeated_setup_keeps_the_proof_that_the_program_is_set_up() {
        let _language = crate::language_lock();
        let (page, dir) = real_page("einrichtung-wieder-weg", "{\"einrichtung_fertig\": true}\n", &[]);
        let mut bench = Bench::new(page);
        bench.page.go_to("hf_sichern");
        bench.click_nth(&language::t("hf_einrichtung"), 1);
        assert!(bench.page.setup_open());
        bench.frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        bench.settle();
        assert_eq!(bench.page.take_setup_end(), Some(SetupEnd { finished: false, overview: false }));
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"einrichtung_fertig\": true"), "{file}");
        assert!(bench.page.is_set_up(), "still set up");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_choices_of_the_assistant_stand_in_the_settings_file_at_once_for_a_running_overlay_to_read() {
        let _language = crate::language_lock();
        let (page, dir) = page_with_game("einrichtung-wahl");
        let mut bench = Bench::new(page);
        bench.page.open_setup();
        // The language, step one: in the file with the click.
        bench.click("English");
        bench.settle();
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"sprache\": \"en\""), "{file}");
        bench.click("Deutsch");
        bench.settle();
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"sprache\": \"de\""), "{file}");
        // The overlay: the mode and the opacity, and the way the overlay is told is the one of every setting.
        if let Some(w) = bench.page.setup.as_mut() {
            w.state.step = w.steps.iter().position(|s| *s == Step::Overlay).expect("the step");
        }
        bench.page.overlay_opacity = 61;
        bench.settle();
        bench.click(&language::t("s_ov_popup"));
        bench.settle();
        let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
        assert!(file.contains("\"overlay_modus\": \"popup\""), "{file}");
        assert!(file.contains("\"deckkraft_prozent\": 61"), "{file}");
        language::set_thread_language(language::Choice::De);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn every_step_fits_the_window_in_both_languages() {
        use language::Choice;
        // The visible part of the lines: what a field clips (a long folder name in its box) is not seen.
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
        let mut findings = Vec::new();
        let (mut page, dir) = page_with_game("einrichtung-mass");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        // The lines of the window, a few frames so that it has settled.
        // `scroll`: the step is scrolled to its end first (the wheel turns over the middle of the window).
        let draw = |page: &mut SamplePage, width: f32, height: f32, scroll: bool| -> Vec<(String, egui::Rect)> {
            let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height));
            let mut lines = Vec::new();
            // A scrolled view has a context of its own: the scroll position and the pointer stay out of the
            // views that follow.
            let own = egui::Context::default();
            let ctx = if scroll {
                crate::install(&own);
                own.all_styles_mut(|style| style.animation_time = 0.0);
                &own
            } else {
                &ctx
            };
            for frame in 0..if scroll { 10 } else { 4 } {
                let mut events = Vec::new();
                if scroll && frame < 6 {
                    events.push(egui::Event::PointerMoved(Pos2::new(width * 0.6, height * 0.5)));
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -2000.0),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                    });
                }
                let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
                let mut out = ctx.run_ui(input, |ui| {
                    page.show(ui);
                });
                out.textures_delta.clear();
                lines.clear();
                out.shapes.iter().for_each(|c| rows(&c.shape, c.clip_rect, &mut lines));
            }
            lines
        };
        for lang in [Choice::De, Choice::En] {
            // The smallest window of the program (880 x 380) is the one where the steps have to scroll.
            // (also two heights between: where the picture stops standing above the area that scrolls)
            // The smallest window also in the scheme with the capital letters (KRT): the widest buttons and titles.
            for (width, height, scheme_id) in [
                (900.0_f32, 900.0_f32, "original"),
                (1300.0, 900.0, "original"),
                (900.0, 640.0, "original"),
                (900.0, 500.0, "original"),
                (880.0, 380.0, "original"),
                (880.0, 380.0, "krt"),
            ] {
                // The scheme of this test thread only: the scheme of the program is one for all tests side by side.
                theme::set_thread_current(scheme_id);
                language::set_thread_language(lang);
                // What the window shows without the assistant stands behind it: only the rest is the assistant's.
                page.setup = None;
                let behind = draw(&mut page, width, height, false);
                // The last page twice: with the steps still open (the yellow line) and with all confirmed.
                // The full assistant and the short one, step by step.
                let mut plan: Vec<(bool, usize, bool, usize)> = Vec::new();
                for short in [false, true] {
                    page.open_wizard(short);
                    let count = page.setup.as_ref().map_or(0, |w| w.steps.len());
                    plan.extend((0..count).map(|s| (short, s, false, count)).chain([(short, count - 1, true, count)]));
                }
                for (short, step, all_done, count) in plan {
                    page.open_wizard(short);
                    if let Some(w) = page.setup.as_mut() {
                        w.state.step = step;
                        if all_done {
                            w.state.done = vec![true; count];
                        }
                        // The offer of the application menu with a long path in its answer, and with its fault.
                        w.menu_offer = true;
                        let file = "/home/someone/.local/share/applications/a-folder-with-a-long-name/versekit.desktop";
                        w.menu_said = Some(if all_done {
                            (true, tf("as_menue_da", &[("0", Arg::from(file))]))
                        } else {
                            (false, tf("as_menue_nein", &[("0", Arg::from(file))]))
                        });
                    }
                    let lines = draw(&mut page, width, height, false);
                    let mine: Vec<&(String, egui::Rect)> = lines
                        .iter()
                        .filter(|(text, rect)| {
                            !behind.iter().any(|(t, r)| t == text && (r.min - rect.min).length() < 1.0)
                        })
                        .collect();
                    let at =
                        format!("{lang:?} {scheme_id} {width}x{height} Kurz {short} Schritt {step} fertig {all_done}");
                    if mine.is_empty() {
                        findings.push(format!("{at}: der Assistent zeichnet nichts"));
                    }
                    // A picture stands above the area that scrolls where the window has room for it and the
                    // controls, and scrolls with the step in the smallest window.
                    if let Some(w) = page.setup.as_ref() {
                        let current = w.steps[w.state.step];
                        if matches!(current, Step::Overlay | Step::Texts) {
                            let room = (height - STEP_FRAME).max(STEP_MIN);
                            let wanted = room - picture_height(current) >= STEP_MIN_BESIDE_PICTURE;
                            if w.picture_pinned != wanted {
                                findings.push(format!("{at}: Bild oben fest {} statt {wanted}", w.picture_pinned));
                            }
                        }
                    }
                    for (text, rect) in &mine {
                        if rect.right() > width + 0.5 || rect.bottom() > height + 0.5 {
                            findings.push(format!("{at}: {text:?} steht über den Rand"));
                        }
                    }
                    // The row of buttons stays in sight, whole: a line cut by the edge is shorter than its height.
                    let back = language::t("zurueck");
                    match mine.iter().find(|(text, _)| *text == back) {
                        Some((_, rect)) if rect.height() > 10.0 && rect.bottom() < height - 2.0 => {}
                        other => findings.push(format!("{at}: die Knopfzeile ist nicht ganz zu sehen ({other:?})")),
                    }
                    'pairs: for (i, (a, ra)) in mine.iter().enumerate() {
                        for (b, rb) in &mine[i + 1..] {
                            let both = ra.intersect(*rb);
                            if both.width() > 1.0 && both.height() > 1.0 {
                                findings.push(format!("{at}: {a:?} {ra:?} liegt auf {b:?} {rb:?}"));
                                break 'pairs;
                            }
                        }
                    }
                    // The offer of the application menu stands at the end of the last page of the full assistant:
                    // measured there, with the step scrolled to its end, with the answer of the click (the long path
                    // when all is confirmed, the fault when not).
                    if !short && step + 1 == count {
                        let lines = draw(&mut page, width, height, true);
                        let mine: Vec<&(String, egui::Rect)> = lines
                            .iter()
                            .filter(|(text, rect)| {
                                !behind.iter().any(|(t, r)| t == text && (r.min - rect.min).length() < 1.0)
                            })
                            .collect();
                        let said = page.setup.as_ref().and_then(|w| w.menu_said.clone()).map(|(_, said)| said);
                        let said = said.unwrap_or_default();
                        let starts = |part: &str| {
                            // The start of the line is enough: a long path wraps after the first word.
                            let part: String = part.chars().take(5).collect();
                            mine.iter().find(|(text, _)| text.starts_with(&part))
                        };
                        for (what, found) in [
                            ("die Frage", starts(&language::t("as_menue_frage"))),
                            ("der Knopf", mine.iter().find(|(text, _)| *text == language::t("as_menue_knopf"))),
                            ("die Antwort", starts(&said)),
                        ] {
                            match found {
                                // A line cut by the edge of the area is shorter than its height.
                                Some((_, rect))
                                    if rect.height() > 10.0
                                        && rect.right() <= width + 0.5
                                        && rect.bottom() <= height + 0.5 => {}
                                other => findings.push(format!(
                                    "{at}: {what} der Startmenü-Karte fehlt oder ragt hinaus ({other:?})"
                                )),
                            }
                        }
                        // The three lines of the card do not lie on one another (the page behind is another matter).
                        let card: Vec<egui::Rect> = [
                            starts(&language::t("as_menue_frage")),
                            mine.iter().find(|(text, _)| *text == language::t("as_menue_knopf")),
                            starts(&said),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|(_, rect)| *rect)
                        .collect();
                        for (i, a) in card.iter().enumerate() {
                            for b in &card[i + 1..] {
                                let both = a.intersect(*b);
                                if both.width() > 1.0 && both.height() > 1.0 {
                                    findings.push(format!("{at}: die Zeilen der Startmenü-Karte liegen aufeinander"));
                                }
                            }
                        }
                    }
                }
            }
        }
        language::set_thread_language(Choice::De);
        theme::set_thread_current("original");
        let _ = std::fs::remove_dir_all(dir);
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }
}
