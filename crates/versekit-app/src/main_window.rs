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

//! The main window of the program: the window model of `versekit_ui::sample` on the user's own data, or on a
//! copy of it (`--daten <folder>`; writing inside the copy only). It marks itself as the main window, so the
//! overlay finds it, brings it to the front on the overview and closes it with itself.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use versekit_core::autostart;
use versekit_core::paths::{Host, Paths};
use versekit_core::report_send::Target;
use versekit_core::trail::{self, PageTrail};
use versekit_core::window_signal::{self, Role};
use versekit_core::{font_size, screen, theme, writeguard};
use versekit_ui::sample::{MainSignals, SamplePage};

/// The options of a window of the program, centered, with the icon of the program, drawn by
/// [`versekit_ui::renderer`] (DX12 on Windows), on the window system [`screen::backend`] chooses: winit takes
/// Wayland wherever it is there, X11 is asked for by name, as for the overlay.
pub(crate) fn native_options(viewport: eframe::egui::ViewportBuilder) -> eframe::NativeOptions {
    let viewport = viewport.with_icon(versekit_ui::window_look::program_icon());
    let mut options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    versekit_ui::renderer::apply(&mut options);
    #[cfg(all(unix, not(target_os = "macos")))]
    if screen::backend() == screen::Backend::X11 {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11 as _;
            builder.with_x11();
        }));
    }
    options
}

struct MainWindow {
    page: SamplePage,
    /// The page changes and the frames that held the window, for the start trail.
    trail: PageTrail,
    /// The first frame was drawn: the start is over.
    running: bool,
    /// What the overlay asked for.
    signals: MainSignals,
    /// This is the start of a program that was not set up: the window opened with the assistant and the
    /// overlay follows its end. The copy of the data the run works on, to pass on to the overlay.
    first_start: Option<Option<String>>,
    /// The copy of the data the run works on, to start the program anew on it.
    data: Option<String>,
    /// A restore asked for the program to start anew: the helper is at work until this is set.
    restarting: Option<Arc<AtomicBool>>,
    /// The title bar of the system, dark or light with the scheme.
    title_bar: versekit_ui::window_look::TitleBar,
    /// The window was told to end for good (an update, a restart, the end of the assistant): closing it is not
    /// hiding it any more.
    ending: bool,
    /// The handle of the window for the system (0 where it has none), to ask whether it is on screen.
    hwnd: isize,
    /// The player closed the window and it went out of sight; the overlay that could bring it back is watched.
    hidden: bool,
    /// Where the window is still to be put in the first frames.
    placing: Option<Placing>,
}

/// The exact place of the window in the pixels of the whole desktop, set in the first frames: first the place alone
/// (the window may change its scaling when it moves to another screen, and then takes another size), then the whole
/// rectangle.
struct Placing {
    area: screen::Area,
    frames: u8,
}

/// A window that is put at its place by pixels is not centered by the options: the middle of the main screen would
/// overrule the place.
fn keep_place(options: &mut eframe::NativeOptions, placed: bool) {
    options.centered &= !placed;
}

/// The place the window opens at (x, y, width, height in desktop pixels), if it is placed by pixels: it has a frame of
/// its own, is not maximized, and the screens can be asked. The screen is the one that holds the remembered place, so
/// every screen counts with its own scaling; the whole window lies in its work area.
fn planned_place(place: &versekit_ui::sample::WindowPlace, own: bool) -> Option<screen::Area> {
    if !own || place.maximized {
        return None;
    }
    let at = place.position.map(|(x, y)| (x as i32, y as i32));
    let size = (place.size.0 as i32, place.size.1 as i32);
    screen::plan_window(at, size, &|x, y| screen::monitor_area(x, y, true), screen::primary_area(true))
}

/// What the closing of the window (its button, Alt+F4) comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closing {
    /// The window closes and the program with it.
    Close,
    /// The window goes out of sight and the program runs on; the overlay brings it back.
    Hide,
    /// The window stays as it is.
    Keep,
}

/// The way a request to close the window is answered. `ending`: the window was told to end for good;
/// `saving`: a backup is being written, which a cut would break; `restarting`: the program is starting anew;
/// `way_back`: an overlay runs that can bring the window back (its gear, its icon, a second start).
fn closing(ending: bool, saving: bool, restarting: bool, way_back: bool) -> Closing {
    if restarting {
        Closing::Close
    } else if saving {
        Closing::Keep
    } else if ending {
        Closing::Close
    } else if way_back {
        Closing::Hide
    } else {
        Closing::Close
    }
}

/// The line of the trail for a request to close the window: what it comes to, and whether the overlay asked for it.
fn closing_line(what: Closing, by_overlay: bool) -> String {
    let who = if by_overlay { "Overlay bittet zu schließen" } else { "Fenster wird geschlossen" };
    let result = match what {
        Closing::Close => "endet",
        Closing::Hide => "geht in die Ablage",
        Closing::Keep => "bleibt (Sicherung läuft)",
    };
    format!("{who}: {result}")
}

/// What a window out of sight does, looked at every [`OVERLAY_WATCH`].
#[derive(Debug, PartialEq, Eq)]
enum Watch {
    /// The window is on screen again: nothing to watch.
    Shown,
    /// Hidden, and no overlay runs that could bring it back: the window ends.
    Alone,
    /// Hidden, the overlay runs: looked at again later.
    Wait,
}

/// How often a hidden window looks whether its overlay still runs.
const OVERLAY_WATCH: std::time::Duration = std::time::Duration::from_secs(1);

/// What a window out of sight does: `shown` whether it is on screen (then it is not hidden any more), `overlay` the
/// overlay of the run, if one runs.
fn hidden_without_overlay(shown: bool, overlay: Option<isize>) -> Watch {
    if shown {
        Watch::Shown
    } else if overlay.is_none() {
        Watch::Alone
    } else {
        Watch::Wait
    }
}

/// Does the player's closing only take the window out of sight? Where the overlay runs (`overlay`) and it has its
/// icon next to the clock to bring the window back (`windows`). Elsewhere there is no such icon: the window ends,
/// as 4.12 does there, and the gear of the overlay or a second start opens a new one.
fn hides_on_close(windows: bool, overlay: bool) -> bool {
    windows && overlay
}

impl MainWindow {
    /// Ends the window for good, whatever the player's closing would come to.
    fn end(&mut self, ctx: &egui::Context) {
        self.ending = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for MainWindow {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // File dialogs open above this window.
        versekit_ui::file_dialog::remember_parent(frame);
        if !std::mem::replace(&mut self.running, true) {
            trail::step(trail::BOUNDARY);
        }
        if let Some(placing) = &mut self.placing {
            placing.frames += 1;
            match placing.frames {
                1 => {
                    screen::place_window(self.hwnd, placing.area, false);
                }
                3 => {
                    screen::place_window(self.hwnd, placing.area, true);
                    self.placing = None;
                }
                _ => {}
            }
            ui.ctx().request_repaint();
        }
        self.title_bar.follow(ui.ctx(), theme::current());
        self.signals.apply(ui.ctx(), &mut self.page);
        if let Some(line) = self.trail.before(self.page.current_page()) {
            trail::step(&line);
        }
        let drawing = std::time::Instant::now();
        self.page.show(ui);
        for line in self.trail.after(drawing.elapsed()) {
            trail::step(&line);
        }
        // An update was handed over: the helper waits for every process of the program file, so the overlay goes
        // first (it takes its main window with it) and this window follows.
        let handed_over = self.page.take_update_quit();
        let find = || window_signal::find(Role::Overlay);
        if end_for_update(handed_over, find, |hwnd| {
            window_signal::close_window(hwnd);
        }) {
            self.end(ui.ctx());
        }
        // A restored backup: the program starts anew, because its modules hold the old data in memory.
        if self.restarting.is_none() && self.page.take_restart() {
            // ⚠ The overlay, closing, asks this window to go with it: this window starts the program again
            // and must outlive that.
            self.signals.keep_open(true);
            self.restarting = Some(restart(self.data.clone()));
        }
        // The player closes the window: with an overlay running it only goes out of sight on Windows (the program
        // goes on, as 4.12 does with its overlay); no closing while the backup works, since a cut file would be
        // the result.
        if ui.input(|i| i.viewport().close_requested()) {
            let ending = self.ending || self.signals.ended();
            let way_back = !ending && hides_on_close(cfg!(windows), window_signal::find(Role::Overlay).is_some());
            let what = closing(ending, self.page.backup_working(), self.restarting.is_some(), way_back);
            trail::step(&closing_line(what, self.signals.ended()));
            match what {
                Closing::Close => {}
                Closing::Hide => {
                    self.hidden = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                Closing::Keep => ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose),
            }
        }
        // Out of sight, the window watches the overlay that can bring it back: with the overlay gone (ended without
        // reaching this window, or ended right after the player closed this one) nothing could show it again, and
        // it would hold the program file and its ports. Then it ends as the overlay would have ended it.
        if self.hidden {
            match hidden_without_overlay(window_signal::is_shown(self.hwnd), window_signal::find(Role::Overlay)) {
                Watch::Shown => self.hidden = false,
                Watch::Alone => {
                    trail::step("Fenster verborgen und kein Overlay mehr: endet");
                    self.end(ui.ctx());
                }
                Watch::Wait => ui.ctx().request_repaint_after(OVERLAY_WATCH),
            }
        }
        if let Some(done) = &self.restarting {
            if done.load(Ordering::Relaxed) {
                self.end(ui.ctx());
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if let Some(end) = self.page.take_setup_end()
            && let Some(data) = &self.first_start
        {
            // Run to its end, or left on a computer that was set up before (the game folder gone, the data
            // folder locked): the overlay follows. Only a program that was never set up ends with the
            // assistant, as 4.12 does.
            if end.finished || self.page.is_set_up() {
                start_overlay(data.as_deref());
            }
            if end.finished && end.overview {
                // The window stays: what it would have started on its own at the beginning starts now.
                self.page.start_hangar_delivery();
                self.page.start_contract_scan();
            } else {
                self.end(ui.ctx());
            }
        }
    }
}

/// The end of the window after an update was handed over (`handed_over`): a running overlay is asked to close as
/// the player would close it, then the window closes (the result). ⚠ The helper waits for every process of the
/// program file, so one that stays makes it wait its whole minute.
fn end_for_update(handed_over: bool, find: impl Fn() -> Option<isize>, close: impl Fn(isize)) -> bool {
    if !handed_over {
        return false;
    }
    if let Some(hwnd) = find() {
        close(hwnd);
    }
    true
}

/// Starts the overlay as a program of its own: this file, with the copy of the data ([`overlay_args`]).
fn start_overlay(data: Option<&str>) {
    let Ok(program) = std::env::current_exe() else { return };
    let _ = std::process::Command::new(program).args(overlay_args(data)).spawn();
}

/// The command line of the overlay this window starts: the mark that this window started it (this process holds
/// the single instance, see `versekit_overlay::already_running`), and the copy of the data if there is one.
fn overlay_args(data: Option<&str>) -> Vec<String> {
    let mut out = vec![crate::BY_MAIN_WINDOW.to_string()];
    if let Some(folder) = data {
        out.push("--daten".to_string());
        out.push(folder.to_string());
    }
    out
}

/// Starts the program anew after a restore: the overlay that runs is asked to close like its own window
/// would be (it carries the single instance), and once it is gone the program starts again as the overlay.
/// The work runs in a thread of its own; the flag is set when it is done and the main window may close.
fn restart(data: Option<String>) -> Arc<AtomicBool> {
    use std::time::Duration;
    use versekit_core::window_signal::{self, Role};
    let done = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&done);
    let started = std::thread::Builder::new().name("neustart".into()).spawn(move || {
        restart_steps(
            || window_signal::find(Role::Overlay),
            |hwnd| {
                window_signal::close_window(hwnd);
            },
            || start_overlay(data.as_deref()),
            // At most ten seconds for it to go; then a moment for its process to end and give up the mutex.
            (Duration::from_millis(250), 40, Duration::from_millis(1500)),
        );
        flag.store(true, Ordering::Relaxed);
    });
    if started.is_err() {
        done.store(true, Ordering::Relaxed);
    }
    done
}

/// The order of a restart: a running overlay is asked to close and waited for (`pause` between the looks, at
/// most `looks` of them, then `after` for its process to end), then the program starts. Without a running
/// overlay it starts at once.
fn restart_steps(
    find: impl Fn() -> Option<isize>,
    close: impl Fn(isize),
    start: impl FnOnce(),
    (pause, looks, after): (std::time::Duration, usize, std::time::Duration),
) {
    if let Some(hwnd) = find() {
        close(hwnd);
        for _ in 0..looks {
            if find().is_none() {
                break;
            }
            std::thread::sleep(pause);
        }
        std::thread::sleep(after);
    }
    start();
}

/// The folder after `--daten`, if given.
fn data_folder(args: &[String]) -> Option<String> {
    args.iter().position(|a| a == "--daten").and_then(|at| args.get(at + 1)).cloned()
}

/// The window title: the product name; a copy of the data says so.
fn title(copy: bool) -> &'static str {
    if copy { "VerseKit (Testkopie)" } else { "VerseKit" }
}

/// The title with the version of the program after the product name; a copy keeps its word behind it.
fn versioned_title(copy: bool, version: &str) -> String {
    match title(copy).strip_prefix("VerseKit") {
        Some(rest) => format!("VerseKit {version}{rest}"),
        None => title(copy).to_string(),
    }
}

/// The title as it stands in the bar: the version of the program, and a test build says so (`SC_BP_TESTFASSUNG`
/// of `host`).
fn shown_title(copy: bool, host: &Host) -> String {
    let text = versioned_title(copy, env!("CARGO_PKG_VERSION"));
    versekit_core::language::window_title_in(versekit_core::language::current(), &text, host)
}

/// The window as it opens before the remembered place is applied: title, the class every window of the program
/// carries (the desktop entry on Linux is matched against it), first and least size.
/// Whether the main window draws its own frame (the head bar as title bar, buttons of its own, edges to drag): not on
/// the Mac, where the frame of the system stays.
pub(crate) fn own_frame() -> bool {
    !cfg!(target_os = "macos")
}

fn base_viewport(copy: bool, (width, height): (f32, f32)) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_decorations(!own_frame())
        .with_title(shown_title(copy, &versekit_core::paths::global().host))
        .with_app_id(versekit_core::paths::WM_CLASS)
        .with_inner_size([width.max(1280.0), height.max(820.0)])
        .with_min_inner_size([width, height])
}

pub fn run(args: &[String]) -> eframe::Result {
    let root = data_folder(args);
    // The paths of the run: a copy of the data (then writing is locked to it), or the user's own.
    let paths = match root.as_deref() {
        Some(root) => {
            writeguard::deny(true);
            writeguard::allow_under(Some(root));
            Paths::for_copy(Host::current(), root)
        }
        None => Some(Paths::new(Host::current())),
    };
    // The overlay writes the same files: one write lock for both processes.
    if let Some(paths) = &paths {
        paths.share_saves();
        // The overlay keeps the game's texts; while none runs (switched off, ended) this window does it.
        versekit_overlay::keep_texts_without_overlay(paths, writeguard::state(), || {
            window_signal::find(Role::Overlay).is_some()
        });
    }
    // The scheme is decided by the settings that are loaded below.
    theme::set_current(theme::DEFAULT);
    let copy = root.is_some();
    let mut page = match paths {
        Some(paths) => SamplePage::with_settings(paths),
        None => SamplePage::preview(),
    };
    let wiring = wiring(args, crate::custom_home(), &Host::current());
    // On the user's own data the switch for the start with the system writes the real entry; on a copy it
    // writes into the copy, with a command that starts the program on the copy; elsewhere there is none.
    if wiring.own {
        page.use_autostart(Box::new(autostart::System), crate::autostart_command(&[]));
        // Only the player's own program updates itself; a copy of the data or a test run never replaces a file.
        page.use_updates(crate::updates::window_env(env!("CARGO_PKG_VERSION")));
    } else if !page.use_copy_autostart(
        &std::env::current_exe().unwrap_or_default(),
        std::env::var("APPIMAGE").ok().as_deref(),
    ) {
        page.use_autostart(Box::new(autostart::Memory::impossible()), String::new());
    }
    page.use_report_target(wiring.report);
    // After a restored backup the program starts itself anew (see `restart`).
    page.offer_restart(true);
    page.use_own_frame(own_frame());
    // The settings file may lie on a network share: written in a thread of its own.
    page.write_in_background();
    // The start of a program that is not set up: the assistant is the first thing the player sees. Nothing opens
    // a port or reads the logs before it has run to its end (the window starts both then, if it stays).
    let first_start = args.iter().any(|a| a == crate::FIRST_START).then(|| root.clone());
    if first_start.is_none() || !page.open_setup_if_needed() {
        // The browser extension can send the hangar (Mein Hangar › Kopplung verwalten).
        page.start_hangar_delivery();
        if args.iter().any(|a| a == "--test-empfaenger") {
            page.use_test_receiver();
        }
        // The mission log reads the game logs in the background.
        page.start_contract_scan();
    }
    // A program set up before 5.0 is welcomed once, short, with what it has filled in; the first start has its own
    // assistant, and the welcome stays away while the game runs.
    if first_start.is_none() {
        page.open_short_if_needed();
    }
    // The backup made at the first start of this version is told once (after the assistant, when one is open).
    page.open_first_backup_hint();
    let (width, height) = font_size::BASE_MIN_SIZE;
    let mut viewport = base_viewport(copy, (width, height));
    let mut placing: Option<Placing> = None;
    // Where the window was: size and place in points of the system scaling, the place only when it still lies
    // on a screen.
    if let Some(place) = page.window_place() {
        let opening =
            versekit_ui::sample::opening(&place, screen::system_scale(), (width, height), screen::on_a_screen);
        // Never larger than the screen it opens on: otherwise its title bar can lie out of reach.
        let work = match place.position {
            Some((x, y)) => screen::monitor_area(x as i32, y as i32, true),
            None => screen::primary_area(true),
        };
        let opening = versekit_ui::sample::fit_to_work_area(opening, work, screen::system_scale());
        viewport = viewport.with_inner_size(opening.size).with_maximized(opening.maximized);
        placing = planned_place(&place, own_frame() && cfg!(windows)).map(|area| Placing { area, frames: 0 });
        // Where the window is placed by pixels, no place in points comes along: one scale for all screens is exactly
        // what that avoids.
        if placing.is_none()
            && let Some(position) = opening.position
        {
            viewport = viewport.with_position(position);
        }
    }
    let mut options = native_options(viewport);
    keep_place(&mut options, placing.is_some());
    let language = page.start_language();
    trail::step("Hauptfenster wird gebaut");
    versekit_ui::renderer::run_native(
        "versekit",
        options,
        versekit_ui::renderer::Look::Opaque,
        Box::new(move |cc| {
            // The stored language on real data; without this the names from the language file follow the
            // system language.
            versekit_core::language::set_language(language);
            versekit_ui::install(&cc.egui_ctx);
            // The overlay finds this window as the main window.
            let signals = MainSignals::serve(cc, &cc.egui_ctx);
            let hwnd = versekit_ui::sample::system_handle(cc);
            // Without a handle the window can not be placed by pixels.
            let placing = placing.filter(|_| hwnd != 0);
            trail::step("Hauptfenster steht");
            Ok(Box::new(MainWindow {
                page,
                trail: PageTrail::new(),
                running: false,
                signals,
                first_start,
                data: root,
                restarting: None,
                ending: false,
                hwnd,
                hidden: false,
                placing,
                title_bar: versekit_ui::window_look::TitleBar::default(),
            }))
        }),
    )
}

/// Where the report page sends to: the built-in target on the player's own data only; a copy of the data (`copy`)
/// and a data folder named from outside (`custom_home`) send nothing.
fn report_target(copy: bool, custom_home: bool, host: &Host) -> Target {
    Target::for_run(host, !copy && !custom_home)
}

/// What the main window of a start with `args` is wired to: `own` when it runs on the player's own data (the
/// real start with the system, the updates), and where its report goes ([`report_target`]).
struct Wiring {
    own: bool,
    report: Target,
}

fn wiring(args: &[String], custom_home: bool, host: &Host) -> Wiring {
    let copy = data_folder(args).is_some();
    Wiring { own: !copy && !custom_home, report: report_target(copy, custom_home, host) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// ⚠ Off Windows there is no icon next to the clock: a hidden window could only be found by a second start.
    #[test]
    fn closing_hides_the_window_only_where_the_icon_next_to_the_clock_brings_it_back() {
        assert!(hides_on_close(true, true), "Windows with an overlay: the icon brings it back");
        assert!(!hides_on_close(false, true), "Linux with an overlay: the window ends, the gear opens a new one");
        assert!(!hides_on_close(true, false) && !hides_on_close(false, false), "no overlay: the program ends");
        assert_eq!(closing(false, false, false, hides_on_close(false, true)), Closing::Close);
    }

    #[test]
    fn closing_the_window_hides_it_while_an_overlay_can_bring_it_back_and_ends_it_otherwise() {
        // (ending, saving, restarting, way_back)
        assert_eq!(closing(false, false, false, true), Closing::Hide, "the player's closing with an overlay");
        assert_eq!(closing(false, false, false, false), Closing::Close, "no way back: the program ends");
        assert_eq!(closing(true, false, false, true), Closing::Close, "told to end for good");
        assert_eq!(closing(true, false, false, false), Closing::Close);
        assert_eq!(closing(false, true, false, true), Closing::Keep, "a backup is being written");
        assert_eq!(closing(false, true, false, false), Closing::Keep);
        assert_eq!(closing(false, true, true, true), Closing::Close, "a restart goes on during the backup");
        assert_eq!(closing(false, false, true, true), Closing::Close, "the restart ends the window");
    }

    /// ⚠ A hidden window nobody can bring back would run on unseen, holding the program file and its ports.
    #[test]
    fn a_hidden_window_ends_once_no_overlay_runs_and_stops_watching_once_it_is_shown() {
        assert_eq!(hidden_without_overlay(false, Some(9)), Watch::Wait, "hidden, the overlay runs");
        assert_eq!(hidden_without_overlay(false, None), Watch::Alone, "hidden, no overlay: the window ends");
        assert_eq!(hidden_without_overlay(true, None), Watch::Shown, "on screen again, overlay or not");
        assert_eq!(hidden_without_overlay(true, Some(9)), Watch::Shown);
    }

    #[test]
    fn the_trail_says_who_asked_the_window_to_close_and_what_it_comes_to() {
        assert_eq!(closing_line(Closing::Close, true), "Overlay bittet zu schließen: endet");
        assert_eq!(closing_line(Closing::Hide, false), "Fenster wird geschlossen: geht in die Ablage");
        assert_eq!(closing_line(Closing::Keep, false), "Fenster wird geschlossen: bleibt (Sicherung läuft)");
    }

    #[test]
    fn the_main_window_carries_the_class_of_the_program_and_the_overlay_the_same() {
        // One class for both windows: the desktop entry on Linux is matched against it, and two classes would
        // be two icons in the panel.
        for copy in [false, true] {
            let viewport = base_viewport(copy, (880.0, 380.0));
            assert_eq!(viewport.app_id.as_deref(), Some("Versekit"), "copy {copy}");
        }
        // The class of 4.12: the entries it wrote in the start menu find the window by it.
        assert_eq!(versekit_core::paths::WM_CLASS, "Versekit");
    }

    /// The main window has no frame of the system where it draws its own (the head bar is its title bar); the other
    /// windows of the program keep theirs.
    #[test]
    fn only_the_main_window_has_no_frame_of_the_system() {
        let main = base_viewport(false, (880.0, 380.0));
        assert_eq!(main.decorations, Some(!own_frame()));
        assert_eq!(own_frame(), !cfg!(target_os = "macos"));
        for (name, viewport) in
            [("learn", crate::learn_window::viewport()), ("first backup", crate::first_backup_window::viewport())]
        {
            assert_ne!(viewport.decorations, Some(false), "{name} keeps the frame of the system");
        }
    }

    /// Three screens side by side: a small one upright at 100 % on the left that reaches far above the middle one,
    /// the middle one and the main screen on the right, each of these at 150 %.
    fn three_screens() -> screen::Setup {
        screen::Setup {
            scale: 1.5,
            primary: 2,
            monitors: vec![
                screen::Monitor { area: (-3640, -520, 1080, 1920), work: (-3640, -520, 1080, 1880) },
                screen::Monitor { area: (-2560, 0, 2560, 1440), work: (-2560, 0, 2560, 1400) },
                screen::Monitor { area: (0, 0, 2560, 1440), work: (0, 0, 2560, 1400) },
            ],
        }
    }

    #[test]
    fn the_window_opens_on_the_screen_of_its_place_in_the_pixels_of_the_desktop_with_its_top_edge_in_reach() {
        use versekit_ui::sample::WindowPlace;
        let _screens = screen::fixed(three_screens());
        let place = |position: Option<(i64, i64)>| WindowPlace { size: (1920, 1230), position, maximized: false };
        // On the middle screen: it stays there.
        assert_eq!(planned_place(&place(Some((-2000, 100))), true), Some((-2000, 100, 1920, 1230)));
        // Above the middle screen, where no screen is: on the middle screen, at the top of its work area.
        assert_eq!(planned_place(&place(Some((-2000, -300))), true), Some((-2000, 0, 1920, 1230)));
        // On the left screen, narrower than the window: it shrinks to it.
        assert_eq!(planned_place(&place(Some((-3600, -500))), true), Some((-3640, -500, 1080, 1230)));
        // No place stored: the middle of the main screen.
        assert_eq!(planned_place(&place(None), true), Some((320, 85, 1920, 1230)));
        // The frame of the system, or a maximized window, is not placed by pixels.
        assert_eq!(planned_place(&place(Some((-2000, 100))), false), None);
        let maximized = WindowPlace { maximized: true, ..place(Some((-2000, 100))) };
        assert_eq!(planned_place(&maximized, true), None);
    }

    #[test]
    fn a_window_placed_by_pixels_is_not_centered_over_its_place() {
        let mut options = native_options(base_viewport(false, (880.0, 380.0)));
        assert!(options.centered, "a window with no place is centered");
        keep_place(&mut options, true);
        assert!(!options.centered, "the centering would overrule the place");
        let mut other = native_options(base_viewport(false, (880.0, 380.0)));
        keep_place(&mut other, false);
        assert!(other.centered);
    }

    #[test]
    fn every_window_of_the_program_opens_with_the_icon_of_the_program() {
        let options = native_options(base_viewport(false, (880.0, 380.0)));
        let icon = options.viewport.icon.expect("an icon");
        assert!(std::sync::Arc::ptr_eq(&icon, &versekit_ui::window_look::program_icon()), "not the one of eframe");
    }

    /// OpenGL must never draw a window of the program, and on Windows wgpu must not open the OpenGL or the Vulkan
    /// driver either: with NVIDIA both are `nvoglv64.dll`, which crashed the start before the first frame. Every
    /// window of this crate takes its options from [`native_options`] and opens through
    /// `versekit_ui::renderer::run_native`, which chooses the stage of DX12 before the first frame.
    #[test]
    fn no_window_of_the_program_is_drawn_by_opengl() {
        let options = native_options(base_viewport(false, (880.0, 380.0)));
        let backends = versekit_ui::renderer::drawn_by(&options).expect("drawn by wgpu, not by OpenGL");
        assert_eq!(backends, versekit_ui::renderer::backends());
        #[cfg(windows)]
        assert_eq!(backends, eframe::egui_wgpu::wgpu::Backends::DX12, "DX12 alone on Windows");

        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut windows = 0;
        for entry in std::fs::read_dir(&src).expect("the sources") {
            let path = entry.expect("an entry").path();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(!text.contains(concat!("Renderer::", "Glow")), "{} asks for OpenGL", path.display());
            assert!(!text.contains(concat!("eframe::", "run_native(")), "{} skips the choice of the stage", path.display());
            if text.contains("renderer::run_native(") {
                windows += 1;
                assert!(text.contains("native_options("), "{} opens a window with options of its own", path.display());
            }
        }
        assert!(windows >= 3, "the main window, the learning window and the first backup: {windows}");
    }

    #[test]
    fn the_data_folder_is_the_word_after_the_switch() {
        assert_eq!(data_folder(&args(&["--hauptfenster", "--daten", "C:/copy"])), Some("C:/copy".to_string()));
        assert_eq!(data_folder(&args(&["--hauptfenster"])), None);
        assert_eq!(data_folder(&args(&["--daten"])), None, "a switch at the end has no folder");
    }

    #[test]
    fn a_running_overlay_is_asked_to_close_and_waited_for_before_the_program_starts_anew() {
        use std::cell::{Cell, RefCell};
        use std::time::Duration;
        let events = RefCell::new(Vec::<String>::new());
        let looks = Cell::new(0);
        let none = Duration::ZERO;
        restart_steps(
            || {
                // The overlay is there for the first three looks (before the close, and two while it goes).
                looks.set(looks.get() + 1);
                events.borrow_mut().push(format!("look {}", looks.get()));
                (looks.get() <= 3).then_some(7)
            },
            |hwnd| events.borrow_mut().push(format!("close {hwnd}")),
            || events.borrow_mut().push("start".to_string()),
            (none, 10, none),
        );
        assert_eq!(*events.borrow(), ["look 1", "close 7", "look 2", "look 3", "look 4", "start"]);
        // One that never goes: the wait ends after the looks, and the program starts all the same.
        let events = RefCell::new(Vec::<String>::new());
        restart_steps(
            || Some(1),
            |_| events.borrow_mut().push("close".into()),
            || events.borrow_mut().push("start".into()),
            (none, 3, none),
        );
        assert_eq!(*events.borrow(), ["close", "start"]);
        // No overlay: no close, the start at once.
        let events = RefCell::new(Vec::<String>::new());
        restart_steps(
            || None,
            |_| events.borrow_mut().push("close".into()),
            || events.borrow_mut().push("start".into()),
            (none, 3, none),
        );
        assert_eq!(*events.borrow(), ["start"]);
    }

    #[test]
    fn the_window_ends_with_its_overlay_after_an_update_was_handed_over_and_not_before() {
        use std::cell::RefCell;
        let closed = RefCell::new(Vec::<isize>::new());
        assert!(!end_for_update(false, || Some(9), |h| closed.borrow_mut().push(h)), "no hand-over, no end");
        assert!(closed.borrow().is_empty(), "the overlay stays while nothing was handed over");
        assert!(end_for_update(true, || Some(9), |h| closed.borrow_mut().push(h)));
        assert_eq!(*closed.borrow(), [9], "the overlay is asked to close");
        assert!(end_for_update(true, || None, |h| closed.borrow_mut().push(h)), "without an overlay the window ends");
        assert_eq!(closed.borrow().len(), 1);
    }

    #[test]
    fn the_overlay_this_window_starts_is_marked_as_its_own_and_runs_on_the_same_copy() {
        assert_eq!(overlay_args(None), ["--vom-hauptfenster"]);
        assert_eq!(overlay_args(Some("C:/copy")), ["--vom-hauptfenster", "--daten", "C:/copy"]);
        // The mark is no data switch: a start with it is not refused.
        assert!(!crate::refuses(&overlay_args(Some("C:/copy"))));
    }

    #[test]
    fn the_report_is_sent_only_from_the_players_own_data() {
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], "/");
        assert!(report_target(false, false, &host).available(), "own data: the report can be sent");
        assert_eq!(report_target(false, false, &host), Target::from_host(&host));
        assert!(!report_target(true, false, &host).available(), "a copy of the data sends nothing");
        assert!(!report_target(false, true, &host).available(), "a data folder named from outside sends nothing");
        assert!(!report_target(true, true, &host).available());
    }

    /// The window's own start line by line: the copy of the data comes from its switches, as `run` reads them.
    #[test]
    fn the_window_of_a_copy_sends_no_report_and_touches_nothing_of_the_player() {
        let host = Host::new(versekit_core::pypath::Flavor::native(), &[], "/");
        let own = wiring(&["--hauptfenster".to_string()], false, &host);
        assert!(own.own && own.report.available(), "own data: start with the system, updates, report");
        let copy = wiring(&crate::main_window_args(Some("C:/kopie")), false, &host);
        assert!(!copy.own && !copy.report.available(), "a copy of the data");
        let named = wiring(&["--hauptfenster".to_string()], true, &host);
        assert!(!named.own && !named.report.available(), "a data folder named from outside");
    }

    #[test]
    fn the_title_is_the_product_name_and_a_copy_says_so() {
        assert_eq!(title(false), "VerseKit");
        assert!(title(true).starts_with("VerseKit") && title(true).contains("Testkopie"));
        assert!(!title(false).to_lowercase().contains("watcher"), "the old name is gone");
    }

    #[test]
    fn the_main_window_shows_the_version_of_the_program_in_its_title() {
        assert_eq!(versioned_title(false, "5.0.0-dev"), "VerseKit 5.0.0-dev");
        assert_eq!(versioned_title(true, "5.0.0-dev"), "VerseKit 5.0.0-dev (Testkopie)");
        let plain = Host::new(versekit_core::pypath::Flavor::native(), &[], "/");
        assert_eq!(shown_title(false, &plain), format!("VerseKit {}", env!("CARGO_PKG_VERSION")));
        let viewport = base_viewport(false, (880.0, 380.0));
        assert_eq!(viewport.title.as_deref(), Some(format!("VerseKit {}", env!("CARGO_PKG_VERSION")).as_str()));
    }

    #[test]
    fn the_other_windows_of_the_program_carry_no_version_in_their_title() {
        for (name, viewport) in
            [("learn", crate::learn_window::viewport()), ("first backup", crate::first_backup_window::viewport())]
        {
            let title = viewport.title.expect("a title");
            assert!(!title.is_empty(), "{name}");
            assert!(!title.contains(env!("CARGO_PKG_VERSION")), "{name} names the version: {title}");
        }
    }

    #[test]
    fn a_test_build_says_so_in_the_title() {
        let flavor = versekit_core::pypath::Flavor::native();
        let named = format!("VerseKit {}", env!("CARGO_PKG_VERSION"));
        let plain = Host::new(flavor, &[], "/");
        assert_eq!(shown_title(false, &plain), named);
        let test = Host::new(flavor, &[("SC_BP_TESTFASSUNG", "1")], "/");
        let warning = versekit_core::language::t("s_testfassung");
        assert_eq!(shown_title(false, &test), format!("{named}   {warning}"));
        assert!(shown_title(true, &test).starts_with(&versioned_title(true, env!("CARGO_PKG_VERSION"))), "the copy keeps its word");
        let off = Host::new(flavor, &[("SC_BP_TESTFASSUNG", "0")], "/");
        assert_eq!(shown_title(false, &off), named);
    }
}
