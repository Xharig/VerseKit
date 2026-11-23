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

//! The notice after a scan by hotkey ([`versekit_core::scan_notice`]), shown for a moment in the middle over the
//! game and hidden again by itself.
//!
//! Under Windows it is the window of the scanned material ([`crate::scan_window::ScanWindow`]) used once more: a
//! card with the brackets of the cards, never activated, always on top, clicks pass through it, and the screen grab
//! leaves it out. The title stands big (in the accent for a taken scan), the line below small. Elsewhere there is
//! no such window; the caller shows the notice as a hint row instead ([`Notices::show`] says so).
//!
//! All calls belong to the thread that runs the frames of the overlay.

use std::time::{Duration, Instant};

use versekit_core::scan_notice::Notice;
use versekit_core::theme::Scheme;

/// The notice on the screen and until when it stays.
#[derive(Default)]
pub struct Notices {
    #[cfg(windows)]
    window: Option<crate::scan_window::ScanWindow>,
    until: Option<Instant>,
}

impl Notices {
    /// Shows `notice` over the game `area` (left, top, width, height in screen points) from `now` on, replacing one
    /// shown. `false` where no window can show it: the caller then says it otherwise.
    pub fn show(&mut self, notice: &Notice, area: (i32, i32, i32, i32), scheme: &Scheme, now: Instant) -> bool {
        #[cfg(windows)]
        {
            use crate::scan_window::{Content, ScanWindow, colorref};
            if self.window.is_none() {
                self.window = ScanWindow::create();
            }
            let Some(window) = &self.window else { return false };
            // The stock of the unit stands in gold, in the frame and (where taken) in the title; everything else in the
            // accent of the scheme.
            let tint = match notice.target {
                Some(versekit_core::scan_notice::Target::Unit) => scheme.gold,
                _ => scheme.accent,
            };
            let content = Content {
                material: notice.shown_title(),
                known: notice.kind.accent(),
                number: notice.detail.clone(),
                extra: Vec::new(),
                font_px: versekit_core::scan_notice::font_px(area.3),
                background: colorref(scheme.surface),
                text: colorref(scheme.fg),
                accent: colorref(tint),
                sub: colorref(scheme.sub),
                line: colorref(scheme.line),
                radius: i32::from(versekit_ui::box_radius(scheme).nw),
                square: scheme.square,
            };
            let size = window.set_content(content);
            window.show_at(versekit_core::scan_notice::place(area, size));
            self.until = Some(now + notice.duration());
            true
        }
        #[cfg(not(windows))]
        {
            let _ = (notice, area, scheme, now);
            false
        }
    }

    /// Hides the notice once its time is up. The time it still stays, for the next frame; `None` when none is shown.
    pub fn expire(&mut self, now: Instant) -> Option<Duration> {
        let until = self.until?;
        if now < until {
            return Some(until - now);
        }
        self.until = None;
        #[cfg(windows)]
        if let Some(window) = &self.window {
            window.hide();
        }
        None
    }

    /// Whether a notice is shown just now.
    #[cfg(test)]
    pub fn showing(&self) -> bool {
        self.until.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_goes_by_itself_after_its_time() {
        let mut notices = Notices::default();
        let now = Instant::now();
        // Without a window (or where the system refuses one) nothing is shown and nothing waits.
        notices.until = Some(now + Duration::from_secs(2));
        assert_eq!(notices.expire(now + Duration::from_millis(500)), Some(Duration::from_millis(1500)));
        assert!(notices.showing());
        assert_eq!(notices.expire(now + Duration::from_secs(2)), None);
        assert!(!notices.showing());
        assert_eq!(notices.expire(now + Duration::from_secs(3)), None);
    }

    /// Every notice of the blueprint scan, in both languages and on the usual screens, is shown, painted and hidden
    /// again, one after the other in the same window, as the hotkey does it. The window stands on a desktop of its
    /// own that is never shown.
    #[cfg(windows)]
    #[test]
    fn every_notice_of_the_blueprint_scan_is_shown_and_painted() {
        use versekit_core::blueprint_scan::{Action, notice_in};
        use versekit_core::language::Lang;
        use windows_sys::Win32::Graphics::Gdi::UpdateWindow;
        use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, CreateDesktopW, SetThreadDesktop};
        use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW};

        std::thread::spawn(|| {
            let name: Vec<u16> = format!("VerseKit-Pruefung-Hinweis-{}", std::process::id())
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: a new desktop for this thread only; it is closed at the end.
            let desktop = unsafe {
                CreateDesktopW(name.as_ptr(), std::ptr::null(), std::ptr::null(), 0, 0x1000_0000, std::ptr::null())
            };
            assert!(!desktop.is_null(), "no desktop");
            assert!(unsafe { SetThreadDesktop(desktop) } != 0, "thread stays on the visible desktop");
            let mut notices = Notices::default();
            let actions = [
                Action::Add("Arrowhead Sniper Rifle".into()),
                Action::AlreadyThere("P4-AR Rifle".into()),
                Action::Confirm(vec!["FS-9 LMG".into(), "F55 LMG".into(), "Demeco LMG".into()]),
                Action::NotFound,
            ];
            for area in [(0, 0, 1920, 1080), (0, 0, 2560, 1440), (0, 0, 3840, 2160), (-1920, 0, 1920, 1080)] {
                for lang in [Lang::De, Lang::En] {
                    for scheme in ["krt", "eis"] {
                        for action in &actions {
                            let notice = notice_in(lang, action);
                            let now = Instant::now();
                            assert!(notices.show(&notice, area, versekit_core::theme::by_id(scheme).expect("scheme"), now));
                            let hwnd = notices.window.as_ref().expect("window").hwnd() as _;
                            // SAFETY: our own window; its messages are handled here, on its thread.
                            unsafe {
                                UpdateWindow(hwnd);
                                let mut msg: MSG = std::mem::zeroed();
                                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                                    DispatchMessageW(&msg);
                                }
                            }
                            assert_eq!(notices.expire(now + notice.duration()), None);
                        }
                    }
                }
            }
            drop(notices);
            // SAFETY: the desktop of this thread, no window left on it.
            unsafe { CloseDesktop(desktop) };
        })
        .join()
        .expect("check thread");
    }
}
