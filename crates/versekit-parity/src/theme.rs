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

//! Rust side of the parity check for the color schemes
//! (`tools/parity_theme.py`): writes every scheme under the Python key names,
//! with the slimmed-down roles resolved.

use std::fs;

use versekit_core::font_size;
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::Host;
use versekit_core::theme::{self, Color, IconTint, Scheme};

fn hex(c: Color) -> Value {
    Value::str(c.to_hex())
}

fn scheme_json(s: &Scheme) -> Value {
    let colors = [
        ("bg", s.bg),
        ("surface", s.surface),
        ("bar", s.bar),
        ("fg", s.fg),
        ("sub", s.sub),
        ("accent", s.accent),
        ("line", s.line),
        ("field", s.field),
        ("gold", s.gold),
        ("red", s.red),
        ("red_pale", s.red_pale),
        ("yellow", s.yellow),
        ("accent_dark", s.accent_dark),
        ("hover", s.hover),
        ("track", s.track),
        ("track_on", s.track_on),
        ("heat_low", s.heat_low()),
        ("selected", s.selected),
        ("danger_fill", s.danger_fill),
        ("scroll_groove", s.scroll_groove),
        ("scroll_grip", s.scroll_grip),
        ("scroll_grip_light", s.scroll_grip_light),
        ("control", s.control()),
        ("control_border", s.control_border()),
        ("hover_fg", s.hover_fg()),
        ("icon_color", s.icon(IconTint::Highlight)),
    ];
    let mut map: Map = vec![("id".into(), Value::str(s.id)), ("label".into(), Value::str(s.label))];
    map.extend(colors.into_iter().map(|(k, c)| (k.to_string(), hex(c))));
    for (k, flag) in [
        ("square", s.square),
        ("upper_headings", s.upper_headings),
        ("accent_headings", s.accent_headings),
        ("filled_buttons", s.filled_buttons),
    ] {
        map.push((k.into(), Value::Bool(flag)));
    }
    Value::Object(map)
}

fn cases<'a>(input: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match input.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt oder ist keine Liste")),
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let mut schemes: Vec<Value> = theme::schemes().iter().map(scheme_json).collect();
    if gegenprobe && let Some(Value::Object(map)) = schemes.get_mut(1) {
        jsonio::map_set(map, "control_border", Value::str("#404145"));
    }

    let mut choices = Vec::new();
    for case in cases(&input, "choices")? {
        let value = case.get("value").cloned().unwrap_or(Value::Null);
        let settings: Map = vec![(theme::SETTING.to_string(), value)];
        choices.push(Value::str(theme::choice_of(&settings).id));
    }

    let mut headings = Vec::new();
    for case in cases(&input, "headings")? {
        let id = case.get("scheme").and_then(Value::as_str).ok_or("scheme fehlt")?;
        let scheme = theme::by_id(id).ok_or_else(|| format!("unbekanntes Schema {id}"))?;
        let text = case.get("text").and_then(Value::as_str).ok_or("text fehlt")?;
        headings.push(Value::str(scheme.heading(text)));
    }

    let color_of = |case: &Value, name: &str| -> Result<Color, String> {
        let text = case.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))?;
        Color::parse(text).ok_or_else(|| format!("keine Farbe: {text}"))
    };
    let number_of = |case: &Value, name: &str| -> Result<f64, String> {
        match case.get(name) {
            Some(Value::Int(n)) => n.parse().map_err(|_| format!("{name}: {n}")),
            Some(Value::Str(s)) => s.parse().map_err(|_| format!("{name}: {s}")),
            Some(Value::Float(x)) => Ok(*x),
            _ => Err(format!("{name} fehlt")),
        }
    };

    let mut mixes = Vec::new();
    for case in cases(&input, "mixes")? {
        let mut share = number_of(case, "share")?;
        if gegenprobe {
            share = (share * 1000.0).floor() / 1000.0;
        }
        mixes.push(hex(color_of(case, "a")?.mix(color_of(case, "b")?, share)));
    }

    let mut heats = Vec::new();
    for case in cases(&input, "heats")? {
        let id = case.get("scheme").and_then(Value::as_str).ok_or("scheme fehlt")?;
        let scheme = theme::by_id(id).ok_or_else(|| format!("unbekanntes Schema {id}"))?;
        heats.push(hex(scheme.heat_cell(number_of(case, "amount")?, number_of(case, "highest")?)));
    }

    let host = Host::current();
    let mut sizes = Vec::new();
    for case in cases(&input, "sizes")? {
        let value = || number_of(case, "value").map(|v| v as i64);
        let result = match case.get("art").and_then(Value::as_str) {
            Some("points") => {
                let settings = case.get("settings").and_then(Value::as_object).cloned().unwrap_or_default();
                let mut points = font_size::font_points(&settings, &host);
                if gegenprobe && points == 3 {
                    points = 2;
                }
                Value::int(points)
            }
            Some("percent") => Value::int(font_size::font_percent(value()?)),
            Some("level") => Value::str(font_size::level_for_points(value()?)),
            Some("auto") => Value::int(font_size::auto_points(value()?)),
            other => return Err(format!("unbekannte Art {other:?}")),
        };
        sizes.push(result);
    }

    let result: Map = vec![
        ("schemes".into(), Value::Array(schemes)),
        ("krt_orange".into(), hex(theme::KRT_ORANGE)),
        ("default".into(), Value::str(theme::DEFAULT)),
        ("choices".into(), Value::Array(choices)),
        ("headings".into(), Value::Array(headings)),
        ("mixes".into(), Value::Array(mixes)),
        ("heats".into(), Value::Array(heats)),
        ("sizes".into(), Value::Array(sizes)),
    ];
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
