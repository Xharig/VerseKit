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

//! What keyboard, mouse and sticks report, by Raw Input in a thread of its own (Windows): for anti-AFK
//! ([`crate::anti_afk`]) and for the hotkey of the overlay ([`on_keys`]).
//!
//! ⚠ **One Raw Input listener per process.** Windows keeps one registration per process and usage: a second
//! window that registers the keyboard takes it from the first, and a removal takes it from both. So the process
//! has one listener ([`Listener`] and [`on_keys`] share it): a message-only window with `RIDEV_INPUTSINK`, the
//! keyboard (usage page 0x01, usage 0x06) from its start on, the mouse (0x02) and the game devices (0x04, 0x05,
//! 0x08) while anti-AFK follows them. The window loop of winit registers mouse and keyboard for itself; the
//! listener takes both from it. winit then sends no device events (raw mouse motion); the overlay uses none.
//!
//! ⚠ **Why not `GetLastInputInfo`.** Windows counts every report of a game controller as input of the user, and
//! a HOTAS sends reports at rest (two a second each, the same values): the last input is never older than a
//! second. The listener counts instead: a key event; a mouse report with movement, wheel or a button change; a
//! stick whose state moved ([`StickState::moved_from`]). The keys anti-AFK sends itself are not the player's
//! input: they carry [`OWN_MARK`], which Raw Input hands back ([`own_key`]), and they are counted apart.
//!
//! ⛔ **No winmm.** `joyGetPosEx` runs DirectInput inside the program; for empty slots DirectInput searches the
//! devices again (`SetupDiEnumDeviceInterfaces`), and in that path it can overrun its own heap block
//! (`0xc0000374`, no `catch` helps). Raw Input is delivered by the window manager: no device is opened, no driver
//! is asked, DirectInput is never loaded. A report of a stick is read with the HID parser (`HidP_GetData`), which
//! reads only the bytes it is given, against the description of the device (`RIDI_PREPARSEDDATA`, asked once per
//! device).
//!
//! The thread keeps the latest state of every stick ([`StickState`]); anti-AFK looks at it every few seconds and
//! compares two looks. Counted are the axes (usage page 0x01, usages 0x30 to 0x38, and the simulation page 0x02),
//! the hats (0x39) and the buttons (page 0x09). Other pages are left out: some devices send counters or sensor
//! values there that change with every report.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

/// The full range of an axis after [`normalize`]: the range winmm gave (0..65535), so that one threshold fits
/// every device whatever the resolution of its axes.
pub const AXIS_FULL: u32 = 65_535;
/// The usages of usage page 0x01 that are followed: joystick, game pad, multi-axis device.
pub const GAME_USAGES: [u16; 3] = [0x04, 0x05, 0x08];
/// The usages of usage page 0x01 of the mouse and the keyboard.
pub const MOUSE_USAGE: u16 = 0x02;
pub const KEYBOARD_USAGE: u16 = 0x06;
/// The usage page of the buttons.
pub const BUTTON_PAGE: u16 = 0x09;
/// The mark anti-AFK gives the keys it sends (`dwExtraInfo` of `SendInput`); Raw Input hands it back in
/// `ExtraInformation`, which holds 32 bits.
pub const OWN_MARK: u32 = 0x5645_4B41;
/// How long after the start of an own press an injected event of an own key counts as own (milliseconds).
pub const OWN_WINDOW_MS: u64 = 2000;

/// What a control of a device is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An axis: counts from a threshold on.
    Axis,
    /// A hat (point of view): counts with every change.
    Hat,
    /// A button: counts with every change.
    Button,
}

/// One control of a device: its kind, the report it comes in, and for a value its size and range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub kind: Kind,
    /// The report id of the report that carries it (0 for a device without report ids).
    pub report: u8,
    pub bits: u16,
    pub min: i32,
    pub max: i32,
}

/// The controls of one device by their data index (the number the HID parser gives every control).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    controls: BTreeMap<u16, Control>,
}

/// A value capability of a device as the HID parser describes it (`HIDP_VALUE_CAPS`), reduced to what is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueCap {
    pub page: u16,
    pub report: u8,
    /// One cap for several usages in a row (`IsRange`).
    pub range: bool,
    pub first_usage: u16,
    pub first_index: u16,
    pub last_index: u16,
    /// How many values the cap carries; more than one without a range is a value array.
    pub count: u16,
    pub bits: u16,
    pub min: i32,
    pub max: i32,
}

/// A button capability (`HIDP_BUTTON_CAPS`), reduced to what is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonCap {
    pub page: u16,
    pub report: u8,
    pub first_index: u16,
    pub last_index: u16,
}

impl Layout {
    /// Adds the controls of a value capability that count (axes and hats). A value array (several values under
    /// one data index) is left out: its values cannot be told apart.
    pub fn add_values(&mut self, cap: ValueCap) {
        if !cap.range && cap.count > 1 {
            return;
        }
        let last = if cap.range { cap.last_index } else { cap.first_index };
        for (offset, index) in (cap.first_index..=last).enumerate() {
            let usage = u16::try_from(offset).ok().and_then(|offset| cap.first_usage.checked_add(offset));
            let Some(kind) = usage.and_then(|usage| value_kind(cap.page, usage)) else { continue };
            self.controls
                .insert(index, Control { kind, report: cap.report, bits: cap.bits, min: cap.min, max: cap.max });
        }
    }

    /// Adds the buttons of a button capability; other pages (keys, LEDs) do not count.
    pub fn add_buttons(&mut self, cap: ButtonCap) {
        if cap.page != BUTTON_PAGE {
            return;
        }
        for index in cap.first_index..=cap.last_index {
            self.controls.insert(index, Control { kind: Kind::Button, report: cap.report, bits: 1, min: 0, max: 1 });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.controls.is_empty()
    }

    /// The kind of the control with this data index.
    pub fn kind(&self, index: u16) -> Option<Kind> {
        self.controls.get(&index).map(|control| control.kind)
    }
}

/// The kind of a value by its usage; `None` for one that does not count.
pub fn value_kind(page: u16, usage: u16) -> Option<Kind> {
    match (page, usage) {
        (0x01, 0x30..=0x38) | (0x02, _) => Some(Kind::Axis),
        (0x01, 0x39) => Some(Kind::Hat),
        _ => None,
    }
}

/// The value of an axis on the scale `0..=AXIS_FULL`. `raw` holds `bits` bits as the report carries them; a
/// range that starts below zero is read as signed. A range the device states wrongly (the maximum not above the
/// minimum) is taken as the whole unsigned range of the bits. Values outside the range are held at its ends.
pub fn normalize(raw: u32, bits: u16, min: i32, max: i32) -> u32 {
    let bits = u32::from(bits.clamp(1, 32));
    let mask = if bits == 32 { u64::from(u32::MAX) } else { (1u64 << bits) - 1 };
    let unsigned = u64::from(raw) & mask;
    let (value, low, high) = if max > min {
        let negative = min < 0 && (unsigned >> (bits - 1)) & 1 == 1;
        let value = if negative { unsigned as i64 - (mask as i64 + 1) } else { unsigned as i64 };
        (value, i64::from(min), i64::from(max))
    } else {
        (unsigned as i64, 0, mask as i64)
    };
    let scaled = (value.clamp(low, high) - low) * i64::from(AXIS_FULL) / (high - low);
    u32::try_from(scaled).unwrap_or(AXIS_FULL)
}

/// The latest state of one device: every control that came in a report so far, by its data index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StickState {
    values: BTreeMap<u16, (Kind, u32)>,
}

impl StickState {
    /// Takes one report with the id `report`: `data` are the controls the parser found in it (data index, raw
    /// value; a button only while it is down). Every button of this report that is not in `data` is up; a value
    /// that is not in `data` keeps its state. Controls of other reports stay as they are.
    pub fn apply(&mut self, layout: &Layout, report: u8, data: &[(u16, u32)]) {
        for (&index, control) in &layout.controls {
            if control.report != report {
                continue;
            }
            let found = data.iter().find(|(at, _)| *at == index).map(|(_, value)| *value);
            let value = match (control.kind, found) {
                (Kind::Button, found) => u32::from(found.is_some()),
                (Kind::Axis, Some(raw)) => normalize(raw, control.bits, control.min, control.max),
                (Kind::Hat, Some(raw)) => raw,
                (Kind::Axis | Kind::Hat, None) => continue,
            };
            self.values.insert(index, (control.kind, value));
        }
    }

    /// Did something move since `before`? A button or a hat with every change, an axis by `threshold` (on the
    /// scale of [`AXIS_FULL`]) or more. A control that `before` did not know yet counts for nothing.
    pub fn moved_from(&self, before: &StickState, threshold: u32) -> bool {
        self.values.iter().any(|(index, (kind, now))| {
            let Some((old_kind, old)) = before.values.get(index) else { return false };
            if old_kind != kind {
                return false;
            }
            match kind {
                Kind::Axis => now.abs_diff(*old) >= threshold,
                Kind::Hat | Kind::Button => now != old,
            }
        })
    }

    /// How many controls the state knows.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// A look at all devices: device → its latest state.
pub type StickStates = BTreeMap<u64, StickState>;

/// The bytes in front of the device part of a Raw Input packet (`RAWINPUTHEADER`: type, size, device,
/// `wParam`).
pub const HEADER: usize = 8 + 2 * std::mem::size_of::<usize>();
/// `RIM_TYPEMOUSE`, `RIM_TYPEKEYBOARD`, `RIM_TYPEHID`.
pub const TYPE_MOUSE: u32 = 0;
pub const TYPE_KEYBOARD: u32 = 1;
pub const TYPE_HID: u32 = 2;
/// `MOUSE_MOVE_ABSOLUTE`: the report carries a position, not a movement.
pub const MOUSE_ABSOLUTE: u16 = 0x01;

/// The plain number at `at` of a packet, in the byte order of the system that wrote it.
fn read_u32(packet: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(packet.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn read_u16(packet: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(packet.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

/// The type and the device of a Raw Input packet.
pub fn packet_head(packet: &[u8]) -> Option<(u32, u64)> {
    let pointer = std::mem::size_of::<usize>();
    let device = usize::from_ne_bytes(packet.get(8..8 + pointer)?.try_into().ok()?) as u64;
    Some((read_u32(packet, 0)?, device))
}

/// A Raw Input packet of a HID device (`RAWINPUT` as `GetRawInputData` fills it): the device and its reports.
/// `None` for a packet of another type, without reports, or one whose stated sizes do not fit in its bytes.
pub fn hid_reports(packet: &[u8]) -> Option<(u64, Vec<&[u8]>)> {
    let (kind, device) = packet_head(packet)?;
    if kind != TYPE_HID {
        return None;
    }
    let size = usize::try_from(read_u32(packet, HEADER)?).ok()?;
    let count = usize::try_from(read_u32(packet, HEADER + 4)?).ok()?;
    if size == 0 || count == 0 {
        return None;
    }
    let start = HEADER + 8;
    let body = packet.get(start..start.checked_add(size.checked_mul(count)?)?)?;
    Some((device, body.chunks_exact(size).collect()))
}

/// One key event as Raw Input reports it (`RAWKEYBOARD`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyReport {
    /// The device; 0 for an event a program injected.
    pub device: u64,
    pub scan: u16,
    /// `RI_KEY_BREAK` (let go), `RI_KEY_E0`, `RI_KEY_E1`.
    pub flags: u16,
    pub vkey: u16,
    /// What the sender gave along (`dwExtraInfo`).
    pub extra: u32,
}

/// The key event of a keyboard packet; `None` for another type or a cut packet.
pub fn key_report(packet: &[u8]) -> Option<KeyReport> {
    let (kind, device) = packet_head(packet)?;
    if kind != TYPE_KEYBOARD {
        return None;
    }
    Some(KeyReport {
        device,
        scan: read_u16(packet, HEADER)?,
        flags: read_u16(packet, HEADER + 2)?,
        vkey: read_u16(packet, HEADER + 6)?,
        extra: read_u32(packet, HEADER + 12)?,
    })
}

/// One mouse report as Raw Input reports it (`RAWMOUSE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseReport {
    pub device: u64,
    /// [`MOUSE_ABSOLUTE`] and the other `MOUSE_MOVE_*` flags.
    pub flags: u16,
    /// The changes of the buttons and the wheels (`usButtonFlags`); 0 when none changed.
    pub buttons: u16,
    pub x: i32,
    pub y: i32,
    pub extra: u32,
}

/// The report of a mouse packet; `None` for another type or a cut packet.
pub fn mouse_report(packet: &[u8]) -> Option<MouseReport> {
    let (kind, device) = packet_head(packet)?;
    if kind != TYPE_MOUSE {
        return None;
    }
    let signed = |at: usize| read_u32(packet, HEADER + at).map(|value| value as i32);
    Some(MouseReport {
        device,
        flags: read_u16(packet, HEADER)?,
        buttons: read_u16(packet, HEADER + 4)?,
        x: signed(12)?,
        y: signed(16)?,
        extra: read_u32(packet, HEADER + 20)?,
    })
}

/// Is the key event one anti-AFK sent itself? It carries [`OWN_MARK`]; or, where a system does not hand the
/// mark back, it has no device (injected) and is one of the keys of the own press that runs (`pressing`, the
/// scan codes; empty while none runs).
pub fn own_key(report: &KeyReport, pressing: &[u16]) -> bool {
    report.extra == OWN_MARK || (report.device == 0 && pressing.contains(&report.scan))
}

/// `RI_KEY_BREAK`: the key event lets go.
pub const KEY_BREAK: u16 = 0x01;
/// `RI_KEY_E0` and `RI_KEY_E1`: the prefix that tells an extended key from the one with the same scan code.
pub const KEY_PREFIXES: u16 = 0x02 | 0x04;
/// After this long without an event of a key that is down (milliseconds), its next press counts as new: its
/// letting go was missed (the input went to a desktop the listener does not see). A key held by hand repeats
/// many times a second.
pub const STALE_DOWN_MS: u64 = 60_000;

/// Was the mouse moved or used? A button or a wheel; a movement that is not zero; for a device that reports
/// positions ([`MOUSE_ABSOLUTE`]: a tablet, a remote desktop) a position other than its last one (`last`; the
/// first position counts for nothing). A report anti-AFK sent counts for nothing.
pub fn mouse_moved(report: &MouseReport, last: Option<(i32, i32)>) -> bool {
    if report.extra == OWN_MARK {
        return false;
    }
    if report.buttons != 0 {
        return true;
    }
    if report.flags & MOUSE_ABSOLUTE != 0 {
        return last.is_some_and(|at| at != (report.x, report.y));
    }
    report.x != 0 || report.y != 0
}

/// What a real input came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Keyboard,
    /// A key event without a device: a program sent it (macro software, a remote desktop).
    KeyboardInjected,
    Mouse,
    Stick,
}

impl Source {
    const ALL: [Source; 4] = [Source::Keyboard, Source::KeyboardInjected, Source::Mouse, Source::Stick];

    /// The word in the file of anti-AFK.
    pub fn key(self) -> &'static str {
        match self {
            Source::Keyboard => "tastatur",
            Source::KeyboardInjected => "tastatur_eingespeist",
            Source::Mouse => "maus",
            Source::Stick => "joystick",
        }
    }

    pub fn from_key(text: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|source| source.key() == text)
    }

    /// The key of its text for the player.
    pub fn text_key(self) -> &'static str {
        match self {
            Source::Keyboard => "aafk_quelle_tastatur",
            Source::KeyboardInjected => "aafk_quelle_eingespeist",
            Source::Mouse => "aafk_quelle_maus",
            Source::Stick => "aafk_quelle_joystick",
        }
    }

    fn code(self) -> u8 {
        match self {
            Source::Keyboard => 1,
            Source::Mouse => 2,
            Source::Stick => 3,
            Source::KeyboardInjected => 4,
        }
    }

    fn of_code(code: u8) -> Option<Source> {
        Source::ALL.into_iter().find(|source| source.code() == code)
    }
}

/// A key as the clock tells keys apart: the device, the scan code and its prefix.
type KeyId = (u64, u16, u16);

/// When keyboard and mouse were used last, and how many own keys came in. The times are milliseconds since the
/// clock was made; the methods with `_at` take that time (tests), the others take now.
///
/// A key counts when it goes down, not while it is held: the repeats of a held key (and a program that sends
/// the same key again and again without letting go) are no new input. Keys without a device (injected) are
/// kept apart; whether they count is asked with every look ([`InputClock::idle_at`]).
pub struct InputClock {
    start: Instant,
    /// The time of the last real input, plus one (0: none yet).
    last: AtomicU64,
    source: AtomicU8,
    /// The time of the last injected key that went down, plus one (0: none yet).
    last_injected: AtomicU64,
    /// Since when the input is followed: what came before does not count.
    since: AtomicU64,
    /// The own key events that came in.
    own: AtomicU32,
    /// Until when an own press runs, plus one (0: none).
    own_until: AtomicU64,
    /// The scan codes of the own press that runs.
    own_scans: Mutex<Vec<u16>>,
    /// The keys that are down, with the time of their last event.
    down: Mutex<HashMap<KeyId, u64>>,
}

impl Default for InputClock {
    fn default() -> Self {
        InputClock {
            start: Instant::now(),
            last: AtomicU64::new(0),
            source: AtomicU8::new(0),
            last_injected: AtomicU64::new(0),
            since: AtomicU64::new(0),
            own: AtomicU32::new(0),
            own_until: AtomicU64::new(0),
            own_scans: Mutex::new(Vec::new()),
            down: Mutex::new(HashMap::new()),
        }
    }
}

impl InputClock {
    /// Milliseconds since the clock was made.
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The following starts (over) at `at`: input before it does not count, and no key counts as held.
    pub fn restart_at(&self, at: u64) {
        self.since.store(at, Ordering::SeqCst);
        self.down.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    /// An own press of the keys `scans` starts at `at`: injected events of these keys count as own for
    /// [`OWN_WINDOW_MS`].
    pub fn pressing_at(&self, at: u64, scans: &[u16]) {
        *self.own_scans.lock().unwrap_or_else(PoisonError::into_inner) = scans.to_vec();
        self.own_until.store(at.saturating_add(OWN_WINDOW_MS).saturating_add(1), Ordering::SeqCst);
    }

    /// The scan codes of the own press that runs at `at`; empty while none runs.
    fn pressing(&self, at: u64) -> Vec<u16> {
        if at < self.own_until.load(Ordering::SeqCst) {
            self.own_scans.lock().unwrap_or_else(PoisonError::into_inner).clone()
        } else {
            Vec::new()
        }
    }

    /// A key event at `at`: an own one is counted apart; of the others only a key that goes down is input of
    /// the player (a repeat while it is held is none, nor is letting go). Gives whether it counted as input.
    pub fn key_at(&self, report: &KeyReport, at: u64) -> bool {
        if own_key(report, &self.pressing(at)) {
            self.own.fetch_add(1, Ordering::SeqCst);
            return false;
        }
        let id: KeyId = (report.device, report.scan, report.flags & KEY_PREFIXES);
        {
            let mut down = self.down.lock().unwrap_or_else(PoisonError::into_inner);
            if report.flags & KEY_BREAK != 0 {
                down.remove(&id);
                return false;
            }
            let held = down.insert(id, at).is_some_and(|before| at.saturating_sub(before) < STALE_DOWN_MS);
            if held {
                return false;
            }
        }
        if report.device == 0 {
            self.last_injected.store(at.saturating_add(1), Ordering::SeqCst);
        } else {
            self.real_at(Source::Keyboard, at);
        }
        true
    }

    /// A mouse report at `at` that [`mouse_moved`] counted.
    pub fn mouse_at(&self, at: u64) {
        self.real_at(Source::Mouse, at);
    }

    fn real_at(&self, source: Source, at: u64) {
        self.source.store(source.code(), Ordering::SeqCst);
        self.last.store(at.saturating_add(1), Ordering::SeqCst);
    }

    /// Seconds since the last real input at `at`, and what it came from; without input since the following
    /// started, the seconds since then and no source. Injected keys count only with `count_injected`.
    pub fn idle_at(&self, at: u64, count_injected: bool) -> (f64, Option<Source>) {
        let since = self.since.load(Ordering::SeqCst);
        let seconds = |from: u64| at.saturating_sub(from) as f64 / 1000.0;
        let real = self
            .last
            .load(Ordering::SeqCst)
            .checked_sub(1)
            .filter(|when| *when >= since)
            .map(|when| (when, Source::of_code(self.source.load(Ordering::SeqCst))));
        let injected = self
            .last_injected
            .load(Ordering::SeqCst)
            .checked_sub(1)
            .filter(|when| count_injected && *when >= since)
            .map(|when| (when, Some(Source::KeyboardInjected)));
        match real.into_iter().chain(injected).max_by_key(|(when, _)| *when) {
            Some((when, source)) => (seconds(when), source),
            None => (seconds(since), None),
        }
    }

    /// How many own key events came in so far.
    pub fn own_keys(&self) -> u32 {
        self.own.load(Ordering::SeqCst)
    }
}

/// An own press of the keys `scans` starts now: the listener of this process counts injected events of these
/// keys as own for a moment ([`own_key`]). Nothing happens where no listener runs.
pub fn own_press_begins(scans: &[u16]) {
    #[cfg(windows)]
    system::own_press_begins(scans);
    #[cfg(not(windows))]
    let _ = scans;
}

#[cfg(windows)]
pub use system::{Listener, on_keys};

#[cfg(windows)]
mod system {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Duration;

    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_DATA, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, HidP_GetButtonCaps, HidP_GetCaps,
        HidP_GetData, HidP_GetValueCaps, HidP_Input, HidP_MaxDataListLength,
    };
    use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, GetRawInputDeviceInfoW, GetRegisteredRawInputDevices, HRAWINPUT, RAWINPUTDEVICE,
        RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_PREPARSEDDATA, RegisterRawInputDevices,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetMessageW, HWND_MESSAGE, MSG, PostThreadMessageW,
        RegisterClassW, WM_APP, WM_INPUT, WNDCLASSW,
    };

    use super::{
        ButtonCap, GAME_USAGES, InputClock, KEYBOARD_USAGE, KeyReport, Layout, MOUSE_ABSOLUTE, MOUSE_USAGE, Source,
        StickState, StickStates, TYPE_HID, TYPE_KEYBOARD, TYPE_MOUSE, ValueCap, hid_reports, key_report, mouse_moved,
        mouse_report, packet_head,
    };

    /// What the functions of Raw Input give when they fail.
    const FAIL: u32 = u32::MAX;
    /// How long an answer of the thread is waited for at most.
    const ANSWER_WAIT: Duration = Duration::from_secs(2);
    /// The thread message that tells the thread a request is waiting.
    const REQUEST: u32 = WM_APP + 0x41;

    /// One device: its description for the HID parser, its controls, and room for the parser's answers.
    struct Device {
        /// The description (`PHIDP_PREPARSED_DATA`), aligned for the parser.
        described: Vec<u64>,
        layout: Layout,
        data: Vec<HIDP_DATA>,
        scratch: Vec<u8>,
    }

    impl Device {
        fn described(&self) -> isize {
            self.described.as_ptr() as isize
        }

        /// The device behind a Raw Input handle; `None` where its description cannot be read or nothing of it
        /// counts.
        fn of(handle: HANDLE) -> Option<Device> {
            let mut size: u32 = 0;
            // SAFETY: a null buffer only asks for the size in bytes.
            let asked =
                unsafe { GetRawInputDeviceInfoW(handle, RIDI_PREPARSEDDATA, std::ptr::null_mut(), &raw mut size) };
            if asked == FAIL || size == 0 {
                return None;
            }
            let mut described: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
            let mut room = size;
            // SAFETY: the buffer holds at least `size` bytes and lives during the call.
            let got = unsafe {
                GetRawInputDeviceInfoW(handle, RIDI_PREPARSEDDATA, described.as_mut_ptr().cast(), &raw mut room)
            };
            if got == FAIL || got > size {
                return None;
            }
            let mut device = Device { described, layout: Layout::default(), data: Vec::new(), scratch: Vec::new() };
            let parsed = device.described();
            let mut caps = HIDP_CAPS::default();
            // SAFETY: the description was filled above and lives in `device`; `caps` lives during the call.
            if unsafe { HidP_GetCaps(parsed, &raw mut caps) } != HIDP_STATUS_SUCCESS
                || caps.UsagePage != 0x01
                || !GAME_USAGES.contains(&caps.Usage)
            {
                return None;
            }
            let mut count = caps.NumberInputValueCaps;
            let mut values = vec![HIDP_VALUE_CAPS::default(); usize::from(count)];
            // SAFETY: the list holds `count` entries; the description lives in `device`.
            if count > 0
                && unsafe { HidP_GetValueCaps(HidP_Input, values.as_mut_ptr(), &raw mut count, parsed) }
                    == HIDP_STATUS_SUCCESS
            {
                values.truncate(usize::from(count));
                for cap in &values {
                    device.layout.add_values(value_cap(cap));
                }
            }
            let mut count = caps.NumberInputButtonCaps;
            let mut buttons = vec![HIDP_BUTTON_CAPS::default(); usize::from(count)];
            // SAFETY: as above.
            if count > 0
                && unsafe { HidP_GetButtonCaps(HidP_Input, buttons.as_mut_ptr(), &raw mut count, parsed) }
                    == HIDP_STATUS_SUCCESS
            {
                buttons.truncate(usize::from(count));
                for cap in &buttons {
                    device.layout.add_buttons(button_cap(cap));
                }
            }
            if device.layout.is_empty() {
                return None;
            }
            // SAFETY: the description lives in `device`.
            let longest = unsafe { HidP_MaxDataListLength(HidP_Input, parsed) };
            device.data = vec![HIDP_DATA::default(); longest as usize];
            Some(device)
        }

        /// The controls one report carries: (data index, raw value; a button only while it is down).
        fn read(&mut self, report: &[u8]) -> Option<Vec<(u16, u32)>> {
            self.scratch.clear();
            self.scratch.extend_from_slice(report);
            let mut length = u32::try_from(self.data.len()).ok()?;
            let parsed = self.described();
            // SAFETY: the list holds `length` entries, the report copy is as long as stated, the description
            // lives in `self`; the parser only reads the report.
            let status = unsafe {
                HidP_GetData(
                    HidP_Input,
                    self.data.as_mut_ptr(),
                    &raw mut length,
                    parsed,
                    self.scratch.as_mut_ptr(),
                    u32::try_from(self.scratch.len()).ok()?,
                )
            };
            if status != HIDP_STATUS_SUCCESS {
                return None;
            }
            let found = self.data.get(..length as usize)?;
            Some(
                found
                    .iter()
                    .filter_map(|item| match self.layout.kind(item.DataIndex)? {
                        super::Kind::Button => Some((item.DataIndex, 1)),
                        // SAFETY: for a value the parser fills the raw value of the union.
                        super::Kind::Axis | super::Kind::Hat => {
                            Some((item.DataIndex, unsafe { item.Anonymous.RawValue }))
                        }
                    })
                    .collect(),
            )
        }
    }

    fn value_cap(cap: &HIDP_VALUE_CAPS) -> ValueCap {
        // SAFETY: both parts of the union are plain numbers; `IsRange` says which one the parser filled.
        let (first_usage, first_index, last_index) = unsafe {
            if cap.IsRange {
                (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.DataIndexMin, cap.Anonymous.Range.DataIndexMax)
            } else {
                (cap.Anonymous.NotRange.Usage, cap.Anonymous.NotRange.DataIndex, cap.Anonymous.NotRange.DataIndex)
            }
        };
        ValueCap {
            page: cap.UsagePage,
            report: cap.ReportID,
            range: cap.IsRange,
            first_usage,
            first_index,
            last_index,
            count: cap.ReportCount,
            bits: cap.BitSize,
            min: cap.LogicalMin,
            max: cap.LogicalMax,
        }
    }

    fn button_cap(cap: &HIDP_BUTTON_CAPS) -> ButtonCap {
        // SAFETY: as in `value_cap`.
        let (first_index, last_index) = unsafe {
            if cap.IsRange {
                (cap.Anonymous.Range.DataIndexMin, cap.Anonymous.Range.DataIndexMax)
            } else {
                (cap.Anonymous.NotRange.DataIndex, cap.Anonymous.NotRange.DataIndex)
            }
        };
        ButtonCap { page: cap.UsagePage, report: cap.ReportID, first_index, last_index }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// A message-only window of this thread: nobody sees it, it only takes the Raw Input packets.
    fn message_window() -> Option<HWND> {
        let class = wide("VerseKitRawInput");
        // SAFETY: the class name lives for the calls; a class that exists already makes the registration fail
        // harmlessly, the window is made all the same.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            RegisterClassW(&raw const wc);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            (!hwnd.is_null()).then_some(hwnd)
        }
    }

    /// Registers `usages` of usage page 0x01 for `window` (also while another program is in front), or takes
    /// their registration of this process away (`window` = `None`).
    fn register(usages: &[u16], window: Option<HWND>) -> bool {
        let devices: Vec<RAWINPUTDEVICE> = usages
            .iter()
            .map(|&usage| RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: usage,
                dwFlags: if window.is_some() { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
                hwndTarget: window.unwrap_or(std::ptr::null_mut()),
            })
            .collect();
        // SAFETY: the list lives during the call and its size is that of one element.
        unsafe {
            RegisterRawInputDevices(
                devices.as_ptr(),
                devices.len() as u32,
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            ) != 0
        }
    }

    /// The usages followed while anti-AFK follows: the mouse and the game devices.
    fn followed() -> Vec<u16> {
        std::iter::once(MOUSE_USAGE).chain(GAME_USAGES).collect()
    }

    /// The window the registration of this process sends `usage` (page 0x01) to; `None` where it is not
    /// registered or the list cannot be read.
    fn target_of(usage: u16) -> Option<isize> {
        let size = std::mem::size_of::<RAWINPUTDEVICE>() as u32;
        let mut count: u32 = 0;
        // SAFETY: a null list only asks for the number of registrations.
        unsafe { GetRegisteredRawInputDevices(std::ptr::null_mut(), &raw mut count, size) };
        if count == 0 {
            return None;
        }
        let mut list = vec![RAWINPUTDEVICE::default(); count as usize];
        // SAFETY: the list holds `count` entries and lives during the call.
        let got = unsafe { GetRegisteredRawInputDevices(list.as_mut_ptr(), &raw mut count, size) };
        if got == FAIL {
            return None;
        }
        list.iter()
            .take(got as usize)
            .find(|device| device.usUsagePage == 0x01 && device.usUsage == usage)
            .map(|device| device.hwndTarget as isize)
    }

    /// The packet behind a `WM_INPUT`, read into `buffer`.
    fn packet(lparam: LPARAM, buffer: &mut Vec<u64>) -> Option<&[u8]> {
        let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        let handle = lparam as HRAWINPUT;
        let mut size: u32 = 0;
        // SAFETY: a null buffer only asks for the size in bytes.
        if unsafe { GetRawInputData(handle, RID_INPUT, std::ptr::null_mut(), &raw mut size, header) } == FAIL {
            return None;
        }
        buffer.clear();
        buffer.resize((size as usize).div_ceil(8).max(1), 0);
        let mut room = size;
        // SAFETY: the buffer holds at least `size` bytes and lives during the call.
        let read = unsafe { GetRawInputData(handle, RID_INPUT, buffer.as_mut_ptr().cast(), &raw mut room, header) };
        if read == FAIL || read > size {
            return None;
        }
        // SAFETY: the buffer holds `buffer.len() * 8` bytes, `read` of them were written above.
        Some(unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), read as usize) })
    }

    /// A function that gets every key event.
    type KeyCallback = Box<dyn Fn(&KeyReport) + Send + Sync>;

    /// What the thread and the callers share.
    struct Shared {
        states: Mutex<StickStates>,
        clock: InputClock,
        keys: Mutex<Vec<KeyCallback>>,
        /// Does the thread run?
        alive: AtomicBool,
        /// Does a [`Listener`] hold the following (mouse and sticks)?
        following: AtomicBool,
    }

    /// What the thread keeps for itself.
    #[derive(Default)]
    struct Local {
        devices: HashMap<u64, Option<Device>>,
        /// The last position of every mouse that reports positions.
        positions: HashMap<u64, (i32, i32)>,
    }

    /// The one listener of the process: its thread, its window, and the way to ask it.
    struct Hub {
        thread: u32,
        window: isize,
        shared: Arc<Shared>,
        /// Requests to follow (`true`) or not; the thread answers whether Windows took the registration.
        requests: Mutex<Sender<(bool, Sender<bool>)>>,
    }

    static HUB: Mutex<Option<Arc<Hub>>> = Mutex::new(None);

    /// The listener of this process, started where it does not run yet; `None` where the window cannot be made
    /// or Windows refuses the keyboard.
    fn hub() -> Option<Arc<Hub>> {
        let mut slot = HUB.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(hub) = slot.as_ref().filter(|hub| hub.shared.alive.load(Ordering::SeqCst)) {
            return Some(Arc::clone(hub));
        }
        let hub = Arc::new(Hub::start()?);
        *slot = Some(Arc::clone(&hub));
        Some(hub)
    }

    impl Hub {
        fn start() -> Option<Hub> {
            let shared = Arc::new(Shared {
                states: Mutex::new(StickStates::new()),
                clock: InputClock::default(),
                keys: Mutex::new(Vec::new()),
                alive: AtomicBool::new(true),
                following: AtomicBool::new(false),
            });
            let (requests, incoming) = mpsc::channel();
            let (started, answer) = mpsc::channel();
            let inside = Arc::clone(&shared);
            let spawned = std::thread::Builder::new().name("versekit-rawinput".into()).spawn(move || {
                serve(&inside, &incoming, &started);
                inside.alive.store(false, Ordering::SeqCst);
            });
            let Ok(handle) = spawned else { return None };
            match answer.recv() {
                Ok(Some((thread, window))) => {
                    drop(handle);
                    Some(Hub { thread, window, shared, requests: Mutex::new(requests) })
                }
                _ => {
                    let _ = handle.join();
                    None
                }
            }
        }

        /// Asks the thread to follow mouse and sticks (`true`) or to let them go; gives whether Windows took it.
        fn request(&self, follow: bool) -> bool {
            let (tell, answer) = mpsc::channel();
            if self.requests.lock().unwrap_or_else(PoisonError::into_inner).send((follow, tell)).is_err() {
                return false;
            }
            // SAFETY: posts a plain message to the thread of the listener; a thread that is gone just fails.
            if unsafe { PostThreadMessageW(self.thread, REQUEST, 0, 0) } == 0 {
                return false;
            }
            answer.recv_timeout(ANSWER_WAIT).unwrap_or(false)
        }
    }

    /// The thread: window, keyboard, then the messages for the life of the process.
    fn serve(shared: &Shared, requests: &Receiver<(bool, Sender<bool>)>, started: &Sender<Option<(u32, isize)>>) {
        let Some(window) = message_window() else {
            let _ = started.send(None);
            return;
        };
        if !register(&[KEYBOARD_USAGE], Some(window)) {
            // SAFETY: the window was made by this thread above.
            unsafe { DestroyWindow(window) };
            let _ = started.send(None);
            return;
        }
        // SAFETY: a call without arguments.
        let _ = started.send(Some((unsafe { GetCurrentThreadId() }, window as isize)));
        let mut local = Local::default();
        let mut buffer: Vec<u64> = Vec::new();
        // SAFETY: an empty message the loop fills.
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: takes the messages of this thread (its window's and its own); `message` lives during the calls.
        while unsafe { GetMessageW(&raw mut message, std::ptr::null_mut(), 0, 0) } > 0 {
            if message.hwnd.is_null() {
                if message.message == REQUEST {
                    while let Ok((follow, answer)) = requests.try_recv() {
                        let _ = answer.send(switch(shared, &mut local, window, follow));
                    }
                }
                continue;
            }
            if message.message == WM_INPUT
                && let Some(bytes) = packet(message.lParam, &mut buffer)
            {
                take(bytes, shared, &mut local);
            }
            // SAFETY: the message as received; for WM_INPUT the system cleans up behind it.
            unsafe { DefWindowProcW(message.hwnd, message.message, message.wParam, message.lParam) };
        }
        let mut all = followed();
        all.push(KEYBOARD_USAGE);
        register(&all, None);
        // SAFETY: the window was made by this thread above.
        unsafe { DestroyWindow(window) };
    }

    /// Follows mouse and sticks, or lets them go. A following starts empty: no stick, no input of before.
    fn switch(shared: &Shared, local: &mut Local, window: HWND, follow: bool) -> bool {
        if !follow {
            return register(&followed(), None);
        }
        shared.states.lock().unwrap_or_else(PoisonError::into_inner).clear();
        local.positions.clear();
        shared.clock.restart_at(shared.clock.now_ms());
        register(&followed(), Some(window))
    }

    /// Takes one packet: a key event goes to the clock and to the callbacks; mouse and sticks count only while
    /// they are followed.
    fn take(packet: &[u8], shared: &Shared, local: &mut Local) {
        let Some((kind, handle)) = packet_head(packet) else { return };
        let following = shared.following.load(Ordering::SeqCst);
        match kind {
            TYPE_KEYBOARD => {
                let Some(report) = key_report(packet) else { return };
                shared.clock.key_at(&report, shared.clock.now_ms());
                for callback in shared.keys.lock().unwrap_or_else(PoisonError::into_inner).iter() {
                    callback(&report);
                }
            }
            TYPE_MOUSE if following => {
                let Some(report) = mouse_report(packet) else { return };
                let last = local.positions.get(&report.device).copied();
                if report.flags & MOUSE_ABSOLUTE != 0 {
                    local.positions.insert(report.device, (report.x, report.y));
                }
                if mouse_moved(&report, last) {
                    shared.clock.mouse_at(shared.clock.now_ms());
                }
            }
            TYPE_HID if following => {
                let Some((_, reports)) = hid_reports(packet) else { return };
                let device = local.devices.entry(handle).or_insert_with(|| Device::of(handle as usize as HANDLE));
                let Some(device) = device else { return };
                let read: Vec<(u8, Vec<(u16, u32)>)> =
                    reports.iter().filter_map(|report| Some((*report.first()?, device.read(report)?))).collect();
                if read.is_empty() {
                    return;
                }
                let mut states = shared.states.lock().unwrap_or_else(PoisonError::into_inner);
                let state: &mut StickState = states.entry(handle).or_default();
                for (id, data) in &read {
                    state.apply(&device.layout, *id, data);
                }
            }
            _ => {}
        }
    }

    /// Calls `callback` with every key event of this process's listener (on its thread; it must be quick), from
    /// now on for the life of the process. `false` where the listener cannot run.
    pub fn on_keys(callback: impl Fn(&KeyReport) + Send + Sync + 'static) -> bool {
        let Some(hub) = hub() else { return false };
        hub.shared.keys.lock().unwrap_or_else(PoisonError::into_inner).push(Box::new(callback));
        true
    }

    /// See [`super::own_press_begins`].
    pub fn own_press_begins(scans: &[u16]) {
        let hub = HUB.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if let Some(hub) = hub {
            hub.shared.clock.pressing_at(hub.shared.clock.now_ms(), scans);
        }
    }

    /// Keyboard, mouse and sticks followed while the value lives (the listener of the process follows mouse and
    /// sticks for it). Dropping it lets mouse and sticks go; the keyboard stays with the listener.
    pub struct Listener {
        hub: Arc<Hub>,
    }

    impl Listener {
        /// Starts following; `None` where the listener cannot run, Windows refuses the registration, or another
        /// value follows already (the registration is one per process).
        pub fn start() -> Option<Listener> {
            let hub = hub()?;
            if hub.shared.following.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                return None;
            }
            if hub.request(true) {
                return Some(Listener { hub });
            }
            hub.request(false);
            hub.shared.following.store(false, Ordering::SeqCst);
            None
        }

        /// The latest state of every stick that sent something since the following started.
        pub fn states(&self) -> StickStates {
            self.hub.shared.states.lock().unwrap_or_else(PoisonError::into_inner).clone()
        }

        /// Does the listener still run?
        pub fn running(&self) -> bool {
            self.hub.shared.alive.load(Ordering::SeqCst)
        }

        /// Seconds since the last real input of keyboard or mouse, and which; without one since the start of
        /// the following the seconds since then and no source. Injected keys count only with `count_injected`.
        pub fn input_idle(&self, count_injected: bool) -> (f64, Option<Source>) {
            let clock = &self.hub.shared.clock;
            clock.idle_at(clock.now_ms(), count_injected)
        }

        /// How many own key events came in.
        pub fn own_keys(&self) -> u32 {
            self.hub.shared.clock.own_keys()
        }

        /// Does the keyboard of this process go to the listener (and with it every key anti-AFK sends)?
        pub fn keyboard_ours(&self) -> bool {
            target_of(KEYBOARD_USAGE) == Some(self.hub.window)
        }
    }

    impl Drop for Listener {
        /// Lets mouse and sticks go and waits for it: a new value must not register before this one took its
        /// registration away.
        fn drop(&mut self) {
            self.hub.request(false);
            self.hub.shared.following.store(false, Ordering::SeqCst);
        }
    }

    #[cfg(test)]
    mod tests {
        use windows_sys::Win32::UI::Input::{RAWHID, RAWINPUT, RAWINPUTHEADER, RAWKEYBOARD, RAWMOUSE};

        use super::Listener;

        #[test]
        fn the_packet_is_read_with_the_layout_the_system_uses() {
            assert_eq!(std::mem::offset_of!(RAWINPUT, data), super::super::HEADER);
            assert_eq!(std::mem::size_of::<RAWINPUTHEADER>(), super::super::HEADER);
            assert_eq!(std::mem::offset_of!(RAWHID, bRawData), 8);
            assert_eq!(
                [
                    std::mem::offset_of!(RAWKEYBOARD, MakeCode),
                    std::mem::offset_of!(RAWKEYBOARD, Flags),
                    std::mem::offset_of!(RAWKEYBOARD, VKey),
                    std::mem::offset_of!(RAWKEYBOARD, ExtraInformation),
                ],
                [0, 2, 6, 12]
            );
            assert_eq!(
                [
                    std::mem::offset_of!(RAWMOUSE, usFlags),
                    std::mem::offset_of!(RAWMOUSE, Anonymous),
                    std::mem::offset_of!(RAWMOUSE, lLastX),
                    std::mem::offset_of!(RAWMOUSE, lLastY),
                    std::mem::offset_of!(RAWMOUSE, ulExtraInformation),
                ],
                [0, 4, 12, 16, 20]
            );
        }

        #[test]
        fn one_listener_at_a_time_follows_and_the_keyboard_goes_to_the_listener() {
            // Passive: only the registration of this test process; nothing is sent, no window can be seen.
            let _alone = super::super::exclusive();
            let listener = Listener::start().expect("Raw Input for keyboard, mouse and the game devices");
            assert!(listener.running());
            assert!(Listener::start().is_none(), "a second one would take the registration of the first");
            assert!(listener.keyboard_ours(), "the keys anti-AFK sends come to the listener");
            assert_eq!(super::target_of(super::MOUSE_USAGE), Some(listener.hub.window), "the mouse is followed");
            let _ = listener.states();
            let (seconds, _) = listener.input_idle(true);
            assert!(seconds >= 0.0);
            drop(listener);
            assert_eq!(super::target_of(super::MOUSE_USAGE), None, "the mouse is let go");
            // After the first: its registration was taken away, a new one can be made.
            let again = Listener::start().expect("again");
            assert!(again.running() && again.keyboard_ours());
        }

        /// By hand, on the desktop that gets the input: `cargo test -p versekit-core --lib --
        /// stick_input::system::tests::show --ignored --nocapture` follows for fifteen seconds and says every
        /// second per stick how many controls it knows and whether it moved since the last look, and how long
        /// keyboard and mouse have been idle. Passive: nothing is sent.
        #[test]
        #[ignore = "needs the desktop that gets the input"]
        fn show_what_the_inputs_report() {
            let _alone = super::super::exclusive();
            let listener = Listener::start().expect("Raw Input");
            let mut before = listener.states();
            for second in 1..=15 {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let now = listener.states();
                let line: Vec<String> = now
                    .iter()
                    .map(|(device, state)| {
                        let moved = before.get(device).is_some_and(|old| state.moved_from(old, 1000));
                        format!("{device:x}: {} controls, moved {moved}", state.len())
                    })
                    .collect();
                let (idle, source) = listener.input_idle(true);
                println!(
                    "{second:2}s keyboard/mouse idle {idle:.1}s ({source:?}), own {} | {} sticks | {}",
                    listener.own_keys(),
                    now.len(),
                    line.join(" | ")
                );
                before = now;
            }
        }
    }
}

/// Under test: the listeners of the tests of this crate one after the other (one per process).
#[cfg(all(test, windows))]
pub(crate) fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake parser of the tests: a report is `[id, x low, x high, y low, y high, buttons]` (16-bit axes and
    /// eight buttons); it gives what the HID parser gives (data index, raw value, buttons only while down).
    fn parse(report: &[u8]) -> (u8, Vec<(u16, u32)>) {
        let axis = |at: usize| u32::from(u16::from_le_bytes([report[at], report[at + 1]]));
        let mut data = vec![(0, axis(1)), (1, axis(3))];
        for bit in 0..8u16 {
            if report[5] & (1 << bit) != 0 {
                data.push((2 + bit, 1));
            }
        }
        (report[0], data)
    }

    /// The controls the fake parser speaks of, as the system describes a stick with two 16-bit axes and eight
    /// buttons in report 0.
    fn layout() -> Layout {
        let mut layout = Layout::default();
        layout.add_values(ValueCap {
            page: 0x01,
            report: 0,
            range: true,
            first_usage: 0x30,
            first_index: 0,
            last_index: 1,
            count: 2,
            bits: 16,
            min: 0,
            max: 65_535,
        });
        layout.add_buttons(ButtonCap { page: BUTTON_PAGE, report: 0, first_index: 2, last_index: 9 });
        layout
    }

    /// The header of a Raw Input packet of `kind` from `device`.
    fn head(kind: u32, device: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&kind.to_ne_bytes());
        out.extend_from_slice(&0u32.to_ne_bytes());
        out.extend_from_slice(&device.to_ne_bytes());
        out.extend_from_slice(&0usize.to_ne_bytes());
        out
    }

    /// A Raw Input packet of device `device` with these reports, laid out as `GetRawInputData` gives it.
    fn packet(device: usize, reports: &[[u8; 6]]) -> Vec<u8> {
        let mut out = head(TYPE_HID, device);
        out.extend_from_slice(&6u32.to_ne_bytes());
        out.extend_from_slice(&(reports.len() as u32).to_ne_bytes());
        for report in reports {
            out.extend_from_slice(report);
        }
        out
    }

    /// A keyboard packet (`RAWKEYBOARD`: make code, flags, reserved, virtual key, message, extra).
    fn key_packet(device: usize, scan: u16, flags: u16, vkey: u16, extra: u32) -> Vec<u8> {
        let mut out = head(TYPE_KEYBOARD, device);
        for half in [scan, flags, 0, vkey] {
            out.extend_from_slice(&half.to_ne_bytes());
        }
        out.extend_from_slice(&0x0100u32.to_ne_bytes());
        out.extend_from_slice(&extra.to_ne_bytes());
        out
    }

    /// A mouse packet (`RAWMOUSE`: flags, padding, button flags and data, raw buttons, x, y, extra).
    fn mouse_packet(device: usize, flags: u16, buttons: u16, x: i32, y: i32, extra: u32) -> Vec<u8> {
        let mut out = head(TYPE_MOUSE, device);
        for half in [flags, 0, buttons, 0] {
            out.extend_from_slice(&half.to_ne_bytes());
        }
        out.extend_from_slice(&0u32.to_ne_bytes());
        out.extend_from_slice(&x.to_ne_bytes());
        out.extend_from_slice(&y.to_ne_bytes());
        out.extend_from_slice(&extra.to_ne_bytes());
        out
    }

    fn report(x: u16, y: u16, buttons: u8) -> [u8; 6] {
        let (x, y) = (x.to_le_bytes(), y.to_le_bytes());
        [0, x[0], x[1], y[0], y[1], buttons]
    }

    /// The states after feeding `packets` in order, as the thread does.
    fn feed(states: &mut StickStates, packets: &[Vec<u8>]) {
        let layout = layout();
        for bytes in packets {
            let (device, reports) = hid_reports(bytes).expect("a HID packet");
            let state = states.entry(device).or_default();
            for one in reports {
                let (id, data) = parse(one);
                state.apply(&layout, id, &data);
            }
        }
    }

    const THRESHOLD: u32 = 1000;

    fn moved(before: &StickStates, after: &StickStates) -> bool {
        after.iter().any(|(device, now)| before.get(device).is_some_and(|old| now.moved_from(old, THRESHOLD)))
    }

    #[test]
    fn a_packet_with_an_axis_that_moves_counts_and_one_that_only_shakes_does_not() {
        let mut states = StickStates::new();
        feed(&mut states, &[packet(7, &[report(32_768, 32_768, 0)])]);
        let rest = states.clone();
        // Shaking at rest: many reports a few places off, the last one 999 off.
        feed(&mut states, &[packet(7, &[report(32_770, 32_766, 0), report(32_760, 32_775, 0)])]);
        feed(&mut states, &[packet(7, &[report(32_768 + 999, 32_768 - 999, 0)])]);
        assert!(!moved(&rest, &states), "noise is no input");
        // The stick is moved: the axis 1000 places (1.5 % of its way).
        feed(&mut states, &[packet(7, &[report(32_768 + 1000, 32_768, 0)])]);
        assert!(moved(&rest, &states), "an axis from the threshold on");
        // A button that is held, and one let go again, both count.
        let still = states.clone();
        feed(&mut states, &[packet(7, &[report(32_768 + 1000, 32_768, 0b100)])]);
        assert!(moved(&still, &states), "a button");
        let held = states.clone();
        feed(&mut states, &[packet(7, &[report(32_768 + 1000, 32_768, 0)])]);
        assert!(moved(&held, &states), "a button let go");
    }

    #[test]
    fn the_reports_a_stick_sends_at_rest_are_no_input() {
        // A HOTAS at rest: two reports a second with the same values, for a minute, on three devices.
        let mut states = StickStates::new();
        let rest = |device: usize| packet(device, &[report(31_000 + device as u16, 33_000, 0)]);
        feed(&mut states, &[rest(1), rest(2), rest(3)]);
        let first = states.clone();
        for _ in 0..120 {
            feed(&mut states, &[rest(1), rest(2), rest(3)]);
        }
        assert_eq!(states, first, "the same values leave the same state");
        assert!(!moved(&first, &states), "a report without change is no input");
        // One hand on one of them: that is input.
        feed(&mut states, &[packet(2, &[report(31_002, 33_000, 0b1)])]);
        assert!(moved(&first, &states));
    }

    #[test]
    fn the_last_report_of_a_packet_counts_and_a_new_device_counts_for_nothing() {
        let mut states = StickStates::new();
        feed(&mut states, &[packet(7, &[report(0, 0, 0)])]);
        let before = states.clone();
        // Two reports in one packet: the first moved, the second came back. The state is the last one.
        feed(&mut states, &[packet(7, &[report(60_000, 0, 0), report(0, 0, 0)])]);
        assert!(!moved(&before, &states), "back where it was");
        // A device that sent nothing before is no movement, however far its axes stand.
        feed(&mut states, &[packet(9, &[report(60_000, 60_000, 0xFF)])]);
        assert!(!moved(&before, &states));
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn a_packet_whose_sizes_do_not_fit_is_not_read() {
        let good = packet(7, &[report(1, 2, 3), report(4, 5, 6)]);
        assert_eq!(hid_reports(&good).map(|(device, reports)| (device, reports.len())), Some((7, 2)));
        // One byte short: the second report would lie outside the packet.
        assert!(hid_reports(&good[..good.len() - 1]).is_none());
        // A count that overflows the size.
        let mut huge = good.clone();
        huge[HEADER + 4..HEADER + 8].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(hid_reports(&huge).is_none());
        // Another type (a keyboard), no reports, a cut header.
        let mut keyboard = good.clone();
        keyboard[..4].copy_from_slice(&1u32.to_ne_bytes());
        assert!(hid_reports(&keyboard).is_none());
        assert!(hid_reports(&packet(7, &[])).is_none());
        assert!(hid_reports(&good[..HEADER]).is_none());
    }

    #[test]
    fn keyboard_and_mouse_packets_are_read_as_the_system_lays_them_out() {
        let key = key_packet(5, 0x2D, 1, 0x58, OWN_MARK);
        assert_eq!(key_report(&key), Some(KeyReport { device: 5, scan: 0x2D, flags: 1, vkey: 0x58, extra: OWN_MARK }));
        assert!(key_report(&key[..key.len() - 1]).is_none(), "a cut packet");
        assert!(mouse_report(&key).is_none() && hid_reports(&key).is_none(), "a keyboard is no mouse and no stick");
        let mouse = mouse_packet(6, 0, 0x0400, -3, 4, 0);
        assert_eq!(
            mouse_report(&mouse),
            Some(MouseReport { device: 6, flags: 0, buttons: 0x0400, x: -3, y: 4, extra: 0 })
        );
        assert!(mouse_report(&mouse[..mouse.len() - 1]).is_none());
        assert!(key_report(&mouse).is_none());
    }

    #[test]
    fn a_mouse_counts_when_it_moves_or_clicks_and_not_when_it_reports_nothing() {
        let at = |flags, buttons, x, y| MouseReport { device: 1, flags, buttons, x, y, extra: 0 };
        assert!(!mouse_moved(&at(0, 0, 0, 0), None), "a report without movement");
        assert!(mouse_moved(&at(0, 0, 1, 0), None), "a movement");
        assert!(mouse_moved(&at(0, 0, 0, -1), None));
        assert!(mouse_moved(&at(0, 0x0001, 0, 0), None), "a button down");
        assert!(mouse_moved(&at(0, 0x0400, 0, 0), None), "the wheel");
        // Positions: only a change counts, the first one does not.
        assert!(!mouse_moved(&at(MOUSE_ABSOLUTE, 0, 500, 500), None));
        assert!(!mouse_moved(&at(MOUSE_ABSOLUTE, 0, 500, 500), Some((500, 500))));
        assert!(mouse_moved(&at(MOUSE_ABSOLUTE, 0, 501, 500), Some((500, 500))));
        assert!(!mouse_moved(&MouseReport { extra: OWN_MARK, ..at(0, 0, 9, 9) }, None), "an own one");
    }

    #[test]
    fn a_key_of_the_player_resets_the_wait_and_an_own_key_does_not() {
        let clock = InputClock::default();
        clock.restart_at(1000);
        assert_eq!(clock.idle_at(5000, true), (4.0, None), "nothing since the start of the following");
        // A key of the player: the wait starts over, with the keyboard as the source.
        let player = KeyReport { device: 0x1234, scan: 0x2D, flags: 0, vkey: 0x58, extra: 0 };
        assert!(clock.key_at(&player, 6000));
        assert_eq!(clock.idle_at(9000, true), (3.0, Some(Source::Keyboard)));
        // The own press: the mark, and (where the mark does not come back) an injected own key during the press.
        let marked = KeyReport { extra: OWN_MARK, ..player };
        assert!(!clock.key_at(&marked, 10_000));
        let own = [0x38, 0x25];
        clock.pressing_at(11_000, &own);
        let injected = KeyReport { device: 0, scan: 0x25, flags: 0, vkey: 0x4B, extra: 0 };
        assert!(!clock.key_at(&injected, 11_500));
        assert_eq!(clock.own_keys(), 2, "both are counted as own");
        assert_eq!(clock.idle_at(12_000, true), (6.0, Some(Source::Keyboard)), "and the wait goes on");
        // After the press an injected key of another program is input (a macro of his HOTAS software), kept
        // apart from the keyboard.
        assert!(clock.key_at(&injected, 11_000 + OWN_WINDOW_MS + 1));
        assert_eq!(clock.idle_at(14_000, true), (0.999, Some(Source::KeyboardInjected)));
        assert_eq!(clock.idle_at(14_000, false), (8.0, Some(Source::Keyboard)), "unless injected keys do not count");
        // Another key injected during the press is not one of the own keys.
        clock.pressing_at(20_000, &own);
        assert!(clock.key_at(&KeyReport { scan: 0x11, ..injected }, 20_100));
        // A mouse that moved.
        clock.mouse_at(30_000);
        assert_eq!(clock.idle_at(30_500, true), (0.5, Some(Source::Mouse)));
        // A new following forgets what came before.
        clock.restart_at(40_000);
        assert_eq!(clock.idle_at(41_000, true), (1.0, None));
    }

    /// A key held down for 400 seconds (or sent again every second without letting go): only its first going
    /// down is input, the wait grows. Letting go and pressing again is new input.
    #[test]
    fn a_key_held_down_is_one_input_and_not_one_per_repeat() {
        let clock = InputClock::default();
        clock.restart_at(0);
        let down = KeyReport { device: 0x1234, scan: 0x25, flags: 0, vkey: 0x4B, extra: 0 };
        for second in 0..400u64 {
            let counted = clock.key_at(&down, second * 1000);
            assert_eq!(counted, second == 0, "second {second}");
        }
        assert_eq!(clock.idle_at(400_000, true), (400.0, Some(Source::Keyboard)), "the wait grew");
        // Let go: no input; down again: input.
        assert!(!clock.key_at(&KeyReport { flags: KEY_BREAK, ..down }, 400_500));
        assert_eq!(clock.idle_at(401_000, true).0, 401.0);
        assert!(clock.key_at(&down, 401_000));
        assert_eq!(clock.idle_at(401_000, true).0, 0.0);
        // The same scan code with the prefix is another key (Right Alt is not Left Alt), another device too.
        assert!(clock.key_at(&KeyReport { flags: 0x02, ..down }, 402_000));
        let other = KeyReport { device: 0x9999, ..down };
        assert!(clock.key_at(&other, 403_000));
        // A key whose letting go was missed counts again after a minute without any event of it.
        assert!(!clock.key_at(&other, 403_000 + STALE_DOWN_MS - 1));
        assert!(clock.key_at(&other, 403_000 + 2 * STALE_DOWN_MS));
        // A new following forgets the keys that were down.
        clock.restart_at(600_000);
        assert!(clock.key_at(&down, 600_001));
    }

    /// The same for a program that sends one key every second without letting go: kept apart as injected, and
    /// its repeats are no input either.
    #[test]
    fn an_injected_key_held_down_is_one_input_too() {
        let clock = InputClock::default();
        clock.restart_at(0);
        let injected = KeyReport { device: 0, scan: 0x25, flags: 0, vkey: 0x4B, extra: 0 };
        for second in 0..400u64 {
            clock.key_at(&injected, second * 1000);
        }
        assert_eq!(clock.idle_at(400_000, true), (400.0, Some(Source::KeyboardInjected)));
        assert_eq!(clock.idle_at(400_000, false), (400.0, None), "not counted: nothing since the start");
    }

    #[test]
    fn the_source_has_its_word_in_the_file_and_back() {
        for source in [Source::Keyboard, Source::KeyboardInjected, Source::Mouse, Source::Stick] {
            assert_eq!(Source::from_key(source.key()), Some(source));
            assert_eq!(Source::of_code(source.code()), Some(source));
        }
        assert_eq!(Source::from_key("tastatur"), Some(Source::Keyboard));
        assert_eq!(Source::from_key("Tastatur"), None);
        assert_eq!(Source::of_code(0), None);
    }

    #[test]
    fn every_resolution_and_sign_lands_on_the_same_scale() {
        assert_eq!(normalize(0, 16, 0, 65_535), 0);
        assert_eq!(normalize(65_535, 16, 0, 65_535), AXIS_FULL);
        assert_eq!(normalize(1023, 10, 0, 1023), AXIS_FULL);
        assert_eq!(normalize(512, 10, 0, 1023), 512 * 65_535 / 1023);
        assert_eq!(normalize(4095, 12, 0, 4095), AXIS_FULL);
        // Signed: -32768 is the bottom, 0 the middle, 32767 the top.
        assert_eq!(normalize(0x8000, 16, -32_768, 32_767), 0);
        assert_eq!(normalize(0, 16, -32_768, 32_767), 32_768 * 65_535 / 65_535);
        assert_eq!(normalize(0x7FFF, 16, -32_768, 32_767), AXIS_FULL);
        assert_eq!(normalize(0xFF, 8, -127, 127), 126 * 65_535 / 254, "0xFF is -1 on a signed axis");
        assert_eq!(normalize(0x80, 8, -127, 127), 0, "-128 lies below the range and is held at its end");
        // A maximum the device states wrongly (-1 for 0xFFFF): the whole range of the bits.
        assert_eq!(normalize(0xFFFF, 16, 0, -1), AXIS_FULL);
        assert_eq!(normalize(0x8000, 16, 0, -1), 0x8000 * 65_535 / 65_535);
        // Outside the range: held at its ends.
        assert_eq!(normalize(2000, 12, 100, 1000), AXIS_FULL);
        assert_eq!(normalize(50, 12, 100, 1000), 0);
        assert_eq!(normalize(u32::MAX, 32, 0, -1), AXIS_FULL);
    }

    #[test]
    fn a_shake_of_a_few_steps_stays_below_the_threshold_on_coarse_axes_and_a_real_move_does_not() {
        // A 10-bit axis: one step is 64 on the scale; a shake of two steps stays below, 16 steps are a move.
        let at = |raw: u32| normalize(raw, 10, 0, 1023);
        assert!(at(514).abs_diff(at(512)) < THRESHOLD);
        assert!(at(528).abs_diff(at(512)) >= THRESHOLD);
        // An 8-bit axis: one step is 257; three steps still are no move, four are.
        let at = |raw: u32| normalize(raw, 8, 0, 255);
        assert!(at(131).abs_diff(at(128)) < THRESHOLD);
        assert!(at(132).abs_diff(at(128)) >= THRESHOLD);
    }

    #[test]
    fn a_report_of_a_coarse_axis_is_measured_on_the_same_scale_as_a_fine_one() {
        // A game pad with a 10-bit X axis: two steps of shaking are no move, sixteen steps are.
        let mut layout = Layout::default();
        layout.add_values(ValueCap {
            page: 0x01,
            report: 0,
            range: false,
            first_usage: 0x30,
            first_index: 0,
            last_index: 0,
            count: 1,
            bits: 10,
            min: 0,
            max: 1023,
        });
        let at = |raw: u32| {
            let mut state = StickState::default();
            state.apply(&layout, 0, &[(0, raw)]);
            state
        };
        assert!(!at(514).moved_from(&at(512), THRESHOLD), "shaking");
        assert!(at(528).moved_from(&at(512), THRESHOLD), "a move of 1.6 % of the way");
    }

    #[test]
    fn the_layout_takes_what_counts_and_leaves_the_rest() {
        let mut layout = Layout::default();
        // X, Y, Z, Rx in one range; a hat; a vendor value; a value array; a throttle of the simulation page.
        layout.add_values(ValueCap {
            page: 0x01,
            report: 1,
            range: true,
            first_usage: 0x30,
            first_index: 10,
            last_index: 13,
            count: 4,
            bits: 16,
            min: 0,
            max: 65_535,
        });
        let single = |page, usage, index| ValueCap {
            page,
            report: 1,
            range: false,
            first_usage: usage,
            first_index: index,
            last_index: index,
            count: 1,
            bits: 8,
            min: 0,
            max: 7,
        };
        layout.add_values(single(0x01, 0x39, 20));
        layout.add_values(single(0xFF00, 0x01, 21));
        layout.add_values(ValueCap { count: 3, ..single(0x01, 0x30, 22) });
        layout.add_values(single(0x02, 0xBB, 23));
        layout.add_buttons(ButtonCap { page: BUTTON_PAGE, report: 2, first_index: 30, last_index: 31 });
        layout.add_buttons(ButtonCap { page: 0x08, report: 2, first_index: 40, last_index: 40 });
        let kinds: Vec<(u16, Kind)> = layout.controls.iter().map(|(index, c)| (*index, c.kind)).collect();
        assert_eq!(
            kinds,
            [
                (10, Kind::Axis),
                (11, Kind::Axis),
                (12, Kind::Axis),
                (13, Kind::Axis),
                (20, Kind::Hat),
                (23, Kind::Axis),
                (30, Kind::Button),
                (31, Kind::Button),
            ]
        );
        // A report of id 2 sets only its buttons; the axes of report 1 stay unknown.
        let mut state = StickState::default();
        state.apply(&layout, 2, &[(31, 1)]);
        assert_eq!(state.len(), 2);
        let before = state.clone();
        state.apply(&layout, 1, &[(10, 100), (20, 3)]);
        assert!(!state.moved_from(&before, THRESHOLD), "controls seen for the first time");
        let before = state.clone();
        state.apply(&layout, 1, &[(10, 100), (20, 4)]);
        assert!(state.moved_from(&before, THRESHOLD), "the hat with every change");
        let before = state.clone();
        state.apply(&layout, 2, &[]);
        assert!(state.moved_from(&before, THRESHOLD), "button 31 let go");
        // A value missing from its report keeps its state.
        let before = state.clone();
        state.apply(&layout, 1, &[]);
        assert_eq!(state, before);
    }
}
