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

//! The interface the Stream Deck plugin talks to ([`crate::streamdeck`]): HTTP/1.1 on `127.0.0.1`, one request
//! per connection. What the state holds and what a command does is the business of the [`Host`]; this module only
//! decides who may ask.
//!
//! | Check, in this order | Answer |
//! |---|---|
//! | head longer than 8 KiB, or no HTTP | 431 / 400 `anfrage` |
//! | `Host` not `127.0.0.1:<port>` or `localhost:<port>` | 400 `host` |
//! | an `Origin` (a web page asks) | 403 `origin` |
//! | locked after [`MAX_FAILS`] wrong keys, for [`LOCK`] | 429 `gesperrt` |
//! | key missing or wrong (the same answer for both, after [`FAIL_DELAY`]) | 401 `schluessel` |
//! | another path or method | 404 `pfad` / 405 `methode` |
//! | body longer than [`MAX_BODY`] or without a length | 413 `zu_gross` / 411 `laenge` |
//! | more than [`RATE`] commands of one action in a second | 429 `zu_oft` |
//!
//! ⚠ Listens on `127.0.0.1` only. The key is never written to a log, a message or an answer.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::streamdeck::{self, Command, MAX_BODY, PROTOCOL};

/// Wrong keys in a row before the interface locks.
pub const MAX_FAILS: u32 = 10;
/// How long it stays locked.
pub const LOCK: Duration = Duration::from_secs(60);
/// How long a wrong key waits for its answer.
pub const FAIL_DELAY: Duration = Duration::from_millis(250);
/// How long a request may take to arrive.
pub const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// Connections served at the same time.
pub const MAX_CONNECTIONS: usize = 8;
/// Commands of one action per second.
pub const RATE: usize = 5;
/// The plugin counts as connected while it asked for the state within this time.
pub const CONNECTED: Duration = Duration::from_secs(5);
const MAX_HEAD: usize = 8 * 1024;

/// What a refused command answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: u16,
    pub code: &'static str,
}

/// What the program behind the interface does.
pub trait Host: Send + Sync {
    /// The fields of the state without `version` and `rev` (they are added here).
    fn state(&self) -> Map<String, Value>;
    /// Does what the command asks.
    ///
    /// # Errors
    ///
    /// [`Refused`] with the status and the code for the plugin.
    fn command(&self, command: &Command) -> Result<(), Refused>;
    /// The bindings of the game actions.
    fn bindings(&self) -> Value;
    /// What the host adds to the report of the connection (the device hint, the game actions on the keys).
    fn report(&self) -> Map<String, Value> {
        Map::new()
    }
}

/// The part of the interface that remembers: key, failures, lock, rate, revision.
pub struct Server {
    key: Mutex<String>,
    enabled: std::sync::atomic::AtomicBool,
    port: Mutex<u16>,
    guard: Mutex<Guard>,
    revision: Mutex<(u64, String)>,
    connections: AtomicUsize,
    /// Seconds since the start of the server at the last request with the right key, plus one; 0 for none.
    contact: AtomicU64,
    started: Instant,
}

#[derive(Default)]
struct Guard {
    fails: u32,
    total_fails: u64,
    locked_until: Option<Instant>,
    rates: HashMap<String, VecDeque<Instant>>,
}

impl Server {
    pub fn new(key: String) -> Arc<Server> {
        Arc::new(Server {
            key: Mutex::new(key),
            enabled: std::sync::atomic::AtomicBool::new(true),
            port: Mutex::new(0),
            guard: Mutex::new(Guard::default()),
            revision: Mutex::new((0, String::new())),
            connections: AtomicUsize::new(0),
            contact: AtomicU64::new(0),
            started: Instant::now(),
        })
    }

    /// Switches the interface on or off; off, every request is refused before its key is looked at.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// A new key: the old one is refused from now on.
    pub fn set_secret(&self, key: String) {
        *self.key.lock().unwrap_or_else(PoisonError::into_inner) = key;
    }

    /// The port it listens on; 0 before [`serve`].
    pub fn port(&self) -> u16 {
        *self.port.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, Guard> {
        self.guard.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wrong keys since the start.
    pub fn failures(&self) -> u64 {
        self.guard().total_fails
    }

    /// Is the interface locked right now?
    pub fn locked(&self) -> bool {
        self.guard().locked_until.is_some_and(|until| Instant::now() < until)
    }

    /// How long ago the plugin asked last with the right key; `None` when it never did.
    pub fn last_contact(&self) -> Option<Duration> {
        let at = self.contact.load(Ordering::Relaxed);
        (at > 0).then(|| self.started.elapsed().saturating_sub(Duration::from_millis(at - 1)))
    }

    fn touch(&self) {
        let at = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX - 1);
        self.contact.store(at + 1, Ordering::Relaxed);
    }

    /// The state with its revision: the revision goes up when the content changed.
    fn stamped(&self, fields: Map<String, Value>) -> (u64, String) {
        let content = Value::Object(fields.clone()).to_string();
        let mut revision = self.revision.lock().unwrap_or_else(PoisonError::into_inner);
        if revision.1 != content {
            revision.0 += 1;
            revision.1 = content;
        }
        let mut whole = Map::new();
        whole.insert("version".into(), json!(PROTOCOL));
        whole.insert("rev".into(), json!(revision.0));
        whole.extend(fields);
        (revision.0, Value::Object(whole).to_string())
    }

    /// One more command of `key` now: false when there were [`RATE`] in the last second already.
    fn within_rate(&self, key: &str, now: Instant) -> bool {
        if key.is_empty() {
            return true;
        }
        let mut guard = self.guard();
        let times = guard.rates.entry(key.to_string()).or_default();
        while times.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1)) {
            times.pop_front();
        }
        if times.len() >= RATE {
            return false;
        }
        times.push_back(now);
        true
    }
}

/// Listens on `127.0.0.1` on a port the system hands out.
///
/// # Errors
///
/// The error of the system.
pub fn bind() -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
}

/// Answers requests until the listener fails, each connection in a thread of its own, at most
/// [`MAX_CONNECTIONS`] at once.
pub fn serve(listener: TcpListener, server: Arc<Server>, host: Arc<dyn Host>) {
    if let Ok(address) = listener.local_addr() {
        *server.port.lock().unwrap_or_else(PoisonError::into_inner) = address.port();
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if server.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            server.connections.fetch_sub(1, Ordering::SeqCst);
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        }
        let (own, host) = (server.clone(), host.clone());
        let spawned = std::thread::Builder::new().name("streamdeck-anfrage".into()).spawn(move || {
            handle(stream, &own, host.as_ref());
            own.connections.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
            server.connections.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// The parts of a request head that matter here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Head {
    method: String,
    path: String,
    query: String,
    host: Option<String>,
    origin: bool,
    authorization: Option<String>,
    content_length: Option<String>,
    chunked: bool,
}

fn parse_head(text: &str) -> Option<Head> {
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    if !first.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut head = Head { method, path: path.into(), query: query.into(), ..Head::default() };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim_matches([' ', '\t']).to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => head.host = Some(value),
            "origin" => head.origin = true,
            "authorization" => head.authorization = Some(value),
            "content-length" => head.content_length = Some(value),
            "transfer-encoding" => head.chunked = true,
            _ => {}
        }
    }
    Some(head)
}

/// An answer: status, body (empty for 304).
struct Answer {
    status: u16,
    body: String,
}

impl Answer {
    fn error(status: u16, code: &str) -> Answer {
        Answer { status, body: json!({"ok": false, "code": code}).to_string() }
    }

    fn to_http(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            411 => "Length Required",
            413 => "Payload Too Large",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            _ => "Service Unavailable",
        };
        let mut head = format!("HTTP/1.1 {} {reason}\r\nConnection: close\r\nCache-Control: no-store\r\n", self.status);
        if self.status != 304 {
            head.push_str(&format!(
                "Content-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n",
                self.body.len()
            ));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        if self.status != 304 {
            bytes.extend_from_slice(self.body.as_bytes());
        }
        bytes
    }
}

fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let host = host.to_ascii_lowercase();
    [format!("127.0.0.1:{port}"), format!("localhost:{port}")].contains(&host)
}

fn since(query: &str) -> Option<u64> {
    query.split('&').find_map(|pair| pair.strip_prefix("seit=")).and_then(|v| v.parse().ok())
}

/// One request on one connection.
fn handle(mut stream: TcpStream, server: &Server, host: &dyn Host) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break end;
        }
        if buffer.len() > MAX_HEAD {
            reply(&mut stream, &Answer::error(431, "kopf"));
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
    let rest = buffer[end + 4..].to_vec();
    let answer = answer(&head, rest, &mut stream, server, host, deadline);
    reply(&mut stream, &answer);
}

fn answer(
    head: &str,
    mut body: Vec<u8>,
    stream: &mut TcpStream,
    server: &Server,
    host: &dyn Host,
    deadline: Instant,
) -> Answer {
    let Some(head) = parse_head(head) else { return Answer::error(400, "anfrage") };
    let port = *server.port.lock().unwrap_or_else(PoisonError::into_inner);
    if !host_allowed(head.host.as_deref(), port) {
        return Answer::error(400, "host");
    }
    if head.origin {
        return Answer::error(403, "origin");
    }
    if !server.enabled() {
        return Answer::error(403, "aus");
    }
    if server.locked() {
        return Answer::error(429, "gesperrt");
    }
    let key = server.key.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let expected = format!("Bearer {key}");
    let given = head.authorization.as_deref().unwrap_or("");
    if !streamdeck::valid_key(&key) || !streamdeck::same_secret(given, &expected) {
        std::thread::sleep(FAIL_DELAY);
        let mut guard = server.guard();
        guard.fails += 1;
        guard.total_fails += 1;
        if guard.fails >= MAX_FAILS {
            guard.fails = 0;
            guard.locked_until = Some(Instant::now() + LOCK);
        }
        return Answer::error(401, "schluessel");
    }
    server.guard().fails = 0;
    match (head.method.as_str(), head.path.as_str()) {
        ("GET", streamdeck::PATH_REPORT) => {
            let mut report = host.report();
            report.insert("verbunden".into(), json!(server.last_contact().is_some_and(|ago| ago < CONNECTED)));
            report.insert("fehlversuche".into(), json!(server.failures()));
            Answer { status: 200, body: Value::Object(report).to_string() }
        }
        ("GET", streamdeck::PATH_STATE) => {
            // Only the plugin asks for the state: its asking is the sign that it is there.
            server.touch();
            let (rev, text) = server.stamped(host.state());
            if since(&head.query) == Some(rev) {
                return Answer { status: 304, body: String::new() };
            }
            Answer { status: 200, body: text }
        }
        ("GET", streamdeck::PATH_BINDINGS) => Answer { status: 200, body: host.bindings().to_string() },
        ("POST", streamdeck::PATH_COMMAND) => {
            let length = match (&head.content_length, head.chunked) {
                (_, true) | (None, _) => return Answer::error(411, "laenge"),
                (Some(text), false) => match text.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => return Answer::error(411, "laenge"),
                },
            };
            if length > MAX_BODY {
                return Answer::error(413, "zu_gross");
            }
            let mut chunk = [0u8; 1024];
            while body.len() < length {
                if Instant::now() >= deadline {
                    return Answer::error(400, "anfrage");
                }
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => return Answer::error(400, "anfrage"),
                    Ok(n) => body.extend_from_slice(&chunk[..n]),
                }
            }
            body.truncate(length);
            let command = match streamdeck::parse_command(&body) {
                Ok(command) => command,
                Err(refusal) => return Answer { status: 400, body: streamdeck::refusal_body(refusal) },
            };
            if !server.within_rate(&command.limit_key(), Instant::now()) {
                return Answer::error(429, "zu_oft");
            }
            match host.command(&command) {
                Ok(()) => Answer { status: 200, body: json!({"ok": true}).to_string() },
                Err(refused) => Answer::error(refused.status, refused.code),
            }
        }
        (_, streamdeck::PATH_STATE | streamdeck::PATH_BINDINGS | streamdeck::PATH_COMMAND | streamdeck::PATH_REPORT) => {
            Answer::error(405, "methode")
        }
        _ => Answer::error(404, "pfad"),
    }
}

fn reply(stream: &mut TcpStream, answer: &Answer) {
    let _ = stream.write_all(&answer.to_http());
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct Fake {
        commands: Mutex<Vec<Command>>,
        counter: AtomicU64,
    }

    impl Host for Fake {
        fn state(&self) -> Map<String, Value> {
            let mut map = Map::new();
            map.insert("sprache".into(), json!("de"));
            map.insert("zaehler".into(), json!(self.counter.load(Ordering::SeqCst)));
            map
        }

        fn command(&self, command: &Command) -> Result<(), Refused> {
            if let Command::GameAction { action, .. } = command
                && action == "unbekannt"
            {
                return Err(Refused { status: 400, code: "aktion" });
            }
            self.commands.lock().unwrap().push(command.clone());
            Ok(())
        }

        fn bindings(&self) -> Value {
            json!({"aktionen": []})
        }
    }

    fn start() -> (u16, Arc<Server>, Arc<Fake>) {
        let listener = bind().expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = Server::new(KEY.into());
        let fake = Arc::new(Fake { commands: Mutex::new(Vec::new()), counter: AtomicU64::new(0) });
        let (s, h): (Arc<Server>, Arc<dyn Host>) = (server.clone(), fake.clone());
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::spawn(move || {
            *s.port.lock().unwrap() = port;
            ready_tx.send(()).ok();
            serve(listener, s, h);
        });
        ready_rx.recv().expect("ready");
        (port, server, fake)
    }

    fn send(port: u16, raw: &str) -> (u16, String) {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        stream.write_all(raw.as_bytes()).expect("write");
        let mut text = String::new();
        stream.read_to_string(&mut text).expect("read");
        let status = text.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
        (status, body)
    }

    fn get(port: u16, path: &str, key: Option<&str>) -> (u16, String) {
        let auth = key.map(|k| format!("Authorization: Bearer {k}\r\n")).unwrap_or_default();
        send(port, &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}\r\n"))
    }

    fn post(port: u16, body: &str) -> (u16, String) {
        send(
            port,
            &format!(
                "POST /v1/befehl HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {KEY}\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    fn code(body: &str) -> String {
        serde_json::from_str::<Value>(body).ok().and_then(|v| v["code"].as_str().map(String::from)).unwrap_or_default()
    }

    #[test]
    fn a_missing_and_a_wrong_key_get_the_same_answer_and_the_key_is_in_no_answer() {
        let (port, server, _) = start();
        let missing = get(port, "/v1/zustand", None);
        let wrong = get(port, "/v1/zustand", Some(&KEY.replace('0', "1")));
        assert_eq!(missing, wrong);
        assert_eq!(missing.0, 401);
        assert_eq!(code(&missing.1), "schluessel");
        assert_eq!(server.failures(), 2);
        let (status, report) = get(port, "/v1/verbindung", Some(KEY));
        assert_eq!(status, 200);
        let report: Value = serde_json::from_str(&report).expect("json");
        assert_eq!((report["verbunden"].as_bool(), report["fehlversuche"].as_u64()), (Some(false), Some(2)));
        assert!(server.last_contact().is_none(), "the page asking is not the plugin");
        let right = get(port, "/v1/zustand", Some(KEY));
        assert_eq!(right.0, 200);
        assert!(!right.1.contains(KEY));
        assert!(server.last_contact().is_some());
        let (_, report) = get(port, "/v1/verbindung", Some(KEY));
        assert!(report.contains("\"verbunden\":true"), "{report}");
    }

    #[test]
    fn the_state_carries_version_and_revision_and_says_when_nothing_changed() {
        let (port, _, fake) = start();
        let (status, body) = get(port, "/v1/zustand", Some(KEY));
        assert_eq!(status, 200);
        let state = streamdeck::State::parse(&body).expect("state");
        let rev = state.rev;
        assert!(rev > 0);
        assert_eq!(get(port, &format!("/v1/zustand?seit={rev}"), Some(KEY)).0, 304);
        fake.counter.fetch_add(1, Ordering::SeqCst);
        let (status, body) = get(port, &format!("/v1/zustand?seit={rev}"), Some(KEY));
        assert_eq!(status, 200);
        assert_eq!(streamdeck::State::parse(&body).expect("state").rev, rev + 1);
    }

    #[test]
    fn a_web_page_and_a_foreign_host_name_are_refused_before_the_key_is_looked_at() {
        let (port, server, _) = start();
        let origin = send(
            port,
            &format!("GET /v1/zustand HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: https://example.org\r\nAuthorization: Bearer {KEY}\r\n\r\n"),
        );
        assert_eq!((origin.0, code(&origin.1)), (403, "origin".into()));
        let rebinding = send(
            port,
            &format!("GET /v1/zustand HTTP/1.1\r\nHost: evil.example:{port}\r\nAuthorization: Bearer {KEY}\r\n\r\n"),
        );
        assert_eq!((rebinding.0, code(&rebinding.1)), (400, "host".into()));
        let no_host = send(port, &format!("GET /v1/zustand HTTP/1.1\r\nAuthorization: Bearer {KEY}\r\n\r\n"));
        assert_eq!(no_host.0, 400);
        let localhost = send(
            port,
            &format!("GET /v1/zustand HTTP/1.1\r\nHost: localhost:{port}\r\nAuthorization: Bearer {KEY}\r\n\r\n"),
        );
        assert_eq!(localhost.0, 200);
        assert_eq!(server.failures(), 0, "refused before the key");
    }

    #[test]
    fn ten_wrong_keys_lock_the_interface_even_for_the_right_one() {
        let (port, server, _) = start();
        for _ in 0..MAX_FAILS {
            assert_eq!(get(port, "/v1/zustand", Some("falsch")).0, 401);
        }
        assert!(server.locked());
        let (status, body) = get(port, "/v1/zustand", Some(KEY));
        assert_eq!((status, code(&body)), (429, "gesperrt".into()));
    }

    #[test]
    fn a_command_arrives_and_a_bad_one_says_why() {
        let (port, _, fake) = start();
        assert_eq!(post(port, r#"{"id":"overlay"}"#).0, 200);
        assert_eq!(post(port, r#"{"id":"seite","seite":"hf_bergbau"}"#).0, 200);
        let page = post(port, r#"{"id":"seite","seite":"s_af_seite"}"#);
        assert_eq!((page.0, code(&page.1)), (400, "seite".into()));
        let unknown = post(port, r#"{"id":"spielaktion","aktion":"unbekannt","phase":"runter"}"#);
        assert_eq!((unknown.0, code(&unknown.1)), (400, "aktion".into()));
        let id = post(port, r#"{"id":"loeschen"}"#);
        assert_eq!((id.0, code(&id.1)), (400, "id".into()));
        assert_eq!(
            *fake.commands.lock().unwrap(),
            vec![Command::Overlay, Command::Page("hf_bergbau".into())]
        );
    }

    #[test]
    fn the_largest_report_of_game_actions_goes_through_and_one_more_action_is_refused() {
        let (port, _, fake) = start();
        let names = |count: usize| -> Vec<String> { (0..count).map(|n| format!("v_{n:04}_{}", "x".repeat(89))).collect() };
        let most = names(streamdeck::MAX_REPORTED_ACTIONS);
        let (status, _) = post(port, &streamdeck::command_body(&Command::GameActionsInUse(most.clone())));
        assert_eq!(status, 200);
        assert_eq!(*fake.commands.lock().unwrap(), vec![Command::GameActionsInUse(most)]);
        let one_more = streamdeck::command_body(&Command::GameActionsInUse(names(streamdeck::MAX_REPORTED_ACTIONS + 1)));
        let (status, body) = post(port, &one_more);
        assert_eq!((status, code(&body)), (400, "liste".into()));
        assert_eq!(fake.commands.lock().unwrap().len(), 1, "the refused report reaches no one");
    }

    #[test]
    fn a_body_too_large_or_without_length_is_refused_unread() {
        let (port, _, fake) = start();
        let big = format!(r#"{{"id":"overlay","x":"{}"}}"#, "a".repeat(MAX_BODY));
        let (status, body) = post(port, &big);
        assert_eq!((status, code(&body)), (413, "zu_gross".into()));
        let chunked = send(
            port,
            &format!("POST /v1/befehl HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {KEY}\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"),
        );
        assert_eq!(chunked.0, 411);
        assert!(fake.commands.lock().unwrap().is_empty());
    }

    #[test]
    fn more_than_five_commands_of_one_action_in_a_second_are_refused_and_others_still_pass() {
        let (port, _, fake) = start();
        let mut statuses = Vec::new();
        for _ in 0..RATE + 2 {
            statuses.push(post(port, r#"{"id":"overlay"}"#).0);
        }
        assert_eq!(statuses.iter().filter(|s| **s == 200).count(), RATE, "{statuses:?}");
        assert!(statuses[RATE..].iter().all(|s| *s == 429));
        assert_eq!(post(port, r#"{"id":"suche"}"#).0, 200, "another action is not held back");
        assert_eq!(fake.commands.lock().unwrap().len(), RATE + 1);
    }

    #[test]
    fn a_release_is_never_held_back_by_the_rate() {
        let (port, _, _) = start();
        for _ in 0..RATE * 2 {
            assert_eq!(post(port, r#"{"id":"spielaktion","aktion":"v_eject","phase":"hoch"}"#).0, 200);
        }
    }

    #[test]
    fn another_path_and_another_method_are_refused() {
        let (port, _, _) = start();
        assert_eq!(get(port, "/v1/geheim", Some(KEY)).0, 404);
        assert_eq!(get(port, "/v1/befehl", Some(KEY)).0, 405);
        assert_eq!(get(port, "/v1/belegung", Some(KEY)).0, 200);
    }

    #[test]
    fn switched_off_it_refuses_everything_and_a_new_key_replaces_the_old_one() {
        let (port, server, fake) = start();
        server.set_enabled(false);
        let (status, body) = get(port, "/v1/zustand", Some(KEY));
        assert_eq!((status, code(&body)), (403, "aus".into()));
        assert_eq!(server.failures(), 0);
        server.set_enabled(true);
        let other = KEY.replace('a', "b");
        server.set_secret(other.clone());
        assert_eq!(get(port, "/v1/zustand", Some(KEY)).0, 401, "the old key no longer");
        assert_eq!(get(port, "/v1/zustand", Some(&other)).0, 200);
        assert!(fake.commands.lock().unwrap().is_empty());
    }

    #[test]
    fn it_listens_on_the_own_computer_only() {
        let listener = bind().expect("bind");
        assert!(listener.local_addr().expect("addr").ip().is_loopback());
    }
}
