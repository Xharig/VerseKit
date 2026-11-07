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

//! Rust side of the parity check for the error report
//! (`tools/parity_report.py`): builds the reports from the recorded facts and
//! the same files, gathers the lines from the same data, runs the helpers and
//! the request pieces, and sends to a made-up server on this computer.
//!
//! ⚠ Nothing leaves this computer: sending goes to `127.0.0.1` only, and the
//! transport refuses every other host (`SC_BP_NO_NET`).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{Value, json};
use versekit_core::basetool::http::HttpTransport;
use versekit_core::crash;
use versekit_core::error_log::ErrorLog;
use versekit_core::formats::{self, LocalTime};
use versekit_core::language::{Lang, t_in};
use versekit_core::legacy;
use versekit_core::paths::{Host, Paths};
use versekit_core::redact::Redactor;
use versekit_core::report::{self, Block, Entry, Facts};
use versekit_core::report_facts as facts_of;
use versekit_core::report_send::{self as send, Attachment, ImageKind, Target};
use versekit_core::trail;

/// Which of the recorded facts is which line: (block, text key).
/// Index 0 is the overview and no line; 3 is joined with the Python version.
const LINES: [(usize, &str); 28] = [
    (0, ""),
    (1, "b_system"),
    (1, "b_verpackung"),
    (1, "b_python"),
    (1, "b_bildschirm"),
    (1, "b_overlay"),
    (2, "b_spiel"),
    (2, "b_gamelog"),
    (2, "b_sicherungen"),
    (2, "b_starter"),
    (2, "b_spielsprache"),
    (2, "b_inj"),
    (2, "b_inj_datei"),
    (3, "b_bestand"),
    (3, "b_unbekannt"),
    (3, "b_merkliste"),
    (3, "b_katalog"),
    (3, "b_historie"),
    (3, "b_lager"),
    (3, "b_rezepte"),
    (3, "b_bergbaudaten"),
    (3, "b_scanner"),
    (3, "b_aafk"),
    (4, "b_ordner"),
    (4, "b_einstellungen"),
    (4, "b_dateischutz"),
    (4, "b_schreibfehler"),
    (4, "b_versionsabgleich"),
];

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value.get(name).and_then(Value::as_str).ok_or_else(|| format!("{name} fehlt"))
}

fn list<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    value.get(name).and_then(Value::as_array).ok_or_else(|| format!("{name} fehlt"))
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    (0..text.len() / 2).map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())).collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn lang_of(code: &str) -> Lang {
    Lang::from_code(code).unwrap_or(Lang::De)
}

fn paths_in(folder: &str) -> Paths {
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, folder);
    Paths::new(host)
}

// ---------------------------------------------------------------- Part A

fn assembly(case: &Value, sabotage: bool) -> Result<String, String> {
    let folder = text(case, "rust_folder")?;
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    for (name, hex) in case["files"].as_object().into_iter().flatten() {
        let bytes = from_hex(hex.as_str().unwrap_or_default())?;
        fs::write(std::path::Path::new(folder).join(name), bytes).map_err(|e| e.to_string())?;
    }
    if let Some(seconds) = case["crash_mtime"].as_f64() {
        let file = fs::File::options()
            .write(true)
            .open(std::path::Path::new(folder).join(crash::PREVIOUS))
            .map_err(|e| e.to_string())?;
        file.set_modified(UNIX_EPOCH + Duration::from_secs_f64(seconds)).map_err(|e| e.to_string())?;
    }
    let paths = paths_in(folder);
    let lang = lang_of(text(case, "lang")?);
    let recorded = list(case, "recorded")?;
    let has_basetool = case["has_basetool"].as_bool().unwrap_or(false);
    let value_of = |i: usize| recorded.get(i).and_then(Value::as_str).map(str::to_string);

    let mut blocks: Vec<Vec<Entry>> = vec![Vec::new(); 4];
    for (i, (block, key)) in LINES.iter().enumerate().skip(1) {
        let mut value = value_of(i);
        // The Python version in front of the Tk version.
        if *key == "b_python" {
            value = value.map(|tk| format!("3.14.7 / {tk}"));
        }
        // Lines that stand only when they have something to say.
        let optional = matches!(*key, "b_unbekannt" | "b_schreibfehler");
        if optional && value.as_deref().is_none_or(|v| v.is_empty() || v == "—") {
            continue;
        }
        blocks[block - 1].push(Entry { label: t_in(lang, key), value });
    }
    if has_basetool {
        blocks[3].push(Entry { label: t_in(lang, "b_basetool"), value: value_of(28) });
    }

    let (trail_start, trail_pages) = trail::split_trail(&paths);
    let log = ErrorLog::new(&paths, "");
    let count = case["error_count"].as_u64().unwrap_or(8) as usize;
    let mut errors = log.latest(count);
    // The counter-check turns the list round: the parts of the report keep the
    // numbers of the entries, so swapping two of them would go unseen.
    if sabotage {
        errors.reverse();
    }
    let datum = t_in(lang, "b_datum");
    let now = formats::strftime(&datum, &LocalTime { year: 2026, month: 10, day: 21, hour: 9, minute: 5, second: 0 });
    let facts = Facts {
        lang,
        version: text(case, "version")?.to_string(),
        now,
        reporter: text(case, "reporter")?.to_string(),
        message: text(case, "message")?.to_string(),
        blocks: ["bf_h_system", "bf_h_spiel", "bf_h_bestand", "bf_h_ordner"]
            .iter()
            .zip(blocks)
            .map(|(title, entries)| Block::new(t_in(lang, title), entries))
            .collect(),
        trail_start,
        trail_pages,
        update_lines: list(case, "update")?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        crash: crash::last_crash(&paths),
        crash_at: crash::crash_time(&paths),
        crash_version: crash::crash_version(&paths),
        // 4.12 is one process: there is no other run to compare.
        others: Vec::new(),
        errors,
        error_total: log.count(),
    };
    Ok(report::build(&facts, &Redactor::new(&paths)))
}

/// The start trail as Rust writes it, step for step with the times of Python's run.
fn trail_written(case: &Value, sabotage: bool) -> Result<Option<Value>, String> {
    let Some(steps) = case["trail_steps"].as_array() else { return Ok(None) };
    let folder = std::path::Path::new(text(case, "rust_folder")?).join("geschrieben");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let paths = paths_in(&folder.to_string_lossy());
    let trail = versekit_core::trail::Trail::new();
    for (index, step) in steps.iter().enumerate() {
        // The counter-check writes the clock one second late.
        let second = index + usize::from(sabotage);
        trail.step_at(&paths, step.as_str().unwrap_or_default(), &format!("10:{:02}:{:02}", second / 60, second % 60));
    }
    let bytes = fs::read(folder.join(versekit_core::trail::FILE)).map_err(|e| e.to_string())?;
    Ok(Some(json!(to_hex(&bytes))))
}

/// What the start does with the crash files of the last run: the files
/// that stand afterwards, by name.
fn crash_installed(case: &Value, sabotage: bool) -> Result<Value, String> {
    let folder = text(case, "rust_folder")?;
    let paths = paths_in(folder);
    // The counter-check starts with the version of the run before.
    crash::install(&paths, if sabotage { "4.9.9" } else { text(case, "version")? });
    let mut names: Vec<String> = fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut out = serde_json::Map::new();
    for name in names {
        let bytes = fs::read(std::path::Path::new(folder).join(&name)).map_err(|e| e.to_string())?;
        out.insert(name, json!(to_hex(&bytes)));
    }
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------- Part B

fn normalized(value: Option<String>, folder: &str) -> Value {
    match value.filter(|v| !v.is_empty()) {
        None => Value::Null,
        Some(v) => json!(v.replace(folder, "<ORDNER>").replace(&folder.replace('\\', "/"), "<ORDNER>")),
    }
}

fn lines(case: &Value, sabotage: bool) -> Result<Value, String> {
    let folder = text(case, "rust_folder")?;
    let paths = paths_in(folder);
    let lang = Lang::De;
    let mut log = facts_of::log_line(&paths, lang);
    if sabotage {
        log = log.replace(" · ", ", ");
    }
    let own = match facts_of::own_reporter(&paths) {
        None => "None".to_string(),
        Some(name) => format!("Name:{name}"),
    };
    let values = [
        ("sicherungen", Some(log)),
        ("bestand", facts_of::collection_line(&paths, lang)),
        ("unbekannt", facts_of::unknown_blueprints(&paths, lang)),
        ("spielsprache", facts_of::game_language(&paths, lang)),
        ("merkliste", facts_of::watchlist_line(&paths, lang)),
        ("katalog", facts_of::catalog_version(&paths)),
        ("historie", facts_of::patch_history_line(&paths)),
        ("lager", facts_of::storage_line(&paths, lang)),
        ("rezepte", Some(facts_of::recipe_line(&paths, lang))),
        ("bergbaudaten", Some(facts_of::mining_line(&paths, lang))),
        // The look at GitHub of this run is left out on both sides: version 5 cannot know it in the main window.
        (
            "versionsabgleich",
            facts_of::version_check_line(&paths, lang).split(" · dieser Lauf:").next().map(str::to_string),
        ),
        ("ordner", facts_of::folder_line(&paths, lang)),
        ("einstellungen", facts_of::settings_line(&paths, lang)),
        ("dateischutz", Some(facts_of::file_flags_line(&paths, lang))),
        ("melder", Some(facts_of::reporter_name(&paths))),
        ("eigener_melder", Some(own)),
    ];
    let mut out = serde_json::Map::new();
    for (key, value) in values {
        out.insert(key.to_string(), normalized(value, folder));
    }
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------- Part C

fn helpers(input: &Value, sabotage: bool) -> Result<Value, String> {
    let redactor = Redactor::new(&Paths::new(Host::current()));
    let mut wraps = Vec::new();
    for case in list(input, "wraps")? {
        let width = case["width"].as_u64().unwrap_or(76) as usize;
        let width = if sabotage && width == 76 { 75 } else { width };
        wraps.push(json!(report::wrap(text(case, "text")?, width)));
    }
    let mut denses = Vec::new();
    for case in list(input, "denses")? {
        let entries: Vec<String> =
            list(case, "entries")?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let mut dense = report::dense(&entries);
        if sabotage {
            dense = dense.into_iter().map(|l| l.replace('×', "x")).collect();
        }
        denses.push(json!(dense));
    }
    let mut briefs = Vec::new();
    for case in list(input, "briefs")? {
        let lines: Vec<String> = list(case, "lines")?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let mut brief = report::crash_brief(&lines, case["limit"].as_u64().unwrap_or(14) as usize);
        if sabotage {
            brief.reverse();
        }
        briefs.push(json!(brief));
    }
    let mut frames = Vec::new();
    for case in list(input, "frames")? {
        let mut found = report::trace_frames(text(case, "trace")?, &redactor);
        if sabotage {
            found.truncate(1);
        }
        frames.push(json!(found));
    }
    Ok(json!({ "wraps": wraps, "denses": denses, "briefs": briefs, "frames": frames }))
}

// ---------------------------------------------------------------- Part D

/// A made-up server on this computer: answers each request with the next status
/// and records what came. Ends after the last answer, so the port is closed.
fn serve(statuses: Vec<u16>) -> Result<(String, std::thread::JoinHandle<Vec<Value>>), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for status in statuses {
            let Ok((mut stream, _)) = listener.accept() else { break };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut data = Vec::new();
            let mut chunk = [0u8; 8192];
            let mut body_start = None;
            let mut wanted = 0usize;
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..read]);
                if body_start.is_none()
                    && let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n")
                {
                    body_start = Some(at + 4);
                    let head = String::from_utf8_lossy(&data[..at]).to_lowercase();
                    wanted = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                        .unwrap_or(0);
                }
                if body_start.is_some_and(|s| data.len() >= s + wanted) {
                    break;
                }
            }
            let start = body_start.unwrap_or(data.len());
            let head = String::from_utf8_lossy(&data[..start.saturating_sub(4)]).into_owned();
            let body = &data[start..];
            let path = head.split_whitespace().nth(1).unwrap_or_default().to_string();
            let content_type = head
                .lines()
                .find_map(|l| l.to_lowercase().starts_with("content-type:").then(|| l[13..].trim().to_string()))
                .unwrap_or_default();
            let lines: Vec<&[u8]> = body.split(|b| *b == b'\n').collect();
            let first = lines.get(3).map(|l| String::from_utf8_lossy(l).trim_end_matches('\r').to_string());
            seen.push(json!({
                "path": path,
                "type_prefix": content_type.split('=').next().unwrap_or_default(),
                "size": body.len(),
                "has_name": body.windows(18).any(|w| w == b"filename=\"bericht-"),
                "head": first.unwrap_or_default(),
            }));
            let _ = stream
                .write_all(format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
            let _ = stream.flush();
        }
        seen
    });
    Ok((format!("http://127.0.0.1:{port}/bericht"), handle))
}

fn outcome(result: Result<(), send::SendFailure>) -> Value {
    match result {
        Ok(()) => json!({ "ok": true, "reason": "" }),
        Err(failure) => json!({ "ok": false, "reason": failure.text(Lang::De) }),
    }
}

fn request(input: &Value, sabotage: bool) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    // multipart
    let mut multi = Vec::new();
    for case in list(input, "multipart")? {
        let mut attachments: Vec<Attachment> = list(case, "attachments")?
            .iter()
            .map(|a| {
                Ok(Attachment {
                    name: a[0].as_str().unwrap_or_default().to_string(),
                    data: from_hex(a[1].as_str().unwrap_or_default())?,
                    mime: a[2].as_str().unwrap_or_default().to_string(),
                })
            })
            .collect::<Result<_, String>>()?;
        if sabotage {
            attachments.reverse();
        }
        let body = send::multipart(
            text(case, "limit")?,
            text(case, "head")?,
            text(case, "name")?,
            text(case, "text")?,
            &attachments,
        );
        multi.push(json!({ "expected": to_hex(&body) }));
    }
    out.insert("multipart".into(), json!(multi));
    // issue addresses
    let mut issues = Vec::new();
    for case in list(input, "issue")? {
        let lang = lang_of(text(case, "lang")?);
        let template = send::template_for(lang);
        let mut url = send::issue_url(text(case, "text")?, text(case, "title")?, template, lang);
        if sabotage {
            url = url.replacen("template=", "vorlage=", 1);
        }
        issues.push(json!({ "expected": url, "template": template }));
    }
    out.insert("issue".into(), json!(issues));
    // images
    let mut images = Vec::new();
    for case in list(input, "images")? {
        let data = from_hex(text(case, "data")?)?;
        let kind = send::image_kind(&data);
        let mut clean = kind.and_then(|k| send::strip_metadata(&data, k)).map(|c| to_hex(&c));
        if sabotage && kind == Some(ImageKind::Jpg) {
            clean = Some(to_hex(&data));
        }
        images.push(json!({ "kind": kind.map(ImageKind::extension), "clean": clean }));
    }
    out.insert("images".into(), json!(images));
    let mut loads = Vec::new();
    for case in list(input, "loads")? {
        let result = match send::load_image(text(case, "path")?) {
            Ok((data, kind)) => json!([to_hex(&data), kind.extension(), null]),
            Err(key) => json!([null, null, key]),
        };
        loads.push(json!({ "result": result }));
    }
    out.insert("loads".into(), json!(loads));
    let files = send::image_files(&[
        (b"a".to_vec(), ImageKind::Png),
        (b"b".to_vec(), ImageKind::Jpg),
        (b"a".to_vec(), ImageKind::Png),
        (b"b".to_vec(), ImageKind::Jpg),
        (b"a".to_vec(), ImageKind::Png),
        (b"b".to_vec(), ImageKind::Jpg),
    ]);
    out.insert(
        "image_files".into(),
        json!(files.iter().map(|f| json!([f.name, to_hex(&f.data), f.mime])).collect::<Vec<_>>()),
    );
    // targets
    let mut targets = Vec::new();
    for case in list(input, "targets")? {
        let mut host = Host::new(versekit_core::pypath::Flavor::native(), &[], "/");
        if let Some(env) = case["env"].as_str() {
            host.set_var(legacy::ENV_REPORT_TARGET, env);
        }
        let target = Target::from_host(&host);
        let available = target.available() != (sabotage && case["env"].as_str() == Some("aus"));
        targets.push(json!({ "target": target.address(), "available": available }));
    }
    out.insert("targets".into(), json!(targets));
    // sending, to the made-up server only
    let (address, server) = serve(vec![200, 204, 429, 413, 500, 404, 302])?;
    let mut host = Host::current();
    host.set_var(legacy::ENV_NO_NET, "1");
    let transport = HttpTransport::new("", &host).map_err(|e| e.code)?;
    let target = Target::local(&address);
    let image = [Attachment { name: "bild-1.png".into(), data: b"\x89PNG".to_vec(), mime: "image/png".into() }];
    let mut submits = Vec::new();
    for _ in 0..7 {
        submits.push(outcome(send::submit(
            &transport,
            &target,
            "Verse-Kit 5 Bericht ä",
            "5.0.0",
            "2026-10-21-0905",
            &image,
        )));
    }
    let seen = server.join().map_err(|_| "Server abgestürzt".to_string())?;
    submits.push(outcome(send::submit(&transport, &target, "Verse-Kit 5 Bericht", "5.0.0", "2026-10-21-0905", &[])));
    let big: Vec<Attachment> = (0..5)
        .map(|i| Attachment { name: format!("bild-{i}.png"), data: vec![0; 2 * 1024 * 1024], mime: "image/png".into() })
        .collect();
    submits.push(outcome(send::submit(&transport, &target, "Verse-Kit 5", "5.0.0", "2026-10-21-0905", &big)));
    submits.push(outcome(send::submit(&transport, &Target::off(), "x", "5", "2026-10-21-0905", &[])));
    if sabotage {
        submits[2] = json!({ "ok": true, "reason": "" });
    }
    out.insert("submits".into(), json!(submits));
    out.insert("seen".into(), json!(seen));
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------- Time

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    (times[times.len() / 2] * 10.0).round() / 10.0
}

/// How long gathering the facts and setting the report together take on the
/// real data (median of five runs after one warm-up run).
fn timing(input: &Value) -> Result<Value, String> {
    let case = list(input, "lines")?.iter().find(|c| c["case"] == "B-real").ok_or("B-real fehlt")?;
    let paths = paths_in(text(case, "rust_folder")?);
    let env = facts_of::Environment { version: "5.0.0".into(), ..Default::default() };
    let make = || {
        let request =
            facts_of::Request { paths: &paths, lang: Lang::De, env: &env, reporter: None, message: "", error_count: 8 };
        let started = std::time::Instant::now();
        let facts = facts_of::collect(&request);
        let collected = started.elapsed().as_secs_f64() * 1000.0;
        let redactor = Redactor::new(&paths);
        let built = std::time::Instant::now();
        let text = report::build(&facts, &redactor);
        (collected, built.elapsed().as_secs_f64() * 1000.0, text.len())
    };
    // The first run reads the catalog; the later ones find it in memory.
    let first = make();
    let first_ms = ((first.0 + first.1) * 10.0).round() / 10.0;
    // Which fact takes how long (one run), to see where the time goes.
    let lang = Lang::De;
    let clock = |name: &str, f: &dyn Fn()| {
        let started = std::time::Instant::now();
        f();
        eprintln!("  {name}: {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
    };
    clock("Sicherungen", &|| drop(facts_of::log_line(&paths, lang)));
    clock("Bestand", &|| drop(facts_of::collection_line(&paths, lang)));
    clock("Nicht im Katalog", &|| drop(facts_of::unknown_blueprints(&paths, lang)));
    clock("Spielsprache", &|| drop(facts_of::game_language(&paths, lang)));
    clock("Merkliste", &|| drop(facts_of::watchlist_line(&paths, lang)));
    clock("Katalogstand", &|| drop(facts_of::catalog_version(&paths)));
    clock("Lager", &|| drop(facts_of::storage_line(&paths, lang)));
    clock("Ordner", &|| drop(facts_of::folder_line(&paths, lang)));
    clock("Einstellungen", &|| drop(facts_of::settings_line(&paths, lang)));
    clock("Dateischutz", &|| drop(facts_of::file_flags_line(&paths, lang)));
    let runs: Vec<(f64, f64, usize)> = (0..5).map(|_| make()).collect();
    let collect = median(runs.iter().map(|r| r.0).collect());
    let build = median(runs.iter().map(|r| r.1).collect());
    let total = median(runs.iter().map(|r| r.0 + r.1).collect());
    Ok(json!({ "first_ms": first_ms, "collect_ms": collect, "build_ms": build, "total_ms": total, "bytes": runs[0].2 }))
}

// ---------------------------------------------------------------- Run

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input: Value =
        serde_json::from_slice(&fs::read(python_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut assembled = serde_json::Map::new();
    for (index, case) in list(&input, "assembly")?.iter().enumerate() {
        let built = assembly(case, gegenprobe && index == 0)?;
        assembled.insert(text(case, "case")?.to_string(), json!(built));
    }
    let mut written = serde_json::Map::new();
    for case in list(&input, "assembly")? {
        if let Some(hex) = trail_written(case, gegenprobe)? {
            written.insert(text(case, "case")?.to_string(), hex);
        }
    }
    let mut installed = serde_json::Map::new();
    for case in list(&input, "crash_install")? {
        installed.insert(text(case, "case")?.to_string(), crash_installed(case, gegenprobe)?);
    }
    let mut collected = serde_json::Map::new();
    for (index, case) in list(&input, "lines")?.iter().enumerate() {
        collected.insert(text(case, "case")?.to_string(), lines(case, gegenprobe && index == 0)?);
    }
    let result = json!({
        "assembly": assembled,
        "trail_written": written,
        "crash_install": installed,
        "lines": collected,
        "helpers": helpers(&input["helpers"], gegenprobe)?,
        "request": request(&input["request"], gegenprobe)?,
        "timing": timing(&input)?,
    });
    fs::write(out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
