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
            let job = update_run::setup_job(paths, downloaded, program, temp);
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
