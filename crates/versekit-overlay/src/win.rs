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

//! The Windows part of the overlay: focus guard, place and size in screen
//! pixels, the pointer, the hotkey by raw input, the process list, the find
//! signal and a photo of the window for the self-check.
//!
//! ⚠ After `SetWindowPos` with a new size the system shows the last picture
//! stretched or squeezed to the new size until the program presents the next
//! one, however late the resize comes; a cloaked window (DWM cloak) keeps
//! the stretched picture on a real screen. The window therefore keeps its
//! size while it folds; a window region (`SetWindowRgn`) cuts out the
//! visible part, and outside it clicks reach the window below.
//!
//! ⚠ On every state change (click-through, always on top) winit rewrites the
//! whole extended window style without keeping foreign bits. A
//! `WS_EX_NOACTIVATE` set once would be gone. The window procedure is
//! therefore hooked and puts its own bits back in `WM_STYLECHANGING` every
//! time; a freshly layered window gets empty attributes in `WM_STYLECHANGED`,
//! otherwise it would be invisible, and no opacity of the whole window, which
//! would take the place of the transparency of each pixel.
//!
//! The window region has the rounded corners of the painted box, open, folded
//! and waiting: the corners stay round where the alpha of the window is lost.
//! DWM is asked not to round the window and to draw no border of its own.
//!
//! ⚠ The overlay is drawn by DX12 through DirectComposition ([`RENDERER`],
//! [`wgpu_options`]), never by OpenGL. Since the NVIDIA driver 621.01 OpenGL
//! is shown through a DXGI swap chain of the driver, which drops the alpha of
//! the window: the background stood black at every opacity (report of 30.10.,
//! measured with `examples/deckkraft_mess.rs`). Under the picture of
//! DirectComposition lies the window's own surface, white until painted;
//! `WM_ERASEBKGND` paints it black, which DWM takes as transparent.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use versekit_core::overlay::{Hotkey, Pointer, Rect};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWINDOWATTRIBUTE, DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLACK_BRUSH, COMPLEXREGION, ClientToScreen, CreateCompatibleDC,
    CreateDIBSection, CreateRectRgn, CreateRoundRectRgn, DIB_RGB_COLORS, DeleteDC, DeleteObject, FillRect, GetDC,
    GetStockObject, GetWindowRgn, GetWindowRgnBox, HBRUSH, HDC, HGDIOBJ, HRGN, NULLREGION, PtInRegion, ReleaseDC,
    SIMPLEREGION, SelectObject, SetWindowRgn,
};
use windows_sys::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GetClientRect, GetCursorInfo,
    GetForegroundWindow, GetLayeredWindowAttributes, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    HTCLIENT, HWND_TOPMOST, LAYERED_WINDOW_ATTRIBUTES_FLAGS, MA_NOACTIVATE, MB_ICONASTERISK, MB_ICONEXCLAMATION, MB_OK,
    MB_SETFOREGROUND, MessageBoxW, PostMessageW, RI_KEY_BREAK, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetForegroundWindow, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, WM_ACTIVATE, WM_DWMCOMPOSITIONCHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCALCSIZE, WM_NCDESTROY, WM_STYLECHANGED, WM_STYLECHANGING, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WindowFromPoint,
};

const SUBCLASS_ID: usize = 0x5645_4B4F;
const WA_INACTIVE: usize = 0;
/// `PW_RENDERFULLCONTENT`: also what the graphics card drew.
const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = 2;
/// The game's program, as the process list names it.
const GAME_EXE: &str = "starcitizen.exe";

/// Guard on or off; off only for the counter check of the self-check.
static GUARD: AtomicBool = AtomicBool::new(true);
/// The client area covers the whole window; off only for a counter check.
static NO_FRAME: AtomicBool = AtomicBool::new(true);
/// DWM is asked to round the window; off only for a counter check.
static ROUND: AtomicBool = AtomicBool::new(true);
/// How often Windows activated the overlay anyway.
static ACTIVATIONS: AtomicU32 = AtomicU32::new(0);
/// How often `WM_STYLECHANGING` put back a removed own bit.
static STYLE_REPAIRS: AtomicU32 = AtomicU32::new(0);
/// How often DWM took the request for no border, and how often it refused it.
static BORDER_TAKEN: AtomicU32 = AtomicU32::new(0);
static BORDER_REFUSED: AtomicU32 = AtomicU32::new(0);
/// The request for no border is made so that DWM must refuse it; on only for a counter check.
static BORDER_SPOILED: AtomicBool = AtomicBool::new(false);
/// The window that may take the keyboard right now (the info bar while its field has the cursor), 0 for none,
/// and the window that had the keyboard before it.
static TYPING: AtomicIsize = AtomicIsize::new(0);
static BEFORE_TYPING: AtomicIsize = AtomicIsize::new(0);

pub fn activations() -> u32 {
    ACTIVATIONS.load(Ordering::Relaxed)
}

pub fn style_repairs() -> u32 {
    STYLE_REPAIRS.load(Ordering::Relaxed)
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // The window that may take the keyboard is left to the system until it gives it back.
    let typing = TYPING.load(Ordering::Relaxed) == hwnd as isize;
    let guard = GUARD.load(Ordering::Relaxed) && !typing;
    // The rounding is asked for again whenever the style or the composition changes.
    if matches!(msg, WM_STYLECHANGED | WM_DWMCOMPOSITIONCHANGED) && ROUND.load(Ordering::Relaxed) {
        round_corners(hwnd as isize);
    }
    match msg {
        // The client area is the whole window. winit leaves a strip of frame
        // one pixel high at the top for the shadow of a window without
        // decorations; inside a window region the system draws it as a line.
        WM_NCCALCSIZE if wparam != 0 && NO_FRAME.load(Ordering::Relaxed) => return 0,
        // The window's own surface under the picture: black is transparent to DWM, white would show through.
        WM_ERASEBKGND => {
            erase_surface(hwnd, wparam as HDC);
            return 1;
        }
        WM_MOUSEACTIVATE if guard => return MA_NOACTIVATE as LRESULT,
        WM_STYLECHANGING if guard && wparam as i32 == GWL_EXSTYLE => {
            // SAFETY: for WM_STYLECHANGING lParam points to a STYLESTRUCT the
            // application may change.
            let style = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            if style.styleNew & WS_EX_NOACTIVATE == 0 {
                style.styleNew |= WS_EX_NOACTIVATE;
                STYLE_REPAIRS.fetch_add(1, Ordering::Relaxed);
            }
        }
        WM_STYLECHANGED if guard && wparam as i32 == GWL_EXSTYLE => {
            // SAFETY: as above, only read here.
            let style = unsafe { &*(lparam as *const STYLESTRUCT) };
            if style.styleNew & WS_EX_LAYERED != 0 {
                // A freshly layered window is invisible until it has attributes. Empty ones, as GLFW gives them:
                // an opacity of the whole window (`LWA_ALPHA`) would take the place of the one of each pixel.
                let (key, alpha, flags) = layered_attributes();
                unsafe { SetLayeredWindowAttributes(hwnd, key, alpha, flags) };
            }
        }
        WM_ACTIVATE if wparam & 0xFFFF != WA_INACTIVE && !typing => {
            ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        }
        WM_NCDESTROY => {
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Hooks the guard into the window and sets `WS_EX_NOACTIVATE` at once;
/// the client area then covers the whole window (`frame` keeps winit's
/// strip of frame, for a counter check). With `round` DWM is asked to round
/// the window, before it is shown first.
pub fn install(hwnd: isize, guard: bool, frame: bool, round: bool) -> bool {
    GUARD.store(guard, Ordering::Relaxed);
    NO_FRAME.store(!frame, Ordering::Relaxed);
    ROUND.store(round, Ordering::Relaxed);
    if round {
        round_corners(hwnd);
    }
    let hwnd = hwnd as HWND;
    // SAFETY: the window handle comes from the running window; the procedure lives for the program.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) } != 0;
    if guard {
        let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | WS_EX_NOACTIVATE) as isize) };
    }
    // The client area is worked out anew, now through the hooked procedure.
    let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
    // SAFETY: plain numbers for our own window.
    unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, flags) };
    ok
}

/// Hooks the guard of the overlay into a second window of the program as well, with the switches the overlay
/// set: it never takes the focus, has no frame and is rounded.
pub fn guard_also(hwnd: isize) -> bool {
    if ROUND.load(Ordering::Relaxed) {
        round_corners(hwnd);
    }
    let hwnd = hwnd as HWND;
    // SAFETY: the window handle comes from the running window; the procedure lives for the program.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) } != 0;
    if GUARD.load(Ordering::Relaxed) {
        let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | WS_EX_NOACTIVATE) as isize) };
    }
    let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
    // SAFETY: plain numbers for our own window.
    unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, flags) };
    ok
}

/// Lets `hwnd`, a window with the guard, take the keyboard while `on` and brings it to the front; off, it never
/// takes it again and the window that had the keyboard before gets it back. Only one window at a time.
pub fn allow_typing(hwnd: isize, on: bool) -> bool {
    let window = hwnd as HWND;
    // SAFETY: plain calls on our own window and on the foreground window.
    unsafe {
        let current = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if on {
            let before = GetForegroundWindow() as isize;
            if before != hwnd {
                BEFORE_TYPING.store(before, Ordering::Relaxed);
            }
            TYPING.store(hwnd, Ordering::Relaxed);
            SetWindowLongPtrW(window, GWL_EXSTYLE, (current & !WS_EX_NOACTIVATE) as isize);
            if SetForegroundWindow(window) != 0 {
                return true;
            }
            // Windows gives the front only to the program the last input went to, which a hotkey read by raw
            // input is not: joined to the input of the window in front for the call, the program may take it.
            use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            let theirs = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            let ours = GetCurrentThreadId();
            if theirs == 0 || theirs == ours || AttachThreadInput(ours, theirs, 1) == 0 {
                return false;
            }
            let done = SetForegroundWindow(window) != 0;
            AttachThreadInput(ours, theirs, 0);
            done
        } else {
            let _ = TYPING.compare_exchange(hwnd, 0, Ordering::Relaxed, Ordering::Relaxed);
            if GUARD.load(Ordering::Relaxed) {
                SetWindowLongPtrW(window, GWL_EXSTYLE, (current | WS_EX_NOACTIVATE) as isize);
            }
            let before = BEFORE_TYPING.swap(0, Ordering::Relaxed);
            if GetForegroundWindow() as isize == hwnd && before != 0 {
                SetForegroundWindow(before as HWND);
            }
            true
        }
    }
}

/// The top-level window of this process with the title `title`; 0 where there is none.
pub fn own_window_titled(title: &str) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW};
    struct Search {
        title: Vec<u16>,
        found: isize,
    }
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the search below, alive during `EnumWindows`.
        let search = unsafe { &mut *(data as *mut Search) };
        let mut process = 0u32;
        // SAFETY: `process` lives during the call.
        unsafe { GetWindowThreadProcessId(hwnd, &mut process) };
        if process != std::process::id() {
            return 1;
        }
        let mut text = [0u16; 256];
        // SAFETY: the buffer lives during the call and its length is given.
        let len = unsafe { GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32) };
        if len > 0 && text[..len as usize] == search.title[..] {
            search.found = hwnd as isize;
            return 0;
        }
        1
    }
    let mut search = Search { title: title.encode_utf16().collect(), found: 0 };
    // SAFETY: the callback only reads the windows and writes the search, which outlives the call.
    unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
    search.found
}

/// Paints the client area of the window black on `dc`.
fn erase_surface(hwnd: HWND, dc: HDC) {
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `client` lives for the call; the device context comes with the message and the stock brush needs no
    // release.
    unsafe {
        GetClientRect(hwnd, &mut client);
        FillRect(dc, &client, GetStockObject(BLACK_BRUSH) as HBRUSH);
    }
}

/// The renderer of the overlay: wgpu, which [`wgpu_options`] sends through DirectComposition.
pub const RENDERER: eframe::Renderer = versekit_ui::renderer::RENDERER;

/// DX12 alone ([`versekit_ui::renderer::setup`]), presented through DirectComposition, the only way Windows
/// composes the alpha of a DX12 picture; the adapter is chosen by [`adapter_rank`]. The run takes the first stage of
/// `versekit_ui::renderer::ladder` that works instead (`versekit_ui::renderer::run_native`), this one first.
pub fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, wgpu};
    let mut setup = versekit_ui::renderer::setup();
    setup.instance_descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
    setup.native_adapter_selector =
        Some(Arc::new(|adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
            let rank = |adapter: &wgpu::Adapter| {
                let blends = surface.is_some_and(|s| {
                    s.get_capabilities(adapter).alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied)
                });
                let info = adapter.get_info();
                adapter_rank(blends, info.device_type, info.backend)
            };
            adapters.iter().max_by_key(|a| rank(a)).cloned().ok_or_else(|| "no graphics adapter".to_string())
        }));
    WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
}

pub use versekit_ui::renderer::adapter_rank;

/// The attributes a layered window gets (color key, opacity, flags): none, so the transparency of each pixel
/// stays the one painted.
pub fn layered_attributes() -> (u32, u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS) {
    (0, 0, 0)
}

/// The flags of the attributes of a layered window; `None` when it has none.
pub fn layered_flags(hwnd: isize) -> Option<LAYERED_WINDOW_ATTRIBUTES_FLAGS> {
    let (mut key, mut alpha, mut flags) = (0u32, 0u8, 0);
    // SAFETY: the three values live for the call.
    let ok = unsafe { GetLayeredWindowAttributes(hwnd as HWND, &mut key, &mut alpha, &mut flags) } != 0;
    ok.then_some(flags)
}

/// The DWM attribute and value that keep Windows 11 from rounding the window itself. The window region
/// already gives the round outline; a DWM rounding would add a border and a drop shadow around the whole
/// window, also where it is transparent.
pub fn corner_request() -> (DWMWINDOWATTRIBUTE, DWM_WINDOW_CORNER_PREFERENCE) {
    (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND)
}

/// The DWM attribute and value that keep Windows 11 from drawing its own border around the window.
pub fn border_request() -> (DWMWINDOWATTRIBUTE, u32) {
    (DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE)
}

/// Asks DWM to leave the corners to the window region and to draw no border around the window; true
/// when it took both requests. Windows 10 refuses them, harmlessly. Every answer to the border request is
/// counted ([`border_requests`]).
pub fn round_corners(hwnd: isize) -> bool {
    let (attribute, value) = corner_request();
    let (border_attribute, border) = border_request();
    // A size DWM does not take for the attribute: the counter check of the answer.
    let border_size = if BORDER_SPOILED.load(Ordering::Relaxed) { 1 } else { std::mem::size_of::<u32>() as u32 };
    // SAFETY: `value` and `border` live for the calls and the sizes given are at most theirs.
    let (rounded, borderless) = unsafe {
        (
            DwmSetWindowAttribute(
                hwnd as HWND,
                attribute as u32,
                (&value as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
                std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            ),
            DwmSetWindowAttribute(hwnd as HWND, border_attribute as u32, (&border as *const u32).cast(), border_size),
        )
    };
    let counter = if borderless == 0 { &BORDER_TAKEN } else { &BORDER_REFUSED };
    counter.fetch_add(1, Ordering::Relaxed);
    rounded == 0 && borderless == 0
}

/// How often DWM took the request for no border so far, and how often it refused it. DWM does not tell the border
/// colour it holds (`DwmGetWindowAttribute` answers `E_INVALIDARG` for it, whatever was set), so the answers to the
/// requests are what there is to judge.
pub fn border_requests() -> (u32, u32) {
    (BORDER_TAKEN.load(Ordering::Relaxed), BORDER_REFUSED.load(Ordering::Relaxed))
}

/// Make the request for no border so that DWM refuses it (the counter check `rand_abgelehnt`).
pub fn spoil_border_request(on: bool) {
    BORDER_SPOILED.store(on, Ordering::Relaxed);
}

/// The corner preference DWM holds for the window; `None` when it cannot be asked.
pub fn corner_preference(hwnd: isize) -> Option<DWM_WINDOW_CORNER_PREFERENCE> {
    let mut value: DWM_WINDOW_CORNER_PREFERENCE = -1;
    // SAFETY: `value` lives for the call and its size is given.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd as HWND,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&mut value as *mut DWM_WINDOW_CORNER_PREFERENCE).cast(),
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        )
    };
    (result == 0).then_some(value)
}

/// Whether the client area covers the whole window, to the pixel.
pub fn client_fills_window(hwnd: isize) -> bool {
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: `client` and `origin` live for the calls.
    let ok = unsafe { GetClientRect(hwnd as HWND, &mut client) != 0 && ClientToScreen(hwnd as HWND, &mut origin) != 0 };
    let window = window_rect(hwnd);
    ok && Rect::new(origin.x, origin.y, client.right - client.left, client.bottom - client.top) == window
}

pub fn ex_style(hwnd: isize) -> u32 {
    // SAFETY: only reads the style of the window.
    unsafe { GetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE) as u32 }
}

pub fn has_noactivate(style: u32) -> bool {
    style & WS_EX_NOACTIVATE != 0
}

pub fn has_layered(style: u32) -> bool {
    style & WS_EX_LAYERED != 0
}

pub fn has_transparent(style: u32) -> bool {
    style & WS_EX_TRANSPARENT != 0
}

pub fn has_topmost(style: u32) -> bool {
    style & WS_EX_TOPMOST != 0
}

/// Asks the window the way Windows does before a click: may it be activated?
pub fn asks_no_activation(hwnd: isize) -> bool {
    let lparam = (HTCLIENT as isize) | ((WM_LBUTTONDOWN as isize) << 16);
    // SAFETY: a message to our own window with plain numbers.
    unsafe { SendMessageW(hwnd as HWND, WM_MOUSEACTIVATE, hwnd as WPARAM, lparam) == MA_NOACTIVATE as isize }
}

/// Back to the front among the always-on-top windows: a game that marks
/// itself topmost moves in front of the overlay on every activation.
pub fn reassert_topmost(hwnd: isize) {
    // SAFETY: changes only the z order of our window, without activating it.
    unsafe { SetWindowPos(hwnd as HWND, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

/// Place and size of the window in screen pixels.
pub fn window_rect(hwnd: isize) -> Rect {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `r` lives for the call.
    unsafe { GetWindowRect(hwnd as HWND, &mut r) };
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Moves and sizes the window at once, without activating it.
pub fn set_rect(hwnd: isize, r: Rect) {
    // SAFETY: plain numbers for our own window.
    unsafe { SetWindowPos(hwnd as HWND, std::ptr::null_mut(), r.x, r.y, r.w, r.h, SWP_NOZORDER | SWP_NOACTIVATE) };
}

/// A region of `r` with its corners rounded by `radius` pixels (square at 0); null when it cannot be made.
fn region_of(r: Rect, radius: i32) -> HRGN {
    let radius = radius.clamp(0, r.w.min(r.h) / 2);
    // SAFETY: plain numbers; the caller owns the region made.
    unsafe {
        if radius == 0 {
            CreateRectRgn(r.x, r.y, r.right(), r.bottom())
        } else {
            // The rounded region ends one pixel before its right and bottom edge given.
            CreateRoundRectRgn(r.x, r.y, r.right() + 1, r.bottom() + 1, 2 * radius, 2 * radius)
        }
    }
}

/// Cuts the window down to `r` (window pixels) with its corners rounded by
/// `radius` pixels: only there it is drawn and takes clicks. `None` gives it
/// back its whole rectangle.
pub fn set_region(hwnd: isize, r: Option<Rect>, radius: i32) -> bool {
    // SAFETY: a region made here; after a successful call it belongs to the
    // system, otherwise it is deleted here.
    unsafe {
        let region = match r {
            Some(r) => region_of(r, radius),
            None => std::ptr::null_mut(),
        };
        if r.is_some() && region.is_null() {
            return false;
        }
        let ok = SetWindowRgn(hwnd as HWND, region, 1) != 0;
        if !ok && !region.is_null() {
            DeleteObject(region as HGDIOBJ);
        }
        ok
    }
}

/// The part of the window Windows shows and lets take clicks, in screen
/// pixels: the box of its region, or the whole window without one.
pub fn visible_rect(hwnd: isize) -> Rect {
    let window = window_rect(hwnd);
    let mut b = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `b` lives for the call.
    match unsafe { GetWindowRgnBox(hwnd as HWND, &mut b) } {
        SIMPLEREGION | COMPLEXREGION => {
            Rect::new(window.x + b.left, window.y + b.top, b.right - b.left, b.bottom - b.top)
        }
        NULLREGION => Rect::new(window.x, window.y, 0, 0),
        _ => window,
    }
}

/// The points of the window region among `points` (window pixels); `None` without a region.
pub fn in_region(hwnd: isize, points: &[(i32, i32)]) -> Option<Vec<bool>> {
    // SAFETY: a region made here receives a copy of the window region and is deleted before returning.
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_null() {
            return None;
        }
        let found = matches!(GetWindowRgn(hwnd as HWND, region), SIMPLEREGION | COMPLEXREGION)
            .then(|| points.iter().map(|&(x, y)| PtInRegion(region, x, y) != 0).collect());
        DeleteObject(region as HGDIOBJ);
        found
    }
}

/// Whether the window has a region and it is empty: nothing of the window is shown.
#[cfg(test)]
pub fn region_is_empty(hwnd: isize) -> bool {
    use windows_sys::Win32::Graphics::Gdi::NULLREGION;
    // SAFETY: a region made here receives a copy of the window region and is deleted before returning.
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_null() {
            return false;
        }
        let empty = GetWindowRgn(hwnd as HWND, region) == NULLREGION;
        DeleteObject(region as HGDIOBJ);
        empty
    }
}

/// The name of the desktop of this thread (`Default` for the player's own), empty when it cannot be asked.
fn desktop_name() -> String {
    use windows_sys::Win32::System::StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_NAME};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    let mut buffer = [0u16; 256];
    let mut needed = 0u32;
    // SAFETY: asks for the name of the thread's own desktop into a buffer of the size given.
    let ok = unsafe {
        let desktop = GetThreadDesktop(GetCurrentThreadId());
        !desktop.is_null()
            && GetUserObjectInformationW(
                desktop,
                UOI_NAME,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 2) as u32,
                &mut needed,
            ) != 0
    };
    if !ok {
        return String::new();
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

/// The name of the single-instance mutex of the run `key` (`window_signal::key`) on the desktop `desktop`:
/// windows are found per desktop and per run, so the single instance is too. The user's own data on the
/// player's own desktop keeps the plain name, so every build of the program on it shares it; a copy of the data
/// and a desktop of a check get their own.
fn instance_name(desktop: &str, key: &str) -> String {
    let mut name = "Local\\VerseKit.Overlay".to_string();
    if !key.is_empty() {
        name.push('.');
        name.push_str(key);
    }
    if !(desktop.is_empty() || desktop.eq_ignore_ascii_case("Default")) {
        name.push('.');
        name.push_str(desktop);
    }
    name
}

/// Whether this is the first overlay of this run (sign-in, desktop, data folder): it holds a named mutex until
/// it ends. A later start finds the mutex taken, also while the first one's window does not stand yet.
pub fn first_instance() -> bool {
    // Asked once per process: a second ask would find the mutex this process made itself.
    static FIRST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FIRST.get_or_init(|| {
        let name = wide(&instance_name(&desktop_name(), &versekit_core::window_signal::key()));
        // SAFETY: the name lives for the call; the handle is kept until the process ends.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        // SAFETY: reads the error of the call just made.
        !handle.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
    })
}

/// The window a click at the screen point `(x, y)` goes to.
pub fn window_at(x: i32, y: i32) -> isize {
    // SAFETY: plain numbers.
    unsafe { WindowFromPoint(POINT { x, y }) as isize }
}

/// Tells the window that the pointer stands at `(x, y)` (window pixels). A
/// window that passed clicks through got no pointer moves meanwhile; without
/// this the first click after it catches clicks again lands where the
/// pointer was last seen.
pub fn nudge_pointer(hwnd: isize, x: i32, y: i32) {
    let lparam = ((y as u32 & 0xFFFF) << 16 | (x as u32 & 0xFFFF)) as LPARAM;
    // SAFETY: posts plain numbers to our own window.
    unsafe { PostMessageW(hwnd as HWND, WM_MOUSEMOVE, 0, lparam) };
}

/// A hidden window of this thread, for the self-check; 0 when none was made.
pub fn hidden_window() -> isize {
    let class = wide("STATIC");
    // SAFETY: a plain window of a system class without parent; the strings live for the call.
    unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        ) as isize
    }
}

pub fn destroy_window(hwnd: isize) {
    // SAFETY: destroys a window this thread made.
    unsafe { DestroyWindow(hwnd as HWND) };
}

/// The pointer: hidden by the game (not shown, or without an image), and
/// where it is.
pub fn pointer() -> Option<Pointer> {
    let mut info = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
    // SAFETY: `info` carries its size and lives for the call.
    if unsafe { GetCursorInfo(&mut info) } == 0 {
        return None;
    }
    let hidden = info.flags & CURSOR_SHOWING == 0 || info.hCursor.is_null();
    Some(Pointer { hidden, x: info.ptScreenPos.x, y: info.ptScreenPos.y })
}

/// Is the game in the process list? `None` when the list cannot be read.
pub fn game_process() -> Option<bool> {
    // SAFETY: a snapshot handle that is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut found = false;
    // SAFETY: `entry` carries its size; the snapshot is valid until closed.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(GAME_EXE) {
            found = true;
            break;
        }
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    Some(found)
}

/// The short system sound on a find; `loud` for one that must not be missed.
pub fn beep(loud: bool) {
    // SAFETY: plays a system sound, nothing else.
    unsafe { MessageBeep(if loud { MB_ICONEXCLAMATION } else { MB_ICONASTERISK }) };
}

/// Shows a message box of the system without an owner window and waits until the player closes it.
pub fn message_box(title: &str, text: &str) {
    let (title, text) = (wide(title), wide(text));
    // SAFETY: both texts are NUL-terminated and live for the call; no owner window.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONEXCLAMATION | MB_SETFOREGROUND)
    };
}

/// The window as Windows draws it: width, height and the pixels (BGRA as
/// `u32`), or `None`.
pub fn photo(hwnd: isize) -> Option<(i32, i32, Vec<u32>)> {
    let r = window_rect(hwnd);
    if r.w <= 0 || r.h <= 0 {
        return None;
    }
    // SAFETY: every GDI object made here is released before returning; the
    // pixel memory belongs to the DIB section and is copied while it lives.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: r.w,
                biHeight: -r.h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let mut result = None;
        if !bitmap.is_null() && !bits.is_null() {
            let old = SelectObject(dc, bitmap as HGDIOBJ);
            if PrintWindow(hwnd as HWND, dc, PW_RENDERFULLCONTENT) != 0 {
                let pixels = std::slice::from_raw_parts(bits as *const u32, (r.w * r.h) as usize).to_vec();
                result = Some((r.w, r.h, pixels));
            }
            SelectObject(dc, old);
            DeleteObject(bitmap as HGDIOBJ);
        }
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
        result
    }
}

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const RI_KEY_E0: u16 = 2;

/// Which keys are down, followed from the raw input messages.
#[derive(Default)]
pub struct Keys {
    ctrl: [bool; 2],
    alt: [bool; 2],
    shift: [bool; 2],
    held: Vec<u16>,
}

/// The scan code of the right Shift key; both Shift keys come without the E0 flag.
const RIGHT_SHIFT: u16 = 0x36;

impl Keys {
    /// Takes a key message; true when it completes `hotkey` with exactly its
    /// modifiers, like `RegisterHotKey`. A held key does not repeat.
    ///
    /// `down_now` asks the system whether a modifier key is down. A release
    /// that never arrived (Ctrl+Alt+Del, a locked screen) would otherwise
    /// leave a modifier pressed for good; on a hit the system's state wins.
    ///
    /// ⚠ Ctrl+Alt is AltGr on a German keyboard: a hotkey on Q, E or M fires
    /// when typing @, € or µ.
    pub fn feed(
        &mut self,
        vkey: u16,
        make: u16,
        flags: u16,
        hotkey: Option<Hotkey>,
        down_now: &dyn Fn(u16) -> bool,
    ) -> bool {
        let down = u32::from(flags) & RI_KEY_BREAK == 0;
        let side = usize::from(flags & RI_KEY_E0 != 0);
        match vkey {
            VK_CONTROL => self.ctrl[side] = down,
            VK_MENU => self.alt[side] = down,
            VK_SHIFT => self.shift[usize::from(make == RIGHT_SHIFT)] = down,
            _ => {
                let was_held = self.held.contains(&vkey);
                if !down {
                    self.held.retain(|&k| k != vkey);
                    return false;
                }
                if was_held {
                    return false;
                }
                self.held.push(vkey);
                let Some(h) = hotkey else { return false };
                if vkey != h.key {
                    return false;
                }
                for (keys, vk) in [(&mut self.ctrl, VK_CONTROL), (&mut self.alt, VK_MENU), (&mut self.shift, VK_SHIFT)]
                {
                    if keys.iter().any(|&d| d) != down_now(vk) {
                        *keys = [down_now(vk), false];
                    }
                }
                let any = |keys: [bool; 2]| keys.iter().any(|&d| d);
                return any(self.ctrl) == h.ctrl && any(self.alt) == h.alt && any(self.shift) == h.shift;
            }
        }
        false
    }
}

/// Is the key down right now, by the system's own state?
fn down_now(vkey: u16) -> bool {
    // SAFETY: reads the key state, nothing else.
    unsafe { GetAsyncKeyState(i32::from(vkey)) < 0 }
}

/// Makes the process aware of the scaling of each monitor, before any
/// window, place or scale is asked. winit does this only when its event
/// loop starts; places worked out before would be in virtual points.
pub fn dpi_aware() {
    // SAFETY: a process-wide switch without pointers; a second call fails harmlessly.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads the hotkey system-wide by raw input and sends a signal for each
/// press. The combination is read from `hotkey` at every key, so a change
/// in the settings takes effect at once.
///
/// ⚠ `RegisterHotKey` does not work while Star Citizen is in front: a game
/// that registers its keyboard with `RIDEV_NOHOTKEYS` switches application
/// hotkeys off. Raw input with `RIDEV_INPUTSINK` delivers the keys to a
/// program in the background too. Nothing is intercepted; the game gets the
/// same keys.
///
/// ⚠ The keys come from the one Raw Input listener of the process
/// (`versekit_core::stick_input::on_keys`), which anti-AFK shares: a window
/// of its own that registered the keyboard would take it from the listener,
/// or the listener from it.
///
/// Returns `false` if Windows refused the registration.
pub fn start_hotkeys(hotkey: Arc<Mutex<Option<Hotkey>>>, sender: Sender<()>, wake: impl Fn() + Send + 'static) -> bool {
    let state = Mutex::new((Keys::default(), sender, wake));
    versekit_core::stick_input::on_keys(move |key| {
        let combo = *hotkey.lock().unwrap_or_else(|e| e.into_inner());
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        let (keys, sender, wake) = &mut *state;
        // A sender whose receiver is gone (the overlay closes) asks nothing more of the window.
        if keys.feed(key.vkey, key.scan, key.flags, combo, &down_now) && sender.send(()).is_ok() {
            wake();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::LWA_ALPHA;

    const UP: u16 = RI_KEY_BREAK as u16;

    fn combo(text: &str) -> Option<Hotkey> {
        Hotkey::parse(text)
    }

    /// A keyboard: the keys physically down, and the messages that reach the reader.
    struct Board {
        keys: Keys,
        down: std::cell::RefCell<Vec<(u16, u16)>>,
        hotkey: Option<Hotkey>,
    }

    impl Board {
        fn new(text: &str) -> Board {
            Board { keys: Keys::default(), down: Default::default(), hotkey: combo(text) }
        }

        /// A key message; `arrives` false for a release the reader never sees.
        fn key(&mut self, vkey: u16, make: u16, flags: u16, arrives: bool) -> bool {
            if flags & UP == 0 {
                self.down.borrow_mut().push((vkey, make));
            } else {
                self.down.borrow_mut().retain(|&k| k != (vkey, make));
            }
            if !arrives {
                return false;
            }
            let down = self.down.borrow().clone();
            self.keys.feed(vkey, make, flags, self.hotkey, &|vk| down.iter().any(|(k, _)| *k == vk))
        }

        fn press(&mut self, vkey: u16) -> bool {
            self.key(vkey, 0, 0, true)
        }

        fn release(&mut self, vkey: u16) -> bool {
            self.key(vkey, 0, UP, true)
        }
    }

    const B: u16 = b'B' as u16;

    #[test]
    fn the_hotkey_fires_once_with_exactly_its_modifiers() {
        let mut k = Board::new("Strg+Alt+B");
        k.press(VK_CONTROL);
        k.press(VK_MENU);
        assert!(k.press(B));
        assert!(!k.press(B), "a held key does not repeat");
        k.release(B);
        k.press(VK_SHIFT);
        assert!(!k.press(B), "an extra Shift is another combination");
        k.release(B);
        k.release(VK_SHIFT);
        k.release(VK_MENU);
        assert!(!k.press(B), "Alt missing");
    }

    #[test]
    fn the_right_alt_counts_and_no_hotkey_fires_nothing() {
        let mut k = Board::new("Ctrl+Alt+F8");
        k.press(VK_CONTROL);
        k.key(VK_MENU, 0, RI_KEY_E0, true);
        assert!(k.press(0x77));
        k.release(0x77);
        k.hotkey = None;
        assert!(!k.press(0x77));
    }

    /// F13 and F24 arrive by raw input with their key codes 0x7C and 0x87, like the other keys.
    #[test]
    fn the_keys_f13_to_f24_fire_like_the_others() {
        let mut k = Board::new("Alt+F13");
        k.press(VK_MENU);
        assert!(!k.press(0x7b), "F12 is another key");
        assert!(k.press(0x7c));
        k.release(0x7c);
        k.hotkey = combo("Alt+F24");
        assert!(!k.press(0x7c), "F13 no more");
        assert!(k.press(0x87));
    }

    /// F13 by itself (Kuehlwagen's handle search): it fires without a modifier, and with one held it is another
    /// combination, as `Alt+F13` does not fire on F13 alone.
    #[test]
    fn a_key_by_itself_fires_only_without_a_modifier() {
        let mut k = Board::new("F13");
        assert!(k.hotkey.is_some(), "F13 by itself is a hotkey");
        assert!(k.press(0x7c));
        assert!(!k.press(0x7c), "a held key does not repeat");
        k.release(0x7c);
        assert!(!k.press(0x7b), "F12 is another key");
        k.release(0x7b);
        for modifier in [VK_CONTROL, VK_MENU, VK_SHIFT] {
            k.press(modifier);
            assert!(!k.press(0x7c), "{modifier:#x} held: another combination");
            k.release(0x7c);
            k.release(modifier);
        }
        assert!(k.press(0x7c), "by itself again");
        k.release(0x7c);
        // The keys of the number block by their own codes.
        k.hotkey = combo("NumPlus");
        assert!(!k.press(0x6d), "the minus is another key");
        assert!(k.press(0x6b));
        // A letter by itself is no hotkey: B typed in the chat fires nothing.
        let mut k = Board::new("B");
        assert_eq!(k.hotkey, None);
        assert!(!k.press(B));
    }

    /// After Ctrl+Alt+Del the releases of Ctrl and Alt never arrive: a plain
    /// B typed in the chat must not fire.
    #[test]
    fn a_lost_release_does_not_leave_the_modifiers_down() {
        let mut k = Board::new("Strg+Alt+B");
        k.press(VK_CONTROL);
        k.press(VK_MENU);
        k.key(VK_CONTROL, 0, UP, false);
        k.key(VK_MENU, 0, UP, false);
        assert!(!k.press(B));
        k.release(B);
        // Afterwards the real combination works again.
        k.press(VK_CONTROL);
        k.press(VK_MENU);
        assert!(k.press(B));
    }

    #[test]
    fn both_shift_keys_count_on_their_own() {
        let mut k = Board::new("Strg+Umschalt+B");
        k.press(VK_CONTROL);
        k.key(VK_SHIFT, 0x2A, 0, true);
        k.key(VK_SHIFT, RIGHT_SHIFT, 0, true);
        k.key(VK_SHIFT, RIGHT_SHIFT, UP, true);
        // The left one is still down: the system says Shift is down too.
        assert!(k.press(B));
    }

    #[test]
    fn the_single_instance_is_per_desktop_and_the_players_own_desktop_keeps_the_plain_name() {
        assert_eq!(instance_name("Default", ""), "Local\\VerseKit.Overlay");
        assert_eq!(instance_name("default", ""), "Local\\VerseKit.Overlay");
        assert_eq!(instance_name("", ""), "Local\\VerseKit.Overlay");
        assert_eq!(instance_name("VerseKit-Pruefung-1", ""), "Local\\VerseKit.Overlay.VerseKit-Pruefung-1");
        assert_ne!(instance_name("A", ""), instance_name("B", ""));
        // This thread runs on a desktop that has a name.
        assert!(!desktop_name().is_empty());
    }

    /// A window that passes clicks is layered: it gets attributes (without them it is invisible), but no opacity of
    /// the whole window, which would leave the background of the overlay black. The window is never shown.
    #[test]
    fn a_layered_window_gets_attributes_without_an_opacity_of_its_own() {
        let hwnd = hidden_window();
        assert_ne!(hwnd, 0);
        install(hwnd, true, false, true);
        let style = ex_style(hwnd);
        // SAFETY: changes the style of a window this thread made.
        unsafe { SetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE, (style | WS_EX_LAYERED | WS_EX_TRANSPARENT) as isize) };
        assert!(has_layered(ex_style(hwnd)));
        let flags = layered_flags(hwnd);
        destroy_window(hwnd);
        assert_eq!(flags, Some(0), "attributes set, without LWA_ALPHA ({LWA_ALPHA}) or a color key");
    }

    /// OpenGL must never draw the overlay: the NVIDIA driver shows it through a swap chain of its own that drops the
    /// alpha of the window, and the background stands black at every opacity (30.10.). DX12 goes through
    /// DirectComposition, which composes the alpha.
    #[test]
    fn the_overlay_is_drawn_through_direct_composition_never_by_opengl() {
        use eframe::egui_wgpu::{WgpuSetup, wgpu::Dx12SwapchainKind};
        assert_eq!(RENDERER, eframe::Renderer::Wgpu);
        let options = wgpu_options();
        let WgpuSetup::CreateNew(setup) = &options.wgpu_setup else { panic!("a setup of its own") };
        assert_eq!(
            setup.instance_descriptor.backend_options.dx12.presentation_system,
            Dx12SwapchainKind::DxgiFromVisual
        );
        assert!(setup.native_adapter_selector.is_some(), "the adapter is chosen by its alpha");
        // Without the OpenGL and the Vulkan driver in the program (`nvoglv64.dll` with NVIDIA).
        assert_eq!(setup.instance_descriptor.backends, eframe::egui_wgpu::wgpu::Backends::DX12);
        // The window opens through the choice of the stage, transparent first, never past it.
        let app = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("app.rs"))
            .expect("the source of the overlay");
        assert!(app.contains("renderer::run_native(") && app.contains("renderer::Look::Transparent"));
        assert!(!app.contains(concat!("eframe::", "run_native(")));
    }

    /// The software renderer of Windows came first among equal adapters in the measurement; a surface that takes
    /// premultiplied alpha comes before everything else.
    #[test]
    fn a_real_adapter_that_blends_comes_first() {
        use eframe::egui_wgpu::wgpu::{Backend, DeviceType};
        let nvidia_dx12 = adapter_rank(true, DeviceType::DiscreteGpu, Backend::Dx12);
        let warp = adapter_rank(true, DeviceType::Cpu, Backend::Dx12);
        let nvidia_vulkan = adapter_rank(true, DeviceType::DiscreteGpu, Backend::Vulkan);
        let integrated_dx12 = adapter_rank(true, DeviceType::IntegratedGpu, Backend::Dx12);
        let opaque = adapter_rank(false, DeviceType::DiscreteGpu, Backend::Dx12);
        assert!(nvidia_dx12 > warp);
        assert!(nvidia_vulkan > warp, "a real card before the software renderer");
        assert!(nvidia_dx12 > nvidia_vulkan);
        assert!(nvidia_dx12 > integrated_dx12);
        assert!(warp > opaque, "only a surface that blends shows the game behind");
    }

    /// The window's own surface under the picture of DirectComposition is white until painted, and DWM showed it
    /// instead of the game behind; `WM_ERASEBKGND` paints it black, which DWM takes as transparent. The window is
    /// never shown.
    #[test]
    fn the_window_surface_is_painted_black() {
        let hwnd = hidden_window();
        assert_ne!(hwnd, 0);
        install(hwnd, true, false, true);
        // SAFETY: a memory DC with a DIB section of 4×4 pixels, released before the end; the pixels are read while
        // the section lives.
        let (answer, pixel) = unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 4,
                    biHeight: -4,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let old = SelectObject(dc, bitmap as HGDIOBJ);
            std::slice::from_raw_parts_mut(bits as *mut u32, 16).fill(0xFFFF_FFFF);
            let answer = SendMessageW(hwnd as HWND, WM_ERASEBKGND, dc as WPARAM, 0);
            let pixel = *(bits as *const u32);
            SelectObject(dc, old);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(dc);
            (answer, pixel)
        };
        destroy_window(hwnd);
        assert_eq!(answer, 1, "erased by the overlay");
        assert_eq!(pixel & 0x00FF_FFFF, 0, "black, not white: {pixel:08X}");
    }

    #[test]
    fn dwm_is_asked_not_to_round_the_window_itself() {
        // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_DONOTROUND = 1.
        assert_eq!(corner_request(), (33, 1));
    }

    #[test]
    fn dwm_is_asked_for_no_border() {
        // DWMWA_BORDER_COLOR = 34, DWMWA_COLOR_NONE = 0xFFFFFFFE.
        assert_eq!(border_request(), (34, 0xFFFF_FFFE));
    }

    /// The box of the region and which corner pixels of `r` (one pixel in) it holds, for `radius`.
    fn region_facts(r: Rect, radius: i32) -> (Rect, [bool; 4], bool) {
        use windows_sys::Win32::Graphics::Gdi::GetRgnBox;
        let region = region_of(r, radius);
        assert!(!region.is_null());
        let mut b = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: the region made above, deleted at the end.
        unsafe {
            GetRgnBox(region, &mut b);
            let at = |x, y| PtInRegion(region, x, y) != 0;
            let corners = [
                at(r.x + 1, r.y + 1),
                at(r.right() - 2, r.y + 1),
                at(r.x + 1, r.bottom() - 2),
                at(r.right() - 2, r.bottom() - 2),
            ];
            let edges = at(r.x + radius, r.y)
                && at(r.right() - 1 - radius, r.bottom() - 1)
                && at(r.x, r.y + radius)
                && at(r.right() - 1, r.bottom() - 1 - radius);
            DeleteObject(region as HGDIOBJ);
            (Rect::new(b.left, b.top, b.right - b.left, b.bottom - b.top), corners, edges)
        }
    }

    /// The rounded region keeps the box of its rectangle to the pixel, leaves out the corner pixels and
    /// holds the straight edges up to the rounding; at radius 0 it is the plain rectangle.
    #[test]
    fn the_window_region_is_rounded_like_the_box() {
        let r = Rect::new(10, 20, 300, 120);
        let (bounds, corners, edges) = region_facts(r, 12);
        assert_eq!(bounds, r);
        assert_eq!(corners, [false; 4], "corner pixels left out");
        assert!(edges, "the edges reach the rounding");
        let (bounds, corners, _) = region_facts(r, 0);
        assert_eq!(bounds, r);
        assert_eq!(corners, [true; 4]);
        // A radius larger than half the height makes a pill, not a broken region.
        let strip = Rect::new(0, 0, 180, 22);
        assert_eq!(region_facts(strip, 40).0, strip);
    }

    #[test]
    fn the_single_instance_is_per_data_folder_and_one_folder_is_one_instance() {
        use versekit_core::window_signal::folder_key;
        let (a, b) = (folder_key("C:/Kopie-A"), folder_key("C:/Kopie-B"));
        assert_eq!(instance_name("Default", &a), format!("Local\\VerseKit.Overlay.{a}"));
        assert_ne!(instance_name("Default", &a), instance_name("Default", &b), "two copies, two programs");
        assert_ne!(instance_name("Default", &a), instance_name("Default", ""), "a copy is not the user's own data");
        assert_eq!(instance_name("D", &a), format!("Local\\VerseKit.Overlay.{a}.D"));
        // The names as the system takes them: a second mutex of the same name exists already, one of another
        // key does not. (Names of this test alone: the mutex of a running program is never touched.)
        let take = |name: &str| {
            let wide = wide(name);
            // SAFETY: the name lives for the call; the handle is closed by the caller.
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
            // SAFETY: reads the error of the call just made.
            let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            (handle, existed)
        };
        let run = std::process::id();
        let first = take(&instance_name("Default", &format!("pruefung-{run}-a")));
        let same = take(&instance_name("Default", &format!("pruefung-{run}-a")));
        let other = take(&instance_name("Default", &format!("pruefung-{run}-b")));
        assert!(!first.0.is_null() && !first.1, "the first one makes it");
        assert!(same.1, "a second start on the same data finds it taken");
        assert!(!other.1, "a start on another copy does not");
        for (handle, _) in [first, same, other] {
            // SAFETY: handles made above.
            unsafe { CloseHandle(handle) };
        }
    }

    #[test]
    fn the_process_list_can_be_read() {
        assert!(game_process().is_some());
    }
}
