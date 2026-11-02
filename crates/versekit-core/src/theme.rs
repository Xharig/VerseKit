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

//! Color schemes (`scbp/theme.py`), the one place where the program's colors
//! live.
//!
//! Values: all schemes from `scbp/theme.py` of pull request 12 (commit
//! `204c964d`), which `tools/parity_theme.py` reads and compares.
//!
//! Slimmed down compared to Python:
//! - `heat_low` is gone: it equals `bar` in every scheme.
//! - `line2` is gone: round boxes and fields use `line`.
//! - `control`, `control_border` and `hover_fg` are only set where a scheme
//!   differs (KRT); otherwise they are `surface`, `line` and `accent`.
//! - No icon set per scheme: icons are tinted at runtime ([`IconTint`]).
//!
//! Switching takes effect immediately ([`set_current`]); the UI reads the
//! scheme every frame.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::jsonio::{Map, Value};
use crate::paths::Paths;

/// An sRGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

impl Color {
    /// `#rrggbb`; for the tables below, so invalid digits count as 0.
    pub const fn hex(text: &str) -> Color {
        let b = text.as_bytes();
        Color {
            r: hex_digit(b[1]) * 16 + hex_digit(b[2]),
            g: hex_digit(b[3]) * 16 + hex_digit(b[4]),
            b: hex_digit(b[5]) * 16 + hex_digit(b[6]),
        }
    }

    /// Parses `#rrggbb`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Color> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Color::hex(text))
    }

    /// `#rrggbb` in lower case, like `theme.py`.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// The color `share` (0…1) of the way to `other`, like Python's `_mix` in
    /// `pages.py`: each channel rounded, ties to even.
    pub fn mix(self, other: Color, share: f64) -> Color {
        let channel = |a: u8, b: u8| {
            let value = f64::from(a) + (f64::from(b) - f64::from(a)) * share;
            value.round_ties_even().clamp(0.0, 255.0) as u8
        };
        Color { r: channel(self.r, other.r), g: channel(self.g, other.g), b: channel(self.b, other.b) }
    }

    /// Relative luminance after WCAG 2.x (0 = black, 1 = white).
    pub fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// Contrast ratio after WCAG 2.x, 1 … 21. Text needs 4.5, borders and
    /// icons 3.
    pub fn contrast(self, other: Color) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

/// How far the border of a search line goes from the field to the accent.
const SEARCH_BORDER_SHARE: f64 = 0.75;
/// How far the ground of a search line goes from the field to white.
const SEARCH_GROUND_SHARE: f64 = 0.05;

/// One color scheme.
#[derive(Clone, Debug, PartialEq)]
pub struct Scheme {
    /// Value of the setting `farbschema`.
    pub id: &'static str,
    /// Text key of the scheme's name.
    pub label: &'static str,
    /// Window and page background.
    pub bg: Color,
    /// Boxes, cards, rows.
    pub surface: Color,
    /// Bars and headers; lowest shade of the heat map.
    pub bar: Color,
    pub fg: Color,
    /// Muted text; normal color of icons.
    pub sub: Color,
    /// House color: chosen tab, progress, hover border, highlighted icons.
    pub accent: Color,
    /// Dark accent area: emphasized buttons, chosen or hovered list row.
    pub accent_dark: Color,
    /// Base line for borders.
    pub line: Color,
    /// Input and choice fields.
    pub field: Color,
    pub gold: Color,
    /// Hints and warnings.
    pub yellow: Color,
    /// New in the game (craftable since the last patch). Each scheme's value
    /// reaches at least 5:1 on its boxes.
    pub blue: Color,
    /// Errors; text and border of danger buttons.
    pub red: Color,
    pub red_pale: Color,
    /// Hovered or edited row; track of progress bars.
    pub hover: Color,
    /// Background of a chosen entry.
    pub selected: Color,
    pub danger_fill: Color,
    /// Toggle switch, off and on.
    pub track: Color,
    pub track_on: Color,
    pub scroll_groove: Color,
    pub scroll_grip: Color,
    pub scroll_grip_light: Color,
    control: Option<Color>,
    control_border: Option<Color>,
    hover_fg: Option<Color>,
    /// Square boxes with corner brackets in the accent color.
    pub square: bool,
    pub upper_headings: bool,
    pub accent_headings: bool,
    /// Buttons filled with the accent, text in the background color.
    pub filled_buttons: bool,
}

impl Scheme {
    /// Fill of controls (buttons, choice and input fields).
    pub fn control(&self) -> Color {
        self.control.unwrap_or(self.surface)
    }

    /// Visible border of controls.
    pub fn control_border(&self) -> Color {
        self.control_border.unwrap_or(self.line)
    }

    /// Border of a search line without the cursor: the accent, muted towards the field.
    pub fn search_border(&self) -> Color {
        self.field.mix(self.accent, SEARCH_BORDER_SHARE)
    }

    /// Ground of a search line: the field, a little lighter.
    pub fn search_ground(&self) -> Color {
        self.field.mix(Color { r: 255, g: 255, b: 255 }, SEARCH_GROUND_SHARE)
    }

    /// Text color of a hovered button.
    pub fn hover_fg(&self) -> Color {
        self.hover_fg.unwrap_or(self.accent)
    }

    /// Lowest shade of the heat map (Python's `heat_low`).
    pub fn heat_low(&self) -> Color {
        self.bar
    }

    /// Color of a heat map cell (`stats_pages.heatmap`): from `heat_low` to
    /// the accent along the square root of `share` (linear would make only the
    /// strongest cells stand out). Channels are cut off like Python's `int()`.
    pub fn heat(&self, share: f64) -> Color {
        // `powf`, not `sqrt`: Python writes `share ** 0.5`, the C library's pow.
        let share = share.max(0.0).powf(0.5);
        let (low, high) = (self.heat_low(), self.accent);
        let channel = |a: u8, b: u8| {
            let value = f64::from(a) + (f64::from(b) - f64::from(a)) * share;
            value.trunc().clamp(0.0, 255.0) as u8
        };
        Color { r: channel(low.r, high.r), g: channel(low.g, high.g), b: channel(low.b, high.b) }
    }

    /// Heat map cell for `amount` out of the largest value: empty cells take
    /// the line color.
    pub fn heat_cell(&self, amount: f64, highest: f64) -> Color {
        if amount == 0.0 {
            return self.line;
        }
        let highest = if highest == 0.0 { 1.0 } else { highest };
        self.heat(amount / highest)
    }

    /// Fill and border of a control that carries the base line as border
    /// (`theme.control_colors`): becomes the control colors of the scheme.
    pub fn control_colors(&self, fill: Option<Color>, border: Option<Color>) -> (Option<Color>, Option<Color>) {
        match border {
            Some(line) if line == self.line => {
                let fill = match fill {
                    Some(surface) if surface == self.surface => Some(self.control()),
                    other => other,
                };
                (fill, Some(self.control_border()))
            }
            _ => (fill, border),
        }
    }

    /// A heading as the scheme writes it.
    pub fn heading(&self, text: &str) -> String {
        if self.upper_headings { text.to_uppercase() } else { text.to_string() }
    }

    /// Color of an icon in a given state.
    pub fn icon(&self, tint: IconTint) -> Color {
        match tint {
            IconTint::Normal => self.sub,
            IconTint::Highlight => self.accent,
            IconTint::Hover => self.fg,
        }
    }
}

/// State of an icon; the scheme gives the color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconTint {
    Normal,
    Highlight,
    Hover,
}

/// House color of the Profit Basetool, for buttons that lead there; the same
/// in every scheme.
pub const KRT_ORANGE: Color = Color::hex("#e77e23");

/// Setting that stores the scheme.
pub const SETTING: &str = "farbschema";

pub const DEFAULT: &str = "original";

const fn rounded(id: &'static str, label: &'static str, c: [&str; 21]) -> Scheme {
    Scheme {
        id,
        label,
        bg: Color::hex(c[0]),
        surface: Color::hex(c[1]),
        bar: Color::hex(c[2]),
        fg: Color::hex(c[3]),
        sub: Color::hex(c[4]),
        accent: Color::hex(c[5]),
        line: Color::hex(c[6]),
        field: Color::hex(c[7]),
        gold: Color::hex(c[8]),
        red: Color::hex(c[9]),
        red_pale: Color::hex(c[10]),
        yellow: Color::hex(c[11]),
        blue: Color::hex("#4aa3d8"),
        accent_dark: Color::hex(c[12]),
        hover: Color::hex(c[13]),
        track: Color::hex(c[14]),
        track_on: Color::hex(c[15]),
        selected: Color::hex(c[16]),
        danger_fill: Color::hex(c[17]),
        scroll_groove: Color::hex(c[18]),
        scroll_grip: Color::hex(c[19]),
        scroll_grip_light: Color::hex(c[20]),
        control: None,
        control_border: None,
        hover_fg: None,
        square: false,
        upper_headings: false,
        accent_headings: false,
        filled_buttons: false,
    }
}

const fn krt() -> Scheme {
    // Field #2d3337 instead of PR 12's #3f474c: red and accent text in a field
    // and the hint in a search line need 4.5:1. Listed in CHANGED of
    // `tools/parity_theme.py`.
    let mut s = rounded(
        "krt",
        "s_da_krt",
        [
            "#202020", "#35393d", "#2a2d31", "#dadbde", "#b0b3b7", "#fb8a28", "#404145", "#2d3337", "#ffd23f",
            "#ff7a70", "#d98a82", "#eeb64b", "#473321", "#3f474c", "#3f474c", "#784a23", "#594839", "#3a1a1c",
            "#35393d", "#8f9399", "#eeb64b",
        ],
    );
    // Controls have their own fill and a visible border; hovered button text
    // is the light accent.
    s.control = Some(Color::hex("#3f474c"));
    s.control_border = Some(Color::hex("#8f9399"));
    s.hover_fg = Some(Color::hex("#eeb64b"));
    // Lighter than elsewhere: KRT's boxes are lighter.
    s.blue = Color::hex("#5cb0e6");
    s.square = true;
    s.upper_headings = true;
    s.accent_headings = true;
    s.filled_buttons = true;
    s
}

// Order of the colors in `rounded`: bg, surface, bar, fg, sub, accent, line,
// field, gold, red, red_pale, yellow, accent_dark, hover, track,
// track_on, selected, danger_fill, scroll_groove, scroll_grip,
// scroll_grip_light.
static SCHEMES: [Scheme; 6] = [
    rounded(
        "original",
        "s_da_original",
        // Red 1 % lighter than Python's #e05252, so error text on boxes reaches
        // 4.5:1. Listed in CHANGED of `tools/parity_theme.py`.
        [
            "#10141c", "#161c28", "#1b2230", "#e6edf3", "#8b98a5", "#9ce430", "#232c3d", "#0c1017", "#e8c353",
            "#e05454", "#c98a8a", "#d8a03a", "#1d2a14", "#222b3b", "#2b3547", "#2a3a1c", "#1d2634", "#2a1414",
            "#0b0e14", "#5a6b85", "#7d90ad",
        ],
    ),
    krt(),
    // Blue leans to violet here so it stands apart from the cyan accent.
    with_blue(
        rounded(
            "eis",
            "s_da_eis",
            [
                "#0b1219", "#111b25", "#15212d", "#e3f1f7", "#86a0ae", "#38d6f5", "#1d2c3a", "#081017", "#e8c353",
                "#e05252", "#c98a8a", "#d8a03a", "#0e2a33", "#1b2836", "#243446", "#12394a", "#162633", "#2a1414",
                "#070c11", "#4f7184", "#78a6bb",
            ],
        ),
        "#8fa8ff",
    ),
    rounded(
        "nebel",
        "s_da_nebel",
        [
            "#131020", "#1a1629", "#201b32", "#ece8f5", "#9a93ad", "#a78bfa", "#2a2440", "#0e0c18", "#e8c353",
            "#e05252", "#c98a8a", "#d8a03a", "#241c3d", "#262040", "#2f2847", "#352a5c", "#221c36", "#2a1418",
            "#0c0a14", "#6b6190", "#8f84b6",
        ],
    ),
    // Red is the accent here, so the error color is yellow-orange.
    rounded(
        "glut",
        "s_da_glut",
        [
            "#151515", "#1c1c1d", "#222223", "#ebe6e4", "#9d9592", "#ff5a4f", "#2c2a2a", "#0f0f0f", "#e8c353",
            "#ffa53a", "#d9a870", "#d8a03a", "#3a1614", "#2a2727", "#353131", "#4a1d1a", "#2a2222", "#2e1d08",
            "#0c0c0c", "#6a6260", "#9a8e8a",
        ],
    ),
    with_blue(
        rounded(
            "kontrast",
            "s_da_kontrast",
            [
                "#000000", "#0d0d0d", "#141414", "#ffffff", "#c8c8c8", "#ffd400", "#5a5a5a", "#000000", "#ffb000",
                "#ff6b6b", "#ffa3a3", "#ffb000", "#332b00", "#262626", "#444444", "#665500", "#2b2600", "#330000",
                "#000000", "#9a9a9a", "#ffffff",
            ],
        ),
        "#66b8ff",
    ),
];

const fn with_blue(mut s: Scheme, blue: &str) -> Scheme {
    s.blue = Color::hex(blue);
    s
}

/// All schemes, in the order of the settings page.
pub fn schemes() -> &'static [Scheme] {
    &SCHEMES
}

pub fn by_id(id: &str) -> Option<&'static Scheme> {
    SCHEMES.iter().find(|s| s.id == id)
}

fn default_scheme() -> &'static Scheme {
    &SCHEMES[0]
}

/// The scheme stored in the settings; anything unknown is `original`.
pub fn choice_of(settings: &Map) -> &'static Scheme {
    match settings.iter().find(|(k, _)| k == SETTING).map(|(_, v)| v) {
        Some(Value::Str(id)) => by_id(id).unwrap_or_else(default_scheme),
        _ => default_scheme(),
    }
}

/// Index into `SCHEMES` + 1; 0 = not decided yet.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// A scheme only this thread uses (index into `SCHEMES` + 1, 0 = none), for checks that draw in
    /// every scheme while other threads go on in theirs.
    static THREAD_SCHEME: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The scheme in use. Decided once from the settings.
pub fn current() -> &'static Scheme {
    if let Some(scheme) = THREAD_SCHEME.with(std::cell::Cell::get).checked_sub(1).and_then(|i| SCHEMES.get(i)) {
        return scheme;
    }
    match CURRENT.load(Ordering::Relaxed) {
        0 => {
            let scheme = choice_of(&crate::paths::global().settings());
            set_current(scheme.id);
            scheme
        }
        n => SCHEMES.get(n - 1).unwrap_or_else(default_scheme),
    }
}

/// Switches the scheme for this run, at once. Returns `false` for an unknown
/// id (nothing changes).
pub fn set_current(id: &str) -> bool {
    match SCHEMES.iter().position(|s| s.id == id) {
        Some(index) => {
            CURRENT.store(index + 1, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Switches the scheme for this thread only; the scheme of the program and of all other threads
/// stays as it is. Returns `false` for an unknown id (nothing changes).
pub fn set_thread_current(id: &str) -> bool {
    match SCHEMES.iter().position(|s| s.id == id) {
        Some(index) => {
            THREAD_SCHEME.with(|s| s.set(index + 1));
            true
        }
        None => false,
    }
}

/// Remembers the scheme in the settings (`theme.choose`) and switches to it.
pub fn choose(paths: &Paths, id: &str) -> bool {
    if by_id(id).is_none() {
        return false;
    }
    set_current(id);
    paths.set_setting(SETTING, Value::str(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scheme of a thread of its own is not the scheme of the others, whatever they switch.
    #[test]
    fn a_scheme_of_one_thread_does_not_reach_the_others() {
        let (own, other) = (SCHEMES[1].id, SCHEMES[2].id);
        assert!(set_thread_current(own));
        assert!(!set_thread_current("gibt-es-nicht"));
        std::thread::spawn(move || {
            assert!(set_thread_current(other));
            assert_eq!(current().id, other, "the other thread sees the scheme it set");
        })
        .join()
        .expect("thread");
        assert_eq!(current().id, own, "this thread keeps its own scheme");
    }

    #[test]
    fn colors_round_trip() {
        assert_eq!(Color::parse("#9CE430"), Some(Color { r: 0x9c, g: 0xe4, b: 0x30 }));
        assert_eq!(Color::parse("#9ce43"), None);
        assert_eq!(Color::parse("9ce430"), None);
        assert_eq!(Color::hex("#0c1017").to_hex(), "#0c1017");
    }

    #[test]
    fn defaults_and_overrides() {
        let original = by_id("original").expect("original exists");
        assert_eq!(original.control(), original.surface);
        assert_eq!(original.control_border(), original.line);
        assert_eq!(original.hover_fg(), original.accent);
        let krt = by_id("krt").expect("krt exists");
        assert_eq!(krt.control().to_hex(), "#3f474c");
        assert!(krt.square && krt.upper_headings);
        assert_eq!(krt.heading("Straße"), "STRASSE");
        assert_eq!(original.heading("Straße"), "Straße");
        assert_eq!(schemes().len(), 6);
    }

    #[test]
    fn control_colors_like_python() {
        let krt = krt();
        let (fill, border) = krt.control_colors(Some(krt.surface), Some(krt.line));
        assert_eq!((fill, border), (Some(krt.control()), Some(krt.control_border())));
        let (fill, border) = krt.control_colors(Some(krt.bg), Some(krt.line));
        assert_eq!((fill, border), (Some(krt.bg), Some(krt.control_border())));
        assert_eq!(krt.control_colors(Some(krt.surface), Some(krt.accent)), (Some(krt.surface), Some(krt.accent)));
        assert_eq!(krt.control_colors(None, None), (None, None));
    }

    #[test]
    fn choice_and_switch() {
        let with = |v: Value| vec![(SETTING.to_string(), v)];
        assert_eq!(choice_of(&with(Value::str("eis"))).id, "eis");
        assert_eq!(choice_of(&with(Value::str("EIS"))).id, "original");
        assert_eq!(choice_of(&with(Value::int(1))).id, "original");
        assert_eq!(choice_of(&Vec::new()).id, "original");
        assert!(!set_current("gibt-es-nicht"));
        assert!(set_current("glut"));
        assert_eq!(current().id, "glut");
        assert!(set_current("krt"));
        assert_eq!(current().accent.to_hex(), "#fb8a28");
    }

    #[test]
    fn the_krt_colors_are_those_of_pull_request_12() {
        let s = by_id("krt").expect("krt exists");
        let got = [
            (s.bg, "#202020"),
            (s.surface, "#35393d"),
            (s.bar, "#2a2d31"),
            (s.fg, "#dadbde"),
            (s.sub, "#b0b3b7"),
            (s.accent, "#fb8a28"),
            (s.line, "#404145"),
            (s.field, "#2d3337"),
            (s.accent_dark, "#473321"),
            (s.hover, "#3f474c"),
            (s.track, "#3f474c"),
            (s.track_on, "#784a23"),
            (s.selected, "#594839"),
            (s.danger_fill, "#3a1a1c"),
            (s.scroll_groove, "#35393d"),
            (s.scroll_grip, "#8f9399"),
            (s.control(), "#3f474c"),
            (s.control_border(), "#8f9399"),
            (s.hover_fg(), "#eeb64b"),
        ];
        for (color, want) in got {
            assert_eq!(color.to_hex(), want);
        }
    }
}
