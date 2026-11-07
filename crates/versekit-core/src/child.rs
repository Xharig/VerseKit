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

//! Starting a helper program without waiting for it (a sound, a message of the system, a browser).
//!
//! ⚠ On Linux a child that ends stays in the process table until its parent asks for its end: a program that
//! starts helpers and never asks collects them as zombies. The end is collected in a thread of its own, so the
//! caller does not wait.

use std::process::{Child, Command, Stdio};

/// Starts `command` with no input and no output and collects its end in a thread. True when it was started.
pub fn spawn_quiet(command: &mut Command) -> bool {
    match command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(child) => {
            reap(child);
            true
        }
        Err(_) => false,
    }
}

/// Collects the end of a child that runs on, in a thread of its own.
pub fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_is_not_there_is_not_started() {
        assert!(!spawn_quiet(&mut Command::new("versekit-gibt-es-nicht-7f3a")));
    }

    #[cfg(unix)]
    #[test]
    fn a_started_child_is_collected_and_leaves_no_zombie() {
        let child = Command::new("true").spawn().expect("true is there");
        let pid = child.id();
        reap(child);
        let state = || std::fs::read_to_string(format!("/proc/{pid}/stat")).ok();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state().is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(state().is_none(), "the process is gone from the table: {:?}", state());
    }
}
