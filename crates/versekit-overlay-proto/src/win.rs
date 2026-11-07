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

//! Windows part of the overlay: focus guard, overall opacity and global hotkeys.
//!
//! ⚠ On every state change (click-through, always on top) winit rewrites the
//! entire extended window style (`SetWindowLongW(GWL_EXSTYLE, …)`) without
//! preserving foreign bits. A `WS_EX_NOACTIVATE` set once would be gone
//! afterwards. This module therefore hooks into the window procedure and
//! re-applies its own bits in `WM_STYLECHANGING` every time.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEKEYBOARD,
    RegisterRawInputDevices,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GWL_EXSTYLE, GetCursorPos, GetLayeredWindowAttributes, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, HTCLIENT, HWND_MESSAGE, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MSG,
    RI_KEY_BREAK, RegisterClassW, STYLESTRUCT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WM_ACTIVATE, WM_INPUT, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_NCDESTROY, WM_STYLECHANGED, WM_STYLECHANGING, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

const SUBCLASS_ID: usize = 0x5645_4B54;
const WA_INACTIVE: usize = 0;

/// Guard on or off. Off only for the negative control (`--no-guard`).
static GUARD: AtomicBool = AtomicBool::new(true);
/// Desired overall opacity, 255 = fully opaque.
static ALPHA: AtomicU8 = AtomicU8::new(255);
/// How often Windows activated the overlay anyway.
static ACTIVATIONS: AtomicU32 = AtomicU32::new(0);
/// How often `WM_STYLECHANGING` restored one of our own bits that had been removed.
static STYLE_REPAIRS: AtomicU32 = AtomicU32::new(0);

pub fn activations() -> u32 {
    ACTIVATIONS.load(Ordering::Relaxed)
}

pub fn style_repairs() -> u32 {
    STYLE_REPAIRS.load(Ordering::Relaxed)
}

fn wanted_bits() -> u32 {
    let mut bits = WS_EX_NOACTIVATE;
    if ALPHA.load(Ordering::Relaxed) < 255 {
        bits |= WS_EX_LAYERED;
    }
    bits
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let guard = GUARD.load(Ordering::Relaxed);
    match msg {
        WM_MOUSEACTIVATE if guard => return MA_NOACTIVATE as LRESULT,
        WM_STYLECHANGING if guard && wparam as i32 == GWL_EXSTYLE => {
            // SAFETY: For WM_STYLECHANGING, Windows documents lParam as pointing to a
            // STYLESTRUCT that the application may modify.
            let style = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            let wanted = wanted_bits();
            if style.styleNew & wanted != wanted {
                style.styleNew |= wanted;
                STYLE_REPAIRS.fetch_add(1, Ordering::Relaxed);
            }
        }
        WM_STYLECHANGED if guard && wparam as i32 == GWL_EXSTYLE => {
            // SAFETY: as above, read-only here.
            let style = unsafe { &*(lparam as *const STYLESTRUCT) };
            if style.styleNew & WS_EX_LAYERED != 0 {
                // A freshly layered window is invisible until its opacity is set.
                unsafe { SetLayeredWindowAttributes(hwnd, 0, ALPHA.load(Ordering::Relaxed), LWA_ALPHA) };
            }
        }
        WM_ACTIVATE if wparam & 0xFFFF != WA_INACTIVE => {
            ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        }
        WM_NCDESTROY => {
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Attaches the guard to the window and sets `WS_EX_NOACTIVATE` immediately.
pub fn install(hwnd: isize, guard: bool) -> bool {
    GUARD.store(guard, Ordering::Relaxed);
    let hwnd = hwnd as HWND;
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) } != 0;
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | WS_EX_NOACTIVATE) as isize) };
    ok
}

/// Sets the overall opacity (255 = fully opaque) via a layered window.
pub fn set_opacity(hwnd: isize, alpha: u8) {
    ALPHA.store(alpha, Ordering::Relaxed);
    let hwnd = hwnd as HWND;
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if alpha < 255 {
        unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | WS_EX_LAYERED) as isize) };
    }
    if current & WS_EX_LAYERED != 0 || alpha < 255 {
        unsafe { SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA) };
    }
}

pub fn ex_style(hwnd: isize) -> u32 {
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

/// Brings the overlay back to the front within the band of "always on top" windows.
///
/// A game that marks itself as topmost moves in front of the overlay on every
/// activation.
pub fn reassert_topmost(hwnd: isize) {
    unsafe { SetWindowPos(hwnd as HWND, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

/// The opacity Windows actually holds for the window, or None without layering.
pub fn layered_alpha(hwnd: isize) -> Option<u8> {
    let (mut key, mut alpha, mut flags) = (0u32, 0u8, 0u32);
    let ok = unsafe { GetLayeredWindowAttributes(hwnd as HWND, &mut key, &mut alpha, &mut flags) };
    (ok != 0 && flags & LWA_ALPHA != 0).then_some(alpha)
}

/// Asks the window the way Windows does before a click: may it be activated?
pub fn ask_mouse_activate(hwnd: isize) -> isize {
    let lparam = (HTCLIENT as isize) | ((WM_LBUTTONDOWN as isize) << 16);
    unsafe { SendMessageW(hwnd as HWND, WM_MOUSEACTIVATE, hwnd as WPARAM, lparam) }
}

pub fn is_ma_noactivate(answer: isize) -> bool {
    answer == MA_NOACTIVATE as isize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    Passthrough,
    Opacity,
    OnTop,
    Quit,
}

/// ⚠ Ctrl+Alt is the same as AltGr on a German keyboard. Letters that carry an
/// AltGr character (Q = @, E = €, M = µ) would fire when typing those characters.
const HOTKEYS: [(u16, Hotkey); 4] = [
    (b'D' as u16, Hotkey::Passthrough),
    (b'O' as u16, Hotkey::Opacity),
    (b'T' as u16, Hotkey::OnTop),
    (b'X' as u16, Hotkey::Quit),
];

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const RI_KEY_E0: u16 = 2;
const USAGE_PAGE_GENERIC: u16 = 0x01;
const USAGE_KEYBOARD: u16 = 0x06;

/// Which keys are currently pressed, tracked from the raw input messages.
#[derive(Default)]
struct Keys {
    ctrl: [bool; 2],
    alt: [bool; 2],
    shift: [bool; 2],
    letters: Vec<u16>,
}

impl Keys {
    /// Processes a key message and returns the hotkey it triggered.
    fn feed(&mut self, vkey: u16, flags: u16) -> Option<Hotkey> {
        let down = u32::from(flags) & RI_KEY_BREAK == 0;
        let side = usize::from(flags & RI_KEY_E0 != 0);
        match vkey {
            VK_CONTROL => self.ctrl[side] = down,
            VK_MENU => self.alt[side] = down,
            VK_SHIFT => self.shift[side] = down,
            _ => {
                let held = self.letters.contains(&vkey);
                if !down {
                    self.letters.retain(|&k| k != vkey);
                    return None;
                }
                if held {
                    return None;
                }
                self.letters.push(vkey);
                let combo = self.ctrl.iter().any(|&d| d) && self.alt.iter().any(|&d| d);
                if combo && !self.shift.iter().any(|&d| d) {
                    return HOTKEYS.iter().find(|(vk, _)| *vk == vkey).map(|(_, key)| *key);
                }
            }
        }
        None
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads Ctrl+Alt+D/O/T/X system-wide via raw input and forwards them.
///
/// ⚠ `RegisterHotKey` does not work while Star Citizen is in the foreground: a
/// game that registers its keyboard with `RIDEV_NOHOTKEYS` disables application
/// hotkeys. Raw input with `RIDEV_INPUTSINK`, in contrast, delivers the keys to
/// a background program as well. This is not a keyboard hook: keys are only
/// read, nothing is intercepted, and the game receives the same keys too.
///
/// Returns `false` if Windows rejected the registration.
pub fn start_hotkeys(sender: Sender<Hotkey>, wake: impl Fn() + Send + 'static) -> bool {
    let (report_tx, report_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let class = wide("VerseKitTastenEmpfang");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe { RegisterClassW(&wc) };
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        let device = RAWINPUTDEVICE {
            usUsagePage: USAGE_PAGE_GENERIC,
            usUsage: USAGE_KEYBOARD,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        let ok = !hwnd.is_null()
            && unsafe { RegisterRawInputDevices(&device, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) } != 0;
        let _ = report_tx.send(ok);
        if !ok {
            return;
        }

        let mut keys = Keys::default();
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
            if msg.message == WM_INPUT {
                let mut raw = RAWINPUT::default();
                let mut size = std::mem::size_of::<RAWINPUT>() as u32;
                let read = unsafe {
                    GetRawInputData(
                        msg.lParam as _,
                        RID_INPUT,
                        (&mut raw as *mut RAWINPUT).cast(),
                        &mut size,
                        std::mem::size_of::<RAWINPUTHEADER>() as u32,
                    )
                };
                if read != u32::MAX && raw.header.dwType == RIM_TYPEKEYBOARD {
                    // SAFETY: When dwType == RIM_TYPEKEYBOARD, the keyboard member of the union is populated.
                    let kb = unsafe { raw.data.keyboard };
                    if let Some(key) = keys.feed(kb.VKey, kb.Flags) {
                        if sender.send(key).is_err() {
                            break;
                        }
                        wake();
                    }
                }
            }
            unsafe { DefWindowProcW(msg.hwnd, msg.message, msg.wParam, msg.lParam) };
        }
    });
    report_rx.recv().unwrap_or(false)
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

pub fn window_pos(hwnd: isize) -> (i32, i32) {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetWindowRect(hwnd as HWND, &mut r) };
    (r.left, r.top)
}

/// Moves the window to the given screen position immediately, without activating it.
pub fn move_to(hwnd: isize, x: i32, y: i32) {
    unsafe { SetWindowPos(hwnd as HWND, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: u16 = RI_KEY_BREAK as u16;

    #[test]
    fn ctrl_alt_letter_fires_once() {
        let mut k = Keys::default();
        assert_eq!(k.feed(VK_CONTROL, 0), None);
        assert_eq!(k.feed(VK_MENU, 0), None);
        assert_eq!(k.feed(b'D' as u16, 0), Some(Hotkey::Passthrough));
        // a held key does not repeat
        assert_eq!(k.feed(b'D' as u16, 0), None);
        assert_eq!(k.feed(b'D' as u16, UP), None);
        assert_eq!(k.feed(b'D' as u16, 0), Some(Hotkey::Passthrough));
    }

    #[test]
    fn needs_both_modifiers() {
        let mut k = Keys::default();
        k.feed(VK_CONTROL, 0);
        assert_eq!(k.feed(b'X' as u16, 0), None);
        k.feed(b'X' as u16, UP);
        k.feed(VK_CONTROL, UP);
        k.feed(VK_MENU, 0);
        assert_eq!(k.feed(b'X' as u16, 0), None);
    }

    #[test]
    fn right_alt_counts_and_release_clears() {
        let mut k = Keys::default();
        k.feed(VK_CONTROL, 0);
        k.feed(VK_MENU, RI_KEY_E0);
        assert_eq!(k.feed(b'O' as u16, 0), Some(Hotkey::Opacity));
        k.feed(b'O' as u16, UP);
        k.feed(VK_MENU, RI_KEY_E0 | UP);
        assert_eq!(k.feed(b'O' as u16, 0), None);
    }

    #[test]
    fn shift_blocks_and_unknown_letter_ignored() {
        let mut k = Keys::default();
        k.feed(VK_CONTROL, 0);
        k.feed(VK_MENU, 0);
        assert_eq!(k.feed(b'Q' as u16, 0), None);
        k.feed(VK_SHIFT, 0);
        assert_eq!(k.feed(b'T' as u16, 0), None);
    }
}
