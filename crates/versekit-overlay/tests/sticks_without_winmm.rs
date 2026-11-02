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

//! The overlay asks no stick through winmm: `joyGetPosEx` runs DirectInput in the process, which searches the
//! devices again and can overrun its own heap block there (`0xc0000374`). The sticks of anti-AFK come by Raw Input.
//!
//! The source check turns red as soon as a winmm joystick call stands anywhere in the code of the workspace
//! outside `input_device.rs` (the wait for a press of the binding window, in the main window), or as soon as
//! the overlay reaches that wait. The run check follows the sticks as the overlay does and finds DirectInput
//! not loaded.

use std::path::{Path, PathBuf};

/// The one file that may still call winmm for sticks, and the one file that may call it.
const WINMM_FILE: &str = "crates/versekit-core/src/input_device.rs";
const WAIT_CALLER: &str = "crates/versekit-ui/src/joysticks_bind.rs";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every `.rs` file under `folder` that is built into a program, with its path relative to the workspace
/// (forward slashes). The `tests` folders are programs of their own (this check among them).
fn sources(folder: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut open = vec![folder.to_path_buf()];
    while let Some(dir) = open.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !path.ends_with("tests") && !path.ends_with("target") {
                    open.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let relative = path.strip_prefix(workspace()).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.push((relative, text));
            }
        }
    }
    out.sort();
    out
}

/// A line without its comment: everything from `//` on that stands outside a string literal.
fn code_of(line: &str) -> &str {
    let bytes = line.as_bytes();
    let (mut in_string, mut escaped) = (false, false);
    for at in 0..bytes.len() {
        let byte = bytes[at];
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'/' && bytes.get(at + 1) == Some(&b'/') {
            return &line[..at];
        }
    }
    line
}

/// The places (line from 1, the word) where code names `pattern`.
fn found_in(text: &str, pattern: &regex::Regex) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .flat_map(|(n, line)| pattern.find_iter(code_of(line)).map(move |m| (n + 1, m.as_str().to_string())))
        .collect()
}

/// The winmm joystick functions and structures (`joyGetPosEx`, `joyGetNumDevs`, `joyGetDevCapsW`, `JOYINFOEX` …).
fn winmm_joystick() -> regex::Regex {
    regex::Regex::new(r"\b(joy[A-Z][A-Za-z]*|JOYINFO(EX)?|JOYCAPS[AW]?)\b").expect("pattern")
}

#[test]
fn the_check_finds_a_call_in_code_and_none_in_a_comment() {
    // The check first catches what it must catch, before it is trusted with the tree.
    let sample = "let n = unsafe { joyGetNumDevs() }; // joyGetDevCapsW never\n/// `joyGetPosEx` in a doc\n\
                  let text = \"// joySetCapture in a string after slashes\"; let i = JOYINFOEX::default();\n";
    let found = found_in(sample, &winmm_joystick());
    assert_eq!(
        found,
        [(1, "joyGetNumDevs".to_string()), (3, "joySetCapture".to_string()), (3, "JOYINFOEX".to_string())]
    );
}

#[test]
fn no_winmm_joystick_call_stands_outside_the_wait_of_the_binding_window() {
    let all = sources(&workspace().join("crates"));
    let names: Vec<&str> = all.iter().map(|(path, _)| path.as_str()).collect();
    for needed in
        [WINMM_FILE, WAIT_CALLER, "crates/versekit-core/src/anti_afk.rs", "crates/versekit-overlay/src/app.rs"]
    {
        assert!(names.contains(&needed), "{needed} was not read: the check would look at nothing");
    }
    let pattern = winmm_joystick();
    let mut wrong = Vec::new();
    let mut allowed = 0;
    for (path, text) in &all {
        for (line, word) in found_in(text, &pattern) {
            if path == WINMM_FILE {
                allowed += 1;
            } else {
                wrong.push(format!("{path}:{line} {word}"));
            }
        }
    }
    assert!(wrong.is_empty(), "winmm for sticks outside {WINMM_FILE}:\n{}", wrong.join("\n"));
    assert!(allowed > 0, "the wait of the binding window still reads winmm: the check must see it there");
}

#[test]
fn only_the_binding_window_of_the_main_window_reaches_the_winmm_wait() {
    // The functions of `input_device` that call winmm: the wait for a press and the question whether one can be
    // waited for.
    let reach = regex::Regex::new(r"input_device::(wait|available)\b|input_device::\{[^}]*\b(wait|available)\b")
        .expect("pattern");
    let mut wrong = Vec::new();
    for (path, text) in sources(&workspace().join("crates")) {
        if path == WINMM_FILE || path == WAIT_CALLER {
            continue;
        }
        for (line, word) in found_in(&text, &reach) {
            wrong.push(format!("{path}:{line} {word}"));
        }
    }
    assert!(wrong.is_empty(), "the winmm wait is reached outside {WAIT_CALLER}:\n{}", wrong.join("\n"));
    // The overlay builds neither the device list, the binding window nor the main window that holds it.
    let overlay = regex::Regex::new(r"\b(input_device|joysticks_bind|joysticks_page|SamplePage)\b").expect("pattern");
    let mut reached = Vec::new();
    for (path, text) in sources(&workspace().join("crates/versekit-overlay/src")) {
        for (line, word) in found_in(&text, &overlay) {
            reached.push(format!("{path}:{line} {word}"));
        }
    }
    assert!(reached.is_empty(), "the overlay reaches the sticks of the main window:\n{}", reached.join("\n"));
    let bind = std::fs::read_to_string(workspace().join(WAIT_CALLER)).expect("the binding window");
    assert!(!found_in(&bind, &reach).is_empty(), "the check must see the one caller where it is");
}

/// Following the sticks as the overlay does loads no DirectInput into the process.
#[cfg(windows)]
#[test]
fn following_the_sticks_loads_no_directinput() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    let loaded = |name: &str| {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: asks whether a module of this name is loaded; the name lives during the call.
        !unsafe { GetModuleHandleW(wide.as_ptr()) }.is_null()
    };
    // The check first sees a module that is there.
    assert!(loaded("kernel32.dll"));
    // Passive: the registration of this test process for the game devices; nothing is sent, nothing is shown.
    let listener = versekit_core::stick_input::Listener::start().expect("Raw Input for the game devices");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(listener.running());
    let _ = listener.states();
    for name in ["dinput.dll", "dinput8.dll"] {
        assert!(!loaded(name), "{name} is loaded");
    }
}
