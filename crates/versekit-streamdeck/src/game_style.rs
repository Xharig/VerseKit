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

//! How a game action looks on a key that names no symbol and no colour of its own: as the profiles show that action,
//! else by the `actionmap` of the game it belongs to (`streamdeck-plugin/spielaktionen.json`).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

const DATA: &str = include_str!("../../../streamdeck-plugin/spielaktionen.json");

/// The colour (a colour of a key in the house style) and the symbol of a game action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameStyle {
    pub colour: &'static str,
    pub icon: &'static str,
}

struct Table {
    actions: HashMap<String, GameStyle>,
    maps: HashMap<String, GameStyle>,
}

/// A `"farbe symbol"` of the file, when both are known.
fn parse(value: &Value) -> Option<GameStyle> {
    let (colour, icon) = value.as_str()?.split_once(' ')?;
    Some(GameStyle { colour: crate::paint::style().key_colour(colour)?, icon: crate::render::known_icon(icon)? })
}

fn section(source: &Value, name: &str) -> HashMap<String, GameStyle> {
    source[name]
        .as_object()
        .map(|entries| entries.iter().filter_map(|(key, value)| Some((key.clone(), parse(value)?))).collect())
        .unwrap_or_default()
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let source: Value = serde_json::from_str(DATA).expect("spielaktionen.json");
        Table { actions: section(&source, "aktionen"), maps: section(&source, "karten") }
    })
}

/// How the game action `action` (also as `actionmap.action`) of the `actionmap` `map` looks; `None` where neither
/// is known.
pub fn of(action: &str, map: &str) -> Option<GameStyle> {
    let (qualified_map, plain) = action.split_once('.').unwrap_or(("", action));
    let table = table();
    table
        .actions
        .get(plain)
        .or_else(|| table.maps.get(if qualified_map.is_empty() { map } else { qualified_map }))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_of_the_file_has_a_colour_of_a_key_and_a_symbol_of_the_set() {
        let source: Value = serde_json::from_str(DATA).expect("json");
        for name in ["aktionen", "karten"] {
            let entries = source[name].as_object().expect("section");
            assert!(entries.len() > 20, "{name}: {}", entries.len());
            for (key, value) in entries {
                assert!(parse(value).is_some(), "{name}.{key}: {value}");
            }
        }
    }

    #[test]
    fn an_action_looks_as_in_the_profiles_else_as_its_actionmap_else_unknown() {
        let shields = of("v_power_toggle_shields", "spaceship_power").expect("known");
        assert_eq!((shields.colour, shields.icon), ("blau", "shield-half"));
        let other = of("v_power_preset_front", "spaceship_power").expect("by its actionmap");
        assert_eq!((other.colour, other.icon), ("blau", "zap"));
        let qualified = of("spaceship_salvage.v_salvage_nothing", "").expect("by the actionmap of the name");
        assert_eq!(qualified.colour, "pink");
        let emote = of("emote_newone", "player_emotes").expect("an emote");
        assert_eq!((emote.colour, emote.icon), ("charakter", "hand"));
        assert_eq!(of("v_something", "flycam"), None, "an actionmap without a colour");
    }
}
