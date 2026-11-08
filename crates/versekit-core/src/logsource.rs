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

//! Read the Game.log, live and retroactively.
//!
//! **Tailing** (`LogTail`): follow the running Game.log and report new
//! blueprints immediately.
//!
//! **Backlog reading** (`LogSource::read_backlog`): go through the preserved
//! logs of past sessions (`logbackups/`). What has already been read is
//! remembered by the read position (`logstand.json`).
//!
//! **The honest gap:** If the last session read lies further back than the
//! oldest backup still present, something is missing in between. This is
//! reported by `Report::gap` as data; the text comes from the language module.
//!
//! **Whom does a log belong to?** Anyone playing two accounts on one machine
//! has the logs of both in `logbackups/`. Blueprints of other accounts do not
//! count; the account appears in the first ~50 KB of each log and is
//! remembered per file (`konten-je-log.json`).
//!
//! The own account is told by its fixed account number where the log names it
//! (`accountId` in the line of the logged-in character), so a changed handle
//! stays the own account; logs without that line are told by the handle.
//! Blueprints of logs that did not count are tallied per session in
//! `fremde-funde.json`, without any handle or number.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use regex::Regex;

use crate::jsonio::{self, Map, Value};
use crate::logread;
pub use crate::logscan::Identity;
use crate::logscan::{self, Find, account_from_text, account_id_from_text, names_from_text};
use crate::paths::{Paths, mtime_seconds, nanos, now_seconds};
use crate::phrases::Sources;
use crate::pypath::Flavor;
use crate::text::{decode_ignore, py_strip};

/// Setting: the user's own account.
pub const ACCOUNT_SETTING: &str = "eigener_account";
/// Setting: the fixed account number of the own account, next to its handle. The key of version 4.12.4,
/// so a number learned there is kept.
pub const ACCOUNT_ID_SETTING: &str = "eigene_account_kennung";
/// Setting: the logs remembered as read with another handle were looked at once more by account number.
pub const ACCOUNTS_RECHECKED: &str = "konten_nachgeprueft";
/// The blueprints of logs that did not count, per session.
pub const FOREIGN_FILE: &str = "fremde-funde.json";
/// Value of the setting when all accounts should count.
pub const ALL_ACCOUNTS: &str = "*";
/// The remembered accounts per backup.
pub const ACCOUNTS_FILE: &str = "konten-je-log.json";
/// The read position.
pub const READ_STATE_FILE: &str = "logstand.json";

/// Every account the data folder knows: the own one of the settings, those remembered per log
/// ([`ACCOUNTS_FILE`]) and per read backup ([`READ_STATE_FILE`]). For the redaction of the
/// error report; reads only, without the logs themselves.
pub fn known_accounts(paths: &Paths) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut take = |name: &str| {
        let name = py_strip(name);
        if !name.is_empty() && name != ALL_ACCOUNTS && !found.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            found.push(name.to_string());
        }
    };
    if let Some(name) = paths.setting(ACCOUNT_SETTING) {
        take(&name);
    }
    if let Ok(Value::Object(logs)) = jsonio::read_json(&paths.app_file(ACCOUNTS_FILE)) {
        for (_, entry) in &logs {
            if let Value::Array(parts) = entry
                && let Some(name) = parts.get(2).and_then(Value::as_str)
            {
                take(name);
            }
        }
    }
    if let Ok(state) = jsonio::read_json(&paths.app_file(READ_STATE_FILE))
        && let Some(Value::Object(backups)) = state.get("sicherungen")
    {
        for (_, entry) in backups {
            if let Some(name) = entry.get("account").and_then(Value::as_str) {
                take(name);
            }
        }
    }
    found
}

/// If no account can be identified yet, the search runs at most this often (seconds).
const OWN_RETRY_SEC: f64 = 300.0;
/// A set own account is compared with the logs at most this often (seconds): number learned, handle followed.
const OWN_FOLLOW_SEC: u64 = 300;
/// Remembered accounts: saved after this many new entries …
const SAVE_EVERY: u32 = 40;
/// … or when the last save was this long ago (seconds).
const SAVE_AFTER_SEC: f64 = 5.0;
/// Size of the in-memory caches; they are cleared above this.
const ACCOUNT_CACHE_MAX: usize = 5000;
const READ_CACHE_MAX: usize = 1000;

/// Keys of the messages in the language module.
pub const KEY_NO_LOGS: &str = "m_keine_logs";
pub const KEY_FIRST_RUN: &str = "m_erster_lauf";
pub const KEY_GAP: &str = "m_luecke_logs";

/// The clock: seconds since 1970 like `time.time()`. Test runs set their own.
pub type Clock = Box<dyn Fn() -> f64 + Send + Sync>;

// ------------------------------------------------------------ small helpers

/// Size and modification time (ns) like `os.stat`.
fn stat(path: &str) -> Option<(i128, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((nanos(meta.modified().ok()?), meta.len()))
}

fn file_size(path: &str) -> Option<u64> {
    fs::metadata(path).ok().map(|m| m.len())
}

/// Python numeric value of a JSON value (`True` counts as 1).
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(text) => text.parse::<f64>().ok(),
        Value::Float(x) => Some(*x),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// `value == n` using Python's comparison of numbers.
fn num_eq(value: Option<&Value>, n: i128) -> bool {
    match value {
        Some(Value::Int(text)) => text.parse::<i128>() == Ok(n),
        Some(Value::Float(x)) => x.fract() == 0.0 && *x == n as f64 && (*x as i128) == n,
        Some(Value::Bool(b)) => i128::from(*b) == n,
        _ => false,
    }
}

/// Python truthiness of a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(text) => text.trim_start_matches('-').bytes().any(|b| b != b'0'),
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// `max(stored, x)` like Python: on a tie the stored value is kept.
fn py_max(stored: Option<&Value>, x: f64) -> Value {
    match stored {
        None => Value::Float(0.0_f64.max(x)),
        Some(value) => match number(value) {
            Some(current) if x > current => Value::Float(x),
            Some(_) => value.clone(),
            None => Value::Float(x),
        },
    }
}

fn obj<'m>(map: &'m Map, key: &str) -> Option<&'m Map> {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_object())
}

/// The sub-object for the key; if it is not one, it becomes one.
fn obj_mut<'m>(map: &'m mut Map, key: &str) -> &'m mut Map {
    let index = match map.iter().position(|(k, _)| k == key) {
        Some(i) => i,
        None => {
            map.push((key.to_string(), Value::Object(Vec::new())));
            map.len() - 1
        }
    };
    if !matches!(map[index].1, Value::Object(_)) {
        map[index].1 = Value::Object(Vec::new());
    }
    match &mut map[index].1 {
        Value::Object(inner) => inner,
        _ => unreachable!("eben gesetzt"),
    }
}

fn basename(path: &str) -> String {
    Flavor::native().basename(path)
}

/// Key for deduplicating names: lowercase, without surrounding whitespace.
fn name_key(name: &str) -> String {
    py_strip(&name.to_lowercase()).to_string()
}

/// Does a log of this account count? Unknown counts (better one blueprint
/// too many than a lost one).
pub fn counts_for(own: Option<&str>, account: Option<&str>) -> bool {
    let own = own.filter(|o| !o.is_empty());
    let account = account.filter(|a| !a.is_empty());
    match (own, account) {
        (None, _) | (_, None) => true,
        (Some(own), _) if own == ALL_ACCOUNTS => true,
        (Some(own), Some(account)) => account.to_lowercase() == own.to_lowercase(),
    }
}

/// The own account: its handle (or [`ALL_ACCOUNTS`]) and, once known, its fixed account number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Own {
    pub handle: String,
    pub id: Option<String>,
}

impl Own {
    /// Does a log of this identity count? The account number decides where both sides have one, else the
    /// handle as in [`counts_for`].
    pub fn counts(&self, log: &Identity) -> bool {
        if self.handle == ALL_ACCOUNTS {
            return true;
        }
        if let (Some(own), Some(theirs)) = (&self.id, &log.id) {
            return own == theirs;
        }
        counts_for(Some(&self.handle), log.handle.as_deref())
    }
}

/// [`Own::counts`]; without an own account every log counts.
pub fn counts_own(own: Option<&Own>, log: &Identity) -> bool {
    own.is_none_or(|own| own.counts(log))
}

fn same_handle(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

// ------------------------------------------------------------ blueprints of other accounts

/// What the logs of other accounts held and did not count. No handle, no number: only how many blueprints
/// (each once), from how many sessions, and when the last of them began.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForeignTally {
    pub blueprints: usize,
    pub sessions: usize,
    /// Seconds since 1970 of the start of the last session with such blueprints.
    pub last: Option<i64>,
}

fn tally_sessions(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(FOREIGN_FILE)) {
        Ok(root) => match root.get("sitzungen") {
            Some(Value::Object(sessions)) => sessions.clone(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

fn write_tally(paths: &Paths, sessions: Map) -> bool {
    let root = Value::Object(vec![("sitzungen".into(), Value::Object(sessions))]);
    let text = jsonio::dumps(&root, Some(1), false);
    jsonio::write_text_atomic(&paths.app_file(FOREIGN_FILE), &text, jsonio::Newline::Keep).is_ok()
}

/// The tally of the blueprints that did not count.
pub fn foreign_tally(paths: &Paths) -> ForeignTally {
    let mut names = BTreeSet::new();
    let mut tally = ForeignTally::default();
    for (_, entry) in tally_sessions(paths) {
        let Some(Value::Array(keys)) = entry.get("bauplaene") else { continue };
        let keys: Vec<&str> = keys.iter().filter_map(Value::as_str).collect();
        if keys.is_empty() {
            continue;
        }
        names.extend(keys.into_iter().map(str::to_string));
        tally.sessions += 1;
        let at = match entry.get("zeit") {
            Some(Value::Int(text)) => text.parse::<i64>().ok(),
            Some(Value::Float(x)) => Some(*x as i64),
            _ => None,
        };
        tally.last = tally.last.max(at);
    }
    tally.blueprints = names.len();
    tally
}

/// Forgets the tally (the own account was chosen anew: the logs are read again and tally anew).
pub fn clear_foreign_tally(paths: &Paths) -> bool {
    let _lock = jsonio::lock_saves();
    !Path::new(&paths.app_file(FOREIGN_FILE)).exists() || write_tally(paths, Vec::new())
}

/// The file names of the tallied sessions.
fn tallied_files(paths: &Paths) -> HashSet<String> {
    tally_sessions(paths)
        .iter()
        .filter_map(|(_, entry)| entry.get("datei").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// The key of a session in the tally: its first stamp, so the running `Game.log` and the backup it becomes
/// are one session; without a stamp the file name.
fn session_key(path: &str) -> (String, Option<i64>) {
    let start = crate::mission_scan::session_start_of(path);
    match crate::logtime::seconds(&start) {
        Some(at) => (start, Some(at)),
        None => (format!("datei:{}", basename(path)), None),
    }
}

/// Changes to the tally, written in one go.
#[derive(Default)]
struct TallyChanges {
    /// Session, file name, blueprint keys, start.
    add: Vec<(String, String, Vec<String>, Option<i64>)>,
    /// File names of logs that count now.
    drop: Vec<String>,
}

impl TallyChanges {
    fn add(&mut self, path: &str, finds: &[Find]) {
        let (session, at) = session_key(path);
        let keys = finds.iter().map(|(name, _)| name_key(name)).collect();
        self.add.push((session, basename(path), keys, at));
    }

    /// Loads the tally under the write lock, changes it and writes it if anything changed.
    fn apply(self, paths: &Paths) {
        if self.add.is_empty() && self.drop.is_empty() {
            return;
        }
        let _lock = jsonio::lock_saves();
        let before = tally_sessions(paths);
        let mut sessions = before.clone();
        sessions.retain(|(_, entry)| {
            !entry.get("datei").and_then(Value::as_str).is_some_and(|file| self.drop.iter().any(|d| d == file))
        });
        for (session, file, keys, at) in self.add {
            let mut entry: Map = match sessions.iter().find(|(k, _)| *k == session) {
                Some((_, Value::Object(entry))) => entry.clone(),
                _ => Vec::new(),
            };
            let mut known: Vec<String> = match entry.iter().find(|(k, _)| k == "bauplaene") {
                Some((_, Value::Array(items))) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                _ => Vec::new(),
            };
            for key in keys {
                if !known.contains(&key) {
                    known.push(key);
                }
            }
            jsonio::map_set(&mut entry, "datei", Value::str(file));
            jsonio::map_set(&mut entry, "bauplaene", Value::Array(known.into_iter().map(Value::Str).collect()));
            if let Some(at) = at {
                jsonio::map_set(&mut entry, "zeit", Value::int(at));
            }
            jsonio::map_set(&mut sessions, &session, Value::Object(entry));
        }
        if sessions != before {
            write_tally(paths, sessions);
        }
    }
}

// ------------------------------------------------------------ read position

/// Remembers what has already been read, across program restarts.
///
/// The file is held as a whole: unknown fields and the order
/// are preserved when writing back.
#[derive(Clone, Debug)]
pub struct ReadState {
    pub path: String,
    pub data: Map,
}

impl ReadState {
    /// Load the read position from the app folder.
    pub fn open(paths: &Paths) -> ReadState {
        ReadState::load(paths.app_file(READ_STATE_FILE))
    }

    /// Unreadable, not an object, broken: then an empty state.
    pub fn load(path: String) -> ReadState {
        let data = match jsonio::read_json(&path) {
            Ok(Value::Object(mut map)) => {
                for (key, default) in [
                    ("aktiv", Value::Object(Vec::new())),
                    ("sicherungen", Value::Object(Vec::new())),
                    ("letzte_sitzung", Value::Float(0.0)),
                ] {
                    if !map.iter().any(|(k, _)| k == key) {
                        map.push((key.to_string(), default));
                    }
                }
                map
            }
            _ => Self::empty(),
        };
        ReadState { path, data }
    }

    fn empty() -> Map {
        vec![
            ("aktiv".into(), Value::Object(Vec::new())),
            ("sicherungen".into(), Value::Object(Vec::new())),
            ("letzte_sitzung".into(), Value::Float(0.0)),
        ]
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Write via a temporary file; a failure stays silent.
    pub fn save(&self) {
        let _ = (|| -> io::Result<()> {
            jsonio::make_dirs(&Flavor::native().dirname(&self.path))?;
            let temp = format!("{}.tmp", self.path);
            jsonio::write_new(&temp, &jsonio::file_bytes(&Value::Object(self.data.clone()), 1))?;
            jsonio::os_replace(&temp, &self.path)
        })();
    }

    /// Has this backup already been read? Recognized by name, size and time;
    /// if a file grows after all, it counts as unread again.
    pub fn knows(&self, filename: &str) -> bool {
        self.knows_with(filename, || file_size(filename), || mtime_seconds(filename))
    }

    /// [`knows`](Self::knows) with size and time given by the caller (from a
    /// folder listing, which is much cheaper than one `stat` per file). The
    /// closures run only when the entry makes them necessary.
    pub fn knows_with(
        &self,
        filename: &str,
        size: impl FnOnce() -> Option<u64>,
        mtime: impl FnOnce() -> Option<f64>,
    ) -> bool {
        let Some(entries) = obj(&self.data, "sicherungen") else { return false };
        let Some((_, Value::Object(entry))) = entries.iter().find(|(k, _)| *k == basename(filename)) else {
            return false;
        };
        if entry.is_empty() {
            return false;
        }
        let field = |key: &str| entry.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let Some(size) = size() else { return false };
        if !num_eq(field("groesse"), i128::from(size)) {
            return false;
        }
        let stored = match field("mtime") {
            None => 0.0,
            Some(v) => match number(v) {
                Some(x) => x,
                None => return false,
            },
        };
        mtime().is_some_and(|m| (stored - m).abs() < 1.0)
    }

    /// Record a backup as read, along with the account if known.
    pub fn remember(&mut self, filename: &str, account: Option<&str>) {
        let (Some(size), Some(mtime)) = (file_size(filename), mtime_seconds(filename)) else { return };
        let mut entry: Map =
            vec![("groesse".into(), Value::Int(size.to_string())), ("mtime".into(), Value::Float(mtime))];
        if let Some(account) = account.filter(|a| !a.is_empty()) {
            entry.push(("account".into(), Value::str(account)));
        }
        jsonio::map_set(obj_mut(&mut self.data, "sicherungen"), &basename(filename), Value::Object(entry));
        let last = py_max(self.get("letzte_sitzung"), mtime);
        jsonio::map_set(&mut self.data, "letzte_sitzung", last);
    }

    /// The account a read backup was remembered with.
    pub fn account_of(&self, filename: &str) -> Option<String> {
        let entries = obj(&self.data, "sicherungen")?;
        let (_, entry) = entries.iter().find(|(k, _)| *k == basename(filename))?;
        entry.get("account").and_then(Value::as_str).map(str::to_string)
    }

    /// Discard entries for backups that no longer exist.
    pub fn cleanup(&mut self, existing: &[String]) {
        let present: HashSet<String> = existing.iter().map(|p| basename(p)).collect();
        if let Some((_, Value::Object(entries))) = self.data.iter_mut().find(|(k, _)| k == "sicherungen") {
            entries.retain(|(k, _)| present.contains(k));
        }
    }

    /// The remembered read position of the running log: `None` if it belongs
    /// to a different file or is stored as `null`; if missing, 0 applies.
    pub fn get_active(&self, path: &str) -> Option<i128> {
        let entry = obj(&self.data, "aktiv")?;
        let field = |key: &str| entry.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        if field("pfad").and_then(Value::as_str) != Some(path) {
            return None;
        }
        match field("offset") {
            None => Some(0),
            Some(Value::Null) => None,
            Some(Value::Int(text)) => Some(text.parse().unwrap_or(0)),
            Some(Value::Bool(b)) => Some(i128::from(*b)),
            Some(Value::Float(x)) => Some(*x as i128),
            Some(_) => Some(0),
        }
    }

    pub fn set_active(&mut self, path: &str, offset: i128, clock: &dyn Fn() -> f64) {
        let entry = vec![
            ("pfad".into(), Value::str(path)),
            ("offset".into(), Value::Int(offset.to_string())),
            ("zeit".into(), Value::Float(clock())),
        ];
        jsonio::map_set(&mut self.data, "aktiv", Value::Object(entry));
        let last = py_max(self.get("letzte_sitzung"), clock());
        jsonio::map_set(&mut self.data, "letzte_sitzung", last);
    }
}

// ------------------------------------------------------------ report

/// Why something remains unknown despite backlog reading.
#[derive(Clone, Debug, PartialEq)]
pub enum Gap {
    /// There are no preserved logs at all.
    NoLogs,
    /// First run: what lies before the oldest backup has never been read.
    FirstRun { oldest: f64 },
    /// Not run for too long: between the last session read and the oldest
    /// backup, Star Citizen has cleared out logs.
    Missing { last: f64, oldest: f64 },
}

impl Gap {
    /// Key of the message in the language module.
    pub fn key(&self) -> &'static str {
        match self {
            Gap::NoLogs => KEY_NO_LOGS,
            Gap::FirstRun { .. } => KEY_FIRST_RUN,
            Gap::Missing { .. } => KEY_GAP,
        }
    }

    /// The points in time for the message, in their order.
    pub fn moments(&self) -> Vec<f64> {
        match self {
            Gap::NoLogs => Vec::new(),
            Gap::FirstRun { oldest } => vec![*oldest],
            Gap::Missing { last, oldest } => vec![*last, *oldest],
        }
    }
}

/// What happened during backlog reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Backups read.
    pub files: usize,
    /// Skipped because already known.
    pub skipped: usize,
    /// Blueprints found, deduplicated.
    pub found: usize,
    /// Backups available.
    pub available: usize,
    pub gap: Option<Gap>,
    /// The running Game.log was read as well.
    pub running: bool,
    /// Not readable; will be retried on the next run.
    pub unreadable: usize,
    /// Logs of another account.
    pub foreign: usize,
}

/// Why reading all logs again ([`LogSource::read_all`]) did not run.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadAllError {
    /// No game folder with a `Game.log` is found.
    NoGameFolder,
    /// The folder of the preserved logs or one of the logs cannot be opened.
    Unreadable { path: String, reason: String },
}

impl std::fmt::Display for ReadAllError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadAllError::NoGameFolder => write!(f, "no game folder with a Game.log"),
            ReadAllError::Unreadable { path, reason } => write!(f, "cannot open {path}: {reason}"),
        }
    }
}

impl std::error::Error for ReadAllError {}

// ------------------------------------------------------------ log source

#[derive(Default)]
struct Memo {
    accounts: HashMap<(String, i128, u64), Identity>,
    stored: Option<Map>,
    unsaved: u32,
    last_save: f64,
    own_tried: f64,
    own_followed: Option<Instant>,
    reads: HashMap<(String, i128, u64, String), Vec<Find>>,
}

/// Everything that backlog reading and tailing need in common: paths, the
/// sources of the phrases and the caches.
pub struct LogSource<'p> {
    pub paths: &'p Paths,
    pub sources: Sources,
    clock: Clock,
    memo: Mutex<Memo>,
}

impl<'p> LogSource<'p> {
    pub fn new(paths: &'p Paths, sources: Sources) -> Self {
        Self::with_clock(paths, sources, Box::new(now_seconds))
    }

    pub fn with_clock(paths: &'p Paths, sources: Sources, clock: Clock) -> Self {
        LogSource { paths, sources, clock, memo: Mutex::new(Memo::default()) }
    }

    /// The phrase sources of these paths: app folder and language folder.
    pub fn sources_of(paths: &Paths) -> Sources {
        Sources {
            phrases_file: Some(paths.app_file("phrasen.json").into()),
            localization_folder: paths.localization_folder(None).map(Into::into),
        }
    }

    pub fn now(&self) -> f64 {
        (self.clock)()
    }

    fn memo(&self) -> MutexGuard<'_, Memo> {
        self.memo.lock().unwrap_or_else(|e| e.into_inner())
    }

    // -------------------------------------------------------- account

    /// The account a log belongs to, or `None` (no login yet,
    /// unreadable). Remembered per file and state; backups also across
    /// restarts in `konten-je-log.json`.
    pub fn account_of_file(&self, filename: &str) -> Option<String> {
        self.identity_of_file(filename).handle
    }

    /// Handle and account number of a log, remembered like [`account_of_file`](Self::account_of_file).
    /// An entry of `konten-je-log.json` without the number (written before it was read) is read again once.
    pub fn identity_of_file(&self, filename: &str) -> Identity {
        let key = stat(filename).map(|(ns, size)| (filename.to_string(), ns, size));
        {
            let mut memo = self.memo();
            if let Some(key) = &key
                && let Some(hit) = memo.accounts.get(key)
            {
                return hit.clone();
            }
            self.load_stored(&mut memo);
            if let Some(k) = &key
                && let Some(hit) = stored_hit(memo.stored.as_ref(), filename, k.1, k.2)
            {
                memo.accounts.insert(k.clone(), hit.clone());
                return hit;
            }
        }
        let found = logscan::identity_of_file(Path::new(filename));
        let Some(key) = key else { return found };
        let mut memo = self.memo();
        if memo.accounts.len() > ACCOUNT_CACHE_MAX {
            memo.accounts.clear();
        }
        memo.accounts.insert(key.clone(), found.clone());
        // Only persist backups: the running `Game.log` changes
        // constantly.
        if let Some(account) = found.handle.as_deref().filter(|a| !a.is_empty())
            && basename(filename).to_lowercase() != "game.log"
        {
            let entry = Value::Array(vec![
                Value::Int(key.1.to_string()),
                Value::Int(key.2.to_string()),
                Value::str(account),
                found.id.clone().map_or(Value::Null, Value::Str),
            ]);
            jsonio::map_set(memo.stored.get_or_insert_with(Vec::new), filename, entry);
            memo.unsaved += 1;
            // Write in batches; whatever is lost in between is read
            // again next time.
            if memo.unsaved >= SAVE_EVERY || self.now() - memo.last_save > SAVE_AFTER_SEC {
                self.save_accounts(&mut memo);
            }
        }
        found
    }

    /// Load the persistently remembered accounts, once per run. If the file
    /// is missing or broken, the store is empty.
    fn load_stored(&self, memo: &mut Memo) {
        if memo.stored.is_none() {
            memo.stored = Some(match jsonio::read_json(&self.paths.app_file(ACCOUNTS_FILE)) {
                Ok(Value::Object(map)) => map,
                _ => Vec::new(),
            });
        }
    }

    fn save_accounts(&self, memo: &mut Memo) {
        memo.unsaved = 0;
        memo.last_save = self.now();
        let target = self.paths.app_file(ACCOUNTS_FILE);
        let temp = format!("{target}.tmp");
        let text = jsonio::dumps(&Value::Object(memo.stored.clone().unwrap_or_default()), None, false);
        // Just a cache: a failure does no harm.
        let _ = jsonio::write_new(&temp, text.as_bytes()).and_then(|()| jsonio::replace_file(&temp, &target));
    }

    /// All backups and the running log.
    pub fn all_log_files(&self) -> Vec<String> {
        let mut files = self.paths.log_backups(None);
        files.extend(self.paths.game_log(None));
        files
    }

    /// Account → number of logs, in order of first appearance.
    pub fn accounts_in_logs(&self, files: Option<&[String]>) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (identity, _) in self.identities(files) {
            let Some(found) = identity.handle.filter(|a| !a.is_empty()) else { continue };
            match counts.iter_mut().find(|(a, _)| *a == found) {
                Some(entry) => entry.1 += 1,
                None => counts.push((found, 1)),
            }
        }
        counts
    }

    /// Every log with its identity and modification time, in the order of `files` (all logs if `None`).
    /// Newly found accounts are saved.
    fn identities(&self, files: Option<&[String]>) -> Vec<(Identity, f64)> {
        let all;
        let files = match files {
            Some(f) => f,
            None => {
                all = self.all_log_files();
                &all
            }
        };
        let found =
            files.iter().map(|f| (self.identity_of_file(f), mtime_seconds(f).unwrap_or(0.0))).collect::<Vec<_>>();
        let mut memo = self.memo();
        if memo.unsaved > 0 {
            self.load_stored(&mut memo);
            self.save_accounts(&mut memo);
        }
        found
    }

    /// The newest of the logs that `pick` takes something from (on equal times the later one in the list).
    fn newest<T>(found: &[(Identity, f64)], pick: impl Fn(&Identity) -> Option<T>) -> Option<T> {
        let mut best: Option<(f64, T)> = None;
        for (identity, at) in found {
            if let Some(value) = pick(identity)
                && best.as_ref().is_none_or(|(when, _)| *at >= *when)
            {
                best = Some((*at, value));
            }
        }
        best.map(|(_, value)| value)
    }

    /// The account number of the newest log of this handle that names one.
    pub fn account_id_of(&self, handle: &str) -> Option<String> {
        let found = self.identities(None);
        Self::newest(&found, |i| i.id.clone().filter(|_| i.handle.as_deref().is_some_and(|h| same_handle(h, handle))))
    }

    /// The user's own account: `None` if none can be identified yet,
    /// `ALL_ACCOUNTS` if all should count.
    pub fn own_account(&self, files: Option<&[String]>) -> Option<String> {
        self.own_identity(files).map(|own| own.handle)
    }

    /// The own account with its account number.
    ///
    /// ⚠ It is fixed the first time, not guessed anew each time: the
    /// account with the **most** logs. On a tie nothing is fixed and every log
    /// counts until the player chooses (a guess there would drop the
    /// blueprints of the other one without a word). After that it is stored
    /// in the settings, with the account number where a log names it.
    ///
    /// A stored account is compared with the logs now and then
    /// ([`OWN_FOLLOW_SEC`]): an account stored without its number (from 4.12)
    /// learns it from a log of the same handle, and a changed handle of the
    /// same number is taken over.
    pub fn own_identity(&self, files: Option<&[String]>) -> Option<Own> {
        let chosen = self.paths.setting(ACCOUNT_SETTING).unwrap_or_default();
        let chosen = py_strip(&chosen).to_string();
        if chosen == ALL_ACCOUNTS {
            return Some(Own { handle: chosen, id: None });
        }
        if !chosen.is_empty() {
            let id = self.paths.setting(ACCOUNT_ID_SETTING).filter(|id| id.bytes().all(|b| b.is_ascii_digit()));
            let own = Own { handle: chosen, id };
            {
                let mut memo = self.memo();
                if memo.own_followed.is_some_and(|at| at.elapsed().as_secs() < OWN_FOLLOW_SEC) {
                    return Some(own);
                }
                memo.own_followed = Some(Instant::now());
            }
            return Some(self.follow(own, files));
        }
        // ⚠ Tailing asks every few seconds; without a recognizable account
        // every query would otherwise touch all backups.
        {
            let mut memo = self.memo();
            if self.now() - memo.own_tried < OWN_RETRY_SEC {
                return None;
            }
            memo.own_tried = self.now();
        }
        self.learn(files)
    }

    /// Learns the account number of a stored account and follows a changed handle of that number.
    fn follow(&self, mut own: Own, files: Option<&[String]>) -> Own {
        let found = self.identities(files);
        if own.id.is_none()
            && let Some(id) = Self::newest(&found, |i| {
                i.id.clone().filter(|_| i.handle.as_deref().is_some_and(|h| same_handle(h, &own.handle)))
            })
        {
            self.paths.set_setting(ACCOUNT_ID_SETTING, Value::str(&id));
            own.id = Some(id);
        }
        if let Some(id) = own.id.clone()
            && let Some(handle) =
                Self::newest(&found, |i| i.handle.clone().filter(|h| !h.is_empty() && i.id.as_ref() == Some(&id)))
            && !same_handle(&handle, &own.handle)
        {
            self.paths.set_setting(ACCOUNT_SETTING, Value::str(&handle));
            own.handle = handle;
        }
        own
    }

    /// Fixes the own account the first time: the one with the most logs, counted by account number (a
    /// changed handle is the same account), where a log has none by handle. Nothing on a tie.
    fn learn(&self, files: Option<&[String]>) -> Option<Own> {
        let found = self.identities(files);
        // Account number (or none), handles, number of logs.
        let mut groups: Vec<(Option<String>, Vec<String>, usize)> = Vec::new();
        for (identity, _) in &found {
            let Some(handle) = identity.handle.as_deref().filter(|h| !h.is_empty()) else { continue };
            let group = match &identity.id {
                Some(id) => groups.iter_mut().find(|(i, _, _)| i.as_ref() == Some(id)),
                None => groups.iter_mut().find(|(i, handles, _)| i.is_none() && handles[0] == handle),
            };
            match group {
                Some(group) => {
                    group.2 += 1;
                    if !group.1.iter().any(|h| h == handle) {
                        group.1.push(handle.to_string());
                    }
                }
                None => groups.push((identity.id.clone(), vec![handle.to_string()], 1)),
            }
        }
        // Logs without the number go to the one account that has their handle.
        let mut index = 0;
        while index < groups.len() {
            let (id, handles, count) = &groups[index];
            let homes: Vec<usize> = (0..groups.len())
                .filter(|&j| groups[j].0.is_some() && groups[j].1.iter().any(|h| same_handle(h, &handles[0])))
                .collect();
            if id.is_none() && homes.len() == 1 {
                let count = *count;
                groups[homes[0]].2 += count;
                groups.remove(index);
            } else {
                index += 1;
            }
        }
        let most = groups.iter().map(|(_, _, n)| *n).max()?;
        let mut best = groups.into_iter().filter(|(_, _, n)| *n == most);
        let (id, handles, _) = best.next()?;
        if best.next().is_some() {
            return None;
        }
        let handle = Self::newest(&found, |i| {
            let handle = i.handle.clone().filter(|h| !h.is_empty())?;
            let mine = match &id {
                Some(id) => i.id.as_ref() == Some(id),
                None => handles.contains(&handle),
            };
            mine.then_some(handle)
        })?;
        self.paths.set_setting(ACCOUNT_SETTING, Value::str(&handle));
        if let Some(id) = &id {
            self.paths.set_setting(ACCOUNT_ID_SETTING, Value::str(id));
        }
        Some(Own { handle, id })
    }

    /// Blueprints that appear ONLY in logs of other accounts, sorted by name.
    /// Basis for cleanup. Removal happens only after confirmation, because a
    /// blueprint looks foreign here if the own log containing the find has
    /// already been cleared away.
    pub fn foreign_only_blueprints(&self, pattern: &Regex, own: Option<&str>) -> Vec<String> {
        let own = match own.filter(|o| !o.is_empty()) {
            // The stored number belongs to the handle given only if it is the stored handle.
            Some(handle) => Some(Own {
                handle: handle.to_string(),
                id: self
                    .paths
                    .setting(ACCOUNT_SETTING)
                    .filter(|stored| same_handle(py_strip(stored), handle))
                    .and_then(|_| self.paths.setting(ACCOUNT_ID_SETTING)),
            }),
            None => self.own_identity(None),
        };
        let Some(own) = own.filter(|o| !o.handle.is_empty() && o.handle != ALL_ACCOUNTS) else { return Vec::new() };
        let mut mine = HashSet::new();
        let mut foreign: Vec<(String, String)> = Vec::new();
        let mut foreign_keys = HashSet::new();
        for filename in self.all_log_files() {
            let identity = self.identity_of_file(&filename);
            for (name, _extra) in self.read_file(&filename, pattern) {
                let key = name_key(&name);
                if own.counts(&identity) {
                    mine.insert(key);
                } else if foreign_keys.insert(key.clone()) {
                    foreign.push((key, name));
                }
            }
        }
        let mut out: Vec<String> = foreign.into_iter().filter(|(k, _)| !mine.contains(k)).map(|(_, n)| n).collect();
        out.sort_by_cached_key(|n| n.to_lowercase());
        out
    }

    // -------------------------------------------------------- reading

    /// Search a whole log file, cached per file, state and search
    /// pattern. If it grows, the state changes and it is read in full
    /// again.
    pub fn read_file(&self, filename: &str, pattern: &Regex) -> Vec<Find> {
        let key = stat(filename).map(|(ns, size)| (filename.to_string(), ns, size, pattern.as_str().to_string()));
        if let Some(key) = &key
            && let Some(hit) = self.memo().reads.get(key)
        {
            return hit.clone();
        }
        let found = logscan::read_file(Path::new(filename), pattern, &self.sources);
        if let Some(key) = key {
            let mut memo = self.memo();
            if memo.reads.len() > READ_CACHE_MAX {
                memo.reads.clear();
            }
            memo.reads.insert(key, found.clone());
        }
        found
    }

    /// Go through the preserved logs.
    ///
    /// Returns the finds (name, suffix), in the same form as when tailing,
    /// and the report. At the end the read position is always saved.
    pub fn read_backlog(
        &self,
        state: &mut ReadState,
        pattern: &Regex,
        only_new: bool,
        incl_running: bool,
    ) -> (Vec<Find>, Report) {
        let all_names = self.paths.log_backups(None);
        // Capture the comparison value BEFORE the run, because `remember` updates it.
        let before = state.get("letzte_sitzung").cloned().unwrap_or(Value::Float(0.0));
        let mut report = Report { available: all_names.len(), ..Report::default() };
        let active = if incl_running { self.paths.game_log(None) } else { None };
        let mut files = all_names.clone();
        files.extend(active.clone());
        // Once: backups remembered with another handle than the one stored then are looked at again by account
        // number (a changed handle made them foreign before the number was read).
        let recheck = !self.paths.setting_bool(ACCOUNTS_RECHECKED, false);
        let stored_handle = self.paths.setting(ACCOUNT_SETTING).filter(|h| h != ALL_ACCOUNTS);
        let own = self.own_identity(Some(&files));
        let tallied = tallied_files(self.paths);
        let mut tally = TallyChanges::default();

        let mut found: Vec<Find> = Vec::new();
        let mut seen = HashSet::new();
        let mut take = |finds: Vec<Find>, found: &mut Vec<Find>| {
            for (name, extra) in finds {
                if seen.insert(name_key(&name)) {
                    found.push((name, extra));
                }
            }
        };
        for filename in &all_names {
            let base = basename(filename);
            if only_new && state.knows(filename) {
                // A log that did not count is looked at again: with the own account chosen anew, a number
                // learned or a handle followed it may count now, and its blueprints come after all.
                let was_foreign = tallied.contains(&base)
                    || (recheck
                        && stored_handle.as_deref().is_some_and(|handle| {
                            state.account_of(filename).is_some_and(|a| !counts_for(Some(handle), Some(&a)))
                        }));
                if !was_foreign || !counts_own(own.as_ref(), &self.identity_of_file(filename)) {
                    report.skipped += 1;
                    if was_foreign && !tallied.contains(&base) {
                        tally.add(filename, &self.read_file(filename, pattern));
                    }
                    continue;
                }
            }
            // Foreign logs do not count but are remembered, otherwise
            // they would be touched again on every start. Their blueprints are tallied.
            let identity = self.identity_of_file(filename);
            let account = identity.handle.clone();
            if !counts_own(own.as_ref(), &identity) {
                report.foreign += 1;
                tally.add(filename, &self.read_file(filename, pattern));
                state.remember(filename, account.as_deref());
                continue;
            }
            take(self.read_file(filename, pattern), &mut found);
            if tallied.contains(&base) {
                tally.drop.push(base);
            }
            state.remember(filename, account.as_deref());
            report.files += 1;
        }

        // ⚠ The running Game.log is **always** read in full, not just the
        // very first time: if the program did not run in between, the read
        // position would sit in the middle of the file and everything before
        // it would be lost.
        if let Some(active) = &active {
            if !counts_own(own.as_ref(), &self.identity_of_file(active)) {
                report.foreign += 1;
                tally.add(active, &self.read_file(active, pattern));
            } else {
                take(self.read_file(active, pattern), &mut found);
                report.running = true;
                let base = basename(active);
                if tallied.contains(&base) {
                    tally.drop.push(base);
                }
            }
            if let Some(size) = file_size(active) {
                state.set_active(active, i128::from(size), &*self.clock);
            }
        }

        report.found = found.len();
        report.gap = check_gap(&before, &all_names);
        state.cleanup(&all_names);
        state.save();
        tally.apply(self.paths);
        if recheck && own.is_some() {
            self.paths.set_setting(ACCOUNTS_RECHECKED, Value::Bool(true));
        }
        (found, report)
    }

    /// Read all logs once more, including the already known ones; the
    /// read position is bypassed and rewritten afterwards.
    ///
    /// Fails before anything is read when no game folder is found, or when the
    /// folder of the preserved logs or one of the logs cannot be opened.
    pub fn read_all(&self, pattern: &Regex) -> Result<(Vec<Find>, Report), ReadAllError> {
        self.all_logs_readable()?;
        let mut state = ReadState::open(self.paths);
        Ok(self.read_backlog(&mut state, pattern, false, true))
    }

    /// Opens the game folder's `logbackups`, every preserved log and the running `Game.log` once.
    fn all_logs_readable(&self) -> Result<(), ReadAllError> {
        let folder = self.paths.game_folder().ok_or(ReadAllError::NoGameFolder)?;
        let unreadable = |path: &str, e: io::Error| ReadAllError::Unreadable { path: path.to_string(), reason: e.to_string() };
        let backups = self.paths.host.flavor.join(&folder, "logbackups");
        if Path::new(&backups).is_dir() {
            fs::read_dir(&backups).map_err(|e| unreadable(&backups, e))?;
        }
        let mut logs = self.paths.log_backups(Some(&folder));
        logs.extend(self.paths.game_log(Some(&folder)));
        for log in &logs {
            File::open(log).map_err(|e| unreadable(log, e))?;
        }
        Ok(())
    }
}

/// A hit in the persistently remembered accounts: `[mtime_ns, size,
/// account, account number]` with a matching state. An entry with an account
/// but without the fourth field was written before the number was read: no hit,
/// the log is read again.
fn stored_hit(stored: Option<&Map>, filename: &str, ns: i128, size: u64) -> Option<Identity> {
    let (_, Value::Array(hit)) = stored?.iter().find(|(k, _)| k == filename)? else { return None };
    if hit.len() < 3 || !num_eq(hit.first(), ns) || !num_eq(hit.get(1), i128::from(size)) {
        return None;
    }
    let id = match hit.get(3) {
        Some(Value::Str(id)) => Some(id.clone()),
        Some(Value::Null) => None,
        _ => return matches!(hit[2], Value::Null).then(Identity::default),
    };
    match &hit[2] {
        Value::Str(account) => Some(Identity { handle: Some(account.clone()), id }),
        Value::Null => Some(Identity::default()),
        _ => None,
    }
}

/// Does something remain unknown despite backlog reading? Very first run, or
/// the oldest backup is newer than the most recently read session.
fn check_gap(before: &Value, all_names: &[String]) -> Option<Gap> {
    if all_names.is_empty() {
        return Some(Gap::NoLogs);
    }
    let oldest = all_names.iter().filter_map(|p| mtime_seconds(p)).reduce(f64::min).unwrap_or(0.0);
    if !truthy(before) {
        return Some(Gap::FirstRun { oldest });
    }
    // If the remembered value is not a number, no message is produced.
    let last = number(before)?;
    (oldest > last + 60.0).then_some(Gap::Missing { last, oldest })
}

// ------------------------------------------------------------ tailing

/// What the contract module reads from each new section of the Game.log.
///
/// ⚠ Gap: contracts, endings, agreements and intermediate objectives (`contracts`)
/// have not been ported yet. A section follows the order of the log; anyone
/// evaluating acceptances and endings must keep them in that order.
pub trait ContractScan {
    /// Before each section, even if nothing new arrives: the results
    /// belong to the section and must not be read twice.
    fn clear(&mut self);
    /// The new section, up to the last complete line.
    fn scan(&mut self, text: &str);
}

/// Keeps reading the running Game.log continuously. The read position survives
/// a program restart.
pub struct LogTail<'s, 'p> {
    pub src: &'s LogSource<'p>,
    pub state: ReadState,
    pub pattern: Regex,
    pub path: Option<String>,
    pub offset: i128,
    /// Whom the running log belongs to: `None` as long as nobody has logged in.
    pub account: Option<String>,
    /// The account number of the running log, once the log names it.
    pub account_id: Option<String>,
    /// The text of the last section, only with a contract module.
    pub last_text: String,
    /// The last reading took a whole block and more may follow: the next one should not wait.
    pub behind: bool,
    pub contracts: Option<Box<dyn ContractScan + 's>>,
}

impl<'s, 'p> LogTail<'s, 'p> {
    pub fn new(src: &'s LogSource<'p>, state: ReadState, pattern: Regex) -> Self {
        LogTail {
            src,
            state,
            pattern,
            path: None,
            offset: 0,
            account: None,
            account_id: None,
            last_text: String::new(),
            behind: false,
            contracts: None,
        }
    }

    fn locate(&mut self) -> Option<String> {
        match self.src.paths.game_log(None) {
            Some(p) if self.path.as_deref() != Some(p.as_str()) => {
                let identity = self.src.identity_of_file(&p);
                self.account = identity.handle;
                self.account_id = identity.id;
                let remembered = self.state.get_active(&p);
                let size = file_size(&p).map_or(0, i128::from);
                // ⚠ Three cases:
                //   never read       → to the end; the backlog reading already read it from the start
                //   remembered > size → fresh Game.log after a game restart → from the start
                //   otherwise        → continue where it left off
                // In the middle case it must be 0, not the size, otherwise
                // the blueprints of the fresh session would be lost.
                self.offset = match remembered {
                    None => size,
                    Some(r) if r > size => 0,
                    Some(r) => r,
                };
                self.path = Some(p);
            }
            None => self.path = None,
            Some(_) => {}
        }
        self.path.clone()
    }

    /// New blueprints since the last call: list of (name, suffix).
    ///
    /// ⚠ At most one block ([`logread::BLOCK`]) of the log is read per call, so a log that grew by gigabytes
    /// while nobody read it is never in memory whole. A section is then one block of whole lines; while
    /// [`behind`](Self::behind) is set, the next call goes on from there.
    pub fn new_names(&mut self) -> Vec<Find> {
        // ⚠ Clear first, before any early return.
        if let Some(contracts) = &mut self.contracts {
            contracts.clear();
        }
        self.last_text.clear();
        self.behind = false;
        let Some(path) = self.locate() else { return Vec::new() };
        let Some(size) = file_size(&path).map(i128::from) else { return Vec::new() };
        if size < self.offset {
            // Log rotated → new game session, new login.
            self.offset = 0;
            self.account = None;
            self.account_id = None;
        }
        if size == self.offset {
            return Vec::new();
        }
        let Ok(chunk) = read_from(&path, self.offset) else { return Vec::new() };
        // The partially written last line is left for the next call. A line that fills the whole block is
        // handed over in parts.
        let (cut, skip) = match chunk.iter().rposition(|&b| b == b'\n') {
            Some(cut) => (cut, cut + 1),
            None if chunk.len() >= logread::BLOCK => (chunk.len(), chunk.len()),
            None => return Vec::new(),
        };
        self.behind = chunk.len() >= logread::BLOCK;
        self.offset += skip as i128;
        self.state.set_active(&path, self.offset, &*self.src.clock);
        self.state.save();
        let text = decode_ignore(&chunk[..cut]);
        if let Some(contracts) = &mut self.contracts {
            self.last_text.clone_from(&text);
            contracts.scan(&text);
        }
        if self.account.is_none() {
            self.account = account_from_text(&text);
        }
        if self.account_id.is_none() {
            self.account_id = account_id_from_text(&text);
        }
        // Only the blueprints of a foreign account are suppressed, not the
        // session's contracts. They are tallied.
        let identity = Identity { handle: self.account.clone(), id: self.account_id.clone() };
        if !counts_own(self.src.own_identity(None).as_ref(), &identity) {
            let names = names_from_text(&text, &self.pattern, &self.src.sources);
            if !names.is_empty() {
                let mut tally = TallyChanges::default();
                tally.add(&path, &names);
                tally.apply(self.src.paths);
            }
            return Vec::new();
        }
        names_from_text(&text, &self.pattern, &self.src.sources)
    }
}

/// The bytes from `offset` to the end of the file, at most one block.
fn read_from(path: &str, offset: i128) -> io::Result<Vec<u8>> {
    let start = u64::try_from(offset).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut chunk = Vec::new();
    file.take(logread::BLOCK as u64).read_to_end(&mut chunk)?;
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::legacy;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::phrases::pattern;

    /// A throwaway folder with a data folder and a game folder including `logbackups`.
    struct Lab {
        root: String,
        home: String,
        game: String,
    }

    impl Lab {
        fn new(name: &str) -> Lab {
            let f = Flavor::native();
            let base = std::env::temp_dir().to_string_lossy().into_owned();
            let root = f.join(&base, &format!("versekit-logsource-{}-{name}", crate::testdir::run_tag()));
            let _ = fs::remove_dir_all(&root);
            let home = f.join(&root, "home");
            let game = f.join_all(&root, &["StarCitizen", "LIVE"]);
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(f.join(&game, "logbackups")).unwrap();
            fs::write(f.join(&game, "Game.log"), "").unwrap();
            Lab { root, home, game }
        }

        fn paths(&self) -> Paths {
            Paths::new(Host::new(
                Flavor::native(),
                &[(legacy::ENV_HOME, self.home.as_str()), (ENV_INSTALL_DIR, self.game.as_str())],
                &self.root,
            ))
        }

        fn game_log(&self) -> String {
            Flavor::native().join(&self.game, "Game.log")
        }

        fn backup(&self, name: &str, text: &str) -> String {
            let p = Flavor::native().join_all(&self.game, &["logbackups", name]);
            fs::write(&p, text).unwrap();
            p
        }

        fn append(&self, text: &str) {
            use std::io::Write;
            let mut f = fs::OpenOptions::new().append(true).open(self.game_log()).unwrap();
            f.write_all(text.as_bytes()).unwrap();
        }
    }

    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// A clock that the test sets.
    fn clock() -> (Arc<AtomicU64>, Clock) {
        let now = Arc::new(AtomicU64::new(1_000_000f64.to_bits()));
        let shared = Arc::clone(&now);
        (now, Box::new(move || f64::from_bits(shared.load(Ordering::SeqCst))))
    }

    fn set(now: &AtomicU64, t: f64) {
        now.store(t.to_bits(), Ordering::SeqCst);
    }

    fn login(account: &str) -> String {
        format!("<Legacy login response> [CIG-net] User Login Success - Handle[{account}] - Time\n")
    }

    fn bp(name: &str) -> String {
        format!("<SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: {name}: \" [1]\n")
    }

    fn rx() -> Regex {
        pattern(&["Bauplan erhalten".into()])
    }

    fn names(finds: &[Find]) -> Vec<&str> {
        finds.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn sources_of_reads_phrases_like_app_file() {
        let lab = Lab::new("phrasen");
        let f = Flavor::native();
        let appdata = f.join(&lab.root, "appdata");
        let folder = f.join_all(&lab.home, &["Documents", "Verse-Kit"]);
        fs::create_dir_all(f.join(&folder, "Einstellungen")).unwrap();
        let nested = f.join_all(&folder, &["Einstellungen", "phrasen.json"]);
        fs::write(&nested, r#"{"phrasen": ["Eigene Formulierung"]}"#).unwrap();
        fs::write(f.join(&folder, "phrasen.json"), r#"{"phrasen": ["Flache Datei"]}"#).unwrap();
        // Without a custom data folder: subfolder `Einstellungen`.
        let p = Paths::new(Host::new(
            f,
            &[
                ("USERPROFILE", lab.home.as_str()),
                ("HOME", lab.home.as_str()),
                ("APPDATA", appdata.as_str()),
                (ENV_INSTALL_DIR, lab.game.as_str()),
            ],
            &lab.root,
        ));
        let sources = LogSource::sources_of(&p);
        assert_eq!(sources.phrases_file.as_deref(), Some(Path::new(&nested)));
        assert_eq!(sources.own(), vec!["Eigene Formulierung"]);
        // With a custom data folder: flat inside it.
        let flat = LogSource::sources_of(&lab.paths());
        assert_eq!(flat.phrases_file.as_deref(), Some(Path::new(&f.join(&lab.home, "phrasen.json"))));
        assert!(flat.own().is_empty());
    }

    #[test]
    fn counts_for_rules() {
        assert!(counts_for(None, Some("a")));
        assert!(counts_for(Some(""), Some("a")));
        assert!(counts_for(Some("*"), Some("a")));
        assert!(counts_for(Some("a"), None));
        assert!(counts_for(Some("a"), Some("")));
        assert!(counts_for(Some("Spieler"), Some("SPIELER")));
        assert!(!counts_for(Some("Spieler"), Some("Zweit")));
    }

    #[test]
    fn max_keeps_stored_on_tie() {
        assert_eq!(py_max(Some(&Value::Int("5".into())), 5.0), Value::Int("5".into()));
        assert_eq!(py_max(Some(&Value::Int("5".into())), 5.5), Value::Float(5.5));
        assert_eq!(py_max(Some(&Value::Float(9.0)), 5.5), Value::Float(9.0));
        assert_eq!(py_max(None, 3.0), Value::Float(3.0));
    }

    #[test]
    fn read_state_defaults_and_order() {
        let lab = Lab::new("state");
        let path = Flavor::native().join(&lab.home, READ_STATE_FILE);
        assert_eq!(ReadState::load(path.clone()).data, ReadState::empty());
        fs::write(&path, "[1, 2]").unwrap();
        assert_eq!(ReadState::load(path.clone()).data, ReadState::empty());
        fs::write(&path, "\u{feff}{}").unwrap();
        assert_eq!(ReadState::load(path.clone()).data, ReadState::empty());
        // Unknown fields stay, missing ones are appended at the end.
        fs::write(&path, r#"{"x": 1, "sicherungen": {}}"#).unwrap();
        let state = ReadState::load(path.clone());
        let keys: Vec<&str> = state.data.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["x", "sicherungen", "aktiv", "letzte_sitzung"]);
        state.save();
        let written = fs::read_to_string(&path).unwrap();
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        assert!(written.starts_with(&format!("{{{nl} \"x\": 1,{nl} \"sicherungen\": {{}},")), "{written:?}");
        assert!(!Path::new(&format!("{path}.tmp")).exists());
    }

    #[test]
    fn read_state_knows_remember_cleanup() {
        let lab = Lab::new("knows");
        let a = lab.backup("a.log", "eins\n");
        let b = lab.backup("b.log", "zwei\n");
        let mut state = ReadState::load(Flavor::native().join(&lab.home, READ_STATE_FILE));
        assert!(!state.knows(&a));
        state.remember(&a, Some("Spieler"));
        state.remember(&b, Some(""));
        assert!(state.knows(&a) && state.knows(&b));
        let entries = obj(&state.data, "sicherungen").unwrap();
        assert_eq!(entries[0].1.get("account"), Some(&Value::str("Spieler")));
        assert_eq!(entries[1].1.get("account"), None);
        let last = number(state.get("letzte_sitzung").unwrap()).unwrap();
        assert_eq!(last, mtime_seconds(&a).unwrap().max(mtime_seconds(&b).unwrap()));
        // If the file grows, it counts as unread again.
        fs::write(&a, "eins\nmehr\n").unwrap();
        assert!(!state.knows(&a));
        state.cleanup(std::slice::from_ref(&b));
        assert_eq!(obj(&state.data, "sicherungen").unwrap().len(), 1);
    }

    #[test]
    fn get_active_cases() {
        let mut state = ReadState { path: String::new(), data: ReadState::empty() };
        assert_eq!(state.get_active("x"), None);
        state.set_active("x", 42, &|| 7.0);
        assert_eq!(state.get_active("x"), Some(42));
        assert_eq!(state.get_active("y"), None);
        assert_eq!(state.get("letzte_sitzung"), Some(&Value::Float(7.0)));
        jsonio::map_set(obj_mut(&mut state.data, "aktiv"), "offset", Value::Null);
        assert_eq!(state.get_active("x"), None);
        obj_mut(&mut state.data, "aktiv").retain(|(k, _)| k != "offset");
        assert_eq!(state.get_active("x"), Some(0));
    }

    #[test]
    fn gap_cases() {
        let lab = Lab::new("gap");
        assert_eq!(check_gap(&Value::Float(5.0), &[]), Some(Gap::NoLogs));
        let a = lab.backup("a.log", "x\n");
        let oldest = mtime_seconds(&a).unwrap();
        let all = [a];
        assert_eq!(check_gap(&Value::Float(0.0), &all), Some(Gap::FirstRun { oldest }));
        assert_eq!(check_gap(&Value::Int("0".into()), &all), Some(Gap::FirstRun { oldest }));
        assert_eq!(check_gap(&Value::Float(oldest - 61.0), &all), Some(Gap::Missing { last: oldest - 61.0, oldest }));
        assert_eq!(check_gap(&Value::Float(oldest - 59.0), &all), None);
        assert_eq!(check_gap(&Value::str("kaputt"), &all), None);
        assert_eq!(Gap::Missing { last: 1.0, oldest: 2.0 }.moments(), [1.0, 2.0]);
        assert_eq!(Gap::FirstRun { oldest: 2.0 }.key(), KEY_FIRST_RUN);
    }

    #[test]
    fn accounts_are_stored_but_not_for_game_log() {
        let lab = Lab::new("konten");
        let a = lab.backup("a.log", &login("Spieler"));
        let b = lab.backup("b.log", &login("Zweit"));
        let empty = lab.backup("c.log", "kein Login\n");
        fs::write(lab.game_log(), login("Spieler")).unwrap();
        let paths = lab.paths();
        let (now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        assert_eq!(src.accounts_in_logs(None), [("Spieler".to_string(), 2), ("Zweit".to_string(), 1)]);
        let stored = jsonio::read_json(&paths.app_file(ACCOUNTS_FILE)).unwrap();
        let keys: Vec<&str> = stored.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, [a.as_str(), b.as_str()]);
        assert_eq!(src.account_of_file(&empty), None);

        // A fresh run takes the remembered account without reading the file.
        let mut map = stored.as_object().unwrap().clone();
        let Value::Array(entry) = &mut map[0].1 else { panic!() };
        entry[2] = Value::str("Gemerkt");
        fs::write(paths.app_file(ACCOUNTS_FILE), jsonio::dumps(&Value::Object(map), None, false)).unwrap();
        let src = LogSource::with_clock(
            &paths,
            Sources::default(),
            Box::new(move || f64::from_bits(now.load(Ordering::SeqCst))),
        );
        assert_eq!(src.account_of_file(&a).as_deref(), Some("Gemerkt"));
        // If the file changes, the entry no longer applies.
        fs::write(&a, format!("{}mehr\n", login("Spieler"))).unwrap();
        assert_eq!(src.account_of_file(&a).as_deref(), Some("Spieler"));
    }

    #[test]
    fn accounts_saved_in_batches() {
        let lab = Lab::new("buendel");
        let files: Vec<String> = (0..3).map(|i| lab.backup(&format!("{i}.log"), &login("Spieler"))).collect();
        let paths = lab.paths();
        let (now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        let target = paths.app_file(ACCOUNTS_FILE);
        // The first new entry is written immediately (last save "never").
        src.account_of_file(&files[0]);
        assert_eq!(jsonio::read_json(&target).unwrap().as_object().unwrap().len(), 1);
        // Within five seconds entries are collected …
        set(&now, 1_000_004.0);
        src.account_of_file(&files[1]);
        assert_eq!(jsonio::read_json(&target).unwrap().as_object().unwrap().len(), 1);
        // … after that they are written.
        set(&now, 1_000_006.0);
        src.account_of_file(&files[2]);
        assert_eq!(jsonio::read_json(&target).unwrap().as_object().unwrap().len(), 3);
    }

    /// The line of the logged-in character with a made-up account number.
    fn character(account: &str, id: u32) -> String {
        format!(
            "<AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 3 \
             - accountId {id} - name {account} - state STATE_CURRENT\n"
        )
    }

    /// A log of `account` with account number `id` that received the blueprints `finds`.
    fn session(account: &str, id: u32, finds: &[&str]) -> String {
        let mut text = format!("<2026-10-01T12:00:00.000Z> Log started\n{}{}", login(account), character(account, id));
        finds.iter().for_each(|f| text.push_str(&bp(f)));
        text
    }

    /// Sets the modification time `seconds` after a fixed point.
    fn stamp(path: &str, seconds: u64) {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000 + seconds);
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(at)).unwrap();
    }

    #[test]
    fn own_account_most_logs_then_throttle() {
        let lab = Lab::new("eigen");
        lab.backup("a.log", &login("Zweit"));
        lab.backup("b.log", &login("Beta"));
        let paths = lab.paths();
        let (now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        // Tie: nothing is fixed, every log counts until the player chooses.
        assert_eq!(src.own_account(None), None);
        assert_eq!(paths.setting(ACCOUNT_SETTING), None);
        // One more log decides, at the next search.
        lab.backup("c.log", &login("Zweit"));
        set(&now, 1_000_301.0);
        assert_eq!(src.own_account(None).as_deref(), Some("Zweit"));
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some("Zweit"));

        let lab = Lab::new("drossel");
        let paths = lab.paths();
        let (now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        assert_eq!(src.own_account(None), None);
        fs::write(lab.game_log(), login("Spieler")).unwrap();
        // The search runs at most every five minutes.
        set(&now, 1_000_299.0);
        assert_eq!(src.own_account(None), None);
        set(&now, 1_000_301.0);
        assert_eq!(src.own_account(None).as_deref(), Some("Spieler"));
    }

    #[test]
    fn backlog_first_run_then_known() {
        let lab = Lab::new("nachlese");
        lab.backup("a.log", &format!("{}{}{}", login("Spieler"), bp("Helm"), bp("Attrition-5 (Mil/1/A)")));
        lab.backup("b.log", &format!("{}{}{}", login("Zweit"), bp("Fremd"), bp("helm")));
        fs::write(lab.game_log(), format!("{}{}{}", login("Spieler"), bp("Laufend"), bp("HELM "))).unwrap();
        let paths = lab.paths();
        let (_now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        let mut state = ReadState::open(&paths);
        let (finds, report) = src.read_backlog(&mut state, &rx(), true, true);
        assert_eq!(names(&finds), ["Helm", "Attrition-5", "Laufend"]);
        assert_eq!(finds[1].1, Some(("Mil".into(), Some("1".into()), Some("A".into()))));
        assert_eq!((report.files, report.foreign, report.skipped, report.found), (1, 1, 0, 3));
        assert!(report.running);
        assert!(matches!(report.gap, Some(Gap::FirstRun { .. })));
        assert_eq!(state.get_active(&lab.game_log()), file_size(&lab.game_log()).map(i128::from));

        // Second run: everything known, no gap.
        let mut state = ReadState::open(&paths);
        let (finds, report) = src.read_backlog(&mut state, &rx(), true, true);
        assert_eq!(names(&finds), ["Laufend", "HELM"]);
        assert_eq!((report.files, report.skipped, report.gap), (0, 2, None));
        // Everything once more, and the foreign blueprint appears only under Zweit.
        let (finds, _) = src.read_all(&rx()).expect("every log can be read");
        assert_eq!(names(&finds), ["Helm", "Attrition-5", "Laufend"]);
        assert_eq!(src.foreign_only_blueprints(&rx(), None), ["Fremd"]);
        assert!(src.foreign_only_blueprints(&rx(), Some(ALL_ACCOUNTS)).is_empty());
    }

    #[test]
    fn reading_all_logs_again_fails_without_a_game_folder_and_reads_nothing() {
        let lab = Lab::new("ohne-ordner");
        lab.backup("a.log", &format!("{}{}", login("Spieler"), bp("Helm")));
        let gone = Flavor::native().join(&lab.root, "fehlt");
        let paths = Paths::new(Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, lab.home.as_str()), (ENV_INSTALL_DIR, gone.as_str())],
            &lab.root,
        ));
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        assert_eq!(src.read_all(&rx()), Err(ReadAllError::NoGameFolder));
        assert!(!Path::new(&paths.app_file(READ_STATE_FILE)).exists(), "no read position is written");
        // The same logs with the folder found: they are read.
        let (finds, report) = LogSource::with_clock(&lab.paths(), Sources::default(), clock().1)
            .read_all(&rx())
            .expect("the folder is there");
        assert_eq!((names(&finds), report.files), (vec!["Helm"], 1));
    }

    /// A log that another program holds without sharing cannot be opened: the reading says so before it starts.
    #[cfg(windows)]
    #[test]
    fn reading_all_logs_again_fails_when_a_log_cannot_be_opened() {
        use std::os::windows::fs::OpenOptionsExt;
        let lab = Lab::new("gesperrt");
        let locked = lab.backup("a.log", &format!("{}{}", login("Spieler"), bp("Helm")));
        let paths = lab.paths();
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let held = fs::OpenOptions::new().read(true).share_mode(0).open(&locked).unwrap();
        match src.read_all(&rx()) {
            Err(ReadAllError::Unreadable { path, .. }) => assert_eq!(path, locked),
            other => panic!("the locked log is told: {other:?}"),
        }
        drop(held);
        assert!(src.read_all(&rx()).is_ok(), "free again: it is read");
    }

    #[test]
    fn tail_follows_restarts_and_accounts() {
        let lab = Lab::new("mitlesen");
        fs::write(lab.game_log(), format!("{}{}", login("Spieler"), bp("Vorher"))).unwrap();
        let paths = lab.paths();
        let (_now, c) = clock();
        let src = LogSource::with_clock(&paths, Sources::default(), c);
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        // Never read: to the end; what comes before belongs to backlog reading.
        assert!(tail.new_names().is_empty());
        lab.append(&bp("Eins"));
        lab.append("<SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Halb");
        assert_eq!(names(&tail.new_names()), ["Eins"]);
        assert!(tail.new_names().is_empty());
        lab.append(": \" [1]\n");
        assert_eq!(names(&tail.new_names()), ["Halb"]);
        assert_eq!(tail.account.as_deref(), Some("Spieler"));

        // Program restart: continue where it left off.
        lab.append(&bp("Zwei"));
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        assert_eq!(names(&tail.new_names()), ["Zwei"]);

        // Game restart: shorter file, from the start, new login.
        fs::write(lab.game_log(), format!("{}{}", login("Zweit"), bp("Fremd"))).unwrap();
        assert!(tail.new_names().is_empty());
        assert_eq!(tail.account.as_deref(), Some("Zweit"));
        paths.set_setting(ACCOUNT_SETTING, Value::str(ALL_ACCOUNTS));
        lab.append(&bp("Alle"));
        assert_eq!(names(&tail.new_names()), ["Alle"]);

        // A new run whose remembered position lies beyond the end of the file: from the start.
        fs::write(lab.game_log(), bp("Kurz")).unwrap();
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        assert_eq!(names(&tail.new_names()), ["Kurz"]);
    }

    #[test]
    fn a_log_that_grew_by_many_blocks_is_read_a_block_at_a_time_and_finds_what_the_whole_text_has() {
        struct Sections(Arc<Mutex<Vec<usize>>>);
        impl ContractScan for Sections {
            fn clear(&mut self) {}
            fn scan(&mut self, text: &str) {
                self.0.lock().unwrap().push(text.len());
            }
        }
        let lab = Lab::new("viele-bloecke");
        let paths = lab.paths();
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let flood = "<2026-11-05T22:30:00.123Z> [Notice] <Monitored Space> \"Überwachter Raum ausgefallen: \" [4711]\n";
        let mut text = login("Spieler");
        for round in 0..10 {
            text.push_str(&bp(&format!("Teil {round}")));
            text.push_str(&flood.repeat(logread::BLOCK / 3 / flood.len()));
        }
        // The line at the very end has no end yet.
        text.push_str("<SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Offen");
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        let sections = Arc::new(Mutex::new(Vec::new()));
        tail.contracts = Some(Box::new(Sections(Arc::clone(&sections))));
        assert!(tail.new_names().is_empty());
        fs::write(lab.game_log(), &text).unwrap();
        // The tail started at the end of the empty log: everything is new.
        let mut found = Vec::new();
        let mut calls = 0;
        loop {
            found.extend(tail.new_names());
            calls += 1;
            assert!(calls < 50);
            if !tail.behind {
                break;
            }
        }
        let whole = names_from_text(&text, &rx(), &Sources::default());
        assert_eq!(found, whole, "the same names as the whole text gives, in the same order");
        assert_eq!(whole.len(), 10);
        assert!(calls >= 3, "{calls}");
        assert!(sections.lock().unwrap().iter().all(|len| *len <= logread::BLOCK), "{:?}", sections.lock().unwrap());
        // Only the unended line is left, and it is not read before it ends.
        assert!(tail.offset < text.len() as i128);
        lab.append(": \" [1]\n");
        assert_eq!(names(&tail.new_names()), ["Offen"]);
        assert!(!tail.behind);
    }

    #[test]
    fn a_line_longer_than_a_block_does_not_make_the_tail_wait_for_ever() {
        let lab = Lab::new("lange-zeile");
        let paths = lab.paths();
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        assert!(tail.new_names().is_empty());
        let long = "x".repeat(logread::BLOCK + 10);
        lab.append(&format!("{long}\n{}", bp("Danach")));
        let mut found = Vec::new();
        for _ in 0..5 {
            found.extend(tail.new_names());
        }
        assert_eq!(names(&found), ["Danach"]);
        assert!(!tail.behind);
    }

    #[test]
    fn tail_hands_sections_to_contracts() {
        struct Seen(Arc<Mutex<Vec<String>>>);
        impl ContractScan for Seen {
            fn clear(&mut self) {
                self.0.lock().unwrap().push("leer".into());
            }
            fn scan(&mut self, text: &str) {
                self.0.lock().unwrap().push(text.into());
            }
        }
        let lab = Lab::new("auftraege");
        let paths = lab.paths();
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        let log = Arc::new(Mutex::new(Vec::new()));
        tail.contracts = Some(Box::new(Seen(Arc::clone(&log))));
        tail.new_names();
        lab.append("Zeile eins\nZeile zwei\nhalb");
        tail.new_names();
        assert_eq!(*log.lock().unwrap(), ["leer", "leer", "Zeile eins\nZeile zwei"]);
        assert_eq!(tail.last_text, "Zeile eins\nZeile zwei");
    }

    #[test]
    fn a_changed_handle_of_the_same_account_number_counts_and_another_number_does_not() {
        let lab = Lab::new("kennung");
        let a = lab.backup("a.log", &session("Alt", 4242, &["Eins"]));
        let b = lab.backup("b.log", &session("Neu", 4242, &["Zwei"]));
        let c = lab.backup("c.log", &session("Neu", 777, &["Drei"]));
        stamp(&a, 0);
        stamp(&b, 100);
        stamp(&c, 200);
        let paths = lab.paths();
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Alt")));
        assert!(paths.set_setting(ACCOUNT_ID_SETTING, Value::str("4242")));
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let (finds, report) = src.read_backlog(&mut ReadState::open(&paths), &rx(), true, false);
        assert_eq!(names(&finds), ["Eins", "Zwei"], "the new handle of the same number is the own account");
        assert_eq!((report.files, report.foreign), (2, 1), "the same handle with another number is not");
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some("Neu"), "the handle follows the number");
        assert_eq!(paths.setting(ACCOUNT_ID_SETTING).as_deref(), Some("4242"));
        // A log without the line of the character is told by its handle.
        let own = Own { handle: "Neu".into(), id: Some("4242".into()) };
        assert!(own.counts(&Identity { handle: Some("neu".into()), id: None }));
        assert!(!own.counts(&Identity { handle: Some("Alt".into()), id: None }));
    }

    #[test]
    fn the_first_account_is_learned_by_its_number_across_a_changed_handle() {
        let lab = Lab::new("lernen");
        let a = lab.backup("a.log", &session("Alt", 9, &[]));
        let b = lab.backup("b.log", &session("Neu", 9, &[]));
        let c = lab.backup("c.log", &session("Zweit", 5, &[]));
        stamp(&a, 0);
        stamp(&b, 100);
        stamp(&c, 50);
        let paths = lab.paths();
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        // By handle it would be a tie of three; by number two logs against one.
        assert_eq!(src.own_identity(None), Some(Own { handle: "Neu".into(), id: Some("9".into()) }));
        assert_eq!(paths.setting(ACCOUNT_ID_SETTING).as_deref(), Some("9"));
    }

    #[test]
    fn an_account_stored_without_number_learns_it_and_logs_skipped_as_foreign_are_read_once() {
        let lab = Lab::new("altbestand");
        let a = lab.backup("a.log", &session("Alt", 4242, &["Eins"]));
        let b = lab.backup("b.log", &session("Neu", 4242, &["Zwei"]));
        stamp(&a, 0);
        stamp(&b, 100);
        let paths = lab.paths();
        // As 4.12 left it: the old handle without a number, both logs remembered as read, the one of the new
        // handle as foreign, and the accounts per log without the number.
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Alt")));
        let mut state = ReadState::open(&paths);
        state.remember(&a, Some("Alt"));
        state.remember(&b, Some("Neu"));
        state.save();
        let entry = |p: &str, who: &str| {
            let (ns, size) = stat(p).unwrap();
            Value::Array(vec![Value::Int(ns.to_string()), Value::Int(size.to_string()), Value::str(who)])
        };
        let old = Value::Object(vec![(a.clone(), entry(&a, "Alt")), (b.clone(), entry(&b, "Neu"))]);
        fs::write(paths.app_file(ACCOUNTS_FILE), jsonio::dumps(&old, None, false)).unwrap();

        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let (finds, report) = src.read_backlog(&mut ReadState::open(&paths), &rx(), true, false);
        assert_eq!(names(&finds), ["Zwei"], "the missed blueprint comes after all");
        assert_eq!((report.files, report.skipped, report.foreign), (1, 1, 0));
        assert_eq!(
            paths.setting(ACCOUNT_ID_SETTING).as_deref(),
            Some("4242"),
            "learned from the log of the old handle"
        );
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some("Neu"));
        assert!(paths.setting_bool(ACCOUNTS_RECHECKED, false));
        // Once: the next start reads nothing again.
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        let (finds, report) = src.read_backlog(&mut ReadState::open(&paths), &rx(), true, false);
        assert!(finds.is_empty(), "{finds:?}");
        assert_eq!((report.files, report.skipped), (0, 2));
    }

    #[test]
    fn blueprints_of_another_account_are_tallied_once_per_session_without_its_handle() {
        let lab = Lab::new("strichliste");
        lab.backup("a.log", &session("Spieler", 1, &["Eigen"]));
        lab.backup("b.log", &session("Zweit", 2, &["Fremd", "Noch eins"]));
        let paths = lab.paths();
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Spieler")));
        let src = LogSource::with_clock(&paths, Sources::default(), clock().1);
        assert_eq!(foreign_tally(&paths), ForeignTally::default());
        let (finds, _) = src.read_backlog(&mut ReadState::open(&paths), &rx(), true, false);
        assert_eq!(names(&finds), ["Eigen"]);
        let at = crate::logtime::seconds("2026-10-01T12:00:00");
        assert_eq!(foreign_tally(&paths), ForeignTally { blueprints: 2, sessions: 1, last: at });
        let file = fs::read_to_string(paths.app_file(FOREIGN_FILE)).unwrap();
        assert!(!file.contains("Zweit") && !file.contains("accountId"), "{file}");
        // Reading everything again counts nothing twice.
        src.read_all(&rx()).expect("every log can be read");
        assert_eq!(foreign_tally(&paths).blueprints, 2);

        // The running log of the other account: what it receives is tallied, not returned.
        fs::write(lab.game_log(), session("Zweit", 2, &[])).unwrap();
        let mut tail = LogTail::new(&src, ReadState::open(&paths), rx());
        assert!(tail.new_names().is_empty());
        lab.append(&bp("Dritt"));
        assert!(tail.new_names().is_empty());
        assert_eq!(foreign_tally(&paths).blueprints, 3);
        assert_eq!(foreign_tally(&paths).sessions, 1, "the running log began at the same moment: one session");

        // Chosen anew: the tally is gone.
        assert!(clear_foreign_tally(&paths));
        assert_eq!(foreign_tally(&paths), ForeignTally::default());
    }
}
