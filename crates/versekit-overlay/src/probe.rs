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

//! The self-check of the real window (`--probe <file>`), meant for the
//! invisible desktop: it goes through the states by itself, checks the
//! window style and photographs the window to see that content is drawn,
//! writes one line per check and closes.
//!
//! The central check: wait as a strip, wait, come back, the rows are on
//! screen again. A counter check (`--gegenprobe leer`) loses the rows after
//! coming back and must turn it red. With `--daten` it waits for the watcher
//! and checks that no thread wrote outside the copy (`--gegenprobe schreiben`
//! tries it).
//!
//! Then it folds and unfolds with the bar below and records every frame:
//! the window keeps its size (`--gegenprobe groesse` sizes it to the
//! visible part on every fold), the frame of a change is painted in the new
//! state (`--gegenprobe altes_mass` lays it out in the old one) and stays
//! transparent beside the visible part, and the window region holds every
//! part painted less than [`STILL_SHOWN`] ago (`--gegenprobe sofort_klein`
//! gives up the old part with the next frame, `--gegenprobe spaet_gross`
//! takes the new one a frame late). Folded, the region is the bar and a
//! click beside it reaches the window below (`--gegenprobe ohne_region`
//! sets no region). The client area covers the whole window, without the
//! strip of frame winit keeps at the top (`--gegenprobe rahmen` keeps it:
//! inside a region the system draws it as a bright line). Every resize of the run comes after the layout of its
//! frame (`--gegenprobe frueh` resizes first). Whether the system of a real
//! screen shows a stretched picture cannot be seen here: the invisible
//! desktop is not composed. The checks show that no step asks it to.
//!
//! In every frame of that run DWM must hold the request for round corners,
//! from the first frame on, and the settled window region must leave out the
//! corner pixels of the visible part, open, folded and waiting, with clicks
//! passing and not (`--gegenprobe eckig` asks DWM for nothing and keeps the
//! region square); the painted frames of folding leave those pixels
//! transparent. Open, the region keeps the corners round where the alpha of
//! the window is lost (`--gegenprobe offen_ohne_bereich` leaves the open
//! window without one). How DWM composes the corners on a real screen is not seen
//! here either. DWM must take every request to draw no border of its own
//! (`--gegenprobe rand_abgelehnt` makes the request one it refuses); the
//! border it holds cannot be read back, so its answers are judged.
//!
//! Then the button "Übersicht öffnen": a hidden window of the probe stands
//! in for the main window and must get the signal; without it the button
//! must start the main program (only noted, nothing is started) or say that
//! it is missing. `--gegenprobe uebersicht` makes the button do nothing.
//! The signal of a second start must reach that window too (`--gegenprobe
//! zweiter_start` passes it over), closing the overlay must close it
//! (`--gegenprobe hauptfenster_bleibt`), and with `--hauptprogramm` the
//! button starts that program with exactly its `--hauptprogramm-arg`
//! (`--gegenprobe hauptprogramm` starts the preview instead).
//! Then the bar with three placeholder extras, narrow and wide
//! (`--gegenprobe menue` puts every extra in the bar), and last the
//! pointer resting on the lock shows the lock's own hint (`--gegenprobe
//! ziehhinweis` lays the area that moves the window under the buttons).
//!
//! With `--daten` and `--archiv` the finds read the archive given
//! (`--gegenprobe archiv` passes it over).
//!
//! With `--daten` a drag by hand must free the corner and tell the main
//! window (`--gegenprobe frei`), and a change the main window reports must
//! arrive at once, the lock switched off among them (`--gegenprobe
//! einstellung` ignores the report). A locked overlay takes clicks on the
//! lock under the visible pointer (`--gegenprobe schloss`), tells egui where
//! the pointer stands (`--gegenprobe kein_anstoss`), and a click there opens
//! the lock. With `--langsam <ms>` the frames must go on while every file
//! access takes that long (`--gegenprobe im_bildfaden` does them in the
//! drawing thread).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;
use egui::ViewportCommand;
use versekit_core::game_start;
use versekit_core::jsonio::Value;
use versekit_core::language::{t, tf};
use versekit_core::live_contracts::Line as ContractLine;
use versekit_core::overlay::{self, BarSide, Corner, EDGE, Layout, Message, Mode, Rect, Remembered, Row, Shown, keys};
use versekit_core::window_signal::{self, Role, Signal};
use versekit_core::{screen, writeguard};
use versekit_ui::overlay::{Action, Drag, Extra};

use windows_sys::Win32::UI::Shell::{NIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY};
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_COMMAND, WM_LBUTTONUP};

use crate::app::{OverlayApp, work_at};
use crate::{tray, win};

/// Pixels brighter than this in every channel count as text in the
/// foreground color; the bar, the status and the icons stay below it.
const BRIGHT: u32 = 200;
/// At least this many such pixels: rows are on screen. The check
/// `ohne_zeilen_kaum_textpunkte` shows that an empty overlay stays below.
const INK: usize = 60;

/// A channel further off than this counts the pixel as different.
const PIXEL_OFF: u32 = 48;
/// A picture matches its resting picture with less than this share of
/// different pixels; a stretched or half-drawn one is far above.
const SHARE_OFF: f64 = 0.03;
/// How long the system may still show a painted picture, in seconds: the
/// window region must hold what it painted for that long.
const STILL_SHOWN: f64 = 0.1;
/// The deliberate faults `--gegenprobe` knows.
const COUNTERS: [&str; 44] = [
    "eckig",
    "ohne_dcomp",
    "offen_ohne_bereich",
    "rand_abgelehnt",
    "archiv",
    "rakete_tot",
    "starter_vergessen",
    "menue_ohne_start",
    "menue_ohne_seite",
    "ohne_rueckgaengig",
    "seite_vergessen",
    "ohne_icon_freigeben",
    "symbol_fokus",
    "ohne_symbol_entfernen",
    "ohne_neuanlage",
    "ohne_systemmeldung",
    "ohne_merkliste",
    "ohne_raffinerie",
    "ohne_katalog_wache",
    "ohne_einst_hinweis",
    "ohne_leiste",
    "ausblenden_klemmt",
    "stand_haengt",
    "ziehhinweis",
    "hauptprogramm",
    "hauptfenster_bleibt",
    "zweiter_start",
    "leer",
    "fokus",
    "rahmen",
    "schreiben",
    "altes_mass",
    "frueh",
    "groesse",
    "ohne_region",
    "sofort_klein",
    "spaet_gross",
    "uebersicht",
    "einstellung",
    "frei",
    "menue",
    "im_bildfaden",
    "schloss",
    "kein_anstoss",
];

/// The deliberate faults that act in the run with data and in the one without, each with checks of its
/// own (the bar of the contracts, the hiding by hand and taking it back: the sample contents in the one,
/// the log of the copy in the other).
const EITHER: [&str; 3] = ["ohne_leiste", "ausblenden_klemmt", "ohne_rueckgaengig"];

/// The deliberate faults that act only in the data run (`--daten`).
const DATA_ONLY: [&str; 15] = [
    "archiv",
    "rakete_tot",
    "starter_vergessen",
    "menue_ohne_start",
    "schreiben",
    "frei",
    "einstellung",
    "schloss",
    "kein_anstoss",
    "im_bildfaden",
    "stand_haengt",
    "ohne_merkliste",
    "ohne_raffinerie",
    "ohne_katalog_wache",
    "ohne_einst_hinweis",
];

/// What eframe painted, by frame number: the system may show a frame later
/// than its photo is taken, the painted picture is exact.
type Painted = Arc<Mutex<Vec<(u64, Arc<egui::ColorImage>)>>>;

/// How often the stand-in main window was asked for the overview.
static OVERVIEW_ASKED: AtomicU32 = AtomicU32::new(0);
/// How often the stand-in main window was told the settings changed.
static SETTINGS_TOLD: AtomicU32 = AtomicU32::new(0);
/// How often the stand-in main window was asked to close.
static CLOSE_ASKED: AtomicU32 = AtomicU32::new(0);
/// What the stand-in main window of the icon's checks was asked, in order.
static TRAY_SEEN: Mutex<Vec<Signal>> = Mutex::new(Vec::new());

/// How often the stand-in main window of the icon's checks was asked for `signal`.
fn tray_asked(signal: Signal) -> usize {
    TRAY_SEEN.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|s| **s == signal).count()
}

pub struct Probe {
    path: PathBuf,
    counter: Option<String>,
    /// The hidden window standing in for the main window.
    helper: isize,
    /// The settings in [`TOUCHED`] before the data run touched them.
    saved: Vec<Option<Value>>,
    /// The bar side and the corner the main window reported.
    wanted_bar: BarSide,
    wanted_corner: Corner,
    /// Run with slow files (`--langsam`), the longest gap between two
    /// frames so far and when the last frame began.
    slow: bool,
    gap: f64,
    last_frame: f64,
    /// The window still passed clicks when the lock was clicked.
    locked_at_click: bool,
    /// Run on a copy of data (`--daten`).
    data: bool,
    lines: Vec<String>,
    failed: usize,
    stage: usize,
    since: f64,
    guard_ok: bool,
    remembered: Option<Remembered>,
    scale: f32,
    open_rows: Vec<String>,
    open_place: Rect,
    /// Every frame is recorded while this is on.
    filming: bool,
    /// Frames since the filming began, the frames with a new window size and that size.
    frame: u64,
    resizes: Vec<(u64, i32, i32)>,
    painted: Painted,
    /// Each frame: its number, when it was painted and the visible part (window pixels).
    frames: Vec<(u64, Instant, Rect)>,
    /// Each window region (window pixels) and when it was set; the first is
    /// the one the filming began with.
    regions: Vec<(Instant, Rect)>,
    /// The size of the window in the system at each frame.
    window_sizes: Vec<(i32, i32)>,
    /// A resize came before the layout of its frame.
    early: bool,
    /// The icon next to the clock: how often the stand-in main window was asked for the overview
    /// before, and the window of the icon.
    tray_base: usize,
    tray_window: isize,
    /// The rounding, looked at in every frame: see [`Corners`].
    corners: Corners,
}

/// The rounding of the window as each frame finds it: the corner preference DWM holds, and whether the
/// window region leaves out the corner pixels of the visible part.
#[derive(Default)]
struct Corners {
    /// Pixels pushed in from each corner of the visible part: past the painted outline, outside the rounding.
    inset: i32,
    /// The preference DWM held in the first frame, and the corner pixels the region held in the first
    /// frame with a region (the one the window is first shown with).
    first_preference: Option<Option<i32>>,
    first_region: Option<Vec<bool>>,
    /// Frames looked at; frames DWM held another preference; frames with a settled region, and those
    /// among them that held a corner pixel, with the state they were in.
    frames: u32,
    not_round: u32,
    settled: u32,
    square: Vec<String>,
    /// Frames layered (clicks pass) and not, and in each shown state.
    layered: u32,
    /// Layered frames whose attributes are not the empty ones (an opacity of the whole window, or none at all).
    layered_not_empty: u32,
    plain: u32,
    open: u32,
    bar: u32,
    strip: u32,
    /// Frames whose visible part is the whole window, and those among them with a window region.
    whole: u32,
    whole_with_region: u32,
}

impl Corners {
    /// Looks at the window as the frame before left it.
    fn watch(&mut self, app: &OverlayApp, ctx: &egui::Context, shown: Shown) {
        let hwnd = app.hwnd;
        self.inset = ctx.pixels_per_point().ceil() as i32;
        let preference = win::corner_preference(hwnd);
        self.first_preference.get_or_insert(preference);
        self.frames += 1;
        if preference != Some(win::corner_request().1) {
            self.not_round += 1;
        }
        if win::has_layered(win::ex_style(hwnd)) {
            self.layered += 1;
            self.layered_not_empty += u32::from(win::layered_flags(hwnd) != Some(0));
        } else {
            self.plain += 1;
        }
        match shown {
            Shown::Open => self.open += 1,
            Shown::Bar => self.bar += 1,
            Shown::Strip => self.strip += 1,
        }
        let part = app.current.relative_to(app.canvas.x, app.canvas.y);
        let region = win::in_region(hwnd, &corner_pixels(part, self.inset));
        // The whole window shown (open): the region rounds its corners as well.
        if app.region == Some(part) && part == Rect::new(0, 0, app.canvas.w, app.canvas.h) {
            self.whole += 1;
            self.whole_with_region += u32::from(region.is_some());
        }
        let Some(held) = region else { return };
        if self.first_region.is_none() {
            self.first_region = Some(held.clone());
        }
        // While a part given up settles the region is larger than the visible part.
        if app.region != Some(part) || versekit_core::theme::current().square {
            return;
        }
        self.settled += 1;
        if held.iter().any(|&h| h) {
            let layered = if win::has_layered(win::ex_style(hwnd)) { "geschichtet" } else { "ungeschichtet" };
            self.square.push(format!("{shown:?} {layered} {held:?}"));
        }
    }
}

/// The four corner pixels of `part`, `inset` pixels in: top left, top right, bottom left, bottom right.
fn corner_pixels(part: Rect, inset: i32) -> [(i32, i32); 4] {
    let (left, top) = (part.x + inset, part.y + inset);
    let (right, bottom) = (part.right() - 1 - inset, part.bottom() - 1 - inset);
    [(left, top), (right, top), (left, bottom), (right, bottom)]
}

impl Probe {
    pub fn new(
        path: PathBuf,
        guard_ok: bool,
        remembered: Option<Remembered>,
        scale: f32,
        counter: Option<String>,
        slow: bool,
    ) -> Probe {
        // An icon file of its own next to the result, so the icon next to the clock is loaded from a
        // file (as in the installed program) and the check can see it given back.
        if let Some(folder) = path.parent() {
            let own = folder.join("symbol");
            if std::fs::create_dir_all(&own).is_ok()
                && std::fs::write(own.join("icon.ico"), tray::sample_icon()).is_ok()
            {
                tray::use_icon_folder(&own);
            }
        }
        Probe {
            path,
            counter,
            helper: 0,
            saved: Vec::new(),
            wanted_bar: BarSide::Top,
            wanted_corner: Corner::Free,
            slow,
            gap: 0.0,
            last_frame: 0.0,
            locked_at_click: true,
            data: false,
            lines: Vec::new(),
            failed: 0,
            stage: 0,
            since: 0.0,
            guard_ok,
            remembered,
            scale,
            open_rows: Vec::new(),
            open_place: Rect::default(),
            filming: false,
            frame: 0,
            resizes: Vec::new(),
            painted: Painted::default(),
            frames: Vec::new(),
            regions: Vec::new(),
            window_sizes: Vec::new(),
            early: false,
            tray_base: 0,
            tray_window: 0,
            corners: Corners::default(),
        }
    }

    /// The rounding over the whole run: DWM holds its preference from the first frame on and in every
    /// frame, and the window region leaves out the corner pixels in every settled frame, in every state.
    fn judge_corners(&mut self) {
        let c = std::mem::take(&mut self.corners);
        let round = win::corner_request().1;
        self.value("dwm_ecken_beim_ersten_bild", format!("{:?}", c.first_preference.flatten()));
        self.value("bilder_mit_ecken_geprueft", c.frames);
        self.value("bilder_ohne_dwm_rundung", c.not_round);
        self.value("bilder_geschichtet_ungeschichtet", format!("{} {}", c.layered, c.plain));
        self.value("bilder_offen_leiste_streifen", format!("{} {} {}", c.open, c.bar, c.strip));
        self.value("eckpunkte_im_bereich_beim_ersten_bild", format!("{:?}", c.first_region));
        self.value("bilder_mit_festem_bereich", c.settled);
        self.value("bilder_mit_eckigem_bereich", c.square.len());
        self.value("bilder_ganz_offen_mit_bereich", format!("{} von {}", c.whole_with_region, c.whole));
        if let Some(first) = c.square.first() {
            self.value("erstes_eckiges_bild", first);
        }
        self.check("dwm_ecken_dem_bereich_ueberlassen_beim_ersten_bild", c.first_preference.flatten() == Some(round));
        self.check("dwm_ecken_dem_bereich_ueberlassen_in_jedem_bild", c.frames > 50 && c.not_round == 0);
        // DWM does not tell the border it holds: what it answered to every request for none is judged.
        let (taken, refused) = win::border_requests();
        self.value("dwm_rand_anfragen_angenommen_abgelehnt", format!("{taken} {refused}"));
        // Windows 10 does not know the request and refuses every one, harmlessly: there only the answers are told.
        let build = versekit_core::paths::windows_build_number();
        self.value("windows_build", build.as_deref().unwrap_or("?"));
        self.check(
            "dwm_nimmt_jede_anfrage_ohne_rand_an",
            !dwm_knows_border(build.as_deref()) || (taken > 0 && refused == 0),
        );
        self.check("ecken_ausgeschnitten_beim_ersten_bild", c.first_region.is_some_and(|h| h.iter().all(|&x| !x)));
        self.check("offen_mit_fensterbereich", c.whole > 50 && c.whole_with_region == c.whole);
        // The stage the overlay draws with: on a sound machine the graphics card through DirectComposition, the
        // first of the ladder.
        let stage = versekit_ui::renderer::chosen();
        let first = versekit_ui::renderer::ladder(versekit_ui::renderer::Look::Transparent).first().copied();
        self.value("grafik_stufe", stage.map_or("?", |s| s.id()));
        self.check("grafik_karte_durch_directcomposition", stage.is_some() && stage == first);
        self.value("geschichtete_bilder_mit_fensterdeckkraft", format!("{} von {}", c.layered_not_empty, c.layered));
        self.check("geschichtet_ohne_fensterdeckkraft", c.layered > 0 && c.layered_not_empty == 0);
        self.check(
            "ecken_ausgeschnitten_in_jedem_bild",
            c.settled > 50
                && c.square.is_empty()
                && c.layered > 0
                && c.plain > 0
                && c.open > 0
                && c.bar > 0
                && c.strip > 0,
        );
    }

    /// The filming begins with the window region as it stands (`None`: the
    /// whole window, whatever its size).
    fn start_filming(&mut self, region: Option<Rect>) {
        self.filming = true;
        let whole = Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2);
        self.regions.push((Instant::now(), region.unwrap_or(whole)));
    }

    /// A frame begins: the painted picture of the next frame (eframe reads
    /// the pixels one frame after the request).
    fn film_frame(&mut self, ctx: &egui::Context) {
        if !self.filming {
            return;
        }
        self.frame += 1;
        let (next, sink) = (self.frame + 1, self.painted.clone());
        ctx.request_screenshot(move |image| sink.lock().unwrap_or_else(|e| e.into_inner()).push((next, image)));
    }

    /// The window went from `before` to `target`; `before_layout` when that
    /// came before its frame was laid out.
    pub fn resized(&mut self, before: Rect, target: Rect, before_layout: bool) {
        self.early |= before_layout;
        if self.filming && (before.w, before.h) != (target.w, target.h) {
            self.resizes.push((self.frame, target.w, target.h));
        }
    }

    /// The window region was set to `region` (window pixels).
    pub fn region_set(&mut self, region: Rect) {
        if self.filming {
            self.regions.push((Instant::now(), region));
        }
    }

    /// This frame is painted now, with `part` visible (window pixels); the
    /// size the window has in the system is noted with it.
    pub fn painted_part(&mut self, hwnd: isize, part: Rect) {
        if self.filming {
            self.frames.push((self.frame, Instant::now(), part));
            let window = win::window_rect(hwnd);
            self.window_sizes.push((window.w, window.h));
        }
    }

    /// The frames whose visible part differs from the frame before, with that part.
    fn changes(&self) -> Vec<(u64, Rect)> {
        self.frames.windows(2).filter(|w| w[0].2 != w[1].2).map(|w| (w[1].0, w[1].2)).collect()
    }

    /// The share of pixels off a resting picture for the painted frame of
    /// each change: the resting picture is the last frame painted before
    /// the next change. 1.0 when a picture is missing or has another size.
    fn first_frames_off(&self, changes: &[(u64, Rect)]) -> Vec<f64> {
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        let at = |nr: u64| painted.iter().find(|(n, _)| *n == nr).map(|(_, image)| image.clone());
        let last_between =
            |from: u64, end: u64| painted.iter().filter(|(n, _)| *n > from && *n < end).map(|(n, _)| *n).max();
        changes
            .iter()
            .enumerate()
            .map(|(i, &(frame, _))| {
                let end = changes.get(i + 1).map_or(u64::MAX, |c| c.0);
                match (at(frame), last_between(frame, end).and_then(at)) {
                    (Some(first), Some(rest)) if first.size == rest.size => share_off(&argb(&rest), &argb(&first)),
                    _ => 1.0,
                }
            })
            .collect()
    }

    /// Over the painted frames: the most pixels covered beside the visible
    /// part (a pixel of edge left aside for rounding) and the fewest
    /// covered pixels inside it.
    fn coverage(&self) -> (usize, usize) {
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        let counts: Vec<(usize, usize)> = painted
            .iter()
            .filter_map(|(n, image)| {
                let &(_, _, part) = self.frames.iter().find(|f| f.0 == *n)?;
                let around = Rect::new(part.x - 1, part.y - 1, part.w + 2, part.h + 2);
                let width = image.size[0];
                let at = |i: usize| ((i % width) as i32, (i / width) as i32);
                let pixels = image.pixels.iter().enumerate();
                let beside = pixels.clone().filter(|&(i, c)| c.a() > 0 && !around.contains(at(i).0, at(i).1)).count();
                let inside = pixels.filter(|&(i, c)| c.a() > 0 && part.contains(at(i).0, at(i).1)).count();
                Some((beside, inside))
            })
            .collect();
        let beside = counts.iter().map(|c| c.0).max().unwrap_or(0);
        let inside = counts.iter().map(|c| c.1).min().unwrap_or(0);
        (beside, inside)
    }

    /// The painted frames that cover a corner pixel of their visible part (see [`corner_pixels`]).
    fn painted_corners(&self) -> usize {
        let painted = self.painted.lock().unwrap_or_else(|e| e.into_inner());
        let inset = self.corners.inset.max(1);
        painted
            .iter()
            .filter(|(n, image)| {
                let Some(&(_, _, part)) = self.frames.iter().find(|f| f.0 == *n) else { return false };
                let [width, height] = image.size;
                corner_pixels(part, inset).iter().any(|&(x, y)| {
                    x >= 0
                        && y >= 0
                        && (x as usize) < width
                        && (y as usize) < height
                        && image.pixels[y as usize * width + x as usize].a() > 0
                })
            })
            .count()
    }

    /// The frames the window region cut while the system may still have
    /// shown them: each region must hold every part painted from
    /// [`STILL_SHOWN`] before it was set until the next region.
    fn cut_frames(&self) -> usize {
        let still = std::time::Duration::from_secs_f64(STILL_SHOWN);
        let mut cut = 0;
        for (i, &(set, region)) in self.regions.iter().enumerate() {
            let until = self.regions.get(i + 1).map(|r| r.0);
            cut += self
                .frames
                .iter()
                .filter(|&&(_, at, part)| at + still >= set && until.is_none_or(|u| at < u) && !region.holds(part))
                .count();
        }
        cut
    }

    /// Fold and unfold seen frame by frame: the window keeps its size, the
    /// frame of each change is painted in the new state and transparent
    /// beside it, and the region never cuts a picture that may be on screen.
    fn judge_folding(&mut self) {
        let changes = self.changes();
        let first = self.first_frames_off(&changes);
        let first_off = first.iter().copied().fold(0.0, f64::max);
        let (covered, inside) = self.coverage();
        let cut = self.cut_frames();
        let corners = self.painted_corners();
        let pictures = self.painted.lock().unwrap_or_else(|e| e.into_inner()).len();
        let mut sizes = self.window_sizes.clone();
        sizes.dedup();
        self.value("bilder_beim_klappen", self.frames.len());
        self.value("wechsel_beim_klappen", changes.len());
        self.value("fenstergroessen_beim_klappen", format!("{sizes:?}"));
        self.value("groessenwechsel_beim_klappen", self.resizes.len());
        self.value("bereiche_beim_klappen", self.regions.len().saturating_sub(1));
        let each: Vec<String> = first.iter().map(|off| format!("{off:.3}")).collect();
        self.value("abweichung_erstes_bild_je_wechsel", each.join(" "));
        self.value("gemalte_punkte_im_sichtbaren_teil_mindestens", inside);
        self.value("gemalte_punkte_neben_dem_sichtbaren_teil_hoechstens", covered);
        self.value("abgeschnittene_bilder", cut);
        self.value("bilder_mit_gemalter_ecke_beim_klappen", corners);
        self.check("klappen_gefilmt", changes.len() >= 2 && pictures > 0 && inside > 0);
        self.check("fenster_behaelt_groesse_beim_klappen", self.resizes.is_empty() && sizes.len() == 1);
        self.check("erstes_bild_im_neuen_zustand", first_off < SHARE_OFF);
        self.check("neben_dem_sichtbaren_teil_durchsichtig", covered == 0);
        self.check("bereich_schneidet_kein_bild_ab", cut == 0);
        self.check("ecken_beim_klappen_nicht_gemalt", pictures > 0 && corners == 0);
    }

    /// The window region is the visible part: what Windows shows and lets
    /// take clicks is `part` (screen pixels).
    fn region_is(&mut self, hwnd: isize, part: Rect, name: &str) {
        let seen = win::visible_rect(hwnd);
        self.value(&format!("{name}_sichtbar"), overlay::geometry_text(seen));
        self.check(name, seen == part);
    }

    fn check(&mut self, name: &str, ok: bool) {
        if !ok {
            self.failed += 1;
        }
        self.lines.push(format!("{} {name}", if ok { "OK  " } else { "FEHL" }));
    }

    fn value(&mut self, name: &str, value: impl std::fmt::Display) {
        self.lines.push(format!("WERT {name} = {value}"));
    }

    fn finish(&mut self) {
        // A misspelt counter check would run as a plain one and pass.
        if let Some(counter) = self.counter.clone() {
            self.value("gegenprobe", &counter);
            self.check("gegenprobe_bekannt", COUNTERS.contains(&counter.as_str()));
            // These faults act in the data run only: without `--daten` they would pass unseen.
            if EITHER.contains(&counter.as_str()) {
                // These act in both runs, each with checks of its own: no run lets them pass unseen, so
                // the run is not asked for; they are red for their own reasons in either.
            } else if DATA_ONLY.contains(&counter.as_str()) {
                self.check("gegenprobe_mit_daten", self.data);
            } else {
                // The others act only in the run without data.
                self.check("gegenprobe_ohne_daten", !self.data);
            }
        }
        let mut text = String::new();
        for line in &self.lines {
            let _ = writeln!(text, "{line}");
        }
        let _ = writeln!(text, "ERGEBNIS {}", if self.failed == 0 { "gruen" } else { "rot" });
        let _ = std::fs::write(&self.path, text);
    }
}

/// The share of pixels with a channel further off than [`PIXEL_OFF`].
fn share_off(a: &[u32], b: &[u32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let off = a
        .iter()
        .zip(b)
        .filter(|&(&p, &q)| (0..4).any(|k| ((p >> (8 * k)) & 0xff).abs_diff((q >> (8 * k)) & 0xff) > PIXEL_OFF))
        .count();
    off as f64 / a.len() as f64
}

/// The buttons that stand in the bar whatever its width.
const FIXED: [Action; 6] = [Action::Quit, Action::Clear, Action::Fold, Action::Lock, Action::Rescan, Action::Overview];

/// What is cut off in the bar from `left` to `right` (points of the window)
/// with "⋯" closed: a button outside it, on the title or on another button;
/// `None` when nothing is.
fn cut_off(
    buttons: &[(Action, egui::Rect)],
    more: Option<egui::Rect>,
    title_end: f32,
    (left, right): (f32, f32),
) -> Option<String> {
    let mut rects: Vec<egui::Rect> = buttons.iter().map(|(_, r)| *r).chain(more).collect();
    rects.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    if let Some(r) = rects.iter().find(|r| r.min.x < left - 0.5 || r.max.x > right + 0.5) {
        return Some(format!("Knopf bei {:.0} außerhalb von {left:.0} bis {right:.0}", r.min.x));
    }
    if let Some(r) = rects.iter().find(|r| r.min.x < title_end - 0.5) {
        return Some(format!("Knopf bei {:.0} auf dem Titel bis {title_end:.0}", r.min.x));
    }
    if let Some(w) = rects.windows(2).find(|w| w[0].max.x > w[1].min.x + 0.5) {
        return Some(format!("Knöpfe bei {:.0} und {:.0} übereinander", w[0].min.x, w[1].min.x));
    }
    None
}

/// A painted picture as the photos hold it, one `u32` per pixel.
fn argb(image: &egui::ColorImage) -> Vec<u32> {
    image.pixels.iter().map(|c| u32::from_le_bytes(c.to_array())).collect()
}

/// Pixels in the text color of the rows in a photo of the window.
fn ink(hwnd: isize) -> usize {
    let Some((_, _, pixels)) = win::photo(hwnd) else { return 0 };
    pixels
        .iter()
        .filter(|&&p| {
            let (b, g, r) = (p & 0xff, (p >> 8) & 0xff, (p >> 16) & 0xff);
            r > BRIGHT && g > BRIGHT && b > BRIGHT
        })
        .count()
}

/// Pixels in the accent color (the strip and its line).
fn accent_pixels(hwnd: isize) -> usize {
    let Some((_, _, pixels)) = win::photo(hwnd) else { return 0 };
    pixels
        .iter()
        .filter(|&&p| {
            let (b, g, r) = (p & 0xff, (p >> 8) & 0xff, (p >> 16) & 0xff);
            g > 180 && r > 110 && r < 200 && b < 110
        })
        .count()
}

/// Whether the system of `build` (`CurrentBuildNumber`) knows DWM's request for no border: Windows 11, build 22000
/// on. A build that cannot be read counts as knowing, so the check stays strict.
fn dwm_knows_border(build: Option<&str>) -> bool {
    build.and_then(|b| b.trim().parse::<u32>().ok()).is_none_or(|b| b >= 22000)
}

fn in_corner(r: Rect) -> bool {
    let work = work_at(r.x, r.y);
    (r.right() - (work.right() - EDGE)).abs() <= 1 && (r.bottom() - (work.bottom() - EDGE)).abs() <= 1
}

/// The settings the data run touches; they are put back at its end.
const TOUCHED: [&str; 5] = [keys::CORNER, keys::BAR, keys::CLICK_THROUGH, keys::AUTO_THROUGH, keys::INTERVAL];

/// The contracts the placeholder run puts into the bar: a normal one with two
/// objectives, a silent one (grey) and a bare title.
fn sample_contracts() -> Vec<ContractLine> {
    let line = |title: &str, text: &str, goals: &[&str], grey: bool| ContractLine {
        title: title.to_string(),
        text: Message::Plain(text.to_string()),
        goals: goals.iter().map(|g| g.to_string()).collect(),
        grey,
    };
    vec![
        line(
            "Probe Eins",
            "Auftrag angenommen: Probe Eins  →  Baupläne 1/3 · dir fehlt: Helm",
            &["Fliege los", "Sammle die Kisten"],
            false,
        ),
        line("Probe Zwei", "Auftrag angenommen: Probe Zwei  →  Baupläne 0/2", &["Nicht gezeigt"], true),
        line("Probe Drei", "Auftrag angenommen: Probe Drei", &[], false),
    ]
}

/// Writes lines at the end of the Game.log, and only a Game.log that lies
/// inside the copy of the data: the real one is never written.
fn append_to_log(app: &OverlayApp, text: &str) -> bool {
    use std::io::Write as _;
    let (Some(paths), Some(root)) = (&app.paths, app.args().data.as_deref()) else { return false };
    let Some(log) = paths.game_log(None) else { return false };
    let norm = |p: &str| p.replace('\\', "/").to_lowercase();
    if !norm(&log).starts_with(&format!("{}/", norm(root).trim_end_matches('/'))) {
        return false;
    }
    std::fs::OpenOptions::new().append(true).open(log).and_then(|mut f| f.write_all(text.as_bytes())).is_ok()
}

/// A line of the log that announces or ends a contract, in the form the game writes it.
fn log_notice(phrase: &str, title: &str, mission: &str, objective: &str) -> String {
    format!(
        "<2026-10-02T09:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{phrase}: {title}: \" [1] MissionId: [{mission}], ObjectiveId: [{objective}]\n"
    )
}

/// The click on the middle of a rectangle (points of the window), as the window gets it.
fn click_events(at: egui::Pos2) -> [egui::Event; 3] {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    [egui::Event::PointerMoved(at), press(true), press(false)]
}

/// A setting as stored, whatever its kind.
fn raw(paths: &versekit_core::paths::Paths, key: &str) -> Option<Value> {
    paths.settings().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Writes settings as the main window would, and says so.
fn main_window_writes(app: &OverlayApp, values: &[(&str, Value)]) {
    if let Some(paths) = &app.paths {
        for (key, value) in values {
            paths.set_setting(key, value.clone());
        }
    }
    window_signal::send(app.hwnd, Signal::Settings);
}

/// On a copy of real data: wait for the watcher, then report what it found.
/// Then the settings between the windows: a drag by hand frees the corner
/// and tells the main window; a change the main window reports arrives at
/// once, the lock among them, without a restart. Then the way back from a
/// locked overlay: the lock under the visible pointer takes clicks, and a
/// click on it opens the lock. With `--langsam` instead: the frames go on
/// while every file access is slow. Last, whether any thread wrote outside
/// the copy. The settings touched are put back.
fn data_run(app: &mut OverlayApp, probe: &mut Probe, ctx: &egui::Context, now: f64) {
    let waited = now - probe.since;
    let transparent = win::has_transparent(win::ex_style(app.hwnd));
    let next = |probe: &mut Probe, stage: usize| {
        probe.stage = stage;
        probe.since = now;
    };
    match probe.stage {
        0 => {
            let started = app.status.render() != t("ov_starte");
            if !(started && waited >= 8.0 || waited >= 90.0) {
                return;
            }
            probe.value("status", app.status.render());
            probe.value("zeilen", app.rows.len());
            for row in app.rows.iter() {
                probe.value("zeile", format!("{row:?}"));
            }
            probe.check("watcher_meldet_sich", started);
            // A Data.p4k given with `--archiv` is the one the finds read.
            probe.value("archiv_gelesen", app.archive_read.as_deref().unwrap_or("-"));
            if let Some(given) = app.args().archive.clone() {
                probe.check("archiv_von_aussen_gelesen", app.archive_read.as_deref() == Some(given.as_str()));
            }
            // The fault shows only with an archive given that differs from the one of the game folder.
            if probe.counter.as_deref() == Some("archiv") {
                let given = app.args().archive.clone();
                probe.check("gegenprobe_mit_archiv", given.is_some() && app.archive_read != given);
            }
            let Some(paths) = &app.paths else { return };
            probe.saved = TOUCHED.iter().map(|key| raw(paths, key)).collect();
            if probe.slow {
                probe.gap = 0.0;
                next(probe, 20);
                return;
            }
            // A hidden window here plays the main window and counts what it is told.
            probe.helper = win::hidden_window();
            window_signal::serve(probe.helper, Role::Main, |signal| {
                if signal == Signal::Settings {
                    SETTINGS_TOLD.fetch_add(1, Ordering::Relaxed);
                }
            });
            // A drag by hand while the overlay sits in a corner, as the main window set it.
            paths.set_setting(keys::CORNER, Value::str(Corner::BottomLeft.setting()));
            app.settings.layout.corner = Corner::BottomLeft;
            let start = Rect { x: app.current.x - 50, ..app.current };
            let shown = app.vis.shown(now);
            app.drag_ended(Drag::Move, start, shown);
            next(probe, 1);
        }
        1 if waited >= 0.8 => {
            let corner = app.paths.as_ref().and_then(|p| p.setting(keys::CORNER));
            probe.value("ecke_nach_ziehen", corner.as_deref().unwrap_or("-"));
            probe.check("gezogen_wird_frei_gespeichert", corner.as_deref() == Some(Corner::Free.setting()));
            probe.value("hauptfenster_benachrichtigt", SETTINGS_TOLD.load(Ordering::Relaxed));
            probe.check("gezogen_meldet_hauptfenster", SETTINGS_TOLD.load(Ordering::Relaxed) >= 1);
            // A read now: the next one by the clock is a second away.
            if let Some(files) = &app.files {
                files.ask(crate::files::Job::Read { fresh: false });
            }
            next(probe, 2);
        }
        2 if waited >= 0.3 => {
            // The main window puts the overlay into a corner and turns the bar.
            let bottom = app.settings.layout.bar == BarSide::Bottom;
            probe.wanted_bar = if bottom { BarSide::Top } else { BarSide::Bottom };
            let right = app.settings.layout.corner == Corner::BottomRight;
            probe.wanted_corner = if right { Corner::TopLeft } else { Corner::BottomRight };
            main_window_writes(
                app,
                &[
                    (keys::BAR, Value::str(probe.wanted_bar.setting())),
                    (keys::CORNER, Value::str(probe.wanted_corner.setting())),
                ],
            );
            next(probe, 3);
        }
        3 => {
            let layout = app.settings.layout;
            let arrived = layout.bar == probe.wanted_bar && layout.corner == probe.wanted_corner;
            if !arrived && waited < 2.0 {
                return;
            }
            probe.value("ecke_und_leiste_kamen_nach_s", format!("{waited:.2}"));
            probe.check("ecke_und_leiste_sofort_da", arrived && waited < 0.5);
            // The lock switched on in the main window.
            main_window_writes(
                app,
                &[(keys::CLICK_THROUGH, Value::Bool(true)), (keys::AUTO_THROUGH, Value::Bool(false))],
            );
            next(probe, 4);
        }
        4 => {
            if !(app.settings.click_through && transparent) && waited < 2.0 {
                return;
            }
            probe.check("durchklick_an_aus_der_einstellung", app.settings.click_through && transparent);
            if let Some(files) = &app.files {
                files.ask(crate::files::Job::Read { fresh: false });
            }
            next(probe, 5);
        }
        5 if waited >= 0.3 => {
            // ... and off again, without a restart.
            main_window_writes(app, &[(keys::CLICK_THROUGH, Value::Bool(false))]);
            next(probe, 6);
        }
        6 => {
            if transparent && waited < 2.0 {
                return;
            }
            probe.value("durchklick_aus_nach_s", format!("{waited:.2}"));
            probe.check("durchklick_aus_ohne_neustart", !transparent && waited < 0.5);
            main_window_writes(app, &[(keys::CLICK_THROUGH, Value::Bool(true))]);
            next(probe, 7);
        }
        7 => {
            if !transparent && waited < 2.0 {
                return;
            }
            // The visible pointer onto the lock in the bar.
            let ppp = ctx.pixels_per_point();
            if let Some(lock) = app.lock {
                let c = lock.center();
                let (x, y) = (app.canvas.x + (c.x * ppp) as i32, app.canvas.y + (c.y * ppp) as i32);
                app.pointer_override = Some(overlay::Pointer { hidden: false, x, y });
            }
            next(probe, 8);
        }
        8 if waited >= 0.5 => {
            probe.check("schloss_klickbar_unter_dem_zeiger", app.lock.is_some() && !transparent);
            // The window got no pointer moves while clicks passed: it must be
            // told the pointer stands on the lock, or the first click misses.
            // (Here the real pointer is elsewhere, so the system takes that
            // move back at once; only the telling can be checked.)
            let ppp = ctx.pixels_per_point();
            let told_on_lock = app.lock.is_some_and(|lock| {
                app.nudged.iter().any(|&(x, y)| lock.contains(egui::pos2(x as f32 / ppp, y as f32 / ppp)))
            });
            probe.value("zeiger_mitgeteilt", format!("{:?}", app.nudged));
            probe.check("zeiger_auf_dem_schloss_mitgeteilt", told_on_lock);
            // A click on the lock as it reaches the window.
            probe.locked_at_click = transparent;
            if let Some(lock) = app.lock {
                let at = lock.center();
                let press = |pressed| egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                app.injected.extend([egui::Event::PointerMoved(at), press(true), press(false)]);
            }
            next(probe, 9);
        }
        9 if waited >= 0.6 => {
            probe.check("klick_aufs_schloss_hebt_auf", !probe.locked_at_click && !app.settings.click_through);
            app.pointer_override = None;
            next(probe, 10);
        }
        // The running contracts, live: the Game.log of the copy grows.
        10 => {
            if let Some(paths) = &app.paths {
                paths.set_setting(keys::INTERVAL, Value::int(1));
            }
            let wrote = append_to_log(app, &log_notice("Auftrag angenommen", "Probelauf Eins", "probe-m1", ""));
            // Only a log inside the copy is ever written. Without one, the checks of the live
            // contracts cannot run, and the run must not pass as if they had.
            probe.check("spielprotokoll_in_der_kopie", wrote);
            if !wrote {
                next(probe, 30);
                return;
            }
            next(probe, 11);
        }
        11 => {
            let found = app.contracts.iter().find(|c| c.title == "Probelauf Eins").cloned();
            if (found.is_none() || app.contract_texts.is_empty()) && waited < 10.0 {
                return;
            }
            probe.value("leiste_nach_s", format!("{waited:.1}"));
            probe.value("leiste", format!("{:?}", app.contract_texts));
            probe.check(
                "auftrag_live_in_der_leiste",
                found.as_ref().is_some_and(|c| c.text.render().ends_with("Probelauf Eins"))
                    && app.contract_texts.iter().any(|t| t.ends_with("Probelauf Eins")),
            );
            let goal = "<2026-10-02T09:00:01.000Z> <ObjectiveUpserted> x mission_id probe-m1 - objective_id 0123abcd-9 - state MISSION_OBJECTIVE_STATE_INPROGRESS - flags=ShowInLog y\n"
                .to_string()
                + "<2026-10-02T09:00:02.000Z> [Notice] Added notification \"Neues Ziel: Fliege los: \" [1] MissionId: [probe-m1], ObjectiveId: [0123abcd-9]\n";
            append_to_log(app, &goal);
            next(probe, 12);
        }
        12 => {
            let goals =
                |app: &OverlayApp| app.contracts.iter().find(|c| c.title == "Probelauf Eins").map(|c| c.goals.clone());
            let drawn = app.contract_texts.iter().any(|t| t == "Fliege los");
            if (goals(app).is_none_or(|g| g.is_empty()) || !drawn) && waited < 10.0 {
                return;
            }
            probe.value("auftraege_nach_s", format!("{waited:.1} {:?}", app.contracts));
            probe.check("ziel_live_unter_dem_auftrag", goals(app) == Some(vec!["Fliege los".to_string()]));
            probe.check("ziel_steht_in_der_leiste", app.contract_texts.iter().any(|t| t == "Fliege los"));
            append_to_log(app, &log_notice("Auftrag abgeschlossen", "Probelauf Eins", "probe-m1", ""));
            next(probe, 13);
        }
        13 => {
            if !(app.contracts.is_empty() && app.contract_texts.is_empty()) && waited < 10.0 {
                return;
            }
            probe.value("auftraege_nach_ende_s", format!("{waited:.1} {:?}", app.contracts));
            probe.check("ende_raeumt_die_leiste", app.contracts.is_empty() && app.contract_texts.is_empty());
            append_to_log(app, &log_notice("Auftrag angenommen", "Probelauf Zwei", "probe-m2", ""));
            next(probe, 14);
        }
        14 => {
            let found = app.contracts.iter().any(|c| c.title == "Probelauf Zwei");
            if !(found && !app.hide_buttons.is_empty()) && waited < 10.0 {
                return;
            }
            probe.value(
                "auftraege_zweiter_s",
                format!("{waited:.1} {:?}", app.contracts.iter().map(|c| &c.title).collect::<Vec<_>>()),
            );
            probe.check("zweiter_auftrag_live", found);
            // The ban button of its line, clicked as the window gets it.
            if let Some(button) = app.hide_buttons.first() {
                app.injected.extend(click_events(button.center()));
            }
            next(probe, 15);
        }
        15 if waited >= 1.5 => {
            probe.check("ausblenden_nimmt_auftrag_heraus_live", app.contracts.is_empty());
            let file = app.paths.as_ref().map(|p| p.app_file(versekit_core::live_contracts::HIDDEN_FILE));
            let kept = file.and_then(|f| std::fs::read_to_string(f).ok()).is_some_and(|text| text.contains("probe-m2"));
            probe.check("ausgeblendet_in_der_kopie_gemerkt", kept);
            // The line with the way back stands there, and its button is clicked as the window gets it.
            let said = tf("ov_a_ausgeblendet", &[("0", "Probelauf Zwei".into())]);
            probe.check("zeile_mit_dem_weg_zurueck", app.contract_texts.contains(&said) && app.unhide_button.is_some());
            if let Some(button) = app.unhide_button {
                app.injected.extend(click_events(button.center()));
            }
            next(probe, 50);
        }
        // Taking the hiding back brings the contract again (read from the log) and takes its id out of the file.
        50 => {
            let back = app.contracts.iter().any(|c| c.title == "Probelauf Zwei");
            if !(back && !app.hide_buttons.is_empty()) && waited < 10.0 {
                return;
            }
            probe.check("rueckgaengig_holt_den_auftrag_zurueck", back);
            let file = app.paths.as_ref().map(|p| p.app_file(versekit_core::live_contracts::HIDDEN_FILE));
            let kept = file.and_then(|f| std::fs::read_to_string(f).ok()).is_some_and(|text| text.contains("probe-m2"));
            probe.check("rueckgaengig_nimmt_die_id_aus_der_datei", !kept);
            probe.check("zeile_mit_dem_weg_zurueck_ist_weg", app.unhide_button.is_none());
            // Hidden once more, for the check of the reading again below.
            if let Some(button) = app.hide_buttons.first() {
                app.injected.extend(click_events(button.center()));
            }
            next(probe, 51);
        }
        51 if waited >= 1.5 => {
            probe.check("erneut_ausgeblendet", app.contracts.is_empty());
            // Reading everything again must not bring it back: the log still holds its acceptance.
            app.act(ctx, Action::Rescan);
            next(probe, 16);
        }
        16 if waited >= 4.0 => {
            // The answer of the reading again must have come, otherwise an empty bar proves nothing.
            let answered = app
                .rows
                .iter()
                .any(|r| matches!(r, Row::Hint { text: Message::Phrase(p), .. } if p.key == "neu_gelesen"));
            probe.check("neu_lesen_wurde_beantwortet", answered);
            probe.check("ausgeblendet_bleibt_nach_neu_lesen", answered && app.contracts.is_empty());
            next(probe, 70);
        }
        // The way to start the game: an own command is entered, the rocket comes, a click on it and the
        // entry of the icon's menu hand the split command to the recorder, and nothing is ever started.
        70 => {
            let Some(paths) = &app.paths else { return };
            let entered = paths.set_setting(game_start::STARTER_SETTING, Value::str("probe-starter --ready"));
            probe.check("startbefehl_in_der_kopie_eingetragen", entered);
            next(probe, 71);
        }
        71 => {
            let rocket = app.buttons.iter().find(|(a, _)| *a == Action::Launch).map(|(_, r)| *r);
            let menu_has_it =
                app.tray.as_ref().is_some_and(|t| t.menu_now().iter().any(|i| i.entry == Some(tray::Entry::Launcher)));
            if !(rocket.is_some() && menu_has_it) && waited < 12.0 {
                return;
            }
            probe.value("rakete_nach_s", format!("{waited:.1}"));
            probe.check("rakete_steht_in_der_leiste", rocket.is_some());
            probe.check("rakete_im_fenster", rocket.is_some_and(|r| ctx.content_rect().contains_rect(r)));
            probe.check("menue_hat_den_startpunkt", menu_has_it);
            if let Some(r) = rocket {
                app.injected.extend(click_events(r.center()));
            }
            next(probe, 72);
        }
        72 => {
            let calls = || app.spawn_log.as_ref().map_or_else(Vec::new, |r| r.calls());
            if calls().is_empty() && waited < 8.0 {
                return;
            }
            let want = game_start::Started { command: vec!["probe-starter".into(), "--ready".into()], env: Vec::new() };
            probe.value("gestartet", format!("{:?}", calls()));
            probe.check("rakete_startet_den_befehl_ueber_den_aufzeichner", calls() == [want.clone()]);
            // The entry of the icon's menu does the same.
            let window = app.tray.as_ref().map_or(0, tray::Tray::window);
            probe.check(
                "menuepunkt_geschickt",
                window != 0 && tray::post(window, WM_COMMAND, tray::Entry::Launcher.command() as usize, 0),
            );
            next(probe, 73);
        }
        73 => {
            let calls = app.spawn_log.as_ref().map_or_else(Vec::new, |r| r.calls());
            if calls.len() < 2 && waited < 8.0 {
                return;
            }
            let want = game_start::Started { command: vec!["probe-starter".into(), "--ready".into()], env: Vec::new() };
            probe.check("menuepunkt_startet_ueber_den_aufzeichner", calls == [want.clone(), want]);
            next(probe, 17);
        }
        // The watch list, a refinery job that is over and a blueprint found: all in the copy.
        17 => {
            let Some(paths) = &app.paths else { return };
            let write = |name: &str, text: String| {
                let target = paths.app_file(name);
                let _ = std::fs::create_dir_all(
                    std::path::Path::new(&target).parent().unwrap_or(std::path::Path::new(".")),
                );
                std::fs::write(target, text).is_ok()
            };
            let list = r#"{"namen": ["Probelauf Bauplan"], "eintraege": [{"titel": "Beliebiger Probelauf", "muster": ["probelauf"]}, {"titel": "Kanone gesucht", "muster": ["kanone"]}]}"#;
            let past =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
                    - 60.0;
            let job = format!(
                r#"{{"version": 1, "auftraege": [{{"materials": ["Gold", "Iron"], "total": "3.5", "ends": {past}}}]}}"#
            );
            let wrote = write(versekit_core::watchlist::FILE, list.to_string())
                && write(versekit_core::refinery_jobs::FILE, job);
            probe.check("merkliste_und_raffinerie_in_der_kopie_angelegt", wrote);
            let line = "<2026-10-02T09:01:00.000Z> <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Probelauf Bauplan: \" [1]\n";
            probe.check("fund_ins_protokoll", append_to_log(app, line));
            next(probe, 18);
        }
        18 => {
            let hint = |app: &OverlayApp, part: &str| {
                app.rows.iter().any(|r| matches!(r, Row::Hint { text, .. } if text.render().contains(part)))
            };
            // The message for the system reaches the icon (a recorder here): a balloon with the text.
            let noticed = |app: &OverlayApp| {
                app.tray_log
                    .as_ref()
                    .is_some_and(|r| r.calls().iter().any(|c| c.message == NIM_MODIFY && c.text.contains("Gold, Iron")))
            };
            let merk = hint(app, "Probelauf Bauplan ist da");
            let raff = hint(app, "Raffinerie fertig") && noticed(app);
            if !(merk && raff) && waited < 22.0 {
                return;
            }
            probe.value("raffinerie_nach_s", format!("{waited:.1}"));
            probe.check("merkliste_wird_abgehakt_gemeldet", merk);
            probe.check("raffinerie_fertig_gemeldet", hint(app, "Raffinerie fertig: Gold, Iron"));
            probe.check("raffinerie_meldung_fuers_system", noticed(app));
            let read = |name: &str| {
                app.paths.as_ref().and_then(|p| std::fs::read_to_string(p.app_file(name)).ok()).unwrap_or_default()
            };
            let list = read(versekit_core::watchlist::FILE);
            probe.check(
                "merkliste_geht_nur_das_muster",
                list.contains("Probelauf Bauplan")
                    && list.contains("Kanone gesucht")
                    && !list.contains("Beliebiger Probelauf"),
            );
            probe.check(
                "raffinerie_auftrag_nur_einmal",
                read(versekit_core::refinery_jobs::FILE).contains("\"notified\": true"),
            );
            next(probe, 19);
        }
        19 => {
            // A patch brings something new into the catalog: the file gets one more blueprint and a new version.
            let Some(paths) = &app.paths else { return };
            let file = versekit_core::catalog_cache::current_file(paths);
            let changed = (|| {
                let Ok(mut data) = versekit_core::jsonio::read_json(&file) else { return false };
                let Value::Object(map) = &mut data else { return false };
                let entry = versekit_core::jsonio::loads(
                    r#"{"n": "Probelauf Kanone", "a": "WeaponGun", "sub": "Gun", "s": 2, "g": 1}"#,
                );
                let (Ok(entry), Some((_, Value::Object(blueprints)))) =
                    (entry, map.iter_mut().find(|(k, _)| k == "bauplaene"))
                else {
                    return false;
                };
                versekit_core::jsonio::map_set(blueprints, "probelauf kanone", entry);
                versekit_core::jsonio::map_set(map, "version", Value::str("probe-2"));
                std::fs::write(&file, versekit_core::jsonio::dumps(&data, None, false)).is_ok()
            })();
            probe.check("katalog_in_der_kopie_erweitert", changed);
            // Reading everything again looks at the catalog without waiting for its minute.
            app.act(ctx, Action::Rescan);
            next(probe, 40);
        }
        40 => {
            let new = |app: &OverlayApp| {
                app.rows.iter().find_map(|r| match r {
                    Row::Catalog { name, watch_title, .. } if name == "Probelauf Kanone" => Some(watch_title.clone()),
                    _ => None,
                })
            };
            if new(app).is_none() && waited < 15.0 {
                return;
            }
            probe.value("katalog_nach_s", format!("{waited:.1}"));
            probe.check("katalog_zuwachs_gemeldet", new(app).is_some());
            probe.check("merkliste_trifft_den_zuwachs", new(app) == Some(Some("Kanone gesucht".to_string())));
            next(probe, 41);
        }
        41 => {
            // A damaged settings file is told, once.
            let Some(paths) = &app.paths else { return };
            let broken = std::fs::write(paths.settings_file(), "{ kaputt").is_ok();
            probe.check("einstellungsdatei_kaputt_gemacht", broken);
            next(probe, 42);
        }
        42 => {
            let told = |app: &OverlayApp| {
                app.rows.iter().any(|r| {
                    matches!(r, Row::Hint { text, .. } if {
                        let shown = text.render();
                        shown.contains("Einstellungen beschädigt")
                    })
                })
            };
            if !told(app) && waited < 15.0 {
                return;
            }
            probe.check("kaputte_einstellungen_gemeldet", told(app));
            next(probe, 30);
        }
        // Slow files: the frames must go on.
        20 if waited >= 4.0 => {
            probe.value("laengste_luecke_zwischen_bildern_s", format!("{:.2}", probe.gap));
            probe.check("bilder_fluessig_bei_langsamem_ordner", probe.gap < 0.25);
            next(probe, 30);
        }
        30 => {
            if probe.helper != 0 {
                win::destroy_window(probe.helper);
            }
            if let Some(paths) = &app.paths {
                for (key, value) in TOUCHED.iter().zip(probe.saved.clone()) {
                    if let Some(value) = value {
                        paths.set_setting(key, value);
                    }
                }
            }
            let mut denied = writeguard::take_denied();
            denied.append(&mut app.denied);
            probe.value("gesperrte_schreibversuche", denied.join(" | "));
            probe.check("nur_in_der_kopie_geschrieben", denied.is_empty());
            probe.finish();
            next(probe, 31);
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        _ => {}
    }
}

/// One step of the self-check; does nothing without `--probe`.
pub fn step(app: &mut OverlayApp, ctx: &egui::Context, now: f64) {
    let Some(mut probe) = app.probe.take() else { return };
    ctx.request_repaint();
    let waited = now - probe.since;
    if probe.last_frame > 0.0 {
        probe.gap = probe.gap.max(now - probe.last_frame);
    }
    probe.last_frame = now;
    probe.data = app.paths.is_some();
    if app.paths.is_some() {
        data_run(app, &mut probe, ctx, now);
        app.probe = Some(probe);
        return;
    }
    let next = |probe: &mut Probe| {
        probe.stage += 1;
        probe.since = now;
    };
    let hwnd = app.hwnd;
    probe.film_frame(ctx);
    let shown = app.vis.shown(now);
    probe.corners.watch(app, ctx, shown);
    let style = win::ex_style(hwnd);
    match probe.stage {
        0 if waited >= 1.0 => {
            app.vis.set_game(true);
            app.settings.popup_seconds = 2;
            // The pointer of the invisible desktop must not decide anything.
            app.settings.auto_through = false;
            probe.check("schutz_eingehaengt", probe.guard_ok);
            probe.check("noactivate_beim_start", win::has_noactivate(style));
            probe.check("immer_oben_beim_start", win::has_topmost(style));
            probe.check("klick_aktiviert_nicht", win::asks_no_activation(hwnd));
            probe.check("inhalt_fuellt_das_fenster", win::client_fills_window(hwnd));
            let r = win::visible_rect(hwnd);
            probe.value("lage_beim_start", overlay::geometry_text(r));
            probe.value("skalierung", probe.scale);
            // A real case: a folded 4.12 overlay saved just below the main
            // screen (`612x26+9+1451`, bar below), where no monitor is.
            let lost = Remembered { w: 612, h: 26, place: Some((9, 1451)) };
            let px = lost.to_pixels(probe.scale);
            if let Some((x, y)) = px.place
                && !screen::on_a_screen(x, y + px.h - 20, px.w)
            {
                let back = crate::app::start_rect(Some(lost), (440, 254), BarSide::Bottom, probe.scale);
                probe.value("unter_dem_hauptschirm_zurueck_nach", overlay::geometry_text(back));
                probe.check(
                    "unter_dem_hauptschirm_zurueckgeholt",
                    screen::on_a_screen(back.x, back.y, back.w) && back == crate::app::centered_on_main((px.w, px.h)),
                );
            } else {
                probe.value("unter_dem_hauptschirm", "liegt auf diesem Rechner auf einem Bildschirm, entfällt");
            }
            match probe.remembered.map(|r| r.to_pixels(probe.scale)) {
                Some(Remembered { w, h, place: Some((x, y)) }) if screen::on_a_screen(x, y, w) => {
                    probe.value("gemerkte_lage_in_pixeln", overlay::geometry_text(Rect::new(x, y, w, h)));
                    probe.check("startlage_wie_gemerkt", (r.x, r.y) == (x, y));
                }
                Some(Remembered { w, h, .. }) => {
                    // Off every screen or without a place: middle of the main screen, size kept.
                    let back = crate::app::centered_on_main((w, h));
                    probe.value("zurueckgeholt_nach", overlay::geometry_text(app.last_open));
                    probe.check("ausserhalb_zurueckgeholt", screen::on_a_screen(r.x, r.y, r.w));
                    probe.check("groesse_bleibt_beim_zurueckholen", app.last_open == back);
                }
                None => {
                    // Enough finds to fill more than the default size.
                    for n in 4..=12 {
                        let row = Row::Found {
                            name: format!("Bauplan {n}"),
                            kind: "Kühler".into(),
                            code: "S1 · A".into(),
                            time: "21:00:00".into(),
                            caught_up: false,
                        };
                        app.rows.push(row, 20);
                    }
                }
            }
            next(&mut probe);
        }
        1 if waited >= 1.0 => {
            let shown = app.vis.shown(now);
            probe.check("offen_mit_spiel", shown == Shown::Open);
            if probe.remembered.is_none() {
                probe.value("standardgroesse", overlay::geometry_text(app.last_open));
                probe.value("sichtbare_zeilen", app.visible_rows);
                probe.check("standardgroesse_5_bis_7_zeilen", (5..=7).contains(&app.visible_rows));
            }
            let dots = ink(hwnd);
            probe.value("textpunkte_offen", dots);
            probe.check("inhalt_offen", dots >= INK);
            probe.open_rows.clone_from(&app.drawn);
            probe.open_place = app.last_open;
            probe.check("zeilen_gezeichnet", app.drawn.len() >= 3);
            app.settings.mode = Mode::Popup;
            app.vis.set_mode(Mode::Popup);
            next(&mut probe);
        }
        2 if waited >= 1.0 => {
            probe.check("wartet_als_streifen", app.vis.shown(now) == Shown::Strip);
            let r = win::visible_rect(hwnd);
            probe.value("streifen", overlay::geometry_text(r));
            let bar = (versekit_ui::overlay::BAR_HEIGHT * ctx.pixels_per_point()) as i32;
            probe.check("streifen_schmal", r.h < bar && r.w < app.last_open.w);
            probe.check("streifen_sagt_was_er_tut", app.drawn.contains(&t("ov_streifen")));
            let accent = accent_pixels(hwnd);
            probe.value("akzentpunkte_streifen", accent);
            probe.check("streifen_sichtbar", accent >= 40);
            app.vis.found(now, 2.0);
            next(&mut probe);
        }
        3 if waited >= 0.8 => {
            probe.check("fund_blendet_auf", app.vis.shown(now) == Shown::Open);
            let dots = ink(hwnd);
            probe.value("textpunkte_nach_fund", dots);
            probe.check("inhalt_nach_fund", dots >= INK);
            next(&mut probe);
        }
        4 if waited >= 2.0 => {
            probe.check("wieder_ausgeblendet", app.vis.shown(now) == Shown::Strip);
            next(&mut probe);
        }
        // Wait while hidden, then the pointer on the strip brings it back.
        5 if waited >= 3.0 => {
            app.vis.set_hovered(now, true, 2.0);
            next(&mut probe);
        }
        6 if waited >= 0.8 => {
            probe.check("zurueckgeholt", app.vis.shown(now) == Shown::Open);
            let dots = ink(hwnd);
            probe.value("textpunkte_nach_zurueckholen", dots);
            probe.check("inhalt_nach_zurueckholen", dots >= INK);
            probe.check("dieselben_zeilen_nach_zurueckholen", app.drawn == probe.open_rows);
            probe.value("lage_nach_zurueckholen", overlay::geometry_text(app.last_open));
            probe.check("lage_bleibt_beim_aufblenden", app.last_open == probe.open_place);
            app.vis.set_hovered(now, false, 2.0);
            app.settings.mode = Mode::Always;
            app.vis.set_mode(Mode::Always);
            app.settings.click_through = true;
            next(&mut probe);
        }
        7 if waited >= 1.0 => {
            probe.check("durchklick_an", win::has_transparent(style) && app.passing);
            probe.check("noactivate_nach_durchklick_an", win::has_noactivate(style));
            let dots = ink(hwnd);
            probe.value("textpunkte_bei_durchklick", dots);
            probe.check("sichtbar_bei_durchklick", dots >= INK);
            app.settings.click_through = false;
            next(&mut probe);
        }
        8 if waited >= 1.0 => {
            probe.check("durchklick_aus", !win::has_transparent(style) && !app.passing);
            probe.check("noactivate_nach_durchklick_aus", win::has_noactivate(style));
            app.settings.layout = Layout { corner: Corner::BottomRight, bar: BarSide::Bottom };
            app.layout_changed();
            next(&mut probe);
        }
        9 if waited >= 1.0 => {
            let r = win::visible_rect(hwnd);
            probe.value("ecke", overlay::geometry_text(r));
            probe.check("ecke_unten_rechts", in_corner(r));
            app.vis.toggle(now, 2.0);
            next(&mut probe);
        }
        10 if waited >= 1.0 => {
            let r = win::visible_rect(hwnd);
            let bar = (versekit_ui::overlay::BAR_HEIGHT * ctx.pixels_per_point()).round() as i32;
            probe.value("eingeklappt", overlay::geometry_text(r));
            probe.check("eingeklappt_auf_die_leiste", (r.h - bar).abs() <= 1);
            probe.check("eingeklappt_in_der_ecke", in_corner(r));
            // Counterpart of the ink checks: open without rows stays below the line.
            app.rows.clear();
            app.vis.toggle(now, 2.0);
            next(&mut probe);
        }
        11 if waited >= 1.0 => {
            let dots = ink(hwnd);
            probe.value("textpunkte_ohne_zeilen", dots);
            probe.check("ohne_zeilen_kaum_textpunkte", app.vis.shown(now) == Shown::Open && dots < INK);
            probe.check("platzhalter_ohne_zeilen", app.drawn == vec![t("ov_warte")]);
            // Fold and unfold with the bar below, frame by frame.
            probe.start_filming(app.region);
            next(&mut probe);
        }
        12 if waited >= 0.5 => {
            app.vis.toggle(now, 2.0);
            next(&mut probe);
        }
        13 if waited >= 1.0 => {
            // Folded: Windows shows only the bar, and a click beside it, in
            // the window as large as the open overlay, reaches what lies below.
            probe.region_is(hwnd, app.current, "bereich_eingeklappt_wie_die_leiste");
            let (bar, open) = (app.current, app.last_open);
            let beside = (open.x + open.w / 2, open.y + open.h / 2);
            let on_bar = (bar.x + bar.w / 2, bar.y + bar.h / 2);
            let below = win::window_at(beside.0, beside.1);
            probe.value("neben_der_leiste_trifft", if below == hwnd { "das Overlay" } else { "das Fenster darunter" });
            probe.check(
                "neben_der_leiste_klickt_durch",
                !bar.contains(beside.0, beside.1) && below != hwnd && win::window_at(on_bar.0, on_bar.1) == hwnd,
            );
            app.vis.toggle(now, 2.0);
            next(&mut probe);
        }
        14 if waited >= 1.0 => {
            probe.filming = false;
            probe.judge_folding();
            probe.judge_corners();
            probe.region_is(hwnd, app.current, "bereich_offen_wie_das_overlay");
            // The icon next to the clock first (stage 100 on), then back to the overview button.
            probe.stage = 100;
            probe.since = now;
        }
        15 => {
            // The overview button: a hidden window here plays the main window.
            let inside = ctx.content_rect();
            let button = app.buttons.iter().find(|(a, _)| *a == Action::Overview).map(|(_, r)| *r);
            probe.check("uebersicht_knopf_da", button.is_some_and(|r| inside.contains_rect(r)));
            probe.helper = win::hidden_window();
            let served = window_signal::serve(probe.helper, Role::Main, |signal| {
                let count = match signal {
                    Signal::Overview => &OVERVIEW_ASKED,
                    Signal::Close => &CLOSE_ASKED,
                    _ => return,
                };
                count.fetch_add(1, Ordering::Relaxed);
            });
            probe.check("hauptfenster_spielt_mit", served);
            app.act(ctx, Action::Overview);
            next(&mut probe);
        }
        // The messages to the stand-in window are handled by the event loop: under load they come
        // later than the half second, so the stages wait for what they count (or give up after four).
        16 if waited >= 0.5 && (OVERVIEW_ASKED.load(Ordering::Relaxed) >= 1 || waited >= 4.0) => {
            probe.value("uebersicht_signale", OVERVIEW_ASKED.load(Ordering::Relaxed));
            probe.check("uebersicht_holt_hauptfenster", OVERVIEW_ASKED.load(Ordering::Relaxed) == 1);
            // A second start of the program asks the running overlay (this one) for the main window.
            let handed = crate::hand_over();
            probe.value("zweiter_start_uebergeben", handed);
            next(&mut probe);
        }
        17 if waited >= 0.5 && (OVERVIEW_ASKED.load(Ordering::Relaxed) >= 2 || waited >= 4.0) => {
            probe.value("uebersicht_signale_nach_zweitem_start", OVERVIEW_ASKED.load(Ordering::Relaxed));
            probe.check("zweiter_start_holt_hauptfenster", OVERVIEW_ASKED.load(Ordering::Relaxed) == 2);
            // The overlay is closed by the menu of the icon (where there is one), else by its button:
            // the main window must close with it, and the icon must go.
            let quit =
                tray::menu(env!("CARGO_PKG_VERSION"), true).into_iter().find(|i| i.entry == Some(tray::Entry::Quit));
            match (probe.tray_window, quit) {
                (window, Some(item)) if window != 0 => {
                    tray::post(window, WM_COMMAND, item.command as usize, 0);
                }
                _ => app.act(ctx, Action::Quit),
            }
            next(&mut probe);
        }
        18 if waited >= 0.5
            && ((CLOSE_ASKED.load(Ordering::Relaxed) >= 1 && tray::icons_given_back() >= 1) || waited >= 4.0) =>
        {
            probe.value("schliess_signale", CLOSE_ASKED.load(Ordering::Relaxed));
            probe
                .check("schliessen_schliesst_hauptfenster", app.quit_asked && CLOSE_ASKED.load(Ordering::Relaxed) == 1);
            let removed = app.tray_log.as_ref().map_or(0, |r| r.count(NIM_DELETE));
            probe.value("symbol_entfernt_mal", removed);
            probe.check("beenden_ueber_das_menue", app.quit_asked);
            probe.check("symbol_wird_beim_beenden_entfernt", removed == 1 && app.tray.is_none());
            probe.check("fenster_des_symbols_weg", probe.tray_window != 0 && !tray::exists(probe.tray_window));
            // The icon was loaded from the file of the check and goes back to the system with the window.
            probe.value("symbole_zurueckgegeben", tray::icons_given_back());
            probe.check("geladenes_symbol_wird_freigegeben", tray::icons_given_back() == 1);
            win::destroy_window(probe.helper);
            if let Some(other) = window_signal::find(Role::Main) {
                // Another main window runs on this desktop: the button reaches that one.
                probe.value("uebersicht_startet", format!("entfällt, ein anderes Hauptfenster läuft ({other})"));
            } else {
                // Without a main window the button starts it (only noted here).
                app.act(ctx, Action::Overview);
                let missing = app.status.render() == t("ov_uebersicht_fehlt");
                let started = match &app.launched {
                    Some((program, args)) => format!("{} {}", program.display(), args.join(" ")),
                    None if missing => "kein Hauptprogramm, Hinweis steht da".to_string(),
                    None => "nichts".to_string(),
                };
                probe.value("uebersicht_startet", started);
                probe.check("uebersicht_startet_hauptfenster", app.launched.is_some() || missing);
                // This fault acts only with a program given: without one it would pass unseen.
                if probe.counter.as_deref() == Some("hauptprogramm") {
                    probe.check("gegenprobe_mit_hauptprogramm", app.args().main_program.is_some());
                }
                // A program given with `--hauptprogramm` is started with exactly its arguments.
                if let Some(given) = app.args().main_program.clone() {
                    // A relative program lies next to the overlay.
                    let folder = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
                    let given = match folder {
                        Some(folder) if given.is_relative() => folder.join(given),
                        _ => given,
                    };
                    let wanted = (given, app.args().main_args.clone());
                    probe.check("zahnrad_startet_angegebenes_hauptprogramm", app.launched == Some(wanted));
                }
            }
            // Three placeholder extras, and the window as narrow as its bar allows.
            app.extras = ["star", "circle", "info"]
                .into_iter()
                .map(|icon| Extra {
                    action: Action::Extra(icon),
                    icon,
                    meaning: format!("Platzhalter {icon}"),
                    id: icon,
                    tone: Default::default(),
                })
                .collect();
            app.extras_kept = true;
            app.last_open.w = 0;
            next(&mut probe);
        }
        19 | 20 if waited >= 1.0 => {
            let narrow = probe.stage == 19;
            let label = if narrow { "schmal" } else { "breit" };
            let width = app.current.w as f32 / ctx.pixels_per_point();
            probe.value(&format!("leiste_{label}_breite"), width);
            probe.value(&format!("leiste_{label}_im_menue"), app.in_menu.len());
            let fixed = FIXED.iter().all(|a| app.buttons.iter().any(|(b, _)| b == a));
            probe.check(&format!("leiste_{label}_feste_knoepfe_da"), fixed);
            if narrow {
                probe.check("leiste_schmal_rest_im_menue", app.more.is_some() && !app.in_menu.is_empty());
                // As wide as an overlay may get here: every extra fits.
                app.last_open.w = (900.0 * ctx.pixels_per_point()) as i32;
            } else {
                let all = app.extras.iter().all(|e| app.buttons.iter().any(|(a, _)| *a == e.action));
                probe.check("leiste_breit_alle_direkt", all && app.more.is_none() && app.in_menu.is_empty());
            }
            let bar = ctx.content_rect();
            let cut = cut_off(&app.buttons, app.more, app.title_end, (bar.min.x, bar.max.x));
            probe.value(&format!("leiste_{label}_abgeschnitten"), cut.as_deref().unwrap_or("nichts"));
            probe.check(&format!("leiste_{label}_nichts_abgeschnitten"), cut.is_none());
            next(&mut probe);
            if narrow {
                app.probe = Some(probe);
                return;
            }
            app.extras.clear();
            // The pointer rests on the lock: its own hint must show, not the one of the bar.
            if let Some(lock) = app.lock {
                let ppp = ctx.pixels_per_point();
                let c = lock.center();
                let (x, y) = (app.canvas.x + (c.x * ppp) as i32, app.canvas.y + (c.y * ppp) as i32);
                app.pointer_override = Some(overlay::Pointer { hidden: false, x, y });
                app.injected.push(egui::Event::PointerMoved(c));
            }
        }
        21 if waited >= 2.0 => {
            let hint = app.hint.clone();
            probe.value("gezeigt_ueber_dem_schloss", hint.as_deref().unwrap_or("keiner"));
            probe.check("knopf_zeigt_eigenen_hinweis", hint == Some(t("hinweis_schloss_zu")));
            app.pointer_override = None;
            probe.check("groesse_erst_nach_dem_layout", !probe.early);
            // The running contracts: three lines, hint rows of the first, filled by hand here.
            app.contracts = sample_contracts();
            app.rows.clear();
            app.rows.push(
                Row::Hint { text: Message::Plain("Hinweis Eins".into()), contract: Some("Probe Eins".into()) },
                20,
            );
            app.rows.push(
                Row::Hint { text: Message::Plain("Hinweis Zwei".into()), contract: Some("Probe Zwei".into()) },
                20,
            );
            next(&mut probe);
        }
        22 if waited >= 1.0 => {
            let texts = app.contract_texts.clone();
            probe.value("leiste", format!("{texts:?}"));
            let wanted: Vec<String> = sample_contracts()
                .iter()
                .map(|c| c.text.render())
                .chain([t("ov_auftraege_kopf"), "Fliege los".into(), "Sammle die Kisten".into(), t("ov_a_unklar")])
                .collect();
            probe.check("auftraege_leiste_gezeichnet", wanted.iter().all(|w| texts.contains(w)));
            probe.check("graue_zeile_ohne_ziele", !texts.iter().any(|x| x == "Nicht gezeigt"));
            probe.check("ein_ausblenden_knopf_je_zeile", app.hide_buttons.len() == app.contracts.len());
            let inside = ctx.content_rect();
            probe.check("ausblenden_knoepfe_im_fenster", app.hide_buttons.iter().all(|r| inside.contains_rect(*r)));
            let dots = ink(hwnd);
            probe.value("textpunkte_mit_leiste", dots);
            probe.check("inhalt_mit_leiste", dots >= INK);
            // The ban button of the first line, clicked as the window gets it.
            if let Some(button) = app.hide_buttons.first() {
                app.injected.extend(click_events(button.center()));
            }
            next(&mut probe);
        }
        23 if waited >= 0.8 => {
            let titles: Vec<&str> = app.contracts.iter().map(|c| c.title.as_str()).collect();
            probe.value("auftraege_danach", format!("{titles:?}"));
            probe.check("ausblenden_nimmt_auftrag_heraus", titles == ["Probe Zwei", "Probe Drei"]);
            let hints = app
                .rows
                .iter()
                .filter(|r| matches!(r, Row::Hint { contract: Some(c), .. } if c == "Probe Eins"))
                .count();
            probe.check("ausblenden_nimmt_den_hinweis_mit", hints == 0 && app.rows.len() == 1);
            // The way back: the line says which contract was hidden, and its button puts the line back.
            let said = tf("ov_a_ausgeblendet", &[("0", "Probe Eins".into())]);
            probe.check("zeile_mit_dem_weg_zurueck", app.contract_texts.contains(&said) && app.unhide_button.is_some());
            if let Some(button) = app.unhide_button {
                app.injected.extend(click_events(button.center()));
            }
            probe.stage = 60;
            probe.since = now;
        }
        60 if waited >= 0.8 => {
            let titles: Vec<&str> = app.contracts.iter().map(|c| c.title.as_str()).collect();
            probe.value("auftraege_nach_rueckgaengig", format!("{titles:?}"));
            probe.check("rueckgaengig_bringt_die_zeile_zurueck", titles.len() == 3 && titles.contains(&"Probe Eins"));
            probe.check("zeile_mit_dem_weg_zurueck_ist_weg", app.unhide_button.is_none());
            app.contracts.clear();
            // Without data no way to start the game is known: no rocket. With one it stands in the bar, inside the window.
            probe.check("keine_rakete_ohne_startweg", !app.buttons.iter().any(|(a, _)| *a == Action::Launch));
            app.launcher = true;
            probe.stage = 61;
            probe.since = now;
        }
        61 if waited >= 0.8 => {
            let inside = ctx.content_rect();
            let rocket: Vec<egui::Rect> =
                app.buttons.iter().filter(|(a, _)| *a == Action::Launch).map(|(_, r)| *r).collect();
            probe.check("rakete_mit_startweg_im_fenster", rocket.len() == 1 && inside.contains_rect(rocket[0]));
            app.launcher = false;
            probe.stage = 24;
            probe.since = now;
        }
        24 if waited >= 0.8 => {
            probe.check("leere_leiste_zeigt_nichts", app.contract_texts.is_empty() && app.hide_buttons.is_empty());
            probe.check("nie_aktiviert", win::activations() == 0);
            probe.value("stil_reparaturen", win::style_repairs());
            probe.finish();
            next(&mut probe);
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        // The icon next to the clock, its recorder in place of the shell: nothing shows on a screen.
        // It comes before the closing of the overlay (stage 17), which takes the icon away.
        100 => {
            let recorder = app.tray_log.clone();
            let tray_window = app.tray.as_ref().map_or(0, tray::Tray::window);
            let first = recorder.as_ref().and_then(|r| r.calls().into_iter().find(|c| c.message == NIM_ADD));
            probe.value("ablagesymbol", format!("{first:?}"));
            probe.check(
                "ablagesymbol_angelegt",
                recorder.as_ref().is_some_and(|r| r.count(NIM_ADD) == 1)
                    && first.as_ref().is_some_and(|c| c.tip == t("hf_titel"))
                    && app.tray.as_ref().is_some_and(tray::Tray::placed),
            );
            probe.check("ablagesymbol_fenster_nicht_sichtbar", tray_window != 0 && !tray::is_visible(tray_window));
            // The focus guard of the overlay stays whole while the icon exists.
            probe.check(
                "fokusschutz_mit_symbol",
                win::asks_no_activation(hwnd) && win::has_noactivate(style) && win::activations() == 0,
            );
            // A stand-in for the main window counts what it is asked.
            probe.helper = win::hidden_window();
            window_signal::serve(probe.helper, Role::Main, |signal| {
                TRAY_SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(signal);
            });
            probe.tray_base = tray_asked(Signal::Overview);
            // A click with the left button on the icon, as Windows sends it.
            probe
                .check("klick_aufs_symbol_geschickt", tray::post(tray_window, tray::WM_TRAY, 0, WM_LBUTTONUP as isize));
            next(&mut probe);
        }
        101 if waited >= 0.8 && (tray_asked(Signal::Overview) > probe.tray_base || waited >= 4.0) => {
            probe.check("klick_aufs_symbol_holt_hauptfenster", tray_asked(Signal::Overview) == probe.tray_base + 1);
            // Each entry of the menu, chosen as Windows reports a choice: the number as a command.
            let tray_window = app.tray.as_ref().map_or(0, tray::Tray::window);
            let items = app.tray.as_ref().map_or_else(Vec::new, tray::Tray::menu_now);
            let shown: Vec<String> =
                items.iter().map(|i| if i.entry.is_none() { "-".into() } else { i.text.clone() }).collect();
            probe.value("menue", format!("{shown:?}"));
            use tray::Entry::{Discord, Donate, Open, Quit, Settings, Translation, Version};
            let entries: Vec<_> = items.iter().filter_map(|i| i.entry).collect();
            // No way to start the game known here (no data): no entry for the launcher.
            probe.check(
                "menue_hat_alle_punkte",
                !app.launcher && entries == [Open, Settings, Translation, Discord, Donate, Version, Quit],
            );
            for item in items.iter().filter(|i| i.entry.is_some_and(|e| e != tray::Entry::Quit)) {
                tray::post(tray_window, WM_COMMAND, item.command as usize, 0);
            }
            next(&mut probe);
        }
        102 if waited >= 1.0
            && (tray_asked(Signal::Overview) >= probe.tray_base + 2
                && [Signal::OpenGeneral, Signal::OpenTranslation, Signal::OpenAbout]
                    .iter()
                    .all(|signal| tray_asked(*signal) >= 1)
                || waited >= 5.0) =>
        {
            // Open brings the main window; settings, translation and version bring it on their page; the
            // chat and the donations open an address.
            probe.check("menue_oeffnet_hauptfenster", tray_asked(Signal::Overview) == probe.tray_base + 2);
            probe.check(
                "menue_oeffnet_seiten",
                [Signal::OpenGeneral, Signal::OpenTranslation, Signal::OpenAbout]
                    .iter()
                    .all(|signal| tray_asked(*signal) == 1),
            );
            probe.value("adressen", format!("{:?}", app.opened));
            probe.check(
                "menue_oeffnet_chat_und_spenden",
                app.opened == ["https://xharig.com/discord", "https://ko-fi.com/xharig"],
            );
            // A message of the system.
            app.notices.push(("Verse-Kit: Raffinerie".into(), "Raffinerie fertig: Gold".into()));
            next(&mut probe);
        }
        103 if waited >= 0.5 => {
            let sent = app.tray_log.as_ref().and_then(|r| r.calls().into_iter().find(|c| c.message == NIM_MODIFY));
            probe.value("systemmeldung", format!("{sent:?}"));
            probe.check(
                "systemmeldung_am_symbol",
                sent.as_ref().is_some_and(|c| {
                    c.flags & NIF_INFO != 0 && c.title == "Verse-Kit: Raffinerie" && c.text == "Raffinerie fertig: Gold"
                }),
            );
            // The taskbar comes back (the Explorer restarted): the icon is placed again.
            let tray_window = app.tray.as_ref().map_or(0, tray::Tray::window);
            tray::post(tray_window, tray::taskbar_created_message(), 0, 0);
            next(&mut probe);
        }
        104 if waited >= 0.8 => {
            let adds = app.tray_log.as_ref().map_or(0, |r| r.count(NIM_ADD));
            probe.value("symbol_angemeldet_mal", adds);
            probe.check("symbol_nach_explorer_neustart_wieder_da", adds == 2);
            // ⚠ Only the choices arrive here as the system reports them; the menu itself (right
            // click, foreground of the invisible window and its return) cannot be opened without a
            // person and is seen on a real screen.
            probe.check("fokusschutz_nach_den_menuepunkten", win::asks_no_activation(hwnd) && win::activations() == 0);
            // The window of the icon is noted: stage 18 sees it gone after the closing.
            probe.tray_window = app.tray.as_ref().map_or(0, tray::Tray::window);
            win::destroy_window(probe.helper);
            probe.helper = 0;
            probe.stage = 105;
            probe.since = now;
        }
        // A page asked for by the menu while the main window is not there yet waits for it.
        105 => {
            if window_signal::find(Role::Main).is_some() {
                probe.value("seite_wartet", "entfällt, ein anderes Hauptfenster läuft");
                probe.stage = 15;
                probe.since = now;
            } else {
                TRAY_SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
                app.open_main(Signal::OpenTranslation);
                probe.check("seite_wartet_auf_das_hauptfenster", app.page_waits());
                // The main window shows up (the started program would mark its window like this).
                probe.helper = win::hidden_window();
                window_signal::serve(probe.helper, Role::Main, |signal| {
                    TRAY_SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(signal);
                });
                next(&mut probe);
            }
        }
        106 if waited >= 0.5 && (tray_asked(Signal::OpenTranslation) >= 1 || waited >= 4.0) => {
            probe.check("seite_kommt_nach_dem_start_an", tray_asked(Signal::OpenTranslation) == 1 && !app.page_waits());
            win::destroy_window(probe.helper);
            probe.helper = 0;
            // Back to the overview button and the closing of the overlay.
            probe.stage = 15;
            probe.since = now;
        }
        _ => {}
    }
    app.probe = Some(probe);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows 10 refuses the request for no border harmlessly: the self-check there does not count it as a fault.
    #[test]
    fn only_windows_11_is_held_to_the_border_request() {
        assert!(!dwm_knows_border(Some("19045")), "Windows 10");
        assert!(dwm_knows_border(Some("22000")) && dwm_knows_border(Some("26200")), "Windows 11");
        assert!(dwm_knows_border(None) && dwm_knows_border(Some("unbekannt")), "unknown: strict");
    }
}
