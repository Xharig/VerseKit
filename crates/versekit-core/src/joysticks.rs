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

//! The controls of the game: which devices the game saw, which are bound in the `actionmaps.xml`, and what
//! lies on them (`joysticks.py` of 4.12, the page "Steuerung"). This part only **reads**.
//!
//! ⚠⚠ **The position in the log is not the number in the binding.** The log calls the left stick `joystick0`
//! while the `actionmaps.xml` gives `instance="1"` (`js1`) to the right one, and the binding works in the game.
//! The game knows its devices by the stored id, not by the order they were found in. Whoever compares position
//! with number takes a healthy setup for a shifted one and swaps **all** devices when rewriting it. So the
//! numbers are never touched; the one case that really breaks is a device that reports a **new id** (another
//! USB port, new firmware), and exactly that is [`State::Replaced`].
//!
//! ⚠ **There can be several `actionmaps.xml`.** Under Windows `USER` and `user` are one folder; on a file system
//! that tells the case apart (Wine) they are two, with different content, and the older one is a leftover. The
//! newest one wins: as soon as the game writes, its file is the newest, so the choice corrects itself.
//!
//! The file is read with a real XML parser; a broken or half written file is no reason to fail, the page shows
//! it as not readable.

use std::cmp::Ordering;
use std::sync::OnceLock;

use regex::Regex;
use roxmltree::{Document, Node};

use crate::paths::{self, Paths};
use crate::text::py_strip;
use crate::usercfg::normalized;

/// Of the start of a log this much (in characters) is read; what comes after does not change the device list.
pub const LOG_HEAD: usize = 200_000;
/// How many bytes are read for that: a character takes at most four.
const LOG_HEAD_BYTES: u64 = 4 * LOG_HEAD as u64;
/// How many joystick places the game knows; the `actionmaps.xml` makes eight empty ones.
pub const SLOTS: u64 = 8;

/// The four spellings of the folders above the file.
const TOPS: [&str; 2] = ["USER", "user"];
const MIDDLES: [&str; 2] = ["Client", "client"];
/// The folder of the saved profiles, in all four spellings.
const MAPPING_FOLDERS: [[&str; 2]; 4] =
    [["controls", "mappings"], ["Controls", "mappings"], ["controls", "Mappings"], ["Controls", "Mappings"]];

/// `Connected joystick<N>: Name {GUID}`. The name may hold blanks; the id stands in curly brackets behind it.
/// ⚠ The name is read "not greedy" and the blanks before the id are cut: in the real log there are two blanks
/// between name and id for some devices, one for others.
fn connected_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"Connected joystick([0-9]+):\s*(.+?)\s*\{([0-9A-Fa-f-]+)\}").expect("pattern"))
}

/// The id in the `Product` of a device: `Name {GUID}`.
fn id_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{([0-9A-Fa-f-]+)\}").expect("pattern"))
}

/// `js1_`, `kb1_`, `mo1_`, `gp1_` in front of an input.
fn prefix_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(js|kb|mo|gp)([0-9]+)_").expect("pattern"))
}

// ------------------------------------------------------------------ Files

/// The identity of a file that stays the same under every spelling of its path.
fn identity(path: &str) -> Option<(u64, u64)> {
    let id = paths::file_identity(path);
    (id != (0, 0)).then_some(id)
}

/// The files and folders newest first. The sort is stable: with the same time the order of the search stays.
fn newest_first(found: &mut [String]) {
    let mut stamped: Vec<(f64, String)> =
        found.iter().map(|path| (paths::mtime_seconds(path).unwrap_or(0.0), path.clone())).collect();
    stamped.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    for (slot, (_, path)) in found.iter_mut().zip(stamped) {
        *slot = path;
    }
}

fn root(paths: &Paths, folder: Option<&str>) -> Option<String> {
    match folder.filter(|f| !f.is_empty()) {
        Some(folder) => Some(folder.to_string()),
        None => paths.game_folder(),
    }
}

/// **All** `actionmaps.xml` there are, newest first, without the ones that are the same file under another
/// spelling (under Windows `USER` and `user` are one folder).
pub fn all_actionmaps(paths: &Paths, folder: Option<&str>) -> Vec<String> {
    let Some(base) = root(paths, folder) else { return Vec::new() };
    let flavor = paths.host.flavor;
    let mut found: Vec<((u64, u64), String)> = Vec::new();
    for top in TOPS {
        for middle in MIDDLES {
            let path = flavor.join_all(&base, &[top, middle, "0", "Profiles", "default", "actionmaps.xml"]);
            if !std::path::Path::new(&path).is_file() {
                continue;
            }
            let Some(id) = identity(&path) else { continue };
            if !found.iter().any(|(known, _)| *known == id) {
                found.push((id, path));
            }
        }
    }
    let mut list: Vec<String> = found.into_iter().map(|(_, path)| path).collect();
    newest_first(&mut list);
    list
}

/// The `actionmaps.xml` the game really works with: the newest.
pub fn actionmaps_path(paths: &Paths, folder: Option<&str>) -> Option<String> {
    all_actionmaps(paths, folder).into_iter().next()
}

/// **All** folders of saved profiles there are, newest first. Which profiles exist is asked of all of them;
/// where a new one goes is the first.
pub fn all_mapping_folders(paths: &Paths, folder: Option<&str>) -> Vec<String> {
    let Some(base) = root(paths, folder) else { return Vec::new() };
    let flavor = paths.host.flavor;
    let mut found: Vec<((u64, u64), String)> = Vec::new();
    for top in TOPS {
        for middle in MIDDLES {
            for parts in MAPPING_FOLDERS {
                let path = flavor.join_all(&base, &[top, middle, "0", parts[0], parts[1]]);
                if !std::path::Path::new(&path).is_dir() {
                    continue;
                }
                // ⚠ 4.12 compared the spelling of the path only: where the file system does not tell the case
                // apart, one folder came up sixteen times. The folder itself decides here.
                let Some(id) = identity(&path) else { continue };
                if !found.iter().any(|(known, _)| *known == id) {
                    found.push((id, path));
                }
            }
        }
    }
    let mut list: Vec<String> = found.into_iter().map(|(_, path)| path).collect();
    newest_first(&mut list);
    list
}

/// The saved profiles, by name, without regard to case. ⚠ Star Citizen makes **two** files when it exports itself:
/// `<name>.xml` and `layout_<name>_exported.xml`. A profile is loaded over the first; the second is a second
/// copy and would only double the list. Asked of **all** folders, not only the one a new profile goes into, or
/// the player misses profiles he does have.
pub fn profiles(paths: &Paths, folder: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for dir in all_mapping_folders(paths, folder) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let Ok(file) = entry.file_name().into_string() else { continue };
            let lower = file.to_lowercase();
            if !lower.ends_with(".xml") || lower.starts_with("layout_") {
                continue;
            }
            // Python cuts four characters: `.xml` in any case.
            let name: String = file.chars().take(file.chars().count() - 4).collect();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
    names
}

/// The path of a saved profile, the newest of the folders first; `None` without a name or a file.
pub fn profile_file(paths: &Paths, name: &str, folder: Option<&str>) -> Option<String> {
    let name = py_strip(name);
    if name.is_empty() {
        return None;
    }
    let wanted = format!("{name}.xml");
    all_mapping_folders(paths, folder)
        .into_iter()
        .map(|dir| paths.host.flavor.join_all(&dir, &[&wanted]))
        .find(|full| std::path::Path::new(full).is_file())
}

/// The saved bindings there are to look at: the profiles of the player and the copies the game exports itself
/// (`layout_<name>_exported.xml`), each file name once (the newest folder wins), sorted by name without
/// regard to case: `(file name, path)`. Unlike [`profiles`] the copies of the game count: a player who
/// exported in the game has only them.
pub fn saved_files(paths: &Paths, folder: Option<&str>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for dir in all_mapping_folders(paths, folder) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let Ok(file) = entry.file_name().into_string() else { continue };
            if !file.to_lowercase().ends_with(".xml") || !entry.path().is_file() {
                continue;
            }
            if !out.iter().any(|(known, _)| known.eq_ignore_ascii_case(&file)) {
                let path = paths.host.flavor.join_all(&dir, &[&file]);
                out.push((file, path));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then_with(|| a.0.cmp(&b.0)));
    out
}

/// May this be the name of a profile? It becomes the name of the file and is what the player types in the game
/// (`pp_rebindkeys load <name>`): no blanks, no special signs. Gives the key of the text on a refusal.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    let name = py_strip(name);
    if name.is_empty() {
        return Err("s_js_f_name_leer");
    }
    if name.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')) {
        return Err("s_js_f_name_zeichen");
    }
    if name.chars().count() > 60 {
        return Err("s_js_f_name_lang");
    }
    // ⚠ Not in 4.12: a name Windows keeps for devices (`CON.xml` never becomes a file) and one that starts
    // with `layout_` (the game's own copies; `profiles` skips them, so the profile would be saved and never listed).
    let lower = name.to_ascii_lowercase();
    let device = matches!(lower.as_str(), "con" | "prn" | "aux" | "nul")
        || (lower.len() == 4
            && (lower.starts_with("com") || lower.starts_with("lpt"))
            && lower.as_bytes()[3].is_ascii_digit()
            && lower.as_bytes()[3] != b'0');
    if device || lower.starts_with("layout_") {
        return Err("s_js_f_name_reserviert");
    }
    Ok(())
}

/// The text of an XML file as the parser takes it: UTF-8, without a mark of the byte order. `None` where the
/// file is not there or is not UTF-8 (the game writes UTF-8).
pub(crate) fn read_xml(path: &str) -> Option<String> {
    use std::io::Read;
    // A file of the game is some hundred kilobytes; what is far larger is none of ours.
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_XML_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_XML_BYTES {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

/// The largest XML file that is read (32 MiB).
const MAX_XML_BYTES: u64 = 32 << 20;

/// What can be said of the binding file of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    /// There is none.
    Missing,
    /// There is one, but it can not be read now (locked, no UTF-8, not well formed, far too large): an answer
    /// "no binding" would be wrong, and it is not kept.
    Unreadable,
    /// Read and well formed.
    Fine,
}

/// The state of the binding file (`file`, else the newest of the game).
pub fn file_state(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> FileState {
    let Some(path) = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)) else {
        return FileState::Missing;
    };
    if !std::path::Path::new(&path).is_file() {
        return FileState::Missing;
    }
    match read_xml(&path) {
        Some(text) if parse(&text).is_some() => FileState::Fine,
        _ => FileState::Unreadable,
    }
}

/// The tree of an XML text; `None` where it is not well formed. A DOCTYPE is allowed, like in 4.12.
fn parse(text: &str) -> Option<Document<'_>> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..roxmltree::ParsingOptions::default() };
    Document::parse_with_options(text, options).ok()
}

// ---------------------------------------------------------------- Devices

/// A device the game reported in the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// The number the game gives (`joystick<N>`).
    pub slot: u64,
    pub name: String,
    /// The id in capital letters.
    pub id: String,
}

/// A device that has a binding in the `actionmaps.xml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assigned {
    /// The `instance`, the `n` in `js<n>_`.
    pub number: i64,
    pub name: String,
    /// The id in capital letters; empty where `Product` has none.
    pub id: String,
}

/// The devices of a log text in the order of the finds. The same line can come up more than once in a session
/// (plugging in again while the game runs): the first find counts.
pub fn devices_from_text(text: &str) -> Vec<Device> {
    let mut found: Vec<Device> = Vec::new();
    for hit in connected_pattern().captures_iter(text) {
        let Ok(slot) = hit[1].parse::<u64>() else { continue };
        if found.iter().any(|device| device.slot == slot) {
            continue;
        }
        crate::streamer::learn_device_id(&hit[3]);
        found.push(Device { slot, name: py_strip(&hit[2]).to_string(), id: hit[3].to_uppercase() });
    }
    found.sort_by_key(|device| device.slot);
    found
}

/// The start of a log as text: bad bytes become the replacement sign, every line end becomes `\n`, at most
/// [`LOG_HEAD`] characters.
fn log_head(path: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(LOG_HEAD_BYTES).read_to_end(&mut bytes).ok()?;
    let text = normalized(&String::from_utf8_lossy(&bytes));
    Some(text.chars().take(LOG_HEAD).collect())
}

/// The devices from the newest log of the game: the running `Game.log` first; where nothing stands in it, the
/// newest of the kept ones. Without a start of the game there is no list.
pub fn devices(paths: &Paths, folder: Option<&str>) -> Vec<Device> {
    let mut files: Vec<String> = Vec::new();
    if let Some(running) = paths.game_log(folder).filter(|p| std::path::Path::new(p).is_file()) {
        files.push(running);
    }
    // `log_backups` has the oldest first; the newest is wanted.
    files.extend(paths.log_backups(folder).into_iter().rev());
    for file in files {
        let Some(head) = log_head(&file) else { continue };
        let hit = devices_from_text(&head);
        if !hit.is_empty() {
            return hit;
        }
    }
    Vec::new()
}

// -------------------------------------------------------------- XML reading

fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

/// Is this an element without a namespace with this name? (A name in a namespace would not match in 4.12.)
fn is_element(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

/// `int()` of Python for the text of an attribute: blanks around, a sign, digits with single underscores
/// between them. Digits other than ASCII are not taken.
pub fn py_int_text(text: &str) -> Option<i64> {
    let text = py_strip(text);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    let mut value: i64 = 0;
    for c in digits.chars() {
        if c == '_' {
            continue;
        }
        let digit = i64::from(c.to_digit(10).filter(|_| c.is_ascii_digit())?);
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(if negative { -value } else { value })
}

/// The devices of the binding, sorted by number. Empty places (`<options type="joystick" instance="7"/>`) say
/// nothing and are left out. A file that is not there or not readable gives an empty list.
pub fn assignment(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Vec<Assigned> {
    let Some(path) = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)) else { return Vec::new() };
    if !std::path::Path::new(&path).is_file() {
        return Vec::new();
    }
    let Some(text) = read_xml(&path) else { return Vec::new() };
    assignment_from_text(&text)
}

/// [`assignment`] for the text of the file.
pub fn assignment_from_text(text: &str) -> Vec<Assigned> {
    let Some(document) = parse(text) else { return Vec::new() };
    let mut out: Vec<Assigned> = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "options")) {
        if attribute(node, "type").unwrap_or("").to_lowercase() != "joystick" {
            continue;
        }
        let product = attribute(node, "Product").unwrap_or("");
        if py_strip(product).is_empty() {
            continue;
        }
        let instance = attribute(node, "instance").filter(|v| !v.is_empty());
        let number = match instance {
            None => 0,
            Some(value) => match py_int_text(value) {
                Some(number) => number,
                None => continue,
            },
        };
        let id = id_pattern().captures(product).map(|hit| hit[1].to_uppercase()).unwrap_or_default();
        crate::streamer::learn_device_id(&id);
        out.push(Assigned { number, name: py_strip(&id_pattern().replace_all(product, "")).to_string(), id });
    }
    out.sort_by_key(|entry| entry.number);
    out
}

// ---------------------------------------------------------------- Comparison

/// What a comparison can find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Every bound device is connected: all well.
    Matches,
    /// A device reports a **new** id (can be repaired).
    Replaced,
    /// A bound device is not connected at all.
    Missing,
    /// No data (never played, file missing).
    Empty,
}

impl State {
    /// The word of 4.12 (`passt`, `ersetzt`, `fehlt`, `leer`).
    pub fn word(self) -> &'static str {
        match self {
            State::Matches => "passt",
            State::Replaced => "ersetzt",
            State::Missing => "fehlt",
            State::Empty => "leer",
        }
    }
}

/// The result of [`compare`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub state: State,
    /// What the game connected last.
    pub devices: Vec<Device>,
    /// What stands in the `actionmaps.xml`.
    pub stored: Vec<Assigned>,
    /// Bound devices that are not connected now.
    pub missing: Vec<Assigned>,
    /// Connected devices without a binding.
    pub new: Vec<Device>,
    /// `(old entry, new device)`: only where exactly **one** bound device is missing and exactly **one** is new.
    /// With more at once the player decides, not the program: a wrong guess swaps two sticks.
    pub replacement: Option<(Assigned, Device)>,
    pub file: Option<String>,
}

/// Is every bound device still there, and under the same id? Over the **name** nothing runs: the same device
/// stands in log and binding under different spellings.
pub fn compare(paths: &Paths, folder: Option<&str>, file: Option<&str>) -> Comparison {
    let found = devices(paths, folder);
    let stored = assignment(paths, file, folder);
    let path = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder));
    compare_lists(found, stored, path)
}

/// [`compare`] on lists already read.
pub fn compare_lists(found: Vec<Device>, stored: Vec<Assigned>, file: Option<String>) -> Comparison {
    let mut result = Comparison {
        state: State::Empty,
        devices: found,
        stored,
        missing: Vec::new(),
        new: Vec::new(),
        replacement: None,
        file,
    };
    if result.devices.is_empty() || result.stored.is_empty() {
        return result;
    }
    result.missing = result
        .stored
        .iter()
        .filter(|entry| !entry.id.is_empty() && !result.devices.iter().any(|d| d.id == entry.id))
        .cloned()
        .collect();
    result.new = result
        .devices
        .iter()
        .filter(|device| !result.stored.iter().any(|entry| !entry.id.is_empty() && entry.id == device.id))
        .cloned()
        .collect();
    if result.missing.len() == 1 && result.new.len() == 1 {
        result.replacement = Some((result.missing[0].clone(), result.new[0].clone()));
        result.state = State::Replaced;
    } else if !result.missing.is_empty() {
        result.state = State::Missing;
    } else {
        result.state = State::Matches;
    }
    result
}

// ------------------------------------------------------------------ Bindings

/// The kinds of devices in the `actionmaps.xml` in the order they appear in the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Joystick,
    Keyboard,
    Mouse,
    Gamepad,
}

impl Kind {
    /// All kinds, in the order of the page.
    pub const ALL: [Kind; 4] = [Kind::Joystick, Kind::Keyboard, Kind::Mouse, Kind::Gamepad];

    /// The word of 4.12 (`joystick`, `tastatur`, `maus`, `gamepad`).
    pub fn word(self) -> &'static str {
        match self {
            Kind::Joystick => "joystick",
            Kind::Keyboard => "tastatur",
            Kind::Mouse => "maus",
            Kind::Gamepad => "gamepad",
        }
    }

    /// The prefix in front of the input (`js`, `kb`, `mo`, `gp`).
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Joystick => "js",
            Kind::Keyboard => "kb",
            Kind::Mouse => "mo",
            Kind::Gamepad => "gp",
        }
    }

    fn from_prefix(prefix: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.prefix() == prefix)
    }
}

/// One binding: an input of a device that triggers an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    /// What is pressed, without the prefix: `button10`, `x`, `f5`.
    pub input: String,
    /// The name the game gives: `v_eject`.
    pub action: String,
    /// The group around it: `spaceship_movement`.
    pub section: String,
    pub kind: Kind,
}

/// The first number in a text (`slider2` → 2); 0 where there is none.
fn trailing_number(text: &str) -> u128 {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(if digits.is_empty() { 0 } else { u128::MAX })
}

/// Axes before buttons, buttons by their number instead of their text (`button10` after `button2`).
/// ⚠ Only with sticks: on a keyboard `x`, `y` and `z` are letters, and keyboard and mouse are sorted by name.
fn sort_key(entry: &Binding) -> (u8, u128, String) {
    sort_key_of(&entry.input, Some(entry.kind))
}

/// [`sort_key`] for an input and the kind of its device (`None`: unknown, sorted like a stick).
pub(crate) fn sort_key_of(input: &str, kind: Option<Kind>) -> (u8, u128, String) {
    if matches!(kind, Some(Kind::Keyboard | Kind::Mouse)) {
        return (0, 0, input.to_string());
    }
    const AXES: [&str; 6] = ["x", "y", "z", "rotx", "roty", "rotz"];
    if let Some(index) = AXES.iter().position(|axis| *axis == input) {
        return (0, index as u128, String::new());
    }
    if input.starts_with("slider") {
        return (1, trailing_number(input), input.to_string());
    }
    if input.starts_with("button") {
        return (2, trailing_number(input), input.to_string());
    }
    (3, 0, input.to_string())
}

/// What lies on the devices: per device (`js1`, `kb1`, `mo1`) a list of bindings, devices in the order they
/// first appear. ⚠ This works for **every** device without any template: the file itself says which button
/// triggers which action.
pub fn bindings(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Vec<(String, Vec<Binding>)> {
    let Some(path) = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)) else { return Vec::new() };
    if !std::path::Path::new(&path).is_file() {
        return Vec::new();
    }
    let Some(text) = read_xml(&path) else { return Vec::new() };
    bindings_from_text(&text)
}

/// [`bindings`] for the text of the file.
pub fn bindings_from_text(text: &str) -> Vec<(String, Vec<Binding>)> {
    let Some(document) = parse(text) else { return Vec::new() };
    let mut out: Vec<(String, Vec<Binding>)> = Vec::new();
    for group in document.descendants().filter(|n| is_element(*n, "actionmap")) {
        let section = attribute(group, "name").unwrap_or("");
        for action in group.descendants().filter(|n| is_element(*n, "action")) {
            let name = attribute(action, "name").unwrap_or("");
            for rebind in action.descendants().filter(|n| is_element(*n, "rebind")) {
                let input = py_strip(attribute(rebind, "input").unwrap_or(""));
                let Some(hit) = prefix_pattern().captures(input) else { continue };
                let whole = hit.get(0).expect("whole match");
                let id = format!("{}{}", &hit[1], &hit[2]);
                let entry = Binding {
                    input: input[whole.end()..].to_string(),
                    action: name.to_string(),
                    section: section.to_string(),
                    kind: Kind::from_prefix(&hit[1]).expect("prefix of the pattern"),
                };
                match out.iter_mut().find(|(known, _)| *known == id) {
                    Some((_, list)) => list.push(entry),
                    None => out.push((id, vec![entry])),
                }
            }
        }
    }
    for (_, entries) in &mut out {
        entries.sort_by_key(sort_key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-joysticks-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn put(path: &str, text: &str) {
        let parent = std::path::Path::new(path).parent().expect("parent");
        std::fs::create_dir_all(parent).expect("folder");
        std::fs::write(path, text).expect("file");
    }

    fn age(path: &str, seconds: u64) {
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(at)).expect("age");
    }

    /// A throwaway home with the game folder `StarCitizen/LIVE` in it; nothing real is touched.
    fn world(name: &str) -> (Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        let vars = [
            ("SC_BP_HOME", home.as_str()),
            ("HOME", home.as_str()),
            ("LOCALAPPDATA", home.as_str()),
            (ENV_INSTALL_DIR, game.as_str()),
            (game_start::ENV_NO_PROCESS, "1"),
        ];
        (Paths::new(Host::new(Flavor::native(), &vars, &home)), game)
    }

    const MAPS: &str = "<ActionMaps>\n <ActionProfiles version=\"1\">\n  <options type=\"keyboard\" instance=\"1\" Product=\"Tastatur  {6F1D2B60-D5A0-11CF-BFC7-444553540000}\"/>\n  <options type=\"joystick\" instance=\"2\" Product=\" Stick R  {3344ABCD-0000-0000-0000-504944564944}\"/>\n  <options type=\"Joystick\" instance=\"1\" Product=\" Stick L  {1234abcd-0000-0000-0000-504944564944}\"/>\n  <options type=\"joystick\" instance=\"7\"/>\n  <options type=\"joystick\" instance=\"x\" Product=\"Kaputt {AAAA}\"/>\n  <options type=\"joystick\" instance=\"3\" Product=\"Ohne Kennung\"/>\n  <actionmap name=\"spaceship_movement\">\n   <action name=\"v_eject\">\n    <rebind input=\"js1_button10\"/>\n    <rebind input=\"js1_button2\"/>\n    <rebind input=\"js1_rotz\"/>\n    <rebind input=\"js2_slider2\"/>\n    <rebind input=\"js1_x\"/>\n    <rebind input=\"js1_hat1_up\"/>\n    <rebind input=\"kb1_z\"/>\n    <rebind input=\"kb1_a\"/>\n    <rebind input=\"mo1_mouse1\"/>\n    <rebind input=\"xx1_foo\"/>\n    <rebind input=\"js_button1\"/>\n   </action>\n  </actionmap>\n </ActionProfiles>\n</ActionMaps>\n";

    #[test]
    fn the_devices_of_a_log_come_in_order_of_their_number_and_the_first_find_counts() {
        let text = "x\nConnected joystick1:  Stick R  {3344abcd-0000-0000-0000-504944564944}\nConnected joystick0:   Stick L {1234ABCD-0000-0000-0000-504944564944}\nConnected joystick1: Anderer {FFFF}\nConnected joystick: ohne Nummer {AA}\nConnected joystick2: ohne Klammer\n";
        let found = devices_from_text(text);
        assert_eq!(
            found,
            vec![
                Device { slot: 0, name: "Stick L".into(), id: "1234ABCD-0000-0000-0000-504944564944".into() },
                Device { slot: 1, name: "Stick R".into(), id: "3344ABCD-0000-0000-0000-504944564944".into() },
            ]
        );
        assert!(devices_from_text("").is_empty());
    }

    #[test]
    fn the_binding_lists_the_devices_by_number_and_leaves_out_empty_places() {
        let stored = assignment_from_text(MAPS);
        let line: Vec<(i64, &str, &str)> = stored.iter().map(|a| (a.number, a.name.as_str(), a.id.as_str())).collect();
        assert_eq!(
            line,
            vec![
                (1, "Stick L", "1234ABCD-0000-0000-0000-504944564944"),
                (2, "Stick R", "3344ABCD-0000-0000-0000-504944564944"),
                (3, "Ohne Kennung", ""),
            ],
            "the type is read without regard to case, the keyboard is no joystick, an instance that is no number is skipped"
        );
        assert!(assignment_from_text("<ActionMaps><options").is_empty(), "a broken file is no reason to fail");
        assert!(assignment_from_text("").is_empty());
    }

    #[test]
    fn an_instance_is_read_like_python_reads_a_number() {
        for (text, want) in [
            ("3", Some(3)),
            (" 3 ", Some(3)),
            ("+3", Some(3)),
            ("-3", Some(-3)),
            ("1_0", Some(10)),
            ("007", Some(7)),
            ("", None),
            ("_1", None),
            ("1_", None),
            ("1__0", None),
            ("1.5", None),
            ("x", None),
            ("٣", None),
            ("99999999999999999999", None),
        ] {
            assert_eq!(py_int_text(text), want, "{text:?}");
        }
    }

    fn device(slot: u64, id: &str) -> Device {
        Device { slot, name: format!("D{slot}"), id: id.into() }
    }

    fn entry(number: i64, id: &str) -> Assigned {
        Assigned { number, name: format!("A{number}"), id: id.into() }
    }

    #[test]
    fn the_comparison_names_the_one_case_that_can_be_repaired_and_guesses_nothing_else() {
        let none = compare_lists(Vec::new(), vec![entry(1, "A")], None);
        assert_eq!(none.state, State::Empty, "no log, no list");
        let blank = compare_lists(vec![device(0, "A")], Vec::new(), None);
        assert_eq!(blank.state, State::Empty, "no binding, no list");

        let fine = compare_lists(vec![device(0, "B"), device(1, "A")], vec![entry(1, "A"), entry(2, "B")], None);
        assert_eq!(fine.state, State::Matches, "the position in the log is not the number in the binding");
        assert!(fine.missing.is_empty() && fine.new.is_empty() && fine.replacement.is_none());

        let swapped = compare_lists(vec![device(0, "B"), device(1, "C")], vec![entry(1, "A"), entry(2, "B")], None);
        assert_eq!(swapped.state, State::Replaced);
        assert_eq!(swapped.replacement, Some((entry(1, "A"), device(1, "C"))));

        let two =
            compare_lists(vec![device(0, "C"), device(1, "D")], vec![entry(1, "A"), entry(2, "B")], Some("f".into()));
        assert_eq!(two.state, State::Missing, "two at once: the player decides");
        assert_eq!(two.replacement, None);
        assert_eq!((two.missing.len(), two.new.len()), (2, 2));
        assert_eq!(two.file.as_deref(), Some("f"));

        let gone = compare_lists(vec![device(0, "A")], vec![entry(1, "A"), entry(2, "B")], None);
        assert_eq!(gone.state, State::Missing, "a device unplugged: missing, nothing new");
        assert!(gone.new.is_empty());

        let bare = compare_lists(vec![device(0, "A")], vec![entry(1, "A"), entry(3, "")], None);
        assert_eq!(bare.state, State::Matches, "an entry without an id says nothing");
    }

    #[test]
    fn the_bindings_are_sorted_like_one_looks_at_a_device() {
        let all = bindings_from_text(MAPS);
        let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["js1", "js2", "kb1", "mo1"], "in the order they first appear; unknown prefixes are left out");
        let inputs = |id: &str| -> Vec<String> {
            all.iter().find(|(known, _)| known == id).expect("device").1.iter().map(|b| b.input.clone()).collect()
        };
        assert_eq!(inputs("js1"), ["x", "rotz", "button2", "button10", "hat1_up"], "axes, then buttons by number");
        assert_eq!(inputs("js2"), ["slider2"]);
        assert_eq!(inputs("kb1"), ["a", "z"], "on a keyboard z is a letter, not an axis");
        let first = &all[0].1[0];
        assert_eq!(
            (first.action.as_str(), first.section.as_str(), first.kind),
            ("v_eject", "spaceship_movement", Kind::Joystick)
        );
        assert_eq!(all[3].1[0].kind, Kind::Mouse);
        assert!(bindings_from_text("<a").is_empty());
    }

    #[test]
    fn the_newest_of_the_files_wins_and_one_file_under_two_spellings_counts_once() {
        let (paths, game) = world("dateien");
        let upper = format!("{game}/USER/Client/0/Profiles/default/actionmaps.xml");
        put(&upper, MAPS);
        assert_eq!(all_actionmaps(&paths, Some(&game)).len(), 1);
        if cfg!(windows) {
            // `USER` and `user` are one folder here: still one file.
            let found = all_actionmaps(&paths, Some(&game));
            assert_eq!(found[0].replace('\\', "/"), upper);
        } else {
            let lower = format!("{game}/user/client/0/Profiles/default/actionmaps.xml");
            put(&lower, "<ActionMaps/>");
            age(&upper, 600);
            assert_eq!(all_actionmaps(&paths, Some(&game)), vec![lower, upper.clone()]);
        }
        assert_eq!(actionmaps_path(&paths, Some(&game)).as_deref().map(|p| p.contains("actionmaps.xml")), Some(true));
        assert_eq!(assignment(&paths, None, Some(&game)).len(), 3);
        assert_eq!(bindings(&paths, None, Some(&game)).len(), 4);
        assert!(all_actionmaps(&paths, Some(&format!("{game}/gibt-es-nicht"))).is_empty());
    }

    #[test]
    fn the_folders_of_the_profiles_are_all_found_and_a_missing_folder_gives_nothing() {
        let (paths, game) = world("profile");
        assert!(all_mapping_folders(&paths, Some(&game)).is_empty());
        let one = format!("{game}/USER/Client/0/controls/mappings/a.xml");
        put(&one, "<x/>");
        let found = all_mapping_folders(&paths, Some(&game));
        assert_eq!(found.len(), 1, "one folder under any spelling counts once: {found:?}");
        assert!(found[0].to_lowercase().replace('\\', "/").ends_with("controls/mappings"), "{found:?}");
    }

    #[test]
    fn the_saved_files_to_look_at_hold_the_copies_of_the_game_and_no_folder_or_other_file() {
        let (paths, game) = world("gespeichert");
        assert!(saved_files(&paths, Some(&game)).is_empty());
        let folder = format!("{game}/USER/Client/0/controls/mappings");
        put(&format!("{folder}/meins.xml"), "<x/>");
        put(&format!("{folder}/layout_1_exported.xml"), "<x/>");
        put(&format!("{folder}/notiz.txt"), "x");
        put(&format!("{folder}/ordner.xml/innen.xml"), "<x/>");
        let found = saved_files(&paths, Some(&game));
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["layout_1_exported.xml", "meins.xml"], "{found:?}");
        assert!(found.iter().all(|(name, path)| path.replace('\\', "/").ends_with(&format!("/{name}"))), "{found:?}");
    }

    #[test]
    fn the_devices_come_from_the_running_log_or_else_from_the_newest_kept_one() {
        let (paths, game) = world("protokoll");
        assert!(devices(&paths, Some(&game)).is_empty(), "without a start of the game there is no list");

        let old = format!("{game}/logbackups/Game Build(1) 01 Jan 26.log");
        let new = format!("{game}/logbackups/Game Build(2) 02 Jan 26.log");
        put(&old, "Connected joystick0: Alt {AAAA}\n");
        put(&new, "Connected joystick0: Neu {BBBB}\nConnected joystick1: Zwei {CCCC}\n");
        age(&old, 7200);
        age(&new, 3600);
        let found = devices(&paths, Some(&game));
        assert_eq!(found.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Neu", "Zwei"], "the newest kept log");

        put(&format!("{game}/Game.log"), "Start\nnothing about devices\n");
        assert_eq!(devices(&paths, Some(&game)).len(), 2, "a running log without devices does not hide the kept one");

        put(&format!("{game}/Game.log"), "Connected joystick0: Jetzt {DDDD}\r\nConnected joystick1: Auch {EEEE}\r\n");
        let found = devices(&paths, Some(&game));
        assert_eq!(
            found.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["Jetzt", "Auch"],
            "the running log first"
        );
    }

    #[test]
    fn only_the_start_of_a_log_is_read() {
        let (paths, game) = world("kopf");
        let mut text = "x".repeat(LOG_HEAD);
        text.push_str("\nConnected joystick0: Zu spaet {AAAA}\n");
        put(&format!("{game}/Game.log"), &text);
        assert!(devices(&paths, Some(&game)).is_empty(), "behind the head nothing counts");
        let mut early = String::from("Connected joystick0: Frueh {BBBB}\n");
        early.push_str(&"x".repeat(LOG_HEAD + 10));
        put(&format!("{game}/Game.log"), &early);
        assert_eq!(devices(&paths, Some(&game)).len(), 1);
        // The head is counted in characters: with two bytes each, the line still lies inside it.
        let mut wide = "ä".repeat(LOG_HEAD - 100);
        wide.push_str("\nConnected joystick0: Mitte {CCCC}\n");
        put(&format!("{game}/Game.log"), &wide);
        assert_eq!(devices(&paths, Some(&game)).len(), 1, "counted in bytes it would lie behind the end");
    }

    #[test]
    fn the_files_are_put_newest_first() {
        let dir = scratch("alter");
        let (old, middle, new) = (format!("{dir}/a.xml"), format!("{dir}/b.xml"), format!("{dir}/c.xml"));
        for path in [&old, &middle, &new] {
            put(path, "<x/>");
        }
        age(&old, 900);
        age(&middle, 100);
        age(&new, 10);
        let mut list = vec![old.clone(), new.clone(), middle.clone()];
        newest_first(&mut list);
        assert_eq!(list, vec![new, middle, old]);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_told_from_one_that_is_not_there() {
        let (paths, game) = world("zustand");
        assert_eq!(file_state(&paths, None, Some(&game)), FileState::Missing);
        let file = format!("{game}/USER/Client/0/Profiles/default/actionmaps.xml");
        put(&file, MAPS);
        assert_eq!(file_state(&paths, None, Some(&game)), FileState::Fine);
        put(&file, "<ActionMaps><options");
        assert_eq!(file_state(&paths, None, Some(&game)), FileState::Unreadable, "half written");
        std::fs::write(&file, [0xff, 0xfe, 0x00]).expect("bytes");
        assert_eq!(file_state(&paths, None, Some(&game)), FileState::Unreadable, "no UTF-8");
        put(&file, "");
        assert_eq!(file_state(&paths, None, Some(&game)), FileState::Unreadable, "empty");
    }

    #[test]
    fn the_whole_comparison_runs_over_the_files() {
        let (paths, game) = world("gesamt");
        put(&format!("{game}/USER/Client/0/Profiles/default/actionmaps.xml"), MAPS);
        put(
            &format!("{game}/Game.log"),
            "Connected joystick0:  Stick L  {1234ABCD-0000-0000-0000-504944564944}\nConnected joystick1: Neu {9999}\n",
        );
        let result = compare(&paths, Some(&game), None);
        assert_eq!(result.state, State::Replaced);
        assert_eq!(result.state.word(), "ersetzt");
        let (old, new) = result.replacement.expect("one pair");
        assert_eq!((old.number, new.slot), (2, 1));
        assert!(result.file.is_some_and(|f| f.ends_with("actionmaps.xml")));
    }
}
