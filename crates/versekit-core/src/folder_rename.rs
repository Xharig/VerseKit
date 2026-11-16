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

//! The data folder under Documents and the second pointer under the name of this program
//! ([`Paths::rename_to_current_name`]), at the start on the player's own data and by the button on the page
//! "Installation & Pfade".
//!
//! ⚠⚠ The data folder is renamed, never copied, and only where that is safe: not in OneDrive, not when a
//! folder of the current name is there, not while its settings file cannot be read. A rename stays in its
//! folder, so on its drive. The settings file is read before and after; when it does not read the same, the
//! folder gets its old name back. Nothing is deleted before the new place was read: the second pointer goes to
//! the folder of the current name, is read back there, and only then does the old one go.

use std::fs;

use crate::errors;
use crate::jsonio::{self, Map, Value};
use crate::language::{self, Arg};
use crate::paths::{EARLIER_FOLDER_NAMES, NEW_FOLDER_NAME, Paths, SETTINGS_FILE};

/// The variables that name the folders of OneDrive under Windows.
const ONEDRIVE_VARS: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];

/// The beginning of the name of the lock files next to the second pointer (`Paths::save_lock_file`).
const LOCK_START: &str = "sperre-";

/// Why the data folder keeps its earlier name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hold {
    /// It lies in OneDrive.
    OneDrive,
    /// A folder of the current name is there already.
    TargetThere,
    /// Its settings file cannot be read right now.
    Busy,
    /// The system refused the rename (a file in use, controlled folder access); the error.
    Refused(String),
    /// The settings file did not read the same after the rename; the folder got its old name back.
    NotSame,
    /// Not tried yet ([`Paths::rename_preview`]): nothing speaks against it.
    NotYet,
}

/// What became of the data folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Renamed {
    /// Nothing to do: the default folder carries the current name, or the data folder is given from outside.
    Nothing,
    /// Renamed from `from` to `to`.
    Done { from: String, to: String },
    /// `folder` keeps its name; `target` is the folder of the current name.
    Stays { folder: String, target: String, why: Hold },
}

impl Renamed {
    /// The sentence for the player, `None` for [`Renamed::Nothing`].
    pub fn message(&self) -> Option<String> {
        self.message_in(language::current())
    }

    /// [`Renamed::message`] in the language `lang`.
    pub fn message_in(&self, lang: language::Lang) -> Option<String> {
        let text = |key: &str, args: &[(&str, Arg)]| Some(language::tf_in(lang, key, args));
        match self {
            Renamed::Nothing => None,
            Renamed::Done { to, .. } => text("nu_fertig", &[("0", Arg::from(to.as_str()))]),
            Renamed::Stays { folder, target, why } => {
                let (folder, target) = (Arg::from(folder.as_str()), Arg::from(target.as_str()));
                match why {
                    Hold::OneDrive => text("nu_bleibt_onedrive", &[("0", folder)]),
                    Hold::TargetThere => text("nu_bleibt_ziel", &[("0", folder), ("1", target)]),
                    Hold::Busy => text("nu_bleibt_belegt", &[("0", folder)]),
                    Hold::Refused(error) => {
                        text("nu_bleibt_fehler", &[("0", folder), ("1", Arg::from(error.as_str()))])
                    }
                    Hold::NotSame => text("nu_bleibt_anders", &[("0", folder)]),
                    Hold::NotYet => text("nu_alt", &[("0", folder)]),
                }
            }
        }
    }

    /// Can the button on the page try again? Not for OneDrive: there the rename never happens by itself.
    pub fn can_retry(&self) -> bool {
        matches!(self, Renamed::Stays { why, .. } if *why != Hold::OneDrive)
    }
}

/// The setting that remembers the folder whose staying was told at the start: it is told once per folder, the
/// page "Installation & Pfade" says it for as long as it stays.
pub const TOLD_SETTING: &str = "namensumzug_gemeldet";

/// Did the rename leave everything as it was? The folder of the new name is there, the old one is gone, and the
/// settings in it read as they did before (`before`; `after` is `None` where there were none before, `Some(None)`
/// where they cannot be read now).
fn rename_left_everything_as_it_was(
    old: &str,
    new: &str,
    before: &Option<Vec<u8>>,
    after: Option<Option<Vec<u8>>>,
) -> bool {
    fs::metadata(new).is_ok_and(|m| m.is_dir())
        && fs::metadata(old).is_err()
        && after.is_none_or(|bytes| bytes == *before)
}

impl Paths {
    /// What the start tells of `renamed`: a rename always, a folder that stays once (for the next folder
    /// again).
    pub fn rename_notice(&self, renamed: &Renamed) -> Option<String> {
        match renamed {
            Renamed::Nothing => None,
            Renamed::Done { .. } => renamed.message(),
            Renamed::Stays { folder, .. } => {
                if self.setting(TOLD_SETTING).as_deref() == Some(folder.as_str()) {
                    return None;
                }
                self.set_setting(TOLD_SETTING, Value::str(folder));
                renamed.message()
            }
        }
    }

    /// The default folder under Documents when it carries an earlier name ([`EARLIER_FOLDER_NAMES`]), and the
    /// folder of the current name next to it. `None` when the data folder is given from outside (a copy, the
    /// environment, a tool): then the Documents of the player are not touched.
    pub fn folder_to_rename(&self) -> Option<(String, String)> {
        if self.given().data.is_some() || self.host.copy.is_some() {
            return None;
        }
        let f = self.host.flavor;
        let old = self.default_folder();
        let name = f.basename(&old);
        if !EARLIER_FOLDER_NAMES.contains(&name.as_str()) || !fs::metadata(&old).is_ok_and(|m| m.is_dir()) {
            return None;
        }
        let new = f.join(&f.dirname(&old), NEW_FOLDER_NAME);
        Some((old, new))
    }

    /// What the page says of the data folder without trying anything: [`Renamed::Nothing`] when it carries the
    /// current name, else why it stays, with [`Hold::NotYet`] where nothing speaks against a rename.
    pub fn rename_preview(&self) -> Renamed {
        let Some((folder, target)) = self.folder_to_rename() else { return Renamed::Nothing };
        let why = if self.in_onedrive(&folder) {
            Hold::OneDrive
        } else if fs::symlink_metadata(&target).is_ok() {
            Hold::TargetThere
        } else {
            Hold::NotYet
        };
        Renamed::Stays { folder, target, why }
    }

    /// Does `path` lie in OneDrive? Below a folder a variable of OneDrive names, or in a folder named the way
    /// OneDrive names its own (`OneDrive`, `OneDrive - <name>`).
    pub fn in_onedrive(&self, path: &str) -> bool {
        let f = self.host.flavor;
        let full = f.normcase(&f.abspath(path, &self.host.cwd));
        let below = ONEDRIVE_VARS.iter().filter_map(|name| self.host.var_set(name)).any(|root| {
            let root = f.normcase(&f.abspath(root, &self.host.cwd));
            full == root || full.starts_with(&format!("{root}{}", f.sep()))
        });
        below
            || full.split(['\\', '/']).any(|part| {
                let part = part.to_lowercase();
                part == "onedrive" || part.starts_with("onedrive - ")
            })
    }

    /// Renames the default folder under an earlier name to the current name where that is safe, and brings the
    /// second pointer to the folder of the current name. A second call finds nothing to do.
    ///
    /// ⚠ Run before anything of the start asks for the data folder of a copy or writes into the data: right
    /// after the backup of the first start.
    pub fn rename_to_current_name(&self) -> Renamed {
        let Some((old, new)) = self.folder_to_rename() else {
            if self.given().data.is_none() && self.host.copy.is_none() {
                self.heal_pointers();
            }
            return Renamed::Nothing;
        };
        let stays = |why: Hold| Renamed::Stays { folder: old.clone(), target: new.clone(), why };
        if self.in_onedrive(&old) {
            self.move_second_pointer(None);
            return stays(Hold::OneDrive);
        }
        if fs::symlink_metadata(&new).is_ok() {
            self.move_second_pointer(None);
            return stays(Hold::TargetThere);
        }
        let f = self.host.flavor;
        let settings_at = |folder: &str| f.join_all(folder, &["Einstellungen", SETTINGS_FILE]);
        let before = match fs::read(settings_at(&old)) {
            Ok(bytes) => Some(bytes),
            Err(error) if jsonio::is_not_found(&error) => None,
            Err(_) => return stays(Hold::Busy),
        };
        if let Err(error) = fs::rename(&old, &new) {
            errors::record("folder_rename.rename", &error);
            self.move_second_pointer(None);
            return stays(Hold::Refused(error.to_string()));
        }
        let after = before.as_ref().map(|_| fs::read(settings_at(&new)).ok());
        let same = rename_left_everything_as_it_was(&old, &new, &before, after);
        // ⚠ A folder that cannot get its old name back lies under the new one: then it goes on as a rename that
        // worked, so that the pointers follow it.
        if !same && self.rename_back(&new, &old) {
            return stays(Hold::NotSame);
        }
        // ⚠ The pointers first: one that still names the old place would make a new, empty folder there. When the
        // first one cannot be written the folder gets its old name back.
        let renamed = Some((old.as_str(), new.as_str()));
        if let Err(error) = self.rewrite_paths(&settings_at(&new), renamed)
            && self.rename_back(&new, &old)
        {
            return stays(Hold::Refused(error));
        }
        self.move_second_pointer(renamed);
        self.keep_old_pointer_for(&new);
        self.forget_storage();
        self.forget_settings();
        let settings = self.settings_file();
        if settings != settings_at(&new) {
            let _ = self.rewrite_paths(&settings, renamed);
        }
        Renamed::Done { from: old, to: new }
    }

    /// Gives the folder `new` its old name `old` back; true when it lies under the old name afterwards.
    fn rename_back(&self, new: &str, old: &str) -> bool {
        let back = fs::rename(new, old);
        if let Err(error) = &back {
            errors::record("folder_rename.back", error);
        }
        self.forget_storage();
        back.is_ok()
    }

    /// After a rename that ended before the pointers were written anew (the program was ended in between): the
    /// default folder carries the current name, a folder under an earlier name is gone, and a pointer still
    /// names a place in it. Such places are written with the current name, then the second pointer goes to the
    /// folder of the current name. Without it the pointer would lead into nothing, and the data folder would be
    /// made anew and empty there.
    fn heal_pointers(&self) {
        let f = self.host.flavor;
        let current = self.default_folder();
        if f.basename(&current) != NEW_FOLDER_NAME || !fs::metadata(&current).is_ok_and(|m| m.is_dir()) {
            self.move_second_pointer(None);
            return;
        }
        let parent = f.dirname(&current);
        let gone: Vec<String> = EARLIER_FOLDER_NAMES
            .iter()
            .map(|name| f.join(&parent, name))
            .filter(|old| fs::symlink_metadata(old).is_err())
            .collect();
        let first = f.join_all(&current, &["Einstellungen", SETTINGS_FILE]);
        let (second, second_old) = self.second_pointer_places();
        for old in &gone {
            let renamed = Some((old.as_str(), current.as_str()));
            for file in [&first, &second, &second_old] {
                let _ = self.rewrite_paths(file, renamed);
            }
        }
        self.move_second_pointer(None);
        self.forget_storage();
        self.forget_settings();
        if gone.is_empty() || fs::metadata(&first).is_err() {
            return;
        }
        let settings = self.settings_file();
        if settings != first {
            for old in &gone {
                let _ = self.rewrite_paths(&settings, Some((old.as_str(), current.as_str())));
            }
        }
    }

    /// `value` with the folder `from` at its start written as `to`; `None` when it does not start with it.
    fn moved_path(&self, value: &str, from: &str, to: &str) -> Option<String> {
        let f = self.host.flavor;
        let (full, base) = (f.abspath(value, &self.host.cwd), f.abspath(from, &self.host.cwd));
        let (low, low_base) = (f.normcase(&full), f.normcase(&base));
        let rest = low.strip_prefix(&low_base)?;
        if !(rest.is_empty() || rest.starts_with(['\\', '/'])) || !full.is_char_boundary(base.len()) {
            return None;
        }
        Some(format!("{to}{}", &full[base.len()..]))
    }

    /// The top-level text values of the JSON file `file` that name a place in the folder `renamed.0`, written
    /// with `renamed.1`. Nothing is written when nothing names it or the file cannot be read. `Ok(true)` when the
    /// file was written; a failed write is recorded and given as the error.
    fn rewrite_paths(&self, file: &str, renamed: Option<(&str, &str)>) -> Result<bool, String> {
        let Some((from, to)) = renamed else { return Ok(false) };
        let _lock = jsonio::lock_saves();
        let Ok(Value::Object(map)) = jsonio::read_json(file) else { return Ok(false) };
        let mut changed = false;
        let map: Map = map
            .into_iter()
            .map(|(key, value)| match &value {
                Value::Str(text) => match self.moved_path(text, from, to) {
                    Some(moved) => {
                        changed = true;
                        (key, Value::Str(moved))
                    }
                    None => (key, value),
                },
                _ => (key, value),
            })
            .collect();
        if !changed {
            return Ok(false);
        }
        match jsonio::save_json(file, &Value::Object(map), 2, false) {
            Ok(()) => Ok(true),
            Err(error) => {
                errors::record("folder_rename.rewrite", &error);
                Err(error.to_string())
            }
        }
    }

    /// Brings the second pointer from the folder of the previous version to the folder of the current name,
    /// with the places in `renamed` written anew. True when it lies in the folder of the current name
    /// afterwards (or there was none).
    ///
    /// ⚠ Written and read back before anything else. The old pointer stays, with the same places, so that an
    /// older version started after this one still finds the data folder.
    fn move_second_pointer(&self, renamed: Option<(&str, &str)>) -> bool {
        let (current, old) = self.second_pointer_places();
        self.carry_high_water(&current, &old);
        if fs::metadata(&current).is_ok() {
            // The new one counts already ([`Paths::second_pointer`]); its places may still name the old folder.
            let _ = self.rewrite_paths(&current, renamed);
            self.mirror_second_pointer(&current, &old, false);
            return true;
        }
        let Ok(Value::Object(map)) = jsonio::read_json(&old) else {
            let none = fs::metadata(&old).is_err();
            if none {
                self.clear_old_config_folder(&old);
            }
            return none;
        };
        let map: Map = map
            .into_iter()
            .map(|(key, value)| match (&value, renamed) {
                (Value::Str(text), Some((from, to))) => {
                    let moved = self.moved_path(text, from, to);
                    (key, moved.map_or(value, Value::Str))
                }
                _ => (key, value),
            })
            .collect();
        let data = Value::Object(map);
        let f = self.host.flavor;
        let written = (|| -> std::io::Result<()> {
            jsonio::make_dirs(&f.dirname(&current))?;
            let temp = format!("{current}.tmp");
            jsonio::write_new(&temp, &jsonio::file_bytes(&data, 2))?;
            jsonio::replace_file(&temp, &current)
        })();
        if let Err(error) = written {
            errors::record("folder_rename.second_pointer", &error);
            return false;
        }
        if jsonio::read_json(&current).ok().as_ref() != Some(&data) {
            errors::record("folder_rename.second_pointer", &"read back differently");
            let _ = jsonio::remove_file(&current);
            return false;
        }
        let kept = (|| -> std::io::Result<()> {
            let temp = format!("{old}.tmp");
            jsonio::write_new(&temp, &jsonio::file_bytes(&data, 2))?;
            jsonio::replace_file(&temp, &old)
        })();
        if let Err(error) = kept {
            errors::record("folder_rename.second_pointer_old", &error);
        }
        true
    }

    /// The mark of the highest blueprint count ([`crate::collection::HIGH_WATER_FILE`]) lies next to the second
    /// pointer. Where it lies next to the old one (`old`) and not yet next to the new one (`current`), it is
    /// copied there, so that the warning "fewer blueprints than before" still holds after the move. The old one
    /// stays: an older version reads it.
    fn carry_high_water(&self, current: &str, old: &str) {
        let f = self.host.flavor;
        let name = crate::collection::HIGH_WATER_FILE;
        let (from, to) = (f.join(&f.dirname(old), name), f.join(&f.dirname(current), name));
        if f.dirname(old) == f.dirname(current) || fs::metadata(&from).is_err() || fs::metadata(&to).is_ok() {
            return;
        }
        let copied = (|| -> std::io::Result<()> {
            jsonio::make_dirs(&f.dirname(&to))?;
            let temp = format!("{to}.tmp");
            jsonio::copy2(&from, &temp)?;
            jsonio::replace_file(&temp, &to)
        })();
        if let Err(error) = copied {
            errors::record("folder_rename.high_water", &error);
        }
    }

    /// The old second pointer `old` written like the new one `current`, where it reads differently: it names the
    /// same places, so that an older version started after this one finds the data folder. Without `force` only
    /// where the old one is there.
    fn mirror_second_pointer(&self, current: &str, old: &str, force: bool) {
        if !force && fs::metadata(old).is_err() {
            return;
        }
        let (Ok(data), read_old) = (jsonio::read_json(current), jsonio::read_json(old)) else { return };
        if read_old.as_ref().ok() == Some(&data) {
            return;
        }
        let kept = (|| -> std::io::Result<()> {
            jsonio::make_dirs(&self.host.flavor.dirname(old))?;
            let temp = format!("{old}.tmp");
            jsonio::write_new(&temp, &jsonio::file_bytes(&data, 2))?;
            jsonio::replace_file(&temp, old)
        })();
        if let Err(error) = kept {
            errors::record("folder_rename.second_pointer_old", &error);
        }
    }

    /// After a rename of the default folder to `new`: both second pointers name where the data are, even where
    /// there was none before. Else an older version started later looks for the folder under the earlier name,
    /// finds nothing and makes a new, empty one. Where they are there they stay as they are.
    ///
    /// The place they name: the one the visible pointer of `new` names, else `new` itself.
    fn keep_old_pointer_for(&self, new: &str) {
        let (current, old) = self.second_pointer_places();
        if fs::metadata(&current).is_ok() {
            self.mirror_second_pointer(&current, &old, true);
            return;
        }
        if fs::metadata(&old).is_ok() {
            return;
        }
        let f = self.host.flavor;
        let place = self
            .read_pointer(&f.join_all(new, &["Einstellungen", SETTINGS_FILE]))
            .unwrap_or_else(|| new.to_string());
        if let Err(error) = jsonio::make_dirs(&f.dirname(&old)) {
            errors::record("folder_rename.second_pointer_old", &error);
            return;
        }
        self.write_second_pointers(&Value::Str(place), None);
    }

    /// The folder of the old second pointer `old_pointer` goes when nothing but lock files of earlier runs lie in
    /// it (they are no data); with anything else in it, it stays as it is.
    fn clear_old_config_folder(&self, old_pointer: &str) {
        let f = self.host.flavor;
        let folder = f.dirname(old_pointer);
        let Ok(entries) = fs::read_dir(&folder) else { return };
        let names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        if !names.iter().all(|n| n.starts_with(LOCK_START) && n.ends_with(".lock")) {
            return;
        }
        for name in &names {
            let _ = fs::remove_file(f.join(&folder, name));
        }
        let _ = fs::remove_dir(&folder);
    }
}

#[cfg(test)]
mod tests;
