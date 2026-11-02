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

//! The screens of X11: RandR knows every monitor and the one marked as the
//! main screen, the window manager the work area (`_NET_WORKAREA`, one
//! rectangle for all screens together), the root window the pointer. The
//! connection stays open; one that fails is made again at the next look.
//! Under Wayland nothing is asked, see [`super::x11_in_use`].

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use super::{Area, Monitor, Setup, work_inside, x11_in_use, x11_scale};

/// How many virtual desktops a work area is read for.
const DESKTOPS: u32 = 64;

struct Link {
    conn: RustConnection,
    root: u32,
    /// The whole root window: the one screen when RandR has no monitors.
    size: (u16, u16),
    scale: f32,
}

/// The connection, and when the last one failed.
struct State {
    link: Option<Link>,
    failed: Option<Instant>,
}

static LINK: Mutex<State> = Mutex::new(State { link: None, failed: None });

/// After a failure nothing is tried for this long: the pointer is asked in every frame.
const RETRY: Duration = Duration::from_secs(2);

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn connect() -> Option<Link> {
    let (conn, number) = RustConnection::connect(None).ok()?;
    let screen = conn.setup().roots.get(number)?;
    let (root, size) = (screen.root, (screen.width_in_pixels, screen.height_in_pixels));
    let dpi = x11rb::resource_manager::new_from_default(&conn)
        .ok()
        .and_then(|db| db.get_string("Xft.dpi", "").map(str::to_owned));
    let scale = x11_scale(var("WINIT_X11_SCALE_FACTOR").as_deref(), dpi.as_deref());
    Some(Link { conn, root, size, scale })
}

/// Runs `ask` on the connection, made first if there is none. A failed ask
/// drops the connection; the next try comes after [`RETRY`].
fn with_link<T>(ask: impl FnOnce(&Link) -> Option<T>) -> Option<T> {
    let (display, wayland, session) = (var("DISPLAY"), var("WAYLAND_DISPLAY"), var("XDG_SESSION_TYPE"));
    if !x11_in_use(display.as_deref(), wayland.as_deref(), session.as_deref()) {
        return None;
    }
    let mut state = LINK.lock().unwrap_or_else(PoisonError::into_inner);
    if state.link.is_none() {
        if state.failed.is_some_and(|at| at.elapsed() < RETRY) {
            return None;
        }
        state.link = connect();
    }
    let answer = state.link.as_ref().and_then(ask);
    if answer.is_none() {
        state.link = None;
        state.failed = Some(Instant::now());
    }
    answer
}

/// Every monitor and the work area of each, in pixels.
pub(super) fn query() -> Option<Setup> {
    with_link(|link| {
        let (areas, primary) = monitors(link)?;
        let desktop = desktop_work(link);
        let monitors = areas
            .into_iter()
            .map(|area| Monitor { area, work: desktop.map_or(area, |work| work_inside(area, work)) })
            .collect();
        Some(Setup { scale: link.scale, primary, monitors })
    })
}

/// Where the pointer is (pixels of the root window).
pub(super) fn pointer() -> Option<(i32, i32)> {
    with_link(|link| {
        let at = link.conn.query_pointer(link.root).ok()?.reply().ok()?;
        Some((i32::from(at.root_x), i32::from(at.root_y)))
    })
}

/// The monitors and the index of the main one; the whole root window as the
/// only monitor when RandR (1.5) tells none or is not there.
fn monitors(link: &Link) -> Option<(Vec<Area>, usize)> {
    let reply = link.conn.randr_get_monitors(link.root, true).ok().and_then(|cookie| cookie.reply().ok());
    let mut areas = Vec::new();
    let mut primary = 0;
    for monitor in reply.iter().flat_map(|r| &r.monitors).filter(|m| m.width > 0 && m.height > 0) {
        if monitor.primary {
            primary = areas.len();
        }
        areas.push((i32::from(monitor.x), i32::from(monitor.y), i32::from(monitor.width), i32::from(monitor.height)));
    }
    if areas.is_empty() {
        areas.push((0, 0, i32::from(link.size.0), i32::from(link.size.1)));
    }
    Some((areas, primary))
}

/// The work area of the current virtual desktop, if the window manager tells.
fn desktop_work(link: &Link) -> Option<Area> {
    let atom = |name: &[u8]| -> Option<u32> {
        let atom = link.conn.intern_atom(true, name).ok()?.reply().ok()?.atom;
        (atom != x11rb::NONE).then_some(atom)
    };
    let cardinals = |property: u32, length: u32| -> Option<Vec<u32>> {
        let reply =
            link.conn.get_property(false, link.root, property, AtomEnum::CARDINAL, 0, length).ok()?.reply().ok()?;
        Some(reply.value32()?.collect())
    };
    let desktop = atom(b"_NET_CURRENT_DESKTOP").and_then(|a| cardinals(a, 1)).and_then(|v| v.first().copied());
    let work = cardinals(atom(b"_NET_WORKAREA")?, 4 * DESKTOPS)?;
    let (rects, _) = work.as_chunks::<4>();
    let rect = rects.get(desktop.unwrap_or(0) as usize).or(rects.first())?;
    let number = |v: u32| i32::try_from(v).ok();
    Some((number(rect[0])?, number(rect[1])?, number(rect[2])?, number(rect[3])?))
}
