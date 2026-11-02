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

//! The window is made on a desktop of its own that is never shown: nothing appears on the screen of the player
//! and nothing takes the keyboard focus from him.

use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, CreateDesktopW, SetThreadDesktop};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, SendMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

use super::*;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Runs `f` in a thread that sits on a new desktop; the desktop goes away afterwards.
fn on_hidden_desktop<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::spawn(move || {
        let name: Vec<u16> =
            format!("VerseKit-Pruefung-Signatur-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
        // SAFETY: a new desktop for this thread only; it is closed after the closure has ended.
        let desktop = unsafe {
            CreateDesktopW(name.as_ptr(), std::ptr::null(), std::ptr::null(), 0, 0x1000_0000, std::ptr::null())
        };
        assert!(!desktop.is_null(), "no desktop");
        assert!(unsafe { SetThreadDesktop(desktop) } != 0, "thread stays on the visible desktop");
        let result = f();
        unsafe { CloseDesktop(desktop) };
        result
    })
    .join()
    .expect("check thread")
}

fn content(material: &str, number: &str, font_px: i32) -> Content {
    Content {
        material: material.into(),
        known: true,
        number: number.into(),
        font_px,
        background: colorref(versekit_core::theme::Color::hex("#101418")),
        text: colorref(versekit_core::theme::Color::hex("#f0f0f0")),
        accent: colorref(versekit_core::theme::Color::hex("#9ce430")),
        sub: colorref(versekit_core::theme::Color::hex("#a0a8b0")),
        line: colorref(versekit_core::theme::Color::hex("#3a4658")),
        radius: i32::from(versekit_ui::BOX_RADIUS),
        square: false,
    }
}

#[test]
fn the_window_is_layered_topmost_never_activated_and_lets_clicks_through() {
    let style = on_hidden_desktop(|| {
        let window = ScanWindow::create().expect("window");
        let style = window.ex_style();
        let foreground = unsafe { GetForegroundWindow() };
        let (w, h) = window.set_content(content("4× Quantainium", "12,680", 24));
        window.show_at((100, 100, w, h));
        // Shown, and still not the window in front.
        assert!(window.visible());
        assert_ne!(unsafe { GetForegroundWindow() }, window.hwnd() as HWND);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        window.hide();
        assert!(!window.visible());
        style
    });
    for (flag, name) in [
        (WS_EX_LAYERED, "layered (left out of the screen grab)"),
        (WS_EX_TOPMOST, "topmost"),
        (WS_EX_NOACTIVATE, "never activated"),
        (WS_EX_TRANSPARENT, "clicks pass through"),
        (WS_EX_TOOLWINDOW, "no entry in the task bar"),
    ] {
        assert!(style & flag != 0, "not {name}: {style:#x}");
    }
}

#[test]
fn the_lock_open_makes_the_window_take_the_mouse_and_closed_lets_it_pass() {
    on_hidden_desktop(|| {
        let window = ScanWindow::create().expect("window");
        window.set_draggable(true);
        assert_eq!(window.ex_style() & WS_EX_TRANSPARENT, 0);
        assert!(window.ex_style() & WS_EX_NOACTIVATE != 0, "the focus stays with the game also when it can be dragged");
        window.set_draggable(false);
        assert!(window.ex_style() & WS_EX_TRANSPARENT != 0);
    });
}

#[test]
fn a_press_and_release_on_the_open_window_reports_where_it_stands_and_on_the_closed_one_nothing() {
    on_hidden_desktop(|| {
        let window = ScanWindow::create().expect("window");
        let (w, h) = window.set_content(content("", "12,680", 24));
        window.show_at((300, 200, w, h));
        let hwnd = window.hwnd() as HWND;
        // Closed: the press is not a drag.
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0);
            SendMessageW(hwnd, WM_LBUTTONUP, 0, 0);
        }
        assert_eq!(window.take_moved(), None);
        assert!(!window.dragging());
        // Open: the press starts a drag, the release ends it and hands over the place.
        window.set_draggable(true);
        unsafe { SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0) };
        assert!(window.dragging());
        unsafe { SendMessageW(hwnd, WM_LBUTTONUP, 0, 0) };
        assert!(!window.dragging());
        assert_eq!(window.take_moved(), Some((300, 200)));
        assert_eq!(window.take_moved(), None, "handed over once");
    });
}

#[test]
fn a_drag_whose_mouse_is_taken_away_ends_with_the_window_where_it_stands() {
    on_hidden_desktop(|| {
        let window = ScanWindow::create().expect("window");
        let (w, h) = window.set_content(content("", "12,680", 24));
        window.show_at((300, 200, w, h));
        let hwnd = window.hwnd() as HWND;
        window.set_draggable(true);
        unsafe { SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0) };
        assert!(window.dragging());
        // No release will come: the window gets the message that the mouse was taken.
        unsafe { SendMessageW(hwnd, WM_CAPTURECHANGED, 0, 0) };
        assert!(!window.dragging(), "the drag does not hang on");
        assert_eq!(window.take_moved(), Some((300, 200)));
        // A press afterwards starts a new drag as usual.
        unsafe { SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0) };
        assert!(window.dragging());
        unsafe { SendMessageW(hwnd, WM_LBUTTONUP, 0, 0) };
        assert!(!window.dragging());
        // The release of the mouse that follows a finished drag is no second drag end.
        assert_eq!(window.take_moved(), Some((300, 200)));
        unsafe { SendMessageW(hwnd, WM_CAPTURECHANGED, 0, 0) };
        assert_eq!(window.take_moved(), None);
    });
}

#[test]
fn the_content_is_drawn_in_the_accent_and_a_longer_text_and_bigger_letters_need_more_room() {
    let (small, long, big) = (
        measure(&content("2× Bexalite", "6,340", 24)),
        measure(&content("2× Bexalite · 1× Taranite", "6,340", 24)),
        measure(&content("2× Bexalite", "6,340", 40)),
    );
    assert!(long.0 > small.0 && long.1 == small.1, "{small:?} {long:?}");
    assert!(big.0 > small.0 && big.1 > small.1, "{small:?} {big:?}");
    let without = measure(&content("", "6,340", 24));
    assert!(without.1 < small.1, "no material: one line");

    let ink = |material: &str| {
        on_hidden_desktop({
            let material = material.to_string();
            move || {
                let window = ScanWindow::create().expect("window");
                let (w, h) = window.set_content(content(&material, "12,680", 24));
                window.show_at((50, 50, w, h));
                let (pw, ph, pixels) = window.snapshot().expect("snapshot");
                // Inside the frame, pixels near the accent colour (0x9ce430 as BGRA).
                let near = |p: u32| {
                    let (b, g, r) = (p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF);
                    r.abs_diff(0x9c) < 40 && g.abs_diff(0xe4) < 40 && b.abs_diff(0x30) < 60
                };
                (0..ph)
                    .flat_map(|y| (0..pw).map(move |x| (x, y)))
                    .filter(|&(x, y)| x > 3 && y > 3 && x < pw - 4 && y < ph - 4 && near(pixels[(y * pw + x) as usize]))
                    .count()
            }
        })
    };
    assert!(ink("4× Quantainium") > 40, "the material stands in the accent");
    assert!(ink("") < 10, "without material there is no accent text");
}

/// The rows (top, bottom) the ink of `text` covers when it is drawn in the font of the window without any clipping.
fn ink_rows(px: i32, text: &str) -> (i32, i32) {
    use windows_sys::Win32::Graphics::Gdi::{
        BLACK_BRUSH, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, FillRect, GetDC, GetPixel,
        GetStockObject, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    };
    let (width, height) = (px * 40, px * 4);
    // SAFETY: a screen device context, a bitmap and a font made and given back here.
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, bitmap);
        let face = font(px);
        SelectObject(memory, face);
        FillRect(memory, &RECT { left: 0, top: 0, right: width, bottom: height }, GetStockObject(BLACK_BRUSH) as _);
        SetBkMode(memory, TRANSPARENT as i32);
        SetTextColor(memory, 0x00ff_ffff);
        let letters = wide(text);
        TextOutW(memory, px, px, letters.as_ptr(), letters.len() as i32);
        let (mut top, mut bottom) = (i32::MAX, i32::MIN);
        for y in 0..height {
            if (0..width).any(|x| GetPixel(memory, x, y) & 0xff > 0x60) {
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
        SelectObject(memory, old);
        DeleteObject(face);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
        // Relative to where the line begins.
        (top - px, bottom - px)
    }
}

const GAME_HEIGHTS: [i32; 3] = [720, 1080, 1440];

#[test]
fn a_line_has_room_for_the_tails_of_g_p_and_y_at_every_size_of_the_window() {
    use versekit_core::signature_window::Size;
    for size in Size::ALL {
        for game in GAME_HEIGHTS {
            let px = size.font_px(game);
            let m = metrics(px);
            for text in ["Gypsy · 6× Bergung", "gpy"] {
                let (top, bottom) = ink_rows(m.big, text);
                assert!(top >= 0, "{size:?} {game}: the head of the letters is above the line ({top})");
                assert!(
                    bottom < m.big_height,
                    "{size:?} {game} {text:?}: the tails reach row {bottom}, the line has {} rows",
                    m.big_height
                );
            }
            let (_, number_bottom) = ink_rows(m.small, "12,000 gpy");
            assert!(number_bottom < m.small_height, "{size:?} {game}: the second line is too low for its tails");
        }
    }
}

#[test]
fn the_text_stays_inside_a_whole_frame_at_every_size_with_the_longest_line() {
    use versekit_core::signature_window::Size;
    let near = |p: u32, rgb: (u32, u32, u32)| {
        let (b, g, r) = (p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF);
        r.abs_diff(rgb.0) < 24 && g.abs_diff(rgb.1) < 24 && b.abs_diff(rgb.2) < 24
    };
    for size in Size::ALL {
        for game in GAME_HEIGHTS {
            let px = size.font_px(game);
            let (w, h, pixels) = on_hidden_desktop(move || {
                let window = ScanWindow::create().expect("window");
                let (w, h) = window.set_content(content("5× Corundum (Raw) · 6× Bergung gpy", "12,000", px));
                window.show_at((50, 50, w, h));
                window.snapshot().expect("snapshot")
            });
            let at = |x: i32, y: i32| pixels[(y * w + x) as usize];
            let (middle_y, middle_x) = (h / 2, w / 2);
            let line = (0x3a, 0x46, 0x58);
            let ground = (0x10, 0x14, 0x18);
            assert!(near(at(0, middle_y), line), "{size:?} {game}: the left edge is not whole");
            assert!(near(at(w - 1, middle_y), line), "{size:?} {game}: the right edge is not whole");
            assert!(near(at(middle_x, 0), line), "{size:?} {game}: the top edge is not whole");
            assert!(near(at(middle_x, h - 1), line), "{size:?} {game}: the bottom edge is not whole");
            // Between the frame and the letters the ground shows on both sides, over the height of the text.
            for y in (h / 5)..(h - h / 5) {
                assert!(near(at(FRAME, y), ground), "{size:?} {game}: the text touches the left frame at row {y}");
                assert!(near(at(w - 1 - FRAME, y), ground), "{size:?} {game}: the text touches the right frame at {y}");
            }
        }
    }
}

/// The snapshot of a window with the given look: width, height and pixels (BGRA).
fn snapshot_of(font_px: i32, square: bool) -> (i32, i32, Vec<u32>) {
    on_hidden_desktop(move || {
        let window = ScanWindow::create().expect("window");
        let mut shown = content("5× Corundum (Raw) · 6× Bergung", "12,000", font_px);
        shown.square = square;
        let (w, h) = window.set_content(shown);
        window.show_at((50, 50, w, h));
        window.snapshot().expect("snapshot")
    })
}

#[test]
fn the_edge_is_closed_at_the_corners_rounded_as_the_cards_with_the_green_brackets_top_left_and_bottom_right() {
    use versekit_core::signature_window::Size;
    let near = |p: u32, (r, g, b): (u32, u32, u32)| {
        let (pb, pg, pr) = (p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF);
        pr.abs_diff(r) < 30 && pg.abs_diff(g) < 30 && pb.abs_diff(b) < 30
    };
    let (line, accent) = ((0x3a, 0x46, 0x58), (0x9c, 0xe4, 0x30));
    let r = f32::from(versekit_ui::BOX_RADIUS);
    for size in Size::ALL {
        let (w, h, pixels) = snapshot_of(size.font_px(1080), false);
        // A pixel of the edge near the point, in the colour: the neighbours count (the edge is one pixel wide).
        let has = |x: f32, y: f32, colour: (u32, u32, u32)| {
            let (cx, cy) = (x.round() as i32, y.round() as i32);
            (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (px, py) = (cx + dx, cy + dy);
                    (0..w).contains(&px) && (0..h).contains(&py) && near(pixels[(py * w + px) as usize], colour)
                })
            })
        };
        // The four corners: centre of the arc, and the quarter of the circle it runs through.
        let (wf, hf) = (w as f32, h as f32);
        let corners = [
            ("top left", (r, r), 180.0f32, true),
            ("top right", (wf - 1.0 - r, r), 270.0, false),
            ("bottom right", (wf - 1.0 - r, hf - 1.0 - r), 0.0, true),
            ("bottom left", (r, hf - 1.0 - r), 90.0, false),
        ];
        for (name, (cx, cy), from, bracket) in corners {
            let (mut edge, mut green) = (0, 0);
            for step in 1..9 {
                let angle = (from + 90.0 * step as f32 / 9.0).to_radians();
                let (x, y) = (cx + (r - 0.5) * angle.cos(), cy + (r - 0.5) * angle.sin());
                if has(x, y, line) || has(x, y, accent) {
                    edge += 1;
                }
                if has(x, y, accent) {
                    green += 1;
                }
            }
            assert_eq!(edge, 8, "{size:?}: the {name} corner is not closed");
            if bracket {
                assert!(green >= 5, "{size:?}: no green bracket at the {name} corner ({green})");
            } else {
                assert_eq!(green, 0, "{size:?}: a green bracket where the cards have none ({name})");
            }
        }
    }
    // The scheme with square boxes: the edge reaches into the corners, the brackets are angular.
    let (w, h, pixels) = snapshot_of(24, true);
    let at = |x: i32, y: i32| pixels[(y * w + x) as usize];
    assert!(near(at(1, 1), accent) && near(at(w - 2, h - 2), accent), "angular green brackets at the corners");
    assert!(near(at(0, 0), line) || near(at(0, 0), accent), "the corner top left is closed");
    assert!(
        near(at(w - 1, 0), line) && near(at(0, h - 1), line),
        "the other two corners are closed in the line colour"
    );
}

#[test]
fn the_frame_that_shows_the_area_is_hollow_never_activated_and_lets_clicks_through() {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, GetWindowRgn, PtInRegion};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, WS_EX_TRANSPARENT,
    };
    on_hidden_desktop(|| {
        let frame = FrameWindow::create().expect("frame");
        let foreground = unsafe { GetForegroundWindow() };
        frame.show((120, 80, 300, 90), 0x0030_e49c);
        let hwnd = frame.hwnd() as HWND;
        // Where it was asked to stand.
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { GetWindowRect(hwnd, &mut rect) };
        assert_eq!((rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top), (120, 80, 300, 90));
        // Hollow: the edge is part of the window, the middle is not.
        unsafe {
            let region = CreateRectRgn(0, 0, 0, 0);
            assert!(GetWindowRgn(hwnd, region) != 0, "the window has a region");
            assert!(PtInRegion(region, 1, 45) != 0, "the left edge belongs to the frame");
            assert!(PtInRegion(region, 150, 1) != 0, "the top edge too");
            assert!(PtInRegion(region, 150, 45) == 0, "the middle is open");
            DeleteObject(region);
        }
        // Never activated, clicks pass, and the focus stays where it was.
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        for (flag, name) in
            [(WS_EX_NOACTIVATE, "never activated"), (WS_EX_TRANSPARENT, "clicks pass"), (WS_EX_LAYERED, "layered")]
        {
            assert!(style & flag != 0, "not {name}: {style:#x}");
        }
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        frame.hide();
    });
}
