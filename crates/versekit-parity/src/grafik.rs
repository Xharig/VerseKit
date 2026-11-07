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

//! Rust side of the check for the graphics settings (`tools/parity_grafik.py`): the same texts and changes
//! through `graphics::{change_text, read_text, levels_text, quality_text, compare, preset_rows}`.

use std::fs;

use serde_json::{Map, Value, json};
use versekit_core::fov;
use versekit_core::graphics::{self, ChangeError};
use versekit_core::machine_info;
use versekit_core::renderer::{self, Failure};
use versekit_core::usercfg;

/// `gegenprobe`: a quality row is put in at the end of the file instead of its place in the alphabet, and the
/// levels of the presets are all the level itself.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let mut done = Vec::with_capacity(cases.len());
    for case in cases {
        let kind = case[0].as_str().ok_or("Art fehlt")?;
        let answer = match kind {
            "lesen" => read(case[1].as_str().ok_or("Text fehlt")?),
            "aendern" => change(case[1].as_str().ok_or("Text fehlt")?, &case[2], gegenprobe)?,
            "vergleich" => compare(&case[1], case[2].as_i64().ok_or("Klasse fehlt")?)?,
            "voreinstellung" => preset(case[1].as_i64().ok_or("Stufe fehlt")?, gegenprobe),
            "cfg_setzen" => cfg_set(case[1].as_str().ok_or("Text fehlt")?, &case[2], gegenprobe)?,
            "cfg_eigene" => json!(usercfg::own_lines(&usercfg::normalized(case[1].as_str().ok_or("Text fehlt")?))),
            "cfg_wert" => {
                cfg_value(case[1].as_str().ok_or("Text fehlt")?, case[2].as_str().ok_or("Name fehlt")?, gegenprobe)
            }
            "cfg_pruefen" => cfg_check(&case[1]),
            "cfg_schreiben" => cfg_write(case[1].as_str().ok_or("Text fehlt")?, &case[2], gegenprobe)?,
            "cfg_status" => cfg_status(case[1].as_bool().unwrap_or(false), case[2].as_str(), gegenprobe)?,
            "fov_fn" => fov_fn(
                case[1].as_str().ok_or("Name fehlt")?,
                &case[2].as_array().ok_or("Werte fehlen")?.iter().map(Value::as_f64).collect::<Vec<_>>(),
                gegenprobe,
            )?,
            "fov_datei" => {
                let setting = fov::game_setting_text(&usercfg::normalized(case[1].as_str().ok_or("Text fehlt")?));
                json!([setting.fov, setting.width, setting.height])
            }
            "maschine" => machine(case[1].as_str().ok_or("Text fehlt")?, gegenprobe),
            "renderer_lesen" => {
                json!(renderer::read_text(&usercfg::normalized(case[1].as_str().ok_or("Text fehlt")?)))
            }
            "renderer_setzen" => {
                renderer_set(case[1].as_str().ok_or("Text fehlt")?, case[2].as_i64().ok_or("Wert fehlt")?, gegenprobe)
            }
            other => return Err(format!("unbekannte Art {other}")),
        };
        let mut row = case.as_array().cloned().unwrap_or_default();
        let last = row.len() - 1;
        row[last] = answer;
        done.push(Value::Array(row));
    }
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 { json!(value as i64) } else { json!(value) }
}

fn pairs(items: Vec<(&str, f64)>) -> Value {
    Value::Object(items.into_iter().map(|(name, value)| (name.to_string(), number(value))).collect::<Map<_, _>>())
}

fn levels(items: Vec<(&str, i64)>) -> Value {
    Value::Object(items.into_iter().map(|(name, value)| (name.to_string(), json!(value))).collect::<Map<_, _>>())
}

fn read(text: &str) -> Value {
    json!({
        "read": pairs(graphics::read_text(text)),
        "levels": levels(graphics::levels_text(text)),
        "quality": levels(graphics::quality_text(text)),
    })
}

fn change(text: &str, changes: &Value, gegenprobe: bool) -> Result<Value, String> {
    let changes: Vec<(String, Option<f64>)> = changes
        .as_array()
        .ok_or("Änderungen fehlen")?
        .iter()
        .map(|pair| (pair[0].as_str().unwrap_or_default().to_string(), pair[1].as_f64()))
        .collect();
    // The counter run takes the order of the alphabet away: a new line goes to the end.
    let result = if gegenprobe {
        graphics::change_text(&text.replace(" <Attr", "<Attr"), &changes)
    } else {
        graphics::change_text(text, &changes)
    };
    Ok(match result {
        Ok(Some(fresh)) => json!(["ok", fresh]),
        Ok(None) => json!(["ok", if gegenprobe { text.replace(" <Attr", "<Attr") } else { text.to_string() }]),
        Err(ChangeError::Level(_)) => json!(["fehler", "Level"]),
        Err(ChangeError::Unknown(_)) => json!(["fehler", "Unknown"]),
        Err(ChangeError::NoClose) => json!(["fehler", "NoClose"]),
    })
}

fn compare(levels_in: &Value, class: i64) -> Result<Value, String> {
    let levels: Vec<(String, i64)> = levels_in
        .as_object()
        .ok_or("Stufen fehlen")?
        .iter()
        .map(|(name, level)| (name.clone(), level.as_i64().unwrap_or(0)))
        .collect();
    let borrowed: Vec<(&str, i64)> = levels.iter().map(|(name, level)| (name.as_str(), *level)).collect();
    let (above, below) = graphics::compare(&borrowed, class);
    let rows = |items: Vec<(&str, i64, i64)>| -> Value {
        Value::Array(items.into_iter().map(|(row, is, should)| json!([row, is, should])).collect())
    };
    Ok(json!([rows(above), rows(below)]))
}

fn round9(value: Option<f64>) -> Value {
    // Like Python's `round(x, 9)`: from the exact decimal value (ties go to the even digit), not `x * 1e9`.
    json!(value.map(|v| format!("{v:.9}").parse::<f64>().unwrap_or(v)))
}

/// The functions of `fov.py` by name; `gegenprobe`: the limits of the rating are exclusive.
fn fov_fn(name: &str, args: &[Option<f64>], gegenprobe: bool) -> Result<Value, String> {
    let a = |n: usize| args.get(n).copied().flatten();
    let both = || a(0).zip(a(1)).ok_or("Zahlen fehlen");
    Ok(match name {
        "field_of_view" => round9(both().ok().and_then(|(w, d)| fov::field_of_view(w, d))),
        "distance_for" => round9(both().ok().and_then(|(w, x)| fov::distance_for(w, x))),
        "horizontal_from_vertical" => round9(both().ok().and_then(|(v, x)| fov::horizontal_from_vertical(v, x))),
        "screen_width_mm" => round9(both().ok().and_then(|(w, m)| fov::screen_width_mm(w, m))),
        "mm_per_pixel" => round9(a(0).and_then(fov::mm_per_pixel)),
        "rating" => {
            let (x, y) = both()?;
            let (grade, deviation) = fov::rating(x, y);
            let word = match grade {
                fov::Grade::Green if gegenprobe && deviation.abs() == fov::GREEN => "gelb",
                fov::Grade::Green => "gruen",
                fov::Grade::Yellow => "gelb",
                fov::Grade::Red => "rot",
            };
            json!([word, round9(Some(deviation))])
        }
        "game_horizontal" | "narrowest_horizontal" => {
            let setting = fov::GameSetting { fov: a(0), width: a(1), height: a(2), file: String::new() };
            round9(if name == "game_horizontal" {
                fov::game_horizontal(&setting)
            } else {
                fov::narrowest_horizontal(&setting)
            })
        }
        other => return Err(format!("unbekannte Funktion {other}")),
    })
}

/// `gegenprobe`: the first rating counts instead of the last.
fn machine(text: &str, gegenprobe: bool) -> Value {
    let mut info = machine_info::parse(text);
    if gegenprobe {
        let first = text.lines().rev().collect::<Vec<_>>().join("\n");
        info.machine_class = machine_info::parse(&first).machine_class;
    }
    json!({
        "cpu": info.cpu,
        "threads": info.threads,
        "ram_mb": info.ram_mb,
        "gpu": info.gpu,
        "driver": info.driver,
        "vulkan": info.vulkan,
        "vram_mb": info.vram_mb,
        "display": info.display.map(|(w, h)| json!([w, h])),
        "dlss": info.dlss,
        "cpu_bench": info.cpu_bench.map(|(a, b)| json!([a, b])),
        "gpu_bench": info.gpu_bench,
        "cpu_index": info.cpu_index,
        "gpu_index": info.gpu_index,
        "machine_class": info.machine_class,
    })
}

/// `gegenprobe`: the text is not made pure ASCII (a sign beyond stays as it is).
fn renderer_set(text: &str, value: i64, gegenprobe: bool) -> Value {
    match renderer::change_text(&usercfg::normalized(text), value) {
        Ok(Some(fresh)) if gegenprobe => json!(["ok", fresh.replace("\\u00e9", "\u{e9}")]),
        Ok(Some(fresh)) => json!(["ok", fresh]),
        // Nothing to write: the file stays as it was read, line ends as they were.
        Ok(None) => json!(["ok", text]),
        Err(Failure::Invalid) => json!(["fehler", "Invalid"]),
        Err(_) => json!(["fehler", "Read"]),
    }
}

/// `gegenprobe`: the changes are applied in the opposite order.
fn cfg_set(text: &str, changes: &Value, gegenprobe: bool) -> Result<Value, String> {
    let mut changes: Vec<(String, Option<String>)> = changes
        .as_array()
        .ok_or("Änderungen fehlen")?
        .iter()
        .map(|pair| (pair[0].as_str().unwrap_or_default().to_string(), pair[1].as_str().map(str::to_string)))
        .collect();
    if gegenprobe {
        changes.reverse();
    }
    Ok(match usercfg::set_text(&usercfg::normalized(text), &changes) {
        Ok(fresh) => json!(["ok", fresh]),
        Err(bad) => json!(["fehler", bad.into_iter().map(|(number, line)| json!([number, line])).collect::<Vec<_>>()]),
    })
}

/// `gegenprobe`: the first line of a command counts instead of the last.
fn cfg_value(text: &str, key: &str, gegenprobe: bool) -> Value {
    let text = usercfg::normalized(text);
    let text = if gegenprobe { text.lines().rev().collect::<Vec<_>>().join("\n") } else { text };
    json!(usercfg::value_in(&text, key))
}

/// `write_own` (the text part): what the file says after the own lines are saved, or the invalid lines.
/// `gegenprobe`: the language lines of the old file stand in front of the own lines instead of behind them.
fn cfg_write(text: &str, lines: &Value, gegenprobe: bool) -> Result<Value, String> {
    let lines: Vec<String> = lines
        .as_array()
        .ok_or("Zeilen fehlen")?
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_string())
        .collect();
    let old = usercfg::normalized(text);
    Ok(match usercfg::own_text(&old, &lines) {
        Ok(fresh) if gegenprobe => {
            let (managed, own): (Vec<&str>, Vec<&str>) =
                fresh.lines().partition(|line| usercfg::MANAGED.contains(&usercfg::key(line)));
            json!(["ok", format!("{}\n", managed.into_iter().chain(own).collect::<Vec<_>>().join("\n"))])
        }
        Ok(fresh) => json!(["ok", fresh]),
        Err(bad) => json!(["fehler", bad.into_iter().map(|(number, line)| json!([number, line])).collect::<Vec<_>>()]),
    })
}

/// `status` of a channel folder with or without `Data.p4k` and with or without a `user.cfg` of this text.
/// `gegenprobe`: the key is looked for without regard to its case.
fn cfg_status(present: bool, text: Option<&str>, gegenprobe: bool) -> Result<Value, String> {
    let folder = versekit_core::testdir::Dir::new("parity-cfgstatus");
    if present {
        fs::write(folder.path().join("Data.p4k"), "").map_err(|e| e.to_string())?;
    }
    if let Some(text) = text {
        fs::write(folder.path().join(usercfg::FILE), text).map_err(|e| e.to_string())?;
    }
    let mut status = usercfg::status(&folder.path().to_string_lossy());
    if gegenprobe
        && status == usercfg::Status::NoLanguage
        && usercfg::value(&folder.path().to_string_lossy(), "g_language").is_some()
    {
        status = usercfg::Status::Ok;
    }
    Ok(json!(match status {
        usercfg::Status::NoInstall => "keine",
        usercfg::Status::NoFile => "ohne_datei",
        usercfg::Status::NoLanguage => "ohne_sprache",
        usercfg::Status::Ok => "ok",
    }))
}

fn cfg_check(lines: &Value) -> Value {
    let lines: Vec<String> = lines
        .as_array()
        .map(|a| a.iter().map(|l| l.as_str().unwrap_or_default().to_string()).collect())
        .unwrap_or_default();
    Value::Array(usercfg::check(&lines).into_iter().map(|(number, line)| json!([number, line])).collect())
}

fn preset(level: i64, gegenprobe: bool) -> Value {
    let rows = graphics::preset_rows(level);
    Value::Object(
        rows.into_iter()
            .map(|(row, value)| (row.to_string(), json!(if gegenprobe { level } else { value })))
            .collect::<Map<_, _>>(),
    )
}
