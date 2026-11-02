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
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::testdir::Dir;

fn paths_in(name: &str) -> (Dir, Paths) {
    let folder = Dir::new(name);
    let text = folder.path().to_string_lossy().into_owned();
    (folder, Paths::new(Host::new(Flavor::native(), &[(legacy::ENV_HOME, &text)], &text)))
}

fn mark(target: &str, previous: &str, start: f64) -> RunMark {
    RunMark { target: target.into(), previous: previous.into(), start, ..RunMark::default() }
}

/// A process number that is certainly not alive: a child that has ended.
fn dead_pid() -> u32 {
    let mut child = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "exit", "0"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(HELPER_FLAGS);
        }
        c
    } else {
        std::process::Command::new("true")
    }
    .spawn()
    .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn a_process_lives_or_not() {
    assert!(pid_alive(i64::from(std::process::id())));
    assert!(!pid_alive(i64::from(dead_pid())));
    assert!(!pid_alive(0) && !pid_alive(-5) && !pid_alive(i64::from(u32::MAX) + 1));
}

#[test]
fn the_lock_lets_one_run_through_and_an_orphaned_one_goes() {
    let (_dir, paths) = paths_in("lauf-sperre");
    let now = 1_000_000.0;
    assert!(take_lock(&paths, std::process::id(), now));
    assert!(!take_lock(&paths, std::process::id(), now + 1.0), "a second click while it runs");
    assert!(lock_held(&paths, now + 1.0));
    // Too old, although the process lives.
    assert!(!lock_held(&paths, now + LOCK_MAX_AGE + 1.0));
    assert!(take_lock(&paths, std::process::id(), now + LOCK_MAX_AGE + 1.0));
    // A process that has ended.
    hand_lock_to(&paths, dead_pid(), now);
    assert!(!lock_held(&paths, now));
    assert!(take_lock(&paths, std::process::id(), now));
    release_lock(&paths);
    assert!(!lock_held(&paths, now));
    // Broken and old: orphaned.
    fs::write(paths.app_file(LOCK_FILE), b"{kaputt").unwrap();
    assert!(!lock_held(&paths, updater::now() + 60.0));
}

#[test]
fn the_start_after_an_update_tells_what_became_of_it() {
    let (_dir, paths) = paths_in("lauf-auswertung");
    let now = 2_000_000.0;
    let case = |m: RunMark, code: Option<i32>, own: &str| {
        begin_run(&paths, &m).unwrap();
        if let Some(code) = code {
            write_result(&paths, code).unwrap();
        }
        let outcome = evaluate(&paths, own, now);
        assert!(read_run(&paths).is_none() && read_result(&paths).is_none(), "cleaned up");
        outcome.map(|o| o.kind)
    };
    // ⚠ The version decides, not the exit code.
    assert_eq!(case(mark("v5.0.1", "5.0.0", now - 60.0), Some(0), "5.0.1"), Some(Kind::Done));
    assert_eq!(
        case(mark("v5.0.1", "5.0.0", now - 60.0), Some(0), "5.0.0"),
        Some(Kind::Failed),
        "0, but the old one runs"
    );
    assert_eq!(case(mark("v5.0.1", "5.0.0", now - 60.0), None, "5.0.0"), Some(Kind::Unclear));
    assert_eq!(case(mark("v5.0.1", "5.0.0", now - 60.0), Some(5), "5.0.0"), Some(Kind::Cancelled));
    assert_eq!(case(mark("v5.0.1", "5.0.0", now - 60.0), Some(90), "5.0.0"), Some(Kind::Failed));
    assert_eq!(case(mark("v5.0.1", "5.0.0", now - RUN_MAX_AGE - 1.0), Some(0), "5.0.1"), None, "too old: no word");
    assert_eq!(evaluate(&paths, "5.0.1", now), None, "nothing to say");
}

#[test]
fn the_mark_the_helper_of_4_12_left_is_read_and_its_installer_removed() {
    let (dir, paths) = paths_in("lauf-412");
    let installer = dir.path().join("VerseKit-Setup.exe");
    let foreign = dir.path().join("fremd.exe");
    fs::write(&installer, b"x").unwrap();
    fs::write(&foreign, b"x").unwrap();
    // As 4.12 writes it (`json.dump` of its dictionary) and its helper the exit code (`echo 0`).
    let start = updater::now() - 30.0;
    fs::write(
        paths.app_file(RUN_FILE),
        format!(
            r#"{{"ziel": "v5.0.0", "alt": "4.12.2", "installer": {}, "sha256": "{}", "start": {start}, "automatisch": true, "fenster": false}}"#,
            serde_json::to_string(&installer.to_string_lossy()).unwrap(),
            "a".repeat(64)
        ),
    )
    .unwrap();
    fs::write(paths.app_file(RESULT_FILE), b"0 \r\n").unwrap();
    let outcome = evaluate(&paths, "5.0.0", updater::now()).expect("an outcome");
    assert_eq!((outcome.kind, outcome.automatic, outcome.from_previous_program()), (Kind::Done, true, true));
    assert!(!installer.exists(), "our installer goes");
    // A foreign file named in the mark stays.
    begin_run(
        &paths,
        &RunMark { installer: foreign.to_string_lossy().into_owned(), start: updater::now(), ..RunMark::default() },
    )
    .unwrap();
    let _ = evaluate(&paths, "5.0.0", updater::now());
    assert!(foreign.exists(), "never anything foreign");
}

#[test]
fn while_the_helper_holds_the_lock_the_start_says_nothing_and_the_mark_stays() {
    let (_dir, paths) = paths_in("lauf-sperre-haelt");
    let now = updater::now();
    begin_run(&paths, &mark("v5.0.1", "5.0.0", now)).unwrap();
    hand_lock_to(&paths, std::process::id(), now);
    assert_eq!(evaluate(&paths, "5.0.0", now), None);
    assert!(read_run(&paths).is_some());
}

#[test]
fn a_failed_automatic_update_waits_a_day_and_opens_the_window() {
    let (_dir, paths) = paths_in("lauf-fehlschlag");
    let now = 3_000_000.0;
    let failed = Outcome {
        kind: Kind::Failed,
        target: "v5.0.1".into(),
        previous: "5.0.0".into(),
        code: Some(1),
        own: "5.0.0".into(),
        automatic: true,
        show_window: false,
    };
    assert!(remember_failure(&paths, &failed, now));
    assert!(failed_recently(&paths, "5.0.1", now + 10.0));
    assert!(failed_recently(&paths, "V5.0.1 ", now + 10.0), "the same version, written otherwise");
    assert!(!failed_recently(&paths, "5.0.2", now + 10.0));
    assert!(!failed_recently(&paths, "5.0.1", now + FAILED_PAUSE + 1.0));
    let done = Outcome { kind: Kind::Done, ..failed.clone() };
    assert!(!remember_failure(&paths, &done, now), "only failures");
    assert!(!remember_failure(&paths, &Outcome { automatic: false, ..failed.clone() }, now), "only automatic ones");
    // The window: after a click, right after the start, and when it failed; not after a quiet success.
    assert!(should_open_window(Some(&failed)));
    assert!(!should_open_window(Some(&done)));
    assert!(should_open_window(Some(&Outcome { show_window: true, ..done.clone() })));
    assert!(should_open_window(Some(&Outcome { automatic: false, ..done.clone() })));
    assert!(!should_open_window(None));
    for o in [&failed, &done] {
        assert!(!o.message().is_empty() && !o.message().starts_with("up_"), "{}", o.message());
    }
    assert!(failed.message().contains("v5.0.1") && failed.message().contains('1'));
}

#[test]
fn the_job_travels_through_the_environment_and_back() {
    let job = HelperJob {
        setup: PathBuf::from(r"C:\Temp\VerseKit-Setup.exe"),
        checksum: "A".repeat(64),
        target_dir: PathBuf::from(r"C:\Pfad mit & ^ % ! ( ) ' Ümlaut"),
        setup_log: PathBuf::from(r"C:\d\update-setup.txt"),
        log: PathBuf::from(r"C:\d\update-helfer.txt"),
        result: PathBuf::from(r"C:\d\update-ergebnis.txt"),
        lock: PathBuf::from(r"C:\d\update-sperre.json"),
        program: PathBuf::from(r"C:\Pfad mit & ^ % ! ( ) ' Ümlaut\SC-BP-Watcher.exe"),
        pid: 4242,
        program_key: PROGRAM_KEY.into(),
        wait: 60,
    };
    let env = job.to_env();
    assert!(env.iter().all(|(k, _)| k.starts_with(ENV_PREFIX)));
    let back = HelperJob::from_env(&env).expect("complete");
    assert_eq!(back, HelperJob { checksum: "a".repeat(64), ..job.clone() });
    // A missing part or a checksum of the wrong length: no job.
    let without: Vec<_> = env.iter().filter(|(k, _)| !k.ends_with("_SETUP")).cloned().collect();
    assert_eq!(HelperJob::from_env(&without), None);
    let short: Vec<_> = env
        .iter()
        .map(|(k, v)| if k.ends_with("_SHA256") { (k.clone(), "ab".into()) } else { (k.clone(), v.clone()) })
        .collect();
    assert_eq!(HelperJob::from_env(&short), None);
    // The job never reaches the programs the helper starts.
    assert!(updater::clean_environment(env).is_empty());
}

#[test]
fn a_setup_log_that_cannot_go_moves_to_the_temporary_folder() {
    let dir = Dir::new("lauf-setuplog");
    let wanted = dir.path().join("update-setup.txt");
    assert_eq!(free_setup_log(&wanted, dir.path()), wanted, "free");
    fs::write(&wanted, b"alt").unwrap();
    assert_eq!(free_setup_log(&wanted, dir.path()), wanted, "the old one is removed");
    assert!(!wanted.exists());
    // A folder of that name cannot be removed as a file: the second place.
    fs::create_dir_all(&wanted).unwrap();
    assert_eq!(free_setup_log(&wanted, dir.path()), dir.path().join("versekit-update-setup.txt"));
}

#[test]
fn the_leftovers_of_an_update_in_the_temporary_folder_go() {
    let dir = Dir::new("lauf-reste");
    for name in [legacy::UPDATE_HELPER_SCRIPT, HELPER_NAME, "VerseKit-Setup.exe", "fremd.cmd"] {
        fs::write(dir.path().join(name), b"x").unwrap();
    }
    remove_leftovers(dir.path());
    assert!(!dir.path().join(legacy::UPDATE_HELPER_SCRIPT).exists() && !dir.path().join(HELPER_NAME).exists());
    assert!(dir.path().join("fremd.cmd").exists(), "nothing foreign");
    assert!(dir.path().join("VerseKit-Setup.exe").exists(), "the installer goes only through the run mark");
}

#[test]
fn the_helpers_log_keeps_the_try_before() {
    let (_dir, paths) = paths_in("lauf-protokoll");
    let log = PathBuf::from(paths.app_file(LOG_FILE));
    log_line(&log, "first");
    rotate_log(&paths);
    log_line(&log, "second");
    let old = fs::read_to_string(paths.app_file(LOG_FILE_OLD)).unwrap();
    let new = fs::read_to_string(&log).unwrap();
    assert!(old.ends_with(" first\r\n") && new.ends_with(" second\r\n") && !new.contains("first"), "{old:?} {new:?}");
}
