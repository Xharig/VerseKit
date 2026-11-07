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

//! Rust side of the check for the signature scanner (`tools/parity_signatur.py`): the crops of the test sets
//! read, the steps in between, the search in made-up frames, the distances to the templates, the possible
//! values and teaching, from the same files in the same order as the Python side wrote them.

use std::collections::BTreeMap;
use std::fs;

use serde_json::{Value, json};
use versekit_core::mining_data::MiningData;
use versekit_core::signature_scan::{self as scan, Box4, Found, Gray, Reading};
use versekit_core::signature_watch::{Change, Seen, Tracker, Voter};
use versekit_core::signature_window;
use versekit_core::testdir;

const CANVAS: (usize, usize) = (1920, 1080);

/// The same picture of the whole game window as `make_canvas` of the Python side.
fn make_canvas(raster: &Gray, at: Option<(usize, usize)>) -> Vec<u8> {
    let (width, height) = CANVAS;
    let mut raw = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let g = (x * 31 + y * 17) % 37;
            let i = (y * width + x) * 4;
            raw[i..i + 4].copy_from_slice(&[((g * 5 + 90) % 256) as u8, g as u8, ((g * 3 + 120) % 256) as u8, 255]);
        }
    }
    if let Some((px, py)) = at {
        for ry in 0..raster.h {
            for rx in 0..raster.w {
                let v = raster.data[ry * raster.w + rx];
                let i = ((py + ry) * width + px + rx) * 4;
                raw[i] = v.saturating_add(60);
                raw[i + 1] = v;
                raw[i + 2] = 255;
            }
        }
    }
    raw
}

/// The beats of the watch on a made-up screen, in the order of the schedule the Python side wrote.
fn watch_run(
    input: &Value,
    raster: &Gray,
    known: &scan::ByVariant<scan::Templates>,
    values: &[u64],
) -> Result<Value, String> {
    let positions = [("p1", (700usize, 420usize)), ("p2", (1000, 500))];
    let mut canvases: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for (name, at) in positions {
        canvases.insert(name, make_canvas(raster, Some(at)));
    }
    let blank = make_canvas(raster, None);
    canvases.insert("leer", blank.clone());
    canvases.insert("kein_spiel", blank);
    let (width, height) = (CANVAS.0 as i32, CANVAS.1 as i32);
    let mut tracker = Tracker::new();
    // The Python side keeps the display four seconds.
    let mut voter = Voter::new(4.0);
    let mut steps = Vec::new();
    for beat in input["ablauf"].as_array().ok_or("ablauf fehlt")? {
        let now = beat[0].as_f64().ok_or("Zeit")?;
        let screen = beat[1].as_str().ok_or("Bild")?;
        let raw = &canvases[screen];
        let game = (screen != "kein_spiel").then_some((0, 0, width, height));
        let mut grab = |area: (i32, i32, i32, i32)| {
            let mut out = Vec::with_capacity((area.2 * area.3 * 4) as usize);
            for y in area.1..area.1 + area.3 {
                let start = ((y * width + area.0) * 4) as usize;
                out.extend_from_slice(&raw[start..start + area.2 as usize * 4]);
            }
            Ok(out)
        };
        let mut search = |bytes: &[u8], w: i32, h: i32| scan::search(bytes, w, h, known, values, 12);
        let look =
            if game.is_some() { tracker.look(game, now, &mut grab, &mut search).map_err(|e| e.key())? } else { None };
        let reading = look.as_ref().and_then(|l| l.found.reading.value).map(|v| Seen::of(v, values));
        let change = voter.step(look.is_some(), reading, now);
        steps.push(json!({
            "t": now,
            "spiel": game.is_some(),
            "gesucht": look.is_some(),
            "wert": look.as_ref().and_then(|l| l.found.reading.value),
            "kasten": look.as_ref().and_then(|l| l.box_abs).map(|b| json!([b.0, b.1, b.2, b.3])),
            "voll": tracker.full_searches,
            "verfolgt": tracker.tracked_searches,
            "veroeffentlicht": match change { Change::Keep => json!("behalten"), Change::Show(v) => json!(v), Change::Clear => json!("leer") },
            "gezeigt": voter.shown(),
        }));
    }
    Ok(Value::Array(steps))
}

const FRAME_OFFSET: (usize, usize) = (60, 40);

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn gray_of(value: &Value) -> Result<Gray, String> {
    let rows = value.as_array().ok_or("Bild ist keine Liste")?;
    let h = rows.len();
    let w = rows.first().and_then(Value::as_array).map_or(0, Vec::len);
    let mut data = Vec::with_capacity(w * h);
    for row in rows {
        let row = row.as_array().ok_or("Zeile ist keine Liste")?;
        if row.len() != w {
            return Err("Zeilen verschieden lang".into());
        }
        for v in row {
            data.push(u8::try_from(v.as_u64().ok_or("Wert ist keine Zahl")?).map_err(|_| "Wert ueber 255")?);
        }
    }
    Ok(Gray::new(w, h, data))
}

/// (blue, green, red) of one point of a crop in a frame, by channel layout (`frame_point` of the Python side).
fn frame_point(mode: &str, v: u8, y: usize, height: usize) -> [u8; 3] {
    match mode {
        "rot" => [v / 2, v, v.saturating_add(40)],
        "band" => [v, if (4..height.saturating_sub(4)).contains(&y) { 170 } else { 12 }, v],
        _ => [v.saturating_add(60), v, 255],
    }
}

/// The same frame as `make_frame` of the Python side: noise ground, the crop in the channels of `mode`.
fn make_frame(raster: &Gray, mode: &str) -> (Vec<u8>, i32, i32) {
    let (fw, fh) = (320.max(raster.w + 120), 160.max(raster.h + 80));
    let (ox, oy) = FRAME_OFFSET;
    let mut raw = vec![0u8; fw * fh * 4];
    for y in 0..fh {
        for x in 0..fw {
            let g = (x * 31 + y * 17) % 37;
            let i = (y * fw + x) * 4;
            raw[i..i + 4].copy_from_slice(&[((g * 5 + 90) % 256) as u8, g as u8, ((g * 3 + 120) % 256) as u8, 255]);
        }
    }
    for ry in 0..raster.h {
        for rx in 0..raster.w {
            let v = raster.data[ry * raster.w + rx];
            let i = ((oy + ry) * fw + ox + rx) * 4;
            raw[i..i + 3].copy_from_slice(&frame_point(mode, v, ry, raster.h));
        }
    }
    (raw, fw as i32, fh as i32)
}

fn boxes_json(boxes: &[Box4]) -> Value {
    Value::Array(boxes.iter().map(|b| json!(b)).collect())
}

fn pack(reading: &Reading) -> Value {
    json!({
        "wert": reading.value,
        "grund": reading.reason.map(|r| r.key()),
        "abstand": reading.distance,
        "ziffern": boxes_json(&reading.digits),
        "schwelle": reading.threshold,
    })
}

/// The reading of `read_variants` or `search`: with the grey image it was taken from.
fn pack_variant(found: &Found) -> Value {
    let mut out = pack(&found.reading);
    out["variante"] = found.variant.map_or(Value::Null, |v| json!(v.key()));
    out
}

fn pack_found(found: &Found) -> Value {
    let mut out = pack_variant(found);
    out["kasten"] = found.boxr.map_or(Value::Null, |b| json!([b.0, b.1, b.2, b.3]));
    out["bild"] = found
        .image
        .as_ref()
        .map_or(Value::Null, |g| json!([g.w, g.h, g.data.iter().map(|&v| u64::from(v)).sum::<u64>()]));
    out
}

/// The test sets by file name, each by the key of the image.
fn sets(input: &Value) -> Result<BTreeMap<String, BTreeMap<String, Gray>>, String> {
    let mut out = BTreeMap::new();
    for path in input["bilder_dateien"].as_array().ok_or("bilder_dateien fehlt")? {
        let path = path.as_str().ok_or("Pfad")?;
        let file = read(path)?;
        let mut images = BTreeMap::new();
        for (key, raster) in file["bilder"].as_object().ok_or("bilder fehlt")? {
            images.insert(key.clone(), gray_of(raster)?);
        }
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.insert(name, images);
    }
    Ok(out)
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    // The rows are found as 4.12 finds them; what the two new rules (units, small print) change is measured below.
    scan::legacy_rows(true);
    let mining = MiningData::load(input["bergbau_datei"].as_str().ok_or("bergbau_datei fehlt")?);
    let values = scan::possible_values(&mining);
    let work = testdir::fresh("parity-signatur");
    let own_file = work.join("signatur-ziffern-eigene.json").to_string_lossy().into_owned();
    let samples = work.join(scan::SAMPLE_FOLDER).to_string_lossy().into_owned();
    let known = scan::templates(scan::SHIPPED_TEMPLATES, &own_file);
    // The Python side hands the templates without a grey image to every grey image.
    let known_both = scan::ByVariant::both(known.clone());
    let files = scan::OwnFiles::in_folder(|name| work.join(name).to_string_lossy().into_owned());
    let sets = sets(&input)?;
    let image = |case: &Value| -> Result<&Gray, String> {
        sets.get(case["datei"].as_str().unwrap_or_default())
            .and_then(|s| s.get(case["schluessel"].as_str().unwrap_or_default()))
            .ok_or_else(|| format!("Bild fehlt: {case}"))
    };

    let mut result = json!({ "werte": values });
    let mut described = Vec::new();
    for case in input["beschreibung"].as_array().ok_or("beschreibung fehlt")? {
        let value = case[0].as_u64().ok_or("Wert")?;
        let text = signature_window::describe_with(value, &mining, false);
        described.push(json!([value, text.line(), signature_window::materials(value, &mining)]));
    }
    result["beschreibung"] = Value::Array(described);
    let mut read_cases = Vec::new();
    for case in input["saetze"].as_array().ok_or("saetze fehlt")? {
        let reading = scan::read(image(case)?, &known, &values);
        let mut packed = pack(&reading);
        packed["datei"] = case["datei"].clone();
        packed["schluessel"] = case["schluessel"].clone();
        read_cases.push(packed);
    }
    result["saetze"] = Value::Array(read_cases);

    let mut steps = Vec::new();
    for case in input["zwischen"].as_array().ok_or("zwischen fehlt")? {
        let raster = image(case)?;
        let levels = scan::thresholds(raster);
        let mut rows = Vec::new();
        for &threshold in &levels {
            let boxes = scan::components(raster, threshold);
            let series = |variants: bool| -> Value {
                Value::Array(
                    scan::digit_rows(&boxes, raster, threshold, variants).iter().map(|r| boxes_json(r)).collect(),
                )
            };
            rows.push(json!({
                "schwelle": threshold,
                "kaesten": boxes_json(&boxes),
                "reihen": series(false),
                "varianten": series(true),
            }));
        }
        steps.push(
            json!({ "datei": case["datei"], "schluessel": case["schluessel"], "schwellen": levels, "reihen": rows }),
        );
    }
    result["zwischen"] = Value::Array(steps);

    let mut frames = Vec::new();
    for case in input["rahmen"].as_array().ok_or("rahmen fehlt")? {
        let (raw, width, height) = make_frame(image(case)?, "standard");
        let found = scan::search(&raw, width, height, &known_both, &values, 12);
        let mut packed = pack_found(&found);
        packed["datei"] = case["datei"].clone();
        packed["schluessel"] = case["schluessel"].clone();
        packed["breite"] = json!(width);
        packed["hoehe"] = json!(height);
        packed["kandidaten"] = Value::Array(
            scan::pill_candidates(&raw, width, height).iter().map(|b| json!([b.0, b.1, b.2, b.3])).collect(),
        );
        frames.push(packed);
    }
    result["rahmen"] = Value::Array(frames);

    // The patterns the templates were drawn from, as the Python side read them.
    let mut tables = Vec::new();
    let patterns = input["muster"].as_array().ok_or("muster fehlt")?;
    for (case, pattern) in input["tabellen"].as_array().ok_or("tabellen fehlt")?.iter().zip(patterns) {
        let digit = case["ziffer"].as_str().and_then(|s| s.chars().next()).ok_or("ziffer")?;
        let pattern = pattern_of(pattern)?;
        let table = scan::digit_table(&[pattern], &known);
        let mut distances = serde_json::Map::new();
        for (d, x) in &table[0] {
            distances.insert(d.to_string(), json!(x));
        }
        let (count, position) = scan::holes(&pattern);
        tables.push(json!({
            "ziffer": case["ziffer"],
            "abstaende": distances,
            "loecher": [count, position],
            "gefuellt": scan::fill(&pattern).iter().enumerate().map(|(i, &v)| i as u64 * u64::from(v)).sum::<u64>(),
            "glaubhaft": scan::plausible_template(digit, &pattern),
            "frei": scan::free_reading(&table),
        }));
    }
    result["tabellen"] = Value::Array(tables);

    // Teaching: the images in the order of the Python side, each typed with its own key.
    let mut taught = Vec::new();
    let mut taught_images = Vec::new();
    for case in input["anlernen"].as_array().ok_or("anlernen fehlt")? {
        let key = case["bild"].as_str().unwrap_or_default();
        let raster = sets.values().find_map(|s| s.get(key)).ok_or_else(|| format!("Bild fehlt: {key}"))?;
        let now = scan::templates(scan::SHIPPED_TEMPLATES, &own_file);
        let entry = match scan::learn(raster, key, &now, &own_file, &samples) {
            Ok(t) => json!({
                "bild": key, "ok": true, "wort": null,
                "bilanz": {"erkannt": t.found, "neu": t.new, "bekannt": t.known, "unklar": t.unclear},
            }),
            Err(word) => json!({ "bild": key, "ok": false, "wort": word, "bilanz": {} }),
        };
        taught.push(entry);
        taught_images.push(raster);
    }
    result["anlernen"] = Value::Array(taught);
    let own: Value = read(&own_file)?;
    result["eigene"] = own["ziffern"].clone();
    let after = scan::templates(scan::SHIPPED_TEMPLATES, &own_file);
    result["nach_dem_anlernen"] =
        Value::Array(taught_images.iter().map(|g| pack(&scan::read(g, &after, &values))).collect());

    let watch_image = {
        let case = &input["wache_bild"];
        image(case)?.clone()
    };
    result["wache"] = watch_run(&input, &watch_image, &known_both, &values)?;
    grey_images(&input, &mut result, &sets, &known_both, &files, &samples, &values)?;

    // With the new rules: the value read of every crop and every frame must stay the same.
    scan::legacy_rows(false);
    let mut new_values = Vec::new();
    for case in input["saetze"].as_array().ok_or("saetze fehlt")? {
        new_values.push(json!(scan::read(image(case)?, &known, &values).value));
    }
    let mut new_frames = Vec::new();
    for case in input["rahmen"].as_array().ok_or("rahmen fehlt")? {
        let (raw, width, height) = make_frame(image(case)?, "standard");
        new_frames.push(json!(scan::search(&raw, width, height, &known_both, &values, 12).reading.value));
    }
    result["neue_regeln"] = json!({ "saetze": new_values, "rahmen": new_frames });
    scan::legacy_rows(true);

    if gegenprobe {
        // One value, one distance, one box, one threshold, one candidate, one table entry, one tally and one
        // beat of the watch changed: all eight must be reported (and the five of the grey images below).
        result["wache"][3]["verfolgt"] = json!(99);
        if let Some(case) = result["saetze"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["wert"].is_u64())) {
            case["wert"] = json!(case["wert"].as_u64().unwrap_or(0) + 10);
        }
        result["saetze"][1]["abstand"] = json!(result["saetze"][1]["abstand"].as_f64().unwrap_or(0.0) + 0.1);
        if let Some(case) =
            result["saetze"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["ziffern"][0].is_array()))
        {
            case["ziffern"][0][0] = json!(case["ziffern"][0][0].as_i64().unwrap_or(0) + 1);
        }
        result["zwischen"][0]["schwellen"][0] = json!(1);
        if let Some(case) =
            result["rahmen"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["kandidaten"][0].is_array()))
        {
            case["kandidaten"][0][0] = json!(case["kandidaten"][0][0].as_i64().unwrap_or(0) + 1);
        }
        result["tabellen"][0]["loecher"][0] = json!(7);
        result["anlernen"][0]["bilanz"]["neu"] = json!(99);
        // The grey images: one chosen grey image turned round, the validity of the shared file, one frame with
        // other channels, one tally of teaching from both and the count of images kept.
        if let Some(case) =
            result["varianten"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["variante"] == json!("hell")))
        {
            case["variante"] = json!("gruen");
        }
        result["vorlagen"][0]["gilt_fuer"] = json!(["gruen", "hell"]);
        if let Some(case) =
            result["rahmen2"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["kasten"][0].is_i64()))
        {
            case["kasten"][0] = json!(case["kasten"][0].as_i64().unwrap_or(0) + 1);
        }
        if let Some(case) =
            result["anlernen2"].as_array_mut().and_then(|c| c.iter_mut().find(|c| c["ok"] == json!(true)))
        {
            case["bilanz"]["neu"] = json!(99);
        }
        result["proben"][1] = json!(0);
    }
    fs::write(out, serde_json::to_vec(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The pair (green, bright) of a case (`variant_pair` of the Python side).
fn variant_pair(name: &str, raster: &Gray, other: &Gray) -> (Gray, Gray) {
    let solid = |value: u8| Gray::new(raster.w, raster.h, vec![value; raster.w * raster.h]);
    match name {
        "gleich" => (raster.clone(), raster.clone()),
        "nur_hell" => (solid(128), raster.clone()),
        "gruen_vorn" => (raster.clone(), other.clone()),
        "gruen_zerstoert" => {
            let cut = (raster.w as f64 * 0.4) as usize;
            let data = raster.data.iter().enumerate().map(|(i, &v)| if i % raster.w < cut { 0 } else { v }).collect();
            (Gray::new(raster.w, raster.h, data), raster.clone())
        }
        _ => (solid(0), solid(0)),
    }
}

fn brighter(raster: &Gray) -> Gray {
    Gray::new(raster.w, raster.h, raster.data.iter().map(|&v| v.saturating_add(30)).collect())
}

/// The templates as `digest` of the Python side: per digit the count and the points weighted by place.
fn digest_json(templates: &scan::Templates) -> Value {
    let mut out = serde_json::Map::new();
    for (digit, count, weighted) in templates.digest() {
        out.insert(digit.to_string(), json!([count, weighted]));
    }
    Value::Object(out)
}

fn set_written(path: &str, seconds: u64) -> Result<(), String> {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    fs::File::options().write(true).open(path).and_then(|f| f.set_modified(when)).map_err(|e| format!("{path}: {e}"))
}

/// The checks of the two grey images, in the order of the Python side: which one reads, the frames with other
/// channels, the templates of each with the shared file written before and after the cut-off, teaching from both.
#[allow(clippy::too_many_arguments, reason = "the parts of one check")]
fn grey_images(
    input: &Value,
    result: &mut Value,
    sets: &BTreeMap<String, BTreeMap<String, Gray>>,
    known_both: &scan::ByVariant<scan::Templates>,
    files: &scan::OwnFiles,
    samples: &str,
    values: &[u64],
) -> Result<(), String> {
    let small = sets.get("signatur-windows-klein.json").ok_or("signatur-windows-klein.json fehlt")?;
    let raster_of = |key: &str| small.get(key).ok_or_else(|| format!("Bild fehlt: {key}"));

    let mut pairs = Vec::new();
    for case in input["varianten_faelle"].as_array().ok_or("varianten_faelle fehlt")? {
        let (key, name, other) = (case[0].as_str().ok_or("Schluessel")?, case[1].as_str().ok_or("Fall")?, &case[2]);
        let (green, bright) = variant_pair(name, raster_of(key)?, raster_of(other.as_str().ok_or("andere")?)?);
        let images = scan::Images { green: Some(green), bright: Some(bright) };
        let mut packed = pack_variant(&scan::read_variants(&images, known_both, values));
        packed["schluessel"] = json!(key);
        packed["fall"] = json!(name);
        pairs.push(packed);
    }
    result["varianten"] = Value::Array(pairs);

    let mut frames = Vec::new();
    for case in input["rahmen2_faelle"].as_array().ok_or("rahmen2_faelle fehlt")? {
        let (mode, key) = (case[0].as_str().ok_or("Modus")?, case[1].as_str().ok_or("Schluessel")?);
        let (raw, width, height) = make_frame(raster_of(key)?, mode);
        let mut packed = pack_found(&scan::search(&raw, width, height, known_both, values, 12));
        packed["modus"] = json!(mode);
        packed["schluessel"] = json!(key);
        frames.push(packed);
    }
    result["rahmen2"] = Value::Array(frames);

    let mut saved = Vec::new();
    for (name, written) in [("alt", 1_791_000_000u64), ("neu", 1_793_000_000)] {
        set_written(&files.shared, written)?;
        let holds = scan::shared_own_variants(&files.shared);
        let mut valid: Vec<&str> =
            scan::VARIANTS.into_iter().filter(|v| *holds.get(*v)).map(scan::Variant::key).collect();
        valid.sort_unstable();
        let mut entry = json!({
            "zeit": name,
            "gilt_fuer": valid,
            "ohne": digest_json(&scan::templates(scan::SHIPPED_TEMPLATES, &files.shared)),
        });
        for variant in scan::VARIANTS {
            entry[variant.key()] = digest_json(&scan::templates_for(scan::SHIPPED_TEMPLATES, files, variant));
        }
        saved.push(entry);
    }
    result["vorlagen"] = Value::Array(saved);

    let before = scan::samples(samples).len();
    let mut taught = Vec::new();
    for key in input["anlernen2_faelle"].as_array().ok_or("anlernen2_faelle fehlt")? {
        let key = key.as_str().ok_or("Schluessel")?;
        let raster = raster_of(key)?;
        let images = scan::Images { green: Some(raster.clone()), bright: Some(brighter(raster)) };
        let now = scan::templates_by_variant(scan::SHIPPED_TEMPLATES, files);
        let typed = key.split('#').next().unwrap_or_default();
        taught.push(match scan::learn_variants(&images, typed, Some(scan::Variant::Bright), &now, files, samples) {
            Ok(t) => json!({
                "bild": key, "ok": true, "wort": null,
                "bilanz": {"erkannt": t.found, "neu": t.new, "bekannt": t.known, "unklar": t.unclear},
            }),
            Err(word) => json!({ "bild": key, "ok": false, "wort": word, "bilanz": {} }),
        });
    }
    result["anlernen2"] = Value::Array(taught);
    for variant in scan::VARIANTS {
        let own: Value = read(files.of(variant))?;
        result[format!("eigene_{}", variant.key())] = own["ziffern"].clone();
    }
    result["proben"] = json!([before, scan::samples(samples).len()]);
    Ok(())
}

fn pattern_of(value: &Value) -> Result<scan::Pattern, String> {
    let list = value.as_array().ok_or("Muster")?;
    let mut pattern = [0u8; scan::NORM_W * scan::NORM_H];
    if list.len() != pattern.len() {
        return Err("Muster hat falsche Laenge".into());
    }
    for (slot, v) in pattern.iter_mut().zip(list) {
        *slot = u8::from(v.as_i64().ok_or("Musterwert")? != 0);
    }
    Ok(pattern)
}
