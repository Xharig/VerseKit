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

//! The snap layouts of Windows 11 for the own title bar of the main window.
//!
//! Windows shows the flyout of the snap layouts when the pointer rests on the maximize button of a window, and it
//! knows that button from the answer of the window to `WM_NCHITTEST` (`HTMAXBUTTON`). The own title bar draws its
//! buttons itself, so the window answers for it: the frame tells where the button is (in pixels of the client
//! area, [`MaxButton::set_rect`]), the window procedure answers `HTMAXBUTTON` there.
//!
//! A click on a part the system takes for the title bar does not reach the frame as a click any more, it comes
//! as `WM_NCLBUTTONDOWN` and `WM_NCLBUTTONUP`. So the window procedure keeps the click for the frame
//! ([`MaxButton::take_toggle`]), which maximizes or restores as before, and the hover for the paint of the button
//! ([`MaxButton::hot`]). Nothing here changes the window itself; where the system has no snap layouts (Windows 10)
//! the answer does no harm. Elsewhere than on Windows nothing is installed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// The answer of the window for a point on the maximize button (`HTMAXBUTTON`).
pub const HTMAXBUTTON: isize = 9;

/// What the window procedure and the frame tell each other about the maximize button.
#[derive(Default)]
pub struct MaxButton {
    /// Left, top, right, bottom in pixels of the client area; empty (all zero) before the first frame.
    rect: [AtomicI32; 4],
    hot: AtomicBool,
    toggle: AtomicBool,
}

impl MaxButton {
    pub fn new() -> Arc<MaxButton> {
        Arc::new(MaxButton::default())
    }

    /// Where the button stands now, in pixels of the client area.
    pub fn set_rect(&self, left: i32, top: i32, right: i32, bottom: i32) {
        for (slot, value) in self.rect.iter().zip([left, top, right, bottom]) {
            slot.store(value, Ordering::Relaxed);
        }
    }

    /// Whether the client point (`x`, `y`) lies on the button.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let [left, top, right, bottom] = [0, 1, 2, 3].map(|i| self.rect[i].load(Ordering::Relaxed));
        x >= left && x < right && y >= top && y < bottom
    }

    /// The pointer rests on the button (told by the system, not by the pointer events of the frame).
    pub fn hot(&self) -> bool {
        self.hot.load(Ordering::Relaxed)
    }

    /// The click on the button is noted for the frame (the window procedure does so on `WM_NCLBUTTONUP`).
    pub fn request_toggle(&self) {
        self.toggle.store(true, Ordering::Relaxed);
    }

    /// The button was clicked since the last call.
    pub fn take_toggle(&self) -> bool {
        self.toggle.swap(false, Ordering::Relaxed)
    }
}

/// The screen point a mouse message of the non-client area carries in its `lparam`: two signed 16-bit numbers
/// (a screen left of the main one has negative x).
pub fn point_of(lparam: isize) -> (i32, i32) {
    let x = i32::from((lparam & 0xFFFF) as u16 as i16);
    let y = i32::from(((lparam >> 16) & 0xFFFF) as u16 as i16);
    (x, y)
}

/// Lets the window `hwnd` answer for the button `state`; `wake` is called when `state` changed and the frame has to
/// look again. False where nothing could be installed (not Windows, no window).
pub fn install(hwnd: isize, state: Arc<MaxButton>, wake: impl Fn() + Send + Sync + 'static) -> bool {
    imp::install(hwnd, state, Box::new(wake))
}

#[cfg(windows)]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TRACKMOUSEEVENT, TrackMouseEvent};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};

    use super::{HTMAXBUTTON, MaxButton, point_of};

    const SUBCLASS_ID: usize = 0x5645_534C;
    const WM_NCDESTROY: u32 = 0x0082;
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_NCMOUSEMOVE: u32 = 0x00A0;
    const WM_NCLBUTTONDOWN: u32 = 0x00A1;
    const WM_NCLBUTTONUP: u32 = 0x00A2;
    const WM_NCMOUSELEAVE: u32 = 0x02A2;
    const TME_LEAVE: u32 = 0x2;
    const TME_NONCLIENT: u32 = 0x10;

    type Wake = Box<dyn Fn() + Send + Sync>;

    struct Data {
        state: Arc<MaxButton>,
        wake: Wake,
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        // SAFETY: `data` is the box made in `install`; it lives until WM_NCDESTROY.
        let this = unsafe { &*(data as *const Data) };
        match msg {
            WM_NCHITTEST => {
                let (x, y) = point_of(lparam);
                let mut at = POINT { x, y };
                // SAFETY: turns a screen point into one of this window's client area.
                if unsafe { ScreenToClient(hwnd, &mut at) } != 0 && this.state.contains(at.x, at.y) {
                    return HTMAXBUTTON as LRESULT;
                }
                if this.state.hot.swap(false, Ordering::Relaxed) {
                    (this.wake)();
                }
            }
            WM_NCMOUSEMOVE if wparam as isize == HTMAXBUTTON => {
                if !this.state.hot.swap(true, Ordering::Relaxed) {
                    // Without this the system never says that the pointer left the non-client area.
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    // SAFETY: asks for one WM_NCMOUSELEAVE of this window.
                    unsafe { TrackMouseEvent(&mut track) };
                    (this.wake)();
                }
            }
            WM_NCMOUSELEAVE => {
                if this.state.hot.swap(false, Ordering::Relaxed) {
                    (this.wake)();
                }
            }
            // The click is the frame's: it maximizes or restores as the button always did.
            WM_NCLBUTTONDOWN if wparam as isize == HTMAXBUTTON => return 0,
            WM_NCLBUTTONUP if wparam as isize == HTMAXBUTTON => {
                this.state.request_toggle();
                (this.wake)();
                return 0;
            }
            WM_NCDESTROY => {
                // SAFETY: the window goes away: the hook and the data with it.
                unsafe {
                    RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                    drop(Box::from_raw(data as *mut Data));
                }
            }
            _ => {}
        }
        // SAFETY: passes the message on as received.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub fn install(hwnd: isize, state: Arc<MaxButton>, wake: Wake) -> bool {
        if hwnd == 0 {
            return false;
        }
        let data = Box::into_raw(Box::new(Data { state, wake })) as usize;
        // SAFETY: the window handle is that of a running window of this thread; the data lives until it is destroyed.
        let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, data) } != 0;
        if !ok {
            // SAFETY: not taken over by the window: still ours.
            drop(unsafe { Box::from_raw(data as *mut Data) });
        }
        ok
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use windows_sys::Win32::Foundation::{HWND, POINT};
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, WS_POPUP,
        };

        use super::*;

        /// A window of the class STATIC that is never shown: no window on any screen.
        fn hidden_window() -> HWND {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            // SAFETY: creates a window of a system class, never shown, destroyed by the test.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    300,
                    200,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null(), "the hidden window was made");
            hwnd
        }

        fn screen_lparam(hwnd: HWND, x: i32, y: i32) -> isize {
            let mut at = POINT { x, y };
            // SAFETY: turns a client point of the window into a screen point.
            unsafe { ClientToScreen(hwnd, &mut at) };
            ((at.y as i16 as u16 as isize) << 16) | (at.x as i16 as u16 as isize)
        }

        #[test]
        fn the_window_answers_maximize_button_on_the_button_and_keeps_the_click() {
            let hwnd = hidden_window();
            let state = MaxButton::new();
            let woken = Arc::new(AtomicUsize::new(0));
            let counted = woken.clone();
            assert!(install(
                hwnd as isize,
                state.clone(),
                Box::new(move || {
                    counted.fetch_add(1, Ordering::Relaxed);
                })
            ));
            state.set_rect(100, 0, 146, 32);
            let ask = |x: i32, y: i32| {
                // SAFETY: a message to a window of this thread.
                unsafe { SendMessageW(hwnd, WM_NCHITTEST, 0, screen_lparam(hwnd, x, y)) }
            };
            assert_eq!(ask(120, 10), HTMAXBUTTON, "on the button");
            assert_ne!(ask(50, 10), HTMAXBUTTON, "beside it");
            assert_ne!(ask(120, 40), HTMAXBUTTON, "below it");
            // Pointer on the button: hot; the click comes down and up, the frame gets it once.
            // SAFETY: messages to a window of this thread.
            unsafe {
                SendMessageW(hwnd, WM_NCMOUSEMOVE, HTMAXBUTTON as usize, screen_lparam(hwnd, 120, 10));
                assert!(state.hot());
                assert_eq!(SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTMAXBUTTON as usize, 0), 0);
                assert!(!state.take_toggle(), "not before the button is released");
                assert_eq!(SendMessageW(hwnd, WM_NCLBUTTONUP, HTMAXBUTTON as usize, 0), 0);
            }
            assert!(state.take_toggle());
            assert!(!state.take_toggle(), "once");
            // A click on another part is not ours.
            // SAFETY: a message to a window of this thread.
            unsafe { SendMessageW(hwnd, WM_NCLBUTTONUP, 2, 0) };
            assert!(!state.take_toggle());
            // The pointer goes: no longer hot.
            ask(50, 10);
            assert!(!state.hot());
            assert!(woken.load(Ordering::Relaxed) >= 2, "the frame is woken when something changes");
            // SAFETY: ends the window of this test.
            unsafe { DestroyWindow(hwnd) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::Arc;

    use super::MaxButton;

    pub fn install(_hwnd: isize, _state: Arc<MaxButton>, _wake: Box<dyn Fn() + Send + Sync>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_point_of_a_message_has_signed_numbers() {
        assert_eq!(point_of((20 << 16) | 10), (10, 20));
        // A screen left of the main one: x = -1920, y = -5.
        let lparam = (((-5i16) as u16 as isize) << 16) | ((-1920i16) as u16 as isize);
        assert_eq!(point_of(lparam), (-1920, -5));
    }

    #[test]
    fn the_button_holds_its_rect_and_nothing_before_the_first_frame() {
        let state = MaxButton::new();
        assert!(!state.contains(0, 0), "nothing before the frame told");
        state.set_rect(100, 0, 146, 32);
        assert!(state.contains(100, 0) && state.contains(145, 31));
        assert!(!state.contains(146, 10) && !state.contains(99, 10) && !state.contains(120, 32));
    }
}
