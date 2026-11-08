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

//! The look of the menu at the icon next to the clock: the colors of the house instead of those of Windows.
//!
//! The menu stays the one of Windows (`TrackPopupMenu`): it opens, closes on a click beside it, answers the keys
//! and hands the foreground back exactly as before. Only its entries are drawn here (owner-drawn items,
//! `WM_MEASUREITEM` and `WM_DRAWITEM` of the window of the icon): the surface of the house scheme, its text, and the
//! entry under the pointer in the brand color [`BRAND`] with dark text.

use versekit_core::theme::{self, Color};
use windows_sys::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, CreateSolidBrush, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW,
    FillRect, GetDC, GetTextExtentPoint32W, HDC, HFONT, HGDIOBJ, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MF_OWNERDRAW, MF_SEPARATOR, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, SystemParametersInfoW,
};

/// The brand color of VerseKit, the one color of the house that marks the entry under the pointer.
pub const BRAND: &str = "#9ce430";

/// The scheme whose colors the menu wears: the one of the house, whatever scheme the window has.
const HOUSE_SCHEME: &str = "original";

/// The colors of the menu as Windows takes them (`0x00BBGGRR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// The ground of the menu and of an entry.
    pub back: COLORREF,
    pub text: COLORREF,
    /// The line of a separator.
    pub line: COLORREF,
    /// The ground of the entry under the pointer: the brand color.
    pub hover_back: COLORREF,
    /// Its text: the darkest color of the house, for contrast on the brand color.
    pub hover_text: COLORREF,
}

/// A color as Windows takes it.
pub fn colorref(color: Color) -> COLORREF {
    u32::from(color.r) | (u32::from(color.g) << 8) | (u32::from(color.b) << 16)
}

impl Look {
    /// The colors of the house scheme, the brand color for the entry under the pointer.
    pub fn house() -> Look {
        let scheme = theme::by_id(HOUSE_SCHEME).unwrap_or_else(theme::current);
        let brand = Color::parse(BRAND).unwrap_or(scheme.accent);
        Look {
            back: colorref(scheme.surface),
            text: colorref(scheme.fg),
            line: colorref(scheme.line),
            hover_back: colorref(brand),
            hover_text: colorref(scheme.bg),
        }
    }
}

/// How an entry of the menu is appended: drawn here; a separator stays one (it cannot be chosen).
pub fn item_flags(separator: bool) -> u32 {
    if separator { MF_OWNERDRAW | MF_SEPARATOR } else { MF_OWNERDRAW }
}

/// The font of the menus of the system, given back with the value.
pub struct MenuFont(pub HFONT);

impl MenuFont {
    /// The font Windows uses for menus, at the size the user set; `None` when it cannot be asked.
    pub fn of_system() -> Option<MenuFont> {
        // SAFETY: the structure carries its size; Windows fills it.
        unsafe {
            let mut metrics: NONCLIENTMETRICSW = std::mem::zeroed();
            metrics.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
            let asked = SystemParametersInfoW(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&raw mut metrics).cast(),
                0,
            );
            if asked == 0 {
                return None;
            }
            let font = CreateFontIndirectW(&metrics.lfMenuFont);
            (!font.is_null()).then_some(MenuFont(font))
        }
    }
}

impl Drop for MenuFont {
    fn drop(&mut self) {
        // SAFETY: the font was made by CreateFontIndirectW and is given back once.
        unsafe { DeleteObject(self.0 as HGDIOBJ) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// The width and height of the text `text` in `font` on `dc`.
fn text_size(dc: HDC, font: HFONT, text: &str) -> (i32, i32) {
    let chars = wide(if text.is_empty() { " " } else { text });
    let mut size = SIZE { cx: 0, cy: 0 };
    // SAFETY: the font is selected for the measure and the old one put back; the text lives for the call.
    unsafe {
        let old = SelectObject(dc, font as HGDIOBJ);
        GetTextExtentPoint32W(dc, chars.as_ptr(), chars.len() as i32, &mut size);
        SelectObject(dc, old);
    }
    (size.cx, size.cy)
}

/// The room around the text of an entry, from the height of a line: left and right, and above and below.
fn padding(line: i32) -> (i32, i32) {
    (line.max(8), (line / 3).max(3))
}

/// The size of an entry: `text` with room around it, a separator (`None`) a thin row.
pub fn measure(dc: HDC, font: HFONT, text: Option<&str>) -> (u32, u32) {
    let (width, height) = text_size(dc, font, text.unwrap_or(""));
    let (side, above) = padding(height);
    match text {
        Some(_) => ((width + 2 * side).max(0) as u32, (height + 2 * above).max(0) as u32),
        None => (1, (above * 2 + 1).max(5) as u32),
    }
}

/// The size of an entry measured on the screen of the window `hwnd`.
pub fn measure_for(hwnd: HWND, font: HFONT, text: Option<&str>) -> (u32, u32) {
    // SAFETY: the device context of the window is asked for and given back.
    unsafe {
        let dc = GetDC(hwnd);
        if dc.is_null() {
            return (120, 24);
        }
        let size = measure(dc, font, text);
        ReleaseDC(hwnd, dc);
        size
    }
}

/// Fills `rect` on `dc` in `color`.
fn fill(dc: HDC, rect: &RECT, color: COLORREF) {
    // SAFETY: a brush made for the fill and given back.
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, rect, brush);
        DeleteObject(brush as HGDIOBJ);
    }
}

/// Draws an entry into `rect`: the ground, then the text (or the line of a separator), the entry under the pointer
/// (`selected`) in the brand color.
pub fn draw(dc: HDC, rect: RECT, font: HFONT, text: Option<&str>, selected: bool, look: &Look) {
    let Some(text) = text else {
        fill(dc, &rect, look.back);
        let (side, _) = padding((rect.bottom - rect.top).max(8));
        let middle = (rect.top + rect.bottom) / 2;
        let line = RECT { left: rect.left + side / 2, top: middle, right: rect.right - side / 2, bottom: middle + 1 };
        fill(dc, &line, look.line);
        return;
    };
    let (back, color) = if selected { (look.hover_back, look.hover_text) } else { (look.back, look.text) };
    fill(dc, &rect, back);
    let (_, height) = text_size(dc, font, text);
    let (side, _) = padding(height);
    let mut room = RECT { left: rect.left + side, ..rect };
    let chars = wide(text);
    // SAFETY: the font is selected for the text and the old one put back; the text lives for the call.
    unsafe {
        let old = SelectObject(dc, font as HGDIOBJ);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        DrawTextW(dc, chars.as_ptr(), chars.len() as i32, &mut room, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, old);
    }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Graphics::Gdi::{CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, GetPixel};

    use super::*;

    #[test]
    fn the_menu_wears_the_house_colors_and_the_brand_color_under_the_pointer() {
        let look = Look::house();
        assert_eq!(look.hover_back, 0x0030_e49c, "#9ce430 as Windows takes it");
        let scheme = theme::by_id(HOUSE_SCHEME).unwrap();
        assert_eq!((look.back, look.text), (colorref(scheme.surface), colorref(scheme.fg)));
        assert_ne!(look.back, 0x00ff_ffff, "not the white of Windows");
        assert_eq!(look.hover_text, colorref(scheme.bg), "dark text on the brand color");
        assert_eq!(item_flags(false), MF_OWNERDRAW);
        assert_eq!(item_flags(true), MF_OWNERDRAW | MF_SEPARATOR, "a separator stays one");
    }

    /// The real drawing of Windows into a picture in memory: no window, nothing on a screen.
    #[test]
    fn an_entry_is_drawn_in_the_house_colors_and_measured_with_room_around_its_text() {
        let font = MenuFont::of_system().expect("the font of the menus");
        let look = Look::house();
        // SAFETY: a device context and a picture in memory, given back at the end.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let screen = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
            let picture = CreateCompatibleBitmap(screen, 300, 40);
            ReleaseDC(std::ptr::null_mut(), screen);
            let old = SelectObject(dc, picture as HGDIOBJ);
            let rect = RECT { left: 0, top: 0, right: 300, bottom: 40 };
            draw(dc, rect, font.0, Some("VerseKit zeigen"), true, &look);
            assert_eq!(GetPixel(dc, 1, 1), look.hover_back, "the entry under the pointer");
            draw(dc, rect, font.0, Some("VerseKit zeigen"), false, &look);
            assert_eq!(GetPixel(dc, 1, 1), look.back, "an entry at rest");
            let text_drawn = (0..300).any(|x| (0..40).any(|y| GetPixel(dc, x, y) != look.back));
            assert!(text_drawn, "the text stands on the ground");
            draw(dc, rect, font.0, None, false, &look);
            assert_eq!(GetPixel(dc, 150, 20), look.line, "the line of a separator");
            assert_eq!(GetPixel(dc, 150, 2), look.back);
            let (text_width, text_height) = text_size(dc, font.0, "VerseKit zeigen");
            let (width, height) = measure(dc, font.0, Some("VerseKit zeigen"));
            assert!(width as i32 > text_width && height as i32 > text_height, "{width}x{height}");
            let (_, line) = measure(dc, font.0, None);
            assert!(line < height, "a separator is a thin row");
            SelectObject(dc, old);
            DeleteObject(picture as HGDIOBJ);
            DeleteDC(dc);
        }
    }
}
