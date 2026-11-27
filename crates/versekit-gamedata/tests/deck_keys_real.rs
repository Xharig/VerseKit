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

//! Works out the plan of the Stream Deck keys on real data, applies it in memory only and checks every action of the
//! deck afterwards: the key the deck sends, whether it is safe, and which other action of the same group answers to
//! it too. Writes nothing but the report.
//!
//! Run with `cargo test -p versekit-gamedata --release --test deck_keys_real -- --ignored --nocapture` and
//! `VERSEKIT_DECK_ACTIONS` (a file with one action name per line), `VERSEKIT_DECK_MAPS` (an `actionmaps.xml`),
//! `VERSEKIT_DECK_P4K` (a `Data.p4k`) and `VERSEKIT_DECK_REPORT` (the file the report goes to).

use versekit_core::controls::{self, Profile, Source, View};
use versekit_core::game_binding::{self, How};
use versekit_core::joysticks::bindings_from_text;
use versekit_core::joysticks_write;
use versekit_core::streamdeck_fill::{self, Outcome};
use versekit_gamedata::GameData;
use versekit_gamedata::control_profile::profile_xml;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// The other actions of the same group that answer to `input` on the keyboard, factory or own.
fn others_on(all: &controls::Devices, action: &str, group: &str, input: &str, profile: &Profile) -> Vec<String> {
    controls::conflicts(all, action, "kb1", input)
        .into_iter()
        .chain(controls::conflicts(all, action, "mo1", input))
        .filter(|e| profile.groups.get(e.action.as_str()).map(String::as_str) == Some(group))
        .map(|e| format!("{}{}", e.action, if e.source == Source::Mine { " (eigen)" } else { "" }))
        .collect()
}

#[test]
#[ignore = "reads the real game and bindings"]
fn every_key_of_the_deck_is_safe_and_its_own_after_the_plan() {
    let names: Vec<String> =
        std::fs::read_to_string(var("VERSEKIT_DECK_ACTIONS")).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    let maps = std::fs::read_to_string(var("VERSEKIT_DECK_MAPS")).unwrap();
    let game = GameData::open_file(var("VERSEKIT_DECK_P4K")).unwrap();
    let profile = Profile::from_xml(&profile_xml(&game).unwrap()).unwrap();
    let own = bindings_from_text(&maps);
    let items = streamdeck_fill::plan(&names, &own, &profile, &[], false);
    let (adds, dropped, _) = streamdeck_fill::writing_of(&items);
    let mice: Vec<(String, String, String)> = items
        .iter()
        .filter_map(|i| match &i.outcome {
            Outcome::Move { mouse, .. } => Some((i.action.clone(), i.group.clone(), mouse.clone())),
            _ => None,
        })
        .collect();
    let after_text = joysticks_write::fill_text(&maps, &adds, &mice, &dropped).unwrap();
    let mut moved: Vec<String> = mice.iter().map(|(_, _, m)| m.clone()).collect();
    moved.extend(joysticks_write::mouse_inputs(&dropped));
    let after = bindings_from_text(&after_text);
    let all = controls::view(View::All, &after, &profile);
    let mut out = String::from("aktion|plan|taste_danach|quelle|sicher|auch_auf_der_taste\n");
    let mut problems = 0;
    for (name, item) in names.iter().zip(&items) {
        let plan = match &item.outcome {
            Outcome::Add(k) => format!("neu {k}"),
            Outcome::Relocate { from, key, .. } => format!("umziehen {from} -> {key}"),
            Outcome::Restore { from, factory, .. } => format!("werk {from} -> {factory}"),
            Outcome::HasKey => "hat".into(),
            other => format!("{other:?}"),
        };
        let resolved = game_binding::resolve(name, &after, &profile);
        let (key, safe, others) = match &resolved.how {
            How::Keyboard { input, .. } => {
                let safe = !streamdeck_fill::unsafe_input(&all, input);
                let (_, plain) = game_binding::qualified(name, &profile);
                (input.clone(), safe, others_on(&all, plain, &resolved.group, input, &profile))
            }
            other => (format!("({})", other.word()), false, Vec::new()),
        };
        if !safe {
            problems += 1;
        }
        // The player's own mouse button stays; the deck presses it.
        let on_mouse = matches!(&resolved.how, How::Keyboard { keys, .. } if keys.last().is_some_and(|k| k.is_mouse()));
        let source = match (resolved.own, on_mouse) {
            (true, true) => "eigen (Maus)",
            (true, false) => "eigen",
            (false, _) => "werk",
        };
        out.push_str(&format!("{name}|{plan}|{key}|{source}|{}|{}\n", if safe { "ja" } else { "NEIN" }, others.join(", ")));
    }
    out.push_str(&format!("\nnicht sicher oder nicht sendbar: {problems}\n"));
    out.push_str(&format!("andere Geraetezeilen unberuehrt: {:?}\n", joysticks_write_check(&maps, &after_text, &moved)));
    let untouched = joysticks_write_check(&maps, &after_text, &moved);
    std::fs::write(var("VERSEKIT_DECK_REPORT"), out).unwrap();
    assert_eq!(problems, 0, "every key of the deck is safe and can be sent after the plan");
    assert!(untouched, "no stick, pad or other mouse line changed");
}

/// Writes the factory keys of the game actions of the deck the plugin brings (`streamdeck-plugin/werkstasten.json`):
/// each action with a safe key or mouse button of the factory setting, and how the game takes it. Needs
/// `VERSEKIT_DECK_ACTIONS`, `VERSEKIT_DECK_P4K` and `VERSEKIT_DECK_FACTORY` (the file written).
#[test]
#[ignore = "reads the real game"]
fn the_factory_keys_of_the_deck_are_written_for_the_plugin() {
    let names: Vec<String> =
        std::fs::read_to_string(var("VERSEKIT_DECK_ACTIONS")).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    let game = GameData::open_file(var("VERSEKIT_DECK_P4K")).unwrap();
    let profile = Profile::from_xml(&profile_xml(&game).unwrap()).unwrap();
    let all = controls::view(View::All, &[], &profile);
    let mut entries = serde_json::Map::new();
    for name in &names {
        let resolved = game_binding::resolve(name, &[], &profile);
        let How::Keyboard { input, .. } = &resolved.how else { continue };
        if streamdeck_fill::unsafe_input(&all, input) {
            continue;
        }
        let timing = profile.timing_of(name).word();
        entries.insert(name.clone(), serde_json::json!({"eingabe": input, "halten": timing}));
    }
    let value = serde_json::json!({
        "hinweis": "Werksbelegung der Spielaktionen der VerseKit-Profile, aus der defaultProfile.xml des Spiels (tests/deck_keys_real.rs). Das Plugin drückt sie, solange es von VerseKit noch keine Tabelle bekam.",
        "fassung": 1,
        "sprache": "de",
        "von_versekit": false,
        "aktionen": entries,
    });
    std::fs::write(var("VERSEKIT_DECK_FACTORY"), serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
}

/// Whether the writing left every stick, pad and mouse line but the allowed mouse lines as they were.
fn joysticks_write_check(old: &str, new: &str, mice: &[String]) -> bool {
    let lines = |text: &str| joysticks_write::other_device_lines(text);
    let (mut before, mut after) = (lines(old), lines(new));
    before.retain(|line| match after.iter().position(|l| l == line) {
        Some(at) => {
            after.remove(at);
            false
        }
        None => true,
    });
    before.iter().chain(after.iter()).all(|line| mice.iter().any(|m| line.eq_ignore_ascii_case(&format!("<rebind input=\"mo1_{m}\"/>"))))
}
