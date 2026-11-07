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

//! A lock for every write access of `jsonio` and `paths`.
//!
//! Test runs that read real data switch it on. Every place that creates,
//! writes, renames or deletes asks `check` first; with the lock switched
//! on, it fails as if permissions were missing (`PermissionDenied`), and
//! the attempt is recorded.
//!
//! The lock applies only to the thread that switches it on, so unit tests
//! running at the same time do not interfere with each other.

use std::cell::{Cell, RefCell};
use std::io;

use crate::pypath::Flavor;

thread_local! {
    static DENY: Cell<bool> = const { Cell::new(false) };
    static ALLOWED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DENIED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Block writing in this thread (`true`) or allow it again.
pub fn deny(on: bool) {
    DENY.with(|d| d.set(on));
}

fn normal(folder: &str) -> String {
    let f = Flavor::native();
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    f.normcase(&f.abspath(folder, &cwd))
}

/// Allowed despite the lock: everything in this folder (a throwaway folder).
/// Replaces every folder allowed before; `None` allows none.
pub fn allow_under(folder: Option<&str>) {
    ALLOWED.with(|a| *a.borrow_mut() = folder.map(normal).into_iter().collect());
}

/// Allowed despite the lock, besides the folders allowed before.
pub fn allow_also(folder: &str) {
    ALLOWED.with(|a| a.borrow_mut().push(normal(folder)));
}

fn allowed(path: &str) -> bool {
    let f = Flavor::native();
    ALLOWED.with(|a| {
        let folders = a.borrow();
        if folders.is_empty() {
            return false;
        }
        let path = normal(path);
        folders.iter().any(|folder| path == *folder || path.starts_with(&format!("{folder}{}", f.sep())))
    })
}

/// The lock of one thread, to carry over into another one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    deny: bool,
    allowed: Vec<String>,
}

/// The lock as this thread has it.
pub fn state() -> State {
    State { deny: DENY.with(Cell::get), allowed: ALLOWED.with(|a| a.borrow().clone()) }
}

/// Take over a lock from another thread (for a thread started from it).
pub fn adopt(state: &State) {
    DENY.with(|d| d.set(state.deny));
    ALLOWED.with(|a| *a.borrow_mut() = state.allowed.clone());
}

/// Whether a write to `path` would be refused in this thread; asks only, records nothing.
pub fn would_refuse(path: &str) -> bool {
    DENY.with(Cell::get) && !allowed(path)
}

/// Before every write access: error if locked.
pub fn check(op: &str, path: &str) -> io::Result<()> {
    if !DENY.with(Cell::get) || allowed(path) {
        return Ok(());
    }
    DENIED.with(|list| list.borrow_mut().push(format!("{op} {path}")));
    Err(io::Error::new(io::ErrorKind::PermissionDenied, Locked))
}

/// The inner error of a refused write. Its text is the one the player reads, in the language of the moment
/// it is shown (`schutz_testkopie`), never the name of the step that was refused.
#[derive(Debug)]
pub struct Locked;

/// The text key of a refused write.
pub const LOCKED_KEY: &str = "schutz_testkopie";

impl std::fmt::Display for Locked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::language::t(LOCKED_KEY))
    }
}

impl std::error::Error for Locked {}

/// Whether an error is a write this lock refused (not a missing permission).
pub fn refused(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied && e.get_ref().is_some_and(|inner| inner.is::<Locked>())
}

/// All rejected attempts of this thread since the last call.
pub fn take_denied() -> Vec<String> {
    DENIED.with(|list| std::mem::take(&mut *list.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parts: &[&str]) -> String {
        let mut p = std::env::temp_dir().join("wg-folders");
        for part in parts {
            p.push(part);
        }
        p.to_string_lossy().into_owned()
    }

    fn reset() {
        deny(false);
        allow_under(None);
        take_denied();
    }

    #[test]
    fn two_folders_can_be_allowed_and_nothing_else() {
        reset();
        deny(true);
        allow_under(Some(&path(&["eins"])));
        allow_also(&path(&["zwei"]));
        assert!(check("write", &path(&["eins", "a.txt"])).is_ok());
        assert!(check("write", &path(&["zwei", "tief", "b.txt"])).is_ok());
        assert!(check("write", &path(&["eins"])).is_ok(), "the folder itself");
        // Counter-check: a third folder, and a folder that only starts with the same letters.
        assert!(check("write", &path(&["drei", "c.txt"])).is_err());
        assert!(check("write", &path(&["eins-daneben", "c.txt"])).is_err());
        assert_eq!(take_denied().len(), 2);
        reset();
    }

    #[test]
    fn a_refused_write_reads_as_a_sentence_in_both_languages_without_the_name_of_the_step() {
        use crate::language::{self, Lang};
        reset();
        deny(true);
        let error = check("makedirs", &path(&["vier", "a"])).expect_err("locked");
        assert!(refused(&error));
        for lang in [Lang::De, Lang::En] {
            let text = language::with_language(lang, || error.to_string());
            assert_eq!(text, language::t_in(lang, LOCKED_KEY), "{lang:?}");
            assert!(text.contains("VerseKit") && !text.contains("makedirs") && !text.contains("gesperrt:"), "{text}");
        }
        // Counter-check: an error with the same kind that this lock did not make is no refused write.
        assert!(!refused(&io::Error::new(io::ErrorKind::PermissionDenied, "Zugriff verweigert")));
        reset();
    }

    #[test]
    fn allowing_under_replaces_every_folder_allowed_before() {
        reset();
        deny(true);
        allow_under(Some(&path(&["eins"])));
        allow_also(&path(&["zwei"]));
        allow_under(Some(&path(&["drei"])));
        assert!(check("write", &path(&["zwei", "a.txt"])).is_err());
        assert!(check("write", &path(&["drei", "a.txt"])).is_ok());
        allow_under(None);
        assert!(check("write", &path(&["drei", "a.txt"])).is_err());
        reset();
    }

    #[test]
    fn a_thread_that_adopts_the_lock_keeps_both_folders() {
        reset();
        deny(true);
        allow_under(Some(&path(&["eins"])));
        allow_also(&path(&["zwei"]));
        let state = state();
        let (first, second, third) = std::thread::spawn(move || {
            adopt(&state);
            (
                check("write", &path(&["eins", "a"])).is_ok(),
                check("write", &path(&["zwei", "a"])).is_ok(),
                check("write", &path(&["drei", "a"])).is_ok(),
            )
        })
        .join()
        .expect("thread");
        assert!(first && second && !third);
        reset();
    }
}
