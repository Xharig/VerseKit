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

//! Replacing the AppImage under Linux (4.12 `updater.install`, `restart`, `roll_back`).
//!
//! A running AppImage may be replaced as long as the file is exchanged and not written into: the running
//! process keeps the old file. In this order: the checksum once more (the file may have been replaced since it
//! was fetched), a copy of the running file next to it ([`BACKUP_SUFFIX`]; ⚠ without it no exchange: an update
//! without a way back is none), the exchange, the start of the new version with a cleaned environment, and if
//! it does not live through the first seconds, the copy goes back ([`roll_back`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::errors;
use crate::language;
use crate::updater::{discard, is_ours, still_matches};

/// The copy of the version installed before, right next to the file.
pub const BACKUP_SUFFIX: &str = ".vorher";
/// Where the output of the newly started version goes, in `Intern`: why it did not come up.
pub const RESTART_OUTPUT_FILE: &str = "neustart-ausgabe.txt";
/// How long the new version has to live before the old one steps down.
pub const ALIVE_WAIT: Duration = Duration::from_secs(3);
/// The variable that tells the new version which process it replaces: it waits for its end instead of handing
/// over to it (`window_signal::first_instance`).
pub const REPLACES_PID: &str = "VERSEKIT_ERSETZT_PID";

/// Why the AppImage was not replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwapError {
    /// The file to replace does not belong to this program (its name).
    Foreign(String),
    /// The new file does not carry the checksum it was checked against.
    Unchecked,
    /// The running file could not be copied.
    NoBackup,
    /// The exchange failed.
    Disk(String),
}

impl SwapError {
    /// The sentence for the player.
    pub fn message(&self) -> String {
        match self {
            Self::Foreign(name) => language::tf("up_fremde_datei", &[("0", language::Arg::from(name.clone()))]),
            Self::Unchecked => language::t("up_ohne_pruefung"),
            Self::NoBackup => language::t("up_sicherung_nein"),
            Self::Disk(what) => what.clone(),
        }
    }
}

/// The copy of `target` next to it.
pub fn backup_of(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(BACKUP_SUFFIX);
    PathBuf::from(name)
}

/// Copy the running file next to itself; true only for a complete copy.
pub fn backup(target: &Path) -> bool {
    let copy = backup_of(target);
    match fs::copy(target, &copy) {
        Ok(_) => fs::metadata(&copy).ok().map(|m| m.len()) == fs::metadata(target).ok().map(|m| m.len()),
        Err(error) => {
            errors::record("update_swap.backup", &error);
            false
        }
    }
}

/// Make the file runnable (`755`).
fn make_runnable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Put `new_file` in the place of `target`. Within one file system the exchange is one step; across file
/// systems (the file had to go to the temporary folder) it is copied and removed.
fn exchange(new_file: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(new_file, target) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            let staging = target.with_file_name(format!(
                ".{}.kopie",
                target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ));
            fs::copy(new_file, &staging)?;
            fs::rename(&staging, target)?;
            let _ = fs::remove_file(new_file);
            Ok(())
        }
        other => other,
    }
}

/// Replace the running AppImage `target` by `new_file`, which was checked against `checksum`. Whatever fails,
/// the new file goes and the running one stays.
///
/// # Errors
///
/// See [`SwapError`].
pub fn swap(new_file: &Path, checksum: &str, target: &Path) -> Result<(), SwapError> {
    if !is_ours(&target.to_string_lossy()) {
        discard(new_file);
        return Err(SwapError::Foreign(
            target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ));
    }
    if !still_matches(new_file, checksum) {
        discard(new_file);
        return Err(SwapError::Unchecked);
    }
    if !backup(target) {
        discard(new_file);
        return Err(SwapError::NoBackup);
    }
    if let Err(error) = exchange(new_file, target) {
        discard(new_file);
        return Err(SwapError::Disk(error.to_string()));
    }
    make_runnable(target).map_err(|e| SwapError::Disk(e.to_string()))
}

/// Put the copy of before back, when the new version died at its start. True when it worked.
pub fn roll_back(target: &Path) -> bool {
    let copy = backup_of(target);
    if !copy.is_file() {
        return false;
    }
    match fs::rename(&copy, target).and_then(|()| make_runnable(target)) {
        Ok(()) => true,
        Err(error) => {
            errors::record("update_swap.roll_back", &error);
            false
        }
    }
}

/// Start the newly installed version: a cleaned environment (⚠ the library paths of the running AppImage point
/// into its mount, which goes when it ends) that names the process it replaces (`replaces`, the one that holds
/// the single instance), its error output into `output`, in a process group of its own.
///
/// # Errors
///
/// When it cannot be started.
pub fn restart(
    target: &Path,
    env: Vec<(String, String)>,
    replaces: u32,
    output: Option<fs::File>,
) -> io::Result<std::process::Child> {
    let mut command = std::process::Command::new(target);
    command.env_clear().envs(replacing(env, replaces)).stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(output.map_or_else(std::process::Stdio::null, std::process::Stdio::from));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn()
}

/// `env` naming `replaces` as the process the new version replaces ([`REPLACES_PID`]), in place of any value
/// there was.
pub fn replacing(env: Vec<(String, String)>, replaces: u32) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = env.into_iter().filter(|(k, _)| k != REPLACES_PID).collect();
    out.push((REPLACES_PID.to_string(), replaces.to_string()));
    out
}

/// Something started whose end can be asked for.
pub trait Started {
    /// The exit code once it has ended, `None` while it lives.
    fn ended(&mut self) -> Option<i32>;
}

impl Started for std::process::Child {
    fn ended(&mut self) -> Option<i32> {
        self.try_wait().ok().flatten().map(|status| status.code().unwrap_or(-1))
    }
}

/// Does the started version live through `wait`? ⚠ Starting a program does not mean it runs: a missing
/// library ends it a second later. Only after this may the old version step down. `Err` carries the exit code.
///
/// # Errors
///
/// The exit code when it ended within `wait`.
pub fn alive_after(started: &mut dyn Started, wait: Duration, poll: Duration) -> Result<(), i32> {
    let end = Instant::now() + wait;
    loop {
        if let Some(code) = started.ended() {
            return Err(code);
        }
        if Instant::now() >= end {
            return Ok(());
        }
        std::thread::sleep(poll);
    }
}

#[cfg(test)]
mod tests;
