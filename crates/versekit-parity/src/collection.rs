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

//! Rust side of the parity check for the blueprint collection.
//!
//! Parts N and B as in `tools/parity_collection.py`, in their own folders
//! (`names_rs`, `home_rs`, `appdata_rs`) inside the Python side's scratch folder.
//! Catalog, English archive texts and blueprint names come from its output.

use std::cell::Cell;
use std::fs;
use std::sync::Mutex;

use versekit_core::collection::{self, Catalog, Collection, Exports, Known, Shrinkage};
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::{Host, Paths, name_key};
use versekit_core::phrases::{ArchiveText, BackRoute, EnglishNames, clear_back_caches};
use versekit_core::pypath::Flavor;
use versekit_core::{errors, legacy, writeguard};

const FIXED_NOW: &str = "2026-10-19 12:00:00";
const SOURCES: [&str; 7] = ["nachlese", "log", "basetool", "launcher", "hand", "start", "import"];

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn fixed() -> String {
    FIXED_NOW.to_string()
}

fn s(text: &str) -> Value {
    Value::str(text)
}

fn opt(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::Str)
}

fn strs(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::Str).collect())
}

fn bools(items: Vec<bool>) -> Value {
    Value::Array(items.into_iter().map(Value::Bool).collect())
}

fn int(n: usize) -> Value {
    Value::Int(n.to_string())
}

fn shrinkage(value: Option<Shrinkage>) -> Value {
    value.map_or(Value::Null, |x| Value::Array(vec![int(x.now), Value::Int(x.highest.to_string()), x.folder]))
}

/// The catalog from the Python output, rebuilt on every call.
struct ListCatalog(Vec<(String, Option<String>)>);

impl Catalog for ListCatalog {
    fn known(&self) -> Result<Known, String> {
        Ok(Known::new(self.0.iter().cloned()))
    }
}

/// The English archive texts the Python side has read once.
struct FileArchive(Option<String>);

impl ArchiveText for FileArchive {
    fn english_global_ini(&self) -> Option<Vec<u8>> {
        fs::read(self.0.as_ref()?).ok()
    }
}

#[derive(Default)]
struct CountingExports(Cell<usize>);

impl Exports for CountingExports {
    fn archive(&self, _data: &Map) -> Result<(), String> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("Feld fehlt: {key}"))
}

fn str_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn summary(data: &Map) -> Value {
    let get = |key: &str| data.iter().find(|(k, _)| k == key).map_or(Value::Null, |(_, v)| v.clone());
    Value::Object(vec![
        ("version".into(), get("version")),
        ("stand".into(), get("stand")),
        ("count".into(), int(collection::count(data))),
        (
            "by_source".into(),
            Value::Array(
                collection::by_source(data).into_iter().map(|(k, n)| Value::Array(vec![s(&k), int(n)])).collect(),
            ),
        ),
        ("restorable".into(), int(collection::restorable(data))),
        ("names".into(), strs(collection::names(data))),
        ("keys".into(), strs(collection::keys(data))),
        ("seen".into(), strs(collection::keys_seen_in_game(data))),
        ("data".into(), Value::Object(data.clone())),
    ])
}

fn home_paths(home: &str, appdata: Option<&str>) -> Paths {
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, home);
    if let Some(appdata) = appdata {
        host.set_var("APPDATA", appdata);
    }
    Paths::new(host)
}

/// Place input files in the custom data folder (flat, as with the environment variable).
fn place(inputs: &str, home: &str, names: &[&str], target: Option<&str>) -> Result<(), String> {
    let f = Flavor::native();
    fs::create_dir_all(home).map_err(|e| e.to_string())?;
    for name in names {
        fs::copy(f.join(inputs, name), f.join(home, target.unwrap_or(name))).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// Place the collection and shrinkage mark of this stage next to the other files.
fn stage(home: &str, name: &str, mark: &str) -> Result<(), String> {
    let f = Flavor::native();
    let folder = f.join(home, "Stufen");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let pairs = [
        (f.join(home, "bestand.json"), format!("{name}.json")),
        (f.join(home, "bestand.bak.json"), format!("{name}.bak.json")),
        (mark.to_string(), format!("{name}-hochwasser.json")),
    ];
    for (source, target) in pairs {
        if fs::metadata(&source).is_ok_and(|m| m.is_file()) {
            fs::copy(&source, f.join(&folder, &target)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

struct Shared<'a> {
    scratch: &'a str,
    inputs: String,
    catalog: &'a ListCatalog,
    archive: &'a FileArchive,
    exports: &'a CountingExports,
}

fn phase_n(sh: &Shared, names: &[String]) -> Result<Value, String> {
    let home = Flavor::native().join(sh.scratch, "names_rs");
    place(&sh.inputs, &home, &["einstellungen.json"], None)?;
    let paths = home_paths(&home, None);
    clear_back_caches();
    let route = BackRoute { paths: &paths, archive: sh.archive };
    let c = Collection { paths: &paths, catalog: sh.catalog, names: &route, exports: sh.exports, clock: fixed };
    let known = sh.catalog.known()?;
    let results = names
        .iter()
        .map(|name| {
            Value::Array(vec![
                s(&name_key(name)),
                s(&collection::unique_match(name, &known)),
                strs(route.english_names(name)),
                opt(c.translated_back(name, &known)),
                s(&c.catalog_name(name, Some(&known))),
            ])
        })
        .collect();
    let without = names.iter().take(60).map(|name| c.catalog_name(name, None)).collect::<Vec<_>>();
    Ok(Value::Object(vec![
        ("results".into(), Value::Array(results)),
        ("without_known".into(), strs(without)),
        ("localization_folder".into(), opt(paths.localization_folder(None))),
    ]))
}

fn phase_b(sh: &Shared, log_names: &[String], names: &[String]) -> Result<Vec<Value>, String> {
    let f = Flavor::native();
    let home = f.join(sh.scratch, "home_rs");
    let appdata = f.join(sh.scratch, "appdata_rs");
    place(&sh.inputs, &home, &["bestand.json", "bestand.bak.json", "einstellungen.json", "namen-rueckweg.json"], None)?;
    let paths = home_paths(&home, Some(&appdata));
    clear_back_caches();
    let route = BackRoute { paths: &paths, archive: sh.archive };
    let c = Collection { paths: &paths, catalog: sh.catalog, names: &route, exports: sh.exports, clock: fixed };
    let mut steps: Vec<Value> = Vec::new();
    let mut step = |name: &str, value: Value| steps.push(Value::Array(vec![s(name), value]));

    let mark = c.high_water_file();
    let mut b = c.load();
    step("load", summary(&b));
    step("high_water_file", s(&mark));
    step("shrink_1", shrinkage(c.check_shrinkage(&b)));
    step("shrink_1b", shrinkage(c.check_shrinkage(&b)));
    step("align", int(c.align(&mut b)));
    step("save_1", Value::Bool(c.save(&mut b)));
    stage(&home, "1-angeglichen", &mark)?;
    let mut added = Vec::new();
    for (i, name) in log_names.iter().enumerate() {
        let when = (i % 3 != 0).then(|| format!("2026-09-{:02} 10:00:00", i % 28 + 1));
        added.push(c.add(&mut b, name, SOURCES[i % SOURCES.len()], when.as_deref()));
    }
    let first: Vec<String> = collection::names(&b).into_iter().take(150).collect();
    for (i, name) in first.iter().enumerate() {
        added.push(c.add(&mut b, name, SOURCES[(i + 3) % SOURCES.len()], None));
    }
    step("add", bools(added));
    step("save_2", Value::Bool(c.save(&mut b)));
    stage(&home, "2-aufgenommen", &mark)?;
    let mut removed: Vec<bool> =
        log_names.iter().enumerate().filter(|(i, _)| i % 5 == 0).map(|(_, n)| collection::remove(&mut b, n)).collect();
    let first_keys: Vec<String> = collection::keys(&b).into_iter().take(30).collect();
    removed.extend(first_keys.iter().map(|k| collection::remove(&mut b, k)));
    removed.push(collection::remove(&mut b, "gibt es nicht"));
    step("remove", bools(removed));
    step("contains", bools(log_names.iter().map(|n| collection::contains(&b, n)).collect()));
    step("after_remove", summary(&b));
    step("shrink_2", shrinkage(c.check_shrinkage(&b)));
    let more: Vec<String> = collection::keys(&b).into_iter().take(25).collect();
    step("remove_2", bools(more.iter().map(|k| collection::remove(&mut b, k)).collect()));
    step("shrink_2b", shrinkage(c.check_shrinkage(&b)));
    step("save_3", Value::Bool(c.save(&mut b)));
    stage(&home, "3-entfernt", &mark)?;
    step("state_1", shrinkage(c.shrinkage_state()));
    c.reset_high_water();
    step("state_2", shrinkage(c.shrinkage_state()));
    step("shrink_3", shrinkage(c.check_shrinkage(&b)));
    place(&sh.inputs, &home, &["alt.json"], Some("bestand.json"))?;
    let b = c.load();
    step("migrate", summary(&b));
    stage(&home, "4-umgezogen", &mark)?;
    place(&sh.inputs, &home, &["kaputt.json"], Some("bestand.json"))?;
    let mut b = c.load();
    step("broken", summary(&b));
    step("save_4", Value::Bool(c.save(&mut b)));
    stage(&home, "5-gerettet", &mark)?;
    let repr = |e: Option<std::io::Error>| e.map_or_else(|| "None".to_string(), |e| format!("{e:?}"));
    step("reset_1", s(&repr(c.reset())));
    step("reset_2", s(&repr(c.reset())));
    let mut b = c.load();
    step("empty", summary(&b));
    let first = c.add(&mut b, "Helm", "hand", None);
    let second = c.add(&mut b, "Helm", "basetool", None);
    step("add_empty", bools(vec![first, second]));
    step("save_5", Value::Bool(c.save(&mut b)));
    step("shrink_end", shrinkage(c.check_shrinkage(&b)));
    step("final", summary(&b));
    let mut b = c.empty();
    let added_all = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let when = (i % 2 == 0).then(|| format!("2026-08-{:02} 08:00:00", i % 28 + 1));
            c.add(&mut b, name, SOURCES[i % SOURCES.len()], when.as_deref())
        })
        .collect();
    step("add_all", bools(added_all));
    step("all", summary(&b));
    let mut raw = c.empty();
    if let Some((_, Value::Object(bp))) = raw.iter_mut().find(|(k, _)| k == "bauplaene") {
        for name in names {
            let entry = Value::Object(vec![
                ("name".into(), s(name)),
                ("quelle".into(), s("log")),
                ("zeit".into(), s(FIXED_NOW)),
            ]);
            jsonio::map_set(bp, &name_key(name), entry);
        }
    }
    step("align_all", int(c.align(&mut raw)));
    step("aligned", summary(&raw));
    step("save_6", Value::Bool(c.save(&mut raw)));
    stage(&home, "6-alle", &mark)?;
    Ok(steps)
}

/// Deliberately wrong: one byte in the collection and one result.
fn sabotage(scratch: &str, result: &mut Map) {
    let path = Flavor::native().join_all(scratch, &["home_rs", "bestand.json"]);
    if let Ok(mut bytes) = fs::read(&path)
        && let Some(b) = bytes.get_mut(10)
    {
        *b ^= 0x01;
        let _ = fs::write(&path, bytes);
    }
    if let Some((_, Value::Array(steps))) = result.iter_mut().find(|(k, _)| k == "phase_b")
        && let Some(Value::Array(pair)) =
            steps.iter_mut().find(|v| matches!(v, Value::Array(p) if p.first() == Some(&s("align"))))
    {
        pair[1] = s("absichtlich falsch");
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let scratch = field(&input, "scratch")?.as_str().ok_or("scratch ist kein Text")?.to_string();
    let known: Vec<(String, Option<String>)> = match field(&input, "known")? {
        Value::Array(items) => items
            .iter()
            .filter_map(|pair| match pair {
                Value::Array(p) => match p.as_slice() {
                    [Value::Str(k), n] => Some((k.clone(), n.as_str().map(String::from))),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => return Err("known ist keine Liste".into()),
    };
    let catalog = ListCatalog(known);
    let archive = FileArchive(field(&input, "archive_file")?.as_str().map(String::from));
    let names = str_list(field(&input, "names")?);
    let log_names = str_list(field(&input, "log_names")?);
    let exports = CountingExports::default();
    let shared = Shared {
        scratch: &scratch,
        inputs: Flavor::native().join(&scratch, "inputs"),
        catalog: &catalog,
        archive: &archive,
        exports: &exports,
    };

    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch));
    let n = phase_n(&shared, &names);
    let b = phase_b(&shared, &log_names, &names);
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    let (n, b) = (n?, b?);

    let errors = ERRORS.lock().map(|list| list.clone()).unwrap_or_default();
    let mut result: Map = vec![
        ("scratch".into(), s(&scratch)),
        ("phase_n".into(), n),
        ("phase_b".into(), Value::Array(b)),
        ("exports".into(), int(exports.0.get())),
        ("blocked".into(), strs(blocked)),
        ("errors".into(), strs(errors)),
    ];
    if gegenprobe {
        sabotage(&scratch, &mut result);
    }
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
