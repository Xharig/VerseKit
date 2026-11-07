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

//! The backup at the first start of this program on a data folder, before it writes anything there.
//!
//! The program works on the data folder the older version left (the pointer `ablage_ordner`). Before the first
//! write, everything of one's own goes into one file, as the button "Sicherung" writes it (with the controls),
//! into the folder [`FOLDER`] of the data folder, with the mark of 4.12 in its attachment so that 4.12 can restore
//! it. A mark in the data folder ([`MARK_FILE`]) says that it is done;
//! it is written only after the file was written and read back. A data folder without data of its own gets the
//! mark without a file.
//!
//! ⚠ While the backup is made, every write of this thread is refused (`writeguard`) except into the folder of the
//! file: finding the data folder must not create, heal or move anything before the state of before is kept.

use std::fs;
use std::path::Path;

use crate::backup::{self, Binding, WriteError};
use crate::jsonio::{self, Map, Value};
use crate::language::{self, Arg};
use crate::paths::Paths;
use crate::{legacy, writeguard};

/// The mark in the data folder (below `Intern`): the backup of the first start is done.
pub const MARK_FILE: &str = "erststart-sicherung.json";
/// The folder in the data folder the backup goes to unless the player picks another one.
pub const FOLDER: &str = "Sicherungen";

/// The fields of the mark: the file written (empty: there was nothing to keep), when, by which version, and
/// whether the player was told.
const KEY_FILE: &str = "sicherung";
const KEY_TIME: &str = "zeit";
const KEY_VERSION: &str = "version";
const KEY_SHOWN: &str = "gemeldet";

/// What stays free on the disk beyond the size of the backup (bytes).
const MARGIN: u64 = 8 * 1024 * 1024;
/// What a file costs in the archive beyond its content, at most (headers of the zip format).
const PER_FILE: u64 = 256;

/// What the start found or did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The mark is there: the backup of the first start was made before.
    Done,
    /// No data of one's own in the data folder: nothing to keep, the mark is written.
    Nothing,
    /// The backup is written and read back, the mark is written.
    Written { file: String, count: usize },
}

/// Why the backup of the first start could not be made. The program does not write until it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Not enough room in the folder of the backup (bytes).
    NoRoom { folder: String, needed: u64, free: u64 },
    /// The reason, for the player.
    Failed { folder: String, reason: String },
}

impl Failure {
    /// The folder the backup was to go to.
    pub fn folder(&self) -> &str {
        match self {
            Failure::NoRoom { folder, .. } | Failure::Failed { folder, .. } => folder,
        }
    }

    /// What the player reads, in the language of the moment.
    pub fn text(&self) -> String {
        match self {
            Failure::NoRoom { folder, needed, free } => language::tf(
                "erst_sich_kein_platz",
                &[
                    ("0", Arg::from(folder.as_str())),
                    ("1", Arg::from(megabytes(*needed))),
                    ("2", Arg::from(megabytes(*free))),
                ],
            ),
            Failure::Failed { folder, reason } => {
                language::tf("erst_sich_grund", &[("0", Arg::from(folder.as_str())), ("1", Arg::from(reason.as_str()))])
            }
        }
    }
}

/// Bytes as whole megabytes, rounded up (`1 MB` at least).
fn megabytes(bytes: u64) -> String {
    format!("{} MB", bytes.div_ceil(1024 * 1024).max(1))
}

/// The mark in the data folder `root`, as `Paths::app_file` places it, without creating a folder.
fn mark_in(paths: &Paths, root: &str) -> String {
    let flavor = paths.host.flavor;
    if paths.host.var_set(legacy::ENV_HOME).is_some() {
        flavor.join(root, MARK_FILE)
    } else {
        flavor.join_all(root, &[crate::paths::subfolder(MARK_FILE), MARK_FILE])
    }
}

/// The folder the backup goes to by default: [`FOLDER`] in the data folder `root`.
fn default_folder(paths: &Paths, root: &str) -> String {
    paths.host.flavor.join(root, FOLDER)
}

/// The name of the file: the date, the time and what it is for.
pub fn file_name() -> String {
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    format!("Verse-Kit-Sicherung-vor-VerseKit-5-{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}.zip")
}

/// Is the backup of the first start still to be made (the mark is missing)? Reads only.
pub fn needed(paths: &Paths) -> bool {
    let before = writeguard::state();
    writeguard::deny(true);
    writeguard::allow_under(None);
    let root = paths.app_folder();
    writeguard::adopt(&before);
    !Path::new(&mark_in(paths, &root)).is_file()
}

/// The free bytes for the caller on the disk of `folder` (the nearest folder above it that exists); `None` when
/// nobody can tell.
pub fn free_space(folder: &str) -> Option<u64> {
    let mut at = Path::new(folder);
    while !at.is_dir() {
        at = at.parent()?;
    }
    free_on(at)
}

#[cfg(windows)]
fn free_on(folder: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = folder.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free = 0u64;
    // SAFETY: the text ends with a zero and lives until the call returns; the other two answers are not asked for.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(unix)]
fn free_on(folder: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let text = std::ffi::CString::new(folder.as_os_str().as_bytes()).ok()?;
    let mut info: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the text ends with a zero, the answer is a struct of the right type.
    let ok = unsafe { libc::statvfs(text.as_ptr(), &mut info) };
    #[allow(clippy::unnecessary_cast)]
    (ok == 0).then(|| info.f_bavail as u64 * info.f_frsize as u64)
}

#[cfg(not(any(windows, unix)))]
fn free_on(_: &Path) -> Option<u64> {
    None
}

/// Makes the backup of the first start where it is still to be made (see the module). `folder` is the folder
/// the player picked; `None` is [`FOLDER`] in the data folder.
///
/// # Errors
///
/// [`Failure`] when the backup or the mark could not be written; nothing else was written then.
pub fn ensure(paths: &Paths, version: &str, folder: Option<&str>) -> Result<Outcome, Failure> {
    ensure_with(paths, version, folder, &free_space)
}

/// [`ensure`] with the look at the free room given.
///
/// # Errors
///
/// As [`ensure`].
pub fn ensure_with(
    paths: &Paths,
    version: &str,
    folder: Option<&str>,
    free: &dyn Fn(&str) -> Option<u64>,
) -> Result<Outcome, Failure> {
    let before = writeguard::state();
    writeguard::deny(true);
    writeguard::allow_under(None);
    let made = make(paths, version, folder, free);
    writeguard::adopt(&before);
    let outcome = made?;
    if outcome == Outcome::Done {
        return Ok(outcome);
    }
    let file = match &outcome {
        Outcome::Written { file, .. } => file.clone(),
        _ => String::new(),
    };
    write_mark(paths, &file, version)?;
    Ok(outcome)
}

/// The backup under the lock of [`ensure_with`]: everything but the mark.
fn make(
    paths: &Paths,
    version: &str,
    folder: Option<&str>,
    free: &dyn Fn(&str) -> Option<u64>,
) -> Result<Outcome, Failure> {
    let root = paths.app_folder();
    if Path::new(&mark_in(paths, &root)).is_file() {
        return Ok(Outcome::Done);
    }
    let folder = folder.map_or_else(|| default_folder(paths, &root), str::to_string);
    let failed = |reason: String| Failure::Failed { folder: folder.clone(), reason };
    let files = match backup::files(&root) {
        Ok(files) => files,
        // No data folder yet: a computer without the older version.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(failed(error.to_string())),
    };
    if files.is_empty() {
        return Ok(Outcome::Nothing);
    }
    let bindings = backup::bindings(paths, None);
    let needed = size_of(files.iter().map(|(full, _)| full.as_str()), &bindings);
    let room = free(&folder).ok_or_else(|| failed(language::t("erst_sich_platz_unklar")))?;
    if room < needed {
        return Err(Failure::NoRoom { folder, needed, free: room });
    }
    writeguard::allow_under(Some(&folder));
    jsonio::make_dirs(&folder).map_err(|e| failed(e.to_string()))?;
    let target = free_name(paths, &folder);
    // ⚠ The mark of 4.12 (`legacy::BACKUP_MARKER`): 4.12 restores only a backup with that one, so the way back to it
    // can take this file. This program reads both marks.
    let count = match backup::write_marked(paths, &target, version, &bindings, legacy::BACKUP_MARKER) {
        Ok(count) => count,
        // Gone between the look and the writing.
        Err(WriteError::Empty) => return Ok(Outcome::Nothing),
        Err(WriteError::Failed(reason)) => return Err(failed(reason)),
    };
    // Read back: a file that cannot be read is no backup.
    match backup::check(&target) {
        Some(found) if found.count == count => Ok(Outcome::Written { file: target, count }),
        _ => Err(failed(language::t("erst_sich_nicht_lesbar"))),
    }
}

/// What the backup of these files and `bindings` needs on the disk, at most.
fn size_of<'a>(files: impl Iterator<Item = &'a str>, bindings: &'a [Binding]) -> u64 {
    files
        .chain(bindings.iter().map(|b| b.full.as_str()))
        .map(|full| fs::metadata(full).map_or(0, |m| m.len()) + PER_FILE)
        .sum::<u64>()
        + MARGIN
}

/// A name in `folder` that is not taken yet: [`file_name`], else with a number.
fn free_name(paths: &Paths, folder: &str) -> String {
    let name = file_name();
    let first = paths.host.flavor.join(folder, &name);
    if !Path::new(&first).exists() {
        return first;
    }
    let stem = name.trim_end_matches(".zip");
    (2..)
        .map(|n| paths.host.flavor.join(folder, &format!("{stem}-{n}.zip")))
        .find(|candidate| !Path::new(candidate).exists())
        .unwrap_or(first)
}

/// Writes the mark: the file of the backup (empty when there was nothing to keep), the time, the version.
fn write_mark(paths: &Paths, file: &str, version: &str) -> Result<(), Failure> {
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    let data: Map = vec![
        (KEY_FILE.into(), Value::str(file)),
        (KEY_TIME.into(), Value::str(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"))),
        (KEY_VERSION.into(), Value::str(version)),
        (KEY_SHOWN.into(), Value::Bool(file.is_empty())),
    ];
    let target = paths.app_file(MARK_FILE);
    jsonio::save_json(&target, &Value::Object(data), 2, false)
        .map_err(|e| Failure::Failed { folder: paths.host.flavor.dirname(&target), reason: e.to_string() })
}

/// The mark as it lies in the data folder, if it can be read.
fn read_mark(paths: &Paths) -> Option<Map> {
    match jsonio::read_json(&paths.app_file(MARK_FILE)).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// The file of the backup of the first start, while the player has not been told about it yet.
pub fn hint(paths: &Paths) -> Option<String> {
    let mark = read_mark(paths)?;
    let field = |key: &str| mark.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    if matches!(field(KEY_SHOWN), Some(Value::Bool(true))) {
        return None;
    }
    field(KEY_FILE).and_then(Value::as_str).filter(|file| !file.is_empty()).map(str::to_string)
}

/// Notes in the mark that the player was told; false when it could not be written.
pub fn hint_shown(paths: &Paths) -> bool {
    let Some(mut mark) = read_mark(paths) else { return false };
    jsonio::map_set(&mut mark, KEY_SHOWN, Value::Bool(true));
    jsonio::save_json(&paths.app_file(MARK_FILE), &Value::Object(mark), 2, false).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    /// A throwaway home with the data folder in the real layout, a made-up game folder with controls, and no
    /// look at the processes. Returns the paths, the home and the data folder.
    fn home(name: &str) -> (Paths, crate::testdir::Dir, String) {
        let dir = crate::testdir::Dir::new(&format!("erststart-{name}"));
        let text = dir.path().to_string_lossy().into_owned();
        let game = dir.path().join("StarCitizen").join("LIVE");
        let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("actionmaps.xml"), "<ActionMaps/>").unwrap();
        fs::write(game.join("Game.log"), "").unwrap();
        let game_text = game.to_string_lossy().into_owned();
        let vars = [
            ("HOME", text.as_str()),
            ("USERPROFILE", text.as_str()),
            ("APPDATA", text.as_str()),
            (crate::paths::ENV_INSTALL_DIR, game_text.as_str()),
            (crate::game_start::ENV_NO_PROCESS, "1"),
        ];
        let paths = Paths::new(Host::new(Flavor::native(), &vars, &text));
        let root = paths.app_folder();
        assert!(root.starts_with(&text), "the data folder lies in the throwaway home: {root}");
        (paths, dir, root)
    }

    /// Data of the older version in `root`: settings and an inventory.
    fn old_data(root: &str) {
        let root = Path::new(root);
        fs::create_dir_all(root.join("Einstellungen")).unwrap();
        fs::create_dir_all(root.join("Bauplaene")).unwrap();
        fs::write(root.join("Einstellungen").join("einstellungen.json"), "{\"sprache\": \"de\"}").unwrap();
        fs::write(root.join("Bauplaene").join("bestand.json"), "[\"Arrowhead\"]").unwrap();
    }

    fn plenty(_: &str) -> Option<u64> {
        Some(u64::MAX / 2)
    }

    /// Every file below `folder`, relative, sorted.
    fn listing(folder: &str) -> Vec<String> {
        fn walk(at: &Path, rel: &str, out: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(at) else { return };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let below = if rel.is_empty() { name } else { format!("{rel}/{name}") };
                if entry.path().is_dir() {
                    walk(&entry.path(), &below, out);
                } else {
                    out.push(below);
                }
            }
        }
        let mut out = Vec::new();
        walk(Path::new(folder), "", &mut out);
        out.sort();
        out
    }

    fn names_in(zip: &str) -> Vec<String> {
        let bytes = fs::read(zip).unwrap();
        crate::zip::entries(&bytes).unwrap().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn the_first_start_backs_up_everything_with_the_controls_and_marks_it_after() {
        let (paths, _dir, root) = home("erster");
        old_data(&root);
        let before = listing(&root);
        assert!(needed(&paths));
        let Ok(Outcome::Written { file, count }) = ensure_with(&paths, "5.0.0", None, &plenty) else {
            panic!("a backup is written");
        };
        // The file lies in the folder of the backups, with the date and what it is for in its name.
        let name = Path::new(&file).file_name().unwrap().to_string_lossy().into_owned();
        assert!(Path::new(&file).starts_with(Path::new(&root).join(FOLDER)), "{file}");
        assert!(name.starts_with("Verse-Kit-Sicherung-vor-VerseKit-5-20") && name.ends_with(".zip"), "{name}");
        let names = names_in(&file);
        for wanted in ["Einstellungen/einstellungen.json", "Bauplaene/bestand.json", "Steuerung/actionmaps.xml"] {
            assert!(names.iter().any(|n| n == wanted), "{wanted} in {names:?}");
        }
        assert_eq!(backup::check(&file).map(|f| f.count), Some(count));
        // Besides the file and the mark, nothing in the data folder changed.
        let mut after = listing(&root);
        let mark = format!("Intern/{MARK_FILE}");
        let zip = format!("{FOLDER}/{name}");
        assert!(after.contains(&mark) && after.contains(&zip), "{after:?}");
        after.retain(|f| *f != mark && *f != zip);
        assert_eq!(after, before);
        assert_eq!(fs::read_to_string(Path::new(&root).join("Bauplaene/bestand.json")).unwrap(), "[\"Arrowhead\"]");
        assert!(!needed(&paths));
        assert_eq!(hint(&paths).as_deref(), Some(file.as_str()), "the player is to be told");
    }

    /// The attachment of a backup file, as text.
    fn attachment(zip: &str) -> String {
        let bytes = fs::read(zip).unwrap();
        let entries = crate::zip::entries(&bytes).unwrap();
        let info = entries.iter().find(|e| e.name == backup::INFO_FILE).expect("attachment");
        String::from_utf8_lossy(&crate::zip::read(&bytes, info).unwrap()).into_owned()
    }

    #[test]
    fn the_first_start_writes_the_mark_of_4_12_and_both_marks_are_read() {
        let (paths, dir, root) = home("kennung");
        old_data(&root);
        let Ok(Outcome::Written { file, count }) = ensure_with(&paths, "5.0.0", None, &plenty) else { panic!() };
        // As 4.12 `backup.check` looks: the attachment is there and holds its mark.
        let head = attachment(&file);
        assert!(head.starts_with(&format!("{}\r\n", legacy::BACKUP_MARKER)), "{head:?}");
        assert!(!head.contains(backup::MARKER), "{head:?}");
        assert_eq!(backup::check(&file).map(|f| f.count), Some(count));
        // A backup with the mark of v5 is read as well.
        let new = dir.path().join("neu.zip").to_string_lossy().into_owned();
        let written = backup::write_marked(&paths, &new, "5.0.0", &[], backup::MARKER).expect("written");
        assert!(attachment(&new).starts_with(&format!("{}\r\n", backup::MARKER)));
        assert_eq!(backup::check(&new).map(|f| f.count), Some(written));
    }

    #[test]
    fn the_second_start_does_not_back_up_again() {
        let (paths, _dir, root) = home("zweiter");
        old_data(&root);
        assert!(matches!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Written { .. })));
        let first = listing(&root);
        // v5 writes afterwards: the next start leaves that alone.
        fs::write(Path::new(&root).join("Bauplaene/bestand.json"), "[\"Arrowhead\", \"Karna\"]").unwrap();
        assert_eq!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Done));
        assert_eq!(listing(&root), first, "no second file, no second mark");
        // Counter-check: without the mark it would back up again.
        fs::remove_file(paths.app_file(MARK_FILE)).unwrap();
        assert!(matches!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Written { .. })));
    }

    #[test]
    fn an_empty_data_folder_needs_no_backup_and_is_not_asked_again() {
        let (paths, _dir, root) = home("leer");
        assert_eq!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Nothing));
        assert!(!Path::new(&root).join(FOLDER).exists(), "no folder of backups");
        assert_eq!(hint(&paths), None, "nothing to tell");
        // What v5 writes afterwards is no data of the older version.
        old_data(&root);
        assert_eq!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Done));
        // A data folder that does not exist yet (a computer without the older version) is the same.
        let (fresh, _dir, root) = home("ohne-ordner");
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(ensure_with(&fresh, "5.0.0", None, &plenty), Ok(Outcome::Nothing));
    }

    #[test]
    fn without_room_nothing_is_written_and_the_reason_says_how_much() {
        let (paths, _dir, root) = home("kein-platz");
        old_data(&root);
        let before = listing(&root);
        let little = |_: &str| Some(1024u64);
        let Err(Failure::NoRoom { needed: wanted, free, .. }) = ensure_with(&paths, "5.0.0", None, &little) else {
            panic!("no room");
        };
        assert!(wanted > free && free == 1024);
        assert_eq!(listing(&root), before, "neither the file nor the mark");
        assert!(needed(&paths), "the next start tries again");
        // Nobody can tell the room: no attempt either.
        assert!(matches!(ensure_with(&paths, "5.0.0", None, &|_| None), Err(Failure::Failed { .. })));
        assert_eq!(listing(&root), before);
    }

    #[test]
    fn a_folder_that_cannot_be_written_fails_without_a_mark_and_another_folder_works() {
        let (paths, dir, root) = home("ordner");
        old_data(&root);
        // A file where the folder should be: it cannot be made.
        let blocked = dir.path().join("besetzt");
        fs::write(&blocked, "x").unwrap();
        let blocked = blocked.join("unter").to_string_lossy().into_owned();
        let failure = ensure_with(&paths, "5.0.0", Some(&blocked), &plenty).expect_err("fails");
        assert_eq!(failure.folder(), blocked);
        assert!(needed(&paths), "no mark after a failure");
        let chosen = dir.path().join("gewaehlt").to_string_lossy().into_owned();
        let Ok(Outcome::Written { file, .. }) = ensure_with(&paths, "5.0.0", Some(&chosen), &plenty) else {
            panic!("the folder picked works");
        };
        assert!(Path::new(&file).starts_with(&chosen), "{file}");
        assert!(!needed(&paths));
    }

    #[test]
    fn nothing_but_the_backup_is_written_while_it_is_made() {
        let (paths, dir, default) = home("sperre");
        // A data folder somewhere else, named by the visible pointer only: finding it heals the second pointer,
        // which is a write before the backup.
        let elsewhere = dir.path().join("Ablage").to_string_lossy().into_owned();
        old_data(&elsewhere);
        let pointer = Path::new(&default).join("Einstellungen").join(crate::paths::SETTINGS_FILE);
        fs::create_dir_all(pointer.parent().unwrap()).unwrap();
        let text = jsonio::dumps(
            &Value::Object(vec![(crate::paths::STORAGE_KEY.into(), Value::str(&elsewhere))]),
            Some(2),
            false,
        );
        fs::write(&pointer, text).unwrap();
        let second = paths.second_pointer();
        assert!(!Path::new(&second).exists());
        writeguard::take_denied();
        let Ok(Outcome::Written { file, .. }) = ensure_with(&paths, "5.0.0", None, &plenty) else { panic!() };
        assert!(Path::new(&file).starts_with(&elsewhere), "the data folder of the pointer: {file}");
        let denied = writeguard::take_denied();
        let second_folder = paths.host.flavor.dirname(&second);
        assert!(denied.iter().any(|d| d.contains(&second_folder)), "the healing waited: {denied:?}");
        assert!(denied.iter().all(|d| !d.contains(FOLDER)), "the file itself was allowed: {denied:?}");
        // The lock of the caller is back as it was.
        assert_eq!(writeguard::state(), writeguard::State::default());
        // A caller with a lock of its own (a copy of the data) keeps it.
        writeguard::deny(true);
        writeguard::allow_under(Some(&elsewhere));
        let held = writeguard::state();
        assert_eq!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Done));
        assert_eq!(writeguard::state(), held);
        writeguard::deny(false);
        writeguard::allow_under(None);
        writeguard::take_denied();
    }

    #[test]
    fn the_hint_is_told_once() {
        let (paths, _dir, root) = home("hinweis");
        old_data(&root);
        let Ok(Outcome::Written { file, .. }) = ensure_with(&paths, "5.0.0", None, &plenty) else { panic!() };
        assert_eq!(hint(&paths), Some(file));
        assert!(hint_shown(&paths));
        assert_eq!(hint(&paths), None, "once");
        assert_eq!(ensure_with(&paths, "5.0.0", None, &plenty), Ok(Outcome::Done), "the mark stays a mark");
    }

    #[test]
    fn the_reasons_read_in_both_languages() {
        use crate::language::Lang;
        let no_room = Failure::NoRoom { folder: "D:/x".into(), needed: 5 * 1024 * 1024 + 1, free: 10 };
        let failed = Failure::Failed { folder: "D:/x".into(), reason: "kaputt".into() };
        for lang in [Lang::De, Lang::En] {
            let text = language::with_language(lang, || no_room.text());
            assert!(text.contains("D:/x") && text.contains("6 MB") && text.contains("1 MB"), "{text}");
            let text = language::with_language(lang, || failed.text());
            assert!(text.contains("D:/x") && text.contains("kaputt"), "{text}");
        }
    }
}
