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

//! A graphics device that is lost while a window runs (a reset of the graphics driver takes every device of every
//! program away at once): the window starts anew instead of ending in a crash.
//!
//! ⚠ egui-wgpu panics in the first frame it draws on a lost device (`Renderer::update_buffers`: "Failed to create
//! staging buffer"), and nothing in egui-wgpu or eframe makes a new device for a window that runs. So the device
//! tells [`Loss`] when it is lost ([`watch`]); the panic that follows comes out of `eframe::run_native` (winit hands
//! it on after its loop) and [`Loss::guard`] takes it there; the program then starts itself anew ([`restart`]) and
//! this process ends without a crash. The new process waits for this one to be gone ([`take_switch`]), so the single
//! instance, the locks and the files of the run are free, and gives the driver a few seconds before it makes its
//! device. A window that draws nothing never reaches that panic, and one whose surface egui-wgpu cannot make anew
//! draws nothing without one: the loss closes the window at once ([`lost_into`], [`surface_unrecoverable`]) and its
//! end starts the program anew as well ([`after_run`]). Up to [`MOST_STARTS`] starts may follow one right after the
//! other (an installed driver resets the graphics more than once); a device that is lost again after that ends the
//! program as before, with its crash file.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use eframe::egui_wgpu::wgpu;

/// The switch of a start after a lost device, with the process id of the program that started it and how many starts
/// after a loss came one right after the other.
pub const AFTER_LOSS: &str = "--nach-grafikverlust";
/// How long a start after a lost device runs before a new loss counts as a loss of its own again.
const SETTLE: Duration = Duration::from_secs(60);
/// How many starts after a loss may follow one right after the other (a driver that is installed resets the
/// graphics more than once); a device lost again after that ends the program as before.
const MOST_STARTS: u32 = 3;
/// How long the `n`-th start after a loss waits before it makes its device: the driver gets the time to settle.
fn settle_pause(n: u32) -> Duration {
    Duration::from_secs(2 * u64::from(n.min(MOST_STARTS)))
}
/// How long a start after a lost device waits at most for the program before it to be gone.
const WAIT: Duration = Duration::from_secs(15);
/// How often it looks whether that program is gone.
const LOOK: Duration = Duration::from_millis(100);

/// What is known about the loss of the device of a process.
pub(crate) struct Loss {
    lost: AtomicBool,
    restart: AtomicBool,
    text: Mutex<String>,
}

impl Loss {
    pub(crate) const fn new() -> Loss {
        Loss { lost: AtomicBool::new(false), restart: AtomicBool::new(false), text: Mutex::new(String::new()) }
    }

    /// The device is lost (`text` says how); `restart` whether the window may start anew. Returns `restart`.
    pub(crate) fn seen(&self, text: &str, restart: bool) -> bool {
        *self.text.lock().unwrap_or_else(PoisonError::into_inner) = text.to_string();
        self.restart.store(restart, Ordering::Release);
        self.lost.store(true, Ordering::Release);
        restart
    }

    pub(crate) fn lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    /// The device is lost and the window starts anew.
    pub(crate) fn restarting(&self) -> bool {
        self.lost() && self.restart.load(Ordering::Acquire)
    }

    pub(crate) fn text(&self) -> String {
        self.text.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Runs `run`. A panic after the device was lost comes back as `Err` with the text of the loss, when the window
    /// starts anew; every other panic goes on as it came.
    pub(crate) fn guard<T>(&self, run: impl FnOnce() -> T) -> Result<T, String> {
        match catch_unwind(AssertUnwindSafe(run)) {
            Ok(value) => Ok(value),
            Err(_) if self.restarting() => Err(self.text()),
            Err(payload) => resume_unwind(payload),
        }
    }
}

/// The loss of the device of this process.
pub(crate) static LOSS: Loss = Loss::new();

/// The time of a start after a lost device and how many came one right after the other ([`take_switch`]).
static STARTED_AFTER_LOSS: OnceLock<(Instant, u32)> = OnceLock::new();

/// May a loss start the program anew, after a start after a loss `started` (how long it runs, how many starts in a
/// row)? Not in the self-check (`quiet`); after [`MOST_STARTS`] starts in a row only once the last one settled.
fn may_restart_after(quiet: bool, started: Option<(Duration, u32)>) -> bool {
    !quiet && started.is_none_or(|(running, n)| running >= SETTLE || n < MOST_STARTS)
}

fn may_restart(quiet: bool) -> bool {
    may_restart_after(quiet, STARTED_AFTER_LOSS.get().map(|(at, n)| (at.elapsed(), *n)))
}

/// The number of the start that follows a loss now: one more while the start before it has not settled.
fn next_start() -> u32 {
    match STARTED_AFTER_LOSS.get() {
        Some((at, n)) if at.elapsed() < SETTLE => n + 1,
        _ => 1,
    }
}

/// The window that is to end when its device is lost, for the losses found outside a frame.
static WINDOW: Mutex<Option<eframe::egui::Context>> = Mutex::new(None);

/// The device is lost (`text`), `allowed` whether the window may start anew: `loss` takes it, and a window that
/// starts anew is told to close at once. A window that draws nothing (the overlay waits hidden) would otherwise
/// never reach the panic of egui-wgpu, and one whose surface cannot be made anew draws nothing without one.
pub(crate) fn lost_into(loss: &Loss, ctx: Option<&eframe::egui::Context>, text: &str, allowed: bool) {
    if loss.seen(text, allowed) {
        versekit_core::diagnostics::mark_graphics_lost();
        if let Some(ctx) = ctx {
            ctx.send_viewport_cmd_to(eframe::egui::ViewportId::ROOT, eframe::egui::ViewportCommand::Close);
        }
    }
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

/// Tells `loss` when `device` is lost (`allowed` whether the window may start anew then) and ends the window `ctx`
/// for the new start ([`lost_into`]). When the window starts anew, the panic that may follow is no crash for the
/// diagnostics.
pub(crate) fn watch_into(
    loss: &'static Loss,
    device: &wgpu::Device,
    ctx: Option<eframe::egui::Context>,
    allowed: impl Fn() -> bool + Send + 'static,
) {
    device.set_device_lost_callback(move |reason, message| {
        lost_into(loss, ctx.as_ref(), &format!("{reason:?}: {message}"), allowed());
    });
}

/// [`watch_into`] for this process; not in the self-check (`quiet`).
pub(crate) fn watch(device: &wgpu::Device, ctx: Option<eframe::egui::Context>, quiet: bool) {
    *WINDOW.lock().unwrap_or_else(PoisonError::into_inner) = ctx.clone();
    QUIET_WATCH.store(quiet, Ordering::Relaxed);
    watch_into(&LOSS, device, ctx, move || may_restart(quiet));
}

static QUIET_WATCH: AtomicBool = AtomicBool::new(false);

/// egui-wgpu could not make the surface of the window anew after it was lost (a reset of the graphics driver): from
/// now on every frame ends before it is drawn, without a panic, and the window stays empty. It counts as a lost
/// device of this process.
pub(crate) fn surface_unrecoverable(text: &str) {
    let ctx = WINDOW.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let allowed = may_restart(QUIET_WATCH.load(Ordering::Relaxed));
    lost_into(&LOSS, ctx.as_ref(), &format!("Surface: {text}"), allowed);
}

/// Is a log line of egui-wgpu the failed new surface of a window ([`surface_unrecoverable`])?
pub(crate) fn is_unrecoverable_surface(target: &str, message: &str) -> bool {
    target.starts_with("egui_wgpu") && message.starts_with("Failed to recreate surface")
}

/// What `eframe::run_native` left (its result, or the text of a loss when its panic was taken): the result, or the
/// text of the loss when the window starts anew. A window closed for a loss ([`lost_into`]) ends without a panic.
pub(crate) fn after_run(outcome: Result<eframe::Result, String>, loss: &Loss) -> Result<eframe::Result, String> {
    match outcome {
        Ok(_) if loss.restarting() => Err(loss.text()),
        other => other,
    }
}

/// The command line of the start after a loss: `args` without a switch of an earlier loss, then [`AFTER_LOSS`] with
/// `pid` and the number `n` of this start in a row.
pub fn restart_args(args: &[String], pid: u32, n: u32) -> Vec<String> {
    let (mut out, _) = split_switch(args);
    out.push(AFTER_LOSS.to_string());
    out.push(pid.to_string());
    out.push(n.to_string());
    out
}

/// `args` without [`AFTER_LOSS`], its process id and its number, and the id with the number (1 without one) when
/// there is one.
pub fn split_switch(args: &[String]) -> (Vec<String>, Option<(u32, u32)>) {
    let mut out = Vec::with_capacity(args.len());
    let mut found = None;
    let mut rest = args.iter().peekable();
    while let Some(arg) = rest.next() {
        if arg == AFTER_LOSS {
            let pid = rest.next().and_then(|p| p.parse().ok());
            let n = rest.peek().and_then(|n| n.parse::<u32>().ok());
            if n.is_some() {
                rest.next();
            }
            found = pid.map(|pid| (pid, n.unwrap_or(1).max(1)));
        } else {
            out.push(arg.clone());
        }
    }
    (out, found)
}

/// The start of every process of the program: `args` without [`AFTER_LOSS`]. A start after a lost device first waits
/// (at most [`WAIT`]) for the program before it (`alive` asks for it) to be gone, then gives the driver its time
/// ([`settle_pause`], `pause` waits it), and is noted as such.
pub fn take_switch(args: Vec<String>, alive: impl Fn(u32) -> bool) -> Vec<String> {
    take_switch_with(args, alive, std::thread::sleep)
}

fn take_switch_with(args: Vec<String>, alive: impl Fn(u32) -> bool, pause: impl Fn(Duration)) -> Vec<String> {
    let (args, found) = split_switch(&args);
    if let Some((pid, n)) = found {
        let until = Instant::now() + WAIT;
        while alive(pid) && Instant::now() < until {
            std::thread::sleep(LOOK);
        }
        pause(settle_pause(n));
        let _ = STARTED_AFTER_LOSS.set((Instant::now(), n));
    }
    args
}

/// Was this process started after a lost device ([`take_switch`])?
pub fn started_after_loss() -> bool {
    STARTED_AFTER_LOSS.get().is_some()
}

/// Starts the program anew as this process was started, after a loss with `text`; this process is to end right
/// after. Noted in the error log and the trail; `Err` with the reason when no program could be started.
pub(crate) fn restart(text: &str) -> Result<(), String> {
    use versekit_core::diagnostics::{GRAPHICS, GRAPHICS_LOST, graphics_line};
    let line = graphics_line(GRAPHICS_LOST);
    versekit_core::trail::step(&line);
    versekit_core::diagnostics::note(GRAPHICS, &format!("{line} ({})", text.replace('\n', " | ")));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(program)
        .args(restart_args(&args, std::process::id(), next_start()))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::epaint::{ClippedPrimitive, Color32, Mesh, Pos2, Primitive, Rect};
    use eframe::egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut context) {
                return out;
            }
            std::thread::yield_now();
        }
    }

    /// How long a check on a real device may take before it counts as hung.
    const HUNG: Duration = Duration::from_secs(120);

    /// Runs `work` on a thread of its own and waits at most [`HUNG`]: a graphics driver that never answers fails the
    /// check with this message instead of holding the whole run; a panic of `work` comes through as it is.
    /// The checks that lose a real device run one at a time, also across processes (two runs of the checks side by
    /// side): a device destroyed while another is lost held the graphics driver.
    fn bounded(what: &'static str, work: impl FnOnce() + Send + 'static) {
        static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _here = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(std::env::temp_dir().join("versekit-grafiktests.lock"));
        let _everywhere = file.ok().filter(|file| file.lock().is_ok());
        bounded_for(what, HUNG, work);
    }

    /// [`bounded`] with its own `limit`.
    fn bounded_for(what: &'static str, limit: Duration, work: impl FnOnce() + Send + 'static) {
        let (done, finished) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name(what.into())
            .spawn(move || {
                work();
                let _ = done.send(());
            })
            .expect("a thread for the check");
        match finished.recv_timeout(limit) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("{what}: no answer of the graphics driver within {limit:?}, the check is given up")
            }
            _ => {
                if let Err(payload) = thread.join() {
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }

    /// Waits for the device after its loss, at most 30 s: a driver that does not answer then must not hold the run.
    fn settle(device: &wgpu::Device) {
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) });
    }

    #[test]
    fn a_check_on_a_hung_driver_ends_with_a_message_and_a_panic_comes_through() {
        let (sender, hold) = std::sync::mpsc::channel::<()>();
        let started = std::time::Instant::now();
        let hung = catch_unwind(AssertUnwindSafe(|| {
            bounded_for("hangs", Duration::from_millis(200), move || {
                let _ = hold.recv();
            })
        }));
        drop(sender);
        let text = hung.expect_err("gives up").downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(text.starts_with("hangs: no answer of the graphics driver"), "{text}");
        assert!(started.elapsed() < Duration::from_secs(60), "{:?}", started.elapsed());
        let passed = catch_unwind(|| bounded_for("fails", HUNG, || panic!("eigener Fehler")));
        let text = passed.expect_err("comes through").downcast_ref::<&str>().copied().unwrap_or_default();
        assert_eq!(text, "eigener Fehler");
        bounded_for("works", HUNG, || {});
    }

    /// A device without a window, on the graphics interfaces of the program; `None` where there is no adapter.
    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: super::super::backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// One frame of egui as the window draws it: a filled rectangle.
    fn frame(renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(Rect::from_min_max(Pos2::ZERO, Pos2::new(40.0, 20.0)), Color32::RED);
        let jobs = [ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(mesh) }];
        let screen = ScreenDescriptor { size_in_pixels: [64, 64], pixels_per_point: 1.0 };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let _ = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
        queue.submit([encoder.finish()]);
    }

    /// The fault: egui-wgpu draws a frame on a device that is lost, and panics (the device is lost here
    /// by `destroy`, a reset of the driver loses it by an error of DirectX; egui-wgpu finds the same invalid device).
    /// Then the device tells the loss, the panic comes back as the loss, and the window starts anew.
    #[test]
    fn a_lost_device_ends_the_window_for_a_new_start_instead_of_a_crash() {
        bounded("lost-device-frame", || {
            let Some((device, queue)) = device() else {
                eprintln!("no graphics adapter here: the test has nothing to draw on");
                return;
            };
            let loss: &'static Loss = Box::leak(Box::new(Loss::new()));
            watch_into(loss, &device, None, || true);
            let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, RendererOptions::default());
            // A frame on the working device draws.
            assert_eq!(loss.guard(|| frame(&mut renderer, &device, &queue)), Ok(()));
            assert!(!loss.lost());

            device.destroy();
            settle(&device);
            assert!(loss.lost(), "the device tells its loss");
            assert!(loss.restarting());

            let drawn = loss.guard(|| frame(&mut renderer, &device, &queue));
            let text = drawn.expect_err("the frame on the lost device ends the window");
            assert!(text.starts_with("Destroyed"), "{text}");
        });
    }

    /// A real device lost under a window that draws nothing (the overlay waiting hidden): no frame reaches the panic,
    /// the window is closed by the loss itself and its end starts the program anew.
    #[test]
    fn a_lost_device_under_a_window_that_draws_nothing_closes_it_for_a_new_start() {
        bounded("lost-device-idle", || {
            let Some((device, _queue)) = device() else {
                eprintln!("no graphics adapter here: the test has nothing to lose");
                return;
            };
            let loss: &'static Loss = Box::leak(Box::new(Loss::new()));
            let ctx = eframe::egui::Context::default();
            watch_into(loss, &device, Some(ctx.clone()), || true);
            device.destroy();
            settle(&device);
            let mut out = ctx.run_ui(Default::default(), |_| {});
            out.textures_delta.clear();
            let commands = out.viewport_output.get(&eframe::egui::ViewportId::ROOT).map(|v| v.commands.clone());
            assert!(
                commands.is_some_and(|c| c.contains(&eframe::egui::ViewportCommand::Close)),
                "the window is closed for the new start"
            );
            assert!(matches!(after_run(Ok(Ok(())), loss), Err(text) if text.starts_with("Destroyed")));
        });
    }

    /// The panic of egui-wgpu itself on a lost device, which [`Loss::guard`] stands in for: should a later egui-wgpu
    /// draw on without it, this fails and the guard can be looked at again.
    #[test]
    fn egui_wgpu_panics_on_a_lost_device() {
        bounded("lost-device-panic", || {
            let Some((device, queue)) = device() else { return };
            let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, RendererOptions::default());
            device.destroy();
            let drawn = catch_unwind(AssertUnwindSafe(|| frame(&mut renderer, &device, &queue)));
            let payload = drawn.expect_err("egui-wgpu panics");
            let text = payload.downcast_ref::<String>().cloned().unwrap_or_default();
            assert!(text.starts_with("Failed to create staging buffer"), "{text}");
        });
    }

    #[test]
    fn a_panic_without_a_lost_device_stays_a_panic() {
        let loss = Loss::new();
        let caught = catch_unwind(AssertUnwindSafe(|| loss.guard(|| panic!("kaputt"))));
        assert!(caught.is_err(), "the panic goes on to the crash file");
        // A loss on which the window does not start anew (the self-check, a start that settles) neither stops it.
        let loss = Loss::new();
        assert!(!loss.seen("Unknown: removed", false));
        let caught = catch_unwind(AssertUnwindSafe(|| loss.guard(|| panic!("kaputt"))));
        assert!(caught.is_err());
        assert_eq!(Loss::new().guard(|| 7), Ok(7));
    }

    #[test]
    fn the_start_after_a_loss_carries_the_switch_once_and_waits_for_the_program_before_it() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let started = args(&["--hauptfenster", "--daten", "X"]);
        let again = restart_args(&started, 41, 2);
        assert_eq!(again, args(&["--hauptfenster", "--daten", "X", AFTER_LOSS, "41", "2"]));
        // A second loss names only the newest program and its number.
        assert_eq!(restart_args(&again, 42, 3), args(&["--hauptfenster", "--daten", "X", AFTER_LOSS, "42", "3"]));
        assert_eq!(split_switch(&again), (started.clone(), Some((41, 2))));
        assert_eq!(split_switch(&started), (started.clone(), None));
        // The switch of an older start without its number counts as the first start; what follows stays.
        let old = args(&[AFTER_LOSS, "41", "--daten", "X"]);
        assert_eq!(split_switch(&old), (args(&["--daten", "X"]), Some((41, 1))));

        // Without the switch nothing waits and nothing is noted.
        let asked = std::cell::Cell::new(0);
        let paused = std::cell::RefCell::new(Vec::new());
        let alive = |_: u32| {
            asked.set(asked.get() + 1);
            true
        };
        assert_eq!(take_switch_with(started.clone(), alive, |d| paused.borrow_mut().push(d)), started);
        assert_eq!((asked.get(), started_after_loss(), paused.borrow().len()), (0, false, 0));
        assert!(may_restart(false) && !may_restart(true));
        // With it the start waits until the program before it is gone, then gives the driver its pause.
        let looks = std::cell::Cell::new(0);
        let alive = |pid: u32| {
            assert_eq!(pid, 41);
            looks.set(looks.get() + 1);
            looks.get() < 3
        };
        assert_eq!(take_switch_with(again, alive, |d| paused.borrow_mut().push(d)), started);
        assert_eq!(looks.get(), 3);
        assert_eq!(*paused.borrow(), [settle_pause(2)]);
        assert!(started_after_loss());
        assert!(may_restart(false), "the second start in a row may still start anew: the driver resets more than once");
        assert_eq!(next_start(), 3);
    }

    /// Starts after a loss one right after the other: up to [`MOST_STARTS`], and after a start that settled anew.
    #[test]
    fn a_loss_right_after_a_start_starts_anew_up_to_the_most_starts() {
        let soon = Duration::from_secs(5);
        assert!(may_restart_after(false, None));
        assert!(!may_restart_after(true, None), "never in the self-check");
        for n in 1..MOST_STARTS {
            assert!(may_restart_after(false, Some((soon, n))), "start {n}");
        }
        assert!(!may_restart_after(false, Some((soon, MOST_STARTS))), "lost again and again: it ends as before");
        assert!(may_restart_after(false, Some((SETTLE, MOST_STARTS))), "a start that settled counts anew");
        assert!(settle_pause(1) < settle_pause(2) && settle_pause(2) < settle_pause(3));
        assert_eq!(settle_pause(9), settle_pause(MOST_STARTS));
    }

    /// The lost device ends the window at once when it starts anew (also when it draws nothing and so never reaches
    /// the panic of egui-wgpu), and the end without a panic starts the program anew too.
    #[test]
    fn a_lost_device_closes_the_window_and_its_end_starts_anew() {
        let close_sent = |ctx: &eframe::egui::Context| {
            let mut out = ctx.run_ui(Default::default(), |_| {});
            out.textures_delta.clear();
            out.viewport_output
                .get(&eframe::egui::ViewportId::ROOT)
                .is_some_and(|v| v.commands.contains(&eframe::egui::ViewportCommand::Close))
        };
        let ctx = eframe::egui::Context::default();
        let loss = Loss::new();
        lost_into(&loss, Some(&ctx), "Unknown: Device is lost", true);
        assert!(loss.restarting() && close_sent(&ctx), "the window that starts anew is closed");
        assert!(matches!(after_run(Ok(Ok(())), &loss), Err(text) if text == "Unknown: Device is lost"));
        // A loss on which the window may not start anew leaves the window open and its end as it is.
        let ctx = eframe::egui::Context::default();
        let loss = Loss::new();
        lost_into(&loss, Some(&ctx), "Unknown: Device is lost", false);
        assert!(loss.lost() && !close_sent(&ctx));
        assert!(matches!(after_run(Ok(Ok(())), &loss), Ok(Ok(()))));
        assert!(matches!(after_run(Ok(Ok(())), &Loss::new()), Ok(Ok(()))), "a window closed by the player ends");
    }

    /// The line egui-wgpu logs when it cannot make a lost surface anew is the one taken as a lost device.
    #[test]
    fn a_surface_that_cannot_be_made_anew_is_a_lost_device() {
        let line = "Failed to recreate surface for ViewportId(0): Surface creation failed";
        assert!(is_unrecoverable_surface("egui_wgpu::winit", line));
        assert!(!is_unrecoverable_surface("wgpu_core::device", line), "only the line of egui-wgpu");
        assert!(!is_unrecoverable_surface("egui_wgpu::winit", "Failed to create staging buffer"));
    }
}
