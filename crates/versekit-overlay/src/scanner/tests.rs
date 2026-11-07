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

//! The scanner in the overlay with a made-up screen, on a desktop that is never shown.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::screen_grab::{Area, GrabError};
use versekit_core::signature_scan::Gray;
use versekit_core::signature_watch::{LAST_FRAME_FILE, read_last_frame};
use versekit_core::testdir;
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, CreateDesktopW, SetThreadDesktop};

use super::*;

const PICTURES: &str = include_str!("../../../versekit-core/tests/data/signatur-pruefbilder.json");

fn crop() -> (u64, Gray) {
    let file = jsonio::loads(PICTURES).unwrap();
    let Some(Value::Object(images)) = file.get("bilder") else { panic!("bilder") };
    let (key, raster) = &images[0];
    let number: String = key.split('#').next().unwrap().chars().filter(char::is_ascii_digit).collect();
    let Value::Array(rows) = raster else { panic!("rows") };
    let (mut data, mut w) = (Vec::new(), 0);
    for row in rows {
        let Value::Array(row) = row else { panic!("row") };
        w = row.len();
        data.extend(row.iter().map(|v| if let Value::Int(v) = v { v.parse::<u8>().unwrap() } else { 0 }));
    }
    (number.parse().unwrap(), Gray::new(w, rows.len(), data))
}

const GAME: Area = (100, 50, 1000, 700);
const AT: (usize, usize) = (420, 300);

/// The game window with or without the crop; the area asked for is cut out of the picture.
struct Screen1 {
    pill: Arc<AtomicBool>,
    with: Vec<u8>,
    without: Vec<u8>,
}

impl Source for Screen1 {
    fn game(&mut self) -> Option<Area> {
        Some(GAME)
    }
    fn grab(&mut self, area: Area) -> Result<Vec<u8>, GrabError> {
        let raw = if self.pill.load(Ordering::Relaxed) { &self.with } else { &self.without };
        let mut out = Vec::new();
        for y in area.1..area.1 + area.3 {
            let start = (((y - GAME.1) * GAME.2 + (area.0 - GAME.0)) * 4) as usize;
            out.extend_from_slice(&raw[start..start + area.2 as usize * 4]);
        }
        Ok(out)
    }
}

fn screens(crop: &Gray) -> (Vec<u8>, Vec<u8>) {
    let mut without = vec![0u8; (GAME.2 * GAME.3 * 4) as usize];
    for p in without.as_chunks_mut::<4>().0 {
        *p = [5, 12, 7, 255];
    }
    let mut with = without.clone();
    for y in 0..crop.h {
        for x in 0..crop.w {
            let v = crop.data[y * crop.w + x];
            let at = ((AT.1 + y) * GAME.2 as usize + AT.0 + x) * 4;
            with[at..at + 4].copy_from_slice(&[v.saturating_add(60), v, 255, 255]);
        }
    }
    (with, without)
}

fn on_hidden_desktop<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::spawn(move || {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let name: Vec<u16> =
            format!("VerseKit-Pruefung-Scanner-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
        // SAFETY: a new desktop for this thread only; closed after the closure has ended.
        let desktop = unsafe {
            CreateDesktopW(name.as_ptr(), std::ptr::null(), std::ptr::null(), 0, 0x1000_0000, std::ptr::null())
        };
        assert!(!desktop.is_null() && unsafe { SetThreadDesktop(desktop) } != 0, "no hidden desktop");
        let result = f();
        unsafe { CloseDesktop(desktop) };
        result
    })
    .join()
    .expect("check thread")
}

/// A copy of the data on a made-up computer: nothing of the shell the check runs in counts.
fn copy_in(root: &std::path::Path) -> Paths {
    std::fs::create_dir_all(root.join(versekit_core::paths::COPY_DATA_FOLDER)).expect("data folder");
    let host = Host::new(versekit_core::pypath::Flavor::native(), &[], &root.to_string_lossy());
    Paths::for_copy(host, &root.to_string_lossy()).expect("a copy")
}

/// Steps until `what` holds; gives the place of the pill that was reported on the way.
fn wait_for(scanner: &mut Scanner, scheme: &Scheme, what: impl Fn(&Scanner) -> bool) -> Option<(String, [i32; 4])> {
    let mut place = None;
    for _ in 0..300 {
        place = scanner.step(scheme).place.or(place);
        if what(scanner) {
            return place;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("not reached");
}

#[test]
fn a_run_that_may_not_grab_has_no_buttons_no_thread_and_no_window() {
    for (paths, real) in [(None, true), (Some(testdir::fresh("scanner-kopie")), false)] {
        let paths = paths.map(|root| copy_in(&root));
        let mut scanner = Scanner::new(paths.as_ref(), real);
        assert!(!scanner.active());
        scanner.apply(true, Layout::default(), &Hint::default(), false, || {});
        assert!(!scanner.is_on() && !scanner.alive());
        assert!(scanner.extras().is_empty());
        assert_eq!(scanner.step(versekit_core::theme::current()), Step::default());
    }
}

#[test]
fn the_window_stands_above_the_pill_while_the_number_is_read_follows_the_layout_and_goes_when_it_is_gone() {
    let (number, crop) = crop();
    let (with, without) = screens(&crop);
    let root = testdir::fresh("scanner-lauf");
    let results = on_hidden_desktop(move || {
        let paths = copy_in(&root);
        let mut scanner = Scanner::new(Some(&paths), true);
        assert!(scanner.active());
        let scheme = versekit_core::theme::current();
        let pill = Arc::new(AtomicBool::new(true));
        let source = Screen1 { pill: pill.clone(), with, without };
        scanner.apply_from(true, Layout::default(), &Hint::default(), false, || {}, || Box::new(source));
        assert!(scanner.is_on());
        // The buttons: the eye in the accent and the teaching.
        let extras = scanner.extras();
        assert_eq!(extras.iter().map(|e| e.id).collect::<Vec<_>>(), [EYE, LEARN]);
        assert_eq!(extras[0].icon, "eye");
        assert_eq!(extras[0].tone, Tone::Lit);

        let place = wait_for(&mut scanner, scheme, |s| s.shown.is_some());
        // The first sure find is handed on to be kept for this size of the game window, once.
        assert_eq!(place.map(|p| p.0), Some("1000x700".to_string()));
        assert_eq!(scanner.step(scheme).place, None);
        let window = scanner.window.as_ref().expect("window");
        assert!(window.visible());
        let text = scanner.shown.clone().expect("text");
        assert_eq!(text.number, signature_window::with_thousands(number));
        // Directly above the pill, inside the game window.
        let mut rect = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(window.hwnd() as _, &mut rect) };
        let (left, top, width, height) = (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top);
        let state = scanner.watch.as_ref().unwrap().state();
        let (pill_box, game) = (state.pill.expect("pill"), state.game.expect("game"));
        assert_eq!(game, GAME);
        assert!(top + height <= pill_box.1, "window {top}+{height} reaches into the pill at {}", pill_box.1);
        assert!(left >= GAME.0 && left + width <= GAME.0 + GAME.2 && top >= GAME.1);
        assert!((left + width / 2 - (pill_box.0 + pill_box.2 / 2)).abs() <= 2, "centred on the pill");

        // The picture for teaching was left for the main window.
        let frame_file = format!("{}/{LAST_FRAME_FILE}", scanner.folder.clone().unwrap());
        wait_for(&mut scanner, scheme, |_| read_last_frame(&frame_file).is_some());
        assert_eq!(read_last_frame(&frame_file).unwrap().value, Some(number));

        // A new layout moves it at once, the lock open lets it take the mouse.
        let moved = Layout { dx: 100, dy: -100, ..Layout::default() };
        scanner.apply_from(true, moved, &Hint::default(), true, || {}, || unreachable!("the watch runs already"));
        scanner.step(scheme);
        let window = scanner.window.as_ref().unwrap();
        let mut again = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(window.hwnd() as _, &mut again) };
        assert_eq!(again.left - left, 100, "100 per mille of 1000 points to the right");
        assert_eq!(again.top - top, -70, "100 per mille of 700 points up");
        assert_eq!(window.ex_style() & windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TRANSPARENT, 0);

        // The info bar shows the reading: the window closes, the reading stays; without the bar it is back.
        scanner.set_window_off(true);
        scanner.step(scheme);
        assert!(!scanner.window.as_ref().unwrap().visible(), "no window beside the info bar");
        assert!(scanner.shown.is_some(), "the bar still gets the reading");
        scanner.set_window_off(false);
        scanner.step(scheme);
        assert!(scanner.window.as_ref().unwrap().visible(), "the window is back without the bar");

        // The pill is gone: after the grace of the watch the reading stays for the time the layout says, the
        // window with it; the overlay is told when to wake.
        pill.store(false, Ordering::Relaxed);
        wait_for(&mut scanner, scheme, |s| s.clear_at.is_some());
        assert!(scanner.shown.is_some(), "still shown while it stays");
        assert!(scanner.window.as_ref().unwrap().visible(), "the window stays too");
        let left = scanner.wake_in().expect("a time to wake");
        let stays = moved.linger as f64 - signature_watch::CLEAR_S;
        assert!(left.as_secs_f64() <= stays && left.as_secs_f64() > stays - 1.0, "{left:?} for {stays} s");
        // Its time over: it goes at the next frame.
        scanner.clear_at = Some(Instant::now());
        scanner.step(scheme);
        assert!(scanner.shown.is_none() && scanner.wake_in().is_none());
        assert!(!scanner.window.as_ref().unwrap().visible());
        // Switched off: the thread ends and the eye is grey.
        scanner.apply_from(false, moved, &Hint::default(), true, || {}, || unreachable!());
        assert!(!scanner.alive());
        assert_eq!(scanner.extras()[0].icon, "eye-off");
        assert_eq!(scanner.extras()[0].tone, Tone::Normal);
        true
    });
    assert!(results);
}

#[test]
fn a_drag_of_the_window_gives_the_offset_for_the_place_it_was_left_at() {
    let (_, crop) = crop();
    let (with, without) = screens(&crop);
    let root = testdir::fresh("scanner-ziehen");
    on_hidden_desktop(move || {
        let paths = copy_in(&root);
        let mut scanner = Scanner::new(Some(&paths), true);
        let scheme = versekit_core::theme::current();
        let source = Screen1 { pill: Arc::new(AtomicBool::new(true)), with, without };
        scanner.apply_from(true, Layout::default(), &Hint::default(), true, || {}, || Box::new(source));
        wait_for(&mut scanner, scheme, |s| s.shown.is_some());
        let hwnd = scanner.window.as_ref().unwrap().hwnd() as isize;
        // A press and release at the same place: the window stays and the offset is the one it has.
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP};
            SendMessageW(hwnd as _, WM_LBUTTONDOWN, 0, 0);
            SendMessageW(hwnd as _, WM_LBUTTONUP, 0, 0);
        }
        let step = scanner.step(scheme);
        assert_eq!(step.offset, Some((0, 0)));
        scanner.close();
    });
}
