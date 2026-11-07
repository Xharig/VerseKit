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

//! The page "Steuerung" (`_joysticks` and `_device_hub` of 4.12): which stick is which, and what lies on it.
//!
//! This part: the block "Deine Geräte" and the comparison of log and binding. Everything is **read**; the files
//! are not touched. The system is asked every three seconds in a thread of its own (a device list that hangs
//! must not hold the window), the files only when they changed (`device_hub::source_stamp`).
//!
//! ⚠ **The number the game gives is not the number the system gives** (the same stick can be `js0` for the
//! system and `js2` in the game), and the name is spelled differently in all three places. What joins them is
//! the id, never the name.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::device_hub::{self, Entry, SourceStamp, Status, Step, Summary, Watchdog};
use versekit_core::game_files;
use versekit_core::input_device::{self, SystemDevice};
use versekit_core::joysticks::{self, Assigned, Comparison, Device, State};
use versekit_core::joysticks_write;
use versekit_core::language::{Arg, t, tf};
use versekit_core::paths::Paths;
use versekit_core::theme::{Color, Scheme};

use crate::settings::Said;
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How often the system is asked, in seconds.
const POLL_SECONDS: f64 = 3.0;
/// The page counts as left when it was not drawn for this long, in seconds: it starts afresh when it is shown
/// again (what came and went is not kept from an earlier visit).
const LEFT_SECONDS: f64 = 4.0 * POLL_SECONDS;
/// A look that takes longer than this (a device that does not answer) is given up, and a new one is asked.
const LOOK_GIVE_UP_SECONDS: f64 = 15.0;

/// What the files say; read again only when they changed.
#[derive(Clone)]
struct FileData {
    /// Counts the files read: a number nobody else has (what is made of the data compares it).
    id: u64,
    /// The bindings per device in the `actionmaps.xml`.
    own: Vec<(String, Vec<joysticks::Binding>)>,
    stamp: SourceStamp,
    /// The state of the binding file: a file that can not be read now is not an answer "no binding", and what was
    /// read of it is not kept for the next look.
    state: joysticks::FileState,
    seen: Vec<Device>,
    stored: Vec<Assigned>,
    compare: Comparison,
}

/// One look at the system and the files.
struct Look {
    /// Why nothing may be written now (the game runs, or nobody can tell); `None` when the game is quiet.
    quiet: Option<String>,
    system: Vec<SystemDevice>,
    files: Arc<FileData>,
}

/// Where the sticks of the system come from (the tests put in their own).
type Source = Arc<dyn Fn() -> Vec<SystemDevice> + Send + Sync>;

/// What the page keeps.
pub struct JoystickState {
    source: Source,
    look: Option<Look>,
    summary: Summary,
    job: Option<Receiver<Look>>,
    /// When the look in the thread was started.
    job_started: f64,
    /// The last look failed (the thread ended without an answer): told once, not at every beat.
    read_failed: bool,
    polled_at: Option<f64>,
    drawn_at: Option<f64>,
    watchdog: Watchdog,
    /// Plugged in / pulled since the page was opened, by id in the order they came: (id, name).
    plugged: Vec<(String, String)>,
    unplugged: Vec<(String, String)>,
    notice: Option<Said>,
    /// The list of the bindings below.
    list: crate::joysticks_list::ListState,
    /// A change that waits for the yes of the player.
    pending: Option<Pending>,
}

impl Default for JoystickState {
    fn default() -> Self {
        JoystickState {
            source: Arc::new(input_device::devices),
            look: None,
            summary: device_hub::summary_of(Vec::new()),
            job: None,
            job_started: 0.0,
            read_failed: false,
            polled_at: None,
            drawn_at: None,
            watchdog: Watchdog::default(),
            plugged: Vec::new(),
            unplugged: Vec::new(),
            notice: None,
            list: crate::joysticks_list::ListState::default(),
            pending: None,
        }
    }
}

impl JoystickState {
    /// The line for the foot, once.
    pub fn take_notice(&mut self) -> Option<Said> {
        self.notice.take()
    }

    /// Asks the sticks of the given source instead of the system (the tests: no hardware).
    #[cfg(test)]
    pub(crate) fn with_source(source: impl Fn() -> Vec<SystemDevice> + Send + Sync + 'static) -> Self {
        JoystickState { source: Arc::new(source), ..JoystickState::default() }
    }

    /// Reads everything again at the next look.
    pub fn invalidate(&mut self) {
        self.polled_at = None;
        self.list.invalidate();
        // What is shown stays until the new look is there; only its mark is dropped, so the files are read.
        if let Some(look) = &mut self.look {
            look.files = Arc::new(FileData { stamp: SourceStamp::default(), ..(*look.files).clone() });
        }
    }

    /// Starts the look in a thread of its own when it is due, takes its result when it is there.
    fn poll(&mut self, ui: &Ui, paths: &Paths) {
        let now = ui.input(|i| i.time);
        if self.drawn_at.is_some_and(|at| now - at > LEFT_SECONDS) {
            self.watchdog = Watchdog::default();
            self.plugged.clear();
            self.unplugged.clear();
            self.polled_at = None;
            // An answer that was still on its way from the earlier visit is dropped: it would be taken for the
            // first look of this visit, and what happened in the meantime would be told as news.
            self.job = None;
            // A question that waited while the page was away counts "N bindings" of an old state: asked anew.
            self.pending = None;
        }
        self.drawn_at = Some(now);
        if let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(look) => {
                    self.job = None;
                    self.read_failed = false;
                    self.take(look);
                }
                Err(mpsc::TryRecvError::Empty) if now - self.job_started > LOOK_GIVE_UP_SECONDS => {
                    // Does not answer: given up (the thread may hang on; its answer is not taken).
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
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
            let source = Arc::clone(&self.source);
            let before = self.look.as_ref().map(|look| Arc::clone(&look.files));
            let spawned = std::thread::Builder::new().name("joysticks-look".into()).spawn(move || {
                let _ = sender.send(read_look(&copy, &*source, before));
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

    /// A look arrived: who came and who went, then the list.
    fn take(&mut self, look: Look) {
        let (came, went) = self.watchdog.check(&look.system);
        for device in came {
            let id = device.id.to_uppercase();
            self.unplugged.retain(|(known, _)| *known != id);
            self.plugged.retain(|(known, _)| *known != id);
            self.plugged.push((id, shown_name(&device.name)));
        }
        for device in went {
            let id = device.id.to_uppercase();
            self.plugged.retain(|(known, _)| *known != id);
            self.unplugged.retain(|(known, _)| *known != id);
            self.unplugged.push((id, shown_name(&device.name)));
        }
        self.summary =
            device_hub::summary_of(device_hub::overview_from(&look.system, &look.files.seen, &look.files.stored));
        self.look = Some(look);
    }
}

fn shown_name(name: &str) -> String {
    if name.is_empty() { "?".to_string() } else { name.to_string() }
}

/// The look: the system always, the files only where they changed since `before`.
fn read_look(
    paths: &Paths,
    source: &(dyn Fn() -> Vec<SystemDevice> + Send + Sync),
    before: Option<Arc<FileData>>,
) -> Look {
    let system = source();
    let quiet = game_files::check_quiet(paths).err().map(|error| error.reason());
    let stamp = device_hub::source_stamp(paths, None);
    // ⚠ What was read of a file that could not be read is not kept: the mark does not change when the lock goes.
    if let Some(files) = before
        && files.stamp == stamp
        && stamp != SourceStamp::default()
        && files.state != joysticks::FileState::Unreadable
    {
        return Look { quiet, system, files };
    }
    let seen = joysticks::devices(paths, None);
    let stored = joysticks::assignment(paths, None, None);
    let file = joysticks::actionmaps_path(paths, None);
    let state = joysticks::file_state(paths, None, None);
    let compare = joysticks::compare_lists(seen.clone(), stored.clone(), file);
    let own = joysticks::bindings(paths, None, None);
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Look { quiet, system, files: Arc::new(FileData { id, own, stamp, state, seen, stored, compare }) }
}

/// The path of a device so it fits in a line. Under Linux it is short (`/dev/input/js0`) and stays; under
/// Windows it is the whole device path of over a hundred characters: only vendor and product number are
/// recognised in it, the rest is noise.
pub fn system_path_label(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if let Some((vendor, product)) = vendor_and_product(path) {
        return tf("s_gh_vidpid", &[("0", Arg::from(vendor.as_str())), ("1", Arg::from(product.as_str()))]);
    }
    if path.chars().count() <= 24 {
        path.to_string()
    } else {
        let head: String = path.chars().take(22).collect();
        format!("{head}…")
    }
}

/// `VID_xxxx` … `PID_xxxx` in a device path, in capital letters.
fn vendor_and_product(path: &str) -> Option<(String, String)> {
    let upper = path.to_uppercase();
    let take = |from: usize, mark: &str| -> Option<(String, usize)> {
        let at = upper[from..].find(mark)? + from + mark.len();
        let digits = upper.get(at..at + 4)?;
        digits.chars().all(|c| c.is_ascii_hexdigit()).then(|| (digits.to_string(), at + 4))
    };
    let (vendor, after) = take(0, "VID_")?;
    let (product, _) = take(after, "PID_")?;
    Some((vendor, product))
}

/// The space between two blocks of the page and between the parts of a block, in points.
pub(crate) const GAP: f32 = 8.0;
/// The space between two lines of a list.
pub(crate) const LINE_GAP: f32 = 2.0;
/// The state column of a line takes at most this part of the width, so the name always keeps room.
const STATE_SHARE: f32 = 0.35;

/// Only a state that is right is in the colour of the brand; every other one is in the gold. (Red would say "a
/// fault", and a stick that is pulled out is none: the binding does no harm.)
fn status_color(scheme: &Scheme, status: Status) -> Color {
    match status {
        Status::Ready => scheme.accent,
        Status::NoNumber | Status::Unknown | Status::Unplugged => scheme.gold,
    }
}

/// One line of a list: number of the game, name with a second line, state.
struct Row {
    tone: Color,
    number: String,
    number_bound: bool,
    name: String,
    sub: String,
    state: String,
    /// What a screen reader says for the whole line, and what shows when the mouse rests on it.
    spoken: String,
    help: String,
    salt: String,
}

/// The width of a text in the given style.
pub(crate) fn text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), style.resolve(ui.style()), egui::Color32::WHITE).size().x
}

/// A cell of fixed width in a line; its content wraps inside.
pub(crate) fn cell(ui: &mut Ui, width: f32, align: egui::Align, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(align), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

/// The widths of the two outer columns of a list: the number of the game and the state. They are the same in
/// every line (text never moves them) and only the name between them gives way.
fn columns(ui: &Ui, states: &[String]) -> (f32, f32) {
    let number = text_width(ui, "js00", egui::TextStyle::Body);
    let widest = states.iter().map(|s| text_width(ui, s, egui::TextStyle::Small)).fold(0.0_f32, f32::max);
    (number, widest.min(ui.available_width() * STATE_SHARE))
}

/// A line of a list with the bar of its state at the left. The line is one stop for the keyboard and the
/// screen reader, with the whole sentence.
fn list_row(ui: &mut Ui, scheme: &Scheme, row: &Row, columns: (f32, f32)) {
    let tone = color(row.tone);
    let radius = control_radius(scheme);
    let inner = egui::Frame::new()
        .fill(color(scheme.field))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(radius)
        .inner_margin(egui::Margin { left: 12, right: 8, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let gap = ui.spacing().item_spacing.x;
            let middle = (ui.available_width() - columns.0 - columns.1 - 2.0 * gap).max(60.0);
            ui.horizontal_top(|ui| {
                // The number of the game first and in the colour of the brand: that is the one the player has
                // to do with in the game and in guides.
                cell(ui, columns.0, egui::Align::Min, |ui| {
                    let hue = if row.number_bound { color(scheme.accent) } else { color(scheme.sub) };
                    ui.add(egui::Label::new(RichText::new(&row.number).strong().color(hue)).wrap());
                });
                cell(ui, middle, egui::Align::Min, |ui| {
                    ui.add(egui::Label::new(&row.name).wrap());
                    if !row.sub.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&row.sub).small().color(color(scheme.sub))).wrap());
                    }
                });
                cell(ui, columns.1, egui::Align::Max, |ui| {
                    ui.add(egui::Label::new(RichText::new(&row.state).small().color(tone)).wrap());
                });
            });
        });
    let rect = inner.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(widgets::LIST_BAR_WIDTH, rect.height())),
        0.0,
        tone,
    );
    let response =
        ui.interact(rect, ui.make_persistent_id(("joysticks-row", &row.salt)), egui::Sense::focusable_noninteractive());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListItem, true, &row.spoken));
    widgets::focus_ring(ui, &response, scheme, radius);
    if !row.help.is_empty() {
        response.on_hover_text(&row.help);
    }
    ui.add_space(LINE_GAP);
}

/// A small grey text, wrapped.
fn small(ui: &mut Ui, scheme: &Scheme, text: &str) {
    widgets::reading(ui, RichText::new(text).small().color(color(scheme.sub)));
}

/// The name of a block: the title of its card.
fn block_title(ui: &mut Ui, scheme: &Scheme, text: &str) {
    widgets::card_title(ui, scheme, text);
}

fn named(entry: &Entry) -> String {
    if entry.name.is_empty() { entry.short.clone() } else { entry.name.clone() }
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut JoystickState, paths: &Paths) {
    // Before anything is drawn: a key that is bound must not press a button of the page at the same time.
    crate::joysticks_list::take_keys(&mut state.list, ui.ctx());
    let mut written = crate::joysticks_bind::Outcome::Nothing;
    state.poll(ui, paths);
    ui.add(egui::Label::new(RichText::new(t("s_js_lead")).small().color(color(scheme.sub))).wrap());
    ui.add_space(GAP);
    let mut act = None;
    widgets::card(ui, scheme, |ui| {
        ui.set_min_width(ui.available_width());
        act = hub(ui, scheme, state);
        ui.add_space(GAP);
        ui.add(egui::Label::new(RichText::new(t("s_js_hinweis")).small().color(color(scheme.sub))).wrap());
    });
    if state.look.is_some() {
        ui.add_space(GAP);
        widgets::card(ui, scheme, |ui| {
            ui.set_min_width(ui.available_width());
            comparison(ui, scheme, state);
        });
        let files = state.look.as_ref().map(|look| Arc::clone(&look.files));
        if let Some(files) = files {
            ui.add_space(GAP);
            let quiet = state.look.as_ref().and_then(|look| look.quiet.clone());
            let (list, pending) = (&mut state.list, &state.pending);
            widgets::card(ui, scheme, |ui| {
                ui.set_min_width(ui.available_width());
                if let Some(next) = reset_block(ui, scheme, pending.as_ref(), quiet.as_deref(), &files.own) {
                    act = Some(next);
                }
                let data = crate::joysticks_list::Data { id: files.id, own: &files.own, stored: &files.stored };
                written = crate::joysticks_list::show(ui, scheme, list, paths, quiet.as_deref(), &data);
            });
        }
    }
    if let Some(act) = act {
        state.run(act, paths);
    }
    // The window "Neu belegen" has changed the file: told in the foot, and the files are read again at once.
    match written {
        crate::joysticks_bind::Outcome::Written(said) => {
            state.notice = Some(said);
            state.invalidate();
        }
        crate::joysticks_bind::Outcome::Refresh => state.invalidate(),
        crate::joysticks_bind::Outcome::Nothing => {}
    }
}

/// What the player asked for with a click on the page.
enum Act {
    /// A question is put (the change waits for the yes).
    Ask(Pending),
    /// The question is taken back.
    Cancel,
    /// The yes: the change under the question is made.
    Confirm,
}

/// A change that waits for the yes of the player.
#[derive(Clone, PartialEq, Eq)]
enum Pending {
    /// Hang the binding of a device on its new id.
    Swap { old_id: String, new_id: String },
    /// Throw away the own bindings (so many).
    Reset { count: usize },
}

impl JoystickState {
    /// Makes a change the player confirmed, tells what came of it and has the files read again.
    fn run(&mut self, act: Act, paths: &Paths) {
        match act {
            Act::Ask(pending) => self.pending = Some(pending),
            Act::Cancel => self.pending = None,
            Act::Confirm => {
                let Some(pending) = self.pending.take() else { return };
                let (result, done_key) = match &pending {
                    Pending::Swap { old_id, new_id, .. } => {
                        (joysticks_write::swap_id(paths, old_id, new_id, None, None), "s_js_fertig")
                    }
                    Pending::Reset { .. } => (joysticks_write::reset(paths, None, None), "s_js_zurueck_ok"),
                };
                self.notice = Some(match result {
                    Ok(done) => {
                        let copy = done.backup.as_deref().map(base_name).unwrap_or_default();
                        Said::note(tf(done_key, &[("0", Arg::from(copy.as_str()))]))
                    }
                    Err(failure) => Said::fault(tf("s_js_schief", &[("0", Arg::from(failure.text().as_str()))])),
                });
                // Read again at once: the list shows what the file says now.
                self.invalidate();
            }
        }
    }
}

/// The name of a file in a path.
pub(crate) fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// The question under a button and its two answers; gives `Cancel` or `Confirm` when one was clicked.
fn question(ui: &mut Ui, scheme: &Scheme, text: &str, danger: bool) -> Option<Act> {
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(GAP / 2.0);
    let mut answer = None;
    ui.horizontal_wrapped(|ui| {
        if ui.add(widgets::Button::new(&t("abbrechen"), ButtonKind::Normal, scheme)).clicked() {
            answer = Some(Act::Cancel);
        }
        let kind = if danger { ButtonKind::Danger } else { ButtonKind::Strong };
        if ui.add(widgets::Button::new(&t("s_mlg_bestaetigen"), kind, scheme)).clicked() {
            answer = Some(Act::Confirm);
        }
    });
    answer
}

/// The button that throws away the own bindings, on a line of its own: a button that throws away what someone
/// sat at for an evening does not stand in a row with harmless ones. The question names the **number** of the
/// bindings that go.
fn reset_block(
    ui: &mut Ui,
    scheme: &Scheme,
    pending: Option<&Pending>,
    quiet: Option<&str>,
    own: &[(String, Vec<joysticks::Binding>)],
) -> Option<Act> {
    let count = own.iter().flat_map(|(_, list)| list.iter()).filter(|b| !b.input.is_empty()).count();
    let mut act = None;
    match pending {
        Some(Pending::Reset { count }) => {
            let key = if *count == 1 { "s_js_zurueck_frage_eins" } else { "s_js_zurueck_frage" };
            let text = tf(key, &[("0", Arg::from(i64::try_from(*count).unwrap_or(i64::MAX)))]);
            act = question(ui, scheme, &text, true);
        }
        _ => {
            let name = t("s_js_zurueck");
            // Nothing of his own: nothing to throw away, and a question about "0 bindings" is no help.
            let nothing = t("s_js_zurueck_leer");
            let reason = quiet.or_else(|| (count == 0).then_some(nothing.as_str()));
            let button = widgets::Button::new(&name, ButtonKind::Danger, scheme).blocked(reason);
            if ui.add(button).clicked() {
                act = Some(Act::Ask(Pending::Reset { count }));
            }
            if let Some(reason) = reason {
                small(ui, scheme, reason);
            }
        }
    }
    ui.add_space(GAP);
    act
}

/// The block "Deine Geräte": all sticks of the three sources, then what is to be done.
fn hub(ui: &mut Ui, scheme: &Scheme, state: &JoystickState) -> Option<Act> {
    block_title(ui, scheme, &t("s_gh_titel"));
    small(ui, scheme, &t("s_gh_lead"));
    ui.add_space(GAP);
    let Some(look) = &state.look else {
        widgets::loading(ui, scheme, &t("s_gh_liest"));
        return None;
    };
    let summary = &state.summary;
    if summary.devices.is_empty() {
        small(ui, scheme, &t("s_gh_kein_geraet"));
    }
    let states: Vec<String> =
        [Status::Ready, Status::NoNumber, Status::Unplugged, Status::Unknown].iter().map(|s| status_text(*s)).collect();
    let widths = columns(ui, &states);
    for device in &summary.devices {
        let name = named(device);
        // Without a number the cell stays empty: the state next to it says why.
        let number = device.number.map_or_else(String::new, |n| format!("js{n}"));
        let state_text = status_text(device.status);
        let spoken = match device.number {
            Some(n) => format!("js{n}, {name}, {state_text}"),
            None => format!("{name}, {state_text}"),
        };
        let row = Row {
            tone: status_color(scheme, device.status),
            number,
            number_bound: device.number.is_some(),
            name,
            sub: system_path_label(&device.system_path),
            state: state_text,
            spoken,
            help: t(&format!("s_gh_{}_hilfe", device.status.word())),
            salt: format!("hub-{}", device.id),
        };
        list_row(ui, scheme, &row, widths);
    }
    // What came and went since the page was opened: all of it, not only the last.
    let join = |list: &[(String, String)]| list.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", ");
    for (key, list) in [("s_gh_neu", &state.plugged), ("s_gh_weg", &state.unplugged)] {
        if !list.is_empty() {
            let line = tf(key, &[("0", Arg::from(join(list).as_str()))]);
            ui.add(egui::Label::new(RichText::new(line).small().color(color(scheme.gold))).wrap());
        }
    }
    if summary.devices.is_empty() {
        return None;
    }
    // A binding file that can not be read now: what the list says about numbers is not true, and advice built
    // on it would be wrong.
    if look.files.state == joysticks::FileState::Unreadable {
        ui.add_space(GAP);
        small(ui, scheme, &t("s_js_datei_gesperrt"));
        return None;
    }
    if summary.all_well {
        ui.add_space(GAP);
        ui.add(egui::Label::new(RichText::new(t("s_gh_alles_gut")).small().color(color(scheme.accent))).wrap());
        return None;
    }
    ui.add_space(GAP);
    block_title(ui, scheme, &t("s_gh_was_tun"));
    let mut act = None;
    for suggestion in device_hub::suggestions_of(&summary.devices) {
        ui.add_space(GAP);
        let name = named(&suggestion.device);
        match suggestion.step {
            Step::Swap => {
                let Some(old) = suggestion.old.as_ref() else { continue };
                let head = tf("s_gh_tausch", &[("0", Arg::from(name.as_str())), ("1", Arg::from(named(old).as_str()))]);
                crate::blickwinkel_page::verdict_box(ui, scheme, scheme.accent, "info", &head, &t("s_gh_tausch_lang"));
                ui.add_space(GAP / 2.0);
                let wanted = Pending::Swap { old_id: old.id.clone(), new_id: suggestion.device.id.clone() };
                if state.pending.as_ref().is_some_and(|p| same_swap(p, &wanted)) {
                    let text = tf(
                        "s_gh_tausch_frage",
                        &[("0", Arg::from(named(old).as_str())), ("1", Arg::from(name.as_str()))],
                    );
                    if let Some(answer) = question(ui, scheme, &text, false) {
                        act = Some(answer);
                    }
                } else {
                    let label = t("s_gh_tausch_knopf");
                    let button =
                        widgets::Button::new(&label, ButtonKind::Strong, scheme).blocked(look.quiet.as_deref());
                    if ui.add(button).clicked() {
                        act = Some(Act::Ask(wanted));
                    }
                    if let Some(reason) = &look.quiet {
                        small(ui, scheme, reason);
                    }
                }
            }
            Step::Start => {
                let head = tf("s_gh_starten_rat", &[("0", Arg::from(name.as_str()))]);
                crate::blickwinkel_page::verdict_box(ui, scheme, scheme.gold, "info", &head, "");
            }
            Step::PlugIn => {
                let head = tf("s_gh_anstecken_rat", &[("0", Arg::from(name.as_str()))]);
                crate::blickwinkel_page::verdict_box(ui, scheme, scheme.gold, "info", &head, "");
            }
        }
    }
    act
}

/// Is the question under the button the one for this swap (the ids count, not the names or the lines)?
fn same_swap(pending: &Pending, wanted: &Pending) -> bool {
    match (pending, wanted) {
        (Pending::Swap { old_id: a, new_id: b, .. }, Pending::Swap { old_id: c, new_id: d, .. }) => a == c && b == d,
        _ => false,
    }
}

/// The word for a state of the hub.
fn status_text(status: Status) -> String {
    t(&format!("s_gh_{}", status.word()))
}

/// The sticks the game saw, and the state of log and binding in one sentence.
fn comparison(ui: &mut Ui, scheme: &Scheme, state: &JoystickState) {
    let Some(look) = &state.look else { return };
    let compare = &look.files.compare;
    block_title(ui, scheme, &t("s_js_geraete"));
    if compare.devices.is_empty() {
        small(ui, scheme, &t("s_js_leer"));
        return;
    }
    let count = i64::try_from(compare.missing.len()).unwrap_or(i64::MAX);
    let (sentence, hue) = match compare.state {
        State::Matches => (t("s_js_passt"), scheme.accent),
        State::Replaced => (t("s_js_ersetzt"), scheme.gold),
        State::Missing => {
            let key = if count == 1 { "s_js_fehlt_eins" } else { "s_js_fehlt" };
            (tf(key, &[("0", Arg::from(count))]), scheme.gold)
        }
        State::Empty if look.files.state == joysticks::FileState::Unreadable => (t("s_js_datei_gesperrt"), scheme.gold),
        State::Empty => (t("s_js_keine_datei"), scheme.sub),
    };
    ui.add(egui::Label::new(RichText::new(sentence).color(color(hue))).wrap());
    ui.add_space(GAP);
    let (without, missing) = (t("s_js_ohne"), t("s_js_nicht_erkannt"));
    let widths = columns(ui, &[without.clone(), missing.clone()]);
    for device in &compare.devices {
        // Two identical sticks share one id: the last entry counts, like in the hub above and in 4.12.
        let bound = compare.stored.iter().rev().find(|entry| !entry.id.is_empty() && entry.id == device.id);
        let name = if device.name.is_empty() { t("s_gh_unbenannt") } else { device.name.clone() };
        // Only the front part of the id: the rest (`-0000-0000-0000-504944564944`) is the same for all.
        let short = device.id.split('-').next().unwrap_or("");
        let place = tf("s_js_platz", &[("0", Arg::from(i64::try_from(device.slot).unwrap_or(i64::MAX)))]);
        let (number, state_text) = match bound {
            Some(entry) => (format!("js{}", entry.number), String::new()),
            None => (String::new(), without.clone()),
        };
        let spoken = match bound {
            Some(_) => format!("{place}, {number}, {name}"),
            None => format!("{place}, {without}, {name}"),
        };
        let row = Row {
            tone: if bound.is_some() { scheme.accent } else { scheme.gold },
            number,
            number_bound: bound.is_some(),
            name,
            sub: format!("{place} · {short}"),
            state: state_text,
            spoken,
            help: String::new(),
            salt: format!("seen-{}-{}", device.slot, device.id),
        };
        list_row(ui, scheme, &row, widths);
    }
    // Bound devices the game did not see: else one sees only what is there.
    for entry in &compare.missing {
        let name = if entry.name.is_empty() { t("s_gh_unbenannt") } else { entry.name.clone() };
        let number = format!("js{}", entry.number);
        let row = Row {
            tone: scheme.gold,
            spoken: format!("{number}, {name}, {missing}"),
            number,
            number_bound: true,
            name,
            sub: entry.id.split('-').next().unwrap_or("").to_string(),
            state: missing.clone(),
            help: String::new(),
            salt: format!("missing-{}-{}", entry.number, entry.id),
        };
        list_row(ui, scheme, &row, widths);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use egui::{Pos2, Rect};
    use versekit_core::language::{self, Choice};
    use versekit_core::paths::{ENV_INSTALL_DIR, Host};
    use versekit_core::pypath::Flavor;
    use versekit_core::theme;

    use super::*;

    type Texts = Vec<(String, Pos2)>;

    const LEFT: &str = "1234ABCD-0000-0000-0000-504944564944";
    const RIGHT: &str = "3344ABCD-0000-0000-0000-504944564944";
    const NEW: &str = "99990000-0000-0000-0000-504944564944";

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-joypage-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn maps(entries: &[(u32, &str, &str)]) -> String {
        let mut text = String::from("<ActionMaps>\n <ActionProfiles version=\"1\">\n");
        for (number, name, id) in entries {
            text.push_str(&format!(
                "  <options type=\"joystick\" instance=\"{number}\" Product=\" {name}  {{{id}}}\"/>\n"
            ));
        }
        text.push_str(" </ActionProfiles>\n</ActionMaps>\n");
        text
    }

    fn log(entries: &[(u32, &str, &str)]) -> String {
        entries.iter().map(|(slot, name, id)| format!("Connected joystick{slot}:  {name}  {{{id}}}\n")).collect()
    }

    /// A made-up installation under a throwaway home.
    fn world(name: &str, log_text: &str, maps_text: Option<&str>) -> (Paths, String, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(format!("{game}/Game.log"), log_text).expect("log");
        if let Some(text) = maps_text {
            std::fs::write(format!("{profile}/actionmaps.xml"), text).expect("maps");
        }
        // The game lies quiet for two hours, and the process list is not asked (a game that runs on this
        // computer must not change what the test sees).
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
        (Paths::new(Host::new(Flavor::Posix, &vars, &home)), home, profile)
    }

    fn stick(name: &str, id: &str) -> SystemDevice {
        SystemDevice { path: format!("\\\\?\\HID#VID_3344&PID_43F5#{name}"), name: name.into(), id: id.into() }
    }

    struct Bench {
        ctx: egui::Context,
        state: JoystickState,
        paths: Paths,
        home: String,
        time: f64,
        system: Arc<Mutex<Vec<SystemDevice>>>,
        /// Every text of the last frame with the place it takes.
        rects: Vec<(String, Rect)>,
        /// The nodes of the last frame for a screen reader: role and name.
        nodes: Vec<(egui::Role, String)>,
        /// The nodes of the last frame for a screen reader that have a place: name and place.
        places: Vec<(String, Rect)>,
        /// The height of the window in points (what lies below it is not drawn).
        height: f32,
    }

    impl Bench {
        fn new(name: &str, log_text: &str, maps_text: Option<&str>, system: Vec<SystemDevice>) -> Bench {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx.enable_accesskit();
            let (paths, home, _) = world(name, log_text, maps_text);
            let system = Arc::new(Mutex::new(system));
            let source = Arc::clone(&system);
            Bench {
                ctx,
                state: JoystickState::with_source(move || source.lock().expect("lock").clone()),
                paths,
                home,
                time: 10.0,
                system,
                rects: Vec::new(),
                nodes: Vec::new(),
                places: Vec::new(),
                height: 4000.0,
            }
        }

        fn frame(&mut self) -> Texts {
            self.frame_with(Vec::new())
        }

        /// Clicks the first text that is `label` (the middle of it).
        fn click(&mut self, label: &str) {
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
            self.settle();
        }

        /// [`Bench::click`], then the texts.
        fn click_and_settle(&mut self, label: &str) -> Texts {
            self.click(label);
            self.settle()
        }

        fn frame_with(&mut self, events: Vec<egui::Event>) -> Texts {
            let scheme = theme::current();
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, self.height))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let (state, paths) = (&mut self.state, &self.paths);
            let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state, paths));
            output.textures_delta.clear();
            let mut texts = Vec::new();
            let mut rects = Vec::new();
            fn collect(shape: &egui::Shape, into: &mut Texts, rects: &mut Vec<(String, Rect)>) {
                match shape {
                    egui::Shape::Text(text) => {
                        into.push((text.galley.text().to_string(), text.pos));
                        rects.push((text.galley.text().to_string(), Rect::from_min_size(text.pos, text.galley.size())));
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into, rects)),
                    _ => {}
                }
            }
            output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts, &mut rects));
            self.rects = rects;
            let nodes = output.platform_output.accesskit_update.map(|update| update.nodes).unwrap_or_default();
            self.nodes =
                nodes.iter().map(|(_, node)| (node.role(), node.label().unwrap_or_default().to_string())).collect();
            self.places = nodes
                .iter()
                .filter_map(|(_, node)| {
                    let b = node.bounds()?;
                    let place =
                        Rect::from_min_max(Pos2::new(b.x0 as f32, b.y0 as f32), Pos2::new(b.x1 as f32, b.y1 as f32));
                    Some((node.label().unwrap_or_default().to_string(), place))
                })
                .collect();
            texts
        }

        /// Clicks the middle of the first place a screen reader knows by `name`.
        fn click_node(&mut self, name: &str) {
            self.settle();
            let at = self
                .places
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{name} is not known to a screen reader: {:?}", self.nodes));
            self.click_at(at);
        }

        /// Clicks the last text that is `label` (a drop-down draws its entries above the page).
        fn click_last(&mut self, label: &str) {
            let texts = self.settle();
            let at = self
                .rects
                .iter()
                .rev()
                .find(|(text, _)| text == label)
                .map(|(_, rect)| rect.center())
                .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
            self.click_at(at);
        }

        fn click_at(&mut self, at: Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame_with(vec![egui::Event::PointerMoved(at)]);
            self.frame_with(vec![button(true)]);
            self.frame_with(vec![button(false)]);
            self.settle();
        }

        /// Frames until the look in the thread has come, then a few more. At most a minute: a busy processor
        /// lets the thread of a look take its time, and a look that never comes is told by the asserts after it.
        fn settle(&mut self) -> Texts {
            let mut texts = Vec::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while std::time::Instant::now() < deadline {
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

        /// The time goes on by `seconds` (the next look is due), then settle.
        fn later(&mut self, seconds: f64) -> Texts {
            self.time += seconds;
            self.settle()
        }

        fn set_system(&self, system: Vec<SystemDevice>) {
            *self.system.lock().expect("lock") = system;
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
    fn a_setup_where_all_is_right_shows_the_numbers_of_the_game_and_says_so() {
        let _guard = setup();
        let both = [(0, "Stick L", LEFT), (1, "Stick R", RIGHT)];
        let mut bench = Bench::new(
            "alles-gut",
            &log(&both),
            Some(&maps(&[(2, "Stick L", LEFT), (1, "Stick R", RIGHT)])),
            vec![stick("Stick L", LEFT), stick("Stick R", RIGHT)],
        );
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_gh_titel")) && shows(&texts, &t("s_js_lead")), "{texts:?}");
        // The number of the game, not the order of the system.
        assert!(shows(&texts, "js2") && shows(&texts, "js1"), "{texts:?}");
        assert_eq!(texts.iter().filter(|(s, _)| *s == t("s_gh_bereit")).count(), 2, "both ready: {texts:?}");
        assert!(shows(&texts, &t("s_gh_alles_gut")) && !shows(&texts, &t("s_gh_was_tun")), "{texts:?}");
        assert!(shows(&texts, &t("s_js_passt")), "the sentence of the comparison: {texts:?}");
        let vendor = tf("s_gh_vidpid", &[("0", Arg::from("3344")), ("1", Arg::from("43F5"))]);
        assert!(shows(&texts, &vendor), "the path of the system as vendor and product: {texts:?}");
        // The games sees the first as "Platz 0" and "Platz 1", bound to js2 and js1.
        assert!(shows(&texts, &tf("s_js_platz", &[("0", Arg::from(0))])), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_stick_with_a_new_id_is_offered_as_one_swap_and_not_as_two_problems() {
        let _guard = setup();
        let mut bench = Bench::new(
            "tausch",
            &log(&[(0, "Stick L", LEFT), (1, "Stick R", NEW)]),
            Some(&maps(&[(1, "Stick L", LEFT), (2, "Stick R", RIGHT)])),
            vec![stick("Stick L", LEFT), stick("Stick R", NEW)],
        );
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_gh_was_tun")), "{texts:?}");
        let head = tf("s_gh_tausch", &[("0", Arg::from("Stick R")), ("1", Arg::from("Stick R"))]);
        assert!(shows(&texts, &head), "{head}: {texts:?}");
        assert!(shows(&texts, t("s_gh_tausch_lang").lines().next().unwrap_or_default()), "{texts:?}");
        assert!(shows(&texts, &t("s_js_ersetzt")), "the comparison says the same: {texts:?}");
        // The old entry and the new device stand in the list.
        assert!(shows(&texts, &t("s_gh_abgesteckt")) && shows(&texts, &t("s_gh_ohne_nummer")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn with_two_missing_nothing_is_guessed_and_each_gets_its_own_advice() {
        let _guard = setup();
        let mut bench = Bench::new(
            "zwei-fehlen",
            &log(&[(0, "Stick L", LEFT)]),
            Some(&maps(&[(1, "Stick L", LEFT), (2, "Stick R", RIGHT), (3, "Pedale", NEW)])),
            vec![stick("Stick L", LEFT)],
        );
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_gh_tausch_lang")), "no swap: {texts:?}");
        let advice = tf("s_gh_anstecken_rat", &[("0", Arg::from("Stick R"))]);
        assert!(shows(&texts, &advice), "{advice}: {texts:?}");
        assert!(shows(&texts, &tf("s_js_fehlt", &[("0", Arg::from(2))])), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_stick_the_game_has_never_seen_is_told_to_start_the_game() {
        let _guard = setup();
        let mut bench = Bench::new(
            "neu",
            &log(&[(0, "Stick L", LEFT)]),
            Some(&maps(&[(1, "Stick L", LEFT)])),
            vec![stick("Stick L", LEFT), stick("Brandneu", NEW)],
        );
        let texts = bench.settle();
        let advice = tf("s_gh_starten_rat", &[("0", Arg::from("Brandneu"))]);
        assert!(shows(&texts, &advice), "{advice}: {texts:?}");
        assert!(shows(&texts, &t("s_gh_unbekannt")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn what_is_plugged_in_and_pulled_while_the_page_is_open_is_told_and_all_of_it_stays() {
        let _guard = setup();
        let mut bench = Bench::new(
            "stecken",
            &log(&[(0, "Stick L", LEFT)]),
            Some(&maps(&[(1, "Stick L", LEFT)])),
            vec![stick("Stick L", LEFT)],
        );
        let texts = bench.settle();
        let (came_word, went_word) = (prefix("s_gh_neu"), prefix("s_gh_weg"));
        assert!(!shows(&texts, &came_word) && !shows(&texts, &went_word), "the first look only sets the basis");
        bench.set_system(vec![stick("Stick L", LEFT), stick("Pedale", NEW)]);
        let texts = bench.later(4.0);
        assert!(shows(&texts, &tf("s_gh_neu", &[("0", Arg::from("Pedale"))])), "{texts:?}");
        bench.set_system(vec![stick("Pedale", NEW)]);
        let texts = bench.later(4.0);
        assert!(shows(&texts, &tf("s_gh_neu", &[("0", Arg::from("Pedale"))])), "still told: {texts:?}");
        assert!(shows(&texts, &tf("s_gh_weg", &[("0", Arg::from("Stick L"))])), "{texts:?}");
        // Pulled and plugged in again: it leaves the list of the pulled ones.
        bench.set_system(vec![stick("Pedale", NEW), stick("Stick L", LEFT)]);
        let texts = bench.later(4.0);
        assert!(!shows(&texts, &went_word), "{texts:?}");
        assert!(shows(&texts, &tf("s_gh_neu", &[("0", Arg::from("Pedale, Stick L"))])), "{texts:?}");
        // A long absence: the page starts afresh, and what happens while it is away is not told as news.
        bench.set_system(vec![stick("Pedale", NEW)]);
        let texts = bench.later(60.0);
        assert!(!shows(&texts, &came_word) && !shows(&texts, &went_word), "nothing is kept: {texts:?}");
        bench.set_system(vec![stick("Pedale", NEW), stick("Stick L", LEFT)]);
        let texts = bench.later(4.0);
        assert!(
            shows(&texts, &tf("s_gh_neu", &[("0", Arg::from("Stick L"))])),
            "the basis is the new visit: {texts:?}"
        );
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    /// The text of a message up to its first place for a value ("Angesteckt: ").
    fn prefix(key: &str) -> String {
        t(key).split('{').next().unwrap_or_default().to_string()
    }

    #[test]
    fn an_answer_still_on_its_way_from_an_earlier_visit_is_not_taken_for_this_one() {
        let _guard = setup();
        let mut bench = Bench::new(
            "verwaist",
            &log(&[(0, "Stick L", LEFT)]),
            Some(&maps(&[(1, "Stick L", LEFT)])),
            vec![stick("Stick L", LEFT)],
        );
        bench.settle();
        // A look is started, the page is left before it is taken, and shown again after a long time.
        bench.set_system(vec![stick("Stick L", LEFT), stick("Pedale", NEW)]);
        bench.time += POLL_SECONDS + 0.1;
        bench.frame();
        assert!(bench.state.job.is_some(), "a look is under way");
        std::thread::sleep(std::time::Duration::from_millis(300));
        bench.time += 60.0;
        bench.frame();
        let texts = bench.settle();
        assert!(!shows(&texts, &prefix("s_gh_neu")), "the old answer is dropped: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_look_that_never_answers_is_given_up_and_a_new_one_is_asked() {
        let _guard = setup();
        let mut bench = Bench::new("haengt", "", None, Vec::new());
        // The source hangs for this first call only.
        let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag = Arc::clone(&first);
        bench.state = JoystickState::with_source(move || {
            if flag.swap(false, std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            Vec::new()
        });
        bench.frame();
        assert!(bench.state.job.is_some());
        bench.time += LOOK_GIVE_UP_SECONDS + 1.0;
        bench.frame();
        let texts = bench.settle();
        assert!(bench.state.look.is_some(), "the second look answered: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_binding_file_that_cannot_be_read_is_said_so_and_not_taken_for_no_binding() {
        let _guard = setup();
        let mut bench = Bench::new(
            "gesperrt",
            &log(&[(0, "Stick L", LEFT)]),
            Some("<ActionMaps><options"),
            vec![stick("Stick L", LEFT)],
        );
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_datei_gesperrt")), "{texts:?}");
        assert!(!shows(&texts, &t("s_js_keine_datei")), "the file is there: {texts:?}");
        assert!(!shows(&texts, &t("s_gh_was_tun")), "no advice built on numbers that are not true: {texts:?}");
        // The game finishes writing: the same mark would hold, but the broken result is not kept.
        let (_, _, profile) = (0, 0, format!("{}/StarCitizen/LIVE/USER/Client/0/Profiles/default", bench.home));
        std::fs::write(format!("{profile}/actionmaps.xml"), maps(&[(1, "Stick L", LEFT)])).expect("write");
        let texts = bench.later(4.0);
        assert!(shows(&texts, &t("s_gh_alles_gut")), "read again: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn two_identical_sticks_show_the_same_number_in_the_hub_and_in_the_list_below() {
        let _guard = setup();
        // One id for both (identical sticks): the last entry counts in both places.
        let mut bench = Bench::new(
            "baugleich",
            &log(&[(0, "Stick", LEFT), (1, "Stick", LEFT)]),
            Some(&maps(&[(1, "Stick", LEFT), (2, "Stick", LEFT)])),
            vec![stick("Stick", LEFT)],
        );
        let texts = bench.settle();
        assert!(!shows(&texts, "js1"), "{texts:?}");
        assert_eq!(texts.iter().filter(|(s, _)| s == "js2").count(), 3, "hub and both places: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_line_is_one_stop_for_a_screen_reader_with_the_whole_sentence_and_the_state_has_a_word() {
        let _guard = setup();
        let mut bench = Bench::new(
            "sprecher",
            &log(&[(0, "Stick L", LEFT), (1, "Stick R", NEW)]),
            Some(&maps(&[(1, "Stick L", LEFT), (2, "Stick R", RIGHT)])),
            vec![stick("Stick L", LEFT), stick("Stick R", NEW)],
        );
        bench.settle();
        let said = |text: &str| bench.nodes.iter().any(|(role, name)| *role == egui::Role::ListItem && name == text);
        assert!(said(&format!("js1, Stick L, {}", t("s_gh_bereit"))), "{:?}", bench.nodes);
        assert!(said(&format!("js2, Stick R, {}", t("s_gh_abgesteckt"))), "{:?}", bench.nodes);
        assert!(said(&format!("Stick R, {}", t("s_gh_ohne_nummer"))), "{:?}", bench.nodes);
        // The list of the game: the one not detected has a word, not only a colour.
        assert!(said(&format!("js2, Stick R, {}", t("s_js_nicht_erkannt"))), "{:?}", bench.nodes);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_states_stand_in_one_column_whatever_the_names_are() {
        let _guard = setup();
        let long = "x".repeat(90);
        let mut bench = Bench::new(
            "spalten",
            &log(&[(0, "Kurz", LEFT), (1, &long, RIGHT)]),
            Some(&maps(&[(1, "Kurz", LEFT), (2, &long, RIGHT)])),
            vec![stick("Kurz", LEFT), stick(&long, RIGHT)],
        );
        bench.settle();
        let ready: Vec<f32> =
            bench.rects.iter().filter(|(s, _)| *s == t("s_gh_bereit")).map(|(_, rect)| rect.right()).collect();
        assert_eq!(ready.len(), 2, "{:?}", bench.rects);
        assert!((ready[0] - ready[1]).abs() < 0.5, "the right edge of the state is the same: {ready:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn one_missing_device_is_said_in_the_singular() {
        let _guard = setup();
        let mut bench = Bench::new(
            "einzahl",
            &log(&[(0, "Stick L", LEFT)]),
            Some(&maps(&[(1, "Stick L", LEFT), (2, "Stick R", RIGHT)])),
            vec![stick("Stick L", LEFT)],
        );
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_fehlt_eins")), "{texts:?}");
        assert!(!shows(&texts, "(s)"), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn without_any_stick_or_file_the_page_says_what_to_do() {
        let _guard = setup();
        let mut bench = Bench::new("leer", "", None, Vec::new());
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_gh_kein_geraet")) && shows(&texts, &t("s_js_leer")), "{texts:?}");
        assert!(!shows(&texts, &t("s_gh_alles_gut")) && !shows(&texts, &t("s_gh_was_tun")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_files_are_read_again_only_when_they_changed() {
        let (paths, home, profile) =
            world("stempel", &log(&[(0, "Stick L", LEFT)]), Some(&maps(&[(1, "Stick L", LEFT)])));
        let source = || Vec::new();
        let first = read_look(&paths, &source, None);
        assert_eq!(first.files.stored.len(), 1);
        let again = read_look(&paths, &source, Some(Arc::clone(&first.files)));
        assert!(Arc::ptr_eq(&first.files, &again.files), "nothing changed: not read again");
        // The game wrote the binding anew.
        std::fs::write(format!("{profile}/actionmaps.xml"), maps(&[(1, "Stick L", LEFT), (2, "Stick R", RIGHT)]))
            .expect("write");
        let changed = read_look(&paths, &source, Some(Arc::clone(&first.files)));
        assert!(!Arc::ptr_eq(&first.files, &changed.files) && changed.files.stored.len() == 2, "read again");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_path_of_a_device_is_cut_to_what_one_recognises() {
        let _guard = setup();
        assert_eq!(system_path_label(""), "", "no path, no second line");
        // Only a state that is right is in the colour of the brand; a stick that is pulled out is no fault (red).
        for scheme in theme::schemes() {
            assert_eq!(status_color(scheme, Status::Ready), scheme.accent);
            for status in [Status::NoNumber, Status::Unknown, Status::Unplugged] {
                assert_eq!(status_color(scheme, status), scheme.gold, "{}", scheme.id);
            }
        }
        assert_eq!(system_path_label("/dev/input/js0"), "/dev/input/js0");
        assert_eq!(
            system_path_label("\\\\?\\HID#VID_3344&PID_43f5&Col01#7&2b#{4d1e55b2}"),
            tf("s_gh_vidpid", &[("0", Arg::from("3344")), ("1", Arg::from("43F5"))]),
            "capital letters"
        );
        let long = "x".repeat(60);
        assert_eq!(system_path_label(&long), format!("{}…", "x".repeat(22)));
        assert_eq!(system_path_label("VID_33&PID_43F5"), "VID_33&PID_43F5", "short numbers are no numbers");
    }

    // ----------------------------------------------------------------------- the changes

    fn actionmaps(home: &str) -> String {
        format!("{home}/StarCitizen/LIVE/USER/Client/0/Profiles/default/actionmaps.xml")
    }

    /// The case of a stick with a new id: the right one is bound under `RIGHT`, the system has it as `NEW`.
    fn swap_bench(name: &str) -> Bench {
        let maps = "<ActionMaps><ActionProfiles>\n <options type=\"joystick\" instance=\"1\" Product=\" Stick L  {1234ABCD-0000-0000-0000-504944564944}\"/>\n <options type=\"joystick\" instance=\"2\" Product=\" Stick R  {3344ABCD-0000-0000-0000-504944564944}\"/>\n <actionmap name=\"a\"><action name=\"b\"><rebind input=\"js2_button1\"/></action></actionmap>\n</ActionProfiles></ActionMaps>\n";
        Bench::new(
            name,
            &log(&[(0, "Stick L", LEFT), (1, "Stick R", NEW)]),
            Some(maps),
            vec![stick("Stick L", LEFT), stick("Stick R", NEW)],
        )
    }

    #[test]
    fn the_swap_is_asked_first_and_then_made_with_a_copy_and_the_page_reads_the_file_anew() {
        let _guard = setup();
        let mut bench = swap_bench("umhaengen");
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click(&t("s_gh_tausch_knopf"));
        let texts = bench.settle();
        let question = tf("s_gh_tausch_frage", &[("0", Arg::from("Stick R")), ("1", Arg::from("Stick R"))]);
        let first_line = question.lines().next().unwrap_or_default().to_string();
        assert!(shows(&texts, &first_line), "the question names both devices and the lines: {texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before, "asked, nothing written");
        // No: the question goes, the file stays.
        bench.click(&t("abbrechen"));
        let texts = bench.settle();
        assert!(!shows(&texts, &first_line) && shows(&texts, &t("s_gh_tausch_knopf")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before);
        // Yes: written.
        bench.click(&t("s_gh_tausch_knopf"));
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains(NEW) && !after.contains(RIGHT), "{after}");
        assert_eq!(versekit_core::game_files::backups(&file).len(), 1, "a copy was made first");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.contains("actionmaps.xml.versekit-"), "{said:?}");
        // The page has read the file again: nothing left to do.
        let texts = bench.later(4.0);
        assert!(shows(&texts, &t("s_gh_alles_gut")) && !shows(&texts, &t("s_gh_tausch_knopf")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn while_the_game_runs_the_buttons_are_blocked_and_say_why_and_nothing_is_written() {
        let _guard = setup();
        let mut bench = swap_bench("laeuft");
        // The log was written just now: the game runs.
        std::fs::write(format!("{}/StarCitizen/LIVE/Game.log", bench.home), log(&[(0, "Stick L", LEFT)])).expect("log");
        bench.state.invalidate();
        let texts = bench.later(4.0);
        let reason = t("s_gf_spiel");
        assert!(shows(&texts, &reason), "the reason stands beside the button: {texts:?}");
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click(&t("s_gh_tausch_knopf"));
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "a blocked button asks nothing: {texts:?}");
        bench.click(&t("s_js_zurueck"));
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_reset_names_the_number_of_the_bindings_that_go_and_keeps_the_settings_of_the_devices() {
        let _guard = setup();
        let mut bench = list_bench("zurueck", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click(&t("s_js_zurueck"));
        let texts = bench.settle();
        let question = tf("s_js_zurueck_frage_eins", &[("0", Arg::from(1))]);
        assert!(shows(&texts, question.lines().next().unwrap_or_default()), "{texts:?}");
        assert!(shows(&texts, "einzige eigene Belegung"), "the singular: {texts:?}");
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(!after.contains("actionmap") && after.contains("instance=\"1\""), "{after}");
        assert_eq!(versekit_core::game_files::backups(&file).len(), 1);
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.starts_with("Zurückgesetzt"), "{said:?}");
        // Nothing left to throw away: the button is blocked and says why, no question about "0 bindings".
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_zurueck_leer")), "the reason stands beside the button: {texts:?}");
        bench.click(&t("s_js_zurueck"));
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_mlg_bestaetigen")), "a blocked button asks nothing: {texts:?}");
        assert_eq!(versekit_core::game_files::backups(&file).len(), 1, "no second copy");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_page_reads_at_once_after_the_swap_and_keeps_what_it_shows() {
        let _guard = setup();
        let mut bench = swap_bench("zzumhaengen");
        bench.click(&t("s_gh_tausch_knopf"));
        // The yes by hand, so that the frame right after it can be looked at.
        let at = bench
            .rects
            .iter()
            .find(|(s, _)| *s == t("s_mlg_bestaetigen"))
            .map(|(_, r)| r.center())
            .expect("confirm button");
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame_with(vec![egui::Event::PointerMoved(at)]);
        bench.frame_with(vec![button(true)]);
        bench.frame_with(vec![button(false)]);
        let next = bench.frame();
        assert!(!shows(&next, &t("s_gh_liest")), "what is shown stays until the new look is there: {next:?}");
        // Right after the yes, with no time gone by: the page has read the file again.
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_gh_alles_gut")) && !shows(&texts, &t("s_gh_tausch_knopf")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    // ------------------------------------------------------------------ the window "Neu belegen"

    fn press(bench: &mut Bench, key: egui::Key) {
        press_with(bench, key, egui::Modifiers::NONE);
    }

    fn press_with(bench: &mut Bench, key: egui::Key, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers };
        bench.frame_with(vec![event(true)]);
        bench.frame_with(vec![event(false)]);
        bench.settle();
    }

    /// A wait for a press that answers once with a press of button 3 of the stick `id`, then waits like the
    /// real one until it is told to stop.
    fn stick_press(id: &str) -> crate::joysticks_bind::Waiter {
        let (id, sent) = (id.to_string(), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
        std::sync::Arc::new(move |_, stop| {
            if !sent.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return Some(versekit_core::input_device::Pressed {
                    id: id.clone(),
                    input: "button3".to_string(),
                    name: "Stick L".to_string(),
                });
            }
            while !stop() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            None
        })
    }

    /// Frames until the text stands there (the thread of the stick needs real time).
    fn until_shown(bench: &mut Bench, text: &str) -> Texts {
        let mut texts = bench.settle();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !shows(&texts, text) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            texts = bench.later(0.2);
        }
        texts
    }

    #[test]
    fn a_press_on_a_stick_is_shown_as_the_number_the_game_gives_it() {
        let _guard = setup();
        let mut bench = list_bench("stick", &own_maps(""));
        bench.state.list.with_waiter(stick_press(LEFT));
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click("Knopf 5");
        let texts = until_shown(&mut bench, "js1_button3");
        assert!(shows(&texts, "js1_button3") && shows(&texts, &t("s_js_b_nochmal")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before, "detected, nothing written");
        bench.click(&t("s_js_b_uebernehmen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("js1_button3") && !after.contains("js1_button5"), "{after}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn two_equal_sticks_are_not_guessed() {
        let _guard = setup();
        let maps = own_maps("").replace(
            "<actionmap name=\"spaceship_general\">",
            &format!("<options type=\"joystick\" instance=\"2\" Product=\" Stick L  {{{LEFT}}}\"/>\n <actionmap name=\"spaceship_general\">"),
        );
        let mut bench = list_bench("zwei-gleiche", &maps);
        bench.state.list.with_waiter(stick_press(LEFT));
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click("Knopf 5");
        let texts = until_shown(&mut bench, &t("s_js_b_gleich"));
        assert!(shows(&texts, &t("s_js_b_gleich")), "{texts:?}");
        assert!(!shows(&texts, "js1_button3") && !shows(&texts, "js2_button3"), "no number guessed: {texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_wait_that_ends_at_once_is_not_started_again_and_again() {
        let _guard = setup();
        let mut bench = list_bench("sofort", &own_maps(""));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&calls);
        bench.state.list.with_waiter(std::sync::Arc::new(move |_, _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        }));
        bench.click("Knopf 5");
        let texts = until_shown(&mut bench, &t("s_js_b_nur_tastatur"));
        assert!(shows(&texts, &t("s_js_b_nur_tastatur")), "keyboard and mouse are said to go: {texts:?}");
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            bench.later(0.2);
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1, "asked once, not in every frame");
        // The keyboard still goes.
        press(&mut bench, egui::Key::F5);
        assert!(shows(&bench.settle(), "kb1_f5"));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_key_with_modifiers_and_a_modifier_alone_are_taken_by_their_side_and_written() {
        let _guard = setup();
        let mut bench = list_bench("kombi", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click("Knopf 5");
        // Right Shift alone: counts when let go.
        press_with(&mut bench, egui::Key::ShiftRight, egui::Modifiers::SHIFT);
        let texts = bench.settle();
        assert!(shows(&texts, "kb1_rshift") && shows(&texts, &t("s_js_t_rshift")), "{texts:?}");
        // Right Ctrl held, S pressed: joined with the side.
        let ctrl_down = egui::Event::Key {
            key: egui::Key::ControlRight,
            physical_key: Some(egui::Key::ControlRight),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        bench.frame_with(vec![ctrl_down]);
        press_with(&mut bench, egui::Key::S, egui::Modifiers::CTRL);
        let texts = bench.settle();
        assert!(shows(&texts, "kb1_rctrl+s") && shows(&texts, &t("s_js_b_nochmal")), "{texts:?}");
        bench.click(&t("s_js_b_uebernehmen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("kb1_rctrl+s"), "written as the game writes it: {after}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_key_with_the_windows_key_is_refused_and_said() {
        let _guard = setup();
        let mut bench = list_bench("systemtaste", &own_maps(""));
        bench.click("Knopf 5");
        let win_down = egui::Event::Key {
            key: egui::Key::SuperLeft,
            physical_key: Some(egui::Key::SuperLeft),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame_with(vec![win_down]);
        press(&mut bench, egui::Key::S);
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_b_systemtaste")), "{texts:?}");
        assert!(!shows(&texts, "kb1_s") && !shows(&texts, &t("s_js_b_nochmal")), "nothing detected: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_window_is_worked_by_the_keyboard_tab_leaves_the_box_and_enter_presses_the_button() {
        let _guard = setup();
        let mut bench = list_bench("tastatur", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click("Knopf 5");
        // The box has the focus when the window opens: the key is taken.
        press(&mut bench, egui::Key::F9);
        assert!(shows(&bench.settle(), "kb1_f9"));
        // Tab moves on to "Übernehmen", Enter presses it: written, window closed.
        press(&mut bench, egui::Key::Tab);
        press(&mut bench, egui::Key::Enter);
        let texts = bench.later(4.0);
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("kb1_f9"), "written with the keyboard alone: {after}\n{texts:?}");
        assert!(!shows(&texts, &t("s_js_b_titel")), "closed: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_click_beside_the_window_closes_it_like_every_other_dialog() {
        let _guard = setup();
        let mut bench = list_bench("daneben", &own_maps(""));
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click("Knopf 5");
        press(&mut bench, egui::Key::F9);
        let at = egui::pos2(3.0, 3.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame_with(vec![egui::Event::PointerMoved(at)]);
        bench.frame_with(vec![button(true)]);
        bench.frame_with(vec![button(false)]);
        assert!(!shows(&bench.settle(), &t("s_js_b_titel")), "closed");
        assert_eq!(std::fs::read(&file).expect("file"), before, "nothing written");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_line_is_bound_anew_by_a_key_and_only_the_click_on_take_over_writes() {
        let _guard = setup();
        let mut bench = list_bench("belegen", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click("Knopf 5");
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_b_titel")) && shows(&texts, &t("s_js_b_nur_tastatur")), "{texts:?}");
        assert!(shows(&texts, &tf("s_js_b_bisher", &[("0", Arg::from("Knopf 5"))])), "what lay there: {texts:?}");
        assert!(shows(&texts, &t("s_js_b_leer")), "{texts:?}");
        let before = std::fs::read(&file).expect("file");
        // A key is detected and shown, not written.
        press(&mut bench, egui::Key::F5);
        let texts = bench.settle();
        assert!(shows(&texts, "F5") && shows(&texts, &t("s_js_b_nochmal")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before, "detected, nothing written");
        // Mistyped: pressed again, the last one counts.
        press(&mut bench, egui::Key::F6);
        let texts = bench.settle();
        assert!(shows(&texts, "kb1_f6") && !shows(&texts, "kb1_f5"), "{texts:?}");
        bench.click(&t("s_js_b_uebernehmen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("kb1_f6") && !after.contains("kb1_f5"), "{after}");
        assert!(after.contains("js1_button5"), "the binding on the stick stays: {after}");
        assert_eq!(versekit_core::game_files::backups(&file).len(), 1, "a copy was made first");
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.contains("actionmaps.xml.versekit-"), "{said:?}");
        let texts = bench.later(4.0);
        assert!(!shows(&texts, &t("s_js_b_titel")), "the window is closed: {texts:?}");
        assert!(shows(&texts, "F6"), "the list shows the new line: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn escape_closes_the_window_and_is_not_a_binding_and_cancel_writes_nothing() {
        let _guard = setup();
        let mut bench = list_bench("abbruch", &own_maps(""));
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click("Knopf 5");
        press(&mut bench, egui::Key::Escape);
        assert!(!shows(&bench.settle(), &t("s_js_b_titel")), "closed by Escape");
        bench.click("Knopf 5");
        press(&mut bench, egui::Key::A);
        bench.click(&t("s_js_b_abbruch"));
        assert!(!shows(&bench.settle(), &t("s_js_b_titel")), "closed by Cancel");
        assert_eq!(std::fs::read(&file).expect("file"), before);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_mouse_button_counts_only_in_the_box_and_the_keys_do_not_press_the_buttons_of_the_page() {
        let _guard = setup();
        let mut bench = list_bench("maus", &own_maps(""));
        bench.click("Knopf 5");
        // A click on a button of the window is no binding.
        let texts = bench.settle();
        assert!(!shows(&texts, &t("s_js_b_nochmal")), "{texts:?}");
        // The right button in the box is "mouse2" (the game counts middle and right the other way round).
        let at = bench.rects.iter().find(|(s, _)| *s == t("s_js_b_leer")).map(|(_, r)| r.center()).expect("box");
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        bench.frame_with(vec![egui::Event::PointerMoved(at)]);
        bench.frame_with(vec![button(true)]);
        bench.frame_with(vec![button(false)]);
        let texts = bench.settle();
        assert!(shows(&texts, "mo1_mouse2") && shows(&texts, &t("s_js_b_nochmal")), "{texts:?}");
        // Space and Enter are keys to bind, they do not press "Übernehmen" that has the focus.
        press(&mut bench, egui::Key::Space);
        let texts = bench.settle();
        assert!(shows(&texts, "kb1_space") && shows(&texts, &t("s_js_b_titel")), "{texts:?}");
        press(&mut bench, egui::Key::Enter);
        assert!(shows(&bench.settle(), "kb1_enter"), "the window stays open");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn who_already_sits_on_the_input_is_said_before_it_is_bound() {
        let _guard = setup();
        let mut bench = list_bench("konflikt", &own_maps(""));
        bench.click("Knopf 5");
        press(&mut bench, egui::Key::F);
        let texts = bench.settle();
        assert!(shows(&texts, &tf("s_js_b_konflikt", &[("0", Arg::from("Wiederbeleben"))])), "{texts:?}");
        press(&mut bench, egui::Key::G);
        assert!(!shows(&bench.settle(), "Liegt schon auf"), "no conflict on a free key");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_binding_is_removed_the_way_the_game_understands_it_and_not_while_the_game_runs() {
        let _guard = setup();
        let mut bench = list_bench("entfernen", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click("Knopf 5");
        bench.click(&t("s_js_b_loeschen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("input=\"js1_\""), "an empty input is a deliberate nothing: {after}");
        let _ = std::fs::remove_dir_all(&bench.home);

        // The game runs: both buttons are blocked, the reason stands there, nothing is written.
        let mut bench = list_bench("entfernen-laeuft", &own_maps(""));
        std::fs::write(format!("{}/StarCitizen/LIVE/Game.log", bench.home), log(&[(0, "Stick L", LEFT)])).expect("log");
        // The log is another file now: the page reads again and finds out that the game runs.
        bench.later(4.0);
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click("Knopf 5");
        press(&mut bench, egui::Key::F9);
        bench.click(&t("s_js_b_uebernehmen"));
        let texts = bench.click_and_settle(&t("s_js_b_loeschen"));
        assert!(shows(&texts, &t("s_gf_spiel")) && shows(&texts, &t("s_js_b_titel")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    // ------------------------------------------------------------ the buttons above the list

    use crate::file_dialog::{Filters, Pending};

    /// What the dialogs of the tests answer: where to save, and what to open.
    static ANSWER: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

    fn saves(_: &egui::Context, _: &str, _: Filters, _: Option<&str>, _: &str) -> Pending {
        Pending::answered(Some(ANSWER.lock().expect("lock").0.clone()))
    }

    fn opens(_: &egui::Context, _: &str, _: Filters, _: Option<&str>) -> Pending {
        Pending::answered(Some(ANSWER.lock().expect("lock").1.clone()))
    }

    fn tools_bench(name: &str) -> Bench {
        let mut bench = list_bench(name, &own_maps(""));
        bench.state.list.with_dialogs(opens, saves);
        bench
    }

    fn type_into_the_field(bench: &mut Bench, text: &str) {
        bench.frame_with(vec![egui::Event::Text(text.to_string())]);
        bench.settle();
    }

    fn enter(bench: &mut Bench) {
        press(bench, egui::Key::Enter);
    }

    fn mappings_folder(home: &str) -> String {
        format!("{home}/StarCitizen/LIVE/USER/Client/0/controls/mappings")
    }

    #[test]
    fn a_profile_is_saved_under_a_name_that_is_checked_and_replaced_only_after_a_question() {
        let _guard = setup();
        let mut bench = tools_bench("profil");
        let folder = mappings_folder(&bench.home);
        bench.click(&t("s_js_profil"));
        let texts = bench.settle();
        assert!(shows(&texts, t("s_js_profil_frage").lines().next().unwrap_or_default()), "{texts:?}");
        // A name with a blank is refused at the field.
        type_into_the_field(&mut bench, "a b");
        enter(&mut bench);
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_f_name_zeichen")), "{texts:?}");
        assert!(!std::path::Path::new(&folder).exists(), "nothing written");
        // A good name: saved, and how to load it in the game stands in a window. (Enter took the focus out of the
        // field: a click gives it back, then everything typed replaces the text.)
        bench.click("a b");
        bench.frame_with(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
        type_into_the_field(&mut bench, "Mein_Profil");
        enter(&mut bench);
        let texts = bench.settle();
        assert!(shows(&texts, "pp_rebindkeys load Mein_Profil"), "{texts:?}");
        let saved = format!("{folder}/Mein_Profil.xml");
        let text = std::fs::read_to_string(&saved).expect("saved");
        assert!(text.contains("profileName=\"Mein_Profil\"") && text.contains("CustomisationUIHeader"), "{text}");
        bench.click(&t("e_ok"));
        assert!(!shows(&bench.settle(), "pp_rebindkeys"), "closed");
        // The same name again: asked first, then replaced with a copy of the old one.
        bench.click(&t("s_js_profil"));
        let texts = bench.settle();
        assert!(
            shows(&texts, &t("s_js_profil_liste")) && shows(&texts, "Mein_Profil"),
            "the profiles there: {texts:?}"
        );
        type_into_the_field(&mut bench, "Mein_Profil");
        enter(&mut bench);
        let texts = bench.settle();
        assert!(shows(&texts, &tf("s_js_profil_ersetzen", &[("0", Arg::from("Mein_Profil"))])), "{texts:?}");
        assert!(versekit_core::game_files::backups(&saved).is_empty(), "asked, nothing replaced yet");
        bench.click(&t("s_mlg_bestaetigen"));
        assert_eq!(versekit_core::game_files::backups(&saved).len(), 1, "a copy of the old profile");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_binding_is_copied_to_a_place_the_player_chose_as_a_file_and_as_a_list() {
        let _guard = setup();
        let mut bench = tools_bench("sichern");
        let (xml, csv) = (format!("{}/sicherung.xml", bench.home), format!("{}/liste.csv", bench.home));
        ANSWER.lock().expect("lock").0 = xml.clone();
        bench.click(&t("s_js_ausgeben"));
        bench.settle();
        assert_eq!(
            std::fs::read(&xml).expect("copy"),
            std::fs::read(actionmaps(&bench.home)).expect("file"),
            "the file as it is"
        );
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.contains("sicherung.xml"), "{said:?}");
        ANSWER.lock().expect("lock").0 = csv.clone();
        bench.click(&t("s_js_ausgeben_csv"));
        bench.settle();
        let raw = std::fs::read(&csv).expect("list");
        assert!(raw.starts_with(&[0xef, 0xbb, 0xbf]), "the mark of the byte order for Excel");
        let text = String::from_utf8(raw[3..].to_vec()).expect("utf8");
        assert!(text.starts_with("Geraet;Eingabe;Aktion;Bezeichnung;Gruppe;Quelle\n"), "{text}");
        assert!(text.contains("js1;button5;v_eject;Aussteigen;spaceship_general;meine"), "{text}");
        // Where it can not be written, the page says so.
        ANSWER.lock().expect("lock").0 = format!("{}/gibt-es-nicht/liste.csv", bench.home);
        bench.click(&t("s_js_ausgeben_csv"));
        bench.settle();
        assert!(bench.state.take_notice().expect("told").fault);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_binding_is_played_back_from_a_file_after_a_question_and_with_a_copy_first() {
        let _guard = setup();
        let mut bench = tools_bench("einspielen");
        let source = format!("{}/fremd.xml", bench.home);
        std::fs::write(
            &source,
            "<ActionMaps><ActionProfiles><actionmap name=\"a\"><action name=\"b\"><rebind input=\"kb1_q\"/></action></actionmap><actionmap name=\"c\"><action name=\"d\"><rebind input=\"kb1_w\"/></action></actionmap></ActionProfiles></ActionMaps>",
        )
        .expect("source");
        ANSWER.lock().expect("lock").1 = source;
        let file = actionmaps(&bench.home);
        let before = std::fs::read(&file).expect("file");
        bench.click(&t("s_js_einlesen"));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_einlesen_frage")), "{texts:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before, "asked, nothing written");
        bench.click(&t("abbrechen"));
        assert!(!shows(&bench.settle(), &t("s_js_einlesen_frage")));
        assert_eq!(std::fs::read(&file).expect("file"), before);
        bench.click(&t("s_js_einlesen"));
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("kb1_w") && !after.contains("v_eject"), "{after}");
        assert_eq!(versekit_core::game_files::backups(&file).len(), 1);
        let said = bench.state.take_notice().expect("told");
        assert!(!said.fault && said.text.starts_with("Eingespielt: 2 Gruppen"), "{said:?}");
        // A file that is no binding of the game is refused.
        ANSWER.lock().expect("lock").1 = format!("{}/leer.xml", bench.home);
        std::fs::write(format!("{}/leer.xml", bench.home), "<Other/>").expect("source");
        bench.click(&t("s_js_einlesen"));
        bench.click(&t("s_mlg_bestaetigen"));
        let said = bench.state.take_notice().expect("told");
        assert!(said.fault && said.text.contains(&t("s_js_f_fremd")), "{said:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn with_profiles_of_his_own_the_player_is_asked_where_from_and_picks_one_by_its_name() {
        let _guard = setup();
        let mut bench = tools_bench("einspielen-profil");
        let folder = mappings_folder(&bench.home);
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(
            format!("{folder}/Kampf.xml"),
            "<ActionMaps version=\"1\"><CustomisationUIHeader/><actionmap name=\"a\"><action name=\"b\"><rebind input=\"kb1_q\"/></action></actionmap></ActionMaps>",
        )
        .expect("profile");
        bench.click(&t("s_js_einlesen"));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_einlesen_woher")), "{texts:?}");
        bench.click(&t("s_js_einlesen_profil"));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_einlesen_waehlen")) && shows(&texts, "Kampf"), "{texts:?}");
        bench.click("Kampf");
        assert!(shows(&bench.settle(), &t("s_js_einlesen_frage")));
        bench.click(&t("s_mlg_bestaetigen"));
        let after = std::fs::read_to_string(actionmaps(&bench.home)).expect("file");
        assert!(
            after.contains("kb1_q") && after.contains("profileName=\"default\"") && !after.contains("Customisation"),
            "{after}"
        );
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn while_the_game_runs_the_buttons_that_write_into_the_game_are_blocked_and_the_copies_are_not() {
        let _guard = setup();
        let mut bench = tools_bench("werkzeug-laeuft");
        std::fs::write(format!("{}/StarCitizen/LIVE/Game.log", bench.home), log(&[(0, "Stick L", LEFT)])).expect("log");
        bench.later(4.0);
        let folder = mappings_folder(&bench.home);
        bench.click(&t("s_js_profil"));
        assert!(!shows(&bench.settle(), &t("s_js_profil_frage")), "blocked: no dialog");
        bench.click(&t("s_js_einlesen"));
        assert!(!shows(&bench.settle(), &t("s_js_einlesen_woher")));
        assert!(!std::path::Path::new(&folder).exists());
        // The copies for the player are no files of the game.
        let xml = format!("{}/sicherung.xml", bench.home);
        ANSWER.lock().expect("lock").0 = xml.clone();
        bench.click(&t("s_js_ausgeben"));
        bench.settle();
        assert!(std::path::Path::new(&xml).is_file());
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    // ------------------------------------------------------------------ the list of the bindings

    const GAME_PROFILE: &str = r#"<profile>
 <actionmap name="spaceship_general" UILabel="@ui_CGGeneral" UICategory="@ui_CCVehicle">
  <action name="v_eject" UILabel="@ui_CIEject" UIDescription="@ui_CIEjectDesc" keyboard="ralt+y"/>
  <action name="v_respawn" UILabel="@ui_CIRespawn" keyboard="f"/>
  <action name="v_free" UILabel="@ui_CIFree"/>
 </actionmap>
 <actionmap name="player" UILabel="@ui_CGPlayer">
  <action name="pl_wave" UILabel="@ui_CIWave"/>
 </actionmap></profile>"#;

    /// The binding of the player: the eject on a stick, and `maps` for the sticks.
    fn own_maps(extra: &str) -> String {
        format!(
            "<ActionMaps><ActionProfiles>\n <options type=\"joystick\" instance=\"1\" Product=\" Stick L  {{{LEFT}}}\"/>\n <actionmap name=\"spaceship_general\">\n  <action name=\"v_eject\"><rebind input=\"js1_button5\"/></action>\n{extra} </actionmap></ActionProfiles></ActionMaps>"
        )
    }

    fn names() -> crate::joysticks_list::ListState {
        let profile = versekit_core::controls::Profile::from_xml(GAME_PROFILE).expect("profile");
        let text = |key: &str| match key {
            "ui_CIEject" => Some("Aussteigen".to_string()),
            "ui_CIRespawn" => Some("Wiederbeleben".to_string()),
            "ui_CIWave" => Some("Winken".to_string()),
            "ui_CGGeneral" => Some("Allgemein".to_string()),
            "ui_CCVehicle" => Some("Fahrzeug".to_string()),
            "ui_CGPlayer" => Some("Spieler".to_string()),
            _ => None,
        };
        let labels = versekit_core::controls::build_labels(&profile, &text);
        let groups = versekit_core::controls::build_group_labels(&profile, &text);
        crate::joysticks_list::ListState::with_names(profile, labels, groups)
    }

    /// The name of the group of the ship in the table.
    const SHIP_GROUP: &str = "Fahrzeug · Allgemein";

    fn list_bench(name: &str, maps: &str) -> Bench {
        let mut bench = Bench::new(name, &log(&[(0, "Stick L", LEFT)]), Some(maps), vec![stick("Stick L", LEFT)]);
        bench.state.list = names();
        bench.state.list.without_sticks();
        bench
    }

    fn count(texts: &Texts, text: &str) -> usize {
        texts.iter().filter(|(s, _)| s == text).count()
    }

    /// Is there a button for a screen reader whose name begins with `head` and holds `part`?
    fn button_said(bench: &Bench, head: &str, part: &str) -> bool {
        bench
            .nodes
            .iter()
            .any(|(role, name)| *role == egui::Role::Button && name.starts_with(head) && name.contains(part))
    }

    #[test]
    fn the_table_has_one_row_per_action_with_its_bindings_in_the_columns_and_marks_the_own() {
        let _guard = setup();
        let mut bench = list_bench("tabelle", &own_maps(""));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_belegt")), "{texts:?}");
        // One row for the eject: on the stick (changed) and still on the key (the factory's).
        assert_eq!(count(&texts, "Aussteigen"), 1, "{texts:?}");
        assert!(shows(&texts, "Knopf 5") && shows(&texts, "Alt Gr + Y"), "{texts:?}");
        assert_eq!(count(&texts, "Wiederbeleben"), 1, "{texts:?}");
        // The actions nothing points to have their row, with a "–" in every column.
        assert!(shows(&texts, "Free") && shows(&texts, "Winken"), "{texts:?}");
        assert!(count(&texts, "–") >= 6, "{texts:?}");
        for head in [t("s_js_k_aktion"), t("s_js_k_tasten"), t("s_js_a_gamepad"), t("s_js_k_stick")] {
            assert!(shows(&texts, &head), "{head}: {texts:?}");
        }
        // The groups of the game with the number of their rows.
        assert!(shows(&texts, SHIP_GROUP) && shows(&texts, "Spieler"), "{texts:?}");
        assert!(shows(&texts, &tf("s_js_aktionen", &[("0", Arg::from(4))])), "{texts:?}");
        // The own binding says so, the factory's as well; the column is named.
        let stick = format!("Aussteigen, {}, Knopf 5, {}", t("s_js_k_stick"), t("s_js_eigen"));
        let key = format!("Aussteigen, {}, Alt Gr + Y, {}", t("s_js_k_tasten"), t("s_js_werk"));
        assert!(button_said(&bench, &stick, "") && button_said(&bench, &key, ""), "{:?}", bench.nodes);
        // One stick only: no name of a stick in the cells.
        assert!(!button_said(&bench, "Aussteigen", "Stick L"), "{:?}", bench.nodes);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn with_two_sticks_every_binding_of_a_stick_names_its_stick() {
        let _guard = setup();
        let maps = own_maps("  <action name=\"v_respawn\"><rebind input=\"js2_button7\"/></action>\n").replace(
            "<actionmap name=\"spaceship_general\">",
            &format!("<options type=\"joystick\" instance=\"2\" Product=\" Stick R  {{{RIGHT}}}\"/>\n <actionmap name=\"spaceship_general\">"),
        );
        let mut bench = list_bench("zwei-sticks", &maps);
        let texts = bench.settle();
        assert!(texts.iter().any(|(s, _)| s == "Stick R"), "{texts:?}");
        assert!(button_said(&bench, "Aussteigen", "Stick L, Knopf 5"), "{:?}", bench.nodes);
        assert!(button_said(&bench, "Wiederbeleben", "Stick R, Knopf 7"), "{:?}", bench.nodes);
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_switches_and_the_group_narrow_the_table() {
        let _guard = setup();
        let mut bench = list_bench("schalter", &own_maps(""));
        bench.click(&t("s_js_nur_eigene"));
        let texts = bench.settle();
        assert!(shows(&texts, "Aussteigen") && !shows(&texts, "Wiederbeleben"), "only the own: {texts:?}");
        assert!(shows(&texts, &t("s_js_aktionen_eins")), "singular: {texts:?}");
        // Unbound and own exclude each other: the one switches the other off.
        bench.click(&t("s_js_nur_frei"));
        let texts = bench.settle();
        assert!(shows(&texts, "Free") && shows(&texts, "Winken") && !shows(&texts, "Aussteigen"), "{texts:?}");
        bench.click(&t("s_js_nur_frei"));
        // One group.
        bench.click(&t("s_js_alle_gruppen"));
        bench.click_last("Spieler");
        let texts = bench.settle();
        assert!(shows(&texts, "Winken") && !shows(&texts, "Aussteigen"), "only the group: {texts:?}");
        assert!(!shows(&texts, SHIP_GROUP), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_factory_setting_and_a_saved_file_can_be_shown_and_a_saved_file_only_to_look_at() {
        let _guard = setup();
        let mut bench = list_bench("dateiwahl", &own_maps(""));
        let folder = mappings_folder(&bench.home);
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(
            format!("{folder}/layout_1_exported.xml"),
            "<ActionMaps><ActionProfiles><actionmap name=\"spaceship_general\"><action name=\"v_eject\"><rebind input=\"js1_button9\"/></action></actionmap></ActionProfiles></ActionMaps>",
        )
        .expect("saved");
        bench.later(4.0);
        bench.click(&t("s_js_q_spiel"));
        bench.click_last(&t("s_js_q_werk"));
        let texts = bench.settle();
        assert!(!shows(&texts, "Knopf 5") && shows(&texts, "Alt Gr + Y"), "only the factory setting: {texts:?}");
        assert!(!bench.nodes.iter().any(|(_, name)| name.contains(&t("s_js_eigen"))), "nothing own: {:?}", bench.nodes);
        bench.click(&t("s_js_q_werk"));
        bench.click_last("layout_1_exported.xml");
        let texts = bench.settle();
        assert!(shows(&texts, "Knopf 9") && !shows(&texts, "Knopf 5"), "the saved file: {texts:?}");
        assert!(shows(&texts, &t("s_js_nur_ansehen")), "{texts:?}");
        bench.click("Knopf 9");
        assert!(!shows(&bench.settle(), &t("s_js_b_nur_tastatur")), "only to look at: no window");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_favourite_is_kept_in_the_settings_and_the_switch_shows_only_the_favourites() {
        let _guard = setup();
        let mut bench = list_bench("favoriten", &own_maps(""));
        bench.click_node(&format!("{}: Aussteigen", t("s_js_fav")));
        let settings = std::fs::read_to_string(bench.paths.settings_file()).expect("settings");
        assert!(settings.contains("steuerung_favoriten") && settings.contains("v_eject"), "{settings}");
        // A new page reads it back.
        bench.state.list = names();
        bench.state.list.without_sticks();
        bench.click(&t("s_js_nur_fav"));
        let texts = bench.settle();
        assert!(shows(&texts, "Aussteigen") && !shows(&texts, "Wiederbeleben"), "{texts:?}");
        // Taken back: nothing is a favourite any more.
        bench.click_node(&format!("{}: Aussteigen", t("s_js_fav")));
        assert!(shows(&bench.settle(), &t("s_js_nichts_sicht")));
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn a_group_folds_up_by_its_head_and_all_of_them_by_one_button() {
        let _guard = setup();
        let mut bench = list_bench("klappen", &own_maps(""));
        bench.click(SHIP_GROUP);
        let texts = bench.settle();
        assert!(!shows(&texts, "Wiederbeleben") && shows(&texts, "Winken"), "one folded: {texts:?}");
        bench.click(&t("s_js_einklappen"));
        let texts = bench.settle();
        assert!(!shows(&texts, "Winken") && shows(&texts, SHIP_GROUP), "all folded, the heads stay: {texts:?}");
        bench.click(&t("s_js_aufklappen"));
        let texts = bench.settle();
        assert!(shows(&texts, "Wiederbeleben") && shows(&texts, "Winken"), "all open: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn an_empty_cell_is_bound_by_a_click_without_a_binding_to_remove() {
        let _guard = setup();
        let mut bench = list_bench("leere-zelle", &own_maps(""));
        let file = actionmaps(&bench.home);
        bench.click_node(&format!("Free, {}, {}", t("s_js_a_gamepad"), t("s_js_frei_zelle")));
        let texts = bench.settle();
        assert!(shows(&texts, &t("s_js_b_nur_tastatur")) && !shows(&texts, &t("s_js_b_loeschen")), "{texts:?}");
        assert!(!shows(&texts, &tf("s_js_b_bisher", &[("0", Arg::from(""))])), "{texts:?}");
        press(&mut bench, egui::Key::F7);
        bench.click(&t("s_js_b_uebernehmen"));
        let after = std::fs::read_to_string(&file).expect("file");
        assert!(after.contains("v_free") && after.contains("kb1_f7"), "{after}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn the_search_goes_over_the_plain_name_the_input_and_the_technical_name() {
        let _guard = setup();
        let mut bench = list_bench("suche", &own_maps(""));
        bench.settle();
        // The field takes the focus with the first click; after that everything typed replaces what stands there.
        bench.click(&t("s_pl_belegung"));
        let search = |bench: &mut Bench, text: &str| {
            bench.frame_with(vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: Some(egui::Key::A),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }]);
            bench.frame_with(vec![egui::Event::Text(text.to_string())]);
            bench.settle()
        };
        let texts = search(&mut bench, "wieder");
        assert!(shows(&texts, "Wiederbeleben") && !shows(&texts, "Aussteigen"), "plain name: {texts:?}");
        let texts = search(&mut bench, "V_EJECT");
        assert!(shows(&texts, "Aussteigen") && !shows(&texts, "Wiederbeleben"), "technical, any case: {texts:?}");
        let texts = search(&mut bench, "knopf 5");
        assert!(shows(&texts, "Aussteigen") && !shows(&texts, "Wiederbeleben"), "the readable input: {texts:?}");
        let texts = search(&mut bench, "spieler");
        assert!(shows(&texts, "Winken") && !shows(&texts, "Aussteigen"), "the name of the group: {texts:?}");
        let texts = search(&mut bench, "gibtesnicht");
        assert!(shows(&texts, &t("s_js_nichts")), "{texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn rows_outside_the_window_are_not_drawn_and_the_count_names_all_of_them() {
        let _guard = setup();
        let many: String = (0..260)
            .map(|n| format!("  <action name=\"x_{n:03}\"><rebind input=\"js1_button{}\"/></action>\n", n + 10))
            .collect();
        let mut bench = list_bench("viele", &own_maps(&many));
        // The rows of the unnamed actions, the four named ones.
        let all = tf("s_js_aktionen", &[("0", Arg::from(264))]);
        let row_of =
            |bench: &Bench, action: &str| bench.nodes.iter().any(|(_, name)| name.starts_with(&format!("{action}, ")));
        bench.height = 30_000.0;
        let texts = bench.settle();
        assert!(shows(&texts, &all) && shows(&texts, "x_259"), "a tall window: all rows: {texts:?}");
        assert!(row_of(&bench, "x_259"), "{:?}", bench.nodes);
        bench.height = 1_500.0;
        let texts = bench.settle();
        assert!(shows(&texts, &all), "the count names all of them: {texts:?}");
        assert!(row_of(&bench, "x_000"), "the first rows are drawn: {:?}", bench.nodes);
        assert!(!row_of(&bench, "x_259"), "the last row is not drawn at all, not even for a screen reader");
        let _ = std::fs::remove_dir_all(&bench.home);
    }

    #[test]
    fn while_the_names_are_read_the_list_says_so_and_a_game_without_names_says_that() {
        let _guard = setup();
        let mut bench =
            Bench::new("namen", &log(&[(0, "Stick L", LEFT)]), Some(&own_maps("")), vec![stick("Stick L", LEFT)]);
        // No archive of the game here: the thread finds none and the list shows the technical names.
        let mut texts = bench.settle();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !shows(&texts, &t("s_js_keine_namen")) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(15));
            bench.time += 4.0;
            texts = bench.settle();
        }
        assert!(shows(&texts, &t("s_js_keine_namen")), "{texts:?}");
        assert!(shows(&texts, "v_eject"), "the action as it is, where the game says nothing about it: {texts:?}");
        let _ = std::fs::remove_dir_all(&bench.home);
    }
}
