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

//! The size of the whole user interface (`main_window.py`: `FONT_LEVELS`,
//! `font_points`, `font_percent`, `auto_points`).
//!
//! One setting in points over the level `klein` (0 … 7); the base text is
//! 10 + points, so `normal` (1 point) is 100 % and 7 points are 155 %. The
//! older setting `schriftgroesse` (a level name) stays beside it: the points
//! only count while they belong to that level.

use crate::jsonio::{Map, Value};
use crate::paths::Host;
use crate::text::py_strip;

/// Level names and their points.
pub const LEVELS: [(&str, i64); 4] = [("klein", 0), ("normal", 1), ("gross", 3), ("sehrgross", 5)];
/// Setting with the level name.
pub const LEVEL_SETTING: &str = "schriftgroesse";
/// Setting with the points.
pub const POINTS_SETTING: &str = "schrift_punkte";
pub const POINTS_RANGE: (i64, i64) = (0, 7);
/// Presets per screen → points; `auto` follows the screen height.
pub const PRESETS: [(&str, Option<i64>); 5] =
    [("auto", None), ("fullhd", Some(1)), ("wqhd", Some(3)), ("uhd125", Some(4)), ("uhd150", Some(6))];

fn level_points(name: &str) -> Option<i64> {
    LEVELS.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

/// The nearest level to a number of points (the larger one on a tie).
pub fn level_for_points(points: i64) -> &'static str {
    LEVELS.iter().min_by_key(|(_, p)| ((p - points).unsigned_abs(), -p)).map(|(name, _)| *name).unwrap_or("normal")
}

/// The level name stored in the settings, read like `paths.setting`.
fn level_of(settings: &Map, host: &Host) -> String {
    match settings.iter().rev().find(|(k, _)| k == LEVEL_SETTING).map(|(_, v)| v) {
        Some(Value::Str(text)) => {
            let text = py_strip(text);
            if text.is_empty() { "normal".into() } else { host.expanduser(text) }
        }
        _ => "normal".into(),
    }
}

/// The size in points (0 … 7) from the settings (`font_points`).
pub fn font_points(settings: &Map, host: &Host) -> i64 {
    let level = level_of(settings, host);
    if let Some((_, Value::Int(text))) = settings.iter().rev().find(|(k, _)| k == POINTS_SETTING) {
        // Beyond i128 only the sign matters for the clamp.
        let stored = match text.parse::<i128>() {
            Ok(n) => n.clamp(i128::from(POINTS_RANGE.0), i128::from(POINTS_RANGE.1)) as i64,
            Err(_) if text.starts_with('-') => POINTS_RANGE.0,
            Err(_) => POINTS_RANGE.1,
        };
        if level_for_points(stored) == level {
            return stored;
        }
    }
    level_points(&level).unwrap_or(1)
}

/// Points as percent of the level `normal`, for display (`font_percent`).
pub fn font_percent(points: i64) -> i64 {
    ((10 + points) as f64 * 100.0 / 11.0).round_ties_even() as i64
}

/// The preset `auto`: points from the screen height in pixels.
pub fn auto_points(screen_height: i64) -> i64 {
    if screen_height >= 2000 {
        6
    } else if screen_height >= 1400 {
        3
    } else {
        1
    }
}

/// How much larger everything is drawn than at `normal`.
pub fn zoom_factor(points: i64) -> f32 {
    (10 + points.clamp(POINTS_RANGE.0, POINTS_RANGE.1)) as f32 / 11.0
}

/// Smallest window at the level `normal`, in points. Every page must fit
/// into it without cutting anything off; at other sizes it grows with
/// [`zoom_factor`].
pub const BASE_MIN_SIZE: (f32, f32) = (880.0, 380.0);

/// Share of the screen a window may take at most (room for task bar and
/// window frame).
pub const SCREEN_SHARE: f32 = 0.95;

/// The smallest window at `points`, in screen points.
pub fn min_window(points: i64) -> (f32, f32) {
    let factor = zoom_factor(points);
    (BASE_MIN_SIZE.0 * factor, BASE_MIN_SIZE.1 * factor)
}

/// The largest size the screen can hold: the smallest window at that size
/// must fit into [`SCREEN_SHARE`] of the screen (size in screen points).
/// Never below 0, even if the screen is smaller than that.
pub fn max_points_for_screen(width: f32, height: f32) -> i64 {
    (POINTS_RANGE.0..=POINTS_RANGE.1)
        .rev()
        .find(|p| {
            let (w, h) = min_window(*p);
            w <= width * SCREEN_SHARE && h <= height * SCREEN_SHARE
        })
        .unwrap_or(POINTS_RANGE.0)
}

/// The settings to store for new points: points (clamped) and the nearest
/// level, as `set_font_size` writes them.
pub fn settings_for(points: i64) -> [(&'static str, Value); 2] {
    let points = points.clamp(POINTS_RANGE.0, POINTS_RANGE.1);
    [(POINTS_SETTING, Value::int(points)), (LEVEL_SETTING, Value::str(level_for_points(points)))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    fn host() -> Host {
        Host::new(Flavor::Posix, &[("HOME", "/home/x")], "/")
    }

    #[test]
    fn window_grows_and_screen_caps() {
        assert_eq!(min_window(1), BASE_MIN_SIZE);
        let (w, _) = min_window(7);
        assert!((w - 880.0 * 17.0 / 11.0).abs() < 1e-3);
        // Full HD (1920 × 1080 points): 7 points need 1360 wide, which fits in 1824.
        assert_eq!(max_points_for_screen(1920.0, 1080.0), 7);
        // A 1366 × 768 laptop: at most 1297 wide → up to 6 points (1280).
        assert_eq!(max_points_for_screen(1366.0, 768.0), 6);
        // Smaller than even the smallest window: 0, never negative.
        assert_eq!(max_points_for_screen(700.0, 400.0), 0);
    }

    /// At its automatic size the window fits into half the screen on Full HD,
    /// WQHD and UHD, with and without Windows scaling.
    #[test]
    fn half_the_screen_is_enough() {
        // (width, height) in screen points: UHD at 150 % scaling is 2560 × 1440.
        for (width, height) in [(1920.0, 1080.0), (2560.0, 1440.0), (3840.0, 2160.0), (2560.0_f32, 1440.0_f32)] {
            let (needed, _) = min_window(auto_points(height as i64));
            assert!(needed <= width / 2.0, "{width} × {height}: {needed} wide, half is {}", width / 2.0);
        }
    }

    #[test]
    fn levels_and_percent() {
        assert_eq!(level_for_points(2), "gross");
        assert_eq!(level_for_points(4), "sehrgross");
        assert_eq!(level_for_points(-5), "klein");
        assert_eq!(font_percent(1), 100);
        assert_eq!(font_percent(7), 155);
        assert_eq!(font_percent(0), 91);
        assert_eq!(auto_points(2160), 6);
        assert_eq!(auto_points(1440), 3);
        assert_eq!(auto_points(1080), 1);
        assert!((zoom_factor(7) - 17.0 / 11.0).abs() < 1e-6);
    }

    #[test]
    fn points_only_count_with_their_level() {
        let with = |level: &str, points: i64| {
            vec![(LEVEL_SETTING.to_string(), Value::str(level)), (POINTS_SETTING.to_string(), Value::int(points))]
        };
        assert_eq!(font_points(&with("gross", 2), &host()), 2);
        assert_eq!(font_points(&with("klein", 2), &host()), 0);
        assert_eq!(font_points(&with("sehrgross", 99), &host()), 7);
        assert_eq!(font_points(&Vec::new(), &host()), 1);
        assert_eq!(settings_for(9)[1].1, Value::str("sehrgross"));
    }
}
