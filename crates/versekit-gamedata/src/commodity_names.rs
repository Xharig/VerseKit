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

//! The English names of the goods by their GUID: every `ResourceType` of the game database, named with the English
//! text its `displayName` points to. The commodity terminal names a good in the log by this GUID only
//! (`resourceGUID[…]`). Placeholders and goods without a text stay out.
//!
//! The names are read once per state of the `Data.p4k` (size and time) and kept as `waren-namen.json` in the data
//! folder, where the stock journal reads them (`versekit_core::stock_journal::names`).

use std::path::Path;

use p4kx_dataforge::{DataForge, Value};
use versekit_core::jsonio::{self, Newline, Value as Json};

use crate::Error;
use crate::quality_goods::{Keep, archive_stamp, parse_ini};

/// The file in the data folder.
pub use versekit_core::stock_journal::names::COMMODITY_NAMES_FILE as FILE;
/// The layout of [`FILE`].
pub const FORMAT: i64 = 1;
/// The type of the records that are goods.
const RESOURCE_TYPE: &str = "ResourceType";
/// Fewer names mean the database was not understood: then nothing is kept.
pub const MINIMUM: usize = 50;

/// The names of the goods the DataForge `forge` holds, named from the English `global.ini` `raw_ini`: (GUID in lower
/// case, name) in the order of the records.
pub fn extract(forge: &DataForge<'_>, raw_ini: &[u8]) -> Vec<(String, String)> {
    let texts = parse_ini(raw_ini);
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut found: Vec<(String, String)> = Vec::new();
    for record in forge.records() {
        if record.type_name().ok() != Some(RESOURCE_TYPE) {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let key = match tree.field("displayName") {
            Some(Value::Locale(text) | Value::String(text)) => text.trim_start_matches('@').to_lowercase(),
            _ => String::new(),
        };
        let Some(name) = texts.get(&key).filter(|value| !value.is_empty() && !value.starts_with('@')) else {
            continue;
        };
        let guid = record.guid().to_string().to_lowercase();
        if !found.iter().any(|(known, _)| *known == guid) {
            found.push((guid, name.clone()));
        }
    }
    found
}

/// The names of the goods of the DataForge `bytes`, named from the English `global.ini` `ini`.
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn names_from_dataforge(bytes: &[u8], ini: &[u8]) -> Result<Vec<(String, String)>, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: "Data\\Game2.dcb".to_string(), source: Box::new(source) })?;
    Ok(extract(&forge, ini))
}

/// The text of [`FILE`] for the names read from an archive with `stamp`.
pub fn file_text(stamp: &str, names: &[(String, String)]) -> String {
    let goods = names.iter().map(|(guid, name)| (guid.clone(), Json::str(name))).collect();
    let data = Json::Object(vec![
        ("format".into(), Json::int(FORMAT)),
        ("stand".into(), Json::str(stamp)),
        ("waren".into(), Json::Object(goods)),
    ]);
    jsonio::dumps(&data, None, false)
}

/// Whether `file` was read from an archive with `stamp`, in the current layout.
pub fn is_current(file: &str, stamp: &str) -> bool {
    let Ok(read) = jsonio::read_json(file) else { return false };
    read.get("stand").and_then(Json::as_str) == Some(stamp) && read.get("format") == Some(&Json::int(FORMAT))
}

/// Reads the names with `read` and writes `file` when it does not belong to the archive at `archive` as it is now.
pub fn keep_current_with(
    archive: &Path,
    file: &str,
    read: impl FnOnce() -> Result<Vec<(String, String)>, Error>,
) -> Keep {
    let Some(stamp) = archive_stamp(archive) else { return Keep::NoArchive };
    if is_current(file, &stamp) {
        return Keep::Current;
    }
    let names = match read() {
        Ok(names) => names,
        Err(error) => return Keep::Failed(error.to_string()),
    };
    if names.len() < MINIMUM {
        return Keep::TooFew(names.len());
    }
    match jsonio::write_text_atomic(file, &file_text(&stamp, &names), Newline::Keep) {
        Ok(()) => Keep::Written,
        Err(error) if versekit_core::writeguard::refused(&error) => Keep::Refused,
        Err(error) => Keep::Failed(error.to_string()),
    }
}

/// Keeps `file` current for the archive at `archive`: reads the game database (seconds) only after a patch.
pub fn keep_current(archive: &Path, file: &str) -> Keep {
    keep_current_with(archive, file, || {
        let game = crate::GameData::open_file(archive)?;
        names_from_dataforge(&game.dataforge_bytes()?, &game.english_ini_bytes()?)
    })
}

/// Keeps the names of the data folder of `paths` current for its archive and notes what went wrong: a failure in
/// the error log, too few names in the start trail. Without a game nothing happens.
pub fn keep(paths: &versekit_core::paths::Paths) -> Option<Keep> {
    let archive = paths.archive_file()?;
    let _topic = versekit_core::archive_lock::reading_for("signatures");
    let outcome = keep_current(Path::new(&archive), &paths.app_file(FILE));
    match &outcome {
        Keep::Failed(reason) => versekit_core::errors::record("stock_journal.refresh_goods", reason),
        Keep::TooFew(count) => {
            versekit_core::trail::step(&format!("Warennamen: nur {count} erkannt, nicht abgelegt"));
        }
        _ => {}
    }
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use p4kx_testkit::{DataForgeBuilder, TestRecord, TestStruct, TestType, TestValue};

    use super::*;

    fn good(guid: u128, kind: &str, name: &str, key: &str) -> TestRecord {
        TestRecord::new(guid, kind, format!("{kind}.{name}"), "libs/foundry/records/resourcetypedatabase/db.xml")
            .value("displayName", TestValue::text(key))
    }

    /// A database with `count` goods, a placeholder, a record of another type and one without a text.
    fn forge(count: u128) -> Vec<u8> {
        let mut builder = DataForgeBuilder::new()
            .version(8)
            .structure(TestStruct::new(RESOURCE_TYPE).field("displayName", TestType::Locale))
            .structure(TestStruct::new("OtherThing").field("displayName", TestType::Locale));
        for n in 0..count {
            builder = builder.record(good(0x5000 + n, RESOURCE_TYPE, &format!("Good{n}"), &format!("@good_{n}")));
        }
        builder
            .record(good(0x4000, RESOURCE_TYPE, "Stims", "@items_commodities_stims"))
            .record(good(0x4001, RESOURCE_TYPE, "Empty", "@LOC_EMPTY"))
            .record(good(0x4002, "OtherThing", "NotAGood", "@items_commodities_stims"))
            .record(good(0x4003, RESOURCE_TYPE, "NoText", "@no_such_key"))
            .build()
    }

    fn ini(count: u128) -> Vec<u8> {
        let mut text = String::from("\u{feff}items_commodities_Stims=Stims\r\nLOC_EMPTY=@LOC_EMPTY\r\n");
        for n in 0..count {
            text.push_str(&format!("good_{n},P=Good {n}\r\n"));
        }
        text.into_bytes()
    }

    /// The GUID the database gives the record of this name.
    fn guid_of(record: &str) -> String {
        let forge_bytes = forge(0);
        let forge = DataForge::open(&forge_bytes).unwrap();
        forge.records().find(|r| r.name().ok() == Some(record)).expect("the record").guid().to_string()
    }

    #[test]
    fn the_goods_are_named_by_their_guid_from_the_english_texts_and_placeholders_stay_out() {
        let names = names_from_dataforge(&forge(1), &ini(1)).expect("names");
        let stims = guid_of("ResourceType.Stims").to_lowercase();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.iter().any(|(guid, name)| name == "Good 0" && *guid == guid.to_lowercase()));
        assert!(names.contains(&(stims, "Stims".to_string())), "{names:?}");
    }

    /// The real archive (`VERSEKIT_P4K`, the path of a `Data.p4k`), read only: the two goods of a real terminal
    /// sale are named.
    #[test]
    #[ignore = "needs a real Data.p4k"]
    fn the_real_archive_names_gold_and_stims() {
        let Ok(archive) = std::env::var("VERSEKIT_P4K") else { return };
        let game = crate::GameData::open_file(Path::new(&archive)).expect("archive");
        let names =
            names_from_dataforge(&game.dataforge_bytes().unwrap(), &game.english_ini_bytes().unwrap()).expect("names");
        println!("Waren mit Namen: {}", names.len());
        let name_of = |guid: &str| names.iter().find(|(g, _)| g == guid).map(|(_, n)| n.as_str());
        assert_eq!(name_of("21825507-7923-4683-9bf3-9cfe316940e3"), Some("Gold"));
        assert_eq!(name_of("f4c68747-fa77-4c42-aff5-c3d72b8c8388"), Some("Stims"));
        assert!(names.len() >= MINIMUM);
    }

    #[test]
    fn the_file_is_written_once_per_state_of_the_archive_and_the_journal_reads_it() {
        let dir = versekit_core::testdir::Dir::new("warennamen");
        let archive = dir.path().join("Data.p4k");
        let home = dir.path().to_string_lossy().into_owned();
        let paths = versekit_core::paths::Paths::new(versekit_core::paths::Host::new(
            versekit_core::pypath::Flavor::native(),
            &[("SC_BP_HOME", home.as_str())],
            &home,
        ));
        let file = paths.app_file(FILE);
        assert_eq!(keep_current_with(&archive, &file, || unreachable!("no archive, no reading")), Keep::NoArchive);
        std::fs::write(&archive, b"p4k").unwrap();
        let read = || names_from_dataforge(&forge(MINIMUM as u128), &ini(MINIMUM as u128));
        assert_eq!(keep_current_with(&archive, &file, read), Keep::Written);
        let kept = jsonio::read_json(&file).expect("the file");
        assert_eq!(kept.get("format"), Some(&Json::int(FORMAT)));
        let stims = guid_of("ResourceType.Stims");
        // The journal finds the good by its GUID in any case.
        let items = versekit_core::stock_journal::names::Items::load(&paths);
        assert_eq!(items.candidates("", &stims.to_uppercase()), ["Stims"]);
        assert_eq!(
            keep_current_with(&archive, &file, || unreachable!("the same archive is not read again")),
            Keep::Current
        );
        std::fs::write(&archive, b"patched p4k").unwrap();
        let few = || names_from_dataforge(&forge(3), &ini(3));
        assert_eq!(keep_current_with(&archive, &file, few), Keep::TooFew(4), "too few names: the file stays");
        assert_eq!(jsonio::read_json(&file).unwrap(), kept);
    }
}
