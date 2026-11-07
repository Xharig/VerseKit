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

//! The diagnostics in a real process: a panic of the main thread, a panic of
//! another thread, a Windows exception and recording switched off. The
//! program under test is this very file, started again with a scenario in an
//! environment variable (this test has no harness, so `main` is the main
//! thread of the child).
//!
//! The counter-check: the same panic without `diagnostics::init` leaves the
//! crash file empty, so the checks of the real scenarios can fail.

use std::path::Path;
use std::process::Command;

use versekit_core::diagnostics::{self, Role};
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{self, Host, Paths};
use versekit_core::{crash, trail};

const SCENARIO: &str = "VERSEKIT_TEST_SCENARIO";
/// The folder of the copy that scenario "copy" sets the diagnostics up for.
const OTHER: &str = "VERSEKIT_TEST_COPY";
const VERSION: &str = "5.0.0-test";

fn child(scenario: &str) {
    if scenario == "copy" {
        // The data of a copy, not the running program's: errors and crash file go there.
        let other = std::env::var(OTHER).expect("Ordner der Kopie");
        let copy: &'static Paths = Box::leak(Box::new(paths_in(Path::new(&other))));
        diagnostics::init_for(copy, Role::Main, VERSION);
    } else if scenario == "overlay" {
        diagnostics::init(Role::Overlay, VERSION);
    } else if scenario != "without-init" {
        diagnostics::init(Role::Main, VERSION);
    }
    trail::step("Start des Prüfprogramms");
    match scenario {
        "panic" | "without-init" | "old-key" | "overlay" => panic!("kaputt in {}", paths::global().host.home()),
        "thread" | "copy" => {
            let worker = std::thread::Builder::new().name("arbeiter".into()).spawn(|| panic!("im Faden kaputt"));
            let _ = worker.map(|w| w.join());
        }
        "exception" | "raise" => {
            // No error window of the system for this throw-away process.
            #[cfg(windows)]
            {
                use windows_sys::Win32::System::Diagnostics::Debug::{
                    RaiseException, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode,
                };
                // SAFETY: only switches off the error window of this process.
                unsafe { SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX) };
                if scenario == "raise" {
                    // SAFETY: raises a system exception nobody handles, in this
                    // throw-away process (an integer division by zero).
                    unsafe { RaiseException(0xC000_0094, 0, 0, std::ptr::null()) };
                }
            }
            // SAFETY: deliberately invalid, to raise an access violation in this
            // throw-away process.
            unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
        }
        other => eprintln!("unbekanntes Szenario {other}"),
    }
}

fn run(scenario: &str, folder: &Path) -> std::process::ExitStatus {
    run_with(scenario, folder, folder)
}

/// Like [`run`], with a copy of the data apart from the folder of the running program.
fn run_with(scenario: &str, folder: &Path, copy: &Path) -> std::process::ExitStatus {
    Command::new(std::env::current_exe().expect("eigenes Programm"))
        .env(SCENARIO, scenario)
        .env(OTHER, copy)
        .env("SC_BP_HOME", folder)
        .env("RUST_BACKTRACE", "0")
        .stderr(std::process::Stdio::null())
        .status()
        .expect("Kindprozess")
}

fn fresh(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("versekit-prozess-{name}-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("Ordner");
    dir
}

/// The data folder `folder` on a made-up computer: nothing of the shell the check runs in counts.
fn paths_in(folder: &Path) -> Paths {
    let folder = folder.to_string_lossy();
    Paths::new(Host::new(versekit_core::pypath::Flavor::native(), &[("SC_BP_HOME", &folder)], &folder))
}

fn errors_of(folder: &Path) -> Vec<Value> {
    match jsonio::read_json(&folder.join("fehler.json").to_string_lossy()) {
        Ok(data) => match data.get("eintraege") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

fn field(entry: &Value, name: &str) -> String {
    entry.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn check(ok: bool, what: &str, failures: &mut Vec<String>) {
    if !ok {
        failures.push(what.to_string());
    }
}

fn parent() -> Vec<String> {
    let mut failures = Vec::new();

    // A panic of the main thread: the dump, the error, the trail.
    let dir = fresh("panic");
    let status = run("panic", &dir);
    check(status.code() == Some(101), "panic: exit code 101", &mut failures);
    let dump = std::fs::read_to_string(dir.join(crash::FILE)).unwrap_or_default();
    check(dump.starts_with("Fatal error: panic: kaputt in "), "panic: the dump begins with the message", &mut failures);
    check(
        dump.contains("Current thread 'main' (most recent call first):"),
        "panic: the dump names the thread",
        &mut failures,
    );
    let errors = errors_of(&dir);
    check(
        errors.iter().any(|e| field(e, "stelle") == "unbehandelt" && field(e, "art") == "panic"),
        "panic: entered in the error log as unbehandelt",
        &mut failures,
    );
    check(
        errors.iter().all(|e| !field(e, "meldung").contains(&paths::global().host.home())),
        "panic: no home folder in the error log",
        &mut failures,
    );
    // The next start sets the dump aside with its version.
    let paths = paths_in(&dir);
    crash::install(&paths, "5.0.1-test");
    check(
        crash::last_crash(&paths).first().is_some_and(|l| l.starts_with("Fatal error: panic")),
        "panic: set aside at the next start",
        &mut failures,
    );
    check(crash::crash_version(&paths) == VERSION, "panic: the version of the crashed run", &mut failures);
    check(
        trail::last_trail(&paths).iter().any(|l| l.ends_with("Start des Prüfprogramms")),
        "panic: the trail was written",
        &mut failures,
    );
    check(
        trail::last_trail(&paths).first().is_some_and(|l| l.ends_with(&diagnostics::start_line(VERSION))),
        "panic: the trail begins with version and system",
        &mut failures,
    );

    // The overlay: its own crash file and trail, the error log shared, nothing of the main window's touched.
    let dir = fresh("overlay");
    std::fs::write(dir.join(crash::FILE), "Fatal error: panic: hauptfenster\n").expect("Absturz des Hauptfensters");
    let status = run("overlay", &dir);
    check(status.code() == Some(101), "overlay: exit code 101", &mut failures);
    let dump = std::fs::read_to_string(dir.join(crash::OVERLAY.current)).unwrap_or_default();
    check(dump.starts_with("Fatal error: panic: kaputt in "), "overlay: the dump in its own file", &mut failures);
    check(
        std::fs::read_to_string(dir.join(crash::FILE)).unwrap_or_default() == "Fatal error: panic: hauptfenster\n",
        "overlay: the main window's crash file is left as it was",
        &mut failures,
    );
    check(
        errors_of(&dir).iter().any(|e| field(e, "stelle") == "unbehandelt"),
        "overlay: entered in the shared error log",
        &mut failures,
    );
    let paths = paths_in(&dir);
    check(
        trail::last_trail_of(&paths, trail::FILE_OVERLAY).iter().any(|l| l.ends_with("Start des Prüfprogramms")),
        "overlay: the trail in its own file",
        &mut failures,
    );
    check(!dir.join(trail::FILE).exists(), "overlay: no trail of the main window", &mut failures);

    // A panic of another thread: error log yes, crash file stays empty.
    let dir = fresh("thread");
    let status = run("thread", &dir);
    check(status.success(), "thread: the program goes on and ends well", &mut failures);
    let size = std::fs::metadata(dir.join(crash::FILE)).ok().map(|m| m.len());
    check(size == Some(0), "thread: crash file stays empty", &mut failures);
    check(
        errors_of(&dir).iter().any(|e| field(e, "stelle") == "thread:arbeiter"),
        "thread: entered as thread:arbeiter",
        &mut failures,
    );

    // Set up for the data of a copy: the panic of a thread lands there, the running program's data stay empty.
    let (own, copy) = (fresh("copy-own"), fresh("copy-data"));
    let status = run_with("copy", &own, &copy);
    check(status.success(), "copy: the program goes on and ends well", &mut failures);
    check(
        errors_of(&copy).iter().any(|e| field(e, "stelle") == "thread:arbeiter"),
        "copy: the error is entered in the log of the copy",
        &mut failures,
    );
    check(errors_of(&own).is_empty(), "copy: nothing in the log of the running program", &mut failures);
    check(copy.join(crash::FILE).exists(), "copy: the crash file lies in the copy", &mut failures);
    check(!own.join(crash::FILE).exists(), "copy: no crash file in the running program's data", &mut failures);

    // The old switch of 4.12 is not read: with its key set to false, errors
    // are recorded all the same.
    let dir = fresh("old-key");
    std::fs::write(dir.join("einstellungen.json"), "{\"fehler_mitschreiben\": false}").expect("Einstellungen");
    run("old-key", &dir);
    check(
        errors_of(&dir).iter().any(|e| field(e, "stelle") == "unbehandelt"),
        "old key: the error is recorded all the same",
        &mut failures,
    );
    check(
        std::fs::read_to_string(dir.join(crash::FILE)).unwrap_or_default().starts_with("Fatal error: panic"),
        "old key: the crash file is written",
        &mut failures,
    );
    check(
        std::fs::read_to_string(dir.join("einstellungen.json")).unwrap_or_default().contains("false"),
        "old key: the settings file is left as it was",
        &mut failures,
    );

    // A Windows exception.
    if cfg!(windows) {
        let dir = fresh("exception");
        let status = run("exception", &dir);
        check(!status.success(), "exception: the program ends hard", &mut failures);
        let dump = std::fs::read_to_string(dir.join(crash::FILE)).unwrap_or_default();
        check(
            dump.starts_with("Windows fatal exception: code 0xc0000005 at address 0x"),
            "exception: code and address in the dump",
            &mut failures,
        );
        // A raised exception takes the same way (not only a bad access).
        let dir = fresh("raise");
        let status = run("raise", &dir);
        check(!status.success(), "raise: the program ends hard", &mut failures);
        let dump = std::fs::read_to_string(dir.join(crash::FILE)).unwrap_or_default();
        check(
            dump.starts_with("Windows fatal exception: code 0xc0000094 at address 0x"),
            "raise: code and address in the dump",
            &mut failures,
        );
    }
    failures
}

fn counter_check() -> Vec<String> {
    // Without the wiring the same panic leaves nothing: the checks above can fail.
    let dir = fresh("ohne");
    run("without-init", &dir);
    let mut failures = Vec::new();
    check(std::fs::metadata(dir.join(crash::FILE)).is_err(), "counter: no crash file without init", &mut failures);
    check(errors_of(&dir).is_empty(), "counter: no error without init", &mut failures);
    check(!dir.join(trail::FILE).exists(), "counter: no trail without init", &mut failures);
    failures
}

fn main() {
    if let Ok(scenario) = std::env::var(SCENARIO) {
        child(&scenario);
        return;
    }
    let mut failures = parent();
    failures.extend(counter_check());
    if failures.is_empty() {
        println!("diagnostics_process: ok");
    } else {
        for failure in &failures {
            eprintln!("FEHLER {failure}");
        }
        std::process::exit(1);
    }
}
