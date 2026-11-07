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

//! The keys anti-AFK ([`crate::anti_afk`]) presses: the keyboard binding of the action "wipe visor"
//! ([`ACTION`]) as the player has it in the game.
//!
//! The `actionmaps.xml` holds only what the player changed ([`crate::controls`]): without a keyboard rebind of
//! the action the factory setting of the game holds, Left Alt + X ([`DEFAULT_INPUT`], `defaultProfile.xml`,
//! group `vehicle_general`). A rebind on a stick leaves the keyboard default in place; only a keyboard rebind
//! replaces it. Where the file cannot be read, the keyboard binding was deleted (an empty input) or names a key
//! this module has no scan code for (a mouse button, an unknown name), Left Alt + X is pressed and the origin
//! says so ([`Origin::Fallback`]).
//!
//! ⚠ The modifiers are separate keys for Windows: `lalt+k` is Left Alt down, K down, K up, Left Alt up. Right
//! Alt, Right Ctrl, the arrows and the block above them are extended keys (`E0`): their scan code alone would
//! be the key of the number block.

use crate::joysticks::{self, Binding, FileState, Kind};
use crate::paths::Paths;

/// The action of the game that wipes the visor.
pub const ACTION: &str = "visor_wipe";
/// Its keyboard binding in the factory setting of the game.
pub const DEFAULT_INPUT: &str = "lalt+x";

/// One key as `SendInput` takes it: the scan code (set 1) and whether it is an extended key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub scan: u16,
    pub extended: bool,
}

const fn plain(scan: u16) -> Key {
    Key { scan, extended: false }
}

const fn extended(scan: u16) -> Key {
    Key { scan, extended: true }
}

/// Left Alt and X: what is pressed where nothing else can be told.
pub const FALLBACK: [Key; 2] = [plain(0x38), plain(0x2D)];

/// The names the game gives the keys of the keyboard, with their scan codes.
const NAMES: &[(&str, Key)] = &[
    ("escape", plain(0x01)),
    ("1", plain(0x02)),
    ("2", plain(0x03)),
    ("3", plain(0x04)),
    ("4", plain(0x05)),
    ("5", plain(0x06)),
    ("6", plain(0x07)),
    ("7", plain(0x08)),
    ("8", plain(0x09)),
    ("9", plain(0x0A)),
    ("0", plain(0x0B)),
    ("minus", plain(0x0C)),
    ("equals", plain(0x0D)),
    ("backspace", plain(0x0E)),
    ("tab", plain(0x0F)),
    ("q", plain(0x10)),
    ("w", plain(0x11)),
    ("e", plain(0x12)),
    ("r", plain(0x13)),
    ("t", plain(0x14)),
    ("y", plain(0x15)),
    ("u", plain(0x16)),
    ("i", plain(0x17)),
    ("o", plain(0x18)),
    ("p", plain(0x19)),
    ("lbracket", plain(0x1A)),
    ("rbracket", plain(0x1B)),
    ("enter", plain(0x1C)),
    ("lctrl", plain(0x1D)),
    ("a", plain(0x1E)),
    ("s", plain(0x1F)),
    ("d", plain(0x20)),
    ("f", plain(0x21)),
    ("g", plain(0x22)),
    ("h", plain(0x23)),
    ("j", plain(0x24)),
    ("k", plain(0x25)),
    ("l", plain(0x26)),
    ("semicolon", plain(0x27)),
    ("apostrophe", plain(0x28)),
    ("grave", plain(0x29)),
    ("lshift", plain(0x2A)),
    ("backslash", plain(0x2B)),
    ("z", plain(0x2C)),
    ("x", plain(0x2D)),
    ("c", plain(0x2E)),
    ("v", plain(0x2F)),
    ("b", plain(0x30)),
    ("n", plain(0x31)),
    ("m", plain(0x32)),
    ("comma", plain(0x33)),
    ("period", plain(0x34)),
    ("slash", plain(0x35)),
    ("rshift", plain(0x36)),
    ("np_multiply", plain(0x37)),
    ("lalt", plain(0x38)),
    ("space", plain(0x39)),
    ("capslock", plain(0x3A)),
    ("f1", plain(0x3B)),
    ("f2", plain(0x3C)),
    ("f3", plain(0x3D)),
    ("f4", plain(0x3E)),
    ("f5", plain(0x3F)),
    ("f6", plain(0x40)),
    ("f7", plain(0x41)),
    ("f8", plain(0x42)),
    ("f9", plain(0x43)),
    ("f10", plain(0x44)),
    ("numlock", plain(0x45)),
    ("scrolllock", plain(0x46)),
    ("np_7", plain(0x47)),
    ("np_8", plain(0x48)),
    ("np_9", plain(0x49)),
    ("np_subtract", plain(0x4A)),
    ("np_4", plain(0x4B)),
    ("np_5", plain(0x4C)),
    ("np_6", plain(0x4D)),
    ("np_add", plain(0x4E)),
    ("np_1", plain(0x4F)),
    ("np_2", plain(0x50)),
    ("np_3", plain(0x51)),
    ("np_0", plain(0x52)),
    ("np_period", plain(0x53)),
    ("f11", plain(0x57)),
    ("f12", plain(0x58)),
    ("np_enter", extended(0x1C)),
    ("rctrl", extended(0x1D)),
    ("np_divide", extended(0x35)),
    ("ralt", extended(0x38)),
    ("home", extended(0x47)),
    ("up", extended(0x48)),
    ("pgup", extended(0x49)),
    ("left", extended(0x4B)),
    ("right", extended(0x4D)),
    ("end", extended(0x4F)),
    ("down", extended(0x50)),
    ("pgdn", extended(0x51)),
    ("insert", extended(0x52)),
    ("delete", extended(0x53)),
];

/// The scan code of a key by the name the game gives it (`lalt`, `k`, `np_5`), in any case.
pub fn key_of(name: &str) -> Option<Key> {
    let name = name.trim().to_ascii_lowercase();
    NAMES.iter().find(|(known, _)| *known == name).map(|(_, key)| *key)
}

/// The name the game gives a key by its scan code (set 1) and the extended flag; `None` for a code not in the table.
pub fn name_of(key: Key) -> Option<&'static str> {
    NAMES.iter().find(|(_, known)| *known == key).map(|(name, _)| *name)
}

/// The keys of a keyboard input of the game (`lalt+k`, `kb1_lalt+k`) in the order they go down: the modifiers
/// first, the key last. `None` for an empty input or one with a name without a scan code.
pub fn parse(input: &str) -> Option<Vec<Key>> {
    let input = input.trim();
    let input = input.strip_prefix("kb1_").unwrap_or(input);
    if input.is_empty() {
        return None;
    }
    let keys: Option<Vec<Key>> = input.split('+').map(key_of).collect();
    keys.filter(|keys| !keys.is_empty())
}

/// Where the keys come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The keyboard rebind of the player in the `actionmaps.xml`.
    Own,
    /// No keyboard rebind: the factory setting of the game.
    Default,
    /// The binding cannot be told or not be pressed: Left Alt + X.
    Fallback,
}

impl Origin {
    /// The word in the file of anti-AFK.
    pub fn key(self) -> &'static str {
        match self {
            Origin::Own => "eigene",
            Origin::Default => "standard",
            Origin::Fallback => "ersatz",
        }
    }

    pub fn from_key(text: &str) -> Option<Origin> {
        [Origin::Own, Origin::Default, Origin::Fallback].into_iter().find(|origin| origin.key() == text)
    }
}

/// The keys of one press with the input they were read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    /// The input as the game writes it (`lalt+k`).
    pub input: String,
    pub keys: Vec<Key>,
    pub origin: Origin,
}

impl Chord {
    /// Left Alt + X with the origin `origin`.
    pub fn fallback(origin: Origin) -> Chord {
        Chord { input: DEFAULT_INPUT.to_string(), keys: FALLBACK.to_vec(), origin }
    }

    /// The scan codes of the keys.
    pub fn scans(&self) -> Vec<u16> {
        self.keys.iter().map(|key| key.scan).collect()
    }
}

impl Default for Chord {
    fn default() -> Self {
        Chord::fallback(Origin::Default)
    }
}

/// The chord from the bindings of the `actionmaps.xml` ([`joysticks::bindings`]): the first keyboard rebind
/// of [`ACTION`]. A rebind with an input wins; only empty ones (deleted) give the fallback.
pub fn from_bindings(own: &[(String, Vec<Binding>)]) -> Chord {
    let rebinds: Vec<&Binding> = own
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .filter(|entry| entry.kind == Kind::Keyboard && entry.action == ACTION)
        .collect();
    if rebinds.is_empty() {
        return Chord::fallback(Origin::Default);
    }
    let Some(input) = rebinds.iter().map(|entry| entry.input.trim()).find(|input| !input.is_empty()) else {
        return Chord::fallback(Origin::Fallback);
    };
    match parse(input) {
        Some(keys) => Chord { input: input.to_ascii_lowercase(), keys, origin: Origin::Own },
        None => Chord::fallback(Origin::Fallback),
    }
}

/// The chord of the game of these paths: from its `actionmaps.xml`, the factory setting where there is none,
/// the fallback where it cannot be read.
pub fn resolve(paths: &Paths) -> Chord {
    let Some(file) = joysticks::actionmaps_path(paths, None) else { return Chord::fallback(Origin::Default) };
    match joysticks::file_state(paths, Some(&file), None) {
        FileState::Missing => Chord::fallback(Origin::Default),
        FileState::Unreadable => Chord::fallback(Origin::Fallback),
        FileState::Fine => from_bindings(&joysticks::bindings(paths, Some(&file), None)),
    }
}

/// A mark of the `actionmaps.xml` of these paths that changes when the file does (path, size, time in
/// nanoseconds); empty where there is none. Cheap: the file is not read.
pub fn stamp(paths: &Paths) -> String {
    let Some(file) = joysticks::actionmaps_path(paths, None) else { return String::new() };
    let Ok(info) = std::fs::metadata(&file) else { return String::new() };
    let nanos = info
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    format!("{file}:{}:{nanos}", info.len())
}

/// [`resolve`], read again only when the [`stamp`] of the file changed.
#[derive(Debug, Default)]
pub struct Cached {
    kept: Option<(String, Chord)>,
}

impl Cached {
    pub fn get(&mut self, paths: &Paths) -> Chord {
        let mark = stamp(paths);
        if let Some((known, chord)) = &self.kept
            && *known == mark
        {
            return chord.clone();
        }
        let chord = resolve(paths);
        self.kept = Some((mark, chord.clone()));
        chord
    }
}

/// Under test: a throwaway home with the game folder `StarCitizen/LIVE` in it, and the path of its
/// `actionmaps.xml` (its folder made, the file not). Nothing real is touched.
#[cfg(test)]
pub(crate) fn test_world(name: &str) -> (Paths, String, crate::testdir::Dir) {
    let folder = crate::testdir::Dir::new(name);
    let home = folder.path().to_string_lossy().replace('\\', "/");
    let game = format!("{home}/StarCitizen/LIVE");
    let profile = format!("{game}/USER/Client/0/Profiles/default");
    std::fs::create_dir_all(&profile).expect("folder");
    // The game folder counts as one with a log in it.
    std::fs::write(format!("{game}/Game.log"), "").expect("log");
    let vars = [
        ("SC_BP_HOME", home.as_str()),
        ("HOME", home.as_str()),
        ("LOCALAPPDATA", home.as_str()),
        (crate::paths::ENV_INSTALL_DIR, game.as_str()),
        (crate::game_start::ENV_NO_PROCESS, "1"),
    ];
    let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
    (paths, format!("{profile}/actionmaps.xml"), folder)
}

/// Under test: an `actionmaps.xml` with these rebinds of [`ACTION`] and a keyboard rebind of another action.
#[cfg(test)]
pub(crate) fn test_maps(rebinds: &str) -> String {
    format!(
        "<ActionMaps><ActionProfiles version=\"1\"><actionmap name=\"vehicle_general\"><action name=\"visor_wipe\">{rebinds}</action></actionmap><actionmap name=\"player\"><action name=\"v_other\"><rebind input=\"kb1_lalt+j\"/></action></actionmap></ActionProfiles></ActionMaps>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joysticks::bindings_from_text;

    use super::test_maps as maps;

    fn chord(rebinds: &str) -> Chord {
        from_bindings(&bindings_from_text(&maps(rebinds)))
    }

    const LALT: Key = Key { scan: 0x38, extended: false };

    #[test]
    fn the_own_keyboard_binding_wins_and_modifiers_go_down_first() {
        let own = chord(r#"<rebind input="kb1_lalt+k"/>"#);
        assert_eq!(own.origin, Origin::Own);
        assert_eq!(own.input, "lalt+k");
        assert_eq!(own.keys, [LALT, plain(0x25)], "Left Alt, then K");
        let three = chord(r#"<rebind input="kb1_lctrl+lshift+f5"/>"#);
        assert_eq!(three.keys, [plain(0x1D), plain(0x2A), plain(0x3F)]);
        let right = chord(r#"<rebind input="kb1_ralt+rctrl+delete"/>"#);
        assert_eq!(right.keys, [extended(0x38), extended(0x1D), extended(0x53)], "the right ones are extended");
        assert_eq!(chord(r#"<rebind input="kb1_np_5"/>"#).keys, [plain(0x4C)]);
        assert_eq!(chord(r#"<rebind input="kb1_K"/>"#).keys, [plain(0x25)], "any case");
    }

    #[test]
    fn without_a_keyboard_rebind_the_factory_setting_holds_and_a_stick_rebind_does_not_replace_it() {
        let none = from_bindings(&bindings_from_text(&maps("")));
        assert_eq!((none.origin, none.keys.as_slice()), (Origin::Default, FALLBACK.as_slice()));
        let stick = chord(r#"<rebind input="js1_button5"/>"#);
        assert_eq!(stick.origin, Origin::Default, "the key stays bound in the game");
        assert_eq!(stick.keys, FALLBACK);
        // The action of another group with Alt + J does not count.
        assert_eq!(chord("").input, DEFAULT_INPUT);
    }

    #[test]
    fn a_deleted_or_unknown_binding_gives_alt_x_and_says_so() {
        let deleted = chord(r#"<rebind input="kb1_ "/>"#);
        assert_eq!((deleted.origin, deleted.keys.as_slice()), (Origin::Fallback, FALLBACK.as_slice()));
        let mouse = chord(r#"<rebind input="kb1_mouse4"/>"#);
        assert_eq!((mouse.origin, mouse.input.as_str()), (Origin::Fallback, DEFAULT_INPUT));
        let unknown = chord(r#"<rebind input="kb1_lalt+mystery"/>"#);
        assert_eq!(unknown.origin, Origin::Fallback);
        // A deleted one and a filled one: the filled one.
        assert_eq!(chord(r#"<rebind input="kb1_ "/><rebind input="kb2_lalt+k"/>"#).input, "lalt+k");
        assert_eq!(parse(""), None);
        assert_eq!(parse("lalt+"), None, "a modifier without a key name after it");
    }

    #[test]
    fn every_name_has_its_own_scan_code() {
        let mut seen = std::collections::BTreeSet::new();
        for &(name, key) in NAMES {
            assert!(seen.insert((key.scan, key.extended)), "{name} twice");
            assert_eq!(key_of(name), Some(key));
        }
        for origin in [Origin::Own, Origin::Default, Origin::Fallback] {
            assert_eq!(Origin::from_key(origin.key()), Some(origin));
        }
    }

    #[test]
    fn the_file_of_the_game_is_read_and_read_again_when_it_changes() {
        let (paths, file, _folder) = test_world("aafk-taste");
        assert_eq!(resolve(&paths), Chord::fallback(Origin::Default), "no file: the factory setting");
        assert_eq!(stamp(&paths), "");
        std::fs::write(&file, maps(r#"<rebind input="kb1_lalt+k"/>"#)).unwrap();
        let mut cached = Cached::default();
        assert_eq!(cached.get(&paths).input, "lalt+k");
        // Changed: the next look reads it again (another size makes another mark).
        std::fs::write(&file, maps(r#"<rebind input="kb1_lalt+lshift+v"/>"#)).unwrap();
        assert_eq!(cached.get(&paths).input, "lalt+lshift+v");
        std::fs::write(&file, "<ActionMaps><broken").unwrap();
        let broken = cached.get(&paths);
        assert_eq!((broken.origin, broken.keys.as_slice()), (Origin::Fallback, FALLBACK.as_slice()));
    }
}
