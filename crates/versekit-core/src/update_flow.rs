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

//! Fetching and installing a version, the one way the click in the window and the automatic update share
//! (4.12 `pages._fetch_version` and `_auto_update` of the main module).
//!
//! [`run`] is the sequence for a release that is chosen: file that fits, lock, download with the percentage,
//! installing, the hand-over. [`Runner`] is the same in a thread of its own, for a window that must not freeze
//! while twelve megabytes come in; it first looks which release the channel wants.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::auto_update::{self, Installed, Request};
use crate::language::{Arg, SpawnWithLanguage as _, t, tf};
use crate::net::Net;
use crate::paths::Paths;
use crate::update_swap::{self, Started};
use crate::updater::{self, Packaging, Release};
use crate::{errors, update_run};

/// Starts the new AppImage with a cleaned environment.
pub type StartFn = dyn Fn(&Path) -> io::Result<Box<dyn Started>> + Send + Sync;

/// Whether the game runs right now.
pub type GameFn = dyn Fn() -> bool + Send + Sync;

/// What the sequence needs from outside, so that a check can stand in for the net, the game and the start.
pub struct Job<'a> {
    pub paths: &'a Paths,
    /// For the file itself (a longer time limit than the list of releases).
    pub download_net: &'a dyn Net,
    /// The running version.
    pub own_version: &'a str,
    /// The variable `APPIMAGE`.
    pub appimage: Option<&'a str>,
    /// The running program file.
    pub program: &'a Path,
    pub temp: &'a Path,
    pub now: f64,
    /// By itself, without a click: the window opens after the update only if the program has just started.
    pub automatic: bool,
    pub show_window: bool,
    /// Looked at again after the download: the game may have started in the meantime.
    pub game_running: &'a dyn Fn() -> bool,
    pub start: &'a dyn Fn(&Path) -> io::Result<Box<dyn Started>>,
    /// How long the new AppImage has to live.
    pub alive_wait: Duration,
}

/// What happens on the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The percentage of the download, once per value.
    Progress(u8),
    /// A sentence for the player.
    Say(String),
}

/// How the sequence ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// The helper took over (Windows) or the new AppImage lives (Linux): every window of the program has to
    /// end now.
    Quit,
    /// Nothing was installed; the sentence for the player, if there is one.
    Stopped(Option<String>),
    /// The game started while the file came in; the automatic update waits for its end.
    GameStarted,
}

fn said(key: &str, argument: &str) -> String {
    tf(key, &[("0", Arg::from(argument.to_string()))])
}

fn failed(why: &str) -> Done {
    Done::Stopped(Some(said("update_fehler", why)))
}

/// Fetch and install `release`.
///
/// ⚠ The lock comes before the download: two clicks in a row, or a click and the automatic update, would
/// otherwise let two installers go. It is given back on every way out but the hand-over to the helper, which
/// owns it from then on.
pub fn run(job: &Job<'_>, release: &Release, report: &mut dyn FnMut(Event)) -> Done {
    let kind = updater::packaging(job.appimage, job.own_version);
    if kind == Packaging::Source {
        return Done::Stopped(Some(t("update_quellcode")));
    }
    let Some(asset) = updater::matching_asset(release, kind) else {
        // ⚠ Two different situations: a release without any file is still being built (GitHub needs a minute or
        // two for the installer), one with files but none that fits is for fetching by hand.
        let key = if release.assets.is_empty() { "s_ub_wird_gebaut" } else { "selbst_holen" };
        return Done::Stopped(Some(t(key)));
    };
    let Some(name) = updater::safe_filename(&asset.name) else {
        return Done::Stopped(Some(updater::UpdateError::ForeignFile(asset.name.clone()).message()));
    };
    if !update_run::take_lock(job.paths, std::process::id(), job.now) {
        return Done::Stopped(Some(t("up_laeuft_schon")));
    }
    let appimage = updater::own_appimage(job.appimage);
    let program = appimage.clone().unwrap_or_else(|| job.program.to_path_buf());
    let target = updater::download_target(&name, kind, appimage.as_deref(), job.temp);
    // ⚠ The display must not end the download: a progress call that fails is dropped, and not asked again.
    let mut display_broken = false;
    let mut progress = |percent: u8| {
        if !display_broken && catch_unwind(AssertUnwindSafe(|| report(Event::Progress(percent)))).is_err() {
            display_broken = true;
        }
    };
    let downloaded = match updater::download(job.download_net, release, asset, &target, &mut progress) {
        Ok(downloaded) => downloaded,
        Err(error) => {
            errors::record("updates.download", &error.message());
            update_run::release_lock(job.paths);
            return failed(&error.message());
        }
    };
    // ⚠ Between the download and the installing the game may have started: seconds to minutes pass.
    if job.automatic && (job.game_running)() {
        update_run::release_lock(job.paths);
        return Done::GameStarted;
    }
    let installed = auto_update::install(Request {
        paths: job.paths,
        downloaded: &downloaded,
        program: &program,
        packaging: kind,
        own_version: job.own_version,
        target_version: &release.version,
        automatic: job.automatic,
        show_window: job.show_window,
        temp: job.temp,
        env: std::env::vars().collect(),
    });
    match installed {
        Ok(Installed::HandedOver(_)) => {
            report(Event::Say(t("up_wird_eingespielt")));
            Done::Quit
        }
        Ok(Installed::Swapped) => {
            let lives = (job.start)(&program).map_err(|e| e.to_string()).and_then(|mut started| {
                update_swap::alive_after(started.as_mut(), job.alive_wait, Duration::from_millis(150))
                    .map_err(|code| format!("ended with {code}"))
            });
            update_run::release_lock(job.paths);
            match lives {
                Ok(()) => Done::Quit,
                Err(why) => {
                    errors::record("updates.restart", &why);
                    update_swap::roll_back(&program);
                    update_run::remember_failed_version(job.paths, &release.version, job.now);
                    Done::Stopped(Some(t("up_zurueckgerollt")))
                }
            }
        }
        Err(why) => {
            errors::record("updates.install", &why);
            update_run::release_lock(job.paths);
            failed(&why)
        }
    }
}

/// Where the program is, what it runs on and how it is started: everything the thread of a click needs.
#[derive(Clone)]
pub struct Env {
    /// For the list of releases.
    pub net: Arc<dyn Net>,
    /// For the file itself.
    pub download_net: Arc<dyn Net>,
    pub own_version: String,
    pub appimage: Option<String>,
    pub program: PathBuf,
    pub temp: PathBuf,
    pub start: Arc<StartFn>,
    pub game_running: Arc<GameFn>,
    pub alive_wait: Duration,
}

/// What the window shows of an update that is under way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// Fetching: the version once chosen, the percentage once the file is on its way.
    Busy {
        version: String,
        percent: Option<u8>,
    },
    /// Every window of the program has to end now.
    Quit,
}

#[derive(Default)]
struct Shared {
    state: Option<State>,
    said: Vec<String>,
}

/// The click on a button that fetches: one thread at a time, its state to look at from the window.
#[derive(Clone, Default)]
pub struct Runner {
    shared: Arc<Mutex<Shared>>,
}

impl Runner {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn state(&self) -> State {
        self.lock().state.clone().unwrap_or(State::Idle)
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state(), State::Busy { .. })
    }

    /// The sentences for the player since the last call.
    pub fn take_said(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().said)
    }

    /// Fetches the newest release of the channel in a thread of its own: first a look at the list (what is
    /// remembered counts while GitHub was asked recently), then the sequence. `false` if a fetch is under way.
    pub fn start(&self, env: &Env, host: crate::paths::Host, with_prerelease: bool) -> bool {
        {
            let mut shared = self.lock();
            if matches!(shared.state, Some(State::Busy { .. })) {
                return false;
            }
            shared.state = Some(State::Busy { version: String::new(), percent: None });
        }
        let (runner, env) = (self.clone(), env.clone());
        let spawned = std::thread::Builder::new().name("update-fetch".into()).spawn_with_language(move || {
            let paths = Paths::new(host);
            let done = fetch_newest(&env, &paths, with_prerelease, &mut |event| runner.event(event), &|version| {
                runner.lock().state = Some(State::Busy { version: version.to_string(), percent: None });
            });
            runner.finish(done);
        });
        if spawned.is_err() {
            self.finish(Done::Stopped(Some(said("update_fehler", "thread"))));
        }
        true
    }

    fn event(&self, event: Event) {
        let mut shared = self.lock();
        match event {
            Event::Progress(percent) => {
                if let Some(State::Busy { percent: shown, .. }) = &mut shared.state {
                    *shown = Some(percent);
                }
            }
            Event::Say(text) => shared.said.push(text),
        }
    }

    fn finish(&self, done: Done) {
        let mut shared = self.lock();
        shared.state = Some(match done {
            Done::Quit => State::Quit,
            Done::Stopped(text) => {
                shared.said.extend(text);
                State::Idle
            }
            Done::GameStarted => State::Idle,
        });
    }
}

/// The look at the list and the sequence for the newest release of the channel (4.12 `_fetch_version`): the
/// list is asked for unless it is recent, then the newest of the channel is taken whatever runs, so that from a
/// test version the way back to the finished one stays open. `chosen` hears the version once it is known.
pub fn fetch_newest(
    env: &Env,
    paths: &Paths,
    with_prerelease: bool,
    report: &mut dyn FnMut(Event),
    chosen: &dyn Fn(&str),
) -> Done {
    let now = updater::now();
    // ⚠ Look first, then take: the remembered list can be a day old.
    updater::check(&*env.net, paths, &env.own_version, false, now);
    let cache = updater::cache_read(paths);
    let Some(release) = updater::latest(&cache.releases, with_prerelease) else {
        return Done::Stopped(Some(t("s_ub_keine_bekannt")));
    };
    chosen(&release.version);
    let job = Job {
        paths,
        download_net: &*env.download_net,
        own_version: &env.own_version,
        appimage: env.appimage.as_deref(),
        program: &env.program,
        temp: &env.temp,
        now,
        automatic: false,
        show_window: false,
        game_running: &*env.game_running,
        start: &*env.start,
        alive_wait: env.alive_wait,
    };
    run(&job, release, report)
}

#[cfg(test)]
mod tests;
