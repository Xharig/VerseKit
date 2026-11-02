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

use super::*;
use crate::testdir::Dir;
use crate::updater::checksum_of;

/// A running AppImage and a checked new one next to it.
fn setup(dir: &Dir) -> (PathBuf, PathBuf, String) {
    let target = dir.path().join("VerseKit-x86_64.AppImage");
    fs::write(&target, b"die alte Fassung").unwrap();
    let new_file = dir.path().join(".VerseKit-x86_64.AppImage.neu");
    fs::write(&new_file, b"die neue Fassung").unwrap();
    let checksum = checksum_of(&new_file).unwrap();
    (target, new_file, checksum)
}

#[test]
fn the_appimage_is_backed_up_exchanged_and_can_go_back() {
    let dir = Dir::new("tausch-gut");
    let (target, new_file, checksum) = setup(&dir);
    swap(&new_file, &checksum, &target).expect("exchanged");
    assert_eq!(fs::read(&target).unwrap(), b"die neue Fassung");
    assert_eq!(fs::read(backup_of(&target)).unwrap(), b"die alte Fassung");
    assert!(!new_file.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o755);
    }
    // The new version died at its start: the old one goes back.
    assert!(roll_back(&target));
    assert_eq!(fs::read(&target).unwrap(), b"die alte Fassung");
    assert!(!backup_of(&target).exists());
    assert!(!roll_back(&target), "nothing left to put back");
}

#[test]
fn a_file_changed_since_it_was_checked_is_never_put_in_place() {
    let dir = Dir::new("tausch-summe");
    let (target, new_file, checksum) = setup(&dir);
    fs::write(&new_file, b"untergeschoben").unwrap();
    assert_eq!(swap(&new_file, &checksum, &target), Err(SwapError::Unchecked));
    assert_eq!(fs::read(&target).unwrap(), b"die alte Fassung");
    assert!(!new_file.exists(), "the unchecked file goes");
    assert!(!backup_of(&target).exists());
    // Without any checksum neither.
    let (target, new_file, _) = setup(&Dir::new("tausch-ohne"));
    assert_eq!(swap(&new_file, "", &target), Err(SwapError::Unchecked));
}

#[test]
fn a_foreign_file_is_never_replaced() {
    let dir = Dir::new("tausch-fremd");
    let (_, new_file, checksum) = setup(&dir);
    let foreign = dir.path().join("Krita.AppImage");
    fs::write(&foreign, b"fremd").unwrap();
    assert_eq!(swap(&new_file, &checksum, &foreign), Err(SwapError::Foreign("Krita.AppImage".into())));
    assert_eq!(fs::read(&foreign).unwrap(), b"fremd");
}

#[test]
fn without_a_backup_there_is_no_exchange() {
    let dir = Dir::new("tausch-keine-sicherung");
    let (target, new_file, checksum) = setup(&dir);
    // A folder in the place of the backup: the copy cannot be written.
    fs::create_dir_all(backup_of(&target)).unwrap();
    assert_eq!(swap(&new_file, &checksum, &target), Err(SwapError::NoBackup));
    assert_eq!(fs::read(&target).unwrap(), b"die alte Fassung");
    assert!(!new_file.exists());
    assert!(!SwapError::NoBackup.message().starts_with("up_"));
}

/// Something started that ends after so many looks, with a code.
struct Ends(u32, i32);

impl Started for Ends {
    fn ended(&mut self) -> Option<i32> {
        if self.0 == 0 {
            return Some(self.1);
        }
        self.0 -= 1;
        None
    }
}

#[test]
fn the_old_version_steps_down_only_when_the_new_one_lives() {
    let poll = Duration::from_millis(5);
    assert_eq!(alive_after(&mut Ends(1, 127), Duration::from_secs(5), poll), Err(127), "died at once");
    assert_eq!(alive_after(&mut Ends(u32::MAX, 0), Duration::from_millis(40), poll), Ok(()), "lives on");
}

#[test]
fn the_new_version_is_told_the_one_process_it_replaces() {
    let env = vec![("PATH".to_string(), "/usr/bin".to_string()), (REPLACES_PID.to_string(), "11".to_string())];
    assert_eq!(
        replacing(env, 4711),
        [("PATH".to_string(), "/usr/bin".to_string()), (REPLACES_PID.to_string(), "4711".to_string())],
        "an older value goes"
    );
    assert_eq!(replacing(Vec::new(), 7), [(REPLACES_PID.to_string(), "7".to_string())]);
}

#[test]
fn a_real_program_that_ends_at_once_is_seen() {
    let dir = Dir::new("tausch-start");
    let output = fs::File::create(dir.path().join(RESTART_OUTPUT_FILE)).unwrap();
    let (program, code) = if cfg!(windows) { ("cmd", 3) } else { ("false", 1) };
    let mut env: Vec<(String, String)> = std::env::vars().collect();
    env.retain(|(k, _)| {
        k.eq_ignore_ascii_case("PATH") || k.eq_ignore_ascii_case("SystemRoot") || k.eq_ignore_ascii_case("ComSpec")
    });
    let mut command = std::process::Command::new(program);
    if cfg!(windows) {
        command.args(["/c", "exit", "3"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(crate::update_run::HELPER_FLAGS);
        }
    }
    let mut child = command.env_clear().envs(env).stderr(output).spawn().unwrap();
    assert_eq!(alive_after(&mut child, Duration::from_secs(10), Duration::from_millis(20)), Err(code));
}
