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

//! Which field of view suits the screen (`fov.py` of 4.12, the page "Blickwinkel").
//!
//! There is exactly **one** field of view at which the picture looks as large as what it shows would be in
//! reality: when the screen takes the same angle in the eye as what it shows. Then sizes and distances are right.
//!
//! ```text
//! field of view = 2 · arctan( width of the screen / (2 · distance of the eyes) )
//! ```
//!
//! Wider than that one sees more but everything looks smaller and farther away; narrower, everything looks
//! closer and one sees less. Both can be wanted; one should only know where the neutral point lies.
//!
//! ⚠⚠ **Why the screen is measured by hand.** The computer cannot tell the width of one screen of several (Tk
//! gave 1640 mm, the width of the whole desktop of three screens). **A bank card held to the screen takes ten
//! seconds and is right:** every bank card, driving licence and identity card has the size of ISO/IEC 7810 ID-1,
//! 85.60 × 53.98 mm.
//!
//! The pixel width of the right screen comes from the window of the measuring itself: it goes full screen on the
//! screen it opens on and measures itself.
//!
//! ⚠ Only **read** here: `attributes.xml` holds all graphics settings and is not written by this page (4.12 did
//! not either). What is written are the three numbers of the calibration, in the settings of the program.

use std::sync::OnceLock;

use regex::Regex;

use crate::graphics;
use crate::jsonio::Value;
use crate::paths::Paths;
use crate::stock_data::py_float;

/// ISO/IEC 7810 ID-1: bank card, driving licence, identity card. ⚠ A standard, not a guess. Do not "smooth" it.
pub const CARD_WIDTH_MM: f64 = 85.60;
/// The height of the same card.
pub const CARD_HEIGHT_MM: f64 = 53.98;
/// How far may the distance of the eyes lie from the calculated point before it is shown? Generous on
/// purpose: the neutral angle is a point of reference, not a rule, and many fly with a wider one on purpose.
/// Up to 8 % off: right.
pub const GREEN: f64 = 0.08;
/// Up to 25 %: noticeable, but fair.
pub const YELLOW: f64 = 0.25;
/// The smallest vertical angle Star Citizen puts in `attributes.xml` when the slider "Sichtfeld" stands all the
/// way to the left, measured at 32:9 (5120×1440): menu 124, file 54.7681.
pub const MIN_VERTICAL_DEG: f64 = 54.7681;
/// The keys of the calibration in the settings: millimetres per pixel, width of the screen in pixels, distance
/// of the eyes in millimetres.
pub const KEYS: [&str; 3] = ["fov_mm_je_pixel", "fov_pixelbreite", "fov_abstand_mm"];

fn radians(deg: f64) -> f64 {
    deg * std::f64::consts::PI / 180.0
}

fn degrees(rad: f64) -> f64 {
    rad * 180.0 / std::f64::consts::PI
}

/// The neutral field of view for this screen and distance, in degrees: the value at which the picture looks
/// neither enlarged nor made smaller. `None` where the inputs are no use (a calculator that stops at distance 0
/// is worse than one that says nothing).
pub fn field_of_view(width_mm: f64, distance_mm: f64) -> Option<f64> {
    if !(width_mm > 0.0 && distance_mm > 0.0) {
        return None;
    }
    Some(degrees(2.0 * (width_mm / (2.0 * distance_mm)).atan()))
}

/// At which distance (in mm) is this angle the neutral one? The inverse of [`field_of_view`] and the useful
/// direction: whoever has set the game already wants to know where to sit for it, not to throw the setting over.
pub fn distance_for(width_mm: f64, angle_deg: f64) -> Option<f64> {
    if !(width_mm > 0.0 && angle_deg > 0.0 && angle_deg < 180.0) {
        return None;
    }
    Some(width_mm / (2.0 * (radians(angle_deg) / 2.0).tan()))
}

/// From the vertical angle the horizontal one. Star Citizen keeps the **vertical** angle and widens the
/// horizontal one when the screen gets wider ("Hor+").
pub fn horizontal_from_vertical(vertical_deg: f64, aspect: f64) -> Option<f64> {
    if !(vertical_deg > 0.0 && aspect > 0.0) {
        return None;
    }
    let half = (radians(vertical_deg) / 2.0).tan() * aspect;
    Some(degrees(2.0 * half.atan()))
}

/// What the game says: the field of view and the resolution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameSetting {
    /// The **vertical** angle in degrees, `FOV` in the file.
    pub fov: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// The file it was read from; empty where there is none.
    pub file: String,
}

impl GameSetting {
    fn size(&self) -> Option<(f64, f64)> {
        match (self.width, self.height) {
            (Some(width), Some(height)) if width != 0.0 && height != 0.0 => Some((width, height)),
            _ => None,
        }
    }
}

/// The horizontal angle from the game's setting, the number in the menu. The file holds the **vertical**
/// angle, the menu shows the horizontal one of the resolution set (measured at 32:9: menu 134 ↔ file 67.1,
/// menu 124 ↔ file 54.77). `None` without the angle or the resolution (`0` counts as missing, as in 4.12).
pub fn game_horizontal(setting: &GameSetting) -> Option<f64> {
    let vertical = setting.fov.filter(|v| *v != 0.0)?;
    let (width, height) = setting.size()?;
    horizontal_from_vertical(vertical, width / height)
}

/// The narrowest horizontal angle the game allows at this resolution, `None` without one.
pub fn narrowest_horizontal(setting: &GameSetting) -> Option<f64> {
    let (width, height) = setting.size()?;
    horizontal_from_vertical(MIN_VERTICAL_DEG, width / height)
}

/// The mark of a distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    Green,
    Yellow,
    Red,
}

/// How far is the distance of the eyes from the neutral point? The sign says the direction: **positive is too
/// far away**, negative too close.
///
/// ⚠ Red means far off, not wrong. Whoever flies wider on purpose to see more does nothing wrong, and should only
/// know that this is what is being done.
pub fn rating(actual_mm: f64, target_mm: f64) -> (Grade, f64) {
    if target_mm <= 0.0 {
        return (Grade::Red, 0.0);
    }
    let deviation = (actual_mm - target_mm) / target_mm;
    let amount = deviation.abs();
    let grade = if amount <= GREEN {
        Grade::Green
    } else if amount <= YELLOW {
        Grade::Yellow
    } else {
        Grade::Red
    };
    (grade, deviation)
}

/// From the card as measured, the size of a pixel: `measured_px` is the width to which the player has drawn the
/// rectangle until it matched the card.
pub fn mm_per_pixel(measured_px: f64) -> Option<f64> {
    (measured_px > 0.0).then(|| CARD_WIDTH_MM / measured_px)
}

/// The width of the screen in millimetres.
pub fn screen_width_mm(width_px: f64, mm_per_px: f64) -> Option<f64> {
    (width_px > 0.0 && mm_per_px > 0.0).then_some(width_px * mm_per_px)
}

fn attr_pattern(name: &str) -> Regex {
    static CACHE: OnceLock<std::sync::Mutex<Vec<(String, Regex)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, regex)) = cache.iter().find(|(n, _)| n == name) {
        return regex.clone();
    }
    let regex = Regex::new(&format!(r#"<Attr\s+name="{}"\s+value="([^"]*)""#, regex::escape(name))).expect("pattern");
    cache.push((name.to_string(), regex.clone()));
    regex
}

/// The number of the first `<Attr name="…" value="…"` for the name anywhere in the text (4.12 took the first,
/// not bound to the start of a line); `None` where there is none or it is no number.
fn pick(text: &str, name: &str) -> Option<f64> {
    let hit = attr_pattern(name).captures(text)?;
    py_float(&hit[1]).filter(|v| v.is_finite())
}

/// `FOV`, `Width` and `Height` from the text of `attributes.xml`.
pub fn game_setting_text(text: &str) -> GameSetting {
    GameSetting {
        fov: pick(text, "FOV"),
        width: pick(text, "Width"),
        height: pick(text, "Height"),
        file: String::new(),
    }
}

/// What is set in the game: the field of view and the resolution, read from the `attributes.xml` of the player
/// (`game_folder`, else the folder of the game). Every value is `None` where it does not stand there.
pub fn game_setting(paths: &Paths, game_folder: Option<&str>) -> GameSetting {
    let Some(file) = graphics::attribute_file(paths, game_folder) else { return GameSetting::default() };
    // Bytes that are no UTF-8 become the replacement sign, as in 4.12 (`errors='replace'`).
    let Ok(bytes) = std::fs::read(&file) else { return GameSetting { file, ..GameSetting::default() } };
    let mut setting = game_setting_text(&String::from_utf8_lossy(&bytes));
    setting.file = file;
    setting
}

/// The calibration kept in the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Calibration {
    pub mm_per_pixel: Option<f64>,
    pub pixel_width: Option<f64>,
    pub distance_mm: Option<f64>,
}

impl Calibration {
    /// The width of the screen in mm, where both numbers are there.
    pub fn width_mm(&self) -> Option<f64> {
        screen_width_mm(self.pixel_width?, self.mm_per_pixel?)
    }
}

/// A number as the settings hold it: a number, or a text that is one (Python's `float()` takes both).
fn setting_float(value: &Value) -> Option<f64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Str(text) => py_float(text),
        _ => None,
    }
}

/// The last calibration, if there is one. ⚠⚠ Read from the settings **as numbers**: `Paths::setting` is for
/// paths and gives `None` for anything that is no text (the calibration would be gone at every start without a
/// word), and `setting_int` cuts 0.2330 mm per pixel to 0.
pub fn stored(paths: &Paths) -> Calibration {
    let settings = paths.settings();
    let get = |key: &str| settings.iter().find(|(k, _)| k == key).and_then(|(_, v)| setting_float(v));
    Calibration { mm_per_pixel: get(KEYS[0]), pixel_width: get(KEYS[1]), distance_mm: get(KEYS[2]) }
}

/// Keeps the calibration so that it is not new at every start. ⚠ Only what is named is written: the distance can
/// be changed without losing the calibration and the other way round. Gives whether **all** named numbers were
/// written: if one cannot be written, those written before are put back as they were (4.12 said "done" in every
/// case, and a new size of the pixel with the old width of the screen in the file makes the width of the screen
/// wrong without a word).
pub fn remember(paths: &Paths, mm_per_px: Option<f64>, width_px: Option<f64>, distance_mm: Option<f64>) -> bool {
    let before = stored(paths);
    let old = [before.mm_per_pixel, before.pixel_width, before.distance_mm];
    let wanted = [mm_per_px, width_px, distance_mm];
    for (i, value) in wanted.iter().enumerate() {
        let Some(value) = value else { continue };
        if paths.set_setting(KEYS[i], Value::Float(*value)) {
            continue;
        }
        crate::errors::record("fov.remember", &format!("{} could not be written", KEYS[i]));
        for j in (0..i).filter(|j| wanted[*j].is_some()) {
            let back = old[j].map_or(Value::Null, Value::Float);
            paths.set_setting(KEYS[j], back);
        }
        return false;
    }
    true
}

/// What the distance of the eyes says about the setting of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// The narrowest angle the screen allows is set: closer is not possible.
    AtMinimum,
    /// The neutral angle is not reachable and the setting is wider than the narrowest: set the smallest value
    /// (about this many degrees).
    ToMinimum(f64),
    /// The distance fits (within 8 %).
    Good,
    /// Too far away by so many centimetres, with the mark.
    TooFar(f64, Grade),
    /// Too close by so many centimetres, with the mark.
    TooNear(f64, Grade),
}

/// What the game says compared with the neutral point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Game {
    /// No value found in the game (no file, no line, no resolution).
    Unknown,
    /// The horizontal angle set in the game, the distance at which it is the neutral one (none for an angle
    /// of 180° or more) and what that says.
    Known { in_game: f64, optimal: Option<f64>, verdict: Option<Verdict> },
}

/// The page's results from the width of the screen, the distance of the eyes and the setting of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Analysis {
    /// The neutral horizontal angle for this screen and distance.
    pub neutral: f64,
    /// The narrowest horizontal angle the game allows at this resolution.
    pub narrowest: Option<f64>,
    /// The neutral angle lies more than half a degree below the narrowest one: it cannot be set.
    pub unreachable: bool,
    pub game: Game,
}

/// Works out what the page shows. `None` where the numbers are no use (nothing to say).
pub fn analyse(width_mm: f64, distance_mm: f64, setting: &GameSetting) -> Option<Analysis> {
    let neutral = field_of_view(width_mm, distance_mm)?;
    let narrowest = narrowest_horizontal(setting);
    // Below the narrowest angle the game allows at this resolution the neutral one is not reachable: then it is
    // not asked to enter it, and the distance is not rated.
    let unreachable = narrowest.is_some_and(|narrowest| neutral < narrowest - 0.5);
    // ⚠⚠ The file holds the VERTICAL angle, the neutral value is the HORIZONTAL one: only equals are compared,
    // the setting is converted with the resolution of the same file.
    let Some(in_game) = setting.fov.and(game_horizontal(setting)) else {
        return Some(Analysis { neutral, narrowest, unreachable, game: Game::Unknown });
    };
    let Some(optimal) = distance_for(width_mm, in_game) else {
        return Some(Analysis {
            neutral,
            narrowest,
            unreachable,
            game: Game::Known { in_game, optimal: None, verdict: None },
        });
    };
    let (grade, deviation) = rating(distance_mm, optimal);
    let centimetres = (distance_mm - optimal).abs() / 10.0;
    let verdict = if unreachable {
        // A distance of a few centimetres is no advice. Rated is only whether the narrowest possible value is set.
        let narrowest = narrowest.unwrap_or(0.0);
        if in_game <= narrowest + 1.5 { Verdict::AtMinimum } else { Verdict::ToMinimum(narrowest) }
    } else if grade == Grade::Green {
        Verdict::Good
    } else if deviation > 0.0 {
        Verdict::TooFar(centimetres, grade)
    } else {
        Verdict::TooNear(centimetres, grade)
    };
    Some(Analysis {
        neutral,
        narrowest,
        unreachable,
        game: Game::Known { in_game, optimal: Some(optimal), verdict: Some(verdict) },
    })
}

/// `'%g' % round(value, 1)` for a distance in centimetres: as the field shows what was typed (60, 62.5).
pub fn centimetres_text(distance_mm: f64) -> String {
    let text = format!("{:.1}", distance_mm / 10.0);
    text.strip_suffix(".0").map_or(text.clone(), str::to_string)
}

/// A distance the player typed, in millimetres: comma or point, blanks around; `None` for no number or one that
/// is not above 0 or beyond [`MAX_DISTANCE_CM`] (4.12 took anything above 0, "inf" and 1e308 included).
pub fn typed_distance_mm(text: &str) -> Option<f64> {
    let centimetres = py_float(text.replace(',', ".").trim())?;
    (centimetres.is_finite() && centimetres > 0.0 && centimetres <= MAX_DISTANCE_CM).then_some(centimetres * 10.0)
}

/// The largest distance of the eyes that is taken: a hundred metres is no screen to sit in front of.
pub const MAX_DISTANCE_CM: f64 = 10_000.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(fov: Option<f64>, width: Option<f64>, height: Option<f64>) -> GameSetting {
        GameSetting { fov, width, height, file: String::new() }
    }

    #[test]
    fn the_neutral_angle_and_the_distance_are_the_two_ends_of_one_formula() {
        // A screen as wide as twice its distance is 90 degrees: 2·atan(1).
        let angle = field_of_view(600.0, 300.0).unwrap();
        assert!((angle - 90.0).abs() < 1e-9, "{angle}");
        assert!((distance_for(600.0, 90.0).unwrap() - 300.0).abs() < 1e-9);
        for (width, distance) in [(700.0, 800.0), (1193.0, 700.0), (344.0, 450.0)] {
            let back = distance_for(width, field_of_view(width, distance).unwrap()).unwrap();
            assert!((back - distance).abs() < 1e-6, "{width} {distance} {back}");
        }
        assert_eq!(field_of_view(0.0, 500.0), None);
        assert_eq!(field_of_view(500.0, -1.0), None);
        assert_eq!(field_of_view(f64::NAN, 500.0), None);
        assert_eq!(distance_for(500.0, 0.0), None);
        assert_eq!(distance_for(500.0, 180.0), None);
        assert_eq!(distance_for(-1.0, 60.0), None);
    }

    #[test]
    fn the_game_keeps_the_vertical_angle_and_the_menu_shows_the_horizontal_one() {
        // Measured at 32:9 (5120 x 1440): menu 134 ↔ file 67.1, menu 124 ↔ file 54.77.
        let wide = setting(Some(67.1), Some(5120.0), Some(1440.0));
        assert!((game_horizontal(&wide).unwrap() - 134.0).abs() < 0.5, "{:?}", game_horizontal(&wide));
        let narrow = setting(Some(MIN_VERTICAL_DEG), Some(5120.0), Some(1440.0));
        let menu = game_horizontal(&narrow).unwrap();
        assert!((menu - narrowest_horizontal(&narrow).unwrap()).abs() < 1e-9, "the narrowest is the smallest value");
        // 16:9 at the smallest value; no resolution, nothing to convert; 0 counts as missing.
        let flat = narrowest_horizontal(&setting(None, Some(1920.0), Some(1080.0))).unwrap();
        assert!((flat - 85.3).abs() < 0.1, "{flat}");
        assert_eq!(game_horizontal(&setting(Some(60.0), None, Some(1080.0))), None);
        assert_eq!(game_horizontal(&setting(Some(0.0), Some(1920.0), Some(1080.0))), None);
        assert_eq!(narrowest_horizontal(&setting(Some(60.0), Some(0.0), Some(1080.0))), None);
    }

    #[test]
    fn the_distance_is_rated_by_its_share_off_with_inclusive_limits() {
        assert_eq!(rating(1000.0, 1000.0), (Grade::Green, 0.0));
        assert_eq!(rating(1080.0, 1000.0).0, Grade::Green, "8 % is green");
        assert_eq!(rating(1081.0, 1000.0).0, Grade::Yellow);
        assert_eq!(rating(750.0, 1000.0), (Grade::Yellow, -0.25), "25 % is yellow");
        assert_eq!(rating(1260.0, 1000.0).0, Grade::Red);
        assert!(rating(1200.0, 1000.0).1 > 0.0, "positive: too far away");
        assert_eq!(rating(500.0, 0.0), (Grade::Red, 0.0));
        assert_eq!(rating(500.0, -5.0), (Grade::Red, 0.0));
    }

    #[test]
    fn a_card_gives_the_size_of_a_pixel_and_the_width_of_the_screen() {
        assert!((mm_per_pixel(CARD_WIDTH_MM).unwrap() - 1.0).abs() < 1e-12);
        let per_pixel = mm_per_pixel(342.4).unwrap();
        assert!((per_pixel - 0.25).abs() < 1e-12);
        assert!((screen_width_mm(2560.0, per_pixel).unwrap() - 640.0).abs() < 1e-9);
        assert_eq!(mm_per_pixel(0.0), None);
        assert_eq!(screen_width_mm(1920.0, 0.0), None);
        assert_eq!(screen_width_mm(-1.0, 0.2), None);
    }

    #[test]
    fn the_file_is_read_by_its_first_line_of_a_name_anywhere_and_a_text_is_no_number() {
        let text = "<Attributes>\n <Attr name=\"FOV\" value=\"54.7681\"/>\n <Attr name=\"Width\" value=\"2560\"/>\n <Attr name=\"Height\" value=\"1440\"/>\n</Attributes>\n";
        let read = game_setting_text(text);
        assert_eq!((read.fov, read.width, read.height), (Some(54.7681), Some(2560.0), Some(1440.0)));
        let odd = game_setting_text(
            "x <Attr  name=\"FOV\"   value=\"viel\"/> <Attr name=\"FOV\" value=\"60\"/> <Attr name=\"Width\" value=\"1e3\"/>",
        );
        assert_eq!(odd.fov, None, "the first line counts, and it is no number");
        assert_eq!(odd.width, Some(1000.0));
        assert_eq!(game_setting_text(""), GameSetting::default());
    }

    #[test]
    fn the_analysis_follows_the_page_of_4_12_step_by_step() {
        let screen = 700.0; // mm
        // 16:9, 60 degrees vertical = about 91.5 horizontal; sitting at 40 cm gives about 82.
        let game = setting(Some(60.0), Some(1920.0), Some(1080.0));
        let a = analyse(screen, 250.0, &game).unwrap();
        let in_game = game_horizontal(&game).unwrap();
        assert!(!a.unreachable);
        let Game::Known { in_game: shown, optimal: Some(optimal), verdict: Some(verdict) } = a.game else {
            panic!("{a:?}")
        };
        assert_eq!(shown, in_game);
        // A fixed number, not the formula again: 60 degrees vertical at 16:9 is 91.5 horizontal, and a screen 700 mm
        // wide has its neutral point at 700 / (2 · tan(45.75°)) = 341 mm.
        assert!((in_game - 91.5).abs() < 0.1, "{in_game}");
        assert!((optimal - 341.0).abs() < 1.0, "{optimal}");
        assert!(matches!(verdict, Verdict::TooNear(cm, Grade::Red | Grade::Yellow) if cm > 0.0), "{verdict:?}");
        // At the optimal point itself: good.
        let a = analyse(screen, optimal, &game).unwrap();
        assert!(matches!(a.game, Game::Known { verdict: Some(Verdict::Good), .. }));
        // Far away (but not so far that the neutral angle is below what the game allows): too far.
        let a = analyse(screen, optimal * 1.12, &game).unwrap();
        assert!(!a.unreachable);
        assert!(matches!(a.game, Game::Known { verdict: Some(Verdict::TooFar(_, Grade::Yellow)), .. }), "{:?}", a.game);
        // No angle or no resolution in the game: the neutral value only.
        assert!(matches!(
            analyse(screen, 400.0, &setting(None, Some(1920.0), Some(1080.0))).unwrap().game,
            Game::Unknown
        ));
        assert!(matches!(analyse(screen, 400.0, &setting(Some(60.0), None, None)).unwrap().game, Game::Unknown));
        // Unusable numbers: nothing.
        assert_eq!(analyse(0.0, 400.0, &game), None);
        assert_eq!(analyse(screen, 0.0, &game), None);
    }

    /// The distance at which a screen of `width_mm` has the neutral angle `angle`.
    fn distance_at(width_mm: f64, angle: f64) -> f64 {
        distance_for(width_mm, angle).unwrap()
    }

    /// The vertical angle that gives `horizontal` at the aspect of `setting`.
    fn vertical_for(horizontal: f64, aspect: f64) -> f64 {
        degrees(2.0 * ((radians(horizontal) / 2.0).tan() / aspect).atan())
    }

    #[test]
    fn the_limits_half_a_degree_and_one_and_a_half_hold_to_a_millionth_of_a_degree() {
        let narrow = setting(Some(MIN_VERTICAL_DEG), Some(5120.0), Some(1440.0));
        let narrowest = narrowest_horizontal(&narrow).unwrap();
        let width = 1190.0;
        // The neutral angle half a degree below the narrowest: reachable by a hair, and not by a hair less.
        let inside = analyse(width, distance_at(width, narrowest - 0.5 + 1e-6), &narrow).unwrap();
        let outside = analyse(width, distance_at(width, narrowest - 0.5 - 1e-6), &narrow).unwrap();
        assert!(!inside.unreachable && outside.unreachable, "{inside:?} {outside:?}");
        // Unreachable: the setting up to 1.5 degrees above the narrowest is "at the minimum", a hair more is not.
        let aspect = 5120.0 / 1440.0;
        let far = distance_at(width, narrowest - 5.0);
        for (offset, at_minimum) in [(1.5 - 1e-6, true), (1.5 + 1e-6, false), (0.0, true)] {
            let game = setting(Some(vertical_for(narrowest + offset, aspect)), Some(5120.0), Some(1440.0));
            let a = analyse(width, far, &game).unwrap();
            assert!(a.unreachable, "{a:?}");
            let Game::Known { verdict: Some(verdict), .. } = a.game else { panic!("{a:?}") };
            assert_eq!(matches!(verdict, Verdict::AtMinimum), at_minimum, "{offset}: {verdict:?}");
            assert_eq!(matches!(verdict, Verdict::ToMinimum(_)), !at_minimum, "{offset}: {verdict:?}");
        }
    }

    #[test]
    fn a_calibration_that_cannot_be_written_is_reported_as_not_kept() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let home = std::env::temp_dir().join(format!("versekit-fov-back-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let home = home.to_string_lossy().replace('\\', "/");
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        assert!(remember(&paths, Some(0.25), Some(2560.0), None));
        // The settings file is a folder now: nothing can be written, nothing may be half done.
        let file = paths.settings_file();
        std::fs::remove_file(&file).ok();
        std::fs::create_dir_all(&file).unwrap();
        assert!(!remember(&paths, Some(0.3), Some(1920.0), None), "reported");
        std::fs::remove_dir_all(&file).unwrap();
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_neutral_angle_below_the_narrowest_is_not_reachable_and_only_the_minimum_is_rated() {
        // 32:9, a screen 1190 mm wide and a very short distance: neutral is wide; a long distance makes it narrow.
        let narrow = setting(Some(MIN_VERTICAL_DEG), Some(5120.0), Some(1440.0));
        let narrowest = narrowest_horizontal(&narrow).unwrap();
        // Distance so large that the neutral angle is well below the narrowest one.
        let a = analyse(1190.0, 3000.0, &narrow).unwrap();
        assert!(a.neutral < narrowest - 0.5 && a.unreachable, "{a:?}");
        assert!(matches!(a.game, Game::Known { verdict: Some(Verdict::AtMinimum), .. }), "{:?}", a.game);
        // A wider setting than the narrowest + 1.5: set the smallest value.
        let wide = setting(Some(MIN_VERTICAL_DEG + 12.0), Some(5120.0), Some(1440.0));
        let a = analyse(1190.0, 3000.0, &wide).unwrap();
        assert!(
            matches!(a.game, Game::Known { verdict: Some(Verdict::ToMinimum(n)), .. } if (n - narrowest).abs() < 1e-9),
            "{:?}",
            a.game
        );
        // Half a degree of room: just inside the limit is still reachable.
        let reachable =
            analyse(1190.0, 1190.0 / (2.0 * ((narrowest - 0.4).to_radians() / 2.0).tan()), &narrow).unwrap();
        assert!(!reachable.unreachable, "{reachable:?}");
    }

    #[test]
    fn the_distance_in_the_field_is_shown_without_a_zero_and_read_back_with_comma_or_point() {
        assert_eq!(centimetres_text(600.0), "60");
        assert_eq!(centimetres_text(625.0), "62.5");
        assert_eq!(centimetres_text(627.0), "62.7");
        assert_eq!(typed_distance_mm("62,5"), Some(625.0));
        assert_eq!(typed_distance_mm("  70 "), Some(700.0));
        assert_eq!(typed_distance_mm("0"), None);
        assert_eq!(typed_distance_mm("-3"), None);
        assert_eq!(typed_distance_mm("viel"), None);
        assert_eq!(typed_distance_mm(""), None);
        assert_eq!(typed_distance_mm("inf"), None);
        assert_eq!(typed_distance_mm("1e308"), None, "beyond any screen");
        assert_eq!(typed_distance_mm("10000"), Some(100_000.0), "the limit itself is taken");
        assert_eq!(typed_distance_mm("10000,1"), None);
    }

    #[test]
    fn the_calibration_is_kept_as_numbers_and_only_what_is_named_is_written() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let home = std::env::temp_dir().join(format!("versekit-fov-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let home = home.to_string_lossy().replace('\\', "/");
        let paths =
            Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home.as_str()), ("HOME", home.as_str())], &home));
        assert_eq!(stored(&paths), Calibration::default());
        assert!(remember(&paths, Some(0.2330), Some(2560.0), None));
        let calibration = stored(&paths);
        assert_eq!(calibration.mm_per_pixel, Some(0.2330), "a fraction survives (no integer helper)");
        assert_eq!(calibration.pixel_width, Some(2560.0));
        assert_eq!(calibration.distance_mm, None);
        assert!((calibration.width_mm().unwrap() - 596.48).abs() < 1e-9);
        // Only the distance: the calibration stays.
        assert!(remember(&paths, None, None, Some(700.0)));
        let calibration = stored(&paths);
        assert_eq!((calibration.mm_per_pixel, calibration.distance_mm), (Some(0.2330), Some(700.0)));
        // A text that is a number counts, like Python's float().
        assert!(paths.set_setting(KEYS[2], Value::str(" 650 ")));
        assert_eq!(stored(&paths).distance_mm, Some(650.0));
        let _ = std::fs::remove_dir_all(home);
    }
}
