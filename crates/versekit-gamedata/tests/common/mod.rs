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

//! The small synthetic `Data.p4k` the tests read, written under the target
//! folder.

use std::fs;
use std::path::{Path, PathBuf};

use p4kx_testkit::{
    ArchiveBuilder, DataForgeBuilder, GlobalIniBuilder, TestEntry, TestInstance, TestRecord, TestStruct, TestType,
    TestValue, write_build_manifest,
};

const MAKER: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0001;
const RIFLE: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0002;
const BP_RIFLE: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0003;
const BP_TEST: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0004;
const BP_LONELY: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0005;
const BP_DISMANTLE: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0006;
const POOL: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0007;
const FACTION_REP: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0008;
const STANDING: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0009;
const REP_AMOUNT: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000a;
const LOCALITY: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000b;
const PLANET: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000c;
const GENERATOR: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000d;
const SCENARIO: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000e;
const FACTION: u128 = 0x1000_0000_0000_4000_8000_0000_0000_000f;
const TEMPLATE: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0010;
const MISSION_TYPE: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0011;
const ORGANIZATION: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0012;
const TAG_NAME: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0013;
const TAG_OTHER: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0014;
const REP_AFFINITY: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0015;
const REP_LOSS: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0016;
const LOCALITY_SUB: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0017;
const PLANET_SUB: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0018;
const MISSION_TYPE_SUB: u128 = 0x1000_0000_0000_4000_8000_0000_0000_0019;
const STANDING_MAX: u128 = 0x1000_0000_0000_4000_8000_0000_0000_001a;
const GLOBALS: u128 = 0x1000_0000_0000_4000_8000_0000_0000_001b;
const PISTOL: u128 = 0x1000_0000_0000_4000_8000_0000_0000_001c;
const BP_PISTOL: u128 = 0x1000_0000_0000_4000_8000_0000_0000_001d;
/// The `id`s of the contracts and their sub contracts.
const HUNT_EASY: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0001;
const HUNT_UNRELEASED: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0002;
const HUNT_UNTITLED: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0003;
const HUNT_PAID: u128 = 0x2000_0000_0000_4000_8000_0000_0000_0004;
const SUB_OFFSET: u128 = 0x10;

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
        TestStruct::new("SAttachableComponentParams")
            .parent("DataForgeComponentParams")
            .field("AttachDef", class("SItemDefinition")),
        TestStruct::new("SItemDefinition")
            .field("Type", TestType::Enum("EItemType".to_string()))
            .field("SubType", TestType::Enum("EItemSubType".to_string()))
            .field("Size", TestType::Int32)
            .field("Grade", TestType::Int32)
            .field("Manufacturer", TestType::Reference)
            .field("Localization", class("SCItemLocalization")),
        TestStruct::new("SCItemLocalization").field("Name", TestType::Locale).field("Description", TestType::Locale),
        TestStruct::new("SCItemManufacturer")
            .field("Code", TestType::String)
            .field("Localization", class("SCItemLocalization")),
        TestStruct::new("CraftingBlueprintRecord").field("blueprint", pointer("CraftingBlueprint")),
        TestStruct::new("CraftingBlueprint")
            .field("blueprintName", TestType::Locale)
            .field("processSpecificData", pointer("CraftingProcess_Creation")),
        TestStruct::new("CraftingProcess_Creation").field("entityClass", TestType::Reference),
        TestStruct::new("CraftingGlobalParams").field("defaultBlueprintSelection", class("DefaultBlueprintSelection")),
        TestStruct::new("DefaultBlueprintSelection").array("blueprintRecords", TestType::Reference),
        TestStruct::new("BlueprintPoolRecord").array("blueprintRewards", class("BlueprintReward")),
        TestStruct::new("BlueprintReward")
            .field("weight", TestType::Int32)
            .field("blueprintRecord", TestType::Reference),
        TestStruct::new("FactionReputation").field("displayName", TestType::Locale),
        TestStruct::new("Faction").field("name", TestType::Locale).field("factionReputationRef", TestType::Reference),
        TestStruct::new("SReputationStandingParams")
            .field("displayName", TestType::Locale)
            .field("minReputation", TestType::Int32),
        TestStruct::new("SReputationRewardAmount").field("reputationAmount", TestType::Int32),
        TestStruct::new("MissionLocality").array("availableLocations", TestType::Reference),
        TestStruct::new("StarMapObject").field("name", TestType::Locale),
    ]
    .into_iter()
    .fold(builder, DataForgeBuilder::structure)
}

fn contract_structures(builder: DataForgeBuilder) -> DataForgeBuilder {
    [
        TestStruct::new("ContractGenerator").array("generators", pointer("ContractGeneratorHandler")),
        TestStruct::new("ContractGeneratorHandler")
            .field("notForRelease", TestType::Bool)
            .field("workInProgress", TestType::Bool)
            .field("debugName", TestType::String)
            .field("defaultAvailability", class("ContractAvailability"))
            .field("contractParams", class("ContractParamOverrides")),
        TestStruct::new("ContractGeneratorHandler_Career")
            .parent("ContractGeneratorHandler")
            .field("factionReputation", TestType::Reference)
            .array("contracts", pointer("Contract")),
        TestStruct::new("ContractAvailability")
            .array("prerequisites", pointer("ContractPrerequisite"))
            .field("hasPersonalCooldown", TestType::Bool)
            .field("personalCooldownTime", TestType::Single),
        TestStruct::new("ContractPrerequisite"),
        TestStruct::new("ContractPrerequisite_Locality")
            .parent("ContractPrerequisite")
            .field("localityAvailable", TestType::Reference),
        TestStruct::new("ContractParamOverrides")
            .array("stringParamOverrides", class("ContractStringParam"))
            .array("boolParamOverrides", class("ContractBoolParam"))
            .array("propertyOverrides", class("MissionProperty"))
            .field("missionTypeOverride", TestType::Reference),
        TestStruct::new("ContractStringParam").field("param", TestType::String).field("value", TestType::String),
        TestStruct::new("ContractBoolParam").field("param", TestType::String).field("value", TestType::Bool),
        TestStruct::new("MissionProperty")
            .field("extendedTextToken", TestType::String)
            .field("value", pointer("MissionPropertyValue")),
        TestStruct::new("MissionPropertyValue"),
        TestStruct::new("MissionPropertyValue_StringHash")
            .parent("MissionPropertyValue")
            .array("options", class("MissionPropertyValueOption_StringHash")),
        TestStruct::new("MissionPropertyValueOption_StringHash").field("textId", TestType::Locale),
        TestStruct::new("MissionPropertyValue_Location").parent("MissionPropertyValue"),
        TestStruct::new("MissionPropertyValue_Organization")
            .parent("MissionPropertyValue")
            .array("matchConditions", pointer("DataSetMatchCondition")),
        TestStruct::new("DataSetMatchCondition"),
        TestStruct::new("DataSetMatchCondition_SpecificOrganizationsDef")
            .parent("DataSetMatchCondition")
            .array("organizations", TestType::Reference),
        TestStruct::new("MissionOrganization").field("stringVariants", class("MissionStringVariants")),
        TestStruct::new("MissionStringVariants").array("variants", class("MissionStringVariant")),
        TestStruct::new("MissionStringVariant").field("string", TestType::Locale).field("tag", TestType::Reference),
        TestStruct::new("Tag").field("tagName", TestType::String),
        TestStruct::new("MissionType").field("LocalisedTypeName", TestType::Locale),
        TestStruct::new("ContractTemplate")
            .field("contractClass", pointer("ContractClass"))
            .field("contractDisplayInfo", class("ContractDisplayInfo"))
            .array("contractProperties", class("MissionProperty")),
        TestStruct::new("ContractClass").field("additionalParams", class("ActiveContractSettings")),
        TestStruct::new("ActiveContractSettings").field("canBeShared", TestType::Bool),
        TestStruct::new("ContractDisplayInfo")
            .array("displayString", TestType::Locale)
            .field("type", TestType::Reference),
        TestStruct::new("SubContract")
            .field("id", TestType::Guid)
            .array("stringParamOverrides", class("ContractStringParam"))
            .array("propertyOverrides", class("MissionProperty"))
            .field("missionTypeOverride", TestType::Reference)
            .array("additionalPrerequisites", pointer("ContractPrerequisite")),
        TestStruct::new("Contract")
            .field("id", TestType::Guid)
            .field("notForRelease", TestType::Bool)
            .field("workInProgress", TestType::Bool)
            .field("debugName", TestType::String)
            .field("template", TestType::Reference)
            .field("paramOverrides", class("ContractParamOverrides"))
            .array("subContracts", class("SubContract"))
            .array("additionalPrerequisites", pointer("ContractPrerequisite"))
            .field("contractResults", class("ContractResults")),
        TestStruct::new("CareerContract")
            .parent("Contract")
            .field("minStanding", TestType::Reference)
            .field("maxStanding", TestType::Reference),
        TestStruct::new("ContractResults").array("contractResults", pointer("ContractResult")),
        TestStruct::new("ContractResult").array("missionResults", TestType::Bool),
        TestStruct::new("BlueprintRewards")
            .parent("ContractResult")
            .field("chance", TestType::Single)
            .field("blueprintPool", TestType::Reference),
        TestStruct::new("ContractResult_LegacyReputation")
            .parent("ContractResult")
            .field("contractResultReputationAmounts", class("SReputationAmountParams")),
        TestStruct::new("SReputationAmountParams")
            .field("factionReputation", TestType::Reference)
            .field("reward", TestType::Reference),
        TestStruct::new("ContractResult_Reward")
            .parent("ContractResult")
            .field("contractReward", class("MissionReward")),
        TestStruct::new("MissionReward").field("reward", TestType::Int32),
        TestStruct::new("ScenarioProgress").array("factionRewardTiers", class("SScenarioProgressRewardsTiers")),
        TestStruct::new("SScenarioProgressRewardsTiers")
            .field("faction", TestType::Reference)
            .array("tierProgressions", class("STierProgressions")),
        TestStruct::new("STierProgressions")
            .field("progressionText", TestType::Locale)
            .array("tierRewards", class("STierReward")),
        TestStruct::new("STierReward").field("minPoints", TestType::Int32).array("blueprintPool", TestType::Reference),
    ]
    .into_iter()
    .fold(builder, DataForgeBuilder::structure)
}

fn record(guid: u128, type_name: &str, name: &str, path: &str) -> TestRecord {
    TestRecord::new(guid, type_name, format!("{type_name}.{name}"), format!("libs/foundry/records/{path}.xml"))
}

fn localization(name: &str, description: &str) -> TestValue {
    TestValue::Struct(
        TestInstance::new("SCItemLocalization")
            .value("Name", TestValue::text(name))
            .value("Description", TestValue::text(description)),
    )
}

fn blueprint(guid: u128, name: &str, path: &str, item: Option<u128>) -> TestRecord {
    let creation = TestInstance::new("CraftingProcess_Creation")
        .value("entityClass", item.map_or(TestValue::Null, TestValue::Reference));
    record(guid, "CraftingBlueprintRecord", name, path).value(
        "blueprint",
        TestValue::Struct(
            TestInstance::new("CraftingBlueprint")
                .value("blueprintName", TestValue::text("@LOC_PLACEHOLDER"))
                .value("processSpecificData", TestValue::Struct(creation)),
        ),
    )
}

fn item_records(builder: DataForgeBuilder) -> DataForgeBuilder {
    let definition = TestInstance::new("SItemDefinition")
        .value("Type", TestValue::text("WeaponPersonal"))
        .value("SubType", TestValue::text("Rifle"))
        .value("Size", TestValue::Int32(3))
        .value("Grade", TestValue::Int32(2))
        .value("Manufacturer", TestValue::Reference(MAKER))
        .value("Localization", localization("@item_Namebehr_rifle", "@item_Descbehr_rifle"));
    builder
        .record(
            record(MAKER, "SCItemManufacturer", "BEHR", "scitemmanufacturer/behr")
                .value("Code", TestValue::text("BEH"))
                .value("Localization", localization("@manufacturer_NameBEHR", "@LOC_EMPTY")),
        )
        .record(record(RIFLE, "EntityClassDefinition", "behr_rifle", "entities/scitem/weapons/behr_rifle").value(
            "Components",
            TestValue::Array(vec![TestValue::Struct(
                TestInstance::new("SAttachableComponentParams").value("AttachDef", TestValue::Struct(definition)),
            )]),
        ))
        .record(blueprint(
            BP_RIFLE,
            "BP_CRAFT_behr_rifle",
            "crafting/blueprints/crafting/weapons/bp_craft_behr_rifle",
            Some(RIFLE),
        ))
        .record(blueprint(
            BP_TEST,
            "BP_CRAFT_test_rifle",
            "crafting/blueprints/crafting/weapons/bp_craft_test_rifle",
            Some(RIFLE),
        ))
        .record(record(PISTOL, "EntityClassDefinition", "s38_pistol", "entities/scitem/weapons/s38_pistol").value(
            "Components",
            TestValue::Array(vec![TestValue::Struct(TestInstance::new("SAttachableComponentParams").value(
                "AttachDef",
                TestValue::Struct(
                    TestInstance::new("SItemDefinition")
                        .value("Type", TestValue::text("WeaponPersonal"))
                        .value("SubType", TestValue::text("Rifle"))
                        .value("Size", TestValue::Int32(1))
                        .value("Grade", TestValue::Int32(1))
                        .value("Manufacturer", TestValue::Reference(MAKER))
                        .value("Localization", localization("@item_Names38_pistol", "@LOC_EMPTY")),
                ),
            ))]),
        ))
        .record(blueprint(
            BP_PISTOL,
            "BP_CRAFT_s38_pistol",
            "crafting/blueprints/crafting/weapons/bp_craft_s38_pistol",
            Some(PISTOL),
        ))
        .record(
            record(GLOBALS, "CraftingGlobalParams", "global", "crafting/craftingglobalparams").value(
                "defaultBlueprintSelection",
                TestValue::Struct(
                    TestInstance::new("DefaultBlueprintSelection")
                        .value("blueprintRecords", TestValue::Array(vec![TestValue::Reference(BP_PISTOL)])),
                ),
            ),
        )
        .record(blueprint(BP_LONELY, "BP_CRAFT_lonely", "crafting/blueprints/crafting/misc/bp_craft_lonely", None))
        .record(blueprint(BP_DISMANTLE, "BP_DISMANTLE", "crafting/blueprints/dismantle/bp_dismantle", None))
}

fn reputation_records(builder: DataForgeBuilder) -> DataForgeBuilder {
    builder
        .record(
            record(POOL, "BlueprintPoolRecord", "BP_MISSIONREWARD_rifles", "crafting/blueprintrewards/rifles").value(
                "blueprintRewards",
                TestValue::Array(vec![TestValue::Struct(
                    TestInstance::new("BlueprintReward")
                        .value("weight", TestValue::Int32(1))
                        .value("blueprintRecord", TestValue::Reference(BP_RIFLE)),
                )]),
            ),
        )
        .record(
            record(FACTION_REP, "FactionReputation", "highpoint", "factions/factionreputation/highpoint")
                .value("displayName", TestValue::text("@Highpoint_RepUI_Name")),
        )
        .record(
            record(FACTION, "Faction", "foxwell", "factions/faction_foxwell")
                .value("name", TestValue::text("@Foxwell_Name"))
                .value("factionReputationRef", TestValue::Null),
        )
        .record(
            record(STANDING, "SReputationStandingParams", "rank2", "reputation/standings/rank2")
                .value("displayName", TestValue::text("@Rank_Two"))
                .value("minReputation", TestValue::Int32(2500)),
        )
        .record(
            record(STANDING_MAX, "SReputationStandingParams", "rank6", "reputation/standings/rank6")
                .value("displayName", TestValue::text("@Rank_Top"))
                .value("minReputation", TestValue::Int32(95250)),
        )
        .record(
            record(REP_AMOUNT, "SReputationRewardAmount", "positive_150", "reputation/rewards/positive_150")
                .value("reputationAmount", TestValue::Int32(150)),
        )
        .record(
            record(LOCALITY, "MissionLocality", "stanton", "missiondata/pu_missionlocality/stanton")
                .value("availableLocations", TestValue::Array(vec![TestValue::Reference(PLANET)])),
        )
        .record(
            record(PLANET, "StarMapObject", "stanton1", "starmap/pu/system/stanton/stanton1")
                .value("name", TestValue::text("@Stanton1")),
        )
        .record(
            record(REP_AFFINITY, "SReputationRewardAmount", "positive_50", "reputation/rewards/positive_50")
                .value("reputationAmount", TestValue::Int32(50)),
        )
        .record(
            record(REP_LOSS, "SReputationRewardAmount", "negative_250", "reputation/rewards/negative_250")
                .value("reputationAmount", TestValue::Int32(-250)),
        )
        .record(
            record(LOCALITY_SUB, "MissionLocality", "stanton2", "missiondata/pu_missionlocality/stanton2")
                .value("availableLocations", TestValue::Array(vec![TestValue::Reference(PLANET_SUB)])),
        )
        .record(
            record(PLANET_SUB, "StarMapObject", "stanton2", "starmap/pu/system/stanton/stanton2")
                .value("name", TestValue::text("@Stanton2")),
        )
}

fn property(token: &str, value: TestInstance) -> TestValue {
    TestValue::Struct(
        TestInstance::new("MissionProperty")
            .value("extendedTextToken", TestValue::text(token))
            .value("value", TestValue::Struct(value)),
    )
}

fn variant(key: &str, tag: u128) -> TestValue {
    TestValue::Struct(
        TestInstance::new("MissionStringVariant")
            .value("string", TestValue::text(key))
            .value("tag", TestValue::Reference(tag)),
    )
}

/// The template, mission type, organisation and tags the hunt contract
/// fills its title from.
fn mission_records(builder: DataForgeBuilder) -> DataForgeBuilder {
    let display = TestInstance::new("ContractDisplayInfo")
        .value("displayString", TestValue::Array(vec![TestValue::text("@Template_Title")]))
        .value("type", TestValue::Reference(MISSION_TYPE));
    let location = property("Location", TestInstance::new("MissionPropertyValue_Location"));
    builder
        .record(
            record(TEMPLATE, "ContractTemplate", "hunt", "contracts/contracttemplates/hunt")
                .value(
                    "contractClass",
                    TestValue::Struct(TestInstance::new("ContractClass").value(
                        "additionalParams",
                        TestValue::Struct(
                            TestInstance::new("ActiveContractSettings").value("canBeShared", TestValue::Bool(true)),
                        ),
                    )),
                )
                .value("contractDisplayInfo", TestValue::Struct(display))
                .value("contractProperties", TestValue::Array(vec![location])),
        )
        .record(
            record(MISSION_TYPE, "MissionType", "Hunting", "missiontype/pu/hunting")
                .value("LocalisedTypeName", TestValue::text("@MissionType_Hunting")),
        )
        .record(
            record(MISSION_TYPE_SUB, "MissionType", "Special", "missiontype/pu/special")
                .value("LocalisedTypeName", TestValue::text("@MissionType_Special")),
        )
        .record(
            record(ORGANIZATION, "MissionOrganization", "Highpoint", "missiondata/pu_organizations/highpoint").value(
                "stringVariants",
                TestValue::Struct(TestInstance::new("MissionStringVariants").value(
                    "variants",
                    TestValue::Array(vec![variant("@Org_Other", TAG_OTHER), variant("@Org_Name", TAG_NAME)]),
                )),
            ),
        )
        .record(
            record(TAG_NAME, "Tag", "name", "tagdatabase/tagdatabase.tagdatabase")
                .value("tagName", TestValue::text("Name")),
        )
        .record(
            record(TAG_OTHER, "Tag", "other", "tagdatabase/tagdatabase.tagdatabase")
                .value("tagName", TestValue::text("Other")),
        )
}

fn string_list(params: &[(&str, &str)]) -> TestValue {
    let params = params
        .iter()
        .map(|(param, value)| {
            TestValue::Struct(
                TestInstance::new("ContractStringParam")
                    .value("param", TestValue::text(*param))
                    .value("value", TestValue::text(*value)),
            )
        })
        .collect();
    TestValue::Array(params)
}

fn string_params(params: &[(&str, &str)]) -> TestValue {
    TestValue::Struct(TestInstance::new("ContractParamOverrides").value("stringParamOverrides", string_list(params)))
}

/// The flags of a result that applies when the contract succeeds.
fn on_success() -> TestValue {
    TestValue::Array(vec![TestValue::Bool(true), TestValue::Bool(false)])
}

fn reputation(amount: u128, on_success: bool) -> TestValue {
    TestValue::Struct(
        TestInstance::new("ContractResult_LegacyReputation")
            .value("missionResults", TestValue::Array(vec![TestValue::Bool(on_success), TestValue::Bool(!on_success)]))
            .value(
                "contractResultReputationAmounts",
                TestValue::Struct(
                    TestInstance::new("SReputationAmountParams")
                        .value("factionReputation", TestValue::Reference(FACTION_REP))
                        .value("reward", TestValue::Reference(amount)),
                ),
            ),
    )
}

/// The hunt contract: its title has a fixed choice, a place the game picks
/// (named by the template) and the name of an organisation; one sub
/// contract with its own title and place; reputation for the faction and
/// its affinity on success, a loss on failure.
fn contract(id: u128, debug_name: &str, released: bool) -> TestValue {
    let creature = TestInstance::new("MissionPropertyValue_StringHash").value(
        "options",
        TestValue::Array(vec![TestValue::Struct(
            TestInstance::new("MissionPropertyValueOption_StringHash").value("textId", TestValue::text("@Creature")),
        )]),
    );
    let organization = TestInstance::new("MissionPropertyValue_Organization").value(
        "matchConditions",
        TestValue::Array(vec![TestValue::Struct(
            TestInstance::new("DataSetMatchCondition_SpecificOrganizationsDef")
                .value("organizations", TestValue::Array(vec![TestValue::Reference(ORGANIZATION)])),
        )]),
    );
    let params = TestInstance::new("ContractParamOverrides")
        .value("stringParamOverrides", string_list(&[("Title", "@Hunt_Title")]))
        .value(
            "propertyOverrides",
            TestValue::Array(vec![property("Creature", creature), property("Contractor", organization)]),
        )
        .value("missionTypeOverride", TestValue::Null);
    let sub_locality = TestInstance::new("ContractPrerequisite_Locality")
        .value("localityAvailable", TestValue::Reference(LOCALITY_SUB));
    let sub = TestInstance::new("SubContract")
        .value("id", TestValue::Guid(id + SUB_OFFSET))
        .value("stringParamOverrides", string_list(&[("Title", "@Hunt_Sub_Title")]))
        .value("propertyOverrides", TestValue::Array(Vec::new()))
        .value("missionTypeOverride", TestValue::Reference(MISSION_TYPE_SUB))
        .value("additionalPrerequisites", TestValue::Array(vec![TestValue::Struct(sub_locality)]));
    let results = vec![
        TestValue::Struct(
            TestInstance::new("BlueprintRewards")
                .value("missionResults", on_success())
                .value("chance", TestValue::Single(0.5))
                .value("blueprintPool", TestValue::Reference(POOL)),
        ),
        reputation(REP_AMOUNT, true),
        reputation(REP_AFFINITY, true),
        reputation(REP_LOSS, false),
        TestValue::Struct(TestInstance::new("ContractResult_Reward").value(
            "contractReward",
            TestValue::Struct(TestInstance::new("MissionReward").value("reward", TestValue::Int32(12000))),
        )),
    ];
    TestValue::Struct(
        TestInstance::new("CareerContract")
            .value("id", TestValue::Guid(id))
            .value("notForRelease", TestValue::Bool(!released))
            .value("debugName", TestValue::text(debug_name))
            .value("template", TestValue::Reference(TEMPLATE))
            .value("paramOverrides", TestValue::Struct(params))
            .value("subContracts", TestValue::Array(vec![TestValue::Struct(sub)]))
            .value("additionalPrerequisites", TestValue::Array(Vec::new()))
            .value(
                "contractResults",
                TestValue::Struct(
                    TestInstance::new("ContractResults").value("contractResults", TestValue::Array(results)),
                ),
            )
            .value("minStanding", TestValue::Reference(STANDING))
            .value("maxStanding", TestValue::Reference(STANDING_MAX)),
    )
}

/// A contract without a title of its own: the template's title stands in.
fn untitled_contract() -> TestValue {
    let results = vec![TestValue::Struct(
        TestInstance::new("BlueprintRewards")
            .value("missionResults", on_success())
            .value("chance", TestValue::Single(1.0))
            .value("blueprintPool", TestValue::Reference(POOL)),
    )];
    TestValue::Struct(
        TestInstance::new("CareerContract")
            .value("id", TestValue::Guid(HUNT_UNTITLED))
            .value("debugName", TestValue::text("Hunt_Untitled"))
            .value("template", TestValue::Reference(TEMPLATE))
            .value(
                "paramOverrides",
                // This one cannot be shared, whatever the template says.
                TestValue::Struct(
                    TestInstance::new("ContractParamOverrides").value("stringParamOverrides", string_list(&[])).value(
                        "boolParamOverrides",
                        TestValue::Array(vec![TestValue::Struct(
                            TestInstance::new("ContractBoolParam")
                                .value("param", TestValue::text("CanBeShared"))
                                .value("value", TestValue::Bool(false)),
                        )]),
                    ),
                ),
            )
            .value("subContracts", TestValue::Array(Vec::new()))
            .value("additionalPrerequisites", TestValue::Array(Vec::new()))
            .value(
                "contractResults",
                TestValue::Struct(
                    TestInstance::new("ContractResults").value("contractResults", TestValue::Array(results)),
                ),
            )
            .value("minStanding", TestValue::Reference(STANDING))
            .value("maxStanding", TestValue::Reference(STANDING_MAX)),
    )
}

/// A contract that pays and hands out no blueprint: p4kx lists it, it is no
/// source and is not counted.
fn paid_contract() -> TestValue {
    let results = vec![TestValue::Struct(TestInstance::new("ContractResult_Reward").value(
        "contractReward",
        TestValue::Struct(TestInstance::new("MissionReward").value("reward", TestValue::Int32(5000))),
    ))];
    TestValue::Struct(
        TestInstance::new("CareerContract")
            .value("id", TestValue::Guid(HUNT_PAID))
            .value("debugName", TestValue::text("Hunt_Paid"))
            .value("template", TestValue::Reference(TEMPLATE))
            .value("subContracts", TestValue::Array(Vec::new()))
            .value("additionalPrerequisites", TestValue::Array(Vec::new()))
            .value(
                "contractResults",
                TestValue::Struct(
                    TestInstance::new("ContractResults").value("contractResults", TestValue::Array(results)),
                ),
            ),
    )
}

fn source_records(builder: DataForgeBuilder) -> DataForgeBuilder {
    let locality =
        TestInstance::new("ContractPrerequisite_Locality").value("localityAvailable", TestValue::Reference(LOCALITY));
    let handler = TestInstance::new("ContractGeneratorHandler_Career")
        .value("debugName", TestValue::text("Highpoint_Career"))
        .value(
            "defaultAvailability",
            TestValue::Struct(
                TestInstance::new("ContractAvailability")
                    .value("prerequisites", TestValue::Array(vec![TestValue::Struct(locality)]))
                    .value("hasPersonalCooldown", TestValue::Bool(true))
                    .value("personalCooldownTime", TestValue::Single(45.0)),
            ),
        )
        .value("contractParams", string_params(&[("Contractor", "@Highpoint_from")]))
        .value("factionReputation", TestValue::Reference(FACTION_REP))
        .value(
            "contracts",
            TestValue::Array(vec![
                contract(HUNT_EASY, "Hunt_Easy", true),
                contract(HUNT_UNRELEASED, "Hunt_Unreleased", false),
                untitled_contract(),
                paid_contract(),
            ]),
        );
    let tier = TestInstance::new("STierReward")
        .value("minPoints", TestValue::Int32(18000))
        .value("blueprintPool", TestValue::Array(vec![TestValue::Reference(POOL)]));
    let progression = TestInstance::new("STierProgressions")
        .value("progressionText", TestValue::text("@Scenario_Progress"))
        .value("tierRewards", TestValue::Array(vec![TestValue::Struct(tier)]));
    let tiers = TestInstance::new("SScenarioProgressRewardsTiers")
        .value("faction", TestValue::Reference(FACTION))
        .value("tierProgressions", TestValue::Array(vec![TestValue::Struct(progression)]));
    builder
        .record(
            record(GENERATOR, "ContractGenerator", "highpoint_hunt", "contracts/contractgenerator/highpoint_hunt")
                .value("generators", TestValue::Array(vec![TestValue::Struct(handler)])),
        )
        .record(
            record(SCENARIO, "ScenarioProgress", "xeno_scenarioprogress", "contracts/contractscenarios/xeno")
                .value("factionRewardTiers", TestValue::Array(vec![TestValue::Struct(tiers)])),
        )
}

fn database() -> Vec<u8> {
    let builder = DataForgeBuilder::new()
        .version(8)
        .enumeration("EItemType", ["WeaponPersonal"])
        .enumeration("EItemSubType", ["Rifle"]);
    source_records(mission_records(reputation_records(item_records(contract_structures(structures(builder)))))).build()
}

fn english() -> Vec<u8> {
    GlobalIniBuilder::new()
        .text("item_Namebehr_rifle", "P8-AR Rifle")
        .text("item_Descbehr_rifle", "Manufacturer: Behring\\nClass: Military\\n\\nA rifle.")
        .text("manufacturer_NameBEHR", "Behring")
        .text("Highpoint_RepUI_Name", "Highpoint Wilderness Specialists")
        .text("Highpoint_from", "Highpoint")
        .text("Rank_Two", "Rank Two")
        .text("Rank_Top", "Elite")
        .text("item_Names38_pistol", "S-38 Pistol")
        .text("Stanton1", "Hurston")
        .text("Stanton2", "Crusader")
        .text("Hunt_Title", "Hunt the ~mission(Creature) at ~mission(Location) for ~mission(Contractor|Name) ")
        .text("Hunt_Sub_Title", "Hunt ~mission(TargetName) in the ~mission(Missing)")
        .text("Creature", "Kopion")
        .text("Org_Name", "Highpoint Org")
        .text("Org_Other", "Somebody else")
        .text("Template_Title", "Template title")
        .text("MissionType_Hunting", "Hunting")
        .text("MissionType_Special", "Special hunt")
        .text("Foxwell_Name", "Foxwell Enforcement")
        .text("Scenario_Progress", "Total contribution")
        .text("LOC_PLACEHOLDER", "<= PLACEHOLDER =>")
        .build()
}

fn german() -> Vec<u8> {
    GlobalIniBuilder::new()
        .text("item_Namebehr_rifle", "P8-AR-Gewehr")
        .text("item_Descbehr_rifle", "Hersteller: Behring\\nKlasse: Militär")
        .text("Hunt_Title", "Jagd auf den Kopion")
        .build()
}

/// Writes the fixture archive into a fresh channel folder below the target
/// folder and returns the folder.
pub fn channel(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("versekit-gamedata").join(name).join("LIVE");
    if folder.exists() {
        fs::remove_dir_all(&folder).unwrap();
    }
    fs::create_dir_all(&folder).unwrap();
    // Game2.dcb and the global.ini files are not encrypted in the real
    // archive (checked on 4.10.197): reading them needs no key.
    ArchiveBuilder::new()
        .entry(TestEntry::zstd("Data\\Game2.dcb", database()))
        .entry(TestEntry::deflated("Data\\Localization\\english\\global.ini", english()))
        .entry(TestEntry::stored("Data\\Localization\\german_(germany)\\global.ini", german()))
        .write_to(folder.join("Data.p4k"))
        .unwrap();
    write_build_manifest(&folder, "sc-alpha-4.12.0", "4.12.150.1", "13000000", "00000000-0000-0000-0000-000000000000")
        .unwrap();
    folder
}

/// Like [`channel`], with an encrypted DataForge and an encrypted German
/// `global.ini`, as CIG could ship them one day (the test kit encrypts them
/// with a test key of its own).
#[allow(dead_code, reason = "only the key tests use it")]
pub fn encrypted_channel(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("versekit-gamedata").join(name).join("LIVE");
    if folder.exists() {
        fs::remove_dir_all(&folder).unwrap();
    }
    fs::create_dir_all(&folder).unwrap();
    ArchiveBuilder::new()
        .entry(TestEntry::zstd("Data\\Game2.dcb", database()).encrypted())
        .entry(TestEntry::deflated("Data\\Localization\\english\\global.ini", english()))
        .entry(TestEntry::stored("Data\\Localization\\german_(germany)\\global.ini", german()).encrypted())
        .write_to(folder.join("Data.p4k"))
        .unwrap();
    write_build_manifest(&folder, "sc-alpha-4.12.0", "4.12.150.1", "13000000", "00000000-0000-0000-0000-000000000000")
        .unwrap();
    folder
}
