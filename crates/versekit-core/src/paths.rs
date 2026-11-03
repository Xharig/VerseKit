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

//! Where things live, the only place that knows Windows and Linux paths.
//!
//! Two kinds of paths:
//!
//! 1. **Own files** (settings, collection, read position): visible under
//!    Documents, or at a location of the user's choosing that two pointer
//!    files point to.
//! 2. **Star Citizen itself** (Game.log and the backups): under Windows in
//!    the program folders, under Linux in a Wine prefix.
//!
//! Precedence when several sources are present: environment variable, then
//! `einstellungen.json`, then the search at the usual locations.
//!
//! All paths are text and follow the rules of `pypath`, so they stay
//! character-identical to what is stored in the files.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::errors;
use crate::given::{Fixed, Given, GivenBy};
use crate::jsonio::{self, Map, Newline, ReadError, Value};
use crate::legacy;
use crate::pypath::Flavor;
use crate::text::{lines_universal, py_isspace, py_strip};

/// Game channels in search order. LIVE first.
///
/// ⚠⚠ HOTFIX must be included: the usual way to a patched version is to
/// rename the LIVE folder to HOTFIX. If HOTFIX is missing here, the
/// configured game folder is gone from one day to the next.
pub const CHANNELS: [&str; 5] = ["LIVE", "HOTFIX", "PTU", "EPTU", "TECH-PREVIEW"];

/// ⚠⚠ Only these two share a blueprint collection. The test environments
/// run on separate save data; reading them too would claim a collection
/// that does not exist on LIVE.
pub const SHARED_STOCK_CHANNELS: [&str; 2] = ["LIVE", "HOTFIX"];

/// Which subfolder each own file belongs in. Anything missing here ends up in `Intern`.
pub const SUBFOLDERS: [(&str, &str); 16] = [
    ("bestand.json", "Bauplaene"),
    ("bestand.bak.json", "Bauplaene"),
    ("merkliste.json", "Bauplaene"),
    ("watchlist.json", "Bauplaene"),
    ("catalog-seen.json", "Bauplaene"),
    ("bp-overrides.json", "Bauplaene"),
    ("auftragslog.json", "Intern"),
    ("auftragslog.bak.json", "Intern"),
    ("einstellungen.json", "Einstellungen"),
    ("phrasen.json", "Einstellungen"),
    ("gesehen.json", "Einstellungen"),
    ("fehler.json", "Diagnose"),
    ("bericht.txt", "Diagnose"),
    ("update-setup.txt", "Diagnose"),
    ("update-helfer.txt", "Diagnose"),
    ("update-helfer.1.txt", "Diagnose"),
];

/// The subfolder of an own file.
pub fn subfolder(name: &str) -> &'static str {
    SUBFOLDERS.iter().find(|(n, _)| *n == name).map_or("Intern", |(_, f)| f)
}

/// Newly created default folder under Documents.
pub const NEW_FOLDER_NAME: &str = "Verse-Kit";

/// The window class (Wayland: app id) of every window of the program, the main window and the overlay alike.
/// ⚠ NOT the display name: on Linux the desktop entry (`StartupWMClass`) is matched against this, and two
/// windows of one program with different classes become two icons in the panel. A window built anew carries it
/// too, or it falls out of the grouping. ⚠ The value is the one of 4.12 (`paths.WM_CLASS`) and of the
/// packaging (`StartupWMClass` in `packaging/versekit.desktop`): entries in the start menu that 4.12 wrote
/// find the window by it, so it does not change.
pub const WM_CLASS: &str = "Versekit";
pub const SETTINGS_FILE: &str = "einstellungen.json";
/// Field of the pointer files holding the storage folder.
pub const STORAGE_KEY: &str = "ablage_ordner";
/// Field of the secondary copy: path of the visible pointer that could not
/// be written. While it is set, nothing is written there again.
pub const BLOCKED_KEY: &str = "zeiger_gesperrt";
/// Environment variable for a different game folder.
pub const ENV_INSTALL_DIR: &str = "SC_INSTALL_DIR";
/// Environment variable that puts every place of the user into one folder (`Host::contained`): cargo sets it for
/// every test run (`.cargo/config.toml`), so no test reaches the real configuration folder or Documents.
pub const ENV_TEST_MACHINE: &str = "VERSEKIT_TESTUMGEBUNG";

/// The folders chosen for the other channels on the page „Installation &
/// Pfade“, `{"PTU": folder, …}`. New in v5: 4.12 only finds them next to
/// the game folder. The page offers PTU and TECH-PREVIEW; HOTFIX and EPTU
/// count as well when entered by hand, LIVE never (that is `spiel_ordner`).
pub const SETTING_CHANNEL_FOLDERS: &str = "kanal_ordner";
/// The name of the game's archive in a channel folder.
pub const ARCHIVE_NAME: &str = "Data.p4k";

/// How long a read storage folder stays valid, as long as the pointers are unchanged.
const STORAGE_MEMO: Duration = Duration::from_secs(2);
/// A settings file that is younger than this counts as "being written right now".
pub const SETTINGS_FRESH_SEC: f64 = 2.0;
/// Locked settings file: how often to read, with this pause in between.
pub const SETTINGS_LOCK_TRIES: u32 = 3;
pub const SETTINGS_LOCK_PAUSE: Duration = Duration::from_millis(200);

/// Files with these extensions in `logbackups/` are never text.
const SKIP_BACKUP_EXT: [&str; 8] = [".zip", ".7z", ".gz", ".rar", ".dmp", ".mdmp", ".png", ".jpg"];

// ------------------------------------------------------------ File system

fn is_dir(p: &str) -> bool {
    !p.is_empty() && fs::metadata(p).is_ok_and(|m| m.is_dir())
}

/// An existing folder with nothing in it.
fn is_empty_dir(p: &str) -> bool {
    is_dir(p) && fs::read_dir(p).is_ok_and(|mut entries| entries.next().is_none())
}

pub(crate) fn is_file(p: &str) -> bool {
    !p.is_empty() && fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// Modification time in nanoseconds since 1970.
pub(crate) fn nanos(t: SystemTime) -> i128 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

/// Like `os.path.getmtime`: seconds as a float, computed as Python does
/// (`Sekunden + Nanosekunden * 1e-9`).
pub(crate) fn mtime_seconds(p: &str) -> Option<f64> {
    Some(seconds_of(fs::metadata(p).ok()?.modified().ok()?))
}

/// A time as seconds since 1970, computed the way `mtime_seconds` does.
pub(crate) fn seconds_of(t: SystemTime) -> f64 {
    let ns = nanos(t);
    let secs = ns.div_euclid(1_000_000_000);
    let rest = ns.rem_euclid(1_000_000_000);
    secs as f64 + rest as f64 * 1e-9
}

/// Like `time.time()`.
pub(crate) fn now_seconds() -> f64 {
    nanos(SystemTime::now()) as f64 / 1e9
}

/// Device and file number, detects a replacement even with identical size and time.
#[cfg(windows)]
pub(crate) fn file_identity(p: &str) -> (u64, u64) {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
    };
    let Ok(file) =
        fs::OpenOptions::new().access_mode(0x80).share_mode(7).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(p)
    else {
        return (0, 0);
    };
    // SAFETY: The structure consists only of numbers; GetFileInformationByHandle fills it.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: valid handle as long as `file` is alive.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return (0, 0);
    }
    (u64::from(info.dwVolumeSerialNumber), (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
}

#[cfg(unix)]
pub(crate) fn file_identity(p: &str) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(p).map_or((0, 0), |m| (m.dev(), m.ino()))
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn file_identity(_p: &str) -> (u64, u64) {
    (0, 0)
}

// ------------------------------------------------------------ Environment

/// What the program knows about the machine: path rules, environment
/// variables, working folder, whether the Windows registry is queried.
///
/// A snapshot taken at creation, like Python's `os.environ`; under Windows
/// variable names are case-insensitive.
#[derive(Clone, Debug)]
pub struct Host {
    pub flavor: Flavor,
    env: HashMap<String, String>,
    /// Query the registry (Documents folder, launcher). Windows only.
    pub registry: bool,
    /// Base for relative paths.
    pub cwd: String,
    /// The copy of the data the run is on (`--daten`, the full path), `None` on the user's own data. A command
    /// that starts this program again carries it (`desktop_entry::run_arguments`).
    pub copy: Option<String>,
}

impl Host {
    /// The machine the program runs on.
    pub fn current() -> Host {
        let flavor = Flavor::native();
        let mut host = Host {
            flavor,
            env: HashMap::new(),
            registry: cfg!(windows),
            cwd: std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            copy: None,
        };
        for (key, value) in std::env::vars_os() {
            if let (Ok(key), Ok(value)) = (key.into_string(), value.into_string()) {
                host.set_var(&key, &value);
            }
        }
        match host.var_set(ENV_TEST_MACHINE).map(String::from) {
            Some(root) => host.contained(&root),
            None => host,
        }
    }

    /// The machine with every place of the user inside `root`: home, Documents, the configuration and the local
    /// folders, the XDG folders, and no registry (it names the real Documents). What `ENV_TEST_MACHINE` gives.
    pub fn contained(mut self, root: &str) -> Host {
        let f = self.flavor;
        let places = [
            ("USERPROFILE", f.join(root, "home")),
            ("HOME", f.join(root, "home")),
            ("APPDATA", f.join_all(root, &["AppData", "Roaming"])),
            ("LOCALAPPDATA", f.join_all(root, &["AppData", "Local"])),
            ("XDG_CONFIG_HOME", f.join_all(root, &["home", ".config"])),
            ("XDG_DATA_HOME", f.join_all(root, &["home", ".local", "share"])),
            ("XDG_CACHE_HOME", f.join_all(root, &["home", ".cache"])),
        ];
        for (name, value) in places {
            self.set_var(name, &value);
        }
        self.registry = false;
        self
    }

    /// A made-up machine for tests. No registry.
    pub fn new(flavor: Flavor, vars: &[(&str, &str)], cwd: &str) -> Host {
        let mut host = Host { flavor, env: HashMap::new(), registry: false, cwd: cwd.to_string(), copy: None };
        for (key, value) in vars {
            host.set_var(key, value);
        }
        host
    }

    fn key(&self, name: &str) -> String {
        if self.flavor == Flavor::Windows { name.to_uppercase() } else { name.to_string() }
    }

    pub fn set_var(&mut self, name: &str, value: &str) {
        let key = self.key(name);
        self.env.insert(key, value.to_string());
    }

    pub fn remove_var(&mut self, name: &str) {
        let key = self.key(name);
        self.env.remove(&key);
    }

    /// The variable, even if it is empty (`name in os.environ`).
    pub fn var(&self, name: &str) -> Option<&str> {
        self.env.get(&self.key(name)).map(String::as_str)
    }

    /// The variable, only if it contains something (`os.environ.get(name)` as a
    /// truth value).
    pub fn var_set(&self, name: &str) -> Option<&str> {
        self.var(name).filter(|v| !v.is_empty())
    }

    /// The home directory (`os.path.expanduser('~')`).
    pub fn home(&self) -> String {
        self.expanduser("~")
    }

    /// Like `os.path.expanduser`.
    pub fn expanduser(&self, path: &str) -> String {
        if !path.starts_with('~') {
            return path.to_string();
        }
        let f = self.flavor;
        match f {
            Flavor::Windows => {
                let chars: Vec<char> = path.chars().collect();
                let i = (1..chars.len()).find(|&i| chars[i] == '\\' || chars[i] == '/').unwrap_or(chars.len());
                let mut userhome = if let Some(profile) = self.var("USERPROFILE") {
                    profile.to_string()
                } else if let Some(homepath) = self.var("HOMEPATH") {
                    f.join(self.var("HOMEDRIVE").unwrap_or(""), homepath)
                } else {
                    return path.to_string();
                };
                if i != 1 {
                    let target: String = chars[1..i].iter().collect();
                    let current = self.var("USERNAME");
                    if current != Some(target.as_str()) {
                        if current != Some(f.basename(&userhome).as_str()) {
                            return path.to_string();
                        }
                        userhome = f.join(&f.dirname(&userhome), &target);
                    }
                }
                let rest: String = chars[i..].iter().collect();
                format!("{userhome}{rest}")
            }
            Flavor::Posix => {
                let i = path[1..].find('/').map_or(path.len(), |i| i + 1);
                if i != 1 {
                    // `~name`: without a user database the path is left unchanged.
                    return path.to_string();
                }
                let userhome = match self.var("HOME") {
                    Some(home) => home.to_string(),
                    None => match posix_home_fallback() {
                        Some(home) => home,
                        None => return path.to_string(),
                    },
                };
                let joined = format!("{}{}", userhome.trim_end_matches('/'), &path[i..]);
                if joined.is_empty() { "/".into() } else { joined }
            }
        }
    }

    /// Like `ntpath.expandvars`: `%NAME%`, `$NAME`, `${NAME}`; nothing is
    /// replaced inside single quotes.
    pub fn expandvars(&self, path: &str) -> String {
        if !path.contains('$') && !path.contains('%') {
            return path.to_string();
        }
        let s: Vec<char> = path.chars().collect();
        let n = s.len();
        let text = |a: usize, b: usize| s[a.min(n)..b.min(n)].iter().collect::<String>();
        let find = |from: usize, c: char| (from..n).find(|&i| s[i] == c);
        let lookup = |name: &str| if name.is_empty() { None } else { self.var(name).map(String::from) };
        let mut out = String::new();
        let mut i = 0;
        while i < n {
            match s[i] {
                '\'' => match find(i + 1, '\'') {
                    Some(end) => {
                        out.push_str(&text(i, end + 1));
                        i = end + 1;
                    }
                    None => {
                        out.push_str(&text(i, n));
                        i = n;
                    }
                },
                '%' if s.get(i + 1) == Some(&'%') => {
                    out.push('%');
                    i += 2;
                }
                '%' => match find(i + 1, '%') {
                    Some(end) => {
                        let name = text(i + 1, end);
                        out.push_str(&lookup(&name).unwrap_or_else(|| format!("%{name}%")));
                        i = end + 1;
                    }
                    None => {
                        out.push_str(&text(i, n));
                        i = n;
                    }
                },
                '$' if s.get(i + 1) == Some(&'$') => {
                    out.push('$');
                    i += 2;
                }
                '$' if s.get(i + 1) == Some(&'{') => match find(i + 2, '}') {
                    Some(end) => {
                        let name = text(i + 2, end);
                        out.push_str(&lookup(&name).unwrap_or_else(|| format!("${{{name}}}")));
                        i = end + 1;
                    }
                    None => {
                        out.push_str(&text(i, n));
                        i = n;
                    }
                },
                '$' => {
                    let mut end = i + 1;
                    while end < n && (s[end].is_ascii_alphanumeric() || s[end] == '_' || s[end] == '-') {
                        end += 1;
                    }
                    let name = text(i + 1, end);
                    out.push_str(&lookup(&name).unwrap_or_else(|| format!("${name}")));
                    i = end;
                }
                c => {
                    out.push(c);
                    i += 1;
                }
            }
        }
        out
    }
}

#[cfg(unix)]
fn posix_home_fallback() -> Option<String> {
    // SAFETY: getpwuid returns a pointer to static memory or null.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy().into_owned())
    }
}

#[cfg(not(unix))]
fn posix_home_fallback() -> Option<String> {
    None
}

// ------------------------------------------------------------ Registry

/// A value from the registry, as far as `str(…)` makes it readable in Python.
#[cfg(windows)]
pub(crate) mod registry {
    use std::ptr::null_mut;

    use windows_sys::Win32::System::Registry::{
        HKEY, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_QWORD, REG_SZ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
        RegQueryInfoKeyW, RegQueryValueExW,
    };

    use crate::jsonio::wide;

    pub use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    pub struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: The key was opened with RegOpenKeyExW.
            unsafe { RegCloseKey(self.0) };
        }
    }

    pub fn open(root: HKEY, path: &str) -> Option<Key> {
        let name = wide(path);
        let mut key: HKEY = null_mut();
        // SAFETY: null-terminated name, result in `key`.
        let rc = unsafe { RegOpenKeyExW(root, name.as_ptr(), 0, KEY_READ, &mut key) };
        (rc == 0).then_some(Key(key))
    }

    impl Key {
        pub fn sub(&self, path: &str) -> Option<Key> {
            open(self.0, path)
        }

        pub fn subkey_count(&self) -> Option<u32> {
            let mut count = 0u32;
            let n = null_mut();
            // SAFETY: Only the number of subkeys is queried.
            let rc = unsafe {
                RegQueryInfoKeyW(self.0, null_mut(), n, null_mut(), &mut count, n, n, n, n, n, n, null_mut())
            };
            (rc == 0).then_some(count)
        }

        pub fn enum_key(&self, index: u32) -> Option<String> {
            let mut size = 256usize;
            loop {
                let mut buf = vec![0u16; size + 1];
                let mut len = buf.len() as u32;
                // SAFETY: buffer and length match.
                let rc = unsafe {
                    RegEnumKeyExW(
                        self.0,
                        index,
                        buf.as_mut_ptr(),
                        &mut len,
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                    )
                };
                match rc {
                    0 => return Some(String::from_utf16_lossy(&buf[..len as usize])),
                    234 if size < 1 << 16 => size *= 2,
                    _ => return None,
                }
            }
        }

        /// A value as text: strings up to the first null character, numbers in
        /// decimal, everything else empty. `None` if the value does not exist.
        pub fn value(&self, name: &str) -> Option<String> {
            let name = wide(name);
            let mut kind = 0u32;
            let mut size = 0u32;
            // SAFETY: first query only type and size.
            let rc = unsafe { RegQueryValueExW(self.0, name.as_ptr(), null_mut(), &mut kind, null_mut(), &mut size) };
            if rc != 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize + 2];
            let mut len = size;
            // SAFETY: the buffer is at least `len` bytes long.
            let rc =
                unsafe { RegQueryValueExW(self.0, name.as_ptr(), null_mut(), &mut kind, data.as_mut_ptr(), &mut len) };
            if rc != 0 {
                return None;
            }
            let data = &data[..len as usize];
            Some(match kind {
                REG_SZ | REG_EXPAND_SZ => {
                    let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
                    String::from_utf16_lossy(&units[..end])
                }
                REG_DWORD if data.len() >= 4 => u32::from_le_bytes([data[0], data[1], data[2], data[3]]).to_string(),
                REG_QWORD if data.len() >= 8 => u64::from_le_bytes(data[..8].try_into().expect("8 Bytes")).to_string(),
                _ => String::new(),
            })
        }
    }
}

/// The command of a start with the system named `name` under the user's `Run` key, `None` without one
/// (`autostart._win_on`).
#[cfg(windows)]
pub fn run_entry(name: &str) -> Option<String> {
    registry::open(registry::HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run")?.value(name)
}

/// The build number of Windows (`26200`), for the error report.
#[cfg(windows)]
pub fn windows_build_number() -> Option<String> {
    registry::open(registry::HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")?
        .value("CurrentBuildNumber")
        .filter(|build| !build.is_empty())
}

// ------------------------------------------------------------ State

/// What happened when a corrupt settings file was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovered {
    /// The previous version was restored.
    Backup,
    /// There was no usable one; the default values apply.
    Defaults,
}

type PointerStamp = Vec<(String, Option<(i128, u64)>)>;
type SettingsStamp = (String, u64, u64, i128, u64);
/// (game folder, modification time of each `logbackups` folder) → list.
type BackupsMemo = ((String, Vec<Option<i128>>), Vec<String>);

struct StorageMemo {
    stamp: PointerStamp,
    value: Option<String>,
    at: Instant,
}

#[derive(Default)]
struct State {
    storage_memo: Option<StorageMemo>,
    heal_failed: HashSet<(String, String)>,
    settings_memo: Option<(SettingsStamp, Map)>,
    still_locked: Option<String>,
    writeback_failed: bool,
    recovered: Option<Recovered>,
    log_backups: Option<BackupsMemo>,
    given: Given,
}

/// A game channel with a Game.log: name, folder, modification time of the Game.log.
pub type Channel = (String, String, f64);

/// All path queries in one place, with the caches of the Python version.
pub struct Paths {
    pub host: Host,
    state: Mutex<State>,
}

static GLOBAL: OnceLock<Paths> = OnceLock::new();

/// The paths of the running program.
pub fn global() -> &'static Paths {
    GLOBAL.get_or_init(|| Paths::new(Host::current()))
}

/// The data folder inside a copy of the data (see [`Paths::for_copy`]).
pub const COPY_DATA_FOLDER: &str = "VerseKit-Daten";

impl Paths {
    /// The paths of a copy of the user's data in `root`: `root` stands in for
    /// the home folder (`Dokumente` holds the visible pointer), `root/AppData`
    /// for the configuration folder (the second pointer). If the pointers lead
    /// outside the copy (it was taken on another machine), both are pointed
    /// at `root/VerseKit-Daten`. An empty `root` is a new copy (a first start
    /// on nothing): its data folder is `root/VerseKit-Daten` as well. `None`
    /// if the pointers lead outside a `root` that is neither empty nor holds
    /// that folder, or if the settings file still lies outside `root`.
    ///
    /// ⚠ Writes go through the normal paths; the caller locks writing to
    /// `root` first (`writeguard`).
    pub fn for_copy(base: Host, root: &str) -> Option<Paths> {
        let mut host = base;
        let flavor = host.flavor;
        let config = flavor.join(root, "AppData");
        for (name, value) in [("USERPROFILE", root), ("HOME", root), ("APPDATA", &config), ("XDG_CONFIG_HOME", &config)]
        {
            host.set_var(name, value);
        }
        host.remove_var(legacy::ENV_HOME);
        host.registry = false;
        host.copy = Some(flavor.abspath(root, &host.cwd));
        let full = |p: &str| flavor.normcase(&flavor.abspath(p, &host.cwd));
        let prefix = format!("{}{}", full(root), flavor.sep());
        let inside = |path: &str| full(path).starts_with(&prefix);
        let paths = Paths::new(host.clone());
        let data = flavor.join(root, COPY_DATA_FOLDER);
        // Looked at before the pointers are read or written.
        let empty = is_empty_dir(root);
        // Checked on the pointers themselves: asking for the settings file
        // first would create the storage folder where they point.
        let points_inside = [paths.pointer_file(), paths.second_pointer()]
            .iter()
            .all(|p| paths.read_pointer(p).is_some_and(|v| inside(&host.expanduser(&v))));
        if !points_inside {
            if !is_dir(&data) && !empty {
                return None;
            }
            paths.set_setting(STORAGE_KEY, Value::str(&data));
        }
        if !inside(&paths.settings_file()) {
            return None;
        }
        paths.set_given_data(&paths.app_folder(), GivenBy::CommandLine);
        Some(paths)
    }

    pub fn new(host: Host) -> Paths {
        Paths { host, state: Mutex::new(State::default()) }
    }

    /// The folders that were fixed from outside: by a tool through
    /// [`Paths::set_given_data`] and [`Paths::set_given_game`], by
    /// [`Paths::for_copy`], or by the environment variables for the data
    /// folder and the game folder.
    pub fn given(&self) -> Given {
        let set = self.state().given.clone();
        let data = set.data.or_else(|| {
            let value = self.host.var_set(legacy::ENV_HOME)?;
            Some(Fixed { folder: self.host.expanduser(value), by: GivenBy::Environment(legacy::ENV_HOME.into()) })
        });
        let game = set.game.or_else(|| {
            let value = self.host.var_set(ENV_INSTALL_DIR)?;
            let folder = self.game_folder().unwrap_or_else(|| self.host.expanduser(value));
            Some(Fixed { folder, by: GivenBy::Environment(ENV_INSTALL_DIR.into()) })
        });
        Given { data, game, archive: set.archive }
    }

    /// The same paths for a thread of its own (a move that runs next to the
    /// window): same machine, same folders given from outside, own caches.
    pub fn working_copy(&self) -> Paths {
        let copy = Paths::new(self.host.clone());
        copy.state().given = self.state().given.clone();
        copy
    }

    /// Fix the data folder from outside: the page shows who decided and
    /// offers neither choosing nor moving.
    pub fn set_given_data(&self, folder: &str, by: GivenBy) {
        self.state().given.data = Some(Fixed { folder: folder.to_string(), by });
    }

    /// Fix the game folder (LIVE) from outside.
    pub fn set_given_game(&self, folder: &str, by: GivenBy) {
        self.state().given.game = Some(Fixed { folder: folder.to_string(), by });
    }

    /// Fix the file `Data.p4k` from outside, where it does not lie in the game
    /// folder. [`Paths::archive_file`] then names it.
    pub fn set_given_archive(&self, file: &str, by: GivenBy) {
        self.state().given.archive = Some(Fixed { folder: file.to_string(), by });
    }

    /// The file `Data.p4k` to read: the one given from outside, else the one
    /// in the game folder. The file may not exist.
    pub fn archive_file(&self) -> Option<String> {
        if let Some(fixed) = self.state().given.archive.clone() {
            return Some(fixed.folder);
        }
        self.game_folder().map(|folder| self.join(&folder, ARCHIVE_NAME))
    }

    /// Hand the folders and the file back to the user.
    pub fn clear_given(&self) {
        self.state().given = Given::default();
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn join(&self, a: &str, b: &str) -> String {
        self.host.flavor.join(a, b)
    }

    fn join_all(&self, a: &str, parts: &[&str]) -> String {
        self.host.flavor.join_all(a, parts)
    }

    fn sc_subpath(&self) -> String {
        self.join("Roberts Space Industries", "StarCitizen")
    }

    /// Like `glob.glob(os.path.join(folder, '*'))`: all entries except hidden
    /// ones (name starts with `.`), in folder order.
    fn glob_star(&self, folder: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(if folder.is_empty() { "." } else { folder }) else { return Vec::new() };
        entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| !name.starts_with('.'))
            .map(|name| self.join(folder, &name))
            .collect()
    }

    // ------------------------------------------------------------ own files

    /// The user's Documents folder, or the home directory.
    pub fn documents(&self) -> String {
        let home = self.host.home();
        if self.host.flavor == Flavor::Windows {
            if let Some(value) = self.documents_from_registry() {
                let expanded = self.host.expandvars(&value);
                if !value.is_empty() && is_dir(&expanded) {
                    return expanded;
                }
            }
        } else {
            let base = match self.host.var_set("XDG_CONFIG_HOME") {
                Some(v) => v.to_string(),
                None => self.join(&home, ".config"),
            };
            if let Ok(jsonio_text) = jsonio::read_text(&self.join(&base, "user-dirs.dirs")) {
                for line in lines_universal(&jsonio_text) {
                    if !line.starts_with("XDG_DOCUMENTS_DIR") {
                        continue;
                    }
                    let Some((_, value)) = line.split_once('=') else { break };
                    let value = py_strip(value).trim_matches('"').replace("$HOME", &home);
                    if is_dir(&value) {
                        return value;
                    }
                }
            }
        }
        for name in ["Documents", "Dokumente"] {
            let p = self.join(&home, name);
            if is_dir(&p) {
                return p;
            }
        }
        home
    }

    #[cfg(windows)]
    fn documents_from_registry(&self) -> Option<String> {
        if !self.host.registry {
            return None;
        }
        registry::open(
            registry::HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders",
        )?
        .value("Personal")
    }

    #[cfg(not(windows))]
    fn documents_from_registry(&self) -> Option<String> {
        None
    }

    /// The default folder under Documents, the old one if it already exists.
    ///
    /// ⚠⚠ Never rename what is already there: the old folder holds the
    /// collection and settings, or the pointer to a custom location.
    pub fn default_folder(&self) -> String {
        let documents = self.documents();
        let old = self.join(&documents, legacy::DOCUMENTS_FOLDER);
        if is_dir(&old) {
            return old;
        }
        self.join(&documents, NEW_FOLDER_NAME)
    }

    /// The visible pointer file, always at the default location, never in the storage folder.
    pub fn pointer_file(&self) -> String {
        self.join_all(&self.default_folder(), &["Einstellungen", SETTINGS_FILE])
    }

    /// The second pointer file in the configuration folder, where nobody
    /// cleans up. Each heals the other: if one is missing, it is created from
    /// the other.
    pub fn second_pointer(&self) -> String {
        let base = match self.host.flavor {
            Flavor::Windows => self.host.var_set("APPDATA").map(String::from).unwrap_or_else(|| self.host.home()),
            Flavor::Posix => match self.host.var_set("XDG_CONFIG_HOME") {
                Some(v) => v.to_string(),
                None => self.join(&self.host.home(), ".config"),
            },
        };
        self.join_all(&base, &[legacy::CONFIG_FOLDER, "ablage.json"])
    }

    /// The lock file of the write lock that the processes of one run share (overlay, main window, small
    /// window): next to the second pointer, so outside the data folder (neither a backup nor a move takes it
    /// along, and a read-only or network data folder does not block it), named after the data folder, so a
    /// copy of the data or a test run has a lock of its own.
    pub fn save_lock_file(&self) -> String {
        let flavor = self.host.flavor;
        let key = crate::window_signal::folder_key(&self.app_folder());
        flavor.join(&flavor.dirname(&self.second_pointer()), &format!("sperre-{key}.lock"))
    }

    /// The lock file of the game files (`injection::game_file_lock`) that the processes of one run share, next to
    /// [`Self::save_lock_file`] and named like it.
    pub fn game_lock_file(&self) -> String {
        let flavor = self.host.flavor;
        let key = crate::window_signal::folder_key(&self.app_folder());
        flavor.join(&flavor.dirname(&self.second_pointer()), &format!("sperre-spiel-{key}.lock"))
    }

    /// From now on the write lock (`jsonio::lock_saves`) and the lock of the game files
    /// (`injection::game_file_lock`) hold against the other processes of this run too. Gives `false` where a lock
    /// file cannot be opened: that lock stays inside this process then.
    pub fn share_saves(&self) -> bool {
        let saves = jsonio::share_saves_between_processes(&self.save_lock_file());
        let games = crate::injection::share_game_files_between_processes(&self.game_lock_file());
        saves && games
    }

    /// The storage folder from a pointer file, or `None`.
    pub fn read_pointer(&self, path: &str) -> Option<String> {
        if !is_file(path) {
            return None;
        }
        let data = jsonio::read_json(path).ok()?;
        let value = data.as_object()?.iter().find(|(k, _)| k == STORAGE_KEY).map(|(_, v)| v)?.as_str()?;
        let value = py_strip(value);
        (!value.is_empty()).then(|| value.to_string())
    }

    /// Write a pointer silently (it is only the secondary copy). `blocked`
    /// records the path of a visible pointer that could not be written.
    ///
    /// ⚠ A leftover `.tmp` is not cleaned up.
    fn write_pointer(&self, path: &str, value: &Value, blocked: Option<&str>) -> bool {
        let result = (|| -> io::Result<()> {
            jsonio::make_dirs(&self.host.flavor.dirname(path))?;
            let temp = format!("{path}.tmp");
            let mut data: Map = vec![(STORAGE_KEY.into(), value.clone())];
            if let Some(blocked) = blocked.filter(|b| !b.is_empty()) {
                data.push((BLOCKED_KEY.into(), Value::str(blocked)));
            }
            jsonio::write_new(&temp, &jsonio::file_bytes(&Value::Object(data), 2))?;
            jsonio::replace_file(&temp, path)
        })();
        result.is_ok()
    }

    fn same_path(&self, a: &str, b: &str) -> bool {
        let f = self.host.flavor;
        f.normcase(&f.abspath(a, &self.host.cwd)) == f.normcase(&f.abspath(b, &self.host.cwd))
    }

    fn blocked_first(&self, second: &str, first: &str) -> bool {
        let Ok(data) = jsonio::read_json(second) else { return false };
        let Value::Object(map) = data else { return false };
        match map.iter().find(|(k, _)| k == BLOCKED_KEY).map(|(_, v)| v) {
            Some(Value::Str(value)) => self.same_path(value, first),
            _ => false,
        }
    }

    /// Is the visible pointer under Documents unwritable?
    pub fn first_pointer_blocked(&self) -> bool {
        self.blocked_first(&self.second_pointer(), &self.pointer_file())
    }

    /// Write the visible pointer and record in the secondary copy whether it
    /// worked.
    ///
    /// ⚠⚠ Under Windows, controlled folder access locks Documents against
    /// unknown programs; every attempt there produces an entry in the
    /// protection history. A failed attempt is therefore recorded.
    pub(crate) fn write_first_pointer(&self, value: &str) -> bool {
        let (first, second) = (self.pointer_file(), self.second_pointer());
        let ok = self.write_pointer(&first, &Value::str(value), None);
        if ok == self.blocked_first(&second, &first) {
            let keep = self.read_pointer(&second).unwrap_or_else(|| value.to_string());
            self.write_pointer(&second, &Value::Str(keep), if ok { None } else { Some(&first) });
        }
        ok
    }

    /// Read the custom storage folder from the two pointers.
    ///
    /// ⚠⚠ The visible one first: the player can change it by hand. If it is
    /// missing, the second one steps in and recreates the first. The folder
    /// must exist: a pointer into nothing would create an empty folder in the
    /// wrong place. If the visible one is blocked, the secondary copy takes
    /// precedence, and the visible one is not restored.
    fn storage_from_file(&self) -> Option<String> {
        let first = self.pointer_file();
        let second = self.second_pointer();
        let a = self.read_pointer(&first);
        let b = self.read_pointer(&second);
        let blocked = self.blocked_first(&second, &first);
        let mut candidates = [(a.clone(), second.clone()), (b.clone(), first.clone())];
        if blocked {
            candidates.reverse();
        }
        for (value, missing_at) in candidates {
            let Some(value) = value else { continue };
            if !is_dir(&self.host.expanduser(&value)) {
                // Points into nothing (disk not mounted): ask the other one.
                continue;
            }
            if self.read_pointer(&missing_at).as_deref() != Some(value.as_str()) {
                if missing_at == second {
                    self.write_pointer(&second, &Value::str(&value), if blocked { Some(&first) } else { None });
                } else if !blocked {
                    // Only one attempt per pointer and location per run.
                    let key = (first.clone(), value.clone());
                    let failed_before = self.state().heal_failed.contains(&key);
                    if !failed_before && !self.write_first_pointer(&value) {
                        self.state().heal_failed.insert(key);
                    }
                }
            }
            return Some(value);
        }
        // None is usable: return the first one unchanged so that a disk that is
        // just missing is not silently dropped.
        if blocked { b.or(a) } else { a.or(b) }
    }

    fn pointer_stamp(&self) -> PointerStamp {
        [self.pointer_file(), self.second_pointer()]
            .into_iter()
            .map(|name| {
                let info = fs::metadata(&name).ok().and_then(|m| Some((nanos(m.modified().ok()?), m.len())));
                (name, info)
            })
            .collect()
    }

    /// `storage_from_file`, cached while both pointers are unchanged, for at
    /// most two seconds.
    fn remembered_storage(&self) -> Option<String> {
        let stamp = self.pointer_stamp();
        let now = Instant::now();
        if let Some(memo) = &self.state().storage_memo
            && memo.stamp == stamp
            && now.duration_since(memo.at) < STORAGE_MEMO
        {
            return memo.value.clone();
        }
        let value = self.storage_from_file();
        // Re-stamp: a pointer may just have been healed.
        let stamp = self.pointer_stamp();
        self.state().storage_memo = Some(StorageMemo { stamp, value: value.clone(), at: now });
        value
    }

    /// Folder for the own files. Created on demand.
    pub fn app_folder(&self) -> String {
        let custom = match self.host.var_set(legacy::ENV_HOME) {
            Some(v) => Some(v.to_string()),
            None => self.remembered_storage(),
        };
        let p = match custom.filter(|c| !c.is_empty()) {
            Some(c) => self.host.expanduser(&c),
            None => self.default_folder(),
        };
        self.ensure_folder(&p);
        p
    }

    /// Create the folder if it is missing. True if it exists afterwards.
    ///
    /// ⚠ Check first, then create: an existing folder then costs only one
    /// `stat`.
    pub fn ensure_folder(&self, folder: &str) -> bool {
        is_dir(folder) || jsonio::make_dirs(folder).is_ok()
    }

    /// Full path to an own file, sorted by `SUBFOLDERS`. With a custom data
    /// folder via environment variable everything is flat.
    pub fn app_file(&self, name: &str) -> String {
        let base = self.app_folder();
        if self.host.var_set(legacy::ENV_HOME).is_some() {
            return self.join(&base, name);
        }
        let target = self.join(&base, subfolder(name));
        let folder = if self.ensure_folder(&target) { target } else { base };
        self.join(&folder, name)
    }

    // ------------------------------------------------------------ Settings

    pub fn settings_file(&self) -> String {
        self.app_file(SETTINGS_FILE)
    }

    /// Read a JSON object; a BOM at the start is allowed (`utf-8-sig`).
    fn read_settings_file(path: &str) -> Result<Map, ReadError> {
        let text = jsonio::read_text(path)?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        match jsonio::loads(text)? {
            Value::Object(map) => Ok(map),
            _ => Err(ReadError::Value("kein JSON-Objekt".into())),
        }
    }

    /// If locked, read up to `SETTINGS_LOCK_TRIES` times. If the same file was
    /// already locked last time, only once.
    fn read_settings_waiting(&self, path: &str) -> Result<Map, ReadError> {
        let tries = if self.state().still_locked.as_deref() == Some(path) { 1 } else { SETTINGS_LOCK_TRIES.max(1) };
        let mut attempt = 0;
        loop {
            match Self::read_settings_file(path) {
                Err(ReadError::Os(e)) if jsonio::is_permission(&e) => {
                    attempt += 1;
                    if attempt >= tries {
                        self.state().still_locked = Some(path.to_string());
                        return Err(ReadError::Os(e));
                    }
                    thread::sleep(SETTINGS_LOCK_PAUSE);
                }
                other => {
                    self.state().still_locked = None;
                    return other;
                }
            }
        }
    }

    /// Younger than `SETTINGS_FRESH_SEC`? A time in the future does not count.
    fn just_written(path: &str) -> bool {
        mtime_seconds(path).is_some_and(|t| {
            let age = now_seconds() - t;
            (0.0..SETTINGS_FRESH_SEC).contains(&age)
        })
    }

    /// (data, readable). Locked or just written means not readable, then
    /// `set_setting` must not overwrite anything.
    pub fn settings_read(&self) -> (Map, bool) {
        let target = self.settings_file();
        match self.read_settings_waiting(&target) {
            Ok(data) => (data, true),
            Err(ReadError::NotFound) => {
                if !self.state().writeback_failed {
                    return (Vec::new(), true);
                }
                match Self::read_settings_file(&jsonio::backup_name(&target)) {
                    Ok(data) => (data, true),
                    Err(ReadError::NotFound | ReadError::Value(_)) => (Vec::new(), true),
                    Err(e) => {
                        errors::record("paths.settings", &e);
                        (Vec::new(), false)
                    }
                }
            }
            Err(ReadError::Value(message)) => {
                if Self::just_written(&target) {
                    return (Vec::new(), false);
                }
                self.settings_recover(&target, &message)
            }
            Err(e) => {
                errors::record("paths.settings", &e);
                (Vec::new(), false)
            }
        }
    }

    /// `einstellungen.kaputt-<Zeit>.json`, with a counter if the name already exists.
    fn aside_name(target: &str) -> String {
        let stem: String = target
            .strip_suffix(".json")
            .map_or_else(|| target.chars().take(target.chars().count().saturating_sub(5)).collect(), String::from);
        let stem = format!("{stem}.kaputt-{}", jsonio::time_stamp());
        let mut name = format!("{stem}.json");
        let mut number = 1;
        while fs::metadata(&name).is_ok() {
            number += 1;
            name = format!("{stem}-{number}.json");
        }
        name
    }

    /// Set a corrupt settings file aside and put the previous version back in
    /// place; if there is none, the default values apply.
    ///
    /// ⚠⚠ Every failure here means *not readable*: otherwise `set_setting`
    /// would write the template over the corrupt file, and `save_json` would
    /// first copy it over the good previous version.
    fn settings_recover(&self, target: &str, broken: &str) -> (Map, bool) {
        let _lock = jsonio::lock_saves();
        match Self::read_settings_file(target) {
            // Another thread may have already done it.
            Ok(data) => return (data, true),
            Err(ReadError::NotFound) => {}
            Err(ReadError::Value(_)) => {
                errors::record("paths.settings", &broken);
                if let Err(e) = jsonio::replace_file(target, &Self::aside_name(target)) {
                    errors::record("paths.settings", &e);
                    return (Vec::new(), false);
                }
            }
            Err(e) => {
                errors::record("paths.settings", &e);
                return (Vec::new(), false);
            }
        }
        let data = match Self::read_settings_file(&jsonio::backup_name(target)) {
            Ok(data) => data,
            Err(ReadError::NotFound) => {
                self.state().recovered = Some(Recovered::Defaults);
                return (Vec::new(), true);
            }
            Err(ReadError::Value(e)) => {
                errors::record("paths.settings", &e);
                self.state().recovered = Some(Recovered::Defaults);
                return (Vec::new(), true);
            }
            Err(e) => {
                // Previous version locked: not *default values*, otherwise the template
                // would end up as the main file over everything.
                errors::record("paths.settings", &e);
                self.state().writeback_failed = true;
                return (Vec::new(), false);
            }
        };
        let text = jsonio::dumps(&Value::Object(data.clone()), Some(2), false);
        match jsonio::write_text_atomic(target, &text, Newline::Native) {
            Ok(()) => self.state().writeback_failed = false,
            Err(e) => {
                // The main file is then missing; `settings_read` reads the previous
                // version in that case.
                errors::record("paths.settings", &e);
                self.state().writeback_failed = true;
            }
        }
        self.state().recovered = Some(Recovered::Backup);
        (data, true)
    }

    /// What happened the last time a corrupt settings file was read.
    pub fn settings_recovered(&self) -> Option<Recovered> {
        self.state().recovered
    }

    pub fn clear_settings_recovered(&self) {
        self.state().recovered = None;
    }

    /// Stamp of the settings file: none if it is missing or younger than
    /// `SETTINGS_FRESH_SEC` (also: time in the future).
    fn settings_stamp(target: &str) -> Option<SettingsStamp> {
        let meta = fs::metadata(target).ok()?;
        let mtime = nanos(meta.modified().ok()?);
        if now_seconds() - mtime as f64 / 1e9 < SETTINGS_FRESH_SEC {
            return None;
        }
        let (dev, ino) = file_identity(target);
        Some((target.to_string(), dev, ino, mtime, meta.len()))
    }

    /// Forgets the cached settings: the next [`Self::settings`] reads the
    /// file whatever its stamp says. For a change another program reported;
    /// on a network share the stamp can lag behind the content.
    pub fn forget_settings(&self) {
        self.state().settings_memo = None;
    }

    /// The settings entered by the user; if the file is missing, they are empty.
    ///
    /// Re-read only if the stamp has changed. Only what was readable is
    /// cached.
    pub fn settings(&self) -> Map {
        let target = self.settings_file();
        let stamp = Self::settings_stamp(&target);
        if let Some(stamp) = &stamp
            && let Some((memo_stamp, data)) = &self.state().settings_memo
            && memo_stamp == stamp
        {
            return data.clone();
        }
        let (data, readable) = self.settings_read();
        if readable && let Some(stamp) = stamp {
            self.state().settings_memo = Some((stamp, data.clone()));
        }
        data
    }

    fn settings_value(&self, name: &str) -> Option<Value> {
        self.settings().into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// A path set by the user, or `None`.
    ///
    /// ⚠⚠ Anything that is not text counts as unset.
    pub fn setting(&self, name: &str) -> Option<String> {
        let Some(Value::Str(value)) = self.settings_value(name) else { return None };
        let value = py_strip(&value);
        (!value.is_empty()).then(|| self.host.expanduser(value))
    }

    /// A number from the settings, clamped to the limits.
    ///
    /// Converted like Python's `int()`: true/false as 1/0, floats truncated
    /// toward zero, text with whitespace and `_` between digits. Anything that
    /// cannot be converted yields the default.
    pub fn setting_int(&self, name: &str, default: i64, minimum: Option<i64>, maximum: Option<i64>) -> i64 {
        let Some(mut number) = self.settings_value(name).as_ref().and_then(py_int) else { return default };
        if let Some(min) = minimum {
            number = number.max(i128::from(min));
        }
        if let Some(max) = maximum {
            number = number.min(i128::from(max));
        }
        number.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    /// A yes/no from the settings. If missing, the default applies.
    pub fn setting_bool(&self, name: &str, default: bool) -> bool {
        match self.settings_value(name) {
            Some(Value::Bool(b)) => b,
            Some(Value::Str(s)) => {
                matches!(py_strip(&s).to_lowercase().as_str(), "ja" | "yes" | "true" | "1" | "an" | "on")
            }
            _ => default,
        }
    }

    /// Persist a value without losing the explanatory lines. Returns whether
    /// it worked; a failure is reported.
    pub fn set_setting(&self, name: &str, value: Value) -> bool {
        if name == STORAGE_KEY {
            // ⚠ This field belongs in the pointer file, otherwise the change only
            // lasts until restart.
            return self.set_storage_folder(&value);
        }
        let target = self.settings_file();
        // Wait out a short lock before taking the write lock.
        let _ = self.read_settings_waiting(&target);
        let result = (|| -> io::Result<()> {
            let _lock = jsonio::lock_saves();
            let (data, readable) = self.settings_read();
            if !readable {
                // ⚠ Locked does not mean empty: with the template written over it, all
                // other settings would be gone.
                return Err(io::Error::other("Einstellungsdatei gerade nicht lesbar"));
            }
            let mut data = if data.is_empty() { self.template() } else { data };
            jsonio::map_set(&mut data, name, value);
            jsonio::save_json(&target, &Value::Object(data), 2, false)
        })();
        match result {
            Ok(()) => true,
            Err(e) => {
                errors::record("paths.set_setting", &e);
                false
            }
        }
    }

    /// Write the storage folder to both pointer files, not into the storage
    /// folder.
    ///
    /// ⚠⚠ Write the second one even if the first fails: the first lives under
    /// Documents, where controlled folder access can lock it out.
    fn set_storage_folder(&self, value: &Value) -> bool {
        let target = self.pointer_file();
        let mut existing: Map = match jsonio::read_json(&target) {
            Ok(Value::Object(map)) => map,
            _ => Vec::new(),
        };
        jsonio::map_set(&mut existing, STORAGE_KEY, value.clone());
        let temp = format!("{target}.tmp");
        let first = (|| -> io::Result<()> {
            jsonio::make_dirs(&self.host.flavor.dirname(&target))?;
            jsonio::write_new(&temp, &jsonio::file_bytes(&Value::Object(existing), 2))?;
            jsonio::replace_file(&temp, &target)
        })();
        if let Err(e) = &first {
            errors::record("paths._set_storage_folder", e);
        }
        let second =
            self.write_pointer(&self.second_pointer(), value, if first.is_ok() { None } else { Some(&target) });
        first.is_ok() || second
    }

    /// The content of a new settings file: the settings with their default
    /// values, without explanatory lines.
    pub fn template(&self) -> Map {
        let s = Value::str;
        vec![
            ("spiel_ordner".into(), s("")),
            ("sprache".into(), s("auto")),
            ("pruefintervall_sekunden".into(), Value::int(3)),
            ("signalton".into(), Value::Bool(true)),
            ("deckkraft_prozent".into(), Value::int(93)),
        ]
    }

    /// Creates the settings file to be filled in, if it is missing.
    pub fn create_template(&self) -> String {
        let target = self.settings_file();
        if fs::metadata(&target).is_err() {
            let _ = jsonio::write_new(&target, &jsonio::file_bytes(&Value::Object(self.template()), 2));
        }
        target
    }

    // ------------------------------------------------------------ Star Citizen

    /// Possible Wine prefixes: lug-helper first, then Lutris, Bottles, Games; behind them the places 4.12 did not
    /// know: Bottles as a Flatpak, Heroic, and Steam with Proton.
    pub fn wine_prefixes(&self) -> Vec<String> {
        let home = self.host.home();
        let mut found = vec![
            self.join_all(&home, &["Games", "star-citizen"]),
            self.join(&home, ".wine"),
            self.join_all(&home, &["Games", "star-citizen-live"]),
        ];
        for parts in [
            &[".local", "share", "lutris", "prefixes"][..],
            &[".var", "app", "net.lutris.Lutris", "data", "lutris", "prefixes"],
            &[".local", "share", "bottles", "bottles"],
            &["Games"],
            &[".var", "app", "com.usebottles.bottles", "data", "bottles", "bottles"],
        ] {
            let mut hits = self.glob_star(&self.join_all(&home, parts));
            hits.sort();
            found.extend(hits);
        }
        // Heroic gives each game a prefix of its own, also as a Flatpak; with Proton the Wine folder is `pfx`
        // inside it. Steam keeps one prefix per game in `compatdata`, always in `pfx`.
        for (parts, inner_only) in [
            (&["Games", "Heroic", "Prefixes"][..], false),
            (&[".local", "share", "Steam", "steamapps", "compatdata"], true),
            (&[".var", "app", "com.valvesoftware.Steam", "data", "Steam", "steamapps", "compatdata"], true),
        ] {
            let mut hits = self.glob_star(&self.join_all(&home, parts));
            hits.sort();
            for hit in hits {
                let inner = self.join(&hit, "pfx");
                if !inner_only {
                    found.push(hit);
                }
                if is_dir(&inner) {
                    found.push(inner);
                }
            }
        }
        let mut seen = HashSet::new();
        found.retain(|p| seen.insert(p.clone()));
        found
    }

    /// Directories under which `Roberts Space Industries/StarCitizen` can be located.
    pub fn game_roots(&self) -> Vec<String> {
        if self.host.flavor == Flavor::Windows {
            let mut roots = Vec::new();
            for drive in "CDEFGH".chars() {
                for programs in ["Program Files", "Program Files (x86)"] {
                    roots.push(format!("{drive}:\\{programs}"));
                }
            }
            return roots;
        }
        let mut roots = Vec::new();
        for prefix in self.wine_prefixes() {
            let c = self.join(&prefix, "drive_c");
            if !is_dir(&c) {
                continue;
            }
            roots.push(self.join(&c, "Program Files"));
            roots.push(self.join(&c, "Program Files (x86)"));
            roots.push(c);
        }
        roots
    }

    /// Folder of the game channel (contains the Game.log) or `None`.
    ///
    /// First the environment variable, then the setting, then the usual
    /// locations. It only checks whether the Game.log is there.
    ///
    /// ⛔ A configured folder without a Game.log is skipped: the real
    /// installation applies then. Not so a game folder given from outside (a
    /// test copy, by a tool or the variable): only it, its channels and the
    /// ones next to it count, else there is none. On purpose unlike 4.12,
    /// which falls back to the real installation then.
    pub fn game_folder(&self) -> Option<String> {
        let f = self.host.flavor;
        let given = self.given_game_folder();
        let only_given = given.is_some();
        let customs = if only_given { [given, None] } else { [None, self.setting("spiel_ordner")] };
        for custom in customs.into_iter().flatten() {
            let custom = self.host.expanduser(&custom);
            if is_file(&self.join(&custom, "Game.log")) {
                return Some(custom);
            }
            for k in CHANNELS {
                let p = self.join(&custom, k);
                if is_file(&self.join(&p, "Game.log")) {
                    return Some(p);
                }
            }
            // ⚠ And next to it: LIVE may have been renamed to HOTFIX.
            let parent = f.dirname(custom.trim_end_matches(f.sep()));
            if !parent.is_empty() && is_dir(&parent) {
                let mut siblings: Vec<(f64, String)> = Vec::new();
                for k in CHANNELS {
                    let p = self.join(&parent, k);
                    let log = self.join(&p, "Game.log");
                    if is_file(&log) {
                        siblings.push((mtime_seconds(&log).unwrap_or(0.0), p));
                    }
                }
                // The most recently played channel wins.
                if let Some(best) = siblings.into_iter().max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
                {
                    return Some(best.1);
                }
            }
        }
        if only_given {
            return None;
        }
        for root in self.game_roots() {
            let base = self.join(&root, &self.sc_subpath());
            if !is_dir(&base) {
                continue;
            }
            for k in CHANNELS {
                let p = self.join(&base, k);
                if is_file(&self.join(&p, "Game.log")) {
                    return Some(p);
                }
            }
        }
        None
    }

    /// The locations that are searched, existing ones first, at most `limit`.
    pub fn searched_game_locations(&self, limit: usize) -> Vec<String> {
        let mut candidates: Vec<String> = Vec::new();
        for root in self.game_roots() {
            let p = self.join_all(&root, &[&self.sc_subpath(), CHANNELS[0]]);
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
        if candidates.is_empty() {
            let home = self.host.home();
            if self.host.flavor == Flavor::Windows {
                candidates.push(self.join_all("C:\\Program Files", &[&self.sc_subpath(), CHANNELS[0]]));
            } else {
                for prefix in [self.join_all(&home, &["Games", "star-citizen"]), self.join(&home, ".wine")] {
                    candidates
                        .push(self.join_all(&prefix, &["drive_c", "Program Files", &self.sc_subpath(), CHANNELS[0]]));
                }
                candidates.push(self.join_all(
                    &home,
                    &[
                        ".local",
                        "share",
                        "lutris",
                        "prefixes",
                        "<Name>",
                        "drive_c",
                        "Program Files",
                        &self.sc_subpath(),
                        CHANNELS[0],
                    ],
                ));
            }
        }
        let f = self.host.flavor;
        let (mut found, rest): (Vec<String>, Vec<String>) = candidates.into_iter().partition(|p| is_dir(&f.dirname(p)));
        found.extend(rest);
        found.truncate(limit);
        found
    }

    /// Folders in which the channels can sit side by side, the configured
    /// game folder first.
    ///
    /// ⛔ A game folder given from outside (a test copy) is the only place:
    /// the channels in it or next to it, never the installation in the usual
    /// places, which 4.12 searches too. Otherwise a test run would show and
    /// write the real game.
    fn channel_bases(&self) -> Vec<String> {
        let f = self.host.flavor;
        let mut bases: Vec<String> = Vec::new();
        let given = self.given_game_folder();
        let only_given = given.is_some();
        let customs = if only_given { [given, None] } else { [None, self.setting("spiel_ordner")] };
        for custom in customs.into_iter().flatten() {
            let custom = self.host.expanduser(&custom);
            let custom = custom.trim_end_matches(f.sep());
            for candidate in [f.dirname(custom), custom.to_string()] {
                if !candidate.is_empty() && !bases.contains(&candidate) {
                    bases.push(candidate);
                }
            }
        }
        if only_given {
            return bases;
        }
        for root in self.game_roots() {
            let base = self.join(&root, &self.sc_subpath());
            if !bases.contains(&base) {
                bases.push(base);
            }
        }
        bases
    }

    /// The game folder given from outside (by a tool, or `SC_INSTALL_DIR`), as given.
    fn given_game_folder(&self) -> Option<String> {
        let given = self.state().given.game.clone().map(|fixed| fixed.folder);
        given.or_else(|| self.host.var_set(ENV_INSTALL_DIR).map(String::from))
    }

    /// The folders chosen for channels ([`SETTING_CHANNEL_FOLDERS`]) that are
    /// there, as (channel, folder) in the order of [`CHANNELS`]. They count
    /// instead of the folder of the same name found next to the game folder.
    ///
    /// ⛔ None while a game folder is given from outside: the copy is the
    /// only place then.
    pub fn chosen_channel_folders(&self) -> Vec<(String, String)> {
        if self.given_game_folder().is_some() {
            return Vec::new();
        }
        let Some(Value::Object(chosen)) = self.settings_value(SETTING_CHANNEL_FOLDERS) else { return Vec::new() };
        // LIVE is the game folder itself (`spiel_ordner`).
        CHANNELS[1..]
            .iter()
            .filter_map(|k| {
                let Some((_, Value::Str(folder))) = chosen.iter().find(|(name, _)| name == k) else { return None };
                let folder = py_strip(folder).trim_end_matches(self.host.flavor.sep());
                let folder = self.host.expanduser(folder);
                (!folder.is_empty() && is_dir(&folder)).then(|| (k.to_string(), folder))
            })
            .collect()
    }

    /// `usercfg.installed_channels`: every channel whose folder is there,
    /// also without a Game.log, as (name, folder, `Data.p4k` there), in the
    /// order of [`CHANNELS`]. A folder chosen for a channel comes first.
    pub fn installed_channels(&self) -> Vec<(String, String, bool)> {
        let chosen = self.chosen_channel_folders();
        let mut found: Vec<(String, String, bool)> = chosen
            .iter()
            .map(|(k, folder)| (k.clone(), folder.clone(), is_file(&self.join(folder, "Data.p4k"))))
            .collect();
        for base in self.channel_bases() {
            if !is_dir(&base) {
                continue;
            }
            for k in CHANNELS {
                let folder = self.join(&base, k);
                if found.iter().any(|c| c.1 == folder) || chosen.iter().any(|c| c.0 == k) || !is_dir(&folder) {
                    continue;
                }
                let p4k = is_file(&self.join(&folder, "Data.p4k"));
                found.push((k.to_string(), folder, p4k));
            }
        }
        found.sort_by_key(|c| CHANNELS.iter().position(|k| *k == c.0).unwrap_or(99));
        found
    }

    /// Every game channel that has a Game.log, newest first; a folder chosen
    /// for a channel instead of the one found next to the game folder.
    pub fn available_channels(&self) -> Vec<Channel> {
        let chosen = self.chosen_channel_folders();
        let mut found: Vec<Channel> = Vec::new();
        for (k, folder) in &chosen {
            let log = self.join(folder, "Game.log");
            if is_file(&log) {
                found.push((k.clone(), folder.clone(), mtime_seconds(&log).unwrap_or(0.0)));
            }
        }
        for base in self.channel_bases() {
            if !is_dir(&base) {
                continue;
            }
            for k in CHANNELS {
                let folder = self.join(&base, k);
                let log = self.join(&folder, "Game.log");
                if !is_file(&log) || found.iter().any(|c| c.1 == folder) || chosen.iter().any(|c| c.0 == k) {
                    continue;
                }
                found.push((k.to_string(), folder, mtime_seconds(&log).unwrap_or(0.0)));
            }
        }
        found.sort_by(|a, b| b.2.total_cmp(&a.2));
        found
    }

    /// Reading from a different channel than the configured one:
    /// (configured, used, all channels), or `None`. Never while a game folder
    /// is given from outside: the setting does not count then.
    pub fn channel_mismatch(&self) -> Option<(String, Option<String>, Vec<Channel>)> {
        if self.given_game_folder().is_some() {
            return None;
        }
        let configured = self.setting("spiel_ordner")?;
        let configured = self.host.expanduser(&configured).trim_end_matches(self.host.flavor.sep()).to_string();
        if is_file(&self.join(&configured, "Game.log")) {
            return None;
        }
        let channels = self.available_channels();
        if channels.is_empty() {
            return None;
        }
        Some((configured, self.game_folder(), channels))
    }

    /// Derive the game folder from a chosen folder: the channel itself, the
    /// folder above it, or up to five levels deeper.
    pub fn resolve_game_folder(&self, chosen: &str) -> Option<String> {
        if chosen.is_empty() {
            return None;
        }
        let f = self.host.flavor;
        let sep = f.sep();
        let mut chosen = self.host.expanduser(py_strip(chosen).trim_end_matches(sep));
        if chosen.is_empty() {
            chosen = sep.to_string();
        }
        if is_file(&chosen) {
            chosen = f.dirname(&chosen);
        }
        if is_file(&self.join(&chosen, "Game.log")) {
            return Some(chosen);
        }
        for k in CHANNELS {
            let p = self.join(&chosen, k);
            if is_file(&self.join(&p, "Game.log")) {
                return Some(p);
            }
        }
        let root_depth = chosen.trim_end_matches(sep).matches(sep).count();
        // Like `os.walk`: top-down, subfolders in folder order.
        let mut stack = vec![chosen];
        while let Some(base) = stack.pop() {
            let Ok(entries) = fs::read_dir(&base) else { continue };
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for entry in entries.flatten() {
                let Ok(name) = entry.file_name().into_string() else { continue };
                if fs::metadata(entry.path()).is_ok_and(|m| m.is_dir()) {
                    dirs.push(name);
                } else {
                    files.push(name);
                }
            }
            if base.matches(sep).count().saturating_sub(root_depth) > 5 {
                continue;
            }
            if files.iter().any(|n| n == "Game.log") {
                return Some(base);
            }
            for name in dirs.iter().rev() {
                let path = self.join(&base, name);
                if !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                    stack.push(path);
                }
            }
        }
        None
    }

    /// Where Windows says the RSI Launcher is installed, or `None`.
    ///
    /// The key name differs with every installation, `InstallLocation` is
    /// usually empty; the folder is in `DisplayIcon`.
    pub fn launcher_from_registry(&self) -> Option<String> {
        if self.host.flavor != Flavor::Windows || !self.host.registry {
            return None;
        }
        self.launcher_search()
    }

    #[cfg(windows)]
    fn launcher_search(&self) -> Option<String> {
        let branches = [
            (registry::HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
            (registry::HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
            (registry::HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        ];
        let f = self.host.flavor;
        for (root, path) in branches {
            let Some(uninstall) = registry::open(root, path) else { continue };
            let Some(count) = uninstall.subkey_count() else { continue };
            for i in 0..count {
                let Some(entry) = uninstall.enum_key(i).and_then(|name| uninstall.sub(&name)) else { continue };
                let value = |field: &str| entry.value(field).unwrap_or_default();
                if !value("DisplayName").to_lowercase().contains("rsi launcher") {
                    continue;
                }
                let mut candidates = Vec::new();
                let place = value("InstallLocation");
                let place = place.trim_matches(['"', ' ']);
                if !place.is_empty() {
                    candidates.push(f.join(place, "RSI Launcher.exe"));
                }
                let icon = value("DisplayIcon");
                let symbol = icon.split(',').next().unwrap_or("").trim_matches(['"', ' ']);
                if !symbol.is_empty() {
                    candidates.push(f.join(&f.dirname(symbol), "RSI Launcher.exe"));
                }
                if let Some(found) = candidates.into_iter().find(|p| is_file(p)) {
                    return Some(found);
                }
            }
        }
        None
    }

    #[cfg(not(windows))]
    fn launcher_search(&self) -> Option<String> {
        None
    }

    fn folder_or_game(&self, folder: Option<&str>) -> Option<String> {
        match folder.filter(|f| !f.is_empty()) {
            Some(f) => Some(f.to_string()),
            None => self.game_folder(),
        }
    }

    /// Path to the active Game.log or `None`.
    pub fn game_log(&self, folder: Option<&str>) -> Option<String> {
        let folder = self.folder_or_game(folder)?;
        let p = self.join(&folder, "Game.log");
        is_file(&p).then_some(p)
    }

    /// The preserved logs of past sessions, oldest first, also from the
    /// neighbouring channel with the same collection (LIVE ↔ HOTFIX).
    ///
    /// Cached as long as the modification time of the `logbackups` folders
    /// does not change.
    pub fn log_backups(&self, folder: Option<&str>) -> Vec<String> {
        let Some(folder) = self.folder_or_game(folder) else { return Vec::new() };
        let channels = self.channel_siblings(&folder);
        let stamps: Vec<Option<i128>> = channels
            .iter()
            .map(|dir| fs::metadata(self.join(dir, "logbackups")).ok().and_then(|m| m.modified().ok()).map(nanos))
            .collect();
        let key = (folder, stamps);
        if let Some((memo_key, hits)) = &self.state().log_backups
            && *memo_key == key
        {
            return hits.clone();
        }
        let hits = self.list_log_backups(&channels);
        self.state().log_backups = Some((key, hits.clone()));
        hits
    }

    /// The search behind `log_backups`, without cache. Everything is taken
    /// except files that are certainly unreadable; deduplicated by real path.
    fn list_log_backups(&self, channels: &[String]) -> Vec<String> {
        let mut hits: Vec<(f64, String)> = Vec::new();
        let mut seen = HashSet::new();
        for channel_dir in channels {
            for p in self.glob_star(&self.join(channel_dir, "logbackups")) {
                let lower = p.to_lowercase();
                if !is_file(&p) || SKIP_BACKUP_EXT.iter().any(|ext| lower.ends_with(ext)) {
                    continue;
                }
                let real = fs::canonicalize(&p)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| self.host.flavor.abspath(&p, &self.host.cwd));
                if !seen.insert(real) {
                    continue;
                }
                hits.push((mtime_seconds(&p).unwrap_or(0.0), p));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        hits.into_iter().map(|(_, p)| p).collect()
    }

    /// The folder itself and the channels next to it with the same collection.
    ///
    /// ⚠ Neighbours only if the folder above is really called `StarCitizen`
    /// and the own one is a channel from `SHARED_STOCK_CHANNELS`.
    fn channel_siblings(&self, folder: &str) -> Vec<String> {
        let f = self.host.flavor;
        let mut result = vec![folder.to_string()];
        let norm = f.normpath(folder);
        let parent = f.dirname(&norm);
        let name = f.basename(&norm).to_uppercase();
        if !SHARED_STOCK_CHANNELS.contains(&name.as_str()) || f.basename(&parent).to_lowercase() != "starcitizen" {
            return result;
        }
        for channel in SHARED_STOCK_CHANNELS {
            if channel == name {
                continue;
            }
            let neighbour = self.join(&parent, channel);
            if is_dir(&self.join(&neighbour, "logbackups")) {
                result.push(neighbour);
            }
        }
        result
    }

    /// `data/Localization` in the game folder, if it exists.
    pub fn localization_folder(&self, folder: Option<&str>) -> Option<String> {
        let folder = self.folder_or_game(folder)?;
        let p = self.join_all(&folder, &["data", "Localization"]);
        is_dir(&p).then_some(p)
    }
}

// ------------------------------------------------------------ Name normalization

/// Class codes that Verse-Kit and other tools append to item names. Only
/// these; real name parentheses like `(S2)` are kept.
const CLASS_CODES: &str = "civ|mil|ind|sth|cmp|las|ele|pla|dis|mic|bal|nah|min|slv|med|tool|trc";

/// Whitespace like Python's `\s`: Rust's `\s` plus `\x1c` to `\x1f`.
const PY_SPACE: &str = r"[\s\x1c-\x1f]";

/// Code in parentheses at the end of the name: `(mil/2/a)`, `(bal)`, `(ir1)`.
static CLASS_CODE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"{PY_SPACE}*\((?:(?:{CLASS_CODES})/(?:\d{{1,2}}|\u{{2013}}|-)/(?:[a-d]|\u{{2013}}|-)|(?:{CLASS_CODES})|(?:ir|em|cs)\d{{1,2}})\){PY_SPACE}*\z"
    ))
    .expect("fester Ausdruck")
});

/// The same code as a prefix and without parentheses: `ind/2/b citadel`.
///
/// ⚠ Python requires via lookahead (`(?=\S)`) that a name follows the
/// whitespace; `regex` has no lookahead, so `name_key` checks this itself
/// after the match.
static CLASS_CODE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"\A{PY_SPACE}*(?:{CLASS_CODES})/(?:\d{{1,2}}|\u{{2013}}|-)/(?:[a-d]|\u{{2013}}|-){PY_SPACE}+"))
        .expect("fester Ausdruck")
});

/// Type, size and grade in brackets as a prefix: `[civ-s2-a] 7ma 'lorica'`,
/// `[e-s1] fl-11 cannon`. A size (`-s1`) is required, so bracketed words
/// like `[prototype]` stay part of the name.
static BRACKET_CODE_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"\A{PY_SPACE}*\[[a-z]{{1,5}}-s\d{{1,2}}(?:-[a-d])?\]{PY_SPACE}+")).expect("fester Ausdruck")
});

/// Quantity in parentheses: `(16 cap)`, `(16 Schuss)`; the number is kept.
static QUANTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\((\d+){PY_SPACE}+[^)]*\)")).expect("fester Ausdruck"));

/// Quotation marks that all become the straight single quote.
const QUOTES: [char; 11] = [
    '"', '\u{201c}', '\u{201d}', '\u{201e}', '\u{2018}', '\u{2019}', '\u{201a}', '\u{ab}', '\u{bb}', '\u{2039}',
    '\u{203a}',
];

/// A blueprint name as a comparison key, the only place for this.
///
/// Normalized are case, non-breaking and broken spaces, quotation marks,
/// class codes (appended or prefixed, also in brackets) and the language of the quantity
/// (`(16 cap)` ↔ `(16 Schuss)` both become `(16)`).
pub fn name_key(s: &str) -> String {
    let low: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '\u{a0}' | '\u{fffd}' => ' ',
            c if QUOTES.contains(&c) => '\'',
            c => c,
        })
        .collect();
    let without_suffix = CLASS_CODE_RE.replace(&low, "");
    let text = py_strip(&without_suffix);
    let text = match CLASS_CODE_PREFIX_RE.find(text).or_else(|| BRACKET_CODE_PREFIX_RE.find(text)) {
        Some(m) if m.end() < text.len() => &text[m.end()..],
        _ => text,
    };
    py_strip(&QUANTITY_RE.replace_all(text, "(${1})")).to_string()
}

/// Like Python's `int(value)` for a setting; `None` instead of an exception.
pub(crate) fn py_int(value: &Value) -> Option<i128> {
    match value {
        Value::Bool(b) => Some(i128::from(*b)),
        Value::Int(text) => Some(parse_saturating(text)),
        Value::Float(x) if x.is_finite() => Some(x.trunc().clamp(i128::MIN as f64, i128::MAX as f64) as i128),
        Value::Str(text) => {
            let text = text.trim_matches(py_isspace);
            let (negative, digits) = match text.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, text.strip_prefix('+').unwrap_or(text)),
            };
            let valid = !digits.is_empty()
                && !digits.starts_with('_')
                && !digits.ends_with('_')
                && !digits.contains("__")
                && digits.chars().all(|c| c.is_ascii_digit() || c == '_');
            if !valid {
                return None;
            }
            let clean: String = digits.chars().filter(char::is_ascii_digit).collect();
            let number = parse_saturating(&clean);
            Some(if negative { -number } else { number })
        }
        _ => None,
    }
}

fn parse_saturating(text: &str) -> i128 {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut n: i128 = 0;
    for d in digits.bytes().filter(u8::is_ascii_digit) {
        n = n.saturating_mul(10).saturating_add(i128::from(d - b'0'));
    }
    if negative { -n } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_class_is_the_one_the_packaging_and_4_12_use() {
        // The desktop entry of the packaging finds the window by this class; a start menu entry that 4.12
        // wrote does too. Changing one of the two without the other makes a second, nameless icon.
        let file =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/versekit.desktop"))
                .expect("the desktop entry of the packaging");
        let class = file
            .lines()
            .find_map(|line| line.strip_prefix("StartupWMClass="))
            .expect("the entry names the window class");
        assert_eq!(class.trim(), WM_CLASS);
    }
    use std::path::Path;

    /// A fresh throwaway folder per test.
    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-paths-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn touch(path: &str, text: &str) {
        fs::create_dir_all(Path::new(path).parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A JSON object from text fields, backslashes in paths escaped correctly.
    fn object(fields: &[(&str, &str)]) -> String {
        let map = fields.iter().map(|(k, v)| (k.to_string(), Value::str(*v))).collect();
        jsonio::dumps(&Value::Object(map), None, false)
    }

    fn set_mtime(path: &str, secs: u64) {
        let time = UNIX_EPOCH + Duration::from_secs(secs);
        fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
    }

    /// A Linux machine with its home folder in the throwaway folder.
    fn linux(home: &str) -> Paths {
        Paths::new(Host::new(Flavor::Posix, &[("HOME", home)], home))
    }

    /// A game folder given from outside never leads to the real installation:
    /// not when the copy has no Game.log yet, not through the setting, not as
    /// the channel question; by the variable and by a tool alike.
    #[test]
    fn a_given_game_folder_never_falls_back_to_the_real_installation() {
        let home = scratch("givenonly");
        let p = linux(&home);
        let real = p.join_all(
            &home,
            &["Games", "star-citizen", "drive_c", "Program Files", "Roberts Space Industries", "StarCitizen"],
        );
        touch(&p.join_all(&real, &["LIVE", "Game.log"]), "");
        touch(&p.join_all(&real, &["PTU", "Game.log"]), "");
        assert_eq!(p.game_folder(), Some(p.join(&real, "LIVE")), "found without a copy given");
        let copy = p.join_all(&home, &["Testumgebung", "Spieldaten LIVE"]);
        fs::create_dir_all(&copy).unwrap();
        // The setting names a folder without a Game.log, which would ask the channel question.
        touch(&p.settings_file(), &object(&[("spiel_ordner", &p.join(&home, "leer"))]));

        let by_variable = Paths::new(Host::new(Flavor::Posix, &[("HOME", &home), (ENV_INSTALL_DIR, &copy)], &home));
        let by_tool = linux(&home);
        by_tool.set_given_game(&copy, GivenBy::Named("Prüfung".into()));
        for given in [&by_variable, &by_tool] {
            assert_eq!(given.game_folder(), None, "a copy without a Game.log is no game, not the real one");
            assert!(
                given.available_channels().iter().all(|c| !c.1.starts_with(&real)),
                "{:?}",
                given.available_channels()
            );
        }
        // The copy played, a PTU of the copy next to it: still no question about the setting.
        touch(&p.join(&copy, "Game.log"), "");
        touch(&p.join_all(&home, &["Testumgebung", "PTU", "Game.log"]), "");
        for given in [&by_variable, &by_tool] {
            assert_eq!(given.game_folder().as_deref(), Some(copy.as_str()));
            assert_eq!(given.channel_mismatch(), None, "no channel question");
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn linux_finds_game_in_lug_helper_prefix() {
        let home = scratch("lug");
        let p = linux(&home);
        let live = p.join_all(
            &home,
            &["Games", "star-citizen", "drive_c", "Program Files", "Roberts Space Industries", "StarCitizen", "LIVE"],
        );
        assert_eq!(p.game_folder(), None);
        touch(&p.join(&live, "Game.log"), "");
        assert_eq!(p.game_folder().as_deref(), Some(live.as_str()));
        assert_eq!(p.game_log(None), Some(p.join(&live, "Game.log")));
        assert!(live.contains("/drive_c/Program Files/Roberts Space Industries/StarCitizen/LIVE"));
        let _ = fs::remove_dir_all(&home);
    }

    /// Without a file given from outside the archive lies in the game folder;
    /// a given file wins, wherever it lies, and is handed back with the rest.
    #[test]
    fn the_archive_lies_in_the_game_folder_unless_a_file_is_given() {
        let home = scratch("archive");
        let p = linux(&home);
        let other = p.join_all(&home, &["woanders", ARCHIVE_NAME]);
        assert_eq!(p.archive_file(), None, "no game folder, no archive");
        let live = p.join_all(&home, &["spiel", "LIVE"]);
        touch(&p.join(&live, "Game.log"), "");
        let p = Paths::new(Host::new(Flavor::Posix, &[("HOME", &home), (ENV_INSTALL_DIR, &live)], &home));
        p.set_given_game(&live, GivenBy::Named("Werkzeug".into()));
        assert_eq!(p.archive_file(), Some(p.join(&live, ARCHIVE_NAME)));
        assert_eq!(p.given().archive, None);
        p.set_given_archive(&other, GivenBy::Named("Werkzeug".into()));
        assert_eq!(p.archive_file().as_deref(), Some(other.as_str()));
        assert_eq!(p.given().archive, Some(Fixed { folder: other.clone(), by: GivenBy::Named("Werkzeug".into()) }));
        assert_eq!(p.working_copy().archive_file().as_deref(), Some(other.as_str()), "a thread of its own keeps it");
        // Counter-check: handing the folders back drops the file as well.
        p.clear_given();
        assert_eq!(p.given().archive, None);
        assert_ne!(p.archive_file().as_deref(), Some(other.as_str()));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn linux_wine_prefixes_sorted_without_hidden() {
        let home = scratch("prefixes");
        let p = linux(&home);
        for name in ["zeta", "alpha", ".versteckt"] {
            fs::create_dir_all(p.join_all(&home, &[".local", "share", "lutris", "prefixes", name])).unwrap();
        }
        fs::create_dir_all(p.join_all(&home, &["Games", "star-citizen"])).unwrap();
        let prefixes = p.wine_prefixes();
        let lutris = p.join_all(&home, &[".local", "share", "lutris", "prefixes"]);
        let pos = |name: &str| prefixes.iter().position(|x| *x == p.join(&lutris, name));
        assert!(pos("alpha").unwrap() < pos("zeta").unwrap());
        assert_eq!(pos(".versteckt"), None);
        // `Games/star-citizen` is fixed at the front and does not appear twice via `Games/*`.
        let lug = p.join_all(&home, &["Games", "star-citizen"]);
        assert_eq!(prefixes.iter().filter(|x| **x == lug).count(), 1);
        assert_eq!(prefixes[0], lug);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn linux_wine_prefixes_of_heroic_flatpak_bottles_and_steam_come_behind_the_known_ones() {
        let home = scratch("prefixes-neu");
        let p = linux(&home);
        let heroic = p.join_all(&home, &["Games", "Heroic", "Prefixes"]);
        let bottles = p.join_all(&home, &[".var", "app", "com.usebottles.bottles", "data", "bottles", "bottles"]);
        let steam = p.join_all(&home, &[".local", "share", "Steam", "steamapps", "compatdata"]);
        let steam_flatpak =
            p.join_all(&home, &[".var", "app", "com.valvesoftware.Steam", "data", "Steam", "steamapps", "compatdata"]);
        for dir in [
            p.join(&heroic, "Star Citizen"),
            p.join_all(&heroic, &["Mit Proton", "pfx"]),
            p.join(&bottles, "SC"),
            p.join_all(&steam, &["3141592653", "pfx"]),
            p.join(&steam, "ohne-pfx"),
            p.join_all(&steam_flatpak, &["271828", "pfx"]),
            p.join_all(&home, &[".local", "share", "lutris", "prefixes", "lutris-sc"]),
        ] {
            fs::create_dir_all(&dir).unwrap();
        }
        let prefixes = p.wine_prefixes();
        let pos = |path: String| prefixes.iter().position(|x| *x == path);
        for wanted in [
            p.join(&heroic, "Star Citizen"),
            p.join(&heroic, "Mit Proton"),
            p.join_all(&heroic, &["Mit Proton", "pfx"]),
            p.join(&bottles, "SC"),
            p.join_all(&steam, &["3141592653", "pfx"]),
            p.join_all(&steam_flatpak, &["271828", "pfx"]),
        ] {
            assert!(pos(wanted.clone()).is_some(), "{wanted} in {prefixes:?}");
        }
        assert_eq!(pos(p.join(&steam, "ohne-pfx")), None, "a Steam folder without pfx is no prefix");
        assert_eq!(pos(p.join(&steam, "3141592653")), None, "only the pfx inside counts");
        let lutris = pos(p.join_all(&home, &[".local", "share", "lutris", "prefixes", "lutris-sc"])).unwrap();
        assert!(lutris < pos(p.join(&heroic, "Star Citizen")).unwrap(), "the places of 4.12 keep their turn");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn linux_documents_from_xdg_and_folder_names() {
        let home = scratch("xdg");
        let p = linux(&home);
        assert_eq!(p.documents(), home);
        fs::create_dir_all(p.join(&home, "Dokumente")).unwrap();
        assert_eq!(p.documents(), p.join(&home, "Dokumente"));
        let custom = p.join(&home, "Meine Ablage");
        fs::create_dir_all(&custom).unwrap();
        touch(&p.join_all(&home, &[".config", "user-dirs.dirs"]), "# x\nXDG_DOCUMENTS_DIR=\"$HOME/Meine Ablage\"\n");
        assert_eq!(p.documents(), custom);
        assert_eq!(p.default_folder(), p.join(&custom, NEW_FOLDER_NAME));
        fs::create_dir_all(p.join(&custom, legacy::DOCUMENTS_FOLDER)).unwrap();
        assert_eq!(p.default_folder(), p.join(&custom, legacy::DOCUMENTS_FOLDER));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn pointers_heal_each_other() {
        let home = scratch("pointer");
        let p = linux(&home);
        let storage = p.join(&home, "Ablage");
        fs::create_dir_all(&storage).unwrap();
        // Only the secondary copy exists: it applies, and the visible pointer is
        // created again.
        touch(&p.second_pointer(), &object(&[("ablage_ordner", &format!(" {storage} "))]));
        assert_eq!(p.app_folder(), storage);
        assert_eq!(p.read_pointer(&p.pointer_file()).as_deref(), Some(storage.as_str()));
        assert!(p.second_pointer().ends_with(&format!("/.config/{}/ablage.json", legacy::CONFIG_FOLDER)));
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        let want = format!("{{{nl}  \"ablage_ordner\": \"{}\"{nl}}}", storage.replace('\\', "\\\\"));
        assert_eq!(fs::read_to_string(p.pointer_file()).unwrap(), want);
        // A pointer into nothing does not count.
        let p2 = linux(&home);
        touch(&p2.pointer_file(), "{\"ablage_ordner\": \"/gibt/es/nicht\"}");
        fs::remove_file(p2.second_pointer()).unwrap();
        assert_eq!(p2.storage_from_file().as_deref(), Some("/gibt/es/nicht"));
        // Custom data folder via environment variable: flat, without subfolders.
        let mut host = Host::new(Flavor::Posix, &[("HOME", &home)], &home);
        host.set_var(legacy::ENV_HOME, &p.join(&home, "test"));
        let p3 = Paths::new(host);
        assert_eq!(p3.app_file("bestand.json"), p.join_all(&home, &["test", "bestand.json"]));
        assert!(Path::new(&p.join(&home, "test")).is_dir());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn app_file_sorts_into_subfolders() {
        let home = scratch("sub");
        let p = linux(&home);
        let base = p.default_folder();
        assert_eq!(p.app_file("bestand.json"), p.join_all(&base, &["Bauplaene", "bestand.json"]));
        assert_eq!(p.app_file("logstand.json"), p.join_all(&base, &["Intern", "logstand.json"]));
        assert_eq!(p.settings_file(), p.join_all(&base, &["Einstellungen", SETTINGS_FILE]));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn settings_values_like_python() {
        let home = scratch("values");
        let p = linux(&home);
        touch(
            &p.settings_file(),
            r#"{"spiel_ordner": "  ~/sc  ", "an": "  Ja ", "aus": "nein", "wahr": true, "zahl": " 4_2 ",
                "komma": 7.9, "neg": -3.2, "gross": 99999999999999999999999, "nan": NaN, "liste": [1],
                "ja_nein": false, "leer": "   ", "eins": true}"#,
        );
        assert_eq!(p.setting("spiel_ordner"), Some(format!("{home}/sc")));
        assert_eq!(p.setting("leer"), None);
        assert_eq!(p.setting("wahr"), None, "kein Text, kein Pfad");
        assert!(p.setting_bool("an", false));
        assert!(!p.setting_bool("aus", true));
        assert!(p.setting_bool("fehlt", true));
        assert!(p.setting_bool("liste", true));
        assert_eq!(p.setting_int("zahl", 0, None, None), 42);
        assert_eq!(p.setting_int("komma", 0, None, None), 7);
        assert_eq!(p.setting_int("neg", 0, None, None), -3);
        assert_eq!(p.setting_int("eins", 0, None, None), 1);
        assert_eq!(p.setting_int("gross", 0, None, Some(60)), 60);
        assert_eq!(p.setting_int("nan", 5, None, None), 5);
        assert_eq!(p.setting_int("liste", 5, None, None), 5);
        assert_eq!(p.setting_int("zahl", 0, Some(1), Some(10)), 10);
        assert_eq!(p.setting_int("an", 3, Some(1), Some(10)), 3);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn broken_settings_recover_from_backup() {
        let home = scratch("recover");
        let p = linux(&home);
        let target = p.settings_file();
        touch(&target, "{kaputt");
        touch(&jsonio::backup_name(&target), "{\"sprache\": \"de\"}");
        set_mtime(&target, 1_000_000_000);
        assert_eq!(p.settings(), vec![("sprache".to_string(), Value::str("de"))]);
        assert_eq!(p.settings_recovered(), Some(Recovered::Backup));
        let folder = Flavor::native().dirname(&target);
        let names: Vec<String> =
            fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().any(|n| n.starts_with("einstellungen.kaputt-")), "{names:?}");
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        assert_eq!(fs::read_to_string(&target).unwrap(), format!("{{{nl}  \"sprache\": \"de\"{nl}}}"));
        // Corrupt and no previous version: default values, and `set_setting`
        // creates the template including the new value.
        let home2 = scratch("defaults");
        let p2 = linux(&home2);
        touch(&p2.settings_file(), "[1, 2]");
        set_mtime(&p2.settings_file(), 1_000_000_000);
        assert!(p2.settings().is_empty());
        assert_eq!(p2.settings_recovered(), Some(Recovered::Defaults));
        assert!(p2.set_setting("sprache", Value::str("en")));
        let saved = jsonio::read_json(&p2.settings_file()).unwrap();
        assert_eq!(saved.get("sprache"), Some(&Value::str("en")));
        let Value::Object(map) = &saved else { panic!("no object") };
        assert!(map.iter().all(|(k, _)| !k.starts_with('_')), "no explanatory line is written: {map:?}");
        assert!(saved.get("spiel_ordner").is_some(), "the other defaults are there");
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&home2);
    }

    #[test]
    fn the_template_of_the_settings_has_no_explanatory_lines_and_no_old_name() {
        let home = scratch("vorlage-name");
        let p = linux(&home);
        let template = p.template();
        let keys: Vec<&str> = template.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["spiel_ordner", "sprache", "pruefintervall_sekunden", "signalton", "deckkraft_prozent"]);
        assert!(keys.iter().all(|k| !k.starts_with('_')), "{keys:?}");
        let text = jsonio::dumps(&Value::Object(template.clone()), None, false);
        assert!(!text.to_lowercase().contains("watcher"), "the old name is gone: {text}");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn freshly_written_broken_settings_are_left_alone() {
        let home = scratch("fresh");
        let p = linux(&home);
        let target = p.settings_file();
        touch(&target, "{halb");
        assert!(p.settings().is_empty());
        assert_eq!(fs::read_to_string(&target).unwrap(), "{halb", "wird gerade geschrieben — nicht anfassen");
        assert!(!p.set_setting("x", Value::Null), "nicht lesbar heißt nicht überschreiben");
        assert_eq!(fs::read_to_string(&target).unwrap(), "{halb");
        let _ = fs::remove_dir_all(&home);
    }

    #[cfg(windows)]
    #[test]
    fn locked_settings_are_not_overwritten() {
        use std::os::windows::fs::OpenOptionsExt;
        let home = scratch("locked");
        let p = linux(&home);
        let target = p.settings_file();
        touch(&target, "{\"sprache\": \"de\"}");
        set_mtime(&target, 1_000_000_000);
        let lock = fs::OpenOptions::new().read(true).share_mode(0).open(&target).unwrap();
        let started = Instant::now();
        assert!(p.settings().is_empty());
        assert!(started.elapsed() >= SETTINGS_LOCK_PAUSE * (SETTINGS_LOCK_TRIES - 1), "es wurde gewartet");
        // Only one attempt the second time.
        let started = Instant::now();
        assert!(p.settings().is_empty());
        assert!(started.elapsed() < SETTINGS_LOCK_PAUSE);
        assert!(!p.set_setting("sprache", Value::str("en")));
        drop(lock);
        assert_eq!(p.setting("sprache").as_deref(), Some("de"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn configured_folder_falls_back_to_sibling_channel() {
        let home = scratch("sibling");
        let p = linux(&home);
        let sc = p.join_all(&home, &["Platte", "StarCitizen"]);
        let live = p.join(&sc, "LIVE");
        let hotfix = p.join(&sc, "HOTFIX");
        touch(&p.settings_file(), &object(&[("spiel_ordner", &format!("{live}/"))]));
        touch(&p.join(&hotfix, "Game.log"), "");
        touch(&p.join_all(&sc, &["PTU", "Game.log"]), "");
        set_mtime(&p.join_all(&sc, &["PTU", "Game.log"]), 1_000);
        assert_eq!(p.game_folder().as_deref(), Some(hotfix.as_str()));
        let (configured, used, channels) = p.channel_mismatch().unwrap();
        assert_eq!(configured, live);
        assert_eq!(used.as_deref(), Some(hotfix.as_str()));
        assert_eq!(channels.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["HOTFIX", "PTU"]);
        assert_eq!(p.resolve_game_folder(&home).as_deref(), Some(hotfix.as_str()));
        assert_eq!(p.resolve_game_folder(&sc).as_deref(), Some(hotfix.as_str()));
        let _ = fs::remove_dir_all(&home);
    }

    /// A folder chosen for a channel counts instead of the one next to the
    /// game folder; one that is not there does not; with a game folder given
    /// from outside the choice counts not at all.
    #[test]
    fn a_chosen_channel_folder_counts_instead_of_the_one_found() {
        let home = scratch("chosen");
        let p = linux(&home);
        let sc = p.join_all(&home, &["Platte", "StarCitizen"]);
        let live = p.join(&sc, "LIVE");
        let other = p.join_all(&home, &["Zweite", "PTU-Kopie"]);
        for folder in [&live, &p.join(&sc, "PTU"), &other] {
            touch(&p.join(folder, "Game.log"), "");
            touch(&p.join(folder, "Data.p4k"), "");
        }
        let settings = |ptu: &str| {
            let live = live.replace('\\', "\\\\");
            let ptu = ptu.replace('\\', "\\\\");
            format!(r#"{{"spiel_ordner": "{live}", "kanal_ordner": {{"PTU": "{ptu}", "LIVE": "{ptu}"}}}}"#)
        };
        touch(&p.settings_file(), &settings(&other));
        let ptu = |list: Vec<String>| list.into_iter().filter(|f| f.contains("PTU")).collect::<Vec<_>>();
        assert_eq!(p.chosen_channel_folders(), [("PTU".to_string(), other.clone())], "LIVE is the game folder");
        assert_eq!(ptu(p.installed_channels().into_iter().map(|c| c.1).collect()), std::slice::from_ref(&other));
        assert_eq!(ptu(p.available_channels().into_iter().map(|c| c.1).collect()), std::slice::from_ref(&other));
        assert!(p.installed_channels().iter().any(|c| c.0 == "LIVE" && c.1 == live), "LIVE stays");

        // A chosen folder that is gone: the one next to the game folder again.
        touch(&p.settings_file(), &settings(&p.join(&home, "weg")));
        assert!(p.chosen_channel_folders().is_empty());
        assert_eq!(ptu(p.installed_channels().into_iter().map(|c| c.1).collect()), [p.join(&sc, "PTU")]);

        // Given from outside: the copy alone, the choice left aside.
        touch(&p.settings_file(), &settings(&other));
        let given = Paths::new(Host::new(Flavor::Posix, &[("HOME", &home), (ENV_INSTALL_DIR, &live)], &home));
        assert!(given.chosen_channel_folders().is_empty());
        assert_eq!(ptu(given.installed_channels().into_iter().map(|c| c.1).collect()), [p.join(&sc, "PTU")]);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn log_backups_from_shared_channels_only() {
        let home = scratch("backups");
        let p = linux(&home);
        let sc = p.join_all(&home, &["x", "StarCitizen"]);
        let live = p.join(&sc, "LIVE");
        touch(&p.join(&live, "Game.log"), "");
        touch(&p.settings_file(), &object(&[("spiel_ordner", &live)]));
        let files = [
            (p.join_all(&live, &["logbackups", "Game Build(1) 02.log"]), 300),
            (p.join_all(&sc, &["HOTFIX", "logbackups", "Game Build(1) 01.log"]), 100),
            (p.join_all(&live, &["logbackups", "b.log"]), 200),
            (p.join_all(&live, &["logbackups", "a.log"]), 200),
            (p.join_all(&sc, &["PTU", "logbackups", "ptu.log"]), 50),
            (p.join_all(&live, &["logbackups", "crash.ZIP"]), 10),
            (p.join_all(&live, &["logbackups", ".versteckt"]), 10),
        ];
        for (path, secs) in &files {
            touch(path, "x");
            set_mtime(path, *secs);
        }
        let want = [&files[1].0, &files[3].0, &files[2].0, &files[0].0].map(String::clone);
        assert_eq!(p.log_backups(None), want);
        assert_eq!(p.log_backups(Some(&live)), want, "aus dem Zwischenspeicher");
        // PTU configured directly: only its own log.
        assert_eq!(p.log_backups(Some(&p.join(&sc, "PTU"))), [files[4].0.clone()]);
        assert_eq!(p.localization_folder(None), None);
        fs::create_dir_all(p.join_all(&live, &["data", "Localization"])).unwrap();
        assert_eq!(p.localization_folder(None), Some(p.join_all(&live, &["data", "Localization"])));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn env_install_dir_wins_and_must_hold_a_log() {
        let home = scratch("envdir");
        let mut host = Host::new(Flavor::Posix, &[("HOME", &home)], &home);
        let p0 = Paths::new(host.clone());
        let own = p0.join(&home, "eigen");
        host.set_var(ENV_INSTALL_DIR, &own);
        let p = Paths::new(host);
        fs::create_dir_all(&own).unwrap();
        assert_eq!(p.game_folder(), None, "ohne Game.log kein Spielordner");
        touch(&p.join_all(&own, &["PTU", "Game.log"]), "");
        assert_eq!(p.game_folder(), Some(p.join(&own, "PTU")));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn windows_host_rules() {
        let host = Host::new(
            Flavor::Windows,
            &[
                ("UserProfile", "D:\\Konten\\Spieler"),
                ("USERNAME", "Spieler"),
                ("APPDATA", "D:\\Konten\\Spieler\\AppData\\Roaming"),
            ],
            "C:\\",
        );
        assert_eq!(host.var("USERPROFILE"), Some("D:\\Konten\\Spieler"));
        assert_eq!(host.expanduser("~\\x"), "D:\\Konten\\Spieler\\x");
        assert_eq!(host.expanduser("~Gast/x"), "D:\\Konten\\Gast/x");
        assert_eq!(host.expandvars("%APPDATA%\\a"), "D:\\Konten\\Spieler\\AppData\\Roaming\\a");
        assert_eq!(host.expandvars("%FEHLT%\\a"), "%FEHLT%\\a");
        assert_eq!(host.expandvars("'%APPDATA%' $USERNAME ${USERNAME} $$ 100%"), "'%APPDATA%' Spieler Spieler $ 100%");
        let p = Paths::new(host);
        assert_eq!(
            p.second_pointer(),
            format!("D:\\Konten\\Spieler\\AppData\\Roaming\\{}\\ablage.json", legacy::CONFIG_FOLDER)
        );
        assert_eq!(p.game_roots()[..2], ["C:\\Program Files", "C:\\Program Files (x86)"]);
        assert_eq!(p.game_roots().len(), 12);
        assert!(p.same_path("d:/konten/spieler/x", "D:\\Konten\\Spieler\\x\\"));
        assert_eq!(p.launcher_from_registry(), None, "ohne Registry");
    }

    #[test]
    fn posix_expanduser() {
        let host = Host::new(Flavor::Posix, &[("HOME", "/home/spieler/")], "/");
        assert_eq!(host.expanduser("~"), "/home/spieler");
        assert_eq!(host.expanduser("~/Games"), "/home/spieler/Games");
        assert_eq!(host.expanduser("~anders/x"), "~anders/x");
        assert_eq!(host.expanduser("/x/~"), "/x/~");
        let root = Host::new(Flavor::Posix, &[("HOME", "/")], "/");
        assert_eq!(root.expanduser("~"), "/");
    }

    /// A copy taken on another machine: both pointers lead outside it and are
    /// pointed at its own data folder, other fields stay; nothing is written
    /// outside the copy. Without a data folder there are no paths.
    #[test]
    fn a_copy_from_another_machine_points_at_itself() {
        let root = std::env::temp_dir().join(format!("versekit-kopie-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root_text = root.to_string_lossy().into_owned();
        let first = root.join("Dokumente").join(legacy::DOCUMENTS_FOLDER).join("Einstellungen");
        let second = root.join("AppData").join(legacy::CONFIG_FOLDER);
        let settings = root.join(COPY_DATA_FOLDER).join("Einstellungen");
        for folder in [&first, &second, &settings] {
            fs::create_dir_all(folder).unwrap();
        }
        let elsewhere = "/Users/spieler/Spiele/VerseKit-Daten";
        fs::write(first.join(SETTINGS_FILE), format!("{{\"_zweck\": \"Zeiger\", \"ablage_ordner\": \"{elsewhere}\"}}"))
            .unwrap();
        fs::write(second.join("ablage.json"), format!("{{\"ablage_ordner\": \"{elsewhere}\"}}")).unwrap();
        fs::write(settings.join(SETTINGS_FILE), "{\"max_zeilen\": 7}").unwrap();

        crate::writeguard::deny(true);
        crate::writeguard::allow_under(Some(&root_text));
        let host = Host::new(Flavor::native(), &[], &root_text);
        let paths = Paths::for_copy(host.clone(), &root_text);
        let max_rows = paths.as_ref().map(|p| p.setting_int("max_zeilen", 20, None, None));
        crate::writeguard::deny(false);
        let denied = crate::writeguard::take_denied();
        let paths = paths.expect("the copy is found");
        let data = Flavor::native().join(&root_text, COPY_DATA_FOLDER);
        assert_eq!(max_rows, Some(7));
        assert_eq!(paths.read_pointer(&paths.pointer_file()).as_deref(), Some(data.as_str()));
        assert_eq!(paths.read_pointer(&paths.second_pointer()).as_deref(), Some(data.as_str()));
        let kept = fs::read_to_string(first.join(SETTINGS_FILE)).unwrap();
        assert!(kept.contains("_zweck"), "other fields of the pointer stay: {kept}");
        assert!(denied.is_empty(), "nothing outside the copy: {denied:?}");

        // Without its data folder the copy is no copy.
        fs::remove_dir_all(root.join(COPY_DATA_FOLDER)).unwrap();
        fs::write(first.join(SETTINGS_FILE), format!("{{\"ablage_ordner\": \"{elsewhere}\"}}")).unwrap();
        fs::write(second.join("ablage.json"), format!("{{\"ablage_ordner\": \"{elsewhere}\"}}")).unwrap();
        crate::writeguard::deny(true);
        let none = Paths::for_copy(host, &root_text);
        crate::writeguard::deny(false);
        assert!(none.is_none());
        assert!(crate::writeguard::take_denied().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// The first start on an empty folder (`--daten` on nothing): a new copy whose data folder lies in it and is
    /// given by the start, nothing written outside it. A folder that is not there, or that holds something else,
    /// is still no copy.
    #[test]
    fn an_empty_folder_is_a_new_copy() {
        let root = std::env::temp_dir().join(format!("versekit-leere-kopie-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root_text = root.to_string_lossy().into_owned();
        let host = Host::new(Flavor::native(), &[], &root_text);
        crate::writeguard::deny(true);
        crate::writeguard::allow_under(Some(&root_text));
        let missing = Paths::for_copy(host.clone(), &root_text);
        fs::create_dir_all(&root).unwrap();
        let paths = Paths::for_copy(host.clone(), &root_text);
        let again = Paths::for_copy(host.clone(), &root_text).map(|p| p.app_folder());
        crate::writeguard::deny(false);
        let denied = crate::writeguard::take_denied();
        assert!(missing.is_none(), "a folder that is not there is no copy");
        let paths = paths.expect("an empty folder is a new copy");
        let data = Flavor::native().join(&root_text, COPY_DATA_FOLDER);
        assert_eq!(paths.app_folder(), data);
        assert_eq!(paths.given().data.map(|f| (f.folder, f.by)), Some((data.clone(), GivenBy::CommandLine)));
        assert_eq!(again.as_deref(), Some(data.as_str()), "the next start finds the same copy");
        assert!(denied.is_empty(), "nothing outside the copy: {denied:?}");
        assert_eq!(paths.host.copy.as_deref(), Some(root_text.as_str()), "the run knows its copy");
        assert_eq!(host.copy, None, "the user's own data have none");

        let other = std::env::temp_dir().join(format!("versekit-fremder-ordner-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&other);
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("fremd.txt"), "x").unwrap();
        let other_text = other.to_string_lossy().into_owned();
        crate::writeguard::deny(true);
        crate::writeguard::allow_under(Some(&other_text));
        let none = Paths::for_copy(Host::new(Flavor::native(), &[], &other_text), &other_text);
        crate::writeguard::deny(false);
        assert!(none.is_none(), "a folder with something else in it is no copy");
        assert_eq!(fs::read_dir(&other).unwrap().count(), 1, "and nothing was written into it");
        assert!(crate::writeguard::take_denied().is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&other);
    }

    #[test]
    fn a_contained_machine_has_every_place_of_the_user_inside_its_folder() {
        for flavor in [Flavor::Windows, Flavor::Posix] {
            let root = if flavor == Flavor::Windows { "D:\\Probe" } else { "/probe" };
            let real = [("USERPROFILE", "C:\\Users\\Spieler"), ("APPDATA", "C:\\Users\\Spieler\\AppData\\Roaming")];
            let mut host = Host::new(flavor, &real, root);
            host.registry = true;
            let host = host.contained(root);
            assert!(!host.registry, "the registry names the real Documents");
            let inside = |p: &str| p.starts_with(root);
            let p = Paths::new(host.clone());
            for place in [host.home(), p.documents(), p.second_pointer(), p.default_folder(), p.pointer_file()] {
                assert!(inside(&place), "{flavor:?}: {place}");
            }
            for name in ["APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"] {
                assert!(host.var(name).is_some_and(inside), "{flavor:?}: {name}");
            }
        }
    }

    /// ⚠ The guard of all test runs: cargo sets `ENV_TEST_MACHINE` (`.cargo/config.toml`), and the machine of the
    /// program then has no place outside it. Red here means the tests of this run can reach the real
    /// configuration folder and Documents.
    #[test]
    fn the_tests_run_on_a_machine_of_their_own() {
        let root = std::env::var(ENV_TEST_MACHINE).unwrap_or_default();
        assert!(!root.is_empty(), "{ENV_TEST_MACHINE} is not set: the tests run on the real machine");
        let host = Host::current();
        let p = Paths::new(host.clone());
        let f = Flavor::native();
        let norm = |x: &str| f.normcase(&f.abspath(x, &host.cwd));
        let inside = |x: &str| norm(x).starts_with(&format!("{}{}", norm(&root), f.sep()));
        for place in [host.home(), p.documents(), p.second_pointer(), p.legacy_app_folder()] {
            assert!(inside(&place), "{place} lies outside {root}");
        }
        assert!(!host.registry);
    }
}
