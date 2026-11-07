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

//! Rust side of the parity check for the output files.
//!
//! Every case of `tools/parity_export.py` is copied from its template into
//! `<case>_rs` inside the Python side's scratch folder; the collection is
//! loaded and saved there with `FileExports`, which writes the output files.

use std::cell::RefCell;
use std::fs;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use versekit_core::collection::{Collection, Exports, NoCatalog};
use versekit_core::export::{FileExports, contract_log, iso_time, unique_tags};
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::language::{self, Choice};
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::EnglishNames;
use versekit_core::pypath::Flavor;
use versekit_core::{errors, legacy, writeguard};

const FIXED_NOW: &str = "2026-10-19 12:00:00";
const HOME_MARK: &str = "<HOME>";

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static FIXED_UNIX: AtomicI64 = AtomicI64::new(0);

fn fixed_now() -> String {
    FIXED_NOW.to_string()
}

fn fixed_unix() -> i64 {
    FIXED_UNIX.load(Ordering::Relaxed)
}

fn s(text: &str) -> Value {
    Value::str(text)
}

fn strs(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::Str).collect())
}

struct NoNames;

impl EnglishNames for NoNames {
    fn english_names(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }
}

/// The output files, with what each run returned (`[ok, folder, files]`, or
/// `null` for a run stopped by a data error).
struct Recording<'a> {
    inner: FileExports<'a>,
    results: RefCell<Vec<Value>>,
}

impl Exports for Recording<'_> {
    fn archive(&self, data: &Map) -> Result<(), String> {
        match self.inner.archive_files(data) {
            Ok(done) => {
                self.results.borrow_mut().push(Value::Array(vec![
                    Value::Bool(!done.written.is_empty()),
                    Value::Str(done.folder),
                    strs(done.written),
                ]));
                Ok(())
            }
            Err(e) => {
                self.results.borrow_mut().push(Value::Null);
                Err(e)
            }
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("Feld fehlt: {key}"))
}

fn copy_dir(source: &str, target: &str) -> Result<(), String> {
    let f = Flavor::native();
    fs::create_dir_all(target).map_err(|e| format!("{target}: {e}"))?;
    for entry in fs::read_dir(source).map_err(|e| format!("{source}: {e}"))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let (from, to) = (f.join(source, &name), f.join(target, &name));
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("{from}: {e}"))?;
        }
    }
    Ok(())
}

/// The template into this side's data folder; `<HOME>` in the settings
/// becomes the own folder.
fn place(template: &str, home: &str) -> Result<(), String> {
    copy_dir(template, home)?;
    let settings = Flavor::native().join(home, "einstellungen.json");
    if let Ok(text) = fs::read_to_string(&settings)
        && text.contains(HOME_MARK)
    {
        fs::write(&settings, text.replace(HOME_MARK, &home.replace('\\', "\\\\"))).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run_case(scratch: &str, case: &Value, version: &str) -> Result<Value, String> {
    let f = Flavor::native();
    let name = field(case, "name")?.as_str().ok_or("name ist kein Text")?;
    let lang = field(case, "lang")?.as_str().ok_or("lang ist kein Text")?;
    let home = f.join(scratch, &format!("{name}_rs"));
    if fs::symlink_metadata(&home).is_ok() {
        return Err(format!("{home} gibt es schon — jeder Lauf braucht einen neuen Wegwerf-Ordner"));
    }
    place(&f.join_all(scratch, &["inputs", name]), &home)?;
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, &home);
    host.set_var("APPDATA", &f.join(scratch, "appdata_rs"));
    let paths = Paths::new(host);
    language::set_language(if lang == "en" { Choice::En } else { Choice::De });
    if let Ok(mut list) = ERRORS.lock() {
        list.clear();
    }
    let exports = Recording {
        inner: FileExports { paths: &paths, version, clock: fixed_unix },
        results: RefCell::new(Vec::new()),
    };
    let c = Collection { paths: &paths, catalog: &NoCatalog, names: &NoNames, exports: &exports, clock: fixed_now };
    let mut data = c.load();
    let saved = c.save(&mut data);
    let errors = ERRORS.lock().map(|list| list.clone()).unwrap_or_default();
    let mut tags: Vec<(String, String)> = unique_tags(&paths).into_iter().collect();
    tags.sort();
    let log = match contract_log(&paths) {
        Ok(entries) => Value::Array(entries),
        Err(_) => s("Abbruch"),
    };
    Ok(Value::Object(vec![
        ("name".into(), s(name)),
        ("save".into(), Value::Bool(saved)),
        ("archive".into(), Value::Array(exports.results.into_inner())),
        ("errors".into(), strs(errors)),
        ("tags".into(), Value::Array(tags.into_iter().map(|(k, v)| strs([k, v])).collect())),
        ("log".into(), log),
    ]))
}

/// Deliberately wrong, five times: one byte in a written file, a file too
/// many, one converted time, the result of a run and the contract log of a
/// case. Fails if a target of the sabotage is missing.
fn sabotage(scratch: &str, result: &mut Map) -> Result<(), String> {
    let f = Flavor::native();
    let path = f.join_all(scratch, &["echt_de_rs", "export", "scmdb-import.json"]);
    let mut bytes = fs::read(&path).map_err(|e| format!("Gegenprobe: {path}: {e}"))?;
    let byte = bytes.get_mut(10).ok_or_else(|| format!("Gegenprobe: {path} ist zu kurz"))?;
    *byte ^= 0x01;
    fs::write(&path, bytes).map_err(|e| format!("Gegenprobe: {path}: {e}"))?;
    let extra = f.join_all(scratch, &["echt_de_rs", "export", "ueberzaehlig.json"]);
    fs::write(&extra, "{}").map_err(|e| format!("Gegenprobe: {extra}: {e}"))?;
    let Some(Value::Array(pair)) = result.iter_mut().find(|(k, _)| k == "iso").and_then(|(_, v)| match v {
        Value::Array(iso) => iso.first_mut(),
        _ => None,
    }) else {
        return Err("Gegenprobe: kein Zeitwert".into());
    };
    pair[1] = s("absichtlich falsch");
    let Some((_, Value::Array(cases))) = result.iter_mut().find(|(k, _)| k == "cases") else {
        return Err("Gegenprobe: keine Fälle".into());
    };
    let rand = cases
        .iter_mut()
        .find_map(|c| match c {
            Value::Object(map) if map.iter().any(|(k, v)| k == "name" && *v == s("rand")) => Some(map),
            _ => None,
        })
        .ok_or("Gegenprobe: Fall rand fehlt")?;
    jsonio::map_set(rand, "archive", Value::Array(vec![Value::Null]));
    jsonio::map_set(rand, "log", Value::Array(Vec::new()));
    Ok(())
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let scratch = field(&input, "scratch")?.as_str().ok_or("scratch ist kein Text")?.to_string();
    let version = field(&input, "version")?.as_str().ok_or("version ist kein Text")?.to_string();
    let unix = match field(&input, "fixed_unix")? {
        Value::Int(text) => text.parse().map_err(|_| "fixed_unix ist keine Zahl")?,
        _ => return Err("fixed_unix ist keine Zahl".into()),
    };
    FIXED_UNIX.store(unix, Ordering::Relaxed);
    let Value::Array(times) = field(&input, "times")? else { return Err("times ist keine Liste".into()) };
    let Value::Array(cases) = field(&input, "cases")? else { return Err("cases ist keine Liste".into()) };

    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    let iso = times.iter().map(|t| Value::Array(vec![t.clone(), iso_time(Some(t)).map_or(Value::Null, Value::Str)]));
    let iso: Vec<Value> = iso.collect();
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch));
    let results: Result<Vec<Value>, String> = cases.iter().map(|case| run_case(&scratch, case, &version)).collect();
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);

    let mut result: Map = vec![
        ("scratch".into(), s(&scratch)),
        ("iso".into(), Value::Array(iso)),
        ("cases".into(), Value::Array(results?)),
        ("blocked".into(), strs(blocked)),
    ];
    if gegenprobe {
        sabotage(&scratch, &mut result)?;
    }
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
