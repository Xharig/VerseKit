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

//! The stand of a build (`versekit_core::stands`) made of what the readers of this crate give, and the hooks that keep
//! it with every build of the catalog.
//!
//! - [`read_stand`] gives the six lists of the caches (catalog, values, ship slots, parts, reputation of the
//!   contracts, rank thresholds) as the readers give them from the archive alone, in the shape the cache files have,
//!   and lets `stands::add_areas` make the areas, so that a stand of a reading and a stand of the kept caches are made
//!   the same way. Nothing of scmdb, nothing of the network: a stand says "from the archive".
//! - [`keep_base`] takes the stand of the caches before a rebuild overwrites them; without it the first patch after
//!   the first start would have nothing to compare with, the old archive being gone when the launcher has patched.
//! - [`record_in_background`] reads the new build in a thread of its own after the rebuild, keeps the stand and
//!   writes the comparison with the one before ([`finish`]). It never makes the build of the catalog wait.
//!
//! ⚠ A channel the program is not built for makes no stand (`stands::save` refuses it, and `channel_of` does not
//! know its folder): the user build never reads EPTU or Tech-Preview.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::stand_diff::{self, Comparison};
use versekit_core::stands::{self, CACHES, CacheFile, Leaf, Stand};
use versekit_core::{patch_changes, patch_history};

use crate::{GameData, Language, corrections, list};

/// The version of what [`read_stand`] reads; a new one when a list is added or read another way, so that the
/// comparison of two stands knows the readers differ.
pub const READER: &str = "archiv-3";

/// Whether the builds of the catalog make stands. Not in the checks of this crate, which build catalogs of invented
/// archives all the time.
pub fn recording() -> bool {
    !cfg!(test)
}

fn today() -> String {
    stands::today()
}

/// The channel of an archive file by the name of its folder, in the form of `paths::CHANNELS` (`LIVE`); `None` for
/// a folder the program is not built for.
fn channel_of_archive(archive: &Path) -> Option<String> {
    let folder = archive.parent()?.file_name()?.to_str()?;
    crate::channel_of(folder).map(str::to_ascii_uppercase)
}

fn cache(area: &str) -> &'static CacheFile {
    CACHES.iter().find(|c| c.area == area).expect("a cache of the list of the stands")
}

/// The counters of the texts: keys that say they have no translation, and keys English has and German lacks.
pub const PROBE_NOT_FOUND: &str = "texte_nicht_gefunden";
pub const PROBE_NO_GERMAN: &str = "texte_ohne_deutsch";
/// What the game puts in a text it has no translation for.
const NOT_FOUND: &str = "TRANSLATION NOT FOUND";

/// A checksum of a text that stays the same in every version of the program (not `DefaultHasher`): FNV-1a, 64 bit,
/// kept as the number with the same bits.
pub fn checksum(text: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    i64::from_ne_bytes(hash.to_ne_bytes())
}

/// The keys of a `global.ini` with the checksum of each text, and how many texts say they have no translation.
pub fn texts_of(bytes: &[u8]) -> (BTreeMap<String, stands::Entry>, i64) {
    let decoded = String::from_utf8_lossy(bytes);
    let text = decoded.strip_prefix('\u{feff}').unwrap_or(&decoded);
    let mut entries = BTreeMap::new();
    let mut not_found = 0;
    for line in text.lines() {
        let Some((key, value)) = line.trim_end_matches('\r').split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || key.starts_with([';', '#']) {
            continue;
        }
        if value.contains(NOT_FOUND) {
            not_found += 1;
        }
        let entry: stands::Entry = BTreeMap::from([("h".to_string(), Leaf::Int(checksum(value)))]);
        entries.insert(key.to_string(), entry);
    }
    (entries, not_found)
}

/// The areas of the texts: English and German, a checksum per key. Only two languages: the others wait for a
/// measure of what they weigh. English that cannot be read is a failure; German missing is not (some archives hold
/// no file of it).
fn add_texts(game: &GameData, stand: &mut Stand, failed: &mut i64) -> BTreeMap<String, i64> {
    let mut counters = BTreeMap::new();
    let mut english_keys: Option<Vec<String>> = None;
    for (language, area) in [(Language::English, "texte.en"), (Language::German, "texte.de")] {
        match game.entry_bytes(language.global_ini_path()) {
            Err(error) => {
                if language == Language::English {
                    lost(failed, "texts-file", &error);
                }
            }
            Ok(bytes) => {
                let (entries, not_found) = texts_of(&bytes);
                if language == Language::English {
                    english_keys = Some(entries.keys().cloned().collect());
                    *counters.entry(PROBE_NOT_FOUND.to_string()).or_insert(0) += not_found;
                } else if let Some(keys) = &english_keys {
                    let missing = keys.iter().filter(|key| !entries.contains_key(*key)).count();
                    counters.insert(PROBE_NO_GERMAN.to_string(), i64::try_from(missing).unwrap_or(i64::MAX));
                }
                stand.areas.insert(area.to_string(), stands::Area { entries });
            }
        }
    }
    counters
}

/// The structs of the DataForge that the readers of this crate read (the names written in the readers). A field of
/// one of these that goes away breaks a reader; the shapes of all others do not matter to VerseKit. Names that
/// only appear as field names or in the test fixtures of the readers are not in the list: the real archive does not
/// hold a struct of that name (checked against a real archive with `tests/patch_stand_real.rs`).
pub const WATCHED_STRUCTS: &[&str] = &[
    "BlueprintPoolRecord",
    "CrafterComponentParams",
    "CraftingBlueprintRecord",
    "CraftingGlobalParams",
    "CraftingQualityQuantizationRecord",
    "DataForgeComponentParams",
    "EntityClassDefinition",
    "IFCSParams",
    "ItemResourceComponentParams",
    "MineableParams",
    "RadarSystemGlobalParams",
    "SAmmoContainerComponentParams",
    "SAttachableComponentParams",
    "SCItemClothingParams",
    "SCItemMissileParams",
    "SCItemQuantumDriveParams",
    "SCItemShieldGeneratorParams",
    "SCItemSuitArmorParams",
    "SCItemWeaponComponentParams",
    "SDistortionParams",
    "SHealthComponentParams",
    "SItemPortContainerComponentParams",
    "SSCSignatureSystemParams",
    "SWeaponActionDynamicConditionParams",
    "SWeaponActionFireBeamParams",
    "SWeaponActionFireBurstParams",
    "SWeaponActionFireChargedParams",
    "SWeaponActionFireRapidParams",
    "SWeaponActionFireSingleParams",
    "SWeaponActionParallelParams",
    "SWeaponActionSequenceParams",
    "VehicleComponentParams",
];

/// How a field is written in the area of the shapes: its type, `[]` for a list, and the struct or enum it points to.
fn shape_of(property: &p4kx_dataforge::Property<'_>) -> String {
    let kind = property.data_type().map_or_else(|_| "?".to_string(), |data_type| format!("{data_type:?}"));
    let list = if property.is_array() { "[]" } else { "" };
    let target = property
        .target_struct()
        .and_then(|s| s.name().ok().map(str::to_string))
        .or_else(|| property.target_enum().and_then(|e| e.name().ok().map(str::to_string)));
    match target {
        Some(target) => format!("{kind}{list}->{target}"),
        None => format!("{kind}{list}"),
    }
}

/// The area of the shapes: the fields of the structs the readers read, taken from the DataForge. A struct the
/// archive no longer holds is missing from the area: in the comparison it is gone.
fn add_forms(game: &GameData, stand: &mut Stand, failed: &mut i64) {
    let bytes = match game.dataforge_bytes() {
        Ok(bytes) => bytes,
        Err(error) => return lost(failed, "forms", &error),
    };
    let forge = match p4kx_dataforge::DataForge::open(&bytes) {
        Ok(forge) => forge,
        Err(error) => return lost(failed, "forms", &error),
    };
    let mut entries = BTreeMap::new();
    for def in forge.structs() {
        let Ok(name) = def.name() else { continue };
        if !WATCHED_STRUCTS.contains(&name) {
            continue;
        }
        let fields: stands::Entry =
            def.properties().filter_map(|p| Some((p.name().ok()?.to_string(), Leaf::Text(shape_of(&p))))).collect();
        entries.insert(name.to_string(), fields);
    }
    stand.areas.insert(stands::AREA_FORMS.to_string(), stands::Area { entries });
}

/// A value of serde as a value of `jsonio`: a whole number stays whole, a number with a fraction keeps it.
fn from_serde(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => Value::int(i),
            (None, Some(u)) => Value::Int(u.to_string()),
            _ => Value::Float(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(s) => Value::str(s),
        serde_json::Value::Array(items) => Value::Array(items.iter().map(from_serde).collect()),
        serde_json::Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), from_serde(v))).collect()),
    }
}

/// Counts a list that could not be read and tells the log; the stand says it in the counter `failed`.
fn lost(failed: &mut i64, what: &str, error: &dyn std::fmt::Display) {
    *failed += 1;
    versekit_core::errors::record(&format!("stand_read.{what}"), error);
}

/// The stand of the build of `game`, read from the archive alone: `None` when the build or the channel is not
/// known, or when no list could be read.
pub fn read_stand(game: &GameData, date: &str) -> Option<Stand> {
    let build = game.scmdb_build().or_else(|| game.version().map(str::to_string))?;
    let folder = game.channel_folder();
    let channel = crate::channel_of(folder.file_name()?.to_str()?)?.to_ascii_uppercase();
    let mut stand = Stand::new(&channel, &build, date, READER);
    stand.archive_stamp = jsonio::dumps(&list::file_stamp(game.archive_path()), None, true);
    let mut failed = 0_i64;
    let mut add = |area: &str, root: &Value| stands::add_areas(cache(area), root, &mut stand.areas);

    let fixes = corrections::built_in();
    match game.localization(Language::English) {
        Err(error) => lost(&mut failed, "texts", &error),
        Ok(texts) => {
            match game.values(&texts) {
                Err(error) => lost(&mut failed, "values", &error),
                Ok(values) => match serde_json::to_value(&values) {
                    Ok(json) => add("werte", &Value::Object(vec![("values".into(), from_serde(&json))])),
                    Err(error) => lost(&mut failed, "values", &error),
                },
            }
            match game.contract_catalog() {
                Err(error) => lost(&mut failed, "contracts", &error),
                Ok(contracts) => {
                    let locations = game.location_catalog().unwrap_or_else(|error| {
                        lost(&mut failed, "locations", &error);
                        Vec::new()
                    });
                    match game.blueprints_and_stats(&texts, &fixes, &contracts, &locations) {
                        Err(error) => lost(&mut failed, "catalog", &error),
                        Ok((catalog, stats, names)) => {
                            let cached = list::from_game(&catalog, &build, None);
                            let root =
                                list::cache_value(&Value::Null, &cached, &list::item_texts(&catalog), &stats, &names);
                            add("katalog", &root);
                            match game.contract_reputation(
                                &contracts,
                                &catalog.shown_keys,
                                &catalog.offered,
                                &fixes,
                                &catalog.reward_amounts,
                            ) {
                                Err(error) => lost(&mut failed, "reputation", &error),
                                Ok(read) => add(
                                    "auftragsruf",
                                    &Value::Object(vec![
                                        ("auftraege".into(), Value::Object(read.parties)),
                                        ("regionen".into(), Value::Object(read.regions)),
                                        ("angaben".into(), Value::Object(read.terms)),
                                    ]),
                                ),
                            }
                        }
                    }
                }
            }
        }
    }
    match game.ship_slots() {
        Err(error) => lost(&mut failed, "slots", &error),
        Ok((ships, parts)) => {
            add("schiffe", &ships);
            add("teile", &parts);
        }
    }
    match game.rank_thresholds() {
        Err(error) => lost(&mut failed, "ranks", &error),
        Ok(table) => {
            let steps: Map = table.into_iter().map(|(key, threshold)| (key, Value::int(threshold))).collect();
            add("rufstufen", &Value::Object(vec![("stufen".into(), Value::Object(steps))]));
        }
    }
    let text_counters = add_texts(game, &mut stand, &mut failed);
    add_forms(game, &mut stand, &mut failed);
    if stand.areas.is_empty() {
        return None;
    }
    stand.probe = stands::probe_of(&stand.areas);
    stand.probe.extend(text_counters);
    *stand.probe.entry(stands::PROBE_FAILED.into()).or_default() += failed;
    Some(stand)
}

/// A stand of this channel and build is kept already.
fn has_stand(paths: &Paths, channel: &str, build: &str) -> bool {
    let build = patch_changes::safe_name(build);
    stands::list(paths).iter().any(|i| i.channel.eq_ignore_ascii_case(channel) && patch_changes::same_version(&i.build, &build))
}

/// Keeps the stand of the caches next to `cache_file` as the stand of the build the catalog cache names, before a
/// rebuild writes them anew; not when that build has a stand already. Whether one was kept.
pub fn keep_base(host: &Host, archive: &Path, cache_file: &str) -> bool {
    let Some(channel) = channel_of_archive(archive) else { return false };
    let Some(folder) = Path::new(cache_file).parent() else { return false };
    let Ok(root) = jsonio::read_json(cache_file) else { return false };
    let Some(build) = root.get("version").and_then(Value::as_str).filter(|b| !b.is_empty()) else { return false };
    let paths = Paths::new(host.clone());
    if has_stand(&paths, &channel, build) {
        return false;
    }
    let files: Vec<(&CacheFile, String)> =
        CACHES.iter().map(|c| (c, folder.join(c.file).to_string_lossy().into_owned())).collect();
    let Some(stand) = Stand::from_cache_files(&files, &channel, build, &today()) else { return false };
    match stands::save(&paths, &stand) {
        Ok(()) => true,
        Err(stands::SaveError::ChannelNotBuiltIn) => false,
        Err(stands::SaveError::Write(error)) => {
            versekit_core::errors::record("stand_read.base", &error);
            false
        }
    }
}

/// Keeps `new` and, when the channel has an older stand, compares the two and writes the comparison (`Patches`).
/// The comparison, or `None` for the first stand of a channel (it only lays the base) and when nothing could be kept.
pub fn finish(paths: &Paths, new: Stand) -> Option<Comparison> {
    let build = patch_changes::safe_name(&new.build);
    let newest = patch_history::rank(&new.build);
    let previous = stands::list(paths)
        .into_iter()
        .find(|i| {
            i.channel.eq_ignore_ascii_case(&new.channel)
                && !patch_changes::same_version(&i.build, &build)
                && patch_history::rank(&i.build) <= newest
        })
        .and_then(|i| stands::load(paths, &i.channel, &i.build));
    match stands::save(paths, &new) {
        Ok(()) => {}
        Err(stands::SaveError::ChannelNotBuiltIn) => return None,
        Err(stands::SaveError::Write(error)) => {
            versekit_core::errors::record("stand_read.save", &error);
            return None;
        }
    }
    let old = previous?;
    let comparison = stand_diff::compare(&old, &new);
    if let Err(stand_diff::WriteError::Write(error)) = stand_diff::write_patch(paths, &comparison) {
        versekit_core::errors::record("stand_read.patch", &error);
    }
    Some(comparison)
}

/// Reads the stand of the archive and keeps it ([`finish`]). Waits while the game or the launcher has the archive.
pub fn record(host: &Host, archive: &Path) -> Option<Comparison> {
    let paths = Paths::new(host.clone());
    let game = match GameData::open_file(archive) {
        Ok(game) => game,
        Err(error) => {
            versekit_core::errors::record("stand_read.open", &error);
            return None;
        }
    };
    let topic = versekit_core::archive_lock::reading_for("stand");
    let stand = read_stand(&game, &today());
    drop(topic);
    let stand = stand?;
    // The same build read by the same reader is kept: the archive says so by its stamp.
    if let Some(kept) = stands::load(&paths, &stand.channel, &stand.build)
        && kept.reader == stand.reader
        && kept.archive_stamp == stand.archive_stamp
    {
        return None;
    }
    finish(&paths, stand)
}

/// One reading at a time: a second ask while one runs is dropped, the next build asks again.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts [`record`] in a thread of its own.
pub fn record_in_background(host: Host, archive: PathBuf) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("stand".into()).spawn(move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| record(&host, &archive)));
        RUNNING.store(false, Ordering::SeqCst);
        if outcome.is_err() {
            versekit_core::errors::record("stand_read.thread", &"the reading of a stand failed");
        }
    });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
        versekit_core::errors::record("stand_read.thread", &"no thread for the reading of a stand");
    }
}

#[cfg(test)]
mod tests;
