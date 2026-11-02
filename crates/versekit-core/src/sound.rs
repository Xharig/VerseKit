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

//! The signal sound outside Windows (`sound.py` of 4.12).
//!
//! ⚠ The bell of X11 is switched off on nearly every desktop and does not exist under Wayland: a sound of the
//! system is played by a player that is there, in this order:
//!
//! 1. `canberra-gtk-play`: knows the sound theme of the desktop, so the sound is the one of the other programs;
//! 2. `paplay` (PulseAudio) and 3. `pw-play` (PipeWire): play the file of the freedesktop set.
//!
//! `aplay` is not in the list: it cannot play Ogg and would fail without a sound. The player is looked for once,
//! not at every sound. Nothing waits for the sound: the line in the overlay comes first. Windows has its own
//! sound in the overlay; here nothing plays there.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The folder of the freedesktop sounds.
pub const SOUND_DIR: &str = "/usr/share/sounds/freedesktop/stereo";

/// The players, in the order they are taken.
pub const PLAYERS: [&str; 3] = ["canberra-gtk-play", "paplay", "pw-play"];

/// The sound of an occasion: its name in the sound theme and its file in [`SOUND_DIR`]. The loud one is for a
/// hit of the watch list.
pub fn sound_of(loud: bool) -> (&'static str, &'static str) {
    if loud { ("complete", "complete.oga") } else { ("message", "message.oga") }
}

/// The first player of [`PLAYERS`] that `there` finds.
pub fn choose(there: impl Fn(&str) -> bool) -> Option<&'static str> {
    PLAYERS.into_iter().find(|player| there(player))
}

/// The command that plays the sound with `player`: the name in the theme for `canberra-gtk-play`, the file for
/// the others; `None` when that file is not there.
pub fn command(player: &str, loud: bool, file_there: impl Fn(&Path) -> bool) -> Option<Vec<String>> {
    let (name, file) = sound_of(loud);
    if player == PLAYERS[0] {
        return Some(vec![player.to_string(), "-i".to_string(), name.to_string()]);
    }
    let path = PathBuf::from(SOUND_DIR).join(file);
    file_there(&path).then(|| vec![player.to_string(), path.to_string_lossy().into_owned()])
}

/// The player of this system, looked for once.
fn player() -> Option<&'static str> {
    static PLAYER: OnceLock<Option<&'static str>> = OnceLock::new();
    *PLAYER.get_or_init(|| {
        if cfg!(windows) {
            return None;
        }
        let path = std::env::var("PATH").unwrap_or_default();
        choose(|name| crate::game_start::on_path(name, &path))
    })
}

/// Plays the signal sound, the loud one for a hit of the watch list, without waiting for it. Without a player
/// nothing happens.
pub fn play(loud: bool) {
    let Some(player) = player() else { return };
    let Some(command) = command(player, loud, Path::is_file) else { return };
    crate::child::spawn_quiet(Command::new(&command[0]).args(&command[1..]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_is_the_first_one_there_in_the_order_of_4_12() {
        assert_eq!(choose(|_| true), Some("canberra-gtk-play"), "the theme of the desktop first");
        assert_eq!(choose(|p| p != "canberra-gtk-play"), Some("paplay"));
        assert_eq!(choose(|p| p == "pw-play"), Some("pw-play"), "PipeWire alone is enough");
        assert_eq!(choose(|p| p == "aplay"), None, "aplay cannot play Ogg and is never taken");
        assert_eq!(choose(|_| false), None);
    }

    #[test]
    fn canberra_plays_the_name_of_the_theme_and_the_others_the_freedesktop_file() {
        let there = |_: &Path| true;
        assert_eq!(command("canberra-gtk-play", false, there).unwrap(), ["canberra-gtk-play", "-i", "message"]);
        assert_eq!(command("canberra-gtk-play", true, there).unwrap(), ["canberra-gtk-play", "-i", "complete"]);
        let file = |name: &str| PathBuf::from(SOUND_DIR).join(name).to_string_lossy().into_owned();
        assert_eq!(command("paplay", false, there).unwrap(), ["paplay".to_string(), file("message.oga")]);
        assert_eq!(command("pw-play", true, there).unwrap(), ["pw-play".to_string(), file("complete.oga")]);
    }

    #[test]
    fn without_the_file_a_player_of_files_plays_nothing_and_canberra_still_asks_the_theme() {
        let gone = |_: &Path| false;
        assert_eq!(command("paplay", false, gone), None);
        assert_eq!(command("pw-play", true, gone), None);
        assert!(command("canberra-gtk-play", false, gone).is_some());
    }

    #[test]
    fn the_loud_sound_is_another_one_than_the_normal_one() {
        assert_ne!(sound_of(true), sound_of(false));
    }
}
