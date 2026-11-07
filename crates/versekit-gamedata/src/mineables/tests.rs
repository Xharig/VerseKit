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

use p4kx_testkit::{DataForgeBuilder, GlobalIniBuilder, TestInstance, TestRecord, TestStruct, TestType, TestValue};

use super::*;
use crate::Language;

const RADAR: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0001;
const COMPOSITION: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0002;
const ROCK_C: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0010;
const ROCK_ZERO: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0011;
const ROCK_SHORT: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0012;
const ROCK_TEST: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0013;
const ROCK_ELSEWHERE: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0014;
const ROCK_BARE: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0015;
const ROCK_SIX: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0016;
const ROCK_NINE: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0017;
const STATION: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0020;
const STATION_TEMPLATE: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0021;
const NOT_A_STATION: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0022;
const SHIP_STATION: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0023;
const SCAN_KEY: &str = "@hud_scanning_info_rs_signature";

fn pointer(name: &str) -> TestType {
    TestType::StrongPointer(name.to_string())
}

fn class(name: &str) -> TestType {
    TestType::Class(name.to_string())
}

fn structures(builder: DataForgeBuilder) -> DataForgeBuilder {
    [
        TestStruct::new("DataForgeComponentParams"),
        TestStruct::new("EntityClassDefinition").array("Components", pointer("DataForgeComponentParams")),
        TestStruct::new("MineableParams").parent("DataForgeComponentParams").field("composition", TestType::Reference),
        TestStruct::new("SSCSignatureSystemParams")
            .parent("DataForgeComponentParams")
            .field("radarProperties", class("RadarProperties")),
        TestStruct::new("RadarProperties").field("baseSignatureParams", class("BaseSignatureParams")),
        TestStruct::new("BaseSignatureParams").array("signatures", TestType::Single),
        TestStruct::new("MineableComposition").field("depositName", TestType::Locale),
        TestStruct::new("RadarSystemGlobalParams").field("signatureSystemParams", class("SignatureSystemParams")),
        TestStruct::new("SignatureSystemParams").array("signatureTypeParams", class("SignatureTypeParams")),
        TestStruct::new("SignatureTypeParams").field("displayName", TestType::Locale),
        TestStruct::new("CrafterComponentParams")
            .parent("DataForgeComponentParams")
            .field("maxJobsInHistory", TestType::Int32)
            .array("queues", class("CrafterQueue")),
        TestStruct::new("CrafterQueue")
            .field("debugName", TestType::String)
            .field("maxJobsInProgress", TestType::Int32)
            .field("maxJobsWaiting", TestType::Int32),
    ]
    .into_iter()
    .fold(builder, DataForgeBuilder::structure)
}

fn record(guid: u128, type_name: &str, name: &str, path: &str) -> TestRecord {
    TestRecord::new(guid, type_name, format!("{type_name}.{name}"), format!("libs/foundry/records/{path}.xml"))
}

/// The radar parameters: one entry per signature type, the fifth is the scan signature.
fn radar(guid: u128, name: &str, path: &str, keys: &[&str]) -> TestRecord {
    let entries = keys
        .iter()
        .map(|key| {
            TestValue::Struct(TestInstance::new("SignatureTypeParams").value("displayName", TestValue::text(*key)))
        })
        .collect();
    record(guid, "RadarSystemGlobalParams", name, path).value(
        "signatureSystemParams",
        TestValue::Struct(
            TestInstance::new("SignatureSystemParams").value("signatureTypeParams", TestValue::Array(entries)),
        ),
    )
}

fn five_types() -> [&'static str; 8] {
    ["@ir", "@em", "@cs", "@db", SCAN_KEY, "@x1", "@x2", "@x3"]
}

fn rock(guid: u128, name: &str, path: &str, signatures: &[f32]) -> TestRecord {
    let values = signatures.iter().map(|s| TestValue::Single(*s)).collect();
    let signature = TestInstance::new("SSCSignatureSystemParams").value(
        "radarProperties",
        TestValue::Struct(TestInstance::new("RadarProperties").value(
            "baseSignatureParams",
            TestValue::Struct(TestInstance::new("BaseSignatureParams").value("signatures", TestValue::Array(values))),
        )),
    );
    let mining = TestInstance::new("MineableParams").value("composition", TestValue::Reference(COMPOSITION));
    record(guid, "EntityClassDefinition", name, path)
        .value("Components", TestValue::Array(vec![TestValue::Struct(mining), TestValue::Struct(signature)]))
}

fn station(guid: u128, name: &str, path: &str, history: i32, queues: &[(&str, i32, i32)]) -> TestRecord {
    let queues = queues
        .iter()
        .map(|(queue, progress, waiting)| {
            TestValue::Struct(
                TestInstance::new("CrafterQueue")
                    .value("debugName", TestValue::text(*queue))
                    .value("maxJobsInProgress", TestValue::Int32(*progress))
                    .value("maxJobsWaiting", TestValue::Int32(*waiting)),
            )
        })
        .collect();
    let params = TestInstance::new("CrafterComponentParams")
        .value("maxJobsInHistory", TestValue::Int32(history))
        .value("queues", TestValue::Array(queues));
    record(guid, "EntityClassDefinition", name, path)
        .value("Components", TestValue::Array(vec![TestValue::Struct(params)]))
}

fn forge(radar_records: Vec<TestRecord>) -> Vec<u8> {
    let mut builder = structures(DataForgeBuilder::new().version(8));
    for radar in radar_records {
        builder = builder.record(radar);
    }
    builder
        .record(
            record(COMPOSITION, "MineableComposition", "Asteroid_C", "mining/rockcompositionpresets/asteroid_c")
                .value("depositName", TestValue::text("@deposit_asteroid_c")),
        )
        .record(rock(
            ROCK_C,
            "AsteroidCTypeMineableRock_Iron",
            "entities/mineable/asteroid_c_iron",
            &[0.0, 0.0, 0.0, 0.0, 4700.0, 0.0, 0.0, 0.0],
        ))
        .record(rock(ROCK_ZERO, "MineableRock_Zero", "entities/mineable/zero", &[0.0; 8]))
        .record(rock(ROCK_SHORT, "MineableRock_Short", "entities/mineable/short", &[0.0, 5.0, 0.0, 0.0]))
        .record(rock(
            ROCK_TEST,
            "MineableRock_test_rock",
            "entities/mineable/test_rock",
            &[0.0, 0.0, 0.0, 0.0, 4000.0, 0.0, 0.0, 0.0],
        ))
        .record(rock(
            ROCK_ELSEWHERE,
            "ElsewhereRock",
            "entities/scitem/elsewhere",
            &[0.0, 0.0, 0.0, 0.0, 3000.0, 0.0, 0.0, 0.0],
        ))
        .record(record(ROCK_BARE, "EntityClassDefinition", "MineableRock_Bare", "entities/mineable/bare"))
        .record(rock(ROCK_SIX, "MineableRock_Six", "entities/mineable/six", &[0.0, 0.0, 0.0, 0.0, 5.0, 0.0]))
        .record(rock(
            ROCK_NINE,
            "MineableRock_Nine",
            "entities/mineable/nine",
            &[0.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0],
        ))
        .record(station(
            STATION,
            "Fabricator_One",
            "entities/basebuilding/interactables/fabricator_one",
            10,
            &[("Create/Repair/Upgrade/Refine", 2, 4), ("Dismantle", 4, 4)],
        ))
        .record(station(
            STATION_TEMPLATE,
            "Fabricator_template",
            "entities/basebuilding/interactables/fabricator_template",
            99,
            &[("Create", 1, 1)],
        ))
        .record(record(NOT_A_STATION, "EntityClassDefinition", "Plain", "entities/scitem/plain"))
        .record(station(SHIP_STATION, "Ship_Crafter", "entities/scitem/ships/crafter", 5, &[("Create", 1, 2)]))
        .build()
}

fn texts() -> Localization {
    let english = GlobalIniBuilder::new().text("deposit_asteroid_c", "Asteroid (C-Type)").build();
    Localization::from_files(Language::English, &english, None).expect("texts")
}

#[test]
fn a_rock_shows_the_slot_the_radar_parameters_name_and_the_name_of_its_deposit() {
    let bytes = forge(vec![radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &five_types())]);
    let rocks = rocks_from_dataforge(&bytes, &texts()).expect("rocks");
    let names: Vec<&str> = rocks.iter().map(|r| r.class_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "AsteroidCTypeMineableRock_Iron",
            "MineableRock_Zero",
            "MineableRock_Short",
            "MineableRock_Bare",
            "MineableRock_Six",
            "MineableRock_Nine"
        ],
        "by GUID; neither the leftover nor the entity outside the folder"
    );
    assert_eq!(
        (rocks[4].scan_signature, rocks[5].scan_signature),
        (None, None),
        "a list longer or shorter than the signature types, with a number in the slot, is no signature"
    );
    let iron = &rocks[0];
    assert_eq!(iron.scan_signature, Some(4700.0));
    assert_eq!(iron.deposit.as_deref(), Some("Asteroid (C-Type)"));
    assert_eq!(iron.signatures.len(), 8);
    assert_eq!(rocks[1].scan_signature, None, "all zero is no signature");
    assert_eq!(rocks[2].scan_signature, None, "not as many numbers as signature types");
    assert_eq!((rocks[3].scan_signature, rocks[3].signatures.len(), rocks[3].deposit.clone()), (None, 0, None));
}

#[test]
fn without_one_clear_slot_no_rock_has_a_scan_signature() {
    let none = forge(Vec::new());
    let ambiguous = forge(vec![
        radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &five_types()),
        radar(RADAR + 100, "second", "radarsystem/second", &five_types()),
    ]);
    let doubled = forge(vec![radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &[SCAN_KEY, SCAN_KEY])]);
    let unnamed = forge(vec![radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &["@a", "@b"])]);
    for (what, bytes) in
        [("no record", none), ("two records", ambiguous), ("two entries", doubled), ("no entry", unnamed)]
    {
        let rocks = rocks_from_dataforge(&bytes, &texts()).expect("rocks");
        assert!(!rocks.is_empty() && rocks.iter().all(|r| r.scan_signature.is_none()), "{what}");
    }
}

#[test]
fn a_crafting_station_gives_the_limits_of_its_queues_and_leftovers_and_plain_entities_are_none() {
    let bytes = forge(Vec::new());
    let stations = crafters_from_dataforge(&bytes).expect("crafters");
    let names: Vec<&str> = stations.iter().map(|c| c.class_name.as_str()).collect();
    assert_eq!(names, ["Fabricator_One", "Ship_Crafter"], "by GUID; not the template, not the plain entity");
    let one = &stations[0];
    assert_eq!(one.max_jobs_in_history, Some(10));
    assert_eq!(
        one.queues,
        [
            Queue {
                name: Some("Create/Repair/Upgrade/Refine".into()),
                max_jobs_in_progress: Some(2),
                max_jobs_waiting: Some(4)
            },
            Queue { name: Some("Dismantle".into()), max_jobs_in_progress: Some(4), max_jobs_waiting: Some(4) },
        ]
    );
    assert_eq!(stations[1].queues.len(), 1);
}

#[test]
fn the_file_of_signatures_lists_the_rocks_with_a_scan_signature_only() {
    let bytes = forge(vec![radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &five_types())]);
    let rocks = rocks_from_dataforge(&bytes, &texts()).expect("rocks");
    let text = signatures_text(&rocks, Some("4.10.2-live.1"), None);
    let file = versekit_core::jsonio::loads(&text).expect("json");
    assert_eq!(file.get("format"), Some(&Json::int(1)));
    assert_eq!(file.get("build").and_then(Json::as_str), Some("4.10.2-live.1"));
    let Some(Json::Array(entries)) = file.get("steine") else { panic!("steine") };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].get("klasse").and_then(Json::as_str), Some("AsteroidCTypeMineableRock_Iron"));
    assert_eq!(entries[0].get("name").and_then(Json::as_str), Some("Asteroid (C-Type)"));
    assert_eq!(entries[0].get("signatur"), Some(&Json::Float(4700.0)));

    let folder = versekit_core::testdir::fresh("gamedata-signaturen");
    let path = folder.join(SIGNATURES_FILE).to_string_lossy().into_owned();
    write_signatures(&path, &rocks, None, None).expect("written");
    let read = std::fs::read_to_string(&path).expect("file");
    assert_eq!(versekit_core::jsonio::loads(&read).expect("json").get("build"), Some(&Json::Null));
    assert_eq!(versekit_core::jsonio::loads(&read).expect("json").get("archiv"), None, "no archive, no stamp");
}

fn rocks_of_the_forge() -> Vec<Rock> {
    let bytes = forge(vec![radar(RADAR, "global", "radarsystem/radarsystemglobalparams", &five_types())]);
    rocks_from_dataforge(&bytes, &texts()).expect("rocks")
}

/// A folder with a made-up archive file `Data.p4k` and the path of the file of signatures beside it.
fn archive_in(name: &str) -> (std::path::PathBuf, String, std::path::PathBuf) {
    let folder = versekit_core::testdir::fresh(name);
    let archive = folder.join("Data.p4k");
    std::fs::write(&archive, b"archive one").expect("archive");
    let file = folder.join(SIGNATURES_FILE).to_string_lossy().into_owned();
    (archive, file, folder)
}

#[test]
fn the_file_of_signatures_is_current_only_for_the_archive_it_was_read_from() {
    let (archive, file, _folder) = archive_in("gamedata-signaturen-stempel");
    let stamp = ArchiveStamp::of(&archive).expect("stamp");
    assert!(!is_current(&file, stamp), "no file yet");
    write_signatures(&file, &rocks_of_the_forge(), None, Some(stamp)).expect("written");
    assert!(is_current(&file, stamp));
    let text = std::fs::read_to_string(&file).expect("file");
    assert!(versekit_core::jsonio::loads(&text).expect("json").get("archiv").is_some());
    // Another size or another time is another archive.
    std::fs::write(&archive, b"archive two, longer").expect("archive");
    let changed = ArchiveStamp::of(&archive).expect("stamp");
    assert_ne!(changed, stamp);
    assert!(!is_current(&file, changed));
    let later = ArchiveStamp { modified: stamp.modified + 1, ..stamp };
    assert!(!is_current(&file, later), "the same size, another time");
    // A file without a stamp, of another format or unreadable is never current.
    write_signatures(&file, &rocks_of_the_forge(), None, None).expect("written");
    assert!(!is_current(&file, stamp), "written without a stamp");
    std::fs::write(&file, r#"{"format": 2, "archiv": {"groesse": 1, "zeit": 1}}"#).expect("file");
    assert!(!is_current(&file, ArchiveStamp { size: 1, modified: 1 }), "another format");
    std::fs::write(&file, "{kaputt").expect("file");
    assert!(!is_current(&file, stamp));
    assert_eq!(ArchiveStamp::of(&archive.with_file_name("fehlt.p4k")), None);
}

#[test]
fn the_archive_is_read_when_it_has_changed_and_not_otherwise() {
    use std::cell::Cell;
    let (archive, file, folder) = archive_in("gamedata-signaturen-halten");
    let reads = Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        Ok((rocks_of_the_forge(), Some("4.10.2-live.1".to_string())))
    };
    // No archive: nothing is read, nothing written.
    let missing = folder.join("fehlt.p4k");
    assert_eq!(keep_current_with(&missing, &file, read), Keep::NoArchive);
    assert_eq!((reads.get(), std::path::Path::new(&file).exists()), (0, false));
    // The first start reads and writes; the next ones find the file current.
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Written);
    assert_eq!(reads.get(), 1);
    let text = std::fs::read_to_string(&file).expect("file");
    let written = versekit_core::jsonio::loads(&text).expect("json");
    assert_eq!(written.get("build").and_then(Json::as_str), Some("4.10.2-live.1"));
    assert!(matches!(written.get("steine"), Some(Json::Array(entries)) if entries.len() == 1));
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Current);
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Current);
    assert_eq!(reads.get(), 1, "a start with an unchanged archive reads nothing");
    // A patch changes the archive: it is read again, once.
    std::fs::write(&archive, b"the archive after a patch").expect("archive");
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Written);
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Current);
    assert_eq!(reads.get(), 2);
    // A reading that fails leaves the file as it was and is tried again at the next start.
    std::fs::write(&archive, b"another patch, longer than the one before").expect("archive");
    let before = std::fs::read_to_string(&file).expect("file");
    let failing = || {
        reads.set(reads.get() + 1);
        Err(Error::NoGameFolder)
    };
    assert!(matches!(keep_current_with(&archive, &file, failing), Keep::Failed(_)));
    assert_eq!(std::fs::read_to_string(&file).expect("file"), before);
    assert_eq!(keep_current_with(&archive, &file, read), Keep::Written);
    assert_eq!(reads.get(), 4);
}

fn crafter(history: Option<i64>, queues: &[(&str, Option<i64>, Option<i64>)]) -> Crafter {
    Crafter {
        guid: String::new(),
        class_name: String::new(),
        record_path: String::new(),
        max_jobs_in_history: history,
        queues: queues
            .iter()
            .map(|(name, progress, waiting)| Queue {
                name: Some((*name).to_string()),
                max_jobs_in_progress: *progress,
                max_jobs_waiting: *waiting,
            })
            .collect(),
    }
}

#[test]
fn the_limits_of_the_fabricator_come_from_the_first_station_that_states_them() {
    let bytes = forge(Vec::new());
    let stations = crafters_from_dataforge(&bytes).expect("crafters");
    assert_eq!(
        crafter_limits(&stations),
        Some(CrafterLimits { in_progress: 2, waiting: 4, history: 10, dismantle: Some((4, 4)) })
    );
    let work = ("Create", Some(3), Some(5));
    let take_apart = ("Dismantle", Some(6), Some(7));
    // Without a queue of dismantling there is none to show.
    let plain = crafter(Some(8), &[work]);
    assert_eq!(
        crafter_limits(std::slice::from_ref(&plain)),
        Some(CrafterLimits { in_progress: 3, waiting: 5, history: 8, dismantle: None })
    );
    // The queue of dismantling stands beside the other one in whatever order.
    let first = crafter(Some(8), &[take_apart, work]);
    assert_eq!(
        crafter_limits(&[first]),
        Some(CrafterLimits { in_progress: 3, waiting: 5, history: 8, dismantle: Some((6, 7)) })
    );
    // A station that lacks the history, the working queue or a number is passed over for the next one.
    let no_history = crafter(None, &[work]);
    let only_dismantle = crafter(Some(1), &[take_apart]);
    let half = crafter(Some(1), &[("Create", Some(1), None)]);
    assert_eq!(
        crafter_limits(&[no_history, only_dismantle, half, plain]),
        Some(CrafterLimits { in_progress: 3, waiting: 5, history: 8, dismantle: None })
    );
    assert_eq!(crafter_limits(&[]), None);
    assert_eq!(crafter_limits(&[crafter(Some(1), &[])]), None);
}

#[test]
fn the_game_values_of_an_archive_carry_the_limits_and_an_older_cache_without_them_still_reads() {
    let bytes = forge(Vec::new());
    let values = crate::values_from_dataforge(&bytes, &texts(), None).expect("values");
    assert_eq!(values.crafter_limits.map(|l| (l.in_progress, l.waiting, l.history)), Some((2, 4, 10)));
    let mut stored = serde_json::to_value(&values).expect("json");
    assert!(stored.as_object_mut().expect("object").remove("crafter_limits").is_some());
    let old: crate::values::GameValues = serde_json::from_value(stored).expect("an older cache");
    assert_eq!(old.crafter_limits, None);
    // A source without game data (scmdb) states no limits.
    assert_eq!(crate::values::GameValues::default().crafter_limits, None);
}

#[test]
fn an_archive_that_cannot_be_opened_is_a_failure_not_a_file() {
    let (archive, file, _folder) = archive_in("gamedata-signaturen-kein-archiv");
    // The made-up file is no archive: reading fails, nothing is written.
    assert!(matches!(keep_current(&archive, &file), Keep::Failed(_)));
    assert!(!std::path::Path::new(&file).exists());
}
