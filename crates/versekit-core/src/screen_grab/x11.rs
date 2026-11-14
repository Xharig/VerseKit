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

//! The picture of the game under X11. Star Citizen runs under Proton (Wine) as an X11 window, on a Wayland desktop
//! through XWayland; Wine names the window class after the program (`WM_CLASS` "starcitizen.exe",
//! "StarCitizen.exe"), so the window is found by its class, as under Windows by the file name of its program.
//!
//! ⚠ The picture is taken from the window of the game itself (`GetImage` on that window), not from the root
//! window: under XWayland the root window holds no picture of the windows on it (it reads black), and on an X11
//! desktop with a compositor the window's own picture is the one without the overlay above it, as under Windows
//! (`BitBlt` leaves the layered windows out). Only where no window of the game holds the part asked for is the
//! root window read.
//!
//! In front: `_NET_ACTIVE_WINDOW` of the root window, as every current window manager writes it (KWin also for
//! the windows of XWayland; with a Wayland window in front it names none).
//!
//! The choices are built everywhere, so that their tests run on every system; the connection only where X11 is.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use super::{Area, is_game_path};

/// Whether a `WM_CLASS` (instance and class, each ending in a zero byte) names the program of the game.
pub(super) fn class_is_game(value: &[u8]) -> bool {
    value.split(|&b| b == 0).filter(|part| !part.is_empty()).any(|part| is_game_path(&String::from_utf8_lossy(part)))
}

/// A window found in the tree: its id, its place on the screen, whether it is viewable and of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub id: u32,
    pub area: Area,
    pub viewable: bool,
    pub game: bool,
}

/// Is a window of this size a window of the game, not a splash or a helper? The same bound as under Windows.
pub(super) fn big_enough(area: Area) -> bool {
    area.2 >= 200 && area.3 >= 200
}

/// The window of the game among `windows`: the largest viewable one of the game.
pub(super) fn largest_game(windows: &[Found]) -> Option<Found> {
    windows
        .iter()
        .filter(|w| w.game && w.viewable && big_enough(w.area))
        .max_by_key(|w| i64::from(w.area.2) * i64::from(w.area.3))
        .copied()
}

/// `area` (screen points) within the window at `window` (screen points): the part in points of the window, or
/// `None` when it reaches outside.
pub(super) fn part_in(area: Area, window: Area) -> Option<(i16, i16, u16, u16)> {
    let (x, y) = (area.0 - window.0, area.1 - window.1);
    let inside = x >= 0 && y >= 0 && x + area.2 <= window.2 && y + area.3 <= window.3;
    if !inside {
        return None;
    }
    Some((i16::try_from(x).ok()?, i16::try_from(y).ok()?, u16::try_from(area.2).ok()?, u16::try_from(area.3).ok()?))
}

/// The bytes of a `GetImage` reply (Z pixmap) as BGRA, top row first: `bits` per pixel and the byte order of the
/// server (`msb_first`) as the setup says. Only 32 bits per pixel are taken (depth 24 or 32, the format of every
/// true colour screen); `None` for anything else or too few bytes.
pub(super) fn bgra(data: &[u8], width: usize, height: usize, bits: u8, msb_first: bool) -> Option<Vec<u8>> {
    let size = width.checked_mul(height)?.checked_mul(4)?;
    if bits != 32 || data.len() < size {
        return None;
    }
    let mut out = data[..size].to_vec();
    if msb_first {
        // X R G B in memory: turned round to B G R X.
        for pixel in out.as_chunks_mut::<4>().0 {
            pixel.reverse();
        }
    }
    Some(out)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) use system::{find_game_rect, foreground_is_game, game_rect, grab_raw, in_use};

#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use std::sync::{Mutex, PoisonError};

    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, ImageFormat, ImageOrder, MapState, Window};
    use x11rb::rust_connection::RustConnection;

    use super::super::{Area, GrabError, MAX_POINTS};
    use super::{Found, bgra, big_enough, class_is_game, largest_game, part_in};

    /// How deep the tree is searched below the root: the window of a program sits in a frame of the window
    /// manager, sometimes in one more wrapper.
    const DEPTH: u32 = 3;

    struct Link {
        conn: RustConnection,
        root: Window,
        /// The window of the game found last: asked first at the next grab.
        game: Option<Window>,
    }

    /// One connection for the life of the process; one that fails is made again at the next call.
    static LINK: Mutex<Option<Link>> = Mutex::new(None);

    fn var(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// Whether the windows are on X11 here (an X server, also XWayland).
    pub fn in_use() -> bool {
        let backend = crate::screen::choose_backend(var("DISPLAY").as_deref(), var("WAYLAND_DISPLAY").as_deref(), None);
        backend == crate::screen::Backend::X11
    }

    /// Runs `ask` on the connection, made first if there is none; a failed ask drops it.
    fn with_link<T>(ask: impl FnOnce(&mut Link) -> Option<T>) -> Option<T> {
        if !in_use() {
            return None;
        }
        let mut held = LINK.lock().unwrap_or_else(PoisonError::into_inner);
        if held.is_none() {
            let (conn, number) = RustConnection::connect(None).ok()?;
            let root = conn.setup().roots.get(number)?.root;
            *held = Some(Link { conn, root, game: None });
        }
        let link = held.as_mut()?;
        let answer = ask(link);
        if answer.is_none() && link.conn.get_input_focus().ok().and_then(|c| c.reply().ok()).is_none() {
            // The server is gone: a new connection at the next call.
            *held = None;
        }
        answer
    }

    fn atom(link: &Link, name: &str) -> Option<u32> {
        let atom = link.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom;
        (atom != x11rb::NONE).then_some(atom)
    }

    /// Whether the window carries the class of the game.
    fn is_game(link: &Link, window: Window) -> bool {
        link.conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| class_is_game(&r.value))
    }

    /// The place of a window on the screen (its inside, without the frame of the window manager).
    fn area_of(link: &Link, window: Window) -> Option<Area> {
        let geometry = link.conn.get_geometry(window).ok()?.reply().ok()?;
        let at = link.conn.translate_coordinates(window, link.root, 0, 0).ok()?.reply().ok()?;
        Some((i32::from(at.dst_x), i32::from(at.dst_y), i32::from(geometry.width), i32::from(geometry.height)))
    }

    fn viewable(link: &Link, window: Window) -> bool {
        link.conn
            .get_window_attributes(window)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state == MapState::VIEWABLE)
    }

    /// Every window of the game down to [`DEPTH`] below the root.
    fn game_windows(link: &Link) -> Vec<Found> {
        let mut found = Vec::new();
        let mut level = vec![link.root];
        for _ in 0..DEPTH {
            let mut below = Vec::new();
            for parent in level {
                let Some(tree) = link.conn.query_tree(parent).ok().and_then(|c| c.reply().ok()) else { continue };
                below.extend(tree.children);
            }
            for &id in &below {
                if !is_game(link, id) {
                    continue;
                }
                if let Some(area) = area_of(link, id) {
                    found.push(Found { id, area, viewable: viewable(link, id), game: true });
                }
            }
            level = below;
        }
        found
    }

    /// The window the window manager names active.
    fn active(link: &Link) -> Option<Window> {
        let property = atom(link, "_NET_ACTIVE_WINDOW")?;
        let reply = link.conn.get_property(false, link.root, property, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok()?;
        reply.value32()?.next().filter(|&w| w != x11rb::NONE)
    }

    /// The active window when it is the game's, with its place.
    fn active_game(link: &mut Link) -> Option<Area> {
        let window = active(link)?;
        if !is_game(link, window) {
            return None;
        }
        let area = area_of(link, window).filter(|a| big_enough(*a))?;
        link.game = Some(window);
        Some(area)
    }

    pub fn foreground_is_game() -> bool {
        with_link(|link| active(link).map(|w| is_game(link, w))).unwrap_or(false)
    }

    pub fn game_rect() -> Option<Area> {
        with_link(active_game)
    }

    pub fn find_game_rect() -> Option<Area> {
        with_link(|link| {
            let best = largest_game(&game_windows(link))?;
            link.game = Some(best.id);
            Some(best.area)
        })
    }

    /// The pixels of `part` of `window` as BGRA.
    fn image(link: &Link, window: Window, part: (i16, i16, u16, u16)) -> Option<Vec<u8>> {
        let reply = link
            .conn
            .get_image(ImageFormat::Z_PIXMAP, window, part.0, part.1, part.2, part.3, !0)
            .ok()?
            .reply()
            .ok()?;
        let setup = link.conn.setup();
        let bits = setup.pixmap_formats.iter().find(|f| f.depth == reply.depth)?.bits_per_pixel;
        let msb_first = setup.image_byte_order == ImageOrder::MSB_FIRST;
        bgra(&reply.data, usize::from(part.2), usize::from(part.3), bits, msb_first)
    }

    pub fn grab_raw(area: Area) -> Result<Vec<u8>, GrabError> {
        let (_, _, width, height) = area;
        if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > MAX_POINTS {
            return Err(GrabError::BadArea);
        }
        if !in_use() {
            return Err(GrabError::Unsupported);
        }
        let grabbed = with_link(|link| {
            // The window of the game first: its own picture, also under XWayland.
            let window = link.game.or_else(|| active(link).filter(|&w| is_game(link, w)));
            if let Some(window) = window
                && let Some(part) = area_of(link, window).and_then(|at| part_in(area, at))
                && let Some(bytes) = image(link, window, part)
            {
                return Some(bytes);
            }
            let part = area_of(link, link.root).and_then(|screen| part_in(area, screen))?;
            image(link, link.root, part)
        });
        grabbed.ok_or(GrabError::Failed)
    }

    #[cfg(test)]
    mod tests {
        use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
        use x11rb::wrapper::ConnectionExt as _;

        use super::*;

        /// On an X server (the Linux check under a virtual screen): a mapped window named as Wine names the game
        /// is found, its picture comes back in the size asked for, and a part outside it is refused. Without an
        /// X server there is nothing to look at.
        #[test]
        fn a_window_of_the_game_is_found_by_its_class_and_its_picture_taken() {
            if !in_use() {
                eprintln!("no X server: nothing to look at");
                return;
            }
            let Ok((conn, number)) = RustConnection::connect(None) else { return };
            let root = conn.setup().roots[number].root;
            let id = conn.generate_id().expect("id");
            let aux = CreateWindowAux::new().background_pixel(0x00ff_8040);
            conn.create_window(0, id, root, 10, 20, 320, 240, 0, WindowClass::INPUT_OUTPUT, 0, &aux).expect("make");
            conn.change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"starcitizen.exe\0StarCitizen.exe\0",
            )
            .expect("class");
            conn.map_window(id).expect("map");
            conn.sync().expect("sync");
            let found = (0..50).find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                find_game_rect()
            });
            let (x, y, w, h) = found.expect("the window of the game");
            assert_eq!((w, h), (320, 240));
            let raw = grab_raw((x + 5, y + 5, 30, 20)).expect("picture");
            assert_eq!(raw.len(), 30 * 20 * 4);
            assert_eq!(&raw[..3], &[0x40, 0x80, 0xff], "the background, blue green red");
            let _ = conn.destroy_window(id);
            let _ = conn.sync();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_is_known_by_the_class_wine_gives_its_window() {
        assert!(class_is_game(b"starcitizen.exe\0StarCitizen.exe\0"));
        assert!(class_is_game(b"\0StarCitizen.exe\0"));
        assert!(!class_is_game(b"rsi launcher.exe\0RSI Launcher.exe\0"));
        assert!(!class_is_game(b"steam\0Steam\0"));
        assert!(!class_is_game(b""));
    }

    #[test]
    fn the_largest_viewable_window_of_the_game_is_the_game() {
        let window = |id, area, viewable, game| Found { id, area, viewable, game };
        let windows = [
            window(1, (0, 0, 800, 600), true, true),
            window(2, (0, 0, 3840, 2160), false, true),
            window(3, (0, 0, 2560, 1440), true, true),
            window(4, (0, 0, 5120, 1440), true, false),
            window(5, (0, 0, 150, 4000), true, true),
        ];
        assert_eq!(largest_game(&windows).map(|w| w.id), Some(3), "viewable, of the game, large enough");
        assert_eq!(largest_game(&windows[3..]), None);
    }

    #[test]
    fn a_part_is_taken_in_points_of_the_window_and_only_within_it() {
        assert_eq!(part_in((110, 220, 30, 20), (100, 200, 1920, 1080)), Some((10, 20, 30, 20)));
        assert_eq!(part_in((100, 200, 1920, 1080), (100, 200, 1920, 1080)), Some((0, 0, 1920, 1080)));
        assert_eq!(part_in((99, 200, 10, 10), (100, 200, 1920, 1080)), None, "left of it");
        assert_eq!(part_in((2000, 1270, 30, 20), (100, 200, 1920, 1080)), None, "beyond its corner");
    }

    #[test]
    fn the_pixels_of_the_server_become_blue_green_red_in_either_byte_order() {
        let lsb = [1, 2, 3, 0, 4, 5, 6, 0];
        assert_eq!(bgra(&lsb, 2, 1, 32, false), Some(lsb.to_vec()));
        let msb = [0, 3, 2, 1, 0, 6, 5, 4];
        assert_eq!(bgra(&msb, 2, 1, 32, true), Some(vec![1, 2, 3, 0, 4, 5, 6, 0]));
        assert_eq!(bgra(&lsb, 2, 1, 24, false), None, "only 32 bits a point");
        assert_eq!(bgra(&lsb, 3, 1, 32, false), None, "too few bytes");
    }
}
