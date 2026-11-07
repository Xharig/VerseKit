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

//! Rust side of the check of the refinery scanner on Windows itself (`tools/parity_raffinerie_windows.py`): the
//! pictures the Python side drew by GDI, enlarged by GDI+, read by the text recognition of Windows and read whole
//! through `read_image`, in the same order. The enlarged parts are compared byte for byte with the ones the Python
//! side wrote.

use std::fs;

use serde_json::{Value, json};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::refinery_scan::digits::{self, Digits, Typed};
use versekit_core::refinery_scan::{self as scan, Area, Index, Job, Mode, Ocr, Word};
use versekit_core::testdir;
use versekit_core::win_ocr::{self, Bitmap, Recognition};

fn read_json(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

/// `win_ocr.read_bmp`: a BMP of 24 or 32 bits as BGRA, top row first (the fourth byte of a 24-bit picture is 0).
fn read_bmp(path: &str) -> Result<Bitmap, String> {
    let data = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let le32 = |at: usize| data.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if data.get(..2) != Some(b"BM") {
        return Err(format!("{path}: not a BMP"));
    }
    let offset = le32(10).ok_or("kurz")? as usize;
    let (width, height) = (le32(18).ok_or("kurz")?, le32(22).ok_or("kurz")?);
    let bits = data.get(28..30).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or("kurz")?;
    if bits != 24 && bits != 32 {
        return Err(format!("{path}: BMP with {bits} bits"));
    }
    let top_down = height < 0;
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let bpp = usize::from(bits / 8);
    let row = (w * bpp + 3) & !3;
    let mut raw = vec![0u8; w * h * 4];
    for y in 0..h {
        let src = if top_down { y } else { h - 1 - y };
        let line = data.get(offset + src * row..offset + src * row + w * bpp).ok_or("BMP zu kurz")?;
        for x in 0..w {
            for c in 0..bpp.min(4) {
                raw[(y * w + x) * 4 + c] = line[x * bpp + c];
            }
        }
    }
    Ok(Bitmap { width: i64::from(width), height: h as i64, raw })
}

fn mode_of(name: &str) -> Mode {
    match name {
        "dark" => Mode::Dark,
        "gray" => Mode::Gray,
        _ => Mode::Plain,
    }
}

fn area_of(value: &Value) -> Area {
    let n = |i: usize| value[i].as_i64().unwrap_or(0);
    (n(0), n(1), n(2), n(3))
}

fn word_json(w: &Word) -> Value {
    json!({"t": w.t, "x": w.x as i64, "y": w.y as i64, "w": w.w as i64, "h": w.h as i64})
}

fn jobs_json(jobs: &[scan::ScannedJob]) -> Value {
    Value::Array(
        jobs.iter()
            .map(|j| {
                json!({
                    "text": j.text, "unsure": j.unsure, "materials": j.materials, "total": j.total,
                    "state": j.state.word(), "remaining": j.remaining,
                    "cells": j.cells.iter().map(|c| json!({
                        "material": c.material, "column": c.column, "read": c.read,
                        "crop": [c.crop.width, c.crop.height, c.crop.gray],
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

pub fn run(folder: &str, gegenprobe: bool) -> Result<(), String> {
    let file = |name: &str| format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
    let input = read_json(&file("python.json"))?;
    let strings = |v: &Value| -> Vec<String> {
        v.as_array().map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
    };
    let (names, pieces) = (strings(&input["rohstoffe"]), strings(&input["stueck"]));
    let index = Index::new(&names, |name| pieces.iter().any(|p| p == name));
    let mut result = json!({});

    // GDI+, byte for byte.
    let mut parts = Vec::new();
    for (n, part) in input["vergroessert"].as_array().into_iter().flatten().enumerate() {
        let bitmap = read_bmp(&file(part["bild"].as_str().unwrap_or_default()))?;
        let area = area_of(&part["kasten"]);
        let (tw, th, mut raw) = win_ocr::scaled(
            &bitmap,
            area,
            part["faktor"].as_f64().unwrap_or(1.0),
            mode_of(part["art"].as_str().unwrap_or("")),
        )?;
        if gegenprobe && n == 0 {
            raw[7] ^= 0x10;
        }
        let theirs = fs::read(file(part["datei"].as_str().unwrap_or_default())).map_err(|e| e.to_string())?;
        let differing = raw.iter().zip(&theirs).filter(|(a, b)| a != b).count() + raw.len().abs_diff(theirs.len());
        let largest = raw.iter().zip(&theirs).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        parts
            .push(json!({"breite": tw, "hoehe": th, "abweichende_bytes": differing, "groesster_unterschied": largest}));
    }
    result["vergroessert"] = Value::Array(parts);

    // The text recognition.
    let mut recognized = Vec::new();
    for entry in input["erkannt"].as_array().into_iter().flatten() {
        let bitmap = read_bmp(&file(entry["bild"].as_str().unwrap_or_default()))?;
        let jobs: Vec<Job> = entry["auftraege"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|j| Job {
                area: area_of(&j[0]),
                scale: j[1].as_f64().unwrap_or(1.0),
                mode: mode_of(j[2].as_str().unwrap_or("")),
            })
            .collect();
        let passes = Recognition::new(&bitmap).read(&jobs).map_err(|e| e.to_string())?;
        recognized.push(json!({
            "bild": entry["bild"], "auftraege": entry["auftraege"],
            "lesungen": passes.iter().map(|p| p.iter().map(word_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
        }));
    }
    if gegenprobe && let Some(first) = recognized.first_mut() {
        first["lesungen"][0] = json!([]);
    }
    result["erkannt"] = Value::Array(recognized);

    // The whole reading, learning from the cells of each picture in the first round as the Python side does.
    let work = testdir::fresh("parity-raffinerie-win");
    let home = work.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
    let mut readings = Vec::new();
    for learned in [false, true] {
        for picture in input["bilder"].as_array().into_iter().flatten() {
            let name = picture["datei"].as_str().unwrap_or_default();
            let bitmap = read_bmp(&file(name))?;
            let digits_now = Digits::load(&paths);
            let mut ocr = Recognition::new(&bitmap);
            let (jobs, kept) =
                scan::read_image((bitmap.width, bitmap.height), Some(&bitmap), &mut ocr, &index, &digits_now, None)
                    .map_err(|e| e.to_string())?;
            readings.push(json!({"bild": name, "gelernt": learned, "kept": kept, "auftraege": jobs_json(&jobs)}));
            let cells: Vec<_> = jobs.iter().flat_map(|j| j.cells.iter().cloned()).collect();
            if !learned && !cells.is_empty() {
                let typed: Vec<Typed> = cells.iter().map(|c| (c.material.clone(), Some(808.0), Some(808.0))).collect();
                digits::learn(&paths, &cells, &typed);
            }
        }
    }
    if gegenprobe
        && let Some(first) = readings.iter_mut().find(|r| r["auftraege"].as_array().is_some_and(|a| !a.is_empty()))
    {
        first["auftraege"][0]["text"] = json!("Gold 1 1 cSCU");
    }
    result["lesen"] = Value::Array(readings);
    let _ = fs::remove_dir_all(&work);
    let out = file("rust.json");
    fs::write(&out, serde_json::to_string(&result).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    println!("geschrieben {out}");
    Ok(())
}
