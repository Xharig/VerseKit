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

//! Rust side of the check of the bridge to scmdb.net (`tools/parity_scmdb.py`): the same sections of the log
//! through the reader, and the same session over the own server on a free port of `127.0.0.1`, with the data
//! folder, `APPDATA`, `LOCALAPPDATA` and the game folder in a scratch folder.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::{Value, json};
use versekit_core::collection::Known;
use versekit_core::jsonio;
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{self, Sources};
use versekit_core::pypath::Flavor;
use versekit_core::scmdb_bridge::{self as bridge, Bridge, Reader, Setup};
use versekit_core::testdir;

fn read(path: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// A jsonio value as serde_json, through the text.
fn to_serde(value: &jsonio::Value) -> Value {
    serde_json::from_str(&jsonio::dumps(value, None, false)).unwrap_or(Value::Null)
}

/// One request as `urllib` of the Python side sends it; the fields the Python side kept of the answer.
fn ask(port: u16, path: &str, heads: &[(String, String)]) -> Result<Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut request = format!("GET {path} HTTP/1.1\r\nAccept-Encoding: identity\r\n");
    if !heads.iter().any(|(n, _)| n == "Host") {
        request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (name, value) in heads {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut answer = Vec::new();
    let mut buffer = [0u8; 4096];
    // The head first; a body as long as Content-Length says.
    let (head, mut body) = loop {
        let n = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Antwort ohne Kopf".into());
        }
        answer.extend_from_slice(&buffer[..n]);
        if let Some(at) = answer.windows(4).position(|w| w == b"\r\n\r\n") {
            break (String::from_utf8_lossy(&answer[..at]).into_owned(), answer[at + 4..].to_vec());
        }
    };
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).unwrap_or(0);
    let heads: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':')).map(|(n, v)| (n.trim().to_string(), v.trim().to_string())).collect();
    let head_of = |name: &str| heads.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    let length: usize = head_of("Content-Length").and_then(|l| l.parse().ok()).unwrap_or(0);
    while body.len() < length {
        let n = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..n]);
    }
    if status >= 400 {
        return Ok(json!({"status": status, "body": "", "cors": head_of("Access-Control-Allow-Origin"),
                         "pna": null, "vary": null, "type": null}));
    }
    Ok(json!({
        "status": status, "body": String::from_utf8_lossy(&body[..length.min(body.len())]),
        "cors": head_of("Access-Control-Allow-Origin"), "pna": head_of("Access-Control-Allow-Private-Network"),
        "vary": head_of("Vary"), "type": head_of("Content-Type"),
    }))
}

/// A listener of the stream in a thread of its own: everything it gets until the connection closes, without the
/// head of the answer and without heartbeats.
fn listen(port: u16) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else { return String::new() };
        let request = format!(
            "GET /events HTTP/1.1\r\nAccept-Encoding: identity\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(request.as_bytes());
        let mut all = Vec::new();
        let _ = stream.read_to_end(&mut all);
        let text = String::from_utf8_lossy(&all).into_owned();
        let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
        body.replace(": heartbeat\n\n", "")
    })
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = read(python_json)?;
    let work = testdir::fresh("parity-scmdb");
    let text = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
    let (home, app, local) = (text(work.join("daten")), text(work.join("appdata")), text(work.join("local")));
    let folder = text(work.join("StarCitizen").join("LIVE"));
    for dir in [&home, &app, &local, &folder] {
        fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
    }
    let log = format!("{folder}/Game.log");
    fs::write(&log, "").map_err(|e| e.to_string())?;
    let vars = [
        (versekit_core::legacy::ENV_HOME, home.as_str()),
        ("APPDATA", app.as_str()),
        ("LOCALAPPDATA", local.as_str()),
        (versekit_core::paths::ENV_INSTALL_DIR, folder.as_str()),
    ];
    let host = Host::new(Flavor::native(), &vars, &home);
    let paths = Paths::new(host.clone());
    if paths.game_folder().as_deref() != Some(folder.as_str()) {
        return Err("Spielordner ist nicht der Wegwerf-Ordner: Abbruch".into());
    }
    let known = || {
        Known::new(
            input["katalog"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| (e[0].as_str().unwrap_or_default().to_string(), e[1].as_str().map(str::to_string))),
        )
    };
    let pattern = phrases::pattern(&strings(&input["phrasen"]));
    let mut result = json!({});

    let mut readers = Vec::new();
    for case in input["leser"].as_array().into_iter().flatten() {
        let mut reader = Reader::new(
            case["kanal"].as_str().unwrap_or_default(),
            bridge::namer(&paths, known()),
            pattern.clone(),
            Sources::default(),
        );
        let (mut sent, mut unmatched) = (Vec::new(), Vec::new());
        for line in strings(&case["zeilen"]) {
            sent.extend(reader.feed(&line).iter().map(bridge::message));
            unmatched.append(&mut reader.unmatched);
        }
        readers.push(json!({
            "kanal": case["kanal"], "zeilen": case["zeilen"], "gesendet": sent, "nicht_zugeordnet": unmatched,
            "stand": [bridge::message(&reader.snapshot())],
        }));
    }
    result["leser"] = Value::Array(readers);
    result["kanaele"] = Value::Array(
        input["kanaele"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let f = c[0].as_str().unwrap_or_default();
                json!([f, bridge::channel_of(Some(f))])
            })
            .collect(),
    );
    result["zeiten"] = Value::Array(
        input["zeiten"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let l = c[0].as_str().unwrap_or_default();
                json!([l, to_serde(&jsonio::Value::Float(bridge::line_time(l)))])
            })
            .collect(),
    );
    result["hosts"] = Value::Array(
        input["hosts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let h = c[0].as_str().unwrap_or_default();
                json!([h, bridge::host_allowed(Some(h), 23456)])
            })
            .collect(),
    );

    // The session over the own server.
    let session = &input["sitzung"];
    let lines = strings(&session["zeilen"]);
    let port =
        std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
    let mut session_host = host.clone();
    session_host.set_var(versekit_core::legacy::ENV_SCMDB_PORT, &port.to_string());
    let version = session["ping"][0]["body"]
        .as_str()
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|b| b["clientVersion"].as_str().map(str::to_string))
        .unwrap_or_default();
    let mut server = Bridge::new();
    let mut listeners = Vec::new();
    for step in session["schritte"].as_array().into_iter().flatten() {
        let kind = step[0].as_str().unwrap_or_default();
        if kind == "connect" {
            listeners.push(listen(port));
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }
        let (a, b) = (step[1].as_u64().unwrap_or(0) as usize, step[2].as_u64().unwrap_or(0) as usize);
        let part: String = lines[a.min(lines.len())..b.min(lines.len())].iter().map(|l| format!("{l}\n")).collect();
        if kind == "append" {
            let mut file = fs::OpenOptions::new().append(true).open(&log).map_err(|e| e.to_string())?;
            file.write_all(part.as_bytes()).map_err(|e| e.to_string())?;
        } else {
            fs::write(&log, part).map_err(|e| e.to_string())?;
        }
        if server.status().state == bridge::State::Off {
            let started = server.start(Setup {
                host: session_host.clone(),
                version: version.clone(),
                namer: Box::new({
                    let entries = input["katalog"].clone();
                    move |paths: &Paths| {
                        let known = Known::new(entries.as_array().into_iter().flatten().map(|e| {
                            (e[0].as_str().unwrap_or_default().to_string(), e[1].as_str().map(str::to_string))
                        }));
                        bridge::namer(paths, known)
                    }
                }),
                poll: Duration::from_millis(100),
            });
            if !started {
                return Err("Server startet nicht".into());
            }
        }
        std::thread::sleep(Duration::from_millis(800));
    }
    let heads = |origin: Option<&str>| origin.map(|o| vec![("Origin".to_string(), o.to_string())]).unwrap_or_default();
    let ping: Vec<Value> =
        [Some("https://scmdb.net"), Some("https://www.scmdb.net"), Some("https://boese.example"), None]
            .iter()
            .map(|o| ask(port, "/ping", &heads(*o)))
            .collect::<Result<_, _>>()?;
    let rebound = ask(port, "/events", &[("Host".to_string(), format!("boese.example:{port}"))])?["status"].clone();
    let missing = ask(port, "/nichts", &[])?["status"].clone();
    std::thread::sleep(Duration::from_millis(500));
    server.stop();
    let streams: Vec<String> = listeners.into_iter().map(|l| l.join().unwrap_or_default()).collect();
    let mut session_out = session.clone();
    session_out["ping"] = json!(ping);
    session_out["umgelenkt"] = rebound;
    session_out["fehlt"] = missing;
    session_out["stroeme"] = json!(streams);
    result["sitzung"] = session_out;

    if gegenprobe {
        // One message of the reader, one name not sent, one channel, one time, one host, the body of the ping and
        // one byte of a stream changed: each must be reported.
        result["leser"][0]["gesendet"][0] = json!("data: {}\n\n");
        result["leser"][1]["nicht_zugeordnet"] = json!(["Banane"]);
        result["kanaele"][0][1] = json!("PTU");
        result["zeiten"][0][1] = json!(1.0);
        result["hosts"][0][1] = json!(false);
        result["sitzung"]["ping"][0]["cors"] = json!("*");
        let stream = result["sitzung"]["stroeme"][0].as_str().unwrap_or_default().replacen("\"LIVE\"", "\"PTU\"", 1);
        result["sitzung"]["stroeme"][0] = json!(stream);
    }
    fs::write(out, serde_json::to_string(&result).map_err(|e| e.to_string())?).map_err(|e| format!("{out}: {e}"))?;
    let _ = fs::remove_dir_all(&work);
    println!("geschrieben {out}");
    Ok(())
}
