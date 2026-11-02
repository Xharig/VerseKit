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

//! The write lock across processes (`jsonio::share_saves_between_processes`): two programs of one run (overlay
//! and main window) write the same JSON files and lose nothing.
//!
//! The test starts itself again as the other process (`child_entry`, told what to do by variables), so no
//! window and no second program is needed.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use versekit_core::jsonio::{self, Value};
use versekit_core::testdir::Dir;

const ROLE: &str = "VK_LOCK_ROLE";
const DIR: &str = "VK_LOCK_DIR";
const NAME: &str = "VK_LOCK_NAME";
const COUNT: &str = "VK_LOCK_COUNT";
const HOLD_MS: &str = "VK_LOCK_HOLD_MS";
const SHARE: &str = "VK_LOCK_SHARE";

/// The processes share one lock file per test, the tests of this binary one lock: one test at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_file(dir: &Path) -> String {
    dir.join("sperre.lock").to_string_lossy().into_owned()
}

fn list_file(dir: &Path) -> String {
    dir.join("liste.json").to_string_lossy().into_owned()
}

fn marker(dir: &Path) -> PathBuf {
    dir.join("haelt.txt")
}

fn try_read_names(file: &str) -> Option<Vec<String>> {
    match jsonio::read_json(file) {
        Ok(Value::Array(items)) => Some(items.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
        _ => None,
    }
}

fn read_names(file: &str) -> Vec<String> {
    try_read_names(file).expect("Liste nicht lesbar")
}

/// The other process: runs this test binary for `child_entry` only.
fn spawn(role: &str, dir: &Path, extra: &[(&str, String)]) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("eigenes Programm"));
    command
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .env(DIR, dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in extra {
        command.env(key, value);
    }
    command.spawn().expect("Kindprozess startet")
}

fn wait_for(path: &Path, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn finish(mut child: Child) {
    let status = child.wait().expect("Kindprozess endet");
    assert!(status.success(), "Kindprozess fehlgeschlagen: {status:?}");
}

/// The part of the other process; a normal run of the test does nothing here.
#[test]
fn child_entry() {
    let Ok(role) = std::env::var(ROLE) else { return };
    let dir = PathBuf::from(std::env::var(DIR).expect("Ordner"));
    let shared = std::env::var(SHARE).as_deref() != Ok("0");
    if shared {
        assert!(jsonio::share_saves_between_processes(&lock_file(&dir)), "Sperrdatei öffnet");
    }
    match role.as_str() {
        // Read, change and save under the write lock, the way the overlay and the pages do. The pause opens the
        // window in which the other process would write between the read and the save.
        "writer" => {
            let name = std::env::var(NAME).expect("Name");
            let count: usize = std::env::var(COUNT).expect("Anzahl").parse().expect("Zahl");
            // Both start together: on a busy computer the second may be started long after the first is done.
            std::fs::write(dir.join(format!("bereit-{name}.txt")), "ja").expect("Meldung");
            assert!(wait_for(&dir.join("los.txt"), Duration::from_secs(120)), "das Startzeichen kommt");
            for n in 0..count {
                let _lock = jsonio::lock_saves();
                // Without the shared lock the other process may be replacing the file just now: then the
                // read or the write fails now and then, and the control run only tries again.
                let mut names = loop {
                    match try_read_names(&list_file(&dir)) {
                        Some(names) => break names,
                        None if !shared => std::thread::sleep(Duration::from_millis(1)),
                        None => panic!("Liste nicht lesbar"),
                    }
                };
                std::thread::sleep(Duration::from_millis(1));
                names.push(format!("{name}-{n}"));
                let value = Value::Array(names.iter().map(Value::str).collect());
                let saved = jsonio::save_json(&list_file(&dir), &value, 1, false);
                if shared {
                    saved.expect("speichert");
                }
            }
        }
        // Take the lock, say so, hold it (a number of milliseconds, or until killed).
        "holder" => {
            let hold: u64 = std::env::var(HOLD_MS).ok().and_then(|v| v.parse().ok()).unwrap_or(60_000);
            let _lock = jsonio::lock_saves();
            std::fs::write(marker(&dir), "ja").expect("Marke");
            std::thread::sleep(Duration::from_millis(hold));
        }
        other => panic!("unbekannte Rolle {other}"),
    }
}

fn run_writers(share: bool) -> Vec<String> {
    let dir = Dir::new("sperre-schreiber");
    let list = list_file(dir.path());
    jsonio::save_json(&list, &Value::Array(Vec::new()), 1, false).expect("Liste anlegen");
    let extra = |name: &str| {
        vec![(NAME, name.to_string()), (COUNT, "150".to_string()), (SHARE, if share { "1" } else { "0" }.to_string())]
    };
    let a = spawn("writer", dir.path(), &extra("a"));
    let b = spawn("writer", dir.path(), &extra("b"));
    // Both are started and say so: off they go.
    for name in ["a", "b"] {
        assert!(wait_for(&dir.path().join(format!("bereit-{name}.txt")), Duration::from_secs(120)), "{name} startet");
    }
    std::fs::write(dir.path().join("los.txt"), "los").expect("Startzeichen");
    finish(a);
    finish(b);
    read_names(&list)
}

#[test]
fn two_processes_lose_no_write() {
    let _one = serial();
    let names = run_writers(true);
    assert_eq!(names.len(), 300, "jeder der 300 Einträge steht in der Liste");
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), 300);
}

/// The counter-check built into the test: without the shared lock the same scenario does lose entries, so
/// the check above can turn red.
#[test]
fn without_the_shared_lock_the_same_scenario_loses_writes() {
    let _one = serial();
    let names = run_writers(false);
    assert!(names.len() < 300, "ohne gemeinsame Sperre gehen Einträge verloren, es blieben {}", names.len());
}

#[test]
fn a_process_waits_for_the_holder_and_goes_on_after() {
    let _one = serial();
    let dir = Dir::new("sperre-wartet");
    assert!(jsonio::share_saves_between_processes(&lock_file(dir.path())));
    jsonio::set_process_wait(Duration::from_secs(15));
    let child = spawn("holder", dir.path(), &[(HOLD_MS, "700".to_string())]);
    assert!(wait_for(&marker(dir.path()), Duration::from_secs(20)), "das Kind hält die Sperre");
    let start = Instant::now();
    let guard = jsonio::lock_saves();
    let waited = start.elapsed();
    drop(guard);
    finish(child);
    assert!(waited >= Duration::from_millis(300), "gewartet hat der Prozess nur {waited:?}");
    assert!(waited < Duration::from_secs(10), "gewartet hat der Prozess {waited:?}");
}

#[test]
fn the_lock_is_free_again_when_the_guard_is_dropped() {
    let _one = serial();
    let dir = Dir::new("sperre-frei");
    let file = lock_file(dir.path());
    assert!(jsonio::share_saves_between_processes(&file));
    jsonio::set_process_wait(Duration::from_secs(15));
    // Another handle of the same file stands for another process.
    let other = jsonio::ProcessLock::open(&file).expect("zweiter Zugriff");
    let guard = jsonio::lock_saves();
    assert!(
        matches!(other.acquire(Duration::ZERO), jsonio::Taken::TimedOut),
        "while the guard is held, the other one is kept out"
    );
    drop(guard);
    assert!(matches!(other.acquire(Duration::ZERO), jsonio::Taken::Held), "after the drop it is free at once");
    other.release();
}

#[test]
fn a_killed_holder_leaves_no_lock_behind() {
    let _one = serial();
    let dir = Dir::new("sperre-kill");
    assert!(jsonio::share_saves_between_processes(&lock_file(dir.path())));
    jsonio::set_process_wait(Duration::from_secs(15));
    let mut child = spawn("holder", dir.path(), &[]);
    assert!(wait_for(&marker(dir.path()), Duration::from_secs(20)), "das Kind hält die Sperre");
    child.kill().expect("Kind beenden");
    let _ = child.wait();
    let start = Instant::now();
    let guard = jsonio::lock_saves();
    let waited = start.elapsed();
    drop(guard);
    assert!(waited < Duration::from_secs(3), "die Sperre blieb stehen: {waited:?}");
}

#[test]
fn the_wait_has_a_limit() {
    let _one = serial();
    let dir = Dir::new("sperre-grenze");
    assert!(jsonio::share_saves_between_processes(&lock_file(dir.path())));
    let reports = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = reports.clone();
    versekit_core::errors::set_sink(move |label, text| {
        if label == "jsonio.sperre" {
            sink.lock().unwrap().push(text.to_string());
        }
    });
    jsonio::set_process_wait(Duration::from_millis(400));
    let mut child = spawn("holder", dir.path(), &[]);
    assert!(wait_for(&marker(dir.path()), Duration::from_secs(20)), "das Kind hält die Sperre");
    let start = Instant::now();
    let guard = jsonio::lock_saves();
    let waited = start.elapsed();
    drop(guard);
    assert!(waited >= Duration::from_millis(350), "zu früh zurück: {waited:?}");
    assert!(waited < Duration::from_secs(5), "friert ein: {waited:?}");
    assert_eq!(reports.lock().unwrap().len(), 1, "genau eine Meldung: {:?}", reports.lock().unwrap());
    // The lock works again afterwards: once the holder is gone, no further wait, no further report.
    child.kill().expect("Kind beenden");
    let _ = child.wait();
    let start = Instant::now();
    drop(jsonio::lock_saves());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(reports.lock().unwrap().len(), 1);
    jsonio::set_process_wait(jsonio::PROCESS_WAIT);
}
