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

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::testdir::Dir;
use crate::update_run::read_run;
use crate::updater::checksum_of;

#[test]
fn the_moment_of_a_release_is_read_as_github_writes_it() {
    assert_eq!(parse_moment("1970-01-01T00:00:00Z"), Some(0.0));
    assert_eq!(parse_moment("2026-11-15T18:30:05Z"), Some(1_794_767_405.0));
    assert_eq!(parse_moment("2000-03-01T00:00:00Z"), Some(951_868_800.0), "after a leap day");
    for bad in ["", "2026-11-15", "2026-11-15T18:30:05", "2026-13-01T00:00:00Z", "2026-11-15 18:30:05Z"] {
        assert_eq!(parse_moment(bad), None, "{bad}");
    }
}

fn fresh(time: &str) -> Release {
    Release { version: "v5.0.1".into(), time: time.into(), ..Release::default() }
}

#[test]
fn a_fresh_release_waits_two_minutes_and_never_comes_in_the_game() {
    let published = parse_moment("2026-11-15T18:00:00Z").unwrap();
    let r = fresh("2026-11-15T18:00:00Z");
    assert_eq!(wait_left(&r, published + 30.0), 90);
    assert_eq!(wait_left(&r, published + 119.5), 1, "rounded up");
    assert_eq!(wait_left(&r, published + 120.0), 0);
    assert_eq!(wait_left(&fresh(""), published), 0, "no moment: rather fetch than never");
    assert_eq!(next_step(None, published, false, false), Step::Nothing);
    assert_eq!(next_step(Some(&r), published + 30.0, false, false), Step::NotYet(90));
    assert_eq!(next_step(Some(&r), published + 300.0, true, false), Step::AfterGame);
    assert_eq!(next_step(Some(&r), published + 300.0, false, false), Step::Install);
    assert_eq!(next_step(Some(&r), published + 300.0, false, true), Step::Nothing, "failed a short while ago");
}

/// A throwaway data folder in the real layout, a temporary folder, and a fetched installer.
fn world(name: &str) -> (Dir, Paths, PathBuf, Downloaded) {
    let dir = Dir::new(name);
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let text = home.to_string_lossy().into_owned();
    let paths =
        Paths::new(Host::new(Flavor::native(), &[("HOME", &text), ("USERPROFILE", &text), ("APPDATA", &text)], &text));
    assert!(paths.app_folder().starts_with(&text), "{}", paths.app_folder());
    fs::write(paths.settings_file(), b"{}").unwrap();
    let temp = dir.path().join("temp");
    fs::create_dir_all(&temp).unwrap();
    let setup = temp.join("VerseKit-Setup.exe");
    fs::write(&setup, b"der Installer").unwrap();
    let downloaded = Downloaded { checksum: checksum_of(&setup).unwrap(), path: setup };
    (dir, paths, temp, downloaded)
}

fn request<'a>(
    paths: &'a Paths,
    downloaded: &'a Downloaded,
    program: &'a Path,
    packaging: Packaging,
    temp: &'a Path,
) -> Request<'a> {
    Request {
        paths,
        downloaded,
        program,
        packaging,
        own_version: "5.0.0",
        target_version: "v5.0.1",
        automatic: true,
        show_window: false,
        temp,
        env: vec![("PATH".into(), std::env::var("PATH").unwrap_or_default())],
    }
}

#[cfg(windows)]
#[test]
fn under_windows_a_backup_then_the_installer_itself_is_started_and_no_copy_of_the_program() {
    let (dir, paths, temp, old_place) = world("auto-windows");
    fs::remove_file(&old_place.path).unwrap();
    let folder = dir.path().join("Programme").join("Verse-Kit");
    fs::create_dir_all(&folder).unwrap();
    let system = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
    let program = folder.join("SC-BP-Watcher.exe");
    fs::copy(system.join(r"System32\PING.EXE"), &program).unwrap();
    // A copy of `where` stands in for the installer: on these arguments it ends at once, without a window.
    let fetched = dir.path().join("lokal").join("VerseKit").join("Update");
    fs::create_dir_all(&fetched).unwrap();
    let setup = fetched.join("VerseKit-Setup.exe");
    fs::copy(system.join(r"System32\where.exe"), &setup).unwrap();
    let downloaded = Downloaded { checksum: checksum_of(&setup).unwrap(), path: setup.clone() };
    let error = install(request(&paths, &downloaded, &program, Packaging::Exe, &temp)).expect_err("ended at once");
    assert!(error.contains("ended at once"), "{error}");
    let backups = fs::read_dir(paths.app_file(backup::BEFORE_UPDATE_FOLDER)).unwrap().count();
    assert_eq!(backups, 1, "a backup before the update");
    let log = fs::read_to_string(paths.app_file(update_run::LOG_FILE)).unwrap();
    assert!(log.contains("program starts the installer VerseKit-Setup.exe"), "the installer itself: {log}");
    assert!(log.contains("Installer ended before it ran"), "{log}");
    assert!(read_run(&paths).is_none(), "no run mark: the program runs on");
    let in_temp: Vec<_> = fs::read_dir(&temp).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(
        in_temp.iter().all(|name| !name.to_string_lossy().to_lowercase().ends_with(".exe")),
        "no program file in the temporary folder, no copy of the program: {in_temp:?}"
    );
}

#[test]
fn a_file_changed_since_it_was_fetched_or_a_foreign_program_stops_everything() {
    let (dir, paths, temp, downloaded) = world("auto-abgelehnt");
    let program = dir.path().join("SC-BP-Watcher.exe");
    fs::write(&program, b"x").unwrap();
    let foreign = dir.path().join("Fremd.exe");
    fs::write(&foreign, b"x").unwrap();
    assert!(install(request(&paths, &downloaded, &foreign, Packaging::Exe, &temp)).is_err());
    assert!(install(request(&paths, &downloaded, &program, Packaging::Source, &temp)).is_err());
    fs::write(&downloaded.path, b"ausgetauscht").unwrap();
    assert!(install(request(&paths, &downloaded, &program, Packaging::Exe, &temp)).is_err());
    assert!(!downloaded.path.exists(), "the changed file goes");
    assert!(read_run(&paths).is_none() && !temp.join(update_run::HELPER_NAME).exists(), "nothing handed over");
}

#[test]
fn under_linux_the_appimage_is_exchanged_after_the_backup() {
    let (dir, paths, _temp, _) = world("auto-linux");
    let appimage = dir.path().join("VerseKit-x86_64.AppImage");
    fs::write(&appimage, b"alt").unwrap();
    let new_file = dir.path().join(".VerseKit-x86_64.AppImage.neu");
    fs::write(&new_file, b"neu").unwrap();
    let downloaded = Downloaded { checksum: checksum_of(&new_file).unwrap(), path: new_file };
    let installed =
        install(request(&paths, &downloaded, &appimage, Packaging::AppImage, dir.path())).expect("exchanged");
    assert_eq!(installed, Installed::Swapped);
    assert_eq!(fs::read(&appimage).unwrap(), b"neu");
    assert!(update_swap::backup_of(&appimage).is_file());
}
