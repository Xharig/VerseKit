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

//! `versekit-parity satz <auszug> <typ> <pfadteil> [<muster>]`: prints, as
//! JSON, the first record of type `<typ>` whose record path holds
//! `<pfadteil>` (and, with `<muster>`, whose JSON holds that text), to look
//! at the fields of the game data. `<auszug>` is a folder made by
//! `versekit-parity auszug`.

use std::path::Path;

use p4kx_dataforge::{DataForge, JsonLayout, RecordJson};

pub fn run(extract: &str, type_name: &str, path_part: &str, pattern: Option<&str>) -> Result<(), String> {
    let bytes = std::fs::read(Path::new(extract).join("Game2.dcb")).map_err(|e| e.to_string())?;
    let forge = DataForge::open(&bytes).map_err(|e| e.to_string())?;
    let mut writer = RecordJson::new(JsonLayout::Pretty);
    for record in forge.records() {
        let (Ok(found_type), Ok(path)) = (record.type_name(), record.record_path()) else { continue };
        if found_type != type_name || !path.to_ascii_lowercase().contains(&path_part.to_ascii_lowercase()) {
            continue;
        }
        let Ok(tree) = record.tree() else { continue };
        let mut out = Vec::new();
        writer.write(&tree, &mut out).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if pattern.is_some_and(|pattern| !text.contains(pattern)) {
            continue;
        }
        println!("{path}\n{text}");
        return Ok(());
    }
    Err("kein Datensatz gefunden".into())
}
