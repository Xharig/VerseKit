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

//! The mineable rocks with the scan signature the mining scanner shows, and the crafting stations with the
//! limits of their job queues, read from the DataForge.
//!
//! A rock is an `EntityClassDefinition` under `entities/mineable`. Its `signatures` are one number per signature
//! type of the radar parameters; the scan signature is the slot that the `RadarSystemGlobalParams` record names
//! `@hud_scanning_info_rs_signature`. The same rules as the Basetool catalog of p4kx (`mineableEntities`,
//! `crafters`); `versekit-parity felsen` compares the two on the real archive.

use std::path::Path;

use p4kx_dataforge::{DataForge, RecordTree, Value};
use versekit_core::jsonio::{Newline, Value as Json};

use crate::fields::{class_name, component, elements, integer, locale_key, reference, structs, structure, text};
use crate::values::CrafterLimits;
use crate::{Error, Localization};

/// The record type of the radar parameters that name the slots of the signatures.
const RADAR_GLOBAL_PARAMS: &str = "RadarSystemGlobalParams";
/// The key of the text of the slot that holds the scan signature.
const SCAN_SIGNATURE_KEY: &str = "@hud_scanning_info_rs_signature";
const ROCK_PREFIX: &str = "libs/foundry/records/entities/mineable/";
const ENTITY_PREFIX: &str = "libs/foundry/records/entities/";
/// The file the signatures are kept in, next to `mining-data.json`.
pub use versekit_core::mining_data::ROCK_SIGNATURES_FILE as SIGNATURES_FILE;
/// The version of the layout of [`SIGNATURES_FILE`].
const SIGNATURES_FORMAT: i64 = 1;

/// One mineable rock.
#[derive(Debug, Clone, PartialEq)]
pub struct Rock {
    pub guid: String,
    /// The record name without its type.
    pub class_name: String,
    pub record_path: String,
    /// The name of the deposit its composition states, in English (`Asteroid (C-Type)`).
    pub deposit: Option<String>,
    /// `radarProperties.baseSignatureParams.signatures`, one number per signature type.
    pub signatures: Vec<f32>,
    /// The scan signature; `None` where the record states no signatures, all are zero, they are not as many as
    /// the signature types, or the slot is unknown.
    pub scan_signature: Option<f32>,
}

/// One queue of a crafting station.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    pub name: Option<String>,
    /// How many jobs it crafts at once.
    pub max_jobs_in_progress: Option<i64>,
    /// How many further jobs it holds.
    pub max_jobs_waiting: Option<i64>,
}

/// One crafting station.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crafter {
    pub guid: String,
    pub class_name: String,
    pub record_path: String,
    /// How many finished jobs the station lists.
    pub max_jobs_in_history: Option<i64>,
    pub queues: Vec<Queue>,
}

/// The slot of the scan signature among the signatures, as the radar parameters name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    index: usize,
    types: usize,
}

impl Slot {
    /// The slot the one `RadarSystemGlobalParams` record names; `None` where there is not exactly one record with
    /// exactly one entry for the scan signature.
    fn read(forge: &DataForge<'_>) -> Option<Slot> {
        let mut builder = p4kx_dataforge::TreeBuilder::new();
        let mut found: Vec<Option<Slot>> = Vec::new();
        for record in forge.records() {
            if record.type_name().ok() != Some(RADAR_GLOBAL_PARAMS) {
                continue;
            }
            found.push(builder.build(&record).ok().and_then(|tree| Slot::of_tree(&tree)));
        }
        match found.as_slice() {
            [only] => *only,
            _ => None,
        }
    }

    fn of_tree(tree: &RecordTree<'_>) -> Option<Slot> {
        let system = structure(Some(tree.root()), "signatureSystemParams");
        let types = elements(system, "signatureTypeParams");
        let mut named = types.iter().enumerate().filter(|(_, entry)| {
            matches!(entry, Value::Struct(entry) if locale_key(Some(entry), "displayName") == Some(SCAN_SIGNATURE_KEY))
        });
        match (named.next(), named.next()) {
            (Some((index, _)), None) => Some(Slot { index, types: types.len() }),
            _ => None,
        }
    }

    fn value(self, signatures: &[f32]) -> Option<f32> {
        if signatures.len() != self.types || signatures.iter().all(|s| *s == 0.0) {
            return None;
        }
        signatures.get(self.index).copied()
    }
}

/// Development leftovers are not rocks or stations: the record path or the name holds such a word.
fn left_over(path: &str, name: &str) -> bool {
    crate::catalog::is_leftover(path) || crate::catalog::is_leftover(name)
}

/// The mineable rocks of the DataForge `bytes`, ordered by GUID. The names of the deposits come from `texts`.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn rocks_from_dataforge(bytes: &[u8], texts: &Localization) -> Result<Vec<Rock>, Error> {
    let forge = open(bytes)?;
    Ok(rocks(&forge, texts))
}

/// The crafting stations of the DataForge `bytes`, ordered by GUID.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn crafters_from_dataforge(bytes: &[u8]) -> Result<Vec<Crafter>, Error> {
    let forge = open(bytes)?;
    Ok(crafters(&forge))
}

fn open(bytes: &[u8]) -> Result<DataForge<'_>, Error> {
    DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: "Data\\Game2.dcb".to_string(), source: Box::new(source) })
}

fn rocks(forge: &DataForge<'_>, texts: &Localization) -> Vec<Rock> {
    let slot = Slot::read(forge);
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut out = Vec::new();
    for record in forge.records() {
        let (Ok("EntityClassDefinition"), Ok(path), Ok(name)) =
            (record.type_name(), record.record_path(), record.name())
        else {
            continue;
        };
        if !crate::catalog::starts_with_ignore_case(path, ROCK_PREFIX) || left_over(path, name) {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let mining = component(&tree, "MineableParams");
        let radar = structure(component(&tree, "SSCSignatureSystemParams"), "radarProperties");
        let base = structure(radar, "baseSignatureParams");
        let signatures: Vec<f32> = elements(base, "signatures")
            .iter()
            .map(|value| match value {
                Value::Single(signature) if signature.is_finite() => *signature,
                _ => 0.0,
            })
            .collect();
        let deposit = reference(mining, "composition")
            .and_then(|guid| forge.record_by_guid(guid))
            .and_then(|composition| builder.build(&composition).ok())
            .and_then(|composition| {
                locale_key(Some(composition.root()), "depositName").and_then(|key| texts.english(key))
            });
        out.push(Rock {
            guid: tree.guid().to_string(),
            class_name: class_name(&tree).to_string(),
            record_path: tree.record_path().to_string(),
            deposit,
            scan_signature: slot.and_then(|slot| slot.value(&signatures)),
            signatures,
        });
    }
    out.sort_by(|a, b| a.guid.cmp(&b.guid));
    out
}

pub(crate) fn crafters(forge: &DataForge<'_>) -> Vec<Crafter> {
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut out = Vec::new();
    for record in forge.records() {
        let (Ok("EntityClassDefinition"), Ok(path), Ok(name)) =
            (record.type_name(), record.record_path(), record.name())
        else {
            continue;
        };
        if !crate::catalog::starts_with_ignore_case(path, ENTITY_PREFIX) || left_over(path, name) {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let Some(params) = component(&tree, "CrafterComponentParams") else { continue };
        out.push(Crafter {
            guid: tree.guid().to_string(),
            class_name: class_name(&tree).to_string(),
            record_path: tree.record_path().to_string(),
            max_jobs_in_history: integer(Some(params), "maxJobsInHistory"),
            queues: structs(Some(params), "queues")
                .map(|queue| Queue {
                    name: text(Some(queue), "debugName").map(str::to_string),
                    max_jobs_in_progress: integer(Some(queue), "maxJobsInProgress"),
                    max_jobs_waiting: integer(Some(queue), "maxJobsWaiting"),
                })
                .collect(),
        });
    }
    out.sort_by(|a, b| a.guid.cmp(&b.guid));
    out
}

/// The limits of the job queues the fabricator states: the first station that names a history length and a queue
/// that is not the dismantling one, with the queue of dismantling beside it where it has one.
pub fn crafter_limits(stations: &[Crafter]) -> Option<CrafterLimits> {
    let is_dismantle = |queue: &Queue| queue.name.as_deref().is_some_and(|n| n.to_ascii_lowercase().contains("dismantle"));
    let limits = |queue: &Queue| Some((queue.max_jobs_in_progress?, queue.max_jobs_waiting?));
    stations.iter().find_map(|station| {
        let history = station.max_jobs_in_history?;
        let (in_progress, waiting) = station.queues.iter().filter(|q| !is_dismantle(q)).find_map(limits)?;
        let dismantle = station.queues.iter().filter(|q| is_dismantle(q)).find_map(limits);
        Some(CrafterLimits { in_progress, waiting, history, dismantle })
    })
}

/// Size and modification time of an archive: what tells that it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveStamp {
    size: i64,
    /// Milliseconds since 1970.
    modified: i64,
}

impl ArchiveStamp {
    /// The stamp of the file; `None` where it does not exist or states no time.
    pub fn of(path: &Path) -> Option<ArchiveStamp> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(ArchiveStamp { size: i64::try_from(meta.len()).ok()?, modified: i64::try_from(modified.as_millis()).ok()? })
    }

    fn json(self) -> Json {
        Json::Object(vec![("groesse".into(), Json::int(self.size)), ("zeit".into(), Json::int(self.modified))])
    }
}

/// Whether the signatures in `file` were read from an archive with this `stamp`.
pub fn is_current(file: &str, stamp: ArchiveStamp) -> bool {
    let Ok(read) = versekit_core::jsonio::read_json(file) else { return false };
    read.get("format") == Some(&Json::int(SIGNATURES_FORMAT)) && read.get("archiv") == Some(&stamp.json())
}

/// What keeping the signatures current did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keep {
    /// The file belongs to the archive as it is: nothing was read.
    Current,
    /// The archive was read and the file written anew.
    Written,
    /// There is no archive to read.
    NoArchive,
    /// The write lock of a copy of the data refused the file.
    Refused,
    /// Reading or writing failed; the file stays as it was.
    Failed(String),
}

/// Reads the rocks with `read` and writes `file` when it does not belong to the archive at `archive` as it is now.
/// `read` gives the rocks and the build.
pub fn keep_current_with(
    archive: &Path,
    file: &str,
    read: impl FnOnce() -> Result<(Vec<Rock>, Option<String>), Error>,
) -> Keep {
    let Some(stamp) = ArchiveStamp::of(archive) else { return Keep::NoArchive };
    if is_current(file, stamp) {
        return Keep::Current;
    }
    let (rocks, build) = match read() {
        Ok(read) => read,
        Err(error) => return Keep::Failed(error.to_string()),
    };
    match write_signatures(file, &rocks, build.as_deref(), Some(stamp)) {
        Ok(()) => Keep::Written,
        Err(error) if versekit_core::writeguard::refused(&error) => Keep::Refused,
        Err(error) => Keep::Failed(error.to_string()),
    }
}

/// Keeps `file` current for the archive at `archive`: reads the archive (seconds) only when it changed.
pub fn keep_current(archive: &Path, file: &str) -> Keep {
    let _topic = versekit_core::archive_lock::reading_for("signatures");
    keep_current_with(archive, file, || {
        let game = crate::GameData::open_file(archive)?;
        let texts = game.localization(crate::Language::English)?;
        let rocks = rocks_from_dataforge(&game.dataforge_bytes()?, &texts)?;
        Ok((rocks, game.scmdb_build()))
    })
}

/// The text of [`SIGNATURES_FILE`]: one entry per rock with a scan signature, and the stamp of the archive they
/// were read from.
pub fn signatures_text(rocks: &[Rock], build: Option<&str>, archive: Option<ArchiveStamp>) -> String {
    let entries = rocks
        .iter()
        .filter_map(|rock| {
            let signature = rock.scan_signature?;
            Some(Json::Object(vec![
                ("klasse".into(), Json::str(&rock.class_name)),
                ("name".into(), rock.deposit.as_deref().map_or(Json::Null, Json::str)),
                ("signatur".into(), Json::Float(f64::from(signature))),
            ]))
        })
        .collect();
    let mut fields = vec![
        ("format".into(), Json::int(SIGNATURES_FORMAT)),
        ("build".into(), build.map_or(Json::Null, Json::str)),
    ];
    fields.extend(archive.map(|stamp| ("archiv".to_string(), stamp.json())));
    fields.push(("steine".into(), Json::Array(entries)));
    versekit_core::jsonio::dumps(&Json::Object(fields), None, false)
}

/// Keeps the signatures of `rocks` in `file`, replacing the file before without ever leaving a half-written one.
///
/// # Errors
///
/// The error of writing.
pub fn write_signatures(
    file: &str,
    rocks: &[Rock],
    build: Option<&str>,
    archive: Option<ArchiveStamp>,
) -> std::io::Result<()> {
    versekit_core::jsonio::write_text_atomic(file, &signatures_text(rocks, build, archive), Newline::Native)
}

#[cfg(test)]
mod tests;
