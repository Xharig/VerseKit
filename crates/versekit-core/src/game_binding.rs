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

//! How each action of the game can be pressed from the keyboard, as the player has bound it: the factory setting
//! and the own changes joined ([`crate::controls::view`] with [`View::All`]), and of the keyboard inputs the first
//! one whose keys can be sent ([`crate::anti_afk_key::parse`]).
//!
//! ⚠ The own change wins only on the kind of device it was made on: a stick binding of the player leaves the
//! factory key on the keyboard. ⚠ An own keyboard line with an empty input is a deleted binding: the action has
//! no key then, whatever the factory says, and [`Resolved::deleted`] says so (keys are never added over it).

use crate::anti_afk_key::{self, Key};
use crate::controls::{self, Profile, Source, View};
use crate::joysticks::{Binding, Kind};

/// How an action can be pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum How {
    /// From the keyboard: the input as the game writes it (`lalt+x`) and its keys, modifiers first.
    Keyboard { input: String, keys: Vec<Key> },
    /// Bound only to the mouse.
    MouseOnly,
    /// Bound only to a stick.
    JoystickOnly,
    /// Bound only to a gamepad.
    GamepadOnly,
    /// Bound to nothing.
    Unbound,
    /// On the keyboard, but with a key that cannot be sent (a mouse button in the combination, an unknown name).
    NotSendable { input: String },
}

impl How {
    /// The word in the interface (`tastatur`, `nur_maus`, …).
    pub fn word(&self) -> &'static str {
        match self {
            How::Keyboard { .. } => "tastatur",
            How::MouseOnly => "nur_maus",
            How::JoystickOnly => "nur_joystick",
            How::GamepadOnly => "nur_gamepad",
            How::Unbound => "nicht_belegt",
            How::NotSendable { .. } => "nicht_sendbar",
        }
    }
}

/// One action and how it is pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub action: String,
    /// The group (`actionmap`) the action lives in.
    pub group: String,
    pub how: How,
    /// The keyboard binding comes from the player (`true`) or the factory.
    pub own: bool,
    /// The player deleted the keyboard binding in the game.
    pub deleted: bool,
}

/// A name of an action as a key names it: plain (`v_view_zoom_in`), or with the `actionmap` before a dot
/// (`vehicle_general.v_view_zoom_in`) where the action stands in several with other keys. Gives the `actionmap`
/// (only one the factory setting knows) and the plain name; a dot before an unknown `actionmap` is part of the name.
pub fn qualified<'a>(name: &'a str, profile: &Profile) -> (Option<&'a str>, &'a str) {
    match name.split_once('.') {
        Some((map, action)) if !action.is_empty() && profile.maps.get(map).is_some() => (Some(map), action),
        _ => (None, name),
    }
}

/// How `action` is pressed with the bindings `own` (of the `actionmaps.xml`) and the factory setting `profile`.
///
/// ⚠ A qualified name ([`qualified`]) takes the own lines of the action only from its `actionmap`; the factory keys
/// are those of the action, which the factory setting keeps once per name. The group is that `actionmap`.
pub fn resolve(action: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> Resolved {
    let (map, plain) = qualified(action, profile);
    let Some(map) = map else { return resolve_in(action, &controls::view(View::All, own, profile), own, profile) };
    let here: Vec<(String, Vec<Binding>)> = own
        .iter()
        .map(|(id, list)| (id.clone(), list.iter().filter(|b| b.action != plain || b.section == map).cloned().collect()))
        .collect();
    let mut resolved = resolve_in(plain, &controls::view(View::All, &here, profile), &here, profile);
    resolved.action = action.to_string();
    resolved.group = map.to_string();
    resolved
}

/// Is `name` an action of the game, plain or qualified: one the factory setting names or the player bound?
pub fn known(name: &str, own: &[(String, Vec<Binding>)], profile: &Profile) -> bool {
    let (_, plain) = qualified(name, profile);
    profile.labels.get(plain).is_some() || own.iter().flat_map(|(_, list)| list.iter()).any(|b| b.action == plain)
}

fn resolve_in(action: &str, all: &controls::Devices, own: &[(String, Vec<Binding>)], profile: &Profile) -> Resolved {
    let entries: Vec<&controls::Entry> =
        all.iter().flat_map(|(_, list)| list.iter()).filter(|e| e.action == action && !e.input.is_empty()).collect();
    let deleted = own.iter().flat_map(|(_, list)| list.iter()).any(|b| {
        b.action == action && b.kind == Kind::Keyboard && b.input.trim().is_empty()
    });
    let group = own
        .iter()
        .flat_map(|(_, list)| list.iter())
        .find(|b| b.action == action && !b.section.is_empty())
        .map(|b| b.section.clone())
        .or_else(|| profile.groups.get(action).cloned())
        .unwrap_or_default();
    let keyboard: Vec<&&controls::Entry> = entries.iter().filter(|e| e.kind == Some(Kind::Keyboard)).collect();
    let sendable = keyboard.iter().find_map(|e| anti_afk_key::parse(&e.input).map(|keys| (*e, keys)));
    let how = if let Some((entry, keys)) = sendable {
        let own = entry.source == Source::Mine;
        return Resolved {
            action: action.to_string(),
            group,
            how: How::Keyboard { input: entry.input.to_ascii_lowercase(), keys },
            own,
            deleted,
        };
    } else if let Some(first) = keyboard.first() {
        How::NotSendable { input: first.input.clone() }
    } else if entries.iter().any(|e| e.kind == Some(Kind::Mouse)) {
        How::MouseOnly
    } else if entries.iter().any(|e| e.kind == Some(Kind::Joystick)) {
        How::JoystickOnly
    } else if entries.iter().any(|e| e.kind == Some(Kind::Gamepad)) {
        How::GamepadOnly
    } else {
        How::Unbound
    };
    let own_keyboard = keyboard.first().is_some_and(|e| e.source == Source::Mine);
    Resolved { action: action.to_string(), group, how, own: own_keyboard, deleted }
}

/// Every action the game names (it has a `UILabel`) and every action the player bound, how each is pressed, in
/// the order of the factory setting.
pub fn resolve_all(own: &[(String, Vec<Binding>)], profile: &Profile) -> Vec<Resolved> {
    let all = controls::view(View::All, own, profile);
    let mut names: Vec<String> = profile
        .labels
        .iter()
        .filter(|(_, (label, _))| !label.is_empty())
        .map(|(action, _)| action.to_string())
        .collect();
    for (_, list) in own {
        for binding in list {
            if !names.contains(&binding.action) {
                names.push(binding.action.clone());
            }
        }
    }
    names.iter().map(|action| resolve_in(action, &all, own, profile)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joysticks::bindings_from_text;

    fn profile() -> Profile {
        Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_general" UILabel="@ui_g" UICategory="@ui_c">
                <action name="v_eject" UILabel="@ui_eject" keyboard="ralt+y"/>
                <action name="visor_wipe" UILabel="@ui_wipe" keyboard="lalt+x"/>
                <action name="v_respawn" UILabel="@ui_respawn" keyboard="f"/>
                <action name="v_mouse" UILabel="@ui_mouse" mouse="mouse2"/>
                <action name="v_nothing" UILabel="@ui_nothing"/>
                <action name="v_internal" keyboard="f12"/>
              </actionmap>
              <actionmap name="spaceship_mining" UILabel="@ui_m">
                <action name="v_toggle_mining_laser_type" UILabel="@ui_laser" keyboard="lalt+mouse1"/>
                <action name="v_pad" UILabel="@ui_pad" gamepad="shoulderl"/>
              </actionmap>
            </ActionMaps>"#,
        )
        .expect("profile")
    }

    fn own(body: &str) -> Vec<(String, Vec<Binding>)> {
        bindings_from_text(&format!(
            r#"<ActionMaps version="1"><ActionProfiles version="1" profileName="default">{body}</ActionProfiles></ActionMaps>"#
        ))
    }

    fn how(action: &str, body: &str) -> Resolved {
        resolve(action, &own(body), &profile())
    }

    /// The same action in two `actionmap`s with other own keys, as the zoom of the view in the ship and in the
    /// vehicle.
    fn twice() -> (Profile, Vec<(String, Vec<Binding>)>) {
        let profile = Profile::from_xml(
            r#"<ActionMaps>
              <actionmap name="spaceship_view" UILabel="@v"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
              <actionmap name="vehicle_general" UILabel="@g"><action name="v_view_zoom_in" UILabel="@z"/></actionmap>
              <actionmap name="spaceship_hud" UILabel="@h"><action name="v_starmap" UILabel="@s" keyboard="f2"/></actionmap>
            </ActionMaps>"#,
        )
        .expect("profile");
        let own = own(
            r#"<actionmap name="spaceship_view"><action name="v_view_zoom_in"><rebind input="kb1_ralt+6"/></action></actionmap>
               <actionmap name="vehicle_general"><action name="v_view_zoom_in"><rebind input="kb1_ralt+8"/></action></actionmap>"#,
        );
        (profile, own)
    }

    #[test]
    fn a_qualified_name_takes_the_key_of_its_own_actionmap() {
        let (profile, own) = twice();
        let ship = resolve("spaceship_view.v_view_zoom_in", &own, &profile);
        let car = resolve("vehicle_general.v_view_zoom_in", &own, &profile);
        assert!(matches!(&ship.how, How::Keyboard { input, .. } if input == "ralt+6"), "{ship:?}");
        assert!(matches!(&car.how, How::Keyboard { input, .. } if input == "ralt+8"), "{car:?}");
        assert_eq!((ship.group.as_str(), car.group.as_str()), ("spaceship_view", "vehicle_general"));
        assert_eq!(car.action, "vehicle_general.v_view_zoom_in", "the name stays as the key names it");
        // Plain, the first key found counts, as before.
        assert_eq!(resolve("v_view_zoom_in", &own, &profile).how.word(), "tastatur");
        // A factory key of the action counts in every actionmap until the player changes it there.
        let starmap = resolve("spaceship_hud.v_starmap", &own, &profile);
        assert!(matches!(&starmap.how, How::Keyboard { input, .. } if input == "f2"));
    }

    #[test]
    fn a_deleted_line_counts_only_in_its_own_actionmap() {
        let (profile, _) = twice();
        let own = own(
            r#"<actionmap name="vehicle_general"><action name="v_starmap"><rebind input="kb1_"/></action></actionmap>"#,
        );
        assert!(resolve("vehicle_general.v_starmap", &own, &profile).deleted);
        assert!(!resolve("spaceship_hud.v_starmap", &own, &profile).deleted, "the ship keeps its key");
        assert_eq!(resolve("spaceship_hud.v_starmap", &own, &profile).how.word(), "tastatur");
    }

    #[test]
    fn a_dot_before_an_unknown_actionmap_is_part_of_the_name() {
        let (profile, own) = twice();
        assert_eq!(qualified("gibt_es_nicht.v_view_zoom_in", &profile), (None, "gibt_es_nicht.v_view_zoom_in"));
        assert_eq!(qualified("vehicle_general.v_view_zoom_in", &profile), (Some("vehicle_general"), "v_view_zoom_in"));
        assert!(known("vehicle_general.v_view_zoom_in", &own, &profile));
        assert!(!known("gibt_es_nicht.v_view_zoom_in", &own, &profile));
        assert!(!known("vehicle_general.gibt_es_nicht", &own, &profile));
    }

    #[test]
    fn the_factory_key_counts_while_the_player_changed_nothing() {
        let r = how("v_eject", "");
        assert_eq!(r.how.word(), "tastatur");
        assert!(matches!(&r.how, How::Keyboard { input, keys } if input == "ralt+y" && keys.len() == 2));
        assert!(!r.own && !r.deleted);
        assert_eq!(r.group, "spaceship_general");
    }

    #[test]
    fn an_own_keyboard_binding_replaces_the_factory_key_and_kb1_is_cut() {
        let r = how(
            "v_eject",
            r#"<actionmap name="spaceship_general"><action name="v_eject"><rebind input="kb1_lctrl+np_5"/></action></actionmap>"#,
        );
        assert!(matches!(&r.how, How::Keyboard { input, .. } if input == "lctrl+np_5"));
        assert!(r.own);
    }

    #[test]
    fn an_own_stick_binding_leaves_the_factory_key_on_the_keyboard() {
        let r = how(
            "v_respawn",
            r#"<actionmap name="spaceship_general"><action name="v_respawn"><rebind input="js1_button5"/></action></actionmap>"#,
        );
        assert!(matches!(&r.how, How::Keyboard { input, .. } if input == "f"), "{r:?}");
    }

    #[test]
    fn a_deleted_keyboard_binding_is_not_bound_and_says_it_was_deleted() {
        let r = how(
            "visor_wipe",
            r#"<actionmap name="spaceship_general"><action name="visor_wipe"><rebind input="kb1_"/></action></actionmap>"#,
        );
        assert_eq!(r.how, How::Unbound);
        assert!(r.deleted);
    }

    #[test]
    fn only_on_mouse_stick_or_pad_says_so_and_nothing_bound_is_unbound() {
        assert_eq!(how("v_mouse", "").how, How::MouseOnly);
        assert_eq!(how("v_pad", "").how, How::GamepadOnly);
        assert_eq!(how("v_nothing", "").how, How::Unbound);
        let stick = how(
            "v_nothing",
            r#"<actionmap name="spaceship_general"><action name="v_nothing"><rebind input="js2_button1"/></action></actionmap>"#,
        );
        assert_eq!(stick.how, How::JoystickOnly);
    }

    #[test]
    fn a_keyboard_binding_with_a_mouse_button_cannot_be_sent() {
        let r = how("v_toggle_mining_laser_type", "");
        assert_eq!(r.how, How::NotSendable { input: "lalt+mouse1".into() });
    }

    #[test]
    fn an_extended_key_keeps_its_mark() {
        let r = how(
            "v_eject",
            r#"<actionmap name="spaceship_general"><action name="v_eject"><rebind input="kb1_ralt+up"/></action></actionmap>"#,
        );
        let How::Keyboard { keys, .. } = r.how else { panic!("keyboard") };
        assert!(keys.iter().all(|k| k.extended), "{keys:?}");
    }

    #[test]
    fn the_list_holds_the_named_actions_and_the_own_ones_but_no_internal_one() {
        let list = resolve_all(
            &own(r#"<actionmap name="x"><action name="pl_custom"><rebind input="kb1_k"/></action></actionmap>"#),
            &profile(),
        );
        let names: Vec<&str> = list.iter().map(|r| r.action.as_str()).collect();
        assert!(names.contains(&"v_eject") && names.contains(&"pl_custom"));
        assert!(!names.contains(&"v_internal"), "no label: not in the options of the game");
    }
}
