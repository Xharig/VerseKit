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

//! Which build this is, the build players get or the test build of the developer, and the rule that the two never
//! run at the same time.
//!
//! The build is decided when the program is compiled (the feature `test-build`), never by a setting or a variable a
//! player could set. Each running program holds a shared lock on the file of its own build in a folder both builds
//! know; a start asks whether the other build holds its file, which only a running program does.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Whether this program was built as the test build.
pub const IS_TEST_BUILD: bool = cfg!(feature = "test-build");

/// Whether this is the build of the developer (the test build): the one question every part asks. Only the feature
/// `test-build` at compile time makes it true; no setting and no environment variable can.
pub const fn developer_build() -> bool {
    IS_TEST_BUILD
}

/// One of the two builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    /// The build players get.
    Live,
    /// The test build of the developer.
    Test,
}

impl Edition {
    /// The build this program is.
    pub const fn this() -> Edition {
        if IS_TEST_BUILD { Edition::Test } else { Edition::Live }
    }

    /// The other build.
    pub const fn other(self) -> Edition {
        match self {
            Edition::Live => Edition::Test,
            Edition::Test => Edition::Live,
        }
    }

    fn lock_file(self) -> &'static str {
        match self {
            Edition::Live => "versekit-edition-live.lock",
            Edition::Test => "versekit-edition-test.lock",
        }
    }

    /// The text keys of the message a start shows when this build runs already: title and text.
    pub const fn running_keys(self) -> (&'static str, &'static str) {
        match self {
            Edition::Live => ("s_fassung_sperre_titel", "s_fassung_live_laeuft"),
            Edition::Test => ("s_fassung_sperre_titel", "s_fassung_test_laeuft"),
        }
    }
}

/// The hold of a running program on the file of its build; it ends with the program.
#[derive(Debug)]
pub struct Claim {
    /// `None` where the folder could not be used: the start goes on without a hold.
    _own: Option<File>,
}

/// The folder both builds keep their lock files in: the temporary folder of the user.
pub fn lock_folder() -> PathBuf {
    std::env::temp_dir()
}

/// Takes the hold of the build `this` in `folder`, unless the other build runs: then it is named in the error and
/// nothing is held. A folder that cannot be used does not stop the start.
///
/// # Errors
///
/// The build that runs.
pub fn claim_in(folder: &Path, this: Edition) -> Result<Claim, Edition> {
    let open = |edition: Edition| -> Option<File> {
        std::fs::create_dir_all(folder).ok()?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join(edition.lock_file()))
            .ok()
    };
    let own = open(this).and_then(|file| file.lock_shared().ok().map(|()| file));
    if let Some(other) = open(this.other()) {
        match other.try_lock() {
            Ok(()) => {
                let _ = other.unlock();
            }
            Err(TryLockError::WouldBlock) => return Err(this.other()),
            Err(TryLockError::Error(error)) => crate::errors::record("edition.lock", &error),
        }
    }
    Ok(Claim { _own: own })
}

/// [`claim_in`] for this program in the shared folder ([`lock_folder`]).
///
/// # Errors
///
/// The build that runs.
pub fn claim() -> Result<Claim, Edition> {
    claim_in(&lock_folder(), Edition::this())
}

/// A running process as the list of the system gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    /// The name of the program file.
    pub name: String,
    /// The program file with its path, where the system tells it.
    pub image: Option<PathBuf>,
}

/// The names of the program files (without the ending, ignoring case) of the builds players get, those that hold no
/// lock file: the program file of the installation, which earlier versions share, the file of the program under its
/// own name and the overlay of the time before the two were one program.
const LIVE_PROGRAMS: [&str; 3] = [crate::legacy::PROGRAM_PROCESS, "versekit", "versekit-overlay"];

/// The processes among `processes` that are a build players get: a program file of [`LIVE_PROGRAMS`] that lies
/// elsewhere than this program's own file (`own_image`) and is not this process. A process whose file the system
/// does not tell counts. Such a build, which earlier versions do not hold a lock file for, is what the test build
/// must not run beside.
pub fn live_programs(processes: &[ProcessEntry], own_pid: u32, own_image: Option<&Path>) -> Vec<u32> {
    processes
        .iter()
        .filter(|process| process.pid != own_pid && is_live_program(&process.name))
        .filter(|process| match (&process.image, own_image) {
            (Some(image), Some(own)) => !same_file(image, own),
            _ => true,
        })
        .map(|process| process.pid)
        .collect()
}

/// Whether the name of a program file is one of [`LIVE_PROGRAMS`].
fn is_live_program(name: &str) -> bool {
    let bare = name.rsplit_once('.').filter(|(_, ending)| ending.eq_ignore_ascii_case("exe")).map_or(name, |(bare, _)| bare);
    LIVE_PROGRAMS.iter().any(|program| program.eq_ignore_ascii_case(bare))
}

/// Whether two paths name the same file: the separators and, under Windows, the case do not count.
fn same_file(a: &Path, b: &Path) -> bool {
    let text = |path: &Path| {
        let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let text = full.to_string_lossy().replace('\\', "/");
        let text = text.strip_prefix("//?/").unwrap_or(&text).to_string();
        if cfg!(windows) { text.to_lowercase() } else { text }
    };
    text(a) == text(b)
}

/// The processes the system lists now; `None` where it cannot be read, and on a system this is not made for.
pub fn running_processes() -> Option<Vec<ProcessEntry>> {
    #[cfg(windows)]
    {
        windows_processes()
    }
    #[cfg(target_os = "linux")]
    {
        linux_processes()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_processes() -> Option<Vec<ProcessEntry>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    // SAFETY: a snapshot handle that is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: an all-zero entry is a valid empty one; its size is set as the call wants.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = Vec::new();
    // SAFETY: `entry` carries its size; the snapshot is valid until closed.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) } != 0;
    while more {
        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
        let pid = entry.th32ProcessID;
        found.push(ProcessEntry { pid, name, image: crate::update_run::image_of(pid) });
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the handle from above, closed once.
    unsafe { CloseHandle(snapshot) };
    Some(found)
}

#[cfg(target_os = "linux")]
fn linux_processes() -> Option<Vec<ProcessEntry>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else { continue };
        let image = std::fs::read_link(entry.path().join("exe")).ok();
        let name = image.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        let Some(name) = name.or_else(|| {
            std::fs::read_to_string(entry.path().join("comm")).ok().map(|comm| comm.trim_end().to_string())
        }) else {
            continue;
        };
        found.push(ProcessEntry { pid, name, image });
    }
    Some(found)
}

/// Whether a build players get runs now without a lock file (an installed version from before the lock files): only
/// the test build asks, the player build is the one that holds a lock file. A list of processes that cannot be read
/// is noted and answers no.
pub fn live_program_runs() -> bool {
    if !IS_TEST_BUILD {
        return false;
    }
    let Some(processes) = running_processes() else {
        crate::errors::note("edition.processes", &"the list of the processes cannot be read");
        return false;
    };
    let own = std::env::current_exe().ok();
    !live_programs(&processes, std::process::id(), own.as_deref()).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("versekit-edition-{name}-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn the_player_build_is_live_and_only_the_test_build_feature_makes_it_the_test_build() {
        assert_eq!(IS_TEST_BUILD, cfg!(feature = "test-build"));
        assert_eq!(developer_build(), cfg!(feature = "test-build"));
        assert_eq!(Edition::this(), if IS_TEST_BUILD { Edition::Test } else { Edition::Live });
        assert_eq!(Edition::Live.other(), Edition::Test);
        assert_eq!(Edition::Test.other(), Edition::Live);
    }

    #[test]
    fn a_running_live_build_keeps_the_test_build_from_starting() {
        let dir = folder("live-first");
        let live = claim_in(&dir, Edition::Live).expect("nothing runs");
        assert_eq!(claim_in(&dir, Edition::Test).unwrap_err(), Edition::Live);
        drop(live);
        assert!(claim_in(&dir, Edition::Test).is_ok(), "the lock goes with the program");
    }

    #[test]
    fn a_running_test_build_keeps_the_live_build_from_starting() {
        let dir = folder("test-first");
        let test = claim_in(&dir, Edition::Test).expect("nothing runs");
        assert_eq!(claim_in(&dir, Edition::Live).unwrap_err(), Edition::Test);
        drop(test);
        assert!(claim_in(&dir, Edition::Live).is_ok());
    }

    #[test]
    fn several_programs_of_one_build_run_together() {
        let dir = folder("same");
        let first = claim_in(&dir, Edition::Live).unwrap();
        let second = claim_in(&dir, Edition::Live).expect("the window of the same build");
        assert_eq!(claim_in(&dir, Edition::Test).unwrap_err(), Edition::Live);
        drop(first);
        assert_eq!(claim_in(&dir, Edition::Test).unwrap_err(), Edition::Live, "one is still running");
        drop(second);
        assert!(claim_in(&dir, Edition::Test).is_ok());
    }

    #[test]
    fn a_refused_start_holds_nothing() {
        let dir = folder("refused");
        let _live = claim_in(&dir, Edition::Live).unwrap();
        assert!(claim_in(&dir, Edition::Test).is_err());
        drop(_live);
        // The refused start left no hold behind that would keep the live build out.
        assert!(claim_in(&dir, Edition::Live).is_ok());
    }

    #[test]
    fn a_folder_that_cannot_be_used_does_not_stop_the_start() {
        let file = std::env::temp_dir().join(format!("versekit-edition-file-{}", crate::testdir::run_tag()));
        std::fs::write(&file, "").unwrap();
        assert!(claim_in(&file.join("deeper"), Edition::Live).is_ok());
        let _ = std::fs::remove_file(&file);
    }

    fn process(pid: u32, name: &str, image: Option<&str>) -> ProcessEntry {
        ProcessEntry { pid, name: name.to_string(), image: image.map(PathBuf::from) }
    }

    /// An installed version holds no lock file: the test build finds it in the list of the processes, by the
    /// program files of the builds players get, wherever they lie but at its own place.
    #[test]
    fn a_program_of_the_player_build_from_another_place_is_found_in_the_process_list() {
        let own = Path::new("C:/Test/release/versekit.exe");
        let old_file = crate::legacy::PROGRAM_FILE;
        let list = [
            process(10, old_file, Some(&format!("C:/Programs/Installed/{old_file}"))),
            process(11, "versekit.exe", Some("D:/Build/release/versekit.exe")),
            process(12, "VERSEKIT-OVERLAY.EXE", Some("D:/Old/versekit-overlay.exe")),
            process(13, "notepad.exe", Some("C:/Windows/notepad.exe")),
        ];
        assert_eq!(live_programs(&list, 99, Some(own)), [10, 11, 12]);
    }

    #[test]
    fn this_process_and_the_other_processes_of_its_own_program_file_are_not_a_second_build() {
        let own = Path::new("C:/Test/release/versekit.exe");
        let list = [
            process(99, "versekit.exe", Some("C:/Test/release/versekit.exe")),
            process(98, "versekit.exe", Some("C:/Test/release/versekit.exe")),
            process(97, "versekit.exe", Some("c:\\test\\release\\VERSEKIT.exe")),
        ];
        assert!(live_programs(&list, 99, Some(own)).is_empty(), "the windows and the overlay of the same file");
        assert_eq!(live_programs(&list[..1], 1, None), [99], "without the own file only the own process is known");
        assert!(
            live_programs(&list[..1], 99, None).is_empty(),
            "this process is never a second build, whatever is known of its file"
        );
        let elsewhere = [process(99, "versekit.exe", Some("D:/Other/versekit.exe"))];
        assert!(live_programs(&elsewhere, 99, Some(own)).is_empty(), "nor when its file reads differently");
    }

    #[test]
    fn other_programs_and_tools_of_the_family_are_no_build_players_get() {
        let list = [
            process(20, "versekit-parity.exe", Some("C:/T/versekit-parity.exe")),
            process(21, "versekit-orgserver.exe", Some("C:/T/versekit-orgserver.exe")),
            process(22, "versekit-bar.exe", None),
            process(23, "StarCitizen.exe", Some("C:/Games/StarCitizen.exe")),
        ];
        assert!(live_programs(&list, 1, Some(Path::new("C:/Test/versekit.exe"))).is_empty());
    }

    #[test]
    fn a_program_of_the_player_build_whose_file_the_system_does_not_tell_counts() {
        let list = [process(30, crate::legacy::PROGRAM_FILE, None)];
        assert_eq!(live_programs(&list, 1, Some(Path::new("C:/Test/versekit.exe"))), [30]);
    }

    /// The player build never asks the list of the processes: it is the build with the lock file.
    #[test]
    fn the_player_build_does_not_look_for_another_build_in_the_process_list() {
        if !IS_TEST_BUILD {
            assert!(!live_program_runs());
        }
    }

    /// The list of the system holds at least this very process, on the systems the build is made for.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn the_list_of_the_system_holds_this_process() {
        let list = running_processes().expect("the list of the processes");
        assert!(list.iter().any(|entry| entry.pid == std::process::id()), "{} processes", list.len());
        let own = list.iter().find(|entry| entry.pid == std::process::id()).unwrap();
        let image = own.image.as_deref().expect("the file of this process");
        assert!(crate::update_run::same_path(image, &std::env::current_exe().unwrap()), "{image:?}");
    }

    #[test]
    fn both_messages_name_the_build_that_runs() {
        assert_eq!(Edition::Live.running_keys().1, "s_fassung_live_laeuft");
        assert_eq!(Edition::Test.running_keys().1, "s_fassung_test_laeuft");
        for key in [Edition::Live.running_keys(), Edition::Test.running_keys()] {
            for lang in [crate::language::Lang::De, crate::language::Lang::En] {
                assert!(!crate::language::t_in(lang, key.0).starts_with("s_"), "{key:?}");
                assert!(!crate::language::t_in(lang, key.1).starts_with("s_"), "{key:?}");
            }
        }
    }
}
