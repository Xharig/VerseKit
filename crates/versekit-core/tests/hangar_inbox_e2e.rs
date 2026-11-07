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

//! The hangar delivery end to end: a receiver on a free local port, the
//! requests the browser extension sends, every answer of the table, and the
//! ships in `hangar.json` afterwards.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use versekit_core::hangar_inbox::{self as inbox, Event, Files, Gate, Inbox, PATH};
use versekit_core::{hangar, writeguard};

const LIST: &str = include_str!("daten/hangar-schiffe-v1.json");

fn post(port: u16, path: &str, auth: Option<&str>, body: &[u8], length: Option<usize>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut head = format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n");
    if let Some(auth) = auth {
        head.push_str(&format!("Authorization: {auth}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", length.unwrap_or(body.len())));
    stream.write_all(head.as_bytes()).unwrap();
    let _ = stream.write_all(body);
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    let status = answer.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let body = answer.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

fn code(body: &str) -> String {
    versekit_core::jsonio::loads(body)
        .ok()
        .and_then(|v| v.get("code").and_then(|c| c.as_str().map(str::to_string)))
        .unwrap_or_default()
}

/// A fresh folder of this test run under the build's own temporary folder.
fn fresh_dir(name: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}-{stamp}", versekit_core::testdir::run_tag()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A receiver on a free port with the given write lock and time limit.
fn start(file: &str, token: &str, guard: writeguard::State, limit: Duration) -> (u16, Arc<Inbox>) {
    let shared = Inbox::new(Gate { enabled: true, token: token.to_string(), ready: true });
    let listener = inbox::bind(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
    let files = Files { hangar: file.to_string(), ship_catalog: String::new() };
    let thread_inbox = Arc::clone(&shared);
    std::thread::spawn(move || inbox::serve_with(listener, thread_inbox, files, guard, limit, || {}));
    (port, shared)
}

/// The write lock of the starting thread holds in the receiver thread: a
/// hangar file outside the allowed folder is not written.
#[test]
fn the_write_lock_reaches_the_receiver() {
    let allowed = fresh_dir("einlieferung-erlaubt");
    let outside = fresh_dir("einlieferung-draussen");
    writeguard::deny(true);
    writeguard::allow_under(Some(&allowed.to_string_lossy()));
    let guard = writeguard::state();
    writeguard::deny(false);
    writeguard::allow_under(None);
    let token = inbox::new_token().unwrap();
    let auth = format!("Bearer {token}");
    let blocked = outside.join("hangar.json").to_string_lossy().into_owned();
    let (port, shared) = start(&blocked, &token, guard.clone(), inbox::DEADLINE);
    let (status, body) = post(port, PATH, Some(&auth), LIST.as_bytes(), None);
    assert_eq!((status, code(&body).as_str()), (503, "nicht_bereit"));
    assert!(!std::path::Path::new(&blocked).exists(), "nothing written outside");
    assert!(matches!(shared.last(), Some(Event::Refused { status: 503, .. })));
    let inside = allowed.join("hangar.json").to_string_lossy().into_owned();
    let (port, _) = start(&inside, &token, guard, inbox::DEADLINE);
    assert_eq!(post(port, PATH, Some(&auth), LIST.as_bytes(), None).0, 200);
    assert!(std::path::Path::new(&inside).exists());
}

/// A client that sends nothing holds the receiver only until the time limit.
#[test]
fn a_slow_client_is_cut_off() {
    let dir = fresh_dir("einlieferung-frist");
    let file = dir.join("hangar.json").to_string_lossy().into_owned();
    let token = inbox::new_token().unwrap();
    let (port, _) = start(&file, &token, writeguard::state(), Duration::from_millis(400));
    let mut slow = TcpStream::connect(("127.0.0.1", port)).unwrap();
    slow.write_all(b"POST /api/v1/hangar/uebergabe HTTP/1.1\r\nHost: x\r\n").unwrap();
    let begun = Instant::now();
    let auth = format!("Bearer {token}");
    let (status, _) = post(port, PATH, Some(&auth), b"{}", None);
    assert_eq!(status, 400, "the next request is answered");
    assert!(begun.elapsed() < Duration::from_secs(5), "took {:?}", begun.elapsed());
    slow.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut rest = Vec::new();
    let _ = slow.read_to_end(&mut rest);
    assert!(rest.is_empty(), "the slow client gets no answer, only the end");
}

#[test]
fn every_answer_over_the_wire() {
    let dir = fresh_dir("einlieferung");
    let file = dir.join("hangar.json").to_string_lossy().into_owned();
    // A ship bought in the game and a wish list must survive the delivery.
    std::fs::write(
        &file,
        r#"{"format": 1, "schiffe": [{"name": "Avenger Titan", "hersteller": "", "herkunft": "ingame", "belegung": {}}], "wunsch": [{"name": "Polaris"}]}"#,
    )
    .unwrap();
    let token = inbox::new_token().unwrap();
    let (port, shared) = start(&file, &token, writeguard::state(), inbox::DEADLINE);
    let bearer = format!("Bearer {token}");
    let auth = Some(bearer.as_str());

    // The pairing code the settings show leads here.
    assert_eq!(inbox::parse_pairing_code(&inbox::pairing_code(port, &token)), Some((port, token.clone())));

    // The probe: an incomplete file is refused, which means the connection stands.
    assert_eq!(post(port, PATH, auth, b"{}", None).0, 400);
    assert!(matches!(shared.last(), Some(Event::Refused { status: 400, ref code, .. }) if code == "schema"));

    let (status, body) = post(port, PATH, auth, LIST.as_bytes(), None);
    assert_eq!((status, body.as_str()), (200, r#"{"schiffe": 5, "unveraendert": false}"#));
    assert_eq!(shared.deliveries(), 1);
    let (status, body) = post(port, PATH, auth, LIST.as_bytes(), None);
    assert_eq!((status, body.as_str()), (200, r#"{"schiffe": 5, "unveraendert": true}"#));
    assert_eq!(shared.deliveries(), 1, "an unchanged delivery writes nothing");

    let data = hangar::read(&file).expect("hangar.json");
    let names: Vec<&str> = hangar::ships(&data).iter().map(|s| hangar::field_text(s, "name")).collect();
    for name in ["Avenger Titan", "Cutlass Black", "Carrack", "Gladius", "600i Explorer", "G12", "Freelancer MAX"] {
        assert!(names.contains(&name), "{name} missing in {names:?}");
    }
    // The catalog vehicles of the Carrack follow it.
    assert!(names.contains(&"C8 Pisces"), "{names:?}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"wunsch\""), "the wish list stays");
    assert!(!text.contains("pledgeId") && !text.contains(&token), "no pledge number, no token");
    if let Some(Event::Delivered { delivered, .. }) = shared.last() {
        assert_eq!(delivered.approximations, vec!["Freelancer MAX (Freelancer MAX Anniversary)".to_string()]);
    } else {
        panic!("no delivery noted");
    }

    let wrong = format!("Bearer {}", "0".repeat(64));
    assert_eq!(post(port, PATH, Some(&wrong), LIST.as_bytes(), None).0, 401);
    assert_eq!(post(port, PATH, None, LIST.as_bytes(), None).0, 401);
    let version_two = LIST.replacen("\"version\": 1", "\"version\": 2", 1);
    let (status, body) = post(port, PATH, auth, version_two.as_bytes(), None);
    assert_eq!((status, code(&body).as_str()), (400, "version"));
    let transfer = br#"{"schema": "hangar-uebergabe", "version": 1, "pledges": []}"#;
    let (status, body) = post(port, PATH, auth, transfer, None);
    assert_eq!((status, code(&body).as_str()), (400, "uebergabe"));
    let (status, body) = post(port, PATH, auth, b"kein JSON", None);
    assert_eq!((status, code(&body).as_str()), (400, "json"));
    let (status, body) = post(port, "/anders", auth, b"{}", None);
    assert_eq!((status, code(&body).as_str()), (404, "pfad"));
    // Too big: answered from the header, before the body.
    let (status, body) = post(port, PATH, auth, b"{}", Some(inbox::MAX_BYTES + 1));
    assert_eq!((status, code(&body).as_str()), (413, "zu_gross"));

    // Removing ships: first a copy of the hangar, then the write.
    let backup = versekit_core::jsonio::backup_name(&file);
    assert!(!std::path::Path::new(&backup).exists(), "no copy while nothing was removed");
    let only_one = r#"{"schema": "hangar-schiffe", "version": 1, "schiffe": [
        {"matrix": "Cutlass Black", "matrixZuordnung": "sicher", "name": "Cutlass Black", "code": "DRAK_Cutlass_Black",
         "manufacturer": {"code": "DRAK", "name": "Drake Interplanetary"}, "insurance": "120MI"}]}"#;
    assert_eq!(post(port, PATH, auth, only_one.as_bytes(), None).0, 200);
    let kept = std::fs::read_to_string(&backup).expect("hangar.bak.json");
    assert!(kept.contains("\"Carrack\""), "the copy holds the hangar before");
    assert!(!std::fs::read_to_string(&file).unwrap().contains("\"Carrack\""));
    assert!(std::fs::read_to_string(&file).unwrap().contains("Avenger Titan"), "bought in the game stays");

    shared.set_gate(Gate { enabled: false, token: token.clone(), ready: true });
    let (status, body) = post(port, PATH, auth, LIST.as_bytes(), None);
    assert_eq!((status, code(&body).as_str()), (403, "hangarzugang_aus"));
    shared.set_gate(Gate { enabled: true, token: token.clone(), ready: false });
    assert_eq!(post(port, PATH, auth, LIST.as_bytes(), None).0, 503);

    // A renewed code: the old one no longer fits.
    let renewed = inbox::new_token().unwrap();
    shared.set_gate(Gate { enabled: true, token: renewed.clone(), ready: true });
    assert_eq!(post(port, PATH, auth, LIST.as_bytes(), None).0, 401);
    assert_eq!(post(port, PATH, Some(&format!("Bearer {renewed}")), LIST.as_bytes(), None).0, 200);

    // Counter-check: the file really is what the page reads.
    assert_eq!(hangar::read(&file).map(|d| hangar::ships(&d).len()), Some(names.len()));
}
