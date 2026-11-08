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

use crate::paths::Paths;

/// The setting: the volume of every sound, in percent.
pub const VOLUME_SETTING: &str = "lautstaerke";
/// The volume without a setting.
pub const VOLUME_DEFAULT: i64 = 70;
/// The setting: a sound when a refinery job is done.
pub const REFINERY_SETTING: &str = "ton_raffinerie";

/// The volume of every sound in percent, 0 to 100.
pub fn volume(paths: &Paths) -> i64 {
    paths.setting_int(VOLUME_SETTING, VOLUME_DEFAULT, Some(0), Some(100))
}

/// Is there a sound when a refinery job is done? On without a setting.
pub fn refinery_on(paths: &Paths) -> bool {
    paths.setting_bool(REFINERY_SETTING, true)
}

/// The loudness of `percent` as a factor of the amplitude: the ear hears a doubling of the amplitude as a small
/// step, so the slider goes by the square (50 % is a quarter of the amplitude).
pub fn gain(percent: i64) -> f64 {
    let share = percent.clamp(0, 100) as f64 / 100.0;
    share * share
}

/// The sample rate of [`tone`].
const RATE: u32 = 22_050;

/// The signal sound as a WAV file in memory (16 bit, mono): one short note, the loud one for a hit of the watch
/// list is two rising notes. `percent` is the volume; at 0 the sound has no sample but silence.
pub fn tone(loud: bool, percent: i64) -> Vec<u8> {
    let notes: &[(f64, f64)] = if loud { &[(988.0, 0.14), (1319.0, 0.22)] } else { &[(880.0, 0.18)] };
    let amplitude = gain(percent) * 0.6 * f64::from(i16::MAX);
    let mut samples: Vec<i16> = Vec::new();
    for &(hertz, seconds) in notes {
        let count = (seconds * f64::from(RATE)) as usize;
        for n in 0..count {
            let t = n as f64 / f64::from(RATE);
            // A short rise and a longer fall, so the note does not click.
            let edge = (n as f64 / 220.0).min(1.0).min((count - n) as f64 / 1100.0);
            samples.push((amplitude * edge * (std::f64::consts::TAU * hertz * t).sin()) as i16);
        }
    }
    let bytes = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

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

/// The volume as the player takes it: `canberra-gtk-play` in decibels, `paplay` from 0 to 65536 and `pw-play` from
/// 0 to 1. At 100 % no argument is given, the player plays as loud as it does for any other program.
fn volume_argument(player: &str, percent: i64) -> Option<String> {
    if percent >= 100 {
        return None;
    }
    let gain = gain(percent);
    Some(match player {
        "canberra-gtk-play" => format!("--volume={:.1}", 20.0 * gain.max(0.001).log10()),
        "paplay" => format!("--volume={}", (gain * 65_536.0).round() as u32),
        _ => format!("--volume={gain:.3}"),
    })
}

/// The command that plays the sound with `player` at `percent` volume: the name in the theme for
/// `canberra-gtk-play`, the file for the others; `None` when that file is not there.
pub fn command(player: &str, loud: bool, percent: i64, file_there: impl Fn(&Path) -> bool) -> Option<Vec<String>> {
    let (name, file) = sound_of(loud);
    let volume = volume_argument(player, percent);
    if player == PLAYERS[0] {
        let mut command = vec![player.to_string(), "-i".to_string(), name.to_string()];
        command.extend(volume);
        return Some(command);
    }
    let path = PathBuf::from(SOUND_DIR).join(file);
    file_there(&path).then(|| {
        let mut command = vec![player.to_string()];
        command.extend(volume);
        command.push(path.to_string_lossy().into_owned());
        command
    })
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

/// Plays the signal sound at `percent` volume, the loud one for a hit of the watch list, without waiting for it.
/// Without a player, or at 0 %, nothing happens.
pub fn play(loud: bool, percent: i64) {
    if percent <= 0 {
        return;
    }
    let Some(player) = player() else { return };
    let Some(command) = command(player, loud, percent, Path::is_file) else { return };
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
        assert_eq!(command("canberra-gtk-play", false, 100, there).unwrap(), ["canberra-gtk-play", "-i", "message"]);
        assert_eq!(command("canberra-gtk-play", true, 100, there).unwrap(), ["canberra-gtk-play", "-i", "complete"]);
        let file = |name: &str| PathBuf::from(SOUND_DIR).join(name).to_string_lossy().into_owned();
        assert_eq!(command("paplay", false, 100, there).unwrap(), ["paplay".to_string(), file("message.oga")]);
        assert_eq!(command("pw-play", true, 100, there).unwrap(), ["pw-play".to_string(), file("complete.oga")]);
    }

    #[test]
    fn every_player_gets_the_volume_in_its_own_unit() {
        let there = |_: &Path| true;
        // 50 %: a quarter of the amplitude, 12 dB less.
        let quiet = command("canberra-gtk-play", false, 50, there).unwrap();
        assert_eq!(quiet, ["canberra-gtk-play", "-i", "message", "--volume=-12.0"]);
        let file = |name: &str| PathBuf::from(SOUND_DIR).join(name).to_string_lossy().into_owned();
        assert_eq!(
            command("paplay", false, 50, there).unwrap(),
            ["paplay".to_string(), "--volume=16384".into(), file("message.oga")]
        );
        assert_eq!(
            command("pw-play", true, 50, there).unwrap(),
            ["pw-play".to_string(), "--volume=0.250".into(), file("complete.oga")]
        );
        // The slider at the end plays the sound as loud as every program does.
        assert!(!command("paplay", false, 100, there).unwrap().iter().any(|a| a.starts_with("--volume")));
    }

    #[test]
    fn the_volume_goes_by_the_square_and_the_setting_is_kept_between_0_and_100() {
        assert_eq!((gain(0), gain(100), gain(50)), (0.0, 1.0, 0.25));
        assert_eq!((gain(-5), gain(300)), (0.0, 1.0));
        let root = std::env::temp_dir().join(format!("versekit-ton-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let home = root.to_string_lossy().into_owned();
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, home.as_str())],
            &home,
        ));
        assert_eq!((volume(&paths), refinery_on(&paths)), (VOLUME_DEFAULT, true));
        paths.set_setting(VOLUME_SETTING, crate::jsonio::Value::int(40));
        paths.set_setting(REFINERY_SETTING, crate::jsonio::Value::Bool(false));
        assert_eq!((volume(&paths), refinery_on(&paths)), (40, false));
        paths.set_setting(VOLUME_SETTING, crate::jsonio::Value::int(900));
        assert_eq!(volume(&paths), 100);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The largest sample of the WAV file made by [`tone`].
    fn loudest(wav: &[u8]) -> i32 {
        wav[44..].as_chunks::<2>().0.iter().map(|b| i32::from(i16::from_le_bytes(*b)).abs()).max().unwrap_or(0)
    }

    #[test]
    fn the_built_in_tone_is_a_wav_file_that_gets_quieter_with_the_slider() {
        let normal = tone(false, 100);
        assert_eq!(&normal[..4], b"RIFF");
        assert_eq!(&normal[8..16], b"WAVEfmt ");
        let data = u32::from_le_bytes(normal[40..44].try_into().unwrap()) as usize;
        assert_eq!(normal.len(), 44 + data);
        assert_eq!(u32::from_le_bytes(normal[4..8].try_into().unwrap()) as usize, normal.len() - 8);
        assert_eq!(u32::from_le_bytes(normal[24..28].try_into().unwrap()), RATE);
        let (full, half, low) = (loudest(&normal), loudest(&tone(false, 50)), loudest(&tone(false, 20)));
        assert!(full > 15_000 && full < 21_000, "{full}");
        assert!(half < full / 3 && low < half / 3, "{full} {half} {low}");
        assert_eq!(loudest(&tone(false, 0)), 0, "at 0 % the file is silence");
        // The loud one is longer: two notes.
        assert!(tone(true, 100).len() > normal.len());
    }

    #[test]
    fn without_the_file_a_player_of_files_plays_nothing_and_canberra_still_asks_the_theme() {
        let gone = |_: &Path| false;
        assert_eq!(command("paplay", false, 70, gone), None);
        assert_eq!(command("pw-play", true, 70, gone), None);
        assert!(command("canberra-gtk-play", false, 70, gone).is_some());
    }

    #[test]
    fn the_loud_sound_is_another_one_than_the_normal_one() {
        assert_ne!(sound_of(true), sound_of(false));
    }
}
