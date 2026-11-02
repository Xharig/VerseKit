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

//! Signals between the windows of Verse-Kit on one desktop. Each window
//! marks itself with its role; the others find it by that mark, not by its
//! title (which follows the language), and post it a registered message.
//!
//! Elsewhere than Windows the mark belongs to the process, not to the window: a process that serves a role
//! holds a locked file for it and listens on a socket of the same name, one byte for each signal, in a folder of
//! the user (`XDG_RUNTIME_DIR/versekit`). A handle there is the mark of the role, not a window of the system; the
//! role goes when its process ends. Under X11 the window of that process is found by its process
//! (`window_signal::x11`), brought to the front and asked whether it is shown; elsewhere (Wayland without X11,
//! macOS) the signals that ask for a page show the window themselves.
//!
//! ⚠ The mark belongs to the data folder of the run ([`key_for`], [`bind`]): two starts on the same data are
//! one program, two starts on different copies of the data are two, and neither finds the windows of the
//! other. A process that never bound itself (a test) has a key of its own and reaches no window of a running
//! program.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

/// The key this process is bound to; `None` until [`bind`].
static KEY: RwLock<Option<String>> = RwLock::new(None);

/// Binds this process to the run on one data folder (`key` from [`key_for`]): marks, searches and the one
/// overlay belong to it from now on. Called at the start of every program, before any window.
pub fn bind(key: &str) {
    *KEY.write().unwrap_or_else(PoisonError::into_inner) = Some(key.to_string());
}

/// The key of a process that never bound itself.
pub const UNBOUND: &str = "ungebunden";
/// The key of a run without data (sample contents): the overlay and the preview window it starts.
pub const SAMPLE: &str = "beispiel";

/// The key of this process: what [`bind`] set, or [`UNBOUND`]: no run of the program has it.
pub fn key() -> String {
    KEY.read().unwrap_or_else(PoisonError::into_inner).clone().unwrap_or_else(|| UNBOUND.to_string())
}

/// The key of a run: on a copy of the data (`data`, from `--daten`) or a data folder named from outside
/// (`home`, the variable for test runs) a short hash of that folder, resolved; on the user's own data the
/// empty key, so every build of the program on it is one program. The copy wins, as it does for the paths.
pub fn key_for(data: Option<&str>, home: Option<&str>) -> String {
    let folder = data.filter(|f| !f.is_empty()).or_else(|| home.filter(|f| !f.is_empty()));
    folder.map_or_else(String::new, folder_key)
}

/// A short hash of the resolved folder: the same folder in any spelling (case, slashes, a trailing
/// separator, `..`, a link) gives the same key, two folders two keys.
pub fn folder_key(folder: &str) -> String {
    // FNV-1a, 64 bits: the same number in every process (the hasher of std is seeded anew in each).
    let hash = resolved(folder).bytes().fold(FNV_START, fnv_step);
    format!("{:012x}", hash & 0xFFFF_FFFF_FFFF)
}

/// The folder as the system resolves it, in one spelling.
fn resolved(folder: &str) -> String {
    let path = std::path::Path::new(folder);
    let full = std::fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.into());
    let mut text = full.to_string_lossy().into_owned();
    if cfg!(windows) {
        text = text.replace('/', "\\");
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            text = format!(r"\\{rest}");
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            text = rest.to_string();
        }
        text = text.to_lowercase();
    }
    let trimmed = text.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() { text } else { trimmed.to_string() }
}

/// The value of the mark of a window with `role` in the run `key`: the role in the lowest byte, a hash of the
/// key above it; the user's own data (the empty key) carries the role alone.
///
/// ⚠ One mark name for every run, the key in the value: a name of its own per key would be a new entry in
/// the atom table of the session for every copy of the data, and one left behind by every process that ends
/// without destroying its windows.
fn mark_value(role: Role, key: &str) -> usize {
    if key.is_empty() {
        return role as usize;
    }
    let hash = key.bytes().fold(FNV_START, fnv_step) | 1;
    let room = usize::MAX >> 8;
    (((hash as usize) & room) << 8) | role as usize
}

const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv_step(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
}

/// What a window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The main window (the overview and the settings).
    Main = 1,
    Overlay = 2,
    /// The window of the backup at the first start, shown before the overlay or the assistant stands.
    FirstBackup = 3,
}

/// What a window is asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// Show the overview.
    Overview = 1,
    /// Read the settings again.
    Settings = 2,
    /// Close the window.
    Close = 3,
    /// Show the page "Allgemein" (the settings), as "Einstellungen öffnen" of the icon's menu does.
    OpenGeneral = 4,
    /// Show the page "Übersetzung".
    OpenTranslation = 5,
    /// Show the page "Update & Über".
    OpenAbout = 6,
    /// Switch anti-AFK on (to the overlay: it presses the keys; the main window only asks).
    AntiAfkOn = 7,
    /// Switch anti-AFK off.
    AntiAfkOff = 8,
    /// Show the page "Mining" (the signature scanner and the teaching of its digits).
    OpenMining = 9,
    /// Show the area the signature scanner reads over the game for a few seconds (to the overlay).
    ShowFrame = 10,
    /// Show the page "Mein Lager" and take the reading of the refinery terminal that waits there
    /// (`refinery_scan::screen::HANDOVER_FILE`).
    OpenStockScan = 11,
}

impl Signal {
    fn from_code(code: usize) -> Option<Signal> {
        match code {
            1 => Some(Signal::Overview),
            2 => Some(Signal::Settings),
            3 => Some(Signal::Close),
            4 => Some(Signal::OpenGeneral),
            5 => Some(Signal::OpenTranslation),
            6 => Some(Signal::OpenAbout),
            7 => Some(Signal::AntiAfkOn),
            8 => Some(Signal::AntiAfkOff),
            9 => Some(Signal::OpenMining),
            10 => Some(Signal::ShowFrame),
            11 => Some(Signal::OpenStockScan),
            _ => None,
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetPropW, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, PostMessageW,
        RegisterWindowMessageW, RemovePropW, SMTO_ABORTIFHUNG, SMTO_BLOCK, SW_SHOWNOACTIVATE, SWP_ASYNCWINDOWPOS,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageTimeoutW, SetPropW, SetWindowPos, ShowWindowAsync, WM_CLOSE,
        WM_NCDESTROY, WM_NULL,
    };

    use super::{ANSWER_WAIT, Role, Signal, mark_value, needs_show};

    const SUBCLASS_ID: usize = 0x5645_5349;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The name of the mark every Verse-Kit window carries.
    fn mark_name() -> &'static [u16] {
        static NAME: OnceLock<Vec<u16>> = OnceLock::new();
        NAME.get_or_init(|| wide("VerseKit.Rolle"))
    }

    /// The message number all Verse-Kit windows agree on.
    fn message() -> u32 {
        static MESSAGE: OnceLock<u32> = OnceLock::new();
        // SAFETY: the name is a terminated wide string that lives for the call.
        *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(wide("VerseKit.Signal").as_ptr()) })
    }

    type Handler = Box<dyn Fn(Signal) + Send + Sync>;

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        data: usize,
    ) -> LRESULT {
        if msg == message() && msg != 0 {
            if let Some(signal) = Signal::from_code(wparam) {
                // SAFETY: `data` is the handler boxed in `serve`; it lives until WM_NCDESTROY.
                let handler = unsafe { &*(data as *const Handler) };
                handler(signal);
            }
            return 0;
        }
        if msg == WM_NCDESTROY {
            // SAFETY: the window goes away: its mark, the hook and the handler with it.
            unsafe {
                RemovePropW(hwnd, mark_name().as_ptr());
                RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
                drop(Box::from_raw(data as *mut Handler));
            }
        }
        // SAFETY: passes the message on as received.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    pub fn serve(hwnd: isize, role: Role, key: &str, on_signal: impl Fn(Signal) + Send + Sync + 'static) -> bool {
        if hwnd == 0 || message() == 0 {
            return false;
        }
        let handler: Box<Handler> = Box::new(Box::new(on_signal));
        let data = Box::into_raw(handler) as usize;
        // SAFETY: the window handle comes from the running window of this
        // thread; the handler lives until the window is destroyed.
        unsafe {
            if SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, data) == 0 {
                drop(Box::from_raw(data as *mut Handler));
                return false;
            }
            SetPropW(hwnd as HWND, mark_name().as_ptr(), mark_value(role, key) as HANDLE) != 0
        }
    }

    pub fn find(role: Role, key: &str) -> Option<isize> {
        search(role, key, true).into_iter().next()
    }

    pub fn find_all(role: Role, key: &str) -> Vec<isize> {
        search(role, key, false)
    }

    /// The windows marked with `role` in the run `key`, in the order of the system; only the first with `first`.
    fn search(role: Role, key: &str, first: bool) -> Vec<isize> {
        struct Search {
            mark: usize,
            first: bool,
            found: Vec<isize>,
        }
        unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
            // SAFETY: `lparam` is the search below, alive during EnumWindows.
            let search = unsafe { &mut *(lparam as *mut Search) };
            // SAFETY: reads a property of a window by a terminated name.
            if unsafe { GetPropW(hwnd, mark_name().as_ptr()) } as usize == search.mark {
                search.found.push(hwnd as isize);
                if search.first {
                    return 0;
                }
            }
            1
        }
        let mut search = Search { mark: mark_value(role, key), first, found: Vec::new() };
        // SAFETY: the callback only touches `search`, which outlives the call.
        unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
        search.found
    }

    pub fn send(hwnd: isize, signal: Signal) -> bool {
        let msg = message();
        // SAFETY: posts plain numbers to a window; a gone window just fails.
        msg != 0 && unsafe { PostMessageW(hwnd as HWND, msg, signal as usize, 0) } != 0
    }

    pub fn close_window(hwnd: isize) -> bool {
        // SAFETY: posts the close request to a window; a gone window just fails.
        unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) != 0 }
    }

    pub fn bring_to_front(hwnd: isize) {
        let hwnd = hwnd as HWND;
        // Asynchronous: a busy or hanging window of another program must not
        // stop the caller.
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
        // SAFETY: changes place in the z order and the shown or minimized state of a
        // window, never the keyboard focus.
        unsafe {
            if needs_show(IsWindowVisible(hwnd) != 0, IsIconic(hwnd) != 0) {
                ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
            }
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
        }
    }

    /// The single instance is a named mutex of the overlay (`versekit_overlay`), which names no process.
    pub fn instance_owner(_key: &str) -> Option<u32> {
        None
    }

    pub fn is_shown(hwnd: isize) -> bool {
        // SAFETY: only reads the visible state of a window; a gone window reads as not shown.
        unsafe { IsWindowVisible(hwnd as HWND) != 0 }
    }

    pub fn answers(hwnd: isize) -> bool {
        let wait = u32::try_from(ANSWER_WAIT.as_millis()).unwrap_or(u32::MAX);
        let mut result = 0;
        // SAFETY: sends the empty message, which no window acts on; the call returns after `wait` at the latest
        // and at once for a window the system counts as hung or one that is gone.
        unsafe {
            SendMessageTimeoutW(hwnd as HWND, WM_NULL, 0, 0, SMTO_ABORTIFHUNG | SMTO_BLOCK, wait, &mut result) != 0
        }
    }
}

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
use unix as imp;
mod x11;

/// Marks `hwnd` (a window of this thread) with its role in the run of this
/// process ([`key`]) and calls `on_signal` for every signal it receives, on
/// the window's thread. Returns `false` when the window could not be marked.
pub fn serve(hwnd: isize, role: Role, on_signal: impl Fn(Signal) + Send + Sync + 'static) -> bool {
    imp::serve(hwnd, role, &key(), on_signal)
}

/// [`serve`] for the run `key` (a check that stands in for another run).
pub fn serve_as(hwnd: isize, role: Role, key: &str, on_signal: impl Fn(Signal) + Send + Sync + 'static) -> bool {
    imp::serve(hwnd, role, key, on_signal)
}

/// The window with that role in the run of this process on this desktop, if one runs.
pub fn find(role: Role) -> Option<isize> {
    imp::find(role, &key())
}

/// [`find`] in the run `key`.
pub fn find_as(role: Role, key: &str) -> Option<isize> {
    imp::find(role, key)
}

/// Every window with that role in the run of this process on this desktop (more than one main window can run: one
/// the overlay started, one the player opened besides). Elsewhere than Windows a role has one window at most.
pub fn find_all(role: Role) -> Vec<isize> {
    imp::find_all(role, &key())
}

/// [`find_all`] in the run `key`.
pub fn find_all_as(role: Role, key: &str) -> Vec<isize> {
    imp::find_all(role, key)
}

/// Asks the window `hwnd` to do `signal`; `false` when it cannot be reached.
pub fn send(hwnd: isize, signal: Signal) -> bool {
    imp::send(hwnd, signal)
}

/// Sends `signal` to the window with that role, if one runs.
pub fn send_to(role: Role, signal: Signal) -> bool {
    find(role).is_some_and(|hwnd| send(hwnd, signal))
}

/// Asks the window `hwnd` to close, as its own close button does (a window of another program of
/// Verse-Kit, which ends with it); `false` when it cannot be reached.
pub fn close_window(hwnd: isize) -> bool {
    imp::close_window(hwnd)
}

/// Whether a window has to be shown first to come to the front: one that is hidden (closed to the tray
/// area) or minimized.
#[cfg_attr(not(windows), allow(dead_code))]
fn needs_show(visible: bool, minimized: bool) -> bool {
    !visible || minimized
}

/// Puts a window in front of the other windows and restores it when
/// hidden or minimized, without taking the keyboard: a game in front keeps its keys.
pub fn bring_to_front(hwnd: isize) {
    imp::bring_to_front(hwnd);
}

/// Whether the window `hwnd` (a handle from [`find`]) is on screen, not hidden (a minimized window counts as shown).
/// Where the system cannot tell it of the window of another process (without X11, macOS), it counts as shown.
pub fn is_shown(hwnd: isize) -> bool {
    imp::is_shown(hwnd)
}

/// How long [`answers`] waits for a window that is busy.
#[cfg_attr(not(windows), allow(dead_code))]
const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// Whether the window `hwnd` (a handle from [`find`]) takes its messages: false for a window that is gone, one the
/// system counts as hung, and one that has not taken a message after [`ANSWER_WAIT`]. Where the system cannot tell
/// it of the window of another process (everywhere but Windows), a window that is found counts as answering.
pub fn answers(hwnd: isize) -> bool {
    imp::answers(hwnd)
}

/// The process behind the window `hwnd` (a handle from [`find`]); 0 when it cannot be told or no longer runs.
#[cfg(not(windows))]
pub fn process_of(hwnd: isize) -> u32 {
    imp::process_of(hwnd)
}

/// Whether this start is the first of the run of this process ([`key`]) elsewhere than Windows: it holds the
/// single instance until it ends; a later start finds it taken, also while the windows of the first one do not
/// stand yet. Asked once per process. ⚠ An update starts the new version while the old one still holds it
/// ([`crate::update_swap::REPLACES_PID`]): the new one waits for the end of the old one instead of handing over
/// to it, which would end it at once and roll the update back.
#[cfg(not(windows))]
pub fn first_instance() -> bool {
    static FIRST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FIRST.get_or_init(|| {
        let replaces = std::env::var(crate::update_swap::REPLACES_PID).ok();
        imp::first_instance(&key(), replaced_pid(replaces.as_deref()))
    })
}

/// The process the program of the run of this process runs in, where the system names it (not under Windows):
/// its overlay where one answers, else the process that holds the single instance. The one an update replaces.
pub fn instance_owner() -> Option<u32> {
    imp::instance_owner(&key())
}

type CloseHook = Arc<dyn Fn() + Send + Sync>;

/// What a request to close a window of this process does, set by [`on_close`].
static ON_CLOSE: RwLock<Option<CloseHook>> = RwLock::new(None);

/// What a request to close the windows of this process ([`close_window`] from another process) does where the
/// system has no close message that reaches the window of another process (everywhere but Windows, where the
/// window closes as by its own button). Without it the request arrives as [`Signal::Close`].
pub fn on_close(hook: impl Fn() + Send + Sync + 'static) {
    *ON_CLOSE.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
}

#[cfg_attr(windows, allow(dead_code))]
fn close_hook() -> Option<CloseHook> {
    ON_CLOSE.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// What one byte on the socket of a role asks (elsewhere than Windows).
#[cfg_attr(windows, allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Signal(Signal),
    /// Close the window, as its own close button does.
    Close,
}

/// The byte of [`Asked::Close`]; the signals are their codes.
#[cfg_attr(windows, allow(dead_code))]
const CLOSE_BYTE: u8 = 0xC0;

#[cfg_attr(windows, allow(dead_code))]
fn encode(asked: Asked) -> u8 {
    match asked {
        Asked::Signal(signal) => signal as u8,
        Asked::Close => CLOSE_BYTE,
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn decode(byte: u8) -> Option<Asked> {
    if byte == CLOSE_BYTE { Some(Asked::Close) } else { Signal::from_code(usize::from(byte)).map(Asked::Signal) }
}

/// Does what `asked` asks of a process: a signal goes to `on_signal`; a request to close goes to `hook` when one
/// is set ([`on_close`]), else to `on_signal` as [`Signal::Close`].
#[cfg_attr(windows, allow(dead_code))]
fn deliver(asked: Asked, hook: Option<&CloseHook>, on_signal: &dyn Fn(Signal)) {
    match (asked, hook) {
        (Asked::Signal(signal), _) => on_signal(signal),
        (Asked::Close, Some(hook)) => hook(),
        (Asked::Close, None) => on_signal(Signal::Close),
    }
}

/// The handle of the window with `role` in the run `key` elsewhere than Windows: its mark ([`mark_value`]).
#[cfg_attr(windows, allow(dead_code))]
fn handle_of(role: Role, key: &str) -> isize {
    mark_value(role, key) as isize
}

/// The name of the files of the window `handle` (elsewhere than Windows): its mark in hexadecimal.
#[cfg_attr(windows, allow(dead_code))]
fn name_of(handle: isize) -> String {
    format!("{:016x}", handle as usize)
}

/// The folder of the files of the signals elsewhere than Windows: `versekit` in the runtime folder of the session
/// (`runtime`, the variable `XDG_RUNTIME_DIR`), else a folder of the user `uid` in the temporary folder `temp`.
#[cfg_attr(windows, allow(dead_code))]
fn signal_folder(runtime: Option<&std::ffi::OsStr>, temp: &Path, uid: u32) -> PathBuf {
    match runtime.filter(|folder| !folder.is_empty()) {
        Some(folder) => Path::new(folder).join("versekit"),
        None => temp.join(format!("versekit-{uid}")),
    }
}

/// The process an update replaces, from the variable [`crate::update_swap::REPLACES_PID`].
#[cfg_attr(windows, allow(dead_code))]
fn replaced_pid(value: Option<&str>) -> Option<u32> {
    value.and_then(|v| v.trim().parse().ok()).filter(|&pid| pid != 0)
}

/// Takes the lock file `path` for this process (made if missing) and writes the process number to `pid`.
/// `Ok(None)`: another process holds it. The lock goes with the file handle, at the latest when the process ends.
#[cfg_attr(windows, allow(dead_code))]
fn lock_file(path: &Path, pid: &Path) -> std::io::Result<Option<std::fs::File>> {
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
    match file.try_lock() {
        Ok(()) => {
            // Written anew in one step: a reader never sees half a number. Without it the lock still counts.
            let mut fresh = pid.as_os_str().to_owned();
            fresh.push(".neu");
            if std::fs::write(&fresh, std::process::id().to_string()).is_ok() {
                let _ = std::fs::rename(&fresh, pid);
            }
            Ok(Some(file))
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

/// The process number written by [`lock_file`].
#[cfg_attr(windows, allow(dead_code))]
fn read_pid(path: &Path) -> Option<u32> {
    replaced_pid(std::fs::read_to_string(path).ok().as_deref())
}

/// How long a start waits for the end of the version it replaces: `pause` between the tries, this many tries.
#[cfg_attr(windows, allow(dead_code))]
const REPLACE_WAIT: (Duration, usize) = (Duration::from_millis(200), 300);

/// Takes the single instance: `take` tries to take it, `owner` names the process that holds it. Held by the
/// process this start replaces (`replaces`, an update), the start waits for its end, `pause` between the tries, at
/// most `tries` of them; it gives up when another process took it in the meantime. True when it is taken.
#[cfg_attr(windows, allow(dead_code))]
fn take_instance(
    mut take: impl FnMut() -> bool,
    owner: impl Fn() -> Option<u32>,
    replaces: Option<u32>,
    (pause, tries): (Duration, usize),
) -> bool {
    if take() {
        return true;
    }
    if replaces.is_none() || owner() != replaces {
        return false;
    }
    for _ in 0..tries {
        std::thread::sleep(pause);
        if take() {
            return true;
        }
        if owner().is_some_and(|pid| Some(pid) != replaces) {
            return false;
        }
    }
    false
}

/// Waits while the version a start replaces still answers (`replaced_answers`), `pause` between the looks, at most
/// `tries` of them. True when it is gone.
#[cfg_attr(windows, allow(dead_code))]
fn outlive(replaced_answers: impl Fn() -> bool, (pause, tries): (Duration, usize)) -> bool {
    for _ in 0..tries {
        if !replaced_answers() {
            return true;
        }
        std::thread::sleep(pause);
    }
    !replaced_answers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_or_minimized_window_is_shown_before_it_comes_to_the_front_and_a_shown_one_is_left() {
        assert!(needs_show(false, false), "hidden");
        assert!(needs_show(true, true), "minimized");
        assert!(needs_show(false, true));
        assert!(!needs_show(true, false), "shown windows are only raised");
    }

    #[test]
    fn only_known_codes_are_signals() {
        assert_eq!(Signal::from_code(Signal::Overview as usize), Some(Signal::Overview));
        assert_eq!(Signal::from_code(Signal::Settings as usize), Some(Signal::Settings));
        assert_eq!(Signal::from_code(Signal::Close as usize), Some(Signal::Close));
        for signal in [
            Signal::OpenGeneral,
            Signal::OpenTranslation,
            Signal::OpenAbout,
            Signal::AntiAfkOn,
            Signal::AntiAfkOff,
            Signal::OpenMining,
            Signal::ShowFrame,
            Signal::OpenStockScan,
        ] {
            assert_eq!(Signal::from_code(signal as usize), Some(signal));
        }
        assert_eq!(Signal::from_code(0), None);
        assert_eq!(Signal::from_code(12), None);
    }

    #[test]
    fn one_folder_in_any_spelling_is_one_key_and_two_folders_are_two() {
        let base = std::env::temp_dir().join(format!("versekit-schluessel-{}", crate::testdir::run_tag()));
        let (one, two) = (base.join("Kopie-A"), base.join("Kopie-B"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
        let key = folder_key(&text(&one));
        assert_eq!(key.len(), 12, "{key}");
        let mut spellings = vec![
            format!("{}{}", text(&one), std::path::MAIN_SEPARATOR),
            text(&base.join("Kopie-B").join("..").join("Kopie-A")),
        ];
        if cfg!(windows) {
            spellings.push(text(&one).to_uppercase());
            spellings.push(text(&one).replace('\\', "/"));
        }
        for spelling in &spellings {
            assert_eq!(folder_key(spelling), key, "{spelling}");
        }
        assert_ne!(folder_key(&text(&two)), key, "another copy is another program");
        // A folder that is not there (yet) still has its key, the same as once it is there.
        let later = base.join("Kopie-C");
        let before = folder_key(&text(&later));
        std::fs::create_dir_all(&later).unwrap();
        assert_eq!(folder_key(&text(&later)), before);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_users_own_data_has_the_empty_key_and_a_copy_wins_over_the_variable() {
        assert_eq!(key_for(None, None), "");
        assert_eq!(key_for(Some(""), Some("")), "", "an empty word names no folder");
        let copy = std::env::temp_dir().join("versekit-kopie-x").to_string_lossy().into_owned();
        let home = std::env::temp_dir().join("versekit-heim-y").to_string_lossy().into_owned();
        assert_eq!(key_for(Some(&copy), None), folder_key(&copy));
        assert_eq!(key_for(None, Some(&home)), folder_key(&home));
        assert_eq!(key_for(Some(&copy), Some(&home)), folder_key(&copy));
        // A process that never bound itself has a key that no run has.
        assert_eq!(key(), UNBOUND);
        assert!(!UNBOUND.is_empty() && UNBOUND.len() != 12);
    }

    #[test]
    fn the_mark_of_the_users_own_data_is_the_role_alone_and_every_key_marks_apart() {
        // The role alone: the mark of every build on the user's own data, before and after keys.
        assert_eq!(mark_value(Role::Main, ""), 1);
        assert_eq!(mark_value(Role::Overlay, ""), 2);
        let (a, b) = (folder_key("C:/a"), folder_key("C:/b"));
        let mut seen = std::collections::HashSet::new();
        for key in ["", UNBOUND, a.as_str(), b.as_str()] {
            for role in [Role::Main, Role::Overlay, Role::FirstBackup] {
                let mark = mark_value(role, key);
                assert_eq!(mark & 0xFF, role as usize, "the role stays in the lowest byte");
                assert!(seen.insert(mark), "{key:?} {role:?} marks like another");
            }
        }
    }

    #[test]
    fn every_signal_and_the_close_request_go_over_one_byte_and_back_and_no_other_byte_asks_anything() {
        let all = [
            Signal::Overview,
            Signal::Settings,
            Signal::Close,
            Signal::OpenGeneral,
            Signal::OpenTranslation,
            Signal::OpenAbout,
            Signal::AntiAfkOn,
            Signal::AntiAfkOff,
            Signal::OpenMining,
            Signal::ShowFrame,
            Signal::OpenStockScan,
        ];
        let mut bytes = std::collections::HashSet::new();
        for asked in all.map(Asked::Signal).into_iter().chain([Asked::Close]) {
            let byte = encode(asked);
            assert_eq!(decode(byte), Some(asked));
            assert!(bytes.insert(byte), "{asked:?} has a byte of its own");
        }
        assert_eq!(encode(Asked::Signal(Signal::Overview)), 1, "the codes of the signals");
        for byte in 0..=u8::MAX {
            assert_eq!(decode(byte).is_some(), bytes.contains(&byte), "{byte}");
        }
    }

    #[test]
    fn a_close_request_goes_to_the_hook_when_one_is_set_and_else_arrives_as_a_close_signal() {
        let got = std::sync::Mutex::new(Vec::new());
        let note = |signal: Signal| got.lock().unwrap().push(format!("{signal:?}"));
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&closed);
        let hook: CloseHook = Arc::new(move || flag.store(true, std::sync::atomic::Ordering::Relaxed));
        deliver(Asked::Close, Some(&hook), &note);
        assert!(closed.load(std::sync::atomic::Ordering::Relaxed), "the hook closes");
        assert!(got.lock().unwrap().is_empty(), "the handler hears nothing of it");
        deliver(Asked::Close, None, &note);
        deliver(Asked::Signal(Signal::Settings), Some(&hook), &note);
        assert_eq!(*got.lock().unwrap(), ["Close", "Settings"]);
    }

    #[test]
    fn the_files_of_a_role_are_named_by_its_mark_and_two_runs_or_two_roles_never_share_them() {
        let (a, b) = (folder_key("C:/Kopie-A"), folder_key("C:/Kopie-B"));
        assert_eq!(name_of(handle_of(Role::Main, "")), "0000000000000001", "the user's own data");
        assert_eq!(name_of(handle_of(Role::Overlay, "")), "0000000000000002");
        let mut names = std::collections::HashSet::new();
        for key in ["", UNBOUND, SAMPLE, a.as_str(), b.as_str()] {
            for role in [Role::Main, Role::Overlay, Role::FirstBackup] {
                let name = name_of(handle_of(role, key));
                assert_eq!(name.len(), 16, "{name}");
                assert!(name.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{name}");
                assert!(names.insert(name), "{key:?} {role:?}");
            }
        }
    }

    #[test]
    fn the_signals_live_in_the_runtime_folder_of_the_session_or_in_a_folder_of_the_user() {
        let temp = Path::new("/tmp");
        let runtime = std::ffi::OsStr::new("/run/user/1000");
        assert_eq!(signal_folder(Some(runtime), temp, 1000), Path::new("/run/user/1000").join("versekit"));
        assert_eq!(signal_folder(None, temp, 1000), temp.join("versekit-1000"));
        assert_eq!(signal_folder(Some(std::ffi::OsStr::new("")), temp, 7), temp.join("versekit-7"), "empty is none");
    }

    #[test]
    fn the_replaced_process_is_read_from_the_variable_and_nothing_else_is_one() {
        assert_eq!(replaced_pid(Some("4711")), Some(4711));
        assert_eq!(replaced_pid(Some(" 4711\n")), Some(4711));
        for wrong in [None, Some(""), Some("0"), Some("-3"), Some("x"), Some("4711x")] {
            assert_eq!(replaced_pid(wrong), None, "{wrong:?}");
        }
    }

    /// The single instance as two starts see it: the lock files of a test folder, never those of a program.
    #[test]
    fn the_first_start_takes_the_instance_a_second_finds_it_taken_and_once_the_first_ends_it_is_free() {
        let dir = crate::testdir::Dir::new("signal-instanz");
        let (lock, pid) = (dir.path().join("x.instanz"), dir.path().join("x.instanz-pid"));
        let first = lock_file(&lock, &pid).unwrap().expect("the first start takes it");
        assert_eq!(read_pid(&pid), Some(std::process::id()), "it names its process");
        assert!(lock_file(&lock, &pid).unwrap().is_none(), "a second start finds it taken");
        drop(first);
        assert!(lock_file(&lock, &pid).unwrap().is_some(), "free once the first one ends");
    }

    /// The tries of a start and the process that holds the instance, as `take_instance` sees them.
    fn start(free_after: usize, owners: &[Option<u32>], replaces: Option<u32>, tries: usize) -> (bool, usize) {
        let taken = std::cell::Cell::new(0);
        let looked = std::cell::Cell::new(0);
        let first = take_instance(
            || {
                taken.set(taken.get() + 1);
                taken.get() > free_after
            },
            || {
                looked.set(looked.get() + 1);
                owners[(looked.get() - 1).min(owners.len() - 1)]
            },
            replaces,
            (Duration::ZERO, tries),
        );
        (first, taken.get())
    }

    #[test]
    fn a_start_takes_a_free_instance_hands_over_to_a_running_one_and_waits_only_for_the_one_it_replaces() {
        assert_eq!(start(0, &[None], None, 10), (true, 1), "free: the first start");
        assert_eq!(start(usize::MAX, &[Some(5)], None, 10), (false, 1), "taken: hands over at once");
        assert_eq!(start(usize::MAX, &[Some(5)], Some(7), 10), (false, 1), "an update of another one hands over");
        // An update: the old version holds it until it has seen the new one live, then ends.
        assert_eq!(start(3, &[Some(7)], Some(7), 10), (true, 4), "waits for the end of the version it replaces");
        assert_eq!(start(usize::MAX, &[Some(7)], Some(7), 10), (false, 11), "an old version that stays: given up");
        // Another start took it in the meantime: that one is the program now.
        assert_eq!(start(usize::MAX, &[Some(7), Some(9)], Some(7), 10), (false, 2));
        // The number of the holder not yet written: waited on.
        assert_eq!(start(2, &[Some(7), None], Some(7), 10), (true, 3));
    }

    #[test]
    fn a_start_that_replaces_a_version_waits_until_its_overlay_no_longer_answers() {
        let counter = std::cell::Cell::new(0);
        let looks = &counter;
        let answers_for = |times: usize| {
            looks.set(0);
            move || {
                looks.set(looks.get() + 1);
                looks.get() <= times
            }
        };
        assert!(outlive(answers_for(0), (Duration::ZERO, 10)), "gone already");
        assert_eq!(looks.get(), 1);
        assert!(outlive(answers_for(3), (Duration::ZERO, 10)), "goes after three looks");
        assert_eq!(looks.get(), 4);
        assert!(!outlive(answers_for(usize::MAX), (Duration::ZERO, 10)), "one that stays: given up");
    }

    /// A real socket in the folder of the user, under a key of this test alone (the runs of the CI on Linux).
    #[cfg(not(windows))]
    #[test]
    fn a_served_role_is_found_reached_and_named_by_its_process_and_another_key_or_role_is_not() {
        let key = format!("pruefung-signal-{}", std::process::id());
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        assert!(serve_as(0, Role::Overlay, &key, move |signal| {
            let _ = tx.lock().unwrap().send(signal);
        }));
        let hwnd = find_as(Role::Overlay, &key).expect("served");
        assert_eq!(find_as(Role::Main, &key), None, "another role");
        assert_eq!(find_as(Role::Overlay, &format!("{key}-andere")), None, "another key");
        assert_eq!(process_of(hwnd), std::process::id());
        assert!(send(hwnd, Signal::Settings));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(Signal::Settings));
        assert!(!serve_as(0, Role::Overlay, &key, |_| {}), "a role is served by one process only");
        assert_eq!(process_of(handle_of(Role::Main, &key)), 0, "nothing serves it");
        for file in imp::files_of(hwnd) {
            let _ = std::fs::remove_file(file);
        }
        assert_eq!(find_as(Role::Overlay, &key), None, "without its socket file the role is gone");
    }

    /// Hidden top-level windows of this test (never shown, they take no focus) marked in two runs that stand
    /// in for two copies of the data.
    #[cfg(windows)]
    #[test]
    fn two_keys_keep_their_windows_apart_and_one_key_finds_its_own() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a plain window of a system class without parent, never shown; destroyed below.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        let (here, there) = ("pruefung-schluessel-a", "pruefung-schluessel-b");
        assert!(serve_as(window as isize, Role::Overlay, here, |_| {}));
        assert_eq!(find_as(Role::Overlay, here), Some(window as isize), "the same key finds its window");
        assert_eq!(find_as(Role::Overlay, there), None, "another key does not");
        assert_eq!(find_as(Role::Main, here), None, "nor another role");
        assert_eq!(find(Role::Overlay).filter(|found| *found == window as isize), None, "nor this test's own key");
        // SAFETY: the window made above.
        unsafe { DestroyWindow(window) };
        assert_eq!(find_as(Role::Overlay, here), None, "a destroyed window takes its mark with it");
    }

    /// Two main windows of one run (one the overlay started, one opened besides): both are found, and a window that
    /// was never shown counts as hidden. Hidden top-level windows of this test, never shown, taking no focus.
    #[cfg(windows)]
    #[test]
    fn every_main_window_of_a_run_is_found_and_a_hidden_one_is_told_apart() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let make = || {
            // SAFETY: a plain window of a system class without parent, never shown; destroyed below.
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!window.is_null());
            window
        };
        let key = "pruefung-zwei-hauptfenster";
        let (first, second) = (make(), make());
        assert!(serve_as(first as isize, Role::Main, key, |_| {}));
        assert!(serve_as(second as isize, Role::Main, key, |_| {}));
        let mut found = imp::find_all(Role::Main, key);
        found.sort_unstable();
        let mut made = vec![first as isize, second as isize];
        made.sort_unstable();
        assert_eq!(found, made, "both main windows of the run");
        assert!(imp::find(Role::Main, key).is_some_and(|one| made.contains(&one)), "find gives one of them");
        assert!(imp::find_all(Role::Overlay, key).is_empty(), "no window of another role");
        assert!(!is_shown(first as isize), "a window that was never shown is hidden");
        assert!(!is_shown(second as isize));
        // The other way round: a window that is shown counts as shown (else every main window would close with the
        // overlay). Only the style bit is set: nothing is drawn or activated, and the window lies far off the screen.
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GWL_STYLE, GetWindowLongPtrW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
                SetWindowPos, WS_VISIBLE,
            };
            let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
            // SAFETY: the window made above; it is moved off the screen first, then marked visible.
            unsafe {
                SetWindowPos(second, std::ptr::null_mut(), -32000, -32000, 0, 0, flags);
                SetWindowLongPtrW(second, GWL_STYLE, GetWindowLongPtrW(second, GWL_STYLE) | WS_VISIBLE as isize);
            }
        }
        assert!(is_shown(second as isize), "a shown window counts as shown");
        assert!(!is_shown(first as isize), "the other stays hidden");
        // SAFETY: the windows made above.
        unsafe {
            DestroyWindow(first);
            DestroyWindow(second);
        }
        assert!(imp::find_all(Role::Main, key).is_empty());
    }

    /// A hidden top-level window of a system class without parent, never shown.
    #[cfg(windows)]
    fn hidden_window() -> isize {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_POPUP};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a plain window of a system class without parent, never shown; the caller destroys it.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        window as isize
    }

    /// The window of the backup at the first start is found by its own role, apart from the main window and the
    /// overlay of the same run. Hidden windows of this test, never shown.
    #[cfg(windows)]
    #[test]
    fn the_window_of_the_first_backup_is_found_by_a_role_of_its_own() {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
        let key = "pruefung-erste-sicherung";
        let window = hidden_window();
        assert!(serve_as(window, Role::FirstBackup, key, |_| {}));
        assert_eq!(find_as(Role::FirstBackup, key), Some(window));
        assert_eq!(find_as(Role::Main, key), None, "no main window");
        assert_eq!(find_as(Role::Overlay, key), None, "no overlay");
        // SAFETY: the window made above.
        unsafe { DestroyWindow(window as _) };
        assert_eq!(find_as(Role::FirstBackup, key), None);
    }

    /// A window of a thread that takes its messages answers, a window that is gone does not. A hidden window of this
    /// test, never shown.
    #[cfg(windows)]
    #[test]
    fn a_window_that_takes_its_messages_answers_and_one_that_is_gone_does_not() {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
        // A window of this thread: the message goes to it directly.
        let own = hidden_window();
        assert!(answers(own), "a window of this thread answers");
        // SAFETY: the window made above.
        unsafe { DestroyWindow(own as _) };
        assert!(!answers(own), "a window that is gone does not answer");
    }

    /// A window whose thread stops taking its messages does not answer, and the question waits no longer than its
    /// time. A hidden window of this test, never shown.
    #[cfg(windows)]
    #[test]
    fn a_window_whose_thread_takes_no_messages_does_not_answer() {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
        // A window of a thread that sleeps instead of taking its messages, as a hanging program does.
        let (tx, rx) = std::sync::mpsc::channel();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let stuck = std::thread::spawn(move || {
            let window = hidden_window();
            tx.send(window).unwrap();
            let _ = held.recv_timeout(ANSWER_WAIT * 3);
            // SAFETY: the window made above, on this thread.
            unsafe { DestroyWindow(window as _) };
        });
        let window = rx.recv().unwrap();
        let asked = std::time::Instant::now();
        assert!(!answers(window), "a thread that takes no messages does not answer");
        assert!(asked.elapsed() < ANSWER_WAIT * 2, "the question waits no longer than its time");
        release.send(()).unwrap();
        stuck.join().unwrap();
    }
}
