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

//! The threads of this process by name, for the checks of what a page starts in the background.

/// The names of the threads of this process (an empty text for a thread without a name), one entry per thread.
#[cfg(windows)]
pub(super) fn names() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, GetThreadDescription, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
    };
    let mut found = Vec::new();
    // SAFETY: plain calls of the system with handles opened and closed here; the entry has its size set.
    unsafe {
        let pid = GetCurrentProcessId();
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snapshot, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == pid {
                let mut name = String::new();
                let handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.th32ThreadID);
                if !handle.is_null() {
                    let mut text: *mut u16 = std::ptr::null_mut();
                    if GetThreadDescription(handle, &mut text) >= 0 && !text.is_null() {
                        let mut len = 0;
                        while *text.add(len) != 0 {
                            len += 1;
                        }
                        name = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                        LocalFree(text.cast());
                    }
                    CloseHandle(handle);
                }
                found.push(name);
            }
            more = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    found
}

/// Without the thread list of the system nothing can be told.
#[cfg(not(windows))]
pub(super) fn names() -> Vec<String> {
    Vec::new()
}

/// The threads of the process counted by name (name, how many), for a report.
pub(super) fn summary(names: &[String]) -> String {
    let mut counted: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for name in names {
        *counted.entry(name.as_str()).or_default() += 1;
    }
    counted.iter().map(|(name, n)| format!("{}x'{}'", n, name)).collect::<Vec<_>>().join(" ")
}

/// How many threads of the process are not in `before` by name and count: what a step started and left running.
pub(super) fn started_since(before: &[String], after: &[String]) -> Vec<String> {
    let mut left = before.to_vec();
    let mut started = Vec::new();
    for name in after {
        match left.iter().position(|b| b == name) {
            Some(at) => {
                left.swap_remove(at);
            }
            None => started.push(name.clone()),
        }
    }
    started
}

const CENSUS_TEST: &str = "sample::thread_census::census_of_a_page_on_real_data";

/// The census of a page built on real data in a throwaway folder: run alone in a process of its own (the threads
/// of other tests would count too), it prints the threads the page started and left running.
#[test]
#[ignore = "run by a_page_on_real_data_starts_no_reading_of_this_computers_game_log, alone in a process"]
fn census_of_a_page_on_real_data() {
    let _language = crate::language_lock();
    let before = names();
    let (page, dir) = super::real_data::real_page("zensus", "", &[]);
    let built = names();
    let mut bench = super::controls::tests::Bench::new(page);
    // Frames over a second, so a reading started by the first frames is still running.
    let until = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    while std::time::Instant::now() < until {
        bench.settle();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let started = started_since(&before, &names());
    println!("census_built={}", summary(&started_since(&before, &built)));
    println!("census_frames={}", summary(&started));
    println!("census_unnamed={}", started.iter().filter(|name| name.is_empty()).count());
    let _ = std::fs::remove_dir_all(dir);
}

/// The pages of the tests run on a host that looks at no drive: the game of this computer is not found.
#[test]
fn the_host_of_the_tests_finds_no_game_of_this_computer() {
    let dir = super::real_data::folder("host-ohne-spiel");
    let paths = versekit_core::paths::Paths::new(super::real_data::host_in(&dir, &[]));
    assert_eq!(paths.game_folder(), None, "a page on this host must not read the logs of this computer");
    // A test that names a game folder still gets it.
    let game = dir.join("LIVE");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join("Game.log"), "").unwrap();
    let named = game.to_string_lossy().into_owned();
    let paths = versekit_core::paths::Paths::new(super::real_data::host_in(
        &dir,
        &[(versekit_core::paths::ENV_INSTALL_DIR, named.as_str())],
    ));
    assert!(paths.game_folder().is_some(), "a folder named by the test is found");
    let _ = std::fs::remove_dir_all(dir);
}

/// A page on real data starts no reader of this computer's game log: run alone in a process of its own, it leaves no
/// thread without a name behind (the helpers that spread a reading over the cores).
#[test]
fn a_page_on_real_data_starts_no_reading_of_this_computers_game_log() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([CENSUS_TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "the census test failed: {text}");
    let unnamed = text.lines().find_map(|line| line.strip_prefix("census_unnamed=")).expect("the census prints its result");
    assert_eq!(unnamed.trim(), "0", "threads left behind: {text}");
}
