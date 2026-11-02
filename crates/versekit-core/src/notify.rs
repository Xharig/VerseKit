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

//! Messages of the system outside Windows (`notify.py`): `notify-send`, if
//! the system has it. Nothing else: no library, no helper program under
//! Windows. There, the message is a balloon at the icon next to the clock
//! (`versekit-overlay`, `tray`). macOS has neither, as in 4.12.

use std::process::Command;

/// The program that shows the message.
pub const SENDER: &str = "notify-send";

/// The arguments for it: the name of the program, the title, the text.
pub fn arguments(title: &str, text: &str) -> Vec<String> {
    vec!["-a".into(), "Verse-Kit".into(), title.into(), text.into()]
}

/// Can this system show a message this way? Not on Windows (the balloon at the icon does it there), elsewhere
/// when [`SENDER`] is in `path`.
pub fn available_in(windows: bool, path: &str) -> bool {
    !windows && crate::game_start::on_path(SENDER, path)
}

/// [`available_in`] for this machine.
pub fn available() -> bool {
    available_in(cfg!(windows), &std::env::var("PATH").unwrap_or_default())
}

/// Sends a message of the system. True if the program was started; its end is collected, so no zombie stays.
pub fn show(title: &str, text: &str) -> bool {
    if cfg!(windows) {
        return false;
    }
    // Shown on the screen: streamer mode covers what is private.
    let arguments = arguments(&crate::streamer::scrub(title), &crate::streamer::scrub(text));
    crate::child::spawn_quiet(Command::new(SENDER).args(arguments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::Dir;

    #[test]
    fn the_message_goes_out_under_the_name_of_the_program() {
        assert_eq!(arguments("Titel", "Text"), ["-a", "Verse-Kit", "Titel", "Text"]);
    }

    #[test]
    fn the_message_is_offered_where_notify_send_is_on_the_path_and_never_on_windows() {
        let dir = Dir::new("notify-pfad");
        let path = dir.path().to_string_lossy().into_owned();
        assert!(!available_in(false, &path), "an empty folder has no sender");
        let sender = dir.path().join(SENDER);
        std::fs::write(&sender, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sender, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(available_in(false, &path), "found on the path");
        assert!(!available_in(true, &path), "Windows shows its balloon instead");
    }
}
