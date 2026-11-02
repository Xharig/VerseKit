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

//! How sharply does an axis answer, and does the setting still count? (`curves.py` of 4.12.)
//!
//! ## Two levels, often mixed up
//!
//! Star Citizen sets an axis in **two places** of the same `actionmaps.xml`:
//!
//! | Level | Element | Holds for | What stands there |
//! |---|---|---|---|
//! | **physical** | `<deviceoptions name="Device {id}">` | the axis on the device (`x`, `rotz`, `slider1`) | dead zone, saturation |
//! | **logical** | `<options type="joystick" instance="N">` | the axis in the game (`flight_move_pitch`) | exponent, invert, curve |
//!
//! The dead zone holds for **everything** on that axis, the exponent only for its one flight function.
//!
//! ## ⚠⚠ The id decides, not the name
//!
//! Measured on a real setup: **three** `<deviceoptions>` blocks for **one** stick, all under one name with three
//! different ids, and only one of them belonged to the device that was plugged in. The saturation the player had set
//! hung on an id that was gone, so it did nothing, and the game shows that nowhere. An id is **alive** if the log of
//! the game names it (the device was connected lately) or the binding gives it a number.
//!
//! ## ⚠ Several entries are the normal case
//!
//! The game **appends instead of replacing**: every saturation stands twice, dead zone and saturation of one axis
//! stand in separate `<option>` elements, and a device can have two blocks of one id with different values. When
//! reading, **the last entry wins** (a reasoned assumption of 4.12, not measured at the running game).
//!
//! ## What is different from 4.12
//!
//! The file is read as XML (a comment is no block, a name with `&amp;` is the name, single quotes work), 4.12 read the
//! raw text with regular expressions. A file that does not parse gives nothing (4.12 still found blocks in half a
//! file); the page says the file cannot be read.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use roxmltree::{Document, Node};

use crate::joysticks;
use crate::paths::Paths;
use crate::pyfloat;
use crate::text::py_strip;

/// The physical axes in the order the page shows them: the two main axes, then the twist, then the sliders.
pub const AXES: [&str; 8] = ["x", "y", "z", "rotx", "roty", "rotz", "slider1", "slider2"];

/// What can be set on a physical axis. Both are shares from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    /// The dead part around the middle.
    Deadzone,
    /// The point from which the full deflection counts.
    Saturation,
}

impl Prop {
    /// Both, in the order 4.12 goes through them.
    pub const ALL: [Prop; 2] = [Prop::Deadzone, Prop::Saturation];

    /// The name in the file.
    pub fn name(self) -> &'static str {
        match self {
            Prop::Deadzone => "deadzone",
            Prop::Saturation => "saturation",
        }
    }

    /// From the name in the file.
    pub fn from_name(name: &str) -> Option<Prop> {
        Prop::ALL.into_iter().find(|p| p.name() == name)
    }

    /// What holds when nothing stands in the file, and **not 0** for both: no saturation means the whole way (1),
    /// no dead zone means none (0). A slider that starts at 0 with no saturation would write a value after which
    /// the stick hardly steers.
    pub fn default_value(self) -> f64 {
        match self {
            Prop::Deadzone => 0.0,
            Prop::Saturation => 1.0,
        }
    }
}

/// What can be set on a game axis, with the permitted range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameProp {
    /// The sensitivity of the middle of the curve. Not a share: measured were 1, 1.1, 1.5 and 3.
    Exponent,
    /// Inverted (0 or 1).
    Invert,
}

impl GameProp {
    /// The name in the file.
    pub fn name(self) -> &'static str {
        match self {
            GameProp::Exponent => "exponent",
            GameProp::Invert => "invert",
        }
    }

    /// From the name in the file.
    pub fn from_name(name: &str) -> Option<GameProp> {
        [GameProp::Exponent, GameProp::Invert].into_iter().find(|p| p.name() == name)
    }

    /// The permitted range, both ends included.
    pub fn range(self) -> (f64, f64) {
        match self {
            GameProp::Exponent => (0.1, 10.0),
            GameProp::Invert => (0.0, 1.0),
        }
    }
}

/// The exponent that holds when the file says nothing.
pub const DEFAULT_EXPONENT: f64 = 1.0;

/// A ready choice for an axis: dead zone, saturation and sensitivity (the exponent) in one go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    /// The end of the language keys: `s_ac_vor_<key>` is the name, `s_ac_vor_<key>_t` tells what it is for.
    pub key: &'static str,
    pub deadzone: f64,
    pub saturation: f64,
    pub exponent: f64,
}

/// The presets the page offers, in the order of the buttons. The one place to change a value. Every value must lie
/// on the grid of the sliders (thousandths for the two shares, twentieths for the exponent), else the slider would
/// show another number than the one chosen.
pub const PRESETS: [Preset; 5] = [
    Preset { key: "standard", deadzone: 0.0, saturation: 1.0, exponent: 1.0 },
    Preset { key: "balanced", deadzone: 0.05, saturation: 1.0, exponent: 1.5 },
    Preset { key: "fine", deadzone: 0.05, saturation: 1.0, exponent: 2.0 },
    Preset { key: "direct", deadzone: 0.03, saturation: 1.0, exponent: 1.0 },
    Preset { key: "pedals", deadzone: 0.08, saturation: 1.0, exponent: 1.5 },
];

/// The two values of one physical axis; `None` is "not set".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values {
    pub deadzone: Option<f64>,
    pub saturation: Option<f64>,
}

impl Values {
    /// One of the two.
    pub fn get(&self, prop: Prop) -> Option<f64> {
        match prop {
            Prop::Deadzone => self.deadzone,
            Prop::Saturation => self.saturation,
        }
    }

    fn set(&mut self, prop: Prop, value: Option<f64>) {
        match prop {
            Prop::Deadzone => self.deadzone = value,
            Prop::Saturation => self.saturation = value,
        }
    }
}

/// A `<deviceoptions>` block.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceBlock {
    /// The device name without the id.
    pub name: String,
    /// The id in capital letters; empty for mouse and keyboard.
    pub ident: String,
    /// Does the block still count? Its id is connected or bound; a block with no id (mouse, keyboard) counts.
    /// ⚠ `false` means: the values here do nothing. They are in the file, they look real, the game ignores them.
    pub active: bool,
    /// The axes in the order of the first find, with their values.
    pub axes: Vec<(String, Values)>,
    /// The axes with entries that contradict each other (sorted).
    pub multiple: Vec<String>,
    /// Dead, but a block with the same name is alive: the device has a new id, the setting can be carried over.
    pub overtaken: bool,
    /// Dead, and the device is gone altogether.
    pub orphaned: bool,
    /// Does the id of the block belong to a device that is connected or bound? (`active` can say no for a block
    /// whose id is alive: the older name of a device the game now calls otherwise.) False without an id.
    pub ident_alive: bool,
    /// Where the element stands in the text it was read from.
    pub range: std::ops::Range<usize>,
}

impl DeviceBlock {
    /// The values of an axis.
    pub fn axis(&self, name: &str) -> Option<&Values> {
        self.axes.iter().find(|(known, _)| known == name).map(|(_, v)| v)
    }

    fn has_values(&self) -> bool {
        self.axes.iter().any(|(_, v)| v.deadzone.is_some() || v.saturation.is_some())
    }
}

/// What a game axis holds.
#[derive(Clone, Debug, PartialEq)]
pub struct GameAxis {
    pub exponent: Option<f64>,
    /// `None` where the attribute is not there.
    pub invert: Option<f64>,
    /// The points of `<nonlinearity_curve>` as `(in, out)`. ⚠ In every measured file the block was empty; the page
    /// does not show it.
    pub curve: Vec<(Option<f64>, Option<f64>)>,
    pub has_curve_block: bool,
}

/// An `<options>` block of a device in the game.
#[derive(Clone, Debug, PartialEq)]
pub struct GameBlock {
    /// `joystick`, `keyboard`, `gamepad` (small letters).
    pub kind: String,
    /// The `instance`, the `n` in `js<n>_`.
    pub number: i64,
    pub name: String,
    pub ident: String,
    pub active: bool,
    /// The game axes (`flight_move_pitch` …) in file order.
    pub axes: Vec<(String, GameAxis)>,
}

/// What lies on a physical axis: a game axis and the action that puts it there.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The name in `<options>` (`flight_move_pitch`).
    pub axis: String,
    /// The name in the binding (`v_pitch`).
    pub action: String,
    pub exponent: Option<f64>,
    pub invert: Option<f64>,
}

/// An overview for the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// All physical blocks, the live ones first, then by name.
    pub blocks: Vec<DeviceBlock>,
    /// Dead blocks that still carry a value (overtaken first).
    pub orphans: Vec<DeviceBlock>,
    /// A device is there but the setting hangs on the old id.
    pub adoptable: Vec<Adoptable>,
    /// The game axes.
    pub game: Vec<GameBlock>,
    /// `(device, axis)` where entries contradict each other.
    pub conflicts: Vec<(String, String)>,
}

/// What could be carried over from an overtaken block into the live one.
#[derive(Clone, Debug, PartialEq)]
pub struct Adoptable {
    pub name: String,
    pub old: DeviceBlock,
    pub new: DeviceBlock,
    /// `(axis, property, old value, value now)`; only what differs.
    pub values: Vec<(String, Prop, f64, Option<f64>)>,
    /// Several live devices (other ids) carry the name: which one the old values belong to can not be told by the
    /// name, so nothing is offered to be written (4.12 took the last one).
    pub ambiguous: bool,
}

fn ident_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{([0-9A-Fa-f-]{8,})\}").expect("pattern"))
}

/// The bare id from a name with the braces, in capital letters; empty where there is none.
pub fn ident_from(text: &str) -> String {
    // The id of a game controller: a device code, private for the streamer mode.
    let ident = ident_pattern().captures(text).map(|hit| hit[1].to_uppercase()).unwrap_or_default();
    crate::streamer::learn_device_id(&ident);
    ident
}

/// The device name without the id in braces, trimmed.
pub fn name_without_ident(text: &str) -> String {
    py_strip(&ident_pattern().replace_all(text, "")).to_string()
}

fn parse(text: &str) -> Option<Document<'_>> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..roxmltree::ParsingOptions::default() };
    Document::parse_with_options(text, options).ok()
}

fn is_element(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

/// Which ids count as alive: those of the newest log of the game and those the binding gives a number.
pub fn valid_idents(paths: &Paths, folder: Option<&str>, file: Option<&str>) -> HashSet<String> {
    let mut alive: HashSet<String> =
        joysticks::devices(paths, folder).into_iter().map(|d| d.id.to_uppercase()).collect();
    alive.extend(joysticks::assignment(paths, file, folder).into_iter().filter(|a| !a.id.is_empty()).map(|a| a.id));
    alive.remove("");
    alive
}

/// The names the game gives the devices **now**: `{id: name}` from the heads of `<options type="joystick">`.
fn managed_names(document: &Document) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "options")) {
        if node.attribute("type") != Some("joystick") {
            continue;
        }
        let Some(product) = node.attribute("Product") else { continue };
        let ident = ident_from(product);
        if ident.is_empty() {
            continue;
        }
        let name = name_without_ident(product);
        match found.iter_mut().find(|(known, _)| *known == ident) {
            Some(entry) => entry.1 = name,
            None => found.push((ident, name)),
        }
    }
    found
}

/// The physical blocks of the text, in file order. `alive` is the set of ids that count.
pub fn device_axes_from_text(text: &str, alive: &HashSet<String>) -> Vec<DeviceBlock> {
    let Some(document) = parse(text) else { return Vec::new() };
    let mut out: Vec<DeviceBlock> = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "deviceoptions")) {
        let name_raw = node.attribute("name").unwrap_or("");
        let ident = ident_from(name_raw);
        let mut axes: Vec<(String, Values)> = Vec::new();
        let mut duplicates: Vec<String> = Vec::new();
        for entry in node.descendants().filter(|n| is_element(*n, "option")) {
            let axis = entry.attribute("input").unwrap_or("");
            if axis.is_empty() {
                continue;
            }
            let at = match axes.iter().position(|(known, _)| known == axis) {
                Some(at) => at,
                None => {
                    axes.push((axis.to_string(), Values::default()));
                    axes.len() - 1
                }
            };
            for prop in Prop::ALL {
                let Some(value) = entry.attribute(prop.name()) else { continue };
                let fresh = pyfloat::parse(value);
                let previous = axes[at].1.get(prop);
                // ⚠ The LAST wins. A contradiction is remembered so the page can show it; the same value twice is
                // the normal case and no hint.
                if let (Some(old), Some(new)) = (previous, fresh)
                    && old != new
                    && !duplicates.iter().any(|d| d == axis)
                {
                    duplicates.push(axis.to_string());
                }
                axes[at].1.set(prop, fresh);
            }
        }
        duplicates.sort();
        out.push(DeviceBlock {
            name: name_without_ident(name_raw),
            // ⚠ Without an id nothing can be judged. Mouse and keyboard stand without braces: calling them dead
            // would be wrong.
            active: ident.is_empty() || alive.contains(&ident),
            ident_alive: alive.contains(&ident),
            ident,
            axes,
            multiple: duplicates,
            overtaken: false,
            orphaned: false,
            range: node.range(),
        });
    }
    keep_only_managed(&mut out, &managed_names(&document));
    conflicts_across_blocks(&mut out);
    sort_orphans(&mut out);
    out
}

/// With several blocks of one id only **one** is alive: the one under the name the game gives the device now.
/// Otherwise one stick stands twice in the bar (an old and a new name, one id). If no name fits, nothing is done:
/// one tab too many is better than the right one gone.
fn keep_only_managed(blocks: &mut [DeviceBlock], managed: &[(String, String)]) {
    if managed.is_empty() {
        return;
    }
    let mut seen: Vec<String> = Vec::new();
    let idents: Vec<String> = blocks.iter().map(|b| b.ident.clone()).filter(|i| !i.is_empty()).collect();
    for ident in idents {
        if seen.contains(&ident) {
            continue;
        }
        seen.push(ident.clone());
        let group: Vec<usize> = (0..blocks.len()).filter(|at| blocks[*at].ident == ident).collect();
        if group.len() < 2 {
            continue;
        }
        let Some((_, name)) = managed.iter().find(|(known, _)| *known == ident) else { continue };
        if name.is_empty() {
            continue;
        }
        let Some(first) = group.iter().copied().find(|at| blocks[*at].name == *name) else { continue };
        for at in group {
            blocks[at].active = at == first;
        }
    }
}

/// Contradictions that go over two blocks of one id, compared **only within the same state** (live against live,
/// dead against dead): a dead block does not act, so it cannot contradict what acts.
fn conflicts_across_blocks(blocks: &mut [DeviceBlock]) {
    let mut groups: Vec<((String, bool), Vec<usize>)> = Vec::new();
    for (at, block) in blocks.iter().enumerate() {
        if block.ident.is_empty() {
            continue;
        }
        let key = (block.ident.clone(), block.active);
        match groups.iter_mut().find(|(known, _)| *known == key) {
            Some((_, members)) => members.push(at),
            None => groups.push((key, vec![at])),
        }
    }
    for (_, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let mut seen: Vec<((String, Prop), f64)> = Vec::new();
        for &at in &members {
            let axes = blocks[at].axes.clone();
            for (axis, values) in &axes {
                for prop in Prop::ALL {
                    let Some(value) = values.get(prop) else { continue };
                    let key = (axis.clone(), prop);
                    match seen.iter_mut().find(|(known, _)| *known == key) {
                        Some((_, kept)) => {
                            if *kept != value {
                                for &part in &members {
                                    if blocks[part].axis(axis).is_some() && !blocks[part].multiple.contains(axis) {
                                        blocks[part].multiple.push(axis.clone());
                                        blocks[part].multiple.sort();
                                    }
                                }
                            }
                            *kept = value;
                        }
                        None => seen.push((key, value)),
                    }
                }
            }
        }
    }
}

/// A dead block is **overtaken** if its name stands in a live block (the device has a new id), else **orphaned**.
/// ⚠ The name is otherwise taboo; here it only files a hint, nothing is written because of it.
fn sort_orphans(blocks: &mut [DeviceBlock]) {
    let active_names: Vec<String> =
        blocks.iter().filter(|b| b.active && !b.name.is_empty()).map(|b| b.name.clone()).collect();
    for block in blocks.iter_mut() {
        let dead = !block.active;
        block.overtaken = dead && active_names.contains(&block.name);
        block.orphaned = dead && !block.overtaken;
    }
}

/// [`device_axes_from_text`] for the file; empty where there is none or it cannot be read.
pub fn device_axes(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Vec<DeviceBlock> {
    let Some(text) = read_text(paths, file, folder) else { return Vec::new() };
    device_axes_from_text(&text, &valid_idents(paths, folder, file))
}

/// The text of the binding file in use (the newest of the files that lie there), without the mark of the byte
/// order; `None` where there is none or it can not be read.
pub fn file_text(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Option<String> {
    read_text(paths, file, folder)
}

fn read_text(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Option<String> {
    let path = file.map(str::to_string).or_else(|| joysticks::actionmaps_path(paths, folder))?;
    joysticks::read_xml(&path)
}

/// The game blocks of the text (`<options type=…>`), empty places (no `Product`) left out.
pub fn game_axes_from_text(text: &str, alive: &HashSet<String>) -> Vec<GameBlock> {
    let Some(document) = parse(text) else { return Vec::new() };
    let mut out = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "options")) {
        let product = node.attribute("Product").unwrap_or("");
        if py_strip(product).is_empty() {
            // The game makes eight of these empty places.
            continue;
        }
        let kind = node.attribute("type").unwrap_or("").to_lowercase();
        // Digits only, like `instance="(\d+)"` of 4.12: a sign or blanks make it 0.
        let number = node
            .attribute("instance")
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let ident = ident_from(product);
        let mut axes: Vec<(String, GameAxis)> = Vec::new();
        for child in node.children().filter(Node::is_element) {
            let name = child.tag_name().name();
            if name == "nonlinearity_curve" || name == "point" {
                continue;
            }
            let curve: Vec<(Option<f64>, Option<f64>)> = child
                .descendants()
                .filter(|n| is_element(*n, "point"))
                .filter_map(|p| Some((pyfloat::parse(p.attribute("in")?), pyfloat::parse(p.attribute("out")?))))
                .collect();
            let axis = GameAxis {
                exponent: child.attribute("exponent").and_then(pyfloat::parse),
                invert: child.attribute("invert").and_then(pyfloat::parse),
                curve,
                has_curve_block: child.descendants().any(|n| is_element(n, "nonlinearity_curve")),
            };
            match axes.iter_mut().find(|(known, _)| known == name) {
                Some(entry) => entry.1 = axis,
                None => axes.push((name.to_string(), axis)),
            }
        }
        out.push(GameBlock {
            kind,
            number,
            name: name_without_ident(product),
            active: ident.is_empty() || alive.contains(&ident),
            ident,
            axes,
        });
    }
    out
}

/// [`game_axes_from_text`] for the file.
pub fn game_axes(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Vec<GameBlock> {
    let Some(text) = read_text(paths, file, folder) else { return Vec::new() };
    game_axes_from_text(&text, &valid_idents(paths, folder, file))
}

/// The name of the game axis in `<options>` for an action of the binding. The order of the tests counts:
/// `v_view_pitch` before `v_pitch`, or it would be filed as `flight_move_view_pitch`.
pub fn game_axis_name(action: &str) -> String {
    for (front, replacement) in [("v_view_", "flight_view_"), ("v_mining_", "mining_"), ("v_", "flight_move_")] {
        if let Some(rest) = action.strip_prefix(front) {
            return format!("{replacement}{rest}");
        }
    }
    action.to_string()
}

/// Which game axes lie on this physical axis of the device with this number? Often **several** (measured: on
/// `js2_y` lay `v_pitch` and `v_strafe_vertical`, each with its own sensitivity).
pub fn functions_on_axis_from_text(text: &str, number: i64, axis: &str, game: &[GameBlock]) -> Vec<Function> {
    let Some(document) = parse(text) else { return Vec::new() };
    let wanted = format!("js{number}_{axis}");
    let mut actions: Vec<String> = Vec::new();
    for action in document.descendants().filter(|n| is_element(*n, "action")) {
        let Some(name) = action.attribute("name") else { continue };
        let hit = action
            .children()
            .filter(|n| is_element(*n, "rebind"))
            .any(|r| r.attribute("input").is_some_and(|i| py_strip(i) == wanted));
        if hit {
            actions.push(name.to_string());
        }
    }
    let values = game.iter().find(|b| b.kind == "joystick" && b.number == number).map(|b| &b.axes[..]).unwrap_or(&[]);
    let mut out: Vec<Function> = Vec::new();
    for action in actions {
        let name = game_axis_name(&action);
        if out.iter().any(|f| f.axis == name) {
            continue;
        }
        let props = values.iter().find(|(known, _)| *known == name).map(|(_, v)| v);
        out.push(Function {
            axis: name,
            action,
            exponent: props.and_then(|p| p.exponent),
            invert: props.and_then(|p| p.invert),
        });
    }
    out
}

/// The block the page shows for an id and the writing changes: of the live blocks with this id the first by name
/// (the order of [`Summary::blocks`]). ⚠ 4.12 showed this one and wrote into the **last** block of the id, often
/// the dead one with the old name: the player saw no change afterwards.
pub fn shown_block<'a>(blocks: &'a [DeviceBlock], ident: &str) -> Option<&'a DeviceBlock> {
    blocks.iter().filter(|b| b.active && b.ident == ident).min_by(|a, b| a.name.cmp(&b.name))
}

/// Dead blocks that carry a value at all: an empty dead block is old paper, not a problem. Overtaken first.
pub fn orphans(blocks: &[DeviceBlock]) -> Vec<DeviceBlock> {
    let mut out: Vec<DeviceBlock> = blocks.iter().filter(|b| !b.active && b.has_values()).cloned().collect();
    out.sort_by(|a, b| (!a.overtaken, &a.name).cmp(&(!b.overtaken, &b.name)));
    out
}

/// What could be carried over from an overtaken block into the live block of the same name. A value is the same if
/// it differs by less than a thousandth (the game writes `0.098999992`, a tool `0.099`; nobody feels a thousandth).
pub fn adoptable(blocks: &[DeviceBlock]) -> Vec<Adoptable> {
    let mut out = Vec::new();
    for block in blocks {
        if !block.overtaken {
            continue;
        }
        // Several live blocks of one name: the last wins.
        let Some(target) = blocks.iter().rev().find(|b| b.active && !b.name.is_empty() && b.name == block.name) else {
            continue;
        };
        let mut values: Vec<(String, Prop, f64, Option<f64>)> = Vec::new();
        for (axis, props) in &block.axes {
            for prop in Prop::ALL {
                let Some(old) = props.get(prop) else { continue };
                let now = target.axis(axis).and_then(|v| v.get(prop));
                if now.is_some_and(|now| (now - old).abs() < 1e-3) {
                    continue;
                }
                if now != Some(old) {
                    values.push((axis.clone(), prop, old, now));
                }
            }
        }
        if values.is_empty() {
            continue;
        }
        values.sort_by(|a, b| {
            let rank = |axis: &str| AXES.iter().position(|known| *known == axis).unwrap_or(99);
            (rank(&a.0), a.1.name()).cmp(&(rank(&b.0), b.1.name()))
        });
        let ambiguous = blocks
            .iter()
            .filter(|b| b.active && !b.name.is_empty() && b.name == block.name && b.ident != target.ident)
            .count()
            > 0;
        out.push(Adoptable { name: block.name.clone(), old: block.clone(), new: target.clone(), values, ambiguous });
    }
    out
}

/// An overview from the text: read once, passed on.
pub fn summary_from_text(text: &str, alive: &HashSet<String>) -> Summary {
    let blocks = device_axes_from_text(text, alive);
    let mut conflicts = Vec::new();
    for block in &blocks {
        for axis in &block.multiple {
            conflicts.push((block.name.clone(), axis.clone()));
        }
    }
    let mut sorted = blocks.clone();
    sorted.sort_by(|a, b| (!a.active, &a.name).cmp(&(!b.active, &b.name)));
    Summary {
        orphans: orphans(&blocks),
        adoptable: adoptable(&blocks),
        game: game_axes_from_text(text, alive),
        blocks: sorted,
        conflicts,
    }
}

/// [`summary_from_text`] for the file.
pub fn summary(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Summary {
    let Some(text) = read_text(paths, file, folder) else { return summary_from_text("", &HashSet::new()) };
    summary_from_text(&text, &valid_idents(paths, folder, file))
}

// ------------------------------------------------------------------------------------------------ the answer

/// Clamp to 0..1 the way Python's `max(0.0, min(1.0, v))` does, NaN included.
fn clamp_share(value: f64) -> f64 {
    let low = if value < 1.0 { value } else { 1.0 };
    if low > 0.0 { low } else { 0.0 }
}

/// What comes out at the back if the stick is deflected by `input` (−1 to 1)? The calculation behind the curve the
/// game draws: cut the dead zone, stretch the rest to the saturation, apply the exponent (above 1 the middle gets
/// finer, below 1 coarser); points of a curve, if any, win over the exponent. The sign stays, the calculation is on
/// the amount: the curve is point-symmetric.
///
/// ⚠ **Rebuilt, not taken from the game.** It gives the usual behaviour and is right at the corner points (dead
/// zone 0, saturation 1, exponent 1 gives the straight line). Whether CIG calculates the same in detail is not
/// said: the display is a good preview, not a proof.
pub fn answer(
    input: f64,
    deadzone: Option<f64>,
    saturation: Option<f64>,
    exponent: Option<f64>,
    curve: &[(Option<f64>, Option<f64>)],
) -> f64 {
    let sign = if input < 0.0 { -1.0 } else { 1.0 };
    let mut amount = input.abs();
    if amount > 1.0 {
        amount = 1.0;
    }
    let deadzone = deadzone.map_or(0.0, clamp_share);
    let saturation = saturation.map_or(1.0, clamp_share);
    let exponent = exponent.unwrap_or(1.0);
    if amount <= deadzone {
        return 0.0;
    }
    // ⚠ A saturation below the dead zone is a contradiction: no way is left on which anything can change. Instead
    // of dividing by zero everything beyond the dead zone counts as the full deflection.
    let span = saturation - deadzone;
    if span <= 0.0 {
        return sign;
    }
    let mut share = (amount - deadzone) / span;
    if share > 1.0 {
        share = 1.0;
    }
    if !curve.is_empty() {
        return sign * from_curve(share, curve);
    }
    if exponent > 0.0 && exponent != 1.0 {
        share = share.powf(exponent);
    }
    sign * share
}

/// Read between the set points in straight lines.
fn from_curve(share: f64, curve: &[(Option<f64>, Option<f64>)]) -> f64 {
    let mut points: Vec<(f64, f64)> =
        curve.iter().filter_map(|(a, b)| Some(((*a)?, (*b)?))).filter(|(a, b)| !a.is_nan() && !b.is_nan()).collect();
    if points.is_empty() {
        return share;
    }
    // No NaN is left, so the order is a total one (a sort over a partial order may panic).
    points.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Nail down the ends so that nothing is read into the void.
    if points[0].0 > 0.0 {
        points.insert(0, (0.0, 0.0));
    }
    if points[points.len() - 1].0 < 1.0 {
        points.push((1.0, 1.0));
    }
    for pair in points.windows(2) {
        let ((left_x, left_y), (right_x, right_y)) = (pair[0], pair[1]);
        if left_x <= share && share <= right_x {
            let width = right_x - left_x;
            if width <= 0.0 {
                return right_y;
            }
            return left_y + (share - left_x) / width * (right_y - left_y);
        }
    }
    points[points.len() - 1].1
}

/// The curve as `(in, out)` pairs ready to draw: the quadrant (0 to 1) or the whole (−1 to 1). `steps` only sets
/// how fine the drawing is.
pub fn progression(
    deadzone: Option<f64>,
    saturation: Option<f64>,
    exponent: Option<f64>,
    curve: &[(Option<f64>, Option<f64>)],
    steps: usize,
    whole: bool,
) -> Vec<(f64, f64)> {
    let steps = steps.max(2);
    let start = if whole { -1.0 } else { 0.0 };
    let extent = 1.0 - start;
    (0..=steps)
        .map(|nr| {
            let on = start + extent * nr as f64 / steps as f64;
            (on, answer(on, deadzone, saturation, exponent, curve))
        })
        .collect()
}

/// How the page shows a value: two places, no trailing zeros, a dash where nothing is set.
pub fn two_decimals(value: Option<f64>) -> String {
    let Some(value) = value else { return "—".to_string() };
    let shown = pyfloat::fixed(value, 2);
    let trimmed = shown.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: &str = "03F33344-0000-0000-0000-504944564944";
    const RIGHT: &str = "43F43344-0000-0000-0000-504944564944";
    const OLD: &str = "83F33344-0000-0000-0000-504944564944";

    fn alive(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|i| i.to_string()).collect()
    }

    /// A file like the one of the game (measured: the blocks stand directly under `ActionProfiles`).
    fn file() -> String {
        format!(
            r#"<ActionMaps>
 <ActionProfiles version="1" optionsVersion="2" rebindVersion="2" profileName="default">
  <deviceoptions name="LEFT VPC Stick WarBRD-D  {{{LEFT}}}">
   <option input="x" deadzone="0.152"/>
   <option input="y" deadzone="0.15000001"/>
   <option input="x" saturation="0.84799999"/>
   <option input="x" saturation="0.84799999"/>
  </deviceoptions>
  <deviceoptions name="Mouse">
   <option input="@pause_OptionsMouseSmoothing" saturation="0"/>
  </deviceoptions>
  <deviceoptions name="LEFT VPC Stick WarBRD-D  {{{OLD}}}">
   <option input="x" deadzone="0.099"/>
   <option input="rotx" saturation="0.7"/>
  </deviceoptions>
  <deviceoptions name="RIGHT VPC Stick  {{{RIGHT}}}">
   <option input="x" deadzone="0.1"/>
   <option input="x" deadzone="0.3"/>
  </deviceoptions>
  <options type="joystick" instance="1" Product="LEFT VPC Stick WarBRD-D  {{{LEFT}}}">
   <flight_move_pitch exponent="1.5" invert="1"/>
   <flight_move_yaw invert="0"/>
  </options>
  <options type="joystick" instance="2" Product="RIGHT VPC Stick  {{{RIGHT}}}">
   <flight_move_roll exponent="0.5"><nonlinearity_curve><point in="0.5" out="0.1"/></nonlinearity_curve></flight_move_roll>
  </options>
  <options type="joystick" instance="3"/>
  <actionmap name="spaceship_movement">
   <action name="v_pitch"><rebind input="js1_y"/></action>
   <action name="v_strafe_vertical"><rebind input=" js1_y "/></action>
   <action name="v_view_pitch"><rebind input="js1_y"/></action>
   <action name="v_roll"><rebind input="js1_x"/><rebind input="js2_x"/></action>
  </actionmap>
 </ActionProfiles>
</ActionMaps>
"#
        )
    }

    #[test]
    fn the_blocks_are_read_by_id_with_the_last_entry_winning_and_the_dead_ones_marked() {
        let blocks = device_axes_from_text(&file(), &alive(&[LEFT, RIGHT]));
        let names: Vec<(&str, &str, bool)> =
            blocks.iter().map(|b| (b.name.as_str(), b.ident.as_str(), b.active)).collect();
        assert_eq!(
            names,
            [
                ("LEFT VPC Stick WarBRD-D", LEFT, true),
                ("Mouse", "", true),
                ("LEFT VPC Stick WarBRD-D", OLD, false),
                ("RIGHT VPC Stick", RIGHT, true),
            ]
        );
        let left = &blocks[0];
        assert_eq!(left.axes.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), ["x", "y"], "in file order");
        assert_eq!(left.axis("x"), Some(&Values { deadzone: Some(0.152), saturation: Some(0.84799999) }));
        assert_eq!(left.axis("y"), Some(&Values { deadzone: Some(0.15000001), saturation: None }));
        assert!(left.multiple.is_empty(), "the same value twice is the normal case");
        // Two different values on one axis in one block: the last one wins and the contradiction is noted.
        let right = &blocks[3];
        assert_eq!(right.axis("x").and_then(|v| v.deadzone), Some(0.3));
        assert_eq!(right.multiple, ["x"]);
        // A dead block with the name of a live one: overtaken; the mouse has no id and counts.
        assert!(blocks[2].overtaken && !blocks[2].orphaned);
        assert!(blocks[1].active && !blocks[1].overtaken && !blocks[1].orphaned);
    }

    #[test]
    fn a_device_with_two_names_and_one_id_keeps_only_the_name_the_game_uses_now() {
        // The game names the device in `<options Product>`: "LEFT VPC Stick WarBRD-D". One block (first in the
        // file) carries an older name of the same id, the second the name of the game: only the second counts.
        let text = file()
            .replacen("name=\"LEFT VPC Stick WarBRD-D  {03F33344", "name=\"L-VPC Stick WarBRD-D  {03F33344", 1)
            .replace(OLD, LEFT);
        let blocks = device_axes_from_text(&text, &alive(&[LEFT, RIGHT]));
        let left: Vec<(&str, bool)> =
            blocks.iter().filter(|b| b.ident == LEFT).map(|b| (b.name.as_str(), b.active)).collect();
        assert_eq!(left, [("L-VPC Stick WarBRD-D", false), ("LEFT VPC Stick WarBRD-D", true)]);
        // No block carries the name of the game: nothing is done, both stay (one tab too many is better than the
        // right one gone).
        let text = text.replace("name=\"LEFT VPC Stick WarBRD-D  {03F33344", "name=\"R-VPC Stick  {03F33344");
        let blocks = device_axes_from_text(&text, &alive(&[LEFT, RIGHT]));
        assert!(blocks.iter().filter(|b| b.ident == LEFT).all(|b| b.active), "no name fits");
        // Two blocks under the name of the game: the first counts.
        let text = file().replace(OLD, LEFT);
        let blocks = device_axes_from_text(&text, &alive(&[LEFT, RIGHT]));
        let left: Vec<bool> = blocks.iter().filter(|b| b.ident == LEFT).map(|b| b.active).collect();
        assert_eq!(left, [true, false]);
        // Not a hint in the other direction: a single block is never taken down.
        let blocks = device_axes_from_text(&file(), &alive(&[LEFT, RIGHT]));
        assert!(blocks.iter().filter(|b| b.ident == LEFT).all(|b| b.active));
    }

    #[test]
    fn contradictions_over_two_blocks_are_only_found_within_the_same_state() {
        let text = format!(
            r#"<ActionProfiles>
 <deviceoptions name="A  {{{LEFT}}}"><option input="x" deadzone="0.1"/></deviceoptions>
 <deviceoptions name="A  {{{LEFT}}}"><option input="x" deadzone="0.2"/></deviceoptions>
 <deviceoptions name="B  {{{OLD}}}"><option input="y" deadzone="0.1"/></deviceoptions>
 <deviceoptions name="B  {{{OLD}}}"><option input="y" deadzone="0.2"/></deviceoptions>
</ActionProfiles>"#
        );
        let blocks = device_axes_from_text(&text, &alive(&[LEFT]));
        assert!(blocks[0].active && blocks[1].active && !blocks[2].active);
        assert_eq!(blocks[0].multiple, ["x"]);
        assert_eq!(blocks[1].multiple, ["x"]);
        assert_eq!(blocks[2].multiple, ["y"], "two dead blocks contradict each other too");
        // Live against dead is no contradiction.
        let text = format!(
            r#"<ActionProfiles>
 <deviceoptions name="A  {{{LEFT}}}"><option input="x" deadzone="0.1"/></deviceoptions>
 <deviceoptions name="A  {{{OLD}}}"><option input="x" deadzone="0.2"/></deviceoptions>
</ActionProfiles>"#
        );
        let blocks = device_axes_from_text(&text, &alive(&[LEFT]));
        assert!(blocks.iter().all(|b| b.multiple.is_empty()), "{blocks:?}");
    }

    #[test]
    fn the_game_axes_come_by_number_and_the_empty_places_are_left_out() {
        let game = game_axes_from_text(&file(), &alive(&[LEFT, RIGHT]));
        assert_eq!(game.len(), 2, "the empty place has no Product");
        assert_eq!((game[0].kind.as_str(), game[0].number, game[0].ident.as_str()), ("joystick", 1, LEFT));
        let pitch = &game[0].axes[0];
        assert_eq!(pitch.0, "flight_move_pitch");
        assert_eq!((pitch.1.exponent, pitch.1.invert), (Some(1.5), Some(1.0)));
        let yaw = &game[0].axes[1].1;
        assert_eq!((yaw.exponent, yaw.invert), (None, Some(0.0)));
        let roll = &game[1].axes[0].1;
        assert_eq!(roll.curve, [(Some(0.5), Some(0.1))]);
        assert!(roll.has_curve_block);
    }

    #[test]
    fn the_functions_on_an_axis_are_found_through_the_binding_and_named_by_the_game() {
        assert_eq!(game_axis_name("v_pitch"), "flight_move_pitch");
        assert_eq!(game_axis_name("v_view_pitch"), "flight_view_pitch");
        assert_eq!(game_axis_name("v_mining_throttle"), "mining_throttle");
        assert_eq!(game_axis_name("ui_cancel"), "ui_cancel");
        let text = file();
        let game = game_axes_from_text(&text, &alive(&[LEFT, RIGHT]));
        let on_y = functions_on_axis_from_text(&text, 1, "y", &game);
        let names: Vec<&str> = on_y.iter().map(|f| f.axis.as_str()).collect();
        assert_eq!(
            names,
            ["flight_move_pitch", "flight_move_strafe_vertical", "flight_view_pitch"],
            "several on one axis"
        );
        assert_eq!(on_y[0].exponent, Some(1.5));
        assert_eq!(on_y[1].exponent, None);
        let on_x = functions_on_axis_from_text(&text, 2, "x", &game);
        assert_eq!(on_x.len(), 1);
        assert_eq!((on_x[0].action.as_str(), on_x[0].exponent), ("v_roll", Some(0.5)));
        assert!(functions_on_axis_from_text(&text, 1, "rotz", &game).is_empty());
    }

    #[test]
    fn what_hangs_on_an_old_id_can_be_carried_over_and_a_thousandth_is_no_difference() {
        let summary = summary_from_text(&file(), &alive(&[LEFT, RIGHT]));
        assert_eq!(
            summary.blocks.iter().map(|b| b.active).collect::<Vec<_>>(),
            [true, true, true, false],
            "live first"
        );
        assert_eq!(summary.orphans.len(), 1);
        assert!(summary.orphans[0].overtaken);
        assert_eq!(summary.conflicts, [("RIGHT VPC Stick".to_string(), "x".to_string())]);
        assert_eq!(summary.adoptable.len(), 1);
        let one = &summary.adoptable[0];
        // x: old 0.099 against 0.152 now (differs), rotx: old saturation 0.7 against nothing (lost).
        assert_eq!(
            one.values,
            [("x".to_string(), Prop::Deadzone, 0.099, Some(0.152)), ("rotx".to_string(), Prop::Saturation, 0.7, None),]
        );
        // 0.098999992 against 0.099 is no difference.
        let text = file().replace("0.152\"/>", "0.0995\"/>");
        let carried = summary_from_text(&text, &alive(&[LEFT, RIGHT]));
        let values = &carried.adoptable[0].values;
        assert_eq!(values.len(), 1, "{values:?}");
        assert_eq!(values[0].0, "rotx");
    }

    #[test]
    fn a_file_that_is_not_xml_gives_nothing_and_a_comment_is_no_block() {
        assert!(device_axes_from_text("<deviceoptions name=\"A\"><option input=\"x\"", &alive(&[])).is_empty());
        let text = "<A><!-- <deviceoptions name=\"Old  {03F33344-0000-0000-0000-504944564944}\"/> --></A>";
        assert!(device_axes_from_text(text, &alive(&[])).is_empty());
    }

    #[test]
    fn the_answer_is_the_straight_line_without_settings_and_follows_the_rules_of_4_12() {
        let none: [(Option<f64>, Option<f64>); 0] = [];
        let a = |x: f64, dz: f64, sat: f64, exp: f64| answer(x, Some(dz), Some(sat), Some(exp), &none);
        for x in [-1.0, -0.3, 0.0, 0.2, 0.9, 1.0] {
            assert_eq!(answer(x, None, None, None, &none), x, "the straight line");
        }
        assert_eq!(a(0.05, 0.1, 1.0, 1.0), 0.0);
        assert!((a(0.55, 0.1, 1.0, 1.0) - 0.5).abs() < 1e-12);
        assert_eq!(a(0.5, 0.0, 0.5, 1.0), 1.0);
        assert!((a(0.25, 0.0, 0.5, 1.0) - 0.5).abs() < 1e-12);
        assert!((a(0.5, 0.0, 1.0, 2.0) - 0.25).abs() < 1e-12);
        assert!((a(0.5, 0.1, 0.9, 2.0) - 0.25).abs() < 1e-12);
        assert_eq!(a(0.95, 0.9, 0.2, 1.0), 1.0, "saturation under the dead zone: full deflection");
        assert_eq!(a(5.0, 0.0, 1.0, 1.0), 1.0);
        assert!((a(0.25, 0.0, 1.0, 2.0) - 0.0625).abs() < 1e-12);
        assert_eq!(a(-0.5, 0.0, 1.0, 1.0), -0.5, "the sign stays");
        assert_eq!(answer(1.0, Some(0.0), None, Some(1.0), &none), 1.0, "no saturation means the whole way");
        // The points of a curve win over the exponent.
        let curve = [(Some(0.0), Some(0.0)), (Some(0.5), Some(0.1)), (Some(1.0), Some(1.0))];
        assert!((answer(0.5, None, None, Some(3.0), &curve) - 0.1).abs() < 1e-12);
        assert!((answer(0.25, None, None, None, &curve) - 0.05).abs() < 1e-12);
    }

    #[test]
    fn the_curve_is_a_row_of_pairs_for_the_quadrant_or_the_whole() {
        let quadrant = progression(None, None, None, &[], 4, false);
        assert_eq!(quadrant, [(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (0.75, 0.75), (1.0, 1.0)]);
        let whole = progression(Some(0.5), None, None, &[], 4, true);
        assert_eq!(whole.first(), Some(&(-1.0, -1.0)));
        assert_eq!(whole[2], (0.0, 0.0));
        assert_eq!(progression(None, None, None, &[], 0, false).len(), 3, "at least two steps");
    }

    #[test]
    fn the_page_shows_two_decimals_and_a_dash_for_nothing() {
        for (value, expected) in [
            (Some(0.099), "0.1"),
            (Some(0.098999992), "0.1"),
            (Some(0.0), "0"),
            (Some(1.0), "1"),
            (Some(0.125), "0.12"),
            (Some(0.375), "0.38"),
            (Some(0.85000002), "0.85"),
            (Some(10.0), "10"),
            (None, "—"),
        ] {
            assert_eq!(two_decimals(value), expected, "{value:?}");
        }
    }
}
