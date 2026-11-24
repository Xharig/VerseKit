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

//! The signature scanner in the overlay: the watch thread, the window with the scanned material and the two
//! buttons of the bar (the eye and the teaching).
//!
//! The overlay is the process that is always there, so the watch runs here and not in the main window. What the
//! main window needs (the last picture to teach from) comes through a file in the data folder; what it asks (on,
//! off, the place of the window) comes through the settings and the signal that the settings changed.
//!
//! The eye: grey = off, green = reads, yellow = on but the thread is not running. The switch is not the
//! operation: a green eye that only follows the setting would stay green when the thread has died.

use std::time::{Duration, Instant};

use versekit_core::commodity_prices::Prices;
use versekit_core::language::{Phrase, t};
use versekit_core::mining_data::{self, MiningData};
use versekit_core::mining_gear::{self, Choice, Gear};
use versekit_core::rock_crack::Loadout;
use versekit_core::rock_watch;
use versekit_core::overlay::ScanHits;
use versekit_core::paths::Paths;
use versekit_core::signature_scan::{self, ByVariant};
use versekit_core::signature_watch::{self, Files, Frame, Screen, Source, Times, Watch};
use versekit_core::signature_window::{self, Hint, Layout, Text};
use versekit_core::theme::Scheme;
use versekit_ui::overlay::{Extra, Tone};

#[cfg(windows)]
use crate::scan_window::{Content, FrameWindow, ScanWindow, colorref};

/// The switch that makes the program the main window, and the one that makes it the small window of teaching.
pub const MAIN_WINDOW_SWITCH: &str = "--hauptfenster";
pub const LEARN_SWITCH: &str = "--anlernen";

/// The frame that shows the area read stays so many seconds.
pub const FRAME_SHOW_S: f64 = 6.0;

/// The ids of the two buttons in the bar.
pub const EYE: &str = "signatur";
pub const LEARN: &str = "anlernen";

/// The mining data are read again this often: a new build changes the signatures.
const MINING_RELOAD: Duration = Duration::from_secs(60);
/// The picture for teaching is written at most this often.
const FRAME_EVERY: Duration = Duration::from_secs(1);

/// The equipment, the choice and the prices are read again this often: the page Mining changes the choice.
const GEAR_RELOAD: Duration = Duration::from_secs(5);

/// A place on the screen: left, top, width, height.
type Area4 = (i32, i32, i32, i32);

/// After the last signature the player still mines this long: the rock is scanned from about 3 km on and its
/// signature leaves the HUD, the panel of the rock is searched for in this time.
pub const MINING_AFTER: Duration = Duration::from_secs(180);

/// Whether the player mines: a signature is shown, or the last one left the HUD less than [`MINING_AFTER`] ago.
pub fn mining_now(signature_shown: bool, last_signature: Option<Instant>, now: Instant) -> bool {
    signature_shown || last_signature.is_some_and(|at| now.saturating_duration_since(at) < MINING_AFTER)
}

/// What the lines of the rock are computed with.
struct Mining {
    loadout: Loadout,
    prices: Prices,
    show: (bool, bool),
}

impl Mining {
    fn read(paths: &Paths) -> Mining {
        let gear = Gear::load(paths);
        Mining {
            loadout: Choice::read(paths).loadout(&gear),
            prices: Prices::load(paths),
            show: (
                paths.setting_bool(mining_gear::keys::SHOW_VERDICT, true),
                paths.setting_bool(mining_gear::keys::SHOW_WORTH, true),
            ),
        }
    }
}

/// What a step of the frame found out for the overlay.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// The window was dragged: its new offset (per mille of the game window), to be stored.
    pub offset: Option<(i32, i32)>,
    /// The pill was found in a place not kept yet: the size of the game window and the box, to be stored.
    pub place: Option<(String, [i32; 4])>,
    /// A row of digits was found that was not read sure, and its picture was kept for teaching.
    pub unsure: bool,
    /// A new number was read (not shown before, or another than before): an event that may open the overlay.
    pub hit: bool,
}

pub struct Scanner {
    /// A run that may grab the screen: real data, no self-check, a system that can.
    active: bool,
    folder: Option<String>,
    files: Option<Files>,
    watch: Option<Watch>,
    on: bool,
    layout: Layout,
    /// Where to read, as the settings last said, and the place handed on for storing last.
    hint: Hint,
    stored_place: Option<(String, [i32; 4])>,
    /// The number of the last unsure picture that was dealt with.
    unsure_seen: u64,
    /// Which number was shown at the last step: a new one is a hit.
    hits: ScanHits,
    lock_open: bool,
    mining: MiningData,
    mining_at: Option<Instant>,
    mining_file: String,
    seen: u64,
    shown: Option<Text>,
    /// The number left the HUD: it stays shown until then (the time left of the layout).
    clear_at: Option<Instant>,
    /// The names of the data behind the shown signature, for the demand of the unit.
    names: Vec<String>,
    /// The info bar shows the reading: the window stays closed.
    window_off: bool,
    written: u64,
    written_at: Option<Instant>,
    /// The counts of the grey images as the file for the report has them, and when they were written.
    counts_written: ByVariant<u64>,
    counts_at: Option<Instant>,
    /// The files of the taught templates (the shared one and one for each grey image) and when each was last
    /// seen to change: teaching in the main window reaches the watch through them.
    own_stamp: [Option<std::time::SystemTime>; 3],
    own_checked: Option<Instant>,
    /// The watch of the panel of the scanned rock: runs while the signature watch runs.
    rock: Option<rock_watch::Watch>,
    rock_seen: u64,
    /// The places of the rock's panel kept in the settings, handed to the watch at its start, and the last one
    /// stored.
    rock_places: Vec<(String, [i64; 4])>,
    rock_place_stored: Option<(String, [i64; 4])>,
    /// What the rock gives, as shown below the number and in the overlay line.
    rock_lines: Vec<rock_watch::Line>,
    /// The equipment of the game data, the player's choice, the prices and the switches of the lines, read again
    /// every [`GEAR_RELOAD`].
    gear: Option<Mining>,
    gear_at: Option<Instant>,
    /// When a signature stood in the HUD last ([`mining_now`]).
    last_signature: Option<Instant>,
    /// The panel of the rock is in sight: the window stays while it is, with the last signature and the rock's lines.
    rock_in_sight: bool,
    /// Where the window stood last (the pill and the game window) and what it showed, to show it again for the
    /// rock's lines once the signature has left the HUD.
    last_place: Option<(Area4, Area4)>,
    last_text: Option<Text>,
    #[cfg(windows)]
    window: Option<ScanWindow>,
    /// The frame that shows the area read, and when it goes.
    #[cfg(windows)]
    frame_window: Option<FrameWindow>,
    #[cfg(windows)]
    frame_until: Option<Instant>,
    #[cfg(windows)]
    place_again: bool,
    #[cfg(windows)]
    size: (i32, i32),
}

impl Scanner {
    /// `paths` is `None` for the samples; `real` is false in the self-check.
    pub fn new(paths: Option<&Paths>, real: bool) -> Scanner {
        let active = real && paths.is_some() && versekit_core::screen_grab::supported();
        let files = paths.map(|p| Files {
            shipped: signature_scan::SHIPPED_TEMPLATES.to_string(),
            own: signature_scan::OwnFiles::in_folder(|name| p.app_file(name)),
            mining: p.app_file(mining_data::FILE),
        });
        let folder = paths.and_then(|p| {
            std::path::Path::new(&p.app_file(signature_scan::OWN_TEMPLATE_FILE))
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
        });
        Scanner {
            active,
            mining_file: files.as_ref().map(|f| f.mining.clone()).unwrap_or_default(),
            folder,
            files,
            watch: None,
            on: false,
            layout: Layout::default(),
            hint: Hint::default(),
            stored_place: None,
            unsure_seen: 0,
            hits: ScanHits::default(),
            lock_open: false,
            mining: MiningData::default(),
            mining_at: None,
            seen: 0,
            shown: None,
            clear_at: None,
            names: Vec::new(),
            window_off: false,
            written: 0,
            written_at: None,
            counts_written: ByVariant::default(),
            counts_at: None,
            own_stamp: [None; 3],
            own_checked: None,
            rock: None,
            rock_seen: 0,
            rock_places: paths.map_or_else(Vec::new, |p| {
                let settings = p.settings();
                rock_watch::places_of(settings.iter().find(|(k, _)| k == rock_watch::PLACES).map(|(_, v)| v))
            }),
            rock_place_stored: None,
            rock_lines: Vec::new(),
            gear: None,
            gear_at: None,
            last_signature: None,
            rock_in_sight: false,
            last_place: None,
            last_text: None,
            #[cfg(windows)]
            window: None,
            #[cfg(windows)]
            frame_window: None,
            #[cfg(windows)]
            frame_until: None,
            #[cfg(windows)]
            place_again: false,
            #[cfg(windows)]
            size: (0, 0),
        }
    }

    /// Can this run read the screen at all (the buttons are there only then)?
    #[cfg(all(test, windows))]
    pub fn active(&self) -> bool {
        self.active
    }

    #[cfg(all(test, windows))]
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Does the thread run?
    pub fn alive(&self) -> bool {
        self.watch.as_ref().is_some_and(Watch::alive)
    }

    /// Takes the settings: starts or ends the watch, moves the window where the layout says.
    /// With the watch of the signature the one of the rock's panel starts: only here, on the real screen, and only
    /// on Windows (its recognition is the one of the system).
    pub fn apply(
        &mut self,
        on: bool,
        layout: Layout,
        hint: &Hint,
        lock_open: bool,
        wake: impl Fn() + Send + Sync + 'static,
    ) {
        let wake = std::sync::Arc::new(wake);
        let signature_wake = wake.clone();
        self.apply_from(on, layout, hint, lock_open, move || signature_wake(), || Box::new(Screen));
        if self.active && self.watch.is_some() && self.rock.is_none() && versekit_core::win_ocr::supported() {
            self.rock = rock_watch::Watch::start(
                Box::new(Screen),
                rock_watch::system_reader(),
                self.rock_places.clone(),
                move || wake(),
            );
        }
    }

    /// As [`Scanner::apply`], with the pictures from `source` (the checks make up a screen) and without the watch of
    /// the rock's panel.
    pub fn apply_from(
        &mut self,
        on: bool,
        layout: Layout,
        hint: &Hint,
        lock_open: bool,
        wake: impl Fn() + Send + 'static,
        source: impl FnOnce() -> Box<dyn Source>,
    ) {
        if !self.active {
            return;
        }
        if on && self.watch.is_none() {
            if let Some(files) = self.files.clone() {
                self.watch = Watch::start(source(), files, Times::default(), hint.clone(), wake);
                self.hint = hint.clone();
                self.seen = 0;
                // The counts of the report are the ones since this start.
                self.counts_written = ByVariant::default();
                self.counts_at = None;
                if let Some(folder) = &self.folder {
                    write_counts(folder, &self.counts_written);
                }
            }
        } else if !on && let Some(watch) = self.watch.take() {
            watch.stop();
            self.stop_rock();
            self.hide();
        }
        // Only what the settings say anew is handed on: what the watch learned itself stands until the
        // settings hold it too.
        if *hint != self.hint {
            self.hint = hint.clone();
            if let Some(watch) = &self.watch {
                watch.set_hint(hint.clone());
            }
        }
        self.on = on;
        if layout != self.layout || lock_open != self.lock_open {
            self.layout = layout;
            self.lock_open = lock_open;
            self.seen = 0;
            #[cfg(windows)]
            {
                self.place_again = true;
                if let Some(window) = &self.window {
                    window.set_draggable(lock_open);
                }
            }
        }
    }

    /// Shows the area the scanner reads over the game for a few seconds: the frame the player drew, the place
    /// found before, else the middle. Gives whether it could be shown (the game window was found).
    pub fn show_frame(&mut self, scheme: &Scheme) -> bool {
        #[cfg(windows)]
        {
            if !self.active {
                return false;
            }
            let Some(game) = versekit_core::screen_grab::find_game_rect() else { return false };
            let area =
                self.hint.narrow(game).map(|(area, _)| area).or_else(|| signature_watch::search_area(Some(game)));
            let Some(area) = area else { return false };
            if self.frame_window.is_none() {
                self.frame_window = FrameWindow::create();
            }
            let Some(window) = &self.frame_window else { return false };
            window.show(area, colorref(scheme.accent));
            self.frame_until = Some(Instant::now() + Duration::from_secs_f64(FRAME_SHOW_S));
            true
        }
        #[cfg(not(windows))]
        {
            let _ = scheme;
            false
        }
    }

    /// The frame goes after its few seconds.
    fn expire_frame(&mut self) {
        #[cfg(windows)]
        if self.frame_until.is_some_and(|until| Instant::now() >= until) {
            self.frame_until = None;
            if let Some(window) = &self.frame_window {
                window.hide();
            }
        }
    }

    /// After teaching (the file of the taught templates changed): the watch reads them at its next beat.
    fn follow_teaching(&mut self) {
        if self.own_checked.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
            return;
        }
        let first = self.own_checked.is_none();
        self.own_checked = Some(Instant::now());
        let stamp = self.files.as_ref().map_or([None; 3], |f| {
            f.own.all().map(|(_, path)| std::fs::metadata(path).ok().and_then(|m| m.modified().ok()))
        });
        if stamp != self.own_stamp {
            self.own_stamp = stamp;
            if let (false, Some(watch)) = (first, &self.watch) {
                watch.reload_templates();
            }
        }
    }

    fn stop_rock(&mut self) {
        if let Some(rock) = self.rock.take() {
            rock.stop();
        }
        self.rock_lines.clear();
        self.rock_seen = 0;
    }

    /// Follows the watch of the rock's panel: tells it whether the player mines (a signature is shown), reads the
    /// equipment, the choice and the prices again now and then, and makes the lines of the rock. A change of the
    /// lines places the window anew at the next step. Gives the setting to store when the panel was found in a new
    /// place.
    pub fn follow_rock(&mut self, paths: Option<&Paths>) -> Option<(&'static str, versekit_core::jsonio::Value)> {
        let Some(rock) = &self.rock else { return None };
        let now = Instant::now();
        if self.shown.is_some() && self.clear_at.is_none() {
            self.last_signature = Some(now);
        }
        rock.set_mining(mining_now(self.shown.is_some(), self.last_signature, now));
        let state = rock.state();
        self.rock_in_sight = state.seen;
        // A place the search found is kept for the next start: handed back to be stored.
        let mut store = None;
        if let Some(place) = &state.place
            && self.rock_place_stored.as_ref() != Some(place)
        {
            let value = rock_watch::places_value(self.rock_places.clone(), &place.0, place.1);
            self.rock_places = rock_watch::places_of(Some(&value));
            self.rock_place_stored = Some(place.clone());
            store = Some((rock_watch::PLACES, value));
        }
        let due = self.gear_at.is_none_or(|at| at.elapsed() >= GEAR_RELOAD);
        if due && let Some(paths) = paths {
            self.gear = Some(Mining::read(paths));
            self.gear_at = Some(Instant::now());
            self.rock_seen = 0;
        }
        if state.version == self.rock_seen {
            return store;
        }
        self.rock_seen = state.version;
        let lines = self.gear.as_ref().map_or_else(Vec::new, |mining| {
            rock_watch::lines(&state, &mining.loadout, |name| mining.prices.price(name).map(|p| p.sell), mining.show)
        });
        if lines != self.rock_lines {
            self.rock_lines = lines;
            #[cfg(windows)]
            {
                self.place_again = true;
            }
        }
        store
    }

    fn hide(&mut self) {
        self.clear_at = None;
        self.shown = None;
        self.names.clear();
        self.hide_window();
    }

    fn hide_window(&self) {
        #[cfg(windows)]
        if let Some(window) = &self.window {
            window.hide();
        }
    }

    /// `off`: the info bar shows the reading, its own window stays closed; the reading goes on.
    pub fn set_window_off(&mut self, off: bool) {
        if off == self.window_off {
            return;
        }
        self.window_off = off;
        if off {
            self.hide_window();
        }
        #[cfg(windows)]
        {
            self.place_again = true;
        }
    }

    /// The buttons of the bar: the eye (off, reads, on but not reading) and the teaching.
    pub fn extras(&self) -> Vec<Extra> {
        if !self.active {
            return Vec::new();
        }
        let (icon, tone, hint) = if !self.on {
            ("eye-off", Tone::Normal, "hinweis_scanner_aus")
        } else if self.alive() {
            ("eye", Tone::Lit, "hinweis_scanner_an")
        } else {
            ("eye", Tone::Flash, "hinweis_scanner_tot")
        };
        vec![
            Extra { action: versekit_ui::overlay::Action::Extra(EYE), icon, meaning: t(hint), id: EYE, tone },
            Extra {
                action: versekit_ui::overlay::Action::Extra(LEARN),
                icon: "graduation-cap",
                meaning: t("hinweis_anlernen"),
                id: LEARN,
                tone: Tone::Normal,
            },
        ]
    }

    /// The line the status shows after the switch: "Automatisch erkennen: an/aus".
    pub fn said(on: bool) -> Phrase {
        Phrase::new("s_bg_scan_sagen").with("0", Phrase::new(if on { "e_an" } else { "e_aus" }))
    }

    /// The number with its material in one line, for the open overlay when the player asked for it, or when
    /// the reading itself opened the overlay (`forced`).
    /// Under Linux there is no window above the signature yet: the line stands in the overlay whenever the info
    /// bar does not show the reading.
    pub fn line(&self, forced: bool) -> Option<String> {
        let no_window = cfg!(not(windows)) && !self.window_off;
        if !self.layout.overlay_line && !forced && !no_window {
            return None;
        }
        self.shown.as_ref().map(|text| {
            let mut line = t("ov_signatur").replace("{0}", &text.line());
            for rock in &self.rock_lines {
                line.push('\n');
                line.push_str(&rock.text);
            }
            line
        })
    }

    /// The names of the data behind the signature shown now; empty while none is shown.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// A signature as the watch would have shown it, with the names behind it (for the checks).
    #[cfg(test)]
    pub fn show_read(&mut self, text: Text, names: Vec<String>, overlay_line: bool) {
        self.layout.overlay_line = overlay_line;
        self.shown = Some(text);
        self.names = names;
    }

    /// What the window above the signature shows now; `None` while it is hidden.
    pub fn shown(&self) -> Option<&Text> {
        self.shown.as_ref()
    }

    /// How long until the number that left the HUD goes, while it stays shown: the overlay wakes then.
    pub fn wake_in(&self) -> Option<Duration> {
        self.clear_at.map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// The time the number stays shown after the watch cleared it: the time left of the layout minus the grace
    /// the watch already waited.
    fn linger(&self) -> Duration {
        Duration::from_secs_f64((self.layout.linger as f64 - signature_watch::CLEAR_S).max(0.0))
    }

    fn reload_mining(&mut self) {
        if self.mining_at.is_none_or(|at| at.elapsed() > MINING_RELOAD) {
            self.mining = MiningData::load(&self.mining_file);
            self.mining_at = Some(Instant::now());
        }
    }

    /// One frame: follows what the watch found, shows or hides the window, hands the picture on for teaching.
    pub fn step(&mut self, scheme: &Scheme) -> Step {
        let mut out = Step::default();
        self.expire_frame();
        self.follow_teaching();
        let Some(watch) = &self.watch else { return out };
        let state = watch.state();
        // Before anything that returns early (the drag): a reading must not go unnoticed.
        out.hit = self.hits.update(state.shown);
        if let Some(place) = &state.place
            && self.stored_place.as_ref() != Some(place)
        {
            self.stored_place = Some(place.clone());
            out.place = Some(place.clone());
        }
        // A number that could not be read sure: the picture is kept for teaching, nothing opens by itself.
        if let (Some(frame), Some(folder)) = (&state.unsure, &self.folder)
            && frame.number != self.unsure_seen
        {
            self.unsure_seen = frame.number;
            let kept = signature_watch::keep_pending(
                &format!("{}/{}", folder.trim_end_matches(['/', '\\']), signature_watch::PENDING_FOLDER),
                frame,
            );
            out.unsure = kept;
        }
        if let Some(folder) = &self.folder
            && state.variant_counts != self.counts_written
            && self.counts_at.is_none_or(|at| at.elapsed() >= FRAME_EVERY)
        {
            write_counts(folder, &state.variant_counts);
            self.counts_written = state.variant_counts.clone();
            self.counts_at = Some(Instant::now());
        }
        if let (Some(frame), Some(folder)) = (&state.frame, &self.folder) {
            let due = self.written_at.is_none_or(|at| at.elapsed() >= FRAME_EVERY);
            if frame.number != self.written && due {
                write_frame(folder, frame);
                self.written = frame.number;
                self.written_at = Some(Instant::now());
            }
        }
        #[cfg(windows)]
        {
            // The drag first: its end gives the offset, and while it runs the window stays where the hand holds it.
            if let Some(window) = &self.window {
                if let (Some(top_left), Some(pill), Some(game)) = (window.take_moved(), state.pill, state.game) {
                    out.offset = Some(signature_window::offset_for(top_left, pill, game, self.size));
                }
                if window.dragging() {
                    return out;
                }
            }
        }
        // The number left the HUD a while ago: its time to stay is over, unless the panel of the rock is in sight;
        // then the window stays with the rock's lines and is looked at again in a second.
        if self.clear_at.is_some_and(|at| Instant::now() >= at) {
            if self.rock_in_sight && !self.rock_lines.is_empty() {
                self.clear_at = Some(Instant::now() + Duration::from_secs(1));
            } else {
                self.hide();
            }
        }
        // The window went with the signature, then the panel of the rock came in sight: it opens again with the
        // last signature and the rock's lines.
        if self.shown.is_none()
            && self.rock_in_sight
            && !self.rock_lines.is_empty()
            && let (Some(text), Some((pill, game))) = (self.last_text.clone(), self.last_place)
        {
            self.clear_at = Some(Instant::now() + Duration::from_secs(1));
            if !self.window_off {
                self.show(&text, pill, game, scheme);
            }
            self.shown = Some(text);
            return out;
        }
        if state.version == self.seen && !self.place_pending() {
            return out;
        }
        self.seen = state.version;
        self.reload_mining();
        match (state.shown, state.pill, state.game) {
            (Some(value), Some(pill), Some(game)) => {
                self.clear_at = None;
                let text = signature_window::describe(value, &self.mining);
                self.names = signature_window::materials(value, &self.mining);
                if self.window_off {
                    self.hide_window();
                } else {
                    self.show(&text, pill, game, scheme);
                }
                self.last_text = Some(text.clone());
                self.shown = Some(text);
                self.last_place = Some((pill, game));
            }
            // Gone from the HUD: what was shown stays for the time the player chose, then goes. New lines of the
            // rock show in the window that stays.
            _ if self.shown.is_some() => {
                if self.clear_at.is_none() {
                    self.clear_at = Some(Instant::now() + self.linger());
                }
                if let (Some(text), Some((pill, game))) = (self.shown.clone(), self.last_place)
                    && !self.window_off
                {
                    self.show(&text, pill, game, scheme);
                }
            }
            _ => self.hide(),
        }
        out
    }

    #[cfg(windows)]
    fn place_pending(&mut self) -> bool {
        std::mem::take(&mut self.place_again)
    }

    #[cfg(not(windows))]
    fn place_pending(&mut self) -> bool {
        false
    }

    #[cfg(windows)]
    fn show(&mut self, text: &Text, pill: (i32, i32, i32, i32), game: (i32, i32, i32, i32), scheme: &Scheme) {
        if self.window.is_none() {
            self.window = ScanWindow::create();
            if let Some(window) = &self.window {
                window.set_draggable(self.lock_open);
            }
        }
        let Some(window) = &self.window else { return };
        let content = Content {
            material: text.material.clone(),
            known: text.known,
            number: text.number.clone(),
            extra: self
                .rock_lines
                .iter()
                .map(|line| (line.text.clone(), colorref(if line.good { scheme.accent } else { scheme.fg })))
                .collect(),
            font_px: self.layout.size.font_px(game.3),
            background: colorref(scheme.surface),
            text: colorref(scheme.fg),
            accent: colorref(scheme.accent),
            sub: colorref(scheme.sub),
            line: colorref(scheme.line),
            radius: i32::from(versekit_ui::box_radius(scheme).nw),
            square: scheme.square,
        };
        let size = window.set_content(content);
        self.size = size;
        let (x, y) = signature_window::place(pill, game, size, &self.layout);
        window.show_at((x, y, size.0, size.1));
    }

    #[cfg(not(windows))]
    fn show(&mut self, _: &Text, _: (i32, i32, i32, i32), _: (i32, i32, i32, i32), _: &Scheme) {}

    /// Ends the watch when the overlay closes.
    pub fn close(&mut self) {
        if let Some(watch) = self.watch.take() {
            watch.stop();
        }
        self.stop_rock();
        self.hide();
    }
}

fn write_counts(folder: &str, counts: &ByVariant<u64>) {
    let path = format!("{}/{}", folder.trim_end_matches(['/', '\\']), signature_watch::COUNTS_FILE);
    signature_watch::write_counts(&path, counts);
}

fn write_frame(folder: &str, frame: &Frame) {
    let path = format!("{}/{}", folder.trim_end_matches(['/', '\\']), signature_watch::LAST_FRAME_FILE);
    signature_watch::write_last_frame(&path, frame);
}

#[cfg(all(test, windows))]
mod tests;
