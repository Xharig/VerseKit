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

//! The updates of the running program, in a thread of the overlay process (the part that runs all the time).
//!
//! Every ten minutes a look at GitHub; a newer release is fetched with its checksum and installed once it is two
//! minutes old and Star Citizen is closed. Under Windows the update helper takes over and the overlay ends;
//! under Linux the AppImage is exchanged, the new version started, and the overlay ends only when the new one
//! lives (else the old file goes back). A build of the source never replaces itself.

use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::time::Duration;

use versekit_core::auto_update::{self, Step};
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
    /// The version on offer (none: nothing newer); the bell and the line in the game name it.
    Known(Option<String>),
    /// The version waits for the end of the game; told once for each version.
    Waiting(String),
    /// The version is handed over: the program ends in a moment.
    Installing(String),
}

/// Everything a round needs from outside, so that a check can stand in for the net, the game and the start.
pub(crate) struct World<'a> {
    pub paths: &'a Paths,
    /// For the list of releases and the checksums.
    pub net: &'a dyn Net,
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
}

/// One round: look, decide, and install when it is time.
pub(crate) fn round(world: &World<'_>) -> Round {
    let checked = updater::check(world.net, world.paths, world.version, false, world.now);
    let offer = checked.offer;
    (world.tell)(Tell::Known(offer.as_ref().map(|r| r.version.clone())));
    let failed = offer.as_ref().is_some_and(|r| update_run::failed_recently(world.paths, &r.version, world.now));
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
            Some(release) => install(world, &release),
            None => Round::Sleep(auto_update::CHECK_INTERVAL),
        },
    }
}

/// The wait for the end of the game is told once for each version, not at every look.
fn told_once(world: &World<'_>, release: &updater::Release) {
    if *world.told.borrow() != release.version {
        world.told.replace(release.version.clone());
        (world.tell)(Tell::Waiting(release.version.clone()));
    }
}

fn install(world: &World<'_>, release: &updater::Release) -> Round {
    let job = Job {
        paths: world.paths,
        download_net: world.download_net,
        own_version: world.version,
        appimage: world.appimage,
        program: world.program,
        temp: world.temp,
        now: world.now,
        automatic: true,
        show_window: world.just_started,
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
        Done::Stopped(_) => Round::Sleep(auto_update::CHECK_INTERVAL),
    }
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
            Tell::Known(version) => {
                versekit_overlay::set_update(version.map(|v| updater::shown_version(&v).to_string()))
            }
            Tell::Waiting(version) => versekit_overlay::tell("up_auto_wartet", updater::shown_version(&version)),
            Tell::Installing(version) => versekit_overlay::tell("up_auto_laeuft", updater::shown_version(&version)),
        };
        loop {
            let world = World {
                paths: &paths,
                net: &net,
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
            };
            match round(&world) {
                Round::Sleep(time) => std::thread::sleep(time),
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
        let known = || Tell::Known(Some("v5.0.1".into()));
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
        assert_eq!(*log.borrow(), [Tell::Known(None)]);
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
