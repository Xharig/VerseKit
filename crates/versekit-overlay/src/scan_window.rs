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

//! The small window with the scanned material above the signature (Windows).
//!
//! A window of its own, not part of the overlay: a pop-up that is never activated (a click does not take the
//! focus from the game), always on top, layered. Layered matters twice: `BitBlt` of the screen grab leaves
//! layered windows out, so the window never photographs itself, and the opacity comes from the layering.
//! Clicks pass through it (`WS_EX_TRANSPARENT`) unless the lock of the overlay is open: then it can be dragged.
//!
//! The window is drawn with GDI: the letters in two lines, the material in the accent and the number below.
//! All calls belong to the thread that created the window, the one that runs the frames of the overlay.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateFontW, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_NOCLIP,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_SEMIBOLD,
    FillRect, GetDC, GetStockObject, GetTextExtentPoint32W, GetTextMetricsW, HDC, HFONT, InvalidateRect, NULL_BRUSH,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_INSIDEFRAME, PS_SOLID, Polyline, Rectangle, ReleaseDC, RoundRect, SRCCOPY,
    SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TEXTMETRICW, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, GetWindowRect,
    HWND_TOPMOST, IDC_ARROW, LWA_ALPHA, LoadCursorW, MA_NOACTIVATE, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WM_CAPTURECHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT,
    WM_PRINTCLIENT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

const CLASS: &str = "VerseKitSignaturFenster";
/// 92 percent opaque.
const ALPHA: u8 = 235;

/// What the window shows and in which colours (`COLORREF`: red, green and blue in the low bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    /// The material behind the number; empty where there is none.
    pub material: String,
    /// The material is a name the data know; "no ore recognised" stands in the normal colour.
    pub known: bool,
    pub number: String,
    /// The height of the letters of the material in pixels.
    pub font_px: i32,
    pub background: u32,
    pub text: u32,
    pub accent: u32,
    pub sub: u32,
    /// The colour of the lines of the cards, for the edge.
    pub line: u32,
    /// The rounding of the corners of a card, in pixels, and whether the scheme has square boxes.
    pub radius: i32,
    pub square: bool,
}

/// `COLORREF` of a colour of the scheme.
pub fn colorref(c: versekit_core::theme::Color) -> u32 {
    u32::from(c.r) | (u32::from(c.g) << 8) | (u32::from(c.b) << 16)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn font(px: i32) -> HFONT {
    let face: Vec<u16> = "Segoe UI".encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: plain numbers and a zero-terminated name that lives during the call.
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            u32::from(CLIP_DEFAULT_PRECIS),
            u32::from(CLEARTYPE_QUALITY),
            u32::from(DEFAULT_PITCH) | u32::from(FF_DONTCARE),
            face.as_ptr(),
        )
    }
}

/// The thickness of the frame in pixels.
const FRAME: i32 = 2;

/// How the lines stand: the sizes of the letters, the height a line of them really takes (ascent and descent of
/// the face, so that the tails of g, p and y have room) and the space around them.
pub struct Metrics {
    pub big: i32,
    pub small: i32,
    pub big_height: i32,
    pub small_height: i32,
    pub pad: i32,
    /// The space between the two lines.
    pub gap: i32,
}

/// The height of a line in `face`, from the face itself.
///
/// # Safety
/// `dc` is a valid device context.
unsafe fn line_height(dc: HDC, face: HFONT) -> i32 {
    // SAFETY: the caller gives a valid device context; the font is selected for the call and given back.
    unsafe {
        let old = SelectObject(dc, face);
        let mut tm: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, old);
        tm.tmHeight
    }
}

pub fn metrics(font_px: i32) -> Metrics {
    let (big, small) = (font_px, font_px * 3 / 4);
    // SAFETY: a screen device context and fonts made and given back here.
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let (big_face, small_face) = (font(big), font(small));
        let (big_height, small_height) = (line_height(dc, big_face), line_height(dc, small_face));
        DeleteObject(big_face);
        DeleteObject(small_face);
        ReleaseDC(std::ptr::null_mut(), dc);
        Metrics {
            big,
            small,
            big_height: big_height.max(big),
            small_height: small_height.max(small),
            pad: (font_px / 2).max(6),
            gap: small / 4,
        }
    }
}

/// The width of `text` in `face` on `dc`.
///
/// # Safety
/// `dc` is a valid device context.
unsafe fn width_of(dc: HDC, face: HFONT, text: &str) -> i32 {
    // SAFETY: the caller gives a valid device context; the font is selected for the call and given back.
    unsafe {
        let old = SelectObject(dc, face);
        let letters = wide(text);
        let mut size = SIZE { cx: 0, cy: 0 };
        GetTextExtentPoint32W(dc, letters.as_ptr(), letters.len() as i32, &mut size);
        SelectObject(dc, old);
        size.cx
    }
}

/// The size the window needs for the content, measured with the real font: the width of the longer line, the
/// height of both lines as the face takes them, the padding and the frame all round, and some room to spare.
pub fn measure(content: &Content) -> (i32, i32) {
    let m = metrics(content.font_px);
    // SAFETY: a screen device context and fonts made and given back here.
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let (big, small) = (font(m.big), font(m.small));
        let wide_material = if content.material.is_empty() { 0 } else { width_of(dc, big, &content.material) };
        let wide_number = width_of(dc, if content.material.is_empty() { big } else { small }, &content.number);
        DeleteObject(big);
        DeleteObject(small);
        ReleaseDC(std::ptr::null_mut(), dc);
        let text_w = wide_material.max(wide_number);
        let text_h = if content.material.is_empty() { m.big_height } else { m.big_height + m.gap + m.small_height };
        (text_w + 2 * (m.pad + FRAME) + content.font_px / 4, text_h + 2 * m.pad + 2 * FRAME)
    }
}

struct Inner {
    content: RefCell<Option<Content>>,
    /// Top left corner after a drag, until it is taken.
    moved: Cell<Option<(i32, i32)>>,
    /// While a drag runs: the cursor and the window at its start.
    drag: Cell<Option<(POINT, (i32, i32))>>,
    draggable: Cell<bool>,
}

/// The window.
pub struct ScanWindow {
    hwnd: HWND,
    inner: Box<Inner>,
}

fn register() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let name: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the class is described fully; the name lives for the program (leaked).
        unsafe {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: Box::leak(name.into_boxed_slice()).as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            RegisterClassExW(&class) != 0
        }
    })
}

fn inner_of<'a>(hwnd: HWND) -> Option<&'a Inner> {
    // SAFETY: the pointer is set by `create` to the box that lives as long as the window and cleared before the
    // box is dropped.
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Inner;
    unsafe { pointer.as_ref() }
}

fn cursor() -> POINT {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` lives during the call.
    unsafe { GetCursorPos(&mut at) };
    at
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            paint(hwnd);
            return 0;
        }
        // A print of the window (the check photographs it so): drawn into the context it hands over.
        WM_PRINTCLIENT => {
            draw(hwnd, wparam as HDC);
            return 0;
        }
        WM_LBUTTONDOWN => {
            if let Some(inner) = inner_of(hwnd)
                && inner.draggable.get()
            {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                // SAFETY: `rect` lives during the call.
                unsafe {
                    GetWindowRect(hwnd, &mut rect);
                    SetCapture(hwnd);
                }
                inner.drag.set(Some((cursor(), (rect.left, rect.top))));
                return 0;
            }
        }
        WM_MOUSEMOVE => {
            if let Some(inner) = inner_of(hwnd)
                && let Some((start, (left, top))) = inner.drag.get()
            {
                let now = cursor();
                // SAFETY: moves our own window.
                unsafe {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        left + now.x - start.x,
                        top + now.y - start.y,
                        0,
                        0,
                        windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
                return 0;
            }
        }
        WM_LBUTTONUP => {
            if let Some(inner) = inner_of(hwnd)
                && inner.drag.take().is_some()
            {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                // SAFETY: `rect` lives during the call.
                unsafe {
                    ReleaseCapture();
                    GetWindowRect(hwnd, &mut rect);
                }
                inner.moved.set(Some((rect.left, rect.top)));
                return 0;
            }
        }
        // The mouse was taken away from a drag that is under way (Alt-Tab, the game grabs the mouse, a dialog):
        // no release will come, so the drag ends here, with the window where it stands. Our own release of the
        // mouse after a finished drag finds no drag any more.
        WM_CAPTURECHANGED => {
            if let Some(inner) = inner_of(hwnd)
                && inner.drag.take().is_some()
            {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                // SAFETY: `rect` lives during the call.
                unsafe { GetWindowRect(hwnd, &mut rect) };
                inner.moved.set(Some((rect.left, rect.top)));
                return 0;
            }
        }
        WM_NCDESTROY => {
            // SAFETY: the box is dropped by its owner; nothing reads the pointer after this.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        }
        _ => {}
    }
    // SAFETY: the default procedure for the messages not handled.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn paint(hwnd: HWND) {
    // SAFETY: the paint structure is given back at the end.
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        draw(hwnd, dc);
        EndPaint(hwnd, &ps);
    }
}

/// The edge of the cards of the main window, in GDI: a closed line round the box in the colour of the lines, with
/// the corners rounded as the cards round them (square in the scheme with square boxes), and the two brackets in
/// the accent at the top left and the bottom right, from the same geometry the cards use.
///
/// # Safety
/// `memory` is a valid device context of `w` by `h` pixels.
unsafe fn paint_card_edge(memory: HDC, w: i32, h: i32, content: &Content) {
    use versekit_ui::widgets::{corner_marks, rounded_corner_marks};
    // SAFETY: the pens and the stock brush are selected for the calls and given back; the points live on the stack.
    unsafe {
        let outline = CreatePen(PS_INSIDEFRAME, 1, content.line);
        let (old_pen, old_brush) = (SelectObject(memory, outline), SelectObject(memory, GetStockObject(NULL_BRUSH)));
        if content.square || content.radius == 0 {
            Rectangle(memory, 0, 0, w, h);
        } else {
            RoundRect(memory, 0, 0, w, h, 2 * content.radius, 2 * content.radius);
        }
        SelectObject(memory, old_brush);
        SelectObject(memory, old_pen);
        DeleteObject(outline);

        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32));
        let marks: Vec<Vec<egui::Pos2>> = if content.square {
            corner_marks(rect).iter().map(|mark| mark.to_vec()).collect()
        } else {
            rounded_corner_marks(rect, content.radius as f32).into_iter().collect()
        };
        let bracket = CreatePen(PS_SOLID, FRAME, content.accent);
        let old_pen = SelectObject(memory, bracket);
        for mark in marks {
            let points: Vec<POINT> =
                mark.iter().map(|p| POINT { x: p.x.round() as i32, y: p.y.round() as i32 }).collect();
            Polyline(memory, points.as_ptr(), points.len() as i32);
        }
        SelectObject(memory, old_pen);
        DeleteObject(bracket);
    }
}

/// The region of the window: the rounded box of the cards, so the corners do not show the ground behind them.
fn card_region(w: i32, h: i32, content: Option<&Content>) -> windows_sys::Win32::Graphics::Gdi::HRGN {
    let radius = content.map_or(0, |c| if c.square { 0 } else { c.radius });
    // SAFETY: a region made from plain numbers; the system takes it over with `SetWindowRgn`.
    unsafe {
        if radius == 0 {
            windows_sys::Win32::Graphics::Gdi::CreateRectRgn(0, 0, w + 1, h + 1)
        } else {
            CreateRoundRectRgn(0, 0, w + 1, h + 1, 2 * radius, 2 * radius)
        }
    }
}

/// Draws the window into `dc` (the paint of the system, or the one a print asks for).
fn draw(hwnd: HWND, dc: HDC) {
    let Some(inner) = inner_of(hwnd) else { return };
    let content = inner.content.borrow().clone();
    // SAFETY: every handle made below is given back before the end.
    unsafe {
        let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut area);
        let (w, h) = (area.right - area.left, area.bottom - area.top);
        if let Some(content) = content
            && w > 0
            && h > 0
        {
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, w, h);
            let old = SelectObject(memory, bitmap);
            let m = metrics(content.font_px);
            let background = CreateSolidBrush(content.background);
            FillRect(memory, &area, background);
            DeleteObject(background);
            paint_card_edge(memory, w, h, &content);
            SetBkMode(memory, TRANSPARENT as i32);
            let (big, small) = (font(m.big), font(m.small));
            let flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP;
            if content.material.is_empty() {
                let previous = SelectObject(memory, big);
                SetTextColor(memory, content.text);
                let mut at = RECT { left: 0, top: 0, right: w, bottom: h };
                let letters = wide(&content.number);
                DrawTextW(memory, letters.as_ptr(), letters.len() as i32, &mut at, flags);
                SelectObject(memory, previous);
            } else {
                let previous = SelectObject(memory, big);
                let first = m.pad + FRAME;
                SetTextColor(memory, if content.known { content.accent } else { content.text });
                let mut top = RECT { left: 0, top: first, right: w, bottom: first + m.big_height };
                let letters = wide(&content.material);
                DrawTextW(memory, letters.as_ptr(), letters.len() as i32, &mut top, flags);
                SelectObject(memory, small);
                SetTextColor(memory, content.sub);
                let from = first + m.big_height + m.gap;
                let mut bottom = RECT { left: 0, top: from, right: w, bottom: from + m.small_height };
                let letters = wide(&content.number);
                DrawTextW(memory, letters.as_ptr(), letters.len() as i32, &mut bottom, flags);
                SelectObject(memory, previous);
            }
            DeleteObject(big);
            DeleteObject(small);
            BitBlt(dc, 0, 0, w, h, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
        }
    }
}

impl ScanWindow {
    /// Makes the window, hidden. `None` where the system refuses.
    pub fn create() -> Option<ScanWindow> {
        if !register() {
            return None;
        }
        let inner = Box::new(Inner {
            content: RefCell::new(None),
            moved: Cell::new(None),
            drag: Cell::new(None),
            draggable: Cell::new(false),
        });
        let class: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "Verse-Kit".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the class is registered; the data pointer is set below, before any message of ours is read.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return None;
        }
        // SAFETY: the box lives as long as the window object; `Drop` clears the pointer first.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*inner as *const Inner as isize);
            SetLayeredWindowAttributes(hwnd, 0, ALPHA, LWA_ALPHA);
        }
        Some(ScanWindow { hwnd, inner })
    }

    #[cfg(test)]
    pub fn hwnd(&self) -> isize {
        self.hwnd as isize
    }

    /// Takes the content and gives the size the window needs for it.
    pub fn set_content(&self, content: Content) -> (i32, i32) {
        let size = measure(&content);
        if self.inner.content.borrow().as_ref() != Some(&content) {
            *self.inner.content.borrow_mut() = Some(content);
            // SAFETY: asks for a repaint of our own window.
            unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0) };
        }
        size
    }

    /// Puts the window at `place` (left, top, width, height in screen points) and shows it without activating.
    pub fn show_at(&self, place: (i32, i32, i32, i32)) {
        let (x, y, w, h) = place;
        let mut now = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: our own window; `now` lives during the call.
        unsafe {
            GetWindowRect(self.hwnd, &mut now);
            let resized = now.right - now.left != w || now.bottom - now.top != h;
            SetWindowPos(self.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            if resized {
                SetWindowRgn(self.hwnd, card_region(w, h, self.inner.content.borrow().as_ref()), 1);
            }
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    pub fn hide(&self) {
        // SAFETY: our own window.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    #[cfg(test)]
    pub fn visible(&self) -> bool {
        // SAFETY: our own window.
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(self.hwnd) != 0 }
    }

    /// Open lock: the window takes the mouse and can be dragged. Closed: clicks pass through it.
    pub fn set_draggable(&self, on: bool) {
        self.inner.draggable.set(on);
        // SAFETY: our own window; only the one style bit is changed.
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE) as u32;
            let wanted = if on { style & !WS_EX_TRANSPARENT } else { style | WS_EX_TRANSPARENT };
            if wanted != style {
                SetWindowLongPtrW(self.hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE, wanted as isize);
            }
        }
    }

    /// Is the window being dragged just now? Its place is then not set from the pill.
    pub fn dragging(&self) -> bool {
        self.inner.drag.get().is_some()
    }

    /// The top left corner after a drag, once.
    pub fn take_moved(&self) -> Option<(i32, i32)> {
        self.inner.moved.take()
    }

    /// What the window draws, as the system would paint it: width, height and the pixels (BGRA as `u32`).
    #[cfg(test)]
    pub fn snapshot(&self) -> Option<(i32, i32, Vec<u32>)> {
        use windows_sys::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HGDIOBJ,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: our own window; every GDI object made here is given back before the end.
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(self.hwnd, &mut area);
            let (w, h) = (area.right, area.bottom);
            if w <= 0 || h <= 0 {
                return None;
            }
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
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
                SendMessageW(self.hwnd, WM_PRINTCLIENT, dc as WPARAM, 0);
                result = Some((w, h, std::slice::from_raw_parts(bits as *const u32, (w * h) as usize).to_vec()));
                SelectObject(dc, old);
                DeleteObject(bitmap as HGDIOBJ);
            }
            DeleteDC(dc);
            ReleaseDC(std::ptr::null_mut(), screen);
            result
        }
    }

    /// The extended style, for the checks.
    #[cfg(test)]
    pub fn ex_style(&self) -> u32 {
        // SAFETY: our own window.
        unsafe { GetWindowLongPtrW(self.hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE) as u32 }
    }
}

impl Drop for ScanWindow {
    fn drop(&mut self) {
        // SAFETY: the pointer to the box is cleared before the window and then the box go.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
        }
    }
}

// ------------------------------------------------------------ the frame that shows the area read

const FRAME_CLASS: &str = "VerseKitSignaturRahmen";
/// How thick the frame is drawn, in points.
const FRAME_THICKNESS: i32 = 3;
/// The colour of the frame (`COLORREF`), set with every showing.
static FRAME_COLOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x0030_e49c);

unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => {
            let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            // SAFETY: `area` lives during the call; the brush is made and given back here.
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut area);
                let brush = CreateSolidBrush(FRAME_COLOR.load(std::sync::atomic::Ordering::Relaxed));
                FillRect(wparam as HDC, &area, brush);
                DeleteObject(brush);
            }
            return 1;
        }
        _ => {}
    }
    // SAFETY: the default procedure for the messages not handled.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn register_frame() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let name: Vec<u16> = FRAME_CLASS.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the class is described fully; the name lives for the program (leaked).
        unsafe {
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(frame_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: Box::leak(name.into_boxed_slice()).as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            RegisterClassExW(&class) != 0
        }
    })
}

/// A hollow frame over the game that shows the area the scanner reads: a window of its own, never activated,
/// click-through, layered (so it is not in the screen grab either).
pub struct FrameWindow {
    hwnd: HWND,
}

impl FrameWindow {
    pub fn create() -> Option<FrameWindow> {
        if !register_frame() {
            return None;
        }
        let class: Vec<u16> = FRAME_CLASS.encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "Verse-Kit".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the class is registered; the window is destroyed in `Drop`.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return None;
        }
        // SAFETY: our own window.
        unsafe { SetLayeredWindowAttributes(hwnd, 0, 210, LWA_ALPHA) };
        Some(FrameWindow { hwnd })
    }

    /// Shows the frame around `area` (left, top, width, height in screen points) in `color`.
    pub fn show(&self, area: (i32, i32, i32, i32), color: u32) {
        use windows_sys::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, RGN_DIFF};
        let (x, y, w, h) = area;
        FRAME_COLOR.store(color, std::sync::atomic::Ordering::Relaxed);
        let t = FRAME_THICKNESS.min(w / 2).min(h / 2);
        // SAFETY: our own window; the regions are made here, the outer one goes to the window, the inner one is
        // given back.
        unsafe {
            SetWindowPos(self.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            let outer = CreateRectRgn(0, 0, w, h);
            let inner = CreateRectRgn(t, t, w - t, h - t);
            CombineRgn(outer, outer, inner, RGN_DIFF);
            DeleteObject(inner);
            SetWindowRgn(self.hwnd, outer, 1);
            InvalidateRect(self.hwnd, std::ptr::null(), 1);
        }
    }

    pub fn hide(&self) {
        // SAFETY: our own window.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    #[cfg(test)]
    pub fn hwnd(&self) -> isize {
        self.hwnd as isize
    }
}

impl Drop for FrameWindow {
    fn drop(&mut self) {
        // SAFETY: our own window.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

#[cfg(test)]
mod tests;
