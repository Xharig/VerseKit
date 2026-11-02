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

//! Rust side of the check of the axes and curves against `scbp/curves.py` (`tools/parity_achsen.py`): the reading
//! of the file, the answer of the curve and the numbers the way Python writes them.
//!
//! `gegenprobe`: the lists come back in reverse order, the answers with the other sign, the texts of the numbers
//! with their last character cut off. The comparison must then find the difference.

use std::collections::HashSet;
use std::fs;

use roxmltree::{Document, Node};
use serde_json::{Value, json};
use versekit_core::axes::{self, DeviceBlock, GameBlock, GameProp, Prop};
use versekit_core::axes_write;
use versekit_core::device_sets;
use versekit_core::joysticks_write::{self, Failure};
use versekit_core::jsonio;
use versekit_core::pyfloat;

/// The text as a tree without layout (tags, attributes sorted, text without white space, children in order); with
/// `gegenprobe` the children in the other order.
fn tree_of(text: &str, gegenprobe: bool) -> Value {
    fn node(element: Node, gegenprobe: bool) -> Value {
        let mut attributes: Vec<Value> = element.attributes().map(|a| json!([a.name(), a.value()])).collect();
        attributes.sort_by_key(Value::to_string);
        let mut children: Vec<Value> =
            element.children().filter(Node::is_element).map(|c| node(c, gegenprobe)).collect();
        if gegenprobe {
            children.reverse();
        }
        // Like `element.text` of the tree of 4.12: the text before the first child, without white space.
        let content = element
            .first_child()
            .filter(Node::is_text)
            .and_then(|t| t.text())
            .map_or(String::new(), |t| t.trim().to_string());
        json!([element.tag_name().name(), attributes, content, children])
    }
    match Document::parse(text) {
        Ok(document) => node(document.root_element(), gegenprobe),
        Err(_) => Value::Null,
    }
}

/// A number for the file; what JSON cannot carry (not a number, infinite) is `null`.
fn number(value: Option<f64>) -> Value {
    match value {
        Some(v) if v.is_finite() => json!(v),
        _ => Value::Null,
    }
}

fn block_view(block: &DeviceBlock) -> Value {
    let axes: Vec<Value> = block
        .axes
        .iter()
        .map(|(axis, values)| json!([axis, number(values.deadzone), number(values.saturation)]))
        .collect();
    json!([block.name, block.ident, block.active, axes, block.multiple, block.overtaken, block.orphaned])
}

fn game_view(block: &GameBlock) -> Value {
    let axes: Vec<Value> = block
        .axes
        .iter()
        .map(|(axis, v)| {
            let curve: Vec<Value> = v.curve.iter().map(|(a, b)| json!([number(*a), number(*b)])).collect();
            json!([axis, number(v.exponent), number(v.invert), curve, v.has_curve_block])
        })
        .collect();
    json!([block.kind, block.number, block.name, block.ident, block.active, axes])
}

fn optional(value: &Value) -> Option<f64> {
    value.as_f64()
}

fn curve_of(value: &Value) -> Vec<(Option<f64>, Option<f64>)> {
    value
        .as_array()
        .map(|points| points.iter().map(|p| (optional(&p[0]), optional(&p[1]))).collect())
        .unwrap_or_default()
}

fn reversed(mut list: Vec<Value>, gegenprobe: bool) -> Value {
    if gegenprobe {
        list.reverse();
    }
    Value::Array(list)
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let mut done = Vec::with_capacity(cases.len());
    for case in cases {
        let kind = case[0].as_str().ok_or("Art fehlt")?;
        let answer = match kind {
            "lesen" => {
                let text = case[1].as_str().unwrap_or_default();
                let alive: HashSet<String> = case[2]
                    .as_array()
                    .map(|l| l.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let numbers: Vec<i64> = case[3]
                    .as_array()
                    .map(|l| l.iter().filter_map(|v| v.as_str()?.parse().ok()).collect())
                    .unwrap_or_default();
                let blocks = axes::device_axes_from_text(text, &alive);
                let game = axes::game_axes_from_text(text, &alive);
                let summary = axes::summary_from_text(text, &alive);
                let mut functions = Vec::new();
                for nr in &numbers {
                    for axis in &axes::AXES[..4] {
                        let found = axes::functions_on_axis_from_text(text, *nr, axis, &game);
                        let list: Vec<Value> = found
                            .iter()
                            .map(|f| json!([f.axis, f.action, number(f.exponent), number(f.invert)]))
                            .collect();
                        functions.push(json!([nr.to_string(), axis, list]));
                    }
                }
                json!({
                    "blocks": reversed(blocks.iter().map(block_view).collect(), gegenprobe),
                    "game": reversed(game.iter().map(game_view).collect(), gegenprobe),
                    "summary": {
                        "blocks": reversed(summary.blocks.iter().map(|b| json!([b.name, b.ident, b.active])).collect(), gegenprobe),
                        "orphans": reversed(summary.orphans.iter().map(|b| json!([b.name, b.ident])).collect(), gegenprobe),
                        "adoptable": reversed(
                            summary
                                .adoptable
                                .iter()
                                .map(|z| {
                                    let values: Vec<Value> = z
                                        .values
                                        .iter()
                                        .map(|(a, p, v1, v2)| json!([a, p.name(), number(Some(*v1)), number(*v2)]))
                                        .collect();
                                    json!([z.name, z.old.ident, z.new.ident, values])
                                })
                                .collect(),
                            gegenprobe,
                        ),
                        "conflicts": reversed(summary.conflicts.iter().map(|(n, a)| json!([n, a])).collect(), gegenprobe),
                    },
                    "functions": functions,
                })
            }
            "rechnen" => {
                let args = &case[1];
                let curve = curve_of(&args[4]);
                let (dz, sat, exp) = (optional(&args[1]), optional(&args[2]), optional(&args[3]));
                let steps = usize::try_from(args[5].as_i64().unwrap_or(2).max(0)).unwrap_or(2);
                let x = args[0].as_f64().unwrap_or(0.0);
                let mut answer = axes::answer(x, dz, sat, exp, &curve);
                if gegenprobe {
                    answer = -answer - 0.5;
                }
                let line = axes::progression(dz, sat, exp, &curve, steps, args[6].as_bool().unwrap_or(false));
                let line: Vec<Value> = line.iter().map(|(a, b)| json!([a, b])).collect();
                json!([answer, line])
            }
            "voreinstellung" => {
                // The presets of the page: the values of the table of v5 under the key, and the curve they draw.
                let key = case[1].as_str().unwrap_or_default();
                match axes::PRESETS.iter().find(|preset| preset.key == key) {
                    Some(preset) => {
                        let (dz, sat, exp) = (Some(preset.deadzone), Some(preset.saturation), Some(preset.exponent));
                        let answers: Vec<Value> = (-26..=26)
                            .map(|step| {
                                let mut answer = axes::answer(f64::from(step) / 20.0, dz, sat, exp, &[]);
                                if gegenprobe {
                                    answer = -answer - 0.5;
                                }
                                json!(answer)
                            })
                            .collect();
                        let lines: Vec<Value> = [false, true]
                            .iter()
                            .map(|whole| {
                                let line = axes::progression(dz, sat, exp, &[], 160, *whole);
                                Value::Array(line.iter().map(|(a, b)| json!([a, b])).collect())
                            })
                            .collect();
                        json!([[preset.deadzone, preset.saturation, preset.exponent], answers, lines])
                    }
                    None => json!("fehlt"),
                }
            }
            "setzen_geraet" | "setzen_spiel" => {
                let text = case[1].as_str().unwrap_or_default();
                let args = &case[2];
                let value = args[3].as_f64();
                let result = if kind == "setzen_geraet" {
                    // Every id of the files of this check is alive (the other cases are not covered: 4.12 writes
                    // into the last block of an id, v5 into the one the page shows and only for a live id).
                    let alive: HashSet<String> = ["03F33344", "43F43344", "83F33344", "01F83344", "6F1D2B61"]
                        .iter()
                        .map(|head| format!("{head}-0000-0000-0000-504944564944"))
                        .chain(["6F1D2B61-D5A0-11CF-BFC7-444553540000".to_string()])
                        .collect();
                    let prop = Prop::from_name(args[2].as_str().unwrap_or_default());
                    match prop {
                        Some(prop) => axes_write::set_device_text(
                            text,
                            &alive,
                            args[0].as_str().unwrap_or_default(),
                            args[1].as_str().unwrap_or_default(),
                            prop,
                            value,
                        ),
                        None => Err(Failure::Property),
                    }
                } else {
                    let prop = GameProp::from_name(args[2].as_str().unwrap_or_default());
                    let number = args[0].as_str().and_then(|n| n.parse::<i64>().ok()).unwrap_or(-1);
                    match prop {
                        Some(prop) => {
                            axes_write::set_game_text(text, number, args[1].as_str().unwrap_or_default(), prop, value)
                        }
                        None => Err(Failure::Property),
                    }
                };
                match result {
                    Ok(new) => json!(["ok", "", tree_of(&new, gegenprobe)]),
                    Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), tree_of(text, gegenprobe)]),
                }
            }
            "tauschen" => {
                let text = case[1].as_str().unwrap_or_default();
                let (a, b) = (case[2][0].as_str().unwrap_or_default(), case[2][1].as_str().unwrap_or_default());
                match joysticks_write::swap_bindings_text(text, a, b) {
                    Ok((new, _)) => json!(["ok", "", tree_of(&new, gegenprobe)]),
                    Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), tree_of(text, gegenprobe)]),
                }
            }
            "angleichen" => {
                let text = case[1].as_str().unwrap_or_default();
                let (a, b) = (case[2][0].as_str().unwrap_or_default(), case[2][1].as_str().unwrap_or_default());
                let alive: HashSet<String> = ["03F33344", "43F43344", "83F33344", "01F83344", "6F1D2B61"]
                    .iter()
                    .map(|head| format!("{head}-0000-0000-0000-504944564944"))
                    .chain(["6F1D2B61-D5A0-11CF-BFC7-444553540000".to_string()])
                    .collect();
                match axes_write::align_text(text, &alive, a, b) {
                    Ok((new, _)) => json!(["ok", "", tree_of(&new, gegenprobe)]),
                    Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), tree_of(text, gegenprobe)]),
                }
            }
            "aufraeumen" => {
                let text = case[1].as_str().unwrap_or_default();
                let alive: HashSet<String> = case[2]
                    .as_array()
                    .map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_uppercase)).collect())
                    .unwrap_or_default();
                let blocks = axes::device_axes_from_text(text, &alive);
                let counted = axes_write::dead_blocks(&blocks).len();
                // Python counts: `(ok, '', n)`, and `(False, 's_gs_f_nichts_zu_tun', 0)` where there is nothing.
                let count = json!([counted > 0, counted]);
                match axes_write::clean_up_text(text, &alive) {
                    Ok((new, n)) => json!([count, "ok", n, tree_of(&new, gegenprobe)]),
                    Err(Failure::NothingToClean) => {
                        json!([count, "fehler", "s_gs_f_nichts_zu_tun", tree_of(text, gegenprobe)])
                    }
                    Err(failure) => json!([count, "fehler", failure.key().unwrap_or("?"), tree_of(text, gegenprobe)]),
                }
            }
            "satz" => {
                let (first, second) = (case[1].as_str().unwrap_or_default(), case[2].as_str().unwrap_or_default());
                let alive = |at: usize| -> HashSet<String> {
                    case[3][at]
                        .as_array()
                        .map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_uppercase)).collect())
                        .unwrap_or_default()
                };
                let (alive_first, alive_second) = (alive(0), alive(1));
                let blocks = axes::device_axes_from_text(first, &alive_first);
                let game = axes::game_axes_from_text(first, &alive_first);
                let set = device_sets::capture(&blocks, &game, "");
                if set.devices.is_empty() {
                    json!([["fehler", Failure::SetNothing.key().unwrap_or("?")]])
                } else {
                    let mut stored: Value =
                        serde_json::from_str(&jsonio::dumps(&set.to_value(), None, false)).unwrap_or(Value::Null);
                    if let Some(map) = stored.as_object_mut() {
                        map.remove("stand");
                    }
                    let blocks = axes::device_axes_from_text(second, &alive_second);
                    let game = axes::game_axes_from_text(second, &alive_second);
                    let (settings, missing) = device_sets::plan(&set, &blocks, &game);
                    let device_writes =
                        settings.iter().filter(|s| matches!(s, axes_write::Setting::Device { .. })).count();
                    let applied = match device_sets::apply_to_text(&set, second, &alive_second) {
                        Ok((new, _)) => json!(["ok", "", tree_of(&new, gegenprobe)]),
                        Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), tree_of(second, gegenprobe)]),
                    };
                    json!([["ok", stored], [device_writes, missing], applied])
                }
            }
            "lesen_zahl" => number(pyfloat::parse(case[1].as_str().unwrap_or_default())),
            "zahl" => {
                let value = case[1].as_f64().unwrap_or(0.0);
                let trimmed = {
                    let shown = pyfloat::fixed(value, 2);
                    let trimmed = shown.trim_end_matches('0').trim_end_matches('.');
                    if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
                };
                let mut texts = vec![pyfloat::repr(value), pyfloat::general(value), trimmed, pyfloat::fixed(value, 2)];
                if gegenprobe {
                    for text in &mut texts {
                        text.pop();
                    }
                }
                json!([texts[0], texts[1], texts[2], texts[3], pyfloat::round(value, 4)])
            }
            other => return Err(format!("unbekannte Art {other}")),
        };
        let mut row = case.as_array().cloned().unwrap_or_default();
        let last = row.len() - 1;
        row[last] = answer;
        done.push(Value::Array(row));
    }
    // Not used by every kind: keep the import honest.
    let _ = Prop::ALL;
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}
