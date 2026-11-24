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
use egui::{Rect, vec2};
use versekit_core::jsonio;
use versekit_core::language::{Choice as LanguageChoice, set_thread_language};
use versekit_core::mining_gear::Chosen;

fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
    out
}

/// The card drawn a few times in the page frame: its texts and how far it sticks out.
fn draw(state: &mut RockBox, ships: &[HangarShip], width: f32, scanner_on: bool) -> (Vec<String>, f32) {
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let scheme = versekit_core::theme::current();
    let (mut shown, mut over) = (Vec::new(), 0.0);
    for _ in 0..3 {
        let mut output = ctx.run_ui(
            egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 3000.0))), ..Default::default() },
            |ui| {
                let page =
                    crate::layout::page(ui, scheme, "stein-test", |ui| show_box(ui, scheme, state, ships, scanner_on, 0.0));
                over = page.overflow;
            },
        );
        output.textures_delta.clear();
        shown = texts(&output.shapes);
    }
    (shown, over)
}

fn gear() -> Gear {
    let equipment = jsonio::loads(
        r#"{"format": 1, "koepfe": [
              {"ref": "helix2", "name": "Helix II Mining Laser", "groesse": 2, "leistung": 4080, "widerstand": -30, "plaetze": 3},
              {"ref": "hofstede", "name": "Hofstede-S2 Mining Laser", "groesse": 2, "leistung": 3360, "plaetze": 2}],
            "module": [{"ref": "rieger", "name": "Rieger-C3", "aktiv": false, "leistung": 1.25},
                       {"ref": "focus", "name": "Focus III", "aktiv": false, "leistung": 0.95},
                       {"ref": "surge", "name": "Surge", "aktiv": true, "leistung": 1.5}],
            "gadgets": [{"ref": "sabir", "name": "Sabir", "widerstand": -50}]}"#,
    )
    .unwrap();
    Gear::from_values(Some(&equipment), None)
}

/// A MOLE as a tester flies it: turret 1 Helix II with Focus III and two Rieger-C3, turret 2 Hofstede-S2 with two Focus III.
fn mole() -> HangarShip {
    HangarShip {
        key: "MOLE\u{1f}Argo\u{1f}0".into(),
        index: 2,
        name: "MOLE".into(),
        lasers: vec![
            Chosen { head: "helix2".into(), modules: vec!["focus".into(), "rieger".into(), "rieger".into()] },
            Chosen { head: "hofstede".into(), modules: vec!["focus".into(), "focus".into()] },
        ],
    }
}

fn prospector() -> HangarShip {
    HangarShip {
        key: "Prospector\u{1f}MISC\u{1f}0".into(),
        index: 0,
        name: "Prospector".into(),
        lasers: vec![Chosen { head: "hofstede".into(), modules: vec![] }],
    }
}

#[test]
fn without_game_data_the_card_says_where_the_equipment_comes_from() {
    let _language = crate::language_lock();
    for language in [LanguageChoice::De, LanguageChoice::En] {
        set_thread_language(language);
        let mut state = RockBox::default();
        let (shown, over) = draw(&mut state, &[], 420.0, false);
        assert_eq!(over, 0.0);
        for key in ["s_stein_kopf", "s_stein_scanner_aus", "s_stein_keine_daten"] {
            assert!(shown.iter().any(|s| s.contains(&t(key))), "{key}: {shown:?}");
        }
    }
}

#[test]
fn the_fitting_comes_from_the_hangar_ship_and_the_trial_answers_with_it() {
    let _language = crate::language_lock();
    set_thread_language(LanguageChoice::De);
    let mut state = RockBox { gear: gear(), ..RockBox::default() };
    state.follow_ships(&[prospector(), mole()]);
    assert_eq!(state.choice.ship, prospector().key, "nothing chosen: the first mining ship");
    state.choice.ship = mole().key;
    state.follow_ships(&[prospector(), mole()]);
    assert_eq!(state.choice.lasers, mole().lasers);
    assert_eq!(
        state.fitted_lines(),
        vec![
            "Helix II Mining Laser: Focus III, Rieger-C3, Rieger-C3".to_string(),
            "Hofstede-S2 Mining Laser: Focus III, Focus III".to_string()
        ]
    );
    assert_eq!(state.trial(), None, "nothing typed");
    state.mass = "5000".into();
    state.resistance = "0 %".into();
    let before = |key: &str| t(key).split('{').next().unwrap_or_default().to_string();
    let easy = format!("{}{}", before("s_stein_ja"), t("s_stein_stufe_leicht"));
    assert!(state.trial().unwrap().starts_with(&easy), "{:?}", state.trial());
    state.mass = "32975".into();
    state.resistance = "61".into();
    assert!(state.trial().unwrap().starts_with(&before("s_stein_nein")), "{:?}", state.trial());
    for typed in ["150.000", "150,000", "150000"] {
        state.mass = typed.into();
        state.resistance = "72".into();
        assert!(state.trial().unwrap().starts_with(&before("s_stein_nein")), "{typed}: {:?}", state.trial());
    }
    state.resistance = "abc".into();
    assert_eq!(state.trial(), None);
}

#[test]
fn the_trial_names_the_lasers_that_fire_and_a_laser_switched_off_is_left_out() {
    let _language = crate::language_lock();
    set_thread_language(LanguageChoice::De);
    let mut state = RockBox { gear: gear(), choice: Choice { ship: mole().key, ..Choice::default() }, ..RockBox::default() };
    state.follow_ships(&[mole()]);
    state.mass = "9000".into();
    state.resistance = "20".into();
    let both = state.trial().unwrap();
    let all = versekit_core::language::tf("s_stein_alle", &[("0", versekit_core::language::Arg::Int(2))]);
    assert!(both.contains(&all), "{both}");
    state.choice.set_fires(1, false);
    let alone = state.trial().unwrap();
    let short = |reference: &str| versekit_core::mining_gear::short_head_name(&state.gear.head(reference).unwrap().name);
    assert!(alone.contains(&short("helix2")), "{alone}");
    assert!(!alone.contains(&short("hofstede")), "{alone}");
    let (shown, over) = draw(&mut state, &[mole()], 420.0, true);
    assert_eq!(over, 0.0);
    assert!(shown.iter().any(|s| s.contains(&t("s_stein_feuern"))), "{shown:?}");
}

#[test]
fn the_card_shows_the_ship_its_fitting_and_the_switches_in_both_languages_and_widths() {
    let _language = crate::language_lock();
    for language in [LanguageChoice::De, LanguageChoice::En] {
        set_thread_language(language);
        for width in [360.0, 900.0] {
            let ships = [prospector(), mole()];
            let mut state = RockBox {
                gear: gear(),
                mass: "5000".into(),
                resistance: "0".into(),
                choice: Choice { ship: mole().key, ..Choice::default() },
                ..RockBox::default()
            };
            let (shown, over) = draw(&mut state, &ships, width, true);
            assert_eq!(over, 0.0, "{width}");
            assert!(!shown.iter().any(|s| s.contains(&t("s_stein_scanner_aus"))), "the scanner is on");
            let wanted = [
                t("s_stein_kopf"),
                t("s_stein_schiff"),
                "MOLE".to_string(),
                "Helix II Mining Laser: Focus III, Rieger-C3, Rieger-C3".to_string(),
                t("s_stein_gadget"),
                t("s_stein_aktiv"),
                t("s_stein_zeile_aufbrechbar"),
                t("s_stein_zeile_wert"),
                state.trial().unwrap(),
            ];
            for text in wanted {
                assert!(shown.iter().any(|s| s.contains(&text)), "{width} {text}: {shown:?}");
            }
        }
    }
}
