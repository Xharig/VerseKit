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

//! The built program as the update helper, with stand-ins that open no window: a copy of `where` as the
//! installer (it ends with 2, a code after which the program is started again), a copy of `ping` as the
//! installer that ends with 1 (no restart), and a copy of `ping` as the program file. Nothing is installed.

#![cfg(windows)]

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use versekit_core::testdir::Dir;
use versekit_core::update_run::{HELPER_FLAGS, HELPER_SWITCH, HelperJob, RC_CHECKSUM_BAD};
use versekit_core::updater::checksum_of;

fn system_file(name: &str) -> PathBuf {
    Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("System32").join(name)
}

/// A job in `dir`: the installer is a copy of `setup_from`, the program a copy of `ping` in a folder with the
/// characters that broke paths before.
fn job_in(dir: &Path, setup_from: &str) -> HelperJob {
    let folder = dir.join("Programm & ^ % ! ( ) ' Ümlaut");
    fs::create_dir_all(&folder).unwrap();
    let program = folder.join("SC-BP-Watcher.exe");
    fs::copy(system_file("PING.EXE"), &program).unwrap();
    let setup = dir.join("VerseKit-Setup.exe");
    fs::copy(system_file(setup_from), &setup).unwrap();
    let lock = dir.join("update-sperre.json");
    fs::write(&lock, b"{}").unwrap();
    HelperJob {
        checksum: checksum_of(&setup).unwrap(),
        setup,
        target_dir: folder,
        setup_log: dir.join("update-setup.txt"),
        log: dir.join("update-helfer.txt"),
        result: dir.join("update-ergebnis.txt"),
        lock,
        program,
        pid: u32::MAX - 1,
        program_key: String::new(),
        wait: 5,
    }
}

/// Run the built program as the helper with `job` and return its log.
fn helper(job: &HelperJob) -> String {
    let status = Command::new(env!("CARGO_BIN_EXE_versekit"))
        .arg(HELPER_SWITCH)
        .envs(job.to_env())
        .creation_flags(HELPER_FLAGS)
        .status()
        .expect("the helper starts");
    assert!(status.success(), "{status:?}");
    fs::read_to_string(&job.log).unwrap_or_default()
}

#[test]
fn the_helper_runs_the_checked_installer_and_starts_the_program_again() {
    let dir = Dir::new("app-helfer-gut");
    let job = job_in(dir.path(), "where.exe");
    let log = helper(&job);
    assert_eq!(fs::read_to_string(&job.result).unwrap(), "2\r\n", "{log}");
    assert!(!job.lock.exists(), "the lock is released");
    assert!(log.contains("Checksum matches, starting installer"), "{log}");
    assert!(log.contains("Installer finished, exit code 2"), "{log}");
    assert!(log.contains("Starting the program"), "2 changes nothing: the program comes back: {log}");
}

#[test]
fn after_an_unknown_exit_code_nothing_is_started() {
    let dir = Dir::new("app-helfer-code");
    let job = job_in(dir.path(), "PING.EXE");
    let log = helper(&job);
    assert_eq!(fs::read_to_string(&job.result).unwrap(), "1\r\n", "{log}");
    assert!(log.contains("No restart after exit code 1"), "{log}");
}

#[test]
fn an_installer_changed_after_the_check_is_deleted_by_the_helper() {
    let dir = Dir::new("app-helfer-summe");
    let job = job_in(dir.path(), "where.exe");
    fs::copy(system_file("PING.EXE"), &job.setup).unwrap();
    let log = helper(&job);
    assert_eq!(fs::read_to_string(&job.result).unwrap(), format!("{RC_CHECKSUM_BAD}\r\n"), "{log}");
    assert!(!job.setup.exists(), "{log}");
    assert!(!log.contains("starting installer"), "{log}");
}

#[test]
fn without_a_complete_job_the_helper_does_nothing() {
    let dir = Dir::new("app-helfer-ohne");
    let job = job_in(dir.path(), "where.exe");
    let status = Command::new(env!("CARGO_BIN_EXE_versekit"))
        .arg(HELPER_SWITCH)
        .envs(job.to_env().into_iter().filter(|(k, _)| !k.ends_with("_SHA256")))
        .creation_flags(HELPER_FLAGS)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!job.result.exists() && job.lock.exists() && job.setup.exists(), "nothing touched");
}
