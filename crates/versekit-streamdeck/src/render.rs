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

//! What a key shows: its symbol, its name and the state as a word. [`crate::paint`] draws it in the house style.
//! A state shows in a word, never in the colour alone.

/// The colour of a state or of the symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Muted,
    Warning,
    Error,
}

/// The state under the name: a word, on (in the colour of the key) or off (grey).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub word: String,
    pub filled: bool,
    pub tone: Tone,
}

/// What a key shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub icon: &'static str,
    pub icon_tone: Tone,
    pub label: String,
    pub badge: Option<Badge>,
    /// The colour of the key by its name in the house style (`rot`, `blau` …), set by the key; `None` takes the
    /// colour of the action.
    pub colour: Option<&'static str>,
    /// A dangerous game action held down: how far its ring is filled (0 to 1); `None` while none is held.
    pub hold: Option<f32>,
}

/// How a dial moves in one picture of its short series.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// -1 turned left, 1 turned right, 0 not turned.
    pub turn: i8,
    /// How far the turned half of the ring is lit, in degrees from the top (0 to 90), and how brightly (0 to 1).
    pub reach: f32,
    pub glow: f32,
    /// The press, 0 to 1: the card lights up in the colour of the dial and a dot grows in the ring.
    pub pulse: f32,
}

/// The pictures of a turn, 100 ms apart: the quarter of the ring on the turned side and its sign light up in the
/// light colour of the dial for 300 ms; then the dial is at rest again.
pub fn turn_frames(turn: i8) -> Vec<Motion> {
    vec![Motion { turn: turn.signum(), reach: 90.0, glow: 1.0, pulse: 0.0 }; 3]
}

/// The pictures of a press, 100 ms apart: the pulse rises and falls.
pub fn press_frames() -> Vec<Motion> {
    [0.5, 1.0, 0.5, 0.0].into_iter().map(|pulse| Motion { pulse, ..Motion::default() }).collect()
}

/// What the field of a dial (200 × 100 pixels) shows.
#[derive(Clone, Debug, PartialEq)]
pub struct DialLook {
    /// The name, written in capitals above the ring (not in the compact form).
    pub label: String,
    pub icon: &'static str,
    /// The colour of the dial by its name in the house style.
    pub colour: &'static str,
    /// A value the game or VerseKit gives, in place of the symbol; `None` without one: nothing is made up.
    pub value: Option<String>,
    /// On or off, where only that is known: the ring stands full or dimmed; `None` without.
    pub on: Option<bool>,
    /// Why the dial does nothing, as a word in the colour of warnings in place of the symbol.
    pub warning: Option<String>,
    /// Without the name: the value or the symbol larger.
    pub compact: bool,
    /// A dial of the power of the ship (weapons, thrusters, shields …): the power symbol in the hole of the ring.
    pub energy: bool,
    pub motion: Motion,
    /// A dangerous press held down: how far the ring is filled from the left (0 to 1); `None` while none is held.
    pub hold: Option<f32>,
}

/// What an info dial shows (contested zones, readiness, money, ship): the name above, a large value, a line below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoLook {
    pub label: String,
    /// The colour of the dial by its name in the house style.
    pub colour: &'static str,
    /// The value, large in white; a dash where none is known.
    pub value: String,
    /// The line below the value, in the colour of its tone.
    pub line: String,
    pub tone: Tone,
}

/// What the objective field of a dial shows: the running contract the dial stands at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveLook {
    /// `AUFTRAG` or `OBJECTIVE`, in the language of VerseKit.
    pub head: String,
    /// The objective now, or the title of the contract where no objective is known; the text for no contract.
    pub text: String,
    /// `2 von 3`; `None` where the log named no objectives.
    pub stand: Option<String>,
}

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        /// The Lucide symbols a key can show, from the set of the repository.
        const ICONS: &[(&str, &str)] = &[
            $(($name, include_str!(concat!("../../../tools/symbol-vorlagen/", $name, ".svg")))),*
        ];
    };
}

/// The symbols of the power of the ship drawn after its display in the game, in the manner of Lucide
/// (`streamdeck-plugin/symbole-mfd`): the one set beside Lucide a key may name.
const MFD_ICONS: &[(&str, &str)] = &[
    ("mfd-waffen", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-waffen.svg")),
    ("mfd-triebwerke", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-triebwerke.svg")),
    ("mfd-schilde", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-schilde.svg")),
    ("mfd-quantum", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-quantum.svg")),
    ("mfd-lebenserhaltung", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-lebenserhaltung.svg")),
    ("mfd-radar", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-radar.svg")),
    ("mfd-kuehler", include_str!("../../../streamdeck-plugin/symbole-mfd/mfd-kuehler.svg")),
];

/// Every symbol a key can show: the Lucide set and the symbols of the power of the ship.
fn all_icons() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    ICONS.iter().chain(MFD_ICONS.iter())
}

icons!(
    "layers",
    "app-window",
    "scan-text",
    "file-scan",
    "package",
    "boxes",
    "user-round-search",
    "camera",
    "footprints",
    "flask-conical",
    "panel-left",
    "timer",
    "wallet",
    "keyboard",
    "mouse",
    "joystick",
    "gamepad-2",
    "keyboard-off",
    "triangle-alert",
    "circle-question-mark",
    "shield-alert",
    "unplug",
    "loader",
    "hand",
    "folder-open",
    "plane",
    "compass",
    "wrench",
    "clipboard-list",
    "zap",
    "coins",
    "route",
    "hand-coins",
    "chart-no-axes-combined",
    "settings",
    "coffee",
    "layout-dashboard",
    "pickaxe",
    "lock",
    "volume-2",
    // The rest of the Lucide set of the repository: a key of the profiles may name any of them (`symbol`).
    "activity",
    "archive-restore",
    "arrow-down-left",
    "arrow-down-right",
    "arrow-down-up",
    "arrow-left",
    "arrow-left-right",
    "arrow-right",
    "arrow-right-left",
    "arrow-up-left",
    "arrow-up-right",
    "ban",
    "bell",
    "between-horizontal-end",
    "between-vertical-end",
    "bold",
    "bookmark",
    "bug",
    "chart-column",
    "chart-spline",
    "check",
    "check-check",
    "chevron-down",
    "chevron-right",
    "chevron-up",
    "chevrons-down-up",
    "chevrons-up-down",
    "circle",
    "circle-alert",
    "circle-check",
    "circle-dashed",
    "circle-play",
    "circle-x",
    "clipboard-paste",
    "clock",
    "container",
    "copy",
    "crosshair",
    "database",
    "diamond",
    "door-open",
    "download",
    "ellipsis",
    "eraser",
    "external-link",
    "eye",
    "eye-off",
    "factory",
    "fold-horizontal",
    "fold-vertical",
    "folder",
    "funnel-x",
    "gift",
    "graduation-cap",
    "grip",
    "grip-vertical",
    "hamburger",
    "hammer",
    "heart-handshake",
    "house",
    "image-plus",
    "info",
    "italic",
    "landmark",
    "languages",
    "layout-grid",
    "list",
    "list-ordered",
    "lock-open",
    "log-in",
    "map-pin",
    "message-square-text",
    "minus",
    "monitor",
    "move",
    "notebook-pen",
    "orbit",
    "package-minus",
    "package-plus",
    "palette",
    "pause",
    "pencil",
    "pin",
    "pin-off",
    "plane-landing",
    "plane-takeoff",
    "play",
    "plug-zap",
    "plus",
    "puzzle",
    "receipt",
    "rocket",
    "rotate-ccw",
    "rotate-cw",
    "save",
    "scan-eye",
    "scan-search",
    "search",
    "send",
    "server",
    "share-2",
    "shield-check",
    "shield-half",
    "shirt",
    "sliders-horizontal",
    "sparkles",
    "square",
    "star",
    "stethoscope",
    "sticky-note",
    "store",
    "strikethrough",
    "table",
    "tag",
    "target",
    "timer-off",
    "trash-2",
    "underline",
    "undo-2",
    "user",
    "user-check",
    "users",
    "wand-sparkles",
    "warehouse",
    "x",
    "power",
    // The symbols the keys of the shipped profiles name (`symbol`), from the Lucide set.
    "anchor",
    "armchair",
    "biceps-flexed",
    "chevrons-right-left",
    "door-closed",
    "droplet",
    "face-angry",
    "face-grinning",
    "face-neutral",
    "flame",
    "flashlight",
    "gauge",
    "glasses",
    "handshake",
    "hard-hat",
    "heart-pulse",
    "hourglass",
    "locate-fixed",
    "locate-off",
    "log-out",
    "map",
    "megaphone",
    "moon",
    "mouse-pointer-click",
    "music",
    "octagon-x",
    "party-popper",
    "person-standing",
    "pointer",
    "radar",
    "rotate-3d",
    "siren",
    "skull",
    "smartphone",
    "sun",
    "sword",
    "swords",
    "thumbs-down",
    "thumbs-up",
    "wind",
);

/// The elements inside the `<svg>` of a symbol; empty for one that is not in the set.
pub fn icon_body(name: &str) -> &'static str {
    let Some((_, svg)) = all_icons().find(|(n, _)| *n == name) else { return "" };
    let start = svg.find("<svg").and_then(|at| svg[at..].find('>').map(|end| at + end + 1)).unwrap_or(0);
    let end = svg.rfind("</svg>").unwrap_or(svg.len());
    svg[start..end].trim()
}

/// A white symbol on nothing, `size` wide: the symbols of the action list of the Stream Deck program.
pub fn list_symbol(icon: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
        icon_body(icon)
    )
}

/// A symbol in `colour` (`#rrggbb`) on nothing, 72 wide: what the Stream Deck program shows in the circle of a dial,
/// without any writing, so nothing there needs a language.
pub fn dial_symbol(icon: &str, colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="72" height="72" viewBox="-6 -6 36 36" fill="none" stroke="{colour}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
        icon_body(icon)
    )
}

#[cfg(test)]
pub fn icon_known(name: &str) -> bool {
    all_icons().any(|(n, _)| *n == name)
}

/// The symbol of the set with this name (as a key setting names it); `None` for one that is not in the set.
pub fn known_icon(name: &str) -> Option<&'static str> {
    all_icons().find(|(n, _)| *n == name).map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_is_found_and_has_drawing_elements() {
        for (name, _) in all_icons() {
            let body = icon_body(name);
            assert!(body.starts_with('<') && !body.contains("<svg") && !body.contains("<!--"), "{name}: {body}");
        }
        assert_eq!(icon_body("gibt-es-nicht"), "");
    }

    #[test]
    fn the_symbols_of_the_power_are_the_seven_of_the_ship_in_the_manner_of_lucide() {
        let names: Vec<&str> = MFD_ICONS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            ["mfd-waffen", "mfd-triebwerke", "mfd-schilde", "mfd-quantum", "mfd-lebenserhaltung", "mfd-radar", "mfd-kuehler"]
        );
        for (name, svg) in MFD_ICONS {
            assert_eq!(known_icon(name), Some(*name));
            // Drawn as Lucide draws: lines of 2 in the colour given, nothing filled.
            for mark in [r#"viewBox="0 0 24 24""#, r#"fill="none""#, r#"stroke="currentColor""#, r#"stroke-width="2""#] {
                assert!(svg.contains(mark), "{name}: {mark}");
            }
        }
        // None of them stands among the Lucide set, and none of the Lucide set is called mfd-.
        assert!(ICONS.iter().all(|(n, _)| !n.starts_with("mfd-")));
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/symbol-vorlagen");
        assert!(std::fs::read_dir(folder).expect("set").flatten().all(|e| !e.file_name().to_string_lossy().starts_with("mfd-")));
    }
}
