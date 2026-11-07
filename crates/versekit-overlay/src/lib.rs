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

//! The Verse-Kit overlay above the game.
//!
//! `cargo run -p versekit-overlay --release` shows it with sample contents:
//! it reads none of the user's data and writes nothing.
//!
//! `versekit-overlay --daten <folder>` runs it on a copy of the data
//! (`<folder>` stands in for the user folder as in the preview): the finds
//! come from the Game.log of the game folder set there (only read), the
//! collection, the read position and the place of the overlay are written
//! inside `<folder>` only.
//!
//! The overlay is the way in: its button "Übersicht öffnen" brings the main
//! window to the front or starts it (`vorschau` next to the overlay or in its
//! `examples` folder, with the same `--daten`). Closing the overlay, by its
//! button or by the system, closes the main window it started (or one of its
//! own process); closing the main window leaves the overlay running. A
//! second start on the same data leaves the running overlay alone and only
//! asks it for the main window; when that overlay cannot be reached, the
//! start goes on. A start on another copy of the data is a program of its own.
//!
//! `--hauptprogramm <program>` starts that program as the main window instead
//! (a relative path lies next to the overlay), with the arguments given by
//! `--hauptprogramm-arg <argument>` (repeated, in order) and nothing else.
//! The program marks its window with `versekit_ui::sample::MainSignals`;
//! without the mark every click starts it anew and closing the overlay does
//! not reach it.
//!
//! More switches: `--spiel` (the game counts as running), `--modus popup`,
//! `--ecke unten-rechts`, `--leiste unten`, `--update 5.0.1` (shows the
//! update hint), `--probe <file>` (the self-check, for the invisible
//! desktop; under X11 the shorter one of `probe_x11`, for a virtual screen, with the counter checks
//! `fokus|ohne_region`), `--lage BxH+X+Y` (a remembered place for the self-check),
//! `--langsam <ms>` (with `--probe`: every file access takes that much longer,
//! as on a slow network share), `--steuerung <file>` (watches a small file of
//! switches and takes them over at once, see `versekit_core::overlay_control`),
//! `--archiv <file>` (the game's `Data.p4k` where it does not lie in the game
//! folder; the finds read the catalog from it),
//! `--gegenprobe leer|fokus|rahmen|schreiben|altes_mass|frueh|groesse|ohne_region|
//! sofort_klein|spaet_gross|uebersicht|einstellung|frei|menue|im_bildfaden|
//! schloss|kein_anstoss|hauptprogramm|hauptfenster_bleibt|zweiter_start|ziehhinweis|archiv|ohne_dcomp` (a
//! deliberate fault for the counter check of the self-check; `ohne_dcomp` lets every stage through
//! DirectComposition fail, so the overlay draws opaque).
//!
//! The overlay is a library too: another program runs the same overlay with
//! [`main`], read from its own command line. Such a program sets
//! `windows_subsystem = "windows"` itself, or a console opens with it. A
//! program that is overlay and main window in one file decides before
//! calling [`main`]: [`main`] hands every start over to a running overlay.

mod afk;
mod app;
mod files;
mod fps;
#[cfg(windows)]
mod probe;
mod probe_x11;
mod refinery;
#[cfg(windows)]
mod scan_window;
mod scanner;
#[cfg(windows)]
mod tray;
mod watcher;
#[cfg(windows)]
mod win;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use versekit_core::diagnostics;
use versekit_core::paths::{Host, Paths};
use versekit_core::window_signal::{self, Role, Signal};
use versekit_core::{font_size, language, theme, trail, writeguard};

/// Set by [`ask_to_quit`]; the overlay ends at its next frame.
static QUIT_ASKED: AtomicBool = AtomicBool::new(false);
/// The window of the overlay of this process, to wake it.
static CONTEXT: OnceLock<eframe::egui::Context> = OnceLock::new();

/// Asks the overlay of this process to end as the player ends it (it closes its main window too): an update was
/// handed over and the program file is to be replaced.
pub fn ask_to_quit() {
    QUIT_ASKED.store(true, Ordering::Relaxed);
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

/// What the thread of the updates told the overlay since its last frame.
#[derive(Default)]
struct Told {
    /// The version on offer: `Some(None)` is „nothing newer“.
    update: Option<Option<String>>,
    /// Sentences for the status line: (text key, its argument).
    say: Vec<(String, String)>,
}

static TOLD: std::sync::Mutex<Told> = std::sync::Mutex::new(Told { update: None, say: Vec::new() });

fn told() -> std::sync::MutexGuard<'static, Told> {
    TOLD.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wake() {
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

/// Tells the overlay of this process which version is on offer, shown without the `v` of the tag: the bell
/// in the bar names it, and the line in the game. `None`: nothing newer.
pub fn set_update(version: Option<String>) {
    told().update = Some(version);
    wake();
}

/// Tells the overlay a sentence for its status line: the text of `key` with `argument` in place of `{0}`.
pub fn tell(key: &str, argument: &str) {
    told().say.push((key.to_string(), argument.to_string()));
    wake();
}

/// What the command line asks for; built by [`Args::parse`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Args {
    pub data: Option<String>,
    pub game: bool,
    pub mode: Option<String>,
    pub corner: Option<String>,
    pub bar: Option<String>,
    pub update: Option<String>,
    pub probe: Option<PathBuf>,
    pub counter: Option<String>,
    /// A remembered place for the self-check (`BxH+X+Y`, points).
    pub place: Option<String>,
    /// For the self-check: every file job takes this many milliseconds more.
    pub slow: Option<u64>,
    /// The file of live switches that is watched (`--steuerung`).
    pub control: Option<String>,
    /// How long a find stays open, over the settings; only the control file sets it.
    pub seconds: Option<i64>,
    /// The game's `Data.p4k` given from outside (`--archiv`).
    pub archive: Option<String>,
    /// The program of the main window and its arguments, if not the preview.
    pub main_program: Option<PathBuf>,
    pub main_args: Vec<String>,
    /// Runs on the user's own data (`--echt`): the settings, the files and the game folder are those of the
    /// user, and writing is not locked. Without it and without `--daten` the overlay shows sample contents
    /// and reads and writes nothing. The program `versekit` sets it when it is started without `--daten`.
    pub real: bool,
    /// What became of the last update, told as a message of the system once the icon stands. The program
    /// `versekit` sets it at the start after an update.
    pub notice: Option<String>,
    /// Open the main window on the overview at the start (after an update that asks for it).
    pub open_overview: bool,
}

impl Args {
    /// The switches in `args` (without the program name); unknown ones are passed over.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Args {
        let mut out = Args::default();
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--daten" => out.data = it.next(),
                "--spiel" => out.game = true,
                "--modus" => out.mode = it.next(),
                "--ecke" => out.corner = it.next(),
                "--leiste" => out.bar = it.next(),
                "--update" => out.update = it.next(),
                "--probe" => out.probe = it.next().map(PathBuf::from),
                "--gegenprobe" => out.counter = it.next(),
                "--lage" => out.place = it.next(),
                "--langsam" => out.slow = it.next().and_then(|ms| ms.parse().ok()),
                "--steuerung" => out.control = it.next(),
                "--archiv" => out.archive = it.next(),
                "--hauptprogramm" => out.main_program = it.next().map(PathBuf::from),
                "--hauptprogramm-arg" => out.main_args.extend(it.next()),
                "--echt" => out.real = true,
                _ => {}
            }
        }
        out
    }
}

/// The key of the run `args` ask for ([`window_signal::key_for`]): a copy of the data, a data folder named from
/// outside, or the user's own data. A run without data (sample contents, a self-check without a copy) has the
/// key of the samples.
pub fn instance_key(args: &Args) -> String {
    if args.data.is_none() && !args.real {
        return window_signal::SAMPLE.to_string();
    }
    let home = std::env::var(versekit_core::legacy::ENV_HOME).ok();
    window_signal::key_for(args.data.as_deref(), home.as_deref())
}

/// A second start: asks the overlay running on this desktop for the main
/// window. `false` when none runs or it cannot be reached.
pub fn hand_over() -> bool {
    window_signal::find(Role::Overlay).is_some_and(|running| window_signal::send(running, Signal::Overview))
}

/// Whether this is the first start of this run (sign-in, data folder; under Windows also the desktop): it holds
/// the single instance until it ends, a named mutex under Windows, a locked file elsewhere
/// (`window_signal::first_instance`, which also lets the new version of an update wait for the old one).
fn first_instance() -> bool {
    #[cfg(windows)]
    {
        win::first_instance()
    }
    #[cfg(not(windows))]
    {
        window_signal::first_instance()
    }
}

/// A start while the program runs already on this desktop on the same data (the caller binds the run first,
/// `window_signal::bind` with [`instance_key`]): true when it was handed over (the overlay brings
/// up the main window), a window of a run that has no overlay yet (the backup of the first start, the setup
/// assistant) was brought to the front, or the window found does not answer and the player was told so: this start
/// has nothing left to do. The first start of a sign-in takes the single instance here and goes on; a later one
/// waits a little for the window of the first. ⚠ Never an end without a word: a window that takes no messages
/// would neither come to the front nor bring up the main window.
/// ⚠ Before anything else a start does (the assistant of a first start too): two quick starts would both get
/// through otherwise. `by_main_window`: the overlay was started by the main window of this run (the end of the
/// assistant, a restart); that main window holds the single instance and is no other run to hand over to.
pub fn already_running(by_main_window: bool) -> bool {
    let found = look_for_first(
        first_instance(),
        by_main_window,
        || window_signal::find(Role::Overlay),
        || window_signal::find(Role::Main).or_else(|| window_signal::find(Role::FirstBackup)),
        || std::thread::sleep(std::time::Duration::from_millis(200)),
    );
    if found == Found::Silent {
        NO_ANSWER.store(true, std::sync::atomic::Ordering::Relaxed);
        trail_start_notes();
    }
    let send = |overlay| window_signal::send(overlay, Signal::Overview);
    settle(found, window_signal::answers, send, window_signal::bring_to_front, tell_no_answer)
}

/// What a start does with what it found of a running start ([`look_for_first`]); true when it has nothing left to
/// do. A window that does not take its messages (`answers`) is told to the player (`tell`). An overlay that
/// answers is asked for the main window (`hand_over`); when it cannot be reached (it runs with more rights) this
/// start goes on by itself, as the overlay does for a start after the first. Any other window that answers is
/// brought to the front (`raise`).
fn settle(
    found: Found,
    answers: impl Fn(isize) -> bool,
    hand_over: impl FnOnce(isize) -> bool,
    raise: impl FnOnce(isize),
    tell: impl FnOnce(),
) -> bool {
    match found {
        Found::Overlay(window) | Found::Window(window) if !answers(window) => {
            tell();
            true
        }
        Found::Overlay(overlay) => hand_over(overlay),
        Found::Window(window) => {
            raise(window);
            true
        }
        Found::Silent | Found::Nothing => false,
    }
}

/// Tells the player that VerseKit runs already but its window does not answer, in a message box of the system
/// (Windows) or a message of the desktop.
fn tell_no_answer() {
    let title = language::window_title(&language::t("zweitstart_haengt_titel"));
    let text = language::t("zweitstart_haengt_text");
    #[cfg(windows)]
    win::message_box(&title, &text);
    #[cfg(not(windows))]
    versekit_core::notify::show(&title, &text);
}

/// The line of the start trail for a start that found the single instance taken and no window of it after waiting.
pub const NO_ANSWER_LINE: &str = "Einzelstart: Sperre belegt, nach dem Warten kein Fenster gefunden, Start geht weiter";

/// Set by [`already_running`] when no window answered; the line waits for the trail of this start.
static NO_ANSWER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The main window keeps the game's texts itself while no overlay runs (`watcher::keep_texts_alone`).
pub fn keep_texts_without_overlay(
    window: &versekit_core::paths::Paths,
    guard: versekit_core::writeguard::State,
    overlay_runs: impl Fn() -> bool + Send + 'static,
) {
    watcher::keep_texts_alone(window, guard, overlay_runs);
}

/// Writes what [`already_running`] noted into the trail, once its diagnostics are set up (before that the trail
/// takes nothing, and the note stays for a later call).
pub fn trail_start_notes() {
    if versekit_core::diagnostics::current().is_some() && NO_ANSWER.swap(false, std::sync::atomic::Ordering::Relaxed) {
        trail::step(NO_ANSWER_LINE);
    }
}

/// What a start found of the run that holds the single instance ([`look_for_first`]).
#[derive(Debug, PartialEq, Eq)]
enum Found {
    /// Its overlay.
    Overlay(isize),
    /// A window of it without an overlay: the backup of the first start, or the main window with the setup
    /// assistant.
    Window(isize),
    /// Nothing, although the single instance was taken and this start waited for its window: this start goes on.
    Silent,
    /// Nothing: this start goes on.
    Nothing,
}

/// The looks of a start for the run that holds the mutex. `first`: this start took the mutex; `by_main_window`:
/// the main window of this very run started it, so its window is not another run's and there is nothing to wait
/// for. Any other start while the mutex is taken waits for a window of the first one (`pause` between the
/// looks): its overlay, else `other` (the backup of the first start, the main window with the assistant). It may
/// need some seconds to stand (the first start of a computer, a slow disk), and a start that gave up too early
/// would build a second overlay beside it.
fn look_for_first(
    first: bool,
    by_main_window: bool,
    overlay: impl Fn() -> Option<isize>,
    other: impl Fn() -> Option<isize>,
    pause: impl Fn(),
) -> Found {
    let tries = if first || by_main_window { 1 } else { 75 };
    for n in 0..tries {
        if let Some(window) = overlay() {
            return Found::Overlay(window);
        }
        if !first
            && !by_main_window
            && let Some(window) = other()
        {
            return Found::Window(window);
        }
        if n + 1 < tries {
            pause();
        }
    }
    if first || by_main_window { Found::Nothing } else { Found::Silent }
}

/// Runs the overlay as the command line of this program asks.
pub fn main() -> eframe::Result {
    run(Args::parse(std::env::args().skip(1)))
}

/// Runs the overlay as `args` ask (a program that is overlay and main window in one file builds them itself).
pub fn run(args: Args) -> eframe::Result {
    // The windows, the signals and the one overlay of this run belong to its data folder.
    window_signal::bind(&instance_key(&args));
    // Before the scale and any place are asked.
    #[cfg(windows)]
    win::dpi_aware();
    // An overlay runs already: it brings up the main window, no second one
    // starts. One started a moment before may not have its window yet: a
    // later start waits for it a little.
    if args.probe.is_none() {
        let tries = if first_instance() { 1 } else { 10 };
        for n in 0..tries {
            let found = window_signal::find(Role::Overlay).map_or(Found::Nothing, Found::Overlay);
            let send = |overlay| window_signal::send(overlay, Signal::Overview);
            if settle(found, window_signal::answers, send, |_| {}, tell_no_answer) {
                return Ok(());
            }
            if n + 1 < tries {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    // Where no close message of the system reaches this window from another process (all but Windows), the main
    // window's request to close (an update, a restart) ends the overlay as the player ends it.
    window_signal::on_close(ask_to_quit);
    // The whole run writes inside the copy only, or nowhere. On the user's own data (`--echt`) nothing is locked.
    let paths = if args.real && args.data.is_none() {
        Some(Paths::new(Host::current()))
    } else {
        writeguard::deny(true);
        writeguard::allow_under(args.data.as_deref());
        args.data.as_deref().and_then(|root| Paths::for_copy(Host::current(), root))
    };
    // Scheme, size and language as the copy says; the samples in German.
    let mut points = 1;
    match &paths {
        Some(paths) => {
            // The main window writes the same files: one write lock for both processes.
            paths.share_saves();
            let data = paths.settings();
            theme::set_current(theme::choice_of(&data).id);
            points = font_size::font_points(&data, &paths.host);
            language::set_language(language::choice_of(&data));
        }
        None => {
            theme::set_current(theme::DEFAULT);
            language::set_language(language::Choice::De);
        }
    }
    // Error log, crash file and start trail of the overlay, in the data it runs on; the samples and the
    // self-check write none. A program that set them up before (the program `versekit`) keeps its own.
    if let Some(paths) = paths.as_ref().filter(|_| args.probe.is_none()) {
        let own: &'static Paths = Box::leak(Box::new(paths.working_copy()));
        diagnostics::init_for(own, diagnostics::Role::Overlay, env!("CARGO_PKG_VERSION"));
    }
    trail_start_notes();
    trail::step("Overlay wird gebaut");
    app::run(args, paths, points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn the_overlay_the_main_window_starts_goes_on_and_a_second_start_brings_that_window_to_the_front() {
        let pauses = Cell::new(0);
        let pause = || pauses.set(pauses.get() + 1);
        // The end of the assistant of a first start: its main window holds the mutex, no overlay yet. The overlay
        // it starts goes on, without waiting.
        assert_eq!(look_for_first(false, true, || None, || Some(7), pause), Found::Nothing);
        assert_eq!(pauses.get(), 0, "nothing to wait for");
        // A second start of the player meanwhile: the window of the assistant comes to the front, the start ends.
        assert_eq!(look_for_first(false, false, || None, || Some(7), pause), Found::Window(7));
        // An overlay there: handed over, whoever started this one.
        for by_main_window in [false, true] {
            assert_eq!(look_for_first(false, by_main_window, || Some(3), || Some(7), pause), Found::Overlay(3));
        }
        // The first start of a sign-in goes on at once, a main window found or not.
        assert_eq!(look_for_first(true, false, || None, || Some(7), pause), Found::Nothing);
        assert_eq!(pauses.get(), 0);
        // A later start that finds no window of the first one waits for it, then goes on by itself.
        assert_eq!(look_for_first(false, false, || None, || None, pause), Found::Silent);
        assert_eq!(pauses.get(), 74, "75 looks with a pause between them");
    }

    /// Only a start that found the single instance taken and waited in vain is noted for the trail: the first
    /// start and the overlay of the main window of this run go on without a note, as does a start that found a
    /// window late.
    #[test]
    fn a_start_that_waited_in_vain_is_told_apart() {
        let pause = || {};
        assert_eq!(look_for_first(false, false, || None, || None, pause), Found::Silent);
        assert_eq!(look_for_first(true, false, || None, || None, pause), Found::Nothing);
        assert_eq!(look_for_first(false, true, || None, || None, pause), Found::Nothing);
        assert_eq!(look_for_first(true, true, || None, || None, pause), Found::Nothing);
        // The window of the first start comes on the last look: no note.
        let looks = std::cell::Cell::new(0);
        let late = || {
            looks.set(looks.get() + 1);
            (looks.get() == 75).then_some(5)
        };
        assert_eq!(look_for_first(false, false, late, || None, pause), Found::Overlay(5));
    }

    /// What `settle` did: the windows handed over to, brought to the front, and whether the player was told.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Done {
        handed: Vec<isize>,
        raised: Vec<isize>,
        told: bool,
    }

    /// `settle` with windows that answer when `answering` holds them and an overlay that takes the signal when
    /// `reached`: what it gave back and what it did.
    fn settled(found: Found, answering: &[isize], reached: bool) -> (bool, Done) {
        let done = std::cell::RefCell::new(Done::default());
        let ended = settle(
            found,
            |window| answering.contains(&window),
            |overlay| {
                done.borrow_mut().handed.push(overlay);
                reached
            },
            |window| done.borrow_mut().raised.push(window),
            || done.borrow_mut().told = true,
        );
        (ended, done.into_inner())
    }

    /// ⚠⚠ A second start never ends without a word (4.12.5: the assistant lay hidden after the setup, and every
    /// further start ended silently). (a) The assistant, (b) the window of the backup of the first start: the window
    /// comes to the front. (c) A window that hangs: the player is told.
    #[test]
    fn a_second_start_brings_the_open_window_to_the_front_and_tells_of_one_that_hangs() {
        let raised = |window| Done { raised: vec![window], ..Done::default() };
        // (a), (b): the main window with the assistant, the window of the backup.
        assert_eq!(settled(Found::Window(7), &[7], true), (true, raised(7)), "brought to the front, the start ends");
        // (c): such a window hangs.
        let told = Done { told: true, ..Done::default() };
        assert_eq!(settled(Found::Window(7), &[], true), (true, told), "told, nothing raised");
        // The overlay answers: asked for the main window.
        let handed = Done { handed: vec![3], ..Done::default() };
        assert_eq!(settled(Found::Overlay(3), &[3], true), (true, handed), "handed over");
        // (c): the overlay hangs: a signal would wait in its queue and nothing would come up.
        let told = Done { told: true, ..Done::default() };
        assert_eq!(settled(Found::Overlay(3), &[], true), (true, told), "told, not handed over");
        // It answers, but the signal does not reach it (it runs with more rights): this start goes on.
        let handed = Done { handed: vec![3], ..Done::default() };
        assert_eq!(settled(Found::Overlay(3), &[3], false), (false, handed));
        // (d) Nothing answered while the single instance was taken (another program holds it), and the first start:
        // this start goes on, it builds its own windows.
        for found in [Found::Silent, Found::Nothing] {
            assert_eq!(settled(found, &[], true), (false, Done::default()));
        }
    }
}
