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

//! The choice of the [`Stage`] on Windows: each stage is made on a hidden window of its own (never shown) up to one
//! presented picture; the first that gets there hands its instance, adapter, device and queue to eframe. The error
//! handler of that device stays for the run: an error of a surface afterwards is remembered for the next start
//! ([`super::remember_failed`]), told and noted before the program ends as wgpu ends it.
//!
//! What wgpu and its DX12 layer log as errors is kept (the last [`KEPT_LINES`] lines) and goes with the error of
//! a stage: "Invalid surface" alone does not say which call of DXGI or DirectComposition refused.

use std::future::Future;
use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui_wgpu::{WgpuSetupExisting, wgpu};
use versekit_core::diagnostics::{GRAPHICS as LABEL, GRAPHICS_NONE, GRAPHICS_WINDOW, graphics_line};
use versekit_core::language::{self, Arg};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    RegisterClassW, WNDCLASSW, WS_POPUP,
};

use super::{Adapter, Attempt, Look, Present, Stage};

/// The number of log lines of wgpu kept.
const KEPT_LINES: usize = 20;
/// The size of the hidden window, in pixels.
const SIZE: u32 = 64;

/// The stage of this process and what it made, once a window was opened.
static CHOSEN: Mutex<Option<(Look, Stage, WgpuSetupExisting)>> = Mutex::new(None);

pub fn chosen() -> Option<Stage> {
    CHOSEN.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map(|(_, stage, _)| *stage)
}

/// The set-up for a window with `look`: the one this process chose already, else the first stage of the ladder that
/// works. `Err` with the text told to the player when none works; it is told by a message box unless `quiet`, which
/// also leaves [`super::STATE_FILE`] as it is.
pub fn prepare(look: Look, quiet: bool) -> Result<WgpuSetupExisting, String> {
    let mut chosen = CHOSEN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((had, _, existing)) = chosen.as_ref()
        && *had == look
    {
        return Ok(existing.clone());
    }
    keep_log();
    let paths = run_paths();
    let version = env!("CARGO_PKG_VERSION");
    let failed = if quiet { Vec::new() } else { super::failed_stages(paths, version) };
    let stages = super::order(&super::ladder(look), &failed);
    let window = ProbeWindow::new();
    let result = super::choose(&stages, |stage| match &window {
        _ if super::refused(stage) => Err("refused for the counter check".to_string()),
        Some(window) => attempt(stage, window.hwnd),
        None => Err("no hidden window for the check".to_string()),
    });
    drop(window);
    match result {
        Ok(found) => {
            let line = graphics_line(found.stage.id());
            versekit_core::trail::step(&line);
            if !found.failed.is_empty() {
                let lines = super::attempt_lines(&found.failed);
                versekit_core::diagnostics::record(LABEL, "Fehler", &line, Some(&lines.join("\n")));
            }
            if look == Look::Transparent && !quiet {
                super::remember_overlay(paths, found.stage);
            }
            found.made.errors.watch(found.stage, quiet);
            let existing = found.made.existing;
            *chosen = Some((look, found.stage, existing.clone()));
            Ok(existing)
        }
        Err(failed) => Err(give_up(&failed, quiet)),
    }
}

/// The data of the run: those of the diagnostics, else the player's own.
fn run_paths() -> &'static versekit_core::paths::Paths {
    versekit_core::diagnostics::current().map_or_else(versekit_core::paths::global, |(paths, _)| paths)
}

/// No stage works: noted in the error log and the crash file and told by a message box, both unless `quiet`. The
/// text told.
fn give_up(failed: &[Attempt], quiet: bool) -> String {
    let lines = super::attempt_lines(failed);
    for line in &lines {
        versekit_core::trail::step(&graphics_line(line));
    }
    let text = told(&language::t("grafik_fehlt_text"), &lines);
    if !quiet {
        let version = env!("CARGO_PKG_VERSION");
        versekit_core::diagnostics::note_hard_end(run_paths(), LABEL, GRAPHICS_NONE, &lines, version);
        message_box(&language::t("grafik_fehlt_titel"), &text);
    }
    text
}

/// `lead` with the attempts under it.
fn told(lead: &str, lines: &[String]) -> String {
    let details = language::tf("grafik_einzelheiten", &[("0", Arg::Text(lines.join("\n")))]);
    format!("{lead}\n\n{details}")
}

/// Shows a message box on a thread of its own: the window loop of the calling thread is not entered again.
fn message_box(title: &str, text: &str) {
    let (title, text) = (wide(&language::window_title(title)), wide(text));
    let shown = std::thread::spawn(move || {
        // SAFETY: both texts are NUL-terminated and live for the call; no owner window.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR | MB_SETFOREGROUND)
        };
    });
    let _ = shown.join();
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// What a stage that works made.
struct Made {
    existing: WgpuSetupExisting,
    errors: Arc<Errors>,
}

/// The errors of a device that no error scope caught: gathered while the stage is tried, afterwards the end of the
/// run ([`Errors::fail`]).
struct Errors {
    trying: AtomicBool,
    seen: Mutex<Vec<String>>,
    stage: Mutex<Option<(Stage, bool)>>,
}

impl Errors {
    fn new() -> Arc<Errors> {
        Arc::new(Errors { trying: AtomicBool::new(true), seen: Mutex::new(Vec::new()), stage: Mutex::new(None) })
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.seen.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// From now on an error ends the run, told unless `quiet`.
    fn watch(&self, stage: Stage, quiet: bool) {
        *self.stage.lock().unwrap_or_else(PoisonError::into_inner) = Some((stage, quiet));
        self.trying.store(false, Ordering::Relaxed);
    }

    fn handle(&self, error: wgpu::Error) {
        let text = error.to_string();
        if self.trying.load(Ordering::Relaxed) {
            self.seen.lock().unwrap_or_else(PoisonError::into_inner).push(text);
            return;
        }
        let watched = *self.stage.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((stage, quiet)) = watched {
            self.fail(stage, quiet, &text);
        }
        panic!("wgpu error: {text}");
    }

    /// An error on the real window: a surface error is remembered against the stage for the next start; noted in
    /// the crash file when no panic hook does it. Neither told nor remembered when `quiet`.
    fn fail(&self, stage: Stage, quiet: bool, text: &str) {
        let paths = run_paths();
        let version = env!("CARGO_PKG_VERSION");
        let surface = text.contains("Surface");
        if surface && !quiet {
            super::remember_failed(paths, version, stage);
        }
        let mut lines = vec![format!("{}: {}", stage.id(), text.replace('\n', " | "))];
        lines.extend(kept_lines());
        // With diagnostics the panic that follows goes into the crash file.
        if versekit_core::diagnostics::current().is_some() {
            versekit_core::diagnostics::record(LABEL, "Fehler", GRAPHICS_WINDOW, Some(&lines.join("\n")));
        } else if !quiet {
            versekit_core::diagnostics::note_hard_end(paths, LABEL, GRAPHICS_WINDOW, &lines, version);
        }
        if !quiet {
            let lead = if surface { "grafik_neu_starten_text" } else { "grafik_fehlt_text" };
            message_box(&language::t("grafik_fehlt_titel"), &told(&language::t(lead), &lines));
        }
    }
}

/// Makes `stage` on the hidden window `hwnd` up to one presented picture.
fn attempt(stage: Stage, hwnd: HWND) -> Result<Made, String> {
    let _ = take_kept_lines();
    let errors = Errors::new();
    let result = make(stage, hwnd, &errors);
    let mut found = errors.take();
    match result {
        Ok(existing) if found.is_empty() => Ok(Made { existing, errors }),
        Ok(_) => {
            found.extend(take_kept_lines());
            Err(found.join(" | "))
        }
        Err(error) => {
            found.insert(0, error);
            found.extend(take_kept_lines());
            Err(found.join(" | "))
        }
    }
}

fn make(stage: Stage, hwnd: HWND, errors: &Arc<Errors>) -> Result<WgpuSetupExisting, String> {
    let setup = stage.setup();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: setup.instance_descriptor.backends,
        flags: setup.instance_descriptor.flags,
        backend_options: setup.instance_descriptor.backend_options.clone(),
        memory_budget_thresholds: setup.instance_descriptor.memory_budget_thresholds,
        display: None,
    });
    let handle = NonZeroIsize::new(hwnd as isize).ok_or("no window")?;
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(raw_window_handle::RawDisplayHandle::Windows(
            raw_window_handle::WindowsDisplayHandle::new(),
        )),
        raw_window_handle: raw_window_handle::RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(handle)),
    };
    // SAFETY: the hidden window outlives the surface, which is dropped at the end of this function.
    let surface = unsafe { instance.create_surface_unsafe(target) }.map_err(|e| format!("create_surface: {e}"))?;
    let adapters = block_on(instance.enumerate_adapters(setup.instance_descriptor.backends));
    let adapter = pick(stage, &adapters, &surface)?;
    let capabilities = surface.get_capabilities(&adapter);
    let alpha_mode = alpha_mode(stage, &capabilities.alpha_modes)?;
    let format = eframe::egui_wgpu::preferred_framebuffer_format(&capabilities.formats).map_err(|e| e.to_string())?;
    let (device, queue) =
        block_on(adapter.request_device(&(setup.device_descriptor)(&adapter))).map_err(|e| format!("request_device: {e}"))?;
    let sink = Arc::clone(errors);
    device.on_uncaptured_error(Arc::new(move |error| sink.handle(error)));
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode,
        view_formats: vec![format],
        ..surface.get_default_config(&adapter, SIZE, SIZE).ok_or("the surface does not take this adapter")?
    };
    surface.configure(&device, &config);
    if !errors.seen.lock().unwrap_or_else(PoisonError::into_inner).is_empty() {
        return Err("Surface::configure".to_string());
    }
    let texture = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
        other => return Err(format!("get_current_texture: {other:?}")),
    };
    let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
        })],
        ..Default::default()
    }));
    queue.submit([encoder.finish()]);
    drop(view);
    queue.present(texture);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    drop(surface);
    Ok(WgpuSetupExisting { instance, adapter, device, queue })
}

/// The adapter of `stage` among `adapters`: the software renderer for [`Adapter::Software`], else the best graphics
/// card by [`super::adapter_rank`].
fn pick(stage: Stage, adapters: &[wgpu::Adapter], surface: &wgpu::Surface<'_>) -> Result<wgpu::Adapter, String> {
    let software = |a: &&wgpu::Adapter| a.get_info().device_type == wgpu::DeviceType::Cpu;
    let found = match stage.adapter {
        Adapter::Software => adapters.iter().find(software),
        Adapter::Hardware => adapters.iter().filter(|a| !software(a)).max_by_key(|a| {
            let info = a.get_info();
            let blends = surface.get_capabilities(a).alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied);
            super::adapter_rank(blends, info.device_type, info.backend)
        }),
    };
    let names = adapters.iter().map(|a| a.get_info().name).collect::<Vec<_>>().join(", ");
    found.cloned().ok_or_else(|| format!("no adapter of this kind among [{names}]"))
}

/// The alpha mode of `stage`: premultiplied through DirectComposition (else the stage does not show the game
/// behind), the one the surface offers first otherwise.
fn alpha_mode(stage: Stage, offered: &[wgpu::CompositeAlphaMode]) -> Result<wgpu::CompositeAlphaMode, String> {
    match stage.present {
        Present::Composition if offered.contains(&wgpu::CompositeAlphaMode::PreMultiplied) => {
            Ok(wgpu::CompositeAlphaMode::PreMultiplied)
        }
        Present::Composition => Err(format!("no premultiplied alpha among {offered:?}")),
        Present::Window => offered.first().copied().ok_or_else(|| "no alpha mode".to_string()),
    }
}

/// Runs `future` to its end on this thread; the futures of wgpu on native are ready at once.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut context) {
            return out;
        }
        std::thread::yield_now();
    }
}

/// A window of this thread that is never shown, for the surface of a stage.
struct ProbeWindow {
    hwnd: HWND,
}

impl ProbeWindow {
    fn new() -> Option<ProbeWindow> {
        let class = wide("VerseKitGrafikPruefung");
        // SAFETY: the class name lives for the calls; a class registered before is taken as it is; the window
        // belongs to this thread and is destroyed on drop.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_info = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            RegisterClassW(&class_info);
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                SIZE as i32,
                SIZE as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        (!hwnd.is_null()).then_some(ProbeWindow { hwnd })
    }
}

impl Drop for ProbeWindow {
    fn drop(&mut self) {
        // SAFETY: the window was made by this thread.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

/// The error lines wgpu logged lately.
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct KeptLog;

impl log::Log for KeptLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Error && metadata.target().starts_with("wgpu")
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut lines = LINES.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == KEPT_LINES {
            lines.remove(0);
        }
        lines.push(format!("{}: {}", record.target(), record.args()));
    }

    fn flush(&self) {}
}

static KEPT_LOG: KeptLog = KeptLog;

/// Keeps the error lines of wgpu from now on; nothing happens when the program has a logger already.
fn keep_log() {
    if log::set_logger(&KEPT_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Error);
    }
}

fn kept_lines() -> Vec<String> {
    LINES.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn take_kept_lines() -> Vec<String> {
    std::mem::take(&mut *LINES.lock().unwrap_or_else(PoisonError::into_inner))
}
