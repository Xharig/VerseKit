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

use p4kx_testkit::{DataForgeBuilder, TestInstance, TestRecord, TestStruct, TestType, TestValue};

use super::*;

const Q_IRON: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0001;
const Q_KOPION: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0002;
const Q_PEARL: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0003;
const Q_PLAGUE: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0004;
const Q_TEMPLATE: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0005;
const Q_NAMELESS: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0006;
const Q_SALDYNIUM: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0007;
const Q_QUARTZ: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0008;
const RT_IRON: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0010;
const RT_IRON_REFINED: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0011;
const RT_SALDYNIUM: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0012;
const RT_QUARTZ: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0013;
const HARVEST_KOPION: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0020;
const HARVEST_KOPION_CAVE: u128 = 0x3000_0000_0000_4000_8000_0000_0000_0021;

fn pointer(name: &str) -> TestType {
    TestType::StrongPointer(name.to_string())
}

fn class(name: &str) -> TestType {
    TestType::Class(name.to_string())
}

fn structures(builder: DataForgeBuilder) -> DataForgeBuilder {
    [
        TestStruct::new("CraftingQualityQuantizationRecord").field("steps", TestType::Int32),
        TestStruct::new("ResourceType")
            .field("displayName", TestType::Locale)
            .field("refinedVersion", TestType::Reference)
            .array("qualities", class("QualityLink")),
        TestStruct::new("QualityLink").field("quantization", TestType::Reference),
        TestStruct::new("DataForgeComponentParams"),
        TestStruct::new("EntityClassDefinition").array("Components", pointer("DataForgeComponentParams")),
        TestStruct::new("ResourceContainer").parent("DataForgeComponentParams").field("capacity", class("Capacity")),
        TestStruct::new("Capacity").field("microSCU", TestType::Int32),
    ]
    .into_iter()
    .fold(builder, DataForgeBuilder::structure)
}

fn record(guid: u128, type_name: &str, name: &str) -> TestRecord {
    TestRecord::new(guid, type_name, format!("{type_name}.{name}"), format!("libs/foundry/records/test/{name}.xml"))
}

fn quantization(guid: u128, stem: &str) -> TestRecord {
    record(guid, "CraftingQualityQuantizationRecord", &format!("Quantization_{stem}")).value("steps", TestValue::Int32(4))
}

fn resource(guid: u128, name: &str, key: &str, refined: Option<u128>, links: &[u128]) -> TestRecord {
    let links = links
        .iter()
        .map(|guid| TestValue::Struct(TestInstance::new("QualityLink").value("quantization", TestValue::Reference(*guid))))
        .collect();
    record(guid, "ResourceType", name)
        .value("displayName", TestValue::text(key))
        .value("refinedVersion", refined.map_or(TestValue::Null, TestValue::Reference))
        .value("qualities", TestValue::Array(links))
}

fn harvestable(guid: u128, name: &str, micro: i32) -> TestRecord {
    let container = TestInstance::new("ResourceContainer")
        .value("capacity", TestValue::Struct(TestInstance::new("Capacity").value("microSCU", TestValue::Int32(micro))));
    record(guid, "EntityClassDefinition", name).value("Components", TestValue::Array(vec![TestValue::Struct(container)]))
}

fn forge() -> Vec<u8> {
    structures(DataForgeBuilder::new().version(8))
        .record(quantization(Q_IRON, "Iron"))
        .record(quantization(Q_KOPION, "KopionHorn"))
        .record(quantization(Q_PEARL, "VLKApexPearl"))
        .record(quantization(Q_PLAGUE, "AmioshiPlague"))
        .record(quantization(Q_TEMPLATE, "Template"))
        .record(quantization(Q_NAMELESS, "Nothing"))
        .record(quantization(Q_SALDYNIUM, "Saldynium"))
        .record(quantization(Q_QUARTZ, "Quartz"))
        // Linked by reference, refined: counted in SCU.
        .record(resource(RT_IRON, "Ore_Iron", "@items_commodities_iron_ore", Some(RT_IRON_REFINED), &[Q_IRON]))
        .record(resource(RT_IRON_REFINED, "Iron", "@items_commodities_iron", None, &[]))
        // Found by its name only, without a refined version: in pieces.
        .record(resource(RT_SALDYNIUM, "Ore_Saldynium", "@items_commodities_saldynium_ore", None, &[]))
        // A name that is only a key: the next way is taken.
        .record(resource(RT_QUARTZ, "Raw_Quartz", "@items_commodities_quartz_missing", None, &[Q_QUARTZ]))
        .record(harvestable(HARVEST_KOPION_CAVE, "Harvestable_Kopion_KopionHorn_Cave", 5000))
        .record(harvestable(HARVEST_KOPION, "Harvestable_Kopion_KopionHorn", 1000))
        .build()
}

const INI: &str = "\u{feff}items_commodities_iron_ore=Iron (Ore)\n\
items_commodities_iron_ore=Second line loses\n\
items_commodities_kopionhorn,P=  Kopion Horn  \n\
items_commodities_valakkar_pearl_apex=Valakkar Pearl (Apex)\n\
items_commodities_valakkar_pearl_apex_tier1=Valakkar Pearl (Apex) Tier 1\n\
items_commodities_valakkar_pearl_apex_desc=Not a name\n\
items_commodities_amoishiplague=Amioshi Plague\n\
items_commodities_saldynium_ore=Saldynium (Ore)\n\
items_commodities_quartz=Quartz (Raw)\n";

fn goods() -> Vec<Good> {
    goods_from_dataforge(&forge(), INI.as_bytes()).expect("forge")
}

#[test]
fn each_way_of_naming_and_counting() {
    let found: Vec<(String, &str, Option<f64>)> =
        goods().into_iter().map(|g| (g.name, g.unit.key(), g.piece_scu)).collect();
    assert_eq!(
        found,
        [
            ("Amioshi Plague".to_string(), "stueck", None),
            ("Iron (Ore)".to_string(), "scu", None),
            ("Kopion Horn".to_string(), "stueck", Some(0.001)),
            ("Quartz (Raw)".to_string(), "stueck", None),
            ("Saldynium (Ore)".to_string(), "stueck", None),
            ("Valakkar Pearl (Apex)".to_string(), "stueck", None),
        ],
        "linked, by name, direct key, words with a family, typo; no template, nothing unnamed"
    );
}

#[test]
fn words_split_like_the_pattern() {
    assert_eq!(words("QuasiTongue"), ["quasi", "tongue"]);
    assert_eq!(words("VLKApexPearl"), ["vlk", "apex", "pearl"]);
    assert_eq!(words("ABc"), ["a", "bc"]);
    assert_eq!(words("Tier1_ABC"), ["tier", "1", "abc"]);
    assert!(word_hit("vlk", "valakkar_pearl"), "a short word as an abbreviation");
    assert!(!word_hit("valk", "vlakkar"), "the letters must stand in order");
    assert!(!word_hit("plague", "amoishi"));
}

#[test]
fn the_ini_keeps_the_first_line_of_a_key() {
    let texts = parse_ini(INI.as_bytes());
    assert_eq!(texts.get("items_commodities_iron_ore").map(String::as_str), Some("Iron (Ore)"));
    assert_eq!(texts.get("items_commodities_kopionhorn").map(String::as_str), Some("Kopion Horn"));
}

#[test]
fn the_file_is_written_once_per_state_of_the_archive() {
    let dir = std::env::temp_dir().join(format!("versekit-waren-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let archive = dir.join("Data.p4k");
    std::fs::write(&archive, b"archive").expect("archive");
    let file = dir.join(FILE).to_string_lossy().to_string();
    let many = |n: usize| (0..n).map(|i| Good { name: format!("G{i:02}"), unit: Unit::Scu, piece_scu: None }).collect();

    assert_eq!(keep_current_with(&archive, &file, || Ok(many(MINIMUM - 1))), Keep::TooFew(MINIMUM - 1));
    assert!(!Path::new(&file).exists(), "too few: nothing is kept");
    assert_eq!(keep_current_with(&archive, &file, || Ok(many(MINIMUM))), Keep::Written);
    let mut read = false;
    assert_eq!(
        keep_current_with(&archive, &file, || {
            read = true;
            Ok(many(MINIMUM))
        }),
        Keep::Current
    );
    assert!(!read, "the same archive is not read again");

    let text = std::fs::read_to_string(&file).expect("file");
    std::fs::write(&file, text.replace("\"format\": 4", "\"format\": 3")).expect("old layout");
    assert_eq!(keep_current_with(&archive, &file, || Ok(many(MINIMUM))), Keep::Written, "an older layout is read again");
    assert_eq!(
        keep_current_with(&archive, &file, || Err(Error::MissingEntry { entry: "Data\\Game2.dcb".into() })),
        Keep::Current
    );
    std::fs::write(&archive, b"patched archive").expect("patch");
    assert!(matches!(
        keep_current_with(&archive, &file, || Err(Error::MissingEntry { entry: "Data\\Game2.dcb".into() })),
        Keep::Failed(_)
    ));
    assert!(std::fs::read_to_string(&file).expect("kept").contains("G19"), "a failed read keeps the list");
    assert_eq!(keep_current_with(&dir.join("none.p4k"), &file, || Ok(many(MINIMUM))), Keep::NoArchive);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_file_has_the_layout_of_the_python_version() {
    let goods = [
        Good { name: "Iron (Ore)".into(), unit: Unit::Scu, piece_scu: None },
        Good { name: "Kopion Horn".into(), unit: Unit::Piece, piece_scu: Some(0.00092) },
    ];
    assert_eq!(
        file_text("1:2", &goods),
        "{\n \"format\": 4,\n \"stand\": \"1:2\",\n \"waren\": [\n  [\n   \"Iron (Ore)\",\n   \"scu\",\n   null\n  ],\n  \
         [\n   \"Kopion Horn\",\n   \"stueck\",\n   0.00092\n  ]\n ]\n}"
    );
}
