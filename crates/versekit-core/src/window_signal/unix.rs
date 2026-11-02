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

//! The signals elsewhere than Windows. For the window with a role in a run (its mark `<name>`) the folder of the
//! user holds `<name>.sperre` (locked by the process that serves the role, for as long as it runs), `<name>.pid`
//! (that process) and `<name>.sock` (the socket it listens on); the single instance of a run is
//! `<overlay name>.instanz` with `<overlay name>.instanz-pid`.
//!
//! ⚠ A process that ends without a word leaves its socket file behind: a role counts as served only while its
//! socket takes a connection, and the next process that serves it replaces the file (it holds the lock first).

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use super::{
    Asked, REPLACE_WAIT, Role, Signal, close_hook, decode, deliver, encode, handle_of, lock_file, name_of, outlive,
    read_pid, signal_folder, take_instance,
};

/// How long a signal may take to be written, and a connection to be read.
const WAIT: Duration = Duration::from_secs(1);

/// The locked files of this process: the roles it serves and the single instance, held until it ends.
static HELD: Mutex<Vec<std::fs::File>> = Mutex::new(Vec::new());

fn hold(file: std::fs::File) {
    HELD.lock().unwrap_or_else(PoisonError::into_inner).push(file);
}

/// The folder of the signals, made if missing; `None` when there is none this user alone may write into (in a
/// shared temporary folder another user could have made it first).
fn folder() -> Option<&'static PathBuf> {
    static FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
            // SAFETY: getuid has no preconditions and cannot fail.
            let uid = unsafe { libc::getuid() };
            let dir = signal_folder(std::env::var_os("XDG_RUNTIME_DIR").as_deref(), &std::env::temp_dir(), uid);
            let _ = std::fs::DirBuilder::new().mode(0o700).create(&dir);
            let meta = std::fs::symlink_metadata(&dir).ok()?;
            (meta.is_dir() && meta.uid() == uid && meta.permissions().mode() & 0o077 == 0).then_some(dir)
        })
        .as_ref()
}

fn file(handle: isize, suffix: &str) -> Option<PathBuf> {
    Some(folder()?.join(format!("{}.{suffix}", name_of(handle))))
}

/// A connection to the process that serves `handle`, if it runs.
fn connect(handle: isize) -> Option<UnixStream> {
    UnixStream::connect(file(handle, "sock")?).ok()
}

pub fn serve(_hwnd: isize, role: Role, key: &str, on_signal: impl Fn(Signal) + Send + Sync + 'static) -> bool {
    let handle = handle_of(role, key);
    let (Some(lock), Some(pid), Some(socket)) = (file(handle, "sperre"), file(handle, "pid"), file(handle, "sock"))
    else {
        return false;
    };
    // Another process serves the role already.
    let Ok(Some(locked)) = lock_file(&lock, &pid) else { return false };
    let _ = std::fs::remove_file(&socket);
    let Ok(listener) = UnixListener::bind(&socket) else { return false };
    let listening =
        std::thread::Builder::new().name("fenstersignale".into()).spawn(move || listen(&listener, on_signal));
    if listening.is_err() {
        return false;
    }
    hold(locked);
    true
}

/// Takes the connections to the socket one by one and does what each byte asks; a connection that only looks
/// whether the role is served brings none.
fn listen(listener: &UnixListener, on_signal: impl Fn(Signal)) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        let _ = stream.set_read_timeout(Some(WAIT));
        let mut bytes = Vec::new();
        let _ = stream.take(64).read_to_end(&mut bytes);
        for asked in bytes.into_iter().filter_map(decode) {
            deliver(asked, close_hook().as_ref(), &on_signal);
        }
    }
}

pub fn find(role: Role, key: &str) -> Option<isize> {
    let handle = handle_of(role, key);
    connect(handle).map(|_| handle)
}

/// A role has one socket, so one window at most.
pub fn find_all(role: Role, key: &str) -> Vec<isize> {
    find(role, key).into_iter().collect()
}

fn write(handle: isize, asked: Asked) -> bool {
    let Some(mut stream) = connect(handle) else { return false };
    let _ = stream.set_write_timeout(Some(WAIT));
    stream.write_all(&[encode(asked)]).is_ok()
}

pub fn send(hwnd: isize, signal: Signal) -> bool {
    write(hwnd, Asked::Signal(signal))
}

pub fn close_window(hwnd: isize) -> bool {
    write(hwnd, Asked::Close)
}

/// Under X11 the window of the process that serves `hwnd` (`super::x11`); elsewhere nothing to do from outside:
/// the signals that ask for the window show it themselves (it may be hidden).
pub fn bring_to_front(hwnd: isize) {
    #[cfg(not(target_os = "macos"))]
    match process_of(hwnd) {
        0 => {}
        pid => {
            super::x11::bring_to_front(pid);
        }
    }
    #[cfg(target_os = "macos")]
    let _ = hwnd;
}

/// Under X11 whether the window of the process that serves `hwnd` is shown (`super::x11`); where that cannot be
/// told (no X11, no window of it found, macOS) it counts as shown.
pub fn is_shown(hwnd: isize) -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        match process_of(hwnd) {
            0 => true,
            pid => super::x11::is_shown(pid).unwrap_or(true),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = hwnd;
        true
    }
}

/// The socket of a role is served by a thread of its own, which takes a connection also while the window hangs.
pub fn answers(_hwnd: isize) -> bool {
    true
}

pub fn process_of(hwnd: isize) -> u32 {
    if connect(hwnd).is_none() {
        return 0;
    }
    file(hwnd, "pid").and_then(|pid| read_pid(&pid)).unwrap_or(0)
}

fn instance_files(key: &str) -> Option<(PathBuf, PathBuf)> {
    let handle = handle_of(Role::Overlay, key);
    Some((file(handle, "instanz")?, file(handle, "instanz-pid")?))
}

/// See [`super::first_instance`]. A start that cannot tell (no folder, no lock on this file system) goes on
/// as the first one.
///
/// ⚠ The lock of the replaced version may be free a moment before its socket stops taking connections (a process
/// that ends closes its files one by one), or that version ran without the lock (it was not the first start):
/// the start that replaces it waits until its overlay no longer answers, or it would hand over to it and end.
pub fn first_instance(key: &str, replaces: Option<u32>) -> bool {
    let first = take_lock(key, replaces);
    if let (true, Some(replaced)) = (first, replaces) {
        let overlay = handle_of(Role::Overlay, key);
        outlive(|| process_of(overlay) == replaced, REPLACE_WAIT);
    }
    first
}

fn take_lock(key: &str, replaces: Option<u32>) -> bool {
    let Some((lock, pid)) = instance_files(key) else { return true };
    let mut taken = None;
    let first = take_instance(
        || match lock_file(&lock, &pid) {
            Ok(Some(file)) => {
                taken = Some(file);
                true
            }
            Ok(None) => false,
            Err(_) => true,
        },
        || read_pid(&pid),
        replaces,
        REPLACE_WAIT,
    );
    if let Some(file) = taken {
        hold(file);
    }
    first
}

pub fn instance_owner(key: &str) -> Option<u32> {
    match process_of(handle_of(Role::Overlay, key)) {
        0 => read_pid(&instance_files(key)?.1),
        overlay => Some(overlay),
    }
}

/// The files of the window `handle` (for the checks, which leave none behind).
#[cfg(test)]
pub fn files_of(handle: isize) -> Vec<PathBuf> {
    ["sperre", "pid", "sock"].iter().filter_map(|suffix| file(handle, suffix)).collect()
}
