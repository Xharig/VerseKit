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

//! Backups the program writes by itself: once a day at the start, and after the game has ended (the game writes the
//! active binding and its settings when it ends). They hold the same as a backup made by hand, with the files of the
//! game of every channel ([`backup_game::bindings_all`]), and lie in `Intern/sicherungen/`.
//!
//! ⚠ Only the newest [`keep`] of them stay; the newest one is never removed, whatever the setting says. Only files of
//! the exact form of their names are counted and removed ([`is_own`]): nothing the player put into the folder.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;

use crate::backup::{self, WriteError};
use crate::game_start::{self, RunState};
use crate::jsonio::{self, Value};
use crate::language::SpawnWithLanguage as _;
use crate::paths::Paths;
use crate::{backup_game, errors, updater};

/// The folder in `Intern` with the backups written by themselves.
pub const FOLDER: &str = "sicherungen";
/// The start of their file names.
pub const PREFIX: &str = "auto";
/// The setting: a backup at the first start of a day (on unless switched off).
pub const SETTING_START: &str = "sicherung_beim_start";
/// The setting: a backup after the game has ended (on unless switched off).
pub const SETTING_GAME_END: &str = "sicherung_nach_spielende";
/// The setting: how many of them stay.
pub const SETTING_KEEP: &str = "sicherung_behalten";
/// How many stay where nothing is set.
pub const KEEP_DEFAULT: i64 = 10;
/// The fewest that can be set: the newest always stays.
pub const KEEP_MIN: i64 = 1;
/// The most that can be set.
pub const KEEP_MAX: i64 = 50;
/// How often the watch asks whether the game runs.
pub const WATCH_EVERY: Duration = Duration::from_secs(60);

/// What made a backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The first start of the program on a day.
    Start,
    /// The game has ended.
    GameEnd,
}

impl Trigger {
    /// The word in the file name.
    pub fn word(self) -> &'static str {
        match self {
            Trigger::Start => "start",
            Trigger::GameEnd => "spielende",
        }
    }

    fn setting(self) -> &'static str {
        match self {
            Trigger::Start => SETTING_START,
            Trigger::GameEnd => SETTING_GAME_END,
        }
    }
}

/// The form of the names of the backups written by themselves.
fn name_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^auto-\d{4}-\d{2}-\d{2}-\d{6}-(start|spielende)-[^\\/]+\.zip$").expect("pattern")
    })
}

/// Is this a file name of a backup written by itself?
pub fn is_own(name: &str) -> bool {
    name_pattern().is_match(name)
}

/// Is the backup of `trigger` switched on?
pub fn enabled(paths: &Paths, trigger: Trigger) -> bool {
    paths.setting_bool(trigger.setting(), true)
}

/// Switches the backup of `trigger` on or off. Gives whether it was saved.
pub fn set_enabled(paths: &Paths, trigger: Trigger, on: bool) -> bool {
    paths.set_setting(trigger.setting(), Value::Bool(on))
}

/// How many backups written by themselves stay, within [`KEEP_MIN`] and [`KEEP_MAX`].
pub fn keep(paths: &Paths) -> usize {
    usize::try_from(paths.setting_int(SETTING_KEEP, KEEP_DEFAULT, Some(KEEP_MIN), Some(KEEP_MAX))).unwrap_or(1)
}

/// Sets how many stay (held within the limits). Gives whether it was saved.
///
/// ⚠ Nothing is removed here: a slip of the slider must not cost backups. The folder is cut at the next backup
/// written by itself.
pub fn set_keep(paths: &Paths, count: i64) -> bool {
    paths.set_setting(SETTING_KEEP, Value::Int(count.clamp(KEEP_MIN, KEEP_MAX).to_string()))
}

/// The folder of the backups written by themselves.
pub fn folder(paths: &Paths) -> String {
    paths.app_file(FOLDER)
}

/// The backups written by themselves in `folder`, by file name, oldest first.
pub fn list(folder: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| is_own(name))
                .collect()
        })
        .unwrap_or_default();
    // The time stands right after the prefix, with leading zeros: the name sorts by it.
    names.sort();
    names
}

/// Removes the oldest backups written by themselves until `keep` are left; the newest always stays. Gives the names
/// removed.
pub fn prune(folder: &str, keep: usize) -> Vec<String> {
    let names = list(folder);
    let surplus = names.len().saturating_sub(keep.max(1));
    let mut removed = Vec::new();
    for old in &names[..surplus] {
        match jsonio::remove_file(&Path::new(folder).join(old).to_string_lossy()) {
            Ok(()) => removed.push(old.clone()),
            Err(error) if jsonio::is_not_found(&error) => {}
            Err(error) => errors::record("backup_auto.prune", &error),
        }
    }
    removed
}

/// Is a backup of the start due: switched on, and none written by itself today?
pub fn due_at_start(paths: &Paths) -> bool {
    if !enabled(paths, Trigger::Start) {
        return false;
    }
    let (year, month, day, ..) = jsonio::local_time();
    let today = format!("{PREFIX}-{year:04}-{month:02}-{day:02}-");
    !list(&folder(paths)).iter().any(|name| name.starts_with(&today))
}

/// Writes a backup of `trigger` now, where it is switched on, and cuts the folder to [`keep`]. Gives the file written
/// (`None` where it is switched off).
///
/// # Errors
///
/// As [`backup::write`]: [`WriteError::Empty`] where there is nothing of one's own yet.
pub fn write(paths: &Paths, version: &str, trigger: Trigger) -> Result<Option<String>, WriteError> {
    if !enabled(paths, trigger) {
        return Ok(None);
    }
    write_now(paths, version, trigger).map(Some)
}

/// [`write`] without asking the setting.
fn write_now(paths: &Paths, version: &str, trigger: Trigger) -> Result<String, WriteError> {
    let folder = folder(paths);
    jsonio::make_dirs(&folder).map_err(|e| WriteError::Failed(e.to_string()))?;
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    let version = if version.is_empty() { "unbekannt" } else { version };
    let name = format!(
        "{PREFIX}-{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}-{}-{version}.zip",
        trigger.word()
    );
    let target = Path::new(&folder).join(name).to_string_lossy().into_owned();
    backup::write_marked(paths, &target, version, &backup_game::bindings_all(paths), backup::MARKER)?;
    prune(&folder, keep(paths));
    Ok(target)
}

/// Tells from the state of the game, asked again and again, when it has ended: once after it was seen running and
/// then is seen quiet. A state nobody can tell changes nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct GameWatch {
    seen_running: bool,
}

impl GameWatch {
    /// The next state; `true` once when the game has ended.
    pub fn step(&mut self, state: RunState) -> bool {
        match state {
            RunState::Running => {
                self.seen_running = true;
                false
            }
            RunState::Idle => std::mem::take(&mut self.seen_running),
            RunState::Unknown => false,
        }
    }
}

/// Writes a backup of `trigger` under the mark that holds an update back; a failure goes into the error log.
fn attempt(paths: &Paths, version: &str, trigger: Trigger) {
    let _mark = backup::RunningMark::begin(paths, updater::now());
    match write(paths, version, trigger) {
        Ok(_) | Err(WriteError::Empty) => {}
        Err(WriteError::Failed(why)) => errors::note("backup_auto", &why),
    }
}

/// Starts the thread of the backups written by themselves: the one of the start where it is due, then the watch for
/// the end of the game, as long as the program runs.
pub fn spawn(paths: Paths, version: &'static str) {
    let started = std::thread::Builder::new().name("sicherung-auto".into()).spawn_with_language(move || {
        if due_at_start(&paths) {
            attempt(&paths, version, Trigger::Start);
        }
        let mut watch = GameWatch::default();
        loop {
            std::thread::sleep(WATCH_EVERY);
            if watch.step(game_start::run_state(&paths)) {
                attempt(&paths, version, Trigger::GameEnd);
            }
        }
    });
    if let Err(error) = started {
        errors::record("backup_auto.spawn", &error);
    }
}

#[cfg(test)]
mod tests;
