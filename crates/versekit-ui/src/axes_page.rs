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

//! The page "Achsen & Kurven" (`_axes` of 4.12): how sharply an axis answers, and whether the setting still counts.
//!
//! This part: the devices, the axes of a device with their dead zone and saturation, the curve, the sliders and
//! the sensitivity of the flight functions that lie on the axis. A slider **never writes**: it changes the
//! preview and the mark "Ungespeicherte Änderung"; between "set" and "written" stands a person, because the whole
//! control of the player hangs on that file.
//!
//! ⚠ Unlike 4.12 the sliders **keep their state when the player moves to another axis or device** (4.12 built the
//! page anew and threw away what had been dragged without a word), and the curve shows the sensitivity the sliders
//! show, not the one in the file.

use versekit_core::language::SpawnWithLanguage as _;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::axes::{self, DEFAULT_EXPONENT, DeviceBlock, Function, GameProp, Prop, Summary};
use versekit_core::axes_write::{self, Setting};
use versekit_core::device_hub::{self, SourceStamp};
use versekit_core::device_sets;
use versekit_core::formats;
use versekit_core::game_files;
use versekit_core::joysticks;
use versekit_core::joysticks_write::{self, Failure};
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::axes_plot::{self, Large, Values};
use crate::settings::Said;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius, icons};

/// How often the files are looked at, in seconds (they are read again only when they changed).
const POLL_SECONDS: f64 = 3.0;
/// The page counts as left when it was not drawn for this long.
const LEFT_SECONDS: f64 = 4.0 * POLL_SECONDS;
/// A look that takes longer than this is given up.
const LOOK_GIVE_UP_SECONDS: f64 = 15.0;
/// The space between two parts of the page.
const GAP: f32 = 8.0;
/// The picture of the curve: at least this side, in points (also where the card is narrower than the picture).
const PLOT_LEAST: f32 = 240.0;
/// The picture of the curve: at most this side, in points.
const PLOT_MOST: f32 = 420.0;
/// The share of the axis card the picture takes where it stands beside the sliders.
const PLOT_SHARE: f32 = 0.40;
/// Below this width of the axis card the picture stands above the sliders instead of beside them.
const PLOT_BESIDE: f32 = 640.0;
/// The width of the list of the axes at the left.
const LIST_WIDTH: f32 = 400.0;
/// Below this width the list of the axes stands above the axis instead of beside it.
const SIDE_BY_SIDE: f32 = LIST_WIDTH + GAP + PLOT_BESIDE;
/// The widest the card of the axis gets: wider, its picture and sliders would leave the right of it empty.
const AXIS_MOST: f32 = 1400.0;
/// The widest a slider gets, in points.
const SLIDER_MOST: f32 = 520.0;
/// The id of the menu "Gerät" and of the menu "Gerätesatz".
const DEVICE_MENU: &str = "achsen-menue-geraet";
const SET_MENU: &str = "achsen-menue-satz";
const COPY_MENU: &str = "achsen-menue-uebertragen";

/// A line of the list of the files: where it lies and when it was changed.
#[derive(Clone, Debug)]
struct FileLine {
    path: String,
    /// `TT.MM.JJJJ HH:MM`, empty where the time can not be read.
    changed: String,
}

/// One look at the files.
#[derive(Clone)]
struct Look {
    /// How many blocks of the file belong to no device any more (what the clearing would take out).
    dead: usize,
    /// The device sets, the newest first.
    sets: Vec<device_sets::Set>,
    /// A file is there but can not be read now (locked, half written, no UTF-8, too large): "no file" would be wrong.
    unreadable: bool,
    stamp: SourceStamp,
    /// Why nothing may be written now; `None` when the game is quiet.
    quiet: Option<String>,
    files: Vec<FileLine>,
    summary: Summary,
    /// The number the game gives a device, by its id.
    numbers: HashMap<String, i64>,
    /// What lies on the physical axes, by number of the device and axis.
    functions: HashMap<(i64, String), Vec<Function>>,
}

impl Look {
    /// The flight functions on an axis of a device (by its id).
    fn on_axis(&self, ident: &str, axis: &str) -> &[Function] {
        self.numbers
            .get(ident)
            .and_then(|number| self.functions.get(&(*number, axis.to_string())))
            .map_or(&[], Vec::as_slice)
    }
}

/// What a slider is for: the dead zone or saturation of an axis of a device, or the sensitivity of a function.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Physical { ident: String, axis: String, prop: Prop },
    Game { number: i64, axis: String },
}

/// A slider in whole steps: thousandths for dead zone and saturation, twentieths for the sensitivity.
#[derive(Clone, Copy, Debug)]
struct Slide {
    /// Where it stood when the player took it up (the value of the file, put on the grid of the slider).
    start: i64,
    now: i64,
}

impl Slide {
    fn changed(&self) -> bool {
        self.start != self.now
    }
}

/// The steps of a slider and its ends.
#[derive(Clone, Copy)]
struct Scale {
    per_one: f64,
    from: i64,
    to: i64,
}

const SHARE: Scale = Scale { per_one: 1000.0, from: 0, to: 1000 };
/// 0.2 to 4.0 in steps of 0.05.
const EXPONENT: Scale = Scale { per_one: 20.0, from: 4, to: 80 };

impl Scale {
    /// The value of the file on the grid of the slider, as Tk puts it (to the nearest step, a half upwards).
    fn units(self, value: f64) -> i64 {
        let units = (value * self.per_one + 0.5).floor();
        (units as i64).clamp(self.from, self.to)
    }

    fn value(self, units: i64) -> f64 {
        units as f64 / self.per_one
    }
}

/// What the page keeps.
pub struct AxesState {
    look: Option<Arc<Look>>,
    job: Option<Receiver<Arc<Look>>>,
    job_started: f64,
    polled_at: Option<f64>,
    drawn_at: Option<f64>,
    read_failed: bool,
    /// The id of the device chosen; empty before the first choice.
    device: String,
    /// The axis chosen; empty before the first choice.
    axis: String,
    /// The whole view (−1 to 1) instead of the quadrant. Kept for the session, not saved.
    whole: bool,
    sliders: HashMap<Key, Slide>,
    /// The function whose sensitivity the curve draws: the one whose slider was moved or clicked last. Where it does
    /// not lie on the axis in view, the first function of the axis counts.
    shown: Option<Key>,
    /// The axes ticked in the block "Auf andere Achsen übertragen", and the device and axis they were ticked for.
    copy_targets: HashSet<String>,
    copy_for: (String, String),
    large: Option<Large>,
    notice: Option<Said>,
    /// A change that waits for the yes of the player.
    pending: Option<Pending>,
    /// The block "Altbestand" is open.
    leftovers_open: bool,
    /// The name in the field for a new device set.
    set_name: String,
    /// Why the name in the field was refused, with the name it was refused for: shown under the field for as long as
    /// the field holds that name.
    set_refusal: Option<(String, String)>,
    /// The side of the picture of the curve as last drawn (it follows the width of the card).
    plot_side: f32,
    /// The page draws the axis of a device: the foot with "write" and "discard" stands below it.
    footed: bool,
    /// The height the foot took when it was last drawn, for the room the page leaves it.
    foot_height: f32,
}

impl Default for AxesState {
    fn default() -> Self {
        AxesState {
            look: None,
            job: None,
            job_started: 0.0,
            polled_at: None,
            drawn_at: None,
            read_failed: false,
            device: String::new(),
            axis: String::new(),
            whole: false,
            sliders: HashMap::new(),
            shown: None,
            copy_targets: HashSet::new(),
            copy_for: (String::new(), String::new()),
            large: None,
            notice: None,
            pending: None,
            leftovers_open: false,
            set_name: String::new(),
            set_refusal: None,
            plot_side: PLOT_LEAST,
            footed: false,
            foot_height: 0.0,
        }
    }
}

impl AxesState {
    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// The room the foot below the page needs (0 where the page draws none): the page scrolls above it.
    pub fn foot_height(&self) -> f32 {
        if self.footed { self.foot_height } else { 0.0 }
    }

    /// The block "Altbestand" open (the tests look at what is in it).
    #[cfg(test)]
    pub(crate) fn open_leftovers(&mut self) {
        self.leftovers_open = true;
    }

    /// Reads everything again at the next look.
    pub fn invalidate(&mut self) {
        self.polled_at = None;
        if let Some(look) = self.look.take() {
            // What is shown stays until the new look is there; only its mark is dropped, so the files are read.
            self.look = Some(Arc::new(Look { stamp: SourceStamp::default(), ..(*look).clone() }));
        }
    }

    /// Starts the look in a thread of its own when it is due, takes its result when it is there.
    fn poll(&mut self, ui: &Ui, paths: &Paths) {
        let now = ui.input(|i| i.time);
        if self.drawn_at.is_some_and(|at| now - at > LEFT_SECONDS) {
            self.polled_at = None;
            self.job = None;
            // A question that waited while the page was away asks about an old state: asked anew.
            self.pending = None;
        }
        self.drawn_at = Some(now);
        if let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(look) => {
                    self.job = None;
                    self.read_failed = false;
                    self.look = Some(look);
                }
                Err(mpsc::TryRecvError::Empty) if now - self.job_started > LOOK_GIVE_UP_SECONDS => self.job = None,
                Err(mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(100)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    if !self.read_failed {
                        self.notice = Some(Said::fault(t("s_gh_f_lesen")));
                    }
                    self.read_failed = true;
                }
            }
        }
        if self.job.is_none() && self.polled_at.is_none_or(|at| now - at >= POLL_SECONDS) {
            self.polled_at = Some(now);
            self.job_started = now;
            let (sender, receiver) = mpsc::channel();
            let copy = paths.working_copy();
            let before = self.look.clone();
            let spawned = std::thread::Builder::new().name("axes-look".into()).spawn_with_language(move || {
                let _ = sender.send(read_look(&copy, before));
            });
            match spawned {
                Ok(_) => self.job = Some(receiver),
                Err(_) => {
                    if !self.read_failed {
                        self.notice = Some(Said::fault(t("s_gh_f_lesen")));
                    }
                    self.read_failed = true;
                }
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(POLL_SECONDS));
    }

    /// The slider for `key`, put up at the value of the file, or started afresh where the file has moved on.
    fn slide(&mut self, key: &Key, file: i64) -> &mut Slide {
        let entry = self.sliders.entry(key.clone()).or_insert(Slide { start: file, now: file });
        if entry.start != file && entry.start == entry.now {
            // Not touched, and the file says something else now (it was changed by the game or by a tool).
            *entry = Slide { start: file, now: file };
        }
        entry
    }

    /// Has the player moved a slider that is not written yet?
    fn changed_any(&self) -> bool {
        self.sliders.values().any(Slide::changed)
    }

    /// The settings the sliders ask for: every slider that was moved, in a fixed order (the physical axes by
    /// device and axis, dead zone before saturation; then the game axes by number and name).
    fn settings(&self) -> Vec<Setting> {
        let rank = |axis: &str| axes::AXES.iter().position(|known| *known == axis).unwrap_or(99);
        let mut moved: Vec<(&Key, &Slide)> = self.sliders.iter().filter(|(_, slide)| slide.changed()).collect();
        moved.sort_by_key(|(key, _)| match key {
            Key::Physical { ident, axis, prop } => (0, ident.clone(), rank(axis), axis.clone(), *prop as u8),
            Key::Game { number, axis } => (1, number.to_string(), 0, axis.clone(), 0),
        });
        moved
            .into_iter()
            .map(|(key, slide)| match key {
                Key::Physical { ident, axis, prop } => Setting::Device {
                    ident: ident.clone(),
                    axis: axis.clone(),
                    prop: *prop,
                    // Round to four places the way 4.12 does: what the player sees is what is written.
                    value: Some(versekit_core::pyfloat::round(SHARE.value(slide.now), 4)),
                },
                Key::Game { number, axis } => Setting::Game {
                    number: *number,
                    axis: axis.clone(),
                    prop: GameProp::Exponent,
                    value: Some(versekit_core::pyfloat::round(EXPONENT.value(slide.now), 4)),
                },
            })
            .collect()
    }

    /// The yes under a question: makes the change, tells what came of it, has the files read again.
    fn confirm(&mut self, paths: &Paths) {
        let Some(pending) = self.pending.take() else { return };
        let result = match pending.change {
            Change::Align => axes_write::align(paths, &pending.from, &pending.to, None, None)
                .map(|done| tf("s_ac_angeglichen", &[("0", Arg::from(i64::try_from(done.count).unwrap_or(i64::MAX)))])),
            Change::Swap => joysticks_write::swap_bindings(paths, &pending.from, &pending.to, None, None)
                .map(|_| t("s_ac_getauscht")),
            Change::Clean => axes_write::clean_up(paths, None, None).map(|done| {
                let key = if done.count == 1 { "s_ac_aufgeraeumt_eins" } else { "s_ac_aufgeraeumt" };
                tf(key, &[("0", Arg::from(i64::try_from(done.count).unwrap_or(i64::MAX)))])
            }),
            Change::SetApply => device_sets::apply(paths, &pending.from, None, None).map(|done| {
                let key = if done.count == 1 { "s_gs_angewendet_eins" } else { "s_gs_angewendet" };
                tf(key, &[("0", Arg::from(i64::try_from(done.count).unwrap_or(i64::MAX)))])
            }),
            Change::SetDelete => device_sets::delete(paths, &pending.from)
                .map(|()| tf("s_gs_geloescht", &[("0", Arg::from(pending.from.as_str()))])),
            Change::SetOverwrite => device_sets::save(paths, &pending.from, true, None, None).map(|_| {
                self.set_name.clear();
                tf("s_gs_gesichert", &[("0", Arg::from(pending.from.as_str()))])
            }),
            Change::Adopt => {
                let case = self.look.as_ref().and_then(|look| {
                    look.summary
                        .adoptable
                        .iter()
                        .find(|case| case.old.ident == pending.from && case.new.ident == pending.to && !case.ambiguous)
                        .cloned()
                });
                match case {
                    Some(case) => axes_write::apply_settings(paths, &axes_write::adopt_settings(&case), None, None)
                        .map(|_| t("s_ac_gespeichert")),
                    // The file moved on while the question stood there: nothing to carry over any more.
                    None => Err(Failure::Nothing),
                }
            }
        };
        self.notice = Some(match result {
            Ok(text) => {
                // The files moved under the sliders: a slider the player moved before would write on the next
                // saving onto whatever stands there now (after the swap onto the other stick). Not kept.
                if !matches!(pending.change, Change::SetDelete | Change::SetOverwrite) {
                    self.sliders.clear();
                }
                Said::note(text)
            }
            // Alike already: nothing was written, no copy was made, and that is said as it is.
            Err(Failure::Same) if pending.change == Change::Align => Said::note(t("s_ac_schon_gleich")),
            Err(failure) => Said::fault(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))])),
        });
        self.invalidate();
    }

    /// Saves the state as a device set under the name in the field. A name that is taken asks first.
    fn save_set(&mut self, paths: &Paths) {
        match device_sets::save(paths, &self.set_name, false, None, None) {
            Ok(_) => {
                let name = py_strip_name(&self.set_name);
                self.notice = Some(Said::note(tf("s_gs_gesichert", &[("0", Arg::from(name.as_str()))])));
                self.set_name.clear();
            }
            Err(Failure::SetTaken) => {
                self.pending = Some(Pending {
                    change: Change::SetOverwrite,
                    from: py_strip_name(&self.set_name),
                    to: String::new(),
                    from_name: String::new(),
                    to_name: String::new(),
                });
                return;
            }
            // A bad name is said at the field it was typed in, not in the foot.
            Err(failure @ Failure::Name(_)) => {
                self.set_refusal = Some((self.set_name.clone(), failure.text()));
                return;
            }
            Err(failure) => {
                self.notice = Some(Said::fault(failure.text()));
                return;
            }
        }
        self.set_refusal = None;
        self.invalidate();
    }

    /// Writes what the sliders say: one writing, one copy. Tells what came of it and has the files read again.
    fn save(&mut self, paths: &Paths) {
        let settings = self.settings();
        if settings.is_empty() {
            self.notice = Some(Said::note(t("s_ac_nichts_offen")));
            return;
        }
        match axes_write::apply_settings(paths, &settings, None, None) {
            Ok(_) => {
                self.notice = Some(Said::note(t("s_ac_gespeichert")));
                self.sliders.clear();
                self.invalidate();
            }
            Err(failure) => {
                self.notice = Some(Said::fault(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))])));
            }
        }
    }
}

/// The look: the files only where they changed since `before`.
fn read_look(paths: &Paths, before: Option<Arc<Look>>) -> Arc<Look> {
    let quiet = game_files::check_quiet(paths).err().map(|error| error.reason());
    let stamp = device_hub::source_stamp(paths, None);
    // ⚠ What was read of a file that could not be read is not kept: the mark does not change when the lock goes.
    if let Some(look) = before
        && look.stamp == stamp
        && stamp != SourceStamp::default()
    {
        // The sets lie in a file of their own, which the mark does not cover: read at every look (it is small).
        return Arc::new(Look { stamp, quiet, sets: device_sets::sets(paths), ..(*look).clone() });
    }
    let files: Vec<FileLine> = joysticks::all_actionmaps(paths, None)
        .into_iter()
        .map(|path| {
            let seconds = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|since| i64::try_from(since.as_secs()).ok());
            let changed = seconds
                .and_then(formats::local_time)
                .map(|time| formats::strftime_shown("%d.%m.%Y %H:%M", &time))
                .unwrap_or_default();
            FileLine { path, changed }
        })
        .collect();
    // Not well formed counts as unreadable (the text is there then, but nothing can be said of it).
    let unreadable = joysticks::file_state(paths, None, None) == joysticks::FileState::Unreadable;
    let text = axes::file_text(paths, None, None).unwrap_or_default();
    let alive: HashSet<String> = axes::valid_idents(paths, None, None);
    let summary = axes::summary_from_text(&text, &alive);
    let mut numbers: HashMap<String, i64> = HashMap::new();
    for block in summary.game.iter().filter(|b| b.kind == "joystick" && !b.ident.is_empty()) {
        numbers.entry(block.ident.clone()).or_insert(block.number);
    }
    let mut functions = HashMap::new();
    for number in numbers.values() {
        for axis in axes::AXES {
            let found = axes::functions_on_axis_from_text(&text, *number, axis, &summary.game);
            if !found.is_empty() {
                functions.insert((*number, axis.to_string()), found);
            }
        }
    }
    // Without any live id every block would count as dead: that is not known, so nothing is offered to be cleared.
    let dead = if alive.is_empty() { 0 } else { axes_write::dead_blocks(&summary.blocks).len() };
    let sets = device_sets::sets(paths);
    Arc::new(Look { dead, unreadable, sets, stamp, quiet, files, summary, numbers, functions })
}

fn small(ui: &mut Ui, scheme: &Scheme, text: &str) {
    widgets::reading(ui, RichText::new(text).small().color(color(scheme.sub)));
}

/// The name of a box inside a card (a device set, an old entry), in bold.
fn block_title(ui: &mut Ui, text: &str) {
    let title = widgets::bold(ui, text, egui::TextStyle::Body);
    ui.label(title);
}

/// The plain name of a game axis (`Nicken`), or the technical name where the game has none.
fn function_name(axis: &str) -> String {
    let key = format!("s_ax_{axis}");
    let name = t(&key);
    if name == key { axis.to_string() } else { name }
}

/// What the list shows at the right of an axis: the function, or how many there are.
fn function_label(functions: &[Function]) -> String {
    match functions {
        [] => String::new(),
        [one] => function_name(&one.axis),
        many => tf("s_ac_mehrere_funktionen", &[("n", Arg::from(many.len() as i64))]),
    }
}

/// Draws the page: the devices as tabs with the menus "Gerät" and "Gerätesatz" at their right, a question that waits
/// for its answer, the list of the axes at the left and the axis with its large curve at the right, the old entries
/// below. "Write" and "discard" stand in the foot below the page ([`show_foot`]), which does not scroll.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, paths: &Paths) {
    state.poll(ui, paths);
    state.footed = false;
    lead(ui, scheme);
    let Some(look) = state.look.clone() else {
        widgets::loading(ui, scheme, &t("s_gh_liest"));
        return;
    };
    if look.files.len() > 1 {
        many_files(ui, scheme, &look.files);
        ui.add_space(GAP);
    }
    let live: Vec<&DeviceBlock> =
        look.summary.blocks.iter().filter(|block| block.active && !block.ident.is_empty()).collect();
    if !live.is_empty() && !live.iter().any(|block| block.ident == state.device) {
        state.device = live[0].ident.clone();
    }
    let block = live.iter().find(|block| block.ident == state.device).copied();
    let head = head_bar(ui, scheme, state, &look, &live, block);
    let asked = question(ui, scheme, state, &look);
    let act = head.or(asked);
    let Some(block) = block else {
        small(ui, scheme, &t(if look.unreadable { "s_js_datei_gesperrt" } else { "s_ac_keine" }));
        finish(ui, state, paths, act);
        return;
    };
    let present: Vec<&str> = axes::AXES.iter().copied().filter(|axis| block.axis(axis).is_some()).collect();
    if present.is_empty() {
        // No value: the menus stay, "make it like the other" is the way to fill a device that has none.
        small(ui, scheme, &t("s_ac_keine_werte"));
        let old = leftovers(ui, scheme, state, &look);
        finish(ui, state, paths, act.or(old));
        return;
    }
    if !present.contains(&state.axis.as_str()) {
        state.axis = present[0].to_string();
    }
    state.footed = true;
    let axis = state.axis.clone();
    let wide = ui.available_width() >= SIDE_BY_SIDE;
    let mut chosen: Option<String> = None;
    if wide {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LIST_WIDTH);
                chosen = axes_card(ui, scheme, &look, block, &state.axis);
            });
            ui.add_space(GAP);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width().min(AXIS_MOST));
                axis_card(ui, scheme, state, &look, block, &axis);
            });
        });
    } else {
        chosen = axes_card(ui, scheme, &look, block, &state.axis);
        ui.add_space(GAP);
        axis_card(ui, scheme, state, &look, block, &axis);
    }
    let old = leftovers(ui, scheme, state, &look);
    if let Some(next) = chosen {
        state.axis = next;
    }
    finish(ui, state, paths, act.or(old));
    if let Some(large) = &mut state.large
        && !axes_plot::show_large(ui.ctx(), scheme, large)
    {
        state.large = None;
    }
}

/// The two lines at the top: what the page is for, and on hover of its symbol why old values stop counting.
fn lead(ui: &mut Ui, scheme: &Scheme) {
    let line = ui.horizontal_wrapped(|line| {
        line.add(egui::Label::new(RichText::new(t("s_ac_lead")).small().color(color(scheme.sub))).wrap());
        let side = line.text_style_height(&egui::TextStyle::Small);
        if let Some(image) = icons::themed("info", scheme, versekit_core::theme::IconTint::Normal, side) {
            line.add(image);
        }
    });
    ui.interact(line.response.rect, ui.make_persistent_id("achsen-lead"), egui::Sense::hover())
        .on_hover_text(t("s_ac_hinweis"));
    ui.add_space(GAP);
}

/// Does what was asked for, and closes the menus where a question now waits on the page: it stands under the tabs
/// and must not hide behind a menu.
fn finish(ui: &Ui, state: &mut AxesState, paths: &Paths, act: Option<Act>) {
    run(state, paths, act);
    if state.pending.as_ref().is_some_and(|pending| asked_on_top(pending.change)) {
        for menu in [DEVICE_MENU, SET_MENU] {
            egui::Popup::close_id(ui.ctx(), egui::Id::unique(menu));
        }
    }
}

/// The changes whose question stands under the tabs (the old entries ask in their own card).
fn asked_on_top(change: Change) -> bool {
    !matches!(change, Change::Clean | Change::Adopt)
}

/// The foot below the page, outside the scrolling: "write" and "discard", the mark of an unsaved change and why
/// nothing may be written now. Drawn only where the page shows the axis of a device.
pub fn show_foot(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, paths: &Paths) {
    let Some(look) = state.look.clone().filter(|_| state.footed) else {
        state.foot_height = 0.0;
        return;
    };
    let nothing = t("s_ac_nichts_offen");
    let changed = state.changed_any();
    let save_reason = look.quiet.as_deref().or_else(|| (!changed).then_some(nothing.as_str()));
    let discard_reason = (!changed).then_some(nothing.as_str());
    let mut act = None;
    let top = ui.cursor().top();
    ui.add_space(GAP / 2.0);
    widgets::card(ui, scheme, |ui| {
        ui.horizontal(|ui| {
            let save = t("s_ac_speichern");
            if ui.add(widgets::Button::new(&save, ButtonKind::Strong, scheme).blocked(save_reason)).clicked() {
                act = Some(Act::Save);
            }
            let discard = t("s_ac_verwerfen");
            if ui.add(widgets::Button::new(&discard, ButtonKind::Normal, scheme).blocked(discard_reason)).clicked() {
                act = Some(Act::Discard);
            }
            if changed {
                ui.add(
                    egui::Label::new(RichText::new(t("s_ac_geaendert")).color(color(scheme.gold))).selectable(false),
                );
            }
            // Why nothing may be written now, else that the game has to be closed: at the right end of the foot.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let said = look.quiet.clone().unwrap_or_else(|| t("s_js_spiel_zu"));
                ui.add(egui::Label::new(RichText::new(said).small().color(color(scheme.sub))).wrap());
            });
        });
    });
    state.foot_height = ui.cursor().top() - top;
    run(state, paths, act);
}

/// The tabs of the devices and, at their right, the menus "Gerät" and "Gerätesatz".
fn head_bar(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    look: &Look,
    live: &[&DeviceBlock],
    block: Option<&DeviceBlock>,
) -> Option<Act> {
    let (device_word, set_word) = (t("s_ac_menue_geraet"), t("s_ac_menue_satz"));
    let menus = widgets::icon_button_size(ui, scheme, ButtonKind::Normal, &device_word).x
        + widgets::icon_button_size(ui, scheme, ButtonKind::Normal, &set_word).x
        + 2.0 * ui.spacing().item_spacing.x;
    let mut act = None;
    ui.horizontal_top(|row| {
        let room = (row.available_width() - menus - GAP).max(120.0);
        row.scope(|tabs| {
            tabs.set_max_width(room);
            tabs.horizontal_wrapped(|tabs| {
                for block in live {
                    let on = block.ident == state.device;
                    let kind = if on { ButtonKind::Strong } else { ButtonKind::Normal };
                    let label = device_label(look, block);
                    let response = tabs.add(widgets::Button::new(&label, kind, scheme).wrapping());
                    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, &label));
                    // The device that is chosen already stays as it is, with the axis the player chose.
                    if response.clicked() && !on {
                        state.device = block.ident.clone();
                        state.axis.clear();
                    }
                }
            });
        });
        row.with_layout(egui::Layout::right_to_left(egui::Align::Min), |row| {
            let sets = sets_menu(row, scheme, state, look, &set_word);
            let tools = device_menu(row, scheme, look, block, &device_word);
            act = sets.or(tools);
        });
    });
    ui.add_space(GAP);
    act
}

/// The frame of a menu of the page.
fn menu_frame(scheme: &Scheme) -> egui::Frame {
    egui::Frame::new()
        .fill(color(scheme.field))
        .stroke(egui::Stroke::new(1.0, color(scheme.accent)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin::same(10))
}

/// A menu of the page behind a button with `word`: it stays open while the player works in it and closes on a click
/// beside it or on what it does.
fn menu<R>(
    ui: &mut Ui,
    scheme: &Scheme,
    id: &str,
    word: &str,
    blocked: Option<&str>,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let button = widgets::Button::new(word, ButtonKind::Normal, scheme).icon("chevron-down").blocked(blocked);
    let response = ui.add(button);
    if blocked.is_some() {
        return None;
    }
    egui::Popup::menu(&response)
        .id(egui::Id::unique(id))
        .frame(menu_frame(scheme))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|menu| {
            menu.set_min_width(300.0);
            menu.set_max_width(460.0);
            add(menu)
        })
        .map(|shown| shown.inner)
}

/// The menu "Gerät": make the other stick like this one, and swap the numbers of two sticks. Both ask first and are
/// blocked while the game runs; with one device there is nothing to do with another.
fn device_menu(ui: &mut Ui, scheme: &Scheme, look: &Look, block: Option<&DeviceBlock>, word: &str) -> Option<Act> {
    let others: Vec<&DeviceBlock> = look
        .summary
        .blocks
        .iter()
        .filter(|other| other.active && !other.ident.is_empty() && block.is_some_and(|b| other.ident != b.ident))
        .collect();
    let lonely = t("s_ac_menue_geraet_allein");
    let blocked = (block.is_none() || others.is_empty()).then_some(lonely.as_str());
    let block = block?;
    menu(ui, scheme, DEVICE_MENU, word, blocked, |menu| {
        let mut act = None;
        let quiet = look.quiet.as_deref();
        for (at, (change, head, lead, button)) in [
            (Change::Align, "s_ac_kopf_angleichen", "s_ac_lead_angleichen", "s_ac_angleichen"),
            (Change::Swap, "s_ac_kopf_tauschen", "s_ac_lead_tauschen", "s_ac_tauschen"),
        ]
        .into_iter()
        .enumerate()
        {
            if at > 0 {
                menu.add_space(GAP);
            }
            block_title(menu, &t(head));
            small(menu, scheme, &t(lead));
            menu.add_space(GAP / 2.0);
            for other in &others {
                let wanted = Pending {
                    change,
                    from: block.ident.clone(),
                    to: other.ident.clone(),
                    from_name: device_label(look, block),
                    to_name: device_label(look, other),
                };
                let label = tf(button, &[("0", Arg::from(wanted.to_name.as_str()))]);
                let widget = widgets::Button::new(&label, ButtonKind::Normal, scheme).wrapping().blocked(quiet);
                if menu.add(widget).clicked() {
                    act = Some(Act::Ask(wanted));
                }
            }
        }
        if act.is_some() {
            menu.close();
        }
        act
    })
    .flatten()
}

/// The menu "Gerätesatz": the saved sets with "apply" and "delete", and a new set under a name. Applying asks first
/// (and is blocked while the game runs or when there is nothing to change); deleting asks too; a name that exists
/// asks before it is overwritten. The questions stand under the tabs.
fn sets_menu(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look, word: &str) -> Option<Act> {
    menu(ui, scheme, SET_MENU, word, None, |menu| sets_body(menu, scheme, state, look)).flatten()
}

/// The question of a change that waits for its yes, under the tabs, with its two answers.
fn question(ui: &mut Ui, scheme: &Scheme, state: &AxesState, look: &Look) -> Option<Act> {
    let pending = state.pending.as_ref().filter(|pending| asked_on_top(pending.change))?;
    let names = [("0", Arg::from(pending.from_name.as_str())), ("1", Arg::from(pending.to_name.as_str()))];
    let (text, danger) = match pending.change {
        Change::Align => (tf("s_ac_angleichen_frage", &names), false),
        Change::Swap => (tf("s_ac_tausch_frage", &names), false),
        Change::SetApply => {
            let set = look.sets.iter().find(|set| set.name == pending.from)?;
            let (settings, missing) = device_sets::plan(set, &look.summary.blocks, &look.summary.game);
            let key = if settings.len() == 1 { "s_gs_frage_eins" } else { "s_gs_frage" };
            let mut text = tf(key, &[("0", Arg::from(set.name.as_str())), ("1", Arg::from(settings.len() as i64))]);
            if !missing.is_empty() {
                text.push_str("\n\n");
                text.push_str(&tf("s_gs_fehlt", &[("0", Arg::from(missing.join(", ").as_str()))]));
            }
            (text, false)
        }
        Change::SetDelete => (tf("s_gs_loeschen_frage", &[("0", Arg::from(pending.from.as_str()))]), true),
        Change::SetOverwrite => (tf("s_gs_ueberschreiben_frage", &[("0", Arg::from(pending.from.as_str()))]), true),
        Change::Clean | Change::Adopt => return None,
    };
    let frame = egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.gold)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin::same(10));
    let answer = frame
        .show(ui, |ui| {
            ui.set_max_width(widgets::READ_WIDTH);
            ask(ui, scheme, &text, danger)
        })
        .inner;
    ui.add_space(GAP);
    answer
}

/// The card of the axis: its name and device on top, the large curve at the left, the presets, the sliders and the
/// sensitivity of the functions on it at the right (one above the other where the card is narrow).
fn axis_card(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look, block: &DeviceBlock, axis: &str) {
    // The sliders first: the picture shows what they show.
    let shares = shares_of(state, block, axis);
    let sensitivities = sensitivity_keys(look, block, axis);
    let plotted = plotted_sensitivity(state, &sensitivities);
    let values = Values {
        deadzone: Some(SHARE.value(shares.0)),
        saturation: Some(SHARE.value(shares.1)),
        exponent: Some(plotted.exponent),
        curve: Vec::new(),
    };
    widgets::card(ui, scheme, |ui| {
        let title =
            tf("s_ac_achse_titel", &[("0", Arg::from(axis)), ("1", Arg::from(device_label(look, block).as_str()))]);
        widgets::card_title(ui, scheme, &title);
        ui.add_space(GAP / 2.0);
        let room = ui.available_width();
        let beside = room >= PLOT_BESIDE;
        let side =
            if beside { (room * PLOT_SHARE).clamp(PLOT_LEAST, PLOT_MOST) } else { room.clamp(PLOT_LEAST, PLOT_MOST) };
        // Whole points: the picture is placed on the pixels, its curve must be drawn on the same side.
        state.plot_side = side.round();
        if beside {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(state.plot_side);
                    plot_column(ui, scheme, state, block, axis, &plotted, &values);
                });
                ui.add_space(2.0 * GAP);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    controls(ui, scheme, state, look, block, axis, &sensitivities);
                });
            });
        } else {
            plot_column(ui, scheme, state, block, axis, &plotted, &values);
            ui.add_space(GAP);
            controls(ui, scheme, state, look, block, axis, &sensitivities);
        }
    });
}

/// The picture of the curve with its two buttons and whose sensitivity it draws.
fn plot_column(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    block: &DeviceBlock,
    axis: &str,
    plotted: &Plotted,
    values: &Values,
) {
    let side = state.plot_side;
    axes_plot::show(ui, scheme, egui::vec2(side, side), state.whole, values, None);
    ui.add_space(GAP);
    ui.set_max_width(side);
    // The buttons first: their place must not move with the length of the text below.
    ui.horizontal_wrapped(|ui| {
        // Each says on hover what it does: the range of the curve here, a window of its own there.
        let (key, hint) = if state.whole { ("s_kv_quadrant", "s_kv_quadrant_h") } else { ("s_kv_ganz", "s_kv_ganz_h") };
        let (label, larger_label) = (t(key), t("s_ac_gross"));
        let toggle = widgets::Button::new(&label, ButtonKind::Normal, scheme);
        if ui.add(toggle).on_hover_text(t(hint)).clicked() {
            state.whole = !state.whole;
        }
        let larger = widgets::Button::new(&larger_label, ButtonKind::Normal, scheme);
        if ui.add(larger).on_hover_text(t("s_ac_gross_h")).clicked() {
            state.large = Some(Large {
                title: match &plotted.function {
                    Some(name) => format!("{} {} {} · {}", t("s_ac_titel_gross"), block.name, axis, name),
                    None => format!("{} {} {}", t("s_ac_titel_gross"), block.name, axis),
                },
                values: values.clone(),
                whole: state.whole,
            });
        }
    });
    // Says whose sensitivity the curve draws, and that the player can choose when there are several.
    ui.add_space(GAP / 2.0);
    let says = match &plotted.function {
        Some(name) => tf("s_ac_zeigt", &[("0", Arg::from(name.as_str()))]),
        None => t("s_ac_zeigt_keine"),
    };
    ui.add(egui::Label::new(RichText::new(says).small()).wrap());
    if plotted.several {
        small(ui, scheme, &t("s_ac_zeigt_mehrere"));
    }
}

/// The presets, the dead zone and the saturation, the sensitivity of the functions on the axis and the menu that
/// copies the values to other axes.
fn controls(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    look: &Look,
    block: &DeviceBlock,
    axis: &str,
    sensitivities: &[(Key, Function)],
) {
    presets_block(ui, scheme, state, block, axis, sensitivities);
    ui.add_space(GAP);
    for (prop, text) in [(Prop::Deadzone, t("s_kv_totzone")), (Prop::Saturation, t("s_kv_saettigung"))] {
        let file = physical_file(block, axis, prop);
        let key = physical_key(block, axis, prop);
        // The same column of names as the sensitivity below: all sliders begin in one line.
        slider_row(ui, scheme, state, &key, file, SHARE, &text, 20, Name::Plain);
    }
    if look.numbers.contains_key(&block.ident) {
        ui.add_space(GAP);
        crate::settings::subheading(ui, scheme, &t("s_ac_kopf_empf"), &t("s_ac_lead_empf"));
        if sensitivities.is_empty() {
            small(ui, scheme, &t("s_ac_keine_funktion"));
        }
        let shown = shown_pair(state, sensitivities).map(|(key, _)| key.clone());
        for (key, function) in sensitivities {
            let name = Name::Pick { shown: shown.as_ref() == Some(key) };
            let touched = slider_row(
                ui,
                scheme,
                state,
                key,
                game_file(function),
                EXPONENT,
                &function_name(&function.axis),
                20,
                name,
            );
            if touched {
                // The curve follows the function the player is working on.
                state.shown = Some(key.clone());
            }
        }
    }
    copy_block(ui, scheme, state, look, block, axis);
}

/// Does what the player asked for with a click.
fn run(state: &mut AxesState, paths: &Paths, act: Option<Act>) {
    match act {
        Some(Act::SaveSet) => state.save_set(paths),
        Some(Act::Save) => state.save(paths),
        Some(Act::Discard) => state.sliders.clear(),
        Some(Act::Ask(pending)) => state.pending = Some(pending),
        Some(Act::Cancel) => state.pending = None,
        Some(Act::Confirm) => state.confirm(paths),
        None => {}
    }
}

/// The name of a device on a button: with its number where another connected device has the same name (two sticks
/// of one make), else the player could not tell which way a question goes.
fn device_label(look: &Look, block: &DeviceBlock) -> String {
    let idents: HashSet<&str> = look
        .summary
        .blocks
        .iter()
        .filter(|b| b.active && !b.ident.is_empty() && b.name == block.name)
        .map(|b| b.ident.as_str())
        .collect();
    if idents.len() < 2 {
        return block.name.clone();
    }
    match look.numbers.get(&block.ident) {
        Some(number) => format!("{} (js{number})", block.name),
        None => format!("{} ({})", block.name, block.ident.chars().take(8).collect::<String>()),
    }
}

/// The box with the files that lie side by side: which is read and which only lies there.
fn many_files(ui: &mut Ui, scheme: &Scheme, files: &[FileLine]) {
    let frame = egui::Frame::new()
        .fill(color(scheme.surface))
        .stroke(egui::Stroke::new(1.0, color(scheme.gold)))
        .corner_radius(control_radius(scheme))
        .inner_margin(egui::Margin::same(10));
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.add(egui::Label::new(t("s_ac_doppelt")).wrap());
        ui.add_space(GAP / 2.0);
        for (at, file) in files.iter().enumerate() {
            let word = t(if at == 0 { "s_ac_gelesen" } else { "s_ac_liegt" });
            let line = format!("{word} · {} · {}", file.changed, file.path);
            let hue = if at == 0 { scheme.accent } else { scheme.sub };
            // The whole path: the files are all called the same, they differ only in the folder in front.
            ui.add(egui::Label::new(RichText::new(line).small().color(color(hue))).wrap());
        }
    });
}

/// The slider step of a dead zone or saturation as the file has it.
fn physical_file(block: &DeviceBlock, axis: &str, prop: Prop) -> i64 {
    let values = block.axis(axis).copied().unwrap_or_default();
    SHARE.units(values.get(prop).unwrap_or_else(|| prop.default_value()))
}

/// The slider step of the sensitivity of a function as the file has it.
fn game_file(function: &Function) -> i64 {
    EXPONENT.units(function.exponent.unwrap_or(DEFAULT_EXPONENT))
}

fn physical_key(block: &DeviceBlock, axis: &str, prop: Prop) -> Key {
    Key::Physical { ident: block.ident.clone(), axis: axis.to_string(), prop }
}

/// The dead zone and saturation in steps for the sliders (what the player has set, else what the file says).
fn shares_of(state: &mut AxesState, block: &DeviceBlock, axis: &str) -> (i64, i64) {
    let mut one = |prop: Prop| state.slide(&physical_key(block, axis, prop), physical_file(block, axis, prop)).now;
    (one(Prop::Deadzone), one(Prop::Saturation))
}

/// The keys of the sensitivity sliders of the functions that lie on the axis, with their value in the file.
fn sensitivity_keys(look: &Look, block: &DeviceBlock, axis: &str) -> Vec<(Key, Function)> {
    let Some(number) = look.numbers.get(&block.ident) else { return Vec::new() };
    look.on_axis(&block.ident, axis)
        .iter()
        .map(|function| (Key::Game { number: *number, axis: function.axis.clone() }, function.clone()))
        .collect()
}

/// What the picture draws of the sensitivity.
struct Plotted {
    exponent: f64,
    /// The function the exponent belongs to; `None` where no function lies on the axis (nothing is felt then: 1).
    function: Option<String>,
    /// More than one function lies on the axis, so the player has a choice.
    several: bool,
}

/// The function the curve draws: the one touched last if it lies on the axis in view, else the first of the axis.
/// The game applies one exponent per function, so there is no "common" curve of several.
fn shown_pair<'a>(state: &AxesState, keys: &'a [(Key, Function)]) -> Option<&'a (Key, Function)> {
    state.shown.as_ref().and_then(|shown| keys.iter().find(|(key, _)| key == shown)).or_else(|| keys.first())
}

/// The sensitivity the picture shows: that of the function chosen by [`shown_pair`], at the value of its slider.
fn plotted_sensitivity(state: &mut AxesState, keys: &[(Key, Function)]) -> Plotted {
    for (key, function) in keys {
        state.slide(key, game_file(function));
    }
    let Some((key, function)) = shown_pair(state, keys) else {
        return Plotted { exponent: DEFAULT_EXPONENT, function: None, several: false };
    };
    let exponent = EXPONENT.value(state.slide(key, game_file(function)).now);
    Plotted { exponent, function: Some(function_name(&function.axis)), several: keys.len() > 1 }
}

/// The card with the list of the axes; gives the axis that was clicked.
fn axes_card(ui: &mut Ui, scheme: &Scheme, look: &Look, block: &DeviceBlock, chosen: &str) -> Option<String> {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_ac_kopf_achsen"));
        axis_list(ui, scheme, look, block, chosen)
    })
    .inner
}

/// The list of the axes of the device; gives the axis that was clicked.
fn axis_list(ui: &mut Ui, scheme: &Scheme, look: &Look, block: &DeviceBlock, chosen: &str) -> Option<String> {
    let mut picked = None;
    for axis in axes::AXES.iter().copied().filter(|axis| block.axis(axis).is_some()) {
        let values = block.axis(axis).copied().unwrap_or_default();
        let functions = look.on_axis(&block.ident, axis);
        let mut row = AxisRow {
            axis,
            chosen: axis == chosen,
            values: format!(
                "{} {} · {} {}",
                t("s_kv_totzone"),
                axes::two_decimals(values.deadzone),
                t("s_kv_saettigung"),
                axes::two_decimals(values.saturation)
            ),
            function: function_label(functions),
            clash: block.multiple.iter().any(|known| known == axis),
        };
        if axis_row(ui, scheme, &mut row, &block.ident) {
            picked = Some(axis.to_string());
        }
    }
    picked
}

struct AxisRow<'a> {
    axis: &'a str,
    chosen: bool,
    values: String,
    function: String,
    /// Entries that contradict each other stand in the file for this axis.
    clash: bool,
}

/// One line of the axes; true when it was clicked.
fn axis_row(ui: &mut Ui, scheme: &Scheme, row: &mut AxisRow, salt: &str) -> bool {
    let radius = control_radius(scheme);
    // The text of the chosen line stays in the colour of the text: the accent on its dark ground is below 4.5 to 1.
    let hue = |_chosen: bool| scheme.fg;
    let inner = egui::Frame::new()
        .fill(color(if row.chosen { scheme.accent_dark } else { scheme.field }))
        .stroke(egui::Stroke::new(1.0, color(if row.chosen { scheme.accent } else { scheme.line })))
        .corner_radius(radius)
        .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let gap = ui.spacing().item_spacing.x;
            let name_width = crate::joysticks_page::text_width(ui, "slider1", egui::TextStyle::Body) + 12.0;
            let side = (ui.available_width() * 0.28).max(60.0);
            let middle = (ui.available_width() - name_width - side - 2.0 * gap).max(60.0);
            ui.horizontal_top(|ui| {
                crate::joysticks_page::cell(ui, name_width, egui::Align::Min, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(row.axis).strong().color(color(hue(row.chosen))))
                            .selectable(false),
                    );
                });
                crate::joysticks_page::cell(ui, middle, egui::Align::Min, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&row.values).small().color(color(scheme.sub)))
                            .wrap()
                            .selectable(false),
                    );
                });
                crate::joysticks_page::cell(ui, side, egui::Align::Max, |ui| {
                    if row.clash
                        && let Some(image) = icons::image("triangle-alert", scheme.gold, 14.0)
                    {
                        ui.add(image);
                    }
                    if !row.function.is_empty() {
                        let tone = scheme.sub;
                        ui.add(
                            egui::Label::new(RichText::new(&row.function).small().color(color(tone)))
                                .wrap()
                                .selectable(false),
                        );
                    }
                });
            });
        });
    let rect = inner.response.rect;
    let spoken = [row.axis, row.values.as_str(), row.function.as_str()]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let spoken = if row.clash { format!("{spoken}, {}", t("s_ac_widerspruch")) } else { spoken };
    let id = ui.make_persistent_id(("axes-row", salt, row.axis));
    let response = ui.interact(rect, id, egui::Sense::click());
    let chosen = row.chosen;
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, chosen, &spoken));
    widgets::focus_ring(ui, &response, scheme, radius);
    if response.hovered() && !row.chosen {
        ui.painter().rect_stroke(rect, radius, egui::Stroke::new(1.0, color(scheme.accent)), egui::StrokeKind::Inside);
    }
    if row.clash {
        response.clone().on_hover_text(t("s_ac_widerspruch"));
    }
    ui.add_space(2.0);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// What the player asked for with a click on the page.
enum Act {
    /// Write what the sliders say into the file.
    Save,
    /// Throw away what the sliders say, back to the file.
    Discard,
    /// A question is put (the change waits for the yes).
    Ask(Pending),
    /// The question is taken back.
    Cancel,
    /// The yes: the change under the question is made.
    Confirm,
    /// Save the state as a device set under the name in the field.
    SaveSet,
}

/// What kind of change waits for the yes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// Make the axes of the second device like those of the first.
    Align,
    /// Swap the numbers of two devices.
    Swap,
    /// Take the dead blocks out of the file.
    Clean,
    /// Write the old values of an overtaken block onto the id that counts now.
    Adopt,
    /// Apply the device set whose name is in `from`.
    SetApply,
    /// Delete the device set whose name is in `from`.
    SetDelete,
    /// Save the state over the set whose name is in `from`.
    SetOverwrite,
}

/// A change that waits for the yes of the player, with the names it asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    change: Change,
    from: String,
    to: String,
    from_name: String,
    to_name: String,
}

/// The question under a button and its two answers.
fn ask(ui: &mut Ui, scheme: &Scheme, text: &str, danger: bool) -> Option<Act> {
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(GAP / 2.0);
    let mut answer = None;
    ui.horizontal_wrapped(|ui| {
        if ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked() {
            answer = Some(Act::Cancel);
        }
        // What cannot be taken back is red from the start.
        let kind = if danger { ButtonKind::Danger } else { ButtonKind::Strong };
        if ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), kind, scheme)).clicked() {
            answer = Some(Act::Confirm);
        }
    });
    answer
}

/// What the menu of the device sets holds: the saved sets, each with "apply" and "delete", and below them the name
/// and the button for a new set. The questions stand under the tabs ([`question`]).
fn sets_body(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look) -> Option<Act> {
    block_title(ui, &t("s_gs_titel"));
    small(ui, scheme, &t("s_gs_lead"));
    ui.add_space(GAP / 2.0);
    let mut act = None;
    let quiet = look.quiet.as_deref();
    if look.sets.is_empty() {
        small(ui, scheme, &t("s_gs_keine"));
    }
    for set in &look.sets {
        let (settings, _) = device_sets::plan(set, &look.summary.blocks, &look.summary.game);
        let frame = egui::Frame::new()
            .fill(color(scheme.surface))
            .stroke(egui::Stroke::new(1.0, color(scheme.line)))
            .corner_radius(control_radius(scheme))
            .inner_margin(egui::Margin::same(8));
        frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            block_title(ui, &set.name);
            let devices = if set.devices.len() == 1 {
                t("s_gs_geraete_eins")
            } else {
                tf("s_gs_geraete", &[("0", Arg::from(set.devices.len() as i64))])
            };
            small(ui, scheme, &format!("{devices} · {}", tf("s_gs_stand", &[("0", Arg::from(set.stand.as_str()))])));
            ui.add_space(GAP / 2.0);
            let apply = Pending {
                change: Change::SetApply,
                from: set.name.clone(),
                to: String::new(),
                from_name: String::new(),
                to_name: String::new(),
            };
            let delete = Pending { change: Change::SetDelete, ..apply.clone() };
            let nothing = t("s_gs_f_nichts_zu_tun");
            let reason = quiet.or_else(|| settings.is_empty().then_some(nothing.as_str()));
            ui.horizontal_wrapped(|ui| {
                // Every set has the same two buttons: a screen reader hears whose they are.
                let label = t("s_gs_anwenden");
                let spoken = format!("{label}: {}", set.name);
                let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).spoken(&spoken);
                if ui.add(button.blocked(reason)).clicked() {
                    act = Some(Act::Ask(apply.clone()));
                }
                let label = t("s_gs_loeschen");
                let spoken = format!("{label}: {}", set.name);
                if ui.add(widgets::Button::new(&label, ButtonKind::Normal, scheme).spoken(&spoken)).clicked() {
                    act = Some(Act::Ask(delete.clone()));
                }
            });
        });
        ui.add_space(GAP / 2.0);
    }
    // A new set: the name and the button.
    ui.add_space(GAP / 2.0);
    ui.horizontal_wrapped(|ui| {
        let width = (ui.available_width() * 0.5).clamp(120.0, 220.0);
        widgets::clearable_field(ui, &mut state.set_name, &t("s_pl_satzname"), scheme, width);
        let label = t("s_gs_speichern");
        if ui.add(widgets::Button::new(&label, ButtonKind::Normal, scheme)).clicked() {
            act = Some(Act::SaveSet);
        }
    });
    if let Some((name, why)) = &state.set_refusal
        && *name == state.set_name
    {
        ui.add(egui::Label::new(RichText::new(why).small().color(color(scheme.red_text()))).wrap().selectable(false));
    }
    act
}

/// A name as the program keeps it: without the blanks round it.
fn py_strip_name(name: &str) -> String {
    versekit_core::text::py_strip(name).to_string()
}

/// The block at the very end, closed at first and sober on purpose: old entries of earlier ids. It is information,
/// not a task: in gold at the top with buttons under it it reads as an error that has to be clicked away, and whoever
/// presses all buttons overwrites working values with old ones that contradict each other. So: below, closed, a
/// calm title, and the first sentence says that nothing has to be done.
fn leftovers(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look) -> Option<Act> {
    if look.summary.adoptable.is_empty() && look.dead == 0 {
        return None;
    }
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| leftovers_body(ui, scheme, state, look)).inner
}

/// What the card of the old entries holds: the head that opens and closes it, and below it, while it is open, the
/// clearing and the carrying over.
fn leftovers_body(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look) -> Option<Act> {
    let cases = &look.summary.adoptable;
    let count = if look.dead == 1 {
        t("s_ac_befund_kopf_eins")
    } else {
        tf("s_ac_befund_kopf", &[("0", Arg::from(i64::try_from(look.dead).unwrap_or(i64::MAX)))])
    };
    let head = ui.horizontal(|ui| {
        let name = if state.leftovers_open { "chevron-down" } else { "chevron-right" };
        if let Some(image) = icons::image(name, scheme.sub, 14.0) {
            ui.add(image);
        }
        ui.add(egui::Label::new(widgets::bold(ui, t("s_ac_befund"), egui::TextStyle::Body)).selectable(false));
        ui.add(egui::Label::new(RichText::new(&count).small().color(color(scheme.sub))).wrap().selectable(false));
    });
    let title = format!("{}, {count}", t("s_ac_befund"));
    let response = ui.interact(head.response.rect, ui.make_persistent_id("axes-leftovers"), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    if response.hovered() {
        ui.painter().rect_stroke(
            head.response.rect.expand(2.0),
            control_radius(scheme),
            egui::Stroke::new(1.0, color(scheme.accent)),
            egui::StrokeKind::Outside,
        );
    }
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        state.leftovers_open = !state.leftovers_open;
    }
    if !state.leftovers_open {
        return None;
    }
    let mut act = None;
    let quiet = look.quiet.as_deref();
    ui.add_space(GAP / 2.0);
    ui.add(egui::Label::new(t("s_ac_befund_ruhig")).wrap());
    small(ui, scheme, &t("s_ac_befund_lead"));
    // The harmless button (clear) before the dangerous one (carry over).
    if look.dead > 0 {
        ui.add_space(GAP / 2.0);
        small(ui, scheme, &t("s_ac_aufraeumen_lead"));
        ui.add_space(GAP / 2.0);
        let wanted = Pending {
            change: Change::Clean,
            from: String::new(),
            to: String::new(),
            from_name: String::new(),
            to_name: String::new(),
        };
        if state.pending.as_ref() == Some(&wanted) {
            let key = if look.dead == 1 { "s_ac_aufraeumen_frage_eins" } else { "s_ac_aufraeumen_frage" };
            let text = tf(key, &[("0", Arg::from(i64::try_from(look.dead).unwrap_or(i64::MAX)))]);
            if let Some(next) = ask(ui, scheme, &text, true) {
                act = Some(next);
            }
        } else if ui
            .add(widgets::Button::new(&t("s_ac_aufraeumen"), ButtonKind::Danger, scheme).blocked(quiet))
            .clicked()
        {
            act = Some(Act::Ask(wanted));
        }
    }
    if !cases.is_empty() {
        ui.add_space(GAP / 2.0);
        small(ui, scheme, &t("s_ac_befund_warnung"));
        for case in cases {
            ui.add_space(GAP / 2.0);
            let frame = egui::Frame::new()
                .fill(color(scheme.surface))
                .corner_radius(control_radius(scheme))
                .inner_margin(egui::Margin::same(8));
            frame.show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                block_title(ui, &case.name);
                // By value, not by axis: on every axis of a stick the same value usually stands.
                type Bundle<'a> = ((Prop, f64, Option<f64>), Vec<&'a str>);
                let mut bundles: Vec<Bundle> = Vec::new();
                for (axis, prop, old, now) in &case.values {
                    match bundles.iter_mut().find(|(known, _)| *known == (*prop, *old, *now)) {
                        Some((_, list)) => list.push(axis),
                        None => bundles.push(((*prop, *old, *now), vec![axis.as_str()])),
                    }
                }
                for ((prop, old, now), list) in &bundles {
                    let word = t(if *prop == Prop::Deadzone { "s_kv_totzone" } else { "s_kv_saettigung" });
                    let now = match now {
                        Some(value) => format!("{} {}", t("s_ac_jetzt"), axes::two_decimals(Some(*value))),
                        None => t("s_ac_fehlt"),
                    };
                    let line = format!("{word}: {} {}, {now}", t("s_ac_war"), axes::two_decimals(Some(*old)));
                    ui.add(egui::Label::new(line).wrap());
                    small(ui, scheme, &list.join(", "));
                }
            });
            let wanted = Pending {
                change: Change::Adopt,
                from: case.old.ident.clone(),
                to: case.new.ident.clone(),
                from_name: case.name.clone(),
                to_name: case.name.clone(),
            };
            ui.add_space(GAP / 2.0);
            if case.ambiguous {
                // A wrong stick with the old values is worse than none: only shown, never written.
                small(ui, scheme, &t("s_ac_name_mehrfach"));
            } else if state.pending.as_ref() == Some(&wanted) {
                let text = tf("s_ac_uebernehmen_frage", &[("0", Arg::from(case.name.as_str()))]);
                if let Some(next) = ask(ui, scheme, &text, false) {
                    act = Some(next);
                }
            } else {
                let label = t("s_ac_uebernehmen");
                let spoken = format!("{label}: {}", case.name);
                let button = widgets::Button::new(&label, ButtonKind::Normal, scheme).spoken(&spoken).blocked(quiet);
                if ui.add(button).clicked() {
                    act = Some(Act::Ask(wanted));
                }
            }
        }
    }
    act
}

/// What the name at the start of a slider row does.
#[derive(Clone, Copy)]
enum Name {
    Plain,
    /// The name of a function: a click on it makes the curve show this function. `shown` marks the one it shows.
    Pick {
        shown: bool,
    },
}

/// A row: the name, the slider and the value at its right. The slider only changes the state of the page. Gives
/// `true` when the player worked on the row (moved or clicked the slider, or clicked the name of a function).
#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    key: &Key,
    file: i64,
    scale: Scale,
    name: &str,
    name_chars: usize,
    kind: Name,
) -> bool {
    let mut now = state.slide(key, file).now;
    let mut touched = false;
    ui.horizontal(|ui| {
        let width = crate::joysticks_page::text_width(ui, &"x".repeat(name_chars), egui::TextStyle::Small);
        crate::joysticks_page::cell(ui, width.min(ui.available_width() * 0.4), egui::Align::Min, |ui| match kind {
            Name::Plain => {
                ui.add(egui::Label::new(RichText::new(name).small()).wrap());
            }
            Name::Pick { shown } => {
                // The curve shows this function: its name is bold with a line of the accent under it, the others are
                // dim. The colour alone would not tell them apart for everyone.
                let text = if shown {
                    widgets::bold(ui, name, egui::TextStyle::Small).color(color(scheme.fg))
                } else {
                    RichText::new(name).small().color(color(scheme.sub))
                };
                let label = ui
                    .add(egui::Label::new(text).wrap().selectable(false).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(t("s_ac_zeigen_tip"));
                let enabled = label.enabled();
                let spoken = tf("s_ac_zeigen_name", &[("0", Arg::from(name))]);
                label.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, enabled, shown, &spoken));
                let accent = color(scheme.accent);
                if shown {
                    let line = [label.rect.left_bottom(), label.rect.right_bottom()];
                    ui.painter().line_segment(line, egui::Stroke::new(2.0, accent));
                }
                if label.hovered() {
                    // A name that can be clicked shows it under the mouse, as the lines of the axes do.
                    let frame = label.rect.expand(2.0);
                    ui.painter().rect_stroke(
                        frame,
                        egui::CornerRadius::same(4),
                        egui::Stroke::new(1.0, accent),
                        egui::StrokeKind::Outside,
                    );
                }
                widgets::focus_ring(ui, &label, scheme, egui::CornerRadius::same(4));
                touched |= label.clicked();
            }
        });
        let shown = axes::two_decimals(Some(scale.value(now)));
        let room = (ui.available_width() - 56.0).clamp(80.0, SLIDER_MOST);
        let response = ui.add(widgets::Slider::new(&mut now, scale.from, scale.to, scheme).width(room));
        let enabled = response.enabled();
        response.widget_info(|| egui::WidgetInfo::slider(enabled, scale.value(now), name));
        touched |= response.changed() || response.dragged() || response.gained_focus() || response.clicked();
        ui.add(egui::Label::new(RichText::new(shown).small().color(color(scheme.accent))).selectable(false));
    });
    state.slide(key, file).now = now;
    if touched {
        // The curve is drawn from this state before the rows: it must follow in the next frame, not at the next event.
        ui.ctx().request_repaint();
    }
    touched
}

/// Puts values on the sliders of an axis as if the player had dragged them: the dead zone and the saturation of the
/// axis and, where there is a value, the sensitivity of every function that lies on it. Nothing is written.
fn put_on_axis(
    state: &mut AxesState,
    block: &DeviceBlock,
    axis: &str,
    functions: &[(Key, Function)],
    (deadzone, saturation, exponent): (i64, i64, Option<i64>),
) {
    for (prop, now) in [(Prop::Deadzone, deadzone), (Prop::Saturation, saturation)] {
        state.slide(&physical_key(block, axis, prop), physical_file(block, axis, prop)).now = now;
    }
    if let Some(now) = exponent {
        for (key, function) in functions {
            state.slide(key, game_file(function)).now = now;
        }
    }
}

/// The presets as buttons: a click puts the values on the sliders of the axis (unsaved, like dragging). The preset
/// that matches the sliders right now is marked.
fn presets_block(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    block: &DeviceBlock,
    axis: &str,
    functions: &[(Key, Function)],
) {
    crate::settings::subheading(ui, scheme, &t("s_ac_kopf_vor"), &t("s_ac_lead_vor"));
    let (deadzone, saturation) = shares_of(state, block, axis);
    let exponents: Vec<i64> =
        functions.iter().map(|(key, function)| state.slide(key, game_file(function)).now).collect();
    let mut picked: Option<axes::Preset> = None;
    ui.horizontal_wrapped(|ui| {
        for preset in axes::PRESETS {
            let (dz, sat, exp) =
                (SHARE.units(preset.deadzone), SHARE.units(preset.saturation), EXPONENT.units(preset.exponent));
            let on = deadzone == dz && saturation == sat && exponents.iter().all(|now| *now == exp);
            let name = t(&format!("s_ac_vor_{}", preset.key));
            let kind = if on { ButtonKind::Strong } else { ButtonKind::Normal };
            let tip = format!(
                "{}\n{}",
                t(&format!("s_ac_vor_{}_t", preset.key)),
                tf(
                    "s_ac_vor_werte",
                    &[
                        ("0", Arg::from(axes::two_decimals(Some(preset.deadzone)).as_str())),
                        ("1", Arg::from(axes::two_decimals(Some(preset.saturation)).as_str())),
                        ("2", Arg::from(axes::two_decimals(Some(preset.exponent)).as_str())),
                    ]
                )
            );
            let response = ui.add(widgets::Button::new(&name, kind, scheme).wrapping()).on_hover_text(tip);
            let enabled = response.enabled();
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, &name));
            if response.clicked() {
                picked = Some(preset);
            }
        }
    });
    if let Some(preset) = picked {
        let exponent = (!functions.is_empty()).then(|| EXPONENT.units(preset.exponent));
        let values = (SHARE.units(preset.deadzone), SHARE.units(preset.saturation), exponent);
        put_on_axis(state, block, axis, functions, values);
        state.notice = Some(Said::note(tf(
            "s_ac_vor_gesetzt",
            &[("0", Arg::from(t(&format!("s_ac_vor_{}", preset.key)).as_str()))],
        )));
    }
}

/// The block "Auf andere Achsen übertragen": ticks for the other axes of the device and for all of them, and the
/// button that puts the values of this axis on the sliders of the ticked ones.
fn copy_block(ui: &mut Ui, scheme: &Scheme, state: &mut AxesState, look: &Look, block: &DeviceBlock, axis: &str) {
    let others: Vec<&str> =
        axes::AXES.iter().copied().filter(|other| *other != axis && block.axis(other).is_some()).collect();
    if others.is_empty() {
        return;
    }
    // The ticks belong to the axis they were made for: on another axis or device they would copy somewhere unseen.
    if state.copy_for.0 != block.ident || state.copy_for.1 != axis {
        state.copy_for = (block.ident.clone(), axis.to_string());
        state.copy_targets.clear();
    }
    ui.add_space(GAP);
    let word = t("s_ac_kopf_uebertragen");
    menu(ui, scheme, COPY_MENU, &word, None, |menu| {
        if copy_body(menu, scheme, state, look, block, axis, &others) {
            menu.close();
        }
    });
}

/// What the menu "Auf andere Achsen übertragen" holds: the ticks and the button; true when the values were copied.
fn copy_body(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut AxesState,
    look: &Look,
    block: &DeviceBlock,
    axis: &str,
    others: &[&str],
) -> bool {
    small(ui, scheme, &t("s_ac_lead_uebertragen"));
    ui.add_space(GAP / 2.0);
    let mut all = others.iter().all(|other| state.copy_targets.contains(*other));
    if widgets::switch_row(ui, scheme, &t("s_ac_alle_achsen"), &mut all).changed() {
        if all {
            state.copy_targets.extend(others.iter().map(|other| other.to_string()));
        } else {
            state.copy_targets.clear();
        }
    }
    for other in others {
        let mut on = state.copy_targets.contains(*other);
        let functions = function_label(look.on_axis(&block.ident, other));
        let label = if functions.is_empty() { other.to_string() } else { format!("{other} · {functions}") };
        if widgets::switch_row(ui, scheme, &label, &mut on).changed() {
            if on {
                state.copy_targets.insert(other.to_string());
            } else {
                state.copy_targets.remove(*other);
            }
        }
    }
    ui.add_space(GAP / 2.0);
    let nothing = t("s_ac_uebertragen_keine");
    let reason = state.copy_targets.is_empty().then_some(nothing.as_str());
    if ui
        .add(widgets::Button::new(&t("s_ac_uebertragen"), ButtonKind::Normal, scheme).wrapping().blocked(reason))
        .clicked()
    {
        let (deadzone, saturation) = shares_of(state, block, axis);
        let own = sensitivity_keys(look, block, axis);
        let exponent = shown_pair(state, &own).map(|(key, function)| state.slide(key, game_file(function)).now);
        let mut count = 0_i64;
        for other in others {
            if state.copy_targets.contains(*other) {
                let functions = sensitivity_keys(look, block, other);
                put_on_axis(state, block, other, &functions, (deadzone, saturation, exponent));
                count += 1;
            }
        }
        state.copy_targets.clear();
        let key = if count == 1 { "s_ac_uebertragen_fertig_eins" } else { "s_ac_uebertragen_fertig" };
        state.notice = Some(Said::note(tf(key, &[("0", Arg::from(count))])));
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, Rect};
    use versekit_core::language::{self, Choice};
    use versekit_core::paths::{ENV_INSTALL_DIR, Host};
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    type Texts = Vec<(String, Pos2)>;

    const LEFT: &str = "03F33344-0000-0000-0000-504944564944";
    const RIGHT: &str = "43F43344-0000-0000-0000-504944564944";
    const OLD: &str = "83F33344-0000-0000-0000-504944564944";

    /// A file like the one of the game: the blocks stand under `ActionProfiles`.
    fn file() -> String {
        format!(
            r#"<ActionMaps>
 <ActionProfiles version="1" optionsVersion="2" rebindVersion="2" profileName="default">
  <deviceoptions name="LEFT VPC Stick  {{{LEFT}}}">
   <option input="x" deadzone="0.152"/>
   <option input="y" deadzone="0.098999992"/>
   <option input="x" saturation="0.84799999"/>
   <option input="x" saturation="0.84799999"/>
   <option input="rotz" saturation="0.7"/>
  </deviceoptions>
  <deviceoptions name="RIGHT VPC Stick  {{{RIGHT}}}">
   <option input="x" deadzone="0.1"/>
   <option input="x" deadzone="0.3"/>
  </deviceoptions>
  <deviceoptions name="LEFT VPC Stick  {{{OLD}}}">
   <option input="x" deadzone="0.2"/>
  </deviceoptions>
  <options type="joystick" instance="1" Product="LEFT VPC Stick  {{{LEFT}}}">
   <flight_move_pitch exponent="1.5"/>
   <flight_move_strafe_vertical exponent="1.5"/>
   <flight_move_roll exponent="0.5"/>
  </options>
  <options type="joystick" instance="2" Product="RIGHT VPC Stick  {{{RIGHT}}}"/>
  <actionmap name="spaceship_movement">
   <action name="v_pitch"><rebind input="js1_y"/></action>
   <action name="v_strafe_vertical"><rebind input="js1_y"/></action>
   <action name="v_roll"><rebind input="js1_x"/><rebind input="js1_rotz"/></action>
   <action name="v_yaw"><rebind input="js2_x"/></action>
  </actionmap>
 </ActionProfiles>
</ActionMaps>
"#
        )
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-axespage-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A made-up installation under a throwaway home, the game quiet for two hours.
    fn world(name: &str, maps_text: Option<&str>) -> (Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(
            format!("{game}/Game.log"),
            format!(
                "Connected joystick0:  LEFT VPC Stick  {{{LEFT}}}\nConnected joystick1:  RIGHT VPC Stick  {{{RIGHT}}}\n"
            ),
        )
        .expect("log");
        if let Some(text) = maps_text {
            std::fs::write(format!("{profile}/actionmaps.xml"), text).expect("maps");
        }
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::File::options()
            .write(true)
            .open(format!("{game}/Game.log"))
            .and_then(|f| f.set_modified(old))
            .expect("age");
        let vars = [
            ("SC_BP_HOME", home.as_str()),
            ("HOME", home.as_str()),
            (ENV_INSTALL_DIR, game.as_str()),
            (versekit_core::game_start::ENV_NO_PROCESS, "1"),
        ];
        (Paths::new(Host::new(Flavor::Posix, &vars, &home)), home)
    }

    struct Bench {
        ctx: egui::Context,
        state: AxesState,
        paths: Paths,
        home: String,
        time: f64,
        rects: Vec<(String, Rect)>,
        nodes: Vec<(egui::Role, String)>,
        /// The place of every named element, for the tests that work a slider or a switch with the mouse.
        places: Vec<(egui::Role, String, Rect)>,
        /// The points of the curve as drawn (the accent line with the points of the curve), empty where none is.
        curve: Vec<Pos2>,
        /// The room the page is drawn in.
        size: egui::Vec2,
    }

    impl Bench {
        fn new(name: &str, maps_text: Option<&str>) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let (paths, home) = world(name, maps_text);
            Bench {
                ctx,
                state: AxesState::default(),
                paths,
                home,
                time: 10.0,
                rects: Vec::new(),
                nodes: Vec::new(),
                places: Vec::new(),
                curve: Vec::new(),
                size: egui::vec2(1280.0, 4000.0),
            }
        }

        fn frame(&mut self) -> Texts {
            self.frame_with(Vec::new())
        }

        fn frame_with(&mut self, events: Vec<egui::Event>) -> Texts {
            let scheme = theme::current();
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            // The page and its foot below it, as the window draws them.
            let mut output = self.ctx.run_ui(input, |ui| {
                show_page(ui, scheme, state, paths);
                show_foot(ui, scheme, state, paths);
            });
            output.textures_delta.clear();
            let mut texts = Vec::new();
            let mut rects = Vec::new();
            let mut curve = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts, rects: &mut Vec<(String, Rect)>, curve: &mut Vec<Pos2>) {
                match shape {
                    egui::Shape::Text(text) => {
                        into.push((text.galley.text().to_string(), text.pos));
                        rects.push((text.galley.text().to_string(), Rect::from_min_size(text.pos, text.galley.size())));
                    }
                    // The curve is the one line with the points of the drawing and the stroke of 2 points.
                    egui::Shape::Path(path)
                        if path.points.len() == axes_plot::SUPPORT_POINTS + 1 && path.stroke.width == 2.0 =>
                    {
                        *curve = path.points.clone();
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into, rects, curve)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts, &mut rects, &mut curve));
            self.rects = rects;
            self.curve = curve;
            let mut places = Vec::new();
            self.nodes = output
                .platform_output
                .accesskit_update
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .map(|(_, node)| {
                            let name = node.label().unwrap_or_default().to_string();
                            if let Some(b) = node.bounds() {
                                let corner = |x: f64, y: f64| Pos2::new(x as f32, y as f32);
                                places.push((
                                    node.role(),
                                    name.clone(),
                                    Rect::from_min_max(corner(b.x0, b.y0), corner(b.x1, b.y1)),
                                ));
                            }
                            (node.role(), name)
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.places = places;
            texts
        }

        fn settle(&mut self) -> Texts {
            let mut texts = Vec::new();
            for _ in 0..200 {
                texts = self.frame();
                if self.state.job.is_none() && self.state.look.is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for _ in 0..3 {
                texts = self.frame();
            }
            texts
        }

        /// Opens the menu of the page that holds what `found` looks for, where it is not on the screen yet: each
        /// thing of the page is at most one click on a menu away. True when it is on the screen then.
        fn reveal(&mut self, found: impl Fn(&Bench) -> bool) -> bool {
            self.settle();
            if found(self) {
                return true;
            }
            // From a page with no menu open: an open one could lie over the button of the next.
            for menu in [DEVICE_MENU, SET_MENU, COPY_MENU] {
                egui::Popup::close_id(&self.ctx, egui::Id::unique(menu));
            }
            self.settle();
            for menu in [t("s_ac_menue_satz"), t("s_ac_menue_geraet"), t("s_ac_kopf_uebertragen")] {
                let Some(at) = self.rects.iter().find(|(text, _)| *text == menu).map(|(_, rect)| rect.center()) else {
                    continue;
                };
                self.press_at(at);
                if found(self) {
                    return true;
                }
                // Not in this menu: it is closed again.
                self.press_at(at);
            }
            false
        }

        /// Clicks the first text that is `label`, opening the menu it stands in where it is in one.
        fn click(&mut self, label: &str) -> Texts {
            self.reveal(|bench| bench.rects.iter().any(|(text, _)| text == label));
            let texts = self.settle();
            let at = self
                .rects
                .iter()
                .find(|(text, _)| text == label)
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame_with(vec![egui::Event::PointerMoved(at)]);
            self.frame_with(vec![button(true)]);
            self.frame_with(vec![button(false)]);
            self.settle()
        }

        /// Presses and lets go at a point of the screen, like the mouse.
        fn press_at(&mut self, at: Pos2) -> Texts {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame_with(vec![egui::Event::PointerMoved(at)]);
            self.frame_with(vec![button(true)]);
            self.frame_with(vec![button(false)]);
            self.settle()
        }

        /// The place of the named element of that kind.
        fn place(&mut self, role: egui::Role, name: &str) -> Rect {
            self.reveal(|bench| bench.places.iter().any(|(r, n, _)| *r == role && n == name));
            let texts = self.settle();
            self.places
                .iter()
                .find(|(r, n, _)| *r == role && n == name)
                .map(|(_, _, rect)| *rect)
                .unwrap_or_else(|| panic!("no {role:?} {name} on the screen: {texts:?} {:?}", self.nodes))
        }

        /// Clicks the middle of the named element of that kind (a switch has no text to click).
        fn click_node(&mut self, role: egui::Role, name: &str) -> Texts {
            let at = self.place(role, name).center();
            self.press_at(at)
        }

        /// Puts the slider with this name to `units` with the mouse: one press at the place of that value.
        fn set_slider(&mut self, name: &str, units: i64, scale: Scale) -> Texts {
            let rect = self.place(egui::Role::Slider, name);
            let x = rect.min.x + crate::widgets::slider_x(rect.width(), scale.from, scale.to, units);
            self.press_at(Pos2::new(x, rect.center().y))
        }

        /// The time goes on (the next look is due), then settle.
        fn later_settle(&mut self) -> Texts {
            self.time += 4.0;
            self.settle()
        }

        fn picture(&self) -> String {
            // The icons are pictures too, without a name: the curve is the one that has one.
            self.nodes
                .iter()
                .find(|(role, name)| *role == egui::Role::Image && !name.is_empty())
                .map(|(_, name)| name.clone())
                .unwrap_or_default()
        }
    }

    fn shows(texts: &Texts, text: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(text))
    }

    /// The curve as the page must draw it for these values, moved to where the page drew its first point (the
    /// picture does not stand at the corner of the screen).
    fn expected_curve(drawn: &[Pos2], side: f32, deadzone: f64, saturation: f64, exponent: f64) -> Vec<Pos2> {
        let values = Values {
            deadzone: Some(deadzone),
            saturation: Some(saturation),
            exponent: Some(exponent),
            curve: Vec::new(),
        };
        let area = axes_plot::area(egui::vec2(side, side));
        let line = axes_plot::line(&area, false, &values);
        let shift = drawn.first().map_or(egui::Vec2::ZERO, |first| *first - line[0]);
        line.into_iter().map(|p| p + shift).collect()
    }

    /// Does the page draw exactly the curve of these values (to a thousandth of a point)?
    fn draws(bench: &Bench, deadzone: f64, saturation: f64, exponent: f64) -> bool {
        let want = expected_curve(&bench.curve, bench.state.plot_side, deadzone, saturation, exponent);
        bench.curve.len() == want.len() && bench.curve.iter().zip(&want).all(|(a, b)| (*a - *b).length() < 1e-3)
    }

    fn setup() -> std::sync::MutexGuard<'static, ()> {
        let guard = crate::language_lock();
        theme::set_current("original");
        language::set_thread_language(Choice::De);
        guard
    }

    /// Everything the page can do: on the page, or behind one click on one of its menus.
    #[test]
    fn every_function_of_the_page_is_on_it_or_one_menu_away() {
        let _guard = setup();
        let mut bench = Bench::new("erreichbar", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        bench.state.take_notice();
        bench.click("y");
        let mut wanted: Vec<String> = [
            "s_kv_ganz",
            "s_ac_gross",
            "s_ac_vor_standard",
            "s_ac_vor_balanced",
            "s_ac_vor_fine",
            "s_ac_vor_direct",
            "s_ac_vor_pedals",
            "s_ac_speichern",
            "s_ac_verwerfen",
            "s_gs_anwenden",
            "s_gs_loeschen",
            "s_gs_speichern",
            "s_ac_uebertragen",
            "s_ac_befund",
        ]
        .iter()
        .map(|key| t(key))
        .collect();
        wanted.push(tf("s_ac_angleichen", &[("0", Arg::from("RIGHT VPC Stick"))]));
        wanted.push(tf("s_ac_tauschen", &[("0", Arg::from("RIGHT VPC Stick"))]));
        wanted.extend(["LEFT VPC Stick", "RIGHT VPC Stick", "x", "rotz", "mit Pedalen"].map(String::from));
        wanted.push(function_name("flight_move_pitch"));
        for label in &wanted {
            let found = bench.reveal(|bench| bench.rects.iter().any(|(text, _)| text == label));
            assert!(found, "{label} is neither on the page nor in one of its menus: {:?}", bench.settle());
        }
        // The ticks of the axes to copy to, and the sliders, by their names.
        for (role, name) in [
            (egui::Role::CheckBox, t("s_ac_alle_achsen")),
            (egui::Role::Slider, t("s_kv_totzone")),
            (egui::Role::Slider, t("s_kv_saettigung")),
        ] {
            let found = bench.reveal(|bench| bench.places.iter().any(|(r, n, _)| *r == role && *n == name));
            assert!(found, "no {role:?} {name}");
        }
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    /// The page of a window of 1920×1080 at the usual size (118 %): the page area less the side bar, the head and
    /// the foot of the window. The axis with the most on it, the old entries closed and the foot fit without scrolling.
    #[test]
    fn the_page_fits_a_window_of_1920_by_1080() {
        let _guard = setup();
        let mut bench = Bench::new("passt", Some(&file()));
        bench.size = egui::vec2(1360.0, 790.0);
        bench.click("y");
        let texts = bench.settle();
        let bottom = bench.rects.iter().map(|(_, rect)| rect.max.y).fold(0.0, f32::max);
        assert!(bottom <= bench.size.y, "the page ends at {bottom:.0}, the room at {}: {texts:?}", bench.size.y);
        // The picture is large, the foot below everything.
        assert!(bench.state.plot_side >= 360.0, "the curve only {:.0}", bench.state.plot_side);
        let save = bench.place(egui::Role::Button, &t("s_ac_speichern"));
        let old = bench.place(egui::Role::Button, &format!("{}, {}", t("s_ac_befund"), t("s_ac_befund_kopf_eins")));
        assert!(save.min.y > old.max.y, "the foot stands below the page: {save:?} {old:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_page_shows_the_live_devices_and_the_axes_with_their_values_and_what_lies_on_them() {
        let _guard = setup();
        let mut bench = Bench::new("seite", Some(&file()));
        let texts = bench.settle();
        // Both live devices are tabs; the dead block of the old id is none, and the name stands once.
        assert!(shows(&texts, "LEFT VPC Stick") && shows(&texts, "RIGHT VPC Stick"), "{texts:?}");
        assert_eq!(texts.iter().filter(|(s, _)| s == "LEFT VPC Stick").count(), 1, "{texts:?}");
        // The axes of the first device in the order of the page, with the values of the file.
        for axis in ["x", "y", "rotz"] {
            assert!(texts.iter().any(|(s, _)| s == axis), "{axis}: {texts:?}");
        }
        assert!(!texts.iter().any(|(s, _)| s == "rotx"), "an axis that is not in the file is none");
        let line = format!("{} 0.15 · {} 0.85", t("s_kv_totzone"), t("s_kv_saettigung"));
        assert!(shows(&texts, &line), "{line}: {texts:?}");
        // y: 0.098999992 is shown as 0.1 and there is no saturation.
        assert!(shows(&texts, &format!("{} 0.1 · {} —", t("s_kv_totzone"), t("s_kv_saettigung"))), "{texts:?}");
        // What lies on the axes: one function by name, two as a number.
        assert!(shows(&texts, &function_name("flight_move_roll")), "x has the roll: {texts:?}");
        assert!(shows(&texts, &tf("s_ac_mehrere_funktionen", &[("n", Arg::from(2))])), "y has two: {texts:?}");
        // The picture is told to a screen reader.
        assert!(bench.picture().contains("0.15"), "{}", bench.picture());
        // Beside the picture: the two buttons.
        assert!(shows(&texts, &t("s_kv_ganz")) && shows(&texts, &t("s_ac_gross")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_click_on_a_device_or_an_axis_chooses_it() {
        let _guard = setup();
        let mut bench = Bench::new("waehlen", Some(&file()));
        bench.settle();
        let texts = bench.click("y");
        assert_eq!(bench.state.axis, "y");
        assert!(shows(&texts, &t("s_ac_kopf_empf")), "the sensitivity of the functions on y: {texts:?}");
        assert!(
            shows(&texts, &function_name("flight_move_pitch"))
                && shows(&texts, &function_name("flight_move_strafe_vertical"))
        );
        // Both functions have 1.5: the curve shows it.
        assert!(bench.picture().contains("1.5"), "{}", bench.picture());
        let texts = bench.click("RIGHT VPC Stick");
        assert_eq!(bench.state.device, RIGHT);
        assert_eq!(bench.state.axis, "x", "the first axis of the new device");
        assert!(shows(&texts, &format!("{} 0.3", t("s_kv_totzone"))), "the last entry wins: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn nothing_to_choose_is_said_and_a_missing_file_too() {
        let _guard = setup();
        let mut bench = Bench::new("leer", None);
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_ac_keine")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
        // Only a dead device in the file: no tab, the same sentence.
        let dead = format!(
            "<ActionProfiles><deviceoptions name=\"Weg  {{{OLD}}}\"><option input=\"x\" deadzone=\"0.2\"/></deviceoptions></ActionProfiles>"
        );
        let mut bench = Bench::new("tot", Some(&dead));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_ac_keine")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_slider_changes_the_preview_and_the_mark_but_never_the_file() {
        let _guard = setup();
        let mut bench = Bench::new("regler", Some(&file()));
        let path = format!("{}/StarCitizen/LIVE/USER/Client/0/Profiles/default/actionmaps.xml", bench.home);
        let before = std::fs::read(&path).expect("file");
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_ac_geaendert")), "nothing moved: no mark: {texts:?}");
        // The player drags the dead zone of x to 0.3.
        let key = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        bench.state.slide(&key, 152).now = 300;
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_ac_geaendert")), "{texts:?}");
        assert!(bench.picture().contains("0.3"), "the picture shows the slider: {}", bench.picture());
        assert_eq!(std::fs::read(&path).expect("file"), before, "not written");
        // To another axis and back: the slider stays where it was put.
        bench.click("y");
        assert!(shows(&bench.settle(), &t("s_ac_geaendert")), "kept over the other axis");
        bench.click("x");
        assert_eq!(bench.state.slide(&key, 152).now, 300);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    fn maps_path(bench: &Bench) -> String {
        format!("{}/StarCitizen/LIVE/USER/Client/0/Profiles/default/actionmaps.xml", bench.home)
    }

    #[test]
    fn saving_writes_every_moved_slider_in_one_writing_with_one_copy_and_clears_the_marks() {
        let _guard = setup();
        let mut bench = Bench::new("speichern", Some(&file()));
        let path = maps_path(&bench);
        bench.settle();
        // Dead zone and saturation of x and the sensitivity of the roll, which lies on x.
        let dead = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        let saturation = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Saturation };
        let roll = Key::Game { number: 1, axis: "flight_move_roll".into() };
        bench.state.slide(&dead, 152).now = 300;
        bench.state.slide(&saturation, 848).now = 900;
        bench.state.slide(&roll, 10).now = 40;
        bench.settle();
        let texts = bench.click(&t("s_ac_speichern"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(after.contains("deadzone=\"0.3\""), "{after}");
        assert_eq!(after.matches("saturation=\"0.9\"").count(), 1, "the doubles are gone: {after}");
        assert!(!after.contains("0.84799999"), "{after}");
        assert!(after.contains("<flight_move_roll exponent=\"2\"/>"), "{after}");
        assert!(after.contains("<!-- ") || !after.contains("<!--"), "layout kept");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1, "one copy for the three values");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == t("s_ac_gespeichert"), "{said:?}");
        assert!(!shows(&texts, &t("s_ac_geaendert")), "no mark any more: {texts:?}");
        assert!(!bench.state.changed_any());
        // The page shows what the file says now.
        assert!(shows(&texts, &format!("{} 0.3", t("s_kv_totzone"))), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_slider_that_was_only_looked_at_is_not_written_with_the_others() {
        let _guard = setup();
        let mut bench = Bench::new("nur-angesehen", Some(&file()));
        let path = maps_path(&bench);
        bench.settle();
        // y is looked at (its sliders are drawn), then x is chosen and its dead zone moved.
        bench.click("y");
        bench.click("x");
        let dead = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        bench.state.slide(&dead, 152).now = 300;
        bench.settle();
        bench.click(&t("s_ac_speichern"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(after.contains("deadzone=\"0.3\""), "{after}");
        // The widths the game wrote stay where the player did not touch them.
        assert!(after.contains("deadzone=\"0.098999992\""), "y was not touched: {after}");
        assert_eq!(after.matches("saturation=\"0.84799999\"").count(), 2, "x saturation was not touched: {after}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn nothing_is_written_when_nothing_was_moved_and_when_the_game_runs() {
        let _guard = setup();
        let mut bench = Bench::new("sperre", Some(&file()));
        let path = maps_path(&bench);
        let before = std::fs::read(&path).expect("file");
        bench.settle();
        // Nothing moved: the button is blocked and says why on hover; a click does nothing.
        bench.click(&t("s_ac_speichern"));
        assert_eq!(std::fs::read(&path).expect("file"), before);
        assert!(versekit_core::game_files::backups(&path).is_empty());
        // Moved, but the game runs: blocked, the reason stands on the page, nothing is written.
        let dead = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        bench.state.slide(&dead, 152).now = 300;
        let log = format!("{}/StarCitizen/LIVE/Game.log", bench.home);
        std::fs::File::options()
            .write(true)
            .open(&log)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()))
            .expect("log");
        bench.time += 4.0;
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_gf_spiel")), "the reason: {texts:?}");
        bench.click(&t("s_ac_speichern"));
        assert_eq!(std::fs::read(&path).expect("file"), before, "not written while the game runs");
        assert!(bench.state.changed_any(), "and the sliders keep what the player set");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_other_stick_is_made_like_this_one_after_a_question_and_with_one_copy() {
        let _guard = setup();
        let mut bench = Bench::new("angleichen", Some(&file()));
        let path = maps_path(&bench);
        let before = std::fs::read(&path).expect("file");
        bench.settle();
        // The button names the other device; a click asks and writes nothing.
        let button = tf("s_ac_angleichen", &[("0", Arg::from("RIGHT VPC Stick"))]);
        let texts = bench.click(&button);
        let question =
            tf("s_ac_angleichen_frage", &[("0", Arg::from("LEFT VPC Stick")), ("1", Arg::from("RIGHT VPC Stick"))]);
        assert!(shows(&texts, question.lines().next().unwrap_or_default()), "{texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "asked, nothing written");
        // No: taken back.
        let texts = bench.click(&t("abbrechen"));
        assert!(!shows(&texts, question.lines().next().unwrap_or_default()), "{texts:?}");
        // Yes: written.
        bench.click(&button);
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        // The right stick has only the axis x, with the dead zone 0.1 then 0.3 (the last wins) and no saturation;
        // the left one has 0.152 and 0.848 on it: two values are written.
        assert!(after.matches("deadzone=\"0.152\"").count() >= 2, "{after}");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1, "one copy for all values");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.contains(&tf("s_ac_angeglichen", &[("0", Arg::from(2))])), "{said:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_numbers_of_two_sticks_are_swapped_after_a_question_and_only_while_the_game_is_quiet() {
        let _guard = setup();
        let mut bench = Bench::new("tauschen", Some(&file()));
        let path = maps_path(&bench);
        bench.settle();
        let button = tf("s_ac_tauschen", &[("0", Arg::from("RIGHT VPC Stick"))]);
        bench.click(&button);
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        let numbers = versekit_core::joysticks::assignment_from_text(&after);
        assert_eq!(
            numbers.iter().map(|a| (a.number, a.id.as_str())).collect::<Vec<_>>(),
            [(1, RIGHT), (2, LEFT)],
            "{after}"
        );
        assert!(after.contains("<action name=\"v_pitch\"><rebind input=\"js1_y\"/>"), "no binding line moved");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == t("s_ac_getauscht"), "{said:?}");
        // The game runs: the button is blocked.
        let log = format!("{}/StarCitizen/LIVE/Game.log", bench.home);
        std::fs::File::options()
            .write(true)
            .open(&log)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()))
            .expect("log");
        bench.time += 4.0;
        bench.settle();
        let before = std::fs::read(&path).expect("file");
        let texts = bench.click(&button);
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "a blocked button asks nothing: {texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "not while the game runs");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_old_entries_stand_below_closed_and_are_cleared_after_a_question_with_one_copy() {
        let _guard = setup();
        let mut bench = Bench::new("altbestand", Some(&file()));
        let path = maps_path(&bench);
        let texts = bench.settle();
        // Closed: only the title and the count.
        assert!(shows(&texts, &t("s_ac_befund")), "{texts:?}");
        assert!(shows(&texts, &t("s_ac_befund_kopf_eins")), "{texts:?}");
        assert!(!shows(&texts, &t("s_ac_befund_ruhig")), "closed: {texts:?}");
        let texts = bench.click(&t("s_ac_befund"));
        assert!(shows(&texts, &t("s_ac_befund_ruhig")) && shows(&texts, &t("s_ac_aufraeumen")), "{texts:?}");
        // The old block holds the dead zone 0.2 of x, the live one 0.152: shown as a difference.
        assert!(
            shows(&texts, &format!("{}: {} 0.2, {} 0.15", t("s_kv_totzone"), t("s_ac_war"), t("s_ac_jetzt"))),
            "{texts:?}"
        );
        let before = std::fs::read(&path).expect("file");
        let texts = bench.click(&t("s_ac_aufraeumen"));
        assert!(shows(&texts, t("s_ac_aufraeumen_frage_eins").lines().next().unwrap_or_default()), "{texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "asked, nothing written");
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(!after.contains("83F33344") || after.matches("83F33344").count() == 0, "{after}");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1);
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == t("s_ac_aufgeraeumt_eins"), "{said:?}");
        // Nothing old is left: the block is gone.
        let texts = bench.later_settle();
        assert!(!shows(&texts, &t("s_ac_befund")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_old_values_are_carried_over_to_the_live_block_after_a_question() {
        let _guard = setup();
        let mut bench = Bench::new("uebernehmen", Some(&file()));
        let path = maps_path(&bench);
        bench.settle();
        bench.click(&t("s_ac_befund"));
        let before = std::fs::read(&path).expect("file");
        let texts = bench.click(&t("s_ac_uebernehmen"));
        let question = tf("s_ac_uebernehmen_frage", &[("0", Arg::from("LEFT VPC Stick"))]);
        assert!(shows(&texts, question.lines().next().unwrap_or_default()), "{texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "asked, nothing written");
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        // The live block of x had 0.152; it has the old 0.2 now, in one entry.
        assert!(after.contains("<option input=\"x\" deadzone=\"0.2\"/>"), "{after}");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1);
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == t("s_ac_gespeichert"), "{said:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn discarding_puts_every_slider_back_to_the_file() {
        let _guard = setup();
        let mut bench = Bench::new("verwerfen", Some(&file()));
        bench.settle();
        let dead = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        bench.state.slide(&dead, 152).now = 300;
        assert!(shows(&bench.settle(), &t("s_ac_geaendert")));
        let texts = bench.click(&t("s_ac_verwerfen"));
        assert!(!shows(&texts, &t("s_ac_geaendert")) && !bench.state.changed_any(), "{texts:?}");
        assert!(bench.picture().contains("0.15"), "{}", bench.picture());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    /// The slider of the sensitivity of a function, as the file puts it on the grid.
    fn exponent_key(function: &str) -> Key {
        Key::Game { number: 1, axis: function.into() }
    }

    #[test]
    fn the_drawn_curve_follows_the_sensitivity_slider_dragged_last_even_when_the_functions_differ() {
        let _guard = setup();
        let mut bench = Bench::new("empfindlich", Some(&file()));
        bench.settle();
        // x: the roll alone with 0.5: the curve is that of the exponent 0.5, point for point.
        assert!(draws(&bench, 0.152, 0.848, 0.5), "x: {:?}", bench.curve.get(40));
        // y (dead zone 0.099, no saturation set): pitch and strafe with 1.5 each.
        bench.click("y");
        assert!(draws(&bench, 0.099, 1.0, 1.5), "y at the start");
        let at_start = bench.curve.clone();
        let (pitch, strafe) = (function_name("flight_move_pitch"), function_name("flight_move_strafe_vertical"));
        // The player drags the sensitivity of the pitch to 2.55: the curve is that of 2.55, not a straight line and
        // not the old one. (The old page drew a straight line as soon as the two functions differed.)
        bench.set_slider(&pitch, 51, EXPONENT);
        assert_eq!(bench.state.sliders[&exponent_key("flight_move_pitch")].now, 51, "the slider took the value");
        assert!(draws(&bench, 0.099, 1.0, 2.55), "2.55 of the pitch");
        assert_ne!(bench.curve, at_start, "another sensitivity, another curve");
        let texts = bench.settle();
        assert!(shows(&texts, &tf("s_ac_zeigt", &[("0", Arg::from(pitch.as_str()))])), "says which: {texts:?}");
        assert!(shows(&texts, &t("s_ac_zeigt_mehrere")), "says that there is a choice: {texts:?}");
        // Now the strafe is dragged to 0.5: the curve follows this one, the pitch keeps its slider.
        bench.set_slider(&strafe, 10, EXPONENT);
        assert!(draws(&bench, 0.099, 1.0, 0.5), "0.5 of the strafe");
        assert_eq!(bench.state.sliders[&exponent_key("flight_move_pitch")].now, 51, "the pitch stays");
        assert!(shows(&bench.settle(), &tf("s_ac_zeigt", &[("0", Arg::from(strafe.as_str()))])));
        // And on: the same function again, at the other end.
        bench.set_slider(&strafe, 80, EXPONENT);
        assert!(draws(&bench, 0.099, 1.0, 4.0), "4.0 of the strafe");
        // Dead zone and saturation still draw their bands and their bend.
        let dead = Key::Physical { ident: LEFT.into(), axis: "y".into(), prop: Prop::Deadzone };
        bench.state.slide(&dead, 99).now = 300;
        bench.settle();
        assert!(draws(&bench, 0.3, 1.0, 4.0), "dead zone 0.3 with the sensitivity 4.0");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_click_on_the_name_of_a_function_makes_the_curve_show_that_function() {
        let _guard = setup();
        let mut bench = Bench::new("zeigen", Some(&file()));
        bench.settle();
        bench.click("y");
        let (pitch, strafe) = (function_name("flight_move_pitch"), function_name("flight_move_strafe_vertical"));
        bench.state.slide(&exponent_key("flight_move_pitch"), 30).now = 51;
        bench.state.slide(&exponent_key("flight_move_strafe_vertical"), 30).now = 10;
        // Nothing was touched yet: the first function of the axis is drawn.
        bench.settle();
        assert!(draws(&bench, 0.099, 1.0, 2.55), "the first function (pitch) before anything was touched");
        bench.click(&strafe);
        assert!(draws(&bench, 0.099, 1.0, 0.5), "after the click on the strafe");
        assert!(shows(&bench.settle(), &tf("s_ac_zeigt", &[("0", Arg::from(strafe.as_str()))])));
        bench.click(&pitch);
        assert!(draws(&bench, 0.099, 1.0, 2.55), "after the click on the pitch");
        assert!(shows(&bench.settle(), &tf("s_ac_zeigt", &[("0", Arg::from(pitch.as_str()))])));
        // The names are choices for a screen reader, the chosen one is marked.
        let spoken = tf("s_ac_zeigen_name", &[("0", Arg::from(pitch.as_str()))]);
        assert!(bench.nodes.iter().any(|(role, name)| *role == egui::Role::RadioButton && *name == spoken));
        // The sliders were not touched by the clicks.
        assert_eq!(bench.state.sliders[&exponent_key("flight_move_strafe_vertical")].now, 10);
        // The choice stays over a visit to another axis; a function that lies elsewhere is not drawn there.
        bench.click("x");
        assert!(shows(&bench.settle(), &function_name("flight_move_roll")), "x draws its own roll");
        assert!(!shows(&bench.settle(), &t("s_ac_zeigt_mehrere")), "one function: no choice to tell of");
        bench.click("y");
        assert!(draws(&bench, 0.099, 1.0, 2.55), "back on y: still the pitch");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_presets_are_the_five_asked_for_with_names_in_both_languages_and_values_on_the_grid_of_the_sliders() {
        let _guard = setup();
        // Written out here, not read from the table: the table is what is tested.
        let want = [
            ("standard", 0.0, 1.0, 1.0),
            ("balanced", 0.05, 1.0, 1.5),
            ("fine", 0.05, 1.0, 2.0),
            ("direct", 0.03, 1.0, 1.0),
            ("pedals", 0.08, 1.0, 1.5),
        ];
        assert_eq!(axes::PRESETS.len(), want.len());
        for (preset, (key, deadzone, saturation, exponent)) in axes::PRESETS.iter().zip(want) {
            assert_eq!(
                (preset.key, preset.deadzone, preset.saturation, preset.exponent),
                (key, deadzone, saturation, exponent)
            );
            // The slider can show the value exactly: a preset the slider rounds away would show another number.
            assert_eq!(SHARE.value(SHARE.units(deadzone)), deadzone, "{key}");
            assert_eq!(SHARE.value(SHARE.units(saturation)), saturation, "{key}");
            assert_eq!(EXPONENT.value(EXPONENT.units(exponent)), exponent, "{key}");
        }
        let names_de = ["Spielstandard", "Ausgewogen", "Feines Zielen", "Direkt", "Pedale"];
        for (choice, language) in [(Choice::De, "de"), (Choice::En, "en")] {
            language::set_thread_language(choice);
            for (at, preset) in axes::PRESETS.iter().enumerate() {
                for key in [format!("s_ac_vor_{}", preset.key), format!("s_ac_vor_{}_t", preset.key)] {
                    let text = t(&key);
                    assert!(text != key && !text.contains('—') && !text.is_empty(), "{language}: {key}");
                }
                if language == "de" {
                    assert_eq!(t(&format!("s_ac_vor_{}", preset.key)), names_de[at]);
                }
            }
        }
        language::set_thread_language(Choice::De);
    }

    #[test]
    fn a_preset_puts_its_values_on_the_sliders_as_an_unsaved_change_and_saving_writes_them() {
        let _guard = setup();
        let mut bench = Bench::new("voreinstellung", Some(&file()));
        let path = maps_path(&bench);
        let before = std::fs::read(&path).expect("file");
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_ac_geaendert")));
        let dead = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Deadzone };
        let saturation = Key::Physical { ident: LEFT.into(), axis: "x".into(), prop: Prop::Saturation };
        let roll = exponent_key("flight_move_roll");
        // "Feines Zielen" on x: 0.05 / 1.0 / 2.0 on the dead zone, the saturation and the roll that lies on x.
        let texts = bench.click(&t("s_ac_vor_fine"));
        let now = |bench: &Bench, key: &Key| bench.state.sliders[key].now;
        assert_eq!((now(&bench, &dead), now(&bench, &saturation), now(&bench, &roll)), (50, 1000, 40));
        assert!(shows(&texts, &t("s_ac_geaendert")), "an unsaved change: {texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "nothing is written by a click on a preset");
        assert!(draws(&bench, 0.05, 1.0, 2.0), "the curve shows the preset at once");
        let said = bench.state.take_notice().expect("told");
        assert_eq!(said.text, tf("s_ac_vor_gesetzt", &[("0", Arg::from(t("s_ac_vor_fine").as_str()))]));
        // Another preset replaces it; the game default is 0 / 1 / 1.
        bench.click(&t("s_ac_vor_pedals"));
        assert_eq!((now(&bench, &dead), now(&bench, &saturation), now(&bench, &roll)), (80, 1000, 30));
        bench.click(&t("s_ac_vor_standard"));
        assert_eq!((now(&bench, &dead), now(&bench, &saturation), now(&bench, &roll)), (0, 1000, 20));
        assert!(draws(&bench, 0.0, 1.0, 1.0));
        // Discarding takes the file's own values back.
        bench.click(&t("s_ac_verwerfen"));
        assert!(!bench.state.changed_any());
        assert!(draws(&bench, 0.152, 0.848, 0.5));
        // On y two functions lie: both get the sensitivity.
        bench.click("y");
        bench.click(&t("s_ac_vor_balanced"));
        assert_eq!(now(&bench, &exponent_key("flight_move_pitch")), 30);
        assert_eq!(now(&bench, &exponent_key("flight_move_strafe_vertical")), 30);
        assert_eq!(now(&bench, &Key::Physical { ident: LEFT.into(), axis: "y".into(), prop: Prop::Deadzone }), 50);
        // Saving writes what the preset put on the sliders, nothing else.
        bench.click(&t("s_ac_speichern"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(after.contains("input=\"y\" deadzone=\"0.05\""), "{after}");
        assert!(after.contains("<flight_move_pitch exponent=\"1.5\"/>"), "{after}");
        assert!(after.contains("<flight_move_roll exponent=\"0.5\"/>"), "x was left alone: {after}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_values_of_an_axis_go_to_the_ticked_axes_or_to_all_of_them_as_an_unsaved_change() {
        let _guard = setup();
        let mut bench = Bench::new("uebertragen", Some(&file()));
        let path = maps_path(&bench);
        let before = std::fs::read(&path).expect("file");
        let button = t("s_ac_uebertragen");
        let y_row = format!("y · {}", tf("s_ac_mehrere_funktionen", &[("n", Arg::from(2))]));
        let physical = |axis: &str, prop| Key::Physical { ident: LEFT.into(), axis: axis.into(), prop };
        bench.settle();
        // Nothing ticked: the button is blocked and does nothing.
        bench.click(&button);
        assert!(!bench.state.changed_any() && bench.state.take_notice().is_none(), "blocked");
        // Tick y, copy x (dead zone 0.152, saturation 0.848, roll 0.5) to it.
        bench.click_node(egui::Role::CheckBox, &y_row);
        assert_eq!(bench.state.copy_targets, HashSet::from(["y".to_string()]));
        bench.click(&button);
        let now = |bench: &Bench, key: &Key| bench.state.sliders[key].now;
        assert_eq!(now(&bench, &physical("y", Prop::Deadzone)), 152);
        assert_eq!(now(&bench, &physical("y", Prop::Saturation)), 848);
        assert_eq!(now(&bench, &exponent_key("flight_move_pitch")), 10);
        assert_eq!(now(&bench, &exponent_key("flight_move_strafe_vertical")), 10);
        assert!(bench.state.copy_targets.is_empty(), "the ticks are used up");
        let said = bench.state.take_notice().expect("told");
        assert_eq!(said.text, t("s_ac_uebertragen_fertig_eins"));
        assert!(bench.state.changed_any());
        assert_eq!(std::fs::read(&path).expect("file"), before, "unsaved: the file is untouched");
        // rotz was not ticked: no slider of it was touched.
        assert!(bench.state.sliders.keys().all(|k| !matches!(k, Key::Physical { axis, .. } if axis == "rotz")));
        // Saving writes it.
        bench.click(&t("s_ac_speichern"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(after.contains("input=\"y\" deadzone=\"0.152\""), "{after}");
        assert!(after.contains("<flight_move_pitch exponent=\"0.5\"/>"), "{after}");
        // "All axes" ticks every other axis of the device; the unsaved dead zone of x goes along.
        bench.state.slide(&physical("x", Prop::Deadzone), 152).now = 300;
        bench.settle();
        bench.click_node(egui::Role::CheckBox, &t("s_ac_alle_achsen"));
        assert_eq!(bench.state.copy_targets, HashSet::from(["y".to_string(), "rotz".to_string()]));
        bench.click_node(egui::Role::CheckBox, &y_row);
        assert_eq!(bench.state.copy_targets, HashSet::from(["rotz".to_string()]), "one off: not all any more");
        bench.click_node(egui::Role::CheckBox, &t("s_ac_alle_achsen"));
        bench.click(&button);
        assert_eq!(now(&bench, &physical("rotz", Prop::Deadzone)), 300);
        assert_eq!(now(&bench, &physical("rotz", Prop::Saturation)), 848);
        assert_eq!(now(&bench, &physical("y", Prop::Deadzone)), 300);
        let said = bench.state.take_notice().expect("told");
        assert_eq!(said.text, tf("s_ac_uebertragen_fertig", &[("0", Arg::from(2))]));
        // The ticks belong to the axis they were made for.
        bench.click_node(egui::Role::CheckBox, &y_row);
        assert!(!bench.state.copy_targets.is_empty());
        bench.click("y");
        assert!(bench.state.copy_targets.is_empty(), "another axis, no ticks");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_sensitivity_that_is_copied_is_that_of_the_function_the_curve_shows() {
        let _guard = setup();
        let mut bench = Bench::new("uebertragen-zeigt", Some(&file()));
        bench.settle();
        bench.click("y");
        // The pitch is dragged to 2.55 (the strafe stays at 1.5): the curve shows the pitch.
        bench.set_slider(&function_name("flight_move_pitch"), 51, EXPONENT);
        bench.click_node(egui::Role::CheckBox, &format!("x · {}", function_name("flight_move_roll")));
        bench.click(&t("s_ac_uebertragen"));
        assert_eq!(
            bench.state.sliders[&exponent_key("flight_move_roll")].now,
            51,
            "the roll got the 2.55 of the pitch"
        );
        // Now the strafe is the one shown: copying to x puts its value there.
        bench.click(&function_name("flight_move_strafe_vertical"));
        bench.click_node(egui::Role::CheckBox, &format!("x · {}", function_name("flight_move_roll")));
        bench.click(&t("s_ac_uebertragen"));
        assert_eq!(bench.state.sliders[&exponent_key("flight_move_roll")].now, 30, "and now the 1.5 of the strafe");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_large_view_names_the_function_and_its_sensitivity() {
        let _guard = setup();
        let mut bench = Bench::new("gross", Some(&file()));
        bench.settle();
        bench.click("y");
        bench.state.slide(&exponent_key("flight_move_pitch"), 30).now = 51;
        bench.settle();
        let texts = bench.click(&t("s_ac_gross"));
        assert!(shows(&texts, &format!("{} 2.55", t("s_ac_empf_kurz"))), "{texts:?}");
        let title = format!("{} LEFT VPC Stick y · {}", t("s_ac_titel_gross"), function_name("flight_move_pitch"));
        assert!(shows(&texts, &title), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn without_a_function_on_the_axis_the_curve_is_the_plain_one_and_says_so() {
        let _guard = setup();
        let mut state = AxesState::default();
        let plotted = plotted_sensitivity(&mut state, &[]);
        assert_eq!((plotted.exponent, plotted.function, plotted.several), (DEFAULT_EXPONENT, None, false));
        // With two the touched one wins, else the first.
        let function =
            |axis: &str| Function { axis: axis.into(), action: String::new(), exponent: Some(1.5), invert: None };
        let keys = vec![
            (exponent_key("flight_move_pitch"), function("flight_move_pitch")),
            (exponent_key("flight_move_strafe_vertical"), function("flight_move_strafe_vertical")),
        ];
        state.slide(&keys[1].0, 30).now = 40;
        assert_eq!(plotted_sensitivity(&mut state, &keys).exponent, 1.5, "the first one, the second is not touched");
        state.shown = Some(keys[1].0.clone());
        let plotted = plotted_sensitivity(&mut state, &keys);
        assert_eq!(plotted.exponent, 2.0);
        assert!(plotted.several);
        // A function of another axis, once touched, does not hold the curve of this one.
        state.shown = Some(exponent_key("flight_move_roll"));
        assert_eq!(plotted_sensitivity(&mut state, &keys).exponent, 1.5);
    }

    #[test]
    fn several_files_side_by_side_are_listed_with_the_one_that_is_read_first() {
        let _guard = setup();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let files = vec![
            FileLine { path: "C:/a/USER/actionmaps.xml".into(), changed: "01.02.2026 10:00".into() },
            FileLine { path: "C:/a/user/actionmaps.xml".into(), changed: "01.01.2026 09:00".into() },
        ];
        let scheme = theme::current();
        let mut texts = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| many_files(ui, scheme, &files));
        output.textures_delta.clear();
        fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        let joined = texts.join("\n");
        assert!(
            joined.contains(&format!("{} · 01.02.2026 10:00 · C:/a/USER/actionmaps.xml", t("s_ac_gelesen"))),
            "{joined}"
        );
        assert!(
            joined.contains(&format!("{} · 01.01.2026 09:00 · C:/a/user/actionmaps.xml", t("s_ac_liegt"))),
            "{joined}"
        );
    }

    #[test]
    fn the_sliders_put_the_value_of_the_file_on_their_grid_like_tk() {
        // The game writes 0.098999992: on the grid of a thousandth it is 99, and the player has not moved it.
        assert_eq!(SHARE.units(0.098999992), 99);
        assert_eq!(SHARE.units(0.15000001), 150);
        assert_eq!(SHARE.units(0.8479999), 848);
        assert_eq!(SHARE.units(1.7), 1000, "held at the end");
        assert_eq!(SHARE.units(-0.2), 0);
        assert_eq!(EXPONENT.units(1.5), 30);
        assert_eq!(EXPONENT.units(1.0), 20);
        assert_eq!(EXPONENT.units(0.1), 4, "below the end of the slider");
        assert_eq!(EXPONENT.units(10.0), 80);
        assert!((EXPONENT.value(30) - 1.5).abs() < 1e-12 && (SHARE.value(99) - 0.099).abs() < 1e-12);
    }

    #[test]
    fn a_slider_that_is_not_touched_follows_the_file_and_one_that_is_keeps_its_place() {
        let mut state = AxesState::default();
        let key = Key::Physical { ident: "A".into(), axis: "x".into(), prop: Prop::Deadzone };
        assert_eq!(state.slide(&key, 150).now, 150);
        assert!(!state.changed_any());
        // The file moves on while the slider is not touched: the slider moves with it.
        assert_eq!(state.slide(&key, 200).now, 200);
        // The player drags it: it is changed, and it stays where he put it.
        state.slide(&key, 200).now = 300;
        assert!(state.changed_any());
        assert_eq!(state.slide(&key, 200).now, 300);
        // Back on the starting point is no change.
        state.slide(&key, 200).now = 200;
        assert!(!state.changed_any());
    }

    /// Saves the state of the files as a set, through the field and the button of the page.
    fn save_set_as(bench: &mut Bench, name: &str) -> Texts {
        bench.settle();
        bench.state.set_name = name.to_string();
        bench.click(&t("s_gs_speichern"))
    }

    /// The game is running: the log is written just now.
    fn game_runs(bench: &mut Bench) {
        let log = format!("{}/StarCitizen/LIVE/Game.log", bench.home);
        std::fs::File::options()
            .write(true)
            .open(&log)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()))
            .expect("log");
        bench.time += 4.0;
    }

    /// The file as the player changes it after the set was saved: the dead zone of x of the left stick.
    fn change_left_dead_zone(bench: &mut Bench) -> String {
        let path = maps_path(bench);
        let text = std::fs::read_to_string(&path).expect("file").replace("deadzone=\"0.152\"", "deadzone=\"0.2\"");
        std::fs::write(&path, &text).expect("write");
        bench.later_settle();
        path
    }

    #[test]
    fn a_set_is_saved_under_a_name_and_a_name_that_exists_asks_before_it_is_overwritten() {
        let _guard = setup();
        let mut bench = Bench::new("satz-sichern", Some(&file()));
        let path = maps_path(&bench);
        let before = std::fs::read(&path).expect("file");
        // Without a name: told, nothing saved.
        save_set_as(&mut bench, "  ");
        let texts = bench.settle();
        assert!(bench.state.take_notice().is_none(), "a bad name is said at the field, not in the foot");
        assert!(shows(&texts, &t("s_gs_f_name_leer")), "{texts:?}");
        assert!(device_sets::sets(&bench.paths).is_empty());
        // With a name: saved, the field is empty again, the set stands on the page, the game file is untouched.
        save_set_as(&mut bench, " mit Pedalen ");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == tf("s_gs_gesichert", &[("0", Arg::from("mit Pedalen"))]), "{said:?}");
        assert!(bench.state.set_name.is_empty());
        assert_eq!(std::fs::read(&path).expect("file"), before, "a set is kept beside the game file, not in it");
        let texts = bench.later_settle();
        assert!(texts.iter().any(|(s, _)| s == "mit Pedalen"), "{texts:?}");
        assert!(!shows(&texts, &t("s_gs_keine")), "{texts:?}");
        let stand = device_sets::get(&bench.paths, "mit Pedalen").expect("saved").stand;
        // The same name again asks first; the answer no leaves the set as it was.
        let texts = save_set_as(&mut bench, "mit Pedalen");
        let question = tf("s_gs_ueberschreiben_frage", &[("0", Arg::from("mit Pedalen"))]);
        assert!(shows(&texts, &question), "{texts:?}");
        assert!(bench.state.take_notice().is_none(), "nothing is said before the answer");
        let texts = bench.click(&t("abbrechen"));
        assert!(!shows(&texts, &question), "{texts:?}");
        assert_eq!(device_sets::get(&bench.paths, "mit Pedalen").expect("kept").stand, stand);
        // The answer yes overwrites it and tells so.
        save_set_as(&mut bench, "mit Pedalen");
        bench.click(&t("s_mlg_bestaetigen"));
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == tf("s_gs_gesichert", &[("0", Arg::from("mit Pedalen"))]), "{said:?}");
        assert_eq!(device_sets::sets(&bench.paths).len(), 1, "overwritten, not doubled");
        assert!(bench.state.set_name.is_empty());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_set_is_applied_after_a_question_with_one_copy_and_then_there_is_nothing_to_do() {
        let _guard = setup();
        let mut bench = Bench::new("satz-anwenden", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        bench.state.take_notice();
        // Nothing differs yet: the button is blocked and asks nothing.
        let texts = bench.click(&t("s_gs_anwenden"));
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "nothing to apply: {texts:?}");
        // The player changes a value; now the set has something to say.
        let path = change_left_dead_zone(&mut bench);
        let before = std::fs::read(&path).expect("file");
        let texts = bench.click(&t("s_gs_anwenden"));
        let question = tf("s_gs_frage_eins", &[("0", Arg::from("mit Pedalen"))]);
        assert!(shows(&texts, question.lines().next().unwrap_or_default()), "{texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "asked, nothing written");
        let texts = bench.click(&t("abbrechen"));
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "{texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "no: nothing written");
        bench.click(&t("s_gs_anwenden"));
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        // The dead block of the old id keeps its 0.2: only the live block is set.
        assert!(after.contains("deadzone=\"0.152\"") && after.matches("deadzone=\"0.2\"").count() == 1, "{after}");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1, "one copy");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == t("s_gs_angewendet_eins"), "{said:?}");
        // Applied: the same again is nothing to do and asks nothing.
        bench.later_settle();
        let texts = bench.click(&t("s_gs_anwenden"));
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "{texts:?}");
        assert_eq!(versekit_core::game_files::backups(&path).len(), 1, "no second copy");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_set_is_not_applied_while_the_game_runs() {
        let _guard = setup();
        let mut bench = Bench::new("satz-spiel", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        let path = change_left_dead_zone(&mut bench);
        let before = std::fs::read(&path).expect("file");
        game_runs(&mut bench);
        bench.settle();
        let texts = bench.click(&t("s_gs_anwenden"));
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "a blocked button asks nothing: {texts:?}");
        assert_eq!(std::fs::read(&path).expect("file"), before, "not while the game runs");
        assert!(versekit_core::game_files::backups(&path).is_empty());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_device_that_is_not_there_is_named_in_the_question_and_skipped() {
        let _guard = setup();
        let mut bench = Bench::new("satz-fehlt", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        // The right stick gets another id that no live device has (the set knows the old one).
        let path = maps_path(&bench);
        let moved =
            std::fs::read_to_string(&path).expect("file").replace(RIGHT, "53F43344-0000-0000-0000-504944564944");
        std::fs::write(&path, moved).expect("write");
        change_left_dead_zone(&mut bench);
        let texts = bench.click(&t("s_gs_anwenden"));
        let missing = t("s_gs_fehlt");
        let missing = missing.split("{0}").next().unwrap_or_default();
        assert!(shows(&texts, missing) && shows(&texts, "RIGHT VPC Stick"), "{texts:?}");
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&path).expect("file");
        assert!(after.contains("deadzone=\"0.152\""), "the left stick is set again: {after}");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault, "{said:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn two_sticks_of_one_name_are_told_apart_by_their_number_and_old_values_are_not_carried_to_either() {
        let _guard = setup();
        let same = file().replace("RIGHT VPC Stick", "LEFT VPC Stick");
        let mut bench = Bench::new("gleicher-name", Some(&same));
        let texts = bench.settle();
        assert!(shows(&texts, "LEFT VPC Stick (js1)") && shows(&texts, "LEFT VPC Stick (js2)"), "{texts:?}");
        // The question names the way it goes.
        let texts = bench.click(&tf("s_ac_angleichen", &[("0", Arg::from("LEFT VPC Stick (js2)"))]));
        assert!(
            shows(&texts, "LEFT VPC Stick (js1)") && shows(&texts, "LEFT VPC Stick (js2)"),
            "the question tells which is which: {texts:?}"
        );
        bench.click(&t("abbrechen"));
        // The old block carries the same name as two live devices: it is shown, but nothing offers to write it.
        let texts = bench.click(&t("s_ac_befund"));
        assert!(shows(&texts, &t("s_ac_name_mehrfach")), "{texts:?}");
        // The warning names the button in a sentence, so only a text that is the button itself counts.
        assert!(!texts.iter().any(|(s, _)| *s == t("s_ac_uebernehmen")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_slider_that_was_moved_does_not_survive_a_swap_and_write_onto_the_other_stick() {
        let _guard = setup();
        let mut bench = Bench::new("tausch-regler", Some(&file()));
        let path = maps_path(&bench);
        bench.settle();
        let key = Key::Game { number: 1, axis: "flight_move_pitch".into() };
        bench.state.slide(&key, 30).now = 60;
        assert!(bench.state.changed_any());
        bench.click(&tf("s_ac_tauschen", &[("0", Arg::from("RIGHT VPC Stick"))]));
        bench.click(&t("s_mlg_bestaetigen"));
        assert!(!bench.state.changed_any(), "the moved slider is gone with the swap");
        // Nothing is left to be written: the 3.0 would have landed on the stick that has number 1 now.
        assert!(bench.state.settings().is_empty());
        assert!(!std::fs::read_to_string(&path).expect("file").contains("exponent=\"3\""));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_file_that_can_not_be_read_is_said_so_and_is_not_called_missing() {
        let _guard = setup();
        let mut bench = Bench::new("unlesbar", Some("<ActionMaps><oops"));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_datei_gesperrt")), "{texts:?}");
        assert!(!shows(&texts, &t("s_ac_keine")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_device_without_values_keeps_the_tools_to_fill_it() {
        let _guard = setup();
        // The right stick has no entry of its own that holds a value.
        let text = file()
            .replace("   <option input=\"x\" deadzone=\"0.1\"/>\n   <option input=\"x\" deadzone=\"0.3\"/>\n", "");
        let mut bench = Bench::new("ohne-werte", Some(&text));
        bench.settle();
        let texts = bench.click("RIGHT VPC Stick");
        assert!(shows(&texts, &t("s_ac_keine_werte")), "{texts:?}");
        // The menus stay: "make it like the other" in "Gerät", the sets in "Gerätesatz".
        let button = tf("s_ac_angleichen", &[("0", Arg::from("LEFT VPC Stick"))]);
        for wanted in [button, t("s_gs_titel")] {
            assert!(bench.reveal(|bench| bench.rects.iter().any(|(text, _)| *text == wanted)), "{wanted}: {texts:?}");
        }
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_screen_reader_hears_whose_button_it_is_and_which_device_and_axis_are_chosen() {
        let _guard = setup();
        let mut bench = Bench::new("sprechen", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        save_set_as(&mut bench, "ohne Pedale");
        bench.settle();
        let named =
            |bench: &Bench, role: egui::Role, name: &str| bench.nodes.iter().any(|(r, n)| *r == role && n == name);
        let apply = t("s_gs_anwenden");
        for set in ["mit Pedalen", "ohne Pedale"] {
            assert!(named(&bench, egui::Role::Button, &format!("{apply}: {set}")), "{:?}", bench.nodes);
        }
        assert!(!named(&bench, egui::Role::Button, &apply), "no bare label on a button that belongs to a set");
        // The devices are tabs, the chosen one says so; the axis rows say it as well.
        assert!(named(&bench, egui::Role::Tab, "LEFT VPC Stick") && named(&bench, egui::Role::Tab, "RIGHT VPC Stick"));
        let chosen: Vec<_> =
            bench.nodes.iter().filter(|(r, n)| *r == egui::Role::Button && n.starts_with("x, ")).collect();
        assert_eq!(chosen.len(), 1, "{:?}", bench.nodes);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_set_is_deleted_after_a_question() {
        let _guard = setup();
        let mut bench = Bench::new("satz-loeschen", Some(&file()));
        save_set_as(&mut bench, "mit Pedalen");
        bench.state.take_notice();
        let question = tf("s_gs_loeschen_frage", &[("0", Arg::from("mit Pedalen"))]);
        let texts = bench.click(&t("s_gs_loeschen"));
        assert!(shows(&texts, &question), "{texts:?}");
        assert_eq!(device_sets::sets(&bench.paths).len(), 1, "asked, nothing deleted");
        bench.click(&t("abbrechen"));
        assert_eq!(device_sets::sets(&bench.paths).len(), 1, "no: kept");
        bench.click(&t("s_gs_loeschen"));
        let texts = bench.click(&t("s_mlg_bestaetigen"));
        assert!(device_sets::sets(&bench.paths).is_empty());
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text == tf("s_gs_geloescht", &[("0", Arg::from("mit Pedalen"))]), "{said:?}");
        let none = t("s_gs_keine");
        assert!(bench.reveal(|bench| bench.rects.iter().any(|(text, _)| *text == none)), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
