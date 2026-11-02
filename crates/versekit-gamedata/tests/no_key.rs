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

//! VerseKit never needs nor brings a key for the `Data.p4k`: the DataForge
//! and the `global.ini` files are read without one, an encrypted entry is
//! noted in the error log instead of being asked for, no code of VerseKit
//! sets a key, and the p4k-extractor in the build holds none.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use versekit_core::error_log::ErrorLog;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_gamedata::{Error, GameData, Language, record_encrypted};

#[test]
fn game_data_is_read_without_a_key() {
    let game = GameData::open(common::channel("without_key")).unwrap();
    let texts = game.localization(Language::German).unwrap();
    assert!(texts.has_own_texts());
    assert!(!game.blueprints(&texts).unwrap().blueprints.is_empty());
    game.values(&texts).unwrap();
}

#[test]
fn an_encrypted_dataforge_is_noted_not_asked_for() {
    let game = GameData::open(common::encrypted_channel("encrypted")).unwrap();
    // The encrypted German texts count as missing: English stands in.
    let texts = game.localization(Language::German).unwrap();
    assert!(!texts.has_own_texts());
    let error = game.values(&texts).unwrap_err();
    assert!(error.is_encrypted(), "{error}");
    assert!(game.blueprints(&texts).unwrap_err().is_encrypted());

    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("versekit-gamedata").join("encrypted-log");
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let dir = home.to_string_lossy().to_string();
    let vars = [("SC_BP_HOME", dir.as_str()), ("HOME", dir.as_str()), ("USERPROFILE", dir.as_str())];
    let paths = Paths::new(Host::new(Flavor::native(), &vars, "/"));
    let log = ErrorLog::new(&paths, "5.0.0-test");
    assert!(record_encrypted(&log, "LIVE", &error));
    assert!(!record_encrypted(&log, "LIVE", &Error::NoGameFolder));
    let entries = log.latest(5);
    assert_eq!(entries.len(), 1);
    let note = entries[0].get("meldung").and_then(|value| value.as_str()).unwrap_or_default().to_string();
    assert!(note.contains("LIVE: Data\\Game2.dcb"), "{note}");
    assert!(!note.contains("s_p3_verschluesselt"), "the text is missing: {note}");
}

/// The Rust files of the workspace, without this one.
fn sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("tests/no_key.rs") {
                out.push((path.clone(), fs::read_to_string(&path).unwrap_or_default()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."), &mut out);
    out
}

#[test]
fn no_code_sets_an_archive_key() {
    const WORDS: [&str; 6] = ["set_key(", "open_with_key(", "ArchiveKey", "ARCHIVE_KEY", "P4KX_KEY_FILE", "KeyStore"];
    let found: Vec<String> = sources()
        .iter()
        .flat_map(|(path, text)| {
            WORDS.iter().filter(|word| text.contains(**word)).map(move |word| format!("{}: {word}", path.display()))
        })
        .collect();
    assert!(found.is_empty(), "VerseKit sets or keeps a key:\n{}", found.join("\n"));
}

#[test]
fn the_extractor_in_the_build_holds_no_archive_key() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--offline", "--manifest-path"])
        .arg(&workspace)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut checked = 0;
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap_or_default();
        if !name.starts_with("p4kx-") {
            continue;
        }
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        let mut files = Vec::new();
        let mut stack = vec![manifest.parent().unwrap().join("src")];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).into_iter().flatten().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        for file in files {
            let text = fs::read_to_string(&file).unwrap_or_default();
            assert!(
                !text.contains("const ARCHIVE_KEY"),
                "{name} brings the archive key into the build: {}",
                file.display()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no source of the p4k-extractor found");
}
