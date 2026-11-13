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
//! instance, the locks and the files of the run are free. A loss right after such a start is not taken: a device that
//! is lost again and again ends the program as before, with its crash file.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use eframe::egui_wgpu::wgpu;

/// The switch of a start after a lost device, with the process id of the program that started it.
pub const AFTER_LOSS: &str = "--nach-grafikverlust";
/// How long a start after a lost device runs before a new loss starts the program anew again.
const SETTLE: Duration = Duration::from_secs(60);
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

/// The time of a start after a lost device ([`take_switch`]).
static STARTED_AFTER_LOSS: OnceLock<Instant> = OnceLock::new();

/// May a loss start the program anew? Not in the self-check (`quiet`), and not while a start after a loss settles.
fn may_restart(quiet: bool) -> bool {
    !quiet && STARTED_AFTER_LOSS.get().is_none_or(|at| at.elapsed() >= SETTLE)
}

/// Tells `loss` when `device` is lost (`allowed` whether the window may start anew then), and draws `ctx` once more
/// (the frame that finds the loss). When the window starts anew, the panic that follows is no crash for the
/// diagnostics.
pub(crate) fn watch_into(
    loss: &'static Loss,
    device: &wgpu::Device,
    ctx: Option<eframe::egui::Context>,
    allowed: impl Fn() -> bool + Send + 'static,
) {
    device.set_device_lost_callback(move |reason, message| {
        if loss.seen(&format!("{reason:?}: {message}"), allowed()) {
            versekit_core::diagnostics::mark_graphics_lost();
        }
        if let Some(ctx) = &ctx {
            ctx.request_repaint();
        }
    });
}

/// [`watch_into`] for this process; not in the self-check (`quiet`).
pub(crate) fn watch(device: &wgpu::Device, ctx: Option<eframe::egui::Context>, quiet: bool) {
    watch_into(&LOSS, device, ctx, move || may_restart(quiet));
}

/// The command line of the start after a loss: `args` without a switch of an earlier loss, then [`AFTER_LOSS`] with
/// `pid`.
pub fn restart_args(args: &[String], pid: u32) -> Vec<String> {
    let (mut out, _) = split_switch(args);
    out.push(AFTER_LOSS.to_string());
    out.push(pid.to_string());
    out
}

/// `args` without [`AFTER_LOSS`] and its process id, and that id when there is one.
pub fn split_switch(args: &[String]) -> (Vec<String>, Option<u32>) {
    let mut out = Vec::with_capacity(args.len());
    let mut pid = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == AFTER_LOSS {
            pid = rest.next().and_then(|p| p.parse().ok());
        } else {
            out.push(arg.clone());
        }
    }
    (out, pid)
}

/// The start of every process of the program: `args` without [`AFTER_LOSS`]. A start after a lost device first waits
/// (at most [`WAIT`]) for the program before it (`alive` asks for it) to be gone, and is noted as such.
pub fn take_switch(args: Vec<String>, alive: impl Fn(u32) -> bool) -> Vec<String> {
    let (args, pid) = split_switch(&args);
    if let Some(pid) = pid {
        let until = Instant::now() + WAIT;
        while alive(pid) && Instant::now() < until {
            std::thread::sleep(LOOK);
        }
        let _ = STARTED_AFTER_LOSS.set(Instant::now());
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
        .args(restart_args(&args, std::process::id()))
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
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        assert!(loss.lost(), "the device tells its loss");
        assert!(loss.restarting());

        let drawn = loss.guard(|| frame(&mut renderer, &device, &queue));
        let text = drawn.expect_err("the frame on the lost device ends the window");
        assert!(text.starts_with("Destroyed"), "{text}");
    }

    /// The panic of egui-wgpu itself on a lost device, which [`Loss::guard`] stands in for: should a later egui-wgpu
    /// draw on without it, this fails and the guard can be looked at again.
    #[test]
    fn egui_wgpu_panics_on_a_lost_device() {
        let Some((device, queue)) = device() else { return };
        let mut renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, RendererOptions::default());
        device.destroy();
        let drawn = catch_unwind(AssertUnwindSafe(|| frame(&mut renderer, &device, &queue)));
        let payload = drawn.expect_err("egui-wgpu panics");
        let text = payload.downcast_ref::<String>().cloned().unwrap_or_default();
        assert!(text.starts_with("Failed to create staging buffer"), "{text}");
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
        let again = restart_args(&started, 41);
        assert_eq!(again, args(&["--hauptfenster", "--daten", "X", AFTER_LOSS, "41"]));
        // A second loss names only the newest program.
        assert_eq!(restart_args(&again, 42), args(&["--hauptfenster", "--daten", "X", AFTER_LOSS, "42"]));
        assert_eq!(split_switch(&again), (started.clone(), Some(41)));
        assert_eq!(split_switch(&started), (started.clone(), None));

        // Without the switch nothing waits and nothing is noted.
        let asked = std::cell::Cell::new(0);
        assert_eq!(
            take_switch(started.clone(), |_| {
                asked.set(asked.get() + 1);
                true
            }),
            started
        );
        assert_eq!((asked.get(), started_after_loss()), (0, false));
        assert!(may_restart(false) && !may_restart(true));
        // With it the start waits until the program before it is gone, then a new loss right away is not taken.
        let looks = std::cell::Cell::new(0);
        let alive = |pid: u32| {
            assert_eq!(pid, 41);
            looks.set(looks.get() + 1);
            looks.get() < 3
        };
        assert_eq!(take_switch(again, alive), started);
        assert_eq!(looks.get(), 3);
        assert!(started_after_loss());
        assert!(!may_restart(false), "a device lost again right after the new start ends the program as before");
    }
}
