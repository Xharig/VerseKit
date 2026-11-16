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

//! The X11 part of the overlay: the window by its id (XID) on a connection of its own, its shape (only
//! there it is drawn and takes clicks), staying on top, not taking the keyboard, its place on the screen.
//! The window belongs to winit; this connection only reads and sets what winit leaves alone.
//!
//! ⚠ winit sets the input shape of the window (the part that takes clicks) to the whole window or to
//! nothing, at the click-through switch and at every resize. The X server lets a window take clicks only
//! inside its bounding shape too, so the bounding shape cut here also ends the clicks at the visible part.
//!
//! The arithmetic of the shape and the choice of the window type are built everywhere, so that their tests
//! run on every system; the connection only where X11 is.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use versekit_core::overlay::Rect;

/// The environment variable that gives the window another X11 window type (see [`window_types`]).
pub const WINDOW_TYPE_VAR: &str = "VERSEKIT_X11_FENSTERTYP";

/// The rectangles of `r` (window pixels) with its corners rounded by `radius` pixels, as X11 takes a shape:
/// a pixel belongs to it when its middle lies inside the rounding. Rows of the same width are one rectangle.
/// The radius is cut to half the shorter side (a pill); at 0 it is `r` itself.
pub fn rounded(r: Rect, radius: i32) -> Vec<Rect> {
    if r.w <= 0 || r.h <= 0 {
        return Vec::new();
    }
    let radius = radius.clamp(0, r.w.min(r.h) / 2);
    if radius == 0 {
        return vec![r];
    }
    let round = f64::from(radius);
    // How far row `i` (0 = the outermost) starts in from the edge.
    let inset = |i: i32| -> i32 {
        let dy = round - f64::from(i) - 0.5;
        (round - 0.5 - (round * round - dy * dy).sqrt()).ceil().max(0.0) as i32
    };
    // (first row, number of rows, inset) of the rounded band at the top.
    let mut bands = Vec::new();
    let mut i = 0;
    while i < radius {
        let x = inset(i);
        let mut n = 1;
        while i + n < radius && inset(i + n) == x {
            n += 1;
        }
        bands.push((i, n, x));
        i += n;
    }
    let mut out: Vec<Rect> = bands.iter().map(|&(i, n, x)| Rect::new(r.x + x, r.y + i, r.w - 2 * x, n)).collect();
    let middle = r.h - 2 * radius;
    if middle > 0 {
        out.push(Rect::new(r.x, r.y + radius, r.w, middle));
    }
    out.extend(bands.iter().rev().map(|&(i, n, x)| Rect::new(r.x + x, r.bottom() - i - n, r.w - 2 * x, n)));
    out
}

/// The input shape (window pixels) of a window that passes clicks while the lock is closed (`locked`): the lock in
/// the bar, so that it stays the way back. `None` elsewhere (clicks do not pass, the game hides the pointer, no
/// lock drawn): then the input shape is the one winit sets, the whole window or nothing.
///
/// ⚠ Under XWayland the X server sees the pointer only above a part of an X11 window that takes clicks: with an
/// empty input shape the overlay never learns that the pointer is on the lock, and the lock takes no click.
pub fn passing_input(passing: bool, locked: bool, lock: Option<Rect>) -> Option<Rect> {
    lock.filter(|lock| passing && locked && lock.w > 0 && lock.h > 0)
}

/// True under KDE: `desktop` (the value of `XDG_CURRENT_DESKTOP`, a list split by `:`) names it.
fn is_kde(desktop: Option<&str>) -> bool {
    desktop.is_some_and(|d| d.split(':').any(|part| part.trim().eq_ignore_ascii_case("kde")))
}

/// The window type asked for: the value of [`WINDOW_TYPE_VAR`] (`var`) where it is set, else under KDE (`desktop`,
/// the value of `XDG_CURRENT_DESKTOP`, a list split by `:`) the on-screen display, which KWin keeps above a game in
/// full screen. `normal` keeps the type winit gives.
pub fn window_type_choice(var: Option<&str>, desktop: Option<&str>) -> Option<String> {
    match var.map(str::trim).filter(|v| !v.is_empty()) {
        Some(asked) => Some(asked.to_string()),
        None => is_kde(desktop).then(|| "osd".into()),
    }
}

/// True where the window manager places the window by itself when it appears or changes its size: the on-screen
/// display of [`window_type_choice`] (`var`, `desktop` as there), which KWin sets to the lower middle.
pub fn manager_places(var: Option<&str>, desktop: Option<&str>) -> bool {
    window_type_choice(var, desktop).is_some_and(|t| t.trim().eq_ignore_ascii_case("osd"))
}

/// True where the window takes the keyboard by asking the window manager to activate it (`_NET_ACTIVE_WINDOW`)
/// instead of by `SetInputFocus`: under KDE (`desktop` as for [`window_type_choice`]), whose window manager takes
/// a focus change by another client back, but obeys the request of a tool.
pub fn typing_activation_wanted(desktop: Option<&str>) -> bool {
    is_kde(desktop)
}

/// The data of the `_NET_ACTIVE_WINDOW` message: the source 2 (a tool, which the window manager obeys without the
/// check against focus stealing), no time, `current` (the window that is active now, 0 for none), two unused.
pub fn active_request_data(current: u32) -> [u32; 5] {
    [2, 0, current, 0, 0]
}

/// The window to give the keyboard back to once the typing is over: `active` (the active window as the window
/// manager lists it) while it is a window other than `win` (the info bar), else what was remembered before
/// (`stored`). 0 and 1 are "none" and "pointer root", no window.
pub fn remember_before(stored: u32, active: u32, win: u32) -> u32 {
    if active > 1 && active != win { active } else { stored }
}

/// The window to activate when the typing is over: `before` (see [`remember_before`]), only while the info bar
/// (`win`) is still the active window (`active`), so that a window the user went to meanwhile keeps the keyboard.
pub fn give_back_to(before: u32, active: u32, win: u32) -> Option<u32> {
    (before > 1 && before != win && active == win).then_some(before)
}

/// The X11 window types (`_NET_WM_WINDOW_TYPE`) for the value of [`WINDOW_TYPE_VAR`], the one asked for
/// first and the normal one after it for window managers that do not know it. `None` (unset, empty,
/// `normal`, unknown) leaves the type winit gives.
pub fn window_types(choice: Option<&str>) -> Option<[&'static str; 2]> {
    let first = match choice?.trim().to_ascii_lowercase().as_str() {
        "utility" => "_NET_WM_WINDOW_TYPE_UTILITY",
        "dock" => "_NET_WM_WINDOW_TYPE_DOCK",
        "notification" => "_NET_WM_WINDOW_TYPE_NOTIFICATION",
        "osd" => "_KDE_NET_WM_WINDOW_TYPE_ON_SCREEN_DISPLAY",
        _ => return None,
    };
    Some([first, "_NET_WM_WINDOW_TYPE_NORMAL"])
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use system::*;

#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use std::sync::{Mutex, PoisonError};

    use x11rb::connection::Connection;
    use x11rb::properties::WmHints;
    use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ClipOrdering, ConfigureWindowAux, ConnectionExt as _, EventMask, PropMode,
        Rectangle, StackMode,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use super::{
        Rect, WINDOW_TYPE_VAR, active_request_data, give_back_to, remember_before, rounded, typing_activation_wanted,
        window_type_choice, window_types,
    };

    struct Link {
        conn: RustConnection,
        root: u32,
    }

    /// The connection, made at the first use; `None` inside once it could not be made.
    static LINK: Mutex<Option<Option<Link>>> = Mutex::new(None);

    /// Runs `ask` on the connection; `None` without one.
    fn with_link<T>(ask: impl FnOnce(&Link) -> Option<T>) -> Option<T> {
        let mut link = LINK.lock().unwrap_or_else(PoisonError::into_inner);
        let link = link.get_or_insert_with(|| {
            let (conn, number) = RustConnection::connect(None).ok()?;
            let root = conn.setup().roots.get(number)?.root;
            Some(Link { conn, root })
        });
        link.as_ref().and_then(ask)
    }

    fn atom(link: &Link, name: &str) -> Option<u32> {
        let atom = link.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom;
        (atom != x11rb::NONE).then_some(atom)
    }

    fn window(xid: isize) -> Option<u32> {
        u32::try_from(xid).ok().filter(|&w| w != 0)
    }

    /// [`manager_places`] for this run, from [`WINDOW_TYPE_VAR`] and `XDG_CURRENT_DESKTOP`.
    pub fn window_manager_places() -> bool {
        let asked = std::env::var(WINDOW_TYPE_VAR).ok();
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        manager_places(asked.as_deref(), desktop.as_deref())
    }

    /// Readies the window before it is shown first: it never takes the keyboard (`WM_HINTS.input`), and it
    /// gets the window type of [`window_type_choice`] when there is one. True when the window manager was told.
    pub fn install(xid: isize) -> bool {
        let Some(win) = window(xid) else { return false };
        with_link(|link| {
            let mut hints = WmHints::get(&link.conn, win).ok()?.reply().ok()?.unwrap_or_default();
            hints.input = Some(false);
            hints.set(&link.conn, win).ok()?.check().ok()?;
            let asked = std::env::var(WINDOW_TYPE_VAR).ok();
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
            if let Some(types) = window_types(window_type_choice(asked.as_deref(), desktop.as_deref()).as_deref()) {
                let atoms: Vec<u32> = types.iter().filter_map(|name| atom(link, name)).collect();
                let property = atom(link, "_NET_WM_WINDOW_TYPE")?;
                link.conn.change_property32(PropMode::REPLACE, win, property, AtomEnum::ATOM, &atoms).ok()?;
            }
            link.conn.flush().ok()?;
            Some(())
        })
        .is_some()
    }

    /// The window that had the keyboard before the info bar took it for typing.
    static BEFORE_TYPING: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// The window the window manager lists as active (`_NET_ACTIVE_WINDOW` of the root); 0 where there is none.
    /// `None` where the window manager does not have the property.
    fn active_window(link: &Link) -> Option<u32> {
        let property = atom(link, "_NET_ACTIVE_WINDOW")?;
        let reply = link.conn.get_property(false, link.root, property, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok()?;
        Some(reply.value32().and_then(|mut windows| windows.next()).unwrap_or(0))
    }

    /// Asks the window manager to activate `win` (`_NET_ACTIVE_WINDOW`, [`active_request_data`]), `current` being
    /// the window active now. Sent on the connection that set `WM_HINTS`, so that the hint is read first.
    fn request_active(link: &Link, win: u32, current: u32) -> Option<()> {
        let property = atom(link, "_NET_ACTIVE_WINDOW")?;
        let event = ClientMessageEvent::new(32, win, property, active_request_data(current));
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        link.conn.send_event(false, link.root, mask, event).ok()?;
        Some(())
    }

    /// The keyboard by the window manager: the window active now is remembered and `win` activated; off, the
    /// remembered window is activated while `win` is still the active one.
    fn typing_by_activation(link: &Link, win: u32, on: bool) -> Option<()> {
        use std::sync::atomic::Ordering;
        let active = active_window(link)?;
        if on {
            let stored = BEFORE_TYPING.load(Ordering::Relaxed);
            BEFORE_TYPING.store(remember_before(stored, active, win), Ordering::Relaxed);
            request_active(link, win, active)
        } else {
            let before = BEFORE_TYPING.swap(0, Ordering::Relaxed);
            match give_back_to(before, active, win) {
                Some(target) => request_active(link, target, win),
                None => Some(()),
            }
        }
    }

    /// The keyboard by `SetInputFocus`: the window that has it now is remembered and `win` takes it; off, the
    /// remembered window gets it back while `win` still has it.
    fn typing_by_focus(link: &Link, win: u32, on: bool) -> Option<()> {
        use std::sync::atomic::Ordering;
        use x11rb::protocol::xproto::InputFocus;
        let focus = link.conn.get_input_focus().ok()?.reply().ok()?.focus;
        if on {
            if focus != win {
                BEFORE_TYPING.store(focus, Ordering::Relaxed);
            }
            link.conn.set_input_focus(InputFocus::PARENT, win, x11rb::CURRENT_TIME).ok()?;
        } else {
            let before = BEFORE_TYPING.swap(0, Ordering::Relaxed);
            // 0 and 1 are "none" and "pointer root", no window to give the keyboard back to.
            if focus == win && before > 1 {
                link.conn.set_input_focus(InputFocus::PARENT, before, x11rb::CURRENT_TIME).ok()?;
            }
        }
        Some(())
    }

    /// Lets the window take the keyboard while `on` (`WM_HINTS.input`) and gives it the focus; off, it never takes
    /// it again and the window that had the keyboard before gets it back. Where [`typing_activation_wanted`] the
    /// window manager is asked to activate the window (and the one before), elsewhere, and where it does not know
    /// `_NET_ACTIVE_WINDOW`, the focus is set.
    pub fn allow_typing(xid: isize, on: bool) -> bool {
        let Some(win) = window(xid) else { return false };
        let by_activation = typing_activation_wanted(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());
        with_link(|link| {
            let mut hints = WmHints::get(&link.conn, win).ok()?.reply().ok()?.unwrap_or_default();
            hints.input = Some(on);
            hints.set(&link.conn, win).ok()?;
            if !(by_activation && typing_by_activation(link, win, on).is_some()) {
                typing_by_focus(link, win, on)?;
            }
            link.conn.flush().ok()
        })
        .is_some()
    }

    /// Cuts the window down to `r` (window pixels) with its corners rounded by `radius` pixels: only there
    /// it is drawn and takes clicks. `None` gives it back its whole rectangle.
    pub fn set_region(xid: isize, r: Option<Rect>, radius: i32) -> bool {
        let Some(win) = window(xid) else { return false };
        with_link(|link| {
            let cookie = match r {
                Some(r) => {
                    let rects: Vec<Rectangle> = rounded(r, radius)
                        .into_iter()
                        .filter_map(|p| {
                            Some(Rectangle {
                                x: i16::try_from(p.x).ok()?,
                                y: i16::try_from(p.y).ok()?,
                                width: u16::try_from(p.w).ok()?,
                                height: u16::try_from(p.h).ok()?,
                            })
                        })
                        .collect();
                    link.conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, win, 0, 0, &rects)
                }
                None => link.conn.shape_mask(SO::SET, SK::BOUNDING, win, 0, 0, x11rb::NONE),
            };
            cookie.ok()?.check().ok()
        })
        .is_some()
    }

    /// Lets only `r` (window pixels) of the window take clicks ([`super::passing_input`]), over the empty input
    /// shape winit sets while clicks pass. Sent without waiting for the answer: it is set again every frame.
    pub fn set_input(xid: isize, r: Rect) -> bool {
        let Some(win) = window(xid) else { return false };
        let (Ok(x), Ok(y), Ok(width), Ok(height)) =
            (i16::try_from(r.x), i16::try_from(r.y), u16::try_from(r.w), u16::try_from(r.h))
        else {
            return false;
        };
        let rect = [Rectangle { x, y, width, height }];
        with_link(|link| {
            link.conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &rect).ok()?;
            link.conn.flush().ok()
        })
        .is_some()
    }

    /// Back to the front among the windows kept on top: the state „above“ is asked for again where the
    /// window manager dropped it, and the window is raised, without the keyboard.
    pub fn reassert_topmost(xid: isize) {
        let Some(win) = window(xid) else { return };
        with_link(|link| {
            let (state, above) = (atom(link, "_NET_WM_STATE")?, atom(link, "_NET_WM_STATE_ABOVE")?);
            let held = link.conn.get_property(false, win, state, AtomEnum::ATOM, 0, 32).ok()?.reply().ok()?;
            if !held.value32().is_some_and(|mut atoms| atoms.any(|a| a == above)) {
                // 1 = add; 1 = asked for by a normal program.
                let event = ClientMessageEvent::new(32, win, state, [1, above, 0, 1, 0]);
                let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
                link.conn.send_event(false, link.root, mask, event).ok()?;
            }
            link.conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE)).ok()?;
            link.conn.flush().ok()
        });
    }

    /// The window of this process the window manager keeps with the name `name` (`_NET_CLIENT_LIST`,
    /// `_NET_WM_PID`, `_NET_WM_NAME`); 0 where there is none.
    pub fn own_window_named(name: &str) -> isize {
        with_link(|link| {
            let (list, pid, title, utf8) = (
                atom(link, "_NET_CLIENT_LIST")?,
                atom(link, "_NET_WM_PID")?,
                atom(link, "_NET_WM_NAME")?,
                atom(link, "UTF8_STRING")?,
            );
            let clients =
                link.conn.get_property(false, link.root, list, AtomEnum::WINDOW, 0, 4096).ok()?.reply().ok()?;
            let own = std::process::id();
            clients.value32()?.collect::<Vec<u32>>().into_iter().find_map(|win| {
                let process = link.conn.get_property(false, win, pid, AtomEnum::CARDINAL, 0, 1).ok()?.reply().ok()?;
                if process.value32()?.next()? != own {
                    return None;
                }
                let named = link.conn.get_property(false, win, title, utf8, 0, 256).ok()?.reply().ok()?;
                (named.value == name.as_bytes()).then_some(win as isize)
            })
        })
        .unwrap_or(0)
    }

    /// Moves the window to `x`, `y` (screen pixels), its size kept. Sent without waiting for the answer.
    pub fn move_to(xid: isize, x: i32, y: i32) -> bool {
        let Some(win) = window(xid) else { return false };
        with_link(|link| {
            link.conn.configure_window(win, &ConfigureWindowAux::new().x(x).y(y)).ok()?;
            link.conn.flush().ok()
        })
        .is_some()
    }

    /// Place and size of the window in screen pixels; empty when it cannot be asked.
    pub fn window_rect(xid: isize) -> Rect {
        let Some(win) = window(xid) else { return Rect::default() };
        with_link(|link| {
            let size = link.conn.get_geometry(win).ok()?.reply().ok()?;
            let at = link.conn.translate_coordinates(win, link.root, 0, 0).ok()?.reply().ok()?;
            Some(Rect::new(i32::from(at.dst_x), i32::from(at.dst_y), i32::from(size.width), i32::from(size.height)))
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holds(rects: &[Rect], x: i32, y: i32) -> bool {
        rects.iter().any(|r| r.contains(x, y))
    }

    /// The box around all rectangles.
    fn bounds(rects: &[Rect]) -> Rect {
        rects.iter().skip(1).fold(rects[0], |all, r| all.union(*r))
    }

    /// The facts the Windows region of the same box has: the same bounds to the pixel, the corner pixels
    /// (one pixel in) left out, the straight edges reaching the rounding; at radius 0 the plain rectangle.
    #[test]
    fn the_shape_is_rounded_like_the_box() {
        let r = Rect::new(10, 20, 300, 120);
        let shape = rounded(r, 12);
        assert_eq!(bounds(&shape), r);
        let corners =
            [(r.x + 1, r.y + 1), (r.right() - 2, r.y + 1), (r.x + 1, r.bottom() - 2), (r.right() - 2, r.bottom() - 2)];
        for (x, y) in corners {
            assert!(!holds(&shape, x, y), "corner pixel {x},{y} left out");
        }
        for (x, y) in
            [(r.x + 12, r.y), (r.right() - 13, r.bottom() - 1), (r.x, r.y + 12), (r.right() - 1, r.bottom() - 13)]
        {
            assert!(holds(&shape, x, y), "edge pixel {x},{y} reaches the rounding");
        }
        assert_eq!(rounded(r, 0), vec![r]);
    }

    /// No pixel twice and none outside the box: the rectangles cover exactly the rounded box, row by row the
    /// same on the left and the right, the top and the bottom.
    #[test]
    fn the_shape_covers_each_pixel_once_and_is_symmetric() {
        let r = Rect::new(-5, 7, 61, 33);
        let shape = rounded(r, 9);
        for y in r.y - 1..=r.bottom() {
            for x in r.x - 1..=r.right() {
                let count = shape.iter().filter(|p| p.contains(x, y)).count();
                assert!(count <= 1, "{x},{y} in {count} rectangles");
                if !r.contains(x, y) {
                    assert_eq!(count, 0, "{x},{y} outside the box");
                    continue;
                }
                let mirrored = (r.x + r.right() - 1 - x, r.y + r.bottom() - 1 - y);
                assert_eq!(holds(&shape, x, y), holds(&shape, mirrored.0, mirrored.1), "{x},{y}");
            }
        }
        assert!(shape.len() < 2 * 9 + 1, "rows of the same width are one rectangle: {}", shape.len());
    }

    #[test]
    fn a_radius_beyond_half_the_height_makes_a_pill() {
        let strip = Rect::new(0, 0, 180, 22);
        let pill = rounded(strip, 40);
        assert_eq!(bounds(&pill), strip);
        assert!(holds(&pill, 0, 11) && holds(&pill, 179, 10), "the round ends reach the sides in the middle");
        assert!(!holds(&pill, 1, 1));
        assert!(rounded(Rect::new(0, 0, 0, 10), 4).is_empty(), "nothing to show, no shape");
    }

    /// ⚠ With the lock closed the lock keeps taking clicks: it is the way back.
    #[test]
    fn while_the_lock_passes_clicks_the_lock_alone_takes_them() {
        let lock = Rect::new(300, 4, 24, 24);
        assert_eq!(passing_input(true, true, Some(lock)), Some(lock), "the lock stays the way back");
        assert_eq!(passing_input(false, true, Some(lock)), None, "clicks do not pass: the whole window, from winit");
        assert_eq!(passing_input(true, false, Some(lock)), None, "the game hides the pointer: nothing takes clicks");
        assert_eq!(passing_input(true, true, None), None, "no lock drawn");
        assert_eq!(passing_input(true, true, Some(Rect::new(3, 3, 0, 24))), None, "an empty lock is none");
    }

    #[test]
    fn the_window_type_is_the_one_asked_for_with_the_normal_one_after_it() {
        assert_eq!(window_types(None), None);
        assert_eq!(window_types(Some("")), None);
        assert_eq!(window_types(Some("normal")), None);
        assert_eq!(window_types(Some("anything")), None);
        assert_eq!(
            window_types(Some(" OSD ")),
            Some(["_KDE_NET_WM_WINDOW_TYPE_ON_SCREEN_DISPLAY", "_NET_WM_WINDOW_TYPE_NORMAL"])
        );
        assert_eq!(window_types(Some("utility")).map(|t| t[0]), Some("_NET_WM_WINDOW_TYPE_UTILITY"));
        assert_eq!(window_types(Some("dock")).map(|t| t[0]), Some("_NET_WM_WINDOW_TYPE_DOCK"));
        assert_eq!(window_types(Some("notification")).map(|t| t[0]), Some("_NET_WM_WINDOW_TYPE_NOTIFICATION"));
    }

    /// Under KDE the overlay is an on-screen display without asking; the variable wins, `normal` takes it back, and
    /// other desktops keep the type winit gives.
    #[test]
    fn kde_gets_the_on_screen_display_by_itself() {
        assert_eq!(window_type_choice(None, Some("KDE")).as_deref(), Some("osd"));
        assert_eq!(window_type_choice(None, Some("ubuntu:KDE")).as_deref(), Some("osd"), "a list of desktops");
        assert_eq!(window_type_choice(Some(""), Some("kde")).as_deref(), Some("osd"), "an empty variable is unset");
        assert_eq!(window_type_choice(Some("utility"), Some("KDE")).as_deref(), Some("utility"), "the variable wins");
        assert_eq!(window_types(window_type_choice(Some("normal"), Some("KDE")).as_deref()), None, "normal takes it back");
        assert_eq!(window_type_choice(None, Some("GNOME")), None, "other desktops as before");
        assert_eq!(window_type_choice(None, None), None);
    }

    /// Only the on-screen display is placed by the window manager itself.
    #[test]
    fn the_window_manager_places_only_the_on_screen_display() {
        assert!(manager_places(None, Some("KDE")));
        assert!(manager_places(Some(" OSD "), Some("GNOME")), "asked for by the variable");
        assert!(!manager_places(Some("normal"), Some("KDE")), "normal takes it back");
        assert!(!manager_places(Some("utility"), Some("KDE")));
        assert!(!manager_places(None, Some("GNOME")));
        assert!(!manager_places(None, None));
    }

    /// Only under KDE the keyboard comes by the activation request; other desktops keep `SetInputFocus`.
    #[test]
    fn only_kde_takes_the_keyboard_by_activation() {
        assert!(typing_activation_wanted(Some("KDE")));
        assert!(typing_activation_wanted(Some("ubuntu:KDE")), "a list of desktops");
        assert!(typing_activation_wanted(Some(" kde ")), "case and spaces do not matter");
        assert!(!typing_activation_wanted(Some("GNOME")));
        assert!(!typing_activation_wanted(Some("X-Cinnamon")));
        assert!(!typing_activation_wanted(Some("")));
        assert!(!typing_activation_wanted(None));
    }

    /// ⚠ The source is 2 (a tool): KWin obeys that without the check against focus stealing, which a 1 (a program)
    /// with no time would fail.
    #[test]
    fn the_activation_request_comes_from_a_tool() {
        assert_eq!(active_request_data(0x40_0007), [2, 0, 0x40_0007, 0, 0]);
        assert_eq!(active_request_data(0), [2, 0, 0, 0, 0], "no window active");
    }

    /// The window the window manager lists as active is the one to go back to, not the one that had the focus.
    #[test]
    fn the_window_before_the_typing_is_the_active_one() {
        let (bar, game, other) = (10, 20, 30);
        assert_eq!(remember_before(0, game, bar), game);
        assert_eq!(remember_before(other, game, bar), game, "the newer one wins");
        assert_eq!(remember_before(game, bar, bar), game, "the bar itself is never the one before");
        assert_eq!(remember_before(game, 0, bar), game, "none active: what was remembered stays");
        assert_eq!(remember_before(game, 1, bar), game, "pointer root is no window");
        assert_eq!(remember_before(0, 0, bar), 0);
    }

    /// The game gets the activation back only while the bar is the active window; where the user went elsewhere
    /// meanwhile, or where there is nothing to go back to, nothing is sent.
    #[test]
    fn the_activation_goes_back_only_while_the_bar_is_active() {
        let (bar, game, other) = (10, 20, 30);
        assert_eq!(give_back_to(game, bar, bar), Some(game));
        assert_eq!(give_back_to(game, other, bar), None, "the user went to another window");
        assert_eq!(give_back_to(game, game, bar), None, "the game is active already");
        assert_eq!(give_back_to(game, 0, bar), None, "a window of another kind is active");
        assert_eq!(give_back_to(0, bar, bar), None, "nothing remembered");
        assert_eq!(give_back_to(1, bar, bar), None, "pointer root is no window");
        assert_eq!(give_back_to(bar, bar, bar), None, "not to itself");
    }
}
