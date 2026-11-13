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

//! The lines of every kind of item in both languages, and which descriptions
//! get a block.

use super::*;
use crate::jsonio;

fn item(kind: &str, values: &[(&str, f64)]) -> ItemStats {
    ItemStats {
        name: "item_NameX".into(),
        text: "item_DescX".into(),
        kind: kind.into(),
        values: values.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    }
}

fn ship_weapon() -> ItemStats {
    item(
        kind::SHIP_WEAPON,
        &[
            (value::DPS, 1266.4),
            (value::ALPHA, 63.3),
            (value::FIRE_RATE, 1200.0),
            (value::RANGE, 2799.4),
            (value::SPEED, 1296.0),
            (value::SPREAD, 0.5),
            (value::POWER, 0.1),
            (value::EM, 33.0),
            (value::AMMO, 3660.0),
            (value::HEALTH, 1650.0),
        ],
    )
}

#[test]
fn numbers_take_the_signs_of_the_language() {
    assert_eq!(number(1266.4, 0, &GERMAN), "1.266");
    assert_eq!(number(1266.4, 0, &ENGLISH), "1,266");
    assert_eq!(number(0.45, 2, &GERMAN), "0,45");
    assert_eq!(number(6.0, 2, &GERMAN), "6");
    assert_eq!(number(-40.0, 0, &GERMAN), "-40");
    assert_eq!(number(-0.001, 0, &GERMAN), "0");
    assert_eq!(auto(410.184, &ENGLISH), "410");
    assert_eq!(auto(63.3, &ENGLISH), "63.3");
    assert_eq!(auto(2.03, &GERMAN), "2,03");
    assert_eq!(auto(1_184_000.0, &GERMAN), "1.184.000");
}

#[test]
fn a_ship_weapon_in_both_languages() {
    assert_eq!(
        lines(&ship_weapon(), &GERMAN),
        [
            "DPS 1.266 · Alpha 63,3 · 1.200 Schuss/min",
            "Reichweite 2.799 m · Tempo 1.296 m/s",
            "Streuung 0,5° · Strom 0,1 · EM 33",
            "Munition 3.660 · Haltbarkeit 1.650 HP",
        ]
    );
    assert_eq!(
        lines(&ship_weapon(), &ENGLISH),
        [
            "DPS 1,266 · Alpha 63.3 · 1,200 rpm",
            "Range 2,799 m · Speed 1,296 m/s",
            "Spread 0.5° · Power 0.1 · EM 33",
            "Ammo 3,660 · Durability 1,650 HP",
        ]
    );
}

#[test]
fn an_energy_weapon_without_ammunition_names_none() {
    let mut weapon = ship_weapon();
    weapon.values.insert(value::AMMO.into(), 0.0);
    assert_eq!(lines(&weapon, &GERMAN)[3], "Haltbarkeit 1.650 HP");
}

#[test]
fn a_personal_weapon() {
    let rifle = item(
        kind::PERSONAL_WEAPON,
        &[
            (value::DPS, 312.5),
            (value::ALPHA, 25.0),
            (value::FIRE_RATE, 750.0),
            (value::RANGE, 900.0),
            (value::SPEED, 600.0),
            (value::AMMO, 30.0),
            (value::SPREAD, 1.25),
            (value::HEALTH, 100.0),
        ],
    );
    assert_eq!(
        lines(&rifle, &GERMAN),
        ["DPS 313 · Alpha 25 · 750 Schuss/min", "Reichweite 900 m · Tempo 600 m/s", "Magazin 30 · Streuung 1,25°"]
    );
}

#[test]
fn a_shield() {
    let shield = item(
        kind::SHIELD,
        &[
            (value::SHIELD_HP, 4488.0),
            (value::REGEN, 852.0),
            (value::DAMAGED_DELAY, 6.0),
            (value::DOWNED_DELAY, 10.5),
            (value::RESIST_PHYSICAL, 0.25),
            (value::RESIST_ENERGY, 0.0),
            (value::RESIST_DISTORTION, 0.95),
            (value::POWER, 2.0),
            (value::EM, 1200.0),
            (value::HEALTH, 900.0),
        ],
    );
    assert_eq!(
        lines(&shield, &GERMAN),
        [
            "Schild 4.488 HP · Regeneration 852 HP/s",
            "Neustart nach Treffer 6 s · nach Ausfall 10,5 s",
            "Widerstand: Phys 25 % · Energie 0 % · Distortion 95 %",
            "Strom 2 · EM 1.200 · Haltbarkeit 900 HP",
        ]
    );
    assert_eq!(lines(&shield, &ENGLISH)[1], "Restart after hit 6 s · after down 10.5 s");
}

#[test]
fn a_shield_with_absorption_leaves_power_and_integrity_out() {
    let mut shield = item(
        kind::SHIELD,
        &[
            (value::SHIELD_HP, 3168.0),
            (value::REGEN, 602.0),
            (value::DAMAGED_DELAY, 5.75),
            (value::DOWNED_DELAY, 11.5),
            (value::RESIST_PHYSICAL, 0.25),
            (value::RESIST_ENERGY, 0.0),
            (value::RESIST_DISTORTION, 0.95),
            (value::ABSORB_PHYSICAL, 0.45),
            (value::ABSORB_ENERGY, 1.0),
            (value::ABSORB_DISTORTION, 1.0),
            (value::EM, 1200.0),
        ],
    );
    assert_eq!(
        lines(&shield, &GERMAN),
        [
            "Schild 3.168 HP · Regeneration 602 HP/s",
            "Neustart nach Treffer 5,75 s · nach Ausfall 11,5 s",
            "Widerstand: Phys 25 % · Energie 0 % · Distortion 95 %",
            "Absorption: Phys 45 % · Energie 100 % · Distortion 100 %",
        ]
    );
    shield.values.retain(|name, _| !name.starts_with("absorption"));
    assert_eq!(lines(&shield, &GERMAN)[3], "EM 1.200");
}

#[test]
fn a_cooler_and_a_power_plant() {
    let cooler = item(
        kind::COOLER,
        &[
            (value::COOLING, 38.0),
            (value::POWER, 3.0),
            (value::EM, 1490.0),
            (value::IR, 7920.0),
            (value::HEALTH, 230.0),
        ],
    );
    assert_eq!(lines(&cooler, &GERMAN), ["Kühlleistung 38", "Strom 3 · EM 1.490 · IR 7.920", "Haltbarkeit 230 HP"]);
    let plant = item(
        kind::POWER_PLANT,
        &[(value::OUTPUT, 25.0), (value::EM, 3400.0), (value::HEALTH, 600.0), (value::DISTORTION_HP, 2200.0)],
    );
    assert_eq!(lines(&plant, &ENGLISH), ["Output 25 pips", "EM 3,400", "Durability 600 HP · Distortion 2,200"]);
    let mut resisting = plant;
    for (name, share) in [(value::RESIST_PHYSICAL, 0.1), (value::RESIST_ENERGY, 0.1), (value::RESIST_DISTORTION, 0.0)] {
        resisting.values.insert(name.into(), share);
    }
    assert_eq!(lines(&resisting, &GERMAN)[3], "Widerstand: Phys 10 % · Energie 10 % · Distortion 0 %");
}

#[test]
fn a_quantum_drive() {
    let drive = item(
        kind::QUANTUM_DRIVE,
        &[
            (value::DRIVE_SPEED, 283_046_000.0),
            (value::FUEL, 0.0175),
            (value::SPOOL, 4.5),
            (value::COOLDOWN, 12.0),
            (value::POWER, 2.0),
            (value::HEALTH, 400.0),
        ],
    );
    assert_eq!(
        lines(&drive, &GERMAN),
        ["Tempo 283.046 km/s", "Treibstoff 0,0175 · Aufladen 4,5 s · Abkühlen 12 s", "Strom 2", "Haltbarkeit 400 HP"]
    );
}

#[test]
fn a_missile() {
    let missile = item(
        kind::MISSILE,
        &[
            (value::DAMAGE, 2840.0),
            (value::RADIUS, 10.0),
            (value::LOCK_TIME, 2.5),
            (value::LOCK_RANGE, 7000.0),
            (value::SPEED, 950.0),
        ],
    );
    assert_eq!(
        lines(&missile, &GERMAN),
        ["Schaden 2.840 · Radius 10 m", "Zielerfassung 2,5 s bis 7.000 m", "Tempo 950 m/s"]
    );
    assert_eq!(lines(&missile, &ENGLISH)[1], "Lock 2.5 s up to 7,000 m");
}

#[test]
fn armour() {
    let armor = item(
        kind::ARMOR,
        &[
            (value::RESIST_PHYSICAL, 0.3),
            (value::RESIST_ENERGY, 0.3),
            (value::RESIST_DISTORTION, 0.1),
            (value::TEMP_MIN, -95.0),
            (value::TEMP_MAX, 125.0),
            (value::RADIATION, 9000.0),
        ],
    );
    assert_eq!(
        lines(&armor, &GERMAN),
        ["Widerstand: Phys 30 % · Energie 30 % · Distortion 10 %", "Temperatur -95 bis 125 °C", "Strahlung 9.000 REM"]
    );
}

#[test]
fn an_unknown_kind_and_an_item_without_values_give_no_line() {
    assert!(lines(&item("raumschiff", &[(value::DPS, 1.0)]), &GERMAN).is_empty());
    assert!(lines(&item(kind::COOLER, &[]), &GERMAN).is_empty());
    assert!(lines(&item(kind::COOLER, &[(value::COOLING, f64::NAN)]), &GERMAN).is_empty());
}

/// Every value of every kind at the longest it reaches in the game data (LIVE 4.10.2) and a digit more: no kind
/// has more than four lines, and no line is longer than [`MAX_LINE`], in either language.
#[test]
fn every_kind_stays_within_four_lines_of_the_width_of_the_rule_at_its_longest() {
    let longest: [(&str, &[(&str, f64)]); 8] = [
        (
            kind::SHIP_WEAPON,
            &[
                (value::DPS, 99_999.0),
                (value::ALPHA, 99_999.0),
                (value::FIRE_RATE, 9_999.0),
                (value::RANGE, 99_999.0),
                (value::SPEED, 99_999.0),
                (value::PENETRATION, 99.9),
                (value::SPREAD, 99.9),
                (value::POWER, 99.9),
                (value::EM, 999_999.0),
                (value::IR, 999_999.0),
                (value::AMMO, 99_999.0),
                (value::HEALTH, 999_999.0),
            ],
        ),
        (
            kind::PERSONAL_WEAPON,
            &[
                (value::DPS, 99_999.0),
                (value::ALPHA, 99_999.0),
                (value::FIRE_RATE, 9_999.0),
                (value::RANGE, 99_999.0),
                (value::SPEED, 99_999.0),
                (value::PENETRATION, 99.9),
                (value::AMMO, 9_999.0),
                (value::SPREAD, 9.99),
            ],
        ),
        (
            kind::SHIELD,
            &[
                (value::SHIELD_HP, 9_999_999.0),
                (value::REGEN, 999_999.0),
                (value::DAMAGED_DELAY, 99.99),
                (value::DOWNED_DELAY, 99.99),
                (value::RESIST_PHYSICAL, 1.0),
                (value::RESIST_ENERGY, 1.0),
                (value::RESIST_DISTORTION, 1.0),
                (value::ABSORB_PHYSICAL, 1.0),
                (value::ABSORB_ENERGY, 1.0),
                (value::ABSORB_DISTORTION, 1.0),
            ],
        ),
        (
            kind::COOLER,
            &[
                (value::COOLING, 99_999.0),
                (value::POWER, 99.9),
                (value::EM, 999_999.0),
                (value::IR, 999_999.0),
                (value::HEALTH, 999_999.0),
                (value::DISTORTION_HP, 999_999.0),
                (value::RESIST_PHYSICAL, -1.0),
                (value::RESIST_ENERGY, -1.0),
                (value::RESIST_DISTORTION, -1.0),
            ],
        ),
        (
            kind::POWER_PLANT,
            &[
                (value::OUTPUT, 999.0),
                (value::EM, 999_999.0),
                (value::IR, 999_999.0),
                (value::HEALTH, 999_999.0),
                (value::DISTORTION_HP, 999_999.0),
                (value::RESIST_PHYSICAL, -1.0),
                (value::RESIST_ENERGY, -1.0),
                (value::RESIST_DISTORTION, -1.0),
            ],
        ),
        (
            kind::QUANTUM_DRIVE,
            &[
                (value::DRIVE_SPEED, 9_999_999_999.0),
                (value::FUEL, 0.9999),
                (value::SPOOL, 99.99),
                (value::COOLDOWN, 99.99),
                (value::POWER, 99.9),
                (value::EM, 999_999.0),
                (value::IR, 999_999.0),
                (value::HEALTH, 999_999.0),
                (value::DISTORTION_HP, 999_999.0),
            ],
        ),
        (
            kind::MISSILE,
            &[
                (value::DAMAGE, 999_999.0),
                (value::RADIUS, 999.0),
                (value::LOCK_TIME, 99.99),
                (value::LOCK_RANGE, 999_999.0),
                (value::SPEED, 99_999.0),
            ],
        ),
        (
            kind::ARMOR,
            &[
                (value::RESIST_PHYSICAL, -1.0),
                (value::RESIST_ENERGY, -1.0),
                (value::RESIST_DISTORTION, -1.0),
                (value::TEMP_MIN, -999.0),
                (value::TEMP_MAX, 999.0),
                (value::RADIATION, 9_999_999.0),
            ],
        ),
    ];
    for (kind, values) in longest {
        for words in [&GERMAN, &ENGLISH] {
            let rows = lines(&item(kind, values), words);
            assert!(!rows.is_empty() && rows.len() <= 4, "{kind}: {rows:?}");
            for row in &rows {
                assert!(row.chars().count() <= MAX_LINE, "{kind}, {} characters: {row}", row.chars().count());
            }
        }
    }
}

const FILE: [&str; 6] = [
    "item_DescA=Hersteller: X",
    "item_Desc_Shared=Hersteller: Y",
    "item_DescB=Hersteller: Z",
    "item_NameA=A",
    "item_Mod_Name=Modul",
    "item_Mod_Desc=Hersteller: M",
];

fn keyed(name: &str, text: &str, cooling: f64) -> ItemStats {
    ItemStats { name: name.into(), text: text.into(), ..item(kind::COOLER, &[(value::COOLING, cooling)]) }
}

#[test]
fn a_block_per_description_that_reads_the_same_for_all_its_items() {
    let items = [
        keyed("item_NameA", "item_DescA", 38.0),
        keyed("item_NameS1", "item_Desc_Shared", 20.0),
        keyed("item_NameS2", "item_desc_shared", 20.0),
        keyed("item_NameB1", "item_DescB", 30.0),
        keyed("item_NameB2", "item_DescB", 31.0),
        keyed("item_NameC", "item_DescMissing", 31.0),
        keyed("item_Mod_Name", "item_Mod_Name", 12.0),
    ];
    let blocks = build(&FILE, &items, &GERMAN);
    assert!(blocks["item_DescA"].ends_with("\\n<EM4>VERSEKIT: WERTE AB WERK</EM4>\\nKühlleistung 38"), "{blocks:?}");
    assert!(blocks["item_DescA"].starts_with(&format!("\\n\\n{}\\n\\n<EM4>", super::super::texts::rule())));
    assert!(blocks["item_Desc_Shared"].ends_with("Kühlleistung 20"), "the same for both: {blocks:?}");
    assert!(!blocks.contains_key("item_DescB"), "two values on one description: {blocks:?}");
    assert!(!blocks.contains_key("item_DescMissing"), "a key the file lacks: {blocks:?}");
    assert!(!blocks.contains_key("item_Mod_Name"), "never a name key: {blocks:?}");
    assert!(blocks["item_Mod_Desc"].ends_with("Kühlleistung 12"), "the description beside the name key: {blocks:?}");
    assert_eq!(blocks.len(), 3, "{blocks:?}");
}

#[test]
fn an_item_without_values_or_of_another_kind_on_the_same_description_keeps_the_block_off() {
    let without = ItemStats { name: "item_NameA2".into(), text: "item_DescA".into(), ..ItemStats::default() };
    let blocks = build(&FILE, &[keyed("item_NameA", "item_DescA", 38.0), without], &GERMAN);
    assert!(blocks.is_empty(), "the other item would show numbers not its own: {blocks:?}");
    let plant = ItemStats {
        name: "item_NameA3".into(),
        text: "item_DescA".into(),
        ..item(kind::POWER_PLANT, &[(value::OUTPUT, 20.0)])
    };
    assert!(build(&FILE, &[keyed("item_NameA", "item_DescA", 38.0), plant], &GERMAN).is_empty());
    let only_without = ItemStats { name: "item_NameA".into(), text: "item_DescA".into(), ..ItemStats::default() };
    assert!(build(&FILE, &[only_without], &GERMAN).is_empty(), "no values, no block");
}

#[test]
fn the_shown_key_is_the_description_or_the_one_beside_the_name_key() {
    assert_eq!(shown_text_key("item_NameA", "item_DescA"), "item_desca");
    assert_eq!(shown_text_key("item_Mod_Name", "item_Mod_Name"), "item_mod_desc");
    assert_eq!(shown_text_key("item_Mod", "item_Mod"), "", "a name key that does not end in _Name");
}

#[test]
fn the_blueprint_part_follows_the_values_below_one_rule() {
    let stats = block(&["Kühlleistung 38".to_string()], &GERMAN);
    let blueprint = format!(
        "\\n\\n{}\\n\\n<EM4>VERSEKIT: BAUPLAN & HERSTELLUNG</EM4>\\nBauplan: [  ] fehlt",
        super::super::texts::rule()
    );
    let both = joined(&stats, &blueprint);
    assert_eq!(both.matches(&super::super::texts::rule()).count(), 1, "{both}");
    assert!(
        both.ends_with("Kühlleistung 38\\n\\n<EM4>VERSEKIT: BAUPLAN & HERSTELLUNG</EM4>\\nBauplan: [  ] fehlt"),
        "{both}"
    );
}

#[test]
fn the_values_are_read_from_the_catalog_of_the_game_data() {
    let data = jsonio::loads(
        r#"{"bauplaene": {}, "werte": {
            "g1": {"name": "item_NameA", "text": "item_DescA", "art": "kuehler", "w": {"kuehlung": 38, "em": 1490.5}},
            "g2": {"name": "item_NameB", "text": " ", "art": "kuehler", "w": {}},
            "g3": {"name": "item_NameC", "text": "item_DescC", "art": "", "w": {}}}}"#,
    )
    .unwrap();
    let items = from_catalog(&data);
    assert_eq!(items.len(), 2, "an item without text is left out, one without kind kept");
    assert_eq!(items[0].text, "item_DescA");
    assert_eq!(items[0].values[value::COOLING], 38.0);
    assert_eq!(items[0].values[value::EM], 1490.5);
    assert_eq!((items[1].text.as_str(), items[1].kind.as_str()), ("item_DescC", ""));
    assert!(items[1].values.is_empty());
    assert!(from_catalog(&jsonio::loads(r#"{"bauplaene": {}}"#).unwrap()).is_empty());
}

#[test]
fn the_headings_are_known_as_ours() {
    for heading in HEADINGS {
        assert!(super::super::form::HEADINGS.contains(&heading));
        assert!(super::super::form::is_ours(&format!("<EM4>{heading}</EM4>\\nKühlleistung 38")));
    }
}
