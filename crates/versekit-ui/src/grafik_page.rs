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

//! The page "Grafik" (`_graphics`, `_quality_rows`, `_user_cfg_rows`, `_machine_card` of 4.12): the graphics
//! switches of Star Citizen without going into the game.
//!
//! **Changes gather** in `pending` until the button "Ins Spiel schreiben" takes them over. While the game
//! runs nothing is written (it overwrites the files when its settings are left): the changes are kept in the
//! settings and the overlay writes them once the game is quiet (`graphics_apply::apply_queued`). When nobody
//! can tell whether the game runs, nothing is written and nothing kept, and the page says so.
//!
//! The files are `attributes.xml` (the switches and the quality rows), `GraphicsSettings.json` (the renderer)
//! and the `user.cfg` of the game folder (three commands). All writing goes through `graphics_apply`, which
//! makes a copy first (`game_files`). What the files say is read again when one of them has changed.
//!
//! The box "Dein Rechner laut Spiel" is read from the Game.log in a thread of its own (the kept logs may take
//! a while).

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::SystemTime;

use egui::{RichText, Ui, WidgetInfo};
use versekit_core::formats;
use versekit_core::game_files::WriteError;
use versekit_core::game_start::{self, RunState};
use versekit_core::graphics::{self, Kind};
use versekit_core::graphics_apply::{self, Changes};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::machine_info::{self, Info};
use versekit_core::paths::Paths;
use versekit_core::renderer;
use versekit_core::theme::Scheme;
use versekit_core::usercfg;

use crate::color;
use crate::settings::{self, Said};
use crate::widgets::{self, ButtonKind};

/// The commands of the `user.cfg` the page offers, in the order it shows them.
const MAX_FPS: &str = "sys_MaxFPS";
const IDLE_FPS: &str = "sys_MaxIdleFPS";
const DISPLAY_INFO: &str = "r_DisplayInfo";

/// How often the files are looked at for a change, in seconds.
const LOOK_SECONDS: f64 = 1.0;
/// So many rows a tip names, the others are counted.
const TIP_NAMES: usize = 3;
/// How often the page tries to write what is kept for the end of the game, in seconds.
const QUEUE_SECONDS: f64 = 5.0;

/// How a file looked when it was read: time of change and size.
type Stamp = Option<(SystemTime, u64)>;

fn stamp_of(path: &str) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// What the files say.
struct Files {
    /// The nine switches; `None` where there is no `attributes.xml`.
    switches: Option<Vec<(&'static str, f64)>>,
    levels: Vec<(&'static str, i64)>,
    quality: Vec<(&'static str, i64)>,
    renderer: Option<i64>,
    /// The folder of the game, where the `user.cfg` lies.
    folder: Option<String>,
    max_fps: String,
    idle_fps: String,
    info: String,
}

/// The files the page depends on, for the look at their change.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct Stamps {
    folder: Option<String>,
    files: [(String, Stamp); 3],
}

fn locations(paths: &Paths) -> (Option<String>, [String; 3]) {
    let folder = paths.game_folder();
    let attributes = graphics::attribute_file(paths, None).unwrap_or_default();
    let json = renderer::file(paths).unwrap_or_default();
    let cfg = folder.as_deref().map(|f| paths.host.flavor.join(f, usercfg::FILE)).unwrap_or_default();
    (folder, [attributes, json, cfg])
}

fn stamps(paths: &Paths) -> Stamps {
    let (folder, files) = locations(paths);
    Stamps { folder, files: files.map(|path| (path.clone(), stamp_of(&path))) }
}

fn read_files(paths: &Paths) -> Files {
    let (folder, [attributes, ..]) = locations(paths);
    let text = (!attributes.is_empty())
        .then(|| std::fs::read(&attributes).ok())
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let value = |key: &str| folder.as_deref().and_then(|f| usercfg::value(f, key)).unwrap_or_default();
    Files {
        switches: text.as_deref().map(graphics::read_text),
        levels: text.as_deref().map(graphics::levels_text).unwrap_or_default(),
        quality: text.as_deref().map(graphics::quality_text).unwrap_or_default(),
        renderer: renderer::read(paths, None),
        max_fps: value(MAX_FPS),
        idle_fps: value(IDLE_FPS),
        info: value(DISPLAY_INFO),
        folder,
    }
}

/// The box with the computer of the player.
enum Machine {
    /// Not asked yet.
    Unasked,
    /// The log is being read.
    Reading(Receiver<Option<Info>>),
    /// What the log said.
    Known(Box<Option<Info>>),
}

/// Reads the box in a thread of its own.
fn read_machine(paths: &Paths) -> Receiver<Option<Info>> {
    let (sender, receiver) = mpsc::channel();
    let copy = paths.working_copy();
    std::thread::spawn(move || {
        let _ = sender.send(machine_info::read(&copy, None));
    });
    receiver
}

/// What the page keeps.
pub struct GrafikState {
    /// What the player wants and the files do not say yet.
    pending: Changes,
    /// The changes are kept in the settings for the end of the game.
    queued: bool,
    files: Option<Files>,
    seen: Stamps,
    looked_at: f64,
    machine: Machine,
    /// The box read again while the known one stays on the page.
    machine_again: Option<Receiver<Option<Info>>>,
    /// When the page was drawn last: a longer gap means it was left and is shown again.
    drawn_at: f64,
    /// The typed text of the two frame-rate fields.
    fps: [String; 2],
    /// The typed text has to be taken from the files / `pending` again.
    fps_stale: bool,
    notice: Option<Said>,
    /// What waited in the settings when the page was first drawn has been taken up.
    started: bool,
    /// "Verwerfen" was clicked and waits for the yes.
    discard_ask: bool,
    /// When the page last tried to write what is kept.
    queue_tried: f64,
    /// The width the cards of the scrolling page have (0: not known yet). The scroll area takes the room of its
    /// bar from the page below the head when the page scrolls, so the card of the head takes this width instead
    /// of the one it is offered, and the corner marks of all cards stand on one line.
    body_width: f32,
}

impl Default for GrafikState {
    fn default() -> GrafikState {
        GrafikState {
            body_width: 0.0,
            started: false,
            discard_ask: false,
            queue_tried: f64::NEG_INFINITY,
            pending: Changes::default(),
            queued: false,
            files: None,
            seen: Stamps::default(),
            looked_at: f64::NEG_INFINITY,
            machine: Machine::Unasked,
            machine_again: None,
            drawn_at: f64::NEG_INFINITY,
            fps: [String::new(), String::new()],
            fps_stale: true,
            notice: None,
        }
    }
}

impl GrafikState {
    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Reads the files again at the next look (the settings changed, the folder of the game changed).
    pub fn invalidate(&mut self) {
        self.files = None;
        self.fps_stale = true;
    }

    /// How many changes wait.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The first time the page is drawn: what a stay of the program before kept for the end of the game (the
    /// player wrote it while the game ran) is on the page again, as what is wanted. Without that the bar would
    /// say that all is written, and the next click would put only the new changes in its place.
    fn start(&mut self, paths: &Paths) {
        if self.started {
            return;
        }
        let Some(kept) = graphics_apply::queued_checked(paths) else { return };
        self.started = true;
        if !kept.is_empty() {
            self.pending = kept;
            self.queued = true;
        }
    }

    /// What was kept: the game ended and the kept changes are written (by the overlay, or by this page when the
    /// overlay is not there), so they are the state of the files now. A settings file that cannot be read for
    /// a moment is not "written".
    fn follow_queue(&mut self, ui: &Ui, paths: &Paths) {
        if !self.queued {
            return;
        }
        let now = ui.input(|i| i.time);
        // Written by the page itself when the game is quiet and nobody else did: looked at every few seconds.
        if now - self.queue_tried >= QUEUE_SECONDS {
            self.queue_tried = now;
            match graphics_apply::apply_queued(paths, None) {
                graphics_apply::Queued::Done(Ok(backups)) => {
                    let named = if backups.is_empty() { "-".to_string() } else { backups.join(", ") };
                    self.notice = Some(Said::note(tf("s_gr_nach_spiel_ok", &[("0", Arg::from(named.as_str()))])));
                }
                graphics_apply::Queued::Done(Err(error)) => {
                    let reason = error.reason();
                    self.notice = Some(Said::fault(tf("s_gr_nach_spiel_f", &[("0", Arg::from(reason.as_str()))])));
                }
                graphics_apply::Queued::Nothing | graphics_apply::Queued::NotYet => {}
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(QUEUE_SECONDS));
        }
        if graphics_apply::queued_checked(paths).is_some_and(|kept| kept.is_empty()) {
            self.pending = Changes::default();
            self.queued = false;
            self.invalidate();
        }
    }

    /// The wishes changed: what is kept for the end of the game follows, or the player's last changes would not
    /// be in it when the overlay writes.
    fn changed(&mut self, paths: &Paths) {
        if self.queued && !graphics_apply::set_queued(paths, &self.pending) {
            self.notice = Some(Said::fault(t("s_gr_f_vormerken")));
        }
    }

    /// Drops everything that waits, kept or not.
    fn discard(&mut self, paths: &Paths) {
        self.discard_ask = false;
        if self.queued && !graphics_apply::set_queued(paths, &Changes::default()) {
            self.notice = Some(Said::fault(t("s_gr_f_vormerken")));
            return;
        }
        self.pending = Changes::default();
        self.queued = false;
        self.invalidate();
        self.notice = Some(Said::note(t("s_gr_verworfen")));
    }

    /// Looks whether a file changed (at most once a second) and reads again if so.
    fn refresh(&mut self, ui: &Ui, paths: &Paths) {
        let now = ui.input(|i| i.time);
        if self.files.is_some() && now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        let current = stamps(paths);
        if self.files.is_none() || current != self.seen {
            self.files = Some(read_files(paths));
            self.seen = current;
            self.fps_stale = true;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(LOOK_SECONDS));
    }

    /// Asks for the box once each time the page is shown (the Game.log grows while the game runs, so it is not
    /// read again as long as the page stays).
    fn ask_machine(&mut self, paths: &Paths) {
        if !matches!(self.machine, Machine::Unasked) {
            return;
        }
        self.machine = Machine::Reading(read_machine(paths));
    }

    /// Reads the box again while the known one stays on the page, so nothing below it moves meanwhile.
    fn ask_machine_again(&mut self, paths: &Paths) {
        if matches!(self.machine, Machine::Known(_)) && self.machine_again.is_none() {
            self.machine_again = Some(read_machine(paths));
        }
    }

    fn poll_machine(&mut self, ui: &Ui) {
        if let Machine::Reading(receiver) = &self.machine {
            match receiver.try_recv() {
                Ok(info) => self.machine = Machine::Known(Box::new(info)),
                Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                Err(TryRecvError::Disconnected) => self.machine = Machine::Known(Box::new(None)),
            }
        }
        if let Some(receiver) = &self.machine_again {
            match receiver.try_recv() {
                Ok(info) => {
                    self.machine = Machine::Known(Box::new(info));
                    self.machine_again = None;
                }
                Err(TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                Err(TryRecvError::Disconnected) => self.machine_again = None,
            }
        }
    }

    /// The typed text of the frame-rate fields: what is wanted, else what the file says.
    fn sync_fps(&mut self) {
        if !self.fps_stale {
            return;
        }
        self.fps_stale = false;
        let Some(files) = &self.files else { return };
        let pick = |key: &str, file: &str| match self.pending.cfg_value(key) {
            Some(wanted) => wanted.unwrap_or_default().to_string(),
            None => file.to_string(),
        };
        self.fps = [pick(MAX_FPS, &files.max_fps), pick(IDLE_FPS, &files.idle_fps)];
    }

    /// The button: write, or keep for the end of the game.
    fn store(&mut self, paths: &Paths) {
        if self.pending.is_empty() {
            self.notice = Some(Said::note(t("s_gr_nichts")));
            return;
        }
        // A frame rate with something but digits in it is not what stands in the wishes: no writing with it.
        if self.fps.iter().any(|text| !text.trim().chars().all(|c| c.is_ascii_digit())) {
            self.notice = Some(Said::fault(t("s_gr_fps_ungueltig")));
            return;
        }
        match game_start::run_state(paths) {
            RunState::Running => {
                // The game overwrites the files itself: kept, and written once it is over.
                if graphics_apply::set_queued(paths, &self.pending) {
                    self.queued = true;
                    let n = i64::try_from(self.pending.len()).unwrap_or(i64::MAX);
                    let key = if n == 1 { "s_gr_vorgemerkt_eins" } else { "s_gr_vorgemerkt" };
                    self.notice = Some(Said::note(tf(key, &[("0", Arg::from(n))])));
                } else {
                    self.notice = Some(Said::fault(t("s_gr_f_vormerken")));
                }
                return;
            }
            RunState::Unknown => {
                self.notice = Some(Said::fault(WriteError::GameUnknown.reason()));
                return;
            }
            RunState::Idle => {}
        }
        match graphics_apply::apply_all(paths, None, &self.pending) {
            Ok(backups) => {
                let cleared = graphics_apply::set_queued(paths, &Changes::default());
                self.queued = false;
                self.pending = Changes::default();
                self.invalidate();
                self.notice = Some(if !cleared {
                    // Written, but an old note stays: it would be written once more (the same, harmlessly).
                    Said::fault(t("s_gr_f_vormerken"))
                } else if backups.is_empty() {
                    Said::note(t("s_gr_nichts"))
                } else {
                    Said::note(tf("s_gr_ok", &[("0", Arg::from(backups.join(", ").as_str()))]))
                });
            }
            Err(error) => self.notice = Some(Said::fault(error.reason())),
        }
    }

    /// The level of a quality row to show: what is wanted, else what the file says; `None` is "automatic".
    fn level(&self, row: &str) -> Option<i64> {
        match self.pending.attribute(row) {
            Some(wanted) => wanted.map(|v| v.round() as i64),
            None => self.files.as_ref()?.levels.iter().find(|(r, _)| *r == row).map(|(_, level)| *level),
        }
    }

    /// The levels of the quality rows as they will stand once what is wanted is written (the tips read these, not
    /// the file only): a row set to automatic has no level of its own.
    fn quality_wanted(&self) -> Vec<(&'static str, i64)> {
        let Some(files) = &self.files else { return Vec::new() };
        files
            .quality
            .iter()
            .filter_map(|(row, level)| match self.pending.attribute(row) {
                Some(Some(wanted)) => Some((*row, wanted.round() as i64)),
                Some(None) => None,
                None => Some((*row, *level)),
            })
            .chain(graphics::QUALITY_ROWS.iter().filter_map(|(row, _)| {
                let in_file = files.quality.iter().any(|(r, _)| r == row);
                match self.pending.attribute(row) {
                    Some(Some(wanted)) if !in_file => Some((*row, wanted.round() as i64)),
                    _ => None,
                }
            }))
            .collect()
    }

    /// Is upscaling on "Native AA" as it will stand?
    fn upscaling_native(&self) -> bool {
        self.files.as_ref().is_some_and(|f| f.switches.is_some()) && self.switch_value("Upscaling", 0.0) == 0.0
    }

    /// The value of a switch to show.
    fn switch_value(&self, name: &str, default: f64) -> f64 {
        if let Some(Some(wanted)) = self.pending.attribute(name) {
            return wanted;
        }
        self.files
            .as_ref()
            .and_then(|f| f.switches.as_ref())
            .and_then(|s| s.iter().find(|(n, _)| *n == name).map(|(_, v)| *v))
            .unwrap_or(default)
    }

    fn renderer_value(&self) -> Option<i64> {
        match self.pending.renderer {
            Some(wanted) => Some(wanted),
            None => self.files.as_ref()?.renderer,
        }
    }

    fn info_value(&self) -> String {
        let value = match self.pending.cfg_value(DISPLAY_INFO) {
            Some(wanted) => wanted.unwrap_or_default().to_string(),
            None => self.files.as_ref().map(|f| f.info.clone()).unwrap_or_default(),
        };
        let value = if value.is_empty() { "0".to_string() } else { value };
        if (0..=4).any(|n| n.to_string() == value) { value } else { "0".to_string() }
    }
}

/// A drop-down for the row: the options as (value, text); gives the new value when the player picked one.
fn pick<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: (&str, &str),
    name: &str,
    options: &[(T, String)],
    current: T,
) -> Option<T> {
    let shown = options.iter().find(|(v, _)| *v == current).map(|(_, text)| text.clone()).unwrap_or_default();
    let mut chosen = current;
    settings::choice_box(ui, ("grafik", salt.0, salt.1), name, &shown, |list| {
        for (value, text) in options {
            list.selectable_value(&mut chosen, *value, text.as_str());
        }
    });
    (chosen != current).then_some(chosen)
}

/// The levels one can pick for a row: automatic, then 1 up to the highest the game knows for it.
fn level_options(top: i64) -> Vec<(Option<i64>, String)> {
    let mut options = vec![(None, t("s_gr_q_auto"))];
    options.extend((1..=top).map(|n| (Some(n), t(&format!("s_gr_stufe_{n}")))));
    options
}

/// The fixed part above the scrolling page (as in 4.12: first what stays, then the area that scrolls): the lead
/// and the bar with what waits and the button, so the button is always in sight, however far the page is
/// scrolled.
pub fn show_head(ui: &mut Ui, scheme: &Scheme, state: &mut GrafikState, paths: &Paths) {
    state.start(paths);
    state.follow_queue(ui, paths);
    let body_width = state.body_width;
    ui.scope(|ui| {
        // The width of the cards below (known after the first frame); else a little room, like the page below:
        // rounding must not push a full-width row out.
        let room = ui.available_width() - crate::layout::SAFETY;
        ui.set_max_width(if body_width > 0.0 { body_width.min(room) } else { room });
        ui.label(RichText::new(t("s_gr_lead")).small().color(color(scheme.sub)));
        ui.add_space(8.0);
        let waiting = i64::try_from(state.pending.len()).unwrap_or(0);
        let one = waiting == 1;
        let text = match (state.queued, waiting) {
            _ if state.discard_ask => t("s_gr_verwerfen_frage"),
            (true, n) if n > 0 => {
                tf(if one { "s_gr_vorgemerkt_eins" } else { "s_gr_vorgemerkt" }, &[("0", Arg::from(n))])
            }
            (false, n) if n > 0 => tf(if one { "s_gr_offen_eins" } else { "s_gr_offen" }, &[("0", Arg::from(n))]),
            _ => t("s_gr_alles_drin"),
        };
        let (write_word, discard_word) = (t("s_gr_schreiben"), t("s_gr_verwerfen"));
        let (yes_word, no_word) = (t("s_mlg_bestaetigen"), t("abbrechen"));
        let (mut write, mut discard, mut yes, mut no) = (false, false, false, false);
        widgets::card(ui, scheme, |ui| {
            let hint = if state.discard_ask { String::new() } else { t("s_gr_schreiben_h") };
            settings::setting_row(ui, scheme, &text, &hint, false, |row| {
                if state.discard_ask {
                    // The question stands in the row, no window of its own.
                    no = row.add(widgets::Button::new(&no_word, ButtonKind::Normal, scheme)).clicked();
                    yes = row.add(widgets::Button::new(&yes_word, ButtonKind::Danger, scheme)).clicked();
                } else {
                    if waiting > 0 {
                        discard = row.add(widgets::Button::new(&discard_word, ButtonKind::Normal, scheme)).clicked();
                    }
                    write = row.add(widgets::Button::new(&write_word, ButtonKind::Strong, scheme)).clicked();
                }
            });
        });
        if write {
            state.store(paths);
        }
        if discard {
            state.discard_ask = true;
        }
        if no {
            state.discard_ask = false;
        }
        if yes {
            state.discard(paths);
        }
        ui.add_space(8.0);
    });
}

/// Draws the page that scrolls (below [`show_head`]).
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut GrafikState, paths: &Paths) {
    // Shown again after a stay elsewhere: the files and the box are read anew.
    let now = ui.input(|i| i.time);
    if now - state.drawn_at > 2.0 {
        state.invalidate();
        state.ask_machine_again(paths);
    }
    state.drawn_at = now;
    state.start(paths);
    state.follow_queue(ui, paths);
    state.refresh(ui, paths);
    state.ask_machine(paths);
    state.poll_machine(ui);
    state.sync_fps();

    // The head above takes the width of this card for its own (it is drawn first, so the next frame has it).
    let width = machine_card(ui, scheme, state);
    if (state.body_width - width).abs() > 0.5 {
        state.body_width = width;
        ui.ctx().request_repaint();
    }
    ui.add_space(8.0);

    let Some(files) = &state.files else { return };
    if files.switches.is_none() {
        ui.label(RichText::new(t("s_gr_keine_datei")).color(color(scheme.yellow)));
        return;
    }
    let has_renderer = state.renderer_value().is_some();
    let has_cfg = files.folder.is_some();
    let mut titles = vec![t("hf_grafik"), t("s_gr_q_titel")];
    let mut sizes = vec![usize::from(has_renderer) + graphics::SETTINGS.len(), 2 + graphics::EDIT_ROWS.len()];
    if has_cfg {
        titles.push(t("s_gr_cfg_titel"));
        sizes.push(1 + 3);
    }
    let groups: Vec<(&str, usize)> = titles.iter().map(String::as_str).zip(sizes).collect();
    let mut changed = false;
    let state_ref = &mut *state;
    settings::settings_page(ui, scheme, "grafik", &groups, &mut |ui, g, r| match g {
        0 => changed |= switch_row(ui, scheme, state_ref, has_renderer, r),
        1 => changed |= quality_row(ui, scheme, state_ref, r),
        _ => changed |= cfg_row(ui, scheme, state_ref, r),
    });
    if changed {
        state.changed(paths);
        ui.ctx().request_repaint();
    }
}

fn percent(value: f64) -> i64 {
    (value * 100.0).round() as i64
}

/// One row of the first card: the renderer (when the file is there), then the nine switches.
fn switch_row(ui: &mut Ui, scheme: &Scheme, state: &mut GrafikState, has_renderer: bool, r: usize) -> bool {
    if has_renderer && r == 0 {
        let name = t("s_gr_renderer");
        let options: Vec<(i64, String)> = renderer::CHOICES.iter().map(|(v, key)| (*v, t(key))).collect();
        let current = state.renderer_value().unwrap_or(1);
        let mut picked = None;
        settings::setting_row(ui, scheme, &name, &t("s_gr_renderer_h"), false, |row| {
            picked = pick(row, ("renderer", ""), &name, &options, current);
        });
        if let Some(value) = picked {
            state.pending.renderer = Some(value);
        }
        return picked.is_some();
    }
    let setting = graphics::SETTINGS[r - usize::from(has_renderer)];
    let (caption, help) = captions(setting.name);
    let (name, hint) = (t(caption), if help.is_empty() { String::new() } else { t(help) });
    let value = state.switch_value(setting.name, setting.default);
    let mut wanted = None;
    settings::setting_row(ui, scheme, &name, &hint, false, |row| match setting.kind {
        Kind::Choice => {
            let options: Vec<(i64, String)> = setting.choices.iter().map(|(v, key)| (*v, t(key))).collect();
            wanted = pick(row, (setting.name, ""), &name, &options, value.round() as i64).map(|v| v as f64);
        }
        Kind::Switch => {
            let mut on = value >= 0.5;
            let response = row.add(widgets::Toggle::new(&mut on, scheme));
            let enabled = response.enabled();
            response.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, enabled, on, &name));
            if response.changed() {
                wanted = Some(if on { 1.0 } else { 0.0 });
            }
        }
        Kind::Slider => {
            let mut pct = percent(value);
            let response = settings::slider_with_value(row, scheme, &name, &mut pct, (0, 100), false, |n| {
                formats::percent(n as f64)
            });
            let enabled = response.enabled();
            response.widget_info(|| WidgetInfo::slider(enabled, pct as f64, &name));
            if response.changed() {
                wanted = Some(pct as f64 / 100.0);
            }
        }
    });
    if let Some(value) = wanted {
        state.pending.set_attribute(setting.name, Some(value));
    }
    wanted.is_some()
}

/// The texts of a switch: its name and its hint (empty for none).
fn captions(name: &str) -> (&'static str, &'static str) {
    match name {
        "WindowMode" => ("s_gr_fenstermodus", ""),
        "VSync" => ("s_gr_vsync", "s_gr_vsync_h"),
        "Upscaling" => ("s_gr_upscaling", "s_gr_upscaling_h"),
        "UpscalingTechnique" => ("s_gr_technik", ""),
        "UpscalingModel" => ("s_gr_modell", "s_gr_modell_h"),
        "MotionBlur" => ("s_gr_unschaerfe", ""),
        "FilmGrain" => ("s_gr_koernung", ""),
        "Sharpening" => ("s_gr_schaerfen", ""),
        _ => ("s_gr_aberration", "s_gr_aberration_h"),
    }
}

/// One row of the second card: the lead, the overall level, then the 18 rows.
fn quality_row(ui: &mut Ui, scheme: &Scheme, state: &mut GrafikState, r: usize) -> bool {
    if r == 0 {
        ui.label(RichText::new(t("s_gr_q_lead")).small().color(color(scheme.sub)));
        ui.add_space(6.0);
        return false;
    }
    if r == 1 {
        let name = t("s_gr_q_gesamt");
        let options = level_options(5);
        let current = state.level(graphics::OVERALL);
        let mut picked = None;
        settings::setting_row(ui, scheme, &name, &t("s_gr_q_gesamt_h"), false, |row| {
            picked = pick(row, ("gesamt", ""), &name, &options, current);
        });
        let Some(level) = picked else { return false };
        match level {
            None => state.pending.set_attribute(graphics::OVERALL, None),
            Some(level) => {
                // As the game does it when a preset is chosen: every row gets the level of the preset.
                for (row, wanted) in graphics::preset_rows(level) {
                    state.pending.set_attribute(row, Some(wanted as f64));
                }
                state.pending.set_attribute(graphics::OVERALL, Some(level as f64));
            }
        }
        return true;
    }
    let (row_name, key) = graphics::EDIT_ROWS[r - 2];
    let name = t(key);
    let options = level_options(graphics::top_level(row_name).unwrap_or(4));
    let current = state.level(row_name);
    let mut picked = None;
    settings::setting_row(ui, scheme, &name, "", false, |row| {
        picked = pick(row, (row_name, ""), &name, &options, current);
    });
    let Some(level) = picked else { return false };
    state.pending.set_attribute(row_name, level.map(|n| n as f64));
    true
}

/// One row of the third card: the lead, the two frame rates, the display of performance.
fn cfg_row(ui: &mut Ui, scheme: &Scheme, state: &mut GrafikState, r: usize) -> bool {
    match r {
        0 => {
            ui.label(RichText::new(t("s_gr_cfg_lead")).small().color(color(scheme.sub)));
            ui.add_space(6.0);
            false
        }
        1 | 2 => {
            // The hints differ: the field takes its id from its hint.
            let (key, caption, help, slot, example) = if r == 1 {
                (MAX_FPS, "s_gr_maxfps", "s_gr_maxfps_h", 0, "s_gr_fps_platz")
            } else {
                (IDLE_FPS, "s_gr_idlefps", "s_gr_idlefps_h", 1, "s_gr_fps_platz_hinten")
            };
            let (name, hint) = (t(caption), t(help));
            let mut typed = false;
            let field = &mut state.fps[slot];
            settings::setting_row(ui, scheme, &name, &hint, false, |row| {
                let placeholder = t(example);
                let response = widgets::clearable_field(row, field, &placeholder, scheme, 110.0);
                let (enabled, shown) = (response.enabled(), field.clone());
                response.widget_info(|| {
                    let mut info = WidgetInfo::labeled(egui::Role::TextInput, enabled, &name);
                    info.current_text_value = Some(shown.clone());
                    info
                });
                typed = response.changed();
            });
            let text = state.fps[slot].trim().to_string();
            let digits = text.chars().all(|c| c.is_ascii_digit());
            if typed && digits {
                state.pending.set_cfg(key, (!text.is_empty()).then_some(text));
            }
            // Anything but digits stays in the field and is not taken over: the field says so at once (what is
            // wanted is the number that was typed before).
            if !digits {
                let red = color(scheme.red);
                ui.label(RichText::new(t("s_gr_fps_ungueltig")).small().color(red));
                ui.add_space(4.0);
            }
            typed && digits
        }
        _ => {
            let name = t("s_gr_info");
            let options: Vec<(String, String)> =
                (0..=4).map(|n| (n.to_string(), t(&format!("s_gr_info_{n}")))).collect();
            let current = state.info_value();
            let mut picked = None;
            settings::setting_row(ui, scheme, &name, &t("s_gr_info_h"), false, |row| {
                picked = pick_text(row, &name, &options, &current);
            });
            let Some(value) = picked else { return false };
            state.pending.set_cfg(DISPLAY_INFO, (value != "0").then_some(value));
            true
        }
    }
}

/// [`pick`] for options whose value is a text.
fn pick_text(ui: &mut Ui, name: &str, options: &[(String, String)], current: &str) -> Option<String> {
    let shown = options.iter().find(|(v, _)| v == current).map(|(_, text)| text.clone()).unwrap_or_default();
    let mut chosen = current.to_string();
    settings::choice_box(ui, ("grafik", "info"), name, &shown, |list| {
        for (value, text) in options {
            list.selectable_value(&mut chosen, value.clone(), text.as_str());
        }
    });
    (chosen != current).then_some(chosen)
}

/// Gigabytes with as few decimals as needed (one at most), from megabytes.
fn gigabytes(megabytes: i64) -> String {
    let gb = (megabytes as f64 / 1024.0 * 10.0).round() / 10.0;
    formats::amount(language::current(), gb, "GB")
}

/// An index of the game with as few decimals as needed (two at most).
fn index_number(value: f64) -> String {
    formats::amount(language::current(), (value * 100.0).round() / 100.0, "")
}

/// The lines of the box: caption and value.
fn machine_rows(info: &Info) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(cpu) = &info.cpu {
        let threads =
            info.threads.filter(|n| *n != 0).map(|n| format!(" · {n} {}", t("s_gr_threads"))).unwrap_or_default();
        rows.push((t("s_gr_cpu"), format!("{cpu}{threads}")));
    }
    if let Some(gpu) = &info.gpu {
        let mut parts = vec![gpu.clone()];
        if let Some(vram) = info.vram_mb.filter(|n| *n != 0) {
            parts.push(gigabytes(vram));
        }
        if let Some(driver) = &info.driver {
            parts.push(format!("{} {driver}", t("s_gr_treiber")));
        }
        rows.push((t("s_gr_gpu"), parts.join(" · ")));
    }
    if let Some(ram) = info.ram_mb.filter(|n| *n != 0) {
        rows.push((t("s_gr_ram"), gigabytes(ram)));
    }
    if let Some((width, height)) = info.display {
        rows.push((t("s_gr_aufloesung"), format!("{width} × {height}")));
    }
    if let (Some(cpu), Some(gpu)) = (info.cpu_index, info.gpu_index) {
        rows.push((
            t("s_gr_index"),
            tf(
                "s_gr_index_wert",
                &[("0", Arg::from(index_number(cpu).as_str())), ("1", Arg::from(index_number(gpu).as_str()))],
            ),
        ));
    }
    rows
}

/// What is worth setting down first against the preset of the game for this computer: only from the template
/// of the game, no guessed frame rates. Each line with its color.
fn performance_hints(
    scheme: &Scheme,
    quality: &[(&'static str, i64)],
    native: bool,
    class: i64,
) -> Vec<(egui::Color32, String)> {
    let names: Vec<String> = (1..=machine_info::CLASSES).map(|n| t(&format!("s_gr_stufe_{n}"))).collect();
    let level_name = |n: i64| names.get((n - 1).max(0) as usize).cloned().unwrap_or_default();
    // So many rows are named; the rest is counted, or the line would run over 16 rows.
    let listing = |rows: &[(&str, i64, i64)]| {
        let named = rows
            .iter()
            .take(TIP_NAMES)
            .map(|(row, have, wanted)| {
                let key = graphics::QUALITY_ROWS.iter().find(|(r, _)| r == row).map_or("", |(_, key)| key);
                format!("{} ({} → {})", t(key), level_name(*have), level_name(*wanted))
            })
            .collect::<Vec<_>>()
            .join(", ");
        match rows.len().saturating_sub(TIP_NAMES) {
            0 => named,
            more => format!("{named} +{more}"),
        }
    };
    let mut lines = Vec::new();
    let (above, below) = graphics::compare(quality, class);
    if !above.is_empty() {
        lines.push((
            color(scheme.gold),
            tf(
                "s_gr_tipp_ueber",
                &[("0", Arg::from(level_name(class).as_str())), ("1", Arg::from(listing(&above).as_str()))],
            ),
        ));
    } else if !below.is_empty() {
        lines.push((
            color(scheme.accent),
            tf(
                "s_gr_tipp_unter",
                &[("0", Arg::from(level_name(class).as_str())), ("1", Arg::from(listing(&below).as_str()))],
            ),
        ));
    }
    if native {
        lines.push((color(scheme.gold), t("s_gr_tipp_native")));
    }
    if lines.is_empty() {
        lines.push((color(scheme.accent), tf("s_gr_tipp_passt", &[("0", Arg::from(level_name(class).as_str()))])));
    }
    lines
}

/// The box "Dein Rechner laut Spiel".
fn machine_card(ui: &mut Ui, scheme: &Scheme, state: &GrafikState) -> f32 {
    let shown = widgets::card(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        widgets::card_title(ui, scheme, &t("s_gr_rechner"));
        let info = match &state.machine {
            Machine::Unasked | Machine::Reading(_) => {
                widgets::loading(ui, scheme, &t("s_gr_rechner_liest"));
                return;
            }
            Machine::Known(known) => match known.as_ref() {
                Some(info) => info,
                None => {
                    ui.label(RichText::new(t("s_gr_rechner_leer")).small().color(color(scheme.sub)));
                    return;
                }
            },
        };
        egui::Grid::new("grafik-rechner").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            for (caption, value) in machine_rows(info) {
                ui.label(RichText::new(caption).color(color(scheme.sub)));
                ui.add(egui::Label::new(value).wrap());
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        let Some(class) = info.machine_class else {
            ui.label(RichText::new(t("s_gr_klasse")).small());
            ui.label(RichText::new(t("s_gr_klasse_leer")).small().color(color(scheme.sub)));
            return;
        };
        // The class is named in the line itself (a screen reader reads the bar as five loose words).
        ui.horizontal_wrapped(|line| {
            line.label(RichText::new(t("s_gr_klasse")).small());
            line.label(RichText::new(t(&format!("s_gr_stufe_{class}"))).strong());
        });
        if info.class_remembered
            && let Some(day) =
                info.class_at.and_then(formats::local_time).map(|time| formats::strftime("%Y-%m-%d", &time))
        {
            let said = tf("s_gr_klasse_gemerkt", &[("0", Arg::from(day.as_str()))]);
            ui.label(RichText::new(said).small().color(color(scheme.sub)));
        }
        ui.add_space(4.0);
        let top = machine_info::CLASSES;
        ui.columns(top as usize, |columns| {
            for (i, column) in columns.iter_mut().enumerate() {
                let n = i as i64 + 1;
                let share = (n - 1) as f64 / (top - 1) as f64;
                let bar = if n <= class { scheme.red.mix(scheme.accent, share) } else { scheme.line };
                let on = n == class;
                // All bars are as high as each other and so are the names (the row stays straight); the class
                // of the computer has a frame around its bar and its name in bold.
                let (rect, _) =
                    column.allocate_exact_size(egui::vec2(column.available_width(), 8.0), egui::Sense::hover());
                column.painter().rect_filled(rect, 2.0, color(bar));
                if on {
                    let frame = egui::Stroke::new(1.5, color(scheme.fg));
                    column.painter().rect_stroke(rect.expand(2.0), 3.0, frame, egui::StrokeKind::Outside);
                }
                let name = t(&format!("s_gr_stufe_{n}"));
                let text = if on {
                    RichText::new(name).small().strong()
                } else {
                    RichText::new(name).small().color(color(scheme.sub))
                };
                column.label(text);
            }
        });
        ui.add_space(6.0);
        let native = state.upscaling_native();
        let quality = state.quality_wanted();
        for (hue, line) in performance_hints(scheme, &quality, native, class) {
            ui.add(egui::Label::new(RichText::new(line).small().color(hue)).wrap());
            ui.add_space(4.0);
        }
    });
    shown.response.rect.width()
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect};
    use versekit_core::language::Choice;
    use versekit_core::paths::{ENV_INSTALL_DIR, Host};
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    type Texts = Vec<(String, Pos2)>;
    type Named = Vec<(egui::Role, String, Rect)>;

    const ATTRIBUTES: &str = "<Attributes Version=\"35\">\n <Attr name=\"MotionBlur\" value=\"0\"/>\n</Attributes>\n";
    const JSON: &str = "{\"GraphicsSettings\": {\"GraphicsRenderer\": 1}}";
    const LOG: &str = "<t> Host CPU: AMD Ryzen 7 5800X3D\n<t> Logical CPU Count: 16\n<t> 32768MB physical memory installed\n<t> [VK] Chosen Vulkan GPU Device (NVIDIA GeForce RTX 4070) Driver Version (560.94) Vulkan API (1.3.280)\n<t> GPU: DedicatedVidMemMB = 12282\n<t> Final rating: Machine class 4\n";

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("versekit-grafikpage-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A user on Linux with the game and the three files; the Game.log is `age` seconds old (`None`: none).
    fn world(name: &str, age: Option<u64>) -> (Paths, String, String) {
        let home = scratch(name);
        let prefix = format!("{home}/Games/star-citizen/drive_c");
        let game = format!("{prefix}/Program Files/Roberts Space Industries/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(format!("{profile}/attributes.xml"), ATTRIBUTES).expect("attributes");
        std::fs::write(format!("{game}/user.cfg"), "g_language = german_(germany)\nsys_MaxFPS = 60\n").expect("cfg");
        let version =
            format!("{prefix}/users/ich/AppData/Local/Star Citizen/starcitizen_(sc-alpha-4.2)_x/GraphicsSettings");
        std::fs::create_dir_all(&version).expect("folder");
        std::fs::write(format!("{version}/GraphicsSettings.json"), JSON).expect("json");
        let mut vars = vec![("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        if let Some(age) = age {
            std::fs::write(format!("{game}/Game.log"), LOG).expect("log");
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(format!("{game}/Game.log"))
                .and_then(|f| f.set_modified(at))
                .expect("age");
        }
        vars.push((ENV_INSTALL_DIR, game.as_str()));
        let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
        (paths, home, game)
    }

    struct Bench {
        ctx: egui::Context,
        state: GrafikState,
        paths: Paths,
        game: String,
        home: String,
        clock: Option<f64>,
    }

    impl Bench {
        fn new(name: &str, age: Option<u64>) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let (paths, home, game) = world(name, age);
            Bench { ctx, state: GrafikState::default(), paths, game, home, clock: None }
        }

        fn frame(&mut self, events: Vec<Event>) -> (Texts, Named) {
            let scheme = theme::by_id("original").expect("original");
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1100.0, 2600.0))),
                time: self.clock,
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            let mut output = self.ctx.run_ui(input, |ui| {
                show_head(ui, scheme, state, paths);
                show_page(ui, scheme, state, paths);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts) {
                match shape {
                    egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
            let nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| {
                            let b = node.bounds()?;
                            let rect = Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            );
                            Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (texts, nodes)
        }

        /// Frames until the page stands still. The box of the computer is read in a thread of its own and takes the
        /// rows below it along when it arrives (a loading line becomes a table): a click placed before that lands
        /// beside its row, so the frames wait for the box, not for a time.
        fn settle(&mut self) -> (Texts, Named) {
            let mut last = (Vec::new(), Vec::new());
            for _ in 0..4 {
                last = self.frame(Vec::new());
            }
            let mut waited = false;
            for _ in 0..3000 {
                if !matches!(self.state.machine, Machine::Reading(_)) {
                    break;
                }
                waited = true;
                std::thread::sleep(std::time::Duration::from_millis(10));
                last = self.frame(Vec::new());
            }
            if waited {
                for _ in 0..4 {
                    last = self.frame(Vec::new());
                }
            }
            last
        }

        /// Frames until `done`, at most three seconds (the box reads in a thread of its own).
        fn until(&mut self, done: impl Fn(&Texts) -> bool) -> Texts {
            let mut texts = self.settle().0;
            for _ in 0..60 {
                if done(&texts) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                texts = self.settle().0;
            }
            texts
        }

        fn press_at(&mut self, at: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.settle();
        }

        fn click(&mut self, label: &str) {
            let texts = self.settle().0;
            let at = texts
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.press_at(at);
        }

        fn click_named(&mut self, role: egui::Role, name: &str) {
            let nodes = self.settle().1;
            let rect = nodes
                .iter()
                .find(|(r, n, _)| *r == role && n == name)
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no {role:?} `{name}`: {nodes:?}"));
            self.press_at(rect.center());
        }

        fn text_of(&self, path: &str) -> String {
            std::fs::read_to_string(path).expect("file").replace("\r\n", "\n")
        }

        fn attributes(&self) -> String {
            self.text_of(&format!("{}/USER/Client/0/Profiles/default/attributes.xml", self.game))
        }
    }

    fn shows(texts: &Texts, text: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(text))
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_language(Choice::De);
        guard
    }

    #[test]
    fn the_page_shows_the_files_and_the_computer_according_to_the_game() {
        let _guard = setup();
        let mut bench = Bench::new("zeigt", Some(7200));
        let texts = bench.until(|t| shows(t, "AMD Ryzen"));
        for key in ["s_gr_lead", "s_gr_rechner", "s_gr_renderer", "s_gr_vsync", "s_gr_maxfps"] {
            assert!(shows(&texts, &t(key)), "{key}: {texts:?}");
        }
        // The names of the groups stand as written.
        for key in ["hf_grafik", "s_gr_q_titel", "s_gr_cfg_titel"] {
            assert!(shows(&texts, &t(key)), "{key}: {texts:?}");
        }
        assert!(shows(&texts, "Vulkan"), "the renderer of the file: {texts:?}");
        assert!(shows(&texts, "NVIDIA GeForce RTX 4070"), "{texts:?}");
        assert!(shows(&texts, &t("s_gr_stufe_4")), "the class of the game and the bar: {texts:?}");
        assert!(shows(&texts, &t("s_gr_alles_drin")), "nothing waits: {texts:?}");
        let fps = bench.state.fps.clone();
        assert_eq!(fps[0], "60", "the frame rate of the user.cfg stands in its field");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_page_stands_still_when_nothing_happens_and_when_a_frame_comes_late() {
        let _guard = setup();
        let mut bench = Bench::new("steht", Some(7200));
        bench.until(|t| shows(t, "AMD Ryzen"));
        let mut clock = 100.0;
        bench.clock = Some(clock);
        let reference = bench.settle().0;
        // A repaint every 1/60 s, every second, and one after a pause of three seconds (no input meanwhile).
        for step in [1.0 / 60.0, 1.0, 3.0, 1.0 / 60.0, 1.0] {
            for _ in 0..5 {
                clock += step;
                bench.clock = Some(clock);
                let (texts, _) = bench.frame(Vec::new());
                assert_eq!(texts, reference, "the page moved after a step of {step} s");
            }
        }
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_class_no_log_has_any_more_is_drawn_from_the_note_with_the_day_it_was_rated() {
        let _guard = setup();
        let mut bench = Bench::new("klasse-gemerkt", Some(7200));
        let texts = bench.until(|t| shows(t, "AMD Ryzen"));
        assert!(shows(&texts, &t("s_gr_stufe_4")), "fresh from the log: {texts:?}");
        let marker = t("s_gr_klasse_gemerkt").replace("{0}", "");
        assert!(!shows(&texts, &marker), "a class read from the log has no line of its own: {texts:?}");
        // The game starts anew and writes no rating: the page asks again and gets the noted class.
        let log = LOG.replace("<t> Final rating: Machine class 4\n", "");
        std::fs::write(format!("{}/Game.log", bench.game), log).expect("log");
        bench.state.machine = Machine::Unasked;
        let texts = bench.until(|t| shows(t, &marker));
        let Machine::Known(known) = &bench.state.machine else { panic!("the box was not read") };
        let info = known.as_ref().as_ref().expect("a log");
        assert_eq!((info.machine_class, info.class_remembered), (Some(4), true));
        let day = info.class_at.and_then(formats::local_time).map(|time| formats::strftime("%Y-%m-%d", &time));
        let said = tf("s_gr_klasse_gemerkt", &[("0", Arg::from(day.expect("a time").as_str()))]);
        assert!(shows(&texts, &said), "the day of the rating: {texts:?}");
        assert!(shows(&texts, &t("s_gr_stufe_4")), "the scale is there with the noted class: {texts:?}");
        assert!(!shows(&texts, &t("s_gr_klasse_leer")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_change_waits_for_the_button_and_is_written_with_a_copy_when_the_game_is_quiet() {
        let _guard = setup();
        let mut bench = Bench::new("schreibt", Some(7200));
        bench.settle();
        // VSync is on by default (no line); the switch turns it off.
        bench.click_named(egui::Role::CheckBox, &t("s_gr_vsync"));
        assert_eq!(bench.state.pending.attribute("VSync"), Some(Some(0.0)));
        assert!(!bench.attributes().contains("VSync"), "nothing is written before the button");
        let texts = bench.settle().0;
        assert!(shows(&texts, "1 Änderung noch nicht geschrieben"), "{texts:?}");
        bench.click(&t("s_gr_schreiben"));
        assert!(bench.attributes().contains("name=\"VSync\" value=\"0\""), "{}", bench.attributes());
        let said = bench.state.take_notice().expect("the answer");
        assert!(!said.fault && said.text.contains("attributes.xml.versekit-"), "{said:?}");
        assert!(bench.state.pending.is_empty());
        let copies: Vec<String> = std::fs::read_dir(format!("{}/USER/Client/0/Profiles/default", bench.game))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("attributes.xml.versekit-"))
            .collect();
        assert_eq!(copies.len(), 1, "{copies:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn while_the_game_runs_the_changes_are_kept_and_not_written() {
        let _guard = setup();
        let mut bench = Bench::new("laeuft", Some(5));
        bench.settle();
        bench.click_named(egui::Role::CheckBox, &t("s_gr_vsync"));
        bench.click(&t("s_gr_schreiben"));
        let said = bench.state.take_notice().expect("the answer");
        assert!(!said.fault && said.text.starts_with("1 Änderung vorgemerkt"), "{said:?}");
        assert!(bench.state.queued);
        assert_eq!(graphics_apply::queued(&bench.paths).len(), 1, "kept in the settings");
        assert!(!bench.attributes().contains("VSync"));
        // The game ends and the overlay writes: the page takes the files as they are now.
        let log = format!("{}/Game.log", bench.game);
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options().write(true).open(&log).and_then(|f| f.set_modified(old)).expect("age");
        assert!(matches!(graphics_apply::apply_queued(&bench.paths, None), graphics_apply::Queued::Done(Ok(_))));
        bench.settle();
        assert!(bench.state.pending.is_empty() && !bench.state.queued, "what was kept is the state of the file now");
        assert!(bench.attributes().contains("name=\"VSync\" value=\"0\""));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn when_nobody_can_tell_whether_the_game_runs_nothing_is_written_or_kept_and_the_page_says_so() {
        let _guard = setup();
        let mut bench = Bench::new("unklar", None);
        // Without a Game.log the folder of the game is not found by the page either: the wish is made directly.
        bench.state.pending.set_attribute("VSync", Some(0.0));
        bench.state.store(&bench.paths);
        let said = bench.state.take_notice().expect("the answer");
        assert!(said.fault && said.text == WriteError::GameUnknown.reason(), "{said:?}");
        assert!(graphics_apply::queued(&bench.paths).is_empty() && !bench.state.queued);
        assert!(!bench.attributes().contains("VSync"), "{}", bench.attributes());
        assert_eq!(bench.state.pending.len(), 1, "the wish stays");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_frame_rate_is_taken_from_digits_only_and_goes_into_the_user_cfg() {
        let _guard = setup();
        let mut bench = Bench::new("fps", Some(7200));
        bench.settle();
        let fields: Vec<_> = bench
            .settle()
            .1
            .into_iter()
            .filter(|(r, n, _)| *r == egui::Role::TextInput && *n == t("s_gr_idlefps"))
            .collect();
        assert_eq!(fields.len(), 1, "the field has the name of its row");
        bench.press_at(fields[0].2.center());
        bench.frame(vec![Event::Text("abc".to_string())]);
        bench.settle();
        assert_eq!(bench.state.pending.cfg_value("sys_MaxIdleFPS"), None, "letters are not taken over");
        bench.state.fps[1].clear();
        bench.frame(vec![Event::Text("30".to_string())]);
        bench.settle();
        assert_eq!(bench.state.pending.cfg_value("sys_MaxIdleFPS"), Some(Some("30")));
        bench.click(&t("s_gr_schreiben"));
        let cfg = bench.text_of(&format!("{}/user.cfg", bench.game));
        assert_eq!(cfg, "sys_MaxFPS = 60\nsys_MaxIdleFPS = 30\ng_language = german_(germany)\n");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn an_overall_level_sets_every_row_as_the_game_does_it() {
        let _guard = setup();
        let mut bench = Bench::new("gesamt", Some(7200));
        bench.settle();
        bench.click_named(egui::Role::ComboBox, &t("s_gr_q_gesamt"));
        // The list of the drop-down is the last place the word stands (the bar of the box has it too).
        let at = bench
            .settle()
            .0
            .iter()
            .filter(|(text, _)| *text == t("s_gr_stufe_2"))
            .map(|(_, at)| *at)
            .next_back()
            .expect("the list is open");
        bench.press_at(at + egui::vec2(3.0, 3.0));
        let expected = graphics::preset_rows(2);
        assert_eq!(bench.state.pending.attribute(graphics::OVERALL), Some(Some(2.0)));
        for (row, level) in expected {
            assert_eq!(bench.state.pending.attribute(row), Some(Some(level as f64)), "{row}");
        }
        bench.click(&t("s_gr_schreiben"));
        assert!(bench.attributes().contains("name=\"SysSpec\" value=\"2\""), "{}", bench.attributes());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn without_the_file_the_page_says_where_it_comes_from() {
        let _guard = setup();
        let mut bench = Bench::new("ohne", Some(7200));
        std::fs::remove_file(format!("{}/USER/Client/0/Profiles/default/attributes.xml", bench.game)).unwrap();
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_gr_keine_datei")), "{texts:?}");
        assert!(!shows(&texts, &t("s_gr_q_titel")), "no rows without the file");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn what_was_kept_before_is_on_the_page_again_and_is_not_overwritten_by_new_changes() {
        let _guard = setup();
        let mut bench = Bench::new("neu-geoeffnet", Some(5));
        let mut kept = Changes::default();
        kept.set_attribute("MotionBlur", Some(1.0));
        assert!(graphics_apply::set_queued(&bench.paths, &kept));
        let texts = bench.settle().0;
        assert_eq!(bench.state.pending, kept, "taken up when the page is first drawn");
        assert!(bench.state.queued);
        assert!(shows(&texts, "1 Änderung vorgemerkt"), "the bar says so: {texts:?}");
        // A new wish joins it, and what is kept follows at once (the game runs, the log is fresh).
        bench.click_named(egui::Role::CheckBox, &t("s_gr_vsync"));
        assert_eq!(bench.state.pending.len(), 2);
        assert_eq!(graphics_apply::queued(&bench.paths).len(), 2, "the note follows the wishes");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_page_itself_writes_what_was_kept_when_the_game_is_quiet_and_nobody_else_did() {
        let _guard = setup();
        let mut bench = Bench::new("selbst", Some(7200));
        let mut kept = Changes::default();
        kept.set_attribute("VSync", Some(0.0));
        assert!(graphics_apply::set_queued(&bench.paths, &kept));
        bench.settle();
        assert!(bench.attributes().contains("name=\"VSync\" value=\"0\""), "{}", bench.attributes());
        assert!(graphics_apply::queued(&bench.paths).is_empty(), "the note is gone");
        assert!(bench.state.pending.is_empty() && !bench.state.queued);
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.contains("attributes.xml.versekit-"), "{said:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_frame_rate_with_letters_says_so_at_the_field_and_is_not_written() {
        let _guard = setup();
        let mut bench = Bench::new("buchstaben", Some(7200));
        bench.settle();
        let field = bench
            .settle()
            .1
            .into_iter()
            .find(|(r, n, _)| *r == egui::Role::TextInput && *n == t("s_gr_maxfps"))
            .expect("the field")
            .2;
        bench.press_at(field.center());
        bench.frame(vec![Event::Text("0x".to_string())]);
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_gr_fps_ungueltig")), "{texts:?}");
        bench.state.pending.set_attribute("VSync", Some(0.0));
        bench.click(&t("s_gr_schreiben"));
        let said = bench.state.take_notice().expect("the answer");
        assert!(said.fault && said.text == t("s_gr_fps_ungueltig"), "{said:?}");
        assert!(!bench.attributes().contains("VSync"));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn discarding_asks_in_the_row_and_drops_what_waits() {
        let _guard = setup();
        let mut bench = Bench::new("verwerfen", Some(7200));
        bench.settle();
        bench.click_named(egui::Role::CheckBox, &t("s_gr_vsync"));
        assert_eq!(bench.state.pending.len(), 1);
        bench.click(&t("s_gr_verwerfen"));
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_gr_verwerfen_frage")) && bench.state.pending.len() == 1, "asked first: {texts:?}");
        bench.click(&t("abbrechen"));
        assert_eq!(bench.state.pending.len(), 1, "No keeps everything");
        bench.click(&t("s_gr_verwerfen"));
        bench.click(&t("s_mlg_bestaetigen"));
        assert!(bench.state.pending.is_empty());
        let texts = bench.settle().0;
        assert!(shows(&texts, &t("s_gr_alles_drin")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_tip_names_three_rows_and_counts_the_rest_and_reads_what_is_wanted() {
        let _guard = setup();
        let mut bench = Bench::new("tipp", Some(7200));
        // Every row at "Ultra": all stand above what the game sets for the class of this computer.
        let rows: String = graphics::QUALITY_ROWS
            .iter()
            .map(|(row, _)| format!(" <Attr name=\"{row}\" value=\"{}\"/>\n", graphics::top_level(row).unwrap_or(4)))
            .collect();
        let profile = format!("{}/USER/Client/0/Profiles/default/attributes.xml", bench.game);
        std::fs::write(&profile, format!("<Attributes>\n{rows}</Attributes>\n")).unwrap();
        let tip = |texts: &Texts| texts.iter().map(|(t, _)| t.clone()).find(|t| t.contains(" → ")).unwrap_or_default();
        let texts = bench.until(|t| t.iter().any(|(s, _)| s.contains(" → ")));
        let line = tip(&texts);
        assert_eq!(line.matches(" → ").count(), TIP_NAMES, "three rows named: {line}");
        assert!(line.contains(" +"), "and the rest counted: {line}");
        // The overall level "Low" as a wish: nothing stands above any more, the tip follows at once.
        for (row, _) in graphics::QUALITY_ROWS {
            bench.state.pending.set_attribute(row, Some(1.0));
        }
        let texts = bench.settle().0;
        assert!(tip(&texts).is_empty() || !tip(&texts).contains("Ultra →"), "the wishes are read: {}", tip(&texts));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_sliders_have_a_name_a_role_and_a_number_with_its_unit() {
        let _guard = setup();
        let mut bench = Bench::new("regler", Some(7200));
        let nodes = bench.settle().1;
        for key in ["s_gr_schaerfen", "s_gr_aberration"] {
            assert!(nodes.iter().any(|(r, n, _)| *r == egui::Role::Slider && *n == t(key)), "{key}: {nodes:?}");
        }
        let texts = bench.settle().0;
        assert!(shows(&texts, "100 %") && shows(&texts, "0 %"), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
