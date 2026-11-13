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

const DAY: f64 = 86_400.0;

/// The moment the offer is first seen is kept across looks and processes; a still newer version keeps it, the update
/// (another running version) and the end of the offer start again; a moment in the future starts again at once.
#[test]
fn the_wait_counts_from_the_first_offer_and_ends_with_the_update() {
    let (_dir, paths, _temp, _downloaded) = world("auto-wartet-seit");
    let start = parse_moment("2026-11-10T08:00:00Z").unwrap();
    assert_eq!(offered_since(&paths, "5.2.4", false, start), None, "nothing offered");
    assert_eq!(offered_since(&paths, "5.2.4", true, start), Some(start));
    // Later looks, another process (a fresh Paths on the same files), a still newer version: the first moment stays.
    assert_eq!(offered_since(&paths, "5.2.4", true, start + 3.0 * DAY), Some(start));
    let other = Paths::new(paths.host.clone());
    assert_eq!(offered_since(&other, "v5.2.4", true, start + 5.0 * DAY), Some(start), "same version with its v");
    assert_eq!(days_waiting(Some(start), start + 5.0 * DAY + 10.0), Some(5));
    // Updated: the new version counts from its own first offer.
    assert_eq!(offered_since(&paths, "5.3.0", true, start + 6.0 * DAY), Some(start + 6.0 * DAY));
    // Nothing newer any more: the moment goes, and a later offer starts again.
    assert_eq!(offered_since(&paths, "5.3.0", false, start + 7.0 * DAY), None);
    assert_eq!(paths.setting(OFFERED_SETTING), None, "the setting is emptied");
    assert_eq!(offered_since(&paths, "5.3.0", true, start + 8.0 * DAY), Some(start + 8.0 * DAY));
    // A clock set back: a moment in the future never holds the count.
    assert_eq!(offered_since(&paths, "5.3.0", true, start + 2.0 * DAY), Some(start + 2.0 * DAY));
    // A broken value starts again.
    paths.set_setting(OFFERED_SETTING, crate::jsonio::Value::str("5.3.0|gestern"));
    assert_eq!(offered_since(&paths, "5.3.0", true, start + 9.0 * DAY), Some(start + 9.0 * DAY));
}

#[test]
fn the_wait_shows_from_one_whole_day() {
    let since = parse_moment("2026-11-10T08:00:00Z").unwrap();
    assert_eq!(days_waiting(None, since + 9.0 * DAY), None, "nothing waits");
    assert_eq!(days_waiting(Some(since), since), None);
    assert_eq!(days_waiting(Some(since), since + DAY - 1.0), None, "not yet a whole day");
    assert_eq!(days_waiting(Some(since), since + DAY), Some(1));
    assert_eq!(days_waiting(Some(since), since + 2.0 * DAY - 1.0), Some(1));
    assert_eq!(days_waiting(Some(since), since + 30.0 * DAY), Some(30));
    assert_eq!(days_waiting(Some(since), since - DAY), None, "a moment in the future shows nothing");
}

/// The end of the program installs only the installer of Windows, only while the game is closed and nothing else
/// holds the update back; the end of the Windows session wins over everything.
#[test]
fn the_end_of_the_program_installs_only_the_installer_and_never_in_the_game_or_at_the_end_of_windows() {
    let ready = ExitFacts { offered: true, packaging: Some(Packaging::Exe), ..ExitFacts::default() };
    assert_eq!(at_exit(ready), AtExit::Install);
    assert_eq!(at_exit(ExitFacts { offered: false, ..ready }), AtExit::Nothing, "nothing on offer");
    for packaging in [Packaging::AppImage, Packaging::Source] {
        assert_eq!(at_exit(ExitFacts { packaging: Some(packaging), ..ready }), AtExit::Nothing, "{packaging:?}");
    }
    assert_eq!(at_exit(ExitFacts { packaging: None, ..ready }), AtExit::Nothing, "packaging unknown");
    assert_eq!(at_exit(ExitFacts { game_running: true, ..ready }), AtExit::GameRuns);
    assert_eq!(at_exit(ExitFacts { failed_recently: true, ..ready }), AtExit::Nothing);
    assert_eq!(at_exit(ExitFacts { download_paused: true, ..ready }), AtExit::Nothing);
    assert_eq!(at_exit(ExitFacts { locked: true, ..ready }), AtExit::Nothing, "another update under way");
    assert_eq!(at_exit(ExitFacts { another_running: true, ..ready }), AtExit::Nothing, "a new start of the player");
    let ending = ExitFacts { session_ending: true, ..ready };
    assert_eq!(at_exit(ending), AtExit::SessionEnds);
    assert_eq!(at_exit(ExitFacts { game_running: true, ..ending }), AtExit::SessionEnds);
}

/// Only the messages of the end of the session count: asked, or told that it ends; not told that it goes on, and no
/// other message.
#[test]
fn only_the_messages_of_the_end_of_the_session_count_as_its_end() {
    assert!(ends_session(WM_QUERYENDSESSION, 0), "asked whether it may end");
    assert!(ends_session(WM_ENDSESSION, 1), "told that it ends");
    assert!(!ends_session(WM_ENDSESSION, 0), "told that it goes on");
    for other in [0x0010, 0x0002, 0x0012, 0x0400] {
        assert!(!ends_session(other, 1), "message {other:#x}");
    }
    // The real numbers of Windows.
    assert_eq!((WM_QUERYENDSESSION, WM_ENDSESSION), (0x11, 0x16));
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
        at_exit: false,
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
