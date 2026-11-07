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

//! Rust side of the check of the backup (`tools/parity_sicherung.py`): the same cases through
//! `versekit_core::backup`, in the same throwaway folder one after the other.
//!
//! `gegenprobe`: a backup without its protections: everything comes along, every path is taken, every
//! file counts as a backup, the settings go in as they are, the controls are not collected and the active binding
//! always goes back. The check must turn red.

use std::fs;
use std::path::Path;

use serde_json::{Map as JsonMap, Value as Json, json};
use versekit_core::backup;
use versekit_core::jsonio::{self, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::{writeguard, zip};

/// The file that marks the work folder as this check's own (the Python side lays it).
const MARK: &str = ".vk-parity-sicherung";

/// The marks of the folders of the run in a text of a case, filled in like the Python side does.
fn expand(value: &Json, root: &str, work: &str) -> Json {
    match value {
        Json::String(text) => {
            Json::String(text.replace("{root}", root).replace("{ROOT}", &root.to_uppercase()).replace("{work}", work))
        }
        Json::Array(items) => Json::Array(items.iter().map(|v| expand(v, root, work)).collect()),
        Json::Object(map) => Json::Object(map.iter().map(|(k, v)| (k.clone(), expand(v, root, work))).collect()),
        other => other.clone(),
    }
}

/// A file of a case: a text as it is, an object as JSON.
fn bytes_of(value: &Json, root: &str, work: &str) -> Vec<u8> {
    match expand(value, root, work) {
        Json::String(text) => text.into_bytes(),
        other => other.to_string().into_bytes(),
    }
}

/// A JSON object of the file as the program reads it.
fn object(json: &Json) -> Result<jsonio::Map, String> {
    match jsonio::loads(&json.to_string()).map_err(|e| format!("{e:?}"))? {
        Value::Object(map) => Ok(map),
        _ => Err("Einstellungen sind kein Objekt".to_string()),
    }
}

/// Empties the folder of the run except the archives of the cases; lays the state of the case.
fn fresh(root: &str, work: &str, state: &Json) -> Result<(), String> {
    // ⚠ This empties the folder: only the folder of this check (the Python side marks it).
    if !Path::new(work).join(MARK).is_file() {
        return Err(format!("{work} ist nicht der Arbeitsordner dieser Prüfung (Marke {MARK} fehlt): nichts gelöscht"));
    }
    for entry in fs::read_dir(work).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.file_name().is_some_and(|n| n == "zips" || n == MARK) {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        } else {
            fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    fs::create_dir_all(Path::new(work).join("Spiele")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    if let Some(map) = state.as_object() {
        for (rel, content) in map {
            let target = rel.split('/').fold(Path::new(root).to_path_buf(), |p, part| p.join(part));
            fs::create_dir_all(target.parent().ok_or("kein Ordner")?).map_err(|e| e.to_string())?;
            fs::write(&target, bytes_of(content, root, work)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Every file below `work` except the archives of the cases and the pointer files of the copy: the same
/// description the Python side gives.
fn tree_of(work: &str) -> Json {
    fn walk(dir: &Path, work: &Path, out: &mut JsonMap<String, Json>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(work).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            if path.is_dir() {
                if !matches!(rel.as_str(), "zips" | "AppData" | "Dokumente" | "Documents" | "Verse-Kit") {
                    walk(&path, work, out);
                }
                continue;
            }
            if rel == MARK {
                continue;
            }
            // The error log of 4.12 is written where Rust keeps it in memory.
            if rel.starts_with("VerseKit-Daten/Diagnose/") {
                continue;
            }
            let Ok(data) = fs::read(&path) else { continue };
            let name = rel.rsplit('/').next().unwrap_or("").to_lowercase();
            if name.ends_with(".zip") {
                let mut names: Vec<String> =
                    zip::entries(&data).map(|e| e.into_iter().map(|e| e.name).collect()).unwrap_or_default();
                names.retain(|n| n != backup::INFO_FILE);
                names.sort();
                out.insert("<vorher>".to_string(), json!(names));
                continue;
            }
            let text = String::from_utf8_lossy(&data).into_owned();
            let mut entry = json!({ "text": text });
            if name.ends_with(".json")
                && let Ok(mut value) = serde_json::from_str::<Json>(&text)
            {
                if let Some(map) = value.as_object_mut() {
                    map.remove("letzte_sicherung");
                }
                entry = json!({ "json": value });
            }
            out.insert(rel, entry);
        }
    }
    let mut out = JsonMap::new();
    walk(Path::new(work), Path::new(work), &mut out);
    Json::Object(out)
}

/// A backup without its protections: every file of the archive written where its name says.
fn naive_restore(source: &str, root: &str) -> Json {
    let Ok(data) = fs::read(source) else { return json!(false) };
    let Ok(entries) = zip::entries(&data) else { return json!(false) };
    for entry in entries.iter().filter(|e| !e.name.ends_with('/')) {
        let target = Path::new(root).join(&entry.name);
        if let (Some(parent), Ok(content)) = (target.parent(), zip::read(&data, entry)) {
            let _ = fs::create_dir_all(parent);
            let _ = fs::write(&target, content);
        }
    }
    json!(true)
}

/// The made-up game folder `<work>/Spiele/LIVE` of a case with its files below `USER/Client/0` (the state of the
/// case); the game has been quiet for two hours. Gives the game folder and the `USER/Client/0` below it.
fn lay_game(work: &str, game: &Json) -> Result<(String, std::path::PathBuf), String> {
    let root = Path::new(work).join("Spiele").join("LIVE");
    let zero = root.join("USER").join("Client").join("0");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    if let Some(map) = game.as_object() {
        for (rel, text) in map {
            let target = rel.split('/').fold(zero.clone(), |path, part| path.join(part));
            fs::create_dir_all(target.parent().ok_or("kein Ordner")?).map_err(|e| e.to_string())?;
            fs::write(&target, text.as_str().unwrap_or("").as_bytes()).map_err(|e| e.to_string())?;
        }
    }
    let log = root.join("Game.log");
    fs::write(&log, "x").map_err(|e| e.to_string())?;
    let quiet = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    let file = fs::File::options().write(true).open(&log).map_err(|e| e.to_string())?;
    file.set_modified(quiet).map_err(|e| e.to_string())?;
    Ok((root.to_string_lossy().into_owned(), zero))
}

/// The base of a copy that the game files make (`<file>.scbpw-YYYYMMDD-HHMMSS` of 4.12, `<file>.versekit-…` here,
/// two in a second with `-02`), or `None` where the name is no copy.
fn copy_base(rel: &str) -> Option<&str> {
    let digits = |text: &str, n: usize| text.len() == n && text.bytes().all(|b| b.is_ascii_digit());
    let at = rel.rfind(".scbpw-").or_else(|| rel.rfind(".versekit-"))?;
    let stamp = rel[at..].split_once('-')?.1;
    let mut parts = stamp.split('-');
    let (day, time, more) = (parts.next()?, parts.next()?, parts.next());
    (digits(day, 8) && digits(time, 6) && more.is_none_or(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none())
    .then(|| &rel[..at])
}

/// Every file below `USER/Client/0`: relative path -> sha1; the copies of the two files of the active state by their
/// base (the copies of profiles are not compared: Rust makes them, 4.12 did not).
fn game_tree(zero: &Path) -> Json {
    fn walk(dir: &Path, zero: &Path, files: &mut JsonMap<String, Json>, copies: &mut JsonMap<String, Json>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, zero, files, copies);
                continue;
            }
            let rel = path.strip_prefix(zero).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            let digest = sha1_hex(&fs::read(&path).unwrap_or_default());
            if let Some(base) = copy_base(&rel) {
                if matches!(base, "Profiles/default/actionmaps.xml" | "Profiles/default/attributes.xml") {
                    let list = copies.entry(base.to_string()).or_insert_with(|| json!([]));
                    if let Some(list) = list.as_array_mut() {
                        list.push(json!(digest));
                    }
                }
                continue;
            }
            files.insert(rel, json!(digest));
        }
    }
    let (mut files, mut copies) = (JsonMap::new(), JsonMap::new());
    walk(zero, zero, &mut files, &mut copies);
    for list in copies.values_mut() {
        if let Some(list) = list.as_array_mut() {
            list.sort_by_key(|digest| digest.as_str().map(str::to_string));
        }
    }
    json!({ "files": files, "copies": copies })
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let text = fs::read(python_json).map_err(|e| format!("{python_json}: {e}"))?;
    let input: Json = serde_json::from_slice(&text).map_err(|e| format!("{python_json}: {e}"))?;
    let work = input["work"].as_str().ok_or("work fehlt")?.to_string();
    let root = input["root"].as_str().ok_or("root fehlt")?.to_string();
    let mut result = json!({ "work": work, "root": root });
    let host = Host::current();
    let plain = Paths::new(host.clone());

    // include
    let mut include = Vec::new();
    for item in input["include"].as_array().ok_or("include fehlt")? {
        let name = item[0].as_str().ok_or("Name fehlt")?;
        include.push(json!([name, gegenprobe || backup::include(name)]));
    }
    result["include"] = Json::Array(include);

    // safe
    let mut safe = Vec::new();
    for item in input["safe"].as_array().ok_or("safe fehlt")? {
        let case = &item["case"];
        let before = object(&expand(&case["before"], &root, &work))?;
        let content = match case["hex"].as_str() {
            Some(hex) => {
                (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)).collect()
            }
            None => case["content"].as_str().ok_or("content fehlt")?.as_bytes().to_vec(),
        };
        let got = if gegenprobe {
            Some((content.clone(), Vec::new()))
        } else {
            backup::safe_settings(&content, &before, &root, &plain)
        };
        let answer = match got {
            Some((bytes, off)) => json!([String::from_utf8_lossy(&bytes), off]),
            None => json!([null, []]),
        };
        safe.push(json!({ "case": case, "answer": answer }));
    }
    result["safe"] = Json::Array(safe);

    // check
    let mut check = Vec::new();
    for item in input["check"].as_array().ok_or("check fehlt")? {
        let zip_path = item["zip"].as_str().ok_or("zip fehlt")?;
        let answer = if gegenprobe {
            let count = fs::read(zip_path).ok().and_then(|d| zip::entries(&d).ok()).map_or(0, |e| e.len());
            json!([count > 0, count, ""])
        } else {
            match backup::check(zip_path) {
                Some(found) => json!([true, found.count, found.when]),
                None => json!([false, 0, ""]),
            }
        };
        check.push(json!({ "zip": zip_path, "answer": answer }));
    }
    result["check"] = Json::Array(check);

    // bindings
    let mut bindings = Vec::new();
    for item in input["bindings"].as_array().ok_or("bindings fehlt")? {
        let zip_path = item["zip"].as_str().ok_or("zip fehlt")?;
        let (active, profiles) = if gegenprobe { (false, Vec::new()) } else { backup::bindings_in_archive(zip_path) };
        bindings.push(json!({ "zip": zip_path, "answer": [active, profiles] }));
    }
    result["bindings"] = Json::Array(bindings);

    // restore
    let mut restore = Vec::new();
    for item in input["restore"].as_array().ok_or("restore fehlt")? {
        let case = &item["case"];
        let zip_path = item["zip"].as_str().ok_or("zip fehlt")?;
        fresh(&root, &work, &case["state"])?;
        let paths = Paths::for_copy(host.clone(), &work).ok_or("keine Kopie der Daten")?;
        let answer = if gegenprobe {
            naive_restore(zip_path, &root);
            json!({ "kind": "ok", "count": 0, "disarmed": [], "fallback": false, "tree": tree_of(&work) })
        } else {
            match backup::restore(&paths, zip_path, &[]) {
                Ok(done) => json!({
                    "kind": "ok", "count": done.count, "disarmed": done.disarmed,
                    "fallback": done.fallback.is_some(), "tree": tree_of(&work),
                }),
                Err(backup::RestoreError::Invalid) => json!({
                    "kind": "ungueltig", "count": 0, "disarmed": [], "fallback": false, "tree": tree_of(&work),
                }),
                Err(backup::RestoreError::Failed(_)) => json!({
                    "kind": "fehler", "count": 0, "disarmed": [], "fallback": false, "tree": tree_of(&work),
                }),
            }
        };
        restore.push(json!({ "case": case, "zip": zip_path, "answer": answer }));
    }
    result["restore"] = Json::Array(restore);

    // write
    let mut written = Vec::new();
    for item in input["write"].as_array().ok_or("write fehlt")? {
        let case = &item["case"];
        fresh(&root, &work, &case["state"])?;
        let paths = Paths::for_copy(host.clone(), &work).ok_or("keine Kopie der Daten")?;
        let target = Path::new(&work).join("meine.zip").to_string_lossy().into_owned();
        let outcome = if gegenprobe {
            // Without the rules of what comes along: every file of the data folder goes in.
            let mut every = Vec::new();
            collect(Path::new(&root), Path::new(&root), &mut every);
            every.sort();
            let mut archive = zip::Builder::new((2026, 1, 1, 0, 0, 0));
            for rel in &every {
                let content = fs::read(Path::new(&root).join(rel)).map_err(|e| e.to_string())?;
                archive.add(rel, &content).map_err(|e| e.to_string())?;
            }
            archive.add(backup::INFO_FILE, b"x").map_err(|e| e.to_string())?;
            fs::write(&target, archive.finish().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if every.is_empty() { Err(backup::WriteError::Empty) } else { Ok(every.len()) }
        } else {
            backup::write(&paths, &target, "5.0", &[])
        };
        let (kind, count) = match &outcome {
            Ok(count) => ("ok", *count),
            Err(backup::WriteError::Empty) => ("leer", 0),
            Err(backup::WriteError::Failed(_)) => ("fehler", 0),
        };
        let (mut members, mut contents) = (Vec::new(), JsonMap::new());
        if outcome.is_ok() {
            let data = fs::read(&target).map_err(|e| e.to_string())?;
            for entry in zip::entries(&data).map_err(|e| e.to_string())? {
                if entry.name == backup::INFO_FILE {
                    continue;
                }
                let content = zip::read(&data, &entry).map_err(|e| e.to_string())?;
                contents.insert(entry.name.clone(), json!(sha1_hex(&content)));
                members.push(entry.name);
            }
        }
        // The archive of the full case stays: Python reads it with a reader that is not ours.
        if outcome.is_ok() && case["name"] == "voll" {
            let _ = fs::copy(&target, Path::new(&work).join("zips").join("rust-voll.zip"));
        }
        let set_last = outcome.is_ok() && paths.setting(backup::LAST_SETTING).is_some();
        written.push(json!({ "case": case, "answer": {
            "kind": kind, "count": count, "members": members, "contents": contents, "set_last": set_last,
            "tmp_left": Path::new(&format!("{target}.teil")).exists(),
        } }));
    }
    result["write"] = Json::Array(written);

    // steuerung: the controls of a made-up game folder.
    let mut controls = Vec::new();
    // The game is not asked of the process list, and the log of the folder is the one that tells it is quiet.
    let mut game_host = host.clone();
    game_host.set_var(versekit_core::game_start::ENV_NO_PROCESS, "1");
    for item in input["steuerung"].as_array().ok_or("steuerung fehlt")? {
        let case = &item["case"];
        fresh(&root, &work, &json!({}))?;
        let (game, zero) = lay_game(&work, &case["game"])?;
        let mut host = game_host.clone();
        host.set_var(versekit_core::paths::ENV_INSTALL_DIR, &game);
        let paths = Paths::for_copy(host, &work).ok_or("keine Kopie der Daten")?;
        let answer = if case["kind"] == "sammeln" {
            let found = if gegenprobe { Vec::new() } else { backup::bindings(&paths, Some(&game)) };
            let files: Vec<Json> = found
                .iter()
                .map(|b| json!([b.name, sha1_hex(&fs::read(&b.full).unwrap_or_default())]))
                .collect();
            json!({ "files": files })
        } else {
            let zip_path = item["zip"].as_str().ok_or("zip fehlt")?;
            // Without its protection: the active binding always comes along.
            let with_active = gegenprobe || case["with_active"].as_bool().unwrap_or(false);
            let (kind, count) = match backup::restore_bindings(&paths, zip_path, with_active, Some(&game)) {
                Ok(done) => ("ok", Some(done.count)),
                Err(backup::BindingsError::Invalid) => ("ungueltig", Some(0)),
                Err(_) => ("fehler", Some(0)),
            };
            // A name that stands twice is written twice in 4.12 and once here: not compared there.
            let count = if case["name"].as_str().is_some_and(|n| n.starts_with("zweimal")) { None } else { count };
            json!({ "kind": kind, "count": count, "tree": game_tree(&zero) })
        };
        controls.push(json!({ "case": case, "zip": item["zip"], "answer": answer }));
    }
    result["steuerung"] = Json::Array(controls);

    // Nothing of this run may have been refused by a lock of writing: there is none set.
    debug_assert!(writeguard::take_denied().is_empty());
    fs::write(out, serde_json::to_string(&result).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "{} include, {} safe, {} check, {} restore, {} write, {} steuerung",
        result["include"].as_array().map_or(0, Vec::len),
        result["safe"].as_array().map_or(0, Vec::len),
        result["check"].as_array().map_or(0, Vec::len),
        result["restore"].as_array().map_or(0, Vec::len),
        result["write"].as_array().map_or(0, Vec::len),
        result["steuerung"].as_array().map_or(0, Vec::len),
    );
    Ok(())
}

/// Every file below `dir`, relative to `base`, with `/`.
fn collect(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn sha1_hex(data: &[u8]) -> String {
    versekit_core::text::sha1_hex(data)
}
