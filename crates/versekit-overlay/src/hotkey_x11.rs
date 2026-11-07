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

//! The hotkey under X11: the key is grabbed at the root window (`XGrabKey`) on a connection of its own,
//! once for each state of Caps Lock and Num Lock, which the X server counts as modifiers. A thread waits
//! for the presses and grabs anew when the setting changes.
//!
//! ⚠ A grabbed key goes to this program only, not to the window in front (like `RegisterHotKey`).
//!
//! ⚠ Under a Wayland session (XWayland) the X server sees the keys only while an X11 window has the
//! keyboard: in the game (Wine on XWayland) the hotkey works, on the desktop not.
//!
//! The arithmetic (key symbol, modifier mask, the lock states, the key code) is built everywhere, so that
//! its tests run on every system; the thread only where X11 is.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use versekit_core::language::Phrase;
use versekit_core::overlay::Hotkey;

/// The modifier bits of X11.
pub const SHIFT: u16 = 1 << 0;
pub const LOCK: u16 = 1 << 1;
pub const CONTROL: u16 = 1 << 2;
pub const MOD1: u16 = 1 << 3;
/// Where Num Lock usually is; the keyboard map of the server decides.
pub const MOD2: u16 = 1 << 4;

/// The key symbol of Num Lock.
const NUM_LOCK: u32 = 0xff7f;

/// The X11 key symbol of a Windows key code as [`Hotkey`] holds it: the small letter for `A`–`Z`, the
/// digit for `0`–`9`, `F1`–`F24` (X11 counts them on from `F1` = 0xffbe, as Windows does from 0x70) and the
/// named keys (`overlay::NAMED_KEYS`): Pause, Scroll Lock, Insert and the arithmetic keys of the number block.
/// Insert is `XK_Insert`, not `XK_KP_Insert`: the 0 of the number block with Num Lock off is another symbol.
pub fn keysym(vkey: u16) -> Option<u32> {
    match vkey {
        0x41..=0x5a => Some(u32::from(vkey) + 0x20),
        0x30..=0x39 => Some(u32::from(vkey)),
        0x70..=0x87 => Some(0xffbe + u32::from(vkey - 0x70)),
        0x13 => Some(0xff13),
        0x91 => Some(0xff14),
        0x2d => Some(0xff63),
        0x6f => Some(0xffaf),
        0x6a => Some(0xffaa),
        0x6d => Some(0xffad),
        0x6b => Some(0xffab),
        _ => None,
    }
}

/// The X11 modifier mask of the hotkey: Shift, Control, Alt (`Mod1`).
pub fn modifiers(hotkey: Hotkey) -> u16 {
    let mut mask = 0;
    if hotkey.shift {
        mask |= SHIFT;
    }
    if hotkey.ctrl {
        mask |= CONTROL;
    }
    if hotkey.alt {
        mask |= MOD1;
    }
    mask
}

/// The masks a grab of `base` needs, one for each state of Caps Lock and of Num Lock (`num_lock`, the
/// modifier bit it has on this server): the X server counts both as modifiers, and a grab without them
/// would miss every press while one is on.
pub fn variants(base: u16, num_lock: u16) -> Vec<u16> {
    let mut masks = Vec::new();
    for extra in [0, LOCK, num_lock, LOCK | num_lock] {
        let mask = base | extra;
        if !masks.contains(&mask) {
            masks.push(mask);
        }
    }
    masks
}

/// The key code that carries `symbol` in the keyboard map (`per` symbols for each code from `first` on),
/// the first one that has it; `None` when no key has it.
pub fn keycode_of(symbol: u32, first: u8, per: usize, map: &[u32]) -> Option<u8> {
    if per == 0 {
        return None;
    }
    let index = map.chunks(per).position(|symbols| symbols.contains(&symbol))?;
    u8::try_from(usize::from(first) + index).ok()
}

/// The modifier bit Num Lock has: the one of the eight whose keys (`per` codes each, in the order of the
/// modifier map) hold a key code of `num_lock`; [`MOD2`] when none does.
pub fn num_lock_mask(map: &[u8], per: usize, num_lock: &[u8]) -> u16 {
    if per == 0 {
        return MOD2;
    }
    map.chunks(per)
        .take(8)
        .position(|codes| codes.iter().any(|c| *c != 0 && num_lock.contains(c)))
        .map_or(MOD2, |bit| 1 << bit)
}

/// The hotkey as the settings store it (`Strg+Alt+B`).
pub fn stored_text(hotkey: Hotkey) -> String {
    hotkey.text()
}

/// What became of a grab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// The key is grabbed in every lock state.
    Held,
    /// Another program holds the combination.
    Taken,
    /// No key of the keyboard carries the symbol.
    NoKey,
}

/// The line the overlay shows for a grab: nothing for the first one that worked (the start), the hint for
/// one that failed, a confirmation for a new combination that works.
pub fn told(grab: Grab, hotkey: Hotkey, first: bool) -> Option<Phrase> {
    let key = match grab {
        Grab::Held if first => return None,
        Grab::Held => "ov_hotkey_angemeldet",
        Grab::Taken => "ov_hotkey_belegt",
        Grab::NoKey => "ov_hotkey_keine_taste",
    };
    Some(Phrase::new(key).with("0", stored_text(hotkey)))
}

/// The lines of the grabs not yet taken by the overlay.
static TOLD: std::sync::Mutex<Vec<Phrase>> = std::sync::Mutex::new(Vec::new());

fn tell(phrase: Phrase) {
    TOLD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(phrase);
}

/// The lines about the hotkey since the last look, for the rows of the overlay.
pub fn take_told() -> Vec<Phrase> {
    std::mem::take(&mut *TOLD.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use system::start;

#[cfg(all(unix, not(target_os = "macos")))]
mod system {
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use versekit_core::overlay::Hotkey;
    use x11rb::connection::Connection;
    use x11rb::errors::ReplyError;
    use x11rb::protocol::ErrorKind;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
    use x11rb::rust_connection::RustConnection;

    use super::{Grab, NUM_LOCK, keycode_of, keysym, modifiers, num_lock_mask, tell, told, variants};

    /// How often the thread looks for presses and for a changed setting.
    const EVERY: Duration = Duration::from_millis(40);

    /// A combination grabbed: its key code and masks.
    struct Held {
        code: u8,
        masks: Vec<u16>,
    }

    /// Grabs `hotkey` at `root` in every lock state; on a refusal the masks grabbed are let go again.
    fn grab(conn: &RustConnection, root: u32, hotkey: Hotkey, num_lock: u16) -> (Grab, Option<Held>) {
        let Some(code) = keysym(hotkey.key).and_then(|symbol| code_of(conn, symbol)) else {
            return (Grab::NoKey, None);
        };
        let masks = variants(modifiers(hotkey), num_lock);
        let mut done = Vec::new();
        for &mask in &masks {
            let result = conn
                .grab_key(true, root, ModMask::from(mask), code, GrabMode::ASYNC, GrabMode::ASYNC)
                .map_err(ReplyError::from)
                .and_then(|cookie| cookie.check());
            match result {
                Ok(()) => done.push(mask),
                Err(error) => {
                    let held = Held { code, masks: done };
                    release(conn, root, &held);
                    let taken = matches!(&error, ReplyError::X11Error(e) if e.error_kind == ErrorKind::Access);
                    return (if taken { Grab::Taken } else { Grab::NoKey }, None);
                }
            }
        }
        (Grab::Held, Some(Held { code, masks: done }))
    }

    fn release(conn: &RustConnection, root: u32, held: &Held) {
        for &mask in &held.masks {
            let _ = conn.ungrab_key(held.code, root, ModMask::from(mask));
        }
        let _ = conn.flush();
    }

    /// The key code that carries `symbol` on this server.
    fn code_of(conn: &RustConnection, symbol: u32) -> Option<u8> {
        let setup = conn.setup();
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let map = conn.get_keyboard_mapping(first, last - first + 1).ok()?.reply().ok()?;
        keycode_of(symbol, first, usize::from(map.keysyms_per_keycode), &map.keysyms)
    }

    /// The modifier bit of Num Lock on this server.
    fn num_lock_of(conn: &RustConnection) -> u16 {
        let codes: Vec<u8> = code_of(conn, NUM_LOCK).into_iter().collect();
        let Some(map) = conn.get_modifier_mapping().ok().and_then(|c| c.reply().ok()) else { return super::MOD2 };
        num_lock_mask(&map.keycodes, usize::from(map.keycodes_per_modifier()), &codes)
    }

    /// Grabs the hotkey of `hotkey` and sends a signal for each press. The combination is read from
    /// `hotkey` all the time, so a change in the settings takes effect at once. The line the overlay shows
    /// for a grab ([`told`]) waits in [`super::take_told`]. Returns `false` without an X server.
    pub fn start(hotkey: Arc<Mutex<Option<Hotkey>>>, sender: Sender<()>, wake: impl Fn() + Send + 'static) -> bool {
        let Ok((conn, number)) = RustConnection::connect(None) else { return false };
        let Some(root) = conn.setup().roots.get(number).map(|s| s.root) else { return false };
        std::thread::Builder::new()
            .name("hotkey".into())
            .spawn(move || {
                let num_lock = num_lock_of(&conn);
                let mut wanted: Option<Hotkey> = None;
                let mut held: Option<Held> = None;
                let mut first = true;
                // The time of the last release: a press at the same time is the key repeating, not a new press.
                let mut released_at = None;
                loop {
                    let combo = *hotkey.lock().unwrap_or_else(|e| e.into_inner());
                    if first || combo != wanted {
                        if let Some(old) = held.take() {
                            release(&conn, root, &old);
                        }
                        if let Some(combo) = combo {
                            let (grab, now) = grab(&conn, root, combo, num_lock);
                            held = now;
                            if let Some(phrase) = told(grab, combo, first) {
                                tell(phrase);
                                wake();
                            }
                        }
                        wanted = combo;
                        first = false;
                    }
                    loop {
                        match conn.poll_for_event() {
                            Ok(Some(Event::KeyPress(press))) => {
                                let repeat = released_at == Some((press.detail, press.time));
                                if !repeat && held.as_ref().is_some_and(|h| h.code == press.detail) {
                                    // A sender whose receiver is gone (the overlay closes) ends the thread.
                                    if sender.send(()).is_err() {
                                        return;
                                    }
                                    wake();
                                }
                            }
                            Ok(Some(Event::KeyRelease(release))) => released_at = Some((release.detail, release.time)),
                            Ok(Some(_)) => {}
                            Ok(None) => break,
                            // The connection is gone: no hotkey any more.
                            Err(_) => return,
                        }
                    }
                    std::thread::sleep(EVERY);
                }
            })
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(text: &str) -> Hotkey {
        Hotkey::parse(text).expect("a hotkey")
    }

    #[test]
    fn the_windows_key_codes_become_x11_key_symbols() {
        assert_eq!(keysym(u16::from(b'B')), Some(u32::from(b'b')), "the small letter");
        assert_eq!(keysym(u16::from(b'A')), Some(0x61));
        assert_eq!(keysym(u16::from(b'Z')), Some(0x7a));
        assert_eq!(keysym(u16::from(b'0')), Some(0x30));
        assert_eq!(keysym(u16::from(b'9')), Some(0x39));
        assert_eq!(keysym(0x70), Some(0xffbe), "F1");
        assert_eq!(keysym(0x7b), Some(0xffc9), "F12");
        // The X11 symbols XK_F13 and XK_F24.
        assert_eq!(keysym(0x7c), Some(0xffca), "F13");
        assert_eq!(keysym(0x87), Some(0xffd5), "F24");
        assert_eq!(keysym(0x88), None, "past F24");
        assert_eq!(keysym(0x20), None);
        // Every key the settings accept has a symbol.
        for text in ["Strg+Alt+B", "Alt+F8", "Umschalt+7", "Ctrl+Shift+F12", "Alt+F13", "Strg+F24", "F13"] {
            assert!(keysym(combo(text).key).is_some(), "{text}");
        }
        // The named keys: XK_Pause, XK_Scroll_Lock, XK_Insert and XK_KP_Divide, _Multiply, _Subtract, _Add.
        let named: Vec<Option<u32>> =
            versekit_core::overlay::NAMED_KEYS.iter().map(|(name, _, _)| keysym(combo(name).key)).collect();
        let symbols = [0xff13, 0xff14, 0xff63, 0xffaf, 0xffaa, 0xffad, 0xffab];
        assert_eq!(named, symbols.map(Some));
    }

    /// A key by itself is grabbed without a modifier, in every lock state like the others.
    #[test]
    fn a_key_by_itself_is_grabbed_without_a_modifier() {
        let f13 = combo("F13");
        assert_eq!(modifiers(f13), 0);
        assert_eq!(variants(modifiers(f13), MOD2), vec![0, LOCK, MOD2, LOCK | MOD2]);
        assert_eq!(stored_text(f13), "F13");
    }

    #[test]
    fn the_modifiers_become_the_x11_mask() {
        assert_eq!(modifiers(combo("Strg+Alt+B")), CONTROL | MOD1);
        assert_eq!(modifiers(combo("Umschalt+F2")), SHIFT);
        assert_eq!(modifiers(combo("Ctrl+Alt+Shift+Q")), SHIFT | CONTROL | MOD1);
    }

    /// The grab covers the hotkey with Caps Lock and Num Lock off, each on alone and both on.
    #[test]
    fn the_grab_covers_every_state_of_caps_lock_and_num_lock() {
        let base = CONTROL | MOD1;
        assert_eq!(variants(base, MOD2), vec![base, base | LOCK, base | MOD2, base | LOCK | MOD2]);
        // Num Lock on another modifier of this server.
        let mod3 = 1 << 5;
        assert_eq!(variants(base, mod3), vec![base, base | LOCK, base | mod3, base | LOCK | mod3]);
        // A server without Num Lock: no mask twice.
        assert_eq!(variants(SHIFT, 0), vec![SHIFT, SHIFT | LOCK]);
    }

    #[test]
    fn the_key_code_is_the_first_that_carries_the_symbol() {
        // Codes 8 to 11, two symbols each: Escape, 1 / !, b / B, F1.
        let map = [0xff1b, 0, 0x31, 0x21, 0x62, 0x42, 0xffbe, 0];
        assert_eq!(keycode_of(0x62, 8, 2, &map), Some(10));
        assert_eq!(keycode_of(0x31, 8, 2, &map), Some(9));
        assert_eq!(keycode_of(0xffbe, 8, 2, &map), Some(11));
        assert_eq!(keycode_of(0x7a, 8, 2, &map), None, "no key has it");
        assert_eq!(keycode_of(0x62, 8, 0, &map), None);
    }

    #[test]
    fn num_lock_is_the_modifier_whose_keys_hold_it() {
        // Two codes per modifier: Shift, Lock, Control, Mod1, Mod2 (Num Lock, code 77), Mod3, Mod4, Mod5.
        let map = [50, 62, 66, 0, 37, 105, 64, 108, 77, 0, 0, 0, 133, 134, 92, 0];
        assert_eq!(num_lock_mask(&map, 2, &[77]), MOD2);
        // Num Lock on Mod3.
        let moved = [50, 62, 66, 0, 37, 105, 64, 108, 0, 0, 77, 0, 133, 134, 92, 0];
        assert_eq!(num_lock_mask(&moved, 2, &[77]), 1 << 5);
        assert_eq!(num_lock_mask(&map, 2, &[]), MOD2, "no Num Lock key: the usual place");
        assert_eq!(num_lock_mask(&map, 2, &[0]), MOD2, "an empty slot is no key");
    }

    #[test]
    fn the_hotkey_is_named_as_the_settings_store_it() {
        assert_eq!(stored_text(combo("ctrl+alt+b")), "Strg+Alt+B");
        assert_eq!(stored_text(combo("Shift+F12")), "Umschalt+F12");
        assert_eq!(stored_text(combo("Alt+Shift+7")), "Alt+Umschalt+7");
    }

    #[test]
    fn a_grab_is_told_where_it_failed_or_changed_and_not_at_the_start() {
        let hotkey = combo("Strg+Alt+B");
        assert_eq!(told(Grab::Held, hotkey, true), None, "the start says nothing");
        let said = |grab, first| told(grab, hotkey, first).map(|p| p.key);
        assert_eq!(said(Grab::Held, false).as_deref(), Some("ov_hotkey_angemeldet"));
        assert_eq!(said(Grab::Taken, true).as_deref(), Some("ov_hotkey_belegt"), "also at the start");
        assert_eq!(said(Grab::NoKey, false).as_deref(), Some("ov_hotkey_keine_taste"));
        assert_eq!(told(Grab::Taken, hotkey, false), Some(Phrase::new("ov_hotkey_belegt").with("0", "Strg+Alt+B")));
    }
}
