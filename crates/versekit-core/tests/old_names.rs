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

//! The earlier name of the program stands only where it belongs. The check goes over every text file the
//! repository tracks and looks for the earlier name in its spellings (`sc`, an optional space, underscore or
//! hyphen, `bp`, without regard to case). A hit is allowed in exactly the places of [`ALLOWED`], each with its
//! reason; any other hit is a defect: new code names the variables with `envvars`, the files and folders of
//! earlier versions with `legacy`.
//!
//! The check does not contain the name itself: its own file is a hit like any other.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where in a file hits are allowed.
#[derive(Clone, Copy)]
enum Where {
    /// Anywhere in the file.
    Anywhere,
    /// Only between a line with `old-names: begin` and a line with `old-names: end`, in any kind of comment.
    BetweenMarks,
    /// Only in lines that contain this text.
    LinesWith(&'static str),
}

/// A place where the earlier name may stand, with the reason.
struct Allowed {
    file: &'static str,
    at: Where,
    reason: &'static str,
}

const ALLOWED: [Allowed; 9] = [
    Allowed {
        file: "crates/versekit-core/src/legacy.rs",
        at: Where::Anywhere,
        reason: "the one module that knows what earlier versions called their files, folders and variables",
    },
    Allowed {
        file: "CHANGELOG.md",
        at: Where::Anywhere,
        reason: "the history: earlier releases came out under the earlier name",
    },
    Allowed {
        file: "CHANGELOG.en.md",
        at: Where::Anywhere,
        reason: "the history: earlier releases came out under the earlier name",
    },
    Allowed {
        file: "packaging/installer.iss",
        at: Where::BetweenMarks,
        reason: "the installer ends, replaces and clears what an installation under the earlier name left",
    },
    Allowed {
        file: "tools/deinstaller_pruefen.py",
        at: Where::BetweenMarks,
        reason: "the check of the uninstaller needs the folders an earlier version made, to see them go",
    },
    Allowed {
        file: "index.html",
        at: Where::LinesWith("alternateName"),
        reason: "the other names the program is known by to search engines (decision of 7 November 2026: stays)",
    },
    Allowed {
        file: "en/index.html",
        at: Where::LinesWith("alternateName"),
        reason: "the other names the program is known by to search engines (decision of 7 November 2026: stays)",
    },
    Allowed {
        file: "fragen/index.html",
        at: Where::LinesWith("%LOCALAPPDATA%\\Programs"),
        reason: "the way to the plugin file by hand: users of older installations find the folder under the earlier name",
    },
    Allowed {
        file: "en/fragen/index.html",
        at: Where::LinesWith("%LOCALAPPDATA%\\Programs"),
        reason: "the way to the plugin file by hand: users of older installations find the folder under the earlier name",
    },
];

/// The largest file that is read (the text files of the repository are far below).
const LARGEST: u64 = 4 * 1024 * 1024;

const BEGIN: &str = "old-names: begin";
const END: &str = "old-names: end";

/// Does `line` name the earlier program: `sc`, one optional space, underscore or hyphen, `bp`, in any case?
fn names_the_earlier_program(line: &str) -> bool {
    let bytes = line.to_ascii_lowercase().into_bytes();
    (0..bytes.len().saturating_sub(3)).any(|at| {
        if &bytes[at..at + 2] != b"sc" {
            return false;
        }
        let mut rest = &bytes[at + 2..];
        if rest.first().is_some_and(|b| matches!(b, b' ' | b'_' | b'-')) {
            rest = &rest[1..];
        }
        rest.starts_with(b"bp")
    })
}

/// The root of the repository: two folders above this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The files the repository tracks, with `/` in the path.
fn tracked_files() -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .expect("git runs, the check reads the files the repository tracks");
    assert!(out.status.success(), "git ls-files: {}", String::from_utf8_lossy(&out.stderr));
    let names = out.stdout.split(|b| *b == 0).filter(|name| !name.is_empty());
    names.map(|name| String::from_utf8_lossy(name).into_owned()).collect()
}

/// The hits of a file as `(line number, line)`, with whether the line stands between the marks.
fn hits(text: &str) -> Vec<(usize, String, bool)> {
    let mut inside = false;
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.contains(BEGIN) {
            inside = true;
            continue;
        }
        if line.contains(END) {
            inside = false;
            continue;
        }
        if names_the_earlier_program(line) {
            found.push((number + 1, line.to_string(), inside));
        }
    }
    found
}

/// Is the hit allowed by one of the places for its file? Marks which allowance it used.
fn allowed(file: &str, line: &str, between_marks: bool, used: &mut [bool; ALLOWED.len()]) -> bool {
    for (n, allowance) in ALLOWED.iter().enumerate().filter(|(_, a)| a.file == file) {
        let fits = match allowance.at {
            Where::Anywhere => true,
            Where::BetweenMarks => between_marks,
            Where::LinesWith(text) => line.contains(text),
        };
        if fits {
            used[n] = true;
            return true;
        }
    }
    false
}

#[test]
fn the_earlier_name_stands_only_where_it_belongs() {
    let base = root();
    let mut used = [false; ALLOWED.len()];
    let mut stray = Vec::new();
    for file in tracked_files() {
        let path = base.join(&file);
        if !fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= LARGEST) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        for (number, line, between_marks) in hits(&text) {
            if !allowed(&file, &line, between_marks, &mut used) {
                stray.push(format!("{file}:{number}: {}", line.trim()));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "The earlier name stands where it does not belong (new variables come from `envvars`, the names of earlier \
         versions from `legacy`):\n{}",
        stray.join("\n")
    );
    let unused: Vec<String> =
        ALLOWED.iter().zip(used).filter(|(_, used)| !used).map(|(a, _)| format!("{}: {}", a.file, a.reason)).collect();
    assert!(unused.is_empty(), "An allowance no longer needed goes from the list:\n{}", unused.join("\n"));
}

/// The marks stand in pairs: a begin without an end would let every hit below it pass.
#[test]
fn the_marks_of_the_earlier_name_stand_in_pairs() {
    let base = root();
    for file in tracked_files() {
        if !ALLOWED.iter().any(|a| a.file == file && matches!(a.at, Where::BetweenMarks)) {
            continue;
        }
        let text = fs::read_to_string(base.join(&file)).expect("the file");
        let mut inside = false;
        for (number, line) in text.lines().enumerate() {
            if line.contains(BEGIN) {
                assert!(!inside, "{file}:{}: a begin inside a pair", number + 1);
                inside = true;
            } else if line.contains(END) {
                assert!(inside, "{file}:{}: an end without a begin", number + 1);
                inside = false;
            }
        }
        assert!(!inside, "{file}: a begin without an end");
    }
}

/// The installer of the new name deals with the program file of the earlier name in all three places: it ends the
/// running one, it removes the old file after the installation, and the uninstaller removes it too. The setup itself
/// runs only in the checks of `tools/` (they need a built setup); this reads the script, so that taking one of the
/// three out turns something red without one.
#[test]
fn the_installer_ends_and_removes_the_program_file_of_the_earlier_name() {
    let text = fs::read_to_string(root().join("packaging").join("installer.iss")).expect("the installer script");
    let after = |marker: &str| text.split_once(marker).map_or("", |(_, rest)| rest);
    let section = after("[UninstallDelete]");
    let section = section.split("\n[").next().unwrap_or("");
    assert!(
        section.contains(r#"Type: files; Name: "{app}\{#OldProgramFile}""#),
        "the uninstaller does not remove the old program file"
    );
    let post_install = after("ssPostInstall");
    assert!(
        post_install.contains(r"DeleteFile(ExpandConstant('{app}\{#OldProgramFile}'))"),
        "the installer does not remove the old program file after the installation"
    );
    assert!(
        text.contains("/IM {#ProgramFile} /IM {#OldProgramFile}"),
        "the installer does not end a running program under the old file name"
    );
}

#[test]
fn the_check_finds_every_spelling_and_nothing_else() {
    let home = ["SC", "_BP", "_HOME"].concat();
    let spellings = [
        home.as_str(),
        &["sc", "-bp", "-watcher"].concat(),
        &["SC", " BP", " Watcher"].concat(),
        &["sc", "bp", "-update-setup"].concat(),
        &["[SC", "BPW]"].concat(),
        &["Sc", "Bp"].concat(),
    ];
    for text in spellings {
        assert!(names_the_earlier_program(&format!("x {text} y")), "{text}");
    }
    for text in ["VERSEKIT_HOME", "discount bp", "disc", "scb", "a sc", "bp sc", "sc  bp", "sc.bp"] {
        assert!(!names_the_earlier_program(text), "{text}");
    }
    // The pattern as a part of a longer word counts as a hit as well: such a word is no name of the program.
    assert!(names_the_earlier_program(&["mi", "sc", "-", "bp", "m"].concat()));
}
