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

//! A data folder that a pointer names and that is not there ([`Paths::app_folder`]).
//!
//! ⚠⚠ Such a folder is never made anew and empty: the pointer would count as valid from then on, the data that
//! lie elsewhere would look lost, and nobody would be told. Instead the usual places are searched for folders
//! that carry the mark of the program ([`data_removal::marked`]). Exactly one of them is taken and the pointers
//! are written anew; with none or several the folder is left alone, nothing is created below it
//! ([`jsonio::refuse_creating`]), and the player is told which places were found ([`Paths::lost_message`]).
//!
//! ⚠⚠ A folder whose drive, share or mount point does not answer is not missing: it is waited for a few seconds
//! ([`ROOT_WAIT`]), and when the root still does not answer no other folder is looked for or taken, no pointer is
//! written, nothing is created below it, and the player is told the drive is not reachable.

use std::time::{Duration, Instant};

use crate::jsonio::{self, Value};
use crate::language::{self, Arg};
use crate::paths::{EARLIER_FOLDER_NAMES, NEW_FOLDER_NAME, Paths, SETTINGS_FILE, STORAGE_KEY};
use crate::pypath::Flavor;
use crate::data_removal;

/// How long the answer for one missing folder stands before the places are looked at again.
const MEMO: Duration = Duration::from_secs(5);
/// How long a root that does not answer is waited for, once per missing folder and run.
const ROOT_WAIT: Duration = if cfg!(test) { Duration::from_millis(60) } else { Duration::from_secs(4) };
/// The pause between two questions to a root that does not answer.
const ROOT_PAUSE: Duration = if cfg!(test) { Duration::from_millis(10) } else { Duration::from_millis(500) };

/// What was found out for a pointer to a folder that is not there.
#[derive(Clone, Debug)]
pub(crate) struct LostMemo {
    /// The folder the pointer names.
    pub(crate) missing: String,
    /// The folders with data of the program found elsewhere.
    pub(crate) found: Vec<String>,
    /// The one folder taken instead, when exactly one was found.
    pub(crate) taken: Option<String>,
    /// The root of `missing` (drive, share, mount point) that did not answer; nothing was looked for then.
    pub(crate) unreachable: Option<String>,
    pub(crate) at: Instant,
}

/// The data folder the pointers name is not there, and no single other one was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lost {
    /// The folder the pointer names.
    pub missing: String,
    /// The folders with data of the program found at the usual places; empty or several.
    pub found: Vec<String>,
    /// The drive, share or mount point of `missing` that does not answer; `found` is empty then.
    pub unreachable: Option<String>,
}

/// The root of `path` whose absence means its drive is not there rather than the folder: the drive (`E:\`) or share
/// (`\\server\share\`) under Windows, the mount point under the usual places for them (`/media/<user>/<disk>`,
/// `/run/media/<user>/<disk>`, `/mnt/<disk>`, `/Volumes/<disk>`) elsewhere. `None` for a path without one.
pub(crate) fn storage_root(flavor: Flavor, path: &str) -> Option<String> {
    match flavor {
        Flavor::Windows => {
            let (drive, _, _) = flavor.splitroot(path);
            (!drive.is_empty()).then(|| format!("{}\\", drive.replace('/', "\\")))
        }
        Flavor::Posix => {
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            if !path.starts_with('/') {
                return None;
            }
            let depth = match parts.as_slice() {
                ["media", _, _, ..] => 3,
                ["run", "media", _, _, ..] => 4,
                ["mnt", _, ..] | ["Volumes", _, ..] => 2,
                _ => return None,
            };
            Some(format!("/{}", parts[..depth].join("/")))
        }
    }
}

impl Paths {
    /// Whether the root of `folder` ([`storage_root`]) does not answer right now; asked once, without waiting.
    pub(crate) fn root_down(&self, folder: &str) -> bool {
        storage_root(self.host.flavor, folder).is_some_and(|root| !self.host.root_ready(&root))
    }

    /// The root of `folder` that does not answer, after waiting [`ROOT_WAIT`] for it when `wait`; `None` when it
    /// answers or `folder` has none.
    fn unready_root(&self, folder: &str, wait: bool) -> Option<String> {
        let root = storage_root(self.host.flavor, folder)?;
        let start = Instant::now();
        loop {
            if self.host.root_ready(&root) {
                return None;
            }
            if !wait || start.elapsed() >= ROOT_WAIT {
                return Some(root);
            }
            std::thread::sleep(ROOT_PAUSE);
        }
    }

    /// The folder to use when the pointer names `missing` and nothing is there: the one folder with data found
    /// elsewhere (the pointers are written anew), else `missing` itself, left as it is. With a root of `missing`
    /// that does not answer, always `missing` itself, and nothing is looked for.
    pub(crate) fn storage_missing(&self, missing: &str) -> String {
        let before = self.lost_memo().filter(|memo| self.same_place(&memo.missing, missing));
        if let Some(memo) = &before
            && memo.at.elapsed() < MEMO
        {
            return memo.taken.clone().unwrap_or_else(|| missing.to_string());
        }
        let memo = |found: Vec<String>, taken: Option<String>, unreachable: Option<String>| {
            Some(LostMemo { missing: missing.to_string(), found, taken, unreachable, at: Instant::now() })
        };
        // ⚠ The answer "nothing found" stands before the search: what the search reads may ask for the data folder
        // again (the error log), and that must not start a second search.
        let waited = before.and_then(|memo| memo.unreachable);
        self.set_lost_memo(memo(Vec::new(), None, waited.clone()));
        jsonio::refuse_creating(missing, true);
        if let Some(root) = self.unready_root(missing, waited.is_none()) {
            self.set_lost_memo(memo(Vec::new(), None, Some(root)));
            return missing.to_string();
        }
        if fs_is_dir(missing) {
            // The drive answered while it was waited for, and the folder is there.
            self.set_lost_memo(None);
            jsonio::refuse_creating(missing, false);
            return missing.to_string();
        }
        let found = self.data_folders_besides(missing);
        let taken = match found.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        };
        self.set_lost_memo(memo(found, taken.clone(), None));
        match taken {
            Some(only) => {
                jsonio::refuse_creating(missing, false);
                // The pointers name the place the data are at: written anew, they hold at the next start too.
                self.set_setting(STORAGE_KEY, Value::str(&only));
                self.forget_storage();
                only
            }
            None => {
                jsonio::refuse_creating(missing, true);
                missing.to_string()
            }
        }
    }

    /// Whether the data folder the pointers name is missing and no other one was taken in its place; looks at
    /// the folder in use first.
    pub fn storage_lost(&self) -> Option<Lost> {
        let _ = self.app_folder();
        let memo = self.lost_memo()?;
        (memo.taken.is_none() && !fs_is_dir(&memo.missing))
            .then_some(Lost { missing: memo.missing, found: memo.found, unreachable: memo.unreachable })
    }

    /// The sentence for the player about [`Paths::storage_lost`]; `None` when the data folder is there.
    pub fn lost_message(&self) -> Option<String> {
        self.lost_message_in(language::current())
    }

    /// [`Paths::lost_message`] in the language `lang`.
    pub fn lost_message_in(&self, lang: language::Lang) -> Option<String> {
        let lost = self.storage_lost()?;
        let missing = Arg::from(lost.missing.as_str());
        if let Some(root) = &lost.unreachable {
            return Some(language::tf_in(lang, "ablage_laufwerk_fehlt", &[("0", missing), ("1", Arg::from(root.as_str()))]));
        }
        Some(if lost.found.is_empty() {
            language::tf_in(lang, "ablage_fehlt", &[("0", missing)])
        } else {
            let places = Arg::from(lost.found.join(", "));
            language::tf_in(lang, "ablage_fehlt_funde", &[("0", missing), ("1", places)])
        })
    }

    fn same_place(&self, a: &str, b: &str) -> bool {
        let f = self.host.flavor;
        f.normcase(&f.abspath(a, &self.host.cwd)) == f.normcase(&f.abspath(b, &self.host.cwd))
    }

    /// The folders with the mark of the program at the usual places, `missing` and a folder on the network left
    /// out, each once: the default folder under Documents under every name it had, and every place a pointer
    /// names.
    fn data_folders_besides(&self, missing: &str) -> Vec<String> {
        let f = self.host.flavor;
        let documents = self.documents();
        let names: Vec<&str> = std::iter::once(NEW_FOLDER_NAME).chain(EARLIER_FOLDER_NAMES).collect();
        let mut places: Vec<String> = names.iter().map(|name| f.join(&documents, name)).collect();
        let (second, second_old) = self.second_pointer_places();
        let mut pointers = vec![self.pointer_file(), second, second_old];
        pointers.extend(names.iter().map(|name| f.join_all(&documents, &[name, "Einstellungen", SETTINGS_FILE])));
        places.extend(pointers.iter().filter_map(|file| self.read_pointer(file)).map(|v| self.host.expanduser(&v)));
        let mut found: Vec<String> = Vec::new();
        for place in places {
            let usable = fs_is_dir(&place)
                && !self.same_place(&place, missing)
                && !found.iter().any(|known| self.same_place(known, &place))
                && !data_removal::on_network(&place)
                && data_removal::marked(self, &place);
            if usable {
                found.push(place);
            }
        }
        found
    }
}

fn fs_is_dir(path: &str) -> bool {
    !path.is_empty() && std::fs::metadata(path).is_ok_and(|m| m.is_dir())
}

#[cfg(test)]
mod tests;
