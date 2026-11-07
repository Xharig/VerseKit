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

//! Rust side of the check for the usage report and the page counters (`tools/parity_usage.py`):
//! the same settings and counters through `usage::payload`, the same steps through
//! `usage::PageUsage`, all in a throwaway folder; nothing is sent.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::usage;

/// `gegenprobe`: the version goes last in the report, and the counter file is read back with
/// spaces after the commas.
pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Json = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let home = std::env::temp_dir().join(format!("versekit-parity-usage-{}", versekit_core::testdir::run_tag()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let result = cases(&input, &home, gegenprobe);
    let _ = fs::remove_dir_all(&home);
    let (payload, counting) = result?;
    let pages: Vec<&str> = {
        let mut ids = usage::PAGES.to_vec();
        ids.sort_unstable();
        ids
    };
    let all = json!({ "pages": pages, "payload": payload, "counting": counting });
    fs::write(out, serde_json::to_string(&all).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    println!("{} Meldungen, {} Zählläufe", payload.len(), counting.len());
    Ok(())
}

type Done = (Vec<Json>, Vec<Json>);

fn cases(input: &Json, home: &Path, gegenprobe: bool) -> Result<Done, String> {
    let mut payload = Vec::new();
    for case in input["payload"].as_array().ok_or("payload fehlt")? {
        let paths = paths_in(home, case["settings"].as_str().ok_or("settings fehlt")?)?;
        let mut facts = usage::facts(&paths, case["ui"].as_str().unwrap_or_default());
        facts.age = None;
        facts.game = usage::game_code(case["game"].as_str());
        facts.autostart = case["autostart"].as_bool().unwrap_or(false);
        facts.os = if case["os"] == "linux" { "linux" } else { "windows" };
        let counts = match value(&case["counts"])? {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        let mut report = usage::payload(case["version"].as_str().unwrap_or_default(), &facts, &counts);
        if gegenprobe {
            let version = report.remove(0);
            report.push(version);
        }
        payload.push(json!({ "text": usage::payload_text(&report) }));
    }
    let mut counting = Vec::new();
    for case in input["counting"].as_array().ok_or("counting fehlt")? {
        counting.push(count(case, home, gegenprobe)?);
    }
    Ok((payload, counting))
}

/// A data folder holding exactly this settings file.
fn paths_in(home: &Path, settings: &str) -> Result<Paths, String> {
    fs::write(home.join("einstellungen.json"), settings).map_err(|e| e.to_string())?;
    let data = home.to_string_lossy().into_owned();
    let game = home.join("kein-spiel").to_string_lossy().into_owned();
    let host = Host::new(
        Flavor::native(),
        &[("SC_BP_HOME", data.as_str()), ("HOME", data.as_str()), ("SC_INSTALL_DIR", game.as_str())],
        &data,
    );
    Ok(Paths::new(host))
}

fn value(json: &Json) -> Result<Value, String> {
    jsonio::loads(&json.to_string()).map_err(|e| format!("{e:?}"))
}

fn count(case: &Json, home: &Path, gegenprobe: bool) -> Result<Json, String> {
    let paths = paths_in(home, "{}")?;
    let file: PathBuf = home.join(usage::FILE);
    let _ = fs::remove_file(&file);
    if let Some(start) = case["start"].as_str() {
        fs::write(&file, start).map_err(|e| e.to_string())?;
    }
    let counter = usage::PageUsage::new(paths, "5.0.0");
    let mut sent: Option<Map> = None;
    for event in case["events"].as_array().ok_or("events fehlt")? {
        let clock = |at: usize| event[at].as_str().and_then(|text| text.parse::<f64>().ok());
        match event[0].as_str().unwrap_or_default() {
            "open" => counter.window_opened(),
            "toggle" => counter.group_toggled(),
            "page" => counter.page_opened(event[1].as_str().unwrap_or_default(), event[2].as_str(), clock(3)),
            "action" => counter.action(event[1].as_str().unwrap_or_default()),
            "close" => counter.window_closed(clock(1)),
            "snapshot" => sent = Some(counter.stored()),
            "forget" => {
                if let Some(state) = sent.take() {
                    counter.forget(&state);
                }
            }
            other => return Err(format!("unbekannter Schritt {other}")),
        }
    }
    let mut file_text = fs::read_to_string(&file).ok();
    if gegenprobe {
        file_text = file_text.map(|text| text.replace(',', ", "));
    }
    let stored = counter.stored();
    Ok(json!({
        "file": file_text,
        "stored": usage::file_text(&stored),
        "outgoing": usage::payload_text(&usage::outgoing(&Value::Object(stored))),
    }))
}
