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

//! A backup of the own data whenever the version that runs is not the one that ran last, before anything is
//! renamed, moved or rewritten at the start.
//!
//! The version that ran last is a setting ([`LAST_VERSION_SETTING`]). A change counts in both directions: the
//! way back to an older version is as risky as the way forward, because the older one may read what the newer
//! one wrote differently, or not at all. A change that was not seen (the setting is missing) counts as one.
//!
//! ⚠ The setting is written only after the backup is made and read back: a backup that failed leaves the old
//! version in it, and the next start tries again. Where there is no data yet (a first start) nothing is made and
//! nothing is written.

use crate::backup::{self, WriteError};
use crate::errors;
use crate::jsonio::Value;
use crate::paths::Paths;
use crate::update_run;

/// The setting that holds the version that ran last.
pub const LAST_VERSION_SETTING: &str = "letzte_version";

/// What the look at the version found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The same version ran last: nothing to do.
    Same,
    /// A change, but no data of one's own yet: nothing to keep.
    NoData,
    /// A change, and the backup is made.
    BackedUp {
        /// The version that ran last, `None` when it was not noted.
        previous: Option<String>,
        /// The backup file.
        file: String,
    },
    /// A change, and the backup could not be made; the reason. Nothing may be renamed or moved then.
    Failed(String),
}

impl Change {
    /// May the start go on to rename, move and rewrite?
    pub fn may_go_on(&self) -> bool {
        !matches!(self, Change::Failed(_))
    }
}

/// The version that ran last, `None` when it is not noted.
pub fn last_version(paths: &Paths) -> Option<String> {
    paths.setting(LAST_VERSION_SETTING)
}

/// Compares the version that ran last with `version`, the one that runs now, and makes the backup on a change
/// ([`backup::write_before_version_change`]); notes `version` as the one that ran last once the backup is made.
pub fn back_up_on_change(paths: &Paths, version: &str) -> Change {
    let previous = last_version(paths);
    if previous.as_deref().is_some_and(|last| update_run::norm(last) == update_run::norm(version)) {
        return Change::Same;
    }
    // A data folder that is not there ([`crate::lost_storage`]) has nothing to keep and cannot hold a backup.
    if !std::path::Path::new(&paths.app_folder()).is_dir() {
        return Change::NoData;
    }
    match backup::write_before_version_change(paths, version) {
        Ok(file) => {
            if !paths.set_setting(LAST_VERSION_SETTING, Value::str(version)) {
                errors::record("version_change.note", &"the version could not be noted");
            }
            Change::BackedUp { previous, file }
        }
        Err(WriteError::Empty) => Change::NoData,
        Err(WriteError::Failed(reason)) => {
            errors::record("version_change.backup", &reason);
            Change::Failed(reason)
        }
    }
}

#[cfg(test)]
mod tests;
