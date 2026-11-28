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
use crate::render;

/// VerseKit as a test wants it: a state, bindings, and a list of the commands it got.
struct FakeVerse {
    state: serde_json::Map<String, Value>,
    rev: u64,
    away: bool,
    bindings: Value,
    commands: Vec<Command>,
    refuse: bool,
}

impl FakeVerse {
    fn new() -> FakeVerse {
        let mut state = serde_json::Map::new();
        state.insert("version".into(), json!(1));
        state.insert("sprache".into(), json!("de"));
        state.insert("overlay".into(), json!(false));
        state.insert("hauptfenster".into(), json!("zu"));
        state.insert("belegung_stand".into(), json!("a"));
        FakeVerse {
            state,
            rev: 1,
            away: false,
            bindings: json!({"aktionen": [
                {"id": "v_lights", "art": "tastatur", "taste": "Alt rechts + Y", "name": "Licht", "gruppe": "Schiff"},
                {"id": "v_toggle_mining_laser_type", "art": "nur_maus", "taste": "", "name": "Bergbaulaser wechseln", "gruppe": "Bergbau"},
                {"id": "v_shield_up", "art": "tastatur", "taste": "F5", "name": "Schilde hoch", "gruppe": "Energie"},
                {"id": "v_shield_down", "art": "tastatur", "taste": "F6", "name": "Schilde runter", "gruppe": "Energie"},
                {"id": "v_eject", "art": "tastatur", "taste": "Alt rechts + Y", "name": "Aussteigen", "gruppe": "Schiff"},
                {"id": "v_self_destruct", "art": "tastatur", "taste": "Rück", "name": "Selbstzerstörung", "gruppe": "Schiff"},
                {"id": "emote_cs_yes", "art": "tastatur", "taste": "Ziffernblock 4", "name": "ja", "gruppe": "Emotes"},
                {"id": "emote_wave", "art": "tastatur", "taste": "Strg rechts + Y", "name": "Winken", "gruppe": "Emotes"}
            ]}),
            commands: Vec::new(),
            refuse: false,
        }
    }

    fn set(&mut self, field: &str, value: Value) {
        self.state.insert(field.into(), value);
        self.rev += 1;
    }
}

impl Verse for FakeVerse {
    fn state(&mut self, since: Option<u64>) -> Result<Option<State>, Missing> {
        if self.away {
            return Err(Missing::Unreachable);
        }
        if since == Some(self.rev) {
            return Ok(None);
        }
        let mut map = self.state.clone();
        map.insert("rev".into(), json!(self.rev));
        Ok(State::parse(&Value::Object(map).to_string()))
    }

    fn command(&mut self, command: &Command) -> Result<(), Missing> {
        if self.away {
            return Err(Missing::Unreachable);
        }
        if self.refuse {
            return Err(Missing::Command(400, "nein".into()));
        }
        self.commands.push(command.clone());
        Ok(())
    }

    fn bindings(&mut self) -> Result<Value, Missing> {
        Ok(self.bindings.clone())
    }
}

fn appear(context: &str, action: Action, settings: Value) -> Value {
    json!({"event": "willAppear", "action": action.uuid(), "context": context, "device": "D1",
           "payload": {"settings": settings, "controller": "Keypad"}})
}

fn dial(context: &str, settings: Value) -> Value {
    json!({"event": "willAppear", "action": Action::GameAction.uuid(), "context": context, "device": "D1",
           "payload": {"settings": settings, "controller": "Encoder"}})
}

fn events<'a>(out: &'a [Value], name: &str) -> Vec<&'a Value> {
    out.iter().filter(|m| m["event"] == name).collect()
}

fn ready() -> (Plugin, FakeVerse) {
    let mut plugin = Plugin::new("PLUGIN", Vec::new());
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    (plugin, verse)
}

fn without(commands: &[Command]) -> Vec<Command> {
    commands
        .iter()
        .filter(|c| !matches!(c, Command::GameActionsInUse(_) | Command::DeviceHint(_) | Command::PluginVersion(_)))
        .cloned()
        .collect()
}

#[test]
fn a_key_that_appears_gets_its_picture_and_no_title() {
    let (mut plugin, mut verse) = ready();
    let out = plugin.on_event(&appear("K1", Action::Overlay, json!({})), &mut verse);
    assert_eq!(events(&out, "setTitle")[0]["payload"]["title"], "");
    let image = events(&out, "setImage")[0]["payload"]["image"].as_str().expect("image").to_string();
    assert!(image.starts_with("data:image/png;base64,"));
    // The log of the plugin says that the key appeared and got its first picture.
    let lines: Vec<&str> = events(&out, "logMessage").iter().filter_map(|m| m["payload"]["message"].as_str()).collect();
    assert!(lines.iter().any(|l| l.starts_with("willAppear overlay K1")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("setImage overlay K1")), "{lines:?}");
    // The same state again sends no picture.
    assert!(plugin.poll(&mut verse).is_empty());
}

#[test]
fn a_press_sends_the_command_and_the_new_state_redraws_the_key() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::Overlay, json!({})), &mut verse);
    let before = plugin.look(&plugin.keys["K1"].clone());
    verse.set("overlay", json!(true));
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K1", "payload": {}}), &mut verse);
    assert_eq!(without(&verse.commands), vec![Command::Overlay]);
    assert_eq!(events(&out, "setImage").len(), 1, "the key shows the new state at once");
    let after = plugin.look(&plugin.keys["K1"].clone());
    assert_ne!(before.badge, after.badge);
    assert!(after.badge.expect("badge").filled);
}

#[test]
fn the_main_window_shows_open_filled_and_closed_outlined() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::Main, json!({})), &mut verse);
    let closed = plugin.look(&plugin.keys["K1"].clone()).badge.expect("badge");
    assert_eq!((closed.word.as_str(), closed.filled), ("ZU", false));
    verse.set("hauptfenster", json!("offen"));
    plugin.poll(&mut verse);
    let open = plugin.look(&plugin.keys["K1"].clone()).badge.expect("badge");
    assert_eq!((open.word.as_str(), open.filled, open.tone), ("OFFEN", true, Tone::Accent));
    // Open behind another window: yellow HINTEN, the press brings it to the front.
    verse.set("hauptfenster", json!("hinten"));
    plugin.poll(&mut verse);
    let behind = plugin.look(&plugin.keys["K1"].clone()).badge.expect("badge");
    assert_eq!((behind.word.as_str(), behind.tone), ("HINTEN", Tone::Warning));
    // The three states differ without colour too.
    let mut pictures = Vec::new();
    for window in ["zu", "hinten", "offen"] {
        verse.set("hauptfenster", json!(window));
        plugin.poll(&mut verse);
        pictures.push(crate::paint::grey(&crate::paint::png(&plugin.look(&plugin.keys["K1"].clone()), Action::Main)));
    }
    pictures.dedup();
    assert_eq!(pictures.len(), 3);
}

#[test]
fn without_verse_kit_every_key_says_so_and_a_press_gives_a_warning() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::Overlay, json!({})), &mut verse);
    verse.away = true;
    let out = plugin.poll(&mut verse);
    assert_eq!(events(&out, "setImage").len(), 1);
    let look = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!(look.icon, "unplug");
    assert_eq!(look.badge.expect("badge").word, "Kein VerseKit");
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
    // Back again: the key shows the state.
    verse.away = false;
    plugin.poll(&mut verse);
    assert_eq!(plugin.look(&plugin.keys["K1"].clone()).icon, "layers");
}

/// The system as a test wants it: the game in front, every key event written down.
#[derive(Default)]
struct FakeKeys {
    sent: std::sync::Mutex<Vec<versekit_core::anti_afk::KeyEvent>>,
}

impl versekit_core::streamdeck_keys::KeySink for FakeKeys {
    fn game_in_front(&self) -> bool {
        true
    }
    fn send(&self, events: &[versekit_core::anti_afk::KeyEvent]) -> Result<(), versekit_core::anti_afk::SendError> {
        self.sent.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

impl FakeKeys {
    /// The keys that went down, by the names of the game.
    fn downs(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !e.up)
            .map(|e| versekit_core::anti_afk_key::name_of(e.key).unwrap_or("?").to_string())
            .collect()
    }
}

#[test]
fn without_verse_kit_the_game_actions_press_the_kept_keys_and_show_their_german_names() {
    let folder = std::env::temp_dir().join(format!("versekit-sd-ohne-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    let file = folder.join(versekit_core::streamdeck::KEY_TABLE_FILE);
    let keys = std::sync::Arc::new(FakeKeys::default());
    let mut plugin = Plugin::new("PLUGIN", Vec::new());
    plugin.table_file = Some(file.clone());
    plugin.local = Some(keys.clone());
    let mut verse = FakeVerse::new();
    verse.bindings = json!({"aktionen": [
        {"id": "v_toggle_guns_mode", "art": "tastatur", "taste": "Strg rechts + Num 4", "eingabe": "rctrl+np_4", "halten": "folgen", "name": "Waffenmodus umschalten", "karte": "seat_general"},
        {"id": "v_ifcs_speed_limiter_increment", "art": "tastatur", "taste": "Strg rechts + F1", "eingabe": "rctrl+f1", "halten": "folgen", "name": "Begrenzer erhöhen", "karte": "spaceship_movement"},
        {"id": "v_ifcs_speed_limiter_decrement", "art": "tastatur", "taste": "Alt links + Z", "eingabe": "lalt+z", "halten": "folgen", "name": "Begrenzer senken", "karte": "spaceship_movement"}
    ]});
    plugin.poll(&mut verse);
    plugin.on_event(&appear("G", Action::GameAction, json!({"aktion": "v_toggle_guns_mode"})), &mut verse);
    plugin.on_event(&dial("R", json!({"aktion_links": "v_ifcs_speed_limiter_decrement", "aktion_rechts": "v_ifcs_speed_limiter_increment"})), &mut verse);
    plugin.on_event(&appear("O", Action::Overlay, json!({})), &mut verse);
    assert!(file.is_file(), "the plugin keeps the keys of VerseKit");
    // VerseKit is closed.
    verse.away = true;
    plugin.poll(&mut verse);
    let commands_before = verse.commands.len();
    let look = plugin.look(&plugin.keys["G"].clone());
    assert_eq!(look.label, "Waffenmodus umschalten", "the German name of the game, kept");
    assert!(look.badge.is_none() && look.icon_tone != Tone::Warning, "no „Kein VerseKit“ on a game action: {look:?}");
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "G"}), &mut verse);
    assert!(events(&out, "showAlert").is_empty(), "{out:?}");
    plugin.on_event(&json!({"event": "keyUp", "context": "G"}), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R", "payload": {"ticks": -2}}), &mut verse);
    assert_eq!(keys.downs(), ["rctrl", "np_4", "lalt", "z", "lalt", "z"], "pressed by the plugin itself");
    assert_eq!(verse.commands.len(), commands_before, "nothing went to VerseKit");
    // A key of VerseKit itself says it needs VerseKit.
    assert_eq!(plugin.look(&plugin.keys["O"].clone()).badge.expect("badge").word, "Kein VerseKit");
    // A plugin started anew without VerseKit has the kept keys and their language.
    let mut again = Plugin::new("PLUGIN", Vec::new());
    again.table = crate::table::Table::load(Some(&file));
    again.local = Some(keys.clone());
    again.poll(&mut verse);
    again.on_event(&appear("G", Action::GameAction, json!({"aktion": "v_toggle_guns_mode"})), &mut verse);
    assert_eq!(again.look(&again.keys["G"].clone()).label, "Waffenmodus umschalten");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn without_any_kept_keys_a_game_action_presses_its_factory_key_with_its_short_name() {
    let keys = std::sync::Arc::new(FakeKeys::default());
    let mut plugin = Plugin::new("PLUGIN", Vec::new());
    plugin.local = Some(keys.clone());
    let mut verse = FakeVerse::new();
    verse.away = true;
    plugin.poll(&mut verse);
    plugin.on_event(&appear("W", Action::GameAction, json!({"aktion": "v_power_toggle_weapons"})), &mut verse);
    plugin.on_event(&appear("E", Action::GameAction, json!({"aktion": "v_engineering_assignment_engine_decrease"})), &mut verse);
    plugin.on_event(&appear("A", Action::GameAction, json!({"aktion": "ammoRepool"})), &mut verse);
    let look = plugin.look(&plugin.keys["A"].clone());
    assert_eq!(look.label, versekit_core::streamdeck::written("MUNITION BÜNDELN", Lang::De), "the German short name the plugin brings");
    assert!(look.badge.is_none(), "{look:?}");
    for context in ["W", "E"] {
        plugin.on_event(&json!({"event": "keyDown", "context": context}), &mut verse);
        plugin.on_event(&json!({"event": "keyUp", "context": context}), &mut verse);
    }
    assert_eq!(keys.downs(), ["p", "lalt", "f6"], "the factory keys, Alt before F6");
    // An action the factory gives no key and nothing was kept for: it needs VerseKit once, and says so.
    plugin.on_event(&appear("N", Action::GameAction, json!({"aktion": "v_toggle_guns_mode"})), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["N"].clone()).badge.expect("badge").word, "Kein VerseKit");
}

#[test]
fn a_page_key_opens_its_page_and_an_unknown_page_warns_without_sending() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::Page, json!({"seite": "hf_bergbau"})), &mut verse);
    plugin.on_event(&appear("K2", Action::Page, json!({"seite": "s_af_seite"})), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["K1"].clone()).label, language::t_in(Lang::De, "hf_bergbau"));
    let unknown = plugin.look(&plugin.keys["K2"].clone());
    assert_eq!(unknown.icon, "triangle-alert");
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K2"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
    assert_eq!(without(&verse.commands), vec![Command::Page("hf_bergbau".into())]);
}

#[test]
fn a_refused_command_shows_a_warning() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::RefineryScan, json!({})), &mut verse);
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    assert_eq!(events(&out, "showOk").len(), 1);
    verse.refuse = true;
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
}

#[test]
fn a_key_of_the_profiles_shows_its_short_name_symbol_and_colour_and_the_binding_below() {
    let (mut plugin, mut verse) = ready();
    let settings =
        json!({"aktion": "v_lights", "name": "Licht!", "symbol": "door-open", "farbe": "rot", "taste_anzeigen": true});
    plugin.on_event(&appear("K1", Action::GameAction, settings), &mut verse);
    let look = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!((look.label.as_str(), look.icon, look.colour), ("Licht!", "door-open", Some("rot")), "the profile wins");
    assert_eq!(look.badge.expect("badge").word, "Alt rechts + Y", "the key asked for stays below the short name");
    // What is unknown or empty stays as the action draws it: the long name of the game, the symbol and the colour
    // the profiles give this action.
    let wrong = json!({"aktion": "v_lights", "name": " ", "symbol": "gibt-es-nicht", "farbe": "warnung"});
    plugin.on_event(&appear("K2", Action::GameAction, wrong), &mut verse);
    let look = plugin.look(&plugin.keys["K2"].clone());
    assert_eq!((look.label.as_str(), look.icon, look.colour), ("Licht", "sun", Some("gelb")));
    // A key that cannot be pressed keeps its short name and says why in the word below.
    let settings = json!({"aktion": "v_toggle_mining_laser_type", "name": "Laser", "symbol": "zap", "farbe": "pink"});
    plugin.on_event(&appear("K3", Action::GameAction, settings), &mut verse);
    let look = plugin.look(&plugin.keys["K3"].clone());
    assert_eq!(look.label, "Laser");
    assert_eq!(look.badge.expect("badge").tone, Tone::Warning);
}

#[test]
fn every_symbol_and_colour_a_shipped_profile_names_is_one_the_plugin_draws() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    for entry in std::fs::read_dir(&folder).expect("profiles").flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("streamDeckProfile") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).expect("profile");
        for entry_in in versekit_core::zip::entries(&bytes).expect("zip") {
            if !entry_in.name.to_ascii_lowercase().ends_with("/manifest.json") {
                continue;
            }
            let page: Value = serde_json::from_slice(&versekit_core::zip::read(&bytes, &entry_in).expect("read")).unwrap_or_default();
            for controller in page["Controllers"].as_array().into_iter().flatten() {
                for action in controller["Actions"].as_object().into_iter().flat_map(|m| m.values()) {
                    if let Some(symbol) = action["Settings"]["symbol"].as_str() {
                        assert!(crate::render::known_icon(symbol).is_some(), "{:?}: symbol {symbol}", entry.file_name());
                    }
                    if let Some(colour) = action["Settings"]["farbe"].as_str() {
                        assert!(crate::paint::style().key_colour(colour).is_some(), "{:?}: farbe {colour}", entry.file_name());
                    }
                }
            }
        }
    }
}

#[test]
fn no_shipped_profile_presses_an_action_the_game_takes_no_key_for_or_the_helmet_of_empty_hands() {
    // The toggles of mining and salvage have keyboard="" (the game drops a keyboard line there); the old helmet works
    // only with empty hands. Mining and salvage press M, the helmet Left Alt + H.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    let mut wrong = Vec::new();
    let mut industrial = 0;
    for entry in std::fs::read_dir(&folder).expect("profiles").flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("streamDeckProfile") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).expect("profile");
        for entry_in in versekit_core::zip::entries(&bytes).expect("zip") {
            if !entry_in.name.to_ascii_lowercase().ends_with("/manifest.json") {
                continue;
            }
            let text = String::from_utf8_lossy(&versekit_core::zip::read(&bytes, &entry_in).expect("read")).into_owned();
            for old in ["\"v_toggle_mining_mode\"", "\"v_toggle_salvage_mode\"", "\"toggleEquipHelmet\""] {
                if text.contains(old) {
                    wrong.push(format!("{:?}: {old}", entry.file_name()));
                }
            }
            industrial += text.matches("\"v_cycle_industrial_mode\"").count();
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert!(industrial >= 24, "mining and salvage on every device and language press M: {industrial}");
}

#[test]
fn lock_target_stands_on_the_first_part_of_dogfight_on_the_devices_with_parts() {
    // „Ziel sperren“ stands on the first dogfight part of MK.2, Neo and Mini, „Raketen +“ behind it.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    for device in ["MK2", "Neo", "Mini"] {
        for lang in ["DE", "EN"] {
            let bytes = std::fs::read(folder.join(format!("VerseKit-StreamDeck-{device}-{lang}.streamDeckProfile"))).expect("profile");
            let mut pages = Vec::new();
            for entry in versekit_core::zip::entries(&bytes).expect("zip") {
                if !entry.name.to_ascii_lowercase().ends_with("/manifest.json") {
                    continue;
                }
                let page: Value = serde_json::from_slice(&versekit_core::zip::read(&bytes, &entry).expect("read")).unwrap_or_default();
                let locks = page["Controllers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|c| c["Actions"].as_object().into_iter().flat_map(|m| m.values()))
                    .any(|a| a["Settings"]["aktion"] == "v_target_lock_selected");
                if locks {
                    pages.push(page["Name"].as_str().unwrap_or("").to_string());
                }
            }
            assert_eq!(pages, ["Dogfight 1"], "{device} {lang}");
        }
    }
}

/// The game actions VerseKit was told to press, without the reports.
fn game_presses(verse: &FakeVerse) -> Vec<(String, Phase)> {
    verse
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::GameAction { action, phase } => Some((action.clone(), *phase)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_dangerous_game_action_goes_to_the_game_only_after_it_was_held_for_a_second() {
    let start = Instant::now();
    let (mut plugin, mut verse) = ready();
    plugin.fixed_now = Some(start);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights", "gefaehrlich": true})), &mut verse);
    let look = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!(look.badge.expect("badge").word, "HALTEN", "the key says it must be held");
    let mut pictures = events(&plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse), "setImage").len();
    assert!(game_presses(&verse).is_empty(), "nothing sent on the press");
    // The ring fills: a picture at most every 100 ms, nothing sent before the second.
    for ms in (25..1000).step_by(25) {
        plugin.fixed_now = Some(start + Duration::from_millis(ms));
        pictures += events(&plugin.tick(&mut verse), "setImage").len();
    }
    assert!(game_presses(&verse).is_empty(), "nothing sent before the second");
    assert!((5..=10).contains(&pictures), "{pictures} pictures in the first second");
    plugin.fixed_now = Some(start + HOLD);
    plugin.tick(&mut verse);
    assert_eq!(game_presses(&verse), vec![("v_lights".to_string(), Phase::Down)], "held for a second: pressed");
    plugin.on_event(&json!({"event": "keyUp", "context": "K1"}), &mut verse);
    assert_eq!(game_presses(&verse).last(), Some(&("v_lights".to_string(), Phase::Up)), "let go with the key");
    assert!(!plugin.busy());
}

#[test]
fn a_dangerous_game_action_let_go_too_early_sends_nothing_and_says_hold() {
    let start = Instant::now();
    let (mut plugin, mut verse) = ready();
    plugin.fixed_now = Some(start);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights", "gefaehrlich": true})), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    plugin.fixed_now = Some(start + Duration::from_millis(400));
    plugin.tick(&mut verse);
    let out = plugin.on_event(&json!({"event": "keyUp", "context": "K1"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
    assert_eq!(events(&out, "setImage").len(), 1, "the key says HALTEN in the colour of warnings");
    // Long after: still nothing sent, and the key is at rest again.
    for ms in (500..3000).step_by(100) {
        plugin.fixed_now = Some(start + Duration::from_millis(ms));
        plugin.tick(&mut verse);
    }
    assert!(game_presses(&verse).is_empty(), "{:?}", verse.commands);
    assert!(!plugin.busy(), "the hint is gone");
}

#[test]
fn the_press_of_a_dangerous_dial_waits_for_the_second_too_and_turning_does_not() {
    let start = Instant::now();
    let (mut plugin, mut verse) = ready();
    plugin.fixed_now = Some(start);
    let settings = json!({"aktion_links": "v_shield_down", "aktion_rechts": "v_shield_up", "aktion_druck": "v_lights", "gefaehrlich": true});
    plugin.on_event(&dial("R1", settings), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": 1}}), &mut verse);
    assert_eq!(game_presses(&verse).len(), 2, "turning goes at once");
    plugin.on_event(&json!({"event": "dialDown", "context": "R1"}), &mut verse);
    plugin.fixed_now = Some(start + Duration::from_millis(500));
    plugin.tick(&mut verse);
    assert_eq!(game_presses(&verse).len(), 2, "the press waits");
    plugin.fixed_now = Some(start + HOLD);
    plugin.tick(&mut verse);
    plugin.on_event(&json!({"event": "dialUp", "context": "R1"}), &mut verse);
    let presses = game_presses(&verse);
    assert_eq!(&presses[2..], [("v_lights".to_string(), Phase::Down), ("v_lights".to_string(), Phase::Up)]);
}

#[test]
fn a_game_action_without_a_look_of_its_own_takes_the_one_of_the_profiles_or_of_its_actionmap() {
    let (mut plugin, mut verse) = ready();
    verse.bindings["aktionen"].as_array_mut().expect("list").extend([
        json!({"id": "v_power_toggle_shields", "art": "tastatur", "taste": "F7", "name": "Schilde an/aus", "gruppe": "Energie", "karte": "spaceship_power"}),
        json!({"id": "v_power_preset_front", "art": "tastatur", "taste": "F8", "name": "Vorne", "gruppe": "Energie", "karte": "spaceship_power"}),
        json!({"id": "v_flycam", "art": "tastatur", "taste": "F9", "name": "Kamera", "gruppe": "Kamera", "karte": "flycam"}),
    ]);
    verse.set("belegung_stand", json!("b"));
    plugin.poll(&mut verse);
    let look_of = |plugin: &mut Plugin, verse: &mut FakeVerse, action: &str| {
        plugin.on_event(&appear("K1", Action::GameAction, json!({ "aktion": action })), verse);
        let look = plugin.look(&plugin.keys["K1"].clone());
        (look.icon, look.colour)
    };
    assert_eq!(look_of(&mut plugin, &mut verse, "v_power_toggle_shields"), ("shield-half", Some("blau")), "as in the profiles");
    assert_eq!(look_of(&mut plugin, &mut verse, "v_power_preset_front"), ("zap", Some("blau")), "by its actionmap");
    assert_eq!(look_of(&mut plugin, &mut verse, "v_flycam"), (Action::GameAction.icon(), None), "nothing known: as before");
    let dial = plugin.on_event(&dial("R1", json!({"aktion_links": "v_power_toggle_shields"})), &mut verse);
    assert_eq!(events(&dial, "setFeedback").len(), 1, "a dial is drawn with it too");
}

#[test]
fn eject_self_destruct_emergency_exit_and_all_power_wait_for_the_hold_unless_the_profile_says_no() {
    let start = Instant::now();
    let (mut plugin, mut verse) = ready();
    plugin.fixed_now = Some(start);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_self_destruct"})), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    assert!(game_presses(&verse).is_empty(), "dangerous by itself: nothing sent on the press");
    plugin.fixed_now = Some(start + HOLD);
    plugin.tick(&mut verse);
    plugin.on_event(&json!({"event": "keyUp", "context": "K1"}), &mut verse);
    assert_eq!(game_presses(&verse).len(), 2, "held for the second: down and up");
    verse.commands.clear();
    plugin.on_event(&appear("K2", Action::GameAction, json!({"aktion": "v_eject", "gefaehrlich": false})), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "K2"}), &mut verse);
    assert_eq!(game_presses(&verse), vec![("v_eject".to_string(), Phase::Down)], "the profile said no: at once");
    plugin.on_event(&json!({"event": "keyUp", "context": "K2"}), &mut verse);
    for action in ["v_eject", "v_emergency_exit", "v_power_toggle", "v_self_destruct"] {
        plugin.on_event(&appear("K3", Action::GameAction, json!({ "aktion": action })), &mut verse);
        assert!(Plugin::dangerous(&plugin.keys["K3"].clone()), "{action}");
    }
    plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down", "aktion_druck": "v_eject"})), &mut verse);
    assert!(Plugin::dangerous(&plugin.keys["R1"].clone()), "the push of a dial");
    plugin.on_event(&dial("R2", json!({"aktion_links": "v_eject"})), &mut verse);
    assert!(!Plugin::dangerous(&plugin.keys["R2"].clone()), "turning is never held");
}

#[test]
fn a_game_action_follows_the_key_down_and_up() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    let look = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!(look.label, "Licht");
    assert_eq!(look.badge, None, "a key that works shows only its name");
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    plugin.on_event(&json!({"event": "keyUp", "context": "K1"}), &mut verse);
    assert_eq!(
        without(&verse.commands),
        vec![
            Command::GameAction { action: "v_lights".into(), phase: Phase::Down },
            Command::GameAction { action: "v_lights".into(), phase: Phase::Up },
        ]
    );
    assert!(verse.commands.contains(&Command::GameActionsInUse(vec!["v_lights".into()])), "VerseKit learns the action");
}

#[test]
fn a_key_held_with_alt_gr_says_unsafe_in_the_colour_of_warnings_whether_the_key_is_shown_or_not() {
    let (mut plugin, mut verse) = ready();
    verse.bindings = json!({"aktionen": [
        {"id": "v_quantum", "art": "tastatur", "taste": "Alt rechts + B", "unsicher": true, "name": "Quantum", "gruppe": "Flug"},
        {"id": "v_gear", "art": "tastatur", "taste": "N", "name": "Fahrwerk", "gruppe": "Flug"}
    ]});
    verse.set("belegung_stand", json!("u"));
    plugin.poll(&mut verse);
    for shown in [false, true] {
        plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_quantum", "taste_anzeigen": shown})), &mut verse);
        let look = plugin.look(&plugin.keys["K1"].clone());
        let badge = look.badge.expect("badge");
        assert_eq!((badge.word.as_str(), badge.tone, look.icon_tone), ("Alt Gr unsicher", Tone::Warning, Tone::Warning));
    }
    plugin.on_event(&appear("K2", Action::GameAction, json!({"aktion": "v_gear"})), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["K2"].clone()).badge, None, "a safe key keeps the line empty");
}

#[test]
fn the_switch_of_verse_kit_shows_the_key_on_every_game_action_key_and_the_warnings_stay() {
    let (mut plugin, mut verse) = ready();
    verse.bindings = json!({"aktionen": [
        {"id": "v_gear", "art": "tastatur", "taste": "N", "name": "Fahrwerk", "gruppe": "Flug"},
        {"id": "v_lights", "art": "tastatur", "taste": "L", "name": "Licht", "gruppe": "Schiff"},
        {"id": "v_quantum", "art": "tastatur", "taste": "Alt rechts + B", "unsicher": true, "name": "Quantum", "gruppe": "Flug"},
        {"id": "v_free", "art": "nicht_belegt", "taste": "", "name": "Frei", "gruppe": "Flug"}
    ]});
    verse.set("belegung_stand", json!("t"));
    plugin.poll(&mut verse);
    for (context, action) in [("K1", "v_gear"), ("K2", "v_lights"), ("K3", "v_quantum"), ("K4", "v_free")] {
        plugin.on_event(&appear(context, Action::GameAction, json!({"aktion": action})), &mut verse);
    }
    let badge = |plugin: &Plugin, context: &str| plugin.look(&plugin.keys[context].clone()).badge.map(|b| b.word);
    // Off (the default): no key below the name.
    assert_eq!((badge(&plugin, "K1"), badge(&plugin, "K2")), (None, None));
    // On: every game action key shows its key; the warnings stay as they are.
    verse.set(streamdeck::FIELD_KEYS_SHOWN, json!(true));
    plugin.poll(&mut verse);
    assert_eq!(badge(&plugin, "K1").as_deref(), Some("N"));
    assert_eq!(badge(&plugin, "K2").as_deref(), Some("L"));
    assert_eq!(badge(&plugin, "K3").as_deref(), Some("Alt Gr unsicher"), "the warning of Alt Gr stays");
    assert_eq!(badge(&plugin, "K4").as_deref(), Some(language::t_in(Lang::De, "sd_ga_nicht_belegt").as_str()), "unbound stays");
    // Off again: only a key that asks for it itself.
    verse.set(streamdeck::FIELD_KEYS_SHOWN, json!(false));
    plugin.on_event(&appear("K5", Action::GameAction, json!({"aktion": "v_lights", "taste_anzeigen": true})), &mut verse);
    plugin.poll(&mut verse);
    assert_eq!((badge(&plugin, "K2"), badge(&plugin, "K5").as_deref()), (None, Some("L")));
}

#[test]
fn the_switch_to_show_the_keys_is_kept_and_holds_without_verse_kit_also_after_a_new_start() {
    let folder = std::env::temp_dir().join(format!("versekit-sd-tasten-anzeigen-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    let file = folder.join(versekit_core::streamdeck::KEY_TABLE_FILE);
    let mut plugin = Plugin::new("PLUGIN", Vec::new());
    let keys = std::sync::Arc::new(FakeKeys::default());
    plugin.table_file = Some(file.clone());
    plugin.local = Some(keys.clone());
    let mut verse = FakeVerse::new();
    verse.bindings = json!({"aktionen": [
        {"id": "v_toggle_guns_mode", "art": "tastatur", "taste": "Strg rechts + Num 4", "eingabe": "rctrl+np_4", "halten": "folgen", "name": "Waffenmodus umschalten", "karte": "seat_general"}
    ]});
    verse.set(streamdeck::FIELD_KEYS_SHOWN, json!(true));
    plugin.poll(&mut verse);
    plugin.on_event(&appear("G", Action::GameAction, json!({"aktion": "v_toggle_guns_mode"})), &mut verse);
    let badge = |plugin: &Plugin| plugin.look(&plugin.keys["G"].clone()).badge.map(|b| b.word);
    assert!(badge(&plugin).is_some(), "the key below the name");
    // VerseKit is closed: the line stays.
    verse.away = true;
    plugin.poll(&mut verse);
    assert!(badge(&plugin).is_some_and(|k| k.contains("4")), "without VerseKit: {:?}", badge(&plugin));
    // The plugin starts anew while VerseKit is away: the line stays as the switch was last.
    let mut again = Plugin::new("PLUGIN", Vec::new());
    again.table = crate::table::Table::load(Some(&file));
    again.table_file = Some(file.clone());
    again.local = Some(keys);
    again.poll(&mut verse);
    again.on_event(&appear("G", Action::GameAction, json!({"aktion": "v_toggle_guns_mode"})), &mut verse);
    assert!(badge(&again).is_some_and(|k| k.contains("4")), "after a new start without VerseKit: {:?}", badge(&again));
    // Switched off in VerseKit: kept too.
    verse.away = false;
    verse.set(streamdeck::FIELD_KEYS_SHOWN, json!(false));
    again.poll(&mut verse);
    assert!(!crate::table::Table::load(Some(&file)).keys_shown, "off is kept");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_game_action_off_the_keyboard_says_why_and_sends_nothing() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_toggle_mining_laser_type"})), &mut verse);
    plugin.on_event(&appear("K2", Action::GameAction, json!({"aktion": "gibt_es_nicht"})), &mut verse);
    let mouse = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!((mouse.icon, mouse.badge.expect("badge").word.as_str()), ("mouse", "Nur Maus"));
    let unknown = plugin.look(&plugin.keys["K2"].clone());
    assert_eq!(unknown.icon, "circle-question-mark");
    for context in ["K1", "K2"] {
        let out = plugin.on_event(&json!({"event": "keyDown", "context": context}), &mut verse);
        assert_eq!(events(&out, "showAlert").len(), 1, "{context}");
        plugin.on_event(&json!({"event": "keyUp", "context": context}), &mut verse);
    }
    // Nothing goes to the game. The key that lacks its binding on the keyboard opens the page where it is put in;
    // the action the game does not know opens nothing.
    assert_eq!(without(&verse.commands), vec![Command::Page(streamdeck::PAGE_KEYS.into())], "{:?}", verse.commands);
}

#[test]
fn a_key_without_a_binding_opens_the_page_of_the_keys_on_every_way_it_is_pressed() {
    let (mut plugin, mut verse) = ready();
    verse.bindings = json!({"aktionen": [
        {"id": "v_frei", "art": "nicht_belegt", "taste": "", "name": "Fadenkreuz", "gruppe": "Ziele"},
        {"id": "v_stick", "art": "nur_joystick", "taste": "", "name": "Raketen", "gruppe": "Waffen"}
    ]});
    verse.set("belegung_stand", json!("b"));
    plugin.poll(&mut verse);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_frei"})), &mut verse);
    plugin.on_event(&appear("K2", Action::GameAction, json!({"aktion": "v_stick", "umschalter": true})), &mut verse);
    plugin.on_event(&dial("R1", json!({"aktion_links": "v_frei", "aktion_rechts": "v_frei"})), &mut verse);
    let look = plugin.look(&plugin.keys["K1"].clone());
    let badge = look.badge.expect("badge");
    assert_eq!((look.icon, badge.word.as_str(), badge.tone), ("keyboard-off", "Nicht belegt", Tone::Warning));
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "K2"}), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": 1}}), &mut verse);
    let pages: Vec<Command> = without(&verse.commands);
    assert_eq!(pages, vec![Command::Page(streamdeck::PAGE_KEYS.into()); 3], "a key, a counted switch, a dial");
    assert!(streamdeck::page_allowed(streamdeck::PAGE_KEYS));
}

#[test]
fn a_game_action_is_blocked_while_verse_kit_runs_as_admin_and_the_key_says_why() {
    let (mut plugin, mut verse) = ready();
    verse.set("spielaktion", json!({"gesperrt": "admin"}));
    plugin.poll(&mut verse);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    let look = plugin.look(&plugin.keys["K1"].clone());
    assert_eq!((look.icon, look.badge.expect("badge").word.as_str()), ("shield-alert", "VerseKit als Admin"));
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
    assert!(without(&verse.commands).is_empty(), "the rights block it: no page of the keys");
}

#[test]
fn a_key_that_goes_away_while_held_lets_go() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "K1"}), &mut verse);
    plugin.on_event(&json!({"event": "willDisappear", "context": "K1"}), &mut verse);
    assert_eq!(
        without(&verse.commands).last(),
        Some(&Command::GameAction { action: "v_lights".into(), phase: Phase::Up })
    );
}

#[test]
fn a_dial_presses_its_left_and_right_action_once_per_step() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down", "aktion_rechts": "v_shield_up"})), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": 2}}), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": -1}}), &mut verse);
    let presses: Vec<(String, Phase)> = without(&verse.commands)
        .into_iter()
        .filter_map(|c| match c {
            Command::GameAction { action, phase } => Some((action, phase)),
            _ => None,
        })
        .collect();
    let up = |a: &str| (a.to_string(), Phase::Up);
    let down = |a: &str| (a.to_string(), Phase::Down);
    assert_eq!(
        presses,
        vec![down("v_shield_up"), up("v_shield_up"), down("v_shield_up"), up("v_shield_up"), down("v_shield_down"), up("v_shield_down")]
    );
    let out = plugin.on_event(&json!({"event": "didReceiveSettings", "context": "R1", "payload": {"settings": {"aktion_links": "v_shield_down", "aktion_rechts": "v_shield_up", "aktion_druck": "v_lights"}}}), &mut verse);
    assert_eq!(events(&out, "setFeedback").len(), 1, "a dial is drawn on the strip");
}

/// A dial with the clock of the test standing at `start`.
fn dial_at(start: Instant) -> (Plugin, FakeVerse) {
    let (mut plugin, mut verse) = ready();
    plugin.fixed_now = Some(start);
    plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down", "aktion_rechts": "v_shield_up"})), &mut verse);
    (plugin, verse)
}

#[test]
fn a_dial_is_one_picture_in_the_layout_of_the_plugin() {
    let (mut plugin, mut verse) = ready();
    let out = plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down"})), &mut verse);
    let feedback = events(&out, "setFeedback");
    assert_eq!(feedback.len(), 1);
    let picture = feedback[0]["payload"][crate::pack::DIAL_PICTURE].as_str().expect("picture");
    assert!(picture.starts_with("data:image/png;base64,"));
    assert!(feedback[0]["payload"]["title"].is_null(), "no title of the layout of Elgato");
    let layout = crate::pack::dial_layout();
    assert_eq!(layout["items"][0]["key"], crate::pack::DIAL_PICTURE);
    assert_eq!(layout["items"][0]["rect"], json!([0, 0, 200, 100]));
}

#[test]
fn a_turn_runs_through_its_pictures_once_and_comes_to_rest() {
    let start = Instant::now();
    let (mut plugin, mut verse) = dial_at(start);
    let out = plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": 1}}), &mut verse);
    assert_eq!(events(&out, "setFeedback").len(), 1, "the first picture at once");
    assert!(plugin.busy());
    let first = events(&out, "setFeedback")[0]["payload"].clone();
    let mut pictures = 1;
    let mut last = first.clone();
    for step in 1..=10u32 {
        plugin.fixed_now = Some(start + DIAL_FRAME * step);
        for feedback in events(&plugin.tick(&mut verse), "setFeedback") {
            pictures += 1;
            last = feedback["payload"].clone();
        }
    }
    assert_ne!(first, last, "lit first, at rest last");
    assert!(!plugin.busy(), "the series has ended");
    // The lit quarter for 300 ms (the same picture, sent once), then the dial at rest.
    assert_eq!(pictures, 2, "{pictures} pictures");
    plugin.fixed_now = Some(start + DIAL_FRAME * 20);
    assert!(plugin.tick(&mut verse).is_empty(), "at rest nothing is sent");
}

#[test]
fn a_dial_never_gets_more_than_ten_pictures_a_second_and_the_last_one_comes() {
    let start = Instant::now();
    let (mut plugin, mut verse) = dial_at(start);
    let mut sent: Vec<Instant> = vec![start];
    // Thirty turns within a second, each with its own series.
    for n in 0..30u32 {
        let now = start + Duration::from_millis(u64::from(n) * 33);
        plugin.fixed_now = Some(now);
        let turn = if n % 2 == 0 { 1 } else { -1 };
        let mut out = plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": turn}}), &mut verse);
        out.extend(plugin.tick(&mut verse));
        sent.extend(std::iter::repeat_n(now, events(&out, "setFeedback").len()));
    }
    for step in 0..40u32 {
        let now = start + Duration::from_millis(1000 + u64::from(step) * 50);
        plugin.fixed_now = Some(now);
        sent.extend(std::iter::repeat_n(now, events(&plugin.tick(&mut verse), "setFeedback").len()));
    }
    for (i, at) in sent.iter().enumerate() {
        let within = sent[i..].iter().filter(|later| later.duration_since(*at) < Duration::from_secs(1)).count();
        assert!(within <= DIAL_RATE, "{within} pictures within a second from picture {i}");
    }
    assert!(!plugin.busy(), "everything that waited was sent in the end");
}

#[test]
fn the_objective_dial_goes_through_the_running_contracts_and_opens_the_contract_log() {
    let (mut plugin, mut verse) = ready();
    let appear = json!({"event": "willAppear", "action": Action::Objective.uuid(), "context": "A1", "device": "D1",
                        "payload": {"settings": {}, "controller": "Encoder"}});
    let empty = plugin.on_event(&appear, &mut verse);
    assert_eq!(events(&empty, "setFeedback").len(), 1, "drawn without a contract too");
    verse.set(
        streamdeck::FIELD_CONTRACTS,
        json!([
            {"titel": "Fracht", "schritt": "Fracht abholen bei Everus Harbour", "erledigt": 1, "von": 3},
            {"titel": "Jagd", "schritt": null, "erledigt": null, "von": null}
        ]),
    );
    let first = plugin.poll(&mut verse);
    let picture = |out: &[Value]| events(out, "setFeedback")[0]["payload"][crate::pack::DIAL_PICTURE].clone();
    let fracht = picture(&first);
    assert_ne!(fracht, picture(&empty), "the contract shows");
    let turned = plugin.on_event(&json!({"event": "dialRotate", "context": "A1", "payload": {"ticks": 1}}), &mut verse);
    let jagd = picture(&turned);
    assert_ne!(jagd, fracht, "the next contract");
    let back = plugin.on_event(&json!({"event": "dialRotate", "context": "A1", "payload": {"ticks": 1}}), &mut verse);
    assert_eq!(picture(&back), fracht, "round at the end");
    assert!(without(&verse.commands).is_empty(), "turning presses nothing in the game");
    plugin.on_event(&json!({"event": "dialDown", "context": "A1"}), &mut verse);
    assert_eq!(without(&verse.commands), vec![Command::Page(PAGE_CONTRACT_LOG.into())]);
    assert!(streamdeck::page_allowed(PAGE_CONTRACT_LOG));
    // The second contract is gone: the dial stands at the one that is left.
    verse.set(streamdeck::FIELD_CONTRACTS, json!([{"titel": "Fracht", "schritt": "Fracht abholen bei Everus Harbour", "erledigt": 1, "von": 3}]));
    plugin.on_event(&json!({"event": "dialRotate", "context": "A1", "payload": {"ticks": 1}}), &mut verse);
    let left = plugin.poll(&mut verse);
    assert!(events(&left, "setFeedback").iter().all(|f| f["payload"][crate::pack::DIAL_PICTURE] == fracht));
}

fn info_dial(context: &str, action: Action) -> Value {
    json!({"event": "willAppear", "action": action.uuid(), "context": context, "device": "D1",
           "payload": {"settings": {}, "controller": "Encoder"}})
}

#[test]
fn the_contested_dial_counts_down_live_and_opens_the_contested_page() {
    let (mut plugin, mut verse) = ready();
    let picture = |out: &[Value]| events(out, "setFeedback")[0]["payload"][crate::pack::DIAL_PICTURE].clone();
    let none = picture(&plugin.on_event(&info_dial("C1", Action::Contested), &mut verse));
    assert_eq!(plugin.info_look(Action::Contested).value, "—", "no cycle known");
    verse.set("contested", json!({"offen": true, "sekunden": 725}));
    let open = picture(&plugin.poll(&mut verse));
    assert_ne!(open, none);
    let look = plugin.info_look(Action::Contested);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("12:05", "OFFEN", Tone::Accent));
    verse.set("contested", json!({"offen": true, "sekunden": 724}));
    let later = plugin.poll(&mut verse);
    assert_eq!(events(&later, "setFeedback").len(), 1, "every second a new picture");
    assert_ne!(picture(&later), open);
    verse.set("contested", json!({"offen": false, "sekunden": 3725}));
    plugin.poll(&mut verse);
    let look = plugin.info_look(Action::Contested);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("1:02:05", "ZU", Tone::Muted));
    plugin.on_event(&json!({"event": "dialDown", "context": "C1"}), &mut verse);
    assert_eq!(without(&verse.commands), vec![Command::Page(streamdeck::PAGE_CONTESTED.into())]);
}

#[test]
fn the_status_dial_says_game_log_and_keys_and_opens_the_page_of_the_keys() {
    let (mut plugin, mut verse) = ready();
    let picture = |out: &[Value]| events(out, "setFeedback")[0]["payload"][crate::pack::DIAL_PICTURE].clone();
    plugin.on_event(&info_dial("S1", Action::Status), &mut verse);
    assert_eq!(plugin.info_look(Action::Status).value, "—", "VerseKit gives no readiness");
    let ready_now = streamdeck::Readiness { game_runs: true, log_found: Some(true), keys_missing: 0 };
    verse.set(streamdeck::FIELD_STATUS, ready_now.value());
    let all_good = picture(&plugin.poll(&mut verse));
    let look = plugin.info_look(Action::Status);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("Spiel läuft", "Log ok · Tasten belegt", Tone::Accent));
    let trouble = streamdeck::Readiness { game_runs: false, log_found: Some(false), keys_missing: 3 };
    verse.set(streamdeck::FIELD_STATUS, trouble.value());
    assert_ne!(picture(&plugin.poll(&mut verse)), all_good);
    let look = plugin.info_look(Action::Status);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("Spiel aus", "Log fehlt · 3 Tasten fehlen", Tone::Warning));
    verse.set("sprache", json!("en"));
    plugin.poll(&mut verse);
    assert_eq!(plugin.info_look(Action::Status).line, "Log missing · 3 keys missing");
    // One key: the singular.
    verse.set(streamdeck::FIELD_STATUS, streamdeck::Readiness { keys_missing: 1, log_found: Some(true), ..trouble }.value());
    plugin.poll(&mut verse);
    assert_eq!(plugin.info_look(Action::Status).line, "1 key missing");
    verse.set("sprache", json!("de"));
    plugin.poll(&mut verse);
    assert_eq!(plugin.info_look(Action::Status).line, "1 Taste fehlt");
    plugin.on_event(&json!({"event": "dialRotate", "context": "S1", "payload": {"ticks": 1}}), &mut verse);
    assert!(without(&verse.commands).is_empty(), "turning does nothing");
    plugin.on_event(&json!({"event": "dialDown", "context": "S1"}), &mut verse);
    assert_eq!(without(&verse.commands), vec![Command::Page(streamdeck::PAGE_KEYS.into())]);
    assert!(streamdeck::page_allowed(streamdeck::PAGE_KEYS));
}

#[test]
fn the_wallet_dial_makes_nothing_up_and_the_ship_dial_shows_the_last_ship() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&info_dial("K1", Action::Wallet), &mut verse);
    let look = plugin.info_look(Action::Wallet);
    assert_eq!((look.value.as_str(), look.line.as_str()), ("—", "ab SC 5.0"), "no money without telemetry");
    let picture = |out: &[Value]| events(out, "setFeedback")[0]["payload"][crate::pack::DIAL_PICTURE].clone();
    let none = picture(&plugin.on_event(&info_dial("F1", Action::Ship), &mut verse));
    let look = plugin.info_look(Action::Ship);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("—", "keins in dieser Sitzung", Tone::Muted));
    verse.set(streamdeck::FIELD_SHIP, streamdeck::ship_value(Some("Aegis Sabre")));
    let out = plugin.poll(&mut verse);
    let sabre = events(&out, "setFeedback").iter().find(|f| f["context"] == "F1").map(|f| f["payload"][crate::pack::DIAL_PICTURE].clone());
    assert_ne!(sabre, Some(none), "the ship shows");
    let look = plugin.info_look(Action::Ship);
    assert_eq!((look.value.as_str(), look.line.as_str(), look.tone), ("Aegis Sabre", "zuletzt geflogen", Tone::Accent));
    plugin.on_event(&json!({"event": "dialDown", "context": "F1"}), &mut verse);
    plugin.on_event(&json!({"event": "dialDown", "context": "K1"}), &mut verse);
    assert_eq!(
        without(&verse.commands),
        vec![Command::Page(streamdeck::PAGE_SHIPS.into()), Command::Page(streamdeck::PAGE_FINANCES.into())]
    );
}

#[test]
fn the_health_shows_its_per_cent_in_green_yellow_or_red_and_without_telemetry_makes_nothing_up() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&info_dial("G1", Action::Health), &mut verse);
    plugin.on_event(&appear("G2", Action::Health, json!({})), &mut verse);
    let dial = plugin.info_look(Action::Health);
    assert_eq!((dial.value.as_str(), dial.line.as_str(), dial.tone), ("—", "ab SC 5.0", Tone::Muted), "no telemetry");
    assert_eq!(plugin.look(&plugin.keys["G2"].clone()).badge.expect("badge").word, "ab SC 5.0");
    for (current, value, line, tone) in [
        (87.0, "87 %", "gut", Tone::Accent),
        (45.0, "45 %", "angeschlagen", Tone::Warning),
        (12.0, "12 %", "kritisch", Tone::Error),
    ] {
        verse.set(streamdeck::FIELD_HEALTH, streamdeck::health_value(Some((current, 100.0))));
        plugin.poll(&mut verse);
        let dial = plugin.info_look(Action::Health);
        assert_eq!((dial.value.as_str(), dial.line.as_str(), dial.tone), (value, line, tone));
        let badge = plugin.look(&plugin.keys["G2"].clone()).badge.expect("badge");
        assert_eq!((badge.word.as_str(), badge.tone), (value, tone), "the key in the same colour");
    }
    // Only a display: a press sends nothing, to VerseKit or the game, and no alert.
    let before = verse.commands.len();
    let pressed = plugin.on_event(&json!({"event": "keyDown", "context": "G2"}), &mut verse);
    plugin.on_event(&json!({"event": "keyUp", "context": "G2"}), &mut verse);
    plugin.on_event(&json!({"event": "dialDown", "context": "G1"}), &mut verse);
    assert_eq!(verse.commands.len(), before, "{:?}", verse.commands);
    assert!(events(&pressed, "showAlert").is_empty());
    assert_eq!(crate::plugin::health_level(60).0, "sd_info_gesund");
    assert_eq!(crate::plugin::health_level(30).0, "sd_info_angeschlagen");
    assert_eq!(crate::plugin::health_level(29).0, "sd_info_kritisch");
}

#[test]
fn a_dial_named_as_energy_in_the_profile_shows_the_power_symbol() {
    let (mut plugin, mut verse) = ready();
    let plain = plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down"})), &mut verse);
    let energy = plugin.on_event(&dial("R2", json!({"aktion_links": "v_shield_down", "energie": true})), &mut verse);
    let picture = |out: &[Value]| events(out, "setFeedback")[0]["payload"][crate::pack::DIAL_PICTURE].clone();
    assert_ne!(picture(&plain), picture(&energy));
}

#[test]
fn the_compact_setting_of_verse_kit_draws_the_dials_anew() {
    let (mut plugin, mut verse) = ready();
    let first = plugin.on_event(&dial("R1", json!({"aktion_links": "v_shield_down"})), &mut verse);
    verse.set(streamdeck::FIELD_DIALS, json!({"kompakt": true}));
    let out = plugin.poll(&mut verse);
    let (before, after) = (events(&first, "setFeedback"), events(&out, "setFeedback"));
    assert_eq!(after.len(), 1, "drawn anew");
    assert_ne!(before[0]["payload"], after[0]["payload"], "another picture");
    verse.set("overlay", json!(true));
    assert!(events(&plugin.poll(&mut verse), "setFeedback").is_empty(), "the same picture is not sent again");
}

#[test]
fn the_profile_in_the_language_of_verse_kit_is_offered_once_and_a_device_without_one_is_told() {
    let mut plugin =
        Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE".into(), "VerseKit-StreamDeck-Mobile-EN".into()]);
    let mut verse = FakeVerse::new();
    verse.set("sprache", json!("en"));
    plugin.poll(&mut verse);
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "IPAD", "deviceInfo": {"type": 3, "name": "iPad"}}), &mut verse);
    let out = plugin.on_event(&json!({"event": "didReceiveGlobalSettings", "payload": {"settings": {}}}), &mut verse);
    let switch = events(&out, "switchToProfile");
    assert_eq!(switch.len(), 1);
    // The name as the manifest gives it: the path without the extension.
    assert_eq!(switch[0]["payload"]["profile"], "profiles/VerseKit-StreamDeck-Mobile-EN");
    // The log of the plugin says when the profile was offered.
    assert!(events(&out, "logMessage").iter().any(|m| m["payload"]["message"].as_str().is_some_and(|l| l.starts_with("switchToProfile"))));
    assert_eq!(events(&out, "setGlobalSettings")[0]["payload"]["profil_angeboten"], json!(["IPAD"]));
    // Once only.
    let again = plugin.on_event(&json!({"event": "deviceDidConnect", "device": "IPAD", "deviceInfo": {"type": 3, "name": "iPad"}}), &mut verse);
    assert!(events(&again, "switchToProfile").is_empty());
    // A device kind without a profile is told to VerseKit.
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "PED", "deviceInfo": {"type": 5, "name": "Stream Deck Pedal"}}), &mut verse);
    assert!(verse.commands.contains(&Command::DeviceHint(Some("Stream Deck Pedal".into()))));
}

#[test]
fn a_virtual_deck_gets_no_profile_and_no_hint() {
    // As the Stream Deck program reports the deck of a tool: kind 14, „MCP Deck“; beside it a real XL.
    let mut plugin = Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-XL-DE".into()]);
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    plugin.on_event(&json!({"event": "didReceiveGlobalSettings", "payload": {"settings": {}}}), &mut verse);
    let out = plugin.on_event(&json!({"event": "deviceDidConnect", "device": "MCP", "deviceInfo": {"type": 14, "name": "MCP Deck"}}), &mut verse);
    assert!(events(&out, "switchToProfile").is_empty(), "no profile for the virtual deck");
    assert!(!verse.commands.contains(&Command::DeviceHint(Some("MCP Deck".into()))), "no hint about it either");
    let out = plugin.on_event(&json!({"event": "deviceDidConnect", "device": "XL", "deviceInfo": {"type": 2, "name": "Stream Deck XL"}}), &mut verse);
    assert_eq!(events(&out, "switchToProfile").len(), 1, "the real device beside it gets its profile");
    assert!(!verse.commands.iter().any(|c| matches!(c, Command::DeviceHint(Some(_)))), "{:?}", verse.commands);
}

#[test]
fn the_property_inspector_gets_pages_actions_and_texts_in_the_language_of_verse_kit() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    let out = plugin.on_event(&json!({"event": "propertyInspectorDidAppear", "context": "K1"}), &mut verse);
    let payload = &events(&out, "sendToPropertyInspector")[0]["payload"];
    assert_eq!(payload["sprache"], "de");
    assert_eq!(payload["verbunden"], true);
    assert_eq!(payload["seiten"].as_array().expect("pages").len(), streamdeck::PAGES.len());
    assert_eq!(payload["aktionen"].as_array().expect("actions").len(), 8);
    assert_eq!(payload["texte"]["sd_ga_nur_maus"], "Nur Maus");
    let out = plugin.on_event(&json!({"event": "sendToPlugin", "context": "K1", "payload": {"frage": "steuerung"}}), &mut verse);
    assert!(out.is_empty());
    assert!(verse.commands.contains(&Command::Page(streamdeck::PAGE_CONTROLS.into())));
}

#[test]
fn the_choice_marks_what_the_deck_cannot_press_and_an_emote_key_chooses_among_the_emotes() {
    let (mut plugin, mut verse) = ready();
    let payload = |plugin: &mut Plugin, verse: &mut FakeVerse, context: &str| {
        let out = plugin.on_event(&json!({"event": "propertyInspectorDidAppear", "context": context}), verse);
        events(&out, "sendToPropertyInspector")[0]["payload"].clone()
    };
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    let all = payload(&mut plugin, &mut verse, "K1");
    let entry = |list: &Value, id: &str| list["aktionen"].as_array().expect("list").iter().find(|a| a["id"] == id).cloned();
    let laser = entry(&all, "v_toggle_mining_laser_type").expect("in the list");
    assert_eq!((laser["sendbar"].clone(), laser["hinweis"].clone()), (json!(false), json!("Nur Maus")));
    let lights = entry(&all, "v_lights").expect("in the list");
    assert_eq!((lights["sendbar"].clone(), lights["hinweis"].clone()), (json!(true), json!("")));
    assert_eq!(all["texte"]["sd_pi_nur_sendbar"], "Nur Aktionen, die das Stream Deck drücken kann");
    // Without the actionmaps of the game an emote key chooses among all actions, as before.
    plugin.on_event(&appear("E1", Action::Emote, json!({"aktion": "v_lights"})), &mut verse);
    let before = payload(&mut plugin, &mut verse, "E1");
    assert_eq!(before["aktionen"].as_array().expect("list").len(), all["aktionen"].as_array().expect("list").len());
    // With them: the emotes, and the one the key has.
    verse.bindings["aktionen"].as_array_mut().expect("list").extend([
        json!({"id": "emote_wave", "art": "tastatur", "taste": "", "name": "Winken", "gruppe": "Emotes", "karte": "player_emotes"}),
        json!({"id": "emote_bow", "art": "nicht_belegt", "taste": "", "name": "Verbeugen", "gruppe": "Emotes", "karte": "player_emotes"}),
        json!({"id": "v_horn", "art": "tastatur", "taste": "H", "name": "Hupe", "gruppe": "Schiff", "karte": "seat_general"}),
    ]);
    verse.set("belegung_stand", json!("b"));
    plugin.poll(&mut verse);
    let emotes = payload(&mut plugin, &mut verse, "E1");
    let ids: Vec<&str> = emotes["aktionen"].as_array().expect("list").iter().filter_map(|a| a["id"].as_str()).collect();
    assert_eq!(ids, ["emote_bow", "emote_wave", "v_lights"], "the emotes and the one chosen");
    assert!(entry(&payload(&mut plugin, &mut verse, "K1"), "v_horn").is_some(), "a game action key still has all");
}

#[test]
fn the_log_tells_which_game_action_went_out_how_with_which_keys_and_how_long() {
    let (mut plugin, mut verse) = ready();
    verse.bindings["aktionen"].as_array_mut().expect("list").extend([
        json!({"id": "emote_bow", "art": "tastatur", "taste": "Strg rechts + E", "eingabe": "rctrl+e", "name": "Verbeugen", "gruppe": "Emotes", "karte": "player_emotes"}),
        json!({"id": "emote_sleep", "art": "nicht_belegt", "taste": "", "name": "Schlafen", "gruppe": "Emotes", "karte": "player_emotes"}),
    ]);
    verse.set("belegung_stand", json!("b"));
    plugin.poll(&mut verse);
    let lines = |out: &[Value]| -> Vec<String> {
        events(out, "logMessage").iter().filter_map(|m| m["payload"]["message"].as_str().map(String::from)).collect()
    };
    plugin.on_event(&appear("B1", Action::Emote, json!({"aktion": "emote_bow", "name": "Verbeugen"})), &mut verse);
    let start = Instant::now();
    plugin.fixed_now = Some(start);
    let down = lines(&plugin.on_event(&json!({"event": "keyDown", "context": "B1"}), &mut verse));
    assert!(down.iter().any(|l| l.starts_with("Spielaktion emote_bow gedrückt (über VerseKit, ")), "{down:?}");
    plugin.fixed_now = Some(start + Duration::from_millis(120));
    let up = lines(&plugin.on_event(&json!({"event": "keyUp", "context": "B1"}), &mut verse));
    assert!(up.contains(&"Spielaktion emote_bow losgelassen (nach 120 ms)".to_string()), "{up:?}");
    // A key the game has no keyboard key for says so instead of a press.
    plugin.on_event(&appear("S1", Action::Emote, json!({"aktion": "emote_sleep", "name": "Schlafen"})), &mut verse);
    let refused = lines(&plugin.on_event(&json!({"event": "keyDown", "context": "S1"}), &mut verse));
    assert!(refused.iter().any(|l| l.starts_with("Spielaktion emote_sleep nicht gesendet: ")), "{refused:?}");
    assert!(!refused.iter().any(|l| l.contains("gedrückt")), "{refused:?}");
}

#[test]
fn an_emote_sits_on_a_dial_too_left_right_and_push() {
    let (mut plugin, mut verse) = ready();
    verse.bindings["aktionen"].as_array_mut().expect("list").extend([
        json!({"id": "emote_wave", "art": "tastatur", "taste": "F1", "name": "Winken", "gruppe": "Emotes", "karte": "player_emotes"}),
        json!({"id": "emote_bow", "art": "tastatur", "taste": "F2", "name": "Verbeugen", "gruppe": "Emotes", "karte": "player_emotes"}),
        json!({"id": "emote_cheer", "art": "tastatur", "taste": "F3", "name": "Jubeln", "gruppe": "Emotes", "karte": "player_emotes"}),
    ]);
    verse.set("belegung_stand", json!("b"));
    plugin.poll(&mut verse);
    let appear = json!({"event": "willAppear", "action": Action::Emote.uuid(), "context": "R1", "device": "D1",
                        "payload": {"settings": {"aktion_links": "emote_wave", "aktion_rechts": "emote_bow", "aktion_druck": "emote_cheer"},
                                    "controller": "Encoder"}});
    let drawn = plugin.on_event(&appear, &mut verse);
    assert_eq!(events(&drawn, "setFeedback").len(), 1, "drawn as a dial");
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": -1}}), &mut verse);
    plugin.on_event(&json!({"event": "dialRotate", "context": "R1", "payload": {"ticks": 1}}), &mut verse);
    plugin.on_event(&json!({"event": "dialDown", "context": "R1"}), &mut verse);
    plugin.on_event(&json!({"event": "dialUp", "context": "R1"}), &mut verse);
    let sent: Vec<String> = game_presses(&verse).into_iter().filter(|(_, p)| *p == Phase::Down).map(|(a, _)| a).collect();
    assert_eq!(sent, ["emote_wave", "emote_bow", "emote_cheer"]);
    let look = plugin.look(&plugin.keys["R1"].clone());
    assert_eq!((look.icon, look.colour), ("hand", Some("charakter")), "the look of the emote to the left, as in the profiles");
}

#[test]
fn every_action_draws_in_both_languages_with_the_name_fitting() {
    for lang in ["de", "en"] {
        let (mut plugin, mut verse) = ready();
        verse.set("sprache", json!(lang));
        plugin.poll(&mut verse);
        for (i, action) in Action::ALL.iter().enumerate() {
            plugin.on_event(&appear(&format!("K{i}"), *action, json!({})), &mut verse);
            let look = plugin.look(&plugin.keys[&format!("K{i}")].clone());
            assert!(crate::paint::lines_of(&look.label, true).is_some(), "{lang} {action:?}: {}", look.label);
            assert!(render::icon_known(look.icon), "{action:?}: {}", look.icon);
        }
    }
}

#[test]
fn every_state_of_a_key_differs_without_color() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    let mut pictures = Vec::new();
    for kind in ["tastatur", "nur_maus", "nur_joystick", "nur_gamepad", "nicht_belegt", "nicht_sendbar", "unbekannt"] {
        verse.bindings["aktionen"][0]["art"] = json!(kind);
        verse.set("belegung_stand", json!(kind));
        plugin.poll(&mut verse);
        pictures.push(crate::paint::grey(&crate::paint::png(&plugin.look(&plugin.keys["K1"].clone()), Action::GameAction)));
    }
    let mut unique = pictures.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), pictures.len(), "two states look the same in grey");
}

fn game_commands(verse: &FakeVerse) -> Vec<(String, Phase)> {
    without(&verse.commands)
        .into_iter()
        .filter_map(|c| match c {
            Command::GameAction { action, phase } => Some((action, phase)),
            _ => None,
        })
        .collect()
}

fn press_for(plugin: &mut Plugin, verse: &mut FakeVerse, context: &str, held: Duration) -> Vec<Value> {
    let start = Instant::now();
    plugin.fixed_now = Some(start);
    plugin.on_event(&json!({"event": "keyDown", "context": context}), verse);
    plugin.fixed_now = Some(start + held);
    plugin.on_event(&json!({"event": "keyUp", "context": context}), verse)
}

/// A plugin with the shipped iPad profiles and the iPad connected.
fn ready_with_ipad() -> (Plugin, FakeVerse) {
    let mut plugin =
        Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE".into(), "VerseKit-StreamDeck-Mobile-EN".into()]);
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "D1", "deviceInfo": {"type": 3, "name": "iPad"}}), &mut verse);
    // The iPad shows the profile of the plugin: a key of it is there.
    plugin.on_event(&appear("K0", Action::Overlay, json!({})), &mut verse);
    (plugin, verse)
}

/// The page names of the shipped iPad profiles, as [`profile_pages`] reads them.
fn ipad_pages(plugin: &mut Plugin) {
    let de = ["", "Bergbau", "Bergung", "Dogfight", "Flug", "Zu Fuß", "Fahrzeug", "Emotes"];
    let en = ["VerseKit", "Mining", "Salvage", "Dogfight", "Flight", "On Foot", "Vehicle", "Emotes"];
    plugin.pages.insert("VerseKit-StreamDeck-Mobile-DE".into(), de.iter().map(|s| (*s).to_string()).collect());
    plugin.pages.insert("VerseKit-StreamDeck-Mobile-EN".into(), en.iter().map(|s| (*s).to_string()).collect());
}

fn wish(nr: u64) -> Value {
    streamdeck::device_page_value(Some((nr, streamdeck::DevicePage::OnFoot)))
}

#[test]
fn a_new_wish_of_verse_kit_switches_the_device_to_the_page_on_foot_once() {
    let (mut plugin, mut verse) = ready_with_ipad();
    ipad_pages(&mut plugin);
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(1));
    let out = plugin.poll(&mut verse);
    let switch = events(&out, "switchToProfile");
    assert_eq!(switch.len(), 1);
    assert_eq!(switch[0]["device"], "D1");
    assert_eq!(switch[0]["payload"], json!({"profile": "profiles/VerseKit-StreamDeck-Mobile-DE", "page": 5}));
    // Another field changes, the wish stays: no switch again.
    verse.set("overlay", json!(true));
    assert!(events(&plugin.poll(&mut verse), "switchToProfile").is_empty(), "never twice for one wish");
    // The next wish, in English: the page of the English profile.
    verse.set("sprache", json!("en"));
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(2));
    let out = plugin.poll(&mut verse);
    assert_eq!(events(&out, "switchToProfile")[0]["payload"], json!({"profile": "profiles/VerseKit-StreamDeck-Mobile-EN", "page": 5}));
}

#[test]
fn the_wish_verse_kit_holds_when_it_is_reached_anew_is_passed_over() {
    let (mut plugin, mut verse) = ready_with_ipad();
    ipad_pages(&mut plugin);
    verse.away = true;
    plugin.poll(&mut verse);
    // While VerseKit did not answer, the player left the seat long ago: no switch when it answers again.
    verse.away = false;
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(7));
    assert!(events(&plugin.poll(&mut verse), "switchToProfile").is_empty());
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(8));
    assert_eq!(events(&plugin.poll(&mut verse), "switchToProfile").len(), 1, "the next one switches");
}

#[test]
fn a_wish_switches_nothing_without_a_shipped_profile_or_without_its_page() {
    let (mut plugin, mut verse) = ready_with_ipad();
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(1));
    assert!(events(&plugin.poll(&mut verse), "switchToProfile").is_empty(), "the page names are unknown");
    plugin.pages.insert("VerseKit-StreamDeck-Mobile-DE".into(), vec!["Start".into(), "Flug".into()]);
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(2));
    assert!(events(&plugin.poll(&mut verse), "switchToProfile").is_empty(), "the profile has no page on foot");
    let (mut bare, mut verse) = ready();
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(1));
    assert!(events(&bare.poll(&mut verse), "switchToProfile").is_empty(), "no device, no switch");
}

#[test]
fn every_shipped_profile_has_a_page_on_foot_the_plugin_finds_by_its_name() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    let mut read = 0;
    for entry in std::fs::read_dir(&folder).expect("profiles").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".streamDeckProfile") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).expect("profile");
        let pages = profile_pages(&bytes);
        assert!(pages.len() > 1, "{name}: {pages:?}");
        let number = streamdeck::DevicePage::OnFoot.number_in(&pages);
        assert!(number.is_some(), "{name} has no page on foot: {pages:?}");
        // As shipped: the same pages once the profile has UUIDs of its own.
        let shipped = crate::pack::with_own_uuids(&bytes).expect("shipped");
        assert_eq!(profile_pages(&shipped), pages, "{name}");
        read += 1;
    }
    assert!(read >= 2, "profiles were read");
    let plus_xl = std::fs::read(folder.join("VerseKit-StreamDeck-PlusXL-DE.streamDeckProfile")).expect("profile");
    assert_eq!(streamdeck::DevicePage::OnFoot.number_in(&profile_pages(&plus_xl)), Some(5));
    assert!(profile_pages(b"kein zip").is_empty());
}

#[test]
fn an_emote_tapped_goes_back_to_its_page_and_held_stays() {
    let (mut plugin, mut verse) = ready_with_ipad();
    plugin.on_event(&appear("E1", Action::Emote, json!({"aktion": "v_lights", "zurueck_seite": 5})), &mut verse);
    let tap = press_for(&mut plugin, &mut verse, "E1", Duration::from_millis(120));
    let switch = events(&tap, "switchToProfile");
    assert_eq!(switch.len(), 1, "a tap goes back");
    assert_eq!(switch[0]["device"], "D1");
    assert_eq!(switch[0]["payload"]["profile"], "profiles/VerseKit-StreamDeck-Mobile-DE");
    assert_eq!(switch[0]["payload"]["page"], 5);
    let hold = press_for(&mut plugin, &mut verse, "E1", EMOTE_TAP + Duration::from_millis(50));
    assert!(events(&hold, "switchToProfile").is_empty(), "a hold stays");
    // Both times the emote went to the game, pressed and let go.
    let down = ("v_lights".to_string(), Phase::Down);
    let up = ("v_lights".to_string(), Phase::Up);
    assert_eq!(game_commands(&verse), vec![down.clone(), up.clone(), down, up]);
    assert!(verse.commands.contains(&Command::GameActionsInUse(vec!["v_lights".into()])), "VerseKit learns the emote");
}

#[test]
fn an_emote_goes_back_only_on_its_own_device_and_an_unlinked_profile_is_asked_once_only() {
    let (mut plugin, mut verse) = ready_with_ipad();
    // A second device of the same kind shows another profile: no key of the plugin appears there.
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "D2", "deviceInfo": {"type": 3, "name": "iPad 2"}}), &mut verse);
    ipad_pages(&mut plugin);
    plugin.on_event(&appear("E1", Action::Emote, json!({"aktion": "v_lights", "zurueck_seite": 5})), &mut verse);
    // Linked: the tap sends the emote, the device goes back, the key of the page appears.
    let start = Instant::now();
    plugin.fixed_now = Some(start);
    let tap = press_for(&mut plugin, &mut verse, "E1", Duration::from_millis(120));
    let switch = events(&tap, "switchToProfile");
    assert_eq!((switch.len(), switch[0]["device"].as_str()), (1, Some("D1")), "only the device it was pressed on");
    plugin.on_event(&appear("P5", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    plugin.fixed_now = Some(start + SWITCH_WAIT * 2);
    plugin.tick(&mut verse);
    assert!(plugin.unlinked.is_empty(), "the switch was seen");
    // A wish of VerseKit goes to the device that shows the plugin, never to the one that shows another profile.
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(1));
    let out = plugin.poll(&mut verse);
    let devices: Vec<&str> = events(&out, "switchToProfile").iter().filter_map(|s| s["device"].as_str()).collect();
    assert_eq!(devices, ["D1"]);
    // Unlinked: the program asks to install the profile instead and shows no key; after the wait no switch goes
    // there again, while the emote still goes to the game.
    let tap = press_for(&mut plugin, &mut verse, "E1", Duration::from_millis(120));
    assert_eq!(events(&tap, "switchToProfile").len(), 1);
    let later = plugin.fixed_now.expect("now") + SWITCH_WAIT + Duration::from_millis(10);
    plugin.fixed_now = Some(later);
    plugin.tick(&mut verse);
    assert_eq!(plugin.unlinked, ["D1"]);
    let commands_before = game_commands(&verse).len();
    let again = press_for(&mut plugin, &mut verse, "E1", Duration::from_millis(120));
    assert!(events(&again, "switchToProfile").is_empty(), "no second question of the Stream Deck program");
    assert_eq!(game_commands(&verse).len(), commands_before + 2, "the emote went to the game");
    verse.set(streamdeck::FIELD_DEVICE_PAGE, wish(2));
    assert!(events(&plugin.poll(&mut verse), "switchToProfile").is_empty(), "no wish goes there either");
}

#[test]
fn a_key_of_an_older_profile_gets_its_new_game_action_where_it_appears_and_one_of_the_player_stays() {
    let (mut plugin, mut verse) = ready();
    let old = json!({"aktion": "v_ifcs_throttle_swap_mode", "name": "Cruise", "symbol": "gauge", "farbe": "violett"});
    let out = plugin.on_event(&appear("C", Action::GameAction, old), &mut verse);
    let set = events(&out, "setSettings");
    assert_eq!(set.len(), 1, "kept by the Stream Deck program");
    assert_eq!(set[0]["context"], "C");
    assert_eq!(set[0]["payload"]["aktion"], "v_ifcs_speed_limiter_toggle");
    assert_eq!(set[0]["payload"]["name"], "Begrenzer");
    assert_eq!(set[0]["payload"]["symbol"], "gauge", "the rest of the key stays");
    assert_eq!(plugin.keys["C"].settings["aktion"], "v_ifcs_speed_limiter_toggle", "and pressed so at once");
    // The same action under a name of the player, or the new key: nothing changes.
    let own = json!({"aktion": "v_ifcs_throttle_swap_mode", "name": "Mein Cruise"});
    assert!(events(&plugin.on_event(&appear("D", Action::GameAction, own), &mut verse), "setSettings").is_empty());
    let new = json!({"aktion": "v_ifcs_speed_limiter_toggle", "name": "Begrenzer"});
    assert!(events(&plugin.on_event(&appear("E", Action::GameAction, new), &mut verse), "setSettings").is_empty());
    assert_eq!(updated_settings(&json!({"aktion": "v_ifcs_throttle_swap_mode", "name": "Cruise"}), Lang::En).expect("new")["name"], "Limiter");
}

/// The game action keys of every page of a shipped profile: the place (column, row) and the settings.
fn shipped_game_keys(file: &str) -> Vec<((u64, u64), Value)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles").join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut keys = Vec::new();
    for entry in versekit_core::zip::entries(&bytes).expect("zip") {
        let lower = entry.name.to_ascii_lowercase();
        if !lower.contains("/profiles/") || !lower.ends_with("/manifest.json") {
            continue;
        }
        let page: Value = serde_json::from_slice(&versekit_core::zip::read(&bytes, &entry).expect("read")).expect("json");
        for controller in page["Controllers"].as_array().into_iter().flatten().filter(|c| c["Type"] == "Keypad") {
            for (place, action) in controller["Actions"].as_object().into_iter().flatten() {
                if action["UUID"] != Action::GameAction.uuid() {
                    continue;
                }
                let (column, row) = place.split_once(',').expect("place");
                keys.push(((column.parse().expect("column"), row.parse().expect("row")), action["Settings"].clone()));
            }
        }
    }
    keys
}

fn appear_at(context: &str, device: &str, settings: Value, column: u64, row: u64) -> Value {
    json!({"event": "willAppear", "action": Action::GameAction.uuid(), "context": context, "device": device,
           "payload": {"settings": settings, "controller": "Keypad", "coordinates": {"column": column, "row": row}}})
}

#[test]
fn a_new_edition_of_the_profiles_is_offered_once_more_to_a_device_that_had_the_old_one() {
    // The first edition keeps the names and the marks from before editions: nothing is offered twice.
    assert_eq!(PROFILE_EDITION, 1, "a new edition is a decision of the maintainer; until then the names stay");
    assert_eq!(shipped_profile("VerseKit-StreamDeck-MK2", Lang::En, 1), "VerseKit-StreamDeck-MK2-EN");
    assert_eq!(offer_mark("IPAD", 1), "IPAD");
    let had_one = json!({"event": "didReceiveGlobalSettings", "payload": {"settings": {"profil_angeboten": ["IPAD"]}}});
    let connect = json!({"event": "deviceDidConnect", "device": "IPAD", "deviceInfo": {"type": 3, "name": "iPad"}});
    let mut plugin = Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE".into()]);
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    plugin.on_event(&connect, &mut verse);
    assert!(events(&plugin.on_event(&had_one, &mut verse), "switchToProfile").is_empty());
    // Edition 2: the file has the number, the device is offered it once, the old mark stays.
    let mut plugin = Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE-2".into()]);
    plugin.edition = 2;
    plugin.poll(&mut verse);
    plugin.on_event(&connect, &mut verse);
    let out = plugin.on_event(&had_one, &mut verse);
    let switch = events(&out, "switchToProfile");
    assert_eq!(switch.len(), 1);
    assert_eq!(switch[0]["payload"]["profile"], "profiles/VerseKit-StreamDeck-Mobile-DE-2");
    assert_eq!(events(&out, "setGlobalSettings")[0]["payload"]["profil_angeboten"], json!(["IPAD", "IPAD#2"]));
    assert!(events(&plugin.on_event(&connect, &mut verse), "switchToProfile").is_empty(), "once only");
}

#[test]
fn a_key_that_moved_in_the_shipped_profile_gets_the_key_that_stands_there_now_and_one_of_the_player_stays() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "MK2", "deviceInfo": {"type": 0, "name": "Stream Deck"}}), &mut verse);
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "XL", "deviceInfo": {"type": 2, "name": "Stream Deck XL"}}), &mut verse);
    let missiles = json!({"aktion": "v_weapon_increase_max_missiles", "farbe": "rot", "name": "Raketen +", "symbol": "package-plus"});
    let out = plugin.on_event(&appear_at("A", "MK2", missiles.clone(), 3, 0), &mut verse);
    let set = events(&out, "setSettings");
    assert_eq!(set.len(), 1, "kept by the Stream Deck program");
    assert_eq!(
        set[0]["payload"],
        json!({"aktion": "v_target_lock_selected", "farbe": "orange", "name": "Ziel sperren", "symbol": "lock"})
    );
    assert_eq!(plugin.keys["A"].settings["aktion"], "v_target_lock_selected", "and pressed so at once");
    // Another place, another kind of device, a key the player changed, a key in a multi action: nothing changes.
    for (context, device, settings, column) in [
        ("B", "MK2", missiles.clone(), 2),
        ("C", "XL", missiles.clone(), 3),
        ("D", "MK2", json!({"aktion": "v_weapon_increase_max_missiles", "farbe": "rot", "name": "Mehr Raketen", "symbol": "package-plus"}), 3),
        ("E", "MK2", json!({"aktion": "v_weapon_increase_max_missiles", "farbe": "rot", "name": "Raketen +", "symbol": "rocket"}), 3),
        ("F", "UNKNOWN", missiles.clone(), 3),
    ] {
        assert!(events(&plugin.on_event(&appear_at(context, device, settings, column, 0), &mut verse), "setSettings").is_empty(), "{context}");
    }
    let mut multi = appear_at("G", "MK2", missiles, 3, 0);
    multi["payload"]["isInMultiAction"] = json!(true);
    assert!(events(&plugin.on_event(&multi, &mut verse), "setSettings").is_empty(), "multi action");
    // English names, and the other side of the swap.
    let lock = json!({"aktion": "v_target_lock_selected", "farbe": "orange", "name": "Lock Target", "symbol": "lock"});
    let swapped = place_updated_settings(&lock, 0, &json!({"column": 4, "row": 0}), Lang::En).expect("swapped");
    assert_eq!((swapped["aktion"].as_str(), swapped["name"].as_str()), (Some("v_weapon_increase_max_missiles"), Some("Missiles +")));
}

#[test]
fn every_place_update_brings_the_key_of_the_shipped_profile_and_no_shipped_key_is_touched() {
    for rule in PLACE_UPDATES {
        let base = profile_base(rule.kind).expect("kind with a profile");
        for (lang, index) in [("DE", 0), ("EN", 1)] {
            let keys = shipped_game_keys(&format!("{base}-{lang}.streamDeckProfile"));
            let here: Vec<&Value> = keys.iter().filter(|(p, _)| *p == rule.place).map(|(_, s)| s).collect();
            let wanted = json!({"aktion": rule.action, "farbe": rule.colour, "name": rule.names[index], "symbol": rule.symbol});
            assert!(here.iter().any(|s| **s == wanted), "{base}-{lang} {:?}: {wanted} not shipped there", rule.place);
            // A key of today's profile at that place must never count as an old one.
            let coordinates = json!({"column": rule.place.0, "row": rule.place.1});
            for settings in &here {
                let lang = if index == 1 { Lang::En } else { Lang::De };
                assert!(place_updated_settings(settings, rule.kind, &coordinates, lang).is_none(), "{base}-{lang:?} {settings}");
            }
        }
    }
    // No key of any shipped profile is changed by a place update or a key update.
    for (kind, base, _) in PROFILE_DEVICES {
        for (lang, file_lang) in [(Lang::De, "DE"), (Lang::En, "EN")] {
            for (place, settings) in shipped_game_keys(&format!("{base}-{file_lang}.streamDeckProfile")) {
                let coordinates = json!({"column": place.0, "row": place.1});
                assert!(place_updated_settings(&settings, kind, &coordinates, lang).is_none(), "{base} {place:?} {settings}");
                assert!(updated_settings(&settings, lang).is_none(), "{base} {place:?} {settings}");
            }
        }
    }
}

#[test]
fn the_helmet_key_of_an_older_profile_goes_to_the_helmet_that_works_with_a_weapon_in_hand_and_names_are_put_right() {
    // German and English profile: „Helm an-/ablegen“ (toggleEquipHelmet) works only with empty hands.
    for (name, lang, now) in [("Helm", Lang::De, "Helm"), ("Helmet", Lang::En, "Helmet"), ("Helmet", Lang::De, "Helm")] {
        let old = json!({"aktion": "toggleEquipHelmet", "name": name, "symbol": "hard-hat", "farbe": "charakter"});
        let new = updated_settings(&old, lang).expect("updated");
        assert_eq!((new["aktion"].as_str(), new["name"].as_str()), (Some("toggleAttachHelmet"), Some(now)), "{name} {lang:?}");
        assert_eq!(new["symbol"], "hard-hat", "the rest of the key stays");
    }
    // Mining and salvage press M (cycle industrial mode): the game drops a keyboard line of the toggles.
    for (old, name, lang, now) in [
        ("v_toggle_mining_mode", "Bergbau", Lang::De, "Bergbau"),
        ("v_toggle_mining_mode", "Mining", Lang::En, "Mining"),
        ("v_toggle_salvage_mode", "Bergung", Lang::De, "Bergung"),
        ("v_toggle_salvage_mode", "Salvage", Lang::En, "Salvage"),
    ] {
        let new = updated_settings(&json!({"aktion": old, "name": name, "symbol": "pickaxe"}), lang).expect("updated");
        assert_eq!((new["aktion"].as_str(), new["name"].as_str()), (Some("v_cycle_industrial_mode"), Some(now)), "{old} {name}");
    }
    let greet = updated_settings(&json!({"aktion": "emote_greet", "name": "Grüssen"}), Lang::De).expect("greet");
    assert_eq!((greet["aktion"].as_str(), greet["name"].as_str()), (Some("emote_greet"), Some("Grüßen")));
    for burst in ["Salve +", "Burst +"] {
        let old = json!({"aktion": "v_weapon_countermeasure_decoy_burst_increase", "name": burst});
        assert_eq!(updated_settings(&old, Lang::De).expect("burst")["name"], "Flares +", "{burst}");
    }
    // A key the player named himself, and a key brought up to date, stay.
    assert!(updated_settings(&json!({"aktion": "toggleEquipHelmet", "name": "Kopf"}), Lang::De).is_none());
    assert!(updated_settings(&json!({"aktion": "toggleAttachHelmet", "name": "Helm"}), Lang::De).is_none());
    assert!(updated_settings(&json!({"aktion": "emote_greet", "name": "Grüßen"}), Lang::De).is_none());
    // The helmet the keys press now is one the plugin knows a factory key and a short name of.
    let factory: Value = serde_json::from_str(include_str!("../../../../streamdeck-plugin/werkstasten.json")).expect("json");
    assert_eq!(factory["aktionen"]["toggleAttachHelmet"]["eingabe"], "lalt+h");
    assert_eq!(factory["aktionen"]["v_cycle_industrial_mode"]["eingabe"], "m");
    assert_eq!(versekit_core::streamdeck::short_name("toggleAttachHelmet", Lang::De), Some("HELM"));
}

#[test]
fn the_version_key_shows_the_version_of_verse_kit_also_without_it_and_a_press_does_nothing() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("V", Action::Version, json!({})), &mut verse);
    let word = versekit_core::streamdeck::version_word(env!("CARGO_PKG_VERSION"));
    let look = plugin.look(&plugin.keys["V"].clone());
    assert_eq!((look.label.as_str(), look.badge.expect("badge").word), ("VerseKit", word.clone()));
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "V"}), &mut verse);
    assert!(events(&out, "showAlert").is_empty() && without(&verse.commands).is_empty(), "nothing happens");
    verse.away = true;
    plugin.poll(&mut verse);
    assert_eq!(plugin.look(&plugin.keys["V"].clone()).badge.expect("badge").word, word, "without VerseKit too");
    assert_eq!(versekit_core::streamdeck::version_word("5.7.0-beta.6"), "5.7.0 · Beta 6");
    assert_eq!(versekit_core::streamdeck::version_word("5.7.0-rc.1"), "5.7.0 · RC 1");
    assert_eq!(versekit_core::streamdeck::version_word("5.7.0"), "5.7.0");
}

#[test]
fn a_hand_signal_says_below_its_name_that_it_plays_only_with_a_weapon_drawn() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("J", Action::Emote, json!({"aktion": "emote_cs_yes", "name": "Ja", "symbol": "check"})), &mut verse);
    let look = plugin.look(&plugin.keys["J"].clone());
    assert_eq!(look.label, "Ja");
    let badge = look.badge.expect("badge");
    assert_eq!((badge.word.as_str(), badge.tone), ("mit Waffe", Tone::Muted), "a hint, no warning");
    // Another emote shows only its name; the key asked for stays the word below a hand signal too.
    plugin.on_event(&appear("W", Action::Emote, json!({"aktion": "emote_wave", "name": "Winken"})), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["W"].clone()).badge, None);
    let shown = json!({"aktion": "emote_cs_yes", "name": "Ja", "taste_anzeigen": true});
    plugin.on_event(&appear("K", Action::Emote, shown), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["K"].clone()).badge.expect("badge").word, "Ziffernblock 4");
    assert!(versekit_core::streamdeck::needs_weapon("player_emotes.emote_cs_stop"));
    assert!(!versekit_core::streamdeck::needs_weapon("emote_salute"));
}

/// The last list of game actions VerseKit was told.
fn last_reported(verse: &FakeVerse) -> Vec<String> {
    verse
        .commands
        .iter()
        .rev()
        .find_map(|c| match c {
            Command::GameActionsInUse(names) => Some(names.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn the_game_actions_of_every_page_of_the_shipped_profile_are_reported_not_only_the_shown_keys() {
    let mut plugin =
        Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE".into(), "VerseKit-StreamDeck-Mobile-EN".into()]);
    plugin.profile_actions.insert("VerseKit-StreamDeck-Mobile-DE".into(), vec!["v_target_under_reticle".into(), "v_lights".into()]);
    plugin.profile_actions.insert("VerseKit-StreamDeck-Mobile-EN".into(), vec!["v_english_only".into()]);
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "D1", "deviceInfo": {"type": 3, "name": "iPad"}}), &mut verse);
    assert_eq!(last_reported(&verse), vec!["v_lights", "v_target_under_reticle"], "a page not shown yet counts");
    // A key of the player's own page: added, and it stays when the page changes.
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_shield_up"})), &mut verse);
    plugin.on_event(&json!({"event": "willDisappear", "context": "K1"}), &mut verse);
    assert_eq!(last_reported(&verse), vec!["v_lights", "v_shield_up", "v_target_under_reticle"]);
    // The language of VerseKit picks the profile.
    verse.set("sprache", json!("en"));
    plugin.poll(&mut verse);
    assert_eq!(last_reported(&verse), vec!["v_english_only", "v_shield_up"]);
}

#[test]
fn without_a_device_of_a_shipped_profile_only_the_shown_keys_count() {
    let (mut plugin, mut verse) = ready();
    plugin.profile_actions.insert("VerseKit-StreamDeck-Mobile-DE".into(), vec!["v_target_under_reticle".into()]);
    plugin.on_event(&appear("K1", Action::GameAction, json!({"aktion": "v_lights"})), &mut verse);
    assert_eq!(last_reported(&verse), vec!["v_lights"]);
}

#[test]
fn the_report_of_the_game_actions_always_fits_into_one_command() {
    let mut plugin = Plugin::new("PLUGIN", vec!["VerseKit-StreamDeck-Mobile-DE".into()]);
    // More actions than a report may name, each with a name of the longest form.
    let long: Vec<String> =
        (0..streamdeck::MAX_REPORTED_ACTIONS + 40).map(|n| format!("v_{n:04}_{}", "x".repeat(89))).collect();
    plugin.profile_actions.insert("VerseKit-StreamDeck-Mobile-DE".into(), long);
    let mut verse = FakeVerse::new();
    plugin.poll(&mut verse);
    plugin.on_event(&json!({"event": "deviceDidConnect", "device": "D1", "deviceInfo": {"type": 3, "name": "iPad"}}), &mut verse);
    let names = last_reported(&verse);
    assert_eq!(names.len(), streamdeck::MAX_REPORTED_ACTIONS, "the most a report may name, all of them");
    let body = streamdeck::command_body(&Command::GameActionsInUse(names));
    assert!(body.len() <= streamdeck::MAX_BODY, "{}", body.len());
    assert!(streamdeck::parse_command(body.as_bytes()).is_ok());
}

#[test]
fn the_game_actions_of_a_profile_file_are_read_from_every_page_and_dial() {
    let mut zip = versekit_core::zip::Builder::new((2026, 1, 1, 0, 0, 0));
    let page = |actions: Value| json!({"Controllers": [{"Type": "Keypad", "Actions": actions}]}).to_string();
    zip.add("P.sdProfile/manifest.json", br#"{"Pages":{"Pages":["a","b"]}}"#).expect("add");
    let first = page(json!({
        "0,0": {"UUID": Action::GameAction.uuid(), "Settings": {"aktion": "v_target_under_reticle"}},
        "1,0": {"UUID": "com.elgato.streamdeck.system.hotkey", "Settings": {"aktion": "nicht_meins"}},
        "2,0": {"UUID": Action::Emote.uuid(), "Settings": {"aktion": "pc_emote_wave", "zurueck_seite": 1}}
    }));
    let second = page(json!({
        "0,0": {"UUID": Action::GameAction.uuid(), "Settings": {"aktion_links": "v_power_down", "aktion_rechts": "v_power_up", "aktion_druck": ""}},
        "1,0": {"UUID": Action::GameAction.uuid(), "Settings": {"aktion": "v_target_under_reticle"}}
    }));
    zip.add("P.sdProfile/Profiles/A/manifest.json", first.as_bytes()).expect("add");
    zip.add("P.sdProfile/Profiles/B/manifest.json", second.as_bytes()).expect("add");
    let file = zip.finish().expect("zip");
    let mut found = profile_game_actions(&file);
    found.sort();
    assert_eq!(found, vec!["pc_emote_wave", "v_power_down", "v_power_up", "v_target_under_reticle"]);
    assert!(profile_game_actions(b"kein zip").is_empty());
}

/// With real files, run by hand: the factory setting as XML (`VERSEKIT_PROBE_PROFILE`) and an `actionmaps.xml`
/// (`VERSEKIT_PROBE_ACTIONMAPS`, only read). Every game action of every shipped profile gets a key; nothing is left.
#[test]
#[ignore = "needs the files of a game, see the comment"]
fn every_game_action_of_the_shipped_profiles_gets_a_key_beside_real_bindings() {
    use versekit_core::streamdeck_fill::{Outcome, count, plan};
    let read = |name: &str| std::fs::read_to_string(std::env::var(name).expect(name)).expect(name);
    let profile = versekit_core::controls::Profile::from_xml(&read("VERSEKIT_PROBE_PROFILE")).expect("profile");
    let own = versekit_core::joysticks::bindings_from_text(&read("VERSEKIT_PROBE_ACTIONMAPS"));
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&folder).expect("profiles").flatten() {
        for name in profile_game_actions(&std::fs::read(entry.path()).expect("profile")) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.sort();
    let items = plan(&names, &own, &profile, &[], false);
    let full = items.iter().filter(|i| i.outcome == Outcome::NoFreeKey).count();
    let counted = count(&items);
    println!("{} actions, {} without a key, {} can be put in, {} with no free key", names.len(), counted.without_key, counted.addable, full);
    assert_eq!(full, 0);
    assert_eq!(counted.addable, counted.without_key, "one putting in leaves nothing missing");
}

#[test]
fn every_game_action_in_the_shipped_profiles_has_the_form_verse_kit_takes() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/profiles");
    let mut files = 0;
    for entry in std::fs::read_dir(&folder).expect("profiles").flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("streamDeckProfile") {
            continue;
        }
        files += 1;
        let names = profile_game_actions(&std::fs::read(entry.path()).expect("profile"));
        for name in &names {
            assert!(streamdeck::plausible_action_name(name), "{:?}: {name:?}", entry.file_name());
        }
        let body = streamdeck::command_body(&Command::GameActionsInUse(names.clone()));
        assert!(names.len() <= streamdeck::MAX_REPORTED_ACTIONS && body.len() <= streamdeck::MAX_BODY, "{:?}", entry.file_name());
    }
    assert!(files >= 2);
}

#[test]
fn an_emote_without_a_page_or_without_a_shipped_profile_stays() {
    let (mut plugin, mut verse) = ready_with_ipad();
    plugin.on_event(&appear("E1", Action::Emote, json!({"aktion": "v_lights"})), &mut verse);
    let out = press_for(&mut plugin, &mut verse, "E1", Duration::from_millis(100));
    assert!(events(&out, "switchToProfile").is_empty());
    let (mut bare, mut verse) = ready();
    bare.on_event(&appear("E1", Action::Emote, json!({"aktion": "v_lights", "zurueck_seite": 2})), &mut verse);
    let out = press_for(&mut bare, &mut verse, "E1", Duration::from_millis(100));
    assert!(events(&out, "switchToProfile").is_empty(), "no shipped profile, no switch");
}

#[test]
fn a_counted_switch_switches_on_a_tap_is_unsure_and_a_long_hold_sets_it_back_without_sending() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("S1", Action::GameAction, json!({"aktion": "v_lights", "umschalter": true})), &mut verse);
    let badge = |p: &Plugin| p.look_of("S1").expect("key").badge.expect("badge");
    assert_eq!(badge(&plugin).word, "AUS?", "after a start the state is not known");
    press_for(&mut plugin, &mut verse, "S1", Duration::from_millis(150));
    let on = badge(&plugin);
    assert_eq!((on.word.as_str(), on.filled), ("AN?", true));
    assert_eq!(game_commands(&verse).len(), 2, "one press: down and up");
    let out = press_for(&mut plugin, &mut verse, "S1", SWITCH_RESET);
    assert_eq!(events(&out, "showOk").len(), 1);
    let back = badge(&plugin);
    assert_eq!((back.word.as_str(), back.filled), ("AUS", false), "set back and sure");
    assert_eq!(game_commands(&verse).len(), 2, "the hold sent nothing to the game");
    press_for(&mut plugin, &mut verse, "S1", Duration::from_millis(150));
    assert_eq!(badge(&plugin).word, "AN?", "counted again: unsure again");
}

#[test]
fn a_counted_switch_is_never_sent_while_it_is_held() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("S1", Action::GameAction, json!({"aktion": "v_lights", "umschalter": true})), &mut verse);
    plugin.on_event(&json!({"event": "keyDown", "context": "S1"}), &mut verse);
    assert!(game_commands(&verse).is_empty(), "the press waits for the key to come up");
    // The states of a counted switch differ without color.
    let mut pictures = Vec::new();
    for (on, sure) in [(false, false), (true, false), (false, true), (true, true)] {
        plugin.counted.insert("S1".into(), Counted { on, sure });
        pictures.push(crate::paint::grey(&crate::paint::png(&plugin.look_of("S1").expect("key"), Action::GameAction)));
    }
    let mut unique = pictures.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 4);
}

#[test]
fn the_bindings_folder_key_asks_verse_kit_and_says_ok() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("B1", Action::BindingsFolder, json!({})), &mut verse);
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "B1"}), &mut verse);
    assert_eq!(events(&out, "showOk").len(), 1);
    assert_eq!(without(&verse.commands), vec![Command::BindingsFolder]);
    verse.refuse = true;
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "B1"}), &mut verse);
    assert_eq!(events(&out, "showAlert").len(), 1);
}

#[test]
fn a_page_key_shows_the_symbol_of_its_page() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("K1", Action::Page, json!({"seite": "hf_hangar"})), &mut verse);
    assert_eq!(plugin.look(&plugin.keys["K1"].clone()).icon, "plane");
    for page in streamdeck::PAGES {
        assert!(render::icon_known(page_icon(page)), "{page}: {}", page_icon(page));
    }
}

#[test]
fn a_name_in_two_lines_is_split_where_the_lines_are_most_even() {
    let (lines, _) = crate::paint::lines_of("Key bindings folder", true).expect("fits");
    assert_eq!(lines, vec!["Key bindings".to_string(), "folder".to_string()]);
}

#[test]
fn the_plugin_tells_verse_kit_its_version_whenever_verse_kit_answers_anew() {
    let (mut plugin, mut verse) = ready();
    let told = |v: &FakeVerse| v.commands.iter().filter(|c| matches!(c, Command::PluginVersion(_))).count();
    assert_eq!(told(&verse), 1, "on the first answer");
    assert!(verse.commands.contains(&Command::PluginVersion(crate::pack::version())));
    plugin.poll(&mut verse);
    assert_eq!(told(&verse), 1, "not again while VerseKit keeps answering");
    verse.away = true;
    plugin.poll(&mut verse);
    verse.away = false;
    plugin.poll(&mut verse);
    assert_eq!(told(&verse), 2, "VerseKit started anew: told again");
}

#[test]
fn the_name_of_a_key_with_a_state_stands_in_one_line_of_18_pixels() {
    let mut problems = Vec::new();
    for lang in ["de", "en"] {
        let (mut plugin, mut verse) = ready();
        verse.set("sprache", json!(lang));
        verse.set("contested", json!({"offen": true, "sekunden": 3300}));
        plugin.poll(&mut verse);
        for (i, action) in Action::ALL.iter().enumerate() {
            plugin.on_event(&appear(&format!("Z{i}"), *action, json!({})), &mut verse);
            let look = plugin.look_of(&format!("Z{i}")).expect("key");
            if look.badge.is_none() {
                continue;
            }
            let size = crate::paint::lines_of(&look.label, false).map(|(_, s)| s);
            if size != Some(crate::paint::style().font) {
                problems.push(format!("{lang} {action:?} „{}“: {size:?}", look.label));
            }
        }
    }
    assert!(problems.is_empty(), "names of keys with a state too long for one line of 18 pixels:\n{}", problems.join("\n"));
}

#[test]
fn the_mining_scanner_shows_its_state_and_switches_through_verse_kit() {
    let (mut plugin, mut verse) = ready();
    plugin.on_event(&appear("M1", Action::MiningScanner, json!({})), &mut verse);
    let off = plugin.look(&plugin.keys["M1"].clone()).badge.expect("badge");
    assert_eq!((off.word.as_str(), off.filled), ("AUS", false));
    verse.set("mining_scanner", json!({"an": true}));
    let out = plugin.on_event(&json!({"event": "keyDown", "context": "M1"}), &mut verse);
    assert_eq!(without(&verse.commands), vec![Command::MiningScanner], "the press goes to VerseKit, not as F22");
    assert_eq!(events(&out, "setImage").len(), 1, "the key shows the new state at once");
    let on = plugin.look(&plugin.keys["M1"].clone()).badge.expect("badge");
    assert_eq!((on.word.as_str(), on.filled), ("AN", true));
}

#[test]
fn click_through_and_sounds_show_their_state_and_switch_through_verse_kit() {
    for (action, field, command) in
        [(Action::ClickThrough, "durchklick", Command::ClickThrough), (Action::Sounds, "toene", Command::Sounds)]
    {
        let (mut plugin, mut verse) = ready();
        plugin.on_event(&appear("S1", action, json!({})), &mut verse);
        let off = plugin.look(&plugin.keys["S1"].clone()).badge.expect("badge");
        assert_eq!((off.word.as_str(), off.filled), ("AUS", false), "{action:?}");
        verse.set(field, json!({"an": true}));
        let out = plugin.on_event(&json!({"event": "keyDown", "context": "S1"}), &mut verse);
        assert_eq!(without(&verse.commands), vec![command], "{action:?}");
        assert_eq!(events(&out, "setImage").len(), 1, "{action:?}: the new state at once");
        let on = plugin.look(&plugin.keys["S1"].clone()).badge.expect("badge");
        assert_eq!((on.word.as_str(), on.filled), ("AN", true), "{action:?}");
    }
}

#[test]
fn durations_read_as_hours_minutes_or_seconds() {
    assert_eq!(duration(45), "45 s");
    assert_eq!(duration(61), "2 min");
    assert_eq!(duration(3725), "1:02 h");
}
