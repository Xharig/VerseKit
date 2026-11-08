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

//! The nine processing methods of the refineries as the game data state them. A `RefiningProcess` record names
//! the text of its method (`processName`); the line the terminal shows below it (the same key with `_Details`,
//! „High Yield // High Cost // Slightly Fast“) gives the three levels the mining page compares
//! ([`versekit_core::refining::Methods`]). The record itself only knows three speeds and three qualities, and no
//! costs: the line is finer and has all three.

use std::path::Path;

use p4kx_dataforge::DataForge;
use versekit_core::jsonio::{Newline, Value as Json};

use crate::fields::locale_key;
use crate::{Error, Localization};

/// The file the levels are kept in, beside the cache of the values.
pub use versekit_core::refining::GAME_FILE as FILE;
use versekit_core::refining::GAME_FORMAT;

/// The record type of a processing method.
const PROCESS_TYPE: &str = "RefiningProcess";

/// One processing method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    /// Its name in the game, in English (`Pyrometric Chromalysis`).
    pub name: String,
    /// Yield, speed and cost advantage, as in [`versekit_core::refining::METHODS`].
    pub levels: [u8; 3],
}

/// The levels of a line of the terminal: yield and costs high, moderate or low (low costs are the largest
/// advantage), the speed in nine words onto four levels: the slowest 0, slow and very slow 1, slightly slow and
/// moderate 2, slightly fast and faster 3. The parts are found by their last word, in any order; `None` for a
/// line that does not have exactly these three.
pub fn levels_of(details: &str) -> Option<[u8; 3]> {
    let parts: Vec<String> = details.split("//").map(|part| part.trim().to_ascii_lowercase()).collect();
    if parts.len() != 3 {
        return None;
    }
    let size = |unit: &str| -> Option<u8> {
        let mut found = parts.iter().filter_map(|part| part.strip_suffix(unit));
        let word = found.next()?.trim();
        if found.next().is_some() {
            return None;
        }
        match word {
            "high" => Some(3),
            "moderate" => Some(2),
            "low" => Some(1),
            _ => None,
        }
    };
    let gain = size("yield")?;
    let cost = 4 - size("cost")?;
    let mut rest = parts.iter().filter(|part| !part.ends_with("yield") && !part.ends_with("cost"));
    let speed = match rest.next()?.trim_end_matches(" speed") {
        "slowest" => 0,
        "very slow" | "slow" => 1,
        "slightly slow" | "moderate" => 2,
        "slightly fast" | "fast" | "very fast" | "fastest" => 3,
        _ => return None,
    };
    rest.next().is_none().then_some([gain, speed, cost])
}

/// What the reading found: the methods, and what it left out with the reason, for the error log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Read {
    /// Each name once, ordered by name.
    pub processes: Vec<Process>,
    pub left_out: Vec<String>,
}

/// Puts the methods found together: per record its name (or its key where the texts have none) and its levels
/// (`None` where its line is missing or does not read as three levels). Two records may name the same method
/// (`RefineNormal` beside `ProcessingType-NormalWasteful`); with the same levels it counts once, with other levels
/// it is left out, for which of them holds cannot be told.
pub fn gather(found: Vec<(String, Option<[u8; 3]>)>) -> Read {
    let mut read = Read::default();
    let mut conflicting: Vec<String> = Vec::new();
    for (name, levels) in found {
        let Some(levels) = levels else {
            read.left_out.push(format!("{name}: no line of three levels"));
            continue;
        };
        if conflicting.contains(&name) {
            continue;
        }
        match read.processes.iter().position(|known| known.name == name) {
            Some(at) if read.processes[at].levels != levels => {
                read.processes.remove(at);
                read.left_out.push(format!("{name}: two records with other levels"));
                conflicting.push(name);
            }
            Some(_) => {}
            None => read.processes.push(Process { name, levels }),
        }
    }
    read.processes.sort_by(|a, b| a.name.cmp(&b.name));
    read
}

/// The processing methods of the DataForge `bytes`, with their names and lines from the English texts `texts`
/// ([`gather`]). What is left out leaves the mining page with its own table, for it needs all nine.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn from_dataforge(bytes: &[u8], texts: &Localization) -> Result<Read, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: "Data\\Game2.dcb".to_string(), source: Box::new(source) })?;
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut found: Vec<(String, Option<[u8; 3]>)> = Vec::new();
    for record in forge.records() {
        if record.type_name().ok() != Some(PROCESS_TYPE) {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let Some(key) = locale_key(Some(tree.root()), "processName") else { continue };
        let name = texts.english(key).unwrap_or_else(|| key.to_string());
        let levels = texts.english(&format!("{key}_Details")).and_then(|details| levels_of(&details));
        found.push((name, levels));
    }
    Ok(gather(found))
}

/// The channel the archive at `archive` belongs to: the name of its folder (`LIVE`, `PTU` …).
pub fn channel_of(archive: &Path) -> String {
    archive.parent().and_then(Path::file_name).map(|name| name.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// The text of [`FILE`]: the methods with their levels, the build and the channel they were read in.
pub fn text(processes: &[Process], build: Option<&str>, channel: &str) -> String {
    let entries = processes
        .iter()
        .map(|process| {
            let [gain, speed, cost] = process.levels;
            Json::Object(vec![
                ("name".into(), Json::str(&process.name)),
                ("ertrag".into(), Json::int(i64::from(gain))),
                ("tempo".into(), Json::int(i64::from(speed))),
                ("kosten".into(), Json::int(i64::from(cost))),
            ])
        })
        .collect();
    let fields = vec![
        ("format".into(), Json::int(GAME_FORMAT)),
        ("build".into(), Json::str(build.unwrap_or_default())),
        ("kanal".into(), Json::str(channel)),
        ("verfahren".into(), Json::Array(entries)),
    ];
    versekit_core::jsonio::dumps(&Json::Object(fields), None, false)
}

/// Keeps `processes` in `file`, replacing the file before without ever leaving a half-written one.
///
/// # Errors
///
/// The error of writing.
pub fn write(file: &str, processes: &[Process], build: Option<&str>, channel: &str) -> std::io::Result<()> {
    versekit_core::jsonio::write_text_atomic(file, &text(processes, build, channel), Newline::Native)
}

/// What keeping the file did.
#[derive(Debug)]
pub enum Kept {
    Written,
    /// The methods read are not the nine the mining page needs: the file stays as it was. Why, in words.
    Incomplete(String),
    /// Writing failed (also a refusal of the write lock).
    Failed(std::io::Error),
}

/// Writes `read` to `file` only when the mining page takes it whole ([`versekit_core::refining::Methods`]): a
/// reading that left methods out never replaces a file that had all nine.
pub fn keep(file: &str, read: &Read, build: Option<&str>, channel: &str) -> Kept {
    let text = text(&read.processes, build, channel);
    if let Err(why) = versekit_core::refining::Methods::from_game_text(&text) {
        let mut said = vec![why];
        said.extend(read.left_out.iter().cloned());
        return Kept::Incomplete(said.join("; "));
    }
    match versekit_core::jsonio::write_text_atomic(file, &text, Newline::Native) {
        Ok(()) => Kept::Written,
        Err(error) => Kept::Failed(error),
    }
}

#[cfg(test)]
mod tests;
