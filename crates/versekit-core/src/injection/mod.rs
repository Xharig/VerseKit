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

//! Blueprint notes in the game's texts: port of `injection.py`.
//!
//! Every contract that hands out blueprints gets the list of what it can
//! give, with a box: ticked what the player has, empty what he lacks; the
//! title gets `[BP]`. Items get their tag `(Mil/1/A)`, ranks their
//! threshold, own ship names replace the factory names, a crafted item whose
//! name key the file lacks gets a stand-in name from its class name (its
//! description key, the block alone), and the version box of the main menu
//! gets one line.
//!
//! Everything hangs on the text key, which is the same in every language.
//! A line of the `global.ini` is `KEY=Text` or `KEY,P=Text`; a break inside
//! a text is the two characters `\n`, never a real line break.
//!
//! No mark stands in the game text. The original text of every touched line
//! is stored (`injektion-urtext.json`) and put back before the next writing
//! and when removing, so nothing stacks up and removing restores the file
//! to the character. Where that note is missing, earlier additions are
//! found by their form; never in a freshly placed file, where the same form
//! can belong to another tool.

pub mod form;
pub mod item_info;
pub mod ranks;
pub mod reputation;
pub mod sources;
pub mod specs;
pub mod texts;

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::asop;
use crate::errors;
use crate::game_ini;
use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::Paths;
use crate::text::lines_universal;

use form::Fallback;
use texts::{BlockContext, py_str, truthy};

/// The note of the original texts.
pub const ORIGTEXT_FILE: &str = asop::ORIGTEXT_FILE;
/// The mark in the original texts for a line that was added.
pub const ADDED: &str = asop::ADDED;
/// The version box of the main menu.
pub const MENU_KEY: &str = "Frontend_PU_Version";
pub const WEBSITE: &str = "https://versekit.xharig.com";
pub const MENU_MARK: &str = "\\nVerse-Kit";
/// The setting „Rufschwelle im Auftragstitel“.
pub const SETTING_TITLE_REP: &str = "ruf_im_titel";
/// The setting „Angaben am Gegenstand“.
pub const SETTING_DETAILS: &str = "angaben_am_gegenstand";

/// One writer of the game's text files at a time in this program: the
/// notes, a fetched translation, the original texts and a restored copy
/// all read the file and the note of original texts and write both back.
static GAME_FILES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The lock file the game files are shared through with the other processes of the run, once
/// [`share_game_files_between_processes`] has set it.
static GAME_FILES_SHARED: std::sync::RwLock<Option<jsonio::ProcessLock>> = std::sync::RwLock::new(None);

/// How long a writer waits for another process that writes the game files: a run of the background holds them
/// while it reads a file of megabytes and writes it back, so this is far longer than the wait for the JSON files.
pub const GAME_FILES_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// The wait in milliseconds ([`GAME_FILES_WAIT`], unless a check shortens it).
static GAME_FILES_WAIT_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(GAME_FILES_WAIT.as_millis() as u64);

/// Shorten or restore the wait for the game files of another process (for checks).
pub fn set_game_files_wait(limit: std::time::Duration) {
    GAME_FILES_WAIT_MS
        .store(u64::try_from(limit.as_millis()).unwrap_or(u64::MAX), std::sync::atomic::Ordering::Relaxed);
}

/// From now on [`game_file_lock`] holds against the other processes that name the same lock file too (overlay and
/// main window write the same `global.ini`, its note of original texts and its copies). Gives `false` where the
/// file cannot be opened: the lock then stays inside this process, and one report says so.
pub fn share_game_files_between_processes(lock_file: &str) -> bool {
    match jsonio::ProcessLock::open(lock_file) {
        Ok(lock) => {
            *GAME_FILES_SHARED.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(lock);
            true
        }
        Err(e) => {
            errors::record("injection.sperre", &format!("lock file {lock_file}: {e}"));
            false
        }
    }
}

/// Holds the game files until it is dropped: the lock of the threads and, where it is shared, the one of the
/// processes, which it lets go of first.
pub struct GameFileGuard {
    _threads: std::sync::MutexGuard<'static, ()>,
    shared: bool,
}

impl Drop for GameFileGuard {
    fn drop(&mut self) {
        if self.shared
            && let Some(lock) = GAME_FILES_SHARED.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref()
        {
            lock.release();
        }
    }
}

/// Hold this while reading and writing a game text file and its notes. With the lock shared it waits for another
/// process at most [`GAME_FILES_WAIT`]: then it goes on without it and reports `injection.sperre`, so a stuck
/// process never stops the other one for good.
pub fn game_file_lock() -> GameFileGuard {
    let threads = GAME_FILES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let wait = std::time::Duration::from_millis(GAME_FILES_WAIT_MS.load(std::sync::atomic::Ordering::Relaxed));
    let shared = match GAME_FILES_SHARED.read().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
        None => false,
        Some(lock) => match lock.acquire(wait) {
            jsonio::Taken::Held => true,
            jsonio::Taken::TimedOut => {
                errors::record("injection.sperre", &"another process held the game files too long");
                false
            }
            jsonio::Taken::Unsupported(e) => {
                errors::record("injection.sperre", &e.to_string());
                false
            }
        },
    };
    GameFileGuard { _threads: threads, shared }
}

/// Write a file of the game without ever leaving it half written: a
/// temporary file beside it, then rename. A protection is never lifted (as
/// in 4.12, which lifted it only for its own files): the player may protect
/// the file on purpose, against a launcher. On Windows a write-protected
/// file makes the writing fail; elsewhere the rights of the folder decide.
///
/// # Errors
///
/// When the temporary file cannot be written or renamed.
pub fn write_game_file(target: &str, bytes: &[u8]) -> std::io::Result<()> {
    let temp = jsonio::temp_name(target);
    let result = jsonio::write_new(&temp, bytes).and_then(|()| jsonio::replace_file(&temp, target));
    if result.is_err() {
        let _ = jsonio::remove_file(&temp);
    }
    result
}

/// Everything the writing needs besides the file and the settings.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    /// The contracts of the catalog (`missionen`), in file order.
    pub missions: Map,
    /// The player's blueprints in comparison form (`collection.keys`).
    pub owned: BTreeSet<String>,
    /// Whom a contract credits reputation, and where it plays.
    pub reputation: reputation::Table,
    /// The rank thresholds.
    pub ranks: ranks::Stored,
    /// The craftable items, with ownership.
    pub items: Vec<item_info::Item>,
    pub shops: item_info::Shops,
    /// The stored English names (`namen-original.json`).
    pub original_names: Vec<(String, String)>,
    /// The text keys of the crafted items from the catalog of the game data;
    /// empty for a catalog without them.
    pub item_texts: item_info::ItemTexts,
    /// The version the menu line names (empty: none).
    pub version: String,
}

/// How a writing ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Written; the number of touched text places (4.12 counts a line that
    /// was reset and written again twice).
    Done(usize),
    /// No `global.ini` there (`m_keine_ini`).
    NoFile,
    /// The catalog knows no contracts (`m_keine_missionen`).
    NoMissions,
    ReadFailed(String),
    WriteFailed(String),
}

// ------------------------------------------------------- the note file

fn origtext_file(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(ORIGTEXT_FILE)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// `injection.load_origtext`.
pub fn load_origtext(paths: &Paths) -> Map {
    match field(&origtext_file(paths), "texte") {
        Some(Value::Object(texts)) => texts.clone(),
        _ => Map::new(),
    }
}

/// `injection.is_fresh`: a freshly placed file nobody wrote into yet.
pub fn is_fresh(paths: &Paths) -> bool {
    truthy(field(&origtext_file(paths), "frisch"))
}

/// `time.strftime('%Y-%m-%d %H:%M:%S')`.
pub fn now_stamp() -> String {
    let (y, mo, d, h, mi, s) = jsonio::local_time();
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

fn write_note(paths: &Paths, data: Map, label: &str) -> bool {
    let text = jsonio::dumps(&Value::Object(data), None, false);
    match jsonio::write_text_atomic(&paths.app_file(ORIGTEXT_FILE), &text, jsonio::Newline::Keep) {
        Ok(()) => true,
        Err(error) => {
            errors::record(label, &error);
            false
        }
    }
}

/// `injection.discard_origtext`: forget the stored texts and mark the file
/// as fresh. Belongs to every placing of a new base file.
pub fn discard_origtext(paths: &Paths) -> bool {
    let data = vec![
        ("stand".to_string(), Value::str(now_stamp())),
        ("frisch".to_string(), Value::Bool(true)),
        ("texte".to_string(), Value::Object(Map::new())),
    ];
    write_note(paths, data, "injection.discard_origtext")
}

/// `discard_origtext` for a new base file at `target`, unless the note holds
/// original texts of another file: those are still needed to reset that
/// file (`clean_leftover`), and a side channel never owns them.
pub fn discard_origtext_for(paths: &Paths, target: &str) -> bool {
    if origtext_held_for_other(paths, target) {
        return false;
    }
    discard_origtext(paths)
}

/// Whether the note holds original texts of a file other than `target` that
/// is still there. A file that is gone needs them no more.
pub fn origtext_held_for_other(paths: &Paths, target: &str) -> bool {
    let note = origtext_file(paths);
    let datei = note_file(&note);
    truthy(field(&note, "texte"))
        && !datei.is_empty()
        && !same_file(paths, datei, target)
        && crate::game_ini::is_file(datei)
}

fn note_file(note: &Map) -> &str {
    field(note, "datei").and_then(Value::as_str).map(crate::text::py_strip).unwrap_or("")
}

/// The whole note when it holds original texts of `target`: what a copy of
/// that file needs to be reset later.
pub fn origtext_note_of(paths: &Paths, target: &str) -> Option<Map> {
    let note = origtext_file(paths);
    let datei = note_file(&note);
    (truthy(field(&note, "texte")) && !datei.is_empty() && same_file(paths, datei, target)).then_some(note)
}

/// Put back a note that [`origtext_note_of`] gave.
pub fn put_origtext_note(paths: &Paths, note: Map) -> bool {
    write_note(paths, note, "injection.put_origtext_note")
}

/// Whether two paths name the same file (`os.path.samefile`, else by text).
fn same_file(paths: &Paths, a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => {
            let flavor = paths.host.flavor;
            flavor.normcase(&flavor.normpath(a)) == flavor.normcase(&flavor.normpath(b))
        }
    }
}

/// `injection.save_origtext`.
pub fn save_origtext(paths: &Paths, texts: Map, ini: &str) -> bool {
    let data = vec![
        ("datei".to_string(), Value::str(ini)),
        ("stand".to_string(), Value::str(now_stamp())),
        ("texte".to_string(), Value::Object(texts)),
    ];
    write_note(paths, data, "injection.save_origtext")
}

// ----------------------------------------------------- recognising

/// `injection.is_applied`: does this file hold an addition of ours? Only
/// forms that are ours alone count; the bare `[BP]` is written by others.
pub fn is_applied(ini: &str) -> bool {
    let Ok(bytes) = std::fs::read(ini) else { return false };
    let text = crate::text::decode_ignore(&bytes);
    lines_universal(&text).into_iter().any(|line| {
        line.contains(form::OPEN)
            || form::COUNTING_TITLE.is_match(line)
            || (form::OWN_TRACE.is_match(line) && form::has_box(line))
    })
}

/// `injection._fallback_form`.
fn fallback_form(paths: &Paths, origtext_old: &Map, ini: &str) -> Fallback {
    if is_fresh(paths) || !origtext_old.is_empty() {
        return Fallback::Off;
    }
    if is_applied(ini) { Fallback::Full } else { Fallback::Block }
}

/// `injection._strip_old`: the stored original first, then the form.
fn strip_old(text: &str, key: &str, origtext: &Map, fallback: Fallback) -> String {
    if let Some(Value::Str(original)) = field(origtext, key) {
        return original.clone();
    }
    form::strip_form(text, fallback)
}

/// `injection.menu_lines`: our one line for the version box.
pub fn menu_line(version: &str) -> String {
    let suffix = if version.is_empty() { String::new() } else { format!(" v{version}") };
    let site = WEBSITE.split_once("//").map_or(WEBSITE, |(_, rest)| rest);
    format!("{MENU_MARK}{suffix} · {site}")
}

/// `injection.stock_mark`: a short fingerprint of the own blueprints, by
/// their names.
pub fn stock_mark(owned: &BTreeSet<String>) -> String {
    let raw = owned.iter().cloned().collect::<Vec<_>>().join("\n");
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, raw.as_bytes());
    let hex: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}", owned.len(), &hex[..12])
}

// ------------------------------------------------------- the writing

/// `injection.apply_texts`: write the notes into a `global.ini`, or only
/// take them out (`remove_only`). The file is written whole to a temporary
/// file and then renamed; on any failure the old one stays.
pub fn apply_texts(paths: &Paths, ini: &str, language: &str, inputs: &Inputs, remove_only: bool) -> Outcome {
    let _one_writer = game_file_lock();
    if !game_ini::is_file(ini) {
        return Outcome::NoFile;
    }
    if inputs.missions.is_empty() && !remove_only {
        return Outcome::NoMissions;
    }
    let words = texts::words_for(language, paths.setting_bool(SETTING_TITLE_REP, true));

    // Titles get the mark, descriptions the list; the later entry wins.
    let mut title_keys: HashMap<String, &Map> = HashMap::new();
    let mut text_keys: HashMap<String, &Map> = HashMap::new();
    for (_, entry) in &inputs.missions {
        let Some(entry) = entry.as_object() else { continue };
        if let Some(key) = field(entry, "titel_key").filter(|v| truthy(Some(v))) {
            title_keys.insert(py_str(key), entry);
        }
        if let Some(key) = field(entry, "text_key").filter(|v| truthy(Some(v))) {
            text_keys.insert(py_str(key), entry);
        }
    }

    let bytes = match std::fs::read(ini) {
        Ok(bytes) => bytes,
        Err(error) => return Outcome::ReadFailed(error.to_string()),
    };
    let text = crate::text::decode_ignore(&bytes);
    let lines: Vec<String> = crate::text::py_splitlines(&text).into_iter().map(str::to_string).collect();

    // Original texts of another file that is still there would put that
    // file's wording (maybe another language) into this one: not used here.
    // On purpose unlike 4.12, which takes the note whatever file it names.
    let mut origtext_old = if origtext_held_for_other(paths, ini) { Map::new() } else { load_origtext(paths) };
    let mut origtext_new = Map::new();
    let fallback = fallback_form(paths, &origtext_old, ini);
    // A line we added for a crafted item that now holds another text is the
    // game's own: it is no addition of ours any more and stays as it is.
    if !origtext_old.is_empty() && !inputs.item_texts.is_empty() {
        let held: HashMap<&str, &str> =
            lines.iter().filter_map(|line| form::split_line(line)).map(|(key, _, text)| (key, text)).collect();
        origtext_old.retain(|(key, mark)| {
            !matches!(mark, Value::Str(s) if s == ADDED)
                || !item_info::foreign_text(key, held.get(key.as_str()).copied(), &inputs.item_texts)
        });
    }
    let name_suffix = if remove_only || !paths.setting_bool(SETTING_DETAILS, true) {
        HashMap::new()
    } else {
        specs::build_table(&lines)
    };
    let added_ships = if remove_only { Vec::new() } else { asop::added_ship_names(paths, ini, &lines, &origtext_old) };
    let own_ships: HashMap<String, (String, bool)> = if remove_only {
        HashMap::new()
    } else {
        let mut with_added = lines.clone();
        with_added.extend(added_ships.iter().map(|(k, v)| format!("{k}={v}")));
        asop::build_table(&with_added, &asop::load(&paths.app_file(asop::FILE))).into_iter().collect()
    };
    let rank_suffix: HashMap<String, String> = if remove_only {
        HashMap::new()
    } else {
        ranks::build_table(paths, texts::lang_code(language), &inputs.ranks).into_iter().collect()
    };
    // Name and description keys of crafted items that no line of the file
    // holds (lines we added last time do not count): a stand-in name, and our
    // block alone for the description.
    let gaps = if remove_only {
        Vec::new()
    } else {
        let present: HashSet<String> = lines
            .iter()
            .filter_map(|line| form::split_line(line))
            .filter(|(key, _, _)| !matches!(field(&origtext_old, key), Some(Value::Str(s)) if s == ADDED))
            .map(|(key, _, _)| key.to_lowercase())
            .collect();
        item_info::missing_texts(&present, &inputs.items, &inputs.item_texts, item_info::enabled(paths))
    };
    let item_lines: Vec<String>;
    let item_source: &[String] = if gaps.is_empty() {
        &lines
    } else {
        item_lines = lines
            .iter()
            .cloned()
            .chain(gaps.iter().map(|gap| format!("{}={}", gap.key, gap.name.as_deref().unwrap_or(""))))
            .collect();
        &item_lines
    };
    let item_block = if remove_only || !item_info::enabled(paths) {
        HashMap::new()
    } else {
        let mut raw_titles: HashMap<&str, &str> = HashMap::new();
        for line in &lines {
            if let Some((key, _, text)) = form::split_line(line)
                && title_keys.contains_key(key)
            {
                raw_titles.insert(key, text);
            }
        }
        let title_of = |key: &str| -> String {
            let Some(text) = raw_titles.get(key).filter(|t| !t.is_empty()) else { return String::new() };
            let text = strip_old(text, key, &origtext_old, fallback);
            if text == ADDED {
                return String::new();
            }
            crate::text::py_strip(&form::TITLE_MARK.replace_all(&text, "")).to_string()
        };
        let digits = |v: &Value| texts::group_digits(texts::py_int(Some(v)).unwrap_or(0), words.separator);
        let ctx = item_info::ItemContext { words: &words, title_of: &title_of, digits: &digits };
        item_info::build(
            item_source,
            &inputs.missions,
            &inputs.items,
            &inputs.shops,
            &inputs.original_names,
            language.to_lowercase().starts_with("english"),
            &inputs.item_texts,
            &ctx,
        )
    };

    // Every description of a contract that gets notes gets the same block,
    // found by the beginning title and descriptions share; the first wins.
    let mut stem_block: HashMap<String, &Map> = HashMap::new();
    if !remove_only {
        for (_, entry) in &inputs.missions {
            let Some(entry) = entry.as_object() else { continue };
            let key = [field(entry, "titel_key"), field(entry, "text_key")]
                .into_iter()
                .flatten()
                .find(|v| truthy(Some(v)))
                .map(py_str)
                .unwrap_or_default();
            let stem = form::stem(&key);
            if !stem.is_empty() {
                stem_block.entry(stem).or_insert(entry);
            }
        }
    }
    let block_for = |entry: &Map| {
        let title = field(entry, "titel_key").filter(|v| truthy(Some(v))).map(py_str).unwrap_or_default();
        let ctx = BlockContext {
            words: &words,
            owned: &inputs.owned,
            reputation_line: reputation::line(&inputs.reputation, &title, words.gain_at),
            region_line: reputation::region_line(&inputs.reputation, &title, words.region),
        };
        texts::build_block(entry, &ctx)
    };

    let mut changed = 0usize;
    let mut new: Vec<String> = Vec::with_capacity(lines.len() + added_ships.len());
    for line in &lines {
        let Some((key, suffix, text)) = form::split_line(line) else {
            new.push(line.clone());
            continue;
        };
        let mut orig = strip_old(text, key, &origtext_old, fallback);
        if orig == ADDED {
            changed += 1;
            continue;
        }
        if orig != text {
            changed += 1;
        }
        let mut clean = orig.clone();
        if !remove_only {
            // A foreign block of the same source is replaced by ours; the
            // stored original keeps it, so it comes back when removing.
            let (base, _foreign) = form::split_foreign_block(&orig);
            let mut touched = false;
            if key.trim_start_matches('\u{feff}') == MENU_KEY {
                orig = orig.split(MENU_MARK).next().unwrap_or("").to_string();
                while let Some(shorter) = orig.strip_suffix("\\n") {
                    orig = shorter.to_string();
                }
                clean = format!("{orig}{}", menu_line(&inputs.version));
                touched = true;
            } else if let Some((own, star)) = own_ships.get(key) {
                clean = asop::display_name(&base, own, *star);
                touched = clean != base;
            } else if let Some(tail) = rank_suffix.get(key) {
                clean = ranks::with_suffix(&base, tail);
                touched = clean != base;
            } else if let Some(tag) = name_suffix.get(key) {
                clean = form::name_with_detail(&base, tag);
                touched = true;
            } else if let Some(block) = item_block.get(key) {
                clean = form::append_block(&base, block);
                touched = true;
            } else if let Some(entry) = title_keys.get(key) {
                if !form::has_title_mark(&base) {
                    clean = format!("{base}{}", texts::title_suffix(entry, &words));
                    touched = true;
                }
            } else if let Some(entry) = text_keys.get(key) {
                clean = form::append_block(&base, &block_for(entry));
                touched = true;
            } else if key.to_lowercase().contains("_desc")
                && let Some(entry) = stem_block.get(&form::stem(key))
            {
                clean = form::append_block(&base, &block_for(entry));
                touched = true;
            }
            if touched {
                map_set(&mut origtext_new, key, Value::str(orig.clone()));
                changed += 1;
            } else {
                clean = orig;
            }
        }
        new.push(format!("{key}{suffix}={clean}"));
    }
    for (key, value) in &added_ships {
        let text = match own_ships.get(key) {
            Some((own, star)) => asop::display_name(value, own, *star),
            None => value.clone(),
        };
        new.push(format!("{key}={text}"));
        map_set(&mut origtext_new, key, Value::str(ADDED));
    }
    changed += added_ships.len();
    // A description gets the block without the blank line it starts with;
    // where the block has no item to speak of, nothing is written.
    let added_items: Vec<(&str, String)> = gaps
        .iter()
        .filter_map(|gap| match &gap.name {
            Some(name) => Some((gap.key.as_str(), name.clone())),
            None => {
                item_block.get(&gap.key).map(|block| (gap.key.as_str(), block.trim_start_matches("\\n").to_string()))
            }
        })
        .collect();
    for (key, text) in &added_items {
        new.push(format!("{key}={text}"));
        map_set(&mut origtext_new, key, Value::str(ADDED));
    }
    changed += added_items.len();

    let mut out = new.iter().map(|line| form::single_line(line)).collect::<Vec<_>>().join("\n");
    out.push('\n');
    if let Err(error) = write_game_file(ini, out.as_bytes()) {
        return Outcome::WriteFailed(error.to_string());
    }
    save_origtext(paths, origtext_new, ini);
    Outcome::Done(changed)
}

/// `injection.remove_texts`: take every addition back.
pub fn remove_texts(paths: &Paths, ini: &str, language: &str) -> Outcome {
    apply_texts(paths, ini, language, &Inputs::default(), true)
}

/// `injection.leftover_file`: the file our additions still stand in,
/// though nobody tends them any more, or `None`.
pub fn leftover_file(paths: &Paths, new_path: Option<&str>) -> Option<String> {
    let note = origtext_file(paths);
    let old = field(&note, "datei").and_then(Value::as_str).map(crate::text::py_strip).unwrap_or("").to_string();
    if old.is_empty() || !truthy(field(&note, "texte")) {
        return None;
    }
    if let Some(new_path) = new_path.filter(|p| !p.is_empty()) {
        let same = match (std::fs::canonicalize(&old), std::fs::canonicalize(new_path)) {
            (Ok(a), Ok(b)) => a == b,
            _ => {
                let flavor = paths.host.flavor;
                flavor.normpath(&old) == flavor.normpath(new_path)
            }
        };
        if same {
            return None;
        }
    }
    game_ini::is_file(&old).then_some(old)
}

/// `injection.clean_leftover`: before a change of source, reset the old
/// file. First clean up, then set up: the reset needs the old notes.
pub fn clean_leftover(paths: &Paths, new_path: Option<&str>) -> Option<(String, usize)> {
    let old = leftover_file(paths, new_path)?;
    let flavor = paths.host.flavor;
    let folder = flavor.basename(&flavor.dirname(&old));
    let folder = if folder.is_empty() { "english".to_string() } else { folder };
    match remove_texts(paths, &old, &folder) {
        Outcome::Done(count) => Some((old, count)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
