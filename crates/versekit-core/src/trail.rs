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

//! The start trail `start-spur.txt` (`errors.trail`): one line per start
//! step and per opened page, written at once, so after a crash the last line
//! says how far the program got.
//!
//! The file is created anew at every start. Up to [`BOUNDARY`] every line is
//! forced to disk; afterwards only flushed, because every page change writes
//! two lines. When the file grows past [`CAP`] lines it is cut back to
//! [`KEEP`] lines of operation, the start itself is never cut.
//!
//! Never fails the program: a failed write is remembered once
//! ([`Trail::write_error`]) and the report names it.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::sync::Mutex;

use crate::formats;
use crate::jsonio::{self, Newline};
use crate::paths::Paths;
use crate::text::{lines_universal, py_rstrip, py_strip};

pub const FILE: &str = "start-spur.txt";
/// The trail of the overlay and of the teaching window (`diagnostics::Role`).
pub const FILE_OVERLAY: &str = "start-spur-overlay.txt";
pub const FILE_LEARN: &str = "start-spur-anlernen.txt";
/// The last line of the start; everything after it is operation.
pub const BOUNDARY: &str = "Hauptschleife läuft";
/// After this many new lines the file is looked at and cut back.
pub const CAP: usize = 200;
/// So many lines of operation stay after cutting.
pub const KEEP: usize = 60;

#[derive(Default)]
struct State {
    /// Whether this run already wrote a line (then it appends).
    opened: bool,
    /// Whether the boundary was written (then no more forcing to disk).
    operating: bool,
    count: usize,
    write_error: String,
}

/// The start trail of one run.
#[derive(Default)]
pub struct Trail {
    state: Mutex<State>,
}

/// The trail of the running program.
pub static TRAIL: Trail = Trail::new();

/// Python's name for the kind of a failed file operation.
fn error_name(e: &io::Error) -> &'static str {
    match e.kind() {
        io::ErrorKind::PermissionDenied => "PermissionError",
        io::ErrorKind::NotFound => "FileNotFoundError",
        _ => "OSError",
    }
}

/// `HH:MM:SS` of the local time now.
fn clock() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0);
    formats::local_time(seconds).map(|t| formats::strftime("%H:%M:%S", &t)).unwrap_or_default()
}

/// The file as Python reads it in text mode: strict UTF-8, universal newlines.
fn read_lines(path: &str) -> Result<Vec<String>, io::Error> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let ends_open = !text.ends_with(['\n', '\r']);
    let parts = lines_universal(&text);
    let last = parts.len().saturating_sub(1);
    Ok(parts
        .iter()
        .enumerate()
        .map(|(i, line)| if i == last && ends_open { (*line).to_string() } else { format!("{line}\n") })
        .collect())
}

/// Index of the line that ends the start, or `None`: the start did not finish.
fn boundary_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| py_rstrip(line).ends_with(BOUNDARY))
}

/// The trail of the last run of the main window: start steps and operation, as written.
pub fn last_trail(paths: &Paths) -> Vec<String> {
    last_trail_of(paths, FILE)
}

/// [`last_trail`] of the trail `file`.
pub fn last_trail_of(paths: &Paths, file: &str) -> Vec<String> {
    let Ok(lines) = read_lines(&paths.app_file(file)) else { return Vec::new() };
    lines.iter().filter(|l| !py_strip(l).is_empty()).map(|l| py_rstrip(l).to_string()).collect()
}

/// The trail in two parts: the start steps up to and including the boundary,
/// and the page changes after it. Without a boundary everything is start.
///
/// The report caps both parts separately: five clicks must not push the start
/// out, the start being what the trail exists for.
pub fn split_trail(paths: &Paths) -> (Vec<String>, Vec<String>) {
    let all = last_trail(paths);
    match boundary_index(&all) {
        None => (all, Vec::new()),
        Some(at) => (all[..=at].to_vec(), all[at + 1..].to_vec()),
    }
}

impl Trail {
    pub const fn new() -> Trail {
        Trail { state: Mutex::new(State { opened: false, operating: false, count: 0, write_error: String::new() }) }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Writes one line with the time now.
    pub fn step(&self, paths: &Paths, step: &str) {
        self.step_at(paths, step, &clock());
    }

    /// [`step`](Self::step) with a given time (`HH:MM:SS`).
    pub fn step_at(&self, paths: &Paths, step: &str, time: &str) {
        self.step_at_in(paths, FILE, step, time);
    }

    /// [`step_at`](Self::step_at) into the trail `file`.
    pub fn step_at_in(&self, paths: &Paths, file: &str, step: &str, time: &str) {
        let mut state = self.state();
        let path = paths.app_file(file);
        let appending = state.opened;
        state.opened = true;
        let force = !state.operating;
        let line = jsonio::translate(&format!("{time}  {step}\n"), Newline::Native);
        if let Err(e) = write_line(&path, appending, force, &line) {
            if state.write_error.is_empty() {
                let mode = if appending { 'a' } else { 'w' };
                state.write_error = format!("{file} ({mode}): {}: {e}", error_name(&e));
            }
            return;
        }
        if step == BOUNDARY {
            state.operating = true;
        }
        state.count += 1;
        if state.count >= CAP {
            state.count = 0;
            if let Err(e) = trim(&path)
                && state.write_error.is_empty()
            {
                state.write_error = format!("{file} (a): UnicodeDecodeError: {e}");
            }
        }
    }

    /// From now on this run writes at the end of the file instead of creating it anew: a second process of the same
    /// role keeps the trail of the first.
    pub fn join(&self) {
        self.state().opened = true;
    }

    /// The first failed write of this run, or empty.
    pub fn write_error(&self) -> String {
        self.state().write_error.clone()
    }
}

/// Appends or (the first time) replaces the file with `line`; flushed at
/// once, forced to disk while the start is still running.
fn write_line(path: &str, appending: bool, force: bool, line: &str) -> io::Result<()> {
    let open = |append: bool| -> io::Result<fs::File> {
        crate::writeguard::check(if append { "open-a" } else { "open-w" }, path)?;
        if append { OpenOptions::new().append(true).create(true).open(path) } else { fs::File::create(path) }
    };
    let mut file = match open(appending) {
        Err(e) if !appending && jsonio::is_permission(&e) && jsonio::make_overwritable(path) => open(false)?,
        other => other?,
    };
    file.write_all(line.as_bytes())?;
    file.flush()?;
    if force {
        file.sync_all()?;
    }
    Ok(())
}

/// Cuts the operation part back to [`KEEP`] lines without touching the start.
///
/// ⚠ Cutting the front would be the obvious thing and wrong: the front is the
/// start. Without a finished start (no boundary) the file is cut at the end
/// instead, the cause is then at the beginning.
fn trim(path: &str) -> io::Result<()> {
    // Only text that is not UTF-8 is an error; a file that cannot be read is
    // left alone.
    let lines = match read_lines(path) {
        Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(e),
        Err(_) => return Ok(()),
        Ok(lines) => lines,
    };
    if lines.len() <= KEEP {
        return Ok(());
    }
    let (head, tail) = match boundary_index(&lines) {
        None => (&lines[..KEEP], &lines[..0]),
        Some(at) => (&lines[..=at], &lines[at + 1..]),
    };
    let kept = &tail[tail.len().saturating_sub(KEEP)..];
    let text: String = head.iter().chain(kept).map(String::as_str).collect();
    // A failed cut leaves the file as it was.
    let _ = jsonio::write_new(path, jsonio::translate(&text, Newline::Native).as_bytes());
    Ok(())
}

/// Writes one line of the trail of the running program, into the data and
/// the file of its role; nothing before `diagnostics::init_for` has run.
pub fn step(text: &str) {
    if let Some((paths, role)) = crate::diagnostics::current() {
        TRAIL.step_at_in(paths, role.trail_file(), text, &clock());
    }
}

/// A frame that held the window longer than this many seconds is written down.
pub const HANG_SECONDS: f64 = 2.0;
/// The name of the overview, the page without a key.
pub const OVERVIEW: &str = "uebersicht";

/// The lines a window writes for its pages, as 4.12 does: `Seite <id>: bauen
/// beginnt` at the first visit, `Seite <id>: zeigen` at every later one, and
/// `Seite <id>: steht (N ms)` once the page is drawn. The last line without
/// `steht` is the page the program got stuck on. A frame that held the window
/// longer than [`HANG_SECONDS`] gives `Fenster stand N s`.
#[derive(Default)]
pub struct PageTrail {
    seen: std::collections::HashSet<String>,
    open: Option<String>,
    /// The open page was entered in this frame and is not drawn yet.
    entering: bool,
}

impl PageTrail {
    pub fn new() -> PageTrail {
        PageTrail::default()
    }

    /// Before a frame draws `page`: the line for a page entered, or `None`
    /// while the same page stays open.
    pub fn before(&mut self, page: &str) -> Option<String> {
        let id = if page.is_empty() { OVERVIEW } else { page };
        if self.open.as_deref() == Some(id) {
            return None;
        }
        self.open = Some(id.to_string());
        self.entering = true;
        let first = self.seen.insert(id.to_string());
        Some(if first { format!("Seite {id}: bauen beginnt") } else { format!("Seite {id}: zeigen") })
    }

    /// After the frame that took `frame`: the line for the page entered in
    /// it, and the line for a frame that held the window.
    pub fn after(&mut self, frame: std::time::Duration) -> Vec<String> {
        let mut out = Vec::new();
        if std::mem::take(&mut self.entering)
            && let Some(id) = &self.open
        {
            out.push(format!("Seite {id}: steht ({:.0} ms)", frame.as_secs_f64() * 1000.0));
        }
        if frame.as_secs_f64() > HANG_SECONDS {
            out.push(format!("Fenster stand {:.1} s", frame.as_secs_f64()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-spur-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        let value = dir.to_string_lossy().to_string();
        let home = [("SC_BP_HOME", value.as_str()), ("HOME", "/home/spieler"), ("USERPROFILE", "/home/spieler")];
        (Paths::new(Host::new(Flavor::native(), &home, "/")), dir)
    }

    #[test]
    fn first_step_replaces_and_the_rest_appends() {
        let (paths, dir) = paths_in("neu");
        fs::write(dir.join(FILE), "alter Lauf\n").unwrap();
        let trail = Trail::new();
        trail.step_at(&paths, "Start", "10:00:00");
        trail.step_at(&paths, "Fenster", "10:00:01");
        assert_eq!(last_trail(&paths), ["10:00:00  Start", "10:00:01  Fenster"]);
        assert_eq!(trail.write_error(), "");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_joining_run_keeps_the_trail_of_the_running_one() {
        let (paths, dir) = paths_in("weiteres");
        let running = Trail::new();
        running.step_at(&paths, "Start", "10:00:00");
        let second = Trail::new();
        second.join();
        second.step_at(&paths, "Start, weiteres Fenster", "10:05:00");
        running.step_at(&paths, "Seite liste", "10:06:00");
        assert_eq!(
            last_trail(&paths),
            ["10:00:00  Start", "10:05:00  Start, weiteres Fenster", "10:06:00  Seite liste"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_at_the_boundary() {
        let (paths, dir) = paths_in("grenze");
        let trail = Trail::new();
        for (i, step) in ["Start", BOUNDARY, "Seite liste", "Seite hangar"].iter().enumerate() {
            trail.step_at(&paths, step, &format!("10:00:0{i}"));
        }
        let (start, pages) = split_trail(&paths);
        assert_eq!(start.len(), 2);
        assert!(start[1].ends_with(BOUNDARY));
        assert_eq!(pages, ["10:00:02  Seite liste", "10:00:03  Seite hangar"]);
        // Without a finished start everything is start.
        let (paths2, dir2) = paths_in("ohne-grenze");
        let trail2 = Trail::new();
        trail2.step_at(&paths2, "Start", "10:00:00");
        assert_eq!(split_trail(&paths2), (vec!["10:00:00  Start".to_string()], Vec::new()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn cutting_keeps_the_start_and_the_last_sixty() {
        let (paths, dir) = paths_in("kuerzen");
        let trail = Trail::new();
        trail.step_at(&paths, "Start 1", "09:00:00");
        trail.step_at(&paths, "Start 2", "09:00:01");
        trail.step_at(&paths, BOUNDARY, "09:00:02");
        // The 200th line written is the one that triggers the cut.
        for i in 0..CAP - 3 {
            trail.step_at(&paths, &format!("Seite {i}"), "09:01:00");
        }
        let (start, pages) = split_trail(&paths);
        assert_eq!(start.len(), 3, "the start is never cut");
        assert_eq!(pages.len(), KEEP);
        assert!(pages[KEEP - 1].ends_with(&format!("Seite {}", CAP - 4)), "{:?}", pages.last());
        assert!(pages[0].ends_with(&format!("Seite {}", CAP - 3 - KEEP)), "{:?}", pages.first());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cutting_an_unfinished_start_keeps_the_front() {
        let (paths, dir) = paths_in("unfertig");
        let trail = Trail::new();
        for i in 0..CAP {
            trail.step_at(&paths, &format!("Schritt {i}"), "09:00:00");
        }
        let all = last_trail(&paths);
        assert_eq!(all.len(), KEEP);
        assert!(all[0].ends_with("Schritt 0"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_trail_file_of_another_role_stays_apart() {
        let (paths, dir) = paths_in("rolle");
        let main = Trail::new();
        main.step_at(&paths, "Hauptfenster", "10:00:00");
        let overlay = Trail::new();
        overlay.step_at_in(&paths, FILE_OVERLAY, "Overlay", "10:00:01");
        assert_eq!(last_trail(&paths), ["10:00:00  Hauptfenster"]);
        assert_eq!(last_trail_of(&paths, FILE_OVERLAY), ["10:00:01  Overlay"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pages_are_written_as_entered_drawn_and_held() {
        use std::time::Duration;
        let mut pages = PageTrail::new();
        assert_eq!(pages.before("").as_deref(), Some("Seite uebersicht: bauen beginnt"));
        assert_eq!(pages.after(Duration::from_millis(12)), ["Seite uebersicht: steht (12 ms)"]);
        // The same page in the next frames writes nothing.
        assert_eq!(pages.before(""), None);
        assert!(pages.after(Duration::from_millis(5)).is_empty());
        assert_eq!(pages.before("hf_liste").as_deref(), Some("Seite hf_liste: bauen beginnt"));
        assert_eq!(pages.after(Duration::from_millis(340)), ["Seite hf_liste: steht (340 ms)"]);
        // A page seen before is shown, not built.
        assert_eq!(pages.before("").as_deref(), Some("Seite uebersicht: zeigen"));
        assert_eq!(pages.after(Duration::from_millis(3)), ["Seite uebersicht: steht (3 ms)"]);
        // A frame that held the window says so, also without a page change.
        assert_eq!(pages.before(""), None);
        assert_eq!(pages.after(Duration::from_millis(2500)), ["Fenster stand 2.5 s"]);
        assert!(pages.after(Duration::from_millis(2000)).is_empty(), "two seconds are not yet a hang");
    }

    #[test]
    fn a_failed_write_is_remembered_once() {
        let (paths, dir) = paths_in("fehler");
        // A folder where the file should be: every write fails.
        fs::create_dir_all(dir.join(FILE)).unwrap();
        let trail = Trail::new();
        trail.step_at(&paths, "Start", "10:00:00");
        trail.step_at(&paths, "Zwei", "10:00:01");
        let error = trail.write_error();
        assert!(error.starts_with("start-spur.txt (w): "), "{error}");
        let _ = fs::remove_dir_all(&dir);
    }
}
