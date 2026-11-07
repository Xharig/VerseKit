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

//! A small piece of the screen as pixels, for the signature scanner.
//!
//! | System | Way | State |
//! |---|---|---|
//! | Windows | GDI (`BitBlt`) | built |
//! | Linux | Wayland portal | not built |
//!
//! ⚠ Physical pixels, not the scaled ones of a program without DPI awareness: grabbed in the scaled
//! computation the picture is compressed and washed out, and with digits nine points wide that is the end of
//! every reading. The grabbing thread switches itself to DPI aware for the length of the call.
//!
//! ⚠ A layered window (the result window of the scanner) is left out of the picture: `BitBlt` without
//! `CAPTUREBLT` does not take layered windows along. That is wanted: the window must not photograph itself.
//!
//! The result is BGRA bytes, top row first.

/// Why a grab did not work; the key words the pages know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrabError {
    /// This system cannot grab.
    Unsupported,
    /// The area is empty or larger than 16 million points.
    BadArea,
    /// The system refused the picture.
    Failed,
}

impl GrabError {
    /// The key word of the text key `scan_grund_<word>`.
    pub fn key(self) -> &'static str {
        match self {
            GrabError::Unsupported => "nicht_unterstuetzt",
            GrabError::BadArea => "bereich_ungueltig",
            GrabError::Failed => "abgriff_fehlgeschlagen",
        }
    }
}

/// The most points one grab may hold: an 8K window is never readable anyway.
pub const MAX_POINTS: i64 = 16_000_000;

/// The file name of the game, in lower case.
pub const GAME_EXE: &str = "starcitizen.exe";

/// Is `path` the program of the game?
pub fn is_game_path(path: &str) -> bool {
    path.replace('/', "\\").rsplit('\\').next().is_some_and(|name| name.eq_ignore_ascii_case(GAME_EXE))
}

/// Can this system grab at all?
pub fn supported() -> bool {
    cfg!(windows)
}

/// A rectangle: left, top, width, height in physical screen points.
pub type Area = (i32, i32, i32, i32);

#[cfg(windows)]
mod system {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{CloseHandle, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };

    use super::{Area, GrabError, MAX_POINTS, is_game_path};

    /// Switches this thread to physical points while it lives and back afterwards.
    struct Aware(isize);

    impl Aware {
        fn new() -> Aware {
            // SAFETY: a plain switch of the calling thread; zero comes back where the system cannot do it.
            Aware(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } as isize)
        }
    }

    impl Drop for Aware {
        fn drop(&mut self) {
            if self.0 != 0 {
                // SAFETY: gives back the context the call above returned.
                unsafe { SetThreadDpiAwarenessContext(self.0 as _) };
            }
        }
    }

    pub fn grab_raw(area: Area) -> Result<Vec<u8>, GrabError> {
        let (left, top, width, height) = area;
        if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > MAX_POINTS {
            return Err(GrabError::BadArea);
        }
        let _aware = Aware::new();
        // SAFETY: every handle made below is released in the reverse order before the function ends.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return Err(GrabError::Failed);
            }
            let memory = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let result = if memory.is_null() || bitmap.is_null() || bits.is_null() {
                Err(GrabError::Failed)
            } else {
                let old = SelectObject(memory, bitmap);
                let copied = BitBlt(memory, 0, 0, width, height, screen, left, top, SRCCOPY) != 0;
                let bytes = if copied {
                    Ok(std::slice::from_raw_parts(bits as *const u8, width as usize * height as usize * 4).to_vec())
                } else {
                    Err(GrabError::Failed)
                };
                SelectObject(memory, old);
                bytes
            };
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !memory.is_null() {
                DeleteDC(memory);
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            result
        }
    }

    /// The foreground window and the file name of its program.
    fn foreground() -> Option<(*mut c_void, String)> {
        // SAFETY: calls without arguments the system owns; a zero handle is "no window".
        let window = unsafe { GetForegroundWindow() };
        program_of(window).map(|path| (window, path))
    }

    /// The path of the program a window belongs to.
    fn program_of(window: *mut c_void) -> Option<String> {
        if window.is_null() {
            return None;
        }
        let mut pid = 0u32;
        // SAFETY: `pid` lives during the call.
        unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
        if pid == 0 {
            return None;
        }
        // SAFETY: the handle has the least rights and is closed below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer is as large as `size` says and lives during the call.
        let read = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &raw mut size) } != 0;
        // SAFETY: opened above.
        unsafe { CloseHandle(handle) };
        read.then(|| String::from_utf16_lossy(&buffer[..size as usize]))
    }

    /// The window of the game wherever it stands, in front or not: the largest visible one of its program.
    pub fn find_game_rect() -> Option<Area> {
        unsafe extern "system" fn each(window: *mut c_void, found: isize) -> i32 {
            // SAFETY: `found` is the address of the `Option<Area>` of the call below, alive during the call.
            let best = unsafe { &mut *(found as *mut Option<Area>) };
            // SAFETY: a window handle the system hands over; asking is harmless.
            if unsafe { IsWindowVisible(window) } == 0 || !program_of(window).is_some_and(|p| is_game_path(&p)) {
                return 1;
            }
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            // SAFETY: `rect` lives during the call.
            if unsafe { GetWindowRect(window, &raw mut rect) } != 0 {
                let area = (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top);
                if area.2 >= 200
                    && area.3 >= 200
                    && best.is_none_or(|b| i64::from(area.2) * i64::from(area.3) > i64::from(b.2) * i64::from(b.3))
                {
                    *best = Some(area);
                }
            }
            1
        }
        let _aware = Aware::new();
        let mut best: Option<Area> = None;
        // SAFETY: the callback only reads windows and writes to `best`, which outlives the call.
        unsafe { EnumWindows(Some(each), (&raw mut best) as isize) };
        best
    }

    pub fn foreground_is_game() -> bool {
        foreground().is_some_and(|(_, path)| is_game_path(&path))
    }

    pub fn game_rect() -> Option<Area> {
        let (window, path) = foreground()?;
        if !is_game_path(&path) {
            return None;
        }
        let _aware = Aware::new();
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: `rect` lives during the call.
        if unsafe { GetWindowRect(window, &raw mut rect) } == 0 {
            return None;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        (width >= 200 && height >= 200).then_some((rect.left, rect.top, width, height))
    }
}

/// The window of the game wherever it stands (also when another window is in front); `None` where the game is
/// not running or this system cannot ask.
pub fn find_game_rect() -> Option<Area> {
    #[cfg(windows)]
    {
        system::find_game_rect()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Takes a piece of the screen as BGRA bytes (top row first).
pub fn grab_raw(area: Area) -> Result<Vec<u8>, GrabError> {
    #[cfg(windows)]
    {
        system::grab_raw(area)
    }
    #[cfg(not(windows))]
    {
        let _ = area;
        Err(GrabError::Unsupported)
    }
}

/// Is Star Citizen the active window? Read only while it is: otherwise the scanner photographs the desktop, and
/// a browser with a number in it would be a hit.
pub fn foreground_is_game() -> bool {
    #[cfg(windows)]
    {
        system::foreground_is_game()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// The area of the window of the game in physical points, only while the game is in front.
pub fn game_rect() -> Option<Area> {
    #[cfg(windows)]
    {
        system::game_rect()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_is_known_by_the_file_name_of_its_program_in_any_spelling() {
        assert!(is_game_path(r"C:\Games\StarCitizen\LIVE\Bin64\StarCitizen.exe"));
        assert!(is_game_path("C:/Games/StarCitizen/LIVE/Bin64/starcitizen.exe"));
        assert!(!is_game_path(r"C:\Program Files\Mozilla Firefox\firefox.exe"));
        assert!(!is_game_path(r"C:\StarCitizen\Launcher\RSI Launcher.exe"));
        assert!(!is_game_path(""));
    }

    #[test]
    fn an_empty_or_too_large_area_is_refused_before_anything_is_asked_of_the_system() {
        if supported() {
            assert_eq!(grab_raw((0, 0, 0, 10)), Err(GrabError::BadArea));
            assert_eq!(grab_raw((0, 0, 10, -1)), Err(GrabError::BadArea));
            assert_eq!(grab_raw((0, 0, 8000, 4000)), Err(GrabError::BadArea));
        } else {
            assert_eq!(grab_raw((0, 0, 10, 10)), Err(GrabError::Unsupported));
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_small_piece_of_the_screen_comes_back_in_the_size_asked_for_in_four_bytes_a_point() {
        // Nothing opens a window: only the picture of a corner of the desktop is taken.
        let raw = grab_raw((0, 0, 40, 20)).expect("grab");
        assert_eq!(raw.len(), 40 * 20 * 4);
    }

    #[test]
    fn the_window_of_the_game_is_looked_for_without_it_being_in_front() {
        // Without the game on this computer both answers are none; with it in front they name the same window.
        let found = find_game_rect();
        if let Some(front) = game_rect() {
            assert_eq!(found, Some(front));
        }
        if let Some((_, _, w, h)) = found {
            assert!(w >= 200 && h >= 200);
        }
    }

    #[test]
    fn the_keys_of_the_failures_are_the_words_of_the_pages() {
        assert_eq!(GrabError::Unsupported.key(), "nicht_unterstuetzt");
        assert_eq!(GrabError::BadArea.key(), "bereich_ungueltig");
        assert_eq!(GrabError::Failed.key(), "abgriff_fehlgeschlagen");
    }
}
