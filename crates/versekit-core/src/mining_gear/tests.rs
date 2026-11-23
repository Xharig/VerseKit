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

use super::*;
use crate::rock_crack::{Rock, Verdict, assess};

fn equipment() -> Value {
    jsonio::loads(
        r#"{"format": 1, "spielversion": "sc-alpha-4.4.0.1",
            "koepfe": [
              {"ref": "h2", "name": "Helix I", "groesse": 1, "leistung": 3150, "widerstand": -30, "instabilitaet": 10, "plaetze": 2},
              {"ref": "h1", "name": "Arbor MH1", "groesse": 1, "leistung": 1890, "widerstand": 25, "instabilitaet": -35, "plaetze": 1},
              {"ref": "x", "name": "No beam", "groesse": 1, "leistung": 0}
            ],
            "module": [
              {"ref": "rieger", "name": "Rieger-C3", "aktiv": false, "leistung": 1.25, "widerstand": 0, "instabilitaet": 0},
              {"ref": "surge", "name": "Surge", "aktiv": true, "ladungen": 5, "dauer": 15, "leistung": 1.5, "widerstand": -15},
              {"ref": "focus", "name": "Focus III", "aktiv": false, "leistung": 0.85, "widerstand": -10}
            ],
            "gadgets": [{"ref": "sabir", "name": "Sabir", "widerstand": -50, "instabilitaet": 15}]}"#,
    )
    .unwrap()
}

fn ships() -> Value {
    jsonio::loads(
        r#"{"format": 2, "schiffe": {
            "prospector": {"name": "MISC Prospector", "id": "MISC_Prospector", "slots": [
              {"pfad": "hardpoint_mining_laser", "art": "WeaponMining", "groesse": 1, "werk": {"ref": "h1", "name": "Arbor MH1"}},
              {"pfad": "hardpoint_mining_laser/module", "art": "MiningModifier", "groesse": 1}]},
            "gladius": {"name": "Aegis Gladius", "id": "AEGS_Gladius", "slots": [{"pfad": "gun", "art": "WeaponGun", "groesse": 3}]}
        }}"#,
    )
    .unwrap()
}

#[test]
fn the_files_give_heads_modules_gadgets_and_the_mining_ships() {
    let gear = Gear::from_values(Some(&equipment()), Some(&ships()));
    let names: Vec<&str> = gear.heads.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, vec!["Arbor MH1", "Helix I"], "by name, a head without power left out");
    assert_eq!(gear.head("h2").unwrap().slots, 2);
    let surge = gear.module("surge").unwrap();
    assert!(surge.active && surge.charges == Some(5) && surge.lifetime == Some(15.0));
    assert_eq!(gear.gadget("sabir").unwrap().resistance, -50.0);
    assert_eq!(gear.ships.len(), 1, "only ships with a mining head");
    assert_eq!(gear.ships[0].heads, vec![(1, Some("h1".to_string()))]);
    assert_eq!(gear.build, "sc-alpha-4.4.0.1");
}

#[test]
fn a_gadget_named_by_its_class_is_no_gadget_to_choose() {
    let file = jsonio::loads(
        r#"{"format": 1, "koepfe": [], "module": [], "gadgets": [
              {"ref": "a", "name": "behr_ltp_kinetic_01_MiningBase", "klasse": "behr_ltp_kinetic_01_MiningBase"},
              {"ref": "b", "name": "", "klasse": "rkhm_dply_shield_01_MiningBase"},
              {"ref": "c", "name": "Sabir", "klasse": "mining_gadget_sabir", "widerstand": -50}]}"#,
    )
    .unwrap();
    let gear = Gear::from_values(Some(&file), None);
    assert_eq!(gear.gadgets.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["Sabir"]);
}

#[test]
fn a_file_of_another_format_gives_no_equipment() {
    let mut old = equipment();
    if let Value::Object(map) = &mut old {
        map[0].1 = Value::int(99);
    }
    assert!(Gear::from_values(Some(&old), None).is_empty());
    assert!(Gear::from_values(None, None).is_empty());
}

#[test]
fn the_choice_goes_through_the_settings_and_the_fitting_comes_from_the_hangar() {
    let lasers = vec![Chosen { head: "h2".into(), modules: vec!["rieger".into(), "focus".into()] }];
    let choice = Choice { ship: "MOLE\u{1f}Argo\u{1f}0".into(), lasers: lasers.clone(), gadget: "sabir".into(), active: "surge".into() };
    let values = choice.setting_values();
    assert!(values.iter().all(|(k, _)| *k != "stein_laser"), "the fitting is not kept twice");
    let get = |key: &str| values.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
    let back = Choice::from_settings(get(keys::SHIP), get(keys::GADGET), get(keys::ACTIVE));
    assert_eq!((back.ship.as_str(), back.gadget.as_str(), back.active.as_str()), ("MOLE\u{1f}Argo\u{1f}0", "sabir", "surge"));
    let ships = vec![
        HangarShip {
            key: "Prospector\u{1f}MISC\u{1f}0".into(),
            index: 0,
            name: "Prospector".into(),
            lasers: vec![Chosen::default()],
        },
        HangarShip { key: "MOLE\u{1f}Argo\u{1f}0".into(), index: 3, name: "MOLE".into(), lasers: lasers.clone() },
    ];
    assert_eq!(back.clone().with_ship(&ships).lasers, lasers, "the chosen ship's fitting");
    let gone = Choice { ship: "Gone\u{1f}X\u{1f}0".into(), ..back.clone() }.with_ship(&ships);
    assert_eq!(gone.ship, "Prospector\u{1f}MISC\u{1f}0", "a ship no longer in the hangar: the first mining ship");
    let none = back.with_ship(&[]);
    assert!(none.ship.is_empty() && none.lasers.is_empty());
}

fn slot(path: &str, kind: &str, factory: Option<&str>) -> crate::erkul_slots::SlotInfo {
    crate::erkul_slots::SlotInfo {
        path: path.into(),
        kind: kind.into(),
        size: Some(2),
        fit: None,
        factory: factory.map(|r| (r.to_string(), r.to_string())),
        fixed: false,
    }
}

/// A MOLE as a tester flies it: turret 1 with a planned head and three modules, turret 2 as at the factory with one
/// module slot left empty in the plan.
#[test]
fn the_fitting_is_the_planned_part_else_the_factory_one_and_an_emptied_slot_has_none() {
    let slots = vec![
        slot("turret_1/head", "WeaponMining", Some("arbor")),
        slot("turret_1/head/module_1", "MiningModifier", None),
        slot("turret_1/head/module_2", "MiningModifier", None),
        slot("turret_1/head/module_3", "MiningModifier", None),
        slot("turret_2/head", "WeaponMining", Some("hofstede")),
        slot("turret_2/head/module_1", "MiningModifier", Some("focus")),
        slot("turret_2/head/module_2", "MiningModifier", Some("focus")),
        slot("gun", "WeaponGun", Some("cf337")),
    ];
    let plan = jsonio::loads(
        r#"{"turret_1/head": {"ref": "helix2", "name": "Helix II"},
            "turret_1/head/module_1": {"ref": "focus", "name": "Focus III"},
            "turret_1/head/module_2": {"ref": "rieger", "name": "Rieger-C3"},
            "turret_1/head/module_3": {"ref": "rieger", "name": "Rieger-C3"},
            "turret_2/head/module_2": {"ref": "", "leer": true}}"#,
    )
    .unwrap();
    let Value::Object(plan) = plan else { unreachable!() };
    let fitted = fitted(&slots, &plan);
    assert_eq!(
        fitted,
        vec![
            Chosen { head: "helix2".into(), modules: vec!["focus".into(), "rieger".into(), "rieger".into()] },
            Chosen { head: "hofstede".into(), modules: vec!["focus".into()] },
        ]
    );
}

#[test]
fn the_choice_turns_into_the_numbers_of_the_answer() {
    let gear = Gear::from_values(Some(&equipment()), Some(&ships()));
    let choice = Choice {
        ship: String::new(),
        lasers: vec![
            Chosen { head: "h2".into(), modules: vec!["rieger".into(), "surge".into(), "focus".into()] },
            Chosen { head: "gone".into(), modules: vec![] },
        ],
        gadget: "sabir".into(),
        active: "surge".into(),
    };
    let loadout = choice.loadout(&gear);
    assert_eq!(loadout.lasers.len(), 1, "an unknown head is left out");
    let laser = &loadout.lasers[0];
    assert_eq!(laser.modules.len(), 2, "the active module does not sit in the head as a passive one");
    assert!((laser.head.resistance - 0.7).abs() < 1e-9);
    let all = loadout.combined().unwrap();
    // 3150 × 1.25 × 0.85, then × 1.5 of the active module.
    assert!((all.power - 3150.0 * 1.25 * 0.85 * 1.5).abs() < 1e-6);
    // 0.7 × 0.9 of the head and Focus, × 0.5 of the gadget, × 0.85 of the active module.
    assert!((all.resistance - 0.7 * 0.9 * 0.5 * 0.85).abs() < 1e-9);
    let rock = Rock { mass: Some(20_000.0), resistance: Some(40.0), instability: None };
    assert!(matches!(assess(&rock, &loadout), Verdict::Known(_)));
}

#[test]
fn a_head_takes_no_more_modules_than_it_has_slots() {
    let gear = Gear::from_values(Some(&equipment()), None);
    let choice = Choice {
        lasers: vec![Chosen { head: "h1".into(), modules: vec!["rieger".into(), "focus".into()] }],
        ..Choice::default()
    };
    assert_eq!(choice.loadout(&gear).lasers[0].modules.len(), 1);
}
