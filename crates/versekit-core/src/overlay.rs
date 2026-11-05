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

//! The overlay above the game, without a window: where it sits (corners,
//! bar side, work area, remembered place), what it shows (rows), when it is
//! open, folded to its bar or waiting as a strip, and when clicks pass
//! through it. The window itself and the drawing live elsewhere; everything
//! here is plain numbers and can be checked without a screen.
//!
//! All places are screen pixels. The remembered place in `watcher.json` is
//! in points of the system scaling, as 4.12 writes it; [`to_pixels`] and
//! [`to_points`] convert at that boundary.

use std::collections::VecDeque;

use crate::jsonio::{self, Value};
use crate::language::{self, Phrase};
use crate::paths::Paths;
use crate::text::py_strip;

/// The file with the remembered place of the overlay (`{"geometry": …}`).
pub const GEOMETRY_FILE: &str = "watcher.json";
/// Width of the overlay on the very first start, in points; its height comes
/// from the rows it should show (`versekit_ui::overlay::default_size`).
pub const DEFAULT_WIDTH: i32 = 440;
/// Gap between the overlay and the screen edge in a corner.
pub const EDGE: i32 = 8;
/// Lowest height when resizing at the grip.
pub const MIN_RESIZE_HEIGHT: i32 = 160;
/// How much an open overlay is taller than its bar at least.
pub const OPEN_EXTRA_HEIGHT: i32 = 120;
/// A narrow bar is never narrower than this.
pub const MIN_BAR_WIDTH: i32 = 260;
/// A drag of at most this many pixels is a click and keeps the corner.
pub const DRAG_SLACK: i32 = 2;
/// How many rows the overlay keeps without a setting, and the fewest and most a setting may ask for.
pub const ROWS_DEFAULT: i64 = 20;
pub const ROWS_RANGE: (i64, i64) = (2, 100);
/// How many contracts the game lets a player hold, as long as the player set nothing else.
pub const CONTRACTS_MAX_DEFAULT: i64 = 10;
pub const CONTRACTS_MAX_RANGE: (i64, i64) = (1, 50);
/// Fewer blueprints caught up at once are shown one by one.
pub const CAUGHT_UP_ROWS: usize = 10;
/// The hotkey without a setting, the same on every system. Without Strg: under Wayland no program can keep a key
/// from the game, and Strg alone makes the figure lie down. A hotkey the player saved stays.
pub const DEFAULT_HOTKEY: &str = "Alt+Umschalt+V";
/// The hotkey that reads the refinery terminal, without a setting (R for refinery), in the same way.
pub const DEFAULT_SCAN_HOTKEY: &str = "Alt+Umschalt+R";
/// The hotkey that puts the cursor into the handle search of the info bar, without a setting (H for handle). It is
/// off until switched on ([`keys::HOTKEY_SEARCH_ON`]).
pub const DEFAULT_SEARCH_HOTKEY: &str = "Alt+Umschalt+H";
/// The info bar stands over the running game while its setting is missing; a stored setting wins.
pub const INFO_BAR_DEFAULT: bool = true;
/// The info bar waits hidden without the game while its setting is missing; a stored setting wins.
pub const INFO_BAR_ALWAYS_DEFAULT: bool = false;
/// After a game exit, a log written this recently still counts as running
/// while the process list says the game is gone.
pub const EXIT_QUIET_SECONDS: f64 = 30.0;

/// The settings the overlay reads and writes, as 4.12 names them.
pub mod keys {
    pub const CLICK_THROUGH: &str = "durchklickbar";
    pub const AUTO_THROUGH: &str = "durchklick_auto";
    pub const ROWS: &str = "max_zeilen";
    pub const HOTKEY: &str = "hotkey";
    pub const HOTKEY_ON: &str = "hotkey_an";
    /// The hotkey of the refinery scan is registered (only a literal `false` switches it off).
    pub const HOTKEY_SCAN_ON: &str = "hotkey_scan_an";
    pub const HOTKEY_SCAN: &str = "hotkey_scan";
    /// The hotkey of the handle search in the info bar is registered (only a literal `true` switches it on).
    pub const HOTKEY_SEARCH_ON: &str = "hotkey_suche_an";
    pub const HOTKEY_SEARCH: &str = "hotkey_suche";
    pub const CORNER: &str = "overlay_ecke";
    pub const BAR: &str = "overlay_leiste";
    pub const SOUND: &str = "signalton";
    pub const MODE: &str = "overlay_modus";
    pub const POPUP_SECONDS: &str = "popup_sekunden";
    pub const OPACITY: &str = "deckkraft_prozent";
    pub const INTERVAL: &str = "pruefintervall_sekunden";
    /// A counter the main window raises to ask the overlay to read all logs again (the account was changed):
    /// the overlay does it whenever the number is not the one it saw last.
    pub const RESCAN_NR: &str = "einlesen_nr";
    /// What the last reading of all logs came to ([`super::RescanResult`]), for the main window to tell.
    pub const RESCAN_RESULT: &str = "einlesen_ergebnis";
    /// The overlay was folded to its bar by hand (4.12 `eingeklappt`); it starts so.
    pub const FOLDED: &str = "eingeklappt";
    /// The icon next to the clock, and the messages of the system.
    pub const TRAY: &str = "tray";
    pub const SYSTEM_NOTICE: &str = "systemmeldung";
    /// The line of the Executive Hangar in the open overlay (the page Contested Zones sets it).
    pub const HANGAR_TIMER: &str = "overlay_hangar_timer";
    /// The own frame rate of `r_DisplayInfo` read from the picture of the game, beside the shard (off by default).
    pub const FPS_LINE: &str = "overlay_fps_zeile";
    /// The frame rate of the server from the same picture; without the key it follows [`FPS_LINE`].
    pub const SERVER_FPS: &str = "overlay_server_fps";
    /// The players on the shard and on the own server from the same picture; without the key it follows
    /// [`FPS_LINE`].
    pub const PLAYERS: &str = "overlay_spieler";
    /// The info bar: a small window of its own with the shard and the scanned material
    /// ([`super::INFO_BAR_DEFAULT`] without the key).
    pub const INFO_BAR: &str = "infoleiste";
    /// The info bar shows while the game does not run too ([`super::INFO_BAR_ALWAYS_DEFAULT`] without the key:
    /// without the game it waits hidden).
    pub const INFO_BAR_ALWAYS: &str = "infoleiste_ohne_spiel";
    /// The size of the overlay window in percent of the main window's size (one of [`super::SIZE_STEPS`]).
    pub const SIZE: &str = "overlay_groesse_prozent";
    /// The size of the info bar with its card and lines in percent of the main window's size, in the same steps.
    pub const INFO_BAR_SIZE: &str = "infoleiste_groesse_prozent";
    /// One switch for each event that may open the overlay (see [`super::Event`]).
    pub const EVENT_BLUEPRINT: &str = "overlay_bei_bauplan";
    pub const EVENT_SCANNER: &str = "overlay_bei_scanner";
    pub const EVENT_CONTRACT: &str = "overlay_bei_auftrag";
    pub const EVENT_REFINERY: &str = "overlay_bei_raffinerie";
    pub const EVENT_CATALOG: &str = "overlay_bei_katalog";
    /// One switch for each part the open overlay shows (see [`super::Section`]).
    pub const SHOW_SHARD: &str = "overlay_zeige_server";
    pub const SHOW_LOCATION: &str = "overlay_zeige_ort";
    pub const SHOW_CONTRACTS: &str = "overlay_zeige_auftraege";
    pub const SHOW_UPDATE: &str = "overlay_zeige_update";
    pub const SHOW_REFINERY: &str = "overlay_zeige_raffinerie";
    /// How many contracts the game lets a player hold, for the count in the head of the bar
    /// ([`super::CONTRACTS_MAX_DEFAULT`] without the key).
    pub const CONTRACTS_MAX: &str = "overlay_auftraege_hoechstzahl";
}

// ------------------------------------------------------------ places

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn from_area((x, y, w, h): crate::screen::Area) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }

    /// Whether the point lies inside (right and bottom edge excluded).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.x <= x && x < self.right() && self.y <= y && y < self.bottom()
    }

    /// Whether `other` lies wholly inside.
    pub fn holds(&self, other: Rect) -> bool {
        self.x <= other.x && other.right() <= self.right() && self.y <= other.y && other.bottom() <= self.bottom()
    }

    /// The smallest rectangle around both.
    pub fn union(&self, other: Rect) -> Rect {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Rect { x, y, w: self.right().max(other.right()) - x, h: self.bottom().max(other.bottom()) - y }
    }

    /// The same rectangle relative to the point `(x, y)`.
    pub fn relative_to(&self, x: i32, y: i32) -> Rect {
        Rect { x: self.x - x, y: self.y - y, ..*self }
    }
}

/// Where the overlay is pinned (`overlay_ecke`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    #[default]
    Free,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

const CORNERS: [(Corner, &str); 5] = [
    (Corner::Free, "frei"),
    (Corner::TopLeft, "oben-links"),
    (Corner::TopRight, "oben-rechts"),
    (Corner::BottomLeft, "unten-links"),
    (Corner::BottomRight, "unten-rechts"),
];

impl Corner {
    /// The corner of a stored value; anything unknown is free, as in 4.12.
    pub fn from_setting(value: Option<&str>) -> Corner {
        let value = value.map(py_strip).unwrap_or_default();
        CORNERS.iter().find(|(_, name)| *name == value).map_or(Corner::Free, |(c, _)| *c)
    }

    pub fn setting(self) -> &'static str {
        CORNERS.iter().find(|(c, _)| *c == self).map_or("frei", |(_, name)| name)
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, Corner::BottomLeft | Corner::BottomRight)
    }

    pub fn is_right(self) -> bool {
        matches!(self, Corner::TopRight | Corner::BottomRight)
    }

    pub fn is_left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }
}

/// On which edge of the overlay its bar sits (`overlay_leiste`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarSide {
    #[default]
    Top,
    Bottom,
}

impl BarSide {
    /// The bar side: its own setting; without one, a bottom corner puts it
    /// at the bottom (4.12 `_leiste_seite_wunsch`).
    pub fn from_settings(bar: Option<&str>, corner: Option<&str>) -> BarSide {
        match bar.map(py_strip) {
            Some("unten") => BarSide::Bottom,
            Some("oben") => BarSide::Top,
            _ if corner.map(py_strip).is_some_and(|c| c.starts_with("unten")) => BarSide::Bottom,
            _ => BarSide::Top,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            BarSide::Top => "oben",
            BarSide::Bottom => "unten",
        }
    }
}

/// Corner and bar side together: everything that decides a place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub corner: Corner,
    pub bar: BarSide,
}

impl Layout {
    /// Which window edges stay put: `(bottom, right)`. A bar at the bottom
    /// fixes the bottom edge also without a corner, so the grip never lies
    /// on the bar.
    pub fn anchored(&self) -> (bool, bool) {
        (self.corner.is_bottom() || self.bar == BarSide::Bottom, self.corner.is_right())
    }
}

/// Where a window of `w` × `h` goes: into its corner on the work area, or,
/// free, where it stands, keeping the edge of the bar (4.12 `_klapp_ecke`).
pub fn corner_place(layout: Layout, w: i32, h: i32, current: Rect, work: Rect) -> (i32, i32) {
    if layout.corner == Corner::Free {
        let y = if layout.bar == BarSide::Bottom { current.y + current.h - h } else { current.y };
        return (current.x, y);
    }
    let x = if layout.corner.is_left() { work.x + EDGE } else { work.x + work.w - w - EDGE };
    let y = if layout.corner.is_bottom() { work.y + work.h - h - EDGE } else { work.y + EDGE };
    // Rather lose the gap than push the bar over the top or left edge.
    (x.max(work.x), y.max(work.y))
}

/// The window kept on its work area: never larger than it, and in a corner
/// wholly on it; free windows keep their place (4.12 `_in_arbeitsflaeche`).
pub fn fit_work_area(corner: Corner, r: Rect, work: Rect) -> Rect {
    let w = r.w.min(work.w - 2 * EDGE).max(1);
    let h = r.h.min(work.h - 2 * EDGE).max(1);
    let (mut x, mut y) = (r.x, r.y);
    if corner != Corner::Free {
        x = x.min(work.x + work.w - w - EDGE).max(work.x);
        y = y.min(work.y + work.h - h - EDGE).max(work.y);
    }
    Rect { x, y, w, h }
}

/// The window of `size` placed for `layout` and kept on the work area.
pub fn place(layout: Layout, size: (i32, i32), current: Rect, work: Rect) -> Rect {
    let (x, y) = corner_place(layout, size.0, size.1, current, work);
    fit_work_area(layout.corner, Rect::new(x, y, size.0, size.1), work)
}

/// The window resized at the grip from its free corner: the anchored edges
/// stay, the size follows the pointer within the screen (4.12 `_resize`).
pub fn resized(layout: Layout, window: Rect, pointer: (i32, i32), min_width: i32, work: Option<Rect>) -> Rect {
    let (bottom, right) = layout.anchored();
    let (max_w, max_h) = work.map_or((100_000, 100_000), |a| (a.w - 16, a.h - 16));
    let w = if right { window.right() - pointer.0 } else { pointer.0 - window.x };
    let w = w.max(min_width).min(max_w);
    let x = if right { window.right() - w } else { window.x };
    let h = if bottom { window.bottom() - pointer.1 } else { pointer.1 - window.y };
    let h = h.max(MIN_RESIZE_HEIGHT).min(max_h);
    let y = if bottom { window.bottom() - h } else { window.y };
    Rect { x, y, w, h }
}

/// A window let go after a drag at most this many pixels from an edge of its screen goes onto that edge.
pub const SNAP: i32 = 16;

/// The window `r` held wholly inside `bounds`; a window larger than `bounds` keeps its left or top edge on it.
pub fn held_inside(r: Rect, bounds: Rect) -> Rect {
    let x = r.x.min(bounds.right() - r.w).max(bounds.x);
    let y = r.y.min(bounds.bottom() - r.h).max(bounds.y);
    Rect { x, y, ..r }
}

/// The window `r` let go after a drag on the screen `screen` with its work area `work`: an edge at most [`SNAP`]
/// pixels from an edge of the screen or of the work area goes onto the nearest of them, and the window then lies
/// wholly on the screen ([`held_inside`]). The size stays.
pub fn snapped(r: Rect, screen: Rect, work: Rect) -> Rect {
    let near = |at: i32, edges: [i32; 2]| {
        edges.into_iter().filter(|edge| (edge - at).abs() <= SNAP).min_by_key(|edge| (edge - at).abs())
    };
    let x = near(r.x, [screen.x, work.x]).or_else(|| near(r.right(), [screen.right(), work.right()]).map(|e| e - r.w));
    let y =
        near(r.y, [screen.y, work.y]).or_else(|| near(r.bottom(), [screen.bottom(), work.bottom()]).map(|e| e - r.h));
    held_inside(Rect { x: x.unwrap_or(r.x), y: y.unwrap_or(r.y), ..r }, screen)
}

/// A size limited to a work area with a 16 pixel margin (4.12 `groesse_begrenzen`).
pub fn limit_size(w: i32, h: i32, work: Rect) -> (i32, i32) {
    (w.min(work.w - 16).max(1), h.min(work.h - 16).max(1))
}

/// A window of `w` × `h` in the middle of a screen, never over its top or
/// left edge (4.12 `screen.centered`).
pub fn centered(w: i32, h: i32, screen: Rect) -> Rect {
    Rect { x: screen.x + ((screen.w - w) / 2).max(0), y: screen.y + ((screen.h - h) / 2).max(0), w, h }
}

/// Width, height and, if given, the place of a remembered window.
pub type Geometry = (i32, i32, Option<(i32, i32)>);

/// A remembered place, `BxH+X+Y` or `BxH`, as 4.12 writes it; `+-571` is
/// a negative place.
pub fn parse_geometry(text: &str) -> Option<Geometry> {
    let (size, place) = match text.split_once('+') {
        Some((size, rest)) => (size, Some(rest)),
        None => (text, None),
    };
    let (w, h) = size.split_once('x')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(w) || !digits(h) {
        return None;
    }
    let number = |s: &str| {
        let body = s.strip_prefix('-').unwrap_or(s);
        if digits(body) { s.parse::<i32>().ok() } else { None }
    };
    let place = match place {
        None => None,
        Some(rest) => {
            let (x, y) = rest.split_once('+')?;
            Some((number(x)?, number(y)?))
        }
    };
    Some((w.parse().ok()?, h.parse().ok()?, place))
}

/// The place as 4.12 writes it: `541x387+-571+15`.
pub fn geometry_text(r: Rect) -> String {
    format!("{}x{}+{}+{}", r.w, r.h, r.x, r.y)
}

/// Points of the system scaling to screen pixels.
pub fn to_pixels(r: Rect, scale: f32) -> Rect {
    let px = |v: i32| (v as f32 * scale).round() as i32;
    Rect { x: px(r.x), y: px(r.y), w: px(r.w), h: px(r.h) }
}

/// Screen pixels to points of the system scaling.
pub fn to_points(r: Rect, scale: f32) -> Rect {
    let pt = |v: i32| (v as f32 / scale.max(0.01)).round() as i32;
    Rect { x: pt(r.x), y: pt(r.y), w: pt(r.w), h: pt(r.h) }
}

/// What a start needs to know about the screens, in screen pixels.
pub struct Screens<'a> {
    /// The main screen.
    pub primary: Rect,
    /// The work area of the main screen.
    pub primary_work: Rect,
    /// Whether the title strip of a window at `x`, `y`, `width` lies on a screen.
    pub on_a_screen: &'a dyn Fn(i32, i32, i32) -> bool,
    /// The work area of the monitor at a point.
    pub work_at: &'a dyn Fn(i32, i32) -> Rect,
}

/// A remembered window: its size, and its place if one was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remembered {
    pub w: i32,
    pub h: i32,
    pub place: Option<(i32, i32)>,
}

impl Remembered {
    /// Points of the system scaling to screen pixels.
    pub fn to_pixels(self, scale: f32) -> Remembered {
        let px = |v: i32| (v as f32 * scale).round() as i32;
        Remembered { w: px(self.w), h: px(self.h), place: self.place.map(|(x, y)| (px(x), px(y))) }
    }
}

/// A window of `size` (pixels), limited to the main work area, in the
/// middle of the main screen.
pub fn centered_on_main(size: (i32, i32), screens: &Screens) -> Rect {
    let (w, h) = limit_size(size.0, size.1, screens.primary_work);
    centered(w, h, screens.primary)
}

/// Where the overlay starts (pixels): the remembered place, if the strip of
/// its bar still lies on a screen, with the size limited to that screen.
/// A place off every screen, or a size kept without a place, comes back to
/// the middle of the main screen with the remembered size. Without anything
/// remembered: `default_size` in the middle. With the bar below, the
/// bottom strip is the one to grab.
pub fn start_place(remembered: Option<Remembered>, default_size: (i32, i32), bar: BarSide, screens: &Screens) -> Rect {
    let Some(r) = remembered else { return centered_on_main(default_size, screens) };
    let Some((x, y)) = r.place else { return centered_on_main((r.w, r.h), screens) };
    let grab_y = if bar == BarSide::Bottom { y + r.h - 20 } else { y };
    if !(screens.on_a_screen)(x, grab_y, r.w) {
        return centered_on_main((r.w, r.h), screens);
    }
    let (w, h) = limit_size(r.w, r.h, (screens.work_at)(x, y));
    Rect { x, y, w, h }
}

/// The remembered window from `watcher.json`, in points; `None` without one.
pub fn load_remembered(paths: &Paths) -> Option<Remembered> {
    let value = jsonio::read_json(&paths.app_file(GEOMETRY_FILE)).ok()?;
    let text = value.get("geometry").and_then(Value::as_str)?;
    let (w, h, place) = parse_geometry(text)?;
    Some(Remembered { w, h, place })
}

/// Remembers the place (points) in `watcher.json` as 4.12 writes it.
pub fn save_place(paths: &Paths, place: Rect) -> bool {
    write_geometry(paths, &geometry_text(place))
}

/// Keeps only the size (points), `BxH`: the overlay comes to the middle of
/// the main screen with it, at once if it runs and at the next start.
pub fn save_size_only(paths: &Paths, w: i32, h: i32) -> bool {
    write_geometry(paths, &format!("{w}x{h}"))
}

/// Forgets size and place (`watcher.json` goes): the overlay comes to the middle of the main screen in its
/// default size, at once if it runs and at the next start. True when nothing is left.
pub fn forget_place(paths: &Paths) -> bool {
    match jsonio::remove_file(&paths.app_file(GEOMETRY_FILE)) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

fn write_geometry(paths: &Paths, text: &str) -> bool {
    let target = paths.app_file(GEOMETRY_FILE);
    let value = Value::Object(vec![("geometry".into(), Value::Str(text.to_string()))]);
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() && jsonio::make_dirs(&folder).is_err() {
        return false;
    }
    jsonio::write_text_atomic(&target, &jsonio::dumps(&value, None, false), jsonio::Newline::Keep).is_ok()
}

/// The window a placement starts from: coming back from the waiting strip
/// it is the last open place, otherwise the window as it stands. The strip
/// sits elsewhere than the open window; starting from it would move the
/// overlay with every pop-up.
pub fn placed_from(before: Option<Shown>, current: Rect, last_open: Rect) -> Rect {
    if before == Some(Shown::Strip) { last_open } else { current }
}

/// The strip that stands for a waiting overlay: at the edge and side of the
/// last open place that fit its corner and bar (4.12 `_anfasser_x/_y`).
pub fn strip_place(layout: Layout, last_open: Rect, w: i32, h: i32) -> Rect {
    let (bottom, _) = layout.anchored();
    let x = if layout.corner.is_left() {
        last_open.x
    } else if layout.corner.is_right() {
        last_open.x + (last_open.w - w).max(0)
    } else {
        last_open.x + ((last_open.w - w) / 2).max(0)
    };
    let y = if bottom { last_open.y + (last_open.h - h).max(0) } else { last_open.y };
    Rect { x, y, w, h }
}

// ------------------------------------------------------------ settings

/// When the overlay shows (`overlay_modus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Open while the game runs, folded to its bar otherwise.
    #[default]
    Always,
    /// Waits as a strip and opens for a while on a new blueprint.
    Popup,
}

impl Mode {
    pub fn from_setting(value: Option<&str>) -> Mode {
        if value.map(py_strip) == Some("popup") { Mode::Popup } else { Mode::Always }
    }

    pub fn setting(self) -> &'static str {
        match self {
            Mode::Always => "immer",
            Mode::Popup => "popup",
        }
    }
}

/// What can open a folded or waiting overlay for a while (`overlay_bei_*`): the player chooses which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A blueprint unlocked in the game (also the ones read back).
    Blueprint,
    /// The signature scanner reads a number.
    Scanner,
    /// A hint of a running contract.
    Contract,
    /// A refinery job is over.
    Refinery,
    /// Something is newly craftable in the game.
    Catalog,
}

impl Event {
    pub const ALL: [Event; 5] = [Event::Blueprint, Event::Scanner, Event::Contract, Event::Refinery, Event::Catalog];

    /// The settings key of the switch.
    pub fn key(self) -> &'static str {
        match self {
            Event::Blueprint => keys::EVENT_BLUEPRINT,
            Event::Scanner => keys::EVENT_SCANNER,
            Event::Contract => keys::EVENT_CONTRACT,
            Event::Refinery => keys::EVENT_REFINERY,
            Event::Catalog => keys::EVENT_CATALOG,
        }
    }

    /// What a missing switch means: only the blueprint opens, as in 4.12.
    pub fn default_on(self) -> bool {
        self == Event::Blueprint
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// The events that open the overlay: a set of [`Event`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Events(u8);

impl Default for Events {
    fn default() -> Events {
        Event::ALL.iter().fold(Events(0), |set, &e| set.with(e, e.default_on()))
    }
}

impl Events {
    pub fn contains(self, event: Event) -> bool {
        self.0 & event.bit() != 0
    }

    pub fn with(self, event: Event, on: bool) -> Events {
        Events(if on { self.0 | event.bit() } else { self.0 & !event.bit() })
    }

    /// From the settings: a switch that is missing, or holds no true/false, keeps the default.
    pub fn read(paths: &Paths) -> Events {
        Event::ALL.iter().fold(Events(0), |set, &e| set.with(e, paths.setting_bool(e.key(), e.default_on())))
    }
}

/// A part of the open overlay the player may switch off (`overlay_zeige_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// The shard of the session (with the frame rates) at the right of the first line.
    Shard,
    /// The place the log names, under the status.
    Location,
    /// The bar of the running contracts above the rows.
    Contracts,
    /// The refinery jobs, running with the time left and done, under the contracts.
    Refinery,
    /// The line that a newer version is ready.
    Update,
}

impl Section {
    pub const ALL: [Section; 5] =
        [Section::Shard, Section::Location, Section::Contracts, Section::Refinery, Section::Update];

    /// The settings key of the switch.
    pub fn key(self) -> &'static str {
        match self {
            Section::Shard => keys::SHOW_SHARD,
            Section::Location => keys::SHOW_LOCATION,
            Section::Contracts => keys::SHOW_CONTRACTS,
            Section::Refinery => keys::SHOW_REFINERY,
            Section::Update => keys::SHOW_UPDATE,
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// The parts the open overlay shows: a set of [`Section`]s, all of them by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sections(u8);

impl Default for Sections {
    fn default() -> Sections {
        Section::ALL.iter().fold(Sections(0), |set, &s| set.with(s, true))
    }
}

impl Sections {
    pub fn contains(self, section: Section) -> bool {
        self.0 & section.bit() != 0
    }

    pub fn with(self, section: Section, on: bool) -> Sections {
        Sections(if on { self.0 | section.bit() } else { self.0 & !section.bit() })
    }

    /// From the settings: a switch that is missing stays on.
    pub fn read(paths: &Paths) -> Sections {
        Section::ALL.iter().fold(Sections(0), |set, &s| set.with(s, paths.setting_bool(s.key(), true)))
    }
}

/// Which readings of the scanner are new: a number that was not shown before, or another one than before.
///
/// The scanner state also counts up when the pill only moves; that is no new reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanHits {
    shown: Option<u64>,
}

impl ScanHits {
    /// Takes the number shown now (`None`: none); `true` when it is a new reading. A number that left and
    /// comes back is a new one.
    pub fn update(&mut self, shown: Option<u64>) -> bool {
        let hit = shown.is_some() && shown != self.shown;
        self.shown = shown;
        hit
    }
}

/// A hotkey: modifier keys and one key, as 4.12 accepts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key code: `A`–`Z`, `0`–`9`, `F1` = 0x70 … `F24` = 0x87.
    pub key: u16,
}

/// The highest function key a hotkey can use.
pub const LAST_F_KEY: u16 = 24;

impl Hotkey {
    /// `Strg+Alt+B`; case, spaces and `-` instead of `+` do not matter. At
    /// least one modifier and exactly one key, a letter, a digit or F1–F24
    /// (4.12 `hotkey.parse`, which took F1–F12). Letters beyond A–Z are not taken.
    pub fn parse(text: &str) -> Option<Hotkey> {
        let mut hotkey = Hotkey { ctrl: false, alt: false, shift: false, key: 0 };
        let mut key = None;
        for part in text.replace('-', "+").split('+') {
            let part = py_strip(part);
            if part.is_empty() {
                continue;
            }
            match part.to_lowercase().as_str() {
                "strg" | "ctrl" | "control" => hotkey.ctrl = true,
                "alt" => hotkey.alt = true,
                "umschalt" | "shift" => hotkey.shift = true,
                _ if key.is_some() => return None,
                _ => key = Some(part.to_uppercase()),
            }
        }
        let key = key?;
        hotkey.key = match key.as_bytes() {
            [c] if c.is_ascii_alphanumeric() => u16::from(*c),
            [b'F', rest @ ..] => match std::str::from_utf8(rest).ok()?.parse::<u16>().ok()? {
                n @ 1..=LAST_F_KEY if rest.iter().all(u8::is_ascii_digit) => 0x70 + n - 1,
                _ => return None,
            },
            _ => return None,
        };
        (hotkey.ctrl || hotkey.alt || hotkey.shift).then_some(hotkey)
    }

    /// The hotkey as the settings store it (`Strg+Alt+B`).
    pub fn text(self) -> String {
        let mut parts = Vec::new();
        for (on, name) in [(self.ctrl, "Strg"), (self.alt, "Alt"), (self.shift, "Umschalt")] {
            if on {
                parts.push(name.to_string());
            }
        }
        parts.push(match self.key {
            0x70..=0x87 => format!("F{}", self.key - 0x70 + 1),
            key => char::from_u32(u32::from(key)).map_or_else(String::new, String::from),
        });
        parts.join("+")
    }
}

/// Whether the setting `key` is a literal `false` (4.12 reads its switches of the hotkeys so: anything else is on).
fn literal_false(paths: &Paths, key: &str) -> bool {
    paths.settings().iter().any(|(k, v)| k == key && *v == Value::Bool(false))
}

/// What the settings make of the hotkey that shows the bar, before it is registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyChoice {
    /// `hotkey_an` is a literal `false`.
    Off,
    /// The setting holds a combination 4.12 would not take (no modifier, two keys, a key it does not know): the
    /// text as it stands.
    Wrong(String),
    Combination(Hotkey),
}

impl HotkeyChoice {
    /// The hotkey of the settings, [`DEFAULT_HOTKEY`] where none is set.
    pub fn read(paths: &Paths) -> HotkeyChoice {
        if literal_false(paths, keys::HOTKEY_ON) {
            return HotkeyChoice::Off;
        }
        let text = paths.setting(keys::HOTKEY).unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
        Hotkey::parse(&text).map_or(HotkeyChoice::Wrong(text), HotkeyChoice::Combination)
    }

    /// The hotkey of the refinery scan, [`DEFAULT_SCAN_HOTKEY`] where none is set.
    pub fn read_scan(paths: &Paths) -> HotkeyChoice {
        if !scan_hotkey_on(paths) {
            return HotkeyChoice::Off;
        }
        let text = paths.setting(keys::HOTKEY_SCAN).unwrap_or_else(|| DEFAULT_SCAN_HOTKEY.to_string());
        Hotkey::parse(&text).map_or(HotkeyChoice::Wrong(text), HotkeyChoice::Combination)
    }

    /// The hotkey of the handle search in the info bar, [`DEFAULT_SEARCH_HOTKEY`] where none is set; off unless
    /// switched on.
    pub fn read_search(paths: &Paths) -> HotkeyChoice {
        if !search_hotkey_on(paths) {
            return HotkeyChoice::Off;
        }
        let text = paths.setting(keys::HOTKEY_SEARCH).unwrap_or_else(|| DEFAULT_SEARCH_HOTKEY.to_string());
        Hotkey::parse(&text).map_or(HotkeyChoice::Wrong(text), HotkeyChoice::Combination)
    }

    pub fn hotkey(&self) -> Option<Hotkey> {
        match self {
            HotkeyChoice::Combination(hotkey) => Some(*hotkey),
            HotkeyChoice::Off | HotkeyChoice::Wrong(_) => None,
        }
    }
}

/// Whether the hotkey of the refinery scan is to be registered: off only by a literal `false` (`hotkey_scan_an`).
pub fn scan_hotkey_on(paths: &Paths) -> bool {
    !literal_false(paths, keys::HOTKEY_SCAN_ON)
}

/// Whether the hotkey of the handle search is to be registered: on only by a literal `true` (`hotkey_suche_an`).
pub fn search_hotkey_on(paths: &Paths) -> bool {
    paths.settings().iter().any(|(k, v)| k == keys::HOTKEY_SEARCH_ON && *v == Value::Bool(true))
}

/// One line of the start trail about a hotkey: its name and what came of its registration.
fn trail_line(name: &str, choice: &HotkeyChoice, registered: Result<(), String>) -> String {
    match (choice, registered) {
        (HotkeyChoice::Off, _) => format!("{name}: ausgeschaltet"),
        (HotkeyChoice::Wrong(text), _) => format!("{name}: entfaellt (ungueltig: {text})"),
        (HotkeyChoice::Combination(hotkey), Ok(())) => format!("{name}: {} angemeldet (ok)", hotkey.text()),
        (HotkeyChoice::Combination(_), Err(reason)) => format!("{name}: entfaellt ({reason})"),
    }
}

/// The line of the start trail about the hotkey of the handle search, in the way of [`hotkey_trail`].
pub fn search_hotkey_trail(choice: &HotkeyChoice, registered: Result<(), String>) -> String {
    trail_line("Such-Hotkey", choice, registered)
}

/// The lines of the start trail about the hotkeys, as 4.12 writes them (`hotkey_anmelden`, `hotkey_scan_anmelden`):
/// each hotkey with what came of its registration (`registered`, the reason where the system did not take it).
/// `scan` is `None` where the refinery scan cannot read at all: then 4.12 says nothing of it.
pub fn hotkey_trail(
    choice: &HotkeyChoice,
    registered: Result<(), String>,
    scan: Option<(&HotkeyChoice, Result<(), String>)>,
) -> Vec<String> {
    let mut lines = vec![trail_line("Hotkey", choice, registered)];
    if let Some((scan, registered)) = scan {
        lines.push(trail_line("Scan-Hotkey", scan, registered));
    }
    lines
}

/// The sizes the overlay and the info bar can take, in percent of the main window's size; the first is the default.
pub const SIZE_STEPS: [i64; 6] = [100, 115, 130, 150, 175, 200];

/// The step nearest to `percent`, the smaller one of two as near.
pub fn size_step(percent: i64) -> i64 {
    SIZE_STEPS.into_iter().min_by_key(|step| step.abs_diff(percent)).unwrap_or(SIZE_STEPS[0])
}

/// A size setting as a step: missing or unreadable is the default, anything else the nearest step.
pub fn size_setting(paths: &Paths, key: &str) -> i64 {
    let (smallest, largest) = (SIZE_STEPS[0], SIZE_STEPS[SIZE_STEPS.len() - 1]);
    size_step(paths.setting_int(key, smallest, Some(smallest), Some(largest)))
}

/// Everything the overlay takes from the settings, read fresh each time.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub click_through: bool,
    pub auto_through: bool,
    pub rows: usize,
    pub hotkey: Option<Hotkey>,
    /// The hotkey that reads the refinery terminal.
    pub scan_hotkey: Option<Hotkey>,
    /// The hotkey that puts the cursor into the handle search of the info bar.
    pub search_hotkey: Option<Hotkey>,
    pub layout: Layout,
    pub sound: bool,
    pub mode: Mode,
    pub popup_seconds: i64,
    /// Background opacity in percent; text and icons stay solid.
    pub opacity: i64,
    /// Seconds between two looks at the Game.log.
    pub interval: i64,
    /// The icon next to the clock (Windows).
    pub tray: bool,
    /// The messages of the system, such as a refinery job that is over.
    pub system_notice: bool,
    /// The line with the state of the Executive Hangar in the open overlay.
    pub hangar_timer: bool,
    /// The own frame rate read from the picture of the game. With [`Settings::server_fps`] and
    /// [`Settings::players`] at the same time the consent to grab the screen: any of the three reads it.
    pub fps_line: bool,
    /// The frame rate of the server from the picture.
    pub server_fps: bool,
    /// The players on the shard and on the own server from the picture.
    pub players: bool,
    /// The info bar stands over the game.
    pub info_bar: bool,
    /// The size of the overlay in percent of the main window's size.
    pub size_percent: i64,
    /// The info bar shows while the game does not run too.
    pub info_bar_always: bool,
    /// The size of the info bar in percent of the main window's size.
    pub info_bar_size_percent: i64,
    /// Where the info bar starts at the top while it was not dragged.
    pub info_bar_corner: crate::info_bar::Corner,
    /// Where the player dragged the info bar, per size of the main screen.
    pub info_bar_places: crate::info_bar::Places,
    /// The bridge to scmdb.net runs (`scmdb_bruecke`, off by default).
    pub scmdb: bool,
    /// The signature scanner is on (at the same time the consent to grab the screen).
    pub scanner: bool,
    /// Place, size and the extra line of the window with the scanned material.
    pub scanner_window: crate::signature_window::Layout,
    /// Where the scanner reads: the frame the player drew and the places found.
    pub scanner_hint: crate::signature_window::Hint,
    /// What opens a folded or waiting overlay for a while.
    pub events: Events,
    /// What the open overlay shows besides its rows.
    pub sections: Sections,
    /// The most contracts the game allows at a time, for the head of the bar (`3/10`).
    pub contracts_max: i64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            click_through: false,
            auto_through: true,
            rows: ROWS_DEFAULT as usize,
            hotkey: Hotkey::parse(DEFAULT_HOTKEY),
            scan_hotkey: Hotkey::parse(DEFAULT_SCAN_HOTKEY),
            search_hotkey: None,
            layout: Layout::default(),
            sound: true,
            mode: Mode::Always,
            popup_seconds: 6,
            opacity: 93,
            interval: 3,
            tray: true,
            system_notice: true,
            hangar_timer: false,
            fps_line: false,
            server_fps: false,
            players: false,
            size_percent: SIZE_STEPS[0],
            info_bar: INFO_BAR_DEFAULT,
            info_bar_always: INFO_BAR_ALWAYS_DEFAULT,
            info_bar_size_percent: SIZE_STEPS[0],
            info_bar_corner: crate::info_bar::Corner::default(),
            info_bar_places: crate::info_bar::Places::default(),
            scmdb: false,
            scanner: false,
            scanner_window: crate::signature_window::Layout::default(),
            scanner_hint: crate::signature_window::Hint::default(),
            events: Events::default(),
            sections: Sections::default(),
            contracts_max: CONTRACTS_MAX_DEFAULT,
        }
    }
}

impl Settings {
    /// Whether the picture of the game is read at all: any of the frame rates or the players is on.
    pub fn reads_picture(&self) -> bool {
        self.fps_line || self.server_fps || self.players
    }

    /// The settings with the limits and defaults of 4.12.
    pub fn read(paths: &Paths) -> Settings {
        let corner = paths.setting(keys::CORNER);
        let bar = paths.setting(keys::BAR);
        // Switched off only by a literal `false`, as 4.12 reads both.
        let off = |key: &str| literal_false(paths, key);
        let hotkey = HotkeyChoice::read(paths).hotkey();
        Settings {
            click_through: paths.setting_bool(keys::CLICK_THROUGH, false),
            auto_through: !off(keys::AUTO_THROUGH),
            rows: paths.setting_int(keys::ROWS, ROWS_DEFAULT, Some(ROWS_RANGE.0), Some(ROWS_RANGE.1)) as usize,
            hotkey,
            scan_hotkey: HotkeyChoice::read_scan(paths).hotkey(),
            search_hotkey: HotkeyChoice::read_search(paths).hotkey(),
            layout: Layout {
                corner: Corner::from_setting(corner.as_deref()),
                bar: BarSide::from_settings(bar.as_deref(), corner.as_deref()),
            },
            sound: paths.setting_bool(keys::SOUND, true),
            mode: Mode::from_setting(paths.setting(keys::MODE).as_deref()),
            popup_seconds: paths.setting_int(keys::POPUP_SECONDS, 6, Some(2), Some(60)),
            opacity: paths.setting_int(keys::OPACITY, 93, Some(30), Some(100)),
            interval: paths.setting_int(keys::INTERVAL, 3, Some(1), Some(60)),
            tray: paths.setting_bool(keys::TRAY, true),
            system_notice: paths.setting_bool(keys::SYSTEM_NOTICE, true),
            hangar_timer: paths.setting_bool(keys::HANGAR_TIMER, false),
            fps_line: paths.setting_bool(keys::FPS_LINE, false),
            server_fps: paths.setting_bool(keys::SERVER_FPS, paths.setting_bool(keys::FPS_LINE, false)),
            players: paths.setting_bool(keys::PLAYERS, paths.setting_bool(keys::FPS_LINE, false)),
            size_percent: size_setting(paths, keys::SIZE),
            info_bar: paths.setting_bool(keys::INFO_BAR, INFO_BAR_DEFAULT),
            info_bar_always: paths.setting_bool(keys::INFO_BAR_ALWAYS, INFO_BAR_ALWAYS_DEFAULT),
            info_bar_size_percent: size_setting(paths, keys::INFO_BAR_SIZE),
            info_bar_corner: crate::info_bar::Corner::read(paths),
            info_bar_places: crate::info_bar::Places::read(paths),
            scmdb: crate::scmdb_bridge::enabled(paths),
            scanner: paths.setting_bool(crate::signature_watch::SETTING, false),
            scanner_window: crate::signature_window::Layout::read(paths),
            scanner_hint: crate::signature_window::Hint::read(paths),
            events: Events::read(paths),
            sections: Sections::read(paths),
            contracts_max: paths.setting_int(
                keys::CONTRACTS_MAX,
                CONTRACTS_MAX_DEFAULT,
                Some(CONTRACTS_MAX_RANGE.0),
                Some(CONTRACTS_MAX_RANGE.1),
            ),
        }
    }
}

// ------------------------------------------------------------ reading all logs again

/// What a reading of all logs came to (4.12 `_alles_neu_einlesen`): the logs read, the blueprints added and the
/// contracts the mission log took in or corrected. The overlay keeps it in the settings ([`keys::RESCAN_RESULT`]),
/// the main window tells it as a notice that stays until it is closed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RescanResult {
    /// When it was done (seconds since 1970); a result is told once, and only while it is fresh.
    pub at: f64,
    pub files: usize,
    pub blueprints: usize,
    pub contracts_new: usize,
    pub contracts_corrected: usize,
    /// The logs could not be read; the numbers are zero then.
    pub failed: bool,
}

/// A result older than this is not told any more (the main window was out of sight while it came).
pub const RESCAN_FRESH_SECONDS: f64 = 120.0;

impl RescanResult {
    /// A reading at `at` that could not read the logs.
    pub fn failure(at: f64) -> RescanResult {
        RescanResult { at, failed: true, ..RescanResult::default() }
    }

    /// The sentence of 4.12 with its four numbers, or the one for a reading that failed.
    pub fn phrase(&self) -> Phrase {
        if self.failed {
            return Phrase::new("neu_gelesen_fehler");
        }
        Phrase::new("neu_gelesen")
            .with("0", self.files)
            .with("1", self.blueprints)
            .with("2", self.contracts_new)
            .with("3", self.contracts_corrected)
    }

    pub fn save(&self, paths: &Paths) -> bool {
        let number = |n: usize| Value::int(i64::try_from(n).unwrap_or(i64::MAX));
        let mut fields = vec![
            ("zeit".into(), Value::Float(self.at)),
            ("dateien".into(), number(self.files)),
            ("bauplaene".into(), number(self.blueprints)),
            ("auftraege_neu".into(), number(self.contracts_new)),
            ("auftraege_berichtigt".into(), number(self.contracts_corrected)),
        ];
        if self.failed {
            fields.push(("fehler".into(), Value::Bool(true)));
        }
        paths.set_setting(keys::RESCAN_RESULT, Value::Object(fields))
    }

    /// The result kept in the settings; `None` without one or with one that cannot be read.
    pub fn load(paths: &Paths) -> Option<RescanResult> {
        let settings = paths.settings();
        let (_, value) = settings.iter().find(|(k, _)| k == keys::RESCAN_RESULT)?;
        let number = |key: &str| match value.get(key) {
            Some(Value::Int(text)) => text.parse::<usize>().ok(),
            _ => None,
        };
        let at = match value.get("zeit") {
            Some(Value::Float(at)) => *at,
            Some(Value::Int(text)) => text.parse().ok()?,
            _ => return None,
        };
        Some(RescanResult {
            at,
            files: number("dateien")?,
            blueprints: number("bauplaene")?,
            contracts_new: number("auftraege_neu")?,
            contracts_corrected: number("auftraege_berichtigt")?,
            failed: matches!(value.get("fehler"), Some(Value::Bool(true))),
        })
    }
}

// ------------------------------------------------------------ rows

/// A message that renders in the language shown, or a fixed text.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Phrase(Phrase),
    Plain(String),
}

impl Message {
    pub fn render(&self) -> String {
        match self {
            Message::Phrase(p) => p.to_string(),
            Message::Plain(s) => s.clone(),
        }
    }
}

impl From<Phrase> for Message {
    fn from(p: Phrase) -> Message {
        Message::Phrase(p)
    }
}

/// One row of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A blueprint unlocked in the game (green tick); `caught_up` for one
    /// read back from an earlier session.
    Found { name: String, kind: String, code: String, time: String, caught_up: bool },
    /// Something explained, grey and silent; `contract` ties it to a running
    /// contract so it goes when that ends.
    Hint { text: Message, contract: Option<String> },
    /// Newly craftable in the game (blue); with a watch list title in gold.
    Catalog { name: String, kind: String, time: String, watch_title: Option<String> },
}

impl Row {
    /// The quiet line under a found blueprint: kind, code, time and the mark
    /// for a caught up one, joined by middle dots.
    pub fn found_line(kind: &str, code: &str, time: &str, caught_up: bool) -> String {
        let mut parts: Vec<String> = [kind, code].iter().filter(|p| !p.is_empty()).map(|p| p.to_string()).collect();
        parts.push(time.to_string());
        if caught_up {
            parts.push(language::t("nachlese_marke"));
        }
        parts.join(" · ")
    }
}

/// The rows, newest first, never more than the setting allows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    list: VecDeque<Row>,
}

impl Rows {
    /// Puts a row on top and lets the oldest fall out beyond `max`.
    pub fn push(&mut self, row: Row, max: usize) {
        self.list.push_front(row);
        self.trim(max);
    }

    pub fn trim(&mut self, max: usize) {
        self.list.truncate(max.max(1));
    }

    /// Clears what is shown; the blueprints stay in the collection.
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Takes away the hint rows of a contract that ended.
    pub fn remove_contract(&mut self, contract: &str) {
        self.list.retain(|r| !matches!(r, Row::Hint { contract: Some(c), .. } if c == contract));
    }

    pub fn iter(&self) -> impl Iterator<Item = &Row> {
        self.list.iter()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

// ------------------------------------------------------------ when it shows

/// What the overlay is right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// Open with its rows.
    Open,
    /// Folded to its bar.
    Bar,
    /// Waiting as a narrow strip with a visible hint (pop-up mode).
    Strip,
}

/// Why the overlay is open for a while only, for the hint it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Peek {
    /// Goes back after `f64` more seconds.
    Closes(f64),
    /// Stays because the pointer is on it.
    Held,
}

/// When the overlay is open, folded or waiting: the game opens it and its
/// end folds it to the bar; a find opens a folded or waiting overlay for
/// the pop-up time, held while the pointer is on it; in pop-up mode it
/// waits as a strip and the pointer on the strip opens it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visibility {
    mode: Mode,
    game: bool,
    /// Opened or folded by hand; forgotten when the game starts or ends.
    by_hand: Option<bool>,
    /// Open for a while until then (a find, the pointer on the strip, the hotkey).
    peek_until: Option<f64>,
    hovered: bool,
}

impl Visibility {
    pub fn new(mode: Mode, game: bool) -> Visibility {
        Visibility { mode, game, ..Visibility::default() }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            self.mode = mode;
            self.by_hand = None;
        }
    }

    pub fn game(&self) -> bool {
        self.game
    }

    /// The state the player left it in by hand at the last run (`eingeklappt`): folded or open, until the game
    /// starts or ends. Only the mode "always" has such a state.
    pub fn restore(&mut self, folded: bool) {
        if self.mode == Mode::Always {
            self.by_hand = Some(!folded);
        }
    }

    /// Folded (`true`) or opened (`false`) by hand; `None` while it follows the game.
    pub fn folded_by_hand(&self) -> Option<bool> {
        self.by_hand.map(|open| !open)
    }

    /// The game started or ended: open with the game, folded without it.
    pub fn set_game(&mut self, running: bool) {
        if running != self.game {
            self.game = running;
            self.by_hand = None;
        }
    }

    fn resting(&self) -> Shown {
        match (self.mode, self.by_hand) {
            (Mode::Popup, _) => Shown::Strip,
            (Mode::Always, Some(true)) => Shown::Open,
            (Mode::Always, Some(false)) => Shown::Bar,
            (Mode::Always, None) if self.game => Shown::Open,
            (Mode::Always, None) => Shown::Bar,
        }
    }

    /// A new blueprint: a folded or waiting overlay opens for `seconds`;
    /// several in a row start the time over.
    pub fn found(&mut self, now: f64, seconds: f64) {
        if self.resting() != Shown::Open {
            self.peek_until = Some(now + seconds);
        }
    }

    /// The pointer came onto the window or left it. On the waiting strip it
    /// opens the overlay; on a folded bar it does nothing.
    pub fn set_hovered(&mut self, now: f64, on: bool, seconds: f64) {
        if on && !self.hovered && self.shown(now) == Shown::Strip {
            self.peek_until = Some(now + seconds);
        }
        self.hovered = on;
    }

    /// The fold arrow or the hotkey: open what is closed, close what is open.
    pub fn toggle(&mut self, now: f64, seconds: f64) {
        let open = self.shown(now) == Shown::Open;
        self.peek_until = None;
        match self.mode {
            Mode::Always => self.by_hand = Some(!open),
            Mode::Popup if !open => self.peek_until = Some(now + seconds),
            Mode::Popup => {}
        }
    }

    /// What the overlay is at `now`. A while ended with the pointer still on
    /// it keeps the overlay open until the pointer leaves.
    pub fn shown(&mut self, now: f64) -> Shown {
        let resting = self.resting();
        match self.peek_until {
            Some(_) if resting == Shown::Open => {
                self.peek_until = None;
                Shown::Open
            }
            Some(until) if now < until || self.hovered => Shown::Open,
            Some(_) => {
                self.peek_until = None;
                resting
            }
            None => resting,
        }
    }

    /// Whether and why the overlay is open for a while only.
    pub fn peek(&mut self, now: f64) -> Option<Peek> {
        if self.shown(now) != Shown::Open || self.resting() == Shown::Open {
            return None;
        }
        let until = self.peek_until?;
        Some(if self.hovered { Peek::Held } else { Peek::Closes((until - now).max(0.0)) })
    }
}

// ------------------------------------------------------------ clicks

/// The pointer: hidden by the game, and where it is (screen pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub hidden: bool,
    pub x: i32,
    pub y: i32,
}

/// Should clicks pass through the overlay now?
///
/// * The lock (`click_through`): yes, except where the visible pointer is on
///   the lock in the bar; that one spot stays clickable and is the way back.
/// * Else, with `auto_through`: while the game hides the pointer. Over an
///   overlay that catches clicks the system shows the pointer, so nothing
///   switches while it stands there (4.12 `auto_through_wanted`).
/// * A waiting strip never passes clicks: the pointer on it opens the overlay.
///
/// Unknown pointer: the lock passes clicks; otherwise everything stays as it is.
pub fn passes_clicks(
    settings: &Settings,
    current: bool,
    pointer: Option<Pointer>,
    window: Rect,
    lock: Option<Rect>,
    shown: Shown,
) -> bool {
    if shown == Shown::Strip {
        return false;
    }
    if settings.click_through {
        return !matches!((pointer, lock), (Some(p), Some(lock)) if !p.hidden && lock.contains(p.x, p.y));
    }
    if !settings.auto_through {
        return false;
    }
    let Some(p) = pointer else { return current };
    if window.contains(p.x, p.y) && !current {
        return current;
    }
    p.hidden
}

// ------------------------------------------------------------ the game

/// Does the game run? The process list decides; without it the log alone,
/// with the long quiet time of the contract log (4.12 `auto_update.game_running`).
pub fn game_running(process: Option<bool>, log_age: Option<f64>) -> bool {
    match process {
        Some(true) => true,
        Some(false) => log_age.is_some_and(|age| age < EXIT_QUIET_SECONDS),
        None => log_age.is_some_and(|age| age < crate::mission_log::GAME_IDLE_SEC),
    }
}

/// Seconds since the Game.log was last written, or `None` without one.
pub fn log_age(paths: &Paths) -> Option<f64> {
    let log = paths.game_log(None)?;
    let modified = std::fs::metadata(log).and_then(|m| m.modified()).ok()?;
    Some(std::time::SystemTime::now().duration_since(modified).map_or(0.0, |d| d.as_secs_f64()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1040);
    const SCREEN: Rect = Rect::new(0, 0, 1920, 1080);

    fn layout(corner: Corner, bar: BarSide) -> Layout {
        Layout { corner, bar }
    }

    #[test]
    fn corners_sit_on_the_work_area_with_a_gap() {
        let here = Rect::new(500, 300, 400, 600);
        let at = |c| corner_place(layout(c, BarSide::Top), 400, 600, here, WORK);
        assert_eq!(at(Corner::TopLeft), (8, 8));
        assert_eq!(at(Corner::TopRight), (1920 - 400 - 8, 8));
        assert_eq!(at(Corner::BottomLeft), (8, 1040 - 600 - 8));
        assert_eq!(at(Corner::BottomRight), (1512, 432));
        assert_eq!(at(Corner::Free), (500, 300), "free stays where it is");
    }

    #[test]
    fn a_free_overlay_with_the_bar_below_keeps_its_bottom_edge() {
        // Folding from 600 to 26 high: the bar must not jump up.
        let open = Rect::new(500, 300, 400, 600);
        let (x, y) = corner_place(layout(Corner::Free, BarSide::Bottom), 400, 26, open, WORK);
        assert_eq!((x, y + 26), (500, open.bottom()));
        let (_, y) = corner_place(layout(Corner::Free, BarSide::Top), 400, 26, open, WORK);
        assert_eq!(y, 300, "bar on top keeps the top edge");
    }

    #[test]
    fn a_window_larger_than_the_screen_is_made_smaller_not_pushed_out() {
        let huge = Rect::new(1700, 900, 3000, 2000);
        let fitted = fit_work_area(Corner::BottomRight, huge, WORK);
        assert_eq!(fitted, Rect::new(8, 8, 1920 - 16, 1040 - 16));
        assert!(fitted.right() <= WORK.right() && fitted.bottom() <= WORK.bottom());
        // Free keeps its place, only the size is capped.
        let free = fit_work_area(Corner::Free, huge, WORK);
        assert_eq!((free.x, free.y, free.w), (1700, 900, 1904));
    }

    #[test]
    fn the_grip_grows_away_from_the_anchored_edges() {
        let window = Rect::new(1000, 400, 400, 500);
        // Free, bar on top: top left stays.
        let r = resized(layout(Corner::Free, BarSide::Top), window, (1600, 1100), 300, Some(WORK));
        assert_eq!(r, Rect::new(1000, 400, 600, 700));
        // Bottom right corner: bottom right stays, the window grows up and left.
        let r = resized(layout(Corner::BottomRight, BarSide::Bottom), window, (800, 100), 300, Some(WORK));
        assert_eq!((r.right(), r.bottom()), (window.right(), window.bottom()));
        assert_eq!((r.w, r.h), (600, 800));
        // Never narrower than the bar, never lower than 160.
        let r = resized(layout(Corner::Free, BarSide::Top), window, (1001, 401), 300, Some(WORK));
        assert_eq!((r.w, r.h), (300, MIN_RESIZE_HEIGHT));
        // A free overlay with the bar below fixes the bottom edge.
        assert_eq!(layout(Corner::Free, BarSide::Bottom).anchored(), (true, false));
    }

    #[test]
    fn geometry_reads_and_writes_like_4_12() {
        assert_eq!(parse_geometry("541x387+-571+15"), Some((541, 387, Some((-571, 15)))));
        assert_eq!(parse_geometry("440x1000"), Some((440, 1000, None)));
        assert_eq!(geometry_text(Rect::new(-571, 15, 541, 387)), "541x387+-571+15");
        for wrong in ["", "x", "10x", "10x10+5", "10x10+a+5", "-10x10+0+0", "10x10+0+0+0", "10x10-5-5"] {
            assert_eq!(parse_geometry(wrong), None, "{wrong}");
        }
    }

    fn screens<'a>(on: &'a dyn Fn(i32, i32, i32) -> bool, work: &'a dyn Fn(i32, i32) -> Rect) -> Screens<'a> {
        Screens { primary: SCREEN, primary_work: WORK, on_a_screen: on, work_at: work }
    }

    const DEFAULT: (i32, i32) = (440, 300);

    /// A place off every screen comes back to the middle of the main screen
    /// and keeps the size the user gave it.
    #[test]
    fn a_place_off_every_screen_comes_back_with_its_size() {
        let on = |x: i32, y: i32, w: i32| x + w > 40 && x < 1920 - 40 && (0..1080).contains(&y);
        let work = |_: i32, _: i32| WORK;
        let s = screens(&on, &work);
        let bar = BarSide::Top;
        let lost = Remembered { w: 541, h: 387, place: Some((-30_000, 15)) };
        assert_eq!(start_place(Some(lost), DEFAULT, bar, &s), Rect::new((1920 - 541) / 2, (1080 - 387) / 2, 541, 387));
        // „In die Mitte holen“: the size alone, also centred with it.
        let size_only = Remembered { w: 541, h: 387, place: None };
        assert_eq!(start_place(Some(size_only), DEFAULT, bar, &s), Rect::new(689, 346, 541, 387));
        // Too large for the main screen: shrunk to its work area.
        let huge = Remembered { w: 5000, h: 3000, place: None };
        assert_eq!(start_place(Some(huge), DEFAULT, bar, &s), Rect::new(8, 28, 1920 - 16, 1040 - 16));
        // A place on the screen stays, its size limited to the work area.
        let kept = Remembered { w: 600, h: 2000, place: Some((100, 50)) };
        assert_eq!(start_place(Some(kept), DEFAULT, bar, &s), Rect::new(100, 50, 600, 1040 - 16));
        // Nothing remembered: the default size in the middle.
        assert_eq!(start_place(None, DEFAULT, bar, &s), Rect::new(740, 390, 440, 300));
    }

    /// Three monitors: the main one, one above it, a tall one left above. A
    /// folded overlay remembered just below the main screen lies on none of
    /// them and comes back to the middle of the main screen with its size.
    #[test]
    fn a_folded_bar_below_the_main_screen_comes_back() {
        let monitors = [(0, 0, 5120, 1440), (0, -1440, 5120, 1440), (-1440, -2018, 1440, 3440)];
        let on = |x: i32, y: i32, w: i32| crate::screen::title_visible(x, y, w, &monitors);
        let work = |_: i32, _: i32| Rect::new(0, 0, 5120, 1400);
        let s = Screens {
            primary: Rect::new(0, 0, 5120, 1440),
            primary_work: Rect::new(0, 0, 5120, 1400),
            on_a_screen: &on,
            work_at: &work,
        };
        let lost = Remembered { w: 612, h: 26, place: Some((9, 1451)) };
        for bar in [BarSide::Bottom, BarSide::Top] {
            let back = start_place(Some(lost), DEFAULT, bar, &s);
            assert_eq!(back, Rect::new((5120 - 612) / 2, (1440 - 26) / 2, 612, 26), "{bar:?}");
        }
        // Counterpart: the same window just above the edge stays where it is.
        let seen = Remembered { w: 612, h: 26, place: Some((9, 1400)) };
        assert_eq!(start_place(Some(seen), DEFAULT, BarSide::Top, &s), Rect::new(9, 1400, 612, 26));
    }

    #[test]
    fn with_the_bar_below_its_strip_must_be_on_a_screen() {
        let on = |x: i32, y: i32, w: i32| x + w > 40 && x < 1880 && (0..1070).contains(&y);
        let work = |_: i32, _: i32| WORK;
        let s = screens(&on, &work);
        let low = Remembered { w: 400, h: 900, place: Some((100, 600)) };
        assert_eq!(start_place(Some(low), DEFAULT, BarSide::Top, &s).y, 600);
        let back = start_place(Some(low), DEFAULT, BarSide::Bottom, &s);
        assert_eq!((back.w, back.h, back.y), (400, 900, (1080 - 900) / 2), "back to the middle, size kept");
    }

    #[test]
    fn a_remembered_size_reads_back_in_pixels() {
        let r = Remembered { w: 400, h: 300, place: Some((-571, 15)) };
        assert_eq!(r.to_pixels(1.25), Remembered { w: 500, h: 375, place: Some((-714, 19)) });
        assert_eq!(Remembered { place: None, ..r }.to_pixels(1.5).place, None);
    }

    #[test]
    fn coming_back_from_the_strip_starts_at_the_last_open_place() {
        let open = Rect::new(1000, 200, 400, 600);
        let layout = Layout::default();
        // Ten pop-ups in a row: the open place never moves.
        let mut last = open;
        for _ in 0..10 {
            let strip = strip_place(layout, last, 150, 22);
            let from = placed_from(Some(Shown::Strip), strip, last);
            last = place(layout, (open.w, open.h), from, WORK);
        }
        assert_eq!(last, open);
        // Folding from the open window starts where it stands.
        assert_eq!(placed_from(Some(Shown::Open), open, Rect::default()), open);
    }

    #[test]
    fn corner_and_bar_settings_like_4_12() {
        assert_eq!(Corner::from_setting(Some("unten-rechts")), Corner::BottomRight);
        assert_eq!(Corner::from_setting(Some("irgendwo")), Corner::Free);
        assert_eq!(Corner::from_setting(None), Corner::Free);
        for (c, name) in CORNERS {
            assert_eq!(Corner::from_setting(Some(name)), c);
            assert_eq!(c.setting(), name);
        }
        // Own setting first, then the corner.
        assert_eq!(BarSide::from_settings(Some("oben"), Some("unten-links")), BarSide::Top);
        assert_eq!(BarSide::from_settings(None, Some("unten-links")), BarSide::Bottom);
        assert_eq!(BarSide::from_settings(Some("quer"), Some("oben-links")), BarSide::Top);
        assert_eq!(BarSide::from_settings(Some(" unten "), None), BarSide::Bottom);
    }

    #[test]
    fn the_strip_sits_where_the_overlay_waits() {
        let last = Rect::new(1000, 200, 400, 600);
        assert_eq!(strip_place(layout(Corner::TopLeft, BarSide::Top), last, 100, 20), Rect::new(1000, 200, 100, 20));
        assert_eq!(strip_place(layout(Corner::TopRight, BarSide::Top), last, 100, 20).x, 1300);
        assert_eq!(strip_place(layout(Corner::Free, BarSide::Top), last, 100, 20).x, 1150);
        assert_eq!(strip_place(layout(Corner::BottomLeft, BarSide::Bottom), last, 100, 20).y, 780);
    }

    /// Folded bar and waiting strip lie inside the open place in every
    /// layout: the window can keep the open size while it folds.
    #[test]
    fn bar_and_strip_lie_inside_the_open_place() {
        let work = Rect::new(0, 0, 1920, 1040);
        let corners = [Corner::Free, Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];
        for corner in corners {
            for bar in [BarSide::Top, BarSide::Bottom] {
                let l = layout(corner, bar);
                let open = place(l, (440, 300), Rect::new(700, 300, 440, 300), work);
                let folded = place(l, (MIN_BAR_WIDTH, 30), open, work);
                let strip = strip_place(l, open, 180, 22);
                assert!(open.holds(folded), "{corner:?} {bar:?}: bar {folded:?} outside {open:?}");
                assert!(open.holds(strip), "{corner:?} {bar:?}: strip {strip:?} outside {open:?}");
            }
        }
        let a = Rect::new(10, 20, 30, 40);
        assert_eq!(a.union(Rect::new(0, 50, 5, 30)), Rect::new(0, 20, 40, 60));
        assert!(a.holds(a) && !a.holds(Rect::new(11, 20, 30, 40)));
        assert_eq!(a.relative_to(10, 10), Rect::new(0, 10, 30, 40));
    }

    /// Let go a few pixels before an edge of the screen or of the work area, the window goes onto it; further away
    /// it stays where it was let go, but never beyond the screen.
    #[test]
    fn a_window_let_go_near_an_edge_snaps_onto_it_and_stays_on_the_screen() {
        // 1920 x 1080 with a task bar of 48 at the bottom.
        let screen = Rect::new(0, 0, 1920, 1080);
        let work = Rect::new(0, 0, 1920, 1032);
        let at = |x, y| snapped(Rect::new(x, y, 400, 300), screen, work);
        assert_eq!(at(9, 13), Rect::new(0, 0, 400, 300), "near the top left corner");
        assert_eq!(at(1920 - 400 - 16, 500), Rect::new(1520, 500, 400, 300), "16 pixels: still onto the right edge");
        assert_eq!(at(1920 - 400 - 17, 500), Rect::new(1503, 500, 400, 300), "17 pixels: it stays");
        assert_eq!(at(-12, 500), Rect::new(0, 500, 400, 300), "a little over the edge: back onto it");
        // Above the task bar, onto the work area; over it, onto the bottom of the screen.
        assert_eq!(at(700, 1032 - 300 - 5), Rect::new(700, 732, 400, 300));
        assert_eq!(at(700, 1080 - 300 - 4), Rect::new(700, 780, 400, 300));
        // Far off the screen: held on it.
        assert_eq!(at(5000, -900), Rect::new(1520, 0, 400, 300));
        assert_eq!(at(-5000, 4000), Rect::new(0, 780, 400, 300));
        // In the middle nothing moves.
        assert_eq!(at(600, 400), Rect::new(600, 400, 400, 300));
        // A second screen to the right with negative places above: its own edges.
        let right = Rect::new(1920, -200, 2560, 1440);
        assert_eq!(snapped(Rect::new(1930, -190, 400, 300), right, right), Rect::new(1920, -200, 400, 300));
        // A window larger than the screen keeps its top left corner on it.
        assert_eq!(snapped(Rect::new(300, 200, 3000, 2000), screen, work), Rect::new(0, 0, 3000, 2000));
        assert_eq!(held_inside(Rect::new(10, 20, 30, 40), Rect::new(0, 0, 100, 100)), Rect::new(10, 20, 30, 40));
        assert_eq!(held_inside(Rect::new(90, -5, 30, 40), Rect::new(0, 0, 100, 100)), Rect::new(70, 0, 30, 40));
    }

    #[test]
    fn hotkeys_like_4_12() {
        let b = Hotkey::parse("Strg+Alt+B").unwrap();
        assert_eq!((b.ctrl, b.alt, b.shift, b.key), (true, true, false, u16::from(b'B')));
        assert_eq!(Hotkey::parse("ctrl - shift - f12").unwrap().key, 0x7b);
        assert_eq!(Hotkey::parse("Alt+7").unwrap().key, u16::from(b'7'));
        for wrong in ["", "B", "F13", "Strg+Alt", "Strg+A+B", "Strg+F25", "Strg+F0", "Strg+Ä", "Strg+Tab", "Strg+F1x"]
        {
            assert_eq!(Hotkey::parse(wrong), None, "{wrong}");
        }
    }

    /// F13 to F24 (keys of macro keyboards and of tools such as AutoHotkey) are taken with the Windows key codes
    /// 0x7C to 0x87 and are written back as they were typed.
    #[test]
    fn the_function_keys_run_to_f24() {
        assert_eq!(Hotkey::parse("Strg+F13").unwrap().key, 0x7c);
        assert_eq!(Hotkey::parse("alt + umschalt + f24").unwrap().key, 0x87);
        for n in 1..=LAST_F_KEY {
            let text = format!("Alt+F{n}");
            let hotkey = Hotkey::parse(&text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(hotkey.key, 0x70 + n - 1, "{text}");
            assert_eq!(hotkey.text(), text, "written as read");
        }
    }

    #[test]
    fn rows_stay_newest_first_and_within_the_limit() {
        let mut rows = Rows::default();
        for n in 0..8 {
            let row = Row::Found {
                name: format!("BP {n}"),
                kind: String::new(),
                code: String::new(),
                time: String::new(),
                caught_up: false,
            };
            rows.push(row, 5);
        }
        let names: Vec<String> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Found { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["BP 7", "BP 6", "BP 5", "BP 4", "BP 3"]);
        rows.push(Row::Hint { text: Message::Plain("x".into()), contract: Some("A".into()) }, 5);
        rows.remove_contract("A");
        assert_eq!(rows.len(), 4);
        rows.clear();
        assert!(rows.is_empty());
    }

    #[test]
    fn the_found_line_joins_what_is_known() {
        assert_eq!(
            Row::found_line("Kühler", "Militär · S2 · A", "12:00:01", false),
            "Kühler · Militär · S2 · A · 12:00:01"
        );
        assert_eq!(Row::found_line("", "", "12:00:01", false), "12:00:01");
        assert!(Row::found_line("Helm", "", "08:00:00", true).starts_with("Helm · 08:00:00 · "));
    }

    #[test]
    fn the_overlay_comes_with_the_game_and_folds_without_it() {
        let mut v = Visibility::new(Mode::Always, false);
        assert_eq!(v.shown(0.0), Shown::Bar);
        v.set_game(true);
        assert_eq!(v.shown(1.0), Shown::Open);
        v.toggle(2.0, 6.0);
        assert_eq!(v.shown(2.0), Shown::Bar, "folded by hand");
        // A find opens the folded bar for a while, then it folds again.
        v.found(3.0, 6.0);
        assert_eq!(v.shown(4.0), Shown::Open);
        assert_eq!(v.peek(5.0), Some(Peek::Closes(4.0)));
        assert_eq!(v.shown(9.5), Shown::Bar);
        // The game ends: folded, also after opening by hand before.
        v.toggle(10.0, 6.0);
        assert_eq!(v.shown(10.0), Shown::Open);
        v.set_game(false);
        assert_eq!(v.shown(11.0), Shown::Bar);
    }

    #[test]
    fn the_pointer_on_it_holds_the_overlay_open() {
        let mut v = Visibility::new(Mode::Always, false);
        v.found(0.0, 6.0);
        v.set_hovered(1.0, true, 6.0);
        assert_eq!(v.shown(30.0), Shown::Open, "held while the pointer is on it");
        assert_eq!(v.peek(30.0), Some(Peek::Held));
        v.set_hovered(31.0, false, 6.0);
        assert_eq!(v.shown(31.0), Shown::Bar);
        // The pointer on a folded bar opens nothing.
        v.set_hovered(32.0, true, 6.0);
        assert_eq!(v.shown(33.0), Shown::Bar);
    }

    /// Pop-up mode: waits as a strip, a find opens it, it goes back, after a
    /// wait the pointer brings it back. (That the rows are drawn again is
    /// checked on the real window by the self-check.)
    #[test]
    fn popup_hides_waits_and_comes_back() {
        let mut v = Visibility::new(Mode::Popup, true);
        assert_eq!(v.shown(0.0), Shown::Strip, "waits as a strip, also while the game runs");
        v.found(1.0, 6.0);
        assert_eq!(v.shown(2.0), Shown::Open);
        assert_eq!(v.peek(2.0), Some(Peek::Closes(5.0)), "says it hides");
        assert_eq!(v.shown(7.5), Shown::Strip, "hidden after the pop-up time");
        // Wait, then the pointer on the strip brings it back.
        v.set_hovered(60.0, true, 6.0);
        assert_eq!(v.shown(60.1), Shown::Open);
        v.set_hovered(61.0, false, 6.0);
        assert_eq!(v.shown(62.0), Shown::Open, "the time still runs after the pointer left");
        assert_eq!(v.shown(66.5), Shown::Strip);
        // The hotkey opens it too.
        v.toggle(70.0, 6.0);
        assert_eq!(v.shown(70.0), Shown::Open);
        v.toggle(71.0, 6.0);
        assert_eq!(v.shown(71.0), Shown::Strip);
    }

    #[test]
    fn clicks_pass_through_except_on_the_lock() {
        let window = Rect::new(100, 100, 400, 600);
        let lock = Rect::new(400, 104, 20, 20);
        let mut s = Settings { click_through: true, ..Settings::default() };
        let at = |x, y, hidden| Some(Pointer { hidden, x, y });
        assert!(passes_clicks(&s, false, at(200, 300, false), window, Some(lock), Shown::Open));
        assert!(!passes_clicks(&s, true, at(410, 110, false), window, Some(lock), Shown::Open), "the hole");
        assert!(passes_clicks(&s, true, at(410, 110, true), window, Some(lock), Shown::Open), "hidden: no hole");
        assert!(!passes_clicks(&s, true, at(410, 110, false), window, Some(lock), Shown::Strip), "strip catches");
        assert!(passes_clicks(&s, false, None, window, Some(lock), Shown::Open), "unknown pointer: the lock decides");
        // Automatic: only while the game hides the pointer.
        s.click_through = false;
        assert!(passes_clicks(&s, false, at(900, 900, true), window, None, Shown::Open));
        assert!(!passes_clicks(&s, true, at(900, 900, false), window, None, Shown::Open));
        assert!(!passes_clicks(&s, false, at(200, 200, true), window, None, Shown::Open), "inside: stays");
        assert!(passes_clicks(&s, true, at(200, 200, true), window, None, Shown::Open), "already through: stays");
        assert!(passes_clicks(&s, true, None, window, None, Shown::Open), "unknown pointer: stays");
        s.auto_through = false;
        assert!(!passes_clicks(&s, true, at(900, 900, true), window, None, Shown::Open));
    }

    fn events_paths(name: &str) -> (Paths, std::path::PathBuf) {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = crate::testdir::fresh(name);
        let home = dir.to_string_lossy().into_owned();
        (Paths::new(Host::new(Flavor::Posix, &[("HOME", &home)], &home)), dir)
    }

    #[test]
    fn only_the_blueprint_opens_by_default() {
        let events = Events::default();
        for event in Event::ALL {
            assert_eq!(events.contains(event), event == Event::Blueprint, "{event:?}");
        }
        assert_eq!(Settings::default().events, events);
    }

    #[test]
    fn every_event_has_a_key_of_its_own() {
        let mut keys: Vec<&str> = Event::ALL.iter().map(|e| e.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Event::ALL.len());
        let mut set = Events::default();
        for event in Event::ALL {
            set = set.with(event, true);
        }
        for event in Event::ALL {
            assert!(set.contains(event));
            let without = set.with(event, false);
            assert!(!without.contains(event));
            assert_eq!(Event::ALL.iter().filter(|e| without.contains(**e)).count(), Event::ALL.len() - 1);
        }
    }

    #[test]
    fn every_part_shows_by_default_and_one_switch_takes_away_only_its_own() {
        let (paths, dir) = events_paths("anzeigen-lesen");
        assert_eq!(Settings::read(&paths).sections, Sections::default());
        assert_eq!(Settings::default().sections, Sections::default());
        for section in Section::ALL {
            assert!(Sections::default().contains(section), "{section:?}");
        }
        let mut keys: Vec<&str> = Section::ALL.iter().map(|s| s.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Section::ALL.len());
        paths.set_setting(keys::SHOW_CONTRACTS, Value::Bool(false));
        let read = Settings::read(&paths).sections;
        assert!(!read.contains(Section::Contracts));
        for section in [Section::Location, Section::Shard, Section::Refinery, Section::Update] {
            assert!(read.contains(section), "{section:?}");
        }
        // The most contracts: ten by default, the player's number within its limits.
        assert_eq!(Settings::read(&paths).contracts_max, 10);
        paths.set_setting(keys::CONTRACTS_MAX, Value::int(12));
        assert_eq!(Settings::read(&paths).contracts_max, 12);
        paths.set_setting(keys::CONTRACTS_MAX, Value::int(0));
        assert_eq!(Settings::read(&paths).contracts_max, CONTRACTS_MAX_RANGE.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// ⚠ Off Windows the game gets every key of a combination, and Strg alone makes the figure lie down: the
    /// defaults have no Strg, and they are the same on every system.
    #[test]
    fn the_default_hotkeys_are_the_same_everywhere_and_without_strg() {
        for (text, key) in [(DEFAULT_HOTKEY, "V"), (DEFAULT_SCAN_HOTKEY, "R")] {
            let hotkey = Hotkey::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert!(!hotkey.ctrl && hotkey.alt && hotkey.shift, "{text}");
            assert_eq!(hotkey.text(), format!("Alt+Umschalt+{key}"));
        }
    }

    #[test]
    fn the_hotkeys_are_read_as_4_12_reads_them_and_the_start_trail_says_what_came_of_them() {
        let (paths, dir) = events_paths("tastenkuerzel");
        let b = Hotkey::parse(DEFAULT_HOTKEY).unwrap();
        // Nothing set: the default, registered.
        let r = Hotkey::parse(DEFAULT_SCAN_HOTKEY).unwrap();
        assert_eq!(HotkeyChoice::read(&paths), HotkeyChoice::Combination(b));
        assert_eq!(HotkeyChoice::read_scan(&paths), HotkeyChoice::Combination(r));
        assert!(scan_hotkey_on(&paths));
        let scan = HotkeyChoice::read_scan(&paths);
        let main = format!("Hotkey: {DEFAULT_HOTKEY} angemeldet (ok)");
        assert_eq!(
            hotkey_trail(&HotkeyChoice::read(&paths), Ok(()), Some((&scan, Ok(())))),
            [main.as_str(), "Scan-Hotkey: Alt+Umschalt+R angemeldet (ok)"]
        );
        // Where the scan cannot read, the trail says nothing of it.
        assert_eq!(hotkey_trail(&HotkeyChoice::read(&paths), Ok(()), None), [main.as_str()]);
        // A hotkey saved before, also the old default 4.12 saved, stays: only the default changed.
        paths.set_setting(keys::HOTKEY, Value::str("Strg+Alt+B"));
        paths.set_setting(keys::HOTKEY_SCAN, Value::str("Strg+Alt+R"));
        assert_eq!(HotkeyChoice::read(&paths).hotkey().map(Hotkey::text).as_deref(), Some("Strg+Alt+B"));
        assert_eq!(HotkeyChoice::read_scan(&paths).hotkey().map(Hotkey::text).as_deref(), Some("Strg+Alt+R"));
        // A combination 4.12 would not take is said, not dropped in silence.
        paths.set_setting(keys::HOTKEY, Value::str("Q"));
        paths.set_setting(keys::HOTKEY_SCAN, Value::str("R"));
        assert_eq!(HotkeyChoice::read(&paths), HotkeyChoice::Wrong("Q".into()));
        assert_eq!(Settings::read(&paths).hotkey, None);
        assert_eq!(Settings::read(&paths).scan_hotkey, None);
        let scan = HotkeyChoice::read_scan(&paths);
        assert_eq!(
            hotkey_trail(&HotkeyChoice::read(&paths), Ok(()), Some((&scan, Ok(())))),
            ["Hotkey: entfaellt (ungueltig: Q)", "Scan-Hotkey: entfaellt (ungueltig: R)"]
        );
        // The system refused the registration.
        paths.set_setting(keys::HOTKEY, Value::str("Alt+F8"));
        paths.set_setting(keys::HOTKEY_SCAN, Value::str("Umschalt+Alt+F9"));
        let alt_f8 = HotkeyChoice::read(&paths);
        assert_eq!(alt_f8.hotkey().map(Hotkey::text).as_deref(), Some("Alt+F8"));
        assert_eq!(Settings::read(&paths).scan_hotkey.map(Hotkey::text).as_deref(), Some("Alt+Umschalt+F9"));
        assert_eq!(hotkey_trail(&alt_f8, Err("belegt".into()), None), ["Hotkey: entfaellt (belegt)"]);
        let scan = HotkeyChoice::read_scan(&paths);
        assert_eq!(hotkey_trail(&alt_f8, Ok(()), Some((&scan, Err("belegt".into()))))[1], "Scan-Hotkey: entfaellt (belegt)");
        // Only a literal `false` switches either off, and the trail says so.
        for (value, on) in [(Value::str("false"), true), (Value::int(0), true), (Value::Bool(false), false)] {
            paths.set_setting(keys::HOTKEY_ON, value.clone());
            paths.set_setting(keys::HOTKEY_SCAN_ON, value.clone());
            assert_eq!(HotkeyChoice::read(&paths) != HotkeyChoice::Off, on, "{value:?}");
            assert_eq!(HotkeyChoice::read_scan(&paths) != HotkeyChoice::Off, on, "{value:?}");
            assert_eq!(scan_hotkey_on(&paths), on, "{value:?}");
        }
        let scan = HotkeyChoice::read_scan(&paths);
        assert_eq!(
            hotkey_trail(&HotkeyChoice::read(&paths), Ok(()), Some((&scan, Ok(())))),
            ["Hotkey: ausgeschaltet", "Scan-Hotkey: ausgeschaltet"]
        );
        assert_eq!(Settings::read(&paths).hotkey, None);
        assert_eq!(Settings::read(&paths).scan_hotkey, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The hotkey of the handle search is off until a literal `true` switches it on; then it is the default or the
    /// one saved, and the start trail says what came of it.
    #[test]
    fn the_hotkey_of_the_handle_search_is_off_until_switched_on() {
        let (paths, dir) = events_paths("suchtaste");
        assert!(!search_hotkey_on(&paths));
        assert_eq!(HotkeyChoice::read_search(&paths), HotkeyChoice::Off);
        assert_eq!(Settings::read(&paths).search_hotkey, None);
        assert_eq!(search_hotkey_trail(&HotkeyChoice::read_search(&paths), Ok(())), "Such-Hotkey: ausgeschaltet");
        for (value, on) in [(Value::str("true"), false), (Value::int(1), false), (Value::Bool(true), true)] {
            paths.set_setting(keys::HOTKEY_SEARCH_ON, value.clone());
            assert_eq!(search_hotkey_on(&paths), on, "{value:?}");
        }
        let h = Hotkey::parse(DEFAULT_SEARCH_HOTKEY).unwrap();
        assert!(!h.ctrl && h.alt && h.shift, "like the other defaults, without Strg");
        assert_eq!(Settings::read(&paths).search_hotkey, Some(h));
        let trail = search_hotkey_trail(&HotkeyChoice::read_search(&paths), Ok(()));
        assert_eq!(trail, "Such-Hotkey: Alt+Umschalt+H angemeldet (ok)");
        paths.set_setting(keys::HOTKEY_SEARCH, Value::str("Strg+F13"));
        assert_eq!(Settings::read(&paths).search_hotkey.map(Hotkey::text).as_deref(), Some("Strg+F13"));
        paths.set_setting(keys::HOTKEY_SEARCH, Value::str("H"));
        assert_eq!(HotkeyChoice::read_search(&paths), HotkeyChoice::Wrong("H".into()));
        assert_eq!(Settings::read(&paths).search_hotkey, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_hotkey_is_written_back_as_it_is_read() {
        for text in ["Strg+Alt+B", "Alt+7", "Strg+Umschalt+F12", "Umschalt+F1"] {
            assert_eq!(Hotkey::parse(text).map(Hotkey::text).as_deref(), Some(text));
        }
    }

    #[test]
    fn the_events_come_from_the_settings() {
        let (paths, dir) = events_paths("ereignisse-lesen");
        // Nothing written: the defaults.
        assert_eq!(Settings::read(&paths).events, Events::default());
        paths.set_setting(keys::EVENT_SCANNER, Value::Bool(true));
        paths.set_setting(keys::EVENT_BLUEPRINT, Value::Bool(false));
        paths.set_setting(keys::EVENT_CATALOG, Value::str("ja"));
        paths.set_setting(keys::EVENT_REFINERY, Value::str("unsinn"));
        let events = Settings::read(&paths).events;
        assert!(events.contains(Event::Scanner));
        assert!(!events.contains(Event::Blueprint), "ein falsch gesetzter Schalter gilt");
        assert!(events.contains(Event::Catalog), "\"ja\" gilt wie bei den anderen Schaltern");
        assert!(!events.contains(Event::Refinery), "Unsinn gilt als der Standardwert");
        assert!(!events.contains(Event::Contract));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The two sizes come from the settings as steps, each on its own: missing is 100, a number between two steps
    /// the nearer one, anything out of range the end it is beyond.
    #[test]
    fn the_sizes_of_overlay_and_info_bar_come_from_the_settings_as_steps() {
        let (paths, dir) = events_paths("groessen-lesen");
        let read = || {
            let s = Settings::read(&paths);
            (s.size_percent, s.info_bar_size_percent)
        };
        assert_eq!(read(), (100, 100), "nothing written: as the main window");
        paths.set_setting(keys::SIZE, Value::int(150));
        assert_eq!(read(), (150, 100), "the overlay alone");
        paths.set_setting(keys::INFO_BAR_SIZE, Value::int(200));
        assert_eq!(read(), (150, 200), "the bar on its own");
        paths.set_setting(keys::SIZE, Value::int(121));
        paths.set_setting(keys::INFO_BAR_SIZE, Value::int(999));
        assert_eq!(read(), (115, 200));
        paths.set_setting(keys::SIZE, Value::int(-5));
        paths.set_setting(keys::INFO_BAR_SIZE, Value::str("gross"));
        assert_eq!(read(), (100, 100));
        assert_eq!(SIZE_STEPS.map(size_step), SIZE_STEPS, "every step is its own");
        assert_eq!((size_step(i64::MIN), size_step(i64::MAX), size_step(140)), (100, 200, 130));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Without its keys the info bar stands over the running game and waits hidden without it; a choice stored
    /// before, either way, stays.
    #[test]
    fn the_info_bar_is_on_with_the_game_and_off_without_it_until_the_player_chooses() {
        let (paths, dir) = events_paths("infoleiste-lesen");
        let read = || {
            let s = Settings::read(&paths);
            (s.info_bar, s.info_bar_always)
        };
        assert_eq!(read(), (true, false), "nothing written: on with the game, hidden without it");
        assert_eq!((Settings::default().info_bar, Settings::default().info_bar_always), (true, false));
        paths.set_setting(keys::INFO_BAR, Value::Bool(false));
        assert_eq!(read(), (false, false), "switched off before: stays off");
        paths.set_setting(keys::INFO_BAR, Value::Bool(true));
        paths.set_setting(keys::INFO_BAR_ALWAYS, Value::Bool(true));
        assert_eq!(read(), (true, true), "shown without the game before: stays shown");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reading_is_new_when_the_number_is_new() {
        let mut hits = ScanHits::default();
        assert!(!hits.update(None));
        assert!(hits.update(Some(4200)), "first number");
        assert!(!hits.update(Some(4200)), "the same number again (the pill only moved)");
        assert!(hits.update(Some(4350)), "another number");
        assert!(!hits.update(None));
        assert!(hits.update(Some(4350)), "gone and back is a new reading");
    }

    #[test]
    fn the_overlay_opens_for_every_event_the_same_way() {
        // Folded bar of the always-on mode and waiting strip of the pop-up mode both open for a while.
        let mut bar = Visibility::new(Mode::Always, false);
        assert_eq!(bar.shown(0.0), Shown::Bar);
        bar.found(1.0, 6.0);
        assert_eq!(bar.shown(2.0), Shown::Open);
        assert_eq!(bar.shown(8.0), Shown::Bar);
        let mut strip = Visibility::new(Mode::Popup, true);
        strip.found(1.0, 6.0);
        assert_eq!(strip.shown(2.0), Shown::Open);
        assert_eq!(strip.shown(8.0), Shown::Strip);
    }

    #[test]
    fn a_state_left_by_hand_comes_back_until_the_game_starts_or_ends() {
        // Folded by hand at the last run, the game running at the start: it stays folded.
        let mut v = Visibility::new(Mode::Always, true);
        v.restore(true);
        assert_eq!(v.shown(0.0), Shown::Bar);
        assert_eq!(v.folded_by_hand(), Some(true));
        // The game ends: it follows the game again.
        v.set_game(false);
        assert_eq!(v.folded_by_hand(), None);
        v.set_game(true);
        assert_eq!(v.shown(1.0), Shown::Open);
        // Opened by hand without the game: it stays open.
        let mut v = Visibility::new(Mode::Always, false);
        v.restore(false);
        assert_eq!(v.shown(0.0), Shown::Open);
        v.toggle(1.0, 6.0);
        assert_eq!(v.folded_by_hand(), Some(true), "the fold arrow says what is to be remembered");
        // The pop-up mode waits as a strip whatever was left.
        let mut strip = Visibility::new(Mode::Popup, true);
        strip.restore(false);
        assert_eq!(strip.shown(0.0), Shown::Strip);
        assert_eq!(strip.folded_by_hand(), None);
    }

    #[test]
    fn the_result_of_reading_all_logs_again_is_kept_and_read_back() {
        let (paths, dir) = events_paths("einlesen-ergebnis");
        assert_eq!(RescanResult::load(&paths), None, "nothing kept");
        let result = RescanResult {
            at: 1_790_000_000.5,
            files: 132,
            blueprints: 3,
            contracts_new: 7,
            contracts_corrected: 2,
            failed: false,
        };
        assert!(result.save(&paths));
        assert_eq!(RescanResult::load(&paths), Some(result));
        let said = result.phrase().render(crate::language::Lang::De);
        assert_eq!(said, "132 Protokolle noch einmal gelesen. Baupläne: 3 dazugekommen. Aufträge: 7 neu, 2 berichtigt.");
        // A reading that failed is kept as such and says so instead of numbers.
        let failed = RescanResult::failure(1_790_000_100.0);
        assert!(failed.save(&paths));
        assert_eq!(RescanResult::load(&paths), Some(failed));
        assert_eq!(failed.phrase().render(crate::language::Lang::De), "Das erneute Einlesen hat nicht geklappt.");
        assert_eq!(failed.phrase().render(crate::language::Lang::En), "Reading the logs again did not work.");
        // Something else under the key is no result.
        paths.set_setting(keys::RESCAN_RESULT, Value::str("kaputt"));
        assert_eq!(RescanResult::load(&paths), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn forgetting_the_place_drops_size_and_place() {
        let (paths, dir) = events_paths("lage-vergessen");
        assert!(save_place(&paths, Rect::new(10, 20, 900, 700)));
        assert!(load_remembered(&paths).is_some());
        assert!(forget_place(&paths));
        assert_eq!(load_remembered(&paths), None, "the default size and the middle at the next start");
        assert!(forget_place(&paths), "nothing to forget is no failure");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_game_runs_by_process_list_or_log() {
        assert!(game_running(Some(true), None));
        assert!(game_running(Some(false), Some(10.0)));
        assert!(!game_running(Some(false), Some(31.0)));
        assert!(game_running(None, Some(200.0)));
        assert!(!game_running(None, Some(301.0)));
        assert!(!game_running(None, None));
    }
}
