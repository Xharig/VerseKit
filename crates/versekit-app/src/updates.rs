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

//! The updates of the running program, in a thread of the overlay process (the part that runs all the time).
//!
//! Every ten minutes a look at GitHub; a newer release is fetched with its checksum and installed once it is two
//! minutes old and Star Citizen is closed. Under Windows the installer takes over and the overlay ends;
//! under Linux the AppImage is exchanged, the new version started, and the overlay ends only when the new one
//! lives (else the old file goes back). A build of the source never replaces itself.
//!
//! Whoever runs the program only together with the game would never get an update that way. So the update on offer
//! is also installed when the player ends the program and the game is closed ([`at_exit`], the installer of Windows
//! only), and from one day of waiting the overlay shows it plainly with a button that installs it at once
//! ([`ask_now`]).

use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use versekit_core::auto_update::{self, AtExit, ExitFacts, Step};
use versekit_core::language::{Arg, t, tf};
use versekit_core::net::{Net, Web};
use versekit_core::paths::Paths;
use versekit_core::update_flow::{self, Done, Job};
use versekit_core::update_swap::{self, Started};
use versekit_core::{update_run, updater};

/// How long the program may have run for an automatic update to open the main window afterwards (it was just
/// started, and closing again by itself would look like a crash).
const JUST_STARTED: Duration = Duration::from_secs(300);

/// What the thread does after a round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Round {
    /// Look again after this time.
    Sleep(Duration),
    /// The update is handed over: the program ends.
    Quit,
}

/// What a round tells the overlay of the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tell {
    /// The version on offer (none: nothing newer) and the whole days it has waited from one day on; the bell and
    /// the line in the game name it.
    Known(Option<String>, Option<u64>),
    /// The version waits for the end of the game; told once for each version.
    Waiting(String),
    /// The version is handed over: the program ends in a moment.
    Installing(String),
    /// A sentence written already: why the update asked for by the button did not come.
    Said(String),
}

/// Everything a round needs from outside, so that a check can stand in for the net, the game and the start.
pub(crate) struct World<'a> {
    pub paths: &'a Paths,
    /// For the list of releases.
    pub net: &'a dyn Net,
    /// For the build provenance of the fetched file, asked in a thread of its own.
    pub provenance_net: std::sync::Arc<dyn Net>,
    /// For the file itself (another time limit).
    pub download_net: &'a dyn Net,
    pub version: &'a str,
    /// The variable `APPIMAGE`.
    pub appimage: Option<&'a str>,
    /// The running program file.
    pub program: &'a Path,
    pub temp: &'a Path,
    pub game_running: bool,
    /// Looked at again after the download: the game may have started in the meantime.
    pub game_now: &'a dyn Fn() -> bool,
    pub just_started: bool,
    pub now: f64,
    /// Starts the new AppImage (with a cleaned environment).
    pub start: &'a dyn Fn(&Path) -> io::Result<Box<dyn Started>>,
    /// How long the new AppImage has to live.
    pub alive_wait: Duration,
    /// The version of which the waiting for the game was told last.
    pub told: &'a RefCell<String>,
    pub tell: &'a dyn Fn(Tell),
    /// The player asked for the update now (the button of the overlay): [`by_hand`] instead of the automatic way.
    pub by_hand: bool,
}

/// One round: look, decide, and install when it is time.
pub(crate) fn round(world: &World<'_>) -> Round {
    let checked = updater::check(world.net, world.paths, world.version, false, world.now);
    let offer = checked.offer;
    let since = auto_update::offered_since(world.paths, world.version, offer.is_some(), world.now);
    (world.tell)(Tell::Known(offer.as_ref().map(|r| r.version.clone()), auto_update::days_waiting(since, world.now)));
    if world.by_hand {
        return by_hand(world, offer.as_ref());
    }
    let failed = offer.as_ref().is_some_and(|r| update_run::failed_recently(world.paths, &r.version, world.now));
    // After a failed download of the offered version no new try before the pause is over, whichever process or
    // start tried last. Another version offered fetches at once.
    if let Some(release) = &offer
        && !failed
        && let Some(left) = update_run::download_pause_left(world.paths, &release.version, world.now)
    {
        return Round::Sleep(Duration::from_secs_f64(left.clamp(1.0, auto_update::CHECK_INTERVAL.as_secs_f64())));
    }
    match auto_update::next_step(offer.as_ref(), world.now, world.game_running, failed) {
        Step::Nothing => Round::Sleep(auto_update::CHECK_INTERVAL),
        Step::NotYet(seconds) => Round::Sleep(Duration::from_secs(seconds.max(1))),
        Step::AfterGame => {
            if let Some(release) = &offer {
                told_once(world, release);
            }
            Round::Sleep(auto_update::GAME_POLL)
        }
        Step::Install => match offer {
            Some(release) => install(world, &release, true),
            None => Round::Sleep(auto_update::CHECK_INTERVAL),
        },
    }
}

/// The button of the overlay asked for the update now: fetched at once, whatever failed before and however fresh it
/// is, as a click in the main window. Only the game and the half minute after a failed download hold it back; the
/// player is told why.
fn by_hand(world: &World<'_>, offer: Option<&updater::Release>) -> Round {
    let Some(release) = offer else { return Round::Sleep(auto_update::CHECK_INTERVAL) };
    if world.game_running {
        (world.tell)(Tell::Said(t("ov_update_nach_spiel")));
        return Round::Sleep(auto_update::GAME_POLL);
    }
    if let Some(left) = update_run::manual_wait_left(world.paths, world.now) {
        let seconds = Arg::Int(left.ceil() as i64);
        (world.tell)(Tell::Said(tf("s_up_warte", &[("0", seconds)])));
        return Round::Sleep(auto_update::GAME_POLL);
    }
    install(world, release, false)
}

/// The wait for the end of the game is told once for each version, not at every look.
fn told_once(world: &World<'_>, release: &updater::Release) {
    if *world.told.borrow() != release.version {
        world.told.replace(release.version.clone());
        (world.tell)(Tell::Waiting(release.version.clone()));
    }
}

/// Fetch and install `release`: by itself (`automatic`), or asked for by the button of the overlay.
fn install(world: &World<'_>, release: &updater::Release, automatic: bool) -> Round {
    let job = Job {
        paths: world.paths,
        net: std::sync::Arc::clone(&world.provenance_net),
        download_net: world.download_net,
        own_version: world.version,
        appimage: world.appimage,
        program: world.program,
        temp: world.temp,
        now: world.now,
        automatic,
        show_window: automatic && world.just_started,
        at_exit: false,
        game_running: world.game_now,
        start: world.start,
        alive_wait: world.alive_wait,
    };
    match update_flow::run(&job, release, &mut |_| {}) {
        Done::Quit => {
            (world.tell)(Tell::Installing(release.version.clone()));
            Round::Quit
        }
        Done::GameStarted => {
            told_once(world, release);
            Round::Sleep(auto_update::GAME_POLL)
        }
        Done::Postponed => Round::Sleep(auto_update::GAME_POLL),
        Done::Stopped(said) => {
            if let Some(said) = said.filter(|_| !automatic) {
                (world.tell)(Tell::Said(said));
            }
            Round::Sleep(auto_update::CHECK_INTERVAL)
        }
    }
}

/// Set by the button of the overlay; the thread of the updates wakes for it ([`sleep_or_asked`]).
static ASKED: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// The player asked for the waiting update now: the thread of the updates wakes and fetches it.
pub(crate) fn ask_now() {
    let (asked, wake) = &ASKED;
    *asked.lock().unwrap_or_else(PoisonError::into_inner) = true;
    wake.notify_all();
}

/// Drops a request that came while a round ran: that round has looked already, a second one by hand would only
/// fetch the same again.
fn forget_asked() {
    *ASKED.0.lock().unwrap_or_else(PoisonError::into_inner) = false;
}

/// Sleeps `time`, or less when the player asks for the update meanwhile; whether asked.
fn sleep_or_asked(time: Duration) -> bool {
    let (asked, wake) = &ASKED;
    let guard = asked.lock().unwrap_or_else(PoisonError::into_inner);
    let (mut guard, _) = wake.wait_timeout_while(guard, time, |asked| !*asked).unwrap_or_else(PoisonError::into_inner);
    std::mem::take(&mut *guard)
}

/// How long the end of the program may take to fetch the update: a hidden process must not hang about for half an
/// hour on a slow line.
const EXIT_DOWNLOAD: Duration = Duration::from_secs(5 * 60);

/// What the end of the program looks at, so that a check can stand in for the game, the system and the net.
pub(crate) struct Ending<'a> {
    pub paths: &'a Paths,
    pub version: &'a str,
    pub appimage: Option<&'a str>,
    pub program: &'a Path,
    pub temp: &'a Path,
    pub now: f64,
    /// Star Citizen runs; asked again while it may be ending and once more after the download.
    pub game_now: &'a dyn Fn() -> bool,
    pub session_ending: &'a dyn Fn() -> bool,
    /// Another process of the program file runs (a start of the player meanwhile): it takes the update over.
    pub another_running: &'a dyn Fn() -> bool,
    pub provenance_net: std::sync::Arc<dyn Net>,
    pub download_net: &'a dyn Net,
    pub start: &'a dyn Fn(&Path) -> io::Result<Box<dyn Started>>,
}

/// The end of the program as the player ended it (the icon's „Quit“, the button of the overlay): the update on offer
/// is installed now when the game is closed, by the installer of Windows without a window, and the program is not
/// started again. Nothing when the game runs, Windows is signing out or shutting down, another update is under way,
/// or the player started the program again; all of it is asked before the download and once more right before the
/// installer starts. Says in the update log what it did.
pub(crate) fn at_exit(ending: &Ending<'_>) -> AtExit {
    let offer = updater::known_newer(ending.paths, ending.version);
    let facts = |game_running: bool, another_running: bool| ExitFacts {
        offered: offer.is_some(),
        packaging: Some(updater::packaging(ending.appimage, ending.version)),
        game_running,
        session_ending: (ending.session_ending)(),
        failed_recently: offer
            .as_ref()
            .is_some_and(|r| update_run::failed_recently(ending.paths, &r.version, ending.now)),
        download_paused: offer
            .as_ref()
            .is_some_and(|r| update_run::download_pause_left(ending.paths, &r.version, ending.now).is_some()),
        locked: update_run::lock_held(ending.paths, ending.now),
        another_running,
    };
    // The cheap facts first: the looks at the game and at other processes wait a little, and only when there is
    // something to install.
    let mut decided = auto_update::at_exit(facts(false, false));
    let mut another = false;
    if decided == AtExit::Install {
        let game = (ending.game_now)();
        another = !game && (ending.another_running)();
        decided = auto_update::at_exit(facts(game, another));
    }
    let log = ending.paths.app_file(update_run::LOG_FILE);
    let note = |line: String| update_run::log_line(Path::new(&log), &line);
    let Some(release) = offer else { return decided };
    let version = &release.version;
    match decided {
        AtExit::GameRuns => note(format!("Program ended while the game runs - {version} waits for the next end")),
        AtExit::SessionEnds => note(format!("Program ended with Windows - {version} is not installed now")),
        AtExit::Nothing if another => {
            note(format!("Program ended, but started again meanwhile - {version} is left to that start"))
        }
        AtExit::Nothing => {}
        AtExit::Install => {
            let stop = || (ending.game_now)() || (ending.session_ending)() || (ending.another_running)();
            let job = Job {
                paths: ending.paths,
                net: std::sync::Arc::clone(&ending.provenance_net),
                download_net: ending.download_net,
                own_version: ending.version,
                appimage: ending.appimage,
                program: ending.program,
                temp: ending.temp,
                now: ending.now,
                automatic: true,
                show_window: false,
                at_exit: true,
                game_running: &stop,
                start: ending.start,
                alive_wait: update_swap::ALIVE_WAIT,
            };
            match update_flow::run(&job, &release, &mut |_| {}) {
                Done::Quit => {
                    note(format!("Program ended by the player - {version} is installed now, without a window"))
                }
                Done::GameStarted => {
                    note(format!("{version} not installed: the game, the end of Windows or a new start came first"));
                    return AtExit::GameRuns;
                }
                Done::Postponed => {
                    note(format!("{version} not installed at the end: a backup is at work"));
                    return AtExit::Nothing;
                }
                // The sentence for the player may name the program file: the log keeps to its own lines above.
                Done::Stopped(_) => {
                    note(format!("{version} not installed at the end"));
                    return AtExit::Nothing;
                }
            }
        }
    }
    decided
}

/// Is Star Citizen running at the end of the program? A game that ends right now has written its log seconds ago:
/// up to the quiet time after its end is waited for, as the overlay does (`overlay::EXIT_QUIET_SECONDS`).
fn game_at_exit(paths: &Paths) -> bool {
    let until = Instant::now() + Duration::from_secs_f64(versekit_core::overlay::EXIT_QUIET_SECONDS + 2.0);
    loop {
        let process = versekit_core::game_start::game_process();
        if process == Some(true) {
            return true;
        }
        if !versekit_core::overlay::game_running(process, versekit_core::overlay::log_age(paths)) {
            return false;
        }
        if Instant::now() >= until || auto_update::session_ending() {
            return true;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Does another process of the program file run at the end of the program? The main windows the overlay asked to
/// close may need a moment to go: up to three seconds are waited for them.
fn another_at_exit(program: &Path) -> bool {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let others = update_run::processes_of(program).is_some_and(|others| !others.is_empty());
        if !others || Instant::now() >= until {
            return others;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// [`at_exit`] on the player's own data, in the process that ends. Called after the overlay is gone.
pub(crate) fn install_at_exit(version: &str) {
    let paths = Paths::new(versekit_core::paths::Host::current());
    let program = std::env::current_exe().unwrap_or_default();
    let appimage = std::env::var("APPIMAGE").ok();
    let temp = std::env::temp_dir();
    let game_now = || game_at_exit(&paths);
    let another = || another_at_exit(&program);
    let provenance_net: std::sync::Arc<dyn Net> =
        std::sync::Arc::new(Web::new(updater::USER_AGENT, Duration::from_secs(30), 16 << 20));
    let download_net = Web::new(updater::USER_AGENT, EXIT_DOWNLOAD, 1 << 30);
    at_exit(&Ending {
        paths: &paths,
        version,
        appimage: appimage.as_deref(),
        program: &program,
        temp: &temp,
        now: updater::now(),
        game_now: &game_now,
        session_ending: &auto_update::session_ending,
        another_running: &another,
        provenance_net,
        download_net: &download_net,
        start: &start_new,
    });
}

/// Starts the new AppImage with a cleaned environment, its output in a file of the data folder. It replaces the
/// process that holds the single instance (the overlay, also when the main window fetched the update), or this one.
fn start_new(path: &Path) -> io::Result<Box<dyn Started>> {
    let paths = Paths::new(versekit_core::paths::Host::current());
    let output = std::fs::File::create(paths.app_file(update_swap::RESTART_OUTPUT_FILE)).ok();
    let env = updater::clean_environment(std::env::vars());
    let replaces = versekit_core::window_signal::instance_owner().unwrap_or_else(std::process::id);
    Ok(Box::new(update_swap::restart(path, env, replaces, output)?))
}

/// What the main window needs to fetch a version on a click: the net, the program file, and how the new version
/// is started. The same ways as the thread of the overlay goes.
pub(crate) fn window_env(version: &str) -> update_flow::Env {
    let net: std::sync::Arc<dyn Net> =
        std::sync::Arc::new(Web::new(updater::USER_AGENT, Duration::from_secs(30), 16 << 20));
    let download_net: std::sync::Arc<dyn Net> =
        std::sync::Arc::new(Web::new(updater::USER_AGENT, Duration::from_secs(30 * 60), 1 << 30));
    update_flow::Env {
        net,
        download_net,
        own_version: version.to_string(),
        appimage: std::env::var("APPIMAGE").ok(),
        program: std::env::current_exe().unwrap_or_default(),
        temp: std::env::temp_dir(),
        start: std::sync::Arc::new(start_new),
        game_running: std::sync::Arc::new(|| {
            let paths = Paths::new(versekit_core::paths::Host::current());
            versekit_core::overlay::game_running(
                versekit_core::game_start::game_process(),
                versekit_core::overlay::log_age(&paths),
            )
        }),
        alive_wait: update_swap::ALIVE_WAIT,
    }
}

/// Starts the updates of the program on the player's own data, in a thread of their own. A build of the source
/// asks GitHub as well (for the update hint) but never installs.
pub(crate) fn spawn(version: &'static str) {
    let started = std::time::Instant::now();
    let _ = std::thread::Builder::new().name("updates".into()).spawn(move || {
        let paths = Paths::new(versekit_core::paths::Host::current());
        let net = Web::new(updater::USER_AGENT, Duration::from_secs(30), 16 << 20);
        let download_net = Web::new(updater::USER_AGENT, Duration::from_secs(30 * 60), 1 << 30);
        let provenance_net: std::sync::Arc<dyn Net> =
            std::sync::Arc::new(Web::new(updater::USER_AGENT, Duration::from_secs(30), 16 << 20));
        let program = std::env::current_exe().unwrap_or_default();
        let appimage = std::env::var("APPIMAGE").ok();
        let temp = std::env::temp_dir();
        let start = start_new;
        let game_now = || {
            versekit_core::overlay::game_running(
                versekit_core::game_start::game_process(),
                versekit_core::overlay::log_age(&paths),
            )
        };
        let told = RefCell::new(String::new());
        let tell = |what: Tell| match what {
            Tell::Known(version, days) => {
                versekit_overlay::set_update(version.map(|v| updater::shown_version(&v).to_string()));
                versekit_overlay::set_update_days(days);
            }
            Tell::Waiting(version) => versekit_overlay::tell("up_auto_wartet", updater::shown_version(&version)),
            Tell::Installing(version) => versekit_overlay::tell("up_auto_laeuft", updater::shown_version(&version)),
            Tell::Said(text) => versekit_overlay::say(text),
        };
        versekit_overlay::on_update_now(ask_now);
        let mut by_hand = false;
        loop {
            let world = World {
                paths: &paths,
                net: &net,
                provenance_net: std::sync::Arc::clone(&provenance_net),
                download_net: &download_net,
                version,
                appimage: appimage.as_deref(),
                program: &program,
                temp: &temp,
                game_running: game_now(),
                game_now: &game_now,
                just_started: started.elapsed() < JUST_STARTED,
                now: updater::now(),
                start: &start,
                alive_wait: update_swap::ALIVE_WAIT,
                told: &told,
                tell: &tell,
                by_hand,
            };
            let done = round(&world);
            forget_asked();
            match done {
                Round::Sleep(time) => by_hand = sleep_or_asked(time),
                Round::Quit => {
                    versekit_overlay::ask_to_quit();
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use versekit_core::net::Fake;
    use versekit_core::paths::Host;
    use versekit_core::testdir::Dir;

    use super::*;

    const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download/v5.0.1";

    fn sha(bytes: &[u8]) -> String {
        let dir = Dir::new("updates-summe");
        let file = dir.path().join("x");
        fs::write(&file, bytes).unwrap();
        updater::checksum_of(&file).unwrap()
    }

    /// GitHub with one release 5.0.1 published at `published`, its AppImage and checksums.
    fn github(published: &str, content: &[u8]) -> Fake {
        let api = format!(
            r#"[{{"tag_name":"v5.0.1","published_at":"{published}","prerelease":false,"assets":[
                {{"name":"VerseKit-x86_64.AppImage","browser_download_url":"{BASE}/VerseKit-x86_64.AppImage"}},
                {{"name":"VerseKit-Setup.exe","browser_download_url":"{BASE}/VerseKit-Setup.exe"}},
                {{"name":"SHA256SUMS.txt","browser_download_url":"{BASE}/SHA256SUMS.txt"}}]}}]"#
        );
        Fake::default()
            .with(updater::API, api.into_bytes())
            .with(
                &format!("{BASE}/SHA256SUMS.txt"),
                format!("{}  VerseKit-x86_64.AppImage\n", sha(content)).into_bytes(),
            )
            .with(&format!("{BASE}/VerseKit-x86_64.AppImage"), content.to_vec())
    }

    /// The path of the running program file.
    fn program_in(dir: &Path) -> PathBuf {
        dir.join("SC-BP-Watcher.exe")
    }

    struct Ends(u32);

    impl Started for Ends {
        fn ended(&mut self) -> Option<i32> {
            if self.0 == 0 {
                return Some(127);
            }
            self.0 -= 1;
            None
        }
    }

    fn paths_in(dir: &Path) -> Paths {
        let text = dir.join("daten").to_string_lossy().into_owned();
        fs::create_dir_all(&text).unwrap();
        Paths::new(Host::new(
            versekit_core::pypath::Flavor::native(),
            &[(versekit_core::legacy::ENV_HOME, &text)],
            &text,
        ))
    }

    fn world<'a>(
        paths: &'a Paths,
        net: &'a Fake,
        version: &'a str,
        appimage: Option<&'a str>,
        dir: &'a Path,
        program: &'a Path,
        start: &'a dyn Fn(&Path) -> io::Result<Box<dyn Started>>,
    ) -> World<'a> {
        World {
            paths,
            net,
            // Answers nothing: the build provenance is only noted and checked in versekit-core.
            provenance_net: std::sync::Arc::new(Fake::default()),
            download_net: net,
            version,
            appimage,
            program,
            temp: dir,
            game_running: false,
            game_now: &no_game,
            just_started: false,
            now: auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap(),
            start,
            alive_wait: Duration::from_millis(300),
            told: Box::leak(Box::new(RefCell::new(String::new()))),
            tell: &ignore,
            by_hand: false,
        }
    }

    fn no_game() -> bool {
        false
    }

    fn ignore(_: Tell) {}

    #[test]
    fn nothing_new_waits_ten_minutes_and_a_running_game_twenty_seconds() {
        let dir = Dir::new("updates-warten");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let empty = Fake::default().with(updater::API, b"[]".to_vec());
        assert_eq!(
            round(&world(&paths, &empty, "5.0.1", None, dir.path(), &program, &never)),
            Round::Sleep(auto_update::CHECK_INTERVAL)
        );
        // Another data folder each, so that GitHub is asked again and not the answer of before counts.
        let in_game = paths_in(&dir.path().join("spiel"));
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let mut w = world(&in_game, &net, "5.0.0", None, dir.path(), &program, &never);
        w.game_running = true;
        assert_eq!(round(&w), Round::Sleep(auto_update::GAME_POLL), "never in the game");
        let fresh = github("2026-11-20T09:59:30Z", b"neu");
        // Another data folder, so that GitHub is asked again.
        let other = paths_in(&dir.path().join("frisch"));
        let w = world(&other, &fresh, "5.0.0", None, dir.path(), &program, &never);
        assert_eq!(round(&w), Round::Sleep(Duration::from_secs(90)), "a fresh release waits");
    }

    #[test]
    fn a_build_of_the_source_never_replaces_itself() {
        let dir = Dir::new("updates-quelle");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = github("2026-11-20T09:00:00Z", b"neu");
        assert_eq!(
            round(&world(&paths, &net, "5.0.0-dev", None, dir.path(), &program, &never)),
            Round::Sleep(auto_update::CHECK_INTERVAL)
        );
        let lists: Vec<String> = updater::sources("5.0.0-dev").iter().map(ToString::to_string).collect();
        assert_eq!(net.asked(), lists, "nothing fetched but the lists");
    }

    #[test]
    fn an_appimage_is_exchanged_and_the_old_one_ends_only_when_the_new_one_lives() {
        let dir = Dir::new("updates-appimage");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let lives = |_: &Path| -> io::Result<Box<dyn Started>> { Ok(Box::new(Ends(u32::MAX))) };
        assert_eq!(round(&world(&paths, &net, "5.0.0", Some(&text), dir.path(), &appimage, &lives)), Round::Quit);
        assert_eq!(fs::read(&appimage).unwrap(), b"neu");
        assert!(!update_run::lock_held(&paths, updater::now()), "the lock is free again");
    }

    #[test]
    fn a_new_appimage_that_dies_at_its_start_goes_back_and_is_not_tried_again_at_once() {
        let dir = Dir::new("updates-zurueck");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let net = github("2026-11-20T09:00:00Z", b"kaputt");
        let dies = |_: &Path| -> io::Result<Box<dyn Started>> { Ok(Box::new(Ends(1))) };
        let w = world(&paths, &net, "5.0.0", Some(&text), dir.path(), &appimage, &dies);
        assert_eq!(round(&w), Round::Sleep(auto_update::CHECK_INTERVAL));
        assert_eq!(fs::read(&appimage).unwrap(), b"alt", "the old version is back");
        assert!(update_run::failed_recently(&paths, "v5.0.1", w.now + 60.0));
        // The next round does not try the same version again.
        let asked = net.asked().len();
        assert_eq!(round(&w), Round::Sleep(auto_update::CHECK_INTERVAL));
        assert_eq!(net.asked().len(), asked, "nothing fetched again");
    }

    #[test]
    fn the_wait_for_the_game_is_told_once_for_each_version_and_the_hand_over_when_it_comes() {
        let dir = Dir::new("updates-melden");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let told = RefCell::new(String::new());
        let log = RefCell::new(Vec::<Tell>::new());
        let tell = |what: Tell| log.borrow_mut().push(what);
        let mut w = world(&paths, &net, "5.0.0", None, dir.path(), &program, &never);
        w.game_running = true;
        w.told = &told;
        w.tell = &tell;
        for _ in 0..3 {
            assert_eq!(round(&w), Round::Sleep(auto_update::GAME_POLL));
        }
        let known = || Tell::Known(Some("v5.0.1".into()), None);
        assert_eq!(
            *log.borrow(),
            [known(), Tell::Waiting("v5.0.1".into()), known(), known()],
            "the wait is told at the first look only"
        );
        // Nothing newer: the bell goes.
        log.borrow_mut().clear();
        let newest = paths_in(&dir.path().join("neueste"));
        let mut w = world(&newest, &net, "5.0.1", None, dir.path(), &program, &never);
        w.tell = &tell;
        round(&w);
        assert_eq!(*log.borrow(), [Tell::Known(None, None)]);
        // The game starts while the file comes in: told, nothing installed, looked at again soon.
        log.borrow_mut().clear();
        let c = dir.path().join("spiel");
        fs::create_dir_all(&c).unwrap();
        let paths = paths_in(&c);
        let appimage = c.join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let started = || true;
        let other_told = RefCell::new(String::new());
        let mut w = world(&paths, &net, "5.0.0", Some(&text), &c, &appimage, &never);
        w.game_now = &started;
        w.told = &other_told;
        w.tell = &tell;
        assert_eq!(round(&w), Round::Sleep(auto_update::GAME_POLL));
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
        assert_eq!(*log.borrow(), [known(), Tell::Waiting("v5.0.1".into())]);
        assert!(!update_run::lock_held(&paths, updater::now()));
        // The hand-over is told at the end.
        log.borrow_mut().clear();
        let lives = |_: &Path| -> io::Result<Box<dyn Started>> { Ok(Box::new(Ends(u32::MAX))) };
        let mut w = world(&paths, &net, "5.0.0", Some(&text), &c, &appimage, &lives);
        w.tell = &tell;
        assert_eq!(round(&w), Round::Quit);
        assert_eq!(*log.borrow(), [known(), Tell::Installing("v5.0.1".into())]);
    }

    /// GitHub on a day it is disturbed: the list comes, the checksums of the release answer with a gateway error.
    struct Disturbed(Fake);

    impl versekit_core::net::Net for Disturbed {
        fn get(&self, url: &str) -> Result<Vec<u8>, versekit_core::net::FetchError> {
            let body = self.0.get(url);
            if url.ends_with(updater::CHECKSUM_FILE) {
                return Err(versekit_core::net::FetchError::Failed("HTTP 504".into()));
            }
            body
        }

        fn head(&self, url: &str) -> Result<versekit_core::net::Headers, versekit_core::net::FetchError> {
            self.0.head(url)
        }
    }

    /// 23 looks in 61 minutes, each from a fresh process (an overlay started again, a second one, the window): after
    /// a failed download the next try comes ten minutes later at the earliest, and each further failure waits longer.
    #[test]
    fn after_a_failed_download_no_process_and_no_new_start_tries_again_within_ten_minutes() {
        let dir = Dir::new("updates-504");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = Disturbed(github("2026-11-20T09:00:00Z", b"neu"));
        let start = auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap();
        let checksums = format!("{BASE}/{}", updater::CHECKSUM_FILE);
        let mut tries = Vec::new();
        for look in 0..23 {
            let now = start + f64::from(look) * 166.0;
            let before = net.0.asked().iter().filter(|url| **url == checksums).count();
            let fresh = Paths::new(paths.host.clone());
            let mut w = world(&fresh, &net.0, "5.0.0", Some(&text), dir.path(), &appimage, &never);
            w.download_net = &net;
            w.now = now;
            let Round::Sleep(time) = round(&w) else { panic!("nothing is installed") };
            assert!(time <= auto_update::CHECK_INTERVAL, "look {look}: {time:?}");
            if net.0.asked().iter().filter(|url| **url == checksums).count() > before {
                tries.push(now - start);
            }
        }
        assert!(tries.windows(2).all(|pair| pair[1] - pair[0] >= 600.0), "ten minutes at least: {tries:?}");
        assert_eq!(tries, [0.0, 4.0 * 166.0, 12.0 * 166.0], "tries at 0, 11 and 33 minutes");
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
        // The report tells these tries from clicks.
        let log = fs::read_to_string(paths.app_file(update_run::LOG_FILE)).unwrap();
        assert_eq!(log.matches("Download of v5.0.1 (automatic) failed").count(), 3, "{log}");
    }

    /// The days the offer has waited go to the overlay with the version, from one whole day on.
    #[test]
    fn the_days_an_update_has_waited_go_to_the_overlay_from_one_day_on() {
        let dir = Dir::new("updates-tage");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = github("2026-11-01T09:00:00Z", b"neu");
        let log = RefCell::new(Vec::<Tell>::new());
        let tell = |what: Tell| log.borrow_mut().push(what);
        let mut w = world(&paths, &net, "5.0.0", None, dir.path(), &program, &never);
        w.game_running = true;
        w.tell = &tell;
        let first = w.now;
        round(&w);
        assert_eq!(log.borrow()[0], Tell::Known(Some("v5.0.1".into()), None), "the first look: not a day yet");
        for (later, days) in [(23.0 * 3600.0, None), (24.0 * 3600.0, Some(1)), (3.5 * 86_400.0, Some(3))] {
            log.borrow_mut().clear();
            w.now = first + later;
            round(&w);
            assert_eq!(log.borrow()[0], Tell::Known(Some("v5.0.1".into()), days), "after {later} s");
        }
        // Updated: the new version has nothing newer, the days go.
        log.borrow_mut().clear();
        let mut w = world(&paths, &net, "5.0.1", None, dir.path(), &program, &never);
        w.tell = &tell;
        w.now = first + 4.0 * 86_400.0;
        round(&w);
        assert_eq!(*log.borrow(), [Tell::Known(None, None)]);
    }

    /// The button of the overlay fetches at once, also a version whose automatic update failed a short while ago,
    /// which the automatic way leaves alone for a day.
    #[test]
    fn the_button_fetches_at_once_also_after_a_failed_update_which_the_automatic_way_leaves_alone() {
        let dir = Dir::new("updates-knopf");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let lives = |_: &Path| -> io::Result<Box<dyn Started>> { Ok(Box::new(Ends(u32::MAX))) };
        let mut w = world(&paths, &net, "5.0.0", Some(&text), dir.path(), &appimage, &lives);
        update_run::remember_failed_version(&paths, "v5.0.1", w.now - 60.0);
        assert_eq!(round(&w), Round::Sleep(auto_update::CHECK_INTERVAL), "the automatic way waits");
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
        w.by_hand = true;
        assert_eq!(round(&w), Round::Quit, "the button fetches");
        assert_eq!(fs::read(&appimage).unwrap(), b"neu");
    }

    /// The button never installs in the game and not in the half minute after a failed download: the player is told
    /// why, and nothing is fetched.
    #[test]
    fn the_button_tells_why_while_the_game_runs_or_right_after_a_failed_download() {
        let dir = Dir::new("updates-knopf-nein");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let log = RefCell::new(Vec::<Tell>::new());
        let tell = |what: Tell| log.borrow_mut().push(what);
        let mut w = world(&paths, &net, "5.0.0", Some(&text), dir.path(), &appimage, &never);
        w.by_hand = true;
        w.tell = &tell;
        w.game_running = true;
        let lists = net.asked().len();
        assert_eq!(round(&w), Round::Sleep(auto_update::GAME_POLL));
        assert_eq!(log.borrow().last(), Some(&Tell::Said(t("ov_update_nach_spiel"))));
        // A failed download ten seconds ago: twenty seconds to wait.
        w.game_running = false;
        update_run::remember_download_failure(&paths, "v5.0.1", w.now - 10.0, false);
        assert_eq!(round(&w), Round::Sleep(auto_update::GAME_POLL));
        assert_eq!(log.borrow().last(), Some(&Tell::Said(tf("s_up_warte", &[("0", Arg::Int(20))]))));
        assert!(net.asked().len() <= lists + 1, "nothing fetched but the list: {:?}", net.asked());
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
    }

    /// The button wakes the thread at once; without it the thread sleeps its time.
    #[test]
    fn the_button_wakes_the_sleeping_thread() {
        let started = std::time::Instant::now();
        assert!(!sleep_or_asked(Duration::from_millis(50)), "nobody asked");
        assert!(started.elapsed() >= Duration::from_millis(50));
        let asking = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            ask_now();
        });
        let started = std::time::Instant::now();
        assert!(sleep_or_asked(Duration::from_secs(60)), "asked");
        assert!(started.elapsed() < Duration::from_secs(10), "woken, not slept out");
        asking.join().unwrap();
        assert!(!sleep_or_asked(Duration::from_millis(10)), "the request is taken once");
        // A click while a round ran is dropped with its end: no second round by hand after it.
        ask_now();
        forget_asked();
        assert!(!sleep_or_asked(Duration::from_millis(10)), "a click during the round does not run again");
    }

    fn ending<'a>(
        paths: &'a Paths,
        net: &'a Fake,
        appimage: Option<&'a str>,
        dir: &'a Path,
        program: &'a Path,
        game: &'a dyn Fn() -> bool,
        session: &'a dyn Fn() -> bool,
    ) -> Ending<'a> {
        Ending {
            paths,
            version: "5.0.0",
            appimage,
            program,
            temp: dir,
            now: auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap(),
            game_now: game,
            session_ending: session,
            another_running: &no_game,
            provenance_net: std::sync::Arc::new(Fake::default()),
            download_net: net,
            start: &|_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started at the end") },
        }
    }

    /// At the end of the program nothing is fetched while the game runs or Windows ends (the update log says so),
    /// for an AppImage (it would have to start again) and for a build of the source.
    #[test]
    fn the_end_of_the_program_fetches_nothing_in_the_game_at_the_end_of_windows_or_for_an_appimage() {
        let dir = Dir::new("updates-ende");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let net = github("2026-11-20T09:00:00Z", b"neu");
        let now = auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap();
        updater::check(&net, &paths, "5.0.0", true, now);
        let lists = net.asked().len();
        let yes = || true;
        let log = || fs::read_to_string(paths.app_file(update_run::LOG_FILE)).unwrap_or_default();
        // Under Windows the installer would come; here the program counts as a build of the source.
        let plain = ending(&paths, &net, None, dir.path(), &program, &no_game, &no_game);
        let expected = if cfg!(windows) { AtExit::Install } else { AtExit::Nothing };
        assert_eq!(auto_update::at_exit(ExitFacts { game_running: false, ..exit_facts(&plain) }), expected);
        assert_eq!(at_exit(&ending(&paths, &net, None, dir.path(), &program, &yes, &no_game)), {
            if cfg!(windows) { AtExit::GameRuns } else { AtExit::Nothing }
        });
        assert_eq!(at_exit(&ending(&paths, &net, None, dir.path(), &program, &yes, &yes)), AtExit::SessionEnds);
        assert!(log().contains("Program ended with Windows - v5.0.1 is not installed now"), "{}", log());
        if cfg!(windows) {
            assert!(log().contains("Program ended while the game runs - v5.0.1 waits"), "{}", log());
        }
        // An AppImage stays as it is, game or not.
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        assert_eq!(
            at_exit(&ending(&paths, &net, Some(&text), dir.path(), &appimage, &no_game, &no_game)),
            AtExit::Nothing
        );
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
        assert_eq!(net.asked().len(), lists, "nothing fetched at the end");
        assert!(!update_run::lock_held(&paths, updater::now()));
    }

    /// The facts of the end as [`at_exit`] reads them, the game left out.
    fn exit_facts(ending: &Ending<'_>) -> ExitFacts {
        ExitFacts {
            offered: updater::known_newer(ending.paths, ending.version).is_some(),
            packaging: Some(updater::packaging(ending.appimage, ending.version)),
            ..ExitFacts::default()
        }
    }

    /// Under Windows: a start of the player while the file came in takes the update over, the installer of the end
    /// does not start (it would close that start and not bring it back).
    #[cfg(windows)]
    #[test]
    fn under_windows_the_end_installs_nothing_when_the_player_started_the_program_again_meanwhile() {
        let dir = Dir::new("updates-ende-neustart");
        let paths = paths_in(dir.path());
        let program = program_in(dir.path());
        let installer = "VerseKit-Setup.exe";
        let api = format!(
            r#"[{{"tag_name":"v5.0.1","published_at":"2026-11-20T09:00:00Z","prerelease":false,"assets":[
                {{"name":"{installer}","browser_download_url":"{BASE}/{installer}"}},
                {{"name":"SHA256SUMS.txt","browser_download_url":"{BASE}/SHA256SUMS.txt"}}]}}]"#
        );
        let net = Fake::default()
            .with(updater::API, api.into_bytes())
            .with(&format!("{BASE}/SHA256SUMS.txt"), format!("{}  {installer}\n", sha(b"neu")).into_bytes())
            .with(&format!("{BASE}/{installer}"), b"neu".to_vec());
        let now = auto_update::parse_moment("2026-11-20T10:00:00Z").unwrap();
        updater::check(&net, &paths, "5.0.0", true, now);
        let lists = net.asked().len();
        let log = || fs::read_to_string(paths.app_file(update_run::LOG_FILE)).unwrap_or_default();
        // Started again before the download: nothing fetched at all.
        let again = || true;
        let mut end = ending(&paths, &net, None, dir.path(), &program, &no_game, &no_game);
        end.another_running = &again;
        assert_eq!(at_exit(&end), AtExit::Nothing);
        assert_eq!(net.asked().len(), lists, "nothing fetched");
        assert!(log().contains("v5.0.1 is left to that start"), "{}", log());
        // Started again while the file came in: no installer, the lock free, and no pause for the next start.
        let looks = std::cell::Cell::new(0);
        let later = || {
            looks.set(looks.get() + 1);
            looks.get() > 1
        };
        let mut end = ending(&paths, &net, None, dir.path(), &program, &no_game, &no_game);
        end.another_running = &later;
        assert_eq!(at_exit(&end), AtExit::GameRuns);
        assert!(net.asked().len() > lists, "fetched");
        assert!(update_run::read_run(&paths).is_none(), "no installer started");
        assert!(!update_run::lock_held(&paths, updater::now()));
        assert!(update_run::download_pause_left(&paths, "v5.0.1", now).is_none());
        assert!(log().contains("v5.0.1 not installed: the game, the end of Windows or a new start came first"));
    }

    #[test]
    fn a_download_that_fails_its_checksum_installs_nothing() {
        let dir = Dir::new("updates-falsch");
        let paths = paths_in(dir.path());
        let appimage = dir.path().join("VerseKit-x86_64.AppImage");
        fs::write(&appimage, b"alt").unwrap();
        let text = appimage.to_string_lossy().into_owned();
        let never = |_: &Path| -> io::Result<Box<dyn Started>> { panic!("nothing is started") };
        let net = Fake {
            bodies: github("2026-11-20T09:00:00Z", b"neu")
                .bodies
                .into_iter()
                .map(
                    |(url, body)| {
                        if url.ends_with(".AppImage") { (url, b"untergeschoben".to_vec()) } else { (url, body) }
                    },
                )
                .collect(),
            ..Fake::default()
        };
        assert_eq!(
            round(&world(&paths, &net, "5.0.0", Some(&text), dir.path(), &appimage, &never)),
            Round::Sleep(auto_update::CHECK_INTERVAL)
        );
        assert_eq!(fs::read(&appimage).unwrap(), b"alt");
        assert!(!dir.path().join(".VerseKit-x86_64.AppImage.neu").exists(), "no fragment stays");
        assert!(!update_run::lock_held(&paths, updater::now()));
    }
}
