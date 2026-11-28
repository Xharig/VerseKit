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

//! The fourth number of the plugin version: the time of the commit the plugin is built from, counted in steps from
//! the start of the current VerseKit version (`streamdeck-plugin/version-start.json`). The Stream Deck program
//! installs a plugin file only when its version is higher than the one installed ("AlreadyInstalled" otherwise), so
//! every build of a new commit must count higher within one version; a new version wins by its third number anyway.
//!
//! The commit time comes from the commit, not from the clock of the build; it grows across `rust` and `main` (counting
//! the commits would not: `main` holds one per release) and a flat checkout knows it too. The number stays at most
//! 65535, the limit many version readers have for one part.
//!
//! `VERSEKIT_PLUGIN_BUILD` set from outside wins; without git the number is 0 and the build says so.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let start_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/version-start.json");
    println!("cargo:rerun-if-changed={}", start_file.display());
    println!("cargo:rerun-if-env-changed=VERSEKIT_PLUGIN_BUILD");
    // A new commit changes HEAD or the branch it names.
    for path in [git(&["rev-parse", "--git-path", "HEAD"]), git(&["rev-parse", "--symbolic-full-name", "HEAD"])
        .and_then(|name| git(&["rev-parse", "--git-path", &name]))]
    .into_iter()
    .flatten()
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let text = std::fs::read_to_string(&start_file).expect("streamdeck-plugin/version-start.json");
    let start: serde_json::Value = serde_json::from_str(&text).expect("version-start.json is JSON");
    let start_unix = start["start_unix"].as_i64().expect("version-start.json: start_unix");
    let step = start["minuten_je_schritt"].as_i64().filter(|m| *m > 0).expect("version-start.json: minuten_je_schritt") * 60;
    // A pre-release (`5.7.0-beta.1`) counts from the start of its release: only the numbers are compared.
    let numbers = env!("CARGO_PKG_VERSION").split(['-', '+']).next().unwrap_or(env!("CARGO_PKG_VERSION"));
    if start["version"].as_str() != Some(numbers) {
        println!(
            "cargo:warning=streamdeck-plugin/version-start.json names version {} but VerseKit is {numbers}: move the start along",
            start["version"]
        );
    }
    let number = match std::env::var("VERSEKIT_PLUGIN_BUILD") {
        Ok(given) => given,
        Err(_) => match git(&["log", "-1", "--format=%ct", "HEAD"]).and_then(|t| t.parse::<i64>().ok()) {
            Some(time) => {
                let steps = ((time - start_unix) / step).max(0);
                assert!(
                    steps <= 65535,
                    "the plugin version would pass 65535 steps since the start of {}: set a newer start in streamdeck-plugin/version-start.json",
                    env!("CARGO_PKG_VERSION")
                );
                steps.to_string()
            }
            None => {
                println!("cargo:warning=no git: the plugin version ends in .0, the Stream Deck program will not update an installed plugin of the same version");
                "0".to_string()
            }
        },
    };
    println!("cargo:rustc-env=VERSEKIT_PLUGIN_BUILD={number}");
}
