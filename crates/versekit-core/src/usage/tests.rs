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

//! The daily report and the page counters, against a made-up receiver on the
//! own computer that takes what `tools/nutzung-worker/worker.js` (`check`)
//! takes, with the lists of `pages.js`. Nothing here reaches the network.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use super::*;
use crate::basetool::http::HttpTransport;
use crate::paths::Host;
use crate::pypath::Flavor;

// ------------------------------------------------------------ set-up

struct Setup {
    root: std::path::PathBuf,
    usage: PageUsage,
}

impl Setup {
    fn new(name: &str, vars: &[(&str, &str)], version: &str) -> Setup {
        let root = std::env::temp_dir().join(format!("versekit-usage-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let data = root.to_string_lossy().into_owned();
        let mut all = vec![("HOME", data.as_str()), ("SC_BP_HOME", data.as_str())];
        all.extend_from_slice(vars);
        let host = Host::new(Flavor::native(), &all, &data);
        Setup { usage: PageUsage::new(Paths::new(host), version), root }
    }

    fn paths(&self) -> &Paths {
        &self.usage.paths
    }

    fn file(&self) -> std::path::PathBuf {
        self.root.join(FILE)
    }

    fn file_text(&self) -> Option<String> {
        std::fs::read_to_string(self.file()).ok()
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn obj(text: &str) -> Map {
    match jsonio::loads(text).unwrap() {
        Value::Object(map) => map,
        other => panic!("no object: {other:?}"),
    }
}

fn plain_facts() -> Facts {
    Facts {
        os: "windows",
        ui: "de".into(),
        game: "de".into(),
        rc: false,
        mods: vec!["basetool".into(), "schiffe".into()],
        overlay: "immer".into(),
        autostart: true,
        update: true,
        view: "reiter".into(),
        tiles_off: vec!["spielzeit".into()],
    }
}

// ------------------------------------------------------------ the receiver

/// The lists the receiver takes, read from its source.
struct Lists {
    pages: BTreeSet<String>,
    routes: BTreeSet<String>,
    actions: BTreeSet<String>,
    tiles: BTreeSet<String>,
    numbers: Vec<(String, i64)>,
}

/// The keys of `export const NAME = { … };` in `pages.js`, strings and comments left out.
fn worker_lists() -> Lists {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/nutzung-worker/pages.js");
    let source = std::fs::read_to_string(file).unwrap();
    let mut plain = String::new();
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or_default();
        let mut quoted = false;
        for c in line.chars() {
            if c == '\'' {
                quoted = !quoted;
            } else if !quoted {
                plain.push(c);
            }
        }
        plain.push('\n');
    }
    let block = |name: &str| -> BTreeSet<String> {
        let start = plain.find(&format!("export const {name} = {{")).unwrap_or_else(|| panic!("{name} missing"));
        let body = &plain[start..];
        let body = &body[body.find('{').unwrap() + 1..body.find("};").unwrap()];
        let mut keys = BTreeSet::new();
        let mut word = String::new();
        for c in body.chars() {
            if c.is_ascii_lowercase() || c == '_' || (c.is_ascii_digit() && !word.is_empty()) {
                word.push(c);
            } else {
                if c == ':' && !word.is_empty() {
                    keys.insert(word.clone());
                }
                if !c.is_whitespace() || c == '\n' {
                    word.clear();
                }
            }
        }
        keys
    };
    let numbers = ["MAX_CLICKS", "MAX_COUNT", "MAX_MISS_PAIRS"]
        .iter()
        .map(|name| {
            let at = plain.find(&format!("export const {name} = ")).unwrap() + name.len() + 16;
            let digits: String = plain[at..].chars().take_while(char::is_ascii_digit).collect();
            (name.to_string(), digits.parse().unwrap())
        })
        .collect();
    Lists { pages: block("PAGES"), routes: block("ROUTES"), actions: block("ACTIONS"), tiles: block("TILES"), numbers }
}

const WORKER_FIELDS: [&str; 16] = [
    "actions",
    "autostart",
    "clicks",
    "entry",
    "game",
    "misses",
    "mods",
    "os",
    "overlay",
    "pages",
    "rc",
    "tiles_off",
    "ui",
    "update",
    "v",
    "view",
];
const MAX_BYTES: usize = 24576;

/// `VERSION_RE` of `worker.js`, `^\d{1,3}\.\d{1,3}\.\d{1,3}(?:-rc\d{1,4})?$`, written out on its own
/// so that the receiver does not take the program's own check.
fn worker_version(v: &str) -> bool {
    let digits = |s: &str, most: usize| (1..=most).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    let (core, rc) = match v.find("-rc") {
        Some(at) => (&v[..at], Some(&v[at + 3..])),
        None => (v, None),
    };
    let mut parts = core.split('.');
    let three = (0..3).all(|_| parts.next().is_some_and(|p| digits(p, 3))) && parts.next().is_none();
    three && rc.is_none_or(|rc| digits(rc, 4))
}

/// `worker.js` `check` and `navigation`: `Ok` or the status and text of the refusal.
fn check(text: &str, lists: &Lists) -> Result<(), (u16, String)> {
    let bad = |text: &str| Err((400, text.to_string()));
    if text.len() > MAX_BYTES {
        return Err((413, "zu gross".into()));
    }
    let Ok(Value::Object(d)) = jsonio::loads(text) else { return bad("Form") };
    let get = |key: &str| d.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    if d.iter().any(|(k, _)| !WORKER_FIELDS.contains(&k.as_str())) {
        return bad("Felder");
    }
    if !get("v").and_then(Value::as_str).is_some_and(worker_version) {
        return bad("Version");
    }
    if !matches!(get("os").and_then(Value::as_str), Some("windows" | "linux")) {
        return bad("System");
    }
    let lang = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_lowercase());
    for key in ["ui", "game"] {
        if get(key).is_some_and(|v| !v.as_str().is_some_and(lang)) {
            return bad(key);
        }
    }
    for key in ["rc", "autostart", "update"] {
        if get(key).is_some_and(|v| !matches!(v, Value::Bool(_))) {
            return bad(key);
        }
    }
    if get("overlay").is_some_and(|v| !matches!(v.as_str(), Some("immer" | "popup"))) {
        return bad("overlay");
    }
    let unique_strings = |v: &Value, most: usize, ok: &dyn Fn(&str) -> bool| match v {
        Value::Array(items) if items.len() <= most => {
            let mut seen = BTreeSet::new();
            items.iter().all(|i| i.as_str().is_some_and(|s| ok(s) && seen.insert(s.to_string())))
        }
        _ => false,
    };
    let module = |s: &str| (1..=20).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase());
    if get("mods").is_some_and(|v| !unique_strings(v, 12, &module)) {
        return bad("mods");
    }
    if get("view").is_some_and(|v| !matches!(v.as_str(), Some("karten" | "reiter"))) {
        return bad("view");
    }
    if get("tiles_off").is_some_and(|v| !unique_strings(v, lists.tiles.len(), &|s| lists.tiles.contains(s))) {
        return bad("tiles_off");
    }
    let is_count = |v: &Value| matches!(v, Value::Int(n) if n.parse::<i64>().is_ok_and(|n| (1..=9999).contains(&n)));
    let page = |s: &str| lists.pages.contains(s);
    if let Some(v) = get("pages") {
        let Value::Object(map) = v else { return bad("pages") };
        if !map.iter().all(|(p, n)| page(p) && is_count(n)) {
            return bad("pages");
        }
    }
    let step = |k: &str| {
        (1..=2).contains(&k.len())
            && k.bytes().all(|b| b.is_ascii_digit())
            && k.parse::<i64>().is_ok_and(|n| n.to_string() == k && n <= 10)
    };
    for field in ["entry", "clicks"] {
        let Some(v) = get(field) else { continue };
        let Value::Object(map) = v else { return bad(field) };
        for (p, inner) in map {
            let Value::Object(inner) = inner else { return bad(field) };
            let key_ok = |k: &str| if field == "entry" { lists.routes.contains(k) } else { step(k) };
            if !page(p) || !inner.iter().all(|(k, n)| key_ok(k) && is_count(n)) {
                return bad(field);
            }
        }
    }
    if let Some(v) = get("misses") {
        let Value::Object(map) = v else { return bad("misses") };
        if map.len() > 20 {
            return bad("misses");
        }
        for (pair, n) in map {
            let parts: Vec<&str> = pair.split('>').collect();
            if parts.len() != 2 || !page(parts[0]) || !page(parts[1]) || parts[0] == parts[1] || !is_count(n) {
                return bad("misses");
            }
        }
    }
    if let Some(v) = get("actions") {
        let Value::Object(map) = v else { return bad("actions") };
        if !map.iter().all(|(k, n)| lists.actions.contains(k) && is_count(n)) {
            return bad("actions");
        }
    }
    Ok(())
}

/// What the made-up receiver saw of one request.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
    status: u16,
}

/// A receiver on `127.0.0.1` that answers as the real one would (204, or
/// 400/404/405/413 with its text); `forced` answers every request with that status.
struct Receiver {
    address: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Receiver {
    fn start(forced: Option<u16>) -> Receiver {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/ping", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let lists = worker_lists();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                serve(stream, &lists, forced, &log);
            }
        });
        Receiver { address, seen }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(mut stream: TcpStream, lists: &Lists, forced: Option<u16>, log: &Mutex<Vec<Seen>>) {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (method, path) = (first.next().unwrap_or_default().to_string(), first.next().unwrap_or_default().to_string());
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let length: usize =
        headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    let mut body = raw[head_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    let (status, text) = if let Some(status) = forced {
        (status, String::from("erzwungen"))
    } else if path != "/ping" {
        (404, "nicht hier".into())
    } else if method != "POST" {
        (405, "nur POST".into())
    } else if length > MAX_BYTES {
        (413, "zu gross".into())
    } else {
        match check(&body, lists) {
            Ok(()) => (204, String::new()),
            Err(refusal) => refusal,
        }
    };
    log.lock().unwrap().push(Seen { method, path, headers, body, status });
    let answer = if status == 204 {
        "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_string()
    } else {
        format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len())
    };
    let _ = stream.write_all(answer.as_bytes());
}

/// The real transport with the house switch "no network": only the own computer is reachable.
fn transport() -> HttpTransport {
    HttpTransport::new("", &Host::new(Flavor::native(), &[("SC_BP_NO_NET", "1")], "/")).unwrap()
}

fn post(transport: &HttpTransport, url: &str, body: &str) -> u16 {
    let request = Request {
        method: "POST".into(),
        url: url.into(),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: Some(body.as_bytes().to_vec()),
    };
    transport.send(&request).unwrap().status
}

// ------------------------------------------------------------ the receiver's rules

#[test]
fn the_lists_match_the_receiver() {
    let lists = worker_lists();
    let mut ours: BTreeSet<String> = PAGES.iter().map(|p| p.to_string()).collect();
    for older in ["lager", "handelslager", "bereich_schiffe"] {
        assert!(lists.pages.contains(older), "{older} is a page the receiver knows from older versions");
        ours.insert(older.into());
    }
    assert_eq!(ours, lists.pages, "the counted pages are the receiver's");
    assert_eq!(lists.routes, ROUTES.iter().map(|s| s.to_string()).collect());
    assert_eq!(lists.actions, ACTIONS.iter().map(|s| s.to_string()).collect());
    assert_eq!(lists.tiles, crate::overview::TILES.iter().map(|(id, _)| id.to_string()).collect());
    assert_eq!(
        lists.numbers,
        vec![
            ("MAX_CLICKS".to_string(), MAX_CLICKS),
            ("MAX_COUNT".to_string(), MAX_COUNT),
            ("MAX_MISS_PAIRS".to_string(), MAX_MISS_PAIRS as i64)
        ]
    );
    assert_eq!(lists.pages.len(), 64);
}

#[test]
fn the_receiver_takes_a_version_5_and_refuses_a_build_of_the_source() {
    let receiver = Receiver::start(None);
    let t = transport();
    let report = |v: &str| format!("{{\"v\": \"{v}\", \"os\": \"windows\"}}");
    assert_eq!(post(&t, &receiver.address, &report("5.0.0")), 204);
    assert_eq!(post(&t, &receiver.address, &report("5.0.0-rc3")), 204);
    assert_eq!(post(&t, &receiver.address, &report("4.12.0")), 204);
    assert_eq!(post(&t, &receiver.address, &report("5.0.0-dev")), 400);
    assert_eq!(post(&t, &receiver.address, &report("5.0")), 400);
    assert_eq!(post(&t, &receiver.address, "{\"v\": \"5.0.0\", \"os\": \"mac\"}"), 400);
    assert_eq!(post(&t, &receiver.address, "{\"v\": \"5.0.0\", \"os\": \"linux\", \"id\": \"x\"}"), 400);
    let texts: Vec<u16> = receiver.seen().iter().map(|s| s.status).collect();
    assert_eq!(texts, vec![204, 204, 204, 400, 400, 400, 400]);
}

#[test]
fn the_receiver_refuses_wrong_counters() {
    let lists = worker_lists();
    let base = "\"v\": \"5.0.0\", \"os\": \"windows\"";
    let refused = |extra: &str| check(&format!("{{{base}, {extra}}}"), &lists).unwrap_err().1;
    assert!(check(&format!("{{{base}, \"pages\": {{\"liste\": 1}}}}"), &lists).is_ok());
    assert_eq!(refused("\"pages\": {\"geheim\": 1}"), "pages");
    assert_eq!(refused("\"pages\": {\"liste\": 10000}"), "pages");
    assert_eq!(refused("\"entry\": {\"liste\": {\"maus\": 1}}"), "entry");
    assert_eq!(refused("\"clicks\": {\"liste\": {\"11\": 1}}"), "clicks");
    assert_eq!(refused("\"clicks\": {\"liste\": {\"01\": 1}}"), "clicks");
    assert_eq!(refused("\"misses\": {\"liste>liste\": 1}"), "misses");
    assert_eq!(refused("\"actions\": {\"lager_geheim\": 1}"), "actions");
    assert_eq!(refused("\"mods\": [\"schiffe\", \"schiffe\"]"), "mods");
    assert_eq!(refused("\"tiles_off\": [\"keine\"]"), "tiles_off");
    assert_eq!(refused("\"rc\": 1"), "rc");
    assert_eq!(refused("\"ui\": \"deu\""), "ui");
}

// ------------------------------------------------------------ target, switches, version

#[test]
fn the_version_is_one_the_receiver_takes() {
    for good in ["5.0.0", "5.0.0-rc1", "123.456.789-rc9999", "0.0.0"] {
        assert!(version_ok(good), "{good}");
    }
    for bad in ["5.0.0-dev", "5.0", "5.0.0-rc", "5.0.0-rc12345", "1234.0.0", "", "5.0.0 ", "v5.0.0", "5.0.0-rc1-dev"] {
        assert!(!version_ok(bad), "{bad}");
    }
}

#[test]
fn the_target_is_the_built_in_one_unless_the_variable_names_another() {
    let host = |vars: &[(&str, &str)]| Host::new(Flavor::native(), vars, "/");
    assert_eq!(target(&host(&[])).as_deref(), Some(TARGET));
    assert_eq!(
        target(&host(&[(ENV_TARGET, "  https://probe.example/ping \n")])).as_deref(),
        Some("https://probe.example/ping")
    );
    assert_eq!(target(&host(&[(ENV_TARGET, "aus")])), None);
    assert_eq!(target(&host(&[(ENV_TARGET, "")])), None);
    assert_eq!(target(&host(&[(ENV_TARGET, "http://example.org/ping")])), None);
    assert_eq!(target(&host(&[(ENV_TARGET, "http://127.0.0.1:9/ping")])).as_deref(), Some("http://127.0.0.1:9/ping"));
}

#[test]
fn no_net_switches_report_and_counting_off() {
    let host = |vars: &[(&str, &str)]| Host::new(Flavor::native(), vars, "/");
    assert!(would_send(&host(&[]), "5.0.0"));
    assert!(would_send(&host(&[("SC_BP_NO_NET", "0")]), "5.0.0"));
    assert!(would_send(&host(&[("SC_BP_NO_NET", "")]), "5.0.0"));
    assert!(!would_send(&host(&[("SC_BP_NO_NET", "1")]), "5.0.0"));
    assert!(!would_send(&host(&[(ENV_TARGET, "aus")]), "5.0.0"));
    assert!(!would_send(&host(&[]), "5.0.0-dev"));
}

#[test]
fn the_day_is_counted_in_utc() {
    let at = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    assert_eq!(today_utc(at(0)), "1970-01-01");
    assert_eq!(today_utc(at(1_792_713_599)), "2026-10-22");
    assert_eq!(today_utc(at(1_792_713_600)), "2026-10-23");
}

// ------------------------------------------------------------ the fields

#[test]
fn the_game_language_becomes_a_code() {
    assert_eq!(game_code(Some("german_(germany)")), "de");
    assert_eq!(game_code(Some("English")), "en");
    assert_eq!(game_code(Some("chinese_(simplified)")), "zh");
    assert_eq!(game_code(Some("klingon")), "xx");
    assert_eq!(game_code(None), "en");
    assert_eq!(game_code(Some("")), "en", "an empty entry is English, as `or 'english'` reads it");
}

#[test]
fn the_facts_come_from_the_settings() {
    let s = Setup::new("facts", &[], "5.0.0");
    let f = facts(s.paths(), "DE-de");
    assert_eq!(
        (f.ui.as_str(), f.rc, f.overlay.as_str(), f.update, f.view.as_str()),
        ("de", false, "immer", true, "reiter")
    );
    assert_eq!(
        f.mods,
        vec!["basetool", "bergbau", "bergung", "beschaffung", "handel", "schiffe", "statistiken", "werkstatt"]
    );
    assert_eq!(f.tiles_off, vec!["spielzeit"]);
    assert_eq!(f.os, if cfg!(windows) { "windows" } else { "linux" });
    std::fs::write(
        s.root.join("einstellungen.json"),
        "{\"vorabversionen\": true, \"overlay_modus\": \"popup\", \"update_automatisch\": false, \"ansicht\": \"karten\", \
         \"modul_schiffe\": false, \"uebersicht_aus\": [\"rechner\", \"funde\"]}",
    )
    .unwrap();
    let f = facts(s.paths(), "1x");
    assert_eq!(
        (f.ui.as_str(), f.rc, f.overlay.as_str(), f.update, f.view.as_str()),
        ("xx", true, "popup", false, "karten")
    );
    assert!(!f.mods.contains(&"schiffe".to_string()), "a switched-off module is not named: {:?}", f.mods);
    assert_eq!(f.tiles_off, vec!["funde", "rechner"]);
    std::fs::write(s.root.join("einstellungen.json"), "{\"overlay_modus\": \"gross\"}").unwrap();
    assert_eq!(facts(s.paths(), "en").overlay, "immer", "an unknown mode goes out as the default");
}

#[test]
fn the_report_is_written_as_python_writes_it() {
    let counts =
        obj("{\"pages\": {\"liste\": 2, \"hangar\": 1}, \"entry\": {\"liste\": {\"start\": 1, \"seitenleiste\": 1}}, \
         \"clicks\": {\"liste\": {\"10\": 1, \"2\": 1}}, \"misses\": {}, \"actions\": {\"lager_sync\": 3}}");
    let text = payload_text(&payload("5.0.0", &plain_facts(), &counts));
    assert_eq!(
        text,
        "{\"v\": \"5.0.0\", \"os\": \"windows\", \"ui\": \"de\", \"game\": \"de\", \"rc\": false, \
         \"mods\": [\"basetool\", \"schiffe\"], \"overlay\": \"immer\", \"autostart\": true, \"update\": true, \
         \"view\": \"reiter\", \"tiles_off\": [\"spielzeit\"], \"pages\": {\"hangar\": 1, \"liste\": 2}, \
         \"entry\": {\"liste\": {\"seitenleiste\": 1, \"start\": 1}}, \"clicks\": {\"liste\": {\"2\": 1, \"10\": 1}}, \
         \"misses\": {}, \"actions\": {\"lager_sync\": 3}}"
    );
    let mut odd = String::new();
    encode(&mut odd, &Value::str("ä\u{7f}😀\n"), (", ", ": "), true);
    assert_eq!(odd, "\"\\u00e4\\u007f\\ud83d\\ude00\\n\"", "beyond ASCII as json.dumps escapes it");
}

#[test]
fn the_report_holds_no_more_than_the_known_fields() {
    let map = payload("5.0.0", &plain_facts(), &Map::new());
    let keys: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "v",
            "os",
            "ui",
            "game",
            "rc",
            "mods",
            "overlay",
            "autostart",
            "update",
            "view",
            "tiles_off",
            "pages",
            "entry",
            "clicks",
            "misses",
            "actions"
        ]
    );
    assert!(check(&payload_text(&map), &worker_lists()).is_ok());
}

#[test]
fn the_largest_report_fits_and_is_taken() {
    let mut counts = empty();
    for page in PAGES {
        add(block_mut(&mut counts, "pages"), page, 50_000);
        for route in ROUTES {
            add(block_mut(block_mut(&mut counts, "entry"), page), route, 50_000);
        }
        for step in 0..=MAX_CLICKS {
            add(block_mut(block_mut(&mut counts, "clicks"), page), &step.to_string(), 50_000);
        }
    }
    for (i, page) in PAGES.iter().enumerate().take(40) {
        add(block_mut(&mut counts, "misses"), &format!("{page}>{}", PAGES[i + 1]), 50_000 + i as i64);
    }
    for action in ACTIONS {
        add(block_mut(&mut counts, "actions"), action, 50_000);
    }
    let mut facts = plain_facts();
    facts.tiles_off = crate::overview::TILES.iter().map(|(id, _)| id.to_string()).collect();
    let text = payload_text(&payload("123.456.789-rc9999", &facts, &counts));
    assert!(text.len() <= MAX_BYTES, "{} bytes", text.len());
    assert_eq!(check(&text, &worker_lists()), Ok(()));
    let sent = jsonio::loads(&text).unwrap();
    assert_eq!(sent.get("misses").and_then(Value::as_object).map(Vec::len), Some(MAX_MISS_PAIRS));
    assert_eq!(sent.get("pages").and_then(|p| p.get("liste")), Some(&Value::int(MAX_COUNT)));
}

// ------------------------------------------------------------ counting

#[test]
fn a_way_through_the_window_is_counted_as_4_12_counts_it() {
    let s = Setup::new("way", &[], "5.0.0");
    let u = &s.usage;
    u.window_opened();
    u.page_opened("uebersicht", Some("start"), Some(0.0));
    u.group_toggled();
    u.page_opened("verkauf", Some("seitenleiste"), Some(10.0));
    u.page_opened("routen", Some("seitenleiste"), Some(11.0));
    u.page_opened("laeden", Some("seitenleiste"), Some(12.0));
    u.page_opened("laeden", Some("sprung"), Some(13.0));
    u.page_opened("geheim", Some("sprung"), Some(14.0));
    u.page_opened("liste", None, Some(15.0));
    u.page_opened("liste", Some("maus"), Some(16.0));
    u.window_closed(Some(30.0));
    let stored = u.stored();
    assert_eq!(
        file_text(&stored),
        "{\"pages\":{\"uebersicht\":1,\"verkauf\":1,\"routen\":1,\"laeden\":1},\
         \"entry\":{\"uebersicht\":{\"start\":1},\"verkauf\":{\"seitenleiste\":1},\"routen\":{\"seitenleiste\":1},\
         \"laeden\":{\"seitenleiste\":1}},\"clicks\":{\"uebersicht\":{\"0\":1},\"laeden\":{\"4\":1}},\
         \"misses\":{\"verkauf>laeden\":1,\"routen>laeden\":1},\"actions\":{}}"
    );
    assert_eq!(s.file_text().as_deref(), Some(file_text(&stored).as_str()), "the file holds the same, compact");
}

#[test]
fn a_group_toggle_and_ten_clicks_and_more_count_on_the_way() {
    let s = Setup::new("clicks", &[], "5.0.0");
    let u = &s.usage;
    u.window_opened();
    u.group_toggled();
    u.group_toggled();
    u.page_opened("liste", Some("seitenleiste"), Some(0.0));
    for _ in 0..12 {
        u.group_toggled();
    }
    u.page_opened("hangar", Some("karte"), Some(5.0));
    u.window_closed(Some(10.0));
    let clicks = block(&u.stored(), "clicks").clone();
    assert_eq!(clicks, obj("{\"liste\": {\"3\": 1}, \"hangar\": {\"10\": 1}}"));
}

#[test]
fn misses_without_a_target_fall_away_and_only_five_are_remembered() {
    let s = Setup::new("misses", &[], "5.0.0");
    let u = &s.usage;
    u.window_opened();
    let quick = ["liste", "fortschritt", "auftragslog", "hangar", "wunschliste", "asop", "meinlager"];
    for (i, page) in quick.iter().enumerate() {
        u.page_opened(page, Some("seitenleiste"), Some(i as f64));
    }
    u.page_opened("laeden", Some("seitenleiste"), Some(7.0));
    u.page_opened("verkauf", Some("seitenleiste"), Some(20.0));
    u.window_closed(Some(21.0));
    let misses: Vec<String> = block(&u.stored(), "misses").iter().map(|(k, _)| k.clone()).collect();
    assert_eq!(
        misses,
        vec!["auftragslog>laeden", "hangar>laeden", "wunschliste>laeden", "asop>laeden", "meinlager>laeden"]
    );
    assert!(!block(&u.stored(), "clicks").iter().any(|(k, _)| k == "verkauf"), "verkauf was left within 3 s");
}

#[test]
fn a_page_left_before_three_seconds_is_a_miss_and_one_left_at_three_a_target() {
    let s = Setup::new("threshold", &[], "5.0.0");
    let u = &s.usage;
    u.window_opened();
    u.page_opened("liste", Some("seitenleiste"), Some(0.1));
    u.page_opened("hangar", Some("seitenleiste"), Some(3.0));
    u.page_opened("laeden", Some("seitenleiste"), Some(6.0));
    u.window_closed(Some(100.0));
    let stored = u.stored();
    assert_eq!(file_text(block(&stored, "misses")), "{\"liste>hangar\":1}", "2.9 s is a miss");
    assert_eq!(file_text(block(&stored, "clicks")), "{\"hangar\":{\"2\":1},\"laeden\":{\"1\":1}}", "3.0 s is a target");
}

#[test]
fn an_action_counts_how_often_only() {
    let s = Setup::new("action", &[], "5.0.0");
    s.usage.action("lager_hand");
    s.usage.action("lager_hand");
    s.usage.action("galerie_offen");
    s.usage.action("lager_geheim");
    assert_eq!(block(&s.usage.stored(), "actions").clone(), obj("{\"lager_hand\": 2, \"galerie_offen\": 1}"));
}

#[test]
fn without_network_nothing_is_counted_and_old_counters_go() {
    let s = Setup::new("off", &[("SC_BP_NO_NET", "1")], "5.0.0");
    std::fs::write(s.file(), "{\"pages\":{\"liste\":4}}").unwrap();
    s.usage.window_opened();
    assert!(s.file_text().is_none(), "a counter file of before is dropped");
    s.usage.page_opened("liste", Some("seitenleiste"), Some(0.0));
    s.usage.action("lager_hand");
    assert!(s.file_text().is_none());
    assert!(FIELDS.iter().all(|f| block(&s.usage.stored(), f).is_empty()));
    // Dropped once, not on every change of page.
    std::fs::write(s.file(), "{}").unwrap();
    s.usage.action("lager_hand");
    assert_eq!(s.file_text().as_deref(), Some("{}"));
    let dev = Setup::new("dev", &[], "5.0.0-dev");
    dev.usage.action("lager_hand");
    assert!(dev.file_text().is_none(), "a build of the source counts nothing");
}

#[test]
fn old_page_names_count_on_their_new_page() {
    let state = clean(&Value::Object(obj(
        "{\"pages\": {\"lager\": 2, \"handelslager\": 1, \"meinlager\": 1, \"Liste\": 4, \"liste\": true, \"hangar\": 1.0}, \
          \"entry\": {\"lager\": {\"sprung\": 1}, \"meinlager\": {\"sprung\": 2, \"maus\": 1}}, \
          \"clicks\": {\"handelslager\": {\"3\": 1, \"03\": 1, \"11\": 1}}, \
          \"misses\": {\"lager>meinlager\": 5, \"liste>lager\": 1, \"a>b>c\": 1}, \
          \"actions\": {\"lager_scan\": 1, \"weg\": 1}, \"extra\": {\"x\": 1}}",
    )));
    assert_eq!(
        file_text(&state),
        "{\"pages\":{\"meinlager\":4},\"entry\":{\"meinlager\":{\"sprung\":3}},\"clicks\":{\"meinlager\":{\"3\":1}},\
         \"misses\":{\"liste>meinlager\":1},\"actions\":{\"lager_scan\":1}}"
    );
}

#[test]
fn the_counts_of_the_former_evaluation_page_go_to_the_statistics_overview() {
    let state = clean(&Value::Object(obj("{\"pages\": {\"statistik_auswertung\": 3, \"statistik\": 2}, \
          \"entry\": {\"statistik_auswertung\": {\"sprung\": 1}, \"statistik\": {\"sprung\": 2, \"karte\": 1}}, \
          \"clicks\": {\"statistik_auswertung\": {\"2\": 1}, \"statistik\": {\"2\": 4}}, \
          \"misses\": {\"statistik_auswertung>statistik\": 5, \"liste>statistik_auswertung\": 2}}")));
    assert_eq!(
        file_text(&state),
        "{\"pages\":{\"statistik\":5},\"entry\":{\"statistik\":{\"sprung\":3,\"karte\":1}},\
         \"clicks\":{\"statistik\":{\"2\":5}},\"misses\":{\"liste>statistik\":2},\"actions\":{}}"
    );
}

#[test]
fn outgoing_caps_sorts_and_keeps_the_most_frequent_misses() {
    let mut raw =
        obj("{\"pages\": {\"liste\": 12345, \"hangar\": 2}, \"clicks\": {\"liste\": {\"10\": 1, \"9\": 1, \"0\": 2}}}");
    let mut misses = Map::new();
    // (old names count as their new page: their pairs would add up with the pairs of that page)
    let current: Vec<&str> = PAGES.iter().copied().filter(|page| new_page(page) == *page).collect();
    for i in 0..25 {
        let page = current[i + 1];
        misses.push((format!("{page}>liste"), Value::int(if i < 3 { 1 } else { 5 })));
    }
    raw.push(("misses".into(), Value::Object(misses)));
    let out = outgoing(&Value::Object(raw));
    assert_eq!(file_text(block_ref(&out, "pages")), "{\"hangar\":2,\"liste\":9999}");
    assert_eq!(file_text(block_ref(&out, "clicks")), "{\"liste\":{\"0\":2,\"9\":1,\"10\":1}}");
    let kept = block_ref(&out, "misses");
    assert_eq!(kept.len(), MAX_MISS_PAIRS);
    assert!(kept.iter().all(|(_, n)| *n == Value::int(5)), "the rare pairs fall away first");
    let names: Vec<&String> = kept.iter().map(|(k, _)| k).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "equal counts by name");
}

fn block_ref<'a>(map: &'a Map, field: &str) -> &'a Map {
    block(map, field)
}

#[test]
fn forget_takes_off_only_what_was_sent() {
    let s = Setup::new("forget", &[], "5.0.0");
    s.usage.action("lager_hand");
    s.usage.action("lager_sync");
    let sent = s.usage.stored();
    s.usage.action("lager_hand");
    s.usage.forget(&sent);
    assert_eq!(
        s.file_text().as_deref(),
        Some("{\"pages\":{},\"entry\":{},\"clicks\":{},\"misses\":{},\"actions\":{\"lager_hand\":1}}")
    );
    let rest = s.usage.stored();
    s.usage.forget(&rest);
    assert!(s.file_text().is_none(), "all sent: the file goes");
}

// ------------------------------------------------------------ sending

fn send_setup(name: &str, receiver: &Receiver, version: &str, extra: &[(&str, &str)]) -> Setup {
    let mut vars = vec![(ENV_TARGET, receiver.address.as_str())];
    vars.extend_from_slice(extra);
    Setup::new(name, &vars, version)
}

#[test]
fn once_a_day_the_report_goes_out_and_the_counters_are_emptied() {
    let receiver = Receiver::start(None);
    let s = send_setup("send", &receiver, "5.0.0", &[]);
    s.usage.window_opened();
    s.usage.page_opened("uebersicht", Some("start"), Some(0.0));
    s.usage.window_closed(Some(10.0));
    s.usage.action("galerie_offen");
    let t = transport();
    assert_eq!(send_if_due(&t, &s.usage, &plain_facts(), "2026-10-23"), Sent::Sent);
    let seen = receiver.seen();
    assert_eq!(seen.len(), 1);
    let one = &seen[0];
    assert_eq!((one.method.as_str(), one.path.as_str(), one.status), ("POST", "/ping", 204));
    let header = |name: &str| one.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    assert_eq!(header("content-type"), Some("application/json"));
    assert_eq!(header("user-agent"), Some("VerseKit/5.0.0"));
    assert_eq!(
        one.body,
        "{\"v\": \"5.0.0\", \"os\": \"windows\", \"ui\": \"de\", \"game\": \"de\", \"rc\": false, \
         \"mods\": [\"basetool\", \"schiffe\"], \"overlay\": \"immer\", \"autostart\": true, \"update\": true, \
         \"view\": \"reiter\", \"tiles_off\": [\"spielzeit\"], \"pages\": {\"uebersicht\": 1}, \
         \"entry\": {\"uebersicht\": {\"start\": 1}}, \"clicks\": {\"uebersicht\": {\"0\": 1}}, \"misses\": {}, \
         \"actions\": {\"galerie_offen\": 1}}"
    );
    assert_eq!(s.paths().setting(LAST_SETTING).as_deref(), Some("2026-10-23"));
    assert!(s.file_text().is_none(), "the counters went out and are gone");
    assert_eq!(send_if_due(&t, &s.usage, &plain_facts(), "2026-10-23"), Sent::Already);
    assert_eq!(receiver.seen().len(), 1, "once a day");
    assert_eq!(send_if_due(&t, &s.usage, &plain_facts(), "2026-10-24"), Sent::Sent);
    assert_eq!(receiver.seen().len(), 2);
}

#[test]
fn a_refused_report_keeps_the_counters_and_the_day_open() {
    for status in [500, 400, 429] {
        let receiver = Receiver::start(Some(status));
        let s = send_setup(&format!("refused{status}"), &receiver, "5.0.0", &[]);
        s.usage.action("lager_hand");
        let before = s.file_text();
        assert_eq!(send_if_due(&transport(), &s.usage, &plain_facts(), "2026-10-23"), Sent::Failed);
        assert_eq!(receiver.seen().len(), 1);
        assert_eq!(s.paths().setting(LAST_SETTING), None);
        assert_eq!(s.file_text(), before);
        assert_eq!(wait_after(Sent::Failed), RETRY_AFTER);
    }
    let receiver = Receiver::start(Some(200));
    let s = send_setup("ok200", &receiver, "5.0.0", &[]);
    assert_eq!(send_if_due(&transport(), &s.usage, &plain_facts(), "2026-10-23"), Sent::Sent, "200 counts as taken");
    assert_eq!(wait_after(Sent::Sent), CHECK_EVERY);
}

#[test]
fn no_net_a_switched_off_target_and_a_source_build_send_nothing() {
    let receiver = Receiver::start(None);
    let t = transport();
    let off = send_setup("nonet", &receiver, "5.0.0", &[("SC_BP_NO_NET", "1")]);
    assert_eq!(send_if_due(&t, &off.usage, &plain_facts(), "2026-10-23"), Sent::Off);
    let dev = send_setup("srcbuild", &receiver, "5.0.0-dev", &[]);
    assert_eq!(send_if_due(&t, &dev.usage, &plain_facts(), "2026-10-23"), Sent::Version);
    let aus = Setup::new("aus", &[(ENV_TARGET, "aus")], "5.0.0");
    assert_eq!(send_if_due(&t, &aus.usage, &plain_facts(), "2026-10-23"), Sent::Off);
    assert!(receiver.seen().is_empty(), "nothing reached the receiver");
    assert_eq!(dev.paths().setting(LAST_SETTING), None);
}

#[test]
fn a_settings_file_that_refuses_the_day_sends_no_second_report() {
    let receiver = Receiver::start(None);
    let s = send_setup("refuses", &receiver, "5.0.0", &[]);
    // The settings file is a folder: it can be neither read nor written.
    std::fs::create_dir_all(s.root.join("einstellungen.json")).unwrap();
    let t = transport();
    assert_eq!(send_if_due(&t, &s.usage, &plain_facts(), "2026-10-23"), Sent::Sent);
    assert_eq!(send_if_due(&t, &s.usage, &plain_facts(), "2026-10-23"), Sent::Already);
    assert_eq!(receiver.seen().len(), 1, "once a day, also then");
}

#[test]
fn the_made_up_receiver_reads_the_version_as_the_worker_does() {
    for good in ["5.0.0", "5.0.0-rc1", "123.456.789-rc9999"] {
        assert!(worker_version(good), "{good}");
    }
    for bad in ["5.0.0-dev", "5.0", "5.0.0-rc", "5.0.0-rc12345", "1234.0.0", "", "5.0.0.1"] {
        assert!(!worker_version(bad), "{bad}");
    }
}

#[test]
fn a_report_of_a_release_candidate_is_taken() {
    let receiver = Receiver::start(None);
    let s = send_setup("rc", &receiver, "5.0.0-rc2", &[]);
    let mut f = plain_facts();
    f.rc = true;
    f.os = "linux";
    assert_eq!(send_if_due(&transport(), &s.usage, &f, "2026-10-23"), Sent::Sent);
    assert_eq!(receiver.seen()[0].status, 204);
}

/// Takes every request as the receiver would (204) and keeps its address; nothing leaves the process.
#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl Transport for Recorder {
    fn send(&self, request: &Request) -> Result<crate::basetool::http::Response, crate::basetool::error::ApiError> {
        self.0.lock().unwrap().push(request.url.clone());
        Ok(crate::basetool::http::Response { status: 204, ..Default::default() })
    }
}

/// A copy of the data as `--daten` makes it, with `vars` on its host.
fn copy_usage(name: &str, vars: &[(&str, &str)]) -> (PageUsage, std::path::PathBuf) {
    let root = crate::testdir::fresh(&format!("nutzung-kopie-{name}"));
    std::fs::create_dir_all(root.join(crate::paths::COPY_DATA_FOLDER)).unwrap();
    let text = root.to_string_lossy().into_owned();
    let paths = Paths::for_copy(Host::new(Flavor::native(), vars, &text), &text).expect("a copy");
    (PageUsage::new(paths, "5.0.0"), root)
}

#[test]
fn a_copy_of_the_data_never_reports_to_a_receiver_elsewhere() {
    // A receiver that is not on this computer stands for the real one; the recorder keeps it from the net.
    let elsewhere = "https://nutzung.beispiel.invalid/ping";
    let recorder = Recorder::default();
    let (copy, root) = copy_usage("fremd", &[(ENV_TARGET, elsewhere)]);
    assert!(!copy.active(), "a copy counts nothing for a report elsewhere");
    assert_eq!(send_if_due(&recorder, &copy, &plain_facts(), "2026-10-23"), Sent::Off);
    assert!(recorder.0.lock().unwrap().is_empty(), "nothing went out from the copy");
    assert_eq!(copy.paths.setting(LAST_SETTING), None);
    let _ = std::fs::remove_dir_all(root);
    // Counter-check: the player's own data report to the same receiver, and a copy to one on this computer.
    let own = Setup::new("eigen-fremd", &[(ENV_TARGET, elsewhere)], "5.0.0");
    assert!(own.usage.active());
    assert_eq!(send_if_due(&recorder, &own.usage, &plain_facts(), "2026-10-23"), Sent::Sent);
    let (local, root) = copy_usage("lokal", &[(ENV_TARGET, "http://127.0.0.1:9/ping")]);
    assert!(local.active());
    assert_eq!(send_if_due(&recorder, &local, &plain_facts(), "2026-10-23"), Sent::Sent);
    assert_eq!(*recorder.0.lock().unwrap(), [elsewhere, "http://127.0.0.1:9/ping"]);
    let _ = std::fs::remove_dir_all(root);
}
