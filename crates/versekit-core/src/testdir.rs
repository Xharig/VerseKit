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

//! Names of the throwaway folders of the checks. The process number alone is
//! not enough: Windows hands numbers out again soon, and a folder an earlier
//! run left behind would then hold its files. The tag of a run adds the time
//! the run started, so a folder of this run is never one of an earlier run;
//! within the run it stays the same, as the process number did.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `<process number>-<start of the run>`, the same for the whole run.
pub fn run_tag() -> String {
    static TAG: OnceLock<String> = OnceLock::new();
    TAG.get_or_init(|| {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_micros());
        format!("{}-{:x}", std::process::id(), started % 0x1_0000_0000_0000)
    })
    .clone()
}

/// A new, empty folder `versekit-<name>-<run tag>-<number>` in the temporary
/// folder: a folder of its own for every call, emptied first. The caller
/// removes it; [`Dir`] does that on its own.
pub fn fresh(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("versekit-{name}-{}-{number}", run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A folder of [`fresh`] that is removed when the value goes.
pub struct Dir(PathBuf);

impl Dir {
    pub fn new(name: &str) -> Dir {
        Dir(fresh(name))
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_new_empty_and_goes_with_its_value() {
        assert_eq!(run_tag(), run_tag(), "the same within a run");
        // The second part is the start of the run: within the last hour, as the process number
        // alone would never tell an earlier run apart.
        let started =
            run_tag().strip_prefix(&format!("{}-", std::process::id())).map(str::to_string).expect("pid first");
        let started = u128::from_str_radix(&started, 16).expect("hex");
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() % 0x1_0000_0000_0000;
        assert!(now.wrapping_sub(started) % 0x1_0000_0000_0000 < 3_600_000_000, "{started} against {now}");
        let first = Dir::new("testordner");
        let second = Dir::new("testordner");
        assert_ne!(first.path(), second.path(), "a folder of its own for every call");
        std::fs::write(first.path().join("x"), "x").unwrap();
        let kept = first.path().to_path_buf();
        drop(first);
        assert!(!kept.exists(), "removed with its value");
        assert!(second.path().is_dir() && std::fs::read_dir(second.path()).unwrap().next().is_none());
    }
}
