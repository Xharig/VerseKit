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

//! The info bar: a narrow window of its own over the game with the shard, the room for the search of handles and,
//! while the scanner reads, the scanned material. This module is the part without a window: where it stands.
//!
//! It starts at the top in the middle of the main screen. Where the player dragged it is kept for the size of the
//! main screen (`2560x1440`), so another resolution starts at the top in the middle again and the old one finds its
//! place back. The place is the top left corner in screen pixels: the bar grows to the right when the material
//! comes in.

use crate::jsonio::Value;
use crate::overlay::Rect;
use crate::paths::Paths;

/// The settings of the bar.
pub mod keys {
    /// Where the player dragged the bar, per size of the main screen: `{"2560x1440": [left, top]}` in pixels.
    pub const PLACES: &str = "infoleiste_lage";
    /// Where the bar starts while it was not dragged: `links`, `mitte` (the default) or `rechts`, at the top.
    pub const CORNER: &str = "infoleiste_ecke";
}

/// Where the bar starts at the top of the main screen while it was not dragged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    Left,
    #[default]
    Middle,
    Right,
}

impl Corner {
    pub const ALL: [Corner; 3] = [Corner::Left, Corner::Middle, Corner::Right];

    pub fn word(self) -> &'static str {
        match self {
            Corner::Left => "links",
            Corner::Middle => "mitte",
            Corner::Right => "rechts",
        }
    }

    /// The corner of a stored word; anything else is the middle.
    pub fn from_word(word: &str) -> Corner {
        Corner::ALL.into_iter().find(|c| c.word() == word).unwrap_or_default()
    }

    pub fn read(paths: &Paths) -> Corner {
        paths.setting(keys::CORNER).as_deref().map(Corner::from_word).unwrap_or_default()
    }
}

/// The places kept, by the size of the main screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places(pub Vec<(String, [i32; 2])>);

/// The name of a screen size in the settings: `2560x1440`.
pub fn screen_key(screen: Rect) -> String {
    format!("{}x{}", screen.w, screen.h)
}

fn two(value: &Value) -> Option<[i32; 2]> {
    let Value::Array(list) = value else { return None };
    match list.as_slice() {
        [Value::Int(x), Value::Int(y)] => Some([x.parse().ok()?, y.parse().ok()?]),
        _ => None,
    }
}

impl Places {
    pub fn read(paths: &Paths) -> Places {
        let settings = paths.settings();
        match settings.iter().find(|(k, _)| k == keys::PLACES).map(|(_, v)| v) {
            Some(Value::Object(map)) => {
                Places(map.iter().filter_map(|(k, v)| two(v).map(|p| (k.clone(), p))).collect())
            }
            _ => Places::default(),
        }
    }

    /// The place kept for the screen size `key`.
    pub fn place_for(&self, key: &str) -> Option<[i32; 2]> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, p)| *p)
    }

    /// The same with the place for `key` set.
    pub fn with_place(&self, key: &str, place: [i32; 2]) -> Places {
        let mut next = self.clone();
        match next.0.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = place,
            None => next.0.push((key.to_string(), place)),
        }
        next
    }

    pub fn value(&self) -> Value {
        Value::Object(
            self.0
                .iter()
                .map(|(k, [x, y])| {
                    (k.clone(), Value::Array(vec![Value::int(i64::from(*x)), Value::int(i64::from(*y))]))
                })
                .collect(),
        )
    }
}

/// The top left corner of a bar `width` pixels wide at the top of the work area `work`, at its `corner`.
pub fn default_place(work: Rect, width: i32, corner: Corner) -> (i32, i32) {
    let x = match corner {
        Corner::Left => work.x,
        Corner::Middle => work.x + (work.w - width) / 2,
        Corner::Right => work.x + work.w - width,
    };
    (x, work.y)
}

/// Where the bar starts: the place kept for the size of the main screen `screen` while its left end lies on a
/// screen (`on_a_screen` with the left edge, the top and the width to look at), else at the top of `work`, the
/// work area of the main screen, at `corner`.
pub fn start_place(
    places: &Places,
    screen: Rect,
    work: Rect,
    width: i32,
    corner: Corner,
    on_a_screen: impl Fn(i32, i32, i32) -> bool,
) -> (i32, i32) {
    match places.place_for(&screen_key(screen)) {
        Some([x, y]) if on_a_screen(x, y, width.clamp(1, 120)) => (x, y),
        _ => default_place(work, width, corner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 2560, 1440);
    const WORK: Rect = Rect::new(0, 0, 2560, 1392);

    #[test]
    fn the_bar_starts_at_the_top_in_the_middle_of_the_work_area() {
        assert_eq!(default_place(WORK, 300, Corner::Middle), (1130, 0));
        assert_eq!(default_place(Rect::new(-1920, 40, 1920, 1040), 200, Corner::Middle), (-1060, 40));
        assert_eq!(start_place(&Places::default(), SCREEN, WORK, 300, Corner::default(), |_, _, _| true), (1130, 0));
    }

    /// Left and right put the bar into the top corners of the work area, its right end on the right edge; the
    /// word stored comes back as the corner, anything else is the middle.
    #[test]
    fn the_bar_starts_in_the_corner_chosen() {
        assert_eq!(default_place(WORK, 300, Corner::Left), (0, 0));
        assert_eq!(default_place(WORK, 300, Corner::Right), (2260, 0));
        assert_eq!(default_place(Rect::new(-1920, 40, 1920, 1040), 200, Corner::Right), (-200, 40));
        for corner in Corner::ALL {
            assert_eq!(Corner::from_word(corner.word()), corner);
        }
        assert_eq!(Corner::from_word("oben"), Corner::Middle);
    }

    /// A place dragged to is kept for the size of the main screen: the same size finds it, another size starts in
    /// the middle again, and a place off every screen is not taken.
    #[test]
    fn a_dragged_place_is_kept_for_the_size_of_the_main_screen() {
        let places = Places::default().with_place(&screen_key(SCREEN), [40, 300]);
        let middle = Corner::Middle;
        assert_eq!(start_place(&places, SCREEN, WORK, 300, middle, |_, _, _| true), (40, 300));
        let small = Rect::new(0, 0, 1920, 1080);
        assert_eq!(start_place(&places, small, Rect::new(0, 0, 1920, 1040), 300, middle, |_, _, _| true), (810, 0));
        assert_eq!(start_place(&places, SCREEN, WORK, 300, middle, |_, _, _| false), (1130, 0), "off every screen");
        // Set again for the same size: one place, the new one.
        let moved = places.with_place("2560x1440", [100, 0]);
        assert_eq!(moved.0, vec![("2560x1440".to_string(), [100, 0])]);
    }

    /// The places go into the settings and come back out of them as they were; a broken entry is left out.
    #[test]
    fn the_places_go_into_the_settings_and_back() {
        let dir = std::env::temp_dir().join(format!("versekit-infoleiste-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let vars = [(crate::legacy::ENV_HOME, home.as_str())];
        let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home));
        assert_eq!(Places::read(&paths), Places::default(), "nothing kept at first");
        let places = Places::default().with_place("2560x1440", [40, 300]).with_place("1920x1080", [-5, 12]);
        assert!(paths.set_setting(keys::PLACES, places.value()));
        assert_eq!(Places::read(&paths), places);
        let mut broken = places.value();
        if let Value::Object(map) = &mut broken {
            map.push(("800x600".to_string(), Value::str("links")));
        }
        assert!(paths.set_setting(keys::PLACES, broken));
        assert_eq!(Places::read(&paths), places, "the broken entry is left out");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
