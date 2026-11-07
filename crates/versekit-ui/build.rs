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

//! Embeds every Lucide template from `tools/symbol-vorlagen/` (the same set
//! the Python version builds its icons from): writes `icons.rs` with one
//! `(name, svg)` entry per file into `OUT_DIR`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let folder = manifest.join("..").join("..").join("tools").join("symbol-vorlagen");
    println!("cargo:rerun-if-changed={}", folder.display());

    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|e| e == "svg"));
    files.sort_by(|a, b| a.file_stem().cmp(&b.file_stem()));

    let mut code = String::from("pub static SOURCES: &[(&str, &str)] = &[\n");
    for path in &files {
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let _ = writeln!(code, "    ({name:?}, include_str!({:?})),", path.display().to_string());
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_default()).join("icons.rs");
    if let Err(error) = fs::write(&out, code) {
        println!("cargo:warning=icons.rs not written: {error}");
    }
}
