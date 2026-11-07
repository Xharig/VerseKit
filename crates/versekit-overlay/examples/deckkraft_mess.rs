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

//! Measures whether the desktop shows through a transparent eframe window: a magenta window and above it an
//! eframe window of the same place, with the overlay's peculiarities switched on one by one. Prints numbers only.
//!
//! ⚠ Opens small windows on the visible screen (bottom right of the main screen, 200×120 by default) and grabs the
//! screen there: only with the consent of the person at the machine, never while the game runs.
//!
//! `deckkraft_mess [stufen…]` with the steps `noact`, `nccalc`, `dwm`, `rand`, `ecke`, `region`, `pass`, `erase`,
//! `resize`, `undurchsichtig`, `gdi=rot|schwarz`, `alpha=<0..255>`, `x=<px>`, `y=<px>`; the renderer is OpenGL,
//! `wgpu` takes wgpu as eframe sets it up (`WGPU_BACKEND`, `WGPU_DX12_PRESENTATION_SYSTEM`), `dcomp` the set-up of
//! the overlay. Printed: the magenta beside the window, the left half (nothing painted), the right half (dark grey
//! at `alpha`), the extended style and the graphics modules loaded.
//!
//! `deckkraft_mess fremd <x> <y> <w> <h> <ms> <program> [args…]` puts magenta under the rectangle, starts the
//! program (the overlay with `--daten <copy> --lage <w>x<h>+<x>+<y>`), counts after `ms` the pixels of pure,
//! mixed and no magenta and asks the program's windows to close.

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    mess::main();
}

#[cfg(windows)]
mod mess {
    use std::time::{Duration, Instant};

    use eframe::egui;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BLACK_BRUSH, BeginPaint, EndPaint, FillRect, GetStockObject, HBRUSH, HDC, PAINTSTRUCT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, WM_ERASEBKGND, WM_PAINT};

    static mut ERASES: u32 = 0;
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
        CreateRoundRectRgn, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ, ReleaseDC,
        SRCCOPY, SelectObject, SetWindowRgn,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetLayeredWindowAttributes, GetWindowLongPtrW,
        RegisterClassW, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, STYLESTRUCT, WM_MOUSEACTIVATE,
        WM_NCCALCSIZE, WM_STYLECHANGED, WM_STYLECHANGING, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_POPUP, MA_NOACTIVATE,
    };

    const W: i32 = 200;
    const H: i32 = 120;

    #[derive(Clone, Copy, Default)]
    struct Steps {
        noact: bool,
        nccalc: bool,
        rand: bool,
        ecke: bool,
        region: bool,
        pass: bool,
        opaque: bool,
        /// GDI paints the window's own surface in this colour (0x00BBGGRR) every frame.
        gdi: Option<u32>,
        /// `WM_ERASEBKGND` fills the window's own surface black.
        erase: bool,
        /// The window shrinks and grows back before the measurement.
        resize: bool,
        alpha: u8,
        x: i32,
        y: i32,
    }

    fn wide(t: &str) -> Vec<u16> {
        t.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn magenta_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if m == WM_MOUSEACTIVATE {
            return MA_NOACTIVATE as LRESULT;
        }
        if m == WM_PAINT {
            unsafe {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(h, &mut ps);
                let brush = CreateSolidBrush(0x00FF00FF);
                let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(h, &mut r);
                FillRect(dc, &r, brush);
                DeleteObject(brush as HGDIOBJ);
                EndPaint(h, &ps);
            }
            return 0;
        }
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    /// Margin of the magenta window around the measured one.
    const M: i32 = 10;

    fn magenta(x: i32, y: i32) -> HWND {
        let (x, y) = (x - M, y - M);
        let class = wide("DeckkraftMessMagenta");
        unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(magenta_proc),
                hInstance: GetModuleHandleW(std::ptr::null()),
                hbrBackground: CreateSolidBrush(0x00FF00FF),
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let h = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                x,
                y,
                W + 2 * M,
                H + 2 * M,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                wc.hInstance,
                std::ptr::null(),
            );
            ShowWindow(h, SW_SHOWNOACTIVATE);
            h
        }
    }

    static mut STEPS: Steps =
        Steps { noact: false, nccalc: false, rand: false, ecke: false, region: false, pass: false, opaque: false, gdi: None, erase: false, resize: false, alpha: 128, x: 0, y: 0 };

    unsafe extern "system" fn sub(h: HWND, m: u32, w: WPARAM, l: LPARAM, _: usize, _: usize) -> LRESULT {
        let s = unsafe { STEPS };
        match m {
            WM_NCCALCSIZE if w != 0 && s.nccalc => return 0,
            WM_ERASEBKGND if s.erase => {
                unsafe {
                    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    GetClientRect(h, &mut r);
                    FillRect(w as HDC, &r, GetStockObject(BLACK_BRUSH) as HBRUSH);
                    ERASES += 1;
                }
                return 1;
            }
            WM_MOUSEACTIVATE if s.noact => return MA_NOACTIVATE as LRESULT,
            WM_STYLECHANGING if s.noact && w as i32 == GWL_EXSTYLE => {
                let st = unsafe { &mut *(l as *mut STYLESTRUCT) };
                st.styleNew |= WS_EX_NOACTIVATE;
            }
            WM_STYLECHANGED if w as i32 == GWL_EXSTYLE => {
                let st = unsafe { &*(l as *const STYLESTRUCT) };
                if st.styleNew & WS_EX_LAYERED != 0 {
                    unsafe { SetLayeredWindowAttributes(h, 0, 0, 0) };
                }
            }
            _ => {}
        }
        unsafe { DefSubclassProc(h, m, w, l) }
    }

    const GW: i32 = W + 2 * M;
    const GH: i32 = H + 2 * M;

    /// The screen around the measured window, the margin included.
    fn grab(x: i32, y: i32) -> Vec<u32> {
        grab_rect(x - M, y - M, GW, GH)
    }

    /// The screen in the rectangle given, as Windows composes it (BGRA as `u32`).
    fn grab_rect(x: i32, y: i32, gw: i32, gh: i32) -> Vec<u32> {
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: gw,
                    biHeight: -gh,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let old = SelectObject(dc, bmp as HGDIOBJ);
            BitBlt(dc, 0, 0, gw, gh, screen, x, y, SRCCOPY | CAPTUREBLT);
            let px = std::slice::from_raw_parts(bits as *const u32, (gw * gh) as usize).to_vec();
            SelectObject(dc, old);
            DeleteObject(bmp as HGDIOBJ);
            DeleteDC(dc);
            ReleaseDC(std::ptr::null_mut(), screen);
            px
        }
    }

    /// Which of the graphics modules of interest the process has loaded.
    fn modules() -> String {
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
        };
        let wanted = ["opengl32.dll", "nvoglv64.dll", "dxgi.dll", "d3d11.dll", "d3d12.dll", "dcomp.dll", "nvwgf2umx.dll", "vulkan-1.dll"];
        let mut found = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, 0);
            let mut e = MODULEENTRY32W { dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32, ..std::mem::zeroed() };
            let mut more = Module32FirstW(snap, &mut e) != 0;
            while more {
                let end = e.szModule.iter().position(|&c| c == 0).unwrap_or(e.szModule.len());
                let name = String::from_utf16_lossy(&e.szModule[..end]).to_lowercase();
                if wanted.contains(&name.as_str()) {
                    found.push(name);
                }
                more = Module32NextW(snap, &mut e) != 0;
            }
            windows_sys::Win32::Foundation::CloseHandle(snap);
        }
        found.sort();
        found.join(" ")
    }

    fn rgb(p: u32) -> String {
        format!("{:3} {:3} {:3}", (p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF)
    }

    struct App {
        s: Steps,
        hwnd: isize,
        magenta: HWND,
        start: Instant,
        frames: u32,
        passed: bool,
        resized: u32,
        done: bool,
    }

    impl eframe::App for App {
        fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }

        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            let ctx = ui.ctx().clone();
            if let Some(colour) = self.s.gdi {
                unsafe {
                    let h = self.hwnd as HWND;
                    let dc = GetDC(h);
                    let brush = CreateSolidBrush(colour);
                    let r = RECT { left: 0, top: 0, right: W, bottom: H };
                    FillRect(dc, &r, brush);
                    DeleteObject(brush as HGDIOBJ);
                    ReleaseDC(h, dc);
                }
            }
            self.frames += 1;
            let rect = ctx.content_rect();
            // Right half: the background of the given opacity; left half: nothing painted.
            let right = egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max);
            ui.painter().rect_filled(right, 0.0, egui::Color32::from_rgba_unmultiplied(20, 20, 20, self.s.alpha));
            let t = self.start.elapsed();
            if self.s.pass && !self.passed && t > Duration::from_millis(300) {
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
                self.passed = true;
            }
            if self.s.resize && self.resized < 2 {
                let (step, size) = if self.resized == 0 { (400, (W - 60, H - 40)) } else { (800, (W, H)) };
                if t > Duration::from_millis(step) {
                    let h = self.hwnd as HWND;
                    unsafe { SetWindowPos(h, std::ptr::null_mut(), self.s.x, self.s.y, size.0, size.1, SWP_NOZORDER | SWP_NOACTIVATE) };
                    self.resized += 1;
                }
            }
            if t > Duration::from_millis(1500) && !self.done {
                self.done = true;
                let px = grab(self.s.x, self.s.y);
                let at = |x: i32, y: i32| px[((y + M) * GW + x + M) as usize];
                let h = self.hwnd as HWND;
                let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
                let (mut key, mut a, mut fl) = (0u32, 0u8, 0u32);
                let lwa = unsafe { GetLayeredWindowAttributes(h, &mut key, &mut a, &mut fl) } != 0;
                println!(
                    "daneben {} | links {} | rechts {} | ecke(1,1) {} | exstyle {:08X} layered_attr {} alpha {} flags {} | frames {}",
                    rgb(at(3 - M, 3 - M)),
                    rgb(at(W / 4, H / 2)),
                    rgb(at(3 * W / 4, H / 2)),
                    rgb(at(1, 1)),
                    ex,
                    lwa,
                    a,
                    fl,
                    self.frames
                );
                println!("module: {} | erase {}", modules(), unsafe { ERASES });
                unsafe { DestroyWindow(self.magenta) };
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }

    /// DX12 presented through DirectComposition, the adapter that composes with premultiplied alpha preferred.
    fn dcomp_options() -> eframe::egui_wgpu::WgpuConfiguration {
        use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, surface| {
            let blends = |a: &wgpu::Adapter| {
                surface.is_some_and(|s| s.get_capabilities(a).alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied))
            };
            let rank = |a: &wgpu::Adapter| {
                let info = a.get_info();
                (
                    blends(a),
                    info.device_type != wgpu::DeviceType::Cpu,
                    info.backend == wgpu::Backend::Dx12,
                    info.device_type == wgpu::DeviceType::DiscreteGpu,
                )
            };
            adapters.iter().max_by_key(|a| rank(a)).cloned().ok_or_else(|| "no adapter".to_string())
        }));
        eframe::egui_wgpu::WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
    }

    /// Pumps the messages of this thread for `ms` milliseconds.
    fn pump(ms: u64) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    static mut CLOSE_PID: u32 = 0;
    static mut CLOSED: u32 = 0;

    unsafe extern "system" fn close_of_pid(h: HWND, _: LPARAM) -> i32 {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(h, &mut pid);
            if pid == CLOSE_PID && windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(h) != 0 {
                PostMessageW(h, WM_CLOSE, 0, 0);
                CLOSED += 1;
            }
        }
        1
    }

    /// The box of everything that is not pure magenta, and in each of its corners (top left, top right, bottom
    /// left, bottom right) the pure magenta pixels of a 6×6 field: a round corner shows magenta there, a square one
    /// none.
    fn corners(px: &[u32], w: i32, h: i32) -> String {
        let pure = |x: i32, y: i32| {
            let p = px[(y * w + x) as usize];
            ((p >> 16) & 0xFF) >= 240 && ((p >> 8) & 0xFF) <= 15 && (p & 0xFF) >= 240
        };
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1, -1);
        for y in 0..h {
            for x in 0..w {
                if !pure(x, y) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        if x1 < 0 {
            return "kein Fenster".into();
        }
        const F: i32 = 6;
        let count = |cx: i32, cy: i32| {
            (0..F).flat_map(|dy| (0..F).map(move |dx| (cx + dx, cy + dy))).filter(|&(x, y)| pure(x, y)).count()
        };
        format!(
            "umriss {}x{}+{}+{} | magenta je 6x6-ecke: ol {} or {} ul {} ur {}",
            x1 - x0 + 1,
            y1 - y0 + 1,
            x0,
            y0,
            count(x0, y0),
            count(x1 + 1 - F, y0),
            count(x0, y1 + 1 - F),
            count(x1 + 1 - F, y1 + 1 - F)
        )
    }

    /// `fremd x y w h ms program args…`: magenta below the rectangle, the program started, the screen there grabbed
    /// after `ms`, the program's visible windows asked to close.
    fn fremd(a: &[String]) {
        let n = |i: usize| a[i].parse::<i32>().expect("number");
        let (x, y, w, h, ms) = (n(0), n(1), n(2), n(3), n(4) as u64);
        let class = wide("DeckkraftMessMagenta");
        let mag = unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(magenta_proc),
                hInstance: GetModuleHandleW(std::ptr::null()),
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let m = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                x - M,
                y - M,
                w + 2 * M,
                h + 2 * M,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                wc.hInstance,
                std::ptr::null(),
            );
            ShowWindow(m, SW_SHOWNOACTIVATE);
            m
        };
        pump(300);
        let mut child = std::process::Command::new(&a[5]).args(&a[6..]).spawn().expect("start");
        pump(ms);
        let px = grab_rect(x, y, w, h);
        let (mut pure, mut mixed, mut other) = (0u32, 0u32, 0u32);
        let mut counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for &p in &px {
            let (r, g, b) = (((p >> 16) & 0xFF) as i32, ((p >> 8) & 0xFF) as i32, (p & 0xFF) as i32);
            if r >= 240 && g <= 15 && b >= 240 {
                pure += 1;
            } else if r - g >= 40 && b - g >= 40 {
                mixed += 1;
                *counts.entry(p & 0xFF_FFFF).or_default() += 1;
            } else {
                other += 1;
                *counts.entry(p & 0xFF_FFFF).or_default() += 1;
            }
        }
        let mut top: Vec<_> = counts.into_iter().collect();
        top.sort_by_key(|&(_, k)| std::cmp::Reverse(k));
        let top: Vec<String> = top.iter().take(3).map(|(c, k)| format!("({}) x{}", rgb(*c), k)).collect();
        println!(
            "fremd {}x{}: magenta_rein {} | magenta_gemischt {} | ohne_magenta {} | haeufigste ohne_rein {}",
            w,
            h,
            pure,
            mixed,
            other,
            top.join(" ")
        );
        println!("ecken: {}", corners(&px, w, h));
        unsafe {
            CLOSE_PID = child.id();
            windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows(Some(close_of_pid), 0);
        }
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end && child.try_wait().ok().flatten().is_none() {
            pump(100);
        }
        let gone = child.try_wait().ok().flatten().is_some();
        println!("geschlossen: fenster {} | beendet {}", unsafe { CLOSED }, gone);
        if !gone {
            let _ = child.kill();
        }
        let _ = child.wait();
        unsafe { DestroyWindow(mag) };
    }

    pub fn main() {
        let all: Vec<String> = std::env::args().skip(1).collect();
        if all.first().map(String::as_str) == Some("fremd") {
            fremd(&all[1..]);
            return;
        }
        let mut s = Steps { alpha: 128, x: 4900, y: 1252, ..Default::default() };
        for a in std::env::args().skip(1) {
            match a.as_str() {
                "noact" => s.noact = true,
                "nccalc" => s.nccalc = true,
                "rand" => s.rand = true,
                "ecke" => s.ecke = true,
                "dwm" => {
                    s.rand = true;
                    s.ecke = true
                }
                "region" => s.region = true,
                "pass" => s.pass = true,
                "undurchsichtig" => s.opaque = true,
                "gdi=rot" => s.gdi = Some(0x0000_00FF),
                "gdi=schwarz" => s.gdi = Some(0),
                "erase" => s.erase = true,
                "resize" => s.resize = true,
                other => {
                    if let Some(v) = other.strip_prefix("alpha=") {
                        s.alpha = v.parse().unwrap_or(128);
                    } else if let Some(v) = other.strip_prefix("x=") {
                        s.x = v.parse().unwrap_or(s.x);
                    } else if let Some(v) = other.strip_prefix("y=") {
                        s.y = v.parse().unwrap_or(s.y);
                    }
                }
            }
        }
        unsafe { STEPS = s };
        // Pixels are pixels: the window is placed in screen pixels.
        unsafe {
            windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
                windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            )
        };
        let mag = magenta(s.x, s.y);
        let viewport = egui::ViewportBuilder::default()
            .with_title("deckkraft_mess")
            .with_decorations(false)
            .with_transparent(!s.opaque)
            .with_resizable(false)
            .with_always_on_top()
            .with_active(false)
            .with_taskbar(false)
            .with_inner_size([W as f32, H as f32])
            .with_position([s.x as f32, s.y as f32]);
        let wgpu = std::env::args().any(|a| a == "wgpu" || a == "dcomp");
        let renderer = if wgpu { eframe::Renderer::Wgpu } else { eframe::Renderer::Glow };
        let mut options = eframe::NativeOptions { viewport, renderer, ..Default::default() };
        if std::env::args().any(|a| a == "dcomp") {
            options.wgpu_options = dcomp_options();
        }
        let _ = eframe::run_native(
            "deckkraft_mess",
            options,
            Box::new(move |cc| {
                if let Some(gl) = &cc.gl {
                    use eframe::glow::HasContext;
                    unsafe {
                        let alpha = gl.get_parameter_i32(eframe::glow::ALPHA_BITS);
                        let _ = gl.get_error();
                        let fb_alpha = gl.get_framebuffer_attachment_parameter_i32(
                            eframe::glow::FRAMEBUFFER,
                            eframe::glow::BACK_LEFT,
                            eframe::glow::FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE,
                        );
                        let _ = gl.get_error();
                        println!(
                            "gl: {} | {} | {} | alpha_bits {} fb_alpha {}",
                            gl.get_parameter_string(eframe::glow::VENDOR),
                            gl.get_parameter_string(eframe::glow::RENDERER),
                            gl.get_parameter_string(eframe::glow::VERSION),
                            alpha,
                            fb_alpha
                        );
                    }
                }
                if let Some(rs) = &cc.wgpu_render_state {
                    let i = rs.adapter.get_info();
                    println!("wgpu: {:?} | {} | {}", i.backend, i.name, i.driver);
                }
                let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
                    _ => 0,
                };
                let h = hwnd as HWND;
                unsafe {
                    SetWindowSubclass(h, Some(sub), 1, 0);
                    if s.noact {
                        let cur = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
                        SetWindowLongPtrW(h, GWL_EXSTYLE, (cur | WS_EX_NOACTIVATE) as isize);
                    }
                    if s.ecke {
                        let v: i32 = 1;
                        DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&v as *const i32).cast(), 4);
                    }
                    if s.rand {
                        let v: u32 = 0xFFFF_FFFE;
                        DwmSetWindowAttribute(h, DWMWA_BORDER_COLOR as u32, (&v as *const u32).cast(), 4);
                    }
                    SetWindowPos(
                        h,
                        std::ptr::null_mut(),
                        0,
                        0,
                        0,
                        0,
                        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    SetWindowPos(h, std::ptr::null_mut(), s.x, s.y, W, H, SWP_NOZORDER | SWP_NOACTIVATE);
                    if s.region {
                        let r = CreateRoundRectRgn(0, 0, W + 1, H + 1, 24, 24);
                        SetWindowRgn(h, r, 1);
                    }
                    // The magenta window right below the measured one.
                    SetWindowPos(mag, h, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
                Ok(Box::new(App {
                    s,
                    hwnd,
                    magenta: mag,
                    start: Instant::now(),
                    frames: 0,
                    passed: false,
                    resized: 0,
                    done: false,
                }))
            }),
        );
    }
}
