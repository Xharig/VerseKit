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

//! The window of another process under X11, found by its process (`_NET_WM_PID`): whether it is on screen, and
//! bringing it to the front without the keyboard. The window of a process is the first one winit made in it (the
//! smallest id among its windows with a class): the main window comes before the windows of its lists.
//!
//! To the front: a hidden or minimized window is mapped again (the window manager restores it), the window is
//! raised and kept above the others for a moment ([`ABOVE_FOR`]), so that the window manager puts it in front,
//! and then it is a normal window again. Under a Wayland session (the window through XWayland) the compositor
//! does not let a window be pushed in front: one that does not have the keyboard is unmapped and mapped again
//! there, which it takes as a new window and shows in front. The keyboard stays where it is.
//!
//! The choices are built everywhere, so that their tests run on every system; the connection only where X11 is.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use std::time::Duration;

use super::needs_show;

/// `WM_STATE` of a window the window manager does not manage (closed to the tray area).
#[cfg(test)]
pub(super) const WITHDRAWN: u32 = 0;
/// `WM_STATE` of a window on a desktop, also one that is not the current one.
pub(super) const NORMAL: u32 = 1;
/// `WM_STATE` of a minimized window.
pub(super) const ICONIC: u32 = 3;

/// How long the window stays above the others when brought to the front.
pub(super) const ABOVE_FOR: Duration = Duration::from_millis(400);

/// What X11 tells of a window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Seen {
    /// Mapped, and every window above it in the tree too.
    pub viewable: bool,
    /// The state the window manager wrote (`WM_STATE`), when it wrote one.
    pub wm_state: Option<u32>,
    /// `_NET_WM_STATE_HIDDEN`: minimized as the window manager says.
    pub hidden: bool,
    /// `_NET_WM_STATE_ABOVE`: kept above the others already.
    pub above: bool,
}

impl Seen {
    /// Minimized: iconic, or hidden by the window manager.
    pub(super) fn minimized(self) -> bool {
        self.hidden || self.wm_state == Some(ICONIC)
    }

    /// On screen as the system of Windows counts it: a window that is mapped, minimized, or kept by the window
    /// manager (on another desktop); only a window taken off the screen by its program (withdrawn) is hidden.
    pub(super) fn shown(self) -> bool {
        self.viewable || self.minimized() || self.wm_state == Some(NORMAL)
    }
}

/// What bringing a window to the front does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Plan {
    /// Unmapped first, to be mapped again as a new window.
    pub unmap: bool,
    /// Mapped (shown again, restored from minimized).
    pub map: bool,
    /// Kept above the others for [`ABOVE_FOR`], then a normal window again; not for one kept above already.
    pub above: bool,
}

/// The steps for the window `seen`: mapped when hidden or minimized; under a Wayland session (`wayland`) a shown
/// window without the keyboard (`active` false) is unmapped and mapped again.
pub(super) fn plan(seen: Seen, wayland: bool, active: bool) -> Plan {
    let renew = wayland && !active && seen.viewable && !seen.minimized();
    Plan { unmap: renew, map: renew || needs_show(seen.shown(), seen.minimized()), above: !seen.above }
}

/// Whether the session runs under Wayland: the compositor sets `WAYLAND_DISPLAY`, the login `XDG_SESSION_TYPE`.
pub(super) fn wayland_session(wayland_display: Option<&str>, session_type: Option<&str>) -> bool {
    wayland_display.is_some_and(|v| !v.is_empty()) || session_type == Some("wayland")
}

/// A window found in the tree: its id, its process (`_NET_WM_PID`), whether it carries a class (`WM_CLASS`, as
/// every window of winit does; helper windows without one, such as the one of the clipboard, are no candidates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub id: u32,
    pub pid: Option<u32>,
    pub class: bool,
}

/// The window of the process `pid` among `windows`: the one with a class made first (the smallest id).
pub(super) fn window_of(windows: &[Found], pid: u32) -> Option<u32> {
    windows.iter().filter(|w| w.class && w.pid == Some(pid)).map(|w| w.id).min()
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(super) use system::{bring_to_front, is_shown};

#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask, MapState, StackMode, Window,
    };
    use x11rb::rust_connection::RustConnection;

    use super::{ABOVE_FOR, Found, Seen, plan, wayland_session, window_of};

    /// How deep the tree is searched below the root: the window of a program sits in a frame of the window
    /// manager, sometimes in one more wrapper.
    const DEPTH: u32 = 3;

    struct Link {
        conn: RustConnection,
        root: Window,
    }

    fn var(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// A connection of its own, where the windows are on X11.
    fn connect() -> Option<Link> {
        let backend = crate::screen::choose_backend(var("DISPLAY").as_deref(), var("WAYLAND_DISPLAY").as_deref(), None);
        if backend != crate::screen::Backend::X11 {
            return None;
        }
        let (conn, number) = RustConnection::connect(None).ok()?;
        let root = conn.setup().roots.get(number)?.root;
        Some(Link { conn, root })
    }

    fn atom(link: &Link, name: &str) -> Option<u32> {
        let atom = link.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom;
        (atom != x11rb::NONE).then_some(atom)
    }

    /// The 32-bit values of a property, empty when it is not there.
    fn values(link: &Link, window: Window, property: u32, kind: u32) -> Vec<u32> {
        let reply = link.conn.get_property(false, window, property, kind, 0, 64).ok().and_then(|c| c.reply().ok());
        reply.and_then(|r| r.value32().map(Iterator::collect)).unwrap_or_default()
    }

    /// Every window down to [`DEPTH`] below the root, with its process and whether it has a class.
    fn windows(link: &Link) -> Vec<Found> {
        let Some(pid_atom) = atom(link, "_NET_WM_PID") else { return Vec::new() };
        let mut found = Vec::new();
        let mut level = vec![link.root];
        for _ in 0..DEPTH {
            let mut below = Vec::new();
            for parent in level {
                let Some(tree) = link.conn.query_tree(parent).ok().and_then(|c| c.reply().ok()) else { continue };
                below.extend(tree.children);
            }
            for &id in &below {
                let pid = values(link, id, pid_atom, AtomEnum::CARDINAL.into()).first().copied();
                let class = link
                    .conn
                    .get_property(false, id, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .is_some_and(|r| !r.value.is_empty());
                found.push(Found { id, pid, class });
            }
            level = below;
        }
        found
    }

    fn seen(link: &Link, window: Window) -> Option<Seen> {
        let attributes = link.conn.get_window_attributes(window).ok()?.reply().ok()?;
        let wm_state = atom(link, "WM_STATE").and_then(|a| values(link, window, a, a).first().copied());
        let held = atom(link, "_NET_WM_STATE").map(|a| values(link, window, a, AtomEnum::ATOM.into()));
        let has = |name: &str| held.as_ref().zip(atom(link, name)).is_some_and(|(held, a)| held.contains(&a));
        Some(Seen {
            viewable: attributes.map_state == MapState::VIEWABLE,
            wm_state,
            hidden: has("_NET_WM_STATE_HIDDEN"),
            above: has("_NET_WM_STATE_ABOVE"),
        })
    }

    /// Whether `window` has the keyboard as the window manager says (`_NET_ACTIVE_WINDOW`).
    fn active(link: &Link, window: Window) -> bool {
        atom(link, "_NET_ACTIVE_WINDOW")
            .is_some_and(|a| values(link, link.root, a, AtomEnum::WINDOW.into()).first() == Some(&window))
    }

    /// Asks the window manager to add (`add`) or remove `_NET_WM_STATE_ABOVE` of `window`.
    fn ask_above(link: &Link, window: Window, add: bool) -> Option<()> {
        let (state, above) = (atom(link, "_NET_WM_STATE")?, atom(link, "_NET_WM_STATE_ABOVE")?);
        // 1 = add, 0 = remove; 1 = asked for by a normal program.
        let event = ClientMessageEvent::new(32, window, state, [u32::from(add), above, 0, 1, 0]);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        link.conn.send_event(false, link.root, mask, event).ok()?;
        link.conn.flush().ok()
    }

    /// Whether the window of the process `pid` is on screen ([`Seen::shown`]); `None` when it cannot be told
    /// (no X11, no window of that process).
    pub fn is_shown(pid: u32) -> Option<bool> {
        let link = connect()?;
        let window = window_of(&windows(&link), pid)?;
        Some(seen(&link, window)?.shown())
    }

    /// Brings the window of the process `pid` to the front ([`super::plan`]); false when there is none or no X11.
    pub fn bring_to_front(pid: u32) -> bool {
        let Some(link) = connect() else { return false };
        let Some(window) = window_of(&windows(&link), pid) else { return false };
        let Some(seen) = seen(&link, window) else { return false };
        let wayland = wayland_session(var("WAYLAND_DISPLAY").as_deref(), var("XDG_SESSION_TYPE").as_deref());
        let steps = plan(seen, wayland, active(&link, window));
        if steps.unmap {
            let _ = link.conn.unmap_window(window);
        }
        if steps.map {
            let _ = link.conn.map_window(window);
        }
        let _ = link.conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
        if steps.above && ask_above(&link, window, true).is_some() {
            // Removed in a thread of its own: the caller draws on meanwhile.
            let _ = std::thread::Builder::new().name("nach-vorn".into()).spawn(move || {
                std::thread::sleep(ABOVE_FOR);
                let _ = ask_above(&link, window, false);
            });
            return true;
        }
        link.conn.flush().is_ok()
    }

    #[cfg(test)]
    mod tests {
        use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
        use x11rb::wrapper::ConnectionExt as _;

        use super::*;

        /// A window of this process as winit makes one (its process and a class), not mapped.
        fn made(link: &Link) -> Window {
            let id = link.conn.generate_id().expect("id");
            let aux = CreateWindowAux::new();
            link.conn
                .create_window(0, id, link.root, 0, 0, 40, 30, 0, WindowClass::INPUT_OUTPUT, 0, &aux)
                .expect("make");
            let pid = atom(link, "_NET_WM_PID").expect("atom");
            link.conn.change_property32(PropMode::REPLACE, id, pid, AtomEnum::CARDINAL, &[std::process::id()]).unwrap();
            link.conn
                .change_property8(PropMode::REPLACE, id, AtomEnum::WM_CLASS, AtomEnum::STRING, b"probe\0Probe\0")
                .unwrap();
            link.conn.sync().expect("sync");
            id
        }

        /// On an X server (in the Linux check under a virtual screen): a withdrawn window of this process counts
        /// as hidden, is mapped by `bring_to_front` and counts as shown then. Without an X server there is
        /// nothing to look at.
        #[test]
        fn a_withdrawn_window_of_a_process_is_hidden_and_brought_back_to_the_screen() {
            let Some(link) = connect() else {
                eprintln!("no X server: nothing to look at");
                return;
            };
            let window = made(&link);
            assert_eq!(window_of(&windows(&link), std::process::id()), Some(window));
            assert_eq!(is_shown(std::process::id()), Some(false), "never mapped: withdrawn");
            assert!(bring_to_front(std::process::id()));
            link.conn.sync().expect("sync");
            let shown = (0..50).any(|_| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                is_shown(std::process::id()) == Some(true)
            });
            assert!(shown, "mapped by bringing it to the front");
            let _ = link.conn.destroy_window(window);
            let _ = link.conn.sync();
            assert_eq!(is_shown(std::process::id()), None, "no window of this process: cannot be told");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_counts_as_hidden_only_when_its_program_took_it_off_the_screen() {
        let mapped = Seen { viewable: true, wm_state: Some(NORMAL), ..Seen::default() };
        assert!(mapped.shown());
        let minimized = Seen { viewable: false, wm_state: Some(ICONIC), ..Seen::default() };
        assert!(minimized.shown() && minimized.minimized(), "minimized counts as shown, as under Windows");
        let hidden_by_manager = Seen { viewable: false, wm_state: Some(NORMAL), hidden: true, above: false };
        assert!(hidden_by_manager.shown() && hidden_by_manager.minimized());
        let other_desktop = Seen { viewable: false, wm_state: Some(NORMAL), ..Seen::default() };
        assert!(other_desktop.shown() && !other_desktop.minimized(), "on another desktop");
        assert!(!Seen { viewable: false, wm_state: Some(WITHDRAWN), ..Seen::default() }.shown(), "closed to the tray");
        assert!(!Seen::default().shown(), "never mapped, no state");
        assert!(Seen { viewable: true, ..Seen::default() }.shown(), "mapped without a window manager");
    }

    #[test]
    fn a_hidden_or_minimized_window_is_mapped_and_a_shown_one_is_only_raised() {
        let mapped = Seen { viewable: true, wm_state: Some(NORMAL), ..Seen::default() };
        assert_eq!(plan(mapped, false, false), Plan { unmap: false, map: false, above: true });
        let withdrawn = Seen { wm_state: Some(WITHDRAWN), ..Seen::default() };
        assert_eq!(plan(withdrawn, false, false), Plan { unmap: false, map: true, above: true });
        let minimized = Seen { wm_state: Some(ICONIC), ..Seen::default() };
        assert_eq!(plan(minimized, false, false), Plan { unmap: false, map: true, above: true });
        let kept_above = Seen { above: true, ..mapped };
        assert!(!plan(kept_above, false, false).above, "a window kept above stays so");
    }

    /// As 4.12 does under Wayland: a shown window without the keyboard is made anew, one with it is left.
    #[test]
    fn under_wayland_a_shown_window_without_the_keyboard_is_mapped_anew() {
        let mapped = Seen { viewable: true, wm_state: Some(NORMAL), ..Seen::default() };
        assert_eq!(plan(mapped, true, false), Plan { unmap: true, map: true, above: true });
        assert_eq!(plan(mapped, true, true), Plan { unmap: false, map: false, above: true });
        let withdrawn = Seen::default();
        assert_eq!(plan(withdrawn, true, false), Plan { unmap: false, map: true, above: true }, "mapped, not unmapped");
        let minimized = Seen { wm_state: Some(ICONIC), ..Seen::default() };
        assert_eq!(plan(minimized, true, false), Plan { unmap: false, map: true, above: true });
    }

    #[test]
    fn the_session_is_wayland_by_either_variable() {
        assert!(wayland_session(Some("wayland-0"), None));
        assert!(wayland_session(None, Some("wayland")));
        assert!(!wayland_session(Some(""), Some("x11")));
        assert!(!wayland_session(None, None));
    }

    #[test]
    fn the_window_of_a_process_is_its_first_with_a_class() {
        let windows = [
            Found { id: 0x0420_0007, pid: Some(77), class: true },
            Found { id: 0x0420_0003, pid: Some(77), class: true },
            Found { id: 0x0400_0001, pid: Some(77), class: false },
            Found { id: 0x0300_0001, pid: Some(12), class: true },
            Found { id: 0x0200_0001, pid: None, class: true },
        ];
        assert_eq!(window_of(&windows, 77), Some(0x0420_0003), "the first made, not the clipboard window");
        assert_eq!(window_of(&windows, 12), Some(0x0300_0001));
        assert_eq!(window_of(&windows, 5), None);
    }
}
