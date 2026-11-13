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

//! New versions installed by themselves, but never in the middle of the game (4.12 `auto_update`), and the
//! one way of installing that the click and the automatic update share.
//!
//! This file decides **whether**: every [`CHECK_INTERVAL`] a look at GitHub; a fresh release only once it is
//! [`FRESH_WAIT`] old (right after publishing the download is slow); while Star Citizen runs it waits and looks
//! every [`GAME_POLL`] whether the game has ended. **How** is [`install`]: a small backup first, then the
//! installer started directly under Windows or the exchange of the AppImage under Linux.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::paths::Paths;
use crate::update_run::{self, RunMark};
use crate::update_swap;
use crate::updater::{self, Downloaded, Packaging, Release, is_ours, still_matches};
use crate::{backup, errors, language};

/// How often GitHub is asked.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// While an update waits for the end of the game: how often to look.
pub const GAME_POLL: Duration = Duration::from_secs(20);
/// How old a release has to be before it is fetched by itself, in seconds.
pub const FRESH_WAIT: f64 = 2.0 * 60.0;

/// Seconds since 1970 of a moment `2026-11-15T18:00:00Z`; `None` in another form.
pub fn parse_moment(stamp: &str) -> Option<f64> {
    let b = stamp.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z'
    {
        return None;
    }
    let number = |from: usize, to: usize| stamp.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    // Days since 1970 of a date in the Gregorian calendar.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let of_era = y - era * 400;
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    Some((days * 86_400 + hour * 3600 + minute * 60 + second) as f64)
}

/// How many seconds the release still has to wait: 0 when it may be fetched. Without a readable moment it may
/// (rather fetch than never). ⚠ The time left, not the whole wait: a whole wait every time would block the
/// look at the end of the game.
pub fn wait_left(release: &Release, now: f64) -> u64 {
    let Some(published) = parse_moment(&release.time) else { return 0 };
    let left = FRESH_WAIT - (now - published);
    if left <= 0.0 { 0 } else { left.ceil() as u64 }
}

/// What the automatic update does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing new, or the update to it failed a short while ago.
    Nothing,
    /// The release is too fresh; again in so many seconds.
    NotYet(u64),
    /// The game runs: look again in [`GAME_POLL`].
    AfterGame,
    /// Fetch and install now.
    Install,
}

/// The next step for the release on offer.
pub fn next_step(offer: Option<&Release>, now: f64, game_running: bool, failed_recently: bool) -> Step {
    let Some(release) = offer else { return Step::Nothing };
    if failed_recently {
        return Step::Nothing;
    }
    match wait_left(release, now) {
        0 if game_running => Step::AfterGame,
        0 => Step::Install,
        left => Step::NotYet(left),
    }
}

/// The setting that holds since when a newer version is on offer and not installed: the running version and the
/// moment, `5.2.4|1794767405`. Written by every process that sees the offer, read by the overlay and the main window.
pub const OFFERED_SETTING: &str = "update_angeboten_seit";

/// From how many days of waiting the update is shown plainly in the overlay and the main window.
pub const WAIT_DAYS_SHOWN: u64 = 1;

/// Since when the running version `own` has had a newer one on offer and not installed it, in seconds since 1970;
/// `None` while nothing newer is offered. The moment is kept across starts and processes ([`OFFERED_SETTING`]): it
/// stays while the offer stays, also when a still newer version comes, and goes with the update (another running
/// version) or once nothing newer is offered. ⚠ A moment in the future (a clock set back) starts again at `now`:
/// the count must never stand still for days. Writes only when something changes.
pub fn offered_since(paths: &Paths, own: &str, offered: bool, now: f64) -> Option<f64> {
    let kept = paths.setting(OFFERED_SETTING).unwrap_or_default();
    let own = update_run::norm(own);
    let since = kept
        .split_once('|')
        .filter(|(version, _)| !own.is_empty() && *version == own)
        .and_then(|(_, stamp)| stamp.parse::<f64>().ok())
        .filter(|stamp| stamp.is_finite() && *stamp <= now);
    if !offered {
        if !kept.is_empty() {
            paths.set_setting(OFFERED_SETTING, crate::jsonio::Value::str(String::new()));
        }
        return None;
    }
    if let Some(since) = since {
        return Some(since);
    }
    paths.set_setting(OFFERED_SETTING, crate::jsonio::Value::str(format!("{own}|{}", now as i64)));
    Some(now)
}

/// Whole days the update has waited since `since` at `now`; `None` below [`WAIT_DAYS_SHOWN`], when nothing waits.
pub fn days_waiting(since: Option<f64>, now: f64) -> Option<u64> {
    let days = ((now - since?) / 86_400.0).floor();
    (days >= WAIT_DAYS_SHOWN as f64).then_some(days as u64)
}

/// `days` as the lines of a waiting update write it after „seit“ / „for“: „1 Tag“, „3 Tagen“.
pub fn days_text(days: u64) -> String {
    let count = language::Arg::Int(i64::try_from(days).unwrap_or(i64::MAX));
    language::tf(if days == 1 { "zeit_tag" } else { "zeit_tage" }, &[("0", count)])
}

/// What the end of the program does with the update on offer ([`at_exit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtExit {
    /// Nothing to install, or not now (a build of the source, an AppImage, an update that failed a short while
    /// ago, a pause after a failed download, another update under way).
    Nothing,
    /// Star Citizen runs: the update waits for the next end of the program, or for the overlay after the game.
    GameRuns,
    /// Windows is signing out or shutting down: nothing may hold it up.
    SessionEnds,
    /// Fetch and install now, without a window, and do not start the program again.
    Install,
}

/// Facts about the end of the program for [`at_exit`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExitFacts {
    /// A newer version is on offer.
    pub offered: bool,
    pub packaging: Option<Packaging>,
    pub game_running: bool,
    pub session_ending: bool,
    /// The automatic update to the version on offer failed a short while ago ([`update_run::failed_recently`]).
    pub failed_recently: bool,
    /// The pause after failed downloads of it is not over ([`update_run::download_pause_left`]).
    pub download_paused: bool,
    /// Another process installs right now ([`update_run::lock_held`]).
    pub locked: bool,
    /// Another process of the program file runs (the player started it again): it takes the update over, and the
    /// installer would close it without starting it again.
    pub another_running: bool,
}

/// Whether the update on offer is installed while the program ends, as the player ended it. Only the installer of
/// Windows: it waits for the program to end and is told not to start it again. ⚠ An AppImage is not exchanged
/// here: its new version would have to be started to know that it lives, and the program would come up again
/// right after the player ended it. The game and the end of the Windows session win over everything else.
pub fn at_exit(facts: ExitFacts) -> AtExit {
    if facts.session_ending {
        return AtExit::SessionEnds;
    }
    if !facts.offered || facts.packaging != Some(Packaging::Exe) {
        return AtExit::Nothing;
    }
    if facts.game_running {
        return AtExit::GameRuns;
    }
    if facts.failed_recently || facts.download_paused || facts.locked || facts.another_running {
        return AtExit::Nothing;
    }
    AtExit::Install
}

/// Set once a window of this process was asked by Windows whether the session may end, or told that it ends
/// ([`note_window_message`]).
static SESSION_ENDS: AtomicBool = AtomicBool::new(false);

/// `WM_QUERYENDSESSION`: Windows asks whether the session may end (also the restart manager of an installer).
pub const WM_QUERYENDSESSION: u32 = 0x0011;
/// `WM_ENDSESSION`: Windows says whether the session ends (`wparam` not zero) or goes on.
pub const WM_ENDSESSION: u32 = 0x0016;

/// Does this window message say that the session of Windows ends? Asked counts already: whoever asks may end it.
pub fn ends_session(msg: u32, wparam: usize) -> bool {
    msg == WM_QUERYENDSESSION || (msg == WM_ENDSESSION && wparam != 0)
}

/// Called by the window procedures of this process for every message they see: remembers the end of the session
/// for [`session_ending`]. The message itself goes on as before (no window of this program holds the end up).
pub fn note_window_message(msg: u32, wparam: usize) {
    if ends_session(msg, wparam) {
        SESSION_ENDS.store(true, Ordering::Relaxed);
    }
}

/// Is Windows signing out or shutting down right now? A window of this process was asked or told so
/// ([`note_window_message`]), or the system says so. Elsewhere only the first: there the session ends the program
/// before its end comes this far.
pub fn session_ending() -> bool {
    if SESSION_ENDS.load(Ordering::Relaxed) {
        return true;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};
        // SAFETY: GetSystemMetrics only reads a value of the system.
        unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// What installing set going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installed {
    /// The installer (its process number) takes over: every process of the program has to end now.
    HandedOver(u32),
    /// The AppImage is exchanged: the new version is to be started ([`update_swap::restart`]).
    Swapped,
}

/// One update to install.
pub struct Request<'a> {
    pub paths: &'a Paths,
    pub downloaded: &'a Downloaded,
    /// The running program file (Windows) or AppImage (Linux).
    pub program: &'a Path,
    pub packaging: Packaging,
    /// The running version and the one installed.
    pub own_version: &'a str,
    pub target_version: &'a str,
    pub automatic: bool,
    pub show_window: bool,
    /// The player ended the program: no window of the installer, and the program is not started again.
    pub at_exit: bool,
    /// The temporary folder: the second place of the installer's log.
    pub temp: &'a Path,
    /// The environment of the running program.
    pub env: Vec<(String, String)>,
}

/// Install a fetched update. First a small backup of one's own data (it does not stop the update when it
/// fails), then the way of the packaging. ⚠ Only a file that still carries its checksum, and only over a
/// program file that is ours.
///
/// # Errors
///
/// The sentence for the player when nothing was set going.
pub fn install(request: Request<'_>) -> Result<Installed, String> {
    let Request {
        paths,
        downloaded,
        program,
        packaging,
        own_version,
        target_version,
        automatic,
        show_window,
        at_exit,
        temp,
        env,
    } = request;
    if packaging == Packaging::Source {
        return Err(language::tf("up_fremde_datei", &[("0", language::Arg::from(program.display().to_string()))]));
    }
    if !is_ours(&program.to_string_lossy()) {
        return Err(language::tf(
            "up_fremde_datei",
            &[(
                "0",
                language::Arg::from(program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            )],
        ));
    }
    if !still_matches(&downloaded.path, &downloaded.checksum) {
        updater::discard(&downloaded.path);
        return Err(language::t("up_ohne_pruefung"));
    }
    if let Err(error) = backup::write_before_update(paths, own_version) {
        errors::record("auto_update.backup", &format!("{error:?}"));
    }
    match packaging {
        Packaging::AppImage => {
            update_swap::swap(&downloaded.path, &downloaded.checksum, program).map_err(|e| e.message())?;
            Ok(Installed::Swapped)
        }
        _ => {
            let job = update_run::SetupJob { at_exit, ..update_run::setup_job(paths, downloaded, program, temp) };
            let mark = RunMark {
                target: target_version.to_string(),
                previous: own_version.to_string(),
                installer: downloaded.path.to_string_lossy().into_owned(),
                checksum: downloaded.checksum.clone(),
                start: updater::now(),
                automatic,
                show_window,
            };
            update_run::start_setup(paths, &job, &mark, env, update_run::LIFE_SIGN_WAIT)
                .map(Installed::HandedOver)
                .map_err(|e| e.to_string())
        }
    }
}

#[cfg(test)]
mod tests;
