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

//! Starting with the system (`autostart.py` of 4.12).
//!
//! The entry carries the name of the program, not the old one: a value `VerseKit` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` on Windows, a file `versekit.desktop` in
//! `$XDG_CONFIG_HOME/autostart` elsewhere. The command is the quoted path of the program with the switch
//! [`SYSTEM_START`]: the start with the system is the normal start (the overlay; the setup assistant if the
//! program is not set up yet).
//!
//! The command of an entry carries the switch [`SYSTEM_START`]: it tells the program that the system started
//! it, not the player. Only a start by hand opens the main window by itself. An entry without the switch (written
//! by an earlier version or by the installer) gets it at the next start by hand ([`add_system_start`]).
//!
//! ⚠ The state is the entry itself, never a setting of its own: "no second truth" (4.12). What the entry says
//! is what the switch shows.
//!
//! ⚠ Everything goes through a [`Store`]. The runs on a copy of the data and every test use the [`Memory`]
//! one: nothing writes into the real registry or the real autostart folder by accident. Only the program on
//! the user's own data hands over [`System`].
//!
//! A start with the system under an earlier name ([`EARLIER_NAMES`]) is taken over under the new name and the
//! old entry goes ([`migrate`]): otherwise the old program would start next to the new one.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use crate::legacy;

/// The name of the entry; the installer writes the same one.
pub const NAME: &str = "VerseKit";

/// The names entries had before: the one of 4.12 and the one of the earlier test versions of this program.
pub const EARLIER_NAMES: [&str; 2] = [legacy::AUTOSTART_NAME, "Verse-Kit"];

/// The switch of a start by the system (the entry of the start with the system, the installer writes the same).
/// Without it a start is a start by hand.
pub const SYSTEM_START: &str = "--systemstart";

/// Where an autostart entry lives: a value by name, with the command it runs.
pub trait Store {
    /// Does this system have a start with the system at all? (On Windows when the registry can be
    /// reached, elsewhere always.)
    fn possible(&self) -> bool;
    /// The command of the entry `name`, `None` when there is none.
    fn get(&self, name: &str) -> Option<String>;
    /// Writes the entry `name` with the command.
    fn put(&self, name: &str, command: &str) -> Result<(), String>;
    /// Removes the entry `name`; none there is fine.
    fn remove(&self, name: &str) -> Result<(), String>;
    /// Has the player (or the system on their behalf) left the entry on? An entry that stands but was
    /// switched off in the task manager of Windows, or in the settings of a desktop, does not start. Where
    /// the system has no such switch the entry is on.
    fn approved(&self, _name: &str) -> bool {
        true
    }
}

/// The command of the entry for the program `program` on this system ([`command_for`]).
pub fn command(program: &Path, appimage: Option<&str>, arguments: &[String]) -> String {
    command_for(cfg!(windows), program, appimage, arguments)
}

/// The command of the entry for the program `program`: its path in quotes, then each of `arguments` in quotes
/// (`--daten <copy>` on a copy of the data, nothing on the user's own data, see
/// [`crate::desktop_entry::run_arguments`]). A program that runs as an AppImage starts that file (checked
/// before anything else: the unpacked program lies in a folder that is gone after the next start). On Windows
/// a quote is dropped (no file name has one); elsewhere each part is escaped as the `Exec=` line of a
/// `.desktop` file wants it, the same way as the entry in the menu ([`crate::desktop_entry::exec_argument`]).
pub fn command_for(windows: bool, program: &Path, appimage: Option<&str>, arguments: &[String]) -> String {
    let target = match appimage {
        Some(image) if !image.trim().is_empty() => image.to_string(),
        _ => program.to_string_lossy().into_owned(),
    };
    let mut line = quoted(windows, &target);
    for argument in arguments {
        line.push(' ');
        line.push_str(&quoted(windows, argument));
    }
    line
}

/// One part of a command in quotes, as the system wants it (see [`command_for`]).
fn quoted(windows: bool, part: &str) -> String {
    if windows { format!("\"{}\"", part.replace('"', "")) } else { crate::desktop_entry::exec_argument(part) }
}

/// Does the command say that the system started the program ([`SYSTEM_START`])?
pub fn has_system_start(command: &str) -> bool {
    command.split_whitespace().any(|part| part.trim_matches('"') == SYSTEM_START)
}

/// Gives an entry that is on and lacks the switch [`SYSTEM_START`] the switch, behind its own command: the program
/// and its other arguments stay as they are. Done at a start by hand, so that the start with the system is told
/// apart from it from then on. An entry that is off (none, no command, switched off by the system) is left alone:
/// writing it again could switch it on. True when it was changed.
pub fn add_system_start(store: &dyn Store) -> bool {
    add_system_start_for(cfg!(windows), store)
}

fn add_system_start_for(windows: bool, store: &dyn Store) -> bool {
    let Some(command) = store.get(NAME).filter(|command| !command.trim().is_empty()) else { return false };
    if has_system_start(&command) || !store.approved(NAME) {
        return false;
    }
    let extended = format!("{} {}", command.trim_end(), quoted(windows, SYSTEM_START));
    store.put(NAME, &extended).is_ok()
}

/// Is the program started with the system? The entry exists and says something.
pub fn is_on(store: &dyn Store) -> bool {
    store.get(NAME).is_some_and(|command| !command.trim().is_empty())
}

/// Switches it on (with `command`) or off.
pub fn set_on(store: &dyn Store, on: bool, command: &str) -> Result<(), String> {
    if on { store.put(NAME, command) } else { store.remove(NAME) }
}

/// Takes over the entries under the earlier names: when one is there AND on, the new one is set (with
/// `command`, unless a new one is already there) and the old one goes. True when something was taken over.
///
/// ⚠ The start with the system is voluntary and is never switched on by itself. Whoever switched it off has no
/// entry (4.12 deletes it), or an entry without a command (4.12 reads that as off), or switched the entry off
/// in the task manager or the settings of the desktop: such an old entry is only cleared away, and no new one
/// is made.
pub fn migrate(store: &dyn Store, command: &str) -> bool {
    let mut taken = false;
    for old in EARLIER_NAMES {
        taken |= migrate_one(store, old, command);
    }
    taken
}

fn migrate_one(store: &dyn Store, old: &str, command: &str) -> bool {
    let Some(old_command) = store.get(old) else { return false };
    if old_command.trim().is_empty() || !store.approved(old) {
        let _ = store.remove(old);
        return false;
    }
    if !is_on(store) && store.put(NAME, command).is_err() {
        // The old entry stays when the new one could not be written: the player keeps their start.
        return false;
    }
    store.remove(old).is_ok()
}

/// The file name of an entry on a system with `.desktop` files: lower case, spaces as hyphens
/// (the autostart name of 4.12 becomes the file 4.12 wrote).
pub fn desktop_file_name(name: &str) -> String {
    format!("{}.desktop", name.trim().to_lowercase().replace(' ', "-"))
}

/// The content of a `.desktop` file for the entry `name`.
pub fn desktop_text(name: &str, command: &str, comment: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec={command}\nTerminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// The command a `.desktop` file runs: the value after `Exec=`.
pub fn desktop_command(text: &str) -> Option<String> {
    text.lines().find_map(|line| line.trim().strip_prefix("Exec=")).map(|c| c.trim().to_string())
}

// ------------------------------------------------------------------ in memory

/// An autostart in memory, for the runs on a copy and for the tests; it can be told that the system has none.
#[derive(Default)]
pub struct Memory {
    entries: RefCell<BTreeMap<String, String>>,
    /// Entries that stand but are switched off by the system.
    off: RefCell<std::collections::BTreeSet<String>>,
    /// Writing fails (a locked registry, a folder without rights).
    refuse: bool,
    /// Only the writing of a new entry fails; removing works (a registry that takes nothing new).
    refuse_put: bool,
    /// The system has no start with the system.
    impossible: bool,
}

impl Memory {
    /// With an entry already there.
    pub fn with(name: &str, command: &str) -> Memory {
        let memory = Memory::default();
        memory.entries.borrow_mut().insert(name.to_string(), command.to_string());
        memory
    }

    /// With an entry that is there but switched off (task manager, settings of the desktop).
    pub fn with_switched_off(name: &str, command: &str) -> Memory {
        let memory = Memory::with(name, command);
        memory.off.borrow_mut().insert(name.to_string());
        memory
    }

    /// One where writing fails.
    pub fn refusing() -> Memory {
        Memory { refuse: true, ..Memory::default() }
    }

    /// One that takes no new entry but lets entries go.
    pub fn refusing_new() -> Memory {
        Memory { refuse_put: true, ..Memory::default() }
    }

    /// One of a system that has no start with the system.
    pub fn impossible() -> Memory {
        Memory { impossible: true, ..Memory::default() }
    }

    /// The entries as they are now, by name.
    pub fn entries(&self) -> BTreeMap<String, String> {
        self.entries.borrow().clone()
    }
}

impl Store for Memory {
    fn possible(&self) -> bool {
        !self.impossible
    }
    fn get(&self, name: &str) -> Option<String> {
        self.entries.borrow().get(name).cloned()
    }
    fn put(&self, name: &str, command: &str) -> Result<(), String> {
        if self.refuse || self.refuse_put {
            return Err("refused".to_string());
        }
        self.entries.borrow_mut().insert(name.to_string(), command.to_string());
        Ok(())
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        if self.refuse {
            return Err("refused".to_string());
        }
        self.entries.borrow_mut().remove(name);
        self.off.borrow_mut().remove(name);
        Ok(())
    }
    fn approved(&self, name: &str) -> bool {
        !self.off.borrow().contains(name)
    }
}

// ------------------------------------------------------------------ the system

/// The real autostart of this system: the `Run` key of the user on Windows, `.desktop` files elsewhere.
/// ⚠ Only for the program on the user's own data.
pub struct System;

#[cfg(windows)]
mod windows {
    use std::ptr::null_mut;

    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };

    use crate::jsonio::wide;

    /// The key of the user whose values start with the session.
    pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NOT_FOUND: u32 = 2;

    fn open(path: &str, access: u32, create: bool) -> Result<HKEY, String> {
        let mut key: HKEY = null_mut();
        let path = wide(path);
        // SAFETY: `path` is a zero-ended wide string, `key` receives the handle.
        let rc = unsafe {
            if create {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    null_mut(),
                    REG_OPTION_NON_VOLATILE,
                    access,
                    null_mut(),
                    &mut key,
                    null_mut(),
                )
            } else {
                RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key)
            }
        };
        if rc == 0 { Ok(key) } else { Err(format!("registry {rc}")) }
    }

    pub fn possible(path: &str) -> bool {
        open(path, KEY_QUERY_VALUE, false).map(close).is_ok()
    }

    fn close(key: HKEY) {
        // SAFETY: The key was opened with RegOpenKeyExW.
        unsafe { RegCloseKey(key) };
    }

    /// Removes a key of the user that holds nothing but values (for the test, which works in a key of its own).
    #[cfg(test)]
    pub fn delete_key(path: &str) {
        use windows_sys::Win32::System::Registry::RegDeleteKeyW;
        let path = wide(path);
        // SAFETY: A zero-ended wide string; the key has no subkeys.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, path.as_ptr()) };
    }

    pub fn get(path: &str, name: &str) -> Option<String> {
        let key = open(path, KEY_QUERY_VALUE, false).ok()?;
        let value = wide(name);
        let mut size = 0u32;
        // SAFETY: Asks for the size first, then reads into a buffer of that size.
        let found = unsafe {
            let rc = RegQueryValueExW(key, value.as_ptr(), null_mut(), null_mut(), null_mut(), &mut size);
            if rc != 0 || size == 0 {
                None
            } else {
                let mut data = vec![0u16; (size as usize).div_ceil(2) + 1];
                let mut len = size;
                let rc = RegQueryValueExW(
                    key,
                    value.as_ptr(),
                    null_mut(),
                    null_mut(),
                    data.as_mut_ptr().cast::<u8>(),
                    &mut len,
                );
                (rc == 0).then(|| {
                    let end = data.iter().position(|c| *c == 0).unwrap_or(data.len());
                    String::from_utf16_lossy(&data[..end])
                })
            }
        };
        close(key);
        found
    }

    /// Where the task manager keeps whether an entry of `Run` is switched off.
    pub const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    /// The first byte of a binary value, `None` where there is none.
    pub fn first_byte(path: &str, name: &str) -> Option<u8> {
        let key = open(path, KEY_QUERY_VALUE, false).ok()?;
        let value = wide(name);
        let mut data = [0u8; 16];
        let mut len = data.len() as u32;
        // SAFETY: Reads into a buffer of the size given; a longer value fails with "more data", which is
        // no value for us (the first bytes are all that is wanted, and the answer then is "none").
        let rc = unsafe { RegQueryValueExW(key, value.as_ptr(), null_mut(), null_mut(), data.as_mut_ptr(), &mut len) };
        close(key);
        (rc == 0 && len > 0).then_some(data[0])
    }

    pub fn put(path: &str, name: &str, command: &str) -> Result<(), String> {
        let key = open(path, KEY_SET_VALUE, true)?;
        let (value, data) = (wide(name), wide(command));
        // SAFETY: `data` is a zero-ended wide string; the size is in bytes with the ending.
        let rc = unsafe {
            RegSetValueExW(key, value.as_ptr(), 0, REG_SZ, data.as_ptr().cast::<u8>(), (data.len() * 2) as u32)
        };
        close(key);
        if rc == 0 { Ok(()) } else { Err(format!("registry {rc}")) }
    }

    pub fn remove(path: &str, name: &str) -> Result<(), String> {
        let Ok(key) = open(path, KEY_SET_VALUE, false) else { return Ok(()) };
        let value = wide(name);
        // SAFETY: The key is open for writing.
        let rc = unsafe { RegDeleteValueW(key, value.as_ptr()) };
        close(key);
        if rc == 0 || rc == NOT_FOUND { Ok(()) } else { Err(format!("registry {rc}")) }
    }
}

mod files {
    use std::path::Path;
    #[cfg(not(windows))]
    use std::path::PathBuf;

    use super::{desktop_command, desktop_file_name, desktop_text};

    /// `$XDG_CONFIG_HOME/autostart` of this process, by default `~/.config/autostart`.
    #[cfg(not(windows))]
    pub fn folder() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("autostart"))
    }

    pub fn get(folder: &Path, name: &str) -> Option<String> {
        let text = std::fs::read_to_string(folder.join(desktop_file_name(name))).ok()?;
        Some(desktop_command(&text).unwrap_or_default())
    }

    pub fn put(folder: &Path, name: &str, command: &str) -> Result<(), String> {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let file = folder.join(desktop_file_name(name));
        let temp = file.with_extension("desktop.teil");
        std::fs::write(&temp, desktop_text(name, command, crate::language::t("s_autostart_h").as_str()))
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &file).map_err(|e| e.to_string())?;
        // As 4.12 does: the desktops want the file executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Does the file leave the entry on (see [`super::desktop_approved`])? Where there is no file: yes.
    pub fn approved(folder: &Path, name: &str) -> bool {
        std::fs::read_to_string(folder.join(desktop_file_name(name))).is_ok_and(|text| super::desktop_approved(&text))
            || !folder.join(desktop_file_name(name)).exists()
    }

    pub fn remove(folder: &Path, name: &str) -> Result<(), String> {
        match std::fs::remove_file(folder.join(desktop_file_name(name))) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Store for System {
    fn possible(&self) -> bool {
        #[cfg(windows)]
        return windows::possible(windows::RUN_KEY);
        #[cfg(not(windows))]
        return true;
    }
    fn get(&self, name: &str) -> Option<String> {
        #[cfg(windows)]
        return windows::get(windows::RUN_KEY, name);
        #[cfg(not(windows))]
        return files::get(&files::folder()?, name);
    }
    fn put(&self, name: &str, command: &str) -> Result<(), String> {
        #[cfg(windows)]
        return windows::put(windows::RUN_KEY, name, command);
        #[cfg(not(windows))]
        return files::put(&files::folder().ok_or("no folder")?, name, command);
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        #[cfg(windows)]
        return windows::remove(windows::RUN_KEY, name);
        #[cfg(not(windows))]
        return files::folder().map_or(Ok(()), |folder| files::remove(&folder, name));
    }
    fn approved(&self, name: &str) -> bool {
        #[cfg(windows)]
        return windows::first_byte(windows::APPROVED_KEY, name).is_none_or(approved_byte);
        #[cfg(not(windows))]
        return files::folder().is_none_or(|folder| files::approved(&folder, name));
    }
}

// ------------------------------------------------------------------ a copy of the data

/// The start with the system of a run on a copy of the data: `.desktop` files in the configuration folder of
/// the copy ([`copy_folder`]), on every system. The desktop never reads them, so nothing starts with the real
/// login; the switch holds, and the file shows what would be written.
pub struct CopyFolder(pub std::path::PathBuf);

/// The autostart folder inside the configuration folder of `host` (`$XDG_CONFIG_HOME/autostart`, by default
/// `~/.config/autostart`); for a copy of the data that lies in the copy.
pub fn copy_folder(host: &crate::paths::Host) -> std::path::PathBuf {
    let base = host.var_set("XDG_CONFIG_HOME").map_or_else(|| Path::new(&host.home()).join(".config"), Into::into);
    base.join("autostart")
}

impl Store for CopyFolder {
    fn possible(&self) -> bool {
        true
    }
    fn get(&self, name: &str) -> Option<String> {
        files::get(&self.0, name)
    }
    fn put(&self, name: &str, command: &str) -> Result<(), String> {
        files::put(&self.0, name, command)
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        files::remove(&self.0, name)
    }
    fn approved(&self, name: &str) -> bool {
        files::approved(&self.0, name)
    }
}

/// The first byte of the value of the task manager (`StartupApproved\Run`): an even number (2, 6) is on, an
/// odd one (3, 7) is off.
#[cfg(any(windows, test))]
fn approved_byte(first: u8) -> bool {
    first & 1 == 0
}

/// Does the `.desktop` file leave the entry on? `Hidden=true` and `X-GNOME-Autostart-enabled=false` (what the
/// settings of the desktop write) switch it off.
pub fn desktop_approved(text: &str) -> bool {
    !text.lines().map(str::trim).any(|line| {
        let line = line.to_ascii_lowercase();
        line == "hidden=true" || line == "x-gnome-autostart-enabled=false"
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_command_is_the_quoted_program_with_nothing_after_it_and_an_appimage_goes_first() {
        let program = PathBuf::from(r"C:\Programs\Verse-Kit\versekit.exe");
        let none: &[String] = &[];
        assert_eq!(command_for(true, &program, None, none), r#""C:\Programs\Verse-Kit\versekit.exe""#);
        assert_eq!(command_for(true, &program, Some(""), none), r#""C:\Programs\Verse-Kit\versekit.exe""#);
        let image = Some("/home/me/Verse-Kit.AppImage");
        assert_eq!(command_for(true, &program, image, none), r#""/home/me/Verse-Kit.AppImage""#);
        assert_eq!(command_for(false, &program, image, none), r#""/home/me/Verse-Kit.AppImage""#);
        let quote = PathBuf::from(r#"C:\a"b\p.exe"#);
        assert_eq!(command_for(true, &quote, None, none), r#""C:\ab\p.exe""#, "no quote inside");
        assert_eq!(command(&program, None, none), command_for(cfg!(windows), &program, None, none));
    }

    /// ⚠⚠ The start with the login of a run on a copy starts the program on that copy, never on the user's own
    /// data.
    #[test]
    fn the_command_of_a_run_on_a_copy_names_the_copy_after_the_program() {
        let copy = ["--daten".to_string(), r"C:\Test kopie".to_string()];
        let program = PathBuf::from(r"C:\P\versekit.exe");
        assert_eq!(command_for(true, &program, None, &copy), r#""C:\P\versekit.exe" "--daten" "C:\Test kopie""#);
        let linux = command_for(false, &program, Some("/h/V.AppImage"), &["--daten".into(), "/h/Test kopie".into()]);
        assert_eq!(linux, r#""/h/V.AppImage" "--daten" "/h/Test kopie""#);
        assert_eq!(crate::desktop_entry::exec_program(&linux), "/h/V.AppImage", "the program stays the first part");
    }

    #[test]
    fn the_switch_of_a_copy_writes_a_file_into_the_copy_and_holds() {
        let dir = crate::testdir::Dir::new("autostart-kopie");
        let root = dir.path().to_string_lossy().into_owned();
        let config = dir.path().join("AppData").to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[("XDG_CONFIG_HOME", &config)], &root);
        let store = CopyFolder(copy_folder(&host));
        assert_eq!(store.0, dir.path().join("AppData").join("autostart"), "inside the copy");
        assert!(store.possible() && !is_on(&store));
        let command = r#""/h/V.AppImage" "--daten" "/h/kopie""#;
        set_on(&store, true, command).unwrap();
        assert!(is_on(&store), "the switch holds");
        assert_eq!(store.get(NAME).as_deref(), Some(command));
        assert!(store.0.join(desktop_file_name(NAME)).is_file());
        set_on(&store, false, command).unwrap();
        assert!(!is_on(&store) && !store.0.join(desktop_file_name(NAME)).exists());
        // Without the configuration folder set: the one below the home of the copy.
        let bare = crate::paths::Host::new(crate::pypath::Flavor::Posix, &[("HOME", "/k")], "/");
        assert_eq!(copy_folder(&bare), Path::new("/k").join(".config").join("autostart"));
    }

    /// What made the switch on a copy jump back: an entry with an empty command counts as off.
    #[test]
    fn an_entry_without_a_command_is_off_so_the_switch_needs_one() {
        let memory = Memory::default();
        set_on(&memory, true, "").unwrap();
        assert!(!is_on(&memory), "written, but off");
    }

    #[test]
    fn the_command_of_the_login_file_is_escaped_as_the_menu_entry_and_reads_back_to_the_path() {
        let path = "/home/a b/50% %/Ü's\"/$x/`y`/back\\slash/V.AppImage";
        let command = command_for(false, Path::new("/unused"), Some(path), &[]);
        assert_eq!(command, crate::desktop_entry::exec_argument(path));
        assert_eq!(command, r#""/home/a b/50%% %%/Ü's\\"/\\$x/\\`y\\`/back\\\\slash/V.AppImage""#);
        let text = desktop_text(NAME, &command, "c");
        assert_eq!(crate::desktop_entry::exec_program(&desktop_command(&text).unwrap()), path);
    }

    #[test]
    fn the_switch_is_the_entry_and_nothing_else() {
        let memory = Memory::default();
        assert!(!is_on(&memory));
        set_on(&memory, true, "\"x\"").unwrap();
        assert!(is_on(&memory));
        assert_eq!(memory.entries().get(NAME).map(String::as_str), Some("\"x\""));
        set_on(&memory, false, "\"x\"").unwrap();
        assert!(!is_on(&memory) && memory.entries().is_empty());
        // An entry without a command does not count as on; switching off what is not there is fine.
        let empty = Memory::with(NAME, "  ");
        assert!(!is_on(&empty));
        assert!(set_on(&Memory::default(), false, "").is_ok());
        // A refused write is an error, not a silent wrong state.
        let locked = Memory::refusing();
        assert!(set_on(&locked, true, "\"x\"").is_err() && !is_on(&locked));
    }

    #[test]
    fn the_entry_of_4_12_is_taken_over_under_the_new_name_and_goes() {
        let old = legacy::AUTOSTART_NAME;
        let memory = Memory::with(old, "\"C:\\Old\\SC-BP-Watcher.exe\"");
        assert!(migrate(&memory, "\"C:\\New\\versekit.exe\""));
        assert_eq!(memory.entries().len(), 1, "the old one is gone: {:?}", memory.entries());
        assert_eq!(memory.get(NAME).as_deref(), Some("\"C:\\New\\versekit.exe\""));
        // Nothing old, nothing done; a new one that is there stays as it is.
        assert!(!migrate(&Memory::default(), "\"x\""));
        let both = Memory::with(old, "\"old\"");
        both.put(NAME, "\"mine\"").unwrap();
        assert!(migrate(&both, "\"other\""));
        assert_eq!(both.get(NAME).as_deref(), Some("\"mine\""));
        assert!(both.get(old).is_none());
        // A write that fails leaves the old entry: the player keeps their start.
        let locked = Memory::refusing();
        locked.entries.borrow_mut().insert(old.to_string(), "\"old\"".to_string());
        assert!(!migrate(&locked, "\"new\""));
        assert!(locked.get(old).is_some());
        // The same where only the new entry is refused and the old one could be removed: it must stay, or the
        // player would be left with no start at all.
        let no_new = Memory::refusing_new();
        no_new.entries.borrow_mut().insert(old.to_string(), "\"old\"".to_string());
        assert!(!migrate(&no_new, "\"new\""));
        assert!(no_new.get(old).is_some(), "the old entry stays: {:?}", no_new.entries());
        assert!(no_new.get(NAME).is_none());
    }

    #[test]
    fn the_entry_of_the_earlier_test_versions_is_taken_over_too_and_both_old_ones_go() {
        let earlier = EARLIER_NAMES[1];
        assert_eq!(NAME, "VerseKit");
        let memory = Memory::with(earlier, "\"C:\\Programs\\Verse-Kit\\versekit.exe\"");
        assert!(migrate(&memory, "\"C:\\New\\versekit.exe\""));
        assert_eq!(memory.entries().len(), 1, "{:?}", memory.entries());
        assert_eq!(memory.get(NAME).as_deref(), Some("\"C:\\New\\versekit.exe\""));
        // Both old names at once: one new entry, no old one left.
        let both = Memory::with(legacy::AUTOSTART_NAME, "\"old\"");
        both.put(earlier, "\"earlier\"").unwrap();
        assert!(migrate(&both, "\"new\""));
        assert_eq!(both.entries().into_iter().collect::<Vec<_>>(), [(NAME.to_string(), "\"new\"".to_string())]);
        // One of them switched off: it only goes, the other is taken over.
        let off = Memory::with_switched_off(earlier, "\"earlier\"");
        assert!(!migrate(&off, "\"new\""));
        assert!(off.entries().is_empty(), "{:?}", off.entries());
    }

    #[test]
    fn whoever_switched_the_start_off_gets_none_by_the_takeover() {
        let old = legacy::AUTOSTART_NAME;
        // No old entry (4.12 deletes it when the switch goes off): nothing is made.
        let none = Memory::default();
        assert!(!migrate(&none, "\"new\""));
        assert!(none.entries().is_empty(), "no entry out of nothing: {:?}", none.entries());
        // An old entry with no command is "off" in 4.12: cleared, no new one.
        let empty = Memory::with(old, "  ");
        assert!(!migrate(&empty, "\"new\""));
        assert!(empty.entries().is_empty(), "{:?}", empty.entries());
        // Switched off in the task manager or the settings of the desktop: cleared, no new one.
        let off = Memory::with_switched_off(old, "\"old\"");
        assert!(!is_on(&off));
        assert!(!migrate(&off, "\"new\""));
        assert!(off.entries().is_empty(), "the switch of the system holds: {:?}", off.entries());
        // On: taken over (the old one goes, the new one runs the new program).
        let on = Memory::with(old, "\"old\"");
        assert!(migrate(&on, "\"new\""));
        assert_eq!(on.entries().len(), 1);
        assert_eq!(on.get(NAME).as_deref(), Some("\"new\""));
    }

    #[test]
    fn the_switches_of_the_system_are_read_from_the_task_manager_byte_and_the_desktop_file() {
        // The value of the task manager starts with 2 or 6 for on, 3 or 7 for off.
        assert!(approved_byte(2) && approved_byte(6));
        assert!(!approved_byte(3) && !approved_byte(7));
        assert!(desktop_approved(&desktop_text("x", "\"/opt/x\"", "c")), "the file we write is on");
        assert!(desktop_approved("[Desktop Entry]\nX-GNOME-Autostart-enabled=true\n"));
        assert!(!desktop_approved("[Desktop Entry]\nExec=x\nX-GNOME-Autostart-enabled=false\n"));
        assert!(!desktop_approved("[Desktop Entry]\nHidden=true\n"));
        assert!(!desktop_approved("[Desktop Entry]\n  hidden=TRUE \n"));
        assert!(desktop_approved("[Desktop Entry]\nHidden=false\n"));
    }

    #[test]
    fn the_desktop_file_has_the_name_of_4_12_for_the_old_entry_and_reads_back_its_command() {
        assert_eq!(desktop_file_name(legacy::AUTOSTART_NAME), "sc-bp-watcher.desktop");
        assert_eq!(desktop_file_name(NAME), "versekit.desktop");
        assert_eq!(desktop_file_name(EARLIER_NAMES[1]), "verse-kit.desktop");
        let text = desktop_text(NAME, "\"/opt/versekit\"", "Waits for the game");
        assert!(text.starts_with("[Desktop Entry]\n") && text.contains("Terminal=false"));
        assert_eq!(desktop_command(&text).as_deref(), Some("\"/opt/versekit\""));
        assert_eq!(desktop_command("[Desktop Entry]\nName=x\n"), None);
    }

    #[test]
    fn the_switch_of_the_system_start_is_found_in_the_command_however_it_is_quoted() {
        assert!(has_system_start(r#""C:\P\versekit.exe" "--systemstart""#));
        assert!(has_system_start(r#""C:\P\versekit.exe" --systemstart"#));
        assert!(has_system_start(r#""/h/V.AppImage" "--daten" "/h/k" "--systemstart""#));
        assert!(!has_system_start(r#""C:\P\versekit.exe""#));
        assert!(!has_system_start(r#""C:\P\versekit.exe" "--daten" "C:\k""#));
        assert!(!has_system_start(r#""C:\systemstart\versekit.exe""#), "a folder of that name is no switch");
        assert!(!has_system_start(""));
    }

    #[test]
    fn an_entry_that_is_on_gets_the_switch_behind_its_own_command_once() {
        for windows in [true, false] {
            let own = r#""C:\P\versekit.exe" "--daten" "C:\k""#;
            let store = Memory::with(NAME, own);
            assert!(add_system_start_for(windows, &store), "windows {windows}");
            let now = store.get(NAME).unwrap();
            assert!(now.starts_with(own), "the program and its arguments stay as they are: {now}");
            assert!(has_system_start(&now), "{now}");
            assert!(!add_system_start_for(windows, &store), "a second time changes nothing");
            assert_eq!(store.get(NAME).as_deref(), Some(now.as_str()));
        }
    }

    /// ⚠ The start with the system is voluntary: writing an entry again must never switch one on.
    #[test]
    fn an_entry_that_is_off_or_not_there_is_left_alone() {
        let nothing = Memory::default();
        assert!(!add_system_start(&nothing) && nothing.entries().is_empty(), "no entry, none made");
        let empty = Memory::with(NAME, "  ");
        assert!(!add_system_start(&empty));
        assert_eq!(empty.get(NAME).as_deref(), Some("  "), "an entry without a command reads as off");
        let off = Memory::with_switched_off(NAME, "\"C:/P/versekit.exe\"");
        assert!(!add_system_start(&off));
        assert_eq!(off.get(NAME).as_deref(), Some("\"C:/P/versekit.exe\""), "the entry of the task manager stays");
        let refusing = Memory { refuse_put: true, ..Memory::with(NAME, "\"C:/P/versekit.exe\"") };
        assert!(!add_system_start(&refusing), "a registry that takes nothing is no change");
        assert_eq!(refusing.get(NAME).as_deref(), Some("\"C:/P/versekit.exe\""));
    }

    /// The registry code, in a key of its own that has nothing to do with the start of programs
    /// (`HKCU\Software\Verse-Kit-Pruefung-<tag of the run>`, removed again). ⚠ NEVER in the `Run` key: a test must not
    /// leave an entry there that starts something with the next login.
    #[cfg(windows)]
    #[test]
    fn the_registry_code_writes_reads_and_removes_a_value_in_a_key_of_its_own() {
        let path = format!(r"Software\Verse-Kit-Pruefung-{}", crate::testdir::run_tag());
        // The key goes away at the end of the test, also when a check in the middle fails.
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                windows::delete_key(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        assert!(!windows::possible(&path), "the key is not there before");
        // The user may write below HKCU\Software; if this fails the test says so instead of passing unchecked.
        windows::put(&path, "Wert", "\"C:\\pruefung.exe\"").expect("a key of its own can be written");
        assert_eq!(windows::get(&path, "Wert").as_deref(), Some("\"C:\\pruefung.exe\""));
        assert_eq!(windows::get(&path, "Anderer"), None);
        windows::put(&path, "Wert", "\"C:\\zwei.exe\"").unwrap();
        assert_eq!(windows::get(&path, "Wert").as_deref(), Some("\"C:\\zwei.exe\""), "written over");
        windows::remove(&path, "Wert").unwrap();
        assert_eq!(windows::get(&path, "Wert"), None);
        assert!(windows::remove(&path, "Wert").is_ok(), "removing what is not there is fine");
        windows::delete_key(&path);
        assert!(windows::remove(&path, "Wert").is_ok(), "and with the key gone too");
    }
}
