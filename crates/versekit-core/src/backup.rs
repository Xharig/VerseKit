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

//! Everything of one's own in **one file**, and back (4.12: `backup.py`).
//!
//! Meant for the change of the computer: one file on a stick, read in on the new computer, play on. The
//! blueprint inventory, both stocks, the mission log, the watchlist, the settings: whatever cannot be
//! fetched anywhere else.
//!
//! ⚠⚠ **Shut out instead of listing.** The nearest way would be a list of the files that come along. A new
//! file of one's own would then drop out without anyone noticing, until the computer is set up again. So
//! the other way round: **everything** comes along except the caches that can be loaded again at any time
//! ([`RELOADABLE`]). The worst case is a somewhat larger file, never a missing inventory.
//!
//! ⚠ **A way back belongs to it.** A backup that can only be written does not solve the change of the
//! computer. [`restore`] is the second half of the same function.
//!
//! ⚠ Names that stay as 4.12 wrote them, because they stand in the backups of the players: the folder
//! `Steuerung/` and the attachment `sicherung.txt` in the archive, and the setting keys of
//! [`PATH_FIELDS`]. The mark in the attachment of [`write`] is new ([`MARKER`]); the backups before an update and at
//! the first start carry the old one (`legacy::BACKUP_MARKER`), the only one 4.12 restores. Both are read.

use std::fs;
use std::path::Path;

use crate::jsonio::{self, Map, Value};
use crate::language::{self, Arg};
use crate::paths::Paths;
use crate::pypath::Flavor;
use crate::text::py_strip;
use crate::{errors, hangar_inbox, joysticks, legacy, start_programs, zip};

/// The mark in the head of the attachment: a backup of this program is recognised by it, even renamed.
pub const MARKER: &str = "Verse-Kit-Sicherung";
/// The attachment in the archive: plain text for the person who finds the file in a year.
pub const INFO_FILE: &str = "sicherung.txt";
/// The prefix in the archive of the controls: they lie in the game folder, not in the data folder.
pub const CONTROLS: &str = "Steuerung/";
/// The setting with the date and time of the last backup that worked.
pub const LAST_SETTING: &str = "letzte_sicherung";

/// What does NOT come along: downloaded reference books and traces of the running program. Together
/// several megabytes, and the program fetches each of them again by itself at the next start.
///
/// ⚠ In doubt something does NOT belong here. A backup too large costs seconds, one too small costs the
/// inventory.
pub const RELOADABLE: [&str; 16] = [
    // v5: what the overlay tells the main window about anti-AFK. A state of the running program, never a
    // setting: a backup that took it along would bring a "on" back to a program that has it off.
    "Intern/anti-afk.json",
    // Unused contract data; may lie in old data folders and belongs in no backup.
    "Intern/bp-contracts-de.json",
    "Intern/bp-contracts-en.json",
    "Intern/crafting-blueprints.json",
    "Intern/katalog-cache.json",
    "Intern/mining-data.json",
    "Intern/orte.json",
    "Intern/preise.json",
    "Intern/scmdb-items.json",
    "Intern/serverstatus.json",
    "Intern/uebersetzung.json",
    "Intern/uebersetzung_abfragen.json",
    "Intern/verkauf.json",
    "Intern/aktionsnamen.json",
    "Intern/versionen.json",
    // ⚠ The reading position expressly does NOT come along: it points to log files of the old computer.
    // On the new one it would be wrong and skip the catching up: the mission log would stay empty.
    "Intern/logstand.json",
];

/// Whole folders that stay outside: the export is written anew at every find, the diagnosis holds the
/// error logs of the old computer.
pub const RELOADABLE_FOLDERS: [&str; 2] = ["export", "Diagnose"];

/// File endings a backup never writes into the data folder: it holds data only. A program file in it would
/// be startable by a path that is read in (game folder, start program).
pub const BLOCKED_SUFFIXES: [&str; 40] = [
    // More that Windows starts or loads (added in v5; 4.12 had the shorter list).
    ".inf",
    ".scf",
    ".msc",
    ".application",
    ".gadget",
    ".ps1xml",
    ".psc1",
    ".xll",
    ".appx",
    ".msix",
    ".ocx",
    ".sys",
    ".exe",
    ".dll",
    ".bat",
    ".cmd",
    ".com",
    ".scr",
    ".msi",
    ".msp",
    ".ps1",
    ".psm1",
    ".vbs",
    ".vbe",
    ".js",
    ".jse",
    ".wsf",
    ".wsh",
    ".hta",
    ".cpl",
    ".lnk",
    ".url",
    ".pif",
    ".reg",
    ".sh",
    ".appimage",
    ".desktop",
    ".py",
    ".pyw",
    ".jar",
];

/// Settings that name a place on the disk. At the change of the computer they are the most likely reason
/// why nothing works afterwards. `launcher_ordner` is not read any more; it stands here so that old
/// backups do not carry it along.
pub const PATH_FIELDS: [&str; 3] = ["spiel_ordner", "launcher_ordner", "export_ordner"];

/// The files that are read as settings, wherever they lie in the backup.
const SETTINGS_NAMES: [&str; 2] = ["einstellungen.json", "einstellungen.bak.json"];

/// The file in the data folder that holds the settings.
const SETTINGS_FOLDER: &str = "Einstellungen";

/// A file of the controls to go into the backup: the full path, and the name in the archive (begins with
/// [`CONTROLS`]). The controls come from the game folder; the module that knows them hands them in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub full: String,
    pub name: String,
}

/// Why no backup was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// There is nothing of one's own to put in.
    Empty,
    /// The reason, for the player.
    Failed(String),
}

/// Why no backup was restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreError {
    /// The file is not a backup of this program.
    Invalid,
    /// The reason, for the player.
    Failed(String),
}

/// What a file turned out to be before it is restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The number of files in it (the attachment not counted).
    pub count: usize,
    /// When it was made, as the attachment says; empty if it does not say.
    pub when: String,
}

/// What a restore did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restored {
    pub count: usize,
    /// The copy of the state of before, next to the data folder; `None` if it could not be written.
    pub fallback: Option<String>,
    /// What was switched off or not taken over on the way: the names of start programs, then the keys of
    /// the settings without a switch.
    pub disarmed: Vec<String>,
}

/// Does this file (path relative to the data folder) belong in the backup?
///
/// Without regard to case: under Windows `intern/Versionen.json` is the same file as
/// `Intern/versionen.json`.
pub fn include(rel: &str) -> bool {
    let path = rel.replace('\\', "/").to_lowercase();
    if RELOADABLE.iter().any(|name| name.to_lowercase() == path) {
        return false;
    }
    let first = path.split('/').next().unwrap_or("");
    if RELOADABLE_FOLDERS.iter().any(|name| name.to_lowercase() == first) {
        return false;
    }
    // The backup itself not into the backup, if somebody puts it in the data folder; nor the files that are
    // only there while something is written (they may be gone when they are read).
    !(path.ends_with(".zip") || path.ends_with(".tmp") || path.ends_with(".teil"))
}

/// The names of [`RELOADABLE`] without their folder, in lower case.
fn reloadable_names() -> Vec<String> {
    RELOADABLE.iter().map(|name| name.rsplit('/').next().unwrap_or(name).to_lowercase()).collect()
}

/// All files of the data folder that come along: full path, name in the file; sorted by the name.
pub fn files(root: &str) -> std::io::Result<Vec<(String, String)>> {
    fn walk(folder: &Path, rel: &str, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
        let mut names: Vec<_> = Vec::new();
        for entry in fs::read_dir(folder)? {
            names.push(entry?);
        }
        for entry in names {
            let name = entry.file_name().to_string_lossy().into_owned();
            let full = entry.path();
            let below = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let kind = entry.file_type()?;
            // Like `os.walk`: a folder is entered, a link to a folder is not (and is no file either).
            if kind.is_dir() {
                walk(&full, &below, out)?;
            } else if kind.is_symlink() && fs::metadata(&full).is_ok_and(|m| m.is_dir()) {
                continue;
            } else if include(&below) {
                out.push((full.to_string_lossy().into_owned(), below));
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(Path::new(root), "", &mut found)?;
    found.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(found)
}

/// The time of the last backup that worked, as text; empty if there is none.
pub fn last_written(paths: &Paths) -> String {
    paths.setting(LAST_SETTING).unwrap_or_default()
}

/// A file name with the date: a backup keeps one state.
pub fn suggestion() -> String {
    let (year, month, day, ..) = jsonio::local_time();
    format!("Verse-Kit-Sicherung-{year:04}-{month:02}-{day:02}.zip")
}

/// The attachment: plain text, not identifiers. It lands with the player, so it is in the language of the
/// window.
fn info_text(marker: &str, version: &str, count: usize) -> String {
    let (year, month, day, hour, minute, _) = jsonio::local_time();
    let when = format!("{day:02}.{month:02}.{year:04} {hour:02}:{minute:02}");
    let version = if version.is_empty() { "?" } else { version };
    let body = language::tf(
        "sich_datei_info",
        &[("0", Arg::from(when)), ("1", Arg::from(version.to_string())), ("2", Arg::from(count))],
    );
    format!("{marker}\r\n\r\n{}\r\n", body.replace('\n', "\r\n"))
}

/// Writes everything of one's own into a zip file.
///
/// `bindings` are the files of the controls (from the game folder) that come along; with none the list is
/// empty, which is no error.
///
/// # Errors
///
/// [`WriteError::Empty`] when there is nothing to put in, [`WriteError::Failed`] with the reason.
pub fn write(paths: &Paths, target: &str, version: &str, bindings: &[Binding]) -> Result<usize, WriteError> {
    let count = write_marked(paths, target, version, bindings, MARKER)?;
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    paths.set_setting(
        LAST_SETTING,
        Value::str(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")),
    );
    Ok(count)
}

/// The folder in `Intern` with the backups written before an update.
pub const BEFORE_UPDATE_FOLDER: &str = "vor-update";
/// How many backups before an update are kept.
pub const BEFORE_UPDATE_KEEP: usize = 2;

/// A backup of one's own data right before an update, without the controls, into `Intern/vor-update/`; the
/// last [`BEFORE_UPDATE_KEEP`] stay. ⚠ It carries the mark of 4.12 (`legacy::BACKUP_MARKER`): 4.12 reads only
/// that one, and the way back to it stays open in 5.0. It does not count as the player's last backup. Returns
/// the file written.
///
/// # Errors
///
/// As [`write`]; the caller does not stop the update for it.
pub fn write_before_update(paths: &Paths, version: &str) -> Result<String, WriteError> {
    let folder = paths.app_file(BEFORE_UPDATE_FOLDER);
    jsonio::make_dirs(&folder).map_err(|e| WriteError::Failed(e.to_string()))?;
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    let version = if version.is_empty() { "unbekannt" } else { version };
    let name = format!("vor-update-{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}-{version}.zip");
    let target = Path::new(&folder).join(name).to_string_lossy().into_owned();
    write_marked(paths, &target, version, &[], legacy::BACKUP_MARKER)?;
    let mut written: Vec<_> = fs::read_dir(&folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("vor-update-") && n.ends_with(".zip"))
                .collect()
        })
        .unwrap_or_default();
    written.sort();
    let surplus = written.len().saturating_sub(BEFORE_UPDATE_KEEP);
    for old in &written[..surplus] {
        let _ = jsonio::remove_file(&Path::new(&folder).join(old).to_string_lossy());
    }
    Ok(target)
}

/// Writes the files of the data folder and `bindings` into a zip file whose attachment carries `marker`.
pub(crate) fn write_marked(
    paths: &Paths,
    target: &str,
    version: &str,
    bindings: &[Binding],
    marker: &str,
) -> Result<usize, WriteError> {
    let root = paths.app_folder();
    let mut files = match files(&root) {
        Ok(files) => files,
        Err(error) => {
            errors::record("backup.write", &error);
            return Err(WriteError::Failed(error.to_string()));
        }
    };
    if files.is_empty() {
        return Err(WriteError::Empty);
    }
    files.extend(bindings.iter().map(|b| (b.full.clone(), b.name.clone())));
    // ⚠ First next to the target, then rename. If the writing breaks off (stick pulled, disk full), a half
    // backup would stand there that looks like a whole one.
    let temp = format!("{target}.teil");
    let result = (|| -> Result<usize, String> {
        let mut archive = zip::Builder::new(jsonio::local_time());
        let mut count = 0;
        for (full, name) in &files {
            let content = match fs::read(full) {
                Ok(content) => content,
                // A file that was there when the folder was listed and is gone now (a trace of the running
                // program): nothing to keep.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            archive.add(name, &content).map_err(|e| e.to_string())?;
            count += 1;
        }
        archive.add(INFO_FILE, info_text(marker, version, count).as_bytes()).map_err(|e| e.to_string())?;
        let bytes = archive.finish().map_err(|e| e.to_string())?;
        jsonio::write_new(&temp, &bytes).map_err(|e| e.to_string())?;
        jsonio::replace_file(&temp, target).map_err(|e| e.to_string())?;
        Ok(count)
    })();
    match result {
        Ok(count) => Ok(count),
        Err(message) => {
            errors::record("backup.write", &message);
            let _ = jsonio::remove_file(&temp);
            Err(WriteError::Failed(message))
        }
    }
}

/// The file as bytes, if it is not unreasonably large for a backup.
fn read_archive(source: &str) -> Result<Vec<u8>, String> {
    // A backup holds data of one's own, a few megabytes; whoever picks a film by mistake is told so
    // without reading it all.
    const LARGEST: u64 = 1 << 30;
    let size = fs::metadata(source).map_err(|e| e.to_string())?.len();
    if size > LARGEST {
        return Err("file too large".to_string());
    }
    fs::read(source).map_err(|e| e.to_string())
}

/// What lies in this file? `None` if it is no backup of this program.
///
/// ⚠ **Ask before restoring, not after.** Whoever picks a foreign or broken zip file should learn that
/// before the inventory is overwritten.
pub fn check(source: &str) -> Option<Found> {
    let data = match read_archive(source) {
        Ok(data) => data,
        Err(error) => {
            errors::record("backup.check", &error);
            return None;
        }
    };
    // Not a zip file at all: the player picked the wrong file, which is no fault of the program.
    let entries = zip::entries(&data).ok()?;
    let names: Vec<&zip::Entry> = entries.iter().filter(|e| !e.name.ends_with('/')).collect();
    // Where a name stands twice, the last one counts (as a zip reader gives it by name).
    let info = names.iter().rev().find(|e| e.name == INFO_FILE)?;
    let head = String::from_utf8_lossy(&zip::read(&data, info).ok()?).into_owned();
    if !head.contains(MARKER) && !head.contains(legacy::BACKUP_MARKER) {
        return None;
    }
    Some(Found { count: names.len() - 1, when: made_at(&head) })
}

/// What of the controls lies in this backup, by name: whether the active assignment is in it, and the names
/// of the saved profiles (sorted without regard to case). Nothing when the file cannot be read.
///
/// So that the player sees BEFORE what would be restored.
pub fn bindings_in_archive(source: &str) -> (bool, Vec<String>) {
    let Ok(data) = read_archive(source) else { return (false, Vec::new()) };
    let Ok(entries) = zip::entries(&data) else { return (false, Vec::new()) };
    let (mut active, mut profiles) = (false, Vec::new());
    for entry in &entries {
        let Some(rest) = entry.name.strip_prefix(CONTROLS) else { continue };
        if entry.name.ends_with('/') {
            continue;
        }
        if rest == "actionmaps.xml" {
            active = true;
        } else if rest.starts_with("mappings/") && rest.ends_with(".xml") {
            let base = Flavor::native().basename(rest);
            profiles.push(base[..base.len() - 4].to_string());
        }
    }
    profiles.sort_by_key(|name| name.to_lowercase());
    (active, profiles)
}

/// The active binding in the archive, after [`CONTROLS`].
const ACTIVE_NAME: &str = "actionmaps.xml";
/// The settings of the game that lie next to the active binding, after [`CONTROLS`].
const GAME_SETTINGS_NAME: &str = "attributes.xml";
/// The folder of the saved profiles in the archive, after [`CONTROLS`].
const PROFILES_FOLDER: &str = "mappings/";

/// The files of the controls that come along, from the game folder (`folder`, or the one that is set up):
/// the active `actionmaps.xml` with the `attributes.xml` beside it, and every saved profile of every spelling of
/// the `mappings` folder (4.12: `_binding_files`). Sorted by the name in the archive; empty without a game
/// folder, which is no error.
///
/// ⚠ **A backup takes along what can be reached.** Lost controls are hours of work, so the saved profiles of
/// all the spellings of the folder are collected, not only the one a new profile would be written to. Files of the
/// same name are told apart by the time: the newer one wins, so that the same binding does not lie twice in the
/// archive and chance decides on the way back.
pub fn bindings(paths: &Paths, folder: Option<&str>) -> Vec<Binding> {
    let active = joysticks::actionmaps_path(paths, folder).filter(|path| Path::new(path).is_file());
    // ⚠ Over **all** spellings of the folder, not only the one that would be written to.
    collect_bindings(active, &joysticks::all_mapping_folders(paths, folder), paths.host.flavor)
}

/// [`bindings`] with the active binding and the folders of the profiles given (newest folder first).
fn collect_bindings(active: Option<String>, mapping_folders: &[String], flavor: Flavor) -> Vec<Binding> {
    // The name after `Steuerung/`, and the full path.
    let mut found: Vec<(String, String)> = Vec::new();
    if let Some(active) = active {
        // The settings of the game lie in the same folder; found over the path of the binding so that the case of
        // `USER` or `user`, `Client` or `client` is right.
        let neighbour = flavor.join_all(&flavor.dirname(&active), &[GAME_SETTINGS_NAME]);
        found.push((ACTIVE_NAME.to_string(), active));
        if Path::new(&neighbour).is_file() {
            found.push((GAME_SETTINGS_NAME.to_string(), neighbour));
        }
    }
    for dir in mapping_folders {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else { continue };
            if !name.to_lowercase().ends_with(".xml") {
                continue;
            }
            let full = entry.path();
            if !full.is_file() {
                continue;
            }
            let full = full.to_string_lossy().into_owned();
            let key = format!("{PROFILES_FOLDER}{name}");
            if let Some((_, previous)) = found.iter().find(|(known, _)| *known == key) {
                let time = |path: &str| crate::paths::mtime_seconds(path).unwrap_or(0.0);
                if time(previous) >= time(&full) {
                    continue;
                }
            }
            found.retain(|(known, _)| *known != key);
            found.push((key, full));
        }
    }
    let mut list: Vec<Binding> =
        found.into_iter().map(|(rel, full)| Binding { full, name: format!("{CONTROLS}{rel}") }).collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Why the controls of a backup were not put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingsError {
    /// The file is not a backup of this program.
    Invalid,
    /// The game runs, or nobody can tell: nothing was written. The reason, for the player.
    GameNotQuiet(String),
    /// The reason, for the player.
    Failed(String),
}

/// What [`restore_bindings`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingsRestored {
    /// The files written into the game folder.
    pub count: usize,
    /// The files of the backup that were left out: no place for them here, or no binding of the game.
    pub skipped: usize,
    /// The copy of the active binding that was put aside before it was replaced.
    pub fallback: Option<String>,
}

/// May a profile of this name be written into the folder of the mappings? Only a plain `.xml` name with
/// something before it; under Windows none that the system reads another way (see [`restore_rel`]).
fn profile_name_ok(name: &str, flavor: Flavor) -> bool {
    let lower = name.to_lowercase();
    lower.len() > ".xml".len()
        && lower.ends_with(".xml")
        && !name.contains(':')
        && !(flavor == Flavor::Windows && windows_reads_differently(name))
}

/// Is this text XML whose root is `root` (`None`: any root)? A mark of the byte order is allowed.
fn xml_of(content: &[u8], root: Option<&str>) -> bool {
    let Ok(text) = std::str::from_utf8(content) else { return false };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    crate::joysticks_write::parse(text).is_some_and(|document| {
        root.is_none_or(|name| crate::joysticks_write::is_element(document.root_element(), name))
    })
}

/// Puts the saved controls of a backup back into the game folder (4.12: `restore_bindings`).
///
/// ⚠⚠ **Apart from the rest of the restore, and with intent more awkward.** To add the saved profiles is harmless:
/// they lie there until the player loads one. To replace the **active** binding is not: whoever slips sits in a ship
/// that answers nothing. So it comes only with `with_active`, together with the `attributes.xml` (view angle and
/// graphics belong to the active state, not to the profiles), and the old one is put aside first
/// (`game_files::write` makes the copy).
///
/// ⚠ Nothing is written while the game runs, or when nobody can tell: the game writes the active binding itself
/// when it ends, and a game that has a file open reads it again. Looked at before the first file is written.
///
/// Only names that are plain files go into the folder of the mappings (a name from the archive must not lead out
/// of it), and only what is XML (the binding files: with the root of the game). A file that is none is left out and
/// counted in `skipped`. Nothing is written where there is no place for it: without a game folder with a binding
/// there is no folder for the profiles, as in 4.12.
///
/// # Errors
///
/// [`BindingsError::Invalid`] where the file is no backup of this program, [`BindingsError::GameNotQuiet`] where the
/// game may run, [`BindingsError::Failed`] with the reason where reading or writing broke off.
pub fn restore_bindings(
    paths: &Paths,
    source: &str,
    with_active: bool,
    folder: Option<&str>,
) -> Result<BindingsRestored, BindingsError> {
    use crate::game_files::{self, WriteError};
    if check(source).is_none() {
        return Err(BindingsError::Invalid);
    }
    let data = read_archive(source).map_err(BindingsError::Failed)?;
    let entries = zip::entries(&data).map_err(|e| BindingsError::Failed(e.to_string()))?;
    let flavor = paths.host.flavor;
    // Where a name stands twice, the last one counts (as a zip reader gives it by name).
    let latest: std::collections::HashMap<&str, &zip::Entry> =
        entries.iter().map(|entry| (entry.name.as_str(), entry)).collect();
    enum Kind {
        Active,
        GameSettings,
        Profile(String),
    }
    // ⚠ First read and check EVERYTHING, then write: a damaged entry must not leave half of the controls replaced.
    let mut planned: Vec<(Kind, Vec<u8>)> = Vec::new();
    let mut skipped = 0;
    let mut seen: Vec<&str> = Vec::new();
    for entry in &entries {
        let Some(rest) = entry.name.strip_prefix(CONTROLS) else { continue };
        if entry.name.ends_with('/') || seen.contains(&entry.name.as_str()) {
            continue;
        }
        let kind = if rest == ACTIVE_NAME {
            if !with_active {
                continue;
            }
            Kind::Active
        } else if rest == GAME_SETTINGS_NAME {
            // ⚠ It belongs to the active state, not to the profiles: without the active binding it stays in the archive.
            if !with_active {
                continue;
            }
            Kind::GameSettings
        } else if rest.starts_with(PROFILES_FOLDER) {
            // ⚠ Only the plain file name: a path from the archive must not lead out of the folder of the mappings.
            let name = flavor.basename(rest);
            if !profile_name_ok(&name, flavor) {
                skipped += 1;
                continue;
            }
            Kind::Profile(name)
        } else {
            continue;
        };
        seen.push(entry.name.as_str());
        let content = zip::read(&data, latest[entry.name.as_str()]).map_err(|e| {
            errors::record("backup.restore_bindings", &e);
            BindingsError::Failed(e.to_string())
        })?;
        let fine = match &kind {
            Kind::GameSettings => xml_of(&content, None),
            Kind::Active | Kind::Profile(_) => xml_of(&content, Some("ActionMaps")),
        };
        if fine {
            planned.push((kind, content));
        } else {
            skipped += 1;
        }
    }
    if planned.is_empty() {
        return Ok(BindingsRestored { count: 0, skipped, fallback: None });
    }
    // Before the first file is written, not in the middle.
    let refused = |error: WriteError| match error {
        WriteError::Failed(io) => BindingsError::Failed(io.to_string()),
        quiet => BindingsError::GameNotQuiet(quiet.reason()),
    };
    game_files::check_quiet(paths).map_err(refused)?;

    let active_target = joysticks::actionmaps_path(paths, folder);
    // The folder of the profiles: the newest of the spellings; where there is none, next to the active binding, so
    // that the spelling fits the installation (`…/0/Profiles/default/actionmaps.xml` gives `…/0/controls/mappings`).
    let profile_folder = joysticks::all_mapping_folders(paths, folder).into_iter().next().or_else(|| {
        let active = active_target.as_deref()?;
        let zero = flavor.dirname(&flavor.dirname(&flavor.dirname(active)));
        Some(flavor.join_all(&zero, &["controls", "mappings"]))
    });
    let (mut count, mut fallback) = (0, None);
    for (kind, content) in planned {
        let target = match &kind {
            Kind::Active => active_target.clone(),
            Kind::GameSettings => {
                active_target.as_deref().map(|active| flavor.join_all(&flavor.dirname(active), &[GAME_SETTINGS_NAME]))
            }
            Kind::Profile(name) => profile_folder.as_deref().map(|dir| flavor.join_all(dir, &[name])),
        };
        let Some(target) = target else {
            skipped += 1;
            continue;
        };
        if let Some(parent) = Path::new(&target).parent() {
            fs::create_dir_all(parent).map_err(|e| BindingsError::Failed(e.to_string()))?;
        }
        let copy = game_files::write(paths, &target, &content).map_err(|error| {
            if let WriteError::Failed(io) = &error {
                errors::record("backup.restore_bindings", io);
            }
            refused(error)
        })?;
        if matches!(kind, Kind::Active) {
            fallback = copy;
        }
        count += 1;
    }
    Ok(BindingsRestored { count, skipped, fallback })
}

/// The date the attachment names ("Erstellt am … mit …", or in English "Created on … with …").
fn made_at(head: &str) -> String {
    const LINE_ENDS: [char; 10] =
        ['\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}'];
    for line in head.split(LINE_ENDS) {
        for (start, middle) in [("Erstellt am ", " mit "), ("Created on ", " with ")] {
            if let Some(rest) = line.strip_prefix(start) {
                return rest.split(middle).next().unwrap_or("").to_string();
            }
        }
    }
    String::new()
}

/// Where a name in the archive is written, relative to the data folder (with `/`), or `None` if it is not.
///
/// The rules, in the order of 4.12: folders and the attachment never; the controls never (they have their
/// own way, and a wrong `actionmaps.xml` costs the player the whole control); no path may lead out of the
/// data folder (a zip file can hold `../../`, "Zip Slip"); a data stream (`name:stream`) is no file;
/// nothing the backup does not take along itself ([`include`], the reloadable names, program files).
fn restore_rel(name: &str, flavor: Flavor) -> Option<String> {
    if name.ends_with('/') || name == INFO_FILE || name.starts_with(CONTROLS) {
        return None;
    }
    // Under Windows both separators are the same.
    let name = if flavor == Flavor::Windows { name.replace('\\', "/") } else { name.to_string() };
    // A rooted name leads out (and with a drive letter, to another drive).
    if name.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return None;
    }
    let rel = parts.join("/");
    // Windows cuts dots and blanks at the end of a name (`x.exe.` becomes `x.exe`), and `name:stream`
    // writes into a data stream (`x.exe::$DATA` is `x.exe`).
    if rel.contains(':') {
        return None;
    }
    // ⚠ Windows reads a name another way than it is written: a blank or dot at the end goes (`einstellungen.json `
    // is the file `einstellungen.json`), `EINSTE~1.JSO` can be a short name of a file, `CON` or `NUL` is a
    // device, `?` `*` `<` `>` `|` `"` are no letters of a name. A file of the settings is cleaned by its name:
    // under another spelling it would land on the real file uncleaned. So under Windows such names are not
    // restored at all (4.12 let them through).
    if flavor == Flavor::Windows && parts.iter().any(|part| windows_reads_differently(part)) {
        return None;
    }
    // The controls under another spelling (`steuerung\...`, `STEUERUNG/...`) are the controls too.
    if rel.to_lowercase().starts_with(&CONTROLS.to_lowercase()) {
        return None;
    }
    let cut = rel.to_lowercase();
    let cut = cut.trim_end_matches(['.', ' ']);
    if !include(&rel) || BLOCKED_SUFFIXES.iter().any(|suffix| cut.ends_with(suffix)) {
        return None;
    }
    // Nor the reloadable ones from a subfolder: loose files are sorted into place by the data folder.
    let base = rel.rsplit('/').next().unwrap_or(&rel).to_lowercase();
    if reloadable_names().contains(&base) {
        return None;
    }
    Some(rel)
}

/// Does Windows read this part of a path as something else than it is written (see [`restore_rel`])?
fn windows_reads_differently(part: &str) -> bool {
    if part.ends_with(['.', ' ']) && part != ".." {
        return true;
    }
    if part.chars().any(|c| c < ' ' || matches!(c, '<' | '>' | '"' | '|' | '?' | '*')) {
        return true;
    }
    // A short name (`EINSTE~1.JSO`): a tilde and a digit.
    let chars: Vec<char> = part.chars().collect();
    if chars.windows(2).any(|pair| pair[0] == '~' && pair[1].is_ascii_digit()) {
        return true;
    }
    // A device, with or without an ending (`CON`, `nul.txt`, `COM1`, `LPT3.log`).
    let stem = part.split('.').next().unwrap_or(part).trim_end().to_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.chars().last().is_some_and(|c| ('1'..='9').contains(&c)))
}

/// Is this an absolute path on a local drive, outside the data folder?
///
/// ⚠ Under Windows `/` and `\` are the same: `\\/host/share`, `//host/share` and `\\?\UNC\host\share` are
/// all network paths. So it is checked positively: the drive part must be exactly `X:`.
fn local_path(text: &str, root: &str, paths: &Paths) -> bool {
    let flavor = paths.host.flavor;
    let full = flavor.normpath(&paths.host.expanduser(text));
    if flavor == Flavor::Windows {
        let (drive, root_part, _) = flavor.splitroot(&full);
        let letters: Vec<char> = drive.chars().collect();
        if letters.len() != 2 || !letters[0].is_alphabetic() || letters[1] != ':' || root_part.is_empty() {
            return false;
        }
    } else if !flavor.isabs(&full) || full.starts_with("//") {
        return false;
    }
    !inside(&full, root, paths)
}

/// Does `path` point into the data folder (or is it that folder)?
fn inside(path: &str, root: &str, paths: &Paths) -> bool {
    let flavor = paths.host.flavor;
    let full = flavor.normcase(&flavor.abspath(&paths.host.expanduser(path), &paths.host.cwd));
    // The data folder as it really is: a trailing separator or a relative name must not hide that the game
    // lies inside it.
    let base = flavor.normcase(&flavor.abspath(root, &paths.host.cwd));
    full == base || full.starts_with(&format!("{base}{}", flavor.sep()))
}

/// A settings file of a backup, made safe: the bytes to write, and the names that were switched off. `None`
/// if it cannot be read (then it is not restored).
///
/// ⚠⚠ A backup may come from somebody else, and these values go unchanged to the system or decide what is
/// started. So: what runs a program is treated by [`start_programs::safe_import`]; `ablage_ordner` always
/// falls away; and a path field that does not point to a local drive outside the data folder is emptied
/// (the game folder decides which launcher starts).
pub fn safe_settings(content: &[u8], before: &Map, root: &str, paths: &Paths) -> Option<(Vec<u8>, Vec<String>)> {
    let text = std::str::from_utf8(content).ok()?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Value::Object(mut data) = jsonio::loads(text).ok()? else { return None };
    let turned_off = start_programs::safe_import(&mut data, before);
    data.retain(|(key, _)| key != "ablage_ordner");
    // The receiver of the hangar opens a port on this computer and its pairing code is known to whoever made
    // the backup: whatever a backup says about it, the setting here holds (4.12 had no receiver).
    let previous = |key: &str| before.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    for key in [hangar_inbox::SETTING_ON, hangar_inbox::SETTING_PORT, hangar_inbox::SETTING_TOKEN] {
        match previous(key) {
            Some(value) => jsonio::map_set(&mut data, key, value),
            None => data.retain(|(k, _)| k != key),
        }
    }
    // The folders of the channels: each one by the rule of the game folder (only a local drive, outside the
    // data folder); what does not hold goes (a network path asked for would hand out a login).
    if let Some(slot) = data.iter().position(|(key, _)| key == crate::paths::SETTING_CHANNEL_FOLDERS) {
        match &mut data[slot].1 {
            Value::Object(folders) => {
                for (_, value) in folders.iter_mut() {
                    let keep = matches!(value, Value::Str(text)
                        if py_strip(text).is_empty() || local_path(py_strip(text), root, paths));
                    if !keep {
                        *value = Value::str("");
                    }
                }
            }
            Value::Null => {}
            other => *other = Value::Object(Vec::new()),
        }
    }
    for field in PATH_FIELDS {
        let Some(slot) = data.iter().position(|(key, _)| key == field) else { continue };
        match &data[slot].1 {
            Value::Str(value) => {
                let text = py_strip(value);
                if !text.is_empty() && !local_path(text, root, paths) {
                    data[slot].1 = Value::str("");
                }
            }
            Value::Null => {}
            _ => data[slot].1 = Value::str(""),
        }
    }
    Some((jsonio::dumps(&Value::Object(data), Some(1), false).into_bytes(), turned_off))
}

/// Writes a file next to its place and renames it: no file is ever shown half written (the settings are read
/// by other threads all the time, and a restore that breaks off must not leave a cut inventory).
fn write_atomic(target: &str, content: &[u8]) -> std::io::Result<()> {
    let _lock = jsonio::lock_saves();
    let temp = jsonio::temp_name(target);
    let result = jsonio::write_new(&temp, content).and_then(|()| jsonio::replace_file(&temp, target));
    if result.is_err() {
        let _ = jsonio::remove_file(&temp);
    }
    result
}

/// Restores a backup.
///
/// ⚠⚠ **The state of before is put aside first.** Whoever slips has lost both otherwise: the old backup
/// not restored, and the own inventory overwritten. The copy lies next to the data folder and carries date
/// and time.
///
/// ⚠ The program must start anew afterwards: the modules hold their data in memory and would write them
/// over the files just restored at the next save.
///
/// # Errors
///
/// [`RestoreError::Invalid`] when the file is no backup of this program, [`RestoreError::Failed`] when the
/// writing broke off.
pub fn restore(paths: &Paths, source: &str, bindings: &[Binding]) -> Result<Restored, RestoreError> {
    let flavor = paths.host.flavor;
    let root = paths.app_folder();
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    let folder = flavor.dirname(root.trim_end_matches(flavor.sep()));
    let fallback = flavor.join(
        if folder.is_empty() { &root } else { &folder },
        &format!("Verse-Kit-vorher-{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}.zip"),
    );
    restore_with_copy(paths, source, bindings, &fallback)
}

/// [`restore`] with the file named in which the state of before is put aside.
fn restore_with_copy(
    paths: &Paths,
    source: &str,
    bindings: &[Binding],
    fallback: &str,
) -> Result<Restored, RestoreError> {
    let Some(found) = check(source) else { return Err(RestoreError::Invalid) };
    let data = read_archive(source).map_err(RestoreError::Failed)?;
    let entries = zip::entries(&data).map_err(|e| RestoreError::Failed(e.to_string()))?;
    let flavor = paths.host.flavor;
    let root = paths.app_folder();
    let before = paths.settings();
    // ⚠ Without the copy of before nothing is overwritten: that copy is what the question promised. (Nothing
    // to copy, an empty data folder, is no reason to stop.)
    let previous = match write(paths, fallback, "", bindings) {
        Err(WriteError::Failed(why)) => {
            errors::record("backup.restore", &why);
            return Err(RestoreError::Failed(why));
        }
        other => other.is_ok(),
    };
    let mut disarmed = Vec::new();
    // Where a name stands twice, the last one counts (as a zip reader gives it by name).
    let latest: std::collections::HashMap<&str, &zip::Entry> =
        entries.iter().map(|entry| (entry.name.as_str(), entry)).collect();
    // ⚠ First read and clean EVERYTHING, then write: a damaged entry (its checksum is checked when it is read)
    // must not leave half of the files replaced and half not.
    let mut planned: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in &entries {
        let Some(rel) = restore_rel(&entry.name, flavor) else { continue };
        let content = match zip::read(&data, latest[entry.name.as_str()]) {
            Ok(content) => content,
            Err(error) => {
                let _ = jsonio::remove_file(fallback);
                errors::record("backup.restore", &error);
                return Err(RestoreError::Failed(error.to_string()));
            }
        };
        let is_settings = SETTINGS_NAMES.contains(&rel.rsplit('/').next().unwrap_or(&rel).to_lowercase().as_str());
        // Every settings file is cleaned BEFORE it lands on the disk: if the restore breaks off after
        // it, no live version lies around.
        let content = if is_settings {
            match safe_settings(&content, &before, &root, paths) {
                Some((cleaned, turned_off)) => {
                    // Each name once, even where several settings files in the backup say it.
                    for name in turned_off {
                        if !disarmed.contains(&name) {
                            disarmed.push(name);
                        }
                    }
                    cleaned
                }
                None => continue,
            }
        } else {
            content
        };
        planned.push((flavor.join(&root, &rel.replace('/', &flavor.sep().to_string())), content));
    }
    let outcome = (|| -> Result<(), String> {
        for (target, content) in &planned {
            jsonio::make_dirs(&flavor.dirname(target)).map_err(|e| e.to_string())?;
            write_atomic(target, content).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    // What was switched off is told even where the restore broke off halfway.
    paths.forget_settings();
    if let Err(message) = outcome {
        errors::record("backup.restore", &message);
        return Err(RestoreError::Failed(message));
    }
    clear_foreign_paths(paths, &root);
    paths.forget_settings();
    Ok(Restored { count: found.count, fallback: previous.then(|| fallback.to_string()), disarmed })
}

/// Removes the paths of the old computer where they lead nowhere here.
///
/// ⚠⚠ That is what this is for: the change of the computer. On the old one the game may have stood in
/// `D:\Spiele`, here it lies somewhere else. A path that leads nowhere is worse than none: an empty field
/// lets the program search by itself, a wrong one does not, and nobody thinks of the setting just restored.
///
/// What exists here stays: whoever restores on the same computer keeps the paths. `ablage_ordner` is
/// ALWAYS removed: it stands in the pointer file, not here, and an older backup may still hold it; it would
/// make the program look elsewhere than where the backup was just restored.
fn clear_foreign_paths(paths: &Paths, root: &str) {
    let flavor = paths.host.flavor;
    let target = flavor.join(&flavor.join(root, SETTINGS_FOLDER), crate::paths::SETTINGS_FILE);
    if !Path::new(&target).is_file() {
        return;
    }
    let Ok(Value::Object(mut data)) = jsonio::read_json(&target) else { return };
    let before = data.len();
    data.retain(|(key, _)| key != "ablage_ordner");
    let mut changed = data.len() != before;
    for field in PATH_FIELDS {
        let Some(slot) = data.iter().position(|(key, _)| key == field) else { continue };
        if let Value::Str(value) = &data[slot].1
            && !py_strip(value).is_empty()
            && !Path::new(&paths.host.expanduser(value)).exists()
        {
            data[slot].1 = Value::str("");
            changed = true;
        }
    }
    if changed && let Err(error) = jsonio::save_json(&target, &Value::Object(data), 1, false) {
        errors::record("backup.clear_foreign_paths", &error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;

    /// A throwaway home: the data folder lies in it, in the real layout (with its subfolders).
    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-sicherung-{}-{name}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let host = Host::new(Flavor::native(), &[("HOME", &text), ("USERPROFILE", &text), ("APPDATA", &text)], &text);
        let paths = Paths::new(host);
        let root = paths.app_folder();
        assert!(root.starts_with(&text), "the data folder lies in the throwaway home: {root}");
        (paths, dir)
    }

    fn put(paths: &Paths, rel: &str, content: &str) {
        let target = Path::new(&paths.app_folder()).join(rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, content).unwrap();
    }

    fn tree(paths: &Paths) -> Vec<String> {
        files(&paths.app_folder()).unwrap().into_iter().map(|(_, rel)| rel).collect()
    }

    #[test]
    fn what_comes_along_is_everything_except_what_can_be_loaded_again() {
        for yes in ["Bestand/bauplaene.json", "Einstellungen/einstellungen.json", "neu/Datei.json", "Intern/eigen.json"]
        {
            assert!(include(yes), "{yes}");
        }
        for no in [
            "Intern/katalog-cache.json",
            "intern/Versionen.json",
            "Intern\\preise.json",
            "Intern/logstand.json",
            // The state of anti-AFK that the overlay writes for the main window: of a run, never a setting.
            "Intern/anti-afk.json",
            "export/SC-BP-Watcher-Bestand.json",
            "Diagnose/fehler.txt",
            "DIAGNOSE\\x",
            "alt/Sicherung.zip",
            "x.ZIP",
        ] {
            assert!(!include(no), "{no}");
        }
    }

    #[test]
    fn a_name_in_the_archive_is_written_only_where_it_may() {
        let w = Flavor::Windows;
        for (name, expected) in [
            ("Bestand/bauplaene.json", Some("Bestand/bauplaene.json")),
            ("./Intern/x.json", Some("Intern/x.json")),
            ("Bauplaene/../Intern/x.json", Some("Intern/x.json")),
            ("Intern\\x.json", Some("Intern/x.json")),
            ("../x.json", None),
            ("a/../../x.json", None),
            ("/abs/x.json", None),
            ("\\abs\\x.json", None),
            ("C:/x.json", None),
            ("C:x.json", None),
            ("x.json:strom", None),
            ("Bestand/", None),
            ("", None),
            (".", None),
            (INFO_FILE, None),
            ("Steuerung/actionmaps.xml", None),
            ("Intern/katalog-cache.json", None),
            ("Anderswo/preise.json", None),
            ("Anderswo/Logstand.JSON", None),
            ("Anderswo/Anti-AFK.json", None),
            ("tool.exe", None),
            ("tool.exe.", None),
            ("tool.exe . .", None),
            ("Bestand/skript.PS1", None),
            ("Bestand/x.zip", None),
            ("export/x.json", None),
        ] {
            assert_eq!(restore_rel(name, w).as_deref(), expected, "{name}");
        }
        // A program file with a dot or blanks at the end is no program file for Linux either, but it is refused
        // there too: the rule of the endings does not depend on the rule of Windows.
        assert_eq!(restore_rel("tool.exe.", Flavor::Posix), None);
        assert_eq!(restore_rel("tool.exe . .", Flavor::Posix), None);
        assert_eq!(restore_rel("Tool.EXE ", Flavor::Posix), None);
        // Under Linux a backslash is a letter of the name, not a separator.
        assert_eq!(restore_rel("a\\b.json", Flavor::Posix).as_deref(), Some("a\\b.json"));
        assert_eq!(restore_rel("..\\b.json", Flavor::Posix).as_deref(), Some("..\\b.json"));
        assert_eq!(restore_rel("../b.json", Flavor::Posix), None);
    }

    #[test]
    fn a_backup_holds_what_comes_along_the_attachment_and_the_controls_and_reads_back() {
        let (paths, dir) = paths_in("rund");
        put(&paths, "Bestand/bauplaene.json", "[1,2]");
        put(&paths, "Einstellungen/einstellungen.json", "{\"farbe\": \"rot\"}");
        put(&paths, "Intern/preise.json", "{}");
        let controls = dir.join("actionmaps.xml");
        fs::write(&controls, "<a/>").unwrap();
        let bindings =
            [Binding { full: controls.to_string_lossy().into_owned(), name: "Steuerung/actionmaps.xml".to_string() }];
        let target = dir.join("meine.zip").to_string_lossy().into_owned();
        let count = write(&paths, &target, "5.0.0", &bindings).unwrap();
        assert_eq!(count, 3, "the two files, and the controls; the cache stays out");
        let data = fs::read(&target).unwrap();
        let names: Vec<String> = zip::entries(&data).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            ["Bestand/bauplaene.json", "Einstellungen/einstellungen.json", "Steuerung/actionmaps.xml", INFO_FILE]
        );
        assert!(!Path::new(&format!("{target}.teil")).exists(), "the temporary file is gone");
        let found = check(&target).unwrap();
        assert_eq!(found.count, 3);
        assert!(!found.when.is_empty(), "the date stands in the attachment: {found:?}");
        assert!(paths.setting(LAST_SETTING).is_some(), "the time of the last backup is written down");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_to_back_up_is_said_and_a_broken_target_leaves_no_half_file() {
        let (paths, dir) = paths_in("leer");
        // A new data folder holds no file of its own yet.
        let target = dir.join("leer.zip").to_string_lossy().into_owned();
        assert_eq!(write(&paths, &target, "", &[]), Err(WriteError::Empty));
        assert!(!Path::new(&target).exists());
        put(&paths, "Bestand/x.json", "1");
        let missing = dir.join("gibt-es-nicht").join("a.zip").to_string_lossy().into_owned();
        assert!(matches!(write(&paths, &missing, "", &[]), Err(WriteError::Failed(_))));
        assert!(!Path::new(&format!("{missing}.teil")).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_no_backup_is_refused_before_anything_is_written() {
        let (paths, dir) = paths_in("ungueltig");
        put(&paths, "Bestand/x.json", "1");
        let plain = dir.join("text.zip");
        fs::write(&plain, "kein zip").unwrap();
        let other = dir.join("fremd.zip");
        fs::write(&other, zip::tests::build(&[("a.txt", b"x", true)])).unwrap();
        let mark_only = dir.join("ohne-kennung.zip");
        fs::write(&mark_only, zip::tests::build(&[(INFO_FILE, b"Irgendein Text", true)])).unwrap();
        for file in [&plain, &other, &mark_only, &dir.join("gibt-es-nicht.zip")] {
            let source = file.to_string_lossy().into_owned();
            assert_eq!(check(&source), None, "{source}");
            assert_eq!(restore(&paths, &source, &[]), Err(RestoreError::Invalid), "{source}");
        }
        assert_eq!(tree(&paths), ["Bestand/x.json"], "nothing was touched");
        assert!(
            !fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("vorher")),
            "no copy of before for a refusal"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_backup_of_the_old_program_and_an_english_attachment_are_read() {
        let (paths, dir) = paths_in("alt");
        put(&paths, "Bestand/x.json", "1");
        let old = format!(
            "{}\r\n\r\nErstellt am 01.02.2026 10:11 mit Verse-Kit 4.12.\r\nEnthaelt 1 Dateien",
            legacy::BACKUP_MARKER
        );
        let source = dir.join("alt.zip");
        fs::write(&source, zip::tests::build(&[("Bestand/x.json", b"2", true), (INFO_FILE, old.as_bytes(), true)]))
            .unwrap();
        let found = check(&source.to_string_lossy()).unwrap();
        assert_eq!((found.count, found.when.as_str()), (1, "01.02.2026 10:11"));
        let english = format!("{MARKER}\r\n\r\nCreated on 03.04.2026 12:13 with Verse-Kit 5.0.\r\n");
        let source = dir.join("neu.zip");
        fs::write(&source, zip::tests::build(&[("Bestand/x.json", b"3", true), (INFO_FILE, english.as_bytes(), true)]))
            .unwrap();
        assert_eq!(check(&source.to_string_lossy()).unwrap().when, "03.04.2026 12:13");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restoring_puts_the_state_of_before_aside_and_writes_only_inside_the_data_folder() {
        let (paths, dir) = paths_in("einspielen");
        put(&paths, "Bestand/bauplaene.json", "alt");
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let attack = zip::tests::build(&[
            ("Bestand/bauplaene.json", b"neu", true),
            ("Lager/lager.json", b"lager", false),
            ("../ausserhalb.json", b"boese", true),
            ("a/../../ausserhalb2.json", b"boese", true),
            ("tool.exe", b"MZ", true),
            ("Intern/katalog-cache.json", b"cache", true),
            ("Steuerung/actionmaps.xml", b"<x/>", true),
            ("strom.json:versteckt", b"boese", true),
            (INFO_FILE, info.as_bytes(), true),
        ]);
        let source = dir.join("sicherung.zip");
        fs::write(&source, attack).unwrap();
        let restored = restore(&paths, &source.to_string_lossy(), &[]).unwrap();
        assert_eq!(restored.count, 8);
        // (The settings file is the time of the copy of before: it is written when that copy is made.)
        assert_eq!(
            tree(&paths),
            ["Bestand/bauplaene.json", "Einstellungen/einstellungen.json", "Lager/lager.json"],
            "only the data came in"
        );
        assert_eq!(fs::read_to_string(Path::new(&paths.app_folder()).join("Bestand/bauplaene.json")).unwrap(), "neu");
        assert!(!dir.join("ausserhalb.json").exists() && !dir.join("ausserhalb2.json").exists());
        let aside = restored.fallback.expect("the state of before is put aside");
        let before = fs::read(&aside).unwrap();
        let held = zip::entries(&before).unwrap();
        let old = held.iter().find(|e| e.name == "Bestand/bauplaene.json").unwrap();
        assert_eq!(zip::read(&before, old).unwrap(), b"alt", "the copy holds what was there");
        assert!(aside.contains("Verse-Kit-vorher-"), "{aside}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_that_starts_a_program_is_taken_over_and_foreign_paths_are_emptied() {
        let (paths, dir) = paths_in("einstellungen");
        let game = dir.join("StarCitizen").join("LIVE");
        fs::create_dir_all(&game).unwrap();
        let here = game.to_string_lossy().replace('\\', "\\\\");
        put(
            &paths,
            "Einstellungen/einstellungen.json",
            &format!("{{\"spiel_ordner\": \"{here}\", \"startprogramme\": [], \"spielstarter\": \"\"}}"),
        );
        let incoming = format!(
            "{{\"spiel_ordner\": \"{here}\", \"export_ordner\": \"\\\\\\\\host\\\\share\", \"launcher_ordner\": \"Z:\\\\gibt\\\\es\\\\nicht\", \
              \"ablage_ordner\": \"D:\\\\woanders\", \"spielstarter\": \"boese.exe\", \"farbe\": \"rot\", \
              \"startprogramme\": [{{\"name\": \"Tool\", \"datei\": \"C:\\\\x\\\\tool.exe\", \"an\": true}}]}}"
        );
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let source = dir.join("fremd.zip");
        fs::write(
            &source,
            zip::tests::build(&[
                ("Einstellungen/einstellungen.json", incoming.as_bytes(), true),
                (INFO_FILE, info.as_bytes(), true),
            ]),
        )
        .unwrap();
        let restored = restore(&paths, &source.to_string_lossy(), &[]).unwrap();
        assert_eq!(restored.disarmed, ["Tool (C:\\x\\tool.exe)", "spielstarter"]);
        let after = paths.settings();
        let get = |key: &str| after.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("spiel_ordner"), Some(Value::str(game.to_string_lossy().into_owned())), "what exists stays");
        assert_eq!(get("export_ordner"), Some(Value::str("")), "a network path is emptied");
        assert_eq!(get("launcher_ordner"), Some(Value::str("")), "a path that leads nowhere is emptied");
        assert_eq!(get("ablage_ordner"), None, "the data folder is never taken over");
        assert_eq!(get("spielstarter"), None, "no start command of before: none comes in");
        assert_eq!(get("farbe"), Some(Value::str("rot")), "everything else comes along");
        let Some(Value::Array(list)) = get("startprogramme") else { panic!("the list is there") };
        assert_eq!(list[0].get("an"), Some(&Value::Bool(false)), "a start program arrives switched off");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_settings_file_that_cannot_be_read_is_not_restored() {
        let (paths, dir) = paths_in("kaputt");
        put(&paths, "Einstellungen/einstellungen.json", "{\"farbe\": \"blau\"}");
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let source = dir.join("kaputt.zip");
        fs::write(
            &source,
            zip::tests::build(&[
                ("Einstellungen/einstellungen.json", b"{kein json", true),
                ("Einstellungen/einstellungen.bak.json", b"[1,2]", true),
                (INFO_FILE, info.as_bytes(), true),
            ]),
        )
        .unwrap();
        restore(&paths, &source.to_string_lossy(), &[]).unwrap();
        let after = paths.settings();
        assert_eq!(after.iter().find(|(k, _)| k == "farbe").map(|(_, v)| v.clone()), Some(Value::str("blau")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn under_windows_a_name_that_the_system_reads_another_way_is_not_restored() {
        let w = Flavor::Windows;
        // Each of these would land on a file the settings rule never saw (or on a device).
        for name in [
            "Einstellungen/einstellungen.json ",
            "Einstellungen/einstellungen.json.",
            "Einstellungen/einstellungen.json . .",
            "Einstellungen/EINSTE~1.JSO",
            "Einstellungen/EINSTE~2.JSO",
            "nul.json",
            "CON",
            "aux.txt",
            "Com1.log",
            "LPT9",
            "a<b.json",
            "a?b.json",
            "a*.json",
            "a|b.json",
            "a\"b.json",
            "a/b \\c.json",
            "a./b.json",
            "a /b.json",
            "x\u{1}y.json",
        ] {
            assert_eq!(restore_rel(name, w), None, "{name:?}");
        }
        // The controls under another spelling are the controls.
        for name in ["STEUERUNG/actionmaps.xml", "steuerung\\actionmaps.xml", "Steuerung\\mappings\\a.xml"] {
            assert_eq!(restore_rel(name, w), None, "{name:?}");
        }
        // Names that only look like those stay fine.
        for name in ["Bestand/console.json", "Bestand/com10.json", "Bestand/x~y.json", "Bestand/computer.json"] {
            assert_eq!(restore_rel(name, w).as_deref(), Some(name), "{name:?}");
        }
        // More that Windows starts.
        for name in ["Bestand/x.inf", "Bestand/x.SCF", "Bestand/x.msix", "Bestand/x.sys"] {
            assert_eq!(restore_rel(name, w), None, "{name:?}");
        }
        // Under Linux these are letters of a name like any other.
        assert_eq!(restore_rel("a<b.json", Flavor::Posix).as_deref(), Some("a<b.json"));
    }

    #[test]
    fn what_the_hangar_receiver_and_the_channel_folders_say_in_a_backup_is_not_taken_over() {
        let (paths, dir) = paths_in("empfaenger");
        let root = paths.app_folder();
        let here = dir.join("kanal");
        fs::create_dir_all(&here).unwrap();
        let before = jsonio::loads(&format!(
            "{{\"hangar_einlieferung\": false, \"hangar_einlieferung_port\": 7000, \"kanal_ordner\": {{}}, \"x\": \"{}\"}}",
            here.to_string_lossy().replace('\\', "\\\\")
        ))
        .unwrap();
        let Value::Object(before) = before else { panic!("an object") };
        let incoming = format!(
            "{{\"hangar_einlieferung\": true, \"hangar_einlieferung_port\": 1234, \"hangar_einlieferung_token\": \"geheim\", \
              \"kanal_ordner\": {{\"PTU\": \"\\\\\\\\host\\\\share\", \"LIVE\": \"{}\", \"EPTU\": 5, \"INNEN\": \"{}\"}}, \
              \"farbe\": \"rot\"}}",
            here.to_string_lossy().replace('\\', "\\\\"),
            Path::new(&root).join("Bestand").to_string_lossy().replace('\\', "\\\\")
        );
        let (bytes, _) = safe_settings(incoming.as_bytes(), &before, &root, &paths).unwrap();
        let Value::Object(after) = jsonio::loads(std::str::from_utf8(&bytes).unwrap()).unwrap() else { panic!() };
        let get = |key: &str| after.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("hangar_einlieferung"), Some(Value::Bool(false)), "the switch here holds");
        assert_eq!(get("hangar_einlieferung_port"), Some(Value::int(7000)), "the port here holds");
        assert_eq!(get("hangar_einlieferung_token"), None, "a pairing code from a backup is not taken");
        let Some(Value::Object(folders)) = get("kanal_ordner") else { panic!("the folders stay an object") };
        let folder = |key: &str| folders.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        if cfg!(windows) {
            assert_eq!(folder("PTU"), Some(Value::str("")), "a network path is emptied");
        }
        assert_eq!(folder("EPTU"), Some(Value::str("")), "what is no text is emptied");
        assert_eq!(
            folder("LIVE"),
            Some(Value::str(here.to_string_lossy().into_owned())),
            "a local folder outside the data folder stays"
        );
        assert_eq!(folder("INNEN"), Some(Value::str("")), "a folder inside the data folder goes");
        assert_eq!(get("farbe"), Some(Value::str("rot")), "the rest comes along");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_path_fields_and_the_data_folder_of_a_backup_are_cleaned_by_the_settings_alone() {
        let (paths, dir) = paths_in("pfadfelder");
        let root = paths.app_folder();
        // Folders that EXIST: a folder inside the data folder and one next to it.
        let inside = Path::new(&root).join("Spiel");
        let outside = dir.join("anderswo");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let text = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        let incoming = format!(
            "{{\"spiel_ordner\": \"{}\", \"export_ordner\": \"{}\", \"launcher_ordner\": \"{}\", \
              \"ablage_ordner\": \"{}\"}}",
            text(&inside),
            text(&outside),
            text(&inside),
            text(&outside)
        );
        let (bytes, _) = safe_settings(incoming.as_bytes(), &Vec::new(), &root, &paths).unwrap();
        let Value::Object(after) = jsonio::loads(std::str::from_utf8(&bytes).unwrap()).unwrap() else { panic!() };
        let get = |key: &str| after.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert_eq!(get("spiel_ordner"), Some(Value::str("")), "inside the data folder: no place for the game");
        assert_eq!(get("launcher_ordner"), Some(Value::str("")));
        assert_eq!(get("export_ordner"), Some(Value::str(outside.to_string_lossy().into_owned())), "outside stays");
        assert_eq!(get("ablage_ordner"), None, "the data folder is never taken over, by this step alone");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn without_the_copy_of_before_nothing_is_overwritten() {
        let (paths, dir) = paths_in("ohne-kopie");
        put(&paths, "Bestand/a.json", "alt");
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let source = dir.join("neu.zip");
        fs::write(&source, zip::tests::build(&[("Bestand/a.json", b"neu", true), (INFO_FILE, info.as_bytes(), true)]))
            .unwrap();
        // The copy cannot be written: its folder is not there.
        let nowhere = dir.join("gibt-es-nicht").join("vorher.zip").to_string_lossy().into_owned();
        let result = restore_with_copy(&paths, &source.to_string_lossy(), &[], &nowhere);
        assert!(matches!(result, Err(RestoreError::Failed(_))), "{result:?}");
        let read = |rel: &str| fs::read_to_string(Path::new(&paths.app_folder()).join(rel)).unwrap();
        assert_eq!(read("Bestand/a.json"), "alt", "nothing was overwritten");
        // With a place for it the same restore goes through.
        let here = dir.join("vorher.zip").to_string_lossy().into_owned();
        assert!(restore_with_copy(&paths, &source.to_string_lossy(), &[], &here).is_ok());
        assert_eq!(read("Bestand/a.json"), "neu");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_entry_stops_the_restore_before_anything_is_written() {
        let (paths, dir) = paths_in("beschaedigt");
        put(&paths, "Bestand/a.json", "alt");
        put(&paths, "Bestand/z.json", "alt");
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        // Stored (not packed) entries, so that the content can be found and spoilt without a checksum noticing.
        let mut archive = zip::tests::build(&[
            ("Bestand/a.json", b"NEU-A-NEU-A", false),
            ("Bestand/z.json", b"NEU-Z-NEU-Z", false),
            (INFO_FILE, info.as_bytes(), true),
        ]);
        let at = archive.windows(11).position(|w| w == b"NEU-Z-NEU-Z").unwrap();
        archive[at] = b'X';
        let source = dir.join("kaputt.zip");
        fs::write(&source, archive).unwrap();
        let result = restore(&paths, &source.to_string_lossy(), &[]);
        assert!(matches!(result, Err(RestoreError::Failed(_))), "{result:?}");
        let read = |rel: &str| fs::read_to_string(Path::new(&paths.app_folder()).join(rel)).unwrap();
        assert_eq!(read("Bestand/a.json"), "alt", "the first entry was not written either");
        assert_eq!(read("Bestand/z.json"), "alt");
        assert!(
            !fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("vorher")),
            "and no copy of before is left behind for it"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn traces_of_the_running_program_do_not_go_into_a_backup() {
        let (paths, dir) = paths_in("spuren");
        put(&paths, "Bestand/x.json", "1");
        put(&paths, "Bestand/x.json.123-1.tmp", "halb");
        put(&paths, "Bestand/y.json.teil", "halb");
        assert_eq!(tree(&paths), ["Bestand/x.json"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_controls_in_a_backup_are_named_before_anything_is_restored() {
        let (_paths, dir) = paths_in("steuerung");
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let source = dir.join("s.zip");
        fs::write(
            &source,
            zip::tests::build(&[
                ("Steuerung/actionmaps.xml", b"<x/>", true),
                ("Steuerung/attributes.xml", b"<x/>", true),
                ("Steuerung/mappings/Zeta.xml", b"<x/>", true),
                ("Steuerung/mappings/alpha.xml", b"<x/>", true),
                ("Steuerung/mappings/notiz.txt", b"x", true),
                ("steuerung/klein.xml", b"<x/>", true),
                (INFO_FILE, info.as_bytes(), true),
            ]),
        )
        .unwrap();
        let (active, profiles) = bindings_in_archive(&source.to_string_lossy());
        assert!(active);
        assert_eq!(profiles, ["alpha", "Zeta"]);
        assert_eq!(bindings_in_archive(&dir.join("gibt-es-nicht.zip").to_string_lossy()), (false, Vec::new()));
        let _ = fs::remove_dir_all(dir);
    }

    /// What a binding file of the game says: a real root, and a mark that tells the files apart.
    fn xml(mark: &str) -> String {
        format!(
            "<ActionMaps>\n <ActionProfiles profileName=\"default\"><actionmap name=\"{mark}\"/></ActionProfiles>\n</ActionMaps>\n"
        )
    }

    /// A made-up installation under a throwaway home with its own data folder: the active binding, the settings of
    /// the game and two saved profiles, and a game that has been quiet for `age` seconds. Gives the paths, the
    /// home and the game folder.
    fn game_world(name: &str, age: u64, mark: &str) -> (Paths, std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("versekit-steuerung-{}-{name}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        let game = dir.join("StarCitizen").join("LIVE");
        let zero = game.join("USER").join("Client").join("0");
        fs::create_dir_all(zero.join("Profiles").join("default")).unwrap();
        fs::create_dir_all(zero.join("controls").join("mappings")).unwrap();
        fs::write(zero.join("Profiles/default/actionmaps.xml"), xml(&format!("aktiv-{mark}"))).unwrap();
        fs::write(
            zero.join("Profiles/default/attributes.xml"),
            format!("<Attributes><Attr name=\"{mark}\"/></Attributes>"),
        )
        .unwrap();
        fs::write(zero.join("controls/mappings/Kampf.xml"), xml(&format!("kampf-{mark}"))).unwrap();
        fs::write(zero.join("controls/mappings/Bergbau.xml"), xml(&format!("bergbau-{mark}"))).unwrap();
        fs::write(game.join("Game.log"), "x").unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        fs::File::options().write(true).open(game.join("Game.log")).and_then(|f| f.set_modified(at)).unwrap();
        let (text, game_text) = (dir.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
        let vars = [
            ("HOME", text.as_str()),
            ("USERPROFILE", text.as_str()),
            ("APPDATA", text.as_str()),
            (crate::paths::ENV_INSTALL_DIR, game_text.as_str()),
            (crate::game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(Host::new(Flavor::native(), &vars, &text));
        assert!(paths.app_folder().starts_with(&text), "the data folder lies in the throwaway home");
        (paths, dir, game_text)
    }

    fn zero_of(game: &str) -> std::path::PathBuf {
        Path::new(game).join("USER").join("Client").join("0")
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    /// A backup with the data of `paths` and the controls of its game, written to `<dir>/<name>`.
    fn backup_with_controls(paths: &Paths, dir: &Path, name: &str) -> String {
        put(paths, "Bestand/bauplaene.json", "[1]");
        let target = dir.join(name).to_string_lossy().into_owned();
        write(paths, &target, "5.0.0", &bindings(paths, None)).unwrap();
        target
    }

    #[test]
    fn the_controls_come_from_the_game_folder_and_are_named_like_4_12() {
        let (paths, dir, game) = game_world("sammeln", 7200, "a");
        let names: Vec<String> = bindings(&paths, None).into_iter().map(|b| b.name).collect();
        assert_eq!(
            names,
            [
                "Steuerung/actionmaps.xml",
                "Steuerung/attributes.xml",
                "Steuerung/mappings/Bergbau.xml",
                "Steuerung/mappings/Kampf.xml"
            ]
        );
        // The same, asked for a game folder by name.
        assert_eq!(bindings(&paths, Some(&game)).len(), 4);
        // Without a game folder (or one without controls) the list is empty, which is no error.
        assert!(bindings(&paths, Some(&dir.join("gibt-es-nicht").to_string_lossy())).is_empty());
        // The settings of the game belong to the active binding: where it is not, they are not taken alone.
        let zero = zero_of(&game);
        fs::remove_file(zero.join("Profiles/default/actionmaps.xml")).unwrap();
        let names: Vec<String> = bindings(&paths, None).into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["Steuerung/mappings/Bergbau.xml", "Steuerung/mappings/Kampf.xml"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn of_two_files_of_the_same_name_in_two_folders_the_newer_one_is_taken() {
        let dir = std::env::temp_dir().join(format!("versekit-steuerung-{}-doppelt", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        let (old, new) = (dir.join("a").join("mappings"), dir.join("b").join("mappings"));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("Kampf.xml"), "alt").unwrap();
        fs::write(old.join("nur-alt.xml"), "alt").unwrap();
        fs::write(old.join("notiz.txt"), "keine Belegung").unwrap();
        fs::write(new.join("Kampf.xml"), "neu").unwrap();
        let at = |secs: u64, path: &Path| {
            let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            fs::File::options().write(true).open(path).and_then(|f| f.set_modified(time)).unwrap();
        };
        at(1_000_000, &old.join("Kampf.xml"));
        at(2_000_000, &new.join("Kampf.xml"));
        let folders = [old.to_string_lossy().into_owned(), new.to_string_lossy().into_owned()];
        let found = collect_bindings(None, &folders, Flavor::native());
        let by_name: Vec<(&str, String)> = found.iter().map(|b| (b.name.as_str(), read(Path::new(&b.full)))).collect();
        assert_eq!(
            by_name,
            [
                ("Steuerung/mappings/Kampf.xml", "neu".to_string()),
                ("Steuerung/mappings/nur-alt.xml", "alt".to_string())
            ],
            "the newer file wins, the other name stays, the text file stays out"
        );
        // The other way round (the newer file lies in the first folder): the same file wins.
        let found = collect_bindings(None, &[folders[1].clone(), folders[0].clone()], Flavor::native());
        assert_eq!(read(Path::new(&found[0].full)), "neu");
        // The same time: the folder that comes first keeps its file.
        at(2_000_000, &old.join("Kampf.xml"));
        let found = collect_bindings(None, &folders, Flavor::native());
        assert_eq!(read(Path::new(&found[0].full)), "alt");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_saved_profiles_come_back_alone_and_the_active_binding_and_the_game_settings_stay() {
        let (from, dir_from, _) = game_world("quelle", 7200, "quelle");
        let archive = backup_with_controls(&from, &dir_from, "mit.zip");
        let (to, dir_to, game_to) = game_world("ziel", 7200, "ziel");
        let zero = zero_of(&game_to);
        let done = restore_bindings(&to, &archive, false, None).unwrap();
        assert_eq!(done, BindingsRestored { count: 2, skipped: 0, fallback: None });
        assert!(read(&zero.join("controls/mappings/Kampf.xml")).contains("kampf-quelle"));
        assert!(read(&zero.join("controls/mappings/Bergbau.xml")).contains("bergbau-quelle"));
        assert!(read(&zero.join("Profiles/default/actionmaps.xml")).contains("aktiv-ziel"), "the active one stays");
        assert!(read(&zero.join("Profiles/default/attributes.xml")).contains("\"ziel\""), "the settings stay");
        // The profiles that were replaced have their copy beside them.
        let copies = game_files_named(&zero.join("controls/mappings"), "Kampf.xml.");
        assert_eq!(copies.len(), 1, "{copies:?}");
        let _ = fs::remove_dir_all(dir_from);
        let _ = fs::remove_dir_all(dir_to);
    }

    fn game_files_named(folder: &Path, start: &str) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(start))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn the_active_binding_comes_back_only_when_asked_with_the_game_settings_and_a_copy_of_the_old_one() {
        let (from, dir_from, _) = game_world("aktiv-quelle", 7200, "quelle");
        let archive = backup_with_controls(&from, &dir_from, "mit.zip");
        let (to, dir_to, game_to) = game_world("aktiv-ziel", 7200, "ziel");
        let zero = zero_of(&game_to);
        let old_active = read(&zero.join("Profiles/default/actionmaps.xml"));
        let done = restore_bindings(&to, &archive, true, None).unwrap();
        assert_eq!((done.count, done.skipped), (4, 0));
        assert!(read(&zero.join("Profiles/default/actionmaps.xml")).contains("aktiv-quelle"));
        assert!(read(&zero.join("Profiles/default/attributes.xml")).contains("\"quelle\""));
        let copy = done.fallback.expect("the old active binding was put aside");
        assert_eq!(read(Path::new(&copy)), old_active, "the copy is the old binding, as it was");
        assert!(copy.contains("actionmaps.xml."), "{copy}");
        let settings_copies = game_files_named(&zero.join("Profiles/default"), "attributes.xml.");
        assert_eq!(settings_copies.len(), 1, "the old settings have a copy too: {settings_copies:?}");
        let _ = fs::remove_dir_all(dir_from);
        let _ = fs::remove_dir_all(dir_to);
    }

    #[test]
    fn nothing_of_the_controls_is_written_while_the_game_runs() {
        let (from, dir_from, _) = game_world("lauf-quelle", 7200, "quelle");
        let archive = backup_with_controls(&from, &dir_from, "mit.zip");
        // The log was written a second ago: the game runs.
        let (to, dir_to, game_to) = game_world("lauf-ziel", 0, "ziel");
        let zero = zero_of(&game_to);
        for with_active in [false, true] {
            match restore_bindings(&to, &archive, with_active, None) {
                Err(BindingsError::GameNotQuiet(reason)) => assert!(!reason.is_empty()),
                other => panic!("the game runs, nothing may be written: {other:?}"),
            }
        }
        assert!(read(&zero.join("controls/mappings/Kampf.xml")).contains("kampf-ziel"));
        assert!(read(&zero.join("Profiles/default/actionmaps.xml")).contains("aktiv-ziel"));
        assert!(game_files_named(&zero.join("controls/mappings"), "Kampf.xml.").is_empty(), "no copy either");
        let _ = fs::remove_dir_all(dir_from);
        let _ = fs::remove_dir_all(dir_to);
    }

    #[test]
    fn a_name_from_the_archive_stays_in_the_folder_of_the_profiles_and_only_binding_files_are_written() {
        let (to, dir, game) = game_world("namen", 7200, "ziel");
        let zero = zero_of(&game);
        let info = format!("{MARKER}\r\n\r\nErstellt am 01.01.2026 09:00 mit Verse-Kit 5.\r\n");
        let good = xml("ok");
        let source = dir.join("s.zip");
        fs::write(
            &source,
            zip::tests::build(&[
                ("Steuerung/mappings/../raus.xml", good.as_bytes(), true),
                ("Steuerung/mappings/unter/ordner.xml", good.as_bytes(), true),
                ("Steuerung/mappings/schaedlich.bat", good.as_bytes(), true),
                ("Steuerung/mappings/notiz.txt", good.as_bytes(), true),
                ("Steuerung/mappings/kein-xml.xml", b"das ist kein XML", true),
                ("Steuerung/mappings/falsche-wurzel.xml", b"<html/>", true),
                ("Steuerung/mappings/.xml", good.as_bytes(), true),
                ("Steuerung/ausser.xml", good.as_bytes(), true),
                ("steuerung/klein.xml", good.as_bytes(), true),
                (INFO_FILE, info.as_bytes(), true),
            ]),
        )
        .unwrap();
        let done = restore_bindings(&to, &source.to_string_lossy(), true, None).unwrap();
        let mappings = zero.join("controls/mappings");
        let names = game_files_named(&mappings, "");
        assert!(names.contains(&"raus.xml".to_string()), "only the plain name is written: {names:?}");
        assert!(names.contains(&"ordner.xml".to_string()), "{names:?}");
        for refused in ["schaedlich.bat", "notiz.txt", "kein-xml.xml", "falsche-wurzel.xml", ".xml", "ausser.xml"] {
            assert!(!names.contains(&refused.to_string()), "{refused} must not be written: {names:?}");
        }
        assert!(!zero.join("raus.xml").exists() && !zero.join("controls/raus.xml").exists());
        assert!(!zero.join("ausser.xml").exists() && !zero.join("controls/ausser.xml").exists());
        assert_eq!(done.count, 2);
        assert_eq!(done.skipped, 5, "the bat, the text, the broken ones and the empty name were left out");
        // The active binding was not in the archive: it is as it was.
        assert!(read(&zero.join("Profiles/default/actionmaps.xml")).contains("aktiv-ziel"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn controls_have_no_place_without_a_game_folder_and_a_foreign_file_is_no_backup() {
        let (from, dir_from, _) = game_world("ohne-quelle", 7200, "quelle");
        let archive = backup_with_controls(&from, &dir_from, "mit.zip");
        // A home without any game: no folder for the profiles, nothing is written (and nothing is invented).
        let dir = std::env::temp_dir().join(format!("versekit-steuerung-{}-leer", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let nowhere = dir.join("kein-spiel").to_string_lossy().into_owned();
        let vars = [
            ("HOME", text.as_str()),
            ("USERPROFILE", text.as_str()),
            ("APPDATA", text.as_str()),
            (crate::paths::ENV_INSTALL_DIR, nowhere.as_str()),
            (crate::game_start::ENV_NO_PROCESS, "1"),
        ];
        let bare = Paths::new(Host::new(Flavor::native(), &vars, &text));
        let old_log = dir.join("kein-spiel");
        fs::create_dir_all(&old_log).unwrap();
        fs::write(old_log.join("Game.log"), "x").unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        fs::File::options().write(true).open(old_log.join("Game.log")).and_then(|f| f.set_modified(at)).unwrap();
        let done = restore_bindings(&bare, &archive, true, None).unwrap();
        assert_eq!((done.count, done.skipped), (0, 4), "no place for any of the four");
        assert!(!old_log.join("USER").exists(), "no folder was invented");
        // A file that is no backup is refused before anything is read.
        let wrong = dir.join("kein.zip");
        fs::write(&wrong, "kein zip").unwrap();
        assert_eq!(restore_bindings(&bare, &wrong.to_string_lossy(), true, None), Err(BindingsError::Invalid));
        let _ = fs::remove_dir_all(dir_from);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_controls_survive_the_whole_way_with_the_restore_of_the_data_and_the_copy_of_before() {
        let (from, dir_from, _) = game_world("weg-quelle", 7200, "quelle");
        let archive = backup_with_controls(&from, &dir_from, "mit.zip");
        let (to, dir_to, game_to) = game_world("weg-ziel", 7200, "ziel");
        put(&to, "Bestand/bauplaene.json", "[2]");
        let before = bindings(&to, None);
        let restored = restore(&to, &archive, &before).unwrap();
        let aside = restored.fallback.expect("the state of before lies aside");
        // The copy of before carries the controls of the player, so that a slip costs nothing.
        let data = fs::read(&aside).unwrap();
        let names: Vec<String> = zip::entries(&data).unwrap().into_iter().map(|e| e.name).collect();
        for name in ["Steuerung/actionmaps.xml", "Steuerung/attributes.xml", "Steuerung/mappings/Kampf.xml"] {
            assert!(names.contains(&name.to_string()), "{name} in {names:?}");
        }
        // ... and restoring the data of the archive did not touch the game folder: that is a step of its own.
        assert!(read(&zero_of(&game_to).join("Profiles/default/actionmaps.xml")).contains("aktiv-ziel"));
        let done = restore_bindings(&to, &archive, true, None).unwrap();
        assert_eq!(done.count, 4);
        let _ = fs::remove_dir_all(dir_from);
        let _ = fs::remove_dir_all(dir_to);
    }

    #[test]
    fn local_drives_are_told_from_network_paths_and_the_data_folder() {
        let (paths, dir) = paths_in("pfade");
        let root = paths.app_folder();
        if cfg!(windows) {
            for (text, local) in [
                ("D:\\Spiele\\LIVE", true),
                ("d:/Spiele/LIVE", true),
                ("\\\\host\\share\\x", false),
                ("//host/share/x", false),
                ("\\/host/share", false),
                ("\\\\?\\UNC\\host\\share", false),
                ("\\\\?\\C:\\x", false),
                ("D:relativ", false),
                ("relativ\\x", false),
                ("\\ohne\\laufwerk", false),
            ] {
                assert_eq!(local_path(text, &root, &paths), local, "{text}");
            }
        }
        assert!(!local_path(&root, &root, &paths), "the data folder itself is no place for the game");
        assert!(!local_path(&format!("{root}{}x", paths.host.flavor.sep()), &root, &paths));
        let _ = fs::remove_dir_all(dir);
    }
}
