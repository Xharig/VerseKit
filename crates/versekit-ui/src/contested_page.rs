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

//! The page "Contested Zones" (new in v5): the cycle of the Executive Hangar with its lamps, the next
//! openings in local time, and the self timers of the keycards and compboards. The start of the cycle
//! comes from the page contestedzonetimers.com (fetched in the background, at most every six hours) or
//! from a value typed by hand. The self timers run on the clock, so they go on while another page is open.

use versekit_core::language::SpawnWithLanguage as _;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Align, Layout, RichText, Sense, Ui, vec2};
use versekit_core::contested::{self, GROUPS, Hangar, Kept, LAMPS, Lamp, Outcome, Phase, SelfTimer, TimerState};
use versekit_core::jsonio::Value;
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::theme::Scheme;

use crate::layout::{self, GAP};
use crate::widgets::{self, ButtonKind};
use crate::{color, control_radius, fonts};

/// Openings listed below the hangar.
const OPENINGS_SHOWN: usize = 8;
/// Width of a self timer box with its frame.
const BOX_WIDTH: f32 = 182.0;
/// Width inside the frame of a box.
const BOX_INNER: f32 = 164.0;
/// A running timer with this many seconds left or fewer is drawn as a warning.
const WARN_SECONDS: i64 = 180;
/// Diameter of the circle around the countdown, and the width of its ring.
const RING_SIZE: f32 = 168.0;
const RING_WIDTH: f32 = 10.0;
/// Side of a numbered box and the gap between two.
const LED_SIZE: f32 = 36.0;
const LED_GAP: f32 = 6.0;

/// A self timer with the name it is shown and announced by.
struct Entry {
    label: String,
    timer: SelfTimer,
}

/// What the page keeps: the start of the cycle, the fetch of it, the typed start and the self timers.
pub struct ContestedState {
    /// Where the start is kept; without it (the preview) it lives in the window only.
    paths: Option<Paths>,
    /// The way to the page that publishes the start; none in the preview and in tests.
    net: Option<Arc<dyn Net>>,
    kept: Kept,
    job: Option<Receiver<Outcome>>,
    /// When a fetch was tried last (Unix seconds), so that a failing one is not tried at every visit.
    tried_at: f64,
    /// The last fetch failed.
    failed: bool,
    /// Fetching is switched off.
    offline: bool,
    hand_text: String,
    hand_wrong: bool,
    /// The open overlay shows the line of the hangar (a setting).
    overlay: bool,
    /// The map that stands open (an index of [`MAPS`]), and for each map the part of it in view.
    map: usize,
    scenes: Vec<egui::Rect>,
    /// The map was clicked into and the pointer has not left it since: the plain wheel zooms it.
    map_taken: bool,
    /// Where the frame of the map stood in the last frame, for the checks that move the mouse over it.
    #[cfg(test)]
    map_frame: egui::Rect,
    /// The texture of the map that stands open (the index of the map with it), the one being prepared, and the
    /// map that could not be made into a texture.
    map_tex: Option<(usize, egui::TextureHandle)>,
    map_job: Option<(usize, Receiver<Result<egui::ColorImage, String>>)>,
    map_failed: Option<usize>,
    /// The maps are prepared (a test of another page does not decode megabytes at every frame it draws).
    maps_on: bool,
    /// In the order of [`GROUPS`].
    timers: Vec<Entry>,
    /// The names of the timers that ran out and are not announced yet.
    ran_out: Vec<String>,
}

impl Default for ContestedState {
    fn default() -> Self {
        ContestedState::kept_in(None, None, Kept::default())
    }
}

impl ContestedState {
    /// The start and the typed start of the data folder; the page fetches over the real net.
    pub fn with_files(paths: &Paths) -> ContestedState {
        #[cfg(not(test))]
        let net: Option<Arc<dyn Net>> = {
            let agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
            Some(Arc::new(contested::web(&agent)))
        };
        // The tests of this crate never ask the network.
        #[cfg(test)]
        let net: Option<Arc<dyn Net>> = None;
        ContestedState::with_net(paths, net)
    }

    /// Like [`ContestedState::with_files`], fetching over `net`.
    pub fn with_net(paths: &Paths, net: Option<Arc<dyn Net>>) -> ContestedState {
        let own = Paths::new(paths.host.clone());
        let kept = contested::load(&own);
        let overlay = own.setting_bool(versekit_core::overlay::keys::HANGAR_TIMER, false);
        let map = own.setting(SETTING_MAP).and_then(|id| MAPS.iter().position(|map| map.id == id)).unwrap_or(0);
        ContestedState { overlay, map, ..ContestedState::kept_in(Some(own), net, kept) }
    }

    /// Takes in the map that was prepared, and starts preparing the one that stands open when it is not there:
    /// decoded in a thread of its own (a big map takes a moment) and held as one texture, the one before it
    /// let go.
    fn load_map(&mut self, ctx: &egui::Context) {
        if let Some((at, job)) = &self.map_job {
            let at = *at;
            match job.try_recv() {
                Ok(Ok(image)) => {
                    let texture =
                        ctx.load_texture(format!("contested-map-{}", MAPS[at].id), image, egui::TextureOptions::LINEAR);
                    self.map_tex = Some((at, texture));
                    self.map_job = None;
                }
                Ok(Err(error)) => {
                    versekit_core::errors::record("contested.map", &error);
                    self.map_failed = Some(at);
                    self.map_job = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.map_failed = Some(at);
                    self.map_job = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        let wanted = self.map.min(MAPS.len() - 1);
        let ready = self.map_tex.as_ref().is_some_and(|(at, _)| *at == wanted);
        let busy = self.map_job.as_ref().is_some_and(|(at, _)| *at == wanted);
        if !self.maps_on || ready || busy || self.map_failed == Some(wanted) {
            return;
        }
        self.map_tex = None;
        let side = u32::try_from(ctx.input(|input| input.max_texture_side)).unwrap_or(MAP_SIDE).min(MAP_SIDE);
        let (sender, receiver) = std::sync::mpsc::channel();
        let bytes = MAPS[wanted].bytes;
        let started = std::thread::Builder::new().name("contested-map".into()).spawn_with_language(move || {
            let _ = sender.send(decode_map(bytes, side));
        });
        if started.is_ok() {
            self.map_job = Some((wanted, receiver));
        } else {
            self.map_failed = Some(wanted);
        }
    }

    /// The settings this page keeps in the settings file.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        if self.paths.is_none() {
            return Vec::new();
        }
        vec![
            (versekit_core::overlay::keys::HANGAR_TIMER, Value::Bool(self.overlay)),
            (SETTING_MAP, Value::str(MAPS[self.map.min(MAPS.len() - 1)].id)),
        ]
    }

    /// A cycle that began `since` seconds before `now`, for the preview.
    pub fn sample(now: f64, since: i64) -> ContestedState {
        let start = now.floor() as i64 - since;
        ContestedState::kept_in(None, None, Kept { site: Some(start), fetched: now, hand: None })
    }

    fn kept_in(paths: Option<Paths>, net: Option<Arc<dyn Net>>, kept: Kept) -> ContestedState {
        let offline = paths.as_ref().is_some_and(|paths| versekit_core::net::offline_in(&paths.host));
        let mut timers = Vec::new();
        for group in &GROUPS {
            for section in group.sections {
                for spec in section.timers {
                    let label = format!("{} · {} · {}", group.name, section.name, spec.name);
                    timers.push(Entry { label, timer: SelfTimer::new(spec.seconds) });
                }
            }
        }
        ContestedState {
            paths,
            net,
            kept,
            job: None,
            tried_at: 0.0,
            failed: false,
            offline,
            hand_text: String::new(),
            hand_wrong: false,
            overlay: false,
            map: 0,
            scenes: vec![egui::Rect::ZERO; MAPS.len()],
            map_taken: false,
            #[cfg(test)]
            map_frame: egui::Rect::ZERO,
            map_tex: None,
            map_job: None,
            map_failed: None,
            maps_on: !cfg!(test),
            timers,
            ran_out: Vec::new(),
        }
    }

    /// The start the times are counted from.
    pub fn start(&self) -> Option<i64> {
        self.kept.start()
    }

    /// The page is entered: the start is fetched when it is due.
    pub fn enter(&mut self) {
        let now = contested::now();
        if self.job.is_some() || self.offline || !self.kept.due(now) || now - self.tried_at < contested::RETRY_AFTER {
            return;
        }
        let (Some(paths), Some(net)) = (&self.paths, &self.net) else { return };
        self.tried_at = now;
        let (sender, receiver) = std::sync::mpsc::channel();
        let own = Paths::new(paths.host.clone());
        let net = Arc::clone(net);
        let lock = versekit_core::writeguard::state();
        let started = std::thread::Builder::new().name("contested-start".into()).spawn(move || {
            versekit_core::writeguard::adopt(&lock);
            let _ = sender.send(contested::refresh(&own, net.as_ref(), contested::now()));
        });
        if started.is_ok() {
            self.job = Some(receiver);
        }
    }

    /// Every frame, whatever page is open: takes in the fetched start, notes the timers that ran out and
    /// asks for a frame when the next one does.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok(outcome) => {
                    self.job = None;
                    self.take(outcome);
                }
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    self.failed = true;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
            }
        }
        let now = contested::now();
        for entry in &mut self.timers {
            if entry.timer.tick(now) {
                self.ran_out.push(entry.label.clone());
            }
        }
        let next = self.timers.iter().filter_map(|entry| entry.timer.wake_in(now)).fold(f64::INFINITY, f64::min);
        if next.is_finite() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(next.max(0.05)));
        }
    }

    fn take(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Fresh(kept) => {
                self.kept = kept;
                self.failed = false;
            }
            Outcome::Offline => self.offline = true,
            Outcome::Failed(reason) => {
                versekit_core::errors::record("contested.fetch", &reason);
                self.failed = true;
            }
        }
    }

    /// The names of the timers that ran out since the last call.
    pub fn take_ran_out(&mut self) -> Vec<String> {
        std::mem::take(&mut self.ran_out)
    }

    /// Takes the typed start, or tells that it is none.
    fn set_hand(&mut self) {
        let now = versekit_core::export::now_unix();
        let Some(start) = contested::parse_input(&self.hand_text, now) else {
            self.hand_wrong = true;
            return;
        };
        self.hand_wrong = false;
        self.apply_hand(Some(start));
        self.hand_text.clear();
    }

    /// The player stands at the hangar at `now` (Unix seconds) and sees it open or start to reset: the start
    /// follows from that and takes precedence over the one of the page.
    fn seen_at_hangar(&mut self, phase: Phase, now: i64) {
        let start = match phase {
            Phase::Open => contested::start_just_opened(now),
            Phase::Resetting => contested::start_just_reset(now),
            Phase::Closed => return,
        };
        self.hand_wrong = false;
        self.apply_hand(Some(start));
    }

    fn apply_hand(&mut self, hand: Option<i64>) {
        match &self.paths {
            Some(paths) => match contested::set_hand(paths, hand) {
                Ok(kept) => self.kept = kept,
                Err(error) => {
                    versekit_core::errors::record("contested.hand", &error);
                    self.kept.hand = hand;
                }
            },
            None => self.kept.hand = hand,
        }
    }

    #[cfg(test)]
    fn timer_mut(&mut self, index: usize) -> &mut SelfTimer {
        &mut self.timers[index].timer
    }
}

fn sub(scheme: &Scheme, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(color(scheme.sub))
}

/// The color a phase is drawn in.
fn phase_color(scheme: &Scheme, phase: Phase) -> egui::Color32 {
    color(match phase {
        Phase::Closed => scheme.red_text(),
        Phase::Open => scheme.accent,
        Phase::Resetting => scheme.gold,
    })
}

/// A large figure in the bold face of the headings.
fn figure(ui: &Ui, text: impl Into<String>, tint: egui::Color32) -> RichText {
    let size = egui::TextStyle::Heading.resolve(ui.style()).size;
    RichText::new(text.into()).font(fonts::bold(size)).color(tint)
}

/// Draws the page.
pub fn show_page(ui: &mut Ui, scheme: &Scheme, state: &mut ContestedState) {
    let now = contested::now();
    state.tick(ui.ctx());
    // The clocks tick on the second.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1.02 - now.fract()));
    ui.add(egui::Label::new(sub(scheme, t("s_cz_lead"))).wrap());
    ui.add_space(6.0);
    hangar_card(ui, scheme, state, now);
    ui.add_space(GAP);
    start_card(ui, scheme, state);
    ui.add_space(GAP);
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_cz_overlay_kopf"));
        widgets::switch_row_help(ui, scheme, &t("s_cz_overlay"), &t("s_cz_overlay_h"), &mut state.overlay);
    });
    ui.add_space(GAP);
    // The self timers: one card, each group of them under its name.
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_cz_timer"));
        ui.add(egui::Label::new(sub(scheme, t("s_cz_timer_lead"))).wrap());
        let mut index = 0;
        for group in &GROUPS {
            let first = index;
            index += group.sections.iter().map(|section| section.timers.len()).sum::<usize>();
            ui.add_space(GAP);
            ui.add(egui::Label::new(RichText::new(group.name).strong()).wrap());
            ui.add_space(2.0);
            let mut offsets = Vec::new();
            let mut at = first;
            for section in group.sections {
                offsets.push(at);
                at += section.timers.len();
            }
            layout::columns_or_stack(ui, 3.0 * BOX_WIDTH + 2.0 * GAP, group.sections.len(), |ui, i| {
                let section = &group.sections[i];
                ui.label(sub(scheme, section.name));
                let per_row = (((ui.available_width() + GAP) / (BOX_WIDTH + GAP)).floor() as usize).max(1);
                for (row, specs) in section.timers.chunks(per_row).enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (n, spec) in specs.iter().enumerate() {
                            let entry = &mut state.timers[offsets[i] + row * per_row + n];
                            timer_box(ui, scheme, spec.name, spec.id, entry, now);
                        }
                    });
                }
            });
        }
    });
    ui.add_space(GAP);
    maps_section(ui, scheme, state);
}

/// One map built into the program: its tab, the file and who made it.
struct MapSpec {
    /// The setting value that remembers the tab.
    id: &'static str,
    name: &'static str,
    bytes: &'static [u8],
    creator: &'static str,
    url: &'static str,
}

/// The maps of the Contested Zones, with the files in `assets/maps` (their sources are in `QUELLE.txt`).
const MAPS: [MapSpec; 5] = [
    MapSpec {
        id: "checkmate",
        name: "Checkmate",
        bytes: include_bytes!("../assets/maps/checkmate.webp"),
        creator: "Zane_DragonBorn",
        url: versekit_core::credits::MAPS_STATIONS,
    },
    MapSpec {
        id: "orbituary",
        name: "Orbituary",
        bytes: include_bytes!("../assets/maps/orbituary.webp"),
        creator: "Zane_DragonBorn",
        url: versekit_core::credits::MAPS_STATIONS,
    },
    MapSpec {
        id: "ruin-station",
        name: "Ruin Station",
        bytes: include_bytes!("../assets/maps/ruin-station.webp"),
        creator: "Zane_DragonBorn",
        url: versekit_core::credits::MAPS_STATIONS,
    },
    MapSpec {
        id: "pyam-exhang-0-1",
        name: "PYAM-EXHANG-0-1",
        bytes: include_bytes!("../assets/maps/pyam-exhang-0-1.webp"),
        creator: "Kerast",
        url: versekit_core::credits::MAPS_PYAM,
    },
    MapSpec {
        id: "pyam-supvisr-3-4-5",
        name: "PYAM-SUPVISR-3-4/5",
        bytes: include_bytes!("../assets/maps/pyam-supvisr-3-4-5.webp"),
        creator: "Kerast",
        url: versekit_core::credits::MAPS_PYAM,
    },
];

/// The longest side a map is held in, in pixels: more is of no use on a screen, and a graphics card may hold
/// less in one texture (the window asks the card).
const MAP_SIDE: u32 = 4096;

/// A map as an image for a texture whose longest side is at most `side` pixels: a bigger one is made smaller,
/// in its proportions.
fn decode_map(bytes: &[u8], side: u32) -> Result<egui::ColorImage, String> {
    let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).map_err(|e| e.to_string())?;
    let picture = if picture.width().max(picture.height()) > side {
        picture.resize(side, side, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    let size = [picture.width() as usize, picture.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, picture.to_rgba8().as_raw()))
}

/// The setting that remembers which map stands open.
const SETTING_MAP: &str = "contested_karte";

/// The largest zoom of a map: four screen points to one pixel of the picture, where its details are as big as they
/// get. A frame that is bigger than the picture at this zoom fits the picture and zooms no further.
const MAP_ZOOM_MAX: f32 = 4.0;

/// The smallest and the largest zoom of a picture `picture` big in a frame `frame` big. The smallest shows the
/// whole picture inside the frame (the longer side fills it): a map cannot be zoomed out of reach.
fn map_zoom_range(frame: egui::Vec2, picture: egui::Vec2) -> (f32, f32) {
    let fit = (frame.x / picture.x).min(frame.y / picture.y);
    if fit.is_finite() && fit > 0.0 { (fit, MAP_ZOOM_MAX.max(fit)) } else { (1.0, 1.0) }
}

/// The part of the picture in view when the whole picture is shown: centred, with the room that is left on the
/// shorter side.
fn fit_view(frame: egui::Vec2, picture: egui::Vec2) -> egui::Rect {
    let (zoom, _) = map_zoom_range(frame, picture);
    egui::Rect::from_center_size((picture / 2.0).to_pos2(), frame / zoom)
}

/// `view` (the part of the picture in view, in the pixels of the picture) held to what the frame allows: zoomed
/// within [`map_zoom_range`], and moved so that the frame never leaves the picture, or, on a side where the picture
/// is shorter than the frame, so that the picture stays in the middle. A view that makes no sense shows the whole
/// picture.
fn constrain_view(view: egui::Rect, frame: egui::Vec2, picture: egui::Vec2) -> egui::Rect {
    let usable = view.is_finite() && view.width() > 0.0 && view.height() > 0.0;
    if !usable || frame.x <= 0.0 || frame.y <= 0.0 || picture.x <= 0.0 || picture.y <= 0.0 {
        return fit_view(frame, picture);
    }
    let (least, most) = map_zoom_range(frame, picture);
    let zoom = (frame.x / view.width()).min(frame.y / view.height()).clamp(least, most);
    let size = frame / zoom;
    let at = |centre: f32, seen: f32, whole: f32| {
        if seen >= whole { whole / 2.0 } else { centre.clamp(seen / 2.0, whole - seen / 2.0) }
    };
    let centre = view.center();
    egui::Rect::from_center_size(egui::pos2(at(centre.x, size.x, picture.x), at(centre.y, size.y, picture.y)), size)
}

/// The tabs of the maps and the chosen map, large: the mouse wheel with Ctrl zooms it, and so does the wheel alone
/// after a click into the map until the pointer leaves it; otherwise the wheel scrolls the page. Dragging moves the
/// map, a double click shows the whole map again, and under it stand its maker and a link to where it was published.
fn maps_section(ui: &mut Ui, scheme: &Scheme, state: &mut ContestedState) {
    widgets::card(ui, scheme, |ui| maps_card(ui, scheme, state));
    ui.add_space(GAP);
}

/// The contents of the card of the maps: title, how to use them, the tabs, the chosen map and its maker.
fn maps_card(ui: &mut Ui, scheme: &Scheme, state: &mut ContestedState) {
    widgets::card_title(ui, scheme, &t("s_cz_karten"));
    ui.add(egui::Label::new(sub(scheme, t("s_cz_karten_lead"))).wrap());
    ui.add_space(4.0);
    let names: Vec<&str> = MAPS.iter().map(|map| map.name).collect();
    widgets::tab_bar(ui, scheme, &names, &mut state.map);
    ui.add_space(GAP);
    let chosen = state.map.min(MAPS.len() - 1);
    let map = &MAPS[chosen];
    state.load_map(ui.ctx());
    let height = (ui.available_width() * 0.75).clamp(320.0, 760.0);
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    #[cfg(test)]
    {
        state.map_frame = outer;
    }
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    inner.set_clip_rect(outer.intersect(ui.clip_rect()));
    match state.map_tex.as_ref().filter(|(at, _)| *at == chosen) {
        Some((_, texture)) => {
            let name = tf("s_cz_karte_name", &[("0", Arg::from(map.name))]);
            let picture = egui::Image::from_texture(egui::load::SizedTexture::from_handle(texture))
                .alt_text(name.as_str())
                .fit_to_original_size(1.0);
            let (frame, picture_size) = (outer.size(), texture.size_vec2());
            let zoom = map_zoom_range(frame, picture_size);
            let view = &mut state.scenes[chosen];
            *view = constrain_view(*view, frame, picture_size);
            // Ctrl and the wheel come as a zoom, which the scene takes. The plain wheel zooms only a map clicked
            // into; otherwise the scene does not see it (it would move the map) and the page scrolls with it.
            let taken = state.map_taken;
            let scroll =
                if taken { egui::Vec2::ZERO } else { ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta)) };
            let shown = egui::Scene::new()
                .scroll_zooms(taken)
                .zoom_range(zoom.0..=zoom.1)
                .max_inner_size([8000.0, 8000.0])
                .show(&mut inner, view, |scene| scene.add(picture));
            ui.input_mut(|i| i.smooth_scroll_delta += scroll);
            if shown.response.clicked() || shown.response.drag_started() {
                state.map_taken = true;
            } else if !shown.response.contains_pointer() {
                state.map_taken = false;
            }
            // A double click shows the whole map again; everything else is held inside the frame.
            *view = if shown.response.double_clicked() {
                fit_view(frame, picture_size)
            } else {
                constrain_view(*view, frame, picture_size)
            };
            // The hand says that the map can be taken and moved.
            if shown.response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if shown.response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            // The wheel zooms a map clicked into: the page behind it does not scroll along.
            if taken && shown.response.contains_pointer() {
                ui.input_mut(|input| input.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }
        None if state.map_failed == Some(chosen) => {
            widgets::empty_state(&mut inner, scheme, "triangle-alert", &t("s_cz_karte_fehlt"));
        }
        None => {
            widgets::loading(&mut inner, scheme, &t("s_cz_karte_laedt"));
        }
    }
    ui.painter().rect_stroke(
        outer,
        crate::control_radius(scheme),
        egui::Stroke::new(1.0, color(scheme.line)),
        egui::StrokeKind::Inside,
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|row| {
        row.label(sub(scheme, t("s_cz_karte_von")));
        if widgets::external_link(row, scheme, map.creator).clicked() {
            row.ctx().open_url(egui::OpenUrl::new_tab(map.url));
        }
    });
}

/// The phase, the countdown, the lamps and the next openings.
fn hangar_card(ui: &mut Ui, scheme: &Scheme, state: &ContestedState, now: f64) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_cz_hangar"));
        let Some(start) = state.start() else {
            ui.add_space(4.0);
            let (head, line) = (t("s_cz_ohne_titel"), t("s_cz_ohne_text"));
            widgets::status_box(ui, scheme, scheme.gold, "triangle-alert", &head, &line);
            return;
        };
        let hangar = contested::hangar_at(start, now);
        let seconds = now.floor() as i64;
        ui.add_space(4.0);
        ui.horizontal_top(|row| {
            row.spacing_mut().item_spacing.x = 20.0;
            ring(row, scheme, &hangar);
            row.vertical(|column| {
                let name = match hangar.phase {
                    Phase::Closed => "s_cz_geschlossen",
                    Phase::Open => "s_cz_offen",
                    Phase::Resetting => "s_cz_reset",
                };
                column.add(egui::Label::new(figure(column, t(name), phase_color(scheme, hangar.phase))).wrap());
                column.add(egui::Label::new(opening_line(start, &hangar, now)).wrap());
                column.add_space(6.0);
                leds(column, scheme, &hangar);
                column.add_space(4.0);
                let times = contested::countdown(hangar.next_lamp.map_or(0, |(_, left)| left));
                let line = match hangar.next_lamp {
                    Some((lamp, _)) => tf(
                        "s_cz_led_zeile",
                        &[
                            ("0", Arg::from(hangar.green)),
                            ("1", Arg::from(LAMPS)),
                            ("2", Arg::from(lamp)),
                            ("3", Arg::from(times)),
                        ],
                    ),
                    None => tf(
                        "s_cz_led_reset",
                        &[
                            ("0", Arg::from(hangar.green)),
                            ("1", Arg::from(LAMPS)),
                            ("2", Arg::from(contested::countdown(hangar.to_change))),
                        ],
                    ),
                };
                column.add(egui::Label::new(sub(scheme, line)).wrap());
            });
        });
        ui.add_space(8.0);
        widgets::card_title(ui, scheme, &t("s_cz_naechste"));
        let lang = language::current();
        egui::Grid::new("contested-openings").num_columns(3).spacing([16.0, 2.0]).show(ui, |grid| {
            for opening in contested::next_openings(start, now, OPENINGS_SHOWN) {
                let day = match contested::day(lang, opening.opens, seconds) {
                    Some(contested::Day::Today) => t("s_cz_heute"),
                    Some(contested::Day::Tomorrow) => t("s_cz_morgen"),
                    Some(contested::Day::Other(date)) => date,
                    None => String::new(),
                };
                grid.label(RichText::new(day).strong());
                grid.label(format!("{} – {}", contested::clock(opening.opens), contested::clock(opening.closes)));
                let left = tf(
                    "s_cz_in",
                    &[("0", Arg::from(versekit_core::formats::duration((opening.opens - seconds) as f64)))],
                );
                grid.label(sub(scheme, left));
                grid.end_row();
            }
        });
    });
}

/// When the hangar opens and closes, for the phase it is in.
fn opening_line(start: i64, hangar: &Hangar, now: f64) -> RichText {
    let seconds = now.floor() as i64;
    let text = match hangar.phase {
        Phase::Open => {
            let closes = seconds + hangar.to_change;
            tf(
                "s_cz_offen_seit",
                &[
                    ("0", Arg::from(contested::clock(closes - contested::OPEN))),
                    ("1", Arg::from(contested::clock(closes))),
                ],
            )
        }
        _ => contested::next_openings(start, now, 1).first().map_or_else(String::new, |opening| {
            tf(
                "s_cz_oeffnet_um",
                &[
                    ("0", Arg::from(contested::clock(opening.opens))),
                    ("1", Arg::from(contested::clock(opening.closes))),
                ],
            )
        }),
    };
    RichText::new(text)
}

/// The circle around the countdown: it fills with the progress of the phase, in the color of the phase.
fn ring(ui: &mut Ui, scheme: &Scheme, hangar: &Hangar) {
    let (rect, response) = ui.allocate_exact_size(vec2(RING_SIZE, RING_SIZE), Sense::hover());
    let (name, until) = match hangar.phase {
        Phase::Closed => ("s_cz_geschlossen", "s_cz_bis_offen"),
        Phase::Open => ("s_cz_offen", "s_cz_bis_zu"),
        Phase::Resetting => ("s_cz_reset", "s_cz_bis_neu"),
    };
    let countdown = contested::countdown(hangar.to_change);
    let said = format!("{}: {countdown} {}", t(name), t(until));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, true, &said));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let (center, radius) = (rect.center(), (RING_SIZE - RING_WIDTH) / 2.0);
    let tint = phase_color(scheme, hangar.phase);
    let painter = ui.painter();
    painter.circle_stroke(center, radius, egui::Stroke::new(RING_WIDTH, color(scheme.track)));
    let fraction = hangar.progress();
    if fraction > 0.0 {
        let steps = ((fraction * 96.0).ceil() as usize).max(2);
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|i| {
                let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction * (i as f32 / steps as f32);
                center + radius * vec2(angle.cos(), angle.sin())
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(RING_WIDTH, tint)));
    }
    let big = fonts::bold(egui::TextStyle::Heading.resolve(ui.style()).size);
    let small = egui::TextStyle::Small.resolve(ui.style());
    painter.text(center - vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, countdown, big, color(scheme.fg));
    painter.text(center + vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, t(until), small, color(scheme.sub));
}

/// The five numbered boxes over the door of the hangar.
fn leds(ui: &mut Ui, scheme: &Scheme, hangar: &Hangar) {
    let width = LAMPS as f32 * LED_SIZE + (LAMPS - 1) as f32 * LED_GAP;
    let (rect, response) = ui.allocate_exact_size(vec2(width, LED_SIZE), Sense::hover());
    let said = tf("s_cz_lampen", &[("0", Arg::from(hangar.green)), ("1", Arg::from(LAMPS))]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &said));
    response.on_hover_text(said.as_str());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let font = fonts::bold(egui::TextStyle::Body.resolve(ui.style()).size);
    let painter = ui.painter();
    for (i, lamp) in hangar.lamps.iter().enumerate() {
        let at =
            egui::Rect::from_min_size(rect.min + vec2(i as f32 * (LED_SIZE + LED_GAP), 0.0), vec2(LED_SIZE, LED_SIZE));
        let (edge, text, fill) = match lamp {
            Lamp::Red => (color(scheme.red), color(scheme.red), color(scheme.red).gamma_multiply(0.2)),
            Lamp::Green => (color(scheme.accent), color(scheme.accent), color(scheme.accent).gamma_multiply(0.2)),
            Lamp::Dark => (color(scheme.line), color(scheme.sub), color(scheme.track)),
        };
        painter.rect(at, control_radius(scheme), fill, egui::Stroke::new(2.0, edge), egui::StrokeKind::Inside);
        painter.text(at.center(), egui::Align2::CENTER_CENTER, (i + 1).to_string(), font.clone(), text);
    }
}

/// Five small dots for the lamps of the hangar, for the tile of the overview: green and red ones filled, dark ones
/// as a ring.
pub fn lamp_dots(ui: &mut Ui, scheme: &Scheme, lamps: &[Lamp; LAMPS]) {
    const DOT: f32 = 10.0;
    const GAP_BETWEEN: f32 = 4.0;
    let width = LAMPS as f32 * DOT + (LAMPS - 1) as f32 * GAP_BETWEEN;
    let (rect, response) = ui.allocate_exact_size(vec2(width, DOT + 2.0), Sense::hover());
    let green = lamps.iter().filter(|lamp| **lamp == Lamp::Green).count();
    let said = tf("s_cz_lampen", &[("0", Arg::from(green)), ("1", Arg::from(LAMPS))]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &said));
    for (i, lamp) in lamps.iter().enumerate() {
        let center = egui::pos2(rect.min.x + DOT / 2.0 + i as f32 * (DOT + GAP_BETWEEN), rect.center().y);
        let radius = DOT / 2.0 - 0.5;
        match lamp {
            Lamp::Red => ui.painter().circle_filled(center, radius, color(scheme.red)),
            Lamp::Green => ui.painter().circle_filled(center, radius, color(scheme.accent)),
            Lamp::Dark => ui.painter().circle_stroke(center, radius, egui::Stroke::new(1.0, color(scheme.sub))),
        };
    }
}

/// The source and the age of the start, the buttons that set it from what is seen at the hangar, and the field
/// for the advanced.
fn start_card(ui: &mut Ui, scheme: &Scheme, state: &mut ContestedState) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_cz_start_kopf"));
        let lang = language::current();
        let source_name = format!("{} ({})", contested::SOURCE_NAME, contested::SOURCE_OPERATOR);
        if let Some(start) = state.start() {
            ui.add(egui::Label::new(tf("s_cz_abgeglichen", &[("0", Arg::from(contested::stamp(lang, start)))])).wrap());
        }
        ui.horizontal_wrapped(|row| {
            row.label(sub(scheme, t("s_cz_quelle")));
            if state.kept.source() == Some(contested::StartSource::Hand) {
                row.label(t("s_cz_quelle_hand"));
            } else if widgets::external_link(row, scheme, &source_name).clicked() {
                row.ctx().open_url(egui::OpenUrl::new_tab(contested::SOURCE_URL));
            }
        });
        if state.kept.source() == Some(contested::StartSource::Hand) {
            ui.horizontal_wrapped(|row| {
                row.label(sub(scheme, t("s_cz_quelle_seite")));
                if widgets::external_link(row, scheme, &source_name).clicked() {
                    row.ctx().open_url(egui::OpenUrl::new_tab(contested::SOURCE_URL));
                }
            });
        }
        let line = if state.job.is_some() {
            t("s_cz_holt")
        } else if state.failed {
            t("s_cz_holen_fehl")
        } else if state.offline && state.kept.site.is_none() {
            t("s_cz_netz_aus")
        } else if state.kept.site.is_none() {
            t("s_cz_nie")
        } else {
            let at = contested::when(lang, state.kept.fetched as i64, contested::now() as i64);
            tf("s_cz_geholt", &[("0", Arg::from(at))])
        };
        ui.add(egui::Label::new(sub(scheme, line)).wrap());
        ui.add_space(6.0);
        ui.add(egui::Label::new(sub(scheme, t("s_cz_hand_kopf"))).wrap());
        ui.horizontal_wrapped(|row| {
            let now = versekit_core::export::now_unix();
            let (opened, reset) = (t("s_cz_hand_auf"), t("s_cz_hand_reset"));
            if row.add(widgets::Button::new(&opened, ButtonKind::Normal, scheme).icon("door-open")).clicked() {
                state.seen_at_hangar(Phase::Open, now);
            }
            if row.add(widgets::Button::new(&reset, ButtonKind::Normal, scheme).icon("rotate-ccw")).clicked() {
                state.seen_at_hangar(Phase::Resetting, now);
            }
            if state.kept.hand.is_some() {
                let back = t("s_cz_hand_weg");
                if row.add(widgets::Button::new(&back, ButtonKind::Strong, scheme).icon("undo-2")).clicked() {
                    state.apply_hand(None);
                }
            }
        });
        ui.add_space(4.0);
        widgets::collapsing(&t("s_cz_fortgeschritten")).id_salt("contested-hand-field").show(ui, |ui| {
            ui.horizontal_wrapped(|row| {
                let hint = t("s_cz_hand_hint");
                let field = row.add(widgets::text_field(&mut state.hand_text, &hint, scheme).desired_width(150.0));
                let entered = field.lost_focus() && row.input(|i| i.key_pressed(egui::Key::Enter));
                let word = t("s_cz_hand_setzen");
                let take = widgets::Button::new(&word, ButtonKind::Normal, scheme);
                if row.add_enabled(!state.hand_text.trim().is_empty(), take).clicked() || entered {
                    state.set_hand();
                }
            });
            if state.hand_wrong {
                ui.label(RichText::new(t("s_cz_hand_falsch")).small().color(color(scheme.red_text())));
            }
            ui.add(egui::Label::new(sub(scheme, t("s_cz_hand_hilfe"))).wrap());
        });
    });
}

/// One self timer: its name, the time with a minute less and more, start or stop, and reset.
fn timer_box(ui: &mut Ui, scheme: &Scheme, name: &str, id: &str, entry: &mut Entry, now: f64) {
    ui.push_id(id, |ui| {
        let highlighted = entry.timer.highlighted(now);
        let stroke = if highlighted {
            egui::Stroke::new(2.0, color(scheme.accent))
        } else {
            egui::Stroke::new(1.0, color(scheme.control_border()))
        };
        let fill = if highlighted { scheme.selected } else { scheme.field };
        let frame = egui::Frame::new()
            .fill(color(fill))
            .stroke(stroke)
            .corner_radius(control_radius(scheme))
            .inner_margin(egui::Margin::same(8));
        // The frame takes the layout of the row it stands in, so its rows are laid out top down here.
        frame.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(BOX_INNER);
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let state = entry.timer.state();
                let left = entry.timer.left(now);
                ui.horizontal(|row| {
                    row.label(RichText::new(name).strong());
                    if state == TimerState::Done {
                        row.with_layout(Layout::right_to_left(Align::Center), |row| {
                            row.label(RichText::new(t("s_cz_fertig")).small().color(color(scheme.accent)));
                        });
                    }
                });
                ui.horizontal(|row| {
                    let minus = widgets::icon_button(row, scheme, "minus", &t("s_cz_minus"));
                    row.allocate_ui_with_layout(
                        vec2(BOX_INNER - 2.0 * widgets::ICON_BUTTON_SIZE - 8.0, widgets::ICON_BUTTON_SIZE),
                        Layout::centered_and_justified(egui::Direction::TopDown),
                        |center| {
                            let tint = match state {
                                TimerState::Done => scheme.accent,
                                TimerState::Running if left <= WARN_SECONDS => scheme.gold,
                                _ => scheme.fg,
                            };
                            center.label(figure(center, contested::minutes_seconds(left), color(tint)));
                        },
                    );
                    let plus = widgets::icon_button(row, scheme, "plus", &t("s_cz_plus"));
                    minus.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name}: {}", t("s_cz_minus")))
                    });
                    plus.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name}: {}", t("s_cz_plus")))
                    });
                    if minus.clicked() {
                        entry.timer.adjust(-1, now);
                    }
                    if plus.clicked() {
                        entry.timer.adjust(1, now);
                    }
                });
                ui.horizontal(|row| {
                    let running = state == TimerState::Running;
                    let (word, icon) = if running { (t("s_cz_stopp"), "pause") } else { (t("s_cz_start"), "play") };
                    let spoken = format!("{name}: {word}");
                    let button = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon(icon).spoken(&spoken);
                    if row.add_enabled(running || left > 0, button).clicked() {
                        if running {
                            entry.timer.stop(now);
                        } else {
                            entry.timer.start(now);
                        }
                    }
                    let reset = widgets::icon_button(row, scheme, "rotate-ccw", &t("s_cz_zurueck"));
                    reset.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name}: {}", t("s_cz_zurueck")))
                    });
                    if reset.clicked() {
                        entry.timer.reset();
                    }
                });
            })
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rect;
    use versekit_core::language::{Choice, set_thread_language};
    use versekit_core::net::Fake;

    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// The page in a window of `width`: the texts it drew and how far it sticks out.
    fn frame(ctx: &egui::Context, state: &mut ContestedState, width: f32) -> (Vec<String>, f32) {
        let scheme = versekit_core::theme::current();
        let mut over = 0.0;
        let mut shown = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 4000.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                over = layout::page(ui, scheme, "contested-test", |ui| show_page(ui, scheme, state)).overflow;
            });
            output.textures_delta.clear();
            shown = texts(&output.shapes);
        }
        (shown, over)
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx
    }

    fn paths_in(name: &str, off: bool) -> (Paths, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("versekit-contested-seite-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let mut vars = vec![(versekit_core::legacy::ENV_HOME, home.as_str())];
        if off {
            vars.push((versekit_core::legacy::ENV_NO_NET, "1"));
        }
        let host = versekit_core::paths::Host::new(versekit_core::pypath::Flavor::native(), &vars, &home);
        (Paths::new(host), dir)
    }

    #[test]
    fn the_page_shows_the_phase_the_openings_the_source_and_every_timer() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let ctx = context();
        let mut state = ContestedState::sample(contested::now(), 2_500);
        for width in [640.0, 900.0, 1800.0] {
            let (shown, over) = frame(&ctx, &mut state, width);
            assert_eq!(over, 0.0, "width {width}");
            for key in ["s_cz_geschlossen", "s_cz_bis_offen", "s_cz_naechste", "s_cz_hangar", "s_cz_timer"] {
                assert!(shown.contains(&t(key)), "{key} at {width}: {shown:?}");
            }
            assert!(shown.iter().any(|s| s.contains(contested::SOURCE_NAME)), "the source is named: {shown:?}");
            assert!(shown.iter().any(|s| s.contains(contested::SOURCE_OPERATOR)));
            assert!(shown.iter().any(|s| s.starts_with("Startzeit abgeglichen: ")), "{shown:?}");
            assert_eq!(shown.iter().filter(|s| s.contains(" – ")).count(), OPENINGS_SHOWN, "from and until");
            assert_eq!(shown.iter().filter(|s| s.starts_with("in ")).count(), OPENINGS_SHOWN, "and how long");
            for number in 1..=LAMPS {
                assert!(shown.contains(&number.to_string()), "box {number}: {shown:?}");
            }
            assert!(shown.iter().any(|s| s.starts_with("1/5 grün · LED 2 schaltet in ")), "{shown:?}");
            for group in &GROUPS {
                assert!(shown.contains(&group.name.to_string()), "{} at {width}", group.name);
            }
            assert_eq!(shown.iter().filter(|s| *s == "15:00").count(), 8, "eight timers of 15 minutes");
            assert_eq!(shown.iter().filter(|s| *s == "30:00").count(), 9, "nine of 30");
        }
        set_thread_language(Choice::Auto);
    }

    #[test]
    fn without_a_start_the_page_says_so_instead_of_showing_times() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let ctx = context();
        let mut state = ContestedState::default();
        let (shown, over) = frame(&ctx, &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_cz_ohne_titel")), "{shown:?}");
        for key in ["s_cz_geschlossen", "s_cz_offen", "s_cz_reset", "s_cz_naechste"] {
            assert!(!shown.contains(&t(key)), "{key} must not show without a start: {shown:?}");
        }
        assert!(shown.contains(&t("s_cz_nie")), "{shown:?}");
        set_thread_language(Choice::Auto);
    }

    #[test]
    fn the_start_is_fetched_on_entering_once_and_kept() {
        let (paths, dir) = paths_in("holen", false);
        let net = Arc::new(Fake::default().with(contested::START_URL, "1792860059"));
        let mut state = ContestedState::with_net(&paths, Some(net.clone()));
        assert_eq!(state.start(), None);
        state.enter();
        let ctx = context();
        for _ in 0..400 {
            state.tick(&ctx);
            if state.start().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(state.start(), Some(1_792_860_059));
        assert_eq!(contested::load(&paths).site, Some(1_792_860_059), "kept in the data folder");
        // Entered again at once: not asked again, and a new window reads what is kept.
        state.enter();
        assert!(state.job.is_none());
        assert_eq!(net.asked().len(), 1);
        let again = ContestedState::with_net(&paths, Some(net.clone()));
        assert_eq!(again.start(), Some(1_792_860_059));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_fetched_with_the_net_switched_off_or_when_the_page_fails() {
        let (paths, dir) = paths_in("aus", true);
        let net = Arc::new(Fake::default().with(contested::START_URL, "1792860059"));
        let mut state = ContestedState::with_net(&paths, Some(net.clone()));
        state.enter();
        assert!(state.job.is_none() && net.asked().is_empty());
        let _ = std::fs::remove_dir_all(&dir);

        let (paths, dir) = paths_in("fehler", false);
        let down = Arc::new(Fake::default());
        let mut state = ContestedState::with_net(&paths, Some(down.clone()));
        state.enter();
        let ctx = context();
        for _ in 0..400 {
            state.tick(&ctx);
            if state.job.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.failed && state.start().is_none());
        // A visit right after a failed one does not try again.
        state.enter();
        assert!(state.job.is_none());
        assert_eq!(down.asked().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_start_typed_by_hand_beats_the_page_until_it_is_taken_back() {
        let (paths, dir) = paths_in("hand", false);
        let seed = Fake::default().with(contested::START_URL, "1792860059");
        assert!(matches!(contested::refresh(&paths, &seed, 5.0), Outcome::Fresh(_)));
        let mut state = ContestedState::with_net(&paths, None);
        assert_eq!(state.start(), Some(1_792_860_059));
        state.hand_text = "nonsense".into();
        state.set_hand();
        assert!(state.hand_wrong && state.kept.hand.is_none());
        state.hand_text = "1792860100".into();
        state.set_hand();
        assert!(!state.hand_wrong);
        assert_eq!(state.start(), Some(1_792_860_100));
        assert!(state.hand_text.is_empty());
        assert_eq!(contested::load(&paths).hand, Some(1_792_860_100), "kept for the next start");
        state.apply_hand(None);
        assert_eq!(state.start(), Some(1_792_860_059));
        assert_eq!(contested::load(&paths).hand, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_button_hangar_just_opened_makes_it_open_now_and_stays_until_the_source_is_chosen_again() {
        let (paths, dir) = paths_in("auf", false);
        let seed = Fake::default().with(contested::START_URL, "1792860059");
        assert!(matches!(contested::refresh(&paths, &seed, 5.0), Outcome::Fresh(_)));
        let mut state = ContestedState::with_net(&paths, None);
        assert_eq!(state.kept.source(), Some(contested::StartSource::Page));
        let now = versekit_core::export::now_unix();
        state.seen_at_hangar(Phase::Open, now);
        let hangar = contested::hangar_at(state.start().unwrap(), now as f64);
        assert_eq!((hangar.phase, hangar.to_change, hangar.green), (Phase::Open, contested::OPEN, LAMPS));
        assert_eq!(state.kept.source(), Some(contested::StartSource::Hand));
        assert_eq!(contested::load(&paths).hand, state.start(), "kept for the next start");
        // A fetch brings the start of the page again; the hand still counts.
        assert!(matches!(contested::refresh(&paths, &seed, 9.0), Outcome::Fresh(_)));
        assert_eq!(ContestedState::with_net(&paths, None).start(), state.start());
        state.apply_hand(None);
        assert_eq!(state.start(), Some(1_792_860_059));
        assert_eq!(state.kept.source(), Some(contested::StartSource::Page));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_button_hangar_just_resetting_makes_it_reset_now() {
        let mut state = ContestedState::sample(contested::now(), 100);
        let now = versekit_core::export::now_unix();
        state.seen_at_hangar(Phase::Resetting, now);
        let hangar = contested::hangar_at(state.start().unwrap(), now as f64);
        assert_eq!((hangar.phase, hangar.to_change, hangar.green), (Phase::Resetting, contested::RESET, 0));
        assert_eq!(state.kept.source(), Some(contested::StartSource::Hand));
        // Nothing is set from a closed hangar: the buttons know no such case.
        let before = state.start();
        state.seen_at_hangar(Phase::Closed, now);
        assert_eq!(state.start(), before);
    }

    #[test]
    fn the_source_and_the_age_of_the_start_stand_under_the_hangar() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let ctx = context();
        let mut state = ContestedState::sample(contested::now(), 2_500);
        let (shown, _) = frame(&ctx, &mut state, 900.0);
        assert!(shown.iter().any(|s| s.contains(contested::SOURCE_NAME)));
        assert!(!shown.contains(&t("s_cz_quelle_hand")));
        state.seen_at_hangar(Phase::Open, versekit_core::export::now_unix());
        let (shown, over) = frame(&ctx, &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_cz_quelle_hand")), "by hand: {shown:?}");
        assert!(shown.contains(&t("s_cz_hand_weg")), "{shown:?}");
        assert!(shown.contains(&t("s_cz_offen")), "{shown:?}");
        assert!(shown.iter().any(|s| s.contains(contested::SOURCE_NAME)), "the page stays named: {shown:?}");
        set_thread_language(Choice::Auto);
    }

    #[test]
    fn the_overlay_switch_is_off_from_the_start_and_is_kept_as_a_setting() {
        let key = versekit_core::overlay::keys::HANGAR_TIMER;
        let (paths, dir) = paths_in("overlay", true);
        let mut state = ContestedState::with_net(&paths, None);
        assert!(!state.overlay);
        assert!(state.setting_values().contains(&(key, Value::Bool(false))));
        state.overlay = true;
        assert!(state.setting_values().contains(&(key, Value::Bool(true))));
        assert!(paths.set_setting(key, Value::Bool(true)));
        assert!(ContestedState::with_net(&paths, None).overlay, "read back at the next start");
        // The preview has no data folder to keep it in.
        assert!(ContestedState::sample(contested::now(), 10).setting_values().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_built_in_map_decodes_and_has_its_maker_and_link_in_the_list_of_sources() {
        let sources =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/maps/QUELLE.txt"))
                .expect("QUELLE.txt");
        for map in &MAPS {
            // The size from the header of the file; the big maps take seconds to decode in a debug build, so
            // only the small ones are decoded whole.
            let (width, height) =
                image::ImageReader::with_format(std::io::Cursor::new(map.bytes), image::ImageFormat::WebP)
                    .into_dimensions()
                    .unwrap_or_else(|error| panic!("{}: {error}", map.id));
            assert!(width >= 600 && height >= 600, "{}: {width}x{height}", map.id);
            assert!(width.max(height) <= 8192, "{} fits a texture", map.id);
            if map.bytes.len() < 100_000 {
                let picture = image::load_from_memory_with_format(map.bytes, image::ImageFormat::WebP)
                    .unwrap_or_else(|error| panic!("{}: {error}", map.id));
                assert_eq!((picture.width(), picture.height()), (width, height), "{}", map.id);
            }
            assert!(sources.contains(&format!("{}.webp", map.id)), "{} is named in QUELLE.txt", map.id);
            assert!(sources.contains(map.creator) && sources.contains(map.url), "{}", map.id);
            let credited =
                versekit_core::credits::BUNDLED.iter().any(|c| c.name.contains(map.creator) && c.url == map.url);
            assert!(credited, "{} stands on the page of thanks and licences", map.creator);
        }
    }

    /// The size of the biggest map in its file is more than the graphics cards of some computers take in one
    /// texture: the map is made smaller, in its proportions, and a small one stays as it is.
    #[test]
    fn a_map_bigger_than_the_texture_of_the_card_is_made_smaller_in_its_proportions() {
        let small = MAPS.iter().find(|map| map.id == "pyam-exhang-0-1").expect("a small map");
        let (width, height) =
            image::ImageReader::with_format(std::io::Cursor::new(small.bytes), image::ImageFormat::WebP)
                .into_dimensions()
                .expect("size");
        let whole = decode_map(small.bytes, 4096).expect("decodes");
        assert_eq!(whole.size, [width as usize, height as usize], "as it is where it fits");
        let smaller = decode_map(small.bytes, 500).expect("decodes");
        assert!(smaller.size[0].max(smaller.size[1]) <= 500, "{:?}", smaller.size);
        let proportion = |size: [usize; 2]| size[0] as f32 / size[1] as f32;
        assert!((proportion(smaller.size) - proportion(whole.size)).abs() < 0.01, "{:?}", smaller.size);
        assert!(decode_map(b"no picture", 4096).is_err(), "a file that is none is told, not a crash");
        // The big map of the page of the Contested Zones lies over what a card may hold in one texture.
        let (big, _) = image::ImageReader::with_format(std::io::Cursor::new(MAPS[0].bytes), image::ImageFormat::WebP)
            .into_dimensions()
            .map(|(w, h)| (w.max(h), ()))
            .expect("size");
        assert!(big > 4096, "the biggest map is {big} pixels wide: it needs the downscaling");
    }

    #[test]
    fn the_map_that_stands_open_becomes_one_texture_and_the_one_before_it_is_let_go() {
        let ctx = context();
        let mut state = ContestedState { maps_on: true, map: 3, ..ContestedState::default() };
        let mut waited = 0;
        while !state.map_tex.as_ref().is_some_and(|(at, _)| *at == 3) {
            frame(&ctx, &mut state, 900.0);
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 3000, "the map did not come");
        }
        let texture_size = state.map_tex.as_ref().map(|(_, texture)| texture.size()).expect("texture");
        assert!(texture_size[0].max(texture_size[1]) <= ctx.input(|i| i.max_texture_side), "{texture_size:?}");
        // The other tab: the old texture goes at once, the new one comes.
        state.map = 4;
        frame(&ctx, &mut state, 900.0);
        assert!(!state.map_tex.as_ref().is_some_and(|(at, _)| *at == 3));
        assert!(state.map_job.as_ref().is_some_and(|(at, _)| *at == 4) || state.map_tex.is_some());
    }

    #[test]
    fn a_map_that_cannot_be_made_is_told_on_the_page() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let ctx = context();
        let mut state = ContestedState { maps_on: true, map_failed: Some(0), ..ContestedState::default() };
        let (shown, over) = frame(&ctx, &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&t("s_cz_karte_fehlt")), "{shown:?}");
        assert!(state.map_job.is_none(), "no new try at every frame");
        set_thread_language(Choice::Auto);
    }

    #[test]
    fn the_tabs_switch_the_maps_under_them_with_their_maker_and_the_choice_is_kept() {
        let _language = crate::language_lock();
        set_thread_language(Choice::De);
        let ctx = context();
        let (paths, dir) = paths_in("karten", true);
        let mut state = ContestedState::with_net(&paths, None);
        assert_eq!(state.map, 0, "the first map from the start");
        let (shown, over) = frame(&ctx, &mut state, 900.0);
        assert_eq!(over, 0.0);
        for map in &MAPS {
            assert!(shown.contains(&map.name.to_string()), "tab {}: {shown:?}", map.name);
        }
        assert!(shown.contains(&"Zane_DragonBorn".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Kerast".to_string()));
        state.map = 3;
        let (shown, over) = frame(&ctx, &mut state, 900.0);
        assert_eq!(over, 0.0);
        assert!(shown.contains(&"Kerast".to_string()) && !shown.contains(&"Zane_DragonBorn".to_string()), "{shown:?}");
        assert!(state.setting_values().contains(&(SETTING_MAP, Value::str("pyam-exhang-0-1"))));
        assert!(paths.set_setting(SETTING_MAP, Value::str("pyam-exhang-0-1")));
        assert_eq!(ContestedState::with_net(&paths, None).map, 3, "kept for the next start");
        assert!(paths.set_setting(SETTING_MAP, Value::str("gibt-es-nicht")));
        assert_eq!(ContestedState::with_net(&paths, None).map, 0, "an unknown map is the first");
        let _ = std::fs::remove_dir_all(&dir);
        set_thread_language(Choice::Auto);
    }

    #[test]
    fn a_timer_that_ran_out_is_announced_once_whatever_page_is_open() {
        let ctx = context();
        let mut state = ContestedState::default();
        let now = contested::now();
        state.timer_mut(0).start(now - 1_000.0);
        state.timer_mut(5).start(now);
        state.tick(&ctx);
        assert_eq!(state.take_ran_out(), ["Checkmate · Blue Keycards · Terminal 1"]);
        state.tick(&ctx);
        assert!(state.take_ran_out().is_empty(), "once");
        assert_eq!(state.timer_mut(5).state(), TimerState::Running, "the others go on");
    }

    // ------------------------------------------------------------ the zoom of the maps

    const PICTURE: egui::Vec2 = vec2(4096.0, 2048.0);

    fn size_of(rect: Rect) -> egui::Vec2 {
        rect.size()
    }

    #[test]
    fn the_zoom_of_a_map_runs_from_the_whole_map_in_the_frame_to_four_to_one() {
        let frame = vec2(900.0, 675.0);
        let (least, most) = map_zoom_range(frame, PICTURE);
        assert!((least - 900.0 / 4096.0).abs() < 1e-6, "the longer side fills the frame: {least}");
        assert_eq!(most, MAP_ZOOM_MAX);
        // A picture smaller than its frame at the largest zoom: the range does not turn round.
        let (least, most) = map_zoom_range(vec2(900.0, 675.0), vec2(100.0, 100.0));
        assert!(least <= most && most >= 6.75, "{least} {most}");
        assert_eq!(map_zoom_range(vec2(0.0, 0.0), PICTURE), (1.0, 1.0), "no frame, no range, and no crash");
    }

    #[test]
    fn a_view_is_held_inside_the_picture_and_the_whole_map_stays_in_the_middle() {
        let frame = vec2(900.0, 675.0);
        let whole = fit_view(frame, PICTURE);
        assert!((whole.center() - (PICTURE / 2.0).to_pos2()).length() < 0.01, "{whole:?}");
        assert!(
            whole.width() >= PICTURE.x - 0.01 && whole.height() >= PICTURE.y,
            "the whole map is in view: {whole:?}"
        );
        // Zoomed far out (a view as big as ten maps) and pushed far off: back to the whole map, in the middle.
        let far = Rect::from_center_size(egui::pos2(-90_000.0, 50_000.0), PICTURE * 10.0);
        assert_eq!(constrain_view(far, frame, PICTURE), whole);
        // Zoomed in to four to one and pushed over the edge: it stops at the edge, its size stays.
        let zoomed = frame / MAP_ZOOM_MAX;
        let pushed = constrain_view(Rect::from_center_size(egui::pos2(-500.0, 99_000.0), zoomed), frame, PICTURE);
        assert!((size_of(pushed) - zoomed).length() < 0.01, "{pushed:?}");
        assert!(pushed.min.x >= -0.01 && pushed.max.y <= PICTURE.y + 0.01, "{pushed:?}");
        assert!(pushed.min.x.abs() < 0.01 && (pushed.max.y - PICTURE.y).abs() < 0.01, "right on the edge: {pushed:?}");
        // Zoomed in further than the largest zoom: the largest.
        let tiny = constrain_view(Rect::from_center_size(egui::pos2(100.0, 100.0), vec2(9.0, 7.0)), frame, PICTURE);
        assert!((size_of(tiny) - zoomed).length() < 0.01, "{tiny:?}");
        // Nonsense shows the whole map.
        assert_eq!(constrain_view(Rect::ZERO, frame, PICTURE), whole);
        assert_eq!(constrain_view(Rect::NOTHING, frame, PICTURE), whole);
    }

    fn drive(ctx: &egui::Context, state: &mut ContestedState, events: Vec<egui::Event>) {
        drive_in(ctx, state, events, 4000.0);
    }

    /// One frame of the page in a window 1000 points wide and `height` high.
    fn drive_in(ctx: &egui::Context, state: &mut ContestedState, events: Vec<egui::Event>, height: f32) {
        let scheme = versekit_core::theme::current();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1000.0, height))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            layout::page(ui, scheme, "contested-map-test", |ui| show_page(ui, scheme, state));
        });
        output.textures_delta.clear();
    }

    /// The tab `map` open with its texture there and the pointer over the middle of its frame.
    fn open_map(ctx: &egui::Context, state: &mut ContestedState, map: usize) -> (egui::Pos2, egui::Vec2) {
        state.maps_on = true;
        state.map = map;
        let mut waited = 0;
        while !state.map_tex.as_ref().is_some_and(|(at, _)| *at == map) {
            drive(ctx, state, Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited += 1;
            assert!(waited < 3000, "the map {map} did not come");
        }
        let picture = state.map_tex.as_ref().map(|(_, texture)| texture.size_vec2()).expect("texture");
        drive(ctx, state, Vec::new());
        let centre = state.map_frame.center();
        drive(ctx, state, vec![egui::Event::PointerMoved(centre)]);
        (centre, picture)
    }

    fn wheel(y: f32) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
            source: egui::MouseWheelSource::Wheel,
        }
    }

    /// The wheel with Ctrl held.
    fn zoom_wheel(y: f32) -> egui::Event {
        match wheel(y) {
            egui::Event::MouseWheel { unit, delta, phase, source, .. } => {
                egui::Event::MouseWheel { unit, delta, phase, source, modifiers: egui::Modifiers::COMMAND }
            }
            other => other,
        }
    }

    /// Over the map the plain wheel scrolls the page and leaves the map as it is. After a click into the map the
    /// wheel zooms it until the pointer leaves the map; Ctrl and the wheel zoom it without a click.
    #[test]
    fn the_plain_wheel_scrolls_the_page_over_the_map_until_it_is_clicked() {
        let ctx = context();
        let mut state = ContestedState::default();
        let (centre, _) = open_map(&ctx, &mut state, 3);
        // The same view but for the rounding of a frame that moved on the screen.
        let same = |a: Rect, b: Rect| (a.min - b.min).length() < 0.5 && (a.max - b.max).length() < 0.5;
        // A window that ends 100 points below the middle of the map: the page has something left to scroll.
        let height = centre.y + 100.0;
        // Frames after the wheel for the scrolling of the page to come to rest.
        const SETTLE: usize = 90;
        let run = |state: &mut ContestedState, events: Vec<egui::Event>, frames: usize| {
            drive_in(&ctx, state, events, height);
            for _ in 0..frames {
                drive_in(&ctx, state, Vec::new(), height);
            }
        };
        run(&mut state, vec![egui::Event::PointerMoved(centre)], 1);
        let (top, view) = (state.map_frame.min.y, state.scenes[3]);
        for _ in 0..6 {
            run(&mut state, vec![wheel(-30.0)], 0);
        }
        run(&mut state, Vec::new(), SETTLE);
        let scrolled = state.map_frame.min.y;
        assert!(scrolled < top - 20.0, "the page scrolled: {top} -> {scrolled}");
        assert!(same(state.scenes[3], view), "the map stays as it was: {:?} {view:?}", state.scenes[3]);
        // A click into the map: now the wheel zooms it, and the page stays.
        run(&mut state, vec![button(centre, true)], 0);
        run(&mut state, vec![button(centre, false)], 1);
        for _ in 0..6 {
            run(&mut state, vec![wheel(400.0)], 0);
        }
        run(&mut state, Vec::new(), SETTLE);
        assert!(state.scenes[3].width() < view.width() - 10.0, "zoomed in: {:?} {view:?}", state.scenes[3]);
        assert!((state.map_frame.min.y - scrolled).abs() < 0.5, "the page stayed: {}", state.map_frame.min.y);
        // The pointer leaves the map and comes back: the wheel scrolls the page again.
        run(&mut state, vec![egui::Event::PointerGone], 1);
        run(&mut state, vec![egui::Event::PointerMoved(centre)], 1);
        let zoomed = state.scenes[3];
        for _ in 0..6 {
            run(&mut state, vec![wheel(30.0)], 0);
        }
        run(&mut state, Vec::new(), SETTLE);
        assert!(state.map_frame.min.y > scrolled + 20.0, "the page scrolled back: {}", state.map_frame.min.y);
        assert!(same(state.scenes[3], zoomed), "the map stays as it was: {:?} {zoomed:?}", state.scenes[3]);
        // Ctrl and the wheel zoom without a click, and the page stays.
        let (at, before) = (state.map_frame.min.y, state.scenes[3]);
        for _ in 0..6 {
            run(&mut state, vec![zoom_wheel(-400.0)], 0);
        }
        run(&mut state, Vec::new(), SETTLE);
        assert!(state.scenes[3].width() > before.width() + 10.0, "zoomed out: {:?} {before:?}", state.scenes[3]);
        assert!((state.map_frame.min.y - at).abs() < 0.5, "the page stayed: {}", state.map_frame.min.y);
    }

    fn button(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// The frame is full of picture, or the whole picture is in the frame, on each side: the map is never out of reach.
    fn in_reach(view: Rect, picture: egui::Vec2) -> bool {
        let side = |low: f32, high: f32, whole: f32| {
            (low >= -0.5 && high <= whole + 0.5) || (low <= 0.5 && high >= whole - 0.5)
        };
        side(view.min.x, view.max.x, picture.x) && side(view.min.y, view.max.y, picture.y)
    }

    #[test]
    fn no_map_can_be_zoomed_out_of_its_frame_or_dragged_out_of_it() {
        let ctx = context();
        for (map, shown) in MAPS.iter().enumerate() {
            let mut state = ContestedState::default();
            let (centre, picture) = open_map(&ctx, &mut state, map);
            let frame = state.map_frame.size();
            let whole = fit_view(frame, picture);
            let name = shown.name;
            // The wheel far out: the whole map in the frame, and no smaller.
            for step in 0..40 {
                drive(&ctx, &mut state, vec![zoom_wheel(-400.0)]);
                assert!(in_reach(state.scenes[map], picture), "{name} out, step {step}: {:?}", state.scenes[map]);
            }
            let view = state.scenes[map];
            assert!(
                (size_of(view) - size_of(whole)).length() < 1.0,
                "{name}: smallest zoom shows the whole map: {view:?} {whole:?}"
            );
            // In again, then the mouse dragged far away in every direction, one frame after the other.
            for step in 0..25 {
                drive(&ctx, &mut state, vec![zoom_wheel(400.0)]);
                assert!(in_reach(state.scenes[map], picture), "{name} in, step {step}: {:?}", state.scenes[map]);
            }
            assert!(size_of(state.scenes[map]).x < size_of(whole).x, "{name}: it zoomed in");
            for direction in [vec2(1.0, 1.0), vec2(-1.0, 0.5), vec2(0.3, -1.0), vec2(-1.0, -1.0)] {
                drive(&ctx, &mut state, vec![button(centre, true)]);
                for step in 1..=12 {
                    let at = centre + direction * (step as f32 * 700.0);
                    drive(&ctx, &mut state, vec![egui::Event::PointerMoved(at)]);
                    assert!(
                        in_reach(state.scenes[map], picture),
                        "{name} drag {direction:?}, step {step}: {:?}",
                        state.scenes[map]
                    );
                }
                drive(
                    &ctx,
                    &mut state,
                    vec![button(centre + direction * 14_000.0, false), egui::Event::PointerMoved(centre)],
                );
            }
            // Out again from the edge it was dragged to: the whole map, in the middle.
            for _ in 0..40 {
                drive(&ctx, &mut state, vec![zoom_wheel(-400.0)]);
            }
            let view = state.scenes[map];
            assert!(
                (size_of(view) - size_of(whole)).length() < 1.0 && (view.center() - whole.center()).length() < 1.0,
                "{name}: back out to the whole map: {view:?} {whole:?}"
            );
            // Far in: not beyond four to one.
            for _ in 0..70 {
                drive(&ctx, &mut state, vec![zoom_wheel(400.0)]);
            }
            let zoom = frame.x / state.scenes[map].width();
            assert!(zoom <= MAP_ZOOM_MAX + 0.01, "{name}: zoomed to {zoom}");
            assert!(in_reach(state.scenes[map], picture));
        }
    }

    #[test]
    fn a_double_click_shows_the_whole_map_again() {
        let ctx = context();
        let mut state = ContestedState::default();
        let (centre, picture) = open_map(&ctx, &mut state, 3);
        let whole = fit_view(state.map_frame.size(), picture);
        for _ in 0..20 {
            drive(&ctx, &mut state, vec![zoom_wheel(400.0)]);
        }
        assert!(state.scenes[3].width() < whole.width() - 10.0, "it is zoomed in: {:?}", state.scenes[3]);
        for event in [button(centre, true), button(centre, false), button(centre, true), button(centre, false)] {
            drive(&ctx, &mut state, vec![event]);
        }
        let view = state.scenes[3];
        assert!(
            (size_of(view) - size_of(whole)).length() < 1.0 && (view.center() - whole.center()).length() < 1.0,
            "{view:?} {whole:?}"
        );
    }
}
