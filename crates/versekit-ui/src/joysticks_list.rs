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

//! The table of the bindings on the page "Steuerung": one row per action, in the columns keyboard and mouse,
//! game pad and sticks, grouped as the game groups its actions (`_joysticks` of 4.12, the lower half, laid out
//! anew). A click on a binding or on an empty cell opens the window "Neu belegen".
//!
//! The plain names come from the game itself: `defaultProfile.xml` (the label of each action and group) and
//! the `global.ini` of the language of the program, so the page is bilingual without translating anything.
//! The archive is opened in a thread of its own (it takes seconds) and kept; the page draws the technical
//! names until it is there.
//!
//! What is shown: the binding the game uses (`actionmaps.xml`), the factory setting of the game, or a saved
//! file of the folder of the profiles (only to look at).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{RichText, Ui};
use versekit_core::controls::{self, Column, Label, Profile, Source, View};
use versekit_core::joysticks::{self, Assigned, Binding, Kind};
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, Lang, t, tf};
use versekit_core::ordered::OrderedMap;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::joysticks_page::{GAP, LINE_GAP, cell, text_width};
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius};

/// How often it is looked whether the sources of the names and the saved files changed, in seconds.
const KEY_SECONDS: f64 = 3.0;
/// The name of a stick is cut to this many characters in a cell.
const DEVICE_NAME_CHARS: usize = 20;
/// The height a row is taken for before it was drawn once, in points.
const ROW_GUESS: f32 = 34.0;
/// The setting that holds the favourite actions.
const FAVORITES_SETTING: &str = "steuerung_favoriten";
/// The side of the place of the star, in points.
const STAR_SIZE: f32 = 24.0;
/// The share of the width of a row the name of the action takes; the three columns share the rest.
const NAME_SHARE: f32 = 0.31;
/// The widest a drop-down above the table gets, in points.
const CHOICE_WIDTH: f32 = 360.0;
/// The space for the dot "changed by you" in front of an input, in points.
const DOT_ROOM: f32 = 10.0;

/// The bindings per device of a binding file.
type Own = [(String, Vec<Binding>)];

/// The plain names and the factory setting, for one language and one state of the sources.
pub struct Names {
    key: String,
    profile: Profile,
    labels: OrderedMap<Label>,
    groups: OrderedMap<Label>,
}

/// Which binding the table shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Shown {
    /// The `actionmaps.xml` the game works with, joined with the factory setting.
    Game,
    /// The factory setting of the game alone.
    Factory,
    /// A saved file of the folder of the profiles, by its path: only to look at.
    Saved(String),
}

/// A saved file as it was read.
struct SavedData {
    path: String,
    /// Size and time of the file when it was read.
    stamp: Option<(u64, std::time::SystemTime)>,
    /// A number nobody else has for each reading (what is made of the data compares it).
    id: u64,
    own: Vec<(String, Vec<Binding>)>,
    stored: Vec<Assigned>,
}

/// One binding in a cell as it is drawn.
struct Item {
    device: String,
    /// The short name of the stick, where several sticks are in the table; else empty.
    device_name: String,
    input: String,
    mine: bool,
}

/// One row of the table as it is drawn.
struct Line {
    group: String,
    action: String,
    name: String,
    real: bool,
    hint: String,
    /// All the player can see of the row, in small letters: what the search goes over.
    haystack: String,
    own: bool,
    free: bool,
    cells: [Vec<Item>; 3],
    /// The id of the row, from what it is (not from its place): the focus stays while the filters change.
    salt: u64,
}

/// The table made from one state of the data, the names and the language.
struct Built {
    key: (u64, String, Shown, &'static str),
    lines: Vec<Line>,
    /// The groups in the order of the table: (id, readable name, a name of the game).
    groups: Vec<(String, String, bool)>,
}

/// What the table keeps.
pub struct ListState {
    shown: Shown,
    /// The group chosen, empty for all.
    group: String,
    search: String,
    only_favorites: bool,
    only_own: bool,
    only_free: bool,
    /// The favourite actions, read from the settings at the first look.
    favorites: Option<Vec<String>>,
    /// The groups folded up.
    closed: HashSet<String>,
    /// The height of each row as drawn last, by its id: rows outside the window take this room undrawn.
    heights: HashMap<u64, f32>,
    /// The saved files there are: (file name, path).
    files: Vec<(String, String)>,
    files_at: Option<f64>,
    saved: Option<SavedData>,
    saved_at: Option<f64>,
    names: Option<Arc<Names>>,
    job: Option<Receiver<Arc<Names>>>,
    checked_at: Option<f64>,
    built: Option<Built>,
    /// The names are given and never read again (the tests).
    pinned: bool,
    /// The window to bind a line anew.
    bind: crate::joysticks_bind::BindState,
    /// The buttons above the table: profile, file, list, play back.
    tools: crate::joysticks_tools::ToolsState,
}

impl Default for ListState {
    fn default() -> Self {
        ListState {
            shown: Shown::Game,
            group: String::new(),
            search: String::new(),
            only_favorites: false,
            only_own: false,
            only_free: false,
            favorites: None,
            closed: HashSet::new(),
            heights: HashMap::new(),
            files: Vec::new(),
            files_at: None,
            saved: None,
            saved_at: None,
            names: None,
            job: None,
            checked_at: None,
            built: None,
            pinned: false,
            bind: crate::joysticks_bind::BindState::default(),
            tools: crate::joysticks_tools::ToolsState::default(),
        }
    }
}

/// The sources of the names as one text: the language, the archive, the `global.ini` files. Cheap (only looks
/// at the files), so it can be asked every few seconds.
fn names_key(paths: &Paths, lang: Lang) -> String {
    let archive = paths.archive_file().map(|a| versekit_gamedata::control_profile::mark(std::path::Path::new(&a)));
    let folder = paths.game_folder().unwrap_or_default();
    format!("{}|{}|{}", lang.code(), archive.unwrap_or_default(), controls::ini_stamp(&folder, lang))
}

/// Reads the names in the language: the profile from the archive, the texts from the `global.ini` of the game
/// folder and, where the folder holds none, from the archive.
fn load_names(paths: &Paths, lang: Lang, key: String) -> Names {
    let _topic = versekit_core::archive_lock::reading_for("controls");
    let profile = versekit_gamedata::control_profile::profile(paths);
    let folder = paths.game_folder().unwrap_or_default();
    let (labels, groups) = if controls::ini_texts(&folder, lang).is_empty() {
        let own = match lang {
            Lang::De => versekit_gamedata::Language::German,
            Lang::En => versekit_gamedata::Language::English,
        };
        let texts = paths
            .archive_file()
            .and_then(|a| versekit_gamedata::GameData::open_file(a).ok())
            .and_then(|game| game.localization(own).ok());
        let text = |k: &str| texts.as_ref().and_then(|t| t.get(k));
        (controls::build_labels(&profile, &text), controls::build_group_labels(&profile, &text))
    } else {
        controls::names_from_folder(&profile, &folder, lang)
    };
    Names { key, profile, labels, groups }
}

impl ListState {
    /// A table with the names given (the tests: no archive of the game); they are not read again.
    #[cfg(test)]
    pub(crate) fn with_names(profile: Profile, labels: OrderedMap<Label>, groups: OrderedMap<Label>) -> ListState {
        ListState {
            names: Some(Arc::new(Names { key: "fixed".into(), profile, labels, groups })),
            checked_at: Some(f64::INFINITY),
            pinned: true,
            ..ListState::default()
        }
    }

    /// These dialogs instead of the system ones for the buttons above the table (the tests).
    #[cfg(test)]
    pub(crate) fn with_dialogs(&mut self, open: crate::joysticks_tools::OpenFn, save: crate::joysticks_tools::SaveFn) {
        self.tools = crate::joysticks_tools::ToolsState::with_dialogs(open, save);
    }

    /// No stick is read by the window "Neu belegen" (the tests: no hardware).
    #[cfg(test)]
    pub(crate) fn without_sticks(&mut self) {
        self.bind.without_sticks();
    }

    /// Sticks are there for the window "Neu belegen", and waiting for them gives this (the tests).
    #[cfg(test)]
    pub(crate) fn with_waiter(&mut self, waiter: crate::joysticks_bind::Waiter) {
        self.bind.with_waiter(waiter);
    }

    /// Reads the names, the saved files and the favourites again at the next look.
    pub fn invalidate(&mut self) {
        // The names stay until the key of their sources says they are old (language, archive, ini files): the
        // binding file changing does not change them, and reading them anew takes seconds.
        self.checked_at = None;
        self.built = None;
        self.files_at = None;
        self.saved_at = None;
        self.favorites = None;
    }

    /// Takes the names when the thread has them, asks for new ones when the sources changed.
    fn poll(&mut self, ui: &Ui, paths: &Paths) {
        let now = ui.input(|i| i.time);
        if let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(names) => {
                    self.job = None;
                    self.names = Some(names);
                    self.built = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
                }
                Err(mpsc::TryRecvError::Disconnected) => self.job = None,
            }
        }
        if self.pinned || self.job.is_some() || self.checked_at.is_some_and(|at| now - at < KEY_SECONDS) {
            return;
        }
        self.checked_at = Some(now);
        let lang = language::current();
        let key = names_key(paths, lang);
        if self.names.as_ref().is_some_and(|names| names.key == key) {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let copy = paths.working_copy();
        let spawned = std::thread::Builder::new().name("joysticks-names".into()).spawn(move || {
            let _ = sender.send(Arc::new(load_names(&copy, lang, key)));
        });
        if spawned.is_ok() {
            self.job = Some(receiver);
        }
    }

    /// The saved files there are, looked at again every few seconds; a chosen file that is gone is no longer shown.
    fn poll_files(&mut self, now: f64, paths: &Paths) {
        if self.files_at.is_none_or(|at| now - at >= KEY_SECONDS) {
            self.files_at = Some(now);
            self.files = joysticks::saved_files(paths, None);
        }
        if let Shown::Saved(path) = &self.shown
            && !self.files.iter().any(|(_, known)| known == path)
        {
            self.shown = Shown::Game;
            self.saved = None;
        }
    }

    /// Reads the chosen saved file when it is new or changed.
    fn poll_saved(&mut self, now: f64, paths: &Paths) {
        let Shown::Saved(path) = &self.shown else {
            self.saved = None;
            return;
        };
        let fresh = self.saved.as_ref().is_some_and(|saved| saved.path == *path);
        if fresh && self.saved_at.is_some_and(|at| now - at < KEY_SECONDS) {
            return;
        }
        self.saved_at = Some(now);
        let stamp = std::fs::metadata(path).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
        if fresh && self.saved.as_ref().is_some_and(|saved| saved.stamp == stamp) {
            return;
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.saved = Some(SavedData {
            path: path.clone(),
            stamp,
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            own: joysticks::bindings(paths, Some(path), None),
            stored: joysticks::assignment(paths, Some(path), None),
        });
    }

    /// The favourites, read from the settings the first time.
    fn favorites(&mut self, paths: &Paths) -> &[String] {
        self.favorites.get_or_insert_with(|| match paths.settings().iter().find(|(k, _)| k == FAVORITES_SETTING) {
            Some((_, Value::Array(names))) => names.iter().filter_map(|n| n.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        })
    }

    /// Makes an action a favourite or no longer one, and keeps it in the settings.
    fn toggle_favorite(&mut self, paths: &Paths, action: &str) {
        let mut list = self.favorites(paths).to_vec();
        match list.iter().position(|known| known == action) {
            Some(at) => {
                list.remove(at);
            }
            None => list.push(action.to_string()),
        }
        paths.set_setting(FAVORITES_SETTING, Value::Array(list.iter().map(|n| Value::str(n.as_str())).collect()));
        self.favorites = Some(list);
    }
}

/// The name of a device for a person: not `js1`. ⚠ `js1` is the number under which the game keeps the device;
/// whoever sits in front of the table wants to know whether it is the left or the right stick. So the product
/// name stands there; where it is not known (the device is in no binding) the number stays.
fn device_name(id: &str, stored: &[Assigned], short: bool) -> String {
    match controls::device_kind(id) {
        Some(Kind::Keyboard) => return t("s_js_a_tastatur"),
        Some(Kind::Mouse) => return t("s_js_a_maus"),
        Some(Kind::Gamepad) => return t("s_js_a_gamepad"),
        _ => {}
    }
    let name = id
        .strip_prefix("js")
        .and_then(|n| n.parse::<i64>().ok())
        .and_then(|n| stored.iter().rev().find(|entry| entry.number == n && !entry.name.is_empty()))
        .map(|entry| entry.name.clone());
    match name {
        None => id.to_string(),
        Some(name) if !short || name.chars().count() <= DEVICE_NAME_CHARS => name,
        // Cut: two sticks of one maker share their beginning, so the number stays (`VKBsim Gladia… js1`).
        Some(name) => format!("{}… {id}", name.chars().take(DEVICE_NAME_CHARS - 5).collect::<String>()),
    }
}

/// The readable name of a group, the technical one where the game has none.
fn group_name(names: &Names, group: &str) -> (String, bool) {
    match names.groups.get(group) {
        Some(label) => (label.name.clone(), label.real),
        None if group.is_empty() => ("–".to_string(), false),
        None => (controls::technical_readable(group), false),
    }
}

/// Makes the table of what is shown: the rows with their names and readable inputs, once per state.
fn build(names: &Names, view: &controls::Devices, own: &[(String, Vec<Binding>)], stored: &[Assigned]) -> Vec<Line> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let rows = controls::table(view, &names.profile, own);
    let sticks: HashSet<&str> =
        rows.iter().flat_map(|row| row.cell(Column::Stick).iter().map(|placed| placed.device.as_str())).collect();
    let many = sticks.len() > 1;
    rows.iter()
        .map(|row| {
            let label = names.labels.get(&row.action);
            let (name, hint, real) = match label {
                Some(l) => (l.name.clone(), l.hint.clone(), l.real),
                None => (row.action.clone(), String::new(), false),
            };
            let cells = Column::ALL.map(|column| {
                row.cell(column)
                    .iter()
                    .map(|placed| Item {
                        device: placed.device.clone(),
                        device_name: if many && column == Column::Stick {
                            device_name(&placed.device, stored, true)
                        } else {
                            String::new()
                        },
                        input: controls::input_readable(&placed.entry.input, placed.entry.kind),
                        mine: placed.entry.source == Source::Mine,
                    })
                    .collect::<Vec<_>>()
            });
            let mut haystack = format!("{name} {hint} {} {}", row.action, row.group);
            haystack.push(' ');
            haystack.push_str(&group_name(names, &row.group).0);
            for placed in row.cells.iter().flatten() {
                haystack.push(' ');
                haystack.push_str(&placed.entry.input);
            }
            for item in cells.iter().flatten() {
                haystack.push(' ');
                haystack.push_str(&item.input);
            }
            let salt = {
                let mut hasher = DefaultHasher::new();
                (&row.group, &row.action).hash(&mut hasher);
                hasher.finish()
            };
            Line {
                group: row.group.clone(),
                action: row.action.clone(),
                name,
                real,
                hint,
                haystack: haystack.to_lowercase(),
                own: row.has_own(),
                free: row.is_free(),
                cells,
                salt,
            }
        })
        .collect()
}

/// A line the player clicked: what the window "Neu belegen" needs to know.
struct Request {
    action: String,
    section: String,
    name: String,
    device_id: String,
    previous: String,
}

/// Takes the keys of this frame for the window "Neu belegen", if it is open: call it **before** the page is drawn.
pub fn take_keys(state: &mut ListState, ctx: &egui::Context) {
    state.bind.take_keys(ctx);
}

/// What the table is made from: the files as they were read.
pub struct Data<'a> {
    /// A number nobody else has for each reading of the files: what is made of the data compares it.
    pub id: u64,
    /// The bindings per device in the `actionmaps.xml`.
    pub own: &'a [(String, Vec<Binding>)],
    /// The sticks in the binding.
    pub stored: &'a [Assigned],
}

/// Draws the table of the bindings and, when a binding was clicked, the window to bind it anew.
pub fn show(
    ui: &mut Ui,
    scheme: &Scheme,
    state: &mut ListState,
    paths: &Paths,
    quiet: Option<&str>,
    data: &Data<'_>,
) -> crate::joysticks_bind::Outcome {
    use crate::joysticks_bind::Outcome;
    let (own, stored) = (data.own, data.stored);
    state.poll(ui, paths);
    widgets::card_title(ui, scheme, &t("s_js_belegt"));
    ui.add_space(GAP);
    // The four buttons above the table: profile, file, list, play back.
    let names_now = state.names.clone();
    let csv = || {
        let empty = (Profile::default(), OrderedMap::new());
        let (profile, labels) = names_now.as_ref().map_or((&empty.0, &empty.1), |n| (&n.profile, &n.labels));
        controls::csv_text(&controls::view(View::All, own, profile), labels)
    };
    let tools_info = crate::joysticks_tools::ToolsInfo { paths, quiet, csv: &csv };
    let tools_changed = state.tools.show(ui, scheme, &tools_info);
    ui.add_space(GAP);
    let request = draw(ui, scheme, state, paths, data);
    if let Some(request) = request {
        state.bind.open(&request.action, &request.section, &request.name, &request.device_id, &request.previous);
    }
    let tools_said = state.tools.take_notice();
    let tools_outcome = match (tools_said, tools_changed) {
        (Some(said), _) => Outcome::Written(said),
        (None, true) => Outcome::Refresh,
        (None, false) => Outcome::Nothing,
    };
    let Some(names) = state.names.clone() else { return tools_outcome };
    if !state.bind.is_open() {
        return tools_outcome;
    }
    let all = controls::view(View::All, own, &names.profile);
    let sticks: Vec<(i64, String)> =
        stored.iter().filter(|entry| !entry.id.is_empty()).map(|entry| (entry.number, entry.id.clone())).collect();
    let label = |action: &str| names.labels.get(action).filter(|l| l.real).map(|l| l.name.clone());
    let info = crate::joysticks_bind::BindInfo { paths, quiet, sticks, all: &all, label: &label };
    match state.bind.show(ui.ctx(), scheme, &info) {
        Outcome::Nothing => tools_outcome,
        written => written,
    }
}

/// A small grey name in front of a control above the table, in a column of `width`.
fn field_name(ui: &mut Ui, scheme: &Scheme, text: &str, width: f32) {
    cell(ui, width, egui::Align::Min, |ui| {
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
    });
}

/// The controls above the table: the file, the group, the switches, the search.
fn controls_block(ui: &mut Ui, scheme: &Scheme, state: &mut ListState, groups: &[(String, String, bool)]) {
    let words = [t("s_js_datei"), t("s_js_gruppe"), t("s_js_zeigen")];
    let name_width = words
        .iter()
        .map(|w| text_width(ui, w, egui::TextStyle::Small))
        .fold(0.0_f32, f32::max)
        .min(ui.available_width() * 0.3);
    let shown_text = |shown: &Shown, files: &[(String, String)]| match shown {
        Shown::Game => t("s_js_q_spiel"),
        Shown::Factory => t("s_js_q_werk"),
        Shown::Saved(path) => {
            files.iter().find(|(_, p)| p == path).map_or_else(|| path.clone(), |(name, _)| name.clone())
        }
    };
    ui.horizontal_top(|ui| {
        field_name(ui, scheme, &words[0], name_width);
        let selected = shown_text(&state.shown, &state.files);
        let width = ui.available_width().min(CHOICE_WIDTH);
        let mut chosen = state.shown.clone();
        let combo = egui::ComboBox::from_id_salt("joysticks-file")
            .selected_text(&selected)
            .width(width)
            .truncate()
            .show_ui(ui, |list| {
                list.selectable_value(&mut chosen, Shown::Game, t("s_js_q_spiel"));
                list.selectable_value(&mut chosen, Shown::Factory, t("s_js_q_werk"));
                for (name, path) in &state.files {
                    list.selectable_value(&mut chosen, Shown::Saved(path.clone()), name);
                }
            });
        widgets::name_combo(&combo.response, &words[0], &selected);
        if chosen != state.shown {
            state.shown = chosen;
            state.saved_at = None;
            ui.ctx().request_repaint();
        }
    });
    ui.add_space(GAP / 2.0);
    ui.horizontal_top(|ui| {
        field_name(ui, scheme, &words[1], name_width);
        let all = t("s_js_alle_gruppen");
        let selected =
            groups.iter().find(|(id, _, _)| *id == state.group).map_or(all.clone(), |(_, name, _)| name.clone());
        let width = ui.available_width().min(CHOICE_WIDTH);
        let combo = egui::ComboBox::from_id_salt("joysticks-group")
            .selected_text(&selected)
            .width(width)
            .truncate()
            .show_ui(ui, |list| {
                list.selectable_value(&mut state.group, String::new(), &all);
                for (id, name, _) in groups {
                    list.selectable_value(&mut state.group, id.clone(), name);
                }
            });
        widgets::name_combo(&combo.response, &words[1], &selected);
    });
    ui.add_space(GAP / 2.0);
    ui.horizontal_top(|ui| {
        field_name(ui, scheme, &words[2], name_width);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                let (favorites, own, free) = (t("s_js_nur_fav"), t("s_js_nur_eigene"), t("s_js_nur_frei"));
                let kind = |on: bool| if on { ButtonKind::Strong } else { ButtonKind::Normal };
                if ui.add(widgets::Button::new(&favorites, kind(state.only_favorites), scheme)).clicked() {
                    state.only_favorites = !state.only_favorites;
                }
                if ui.add(widgets::Button::new(&own, kind(state.only_own), scheme)).clicked() {
                    state.only_own = !state.only_own;
                    // Own bindings and none at all exclude each other.
                    state.only_free &= !state.only_own;
                }
                if ui.add(widgets::Button::new(&free, kind(state.only_free), scheme)).clicked() {
                    state.only_free = !state.only_free;
                    state.only_own &= !state.only_free;
                }
            },
        );
    });
    ui.add_space(GAP / 2.0);
    let width = ui.available_width();
    widgets::search_field(ui, &mut state.search, &t("s_pl_belegung"), scheme, width);
}

/// Does a row pass the filters: group, favourites, own, unbound, search?
fn passes(line: &Line, state: &ListState, favorites: &[String], needle: &str) -> bool {
    (state.group.is_empty() || line.group == state.group)
        && (!state.only_favorites || favorites.contains(&line.action))
        && (!state.only_own || line.own)
        && (!state.only_free || line.free)
        && (needle.is_empty() || line.haystack.contains(needle))
}

/// The widths of the parts of a row inside its frame: star, name, each of the three columns.
fn widths(ui: &Ui, inner: f32) -> (f32, f32) {
    let gap = ui.spacing().item_spacing.x;
    let rest = (inner - STAR_SIZE - 4.0 * gap).max(0.0);
    let name = rest * NAME_SHARE;
    (name, ((rest - name) / 3.0).max(24.0))
}

/// The frame of a row and of the head of the columns, so their columns stand one over the other.
fn row_frame() -> egui::Frame {
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 8, top: 4, bottom: 4 })
}

/// The names of the columns, above the rows of an open group.
fn column_head(ui: &mut Ui, scheme: &Scheme) {
    row_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (name, column) = widths(ui, ui.available_width());
        ui.horizontal_top(|ui| {
            ui.add_space(STAR_SIZE);
            let head = |ui: &mut Ui, text: String, width: f32| {
                cell(ui, width, egui::Align::Min, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).small().color(color(scheme.sub))).wrap());
                });
            };
            head(ui, t("s_js_k_aktion"), name);
            head(ui, t("s_js_k_tasten"), column);
            head(ui, t("s_js_a_gamepad"), column);
            head(ui, t("s_js_k_stick"), column);
        });
    });
}

/// The head of a group: arrow, name, the number of its rows; a click folds it. Gives whether it was clicked.
fn group_head(ui: &mut Ui, scheme: &Scheme, name: &str, real: bool, count: usize, open: bool) -> bool {
    let count = count.to_string();
    let head = ui.horizontal(|ui| {
        let arrow = if open { "chevron-down" } else { "chevron-right" };
        if let Some(image) = crate::icons::image(arrow, scheme.sub, 14.0) {
            ui.add(image);
        }
        let hue = if real { scheme.fg } else { scheme.sub };
        let title = widgets::bold(ui, name, egui::TextStyle::Body).color(color(hue));
        ui.add(egui::Label::new(title).wrap().selectable(false));
        ui.add(egui::Label::new(RichText::new(&count).small().color(color(scheme.sub))).selectable(false));
    });
    let rect = head.response.rect;
    let response = ui.interact(rect, ui.make_persistent_id(("joysticks-group", name)), egui::Sense::click());
    let spoken = format!("{name}, {count}");
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::DisclosureTriangle, true, open, &spoken));
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    if response.hovered() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            control_radius(scheme),
            egui::Stroke::new(1.0, color(scheme.accent)),
            egui::StrokeKind::Outside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// The star of a row: a favourite or not. Gives whether it was clicked.
fn star(ui: &mut Ui, scheme: &Scheme, on: bool, name: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(STAR_SIZE), egui::Sense::click());
    let tint = if on {
        scheme.yellow
    } else if response.hovered() {
        scheme.sub
    } else {
        scheme.line
    };
    if let Some(image) = crate::icons::image("star", tint, 16.0) {
        image.paint_at(ui, egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(16.0)));
    }
    let spoken = format!("{}: {name}", t("s_js_fav"));
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, &spoken));
    widgets::focus_ring(ui, &response, scheme, control_radius(scheme));
    let hint = t(if on { "s_js_fav_aus" } else { "s_js_fav_an" });
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked()
}

/// One binding in a cell (or the "–" of an empty one): the short name of the stick above, the dot "changed by
/// you" in front. A button when it can be bound anew. Gives whether it was clicked.
fn binding_item(
    ui: &mut Ui,
    scheme: &Scheme,
    item: Option<&Item>,
    spoken: &str,
    clickable: bool,
    salt: (u64, usize, usize),
) -> bool {
    let shown = ui.vertical(|ui| {
        if let Some(item) = item.filter(|item| !item.device_name.is_empty()) {
            ui.add(egui::Label::new(RichText::new(&item.device_name).small().color(color(scheme.sub))).wrap());
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let height = ui.text_style_height(&egui::TextStyle::Body);
            let (dot, _) = ui.allocate_exact_size(egui::vec2(DOT_ROOM, height), egui::Sense::hover());
            match item {
                Some(item) => {
                    if item.mine {
                        ui.painter().circle_filled(dot.left_center() + egui::vec2(3.5, 0.0), 3.5, color(scheme.accent));
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&item.input).color(color(scheme.fg))).wrap().selectable(false),
                    );
                }
                None => {
                    ui.add(egui::Label::new(RichText::new("–").color(color(scheme.sub))).selectable(false));
                }
            }
        });
    });
    ui.add_space(2.0);
    let rect = shown.response.rect;
    let sense = if clickable { egui::Sense::click() } else { egui::Sense::hover() };
    let response = ui.interact(rect, ui.make_persistent_id(("joysticks-item", salt)), sense);
    let role = if clickable { egui::Role::Button } else { egui::Role::ListItem };
    response.widget_info(|| egui::WidgetInfo::labeled(role, clickable, spoken));
    if !clickable {
        return false;
    }
    let radius = control_radius(scheme);
    widgets::focus_ring(ui, &response, scheme, radius);
    if response.hovered() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            radius,
            egui::Stroke::new(1.0, color(scheme.accent)),
            egui::StrokeKind::Outside,
        );
    }
    let hint = match item {
        Some(item) if item.mine => t("s_js_eigen"),
        Some(_) => t("s_js_werk"),
        None => t("s_js_frei_zelle"),
    };
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked()
}

/// What was clicked in a row.
enum RowClick {
    Star,
    /// A binding (column, its place in the cell) or the empty cell (column, `None`).
    Item(usize, Option<usize>),
}

/// One row: star, name, the three columns; the bar at the left in the colour of the brand where something is
/// the player's own. Gives what was clicked and the height the row took.
fn table_row(ui: &mut Ui, scheme: &Scheme, line: &Line, favorite: bool, clickable: bool) -> (Option<RowClick>, f32) {
    let radius = control_radius(scheme);
    let columns = [t("s_js_k_tasten"), t("s_js_a_gamepad"), t("s_js_k_stick")];
    let mut clicked = None;
    let inner = row_frame()
        .fill(color(scheme.field))
        .stroke(egui::Stroke::new(1.0, color(scheme.line)))
        .corner_radius(radius)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (name_width, column_width) = widths(ui, ui.available_width());
            ui.horizontal_top(|ui| {
                if star(ui, scheme, favorite, &line.name) {
                    clicked = Some(RowClick::Star);
                }
                cell(ui, name_width, egui::Align::Min, |ui| {
                    // Grey means: not a name of the game but the prepared technical one.
                    let hue = if line.real { color(scheme.fg) } else { color(scheme.sub) };
                    let name = ui.add(egui::Label::new(RichText::new(&line.name).color(hue)).wrap());
                    let hover = if line.hint.is_empty() {
                        line.action.clone()
                    } else {
                        format!("{}\n{}", line.hint, line.action)
                    };
                    name.on_hover_text(hover);
                });
                for (index, cell_items) in line.cells.iter().enumerate() {
                    cell(ui, column_width, egui::Align::Min, |ui| {
                        if cell_items.is_empty() {
                            let spoken = format!("{}, {}, {}", line.name, columns[index], t("s_js_frei_zelle"));
                            if binding_item(ui, scheme, None, &spoken, clickable, (line.salt, index, usize::MAX)) {
                                clicked = Some(RowClick::Item(index, None));
                            }
                        }
                        for (place, shown) in cell_items.iter().enumerate() {
                            let mark = t(if shown.mine { "s_js_eigen" } else { "s_js_werk" });
                            let spoken = [line.name.as_str(), &columns[index], &shown.device_name, &shown.input, &mark]
                                .iter()
                                .filter(|part| !part.is_empty())
                                .copied()
                                .collect::<Vec<_>>()
                                .join(", ");
                            if binding_item(ui, scheme, Some(shown), &spoken, clickable, (line.salt, index, place)) {
                                clicked = Some(RowClick::Item(index, Some(place)));
                            }
                        }
                    });
                }
            });
        });
    let rect = inner.response.rect;
    let tone = if line.own { scheme.accent } else { scheme.line };
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(widgets::LIST_BAR_WIDTH, rect.height())),
        0.0,
        color(tone),
    );
    (clicked, rect.height())
}

fn draw(ui: &mut Ui, scheme: &Scheme, state: &mut ListState, paths: &Paths, data: &Data<'_>) -> Option<Request> {
    let Some(names) = state.names.clone() else {
        widgets::loading(ui, scheme, &t("s_js_namen_laedt"));
        return None;
    };
    if names.profile.is_empty() {
        ui.add(egui::Label::new(RichText::new(t("s_js_keine_namen")).small().color(color(scheme.gold))).wrap());
        ui.add_space(GAP);
    }
    let now = ui.input(|i| i.time);
    state.poll_files(now, paths);
    let groups = state.built.as_ref().map(|built| built.groups.clone()).unwrap_or_default();
    controls_block(ui, scheme, state, &groups);
    state.poll_saved(now, paths);

    // The table of what is shown, made again only when its sources changed.
    let (data_id, own, stored): (u64, &Own, &[Assigned]) = match (&state.shown, &state.saved) {
        (Shown::Game, _) => (data.id, data.own, data.stored),
        (Shown::Factory, _) => (0, &[], data.stored),
        (Shown::Saved(_), Some(saved)) => (saved.id, &saved.own, &saved.stored),
        (Shown::Saved(_), None) => (0, &[], &[]),
    };
    let key = (data_id, names.key.clone(), state.shown.clone(), language::current().code());
    if state.built.as_ref().is_none_or(|built| built.key != key) {
        let view = if state.shown == Shown::Factory {
            controls::view(View::Default, &[], &names.profile)
        } else {
            controls::view(View::All, own, &names.profile)
        };
        let lines = build(&names, &view, own, stored);
        let mut groups: Vec<(String, String, bool)> = Vec::new();
        for line in &lines {
            if !groups.iter().any(|(id, _, _)| *id == line.group) {
                let (name, real) = group_name(&names, &line.group);
                groups.push((line.group.clone(), name, real));
            }
        }
        state.built = Some(Built { key, lines, groups });
    }
    let built = state.built.take()?;
    if !state.group.is_empty() && !built.groups.iter().any(|(id, _, _)| *id == state.group) {
        state.group.clear();
    }
    let request = draw_table(ui, scheme, state, paths, &built);
    state.built = Some(built);
    request
}

/// The count, the folding of all groups and the groups with their rows.
fn draw_table(ui: &mut Ui, scheme: &Scheme, state: &mut ListState, paths: &Paths, built: &Built) -> Option<Request> {
    let needle = state.search.trim().to_lowercase();
    let favorites = state.favorites(paths).to_vec();
    let shown: Vec<&Line> = built.lines.iter().filter(|line| passes(line, state, &favorites, &needle)).collect();
    ui.add_space(GAP / 2.0);
    let count = i64::try_from(shown.len()).unwrap_or(i64::MAX);
    let key = if count == 1 { "s_js_aktionen_eins" } else { "s_js_aktionen" };
    let visible: Vec<&(String, String, bool)> =
        built.groups.iter().filter(|(id, _, _)| shown.iter().any(|line| line.group == *id)).collect();
    let all_closed = !visible.is_empty() && visible.iter().all(|(id, _, _)| state.closed.contains(id));
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::Label::new(RichText::new(tf(key, &[("0", Arg::from(count))])).small().color(color(scheme.sub)))
                .wrap(),
        );
        if !visible.is_empty() {
            let fold = t(if all_closed { "s_js_aufklappen" } else { "s_js_einklappen" });
            if ui.add(widgets::Button::new(&fold, ButtonKind::Normal, scheme)).clicked() {
                if all_closed {
                    state.closed.clear();
                } else {
                    state.closed.extend(visible.iter().map(|(id, _, _)| id.clone()));
                }
            }
        }
    });
    ui.add_space(GAP / 2.0);
    if shown.is_empty() {
        // Not the search is to blame where nothing was typed: the choice above holds nothing.
        let key = if needle.is_empty() { "s_js_nichts_sicht" } else { "s_js_nichts" };
        ui.add(egui::Label::new(RichText::new(t(key)).small().color(color(scheme.sub))).wrap());
        return None;
    }
    let clickable = !matches!(state.shown, Shown::Saved(_));
    let hint = if clickable { "s_js_b_hinweis" } else { "s_js_nur_ansehen" };
    ui.add(egui::Label::new(RichText::new(t(hint)).small().color(color(scheme.sub))).wrap());
    ui.add_space(GAP / 2.0);
    let mut request = None;
    let mut star_for = None;
    for (group, name, real) in visible {
        let rows: Vec<&&Line> = shown.iter().filter(|line| line.group == *group).collect();
        let open = !state.closed.contains(group);
        ui.add_space(GAP / 2.0);
        if group_head(ui, scheme, name, *real, rows.len(), open) {
            if open {
                state.closed.insert(group.clone());
            } else {
                state.closed.remove(group);
            }
        }
        if !open {
            continue;
        }
        column_head(ui, scheme);
        for line in rows {
            // A row outside the window takes its room without being drawn: a table of hundreds of rows stays
            // fluid. Its height is the one it had when it was drawn last.
            let height = state.heights.get(&line.salt).copied().unwrap_or(ROW_GUESS);
            let room = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), height));
            if !ui.is_rect_visible(room) {
                ui.allocate_space(egui::vec2(ui.available_width(), height));
                ui.add_space(LINE_GAP);
                continue;
            }
            let favorite = favorites.contains(&line.action);
            let (clicked, drawn) = table_row(ui, scheme, line, favorite, clickable);
            state.heights.insert(line.salt, drawn.max(1.0));
            ui.add_space(LINE_GAP);
            match clicked {
                Some(RowClick::Star) => star_for = Some(line.action.clone()),
                Some(RowClick::Item(column, place)) => {
                    let item = place.and_then(|place| line.cells[column].get(place));
                    request = Some(Request {
                        action: line.action.clone(),
                        section: line.group.clone(),
                        name: line.name.clone(),
                        // An empty cell belongs to no device yet: the input pressed decides.
                        device_id: item.map(|item| item.device.clone()).unwrap_or_default(),
                        previous: item.map(|item| item.input.clone()).unwrap_or_default(),
                    });
                }
                None => {}
            }
        }
    }
    if let Some(action) = star_for {
        state.toggle_favorite(paths, &action);
    }
    request
}
