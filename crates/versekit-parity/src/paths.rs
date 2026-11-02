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

//! Rust side of the parity check for data folder, settings and JSON.
//!
//! The same five parts as `tools/parity_paths.py`. Writing happens only in the
//! Python side's scratch folder: the write guard lets nothing else through
//! and records every attempt.

use std::fs;
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use versekit_core::jsonio::{self, Map, Value};
use versekit_core::legacy;
use versekit_core::paths::{Channel, Host, Paths, Recovered, SETTINGS_FILE, SUBFOLDERS};
use versekit_core::pypath::Flavor;
use versekit_core::{errors, writeguard};

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn s(text: &str) -> Value {
    Value::str(text)
}

fn opt(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::Str)
}

fn list(items: Vec<String>) -> Value {
    Value::Array(items.into_iter().map(Value::Str).collect())
}

fn channels(items: Vec<Channel>) -> Value {
    Value::Array(
        items
            .into_iter()
            .map(|(k, folder, stamp)| Value::Array(vec![s(&k), s(&folder), Value::Float(stamp)]))
            .collect(),
    )
}

fn recovered(r: Option<Recovered>) -> Value {
    match r {
        None => Value::Null,
        Some(Recovered::Backup) => s("backup"),
        Some(Recovered::Defaults) => s("defaults"),
    }
}

fn field<'a>(value: &'a Value, path: &[&str]) -> Result<&'a Value, String> {
    path.iter().try_fold(value, |v, key| v.get(key).ok_or_else(|| format!("Feld fehlt: {}", path.join("."))))
}

/// All files in a folder: relative name (timestamp replaced) → SHA-256,
/// like `listing` on the Python side.
fn listing(folder: &str) -> Value {
    fn walk(base: &str, rel: &str, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let full = Flavor::native().join(base, &name);
            let sub = if rel.is_empty() { name.clone() } else { Flavor::native().join(rel, &name) };
            if entry.path().is_dir() {
                walk(&full, &sub, out);
            } else {
                out.push((sub, full));
            }
        }
    }
    let mut files = Vec::new();
    walk(folder, "", &mut files);
    let kaputt = regex::Regex::new(r"\.kaputt-\d{8}-\d{6}(-\d+)?").expect("fester Ausdruck");
    let mut map: Map = files
        .into_iter()
        .map(|(rel, full)| {
            let rel = kaputt.replace_all(&rel, ".kaputt-<zeit>").into_owned();
            (rel, s(&sha256_hex(&fs::read(&full).unwrap_or_default())))
        })
        .collect();
    map.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Object(map)
}

/// SHA-256, so the list carries the same checksums as the Python side.
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// Part A: the real search.
fn phase_a(p: &Paths, scratch: &str) -> Result<Value, String> {
    let f = p.host.flavor;
    let pointer_file = p.pointer_file();
    let second = p.second_pointer();
    let app_folder = p.app_folder();
    let norm = |x: &str| f.normcase(&f.abspath(x, &p.host.cwd));
    if norm(scratch).starts_with(&format!("{}{}", norm(&app_folder), f.sep())) {
        return Err("Der Wegwerf-Ordner liegt im echten Datenordner — Abbruch.".into());
    }
    let mut names: Vec<&str> = SUBFOLDERS.iter().map(|(n, _)| *n).collect();
    names.extend(["logstand.json", "katalog-cache.json"]);
    let app_files: Map = names.iter().map(|n| (n.to_string(), s(&p.app_file(n)))).collect();
    let settings = p.settings();
    let setting_values: Map = settings
        .iter()
        .map(|(key, _)| {
            let values = vec![
                opt(p.setting(key)),
                Value::Bool(p.setting_bool(key, true)),
                Value::Bool(p.setting_bool(key, false)),
                Value::int(p.setting_int(key, -12345, None, None)),
                Value::int(p.setting_int(key, -12345, Some(1), Some(60))),
            ];
            (key.clone(), Value::Array(values))
        })
        .collect();
    let game = p.game_folder();
    let mismatch = p
        .channel_mismatch()
        .map_or(Value::Null, |(configured, used, all)| Value::Array(vec![s(&configured), opt(used), channels(all)]));
    let mut chosen = Vec::new();
    if let Some(game) = &game {
        let parent = f.dirname(game);
        chosen = vec![game.clone(), parent.clone(), format!("{}{}", f.dirname(&parent), f.sep())];
    }
    chosen.push(String::new());
    let resolve =
        Value::Array(chosen.iter().map(|c| Value::Array(vec![s(c), opt(p.resolve_game_folder(c))])).collect());
    Ok(Value::Object(vec![
        ("documents".into(), s(&p.documents())),
        ("default_folder".into(), s(&p.default_folder())),
        ("pointer_file".into(), s(&pointer_file)),
        ("second_pointer".into(), s(&second)),
        ("pointer_first".into(), opt(p.read_pointer(&pointer_file))),
        ("pointer_second".into(), opt(p.read_pointer(&second))),
        ("first_pointer_blocked".into(), Value::Bool(p.first_pointer_blocked())),
        ("app_folder".into(), s(&app_folder)),
        ("app_files".into(), Value::Object(app_files)),
        ("settings".into(), Value::Object(settings)),
        ("setting_values".into(), Value::Object(setting_values)),
        ("settings_recovered".into(), recovered(p.settings_recovered())),
        ("game_folder".into(), opt(game)),
        ("game_log".into(), opt(p.game_log(None))),
        ("log_backups".into(), list(p.log_backups(None))),
        ("localization_folder".into(), opt(p.localization_folder(None))),
        ("available_channels".into(), channels(p.available_channels())),
        ("channel_mismatch".into(), mismatch),
        ("searched_game_locations".into(), list(p.searched_game_locations(6))),
        ("wine_prefixes".into(), list(p.wine_prefixes())),
        ("game_roots".into(), list(p.game_roots())),
        ("launcher".into(), opt(p.launcher_from_registry())),
        ("resolve".into(), resolve),
        ("template".into(), Value::Object(p.template())),
    ]))
}

/// Part B: load every file from `in_rs` and write it to `out_rs` in all variants.
fn phase_b(scratch: &str, rels: &[String], variants: &[(String, usize, bool)]) -> Value {
    let f = Flavor::native();
    let mut loaded: Map = Vec::new();
    for rel in rels {
        let source = f.join_all(scratch, &["in_rs", rel]);
        let status = match jsonio::load_json(&source, None, "parity.load") {
            None | Some(Value::Null) => "none".to_string(),
            Some(data) => {
                let mut status = "ok".to_string();
                for (folder, indent, sort_keys) in variants {
                    let target = f.join_all(scratch, &["out_rs", folder, rel]);
                    if let Err(e) = jsonio::save_json(&target, &data, *indent, *sort_keys) {
                        status = format!("save:{:?}", e.kind());
                    }
                }
                status
            }
        };
        loaded.push((rel.clone(), s(&status)));
    }
    Value::Object(vec![
        ("rels".into(), list(rels.to_vec())),
        ("loaded".into(), Value::Object(loaded)),
        ("after".into(), listing(&f.join(scratch, "in_rs"))),
    ])
}

/// Part C: custom data folder via the environment variable.
fn phase_c(scratch: &str) -> Result<Value, String> {
    let f = Flavor::native();
    let home = f.join(scratch, "home_rs");
    fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, &home);
    let p = Paths::new(host);
    let mut r: Map = vec![
        ("app_folder".into(), s(&p.app_folder())),
        ("bestand".into(), s(&p.app_file("bestand.json"))),
        ("settings_empty".into(), Value::Object(p.settings())),
    ];
    r.push(("set_1".into(), Value::Bool(p.set_setting("sprache", s("de")))));
    r.push(("set_2".into(), Value::Bool(p.set_setting("pruefintervall_sekunden", Value::int(5)))));
    r.push(("set_3".into(), Value::Bool(p.set_setting("signalton", Value::Bool(false)))));
    r.push(("int".into(), Value::int(p.setting_int("pruefintervall_sekunden", 3, Some(1), Some(60)))));
    r.push(("bool".into(), Value::Bool(p.setting_bool("signalton", true))));
    let target = p.app_file(SETTINGS_FILE);
    fs::write(&target, "{kaputt").map_err(|e| e.to_string())?;
    let old = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    fs::File::options().write(true).open(&target).and_then(|file| file.set_modified(old)).map_err(|e| e.to_string())?;
    p.clear_settings_recovered();
    r.push(("recovered_settings".into(), Value::Object(p.settings())));
    r.push(("recovered".into(), recovered(p.settings_recovered())));
    r.push(("set_4".into(), Value::Bool(p.set_setting("sprache", s("en")))));
    r.push(("after".into(), listing(&home)));
    Ok(Value::Object(r))
}

/// `(name, text)` pairs of a list in the Python output.
fn pairs(value: &Value) -> Vec<(String, String)> {
    let Value::Array(items) = value else { return Vec::new() };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Array(parts) => match parts.as_slice() {
                [Value::Str(a), Value::Str(b)] => Some((a.clone(), b.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn slash(text: &str) -> String {
    text.replace('\\', "/")
}

/// Part D: moving the data folder, in the scenarios the Python side made up.
fn phase_d(scratch: &str, spec: &Value, p: &Paths) -> Result<Value, String> {
    let f = Flavor::native();
    let write = |base: &str, files: &[(String, String)]| -> Result<(), String> {
        for (rel, text) in files {
            let parts: Vec<&str> = rel.split('/').collect();
            let full = f.join_all(base, &parts);
            fs::create_dir_all(f.dirname(&full)).map_err(|e| e.to_string())?;
            fs::write(&full, text).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let source_files = pairs(field(spec, &["src"])?);
    let Value::Array(scenarios) = field(spec, &["scenarios"])? else { return Err("scenarios ist keine Liste".into()) };
    let mut results: Map = Vec::new();
    for scenario in scenarios {
        let name = field(scenario, &["name"])?.as_str().ok_or("name ist kein Text")?;
        let has_source = matches!(field(scenario, &["src"])?, Value::Bool(true));
        let kind = field(scenario, &["target"])?.as_str().ok_or("target ist kein Text")?;
        let base = f.join_all(scratch, &["d_rs", name]);
        let (src, dst) = (f.join(&base, "src"), f.join(&base, "dst"));
        if std::path::Path::new(&base).exists() {
            return Err(format!("Teil D lief hier schon: {base}. Bitte die Python-Seite neu laufen lassen."));
        }
        fs::create_dir_all(&base).map_err(|e| e.to_string())?;
        if has_source {
            write(&src, &source_files)?;
        }
        write(&dst, &pairs(field(scenario, &["dst"])?))?;
        let target = match kind {
            "inside" => f.join(&src, "Neu"),
            "ancestor" => base.clone(),
            "under_file" => f.join_all(&dst, &["datei", "unter"]),
            _ => dst.clone(),
        };
        let slashed = matches!(field(scenario, &["slash"])?, Value::Bool(true));
        let given = if slashed { format!("{}/", slash(&src)) } else { src.clone() };
        let mut items: Vec<String> = p.storage_items(&given).iter().map(|(_, rel)| slash(rel)).collect();
        items.sort();
        let status = p.storage_status(&target);
        let moved = p.move_storage(&given, &target);
        let (removed, kept) = p.remove_old_storage(&given, &target);
        let mut kept: Vec<String> = kept.iter().map(|k| slash(k)).collect();
        kept.sort();
        let count = |n: usize| Value::int(n as i64);
        results.push((
            name.to_string(),
            Value::Object(vec![
                ("items".into(), list(items)),
                (
                    "status".into(),
                    Value::Array(vec![
                        Value::Bool(status.writable),
                        count(status.files),
                        Value::Bool(!status.reason.is_empty()),
                    ]),
                ),
                ("move".into(), Value::Array(vec![count(moved.copied), count(moved.skipped), count(moved.failed)])),
                ("remove".into(), Value::Array(vec![count(removed), list(kept)])),
                ("src_after".into(), listing(&src)),
                ("dst_after".into(), listing(&dst)),
            ]),
        ));
    }
    Ok(Value::Object(vec![("results".into(), Value::Object(results))]))
}

/// All folders below `folder`, relative, with slashes, sorted (`dirs` on the Python side).
fn dirs(folder: &str) -> Vec<String> {
    fn walk(base: &str, rel: &str, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let sub = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            out.push(sub.clone());
            walk(&Flavor::native().join(base, &name), &sub, out);
        }
    }
    let mut out = Vec::new();
    walk(folder, "", &mut out);
    out.sort();
    out
}

/// Files and folders of a folder, or `null` if it is not there.
fn tree(folder: &str) -> Value {
    if !std::path::Path::new(folder).is_dir() {
        return Value::Null;
    }
    Value::Object(vec![("files".into(), listing(folder)), ("dirs".into(), list(dirs(folder)))])
}

/// Part E: the older files at the start, each scenario on a machine of its own in the scratch folder.
fn phase_e(scratch: &str, spec: &Value) -> Result<Value, String> {
    let f = Flavor::native();
    let write = |base: &str, files: &[(String, String)]| -> Result<(), String> {
        for (rel, text) in files {
            let parts: Vec<&str> = rel.split('/').collect();
            let full = f.join_all(base, &parts);
            fs::create_dir_all(f.dirname(&full)).map_err(|e| e.to_string())?;
            fs::write(&full, text).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let count = |n: usize| Value::int(n as i64);
    let Value::Array(scenarios) = field(spec, &["migrate"])? else { return Err("migrate ist keine Liste".into()) };
    let mut migrated: Map = Vec::new();
    for scenario in scenarios {
        let name = field(scenario, &["name"])?.as_str().ok_or("name ist kein Text")?;
        let base = f.join_all(scratch, &["e_rs", "m", name]);
        if std::path::Path::new(&base).exists() {
            return Err(format!("Teil E lief hier schon: {base}. Bitte die Python-Seite neu laufen lassen."));
        }
        let home = f.join(&base, "home");
        let config = f.join(&base, "appdata");
        let docs = f.join(&home, "Dokumente");
        fs::create_dir_all(&docs).map_err(|e| e.to_string())?;
        let mut vars = vec![("APPDATA", config.clone()), ("USERPROFILE", home.clone()), ("HOME", home.clone())];
        vars.push(("XDG_CONFIG_HOME", config.clone()));
        if matches!(scenario.get("home"), Some(Value::Bool(true))) {
            vars.push((legacy::ENV_HOME, f.join(&base, "eigen")));
        }
        let vars: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let p = Paths::new(Host::new(f, &vars, &base));
        let old = p.legacy_app_folder();
        if !matches!(field(scenario, &["legacy"])?, Value::Null) {
            write(&old, &pairs(field(scenario, &["legacy"])?))?;
        }
        write(&docs, &pairs(field(scenario, &["docs"])?))?;
        let needed = p.migration_needed();
        let copied = if needed { count(p.migrate()) } else { Value::Null };
        let relative = old.strip_prefix(&format!("{base}{}", f.sep())).unwrap_or(&old);
        migrated.push((
            name.to_string(),
            Value::Object(vec![
                ("legacy_folder".into(), s(&slash(relative))),
                ("needed".into(), Value::Bool(needed)),
                ("copied".into(), copied),
                ("needed_again".into(), Value::Bool(p.migration_needed())),
                ("docs_after".into(), tree(&docs)),
                ("legacy_after".into(), tree(&old)),
            ]),
        ));
    }
    let Value::Array(scenarios) = field(spec, &["tidy"])? else { return Err("tidy ist keine Liste".into()) };
    let machine = Paths::new(Host::new(f, &[], scratch));
    let mut tidied: Map = Vec::new();
    for scenario in scenarios {
        let name = field(scenario, &["name"])?.as_str().ok_or("name ist kein Text")?;
        let folder = f.join_all(scratch, &["e_rs", "t", name]);
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        write(&folder, &pairs(field(scenario, &["files"])?))?;
        let given = if matches!(scenario.get("slash"), Some(Value::Bool(true))) {
            format!("{folder}{}", f.sep())
        } else {
            folder.clone()
        };
        let moved = machine.tidy_storage(Some(&given));
        let again = machine.tidy_storage(Some(&given));
        tidied.push((
            name.to_string(),
            Value::Object(vec![
                ("moved".into(), count(moved)),
                ("again".into(), count(again)),
                ("after".into(), tree(&folder)),
            ]),
        ));
    }
    let named = f.join_all(scratch, &["e_rs", "t_genannt"]);
    write(&named, &[("bestand.json".into(), "{}".into())])?;
    let p = Paths::new(Host::new(f, &[(legacy::ENV_HOME, &named)], scratch));
    let named_result =
        Value::Object(vec![("moved".into(), count(p.tidy_storage(None))), ("after".into(), tree(&named))]);
    Ok(Value::Object(vec![
        ("migrate".into(), Value::Object(migrated)),
        ("tidy".into(), Value::Object(tidied)),
        ("tidy_named".into(), named_result),
    ]))
}

/// Deliberately wrong: one byte in the first output file and the app folder.
fn sabotage(scratch: &str, result: &mut Map) {
    let f = Flavor::native();
    let first = fs::read_dir(f.join_all(scratch, &["out_rs", "i1", "_synth"]))
        .ok()
        .and_then(|mut entries| entries.next())
        .and_then(|e| e.ok())
        .map(|e| e.path());
    if let Some(path) = first
        && let Ok(mut bytes) = fs::read(&path)
        && let Some(b) = bytes.last_mut()
    {
        *b ^= 0x01;
        let _ = fs::write(&path, bytes);
    }
    if let Some((_, Value::Object(a))) = result.iter_mut().find(|(k, _)| k == "phase_a") {
        jsonio::map_set(a, "app_folder", s("absichtlich falsch"));
    }
    // Part D: a move that claims to have copied nothing.
    if let Some((_, Value::Object(d))) = result.iter_mut().find(|(k, _)| k == "phase_d")
        && let Some((_, Value::Object(all))) = d.iter_mut().find(|(k, _)| k == "results")
        && let Some((_, Value::Object(fresh))) = all.iter_mut().find(|(k, _)| k == "frisch")
    {
        jsonio::map_set(fresh, "move", Value::Array(vec![Value::int(0), Value::int(0), Value::int(0)]));
    }
    // Part E: a copying at the start that claims to have copied nothing.
    if let Some((_, Value::Object(e))) = result.iter_mut().find(|(k, _)| k == "phase_e")
        && let Some((_, Value::Object(all))) = e.iter_mut().find(|(k, _)| k == "migrate")
        && let Some((_, Value::Object(fresh))) = all.iter_mut().find(|(k, _)| k == "frisch")
    {
        jsonio::map_set(fresh, "copied", Value::int(0));
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let scratch = field(&input, &["scratch"])?.as_str().ok_or("scratch ist kein Text")?.to_string();
    let rels: Vec<String> = match field(&input, &["phase_b", "rels"])? {
        Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => return Err("phase_b.rels ist keine Liste".into()),
    };
    let variants: Vec<(String, usize, bool)> = match field(&input, &["variants"])? {
        Value::Array(items) => items
            .iter()
            .filter_map(|v| match v {
                Value::Array(parts) => match parts.as_slice() {
                    [Value::Str(folder), Value::Int(indent), Value::Bool(sort)] => {
                        Some((folder.clone(), indent.parse().ok()?, *sort))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => return Err("variants ist keine Liste".into()),
    };
    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch));

    let paths = Paths::new(Host::current());
    let a = phase_a(&paths, &scratch)?;
    let blocked_a = writeguard::take_denied();
    let b = phase_b(&scratch, &rels, &variants);
    let c = phase_c(&scratch)?;
    let d = phase_d(&scratch, field(&input, &["phase_d", "spec"])?, &paths)?;
    let e = phase_e(&scratch, field(&input, &["phase_e", "spec"])?)?;
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);

    let errors = ERRORS.lock().map(|list| list.clone()).unwrap_or_default();
    let mut result: Map = vec![
        ("scratch".into(), s(&scratch)),
        ("phase_a".into(), a),
        ("phase_b".into(), b),
        ("phase_c".into(), c),
        ("phase_d".into(), d),
        ("phase_e".into(), e),
        ("blocked_a".into(), list(blocked_a)),
        ("blocked".into(), list(blocked)),
        ("errors".into(), list(errors)),
    ];
    if gegenprobe {
        sabotage(&scratch, &mut result);
    }
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_known_values() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        assert_eq!(sha256_hex(&long), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }
}
