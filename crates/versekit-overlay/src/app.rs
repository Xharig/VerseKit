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

//! The overlay window: takes the news of the watcher thread, follows the
//! game, the settings and the pointer, places the window (corner, bar,
//! fold, strip) and draws it with `versekit_ui::overlay`.
//!
//! ⚠ The window is never hidden: eframe draws nothing while a window is
//! hidden. A waiting overlay is the window cut down to its strip.
//!
//! ⚠ Folding never changes the size of the window: the system would show
//! the last picture stretched to the new size until the next one arrives.
//! The window spans the open place and the visible part; egui lays out in
//! the visible part, the rest stays transparent, and the window region cuts
//! the window down to that part, so clicks beside it reach the game.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{ViewportCommand, WindowLevel};
use versekit_core::basetool::demand::{self, Material};
use versekit_core::contested;
use versekit_core::game_start::{self, Spawner};
use versekit_core::language::{self, Lang, Phrase};
use versekit_core::live_contracts::Line as ContractLine;
use versekit_core::overlay::{
    self, BarSide, Corner, DRAG_SLACK, Event, Hotkey, Layout, MIN_BAR_WIDTH, Message, Mode, OPEN_EXTRA_HEIGHT, Pointer,
    Rect, Remembered, Row, Rows, Screens, Section, Settings, Shown, Visibility, keys,
};
use versekit_core::overlay_control::{self, Control, Place};
use versekit_core::paths::{Host, Paths};
use versekit_core::shard::{self, Joins};
use versekit_core::window_signal::{self, Role, Signal};
use versekit_core::{font_size, jsonio::Value, location, screen, theme, writeguard};
use versekit_ui::handle_search::HandleSearch;
use versekit_ui::overlay::{self as view, Action, BAR_HEIGHT, Drag, STRIP_HEIGHT};

use crate::Args;
use crate::afk::{self, Afk};
use crate::files;
use crate::scanner::{self, LEARN_SWITCH, MAIN_WINDOW_SWITCH, Scanner};
use crate::watcher::{self, Ask, News};

#[path = "hotkey_x11.rs"]
mod hotkey_x11;
#[path = "hint_window.rs"]
mod hint_window;
#[path = "info_window.rs"]
mod info_window;
#[path = "x11.rs"]
mod x11;

/// The window functions of the system: Windows, X11; on macOS the window ones do nothing.
mod os {
    #[cfg(not(windows))]
    use versekit_core::overlay::Pointer;
    #[cfg(target_os = "macos")]
    use versekit_core::overlay::Rect;

    #[cfg(windows)]
    pub use crate::win::{
        allow_typing, beep, game_process, nudge_pointer, pointer, reassert_topmost, set_rect, set_region, window_rect,
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    pub use super::x11::{allow_typing, reassert_topmost, set_region, window_rect};

    /// The short sound of a find; `loud` for one that must not be missed.
    #[cfg(not(windows))]
    pub fn beep(loud: bool) {
        versekit_core::sound::play(loud);
    }
    #[cfg(target_os = "macos")]
    pub fn set_region(_hwnd: isize, _r: Option<Rect>, _radius: i32) -> bool {
        false
    }
    /// Nothing to do: X11 tells the window itself where the pointer is once its input shape takes it again.
    #[cfg(not(windows))]
    pub fn nudge_pointer(_hwnd: isize, _x: i32, _y: i32) {}
    #[cfg(not(windows))]
    pub fn game_process() -> Option<bool> {
        versekit_core::game_start::game_process()
    }
    /// The pointer where the system tells it (macOS, X11); never hidden, as nothing tells that.
    #[cfg(not(windows))]
    pub fn pointer() -> Option<Pointer> {
        versekit_core::screen::pointer_position().map(|(x, y)| Pointer { hidden: false, x, y })
    }
    #[cfg(target_os = "macos")]
    pub fn reassert_topmost(_hwnd: isize) {}
    #[cfg(target_os = "macos")]
    pub fn allow_typing(_hwnd: isize, _on: bool) -> bool {
        false
    }
    #[cfg(target_os = "macos")]
    pub fn window_rect(_hwnd: isize) -> Rect {
        Rect::default()
    }
}

/// The id of the bell in the bar: it always stands there, green while a newer version is on offer.
pub const BELL: &str = "bell";

/// The bell: like the other buttons and meaning the history of versions, green and naming the version while one
/// is on offer; a click opens „Update & Über“ in the main window either way.
fn bell(version: Option<&str>) -> view::Extra {
    let (meaning, tone) = match version {
        Some(version) => (language::tf("hf_hinweis_update_da", &[("0", version.into())]), view::Tone::Lit),
        None => (language::t("hinweis_versionen"), view::Tone::Normal),
    };
    view::Extra { action: Action::Extra(BELL), icon: "bell", meaning, id: BELL, tone }
}

/// How long the hiding of a contract can be taken back.
pub const UNHIDE_SECONDS: f64 = 10.0;

/// A contract the player hid by hand, kept for taking it back.
#[derive(Clone, Debug)]
pub struct HiddenNote {
    pub title: String,
    /// The line it had, to put back where no watcher reads the log (the preview with sample contents).
    pub line: Option<ContractLine>,
    /// When it was hidden (the clock of the window, in seconds).
    pub at: f64,
}

/// How long a started main program has to mark its window.
const MAIN_STARTING: Duration = Duration::from_secs(10);
/// How long a page asked for waits for the main window that is being started.
const PAGE_WAIT: Duration = Duration::from_secs(60);
/// How often the game and the location are looked at.
const GAME_EVERY: Duration = Duration::from_secs(2);
const LOCATION_EVERY: Duration = Duration::from_secs(10);
/// How often the open overlay reads the refinery jobs again (the time left counts on in between).
const REFINERY_READ_EVERY: Duration = Duration::from_secs(15);
/// A frame at least this often: pointer, click-through and the timers.
const TICK: Duration = Duration::from_millis(120);
/// The place is written once the window stood still this long.
const PLACE_SETTLE: Duration = Duration::from_millis(750);
/// How long the window region keeps a part the overlay gave up: the system
/// may still show the picture painted before for a few frames.
pub const REGION_SETTLE: Duration = Duration::from_millis(150);
/// How long a place sent to the window system may take to arrive: until then the window standing where it
/// was is no move of the system.
const PLACE_ARRIVES: Duration = Duration::from_secs(1);

/// What the game watch tells the window.
pub enum GameNews {
    Running(bool),
    Location(Option<Phrase>),
    /// Writes the lock refused in that thread.
    Denied(Vec<String>),
}

/// The screens as the system reports them, in pixels.
pub fn screens_now() -> (Rect, Rect) {
    let fallback = Rect::new(0, 0, 1920, 1080);
    let primary = screen::primary_area(false).map_or(fallback, Rect::from_area);
    let work = screen::primary_area(true).map_or(primary, Rect::from_area);
    (primary, work)
}

pub fn work_at(x: i32, y: i32) -> Rect {
    screen::monitor_area(x, y, true).map_or_else(|| screens_now().1, Rect::from_area)
}

/// Everything the screens cover together, in pixels; `None` where they cannot be asked.
fn all_screens() -> Option<Rect> {
    screen::screens().into_iter().map(Rect::from_area).reduce(|a, b| a.union(b))
}

fn screens_with<R>(f: impl FnOnce(&Screens) -> R) -> R {
    let (primary, primary_work) = screens_now();
    let on = |x: i32, y: i32, w: i32| screen::on_a_screen(x, y, w);
    let at = |x: i32, y: i32| work_at(x, y);
    f(&Screens { primary, primary_work, on_a_screen: &on, work_at: &at })
}

/// Where the overlay starts, in pixels: the remembered place if the strip
/// of its bar lies on a screen, otherwise the middle of the main screen with
/// the remembered size, or with `default_size` when nothing is remembered.
pub fn start_rect(remembered: Option<Remembered>, default_size: (i32, i32), bar: BarSide, scale: f32) -> Rect {
    screens_with(|s| overlay::start_place(remembered.map(|r| r.to_pixels(scale)), default_size, bar, s))
}

/// A window of `size` pixels in the middle of the main screen, on its work area.
pub fn centered_on_main(size: (i32, i32)) -> Rect {
    screens_with(|s| overlay::centered_on_main(size, s))
}

/// The default size before a frame was measured, in pixels: six rows of
/// the usual text heights at `points` and the overlay's size `percent` (corrected in the first frame).
fn estimated_default(points: i64, percent: i64, scale: f32) -> (i32, i32) {
    let zoom = versekit_ui::window_zoom(points, percent) * scale;
    ((overlay::DEFAULT_WIDTH as f32 * zoom).round() as i32, (272.0 * zoom).round() as i32)
}

/// What the command line and the control file set over the settings: mode, seconds, corner, bar.
fn apply_overrides(settings: &mut Settings, args: &Args) {
    if let Some(mode) = &args.mode {
        settings.mode = Mode::from_setting(Some(mode));
    }
    if let Some(seconds) = args.seconds {
        settings.popup_seconds = seconds.clamp(2, 60);
    }
    if args.corner.is_some() || args.bar.is_some() {
        let corner = args.corner.as_deref().or(Some(settings.layout.corner.setting()));
        let bar = args.bar.as_deref().or(Some(settings.layout.bar.setting()));
        settings.layout = Layout { corner: Corner::from_setting(corner), bar: BarSide::from_settings(bar, corner) };
    }
}

/// Whether the game counts as running whatever the system says: `--spiel` or the control file.
/// Shared with the game watch, which decides on the location with it.
#[derive(Debug, Default)]
pub struct ForcedGame(AtomicU8);

impl ForcedGame {
    const OFF: u8 = 1;
    const ON: u8 = 2;

    pub fn new(forced: Option<bool>) -> ForcedGame {
        let game = ForcedGame::default();
        game.set(forced);
        game
    }

    pub fn get(&self) -> Option<bool> {
        match self.0.load(Ordering::Relaxed) {
            Self::OFF => Some(false),
            Self::ON => Some(true),
            _ => None,
        }
    }

    pub fn set(&self, forced: Option<bool>) {
        let value = match forced {
            Some(false) => Self::OFF,
            Some(true) => Self::ON,
            None => 0,
        };
        self.0.store(value, Ordering::Relaxed);
    }
}

pub struct OverlayApp {
    pub hwnd: isize,
    pub paths: Option<Paths>,
    pub settings: Settings,
    /// The settings as the file says, before the command line and the control file change them.
    file_settings: Settings,
    /// The game state forced from outside, and what the game watch found last.
    forced_game: Arc<ForcedGame>,
    game_real: bool,
    /// The live switches: what the file watch sends, the file it watches, and the state taken over last.
    control: Option<Receiver<Control>>,
    control_path: Option<String>,
    control_taken: Option<Control>,
    /// The file thread; `None` with sample contents.
    pub files: Option<files::Files>,
    /// Under `--gegenprobe im_bildfaden`: the last read here and the writes done here.
    inline_read: Instant,
    inline_writes: u64,
    args: Args,
    pub vis: Visibility,
    /// Until then (seconds of the run) the line of the scanner shows in the open overlay, also where the
    /// player did not ask for it: a reading opened the overlay, and it should show why.
    scanner_line_until: f64,
    /// The materials the unit wants, as the main window last handed them over; in memory only.
    demand: Arc<Mutex<Vec<Material>>>,
    pub rows: Rows,
    /// The running contracts, as the watcher tells them.
    pub contracts: Vec<ContractLine>,
    /// Messages for the system, not yet shown: (title, text).
    pub notices: Vec<(String, String)>,
    /// The icon next to the clock; under `--probe` its shell is a recorder.
    #[cfg(windows)]
    pub tray: Option<crate::tray::Tray>,
    #[cfg(windows)]
    pub tray_log: Option<Arc<crate::tray::Recorder>>,
    /// Under `--probe`: the addresses the menu would have opened.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub opened: Vec<String>,
    pub status: Message,
    location: Option<Phrase>,
    pub update: Option<String>,
    news: Receiver<News>,
    asks: Option<Sender<Ask>>,
    game_news: Receiver<GameNews>,
    hotkey: Arc<Mutex<Option<Hotkey>>>,
    hotkey_presses: Receiver<()>,
    refinery: crate::refinery::RefineryKey,
    /// The hotkey of the handle search in the info bar, read at every key, and its presses.
    search_hotkey: Arc<Mutex<Option<Hotkey>>>,
    search_presses: Receiver<()>,
    /// The bridge to scmdb.net (`apply_scmdb`).
    scmdb: versekit_core::scmdb_bridge::Bridge,
    started: Instant,
    /// The visible part of the window now and the place it has when open,
    /// in screen pixels.
    pub current: Rect,
    pub last_open: Rect,
    /// The window itself, in screen pixels: around the visible part and the
    /// place the overlay opens at.
    pub canvas: Rect,
    /// The window place asked for in this frame, not yet sent to the window system (elsewhere than Windows,
    /// and on Windows without a window handle).
    window_to: Option<Rect>,
    /// When a window place was sent to the window system last, while it may not have arrived yet.
    place_sent: Option<Instant>,
    /// The hint that the window region was refused is in the rows already.
    #[cfg(all(unix, not(target_os = "macos")))]
    region_refused_told: bool,
    /// Outside Windows the window itself is hidden while the overlay waits without the game.
    #[cfg(not(windows))]
    os_hidden: bool,
    /// The window region, in window pixels, and the rounding of its corners in pixels.
    pub region: Option<Rect>,
    region_rounding: i32,
    /// The visible part painted in the last frame, in window pixels.
    painted: Rect,
    /// When the region shrinks to the visible part again.
    shrink_at: Option<Instant>,
    /// For which state the window was placed last.
    placed_for: Option<(Shown, Layout)>,
    drag: Option<(Drag, (i32, i32), Rect)>,
    pub passing: bool,
    was_locked: bool,
    last_topmost: Instant,
    place_changed: Option<Instant>,
    /// The remembered place exists; its removal resets the place.
    place_known: bool,
    /// No size was remembered: the first frame measures the default size.
    pub default_pending: bool,
    /// What the bar and the strip need, from the last frame (points).
    min_width: f32,
    strip: egui::Vec2,
    pub lock: Option<egui::Rect>,
    pub drawn: Vec<String>,
    pub visible_rows: usize,
    /// What the bar of the contracts drew in the last frame, and where its ban buttons are.
    pub contract_texts: Vec<String>,
    pub hide_buttons: Vec<egui::Rect>,
    /// The contract hidden last, kept for a few seconds so the player can take it back: its line
    /// (to put back when there is no watcher) and when it was hidden.
    pub hidden_note: Option<HiddenNote>,
    /// The refinery jobs kept in the data, read every [`REFINERY_READ_EVERY`], and when.
    pub refinery_jobs: Vec<versekit_core::jsonio::Map>,
    refinery_read: Option<Instant>,
    /// Where the button that takes the hiding back was drawn in the last frame.
    pub unhide_button: Option<egui::Rect>,
    /// A way to start the game is known (the watcher says so): the rocket and the entry of the icon's menu.
    pub launcher: bool,
    /// The game already ran when the overlay started: the start programs for that are started once (4.12).
    game_programs_checked: bool,
    /// What starts the game: the real start, and under `--probe` a recorder that starts nothing.
    spawner: Arc<dyn Spawner>,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub spawn_log: Option<Arc<game_start::Recorder>>,
    /// How a start went (the lookup of the way is done in a thread of its own, not in the frame).
    launch_tx: Sender<Result<(), String>>,
    launch_rx: Receiver<Result<(), String>>,
    scale: f32,
    points: i64,
    /// The language the window title was set in.
    title_lang: Lang,
    /// Writes the lock refused in the other threads.
    pub denied: Vec<String>,
    /// The `Data.p4k` the thread of the finds reads, as it said.
    pub archive_read: Option<String>,
    pub probe: Option<probe_type::Probe>,
    /// Counter check: after coming back from the strip the rows stay empty.
    pub counter: Option<String>,
    was_waiting: bool,
    /// This frame's view is laid out.
    laid_out: bool,
    /// The buttons of the bar in the last frame, the extras behind "⋯",
    /// the "⋯" button and where the title ends.
    pub buttons: Vec<(Action, egui::Rect)>,
    pub in_menu: Vec<Action>,
    pub more: Option<egui::Rect>,
    pub title_end: f32,
    /// The hint of the bar shown in the last frame.
    pub hint: Option<String>,
    /// Buttons besides the fixed ones of the bar.
    pub extras: Vec<view::Extra>,
    /// The extras were set from outside (the self-check's placeholders): the frames leave them as they are.
    pub extras_kept: bool,
    /// Anti-AFK: the button of the bar and the beat.
    afk: Afk,
    /// The signature scanner: the watch, the window with the material and the two buttons of the bar.
    scanner: Scanner,
    /// The joins of a shard in the running session, as the watcher tells them.
    shard: Joins,
    /// The info bar: a window of its own with the shard and the scanned material.
    pub info: info_window::InfoWindow,
    /// The hint of a button of the bar, in a window of its own next to the pointer.
    hint_window: hint_window::HintWindow,
    /// The search of handles in the middle of the info bar, with its log monitor.
    search: HandleSearch,
    /// The frame rates read from the picture of the game, while the player switched them on.
    fps: crate::fps::FpsLine,
    /// The start of the hangar cycle, read with the settings while the line of the hangar is on.
    hangar_start: Option<i64>,
    /// Under `--probe`: the main program that would have been started, and its arguments.
    pub launched: Option<(std::path::PathBuf, Vec<String>)>,
    /// Under `--probe`: the overlay was closed (the window stays for the checks).
    pub quit_asked: bool,
    /// The closing of the overlay was done.
    closed: bool,
    /// The main program this overlay started.
    main_child: Option<std::process::Child>,
    /// The small window for teaching the digits, while it runs.
    learn_child: Option<std::process::Child>,
    /// When the main program was started.
    started_main: Option<Instant>,
    /// A page the icon's menu asked for while the main window was not there yet: handed on once it is.
    pending_page: Option<(Signal, Instant)>,
    /// The main window said it changed the settings.
    pub settings_told: Arc<AtomicBool>,
    /// A second start asked for the main window.
    pub overview_asked: Arc<AtomicBool>,
    /// The main window asked to show the area the signature scanner reads.
    pub frame_asked: Arc<AtomicBool>,
    /// Under `--probe`: the pointer as the self-check places it, the input
    /// it hands to egui in the next frame, and the pointer moves the window
    /// was told of (window pixels).
    pub pointer_override: Option<Pointer>,
    pub injected: Vec<egui::Event>,
    pub nudged: Vec<(i32, i32)>,
}

/// The probe of Windows and the one of X11; on macOS a placeholder type.
pub mod probe_type {
    #[cfg(windows)]
    pub use crate::probe::Probe;
    #[cfg(all(unix, not(target_os = "macos")))]
    pub use crate::probe_x11::Probe;
    #[cfg(target_os = "macos")]
    pub struct Probe;
}

impl OverlayApp {
    /// What the command line asked for.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn args(&self) -> &Args {
        &self.args
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    fn popup_seconds(&self) -> f64 {
        self.settings.popup_seconds as f64
    }

    /// Writes a setting to the copy, or only remembers it with sample
    /// contents; the main window is told once it is written, so it shows
    /// the change.
    pub fn store(&mut self, key: &str, value: Value) {
        self.file_job(files::Job::Setting(key.to_string(), value));
    }

    fn sample_rows() -> Rows {
        let mut rows = Rows::default();
        let samples = [
            ("Ultra-Fine", "Kühler", "Militär · S1 · A", "21:14:03"),
            ("Morningstar Helm", "Helm", "", "21:02:47"),
            ("P8-AR Gewehr", "Handfeuerwaffe", "S3", "20:51:10"),
        ];
        for (name, kind, code, time) in samples.iter().rev() {
            let row = Row::Found {
                name: name.to_string(),
                kind: kind.to_string(),
                code: code.to_string(),
                time: time.to_string(),
                caught_up: false,
            };
            rows.push(row, 20);
        }
        rows
    }

    /// The visible part goes to `target`. The window follows at the end of
    /// this frame, and only where the part leaves it (see [`Self::canvas_needed`]).
    fn resize_window(&mut self, target: Rect) {
        self.current = target;
        // Counter check: the window gets its rect before the frame is laid out.
        if self.counter.as_deref() == Some("frueh") {
            self.apply_canvas(self.canvas.union(target));
        }
    }

    /// Where the overlay stands once it opens from `shown`, as [`Self::place`]
    /// puts it, in screen pixels.
    fn open_place(&self, ctx: &egui::Context, shown: Shown) -> Rect {
        if shown == Shown::Open {
            return self.current;
        }
        let size = self.size_for(ctx, Shown::Open);
        let from = overlay::placed_from(Some(shown), self.current, self.last_open);
        overlay::place(self.settings.layout, size, from, work_at(from.x, from.y))
    }

    /// The window this frame needs: around the visible part and the place
    /// the overlay opens at, so folding, waiting and coming back keep its
    /// size. While it is dragged it moves along as it is.
    fn canvas_needed(&self, ctx: &egui::Context, shown: Shown) -> Rect {
        if matches!(self.drag, Some((Drag::Move, ..))) {
            return self.canvas;
        }
        // Counter check: the window as large as its visible part, resized on every fold.
        if self.counter.as_deref() == Some("groesse") {
            return self.current;
        }
        // Resized at the grip: the window already spans everything the drag can reach, so
        // that only the region follows the pointer, not the size of the window.
        if let Some((Drag::Resize, _, start)) = self.drag {
            return self.canvas.union(self.resize_reach(ctx, start));
        }
        self.open_place(ctx, shown).union(self.current)
    }

    /// Everything the window can cover while the grip drags it from `start`.
    fn resize_reach(&self, ctx: &egui::Context, start: Rect) -> Rect {
        let min_w = self.px(ctx, self.min_width).max(MIN_BAR_WIDTH);
        let work = work_at(start.x, start.y);
        let at = |x: i32, y: i32| overlay::resized(self.settings.layout, start, (x, y), min_w, Some(work));
        at(work.x, work.y).union(at(work.right(), work.bottom()))
    }

    /// Puts the window at `r` (pixels). Windows moves it at once; elsewhere, and on Windows without a
    /// window handle, the move is sent to the window system at the end of the frame, in the points of the
    /// window functions.
    fn set_window(&mut self, r: Rect) {
        #[cfg(windows)]
        if self.hwnd != 0 {
            os::set_rect(self.hwnd, r);
            return;
        }
        self.window_to = Some(r);
    }

    /// Sends the last asked-for window place (see [`Self::set_window`]) to the window system.
    fn flush_window(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.window_to.take() {
            // The window functions take points of egui, which are pixels over the native
            // scale times the zoom: the pixels of the overlay are points times `pixels_per_point`.
            for command in window_commands(r, ctx.pixels_per_point()) {
                ctx.send_viewport_cmd(command);
            }
            self.place_sent = Some(Instant::now());
        }
    }

    /// The system moved the window: the visible part and the open place move along. A place sent to the
    /// window system that did not arrive yet is no such move (see [`system_move`]).
    fn follow_system_move(&mut self) {
        let real = os::window_rect(self.hwnd);
        if (real.x, real.y) == (self.canvas.x, self.canvas.y) {
            self.place_sent = None;
        }
        let Some((dx, dy)) = system_move(real, self.canvas, self.place_sent.map(|at| at.elapsed())) else { return };
        self.place_sent = None;
        for r in [&mut self.current, &mut self.last_open, &mut self.canvas] {
            r.x += dx;
            r.y += dy;
        }
    }

    /// Moves or sizes the window. A new size comes right before painting,
    /// the frame laid out for it already: the system shows the old picture
    /// stretched until this one arrives.
    fn apply_canvas(&mut self, target: Rect) {
        let before = self.canvas;
        self.canvas = target;
        self.set_window(target);
        let before_layout = !self.laid_out;
        #[cfg(windows)]
        if let Some(probe) = self.probe.as_mut() {
            probe.resized(before, target, before_layout);
        }
        let _ = (before, before_layout);
    }

    /// The window region follows the visible part. A part that grows is in
    /// it before its frame is painted; a part given up stays in it for
    /// [`REGION_SETTLE`], while the system may still show the picture painted
    /// before. A window that moved or got a new size takes the part at once,
    /// and so does one that is resized at the grip: its edge follows the pointer.
    fn follow_region(&mut self, ctx: &egui::Context, canvas_changed: bool) {
        // Waiting hidden: an empty region, so nothing of the window is drawn and it takes no clicks.
        if self.waits_hidden() {
            // Outside Windows an empty region leaves a dead window in the task bar: the window goes away there.
            #[cfg(not(windows))]
            if !std::mem::replace(&mut self.os_hidden, true) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            if self.region != Some(Rect::default()) && os::set_region(self.hwnd, Some(Rect::default()), 0) {
                self.region = Some(Rect::default());
            }
            self.painted = Rect::default();
            return;
        }
        #[cfg(not(windows))]
        if std::mem::replace(&mut self.os_hidden, false) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        // Back from waiting hidden: the region takes the visible part at once, as for a new window.
        let canvas_changed = canvas_changed || self.region == Some(Rect::default());
        let visible = self.current.relative_to(self.canvas.x, self.canvas.y);
        let Some(wanted) = self.region_wanted(ctx, canvas_changed, visible) else { return };
        let radius = self.region_radius(ctx);
        if self.region == Some(wanted) && self.region_rounding == radius {
            return;
        }
        let whole = Rect::new(0, 0, self.canvas.w, self.canvas.h);
        // Counter check: the open overlay without a region, its corners left to the alpha alone.
        let shape = if self.counter.as_deref() == Some("offen_ohne_bereich") && wanted.holds(whole) {
            None
        } else {
            Some(wanted)
        };
        if !os::set_region(self.hwnd, shape, radius) {
            // Refused: the next frame tries again as for a new part.
            self.painted = Rect::default();
            // Under X11 without a region the window takes the clicks beside the visible part: said once.
            #[cfg(all(unix, not(target_os = "macos")))]
            if self.hwnd != 0 && !std::mem::replace(&mut self.region_refused_told, true) {
                let hint = Row::Hint { text: Phrase::new("ov_durchklick_geht_nicht").into(), contract: None };
                self.rows.push(hint, self.settings.rows);
            }
            return;
        }
        self.region = Some(wanted);
        self.region_rounding = radius;
        #[cfg(windows)]
        if let Some(probe) = self.probe.as_mut() {
            probe.region_set(wanted);
        }
    }

    /// The part the region holds for the `visible` one of this frame; `None`
    /// where it is left as it is.
    fn region_wanted(&mut self, ctx: &egui::Context, canvas_changed: bool, visible: Rect) -> Option<Rect> {
        let counter = self.counter.as_deref();
        // Counter checks: no region at all, or the whole window as before.
        if matches!(counter, Some("ohne_region" | "groesse")) {
            self.painted = visible;
            return None;
        }
        let now = Instant::now();
        let resizing = matches!(self.drag, Some((Drag::Resize, ..)));
        let wanted = if canvas_changed || resizing {
            self.shrink_at = None;
            visible
        } else if visible != self.painted {
            match counter {
                // Counter check: the region takes the new part one frame late.
                Some("spaet_gross") => {
                    self.shrink_at = Some(now);
                    self.region.unwrap_or(visible)
                }
                _ => {
                    // Counter check: the part given up goes with the next frame.
                    let settle = if counter == Some("sofort_klein") { Duration::ZERO } else { REGION_SETTLE };
                    self.shrink_at = Some(now + settle);
                    ctx.request_repaint_after(settle);
                    self.region.unwrap_or(self.painted).union(visible)
                }
            }
        } else if self.shrink_at.is_some_and(|t| now >= t) {
            self.shrink_at = None;
            visible
        } else {
            self.region.unwrap_or(visible)
        };
        self.painted = visible;
        Some(wanted)
    }

    /// The rounding of the window region in pixels: the corners of the painted box (none in square schemes).
    fn region_radius(&self, ctx: &egui::Context) -> i32 {
        // Counter check: the region keeps square corners.
        if self.counter.as_deref() == Some("eckig") {
            return 0;
        }
        let points = versekit_ui::box_radius(theme::current()).nw;
        (f32::from(points) * ctx.pixels_per_point()).round() as i32
    }

    /// Where this frame draws, in points: the visible part of `canvas`.
    /// egui lays out in the part known when the frame began (`began`);
    /// `None` when that is it already.
    fn drawn_rect(&self, ctx: &egui::Context, began: egui::Rect, canvas: Rect) -> Option<egui::Rect> {
        if self.current.w <= 0 || self.current.h <= 0 || self.counter.as_deref() == Some("altes_mass") {
            return None;
        }
        let ppp = ctx.pixels_per_point();
        let rect = to_points(self.current.relative_to(canvas.x, canvas.y), ppp);
        let off = (began.min - rect.min).abs().max_elem().max((began.size() - rect.size()).abs().max_elem());
        (off * ppp > 0.5).then_some(rect)
    }

    /// Sets the open place and moves the window there; it is remembered at once.
    fn move_open(&mut self, target: Rect) {
        self.last_open = target;
        self.resize_window(target);
        self.placed_for = None;
        self.place_changed = Some(Instant::now() - PLACE_SETTLE);
    }

    /// The first frame without a remembered size: the default size from the
    /// text heights in use, in the middle of the main screen.
    fn measure_default(&mut self, ui: &egui::Ui) {
        if !self.default_pending {
            return;
        }
        self.default_pending = false;
        let size = view::default_size(ui) * ui.ctx().pixels_per_point();
        self.move_open(centered_on_main((size.x.round() as i32, size.y.round() as i32)));
        // Nothing was remembered: the default stays unremembered until the user moves it.
        self.place_changed = None;
    }

    /// A new corner or bar side: the open place moves with it, so the strip
    /// and the next opening stand in the new corner.
    pub fn layout_changed(&mut self) {
        let work = work_at(self.last_open.x, self.last_open.y);
        self.last_open =
            overlay::place(self.settings.layout, (self.last_open.w, self.last_open.h), self.last_open, work);
        self.placed_for = None;
    }

    /// The zoom of the context: the text size of the main window times the overlay's size.
    fn apply_zoom(&self, ctx: &egui::Context) {
        versekit_ui::apply_window_size(ctx, self.points, self.file_settings.size_percent);
    }

    /// The overlay got another size: the open place grows or shrinks with it in its corner and stays wholly on the
    /// work area.
    fn size_changed(&mut self, before: i64, now: i64) {
        let ratio = now as f32 / before.max(1) as f32;
        let open = self.last_open;
        let work = work_at(open.x, open.y);
        let grown = ((open.w as f32 * ratio).round() as i32, (open.h as f32 * ratio).round() as i32);
        let (w, h) = overlay::limit_size(grown.0, grown.1, work);
        let r = overlay::place(self.settings.layout, (w, h), open, work);
        let x = r.x.min(work.right() - r.w).max(work.x);
        let y = r.y.min(work.bottom() - r.h).max(work.y);
        self.move_open(Rect { x, y, ..r });
    }

    /// A file job: in the file thread, or at once here under the counter
    /// check `im_bildfaden`. Returns a snapshot read here at once.
    fn file_job(&mut self, job: files::Job) -> Option<files::Snapshot> {
        if self.counter.as_deref() == Some("im_bildfaden") {
            let paths = self.paths.as_ref()?;
            return files::run(paths, job, &mut self.inline_writes);
        }
        let files = self.files.as_mut()?;
        match job {
            files::Job::Read { .. } => files.ask(job),
            _ => files.write(job),
        }
        None
    }

    /// Takes over what the settings say: the file thread reads them once a
    /// second (a file edited by hand) and at once when the main window said
    /// it changed them, past any cached stamp.
    fn refresh_settings(&mut self, ctx: &egui::Context) {
        let told = self.settings_told.swap(false, Ordering::Relaxed) && self.counter.as_deref() != Some("einstellung");
        let snapshot = if self.counter.as_deref() == Some("im_bildfaden") {
            if !told && !files::read_due(self.inline_read) {
                return;
            }
            self.inline_read = Instant::now();
            self.file_job(files::Job::Read { fresh: told })
        } else {
            if told {
                self.file_job(files::Job::Read { fresh: true });
            }
            self.files.as_ref().and_then(files::Files::take)
        };
        let Some(snapshot) = snapshot else { return };
        let Some(paths) = &self.paths else { return };
        if let Some(recovered) = snapshot.recovered.filter(|_| self.counter.as_deref() != Some("ohne_einst_hinweis")) {
            self.rows.push(
                Row::Hint { text: watcher::recovered_phrase(recovered).into(), contract: None },
                self.settings.rows,
            );
        }
        self.file_settings = snapshot.settings;
        // The search reads its settings and relations anew only when the main window changed something.
        if told {
            self.search.reload_settings();
        }
        self.hangar_start = snapshot.hangar_start;
        follow_streamer_mode(paths);
        let data = snapshot.data;
        if data.iter().any(|(k, _)| k == theme::SETTING) && theme::set_current(theme::choice_of(&data).id) {
            versekit_ui::apply(ctx, theme::current());
        }
        language::set_language(language::choice_of(&data));
        self.points = font_size::font_points(&data, &paths.host);
        self.apply_zoom(ctx);
        // „In die Mitte holen“ in the settings keeps only the size: back to the
        // middle of the main screen with it. A removed file means the default size.
        let remembered = snapshot.remembered;
        let place_known = remembered.is_some();
        if let Some(Remembered { w, h, place: None }) = remembered {
            let size = Remembered { w, h, place: None }.to_pixels(self.scale);
            self.move_open(centered_on_main((size.w, size.h)));
        } else if self.place_known && !place_known {
            self.default_pending = true;
        }
        self.place_known = place_known;
        self.adopt_settings();
    }

    /// The settings of the file with the command line and the control file over them, taken over
    /// where they differ from the ones in use.
    fn adopt_settings(&mut self) {
        let mut fresh = self.file_settings.clone();
        apply_overrides(&mut fresh, &self.args);
        self.adopt(fresh);
    }

    /// Takes over `fresh` where it differs from the settings in use.
    fn adopt(&mut self, fresh: Settings) {
        if fresh != self.settings {
            let layout_changed = fresh.layout != self.settings.layout;
            self.vis.set_mode(fresh.mode);
            self.rows.trim(fresh.rows);
            *self.hotkey.lock().unwrap_or_else(|e| e.into_inner()) = fresh.hotkey;
            *self.refinery.hotkey.lock().unwrap_or_else(|e| e.into_inner()) = fresh.scan_hotkey;
            *self.search_hotkey.lock().unwrap_or_else(|e| e.into_inner()) = fresh.search_hotkey;
            let scmdb_changed = fresh.scmdb != self.settings.scmdb;
            let sizes = (self.settings.size_percent, fresh.size_percent);
            self.settings = fresh;
            if layout_changed {
                self.layout_changed();
            }
            if sizes.0 != sizes.1 {
                self.size_changed(sizes.0, sizes.1);
            }
            if scmdb_changed {
                self.apply_scmdb();
            }
        }
    }

    /// The bridge to scmdb.net as the switch says (`scmdb_bridge.apply_setting`): only on own data, never in the
    /// self-check. A port in use says so once in a hint row.
    pub fn apply_scmdb(&mut self) {
        let on = self.settings.scmdb && self.args.probe.is_none();
        let Some(paths) = self.paths.as_ref().filter(|_| on) else {
            self.scmdb.stop();
            return;
        };
        let setup = versekit_core::scmdb_bridge::Setup {
            host: paths.host.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            namer: Box::new(|paths: &Paths| {
                let catalog = versekit_gamedata::list::list_catalog_offline(paths);
                let known = versekit_core::collection::Known::new(
                    catalog.iter().flat_map(|c| c.blueprints.iter()).map(|b| (b.key.clone(), Some(b.name.clone()))),
                );
                versekit_core::scmdb_bridge::namer(paths, known)
            }),
            poll: versekit_core::scmdb_bridge::POLL,
        };
        if !self.scmdb.start(setup) {
            self.rows
                .push(Row::Hint { text: Phrase::new("s_scmdb_belegt").into(), contract: None }, self.settings.rows);
        }
    }

    /// Takes over what the control file asks for. The state asked for is the whole file: a key that
    /// is gone means "as the data say" again. The place is taken only when it is a new request.
    pub fn apply_control(&mut self, control: Control) {
        if self.control_taken.as_ref() == Some(&control) {
            return;
        }
        self.forced_game.set(control.game);
        self.vis.set_game(control.game.unwrap_or(self.game_real));
        self.update = control.update.clone();
        self.args.corner = control.corner.clone();
        self.args.bar = control.bar.clone();
        self.args.mode = control.mode.clone();
        self.args.seconds = control.seconds;
        // Only what the file can force is set again; a change made here and not yet read back from
        // the file (the lock, a corner freed by dragging) stays as it is.
        let mut fresh = self.settings.clone();
        fresh.mode = self.file_settings.mode;
        fresh.popup_seconds = self.file_settings.popup_seconds;
        fresh.layout = self.file_settings.layout;
        apply_overrides(&mut fresh, &self.args);
        self.adopt(fresh);
        let new_request = self
            .control_taken
            .as_ref()
            .is_none_or(|taken| taken.place_nr != control.place_nr || taken.place != control.place);
        if new_request && let Some(place) = control.place {
            self.go_to(place);
        }
        self.control_taken = Some(control);
    }

    /// Moves the overlay as at the start: to the remembered place, or to the given one, which
    /// finds its way back onto a screen when none shows it.
    fn go_to(&mut self, place: Place) {
        let remembered = match place {
            Place::Remembered => self.paths.as_ref().and_then(overlay::load_remembered),
            Place::Given((w, h, place)) => Some(Remembered { w, h, place }),
        };
        let default = estimated_default(self.points, self.settings.size_percent, self.scale);
        let target = start_rect(remembered, default, self.settings.layout.bar, self.scale);
        self.move_open(target);
    }

    /// The newest request of the control file, if one came since the last look.
    fn take_control(&mut self) {
        let newest = self.control.as_ref().and_then(|rx| rx.try_iter().last());
        if let Some(control) = newest {
            self.apply_control(control);
        }
    }

    /// Reads the control file here and now: a find must be judged by the switches that were
    /// written before the Game.log line that caused it.
    fn read_control_now(&mut self) {
        if let Some(control) = self.control_path.as_deref().and_then(overlay_control::read) {
            self.apply_control(control);
        }
    }

    /// An event that may open the overlay for a while: it does where the player chose it, as the settings
    /// stand now.
    pub(crate) fn opens_for(&mut self, event: Event, now: f64) {
        if !self.settings.events.contains(event) {
            return;
        }
        self.vis.found(now, self.popup_seconds());
        if event == Event::Scanner {
            self.scanner_line_until = now + self.popup_seconds();
        }
    }

    fn take_news(&mut self) {
        let now = self.now();
        // A settings file that broke here, in the window's own reading, is told like the one the watcher finds.
        if let Some(hint) = self.paths.as_ref().and_then(watcher::recovered_hint)
            && self.counter.as_deref() != Some("ohne_einst_hinweis")
        {
            self.rows.push(Row::Hint { text: hint.into(), contract: None }, self.settings.rows);
        }
        while let Ok(news) = self.news.try_recv() {
            match news {
                News::Status(m) => self.status = m,
                // Counter checks: a kind of news is not taken over.
                News::Row(Row::Hint { text: Message::Phrase(p), .. })
                    if self.counter.as_deref() == Some("ohne_merkliste") && p.key == "ov_a_merk" => {}
                News::Row(Row::Hint { text: Message::Phrase(p), .. })
                    if self.counter.as_deref() == Some("ohne_raffinerie") && p.key == "s_rf_fertig_hinweis" => {}
                News::Row(Row::Hint { text: Message::Phrase(p), .. })
                    if self.counter.as_deref() == Some("ohne_einst_hinweis")
                        && matches!(p.key.as_str(), "ov_a_einst_bak" | "ov_a_einst_neu") => {}
                News::Catalog { .. } if self.counter.as_deref() == Some("ohne_katalog_wache") => {}
                // ⚠ A hint of a contract that stands in the bar says the same again right below it: the bar wins.
                News::Row(Row::Hint { contract: Some(title), .. })
                    if self.contracts.iter().any(|c| c.title == title) => {}
                News::Row(row) => self.rows.push(row, self.settings.rows),
                // Counter check: an empty state is not taken over, so what ended stays on screen.
                News::Contracts(lines) if lines.is_empty() && self.counter.as_deref() == Some("stand_haengt") => {}
                News::Contracts(lines) => self.contracts = lines,
                News::ContractGone(title) => self.rows.remove_contract(&title),
                News::Catalog { row, loud } => {
                    self.rows.push(row, self.settings.rows);
                    // No opening and no flash: it is news from the catalog, not a find of the player.
                    if self.settings.sound {
                        os::beep(loud);
                    }
                }
                News::System(title, text) => self.notices.push((title.to_string(), text.to_string())),
                News::Found => {
                    self.read_control_now();
                    self.opens_for(Event::Blueprint, now);
                    if self.settings.sound {
                        os::beep(false);
                    }
                }
                // No sound: the blueprint has its own, the others open quietly.
                News::Opens(event) => {
                    self.read_control_now();
                    self.opens_for(event, now);
                }
                News::Denied(list) => self.denied.extend(list),
                News::Archive(file) => self.archive_read = file,
                // Counter check: the way found is not taken over, no rocket and no entry.
                News::Starter(_) if self.counter.as_deref() == Some("starter_vergessen") => {}
                News::Starter(known) => self.launcher = known,
                News::Shard(joins) => self.shard = joins,
                News::LogLines(lines) => {
                    self.search.take_log(&lines.join("\n"), versekit_ui::handle_search::now());
                }
                News::OwnHandle(handle) => self.search.set_own_handle(Some(handle)),
            }
        }
        // How a start went: a reason is shown and noted, as in 4.12.
        while let Ok(result) = self.launch_rx.try_recv() {
            if let Err(reason) = result {
                self.status = Phrase::new("s_sp_start_nein").with("0", reason.as_str()).into();
                versekit_core::errors::record("overlay.spiel_starten", &reason);
            }
        }
        while let Ok(news) = self.game_news.try_recv() {
            match news {
                GameNews::Running(real) => {
                    // The game ended: the contracts that stayed silent go with it, and the start programs that
                    // asked for it. The game started: the start programs for that.
                    if self.game_real && !real {
                        self.ask(Ask::GameEnded);
                        self.game_programs(false);
                    }
                    if !self.game_real && real {
                        self.game_programs(true);
                    }
                    self.game_real = real;
                    self.vis.set_game(self.forced_game.get().unwrap_or(real));
                }
                GameNews::Location(place) => self.location = place,
                GameNews::Denied(list) => self.denied.extend(list),
            }
        }
        while self.hotkey_presses.try_recv().is_ok() {
            self.fold_by_hand(now);
        }
        // The hotkey of the refinery scan: one reading at a time; the main window opens on what it read.
        let (paths, sound) = (self.paths.as_ref().map(|p| Paths::new(p.host.clone())), self.settings.sound);
        let read = self.refinery.poll(|done| match paths {
            Some(paths) => {
                if sound {
                    os::beep(false);
                }
                let wake = || {
                    if let Some(ctx) = crate::CONTEXT.get() {
                        ctx.request_repaint();
                    }
                };
                crate::refinery::read_screen(paths, writeguard::state(), done, wake);
            }
            None => {
                let _ = done.send(crate::refinery::Outcome::NoGame);
            }
        });
        match read.map(|outcome| outcome.hint()) {
            Some(None) => self.open_main(Signal::OpenStockScan),
            Some(Some(key)) => {
                self.rows.push(Row::Hint { text: Phrase::new(key).into(), contract: None }, self.settings.rows)
            }
            None => {}
        }
        // The hotkey of the handle search: the info bar takes the keyboard and its field the cursor; without the bar
        // on the screen a hint row says where the search is.
        let mut search_asked = false;
        while self.search_presses.try_recv().is_ok() {
            search_asked = true;
        }
        if search_asked {
            if self.info_bar_on() {
                self.info.type_by_key();
            } else {
                let hint = Row::Hint { text: Phrase::new("ov_hk_suche_ohne_leiste").into(), contract: None };
                self.rows.push(hint, self.settings.rows);
            }
        }
        // What became of the hotkey grab under X11: taken, no such key, or a new one that works.
        for phrase in hotkey_x11::take_told() {
            self.rows.push(Row::Hint { text: phrase.into(), contract: None }, self.settings.rows);
        }
    }

    /// The player takes a contract out of the bar. The watcher keeps the ids in its file, so it stays
    /// away after a restart; with sample contents the line just goes.
    pub fn hide_contract(&mut self, title: &str) {
        // Counter check: the button does nothing.
        if self.counter.as_deref() == Some("ausblenden_klemmt") {
            return;
        }
        let line = self.contracts.iter().find(|c| c.title == title).cloned();
        if !self.ask(Ask::Hide(title.to_string())) {
            self.contracts.retain(|c| c.title != title);
            self.rows.remove_contract(title);
        }
        self.hidden_note = Some(HiddenNote { title: title.to_string(), line, at: self.now() });
    }

    /// The player takes the hiding back. The watcher reads the log once more and tells the bar; with
    /// sample contents the line just comes back.
    pub fn unhide_contract(&mut self) {
        let Some(note) = self.hidden_note.take() else { return };
        // Counter check: the button does nothing.
        if self.counter.as_deref() == Some("ohne_rueckgaengig") {
            return;
        }
        if !self.ask(Ask::Unhide(note.title.clone()))
            && let Some(line) = note.line
            && !self.contracts.iter().any(|c| c.title == note.title)
        {
            self.contracts.push(line);
        }
    }

    /// The lines of the refinery jobs for this frame, while their part is shown: the file is read again once
    /// [`REFINERY_READ_EVERY`] is over, the time left counts every second.
    fn refinery_lines(&mut self, ctx: &egui::Context) -> Vec<(String, bool)> {
        if !self.settings.sections.contains(Section::Refinery) {
            return Vec::new();
        }
        if self.refinery_read.is_none_or(|at| at.elapsed() >= REFINERY_READ_EVERY) {
            self.refinery_read = Some(Instant::now());
            if let Some(paths) = self.paths.as_ref().map(|p| Paths::new(p.host.clone())) {
                self.refinery_jobs = versekit_core::refinery_jobs::load(&paths);
            }
        }
        let lines = versekit_core::overview::refinery_lines(&self.refinery_jobs, contested::now());
        if lines.iter().any(|(_, done)| !done) {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        lines
    }

    /// The note of the contract hidden last stays for [`UNHIDE_SECONDS`].
    fn expire_hidden_note(&mut self) {
        let now = self.now();
        if self.hidden_note.as_ref().is_some_and(|n| now - n.at > UNHIDE_SECONDS) {
            self.hidden_note = None;
        }
    }

    /// The icon next to the clock follows the setting: made when it is wanted, taken away when
    /// not. Only with data of the user or under `--probe` (the preview with sample contents has none).
    #[cfg(windows)]
    fn sync_tray(&mut self) {
        let probing = self.args.probe.is_some();
        let wanted = self.settings.tray && !self.closed && (self.paths.is_some() || probing);
        match (wanted, self.tray.is_some()) {
            (true, false) => {
                let shell: Arc<dyn crate::tray::Shell> = if probing {
                    let recorder = Arc::new(crate::tray::Recorder::default());
                    self.tray_log = Some(Arc::clone(&recorder));
                    recorder
                } else {
                    Arc::new(crate::tray::System)
                };
                let counter = self.counter.as_deref();
                self.tray =
                    Some(crate::tray::Tray::start(shell, &language::t("hf_titel"), env!("CARGO_PKG_VERSION"), counter));
                // Counter check: the icon takes the guard of the focus away.
                if counter == Some("symbol_fokus") {
                    crate::win::install(self.hwnd, false, true, true);
                }
            }
            (false, true) => self.tray = None,
            _ => {}
        }
    }

    /// What the icon reports (a click, a choice in its menu) and the messages for the system.
    fn take_tray(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        {
            self.sync_tray();
            if let Some(tray) = &self.tray {
                // Counter check: the entry stays out of the menu.
                tray.set_launcher(self.launcher && self.counter.as_deref() != Some("menue_ohne_start"));
            }
            let events = self.tray.as_ref().map(crate::tray::Tray::events).unwrap_or_default();
            for event in events {
                use crate::tray::{Entry, Event};
                match event {
                    Event::Show | Event::Pick(Entry::Open) => self.act(ctx, Action::Overview),
                    Event::Pick(Entry::Quit) => self.act(ctx, Action::Quit),
                    Event::Pick(Entry::Launcher) => self.act(ctx, Action::Launch),
                    Event::Pick(entry) => {
                        if let Some(signal) = entry.signal() {
                            // Counter check: every entry only brings the main window, none its page.
                            let signal = if self.counter.as_deref() == Some("menue_ohne_seite") {
                                Signal::Overview
                            } else {
                                signal
                            };
                            self.open_main(signal);
                        } else if let Some(url) = entry.address() {
                            self.open_link(url);
                        }
                    }
                }
            }
        }
        // An update was handed over: the program ends as the player would end it, so that its file can be replaced.
        if crate::QUIT_ASKED.swap(false, Ordering::Relaxed) && self.args.probe.is_none() {
            self.act(ctx, Action::Quit);
        }
        self.take_told();
        self.deliver_notices();
    }

    /// Takes over what the thread of the updates told: the version on offer and sentences for the status line.
    fn take_told(&mut self) {
        let told = std::mem::take(&mut *crate::told());
        self.apply_told(told);
    }

    fn apply_told(&mut self, told: crate::Told) {
        if let Some(version) = told.update {
            self.update = version;
        }
        for (key, argument) in told.say {
            self.status = Phrase::new(&key).with("0", argument).into();
        }
    }

    /// Opens an address in the browser; the self-check only notes it.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn open_link(&mut self, url: &str) {
        if self.args.probe.is_some() {
            self.opened.push(url.to_string());
        } else if !versekit_core::browser::open_in_browser(url) {
            self.status = Phrase::new("s_ub_auf_nein").with("0", url).into();
        }
    }

    /// Hands the messages for the system on: at the icon (Windows) or by `notify-send`.
    /// Switched off by the setting `systemmeldung`; without an icon Windows has no way to show one.
    fn deliver_notices(&mut self) {
        // The message about the update of the start waits until the icon stands.
        #[cfg(windows)]
        let ready = self.tray.as_ref().is_some_and(|tray| tray.placed());
        #[cfg(not(windows))]
        let ready = true;
        if ready && let Some(text) = self.args.notice.take() {
            self.notices.push((language::t("hf_update"), text));
        }
        if std::mem::take(&mut self.args.open_overview) {
            self.open_main(Signal::Overview);
        }
        if self.notices.is_empty() {
            return;
        }
        let notices = std::mem::take(&mut self.notices);
        // Counter check: nothing is handed on.
        if !self.settings.system_notice || self.counter.as_deref() == Some("ohne_systemmeldung") {
            return;
        }
        for (title, text) in notices {
            #[cfg(windows)]
            if let Some(tray) = &self.tray {
                tray.notice(&title, &text);
            }
            #[cfg(not(windows))]
            versekit_core::notify::show(&title, &text);
        }
    }

    /// The fold arrow or the hotkey: open what is closed, close what is open, and remember it for the next start.
    fn fold_by_hand(&mut self, now: f64) {
        self.vis.toggle(now, self.popup_seconds());
        if let Some(folded) = self.vis.folded_by_hand() {
            self.store(keys::FOLDED, Value::Bool(folded));
        }
    }

    /// Asks the watcher thread; with sample contents there is none.
    fn ask(&self, ask: Ask) -> bool {
        self.asks.as_ref().is_some_and(|asks| asks.send(ask).is_ok())
    }

    fn px(&self, ctx: &egui::Context, points: f32) -> i32 {
        (points * ctx.pixels_per_point()).round() as i32
    }

    /// The size the window has in a state, in pixels.
    fn size_for(&self, ctx: &egui::Context, shown: Shown) -> (i32, i32) {
        let bar_h = self.px(ctx, BAR_HEIGHT);
        let min_w = self.px(ctx, self.min_width).max(MIN_BAR_WIDTH);
        match shown {
            Shown::Open => (self.last_open.w.max(min_w), self.last_open.h.max(bar_h + OPEN_EXTRA_HEIGHT)),
            Shown::Bar => (min_w, bar_h),
            Shown::Strip => (self.px(ctx, self.strip.x).max(40), self.px(ctx, STRIP_HEIGHT)),
        }
    }

    /// Places the window when its state or the layout changed.
    fn place(&mut self, ctx: &egui::Context, shown: Shown) {
        if self.drag.is_some() {
            return;
        }
        let key = (shown, self.settings.layout);
        let size = self.size_for(ctx, shown);
        let target = match shown {
            Shown::Strip => overlay::strip_place(self.settings.layout, self.last_open, size.0, size.1),
            _ => {
                if self.placed_for == Some(key) && (self.current.w, self.current.h) == size {
                    return;
                }
                let from = overlay::placed_from(self.placed_for.map(|k| k.0), self.current, self.last_open);
                overlay::place(self.settings.layout, size, from, work_at(from.x, from.y))
            }
        };
        self.placed_for = Some(key);
        if target != self.current {
            self.resize_window(target);
            ctx.request_repaint();
        }
        if shown == Shown::Open {
            if self.last_open != self.current {
                // A new corner or size counts as the place to remember.
                self.place_changed = Some(Instant::now());
            }
            self.last_open = self.current;
        }
    }

    /// The custom drag: moving at the bar, resizing at the grip.
    fn follow_drag(&mut self, ctx: &egui::Context, shown: Shown) {
        let Some((kind, grab, start)) = self.drag else { return };
        if ctx.input(|i| i.pointer.primary_down()) {
            // Without the pointer this step is left out.
            let Some(p) = os::pointer() else { return };
            self.drag_to(ctx, kind, grab, start, p);
            ctx.request_repaint();
            return;
        }
        self.drag_ended(kind, start, shown);
    }

    /// The overlay follows the pointer `p` of a drag that began at `start`.
    fn drag_to(&mut self, ctx: &egui::Context, kind: Drag, grab: (i32, i32), start: Rect, p: Pointer) {
        match kind {
            // The whole window moves along, its size and region stay; it cannot leave what the screens cover.
            Drag::Move => {
                let r = Rect { x: p.x - grab.0, y: p.y - grab.1, ..self.current };
                let r = all_screens().map_or(r, |bounds| overlay::held_inside(r, bounds));
                self.move_visible(r);
            }
            // The visible part takes the size at once. The window around it keeps its
            // size (see [`Self::canvas_needed`]) and the region follows this frame.
            Drag::Resize => {
                let min_w = self.px(ctx, self.min_width).max(MIN_BAR_WIDTH);
                let work = Some(work_at(start.x, start.y));
                self.current = overlay::resized(self.settings.layout, start, (p.x, p.y), min_w, work);
            }
        }
    }

    /// Moves the visible part to `r` and the window around it by as much; size and region stay.
    fn move_visible(&mut self, r: Rect) {
        if r != self.current {
            self.canvas.x += r.x - self.current.x;
            self.canvas.y += r.y - self.current.y;
            self.set_window(self.canvas);
            self.current = r;
        }
    }

    /// A drag that began at `start` is over: let go near an edge of the screen under its middle, the window goes
    /// onto that edge and wholly onto that screen; the open place follows it, and a real move frees the corner
    /// (written at once, the main window told).
    pub fn drag_ended(&mut self, kind: Drag, start: Rect, shown: Shown) {
        self.drag = None;
        let dragged = (self.current.x - start.x).abs() + (self.current.y - start.y).abs();
        if kind == Drag::Move && dragged > DRAG_SLACK {
            let (cx, cy) = (self.current.x + self.current.w / 2, self.current.y + self.current.h / 2);
            if let (Some(area), Some(work)) = (screen::monitor_area(cx, cy, false), screen::monitor_area(cx, cy, true))
            {
                self.move_visible(overlay::snapped(self.current, Rect::from_area(area), Rect::from_area(work)));
            }
        }
        match shown {
            Shown::Open => self.last_open = self.current,
            // A folded bar carries the open place along: the same left
            // edge, and the edge of the bar.
            Shown::Bar => {
                self.last_open.x = self.current.x;
                self.last_open.y = match self.settings.layout.bar {
                    BarSide::Top => self.current.y,
                    BarSide::Bottom => self.current.bottom() - self.last_open.h,
                };
            }
            Shown::Strip => {}
        }
        let moved = (self.current.x - start.x).abs() + (self.current.y - start.y).abs();
        // A real move frees the corner; a click on the bar does not.
        if kind == Drag::Move
            && moved > DRAG_SLACK
            && self.settings.layout.corner != Corner::Free
            && self.counter.as_deref() != Some("frei")
        {
            self.settings.layout.corner = Corner::Free;
            self.store(keys::CORNER, Value::str(Corner::Free.setting()));
        }
        self.placed_for = None;
        self.place_changed = Some(Instant::now());
    }

    /// Remembers the open place once the window stood still, in the file thread.
    fn remember_place(&mut self) {
        let Some(since) = self.place_changed else { return };
        if since.elapsed() < PLACE_SETTLE || self.drag.is_some() {
            return;
        }
        self.place_changed = None;
        if self.paths.is_some() {
            self.file_job(files::Job::Place(overlay::to_points(self.last_open, self.scale)));
        }
    }

    pub fn act(&mut self, ctx: &egui::Context, action: Action) {
        let now = self.now();
        match action {
            Action::Quit => {
                self.closing();
                // Under `--probe` the window stays for the checks after it.
                if self.args.probe.is_some() {
                    self.quit_asked = true;
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Action::Clear => self.rows.clear(),
            Action::Fold => self.fold_by_hand(now),
            Action::Lock => {
                let on = !self.settings.click_through;
                self.settings.click_through = on;
                self.store(keys::CLICK_THROUGH, Value::Bool(on));
                self.status = Phrase::new(if on { "ov_v_schloss_zu" } else { "ov_schloss_offen" }).into();
            }
            Action::Overview => self.open_main(Signal::Overview),
            Action::Launch => self.launch_game(),
            Action::Extra(afk::ID) => {
                if let Some(on) = self.afk.toggle() {
                    self.status = Afk::said(on).into();
                }
            }
            Action::Extra(BELL) => self.open_main(Signal::OpenAbout),
            Action::Extra(scanner::EYE) => self.toggle_scanner(),
            Action::Extra(scanner::LEARN) => self.learn_scanner(),
            // An extra without a task does nothing (the placeholders of the self-check).
            Action::Extra(_) => {}
            Action::Rescan => match &self.asks {
                Some(asks) if asks.send(Ask::Rescan).is_ok() => self.status = Phrase::new("ov_liest").into(),
                _ => self.status = Phrase::new("s_be_neu_kein").into(),
            },
        }
    }

    /// The overlay closes, by its button, by "Beenden" of the icon's menu or by the system: its place is
    /// written here and now (the file thread ends with the program), and every main window of the run
    /// closes too. Done once.
    fn closing(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        // The switch goes off for good; the file the main window reads says so.
        self.afk.shutdown();
        self.scanner.close();
        self.fps.close();
        // The icon next to the clock goes first: a dead shell would stay there until the pointer passes over it.
        #[cfg(windows)]
        {
            self.tray = None;
        }
        if let Some(paths) = &self.paths {
            overlay::save_place(paths, overlay::to_points(self.last_open, self.scale));
        }
        // Counter check: the main window stays open.
        if self.counter.as_deref() == Some("hauptfenster_bleibt") {
            return;
        }
        // Every main window of the run on the desktop, whoever started it: the program is one program, as 4.12
        // was one process, and its end is the end of all its windows. ⚠ A main window left running could not be
        // brought back once it is hidden, and would hold the program file and its ports unseen.
        let found = window_signal::find_all(Role::Main);
        let mut told = 0;
        for &hwnd in &found {
            if window_signal::send(hwnd, Signal::Close) {
                told += 1;
            }
        }
        versekit_core::trail::step(&closing_line(found.len(), told));
    }

    /// The main window on the overview, or on the page `signal` names: brought to the front
    /// when it runs (without the keyboard, the game keeps it), otherwise started: the
    /// program of `--hauptprogramm` with its arguments, or the preview with
    /// the same data folder. Under `--probe` the start is only noted.
    ///
    /// A page asked for while the window is not there yet is kept and handed on as soon as the
    /// started window shows up (`deliver_pending_page`).
    pub fn open_main(&mut self, signal: Signal) {
        if self.counter.as_deref() == Some("uebersicht") {
            return;
        }
        if let Some(hwnd) = window_signal::find(Role::Main) {
            window_signal::bring_to_front(hwnd);
            window_signal::send(hwnd, signal);
            return;
        }
        // Counter check: the page is forgotten, the started window shows its overview.
        self.pending_page = (signal != Signal::Overview && self.counter.as_deref() != Some("seite_vergessen"))
            .then(|| (signal, Instant::now()));
        // Started a moment ago, still running and not yet marked: a second click starts no second one.
        let running = self.main_child.as_mut().is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if running && self.started_main.is_some_and(|t| t.elapsed() < MAIN_STARTING) {
            return;
        }
        // Counter check: the program given on the command line is passed over.
        let given = self.args.main_program.as_deref().filter(|_| self.counter.as_deref() != Some("hauptprogramm"));
        let folder = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        let preview = if given.is_none() { main_program() } else { None };
        let chosen = main_command(given, &self.args.main_args, self.args.data.as_deref(), preview, folder.as_deref());
        let Some((program, args)) = chosen else {
            self.status = Phrase::new("ov_uebersicht_fehlt").into();
            return;
        };
        // The probe is taken out of `self` while it runs a step: the switch decides.
        if self.args.probe.is_some() {
            self.launched = Some((program, args));
            return;
        }
        let mut command = std::process::Command::new(&program);
        command.args(&args);
        // Without a console window of its own above the game.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        match command.spawn() {
            Ok(child) => {
                self.started_main = Some(Instant::now());
                self.main_child = Some(child);
            }
            Err(_) => self.status = Phrase::new("ov_uebersicht_fehlt").into(),
        }
    }

    /// The game started or ended: the start programs of the player that go with it (`spiel`), or the
    /// ones that are to end with it. In a thread of its own: the files of the programs may lie on a share.
    pub fn game_programs(&self, started: bool) {
        let Some(paths) = self.paths.as_ref().map(Paths::working_copy) else { return };
        let spawner = Arc::clone(&self.spawner);
        std::thread::spawn(move || {
            if started {
                versekit_core::start_programs::on_game_started(&paths, &spawner);
            } else {
                versekit_core::start_programs::on_game_ended();
            }
        });
    }

    /// Starts the game by the way the player has (the rocket, the entry of the icon's menu): the
    /// lookup of the way and the start itself run in a thread of their own, the frame does not wait
    /// for the file system. A reason it did not work comes back through `launch_rx`.
    fn launch_game(&mut self) {
        // Counter check: the button does nothing.
        if self.counter.as_deref() == Some("rakete_tot") {
            return;
        }
        let Some(paths) = self.paths.as_ref().map(Paths::working_copy) else { return };
        self.status = Phrase::new("s_sp_start_lauft").into();
        let (spawner, sender) = (Arc::clone(&self.spawner), self.launch_tx.clone());
        std::thread::spawn(move || {
            let _ = sender.send(game_start::start_game(&paths, &spawner));
        });
    }

    /// Does a page asked for wait for the main window to show up?
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn page_waits(&self) -> bool {
        self.pending_page.is_some()
    }

    /// Hands a page asked for before the main window was there on to it, once it is marked; gives up
    /// after [`PAGE_WAIT`] (the start failed or took too long: the window shows the overview).
    fn deliver_pending_page(&mut self) {
        let Some((signal, since)) = self.pending_page else { return };
        if since.elapsed() > PAGE_WAIT {
            self.pending_page = None;
        } else if let Some(hwnd) = window_signal::find(Role::Main) {
            window_signal::send(hwnd, signal);
            self.pending_page = None;
        }
    }

    /// The lock in the bar in screen pixels, where it was drawn last.
    pub fn lock_on_screen(&self, ppp: f32) -> Option<Rect> {
        self.lock.map(|r| {
            Rect::new(
                self.canvas.x + (r.min.x * ppp).round() as i32,
                self.canvas.y + (r.min.y * ppp).round() as i32,
                (r.width() * ppp).round() as i32,
                (r.height() * ppp).round() as i32,
            )
        })
    }

    /// Clicks through or not, as the lock, the pointer and the state say.
    fn pass_clicks(&mut self, ctx: &egui::Context, shown: Shown, pointer: Option<Pointer>) {
        let lock = self.lock_on_screen(ctx.pixels_per_point());
        // Opening the lock catches clicks again; only then the hidden pointer decides anew.
        let current = self.passing && !(self.was_locked && !self.settings.click_through);
        self.was_locked = self.settings.click_through;
        let lock = if self.counter.as_deref() == Some("schloss") { None } else { lock };
        let wanted = overlay::passes_clicks(&self.settings, current, pointer, self.current, lock, shown);
        // Under X11 the lock takes clicks while the rest passes them, over what winit set: every frame, as winit
        // empties the input shape at the switch and at every new size.
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(r) = x11::passing_input(self.passing, self.settings.click_through, lock) {
            x11::set_input(self.hwnd, r.relative_to(self.canvas.x, self.canvas.y));
        }
        if wanted != self.passing {
            self.passing = wanted;
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(wanted));
            // The next frame comes after the switch: under X11 it gives the lock its clicks again.
            ctx.request_repaint();
            // The window got no pointer moves while clicks passed: tell it where the pointer is.
            if !wanted
                && let Some(p) = pointer
                && self.counter.as_deref() != Some("kein_anstoss")
            {
                let at = (p.x - self.canvas.x, p.y - self.canvas.y);
                os::nudge_pointer(self.hwnd, at.0, at.1);
                if self.args.probe.is_some() {
                    self.nudged.push(at);
                }
            }
        }
    }

    /// The signature scanner: starts or ends the watch as the settings say, moves the window with the material
    /// and stores the place the player dragged it to.
    fn step_scanner(&mut self, ctx: &egui::Context) {
        let waking = ctx.clone();
        let (on, layout, lock_open) =
            (self.settings.scanner, self.settings.scanner_window, !self.settings.click_through);
        let hint = self.settings.scanner_hint.clone();
        self.scanner.apply(on, layout, &hint, lock_open, move || waking.request_repaint());
        // The player asked in the settings to see the area that is read: for a few seconds over the game.
        if self.frame_asked.swap(false, Ordering::Relaxed) {
            if self.scanner.show_frame(theme::current()) {
                ctx.request_repaint_after(Duration::from_secs_f64(scanner::FRAME_SHOW_S + 0.3));
            } else {
                self.status = Phrase::new("ov_rahmen_kein_spiel").into();
            }
        }
        // The info bar shows the reading itself: the window of the scanner then stays closed, unless the player
        // wants it above the signature as well.
        let in_bar_only = layout.place == versekit_core::signature_window::Place::InfoBar;
        self.scanner.set_window_off(self.info_bar_on() && in_bar_only);
        let step = self.scanner.step(theme::current());
        // A number that left the HUD stays for a while: a frame comes when it goes.
        if let Some(left) = self.scanner.wake_in() {
            ctx.request_repaint_after(left + Duration::from_millis(50));
        }
        if step.hit {
            // The reading is judged by the switches written before it, like a find.
            self.read_control_now();
            let now = self.now();
            self.opens_for(Event::Scanner, now);
        }
        if let Some((dx, dy)) = step.offset {
            self.settings.scanner_window.dx = dx;
            self.settings.scanner_window.dy = dy;
            self.store(versekit_core::signature_window::keys::OFFSET_X, Value::int(i64::from(dx)));
            self.store(versekit_core::signature_window::keys::OFFSET_Y, Value::int(i64::from(dy)));
        }
        // A number that was not sure: a short word in the overlay, the picture is kept to teach later.
        if step.unsure {
            self.status = Phrase::new("ov_signatur_unsicher").into();
        }
        // The pill was found in a place the settings do not hold yet: kept for this size of the game window.
        if let Some((key, place)) = step.place {
            let kept = hint.with_place(&key, place);
            self.store(versekit_core::signature_window::keys::PLACES, kept.places_value());
            self.settings.scanner_hint = kept;
        }
    }

    /// The eye of the bar: the scanner on or off, the same switch as on the page Mining.
    fn toggle_scanner(&mut self) {
        let on = !self.settings.scanner;
        self.settings.scanner = on;
        self.store(versekit_core::signature_watch::SETTING, Value::Bool(on));
        self.status = Scanner::said(on).into();
    }

    /// The teaching: the scanner goes on with it (without a running watch there is no picture to teach from), and
    /// a small window opens above the game (the program with `--anlernen`; where that cannot be started, the page
    /// Mining of the main window takes it over). A second click while it is open starts no second one.
    fn learn_scanner(&mut self) {
        if !self.settings.scanner {
            self.toggle_scanner();
        }
        if self.learn_child.as_mut().is_some_and(|child| matches!(child.try_wait(), Ok(None))) {
            return;
        }
        let folder = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        let given = self.args.main_program.as_deref();
        let Some((program, args)) =
            main_command(given, &self.args.main_args, self.args.data.as_deref(), None, folder.as_deref())
                .filter(|(_, args)| args.iter().any(|a| a == MAIN_WINDOW_SWITCH))
        else {
            self.open_main(Signal::OpenMining);
            return;
        };
        let args: Vec<String> =
            args.into_iter().map(|a| if a == MAIN_WINDOW_SWITCH { LEARN_SWITCH.to_string() } else { a }).collect();
        if self.args.probe.is_some() {
            self.launched = Some((program, args));
            return;
        }
        let mut command = std::process::Command::new(&program);
        command.args(&args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        match command.spawn() {
            Ok(child) => self.learn_child = Some(child),
            Err(_) => self.open_main(Signal::OpenMining),
        }
    }

    /// Anti-AFK: takes the switching the main window asked for, keeps the beat, shows what came of it (the status
    /// line for a switching, a hint line for a stop) and keeps the button of the bar in the state of the switch.
    fn step_anti_afk(&mut self, ctx: &egui::Context) {
        let waking = ctx.clone();
        for news in self.afk.step(move || waking.request_repaint()) {
            match news {
                afk::News::Switched(on) => self.status = Afk::said(on).into(),
                afk::News::Stopped(key) => {
                    let hint = Row::Hint { text: Phrase::new(key).into(), contract: None };
                    self.rows.push(hint, self.settings.rows);
                }
            }
        }
        let mut extras: Vec<_> = self.afk.extra().into_iter().collect();
        extras.extend(self.scanner.extras());
        extras.push(bell(self.update.as_deref()));
        if !self.extras_kept {
            self.extras = extras;
        }
        // The yellow of a press ends by itself: a frame comes when it is over.
        if let Some(left) = self.afk.flash_left() {
            ctx.request_repaint_after(Duration::from_secs_f64(left));
        }
    }

    /// One frame of the overlay.
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.laid_out = false;
        self.take_control();
        self.refresh_settings(&ctx);
        self.take_news();
        if !std::mem::replace(&mut self.game_programs_checked, true) && self.game_real {
            self.game_programs(true);
        }
        self.take_tray(&ctx);
        self.step_scanner(&ctx);
        self.fps.apply(self.settings.reads_picture(), &ctx);
        self.step_anti_afk(&ctx);
        self.deliver_pending_page();
        // A second start asked for the main window (counter check: it is passed over).
        if self.overview_asked.swap(false, Ordering::Relaxed) && self.counter.as_deref() != Some("zweiter_start") {
            self.act(&ctx, Action::Overview);
        }
        self.measure_default(ui);
        if language::current() != self.title_lang {
            self.title_lang = language::current();
            ctx.send_viewport_cmd(ViewportCommand::Title(language::window_title(&language::t("ov_fenstertitel"))));
        }
        let now = self.now();
        #[cfg(windows)]
        crate::probe::step(self, &ctx, now);
        #[cfg(all(unix, not(target_os = "macos")))]
        crate::probe_x11::step(self, &ctx, now);
        if self.drag.is_none() {
            self.follow_system_move();
        }
        let pointer = self.pointer_override.or_else(os::pointer);
        // A hidden overlay is not there for the pointer either.
        let hovered = !self.waits_hidden() && pointer.is_some_and(|p| self.current.contains(p.x, p.y));
        if self.probe.is_none() {
            self.vis.set_hovered(now, hovered, self.popup_seconds());
        }
        let shown = self.vis.shown(now);
        self.place(&ctx, shown);
        self.follow_drag(&ctx, shown);
        if self.last_topmost.elapsed() >= Duration::from_secs(1) {
            os::reassert_topmost(self.hwnd);
            self.last_topmost = Instant::now();
        }

        let scheme = theme::current();
        let refinery = if shown == Shown::Open { self.refinery_lines(&ctx) } else { Vec::new() };
        // Counter check: the content is lost after coming back from the strip.
        let lost = self.counter.as_deref() == Some("leer") && self.was_waiting;
        let empty = Rows::default();
        let rows = if lost { &empty } else { &self.rows };
        let hangar = self.hangar_line(contested::now());
        let signature = self.scanner.line(now < self.scanner_line_until);
        let wanted = signature.as_ref().and_then(|_| self.demand_line());
        let lang = language::current();
        // The frame rates stand in the info bar while it shows, not twice.
        let fps = self.fps.reading().filter(|_| !self.info_bar_on());
        let (shard_part, shard_hint) =
            (shard::status_part(&self.shard, fps.as_ref(), self.shown_parts(), lang), shard::hint(&self.shard, lang));
        // The parts the player switched off are not drawn at all.
        let sections = self.settings.sections;
        let contracts_shown = sections.contains(Section::Contracts) && self.counter.as_deref() != Some("ohne_leiste");
        let shown_view = view::View {
            scheme,
            shown,
            rows,
            // Counter check: the bar is not drawn.
            contracts: if contracts_shown { self.contracts.as_slice() } else { &[] },
            contracts_max: self.settings.contracts_max as usize,
            hidden: self.hidden_note.as_ref().map(|n| n.title.as_str()).filter(|_| contracts_shown),
            refinery: &refinery,
            launcher: self.launcher,
            status: &self.status,
            location: self.location.as_ref().filter(|_| sections.contains(Section::Location)),
            shard: shard_part.as_deref().filter(|_| sections.contains(Section::Shard)),
            shard_hint: shard_hint.as_deref(),
            signature: signature.as_deref(),
            demand: wanted.as_deref(),
            update: self.update.as_deref().filter(|_| sections.contains(Section::Update)),
            game: self.vis.game(),
            locked: self.settings.click_through,
            bar: self.settings.layout.bar,
            anchored: self.settings.layout.anchored(),
            // A window drawn without DirectComposition shows no transparency: the background stands solid.
            opacity: if versekit_ui::renderer::draws_transparent() {
                self.settings.opacity as f32 / 100.0
            } else {
                1.0
            },
            extras: &self.extras,
            menu: self.counter.as_deref() != Some("menue"),
            drag_under_buttons: self.counter.as_deref() == Some("ziehhinweis"),
            hangar: hangar.as_ref(),
            hints_outside: true,
        };
        // The window of this frame; it gets its rect after the layout.
        let canvas = self.canvas_needed(&ctx, shown);
        let out = match self.drawn_rect(&ctx, ui.max_rect(), canvas) {
            Some(rect) => view::show(&mut sized_child(ui, rect), &shown_view),
            None => view::show(ui, &shown_view),
        };
        self.laid_out = true;
        if shown == Shown::Strip {
            self.was_waiting = true;
            if out.strip.x > 0.0 {
                self.strip = out.strip;
            }
        } else if out.min_width > 0.0 {
            self.min_width = out.min_width;
        }
        self.lock = out.lock;
        self.buttons = out.buttons;
        self.in_menu = out.in_menu;
        self.more = out.more;
        self.title_end = out.title_end;
        self.hint = out.hint;
        self.drawn = out.drawn;
        self.visible_rows = out.visible_rows;
        self.contract_texts = out.contract_texts;
        self.hide_buttons = out.hide_buttons;
        self.unhide_button = out.unhide_button;
        if let Some(title) = out.hide {
            self.hide_contract(&title);
        }
        if out.unhide {
            self.unhide_contract();
        }
        self.expire_hidden_note();
        if let Some(kind) = out.drag
            && let Some(p) = os::pointer()
        {
            self.drag = Some((kind, (p.x - self.current.x, p.y - self.current.y), self.current));
        }
        for action in out.actions {
            self.act(&ctx, action);
        }
        // Right before eframe paints this frame, laid out for the window.
        let canvas_changed = canvas != self.canvas;
        if canvas_changed {
            self.apply_canvas(canvas);
        }
        self.flush_window(&ctx);
        self.follow_region(&ctx, canvas_changed);
        #[cfg(windows)]
        if let Some(probe) = self.probe.as_mut() {
            probe.painted_part(self.hwnd, self.painted);
        }
        self.pass_clicks(&ctx, shown, pointer);
        let pointer_now = self.pointer_override.or_else(os::pointer);
        self.step_info_bar(&ctx, pointer_now);
        // No hint while a drag runs: it would follow the pointer over the screen.
        let hint = self.hint.as_deref().filter(|_| self.drag.is_none() && !self.waits_hidden());
        self.hint_window.step(&ctx, hint, pointer_now, scheme);
        self.remember_place();
        ctx.request_repaint_after(TICK);
    }

    /// Whether the info bar shows: its switch is on, the overlay is not closed and the game runs (without the game
    /// the bar has nothing to show and waits hidden). While it shows, the frame rates and the scanner reading
    /// stand in it alone.
    /// The parts of the picture's reading the player switched on.
    fn shown_parts(&self) -> shard::Show {
        shard::Show { own: self.settings.fps_line, server: self.settings.server_fps, players: self.settings.players }
    }

    fn info_bar_on(&self) -> bool {
        self.settings.info_bar && !self.closed && (self.vis.game() || self.settings.info_bar_always)
    }

    /// Whether the overlay waits hidden: the game does not run and the player did not ask for the overlay without
    /// it. The self-check (`--probe`) always has it on the screen.
    fn waits_hidden(&self) -> bool {
        !self.vis.game() && !self.settings.overlay_always && self.probe.is_none()
    }

    /// The info bar: shown while its switch is on, with the frame rates and the shard while the game runs and the
    /// scanned material while the scanner is on; a place it was dragged to is stored.
    fn step_info_bar(&mut self, ctx: &egui::Context, pointer: Option<Pointer>) {
        if self.info.set_corner(self.settings.info_bar_corner) {
            // Another corner chosen: the place dragged to on this screen size goes, so the bar moves there.
            let key = versekit_core::info_bar::screen_key(screens_now().0);
            let places = versekit_core::info_bar::Places(
                self.settings.info_bar_places.0.iter().filter(|(k, _)| *k != key).cloned().collect(),
            );
            self.store(versekit_core::info_bar::keys::PLACES, places.value());
            self.settings.info_bar_places = places.clone();
            self.file_settings.info_bar_places = places;
        }
        self.info.set_places(self.settings.info_bar_places.clone());
        let shard = self.shard.current().filter(|_| self.vis.game()).map(|join| join.shard.short());
        // The reading stands in the bar unless the player chose the window above the signature for it.
        let in_bar = self.settings.scanner_window.place == versekit_core::signature_window::Place::InfoBar;
        let text = self.scanner.shown().filter(|_| self.settings.scanner && in_bar);
        let mining = text.map(|text| versekit_ui::info_bar::Mining {
            material: text.material.as_str(),
            known: text.known,
            number: text.number.as_str(),
        });
        let (show, lang) = (self.shown_parts(), language::current());
        let reading = self.fps.reading();
        let fps = reading.as_ref().and_then(|reading| shard::fps_part(reading, show, lang));
        let players = reading.as_ref().and_then(|reading| shard::players_part(reading, show, lang));
        let transparent = versekit_ui::renderer::draws_transparent();
        let content = versekit_ui::info_bar::View {
            scheme: theme::current(),
            fps: fps.as_deref(),
            players: players.as_deref(),
            shard: shard.as_deref(),
            mining,
            opacity: if transparent { self.settings.opacity as f32 / 100.0 } else { 1.0 },
            // The lock of the overlay is the lock of the bar: closed, clicks pass through both.
            movable: !self.settings.click_through,
        };
        let on = self.info_bar_on();
        // The context draws at the overlay's size; the bar's viewport at its own.
        let zoom = self.settings.info_bar_size_percent as f32 / self.settings.size_percent.max(1) as f32;
        self.info.set_zoom(ctx, zoom);
        let step = self.info.step(ctx, on, &content, pointer, Some(&mut self.search));
        if let Some(places) = step.store {
            self.store(versekit_core::info_bar::keys::PLACES, places.value());
            self.settings.info_bar_places = places.clone();
            self.file_settings.info_bar_places = places;
        }
    }

    /// The visible part in points of the window as it stands; `None` before it has a size.
    fn visible_points(&self, ppp: f32) -> Option<egui::Rect> {
        let part = self.current.relative_to(self.canvas.x, self.canvas.y);
        (part.w > 0 && part.h > 0 && self.canvas.w > 0).then(|| to_points(part, ppp))
    }
}

/// How far the system moved the window: from where the overlay put it (`canvas`) to where it stands
/// (`real`, empty where it cannot be asked), in pixels; `None` without a move. While a place sent `sent`
/// ago may still be on its way ([`PLACE_ARRIVES`]), the window standing elsewhere is not counted: the
/// window system may move it a little later.
fn system_move(real: Rect, canvas: Rect, sent: Option<Duration>) -> Option<(i32, i32)> {
    let offset = (real.x - canvas.x, real.y - canvas.y);
    if real.w <= 0 || offset == (0, 0) || sent.is_some_and(|age| age < PLACE_ARRIVES) {
        return None;
    }
    Some(offset)
}

/// The window place `r` (pixels) as the window system takes it: the outer
/// place and the inner size, in points. The window has no frame, so both
/// are the same rectangle.
fn window_commands(r: Rect, scale: f32) -> [ViewportCommand; 2] {
    let scale = scale.max(0.01);
    [
        ViewportCommand::OuterPosition(egui::pos2(r.x as f32 / scale, r.y as f32 / scale)),
        ViewportCommand::InnerSize(egui::vec2(r.w as f32 / scale, r.h as f32 / scale)),
    ]
}

/// Window pixels to points.
fn to_points(r: Rect, ppp: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(r.x as f32, r.y as f32) / ppp, egui::vec2(r.w as f32, r.h as f32) / ppp)
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    /// Closed by the system (task bar, Alt+F4): the same closing as by the button.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.closing();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        raw_input.events.append(&mut self.injected);
        // egui lays out in the visible part of the window: tooltips stay inside it.
        let native = raw_input.viewports.get(&raw_input.viewport_id).and_then(|v| v.native_pixels_per_point);
        let ppp = native.map_or_else(|| ctx.pixels_per_point(), |n| n * ctx.zoom_factor());
        if let Some(rect) = self.visible_points(ppp) {
            raw_input.screen_rect = Some(rect);
        }
    }
}

/// The main program next to the overlay: the preview, alone or in the
/// `examples` folder of a build.
fn main_program() -> Option<std::path::PathBuf> {
    let folder = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [folder.join("vorschau.exe"), folder.join("examples").join("vorschau.exe")].into_iter().find(|p| p.is_file())
}

/// What the button "Übersicht öffnen" starts: the `given` program with
/// exactly `given_args` (relative to the overlay's `folder`), otherwise the
/// `preview` with the data folder; `None` when there is neither.
fn main_command(
    given: Option<&std::path::Path>,
    given_args: &[String],
    data: Option<&str>,
    preview: Option<std::path::PathBuf>,
    folder: Option<&std::path::Path>,
) -> Option<(std::path::PathBuf, Vec<String>)> {
    match given {
        Some(program) => {
            let program = match folder {
                Some(folder) if program.is_relative() => folder.join(program),
                _ => program.to_path_buf(),
            };
            Some((program, given_args.to_vec()))
        }
        None => preview.map(|p| (p, data.iter().flat_map(|d| ["--daten".to_string(), d.to_string()]).collect())),
    }
}

/// The line of the trail at the end of the overlay: how many main windows of the run were found, and how many of
/// them took the request to close.
fn closing_line(found: usize, told: usize) -> String {
    format!("Overlay endet: {found} Hauptfenster gefunden, {told} zum Schließen gebeten")
}

/// A `Ui` over `rect` that paints there and keeps the widget ids of `ui`:
/// scroll position, hover and drags stay the same as in a frame without it.
fn sized_child(ui: &mut egui::Ui, rect: egui::Rect) -> egui::Ui {
    let mut sized = ui.new_child(egui::UiBuilder::new().max_rect(rect).scope_id(ui.scope_id()));
    sized.set_clip_rect(rect);
    sized
}

/// The handle of the window: the HWND on Windows, the XID under X11; 0 elsewhere (Wayland, macOS).
fn hwnd_of(window: &impl raw_window_handle::HasWindowHandle) -> isize {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Win32(h)) => h.hwnd.get(),
        Ok(raw_window_handle::RawWindowHandle::Xlib(h)) => h.window as isize,
        Ok(raw_window_handle::RawWindowHandle::Xcb(h)) => h.window.get() as isize,
        _ => 0,
    }
}

/// Opens the window on the window system [`screen::backend`] chooses. winit takes Wayland wherever it is
/// there; X11 is asked for by name.
#[cfg(all(unix, not(target_os = "macos")))]
fn choose_window_system(options: &mut eframe::NativeOptions) {
    if screen::backend() == screen::Backend::X11 {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11 as _;
            builder.with_x11();
        }));
    }
}

/// Watches the game: running or not every two seconds, the location every
/// ten while it runs. Writes under the same lock as the window.
fn start_game_watch(
    host: Option<Host>,
    forced: Arc<ForcedGame>,
    guard: writeguard::State,
    news: Sender<GameNews>,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::Builder::new()
        .name("spielwache".into())
        .spawn(move || {
            writeguard::adopt(&guard);
            let paths = host.map(Paths::new);
            let mut last_location = Instant::now() - LOCATION_EVERY;
            loop {
                let age = paths.as_ref().and_then(overlay::log_age);
                let real = overlay::game_running(os::game_process(), age);
                let running = forced.get().unwrap_or(real);
                if news.send(GameNews::Running(real)).is_err() {
                    return;
                }
                if let Some(paths) = &paths
                    && last_location.elapsed() >= LOCATION_EVERY
                {
                    last_location = Instant::now();
                    let phrase = if running {
                        let place = location::place(paths, None);
                        location::overlay_phrase(place.as_ref(), &location::names(paths))
                    } else {
                        None
                    };
                    let _ = news.send(GameNews::Location(phrase));
                    let denied = writeguard::take_denied();
                    if !denied.is_empty() {
                        let _ = news.send(GameNews::Denied(denied));
                    }
                }
                wake();
                std::thread::sleep(GAME_EVERY);
            }
        })
        .expect("game watch thread");
}

/// How often the control file is looked at.
const CONTROL_EVERY: Duration = Duration::from_millis(100);

/// Watches the control file: every new content comes as a request; `wake` brings a frame.
/// A file that is not there (yet, or any more) asks for nothing.
fn start_control_watch(path: String, wake: impl Fn() + Send + 'static) -> Receiver<Control> {
    let (requests, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("steuerung".into())
        .spawn(move || {
            let mut last: Option<Vec<u8>> = None;
            loop {
                let now = std::fs::read(&path).ok();
                if now != last {
                    if let Some(bytes) = &now {
                        let control = Control::parse(&String::from_utf8_lossy(bytes));
                        if requests.send(control).is_err() {
                            return;
                        }
                        wake();
                    }
                    last = now;
                }
                std::thread::sleep(CONTROL_EVERY);
            }
        })
        .expect("control watch thread");
    receiver
}

/// What the overlay is put together from at its start.
struct Parts {
    hwnd: isize,
    paths: Option<Paths>,
    files: Option<files::Files>,
    args: Args,
    file_settings: Settings,
    game_real: bool,
    forced_game: Arc<ForcedGame>,
    news: Receiver<News>,
    asks: Option<Sender<Ask>>,
    game_news: Receiver<GameNews>,
    hotkey: Arc<Mutex<Option<Hotkey>>>,
    hotkey_presses: Receiver<()>,
    refinery: crate::refinery::RefineryKey,
    search_hotkey: Arc<Mutex<Option<Hotkey>>>,
    search_presses: Receiver<()>,
    control: Option<Receiver<Control>>,
    start: Rect,
    /// A size or place was remembered: the first frame does not measure a default size.
    remembered: bool,
    place_known: bool,
    scale: f32,
    points: i64,
    settings_told: Arc<AtomicBool>,
    overview_asked: Arc<AtomicBool>,
    frame_asked: Arc<AtomicBool>,
    /// What the main window asked of anti-AFK (see [`afk::ASKED_ON`]).
    afk_asked: Arc<AtomicU8>,
    /// Where the demand the main window hands over lands.
    demand: Arc<Mutex<Vec<Material>>>,
}

impl OverlayApp {
    /// "Die Einheit sucht: …" for the ores behind the signature shown now, when the unit wants one of them.
    fn demand_line(&self) -> Option<String> {
        let wanted = self.demand.lock().unwrap_or_else(|e| e.into_inner());
        Some(demand::overlay_line(&demand::matching_in(&wanted, self.scanner.names()))).filter(|l| !l.is_empty())
    }

    /// The line of the Executive Hangar at `wall` (Unix seconds), when the player switched it on.
    fn hangar_line(&self, wall: f64) -> Option<contested::OverlayLine> {
        self.settings.hangar_timer.then(|| contested::overlay_line(self.hangar_start, wall))
    }

    fn new(p: Parts) -> OverlayApp {
        let sample = p.paths.is_none();
        // Anti-AFK is there on a system that can do it, on real data, and not in the self-check (which has a
        // bar of its own to measure).
        let afk = match &p.paths {
            Some(paths) if p.args.probe.is_none() && versekit_core::anti_afk::supported() => Afk::with_runner(
                versekit_core::anti_afk::Runner::system(Some(Paths::new(paths.host.clone()))),
                p.afk_asked.clone(),
            ),
            _ => Afk::none(p.afk_asked.clone()),
        };
        // The scanner and the frame rates read the screen only on real data and never in the self-check.
        let reads_screen = p.args.probe.is_none() && (p.args.real || p.args.data.is_some());
        let scanner = Scanner::new(p.paths.as_ref(), reads_screen);
        let fps = crate::fps::FpsLine::new(reads_screen);
        // The self-check and the preview never ask the RSI website.
        let search = match &p.paths {
            Some(paths) if p.args.probe.is_none() => HandleSearch::with_files(paths),
            Some(paths) => HandleSearch::with_net(paths, None),
            None => HandleSearch::empty(),
        };
        let mut settings = p.file_settings.clone();
        apply_overrides(&mut settings, &p.args);
        let start = p.start;
        // The self-check never starts the launcher or the game: its start goes to a recorder.
        let spawn_log = p.args.probe.is_some().then(|| Arc::new(game_start::Recorder::default()));
        let spawner: Arc<dyn Spawner> = match &spawn_log {
            Some(recorder) => recorder.clone(),
            None => Arc::new(game_start::System),
        };
        let (launch_tx, launch_rx) = mpsc::channel();
        let mut vis = Visibility::new(settings.mode, p.forced_game.get().unwrap_or(p.game_real));
        // Folded or opened by hand at the last run, as 4.12 keeps it; the self-check starts from the game alone.
        if p.args.probe.is_none()
            && let Some(folded) = p.paths.as_ref().and_then(folded_by_hand)
        {
            vis.restore(folded);
        }
        OverlayApp {
            hwnd: p.hwnd,
            paths: p.paths,
            settings: settings.clone(),
            file_settings: p.file_settings,
            forced_game: p.forced_game.clone(),
            game_real: p.game_real,
            control: p.control,
            control_path: p.args.control.clone(),
            control_taken: None,
            files: p.files,
            inline_read: Instant::now(),
            inline_writes: 0,
            scanner_line_until: 0.0,
            demand: p.demand,
            update: p.args.update.clone(),
            counter: p.args.counter.clone(),
            args: p.args,
            vis,
            rows: if sample { OverlayApp::sample_rows() } else { Rows::default() },
            contracts: Vec::new(),
            notices: Vec::new(),
            #[cfg(windows)]
            tray: None,
            #[cfg(windows)]
            tray_log: None,
            opened: Vec::new(),
            status: if sample {
                Phrase::new("ov_status").with("0", 3).with("1", "21:14:05").into()
            } else {
                Phrase::new("ov_starte").into()
            },
            location: None,
            news: p.news,
            asks: p.asks,
            game_news: p.game_news,
            hotkey: p.hotkey,
            hotkey_presses: p.hotkey_presses,
            refinery: p.refinery,
            search_hotkey: p.search_hotkey,
            search_presses: p.search_presses,
            scmdb: versekit_core::scmdb_bridge::Bridge::new(),
            started: Instant::now(),
            current: start,
            last_open: start,
            canvas: start,
            window_to: None,
            place_sent: None,
            #[cfg(all(unix, not(target_os = "macos")))]
            region_refused_told: false,
            #[cfg(not(windows))]
            os_hidden: false,
            region: None,
            region_rounding: 0,
            painted: Rect::default(),
            shrink_at: None,
            placed_for: None,
            drag: None,
            passing: false,
            was_locked: false,
            last_topmost: Instant::now(),
            place_changed: None,
            place_known: p.place_known,
            default_pending: !p.remembered,
            min_width: 300.0,
            strip: egui::vec2(180.0, STRIP_HEIGHT),
            lock: None,
            drawn: Vec::new(),
            visible_rows: 0,
            contract_texts: Vec::new(),
            hide_buttons: Vec::new(),
            hidden_note: None,
            refinery_jobs: Vec::new(),
            refinery_read: None,
            unhide_button: None,
            launcher: false,
            game_programs_checked: false,
            spawner,
            spawn_log,
            launch_tx,
            launch_rx,
            scale: p.scale,
            points: p.points,
            title_lang: language::current(),
            denied: Vec::new(),
            archive_read: None,
            probe: None,
            was_waiting: false,
            laid_out: false,
            buttons: Vec::new(),
            in_menu: Vec::new(),
            more: None,
            title_end: 0.0,
            hint: None,
            extras: Vec::new(),
            extras_kept: false,
            afk,
            scanner,
            shard: Joins::default(),
            info: info_window::InfoWindow::new(settings.info_bar_places.clone()),
            hint_window: hint_window::HintWindow::new(),
            search,
            fps,
            hangar_start: None,
            launched: None,
            quit_asked: false,
            closed: false,
            main_child: None,
            learn_child: None,
            started_main: None,
            pending_page: None,
            settings_told: p.settings_told,
            overview_asked: p.overview_asked,
            frame_asked: p.frame_asked,
            pointer_override: None,
            injected: Vec::new(),
            nudged: Vec::new(),
        }
    }
}

/// Whether the player left the overlay folded (`true`) or opened (`false`) by hand; `None` when it follows the game.
fn folded_by_hand(paths: &Paths) -> Option<bool> {
    paths.settings().into_iter().find(|(k, _)| k == keys::FOLDED).and_then(|(_, v)| match v {
        Value::Bool(folded) => Some(folded),
        _ => None,
    })
}

/// The window of the overlay before size and place are set: title, the class every window of the program carries
/// (the desktop entry on Linux is matched against it, so the overlay groups with the main window), no frame,
/// transparent, always on top, in the taskbar with the icon of the program.
fn base_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(language::window_title(&language::t("ov_fenstertitel")))
        .with_icon(versekit_ui::window_look::program_icon())
        .with_app_id(versekit_core::paths::WM_CLASS)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_active(false)
        .with_taskbar(true)
}

/// Streamer mode follows the setting of the main window, which switches it: when it turns on, what
/// the program knows about the person at the machine (home, folders, accounts) is learned.
fn follow_streamer_mode(paths: &Paths) {
    let on = versekit_core::streamer::load(paths);
    if on != versekit_core::streamer::active() {
        versekit_core::streamer::set_active(on);
        if on {
            versekit_core::streamer::learn_paths(paths);
        }
    }
}

pub fn run(args: Args, paths: Option<Paths>, points: i64) -> eframe::Result {
    if let Some(paths) = &paths {
        follow_streamer_mode(paths);
    }
    let scale = screen::system_scale();
    let file_settings = paths.as_ref().map(Settings::read).unwrap_or_default();
    let mut settings = file_settings.clone();
    apply_overrides(&mut settings, &args);
    let remembered = match &args.place {
        Some(text) => overlay::parse_geometry(text).map(|(w, h, place)| Remembered { w, h, place }),
        None => paths.as_ref().and_then(overlay::load_remembered),
    };
    let size_percent = settings.size_percent;
    let start = start_rect(remembered, estimated_default(points, size_percent, scale), settings.layout.bar, scale);
    let game_real = overlay::game_running(os::game_process(), paths.as_ref().and_then(overlay::log_age));
    let forced_game = Arc::new(ForcedGame::new(args.game.then_some(true)));

    let viewport = base_viewport()
        .with_inner_size([start.w as f32 / scale, start.h as f32 / scale])
        .with_position([start.x as f32 / scale, start.y as f32 / scale]);
    #[cfg_attr(target_os = "macos", allow(unused_mut))]
    let mut options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Glow, ..Default::default() };
    // OpenGL drops the alpha of the window on Windows (see `win`).
    #[cfg(windows)]
    {
        options.renderer = crate::win::RENDERER;
        options.wgpu_options = crate::win::wgpu_options();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    choose_window_system(&mut options);
    // The self-check runs where nobody could close a message box.
    versekit_ui::renderer::keep_quiet(args.probe.is_some());
    if args.counter.as_deref() == Some("ohne_dcomp") {
        let composed = versekit_ui::renderer::Stage::ALL.into_iter().filter(|s| s.transparent()).collect::<Vec<_>>();
        versekit_ui::renderer::refuse_for_check(&composed);
    }

    versekit_ui::renderer::run_native(
        "versekit-overlay",
        options,
        versekit_ui::renderer::Look::Transparent,
        Box::new(move |cc| {
            versekit_ui::install(&cc.egui_ctx);
            let _ = crate::CONTEXT.set(cc.egui_ctx.clone());
            versekit_ui::apply(&cc.egui_ctx, theme::current());
            versekit_ui::apply_window_size(&cc.egui_ctx, points, size_percent);
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            let hwnd = hwnd_of(cc);
            #[cfg(windows)]
            crate::win::spoil_border_request(args.counter.as_deref() == Some("rand_abgelehnt"));
            #[cfg(windows)]
            let guard_ok = hwnd != 0
                && crate::win::install(
                    hwnd,
                    args.counter.as_deref() != Some("fokus"),
                    args.counter.as_deref() == Some("rahmen"),
                    args.counter.as_deref() != Some("eckig"),
                );
            // Under X11 the window never takes the keyboard (counter check: it is left as winit makes it).
            #[cfg(all(unix, not(target_os = "macos")))]
            let guard_ok = args.counter.as_deref() != Some("fokus") && x11::install(hwnd);
            #[cfg(target_os = "macos")]
            let guard_ok = false;
            #[cfg(windows)]
            os::set_rect(hwnd, start);
            // The main window finds the overlay and says when it changed the
            // settings; a second start asks it for the main window.
            let settings_told = Arc::new(AtomicBool::new(false));
            let overview_asked = Arc::new(AtomicBool::new(false));
            let afk_asked = Arc::new(AtomicU8::new(0));
            let frame_asked = Arc::new(AtomicBool::new(false));
            let (told, asked, afk_told, framed, ctx) = (
                settings_told.clone(),
                overview_asked.clone(),
                afk_asked.clone(),
                frame_asked.clone(),
                cc.egui_ctx.clone(),
            );
            window_signal::serve(hwnd, Role::Overlay, move |signal| {
                // The main window asks the overlay, which presses the keys, to switch anti-AFK.
                if afk::take_wish(signal, &afk_told) {
                    ctx.request_repaint();
                    return;
                }
                let flag = match signal {
                    Signal::Settings => &told,
                    Signal::Overview => &asked,
                    Signal::ShowFrame => &framed,
                    // The pages are for the main window; the overlay has none. Anti-AFK is handled above.
                    Signal::Close
                    | Signal::OpenGeneral
                    | Signal::OpenTranslation
                    | Signal::OpenAbout
                    | Signal::OpenMining
                    | Signal::OpenStockScan
                    | Signal::AntiAfkOn
                    | Signal::AntiAfkOff
                    | Signal::DemandWanted => return,
                };
                flag.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            });
            // The demand of the unit comes from the main window, in memory only; a main window that runs already is
            // asked for it, one that starts later hands it over by itself.
            let demand: Arc<Mutex<Vec<Material>>> = Arc::new(Mutex::new(Vec::new()));
            let (handed, ctx) = (demand.clone(), cc.egui_ctx.clone());
            window_signal::on_data(move |kind, bytes| {
                if kind == window_signal::DataKind::Demand {
                    *handed.lock().unwrap_or_else(|e| e.into_inner()) = demand::taken_over(&bytes);
                    ctx.request_repaint();
                }
            });
            window_signal::send_to(Role::Main, Signal::DemandWanted);
            let (news_tx, news_rx) = mpsc::channel();
            let texts_news = news_tx.clone();
            let stock_news = news_tx.clone();
            let (game_tx, game_rx) = mpsc::channel();
            let (hotkey_tx, hotkey_rx) = mpsc::channel();
            let hotkey = Arc::new(Mutex::new(settings.hotkey));
            let place_known =
                paths.as_ref().is_some_and(|p| std::path::Path::new(&p.app_file(overlay::GEOMETRY_FILE)).exists());
            // The other threads write under the same lock as this one.
            let guard = writeguard::state();
            // The files of the overlay on a `Paths` of their own, in their own thread.
            if args.probe.is_some() {
                files::set_slow(args.slow.unwrap_or(0));
            }
            let own_files = if args.real && args.data.is_none() {
                Some(Paths::new(Host::current()))
            } else {
                args.data.as_deref().and_then(|root| Paths::for_copy(Host::current(), root))
            };
            let files = own_files.map(|own| {
                let ctx = cc.egui_ctx.clone();
                files::start(own, guard.clone(), move || ctx.request_repaint())
            });
            let mut asks = None;
            if let Some(paths) = &paths {
                let (asks_tx, asks_rx) = mpsc::channel();
                let ctx = cc.egui_ctx.clone();
                let start = watcher::Start {
                    host: paths.host.clone(),
                    version: env!("CARGO_PKG_VERSION"),
                    guard: guard.clone(),
                    counter_write: args.counter.as_deref() == Some("schreiben"),
                    archive: args.archive.clone(),
                    counter_archive: args.counter.as_deref() == Some("archiv"),
                    // The self-check cannot wait twenty seconds for the way to start the game.
                    starter_every: if args.probe.is_some() { Duration::from_secs(1) } else { watcher::STARTER_EVERY },
                };
                watcher::start(start, news_tx, asks_rx, move || ctx.request_repaint());
                asks = Some(asks_tx);
            }
            if args.probe.is_none() {
                // The signatures of the game data for the scanner, read when the game archive changed.
                if let Some(paths) = &paths {
                    let archive = args.archive.as_deref();
                    watcher::keep_signatures(watcher::thread_paths(paths, archive), guard.clone());
                    // The game's texts are kept here, not in the main window: the overlay runs the whole session.
                    let ctx = cc.egui_ctx.clone();
                    let texts = watcher::thread_paths(paths, archive);
                    watcher::keep_texts(texts, guard.clone(), texts_news, move || ctx.request_repaint());
                    // The live game build and the goods list, for every list fetched in the session.
                    let ctx = cc.egui_ctx.clone();
                    let stock = watcher::thread_paths(paths, archive);
                    watcher::keep_stock(stock, guard.clone(), stock_news, move || ctx.request_repaint());
                }
                let ctx = cc.egui_ctx.clone();
                let host = paths.as_ref().map(|p| p.host.clone());
                start_game_watch(host, forced_game.clone(), guard, game_tx, move || ctx.request_repaint());
            }
            // The live switches are watched only when asked for, and in a thread of their own.
            let control_rx = args.control.clone().map(|path| {
                let ctx = cc.egui_ctx.clone();
                start_control_watch(path, move || ctx.request_repaint())
            });
            // What the settings make of the hotkeys; the start trail says what came of them, as in 4.12.
            let hotkey_choice = paths.as_ref().map(overlay::HotkeyChoice::read);
            let scan_choice = paths.as_ref().map(overlay::HotkeyChoice::read_scan);
            let (scan_tx, scan_rx) = mpsc::channel();
            let refinery = crate::refinery::RefineryKey::new(settings.scan_hotkey, scan_rx);
            #[cfg_attr(target_os = "macos", allow(unused_mut))]
            let mut registered: Result<(), String> = Err("kein System, das es kann".into());
            #[cfg_attr(not(windows), allow(unused_mut))]
            let mut scan_registered: Result<(), String> = Err("kein System, das es kann".into());
            // The refinery scan reads only where the screen can be read (Windows with its text recognition).
            #[cfg(windows)]
            if args.probe.is_none() && versekit_core::refinery_scan::screen::supported() {
                let ctx = cc.egui_ctx.clone();
                scan_registered =
                    crate::win::start_hotkeys(refinery.hotkey.clone(), scan_tx, move || ctx.request_repaint())
                        .then_some(())
                        .ok_or_else(|| "Windows nimmt die Tastatur nicht an".to_string());
            }
            #[cfg(not(windows))]
            drop(scan_tx);
            #[cfg(windows)]
            if args.probe.is_none() {
                let ctx = cc.egui_ctx.clone();
                // A refused registration is not shown at the start, as in 4.12; the start trail says it.
                registered = crate::win::start_hotkeys(hotkey.clone(), hotkey_tx, move || ctx.request_repaint())
                    .then_some(())
                    .ok_or_else(|| "Windows nimmt die Tastatur nicht an".to_string());
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            if args.probe.is_none() {
                let ctx = cc.egui_ctx.clone();
                // Without an X server there is no grab; what became of one shows in the rows.
                registered = hotkey_x11::start(hotkey.clone(), hotkey_tx, move || ctx.request_repaint())
                    .then_some(())
                    .ok_or_else(|| "kein X-Server".to_string());
            }
            #[cfg(target_os = "macos")]
            drop(hotkey_tx);
            // The hotkey of the handle search, the same way as the one of the overlay.
            let search_hotkey = Arc::new(Mutex::new(settings.search_hotkey));
            let (search_tx, search_rx) = mpsc::channel();
            #[cfg_attr(target_os = "macos", allow(unused_mut))]
            let mut search_registered: Result<(), String> = Err("kein System, das es kann".into());
            #[cfg(windows)]
            if args.probe.is_none() {
                let ctx = cc.egui_ctx.clone();
                search_registered =
                    crate::win::start_hotkeys(search_hotkey.clone(), search_tx, move || ctx.request_repaint())
                        .then_some(())
                        .ok_or_else(|| "Windows nimmt die Tastatur nicht an".to_string());
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            if args.probe.is_none() {
                let ctx = cc.egui_ctx.clone();
                search_registered = hotkey_x11::start(search_hotkey.clone(), search_tx, move || ctx.request_repaint())
                    .then_some(())
                    .ok_or_else(|| "kein X-Server".to_string());
            }
            #[cfg(target_os = "macos")]
            drop(search_tx);
            if args.probe.is_none()
                && let Some(choice) = &hotkey_choice
            {
                let scan = scan_choice
                    .as_ref()
                    .filter(|_| versekit_core::refinery_scan::screen::supported())
                    .map(|scan| (scan, scan_registered));
                for line in overlay::hotkey_trail(choice, registered, scan) {
                    versekit_core::trail::step(&line);
                }
                if let Some(paths) = &paths {
                    let search = overlay::HotkeyChoice::read_search(paths);
                    versekit_core::trail::step(&overlay::search_hotkey_trail(&search, search_registered));
                }
            }
            let mut app = OverlayApp::new(Parts {
                hwnd,
                paths,
                files,
                args: args.clone(),
                file_settings,
                game_real,
                forced_game,
                news: news_rx,
                asks,
                game_news: game_rx,
                hotkey,
                hotkey_presses: hotkey_rx,
                refinery,
                search_hotkey,
                search_presses: search_rx,
                control: control_rx,
                start,
                remembered: remembered.is_some(),
                place_known,
                scale,
                points,
                settings_told,
                overview_asked,
                frame_asked,
                afk_asked,
                demand,
            });
            #[cfg(windows)]
            if let Some(file) = &args.probe {
                app.probe = Some(crate::probe::Probe::new(
                    file.clone(),
                    guard_ok,
                    remembered,
                    scale,
                    args.counter.clone(),
                    args.slow.is_some(),
                ));
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            if let Some(file) = &args.probe {
                app.probe = Some(crate::probe_x11::Probe::new(file.clone(), guard_ok, args.counter.clone()));
            }
            // Without an X11 window (Wayland) the overlay is neither placed nor kept on top: said once.
            #[cfg(all(unix, not(target_os = "macos")))]
            if hwnd == 0 {
                let hint = Row::Hint { text: Phrase::new("ov_wayland_eingeschraenkt").into(), contract: None };
                app.rows.push(hint, app.settings.rows);
            }
            // A combination the settings hold but 4.12 would not take is said once; the hotkey is off then.
            if let Some(overlay::HotkeyChoice::Wrong(text)) = &hotkey_choice {
                let hint = Row::Hint {
                    text: Phrase::new("ov_hotkey_ungueltig").with("0", text.as_str()).into(),
                    contract: None,
                };
                app.rows.push(hint, app.settings.rows);
            }
            // The bridge to scmdb.net, only when switched on.
            app.apply_scmdb();
            let _ = (guard_ok, &mut app);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_of_the_overlay_carries_the_class_of_the_program() {
        let viewport = base_viewport();
        assert_eq!(viewport.app_id.as_deref(), Some("Versekit"));
        assert_eq!(viewport.app_id.as_deref(), Some(versekit_core::paths::WM_CLASS));
        // In the taskbar with the icon of the program, not the one of eframe.
        let icon = viewport.icon.expect("an icon");
        assert!(Arc::ptr_eq(&icon, &versekit_ui::window_look::program_icon()));
    }

    /// Open and folded the window has a rounded region: the corners stay round where the alpha of the window is
    /// lost (30.10.: OpenGL on NVIDIA 621.01, the open overlay without a region stood with black square corners).
    /// Folded, the region is the bar and the part beside it is not shown. The window is never shown.
    #[cfg(windows)]
    #[test]
    fn the_open_and_the_folded_overlay_have_a_rounded_region() {
        let mut bench = bench(Args::default(), true, None);
        let hwnd = crate::win::hidden_window();
        assert_ne!(hwnd, 0);
        let ctx = egui::Context::default();
        let app = &mut bench.app;
        app.hwnd = hwnd;
        let open = Rect::new(100, 100, 400, 300);
        crate::win::set_rect(hwnd, open);
        app.canvas = open;
        let (middle, above_bar, on_bar) = ((200, 150), (200, 100), (200, 285));
        let corners = [(0, 0), (399, 0), (0, 299), (399, 299)];
        let open_points = [middle, corners[0], corners[1], corners[2], corners[3]];
        let round = Some(vec![true, false, false, false, false]);
        app.current = open;
        app.follow_region(&ctx, true);
        assert_eq!(crate::win::in_region(hwnd, &open_points), round, "open: rounded region");
        app.current = Rect::new(100, 370, 400, 30);
        app.follow_region(&ctx, true);
        assert_eq!(crate::win::in_region(hwnd, &[above_bar, on_bar]), Some(vec![false, true]), "folded: the bar");
        app.current = open;
        app.follow_region(&ctx, true);
        assert_eq!(crate::win::in_region(hwnd, &open_points), round, "open again: rounded region");
        crate::win::destroy_window(hwnd);
    }

    /// Without the game the region of the window is empty: nothing of it is shown and it takes no clicks. With the
    /// game the visible part comes back at once, and so it does with the switch for showing without the game.
    #[cfg(windows)]
    #[test]
    fn the_region_is_empty_while_the_overlay_waits_hidden() {
        let mut bench = bench(Args::default(), true, None);
        let hwnd = crate::win::hidden_window();
        assert_ne!(hwnd, 0);
        let ctx = egui::Context::default();
        let app = &mut bench.app;
        app.hwnd = hwnd;
        // Shown without the game is the default: the player switched it off here.
        app.settings.overlay_always = false;
        let open = Rect::new(100, 100, 400, 300);
        crate::win::set_rect(hwnd, open);
        (app.canvas, app.current) = (open, open);
        let points = [(200, 150), (20, 20), (380, 280), (200, 20)];
        let everywhere = Some(vec![true; points.len()]);
        app.follow_region(&ctx, true);
        assert_eq!(crate::win::in_region(hwnd, &points), everywhere, "the game runs: shown");
        app.vis.set_game(false);
        app.follow_region(&ctx, false);
        assert!(crate::win::region_is_empty(hwnd), "no game: nothing of the window");
        app.follow_region(&ctx, false);
        assert!(crate::win::region_is_empty(hwnd), "and it stays so");
        app.vis.set_game(true);
        app.follow_region(&ctx, false);
        assert_eq!(crate::win::in_region(hwnd, &points), everywhere, "the game again: the whole part at once");
        app.vis.set_game(false);
        app.follow_region(&ctx, false);
        app.settings.overlay_always = true;
        app.follow_region(&ctx, false);
        assert_eq!(crate::win::in_region(hwnd, &points), everywhere, "the switch: shown without the game");
        crate::win::destroy_window(hwnd);
    }

    /// An overlay without a window, fed through the channels the threads use.
    struct Bench {
        app: OverlayApp,
        news: Sender<News>,
        game: Sender<GameNews>,
        control: Sender<Control>,
        /// A press of the hotkey of the handle search.
        search_key: Sender<()>,
    }

    fn bench(args: Args, real_game: bool, paths: Option<Paths>) -> Bench {
        bench_at(args, real_game, paths, 1.0)
    }

    #[test]
    fn the_hangar_line_is_there_only_while_the_switch_is_on_and_says_what_the_page_says() {
        let mut bench = bench(Args::default(), false, None);
        let start = 1_792_860_059;
        bench.app.hangar_start = Some(start);
        let now = (start + contested::CLOSED - (14 * 60 + 39)) as f64;
        assert_eq!(bench.app.hangar_line(now), None, "off from the start: no line");
        bench.app.settings.hangar_timer = true;
        let line = bench.app.hangar_line(now).expect("on: a line");
        assert_eq!((line.label.as_str(), line.text.as_str()), ("EXHANG", "öffnet in 14:39"));
        assert_eq!(line, contested::overlay_line(Some(start), now), "the one calculation of the page");
        assert_eq!(line.lamps.iter().filter(|lamp| **lamp == contested::Lamp::Green).count(), 4);
        // Without a start the line says so instead of showing times.
        bench.app.hangar_start = None;
        assert_eq!(bench.app.hangar_line(now).map(|l| l.text), Some("keine Startzeit".to_string()));
        bench.app.settings.hangar_timer = false;
        assert_eq!(bench.app.hangar_line(now), None);
    }

    /// The hotkey of the handle search asks the info bar for the cursor while the bar is on the screen; without the
    /// bar a hint row says where the search is.
    #[test]
    fn the_hotkey_of_the_search_asks_the_bar_or_says_where_the_search_is() {
        let mut bench = bench(Args::default(), false, None);
        let hint = language::t("ov_hk_suche_ohne_leiste");
        let hints = |app: &OverlayApp| {
            app.rows.iter().filter(|row| matches!(row, Row::Hint { text, .. } if text.render() == hint)).count()
        };
        bench.app.rows.clear();
        bench.search_key.send(()).unwrap();
        bench.app.take_news();
        assert!(!bench.app.info.key_asked(), "the bar is off: nothing to ask");
        assert_eq!(hints(&bench.app), 1, "the hint row");
        (bench.app.settings.info_bar, bench.app.settings.info_bar_always) = (true, true);
        bench.search_key.send(()).unwrap();
        bench.search_key.send(()).unwrap();
        bench.app.take_news();
        assert!(bench.app.info.key_asked(), "the bar is asked for the cursor");
        assert_eq!(hints(&bench.app), 1, "no second hint");
    }

    /// The refinery jobs come to the open overlay while their switch is on, and the switch takes them away.
    #[test]
    fn the_refinery_jobs_follow_their_switch() {
        use versekit_core::jsonio::Value;
        let mut bench = bench(Args::default(), false, None);
        let ctx = egui::Context::default();
        let ends = contested::now() + 600.0;
        bench.app.refinery_jobs = vec![vec![
            ("ends".to_string(), Value::Float(ends)),
            ("materials".to_string(), Value::Array(vec![Value::str("Gold")])),
        ]];
        bench.app.refinery_read = Some(Instant::now());
        let lines = bench.app.refinery_lines(&ctx);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].0.starts_with("Gold") && !lines[0].1, "{lines:?}");
        bench.app.settings.sections = bench.app.settings.sections.with(Section::Refinery, false);
        assert!(bench.app.refinery_lines(&ctx).is_empty(), "switched off: nothing");
    }

    fn bench_at(args: Args, real_game: bool, paths: Option<Paths>, scale: f32) -> Bench {
        // The language of this thread only: asking for the program's would decide it from the
        // real data of whoever runs the test, and race with the tests that set it.
        language::set_thread_language(language::Choice::De);
        let (news, news_rx) = mpsc::channel();
        let (game, game_rx) = mpsc::channel();
        let (_presses, presses_rx) = mpsc::channel();
        let (control, control_rx) = mpsc::channel();
        let (search_key, search_rx) = mpsc::channel();
        let file_settings = Settings::default();
        let forced_game = Arc::new(ForcedGame::new(args.game.then_some(true)));
        let app = OverlayApp::new(Parts {
            hwnd: 0,
            paths,
            files: None,
            args,
            file_settings,
            game_real: real_game,
            forced_game,
            news: news_rx,
            asks: None,
            game_news: game_rx,
            hotkey: Arc::new(Mutex::new(None)),
            hotkey_presses: presses_rx,
            refinery: crate::refinery::RefineryKey::new(None, mpsc::channel().1),
            search_hotkey: Arc::new(Mutex::new(None)),
            search_presses: search_rx,
            control: Some(control_rx),
            start: Rect::new(10, 20, 540, 390),
            remembered: true,
            place_known: false,
            scale,
            points: 0,
            settings_told: Arc::new(AtomicBool::new(false)),
            overview_asked: Arc::new(AtomicBool::new(false)),
            frame_asked: Arc::new(AtomicBool::new(false)),
            afk_asked: Arc::new(AtomicU8::new(0)),
            demand: Arc::new(Mutex::new(Vec::new())),
        });
        Bench { app, news, game, control, search_key }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vk-overlay-{name}-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir
    }

    // ------------------------------------------------ dummy screens

    use versekit_core::screen::{self, Monitor, Setup};

    fn monitor(area: screen::Area, work: screen::Area) -> Monitor {
        Monitor { area, work }
    }

    /// The Retina laptop screen of the dev machine: 1710 x 1107 points at 2.0, the work area
    /// below the menu bar (35 points) and above the dock (55 points).
    fn laptop() -> Setup {
        Setup { scale: 2.0, primary: 0, monitors: vec![monitor((0, 0, 3420, 2214), (0, 70, 3420, 2034))] }
    }

    /// The laptop with a 2560 x 1440 point screen to its left, 200 points higher: negative places.
    fn laptop_with_screen_to_the_left() -> Setup {
        Setup {
            scale: 2.0,
            primary: 1,
            monitors: vec![
                monitor((-5120, -400, 5120, 2880), (-5120, -400, 5120, 2880)),
                monitor((0, 0, 3420, 2214), (0, 70, 3420, 2034)),
            ],
        }
    }

    /// Linux, scale 1.0: a small screen and the main one to its right, with a panel on top.
    fn linux_two_screens() -> Setup {
        Setup {
            scale: 1.0,
            primary: 1,
            monitors: vec![
                monitor((0, 0, 1280, 1024), (0, 0, 1280, 1024)),
                monitor((1280, 0, 1920, 1080), (1280, 30, 1920, 1050)),
            ],
        }
    }

    /// Dragged, the overlay cannot leave what the screens cover; let go a few pixels before an edge it goes onto it,
    /// wholly onto the screen under its middle, and the window around it moves by as much. A click on the bar (a drag
    /// of at most [`DRAG_SLACK`]) moves nothing.
    #[test]
    fn a_dragged_overlay_stays_on_the_screens_and_snaps_onto_an_edge() {
        let _screens = screen::fixed(linux_two_screens());
        let ctx = egui::Context::default();
        let mut bench = bench(Args::default(), false, None);
        let app = &mut bench.app;
        let start = Rect::new(1500, 300, 400, 300);
        (app.current, app.last_open, app.canvas) = (start, start, start);
        let to = |app: &mut OverlayApp, x: i32, y: i32| {
            app.drag_to(&ctx, Drag::Move, (10, 10), start, Pointer { hidden: false, x: x + 10, y: y + 10 })
        };
        // Far beyond the right and the top edge: held where the screens end (both together are 3200 x 1080).
        to(app, 9000, -500);
        assert_eq!(app.current, Rect::new(2800, 0, 400, 300));
        assert_eq!(app.canvas, app.current, "the window moves along");
        // Let go 9 pixels before the right edge of the main screen: onto it.
        to(app, 2791, 500);
        app.drag_ended(Drag::Move, start, Shown::Open);
        assert_eq!(app.current, Rect::new(2800, 500, 400, 300));
        assert_eq!((app.canvas, app.last_open), (app.current, app.current), "window and open place follow");
        // Over the lower edge of the small screen on the left (1024 high, the main one 1080): let go there, the
        // whole window comes back onto the small screen.
        to(app, 300, 900);
        assert_eq!(app.current, Rect::new(300, 780, 400, 300), "held at the bottom of what the screens cover");
        app.drag_ended(Drag::Move, start, Shown::Open);
        assert_eq!(app.current, Rect::new(300, 724, 400, 300), "wholly on the screen under its middle");
        // A click 2 pixels from the left edge keeps the place.
        let near = Rect::new(2, 500, 400, 300);
        (app.current, app.canvas) = (near, near);
        app.drag_ended(Drag::Move, Rect { x: 0, ..near }, Shown::Open);
        assert_eq!(app.current, near);
    }

    const DEFAULT: (i32, i32) = (880, 544);

    fn remembered(w: i32, h: i32, x: i32, y: i32) -> Option<Remembered> {
        Some(Remembered { w, h, place: Some((x, y)) })
    }

    /// The place from the PC (`709x289+1203+783`, points) is taken on the laptop as it is when its
    /// grab strip lies on the screen; a place off the screen, from a PC with other screens, comes
    /// back to the middle of the main screen with the remembered size.
    #[test]
    fn a_place_from_another_machine_is_kept_where_it_fits_and_brought_back_where_not() {
        let _screens = screen::fixed(laptop());
        let bar = BarSide::Top;
        assert_eq!(
            start_rect(remembered(709, 289, 1203, 783), DEFAULT, bar, 2.0),
            Rect::new(2406, 1566, 1418, 578),
            "points to pixels, the place stays"
        );
        let middle = Rect::new((3420 - 1418) / 2, (2214 - 578) / 2, 1418, 578);
        for (x, y) in [(-1500, 300), (4000, 700), (300, 5000), (-30_000, 15)] {
            assert_eq!(start_rect(remembered(709, 289, x, y), DEFAULT, bar, 2.0), middle, "{x},{y}");
        }
        // The size alone, or nothing: the middle, with the size or the default.
        assert_eq!(start_rect(Some(Remembered { w: 709, h: 289, place: None }), DEFAULT, bar, 2.0), middle);
        assert_eq!(start_rect(None, DEFAULT, bar, 2.0), Rect::new((3420 - 880) / 2, (2214 - 544) / 2, 880, 544));
        // With the bar below, its lower strip is the one to grab: 1100 points down lies
        // in the dock zone below the screen for it, but on the screen for the top bar.
        let low = remembered(709, 289, 600, 1100);
        assert_eq!(start_rect(low, DEFAULT, BarSide::Top, 2.0), Rect::new(1200, 2200, 1418, 578));
        assert_eq!(start_rect(low, DEFAULT, BarSide::Bottom, 2.0), middle);
    }

    /// The same place is on a screen of the laptop with a screen to its left, and off every
    /// screen once that one is gone.
    #[test]
    fn a_place_on_a_second_screen_with_negative_coordinates() {
        let bar = BarSide::Top;
        // Points: a place 2000 left of and 100 above the main screen's corner.
        let on_the_left = remembered(709, 289, -2000, -100);
        {
            let _screens = screen::fixed(laptop_with_screen_to_the_left());
            assert_eq!(start_rect(on_the_left, DEFAULT, bar, 2.0), Rect::new(-4000, -200, 1418, 578));
            // The middle of the *main* screen, the one that is marked, not the first of the list.
            assert_eq!(start_rect(None, DEFAULT, bar, 2.0), Rect::new(1270, 835, 880, 544));
        }
        let _screens = screen::fixed(laptop());
        assert_eq!(start_rect(on_the_left, DEFAULT, bar, 2.0), Rect::new(1001, 818, 1418, 578));
    }

    /// The main screen is the one marked, away from the origin (Windows takes the one at 0, 0 on a real
    /// system; dummy screens answer the same everywhere).
    #[test]
    fn linux_at_scale_one_with_the_main_screen_not_first() {
        let _screens = screen::fixed(linux_two_screens());
        let bar = BarSide::Top;
        // The middle of the main screen (right), on its work area for the size limit.
        assert_eq!(
            start_rect(None, (440, 300), bar, 1.0),
            Rect::new(1280 + (1920 - 440) / 2, (1080 - 300) / 2, 440, 300)
        );
        // A place on the small screen stays, one far away comes back to the main one.
        assert_eq!(start_rect(remembered(600, 500, 100, 100), (440, 300), bar, 1.0), Rect::new(100, 100, 600, 500));
        assert_eq!(
            start_rect(remembered(600, 500, -30_000, 100), (440, 300), bar, 1.0),
            Rect::new(1280 + (1920 - 600) / 2, (1080 - 500) / 2, 600, 500)
        );
        // Too large for the work area of the small screen: shrunk to it.
        let big = start_rect(remembered(5000, 3000, 100, 100), (440, 300), bar, 1.0);
        assert_eq!((big.x, big.y, big.w, big.h), (100, 100, 1280 - 16, 1024 - 16));
    }

    /// Every corner on the work area of the laptop (below the menu bar, above the dock), and on
    /// the work area of the screen the window stands on.
    #[test]
    fn the_corners_stand_on_the_work_area_of_the_screen_the_window_is_on() {
        let _screens = screen::fixed(laptop_with_screen_to_the_left());
        let mut bench = bench_at(Args::default(), false, None, 2.0);
        let app = &mut bench.app;
        let size = (1418, 578);
        let corner = |app: &mut OverlayApp, corner: Corner, from: (i32, i32)| {
            app.settings.layout = Layout { corner, bar: BarSide::Top };
            app.last_open = Rect::new(from.0, from.1, size.0, size.1);
            app.layout_changed();
            app.last_open
        };
        // On the laptop: work area (0, 70, 3420, 2034), a gap of 8.
        assert_eq!(corner(app, Corner::TopLeft, (2000, 900)), Rect::new(8, 78, 1418, 578));
        assert_eq!(corner(app, Corner::TopRight, (2000, 900)), Rect::new(3420 - 1418 - 8, 78, 1418, 578));
        assert_eq!(corner(app, Corner::BottomLeft, (2000, 900)), Rect::new(8, 70 + 2034 - 578 - 8, 1418, 578));
        assert_eq!(corner(app, Corner::BottomRight, (2000, 900)), Rect::new(1994, 1518, 1418, 578));
        // Free keeps the place.
        assert_eq!(corner(app, Corner::Free, (2000, 900)), Rect::new(2000, 900, 1418, 578));
        // On the screen to the left (negative places) the corners are that screen's.
        assert_eq!(corner(app, Corner::TopRight, (-4000, 0)), Rect::new(-1426, -392, 1418, 578));
        assert_eq!(corner(app, Corner::BottomLeft, (-4000, 0)), Rect::new(-5112, 2880 - 400 - 578 - 8, 1418, 578));
    }

    /// One frame without a window on a screen of native scale 2.0; the commands it sends to the
    /// window system.
    fn run_frame(app: &mut OverlayApp, ctx: &egui::Context) -> Vec<ViewportCommand> {
        let info = egui::ViewportInfo { native_pixels_per_point: Some(2.0), ..Default::default() };
        let input = egui::RawInput {
            viewports: std::iter::once((egui::ViewportId::ROOT, info)).collect(),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.frame(ui));
        out.textures_delta.clear();
        out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
    }

    /// The info bar stands over the running game at first and goes and comes with its switch; the lock of the
    /// overlay locks it too and opens it again: one switch for both windows.
    #[test]
    fn the_lock_of_the_overlay_locks_the_info_bar_too() {
        let _screens = screen::fixed(laptop());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut bench = bench_at(Args { game: true, ..Args::default() }, true, None, 2.0);
        let app = &mut bench.app;
        run_frame(app, &ctx);
        assert!(app.info.shown(), "on by default while the game runs");
        app.settings.info_bar = false;
        run_frame(app, &ctx);
        assert!(!app.info.shown(), "switched off: gone");
        app.settings.info_bar = true;
        run_frame(app, &ctx);
        assert!(app.info.shown() && !app.info.passing(), "on, lock open: the bar takes the mouse");
        app.act(&ctx, Action::Lock);
        run_frame(app, &ctx);
        assert!(app.settings.click_through && app.info.passing(), "the lock of the overlay closes the bar too");
        app.act(&ctx, Action::Lock);
        run_frame(app, &ctx);
        assert!(!app.settings.click_through && !app.info.passing(), "and opens it again");
        app.settings.info_bar = false;
        run_frame(app, &ctx);
        assert!(!app.info.shown(), "switched off: gone");
    }

    /// The info bar stays hidden while the game does not run, even with its switch on.
    #[test]
    fn the_info_bar_waits_hidden_while_the_game_does_not_run() {
        let _screens = screen::fixed(laptop());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut bench = bench_at(Args::default(), false, None, 2.0);
        let app = &mut bench.app;
        assert!(app.settings.info_bar && !app.settings.info_bar_always, "the defaults: on, but not without the game");
        run_frame(app, &ctx);
        assert!(!app.info.shown(), "no game: no bar");
        app.settings.info_bar_always = true;
        run_frame(app, &ctx);
        assert!(app.info.shown(), "asked for without the game: the bar shows");
    }

    /// Without the game the overlay waits hidden, and the pointer on its place neither opens it nor gets a hint; with
    /// the game it is there, and with its switch on it is there without the game too.
    #[test]
    fn the_overlay_waits_hidden_while_the_game_does_not_run() {
        let _screens = screen::fixed(laptop());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut bench = popup_bench(versekit_core::overlay::Events::default());
        let app = &mut bench.app;
        app.settings.auto_through = false;
        app.settings.overlay_always = false;
        let on_it = |app: &OverlayApp| Some(Pointer { hidden: false, x: app.current.x + 4, y: app.current.y + 4 });
        for _ in 0..3 {
            run_frame(app, &ctx);
        }
        assert!(app.waits_hidden(), "no game: hidden");
        app.pointer_override = on_it(app);
        run_frame(app, &ctx);
        let now = app.now();
        assert_eq!(app.vis.shown(now), Shown::Strip, "the pointer on its place does not open a hidden overlay");
        app.pointer_override = None;
        app.vis.set_game(true);
        for _ in 0..3 {
            run_frame(app, &ctx);
        }
        assert!(!app.waits_hidden(), "the game runs: it is there");
        app.pointer_override = on_it(app);
        run_frame(app, &ctx);
        let now = app.now();
        assert_eq!(app.vis.shown(now), Shown::Open, "the pointer on the strip opens it");
        app.pointer_override = None;
        app.vis.set_game(false);
        assert!(app.waits_hidden(), "the game ended: hidden again");
        app.settings.overlay_always = true;
        assert!(!app.waits_hidden(), "asked for without the game: it is there");
    }

    /// The size of the overlay makes only the overlay larger, the size of the bar only the bar: the context draws at
    /// the overlay's size, the bar's viewport at its own relative to it, and the open place grows in its corner and
    /// stays on the work area. Back at 100 % both are as before.
    #[test]
    fn each_size_makes_only_its_own_window_larger() {
        use versekit_ui::viewport_zoom::factor_of;
        let _screens = screen::fixed(laptop());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let mut bench = bench_at(Args { game: true, ..Args::default() }, true, None, 2.0);
        let app = &mut bench.app;
        let corner = Layout { corner: Corner::TopRight, bar: BarSide::Top };
        (app.settings.info_bar, app.file_settings.info_bar) = (true, true);
        (app.settings.layout, app.file_settings.layout) = (corner, corner);
        app.last_open = Rect::new(3420 - 700 - 8, 78, 700, 400);
        let sizes = |app: &mut OverlayApp, overlay: i64, bar: i64| {
            app.file_settings.size_percent = overlay;
            app.file_settings.info_bar_size_percent = bar;
            app.adopt_settings();
            app.apply_zoom(&ctx);
            run_frame(app, &ctx);
            run_frame(app, &ctx);
        };
        sizes(app, 150, 200);
        let bar = app.info.id();
        assert_eq!(ctx.zoom_factor(), versekit_ui::window_zoom(app.points, 150), "the overlay at its size");
        assert_eq!(factor_of(&ctx, bar), 200.0 / 150.0, "the bar at its own, over the overlay's");
        assert_eq!(factor_of(&ctx, egui::ViewportId::ROOT), 1.0, "the overlay has nothing of the bar's");
        let open = app.last_open;
        assert_eq!((open.w, open.h), (1050, 600), "the open place grew with it");
        assert_eq!((open.right(), open.y), (3420 - 8, 78), "in its corner");
        assert!(Rect::new(0, 70, 3420, 2034).holds(open), "on the work area: {open:?}");
        // Only the bar larger: the overlay is as large as the main window, the bar twice.
        sizes(app, 100, 200);
        assert_eq!(ctx.zoom_factor(), versekit_ui::window_zoom(app.points, 100));
        assert_eq!(factor_of(&ctx, bar), 2.0);
        assert_eq!((app.last_open.w, app.last_open.h), (700, 400), "back to the size it had");
        sizes(app, 100, 100);
        assert_eq!(factor_of(&ctx, bar), 1.0, "both as the main window again");
    }

    /// A frame without a window: a new corner reaches the window system as the place and size
    /// of the window in the points of egui, which the window system multiplies by the native
    /// scale and the zoom (the text size of the settings is a zoom), on the screen, and nothing
    /// is sent while nothing changed. (Windows moves a window with a handle itself, see `set_window`;
    /// this one has none.)
    #[test]
    fn a_new_corner_moves_the_window_on_the_screen() {
        for zoom in [1.0, 1.45] {
            let _screens = screen::fixed(laptop());
            let ctx = egui::Context::default();
            versekit_ui::install(&ctx);
            ctx.set_zoom_factor(zoom);
            let mut bench = bench_at(Args { game: true, ..Args::default() }, true, None, 2.0);
            let app = &mut bench.app;
            run_frame(app, &ctx);
            let before = run_frame(app, &ctx);
            assert!(!before.iter().any(|c| matches!(c, ViewportCommand::OuterPosition(_))), "{before:?}");
            app.apply_control(Control {
                corner: Some("unten-rechts".into()),
                bar: Some("unten".into()),
                ..Control::default()
            });
            let sent = run_frame(app, &ctx);
            let place =
                sent.iter().find_map(|c| if let ViewportCommand::OuterPosition(p) = c { Some(*p) } else { None });
            let size = sent.iter().find_map(|c| if let ViewportCommand::InnerSize(s) = c { Some(*s) } else { None });
            let (place, size) = (place.expect("the window was moved"), size.expect("and sized"));
            // What the window system makes of it (egui-winit): points times native scale times zoom.
            let ppp = 2.0 * zoom;
            assert_eq!(ctx.pixels_per_point(), ppp, "the scale this test assumes");
            let window = Rect::new(
                (place.x * ppp).round() as i32,
                (place.y * ppp).round() as i32,
                (size.x * ppp).round() as i32,
                (size.y * ppp).round() as i32,
            );
            assert_eq!(window, app.canvas, "zoom {zoom}: what was sent is the window the overlay has");
            let screen = Rect::new(0, 70, 3420, 2034);
            assert!(screen.holds(window), "zoom {zoom}: {window:?} lies on the work area");
            assert_eq!(window.right(), 3420 - 8, "in the corner at the right");
            assert_eq!(window.bottom(), 70 + 2034 - 8, "at the bottom, above the dock");
        }
    }

    /// What the window and the region do in the step of each pointer position of a drag at the
    /// grip, as the frame does them: how often the window got a new size, and per step the part
    /// the overlay shows, the part the region holds and the window.
    fn resize_steps(layout: Layout, counter: Option<&str>, drag: bool) -> (usize, Vec<(Rect, Rect, Rect)>) {
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let args = Args { counter: counter.map(String::from), ..Args::default() };
        let mut bench = bench_at(args, false, None, 2.0);
        let app = &mut bench.app;
        app.settings.layout = layout;
        let start = Rect::new(1200, 600, 700, 400);
        (app.current, app.last_open, app.canvas) = (start, start, start);
        if drag {
            app.drag = Some((Drag::Resize, (0, 0), start));
        } else {
            // Without a drag the window is the whole screen, so that only the region differs.
            app.canvas = Rect::new(0, 0, 3420, 2214);
        }
        let shown = start.relative_to(app.canvas.x, app.canvas.y);
        (app.painted, app.region) = (shown, Some(shown));
        let (mut resizes, mut steps) = (0, Vec::new());
        // Out, back in, far out, in beyond where it began, and out to the left and up.
        for (x, y) in [(1960, 1060), (2400, 1300), (2000, 1100), (3300, 2000), (1800, 900), (1100, 500)] {
            app.drag_to(&ctx, Drag::Resize, (0, 0), start, Pointer { hidden: false, x, y });
            let canvas = if drag { app.canvas_needed(&ctx, Shown::Open) } else { app.canvas };
            let changed = canvas != app.canvas;
            if changed {
                resizes += 1;
                app.apply_canvas(canvas);
            }
            let visible = app.current.relative_to(app.canvas.x, app.canvas.y);
            let region = app.region_wanted(&ctx, changed, visible).unwrap_or(visible);
            app.region = Some(region);
            steps.push((visible, region, app.canvas));
        }
        (resizes, steps)
    }

    /// Resized at the grip, the window keeps its size once it spans what the drag reaches and
    /// the region has the size of the pointer's step already. Counter checks: a window sized to
    /// the part resizes on every step; the same steps outside a drag give the part up late.
    #[test]
    fn a_resize_at_the_grip_moves_the_region_and_leaves_the_window_its_size() {
        let _screens = screen::fixed(laptop());
        let corners = [Corner::TopLeft, Corner::BottomRight, Corner::Free];
        for corner in corners {
            let layout = Layout { corner, bar: BarSide::Top };
            let (resizes, steps) = resize_steps(layout, None, true);
            assert_eq!(resizes, 1, "{corner:?}: sized once, at the first step, not with every step");
            let window = steps[0].2;
            for (n, (visible, region, canvas)) in steps.iter().enumerate() {
                assert_eq!(*canvas, window, "{corner:?} step {n}: the window keeps its size");
                assert_eq!(region, visible, "{corner:?} step {n}: the region has the part of this step");
                assert!(Rect::new(0, 0, window.w, window.h).holds(*visible), "{corner:?} step {n}: inside the window");
            }
            let (resizes, _) = resize_steps(layout, Some("groesse"), true);
            assert!(resizes > 1, "{corner:?}: the counter check sizes the window with more than one step");
            let (_, late) = resize_steps(layout, None, false);
            assert!(
                late.iter().any(|(visible, region, _)| region != visible),
                "{corner:?}: outside a drag the region gives a part up late"
            );
        }
    }

    /// A place sent to the window system takes a moment to arrive: meanwhile the window standing where it
    /// was is no move of the system; once it is late, or nothing was sent, a difference is one.
    #[test]
    fn a_place_on_its_way_is_no_move_of_the_system() {
        let canvas = Rect::new(500, 300, 400, 200);
        let old = Rect::new(100, 80, 400, 200);
        assert_eq!(system_move(old, canvas, Some(Duration::from_millis(200))), None, "on its way");
        assert_eq!(system_move(old, canvas, Some(PLACE_ARRIVES)), Some((-400, -220)), "too late: the system placed it");
        assert_eq!(system_move(old, canvas, None), Some((-400, -220)), "nothing sent: the system moved it");
        assert_eq!(system_move(canvas, canvas, None), None, "where it was put");
        assert_eq!(system_move(Rect::default(), canvas, None), None, "nothing known about the window");
    }

    /// A frame follows a move of the system only once the place it sent had its time to arrive.
    #[test]
    fn the_frame_follows_the_system_only_after_the_place_sent_arrived_or_is_late() {
        let mut bench = bench(Args::default(), false, None);
        let app = &mut bench.app;
        let start = (app.current, app.last_open, app.canvas);
        // Without a window the system tells no place: nothing moves.
        app.place_sent = Some(Instant::now());
        app.follow_system_move();
        assert_eq!((app.current, app.last_open, app.canvas), start);
        assert!(app.place_sent.is_some(), "still on its way");
    }

    #[test]
    fn the_window_commands_are_points() {
        let [place, size] = window_commands(Rect::new(2406, 1566, 1418, 578), 2.0);
        assert!(matches!(place, ViewportCommand::OuterPosition(p) if p == egui::pos2(1203.0, 783.0)));
        assert!(matches!(size, ViewportCommand::InnerSize(s) if s == egui::vec2(709.0, 289.0)));
        let [place, _] = window_commands(Rect::new(-250, -10, 100, 100), 1.25);
        assert!(matches!(place, ViewportCommand::OuterPosition(p) if p == egui::pos2(-200.0, -8.0)));
    }

    #[test]
    fn a_control_request_changes_what_the_running_overlay_does() {
        let mut bench = bench(Args { game: true, ..Args::default() }, false, None);
        let app = &mut bench.app;
        assert!(app.vis.game(), "started with the game counted as running");
        assert_eq!(app.update, None);
        app.apply_control(Control {
            game: Some(false),
            update: Some("5.0.1".into()),
            corner: Some("unten-rechts".into()),
            bar: Some("unten".into()),
            mode: Some("popup".into()),
            seconds: Some(9),
            ..Control::default()
        });
        assert!(!app.vis.game(), "the game switch takes effect at once");
        assert_eq!(app.update.as_deref(), Some("5.0.1"));
        assert_eq!(app.settings.layout.corner, Corner::BottomRight);
        assert_eq!(app.settings.layout.bar, BarSide::Bottom);
        assert_eq!(app.settings.mode, Mode::Popup);
        assert_eq!(app.settings.popup_seconds, 9);
        // A game found by the watch does not undo what was forced.
        bench.game.send(GameNews::Running(true)).expect("sent");
        bench.app.take_news();
        assert!(!bench.app.vis.game(), "the forced state wins over the system");
        // Every key gone: back to what the data say, and the real game counts again.
        bench.app.apply_control(Control::default());
        assert!(bench.app.vis.game(), "the real game state is back");
        assert_eq!(bench.app.update, None);
        assert_eq!(bench.app.settings, Settings::default());
    }

    #[test]
    fn the_newest_request_in_the_channel_is_taken_in_the_frame() {
        let mut bench = bench(Args::default(), false, None);
        bench.control.send(Control { update: Some("5.0.1".into()), ..Control::default() }).expect("sent");
        bench.control.send(Control { update: Some("5.2.0".into()), ..Control::default() }).expect("sent");
        bench.app.take_control();
        assert_eq!(bench.app.update.as_deref(), Some("5.2.0"), "the newest request wins");
        bench.app.take_control();
        assert_eq!(bench.app.update.as_deref(), Some("5.2.0"), "nothing new, nothing changes");
    }

    #[test]
    fn a_change_made_here_and_not_yet_read_back_survives_a_control_request() {
        let mut bench = bench(Args::default(), false, None);
        // The user locked the overlay; the file thread has not read the setting back yet.
        bench.app.settings.click_through = true;
        bench.app.apply_control(Control { mode: Some("popup".into()), ..Control::default() });
        assert_eq!(bench.app.settings.mode, Mode::Popup);
        assert!(bench.app.settings.click_through, "the lock is not undone by a control request");
    }

    #[test]
    fn a_place_is_taken_when_asked_for_and_asked_for_again_only_with_a_new_number() {
        let mut bench = bench(Args::default(), false, None);
        let app = &mut bench.app;
        let ask = |nr| Control {
            place: Some(Place::Given((300, 200, Some((100, 120))))),
            place_nr: nr,
            ..Control::default()
        };
        app.apply_control(ask(1));
        assert_eq!(app.last_open, Rect::new(100, 120, 300, 200), "the overlay went there");
        // The user dragged it away; a change of another switch must not pull it back.
        app.last_open = Rect::new(5, 6, 300, 200);
        app.apply_control(Control { game: Some(true), ..ask(1) });
        assert_eq!(app.last_open, Rect::new(5, 6, 300, 200));
        // The same place, asked for again with a new number: it goes there again.
        app.apply_control(ask(2));
        assert_eq!(app.last_open, Rect::new(100, 120, 300, 200));
    }

    #[test]
    fn the_remembered_place_is_taken_back_from_the_data() {
        let root = scratch("remembered");
        std::fs::create_dir_all(root.join(versekit_core::paths::COPY_DATA_FOLDER)).expect("data folder");
        // A made-up computer: nothing of the shell the check runs in counts.
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &root.to_string_lossy());
        let paths = Paths::for_copy(host, &root.to_string_lossy()).expect("a copy");
        assert!(overlay::save_place(&paths, Rect::new(40, 50, 410, 330)));
        let mut bench = bench(Args::default(), false, Some(paths));
        bench
            .app
            .apply_control(Control { place: Some(Place::Given((300, 200, Some((100, 120))))), ..Control::default() });
        assert_eq!(bench.app.last_open, Rect::new(100, 120, 300, 200));
        bench.app.apply_control(Control { place: Some(Place::Remembered), place_nr: 1, ..Control::default() });
        assert_eq!(bench.app.last_open, Rect::new(40, 50, 410, 330), "back where the data say");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// "Die Einheit sucht: …" stands under the signature line when the unit wants one of the ores behind it, as the
    /// main window handed the demand over; no signature line, no wanted ore or no demand: no line.
    #[test]
    fn the_overlay_names_what_the_unit_wants_of_the_ores_behind_the_signature() {
        use versekit_core::signature_window::Text;
        let mut bench = bench(Args::default(), false, None);
        let app = &mut bench.app;
        let text = Text { material: "2× Agricium".into(), number: "8,420".into(), known: true };
        let handed = demand::handover(&[Material {
            name: "Agricium".into(),
            raw: vec!["Agricium (Ore)".into()],
            min_quality: 650,
            amount: 40.0,
            unit: "SCU".into(),
            source: String::new(),
        }]);
        assert_eq!(app.demand_line(), None, "nothing read yet");
        app.scanner.show_read(text.clone(), vec!["Agricium".into()], true);
        assert_eq!(app.demand_line(), None, "nothing handed over yet");
        *app.demand.lock().unwrap() = demand::taken_over(&handed);
        assert_eq!(app.demand_line().as_deref(), Some("Die Einheit sucht: 40 SCU Agricium (ab Q 650)"));
        app.scanner.show_read(text.clone(), vec!["Quantainium".into()], true);
        assert_eq!(app.demand_line(), None, "an ore the unit does not want");
        *app.demand.lock().unwrap() = demand::taken_over(&demand::handover(&[]));
        app.scanner.show_read(text, vec!["Agricium".into()], true);
        assert_eq!(app.demand_line(), None, "the demand went away (switched off, disconnected)");
    }

    /// „In die Mitte holen“ while the overlay runs: the main window forgets size and place and says so, and the
    /// running overlay stands in the middle of the main screen in the default size a start without them measures.
    #[test]
    fn a_forgotten_place_brings_the_running_overlay_to_the_middle_in_its_default_size() {
        let _screens = screen::fixed(laptop());
        let ctx = egui::Context::default();
        versekit_ui::install(&ctx);
        let root = scratch("mitte");
        std::fs::create_dir_all(root.join(versekit_core::paths::COPY_DATA_FOLDER)).expect("data folder");
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &root.to_string_lossy());
        let main_window = Paths::for_copy(host.clone(), &root.to_string_lossy()).expect("a copy");
        assert!(overlay::save_place(&main_window, Rect::new(1500, 900, 600, 400)));
        let args = || Args { game: true, counter: Some("im_bildfaden".into()), ..Args::default() };
        let mut running = bench_at(args(), true, Paths::for_copy(host.clone(), &root.to_string_lossy()), 2.0);
        let told = |bench: &Bench| bench.app.settings_told.store(true, Ordering::Relaxed);
        running.app.place_known = true;
        told(&running);
        run_frame(&mut running.app, &ctx);
        let moved = running.app.last_open;
        assert!(versekit_core::overlay::forget_place(&main_window));
        told(&running);
        run_frame(&mut running.app, &ctx);
        run_frame(&mut running.app, &ctx);
        // A start without a remembered size, for the default size the text heights give.
        let fresh_ctx = egui::Context::default();
        versekit_ui::install(&fresh_ctx);
        let mut fresh = bench_at(args(), true, None, 2.0);
        fresh.app.default_pending = true;
        run_frame(&mut fresh.app, &fresh_ctx);
        run_frame(&mut fresh.app, &fresh_ctx);
        let back = running.app.last_open;
        assert_ne!(back, moved, "the running overlay followed");
        assert_eq!(back, fresh.app.last_open, "the default size in the middle of the main screen");
        assert_eq!(back, centered_on_main((back.w, back.h)));
        assert!(running.app.place_changed.is_none(), "nothing remembered until the player moves it");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Folded by hand at the last run: it starts folded, also while the game runs; the fold arrow writes what the
    /// player left it in (`eingeklappt`, as 4.12).
    #[test]
    fn the_overlay_starts_as_the_player_left_it_by_hand() {
        let root = scratch("eingeklappt");
        std::fs::create_dir_all(root.join(versekit_core::paths::COPY_DATA_FOLDER)).expect("data folder");
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &root.to_string_lossy());
        let paths = Paths::for_copy(host, &root.to_string_lossy()).expect("a copy");
        assert!(paths.set_setting(keys::FOLDED, Value::Bool(true)));
        let reading = Paths::new(paths.host.clone());
        let args = Args { counter: Some("im_bildfaden".into()), ..Args::default() };
        let mut folded = bench(args, true, Some(paths));
        let now = folded.app.now();
        assert_eq!(folded.app.vis.shown(now), Shown::Bar, "folded, although the game runs");
        let ctx = egui::Context::default();
        folded.app.act(&ctx, Action::Fold);
        assert_eq!(folded.app.vis.shown(now), Shown::Open);
        assert_eq!(folded_by_hand(&reading), Some(false), "opened by hand is remembered too");
        // Nothing written: it follows the game.
        let fresh = scratch("eingeklappt-nie");
        std::fs::create_dir_all(fresh.join(versekit_core::paths::COPY_DATA_FOLDER)).expect("data folder");
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &fresh.to_string_lossy());
        let mut open = bench(Args::default(), true, Paths::for_copy(host, &fresh.to_string_lossy()));
        let now = open.app.now();
        assert_eq!(open.app.vis.shown(now), Shown::Open, "with the game, as without a remembered state");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&fresh);
    }

    #[test]
    fn a_find_is_judged_by_the_switches_written_before_the_line_that_caused_it() {
        let dir = scratch("race");
        let file = dir.join(overlay_control::FILE);
        let mut bench = bench(Args { game: true, ..Args::default() }, false, None);
        bench.app.control_path = Some(file.to_string_lossy().into_owned());
        // The switches are written first, then the line: the find arrives at once after them.
        let control = Control { game: Some(true), mode: Some("popup".into()), seconds: Some(9), ..Control::default() };
        overlay_control::write(&file.to_string_lossy(), &control).expect("written");
        bench.news.send(News::Found).expect("sent");
        bench.app.take_news();
        assert_eq!(bench.app.settings.mode, Mode::Popup, "the mode was read before the find was judged");
        let now = bench.app.now();
        assert!(
            matches!(bench.app.vis.peek(now), Some(versekit_core::overlay::Peek::Closes(left)) if left > 8.0),
            "the pop-up opens for the seconds just written"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn popup_bench(events: versekit_core::overlay::Events) -> Bench {
        let mut bench = bench(Args::default(), false, None);
        bench.app.vis.set_mode(Mode::Popup);
        bench.app.settings.mode = Mode::Popup;
        bench.app.settings.events = events;
        bench
    }

    fn only(event: Event) -> versekit_core::overlay::Events {
        versekit_core::overlay::Events::default().with(Event::Blueprint, false).with(event, true)
    }

    #[test]
    fn a_chosen_event_opens_the_waiting_strip_and_one_not_chosen_does_not() {
        for event in [Event::Scanner, Event::Contract, Event::Refinery, Event::Catalog] {
            let mut on = popup_bench(only(event));
            on.news.send(News::Opens(event)).expect("sent");
            on.app.take_news();
            let now = on.app.now();
            assert_eq!(on.app.vis.shown(now), Shown::Open, "{event:?} chosen: opens");
            let mut off = popup_bench(versekit_core::overlay::Events::default());
            off.news.send(News::Opens(event)).expect("sent");
            off.app.take_news();
            let now = off.app.now();
            assert_eq!(off.app.vis.shown(now), Shown::Strip, "{event:?} not chosen: stays a strip");
        }
    }

    #[test]
    fn the_blueprint_opens_only_while_it_is_chosen_and_the_other_events_stay_shut_by_default() {
        let mut bench = popup_bench(versekit_core::overlay::Events::default());
        bench.news.send(News::Found).expect("sent");
        bench.app.take_news();
        let now = bench.app.now();
        assert_eq!(bench.app.vis.shown(now), Shown::Open, "the default: the blueprint opens, as in 4.12");
        let mut off = popup_bench(only(Event::Scanner));
        off.news.send(News::Found).expect("sent");
        off.app.take_news();
        let now = off.app.now();
        assert_eq!(off.app.vis.shown(now), Shown::Strip, "the blueprint is switched off");
    }

    #[test]
    fn an_event_opens_the_folded_bar_of_the_always_on_mode_too() {
        let mut bench = bench(Args::default(), false, None);
        bench.app.settings.events = only(Event::Contract);
        let now = bench.app.now();
        assert_eq!(bench.app.vis.shown(now), Shown::Bar);
        bench.news.send(News::Opens(Event::Contract)).expect("sent");
        bench.app.take_news();
        let now = bench.app.now();
        assert_eq!(bench.app.vis.shown(now), Shown::Open);
    }

    #[test]
    fn several_events_in_a_row_share_one_clock() {
        let mut bench = popup_bench(versekit_core::overlay::Events::default().with(Event::Scanner, true));
        bench.news.send(News::Found).expect("sent");
        bench.news.send(News::Opens(Event::Scanner)).expect("sent");
        bench.app.take_news();
        let now = bench.app.now();
        assert!(
            matches!(bench.app.vis.peek(now), Some(versekit_core::overlay::Peek::Closes(left)) if left > 5.0),
            "one open time for both, started over by the second"
        );
    }

    #[test]
    fn a_scanner_event_shows_the_line_of_the_reading_for_the_open_time_only() {
        let mut bench = popup_bench(only(Event::Scanner));
        bench.news.send(News::Opens(Event::Scanner)).expect("sent");
        bench.app.take_news();
        let now = bench.app.now();
        assert!(now < bench.app.scanner_line_until, "the line is forced while the overlay stands open");
        assert!(bench.app.scanner_line_until <= now + bench.app.popup_seconds() + 0.5);
        let mut other = popup_bench(only(Event::Contract));
        other.news.send(News::Opens(Event::Contract)).expect("sent");
        other.app.take_news();
        assert_eq!(other.app.scanner_line_until, 0.0, "another event leaves the line to the player's choice");
        let mut off = popup_bench(versekit_core::overlay::Events::default());
        off.news.send(News::Opens(Event::Scanner)).expect("sent");
        off.app.take_news();
        assert_eq!(off.app.scanner_line_until, 0.0, "not chosen: no line");
    }

    #[test]
    fn a_change_of_the_chosen_events_is_taken_while_running() {
        let mut bench = popup_bench(versekit_core::overlay::Events::default());
        let mut fresh = bench.app.settings.clone();
        fresh.events = only(Event::Refinery);
        bench.app.adopt(fresh);
        assert!(bench.app.settings.events.contains(Event::Refinery));
        assert!(!bench.app.settings.events.contains(Event::Blueprint));
        bench.news.send(News::Opens(Event::Refinery)).expect("sent");
        bench.app.take_news();
        let now = bench.app.now();
        assert_eq!(bench.app.vis.shown(now), Shown::Open);
    }

    #[test]
    fn the_control_watch_hands_over_every_new_content_of_the_file() {
        let dir = scratch("watch");
        let file = dir.join(overlay_control::FILE).to_string_lossy().into_owned();
        let receiver = start_control_watch(file.clone(), || {});
        assert!(receiver.recv_timeout(Duration::from_millis(400)).is_err(), "no file, no request");
        let first = Control { update: Some("5.0.1".into()), ..Control::default() };
        overlay_control::write(&file, &first).expect("written");
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).expect("a request"), first);
        let second = Control { game: Some(false), ..Control::default() };
        overlay_control::write(&file, &second).expect("rewritten");
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).expect("a request"), second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The frame of a resize draws in a child: the rows keep their scroll
    /// position and the grip its drag only with the same widget ids.
    #[test]
    fn the_sized_child_keeps_the_widget_ids() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let plain = ui.make_persistent_id("overlay-zeilen");
            let rect = egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(300.0, 40.0));
            let sized = sized_child(ui, rect);
            assert_eq!(sized.make_persistent_id("overlay-zeilen"), plain);
            assert_eq!(sized.max_rect(), rect);
            assert_eq!(sized.clip_rect(), rect);
        });
        output.textures_delta.clear();
    }

    /// A given program gets exactly its own arguments, never the data folder;
    /// the preview gets the data folder; a relative program lies next to the overlay.
    #[cfg(windows)]
    #[test]
    fn the_button_starts_the_given_program_with_its_arguments() {
        use std::path::{Path, PathBuf};
        let own = vec!["--quelle".to_string(), "test".to_string()];
        let folder = Some(Path::new(r"C:\Kit"));
        let preview = PathBuf::from(r"C:\Kit\vorschau.exe");
        let given = main_command(Some(Path::new(r"D:\Werkzeug\haupt.exe")), &own, Some(r"E:\Kopie"), None, folder);
        assert_eq!(given, Some((PathBuf::from(r"D:\Werkzeug\haupt.exe"), own.clone())));
        let relative = main_command(Some(Path::new("haupt.exe")), &own, None, None, folder);
        assert_eq!(relative.map(|c| c.0), Some(PathBuf::from(r"C:\Kit\haupt.exe")));
        let plain = main_command(None, &[], Some(r"E:\Kopie"), Some(preview.clone()), folder);
        assert_eq!(plain, Some((preview, vec!["--daten".to_string(), r"E:\Kopie".to_string()])));
        assert_eq!(main_command(None, &own, None, None, folder), None);
    }

    /// The teaching opens the small window: the program of the main window with the one switch exchanged.
    #[cfg(windows)]
    #[test]
    fn the_teaching_button_starts_the_small_window_with_the_arguments_of_the_main_window() {
        let args = Args {
            probe: Some("probe.txt".into()),
            main_program: Some(std::path::PathBuf::from(r"D:\Kit\versekit.exe")),
            main_args: vec!["--hauptfenster".into(), "--daten".into(), r"E:\Kopie".into()],
            ..Args::default()
        };
        let mut bench = bench(args, false, None);
        bench.app.learn_scanner();
        assert_eq!(
            bench.app.launched,
            Some((
                std::path::PathBuf::from(r"D:\Kit\versekit.exe"),
                vec!["--anlernen".to_string(), "--daten".to_string(), r"E:\Kopie".to_string()]
            ))
        );
    }

    /// What the thread of the updates tells is taken over: the version on offer (and its going) and the
    /// sentences for the status line, the later one standing.
    #[test]
    fn what_the_update_thread_tells_is_taken_over_by_the_overlay() {
        let mut bench = bench(Args::default(), false, None);
        let told = crate::Told {
            update: Some(Some("5.0.1".into())),
            say: vec![("up_auto_wartet".into(), "5.0.1".into()), ("up_auto_laeuft".into(), "5.0.1".into())],
        };
        bench.app.apply_told(told);
        assert_eq!(bench.app.update.as_deref(), Some("5.0.1"));
        assert_eq!(bench.app.status.render(), language::tf("up_auto_laeuft", &[("0", "5.0.1".into())]));
        // Nothing newer any more: the version goes, a status line stays as it was.
        bench.app.apply_told(crate::Told { update: Some(None), say: Vec::new() });
        assert_eq!(bench.app.update, None);
        bench.app.apply_told(crate::Told::default());
        assert_eq!(bench.app.update, None, "nothing told, nothing changes");
    }

    /// The bell is there while a version is on offer, names it, stands in the accent, and opens „Update & Über“.
    #[test]
    fn the_bell_stands_in_the_bar_with_a_version_on_offer_and_opens_the_page_about_it() {
        let args = Args {
            probe: Some("probe.txt".into()),
            main_program: Some(std::path::PathBuf::from("versekit-test.exe")),
            ..Args::default()
        };
        let mut bench = bench(args, false, None);
        let ctx = egui::Context::default();
        bench.app.step_anti_afk(&ctx);
        let plain = bench.app.extras.iter().find(|e| e.id == BELL).expect("the bell without a version too");
        assert_eq!(
            (plain.tone, plain.meaning.as_str()),
            (view::Tone::Normal, language::t("hinweis_versionen").as_str()),
            "like the other buttons, the history of versions"
        );
        bench.app.update = Some("5.0.1".into());
        bench.app.step_anti_afk(&ctx);
        let bell = bench.app.extras.iter().find(|e| e.id == BELL).expect("the bell");
        assert_eq!(bell.icon, "bell");
        assert_eq!(bell.tone, view::Tone::Lit);
        assert_eq!(bell.meaning, language::tf("hf_hinweis_update_da", &[("0", "5.0.1".into())]));
        assert_eq!(bell.action, Action::Extra(BELL));
        // The version is gone (installed, or the list is empty): the bell turns plain again.
        let mut gone = bench.app.update.take();
        bench.app.step_anti_afk(&ctx);
        let plain = bench.app.extras.iter().find(|e| e.id == BELL).expect("the bell stays");
        assert_eq!(plain.tone, view::Tone::Normal, "plain without a version");
        bench.app.update = gone.take();
        bench.app.step_anti_afk(&ctx);
        bench.app.act(&ctx, Action::Extra(BELL));
        assert!(matches!(bench.app.pending_page, Some((Signal::OpenAbout, _))), "the page about the update opens");
        assert!(bench.app.launched.is_some(), "the main window is started");
    }

    /// The end of the overlay asks every main window of the run to close, whoever started it and on screen or not:
    /// a window of a test process standing in for one the player opened besides. Hidden windows of this test, never
    /// shown, taking no focus; the request is a message nobody takes, since no window of this test serves a role.
    #[cfg(windows)]
    #[test]
    fn the_end_of_the_overlay_asks_every_main_window_of_the_run_to_close() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use versekit_core::window_signal;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let make = || {
            // SAFETY: a plain window of a system class without parent, never shown; destroyed below.
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!window.is_null());
            window as isize
        };
        let key = "pruefung-overlay-ende";
        let (first, second) = (make(), make());
        let asked = std::sync::Arc::new(AtomicUsize::new(0));
        for window in [first, second] {
            let count = std::sync::Arc::clone(&asked);
            assert!(window_signal::serve_as(window, Role::Main, key, move |signal| {
                if signal == Signal::Close {
                    count.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        let found = window_signal::find_all_as(Role::Main, key);
        assert_eq!(found.len(), 2, "both main windows of the run, whoever opened them");
        let told = found.iter().filter(|&&hwnd| window_signal::send(hwnd, Signal::Close)).count();
        assert_eq!(told, 2, "both take the request");
        // The requests sit in the queue of this thread until it takes its messages.
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW};
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                DispatchMessageW(&msg);
            }
        }
        assert_eq!(asked.load(Ordering::Relaxed), 2, "both windows were asked to close");
        assert_eq!(closing_line(2, 2), "Overlay endet: 2 Hauptfenster gefunden, 2 zum Schließen gebeten");
        // SAFETY: the windows made above.
        unsafe {
            DestroyWindow(first as _);
            DestroyWindow(second as _);
        }
    }
}
