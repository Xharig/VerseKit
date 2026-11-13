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

//! The writing into a made-up game folder in a throwaway folder: the file
//! that comes out, line by line, and the way back to the byte.

use std::fs;
use std::path::Path;

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;

/// A game folder and a data folder in a fresh throwaway folder.
pub(crate) struct Fixture {
    pub root: String,
    pub paths: Paths,
    pub ini: String,
}

impl Fixture {
    pub(crate) fn new(name: &str, language: &str, ini_text: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("versekit-injection-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let game = format!("{root}/StarCitizen/LIVE");
        let data = format!("{root}/data");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&game).unwrap();
        fs::write(format!("{game}/Game.log"), "").unwrap();
        let ini = format!("{game}/data/Localization/{language}/global.ini");
        fs::create_dir_all(Path::new(&ini).parent().unwrap()).unwrap();
        fs::write(&ini, ini_text).unwrap();
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &game)],
            &root,
        );
        Fixture { root, paths: Paths::new(host), ini }
    }

    pub(crate) fn text(&self) -> String {
        fs::read_to_string(&self.ini).unwrap()
    }

    pub(crate) fn data_file(&self, name: &str, text: &str) {
        fs::write(self.paths.app_file(name), text).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn missions(json: &str) -> Map {
    jsonio::loads(json).unwrap().as_object().unwrap().clone()
}

const MISSIONS: &str = r#"{
  "Bounty_title_001": {"bp": ["Atzkav Sniper Rifle", "Calico Helmet"], "sicher": true, "chance": 1, "rep": 800,
    "rang": "Jr. Contractor", "rep_max": 95250, "rang_max": "Elite Contractor", "uec": 70750, "ruf": 150,
    "teilbar": true, "rep_eindeutig": true, "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}
}"#;

const GAME: &str = "\u{feff}Frontend_PU_Version=Star Citizen 4.10\\nOther Tool v1\n\
Bounty_title_001=Kopfgeld\n\
Bounty_desc_001=Fang ihn.\n\
Bounty_desc_002,P=Fang sie.\n\
item_NameCOOL_A=Glacier\n\
item_DescCOOL_A=Hersteller: X\\nGröße: 1\\nGütegrad: A\\nKlasse: Militär\n\
no line\n";

fn inputs(owned: &[&str]) -> Inputs {
    Inputs {
        missions: missions(MISSIONS),
        owned: owned.iter().map(|n| crate::paths::name_key(n)).collect(),
        version: "5.0.0".into(),
        ..Inputs::default()
    }
}

#[test]
fn writes_marks_blocks_tags_and_the_menu_line() {
    let fx = Fixture::new("write", "german_(germany)", GAME);
    let outcome = apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&["Calico Helmet"]), false);
    assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
    let text = fx.text();
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(
        lines[0],
        "\u{feff}Frontend_PU_Version=Star Citizen 4.10\\nOther Tool v1\\nVerseKit v5.0.0 · versekit.xharig.com"
    );
    assert_eq!(lines[1], "Bounty_title_001=Kopfgeld <EM4>[BP 800 Rep]</EM4>");
    assert!(lines[2].starts_with("Bounty_desc_001=Fang ihn.\\n\\n---"), "{}", lines[2]);
    assert!(lines[2].contains("\\n   <EM4>[x]</EM4> Calico Helmet"));
    assert!(lines[2].contains("\\n   [  ] Atzkav Sniper Rifle"));
    assert!(lines[3].starts_with("Bounty_desc_002,P=Fang sie.\\n\\n---"), "same block for the other description");
    assert_eq!(lines[4], "item_NameCOOL_A=Glacier (Mil/1/A)");
    assert_eq!(lines[6], "no line");
    assert_eq!(text.matches('\n').count(), 7, "one entry per line");
}

/// The menu line of an older version (`Verse-Kit`, with the hyphen) in a file without its original texts gives way
/// to the one of today; the header of the translation above it stays, nothing stacks.
#[test]
fn the_old_menu_line_with_the_hyphen_gives_way_to_the_new_one() {
    let old = "Frontend_PU_Version=Luftwerft 4.3\\nEnglish, Source: Data.p4k\\nVerse-Kit v5.0.1 · versekit.xharig.com\n\
Bounty_title_001=Kopfgeld\n";
    let fx = Fixture::new("old-menu", "german_(germany)", old);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    let first = fx.text().lines().next().unwrap_or_default().to_string();
    assert_eq!(
        first,
        "Frontend_PU_Version=Luftwerft 4.3\\nEnglish, Source: Data.p4k\\nVerseKit v5.0.0 · versekit.xharig.com"
    );
    assert_eq!(menu_base("Kopf\\nVerseKit · versekit.xharig.com"), "Kopf", "without a version too");
    assert_eq!(menu_base("Ein Gruß an VerseKit-Freunde\\nVerseKit-Fans"), "Ein Gruß an VerseKit-Freunde\\nVerseKit-Fans");
    assert_eq!(menu_text_of(fx.text().as_bytes()).as_deref(), Some("Luftwerft 4.3\\nEnglish, Source: Data.p4k"));
}

#[test]
fn removing_restores_the_file_to_the_byte_and_nothing_stacks() {
    let fx = Fixture::new("roundtrip", "german_(germany)", GAME);
    let first = apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    let once = fx.text();
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    assert_eq!(fx.text(), once, "a second run changes nothing");
    assert!(is_applied(&fx.ini));
    assert!(matches!(first, Outcome::Done(n) if n > 0));
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), GAME);
    assert!(!is_applied(&fx.ini));
    assert!(load_origtext(&fx.paths).is_empty());
}

#[test]
fn without_contracts_nothing_is_written() {
    let fx = Fixture::new("nomissions", "english", GAME);
    let outcome = apply_texts(&fx.paths, &fx.ini, "english", &Inputs::default(), false);
    assert_eq!(outcome, Outcome::NoMissions);
    assert_eq!(fx.text(), GAME);
    assert_eq!(apply_texts(&fx.paths, "/nowhere/global.ini", "english", &inputs(&[]), false), Outcome::NoFile);
}

/// A StarStrings file as it comes: its own `[BP]` marks and `[CS1]` tags.
const STARSTRINGS: &str = "Frontend_PU_Version=Star Citizen\n\
Bounty_title_001=Bounty <EM4>[150 Rep] [BP]</EM4>\n\
Bounty_desc_001=Catch him.\n\
item_NameMSL_A=[CS1] Spark-G Missile\n\
item_DescMSL_A=Tracking Signal: Cross Section\\nSize: 1\n\
item_NameCOOL_A=Glacier\n\
item_DescCOOL_A=Size: 1\\nGrade: A\\nClass: Military\n";

#[test]
fn starstrings_keeps_its_marks_and_tags() {
    let fx = Fixture::new("starstrings", "english", STARSTRINGS);
    // As after placing the file: fresh, so no fallback cuts its `[BP]`.
    discard_origtext(&fx.paths);
    assert!(is_fresh(&fx.paths));
    assert!(!is_applied(&fx.ini), "a foreign [BP] is not ours");
    apply_texts(&fx.paths, &fx.ini, "english", &inputs(&[]), false);
    let text = fx.text();
    assert!(text.contains("Bounty_title_001=Bounty <EM4>[150 Rep] [BP]</EM4>\n"), "no second mark");
    assert!(text.contains("item_NameMSL_A=[CS1] Spark-G Missile\n"), "a tag in front stays alone");
    assert!(text.contains("item_NameCOOL_A=Glacier (Mil/1/A)\n"));
    assert!(text.contains("POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE"));
    assert!(is_applied(&fx.ini));
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), STARSTRINGS);
}

#[test]
fn a_fresh_file_keeps_marks_that_look_like_ours() {
    // A placed file of another tool with a counting mark: by form it looks
    // written by us, but a fresh file has no notes of ours at all.
    let foreign = format!("{STARSTRINGS}Other_title_001=Other <EM4>[BP 1/2]</EM4>\n");
    let fx = Fixture::new("freshform", "english", &foreign);
    discard_origtext(&fx.paths);
    assert!(is_applied(&fx.ini), "the form alone would take it for ours");
    apply_texts(&fx.paths, &fx.ini, "english", &inputs(&[]), false);
    let text = fx.text();
    assert!(text.contains("Bounty_title_001=Bounty <EM4>[150 Rep] [BP]</EM4>\n"), "{text}");
    assert!(text.contains("Other_title_001=Other <EM4>[BP 1/2]</EM4>\n"), "{text}");
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), foreign);
}

#[test]
fn without_note_a_foreign_bare_mark_is_never_cut() {
    // No note, no trace of ours: only a block may be cut, never a title.
    let fx = Fixture::new("foreignmark", "english", STARSTRINGS);
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), STARSTRINGS);
}

#[test]
fn a_block_of_the_same_source_is_replaced_and_comes_back() {
    let foreign = format!(
        "Bounty_desc_001=Fang ihn.\\n\\n{}\\nBAUPLÄNE AUS DIESEM AUFTRAG\\n    - Atzkav Sniper Rifle\n",
        "-".repeat(57)
    );
    let fx = Fixture::new("foreign-block", "german_(germany)", &foreign);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    let text = fx.text();
    assert!(!text.contains("    - Atzkav"), "replaced, not listed twice: {text}");
    assert!(text.contains("[  ] Atzkav Sniper Rifle"));
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), foreign);
}

/// The block of an earlier version, with the footer that spelled the name with a hyphen, is replaced by the block with
/// the footer of today, once: nothing of the old footer stays and nothing stands twice.
#[test]
fn a_block_with_the_old_footer_is_replaced_and_the_footer_is_written_without_the_hyphen() {
    for (language, old, new, heading) in [
        (
            "german_(germany)",
            "Angaben von scmdb.net · eingefügt von Verse-Kit",
            "Angaben von scmdb.net · eingefügt von VerseKit",
            "MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP",
        ),
        (
            "english",
            "Data from scmdb.net · added by Verse-Kit",
            "Data from scmdb.net · added by VerseKit",
            "POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE",
        ),
    ] {
        let earlier = format!(
            "Bounty_desc_001=Fang ihn.\\n\\n{}\\n{heading}\\n# Chance\\n   [  ] Atzkav Sniper Rifle\\n\\n{old}\n",
            "-".repeat(57)
        );
        let fx = Fixture::new("old-footer", language, &earlier);
        apply_texts(&fx.paths, &fx.ini, language, &inputs(&[]), false);
        let text = fx.text();
        assert!(!text.contains("Verse-Kit"), "{language}: nothing of the old footer stays: {text}");
        assert_eq!(text.matches(new).count(), 1, "{language}: the new footer stands once: {text}");
        assert_eq!(text.matches(heading).count(), 1, "{language}: and so does the block: {text}");
        // Written again, the file does not change.
        let once = text;
        apply_texts(&fx.paths, &fx.ini, language, &inputs(&[]), false);
        assert_eq!(fx.text(), once, "{language}");
    }
}

#[test]
fn our_block_sits_before_a_foreign_appendix_and_it_survives() {
    let game = "Bounty_desc_001=Catch him.\\n\\n--- STATS ---\\nDanger: 3\n";
    let fx = Fixture::new("appendix", "english", game);
    apply_texts(&fx.paths, &fx.ini, "english", &inputs(&[]), false);
    let text = fx.text();
    let ours = text.find("POSSIBLE BLUEPRINTS").unwrap();
    let theirs = text.find("--- STATS ---").unwrap();
    assert!(ours < theirs, "{text}");
    assert!(text.ends_with("\\n\\n--- STATS ---\\nDanger: 3\n"));
    // Without the note the fallback cuts only our block.
    fs::remove_file(fx.paths.app_file(ORIGTEXT_FILE)).unwrap();
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), game);
}

#[test]
fn own_ship_names_and_missing_ships_are_written_and_taken_back() {
    let game = "vehicle_NameXIAN_Railen=Railen\nvehicle_NameAEGS_Gladius=Gladius\n";
    let fx = Fixture::new("ships", "german_(germany)", game);
    let english = fx.ini.replace("german_(germany)", "english");
    fs::create_dir_all(Path::new(&english).parent().unwrap()).unwrap();
    fs::write(&english, "vehicle_NameXIAN_Railen=Railen\nvehicle_NameAEGS_Sabre_Raven_EX=Sabre Raven EX\n").unwrap();
    fx.data_file(
        asop::FILE,
        r#"{"format": 1, "namen": {"vehicle_NameXIAN_Railen": {"name": "Packesel", "stern": true}}}"#,
    );
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    let text = fx.text();
    assert!(text.contains("vehicle_NameXIAN_Railen=*Packesel\n"), "{text}");
    assert!(text.ends_with("vehicle_NameAEGS_Sabre_Raven_EX=Sabre Raven EX\n"), "{text}");
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    assert_eq!(fx.text(), text, "no star before the star, no second added line");
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), game);
}

#[test]
fn a_line_break_in_the_data_makes_no_new_line() {
    let fx = Fixture::new("breaks", "english", GAME);
    let mut inputs = inputs(&[]);
    let entry = inputs.missions[0].1.as_object().unwrap().clone();
    let mut entry = entry;
    map_set(&mut entry, "bp", Value::Array(vec![Value::str("Helm\nFrontend_Evil=fake"), Value::str("Vest\u{2028}x")]));
    inputs.missions[0].1 = Value::Object(entry);
    apply_texts(&fx.paths, &fx.ini, "english", &inputs, false);
    let text = fx.text();
    assert!(!text.lines().any(|l| l.starts_with("Frontend_Evil")), "{text}");
    assert_eq!(text.matches('\n').count(), GAME.matches('\n').count());
}

#[test]
fn a_crlf_file_comes_back_with_line_feeds_like_4_12() {
    let crlf = GAME.replace('\n', "\r\n");
    let fx = Fixture::new("crlf", "german_(germany)", &crlf);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), GAME);
}

#[test]
fn the_leftover_file_is_reset_before_a_change_of_source() {
    let fx = Fixture::new("leftover", "german_(germany)", GAME);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    let english = fx.ini.replace("german_(germany)", "english");
    fs::create_dir_all(Path::new(&english).parent().unwrap()).unwrap();
    fs::write(&english, GAME).unwrap();
    assert_eq!(leftover_file(&fx.paths, Some(&fx.ini)), None, "same file is no leftover");
    let (old, count) = clean_leftover(&fx.paths, Some(&english)).unwrap();
    assert_eq!(old, fx.ini);
    assert!(count > 0);
    assert_eq!(fx.text(), GAME);
}

/// The note holds the German wording of a German file that is still there:
/// written into the English file it would bring that wording along. 4.12
/// takes it; v5 leaves it aside on purpose.
#[test]
fn the_original_texts_of_another_file_are_not_used() {
    let fx = Fixture::new("othernote", "german_(germany)", GAME);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &inputs(&[]), false);
    assert!(origtext_note_of(&fx.paths, &fx.ini).is_some(), "the note belongs to the German file");
    let english = fx.ini.replace("german_(germany)", "english");
    fs::create_dir_all(Path::new(&english).parent().unwrap()).unwrap();
    fs::write(&english, "Bounty_title_001=Bounty\nBounty_desc_001=Catch him.\n").unwrap();
    assert!(origtext_held_for_other(&fx.paths, &english));
    apply_texts(&fx.paths, &english, "english", &inputs(&[]), false);
    let text = fs::read_to_string(&english).unwrap();
    assert!(text.starts_with("Bounty_title_001=Bounty <EM4>[BP 800 Rep]</EM4>\n"), "{text}");
    assert!(text.contains("Bounty_desc_001=Catch him.\\n\\n"), "{text}");
    assert!(!text.contains("Kopfgeld") && !text.contains("Fang ihn"), "{text}");
}

/// The arms and the helmet of the ADP-mk4 set show the description
/// `item_Desc_cds_heavy_armor_01_Shared`, which looks like neither of their
/// name keys; the catalog of the game data names it.
#[test]
fn the_arms_of_a_set_get_their_note_through_the_catalog_of_the_game_data() {
    let game = "cfp_title=Destroy Stolen Data\n\
cfp_desc=Destroy it.\n\
item_Desc_cds_heavy_armor_01_Shared=Item Type: Heavy Armor\n\
item_Desc_cds_heavy_armor_01_core=Item Type: Heavy Armor\n\
item_Name_cds_heavy_armor_01_arms=ADP-mk4 Arms Woodland\n\
item_Name_cds_heavy_armor_01_core=ADP-mk4 Core Woodland\n\
item_Name_cds_heavy_armor_helmet_01=ADP-mk4 Helmet Woodland\n";
    let fx = Fixture::new("sharedtext", "english", game);
    let names = ["ADP-mk4 Arms Woodland", "ADP-mk4 Core Woodland", "ADP-mk4 Helmet Woodland"];
    let catalog = format!(
        r#"{{"bauplaene": {{"a": {{"n": "{}"}}, "c": {{"n": "{}"}}, "h": {{"n": "{}"}}}},
        "missionen": {{"cfp_title": {{"bp": ["{}", "{}", "{}"], "rep": 0, "titel_key": "cfp_title", "text_key": "cfp_desc"}}}},
        "{}": {{
          "bef9cd69": {{"name": "item_Name_cds_heavy_armor_01_arms", "text": "item_Desc_cds_heavy_armor_01_Shared"}},
          "8eabeef4": {{"name": "item_Name_cds_heavy_armor_01_core", "text": "item_Desc_cds_heavy_armor_01_core"}},
          "80fd0b3b": {{"name": "item_Name_cds_heavy_armor_helmet_01", "text": "item_Desc_cds_heavy_armor_01_Shared"}}}}}}"#,
        names[0],
        names[1],
        names[2],
        names[0],
        names[1],
        names[2],
        crate::catalog_cache::ITEM_TEXTS
    );
    fx.data_file(sources::CATALOG_V5, &catalog);
    let recipe = |tag: &str, name: &str, entity: &str| {
        format!(
            r#"{{"tag": "{tag}", "productName": "{name}", "productEntityClass": "{entity}",
            "tiers": [{{"craftTimeSeconds": 240, "slots": [{{"name": "Plate", "options": [{{"resourceName": "Ouratite", "quantity": 1}}]}}]}}]}}"#
        )
    };
    fx.data_file(
        sources::RECIPES_FILE,
        &format!(
            r#"{{"format": {}, "build": "", "blueprints": [{}, {}, {}]}}"#,
            sources::RECIPES_FORMAT,
            recipe("BP_CRAFT_cds_armor_heavy_arms_01_01_01", names[0], "bef9cd69"),
            recipe("BP_CRAFT_cds_armor_heavy_core_01_01_01", names[1], "8eabeef4"),
            recipe("BP_CRAFT_cds_armor_heavy_helmet_01_01_01", names[2], "80fd0b3b"),
        ),
    );
    fx.data_file(
        sources::COLLECTION_FILE,
        r#"{"bauplaene": {"adp-mk4 arms woodland": {}, "adp-mk4 core woodland": {}}}"#,
    );
    let catalog_file = fx.paths.app_file(sources::CATALOG_V5);
    let inputs = sources::inputs(&fx.paths, &catalog_file, "5.0.0");
    assert_eq!(inputs.item_texts.len(), 3, "the keys come from the catalog file");
    assert!(matches!(apply_texts(&fx.paths, &fx.ini, "english", &inputs, false), Outcome::Done(_)));
    let text = fx.text();
    let shared = text.lines().find(|l| l.starts_with("item_Desc_cds_heavy_armor_01_Shared=")).unwrap();
    assert!(shared.contains("\\nBlueprint: <EM4>[x]</EM4> ADP-mk4 Arms Woodland (owned)"), "{shared}");
    assert!(shared.contains("\\nBlueprint: [  ] ADP-mk4 Helmet Woodland (missing) — from \"Destroy Stolen Data\""));
    assert!(shared.ends_with("\\nCrafting: 1 ingredient · 4 min"), "{shared}");
    let core = text.lines().find(|l| l.starts_with("item_Desc_cds_heavy_armor_01_core=")).unwrap();
    assert!(core.contains("\\nBlueprint: <EM4>[x]</EM4> owned"), "{core}");
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), game);
}

/// Two radars the file has no text for, one item whose texts it has, and one
/// whose name it has but not the description.
const GAPS_GAME: &str = "cfp_title=Destroy Stolen Data\n\
cfp_desc=Destroy it.\n\
item_NameCOOL_A=Glacier\n\
item_DescCOOL_A=Hersteller: X\\nGröße: 1\\nGütegrad: A\\nKlasse: Militär\n\
item_Name_RADR_ANVL_S01_Present=Eigener Radar\n";

fn gaps_inputs() -> Inputs {
    let item = |base: &str, entity: &str| item_info::Item {
        base: base.into(),
        entity: entity.into(),
        have: Some(false),
        ..item_info::Item::default()
    };
    let keys = |name: &str, text: &str| item_info::ItemKeys { name: name.into(), text: text.into() };
    Inputs {
        items: vec![item("Glacier", "g1"), item("Lephari", "g2"), item("Idris", "g3"), item("Present", "g4")],
        item_texts: [
            ("g1".to_string(), keys("item_NameCOOL_A", "item_DescCOOL_A")),
            ("g2".to_string(), keys("item_Name_RADR_WLOP_S03_Lephari", "item_Desc_RADR_WLOP_S03_Lephari")),
            ("g3".to_string(), keys("item_Name_RADR_GNRP_S03_Idris", "item_Desc_RADR_GNRP_S03_Idris")),
            ("g4".to_string(), keys("item_Name_RADR_ANVL_S01_Present", "item_Desc_RADR_ANVL_S01_Present")),
        ]
        .into_iter()
        .collect(),
        ..inputs(&[])
    }
}

#[test]
fn a_crafted_item_without_text_gets_a_stand_in_name_and_the_block_alone() {
    let fx = Fixture::new("standin", "english", GAPS_GAME);
    let inputs = gaps_inputs();
    assert!(matches!(apply_texts(&fx.paths, &fx.ini, "english", &inputs, false), Outcome::Done(_)));
    let text = fx.text();
    assert!(text.contains("\nitem_Name_RADR_WLOP_S03_Lephari=Lephari\n"), "{text}");
    assert!(text.contains("\nitem_Name_RADR_GNRP_S03_Idris=Idris\n"), "{text}");
    let desc = text.lines().find(|l| l.starts_with("item_Desc_RADR_WLOP_S03_Lephari=")).expect("description line");
    assert!(desc.starts_with("item_Desc_RADR_WLOP_S03_Lephari=---"), "no text of ours before the block: {desc}");
    assert!(desc.contains("\\nBlueprint: [  ] missing"), "{desc}");
    // What the file has stays: the name without a tag of its own, the item with its texts.
    assert!(text.contains("\nitem_Name_RADR_ANVL_S01_Present=Eigener Radar\n"), "{text}");
    assert!(text.contains("\nitem_NameCOOL_A=Glacier (Mil/1/A)\n"), "{text}");
    assert_eq!(text.matches("item_Name_RADR_ANVL_S01_Present=").count(), 1);
    assert_eq!(text.matches("item_Name_RADR_WLOP_S03_Lephari=").count(), 1);
    // The description the file lacks next to a name it has: the block alone.
    let present = text.lines().find(|l| l.starts_with("item_Desc_RADR_ANVL_S01_Present=")).expect("block line");
    assert!(present.starts_with("item_Desc_RADR_ANVL_S01_Present=---"), "{present}");
    // A second run adds nothing, removing restores the file to the byte.
    apply_texts(&fx.paths, &fx.ini, "english", &inputs, false);
    assert_eq!(fx.text(), text, "no second stand-in line");
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), GAPS_GAME);
}

#[test]
fn a_text_the_game_supplies_later_is_never_overwritten() {
    let fx = Fixture::new("standin-later", "english", GAPS_GAME);
    let inputs = gaps_inputs();
    apply_texts(&fx.paths, &fx.ini, "english", &inputs, false);
    // A patch rewrites the file and now has the text for one radar.
    let patched = format!("{GAPS_GAME}item_Name_RADR_WLOP_S03_Lephari=Echter Name\n");
    fs::write(&fx.ini, &patched).unwrap();
    apply_texts(&fx.paths, &fx.ini, "english", &inputs, false);
    let text = fx.text();
    assert!(text.contains("\nitem_Name_RADR_WLOP_S03_Lephari=Echter Name\n"), "{text}");
    assert!(!text.contains("=Lephari\n"), "{text}");
    assert!(text.contains("\nitem_Name_RADR_GNRP_S03_Idris=Idris\n"), "{text}");
}

#[test]
fn no_description_line_without_the_detail_setting() {
    let fx = Fixture::new("standin-off", "english", GAPS_GAME);
    fx.paths.set_setting(item_info::SETTING, Value::Bool(false));
    apply_texts(&fx.paths, &fx.ini, "english", &gaps_inputs(), false);
    let text = fx.text();
    assert!(text.contains("\nitem_Name_RADR_WLOP_S03_Lephari=Lephari\n"), "{text}");
    assert!(!text.contains("item_Desc_RADR_WLOP_S03_Lephari"), "{text}");
}

#[test]
fn stand_in_names_come_from_the_class_name() {
    use item_info::fallback_name;
    assert_eq!(fallback_name("item_Name_RADR_WLOP_S03_Lephari").as_deref(), Some("Lephari"));
    assert_eq!(fallback_name("item_NameRADR_RSI_S04_Polaris_SCItem").as_deref(), Some("Polaris"));
    assert_eq!(fallback_name("item_NameAMRS_LaserCannon_S1").as_deref(), Some("LaserCannon S1"));
    assert_eq!(fallback_name("item_Name_").as_deref(), None);
}

/// The terms of a contract without blueprints and of one with them, as the game data give them.
fn with_terms() -> Inputs {
    let terms = terms::summarize(&[
        (
            "@TestGang_Raid_E_title_001".to_string(),
            terms::Terms {
                cooldown: Some(45),
                cooldown_spread: Some(10),
                abandoned: Some(45),
                abandoned_spread: None,
                buy_in: Some(13500),
                shareable: Some(false),
                again_abandoned: Some(true),
                again_failed: Some(false),
                description: Some("@TestGang_Raid_E_desc_001".into()),
                lowest: Some(("Neutral".into(), 0)),
                highest: Some(("Top Rank".into(), 95250)),
                reward: None,
            },
        ),
        (
            "Bounty_title_001".to_string(),
            terms::Terms { cooldown: Some(30), buy_in: Some(500), description: Some("Bounty_desc_001".into()), ..Default::default() },
        ),
    ]);
    let contracts = missions(r#"{"testgang_raid_e_title_001": [{"wer": "Test Gang", "was": "Standing", "wieviel": 50}]}"#);
    let mut inputs = inputs(&[]);
    inputs.reputation = reputation::Table {
        contracts,
        regions: vec![("testgang_raid_e_title_001".into(), Value::str("Pyro"))],
        terms,
        terms_source: reputation::TERMS_FROM_GAME.into(),
        ..reputation::Table::default()
    };
    inputs
}

const HEIST: &str = "TestGang_Raid_E_title_001=Quick Grab\n\
TestGang_Raid_E_desc_001=Take the ship.\n\
Other_desc_001=Nothing known.\n";

#[test]
fn every_contract_with_known_terms_gets_a_block_and_it_goes_again() {
    let game = format!("{GAME}{HEIST}");
    let fx = Fixture::new("terms", "german_(germany)", &game);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_terms(), false);
    let text = fx.text();
    let line = |key: &str| text.lines().find(|l| l.starts_with(key)).unwrap_or_default().to_string();
    assert_eq!(line("TestGang_Raid_E_title_001="), "TestGang_Raid_E_title_001=Quick Grab", "no title mark");
    assert_eq!(
        line("TestGang_Raid_E_desc_001="),
        concat!(
            "TestGang_Raid_E_desc_001=Take the ship.\\n\\n",
            "---------------------------------------------------------\\n\\n<EM4>AUFTRAGSANGABEN</EM4>\\n\\n",
            "<EM4># Max. Reputation: Top Rank (95.250 XP)</EM4>\\n# Wartezeit: 45 Minuten (±10)\\n# Wartezeit nach Abbruch: 45 Minuten\\n# Buy-in: 13.500 aUEC\\n",
            "# Mission teilbar: Nein\\n# Wieder annehmbar: nach Abbruch Ja, nach Fehlschlag Nein\\n",
            "<EM4># Ruf: Test Gang +50 Standing</EM4>\\n# Region: Pyro\\n\\n",
            "Angaben aus den Spieldaten · eingefügt von VerseKit"
        )
    );
    assert_eq!(line("Other_desc_001="), "Other_desc_001=Nothing known.");
    // The blueprint block names what it lacked, without the cooldown twice.
    let bounty = line("Bounty_desc_001=");
    assert!(bounty.contains("\\n# Buy-in: 500 aUEC\\n"), "{bounty}");
    assert!(!bounty.contains("# Wartezeit: 30"), "{bounty}");
    assert!(is_applied(&fx.ini));

    // Again: nothing stacks; removed: the file as it was.
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_terms(), false);
    assert_eq!(fx.text(), text);
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), game);
}

/// Without the stored original texts, the form alone finds the contract block: it has no boxes, and it is ours.
#[test]
fn the_contract_block_is_found_by_its_form() {
    let game = format!("{GAME}{HEIST}");
    let fx = Fixture::new("terms-form", "english", &game);
    apply_texts(&fx.paths, &fx.ini, "english", &with_terms(), false);
    let written = fx.text();
    let desc = written.lines().find(|l| l.starts_with("TestGang_Raid_E_desc_001=")).unwrap().to_string();
    let (_, _, value) = form::split_line(&desc).unwrap();
    assert_eq!(form::strip_form(value, Fallback::Block), "Take the ship.");
    assert_eq!(form::split_foreign_block(value).0, value, "not taken for a block of the player");
}

/// Inputs whose terms hold the titles given, each with its cooldown and description, and reputation rows.
fn shared(titles: &[(&str, i64, &str)], rows: &str) -> Inputs {
    let list: Vec<(String, terms::Terms)> = titles
        .iter()
        .map(|(title, cooldown, description)| {
            let terms = terms::Terms { cooldown: Some(*cooldown), description: Some((*description).into()), ..Default::default() };
            ((*title).to_string(), terms)
        })
        .collect();
    let mut inputs = inputs(&[]);
    inputs.reputation = reputation::Table {
        contracts: missions(rows),
        terms: terms::summarize(&list),
        terms_source: reputation::TERMS_FROM_GAME.into(),
        ..reputation::Table::default()
    };
    inputs
}

/// A description several titles share gets what they all say, and nothing where they differ.
#[test]
fn a_description_of_several_titles_names_only_what_they_share() {
    let game = format!("{GAME}Shared_desc_001=Go there.\n");
    let differ = shared(&[("Alpha_title", 45, "Shared_desc_001"), ("Beta_title", 30, "Shared_desc_001")], "{}");
    let fx = Fixture::new("shared-differ", "english", &game);
    apply_texts(&fx.paths, &fx.ini, "english", &differ, false);
    assert!(fx.text().contains("\nShared_desc_001=Go there.\n"), "no block where the titles differ");

    let rows = r#"{"alpha_title": [{"wer": "Org", "was": "Standing", "wieviel": 10}], "beta_title": [{"wer": "Org", "was": "Standing", "wieviel": 10}]}"#;
    let agree = shared(&[("Alpha_title", 45, "Shared_desc_001"), ("Beta_title", 45, "Shared_desc_001")], rows);
    let fx = Fixture::new("shared-agree", "english", &game);
    apply_texts(&fx.paths, &fx.ini, "english", &agree, false);
    let text = fx.text();
    let line = text.lines().find(|l| l.starts_with("Shared_desc_001=")).unwrap();
    assert!(line.contains("# Cooldown: 45 minutes") && line.contains("# Reputation: Org +10 Standing"), "{line}");
}

/// A blueprint contract whose title the contractor fills in (the catalog names a title the terms do not know) gets
/// the terms and reputation of the titles its description belongs to.
#[test]
fn a_blueprint_contract_with_a_placeholder_title_gets_its_terms_through_its_description() {
    let game = "Hunt_title_001=~mission(Contractor|HuntTitle)\nHunt_desc_001=Hunt it.\n";
    let mut inputs = shared(
        &[("Org_Hunt_Title_001", 20, "Hunt_desc_001")],
        r#"{"org_hunt_title_001": [{"wer": "Org", "was": "Standing", "wieviel": 10}]}"#,
    );
    inputs.missions = missions(
        r#"{"Hunt_title_001": {"bp": ["Atzkav Sniper Rifle"], "teilbar": true, "titel_key": "Hunt_title_001", "text_key": "Hunt_desc_001"}}"#,
    );
    let fx = Fixture::new("placeholder", "english", game);
    apply_texts(&fx.paths, &fx.ini, "english", &inputs, false);
    let text = fx.text();
    let line = text.lines().find(|l| l.starts_with("Hunt_desc_001=")).unwrap();
    assert!(line.contains("POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE"), "{line}");
    assert!(line.contains("<EM4># Reputation: Org +10 Standing</EM4>"), "{line}");
}

#[test]
fn stock_mark_is_count_and_hash() {
    let owned: BTreeSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
    // The first twelve hex digits of sha1("a\nb").
    assert_eq!(stock_mark(&owned), "2-fcd127ffa101");
}

/// The cooler `item_DescCOOL_A` of [`GAME`] and [`GAPS_GAME`] with its stock values.
fn with_stats(inputs: Inputs) -> Inputs {
    let cooler = item_stats::ItemStats {
        name: "item_NameCOOL_A".into(),
        text: "item_DescCOOL_A".into(),
        kind: item_stats::kind::COOLER.into(),
        values: [(item_stats::value::COOLING.to_string(), 38.0), (item_stats::value::EM.to_string(), 1490.0)]
            .into_iter()
            .collect(),
    };
    Inputs { item_stats: vec![cooler], ..inputs }
}

fn line_of<'t>(text: &'t str, key: &str) -> &'t str {
    text.lines().find(|l| l.starts_with(&format!("{key}="))).expect("the line")
}

#[test]
fn the_stock_values_stand_under_the_description_and_go_again() {
    let fx = Fixture::new("stats", "german_(germany)", GAME);
    assert!(matches!(
        apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_stats(inputs(&[])), false),
        Outcome::Done(_)
    ));
    let text = fx.text();
    assert!(
        line_of(&text, "item_DescCOOL_A").ends_with(&format!(
            "Klasse: Militär\\n\\n{}\\n\\n<EM4>VERSEKIT: WERTE AB WERK</EM4>\\nKühlleistung 38\\nEM 1.490",
            texts::rule()
        )),
        "{text}"
    );
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_stats(inputs(&[])), false);
    assert_eq!(fx.text(), text, "a second run changes nothing");
    remove_texts(&fx.paths, &fx.ini, "german_(germany)");
    assert_eq!(fx.text(), GAME);
}

#[test]
fn the_stock_values_follow_the_language_of_the_file() {
    let fx = Fixture::new("stats-en", "english", GAME);
    apply_texts(&fx.paths, &fx.ini, "english", &with_stats(inputs(&[])), false);
    let text = fx.text();
    assert!(line_of(&text, "item_DescCOOL_A").ends_with("<EM4>VERSEKIT: STOCK STATS</EM4>\\nCooling 38\\nEM 1,490"));
}

#[test]
fn without_the_setting_no_stock_values_and_switching_off_takes_them_out() {
    let fx = Fixture::new("stats-off", "german_(germany)", GAME);
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_stats(inputs(&[])), false);
    assert!(fx.text().contains("WERTE AB WERK"));
    fx.paths.set_setting(item_stats::SETTING, Value::Bool(false));
    apply_texts(&fx.paths, &fx.ini, "german_(germany)", &with_stats(inputs(&[])), false);
    let text = fx.text();
    assert!(!text.contains("WERTE AB WERK"), "{text}");
    assert_eq!(line_of(&text, "item_DescCOOL_A"), line_of(GAME, "item_DescCOOL_A"));
}

#[test]
fn a_crafted_item_shows_its_values_and_its_blueprint_below_one_rule() {
    let fx = Fixture::new("stats-craft", "english", GAPS_GAME);
    apply_texts(&fx.paths, &fx.ini, "english", &with_stats(gaps_inputs()), false);
    let text = fx.text();
    let line = line_of(&text, "item_DescCOOL_A");
    assert_eq!(line.matches(&texts::rule()).count(), 1, "{line}");
    let values = line.find("VERSEKIT: STOCK STATS").expect("the values");
    let blueprint = line.find("VERSEKIT: BLUEPRINT & CRAFTING").expect("the blueprint");
    assert!(values < blueprint, "{line}");
    assert!(
        line.contains("EM 1,490\\n\\n<EM4>VERSEKIT: BLUEPRINT & CRAFTING</EM4>\\nBlueprint: [  ] missing"),
        "{line}"
    );
    remove_texts(&fx.paths, &fx.ini, "english");
    assert_eq!(fx.text(), GAPS_GAME);
}

#[test]
fn a_block_of_stock_values_is_found_by_its_form_when_the_note_is_lost() {
    let base = "Hersteller: X\\nGröße: 1";
    let stats = item_stats::block(&["Kühlleistung 38".to_string()], &item_stats::GERMAN);
    assert_eq!(form::strip_form(&format!("{base}{stats}"), Fallback::Block), base);
    let blueprint =
        format!("\\n\\n{}\\n\\n<EM4>VERSEKIT: BAUPLAN & HERSTELLUNG</EM4>\\nBauplan: [  ] fehlt", texts::rule());
    assert_eq!(form::strip_form(&format!("{base}{}", item_stats::joined(&stats, &blueprint)), Fallback::Block), base);
    assert_eq!(form::split_foreign_block(&format!("{base}{stats}")).1, "", "ours, not a foreign block");
}

/// Blocks written while the name still had its hyphen (`VERSE-KIT: …`) are ours all the same: taking the texts back
/// finds and cuts them, and writing again replaces them by the block of today.
#[test]
fn a_block_with_the_old_name_in_its_heading_is_found_and_taken_out() {
    for (language, heading, line) in [
        ("german_(germany)", "VERSE-KIT: BAUPLAN & HERSTELLUNG", "Bauplan: [  ] fehlt"),
        ("english", "VERSE-KIT: BLUEPRINT & CRAFTING", "Blueprint: [  ] missing"),
    ] {
        let base = "Hersteller: X\\nGröße: 1";
        let old = format!("\\n\\n{}\\n\\n<EM4>{heading}</EM4>\\n{line}", texts::rule());
        assert_eq!(form::strip_form(&format!("{base}{old}"), Fallback::Block), base, "{heading}");
        let game = format!("item_DescCOOL_A={base}{old}\n");
        let fx = Fixture::new("old-heading", language, &game);
        remove_texts(&fx.paths, &fx.ini, language);
        assert_eq!(fx.text(), format!("item_DescCOOL_A={base}\n"), "{language}: the old block is cut");
        let fx = Fixture::new("old-heading-again", language, &game);
        apply_texts(&fx.paths, &fx.ini, language, &inputs(&[]), false);
        assert!(!fx.text().contains("VERSE-KIT"), "{language}: nothing of the old heading stays: {}", fx.text());
    }
}
