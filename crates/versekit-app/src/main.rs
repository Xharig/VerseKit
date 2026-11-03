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

//! Verse-Kit: ONE program file for the overlay and the main window, as 4.12 is one program.
//!
//! A start without a switch runs the overlay above the game on the user's own data. The overlay starts this
//! same file again as the main window (`--hauptfenster`) when the player asks for it (the gear, the tray icon,
//! the hotkey, a second start of the program). A second start of the program asks the running overlay for the
//! main window and ends; the overlay carries the single instance.
//!
//! `--daten <folder>` runs everything on a copy of the data (writing inside `<folder>` only), as the overlay
//! and the preview do; without `--daten` the program reads and writes the user's own files. All other
//! switches are those of the overlay (see the library `versekit_overlay`).
//!
//! The decision is made before anything is built: the main window and the overlay each need the main thread
//! of a process for their own window loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod first_backup_window;
mod learn_window;
mod main_window;
mod updates;

use first_backup_window::Gate;

/// The switch that makes this process the main window; the overlay passes it when it starts the program.
const MAIN_WINDOW: &str = "--hauptfenster";
/// The switch that makes this process the small window for teaching the digits; the overlay passes it.
const LEARN_WINDOW: &str = "--anlernen";
/// The main window of a start that is not set up yet: it opens with the assistant and starts the overlay when
/// the assistant has run to its end.
pub(crate) const FIRST_START: &str = "--erster-start";
/// The overlay the main window starts (the end of the assistant, a restart): the main window holds the single
/// instance then, and the overlay goes on beside it instead of bringing it to the front.
pub(crate) const BY_MAIN_WINDOW: &str = "--vom-hauptfenster";

/// Does the assistant have to run before anything else (`versekit_core::setup::needed`)? On a copy of the data
/// writing is locked to the copy first: asking whether the data folder can be written writes a trial file.
fn first_start(data: Option<&str>) -> bool {
    run_paths(data).is_some_and(|p| versekit_core::setup::needed(&p))
}

/// The paths of the run: the copy of the data after `--daten` (writing locked to it first), else the user's own.
fn run_paths(data: Option<&str>) -> Option<versekit_core::paths::Paths> {
    use versekit_core::paths::{Host, Paths};
    match data {
        Some(root) => {
            versekit_core::writeguard::deny(true);
            versekit_core::writeguard::allow_under(Some(root));
            Paths::for_copy(Host::current(), root)
        }
        None => Some(Paths::new(Host::current())),
    }
}

/// Copies the files an older version kept in the hidden folder into a data folder without a collection, and
/// brings own files dragged into the data folder by hand to their places (`Paths::migrate`,
/// `Paths::tidy_storage`), removes the explanatory lines of the settings file
/// (`Paths::drop_explanation_lines`) and moves the choice of a translation that is no longer offered to the one
/// that takes its place (`translation::retire_sources`). Returns the message about the copied files, if any were
/// copied.
fn take_in_older_files(data: Option<&str>) -> Option<String> {
    use versekit_core::language::{Arg, tf};
    let paths = run_paths(data)?;
    let copied = if paths.migration_needed() { paths.migrate() } else { 0 };
    paths.tidy_storage(None);
    paths.drop_explanation_lines();
    versekit_core::translation::retire_sources(&paths);
    (copied > 0).then(|| tf("umzug_fertig", &[("0", Arg::from(copied)), ("1", Arg::Text(paths.app_folder()))]))
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The update helper (a copy of this file in the temporary folder) is no start of the program: it does its
    // job and ends, before the single instance or anything else is asked.
    if args.first().map(String::as_str) == Some(versekit_core::update_run::HELPER_SWITCH) {
        run_update_helper(std::env::vars().collect());
        return Ok(());
    }
    // ⚠ A `--daten` that does not arrive (misspelled, no value) must not turn into a run on the player's own
    // data: the start with the system and the real files would be touched by a test run.
    if refuses(&args) {
        return Ok(());
    }
    // One program per data folder: a start on another copy of the data is a program of its own, and neither
    // finds the windows of the other.
    let home = std::env::var(versekit_core::legacy::ENV_HOME).ok();
    versekit_core::window_signal::bind(&run_key(&args, home.as_deref()));
    // The windows the overlay starts come after its backup of the first start; one started by hand gets it too.
    let windows = [MAIN_WINDOW, LEARN_WINDOW];
    if args.iter().any(|a| windows.contains(&a.as_str())) {
        let gate = || first_backup_window::gate(data_folder(&args), env!("CARGO_PKG_VERSION"));
        let diagnose = || {
            if let Some(role) = role_of(&args) {
                start_diagnostics(role, data_folder(&args));
            }
        };
        if window_start(gate, diagnose) == Gate::Quit {
            return Ok(());
        }
    }
    if args.iter().any(|a| a == MAIN_WINDOW) {
        return main_window::run(&args);
    }
    // The small window for teaching the digits, which the overlay opens above the game.
    if args.iter().any(|a| a == LEARN_WINDOW) {
        return learn_window::run(&args);
    }
    let by_main_window = args.iter().any(|a| a == BY_MAIN_WINDOW);
    let role = role_of(&args);
    let mut overlay = versekit_overlay::Args::parse(args);
    // One run on this desktop only: a start while the program runs hands over (the overlay brings up the main
    // window) and ends, also while the setup assistant or the window of the backup of a first start is open and the
    // overlay not there yet (that window comes to the front); a window that does not answer is told to the player.
    if overlay.probe.is_none() && versekit_overlay::already_running(by_main_window) {
        return Ok(());
    }
    // ⚠⚠ Before anything of this program writes into the data (the diagnostics, the assistant's look whether the
    // folder can be written, the settings, the start with the system, the overlay): a backup of what the older
    // version left. The self-check has its own data and is left out. Only the start that goes on sets up its
    // diagnostics: a start that hands over must not set aside the files of the running overlay.
    let check = overlay.probe.is_some();
    let data = overlay.data.clone();
    let mut taken_in = None;
    let start = overlay_start(
        check,
        || first_backup_window::gate(data.as_deref(), env!("CARGO_PKG_VERSION")),
        || {
            // After the backup and before the look whether the program is set up: the data of an older version
            // that lie elsewhere, and files dragged in by hand, are where the program reads them.
            taken_in = take_in_older_files(data.as_deref());
            first_start(data.as_deref())
        },
        || {
            if let Some(role) = role {
                start_diagnostics(role, data.as_deref());
            }
            // What the look for a running start noted, now that the trail takes it (the assistant too).
            versekit_overlay::trail_start_notes();
        },
    );
    let backed_up = match start {
        OverlayStart::Quit => return Ok(()),
        // A program that is not set up yet starts with the assistant in the main window (as 4.12 does before the
        // overlay is built); the overlay follows when the assistant has run to its end.
        OverlayStart::Assistant => {
            let mut own = main_window_args(overlay.data.as_deref());
            own.push(FIRST_START.to_string());
            return main_window::run(&own);
        }
        OverlayStart::Overlay { backed_up } => backed_up,
    };
    if taken_in.is_some() {
        overlay.notice = taken_in;
    }
    if begin_own_run(
        &mut overlay,
        &versekit_core::autostart::System,
        &autostart_command(&[]),
        custom_home(),
        versekit_core::paths::Host::current(),
        env!("CARGO_PKG_VERSION"),
    ) {
        after_update(
            &mut overlay,
            &versekit_core::paths::Paths::new(versekit_core::paths::Host::current()),
            env!("CARGO_PKG_VERSION"),
            &std::env::temp_dir(),
        );
        updates::spawn(env!("CARGO_PKG_VERSION"));
    }
    // A backup made right now is told in the main window: it opens on the overview.
    if backed_up {
        overlay.open_overview = true;
    }
    // The overlay starts this very file as the main window, on the same data.
    if overlay.main_program.is_none()
        && let Ok(own) = std::env::current_exe()
    {
        overlay.main_program = Some(own);
        overlay.main_args = main_window_args(overlay.data.as_deref());
    }
    versekit_overlay::run(overlay)
}

/// What a window started by the overlay (or by hand) does before it is built: the backup of what the older version
/// left (`gate`), then its diagnostics (`diagnose`), which set aside and open files in the data. Nothing after a
/// `Quit`.
fn window_start(gate: impl FnOnce() -> Gate, diagnose: impl FnOnce()) -> Gate {
    let gate = gate();
    if gate != Gate::Quit {
        diagnose();
    }
    gate
}

/// How the start of the overlay goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayStart {
    /// The player ended the program at the backup.
    Quit,
    /// The assistant of a first start, without diagnostics.
    Assistant,
    /// The overlay; `backed_up` when the backup was made right now.
    Overlay { backed_up: bool },
}

/// The start of the overlay before it is built, in this order: the backup (`gate`, not for the self-check
/// `check`), the look whether the assistant has to run (`first_start`, not for the self-check), the diagnostics
/// (`diagnose`). The assistant gets no diagnostics: it runs before the player picked the data folder, and its files
/// would stay behind in the default folder.
fn overlay_start(
    check: bool,
    gate: impl FnOnce() -> Gate,
    first_start: impl FnOnce() -> bool,
    diagnose: impl FnOnce(),
) -> OverlayStart {
    let backed_up = if check {
        false
    } else {
        match gate() {
            Gate::Quit => return OverlayStart::Quit,
            Gate::Go { written } => written,
        }
    };
    if !check && first_start() {
        return OverlayStart::Assistant;
    }
    diagnose();
    OverlayStart::Overlay { backed_up }
}

/// The process of the program a start becomes (`diagnostics::Role`), or `None` for a run without diagnostics: the
/// self-check (`--probe`), which has its own data and its own checks.
fn role_of(args: &[String]) -> Option<versekit_core::diagnostics::Role> {
    use versekit_core::diagnostics::Role;
    if args.iter().any(|a| a == MAIN_WINDOW) {
        Some(Role::Main)
    } else if args.iter().any(|a| a == LEARN_WINDOW) {
        Some(Role::Learn)
    } else if args.iter().any(|a| a == "--probe") {
        None
    } else {
        Some(Role::Overlay)
    }
}

/// Sets up the error log, the crash file and the start trail of this process (`diagnostics::init_for`) on the data
/// of the run: the copy after `--daten` (writing locked to it first, as the run does), else the user's own data.
/// False if they cannot be found or were set up already.
fn start_diagnostics(role: versekit_core::diagnostics::Role, data: Option<&str>) -> bool {
    use versekit_core::paths::{Host, Paths};
    let paths = match data {
        Some(root) => {
            versekit_core::writeguard::deny(true);
            versekit_core::writeguard::allow_under(Some(root));
            Paths::for_copy(Host::current(), root)
        }
        None => Some(Paths::new(Host::current())),
    };
    let Some(paths) = paths else { return false };
    versekit_core::diagnostics::init_for(Box::leak(Box::new(paths)), role, env!("CARGO_PKG_VERSION"))
}

/// The key of the run (`window_signal::key_for`): the copy of the data after `--daten`, else the data folder of
/// the variable `home`, else the user's own data. The overlay and the main window of one run get the same.
fn run_key(args: &[String], home: Option<&str>) -> String {
    let data = args.iter().position(|a| a == "--daten").and_then(|at| args.get(at + 1));
    versekit_core::window_signal::key_for(data.map(String::as_str), home)
}

/// The copy of the data after `--daten`, if one is named.
fn data_folder(args: &[String]) -> Option<&str> {
    args.iter().position(|a| a == "--daten").and_then(|at| args.get(at + 1)).map(String::as_str)
}

/// Does the start refuse to run? Yes when it names a data folder that does not arrive (`--daten` misspelled,
/// without a value): the run would otherwise work on the player's own data.
fn refuses(args: &[String]) -> bool {
    args.iter().any(|a| a.to_lowercase().starts_with("--daten")) && !named_data_folder(args)
}

/// Does every `--daten` in `args` have a folder after it (the exact switch)?
fn named_data_folder(args: &[String]) -> bool {
    args.iter().enumerate().filter(|(_, a)| a.to_lowercase().starts_with("--daten")).all(|(at, a)| {
        a == "--daten" && args.get(at + 1).is_some_and(|folder| !folder.is_empty() && !folder.starts_with("--"))
    })
}

/// Is the data folder named from outside (the variable for test runs and special cases)? Then the run is not
/// the player's own, whatever the switches say.
pub(crate) fn custom_home() -> bool {
    std::env::var_os(versekit_core::legacy::ENV_HOME).is_some()
}

/// Without a copy of the data and without the self-check the program works on the user's own data: the
/// overlay reads and writes the real files, and the start with the system that 4.12 set up goes over to the
/// new name (the old entry goes). ⚠ Only here: a run on a copy (`--daten`), a self-check, or a data folder
/// named from outside (`custom_home`) never touches the start with the system of the player. Returns whether
/// it is the player's own data.
fn take_over_own_data(
    overlay: &mut versekit_overlay::Args,
    store: &dyn versekit_core::autostart::Store,
    command: &str,
    custom_home: bool,
) -> bool {
    if overlay.data.is_some() || overlay.probe.is_some() {
        return false;
    }
    overlay.real = true;
    if custom_home {
        return false;
    }
    versekit_core::autostart::migrate(store, command);
    true
}

/// What the start does for the player's own data, and only for it: the real files, the start with the system
/// of 4.12 taken over ([`take_over_own_data`]), and once a day "the program runs", always on, no switch
/// (`SC_BP_NO_NET` ends it). Not for a copy of the data, a check, or a data folder named from outside.
fn begin_own_run(
    overlay: &mut versekit_overlay::Args,
    store: &dyn versekit_core::autostart::Store,
    command: &str,
    custom_home: bool,
    host: versekit_core::paths::Host,
    version: &str,
) -> bool {
    let own = take_over_own_data(overlay, store, command, custom_home);
    if own {
        versekit_core::desktop_entry::follow(&host);
        start_usage(host, version);
    }
    own
}

/// The daily report "the program runs" with the counters of the pages (they are kept in a file by the main
/// window, which another process of this program draws): started with the overlay, which is the part that
/// runs all the time, so a day without the main window is reported too.
fn start_usage(host: versekit_core::paths::Host, version: &str) {
    use versekit_core::paths::Paths;
    use versekit_core::usage::{PageUsage, spawn_ping};
    let paths = Paths::new(host);
    let usage = std::sync::Arc::new(PageUsage::new(Paths::new(paths.host.clone()), version));
    spawn_ping(paths, usage);
}

/// The run of the update helper: the job from the environment `vars`, without which it does nothing.
fn run_update_helper(vars: Vec<(String, String)>) -> Option<i32> {
    use versekit_core::{update_helper, update_run, updater};
    let job = update_run::HelperJob::from_env(&vars)?;
    let system = update_helper::Real { env: updater::clean_environment(vars) };
    Some(update_helper::run(&job, &system))
}

/// At the start on the player's own data: what became of the last update, and the leftovers of an update in
/// the temporary folder `temp`. The outcome goes to the overlay as a message of the system; the main window
/// opens after an update that asks for it. A failed automatic update is remembered, so that it is not tried
/// again at once. ⚠ The jump from 4.12 is not told in this version (it comes with 5.1), only cleaned up.
fn after_update(
    overlay: &mut versekit_overlay::Args,
    paths: &versekit_core::paths::Paths,
    version: &str,
    temp: &std::path::Path,
) {
    use versekit_core::{update_run, updater};
    let now = updater::now();
    let outcome = update_run::evaluate(paths, version, now);
    update_run::remove_leftovers(temp);
    let Some(outcome) = outcome else { return };
    update_run::remember_failure(paths, &outcome, now);
    if outcome.from_previous_program() {
        return;
    }
    overlay.notice = Some(outcome.message());
    overlay.open_overview = update_run::should_open_window(Some(&outcome));
}

/// What the start with the system runs: this very program (an AppImage starts that file), with `arguments`
/// (`desktop_entry::run_arguments`: the copy of the data of the run, none on the user's own data).
pub(crate) fn autostart_command(arguments: &[String]) -> String {
    let program = std::env::current_exe().unwrap_or_default();
    versekit_core::autostart::command(&program, std::env::var("APPIMAGE").ok().as_deref(), arguments)
}

/// The command line of the main window the overlay starts: the switch, and the copy of the data if there is one.
fn main_window_args(data: Option<&str>) -> Vec<String> {
    let mut out = vec![MAIN_WINDOW.to_string()];
    if let Some(folder) = data {
        out.push("--daten".to_string());
        out.push(folder.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start with the system of a computer that 4.12 set up.
    fn old_system() -> versekit_core::autostart::Memory {
        versekit_core::autostart::Memory::with(versekit_core::legacy::AUTOSTART_NAME, "\"C:/alt/SC-BP-Watcher.exe\"")
    }

    #[test]
    fn the_start_with_the_system_of_4_12_goes_over_only_on_the_players_own_data() {
        use versekit_core::autostart::{NAME, Store};
        let old = versekit_core::legacy::AUTOSTART_NAME;
        let own = &mut versekit_overlay::Args::parse(Vec::<String>::new());
        let store = old_system();
        assert!(take_over_own_data(own, &store, "\"C:/neu/versekit.exe\"", false));
        assert!(own.real, "own data: the real files");
        assert_eq!(store.get(NAME).as_deref(), Some("\"C:/neu/versekit.exe\""), "the new entry is set");
        assert_eq!(store.get(old), None, "the old one goes");

        // A copy of the data and the self-check never touch the start with the system.
        for mut args in [
            versekit_overlay::Args::parse(["--daten".to_string(), "C:/kopie".to_string()]),
            versekit_overlay::Args::parse(["--probe".to_string(), "probe.json".to_string()]),
        ] {
            let store = old_system();
            assert!(!take_over_own_data(&mut args, &store, "\"C:/neu/versekit.exe\"", false));
            assert!(!args.real, "no real files on a copy");
            assert_eq!(store.entries().len(), 1, "{:?}", store.entries());
            assert!(store.get(old).is_some() && store.get(NAME).is_none(), "nothing taken over, nothing removed");
        }

        // A data folder named from outside (the variable for test runs) is not the player's own either.
        let mut named = versekit_overlay::Args::parse(Vec::<String>::new());
        let store = old_system();
        assert!(!take_over_own_data(&mut named, &store, "\"C:/neu/versekit.exe\"", true));
        assert!(store.get(old).is_some() && store.get(NAME).is_none(), "{:?}", store.entries());
    }

    /// A receiver on this computer that takes the report as the real one would (204); each body it gets goes to
    /// the channel. Never the real receiver.
    fn receiver() -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/ping", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head_end, length) = loop {
                    if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&raw[..at]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        break (at + 4, length);
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break (raw.len(), 0),
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                };
                while raw.len() < head_end + length {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&raw[head_end.min(raw.len())..]).into_owned());
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        (address, rx)
    }

    /// The host of a throwaway home with a target for the report.
    fn host_for(name: &str, extra: &[(&str, &str)], address: &str) -> (versekit_core::paths::Host, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-nutzung-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let mut vars = vec![
            (versekit_core::legacy::ENV_HOME, text.as_str()),
            ("HOME", text.as_str()),
            ("USERPROFILE", text.as_str()),
            (versekit_core::usage::ENV_TARGET, address),
        ];
        vars.extend_from_slice(extra);
        (versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &text), dir)
    }

    #[test]
    fn the_one_start_reports_that_the_program_runs_once_a_day_to_the_receiver() {
        use std::time::Duration;
        let (address, rx) = receiver();
        let (host, dir) = host_for("meldet", &[], &address);
        start_usage(host, "5.0.0");
        let body = rx.recv_timeout(Duration::from_secs(15)).expect("the start reports to the receiver");
        assert!(body.contains("\"v\": \"5.0.0\"") && body.contains("\"os\""), "{body}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_start_reports_only_on_the_players_own_data() {
        use std::time::Duration;
        let command = "\"C:/neu/versekit.exe\"";
        // The player's own data: it reports (the receiver here stands for the real one).
        let (address, rx) = receiver();
        let (host, dir) = host_for("start-eigen", &[], &address);
        let mut own = versekit_overlay::Args::parse(Vec::<String>::new());
        assert!(begin_own_run(&mut own, &old_system(), command, false, host, "5.0.0"));
        assert!(rx.recv_timeout(Duration::from_secs(15)).is_ok(), "the one start reports");
        let _ = std::fs::remove_dir_all(dir);
        // A copy of the data, a check, a data folder named from outside: nothing goes out.
        let cases = [
            ("kopie", vec!["--daten", "C:/kopie"], false),
            ("pruefung", vec!["--probe", "p.json"], false),
            ("fremder-ordner", vec![], true),
        ];
        for (name, args, custom) in cases {
            let (address, rx) = receiver();
            let (host, dir) = host_for(name, &[], &address);
            let mut run = versekit_overlay::Args::parse(args.iter().map(|a| (*a).to_string()));
            assert!(!begin_own_run(&mut run, &old_system(), command, custom, host, "5.0.0"), "{name}");
            assert!(rx.recv_timeout(Duration::from_secs(2)).is_err(), "{name}: nothing may go out");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn nothing_is_reported_where_no_report_goes_out() {
        use std::time::Duration;
        // The switch off the net, and a version the receiver does not take (a build of the source).
        for (name, extra, version) in
            [("ohne-netz", vec![(versekit_core::legacy::ENV_NO_NET, "1")], "5.0.0"), ("dev", Vec::new(), "5.0.0-dev")]
        {
            let (address, rx) = receiver();
            let (host, dir) = host_for(name, &extra, &address);
            start_usage(host, version);
            assert!(rx.recv_timeout(Duration::from_secs(2)).is_err(), "{name}: nothing may go out");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_data_switch_that_does_not_arrive_is_told_from_one_that_does() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        // What `main` does with it: a switch that does not arrive stops the start, the others go on.
        assert!(!refuses(&args(&[])) && !refuses(&args(&["--daten", "C:/k"])) && !refuses(&args(&["--probe", "p"])));
        for wrong in [&["--daten"][..], &["--Daten", "C:/k"], &["--daten=C:/k"], &["--daten", ""]] {
            assert!(refuses(&args(wrong)), "{wrong:?}");
        }
        assert!(named_data_folder(&args(&[])), "no switch is fine");
        assert!(named_data_folder(&args(&["--daten", "C:/kopie"])));
        assert!(named_data_folder(&args(&["--probe", "p.json", "--daten", "C:/k", "--spiel"])));
        for wrong in [
            &["--daten"][..],
            &["--daten", ""],
            &["--daten", "--spiel"],
            &["--daten=C:/k"],
            &["--Daten", "C:/k"],
            &["--datenordner", "C:/k"],
            &["--daten", "C:/k", "--daten"],
        ] {
            assert!(!named_data_folder(&args(wrong)), "{wrong:?}");
        }
    }

    #[test]
    fn the_overlay_and_its_main_window_belong_to_one_run_and_another_copy_to_another() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let copy = std::env::temp_dir().join("versekit-lauf-kopie").to_string_lossy().into_owned();
        let other = std::env::temp_dir().join("versekit-lauf-andere").to_string_lossy().into_owned();
        // The user's own data: the empty key, the one every build of the program on it shares.
        assert_eq!(run_key(&[], None), "");
        let mut own = versekit_overlay::Args::parse(Vec::<String>::new());
        own.real = true;
        assert_eq!(versekit_overlay::instance_key(&own), run_key(&[], None), "the overlay of the start");
        // A copy: the overlay, and the main window it starts with its own switches, have the key of the copy.
        let start = args(&["--daten", &copy]);
        assert_eq!(run_key(&start, None), versekit_core::window_signal::folder_key(&copy));
        assert_eq!(
            versekit_overlay::instance_key(&versekit_overlay::Args::parse(start.clone())),
            run_key(&start, None)
        );
        assert_eq!(run_key(&main_window_args(Some(&copy)), None), run_key(&start, None), "its main window");
        assert_ne!(run_key(&args(&["--daten", &other]), None), run_key(&start, None), "another copy");
        assert_ne!(run_key(&start, None), "", "a copy is not the user's own data");
        // A data folder named from outside is a run of its own; a copy wins over it, as for the paths.
        assert_eq!(run_key(&[], Some(&other)), versekit_core::window_signal::folder_key(&other));
        assert_eq!(run_key(&start, Some(&other)), run_key(&start, None));
    }

    #[test]
    fn the_start_after_an_update_tells_what_became_of_it_and_clears_the_leftovers() {
        use versekit_core::paths::{Host, Paths};
        use versekit_core::update_run::{self, RunMark};
        let dir = versekit_core::testdir::Dir::new("app-nach-update");
        let text = dir.path().to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &text)],
            &text,
        ));
        let temp = dir.path().join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let leftover = temp.join(versekit_core::legacy::UPDATE_HELPER_SCRIPT);
        std::fs::write(&leftover, b"x").unwrap();
        let now = versekit_core::updater::now();
        let run = |target: &str, previous: &str, automatic: bool| RunMark {
            target: target.into(),
            previous: previous.into(),
            start: now,
            automatic,
            ..RunMark::default()
        };
        // By click, done: told, and the main window opens.
        update_run::begin_run(&paths, &run("v5.0.1", "5.0.0", false)).unwrap();
        let mut overlay = versekit_overlay::Args::parse(Vec::<String>::new());
        after_update(&mut overlay, &paths, "5.0.1", &temp);
        assert!(overlay.notice.as_deref().is_some_and(|n| n.contains("v5.0.1")), "{:?}", overlay.notice);
        assert!(overlay.open_overview);
        assert!(!leftover.exists() && update_run::read_run(&paths).is_none());
        // Automatic and failed: told, the window opens, and the version is not tried again at once.
        update_run::begin_run(&paths, &run("v5.0.2", "5.0.1", true)).unwrap();
        let mut overlay = versekit_overlay::Args::parse(Vec::<String>::new());
        after_update(&mut overlay, &paths, "5.0.1", &temp);
        assert!(overlay.notice.is_some() && overlay.open_overview);
        assert!(update_run::failed_recently(&paths, "v5.0.2", now + 1.0));
        // The jump from 4.12: cleaned up without a word.
        update_run::begin_run(&paths, &run("v5.0.0", "4.12.2", true)).unwrap();
        let mut overlay = versekit_overlay::Args::parse(Vec::<String>::new());
        after_update(&mut overlay, &paths, "5.0.0", &temp);
        assert!(overlay.notice.is_none() && !overlay.open_overview);
        assert!(update_run::read_run(&paths).is_none());
        // Without a run mark nothing happens.
        let mut overlay = versekit_overlay::Args::parse(Vec::<String>::new());
        after_update(&mut overlay, &paths, "5.0.0", &temp);
        assert!(overlay.notice.is_none());
        // The helper does nothing without its job.
        assert_eq!(run_update_helper(vec![("PATH".into(), "x".into())]), None);
    }

    /// `--daten` on an empty folder is a first start on that folder: the assistant runs on its data, not on the
    /// samples of the preview.
    #[test]
    fn a_start_on_an_empty_folder_opens_the_assistant_on_it() {
        let dir = versekit_core::testdir::Dir::new("app-leerer-ordner");
        let root = dir.path().to_string_lossy().into_owned();
        assert!(first_start(Some(&root)), "the assistant runs on the empty copy");
        assert!(dir.path().join(versekit_core::paths::COPY_DATA_FOLDER).is_dir(), "its data folder lies in it");
        versekit_core::writeguard::deny(false);
        assert!(versekit_core::writeguard::take_denied().is_empty(), "nothing written outside the copy");
    }

    #[test]
    fn each_start_writes_the_diagnostics_of_its_own_process() {
        use versekit_core::diagnostics::Role;
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(role_of(&args(&[])), Some(Role::Overlay), "a start without a switch is the overlay");
        assert_eq!(role_of(&args(&["--daten", "C:/k"])), Some(Role::Overlay));
        assert_eq!(role_of(&args(&[BY_MAIN_WINDOW])), Some(Role::Overlay));
        assert_eq!(role_of(&main_window_args(Some("C:/k"))), Some(Role::Main));
        assert_eq!(role_of(&args(&[LEARN_WINDOW, "--daten", "C:/k"])), Some(Role::Learn));
        assert_eq!(role_of(&args(&["--probe", "p.json", "--daten", "C:/k"])), None, "the self-check has none");
    }

    /// ⚠⚠ The diagnostics set aside and cut files of the older version: they come after its backup, never before.
    #[test]
    fn the_diagnostics_start_only_after_the_backup_and_not_for_the_assistant() {
        use std::cell::RefCell;
        let steps = RefCell::new(Vec::<&str>::new());
        let step = |name: &'static str| steps.borrow_mut().push(name);
        let take = || std::mem::take(&mut *steps.borrow_mut());
        let backup = |answer: Gate| {
            step("sicherung");
            answer
        };
        let assistant = |needed: bool| {
            step("assistent?");
            needed
        };
        let go = Gate::Go { written: true };

        // A window the overlay starts.
        assert_eq!(window_start(|| backup(go), || step("diagnose")), go);
        assert_eq!(take(), ["sicherung", "diagnose"]);
        assert_eq!(window_start(|| backup(Gate::Quit), || step("diagnose")), Gate::Quit);
        assert_eq!(take(), ["sicherung"], "ended at the backup: nothing written");

        // The overlay.
        let start = overlay_start(false, || backup(go), || assistant(false), || step("diagnose"));
        assert_eq!(start, OverlayStart::Overlay { backed_up: true });
        assert_eq!(take(), ["sicherung", "assistent?", "diagnose"]);
        let start = overlay_start(false, || backup(Gate::Quit), || assistant(false), || step("diagnose"));
        assert_eq!(start, OverlayStart::Quit);
        assert_eq!(take(), ["sicherung"]);
        // The assistant runs before the data folder is picked: no diagnostics in the default folder.
        let start = overlay_start(false, || backup(go), || assistant(true), || step("diagnose"));
        assert_eq!(start, OverlayStart::Assistant);
        assert_eq!(take(), ["sicherung", "assistent?"]);
        // The self-check: no backup, no assistant.
        let start = overlay_start(true, || backup(go), || assistant(true), || step("diagnose"));
        assert_eq!(start, OverlayStart::Overlay { backed_up: false });
        assert_eq!(take(), ["diagnose"]);
    }

    #[test]
    fn the_main_window_gets_the_switch_and_the_copy_of_the_data_the_overlay_runs_on() {
        assert_eq!(main_window_args(None), ["--hauptfenster"]);
        assert_eq!(main_window_args(Some("C:/copy")), ["--hauptfenster", "--daten", "C:/copy"]);
    }
}
