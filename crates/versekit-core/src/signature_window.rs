// Verse-Kit — companion for Star Citizen.
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

//! The small window with the scanned material, directly above the signature in the game.
//!
//! It stands as long as the signature is recognised. Its place is relative to the pill (the box of numbers of
//! the scanner), which stands in a fixed place of the ship HUD: directly above it, centred, moved by an offset
//! that is set on the model of the screen in the settings or by dragging with the lock open. The offset is kept
//! as a share of the game window (per mille), so it holds at every resolution. This module is the part without
//! a window: texts, sizes, places and where the scanner reads ([`Hint`]).

use crate::jsonio::Value;
use crate::mining_data::MiningData;
use crate::paths::Paths;
use crate::screen_grab::Area;

/// The settings of the window.
pub mod keys {
    /// The area the player drew on the model of the screen: `[left, top, width, height]` in per mille of the
    /// game window. Only this area is read.
    pub const FRAME: &str = "signatur_rahmen";
    /// Where the pill was found, per size of the game window: `{"2560x1440": [left, top, width, height]}` in
    /// points from the corner of the game window.
    pub const PLACES: &str = "signatur_lage";
    /// Offset to the right of the place directly above the pill, in per mille of the game window's width.
    pub const OFFSET_X: &str = "signatur_fenster_dx";
    /// Offset downwards, in per mille of the game window's height.
    pub const OFFSET_Y: &str = "signatur_fenster_dy";
    /// The size of the window ([`super::Size`]): a number in per mille, older settings hold a word.
    pub const SIZE: &str = "signatur_fenster_groesse";
    /// Where the number shows while the info bar is on ([`super::Place`]).
    pub const PLACE: &str = "signatur_fenster_ort";
    /// Seconds the number stays shown after it left the HUD.
    pub const LINGER: &str = "signatur_fenster_nachlauf";
    /// Besides the window, the line "Signature …" in the open overlay.
    pub const OVERLAY_LINE: &str = "signatur_overlay_zeile";
}

/// Where the scanner reads: the area the player drew, else the place where the pill was found before, else
/// (nothing here) the middle of the game window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hint {
    /// `[left, top, width, height]` in per mille of the game window.
    pub frame: Option<[i32; 4]>,
    /// The places found, by `size_key`.
    pub places: Vec<(String, [i32; 4])>,
}

/// The name of a game window size in the settings: `2560x1440`.
pub fn size_key(game: Area) -> String {
    format!("{}x{}", game.2, game.3)
}

fn four(value: &Value) -> Option<[i32; 4]> {
    let Value::Array(list) = value else { return None };
    let mut out = [0i32; 4];
    if list.len() != 4 {
        return None;
    }
    for (slot, v) in out.iter_mut().zip(list) {
        let Value::Int(text) = v else { return None };
        *slot = text.parse().ok()?;
    }
    Some(out)
}

fn four_value(numbers: [i32; 4]) -> Value {
    Value::Array(numbers.iter().map(|n| Value::int(i64::from(*n))).collect())
}

impl Hint {
    pub fn read(paths: &Paths) -> Hint {
        let settings = paths.settings();
        let get = |key: &str| settings.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let frame = get(keys::FRAME).and_then(four).filter(|f| f[2] >= 10 && f[3] >= 10 && f[0] >= 0 && f[1] >= 0);
        let places = match get(keys::PLACES) {
            Some(Value::Object(map)) => map.iter().filter_map(|(k, v)| four(v).map(|f| (k.clone(), f))).collect(),
            _ => Vec::new(),
        };
        Hint { frame, places }
    }

    /// The place found for this size of the game window.
    pub fn place_for(&self, key: &str) -> Option<[i32; 4]> {
        self.places.iter().find(|(k, _)| k == key).map(|(_, f)| *f)
    }

    /// The place found last, in per mille of the game window it was found in; none without a place or with a
    /// key that is no size.
    pub fn remembered(&self) -> Option<[i32; 4]> {
        let (key, [x, y, w, h]) = self.places.last()?;
        let (game_w, game_h) = key.split_once('x')?;
        let (game_w, game_h): (i32, i32) = (game_w.parse().ok()?, game_h.parse().ok()?);
        if game_w <= 0 || game_h <= 0 {
            return None;
        }
        let share = |part: i32, whole: i32| (f64::from(part) * 1000.0 / f64::from(whole)).round() as i32;
        Some([share(*x, game_w), share(*y, game_h), share(*w, game_w), share(*h, game_h)])
    }

    /// The same with the place for `key` set.
    pub fn with_place(&self, key: &str, place: [i32; 4]) -> Hint {
        let mut next = self.clone();
        match next.places.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = place,
            None => next.places.push((key.to_string(), place)),
        }
        next
    }

    /// Is `place` another place than the one kept for `key` (moved by more than a third of its size)?
    pub fn differs(&self, key: &str, place: [i32; 4]) -> bool {
        match self.place_for(key) {
            None => true,
            Some(old) => {
                (old[0] - place[0]).abs() * 3 > place[2].max(1) || (old[1] - place[1]).abs() * 3 > place[3].max(1)
            }
        }
    }

    pub fn frame_value(&self) -> Value {
        self.frame.map_or(Value::Null, four_value)
    }

    pub fn places_value(&self) -> Value {
        Value::Object(self.places.iter().map(|(k, f)| (k.clone(), four_value(*f))).collect())
    }

    /// The area to read in the game window `game`, and whether the player drew it (then nothing else is ever
    /// read). Without a frame the place found before, with a margin; `None` where there is neither.
    pub fn narrow(&self, game: Area) -> Option<(Area, bool)> {
        let inside = |(x, y, w, h): Area| -> Option<Area> {
            let (x0, y0) = (x.max(game.0), y.max(game.1));
            let (x1, y1) = ((x + w).min(game.0 + game.2), (y + h).min(game.1 + game.3));
            (x1 - x0 >= 8 && y1 - y0 >= 8).then_some((x0, y0, x1 - x0, y1 - y0))
        };
        if let Some([fx, fy, fw, fh]) = self.frame {
            let share = |whole: i32, part: i32| (f64::from(whole) * f64::from(part) / 1000.0).round() as i32;
            let area = (game.0 + share(game.2, fx), game.1 + share(game.3, fy), share(game.2, fw), share(game.3, fh));
            return inside(area).map(|a| (a, true));
        }
        let [px, py, pw, ph] = self.place_for(&size_key(game))?;
        let (mx, my) = ((pw / 4).max(24), (ph / 2).max(16));
        inside((game.0 + px - mx, game.1 + py - my, pw + 2 * mx, ph + 2 * my)).map(|a| (a, false))
    }
}

/// The offset reaches this far (per mille) to each side: the model of the settings can show the whole screen.
pub const OFFSET_LIMIT: i32 = 600;

/// Space between the window and the pill, in per mille of the game window's height.
const GAP: f64 = 4.0;

/// The size of the window: the height of the letters of its headline in per mille of the game window's height,
/// so the window grows with the picture. A slider sets it from [`Size::MIN`] to [`Size::MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size(pub i32);

impl Default for Size {
    fn default() -> Size {
        Size::DEFAULT
    }
}

impl Size {
    pub const MIN: Size = Size(6);
    pub const DEFAULT: Size = Size(11);
    pub const MAX: Size = Size(32);

    /// From the settings: a number in per mille, or one of the words the three fixed sizes were stored as
    /// (`klein`, `mittel`, `gross`); anything else is the default.
    pub fn read(value: Option<&Value>) -> Size {
        let within = |n: i32| Size(n.clamp(Size::MIN.0, Size::MAX.0));
        match value {
            Some(Value::Str(word)) => match word.as_str() {
                "klein" => Size(16),
                "mittel" => Size(22),
                "gross" => Size(32),
                _ => Size::DEFAULT,
            },
            Some(Value::Int(text)) => text.parse().map_or(Size::DEFAULT, within),
            Some(Value::Float(x)) => within(x.round() as i32),
            _ => Size::DEFAULT,
        }
    }

    /// As the settings keep it: the number.
    pub fn value(self) -> Value {
        Value::int(i64::from(self.0))
    }

    /// The height of the letters of the headline in points of a game window `game_height` points high, never
    /// below eight points.
    pub fn font_px(self, game_height: i32) -> i32 {
        ((f64::from(game_height) * f64::from(self.0) / 1000.0).round() as i32).max(8)
    }
}

/// Where the number and its ore show while the info bar is on: in the window above the signature (and not in the
/// bar), or in the info bar (and no window). Without the info bar the window always shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Place {
    AboveSignature,
    #[default]
    InfoBar,
}

impl Place {
    pub const ALL: [Place; 2] = [Place::AboveSignature, Place::InfoBar];

    pub fn from_setting(value: Option<&str>) -> Place {
        match value {
            Some("signatur") => Place::AboveSignature,
            _ => Place::InfoBar,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            Place::AboveSignature => "signatur",
            Place::InfoBar => "infoleiste",
        }
    }

    /// The text key of its name on the page.
    pub fn label(self) -> &'static str {
        match self {
            Place::AboveSignature => "s_sg_ort_signatur",
            Place::InfoBar => "s_sg_ort_leiste",
        }
    }
}

/// How many seconds the number stays shown after it left the HUD: from the first to the second.
pub const LINGER_RANGE: (i64, i64) = (5, 15);

/// The layout of the window as the settings keep it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub dx: i32,
    pub dy: i32,
    pub size: Size,
    /// The number with its ore also as a line of the open overlay, on by default as the line of 4.12.
    pub overlay_line: bool,
    pub place: Place,
    /// Seconds the number stays shown after it left the HUD (within [`LINGER_RANGE`]).
    pub linger: i64,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout {
            dx: 0,
            dy: 0,
            size: Size::default(),
            overlay_line: true,
            place: Place::default(),
            linger: LINGER_RANGE.0,
        }
    }
}

impl Layout {
    pub fn read(paths: &Paths) -> Layout {
        let (low, high) = (Some(-i64::from(OFFSET_LIMIT)), Some(i64::from(OFFSET_LIMIT)));
        let settings = paths.settings();
        let size = settings.iter().find(|(k, _)| k == keys::SIZE).map(|(_, v)| v);
        Layout {
            dx: paths.setting_int(keys::OFFSET_X, 0, low, high) as i32,
            dy: paths.setting_int(keys::OFFSET_Y, 0, low, high) as i32,
            size: Size::read(size),
            overlay_line: paths.setting_bool(keys::OVERLAY_LINE, true),
            place: Place::from_setting(paths.setting(keys::PLACE).as_deref()),
            linger: paths.setting_int(keys::LINGER, LINGER_RANGE.0, Some(LINGER_RANGE.0), Some(LINGER_RANGE.1)),
        }
    }
}

/// What the window says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// The material behind the number (up to three), or "no ore recognised" where nothing fits.
    pub material: String,
    /// The number as the HUD writes it, with thousands separator.
    pub number: String,
    /// Does the material name something the data know? (Else it stands in the normal colour, not the accent.)
    pub known: bool,
}

impl Text {
    /// `12,680 → 4× Quantainium`: what stands behind a signature in one line.
    pub fn line(&self) -> String {
        if self.material.is_empty() { self.number.clone() } else { format!("{} → {}", self.number, self.material) }
    }
}

/// `12,680` for 12680.
pub fn with_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The name of a hit as the player reads it: the ground deposits, the hand mining and the salvage by their words,
/// the raw materials as they are.
pub fn display_name(name: &str) -> String {
    match name {
        "roc" => crate::language::t("s_sg_basis_roc"),
        "fps" => crate::language::t("s_sg_basis_hand"),
        "salvage" => crate::language::t("s_sg_basis_salvage"),
        other => other.to_string(),
    }
}

/// The deposits that fit a signature exactly (deviation under 0.05 percent), at most three, in the order of the
/// data.
fn exact_hits_in_data_order(value: u64, mining: &MiningData) -> Vec<crate::mining_data::SignatureHit> {
    mining.find_signature(&value.to_string()).into_iter().filter(|h| h.deviation.abs() < 0.05).take(3).collect()
}

/// The deposits the window names for a signature: those that fit exactly, at most three, ores before the ground
/// deposits, the hand mining and the salvage.
fn exact_hits(value: u64, mining: &MiningData) -> Vec<crate::mining_data::SignatureHit> {
    let mut hits = exact_hits_in_data_order(value, mining);
    hits.sort_by_key(|h| crate::mining_data::is_base_signature(&h.name));
    hits
}

/// The names of the deposits behind a signature as the data call them, at most three.
pub fn materials(value: u64, mining: &MiningData) -> Vec<String> {
    exact_hits_in_data_order(value, mining).into_iter().map(|h| h.name).collect()
}

/// What stands behind a signature, at most three hits with the chunks: `4× Quantainium · 2× Bexalite`; the
/// salvage after the ores. Where nothing fits: "No ore recognised".
pub fn describe(value: u64, mining: &MiningData) -> Text {
    describe_with(value, mining, true)
}

/// As [`describe`]; without `localized` the names are the ones of the data and nothing fitting leaves the
/// material empty.
pub fn describe_with(value: u64, mining: &MiningData, localized: bool) -> Text {
    if !localized {
        let hits = exact_hits_in_data_order(value, mining);
        let material = hits.iter().map(|h| format!("{}× {}", h.chunks, h.name)).collect::<Vec<_>>().join(" · ");
        return Text { material, number: with_thousands(value), known: !hits.is_empty() };
    }
    let hits = exact_hits(value, mining);
    let material = if hits.is_empty() {
        crate::language::t("s_sg_kein_erz")
    } else {
        hits.iter().map(|h| format!("{}× {}", h.chunks, display_name(&h.name))).collect::<Vec<_>>().join(" · ")
    };
    Text { material, number: with_thousands(value), known: !hits.is_empty() }
}

/// Where the window stands: its top left corner on the screen.
///
/// Directly above the pill and centred on it, moved by the offset, and kept inside the game window.
pub fn place(pill: Area, game: Area, window: (i32, i32), layout: &Layout) -> (i32, i32) {
    let gap = (f64::from(game.3) * GAP / 1000.0).round() as i32;
    let dx = (f64::from(game.2) * f64::from(layout.dx) / 1000.0).round() as i32;
    let dy = (f64::from(game.3) * f64::from(layout.dy) / 1000.0).round() as i32;
    let x = pill.0 + pill.2 / 2 - window.0 / 2 + dx;
    let y = pill.1 - gap - window.1 + dy;
    (clamp(x, game.0, game.0 + game.2 - window.0), clamp(y, game.1, game.1 + game.3 - window.1))
}

fn clamp(value: i32, low: i32, high: i32) -> i32 {
    if high < low { low } else { value.clamp(low, high) }
}

/// The offset (per mille) for a window whose top left corner was moved to `top_left` instead of the place the
/// layout without offset gives: what a drag or the model of the settings turns into.
pub fn offset_for(top_left: (i32, i32), pill: Area, game: Area, window: (i32, i32)) -> (i32, i32) {
    let base = place(pill, game, window, &Layout::default());
    let share = |moved: i32, whole: i32| -> i32 {
        if whole <= 0 {
            return 0;
        }
        ((f64::from(moved) * 1000.0 / f64::from(whole)).round() as i32).clamp(-OFFSET_LIMIT, OFFSET_LIMIT)
    };
    (share(top_left.0 - base.0, game.2), share(top_left.1 - base.1, game.3))
}

/// The size of the window for the text: letters of the headline `font` points high, the number below in
/// three quarters of that. Rough measures of the text (`wide`: width of the longest line in letters) give the
/// window its size; the real width comes from the measure of the system where there is one.
pub fn window_size(font: i32, wide: usize, with_material: bool) -> (i32, i32) {
    let pad = (font / 2).max(6);
    let small = font * 3 / 4;
    let text_h = if with_material { font + small + font / 4 } else { small };
    let text_w = (wide as i32 * font * 11 / 20).max(font * 4);
    (text_w + 2 * pad, text_h + 2 * pad)
}

#[cfg(test)]
mod tests;
