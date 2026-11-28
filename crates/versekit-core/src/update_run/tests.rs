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

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;
use crate::testdir::Dir;

fn paths_in(name: &str) -> (Dir, Paths) {
    let folder = Dir::new(name);
    let text = folder.path().to_string_lossy().into_owned();
    (folder, Paths::new(Host::new(Flavor::native(), &[(crate::envvars::ENV_HOME, &text)], &text)))
}

fn mark(target: &str, previous: &str, start: f64) -> RunMark {
    RunMark { target: target.into(), previous: previous.into(), start, ..RunMark::default() }
}

/// Runs `work` in a thread of its own and waits at most `seconds` for it: a call that hangs (a virus scanner that
/// holds the fresh program file while the system starts it) fails the test with this message instead of hanging the
/// whole run for an hour. Only the tests bound the start of a program: the program itself waits as long as the system
/// does, as an installer that is started late may already be installing.
fn bounded<T: Send + 'static>(what: &str, seconds: u64, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(Duration::from_secs(seconds)).unwrap_or_else(|_| {
        panic!("{what} did not return within {seconds} s: the start of a program is held (a virus scanner?)")
    })
}

/// 14.11.2026: the test that starts a damaged installer opened a message box ("cannot be started because of an
/// incompatibility with 64-bit versions of Windows") that waits for a click, and the start waited with it. The
/// start of the installer runs with the flags that turn such boxes into an error, and puts the mode back. The flags
/// are checked here without starting anything.
#[cfg(windows)]
#[test]
fn the_start_of_a_program_runs_with_the_message_boxes_of_the_system_off() {
    use windows_sys::Win32::System::Diagnostics::Debug::{GetThreadErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX};
    // SAFETY: reads the error mode of this thread.
    let read = || unsafe { GetThreadErrorMode() };
    let before = read();
    let inside = without_dialogs(read);
    assert_eq!(
        inside & (SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX),
        SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
        "no message box while the process is created"
    );
    assert_eq!(read(), before, "the mode is put back");
    // The value comes through, also when it is an error.
    assert_eq!(without_dialogs(|| 7), 7);
}

/// The helper that bounds a call in a test ends with a clear message when the call hangs, and gives the value of a
/// call that returns.
#[test]
fn a_call_that_hangs_in_a_test_fails_with_a_clear_message_instead_of_hanging_the_run() {
    assert_eq!(bounded("quick", 30, || 7), 7);
    let started = Instant::now();
    let hung = std::panic::catch_unwind(|| {
        bounded("the start", 1, || std::thread::sleep(Duration::from_secs(5)));
    });
    let message = hung.expect_err("fails").downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("the start did not return within 1 s"), "{message}");
    assert!(started.elapsed() < Duration::from_secs(4), "did not wait for it: {:?}", started.elapsed());
}

/// A process number that is certainly not alive: a child that has ended. Its handle stays open while the value
/// lives, so the system does not hand the number to a new process meanwhile (many checks start processes).
struct DeadPid {
    pid: u32,
    _ended: std::process::Child,
}

fn dead_pid() -> DeadPid {
    let mut child = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "exit", "0"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(DETACHED_FLAGS);
        }
        c
    } else {
        std::process::Command::new("true")
    }
    .spawn()
    .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    DeadPid { pid, _ended: child }
}

#[test]
fn a_process_lives_or_not() {
    assert!(pid_alive(i64::from(std::process::id())));
    assert!(!pid_alive(i64::from(dead_pid().pid)));
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
    let dead = dead_pid();
    hand_lock_to(&paths, dead.pid, now);
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

/// Karasimsek, 5.2.10 → 5.3.0: the installer starts the new version before its loader (`VerseKit-Setup.exe`) has
/// ended, and a program file that still runs cannot be deleted (os error 5). That is no fault: the update worked,
/// the update log says the installer stays, and a later start removes it ([`remove_fetched`]).
#[cfg(windows)]
#[test]
fn an_installer_whose_loader_still_runs_at_the_start_after_it_stays_without_an_error() {
    let (dir, paths) = paths_in("lauf-lader-laeuft");
    let installer = dir.path().join("VerseKit-Setup.exe");
    fs::write(&installer, system_program("PING.EXE")).unwrap();
    let mut loader = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&installer).raw_arg(living(20)).creation_flags(DETACHED_FLAGS).spawn().unwrap()
    };
    let now = updater::now();
    begin_run(
        &paths,
        &RunMark { installer: installer.to_string_lossy().into_owned(), ..mark("v5.3.0", "5.2.10", now - 10.0) },
    )
    .unwrap();
    write_result(&paths, 0).unwrap();
    let (outcome, caught) = crate::errors::captured(|| evaluate(&paths, "5.3.0", now));
    let _ = loader.kill();
    let _ = loader.wait();
    assert_eq!(outcome.map(|o| o.kind), Some(Kind::Done));
    assert!(caught.iter().all(|c| c.note), "no error for an installer still in use: {caught:?}");
    assert!(read_run(&paths).is_none() && read_result(&paths).is_none(), "the rest is cleaned up");
    let log = fs::read_to_string(paths.app_file(LOG_FILE)).unwrap_or_default();
    assert!(log.contains("Installer VerseKit-Setup.exe is still in use - it goes at a later start"), "{log}");
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
fn the_installer_is_told_the_update_run_on_its_command_line() {
    let job = SetupJob {
        setup: PathBuf::from(r"C:\Users\x\AppData\Local\VerseKit\Update\VerseKit-Setup.exe"),
        checksum: "a".repeat(64),
        target_dir: PathBuf::from(r"C:\Pfad mit & ^ % ! ( ) ' Ümlaut"),
        setup_log: PathBuf::from(r"C:\d\update-setup.txt"),
        log: PathBuf::from(r"C:\d\Diagnose\update-helfer.txt"),
        result: PathBuf::from(r"C:\d\Intern\update-ergebnis.txt"),
        lock: PathBuf::from(r"C:\d\Intern\update-sperre.json"),
        wait_for: vec![4242, 77],
        at_exit: false,
    };
    assert_eq!(
        setup_arguments(&job),
        concat!(
            r#"/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR="C:\Pfad mit & ^ % ! ( ) ' Ümlaut" "#,
            r#"/LOG="C:\d\update-setup.txt" /VKUPDATE /VKPIDS=4242,77 /VKSPERRE="C:\d\Intern\update-sperre.json" "#,
            r#"/VKERGEBNIS="C:\d\Intern\update-ergebnis.txt" /VKPROTOKOLL="C:\d\Diagnose\update-helfer.txt""#
        )
    );
}

/// At the end of the program the installer shows no window and does not start the program again; the rest of the
/// command line stays the same.
#[test]
fn at_the_end_of_the_program_the_installer_runs_without_a_window_and_starts_nothing_again() {
    let job = SetupJob {
        target_dir: PathBuf::from(r"C:\Programme\VerseKit"),
        setup_log: PathBuf::from(r"C:\d\update-setup.txt"),
        log: PathBuf::from(r"C:\d\update-helfer.txt"),
        result: PathBuf::from(r"C:\d\update-ergebnis.txt"),
        lock: PathBuf::from(r"C:\d\update-sperre.json"),
        wait_for: vec![4242],
        ..SetupJob::default()
    };
    let running = setup_arguments(&job);
    let ended = setup_arguments(&SetupJob { at_exit: true, ..job });
    assert!(running.starts_with("/SILENT ") && !running.contains(NO_START_SWITCH), "{running}");
    assert!(ended.starts_with("/VERYSILENT ") && ended.ends_with(&format!(" {NO_START_SWITCH}")), "{ended}");
    assert_eq!(ended.replacen("/VERYSILENT", "/SILENT", 1).replace(&format!(" {NO_START_SWITCH}"), ""), running);
    // The installer knows the switch and starts nothing after it, but still frees the lock.
    let setup = repository_file("packaging/installer.iss");
    assert!(setup.contains(&format!("'{NO_START_SWITCH}'")), "the switch in installer.iss");
    let end = &setup[setup.find("procedure DeinitializeSetup").expect("the end of the installer")..];
    let (lock, no_start, start) = (end.find("DeleteFile(Path)"), end.find("if IsNoStart() then"), end.find("Exec(Exe"));
    assert!(
        lock < no_start && no_start < start && lock.is_some(),
        "lock freed, then no start: {lock:?} {no_start:?} {start:?}"
    );
}

/// A file of the repository, from the folder of this crate.
fn repository_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)).unwrap().replace("\r\n", "\n")
}

#[test]
fn the_check_of_the_delivery_starts_the_setup_as_the_program_does() {
    let script = repository_file("tools/ausliefern_pruefen.py");
    let start = script.find("UPDATE_ARGUMENTS = (").expect("the command line in the check");
    let block = &script[start..start + script[start..].find(")\n").expect("its end")];
    let template: String = block.split('\'').skip(1).step_by(2).collect();
    let job = SetupJob {
        target_dir: PathBuf::from(r"C:\Programme & ^ % ! ( ) ' Ümlaut"),
        setup_log: PathBuf::from(r"C:\Dokumente\VerseKit Ü\update-setup.txt"),
        lock: PathBuf::from(r"C:\Dokumente\VerseKit Ü\update-sperre.json"),
        result: PathBuf::from(r"C:\Dokumente\VerseKit Ü\update-ergebnis.txt"),
        log: PathBuf::from(r"C:\Dokumente\VerseKit Ü\update-helfer.txt"),
        wait_for: vec![4242],
        ..SetupJob::default()
    };
    let filled = [
        ("{ziel}", &job.target_dir),
        ("{setuplog}", &job.setup_log),
        ("{sperre}", &job.lock),
        ("{ergebnis}", &job.result),
        ("{protokoll}", &job.log),
    ]
    .iter()
    .fold(template.replace("{pids}", "4242"), |text, (field, path)| text.replace(field, &path.to_string_lossy()));
    assert_eq!(filled, setup_arguments(&job));
    assert!(script.contains(&format!("\nLIFE_SIGN = '{LIFE_SIGN}'\n")), "the sign of life in the check");
    // The update at the end of the program, as the check builds it from the same line.
    assert!(script.contains(&format!("\nNO_START = '{NO_START_SWITCH}'\n")), "the switch of the end in the check");
    assert!(
        script.contains("UPDATE_ARGUMENTS.replace('/SILENT', '/VERYSILENT', 1) + ' ' + NO_START"),
        "the check builds the line of the end"
    );
    let at_the_end = format!("{} {NO_START_SWITCH}", filled.replacen("/SILENT", "/VERYSILENT", 1));
    assert_eq!(at_the_end, setup_arguments(&SetupJob { at_exit: true, ..job.clone() }));
    // The installer: the same sign and the same names of the files it writes to or removes.
    let setup = repository_file("packaging/installer.iss");
    for (name, value) in
        [("LIFE_SIGN", LIFE_SIGN), ("LOCK_NAME", LOCK_FILE), ("RESULT_NAME", RESULT_FILE), ("LOG_NAME", LOG_FILE)]
    {
        assert!(setup.contains(&format!("\n  {name} = '{value}';\n")), "{name} in installer.iss");
    }
    assert!(setup.contains(&format!("'{UPDATE_SWITCH}'")), "the switch in installer.iss");
}

#[test]
fn the_installer_waits_for_this_process_and_reports_to_the_files_of_the_data_folder() {
    let (dir, paths) = paths_in("lauf-setupjob");
    let setup = dir.path().join("VerseKit-Setup.exe");
    fs::write(&setup, b"der Installer").unwrap();
    let downloaded = updater::Downloaded { checksum: updater::checksum_of(&setup).unwrap(), path: setup.clone() };
    let folder = dir.path().join("Programm");
    let job = setup_job(&paths, &downloaded, &folder.join("VerseKit.exe"), dir.path());
    assert_eq!(job.wait_for.first(), Some(&std::process::id()), "this process first");
    assert_eq!((job.setup, job.target_dir), (setup, folder));
    assert_eq!(job.log, PathBuf::from(paths.app_file(LOG_FILE)));
    assert_eq!(job.result, PathBuf::from(paths.app_file(RESULT_FILE)));
    assert_eq!(job.lock, PathBuf::from(paths.app_file(LOCK_FILE)));
    // The installer checks these names before it writes or removes anything.
    let names = [&job.log, &job.result, &job.lock].map(|p| p.file_name().unwrap().to_string_lossy().into_owned());
    assert_eq!(names, ["update-helfer.txt", "update-ergebnis.txt", "update-sperre.json"]);
}

/// Switches the message boxes of the system off for this whole test process (a damaged program file that is started
/// opens one that waits for a click: 14.11.2026 one stood on the screen of the person who tests). The program itself
/// does the same for the length of its start ([`without_dialogs`]); this is the belt for a test that goes wrong.
fn no_message_boxes() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode;
        // SAFETY: plain switches of the error mode of the process; what was there is kept.
        unsafe { SetErrorMode(SetErrorMode(0) | START_ERROR_MODE) };
    }
}

/// A throwaway data folder, an installer `content` under `lokal\VerseKit\Update`, a program folder and the job.
fn setup_case(name: &str, content: &[u8]) -> (Dir, Paths, SetupJob) {
    no_message_boxes();
    let (dir, paths) = paths_in(name);
    let folder = dir.path().join("lokal").join("VerseKit").join("Update");
    fs::create_dir_all(&folder).unwrap();
    let setup = folder.join("VerseKit-Setup.exe");
    fs::write(&setup, content).unwrap();
    let program = dir.path().join("Programm & ^ % ! ( ) ' Ümlaut").join("VerseKit.exe");
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::write(&program, b"alt").unwrap();
    let downloaded = updater::Downloaded { checksum: updater::checksum_of(&setup).unwrap(), path: setup };
    let job = setup_job(&paths, &downloaded, &program, dir.path());
    (dir, paths, job)
}

/// The contents of a copy of a program of the system, which opens no window.
#[cfg(windows)]
fn system_program(name: &str) -> Vec<u8> {
    let root = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
    fs::read(root.join("System32").join(name)).unwrap()
}

fn log_of(job: &SetupJob) -> String {
    fs::read_to_string(&job.log).unwrap_or_default()
}

/// Every program file below `folder`.
#[cfg(windows)]
fn program_files(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(program_files(&path));
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            out.push(path);
        }
    }
    out
}

/// Arguments on which a copy of `ping` lives for about `seconds`, without a window: the stand-in for a loader
/// whose installer runs.
#[cfg(windows)]
fn living(seconds: u32) -> String {
    format!("-n {} 127.0.0.1", seconds + 1)
}

/// Writes [`LIFE_SIGN`] into the update log of `job` as the installer does, once the run mark lies there and a
/// moment has passed. Before that it tries to rename the installer file: true when that worked (the file was not
/// held).
#[cfg(windows)]
fn sign_later(paths: &Paths, job: &SetupJob) -> std::thread::JoinHandle<bool> {
    let run = PathBuf::from(paths.app_file(RUN_FILE));
    let (setup, log) = (job.setup.clone(), job.log.clone());
    std::thread::spawn(move || {
        let started = Instant::now();
        while !run.exists() && started.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300));
        let renamed = fs::rename(&setup, setup.with_extension("weg")).is_ok();
        log_line(&log, LIFE_SIGN);
        renamed
    })
}

/// Ends the process of its number when it goes (a stand-in started by a check), so its file can go with the
/// throwaway folder.
#[cfg(windows)]
struct EndOnDrop(u32);

#[cfg(windows)]
impl Drop for EndOnDrop {
    fn drop(&mut self) {
        end_process(self.0);
    }
}

#[cfg(windows)]
fn end_process(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    // SAFETY: a handle opened here, used once and closed.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(windows)]
#[test]
fn the_installer_is_started_directly_and_the_program_steps_down_only_after_its_sign() {
    let (dir, paths, job) = setup_case("lauf-start", &system_program("PING.EXE"));
    let now = updater::now();
    assert!(take_lock(&paths, std::process::id(), now));
    let signer = sign_later(&paths, &job);
    // The stand-in of the installer lives two minutes, and its sign is waited for as long: on a loaded computer the sign comes seconds late, and a stand-in that
    // has ended by then counts as an installer that ended at once. It is ended when the check is done.
    let pid = launch(&paths, &job, &mark("v5.0.1", "5.0.0", now), Vec::new(), Duration::from_secs(120), &living(120))
        .expect("started");
    let _end = EndOnDrop(pid);
    assert!(!signer.join().unwrap(), "the installer file is held until its sign has come");
    let run = read_run(&paths).expect("the run mark");
    assert_eq!((run.target.as_str(), run.previous.as_str()), ("v5.0.1", "5.0.0"));
    let lock = fs::read_to_string(paths.app_file(LOCK_FILE)).unwrap();
    assert!(lock.contains(&format!("\"pid\":{pid}")), "the lock belongs to the installer: {lock}");
    let log = log_of(&job);
    assert!(
        log.contains(&format!(
            "Checksum matches, program starts the installer VerseKit-Setup.exe, which waits for PID {}",
            std::process::id()
        )),
        "{log}"
    );
    let (sign, runs) = (log.find(LIFE_SIGN).unwrap(), log.find(&format!("Installer runs (PID {pid})")).unwrap());
    assert!(sign < runs, "the program steps down after the sign: {log}");
    // No copy of a program file anywhere: only the installer and the program.
    let mut found = program_files(dir.path());
    found.sort();
    let mut expected = vec![job.setup.clone(), job.target_dir.join("VerseKit.exe")];
    expected.sort();
    assert_eq!(found, expected);
}

#[cfg(windows)]
#[test]
fn an_installer_that_ends_before_its_sign_hands_nothing_over_and_the_program_stays() {
    // On the arguments of an update `ping` ends at once with an error.
    let (_dir, paths, job) = setup_case("lauf-sofort", &system_program("PING.EXE"));
    let now = updater::now();
    assert!(take_lock(&paths, std::process::id(), now));
    let (own, job_now) = (Paths::new(paths.host.clone()), job.clone());
    let error = bounded("start_setup", 180, move || {
        start_setup(&own, &job_now, &mark("v5.0.1", "5.0.0", now), Vec::new(), Duration::from_secs(10))
    })
    .expect_err("ended at once");
    assert!(error.to_string().contains("ended at once with exit code 1"), "{error}");
    assert!(read_run(&paths).is_none(), "no run mark: the program runs on");
    let lock = fs::read_to_string(paths.app_file(LOCK_FILE)).unwrap();
    assert!(lock.contains(&format!("\"pid\":{}", std::process::id())), "the lock stays with the program: {lock}");
    let log = log_of(&job);
    assert!(log.contains("Installer ended before it ran, exit code 1 - nothing installed"), "{log}");
    assert!(!job.setup.exists(), "the installer goes");
}

#[cfg(windows)]
#[test]
fn an_installer_without_a_sign_is_ended_and_the_program_stays() {
    let (_dir, paths, job) = setup_case("lauf-stumm", &system_program("PING.EXE"));
    let now = updater::now();
    assert!(take_lock(&paths, std::process::id(), now));
    let started = Instant::now();
    // The loader would live 30 s: ended well before that, the wait was its own 1 s and not the life of the loader.
    let error = launch(&paths, &job, &mark("v5.0.1", "5.0.0", now), Vec::new(), Duration::from_secs(1), &living(30))
        .expect_err("no sign");
    let took = started.elapsed();
    assert!(error.to_string().contains("gave no sign within 1 s"), "{error}");
    assert!(took >= Duration::from_secs(1) && took < Duration::from_secs(20), "waited its time: {took:?}");
    assert!(read_run(&paths).is_none(), "no run mark");
    assert!(log_of(&job).contains("Installer gave no sign within 1 s - stopped, nothing installed"), "{}", log_of(&job));
    // A program file that still runs cannot be deleted: the loader was ended.
    assert!(!job.setup.exists(), "the loader is ended and its file goes");
}

#[cfg(windows)]
#[test]
fn the_wait_for_the_sign_watches_the_loader_and_the_log_for_its_whole_time() {
    use std::os::windows::process::CommandExt;
    let dir = Dir::new("lauf-zeichen");
    let log = dir.path().join("update-helfer.txt");
    // The sign of an earlier run lies before the offset and does not count.
    log_line(&log, LIFE_SIGN);
    log_line(&log, "Program starts the installer");
    let offset = fs::metadata(&log).unwrap().len();
    let ping = |seconds: u32| {
        let root = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
        std::process::Command::new(root.join(r"System32\PING.EXE"))
            .raw_arg(living(seconds))
            .creation_flags(DETACHED_FLAGS)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    };
    // No sign: the whole time, not less, and not the life of the loader (30 s).
    let mut child = ping(30);
    let started = Instant::now();
    assert_eq!(await_life_sign(&mut child, &log, offset, Duration::from_millis(700)), Sign::Silent);
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(700) && took < Duration::from_secs(20), "{took:?}");
    // A sign after the offset counts as soon as it comes.
    let writer = {
        let log = log.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            log_line(&log, LIFE_SIGN);
        })
    };
    let started = Instant::now();
    // As soon as it comes: the wait would last 25 s otherwise.
    assert_eq!(await_life_sign(&mut child, &log, offset, Duration::from_secs(25)), Sign::Started);
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
    writer.join().unwrap();
    assert!(!signed(&log, fs::metadata(&log).unwrap().len()), "nothing after the end");
    let _ = child.kill();
    let _ = child.wait();
    // A loader that has ended counts as ended, even with its sign in the log.
    let mut ended = std::process::Command::new("cmd")
        .args(["/c", "exit", "3"])
        .creation_flags(DETACHED_FLAGS)
        .spawn()
        .unwrap();
    let _ = ended.wait();
    assert_eq!(await_life_sign(&mut ended, &log, offset, Duration::from_secs(5)), Sign::Ended(3));
}

#[test]
fn an_installer_that_does_not_start_hands_nothing_over() {
    // Matches its checksum, but is no program file.
    let (_dir, paths, job) = setup_case("lauf-startet-nicht", b"kein Programm");
    let (own, job_now) = (Paths::new(paths.host.clone()), job.clone());
    let error = bounded("start_setup", 180, move || {
        start_setup(&own, &job_now, &mark("v5.0.1", "5.0.0", updater::now()), Vec::new(), Duration::ZERO)
    })
    .expect_err("no start");
    assert!(read_run(&paths).is_none());
    assert!(log_of(&job).contains(&format!("Installer did not start: {error}")), "{}", log_of(&job));
    assert!(!job.setup.exists());
}

#[test]
fn the_checksum_is_compared_right_before_the_start() {
    let (_dir, paths, job) = setup_case("lauf-summe", b"der Installer");
    // Changed after the download and after the check before the backup.
    fs::write(&job.setup, b"ausgetauscht").unwrap();
    let error = start_setup(&paths, &job, &mark("v5.0.1", "5.0.0", updater::now()), Vec::new(), Duration::ZERO)
        .expect_err("refused");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(read_run(&paths).is_none(), "no run mark");
    assert!(!job.setup.exists(), "the changed file goes");
    let log = log_of(&job);
    assert!(log.contains("Checksum mismatch in VerseKit-Setup.exe - file discarded, nothing installed"), "{log}");
    assert!(!log.contains("starts the installer"), "{log}");
}

#[cfg(windows)]
#[test]
fn an_installer_that_cannot_be_read_for_a_moment_is_tried_again_and_never_deleted() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_dir, paths, job) = setup_case("lauf-gesperrt", b"der Installer");
    // Somebody holds the file longer than the tries last: nobody else may read it, but deleting it would work.
    const FILE_SHARE_DELETE: u32 = 4;
    let blocked = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_DELETE).open(&job.setup).unwrap();
    let started = Instant::now();
    let error = start_setup(&paths, &job, &mark("v5.0.1", "5.0.0", updater::now()), Vec::new(), Duration::ZERO)
        .expect_err("cannot be read");
    assert!(started.elapsed() >= Duration::from_millis(800), "tried again: {:?}", started.elapsed());
    drop(blocked);
    assert_ne!(error.kind(), io::ErrorKind::InvalidData);
    assert!(job.setup.exists(), "a file that could only not be read stays");
    assert!(read_run(&paths).is_none());
    assert!(log_of(&job).contains("could not be read - nothing installed"), "{}", log_of(&job));
    // Free again within the tries: it starts its check as usual.
    let blocked = fs::OpenOptions::new().read(true).share_mode(0).open(&job.setup).unwrap();
    let freed = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(blocked);
    });
    assert!(hold_checked_retrying(&job.setup, &job.checksum).is_ok());
    freed.join().unwrap();
}

#[cfg(windows)]
#[test]
fn the_installer_waits_for_every_other_process_of_the_program_file_too() {
    use std::os::windows::process::CommandExt;
    let (dir, paths) = paths_in("lauf-andere");
    let program = dir.path().join("Programm").join("VerseKit.exe");
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::write(&program, system_program("PING.EXE")).unwrap();
    let mut other = std::process::Command::new(&program)
        .raw_arg(living(30))
        .creation_flags(DETACHED_FLAGS)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let setup = dir.path().join("VerseKit-Setup.exe");
    fs::write(&setup, b"der Installer").unwrap();
    let downloaded = updater::Downloaded { checksum: updater::checksum_of(&setup).unwrap(), path: setup };
    let job = setup_job(&paths, &downloaded, &program, dir.path());
    let _ = other.kill();
    let _ = other.wait();
    assert_eq!(job.wait_for, [std::process::id(), other.id()]);
    // The own process stands once, also when the program file is the running one.
    // (A second run of the same checks at the same time runs from that file too and is waited for as well.)
    let own = setup_job(&paths, &downloaded, &std::env::current_exe().unwrap(), dir.path());
    assert_eq!(own.wait_for.first(), Some(&std::process::id()), "{:?}", own.wait_for);
    assert_eq!(own.wait_for.iter().filter(|pid| **pid == std::process::id()).count(), 1, "{:?}", own.wait_for);
}

#[test]
fn what_earlier_updates_fetched_goes_at_the_start_but_not_a_run_under_way_or_a_fresh_download() {
    let dir = Dir::new("lauf-geholt");
    let folder = dir.path().join("VerseKit").join("Update");
    fs::create_dir_all(&folder).unwrap();
    let now = updater::now();
    let aged = |name: &str| {
        let path = folder.join(name);
        fs::write(&path, b"x").unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        fs::OpenOptions::new().write(true).open(&path).unwrap().set_modified(old).unwrap();
        path
    };
    let left = aged("VerseKit-Setup.exe");
    let other = aged("alt.exe");
    let running = aged("VerseKit-Setup (2).exe");
    let fresh = folder.join("frisch.exe");
    fs::write(&fresh, b"x").unwrap();
    remove_fetched(&folder, Some(&running), now);
    assert!(!left.exists() && !other.exists(), "what is left over goes");
    assert!(running.exists(), "the installer of a run under way stays");
    assert!(fresh.exists(), "a download that may be coming in stays");
    remove_fetched(&dir.path().join("fehlt"), None, now);
}

#[cfg(windows)]
#[test]
fn nobody_changes_or_deletes_the_installer_while_it_is_held_for_its_start() {
    let (_dir, _paths, job) = setup_case("lauf-gehalten", b"der Installer");
    let held = hold_checked(&job.setup, &job.checksum).expect("matches");
    assert!(fs::write(&job.setup, b"ausgetauscht").is_err(), "not changed");
    assert!(fs::remove_file(&job.setup).is_err(), "not deleted");
    assert!(fs::read(&job.setup).is_ok(), "but read");
    drop(held);
    assert!(fs::write(&job.setup, b"danach").is_ok());
    assert!(hold_checked(&job.setup, &job.checksum).is_err(), "another content does not match");
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

#[cfg(windows)]
#[test]
fn the_process_list_finds_the_running_test_program_by_its_file() {
    let own = std::env::current_exe().unwrap();
    // This very test runs from that file, and the own process does not count. (A second run of the same checks at
    // the same time is a process of that file too, so only the own number is looked for.)
    let found = processes_of(&own).expect("the list");
    assert!(!found.contains(&std::process::id()), "{found:?}");
    assert_eq!(processes_of(&own.with_file_name("gibt-es-nicht.exe")), Some(Vec::new()));
    // A process of another file is found by that file, and only of that file: a copy of `ping` that waits a
    // few seconds, without a window.
    let dir = Dir::new("lauf-liste");
    let copy = dir.path().join("versekit-liste-test.exe");
    fs::write(&copy, system_program("PING.EXE")).unwrap();
    let mut child = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&copy)
            .args(["-n", "30", "127.0.0.1"])
            .creation_flags(DETACHED_FLAGS)
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

#[test]
fn the_pause_after_failed_downloads_doubles_up_to_an_hour() {
    let minutes: Vec<f64> = (0..=7).map(|n| pause_after(n) / 60.0).collect();
    assert_eq!(minutes, [10.0, 10.0, 20.0, 40.0, 60.0, 60.0, 60.0, 60.0]);
    assert_eq!(pause_after(u32::MAX), PAUSE_MAX);
}

#[test]
fn the_pause_holds_in_the_data_folder_ends_by_itself_and_never_for_days() {
    let (_dir, paths) = paths_in("update-pause");
    let now = 1_800_000_000.0;
    let v = "v5.0.1";
    assert_eq!(download_pause_left(&paths, v, now), None);
    assert_eq!(remember_download_failure(&paths, v, now, true), Some(PAUSE_FIRST));
    // Another process reads the same file, and the version is compared as the tag would write it.
    let other = Paths::new(paths.host.clone());
    assert_eq!(download_pause_left(&other, "5.0.1 ", now + 60.0), Some(PAUSE_FIRST - 60.0));
    assert_eq!(download_pause_left(&other, v, now + PAUSE_FIRST), None);
    assert_eq!(remember_download_failure(&other, v, now + PAUSE_FIRST, true), Some(2.0 * PAUSE_FIRST));
    // A clock set back by a day ends the pause instead of holding it.
    assert_eq!(download_pause_left(&paths, v, now - 86_400.0), None);
    // A broken file is no pause.
    fs::write(paths.app_file(PAUSE_FILE), b"{kaputt").unwrap();
    assert_eq!(download_pause_left(&paths, v, now), None);
    assert_eq!(remember_download_failure(&paths, v, now, true), Some(PAUSE_FIRST), "counts from one again");
    clear_download_failures(&paths);
    assert_eq!(download_pause_left(&paths, v, now), None);
}

#[test]
fn the_pause_holds_only_for_the_version_that_failed() {
    let (_dir, paths) = paths_in("update-pause-version");
    let now = 1_800_000_000.0;
    remember_download_failure(&paths, "v5.0.1", now, true);
    remember_download_failure(&paths, "v5.0.1", now + 1.0, true);
    assert_eq!(download_pause_left(&paths, "v5.0.1", now + 1.0), Some(2.0 * PAUSE_FIRST));
    // A newer version offered meanwhile fetches at once.
    assert_eq!(download_pause_left(&paths, "v5.0.2", now + 1.0), None);
    // Its own failure counts from one.
    assert_eq!(remember_download_failure(&paths, "v5.0.2", now + 2.0, true), Some(PAUSE_FIRST));
}

#[test]
fn the_count_starts_again_after_an_hour_without_a_failure() {
    let (_dir, paths) = paths_in("update-pause-verfall");
    let now = 1_800_000_000.0;
    for n in 0..4 {
        remember_download_failure(&paths, "v5.0.1", now + f64::from(n), true);
    }
    assert_eq!(download_pause_left(&paths, "v5.0.1", now + 3.0), Some(PAUSE_MAX));
    // The last failure is longer ago than the longest pause: the next one is a first one again.
    let later = now + 3.0 + PAUSE_MAX + 1.0;
    assert_eq!(remember_download_failure(&paths, "v5.0.1", later, true), Some(PAUSE_FIRST));
}

#[test]
fn a_failed_click_waits_half_a_minute_but_does_not_lengthen_the_automatic_pause() {
    let (_dir, paths) = paths_in("update-pause-klick");
    let now = 1_800_000_000.0;
    // Clicks alone: half a minute for the next click, no pause for the automatic update.
    for n in 0..5 {
        assert_eq!(remember_download_failure(&paths, "v5.0.1", now + f64::from(n) * 40.0, false), None);
    }
    let last = now + 160.0;
    assert_eq!(manual_wait_left(&paths, last + 10.0), Some(MANUAL_PAUSE - 10.0));
    assert_eq!(download_pause_left(&paths, "v5.0.1", last + 10.0), None);
    // An automatic failure counts, clicks after it do not add to it.
    assert_eq!(remember_download_failure(&paths, "v5.0.1", last + 20.0, true), Some(PAUSE_FIRST));
    assert_eq!(remember_download_failure(&paths, "v5.0.1", last + 80.0, false), Some(PAUSE_FIRST));
    assert_eq!(remember_download_failure(&paths, "v5.0.1", last + 140.0, false), Some(PAUSE_FIRST));
    assert_eq!(download_pause_left(&paths, "v5.0.1", last + 140.0), Some(PAUSE_FIRST));
}

#[test]
fn a_new_version_that_started_ends_the_pause() {
    let (_dir, paths) = paths_in("update-pause-neustart");
    let now = 2_000_000.0;
    remember_download_failure(&paths, "v5.0.1", now - 120.0, true);
    begin_run(&paths, &mark("v5.0.1", "5.0.0", now - 60.0)).unwrap();
    write_result(&paths, 0).unwrap();
    assert_eq!(evaluate(&paths, "5.0.1", now).map(|o| o.kind), Some(Kind::Done));
    assert!(!Path::new(&paths.app_file(PAUSE_FILE)).exists(), "the pause file is gone");
    // An update that did not arrive keeps it.
    remember_download_failure(&paths, "v5.0.2", now - 120.0, true);
    begin_run(&paths, &mark("v5.0.2", "5.0.1", now - 60.0)).unwrap();
    write_result(&paths, 1).unwrap();
    assert_eq!(evaluate(&paths, "5.0.1", now).map(|o| o.kind), Some(Kind::Failed));
    assert!(download_pause_left(&paths, "v5.0.2", now).is_some());
}
