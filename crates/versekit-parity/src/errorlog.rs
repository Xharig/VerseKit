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

//! Rust side of the parity check for redaction and the error log
//! (`tools/parity_errors.py`): redacts the same texts and replays the same
//! `record` calls into its own scratch folder.

use std::fs;

use versekit_core::error_log::{ErrorLog, FILE, Problem};
use versekit_core::jsonio::{self, Map, Newline, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::redact::Redactor;

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match value.get(name) {
        Some(Value::Array(items)) => Ok(items),
        _ => Err(format!("{name} fehlt")),
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let folder = text(&input, "rust_folder")?.to_string();
    let mut host = Host::current();
    host.set_var("SC_BP_HOME", &folder);
    let paths = Paths::new(host);

    let redactor = Redactor::new(&paths);
    let mut redacted = Vec::new();
    for (i, case) in list(&input, "redact")?.iter().enumerate() {
        let original = text(case, "text")?;
        let result = if gegenprobe && i == 3 { original.to_string() } else { redactor.redact(original) };
        redacted.push(Value::str(result));
    }

    let log_file = paths.app_file(FILE);
    jsonio::write_text_atomic(&log_file, text(&input, "initial")?, Newline::Native).map_err(|e| e.to_string())?;
    for step in list(&input, "steps")? {
        if matches!(step.get("clear"), Some(Value::Bool(true))) {
            let before = paths.app_file("fehler-vor-leeren.json");
            fs::copy(&log_file, &before).map_err(|e| e.to_string())?;
            ErrorLog::new(&paths, "").clear();
            continue;
        }
        let log = ErrorLog::new(&paths, text(step, "version")?);
        let problem = match step.get("problem") {
            Some(p @ Value::Object(_)) => Some(Problem {
                kind: text(p, "kind")?,
                message: text(p, "message")?,
                trace: p.get("trace").and_then(Value::as_str),
            }),
            _ => None,
        };
        log.record_at(text(step, "label")?, problem, text(step, "note")?, text(step, "time")?);
    }

    let log = ErrorLog::new(&paths, "");
    let result: Map = vec![
        ("redact".into(), Value::Array(redacted)),
        ("latest".into(), Value::Array(log.latest(5))),
        ("count".into(), Value::int(log.count() as i64)),
    ];
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
