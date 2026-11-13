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

//! The diagnostics of the running program, wired up once at start
//! (`errors.install_hooks` and `install_crash_handler`):
//!
//! * the error log `fehler.json` of the data folder, fed by everything that
//!   calls [`crate::errors::record`] and by the panic hook,
//! * the crash file ([`crate::crash`]), written when the main thread panics,
//! * the start trail ([`crate::trail`]).
//!
//! The panic hook is the counterpart of Python's three hooks: a panic of the
//! main thread is logged as `unbehandelt` and ends the run with a dump in the
//! crash file, a panic of another thread is logged as `thread:<name>`. The
//! message of the previous hook (standard error) is kept.
//!
//! Each process of the program has a [`Role`]. The error log is one for all of
//! them; the crash file and the start trail are the role's own, since every
//! start sets them anew and the overlay, the main window and the teaching
//! window run side by side.
//!
//! Nothing here may fail the program, and nothing sends anything.

use std::cell::Cell;
use std::panic::PanicHookInfo;
use std::sync::OnceLock;

use crate::crash;
use crate::error_log::{ErrorLog, Problem};
use crate::paths::{self, Paths};

/// Which process of the program this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The main window (`--hauptfenster`); the report is made in it.
    Main,
    /// The overlay above the game.
    Overlay,
    /// The small window for teaching the digits of the signature scanner.
    Learn,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Main, Role::Overlay, Role::Learn];

    /// The crash files of the role.
    pub fn crash_files(self) -> &'static crash::Files {
        match self {
            Role::Main => &crash::MAIN,
            Role::Overlay => &crash::OVERLAY,
            Role::Learn => &crash::LEARN,
        }
    }

    /// The start trail of the role.
    pub fn trail_file(self) -> &'static str {
        match self {
            Role::Main => crate::trail::FILE,
            Role::Overlay => crate::trail::FILE_OVERLAY,
            Role::Learn => crate::trail::FILE_LEARN,
        }
    }
}

static LOG: OnceLock<ErrorLog<'static>> = OnceLock::new();
/// The data folder and the role [`init_for`] was given.
static RUN: OnceLock<(&'static Paths, Role)> = OnceLock::new();

thread_local! {
    /// Set while a record is being made: writing the log may itself fail
    /// and report, which would call back in here.
    static RECORDING: Cell<bool> = const { Cell::new(false) };
}

/// The error log of the running program, once [`init`] has run.
pub fn error_log() -> Option<&'static ErrorLog<'static>> {
    LOG.get()
}

/// Records an error under its place; true if it was written. Errors are
/// always recorded: the switch of 4.12 never had an effect and is gone, and
/// its old key is neither read nor written.
pub fn record(label: &str, kind: &str, message: &str, trace: Option<&str>) -> bool {
    let Some(log) = error_log() else { return false };
    if RECORDING.with(|r| r.replace(true)) {
        return false;
    }
    let written = log.record(label, Some(Problem { kind, message, trace }), "");
    RECORDING.with(|r| r.set(false));
    written
}

/// Records a note (kind `Hinweis`) under its place; true if it was written. The report shows it with the
/// errors, the button "Fehler melden" does not count it ([`ErrorLog::unseen`]).
pub fn note(label: &str, message: &str) -> bool {
    let Some(log) = error_log() else { return false };
    if RECORDING.with(|r| r.replace(true)) {
        return false;
    }
    let written = log.record(label, None, message);
    RECORDING.with(|r| r.set(false));
    written
}

/// The data folder and the role of the running program, once [`init_for`] has run.
pub fn current() -> Option<(&'static Paths, Role)> {
    RUN.get().copied()
}

/// Wires everything up for the paths of the running program. Call once, right
/// after the start, with the running version; false if it was already set up.
pub fn init(role: Role, version: &str) -> bool {
    init_for(paths::global(), role, version)
}

/// [`init`] for the data of `paths` instead of the paths of the running
/// program: a program that works on a copy of the data (`--daten`, a tool, a
/// test) gets its errors, its crash file and its trail there, and the real
/// data stay as they are. The first line of the trail names the version and
/// the system.
pub fn init_for(paths: &'static Paths, role: Role, version: &str) -> bool {
    if LOG.set(ErrorLog::new(paths, version)).is_err() {
        return false;
    }
    let _ = RUN.set((paths, role));
    let (first, lock) = claim(paths, role);
    if let Some(lock) = lock {
        let _ = ROLE_LOCK.set(lock);
    }
    if first {
        crash::install_files(paths, role.crash_files(), version);
    } else {
        crash::join_files(paths, role.crash_files());
        crate::trail::TRAIL.join();
    }
    crate::errors::set_sink(|label, message| {
        record(label, "Fehler", message, None);
    });
    crate::errors::set_note_sink(|label, message| {
        note(label, message);
    });
    install_panic_hook();
    let line = start_line(version);
    crate::trail::step(&if first { line } else { format!("{line}, {SECOND}") });
    true
}

/// The note on the start line of a process that joined the files of a running one of its role.
pub const SECOND: &str = "weiteres Fenster";

/// The lock held for the whole run by the process that owns the files of its role.
static ROLE_LOCK: OnceLock<crate::jsonio::ProcessLock> = OnceLock::new();

/// The lock file of `role` in the run of `paths`, named after the data folder, so a copy of the data has its own:
/// next to the lock of the saves (beside the second pointer, which a copy keeps inside the copy), or, with the data
/// folder named from outside (`ENV_HOME`, as test runs do), in that folder, so such a run leaves nothing beside the
/// pointers of the user.
pub fn role_lock_file(paths: &Paths, role: Role) -> String {
    let flavor = paths.host.flavor;
    let data = paths.app_folder();
    let key = crate::window_signal::folder_key(&data);
    let name = match role {
        Role::Main => "hauptfenster",
        Role::Overlay => "overlay",
        Role::Learn => "anlernen",
    };
    let folder = match paths.host.var_set(crate::legacy::ENV_HOME) {
        Some(_) => data,
        None => flavor.dirname(&paths.second_pointer()),
    };
    flavor.join(&folder, &format!("sperre-diagnose-{name}-{key}.lock"))
}

/// Does this process own the crash file and the trail of `role` (no other process of the run holds them)? With
/// the lock to keep while it runs. A lock that cannot be had for another reason than "held" (a folder that cannot
/// be made, a file system without locks) counts as owning, as before there was a lock.
pub fn claim(paths: &Paths, role: Role) -> (bool, Option<crate::jsonio::ProcessLock>) {
    use crate::jsonio::{ProcessLock, Taken};
    let Ok(lock) = ProcessLock::open(&role_lock_file(paths, role)) else { return (true, None) };
    match lock.acquire(std::time::Duration::ZERO) {
        Taken::Held => (true, Some(lock)),
        Taken::TimedOut => (false, None),
        Taken::Unsupported(_) => (true, None),
    }
}

/// Notes a hard end the program tells itself (no panic, no exception) under `label` in the error log and in the
/// crash file of the running role, `lines` as its places; without diagnostics ([`init_for`] did not run) in the
/// crash file of the main window in `fallback`, with `version` next to it. True if the crash file holds it.
pub fn note_hard_end(fallback: &Paths, label: &str, message: &str, lines: &[String], version: &str) -> bool {
    record(label, "Fehler", message, Some(&lines.join("\n")));
    let dump = crash::end_dump(message, lines);
    if crash::write_dump(&dump) {
        return true;
    }
    match current() {
        Some((paths, role)) => crash::append_dump(paths, role.crash_files(), &dump, version),
        None => crash::append_dump(fallback, &crash::MAIN, &dump, version),
    }
}

/// The label of the choice of the graphics stage (`versekit_ui::renderer`) in the error log.
pub const GRAPHICS: &str = "grafik";
/// The head of the crash file when no graphics stage works.
pub const GRAPHICS_NONE: &str = "Grafik: kein Weg zum Zeichnen";
/// The head of the crash file and of the error when the window fails after the choice.
pub const GRAPHICS_WINDOW: &str = "Grafik: Fehler im Fenster";
/// The note when the graphics device was lost while the window ran and the program starts anew.
pub const GRAPHICS_LOST: &str = "Gerät verloren, Programm startet neu";

/// The graphics device of this process is lost and the program starts anew: a panic from now on is the end of the
/// window, noted as such and no crash ([`mark_graphics_lost`]).
static GRAPHICS_LOST_MARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on a panic of this process is the end of a window whose graphics device was lost, and the program starts
/// anew: noted in the error log, without a crash file.
pub fn mark_graphics_lost() {
    GRAPHICS_LOST_MARK.store(true, std::sync::atomic::Ordering::Release);
}

/// Is a panic now the end of a window with a lost graphics device ([`mark_graphics_lost`])?
fn graphics_lost() -> bool {
    GRAPHICS_LOST_MARK.load(std::sync::atomic::Ordering::Acquire)
}

/// The line of the trail and the error log about the graphics stage: `Grafik: grafikkarte-dcomp`.
pub fn graphics_line(text: &str) -> String {
    format!("Grafik: {text}")
}

/// The first line of the trail: `Start, Version 5.0.0, windows`.
pub fn start_line(version: &str) -> String {
    format!("Start, Version {version}, {}", std::env::consts::OS)
}

/// The text of a panic payload.
fn payload_text(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "panic".to_string()
    }
}

/// The trace stored with the error: the place of the panic in the shape of a
/// Python call site, which the report reads (`File "…", line N, in …`).
pub fn panic_trace(file: &str, line: u32, thread: &str, message: &str) -> String {
    let first = message.lines().next().unwrap_or_default();
    format!("File \"{file}\", line {line}, in {thread}\n    {first}")
}

/// The note of a panic that ends a window whose graphics device was lost: the place and the first line of `message`.
fn lost_note(message: &str, file: &str, line: u32) -> String {
    let first = message.lines().next().unwrap_or_default();
    format!("{}: {first} ({file}:{line})", graphics_line(GRAPHICS_LOST))
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let current = std::thread::current();
        let thread = current.name().unwrap_or("?").to_string();
        let message = payload_text(info);
        let (file, line, column) = info.location().map_or(("?", 0, 0), |l| (l.file(), l.line(), l.column()));
        let trace = panic_trace(file, line, &thread, &message);
        let main = thread == "main";
        if main && graphics_lost() {
            note(GRAPHICS, &lost_note(&message, file, line));
            previous(info);
            return;
        }
        let label = if main { "unbehandelt".to_string() } else { format!("thread:{thread}") };
        record(&label, "panic", &message, Some(&trace));
        if main {
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let places = crash::stack_lines(&backtrace, 60);
            let location = format!("{file}:{line}:{column}");
            crash::write_dump(&crash::panic_dump(&thread, &message, Some(&location), &places));
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_end_of_a_window_with_a_lost_device_is_noted_with_its_place() {
        let note = lost_note("Failed to create staging buffer for index data.\nmore", "renderer.rs", 1033);
        assert_eq!(
            note,
            "Grafik: Gerät verloren, Programm startet neu: Failed to create staging buffer for index data. \
             (renderer.rs:1033)"
        );
        assert!(!graphics_lost(), "nothing marks it before a device is lost");
    }

    #[test]
    fn the_trace_reads_like_a_call_site() {
        let trace = panic_trace("crates/x.rs", 12, "main", "kaputt\nzweite Zeile");
        assert_eq!(trace, "File \"crates/x.rs\", line 12, in main\n    kaputt");
    }

    #[test]
    fn nothing_is_recorded_before_init() {
        // The test process never calls `init`, so no log exists.
        assert!(!record("test", "Fehler", "x", None) || error_log().is_some());
        assert!(current().is_none() || error_log().is_some());
    }

    #[test]
    fn a_second_process_of_a_role_in_the_run_does_not_own_its_files() {
        use crate::paths::Host;
        let dir = crate::testdir::Dir::new("diagnose-rolle");
        let root = dir.path().to_string_lossy().into_owned();
        let appdata = dir.path().join("AppData").to_string_lossy().into_owned();
        let other = dir.path().join("andere").to_string_lossy().into_owned();
        let host = |home: &str| {
            let vars =
                [(crate::legacy::ENV_HOME, home), ("APPDATA", appdata.as_str()), ("XDG_CONFIG_HOME", appdata.as_str())];
            Host::new(crate::pypath::Flavor::native(), &vars, &root)
        };
        let paths = Paths::new(host(&root));
        let (first, lock) = claim(&paths, Role::Main);
        assert!(first && lock.is_some(), "the first main window owns the files");
        assert!(!claim(&paths, Role::Main).0, "a second main window of the run joins them");
        assert!(claim(&paths, Role::Overlay).0, "another role has its own");
        assert!(claim(&Paths::new(host(&other)), Role::Main).0, "another data folder is another run");
        drop(lock);
        assert!(claim(&paths, Role::Main).0, "after the first ended, the next owns them again");
        // A data folder named from outside keeps the lock inside it; the user's own data keep it beside the pointers.
        assert!(std::path::Path::new(&role_lock_file(&paths, Role::Main)).starts_with(&root));
        let own_vars = [("APPDATA", appdata.as_str()), ("USERPROFILE", root.as_str()), ("HOME", root.as_str())];
        let own = Paths::new(Host::new(crate::pypath::Flavor::native(), &own_vars, &root));
        let beside = own.host.flavor.dirname(&own.second_pointer());
        assert_eq!(own.host.flavor.dirname(&role_lock_file(&own, Role::Overlay)), beside);
    }

    #[test]
    fn every_role_has_files_of_its_own() {
        let mut names: Vec<&str> = Vec::new();
        for role in Role::ALL {
            let files = role.crash_files();
            names.extend([files.current, files.previous, files.version_current, files.version_previous]);
            names.push(role.trail_file());
        }
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two roles share a file: {names:?}");
        // The main window keeps the names of 4.12, which the report and older versions know.
        assert_eq!(Role::Main.crash_files().current, "absturz.txt");
        assert_eq!(Role::Main.trail_file(), "start-spur.txt");
        assert!(start_line("5.0.0").starts_with("Start, Version 5.0.0, "));
    }
}
