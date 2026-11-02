// Verse-Kit — companion for Star Citizen.
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

//! The readable names of the places of the game (`location.names_from` and `location.refresh_names`): every
//! `StarMapObject` of the game database, keyed by its record name in lower case, named with the English text its
//! `name` points to. Placeholders (`@LOC_EMPTY` …) stay out.
//!
//! The names are read once per state of the `Data.p4k` (size and time) and kept as `orte-namen.json` in the data
//! folder, where the location row of the overlay reads them (`versekit_core::location`).

use std::path::Path;

use p4kx_dataforge::{DataForge, Value};
use versekit_core::jsonio::{self, Newline, Value as Json};

use crate::Error;
use crate::quality_goods::{Keep, archive_stamp, parse_ini};

/// The file in the data folder.
pub use versekit_core::location::NAMES_FILE as FILE;
/// The layout of [`FILE`].
pub const FORMAT: i64 = 1;
/// The type of the records that are places.
const MAP_TYPE: &str = "StarMapObject";
/// Fewer names mean the database was not understood: then nothing is kept.
pub const MINIMUM: usize = 50;

/// The names of the places the DataForge `forge` holds, named from the English `global.ini` `raw_ini`: (key in lower
/// case, name) in the order of the records; a later record of the same key replaces the name in its place.
pub fn extract(forge: &DataForge<'_>, raw_ini: &[u8]) -> Vec<(String, String)> {
    let texts = parse_ini(raw_ini);
    let mut builder = p4kx_dataforge::TreeBuilder::new();
    let mut found: Vec<(String, String)> = Vec::new();
    for record in forge.records() {
        let (Ok(kind), Ok(name)) = (record.type_name(), record.name()) else { continue };
        if kind != MAP_TYPE {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        let key = match tree.field("name") {
            Some(Value::Locale(text) | Value::String(text)) => text.trim_start_matches('@').to_lowercase(),
            _ => String::new(),
        };
        let Some(value) = texts.get(&key).filter(|value| !value.is_empty() && !value.starts_with('@')) else {
            continue;
        };
        let place = name.split_once('.').map_or(name, |(_, rest)| rest).to_lowercase();
        match found.iter_mut().find(|(known, _)| *known == place) {
            Some((_, slot)) => slot.clone_from(value),
            None => found.push((place, value.clone())),
        }
    }
    found
}

/// The names of the places of the DataForge `bytes`, named from the English `global.ini` `ini`.
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
    let places = names.iter().map(|(key, name)| (key.clone(), Json::str(name))).collect();
    let data = Json::Object(vec![
        ("format".into(), Json::int(FORMAT)),
        ("stand".into(), Json::str(stamp)),
        ("orte".into(), Json::Object(places)),
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
    let outcome = keep_current(Path::new(&archive), &paths.app_file(FILE));
    match &outcome {
        Keep::Failed(reason) => versekit_core::errors::record("location.refresh_names", reason),
        Keep::TooFew(count) => {
            versekit_core::trail::step(&format!("Ortsnamen: nur {count} erkannt, nicht abgelegt"));
        }
        _ => {}
    }
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use p4kx_testkit::{DataForgeBuilder, TestRecord, TestStruct, TestType, TestValue};

    use super::*;

    fn place(guid: u128, kind: &str, name: &str, key: &str) -> TestRecord {
        TestRecord::new(guid, kind, format!("{kind}.{name}"), format!("libs/foundry/records/starmap/{name}.xml"))
            .value("name", TestValue::text(key))
    }

    /// A database with `count` places, a placeholder, a record of another type and one without a text.
    fn forge(count: u128) -> Vec<u8> {
        let mut builder = DataForgeBuilder::new()
            .version(8)
            .structure(TestStruct::new(MAP_TYPE).field("name", TestType::Locale))
            .structure(TestStruct::new("OtherObject").field("name", TestType::Locale));
        for n in 0..count {
            builder = builder.record(place(0x5000 + n, MAP_TYPE, &format!("Stanton_Place{n}"), &format!("@place_{n}")));
        }
        builder
            .record(place(0x4000, MAP_TYPE, "Stanton2_Orison", "@Stanton2_Orison"))
            .record(place(0x4001, MAP_TYPE, "Empty_Thing", "@LOC_EMPTY"))
            .record(place(0x4002, "OtherObject", "Not_A_Place", "@Stanton2_Orison"))
            .record(place(0x4003, MAP_TYPE, "No_Text", "@no_such_key"))
            .build()
    }

    fn ini(count: u128) -> Vec<u8> {
        let mut text = String::from("\u{feff}Stanton2_Orison=Orison\r\nLOC_EMPTY=@LOC_EMPTY\r\n");
        for n in 0..count {
            text.push_str(&format!("place_{n},P=Place {n}\r\n"));
        }
        text.into_bytes()
    }

    #[test]
    fn the_places_are_named_from_the_english_texts_and_placeholders_stay_out() {
        let names = names_from_dataforge(&forge(2), &ini(2)).expect("names");
        assert_eq!(
            names,
            [
                ("stanton_place0".to_string(), "Place 0".to_string()),
                ("stanton_place1".to_string(), "Place 1".to_string()),
                ("stanton2_orison".to_string(), "Orison".to_string()),
            ],
            "only places, keyed by the record name in lower case"
        );
    }

    #[test]
    fn the_file_is_written_once_per_state_of_the_archive_and_not_with_too_few_names() {
        let dir = versekit_core::testdir::Dir::new("ortsnamen");
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
        assert_eq!(kept.get("stand").and_then(Json::as_str), archive_stamp(&archive).as_deref());
        assert_eq!(kept.get("orte").and_then(|o| o.get("stanton2_orison")), Some(&Json::str("Orison")));
        // The location row reads the names from it.
        let names = versekit_core::location::names(&paths);
        assert_eq!(versekit_core::location::display_name("Stanton2_Orison", &names), "Orison");
        assert_eq!(
            keep_current_with(&archive, &file, || unreachable!("the same archive is not read again")),
            Keep::Current
        );
        // A patch: read again.
        std::fs::write(&archive, b"patched p4k").unwrap();
        let few = || names_from_dataforge(&forge(3), &ini(3));
        assert_eq!(keep_current_with(&archive, &file, few), Keep::TooFew(4), "too few names: the file stays");
        assert_eq!(jsonio::read_json(&file).unwrap(), kept);
        assert_eq!(keep_current_with(&archive, &file, read), Keep::Written);
        assert_eq!(
            jsonio::read_json(&file).unwrap().get("stand").and_then(Json::as_str),
            archive_stamp(&archive).as_deref()
        );
    }
}
