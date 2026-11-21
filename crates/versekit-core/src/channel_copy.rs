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

//! Copies the settings of one game channel to another: the graphics (`attributes.xml`), the controls
//! (`actionmaps.xml` and the saved profiles of the `mappings` folder) and the own lines of the `user.cfg`.
//!
//! * **Everything is read and checked first, then written:** a source file that is not readable, not XML or a
//!   `user.cfg` with an invalid line stops the copy before the target is touched.
//! * **Nothing is written while the game runs, or when nobody can tell** (`game_files`), and every file of the
//!   target that is replaced gets a copy beside it first (`<file>.versekit-<time>`, ten stay). A file whose
//!   content is already the same is left alone.
//! * **The language lines stay the target's own:** `g_language` and `g_languageAudio` of the `user.cfg` belong to
//!   the program (`usercfg::MANAGED`) and are never carried over; saved profiles that only the target has stay.

use std::path::Path;

use crate::backup;
use crate::errors;
use crate::game_files::{self, WriteError};
use crate::graphics;
use crate::joysticks;
use crate::paths::Paths;
use crate::usercfg::{self, SetError};

/// What can be copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// `attributes.xml`: graphics, view angle and the other settings of the game.
    Graphics,
    /// `actionmaps.xml` and the saved profiles.
    Controls,
    /// The own lines of the `user.cfg`.
    UserCfg,
}

/// All parts, in the order they are shown and copied.
pub const PARTS: [Part; 3] = [Part::Graphics, Part::Controls, Part::UserCfg];

/// Which parts a channel has something to give for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Available {
    pub graphics: bool,
    pub controls: bool,
    pub user_cfg: bool,
}

impl Available {
    /// Is there something of `part`?
    pub fn has(&self, part: Part) -> bool {
        match part {
            Part::Graphics => self.graphics,
            Part::Controls => self.controls,
            Part::UserCfg => self.user_cfg,
        }
    }
}

/// What the channel in `folder` has to give.
pub fn available(paths: &Paths, folder: &str) -> Available {
    Available {
        graphics: graphics::attribute_file(paths, Some(folder)).is_some(),
        controls: !controls_in(paths, folder).is_empty(),
        user_cfg: usercfg::read(folder).is_some_and(|text| !usercfg::own_lines(&text).is_empty()),
    }
}

/// What was done for one part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartReport {
    pub part: Part,
    /// The names of the files that were written.
    pub written: Vec<String>,
    /// How many files were already the same and stayed as they were.
    pub unchanged: usize,
    /// The copies of the replaced files of the target.
    pub backups: Vec<String>,
}

impl PartReport {
    fn new(part: Part) -> PartReport {
        PartReport { part, written: Vec::new(), unchanged: 0, backups: Vec::new() }
    }
}

/// What a copy did, part by part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub parts: Vec<PartReport>,
}

impl Report {
    /// How many files were written in all.
    pub fn written(&self) -> usize {
        self.parts.iter().map(|p| p.written.len()).sum()
    }

    /// How many files were already the same.
    pub fn unchanged(&self) -> usize {
        self.parts.iter().map(|p| p.unchanged).sum()
    }

    /// How many copies of replaced target files were made.
    pub fn backups(&self) -> usize {
        self.parts.iter().map(|p| p.backups.len()).sum()
    }
}

/// Why nothing (or not everything) was copied.
#[derive(Debug)]
pub enum CopyError {
    /// No part chosen.
    NothingChosen,
    /// Source and target are the same folder (also as a link).
    SameFolder,
    /// The source has nothing of the chosen part; nothing was written.
    NothingToGive(Part),
    /// The game runs or cannot be told: the reason, for the player. Nothing was written.
    GameNotQuiet(String),
    /// A source file is damaged; nothing was written.
    Damaged(Part, String),
    /// The writing failed at `part`; `done` holds what was written up to then.
    Failed { part: Part, reason: String, done: Report },
}

/// A file to carry over: its name in the list, where it goes, its content.
struct Item {
    name: String,
    to: String,
    bytes: Vec<u8>,
}

/// The place of a file in a channel that has none yet (the spelling the game itself uses).
fn default_place(paths: &Paths, folder: &str, rest: &[&str]) -> String {
    let mut parts = vec!["USER", "Client", "0"];
    parts.extend_from_slice(rest);
    paths.host.flavor.join_all(folder, &parts)
}

/// The controls of the channel: the active binding and every saved profile (not the `attributes.xml`, which is
/// the graphics part). Name in the list (`actionmaps.xml`, `mappings/<name>.xml`) and the full path.
fn controls_in(paths: &Paths, folder: &str) -> Vec<(String, String)> {
    backup::bindings(paths, Some(folder))
        .into_iter()
        .filter_map(|binding| {
            let rest = binding.name.strip_prefix(backup::CONTROLS)?.to_string();
            (rest != "attributes.xml").then_some((rest, binding.full))
        })
        .collect()
}

fn same_folder(a: &str, b: &str) -> bool {
    let (a, b) = (Path::new(a), Path::new(b));
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn graphics_items(paths: &Paths, source: &str, target: &str) -> Result<Vec<Item>, CopyError> {
    let Some(from) = graphics::attribute_file(paths, Some(source)) else {
        return Err(CopyError::NothingToGive(Part::Graphics));
    };
    let bytes = std::fs::read(&from).map_err(|e| CopyError::Damaged(Part::Graphics, e.to_string()))?;
    if !backup::xml_of(&bytes, None) {
        return Err(CopyError::Damaged(Part::Graphics, "attributes.xml".to_string()));
    }
    let to = graphics::attribute_file(paths, Some(target))
        .unwrap_or_else(|| default_place(paths, target, &["Profiles", "default", "attributes.xml"]));
    Ok(vec![Item { name: "attributes.xml".to_string(), to, bytes }])
}

fn controls_items(paths: &Paths, source: &str, target: &str) -> Result<Vec<Item>, CopyError> {
    let found = controls_in(paths, source);
    if found.is_empty() {
        return Err(CopyError::NothingToGive(Part::Controls));
    }
    let flavor = paths.host.flavor;
    let active_target = joysticks::actionmaps_path(paths, Some(target));
    let profile_folder = joysticks::all_mapping_folders(paths, Some(target))
        .into_iter()
        .next()
        .unwrap_or_else(|| default_place(paths, target, &["controls", "mappings"]));
    let mut items = Vec::new();
    for (name, full) in found {
        let bytes = std::fs::read(&full).map_err(|e| CopyError::Damaged(Part::Controls, format!("{name}: {e}")))?;
        if !backup::xml_of(&bytes, Some("ActionMaps")) {
            return Err(CopyError::Damaged(Part::Controls, name));
        }
        let to = if name == "actionmaps.xml" {
            active_target
                .clone()
                .unwrap_or_else(|| default_place(paths, target, &["Profiles", "default", "actionmaps.xml"]))
        } else {
            // Only the plain file name: a name must not lead out of the folder of the profiles.
            let plain = flavor.basename(&name);
            if !backup::profile_name_ok(&plain, flavor) {
                return Err(CopyError::Damaged(Part::Controls, name));
            }
            flavor.join_all(&profile_folder, &[&plain])
        };
        items.push(Item { name, to, bytes });
    }
    Ok(items)
}

/// Writes the files of one part; each is copied beside first, and one that is the same is left alone.
fn write_items(paths: &Paths, part: Part, items: Vec<Item>) -> Result<PartReport, (PartReport, String)> {
    let mut report = PartReport::new(part);
    for item in items {
        if std::fs::read(&item.to).is_ok_and(|now| now == item.bytes) {
            report.unchanged += 1;
            continue;
        }
        let made = Path::new(&item.to).parent().is_none_or(|parent| paths.ensure_folder(&parent.to_string_lossy()));
        if !made {
            return Err((report, format!("{}: folder", item.name)));
        }
        match game_files::write(paths, &item.to, &item.bytes) {
            Ok(copy) => {
                report.written.push(item.name);
                report.backups.extend(copy);
            }
            Err(error) => {
                if let WriteError::Failed(io) = &error {
                    errors::record("channel_copy.write", io);
                }
                return Err((report, error.reason()));
            }
        }
    }
    Ok(report)
}

/// The own lines of the `user.cfg` of the source, checked.
fn user_cfg_lines(source: &str) -> Result<Vec<String>, CopyError> {
    let text = usercfg::read_checked(source)
        .map_err(|e| CopyError::Damaged(Part::UserCfg, e.to_string()))?
        .ok_or(CopyError::NothingToGive(Part::UserCfg))?;
    let lines = usercfg::own_lines(&text);
    if lines.is_empty() {
        return Err(CopyError::NothingToGive(Part::UserCfg));
    }
    if let Some((number, _)) = usercfg::check(&lines).first() {
        return Err(CopyError::Damaged(Part::UserCfg, format!("user.cfg: {number}")));
    }
    Ok(lines)
}

fn write_user_cfg(paths: &Paths, target: &str, lines: &[String]) -> Result<PartReport, (PartReport, String)> {
    let mut report = PartReport::new(Part::UserCfg);
    let file = Path::new(target).join(usercfg::FILE).to_string_lossy().into_owned();
    let before = game_files::backups(&file);
    match usercfg::write_own(paths, target, lines) {
        Ok(true) => {
            report.written.push(usercfg::FILE.to_string());
            report.backups = game_files::backups(&file).into_iter().filter(|b| !before.contains(b)).collect();
            Ok(report)
        }
        Ok(false) => {
            report.unchanged = 1;
            Ok(report)
        }
        Err(SetError::Write(error)) => Err((report, error.reason())),
        Err(SetError::Bad(bad)) => Err((report, format!("user.cfg: {}", bad.first().map_or(0, |(n, _)| *n)))),
    }
}

/// Copies the chosen `parts` from the channel in `source` to the one in `target` (both game folders).
///
/// # Errors
///
/// [`CopyError`]: see there; before anything is written, everything is read and checked.
pub fn copy(paths: &Paths, source: &str, target: &str, parts: &[Part]) -> Result<Report, CopyError> {
    if parts.is_empty() {
        return Err(CopyError::NothingChosen);
    }
    if same_folder(source, target) {
        return Err(CopyError::SameFolder);
    }
    // Read and check everything first.
    let mut plan: Vec<(Part, Vec<Item>, Option<Vec<String>>)> = Vec::new();
    for part in PARTS.into_iter().filter(|p| parts.contains(p)) {
        match part {
            Part::Graphics => plan.push((part, graphics_items(paths, source, target)?, None)),
            Part::Controls => plan.push((part, controls_items(paths, source, target)?, None)),
            Part::UserCfg => plan.push((part, Vec::new(), Some(user_cfg_lines(source)?))),
        }
    }
    game_files::check_quiet(paths).map_err(|e| CopyError::GameNotQuiet(e.reason()))?;
    let mut done = Report::default();
    for (part, items, lines) in plan {
        let result = match lines {
            Some(lines) => write_user_cfg(paths, target, &lines),
            None => write_items(paths, part, items),
        };
        match result {
            Ok(report) => done.parts.push(report),
            Err((partial, reason)) => {
                done.parts.push(partial);
                return Err(CopyError::Failed { part, reason, done });
            }
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests;
