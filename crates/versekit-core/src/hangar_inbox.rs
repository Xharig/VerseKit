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

//! The hangar delivery: a browser extension sends the ship list
//! `hangar-schiffe` v1 to `POST http://127.0.0.1:<port>/api/v1/hangar/uebergabe`
//! with `Authorization: Bearer <token>`, and it is read into `hangar.json`
//! like an imported file.
//!
//! | Status | Body | Meaning |
//! |---|---|---|
//! | 200 | `{"schiffe": n, "unveraendert": b}` | delivered; `unveraendert`: the hangar already was exactly this |
//! | 400 | `{"fehler": …, "code": …}` | refused, nothing changed (`json`, `schema`, `version`, `uebergabe`, `schiffe`, `leer`, `laenge`, `anfrage` for no HTTP request) |
//! | 431 | `code: "kopf"` | request head longer than 16 KiB |
//! | 401 | `code: "token"` | the pairing code does not match |
//! | 403 | `code: "hangarzugang_aus"` | the delivery is switched off |
//! | 404 / 405 | `code: "pfad"` / `"methode"` | another path or method |
//! | 413 | `code: "zu_gross"` | more than 16 MiB |
//! | 503 | `code: "nicht_bereit"` | not ready, or the hangar could not be written |
//!
//! ⚠ Listens on `127.0.0.1` only. The token is never written to a message,
//! a log or an answer.
//!
//! ⚠ Before a delivery removes a ship, the hangar file is copied to
//! `hangar.bak.json`; without that copy nothing is written.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::hangar::{self, Imported, ShipNames};
use crate::jsonio::{self, Map, Value};
use crate::writeguard;

pub const PATH: &str = "/api/v1/hangar/uebergabe";
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
/// The port tried first; another free one is taken when it is busy.
pub const DEFAULT_PORT: u16 = 47823;
pub const CODE_PREFIX: &str = "VK1";

/// Setting: delivery on (default) or off.
pub const SETTING_ON: &str = "hangar_einlieferung";
/// Setting: the fixed port.
pub const SETTING_PORT: &str = "hangar_einlieferung_port";
/// Setting: the token (64 lower-case hex characters).
pub const SETTING_TOKEN: &str = "hangar_einlieferung_token";

const MAX_HEAD: usize = 16 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// The most one connection may take, so a slow client cannot hold the receiver.
pub const DEADLINE: Duration = Duration::from_secs(30);
/// How long unread request bytes are drained after an early answer.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const DRAIN_MAX: usize = 64 * 1024 * 1024;

// ------------------------------------------------------------ pairing code

/// A new token: 32 random bytes as 64 lower-case hex characters.
pub fn new_token() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 64 lower-case hex characters.
pub fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `VK1-<port>-<token>`.
pub fn pairing_code(port: u16, token: &str) -> String {
    // The pairing code is a secret: private for the streamer mode.
    crate::streamer::learn(token);
    format!("{CODE_PREFIX}-{port}-{token}")
}

/// The other way round, checked like the extension: `^VK1-([1-9]\d{0,4})-([0-9a-f]{64})$`
/// after removing outer white space, port 1 to 65535.
pub fn parse_pairing_code(code: &str) -> Option<(u16, String)> {
    let rest = code.trim().strip_prefix("VK1-")?;
    let (port, token) = rest.split_once('-')?;
    let digits_ok = !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit());
    if !digits_ok || port.starts_with('0') || !valid_token(token) {
        return None;
    }
    let port: u16 = port.parse().ok().filter(|p| *p > 0)?;
    Some((port, token.to_string()))
}

// ------------------------------------------------------------ answers

/// An HTTP answer with a JSON body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

impl Answer {
    fn error(status: u16, message: &str, code: &str) -> Answer {
        let body: Map = vec![("fehler".into(), Value::str(message)), ("code".into(), Value::str(code))];
        Answer { status, body: jsonio::dumps(&Value::Object(body), None, false) }
    }

    fn delivered(ships: usize, unchanged: bool) -> Answer {
        let body: Map =
            vec![("schiffe".into(), Value::int(ships as i64)), ("unveraendert".into(), Value::Bool(unchanged))];
        Answer { status: 200, body: jsonio::dumps(&Value::Object(body), None, false) }
    }

    /// The `code` of an error answer, `""` for 200.
    pub fn code(&self) -> String {
        match jsonio::loads(&self.body) {
            Ok(v) => v.get("code").and_then(Value::as_str).unwrap_or("").to_string(),
            Err(_) => String::new(),
        }
    }

    /// The whole HTTP answer as bytes.
    pub fn to_http(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            reason(self.status),
            self.body.len()
        );
        if self.status == 405 {
            head.push_str("Allow: POST\r\n");
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(self.body.as_bytes());
        bytes
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    }
}

// ------------------------------------------------------------ request head

/// The parts of a request head that matter here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub content_length: Option<String>,
    pub chunked: bool,
    pub expect_continue: bool,
}

/// Parse the head (everything before the empty line).
pub fn parse_head(text: &str) -> Option<Head> {
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    if !first.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let path = target.split('?').next().unwrap_or("").to_string();
    let mut head = Head { method, path, ..Head::default() };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim_matches([' ', '\t']).to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => head.authorization = Some(value),
            "content-length" => head.content_length = Some(value),
            "transfer-encoding" => head.chunked = true,
            "expect" => head.expect_continue = value.eq_ignore_ascii_case("100-continue"),
            _ => {}
        }
    }
    Some(head)
}

/// What decides about a request before its body is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    pub enabled: bool,
    pub token: String,
    pub ready: bool,
}

/// Compare without stopping at the first difference.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// Path, method, token, switch, readiness, size: the body length to read, or
/// the answer.
pub fn check_head(head: &Head, gate: &Gate) -> Result<usize, Answer> {
    if head.path != PATH {
        return Err(Answer::error(404, "Unbekannter Pfad", "pfad"));
    }
    if head.method != "POST" {
        return Err(Answer::error(405, "Nur POST", "methode"));
    }
    let expected = format!("Bearer {}", gate.token);
    let given = head.authorization.as_deref().unwrap_or("");
    if !valid_token(&gate.token) || !same_secret(given, &expected) {
        return Err(Answer::error(401, "Kopplungscode passt nicht", "token"));
    }
    if !gate.enabled {
        return Err(Answer::error(403, "Hangar-Einlieferung ist ausgeschaltet", "hangarzugang_aus"));
    }
    if !gate.ready {
        return Err(Answer::error(503, "VerseKit ist gerade nicht bereit", "nicht_bereit"));
    }
    let length = match &head.content_length {
        None if head.chunked => return Err(Answer::error(400, "Länge fehlt", "laenge")),
        None => 0,
        Some(text) => match text.parse::<u64>() {
            Ok(n) => n,
            Err(_) => return Err(Answer::error(400, "Länge unlesbar", "laenge")),
        },
    };
    if length > MAX_BYTES as u64 {
        return Err(Answer::error(413, "Größer als 16 MiB", "zu_gross"));
    }
    Ok(length as usize)
}

// ------------------------------------------------------------ delivery

/// What a delivery did, for the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivered {
    pub ships: usize,
    pub unchanged: bool,
    pub imported: Imported,
    pub approximations: Vec<String>,
}

/// Check the body and read it into the hangar file.
pub fn deliver(body: &[u8], hangar_file: &str, names: &ShipNames) -> (Answer, Option<Delivered>) {
    let not_json = || (Answer::error(400, "Kein JSON", "json"), None);
    let Ok(text) = std::str::from_utf8(body) else { return not_json() };
    let Ok(raw) = jsonio::loads(text) else { return not_json() };
    let Value::Object(map) = &raw else { return (Answer::error(400, "Kein JSON-Objekt", "json"), None) };
    let schema = raw.get("schema").and_then(Value::as_str);
    if schema == Some(hangar::HANGAR_TRANSFER_SCHEMA) {
        let message = "Übergabedatei je Pledge, erwartet wird die Schiffsliste";
        return (Answer::error(400, message, "uebergabe"), None);
    }
    if schema != Some(hangar::HANGAR_SHIPS_SCHEMA) {
        return (Answer::error(400, "Keine Schiffsliste (schema)", "schema"), None);
    }
    if !hangar::version_one(raw.get("version")) {
        let shown = raw.get("version").map_or("None".to_string(), |v| jsonio::dumps(v, None, false));
        return (Answer::error(400, &format!("Formatversion {shown} nicht unterstützt"), "version"), None);
    }
    let Some(Value::Array(items)) = raw.get("schiffe") else {
        return (Answer::error(400, "Feld schiffe fehlt", "schiffe"), None);
    };
    let count = items.len();
    let entries = hangar::from_hangar_ships_with(map, names).unwrap_or_default();
    if entries.is_empty() {
        return (Answer::error(400, "Keine Schiffe in der Liste", "leer"), None);
    }
    let _lock = hangar::lock();
    let mut data = hangar::load(hangar_file);
    let before = hangar::file_text(&data);
    let imported = hangar::import_entries(&mut data, &entries);
    hangar::settle(&mut data);
    let unchanged = hangar::file_text(&data) == before;
    if !imported.removed.is_empty()
        && Path::new(hangar_file).exists()
        && jsonio::copy2(hangar_file, &jsonio::backup_name(hangar_file)).is_err()
    {
        return (Answer::error(503, "Sicherung des Hangars ließ sich nicht anlegen", "nicht_bereit"), None);
    }
    if !unchanged && !hangar::write(hangar_file, &data) {
        return (Answer::error(503, "Hangar ließ sich nicht speichern", "nicht_bereit"), None);
    }
    let delivered = Delivered { ships: count, unchanged, imported, approximations: hangar::approximations(&entries) };
    (Answer::delivered(count, unchanged), Some(delivered))
}

// ------------------------------------------------------------ the running receiver

/// What happened last, for the page; `at` is the local time `HH:MM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Delivered { at: String, delivered: Delivered },
    Refused { at: String, status: u16, code: String },
}

fn clock() -> String {
    let (_, _, _, h, m, _) = jsonio::local_time();
    format!("{h:02}:{m:02}")
}

/// State shared between the receiver thread and the window.
pub struct Inbox {
    gate: Mutex<Gate>,
    last: Mutex<Option<Event>>,
    deliveries: AtomicU64,
}

impl Inbox {
    pub fn new(gate: Gate) -> Arc<Inbox> {
        Arc::new(Inbox { gate: Mutex::new(gate), last: Mutex::new(None), deliveries: AtomicU64::new(0) })
    }

    pub fn gate(&self) -> Gate {
        self.gate.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_gate(&self, gate: Gate) {
        *self.gate.lock().unwrap_or_else(|e| e.into_inner()) = gate;
    }

    pub fn last(&self) -> Option<Event> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Counts up with every delivery that changed the hangar file.
    pub fn deliveries(&self) -> u64 {
        self.deliveries.load(Ordering::Relaxed)
    }

    fn remember(&self, event: Event) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(event);
    }
}

/// Listen on `127.0.0.1:port`.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// A port to keep: the default when free, else one the system hands out.
pub fn pick_port() -> Option<u16> {
    if bind(DEFAULT_PORT).is_ok() {
        return Some(DEFAULT_PORT);
    }
    bind(0).ok().and_then(|l| l.local_addr().ok()).map(|a| a.port())
}

/// Where the receiver reads and writes.
#[derive(Clone, Debug)]
pub struct Files {
    pub hangar: String,
    pub ship_catalog: String,
}

/// Answer requests until the listener fails; `changed` runs after every
/// request that leaves something new for the page. `guard` is the write
/// lock of the thread that starts the receiver: it holds here as well.
pub fn serve(listener: TcpListener, inbox: Arc<Inbox>, files: Files, guard: writeguard::State, changed: impl Fn()) {
    serve_with(listener, inbox, files, guard, DEADLINE, changed);
}

/// Like `serve`, with the most one connection may take.
pub fn serve_with(
    listener: TcpListener,
    inbox: Arc<Inbox>,
    files: Files,
    guard: writeguard::State,
    limit: Duration,
    changed: impl Fn(),
) {
    writeguard::adopt(&guard);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        handle(stream, &inbox, &files, Instant::now() + limit);
        changed();
    }
}

fn find_head_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Read once, but never past `deadline`.
fn read_until(stream: &mut TcpStream, chunk: &mut [u8], deadline: Instant) -> io::Result<usize> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::Error::from(io::ErrorKind::TimedOut));
    }
    stream.set_read_timeout(Some(left.min(READ_TIMEOUT)))?;
    stream.read(chunk)
}

/// One request per connection, finished by `deadline`.
pub fn handle(mut stream: TcpStream, inbox: &Inbox, files: &Files, deadline: Instant) {
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let end = loop {
        if let Some(end) = find_head_end(&buffer) {
            break end;
        }
        if buffer.len() > MAX_HEAD {
            reply(&mut stream, &Answer::error(431, "Kopfzeilen zu lang", "kopf"), true);
            return;
        }
        match read_until(&mut stream, &mut chunk, deadline) {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
    let Some(head) = parse_head(&head) else {
        reply(&mut stream, &Answer::error(400, "Keine HTTP-Anfrage", "anfrage"), true);
        return;
    };
    let length = match check_head(&head, &inbox.gate()) {
        Ok(length) => length,
        Err(answer) => {
            inbox.remember(Event::Refused { at: clock(), status: answer.status, code: answer.code() });
            reply(&mut stream, &answer, true);
            return;
        }
    };
    if head.expect_continue {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let mut body = buffer[end + 4..].to_vec();
    body.truncate(length);
    while body.len() < length {
        let want = (length - body.len()).min(chunk.len());
        match read_until(&mut stream, &mut chunk[..want], deadline) {
            Ok(0) | Err(_) => return,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    let names = ShipNames::load(&files.ship_catalog);
    let (answer, delivered) = deliver(&body, &files.hangar, &names);
    let at = clock();
    match delivered {
        Some(delivered) => {
            if !delivered.unchanged {
                inbox.deliveries.fetch_add(1, Ordering::Relaxed);
            }
            inbox.remember(Event::Delivered { at, delivered });
        }
        None => inbox.remember(Event::Refused { at, status: answer.status, code: answer.code() }),
    }
    reply(&mut stream, &answer, false);
}

/// Send the answer; after an early answer, drain what the client still sends
/// so it sees the answer and not a reset connection.
fn reply(stream: &mut TcpStream, answer: &Answer, early: bool) {
    let _ = stream.write_all(&answer.to_http());
    let _ = stream.flush();
    if early {
        let _ = stream.shutdown(Shutdown::Write);
        let until = Instant::now() + DRAIN_TIMEOUT;
        let mut chunk = [0u8; 8192];
        let mut drained = 0usize;
        while drained < DRAIN_MAX {
            match read_until(stream, &mut chunk, until) {
                Ok(0) | Err(_) => break,
                Ok(n) => drained += n,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn gate() -> Gate {
        Gate { enabled: true, token: TOKEN.into(), ready: true }
    }

    fn head(auth: &str, length: &str) -> Head {
        Head {
            method: "POST".into(),
            path: PATH.into(),
            authorization: Some(auth.into()),
            content_length: Some(length.into()),
            ..Head::default()
        }
    }

    fn temp_file(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-inbox-{}-{name}", crate::testdir::run_tag()));
        // What an earlier run left in this folder (a hangar.json) would make "refused bodies write nothing" fail.
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir.join("hangar.json").to_string_lossy().into_owned()
    }

    const LIST: &str = r#"{"schema": "hangar-schiffe", "version": 1, "schiffe": [
        {"matrix": "Gladius", "matrixZuordnung": "sicher", "name": "Gladius", "code": "AEGS_Gladius",
         "manufacturer": {"code": "AEGS", "name": "Aegis Dynamics"}, "insurance": "LTI"}]}"#;

    #[test]
    fn pairing_code_round_trip_and_rules() {
        let token = new_token().unwrap();
        assert!(valid_token(&token));
        assert_ne!(token, new_token().unwrap());
        let code = pairing_code(47823, &token);
        assert_eq!(parse_pairing_code(&format!("  {code}\n")), Some((47823, token.clone())));
        for bad in [
            format!("VK2-47823-{token}"),
            format!("VK1-0-{token}"),
            format!("VK1-65536-{token}"),
            format!("VK1-047823-{token}"),
            format!("VK1-47823-{}", token.to_uppercase()),
        ] {
            assert_eq!(parse_pairing_code(&bad), None, "{bad}");
        }
    }

    #[test]
    fn head_answers() {
        let ok = format!("Bearer {TOKEN}");
        assert_eq!(check_head(&head(&ok, "10"), &gate()), Ok(10));
        let status = |h: &Head, g: &Gate| check_head(h, g).unwrap_err().status;
        assert_eq!(status(&head("Bearer falsch", "10"), &gate()), 401);
        assert_eq!(status(&head(&ok.to_uppercase(), "10"), &gate()), 401);
        let mut no_auth = head(&ok, "10");
        no_auth.authorization = None;
        assert_eq!(status(&no_auth, &gate()), 401);
        let off = Gate { enabled: false, ..gate() };
        let refused = check_head(&head(&ok, "10"), &off).unwrap_err();
        assert_eq!((refused.status, refused.code()), (403, "hangarzugang_aus".to_string()));
        // a wrong token is told before the switch
        assert_eq!(status(&head("Bearer x", "10"), &off), 401);
        let busy = Gate { ready: false, ..gate() };
        assert_eq!(status(&head(&ok, "10"), &busy), 503);
        assert_eq!(status(&head(&ok, &(MAX_BYTES + 1).to_string()), &gate()), 413);
        assert_eq!(check_head(&head(&ok, &MAX_BYTES.to_string()), &gate()), Ok(MAX_BYTES));
        let mut other = head(&ok, "1");
        other.path = "/api/v1/hangar".into();
        assert_eq!(status(&other, &gate()), 404);
        let mut get = head(&ok, "1");
        get.method = "GET".into();
        assert_eq!(status(&get, &gate()), 405);
        // an empty token in the settings never matches
        let empty = Gate { token: String::new(), ..gate() };
        assert_eq!(status(&head("Bearer ", "1"), &empty), 401);
    }

    #[test]
    fn parse_head_reads_headers_case_insensitive() {
        let h = parse_head(&format!(
            "POST {PATH}?x=1 HTTP/1.1\r\nhost: 127.0.0.1\r\nAUTHORIZATION: Bearer abc\r\ncontent-length: 5"
        ))
        .unwrap();
        assert_eq!(
            (h.path.as_str(), h.authorization.as_deref(), h.content_length.as_deref()),
            (PATH, Some("Bearer abc"), Some("5"))
        );
        assert!(parse_head("Kaputt").is_none());
    }

    #[test]
    fn body_answers() {
        let file = temp_file("body");
        let names = ShipNames::default();
        let code = |body: &str| {
            let (answer, _) = deliver(body.as_bytes(), &file, &names);
            (answer.status, answer.code())
        };
        assert_eq!(code("{}"), (400, "schema".into()));
        assert_eq!(code("kein json"), (400, "json".into()));
        assert_eq!(code("[1]"), (400, "json".into()));
        assert_eq!(code(r#"{"schema": "hangar-uebergabe", "version": 1}"#), (400, "uebergabe".into()));
        assert_eq!(code(r#"{"schema": "hangar-schiffe", "version": 2, "schiffe": []}"#), (400, "version".into()));
        assert_eq!(code(r#"{"schema": "hangar-schiffe", "version": true, "schiffe": []}"#), (400, "version".into()));
        assert_eq!(code(r#"{"schema": "hangar-schiffe"}"#), (400, "version".into()));
        assert_eq!(code(r#"{"schema": "hangar-schiffe", "version": 1}"#), (400, "schiffe".into()));
        assert_eq!(code(r#"{"schema": "hangar-schiffe", "version": 1, "schiffe": []}"#), (400, "leer".into()));
        assert!(!std::path::Path::new(&file).exists(), "refused bodies write nothing");
        let (first, delivered) = deliver(LIST.as_bytes(), &file, &names);
        assert_eq!(first, Answer { status: 200, body: r#"{"schiffe": 1, "unveraendert": false}"#.into() });
        assert_eq!(delivered.unwrap().imported.new, 1);
        let (second, _) = deliver(LIST.as_bytes(), &file, &names);
        assert_eq!(second.body, r#"{"schiffe": 1, "unveraendert": true}"#);
        let stored = std::fs::read_to_string(&file).unwrap();
        assert!(stored.contains("\"name\": \"Gladius\""));
        assert!(!stored.contains("pledgeId"));
    }

    #[test]
    fn unchanged_ignores_when_and_how_the_list_was_made() {
        let file = temp_file("unchanged");
        let names = ShipNames::default();
        let with = |made: &str, check: bool| {
            LIST.replacen(
                r#""version": 1,"#,
                &format!(r#""version": 1, "erzeugt": "{made}", "selbsttest": {{"ok": {check}}}, "werkzeug": {{"version": "0.4.0"}},"#),
                1,
            )
        };
        let (first, _) = deliver(with("2026-10-20T21:51:00Z", true).as_bytes(), &file, &names);
        assert_eq!(first.body, r#"{"schiffe": 1, "unveraendert": false}"#);
        let (second, _) = deliver(with("2026-10-21T08:00:00Z", false).as_bytes(), &file, &names);
        assert_eq!(second.body, r#"{"schiffe": 1, "unveraendert": true}"#);
        let more = with("2026-10-21T08:05:00Z", true).replacen(
            r#""insurance": "LTI"}"#,
            r#""insurance": "LTI"}, {"matrix": "Arrow", "matrixZuordnung": "sicher", "name": "Arrow", "code": "ANVL_Arrow",
         "manufacturer": {"code": "ANVL", "name": "Anvil Aerospace"}, "insurance": "LTI"}"#,
            1,
        );
        let (third, _) = deliver(more.as_bytes(), &file, &names);
        assert_eq!(third.body, r#"{"schiffe": 2, "unveraendert": false}"#);
    }

    #[test]
    fn answer_is_http() {
        let text = String::from_utf8(Answer::error(405, "Nur POST", "methode").to_http()).unwrap();
        assert!(text.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(text.contains("Allow: POST\r\n"));
        assert!(text.ends_with(r#"{"fehler": "Nur POST", "code": "methode"}"#));
    }
}
