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

//! Every change of the `actionmaps.xml` (`joysticks.py` of 4.12: `swap_id`, `reset`, `import_file`,
//! `bind_action`). The file holds the whole control of the player; a broken one costs him all of it.
//!
//! ⭐ **The file is changed as text, at the places a change concerns, and nothing else is touched.** 4.12 read the
//! file into a tree and wrote the tree out again: that dropped the declaration and the comments, changed the
//! layout and turned every line end into `\n`. Here the new text is made from the old one by cutting out and
//! putting in pieces (the places come from the parser), so that a change of one key changes one line.
//!
//! The rules that hold for every change:
//! * **Nothing is written that is not well formed.** The new text is parsed before; if it does not parse, the
//!   file stays as it is ([`Failure::Broken`]).
//! * **Only over `game_files::write`**: the game must be quiet (unknown counts as running), a copy of the
//!   file is made first (`actionmaps.xml.versekit-<time>`, ten stay), a failed writing takes its copy back.
//! * The line ends and the mark of the byte order of the file stay as they are.
//! * The numbers of the devices are never touched (see [`crate::joysticks`]): the game knows its devices by the
//!   stored id, and whoever renumbers a healthy setup ruins it.

use std::ops::Range;
use std::sync::OnceLock;

use regex::Regex;
use roxmltree::{Document, Node};

use crate::game_files::{self, WriteError};
use crate::joysticks::{self, actionmaps_path};
use crate::paths::Paths;

/// Why nothing was written.
#[derive(Debug)]
pub enum Failure {
    /// There is no binding file (`s_js_f_datei`).
    NoFile,
    /// There is nothing to do, a missing input (`s_js_f_nichts`).
    Nothing,
    /// The old id stands nowhere in the file (`s_js_f_unbekannt`).
    Unknown,
    /// The file is on this state already (`s_js_f_gleich`).
    Same,
    /// The file can not be read now: locked, no UTF-8, not well formed, far too large (`s_js_f_lesen`).
    Unreadable,
    /// The file offered is no binding file of the game (`s_js_f_fremd`).
    Foreign,
    /// The new text would not be well formed: nothing was written (`s_js_f_schreiben`).
    Broken,
    /// The name of a profile is no good; the key says why.
    Name(&'static str),
    /// There is a profile of this name already (`s_js_f_name_belegt`).
    NameTaken,
    /// The place for a copy lies in the folder of the binding file of the game (`s_js_f_ziel`).
    Target,
    /// The file changed between reading and writing: nothing was written (`s_js_f_geaendert`).
    Changed,
    /// A setting of an axis that does not exist (`s_kv_f_eigenschaft`).
    Property,
    /// The value is no number (`s_kv_f_wert`).
    Value,
    /// The value is outside what is permitted (`s_kv_f_bereich`).
    Range,
    /// The device stands in no block of the file (`s_kv_f_geraet`).
    Device,
    /// A device without an id can not be matched (`s_kv_f_kennung`).
    Ident,
    /// Two devices have no axis in common (`s_ac_nichts_gemeinsam`).
    NothingShared,
    /// There are no dead blocks to take out (`s_ac_nichts_aufzuraeumen`).
    NothingToClean,
    /// A device set of this name exists already (`s_gs_f_name_belegt`).
    SetTaken,
    /// There is no device set of this name (`s_gs_f_unbekannt`).
    SetUnknown,
    /// No device is there that could be saved (`s_gs_f_nichts`).
    SetNothing,
    /// The file of the sets could not be written (`s_gs_f_schreiben`).
    SetWrite,
    /// Everything the set says stands in the file already (`s_gs_f_nichts_zu_tun`).
    NothingToDo,
    /// The game runs, or the copy or the writing failed.
    Write(WriteError),
}

impl Failure {
    /// The key of the text of 4.12 for this failure; `None` where the reason comes from the writing itself.
    pub fn key(&self) -> Option<&'static str> {
        Some(match self {
            Failure::NoFile => "s_js_f_datei",
            Failure::Nothing => "s_js_f_nichts",
            Failure::Unknown => "s_js_f_unbekannt",
            Failure::Same => "s_js_f_gleich",
            Failure::Unreadable => "s_js_f_lesen",
            Failure::Foreign => "s_js_f_fremd",
            Failure::Broken => "s_js_f_schreiben",
            Failure::Name(key) => key,
            Failure::NameTaken => "s_js_f_name_belegt",
            Failure::Target => "s_js_f_ziel",
            Failure::Changed => "s_js_f_geaendert",
            Failure::Property => "s_kv_f_eigenschaft",
            Failure::Value => "s_kv_f_wert",
            Failure::Range => "s_kv_f_bereich",
            Failure::Device => "s_kv_f_geraet",
            Failure::Ident => "s_kv_f_kennung",
            Failure::NothingShared => "s_ac_nichts_gemeinsam",
            Failure::NothingToClean => "s_ac_nichts_aufzuraeumen",
            Failure::SetTaken => "s_gs_f_name_belegt",
            Failure::SetUnknown => "s_gs_f_unbekannt",
            Failure::SetNothing => "s_gs_f_nichts",
            Failure::SetWrite => "s_gs_f_schreiben",
            Failure::NothingToDo => "s_gs_f_nichts_zu_tun",
            Failure::Write(_) => return None,
        })
    }

    /// The reason in the language of the program (with what to do about it where the writing refused).
    pub fn text(&self) -> String {
        match self {
            Failure::Write(error) => error.reason(),
            other => crate::language::t(other.key().expect("a key")),
        }
    }

    /// Is the reason that the game runs (or nobody can tell)?
    pub fn game_not_quiet(&self) -> bool {
        matches!(self, Failure::Write(WriteError::GameRuns | WriteError::GameUnknown))
    }
}

/// What was done.
#[derive(Debug, PartialEq, Eq)]
pub struct Done {
    /// The path of the copy made first, if there was a file to copy.
    pub backup: Option<String>,
    /// How many things were changed (ids, groups).
    pub count: usize,
}

// ------------------------------------------------------------------- the text

/// One change of the text: the part `range` is replaced by `with`.
pub(crate) struct Edit {
    pub(crate) range: Range<usize>,
    pub(crate) with: String,
}

/// The text with the edits made; they must not overlap.
pub(crate) fn apply(text: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
    let mut out = text.to_string();
    for edit in edits {
        out.replace_range(edit.range, &edit.with);
    }
    out
}

pub(crate) fn parse(text: &str) -> Option<Document<'_>> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..roxmltree::ParsingOptions::default() };
    Document::parse_with_options(text, options).ok()
}

pub(crate) fn is_element(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

/// The line end of the file: `\r\n` where it has one, else `\n`.
pub(crate) fn newline(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// The white space at the start of the line the position stands in, if nothing else stands before it.
fn line_indent(text: &str, pos: usize) -> Option<&str> {
    let start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[start..pos];
    before.chars().all(|c| c == ' ' || c == '\t').then_some(before)
}

/// The range of an element widened to the whole line when it is alone on it (so removing it leaves no blank
/// line behind).
pub(crate) fn whole_line(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    if !text[start..range.start].chars().all(|c| c == ' ' || c == '\t') {
        return range;
    }
    let rest = &text[range.end..];
    let after = rest.find('\n').map(|i| &rest[..i]);
    match after {
        Some(tail) if tail.trim_matches(['\r', ' ', '\t']).is_empty() => start..range.end + tail.len() + 1,
        _ => range,
    }
}

/// An attribute value as it is written between double quotes.
pub(crate) fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The edit that puts `child` (a complete element as text) at the end of the children of `parent`.
pub(crate) fn append_child(text: &str, parent: Node, child: &str) -> Edit {
    let range = parent.range();
    let source = &text[range.clone()];
    let nl = newline(text);
    let indent = line_indent(text, range.start).unwrap_or("").to_string();
    let name = parent.tag_name().name();
    if source.ends_with("/>") && !source.contains(&format!("</{name}")) {
        // `<parent/>` becomes `<parent>` child `</parent>`.
        let cut = range.start + source.len() - 2;
        let open = text[range.start..cut].trim_end();
        let at = range.start + open.len();
        return Edit { range: at..range.end, with: format!(">{nl}{indent}  {child}{nl}{indent}</{name}>") };
    }
    let close = range.start + source.rfind("</").expect("a closing tag");
    match line_indent(text, close) {
        Some(_) => {
            let line = text[..close].rfind('\n').map_or(0, |i| i + 1);
            // The children of the element stand one step deeper than it; where it has some, as they do.
            let step = parent
                .children()
                .rfind(Node::is_element)
                .and_then(|last| line_indent(text, last.range().start))
                .map_or_else(|| format!("{indent}  "), str::to_string);
            Edit { range: line..line, with: format!("{step}{child}{nl}") }
        }
        None => Edit { range: close..close, with: child.to_string() },
    }
}

// --------------------------------------------------------------------- the file

/// A binding file read: its text without the mark of the byte order, and whether it had one.
pub(crate) struct Read {
    pub(crate) path: String,
    pub(crate) text: String,
    pub(crate) bom: bool,
}

/// The largest file that is read (32 MiB).
const MAX_BYTES: u64 = 32 << 20;

fn read_file(path: &str) -> Result<(String, bool), Failure> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|_| Failure::Unreadable)?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| Failure::Unreadable)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Failure::Unreadable);
    }
    let text = String::from_utf8(bytes).map_err(|_| Failure::Unreadable)?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => (rest.to_string(), true),
        None => (text, false),
    })
}

pub(crate) fn open(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Result<Read, Failure> {
    let path = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)).ok_or(Failure::NoFile)?;
    if !std::path::Path::new(&path).is_file() {
        return Err(Failure::NoFile);
    }
    let (text, bom) = read_file(&path)?;
    Ok(Read { path, text, bom })
}

/// Writes the new text: only when it is well formed, over the guard of the game and with a copy first.
pub(crate) fn commit(paths: &Paths, read: &Read, new: &str, count: usize) -> Result<Done, Failure> {
    if parse(new).is_none() {
        return Err(Failure::Broken);
    }
    if new == read.text {
        return Err(Failure::Same);
    }
    let mut bytes = Vec::with_capacity(new.len() + 3);
    if read.bom {
        bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
    }
    // ⚠ The file may have been written by another program since it was read (the launcher, the game just before
    // it fell quiet): then the change was made on an old state and would wipe the newer one. Looked at again
    // right before the writing; the copy `game_files::write` makes holds the old state anyway.
    let mut old = bytes.clone();
    old.extend_from_slice(read.text.as_bytes());
    if std::fs::read(&read.path).map_err(|_| Failure::Unreadable)? != old {
        return Err(Failure::Changed);
    }
    bytes.extend_from_slice(new.as_bytes());
    let backup = game_files::write(paths, &read.path, &bytes).map_err(Failure::Write)?;
    Ok(Done { backup, count })
}

// ------------------------------------------------------------------------ swap

fn instance_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"instance="([0-9]+)""#).expect("pattern"))
}

/// The text with `old_id` put in place by `new_id` everywhere (without regard to case when looking, in the
/// spelling of the new id), and a second entry of the same id emptied. Gives the new text and how many places
/// were changed.
///
/// ⭐ **Not a single line of the binding is touched.** It is enough to change the id in the head of the file:
/// all `js<n>_` lines then point to a device that is there again, the smallest possible change of a file that
/// holds the whole control.
///
/// If the game has made a second, empty entry for the device already, the new id would stand twice. The later
/// entry becomes an empty place (`<options type="joystick" instance="N"/>`), the form the game writes for
/// unbound places itself, so that exactly one assignment is left.
///
/// # Errors
///
/// [`Failure::Nothing`] without an id, [`Failure::Unknown`] where the old id does not stand in the text,
/// [`Failure::Same`] where nothing changes, [`Failure::Broken`] where the result would not be well formed.
pub fn swap_id_text(text: &str, old_id: &str, new_id: &str) -> Result<(String, usize), Failure> {
    if old_id.is_empty() || new_id.is_empty() || old_id == new_id {
        return Err(Failure::Nothing);
    }
    // ⚠ The case of the id can differ between log and file: so it is looked for without regard to it, but
    // replaced in the spelling the new id has.
    let pattern = Regex::new(&format!("(?i){}", regex::escape(old_id))).map_err(|_| Failure::Nothing)?;
    let hits = pattern.find_iter(text).count();
    if hits == 0 {
        return Err(Failure::Unknown);
    }
    let replaced = pattern.replace_all(text, regex::NoExpand(new_id)).into_owned();
    let cleared = clear_duplicate_entry(&replaced, new_id);
    if cleared == text {
        return Err(Failure::Same);
    }
    if parse(&cleared).is_none() {
        return Err(Failure::Broken);
    }
    Ok((cleared, hits))
}

/// Two devices across: what lay on the one lies on the other afterwards (`swap_bindings` of 4.12).
///
/// ⭐ **Only the two `Product` entries** of the `<options type="joystick">` blocks change their id; not one of the
/// binding lines is touched. The game knows its devices by the stored id, so it is enough to say which id is which
/// number now. The `<deviceoptions>` stay as they are on purpose: dead zone and saturation belong to the physical
/// device, not to the number.
///
/// ⚠⚠ **In one pass.** Swapping twice (A to B, then B to A) would find the ids just written and turn everything back.
///
/// ⚠ Not in 4.12: both ids must have a block. 4.12 counted the hits only (two hits of one id in two blocks passed
/// and turned both into the other id); here an id without a block of its own is [`Failure::Unknown`]. And only the
/// `Product` entry is looked at, not the whole text of the block.
///
/// # Errors
///
/// [`Failure::Nothing`] without an id or with the same one twice (without regard to case), [`Failure::Unknown`]
/// where an id stands in no block, [`Failure::Same`], [`Failure::Unreadable`].
pub fn swap_bindings_text(text: &str, id_a: &str, id_b: &str) -> Result<(String, usize), Failure> {
    let (a, b) = (crate::text::py_strip(id_a), crate::text::py_strip(id_b));
    if a.is_empty() || b.is_empty() || a.to_uppercase() == b.to_uppercase() {
        return Err(Failure::Nothing);
    }
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let pair = Regex::new(&format!("(?i)({}|{})", regex::escape(a), regex::escape(b))).map_err(|_| Failure::Nothing)?;
    let (mut found_a, mut found_b) = (0, 0);
    let mut edits = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "options")) {
        if node.attribute("type") != Some("joystick") {
            continue;
        }
        let Some(attr) = node.attribute_node("Product") else { continue };
        let range = attr.range_value();
        let raw = &text[range.clone()];
        let swapped = pair.replace_all(raw, |hit: &regex::Captures| {
            if hit[0].to_uppercase() == a.to_uppercase() {
                found_a += 1;
                b.to_string()
            } else {
                found_b += 1;
                a.to_string()
            }
        });
        if swapped != raw {
            edits.push(Edit { range, with: swapped.into_owned() });
        }
    }
    if found_a == 0 || found_b == 0 {
        return Err(Failure::Unknown);
    }
    let new = apply(text, edits);
    if new == text {
        return Err(Failure::Same);
    }
    if parse(&new).is_none() {
        return Err(Failure::Broken);
    }
    Ok((new, found_a + found_b))
}

/// [`swap_bindings_text`] for the file: with a copy first, and only while the game is quiet.
///
/// # Errors
///
/// [`Failure::NoFile`] and the refusals of [`swap_bindings_text`] and of the writing.
pub fn swap_bindings(
    paths: &Paths,
    id_a: &str,
    id_b: &str,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let (new, count) = swap_bindings_text(&read.text, id_a, id_b)?;
    commit(paths, &read, &new, count)
}

/// If the same id stands in two `<options>` heads of sticks, the first stays; the later becomes an empty place.
fn clear_duplicate_entry(text: &str, id: &str) -> String {
    let Some(document) = parse(text) else { return text.to_string() };
    let wanted = id.to_uppercase();
    let mut seen = false;
    let mut edits = Vec::new();
    for node in document.descendants().filter(|n| is_element(*n, "options")) {
        if node.attribute("type") != Some("joystick") {
            continue;
        }
        let range = node.range();
        let tag_end = text[range.clone()].find('>').map_or(range.end, |i| range.start + i + 1);
        let tag = &text[range.start..tag_end];
        if !tag.to_uppercase().contains(&wanted) {
            continue;
        }
        if !seen {
            seen = true;
            continue;
        }
        let Some(number) = instance_pattern().captures(tag) else { continue };
        edits.push(Edit { range, with: format!("<options type=\"joystick\" instance=\"{}\"/>", &number[1]) });
    }
    apply(text, edits)
}

/// Hangs the binding of a device on a new id.
///
/// # Errors
///
/// See [`swap_id_text`]; [`Failure::NoFile`], [`Failure::Unreadable`], and the refusals of the writing.
pub fn swap_id(
    paths: &Paths,
    old_id: &str,
    new_id: &str,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let (new, count) = swap_id_text(&read.text, old_id, new_id)?;
    commit(paths, &read, &new, count)
}

// ----------------------------------------------------------------------- reset

/// The parent of the groups: `<ActionProfiles>` directly under the root, else the root itself.
fn profiles_of<'a, 'i>(document: &'a Document<'i>) -> Node<'a, 'i> {
    let root = document.root_element();
    root.children().find(|n| is_element(*n, "ActionProfiles")).unwrap_or(root)
}

/// The text without any `<actionmap>` (the bindings of the keys). Gives the new text and how many groups were
/// cut out.
///
/// ⚠⚠ **Only the bindings go.** What is set on the devices (dead zones, curves, sensitivity: `<deviceoptions>`,
/// `<options>`) stays: those are settings of the devices, not binding, and whoever resets the binding does not
/// want to measure his dead zones again.
///
/// # Errors
///
/// [`Failure::Unreadable`] where the text is not well formed, [`Failure::Same`] where there is no group.
pub fn reset_text(text: &str) -> Result<(String, usize), Failure> {
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let parent = profiles_of(&document);
    let groups: Vec<Range<usize>> =
        parent.children().filter(|n| is_element(*n, "actionmap")).map(|n| whole_line(text, n.range())).collect();
    if groups.is_empty() {
        return Err(Failure::Same);
    }
    let count = groups.len();
    let new = apply(text, groups.into_iter().map(|range| Edit { range, with: String::new() }).collect());
    Ok((new, count))
}

/// Throws away the own bindings: back to the factory setting.
///
/// # Errors
///
/// See [`reset_text`]; [`Failure::NoFile`] and the refusals of the writing.
pub fn reset(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let (new, count) = reset_text(&read.text)?;
    commit(paths, &read, &new, count)
}

// ---------------------------------------------------------------------- import

/// A binding of the game in the form of the active file. A profile from the folder of the mappings carries its
/// data at the root and has a `CustomisationUIHeader`; the active file has an `<ActionProfiles>` and no head.
/// ⚠ The way back belongs to the way there: whoever copies a profile over the active file puts the wrong form
/// there. A file that has the active form already is kept as it is. Gives the text and the number of groups.
///
/// # Errors
///
/// [`Failure::Foreign`] where the text is no binding of the game.
pub fn active_form_text(source: &str) -> Result<(String, usize), Failure> {
    let document = parse(source).ok_or(Failure::Foreign)?;
    let root = document.root_element();
    if !is_element(root, "ActionMaps") {
        return Err(Failure::Foreign);
    }
    let count = document.descendants().filter(|n| is_element(*n, "actionmap")).count();
    // ⚠ Not in 4.12: a file with the right root and nothing in it (`<ActionMaps/>`) would wipe the whole control
    // of the player. Nothing to play back is no binding of the game.
    if count == 0 && !document.descendants().any(|n| is_element(n, "options")) {
        return Err(Failure::Foreign);
    }
    if root.children().any(|n| is_element(n, "ActionProfiles")) {
        return Ok((source.to_string(), count));
    }
    let nl = newline(source);
    let mut head = String::from("<ActionProfiles");
    for name in ["version", "optionsVersion", "rebindVersion"] {
        if let Some(value) = root.attribute(name) {
            head.push_str(&format!(" {name}=\"{}\"", escape(value)));
        }
    }
    // The active binding is always called `default` in the game; the name in the file only counts for the profile.
    head.push_str(" profileName=\"default\">");
    let mut out = format!("<ActionMaps>{nl} {head}{nl}");
    for child in root.children().filter(Node::is_element) {
        if is_element(child, "CustomisationUIHeader") {
            continue;
        }
        out.push_str(&format!("  {}{nl}", &source[child.range()]));
    }
    out.push_str(&format!(" </ActionProfiles>{nl}</ActionMaps>{nl}"));
    Ok((out, count))
}

/// Plays a binding given before back (both forms are taken, see [`active_form_text`]).
///
/// # Errors
///
/// [`Failure::NoFile`] for the target or the source, [`Failure::Foreign`], and the refusals of the writing.
pub fn import_file(paths: &Paths, source: &str, file: Option<&str>, folder: Option<&str>) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    if source.is_empty() || !std::path::Path::new(source).is_file() {
        return Err(Failure::NoFile);
    }
    let (text, _) = read_file(source).map_err(|_| Failure::Foreign)?;
    let (new, count) = active_form_text(&text)?;
    commit(paths, &read, &new, count)
}

// --------------------------------------------------------------------- profile

/// The categories in the head of a profile. They come from a real output of the game (alpha 4.10): language keys
/// of the game, not inventions. ⚠ If a patch brings categories, the list is to be followed; it describes what
/// stands as a section on the screen of the bindings.
const PROFILE_CATEGORIES: [&str; 11] = [
    "@ui_CCSeatGeneral",
    "@ui_CCSpaceFlight",
    "@ui_CGLightControllerDesc",
    "@ui_CCFPS",
    "@ui_CCEVA",
    "@ui_CCVehicle",
    "@ui_CGEASpectator",
    "@ui_CGUIGeneral",
    "@ui_CGOpticalTracking",
    "@ui_CGInteraction",
    "@ui_CCCamera",
];

/// The active binding as a **profile** of the game under the name `name`.
///
/// ⚠⚠ **The two forms are not the same** (measured on a real output of the game):
///
/// | | active `actionmaps.xml` | profile in the folder of the mappings |
/// |---|---|---|
/// | root | `<ActionMaps>` **without attributes** | `<ActionMaps version=… profileName=…>` |
/// | below | one `<ActionProfiles>` that carries everything | the same blocks **directly** at the root |
/// | head | none | `<CustomisationUIHeader>` with `<devices>` and `<categories>` |
///
/// Whoever copies the active file only gets **no loadable profile**. Everything else is hung one level higher
/// unchanged: the binding itself is **not** touched. A head the source has already (a profile given back) is not
/// doubled (4.12 did that).
///
/// # Errors
///
/// [`Failure::Foreign`] where the text does not parse.
pub fn profile_text(source: &str, name: &str) -> Result<String, Failure> {
    let document = parse(source).ok_or(Failure::Foreign)?;
    let root = document.root_element();
    let under = root.children().find(|n| is_element(*n, "ActionProfiles")).unwrap_or(root);
    let nl = newline(source);
    let mut out = String::from("<ActionMaps");
    for key in ["version", "optionsVersion", "rebindVersion"] {
        if let Some(value) = under.attribute(key) {
            out.push_str(&format!(" {key}=\"{}\"", escape(value)));
        }
    }
    out.push_str(&format!(" profileName=\"{}\">{nl}", escape(name)));
    out.push_str(&format!(" <CustomisationUIHeader label=\"{}\" description=\"\" image=\"\">{nl}", escape(name)));
    out.push_str(&format!("  <devices>{nl}   <keyboard instance=\"1\"/>{nl}   <mouse instance=\"1\"/>{nl}"));
    // ⚠ **Only places with a `Product`.** The file makes eight places for sticks, empty ones too; a measurement
    // found five of them unbound. Empty places in the head would promise devices that are not there.
    for option in under.children().filter(|n| is_element(*n, "options")) {
        if option.attribute("type") == Some("joystick") && option.attribute("Product").is_some_and(|p| !p.is_empty()) {
            let instance = option.attribute("instance").filter(|i| !i.is_empty()).unwrap_or("1");
            out.push_str(&format!("   <joystick instance=\"{}\"/>{nl}", escape(instance)));
        }
    }
    out.push_str(&format!("  </devices>{nl}  <categories>{nl}"));
    for label in PROFILE_CATEGORIES {
        out.push_str(&format!("   <category label=\"{label}\"/>{nl}"));
    }
    out.push_str(&format!("  </categories>{nl} </CustomisationUIHeader>{nl}"));
    for child in under.children().filter(Node::is_element) {
        if is_element(child, "CustomisationUIHeader") {
            continue;
        }
        out.push_str(&format!(" {}{nl}", &source[child.range()]));
    }
    out.push_str(&format!("</ActionMaps>{nl}"));
    if parse(&out).is_none() {
        return Err(Failure::Broken);
    }
    Ok(out)
}

/// Saves the active binding as a profile the game knows: afterwards it loads with `pp_rebindkeys load <name>`.
///
/// It goes into the folder of the mappings of the game (the newest of the spellings; where there is none, next
/// to the active file), so it is a file of the game: the same rules as for every change (the game must be
/// quiet; a profile that is replaced gets a copy first). 4.12 wrote without the guard and without a copy.
///
/// # Errors
///
/// [`Failure::Name`], [`Failure::NoFile`], [`Failure::NameTaken`] where the name is taken and `overwrite` is
/// not set, [`Failure::Foreign`], and the refusals of the writing.
pub fn save_profile(
    paths: &Paths,
    name: &str,
    overwrite: bool,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    joysticks::check_name(name).map_err(Failure::Name)?;
    let name = crate::text::py_strip(name);
    let read = open(paths, file, folder)?;
    let flavor = paths.host.flavor;
    let dir = match joysticks::all_mapping_folders(paths, folder).into_iter().next() {
        Some(dir) => dir,
        None => {
            // None yet: next to the active file, so that the spelling fits the installation.
            // `…/<client>/0/Profiles/default/actionmaps.xml` → `…/<client>/0/controls/mappings`
            let zero = flavor.dirname(&flavor.dirname(&flavor.dirname(&read.path)));
            flavor.join_all(&zero, &["controls", "mappings"])
        }
    };
    let target = flavor.join_all(&dir, &[&format!("{name}.xml")]);
    if std::path::Path::new(&target).exists() && !overwrite {
        return Err(Failure::NameTaken);
    }
    let text = profile_text(&read.text, name)?;
    game_files::check_quiet(paths).map_err(Failure::Write)?;
    std::fs::create_dir_all(&dir).map_err(|e| Failure::Write(WriteError::Failed(e)))?;
    let backup = game_files::write(paths, &target, text.as_bytes()).map_err(Failure::Write)?;
    Ok(Done { backup, count: 1 })
}

/// Writes the list of the bindings (see [`crate::controls::csv_text`]) to a place the player chose.
///
/// # Errors
///
/// [`Failure::Target`] where the place lies in the folder of the binding file, and the error of writing.
pub fn export_csv(
    paths: &Paths,
    target: &str,
    text: &str,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<(), Failure> {
    check_target(paths, target, file, folder)?;
    crate::controls::write_csv(target, text).map_err(|e| Failure::Write(WriteError::Failed(e)))
}

/// The folder a place is in, resolved as far as the file exists (the file itself may be new).
fn folder_of(path: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(path);
    path.file_name()?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new("."));
    std::fs::canonicalize(parent).ok()
}

/// ⚠ Not in 4.12 (which stopped the copy onto itself only by the error of `shutil`): a copy for the player must
/// not land in the folder of the binding file. It is no file of the game then, and `std::fs::copy` of a file
/// onto itself can cut it to nothing, a list typed over `actionmaps.xml` would go past the guard and the copy.
fn check_target(paths: &Paths, target: &str, file: Option<&str>, folder: Option<&str>) -> Result<(), Failure> {
    let Some(there) = folder_of(target) else { return Ok(()) };
    let mut files: Vec<String> = joysticks::all_actionmaps(paths, folder);
    files.extend(file.map(str::to_string));
    for game_file in files {
        if folder_of(&game_file).is_some_and(|dir| dir == there) {
            return Err(Failure::Target);
        }
    }
    Ok(())
}

/// Copies the active binding to a place the player chose (outside the game: no guard, no copy needed).
///
/// # Errors
///
/// [`Failure::NoFile`], [`Failure::Target`], and the error of copying.
pub fn export_xml(paths: &Paths, target: &str, file: Option<&str>, folder: Option<&str>) -> Result<(), Failure> {
    let path = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)).ok_or(Failure::NoFile)?;
    if !std::path::Path::new(&path).is_file() {
        return Err(Failure::NoFile);
    }
    check_target(paths, target, file, folder)?;
    std::fs::copy(&path, target).map(|_| ()).map_err(|e| Failure::Write(WriteError::Failed(e)))
}

// ------------------------------------------------------------------------ bind

fn device_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^([a-z]+)([0-9]+)$").expect("pattern"))
}

fn prefix_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(js|kb|mo|gp)([0-9]+)_").expect("pattern"))
}

/// The text with an action put on an input of a device.
///
/// | | |
/// |---|---|
/// | `action` | `v_eject` |
/// | `section` | `spaceship_general`: the group the action lives in |
/// | `device_id` | `js2`, `kb1`, `mo1` |
/// | `input` | `button10`, `f5`, `lalt+y`, **without** the prefix |
///
/// An **empty** input deletes the binding: the game understands `input=""` as deliberately unbound and then does
/// not take the factory setting either, as the game does itself.
///
/// ⚠⚠ **Only ONE `rebind` for each action and kind of device is written.** The game allows several, but a second
/// binding of the same action on the same kind of device is hardly ever wanted, and whoever makes one unnoticed
/// gets two lines of which one works. Existing entries of the same kind are replaced, not added to; those on
/// **other** kinds stay.
///
/// # Errors
///
/// [`Failure::Nothing`] without action or device (or a device that is no `js1`), [`Failure::Unreadable`].
pub fn bind_action_text(
    text: &str,
    action: &str,
    section: &str,
    device_id: &str,
    input: &str,
) -> Result<String, Failure> {
    if action.is_empty() || device_id.is_empty() {
        return Err(Failure::Nothing);
    }
    let device = device_pattern().captures(device_id).ok_or(Failure::Nothing)?;
    let prefix = device[1].to_string();
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let parent = profiles_of(&document);
    let nl = newline(text);
    let section = if section.is_empty() { "spaceship_general" } else { section };
    let full = if input.is_empty() { format!("{device_id}_") } else { format!("{device_id}_{input}") };
    let rebind = |value: &str| format!("<rebind input=\"{}\"/>", escape(value));

    // The group: anywhere under the parent, else a new one at its end.
    let group =
        parent.descendants().find(|n| is_element(*n, "actionmap") && n.attribute("name").unwrap_or("") == section);
    let Some(group) = group else {
        let child = format!(
            "<actionmap name=\"{}\">{nl}   <action name=\"{}\">{nl}    {}{nl}   </action>{nl}  </actionmap>",
            escape(section),
            escape(action),
            rebind(&full)
        );
        return Ok(apply(text, vec![append_child(text, parent, &child)]));
    };
    // The action in the group (a direct child), else a new one at its end.
    let target = group.children().find(|n| is_element(*n, "action") && n.attribute("name").unwrap_or("") == action);
    let Some(target) = target else {
        let child = format!("<action name=\"{}\">{nl}    {}{nl}   </action>", escape(action), rebind(&full));
        return Ok(apply(text, vec![append_child(text, group, &child)]));
    };
    let mut edits = Vec::new();
    let mut replaced = false;
    for binding in target.children().filter(|n| is_element(*n, "rebind")) {
        let existing = binding.attribute("input").unwrap_or("").trim();
        let kind = prefix_pattern().captures(existing);
        if let Some(kind) = &kind
            && kind[1] == prefix
        {
            if replaced {
                edits.push(Edit { range: whole_line(text, binding.range()), with: String::new() });
            } else if let Some(attr) = binding.attribute_node("input") {
                edits.push(Edit { range: attr.range_value(), with: escape(&full) });
                replaced = true;
            }
        } else if existing.is_empty() && kind.is_none() {
            edits.push(Edit { range: whole_line(text, binding.range()), with: String::new() });
        }
    }
    if !replaced {
        edits.push(append_child(text, target, &rebind(&full)));
    }
    Ok(apply(text, edits))
}

/// Puts an action on an input.
///
/// # Errors
///
/// See [`bind_action_text`]; [`Failure::NoFile`] and the refusals of the writing.
pub fn bind_action(
    paths: &Paths,
    action: &str,
    section: &str,
    device_id: &str,
    input: &str,
    file: Option<&str>,
    folder: Option<&str>,
) -> Result<Done, Failure> {
    let read = open(paths, file, folder)?;
    let new = bind_action_text(&read.text, action, section, device_id, input)?;
    commit(paths, &read, &new, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::joysticks::{assignment_from_text, bindings_from_text};
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    const LEFT: &str = "1234ABCD-0000-0000-0000-504944564944";
    const RIGHT: &str = "3344ABCD-0000-0000-0000-504944564944";
    const NEW: &str = "99990000-0000-0000-0000-504944564944";

    fn sample() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- kept -->\n<ActionMaps version=\"1\">\n <ActionProfiles version=\"1\" profileName=\"default\">\n  <options type=\"joystick\" instance=\"1\" Product=\" Stick L  {{{LEFT}}}\">\n   <flight_move_pitch invert=\"1\"/>\n  </options>\n  <options type=\"joystick\" instance=\"2\" Product=\" Stick R  {{{RIGHT}}}\"/>\n  <actionmap name=\"spaceship_general\">\n   <action name=\"v_eject\">\n    <rebind input=\"js1_button5\"/>\n    <rebind input=\"kb1_ralt+y\"/>\n   </action>\n   <action name=\"v_empty\"/>\n  </actionmap>\n  <actionmap name=\"player\">\n   <action name=\"pl_use\"><rebind input=\"kb1_e\"/></action>\n  </actionmap>\n </ActionProfiles>\n</ActionMaps>\n"
        )
    }

    fn inputs(text: &str, id: &str, action: &str) -> Vec<String> {
        bindings_from_text(text)
            .iter()
            .filter(|(known, _)| known == id)
            .flat_map(|(_, list)| list.iter())
            .filter(|b| b.action == action)
            .map(|b| b.input.clone())
            .collect()
    }

    #[test]
    fn a_new_id_replaces_the_old_one_in_the_head_and_touches_nothing_else() {
        let text = sample();
        let (new, hits) = swap_id_text(&text, &RIGHT.to_lowercase(), NEW).expect("swapped");
        assert_eq!(hits, 1);
        assert_eq!(new, text.replace(RIGHT, NEW), "one id changed, the case of the old one did not matter");
        assert!(new.contains("<!-- kept -->") && new.contains("<flight_move_pitch invert=\"1\"/>"));
        let ids: Vec<String> = assignment_from_text(&new).into_iter().map(|a| a.id).collect();
        assert_eq!(ids, [LEFT, NEW]);
        assert!(matches!(swap_id_text(&text, "FFFF", NEW), Err(Failure::Unknown)));
        assert!(matches!(swap_id_text(&text, LEFT, LEFT), Err(Failure::Nothing)));
        assert!(matches!(swap_id_text(&text, "", NEW), Err(Failure::Nothing)));
    }

    #[test]
    fn a_second_entry_of_the_same_id_becomes_an_empty_place() {
        // The game has made an empty second entry for the new id already.
        let text = sample().replace(
            "  <actionmap name=\"spaceship_general\">",
            &format!("  <options type=\"joystick\" instance=\"3\" Product=\" Stick R neu  {{{NEW}}}\"/>\n  <actionmap name=\"spaceship_general\">"),
        );
        let (new, _) = swap_id_text(&text, RIGHT, NEW).expect("swapped");
        let list = assignment_from_text(&new);
        assert_eq!(list.iter().filter(|a| a.id == NEW).count(), 1, "exactly one assignment is left: {new}");
        assert!(new.contains("<options type=\"joystick\" instance=\"3\"/>"), "{new}");
        // An entry with children is emptied whole, and the file stays well formed.
        let with_children = sample().replace(
            &format!("<options type=\"joystick\" instance=\"2\" Product=\" Stick R  {{{RIGHT}}}\"/>"),
            &format!("<options type=\"joystick\" instance=\"2\" Product=\" Stick R  {{{NEW}}}\"><flight_move_yaw invert=\"1\"/></options>"),
        );
        let (new, _) = swap_id_text(&with_children, LEFT, NEW).expect("swapped");
        assert!(parse(&new).is_some(), "{new}");
        assert_eq!(assignment_from_text(&new).iter().filter(|a| a.id == NEW).count(), 1);
    }

    #[test]
    fn the_reset_cuts_out_the_groups_and_keeps_the_settings_of_the_devices_and_the_layout() {
        let text = sample();
        let (new, groups) = reset_text(&text).expect("reset");
        assert_eq!(groups, 2);
        assert!(!new.contains("actionmap") && !new.contains("v_eject"), "{new}");
        assert!(new.contains("<flight_move_pitch invert=\"1\"/>") && new.contains("<!-- kept -->"), "{new}");
        assert_eq!(new.lines().filter(|l| l.trim().is_empty()).count(), 0, "no blank lines left behind: {new}");
        assert!(matches!(reset_text(&new), Err(Failure::Same)));
        assert!(matches!(reset_text("<ActionMaps"), Err(Failure::Unreadable)));
        // Line ends of the file stay.
        let crlf = text.replace('\n', "\r\n");
        let (new, _) = reset_text(&crlf).expect("reset");
        assert!(new.contains("\r\n") && !new.replace("\r\n", "").contains('\n'), "all line ends stay CRLF");
    }

    #[test]
    fn a_profile_is_brought_into_the_form_of_the_active_file_and_an_active_file_stays() {
        let profile = "<ActionMaps version=\"1\" optionsVersion=\"2\" rebindVersion=\"3\" profileName=\"mine\">\n <CustomisationUIHeader label=\"x\"><devices/></CustomisationUIHeader>\n <options type=\"joystick\" instance=\"1\"/>\n <actionmap name=\"a\"><action name=\"b\"><rebind input=\"js1_x\"/></action></actionmap>\n</ActionMaps>";
        let (active, groups) = active_form_text(profile).expect("active form");
        assert_eq!(groups, 1);
        assert!(active.contains(
            "<ActionProfiles version=\"1\" optionsVersion=\"2\" rebindVersion=\"3\" profileName=\"default\">"
        ));
        assert!(!active.contains("CustomisationUIHeader"), "the head belongs to the profile only");
        assert_eq!(inputs(&active, "js1", "b"), ["x"]);
        let (same, _) = active_form_text(&sample()).expect("active");
        assert_eq!(same, sample());
        assert!(matches!(active_form_text("<Other/>"), Err(Failure::Foreign)));
        assert!(matches!(active_form_text("no xml"), Err(Failure::Foreign)));
    }

    #[test]
    fn an_action_goes_on_an_input_and_replaces_only_what_is_of_the_same_kind() {
        let text = sample();
        // The eject moves to another button of another stick: the stick entry is replaced, the key stays.
        let new = bind_action_text(&text, "v_eject", "spaceship_general", "js2", "button9").expect("bound");
        assert_eq!(inputs(&new, "js2", "v_eject"), ["button9"]);
        assert!(inputs(&new, "js1", "v_eject").is_empty(), "one rebind for each action and kind of device");
        assert_eq!(inputs(&new, "kb1", "v_eject"), ["ralt+y"]);
        assert!(new.contains("<!-- kept -->") && new.contains("<flight_move_pitch invert=\"1\"/>"));
        // A line changed, nothing else: the rest of the text is the same.
        assert_eq!(new, text.replace("js1_button5", "js2_button9"));
        // Deleting: an empty input stays as `js1_`.
        let deleted = bind_action_text(&text, "v_eject", "spaceship_general", "js1", "").expect("deleted");
        assert!(deleted.contains("input=\"js1_\""), "{deleted}");
        assert!(inputs(&deleted, "js1", "v_eject") == [""], "{deleted}");
    }

    #[test]
    fn an_action_that_is_not_there_yet_is_put_in_where_it_belongs() {
        let text = sample();
        // A new action in a group that is there, also one that is written as `<action/>`.
        let new = bind_action_text(&text, "v_new", "spaceship_general", "js1", "button1").expect("bound");
        assert_eq!(inputs(&new, "js1", "v_new"), ["button1"]);
        let empty = bind_action_text(&text, "v_empty", "spaceship_general", "kb1", "f2").expect("bound");
        assert_eq!(inputs(&empty, "kb1", "v_empty"), ["f2"]);
        // A group that is not there yet; without a name the usual one.
        let group = bind_action_text(&text, "x_a", "seat_general", "mo1", "mouse1").expect("bound");
        assert!(group.contains("<actionmap name=\"seat_general\">"), "{group}");
        assert_eq!(inputs(&group, "mo1", "x_a"), ["mouse1"]);
        let usual = bind_action_text(&text, "x_b", "", "js1", "button2").expect("bound");
        assert_eq!(inputs(&usual, "js1", "x_b"), ["button2"]);
        let all = bindings_from_text(&usual);
        assert!(all.iter().flat_map(|(_, l)| l).any(|b| b.action == "x_b" && b.section == "spaceship_general"));
        // Values with signs of the markup are written as they must be.
        let odd = bind_action_text(&text, "a\"<b>&", "seat_general", "kb1", "a&b").expect("bound");
        assert!(parse(&odd).is_some(), "{odd}");
        assert_eq!(bindings_from_text(&odd).iter().flat_map(|(_, l)| l).filter(|b| b.action == "a\"<b>&").count(), 1);
        // Nothing to do.
        assert!(matches!(bind_action_text(&text, "", "s", "js1", "b"), Err(Failure::Nothing)));
        assert!(matches!(bind_action_text(&text, "a", "s", "js", "b"), Err(Failure::Nothing)));
        assert!(matches!(bind_action_text("<x", "a", "s", "js1", "b"), Err(Failure::Unreadable)));
    }

    #[test]
    fn doubles_and_empty_lines_of_the_same_kind_are_cleaned_when_binding() {
        let text = sample().replace(
            "    <rebind input=\"kb1_ralt+y\"/>\n",
            "    <rebind input=\"kb1_ralt+y\"/>\n    <rebind input=\"js1_button7\"/>\n    <rebind input=\"\"/>\n",
        );
        let new = bind_action_text(&text, "v_eject", "spaceship_general", "js1", "button1").expect("bound");
        assert_eq!(inputs(&new, "js1", "v_eject"), ["button1"], "{new}");
        assert!(!new.contains("input=\"\""), "the empty line is gone: {new}");
        assert_eq!(inputs(&new, "kb1", "v_eject"), ["ralt+y"]);
    }

    #[test]
    fn the_line_ends_and_the_layout_of_a_file_stay_when_an_action_is_put_in() {
        let crlf = sample().replace('\n', "\r\n");
        let new = bind_action_text(&crlf, "v_new", "spaceship_general", "js1", "button1").expect("bound");
        assert!(!new.replace("\r\n", "").contains('\n'), "all line ends stay CRLF: {new:?}");
        let tidy = bind_action_text(&sample(), "v_new", "spaceship_general", "js1", "button1").expect("bound");
        assert!(
            tidy.contains(
                "\n   <action name=\"v_new\">\n    <rebind input=\"js1_button1\"/>\n   </action>\n  </actionmap>"
            ),
            "{tidy}"
        );
    }

    // ------------------------------------------------------------ over the files

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-joywrite-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A made-up installation under a throwaway home; the game is quiet for `age` seconds.
    fn world(name: &str, age: u64, body: &[u8]) -> (Paths, String, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let profile = format!("{game}/USER/Client/0/Profiles/default");
        std::fs::create_dir_all(&profile).expect("folder");
        std::fs::write(format!("{profile}/actionmaps.xml"), body).expect("maps");
        std::fs::write(format!("{game}/Game.log"), "x").expect("log");
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(format!("{game}/Game.log"))
            .and_then(|f| f.set_modified(at))
            .expect("age");
        let vars = [
            ("SC_BP_HOME", home.as_str()),
            ("HOME", home.as_str()),
            (ENV_INSTALL_DIR, game.as_str()),
            (game_start::ENV_NO_PROCESS, "1"),
        ];
        (Paths::new(Host::new(Flavor::native(), &vars, &home)), home, format!("{profile}/actionmaps.xml"))
    }

    #[test]
    fn the_file_is_changed_with_a_copy_first_and_with_its_mark_and_line_ends() {
        let mut body = vec![0xef, 0xbb, 0xbf];
        body.extend_from_slice(sample().replace('\n', "\r\n").as_bytes());
        let (paths, home, file) = world("schreiben", 7200, &body);
        let done = bind_action(&paths, "v_eject", "spaceship_general", "js2", "button9", None, None).expect("written");
        let backup = done.backup.expect("a copy was made");
        assert!(backup.contains("actionmaps.xml.versekit-"), "{backup}");
        assert_eq!(std::fs::read(&backup).expect("copy"), body, "the copy is the old file, byte for byte");
        let now = std::fs::read(&file).expect("file");
        assert!(now.starts_with(&[0xef, 0xbb, 0xbf]), "the mark of the byte order stays");
        let text = String::from_utf8(now[3..].to_vec()).expect("utf8");
        assert!(!text.replace("\r\n", "").contains('\n'));
        assert_eq!(inputs(&text, "js2", "v_eject"), ["button9"]);
        // The same again: nothing to do, no second copy.
        assert!(matches!(
            bind_action(&paths, "v_eject", "spaceship_general", "js2", "button9", None, None),
            Err(Failure::Same)
        ));
        assert_eq!(crate::game_files::backups(&file).len(), 1);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn nothing_is_written_while_the_game_runs_or_when_the_file_is_not_readable() {
        let (paths, home, file) = world("laeuft", 5, sample().as_bytes());
        let before = std::fs::read(&file).expect("file");
        let refused = swap_id(&paths, RIGHT, NEW, None, None).expect_err("the game runs");
        assert!(refused.game_not_quiet(), "{refused:?}");
        assert!(!refused.text().is_empty());
        assert!(matches!(reset(&paths, None, None), Err(Failure::Write(_))));
        assert_eq!(std::fs::read(&file).expect("file"), before, "the file stays as it is");
        assert!(crate::game_files::backups(&file).is_empty(), "not even a copy");
        let _ = std::fs::remove_dir_all(home);

        let (paths, home, file) = world("kaputt", 7200, b"<ActionMaps><options");
        assert!(matches!(reset(&paths, None, None), Err(Failure::Unreadable)));
        assert_eq!(std::fs::read(&file).expect("file"), b"<ActionMaps><options");
        std::fs::write(&file, [0xff, 0xfe, 0x00]).expect("bytes");
        assert!(matches!(swap_id(&paths, RIGHT, NEW, None, None), Err(Failure::Unreadable)));
        std::fs::remove_file(&file).expect("remove");
        assert!(matches!(reset(&paths, None, None), Err(Failure::NoFile)));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_writings_of_the_axes_page_refuse_while_the_game_runs_and_leave_no_copy() {
        use crate::axes::Prop;
        use crate::axes_write::{Setting, align, apply_settings, clean_up};
        const LEFT_ID: &str = "03F33344-0000-0000-0000-504944564944";
        const RIGHT_ID: &str = "43F43344-0000-0000-0000-504944564944";
        const OLD_ID: &str = "83F33344-0000-0000-0000-504944564944";
        let body = |dead: &str| {
            format!(
                "<ActionMaps><ActionProfiles><deviceoptions name=\"A  {{{LEFT_ID}}}\"><option input=\"x\" deadzone=\"{dead}\"/></deviceoptions><deviceoptions name=\"B  {{{RIGHT_ID}}}\"><option input=\"x\" deadzone=\"0.3\"/></deviceoptions><deviceoptions name=\"C  {{{OLD_ID}}}\"><option input=\"x\" deadzone=\"0.2\"/></deviceoptions><options type=\"joystick\" instance=\"1\" Product=\"A  {{{LEFT_ID}}}\"><flight_move_pitch exponent=\"1.5\"/></options><options type=\"joystick\" instance=\"2\" Product=\"B  {{{RIGHT_ID}}}\"/></ActionProfiles></ActionMaps>"
            )
        };
        let (paths, home, file) = world("achsen-laeuft", 5, body("0.1").as_bytes());
        // A set is kept beside the game file, whatever the game does; then the file moves on.
        crate::device_sets::save(&paths, "satz", false, None, None).expect("set");
        std::fs::write(&file, body("0.4")).expect("file");
        let before = std::fs::read(&file).expect("file");
        let setting =
            Setting::Device { ident: LEFT_ID.into(), axis: "x".into(), prop: Prop::Deadzone, value: Some(0.5) };
        let refused = [
            apply_settings(&paths, &[setting], None, None).err(),
            align(&paths, LEFT_ID, RIGHT_ID, None, None).err(),
            clean_up(&paths, None, None).err(),
            crate::device_sets::apply(&paths, "satz", None, None).err(),
            swap_bindings(&paths, LEFT_ID, RIGHT_ID, None, None).err(),
        ];
        for (at, refusal) in refused.into_iter().enumerate() {
            let refusal = refusal.unwrap_or_else(|| panic!("writing {at} was not refused"));
            assert!(refusal.game_not_quiet(), "writing {at}: {refusal:?}");
        }
        assert_eq!(std::fs::read(&file).expect("file"), before, "the file stays as it is");
        assert!(crate::game_files::backups(&file).is_empty(), "not even a copy");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_active_binding_becomes_a_loadable_profile_with_a_head_and_the_binding_itself_untouched() {
        let text = sample();
        let profile = profile_text(&text, "Mein_Profil-2").expect("profile");
        let document = parse(&profile).expect("well formed");
        let root = document.root_element();
        assert_eq!(root.tag_name().name(), "ActionMaps");
        assert_eq!(root.attribute("profileName"), Some("Mein_Profil-2"));
        assert_eq!(root.attribute("version"), Some("1"), "the versions of the block come along");
        let head = root.children().find(|n| n.is_element()).expect("head");
        assert_eq!(head.tag_name().name(), "CustomisationUIHeader");
        assert_eq!(head.attribute("label"), Some("Mein_Profil-2"));
        let joysticks: Vec<_> = head.descendants().filter(|n| n.tag_name().name() == "joystick").collect();
        assert_eq!(joysticks.len(), 2, "one for each place with a Product");
        assert_eq!(head.descendants().filter(|n| n.tag_name().name() == "category").count(), 11);
        // The binding is hung one level higher, as it is.
        assert!(profile.contains("<flight_move_pitch invert=\"1\"/>"));
        assert_eq!(inputs(&profile, "js1", "v_eject"), ["button5"]);
        assert!(!profile.contains("ActionProfiles"), "everything is at the root: {profile}");
        // A head that is there is not doubled.
        let again = profile_text(&profile, "Zwei").expect("again");
        assert_eq!(again.matches("<CustomisationUIHeader").count(), 1, "{again}");
        assert!(matches!(profile_text("<x", "a"), Err(Failure::Foreign)));
        // Empty places in the head: not promised.
        let empty = sample().replace("Product=\" Stick R ", "Pro=\" Stick R ");
        assert_eq!(profile_text(&empty, "N").expect("profile").matches("<joystick ").count(), 1);
    }

    #[test]
    fn a_profile_is_saved_into_the_folder_of_the_game_and_a_name_is_not_taken_twice() {
        let (paths, home, file) = world("profil", 7200, sample().as_bytes());
        let game = std::path::Path::new(&file).ancestors().nth(6).expect("game folder").to_path_buf();
        let folder = game.join("USER").join("Client").join("0").join("controls").join("mappings");
        assert!(!folder.exists(), "none yet");
        let done = save_profile(&paths, "Mein", false, None, None).expect("saved");
        assert!(done.backup.is_none(), "a new file has nothing to copy");
        let saved = folder.join("Mein.xml");
        assert!(saved.is_file(), "made next to the active file");
        assert_eq!(joysticks::profiles(&paths, None), ["Mein"]);
        assert_eq!(
            joysticks::profile_file(&paths, " Mein ", None).as_deref().map(|p| p.contains("Mein.xml")),
            Some(true)
        );
        // The name is taken: refused; with overwrite there is a copy of the old one first.
        assert!(matches!(save_profile(&paths, "Mein", false, None, None), Err(Failure::NameTaken)));
        let done = save_profile(&paths, "Mein", true, None, None).expect("replaced");
        assert!(done.backup.expect("a copy").contains("Mein.xml.versekit-"));
        assert_eq!(joysticks::profiles(&paths, None), ["Mein"], "the copy is no profile");
        // The second file the game makes is no profile of its own.
        std::fs::write(folder.join("layout_Mein_exported.xml"), "<x/>").expect("file");
        std::fs::write(folder.join("alpha.XML"), "<x/>").expect("file");
        assert_eq!(joysticks::profiles(&paths, None), ["alpha", "Mein"], "sorted without regard to case");
        // Bad names: nothing is written.
        for (name, key) in [
            ("", "s_js_f_name_leer"),
            ("a b", "s_js_f_name_zeichen"),
            ("ä", "s_js_f_name_zeichen"),
            (&"x".repeat(61), "s_js_f_name_lang"),
        ] {
            match save_profile(&paths, name, false, None, None) {
                Err(Failure::Name(got)) => assert_eq!(got, key, "{name:?}"),
                other => panic!("{name:?}: {other:?}"),
            }
        }
        assert!(joysticks::check_name(&"x".repeat(60)).is_ok());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn nothing_is_made_while_the_game_runs_not_even_the_folder_and_the_copy_goes_to_a_place_the_player_chose() {
        let (paths, home, file) = world("profil-laeuft", 5, sample().as_bytes());
        let game = std::path::Path::new(&file).ancestors().nth(6).expect("game folder").to_path_buf();
        let refused = save_profile(&paths, "Mein", false, None, None).expect_err("the game runs");
        assert!(refused.game_not_quiet(), "{refused:?}");
        assert!(!game.join("USER").join("Client").join("0").join("controls").exists(), "no folder made");
        // The copy for the player is no file of the game: it goes even now.
        let target = format!("{home}/kopie.xml");
        export_xml(&paths, &target, None, None).expect("copied");
        assert_eq!(std::fs::read(&target).expect("copy"), std::fs::read(&file).expect("file"));
        assert!(matches!(
            export_xml(&paths, &format!("{home}/nirgends/kopie.xml"), None, None),
            Err(Failure::Write(_))
        ));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn two_devices_swap_their_numbers_in_one_pass_and_nothing_else_moves() {
        let text = sample().replace(
            "  <actionmap name=\"spaceship_general\">",
            &format!("  <deviceoptions name=\"Stick L  {{{LEFT}}}\"/>\n  <actionmap name=\"spaceship_general\">"),
        );
        let (new, hits) = swap_bindings_text(&text, LEFT, &RIGHT.to_lowercase()).expect("swapped");
        assert_eq!(hits, 2);
        let before = assignment_from_text(&text);
        let after = assignment_from_text(&new);
        assert_eq!((before[0].number, before[0].id.as_str()), (1, LEFT));
        assert_eq!(
            (after[0].number, after[0].id.as_str()),
            (1, RIGHT.to_uppercase().as_str()),
            "number 1 is the right one now"
        );
        assert_eq!((after[1].number, after[1].id.as_str()), (2, LEFT));
        // Not one binding line, not one deviceoptions, nothing but the two ids moved.
        assert_eq!(inputs(&new, "js1", "v_eject"), inputs(&text, "js1", "v_eject"));
        assert!(new.contains("<deviceoptions name=\"Stick L  {1234ABCD-0000-0000-0000-504944564944}\"/>"));
        assert_eq!(
            new.replace(LEFT, "@").replace(&RIGHT.to_lowercase(), "@").replace(RIGHT, "@").len(),
            text.replace(LEFT, "@").replace(RIGHT, "@").len()
        );
        // An id without a block of its own: nothing to swap (4.12 swapped one id with itself twice).
        assert!(matches!(swap_bindings_text(&text, LEFT, "FFFFFFFF-0000"), Err(Failure::Unknown)));
        assert!(matches!(swap_bindings_text(&text, LEFT, &LEFT.to_lowercase()), Err(Failure::Nothing)));
        assert!(matches!(swap_bindings_text(&text, "", RIGHT), Err(Failure::Nothing)));
        // The ids stand in the deviceoptions only: that is no block of a device with a number.
        let only_options =
            "<a><deviceoptions name=\"X  {AAAA1111-0000}\"/><deviceoptions name=\"Y  {BBBB2222-0000}\"/></a>";
        assert!(matches!(swap_bindings_text(only_options, "AAAA1111-0000", "BBBB2222-0000"), Err(Failure::Unknown)));
    }

    #[test]
    fn a_text_that_is_not_well_formed_is_not_written() {
        let (paths, home, file) = world("zzbroken", 7200, sample().as_bytes());
        let before = std::fs::read(&file).expect("file");
        let result = bind_action(&paths, "a\u{1}b", "spaceship_general", "js1", "button1", None, None);
        assert!(matches!(result, Err(Failure::Broken)), "{result:?}");
        assert_eq!(std::fs::read(&file).expect("file"), before, "the file stays as it is");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_binding_is_played_back_from_a_file_and_a_foreign_file_is_refused() {
        let (paths, home, file) = world("einspielen", 7200, sample().as_bytes());
        let source = format!("{home}/profil.xml");
        std::fs::write(
            &source,
            "<ActionMaps version=\"1\"><CustomisationUIHeader/><actionmap name=\"seat_general\"><action name=\"q\"><rebind input=\"kb1_q\"/></action></actionmap></ActionMaps>",
        )
        .expect("source");
        let done = import_file(&paths, &source, None, None).expect("played back");
        assert_eq!(done.count, 1);
        let text = std::fs::read_to_string(&file).expect("file");
        assert_eq!(inputs(&text, "kb1", "q"), ["q"]);
        assert!(text.contains("profileName=\"default\"") && !text.contains("v_eject"));
        std::fs::write(&source, "<Other/>").expect("source");
        assert!(matches!(import_file(&paths, &source, None, None), Err(Failure::Foreign)));
        assert!(matches!(import_file(&paths, &format!("{home}/gibt-es-nicht.xml"), None, None), Err(Failure::NoFile)));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_file_with_the_right_root_and_nothing_in_it_is_no_binding_to_play_back() {
        let (paths, home, file) = world("einspielen-leer", 7200, sample().as_bytes());
        let before = std::fs::read(&file).expect("file");
        let source = format!("{home}/leer.xml");
        for empty in ["<ActionMaps/>", "<ActionMaps><ActionProfiles/></ActionMaps>"] {
            std::fs::write(&source, empty).expect("source");
            assert!(matches!(import_file(&paths, &source, None, None), Err(Failure::Foreign)), "{empty}");
        }
        assert_eq!(std::fs::read(&file).expect("file"), before, "nothing written");
        // With only the places of the sticks (a fresh file of the game) it is a binding.
        std::fs::write(
            &source,
            "<ActionMaps><ActionProfiles><options type=\"joystick\" instance=\"1\"/></ActionProfiles></ActionMaps>",
        )
        .expect("source");
        assert!(import_file(&paths, &source, None, None).is_ok());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_copy_for_the_player_never_lands_in_the_folder_of_the_binding_file() {
        let (paths, home, file) = world("ziel", 7200, sample().as_bytes());
        let before = std::fs::read(&file).expect("file");
        let folder = std::path::Path::new(&file).parent().expect("folder").to_string_lossy().into_owned();
        // The file itself, and a new file beside it: both refused, for the XML copy and for the list.
        for target in [file.clone(), format!("{folder}/liste.csv"), format!("{folder}/../default/actionmaps.xml")] {
            assert!(matches!(export_xml(&paths, &target, None, None), Err(Failure::Target)), "{target}");
            assert!(matches!(export_csv(&paths, &target, "a;b\n", None, None), Err(Failure::Target)), "{target}");
        }
        assert_eq!(std::fs::read(&file).expect("file"), before, "not cut, not overwritten");
        assert!(!std::path::Path::new(&format!("{folder}/liste.csv")).exists());
        // Anywhere else it goes.
        export_csv(&paths, &format!("{home}/liste.csv"), "a;b\n", None, None).expect("list written");
        assert!(std::fs::read(format!("{home}/liste.csv")).expect("list").ends_with(b"a;b\n"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_file_that_changed_since_it_was_read_is_not_written_over() {
        let (paths, home, file) = world("geaendert", 7200, sample().as_bytes());
        let read = open(&paths, None, None).expect("read");
        let new = bind_action_text(&read.text, "v_eject", "spaceship_general", "kb1", "f9").expect("text");
        // Another program writes in between.
        let other = format!("{}\n<!-- launcher -->", sample());
        std::fs::write(&file, &other).expect("other");
        assert!(matches!(commit(&paths, &read, &new, 1), Err(Failure::Changed)));
        assert_eq!(std::fs::read_to_string(&file).expect("file"), other, "the newer state stays");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn names_windows_keeps_and_the_games_own_copies_are_no_profile_names() {
        for name in ["CON", "nul", "Aux", "PRN", "com1", "COM9", "lpt3", "layout_x", "Layout_Mein"] {
            assert_eq!(joysticks::check_name(name), Err("s_js_f_name_reserviert"), "{name}");
        }
        for name in ["console", "com0", "com10", "lpt", "layout", "my_layout_x", "Mein"] {
            assert!(joysticks::check_name(name).is_ok(), "{name}");
        }
    }
}
