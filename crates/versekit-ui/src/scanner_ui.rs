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

//! The signature scanner on its pages: the switch and the teaching of the digits on the page Mining, and the
//! model of the screen on which the window with the scanned material is dragged.
//!
//! The scanner itself runs in the overlay (another program); this window sees it through the settings and
//! through the file the overlay leaves the last picture in. A change goes back by the signal that the settings
//! changed.

use egui::{
    CursorIcon, Key, Pos2, Rect, RichText, Role, Sense, Stroke, StrokeKind, TextureOptions, Ui, WidgetInfo, pos2, vec2,
};
use versekit_core::jsonio::Value;
use versekit_core::language::{Arg, t, tf};
use versekit_core::mining_data::MiningData;
use versekit_core::paths::Paths;
use versekit_core::signature_scan::{self, OwnFiles};
use versekit_core::signature_watch::{self, Frame, Screen, Source};
use versekit_core::signature_window::{self, Hint, Layout};
use versekit_core::theme::{Color, Scheme};
use versekit_core::window_signal::{self, Role as WindowRole, Signal};

use crate::game_picture::{self, Fetched, GamePicture};
use crate::trade::{Job, Polled};
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// The model of the screen stands for a game window of this size; the pill is a box near its middle.
const GAME: (i32, i32, i32, i32) = (0, 0, 1600, 900);
const PILL: (i32, i32, i32, i32) = (700, 430, 200, 60);
/// How wide the two models are drawn at most, in points: the picture of the game must show the signature large
/// enough to draw the frame around it, and both grow alike with the window.
const MODEL_WIDTH: f32 = 960.0;
/// The number in the signature box of the model, the dark tone of the HUD and the size of the handles of a frame.
const EXAMPLE: u64 = 21_125;
const HUD: egui::Color32 = egui::Color32::from_rgb(14, 22, 28);
const HUD_TEXT: egui::Color32 = egui::Color32::from_rgb(226, 236, 241);
const HANDLE: f32 = 8.0;
/// The house green, the colour of the search frame in every scheme.
const BRAND: egui::Color32 = egui::Color32::from_rgb(0x9c, 0xe4, 0x30);
/// How often the picture file is looked at, in seconds.
const LOOK_SECONDS: f64 = 0.5;
/// The arrow keys move the window in the model by this many per mille.
const KEY_STEP: i32 = 10;
/// The signal to show the frame goes this many seconds after the click.
const SHOW_DELAY: f64 = 1.5;
/// The longest side of the preview of the picture, in points.
const PREVIEW: (usize, usize) = (360, 130);
/// The smallest frame, per mille of the game window (the settings take no smaller one).
const FRAME_MIN: i32 = 10;
/// The arrow keys move the search frame by this many per mille.
const FRAME_KEY_STEP: i32 = 2;
/// How far around a corner of the frame its handle takes hold, in points of the model.
const HANDLE_REACH: f32 = HANDLE * 0.75;

/// What stands behind a model: the picture of the game, or the grey ground standing for a game window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub texture: Option<egui::TextureId>,
    /// Width and height of the game window.
    pub size: (i32, i32),
}

impl Ground {
    /// No picture: a game window of 16 : 9.
    pub const MODEL: Ground = Ground { texture: None, size: (GAME.2, GAME.3) };

    /// Height to width of the game window; a strange window is kept between a quarter and a square.
    fn ratio(&self) -> f32 {
        (self.size.1 as f32 / self.size.0.max(1) as f32).clamp(0.25, 1.0)
    }

    /// The game window in the points the model computes in: as wide as the model's game, as high as its shape.
    fn game(&self) -> (i32, i32, i32, i32) {
        (0, 0, GAME.2, (GAME.2 as f32 * self.ratio()).round() as i32)
    }

    /// Takes the room of the model on the page: at most `most` points wide, in the shape of the game window.
    fn allocate(&self, ui: &mut Ui, sense: Sense, most: f32) -> (Rect, egui::Response) {
        let width = ui.available_width().min(most);
        let taken = ui.allocate_exact_size(vec2(width, width * self.ratio()), sense);
        // A picture area: content of its card for the checks, as a row is, and an editor of its own width.
        crate::widgets::drawn_cards::note_row(taken.0);
        crate::widgets::drawn_cards::note_picture(taken.0);
        taken
    }
}

/// Where an area in per mille of the game window lies on the model drawn in `rect`.
pub fn mille_to_model(rect: Rect, [x, y, w, h]: [i32; 4]) -> Rect {
    let at = |part: i32, whole: f32| part as f32 / 1000.0 * whole;
    Rect::from_min_size(
        rect.min + vec2(at(x, rect.width()), at(y, rect.height())),
        vec2(at(w, rect.width()), at(h, rect.height())),
    )
}

/// A point of the model drawn in `rect` in per mille of the game window, not cut to it.
fn model_to_mille(rect: Rect, at: Pos2) -> (f32, f32) {
    ((at.x - rect.min.x) / rect.width() * 1000.0, (at.y - rect.min.y) / rect.height() * 1000.0)
}

/// What a press on the model of the search frame takes hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    /// The frame, to move it.
    Move,
    /// The handle of a corner, to change the size: 0 left top, 1 right top, 2 left bottom, 3 right bottom.
    Corner(usize),
    /// Nothing: a new frame is drawn.
    New,
}

impl Grip {
    /// The pointer over it, and while it is held.
    fn cursor(self, held: bool) -> CursorIcon {
        match (self, held) {
            (Grip::Move, false) => CursorIcon::Grab,
            (Grip::Move, true) => CursorIcon::Grabbing,
            (Grip::Corner(0 | 3), _) => CursorIcon::ResizeNwSe,
            (Grip::Corner(_), _) => CursorIcon::ResizeNeSw,
            (Grip::New, _) => CursorIcon::Crosshair,
        }
    }
}

/// What a press at `at` (per mille) takes hold of; `reach` is how far a handle takes hold around its corner, per
/// mille across and down. The handles come before the frame.
pub fn grip_at(frame: Option<[i32; 4]>, at: (f32, f32), reach: (f32, f32)) -> Grip {
    let Some([x, y, w, h]) = frame else { return Grip::New };
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
    if let Some(corner) =
        corners.iter().position(|(cx, cy)| (at.0 - *cx as f32).abs() <= reach.0 && (at.1 - *cy as f32).abs() <= reach.1)
    {
        return Grip::Corner(corner);
    }
    let inside = at.0 >= x as f32 && at.0 <= (x + w) as f32 && at.1 >= y as f32 && at.1 <= (y + h) as f32;
    if inside { Grip::Move } else { Grip::New }
}

/// The frame between two corners, or `None` where it is smaller than [`FRAME_MIN`] in a direction.
fn between((x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> Option<[i32; 4]> {
    let frame = [x0.min(x1), y0.min(y1), (x0 - x1).abs(), (y0 - y1).abs()];
    (frame[2] >= FRAME_MIN && frame[3] >= FRAME_MIN).then_some(frame)
}

/// The frame after a drag from `from` to `to` (per mille) that took hold of `grip` of the frame `start`; it stays
/// inside the game window. `None` where the drag gives no frame (too small).
pub fn drag_frame(start: Option<[i32; 4]>, grip: Grip, from: (i32, i32), to: (i32, i32)) -> Option<[i32; 4]> {
    let to = (to.0.clamp(0, 1000), to.1.clamp(0, 1000));
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    match (grip, start) {
        (Grip::Move, Some([x, y, w, h])) => Some([(x + dx).clamp(0, 1000 - w), (y + dy).clamp(0, 1000 - h), w, h]),
        (Grip::Corner(corner), Some([x, y, w, h])) => {
            let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
            let (held, fixed) = (corners[corner.min(3)], corners[3 - corner.min(3)]);
            between(fixed, ((held.0 + dx).clamp(0, 1000), (held.1 + dy).clamp(0, 1000)))
        }
        _ => between((from.0.clamp(0, 1000), from.1.clamp(0, 1000)), to),
    }
}

/// A drag on the model of the search frame while it runs: what it took hold of, the frame and the point it began
/// with, and the frame it gives so far.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    grip: Grip,
    start: Option<[i32; 4]>,
    from: (i32, i32),
    now: Option<[i32; 4]>,
}

/// How far a handle takes hold around its corner, per mille across and down of the model in `rect`: at most a
/// third of the shorter side of the frame, so that a small frame can still be taken by its inside.
pub fn handle_reach(rect: Rect, frame: Option<[i32; 4]>) -> (f32, f32) {
    let shorter = frame.map_or(f32::INFINITY, |[_, _, w, h]| {
        (w as f32 / 1000.0 * rect.width()).min(h as f32 / 1000.0 * rect.height())
    });
    let reach = HANDLE_REACH.min(shorter / 3.0);
    (reach / rect.width() * 1000.0, reach / rect.height() * 1000.0)
}

/// While a model has the focus, the arrow keys move what it holds and do not move the focus on. egui takes the
/// filter only from the second frame with the focus on: that frame is asked for at once.
fn keep_arrows(ui: &Ui, response: &egui::Response) {
    if response.gained_focus() {
        ui.ctx().request_repaint();
    }
    if response.has_focus() {
        let filter = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
    }
}

/// What the pages know of the scanner.
pub struct ScannerState {
    paths: Option<Paths>,
    /// The scanner is switched on (the setting that is also the consent to grab the screen).
    pub on: bool,
    pub layout: Layout,
    /// The area the player drew for the search, per mille of the game window.
    pub search_frame: Option<[i32; 4]>,
    /// The place of the signature found last, per mille of the game window; shown where no frame stands.
    pub remembered: Option<[i32; 4]>,
    typed: String,
    /// The picture that stands for teaching, and the number of the one set aside with "next image".
    frame: Option<Frame>,
    ignore: Option<u64>,
    looked_at: f64,
    texture: Option<(u64, egui::TextureHandle)>,
    /// What teaching said: the lines and whether the check passed.
    said: Option<(Vec<String>, bool)>,
    /// The file of the picture standing, when it is one kept for its unsure number; the files set aside.
    pending: Option<String>,
    skipped: Vec<String>,
    /// When the signal "show the frame" is due.
    show_due: Option<f64>,
    /// A number was taught since the page was made (the small window closes then).
    pub taught: bool,
    /// The picture of the game behind the two models and its texture.
    picture: Option<GamePicture>,
    picture_texture: Option<egui::TextureHandle>,
    /// The fetch of a new picture while it runs, and when it began.
    fetch: Option<(Job<Fetched>, f64)>,
    /// The text key of what the last fetch came to, where it brought no picture.
    fetch_said: Option<&'static str>,
}

impl Default for ScannerState {
    fn default() -> Self {
        ScannerState {
            paths: None,
            on: false,
            layout: Layout::default(),
            search_frame: None,
            remembered: None,
            typed: String::new(),
            frame: None,
            ignore: None,
            looked_at: f64::NEG_INFINITY,
            texture: None,
            said: None,
            pending: None,
            skipped: Vec::new(),
            show_due: None,
            taught: false,
            picture: None,
            picture_texture: None,
            fetch: None,
            fetch_said: None,
        }
    }
}

impl ScannerState {
    /// With own data: the settings and the picture file of the data folder.
    pub fn with_files(paths: &Paths) -> ScannerState {
        let mut state = ScannerState { paths: Some(Paths::new(paths.host.clone())), ..ScannerState::default() };
        state.load(paths);
        state
    }

    /// Reads the settings again.
    pub fn load(&mut self, paths: &Paths) {
        self.on = paths.setting_bool(signature_watch::SETTING, false);
        self.layout = Layout::read(paths);
        let hint = Hint::read(paths);
        self.search_frame = hint.frame;
        self.remembered = hint.remembered();
        if self.picture.is_none() {
            self.picture = GamePicture::read(&paths.app_file(game_picture::FILE));
        }
    }

    /// What stands behind the models: the picture of the game where there is one.
    pub fn ground(&mut self, ctx: &egui::Context) -> Ground {
        let Some(picture) = &self.picture else { return Ground::MODEL };
        let texture = self.picture_texture.get_or_insert_with(|| {
            let image = egui::ColorImage::from_rgb([picture.w, picture.h], &picture.rgb);
            ctx.load_texture("signatur-spielbild", image, TextureOptions::LINEAR)
        });
        Ground { texture: Some(texture.id()), size: picture.game }
    }

    /// Can a picture of the game be fetched here (own data, a system that can grab)?
    fn can_fetch(&self) -> bool {
        self.paths.is_some() && versekit_core::screen_grab::supported()
    }

    /// Starts waiting for the game to come to the front, to grab its window once then.
    pub fn start_fetch(&mut self, ctx: &egui::Context, now: f64) {
        self.start_fetch_from(ctx, now, Box::new(Screen), game_picture::WAIT);
    }

    /// As [`ScannerState::start_fetch`], with the pictures from `source` (the checks make up a screen).
    fn start_fetch_from(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        mut source: Box<dyn Source>,
        wait: std::time::Duration,
    ) {
        let Some(paths) = &self.paths else { return };
        if self.fetch.is_some() {
            return;
        }
        let file = paths.app_file(game_picture::FILE);
        let job = Job::spawn(ctx, "spielbild", move || {
            let fetched = game_picture::fetch(source.as_mut(), wait, game_picture::LOOK_EVERY);
            match fetched {
                Fetched::Got(picture) if !picture.write(&file) => {
                    Fetched::Failed(versekit_core::screen_grab::GrabError::Failed)
                }
                other => other,
            }
        });
        self.fetch = Some((job, now));
        self.fetch_said = None;
    }

    /// Takes the picture once the fetch is done; until then the page is woken every second for the countdown.
    pub fn poll_fetch(&mut self, ctx: &egui::Context) {
        let Some((job, _)) = &self.fetch else { return };
        let said = match job.poll() {
            Polled::Running => {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
                return;
            }
            Polled::Done(Fetched::Got(picture)) => {
                self.picture = Some(picture);
                self.picture_texture = None;
                None
            }
            Polled::Done(Fetched::NoGame) => Some("s_sg_bild_kein_spiel"),
            Polled::Done(Fetched::Failed(_)) | Polled::Lost => Some("s_sg_bild_fehler"),
        };
        self.fetch = None;
        self.fetch_said = said;
    }

    /// The line under the model of the frame: the countdown, what the fetch came to, or that there is no picture.
    fn ground_line(&self, now: f64) -> Option<String> {
        if let Some((_, started)) = &self.fetch {
            let left = (game_picture::WAIT.as_secs_f64() - (now - started)).ceil().max(0.0) as i64;
            return Some(tf("s_sg_bild_warte", &[("0", Arg::from(left))]));
        }
        if let Some(key) = self.fetch_said {
            return Some(t(key));
        }
        (self.picture.is_none() && self.can_fetch()).then(|| t("s_sg_bild_keins"))
    }

    /// The place of the signature for the model of the window: the place found last, else the middle of the
    /// frame drawn, else (`None`) the place of the model.
    fn pill_place(&self) -> Option<[i32; 4]> {
        // The size of the box of the model, per mille of its game window.
        let (pw, ph) = (PILL.2 * 1000 / GAME.2, PILL.3 * 1000 / GAME.3);
        self.remembered.or_else(|| self.search_frame.map(|[x, y, w, h]| [x + (w - pw) / 2, y + (h - ph) / 2, pw, ph]))
    }

    /// The settings of the scanner as the settings file keeps them.
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            (signature_watch::SETTING, Value::Bool(self.on)),
            (signature_window::keys::OFFSET_X, Value::int(i64::from(self.layout.dx))),
            (signature_window::keys::OFFSET_Y, Value::int(i64::from(self.layout.dy))),
            (signature_window::keys::SIZE, self.layout.size.value()),
            (signature_window::keys::OVERLAY_LINE, Value::Bool(self.layout.overlay_line)),
            (signature_window::keys::PLACE, Value::str(self.layout.place.setting())),
            (signature_window::keys::LINGER, Value::int(self.layout.linger)),
            (signature_window::keys::FRAME, Hint { frame: self.search_frame, places: Vec::new() }.frame_value()),
        ]
    }

    /// Switches the scanner: stored at once, and the overlay is told (it starts or ends the watch).
    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        if let Some(paths) = &self.paths {
            paths.set_setting(signature_watch::SETTING, Value::Bool(on));
            window_signal::send_to(WindowRole::Overlay, Signal::Settings);
        }
    }

    /// The player asked to see the area that is read over the game. The frame he just drew is written first, and
    /// the overlay reads it: the signal goes a moment later.
    pub fn request_show(&mut self, now: f64) {
        self.show_due = Some(now + SHOW_DELAY);
    }

    /// Sends the signal once its time has come; asks for a frame until then.
    pub fn poll_show(&mut self, ctx: &egui::Context) {
        let Some(due) = self.show_due else { return };
        let now = ctx.input(|i| i.time);
        if now >= due {
            self.show_due = None;
            window_signal::send_to(WindowRole::Overlay, Signal::ShowFrame);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now));
        }
    }

    /// Tells the overlay that the settings changed (the place or size of the window).
    pub fn tell_overlay(&self) {
        if self.paths.is_some() {
            window_signal::send_to(WindowRole::Overlay, Signal::Settings);
        }
    }

    /// The files of the taught templates, the folder of the taught images and the file of the last picture.
    fn files(&self) -> Option<(OwnFiles, String, String)> {
        let paths = self.paths.as_ref()?;
        let own = OwnFiles::in_folder(|name| paths.app_file(name));
        let folder = std::path::Path::new(&own.shared).parent()?.to_string_lossy().into_owned();
        let base = folder.trim_end_matches(['/', '\\']);
        Some((
            own,
            format!("{base}/{}", signature_scan::SAMPLE_FOLDER),
            format!("{base}/{}", signature_watch::LAST_FRAME_FILE),
        ))
    }

    /// The folder of the pictures the overlay kept because their number was not sure.
    fn pending_folder(&self) -> Option<String> {
        let (_, _, last) = self.files()?;
        let base = std::path::Path::new(&last).parent()?.to_string_lossy().into_owned();
        Some(format!("{}/{}", base.trim_end_matches(['/', '\\']), signature_watch::PENDING_FOLDER))
    }

    /// Takes a picture while none stands: first the newest one kept for its unsure number, else the newest the
    /// overlay left; what was set aside is not taken again.
    fn follow(&mut self, ui: &Ui) {
        let now = ui.input(|i| i.time);
        if self.frame.is_some() || now - self.looked_at < LOOK_SECONDS {
            return;
        }
        self.looked_at = now;
        if let Some(folder) = self.pending_folder()
            && let Some((path, frame)) =
                signature_watch::pending(&folder).into_iter().find(|(path, _)| !self.skipped.contains(path))
        {
            self.pending = Some(path);
            self.frame = Some(frame);
            return;
        }
        let Some((_, _, file)) = self.files() else { return };
        let Some(fresh) = signature_watch::read_last_frame(&file) else { return };
        if self.ignore == Some(fresh.number) {
            return;
        }
        self.ignore = None;
        self.frame = Some(fresh);
    }

    /// How many pictures wait that were kept for their unsure number.
    fn pending_count(&self) -> usize {
        self.pending_folder().map_or(0, |folder| signature_watch::pending(&folder).len())
    }

    /// Releases the picture that stands: the next one found is shown (a picture kept stays kept).
    fn next(&mut self) {
        match self.pending.take() {
            Some(path) => self.skipped.push(path),
            None => self.ignore = self.frame.as_ref().map(|f| f.number),
        }
        self.frame = None;
        self.said = None;
        self.typed.clear();
        self.texture = None;
    }

    /// The picture that stands is not wanted: a kept one is gone for good.
    fn discard(&mut self) {
        if let Some(path) = self.pending.take() {
            signature_watch::drop_pending(&path);
        }
        self.next();
    }

    /// The typed number is taught for the picture that stands; what came of it is said below.
    fn learn(&mut self, mining: &MiningData) {
        let Some(frame) = &self.frame else {
            self.said = Some((vec![t("scan_warte")], false));
            return;
        };
        let Some((own, samples, _)) = self.files() else { return };
        let known = signature_scan::templates_by_variant(signature_scan::SHIPPED_TEMPLATES, &own);
        let taught = signature_scan::learn_variants(&frame.images, &self.typed, frame.variant, &known, &own, &samples);
        let tally = match taught {
            Ok(tally) => tally,
            Err(word) => {
                self.said = Some((vec![t(&format!("scan_grund_{word}"))], false));
                return;
            }
        };
        // A picture kept is taught: it is done with.
        if let Some(path) = self.pending.take() {
            signature_watch::drop_pending(&path);
        }
        self.taught = true;
        // The digits are new where the scanner would not have read them right before; all known means: enough.
        let digits: String = self.typed.chars().filter(char::is_ascii_digit).collect();
        let number = digits.parse::<u64>().map(signature_window::with_thousands).unwrap_or_default();
        let mut lines = vec![if tally.new == 0 && tally.unclear.is_empty() {
            tf("scan_gelernt_alle", &[("0", Arg::from(number.as_str())), ("1", Arg::from(tally.found))])
        } else {
            tf(
                "scan_gelernt",
                &[("0", Arg::from(number.as_str())), ("1", Arg::from(tally.new)), ("2", Arg::from(tally.found))],
            )
        }];
        if !tally.unclear.is_empty() {
            let places = tally.unclear.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            lines.push(tf("scan_gelernt_unklar", &[("0", Arg::from(places.as_str()))]));
        }
        let after = signature_scan::templates_by_variant(signature_scan::SHIPPED_TEMPLATES, &own);
        let reading = signature_scan::read_variants(&frame.images, &after, &signature_scan::possible_values(mining));
        let passed = reading.reading.value.is_some_and(|v| v.to_string() == digits);
        lines.push(if passed {
            tf("scan_probe_ok", &[("0", Arg::from(number.as_str()))])
        } else {
            t("scan_probe_nein")
        });
        self.said = Some((lines, passed));
        self.typed.clear();
    }
}

/// The picture of the crop as a texture, grown by a whole number as far as the preview allows, shrunk to the
/// width there is when it is wider (else it pushes the window and its text past the edge).
fn picture(ui: &mut Ui, state: &mut ScannerState) {
    let Some(frame) = &state.frame else { return };
    if state.texture.as_ref().is_none_or(|(number, _)| *number != frame.number) {
        let image = egui::ColorImage::from_gray([frame.image.w, frame.image.h], &frame.image.data);
        state.texture = Some((frame.number, ui.ctx().load_texture("signatur-bild", image, TextureOptions::NEAREST)));
    }
    if let Some((_, texture)) = &state.texture {
        let zoom = picture_zoom((frame.image.w, frame.image.h), ui.available_width());
        ui.image((texture.id(), vec2(frame.image.w as f32 * zoom, frame.image.h as f32 * zoom)));
    }
}

/// The scale of a crop of `size` pixels in `room` points of width: a whole number up to 6 within the preview,
/// below 1 only to fit the room.
fn picture_zoom(size: (usize, usize), room: f32) -> f32 {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let whole = (PREVIEW.0 / w).min(PREVIEW.1 / h).clamp(1, 6) as f32;
    whole.min(room.max(1.0) / w as f32)
}

/// The scanner on the page Mining, in a card of its own: the switch and, while it is on, the teaching of the digits.
pub fn show_box(ui: &mut Ui, scheme: &Scheme, state: &mut ScannerState, mining: &MiningData) {
    widgets::card(ui, scheme, |ui| {
        widgets::card_title(ui, scheme, &t("s_bg_scan_kopf"));
        if !versekit_core::screen_grab::supported() {
            widgets::hint(ui, scheme, &t("s_sg_nur_windows"));
            return;
        }
        let mut on = state.on;
        let toggled = widgets::switch_row_help(ui, scheme, &t("s_bg_scan_kopf"), &t("s_bg_scan_h"), &mut on).changed();
        if toggled {
            state.set_on(on);
        }
        if !state.on {
            widgets::hint(ui, scheme, &t("scan_wache_aus"));
            return;
        }
        ui.add_space(6.0);
        ui.label(RichText::new(t("s_bg_scan_anlernen")).strong());
        show_teaching(ui, scheme, state, mining);
    });
}

/// The teaching of the digits: the picture that stands, the number typed for it and what came of it. On the page
/// Mining and in the small window the overlay opens above the game.
pub fn show_teaching(ui: &mut Ui, scheme: &Scheme, state: &mut ScannerState, mining: &MiningData) {
    state.follow(ui);
    if state.frame.is_none() {
        widgets::hint(ui, scheme, &t("scan_warte"));
    } else {
        picture(ui, state);
        let line = match state.frame.as_ref().and_then(|f| f.value) {
            Some(value) => {
                let text = signature_window::describe(value, mining);
                RichText::new(tf("scan_gelesen", &[("0", Arg::from(text.line().as_str()))])).color(color(scheme.accent))
            }
            None => RichText::new(t("scan_grund_unsicher")).color(color(scheme.gold)),
        };
        ui.label(line);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|row| {
            row.label(t("scan_richtig"));
            let field = row.add(widgets::text_field(&mut state.typed, "", scheme).desired_width(110.0));
            let enter = field.lost_focus() && row.input(|i| i.key_pressed(Key::Enter));
            let go = row.add(widgets::Button::new(&t("scan_anlernen"), ButtonKind::Strong, scheme)).clicked();
            if go || enter {
                state.learn(mining);
            }
        });
    }
    // Wraps instead of running past the edge of a narrow window.
    ui.horizontal_wrapped(|row| {
        if row.add(widgets::Button::new(&t("scan_naechstes"), ButtonKind::Normal, scheme)).clicked() {
            state.next();
        }
        if state.pending.is_some()
            && row.add(widgets::Button::new(&t("s_sg_verwerfen"), ButtonKind::Normal, scheme)).clicked()
        {
            state.discard();
        }
        if row.add(widgets::Button::new(&t("scan_ordner"), ButtonKind::Normal, scheme)).clicked()
            && let Some((_, samples, _)) = state.files()
        {
            let _ = versekit_core::jsonio::make_dirs(&samples);
            versekit_core::browser::show_folder(&samples);
        }
    });
    if let Some((lines, passed)) = &state.said {
        for line in lines {
            let tone = if *passed { scheme.accent } else { scheme.gold };
            ui.label(RichText::new(line).color(color(tone)));
        }
    }
    if let Some((_, samples, _)) = state.files() {
        let count = signature_scan::samples(&samples).len();
        if count > 0 {
            widgets::hint(ui, scheme, &tf("s_bg_scan_bilder", &[("0", Arg::from(count))]));
        }
    }
    let waiting = state.pending_count();
    if waiting > 0 {
        widgets::hint(ui, scheme, &tf("s_sg_gemerkt", &[("0", Arg::from(waiting))]));
    }
}

/// The model of the screen: the game picture with the pill in it and the window with the scanned material,
/// which is dragged (or moved with the arrow keys) to where it is to stand. `pill` is the place of the signature
/// in per mille of the game window (`None`: the place of the model). Gives whether the place changed.
pub fn place_model(ui: &mut Ui, scheme: &Scheme, layout: &mut Layout, ground: &Ground, pill: Option<[i32; 4]>) -> bool {
    let (rect, _) = ground.allocate(ui, Sense::hover(), MODEL_WIDTH);
    let game = ground.game();
    let scale = rect.width() / game.2 as f32;
    let share = |part: i32, whole: i32| (f64::from(part) * f64::from(whole) / 1000.0).round() as i32;
    let pill_at = match pill {
        Some([x, y, w, h]) => (share(x, game.2), share(y, game.3), share(w, game.2), share(h, game.3)),
        None => (PILL.0, PILL.1 * game.3 / GAME.3, PILL.2, PILL.3 * game.3 / GAME.3),
    };
    let size = signature_window::window_size(layout.size.font_px(game.3), 18, true);
    let top_left = signature_window::place(pill_at, game, size, layout);
    let to_model = |x: i32, y: i32, w: i32, h: i32| {
        Rect::from_min_size(rect.min + vec2(x as f32, y as f32) * scale, vec2(w as f32, h as f32) * scale)
    };
    let window = to_model(top_left.0, top_left.1, size.0, size.1);
    let id = ui.make_persistent_id("signatur-fenster-modell");
    let response = ui.interact(window, id, Sense::click_and_drag());
    let mut changed = false;
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    keep_arrows(ui, &response);

    // Where the hand took hold of the window: the press, not the place where the drag began to count.
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        ui.data_mut(|d| d.insert_temp(id, origin - window.min));
    }
    if response.dragged()
        && let (Some(pointer), Some(grab)) =
            (response.interact_pointer_pos(), ui.data(|d| d.get_temp::<egui::Vec2>(id)))
    {
        let corner = (pointer - grab - rect.min) / scale;
        let (dx, dy) =
            signature_window::offset_for((corner.x.round() as i32, corner.y.round() as i32), pill_at, game, size);
        if (dx, dy) != (layout.dx, layout.dy) {
            (layout.dx, layout.dy) = (dx, dy);
            changed = true;
        }
    }
    if response.has_focus() {
        let step = |positive: Key, negative: Key| {
            ui.input(|i| i32::from(i.key_pressed(positive)) - i32::from(i.key_pressed(negative))) * KEY_STEP
        };
        let (mx, my) = (step(Key::ArrowRight, Key::ArrowLeft), step(Key::ArrowDown, Key::ArrowUp));
        if mx != 0 || my != 0 {
            let limit = signature_window::OFFSET_LIMIT;
            layout.dx = (layout.dx + mx).clamp(-limit, limit);
            layout.dy = (layout.dy + my).clamp(-limit, limit);
            changed = true;
        }
    }

    paint_ground(ui, scheme, rect, ground);
    let pill_rect = to_model(pill_at.0, pill_at.1, pill_at.2, pill_at.3);
    // On the picture of the game the signature is its own: only where it stands is marked.
    if ground.texture.is_none() {
        paint_pill(ui, pill_rect);
    } else {
        // Light on dark, so it shows on a light picture as well as on a dark one.
        paint_dashed(ui, pill_rect, Stroke::new(3.5, HUD));
        paint_dashed(ui, pill_rect, Stroke::new(1.5, HUD_TEXT));
    }
    let painter = ui.painter();
    let (now_x, now_y) = signature_window::place(pill_at, game, size, layout);
    let window_now = to_model(now_x, now_y, size.0, size.1);
    let stroke = if response.has_focus() || response.hovered() || response.dragged() {
        Stroke::new(2.5, color(scheme.accent))
    } else {
        Stroke::new(1.5, color(scheme.accent))
    };
    painter.rect(window_now, 3.0, HUD, stroke, StrokeKind::Inside);
    painter.text(
        window_now.center(),
        egui::Align2::CENTER_CENTER,
        tf("s_bg_sig_treffer", &[("0", Arg::from(4)), ("1", Arg::from("Quantainium"))]),
        egui::FontId::proportional((window_now.height() * 0.4).clamp(7.0, 14.0)),
        color(scheme.accent),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, t("s_sg_lage")));
    let cursor = if response.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab };
    response.on_hover_cursor(cursor);
    changed
}

/// The game picture of a model: the picture of the game where there is one, else a ground lighter than the card;
/// a clear edge, and its caption (on a dark backing over the picture, so it stays readable).
fn paint_ground(ui: &Ui, scheme: &Scheme, rect: Rect, ground: &Ground) {
    let painter = ui.painter();
    let edge = Stroke::new(1.5, color(scheme.sub));
    let caption = t("s_sg_modell_spiel");
    let font = egui::TextStyle::Small.resolve(ui.style());
    match ground.texture {
        Some(texture) => {
            let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture, rect, full, egui::Color32::WHITE);
            painter.rect_stroke(rect, 4.0, edge, StrokeKind::Inside);
            let galley = painter.layout_no_wrap(caption, font, HUD_TEXT);
            let backing = Rect::from_min_size(rect.left_top() + vec2(3.0, 3.0), galley.size() + vec2(6.0, 2.0));
            painter.rect_filled(backing, 2.0, HUD.gamma_multiply(0.8));
            painter.galley(backing.min + vec2(3.0, 1.0), galley, HUD_TEXT);
        }
        None => {
            let fill = color(scheme.surface).lerp_to_gamma(egui::Color32::WHITE, 0.16);
            painter.rect(rect, 4.0, fill, edge, StrokeKind::Inside);
            painter.text(rect.left_top() + vec2(6.0, 4.0), egui::Align2::LEFT_TOP, caption, font, color(scheme.sub));
        }
    }
}

/// A dashed outline around `area`.
fn paint_dashed(ui: &Ui, area: Rect, stroke: Stroke) {
    let [a, b, c, d] = [area.left_top(), area.right_top(), area.right_bottom(), area.left_bottom()];
    ui.painter().extend(egui::Shape::dashed_line(&[a, b, c, d, a], stroke, 5.0, 4.0));
}

/// The signature box as the game shows it: a dark box with the location symbol and a number.
fn paint_pill(ui: &Ui, pill: Rect) {
    let painter = ui.painter();
    painter.rect(pill, 3.0, HUD, Stroke::new(1.0, HUD_TEXT.gamma_multiply(0.5)), StrokeKind::Inside);
    let side = (pill.height() * 0.62).clamp(8.0, 28.0);
    let symbol = Rect::from_center_size(pos2(pill.left() + pill.height() * 0.5, pill.center().y), vec2(side, side));
    if let Some(image) = icons::image("map-pin", Color::hex("#e2ecf1"), side) {
        image.paint_at(ui, symbol);
    }
    painter.text(
        pos2(symbol.right() + 3.0, pill.center().y),
        egui::Align2::LEFT_CENTER,
        signature_window::with_thousands(EXAMPLE),
        egui::FontId::proportional((pill.height() * 0.42).clamp(7.0, 16.0)),
        HUD_TEXT,
    );
}

/// Where the model of the window shows the signature: on the picture of the game at the place kept, so the
/// window stands over the real signature; without a picture at the place of the model, like the model of the
/// search frame, because the place kept is true to a wide screen and too small to see on the model.
fn shown_pill(ground: &Ground, kept: Option<[i32; 4]>) -> Option<[i32; 4]> {
    ground.texture.and(kept)
}

/// The two models one below the other in a card each, each as wide as the page: where the window stands, and the
/// area the scanner reads, the latter up to [`MODEL_WIDTH`] so that the signature in the picture is large
/// enough to frame. `place` and `frame` are title, hint and the button of each. Both stand on the picture of the
/// game where there is one.
pub fn model_cards(ui: &mut Ui, scheme: &Scheme, state: &mut ScannerState, place: [&str; 3], frame: [&str; 3]) {
    state.poll_fetch(ui.ctx());
    let ground = state.ground(ui.ctx());
    let now = ui.input(|i| i.time);
    // Above both cards: both models stand on the picture, or both lack it.
    if let Some(line) = state.ground_line(now) {
        widgets::hint(ui, scheme, &line);
    }
    // Side by side where each keeps the width of a picture area, else one under the other over the whole width.
    crate::layout::columns_or_stack_loose(ui, MODELS_SIDE_BY_SIDE, 2, |ui, i| {
        let [title, hint, button] = if i == 0 { place } else { frame };
        widgets::card(ui, scheme, |ui| {
            widgets::card_title(ui, scheme, title);
            widgets::hint(ui, scheme, hint);
            ui.add_space(4.0);
            if i == 0 {
                let pill = shown_pill(&ground, state.pill_place());
                place_model(ui, scheme, &mut state.layout, &ground, pill);
                if ui.add(widgets::Button::new(button, ButtonKind::Normal, scheme)).clicked() {
                    (state.layout.dx, state.layout.dy) = (0, 0);
                }
            } else {
                frame_model(ui, scheme, &mut state.search_frame, state.remembered, &ground);
                ui.horizontal_wrapped(|row| {
                    if row.add(widgets::Button::new(button, ButtonKind::Normal, scheme)).clicked() {
                        state.search_frame = None;
                    }
                    let show = t("s_sg_rahmen_zeigen");
                    if row.add(widgets::Button::new(&show, ButtonKind::Normal, scheme)).clicked() {
                        state.request_show(now);
                    }
                    state.poll_show(row.ctx());
                    if state.can_fetch() {
                        let fetch = t(if state.picture.is_some() { "s_sg_bild_neu" } else { "s_sg_bild_holen" });
                        let waiting = state.fetch.is_some().then(|| t("s_sg_bild_laeuft"));
                        let button = widgets::Button::new(&fetch, ButtonKind::Normal, scheme)
                            .icon("image-plus")
                            .blocked(waiting.as_deref());
                        if row.add(button).clicked() && state.fetch.is_none() {
                            state.start_fetch(row.ctx(), now);
                        }
                    }
                });
                widgets::hint(ui, scheme, &t("s_sg_rahmen_privat"));
            }
        });
    });
}

/// The narrowest a card of a model gets beside the other one: its picture area at its least width, with the margins
/// of the card.
const MODELS_SIDE_BY_SIDE: f32 = crate::widgets::PICTURE_MIN_WIDTH + 2.0 * crate::layout::GAP + 4.0;

/// The model of the screen for the search frame: the player draws the area in which the signature stands (in the
/// HUD of the ship, near the top in the middle); only this area is read. A drag inside the frame moves it, one on
/// a handle changes its size, one outside draws a new one; the frame stays inside the picture. The frame is given
/// back when the drag ends, not while it runs. The arrow keys move it while the model has the focus. Gives
/// whether the frame changed. Where no frame stands, the place kept (`remembered`, per mille) is shown dashed.
pub fn frame_model(
    ui: &mut Ui,
    scheme: &Scheme,
    frame: &mut Option<[i32; 4]>,
    remembered: Option<[i32; 4]>,
    ground: &Ground,
) -> bool {
    let (rect, response) = ground.allocate(ui, Sense::click_and_drag(), MODEL_WIDTH);
    let id = response.id;
    let mut changed = false;
    let reach = handle_reach(rect, *frame);
    let whole = |(x, y): (f32, f32)| (x.round() as i32, y.round() as i32);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    keep_arrows(ui, &response);
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        let at = model_to_mille(rect, origin);
        let hold = Hold { grip: grip_at(*frame, at, reach), start: *frame, from: whole(at), now: None };
        ui.data_mut(|d| d.insert_temp(id, hold));
    }
    let mut hold = ui.data(|d| d.get_temp::<Hold>(id));
    if response.dragged()
        && let (Some(pointer), Some(held)) = (response.interact_pointer_pos(), hold.as_mut())
    {
        if let Some(next) = drag_frame(held.start, held.grip, held.from, whole(model_to_mille(rect, pointer))) {
            held.now = Some(next);
        }
        ui.data_mut(|d| d.insert_temp(id, *held));
    }
    // Let go: the frame the drag gave is the frame now, and only now is it stored.
    if !response.dragged()
        && let Some(held) = hold.take()
    {
        ui.data_mut(|d| d.remove::<Hold>(id));
        if let Some(next) = held.now
            && *frame != Some(next)
        {
            *frame = Some(next);
            changed = true;
        }
    }
    if response.has_focus()
        && let Some(standing) = *frame
    {
        let step = |positive: Key, negative: Key| {
            ui.input(|i| i32::from(i.key_pressed(positive)) - i32::from(i.key_pressed(negative))) * FRAME_KEY_STEP
        };
        let (mx, my) = (step(Key::ArrowRight, Key::ArrowLeft), step(Key::ArrowDown, Key::ArrowUp));
        if (mx != 0 || my != 0)
            && let Some(moved) = drag_frame(Some(standing), Grip::Move, (500, 500), (500 + mx, 500 + my))
            && moved != standing
        {
            *frame = Some(moved);
            changed = true;
        }
    }

    paint_ground(ui, scheme, rect, ground);
    if ground.texture.is_none() {
        let scale = rect.width() / GAME.2 as f32;
        let (at, size) = (vec2(PILL.0 as f32, PILL.1 as f32), vec2(PILL.2 as f32, PILL.3 as f32));
        paint_pill(ui, Rect::from_min_size(rect.min + at * scale, size * scale));
    }
    let painter = ui.painter();
    let accent = BRAND;
    // The focus: a ring around the whole model, with or without a frame.
    if response.has_focus() {
        painter.rect_stroke(rect.expand(2.0), 5.0, Stroke::new(2.0, color(scheme.accent)), StrokeKind::Outside);
    }
    let hover = response.hover_pos().map(|at| grip_at(*frame, model_to_mille(rect, at), reach));
    let held = hold.map(|h| h.grip);
    match (hold.and_then(|h| h.now).or(*frame), remembered) {
        (Some(drawn), _) => {
            let area = mille_to_model(rect, drawn);
            let lit = held.is_some() || hover.is_some_and(|g| g != Grip::New);
            let (fill, width) = if lit { (0.4, 3.0) } else { (0.2, 2.5) };
            // A dark edge under the green one, so it stands out on a green picture too.
            painter.rect_stroke(area, 2.0, Stroke::new(width + 2.0, HUD), StrokeKind::Middle);
            painter.rect(area, 2.0, accent.gamma_multiply(fill), Stroke::new(width, accent), StrokeKind::Middle);
            let corners = [area.left_top(), area.right_top(), area.left_bottom(), area.right_bottom()];
            for (i, corner) in corners.into_iter().enumerate() {
                // The handle under the pointer or in the hand is larger and light.
                let active = held.or(hover) == Some(Grip::Corner(i));
                let (side, fill) = if active { (HANDLE * 1.5, HUD_TEXT) } else { (HANDLE, accent) };
                let handle = Rect::from_center_size(corner, vec2(side, side));
                painter.rect(handle, 1.0, fill, Stroke::new(1.0, HUD), StrokeKind::Inside);
            }
        }
        // Without a frame the place kept: dashed, and named.
        (None, Some(place)) => {
            let area = mille_to_model(rect, place);
            paint_dashed(ui, area, Stroke::new(2.0, accent));
            let at = pos2(area.left().max(rect.left() + 2.0), (area.top() - 3.0).max(rect.top() + 14.0));
            painter.text(
                at,
                egui::Align2::LEFT_BOTTOM,
                t("s_sg_modell_gemerkt"),
                egui::TextStyle::Small.resolve(ui.style()),
                color(scheme.fg),
            );
        }
        (None, None) => {}
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, t("s_sg_rahmen")));
    let cursor = match hold {
        Some(held) => held.grip.cursor(true),
        None => hover.unwrap_or(Grip::New).cursor(false),
    };
    response.on_hover_cursor(cursor).on_hover_text(t("s_sg_rahmen_tipp"));
    changed
}

#[cfg(test)]
mod tests;
