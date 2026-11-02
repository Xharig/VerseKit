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

//! Recognize the blueprint notification in the Game.log, in every game language.
//!
//! On unlocking, Star Citizen writes a line like
//! `<SHUDEvent_OnNotification> Added notification "Bauplan erhalten: Attrition-5 Repeater: "`.
//! The text before the name is translated. All known phrases are searched
//! for at once, from three sources in this order of precedence:
//! the user's own additions (`phrasen.json`), the unpacked `global.ini` files
//! of the installation, the bundled table.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;

use crate::jsonio::{Map, Value, map_set};
use crate::logscan::split_names;
use crate::paths::Paths;
use crate::text::{PY_SPACE, decode_ignore, lines_universal, py_strip};

/// The language-neutral key of the notification in the `global.ini`.
pub const INI_KEY: &str = "crafting_hud_notification_received_blueprint";

/// Bundled phrases; used when no `global.ini` is readable.
pub const TABLE: [(&str, &[&str]); 2] = [
    ("de", &["Bauplan erhalten"]),
    (
        "en",
        &["Received Blueprint", "Blueprint Received", "Blueprint Acquired", "Blueprint Obtained", "Blueprint Unlocked"],
    ),
];

/// Where the most specific phrase comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Own,
    Ini,
    Table,
}

/// Where the phrases come from: the user's own `phrasen.json` and the
/// language folder of the installation (`…/data/Localization`).
///
/// ⚠ `phrases_file` is the full path as in `Paths::app_file("phrasen.json")`:
/// normally the file lives in the `Einstellungen` subfolder; only with a
/// custom data folder set via environment variable is it flat in the app folder.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub phrases_file: Option<PathBuf>,
    pub localization_folder: Option<PathBuf>,
}

impl Sources {
    /// All unpacked `global.ini` files, sorted by language folder.
    pub fn ini_files(&self) -> Vec<PathBuf> {
        let Some(folder) = &self.localization_folder else { return Vec::new() };
        let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
        let mut names: Vec<String> =
            entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names.into_iter().map(|language| folder.join(language).join("global.ini")).filter(|p| p.is_file()).collect()
    }

    /// User-added phrases from `phrasen.json` (`{"phrasen": [...]}`).
    pub fn own(&self) -> Vec<String> {
        let Some(file) = &self.phrases_file else { return Vec::new() };
        let Ok(bytes) = fs::read(file) else { return Vec::new() };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return Vec::new() };
        let Some(list) = value.get("phrasen").and_then(|v| v.as_array()) else { return Vec::new() };
        // Non-text entries like Python's `str()`.
        list.iter()
            .map(|v| match v {
                serde_json::Value::String(s) => py_strip(s).to_string(),
                serde_json::Value::Bool(true) => "True".to_string(),
                serde_json::Value::Bool(false) => "False".to_string(),
                serde_json::Value::Null => "None".to_string(),
                other => py_strip(&other.to_string()).to_string(),
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// All phrases, along with the origin of the most specific one.
    pub fn collect(&self) -> (Vec<String>, Origin) {
        let mut list: Vec<String> = Vec::new();
        let mut origin = Origin::Table;
        for p in self.own() {
            if !list.contains(&p) {
                list.push(p);
                origin = Origin::Own;
            }
        }
        for file in self.ini_files() {
            if let Some(p) = phrase_from_ini(&file)
                && !list.contains(&p)
            {
                list.push(p);
                origin = Origin::Ini;
            }
        }
        for (_, phrases) in TABLE {
            for p in phrases {
                if !list.iter().any(|x| x == p) {
                    list.push((*p).to_string());
                }
            }
        }
        (list, origin)
    }

    /// Whether the phrase is certain (from `phrasen.json` or a `global.ini`) rather than guessed from the table.
    pub fn confirmed(&self) -> bool {
        matches!(self.collect().1, Origin::Own | Origin::Ini)
    }

    /// Keeps a phrase found in `phrasen.json`, the same file that is edited by hand. False when it is empty,
    /// already there or the file cannot be written.
    ///
    /// ⚠ The file is written anew from what [`own`](Self::own) reads: other keys go, entries that are not text
    /// stay as their text. Written directly, not through a temporary file, in text mode (`\r\n` on Windows).
    pub fn remember(&self, phrase: &str) -> bool {
        if phrase.is_empty() {
            return false;
        }
        let existing = self.own();
        if existing.iter().any(|p| p == phrase) {
            return false;
        }
        let Some(file) = &self.phrases_file else { return false };
        let mut list: Vec<Value> = existing.into_iter().map(Value::Str).collect();
        list.push(Value::str(phrase));
        let data =
            Value::Object(vec![("phrasen".into(), Value::Array(list)), ("_hinweis".into(), Value::str(REMEMBER_HINT))]);
        crate::jsonio::write_new(&file.to_string_lossy(), &crate::jsonio::file_bytes(&data, 1)).is_ok()
    }

    /// Works out the phrase of this client from the kept logs ([`find_self`]) and keeps it ([`remember`](Self::remember)),
    /// unless the phrase is [`confirmed`](Self::confirmed) already or no blueprint name is known. Gives the phrase
    /// back when it was newly kept.
    pub fn learn(&self, catalog_names: &[String], backups: &[String]) -> Option<String> {
        if self.confirmed() || catalog_names.is_empty() {
            return None;
        }
        let found = find_self(catalog_names, backups, FIND_AT_MOST)?;
        (!found.is_empty() && self.remember(&found)).then_some(found)
    }

    /// Turn an unresolved text key (`@Item_Name`) into a readable name.
    ///
    /// Anything else is returned unchanged. All unpacked `global.ini` files are
    /// searched; if nothing is found, the key is made readable.
    ///
    /// ⚠ The Python version also queries the original names from `Data.p4k`
    /// before making the key readable. That is missing here until the p4k reader exists.
    pub fn resolve_key(&self, name: &str) -> String {
        if name.is_empty() {
            return name.to_string();
        }
        let Some(key) = unresolved_key(py_strip(name)) else { return name.to_string() };
        let mut cache = KEY_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = cache.get(key) {
            return hit.clone();
        }
        let resolved = self.lookup_key(key).unwrap_or_else(|| readable(key));
        cache.insert(key.to_string(), resolved.clone());
        resolved
    }

    fn lookup_key(&self, key: &str) -> Option<String> {
        let needle = format!("{}=", key.to_lowercase());
        let short = format!("{},p=", key.to_lowercase());
        for path in self.ini_files() {
            let Ok(bytes) = fs::read(&path) else { continue };
            let text = decode_ignore(strip_bom(&bytes));
            for line in lines_universal(&text) {
                let head: String = line.chars().take(key.chars().count() + 3).collect::<String>().to_lowercase();
                if head.starts_with(&needle) || head.starts_with(&short) {
                    let value = line.split_once('=').map(|(_, v)| v).unwrap_or("");
                    let value = py_strip(py_strip(value).trim_start_matches('*'));
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }
        None
    }
}

/// The note [`Sources::remember`] writes beside the phrases.
const REMEMBER_HINT: &str =
    "Formulierungen, an denen ein neuer Bauplan im Spiel-Log erkannt wird. Selbst gefundene stehen hier mit drin.";

/// How many of the newest kept logs [`find_self`] reads when the phrase is learned.
pub const FIND_AT_MOST: usize = 40;

/// The notification with any label in front of the name: label and name.
static FRAME_OPEN: LazyLock<Regex> = LazyLock::new(|| {
    let s = PY_SPACE;
    Regex::new(&format!(r#"Added notification "([^":]{{3,60}}):{s}*(.+?){s}*:{s}*""#)).expect("fester Ausdruck")
});

/// The class suffix behind a name (`(Mil/1/A)`), case-insensitive.
///
/// ⚠ Without regard to case Python also takes `ı` and `İ` for `i`; Rust's case folding does not, so they are named.
static CLASS_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    let s = PY_SPACE;
    Regex::new(&format!(r"(?i){s}*\((?:C[iıİ]v|M[iıİ]l|[iıİ]nd|Sth|Cmp)/\d+/[A-D]\){s}*$")).expect("fester Ausdruck")
});

/// The blueprint phrase worked out from the own logs, in any language, or None.
///
/// A notification whose name after the label is a known blueprint makes the label a candidate. Only the newest
/// `at_most` files are read (`0` reads all of them, as the slice `[-0:]` does).
///
/// ⚠ At least two different known names must stand behind the same label: a single one may be chance, a
/// blueprint name appears in other notifications too. The class suffix is cut off first, or no name fits the
/// catalog. Of several labels the one with the most names wins; with as many, the one that sorts last.
pub fn find_self(catalog_names: &[String], backups: &[String], at_most: usize) -> Option<String> {
    if catalog_names.is_empty() || backups.is_empty() {
        return None;
    }
    let known: HashSet<String> = catalog_names.iter().map(|n| py_strip(&n.to_lowercase()).to_string()).collect();
    let mut counter: HashMap<String, HashSet<String>> = HashMap::new();
    let newest = if at_most == 0 { backups } else { &backups[backups.len().saturating_sub(at_most)..] };
    for file in newest {
        let Ok(bytes) = fs::read(file) else { continue };
        let text = decode_ignore(&bytes);
        for caps in FRAME_OPEN.captures_iter(&text) {
            let phrase = py_strip(&caps[1]);
            let name = py_strip(&caps[2]);
            let name = CLASS_SUFFIX.replace_all(name, "");
            let name = py_strip(&name).to_lowercase();
            if known.contains(&name) {
                counter.entry(phrase.to_string()).or_default().insert(name);
            }
        }
    }
    counter.into_iter().filter(|(_, names)| names.len() >= 2).map(|(p, names)| (names.len(), p)).max().map(|(_, p)| p)
}

/// Resolved keys: a lookup costs a pass over the files, so it is done only once.
static KEY_CACHE: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// `@Schluessel_Name` → `Schluessel_Name`; otherwise None.
fn unresolved_key(name: &str) -> Option<&str> {
    let key = name.strip_prefix('@')?;
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(key)
}

/// Build a readable name from a key, the last resort.
fn readable(key: &str) -> String {
    let mut text = key;
    for tail in ["_Name", "_name", "_Title", "_title"] {
        if let Some(stripped) = text.strip_suffix(tail) {
            text = stripped;
            break;
        }
    }
    let out = py_strip(&text.replace('_', " ")).to_string();
    if out.is_empty() { key.to_string() } else { out }
}

/// The phrase from a `global.ini`, or None. Cached per file state.
pub fn phrase_from_ini(path: &Path) -> Option<String> {
    let mark = fs::metadata(path).ok().map(|m| (path.to_path_buf(), m.len(), m.modified().ok()));
    let mut cache = INI_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mark) = &mark
        && let Some(hit) = cache.get(mark)
    {
        return hit.clone();
    }
    let result = read_ini_phrase(path);
    if let Some(mark) = mark {
        cache.insert(mark, result.clone());
    }
    result
}

type IniMark = (PathBuf, u64, Option<std::time::SystemTime>);
static INI_CACHE: std::sync::LazyLock<Mutex<HashMap<IniMark, Option<String>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn read_ini_phrase(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = decode_ignore(strip_bom(&bytes));
    for line in lines_universal(&text) {
        if !line.starts_with(INI_KEY) {
            continue;
        }
        let value = line.split_once('=').map(|(_, v)| py_strip(v)).unwrap_or("");
        let (before, after) = value.split_once("%s").unwrap_or((value, ""));
        // If there is text after the name, the whole phrase applies.
        if !py_strip(py_strip(after).trim_matches(':')).is_empty() {
            let whole = py_strip(value);
            return (!whole.is_empty()).then(|| whole.to_string());
        }
        let front = py_strip(py_strip(before).trim_end_matches(':'));
        return (!front.is_empty()).then(|| front.to_string());
    }
    None
}

/// Split a phrase into the text before and after the blueprint name.
pub fn split_phrase(phrase: &str) -> (String, String) {
    match phrase.split_once("%s") {
        None => (py_strip(py_strip(phrase).trim_end_matches(':')).to_string(), String::new()),
        Some((before, after)) => (
            py_strip(py_strip(before).trim_end_matches(':')).to_string(),
            py_strip(py_strip(after).trim_matches(':')).to_string(),
        ),
    }
}

/// The search expression for the log lines. It can have several capture
/// groups: labels before the name share one, each rearranged phrase gets
/// its own.
pub fn pattern(phrases: &[String]) -> Regex {
    let mut front = Vec::new();
    let mut tail = Vec::new();
    for p in phrases {
        let (v, n) = split_phrase(p);
        if !n.is_empty() {
            tail.push((v, n));
        } else if !v.is_empty() {
            front.push(v);
        }
    }
    let mut parts = Vec::new();
    if !front.is_empty() {
        let alts: Vec<String> = front.iter().map(|v| regex::escape(v)).collect();
        let s = PY_SPACE;
        parts.push(format!(r#"Added notification "(?:{}):{s}*(.+?){s}*:{s}*""#, alts.join("|")));
    }
    for (v, n) in &tail {
        let s = PY_SPACE;
        let head = if v.is_empty() { format!("{s}*") } else { format!("{}:?{s}*", regex::escape(v)) };
        parts.push(format!(r#"Added notification "{head}(.+?){s}+{}{s}*:{s}*""#, regex::escape(n)));
    }
    if parts.is_empty() {
        // An expression that never matches, rather than one that matches every notification.
        return Regex::new(r"[^\s\S]").expect("fester Ausdruck");
    }
    Regex::new(&parts.join("|")).expect("aus maskierten Teilen gebaut")
}

// ---------------------------------------------------- Translated items
//
// A translation can also translate item names; the blueprint notification
// then contains `Helix II Bergbaulaser`, and the English catalog does not
// know that name. The way back goes through the text key: translated
// text → key in the translated `global.ini` → English text of the same
// key. The result is stored in `BACK_FILE`, tied to the state of the
// translated files.

/// The stored reverse lookups (`{"stand": …, "namen": {lowercase: [english]}}`).
pub const BACK_FILE: &str = "namen-rueckweg.json";

/// The English `global.ini` from the `Data.p4k`.
///
/// ⚠ Gap: The p4k reader has not been ported yet. With `NoArchive`, only the
/// unpacked `global.ini` files count.
pub trait ArchiveText {
    fn english_global_ini(&self) -> Option<Vec<u8>>;
}

/// No archive (stands in until the p4k reader exists).
pub struct NoArchive;

impl ArchiveText for NoArchive {
    fn english_global_ini(&self) -> Option<Vec<u8>> {
        None
    }
}

/// The English original names for a translated item name.
pub trait EnglishNames {
    /// Empty if no translated `global.ini` contains the name; several
    /// entries if the text appears under several keys.
    fn english_names(&self, name: &str) -> Vec<String>;
}

/// The reverse lookup via the installation's `global.ini`.
pub struct BackRoute<'a> {
    pub paths: &'a Paths,
    pub archive: &'a dyn ArchiveText,
}

/// Caches as in the Python version: stored reverse lookups (per
/// state), the index "translated text → key" (per state) and the parsed
/// English texts (per file, the archive once per run).
#[derive(Default)]
struct BackState {
    store: Option<(String, Map)>,
    index: Option<(String, HashMap<String, BTreeSet<String>>)>,
    texts: HashMap<String, Arc<HashMap<String, String>>>,
}

static BACK: LazyLock<Mutex<BackState>> = LazyLock::new(|| Mutex::new(BackState::default()));

/// Clear all caches of the reverse lookup (for test runs).
pub fn clear_back_caches() {
    *BACK.lock().unwrap_or_else(|e| e.into_inner()) = BackState::default();
}

/// Like Python's `os.path.realpath`: links resolved, on Windows
/// without the `\\?\` prefix.
fn realpath(path: &str) -> String {
    let Ok(full) = fs::canonicalize(path) else { return path.to_string() };
    let text = full.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").map_or(text.clone(), String::from)
}

/// How to tell that a `global.ini` has changed: path, size,
/// modification time in nanoseconds, without reading it.
fn ini_mark(path: &str) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    let ns = crate::paths::nanos(meta.modified().ok()?);
    Some(format!("{}:{}:{}", realpath(path), meta.len(), ns))
}

/// Does the key belong to an item or vehicle name?
fn item_key(key: &str) -> bool {
    let low = key.to_lowercase();
    low.starts_with("item_") || low.contains("name")
}

/// Text without the foreign marker (`*`) and without appended details, lowercased.
fn plain(text: &str) -> String {
    let text = py_strip(py_strip(text).trim_start_matches('*'));
    py_strip(&split_names(text).0).to_lowercase()
}

/// Like Python's `str.splitlines()`: also splits at `\v`, `\f`, `\x1c` to `\x1e`,
/// `\x85`, U+2028 and U+2029.
fn py_splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = match c {
            '\r' => {
                if let Some((_, '\n')) = chars.peek() {
                    chars.next();
                }
                true
            }
            '\n' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}' => true,
            _ => false,
        };
        if end {
            out.push(&text[start..i]);
            start = chars.peek().map_or(text.len(), |(j, _)| *j);
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// `{lowercase key: text}` of the item names from a `global.ini`;
/// for a duplicate key the first one wins.
fn item_texts(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in py_splitlines(text) {
        let Some(sep) = line.find('=').filter(|&s| s >= 1) else { continue };
        let key = line[..sep].split(',').next().unwrap_or("");
        if item_key(key) {
            out.entry(key.to_lowercase())
                .or_insert_with(|| py_strip(py_strip(&line[sep + 1..]).trim_start_matches('*')).to_string());
        }
    }
    out
}

/// The names of a stored list, like `list(value)` in Python.
fn value_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                other => crate::jsonio::dumps(other, None, false),
            })
            .collect(),
        Value::Str(s) => s.chars().map(String::from).collect(),
        _ => Vec::new(),
    }
}

impl BackRoute<'_> {
    /// The unpacked `global.ini` files (path, language folder), sorted by language folder.
    fn ini_files(&self) -> Vec<(String, String)> {
        let Some(folder) = self.paths.localization_folder(None) else { return Vec::new() };
        let Ok(entries) = fs::read_dir(&folder) else { return Vec::new() };
        let mut names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        names.sort();
        let flavor = self.paths.host.flavor;
        names
            .into_iter()
            .map(|language| (flavor.join_all(&folder, &[&language, "global.ini"]), language))
            .filter(|(p, _)| crate::paths::is_file(p))
            .collect()
    }

    /// The unpacked `global.ini` files other than the English one.
    fn translated_files(&self) -> Vec<String> {
        self.ini_files()
            .into_iter()
            .filter(|(_, language)| language.to_lowercase() != "english")
            .map(|(p, _)| p)
            .collect()
    }

    /// Load the stored reverse lookups for state `mark` into the cache.
    fn back_store(&self, state: &mut BackState, mark: &str) {
        if state.store.as_ref().is_some_and(|(stand, _)| stand == mark) {
            return;
        }
        let mut names = Map::new();
        if let Ok(Value::Object(data)) = crate::jsonio::read_json(&self.paths.app_file(BACK_FILE))
            && data.iter().any(|(k, v)| k == "stand" && v.as_str() == Some(mark))
            && let Some((_, Value::Object(found))) = data.iter().find(|(k, _)| k == "namen")
        {
            names = found.clone();
        }
        state.store = Some((mark.to_string(), names));
    }

    /// Store the reverse lookups; a failure is reported, not thrown.
    fn back_save(&self, mark: &str, names: &Map) {
        let target = self.paths.app_file(BACK_FILE);
        let temp = format!("{target}.tmp");
        let data =
            Value::Object(vec![("stand".into(), Value::str(mark)), ("namen".into(), Value::Object(names.clone()))]);
        let result = crate::jsonio::write_new(&temp, crate::jsonio::dumps(&data, None, false).as_bytes())
            .and_then(|()| crate::jsonio::os_replace(&temp, &target));
        if let Err(e) = result {
            crate::errors::record("phrases.back_save", &e);
        }
    }

    /// The keys whose translated text is `name`. The index is built
    /// once per state in a single pass over all files.
    fn keys_for(state: &mut BackState, name: &str, files: &[String], mark: &str) -> BTreeSet<String> {
        if state.index.as_ref().is_none_or(|(stand, _)| stand != mark) {
            state.index = None;
            let mut index: HashMap<String, BTreeSet<String>> = HashMap::new();
            for path in files {
                let Ok(bytes) = fs::read(path) else { continue };
                let text = decode_ignore(strip_bom(&bytes));
                for line in lines_universal(&text) {
                    let Some(sep) = line.find('=').filter(|&s| s >= 1) else { continue };
                    let key = line[..sep].split(',').next().unwrap_or("");
                    if !item_key(key) {
                        continue;
                    }
                    index.entry(plain(&line[sep + 1..])).or_default().insert(key.to_lowercase());
                }
            }
            state.index = Some((mark.to_string(), index));
        }
        let wanted = py_strip(name).to_lowercase();
        state.index.as_ref().and_then(|(_, index)| index.get(&wanted).cloned()).unwrap_or_default()
    }

    /// `{key: English text}` for the requested keys: first the
    /// unpacked English `global.ini`, the archive for the rest.
    fn english_texts(&self, state: &mut BackState, keys: &BTreeSet<String>) -> HashMap<String, String> {
        let mut found: HashMap<String, String> = HashMap::new();
        let english = self
            .paths
            .localization_folder(None)
            .map(|folder| self.paths.host.flavor.join_all(&folder, &["english", "global.ini"]))
            .unwrap_or_default();
        let mut sources = Vec::new();
        if crate::paths::is_file(&english) {
            let mark = format!("ini:{}", ini_mark(&english).unwrap_or_else(|| "None".into()));
            let texts = state
                .texts
                .entry(mark)
                .or_insert_with(|| {
                    Arc::new(fs::read(&english).map(|b| item_texts(&decode_ignore(strip_bom(&b)))).unwrap_or_default())
                })
                .clone();
            sources.push(texts);
        }
        let lookup = |texts: &HashMap<String, String>, found: &mut HashMap<String, String>| {
            for key in keys {
                if let Some(text) = texts.get(key).filter(|t| !t.is_empty())
                    && !found.contains_key(key)
                {
                    found.insert(key.clone(), py_strip(&split_names(text).0).to_string());
                }
            }
        };
        for texts in &sources {
            lookup(texts, &mut found);
        }
        if found.len() == keys.len() {
            return found;
        }
        let archive = state
            .texts
            .entry("daten".into())
            .or_insert_with(|| {
                let data = self.archive.english_global_ini().unwrap_or_default();
                Arc::new(item_texts(&decode_ignore(strip_bom(&data))))
            })
            .clone();
        lookup(&archive, &mut found);
        found
    }
}

impl EnglishNames for BackRoute<'_> {
    fn english_names(&self, name: &str) -> Vec<String> {
        if py_strip(name).is_empty() {
            return Vec::new();
        }
        let files = self.translated_files();
        if files.is_empty() {
            return Vec::new();
        }
        let mark = files.iter().map(|p| ini_mark(p).unwrap_or_else(|| p.clone())).collect::<Vec<_>>().join("|");
        let mut state = BACK.lock().unwrap_or_else(|e| e.into_inner());
        self.back_store(&mut state, &mark);
        let wanted = py_strip(name).to_lowercase();
        if let Some((_, names)) = &state.store
            && let Some((_, value)) = names.iter().find(|(k, _)| *k == wanted)
        {
            return value_list(value);
        }
        let keys = Self::keys_for(&mut state, &wanted, &files, &mark);
        let english: Vec<String> = if keys.is_empty() {
            Vec::new()
        } else {
            let found = self.english_texts(&mut state, &keys);
            found.into_values().collect::<BTreeSet<_>>().into_iter().collect()
        };
        if let Some((_, names)) = &mut state.store {
            map_set(names, &wanted, Value::Array(english.iter().cloned().map(Value::Str).collect()));
            self.back_save(&mark, names);
        }
        english
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_phrase_cases() {
        assert_eq!(split_phrase("Bauplan erhalten: %s"), ("Bauplan erhalten".into(), "".into()));
        assert_eq!(split_phrase("%s ist eingetroffen"), ("".into(), "ist eingetroffen".into()));
        assert_eq!(split_phrase("Bauplan erhalten"), ("Bauplan erhalten".into(), "".into()));
    }

    #[test]
    fn pattern_finds_both_forms() {
        let rx = pattern(&["Bauplan erhalten".into(), "%s ist eingetroffen".into()]);
        let line = r#"<SHUDEvent_OnNotification> Added notification "Bauplan erhalten: Attrition-5 Repeater: " [136]"#;
        let caps = rx.captures(line).unwrap();
        assert_eq!(caps.iter().skip(1).flatten().next().unwrap().as_str(), "Attrition-5 Repeater");
        let line2 = r#"Added notification "Helm X ist eingetroffen: ""#;
        let caps = rx.captures(line2).unwrap();
        assert_eq!(caps.iter().skip(1).flatten().next().unwrap().as_str(), "Helm X");
    }

    #[test]
    fn space_and_strip_like_python() {
        assert_eq!(split_phrase("\u{1f}Bauplan erhalten:\u{1c}"), ("Bauplan erhalten".into(), "".into()));
        let rx = pattern(&["Bauplan erhalten".into()]);
        let line = "Added notification \"Bauplan erhalten:\u{1f}Helm X\u{1e}:\u{1d}\"";
        let caps = rx.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Helm X");
        assert_eq!(readable("_Helm_\u{1f}"), "Helm");
    }

    #[test]
    fn empty_pattern_never_matches() {
        assert!(!pattern(&[]).is_match("Added notification \"x: y: \""));
    }

    #[test]
    fn own_reads_given_file_like_python_str() {
        let dir = std::env::temp_dir().join(format!("versekit-phrasen-{}", crate::testdir::run_tag()));
        fs::create_dir_all(dir.join("Einstellungen")).unwrap();
        let file = dir.join("Einstellungen").join("phrasen.json");
        fs::write(&file, r#"{"phrasen": [" Blueprint Gained ", true, null, ""]}"#).unwrap();
        let sources = Sources { phrases_file: Some(file), localization_folder: None };
        assert_eq!(sources.own(), vec!["Blueprint Gained", "True", "None"]);
        let flat = Sources { phrases_file: Some(dir.join("phrasen.json")), localization_folder: None };
        assert!(flat.own().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn splitlines_like_python() {
        assert_eq!(
            py_splitlines("a\r\nb\rc\nd\u{b}e\u{1c}f\u{85}g\u{2028}h"),
            ["a", "b", "c", "d", "e", "f", "g", "h"]
        );
        assert_eq!(py_splitlines("a\n\nb\n"), ["a", "", "b"]);
        assert!(py_splitlines("").is_empty());
        // `\x1f` does not split lines but is whitespace.
        assert_eq!(py_splitlines("a\u{1f}b"), ["a\u{1f}b"]);
    }

    #[test]
    fn item_texts_first_wins() {
        let texts = item_texts("item_NameA=*  Erst \nitem_namea=Zweit\n=x\nfoo=bar\nVehicle_Name_B,P=Schiff\n");
        assert_eq!(texts.get("item_namea").map(String::as_str), Some("Erst"));
        assert_eq!(texts.get("vehicle_name_b").map(String::as_str), Some("Schiff"));
        assert!(!texts.contains_key("foo"));
        assert_eq!(plain("  *Helm (Mil/1/A) \n"), "helm");
    }

    struct Archive(&'static str);

    impl ArchiveText for Archive {
        fn english_global_ini(&self) -> Option<Vec<u8>> {
            Some(self.0.as_bytes().to_vec())
        }
    }

    #[test]
    fn english_names_back_route() {
        use crate::paths::Host;
        let base = std::env::temp_dir().join(format!("versekit-rueckweg-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&base);
        let game = base.join("LIVE");
        let loc = game.join("data").join("Localization");
        fs::create_dir_all(loc.join("german_(germany)")).unwrap();
        fs::create_dir_all(loc.join("english")).unwrap();
        fs::write(game.join("Game.log"), "").unwrap();
        fs::write(
            loc.join("german_(germany)").join("global.ini"),
            "\u{feff}item_NameHelix=Helix II Bergbaulaser\r\nitem_DescHelix=Helix II Bergbaulaser\r\n\
             item_NameDouble=Doppelt\r\nitem_NameDouble2,P=*Doppelt (Mil/1/A)\r\n",
        )
        .unwrap();
        fs::write(
            loc.join("english").join("global.ini"),
            "item_NameHelix=Helix II Mining Laser\nitem_NameDouble=Parallax A\n",
        )
        .unwrap();
        let home = base.join("home");
        let (game, home) = (game.to_string_lossy(), home.to_string_lossy());
        let vars = [(crate::paths::ENV_INSTALL_DIR, &*game), (crate::legacy::ENV_HOME, &*home)];
        let paths = Paths::new(Host::new(crate::pypath::Flavor::native(), &vars, &home));
        let archive = Archive("item_NameDouble2=Parallax B (Civ/1/A)\u{1c}item_NameHelix=Falsch\n");
        let route = BackRoute { paths: &paths, archive: &archive };
        assert_eq!(route.english_names(" helix ii BERGBAULASER "), ["Helix II Mining Laser"]);
        assert_eq!(route.english_names("Doppelt"), ["Parallax A", "Parallax B"]);
        assert!(route.english_names("Unbekannt").is_empty());
        assert!(route.english_names("  ").is_empty());
        let stored = crate::jsonio::read_json(&paths.app_file(BACK_FILE)).unwrap();
        let namen = stored.get("namen").and_then(Value::as_object).unwrap();
        assert_eq!(
            namen.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["helix ii bergbaulaser", "doppelt", "unbekannt"]
        );
        assert!(stored.get("stand").and_then(Value::as_str).unwrap().contains("global.ini:"));
        // Stored: still applies after the caches are cleared.
        clear_back_caches();
        let empty = BackRoute { paths: &paths, archive: &NoArchive };
        assert_eq!(empty.english_names("Doppelt"), ["Parallax A", "Parallax B"]);
        let _ = fs::remove_dir_all(&base);
    }

    /// A throwaway folder with kept logs, each written from its notification texts.
    fn logs(name: &str, files: &[&[&str]]) -> (PathBuf, Vec<String>) {
        let dir = std::env::temp_dir().join(format!("versekit-selbst-{}-{name}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for (i, texts) in files.iter().enumerate() {
            let body: String = texts
                .iter()
                .enumerate()
                .map(|(n, t)| {
                    format!("<2026-10-30T10:00:0{n}.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{t}: \" [{n}]\n")
                })
                .collect();
            let file = dir.join(format!("log{i:03}.log"));
            fs::write(&file, body).unwrap();
            paths.push(file.to_string_lossy().into_owned());
        }
        (dir, paths)
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    const CATALOG: [&str; 3] = ["Attrition-5 Repeater", "Singe Cannon (S2)", "10-Series Greatsword Cannon"];

    #[test]
    fn an_unknown_language_is_found_from_two_known_names_and_a_single_one_is_no_proof() {
        let (dir, files) = logs(
            "fremd",
            &[&[
                "Plan de construction reçu: Attrition-5 Repeater",
                "Mission terminée: Irgendwas",
                "Plan de construction reçu: Singe Cannon (S2)",
            ]],
        );
        assert_eq!(find_self(&names(&CATALOG), &files, 40).as_deref(), Some("Plan de construction reçu"));
        let (single, one) = logs("einzeln", &[&["Irgendein Text: Attrition-5 Repeater"]]);
        assert_eq!(find_self(&names(&CATALOG), &one, 40), None, "one name behind a label may be chance");
        // The same name twice is still one name.
        let (twice, same) =
            logs("zweimal", &[&["Irgendein Text: Attrition-5 Repeater", "Irgendein Text: Attrition-5 Repeater"]]);
        assert_eq!(find_self(&names(&CATALOG), &same, 40), None);
        assert_eq!(find_self(&[], &files, 40), None);
        assert_eq!(find_self(&names(&CATALOG), &[], 40), None);
        for d in [dir, single, twice] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn the_class_suffix_and_case_are_left_aside_as_python_does() {
        let (dir, files) = logs(
            "zusatz",
            &[&[
                "Bauplan bekommen: ATTRITION-5 repeater (Mil/1/A)",
                "Bauplan bekommen:  Singe Cannon (S2)\u{1f}(cıv/12/d) ",
                "Andere Meldung: 10-Series Greatsword Cannon (Xyz/1/A)",
                "Andere Meldung: Attrition-5 Repeater (Mil/1/E)",
            ]],
        );
        assert_eq!(find_self(&names(&CATALOG), &files, 40).as_deref(), Some("Bauplan bekommen"));
        let (bare, other) = logs("ohne", &[&["Kurz: Attrition-5 Repeater", "Kurz: Singe Cannon (S2) (Sth/1/B)"]]);
        assert_eq!(
            find_self(&names(&[" attrition-5 REPEATER ", "singe cannon (s2)"]), &other, 40).as_deref(),
            Some("Kurz")
        );
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(bare);
    }

    #[test]
    fn the_label_with_most_names_wins_and_a_tie_goes_to_the_one_that_sorts_last() {
        let (dir, files) = logs(
            "gleichstand",
            &[&[
                "Aaa Label: Attrition-5 Repeater",
                "Aaa Label: Singe Cannon (S2)",
                "Zzz Label: Attrition-5 Repeater",
                "Zzz Label: Singe Cannon (S2)",
            ]],
        );
        assert_eq!(find_self(&names(&CATALOG), &files, 40).as_deref(), Some("Zzz Label"));
        let (more, three) = logs(
            "mehr",
            &[
                &["Zzz Label: Attrition-5 Repeater", "Zzz Label: Singe Cannon (S2)"],
                &["Aaa Label: Attrition-5 Repeater", "Aaa Label: Singe Cannon (S2)"],
                &["Aaa Label: 10-Series Greatsword Cannon"],
            ],
        );
        assert_eq!(find_self(&names(&CATALOG), &three, 40).as_deref(), Some("Aaa Label"), "names across files count");
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(more);
    }

    #[test]
    fn only_the_newest_files_are_read_and_zero_reads_all() {
        let old: &[&str] = &["Alt: Attrition-5 Repeater", "Alt: Singe Cannon (S2)"];
        let mut files: Vec<&[&str]> = vec![old];
        files.extend(std::iter::repeat_n(&[][..], 40));
        let (dir, paths) = logs("neueste", &files);
        assert_eq!(find_self(&names(&CATALOG), &paths, 40), None, "the oldest of 41 is not read");
        assert_eq!(find_self(&names(&CATALOG), &paths, 41).as_deref(), Some("Alt"));
        assert_eq!(find_self(&names(&CATALOG), &paths, 0).as_deref(), Some("Alt"), "[-0:] is the whole list");
        // A file that cannot be read is passed over; bytes that are no UTF-8 are dropped.
        let broken = dir.join("kaputt.log");
        fs::write(
            &broken,
            b"Added notification \"Al\xfft: Attrition-5 Repeater: \"\nAdded notification \"Alt: Singe Cannon (S2): \"",
        )
        .unwrap();
        let list = vec![dir.join("fehlt.log").to_string_lossy().into_owned(), broken.to_string_lossy().into_owned()];
        assert_eq!(find_self(&names(&CATALOG), &list, 40).as_deref(), Some("Alt"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_phrase_is_kept_in_the_own_file_once() {
        let dir = std::env::temp_dir().join(format!("versekit-merken-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("phrasen.json");
        let sources = Sources { phrases_file: Some(file.clone()), localization_folder: None };
        assert!(!sources.remember(""), "nothing to keep");
        assert!(sources.remember("Plan de construction reçu"));
        assert!(!sources.remember("Plan de construction reçu"), "kept once");
        fs::write(&file, r#"{"phrasen": [" Alt ", true, 7, ""], "anders": 1}"#).unwrap();
        assert!(sources.remember("Neu"));
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        let expected = [
            "{",
            " \"phrasen\": [",
            "  \"Alt\",",
            "  \"True\",",
            "  \"7\",",
            "  \"Neu\"",
            " ],",
            " \"_hinweis\": \"Formulierungen, an denen ein neuer Bauplan im Spiel-Log erkannt wird. Selbst gefundene stehen hier mit drin.\"",
            "}",
        ]
        .join(nl);
        assert_eq!(fs::read_to_string(&file).unwrap(), expected, "other keys go, entries become text");
        assert!(sources.remember(" Alt "), "compared as it is, not stripped: kept a second time");
        assert_eq!(sources.own(), ["Alt", "True", "7", "Neu", "Alt"]);
        let nowhere = Sources { phrases_file: None, localization_folder: None };
        assert!(!nowhere.remember("Neu"));
        let folder = Sources { phrases_file: Some(dir.clone()), localization_folder: None };
        assert!(!folder.remember("Neu"), "a folder cannot be written as a file");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn learning_keeps_the_phrase_only_while_it_is_not_certain() {
        let (dir, files) = logs(
            "lernen",
            &[&["Plan de construction reçu: Attrition-5 Repeater", "Plan de construction reçu: Singe Cannon (S2)"]],
        );
        let sources = Sources { phrases_file: Some(dir.join("phrasen.json")), localization_folder: None };
        assert!(!sources.confirmed());
        assert_eq!(sources.learn(&[], &files), None, "no names, nothing to learn from");
        assert_eq!(sources.learn(&names(&CATALOG), &files).as_deref(), Some("Plan de construction reçu"));
        assert!(sources.confirmed());
        assert!(sources.collect().0.contains(&"Plan de construction reçu".to_string()));
        let rx = pattern(&sources.collect().0);
        let line = r#"Added notification "Plan de construction reçu: Attrition-5 Repeater: ""#;
        assert_eq!(rx.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str()), Some("Attrition-5 Repeater"));
        assert_eq!(sources.learn(&names(&CATALOG), &files), None, "certain now: not looked for again");
        // A language file makes the phrase certain as well.
        let loc = dir.join("Localization");
        fs::create_dir_all(loc.join("french_(france)")).unwrap();
        fs::write(loc.join("french_(france)").join("global.ini"), format!("{INI_KEY},P=Plan reçu : %s\n")).unwrap();
        let with_ini = Sources { phrases_file: Some(dir.join("andere.json")), localization_folder: Some(loc) };
        assert_eq!(with_ini.learn(&names(&CATALOG), &files), None);
        assert!(!dir.join("andere.json").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn readable_and_unresolved() {
        assert_eq!(unresolved_key("@Nozzle_FuelGiver_Name"), Some("Nozzle_FuelGiver_Name"));
        assert_eq!(unresolved_key("Attrition-5"), None);
        assert_eq!(readable("Nozzle_FuelGiver_GRIN_NozzleSecure_Name"), "Nozzle FuelGiver GRIN NozzleSecure");
    }
}
