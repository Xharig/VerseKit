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

//! The refinery terminal read from the screen of the game (`scan_from_game`, `read_screen`, `wait_for_game`),
//! the pictures kept of readings that were not sure (`keep`), and the handing over of a reading from the overlay
//! to the page Mein Lager.
//!
//! ⚠ Read only while Star Citizen is in front: else the grab photographs the desktop.
//!
//! ⚠ The overlay reads (it hears the key in the game), the main window shows the lines: two processes. The
//! reading waits in [`HANDOVER_FILE`] until the page takes it; a newer one replaces an older one, as in 4.12
//! there is one place for a reading that waits.

use std::time::{Duration, Instant};

use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::screen_grab;
use crate::stock_data::StockData;
use crate::win_ocr::{self, Bitmap, Recognition};

use super::digits::{Cell, Digits, Gray};
use super::{Area, Index, Keep, Reason, ScanError, ScannedJob, State, read_image};

/// How many pictures of readings that were not sure are kept for a bug report.
pub const KEEP_SCANS: usize = 3;
/// The widest a kept whole picture is.
pub const KEEP_MAX_WIDTH: i64 = 2560;
pub const KEEP_FOLDER: &str = "raffinerie-bilder";
/// The reading that waits for the page Mein Lager.
pub const HANDOVER_FILE: &str = "raffinerie-lesung.json";

/// Can this system read the screen at all (Windows with its text recognition)?
pub fn supported() -> bool {
    win_ocr::supported() && screen_grab::supported()
}

/// The folder of the kept pictures, beside the files of the program.
pub fn keep_folder(paths: &Paths) -> String {
    let file = paths.app_file("x");
    let folder = file.trim_end_matches(|c| c != '/' && c != '\\').trim_end_matches(['/', '\\']);
    format!("{folder}{}{KEEP_FOLDER}", if file.contains('\\') { "\\" } else { "/" })
}

/// The kept pictures (file names), oldest first.
pub fn kept_images(paths: &Paths) -> Vec<String> {
    kept_in(&keep_folder(paths))
}

/// The kept pictures in `folder` (file names), oldest first.
pub fn kept_in(folder: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("raffinerie_") && n.ends_with(".png"))
        .collect();
    names.sort();
    names
}

/// The name of a kept picture: `raffinerie_<YYYYMMDD-HHMMSS>-<ms>_<kind>.png`.
fn kept_name(kind: &str) -> String {
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_millis());
    format!("raffinerie_{}-{millis:03}_{kind}.png", jsonio::time_stamp())
}

/// `keep`: a PNG laid into the folder of the kept pictures; afterwards only the newest [`KEEP_SCANS`] stay.
pub fn keep(paths: &Paths, png: &[u8], kind: &str) -> bool {
    let folder = keep_folder(paths);
    let written = jsonio::make_dirs(&folder)
        .and_then(|()| jsonio::write_new(&format!("{folder}/{}", kept_name(kind)), png))
        .map(|()| {
            let names = kept_images(paths);
            for old in &names[..names.len().saturating_sub(KEEP_SCANS)] {
                let _ = jsonio::remove_file(&format!("{folder}/{old}"));
            }
        });
    match written {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("refinery_scan.keep", &e);
            false
        }
    }
}

/// `wait_for_game`: waits until Star Citizen is in front, at most `timeout`; never brings a window forward
/// itself. The area of the window, or `None`. After the change a moment's rest, until the picture of the game is
/// drawn whole again.
pub fn wait_for_game(timeout: Duration, step: Duration, sleep: &dyn Fn(Duration)) -> Option<screen_grab::Area> {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if let Some(rect) = screen_grab::game_rect() {
            sleep(Duration::from_millis(600));
            return screen_grab::game_rect().or(Some(rect));
        }
        sleep(step);
    }
    None
}

/// The materials of the player with the goods counted in pieces.
pub fn index(paths: &Paths) -> Index {
    Index::of(&StockData::load(|name| paths.app_file(name)))
}

/// `read_screen`: the area of the game grabbed and read, `(jobs, kept)`. A reading that stays empty or unsure
/// keeps a picture: the part of the table, or the whole picture at most [`KEEP_MAX_WIDTH`] wide when no table
/// was found.
pub fn read_screen(paths: &Paths, rect: screen_grab::Area) -> Result<(Vec<ScannedJob>, bool), ScanError> {
    let raw = screen_grab::grab_raw(rect).map_err(|e| ScanError::new(Reason::Grab, e.key()))?;
    let bitmap = Bitmap { width: i64::from(rect.2), height: i64::from(rect.3), raw };
    let size = (bitmap.width, bitmap.height);
    let mut keep_one = |what: Keep| {
        let (area, most, kind) = match what {
            Keep::Picture => ((0, 0, size.0, size.1), Some(KEEP_MAX_WIDTH), "bild"),
            Keep::Table(area) => (clamped(area, size), None, "tabelle"),
        };
        win_ocr::png_of(&bitmap, area, most).is_ok_and(|png| keep(paths, &png, kind))
    };
    let learned = Digits::load(paths);
    let mut ocr = Recognition::new(&bitmap);
    read_image(size, Some(&bitmap), &mut ocr, &index(paths), &learned, Some(&mut keep_one))
}

/// A part cut to the picture, as `ocr_image` lays the part of a table aside.
fn clamped(area: Area, size: (i64, i64)) -> Area {
    let (x, y) = (area.0.max(0), area.1.max(0));
    (x, y, area.2.min(size.0 - x), area.3.min(size.1 - y))
}

/// `scan_from_game`: read while Star Citizen is in front, without waiting; `None` when it is not. The jobs with
/// a time left go into the store of the jobs.
pub fn scan_from_game(paths: &Paths) -> Result<Option<(Vec<ScannedJob>, bool)>, ScanError> {
    let Some(rect) = screen_grab::game_rect() else { return Ok(None) };
    let read = read_screen(paths, rect)?;
    remember(paths, &read.0);
    Ok(Some(read))
}

/// The jobs of a reading into the store of the jobs (`refinery_jobs.remember`).
pub fn remember(paths: &Paths, jobs: &[ScannedJob]) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    crate::refinery_jobs::remember(paths, jobs, now);
}

// ------------------------------------------------------------ handing over

fn gray_value(g: &Gray) -> Value {
    Value::Object(vec![
        ("breite".into(), Value::int(g.width as i64)),
        ("hoehe".into(), Value::int(g.height as i64)),
        ("grau".into(), Value::str(base64_of(&g.gray))),
    ])
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn bytes_of(text: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

/// A reading as it waits in [`HANDOVER_FILE`].
pub fn reading_value(jobs: &[ScannedJob], kept: bool) -> Value {
    let strings = |list: &[String]| Value::Array(list.iter().map(|s| Value::str(s.clone())).collect());
    let job = |j: &ScannedJob| {
        Value::Object(vec![
            ("text".into(), Value::str(j.text.clone())),
            ("unsure".into(), strings(&j.unsure)),
            ("materials".into(), strings(&j.materials)),
            ("total".into(), Value::str(j.total.clone())),
            ("state".into(), Value::str(j.state.word())),
            ("remaining".into(), j.remaining.map_or(Value::Null, Value::int)),
            (
                "cells".into(),
                Value::Array(
                    j.cells
                        .iter()
                        .map(|c| {
                            Value::Object(vec![
                                ("material".into(), Value::str(c.material.clone())),
                                ("column".into(), Value::int(c.column as i64)),
                                ("read".into(), Value::str(c.read.clone())),
                                ("crop".into(), gray_value(&c.crop)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    };
    Value::Object(vec![
        ("auftraege".into(), Value::Array(jobs.iter().map(job).collect())),
        ("aufgehoben".into(), Value::Bool(kept)),
    ])
}

/// A reading from [`reading_value`]; `None` for anything else.
pub fn reading_of(value: &Value) -> Option<(Vec<ScannedJob>, bool)> {
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let list = |v: &Value, key: &str| match v.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let number = |v: &Value, key: &str| match v.get(key) {
        Some(Value::Int(n)) => n.parse::<i64>().ok(),
        _ => None,
    };
    let Some(Value::Array(items)) = value.get("auftraege") else { return None };
    let mut jobs = Vec::new();
    for item in items {
        let mut cells = Vec::new();
        if let Some(Value::Array(found)) = item.get("cells") {
            for c in found {
                let crop = c.get("crop")?;
                let gray = bytes_of(&text(crop, "grau")?)?;
                let (width, height) = (number(crop, "breite")? as usize, number(crop, "hoehe")? as usize);
                if gray.len() != width * height {
                    return None;
                }
                cells.push(Cell {
                    material: text(c, "material")?,
                    column: number(c, "column")? as usize,
                    read: text(c, "read").unwrap_or_default(),
                    crop: Gray { width, height, gray },
                });
            }
        }
        jobs.push(ScannedJob {
            text: text(item, "text").unwrap_or_default(),
            unsure: list(item, "unsure"),
            materials: list(item, "materials"),
            total: text(item, "total").unwrap_or_default(),
            state: match text(item, "state").as_deref() {
                Some("fertig") => State::Done,
                Some("laeuft") => State::Running,
                _ => State::Unknown,
            },
            remaining: number(item, "remaining"),
            cells,
        });
    }
    Some((jobs, value.get("aufgehoben") == Some(&Value::Bool(true))))
}

/// `hand_over` between the processes: the reading waits for the page; a newer one replaces it.
pub fn hand_over(paths: &Paths, jobs: &[ScannedJob], kept: bool) -> bool {
    let text = jsonio::dumps(&reading_value(jobs, kept), None, false);
    match jsonio::write_text_atomic(&paths.app_file(HANDOVER_FILE), &text, jsonio::Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("refinery_scan.hand_over", &e);
            false
        }
    }
}

/// `listen`: the reading that waits, taken away; `None` when there is none.
pub fn take_reading(paths: &Paths) -> Option<(Vec<ScannedJob>, bool)> {
    let file = paths.app_file(HANDOVER_FILE);
    let value = jsonio::read_json(&file).ok()?;
    let _ = jsonio::remove_file(&file);
    reading_of(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_waits_as_it_was_read_and_is_taken_once() {
        let jobs = vec![ScannedJob {
            text: "Gold 553 122 cSCU".into(),
            unsure: vec!["Gold".into()],
            materials: vec!["Gold".into(), "Iron".into()],
            total: "12.76".into(),
            state: State::Running,
            remaining: Some(3780),
            cells: vec![Cell {
                material: "Gold".into(),
                column: 1,
                read: "122".into(),
                crop: Gray { width: 2, height: 2, gray: vec![0, 255, 30, 7] },
            }],
        }];
        let value = reading_value(&jobs, true);
        let text = jsonio::dumps(&value, None, false);
        assert_eq!(reading_of(&jsonio::loads(&text).expect("json")), Some((jobs, true)));
        assert_eq!(reading_of(&Value::str("nichts")), None);
    }

    #[test]
    fn only_the_newest_pictures_stay() {
        let dir = crate::testdir::Dir::new("raffinerie-bilder");
        let home = dir.path().to_string_lossy().into_owned();
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[(crate::legacy::ENV_HOME, home.as_str())],
            &home,
        ));
        for kind in ["bild", "tabelle", "tabelle", "bild", "tabelle"] {
            assert!(keep(&paths, b"\x89PNG\r\n\x1a\n", kind));
            std::thread::sleep(Duration::from_millis(3));
        }
        let names = kept_images(&paths);
        assert_eq!(names.len(), KEEP_SCANS, "{names:?}");
        assert!(names[2].ends_with("_tabelle.png") && names[1].ends_with("_bild.png"), "{names:?}");
        assert!(keep_folder(&paths).ends_with(KEEP_FOLDER));
    }
}
