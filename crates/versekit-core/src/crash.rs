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

//! The crash file (`errors.install_crash_handler`): what a run that ended
//! hard leaves behind, so the next start can name it in the report.
//!
//! At every start the file of the last run is set aside when it holds
//! anything (`absturz.txt` → `absturz-letzter.txt`, the version next to it in
//! `absturz-fassung.txt` → `absturz-letzter-fassung.txt`) and a fresh, empty
//! one is opened. A run that ends cleanly leaves it empty. What ends a run
//! hard is written into it: a panic of the main thread (by the panic hook
//! of [`crate::diagnostics`]) and, on Windows, an exception no code caught.
//!
//! The text is laid out like the dump of Python's `faulthandler`: a head,
//! then `Current thread …` with the places, most recent call first. The
//! report ([`crate::report`]) brings the block of the guilty thread to the
//! front.
//!
//! Never fails the program: every function swallows its own errors.

use std::fs::{self, File};
use std::io::Write;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use crate::jsonio;
use crate::paths::Paths;
use crate::text::{py_rstrip, py_strip};

pub const FILE: &str = "absturz.txt";
pub const PREVIOUS: &str = "absturz-letzter.txt";
/// The version that belongs to the crash file, in a file of its own: a head
/// line in the crash file itself would make every clean run look like a crash.
pub const VERSION_FILE: &str = "absturz-fassung.txt";
pub const VERSION_PREVIOUS: &str = "absturz-letzter-fassung.txt";

/// The four files of one process: the crash file of the run, the one set
/// aside from the run before, and the version of each.
#[derive(Debug, PartialEq, Eq)]
pub struct Files {
    pub current: &'static str,
    pub previous: &'static str,
    pub version_current: &'static str,
    pub version_previous: &'static str,
}

/// The files of the main window.
pub const MAIN: Files =
    Files { current: FILE, previous: PREVIOUS, version_current: VERSION_FILE, version_previous: VERSION_PREVIOUS };
/// The files of the overlay.
pub const OVERLAY: Files = Files {
    current: "absturz-overlay.txt",
    previous: "absturz-overlay-letzter.txt",
    version_current: "absturz-overlay-fassung.txt",
    version_previous: "absturz-overlay-letzter-fassung.txt",
};
/// The files of the teaching window.
pub const LEARN: Files = Files {
    current: "absturz-anlernen.txt",
    previous: "absturz-anlernen-letzter.txt",
    version_current: "absturz-anlernen-fassung.txt",
    version_previous: "absturz-anlernen-letzter-fassung.txt",
};

/// The open crash file for the whole run; the dump is written into it.
static STREAM: Mutex<Option<File>> = Mutex::new(None);

/// [`install_files`] with the files of the main window.
pub fn install(paths: &Paths, version: &str) -> bool {
    install_files(paths, &MAIN, version)
}

/// Sets the last run's crash file aside and opens a fresh empty one.
/// True if the crash file is ready.
pub fn install_files(paths: &Paths, files: &Files, version: &str) -> bool {
    let current = paths.app_file(files.current);
    let previous = paths.app_file(files.previous);
    let version_current = paths.app_file(files.version_current);
    let version_previous = paths.app_file(files.version_previous);
    // What stands in it from the last run is a crash. The first step that
    // fails ends this part, like Python's single `try`.
    let _ = set_aside(&current, &previous, &version_current, &version_previous);
    let _ = jsonio::write_new(&version_current, version.as_bytes());
    let Ok(file) = open_fresh(&current) else { return false };
    keep_stream(file);
    true
}

/// [`install_files`] for a second process of the same role while the first still runs: the crash file of the
/// first stays as it is (nothing set aside, the version left alone), a dump of this process goes at its end.
/// True if the crash file is ready.
pub fn join_files(paths: &Paths, files: &Files) -> bool {
    let Ok(file) = open_append(&paths.app_file(files.current)) else { return false };
    keep_stream(file);
    true
}

/// Makes `file` the crash file of the run and hooks up the exception filter.
fn keep_stream(file: File) {
    if let Ok(mut slot) = STREAM.lock() {
        *slot = Some(file);
    }
    #[cfg(windows)]
    seh::install();
}

/// What no code catches on Windows (an access violation, an illegal
/// instruction): the exception filter writes code and address into the crash
/// file and lets the system end the program as usual.
///
/// ⚠ No call stack: walking the stack of a damaged process is itself a risk.
/// A stack overflow does not get here (Rust ends the program on its own).
#[cfg(windows)]
mod seh {
    use windows_sys::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};

    /// Let the search for another handler go on: the system ends the program.
    const CONTINUE_SEARCH: i32 = 0;

    pub fn install() {
        // SAFETY: `filter` has the signature the system calls it with and
        // lives for the whole run.
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        // SAFETY: the system passes a valid structure; a null pointer is checked.
        let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
        if let Some(record) = record {
            let thread = std::thread::current();
            let dump = super::exception_dump(
                record.ExceptionCode as u32,
                record.ExceptionAddress as usize,
                thread.name().unwrap_or("?"),
            );
            super::write_dump(&dump);
        }
        CONTINUE_SEARCH
    }
}

/// The dump of a Windows exception, laid out like the dump of a panic.
pub fn exception_dump(code: u32, address: usize, thread: &str) -> String {
    format!(
        "Windows fatal exception: code 0x{code:08x} at address 0x{address:x}\n\nCurrent thread '{thread}' (most recent call first):\n  (call stack not available)\n"
    )
}

fn is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

fn set_aside(current: &str, previous: &str, version_current: &str, version_previous: &str) -> std::io::Result<()> {
    if is_file(current) && fs::metadata(current)?.len() > 0 {
        if is_file(previous) {
            jsonio::remove_file(previous)?;
        }
        jsonio::os_replace(current, previous)?;
        // The version goes with it; without one, the old one must not stay.
        if is_file(version_current) {
            jsonio::os_replace(version_current, version_previous)?;
        } else if is_file(version_previous) {
            jsonio::remove_file(version_previous)?;
        }
    } else if is_file(current) {
        jsonio::remove_file(current)?;
    }
    Ok(())
}

fn open_fresh(path: &str) -> std::io::Result<File> {
    crate::writeguard::check("open-w", path)?;
    File::create(path)
}

fn open_append(path: &str) -> std::io::Result<File> {
    crate::writeguard::check("open-a", path)?;
    fs::OpenOptions::new().append(true).create(true).open(path)
}

/// Writes a dump into the open crash file; a best effort that never blocks
/// (a panic while the file is being written must not wait for itself).
pub fn write_dump(text: &str) -> bool {
    let Ok(mut slot) = STREAM.try_lock() else { return false };
    let Some(file) = slot.as_mut() else { return false };
    let ok = file.write_all(text.as_bytes()).is_ok() && file.flush().is_ok();
    // Forced to disk: the process may be gone a moment later.
    let _ = file.sync_all();
    ok
}

/// The dump of a panic, laid out like a `faulthandler` dump.
///
/// `places` are the lines of the call stack, most recent first; `location`
/// is where the panic was raised.
pub fn panic_dump(thread: &str, message: &str, location: Option<&str>, places: &[String]) -> String {
    let mut out =
        format!("Fatal error: panic: {}\n\nCurrent thread '{thread}' (most recent call first):\n", first_line(message));
    if let Some(location) = location {
        out.push_str(&format!("  at {location}\n"));
    }
    for place in places {
        out.push_str("  ");
        out.push_str(place);
        out.push('\n');
    }
    out
}

/// The dump of a hard end the program tells itself (no panic, no exception): `message` as the head, `lines` as the
/// places, in the layout of [`panic_dump`].
pub fn end_dump(message: &str, lines: &[String]) -> String {
    let mut out = format!("Fatal error: {}\n\nCurrent thread 'main' (most recent call first):\n", first_line(message));
    for line in lines {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Writes `dump` at the end of the crash file of `files` in `paths`, with `version` next to it, for a process
/// whose crash file is not open ([`install_files`] did not run). True if it is written.
pub fn append_dump(paths: &Paths, files: &Files, dump: &str, version: &str) -> bool {
    let Ok(mut file) = open_append(&paths.app_file(files.current)) else { return false };
    let ok = file.write_all(dump.as_bytes()).is_ok() && file.flush().is_ok();
    let _ = file.sync_all();
    let _ = jsonio::write_new(&paths.app_file(files.version_current), version.as_bytes());
    ok
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// The call stack as lines (`name` and `at file:line` alternating), without
/// the frames of the panic machinery itself, at most `limit` lines.
pub fn stack_lines(backtrace: &str, limit: usize) -> Vec<String> {
    const MACHINERY: [&str; 5] =
        ["rust_begin_unwind", "core::panicking", "std::panicking", "backtrace::Backtrace", "panic_fmt"];
    let lines: Vec<&str> = backtrace.lines().map(py_strip).filter(|l| !l.is_empty()).collect();
    // Everything up to the last frame of the panic machinery belongs to the
    // panic itself, with the `at` lines under those frames.
    let mut start = lines.iter().rposition(|l| MACHINERY.iter().any(|m| l.contains(m))).map_or(0, |at| at + 1);
    while lines.get(start).is_some_and(|l| l.starts_with("at ")) {
        start += 1;
    }
    lines.into_iter().skip(start).take(limit).map(str::to_string).collect()
}

/// The crash file of the last run of the main window as lines (not blank,
/// trailing space cut); empty if there was none.
pub fn last_crash(paths: &Paths) -> Vec<String> {
    last_crash_of(paths, &MAIN)
}

/// [`last_crash`] of the process with `files`.
pub fn last_crash_of(paths: &Paths, files: &Files) -> Vec<String> {
    dump_lines(&paths.app_file(files.previous))
}

/// The lines of a crash file that say something; empty if there is none.
fn dump_lines(path: &str) -> Vec<String> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    let Ok(text) = String::from_utf8(bytes) else { return Vec::new() };
    crate::text::lines_universal(&text)
        .into_iter()
        .filter(|l| !py_strip(l).is_empty())
        .map(|l| py_rstrip(l).to_string())
        .collect()
}

/// A hard crash of another process: its lines, when (seconds since 1970) and
/// in which version.
#[derive(Debug, Default, PartialEq)]
pub struct Found {
    pub lines: Vec<String>,
    pub at: Option<f64>,
    pub version: String,
}

/// The newest hard crash of the process with `files`, seen from another
/// process: a crash file that is not empty means that process ended hard and
/// was not started since; else the one set aside at its last start.
pub fn newest_crash_of(paths: &Paths, files: &Files) -> Found {
    let current = paths.app_file(files.current);
    let lines = dump_lines(&current);
    if lines.is_empty() {
        return Found {
            lines: last_crash_of(paths, files),
            at: crash_time_of(paths, files),
            version: crash_version_of(paths, files),
        };
    }
    let at = fs::metadata(&current)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());
    let version = fs::read(paths.app_file(files.version_current))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|t| py_strip(&t).to_string())
        .unwrap_or_default();
    Found { lines, at, version }
}

/// When the set-aside crash happened (seconds since 1970), or `None`.
///
/// ⚠ The file stays until a new crash replaces it. Without the date a crash
/// of days ago would pass for the one of the last run.
pub fn crash_time(paths: &Paths) -> Option<f64> {
    crash_time_of(paths, &MAIN)
}

/// [`crash_time`] of the process with `files`.
pub fn crash_time_of(paths: &Paths, files: &Files) -> Option<f64> {
    let modified = fs::metadata(paths.app_file(files.previous)).ok()?.modified().ok()?;
    modified.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs_f64())
}

/// The version of the set-aside crash, or empty if unknown.
pub fn crash_version(paths: &Paths) -> String {
    crash_version_of(paths, &MAIN)
}

/// [`crash_version`] of the process with `files`.
pub fn crash_version_of(paths: &Paths, files: &Files) -> String {
    let Ok(bytes) = fs::read(paths.app_file(files.version_previous)) else { return String::new() };
    String::from_utf8(bytes).map(|t| py_strip(&t).to_string()).unwrap_or_default()
}

/// Removes the set-aside crash: it is reported and done.
pub fn clear_crash(paths: &Paths) -> bool {
    if jsonio::remove_file(&paths.app_file(PREVIOUS)).is_err() {
        return false;
    }
    let _ = jsonio::remove_file(&paths.app_file(VERSION_PREVIOUS));
    true
}

/// The setting that holds which crash of the main window the player was told of: the time of its file in
/// milliseconds since 1970.
pub const TOLD_SETTING: &str = "absturz_gemeldet";

/// A crash of the last run of the main window the player was not told of yet: its first line, when and in which
/// version.
#[derive(Clone, Debug, PartialEq)]
pub struct Untold {
    pub first: String,
    pub at: Option<f64>,
    pub version: String,
}

/// The mark of the set-aside crash: the time of its file in milliseconds, 0 where the time cannot be read.
fn crash_mark(paths: &Paths) -> i64 {
    crash_time(paths).map_or(0, |at| (at * 1000.0).round() as i64)
}

/// The set-aside crash of the main window, where the player was not told of it yet; `None` without one.
pub fn untold(paths: &Paths) -> Option<Untold> {
    let first = last_crash(paths).into_iter().next()?;
    if paths.setting_int(TOLD_SETTING, -1, None, None) == crash_mark(paths) {
        return None;
    }
    Some(Untold { first, at: crash_time(paths), version: crash_version(paths) })
}

/// Notes that the player was told of the set-aside crash: [`untold`] has nothing until the next crash.
pub fn mark_told(paths: &Paths) -> bool {
    paths.set_setting(TOLD_SETTING, crate::jsonio::Value::int(crash_mark(paths)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-absturz-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        let value = dir.to_string_lossy().to_string();
        let home = [("SC_BP_HOME", value.as_str()), ("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        (Paths::new(Host::new(Flavor::native(), &home, "/")), dir)
    }

    #[test]
    fn a_dump_is_set_aside_with_its_version() {
        let (paths, dir) = paths_in("beiseite");
        fs::write(
            dir.join(FILE),
            "Fatal error: panic: x\n\nCurrent thread 'main' (most recent call first):\n  at a.rs:1:1\n",
        )
        .unwrap();
        fs::write(dir.join(VERSION_FILE), "5.0.0").unwrap();
        assert!(install(&paths, "5.0.1"));
        assert_eq!(last_crash(&paths).len(), 3);
        assert_eq!(crash_version(&paths), "5.0.0");
        assert!(crash_time(&paths).is_some());
        // The fresh crash file is empty and the new version stands next to it.
        assert_eq!(fs::metadata(dir.join(FILE)).unwrap().len(), 0);
        assert_eq!(fs::read_to_string(dir.join(VERSION_FILE)).unwrap(), "5.0.1");
        assert!(clear_crash(&paths));
        assert!(last_crash(&paths).is_empty());
        assert_eq!(crash_version(&paths), "");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A crash is told once: after the mark it is not untold any more, a newer crash is, and a clean run has none.
    #[test]
    fn a_crash_is_untold_until_marked_and_the_next_crash_is_new() {
        let (paths, dir) = paths_in("gemeldet");
        assert!(install(&paths, "5.2.0"));
        assert_eq!(untold(&paths), None, "no crash");
        fs::write(dir.join(FILE), "Fatal error: panic: eins\n\nCurrent thread 'main':\n").unwrap();
        assert!(install(&paths, "5.2.1"));
        let found = untold(&paths).expect("the crash of the run before");
        assert_eq!(found.first, "Fatal error: panic: eins");
        assert_eq!(found.version, "5.2.0");
        assert!(found.at.is_some());
        assert!(mark_told(&paths));
        assert_eq!(untold(&paths), None, "told once");
        // A clean run keeps the crash set aside and still told.
        assert!(install(&paths, "5.2.1"));
        assert_eq!(untold(&paths), None, "not again after a clean run");
        // A new crash, a little later: untold again.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join(FILE), "Fatal error: panic: zwei\n").unwrap();
        assert!(install(&paths, "5.2.1"));
        assert_eq!(untold(&paths).map(|u| u.first).as_deref(), Some("Fatal error: panic: zwei"));
        // Cleared ("Nicht senden"): nothing is left to tell, nor in the report.
        assert!(clear_crash(&paths));
        assert_eq!(untold(&paths), None);
        assert!(last_crash(&paths).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_run_leaves_nothing_behind() {
        let (paths, dir) = paths_in("sauber");
        assert!(install(&paths, "5.0.0"));
        // Next start: the empty file is removed, nothing is set aside.
        assert!(install(&paths, "5.0.1"));
        assert!(last_crash(&paths).is_empty());
        assert_eq!(crash_time(&paths), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_version_missing_next_to_a_dump_does_not_pass_on_the_old_one() {
        let (paths, dir) = paths_in("ohne-fassung");
        fs::write(dir.join(VERSION_PREVIOUS), "4.9.0").unwrap();
        fs::write(dir.join(FILE), "Fatal error: panic: y\n").unwrap();
        assert!(install(&paths, "5.0.0"));
        assert_eq!(last_crash(&paths), ["Fatal error: panic: y"]);
        assert_eq!(crash_version(&paths), "", "the old version must not belong to the new crash");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_overlay_keeps_its_crash_apart_from_the_main_window() {
        let (paths, dir) = paths_in("rollen");
        fs::write(dir.join(OVERLAY.current), "Fatal error: panic: overlay\n").unwrap();
        fs::write(dir.join(FILE), "Fatal error: panic: hauptfenster\n").unwrap();
        // The overlay starts anew: only its own file is set aside.
        assert!(install_files(&paths, &OVERLAY, "5.0.1"));
        assert_eq!(last_crash_of(&paths, &OVERLAY), ["Fatal error: panic: overlay"]);
        assert_eq!(crash_version_of(&paths, &OVERLAY), "");
        assert!(crash_time_of(&paths, &OVERLAY).is_some());
        assert!(last_crash(&paths).is_empty(), "the main window's crash is not set aside by the overlay");
        assert_eq!(fs::read_to_string(dir.join(FILE)).unwrap(), "Fatal error: panic: hauptfenster\n");
        assert_eq!(fs::read_to_string(dir.join(OVERLAY.version_current)).unwrap(), "5.0.1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_process_of_the_role_leaves_the_files_of_the_first_and_writes_at_their_end() {
        let (paths, dir) = paths_in("zweites");
        fs::write(dir.join(FILE), "Fatal error: panic: erstes\n").unwrap();
        fs::write(dir.join(PREVIOUS), "Fatal error: panic: davor\n").unwrap();
        fs::write(dir.join(VERSION_FILE), "5.0.0").unwrap();
        assert!(join_files(&paths, &MAIN));
        // The file as `join_files` opens it; the global stream may belong to another test by now.
        let mut joined = open_append(&paths.app_file(FILE)).unwrap();
        joined.write_all(b"Fatal error: panic: zweites\n").unwrap();
        drop(joined);
        assert_eq!(
            fs::read_to_string(dir.join(FILE)).unwrap(),
            "Fatal error: panic: erstes\nFatal error: panic: zweites\n",
            "nothing cut, the dump of the second goes at the end"
        );
        assert_eq!(last_crash(&paths), ["Fatal error: panic: davor"], "nothing set aside");
        assert_eq!(fs::read_to_string(dir.join(VERSION_FILE)).unwrap(), "5.0.0", "the version stays");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_process_that_ended_hard_and_was_not_started_since_is_seen_by_the_others() {
        let (paths, dir) = paths_in("andere");
        assert_eq!(newest_crash_of(&paths, &OVERLAY), Found::default(), "nothing left behind");
        // Set aside at its last start.
        fs::write(dir.join(OVERLAY.previous), "Fatal error: panic: alt\n").unwrap();
        fs::write(dir.join(OVERLAY.version_previous), "5.0.0").unwrap();
        let found = newest_crash_of(&paths, &OVERLAY);
        assert_eq!((found.lines, found.version), (vec!["Fatal error: panic: alt".to_string()], "5.0.0".to_string()));
        // Ended hard just now and not started again: the open file holds the dump, and it is the newer one.
        fs::write(dir.join(OVERLAY.current), "Fatal error: panic: eben\n").unwrap();
        fs::write(dir.join(OVERLAY.version_current), "5.0.1").unwrap();
        let found = newest_crash_of(&paths, &OVERLAY);
        assert_eq!(found.lines, ["Fatal error: panic: eben"]);
        assert_eq!(found.version, "5.0.1");
        assert!(found.at.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dump_has_the_shape_the_report_reads() {
        let places = vec!["0: versekit_ui::page::show".to_string(), "at crates/x.rs:3".to_string()];
        let dump = panic_dump("main", "boom\nsecond line", Some("crates/x.rs:9:5"), &places);
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines[0], "Fatal error: panic: boom");
        assert_eq!(lines[2], "Current thread 'main' (most recent call first):");
        assert_eq!(lines[3], "  at crates/x.rs:9:5");
        assert_eq!(lines[4], "  0: versekit_ui::page::show");
    }

    #[test]
    fn a_hard_end_without_an_open_crash_file_is_written_into_it_and_set_aside_at_the_next_start() {
        let (paths, dir) = paths_in("selbst-beendet");
        let lines = vec!["grafikkarte-fenster: Invalid surface".to_string()];
        let dump = end_dump("Grafik: kein Weg zum Zeichnen\nzweite Zeile", &lines);
        assert!(dump.starts_with("Fatal error: Grafik: kein Weg zum Zeichnen\n\nCurrent thread 'main'"), "{dump}");
        assert!(append_dump(&paths, &MAIN, &dump, "5.0.0"));
        assert!(install(&paths, "5.0.1"));
        assert_eq!(
            last_crash(&paths),
            [
                "Fatal error: Grafik: kein Weg zum Zeichnen",
                "Current thread 'main' (most recent call first):",
                "  grafikkarte-fenster: Invalid surface"
            ]
        );
        assert_eq!(crash_version(&paths), "5.0.0");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stack_drops_the_frames_of_the_panic_itself() {
        let raw = "   0: std::backtrace::Backtrace::force_capture\n   1: core::panicking::panic_fmt\n             at /rustc/x/library/core/src/panicking.rs:80\n   2: rust_begin_unwind\n   3: versekit_ui::page::show\n             at crates/ui/src/page.rs:12\n";
        let lines = stack_lines(raw, 10);
        assert!(lines[0].contains("versekit_ui::page::show"), "{lines:?}");
    }
}
