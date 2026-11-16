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

use std::time::{Duration, Instant};

use crate::jsonio::{self, Value};
use crate::language::{self, Arg};
use crate::paths::{EARLIER_FOLDER_NAMES, NEW_FOLDER_NAME, Paths, SETTINGS_FILE, STORAGE_KEY};
use crate::data_removal;

/// How long the answer for one missing folder stands before the places are looked at again.
const MEMO: Duration = Duration::from_secs(5);

/// What was found out for a pointer to a folder that is not there.
#[derive(Clone, Debug)]
pub(crate) struct LostMemo {
    /// The folder the pointer names.
    pub(crate) missing: String,
    /// The folders with data of the program found elsewhere.
    pub(crate) found: Vec<String>,
    /// The one folder taken instead, when exactly one was found.
    pub(crate) taken: Option<String>,
    pub(crate) at: Instant,
}

/// The data folder the pointers name is not there, and no single other one was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lost {
    /// The folder the pointer names.
    pub missing: String,
    /// The folders with data of the program found at the usual places; empty or several.
    pub found: Vec<String>,
}

impl Paths {
    /// The folder to use when the pointer names `missing` and nothing is there: the one folder with data found
    /// elsewhere (the pointers are written anew), else `missing` itself, left as it is.
    pub(crate) fn storage_missing(&self, missing: &str) -> String {
        if let Some(memo) = self.lost_memo()
            && self.same_place(&memo.missing, missing)
            && memo.at.elapsed() < MEMO
        {
            return memo.taken.unwrap_or_else(|| missing.to_string());
        }
        // ⚠ The answer "nothing found" stands before the search: what the search reads may ask for the data folder
        // again (the error log), and that must not start a second search.
        let at = Instant::now();
        self.set_lost_memo(Some(LostMemo { missing: missing.to_string(), found: Vec::new(), taken: None, at }));
        let found = self.data_folders_besides(missing);
        let taken = match found.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        };
        self.set_lost_memo(Some(LostMemo { missing: missing.to_string(), found, taken: taken.clone(), at }));
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
            .then_some(Lost { missing: memo.missing, found: memo.found })
    }

    /// The sentence for the player about [`Paths::storage_lost`]; `None` when the data folder is there.
    pub fn lost_message(&self) -> Option<String> {
        self.lost_message_in(language::current())
    }

    /// [`Paths::lost_message`] in the language `lang`.
    pub fn lost_message_in(&self, lang: language::Lang) -> Option<String> {
        let lost = self.storage_lost()?;
        let missing = Arg::from(lost.missing.as_str());
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
