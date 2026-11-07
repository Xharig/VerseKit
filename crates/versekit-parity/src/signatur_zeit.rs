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

//! Time of one beat of the signature scanner, reading one grey image (before) against reading both (now).
//!
//! `versekit-parity signatur-zeit [runs]` searches a made-up picture of 5120 by 1440 points: the real crop of
//! the small type in the middle of the game window, once around the pill (the usual beat), once over the whole
//! search area with and without the pill, and once over a lot of rubble that reads as nothing. Nothing is
//! compared with Python here; the numbers go to the console as a table.

use std::time::Instant;

use serde_json::Value;
use versekit_core::signature_scan::{self as scan, ByVariant, Gray, Templates, Variant};

const REAL: &str = include_str!("../../../tools/pruefdaten/signatur-windows-klein.json");
const KEY: &str = "2,000";
const SIZE: (usize, usize) = (5120, 1440);
const PILL_AT: (usize, usize) = (2400, 600);

/// Left, top, width, height.
type Area = (usize, usize, usize, usize);

fn crop() -> Result<(Gray, Vec<u64>), String> {
    let file: Value = serde_json::from_str(REAL).map_err(|e| e.to_string())?;
    let images = file["bilder"].as_object().ok_or("bilder fehlt")?;
    let rows = images.get(KEY).and_then(Value::as_array).ok_or("Bild fehlt")?;
    let mut data = Vec::new();
    for row in rows {
        for v in row.as_array().ok_or("Zeile")? {
            data.push(u8::try_from(v.as_u64().ok_or("Wert")?).map_err(|e| e.to_string())?);
        }
    }
    let numbers = images
        .keys()
        .filter_map(|k| k.split('#').next()?.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok())
        .collect();
    Ok((Gray::new(rows[0].as_array().map_or(0, Vec::len), rows.len(), data), numbers))
}

/// Noise ground as BGRA bytes.
fn ground() -> Vec<u8> {
    let (width, height) = SIZE;
    let mut raw = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let g = (x * 31 + y * 17) % 37;
            let i = (y * width + x) * 4;
            raw[i..i + 4].copy_from_slice(&[((g * 5 + 90) % 256) as u8, g as u8, ((g * 3 + 120) % 256) as u8, 255]);
        }
    }
    raw
}

fn paste(raw: &mut [u8], pill: &Gray) {
    for y in 0..pill.h {
        for x in 0..pill.w {
            let v = pill.data[y * pill.w + x];
            let i = ((PILL_AT.1 + y) * SIZE.0 + PILL_AT.0 + x) * 4;
            raw[i..i + 3].copy_from_slice(&[v.saturating_add(60), v, 255]);
        }
    }
}

/// Short bright strokes in the size of a number in many places: nests that are candidates and read as nothing.
fn rubble(raw: &mut [u8], area: Area, blobs: usize) {
    let mut state = 12_345u64;
    let mut next = |limit: usize| {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % limit
    };
    for _ in 0..blobs {
        let (x0, y0) = (area.0 + next(area.2 - 120), area.1 + next(area.3 - 30));
        for stroke in 0..4 {
            for row in 0..6 {
                for x in 0..10 {
                    let (x, y) = (x0 + stroke * 14 + x, y0 + row * 3 + stroke % 2);
                    let i = (y * SIZE.0 + x) * 4;
                    raw[i..i + 4].copy_from_slice(&[200, 220, 210, 255]);
                }
            }
        }
    }
}

/// The crop of a part of the picture.
fn cut(raw: &[u8], area: Area) -> Vec<u8> {
    let mut out = Vec::with_capacity(area.2 * area.3 * 4);
    for y in area.1..area.1 + area.3 {
        let start = (y * SIZE.0 + area.0) * 4;
        out.extend_from_slice(&raw[start..start + area.2 * 4]);
    }
    out
}

/// The search as it was with the green grey image alone.
fn green_only(raw: &[u8], width: i32, height: i32, known: &Templates, values: &[u64]) -> Option<u64> {
    for boxr in scan::pill_candidates(raw, width, height).into_iter().take(12) {
        let reading = scan::read(&scan::gray_crop(raw, width, boxr, Variant::Green), known, values);
        if reading.value.is_some() {
            return reading.value;
        }
    }
    None
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn time(runs: usize, mut f: impl FnMut()) -> f64 {
    f();
    median(
        (0..runs)
            .map(|_| {
                let started = Instant::now();
                f();
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect(),
    )
}

pub fn run(runs: usize) -> Result<(), String> {
    let (pill, mut values) = crop()?;
    values.sort_unstable();
    values.dedup();
    let mut known = Templates::default();
    known.add_text(scan::SHIPPED_TEMPLATES);
    let both = ByVariant::both(known.clone());

    let mut with_pill = ground();
    paste(&mut with_pill, &pill);
    let without = ground();
    let mut rubbish = ground();
    let (fx, fy, fw, fh) = scan::SEARCH_AREA;
    let full = (
        (SIZE.0 as f64 * fx) as usize,
        (SIZE.1 as f64 * fy) as usize,
        (SIZE.0 as f64 * fw) as usize,
        (SIZE.1 as f64 * fh) as usize,
    );
    rubble(&mut rubbish, full, 400);
    let window = (PILL_AT.0 - 240, PILL_AT.1 - 120, pill.w + 480, pill.h + 240);

    println!("| case | before (ms) | now (ms) | read |");
    println!("|---|---|---|---|");
    let cases: [(&str, &[u8], Area); 4] = [
        ("window around the pill (usual beat)", &with_pill, window),
        ("whole area, pill there", &with_pill, full),
        ("whole area, no pill", &without, full),
        ("whole area, rubble, green reads nothing", &rubbish, full),
    ];
    for (name, raw, area) in cases {
        let part = cut(raw, area);
        let (w, h) = (area.2 as i32, area.3 as i32);
        let before = time(runs, || {
            std::hint::black_box(green_only(&part, w, h, &known, &values));
        });
        let now = time(runs, || {
            std::hint::black_box(scan::search(&part, w, h, &both, &values, 12));
        });
        let read = scan::search(&part, w, h, &both, &values, 12);
        println!("| {name} | {before:.1} | {now:.1} | {:?} {:?} |", read.reading.value, read.variant.map(Variant::key));
    }

    // Loading the templates (every 60 seconds and after teaching): once before, once for each grey image now.
    let folder = versekit_core::testdir::fresh("signatur-zeit");
    let files = scan::OwnFiles::in_folder(|name| folder.join(name).to_string_lossy().into_owned());
    let load_before = time(runs, || {
        std::hint::black_box(scan::templates(scan::SHIPPED_TEMPLATES, &files.shared));
    });
    let load_now = time(runs, || {
        std::hint::black_box(scan::templates_by_variant(scan::SHIPPED_TEMPLATES, &files));
    });
    println!("| loading the templates | {load_before:.1} | {load_now:.1} | |");
    Ok(())
}
