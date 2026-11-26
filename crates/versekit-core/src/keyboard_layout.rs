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

//! The sign a key makes on the keyboard layout that is active. The game names a key by its place on a US
//! keyboard (`y` is the key right of `t`); on a German layout that place makes a `Z`. [`label`] turns the
//! name of the game into the sign on the cap, from any table of signs by scan code ([`system_char`] is the
//! one of the system).
//!
//! The place is told by the scan code ([`crate::anti_afk_key::key_of`]): scan code to virtual key to sign. Only
//! the keys of the main block that make a sign are handled; a name without a scan code, an extended key or
//! the number block gets `None`.

use crate::anti_afk_key::{self, Key};

/// The signs of the keys by name on a US keyboard, where the name is no letter or figure.
const US_SIGNS: [(&str, char); 11] = [
    ("minus", '-'),
    ("equals", '='),
    ("lbracket", '['),
    ("rbracket", ']'),
    ("semicolon", ';'),
    ("apostrophe", '\''),
    ("grave", '`'),
    ("backslash", '\\'),
    ("comma", ','),
    ("period", '.'),
    ("slash", '/'),
];

/// The scan codes of the keys of the main block that make a sign: the row of figures, the three letter rows
/// and the signs around them.
fn makes_a_sign(scan: u16) -> bool {
    matches!(scan, 0x02..=0x0D | 0x10..=0x1B | 0x1E..=0x29 | 0x2B..=0x35)
}

/// The sign the key `name` makes on a US keyboard, in capitals; `None` for a key that makes none.
pub fn us_sign(name: &str) -> Option<char> {
    let name = name.trim().to_ascii_lowercase();
    let key = anti_afk_key::key_of(&name)?;
    if key.extended || !makes_a_sign(key.scan) {
        return None;
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => Some(only.to_ascii_uppercase()),
        _ => US_SIGNS.iter().find(|(known, _)| *known == name).map(|(_, sign)| *sign),
    }
}

/// A sign in capitals; one that has no single capital (`ß`) stays as it is.
fn capital(sign: char) -> char {
    let mut upper = sign.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(only), None) => only,
        _ => sign,
    }
}

/// What stands on the key `name` of the game on the layout `layout` (scan code to sign), where it differs from
/// the US keyboard; `None` where it is the same sign, where the key makes no sign or the layout knows none.
pub fn label(name: &str, layout: impl Fn(u16) -> Option<char>) -> Option<String> {
    let us = us_sign(name)?;
    let key: Key = anti_afk_key::key_of(name)?;
    let sign = layout(key.scan).filter(|sign| !sign.is_control() && !sign.is_whitespace())?;
    let sign = capital(sign);
    (sign != us).then(|| sign.to_string())
}

/// The sign the scan code makes on the layout of the system (unshifted, in capitals for letters). A dead key
/// gives its own sign. `None` where the layout has none and on every system but Windows.
#[cfg(windows)]
pub fn system_char(scan: u16) -> Option<char> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
    };
    // SAFETY: both calls take plain integers and the handle of the layout of this thread; they touch no memory.
    let (vk, sign) = unsafe {
        let layout = GetKeyboardLayout(0);
        let vk = MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout);
        (vk, MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, layout))
    };
    if vk == 0 {
        return None;
    }
    // The top bit marks a dead key.
    char::from_u32(sign & 0x7FFF_FFFF).filter(|sign| *sign != '\0')
}

#[cfg(not(windows))]
pub fn system_char(_scan: u16) -> Option<char> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout table by key name; every key not listed makes its US sign.
    fn table(changed: &'static [(&'static str, char)]) -> impl Fn(u16) -> Option<char> {
        move |scan| {
            let name = anti_afk_key::name_of(Key { scan, extended: false })?;
            changed.iter().find(|(known, _)| *known == name).map(|(_, sign)| *sign).or_else(|| us_sign(name))
        }
    }

    const GERMAN: &[(&str, char)] = &[
        ("y", 'Z'),
        ("z", 'Y'),
        ("semicolon", 'ö'),
        ("apostrophe", 'ä'),
        ("lbracket", 'ü'),
        ("minus", 'ß'),
        ("rbracket", '+'),
        ("slash", '-'),
        ("equals", '´'),
    ];

    #[test]
    fn the_german_layout_turns_the_places_into_the_signs_on_the_cap() {
        let de = table(GERMAN);
        assert_eq!(label("y", &de).as_deref(), Some("Z"));
        assert_eq!(label("z", &de).as_deref(), Some("Y"));
        assert_eq!(label("semicolon", &de).as_deref(), Some("Ö"));
        assert_eq!(label("apostrophe", &de).as_deref(), Some("Ä"));
        assert_eq!(label("lbracket", &de).as_deref(), Some("Ü"));
        assert_eq!(label("minus", &de).as_deref(), Some("ß"), "no capital for it");
        assert_eq!(label("slash", &de).as_deref(), Some("-"));
        assert_eq!(label("x", &de), None, "same sign as on a US keyboard");
        assert_eq!(label("comma", &de), None);
    }

    #[test]
    fn a_key_of_the_game_is_the_same_key_on_a_german_and_a_us_keyboard_only_its_cap_differs() {
        let (de, us) = (table(GERMAN), table(&[]));
        let caps = [("lbracket", "Ü"), ("rbracket", "+"), ("semicolon", "Ö"), ("apostrophe", "Ä"), ("y", "Z"), ("z", "Y")];
        for (name, german) in caps {
            let key = anti_afk_key::key_of(name).expect("a key");
            // What goes out is the scan code of the place: one and the same for both layouts.
            assert_eq!(anti_afk_key::name_of(key), Some(name), "{name}");
            assert_eq!(label(name, &de).as_deref(), Some(german), "{name} on a German keyboard");
            assert_eq!(label(name, &us), None, "{name} on a US keyboard is what the game says");
        }
    }

    #[test]
    fn the_us_layout_changes_nothing() {
        let us = table(&[]);
        for name in ["y", "z", "semicolon", "minus", "lbracket", "5", "slash"] {
            assert_eq!(label(name, &us), None, "{name}");
        }
    }

    #[test]
    fn without_a_table_or_a_sign_there_is_no_label() {
        assert_eq!(label("y", |_| None), None);
        assert_eq!(label("y", |_| Some('\0')), None, "a control character is no label");
        assert_eq!(label("y", |_| Some(' ')), None);
        for name in ["space", "f5", "up", "lshift", "np_5", "np_divide", "mouse1", "nonsense", ""] {
            assert_eq!(label(name, |_| Some('Q')), None, "{name} keeps its own name");
        }
    }

    #[test]
    fn the_names_are_taken_in_any_case() {
        assert_eq!(label("Y", table(GERMAN)).as_deref(), Some("Z"));
    }
}
