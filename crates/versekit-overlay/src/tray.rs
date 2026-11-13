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

//! The icon next to the clock (the tray, `tray_icon.py`) with its menu and
//! the messages of the system.
//!
//! An icon of the notification area needs a window that Windows can send its
//! messages to; a click on the icon arrives there as a message of our own.
//! The window is invisible, never shown, never activated and does nothing
//! else. It needs a message loop of its own and so lives in a thread of its
//! own; what is to happen after a click is handed on as an [`Event`].
//!
//! ⚠ Everything here is meant kindly: when a step fails there is no icon and
//! the program goes on. A tool must not fail because an icon is missing.
//!
//! ⚠ The overlay is never touched: only the invisible window of the icon
//! takes the foreground, and only while its menu is open (without that the
//! menu would stay until a second click, a known special case of Windows).
//!
//! The shell is behind [`Shell`], so a self-check can put a recorder in its
//! place and nothing shows on a screen.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use versekit_core::language::{t, tf};
use versekit_core::window_signal::Signal;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HFONT, HGDIOBJ};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED, ODT_MENU};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW, IDI_APPLICATION, IMAGE_ICON, IsWindow,
    IsWindowVisible, LR_DEFAULTSIZE, LR_LOADFROMFILE, LoadIconW, LoadImageW, MENUINFO, MF_SEPARATOR, MF_STRING,
    MIM_BACKGROUND, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    SetMenuInfo, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY,
    WM_DRAWITEM, WM_LBUTTONUP, WM_MEASUREITEM, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use crate::tray_look;

/// The message of our own for everything the icon reports. `WM_APP` is the
/// range Windows leaves to programs.
pub const WM_TRAY: u32 = WM_APP + 17;

/// The number of the first entry of the menu; each further one has the next.
pub const COMMAND_FIRST: u32 = 1001;

/// The title of the program, where the page of its donations and its chat are.
const DISCORD: &str = "https://xharig.com/discord";
const DONATE: &str = "https://ko-fi.com/xharig";

/// What a click on an entry of the menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Bring the main window to the front.
    Open,
    /// The settings: the main window on its page "Allgemein".
    Settings,
    /// Start the game by the way the player has (4.12: "RSI Launcher starten"); only there while a way is known.
    Launcher,
    /// The translation: the main window on its page "Übersetzung".
    Translation,
    /// The chat of the program in a browser.
    Discord,
    /// The page for donations in a browser.
    Donate,
    /// The version and the way to the updates: the main window on its page "Update & Über".
    Version,
    /// Quit the overlay.
    Quit,
}

impl Entry {
    /// The number Windows reports for a choice of this entry: fixed per entry, so it does not matter
    /// whether the entry for the launcher is in the menu at the moment.
    pub fn command(self) -> u32 {
        COMMAND_FIRST
            + match self {
                Entry::Open => 0,
                Entry::Settings => 1,
                Entry::Launcher => 2,
                Entry::Translation => 3,
                Entry::Discord => 4,
                Entry::Donate => 5,
                Entry::Version => 6,
                Entry::Quit => 7,
            }
    }

    /// The address an entry opens, if it opens one.
    pub fn address(self) -> Option<&'static str> {
        match self {
            Entry::Discord => Some(DISCORD),
            Entry::Donate => Some(DONATE),
            _ => None,
        }
    }

    /// What the main window is asked to show for an entry, if it shows anything.
    pub fn signal(self) -> Option<Signal> {
        match self {
            Entry::Open => Some(Signal::Overview),
            Entry::Settings => Some(Signal::OpenGeneral),
            Entry::Translation => Some(Signal::OpenTranslation),
            Entry::Version => Some(Signal::OpenAbout),
            _ => None,
        }
    }
}

/// What the icon tells the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A click with the left button on the icon.
    Show,
    /// An entry of the menu was chosen.
    Pick(Entry),
}

/// One line of the menu: `None` is a separator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub command: u32,
    pub text: String,
    pub entry: Option<Entry>,
}

/// The menu as it stands now (in the language now), as in 4.12: open, settings | the launcher
/// (only while `launcher`: a way to start the game is known) and the translation | chat and
/// donations | version | quit, with separators between the groups.
///
/// ⚠ An entry for the launcher without a way to start would do nothing, and that is worse than none.
pub fn menu(version: &str, launcher: bool) -> Vec<Item> {
    let mut starting = Vec::new();
    if launcher {
        starting.push((Entry::Launcher, t("tray_launcher")));
    }
    starting.push((Entry::Translation, t("tray_uebersetzung")));
    let groups = [
        vec![(Entry::Open, t("tray_zeigen")), (Entry::Settings, t("tray_einstellungen"))],
        starting,
        vec![(Entry::Discord, t("tray_discord")), (Entry::Donate, t("hf_kofi"))],
        vec![(Entry::Version, tf("ov_a_tray_version", &[("0", version.into())]))],
        vec![(Entry::Quit, t("tray_beenden"))],
    ];
    let mut items = Vec::new();
    for (n, group) in groups.into_iter().enumerate() {
        if n > 0 {
            items.push(Item { command: 0, text: String::new(), entry: None });
        }
        for (entry, text) in group {
            items.push(Item { command: entry.command(), text, entry: Some(entry) });
        }
    }
    items
}

/// What is asked of the notification area: a call of `Shell_NotifyIconW`.
pub trait Shell: Send + Sync {
    /// `message` is `NIM_ADD`, `NIM_MODIFY` or `NIM_DELETE`; true if the shell took it.
    fn notify(&self, message: u32, data: &NOTIFYICONDATAW) -> bool;
}

/// The real shell of Windows.
pub struct System;

impl Shell for System {
    fn notify(&self, message: u32, data: &NOTIFYICONDATAW) -> bool {
        // SAFETY: `data` carries its size and lives for the call.
        unsafe { Shell_NotifyIconW(message, data) != 0 }
    }
}

/// What a recorder saw of one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub message: u32,
    pub flags: u32,
    pub tip: String,
    pub title: String,
    pub text: String,
}

/// The shell of the self-check: takes every call and writes it down; nothing shows.
#[derive(Default)]
pub struct Recorder {
    pub calls: Mutex<Vec<Call>>,
}

impl Recorder {
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn count(&self, message: u32) -> usize {
        self.calls().iter().filter(|c| c.message == message).count()
    }
}

impl Shell for Recorder {
    fn notify(&self, message: u32, data: &NOTIFYICONDATAW) -> bool {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push(Call {
            message,
            flags: data.uFlags,
            tip: text_of(&data.szTip),
            title: text_of(&data.szInfoTitle),
            text: text_of(&data.szInfo),
        });
        true
    }
}

/// A fixed field of UTF-16 as text, up to its first 0.
fn text_of(field: &[u16]) -> String {
    let end = field.iter().position(|&c| c == 0).unwrap_or(field.len());
    String::from_utf16_lossy(&field[..end])
}

/// Text into a fixed field, cut so that the 0 at the end still fits.
fn fill(field: &mut [u16], text: &str) {
    field.fill(0);
    let room = field.len().saturating_sub(1);
    for (slot, unit) in field.iter_mut().zip(text.encode_utf16().take(room)) {
        *slot = unit;
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// What the window procedure needs; there is one icon in a process.
struct Inner {
    sender: Sender<Event>,
    shell: Arc<dyn Shell>,
    hwnd: AtomicIsize,
    icon: AtomicIsize,
    tip: String,
    version: String,
    /// The icon is in the notification area.
    placed: AtomicBool,
    /// The message Windows sends when the taskbar comes back (0 before the thread has asked).
    taskbar_created: AtomicU32,
    /// Counter check: the icon stays when it is stopped.
    keep: bool,
    /// Counter check: a new taskbar does not bring the icon back.
    forget: bool,
    /// The icon was stopped: the thread places nothing any more and closes its window.
    stopped: AtomicBool,
    /// Counter check: the loaded icon is not given back.
    leak: bool,
    /// A way to start the game is known: the menu has the entry for the launcher.
    launcher: AtomicBool,
    /// The texts of the menu that is open, by the number each entry carries as its data (1 is the first; a
    /// separator carries 0), and the font it is drawn in; both only while it is open.
    open_menu: Mutex<Vec<String>>,
    menu_font: AtomicIsize,
}

static INNER: OnceLock<Mutex<Option<Arc<Inner>>>> = OnceLock::new();

fn current() -> Option<Arc<Inner>> {
    INNER.get()?.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

impl Inner {
    /// The structure Windows expects for the icon.
    fn data(&self, flags: u32) -> NOTIFYICONDATAW {
        // SAFETY: the structure is plain numbers and text fields; zero is a valid start.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd.load(Ordering::Relaxed) as HWND;
        data.uID = 1;
        data.uFlags = flags;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = self.icon.load(Ordering::Relaxed) as _;
        fill(&mut data.szTip, &self.tip);
        data
    }

    /// Puts the icon into the area, with tries: the call fails as long as the
    /// taskbar is not ready (at the autostart, right after an installation,
    /// at every restart of the Explorer).
    fn place(&self, tries: u32) -> bool {
        let data = self.data(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        for attempt in 0..tries {
            // Stopped meanwhile (the window is gone or going): no icon must be placed behind it.
            if self.stopped.load(Ordering::Relaxed) {
                return false;
            }
            if self.shell.notify(NIM_ADD, &data) {
                self.placed.store(true, Ordering::Relaxed);
                return true;
            }
            if attempt + 1 < tries {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        versekit_core::errors::record("tray_icon.create_icon", &format!("Shell_NotifyIcon failed {tries} times"));
        self.placed.store(false, Ordering::Relaxed);
        false
    }

    /// The menu of the right click; the choice comes back as `WM_COMMAND`. Its entries are drawn in the colors of
    /// the house (`tray_look`); without the font of the menus they are the plain ones of Windows.
    fn show_menu(&self, hwnd: HWND) {
        let font = tray_look::MenuFont::of_system();
        let items = menu(&self.version, self.launcher.load(Ordering::Relaxed));
        *self.open_menu.lock().unwrap_or_else(|e| e.into_inner()) = items.iter().map(|i| i.text.clone()).collect();
        self.menu_font.store(font.as_ref().map_or(0, |f| f.0 as isize), Ordering::Relaxed);
        let look = tray_look::Look::house();
        // SAFETY: the menu is made, used and destroyed here, by this thread.
        unsafe {
            let handle = CreatePopupMenu();
            if handle.is_null() {
                return;
            }
            for (n, item) in items.iter().enumerate() {
                let separator = item.entry.is_none();
                let text = wide(&item.text);
                let appended = if font.is_some() {
                    // An entry drawn here carries its number in the menu (1 is the first) as its data.
                    let data = if separator { 0 } else { n + 1 };
                    AppendMenuW(handle, tray_look::item_flags(separator), item.command as usize, data as *const u16)
                } else if separator {
                    AppendMenuW(handle, MF_SEPARATOR, 0, std::ptr::null())
                } else {
                    AppendMenuW(handle, MF_STRING, item.command as usize, text.as_ptr())
                };
                if appended == 0 {
                    versekit_core::errors::record(
                        "tray_icon.show_menu",
                        &format!("AppendMenuW failed for {}", item.text),
                    );
                }
            }
            // The ground around the entries (the edge inside the frame) in the color of the house as well.
            let ground = CreateSolidBrush(look.back);
            if font.is_some() {
                let info = MENUINFO { cbSize: std::mem::size_of::<MENUINFO>() as u32, fMask: MIM_BACKGROUND, hbrBack: ground, ..Default::default() };
                SetMenuInfo(handle, &info);
            }
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            // What had the foreground (the game): it gets it back when the menu is closed,
            // otherwise the invisible window keeps the keys until the next click.
            let before = GetForegroundWindow();
            // ⚠ Without this the menu stays until a second click (Q135788); the message
            // afterwards lets it close cleanly. Only the invisible window of the icon
            // takes the foreground, never the overlay.
            SetForegroundWindow(hwnd);
            TrackPopupMenu(handle, TPM_RIGHTBUTTON, at.x, at.y, 0, hwnd, std::ptr::null());
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(handle);
            DeleteObject(ground as HGDIOBJ);
            if !before.is_null() && before != hwnd && IsWindow(before) != 0 {
                SetForegroundWindow(before);
            }
        }
        self.menu_font.store(0, Ordering::Relaxed);
        self.open_menu.lock().unwrap_or_else(|e| e.into_inner()).clear();
        drop(font);
    }

    /// The text of the entry of the open menu that carries `data` as its number; `None` for a separator.
    fn open_text(&self, data: usize) -> Option<String> {
        let texts = self.open_menu.lock().unwrap_or_else(|e| e.into_inner());
        data.checked_sub(1).and_then(|n| texts.get(n)).filter(|t| !t.is_empty()).cloned()
    }

    /// `WM_MEASUREITEM` of an entry of the open menu; false for anything else.
    fn measure_item(&self, hwnd: HWND, item: &mut MEASUREITEMSTRUCT) -> bool {
        let font = self.menu_font.load(Ordering::Relaxed);
        if item.CtlType != ODT_MENU || font == 0 {
            return false;
        }
        let (width, height) = tray_look::measure_for(hwnd, font as HFONT, self.open_text(item.itemData).as_deref());
        item.itemWidth = width;
        item.itemHeight = height;
        true
    }

    /// `WM_DRAWITEM` of an entry of the open menu; false for anything else.
    fn draw_item(&self, item: &DRAWITEMSTRUCT) -> bool {
        let font = self.menu_font.load(Ordering::Relaxed);
        if item.CtlType != ODT_MENU || font == 0 {
            return false;
        }
        let text = self.open_text(item.itemData);
        let selected = item.itemState & ODS_SELECTED != 0;
        tray_look::draw(item.hDC, item.rcItem, font as HFONT, text.as_deref(), selected, &tray_look::Look::house());
        true
    }

    fn command(&self, command: u32) {
        // Every entry has its number whether the one for the launcher is in the menu or not.
        let entry = menu(&self.version, true).into_iter().find(|i| i.command == command).and_then(|i| i.entry);
        if let Some(entry) = entry {
            let _ = self.sender.send(Event::Pick(entry));
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Signing out or shutting down: an update at the end of the program must not start any more. The window lives
    // in a thread of its own and still answers while the program ends.
    versekit_core::auto_update::note_window_message(msg, wparam);
    if let Some(inner) = current() {
        match msg {
            WM_TRAY => {
                match (lparam & 0xFFFF) as u32 {
                    WM_LBUTTONUP => {
                        let _ = inner.sender.send(Event::Show);
                    }
                    WM_RBUTTONUP => inner.show_menu(hwnd),
                    _ => {}
                }
                return 0;
            }
            WM_COMMAND => {
                inner.command((wparam & 0xFFFF) as u32);
                return 0;
            }
            // The entries of the menu, drawn in the colors of the house.
            WM_MEASUREITEM if lparam != 0 => {
                // SAFETY: Windows hands the structure of the entry with this message, for the time of the call.
                let item = unsafe { &mut *(lparam as *mut MEASUREITEMSTRUCT) };
                if inner.measure_item(hwnd, item) {
                    return 1;
                }
            }
            WM_DRAWITEM if lparam != 0 => {
                // SAFETY: as above.
                let item = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
                if inner.draw_item(item) {
                    return 1;
                }
            }
            WM_DESTROY => {
                // SAFETY: ends the message loop of this thread.
                unsafe { PostQuitMessage(0) };
                return 0;
            }
            // The taskbar came back (the Explorer restarted): the icon is gone and must be placed again.
            // ⚠ Windows may send this message directly to the window (not through the queue), so it is
            // taken here and not in the loop.
            created if created != 0 && created == inner.taskbar_created.load(Ordering::Relaxed) => {
                if !inner.forget {
                    inner.place(3);
                }
                return 0;
            }
            _ => {}
        }
    }
    // SAFETY: the default handling for a message this procedure did not take.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// A folder to look for `icon.ico` in before the folder of the program (the self-check puts its own file there).
static ICON_FOLDER: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// How many loaded icons were given back with `DestroyIcon`.
static ICONS_GIVEN_BACK: AtomicU32 = AtomicU32::new(0);

/// Look for `icon.ico` in `folder` first.
pub fn use_icon_folder(folder: &std::path::Path) {
    *ICON_FOLDER.lock().unwrap_or_else(|e| e.into_inner()) = Some(folder.to_path_buf());
}

/// How many icons loaded from a file were given back so far.
pub fn icons_given_back() -> u32 {
    ICONS_GIVEN_BACK.load(Ordering::Relaxed)
}

/// The bytes of a valid icon file of 16 by 16 points, one colour: for the self-check and the tests.
pub fn sample_icon() -> Vec<u8> {
    let mut bytes = vec![0, 0, 1, 0, 1, 0];
    // One image of 16 x 16, 32 bits: size 40 + 1024 pixels + 64 of mask, at offset 22.
    bytes.extend([16, 16, 0, 0, 1, 0, 32, 0]);
    bytes.extend((40u32 + 1024 + 64).to_le_bytes());
    bytes.extend(22u32.to_le_bytes());
    bytes.extend(40u32.to_le_bytes());
    bytes.extend(16i32.to_le_bytes());
    bytes.extend(32i32.to_le_bytes());
    bytes.extend([1, 0, 32, 0]);
    bytes.extend([0; 24]);
    for _ in 0..256 {
        bytes.extend([0xC8, 0x8C, 0x28, 0xFF]);
    }
    bytes.extend([0; 64]);
    bytes
}

/// The number of the program icon in the resources of the program file (`winresource` gives it 1).
const OWN_ICON: usize = 1;

/// The program icon: the one in the resources of the program file, else the `icon.ico` of the
/// folder of the program or the one above it, else the standard one of Windows. The flag says that
/// the icon was loaded from a file and so must be given back with `DestroyIcon`; the other two
/// belong to the system.
fn load_icon() -> (isize, bool) {
    // SAFETY: the module of this program and the number of an icon in its resources.
    let own = unsafe { LoadIconW(GetModuleHandleW(std::ptr::null()), OWN_ICON as *const u16) };
    if !own.is_null() {
        return (own as isize, false);
    }
    let given = ICON_FOLDER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let folders = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .into_iter()
        .flat_map(|dir| [Some(dir.clone()), dir.parent().map(std::path::Path::to_path_buf)])
        .flatten();
    for folder in given.into_iter().chain(folders) {
        let file = folder.join("icon.ico");
        if file.is_file() {
            let name = wide(&file.to_string_lossy());
            // SAFETY: a plain file name that lives for the call.
            let icon = unsafe {
                LoadImageW(std::ptr::null_mut(), name.as_ptr(), IMAGE_ICON, 0, 0, LR_LOADFROMFILE | LR_DEFAULTSIZE)
            };
            if !icon.is_null() {
                return (icon as isize, true);
            }
        }
    }
    // SAFETY: loads a standard icon of the system.
    (unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) as isize }, false)
}

/// The icon next to the clock. Dropping it takes the icon away.
pub struct Tray {
    inner: Arc<Inner>,
    events: Receiver<Event>,
}

impl Tray {
    /// Starts the icon in a thread of its own. It is there once [`Tray::placed`] says so;
    /// a failure leaves the program without an icon, nothing more.
    pub fn start(shell: Arc<dyn Shell>, tip: &str, version: &str, counter: Option<&str>) -> Tray {
        let (sender, events) = mpsc::channel();
        let inner = Arc::new(Inner {
            sender,
            shell,
            hwnd: AtomicIsize::new(0),
            icon: AtomicIsize::new(0),
            tip: tip.to_string(),
            version: version.to_string(),
            placed: AtomicBool::new(false),
            taskbar_created: AtomicU32::new(0),
            keep: counter == Some("ohne_symbol_entfernen"),
            forget: counter == Some("ohne_neuanlage"),
            stopped: AtomicBool::new(false),
            leak: counter == Some("ohne_icon_freigeben"),
            launcher: AtomicBool::new(false),
            open_menu: Mutex::new(Vec::new()),
            menu_font: AtomicIsize::new(0),
        });
        *INNER.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&inner));
        let thread = Arc::clone(&inner);
        let started = std::thread::Builder::new().name("ablage".into()).spawn(move || run(&thread));
        if started.is_err() {
            versekit_core::errors::record("tray_icon.loop", &"the thread of the icon could not start");
        }
        Tray { inner, events }
    }

    /// The menu as it would be shown now.
    pub fn menu_now(&self) -> Vec<Item> {
        menu(&self.inner.version, self.inner.launcher.load(Ordering::Relaxed))
    }

    /// Whether the menu has the entry for the launcher: only while a way to start the game is known.
    pub fn set_launcher(&self, known: bool) {
        self.inner.launcher.store(known, Ordering::Relaxed);
    }

    /// What the icon reported since the last call.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// The invisible window of the icon; 0 before it exists.
    pub fn window(&self) -> isize {
        self.inner.hwnd.load(Ordering::Relaxed)
    }

    /// Is the icon in the area?
    pub fn placed(&self) -> bool {
        self.inner.placed.load(Ordering::Relaxed)
    }

    /// A message of the system at the icon. Windows 10 and 11 show it as a
    /// notification at the bottom right. False without an icon.
    pub fn notice(&self, title: &str, text: &str) -> bool {
        if !self.placed() {
            return false;
        }
        let mut data = self.inner.data(NIF_INFO);
        // A notice of the system is shown on the screen too: streamer mode covers it.
        let (title, text) = (versekit_core::streamer::scrub(title), versekit_core::streamer::scrub(text));
        fill(&mut data.szInfoTitle, &title);
        fill(&mut data.szInfo, if text.is_empty() { " " } else { &text });
        data.dwInfoFlags = NIIF_INFO;
        self.inner.shell.notify(NIM_MODIFY, &data)
    }

    /// Takes the icon away, or a dead shell stays next to the clock.
    ///
    /// ⚠ No `DestroyWindow` from here: Windows lets only the thread that made a window
    /// destroy it. A message ends the loop of the thread by itself.
    pub fn stop(&self) {
        if self.inner.keep {
            return;
        }
        // Also when the thread has no window yet: it sees the flag and closes the window it makes.
        self.inner.stopped.store(true, Ordering::Relaxed);
        let hwnd = self.inner.hwnd.swap(0, Ordering::Relaxed);
        if hwnd == 0 {
            return;
        }
        let mut data = self.inner.data(NIF_MESSAGE);
        data.hWnd = hwnd as HWND;
        self.inner.shell.notify(NIM_DELETE, &data);
        self.inner.placed.store(false, Ordering::Relaxed);
        // SAFETY: a plain message to the window of the icon.
        unsafe { PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0) };
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Is this window there and shown?
pub fn is_visible(hwnd: isize) -> bool {
    // SAFETY: only asks about a window.
    unsafe { IsWindow(hwnd as HWND) != 0 && IsWindowVisible(hwnd as HWND) != 0 }
}

/// Is there such a window?
pub fn exists(hwnd: isize) -> bool {
    // SAFETY: only asks about a window.
    unsafe { IsWindow(hwnd as HWND) != 0 }
}

/// The message Windows sends to every window when the taskbar comes back.
pub fn taskbar_created_message() -> u32 {
    let name = wide("TaskbarCreated");
    // SAFETY: a registered message by name.
    unsafe { RegisterWindowMessageW(name.as_ptr()) }
}

/// Puts a message into the queue of the window of the icon, as Windows would.
pub fn post(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> bool {
    // SAFETY: a plain message to a window.
    unsafe { PostMessageW(hwnd as HWND, message, wparam, lparam) != 0 }
}

/// The thread of the icon: class, window, icon, message loop.
fn run(inner: &Arc<Inner>) {
    let class = wide("VerseKitAblage");
    // SAFETY: class and window are made and used by this thread only; the strings live as long as the thread.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        // Not shown, not activatable: the window only receives messages.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            // Not silent: otherwise the report shows no error although the icon is missing.
            versekit_core::errors::record("tray_icon.window", &"CreateWindowExW made no window");
            return;
        }
        if inner.stopped.load(Ordering::Relaxed) {
            // Stopped before the window was there: nothing was placed, nothing stays.
            DestroyWindow(hwnd);
            return;
        }
        inner.hwnd.store(hwnd as isize, Ordering::Relaxed);
        inner.taskbar_created.store(taskbar_created_message(), Ordering::Relaxed);
        let (icon, from_file) = load_icon();
        inner.icon.store(icon, Ordering::Relaxed);
        inner.place(5);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // The window is gone and the icon was taken out of the area: the loaded icon is ours to give back.
        // Counter check: it stays loaded.
        if from_file && !inner.leak {
            inner.icon.store(0, Ordering::Relaxed);
            if DestroyIcon(icon as _) != 0 {
                ICONS_GIVEN_BACK.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    inner.placed.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window of the icon without a window: what the procedure needs to measure and draw the open menu.
    fn inner_with_menu(texts: &[&str], font: isize) -> Inner {
        let (sender, _events) = mpsc::channel();
        Inner {
            sender,
            shell: Arc::new(Recorder::default()),
            hwnd: AtomicIsize::new(0),
            icon: AtomicIsize::new(0),
            tip: String::new(),
            version: String::new(),
            placed: AtomicBool::new(false),
            taskbar_created: AtomicU32::new(0),
            keep: false,
            forget: false,
            stopped: AtomicBool::new(false),
            leak: false,
            launcher: AtomicBool::new(false),
            open_menu: Mutex::new(texts.iter().map(|t| t.to_string()).collect()),
            menu_font: AtomicIsize::new(font),
        }
    }

    /// An entry carries its number in the menu; a separator carries 0 and has no text. Only the entries of the
    /// open menu are measured and drawn here: another owner-drawn thing and a closed menu are left to Windows.
    #[test]
    fn the_entries_of_the_open_menu_are_found_by_their_number_and_only_they_are_drawn_here() {
        let font = tray_look::MenuFont::of_system().expect("the font of the menus");
        let inner = inner_with_menu(&["VerseKit zeigen", "", "Beenden"], font.0 as isize);
        assert_eq!(inner.open_text(1).as_deref(), Some("VerseKit zeigen"));
        assert_eq!(inner.open_text(2), None, "a separator");
        assert_eq!(inner.open_text(0), None);
        assert_eq!(inner.open_text(3).as_deref(), Some("Beenden"));
        assert_eq!(inner.open_text(9), None);
        // SAFETY: plain structures; zero is a valid start.
        let mut measured: MEASUREITEMSTRUCT = unsafe { std::mem::zeroed() };
        measured.CtlType = ODT_MENU;
        measured.itemData = 1;
        assert!(inner.measure_item(std::ptr::null_mut(), &mut measured));
        assert!(measured.itemWidth > 0 && measured.itemHeight > 0);
        measured.CtlType = ODT_MENU + 1;
        assert!(!inner.measure_item(std::ptr::null_mut(), &mut measured), "not a menu: Windows draws it");
        let closed = inner_with_menu(&["VerseKit zeigen"], 0);
        measured.CtlType = ODT_MENU;
        assert!(!closed.measure_item(std::ptr::null_mut(), &mut measured), "no menu open: nothing taken");
        // SAFETY: as above.
        let mut drawn: DRAWITEMSTRUCT = unsafe { std::mem::zeroed() };
        drawn.CtlType = ODT_MENU + 1;
        assert!(!inner.draw_item(&drawn));
        assert!(!closed.draw_item(&DRAWITEMSTRUCT { CtlType: ODT_MENU, ..drawn }));
    }

    #[test]
    fn the_menu_has_its_groups_and_numbers_each_entry_once() {
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let items = menu("5.0.0", false);
        let entries: Vec<Entry> = items.iter().filter_map(|i| i.entry).collect();
        assert_eq!(
            entries,
            [
                Entry::Open,
                Entry::Settings,
                Entry::Translation,
                Entry::Discord,
                Entry::Donate,
                Entry::Version,
                Entry::Quit
            ],
            "no entry for the launcher without a way to start"
        );
        // Separators between the groups (open, settings | translation | chat, donations | version | quit), none at the ends.
        assert_eq!(items.iter().filter(|i| i.entry.is_none()).count(), 4);
        assert!(items.first().is_some_and(|i| i.entry.is_some()) && items.last().is_some_and(|i| i.entry.is_some()));
        let mut commands: Vec<u32> = items.iter().filter(|i| i.entry.is_some()).map(|i| i.command).collect();
        let before = commands.clone();
        commands.sort_unstable();
        commands.dedup();
        assert_eq!(commands.len(), before.len(), "each entry has a number of its own");
        assert_eq!(Entry::Open.command(), COMMAND_FIRST);
        assert!(items.iter().any(|i| i.text == "Version 5.0.0 · Updates prüfen"));
        assert_eq!(Entry::Discord.address(), Some("https://xharig.com/discord"));
        assert_eq!(Entry::Open.address(), None);
    }

    #[test]
    fn the_entry_for_the_launcher_comes_with_a_way_to_start_and_keeps_every_number() {
        versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
        let without = menu("5.0.0", false);
        let with = menu("5.0.0", true);
        let entries: Vec<Entry> = with.iter().filter_map(|i| i.entry).collect();
        assert_eq!(
            entries,
            [
                Entry::Open,
                Entry::Settings,
                Entry::Launcher,
                Entry::Translation,
                Entry::Discord,
                Entry::Donate,
                Entry::Version,
                Entry::Quit
            ]
        );
        assert!(with.iter().any(|i| i.entry == Some(Entry::Launcher) && i.text == "RSI Launcher starten"));
        // The same entry has the same number with and without the launcher: a choice made while the
        // menu changes cannot become another entry.
        for item in without.iter().filter(|i| i.entry.is_some()) {
            assert!(with.iter().any(|w| w.entry == item.entry && w.command == item.command), "{:?}", item.entry);
        }
        assert_eq!(Entry::Launcher.signal(), None);
        assert_eq!(Entry::Launcher.address(), None);
        // With the launcher in the menu every entry still has a number of its own.
        let mut numbers: Vec<u32> = with.iter().filter(|i| i.entry.is_some()).map(|i| i.command).collect();
        let count = numbers.len();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), count, "two entries share a number");
        assert_eq!(Entry::Launcher.command(), COMMAND_FIRST + 2);
    }

    #[test]
    fn an_icon_loaded_from_a_file_is_a_real_icon_that_can_be_given_back() {
        let dir = std::env::temp_dir().join(format!("versekit-tray-icon-{}", versekit_core::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("folder");
        std::fs::write(dir.join("icon.ico"), sample_icon()).expect("icon file");
        use_icon_folder(&dir);
        let (icon, from_file) = load_icon();
        *ICON_FOLDER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(from_file && icon != 0, "the file icon is loaded");
        // SAFETY: the handle was made above and is given back once.
        assert_ne!(unsafe { DestroyIcon(icon as _) }, 0, "a loaded icon is destroyed");
        // SAFETY: a second time the handle is dead; the call must refuse it.
        assert_eq!(unsafe { DestroyIcon(icon as _) }, 0, "and it was alive until then");
    }

    #[test]
    fn the_entries_that_show_a_window_ask_for_their_page() {
        assert_eq!(Entry::Open.signal(), Some(Signal::Overview));
        assert_eq!(Entry::Settings.signal(), Some(Signal::OpenGeneral));
        assert_eq!(Entry::Translation.signal(), Some(Signal::OpenTranslation));
        assert_eq!(Entry::Version.signal(), Some(Signal::OpenAbout));
        for entry in [Entry::Discord, Entry::Donate, Entry::Quit] {
            assert_eq!(entry.signal(), None, "{entry:?} shows no page");
        }
    }

    #[test]
    fn fixed_fields_are_cut_so_that_the_end_mark_fits() {
        let mut field = [7u16; 4];
        fill(&mut field, "abcdef");
        assert_eq!(text_of(&field), "abc");
        let mut field = [7u16; 8];
        fill(&mut field, "ä");
        assert_eq!(text_of(&field[..2]), "ä");
    }
}
