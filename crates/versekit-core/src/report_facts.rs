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

//! The facts of the error report, gathered from the data folder, the game
//! folder and what the program tells ([`Environment`]). The report itself is
//! set together by [`crate::report`].
//!
//! Every fact is gathered on its own and may fail without taking the report
//! down: what cannot be found is `None` and shows as a dash. Nothing here
//! writes in the data folder except the probe file of [`folder_line`].
//!
//! Lines whose subject lives outside this crate come as [`Environment::extra`] or in a block of their own.
//!
//! ⚠ What runs in the overlay (the signature watch, the look at GitHub) is a process of its own: the report
//! made in the main window reads what the overlay left in the data folder and says where it cannot know
//! ([`scanner_line`], [`version_check_line`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::anti_afk;
use crate::anti_afk_key;
use crate::catalog_cache;
use crate::collection;
use crate::crash;
use crate::diagnostics::Role;
use crate::error_log::ErrorLog;
use crate::formats;
use crate::hangar_inbox;
use crate::jsonio::{self, Map, Value};
use crate::language::{Arg, Choice, Lang, tf_in};
use crate::logsource::{
    ACCOUNT_ID_SETTING, ACCOUNT_SETTING, ACCOUNTS_RECHECKED, ALL_ACCOUNTS, LogSource, ReadState, foreign_tally,
};
use crate::paths::{Paths, name_key, seconds_of};
use crate::phrases::{self, Origin};
use crate::report::{Block, Entry, Facts, OtherRun, py_repr, py_text, truthy};
use crate::stock;
use crate::text::py_strip;
use crate::trail;

/// How many names of blueprints the catalog does not know the report lists.
pub const UNKNOWN_MAX: usize = 12;
/// The setting with the name the player gives as reporter, and the mark that
/// the player emptied the field on purpose.
pub const REPORTER_SETTING: &str = "melder_name";
pub const REPORTER_CLEARED: &str = "melder_leer_gewollt";
/// Settings whose value is an id or a secret (units of the Basetool, the
/// game account and its number, the name of the reporter, the pairing code of the hangar
/// delivery): the report shows `•••` instead.
pub const MASKED_SETTINGS: [&str; 6] = [
    "basetool_einheit_neu",
    "staffellager_einheit",
    ACCOUNT_SETTING,
    ACCOUNT_ID_SETTING,
    REPORTER_SETTING,
    hangar_inbox::SETTING_TOKEN,
];
/// Settings that do not appear in the settings line at all.
pub const HIDDEN_SETTINGS: [&str; 2] = [REPORTER_CLEARED, ACCOUNTS_RECHECKED];
/// Own files that are rewritten at every start or sync.
pub const REWRITTEN_FILES: [&str; 5] =
    ["start-spur.txt", "versionen.json", "fehler.json", "einstellungen.json", "bestand.json"];
/// The setting that lets blueprints without a known way into the catalog.
pub const SETTING_ALL: &str = "alle_bauplaene";

/// The setup log the installer leaves in the data folder.
pub const SETUP_LOG: &str = "update-setup.txt";
/// If it could not be written there, it goes to the temp folder as
/// `scbp-update-setup*.txt`.
pub const SETUP_LOG_PREFIX: &str = "scbp-update-setup";
/// Lines of the setup log that say something about the outcome.
const SETUP_KEYWORDS: [&str; 11] = [
    "error",
    "exception",
    "fail",
    "succeeded",
    "rolling back",
    "exit code",
    "abort",
    "code 32",
    "denied",
    "cannot",
    "log closed",
];
/// So many of those lines the report shows.
const SETUP_LINES: usize = 8;

/// What was found when the temp folder was searched: the logs, and how often
/// it was searched at all.
#[derive(Default)]
struct TempScan {
    logs: Option<Vec<PathBuf>>,
    scans: usize,
}

/// One entry per searched folder.
static TEMP_SCANS: Mutex<BTreeMap<PathBuf, TempScan>> = Mutex::new(BTreeMap::new());

/// The setup logs `scbp-update-setup*.txt` directly in `folder` (any case,
/// like the Windows search).
///
/// ⚠ The temp folder is searched **once per program run**, not at every
/// report: it can hold tens of thousands of entries, and the page builds the
/// report on every visit and after every keystroke. Only whoever writes such
/// a log in the temp folder calls [`forget_temp_setup_logs`]; the log in the
/// data folder is looked at directly each time ([`newest_setup_log`]).
pub fn temp_setup_logs(folder: &Path) -> Vec<PathBuf> {
    {
        let mut all = TEMP_SCANS.lock().unwrap_or_else(|e| e.into_inner());
        let slot = all.entry(folder.to_path_buf()).or_default();
        if let Some(logs) = &slot.logs {
            return logs.clone();
        }
        slot.scans += 1;
    }
    // Searched without the lock: a folder with tens of thousands of entries
    // takes a while, and nobody else is to wait for it.
    let mut found: Vec<PathBuf> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.starts_with(SETUP_LOG_PREFIX) && name.ends_with(".txt")
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let mut all = TEMP_SCANS.lock().unwrap_or_else(|e| e.into_inner());
    all.entry(folder.to_path_buf()).or_default().logs = Some(found.clone());
    found
}

/// Drops what was found in the temp folder: the next report searches again.
pub fn forget_temp_setup_logs() {
    let mut all = TEMP_SCANS.lock().unwrap_or_else(|e| e.into_inner());
    all.values_mut().for_each(|slot| slot.logs = None);
}

/// How often `folder` was searched in this run.
pub fn temp_scans(folder: &Path) -> usize {
    TEMP_SCANS.lock().unwrap_or_else(|e| e.into_inner()).get(folder).map_or(0, |slot| slot.scans)
}

/// The newest setup log: in the data folder, or in `temp` if it could not be
/// written there.
pub fn newest_setup_log(paths: &Paths, temp: &Path) -> Option<PathBuf> {
    std::iter::once(PathBuf::from(paths.app_file(SETUP_LOG)))
        .chain(temp_setup_logs(temp))
        .filter_map(|p| Some((fs::metadata(&p).ok().filter(fs::Metadata::is_file)?.modified().ok()?, p)))
        // At the same time the first one stays, the data folder (as `max` in 4.12).
        .reduce(|best, next| if next.0 > best.0 { next } else { best })
        .map(|(_, path)| path)
}

/// The last `count` lines of a file that are not blank, cut at the end and
/// kept only if they hold one of `keywords`.
fn tail_lines(path: &Path, count: usize, keywords: &[&str]) -> Vec<String> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    let text = String::from_utf8_lossy(&bytes);
    let rows: Vec<String> = text
        .lines()
        .filter(|l| !py_strip(l).is_empty())
        .filter(|l| {
            let lower = l.to_lowercase();
            keywords.is_empty() || keywords.iter().any(|k| lower.contains(k))
        })
        .map(|l| l.trim_end().to_string())
        .collect();
    rows[rows.len().saturating_sub(count)..].to_vec()
}

/// A moment in seconds since 1970 as the report writes dates, a dash if it has none.
fn report_date(lang: Lang, seconds: Option<f64>) -> String {
    seconds
        .and_then(formats::floor_seconds)
        .and_then(formats::local_time)
        .map_or_else(|| "—".to_string(), |time| formats::strftime(&t(lang, "b_datum"), &time))
}

/// The modification time of a file as the report writes dates.
fn file_stamp(lang: Lang, path: &Path) -> String {
    report_date(lang, fs::metadata(path).ok().and_then(|m| m.modified().ok()).map(seconds_of))
}

/// What the last update attempt left behind (`_update_attempt_lines` of 4.12): the run mark while it lies there,
/// the exit code of the helper, the last lines of both helper logs (the older one first), and the outcome lines
/// of the setup log (`temp` is the temp folder).
pub fn update_lines(paths: &Paths, lang: Lang, temp: &Path) -> Vec<String> {
    use crate::update_run::{LOG_FILE, LOG_FILE_OLD, read_result, read_run};
    let mut out = Vec::new();
    if let Some(run) = read_run(paths) {
        let or_unknown = |text: &str| if text.is_empty() { "?".to_string() } else { text.to_string() };
        let began = report_date(lang, Some(run.start).filter(|start| *start >= 0.0));
        out.push(tf_in(
            lang,
            "b_up_marke",
            &[
                ("0", Arg::Text(or_unknown(&run.previous))),
                ("1", Arg::Text(or_unknown(&run.target))),
                ("2", Arg::Text(began)),
            ],
        ));
    }
    if let Some(code) = read_result(paths) {
        out.push(tf_in(lang, "b_up_ergebnis", &[("0", Arg::Int(i64::from(code)))]));
    }
    for name in [LOG_FILE_OLD, LOG_FILE] {
        let path = PathBuf::from(paths.app_file(name));
        let rows = tail_lines(&path, SETUP_LINES, &[]);
        if rows.is_empty() {
            continue;
        }
        out.push(tf_in(lang, "b_up_helfer", &[("0", Arg::from(name)), ("1", Arg::Text(file_stamp(lang, &path)))]));
        out.extend(rows.into_iter().map(|row| format!("  {row}")));
    }
    if let Some(path) = newest_setup_log(paths, temp) {
        let rows = tail_lines(&path, SETUP_LINES, &SETUP_KEYWORDS);
        if !rows.is_empty() {
            out.push(tf_in(lang, "b_up_setup", &[("0", Arg::Text(file_stamp(lang, &path)))]));
            out.extend(rows.into_iter().map(|row| format!("  {row}")));
        }
    }
    out
}

/// What only the running program knows.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub version: String,
    /// How the program is packaged, and what it runs on (version of the
    /// language and of the interface library).
    pub packaging: Option<String>,
    pub runtime: Option<String>,
    /// Screen and window, already written (`b_skalierung`, `b_fenstermass`, `b_scharf`).
    pub screen: Option<String>,
    /// Physical pixels per point of the system scaling (125 % is 1.25), for the signature scan line.
    pub scale: Option<f64>,
    /// The overlay as it stands, if there is one.
    pub overlay: Option<String>,
    /// The Basetool line, only for someone who uses it.
    pub basetool: Option<String>,
    /// More lines for the facts block that follows the folder line, for
    /// parts of the program that are not part of this crate.
    pub extra: Vec<Entry>,
}

fn t(lang: Lang, key: &str) -> String {
    crate::language::t_in(lang, key)
}

fn n(lang: Lang, key: &str, value: usize) -> String {
    tf_in(lang, key, &[("0", Arg::from(value))])
}

/// The collection as it is on disk, read only; an empty one if the file is
/// missing or has no blueprints.
fn read_collection(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(collection::FILE_NAME)) {
        Ok(Value::Object(map)) if matches!(map.iter().find(|(k, _)| k == "bauplaene"), Some((_, Value::Object(_)))) => {
            map
        }
        _ => vec![("bauplaene".to_string(), Value::Object(Vec::new()))],
    }
}

/// What the report needs of the stored catalog: its version and the key of
/// every blueprint, with whether it has no known way.
#[derive(Debug, Default)]
struct CatalogFacts {
    version: Option<String>,
    keys: Vec<(String, bool)>,
}

type CatalogMemo = ((String, u64, Option<std::time::SystemTime>), Arc<CatalogFacts>);

/// The catalog of the last read, as long as the file is the same (name,
/// size and modification time): the file is some 12 MB, reading it takes
/// longer than everything else in the report together.
static CATALOG_MEMO: Mutex<Option<CatalogMemo>> = Mutex::new(None);

fn read_catalog(path: &str) -> CatalogFacts {
    let Ok(root) = jsonio::read_json(path) else { return CatalogFacts::default() };
    let Some(Value::Object(blueprints)) = root.get("bauplaene") else { return CatalogFacts::default() };
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
    let keys = blueprints
        .iter()
        .map(|(key, entry)| {
            let key = text(entry.get("n")).map_or_else(|| key.clone(), |name| name_key(&name));
            (key, matches!(entry.get("ohne_weg"), Some(Value::Bool(true))))
        })
        .collect();
    let version = match root.get("version") {
        Some(v) if truthy(v) => Some(py_text(v)),
        _ => None,
    };
    CatalogFacts { version, keys }
}

fn catalog_facts(paths: &Paths) -> Arc<CatalogFacts> {
    let path = catalog_cache::current_file(paths);
    let stamp = fs::metadata(&path).ok().map(|m| (path.clone(), m.len(), m.modified().ok()));
    let Some(stamp) = stamp else { return Arc::new(CatalogFacts::default()) };
    let mut memo = CATALOG_MEMO.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((known, facts)) = memo.as_ref()
        && *known == stamp
    {
        return Arc::clone(facts);
    }
    let facts = Arc::new(read_catalog(&path));
    *memo = Some((stamp, Arc::clone(&facts)));
    facts
}

/// The keys of the catalog's blueprints: without those with no known way,
/// unless the player lets them in.
fn known_keys(paths: &Paths) -> BTreeSet<String> {
    let all = paths.setting_bool(SETTING_ALL, false);
    catalog_facts(paths).keys.iter().filter(|(_, no_way)| all || !no_way).map(|(key, _)| key.clone()).collect()
}

/// How many logs there are, how many were read, and what came of them.
///
/// Many logs and no blueprints can mean the detection fails or that someone
/// is new to the game; this line tells them apart (read 0 of N: the backlog
/// never ran).
pub fn log_line(paths: &Paths, lang: Lang) -> String {
    log_line_with(paths, &read_collection(paths), lang)
}

/// How many of the logs the read state knows: the same answer as
/// `ReadState::knows` for each, but size and time come from one listing per
/// folder, not from one `stat` per file (on Windows the listing is far
/// cheaper: 240 files take a few milliseconds instead of a third of a second).
fn read_count(state: &ReadState, backups: &[String]) -> usize {
    let mut listings: HashMap<PathBuf, HashMap<OsString, (u64, f64)>> = HashMap::new();
    let mut read = 0;
    for backup in backups {
        let path = Path::new(backup);
        let listing = path.parent().map(|parent| {
            listings.entry(parent.to_path_buf()).or_insert_with(|| {
                let Ok(entries) = fs::read_dir(parent) else { return HashMap::new() };
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        Some((e.file_name(), (meta.len(), seconds_of(meta.modified().ok()?))))
                    })
                    .collect()
            })
        });
        let found = listing.and_then(|l| l.get(path.file_name()?));
        let known = match found {
            Some(&(size, time)) => state.knows_with(backup, || Some(size), || Some(time)),
            None => state.knows(backup),
        };
        read += usize::from(known);
    }
    read
}

fn log_line_with(paths: &Paths, data: &Map, lang: Lang) -> String {
    let backups = paths.log_backups(None);
    let mut parts = vec![n(lang, if backups.len() == 1 { "b_protokolle_1" } else { "b_protokolle" }, backups.len())];
    let state = ReadState::open(paths);
    let read = read_count(&state, &backups);
    parts.push(n(lang, "b_logs_gelesen", read));
    let sources = collection::by_source(data);
    let count = |name: &str| sources.iter().find(|(s, _)| s == name).map_or(0, |(_, c)| *c);
    let found = count("log") + count("nachlese");
    parts.push(n(lang, if found == 1 { "b_logs_funde_1" } else { "b_logs_funde" }, found));
    parts.join(" · ")
}

/// The blueprints of sessions of another account that did not count: how many, from how many sessions, when
/// the last began, and whether the own account number is known. No handle, no number. `None` while there are none.
pub fn foreign_line(paths: &Paths, lang: Lang) -> Option<String> {
    let tally = foreign_tally(paths);
    if tally.blueprints == 0 {
        return None;
    }
    let last = tally.last.map_or(Arg::from("—"), |at| Arg::moment(at as f64));
    let id = t(lang, if paths.setting(ACCOUNT_ID_SETTING).is_some() { "b_fremd_id_ja" } else { "b_fremd_id_nein" });
    Some(tf_in(
        lang,
        "b_fremd_wert",
        &[("0", Arg::from(tally.blueprints)), ("1", Arg::from(tally.sessions)), ("2", last), ("3", Arg::Text(id))],
    ))
}

/// How many blueprints, and how many of them the list shows: the report
/// counts the entries of `bestand.json`, the list goes through the catalog.
/// Both numbers are right, and whoever sees only one takes the other for
/// broken, so the difference is written.
pub fn collection_line(paths: &Paths, lang: Lang) -> Option<String> {
    collection_line_with(&read_collection(paths), &known_keys(paths), lang)
}

fn collection_line_with(data: &Map, known: &BTreeSet<String>, lang: Lang) -> Option<String> {
    let total = collection::count(data);
    if known.is_empty() {
        return Some(n(lang, "b_n_bauplaene", total));
    }
    let in_catalog = collection::keys(data).intersection(known).count();
    if in_catalog == total {
        return Some(n(lang, "b_n_bauplaene", total));
    }
    Some(tf_in(
        lang,
        "b_n_bp_katalog",
        &[("0", Arg::from(total)), ("1", Arg::from(in_catalog)), ("2", Arg::from(total - in_catalog))],
    ))
}

/// The blueprints of the collection the catalog does not know: the names say
/// what does not fit, the count alone only that something does not.
pub fn unknown_blueprints(paths: &Paths, lang: Lang) -> Option<String> {
    unknown_blueprints_with(&read_collection(paths), &known_keys(paths), lang)
}

fn unknown_blueprints_with(data: &Map, known: &BTreeSet<String>, lang: Lang) -> Option<String> {
    if known.is_empty() {
        return None;
    }
    let mut missing: Vec<String> = collection::blueprints(data)
        .iter()
        .filter(|(key, _)| !known.contains(key))
        .map(|(key, entry)| match entry.get("name") {
            Some(v) if truthy(v) => py_text(v),
            _ => key.clone(),
        })
        .collect();
    if missing.is_empty() {
        return None;
    }
    missing.sort();
    let mut text = missing.iter().take(UNKNOWN_MAX).cloned().collect::<Vec<_>>().join(" · ");
    if missing.len() > UNKNOWN_MAX {
        text.push_str("  ");
        text.push_str(&n(lang, "b_und_weitere", missing.len() - UNKNOWN_MAX));
    }
    Some(text)
}

/// The language lines of the `user.cfg` in the game folder, as the game reads them: `g_language` and, where it
/// stands, `g_languageAudio`. Says so where there is no folder, no file or no language line.
pub fn user_cfg_line(folder: Option<&str>, lang: Lang) -> String {
    let Some(folder) = folder else { return t(lang, "b_nicht_gefunden") };
    let Some(text) = crate::usercfg::read(folder) else { return t(lang, "b_usercfg_keine") };
    let shown: Vec<String> = crate::usercfg::MANAGED
        .iter()
        .filter_map(|key| crate::usercfg::value_in(&text, key).map(|value| format!("{key} = {value}")))
        .collect();
    if shown.is_empty() { t(lang, "b_usercfg_ohne") } else { shown.join(", ") }
}

/// What the log is searched for, and where **each** wording comes from: the
/// list is mixed (own entry, `global.ini`, the built-in table), so one origin
/// for all would send the search the wrong way.
pub fn game_language(paths: &Paths, lang: Lang) -> Option<String> {
    let sources = LogSource::sources_of(paths);
    let (found, _origin) = sources.collect();
    if found.is_empty() {
        return None;
    }
    let mut own: Vec<String> = Vec::new();
    for phrase in sources.own() {
        if !own.contains(&phrase) {
            own.push(phrase);
        }
    }
    let mut from_ini: Vec<String> = Vec::new();
    for file in sources.ini_files() {
        if let Some(phrase) = phrases::phrase_from_ini(&file)
            && !own.contains(&phrase)
            && !from_ini.contains(&phrase)
        {
            from_ini.push(phrase);
        }
    }
    let table: Vec<&String> = found.iter().filter(|p| !own.contains(p) && !from_ini.contains(p)).collect();
    let mut parts = Vec::new();
    if !own.is_empty() {
        parts.push(format!("{} ({})", own.join(", "), t(lang, "b_woher_eigen")));
    }
    if !from_ini.is_empty() {
        parts.push(format!("{} ({})", from_ini.join(", "), t(lang, "b_woher_ini")));
    }
    if !table.is_empty() {
        parts.push(format!(
            "{} ({})",
            table.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            t(lang, "b_woher_tabelle")
        ));
    }
    let _ = Origin::Table;
    Some(parts.join(" · "))
}

/// `report._injection_state`: whether the blueprint notes stand in the
/// game, the two switches, the source with its identifier and stand, and
/// which language file against which language the game reads. No file is
/// not the same as not written: then only `b_inj_keine`.
pub fn injection_line(paths: &Paths, lang: Lang) -> String {
    let status = crate::game_texts::status(paths);
    let Some(file) = status.file.as_deref() else { return t(lang, "b_inj_keine") };
    let mut parts = vec![t(lang, if status.applied { "b_inj_drin" } else { "b_inj_weg" })];
    if !paths.setting_bool(crate::game_texts::SETTING_ON, true) {
        parts.push(t(lang, "b_inj_aus"));
    }
    parts.push(t(
        lang,
        if paths.setting_bool(crate::game_texts::SETTING_AUTO, true) { "b_inj_auto" } else { "b_inj_hand" },
    ));
    if let Some(source) = &status.source {
        let ident = crate::translation::installed(paths, source, None).unwrap_or_default();
        let stand = crate::translation::status_text(paths, source, None, lang, &crate::translation::today());
        parts.push(format!("{source} {ident} {stand}"));
    }
    let flavor = paths.host.flavor;
    let folder = flavor.basename(&flavor.dirname(file));
    let folder = if folder.is_empty() { "?".to_string() } else { folder };
    let played = crate::game_ini::game_language(paths).unwrap_or_else(|| "—".into());
    parts.push(crate::language::tf_in(lang, "b_inj_spiel", &[("0", Arg::from(folder)), ("1", Arg::from(played))]));
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// The number of entries in one of the own JSON files, a dash if the file or
/// the field is missing. A missing file is the normal case (nothing on the
/// watchlist yet), not an error.
fn json_size(path: &str, key: &str) -> Option<String> {
    if !std::path::Path::new(path).exists() {
        return Some("—".into());
    }
    let data = jsonio::read_json(path).ok()?;
    let Some(value) = data.get(key) else { return Some("—".into()) };
    Some(match value {
        Value::Str(s) => s.chars().count().to_string(),
        Value::Array(a) => a.len().to_string(),
        Value::Object(m) => m.len().to_string(),
        _ => "—".into(),
    })
}

/// The watchlist line.
pub fn watchlist_line(paths: &Paths, lang: Lang) -> Option<String> {
    let size = json_size(&paths.app_file("watchlist.json"), "eintraege")?;
    Some(tf_in(lang, "b_n_eintraege", &[("0", Arg::Text(size))]))
}

/// The version of the stored catalog; what is on disk, not what the network
/// says (a report must not wait for a time-out).
pub fn catalog_version(paths: &Paths) -> Option<String> {
    catalog_facts(paths).version.clone()
}

/// The signature scan line (`_scanner_line` of 4.12): the switch and whether a watch can read, the digit images
/// the player taught (how many, for how many digits), the scan images kept, the last number read, the scaling,
/// and from which grey image the number was read since the overlay started. Where the screen cannot be grabbed,
/// only that.
///
/// ⚠ The watch runs in the overlay: switched on with no overlay running nothing reads; with one running the line
/// says so and no more (whether its thread lives only the overlay sees). The last number and the counts are what
/// the overlay left in the data folder ([`crate::signature_watch::write_last_frame`], `write_counts`).
pub fn scanner_line(paths: &Paths, lang: Lang, scale: Option<f64>) -> String {
    scanner_line_with(paths, lang, scale, crate::screen_grab::supported(), overlay_runs())
}

/// [`scanner_line`] for a known answer to "can the screen be grabbed" and "does an overlay run".
pub fn scanner_line_with(paths: &Paths, lang: Lang, scale: Option<f64>, supported: bool, overlay_runs: bool) -> String {
    use crate::{signature_scan, signature_watch};
    if !supported {
        return t(lang, "b_scan_nicht");
    }
    let state = if !paths.setting_bool(signature_watch::SETTING, false) {
        t(lang, "e_aus")
    } else if overlay_runs {
        t(lang, "bv_scan_overlay")
    } else {
        t(lang, "b_scan_tot")
    };
    let (examples, digits) = own_digit_images(paths);
    let samples = signature_scan::samples(&paths.app_file(signature_scan::SAMPLE_FOLDER)).len();
    let last = signature_watch::read_last_frame(&paths.app_file(signature_watch::LAST_FRAME_FILE))
        .and_then(|frame| frame.value)
        .map_or_else(|| "—".to_string(), |value| value.to_string());
    let scale = scale.map_or_else(|| "—".to_string(), |factor| format!("{factor:.2}"));
    let counts = signature_watch::read_counts(&paths.app_file(signature_watch::COUNTS_FILE));
    let int = |count: u64| Arg::Int(i64::try_from(count).unwrap_or(i64::MAX));
    let line = tf_in(
        lang,
        "b_scan",
        &[
            ("0", Arg::Text(state)),
            ("1", Arg::from(examples)),
            ("2", Arg::from(digits)),
            ("3", Arg::from(samples)),
            ("4", Arg::Text(last)),
            ("5", Arg::Text(scale)),
        ],
    );
    let grey = tf_in(lang, "b_scan_graubild", &[("0", int(counts.green)), ("1", int(counts.bright))]);
    format!("{line} · {grey}")
}

/// The digit images the player taught, over the shared file and the one of each grey image: how many examples,
/// and for how many digits (each key counted once, as 4.12 adds the files together).
fn own_digit_images(paths: &Paths) -> (usize, usize) {
    let files = crate::signature_scan::OwnFiles::in_folder(|name| paths.app_file(name));
    let mut digits: BTreeSet<String> = BTreeSet::new();
    let mut examples = 0;
    for (_, file) in files.all() {
        let Ok(data) = jsonio::read_json(file) else { continue };
        if data.get("format") != Some(&Value::int(1)) {
            continue;
        }
        let Some(Value::Object(by_digit)) = data.get("ziffern") else { continue };
        for (digit, list) in by_digit {
            digits.insert(digit.clone());
            examples += match list {
                Value::Array(items) => items.len(),
                Value::Object(items) => items.len(),
                Value::Str(text) => text.chars().count(),
                _ => 0,
            };
        }
    }
    (examples, digits.len())
}

/// The patch history line (`_patch_history` of 4.12): the five newest game versions with how many blueprints
/// each brought. A short form (`4.10.0`) stands only while it is the only one of its kind among them, else the
/// full version: two builds of one version would read alike.
///
/// ⚠ Ordered as 4.12 orders them: by the version number alone, newest first, and two builds of one version in
/// the order they were written (the shipped ones first, then the own file as it stands), not by build number.
pub fn patch_history_line(paths: &Paths) -> Option<String> {
    use crate::patch_history::{FILE, load, rank};
    let history = load(paths);
    let own: Vec<String> = match jsonio::read_json(&paths.app_file(FILE)).ok().as_ref().and_then(|r| r.get("patches")) {
        Some(Value::Object(patches)) => patches.iter().map(|(version, _)| version.clone()).collect(),
        _ => Vec::new(),
    };
    let mut versions: Vec<&String> = history.keys().filter(|v| !own.contains(v)).collect();
    versions.extend(own.iter().filter(|v| history.contains_key(*v)));
    versions.sort_by_key(|version| std::cmp::Reverse(rank(version)));
    versions.truncate(5);
    if versions.is_empty() {
        return None;
    }
    let short = |version: &str| version.split('-').next().filter(|s| !s.is_empty()).unwrap_or(version).to_string();
    let shorts: Vec<String> = versions.iter().map(|v| short(v)).collect();
    let shown: Vec<String> = versions
        .iter()
        .zip(&shorts)
        .map(|(full, brief)| {
            let count = history.get(*full).map_or(0, |patch| patch.new.len());
            let unique = shorts.iter().filter(|other| *other == brief).count() == 1;
            format!("{} ({count})", if unique { brief.as_str() } else { full.as_str() })
        })
        .collect();
    Some(shown.join(", "))
}

/// The recipes line (`_recipe_line` of 4.12): how many things the stored recipe file lets craft, and its build;
/// "not loaded yet" without a file of the format or without a build.
pub fn recipe_line(paths: &Paths, lang: Lang) -> String {
    let build = jsonio::read_json(&paths.app_file(crate::recipe_file::FILE))
        .ok()
        .filter(|data| data.get("format") == Some(&Value::int(crate::recipe_file::FORMAT)))
        .and_then(|data| data.get("build").filter(|b| truthy(b)).map(py_text));
    let Some(build) = build else { return t(lang, "b_nicht_geladen") };
    let count = crate::injection::sources::items(paths, &BTreeSet::new()).len();
    tf_in(lang, "b_n_bauplaene_kurz", &[("0", Arg::from(count)), ("1", Arg::Text(build))])
}

/// The mining data line (`_mining_line` of 4.12): how many places with ores the stored file names, and its
/// build; "not loaded yet" without a build.
pub fn mining_line(paths: &Paths, lang: Lang) -> String {
    let data = crate::mining_data::MiningData::load(&paths.app_file(crate::mining_data::FILE));
    if data.build().is_empty() {
        return t(lang, "b_nicht_geladen");
    }
    tf_in(lang, "b_n_orte", &[("0", Arg::from(data.places().len())), ("1", Arg::from(data.build()))])
}

/// The version check line (`_version_check_line` of 4.12): when the releases were last stored, the newest one
/// known of the channel, and the look of this run. ⚠ The look at GitHub runs in the overlay; the main window does
/// not know how it went, and the line says so instead of "not checked yet".
pub fn version_check_line(paths: &Paths, lang: Lang) -> String {
    let cache = crate::updater::cache_read(paths);
    let when = (cache.checked != 0.0)
        .then_some(cache.checked)
        .and_then(formats::floor_seconds)
        .and_then(formats::local_time)
        .map_or_else(|| t(lang, "b_va_nie"), |time| formats::strftime(&t(lang, "b_datum"), &time));
    let with_prerelease = paths.setting_bool(crate::updater::PRERELEASE_SETTING, false);
    let newest = crate::updater::latest(&cache.releases, with_prerelease)
        .map(|release| release.version.clone())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "—".to_string());
    tf_in(
        lang,
        "b_va_wert",
        &[("0", Arg::Text(when)), ("1", Arg::Text(newest)), ("2", Arg::Text(t(lang, "bv_va_overlay")))],
    )
}

/// The overlay line: the place the overlay remembers (`watcher.json`), its corner, the edge of its bar, whether
/// clicks go through it, and whether one runs. ⚠ The overlay is a process of its own: its size at this moment,
/// folded or open, only it knows; the line holds what the data folder says.
pub fn overlay_line(paths: &Paths, lang: Lang) -> String {
    overlay_line_with(paths, lang, overlay_runs())
}

/// [`overlay_line`] for a known answer to "does an overlay run".
pub fn overlay_line_with(paths: &Paths, lang: Lang, runs: bool) -> String {
    use crate::overlay::{BarSide, Rect, geometry_text, keys, load_remembered};
    let place = load_remembered(paths).map_or_else(
        || "—".to_string(),
        |r| match r.place {
            Some((x, y)) => geometry_text(Rect::new(x, y, r.w, r.h)),
            None => format!("{}x{}", r.w, r.h),
        },
    );
    let corner_setting = paths.setting(keys::CORNER);
    let corner = corner_setting.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| "frei".to_string());
    let bar = BarSide::from_settings(paths.setting(keys::BAR).as_deref(), corner_setting.as_deref()).setting();
    let through = t(lang, if paths.setting_bool(keys::CLICK_THROUGH, false) { "e_an" } else { "e_aus" });
    let running = t(lang, if runs { "bv_ov_laeuft" } else { "bv_ov_zu" });
    tf_in(
        lang,
        "bv_ov_wert",
        &[
            ("0", Arg::Text(place)),
            ("1", Arg::Text(corner)),
            ("2", Arg::from(bar)),
            ("3", Arg::Text(through)),
            ("4", Arg::Text(running)),
        ],
    )
}

/// The anti-AFK line (`_anti_afk_line` of 4.12): on or off, since when, how often it pressed, which key, whether
/// it looks at the sticks and how many it saw, the reason of the last beat, the last input with its source, and
/// the last stop since the start of the program, with its time. The state comes from what the overlay wrote
/// into the data folder (the overlay is the process that presses; see [`anti_afk`]) and counts only while an
/// overlay runs: the file of an overlay that is gone says nothing about the present, but its last stop stays
/// (it is a fact of the run that was). The key is the one the overlay presses; while it is off, the one it
/// would press ([`anti_afk_key::resolve`]).
pub fn anti_afk_line(paths: &Paths, lang: Lang) -> String {
    let snapshot = anti_afk::Snapshot::read(paths);
    let runs = overlay_runs();
    let key = snapshot.key.clone().filter(|_| runs).unwrap_or_else(|| {
        let chord = anti_afk_key::resolve(paths);
        (chord.input, chord.origin)
    });
    let wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    anti_afk_line_with(&snapshot, &key, wall, anti_afk::supported(), runs, lang)
}

/// [`anti_afk_line`] for a known snapshot, the key, and `wall` (seconds since 1970, now).
pub fn anti_afk_line_with(
    snapshot: &anti_afk::Snapshot,
    key: &(String, anti_afk_key::Origin),
    wall: i64,
    supported: bool,
    overlay_runs: bool,
    lang: Lang,
) -> String {
    if !supported {
        return t(lang, "b_aafk_nicht");
    }
    let none = || "—".to_string();
    let word = |on: bool| t(lang, if on { "e_an" } else { "e_aus" });
    let on = snapshot.on && overlay_runs;
    let ago = |at: i64| Arg::from((wall - at).max(0));
    let failure = snapshot.failure.as_ref().map_or_else(none, |(key, when)| format!("{key} ({when})"));
    let since = snapshot.on_since.filter(|_| on).map_or_else(none, |at| tf_in(lang, "b_aafk_seit", &[("0", ago(at))]));
    let reason = snapshot.reason.filter(|_| on).map_or_else(none, |reason| reason.key().to_string());
    let input = match (snapshot.input_at.filter(|_| on), snapshot.input_source) {
        (None, _) => none(),
        (Some(at), Some(source)) => {
            tf_in(lang, "b_aafk_eingabe", &[("0", ago(at)), ("1", Arg::from(t(lang, source.text_key())))])
        }
        (Some(at), None) => tf_in(lang, "b_aafk_still", &[("0", ago(at))]),
    };
    let sticks = if on { snapshot.sticks } else { 0 };
    tf_in(
        lang,
        "b_aafk_wert",
        &[
            ("0", Arg::from(word(on))),
            ("1", Arg::from(since)),
            ("2", Arg::from(snapshot.sent)),
            ("3", Arg::from(format!("{} ({})", key.0, key.1.key()))),
            ("4", Arg::from(word(snapshot.joystick && overlay_runs))),
            ("5", Arg::from(sticks)),
            ("6", Arg::from(reason)),
            ("7", Arg::from(input)),
            ("8", Arg::from(failure)),
        ],
    )
}

/// Does an overlay run on this desktop?
fn overlay_runs() -> bool {
    crate::window_signal::find(crate::window_signal::Role::Overlay).is_some()
}

/// All posts of My Inventory, and how many different raw materials, goods
/// and items they are.
pub fn storage_line(paths: &Paths, lang: Lang) -> Option<String> {
    let raw = stock::load(&paths.app_file(stock::RAW_FILE), "report.storage");
    let trade = stock::load(&paths.app_file(stock::TRADE_FILE), "report.storage");
    let names = |rows: &mut dyn Iterator<Item = &Value>, field: &str| -> Option<usize> {
        let mut set = BTreeSet::new();
        for row in rows {
            let Value::Object(_) = row else { return None };
            let name = match row.get(field) {
                Some(v) if truthy(v) => py_text(v),
                _ => String::new(),
            };
            let name = py_strip(&name).to_lowercase();
            if !name.is_empty() {
                set.insert(name);
            }
        }
        Some(set.len())
    };
    let materials = names(&mut raw.iter(), "material")?;
    let goods = names(&mut trade.iter().filter(|g| !stock::is_pieces(g)), "ware")?;
    let items = names(&mut trade.iter().filter(|g| stock::is_pieces(g)), "ware")?;
    Some(tf_in(
        lang,
        "b_n_lager",
        &[
            ("0", Arg::from(raw.len() + trade.len())),
            ("1", Arg::from(materials)),
            ("2", Arg::from(goods)),
            ("3", Arg::from(items)),
        ],
    ))
}

/// Whether anything can be saved in the data folder: a small file is written
/// and removed. The reason as the system names it, empty if it worked.
pub fn storage_status(folder: &str) -> Result<(), String> {
    static PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    jsonio::make_dirs(folder).map_err(|e| e.to_string())?;
    // A name of its own: two reports made at the same time must not take each
    // other's file away.
    let number = PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = std::path::Path::new(folder).join(format!(".schreibprobe-{}-{number}", std::process::id()));
    let probe = probe.to_string_lossy().to_string();
    jsonio::write_new(&probe, b"x").map_err(|e| e.to_string())?;
    jsonio::remove_file(&probe).map_err(|e| e.to_string())?;
    Ok(())
}

/// The data folder, and whether anything can be saved there.
///
/// ⚠ If nothing can be saved in the data folder, the report would show
/// standard settings and no errors (the error log is not written either)
/// and read as if all were well.
pub fn folder_line(paths: &Paths, lang: Lang) -> Option<String> {
    let folder = paths.app_folder();
    let mut text = match storage_status(&folder) {
        Ok(()) => format!("{folder} · {}", t(lang, "b_ordner_ok")),
        Err(reason) => format!("{folder} · {}", tf_in(lang, "bv_ordner_gesperrt", &[("0", Arg::Text(reason))])),
    };
    if paths.first_pointer_blocked() {
        text.push_str(" · ");
        text.push_str(&t(lang, "b_zeiger_gesperrt"));
    }
    Some(text)
}

/// The settings that differ from the default, sorted; ids as `•••`.
pub fn settings_line(paths: &Paths, lang: Lang) -> Option<String> {
    let (settings, readable) = paths.settings_read();
    if !readable {
        return Some(t(lang, "bv_einst_gesperrt"));
    }
    let mut set: Vec<(&String, &Value)> =
        settings.iter().filter(|(k, v)| !k.starts_with('_') && truthy(v)).map(|(k, v)| (k, v)).collect();
    set.sort_by(|a, b| a.0.cmp(b.0));
    let items: Vec<String> = set
        .into_iter()
        .filter(|(k, _)| !HIDDEN_SETTINGS.contains(&k.as_str()))
        .map(|(k, v)| {
            let shown = if MASKED_SETTINGS.contains(&k.as_str()) { "•••".to_string() } else { py_text(v) };
            format!("{k}={shown}")
        })
        .collect();
    Some(if items.is_empty() { t(lang, "b_standard") } else { items.join(", ") })
}

/// Read-only, hidden, system file or cloud placeholder on the own files that
/// are rewritten all the time: overwriting such a file fails on Windows.
pub fn file_flags_line(paths: &Paths, lang: Lang) -> String {
    let mut found = Vec::new();
    for name in REWRITTEN_FILES {
        let path = paths.app_file(name);
        let Ok(meta) = fs::metadata(&path) else { continue };
        let attrs = file_attributes(&meta);
        let mut marks = Vec::new();
        if attrs & 0x1 != 0 || meta.permissions().readonly() {
            marks.push(t(lang, "b_ds_schreibschutz"));
        }
        if attrs & 0x2 != 0 {
            marks.push(t(lang, "b_ds_versteckt"));
        }
        if attrs & 0x4 != 0 {
            marks.push(t(lang, "b_ds_system"));
        }
        if attrs & (0x400 | 0x1000 | 0x40_0000) != 0 {
            marks.push(t(lang, "b_ds_cloud"));
        }
        if !marks.is_empty() {
            found.push(format!("{name}: {}", marks.join(", ")));
        }
    }
    if found.is_empty() { t(lang, "b_ds_ok") } else { found.join(" · ") }
}

#[cfg(windows)]
fn file_attributes(meta: &fs::Metadata) -> u32 {
    std::os::windows::fs::MetadataExt::file_attributes(meta)
}

#[cfg(not(windows))]
fn file_attributes(_meta: &fs::Metadata) -> u32 {
    0
}

/// The Basetool line: connection, rights, last sync, last refusal.
pub fn basetool_value(lang: Lang, connected: bool, rights: &[String], last_sync: &str, code: &str) -> String {
    let mut shown: Vec<String> = rights.iter().map(|r| r.replace("exchange.", "")).collect();
    shown.sort();
    let or_dash = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
    tf_in(
        lang,
        "b_basetool_wert",
        &[
            ("0", Arg::Text(t(lang, if connected { "e_an" } else { "e_aus" }))),
            ("1", Arg::Text(or_dash(&shown.join(", ")))),
            ("2", Arg::Text(or_dash(last_sync))),
            ("3", Arg::Text(or_dash(code))),
        ],
    )
}

/// The Basetool line of the program (`_basetool_line` of 4.12), only for someone who switched an area on: the
/// rights the login granted (else those of the last pass), the last pass that went through and the code of the
/// last refusal, from the sync of the program. Without one (`sync` none) nothing is connected.
pub fn basetool_line(paths: &Paths, lang: Lang, sync: Option<&crate::basetool::sync::SyncHandle>) -> Option<String> {
    if !crate::basetool::scopes::enabled(paths) {
        return None;
    }
    let Some(sync) = sync else { return Some(basetool_value(lang, false, &[], "", "")) };
    let status = sync.status().get().1.map(|s| (*s).clone()).unwrap_or_default();
    let connection = sync.connection();
    let granted = connection.granted();
    let rights = if granted.is_empty() { &status.capabilities } else { &granted };
    let last = status
        .last_sync
        .and_then(formats::floor_seconds)
        .and_then(formats::local_time)
        .map(|time| formats::strftime("%Y-%m-%d %H:%M:%S", &time))
        .unwrap_or_default();
    Some(basetool_value(lang, connection.connected(), rights, &last, &status.code))
}

/// The name the player entered as reporter, or `None`: an empty entry counts
/// only with the mark that the field was emptied on purpose.
pub fn own_reporter(paths: &Paths) -> Option<String> {
    let settings = paths.settings();
    let Some(Value::Str(raw)) = settings.iter().find(|(k, _)| k == REPORTER_SETTING).map(|(_, v)| v) else {
        return None;
    };
    let name = py_strip(raw).to_string();
    (!name.is_empty() || paths.setting_bool(REPORTER_CLEARED, false)).then_some(name)
}

/// Who sends the report: the entered name (also one emptied on purpose, then
/// none), else the chosen game account. The field on the report page shows
/// exactly this, so no name leaves unseen.
pub fn reporter_name(paths: &Paths) -> String {
    if let Some(own) = own_reporter(paths) {
        return own;
    }
    let account = paths.setting(ACCOUNT_SETTING).unwrap_or_default();
    let account = py_strip(&account);
    if account == ALL_ACCOUNTS { String::new() } else { account.to_string() }
}

/// `Windows 11 · Build 10.0.26200`, or the Linux counterpart.
pub fn system_line(paths: &Paths) -> Option<String> {
    #[cfg(windows)]
    {
        let _ = paths;
        let build = crate::paths::windows_build_number()?;
        let release = if build.parse::<u32>().ok()? >= 22000 { "11" } else { "10" };
        Some(format!("Windows {release} · Build 10.0.{build}"))
    }
    #[cfg(not(windows))]
    {
        let release = fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|s| py_strip(&s).to_string());
        let pretty = fs::read_to_string("/etc/os-release").ok().and_then(|text| {
            text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_string())
        });
        let parts: Vec<String> = ["Linux".to_string()]
            .into_iter()
            .chain(pretty)
            .chain(release)
            .chain(session_parts(&paths.host))
            .filter(|p| !p.is_empty())
            .collect();
        Some(parts.join(" · "))
    }
}

/// The kind of session and the desktop, from the environment the program was given, as every other value of
/// the report: `wayland`, `KDE`, and `XWayland` when a Wayland session also offers X11 (the windows of the
/// program then run through X11).
pub fn session_parts(host: &crate::paths::Host) -> Vec<String> {
    let mut parts: Vec<String> = ["XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP"]
        .iter()
        .filter_map(|key| host.var_set(key))
        .map(str::to_string)
        .collect();
    if host.var_set("WAYLAND_DISPLAY").is_some() && host.var_set("DISPLAY").is_some() {
        parts.push("XWayland".to_string());
    }
    parts
}

/// Everything needed to set the report together.
pub struct Request<'a> {
    pub paths: &'a Paths,
    pub lang: Lang,
    pub env: &'a Environment,
    /// The reporter as the field of the page shows it; `None`: `reporter_name`.
    pub reporter: Option<&'a str>,
    pub message: &'a str,
    pub error_count: usize,
}

fn entry(label: String, value: Option<String>) -> Entry {
    Entry { label, value }
}

/// The way the game would be started, shortened and sorted (`_game_launcher`): whether something was
/// found, what, and whether it is the start command the player entered. The path goes through the
/// redactor, the report may be pasted anywhere.
fn starter_line(paths: &Paths, lang: Lang) -> String {
    let Some(starter) = crate::game_start::game_starter(paths) else { return t(lang, "b_starter_kein") };
    let short = crate::redact::Redactor::new(paths).redact(&starter);
    if paths.setting(crate::game_start::STARTER_SETTING).is_some() {
        return tf_in(lang, "b_starter_eigen", &[("0", Arg::from(short.as_str()))]);
    }
    short
}

/// The language of the interface the report is made in (`de`, `en`), and
/// whether it follows the language of the system.
pub fn interface_language_line(paths: &Paths, lang: Lang) -> String {
    match crate::language::choice_of(&paths.settings()) {
        Choice::Auto => tf_in(lang, "b_osprache_auto", &[("0", Arg::from(lang.code()))]),
        _ => lang.code().to_string(),
    }
}

/// What the process of `role` left behind, named `name`. The trail of the
/// teaching window, which opens and closes often, only goes along with a crash.
pub fn other_run(paths: &Paths, role: Role, name: String) -> OtherRun {
    let found = crash::newest_crash_of(paths, role.crash_files());
    let trail = if role == Role::Overlay || !found.lines.is_empty() {
        trail::last_trail_of(paths, role.trail_file())
    } else {
        Vec::new()
    };
    OtherRun { name, crash: found.lines, crash_at: found.at, crash_version: found.version, trail }
}

/// Gathers the facts of the report.
pub fn collect(request: &Request<'_>) -> Facts {
    let Request { paths, lang, env, .. } = *request;
    let lang_text = |key: &str| t(lang, key);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .and_then(formats::local_time)
        .map(|time| formats::strftime(&lang_text("b_datum"), &time))
        .unwrap_or_default();

    let mut system = vec![
        entry(lang_text("b_system"), system_line(paths)),
        entry(lang_text("b_verpackung"), env.packaging.clone()),
        entry(lang_text("bv_laufzeit"), env.runtime.clone()),
        entry(lang_text("b_bildschirm"), env.screen.clone()),
    ];
    if let Some(overlay) = env.overlay.as_deref().filter(|o| !o.is_empty()) {
        system.push(Entry::new(lang_text("b_overlay"), overlay));
    }

    // The collection and the catalog are read once for all lines.
    let data = read_collection(paths);
    let known = known_keys(paths);
    let game_folder = paths.game_folder();
    let game_log = paths.game_log(game_folder.as_deref());
    let not_found = lang_text("b_nicht_gefunden");
    let user_cfg = user_cfg_line(game_folder.as_deref(), lang);
    let mut game = vec![
        entry(lang_text("b_spiel"), Some(game_folder.unwrap_or_else(|| not_found.clone()))),
        entry(lang_text("b_gamelog"), Some(game_log.unwrap_or(not_found))),
        entry(lang_text("b_sicherungen"), Some(log_line_with(paths, &data, lang))),
    ];
    if let Some(line) = foreign_line(paths, lang) {
        game.push(Entry::new(lang_text("b_fremd"), line));
    }
    game.extend([
        entry(lang_text("b_starter"), Some(starter_line(paths, lang))),
        entry(lang_text("b_spielsprache"), game_language(paths, lang)),
        entry(lang_text("b_osprache"), Some(interface_language_line(paths, lang))),
        entry(lang_text("b_usercfg"), Some(user_cfg)),
        entry(lang_text("b_inj"), Some(injection_line(paths, lang))),
        entry(
            lang_text("b_inj_datei"),
            Some(crate::game_ini::ini_file(paths).unwrap_or_else(|| lang_text("b_inj_keine"))),
        ),
    ]);

    let mut stock_block = vec![entry(lang_text("b_bestand"), collection_line_with(&data, &known, lang))];
    if let Some(unknown) = unknown_blueprints_with(&data, &known, lang).filter(|u| u != "—") {
        stock_block.push(Entry::new(lang_text("b_unbekannt"), unknown));
    }
    stock_block.push(entry(lang_text("b_merkliste"), watchlist_line(paths, lang)));
    stock_block.push(entry(lang_text("b_katalog"), catalog_version(paths)));
    stock_block.push(entry(lang_text("b_historie"), patch_history_line(paths)));
    stock_block.push(entry(lang_text("b_lager"), storage_line(paths, lang)));
    stock_block.push(Entry::new(lang_text("b_rezepte"), recipe_line(paths, lang)));
    stock_block.push(Entry::new(lang_text("b_bergbaudaten"), mining_line(paths, lang)));
    stock_block.push(Entry::new(lang_text("b_scanner"), scanner_line(paths, lang, env.scale)));
    stock_block.push(Entry::new(lang_text("b_aafk"), anti_afk_line(paths, lang)));

    let log = ErrorLog::new(paths, &env.version);
    let mut folder = vec![
        entry(lang_text("b_ordner"), folder_line(paths, lang)),
        entry(lang_text("b_einstellungen"), settings_line(paths, lang)),
        entry(lang_text("b_dateischutz"), Some(file_flags_line(paths, lang))),
    ];
    let failures = trail::TRAIL.write_error();
    if !failures.is_empty() {
        folder.push(Entry::new(lang_text("b_schreibfehler"), failures));
    }
    folder.push(Entry::new(lang_text("b_versionsabgleich"), version_check_line(paths, lang)));
    if let Some(basetool) = env.basetool.as_deref() {
        folder.push(Entry::new(lang_text("b_basetool"), basetool));
    }
    folder.extend(env.extra.iter().cloned());

    let (trail_start, trail_pages) = trail::split_trail(paths);
    Facts {
        lang,
        version: env.version.clone(),
        now,
        reporter: request.reporter.map_or_else(|| reporter_name(paths), str::to_string),
        message: request.message.to_string(),
        blocks: vec![
            Block::new(lang_text("bf_h_system"), system),
            Block::new(lang_text("bf_h_spiel"), game),
            Block::new(lang_text("bf_h_bestand"), stock_block),
            Block::new(lang_text("bf_h_ordner"), folder),
        ],
        trail_start,
        trail_pages,
        update_lines: update_lines(paths, lang, &std::env::temp_dir()),
        crash: crash::last_crash(paths),
        crash_at: crash::crash_time(paths),
        crash_version: crash::crash_version(paths),
        others: vec![
            other_run(paths, Role::Overlay, lang_text("b_overlay")),
            other_run(paths, Role::Learn, lang_text("b_anlernfenster")),
        ],
        errors: log.latest(request.error_count),
        error_total: log.count(),
    }
}

/// Python's `repr` of a settings value, for the parity check of the line.
pub fn settings_repr(value: &Value) -> String {
    py_repr(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-bericht-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        let value = dir.to_string_lossy().to_string();
        let home = [("SC_BP_HOME", value.as_str()), ("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        (Paths::new(Host::new(Flavor::native(), &home, "/")), dir)
    }

    /// The report names the language lines of the `user.cfg`, and says so where the file or the line is missing.
    #[test]
    fn the_report_shows_the_language_lines_of_the_user_cfg() {
        let (_paths, dir) = paths_in("usercfg");
        let folder = dir.to_string_lossy().to_string();
        assert_eq!(user_cfg_line(None, Lang::De), t(Lang::De, "b_nicht_gefunden"));
        assert_eq!(user_cfg_line(Some(&folder), Lang::De), t(Lang::De, "b_usercfg_keine"), "no file");
        fs::write(dir.join(crate::usercfg::FILE), "r_DisplayInfo = 3\n").unwrap();
        assert_eq!(user_cfg_line(Some(&folder), Lang::En), t(Lang::En, "b_usercfg_ohne"), "no language line");
        fs::write(dir.join(crate::usercfg::FILE), "g_language = english\r\ng_languageAudio = english\r\n").unwrap();
        assert_eq!(
            user_cfg_line(Some(&folder), Lang::De),
            "g_language = english, g_languageAudio = english",
            "both lines as the game reads them"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_data_folder_gives_a_report_with_dashes() {
        let (paths, dir) = paths_in("leer");
        let env = Environment { version: "5.0.0".into(), ..Default::default() };
        let request =
            Request { paths: &paths, lang: Lang::De, env: &env, reporter: Some(""), message: "", error_count: 8 };
        let facts = collect(&request);
        assert_eq!(facts.blocks.len(), 4);
        assert!(facts.errors.is_empty() && facts.crash.is_empty());
        let redactor = crate::redact::Redactor::new(&paths);
        let text = crate::report::build(&facts, &redactor);
        let row = |label: &str| {
            text.lines()
                .find(|l| l.trim_start().starts_with(label))
                .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        assert_eq!(row("Von").as_deref(), Some("Von nicht angegeben"), "{text}");
        assert!(text.contains("## Letzte Fehler\n  Keine aufgezeichnet."), "{text}");
        assert_eq!(
            row("Merkliste").as_deref(),
            Some("Merkliste — Einträge"),
            "a missing file reads as the Python version has it:\n{text}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_interface_language_stands_next_to_the_game_language() {
        let (paths, dir) = paths_in("oberflaeche");
        assert_eq!(interface_language_line(&paths, Lang::De), "de (automatisch nach dem System)");
        assert_eq!(interface_language_line(&paths, Lang::En), "en (automatic, from the system)");
        assert!(paths.set_setting("sprache", Value::str("en")));
        assert_eq!(interface_language_line(&paths, Lang::En), "en");
        let env = Environment { version: "5.0.0".into(), ..Default::default() };
        let request =
            Request { paths: &paths, lang: Lang::En, env: &env, reporter: Some(""), message: "", error_count: 8 };
        let entries = collect(&request).blocks[1].entries.clone();
        let at = |label: &str| entries.iter().position(|e| e.label == label);
        assert_eq!(at("Interface language"), at("Game language").map(|n| n + 1), "{entries:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_overlay_and_the_teaching_window_bring_their_crash_and_trail() {
        let (paths, dir) = paths_in("andere");
        let env = Environment { version: "5.0.0".into(), ..Default::default() };
        let request =
            Request { paths: &paths, lang: Lang::De, env: &env, reporter: Some(""), message: "", error_count: 8 };
        fs::write(paths.app_file(trail::FILE_OVERLAY), "10:00:00  Overlay wird gebaut\n").unwrap();
        fs::write(paths.app_file(trail::FILE_LEARN), "10:00:00  Start, Version 5.0.0, windows\n").unwrap();
        let others = collect(&request).others;
        assert_eq!(others.len(), 2);
        assert_eq!(others[0].name, "Overlay");
        assert_eq!(others[0].trail, ["10:00:00  Overlay wird gebaut"], "the overlay's trail always goes along");
        assert!(others[1].trail.is_empty(), "the teaching window's trail only with a crash");
        fs::write(paths.app_file(crash::LEARN.current), "Fatal error: panic: anlernen\n").unwrap();
        let others = collect(&request).others;
        assert_eq!(others[1].crash, ["Fatal error: panic: anlernen"]);
        assert_eq!(others[1].trail.len(), 1);
        let text = crate::report::build(&collect(&request), &crate::redact::Redactor::new(&paths));
        assert!(text.contains("## Harter Abbruch · Anlernfenster"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_starter_line_says_none_found_or_the_own_command() {
        let dir = std::env::temp_dir().join(format!("versekit-bericht-starter-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("folder");
        let value = dir.to_string_lossy().to_string();
        // Posix rules and a home folder without anything: no game, no script, no starter.
        let home = [("SC_BP_HOME", value.as_str()), ("HOME", value.as_str())];
        let paths = Paths::new(Host::new(Flavor::Posix, &home, &value));
        assert_eq!(starter_line(&paths, Lang::De), "keiner gefunden, der Startknopf erscheint nicht");
        assert!(starter_line(&paths, Lang::En).contains("none found"), "{}", starter_line(&paths, Lang::En));
        // An own command: shown and marked as entered by hand.
        assert!(paths.set_setting("spielstarter", Value::str("lutris rungame/star-citizen")));
        assert_eq!(starter_line(&paths, Lang::De), "lutris rungame/star-citizen  (selbst eingetragen)");
        // And it stands in the block of the game, after the line of the backups.
        let env = Environment { version: "5.0.0".into(), ..Default::default() };
        let request =
            Request { paths: &paths, lang: Lang::De, env: &env, reporter: Some(""), message: "", error_count: 8 };
        let labels: Vec<String> = collect(&request).blocks[1].entries.iter().map(|e| e.label.clone()).collect();
        let at = |label: &str| labels.iter().position(|l| l == label);
        assert!(at("Spielstarter").is_some() && at("Spielstarter") > at("Sicherungen"), "{labels:?}");
        assert!(at("Spielstarter") < at("Spielsprache"), "{labels:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file with `text` and the given age in seconds.
    fn write_aged(path: &Path, text: &str, age: u64) {
        fs::write(path, text).expect("write");
        let time = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        fs::File::options().write(true).open(path).and_then(|f| f.set_modified(time)).expect("time");
    }

    #[test]
    fn the_temp_folder_is_searched_once_per_run() {
        let (_paths, temp) = paths_in("temp-suche");
        for name in ["scbp-update-setup.txt", "SCBP-Update-Setup-1700000000.txt", "scbp-update-setup.log", "andere.txt"]
        {
            write_aged(&temp.join(name), "Log closed\n", 10);
        }
        let names = |found: Vec<PathBuf>| {
            found.iter().filter_map(|p| p.file_name()?.to_str().map(str::to_string)).collect::<Vec<_>>()
        };
        assert_eq!(temp_scans(&temp), 0);
        let first = names(temp_setup_logs(&temp));
        assert_eq!(first, ["SCBP-Update-Setup-1700000000.txt", "scbp-update-setup.txt"], "only the setup logs count");
        // Building the report again and again does not search again, whatever lies there now.
        write_aged(&temp.join("scbp-update-setup-neu.txt"), "Log closed\n", 5);
        for _ in 0..5 {
            assert_eq!(names(temp_setup_logs(&temp)), first);
            let _ = newest_setup_log(&paths_in("temp-suche-daten").0, &temp);
        }
        assert_eq!(temp_scans(&temp), 1, "one search for all of them");
        // Whoever writes a log there says so, and the next report finds it.
        forget_temp_setup_logs();
        assert!(names(temp_setup_logs(&temp)).contains(&"scbp-update-setup-neu.txt".to_string()));
        assert_eq!(temp_scans(&temp), 2);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn the_newest_setup_log_of_data_folder_and_temp_folder_is_read() {
        let (paths, data) = paths_in("setup-daten");
        let (_unused, temp) = paths_in("setup-temp");
        // Nothing there: no block.
        assert!(update_lines(&paths, Lang::De, &temp).is_empty());
        write_aged(&data.join(SETUP_LOG), "Start\nlog closed\n", 3600);
        let older = update_lines(&paths, Lang::De, &temp);
        assert!(older[0].starts_with("Setup-Protokoll, Stand "), "{older:?}");
        assert_eq!(older[1..], ["  log closed"], "only the lines that say something about the outcome");
        // A newer one in the temp folder wins (it is searched once: write it first).
        let (_unused, temp2) = paths_in("setup-temp2");
        write_aged(&temp2.join("scbp-update-setup-1.txt"), "Fehler\nError 5: Zugriff verweigert\nExit code 5\n", 60);
        let newer = update_lines(&paths, Lang::De, &temp2);
        assert_eq!(newer[1..], ["  Error 5: Zugriff verweigert", "  Exit code 5"], "{newer:?}");
        for folder in [&data, &temp, &temp2] {
            let _ = fs::remove_dir_all(folder);
        }
    }

    #[test]
    fn the_update_attempt_names_run_mark_helper_code_and_both_helper_logs_as_4_12() {
        use crate::update_run::{LOG_FILE, LOG_FILE_OLD, RunMark, begin_run, write_result};
        let (paths, data) = paths_in("update-versuch");
        let (_unused, temp) = paths_in("update-versuch-temp");
        assert!(update_lines(&paths, Lang::De, &temp).is_empty(), "nothing left behind: no block");
        let mark = RunMark { target: "v5.0.1".into(), previous: "5.0.0".into(), start: 0.0, ..Default::default() };
        begin_run(&paths, &mark).expect("run mark");
        write_result(&paths, 90).expect("result");
        write_aged(&data.join(LOG_FILE_OLD), "alt eins\n", 7200);
        let many: String = (1..=10).map(|n| format!("Schritt {n}\n\n")).collect();
        write_aged(&data.join(LOG_FILE), &many, 60);
        write_aged(&data.join(SETUP_LOG), "Start\nExit code 0\n", 30);
        let lines = update_lines(&paths, Lang::De, &temp);
        let epoch = report_date(Lang::De, Some(0.0));
        assert_eq!(lines[0], format!("Laufmarke liegt noch: 5.0.0 → v5.0.1, begonnen {epoch}"), "{lines:?}");
        assert_eq!(lines[1], "Rückgabewert des Helfers: 90");
        assert!(lines[2].starts_with("update-helfer.1.txt, Stand "), "the older log first: {lines:?}");
        assert_eq!(lines[3], "  alt eins");
        assert!(lines[4].starts_with("update-helfer.txt, Stand "), "{lines:?}");
        let tail: Vec<&str> = lines[5..13].iter().map(String::as_str).collect();
        assert_eq!(tail, (3..=10).map(|n| format!("  Schritt {n}")).collect::<Vec<_>>(), "the last eight lines");
        assert!(lines[13].starts_with("Setup-Protokoll, Stand "), "{lines:?}");
        assert_eq!(lines[14..], ["  Exit code 0"]);
        let english = update_lines(&paths, Lang::En, &temp);
        assert!(english[0].starts_with("Run marker still present: 5.0.0 → v5.0.1"), "{english:?}");
        // A mark without versions names them as unknown.
        begin_run(&paths, &RunMark::default()).expect("run mark");
        assert!(update_lines(&paths, Lang::De, &temp)[0].starts_with("Laufmarke liegt noch: ? → ?, begonnen "));
        for folder in [&data, &temp] {
            let _ = fs::remove_dir_all(folder);
        }
    }

    #[test]
    fn at_the_same_time_the_log_of_the_data_folder_wins() {
        let (paths, data) = paths_in("setup-gleich-daten");
        let (_unused, temp) = paths_in("setup-gleich-temp");
        write_aged(&data.join(SETUP_LOG), "log closed\n", 100);
        write_aged(&temp.join("scbp-update-setup.txt"), "log closed\n", 100);
        let same = fs::metadata(data.join(SETUP_LOG)).and_then(|m| m.modified()).expect("time");
        fs::File::options()
            .write(true)
            .open(temp.join("scbp-update-setup.txt"))
            .and_then(|f| f.set_modified(same))
            .expect("time");
        assert_eq!(newest_setup_log(&paths, &temp), Some(data.join(SETUP_LOG)));
        let _ = fs::remove_dir_all(&data);
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn the_session_says_its_kind_its_desktop_and_xwayland_when_both_are_offered() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let parts = |vars: &[(&str, &str)]| session_parts(&Host::new(Flavor::Posix, vars, "/h"));
        let both = [
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_CURRENT_DESKTOP", "KDE"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("DISPLAY", ":0"),
        ];
        assert_eq!(parts(&both), ["wayland", "KDE", "XWayland"]);
        assert_eq!(parts(&both[..3]), ["wayland", "KDE"], "pure Wayland: no X11 for the windows");
        assert_eq!(parts(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]), ["x11"]);
        assert_eq!(parts(&[("XDG_SESSION_TYPE", ""), ("XDG_CURRENT_DESKTOP", "")]), Vec::<String>::new());
    }

    #[test]
    fn ids_in_the_settings_show_as_dots() {
        let (paths, dir) = paths_in("einstellungen");
        paths.set_setting("melder_name", Value::str("Meldername"));
        paths.set_setting("eigener_account", Value::str("Konto"));
        paths.set_setting("overlay_modus", Value::str("klein"));
        let line = settings_line(&paths, Lang::De).unwrap_or_default();
        assert!(line.contains("melder_name=•••") && line.contains("eigener_account=•••"), "{line}");
        assert!(line.contains("overlay_modus=klein"), "{line}");
        assert!(!line.contains("Meldername") && !line.contains("Konto"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn blueprints_of_another_account_that_did_not_count_get_a_line_without_handle_or_number() {
        let (paths, dir) = paths_in("fremd");
        paths.set_setting(ACCOUNT_SETTING, Value::str("Eigenkonto"));
        assert_eq!(foreign_line(&paths, Lang::De), None, "no line while nothing was left out");
        let tally = r#"{"sitzungen": {"2026-10-01T12:00:00.000Z": {"datei": "a.log", "bauplaene": ["x", "y"], "zeit": 1790000000},
            "2026-10-02T12:00:00.000Z": {"datei": "b.log", "bauplaene": ["y"], "zeit": 1790086400}}}"#;
        fs::write(paths.app_file(crate::logsource::FOREIGN_FILE), tally).unwrap();
        let date = |at: i64| formats::moment(Lang::De, at as f64, "m_erster_datum");
        let line = foreign_line(&paths, Lang::De).expect("a line");
        let missing = t(Lang::De, "b_fremd_id_nein");
        assert!(line.starts_with("2 Baupläne aus 2 Sitzungen"), "{line}");
        assert!(line.contains(&date(1_790_086_400)) && line.ends_with(&missing), "{line}");
        paths.set_setting(ACCOUNT_ID_SETTING, Value::str("4242"));
        let line = foreign_line(&paths, Lang::De).expect("a line");
        assert!(
            line.ends_with(&t(Lang::De, "b_fremd_id_ja")) && !line.contains("4242") && !line.contains("Eigenkonto")
        );
        // In the report it stands in the block of the game, after the kept logs.
        let env = Environment { version: "5.0.0".into(), ..Default::default() };
        let request =
            Request { paths: &paths, lang: Lang::De, env: &env, reporter: Some(""), message: "", error_count: 0 };
        let game = &collect(&request).blocks[1];
        let labels: Vec<&str> = game.entries.iter().map(|e| e.label.as_str()).collect();
        let kept = labels.iter().position(|l| *l == t(Lang::De, "b_sicherungen")).expect("kept logs");
        assert_eq!(labels.get(kept + 1).copied(), Some(t(Lang::De, "b_fremd").as_str()), "{labels:?}");
        let settings = settings_line(&paths, Lang::De).unwrap_or_default();
        assert!(settings.contains("eigene_account_kennung=•••") && !settings.contains("4242"), "{settings}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_reporter_follows_the_entry_then_the_account() {
        let (paths, dir) = paths_in("melder");
        assert_eq!(reporter_name(&paths), "");
        paths.set_setting(ACCOUNT_SETTING, Value::str("Konto"));
        assert_eq!(reporter_name(&paths), "Konto");
        paths.set_setting(ACCOUNT_SETTING, Value::str(ALL_ACCOUNTS));
        assert_eq!(reporter_name(&paths), "");
        paths.set_setting(REPORTER_SETTING, Value::str(" Eigener "));
        assert_eq!(reporter_name(&paths), "Eigener");
        // Emptied on purpose: no name, even with an account chosen.
        paths.set_setting(ACCOUNT_SETTING, Value::str("Konto"));
        paths.set_setting(REPORTER_SETTING, Value::str(""));
        assert_eq!(reporter_name(&paths), "Konto", "an empty entry without the mark counts as never entered");
        paths.set_setting(REPORTER_CLEARED, Value::Bool(true));
        assert_eq!(reporter_name(&paths), "");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_anti_afk_line_says_state_count_sticks_and_the_last_stop_and_where_it_cannot_be_done() {
        use anti_afk_key::Origin;
        let alt_x = ("lalt+x".to_string(), Origin::Default);
        let off = anti_afk::Snapshot::default();
        assert_eq!(
            anti_afk_line_with(&off, &alt_x, 2000, true, true, Lang::De),
            "aus · an seit — · 0× gesendet · Taste lalt+x (standard) · Joystick-Blick aus (0 Sticks) · Grund — · \
             letzte Eingabe — · letzter Abbruch —"
        );
        let stopped = anti_afk::Snapshot {
            on: true,
            sent: 12,
            joystick: true,
            failure: Some(("aafk_err_admin".into(), "2026-10-23 12:34".into())),
            ..anti_afk::Snapshot::default()
        };
        let own = ("lalt+k".to_string(), Origin::Own);
        assert_eq!(
            anti_afk_line_with(&stopped, &own, 2000, true, true, Lang::De),
            "an · an seit — · 12× gesendet · Taste lalt+k (eigene) · Joystick-Blick an (0 Sticks) · Grund — · \
             letzte Eingabe — · letzter Abbruch aafk_err_admin (2026-10-23 12:34)",
            "the raw key stands in the report, as in 4.12"
        );
        // The overlay is gone: the file of its run does not say it is on, but the stop stays.
        assert_eq!(
            anti_afk_line_with(&stopped, &own, 2000, true, false, Lang::En),
            "off · on since — · sent 12× · key lalt+k (eigene) · joystick watch off (0 sticks) · reason — · \
             last input — · last abort aafk_err_admin (2026-10-23 12:34)"
        );
        assert_eq!(anti_afk_line_with(&stopped, &own, 2000, false, true, Lang::De), "nur unter Windows");
        assert_eq!(anti_afk_line_with(&stopped, &own, 2000, false, true, Lang::En), t(Lang::En, "b_aafk_nicht"));
    }

    /// While it is on, the line says why the last beat did not press, how long ago the last input came and from
    /// what, since when it is on, which key it sends and how many sticks it sees.
    #[test]
    fn the_anti_afk_line_says_reason_last_input_with_its_source_the_time_on_the_key_and_the_sticks() {
        use crate::stick_input::Source;
        use anti_afk::Reason;
        use anti_afk_key::Origin;
        let waiting = anti_afk::Snapshot {
            on: true,
            sent: 2,
            joystick: true,
            reason: Some(Reason::Waiting),
            input_at: Some(1_958),
            input_source: Some(Source::KeyboardInjected),
            on_since: Some(1_000),
            key: Some(("lalt+k".into(), Origin::Own)),
            sticks: 3,
            ..anti_afk::Snapshot::default()
        };
        let key = ("lalt+k".to_string(), Origin::Own);
        assert_eq!(
            anti_afk_line_with(&waiting, &key, 2000, true, true, Lang::De),
            "an · an seit 1000 s · 2× gesendet · Taste lalt+k (eigene) · Joystick-Blick an (3 Sticks) · \
             Grund wartet · letzte Eingabe vor 42 s (Tastatur eingespeist) · letzter Abbruch —"
        );
        let en = anti_afk_line_with(&waiting, &key, 2000, true, true, Lang::En);
        assert!(en.contains("on since 1000 s") && en.contains("last input 42 s ago (injected keyboard)"), "{en}");
        for (source, word) in [(Source::Keyboard, "Tastatur"), (Source::Mouse, "Maus"), (Source::Stick, "Joystick")] {
            let one = anti_afk::Snapshot { input_source: Some(source), ..waiting.clone() };
            let line = anti_afk_line_with(&one, &key, 2000, true, true, Lang::De);
            assert!(line.contains(&format!("letzte Eingabe vor 42 s ({word})")), "{line}");
        }
        let still = anti_afk::Snapshot { input_source: None, reason: Some(Reason::NotInFront), ..waiting.clone() };
        let line = anti_afk_line_with(&still, &key, 2000, true, true, Lang::De);
        assert!(line.contains("Grund nicht_vorne") && line.contains("letzte Eingabe keine seit 42 s"), "{line}");
        // The overlay is gone: nothing of the run counts as now.
        let gone = anti_afk_line_with(&waiting, &key, 2000, true, false, Lang::De);
        assert!(gone.contains("an seit —") && gone.contains("Grund —") && gone.contains("letzte Eingabe —"), "{gone}");
        assert!(gone.contains("(0 Sticks)"), "{gone}");
    }

    #[test]
    fn the_scanner_line_says_switch_watch_taught_digits_images_last_number_scale_and_grey_image() {
        use crate::{signature_scan, signature_watch};
        let (paths, dir) = paths_in("scanner-zeile");
        let line = |scale, runs| scanner_line_with(&paths, Lang::De, scale, true, runs);
        assert_eq!(
            line(Some(1.25), false),
            "aus · 0 eigene Ziffernbilder (0 Ziffern) · 0 Scan-Bilder · zuletzt — · Skalierung 1.25 · \
             gelesen aus Graubild grün 0, hell 0"
        );
        assert_eq!(scanner_line_with(&paths, Lang::De, None, false, true), "auf diesem System noch nicht verfügbar");
        // Switched on: without an overlay nothing reads, with one the line says it runs there.
        assert!(paths.set_setting(signature_watch::SETTING, Value::Bool(true)));
        assert!(line(None, false).starts_with("an, aber die Wache liest NICHT · "), "{}", line(None, false));
        assert!(line(None, true).starts_with("an, Overlay läuft · "), "{}", line(None, true));
        assert!(line(None, true).contains("Skalierung — · "), "{}", line(None, true));
        // Two digits taught in the shared file, one of them again in the green one; a broken third file adds none.
        let pattern = Value::Array(vec![Value::int(0); 384]);
        let file = |digits: &[(&str, usize)]| {
            let ziffern: Map =
                digits.iter().map(|(d, n)| ((*d).to_string(), Value::Array(vec![pattern.clone(); *n]))).collect();
            jsonio::dumps(
                &Value::Object(vec![("format".into(), Value::int(1)), ("ziffern".into(), Value::Object(ziffern))]),
                None,
                false,
            )
        };
        fs::write(paths.app_file(signature_scan::OWN_TEMPLATE_FILE), file(&[("0", 2), ("1", 1)])).unwrap();
        fs::write(paths.app_file(signature_scan::OWN_GREEN_FILE), file(&[("1", 1)])).unwrap();
        fs::write(paths.app_file(signature_scan::OWN_BRIGHT_FILE), "{kaputt").unwrap();
        let samples = paths.app_file(signature_scan::SAMPLE_FOLDER);
        fs::create_dir_all(&samples).unwrap();
        fs::write(Path::new(&samples).join("a.png"), b"png").unwrap();
        fs::write(Path::new(&samples).join("b.txt"), b"not an image").unwrap();
        let frame = signature_watch::Frame::new(signature_scan::Gray::new(2, 2, vec![0; 4]), Some(12_345), 1);
        assert!(signature_watch::write_last_frame(&paths.app_file(signature_watch::LAST_FRAME_FILE), &frame));
        let counts = signature_scan::ByVariant { green: 12, bright: 3 };
        assert!(signature_watch::write_counts(&paths.app_file(signature_watch::COUNTS_FILE), &counts));
        assert_eq!(
            line(Some(1.5), true),
            "an, Overlay läuft · 4 eigene Ziffernbilder (2 Ziffern) · 1 Scan-Bilder · zuletzt 12345 · Skalierung 1.50 · \
             gelesen aus Graubild grün 12, hell 3"
        );
        assert_eq!(
            scanner_line_with(&paths, Lang::En, Some(1.0), true, true),
            "on, overlay running · 4 own digit images (2 digits) · 1 scan images · last 12345 · scale 1.00 · \
             read from gray image green 12, bright 3"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_patch_history_names_the_five_newest_short_unless_two_builds_of_one_version_stand_there() {
        let (paths, dir) = paths_in("historie");
        let shipped = patch_history_line(&paths).expect("the shipped history");
        assert!(shipped.starts_with("4.10.0 ("), "{shipped}");
        // Two builds of 9.9.0, written in the order the device saw them: 4.12 keeps that order.
        let own = r#"{"patches": {
            "9.9.0-live.1": {"datum": "2026-10-01", "neu": ["A"]},
            "9.9.0-live.2": {"datum": "2026-10-02", "neu": ["B", "C"]},
            "9.8.0-live.5": {"datum": "2026-09-01", "neu": ["D"]},
            "9.7.1": {"datum": "2026-08-01", "neu": ["E"]},
            "9.6.0-live.3": {"datum": "2026-07-01", "neu": ["F"]},
            "9.5.0": {"datum": "2026-06-01", "neu": ["G"]}}}"#;
        fs::write(paths.app_file(crate::patch_history::FILE), own).unwrap();
        assert_eq!(
            patch_history_line(&paths).as_deref(),
            Some("9.9.0-live.1 (1), 9.9.0-live.2 (2), 9.8.0 (1), 9.7.1 (1), 9.6.0 (1)")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recipes_and_mining_data_say_how_many_and_which_build_or_not_loaded() {
        let (paths, dir) = paths_in("rezepte-bergbau");
        assert_eq!(recipe_line(&paths, Lang::De), "noch nicht geladen");
        assert_eq!(mining_line(&paths, Lang::En), "not loaded yet");
        let recipes = |format: i64, build: &str| {
            format!(
                r#"{{"format": {format}, "build": "{build}", "blueprints": [
                    {{"tag": "BP_CRAFT_A", "productName": "Alpha"}}, {{"tag": "BP_CRAFT_B", "productName": "Beta"}},
                    {{"tag": "BP_CRAFT_A2", "productName": "Alpha"}}]}}"#
            )
        };
        let file = paths.app_file(crate::recipe_file::FILE);
        fs::write(&file, recipes(1, "4.3.1-live.7")).unwrap();
        assert_eq!(recipe_line(&paths, Lang::De), "2 Baupläne · Stand 4.3.1-live.7", "one entry per item");
        fs::write(&file, recipes(1, "")).unwrap();
        assert_eq!(recipe_line(&paths, Lang::De), "noch nicht geladen", "no build");
        fs::write(&file, recipes(2, "4.3.1-live.7")).unwrap();
        assert_eq!(recipe_line(&paths, Lang::De), "noch nicht geladen", "another format");
        let mining = r#"{"format": 1, "build": "4.3.1-live.7", "locations": [
            {"locationName": "Daymar", "system": "Stanton", "groups": [
              {"groupName": "SpaceShip_Mineables", "deposits": [{"compositionGuid": "c1", "relativeProbability": 1}]}]},
            {"locationName": "Aaron Halo", "system": "Stanton", "groups": [
              {"groupName": "FPS_Mineables", "deposits": [{"compositionGuid": "c1", "relativeProbability": 1}]}]},
            {"locationName": "Nur Pflanzen", "system": "Stanton", "groups": [
              {"groupName": "Harvestables", "deposits": [{"compositionGuid": "c1", "relativeProbability": 1}]}]}],
          "compositions": {"c1": {"parts": [{"elementName": "Iron", "probability": 1, "minPercent": 50, "maxPercent": 50}]}}}"#;
        fs::write(paths.app_file(crate::mining_data::FILE), mining).unwrap();
        assert_eq!(mining_line(&paths, Lang::De), "2 Orte · Stand 4.3.1-live.7", "only places with ores");
        assert_eq!(mining_line(&paths, Lang::En), "2 locations · build 4.3.1-live.7");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_version_check_says_when_what_and_that_the_look_of_this_run_is_the_overlays() {
        use crate::updater::{Cache, Release, cache_write};
        let (paths, dir) = paths_in("versionsabgleich");
        assert_eq!(
            version_check_line(&paths, Lang::De),
            "gespeichert nie · neueste bekannte — · dieser Lauf: nachgesehen wird im Overlay, hier nicht bekannt"
        );
        let release =
            |version: &str, prerelease: bool| Release { version: version.into(), prerelease, ..Release::default() };
        let checked = 1_790_000_000.0;
        let releases = vec![release("v5.0.1", false), release("v5.1.0-rc2", true), release("v5.0.0", false)];
        cache_write(&paths, &Cache { checked, releases }).unwrap();
        let when = formats::local_time(1_790_000_000).map(|t| formats::strftime("%d.%m.%Y, %H:%M", &t)).unwrap();
        assert_eq!(
            version_check_line(&paths, Lang::De),
            format!(
                "gespeichert {when} · neueste bekannte v5.0.1 · dieser Lauf: nachgesehen wird im Overlay, hier nicht bekannt"
            )
        );
        assert!(paths.set_setting(crate::updater::PRERELEASE_SETTING, Value::Bool(true)));
        assert!(
            version_check_line(&paths, Lang::En).contains("newest known v5.1.0-rc2 · this run: checked in the overlay")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_overlay_line_says_place_corner_bar_click_through_and_whether_it_runs() {
        let (paths, dir) = paths_in("overlay-zeile");
        assert_eq!(
            overlay_line_with(&paths, Lang::De, false),
            "gemerkter Platz — · Ecke frei · Leiste oben · durchklickbar aus · läuft nicht"
        );
        assert!(crate::overlay::save_place(&paths, crate::overlay::Rect::new(-571, 15, 541, 387)));
        assert!(paths.set_setting(crate::overlay::keys::CORNER, Value::str("unten-rechts")));
        assert!(paths.set_setting(crate::overlay::keys::CLICK_THROUGH, Value::Bool(true)));
        assert_eq!(
            overlay_line_with(&paths, Lang::En, true),
            "remembered place 541x387+-571+15 · corner unten-rechts · bar unten · click-through on · running"
        );
        assert!(crate::overlay::save_size_only(&paths, 440, 300));
        assert!(overlay_line_with(&paths, Lang::De, true).starts_with("gemerkter Platz 440x300 · "));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_new_lines_stand_where_4_12_puts_them_and_go_through_the_redaction() {
        let (paths, dir) = paths_in("neue-zeilen");
        let env = Environment {
            version: "5.0.0".into(),
            overlay: Some("Overlay unter /home/spieler/x".into()),
            basetool: Some(basetool_value(Lang::De, false, &[], "", "")),
            ..Default::default()
        };
        let request =
            Request { paths: &paths, lang: Lang::De, env: &env, reporter: Some(""), message: "", error_count: 8 };
        let facts = collect(&request);
        let labels = |block: usize| facts.blocks[block].entries.iter().map(|e| e.label.clone()).collect::<Vec<_>>();
        let stock = labels(2);
        let names =
            ["Katalogstand", "Patch-Historie", "Mein Lager", "Rezepte", "Bergbaudaten", "Signatur-Scan", "Anti-AFK"];
        let at: Vec<Option<usize>> = names.iter().map(|n| stock.iter().position(|l| l == n)).collect();
        assert!(at.iter().all(Option::is_some), "{stock:?}");
        assert!(at.windows(2).all(|w| w[0] < w[1]), "the order of 4.12: {stock:?}");
        let folder = labels(3);
        let check = folder.iter().position(|l| l == "Versionsabgleich").expect("version check");
        assert_eq!(folder.get(check + 1).map(String::as_str), Some("Basetool"), "{folder:?}");
        assert!(labels(0).contains(&"Overlay".to_string()));
        let text = crate::report::build(&facts, &crate::redact::Redactor::new(&paths));
        assert!(!text.contains("/home/spieler"), "every value goes through the redaction:\n{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_anti_afk_line_reads_the_file_the_overlay_wrote() {
        let (paths, dir) = paths_in("afk");
        let fresh = anti_afk_line(&paths, Lang::De);
        if anti_afk::supported() {
            // The key is the one of the game this computer finds; without one Left Alt + X.
            let key = anti_afk_key::resolve(&paths);
            let expected = format!(
                "aus · an seit — · 0× gesendet · Taste {} ({}) · Joystick-Blick aus (0 Sticks) · Grund — · \
                 letzte Eingabe — · letzter Abbruch —",
                key.input,
                key.origin.key()
            );
            assert_eq!(fresh, expected);
        } else {
            assert_eq!(fresh, "nur unter Windows");
        }
        // What the overlay wrote is what the line reads (the sent count and the last stop do not depend on
        // whether an overlay runs; the "on" does).
        anti_afk::Snapshot {
            on: true,
            sent: 5,
            joystick: true,
            failure: Some(("aafk_err_send".into(), "2026-10-23 08:15".into())),
            ..anti_afk::Snapshot::default()
        }
        .write(&paths)
        .unwrap();
        let line = anti_afk_line(&paths, Lang::De);
        if anti_afk::supported() {
            assert!(line.contains("5× gesendet") && line.contains("aafk_err_send (2026-10-23 08:15)"), "{line}");
        } else {
            assert_eq!(line, "nur unter Windows");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
