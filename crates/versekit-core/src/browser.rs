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

//! Opening a web address in the user's browser, with an answer whether it
//! worked, so the page can show the address when it did not
//! (`paths.web_address`, `paths.open_in_browser`).
//!
//! ⚠ Only web addresses get through: the system's opener also opens files,
//! network paths and any protocol. Folders have their own way, [`show_folder`].

/// Is `url` a web address: http or https with a host, no control characters,
/// spaces or backslashes (`web_address` without a host list)?
pub fn web_address(url: &str) -> bool {
    if url.is_empty() || url.chars().any(|c| (c as u32) < 33 || c as u32 == 127 || c == '\\') {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else { return false };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = match host_port.strip_prefix('[') {
        Some(inner) => inner.split(']').next().unwrap_or_default(),
        None => host_port.split(':').next().unwrap_or_default(),
    };
    !host.is_empty()
}

/// Opens a web address; false when it is none or nothing could open it.
pub fn open_in_browser(url: &str) -> bool {
    if !web_address(url) {
        crate::errors::record(
            "browser.open_in_browser",
            &format!("keine Webadresse: {:?}", url.chars().take(80).collect::<String>()),
        );
        return false;
    }
    open_native(url)
}

/// Opens a folder in the file manager (`_show_folder`); false when it is no
/// folder or nothing could open it.
pub fn show_folder(path: &str) -> bool {
    std::path::Path::new(path).is_dir() && open_native(path)
}

#[cfg(windows)]
fn open_native(url: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    // SAFETY: both texts end with a zero and live until the call returns.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    result as usize > 32
}

#[cfg(not(windows))]
fn open_native(url: &str) -> bool {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let commands: &[&[&str]] =
        if cfg!(target_os = "macos") { &[&["open"]] } else { &[&["xdg-open"], &["gio", "open"]] };
    for command in commands {
        let Ok(mut child) =
            Command::new(command[0]).args(&command[1..]).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
        else {
            continue;
        };
        // Still running after a second, it took the address; ended with an error, it did not. One that runs on
        // has its end collected later, so no zombie stays.
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return true,
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if started.elapsed() >= Duration::from_secs(1) => {
                    crate::child::reap(child);
                    return true;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_folder_that_exists_is_shown() {
        // Nothing is opened here: both cases are refused before the system is asked.
        assert!(!show_folder(""));
        assert!(!show_folder("/gibt/es/nicht/versekit"));
    }

    #[test]
    fn only_web_addresses() {
        assert!(web_address("https://issue-council.robertsspaceindustries.com/projects/STAR-CITIZEN/issues?search=x"));
        assert!(web_address("http://localhost:8080/"));
        assert!(web_address("HTTPS://[::1]/x"));
        for bad in [
            "",
            "file:///C:/Windows",
            "ftp://x.org",
            "https://",
            "https:///pfad",
            "https://a b",
            "\\\\server\\share",
            "https://x\\y",
        ] {
            assert!(!web_address(bad), "{bad}");
        }
        // Nothing that is no web address reaches the opener.
        assert!(!open_in_browser("file:///C:/Windows/notepad.exe"));
    }
}
