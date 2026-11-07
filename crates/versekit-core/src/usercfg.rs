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

//! The `user.cfg` of a game channel and the lines of the player in it (`usercfg.py` of 4.12).
//!
//! ⚠⚠ **The language lines belong to Verse-Kit.** `g_language` and `g_languageAudio` are set by the program
//! itself, depending on the source of the texts chosen, and checked at every start. Whoever changed them among
//! the lines of the player would have them overwritten silently at the next start: so they are not among them,
//! and [`write_own`] never touches them.
//!
//! ⚠ **A copy of the state before the first change** (`user.cfg.versekit-vorher`): the file often holds graphics
//! settings that the player collected with effort. This copy is made once and never overwritten. On top of
//! that every writing goes through `game_files` (the game must be quiet; a copy `user.cfg.versekit-<time>`
//! first, ten stay).

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::errors;
use crate::game_files::{self, WriteError};
use crate::jsonio;
use crate::paths::Paths;
use crate::text::{py_rstrip, py_splitlines, py_strip};

/// The file in the folder of the channel.
pub const FILE: &str = "user.cfg";
/// The copy of the state before the first change.
pub const BACKUP: &str = "user.cfg.versekit-vorher";
/// The lines Verse-Kit manages itself.
pub const MANAGED: [&str; 2] = ["g_language", "g_languageAudio"];

/// `key = value`; dots and underscores in the key are usual (`pl_pit.forceSoftwareCursor`).
fn line_pattern() -> &'static Regex {
    static LINE: OnceLock<Regex> = OnceLock::new();
    LINE.get_or_init(|| Regex::new(r"^\s*[A-Za-z_][\w.]*\s*=\s*\S.*$").expect("pattern"))
}

/// Every line end as `\n` (Python reads text files that way).
pub fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The content as 4.12 reads it: bad bytes become the replacement sign, every line end becomes `\n`. A mark of
/// the byte order at the start (some editors put one in) is left off: it would make the first line invalid.
/// `None` only where the file is not there; any other failure is an error (a file that is locked is not "no
/// file": the writing would take it for empty and put only the new lines in its place).
fn read_text(path: &str) -> std::io::Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            Ok(Some(normalized(text.strip_prefix('\u{feff}').unwrap_or(&text))))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The content of the `user.cfg` in the folder; `None` where there is none or it cannot be read.
pub fn read(folder: &str) -> Option<String> {
    read_text(&Path::new(folder).join(FILE).to_string_lossy()).ok().flatten()
}

/// Like [`read`], but a file that cannot be read is told apart from one that is not there: `Ok(None)` only for
/// the latter. Whoever shows the file to be edited and saved needs the difference (a locked file is no empty one).
///
/// # Errors
///
/// Where the file exists but cannot be read.
pub fn read_checked(folder: &str) -> std::io::Result<Option<String>> {
    read_text(&Path::new(folder).join(FILE).to_string_lossy())
}

/// The part before the first `=`, without blanks; empty where there is no `=`.
pub fn key(line: &str) -> &str {
    match line.split_once('=') {
        Some((left, _)) => py_strip(left),
        None => "",
    }
}

/// All lines but the managed ones, without empty lines (and without blanks at the end).
pub fn own_lines(text: &str) -> Vec<String> {
    py_splitlines(text)
        .into_iter()
        .filter(|line| !py_strip(line).is_empty() && !MANAGED.contains(&key(line)))
        .map(|line| py_rstrip(line).to_string())
        .collect()
}

/// The lines that are not valid, as `(number from 1, line)`; empty means all is well. Empty lines and comments
/// (`;`, `--`) are allowed.
pub fn check(lines: &[String]) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let stripped = py_strip(line);
        if stripped.is_empty() || stripped.starts_with(';') || stripped.starts_with("--") {
            continue;
        }
        if !line_pattern().is_match(stripped) {
            bad.push((index + 1, line.clone()));
        }
    }
    bad
}

/// The value of a command in the text (case does not matter, as in the game); the last line counts. `None`
/// where it does not stand there.
pub fn value_in(text: &str, wanted: &str) -> Option<String> {
    let mut found = None;
    for line in py_splitlines(text) {
        if key(line).to_lowercase() == wanted.to_lowercase() {
            found = line.split_once('=').map(|(_, right)| py_strip(right).to_string());
        }
    }
    found
}

/// The value of a command in the file of the folder.
pub fn value(folder: &str, wanted: &str) -> Option<String> {
    value_in(&read(folder).unwrap_or_default(), wanted)
}

/// The text with the own `lines` and the managed lines of the old text behind them. `Err` with the invalid
/// lines: with those **nothing** is written, a half taken over file would be worse than no change.
///
/// # Errors
///
/// The invalid lines, see [`check`].
pub fn own_text(old: &str, lines: &[String]) -> Result<String, Vec<(usize, String)>> {
    let bad = check(lines);
    if !bad.is_empty() {
        return Err(bad);
    }
    let managed = py_splitlines(old).into_iter().filter(|line| MANAGED.contains(&key(line))).map(py_rstrip);
    let mut fresh: Vec<String> = lines
        .iter()
        .filter(|line| !py_strip(line).is_empty() && !MANAGED.contains(&key(line)))
        .map(|line| py_rstrip(line).to_string())
        .collect();
    fresh.extend(managed.map(str::to_string));
    Ok(fresh.join("\n") + "\n")
}

/// The text after the commands are set (`None` takes a line out): the lines of the same command are replaced in
/// every spelling, missing ones come at the end. Whatever is managed stays as it is.
///
/// # Errors
///
/// The invalid lines, see [`check`].
pub fn set_text(old: &str, changes: &[(String, Option<String>)]) -> Result<String, Vec<(usize, String)>> {
    let mut lines = own_lines(old);
    for (command, new) in changes {
        lines.retain(|line| key(line).to_lowercase() != command.to_lowercase());
        if let Some(new) = new.as_deref().map(py_strip).filter(|new| !new.is_empty()) {
            lines.push(format!("{command} = {new}"));
        }
    }
    own_text(old, &lines)
}

/// Why the commands were not set.
#[derive(Debug)]
pub enum SetError {
    /// Invalid lines (number, line): nothing was written.
    Bad(Vec<(usize, String)>),
    /// The game runs (or nobody can tell) or the copy or the writing failed.
    Write(WriteError),
}

/// Everything [`set_values`] does except the writing (the file is read and the new text worked out), so that a
/// bad line or an unreadable file is known before any other file is touched.
///
/// # Errors
///
/// [`SetError`]: invalid lines or a file that cannot be read.
pub fn check_set(folder: &str, changes: &[(String, Option<String>)]) -> Result<(), SetError> {
    let path = Path::new(folder).join(FILE).to_string_lossy().into_owned();
    let old = read_text(&path).map_err(|error| SetError::Write(WriteError::Failed(error)))?;
    set_text(old.as_deref().unwrap_or_default(), changes).map(|_| ()).map_err(SetError::Bad)
}

/// Sets the commands in the `user.cfg` of the folder. Where the file is not there it is made. Before the first
/// change the old file is kept once (`user.cfg.versekit-vorher`); the writing itself goes through `game_files`.
/// Nothing is written when the text stays as it is: gives whether the file was written.
///
/// # Errors
///
/// [`SetError`]: invalid lines, or the file cannot be read, or the game is not quiet, or the writing failed.
pub fn set_values(paths: &Paths, folder: &str, changes: &[(String, Option<String>)]) -> Result<bool, SetError> {
    // Reading, working out and writing under one lock: no other writer of a game file slips in between.
    let _one_writer = crate::injection::game_file_lock();
    let path = Path::new(folder).join(FILE).to_string_lossy().into_owned();
    let old = read_text(&path).map_err(|error| SetError::Write(WriteError::Failed(error)))?;
    let text = set_text(old.as_deref().unwrap_or_default(), changes).map_err(SetError::Bad)?;
    write_text(paths, folder, &path, old.as_deref(), &text)
}

/// Writes the own `lines` of the player as they stand (the page of the game settings): the managed lines stay
/// behind them, empty lines go. Where the file is not there it is made. Same safety as [`set_values`]: one invalid
/// line and **nothing** is written; the old file is kept once (`user.cfg.versekit-vorher`), and **without that
/// copy nothing is written** either; the game must be quiet. Gives whether the file was written (it is not where
/// the text stays as it is).
///
/// # Errors
///
/// [`SetError`]: invalid lines, or the file cannot be read, or the game is not quiet, or the copy or the writing
/// failed.
pub fn write_own(paths: &Paths, folder: &str, lines: &[String]) -> Result<bool, SetError> {
    let _one_writer = crate::injection::game_file_lock();
    let path = Path::new(folder).join(FILE).to_string_lossy().into_owned();
    let old = read_text(&path).map_err(|error| SetError::Write(WriteError::Failed(error)))?;
    let text = own_text(old.as_deref().unwrap_or_default(), lines).map_err(SetError::Bad)?;
    // Nothing to write and no file: no file is made for an empty box.
    if old.is_none() && text.trim().is_empty() {
        return Ok(false);
    }
    write_text(paths, folder, &path, old.as_deref(), &text)
}

/// The shared end of [`set_values`] and [`write_own`]: the finished `text` against the `old` content.
fn write_text(paths: &Paths, folder: &str, path: &str, old: Option<&str>, text: &str) -> Result<bool, SetError> {
    let text = jsonio::translate(text, jsonio::Newline::Native);
    if old.is_some() && std::fs::read(path).is_ok_and(|bytes| bytes == text.as_bytes()) {
        return Ok(false);
    }
    game_files::check_quiet(paths).map_err(SetError::Write)?;
    let backup = Path::new(folder).join(BACKUP);
    if old.is_some_and(|old| !old.is_empty())
        && !backup.exists()
        && let Err(error) = keep_once(path, &backup.to_string_lossy())
    {
        errors::record("usercfg.backup", &error);
        return Err(SetError::Write(WriteError::Failed(error)));
    }
    // The caller holds the lock of the game files.
    game_files::write_locked(paths, path, text.as_bytes()).map(|_| true).map_err(SetError::Write)
}

/// The once-only copy: made beside the target and renamed, so that a copy that breaks off never stands there as
/// the copy of the state before (it would never be made again).
fn keep_once(source: &str, backup: &str) -> std::io::Result<()> {
    let part = format!("{backup}.tmp");
    let result = jsonio::copy2(source, &part).and_then(|()| std::fs::rename(&part, backup));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// The short verdict on the `user.cfg` of one channel (the bubble of the page "Installation & Pfade").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No installation: the folder has no `Data.p4k`.
    NoInstall,
    /// An installation without a `user.cfg` (or one that cannot be read).
    NoFile,
    /// The file is there but has no `g_language` line: the game would start in English.
    NoLanguage,
    /// The file is there and names a language.
    Ok,
}

/// `usercfg.status`: the verdict for the folder of a channel. The key is compared as it stands: `G_LANGUAGE` is
/// no `g_language`.
pub fn status(folder: &str) -> Status {
    if !Path::new(folder).join("Data.p4k").is_file() {
        return Status::NoInstall;
    }
    let Some(text) = read(folder) else { return Status::NoFile };
    if py_splitlines(&text).iter().any(|line| key(line) == "g_language") { Status::Ok } else { Status::NoLanguage }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_start;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    fn change(key: &str, value: Option<&str>) -> (String, Option<String>) {
        (key.to_string(), value.map(str::to_string))
    }

    #[test]
    fn own_lines_leave_out_the_managed_ones_and_the_empty_ones() {
        let text = "g_language = german_(germany)\n\nsys_MaxFPS = 60  \n  \n; a comment\ng_languageAudio=english\nG_LANGUAGE = x\n";
        assert_eq!(
            own_lines(text),
            ["sys_MaxFPS = 60", "; a comment", "G_LANGUAGE = x"],
            "the case of the key matters here"
        );
        assert_eq!(key("  sys_MaxFPS =60"), "sys_MaxFPS");
        assert_eq!(key("no equals sign"), "");
    }

    #[test]
    fn a_line_must_be_a_command_with_a_value_a_comment_or_empty() {
        let ok = lines("sys_MaxFPS = 60\n\n; comment\n-- also a comment\npl_pit.forceSoftwareCursor=1\n_x.y = a b c");
        assert!(check(&ok).is_empty(), "{:?}", check(&ok));
        let bad = lines("good = 1\nfoo\nkey=\n// no comment\n1abc = 2");
        assert_eq!(
            check(&bad).iter().map(|(n, l)| (*n, l.as_str())).collect::<Vec<_>>(),
            [(2, "foo"), (3, "key="), (4, "// no comment"), (5, "1abc = 2")]
        );
    }

    #[test]
    fn the_last_line_of_a_command_counts_and_the_case_does_not_matter() {
        let text = "sys_MaxFPS = 30\nSYS_MAXFPS=  60 ; x \nother = 1\n";
        assert_eq!(
            value_in(text, "sys_maxfps").as_deref(),
            Some("60 ; x"),
            "everything after the first = and nothing cut"
        );
        assert_eq!(value_in(text, "missing"), None);
    }

    #[test]
    fn commands_are_set_replaced_in_every_spelling_and_moved_to_the_end_and_the_managed_lines_stay_last() {
        let old = "g_language = german_(germany)\nsys_maxfps = 30\n\nr_DisplayInfo = 2\nkeep = 1\n";
        let text = set_text(
            old,
            &[change("sys_MaxFPS", Some(" 60 ")), change("r_DisplayInfo", None), change("sys_MaxIdleFPS", Some("20"))],
        )
        .expect("set");
        assert_eq!(
            text, "keep = 1\nsys_MaxFPS = 60\nsys_MaxIdleFPS = 20\ng_language = german_(germany)\n",
            "empty lines go, changed lines move to the end"
        );
        // An empty value takes the line out; so does None.
        assert_eq!(set_text("a = 1\nb = 2\n", &[change("a", Some("  "))]).unwrap(), "b = 2\n");
        // A managed key among the changes is dropped again.
        assert_eq!(set_text("g_language = x\n", &[change("g_language", Some("y"))]).unwrap(), "g_language = x\n");
        // An invalid line anywhere: nothing.
        let error = set_text("good = 1\nfoo\n", &[change("a", Some("1"))]).expect_err("refused");
        assert_eq!(error, [(2, "foo".to_string())]);
    }

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-usercfg-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A user whose game is quiet and the folder of the channel with the file.
    fn world(name: &str, age: Option<u64>, content: Option<&str>) -> (String, Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        if let Some(content) = content {
            std::fs::write(format!("{game}/user.cfg"), content).expect("file");
        }
        let mut vars = vec![("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        if let Some(age) = age {
            std::fs::write(format!("{game}/Game.log"), "x").expect("log");
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(format!("{game}/Game.log"))
                .and_then(|f| f.set_modified(at))
                .expect("age");
            vars.push((ENV_INSTALL_DIR, game.as_str()));
        }
        (home.clone(), Paths::new(Host::new(Flavor::Posix, &vars, &home)), game)
    }

    #[test]
    fn setting_writes_with_the_once_only_copy_and_a_rolling_one_and_changes_nothing_when_nothing_changes() {
        let (home, paths, game) = world("setzen", Some(7200), Some("g_language = german_(germany)\nsys_MaxFPS = 30\n"));
        let file = format!("{game}/user.cfg");
        set_values(&paths, &game, &[change("sys_MaxFPS", Some("60"))]).expect("set");
        let now = std::fs::read_to_string(&file).unwrap().replace("\r\n", "\n");
        assert_eq!(now, "sys_MaxFPS = 60\ng_language = german_(germany)\n");
        assert_eq!(
            std::fs::read_to_string(format!("{game}/{BACKUP}")).unwrap(),
            "g_language = german_(germany)\nsys_MaxFPS = 30\n",
            "the state before the first change"
        );
        assert_eq!(game_files::backups(&file).len(), 1, "and a rolling copy");
        // A second change: the once-only copy stays what it was.
        set_values(&paths, &game, &[change("sys_MaxFPS", Some("90"))]).expect("set");
        assert!(std::fs::read_to_string(format!("{game}/{BACKUP}")).unwrap().contains("sys_MaxFPS = 30"));
        // The same again: nothing is written, no new copy.
        let count = game_files::backups(&file).len();
        set_values(&paths, &game, &[change("sys_MaxFPS", Some("90"))]).expect("set");
        assert_eq!(game_files::backups(&file).len(), count);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn nothing_is_written_with_a_bad_line_in_the_file_or_while_the_game_runs() {
        let (home, paths, game) = world("schlecht", Some(7200), Some("foo\nsys_MaxFPS = 30\n"));
        let result = set_values(&paths, &game, &[change("sys_MaxFPS", Some("60"))]);
        assert!(matches!(result, Err(SetError::Bad(ref bad)) if bad == &[(1, "foo".to_string())]), "{result:?}");
        assert_eq!(std::fs::read_to_string(format!("{game}/user.cfg")).unwrap(), "foo\nsys_MaxFPS = 30\n");
        assert!(!Path::new(&format!("{game}/{BACKUP}")).exists());
        let _ = std::fs::remove_dir_all(home);
        let (home, paths, game) = world("laeuft", Some(5), Some("sys_MaxFPS = 30\n"));
        let result = set_values(&paths, &game, &[change("sys_MaxFPS", Some("60"))]);
        assert!(matches!(result, Err(SetError::Write(WriteError::GameRuns))), "{result:?}");
        assert_eq!(std::fs::read_to_string(format!("{game}/user.cfg")).unwrap(), "sys_MaxFPS = 30\n");
        assert!(!Path::new(&format!("{game}/{BACKUP}")).exists(), "not even the once-only copy");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_mark_of_the_byte_order_is_left_off_and_an_unchanged_file_is_not_written_or_copied() {
        let (home, paths, game) = world("bom", Some(7200), Some("\u{feff}sys_MaxFPS = 30\n"));
        assert_eq!(value(&game, "sys_MaxFPS").as_deref(), Some("30"), "the first line is read as any other");
        assert!(set_values(&paths, &game, &[change("sys_MaxFPS", Some("60"))]).expect("set"), "written");
        assert!(!set_values(&paths, &game, &[change("sys_MaxFPS", Some("60"))]).expect("set"), "the same: not written");
        assert_eq!(game_files::backups(&format!("{game}/user.cfg")).len(), 1, "one copy, not two");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_own_lines_are_written_as_they_stand_with_the_language_lines_behind_them_and_the_copy_before() {
        let old = "g_language = german_(germany)\nsys_MaxFPS = 30\ng_languageAudio = english\n";
        let (home, paths, game) = world("eigene", Some(7200), Some(old));
        // The player typed an empty line, a comment, a language line of his own and a command.
        let typed = lines("; mine\n\nr_DisplayInfo = 2\ng_language = x\nsys_MaxFPS=45");
        assert!(write_own(&paths, &game, &typed).expect("written"));
        let now = std::fs::read_to_string(format!("{game}/user.cfg")).unwrap().replace("\r\n", "\n");
        assert_eq!(
            now, "; mine\nr_DisplayInfo = 2\nsys_MaxFPS=45\ng_language = german_(germany)\ng_languageAudio = english\n",
            "empty lines and the typed language line go, the managed lines of the file stay, behind the rest"
        );
        assert_eq!(std::fs::read_to_string(format!("{game}/{BACKUP}")).unwrap(), old, "the state before");
        // Again with other lines: the once-only copy is not written over.
        assert!(write_own(&paths, &game, &lines("a = 1")).expect("written"));
        assert_eq!(std::fs::read_to_string(format!("{game}/{BACKUP}")).unwrap(), old);
        // The same text again: not written.
        assert!(!write_own(&paths, &game, &lines("a = 1")).expect("same"));
        // No lines at all: only the managed ones are left, and the line end is there.
        assert!(write_own(&paths, &game, &[]).expect("written"));
        let now = std::fs::read_to_string(format!("{game}/user.cfg")).unwrap().replace("\r\n", "\n");
        assert_eq!(now, "g_language = german_(germany)\ng_languageAudio = english\n");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn one_bad_line_or_a_running_game_and_the_own_lines_are_not_written_and_not_even_copied() {
        let old = "g_language = german_(germany)\nsys_MaxFPS = 30\n";
        let (home, paths, game) = world("eigene-schlecht", Some(7200), Some(old));
        let result = write_own(&paths, &game, &lines("good = 1\nfoo\n; fine\nkey=\n"));
        assert!(
            matches!(result, Err(SetError::Bad(ref bad)) if bad == &[(2, "foo".to_string()), (4, "key=".to_string())]),
            "{result:?}"
        );
        assert_eq!(std::fs::read_to_string(format!("{game}/user.cfg")).unwrap(), old);
        assert!(!Path::new(&format!("{game}/{BACKUP}")).exists(), "no copy for a refused writing");
        let _ = std::fs::remove_dir_all(home);
        let (home, paths, game) = world("eigene-laeuft", Some(5), Some(old));
        let result = write_own(&paths, &game, &lines("good = 1"));
        assert!(matches!(result, Err(SetError::Write(WriteError::GameRuns))), "{result:?}");
        assert_eq!(std::fs::read_to_string(format!("{game}/user.cfg")).unwrap(), old);
        assert!(!Path::new(&format!("{game}/{BACKUP}")).exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn an_empty_box_makes_no_file_and_the_once_only_copy_is_made_in_one_piece_without_leftovers() {
        let (home, paths, game) = world("eigene-leer", Some(7200), None);
        assert!(!write_own(&paths, &game, &[]).expect("nothing to write"), "no lines, no file: nothing written");
        assert!(!write_own(&paths, &game, &lines("\n   \n")).expect("nothing to write"));
        assert!(!Path::new(&format!("{game}/user.cfg")).exists(), "no file was made for an empty box");
        // With a file: the copy stands whole and no half-made part is left beside it.
        std::fs::write(format!("{game}/user.cfg"), "g_language = x\nsys_MaxFPS = 30\n").unwrap();
        assert!(write_own(&paths, &game, &lines("a = 1")).expect("written"));
        let copy = std::fs::read_to_string(format!("{game}/{BACKUP}")).unwrap();
        assert_eq!(copy, "g_language = x\nsys_MaxFPS = 30\n");
        assert!(!Path::new(&format!("{game}/{BACKUP}.tmp")).exists(), "the part was renamed");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_told_apart_from_no_file() {
        let (home, _paths, game) = world("lesen", Some(7200), None);
        assert!(read_checked(&game).expect("no file is no error").is_none());
        std::fs::write(format!("{game}/user.cfg"), "a = 1\n").unwrap();
        assert_eq!(read_checked(&game).expect("read").as_deref(), Some("a = 1\n"));
        // A folder where the file should be: it is there, and it cannot be read.
        std::fs::remove_file(format!("{game}/user.cfg")).unwrap();
        std::fs::create_dir(format!("{game}/user.cfg")).unwrap();
        assert!(read_checked(&game).is_err());
        assert_eq!(read(&game), None, "the plain reading cannot tell them apart");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_status_of_a_channel_is_none_without_file_without_language_or_ok() {
        let (home, _paths, game) = world("status", Some(7200), Some("sys_MaxFPS = 30\n"));
        assert_eq!(status(&game), Status::NoInstall, "no Data.p4k");
        std::fs::write(format!("{game}/Data.p4k"), "").unwrap();
        assert_eq!(status(&game), Status::NoLanguage, "a file without g_language");
        std::fs::write(format!("{game}/user.cfg"), "G_LANGUAGE = german_(germany)\n").unwrap();
        assert_eq!(status(&game), Status::NoLanguage, "the case of the key matters");
        std::fs::write(format!("{game}/user.cfg"), "a = 1\n g_language=english\n").unwrap();
        assert_eq!(status(&game), Status::Ok);
        std::fs::remove_file(format!("{game}/user.cfg")).unwrap();
        assert_eq!(status(&game), Status::NoFile);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_missing_file_is_made_and_no_copy_of_nothing_is_kept() {
        let (home, paths, game) = world("neu", Some(7200), None);
        set_values(&paths, &game, &[change("r_DisplayInfo", Some("2"))]).expect("set");
        assert_eq!(
            std::fs::read_to_string(format!("{game}/user.cfg")).unwrap().replace("\r\n", "\n"),
            "r_DisplayInfo = 2\n"
        );
        assert!(!Path::new(&format!("{game}/{BACKUP}")).exists());
        assert_eq!(value(&game, "R_DISPLAYINFO").as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(home);
    }
}
