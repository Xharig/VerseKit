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

//! What lies on the keys: the views of the bindings and their plain names (`joysticks.py` of 4.12, the list of
//! the page "Steuerung"). Pure logic over what [`crate::joysticks`] read from the `actionmaps.xml` and what the
//! game says about itself in its `defaultProfile.xml`.
//!
//! ⭐ **The `actionmaps.xml` of the player holds only his deviations** from the factory setting. Whoever changed
//! nothing has nothing there, and a list made from it alone would be almost empty. Only both together say what
//! each key does.
//!
//! The four views:
//!
//! | View | Content |
//! |---|---|
//! | `meine` | only the own `actionmaps.xml`: what the player changed |
//! | `standard` | only the factory setting of the game |
//! | `alles` | both joined; own changes **replace** the factory setting |
//! | `frei` | actions that nothing points to |
//!
//! ⚠⚠ **When joining, the player wins, but only on the kind of device he touched.** To throw away the factory
//! setting for **all** devices as soon as one action is bound somewhere would be wrong: whoever puts "respawn"
//! on a stick would no longer see the key `F`, which still works in the game. So the pushing out goes by
//! (action, kind of device), not by action. ⚠ An own binding with an **empty** input is a deleted one: it pushes
//! the factory setting out but does not show itself, as in the game.

use std::sync::OnceLock;

use regex::Regex;
use roxmltree::Node;

use crate::joysticks::{Binding, Kind};
use crate::language::{Arg, t, tf};
use crate::ordered::OrderedMap;
use crate::text::py_strip;

/// The views, in the order of the buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    /// What the player changed.
    Mine,
    /// Both joined: the real binding.
    All,
    /// The factory setting.
    Default,
    /// Actions that nothing points to.
    Free,
}

impl View {
    pub const ALL_VIEWS: [View; 4] = [View::Mine, View::All, View::Default, View::Free];

    /// The word of 4.12 (`meine`, `alles`, `standard`, `frei`).
    pub fn word(self) -> &'static str {
        match self {
            View::Mine => "meine",
            View::All => "alles",
            View::Default => "standard",
            View::Free => "frei",
        }
    }
}

/// Where an entry of a view comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Mine,
    Default,
    Free,
}

impl Source {
    /// The word of 4.12.
    pub fn word(self) -> &'static str {
        match self {
            Source::Mine => "meine",
            Source::Default => "standard",
            Source::Free => "frei",
        }
    }
}

/// One line of a view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The input without the prefix (`button10`, `lalt+y`); empty with an action nothing points to.
    pub input: String,
    pub action: String,
    /// The group (`spaceship_movement`); empty from the factory setting.
    pub section: String,
    /// The kind of device, `None` with an action nothing points to.
    pub kind: Option<Kind>,
    pub source: Source,
}

/// The devices of a view, in the order they first appear, each with its lines.
pub type Devices = Vec<(String, Vec<Entry>)>;

/// The prefixes of the kinds, for [`kind_of`].
fn kind_of(device_id: &str) -> Option<Kind> {
    Kind::ALL.into_iter().find(|kind| device_id.starts_with(kind.prefix()))
}

/// `js1` becomes the kind of devices (the word of 4.12 is [`Kind::word`]).
pub fn device_kind(device_id: &str) -> Option<Kind> {
    kind_of(device_id)
}

/// The field of the `defaultProfile.xml` for each kind: the device it fills (the game keeps one per kind).
const DEFAULT_FIELDS: [(&str, &str); 4] =
    [("keyboard", "kb1"), ("joystick", "js1"), ("mouse", "mo1"), ("gamepad", "gp1")];

fn prefix_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(js|kb|mo|gp)([0-9]+)_").expect("pattern"))
}

// ------------------------------------------------------------------- Profile

/// What the `defaultProfile.xml` of the game says.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// Action to `(UILabel, UIDescription)`: the keys of the texts, with the `@`.
    pub labels: OrderedMap<(String, String)>,
    /// Action to its factory inputs, by field (`keyboard`, `joystick`, `mouse`, `gamepad`); only the filled ones.
    pub defaults: OrderedMap<Vec<(String, String)>>,
    /// Action to the `actionmap` it lives in.
    pub groups: OrderedMap<String>,
    /// The `actionmap`s in the order of the file, each to `(UILabel, UICategory)`: the keys of the texts, with the `@`.
    pub maps: OrderedMap<(String, String)>,
}

fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

fn is_element(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

impl Profile {
    /// Reads the `defaultProfile.xml` (as XML text, from the archive converted from CryXmlB). ⚠ It goes over the
    /// **groups**, not flat over all `action` nodes: only so it is known in which `actionmap` an action lives.
    /// Without the group a new binding lands in the wrong section and the game does not find it.
    pub fn from_xml(text: &str) -> Option<Profile> {
        let options = roxmltree::ParsingOptions { allow_dtd: true, ..roxmltree::ParsingOptions::default() };
        let document = roxmltree::Document::parse_with_options(text, options).ok()?;
        let mut profile = Profile::default();
        for group in document.descendants().filter(|n| is_element(*n, "actionmap")) {
            let section = attribute(group, "name").unwrap_or("");
            if !section.is_empty() && profile.maps.get(section).is_none() {
                profile.maps.set(
                    section,
                    (
                        attribute(group, "UILabel").unwrap_or("").to_string(),
                        attribute(group, "UICategory").unwrap_or("").to_string(),
                    ),
                );
            }
            for node in group.children().filter(|n| is_element(*n, "action")) {
                let name = attribute(node, "name").unwrap_or("");
                if !name.is_empty() && !section.is_empty() {
                    profile.groups.set(name, section.to_string());
                }
            }
        }
        for node in document.descendants().filter(|n| is_element(*n, "action")) {
            let name = attribute(node, "name").unwrap_or("");
            if name.is_empty() {
                continue;
            }
            profile.labels.set(
                name,
                (
                    attribute(node, "UILabel").unwrap_or("").to_string(),
                    attribute(node, "UIDescription").unwrap_or("").to_string(),
                ),
            );
            // ⚠ An empty field means "not bound" at the factory and is something else than a missing field.
            // Both come up.
            let preset: Vec<(String, String)> = ["keyboard", "joystick", "mouse", "gamepad"]
                .into_iter()
                .filter_map(|field| {
                    let value = py_strip(attribute(node, field).unwrap_or(""));
                    (!value.is_empty()).then(|| (field.to_string(), value.to_string()))
                })
                .collect();
            if !preset.is_empty() {
                profile.defaults.set(name, preset);
            }
        }
        Some(profile)
    }

    /// Does the profile say anything (an empty one is what a missing archive gives)?
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

// --------------------------------------------------------------------- Views

fn device_entries(own: &[(String, Vec<Binding>)]) -> Devices {
    own.iter()
        .map(|(id, entries)| {
            let entries = entries
                .iter()
                .map(|b| Entry {
                    input: b.input.clone(),
                    action: b.action.clone(),
                    section: b.section.clone(),
                    kind: Some(b.kind),
                    source: Source::Mine,
                })
                .collect();
            (id.clone(), entries)
        })
        .collect()
}

fn sort_entries(devices: &mut Devices) {
    for (_, entries) in devices.iter_mut() {
        entries.sort_by_key(|e| crate::joysticks::sort_key_of(&e.input, e.kind));
    }
}

fn push(devices: &mut Devices, id: &str, entry: Entry) {
    match devices.iter_mut().find(|(known, _)| known == id) {
        Some((_, list)) => list.push(entry),
        None => devices.push((id.to_string(), vec![entry])),
    }
}

/// The factory setting in the shape of the bindings. ⚠ The factory setting knows **one** device per kind
/// (`js1`, `kb1` …): the game does not keep its presets per connected stick. Whoever flies two sticks finds
/// all presets under `js1`.
///
/// ⚠ **Only actions the game names itself.** Without a `UILabel` an action does not show in the options of the
/// game either: they are internal and developer commands. Showing them adds about 180 lines nobody can bind.
/// Own bindings stay untouched by this: what the player entered is always shown.
pub fn default_bindings(profile: &Profile) -> Devices {
    let mut out: Devices = Vec::new();
    for (action, fields) in profile.defaults.iter() {
        if profile.labels.get(action).is_none_or(|pair| pair.0.is_empty()) {
            continue;
        }
        for (field, input) in fields {
            let Some((_, base)) = DEFAULT_FIELDS.iter().find(|(name, _)| name == field) else { continue };
            if input.is_empty() {
                continue;
            }
            // In the factory setting the input stands partly with, partly without the prefix.
            let (device_id, plain) = match prefix_pattern().captures(input) {
                Some(hit) => {
                    (format!("{}{}", &hit[1], &hit[2]), input[hit.get(0).expect("whole match").end()..].to_string())
                }
                None => ((*base).to_string(), input.clone()),
            };
            let kind = kind_of(&device_id);
            push(
                &mut out,
                &device_id,
                Entry {
                    input: plain,
                    action: action.to_string(),
                    section: String::new(),
                    kind,
                    source: Source::Default,
                },
            );
        }
    }
    sort_entries(&mut out);
    out
}

/// The bindings in one of the views. `own` is [`crate::joysticks::bindings`].
pub fn view(which: View, own: &[(String, Vec<Binding>)], profile: &Profile) -> Devices {
    match which {
        View::Free => unbound(own, profile),
        View::Mine => device_entries(own)
            .into_iter()
            .map(|(id, entries)| (id, entries.into_iter().filter(|e| !e.input.is_empty()).collect()))
            .collect(),
        View::Default => default_bindings(profile),
        View::All => {
            let own = device_entries(own);
            // First remember which action the player touched on which **kind** of device.
            let mut touched: Vec<(String, Option<Kind>)> = Vec::new();
            for (id, entries) in &own {
                let kind = kind_of(id);
                for e in entries {
                    let key = (e.action.clone(), kind);
                    if !touched.contains(&key) {
                        touched.push(key);
                    }
                }
            }
            let mut out: Devices = Vec::new();
            for (id, entries) in default_bindings(profile) {
                let kind = kind_of(&id);
                let rest: Vec<Entry> =
                    entries.into_iter().filter(|e| !touched.contains(&(e.action.clone(), kind))).collect();
                if !rest.is_empty() {
                    out.push((id, rest));
                }
            }
            for (id, entries) in own {
                for e in entries {
                    if e.input.is_empty() {
                        continue; // a deleted binding: nothing to show
                    }
                    push(&mut out, &id, e);
                }
            }
            sort_entries(&mut out);
            out
        }
    }
}

/// Actions that neither an own nor a factory binding points to.
///
/// ⭐ **Without this list one could not reach them at all.** The view shows what is bound; an action without any
/// binding does not turn up there, and the player could never click it to bind it. Measured: **411 of 646**
/// named actions are unbound at the factory (emotes, mining details, emergency commands).
pub fn unbound(own: &[(String, Vec<Binding>)], profile: &Profile) -> Devices {
    let mut bound: Vec<String> = Vec::new();
    for (_, entries) in view(View::All, own, profile) {
        for e in entries {
            if !bound.contains(&e.action) {
                bound.push(e.action);
            }
        }
    }
    let known: std::collections::HashSet<&str> = bound.iter().map(String::as_str).collect();
    let mut out: Vec<Entry> = Vec::new();
    for (action, pair) in profile.labels.iter() {
        if known.contains(action) || pair.0.is_empty() {
            continue;
        }
        out.push(Entry {
            input: String::new(),
            action: action.to_string(),
            section: profile.groups.get(action).cloned().unwrap_or_default(),
            kind: None,
            source: Source::Free,
        });
    }
    // By group, then by name: so related actions stand together (all emotes, all mining commands).
    out.sort_by(|a, b| (&a.section, &a.action).cmp(&(&b.section, &b.action)));
    if out.is_empty() { Vec::new() } else { vec![("frei".to_string(), out)] }
}

/// Who already sits on this input of this device? The actions of the view "all" on the same input, without the
/// action itself.
///
/// ⭐ **Asked BEFORE binding.** To bind a key twice is allowed in Star Citizen and sometimes wanted (different
/// kinds of vehicles), but mostly a slip, and one noticed only in battle. So it is shown and the player
/// decides, instead of the program taking something away quietly.
pub fn conflicts<'a>(all: &'a Devices, action: &str, device_id: &str, input: &str) -> Vec<&'a Entry> {
    if input.is_empty() {
        return Vec::new();
    }
    all.iter()
        .filter(|(id, _)| id == device_id)
        .flat_map(|(_, entries)| entries.iter())
        .filter(|e| e.input == input && e.action != action)
        .collect()
}

// --------------------------------------------------------------------- Table

/// The three columns of the table of the bindings: keyboard and mouse together, the game pad, the sticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Keys,
    Pad,
    Stick,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Keys, Column::Pad, Column::Stick];

    /// The column of a kind of device.
    pub fn of(kind: Kind) -> Column {
        match kind {
            Kind::Keyboard | Kind::Mouse => Column::Keys,
            Kind::Gamepad => Column::Pad,
            Kind::Joystick => Column::Stick,
        }
    }

    fn index(self) -> usize {
        match self {
            Column::Keys => 0,
            Column::Pad => 1,
            Column::Stick => 2,
        }
    }
}

/// A binding in a cell of the table: the device it is on (`js2`) and the line of the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub device: String,
    pub entry: Entry,
}

/// One action of the table with what lies on it, per column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The `actionmap` the action belongs to.
    pub group: String,
    pub action: String,
    /// The bindings in the order of [`Column::ALL`]; an empty cell is an empty list.
    pub cells: [Vec<Placed>; 3],
}

impl Row {
    pub fn cell(&self, column: Column) -> &[Placed] {
        &self.cells[column.index()]
    }

    /// Nothing lies on the action.
    pub fn is_free(&self) -> bool {
        self.cells.iter().all(Vec::is_empty)
    }

    /// At least one binding is the player's own.
    pub fn has_own(&self) -> bool {
        self.cells.iter().flatten().any(|placed| placed.entry.source == Source::Mine)
    }
}

/// The group of an action: the one the line brings, else the one the game says, else the one of an own
/// binding of the same action. ⚠ The group must go with it: Star Citizen sorts actions into groups, and a
/// binding in the wrong group is not found by the game. The factory setting does not name it.
pub fn group_of(entry: &Entry, profile: &Profile, own: &[(String, Vec<Binding>)]) -> String {
    if !entry.section.is_empty() {
        return entry.section.clone();
    }
    if let Some(group) = profile.groups.get(&entry.action) {
        return group.clone();
    }
    own.iter()
        .flat_map(|(_, list)| list.iter())
        .find(|b| b.action == entry.action && !b.section.is_empty())
        .map(|b| b.section.clone())
        .unwrap_or_default()
}

/// The table of the bindings: one row per action, with what lies on it in the three columns, the sticks in the
/// order of their numbers. Every action the game names gets a row, bound or not (so one that nothing points
/// to can be clicked and bound); an own binding of an action the game does not name gets one as well. The
/// groups stand in the order of the game file, the actions within a group as well; what the file does not
/// know follows by its name.
///
/// ⚠ One action, one row: the same binding standing twice in a cell (the game file has some actions in two
/// groups) is shown once.
pub fn table(view: &Devices, profile: &Profile, own: &[(String, Vec<Binding>)]) -> Vec<Row> {
    use std::collections::{HashMap, HashSet};
    let mut rows: Vec<Row> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut bound: HashSet<String> = HashSet::new();
    let mut devices: Vec<&(String, Vec<Entry>)> = view.iter().collect();
    devices.sort_by(|a, b| a.0.cmp(&b.0));
    for (device, entries) in devices {
        let Some(kind) = kind_of(device) else { continue };
        let column = Column::of(kind).index();
        for entry in entries.iter().filter(|e| !e.input.is_empty()) {
            let group = group_of(entry, profile, own);
            let at = *index.entry((group.clone(), entry.action.clone())).or_insert_with(|| {
                rows.push(Row { group, action: entry.action.clone(), cells: Default::default() });
                rows.len() - 1
            });
            bound.insert(entry.action.clone());
            let cell = &mut rows[at].cells[column];
            if !cell.iter().any(|p| p.device == *device && p.entry.input == entry.input) {
                cell.push(Placed { device: device.clone(), entry: entry.clone() });
            }
        }
    }
    for (action, (label, _)) in profile.labels.iter() {
        if label.is_empty() || bound.contains(action) {
            continue;
        }
        bound.insert(action.to_string());
        let group = profile.groups.get(action).cloned().unwrap_or_default();
        rows.push(Row { group, action: action.to_string(), cells: Default::default() });
    }
    let group_rank: HashMap<&str, usize> = profile.maps.iter().enumerate().map(|(i, (name, _))| (name, i)).collect();
    let action_rank: HashMap<&str, usize> = profile.labels.iter().enumerate().map(|(i, (name, _))| (name, i)).collect();
    let rank = |ranks: &HashMap<&str, usize>, name: &str| ranks.get(name).copied().unwrap_or(usize::MAX);
    rows.sort_by(|a, b| {
        (rank(&group_rank, &a.group), &a.group, rank(&action_rank, &a.action), &a.action).cmp(&(
            rank(&group_rank, &b.group),
            &b.group,
            rank(&action_rank, &b.action),
            &b.action,
        ))
    });
    rows
}

// ------------------------------------------------------------- Plain names

/// The prefixes with which the game sorts its actions: they only say in what context the action stands and
/// are in the way on display.
const ACTION_PREFIXES: [&str; 6] = ["v_", "pl_", "ui_", "mg_", "ca_", "sc_"];

/// `v_ads_stable_max_zoom_hold` becomes `Ads Stable Max Zoom Hold`: purely mechanical, no invented name.
pub fn technical_readable(action: &str) -> String {
    let mut rest = action;
    for prefix in ACTION_PREFIXES {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped;
            break;
        }
    }
    let spaced = rest.replace('_', " ");
    let words: Vec<String> = py_strip(&spaced)
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let joined = words.join(" ");
    if joined.is_empty() { action.to_string() } else { joined }
}

/// The name of an action as the player sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    pub hint: String,
    /// A name of the game (`true`) or the prepared technical one (`false`; the page shows it grey).
    pub real: bool,
}

/// Action to its readable name. `text` gives the text of a key of the game (`ui_CIEject` without the `@`) in
/// the language of the program with the English as a fallback, `None` where there is none. ⚠ **No guessing**: a
/// label without a text stays a technical name instead of making a pretty one out of the key. 314 actions have
/// no label at all and 68 point nowhere; the game has no name for them either.
pub fn build_labels(profile: &Profile, text: &dyn Fn(&str) -> Option<String>) -> OrderedMap<Label> {
    let mut out = OrderedMap::new();
    let look = |key: &str| -> String {
        let key = key.trim_start_matches('@');
        text(key).filter(|t| !t.is_empty()).unwrap_or_default()
    };
    for (action, (label, description)) in profile.labels.iter() {
        let name = look(label);
        let hint = look(description);
        if name.is_empty() {
            out.set(action, Label { name: technical_readable(action), hint, real: false });
        } else {
            out.set(action, Label { name, hint, real: true });
        }
    }
    out
}

/// Group (`actionmap`) to its readable name, as the options of the game show it: the category and the name
/// of the group ("Fahrzeug · Sitze und Bedienmodi"). `text` as with [`build_labels`]. Without a text the group
/// keeps a technical name (`seat_general` becomes "Seat General", grey on the page).
pub fn build_group_labels(profile: &Profile, text: &dyn Fn(&str) -> Option<String>) -> OrderedMap<Label> {
    let mut out = OrderedMap::new();
    let look = |key: &str| -> String {
        let key = key.trim_start_matches('@');
        if key.is_empty() { String::new() } else { text(key).filter(|t| !t.is_empty()).unwrap_or_default() }
    };
    for (group, (label, category)) in profile.maps.iter() {
        let (name, category) = (look(label), look(category));
        let label = match (category.is_empty(), name.is_empty()) {
            (false, false) => Label {
                name: tf("s_js_gruppe_name", &[("0", Arg::from(category.as_str())), ("1", Arg::from(name.as_str()))]),
                hint: String::new(),
                real: true,
            },
            (true, false) => Label { name, hint: String::new(), real: true },
            _ => Label { name: technical_readable(group), hint: String::new(), real: false },
        };
        out.set(group, label);
    }
    out
}

/// The folders of a language under `Localization` in the game folder: German is `german_(germany)`, with
/// underscores and brackets.
fn ini_folders(lang: crate::language::Lang) -> &'static [&'static str] {
    match lang {
        crate::language::Lang::De => &["german_(germany)", "german"],
        crate::language::Lang::En => &["english"],
    }
}

/// The `ui_…` lines of the `global.ini` of a language in the game folder, key to text. Only these are read: the
/// file has about 12 MB and nothing else is needed. The first folder that holds any such line wins.
pub fn ini_texts(game_folder: &str, lang: crate::language::Lang) -> std::collections::HashMap<String, String> {
    let root = std::path::Path::new(game_folder);
    let mut base = root.join("data").join("Localization");
    if !base.is_dir() {
        base = root.join("Data").join("Localization");
    }
    let mut out = std::collections::HashMap::new();
    for folder in ini_folders(lang) {
        let path = base.join(folder).join("global.ini");
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let text = crate::usercfg::normalized(&String::from_utf8_lossy(&bytes));
        for line in text.split('\n') {
            if !line.starts_with("ui_") {
                continue;
            }
            if let Some((key, value)) = line.split_once('=')
                && !value.is_empty()
            {
                out.insert(py_strip(key).to_string(), py_strip(value).to_string());
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// Which state the `global.ini` files that [`labels_from_folder`] may read are in, as one text (path, size, time
/// in nanoseconds of each): cheap, no reading of the content (the files have about 12 MB). ⚠ A heuristic, no
/// proof of the content: a backup put back with the date kept is not noticed.
pub fn ini_stamp(game_folder: &str, lang: crate::language::Lang) -> String {
    let mut folders: Vec<&str> = ini_folders(lang).to_vec();
    if lang != crate::language::Lang::En {
        folders.extend_from_slice(ini_folders(crate::language::Lang::En));
    }
    let mut parts = Vec::new();
    for sub in ["data", "Data"] {
        for folder in &folders {
            let path = std::path::Path::new(game_folder).join(sub).join("Localization").join(folder).join("global.ini");
            if let Ok(info) = std::fs::metadata(&path) {
                let nanos = info
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                parts.push(format!("{}:{}:{nanos}", path.display(), info.len()));
            }
        }
    }
    parts.join("|")
}

/// The names of the actions from the `global.ini` files of the game folder: the language asked first, the
/// English where CIG gave no text in it (better than a technical short form).
pub fn labels_from_folder(profile: &Profile, game_folder: &str, lang: crate::language::Lang) -> OrderedMap<Label> {
    names_from_folder(profile, game_folder, lang).0
}

/// [`labels_from_folder`] and the names of the groups ([`build_group_labels`]), from one reading of the files.
pub fn names_from_folder(
    profile: &Profile,
    game_folder: &str,
    lang: crate::language::Lang,
) -> (OrderedMap<Label>, OrderedMap<Label>) {
    let texts = ini_texts(game_folder, lang);
    let fallback = if lang == crate::language::Lang::En {
        Default::default()
    } else {
        ini_texts(game_folder, crate::language::Lang::En)
    };
    let text = |key: &str| texts.get(key).filter(|t| !t.is_empty()).or_else(|| fallback.get(key)).cloned();
    (build_labels(profile, &text), build_group_labels(profile, &text))
}

/// The names of keys of the keyboard and the mouse that are not clear as a short form. What is not here is
/// written in capitals (`f5` becomes `F5`).
const KEY_READABLE: [&str; 34] = [
    "lshift",
    "rshift",
    "lctrl",
    "rctrl",
    "lalt",
    "ralt",
    "space",
    "enter",
    "escape",
    "backspace",
    "tab",
    "comma",
    "period",
    "slash",
    "minus",
    "equals",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pgup",
    "pgdn",
    "insert",
    "delete",
    "pause",
    "lbracket",
    "rbracket",
    "mouse1",
    "mouse2",
    "mouse3",
    "mwheel_up",
    "mwheel_down",
];

fn button_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^button([0-9]+)$").expect("pattern"))
}

fn slider_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^slider([0-9]+)$").expect("pattern"))
}

fn hat_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^hat([0-9]+)_(\w+)$").expect("pattern"))
}

/// `x` becomes "X-Achse", `button12` "Knopf 12". ⚠⚠ A bare `x` is a letter on a keyboard; in the line of a
/// stick it would look as if the key X were meant. So the names come from the language file in both languages;
/// what is not there is passed through in capitals instead of inventing a pretty name.
pub fn input_readable(input: &str, kind: Option<Kind>) -> String {
    if input.is_empty() {
        return String::new();
    }
    // Joined inputs: `ralt+y` becomes "Alt Gr + Y".
    if input.contains('+') {
        return input
            .split('+')
            .filter(|part| !part.is_empty())
            .map(|part| input_readable(part, kind))
            .collect::<Vec<_>>()
            .join(" + ");
    }
    let known_key = KEY_READABLE.contains(&input);
    if matches!(kind, Some(Kind::Keyboard | Kind::Mouse)) || known_key {
        if known_key {
            return t(&format!("s_js_t_{input}"));
        }
        if let Some(number) = input.strip_prefix("np_") {
            return tf("s_js_t_np", &[("0", Arg::from(number.to_uppercase().as_str()))]);
        }
        return input.to_uppercase();
    }
    if matches!(input, "x" | "y" | "z") {
        return tf("s_js_e_achse", &[("0", Arg::from(input.to_uppercase().as_str()))]);
    }
    if matches!(input, "rotx" | "roty" | "rotz") {
        return tf("s_js_e_drehachse", &[("0", Arg::from(input[3..].to_uppercase().as_str()))]);
    }
    let number = |text: &str| text.parse::<i64>().map(Arg::from).unwrap_or_else(|_| Arg::from(text));
    if let Some(hit) = button_pattern().captures(input) {
        return tf("s_js_e_knopf", &[("0", number(&hit[1]))]);
    }
    if let Some(hit) = slider_pattern().captures(input) {
        return tf("s_js_e_schieber", &[("0", number(&hit[1]))]);
    }
    if let Some(hit) = hat_pattern().captures(input) {
        let arrow = match &hit[2] {
            "up" => "↑",
            "down" => "↓",
            "left" => "←",
            "right" => "→",
            other => other,
        };
        return tf("s_js_e_hut", &[("0", number(&hit[1])), ("1", Arg::from(arrow))]);
    }
    input.to_string()
}

// ------------------------------------------------------------------- Lists

/// The first line of the list as a file (⚠ semicolons: the columns).
const CSV_HEAD: &str = "Geraet;Eingabe;Aktion;Bezeichnung;Gruppe;Quelle";

/// The binding as a list for looking up and printing: device, input, action, name, group, source. ⚠ A
/// semicolon in a text would tear the columns apart (it does come up in names of the game): it becomes a
/// comma. The text of the file ends in a line end; the writing adds the mark of the byte order for Excel (else
/// it reads the umlauts as Windows-1252).
pub fn csv_text(all: &Devices, labels: &OrderedMap<Label>) -> String {
    let mut lines = vec![CSV_HEAD.to_string()];
    let mut devices: Vec<&(String, Vec<Entry>)> = all.iter().collect();
    devices.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, entries) in devices {
        for e in entries {
            let name = labels.get(&e.action).map_or("", |label| label.name.as_str());
            let fields = [id.as_str(), e.input.as_str(), e.action.as_str(), name, e.section.as_str(), e.source.word()];
            lines.push(fields.iter().map(|field| field.replace(';', ",")).collect::<Vec<_>>().join(";"));
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Writes the list to a file the player chose (nothing of the game): with the mark of the byte order, so that
/// Excel reads the umlauts right.
///
/// # Errors
///
/// The error of writing.
pub fn write_csv(path: &str, text: &str) -> std::io::Result<()> {
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(text.as_bytes());
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joysticks::bindings_from_text;
    use crate::language::{self, Choice};

    const PROFILE: &str = r#"<profile>
 <actionmap name="spaceship_general">
  <action name="v_eject" UILabel="@ui_CIEject" UIDescription="@ui_CIEjectDesc" keyboard="ralt+y" joystick="js1_button1"/>
  <action name="v_respawn" UILabel="@ui_CIRespawn" keyboard="f"/>
  <action name="v_hidden" keyboard="g"/>
  <action name="v_free" UILabel="@ui_CIFree"/>
 </actionmap>
 <actionmap name="player"><action name="pl_use" UILabel="@ui_CIUse" keyboard="  e  " mouse=""/></actionmap>
</profile>"#;

    const OWN: &str = r#"<ActionMaps><ActionProfiles>
 <actionmap name="spaceship_general">
  <action name="v_eject"><rebind input="js2_button5"/></action>
  <action name="v_respawn"><rebind input="js1_"/></action>
 </actionmap></ActionProfiles></ActionMaps>"#;

    fn german() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        language::set_language(Choice::De);
        guard
    }

    fn lines(devices: &Devices, id: &str) -> Vec<(String, String, Source)> {
        devices
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, list)| list.iter().map(|e| (e.input.clone(), e.action.clone(), e.source)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_profile_keeps_labels_presets_and_groups_and_leaves_out_empty_fields() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        assert_eq!(profile.labels.get("v_eject").map(|p| p.0.as_str()), Some("@ui_CIEject"));
        assert_eq!(profile.groups.get("pl_use").map(String::as_str), Some("player"));
        assert_eq!(
            profile.defaults.get("pl_use").cloned(),
            Some(vec![("keyboard".to_string(), "e".to_string())]),
            "stripped; the empty mouse field is no preset"
        );
        assert!(profile.defaults.get("v_free").is_none());
        assert!(Profile::from_xml("<profile").is_none());
    }

    #[test]
    fn the_player_replaces_the_factory_setting_only_on_the_kind_of_device_he_touched() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        let own = bindings_from_text(OWN);
        let all = view(View::All, &own, &profile);
        // v_eject was moved to another stick: the stick preset is gone, the key stays.
        assert_eq!(lines(&all, "kb1").iter().filter(|(_, a, _)| a == "v_eject").count(), 1, "{all:?}");
        assert!(lines(&all, "js1").iter().all(|(_, a, _)| a != "v_eject"), "{all:?}");
        assert_eq!(lines(&all, "js2"), vec![("button5".into(), "v_eject".into(), Source::Mine)]);
        // v_respawn was deleted on the stick (empty input): nothing to show there, the key stays.
        assert!(lines(&all, "js1").iter().all(|(_, a, _)| a != "v_respawn"));
        assert_eq!(lines(&all, "kb1").iter().filter(|(_, a, _)| a == "v_respawn").count(), 1);
        // An action the game does not name is not shown.
        assert!(lines(&all, "kb1").iter().all(|(_, a, _)| a != "v_hidden"));
        // Mine: only the filled ones.
        assert_eq!(lines(&view(View::Mine, &own, &profile), "js1"), vec![]);
        // Unbound: what nothing points to.
        let free = view(View::Free, &own, &profile);
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].1.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(), ["v_free"]);
    }

    #[test]
    fn whoever_already_sits_on_an_input_is_found_before_it_is_bound_again() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        let own = bindings_from_text(OWN);
        let all = view(View::All, &own, &profile);
        let on_key = conflicts(&all, "v_other", "kb1", "f");
        assert_eq!(on_key.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(), ["v_respawn"]);
        assert!(conflicts(&all, "v_respawn", "kb1", "f").is_empty(), "the action itself is no conflict");
        assert!(conflicts(&all, "v_other", "kb1", "").is_empty(), "no input, no conflict");
        assert!(conflicts(&all, "v_other", "js2", "f").is_empty(), "another device");
        assert_eq!(conflicts(&all, "v_other", "js2", "button5").len(), 1);
    }

    #[test]
    fn inputs_and_actions_get_plain_names() {
        let _guard = german();
        assert_eq!(input_readable("button12", Some(Kind::Joystick)), "Knopf 12");
        assert_eq!(input_readable("x", Some(Kind::Joystick)), "X-Achse");
        assert_eq!(input_readable("x", Some(Kind::Keyboard)), "X", "a letter on a keyboard");
        assert_eq!(input_readable("ralt+y", Some(Kind::Keyboard)), "Alt Gr + Y");
        assert_eq!(input_readable("np_5", Some(Kind::Keyboard)), "Ziffernblock 5");
        assert_eq!(input_readable("hat1_up", Some(Kind::Joystick)), "Hut 1 ↑");
        assert_eq!(input_readable("mystery", Some(Kind::Joystick)), "mystery", "passed through, not invented");
        assert_eq!(input_readable("", None), "");
        assert_eq!(technical_readable("v_ads_stable_max_zoom_hold"), "Ads Stable Max Zoom Hold");
        assert_eq!(technical_readable("v_"), "v_", "nothing left: the action itself");
    }

    fn placed(row: &Row, column: Column) -> Vec<(String, String, Source)> {
        row.cell(column).iter().map(|p| (p.device.clone(), p.entry.input.clone(), p.entry.source)).collect()
    }

    #[test]
    fn the_table_has_one_row_per_action_with_its_bindings_in_three_columns() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        let own = bindings_from_text(
            r#"<ActionMaps><ActionProfiles><actionmap name="spaceship_general">
  <action name="v_eject"><rebind input="js2_button5"/><rebind input="js1_button2"/><rebind input="mo1_mouse3"/></action>
  <action name="v_mine"><rebind input="gp1_a"/></action>
 </actionmap></ActionProfiles></ActionMaps>"#,
        );
        let rows = table(&view(View::All, &own, &profile), &profile, &own);
        let actions: Vec<(&str, &str)> = rows.iter().map(|r| (r.group.as_str(), r.action.as_str())).collect();
        // The groups and actions in the order of the game file, the action the game does not name at the end of
        // its group; the action without a label is left out, the one nothing points to has its row.
        assert_eq!(
            actions,
            [
                ("spaceship_general", "v_eject"),
                ("spaceship_general", "v_respawn"),
                ("spaceship_general", "v_free"),
                ("spaceship_general", "v_mine"),
                ("player", "pl_use"),
            ]
        );
        let eject = &rows[0];
        assert_eq!(
            placed(eject, Column::Keys),
            [("kb1".into(), "ralt+y".into(), Source::Default), ("mo1".into(), "mouse3".into(), Source::Mine)],
            "keyboard and mouse share a column"
        );
        assert_eq!(
            placed(eject, Column::Stick),
            [("js1".into(), "button2".into(), Source::Mine), ("js2".into(), "button5".into(), Source::Mine)],
            "the sticks in the order of their numbers"
        );
        assert!(eject.cell(Column::Pad).is_empty() && eject.has_own() && !eject.is_free());
        assert!(rows[2].is_free() && !rows[2].has_own(), "v_free: {:?}", rows[2]);
        assert_eq!(placed(&rows[3], Column::Pad), [("gp1".into(), "a".into(), Source::Mine)]);
        // The factory setting alone: nothing is the player's.
        let factory = table(&view(View::Default, &own, &profile), &profile, &[]);
        assert!(factory.iter().all(|r| !r.has_own()), "{factory:?}");
        assert_eq!(factory.iter().filter(|r| r.action == "v_eject").count(), 1);
    }

    #[test]
    fn a_binding_that_stands_twice_in_the_file_stands_once_in_its_cell() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        let own = bindings_from_text(
            r#"<ActionMaps><ActionProfiles><actionmap name="spaceship_general">
  <action name="v_eject"><rebind input="js1_button2"/><rebind input="js1_button2"/></action>
 </actionmap></ActionProfiles></ActionMaps>"#,
        );
        let rows = table(&view(View::All, &own, &profile), &profile, &own);
        let eject = rows.iter().find(|r| r.action == "v_eject").expect("row");
        assert_eq!(placed(eject, Column::Stick), [("js1".into(), "button2".into(), Source::Mine)]);
    }

    #[test]
    fn the_groups_get_the_names_of_the_game_with_their_category_and_a_technical_one_without() {
        let _guard = german();
        let profile = Profile::from_xml(
            r#"<profile><actionmap name="seat_general" UILabel="@ui_CGSeat" UICategory="@ui_CCVehicle"><action name="v_a"/></actionmap>
<actionmap name="mining" UILabel="@ui_CGMining"><action name="v_b"/></actionmap>
<actionmap name="debug_things"><action name="v_c"/></actionmap></profile>"#,
        )
        .expect("profile");
        assert_eq!(
            profile.maps.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["seat_general", "mining", "debug_things"]
        );
        let labels = build_group_labels(&profile, &|key| match key {
            "ui_CGSeat" => Some("Sitze und Bedienmodi".into()),
            "ui_CCVehicle" => Some("Fahrzeug".into()),
            "ui_CGMining" => Some("Bergbau".into()),
            _ => None,
        });
        let name = |group: &str| labels.get(group).map(|l| (l.name.clone(), l.real));
        assert_eq!(name("seat_general"), Some(("Fahrzeug · Sitze und Bedienmodi".into(), true)));
        assert_eq!(name("mining"), Some(("Bergbau".into(), true)));
        assert_eq!(name("debug_things"), Some(("Debug Things".into(), false)), "not invented: the technical name");
    }

    #[test]
    fn the_list_as_a_file_keeps_its_columns_and_gets_the_mark_for_excel() {
        let profile = Profile::from_xml(PROFILE).expect("profile");
        let own = bindings_from_text(OWN);
        let mut labels = OrderedMap::new();
        labels.set("v_eject", Label { name: "Aussteigen; schnell".into(), hint: String::new(), real: true });
        let text = csv_text(&view(View::All, &own, &profile), &labels);
        let mut rows = text.lines();
        assert_eq!(rows.next(), Some("Geraet;Eingabe;Aktion;Bezeichnung;Gruppe;Quelle"));
        assert!(text.contains("js2;button5;v_eject;Aussteigen, schnell;spaceship_general;meine"), "{text}");
        assert!(text.ends_with('\n'));
        let dir = std::env::temp_dir().join(format!("versekit-controls-csv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("folder");
        let file = dir.join("liste.csv").to_string_lossy().into_owned();
        write_csv(&file, &text).expect("write");
        assert!(std::fs::read(&file).expect("read").starts_with(&[0xef, 0xbb, 0xbf]));
        let _ = std::fs::remove_dir_all(dir);
    }
}
