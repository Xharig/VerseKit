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

//! The sticks the system sees now (`input_device.py` of 4.12): the third view on the same devices, next to the
//! log (what the game saw last) and the binding (which number the binding means).
//!
//! ⭐ **The id of the game is the id of the device.** Star Citizen names its devices like
//! `{03F33344-0000-0000-0000-504944564944}`: `504944564944` is `PIDVID` in ASCII, and in front of it stand the
//! product and the vendor number, four hex digits each. Linux gives the same numbers in
//! `/sys/class/input/js0/device/id/`, Windows in the Raw Input list. So a pressed button can be given to the
//! right device in the `actionmaps.xml` without comparing names and without guessing.
//!
//! ⚠ With two **identical** sticks vendor and product are the same; then the match stays open and the page has
//! to ask instead of guessing.
//!
//! ⛔ **The list is not made over `winmm`.** `joyGetDevCapsW` for every one of the 16 places, asked every three
//! seconds, can bring down a Windows with a HOTAS setup (heap damage in the driver path, code `0xc0000374`; no
//! `try` helps). `GetRawInputDeviceList` only asks the list of the system and speaks to no joystick driver. The
//! name comes from a short opening of the device (`HidD_GetProductString`), **once per device**, then it is
//! remembered.
//!
//! ⚠ `joyGetPosEx` is no safe way either: it runs DirectInput in the process, which searches the devices again
//! for empty slots (`SetupDiEnumDeviceInterfaces`) and can overrun its own heap block there (`0xc0000374`).
//! Only the wait for a press (`wait`, while the binding window of
//! the main window is open) still reads the buttons over `joyGetPosEx`, never
//! `joyGetDevCapsW`; the overlay follows the sticks by Raw Input ([`crate::stick_input`]).
//!
//! ⚠ This is **not** the number the game uses. The order of the system and the order of the game are two
//! things (the same stick was `js0` on Linux and `js2` in the game). Both are joined over the id.

/// The id the game writes: product, vendor, then "PIDVID".
const ID_PATTERN_TAIL: &str = "-0000-0000-0000-504944564944";

/// A joystick as the system lists it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SystemDevice {
    /// `/dev/input/js0` or the device path of Windows (`\\?\HID#VID_…`).
    pub path: String,
    pub name: String,
    /// The id of the game; empty where it could not be read.
    pub id: String,
}

/// The id the game uses, from vendor and product number.
pub fn ident_from_ids(vendor: u64, product: u64) -> String {
    format!("{product:04X}{vendor:04X}{ID_PATTERN_TAIL}")
}

/// The joysticks that are connected now, on both systems the same. Empty on a system without a joystick, where
/// the question cannot be asked (other systems) or fails.
pub fn devices() -> Vec<SystemDevice> {
    platform_devices()
}

/// The joysticks of a Linux system with the given folders (`/sys/class/input`, `/dev/input`): the folder of the
/// kernel tells the numbers, the second one whether the device file is there.
pub fn linux_devices_in(sys: &std::path::Path, dev: &std::path::Path) -> Vec<SystemDevice> {
    let Ok(entries) = std::fs::read_dir(sys) else { return Vec::new() };
    let mut names: Vec<String> =
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.starts_with("js")).collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let device_file = dev.join(&name);
        if !device_file.exists() {
            continue;
        }
        let read = |field: &str| std::fs::read(sys.join(&name).join(field)).ok();
        let text = |field: &str| read(field).map(|b| crate::text::py_strip(&String::from_utf8_lossy(&b)).to_string());
        let number = |field: &str| u64::from_str_radix(text(field)?.trim_start_matches("0x"), 16).ok();
        let id = match (number("device/id/vendor"), number("device/id/product")) {
            (Some(vendor), Some(product)) => ident_from_ids(vendor, product),
            _ => String::new(),
        };
        out.push(SystemDevice {
            path: device_file.to_string_lossy().into_owned(),
            name: text("device/name").unwrap_or_default(),
            id,
        });
    }
    out
}

// ------------------------------------------------------------ waiting for a press

/// What a stick reported when the player pressed something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pressed {
    /// The id of the device (as the game writes it).
    pub id: String,
    /// The input without the prefix: `button10`, `x`.
    pub input: String,
    /// The name of the device.
    pub name: String,
}

/// From this movement of an axis (range −32767 to 32767) on an axis counts as moved. High on purpose: a stick
/// seldom rests on exactly zero, and a shaky axis must not trigger a binding.
pub const AXIS_THRESHOLD: i32 = 24_000;

/// The usual order of the axes as DirectInput reports them. ⚠ An **assumption**, no measurement: which axis the
/// game calls `x` depends on the driver. So the page shows a detected axis and lets the player confirm it
/// instead of setting it quietly.
const AXES: [&str; 8] = ["x", "y", "z", "rotx", "roty", "rotz", "slider1", "slider2"];

/// The name of the axis with this number, if there is one.
pub fn axis_name(number: usize) -> Option<&'static str> {
    AXES.get(number).copied()
}

/// What an event of the joystick protocol of Linux (`/dev/input/js*`: four bytes of time, two of value, one of
/// kind, one of number) says, if it says a press: the input name. The events right after opening carry the init
/// bit and only describe the state: they are no press.
///
/// ⚠ Star Citizen counts buttons from **one**, Linux from zero. Without the plus every binding sits one button
/// off, and that is noticed only in the game.
pub fn linux_event(raw: [u8; 8]) -> Option<String> {
    const BUTTON: u8 = 0x01;
    const AXIS: u8 = 0x02;
    const INIT: u8 = 0x80;
    let value = i16::from_le_bytes([raw[4], raw[5]]);
    let (kind, number) = (raw[6], raw[7]);
    if kind & INIT != 0 {
        return None;
    }
    if kind & BUTTON != 0 && value != 0 {
        return Some(format!("button{}", u32::from(number) + 1));
    }
    if kind & AXIS != 0 && i32::from(value).abs() >= AXIS_THRESHOLD {
        return axis_name(usize::from(number)).map(str::to_string);
    }
    None
}

/// The slots of the Windows registry as `(slot from 0, id)`: `Joystick1OEMName = VID_3344&PID_03F3` is slot 0.
/// ⚠ **The registry counts from 1, winmm from 0** (measured on three devices, three of three). Kept apart from
/// the query of the registry so that it can be checked on any system.
pub fn slots_from_values(pairs: &[(String, String)]) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (name, value) in pairs {
        let lower = name.to_lowercase();
        let Some(number) = lower.strip_prefix("joystick").and_then(|r| r.strip_suffix("oemname")) else { continue };
        let Ok(number) = number.parse::<usize>() else { continue };
        let upper = value.trim().to_uppercase();
        let Some(rest) = upper.strip_prefix("VID_") else { continue };
        let Some((vendor, product)) = rest.split_once("&PID_") else { continue };
        let (Some(vendor), Some(product)) = (hex4(vendor), hex4(product)) else { continue };
        let Some(slot) = number.checked_sub(1) else { continue };
        // Several drivers can keep the same slot: the first find counts.
        if !out.iter().any(|(known, _)| *known == slot) {
            out.push((slot, ident_from_ids(vendor, product)));
        }
    }
    out
}

fn hex4(text: &str) -> Option<u64> {
    (text.len() == 4 && text.chars().all(|c| c.is_ascii_hexdigit())).then(|| u64::from_str_radix(text, 16).ok())?
}

/// Can a press be waited for at all on this system?
pub fn available() -> bool {
    platform_available()
}

/// Waits for the next press of a stick, at most `seconds`. `stop` says when to give up earlier (the window was
/// closed). **Blocks:** belongs in a thread of its own, never in the one of the surface.
///
/// ⚠ Read only while the player explicitly waits for a press, and for a few seconds at most: a tool that reads
/// input devices all the time is what one does not want from an overlay. Keyboard and mouse are not read here
/// at all: the window of the program catches them itself while it has the focus.
pub fn wait(seconds: f64, stop: &dyn Fn() -> bool) -> Option<Pressed> {
    platform_wait(seconds, stop)
}

/// The key of the number block in the `lParam` of a key message of Windows (`WM_KEYDOWN`), by the name the game
/// gives it (`np_5`, `np_enter`); `None` for any other key and for the repeat of a key held down.
///
/// The scan code tells the number block from the row of figures and from the arrows, whatever the layout and
/// whatever Num Lock says: `0x47` is `np_7`, the same code as an extended key is Home.
pub fn number_block_key(lparam: isize) -> Option<&'static str> {
    // Bit 30: the key was down before the message (a repeat).
    if lparam & (1 << 30) != 0 {
        return None;
    }
    let scan = u16::try_from((lparam >> 16) & 0xFF).unwrap_or(0);
    let extended = lparam & (1 << 24) != 0;
    crate::anti_afk_key::name_of(crate::anti_afk_key::Key { scan, extended }).filter(|name| name.starts_with("np_"))
}

/// Notes the keys of the number block pressed in the window that is active on the calling thread, while it lives.
/// The toolkit gives the number block the names of the row of figures; the messages of the own window still carry
/// the scan code. Only the own window is seen, nothing all over the system. Windows only: elsewhere
/// [`NumberBlockTap::start`] gives `None`.
#[derive(Debug)]
pub struct NumberBlockTap {
    #[cfg(windows)]
    hwnd: isize,
}

impl NumberBlockTap {
    /// Starts on the active window of the calling thread; `None` where there is none or the system says no.
    pub fn start() -> Option<NumberBlockTap> {
        tap::start()
    }

    /// The keys pressed since the last call, oldest first.
    pub fn take(&self) -> Vec<&'static str> {
        tap::take()
    }
}

#[cfg(not(windows))]
mod tap {
    use super::NumberBlockTap;

    pub(super) fn start() -> Option<NumberBlockTap> {
        None
    }

    pub(super) fn take() -> Vec<&'static str> {
        Vec::new()
    }
}

#[cfg(windows)]
impl Drop for NumberBlockTap {
    fn drop(&mut self) {
        tap::stop(self.hwnd);
    }
}

#[cfg(windows)]
mod tap {
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_KEYDOWN, WM_NCDESTROY, WM_SYSKEYDOWN};

    use super::{NumberBlockTap, number_block_key};

    const SUBCLASS_ID: usize = 0x5645_4E42;
    /// The most keys kept between two calls of `take`.
    const TAP_LIMIT: usize = 32;

    pub(super) fn start() -> Option<NumberBlockTap> {
        // SAFETY: a function without arguments.
        let hwnd = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() } as isize;
        start_on(hwnd)
    }

    /// The keys noted by the hook and not yet taken.
    static PRESSED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN)
            && let Some(name) = number_block_key(lparam)
            && let Ok(mut pressed) = PRESSED.lock()
            && pressed.len() < TAP_LIMIT
        {
            pressed.push(name);
        }
        if msg == WM_NCDESTROY {
            // SAFETY: the window goes away: the hook goes with it.
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        // SAFETY: passes the message on as received.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub(super) fn start_on(hwnd: isize) -> Option<NumberBlockTap> {
        if hwnd == 0 {
            return None;
        }
        take();
        // SAFETY: the handle is a window of the calling thread; the hook only reads the message and passes it on.
        let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, 0) } != 0;
        ok.then_some(NumberBlockTap { hwnd })
    }

    pub(super) fn take() -> Vec<&'static str> {
        PRESSED.lock().map(|mut pressed| std::mem::take(&mut *pressed)).unwrap_or_default()
    }

    pub(super) fn stop(hwnd: isize) {
        // SAFETY: removes the hook set in `start_on`; a window that is gone already makes the call fail, nothing more.
        unsafe { RemoveWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID) };
        take();
    }
}

#[cfg(windows)]
fn platform_available() -> bool {
    // ⚠ `joyGetNumDevs` says 16 (the places) even with no stick at all: a device must be on the list as well.
    // SAFETY: a function without arguments.
    let places = unsafe { windows_sys::Win32::Media::Multimedia::joyGetNumDevs() };
    places != 0 && !devices().is_empty()
}

/// The time to wait as a duration; what is not a sensible number (NaN, infinite) is nothing, an hour at most.
fn patience(seconds: f64) -> std::time::Duration {
    std::time::Duration::from_secs_f64(if seconds.is_finite() { seconds.clamp(0.0, 3600.0) } else { 0.0 })
}

#[cfg(unix)]
fn platform_available() -> bool {
    !platform_devices().is_empty()
}

#[cfg(not(any(unix, windows)))]
fn platform_available() -> bool {
    false
}

#[cfg(not(any(unix, windows)))]
fn platform_wait(_seconds: f64, _stop: &dyn Fn() -> bool) -> Option<Pressed> {
    None
}

#[cfg(unix)]
fn platform_wait(seconds: f64, stop: &dyn Fn() -> bool) -> Option<Pressed> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut open: Vec<(std::fs::File, SystemDevice)> = Vec::new();
    for device in platform_devices() {
        let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&device.path);
        if let Ok(file) = file {
            open.push((file, device));
        }
    }
    if open.is_empty() {
        return None;
    }
    let end = std::time::Instant::now() + patience(seconds);
    while std::time::Instant::now() < end {
        if stop() {
            return None;
        }
        for (file, device) in &mut open {
            let mut raw = [0u8; 8];
            // Not blocking: nothing to read is an error that just says "later". Everything that lies ready is
            // read in one go: right after opening a device with many buttons sends a long run of init events,
            // one per round would take seconds to get to the first real press.
            for _ in 0..1024 {
                if file.read_exact(&mut raw).is_err() {
                    break;
                }
                if let Some(input) = linux_event(raw) {
                    return Some(Pressed { id: device.id.clone(), input, name: device.name.clone() });
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    None
}

#[cfg(windows)]
fn platform_wait(seconds: f64, stop: &dyn Fn() -> bool) -> Option<Pressed> {
    use windows_sys::Win32::Media::Multimedia::{JOYINFOEX, joyGetNumDevs, joyGetPosEx};
    const RETURN_ALL: u32 = 0xFF;
    // SAFETY: a function without arguments.
    let count = unsafe { joyGetNumDevs() } as usize;
    if count == 0 {
        return None;
    }
    // winmm calls every HID stick "Microsoft PC-joystick driver": the real name comes from the Raw Input list,
    // joined over the id.
    let known = devices();
    let mut by_slot = raw::winmm_slots();
    if by_slot.is_empty() && known.iter().filter(|d| !d.id.is_empty()).count() == 1 {
        // Without a registry entry the join is clear only where exactly one device is connected. With more it is
        // not guessed: a wrong id binds the key to the wrong stick, and nobody notices.
        let only = known.iter().find(|d| !d.id.is_empty()).map(|d| d.id.clone()).unwrap_or_default();
        by_slot = (0..count).map(|slot| (slot, only.clone())).collect();
    }
    let read = |slot: usize| -> Option<u32> {
        let mut info =
            JOYINFOEX { dwSize: std::mem::size_of::<JOYINFOEX>() as u32, dwFlags: RETURN_ALL, ..Default::default() };
        // SAFETY: `info` is as large as `dwSize` says and lives during the call.
        (unsafe { joyGetPosEx(slot as u32, &raw mut info) } == 0).then_some(info.dwButtons)
    };
    // The state at the start: a button that is held already is no press (the counterpart of the init bit).
    // `joyGetPosEx` says in the same go whether the slot is taken at all.
    let mut devices: Vec<(usize, String, u32)> = Vec::new();
    for slot in 0..count {
        let Some((_, id)) = by_slot.iter().find(|(known_slot, _)| *known_slot == slot) else { continue };
        if let Some(buttons) = read(slot) {
            devices.push((slot, id.clone(), buttons));
        }
    }
    if devices.is_empty() {
        return None;
    }
    let end = std::time::Instant::now() + patience(seconds);
    while std::time::Instant::now() < end {
        if stop() {
            return None;
        }
        for (slot, id, before) in &mut devices {
            let Some(buttons) = read(*slot) else { continue };
            let fresh = buttons & !*before;
            if fresh != 0 {
                let number = fresh.trailing_zeros() + 1;
                let name = known.iter().find(|d| d.id == *id).map(|d| d.name.clone()).unwrap_or_default();
                return Some(Pressed { id: id.clone(), input: format!("button{number}"), name });
            }
            *before = buttons;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    None
}

#[cfg(unix)]
fn platform_devices() -> Vec<SystemDevice> {
    linux_devices_in(std::path::Path::new("/sys/class/input"), std::path::Path::new("/dev/input"))
}

#[cfg(not(any(unix, windows)))]
fn platform_devices() -> Vec<SystemDevice> {
    Vec::new()
}

#[cfg(windows)]
fn platform_devices() -> Vec<SystemDevice> {
    raw::devices()
}

#[cfg(windows)]
mod raw {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::{Mutex, OnceLock};

    use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetProductString;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
    use windows_sys::Win32::UI::Input::{
        GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO, RIDI_DEVICEINFO,
        RIDI_DEVICENAME, RIM_TYPEHID,
    };

    use super::{SystemDevice, ident_from_ids};

    /// HID usages that count as an input device for a game (usage page 0x01 "Generic Desktop"): joystick,
    /// game pad, multi-axis device.
    const GAME_USAGES: [u16; 3] = [0x04, 0x05, 0x08];
    /// The result of the functions when they fail.
    const FAIL: u32 = u32::MAX;

    /// The name of a device by its path: read once, then remembered (opening the device must not happen at
    /// every beat).
    fn names() -> &'static Mutex<HashMap<String, String>> {
        static NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        NAMES.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// The raw list of the system: handle and type of every input device.
    fn list() -> Option<Vec<RAWINPUTDEVICELIST>> {
        let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
        let mut count: u32 = 0;
        // SAFETY: a null buffer only asks how many devices there are; `count` is a valid number.
        if unsafe { GetRawInputDeviceList(std::ptr::null_mut(), &raw mut count, size) } == FAIL {
            return None;
        }
        // A device can come in between counting and fetching: then the buffer is too small and the new count
        // is tried again.
        for _ in 0..3 {
            let slots = count.max(1);
            let mut buffer: Vec<RAWINPUTDEVICELIST> = vec![RAWINPUTDEVICELIST::default(); slots as usize];
            count = slots;
            // SAFETY: the buffer holds `count` entries and stays alive during the call.
            let got = unsafe { GetRawInputDeviceList(buffer.as_mut_ptr(), &raw mut count, size) };
            if got != FAIL {
                buffer.truncate(got as usize);
                return Some(buffer);
            }
        }
        None
    }

    /// The path of the device (`\\?\HID#VID_…`) or `None`.
    fn path_of(handle: HANDLE) -> Option<String> {
        let mut size: u32 = 0;
        // SAFETY: a null buffer only asks for the size (in characters).
        unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, std::ptr::null_mut(), &raw mut size) };
        if size == 0 {
            return None;
        }
        let mut buffer: Vec<u16> = vec![0; size as usize + 1];
        // SAFETY: the buffer holds `size` characters plus one and stays alive during the call.
        let got = unsafe {
            GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, buffer.as_mut_ptr().cast::<c_void>(), &raw mut size)
        };
        if got == FAIL {
            return None;
        }
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    /// The product name of a HID device or an empty text. ⚠ Opened **without** right to read or write (access
    /// 0): enough for the description, and it does not collide with the game that uses the stick.
    fn product_name(path: &str) -> String {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` ends in a zero and stays alive; the other arguments are plain numbers or null.
        let handle =
            unsafe { CreateFileW(wide.as_ptr(), 0, 1 | 2, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return String::new();
        }
        // 126 characters is the limit HID gives for strings; the buffer is given in bytes.
        let mut text = [0u16; 127];
        // SAFETY: the handle is valid until the next call; the buffer is as long as it is said to be.
        let ok = unsafe {
            HidD_GetProductString(handle, text.as_mut_ptr().cast::<c_void>(), std::mem::size_of_val(&text) as u32)
        };
        // SAFETY: the handle was opened above and is closed once.
        unsafe { CloseHandle(handle) };
        if !ok {
            return String::new();
        }
        let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
        crate::text::py_strip(&String::from_utf16_lossy(&text[..end])).to_string()
    }

    /// `…&Col01` and `…&Col02` are two function blocks of one stick: the path without it is the key.
    fn key_of(path: &str) -> String {
        let lower = path.to_lowercase();
        let mut out = String::with_capacity(lower.len());
        let mut rest = lower.as_str();
        while let Some(at) = rest.find("&col") {
            out.push_str(&rest[..at]);
            let tail = &rest[at + 4..];
            let digits = tail.chars().take_while(char::is_ascii_hexdigit).count();
            if digits == 0 {
                out.push_str("&col");
            }
            rest = &tail[digits..];
        }
        out.push_str(rest);
        out
    }

    /// Which winmm slot belongs to which id, from the registry (`HKCU\System\CurrentControlSet\Control\
    /// MediaResources\Joystick\<driver>\CurrentJoystickSettings`, values `Joystick<n>OEMName`).
    ///
    /// ⛔⛔ **This replaces `joyGetDevCapsW`**, which gives vendor and product as well but can bring down a Windows
    /// with a HOTAS setup (`0xc0000374`). The registry answers the same question without speaking to a joystick
    /// driver. Empty where the registry says nothing (a foreign driver): the caller then falls back to "only one
    /// device connected", and else to nothing. Nothing is guessed.
    pub(super) fn winmm_slots() -> Vec<(usize, String)> {
        use crate::paths::registry::{HKEY_CURRENT_USER, open};
        const BASE: &str = r"System\CurrentControlSet\Control\MediaResources\Joystick";
        let Some(root) = open(HKEY_CURRENT_USER, BASE) else { return Vec::new() };
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut index = 0;
        while let Some(driver) = root.enum_key(index) {
            index += 1;
            let Some(key) = root.sub(&format!("{driver}\\CurrentJoystickSettings")) else { continue };
            for number in 1..=16 {
                let name = format!("Joystick{number}OEMName");
                if let Some(value) = key.value(&name) {
                    pairs.push((name, value));
                }
            }
        }
        super::slots_from_values(&pairs)
    }

    pub(super) fn devices() -> Vec<SystemDevice> {
        let Some(entries) = list() else { return Vec::new() };
        let mut seen: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for entry in entries {
            if entry.dwType != RIM_TYPEHID {
                continue;
            }
            let mut info =
                RID_DEVICE_INFO { cbSize: std::mem::size_of::<RID_DEVICE_INFO>() as u32, ..Default::default() };
            let mut size = info.cbSize;
            // SAFETY: `info` is as large as `cbSize` says; `size` is a valid number.
            let got = unsafe {
                GetRawInputDeviceInfoW(entry.hDevice, RIDI_DEVICEINFO, (&raw mut info).cast::<c_void>(), &raw mut size)
            };
            if got == FAIL {
                continue;
            }
            // SAFETY: the type is `RIM_TYPEHID`, so the `hid` part of the union was filled.
            let hid = unsafe { info.Anonymous.hid };
            if hid.usUsagePage != 0x01 || !GAME_USAGES.contains(&hid.usUsage) {
                continue;
            }
            let Some(path) = path_of(entry.hDevice) else { continue };
            let key = key_of(&path);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let name = {
                let known = names().lock().ok().and_then(|map| map.get(&path).cloned());
                match known {
                    Some(name) => name,
                    None => {
                        let name = product_name(&path);
                        // ⚠ An empty name is not kept (a device just plugged in may not be open yet): it is
                        // asked again at the next look. 4.12 kept it until the end of the program.
                        if !name.is_empty()
                            && let Ok(mut map) = names().lock()
                        {
                            map.insert(path.clone(), name.clone());
                        }
                        name
                    }
                }
            };
            out.push(SystemDevice {
                id: ident_from_ids(u64::from(hid.dwVendorId & 0xFFFF), u64::from(hid.dwProductId & 0xFFFF)),
                path,
                name,
            });
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::key_of;

        #[test]
        fn the_function_blocks_of_one_stick_have_one_key() {
            assert_eq!(
                key_of("\\\\?\\HID#VID_3344&PID_43F5&Col01#7&2"),
                key_of("\\\\?\\HID#VID_3344&PID_43F5&COL02#7&2")
            );
            assert_ne!(key_of("\\\\?\\HID#VID_3344&PID_43F5#7&2"), key_of("\\\\?\\HID#VID_3344&PID_43F6#7&2"));
            assert_eq!(key_of("a&colx"), "a&colx", "only digits after Col count as a block");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("versekit-inputdevice-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir
    }

    fn put(path: &std::path::Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
        std::fs::write(path, text).expect("file");
    }

    fn event(value: i16, kind: u8, number: u8) -> [u8; 8] {
        let v = value.to_le_bytes();
        [0, 0, 0, 0, v[0], v[1], kind, number]
    }

    #[test]
    fn a_press_of_a_stick_is_read_from_its_events_with_the_buttons_counted_from_one() {
        assert_eq!(
            linux_event(event(1, 0x01, 0)).as_deref(),
            Some("button1"),
            "Linux counts from zero, the game from one"
        );
        assert_eq!(linux_event(event(1, 0x01, 11)).as_deref(), Some("button12"));
        assert_eq!(linux_event(event(0, 0x01, 3)), None, "letting go is no press");
        assert_eq!(linux_event(event(1, 0x81, 3)), None, "the init bit only describes the state");
        assert_eq!(linux_event(event(24_000, 0x02, 1)).as_deref(), Some("y"));
        assert_eq!(linux_event(event(-24_000, 0x02, 3)).as_deref(), Some("rotx"));
        assert_eq!(linux_event(event(23_999, 0x02, 0)), None, "a shaky axis is no press");
        assert_eq!(linux_event(event(i16::MIN, 0x02, 0)).as_deref(), Some("x"), "the full left deflection");
        assert_eq!(linux_event(event(30_000, 0x02, 9)), None, "an axis the order does not know");
    }

    #[test]
    fn the_slots_of_the_registry_count_from_one_and_the_first_driver_keeps_a_slot() {
        let pairs: Vec<(String, String)> = [
            ("Joystick1OEMName", "VID_3344&PID_03F3"),
            ("Joystick2OEMName", " vid_3344&pid_43f5 "),
            ("joystick2oemname", "VID_0001&PID_0002"),
            ("Joystick3OEMName", "kein Geraet"),
            ("Joystick0OEMName", "VID_0001&PID_0002"),
            ("JoystickXOEMName", "VID_0001&PID_0002"),
            ("Anderes", "VID_0001&PID_0002"),
        ]
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect();
        assert_eq!(
            slots_from_values(&pairs),
            vec![
                (0, "03F33344-0000-0000-0000-504944564944".to_string()),
                (1, "43F53344-0000-0000-0000-504944564944".to_string()),
            ]
        );
    }

    /// By hand, with sticks plugged in: `cargo test -p versekit-core --lib -- input_device::tests::show --ignored
    /// --nocapture` lists what the system and the registry say, and waits ten seconds for a press.
    #[test]
    #[ignore = "needs sticks and a hand"]
    fn show_what_the_system_says_about_the_sticks() {
        println!("devices: {:#?}", devices());
        #[cfg(windows)]
        println!("slots: {:?}", raw::winmm_slots());
        println!("available: {}", available());
        println!("press a button within ten seconds ...");
        println!("pressed: {:?}", wait(10.0, &|| false));
    }

    #[test]
    fn asking_whether_a_press_can_be_waited_for_does_not_fail() {
        let _ = available();
        assert_eq!(wait(0.0, &|| false), None, "no time, no press");
        assert_eq!(axis_name(0), Some("x"));
        assert_eq!(axis_name(8), None);
    }

    #[test]
    fn the_id_is_product_vendor_and_pidvid() {
        assert_eq!(ident_from_ids(0x3344, 0x43F5), "43F53344-0000-0000-0000-504944564944");
        assert_eq!(ident_from_ids(0x1, 0x2), "00020001-0000-0000-0000-504944564944");
    }

    #[test]
    fn the_joysticks_of_a_linux_system_come_with_their_id_in_the_order_of_their_names() {
        let root = scratch("linux");
        let (sys, dev) = (root.join("sys"), root.join("dev"));
        for (name, text, vendor, product) in
            [("js1", "Stick R\n", "3344\n", "43f5\n"), ("js0", " Stick L ", "0x3344", "43F4")]
        {
            put(&sys.join(name).join("device/name"), text);
            put(&sys.join(name).join("device/id/vendor"), vendor);
            put(&sys.join(name).join("device/id/product"), product);
            put(&dev.join(name), "");
        }
        put(&sys.join("js2/device/name"), "Without a device file");
        put(&sys.join("js3/device/name"), "Without numbers");
        put(&dev.join("js3"), "");
        put(&sys.join("event5/device/name"), "No joystick");
        put(&dev.join("event5"), "");
        let found = linux_devices_in(&sys, &dev);
        let line: Vec<(&str, &str)> = found.iter().map(|d| (d.name.as_str(), d.id.as_str())).collect();
        assert_eq!(
            line,
            [
                ("Stick L", "43F43344-0000-0000-0000-504944564944"),
                ("Stick R", "43F53344-0000-0000-0000-504944564944"),
                ("Without numbers", ""),
            ]
        );
        assert!(found[0].path.ends_with("js0"));
        assert!(linux_devices_in(&root.join("nothing"), &dev).is_empty());
    }

    /// The `lParam` of a key that goes down: repeat count 1, the scan code, the extended flag.
    fn key_down(scan: isize, extended: bool) -> isize {
        1 | (scan << 16) | if extended { 1 << 24 } else { 0 }
    }

    #[test]
    fn the_number_block_is_told_by_its_scan_code_not_by_the_figure() {
        assert_eq!(number_block_key(key_down(0x4C, false)), Some("np_5"));
        assert_eq!(number_block_key(key_down(0x52, false)), Some("np_0"));
        assert_eq!(number_block_key(key_down(0x53, false)), Some("np_period"));
        assert_eq!(number_block_key(key_down(0x37, false)), Some("np_multiply"));
        assert_eq!(number_block_key(key_down(0x4E, false)), Some("np_add"));
        assert_eq!(number_block_key(key_down(0x4A, false)), Some("np_subtract"));
        assert_eq!(number_block_key(key_down(0x1C, true)), Some("np_enter"));
        assert_eq!(number_block_key(key_down(0x35, true)), Some("np_divide"));
        assert_eq!(number_block_key(key_down(0x47, true)), None, "the same code as an extended key is Home");
        assert_eq!(number_block_key(key_down(0x1C, false)), None, "Enter of the main block");
        assert_eq!(number_block_key(key_down(0x35, false)), None, "the slash of the main block");
        assert_eq!(number_block_key(key_down(0x0B, false)), None, "the 0 of the row of figures");
        assert_eq!(number_block_key(key_down(0x4C, false) | (1 << 30)), None, "the repeat of a key held down");
    }

    #[cfg(windows)]
    #[test]
    fn the_tap_notes_the_number_block_of_its_window_and_lets_go() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE, SendMessageW, WM_KEYDOWN,
        };
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a message-only window of this thread, never shown; the class name is a terminated wide string.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null(), "a window to listen on");
        // SAFETY: a key message to the window of this thread, handled at once.
        let send = |lparam: isize| unsafe { SendMessageW(hwnd, WM_KEYDOWN, 0, lparam) };
        let tap = tap::start_on(hwnd as isize).expect("the hook is set");
        send(key_down(0x4C, false));
        send(key_down(0x0B, false));
        send(key_down(0x1C, true));
        assert_eq!(tap.take(), ["np_5", "np_enter"], "only the number block, in order");
        assert!(tap.take().is_empty(), "taken once");
        drop(tap);
        send(key_down(0x4C, false));
        assert!(tap::take().is_empty(), "the hook went with the tap");
        // SAFETY: the window made above.
        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn asking_the_system_does_not_fail_and_every_stick_has_an_id_of_the_game() {
        for device in devices() {
            assert!(device.id.is_empty() || device.id.ends_with(ID_PATTERN_TAIL), "{device:?}");
        }
    }
}
