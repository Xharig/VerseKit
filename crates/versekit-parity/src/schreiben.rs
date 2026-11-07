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

//! Rust side of the check for the changes of the `actionmaps.xml` (`tools/parity_schreiben.py`) through
//! `joysticks_write::{bind_action_text, reset_text, active_form_text, swap_id_text}`. What is compared is the
//! meaning of the result (a tree without layout), for the swap of an id the text itself.

use std::fs;

use roxmltree::{Document, Node};
use serde_json::{Value, json};
use versekit_core::joysticks;
use versekit_core::joysticks_write::{self, Failure};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

/// `gegenprobe`: the trees are compared with their children in reverse order and without the name of the
/// profile.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let mut done = Vec::with_capacity(cases.len());
    let folder = std::env::temp_dir().join(format!("versekit-parity-schreiben-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let home = folder.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &home)], &home));
    let mut counter = 0_usize;
    for case in cases {
        let kind = case[0].as_str().ok_or("Art fehlt")?;
        let text = case[1].as_str().unwrap_or_default();
        let answer = match kind {
            "profil" => {
                let name = case[2].as_str().unwrap_or_default();
                match joysticks_write::profile_text(text, name) {
                    Ok(profile) => json!(["ok", 1, tree_of(&profile, gegenprobe)]),
                    Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), tree_of(text, gegenprobe)]),
                }
            }
            "name" => match joysticks::check_name(text) {
                Ok(()) => json!(["ok", 1]),
                Err(key) => json!(["fehler", key]),
            },
            "liste" => {
                counter += 1;
                let game = folder.join(format!("l{counter}"));
                let mappings = game.join("USER").join("Client").join("0").join("controls").join("mappings");
                fs::create_dir_all(&mappings).map_err(|e| e.to_string())?;
                for entry in case[1].as_array().ok_or("Dateien fehlen")? {
                    let name = entry.as_str().unwrap_or_default();
                    fs::write(mappings.join(name), "<x/>").map_err(|e| format!("{name}: {e}"))?;
                }
                json!(joysticks::profiles(&paths, Some(&game.to_string_lossy())))
            }
            "bind" => {
                let args = &case[2];
                let part = |i: usize| args[i].as_str().unwrap_or_default();
                outcome(
                    joysticks_write::bind_action_text(text, part(0), part(1), part(2), part(3)).map(|t| (t, 1)),
                    text,
                    gegenprobe,
                    false,
                )
            }
            "reset" => outcome(joysticks_write::reset_text(text), text, gegenprobe, false),
            "import" => {
                let incoming = case[2].as_str().ok_or("Quelle fehlt")?;
                // The target must be readable too (4.12 reads it first), but is not changed by the text.
                outcome(joysticks_write::active_form_text(incoming), text, gegenprobe, false)
            }
            "swap" => {
                let args = &case[2];
                outcome(
                    joysticks_write::swap_id_text(
                        text,
                        args[0].as_str().unwrap_or_default(),
                        args[1].as_str().unwrap_or_default(),
                    ),
                    text,
                    gegenprobe,
                    true,
                )
            }
            other => return Err(format!("unbekannte Art {other}")),
        };
        let mut row = case.as_array().cloned().unwrap_or_default();
        let last = row.len() - 1;
        row[last] = answer;
        done.push(Value::Array(row));
    }
    let _ = fs::remove_dir_all(&folder);
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}

/// `[status, count or key, result]`: the result is the tree (or, for the swap of an id, the text).
fn outcome(result: Result<(String, usize), Failure>, input: &str, gegenprobe: bool, as_text: bool) -> Value {
    let show = |text: &str| if as_text { json!(text) } else { tree_of(text, gegenprobe) };
    match result {
        Ok((text, count)) => json!(["ok", count, show(&text)]),
        Err(failure) => json!(["fehler", failure.key().unwrap_or("?"), show(input)]),
    }
}

fn tree_of(text: &str, gegenprobe: bool) -> Value {
    match Document::parse(text) {
        Ok(document) => node(document.root_element(), gegenprobe),
        Err(_) => Value::Null,
    }
}

fn node(element: Node, gegenprobe: bool) -> Value {
    let mut attributes: Vec<Value> = element
        .attributes()
        .filter(|a| !(gegenprobe && a.name() == "profileName"))
        .map(|a| json!([a.name(), a.value()]))
        .collect();
    attributes.sort_by_key(Value::to_string);
    let mut children: Vec<Value> = element.children().filter(Node::is_element).map(|c| node(c, gegenprobe)).collect();
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
