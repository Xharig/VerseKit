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

//! Rust side of the check for the refinery scanner (`tools/parity_raffinerie.py`): the same made-up words,
//! crops and pictures as the Python side, in the same order, through `refinery_scan` and `refinery_jobs`.
//! Pictures are drawn again from their description (font, numbers, size); the text recognition answers with
//! what the Python side recorded and notes which jobs it was asked for.

use std::fs;

use serde_json::{Value, json};
use versekit_core::jsonio;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::refinery_jobs;
use versekit_core::refinery_scan::digits::{self, Cell, Digits, Gray, Typed};
use versekit_core::refinery_scan::{
    self as scan, Area, Checked, Checks, Index, Int, Job, Keep, Mode, Ocr, Picture, ScanError, ScannedJob, State, Word,
};
use versekit_core::testdir;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

fn word_of(value: &Value) -> Word {
    Word::new(
        value["t"].as_str().unwrap_or_default(),
        num(&value["x"]),
        num(&value["y"]),
        num(&value["w"]),
        num(&value["h"]),
    )
}

fn words_of(value: &Value) -> Vec<Word> {
    value.as_array().map(|list| list.iter().map(word_of).collect()).unwrap_or_default()
}

fn word_json(w: &Word) -> Value {
    json!({"t": w.t, "x": w.x, "y": w.y, "w": w.w, "h": w.h})
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn area_of(value: &Value) -> Area {
    let n = |i: usize| value[i].as_i64().unwrap_or(0);
    (n(0), n(1), n(2), n(3))
}

fn area_json(a: Area) -> Value {
    json!([a.0, a.1, a.2, a.3])
}

fn jobs_json(jobs: &[Job]) -> Value {
    Value::Array(jobs.iter().map(|j| json!([area_json(j.area), j.scale, j.mode.name()])).collect())
}

fn found_json(found: &[(String, Option<Int>, Option<Int>)]) -> Value {
    Value::Array(found.iter().map(|(m, q, a)| json!([m, q.map(|v| v as i64), a.map(|v| v as i64)])).collect())
}

fn pairs_json(pairs: &[(f64, Int)]) -> Value {
    Value::Array(pairs.iter().map(|(x, v)| json!([x, *v as i64])).collect())
}

/// An integer of the scanner; beyond 64 bits as a float (JSON of serde_json holds no more), which the comparison
/// takes to twelve places.
fn int_json(value: Option<Int>) -> Value {
    match value {
        None => Value::Null,
        Some(v) => i64::try_from(v).map_or_else(|_| json!(v as f64), |v| json!(v)),
    }
}

/// A jsonio value as serde_json, through the text.
fn to_serde(value: &jsonio::Value) -> Value {
    serde_json::from_str(&jsonio::dumps(value, None, false)).unwrap_or(Value::Null)
}

fn to_map(value: &Value) -> Result<jsonio::Map, String> {
    match jsonio::loads(&value.to_string()).map_err(|e| e.to_string())? {
        jsonio::Value::Object(map) => Ok(map),
        _ => Err("kein Objekt".into()),
    }
}

/// The picture of the Python side: dark ground, numbers drawn bright, three channels apart.
struct Drawn {
    width: i64,
    height: i64,
    raw: Vec<u8>,
}

fn draw(font: &Value, width: i64, height: i64, numbers: &Value, dot: i64) -> Drawn {
    let (w, h) = (width as usize, height as usize);
    let mut gray = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            gray[y * w + x] = (20 + (x * 31 + y * 17) % 37) as u8;
        }
    }
    for entry in numbers.as_array().into_iter().flatten() {
        let text = entry[0].as_str().unwrap_or_default();
        let (ox, oy) = (entry[1].as_i64().unwrap_or(0), entry[2].as_i64().unwrap_or(0));
        for (i, digit) in text.chars().enumerate() {
            let lines = strings(&font[digit.to_string()]);
            for (r, line) in lines.iter().enumerate() {
                for (c, mark) in line.chars().enumerate() {
                    if mark != '#' {
                        continue;
                    }
                    for dy in 0..dot {
                        for dx in 0..dot {
                            let x = ox + ((i * 6 + c) as i64) * dot + dx;
                            let y = oy + (r as i64) * dot + dy;
                            if 0 <= x && x < width && 0 <= y && y < height {
                                let (x, y) = (x as usize, y as usize);
                                gray[y * w + x] = (200 + (x * 7 + y * 13) % 41).min(255) as u8;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut raw = vec![0u8; w * h * 4];
    for (i, g) in gray.iter().enumerate() {
        raw[i * 4..i * 4 + 4].copy_from_slice(&[g / 2, *g, g.saturating_add(30), 255]);
    }
    Drawn { width, height, raw }
}

impl Picture for Drawn {
    fn size(&self) -> (i64, i64) {
        (self.width, self.height)
    }

    /// `win_ocr.scaled` of the Python side: the nearest pixel.
    fn scaled(&self, area: Area, scale: f64, _mode: Mode) -> Option<(usize, usize, Vec<u8>)> {
        let (x0, y0, w, h) = (area.0 as usize, area.1 as usize, area.2 as usize, area.3 as usize);
        let tw = ((area.2 as f64 * scale) as usize).max(1);
        let th = ((area.3 as f64 * scale) as usize).max(1);
        let mut out = vec![0u8; tw * th * 4];
        for y in 0..th {
            let sy = y0 + (y * h) / th;
            for x in 0..tw {
                let sx = x0 + (x * w) / tw;
                let i = (sy * self.width as usize + sx) * 4;
                out[(y * tw + x) * 4..(y * tw + x) * 4 + 4].copy_from_slice(&self.raw[i..i + 4]);
            }
        }
        Some((tw, th, out))
    }
}

fn picture_of(font: &Value, value: &Value) -> Drawn {
    draw(
        font,
        value["breite"].as_i64().unwrap_or(0),
        value["hoehe"].as_i64().unwrap_or(0),
        &value["zahlen"],
        value["punkt"].as_i64().unwrap_or(1),
    )
}

fn gray_json(g: &Gray) -> Value {
    json!([g.width, g.height, g.gray])
}

fn gray_of(value: &Value) -> Gray {
    Gray {
        width: value[0].as_u64().unwrap_or(0) as usize,
        height: value[1].as_u64().unwrap_or(0) as usize,
        gray: value[2]
            .as_array()
            .map(|l| l.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect())
            .unwrap_or_default(),
    }
}

fn glyphs_json(vectors: &[Vec<f64>]) -> Value {
    json!(vectors)
}

/// The recognition of the Python side, answering in the order recorded and noting what it was asked.
struct Replay<'a> {
    answers: &'a [Value],
    asked: Vec<Value>,
}

impl Ocr for Replay<'_> {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        let call = self.asked.len();
        self.asked.push(jobs_json(jobs));
        let answer = self
            .answers
            .get(call)
            .ok_or_else(|| ScanError::new(scan::Reason::Ocr, "mehr Auftraege als auf der Python-Seite"))?;
        Ok(answer.as_array().map(|passes| passes.iter().map(words_of).collect()).unwrap_or_default())
    }
}

fn pack_jobs(jobs: &[ScannedJob]) -> Value {
    Value::Array(
        jobs.iter()
            .map(|j| {
                json!({
                    "text": j.text, "unsure": j.unsure, "materials": j.materials, "total": j.total,
                    "state": j.state.word(), "remaining": j.remaining,
                    "cells": j.cells.iter().map(|c| json!({
                        "material": c.material, "column": c.column, "read": c.read, "crop": gray_json(&c.crop),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn merged_json(merged: &scan::Merged, index: &Index) -> Value {
    json!({
        "zeilen": found_json(&merged.found),
        "unsicher": merged.unsure,
        "einzeln": merged.details.iter().map(|d| json!({"material": d.material, "y": d.y})).collect::<Vec<_>>(),
        "text": scan::as_text(&merged.found, index),
    })
}

/// The made-up checks of the Python side (`confirm_rule`, `reread_rule`).
fn rule_checks() -> Checks<'static> {
    Checks {
        confirm: Some(Box::new(|_y, _column, value| match value % 3 {
            0 => Checked::Yes,
            1 => Checked::Value(value + 1),
            _ => Checked::No,
        })),
        reread: Some(Box::new(|y, column| {
            let y = y as Int;
            if (y + column as Int) % 4 == 0 { None } else { Some(y * 10 + column as Int) }
        })),
    }
}

fn tables(input: &Value, index: &Index) -> Value {
    let mut out = Vec::new();
    for case in input["tabellen"].as_array().into_iter().flatten() {
        let passes: Vec<Vec<Word>> = case["lesungen"].as_array().into_iter().flatten().map(words_of).collect();
        let rows = scan::rows(&passes[0]);
        let found = scan::material_rows(&rows, index);
        let material_rows: Vec<Value> = found
            .iter()
            .map(|r| {
                json!({"material": r.material, "y": r.y, "height": r.height, "raw": r.raw,
                       "numbers": pairs_json(&r.numbers), "loose_numbers": pairs_json(&r.loose_numbers)})
            })
            .collect();
        let read: Vec<Value> = scan::parse_rows(&rows, index)
            .iter()
            .map(|r| {
                json!({"material": r.material, "quality": int_json(r.quality), "amount": int_json(r.amount),
                       "loose_quality": int_json(r.loose_quality), "loose_amount": int_json(r.loose_amount),
                       "y": r.y, "height": r.height, "raw": r.raw})
            })
            .collect();
        let none = Checks::default();
        let rules = rule_checks();
        out.push(json!({
            "lesungen": case["lesungen"],
            "zeilen": rows.iter().map(|line| line.iter().map(|w| json!([w.t, w.x, w.y, w.w, w.h])).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "rohstoffzeilen": material_rows,
            "spalten": scan::columns(&found),
            "kopf": scan::header_columns(&rows),
            "gelesen": read,
            "reihenfolge": scan::materials_in_order(&passes, index),
            "abgestimmt": merged_json(&scan::merge_passes(&passes, index, None, &none, false), index),
            "geraten": merged_json(&scan::merge_passes(&passes, index, None, &none, true), index),
            "geprueft": merged_json(&scan::merge_passes(&passes, index, None, &rules, false), index),
            "geprueft_geraten": merged_json(&scan::merge_passes(&passes, index, None, &rules, true), index),
            "eine_stimme": merged_json(&scan::merge_passes(&passes, index, Some(1), &none, false), index),
        }));
    }
    Value::Array(out)
}

fn screens(input: &Value) -> Value {
    let mut out = Vec::new();
    for case in input["bildschirme"].as_array().into_iter().flatten() {
        let (width, height) = (case["breite"].as_i64().unwrap_or(0), case["hoehe"].as_i64().unwrap_or(0));
        let tiles: Vec<Vec<Word>> = case["kacheln"].as_array().into_iter().flatten().map(words_of).collect();
        let merged = scan::merge_words(&tiles);
        let found = scan::find_tables(&merged, width, height);
        let columns: Vec<Value> = (0..5120)
            .step_by(400)
            .map(|x0| {
                let area = (x0, 0, 700, height);
                json!([x0, scan::job_state(&merged, area).word(), scan::remaining_seconds(&merged, area)])
            })
            .collect();
        out.push(json!({
            "breite": width, "hoehe": height, "worte": case["worte"], "kacheln": case["kacheln"],
            "zusammen": merged.iter().map(word_json).collect::<Vec<_>>(),
            "gefunden": found.iter().map(|t| json!([area_json(t.area), t.total])).collect::<Vec<_>>(),
            "erste": scan::find_table(&merged, width, height).map(area_json),
            "zustand": found.iter().map(|t| scan::job_state(&merged, t.area).word()).collect::<Vec<_>>(),
            "restzeit": found.iter().map(|t| scan::remaining_seconds(&merged, t.area)).collect::<Vec<_>>(),
            "spalten_zustand": columns,
        }));
    }
    Value::Array(out)
}

fn typed_of(value: &Value) -> Vec<Typed> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| (row[0].as_str().unwrap_or_default().to_string(), row[1].as_f64(), row[2].as_f64()))
        .collect()
}

fn digit_file(paths: &Paths) -> Result<String, String> {
    let file = paths.app_file(digits::DIGIT_FILE);
    Ok(fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?.replace("\r\n", "\n"))
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let names = strings(&input["rohstoffe"]);
    let pieces = strings(&input["stueck"]);
    let index = Index::new(&names, |name| pieces.iter().any(|p| p == name));
    let font = &input["schrift"];
    let work = testdir::fresh("parity-raffinerie");
    let home = work.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, home.as_str())], &home));
    let mut result = json!({});

    result["zahlen"] = Value::Array(
        input["zahlen"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let t = c[0].as_str().unwrap_or_default();
                json!([t, int_json(scan::number(t, false)), int_json(scan::number(t, true))])
            })
            .collect(),
    );
    result["namen"] = Value::Array(
        input["namen"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let n = c[0].as_str().unwrap_or_default();
                json!([n, scan::match_material(n, &index)])
            })
            .collect(),
    );
    result["tabellen"] = tables(&input, &index);
    result["kacheln"] = Value::Array(
        input["kacheln"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let (w, h) = (c[0].as_i64().unwrap_or(0), c[1].as_i64().unwrap_or(0));
                json!([w, h, jobs_json(&scan::search_jobs(w, h))])
            })
            .collect(),
    );
    result["bildschirme"] = screens(&input);

    // The digits: the cells cut from the drawn strip, teaching, the file, assigning.
    let strip = picture_of(font, &input["ziffern_bild"]);
    let mut cells: Vec<Cell> = Vec::new();
    for c in input["ziffern_zellen"].as_array().into_iter().flatten() {
        let crop = scan::cell_crop(&strip, area_of(&c["box"])).ok_or("Zelle ausserhalb des Bildes")?;
        cells.push(Cell {
            material: c["material"].as_str().unwrap_or_default().into(),
            column: c["column"].as_u64().unwrap_or(0) as usize,
            read: c["read"].as_str().unwrap_or_default().into(),
            crop,
        });
    }
    result["zellbilder"] = Value::Array(cells.iter().map(|c| gray_json(&c.crop)).collect());
    let crops: Vec<Gray> = input["zerlegt"].as_array().into_iter().flatten().map(|c| gray_of(&c[0])).collect();
    result["zerlegt"] = Value::Array(
        crops
            .iter()
            .map(|g| {
                json!([
                    gray_json(g),
                    glyphs_json(&digits::glyphs(Some(g), None)),
                    glyphs_json(&digits::glyphs(Some(g), Some(1))),
                    glyphs_json(&digits::glyphs(Some(g), Some(3))),
                    glyphs_json(&digits::glyphs(Some(g), Some(6)))
                ])
            })
            .collect(),
    );
    let typed = typed_of(&input["ziffern_getippt"]);
    let mut steps = Vec::new();
    for k in 0..cells.len() {
        steps.push(digits::learn(&paths, &cells[k..k + 1], &typed));
    }
    steps.push(digits::learn(&paths, &cells, &typed));
    steps.push(digits::learn(&paths, &cells, &[("Gold".to_string(), Some(12.0), Some(1.0))]));
    steps.push(digits::learn(&paths, &[], &typed));
    result["gelernt"] = json!(steps);
    result["ziffern_datei"] = json!(digit_file(&paths)?);
    let learned = Digits::load(&paths);
    result["zuordnung"] = Value::Array(
        crops
            .iter()
            .map(|g| {
                let vectors = digits::glyphs(Some(g), None);
                let reads = ["", "888", "12", "sos", "1234567"];
                let cell: Vec<Value> = reads.iter().map(|r| int_json(digits::read_cell(g, &learned, r))).collect();
                json!({
                    "klassen": vectors.iter().map(|v| digits::classify(v, &learned).map(String::from)).collect::<Vec<_>>(),
                    "alle": vectors.iter().map(|v| digits::classify_any(v, &learned)).collect::<Vec<_>>(),
                    "zelle": cell,
                })
            })
            .collect(),
    );
    let (first, second) = (digits::glyphs(crops.first(), None), digits::glyphs(crops.get(1), None));
    result["abstaende"] = json!(
        second.iter().map(|a| first.iter().map(|b| digits::distance(a, b)).collect::<Vec<_>>()).collect::<Vec<_>>()
    );

    // Whole pictures.
    let mut readings = Vec::new();
    for case in input["lesen"].as_array().into_iter().flatten() {
        let answers = case["antworten"].as_array().cloned().unwrap_or_default();
        let mut ocr = Replay { answers: &answers, asked: Vec::new() };
        let drawn = case["bild"].is_object().then(|| picture_of(font, &case["bild"]));
        let size = match &drawn {
            Some(d) => d.size(),
            None => {
                let n = case["nummer"].as_u64().unwrap_or(0) as usize;
                let screen = &input["bildschirme"][n];
                (screen["breite"].as_i64().unwrap_or(0), screen["hoehe"].as_i64().unwrap_or(0))
            }
        };
        let none = Digits::default();
        let with_digits = case["ziffern"].as_bool().unwrap_or(true);
        let digits_now = if with_digits { &learned } else { &none };
        let mut kinds: Vec<&str> = Vec::new();
        let mut keep = |what: Keep| {
            kinds.push(match what {
                Keep::Picture => "bild",
                Keep::Table(_) => "tabelle",
            });
            true
        };
        let keeping = case["aufbewahren"].as_bool().unwrap_or(false);
        let picture = drawn.as_ref().map(|d| d as &dyn Picture);
        let read =
            scan::read_image(size, picture, &mut ocr, &index, digits_now, if keeping { Some(&mut keep) } else { None });
        let (jobs, kept) = read.map_err(|e| e.to_string())?;
        let mut entry = case.clone();
        entry["gefragt"] = Value::Array(ocr.asked);
        entry["auftraege"] = pack_jobs(&jobs);
        entry["aufgehoben"] = json!(kept);
        entry["arten"] = json!(kinds);
        readings.push(entry);
    }
    result["lesen"] = Value::Array(readings);

    // The store of the jobs.
    let scanned_of = |value: &Value| -> Vec<ScannedJob> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .map(|j| ScannedJob {
                text: String::new(),
                unsure: Vec::new(),
                materials: strings(&j["materials"]),
                total: j["total"].as_str().unwrap_or_default().into(),
                state: match j["state"].as_str() {
                    Some("fertig") => State::Done,
                    Some("laeuft") => State::Running,
                    _ => State::Unknown,
                },
                remaining: j["remaining"].as_i64(),
                cells: Vec::new(),
            })
            .collect()
    };
    let maps = |value: &Value| -> Result<Vec<jsonio::Map>, String> {
        value.as_array().into_iter().flatten().map(to_map).collect()
    };
    let objects =
        |list: &[jsonio::Map]| Value::Array(list.iter().map(|m| to_serde(&jsonio::Value::Object(m.clone()))).collect());
    let mut store = Vec::new();
    for step in input["speicher"].as_array().into_iter().flatten() {
        let now = num(&step["jetzt"]);
        let merged = refinery_jobs::merge(&maps(&step["gemerkt"])?, &scanned_of(&step["gescannt"]), now);
        let (due, after) = refinery_jobs::due(&merged, now);
        let mut entry = step.clone();
        entry["ergebnis"] = objects(&merged);
        entry["faellig"] = objects(&due);
        entry["danach"] = objects(&after);
        store.push(entry);
    }
    result["speicher"] = Value::Array(store);
    let now = num(&input["speicher_datei"]["jetzt"]);
    let file = paths.app_file(refinery_jobs::FILE);
    let _ = fs::remove_file(&file);
    let first = refinery_jobs::remember(
        &paths,
        &scanned_of(&json!([{"materials": ["Gold"], "total": "1", "state": "", "remaining": 60}])),
        now,
    );
    let second = refinery_jobs::remember(
        &paths,
        &scanned_of(&json!([{"materials": ["Iron"], "total": "", "state": "laeuft", "remaining": null}])),
        now,
    );
    let text = fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?.replace("\r\n", "\n");
    result["speicher_datei"] =
        json!({"gemerkt": objects(&first), "unveraendert": objects(&second), "jetzt": now, "datei": text});

    if gegenprobe {
        // One number, one name, one vote, one tile, one table found, one crop, one pattern, one tally, the file
        // of the patterns, one job read and one end of the store changed: each must be reported.
        result["zahlen"][0][1] = json!(481);
        result["namen"][0][1] = json!("Gold");
        result["tabellen"][0]["abgestimmt"]["unsicher"] = json!(["Banane"]);
        result["kacheln"][0][2][0][1] = json!(2.5);
        result["bildschirme"][0]["gefunden"] = json!([]);
        result["zellbilder"][0][2][0] = json!(1);
        result["zerlegt"][0][1][0][0] = json!(0.1234);
        result["gelernt"][0] = json!(99);
        result["ziffern_datei"] = json!(format!("{} ", result["ziffern_datei"].as_str().unwrap_or_default()));
        result["lesen"][0]["auftraege"] = json!([]);
        result["speicher"][1]["ergebnis"] = json!([{"ends": 0}]);
    }
    fs::write(out, serde_json::to_string(&result).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    let _ = fs::remove_dir_all(&work);
    println!("geschrieben {out}");
    Ok(())
}
