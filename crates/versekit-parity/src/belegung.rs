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

//! Rust side of the check for what lies on the keys (`tools/parity_belegung.py`): plain names of inputs and
//! actions, the reading of the `defaultProfile.xml`, the four views and the list as a file, through
//! `controls::{input_readable, technical_readable, Profile::from_xml, view, labels_from_folder, csv_text}`.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use versekit_core::controls::{self, Devices, Profile, View};
use versekit_core::joysticks::{self, Kind};
use versekit_core::language::{self, Choice, Lang};

/// `gegenprobe`: the views are sorted by the text of the input instead of axes first and buttons by number, and
/// the names of the actions are written in capitals.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Value = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let cases = input["cases"].as_array().ok_or("cases fehlt")?;
    let folder = std::env::temp_dir().join(format!("versekit-parity-belegung-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let mut done = Vec::with_capacity(cases.len());
    let mut counter = 0_usize;
    let result = (|| -> Result<(), String> {
        for case in cases {
            let kind = case[0].as_str().ok_or("Art fehlt")?;
            let answer = match kind {
                "lesbar" => {
                    set_language(case[1].as_str().ok_or("Sprache fehlt")?);
                    let kind = kind_of(case[3].as_str().unwrap_or_default());
                    json!(controls::input_readable(case[2].as_str().ok_or("Eingabe fehlt")?, kind))
                }
                "technisch" => json!(controls::technical_readable(case[1].as_str().ok_or("Aktion fehlt")?)),
                "profil" => {
                    let profile = profile_of(&case[1])?;
                    json!({
                        "etiketten": profile.labels.iter().map(|(k, (l, d))| json!([k, [l, d]])).collect::<Vec<_>>(),
                        "standard": profile.defaults.iter().map(|(k, fields)| {
                            json!([k, fields.iter().map(|(f, i)| json!([f, i])).collect::<Vec<_>>()])
                        }).collect::<Vec<_>>(),
                        "gruppen": profile.groups.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                    })
                }
                "ansicht" => {
                    let profile = profile_of(&case[1])?;
                    let own = joysticks::bindings_from_text(case[2].as_str().ok_or("Datei fehlt")?);
                    let which = match case[3].as_str().ok_or("Sicht fehlt")? {
                        "meine" => View::Mine,
                        "alles" => View::All,
                        "standard" => View::Default,
                        _ => View::Free,
                    };
                    rows(&controls::view(which, &own, &profile), gegenprobe)
                }
                "namen" => {
                    set_language(case[2].as_str().ok_or("Sprache fehlt")?);
                    let profile = profile_of(&case[1])?;
                    counter += 1;
                    let game = folder.join(format!("n{counter}"));
                    put_ini(&game, &case[3])?;
                    let lang = if case[2] == "de" { Lang::De } else { Lang::En };
                    let labels = controls::labels_from_folder(&profile, &game.to_string_lossy(), lang);
                    Value::Array(
                        labels
                            .iter()
                            .map(|(action, l)| {
                                let name = if gegenprobe { l.name.to_uppercase() } else { l.name.clone() };
                                json!([action, [name, l.hint, l.real]])
                            })
                            .collect(),
                    )
                }
                "csv" => {
                    set_language(case[3].as_str().ok_or("Sprache fehlt")?);
                    let profile = profile_of(&case[1])?;
                    let own = joysticks::bindings_from_text(case[2].as_str().ok_or("Datei fehlt")?);
                    counter += 1;
                    let game = folder.join(format!("c{counter}"));
                    put_ini(&game, &case[4])?;
                    let lang = if case[3] == "de" { Lang::De } else { Lang::En };
                    let labels = controls::labels_from_folder(&profile, &game.to_string_lossy(), lang);
                    let text = controls::csv_text(&controls::view(View::All, &own, &profile), &labels);
                    let file = folder.join(format!("liste{counter}.csv"));
                    controls::write_csv(&file.to_string_lossy(), &text).map_err(|e| e.to_string())?;
                    let raw = fs::read(&file).map_err(|e| e.to_string())?;
                    let bom = raw.starts_with(&[0xef, 0xbb, 0xbf]);
                    let body =
                        String::from_utf8_lossy(raw.strip_prefix(&[0xef, 0xbb, 0xbf][..]).unwrap_or(&raw)).into_owned();
                    json!([true, bom, body])
                }
                other => return Err(format!("unbekannte Art {other}")),
            };
            let mut row = case.as_array().cloned().unwrap_or_default();
            let last = row.len() - 1;
            row[last] = answer;
            done.push(Value::Array(row));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&folder);
    language::set_language(Choice::De);
    result?;
    fs::write(out, serde_json::to_string(&json!({ "cases": done })).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{out}: {e}"))?;
    println!("{} Fälle", done.len());
    Ok(())
}

fn set_language(code: &str) {
    language::set_language(if code == "de" { Choice::De } else { Choice::En });
}

fn kind_of(word: &str) -> Option<Kind> {
    Kind::ALL.into_iter().find(|kind| kind.word() == word)
}

fn profile_of(xml: &Value) -> Result<Profile, String> {
    Profile::from_xml(xml.as_str().ok_or("Profil fehlt")?).ok_or_else(|| "Profil nicht lesbar".to_string())
}

/// The `global.ini` files of a case under a scratch game folder.
fn put_ini(game: &Path, files: &Value) -> Result<(), String> {
    let sub = files["unter"].as_str().unwrap_or("data");
    let german = files["deordner"].as_str().unwrap_or("german_(germany)");
    for (key, folder) in [("de", german), ("en", "english")] {
        if let Some(text) = files[key].as_str() {
            let path = game.join(sub).join("Localization").join(folder).join("global.ini");
            fs::create_dir_all(path.parent().expect("parent")).map_err(|e| e.to_string())?;
            fs::write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    fs::create_dir_all(game).map_err(|e| e.to_string())
}

fn rows(devices: &Devices, gegenprobe: bool) -> Value {
    Value::Array(
        devices
            .iter()
            .map(|(id, entries)| {
                let mut entries = entries.clone();
                if gegenprobe {
                    entries.sort_by(|a, b| a.input.cmp(&b.input));
                }
                let rows: Vec<Value> = entries
                    .iter()
                    .map(|e| json!([e.input, e.action, e.section, e.kind.map_or("", Kind::word), e.source.word()]))
                    .collect();
                json!([id, rows])
            })
            .collect(),
    )
}
