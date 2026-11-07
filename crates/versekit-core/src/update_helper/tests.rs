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

use std::cell::{Cell, RefCell};
use std::fs;

use super::*;
use crate::testdir::Dir;
use crate::update_run::HelperJob;
use crate::updater::checksum_of;

/// A stand-in for the system: the old program lives for so many looks, the installer answers with a code.
struct Fake {
    old_rounds: Cell<u32>,
    others: Cell<u32>,
    code: i32,
    slept: Cell<u32>,
    setups: RefCell<Vec<String>>,
    started: RefCell<Vec<PathBuf>>,
    installed: Option<PathBuf>,
}

impl Fake {
    fn new(old_rounds: u32, code: i32) -> Fake {
        Fake {
            old_rounds: Cell::new(old_rounds),
            others: Cell::new(0),
            code,
            slept: Cell::new(0),
            setups: RefCell::default(),
            started: RefCell::default(),
            installed: None,
        }
    }
}

impl System for Fake {
    fn processes_of(&self, _program: &Path) -> Option<Vec<u32>> {
        let left = self.others.get();
        if left == 0 {
            return Some(Vec::new());
        }
        self.others.set(left - 1);
        Some(vec![77])
    }

    fn alive(&self, _pid: u32) -> bool {
        let left = self.old_rounds.get();
        if left == 0 {
            return false;
        }
        self.old_rounds.set(left - 1);
        true
    }

    fn sleep(&self, _time: Duration) {
        self.slept.set(self.slept.get() + 1);
    }

    fn run_setup(&self, setup: &Path, arguments: &str) -> io::Result<i32> {
        self.setups.borrow_mut().push(format!("{} {arguments}", setup.display()));
        Ok(self.code)
    }

    fn installed_program(&self, _key: &str) -> Option<PathBuf> {
        self.installed.clone()
    }

    fn start(&self, program: &Path) -> io::Result<()> {
        self.started.borrow_mut().push(program.to_path_buf());
        Ok(())
    }
}

/// A job in a throwaway folder: an installer file, its checksum, the program file and the lock.
fn job_in(dir: &Dir) -> HelperJob {
    let folder = dir.path().join("Programm mit & ^ % ! ( ) ' Ümlaut");
    fs::create_dir_all(&folder).unwrap();
    let program = folder.join("SC-BP-Watcher.exe");
    fs::write(&program, b"alt").unwrap();
    let setup = dir.path().join("VerseKit-Setup.exe");
    fs::write(&setup, b"der Installer").unwrap();
    let lock = dir.path().join("update-sperre.json");
    fs::write(&lock, b"{}").unwrap();
    HelperJob {
        checksum: checksum_of(&setup).unwrap(),
        setup,
        target_dir: folder,
        setup_log: dir.path().join("update-setup.txt"),
        log: dir.path().join("update-helfer.txt"),
        result: dir.path().join("update-ergebnis.txt"),
        lock,
        program,
        pid: 4242,
        program_key: String::new(),
        wait: 5,
    }
}

#[test]
fn the_helper_waits_checks_installs_writes_down_and_starts_again() {
    let dir = Dir::new("helfer-gut");
    let job = job_in(&dir);
    let fake = Fake::new(2, 0);
    fake.others.set(1);
    assert_eq!(run(&job, &fake), 0);
    assert_eq!(fake.slept.get(), 2, "waited while the old program lived");
    let setups = fake.setups.borrow();
    assert_eq!(setups.len(), 1);
    assert!(
        setups[0].ends_with(&format!(
            "/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR=\"{}\" /LOG=\"{}\"",
            job.target_dir.display(),
            job.setup_log.display()
        )),
        "{}",
        setups[0]
    );
    assert_eq!(fs::read_to_string(&job.result).unwrap(), "0\r\n");
    assert!(!job.lock.exists(), "the lock is released");
    assert_eq!(*fake.started.borrow(), std::slice::from_ref(&job.program));
    let log = fs::read_to_string(&job.log).unwrap();
    for line in ["Helper started", "Old version has exited", "Checksum matches", "exit code 0", "Starting the program"]
    {
        assert!(log.contains(line), "{line}: {log}");
    }
}

#[test]
fn a_changed_installer_is_deleted_and_never_run() {
    let dir = Dir::new("helfer-summe");
    let job = job_in(&dir);
    fs::write(&job.setup, b"ausgetauscht").unwrap();
    let fake = Fake::new(0, 0);
    assert_eq!(run(&job, &fake), RC_CHECKSUM_BAD);
    assert!(fake.setups.borrow().is_empty(), "nothing installed");
    assert!(!job.setup.exists(), "the file is gone");
    assert!(fake.started.borrow().is_empty(), "90 starts nothing");
    assert_eq!(fs::read_to_string(&job.result).unwrap(), "90\r\n");
    assert!(!job.lock.exists());
}

#[test]
fn an_old_program_that_does_not_end_stops_the_update() {
    let dir = Dir::new("helfer-haengt");
    let job = job_in(&dir);
    let fake = Fake::new(u32::MAX, 0);
    assert_eq!(run(&job, &fake), RC_OLD_STUCK);
    assert_eq!(fake.slept.get(), 5, "waited the whole time");
    assert!(fake.setups.borrow().is_empty() && fake.started.borrow().is_empty());
    assert!(job.setup.exists(), "the installer stays for the next try");
    // Another process of the same program file (the main window) counts as well.
    let fake = Fake::new(0, 0);
    fake.others.set(u32::MAX);
    assert_eq!(run(&job, &fake), RC_OLD_STUCK);
}

#[test]
fn only_the_codes_that_change_nothing_or_install_start_the_program_again() {
    for (code, restart) in [(0, true), (1, false), (2, true), (3, true), (4, false), (5, true), (8, false)] {
        let dir = Dir::new("helfer-codes");
        let job = job_in(&dir);
        let fake = Fake::new(0, code);
        assert_eq!(run(&job, &fake), code);
        assert_eq!(fake.started.borrow().len(), usize::from(restart), "exit code {code}");
        assert_eq!(fs::read_to_string(&job.result).unwrap(), format!("{code}\r\n"));
    }
}

#[test]
fn the_program_the_installer_names_is_started_when_it_lies_in_the_folder() {
    let dir = Dir::new("helfer-programm");
    let mut job = job_in(&dir);
    job.program_key = "Software\\Test".into();
    let renamed = job.target_dir.join("VerseKit.exe");
    fs::write(&renamed, b"neu").unwrap();
    let mut fake = Fake::new(0, 0);
    fake.installed = Some(renamed.clone());
    assert_eq!(restart_program(&job, &fake), renamed);
    // Missing, or somewhere else: the program that ran.
    fake.installed = Some(job.target_dir.join("fehlt.exe"));
    assert_eq!(restart_program(&job, &fake), job.program);
    let elsewhere = dir.path().join("VerseKit.exe");
    fs::write(&elsewhere, b"x").unwrap();
    fake.installed = Some(elsewhere);
    assert_eq!(restart_program(&job, &fake), job.program);
    // Without a key it is never asked.
    job.program_key.clear();
    fake.installed = Some(renamed);
    assert_eq!(restart_program(&job, &fake), job.program);
}

#[cfg(windows)]
#[test]
fn the_process_list_finds_the_running_test_program_by_its_file() {
    let own = std::env::current_exe().unwrap();
    // This very test runs from that file, and the own process does not count.
    assert_eq!(processes_of(&own), Some(Vec::new()));
    assert_eq!(processes_of(&own.with_file_name("gibt-es-nicht.exe")), Some(Vec::new()));
    // A process of another file is found by that file, and only of that file: a copy of `ping` that waits a
    // few seconds, without a window.
    let dir = Dir::new("helfer-liste");
    let copy = dir.path().join("versekit-liste-test.exe");
    let ping =
        Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\PING.EXE");
    fs::copy(&ping, &copy).unwrap();
    let mut child = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&copy)
            .args(["-n", "30", "127.0.0.1"])
            .creation_flags(crate::update_run::HELPER_FLAGS)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    };
    let found = processes_of(&copy).unwrap();
    let other_copy = dir.path().join("andere").join("versekit-liste-test.exe");
    fs::create_dir_all(other_copy.parent().unwrap()).unwrap();
    let elsewhere = processes_of(&other_copy).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(found, [child.id()]);
    assert!(elsewhere.is_empty(), "the same name in another folder is another program: {elsewhere:?}");
    assert!(same_path(&copy, &dir.path().join("VERSEKIT-LISTE-TEST.EXE")));
}
