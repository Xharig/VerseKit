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

//! The original texts of the game, taken from the `Data.p4k`. Ports
//! `gametext` of the Python version without its own ZIP and zstd reading:
//! the archive is read through [`GameData`].
//!
//! A `global.ini` of a language normally does not lie in the game folder, but
//! every installation holds it in the archive. This module
//!
//! * reads it ([`read_from_archive`], cached once per archive state) without
//!   writing anything,
//! * keeps the names of it apart ([`names_or_fetch`]),
//! * puts the original file where the game expects it ([`fetch_original`],
//!   [`refresh_original`]) and
//! * appends the lines a translation lacks ([`fill_gaps`]).
//!
//! The text file hides the texts of the archive: the game reads it instead.
//! A file that is too old shows `@key` for everything a patch brought.
//! Therefore a file the tool wrote itself is renewed after a patch, and a
//! foreign file is never touched, except for appended missing lines.
//!
//! Every write into the game folder goes through `versekit_core::jsonio`
//! (atomic, checked by the write guard), and a copy of the file is put next
//! to it first (`global.ini.versekit-YYYYMMDD-HHMMSS`, the newest
//! [`BACKUPS_KEPT`] stay; `versekit_core::translation::replace_game_file`). Nothing here asks for a key of the archive.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::UNIX_EPOCH;

use versekit_core::errors;
use versekit_core::game_ini;
use versekit_core::injection;
use versekit_core::jsonio::{self, Map, Newline, Value};
use versekit_core::paths::Paths;
use versekit_core::pypath::Flavor;
use versekit_core::text::{decode_ignore, py_strip};
use versekit_core::translation;

use crate::{ARCHIVE_NAME, Error, GameData};

/// The file with the original names (`NAMES_FILE`).
pub const NAMES_FILE: &str = "namen-original.json";
/// The setting with the archive state the saved names belong to.
pub const NAMES_STAMP: &str = "namen_p4k";
/// The setting with the archive state the placed original belongs to.
pub const ORIGINAL_STAMP: &str = "original_p4k";
/// The setting with the archive state, target file and file state at the last
/// filling of gaps.
pub const GAPS_STAMP: &str = "luecken_stand";
/// How many copies of a game file stay next to it.
pub const BACKUPS_KEPT: usize = 3;

/// The source name of the game's own texts in the note of the text sources.
const ORIGINAL: &str = game_ini::ORIGINAL;
/// The language folder the original texts belong to.
const ENGLISH: &str = "english";
const BOM: &[u8] = b"\xef\xbb\xbf";

/// The saved original names: key to text.
pub type Names = HashMap<String, String>;

// ----------------------------------------------------------------- errors

/// Why a `global.ini` could not be read from the archive. The texts shown to
/// the user belong to the interface; [`ReadError::text_key`] names them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadError {
    /// The game folder holds no `Data.p4k` (or there is no game folder).
    NoArchive,
    /// The archive holds no `global.ini` of this language.
    NoIniInArchive,
    /// The entry was found but is empty.
    Empty,
    /// Anything else, with the reader's own text.
    Other(String),
}

impl ReadError {
    /// The key of the interface text (`m_kein_p4k`, `m_keine_ini_archiv`; an
    /// empty file is no usable one either); `None` for the reader's own text.
    pub const fn text_key(&self) -> Option<&'static str> {
        match self {
            Self::NoArchive => Some("m_kein_p4k"),
            Self::NoIniInArchive | Self::Empty => Some("m_keine_ini_archiv"),
            Self::Other(_) => None,
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoArchive => f.write_str("no Data.p4k in the game folder"),
            Self::NoIniInArchive => f.write_str("the archive holds no global.ini of this language"),
            Self::Empty => f.write_str("the global.ini in the archive is empty"),
            Self::Other(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for ReadError {}

/// Why [`fetch_original`] did not place the file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchError {
    /// The file could not be read from the archive.
    Read(ReadError),
    /// The language has no known place in the game folder.
    UnknownTarget,
    /// The file could not be backed up or written.
    Write(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(f),
            Self::UnknownTarget => f.write_str("the target folder is unknown"),
            Self::Write(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for FetchError {}

/// What [`fetch_original`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchOutcome {
    /// The original was written; its size in bytes.
    Written { bytes: usize },
}

/// The text of an error with its causes, `a: b: c`.
fn error_text(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

// ------------------------------------------------------------ the archive

/// `archive_path`: where the `global.ini` of a language lies in the archive.
pub fn archive_path(language: &str) -> String {
    format!("Data/Localization/{language}/global.ini")
}

fn archive_file(game_folder: &str) -> String {
    Flavor::native().join(game_folder, ARCHIVE_NAME)
}

/// The game folder to use: the one given, else the one found.
fn resolve_folder(paths: &Paths, game_folder: Option<&str>) -> Option<String> {
    match game_folder {
        Some(folder) if !folder.is_empty() => Some(folder.to_string()),
        _ => paths.game_folder(),
    }
}

/// `p4k_path`: the `Data.p4k` of the installation, if there is one.
pub fn p4k_path(paths: &Paths, game_folder: Option<&str>) -> Option<String> {
    let archive = paths.host.flavor.join(&resolve_folder(paths, game_folder)?, ARCHIVE_NAME);
    game_ini::is_file(&archive).then_some(archive)
}

/// Size and modification time in nanoseconds (Python's `st_mtime_ns`).
fn file_stamp_io(path: &str) -> io::Result<(u64, i128)> {
    let meta = fs::metadata(path)?;
    let nanos = match meta.modified()?.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_nanos() as i128,
        Err(before) => -(before.duration().as_nanos() as i128),
    };
    Ok((meta.len(), nanos))
}

fn file_stamp(path: &str) -> Option<(u64, i128)> {
    file_stamp_io(path).ok()
}

/// `archive_stamp`: size and modification time of the `Data.p4k` in
/// `game_folder` as `"<size>:<mtime_ns>"`, or `None` without an archive. A
/// patch changes both; the same stamp means the same content.
pub fn archive_stamp(game_folder: &str) -> Option<String> {
    let archive = archive_file(game_folder);
    if !game_ini::is_file(&archive) {
        return None;
    }
    let (size, nanos) = file_stamp(&archive)?;
    Some(format!("{size}:{nanos}"))
}

/// The reader of the archive: `(archive file, language)` to the raw bytes.
type Reader<'a> = &'a dyn Fn(&str, &str) -> Result<Vec<u8>, ReadError>;

/// What was read last, per archive file, size, time and language.
struct ArchiveCache {
    slot: Mutex<Option<(CacheKey, Arc<Vec<u8>>)>>,
}

type CacheKey = (String, u64, i128, String);

impl ArchiveCache {
    const fn new() -> Self {
        Self { slot: Mutex::new(None) }
    }

    fn lock(&self) -> MutexGuard<'_, Option<(CacheKey, Arc<Vec<u8>>)>> {
        self.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reads once per run and archive state: names, places and mission
    /// titles ask for the same file in threads of their own, often at the
    /// same moment. The lock is held while reading, so the later ones find
    /// the data in the cache. A patch changes size or time, then the file is
    /// read anew.
    fn read(&self, archive: &str, language: &str, reader: Reader<'_>) -> Result<Arc<Vec<u8>>, ReadError> {
        let mut slot = self.lock();
        let key = file_stamp(archive).map(|(size, nanos)| (archive.to_string(), size, nanos, language.to_string()));
        if let (Some(key), Some((held, data))) = (&key, slot.as_ref())
            && held == key
        {
            return Ok(Arc::clone(data));
        }
        let data = Arc::new(reader(archive, language)?);
        if let Some(key) = key {
            *slot = Some((key, Arc::clone(&data)));
        }
        Ok(data)
    }
}

static ARCHIVE_CACHE: ArchiveCache = ArchiveCache::new();

/// The real reading: one entry of the archive, nothing else.
fn read_archive_entry(archive: &str, language: &str) -> Result<Vec<u8>, ReadError> {
    let folder = Path::new(archive).parent().unwrap_or_else(|| Path::new("."));
    let _topic = versekit_core::archive_lock::reading_for("texts");
    let game = GameData::open(folder).map_err(|error| ReadError::Other(error_text(&error)))?;
    match game.read_entry(&archive_path(language)) {
        Ok(bytes) if bytes.is_empty() => Err(ReadError::Empty),
        Ok(bytes) => Ok(bytes),
        Err(Error::MissingEntry { .. }) => Err(ReadError::NoIniInArchive),
        Err(error) => Err(ReadError::Other(error_text(&error))),
    }
}

/// `read_from_archive`: the raw bytes of the `global.ini` of `language`
/// (`english`, `german_(germany)`, ...) in the `Data.p4k` of `game_folder`.
///
/// Reads and writes nothing else: no file into the game, no `g_language`, no
/// discarded original texts. Whoever only looks up a name must not switch the
/// language of the player. Cached for the last archive state of the process.
///
/// # Errors
///
/// [`ReadError::NoArchive`] without a `Data.p4k`, [`ReadError::NoIniInArchive`]
/// when the archive holds no file of the language, otherwise the text of the
/// reader.
pub fn read_from_archive(language: &str, game_folder: &str) -> Result<Arc<Vec<u8>>, ReadError> {
    read_with(&ARCHIVE_CACHE, language, game_folder, &read_archive_entry)
}

fn read_with(
    cache: &ArchiveCache,
    language: &str,
    game_folder: &str,
    reader: Reader<'_>,
) -> Result<Arc<Vec<u8>>, ReadError> {
    let archive = archive_file(game_folder);
    if !game_ini::is_file(&archive) {
        return Err(ReadError::NoArchive);
    }
    cache.read(&archive, language, reader)
}

/// The reading that [`fill_gaps`] and [`names_or_fetch`] use; replaceable for
/// the tests.
type ArchiveRead<'a> = &'a dyn Fn(&str, &str) -> Result<Arc<Vec<u8>>, ReadError>;

// ------------------------------------------------------------------ names

/// The names of a `global.ini` as `(key, text)` in file order.
///
/// A line counts when its key holds `name` anywhere, in any case, not only at
/// the end: ships are called `vehicle_NameAEGS_Sabre_Raven_EX`, with the word
/// in the middle. The `,P` after a key is no part of it, and a star in front
/// of the text is a foreign mark, not part of the name. The first line of a
/// key wins. The rest of the file (mission texts, tooltips, subtitles) is
/// about nine tenths and nobody looks it up here.
pub fn names_from_ini(data: &[u8]) -> Vec<(String, String)> {
    let data = data.strip_prefix(BOM).unwrap_or(data);
    names_from_text(&decode_ignore(data))
}

/// Like [`names_from_ini`], from text.
pub fn names_from_text(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for line in game_ini::py_splitlines(text) {
        let Some(sep) = line.find('=') else { continue };
        if sep < 1 {
            continue;
        }
        let key = &line[..sep];
        if !key.to_lowercase().contains("name") {
            continue;
        }
        let key = key.split(',').next().unwrap_or(key);
        let value = py_strip(py_strip(&line[sep + 1..]).trim_start_matches('*'));
        if !value.is_empty() && seen.insert(key) {
            out.push((key.to_string(), value.to_string()));
        }
    }
    out
}

/// `save_names`: keep the names of a read `global.ini` for good, written
/// atomically. Returns their number, 0 when there are none or on failure.
pub fn save_names(paths: &Paths, data: &[u8]) -> usize {
    let names = names_from_ini(data);
    if names.is_empty() {
        return 0;
    }
    let value = Value::Object(names.iter().map(|(key, text)| (key.clone(), Value::str(text.as_str()))).collect());
    let text = jsonio::dumps(&value, None, false);
    match jsonio::write_text_atomic(&paths.app_file(NAMES_FILE), &text, Newline::Keep) {
        Ok(()) => names.len(),
        Err(error) => {
            errors::record("gametext.save_names", &error);
            0
        }
    }
}

/// `saved_names`: the saved original names, empty when there are none yet.
pub fn saved_names(paths: &Paths) -> Names {
    match jsonio::read_json(&paths.app_file(NAMES_FILE)) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(key, value)| match value {
                Value::Str(text) => Some((key, text)),
                _ => None,
            })
            .collect(),
        _ => Names::new(),
    }
}

/// One try to read the archive per run of the program, not per caller and not
/// per name. It stands here because there are several callers (blueprint names
/// and ship names); with the guard at one of them, the other would read the
/// archive a second time.
static ARCHIVE_TRIED: AtomicBool = AtomicBool::new(false);

/// `names_or_fetch`: the original names, saved or else read once from the
/// `Data.p4k`. Changes nothing in the game: no writing, no `g_language`.
///
/// The saved names belong to an archive state. After a patch the archive is
/// read again, else every new ship and item would be missing for good.
/// Without a readable archive the saved names stay valid.
pub fn names_or_fetch(paths: &Paths, game_folder: Option<&str>) -> Names {
    names_or_fetch_with(paths, game_folder, &ARCHIVE_TRIED, &read_from_archive)
}

fn names_or_fetch_with(paths: &Paths, game_folder: Option<&str>, tried: &AtomicBool, read: ArchiveRead<'_>) -> Names {
    let names = saved_names(paths);
    let folder = resolve_folder(paths, game_folder);
    let stamp = folder.as_deref().and_then(archive_stamp);
    if !names.is_empty()
        && stamp.as_ref().is_none_or(|stamp| paths.setting(NAMES_STAMP).as_deref() == Some(stamp.as_str()))
    {
        return names;
    }
    if tried.swap(true, Ordering::SeqCst) {
        return names;
    }
    let Some(folder) = folder else { return names };
    let Ok(data) = read(ENGLISH, &folder) else { return names };
    if save_names(paths, &data) > 0
        && let Some(stamp) = stamp
    {
        paths.set_setting(NAMES_STAMP, Value::str(stamp));
    }
    saved_names(paths)
}

// ------------------------------------------------------------ place names

/// The file with the place names of the `global.ini` (`play_stats.PLACE_FILE`).
pub const PLACES_FILE: &str = "orte-original.json";

/// The systems whose planets, moons and points the place names hold.
const PLACE_SYSTEMS: [&str; 5] = ["stanton", "pyro", "nyx", "magnus", "castra"];

/// Whether a key of the `global.ini` names a place (`play_stats._PLACE_INI_KEY`, any case): a system, then
/// digits, at most one letter, at most one Lagrange point `_l` with a digit, e.g. `stanton2b`, `pyro2_l4`.
pub fn is_place_key(key: &str) -> bool {
    let low = key.to_lowercase();
    let Some(rest) = PLACE_SYSTEMS.iter().find_map(|system| low.strip_prefix(system)) else { return false };
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let mut chars = rest.chars().peekable();
    if chars.peek().is_some_and(char::is_ascii_lowercase) {
        chars.next();
    }
    let tail: String = chars.collect();
    match tail.strip_prefix("_l") {
        Some(digit) => digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()),
        None => tail.is_empty(),
    }
}

/// The place names of a `global.ini` (`play_stats.place_names`): `{key in lower case: name}`, the first line of a
/// key wins, a line without text counts not.
pub fn places_from_ini(data: &[u8]) -> Vec<(String, String)> {
    let data = data.strip_prefix(BOM).unwrap_or(data);
    let text = decode_ignore(data);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in game_ini::py_splitlines(&text) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = py_strip(key.split(',').next().unwrap_or(key));
        let value = py_strip(value);
        if is_place_key(key) && !value.is_empty() && seen.insert(key.to_lowercase()) {
            out.push((key.to_lowercase(), value.to_string()));
        }
    }
    out
}

/// The saved place names, empty when there are none yet.
pub fn saved_places(paths: &Paths) -> Names {
    match jsonio::read_json(&paths.app_file(PLACES_FILE)) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(key, value)| match value {
                Value::Str(text) => Some((key, text)),
                _ => None,
            })
            .collect(),
        _ => Names::new(),
    }
}

/// One try per run of the program to read the place names from the archive.
static PLACES_TRIED: AtomicBool = AtomicBool::new(false);

/// `play_stats.place_names(fetch=True)`: the saved place names, else read once from the English `global.ini` of
/// the `Data.p4k` and saved, so later looks need no archive. Changes nothing in the game.
pub fn places_or_fetch(paths: &Paths, game_folder: Option<&str>) -> Names {
    places_or_fetch_with(paths, game_folder, &PLACES_TRIED, &read_from_archive)
}

fn places_or_fetch_with(paths: &Paths, game_folder: Option<&str>, tried: &AtomicBool, read: ArchiveRead<'_>) -> Names {
    let places = saved_places(paths);
    if !places.is_empty() || tried.swap(true, Ordering::SeqCst) {
        return places;
    }
    let Some(folder) = resolve_folder(paths, game_folder) else { return places };
    let Ok(data) = read(ENGLISH, &folder) else { return places };
    if data.is_empty() {
        return places;
    }
    let found = places_from_ini(&data);
    let value = Value::Object(found.iter().map(|(key, text)| (key.clone(), Value::str(text.as_str()))).collect());
    if let Err(error) =
        jsonio::write_text_atomic(&paths.app_file(PLACES_FILE), &jsonio::dumps(&value, None, false), Newline::Keep)
    {
        errors::record("play_stats.place_names", &error);
    }
    found.into_iter().collect()
}

// -------------------------------------------------------- the text source

/// The note of the installed text sources (`translation._note`), or empty.
fn read_note(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(game_ini::NOTE_FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

fn note_entry<'a>(note: &'a Map, source: &str) -> Option<&'a Value> {
    note.iter().find(|(key, _)| key == source).map(|(_, value)| value)
}

/// `translation.installed` for the main installation: a source with an
/// identifier in the note.
fn installed(note: &Map, source: &str) -> bool {
    truthy(note_entry(note, source).and_then(|entry| entry.get("kennung")))
}

/// `translation.language_folder` for the main installation.
fn language_folder(paths: &Paths, note: &Map, source: &str) -> Option<String> {
    if source == ORIGINAL {
        return Some(ENGLISH.to_string());
    }
    if source == game_ini::CUSTOM {
        let noted = note_entry(note, source).and_then(|entry| entry.get("sprache")).filter(|value| truthy(Some(value)));
        if let Some(language) = noted {
            return language.as_str().map(str::to_string);
        }
        let settings = paths.settings();
        let custom =
            settings.iter().find(|(key, _)| key == game_ini::SETTING_CUSTOM).and_then(|(_, value)| value.get("MAIN"));
        return match custom.filter(|custom| custom.as_object().is_some()) {
            Some(custom) if truthy(custom.get("url")) => match custom.get("sprache").filter(|v| truthy(Some(v))) {
                Some(language) => language.as_str().map(str::to_string),
                None => Some(ENGLISH.to_string()),
            },
            _ => Some(ENGLISH.to_string()),
        };
    }
    game_ini::SOURCES.iter().find(|(name, _)| *name == source).map(|(_, language)| (*language).to_string())
}

/// `_placed_by_us`: which sources this tool put into the language folder
/// (noted in `uebersetzung.json`), including `original`.
///
/// A file that is there is foreign and stays, unless the tool put it there
/// itself and noted it (StarStrings in `english/`). Empty means: the file is
/// foreign or there is none.
fn placed_by_us(paths: &Paths, language: &str, channel: Option<&str>) -> Vec<String> {
    let Some(channel) = channel else {
        let note = read_note(paths);
        return game_ini::SOURCES
            .iter()
            .map(|(name, _)| *name)
            .chain([game_ini::CUSTOM, ORIGINAL])
            .filter(|source| {
                language_folder(paths, &note, source).as_deref() == Some(language) && installed(&note, source)
            })
            .map(str::to_string)
            .collect();
    };
    // A side channel has its own notes (`source@CHANNEL`); those of the main
    // installation say nothing about its folder.
    game_ini::SOURCES
        .iter()
        .map(|(name, _)| *name)
        .chain([game_ini::CUSTOM, ORIGINAL])
        .filter(|source| {
            versekit_core::translation::installed(paths, source, Some(channel)).is_some()
                && versekit_core::translation::language_folder(paths, source, Some(channel)) == language
        })
        .map(str::to_string)
        .collect()
}

/// The side channel a game folder is: the channel whose folder it is (a
/// folder chosen for a channel may have any name), else its folder name;
/// `None` for the main installation (the game folder of the settings).
fn side_channel(paths: &Paths, folder: &str) -> Option<String> {
    let flavor = paths.host.flavor;
    let same = |a: &str, b: &str| flavor.normcase(&flavor.normpath(a)) == flavor.normcase(&flavor.normpath(b));
    let name = || {
        let channels = paths.installed_channels();
        channels.into_iter().find(|(_, other, _)| same(other, folder)).map_or_else(|| flavor.basename(folder), |c| c.0)
    };
    // Without a known main installation no folder may touch its notes.
    let Some(main) = paths.game_folder() else { return Some(name()) };
    (!same(&main, folder)).then(name)
}

/// `translation.forget_note`: remove the note of a source whose file was
/// replaced (`source` is the key of the note, `source@CHANNEL` for a side
/// channel).
fn forget_note(paths: &Paths, source: &str) {
    let mut note = read_note(paths);
    let Some(position) = note.iter().position(|(key, _)| key == source) else { return };
    note.remove(position);
    let text = jsonio::dumps(&Value::Object(note), Some(1), false);
    if let Err(error) = jsonio::write_text_atomic(&paths.app_file(game_ini::NOTE_FILE), &text, Newline::Native) {
        errors::record("gametext.forget_note", &error);
    }
}

/// `translation.target_ini` with a game folder: where the file of a language
/// folder belongs. An existing `data` or `Data` folder is kept.
fn target_ini(paths: &Paths, language: &str, game_folder: Option<&str>) -> Option<String> {
    if !game_ini::is_language_folder(language) {
        return None;
    }
    let root = resolve_folder(paths, game_folder)?;
    let flavor = paths.host.flavor;
    for spelling in ["data", "Data"] {
        let folder = flavor.join(&root, spelling);
        if fs::metadata(&folder).is_ok_and(|meta| meta.is_dir()) {
            return Some(flavor.join_all(&folder, &["Localization", language, "global.ini"]));
        }
    }
    Some(flavor.join_all(&root, &["data", "Localization", language, "global.ini"]))
}

// ------------------------------------------------------ backup and writing

/// `_write_original`: put the original texts at the target, atomically, after
/// a copy of the file that was there. The remembered original texts of that
/// file are discarded (a fresh base, see `injection::discard_origtext_for`),
/// in the note of the channel the file belongs to.
fn write_original(paths: &Paths, target: &str, data: &[u8]) -> io::Result<()> {
    let folder = paths.host.flavor.dirname(target);
    if !folder.is_empty() {
        jsonio::make_dirs(&folder)?;
    }
    translation::replace_game_file(paths, target, data, BACKUPS_KEPT)?;
    injection::discard_origtext_for(paths, target);
    Ok(())
}

// ---------------------------------------------------------- the original

/// `refresh_original`: renew the placed original texts after a game patch.
/// Returns whether the English `global.ini` was written anew; the mission
/// data must be entered again then, the remembered original texts of the old
/// file are discarded.
///
/// The file hides the texts in the archive. Only what the tool placed itself
/// as "original" is touched, and only if no other own source (StarStrings,
/// own address) holds the folder. Never fails.
pub fn refresh_original(paths: &Paths, game_folder: Option<&str>) -> bool {
    let _one_writer = versekit_core::injection::game_file_lock();
    match refresh_original_with(paths, game_folder, &read_from_archive) {
        Ok(written) => written,
        Err(error) => {
            errors::record("gametext.refresh_original", &error);
            false
        }
    }
}

fn refresh_original_with(paths: &Paths, game_folder: Option<&str>, read: ArchiveRead<'_>) -> io::Result<bool> {
    if !installed(&read_note(paths), ORIGINAL) {
        return Ok(false);
    }
    if placed_by_us(paths, ENGLISH, None).iter().any(|source| source != ORIGINAL) {
        return Ok(false);
    }
    let Some(folder) = resolve_folder(paths, game_folder) else { return Ok(false) };
    if side_channel(paths, &folder).is_some() {
        // The notes and the stamp belong to the main installation.
        return Ok(false);
    }
    let Some(stamp) = archive_stamp(&folder) else { return Ok(false) };
    if paths.setting(ORIGINAL_STAMP).as_deref() == Some(stamp.as_str()) {
        return Ok(false);
    }
    let Some(target) = target_ini(paths, ENGLISH, Some(&folder)) else { return Ok(false) };
    let Ok(data) = read(ENGLISH, &folder) else { return Ok(false) };
    write_original(paths, &target, &data)?;
    paths.set_setting(ORIGINAL_STAMP, Value::str(stamp));
    Ok(true)
}

/// `fetch`: get the `global.ini` of a language from the archive and place it
/// where the game expects it.
///
/// The file that lies there goes first, whoever placed it: what the player
/// gets is the file of the archive and nothing of an older one. It is copied
/// next to it before (see the module text), so another translation is not
/// lost. The notes of the tool's own sources in that folder (StarStrings in
/// `english/`) are forgotten, else the page would still show them as set up.
/// The note "original" itself stays; the file is the original.
///
/// The file alone is not enough: without `g_language` in the `user.cfg` the
/// game does not read it. With `set_language`, `set_user_cfg` is called with
/// the language folder after placing. It returns whether it worked; a failure
/// does not discard the rest.
///
/// # Errors
///
/// [`FetchError::Read`] when the archive or its file is missing,
/// [`FetchError::UnknownTarget`] for a language without a place and
/// [`FetchError::Write`] when the copy or the file cannot be written.
pub fn fetch_original(
    paths: &Paths,
    language: &str,
    game_folder: Option<&str>,
    set_language: bool,
    set_user_cfg: &dyn Fn(&str) -> bool,
) -> Result<FetchOutcome, FetchError> {
    let _one_writer = versekit_core::injection::game_file_lock();
    fetch_original_with(paths, language, game_folder, set_language, set_user_cfg, &read_from_archive)
}

fn fetch_original_with(
    paths: &Paths,
    language: &str,
    game_folder: Option<&str>,
    set_language: bool,
    set_user_cfg: &dyn Fn(&str) -> bool,
    read: ArchiveRead<'_>,
) -> Result<FetchOutcome, FetchError> {
    let folder = resolve_folder(paths, game_folder).ok_or(FetchError::Read(ReadError::NoArchive))?;
    if !game_ini::is_file(&archive_file(&folder)) {
        return Err(FetchError::Read(ReadError::NoArchive));
    }
    let target = target_ini(paths, language, Some(&folder)).ok_or(FetchError::UnknownTarget)?;
    let channel = side_channel(paths, &folder);
    let replaced = placed_by_us(paths, language, channel.as_deref());
    let data = read(language, &folder).map_err(FetchError::Read)?;
    write_original(paths, &target, &data).map_err(|error| FetchError::Write(error_text(&error)))?;
    if language == ENGLISH
        && channel.is_none()
        && let Some(stamp) = archive_stamp(&folder)
    {
        paths.set_setting(ORIGINAL_STAMP, Value::str(stamp));
    }
    for source in replaced.iter().filter(|source| *source != ORIGINAL) {
        forget_note(paths, &versekit_core::translation::note_key(source, channel.as_deref()));
    }
    if set_language && !set_user_cfg(language) {
        errors::record("gametext._set_language", &"g_language could not be entered");
    }
    Ok(FetchOutcome::Written { bytes: data.len() })
}

// ------------------------------------------------------------------ gaps

fn is_space_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Like `bytes.strip()`.
fn strip_bytes(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes
        && is_space_byte(*first)
    {
        bytes = rest;
    }
    while let [rest @ .., last] = bytes
        && is_space_byte(*last)
    {
        bytes = rest;
    }
    bytes
}

/// Like `bytes.splitlines()`: line ends are `\n`, `\r` and `\r\n` only.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < data.len() {
        match data[index] {
            b'\n' => {
                lines.push(&data[start..index]);
                index += 1;
                start = index;
            }
            b'\r' => {
                lines.push(&data[start..index]);
                index += if data.get(index + 1) == Some(&b'\n') { 2 } else { 1 };
                start = index;
            }
            _ => index += 1,
        }
    }
    if start < data.len() {
        lines.push(&data[start..]);
    }
    lines
}

/// The key of a `global.ini` line, lowercase and without the addition after
/// the comma (`name,P` is `name`), or empty for a line without a key and for
/// a comment.
fn ini_key(line: &[u8]) -> Vec<u8> {
    let line = line.strip_prefix(BOM).unwrap_or(line);
    let Some(equals) = line.iter().position(|&byte| byte == b'=') else { return Vec::new() };
    let key = strip_bytes(&line[..equals]);
    if key.first().is_some_and(|&byte| byte == b';' || byte == b'#') {
        return Vec::new();
    }
    let key = key.split(|&byte| byte == b',').next().unwrap_or_default();
    strip_bytes(key).to_ascii_lowercase()
}

/// `missing_lines`: the lines of `original` whose key is missing in
/// `target`, in the order of the original. Both are the bytes of a
/// `global.ini`.
///
/// Keys are compared without case and without the addition after the comma:
/// if `name` is there, `name,P` is not added, else the same text would stand
/// twice and the English one could push out the translated one.
pub fn missing_lines(original: &[u8], target: &[u8]) -> Vec<Vec<u8>> {
    let mut present: HashSet<Vec<u8>> = split_lines(target).into_iter().map(ini_key).collect();
    present.remove(&Vec::new());
    let mut out = Vec::new();
    for line in split_lines(original) {
        let key = ini_key(line);
        if key.is_empty() || present.contains(&key) {
            continue;
        }
        present.insert(key);
        out.push(line.strip_prefix(BOM).unwrap_or(line).to_vec());
    }
    out
}

/// `archive state|target file|size:mtime_ns of the target file`.
fn gaps_mark(paths: &Paths, stamp: &str, target: &str) -> io::Result<String> {
    let (size, nanos) = file_stamp_io(target)?;
    Ok(format!("{stamp}|{}|{size}:{nanos}", paths.host.flavor.normcase(target)))
}

/// `fill_gaps`: a stopgap while there is no newer translation: append what
/// the text file `target` lacks, with the English original from the
/// `Data.p4k`. Returns the number of lines appended; 0 also when nothing was
/// missing or it did not work. Never fails.
///
/// The text file hides the texts in the archive. With a translation older
/// than the game state, everything a patch brought shows as `@key`. Only
/// what is missing is appended, no existing text changes, in the line ending
/// and with the byte order mark of the file. A new version of the translation
/// replaces the file with the addition. Before the file is replaced, a copy
/// is put next to it.
///
/// It looks only when archive or text file changed since the last time
/// ([`GAPS_STAMP`]).
pub fn fill_gaps(paths: &Paths, target: &str, game_folder: Option<&str>) -> usize {
    let _one_writer = versekit_core::injection::game_file_lock();
    match fill_gaps_with(paths, target, game_folder, &read_from_archive) {
        Ok(count) => count,
        Err(error) => {
            errors::record("gametext.fill_gaps", &error);
            0
        }
    }
}

fn fill_gaps_with(paths: &Paths, target: &str, game_folder: Option<&str>, read: ArchiveRead<'_>) -> io::Result<usize> {
    if target.is_empty() || !game_ini::is_file(target) {
        return Ok(0);
    }
    let Some(folder) = resolve_folder(paths, game_folder) else { return Ok(0) };
    let Some(stamp) = archive_stamp(&folder) else { return Ok(0) };
    if Some(gaps_mark(paths, &stamp, target)?) == paths.setting(GAPS_STAMP) {
        return Ok(0);
    }
    let Ok(original) = read(ENGLISH, &folder) else { return Ok(0) };
    let current = fs::read(target)?;
    let lines = missing_lines(&original, &current);
    if !lines.is_empty() {
        let newline: &[u8] =
            if current[..current.len().min(65536)].windows(2).any(|pair| pair == b"\r\n") { b"\r\n" } else { b"\n" };
        let mut extra = Vec::new();
        if !current.is_empty() && !matches!(current.last(), Some(b'\n' | b'\r')) {
            extra.extend_from_slice(newline);
        }
        for line in &lines {
            extra.extend_from_slice(line);
            extra.extend_from_slice(newline);
        }
        let mut bytes = current;
        bytes.extend_from_slice(&extra);
        translation::replace_game_file(paths, target, &bytes, BACKUPS_KEPT)?;
    }
    paths.set_setting(GAPS_STAMP, Value::str(gaps_mark(paths, &stamp, target)?));
    Ok(lines.len())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use p4kx_testkit::{ArchiveBuilder, TestEntry};
    use versekit_core::paths::Host;

    use super::*;

    /// A game folder with a data folder in a throwaway folder.
    struct Fixture {
        root: String,
        game: String,
        paths: Paths,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("versekit-game-text-{name}-{}", versekit_core::testdir::run_tag()));
            let _ = fs::remove_dir_all(&root);
            let root = root.to_string_lossy().into_owned();
            let game = format!("{root}/StarCitizen/LIVE");
            let data = format!("{root}/data");
            fs::create_dir_all(&game).unwrap();
            fs::create_dir_all(&data).unwrap();
            // With its log the folder is known as the main installation.
            fs::write(format!("{game}/Game.log"), "").unwrap();
            let host = Host::new(
                Flavor::native(),
                &[("HOME", root.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", game.as_str())],
                &root,
            );
            Self { root, game, paths: Paths::new(host) }
        }

        /// A `Data.p4k` with an English `global.ini` and a German one.
        fn archive(&self, english: &[u8]) {
            ArchiveBuilder::new()
                .entry(TestEntry::stored("Data\\Localization\\english\\global.ini", english.to_vec()))
                .entry(TestEntry::deflated(
                    "Data\\Localization\\german_(germany)\\global.ini",
                    b"a_name=Hallo\n".to_vec(),
                ))
                .write_to(format!("{}/{ARCHIVE_NAME}", self.game))
                .unwrap();
        }

        /// The place of the English file in the game folder.
        fn english(&self) -> String {
            format!("{}/data/Localization/english/global.ini", self.game)
        }

        fn put(&self, path: &str, bytes: &[u8]) {
            fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        fn note(&self, json: &str) {
            fs::write(self.paths.app_file(game_ini::NOTE_FILE), json).unwrap();
        }

        fn backups(&self, target: &str) -> Vec<String> {
            let folder = Path::new(target).parent().unwrap();
            let prefix = format!("{}.versekit-", Path::new(target).file_name().unwrap().to_string_lossy());
            let mut names: Vec<String> = fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(&prefix))
                .collect();
            names.sort();
            names
        }
    }

    fn file_name_of(path: &str) -> String {
        Path::new(path).file_name().unwrap().to_string_lossy().into_owned()
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn no_language(_: &str) -> bool {
        true
    }

    // ------------------------------------------------------------ names

    #[test]
    fn names_are_found_with_name_anywhere_in_the_key() {
        let raw = "Nozzle_FuelGiver_GRIN_NozzleSecure_Name=Marlin\n\
                   vehicle_NameAEGS_Sabre_Raven_EX=*Aegis Sabre Raven EX\n\
                   vehicle_NameAEGS_Sabre_Raven_EX_short,P=Sabre Raven EX\n\
                   mission_briefing_long=A long mission text\n\
                   broken line without an equals sign\n\
                   =no key\n";
        let names: HashMap<_, _> = names_from_text(raw).into_iter().collect();
        assert_eq!(names["Nozzle_FuelGiver_GRIN_NozzleSecure_Name"], "Marlin");
        // The word in the middle of the key, and no star in front.
        assert_eq!(names["vehicle_NameAEGS_Sabre_Raven_EX"], "Aegis Sabre Raven EX");
        // The `,P` is no part of the key.
        assert_eq!(names["vehicle_NameAEGS_Sabre_Raven_EX_short"], "Sabre Raven EX");
        assert!(!names.contains_key("mission_briefing_long"));
        assert_eq!(names.len(), 3);
        // Bytes and text give the same, also with a byte order mark.
        let mut bytes = BOM.to_vec();
        bytes.extend_from_slice(raw.as_bytes());
        assert_eq!(names_from_ini(raw.as_bytes()), names_from_text(raw));
        assert_eq!(names_from_ini(&bytes), names_from_text(raw));
    }

    #[test]
    fn the_first_line_of_a_key_wins_and_empty_texts_are_skipped() {
        let names =
            names_from_text("a_name=First\r\nA_NAME=Other\r\na_name=Second\r\nb_Name= * \r\nc_NAME =  * Cee \r\n");
        assert_eq!(
            names,
            vec![
                ("a_name".to_string(), "First".to_string()),
                ("A_NAME".to_string(), "Other".to_string()),
                ("c_NAME ".to_string(), "Cee".to_string()),
            ]
        );
        // Invalid bytes are dropped like `decode('utf-8', 'ignore')`.
        assert_eq!(names_from_ini(b"x_name=Caf\xffe\n"), vec![("x_name".to_string(), "Cafe".to_string())]);
    }

    #[test]
    fn names_are_saved_as_json_and_read_back() {
        let fx = Fixture::new("save-names");
        assert!(saved_names(&fx.paths).is_empty());
        assert_eq!(save_names(&fx.paths, b"mission=text\n"), 0);
        assert!(!Path::new(&fx.paths.app_file(NAMES_FILE)).exists());
        assert_eq!(save_names(&fx.paths, "ship_name=Caf\u{e9}\nitem_Name,P=Item\n".as_bytes()), 2);
        let written = fs::read_to_string(fx.paths.app_file(NAMES_FILE)).unwrap();
        assert_eq!(written, "{\"ship_name\": \"Caf\u{e9}\", \"item_Name\": \"Item\"}");
        let names = saved_names(&fx.paths);
        assert_eq!(names["ship_name"], "Caf\u{e9}");
        assert_eq!(names["item_Name"], "Item");
    }

    #[test]
    fn the_archive_is_tried_once_per_run_not_once_per_name() {
        let fx = Fixture::new("tried-once");
        let count = Cell::new(0);
        let tried = AtomicBool::new(false);
        let read = |_: &str, _: &str| {
            count.set(count.get() + 1);
            Err(ReadError::NoArchive)
        };
        for _ in 0..3 {
            assert!(names_or_fetch_with(&fx.paths, Some(&fx.game), &tried, &read).is_empty());
        }
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn names_follow_the_archive_state() {
        let fx = Fixture::new("names-follow");
        fx.archive(b"ship_name=First Ship\nmission=Text\n");
        let first = names_or_fetch_with(&fx.paths, Some(&fx.game), &AtomicBool::new(false), &read_from_archive);
        assert_eq!(first["ship_name"], "First Ship");
        assert_eq!(fx.paths.setting(NAMES_STAMP), archive_stamp(&fx.game));

        // The same state: the saved names are taken, the archive stays unread.
        let count = Cell::new(0);
        let counting = |language: &str, folder: &str| {
            count.set(count.get() + 1);
            read_from_archive(language, folder)
        };
        let again = names_or_fetch_with(&fx.paths, Some(&fx.game), &AtomicBool::new(false), &counting);
        assert_eq!(again, first);
        assert_eq!(count.get(), 0);

        // A patch: the new ship comes in; and a second look in the same run reads nothing.
        fx.archive(b"ship_name=First Ship\nnew_ship_name=Second Ship\nmission=Longer text\n");
        let tried = AtomicBool::new(false);
        let patched = names_or_fetch_with(&fx.paths, Some(&fx.game), &tried, &counting);
        assert_eq!(patched["new_ship_name"], "Second Ship");
        assert_eq!(count.get(), 1);

        // Without a readable archive the saved names stay.
        fs::remove_file(format!("{}/{ARCHIVE_NAME}", fx.game)).unwrap();
        let without = names_or_fetch_with(&fx.paths, Some(&fx.game), &AtomicBool::new(false), &counting);
        assert_eq!(without, patched);
        assert_eq!(count.get(), 1);
    }

    // ------------------------------------------------------ place names

    #[test]
    fn a_place_key_is_a_system_with_digits_a_letter_and_a_lagrange_point() {
        // What `play_stats._PLACE_INI_KEY` of 4.12 says for each of them.
        let table = [
            ("stanton", true),
            ("Stanton1", true),
            ("stanton2b", true),
            ("STANTON2B", true),
            ("pyro2_l4", true),
            ("pyro_l1", true),
            ("pyro6", true),
            ("nyx1a_L2", true),
            ("magnus", true),
            ("castra3", true),
            ("stanton1_l", false),
            ("stanton1_l12", false),
            ("stanton1ab", false),
            ("stanton1_l2x", false),
            ("stanton_desc", false),
            ("pyro2_l4,P", false),
            ("hurston", false),
            ("stanton1a_l2", true),
            ("stantonx", true),
            ("nyx_", false),
            ("castra10c_l9", true),
            ("Magnus_System", false),
            ("pyro5_desc", false),
            ("stanton4a_l1", true),
        ];
        let wrong: Vec<&str> =
            table.iter().filter(|(key, want)| is_place_key(key) != *want).map(|(key, _)| *key).collect();
        assert!(wrong.is_empty(), "judged otherwise than 4.12: {wrong:?}");
    }

    #[test]
    fn the_place_names_of_an_ini_are_lower_case_and_the_first_line_wins() {
        let mut raw = BOM.to_vec();
        raw.extend_from_slice(
            b"Stanton1=Hurston\r\npyro2_L4,P= PYR2 L4 \r\nstanton1=Other\r\nstanton2b=\r\nhurston_desc=Text\r\nnyx=Nyx\r\n",
        );
        assert_eq!(
            places_from_ini(&raw),
            [
                ("stanton1".to_string(), "Hurston".to_string()),
                ("pyro2_l4".to_string(), "PYR2 L4".to_string()),
                ("nyx".to_string(), "Nyx".to_string()),
            ]
        );
    }

    #[test]
    fn the_place_names_are_read_once_from_the_archive_and_then_from_the_file() {
        let fx = Fixture::new("places");
        fx.archive(b"stanton1=Hurston\npyro6=Terminus\nvehicle_Name_X=Ship\n");
        let tried = AtomicBool::new(false);
        let places = places_or_fetch_with(&fx.paths, Some(&fx.game), &tried, &read_from_archive);
        assert_eq!(places.len(), 2);
        assert_eq!(places["pyro6"], "Terminus");
        let written = fs::read_to_string(fx.paths.app_file(PLACES_FILE)).unwrap();
        assert_eq!(written, "{\"stanton1\": \"Hurston\", \"pyro6\": \"Terminus\"}");
        // Saved: no archive is asked again, also not in a new run.
        let count = Cell::new(0);
        let counting = |language: &str, folder: &str| {
            count.set(count.get() + 1);
            read_from_archive(language, folder)
        };
        let again = places_or_fetch_with(&fx.paths, Some(&fx.game), &AtomicBool::new(false), &counting);
        assert_eq!((again, count.get()), (places, 0));
    }

    #[test]
    fn without_saved_place_names_the_archive_is_tried_once_per_run() {
        let fx = Fixture::new("places-once");
        let count = Cell::new(0);
        let tried = AtomicBool::new(false);
        let read = |_: &str, _: &str| {
            count.set(count.get() + 1);
            Err(ReadError::NoArchive)
        };
        for _ in 0..3 {
            assert!(places_or_fetch_with(&fx.paths, Some(&fx.game), &tried, &read).is_empty());
        }
        assert_eq!(count.get(), 1);
        assert!(!Path::new(&fx.paths.app_file(PLACES_FILE)).exists(), "nothing is written without names");
    }

    // ------------------------------------------------------ the archive

    #[test]
    fn the_archive_stamp_is_size_and_time() {
        let fx = Fixture::new("stamp");
        assert_eq!(archive_stamp(&fx.game), None);
        fx.archive(b"a_name=A\n");
        let file = format!("{}/{ARCHIVE_NAME}", fx.game);
        let meta = fs::metadata(&file).unwrap();
        let nanos = meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        assert_eq!(archive_stamp(&fx.game), Some(format!("{}:{nanos}", meta.len())));
        // Compared as paths: the function joins with the system's separator,
        // the fixture's folder is spelt with `/`.
        assert_eq!(
            p4k_path(&fx.paths, Some(&fx.game)).map(std::path::PathBuf::from),
            Some(std::path::PathBuf::from(file))
        );
        assert_eq!(p4k_path(&fx.paths, Some(&format!("{}/none", fx.root))), None);
    }

    #[test]
    fn the_file_comes_from_the_archive_or_says_why_not() {
        let fx = Fixture::new("read");
        assert_eq!(read_from_archive(ENGLISH, &fx.game), Err(ReadError::NoArchive));
        assert_eq!(ReadError::NoArchive.text_key(), Some("m_kein_p4k"));
        fx.archive(b"\xef\xbb\xbfa_name=A\r\n");
        assert_eq!(*read_from_archive(ENGLISH, &fx.game).unwrap(), b"\xef\xbb\xbfa_name=A\r\n".to_vec());
        assert_eq!(*read_from_archive("german_(germany)", &fx.game).unwrap(), b"a_name=Hallo\n".to_vec());
        let missing = read_from_archive("french_(france)", &fx.game);
        assert_eq!(missing, Err(ReadError::NoIniInArchive));
        assert_eq!(missing.unwrap_err().text_key(), Some("m_keine_ini_archiv"));
        // Nothing was written next to the archive.
        let mut files: Vec<_> = fs::read_dir(&fx.game).unwrap().map(|e| e.unwrap().file_name()).collect();
        files.sort();
        assert_eq!(files, vec![std::ffi::OsString::from(ARCHIVE_NAME), std::ffi::OsString::from("Game.log")]);
    }

    #[test]
    fn the_archive_is_read_once_per_state() {
        let fx = Fixture::new("cache");
        fx.archive(b"a_name=A\n");
        let cache = ArchiveCache::new();
        let count = Cell::new(0);
        let reader = |archive: &str, language: &str| {
            count.set(count.get() + 1);
            read_archive_entry(archive, language)
        };
        let first = read_with(&cache, ENGLISH, &fx.game, &reader).unwrap();
        let second = read_with(&cache, ENGLISH, &fx.game, &reader).unwrap();
        assert_eq!(first, second);
        assert_eq!(count.get(), 1);
        // Another language is another state.
        read_with(&cache, "german_(germany)", &fx.game, &reader).unwrap();
        assert_eq!(count.get(), 2);
        // A patch changes the archive: read anew.
        fx.archive(b"a_name=A longer file\n");
        let third = read_with(&cache, "german_(germany)", &fx.game, &reader).unwrap();
        assert_eq!(count.get(), 3);
        assert_eq!(*third, b"a_name=Hallo\n".to_vec());
        // A failure is not cached.
        assert!(read_with(&cache, "french_(france)", &fx.game, &reader).is_err());
        assert!(read_with(&cache, "french_(france)", &fx.game, &reader).is_err());
        assert_eq!(count.get(), 5);
    }

    // -------------------------------------------------------- the gaps

    const ORIGINAL_INI: &[u8] =
        b"\xef\xbb\xbfalt_a=Old A\r\nalt_b,P=Old B\r\nneu_c=New C\r\nNeu_D=New D\r\nalt_e=Old E\r\n";
    const TRANSLATED_INI: &[u8] = b"\xef\xbb\xbfalt_a=Alt A\r\nALT_B=Alt B\r\nalt_e=Alt E\r\n";

    #[test]
    fn missing_lines_compare_keys_loosely() {
        assert_eq!(missing_lines(ORIGINAL_INI, TRANSLATED_INI), vec![b"neu_c=New C".to_vec(), b"Neu_D=New D".to_vec()]);
        assert!(missing_lines(ORIGINAL_INI, ORIGINAL_INI).is_empty());
        // A line of the original that is its own key twice counts once; comments and lines
        // without a key never count; the byte order mark of the first line goes.
        let lines = missing_lines(b"\xef\xbb\xbf; remark\nx=1\n# remark\nX,P=2\nno equals\n =3\n", b"");
        assert_eq!(lines, vec![b"x=1".to_vec()]);
        // Only \n, \r and \r\n end a line in bytes.
        assert_eq!(missing_lines(b"a=1\rb=2\x0bc=3\n", b""), vec![b"a=1".to_vec(), b"b=2\x0bc=3".to_vec()]);
    }

    #[test]
    fn gaps_are_filled_byte_exact_and_once() {
        let fx = Fixture::new("gaps");
        let target = format!("{}/global.ini", fx.root);
        fx.put(&target, TRANSLATED_INI);
        let count = Cell::new(0);
        let counting = |language: &str, folder: &str| {
            count.set(count.get() + 1);
            read_from_archive(language, folder)
        };

        // Without an archive nothing is read and nothing changes.
        assert_eq!(fill_gaps_with(&fx.paths, &target, Some(&fx.game), &counting).unwrap(), 0);
        assert_eq!(count.get(), 0);
        assert_eq!(fs::read(&target).unwrap(), TRANSLATED_INI);

        fx.archive(ORIGINAL_INI);
        assert_eq!(fill_gaps_with(&fx.paths, &target, Some(&fx.game), &counting).unwrap(), 2);
        let mut expected = TRANSLATED_INI.to_vec();
        expected.extend_from_slice(b"neu_c=New C\r\nNeu_D=New D\r\n");
        assert_eq!(fs::read(&target).unwrap(), expected);
        // A copy of the old file lies next to it.
        let backups = fx.backups(&target);
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(format!("{}/{}", fx.root, backups[0])).unwrap(), TRANSLATED_INI);

        // The second look finds nothing changed and does not read the archive.
        assert_eq!(count.get(), 1);
        assert_eq!(fill_gaps_with(&fx.paths, &target, Some(&fx.game), &counting).unwrap(), 0);
        assert_eq!(count.get(), 1);
        assert_eq!(fs::read(&target).unwrap(), expected);
        assert_eq!(fx.backups(&target).len(), 1);

        // A new version of the translation: the gaps are looked at again.
        let mut newer = TRANSLATED_INI.to_vec();
        newer.extend_from_slice(b"neu_c=Neu C\r\n");
        fx.put(&target, &newer);
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 1);
        let text = fs::read(&target).unwrap();
        assert!(text.ends_with(b"neu_c=Neu C\r\nNeu_D=New D\r\n"));
        assert!(!text.windows(5).any(|w| w == b"New C"));
    }

    #[test]
    fn gaps_follow_the_line_ending_and_a_missing_last_one() {
        let fx = Fixture::new("gaps-newline");
        fx.archive(b"a=1\nb=2\nc=3\n");
        let target = format!("{}/global.ini", fx.root);
        fx.put(&target, b"a=1\r\nb=2");
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 1);
        assert_eq!(fs::read(&target).unwrap(), b"a=1\r\nb=2\r\nc=3\r\n");
        fx.put(&target, b"a=1");
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 2);
        assert_eq!(fs::read(&target).unwrap(), b"a=1\nb=2\nc=3\n");
        // An empty file gets the lines as they are, without a leading line end.
        fx.put(&target, b"");
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 3);
        assert_eq!(fs::read(&target).unwrap(), b"a=1\nb=2\nc=3\n");
    }

    #[test]
    fn a_full_file_gets_no_copy_and_no_rewrite() {
        let fx = Fixture::new("gaps-full");
        fx.archive(b"a=1\nb=2\n");
        let target = format!("{}/global.ini", fx.root);
        fx.put(&target, b"b=two\na=one\n");
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 0);
        assert_eq!(fs::read(&target).unwrap(), b"b=two\na=one\n");
        assert!(fx.backups(&target).is_empty());
        assert!(fx.paths.setting(GAPS_STAMP).is_some());
        // A missing target and an unreadable folder never fail.
        assert_eq!(fill_gaps(&fx.paths, &format!("{}/missing.ini", fx.root), Some(&fx.game)), 0);
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&format!("{}/none", fx.root))), 0);
        assert_eq!(fill_gaps(&fx.paths, "", Some(&fx.game)), 0);
    }

    // --------------------------------------------------- the copies

    #[test]
    fn only_the_newest_three_copies_stay() {
        let fx = Fixture::new("backups");
        fx.archive(b"a=1\nb=2\n");
        let target = format!("{}/global.ini", fx.root);
        fx.put(&target, b"a=1\n");
        for stamp in ["20200101-000001", "20200101-000002", "20200101-000003", "20200101-000004"] {
            fx.put(&format!("{target}.scbpw-{stamp}"), stamp.as_bytes());
        }
        // Files that do not follow the pattern are not copies.
        fx.put(&format!("{target}.scbpw-x"), b"x");
        fx.put(&format!("{target}.bak"), b"x");
        fx.put(&format!("{target}.scbpw-20200101-00000"), b"x");
        fx.put(&format!("{}/other.ini.scbpw-20200101-000001", fx.root), b"x");

        // The copies of the older version count with the new one: the
        // newest three by their time stay, whatever their mark.
        assert_eq!(fill_gaps(&fx.paths, &target, Some(&fx.game)), 1);
        let backups = translation::backups_of(&target);
        assert_eq!(backups.len(), 3, "{backups:?}");
        assert!(backups[0].ends_with(".scbpw-20200101-000003"));
        assert!(backups[1].ends_with(".scbpw-20200101-000004"));
        // The fresh one has the new mark and holds the state before the change.
        assert!(file_name_of(&backups[2]).starts_with("global.ini.versekit-"), "{backups:?}");
        assert_eq!(fs::read(&backups[2]).unwrap(), b"a=1\n");
        assert_eq!(fx.backups(&target).len(), 1, "no copy with the old mark is made");
        for kept in ["scbpw-x", "bak", "scbpw-20200101-00000"] {
            assert!(Path::new(&format!("{target}.{kept}")).exists(), "{kept}");
        }
        assert!(Path::new(&format!("{}/other.ini.scbpw-20200101-000001", fx.root)).exists());
    }

    // ------------------------------------------------ the original

    /// A file the tool did not place (another translation, an older English one) goes too: the original comes from
    /// the archive whole, the old file stays as a copy beside it, and the language is entered.
    #[test]
    fn a_foreign_file_makes_way_for_the_original_and_stays_as_a_copy() {
        let fx = Fixture::new("fetch-foreign");
        fx.archive(b"original=Original text\n");
        let target = fx.english();
        fx.put(&target, b"Foreign version <EM4>[BP]</EM4>\nold_only=A line the archive no longer has\n");
        let entered = Cell::new(0);
        let enter = |language: &str| {
            assert_eq!(language, ENGLISH);
            entered.set(entered.get() + 1);
            true
        };
        let outcome = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), true, &enter).unwrap();
        assert_eq!(outcome, FetchOutcome::Written { bytes: b"original=Original text\n".len() });
        assert_eq!(fs::read(&target).unwrap(), b"original=Original text\n", "nothing of the old file is left");
        assert_eq!(entered.get(), 1);
        let backups = fx.backups(&target);
        assert_eq!(backups.len(), 1);
        let copy = format!("{}/{}", Path::new(&target).parent().unwrap().display(), backups[0]);
        assert_eq!(
            fs::read(copy).unwrap(),
            b"Foreign version <EM4>[BP]</EM4>\nold_only=A line the archive no longer has\n",
            "the old file is kept as a copy"
        );
        assert_eq!(fx.paths.setting(ORIGINAL_STAMP), archive_stamp(&fx.game));
        // Not asked for: the user.cfg stays alone.
        fetch_original(&fx.paths, ENGLISH, Some(&fx.game), false, &enter).unwrap();
        assert_eq!(entered.get(), 1);
    }

    /// The original texts chosen for a copy of a game folder (`VERSEKIT_GAME_COPY`, with a `Data.p4k` of any content
    /// and an English file not placed by the tool) are the English file of the archive of an installed channel
    /// (`VERSEKIT_CHANNEL_FOLDER`, only read), whole; the old file stays as a copy and the `user.cfg` names English.
    #[test]
    #[ignore = "reads the installed game and writes into the copy given"]
    fn the_original_of_the_installed_game_replaces_the_english_file_of_a_copy() {
        let real = std::env::var("VERSEKIT_CHANNEL_FOLDER").expect("VERSEKIT_CHANNEL_FOLDER");
        let copy = std::env::var("VERSEKIT_GAME_COPY").expect("VERSEKIT_GAME_COPY");
        let same = |a: &str, b: &str| fs::canonicalize(a).ok() == fs::canonicalize(b).ok();
        assert!(!same(&real, &copy), "the copy is not the installed game");
        let data = format!("{copy}/../versekit-daten");
        fs::create_dir_all(&data).unwrap();
        let host = Host::new(
            Flavor::native(),
            &[("HOME", copy.as_str()), ("SC_BP_HOME", data.as_str()), ("SC_INSTALL_DIR", copy.as_str())],
            &copy,
        );
        let paths = Paths::new(host);
        let target = format!("{copy}/data/Localization/english/global.ini");
        let before = fs::read(&target).expect("an English file in the copy");
        assert!(placed_by_us(&paths, ENGLISH, None).is_empty(), "the file of the copy is not the tool's");
        let original = read_from_archive(ENGLISH, &real).unwrap();
        println!("archive: {} bytes, file of the copy before: {} bytes", original.len(), before.len());
        let set = |language: &str| versekit_core::translation::set_user_cfg(&paths, &copy, language, None);
        let from_real = |language: &str, _: &str| read_from_archive(language, &real);
        let outcome = fetch_original_with(&paths, ENGLISH, Some(&copy), true, &set, &from_real).unwrap();
        assert_eq!(outcome, FetchOutcome::Written { bytes: original.len() });
        let after = fs::read(&target).unwrap();
        assert!(after == original.as_slice(), "the English file is the one of the archive");
        assert!(versekit_core::translation::looks_like_global_ini(&after));
        let copies: Vec<String> = fs::read_dir(format!("{copy}/data/Localization/english"))
            .unwrap()
            .map(|entry| entry.unwrap().path().to_string_lossy().into_owned())
            .filter(|name| name.contains("global.ini.versekit-"))
            .collect();
        assert!(copies.iter().any(|c| fs::read(c).unwrap() == before), "the old file stays as a copy: {copies:?}");
        let cfg = fs::read_to_string(format!("{copy}/user.cfg")).unwrap();
        assert!(cfg.lines().any(|l| l.replace(' ', "").eq_ignore_ascii_case("g_language=english")), "{cfg}");
    }

    /// The archive as the program hands it to a switch of the source: the original through [`fetch_original`].
    struct FromArchive;

    impl versekit_core::game_texts::Archive for FromArchive {
        fn stamp(&self, _: &Paths) -> Option<String> {
            None
        }
        fn refresh_original(&self, _: &Paths) -> bool {
            false
        }
        fn fill_gaps(&self, _: &Paths, _: &str) -> usize {
            0
        }
        fn fetch_original(
            &self,
            paths: &Paths,
            root: &str,
            set_language: bool,
        ) -> Result<versekit_core::game_texts::Placed, String> {
            let set = |language: &str| versekit_core::translation::set_user_cfg(paths, root, language, None);
            match fetch_original(paths, ENGLISH, Some(root), set_language, &set) {
                Ok(FetchOutcome::Written { bytes }) => {
                    Ok(versekit_core::game_texts::Placed { said: bytes.to_string(), written: true })
                }
                Err(error) => Err(error.to_string()),
            }
        }
        fn refresh_names(&self, _: &Paths) {}
    }

    /// The original chosen over an English file of an earlier source that still carries notes and lines of its
    /// own: the file of the archive comes whole, the notes go in as with any source, and taking them out again
    /// leaves exactly the file of the archive. Outside the English folder nothing changes.
    #[test]
    fn the_original_over_an_earlier_english_file_is_clean_once_the_notes_are_taken_out() {
        use versekit_core::game_texts::{remove, switch_source};
        let fx = Fixture::new("original-clean");
        let original = b"Bounty_title_001=Bounty\nBounty_desc_001=Catch him.\n";
        fx.archive(original);
        let target = fx.english();
        fx.put(&target, b"Bounty_title_001=Old bounty <EM4>[BP]</EM4>\nBounty_desc_001=Old text.\nold_only=Rest\n");
        let german = format!("{}/data/Localization/german_(germany)/global.ini", fx.game);
        fx.put(&german, b"Bounty_title_001=Kopfgeld\n");
        let catalog = r#"{"geholt": "2026-10-22 04:00", "bauplaene": {}, "missionen": {"Bounty_title_001": {"bp": ["Atzkav Sniper Rifle"], "rep": 0, "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}}}"#;
        fs::write(fx.paths.app_file(versekit_core::injection::sources::CATALOG_412), catalog).unwrap();
        let net = versekit_core::net::Fake::default();
        let done = switch_source(&fx.paths, &net, &FromArchive, ORIGINAL, "", &|_| {}).unwrap();
        assert_eq!(done.fetched, original.len().to_string());
        let with_notes = fs::read_to_string(&target).unwrap();
        assert!(with_notes.contains("Bounty <EM4>[BP]</EM4>"), "the notes go in: {with_notes}");
        assert!(!with_notes.contains("Old") && !with_notes.contains("old_only"), "nothing old is left: {with_notes}");
        remove(&fx.paths);
        assert_eq!(fs::read(&target).unwrap(), original, "taken out: the file of the archive, exactly");
        assert_eq!(fs::read(&german).unwrap(), b"Bounty_title_001=Kopfgeld\n", "another language stays alone");
    }

    #[test]
    fn the_original_replaces_an_own_starstrings_file_and_forgets_its_note() {
        let fx = Fixture::new("fetch-own");
        fx.archive(b"original=Original text\n");
        let target = fx.english();
        fx.put(&target, b"Foreign version <EM4>[BP]</EM4>\n");
        // Noted by the tool: StarStrings put the file there.
        fx.note(r#"{"starstrings": {"kennung": "probe"}, "dymerz_de": {"kennung": "x"}}"#);
        assert_eq!(placed_by_us(&fx.paths, ENGLISH, None), vec!["starstrings".to_string()]);
        assert_eq!(placed_by_us(&fx.paths, "german_(germany)", None), vec!["dymerz_de".to_string()]);
        assert!(placed_by_us(&fx.paths, ENGLISH, Some("PTU")).is_empty(), "a side channel has its own notes");

        let outcome = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), false, &no_language).unwrap();
        assert_eq!(outcome, FetchOutcome::Written { bytes: b"original=Original text\n".len() });
        assert_eq!(fs::read(&target).unwrap(), b"original=Original text\n");
        // A copy of the replaced file, the state remembered, the note forgotten.
        let backups = fx.backups(&target);
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(format!("{}/{}", Path::new(&target).parent().unwrap().display(), backups[0])).unwrap(),
            b"Foreign version <EM4>[BP]</EM4>\n"
        );
        assert_eq!(fx.paths.setting(ORIGINAL_STAMP), archive_stamp(&fx.game));
        let note = read_note(&fx.paths);
        assert!(note_entry(&note, "starstrings").is_none());
        assert!(note_entry(&note, "dymerz_de").is_some());
        // The remembered original texts of the old file are discarded.
        assert!(Path::new(&fx.paths.app_file(injection::ORIGTEXT_FILE)).exists());

        // A second "original" takes the file of the archive again, also over a file changed by hand.
        fx.put(&target, b"Hand made\n");
        let again = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), false, &no_language).unwrap();
        assert_eq!(again, FetchOutcome::Written { bytes: b"original=Original text\n".len() });
        assert_eq!(fs::read(&target).unwrap(), b"original=Original text\n");
    }

    #[test]
    fn the_original_in_a_side_channel_leaves_the_main_installation_alone() {
        let fx = Fixture::new("fetch-side");
        let ptu = format!("{}/StarCitizen/PTU", fx.root);
        fs::create_dir_all(&ptu).unwrap();
        ArchiveBuilder::new()
            .entry(TestEntry::stored("Data\\Localization\\english\\global.ini", b"ptu_name=PTU ship\n".to_vec()))
            .write_to(format!("{ptu}/{ARCHIVE_NAME}"))
            .unwrap();
        // The main installation has notes written, with their original texts.
        let main = fx.english();
        fx.put(&main, b"main_name=Main ship\n");
        let texts = vec![("main_name".to_string(), Value::str("Main ship"))];
        injection::save_origtext(&fx.paths, texts.clone(), &main);

        let outcome = fetch_original(&fx.paths, ENGLISH, Some(&ptu), false, &no_language).unwrap();
        assert_eq!(outcome, FetchOutcome::Written { bytes: 18 });
        assert_eq!(fs::read(format!("{ptu}/data/Localization/english/global.ini")).unwrap(), b"ptu_name=PTU ship\n");
        assert_eq!(injection::load_origtext(&fx.paths), texts, "the original texts of the main file stay");
        assert!(!injection::is_fresh(&fx.paths));
        assert_eq!(fx.paths.setting(ORIGINAL_STAMP), None, "the stamp belongs to the main installation");
        assert_eq!(fs::read(&main).unwrap(), b"main_name=Main ship\n");

        // Also a note without texts of the main file is not marked fresh by
        // the side channel: only the main installation places a fresh base.
        injection::save_origtext(&fx.paths, Map::new(), &main);
        fs::remove_file(format!("{ptu}/data/Localization/english/global.ini")).unwrap();
        fetch_original(&fx.paths, ENGLISH, Some(&ptu), false, &no_language).unwrap();
        assert!(!injection::is_fresh(&fx.paths), "the side channel left the note alone");
    }

    /// A folder chosen for the PTU under another name (`PTU-Kopie`) is the
    /// channel PTU: the file the tool placed there by the notes of the PTU is
    /// recognised, replaced by the original texts, and its note forgotten.
    #[test]
    fn a_chosen_channel_folder_keeps_the_notes_of_its_channel() {
        let root =
            std::env::temp_dir().join(format!("versekit-game-text-chosen-{}", versekit_core::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let (game, chosen, data) =
            (format!("{root}/StarCitizen/LIVE"), format!("{root}/Zweite/PTU-Kopie"), format!("{root}/data"));
        for folder in [&game, &chosen, &data] {
            fs::create_dir_all(folder).unwrap();
        }
        fs::write(format!("{game}/Game.log"), "").unwrap();
        ArchiveBuilder::new()
            .entry(TestEntry::stored("Data\\Localization\\english\\global.ini", b"ptu_name=PTU ship\n".to_vec()))
            .write_to(format!("{chosen}/{ARCHIVE_NAME}"))
            .unwrap();
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("HOME", root.as_str()), ("SC_BP_HOME", data.as_str())], &root));
        paths.set_setting("spiel_ordner", Value::str(&game));
        let choice: Map = vec![("PTU".to_string(), Value::str(&chosen))];
        paths.set_setting(versekit_core::paths::SETTING_CHANNEL_FOLDERS, Value::Object(choice));
        // StarStrings was placed in the PTU by the tool.
        let target = format!("{chosen}/data/Localization/english/global.ini");
        fs::create_dir_all(Path::new(&target).parent().unwrap()).unwrap();
        fs::write(&target, b"ss_name=StarStrings ship\n").unwrap();
        let key = versekit_core::translation::note_key("starstrings", Some("PTU"));
        versekit_core::translation::note_set(&paths, &key, Some("v1"), Some(ENGLISH), None);

        let outcome = fetch_original(&paths, ENGLISH, Some(&chosen), false, &no_language).unwrap();
        assert_eq!(outcome, FetchOutcome::Written { bytes: 18 }, "the tool's own file is replaced");
        assert_eq!(fs::read(&target).unwrap(), b"ptu_name=PTU ship\n");
        assert!(versekit_core::translation::installed(&paths, "starstrings", Some("PTU")).is_none(), "note forgotten");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_noted_original_is_renewed_by_the_next_original() {
        let fx = Fixture::new("fetch-original");
        fx.archive(b"new_name=New ship\n");
        let target = fx.english();
        fx.put(&target, b"old_name=Old ship\n");
        fx.note(r#"{"original": {"kennung": "Data.p4k"}}"#);
        let outcome = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), false, &no_language).unwrap();
        assert!(matches!(outcome, FetchOutcome::Written { .. }));
        assert_eq!(fs::read(&target).unwrap(), b"new_name=New ship\n");
        // The note of the original itself stays: the file is the original.
        assert!(installed(&read_note(&fx.paths), ORIGINAL));
    }

    #[test]
    fn a_missing_file_is_written_without_a_copy_and_errors_are_named() {
        let fx = Fixture::new("fetch-new");
        let enter = |_: &str| true;
        let no_archive = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), true, &enter);
        assert_eq!(no_archive, Err(FetchError::Read(ReadError::NoArchive)));
        fx.archive(b"a_name=A\n");
        let unknown = fetch_original(&fx.paths, "klingon", Some(&fx.game), true, &enter);
        assert_eq!(unknown, Err(FetchError::UnknownTarget));
        let absent = fetch_original(&fx.paths, "french_(france)", Some(&fx.game), true, &enter);
        assert_eq!(absent, Err(FetchError::Read(ReadError::NoIniInArchive)));
        assert!(!Path::new(&format!("{}/data/Localization/french_(france)", fx.game)).exists());

        let target = fx.english();
        let written = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), true, &enter).unwrap();
        assert_eq!(written, FetchOutcome::Written { bytes: 9 });
        assert_eq!(fs::read(&target).unwrap(), b"a_name=A\n");
        assert!(fx.backups(&target).is_empty());
        // Another language does not remember the state of the English original.
        let state = fx.paths.setting(ORIGINAL_STAMP);
        fx.archive(b"a_name=A longer file\n");
        let german = fetch_original(&fx.paths, "german_(germany)", Some(&fx.game), false, &enter).unwrap();
        assert_eq!(german, FetchOutcome::Written { bytes: 13 });
        assert_eq!(fx.paths.setting(ORIGINAL_STAMP), state);
        // An existing "Data" folder is kept.
        let upper = Fixture::new("fetch-upper");
        upper.archive(b"a_name=A\n");
        fs::create_dir_all(format!("{}/Data", upper.game)).unwrap();
        fetch_original(&upper.paths, ENGLISH, Some(&upper.game), false, &enter).unwrap();
        assert!(Path::new(&format!("{}/Data/Localization/english/global.ini", upper.game)).exists());
    }

    #[test]
    fn a_failing_entry_of_the_language_does_not_discard_the_result() {
        let fx = Fixture::new("fetch-cfg");
        fx.archive(b"a_name=A\n");
        let outcome = fetch_original(&fx.paths, ENGLISH, Some(&fx.game), true, &|_| false);
        assert_eq!(outcome, Ok(FetchOutcome::Written { bytes: 9 }));
        assert!(Path::new(&fx.english()).exists());
    }

    #[test]
    fn the_original_follows_the_game_patch() {
        let fx = Fixture::new("refresh");
        fx.archive(b"vehicle_NameNEU=New ship\n");
        let target = fx.english();
        let refresh = || refresh_original(&fx.paths, Some(&fx.game));

        // a) Without a note of the original the file is not ours: hands off.
        fx.put(&target, b"vehicle_NameALT=Old ship\n");
        assert!(!refresh());
        assert!(fs::read_to_string(&target).unwrap().starts_with("vehicle_NameALT"));

        // b) With the note and a new archive state: fresh from the archive, after a copy.
        fx.note(r#"{"original": {"kennung": "Data.p4k"}}"#);
        assert!(refresh());
        assert_eq!(fs::read(&target).unwrap(), b"vehicle_NameNEU=New ship\n");
        assert_eq!(fx.paths.setting(ORIGINAL_STAMP), archive_stamp(&fx.game));
        assert_eq!(fx.backups(&target).len(), 1);

        // c) The same state: nothing is written again.
        fx.put(&target, b"changed by hand\n");
        assert!(!refresh());
        assert_eq!(fs::read(&target).unwrap(), b"changed by hand\n");

        // d) The next patch arrives as well.
        fx.archive(b"vehicle_NameNEU=New ship\nvehicle_NameNOCHNEUER=Even newer\n");
        assert!(refresh());
        assert!(fs::read_to_string(&target).unwrap().contains("NOCHNEUER"));

        // e) If another own source holds the folder, its file stays.
        fx.archive(b"vehicle_NameDRITTER=Third\n");
        fx.note(r#"{"original": {"kennung": "Data.p4k"}, "starstrings": {"kennung": "probe"}}"#);
        fx.put(&target, b"StarStrings <EM4>[BP]</EM4>\n");
        assert!(!refresh());
        assert_eq!(fs::read(&target).unwrap(), b"StarStrings <EM4>[BP]</EM4>\n");

        // f) A known state is the trigger: with it set, nothing is written.
        fx.note(r#"{"original": {"kennung": "Data.p4k"}}"#);
        fx.paths.set_setting(ORIGINAL_STAMP, Value::str(archive_stamp(&fx.game).unwrap()));
        assert!(!refresh());
        assert_eq!(fs::read(&target).unwrap(), b"StarStrings <EM4>[BP]</EM4>\n");
    }

    #[test]
    fn the_language_folder_of_a_source_follows_the_note() {
        let fx = Fixture::new("folders");
        let note = Map::new();
        assert_eq!(language_folder(&fx.paths, &note, ORIGINAL).as_deref(), Some(ENGLISH));
        assert_eq!(language_folder(&fx.paths, &note, "dymerz_fr").as_deref(), Some("french_(france)"));
        assert_eq!(language_folder(&fx.paths, &note, game_ini::CUSTOM).as_deref(), Some(ENGLISH));
        let note = jsonio::loads(r#"{"eigene": {"kennung": "x", "sprache": "german_(germany)"}}"#)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(language_folder(&fx.paths, &note, game_ini::CUSTOM).as_deref(), Some("german_(germany)"));
        assert_eq!(placed_by_us(&fx.paths, ENGLISH, None), Vec::<String>::new());
    }
}
