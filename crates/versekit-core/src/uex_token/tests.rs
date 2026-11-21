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

//! Every token here is made up; no request leaves the test.

use std::sync::Mutex;

use super::*;
use crate::basetool::secrets::MemorySecrets;
use crate::paths::{Host, Paths};
use crate::pypath::Flavor;

/// The token of the running program is one for the whole process: the checks that set it take turns.
static TURN: Mutex<()> = Mutex::new(());

fn turn() -> std::sync::MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

const MADE_UP: &str = "vk-test-7f3a9c1e5b2d4086a1c3e5f7091b2d4f";
const UEX: &str = "https://api.uexcorp.uk/2.0/commodities";

fn token() -> Token {
    Token::parse(MADE_UP).unwrap()
}

/// A stand-in for UEX: answers by what is sent and records each request as (address, token sent).
struct Wire {
    asked: Mutex<Vec<(String, Option<String>)>>,
    with_token: Result<Vec<u8>, (u16, FetchError)>,
    without_token: Result<Vec<u8>, (u16, FetchError)>,
}

impl Wire {
    fn new(with_token: Result<Vec<u8>, (u16, FetchError)>, without_token: Result<Vec<u8>, (u16, FetchError)>) -> Wire {
        Wire { asked: Mutex::new(Vec::new()), with_token, without_token }
    }

    fn ok() -> Wire {
        Wire::new(Ok(br#"{"status":"ok","data":[]}"#.to_vec()), Ok(br#"{"status":"ok","data":[]}"#.to_vec()))
    }

    fn send(&self, url: &str, bearer: Option<&str>) -> Result<Vec<u8>, (u16, FetchError)> {
        self.asked.lock().unwrap().push((url.to_string(), bearer.map(str::to_string)));
        if bearer.is_some() { self.with_token.clone() } else { self.without_token.clone() }
    }

    fn asked(&self) -> Vec<(String, Option<String>)> {
        self.asked.lock().unwrap().clone()
    }
}

fn denied(status: u16) -> Result<Vec<u8>, (u16, FetchError)> {
    Err((status, if status == 403 { FetchError::Refused } else { FetchError::Failed(format!("HTTP {status}")) }))
}

#[test]
fn a_token_is_cleaned_and_one_that_cannot_stand_in_a_header_is_refused() {
    assert_eq!(Token::parse(&format!("  {MADE_UP}\n")), Some(token()));
    assert_eq!(Token::parse(&format!("Bearer {MADE_UP}")), Some(token()), "copied with the word before it");
    assert_eq!(Token::parse(&format!("bearer  {MADE_UP}")), Some(token()));
    let too_short = "x".repeat(15);
    for bad in [
        "",
        "   ",
        "zwei teile des tokens",
        "zeile\numbruch-im-token",
        "steuer\u{7}zeichen-im-token",
        "ümlaut-im-token-0000",
        &"x".repeat(513),
        &too_short,
    ] {
        assert_eq!(Token::parse(bad), None, "{bad:?}");
    }
    assert!(Token::parse(&"x".repeat(512)).is_some());
    assert!(Token::parse(&"x".repeat(16)).is_some());
}

#[test]
fn no_text_of_a_token_shows_it() {
    let shown = format!("{:?} {:?}", token(), Some(token()));
    assert!(!shown.contains(MADE_UP) && shown.contains("<geheim>"), "{shown}");
}

#[test]
fn without_a_token_every_request_goes_out_as_before() {
    let _turn = turn();
    activate(None);
    let wire = Wire::ok();
    assert!(fetch_through(UEX, &|u, b| wire.send(u, b)).is_ok());
    assert_eq!(wire.asked(), vec![(UEX.to_string(), None)]);
    assert!(!is_set() && !refused());
}

#[test]
fn the_token_goes_to_uex_and_nowhere_else() {
    let _turn = turn();
    activate(Some(token()));
    let wire = Wire::ok();
    let elsewhere = [
        "https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data/x.json",
        "https://scmdb.net/data/x.json",
        "http://api.uexcorp.uk/2.0/commodities",
        "https://api.uexcorp.uk.example.org/2.0/commodities",
        "https://uexcorp.space/api/2.0/commodities",
        "https://api.uexcorp.uk",
        "cig_data_issues-4.10.2-live.12913470.json",
    ];
    for url in [UEX].into_iter().chain(elsewhere) {
        let _ = fetch_through(url, &|u, b| wire.send(u, b));
    }
    let asked = wire.asked();
    assert_eq!(asked[0], (UEX.to_string(), Some(MADE_UP.to_string())));
    for (url, sent) in &asked[1..] {
        assert_eq!(sent, &None, "the token went to {url}");
    }
    assert_eq!(asked.len(), 1 + elsewhere.len(), "one request each, no second try");
    activate(None);
}

#[test]
fn a_token_refused_with_401_falls_back_and_is_noted_once() {
    let _turn = turn();
    activate(Some(token()));
    let wire = Wire::new(denied(401), Ok(br#"{"status":"ok","data":[1]}"#.to_vec()));
    let (results, caught) = crate::errors::captured(|| {
        (0..3).map(|_| fetch_through(UEX, &|u, b| wire.send(u, b))).collect::<Vec<_>>()
    });
    assert!(results.iter().all(Result::is_ok), "the lists come without the token: {results:?}");
    assert!(refused());
    // The first request with the token, again without, then only without.
    let sent: Vec<bool> = wire.asked().iter().map(|(_, b)| b.is_some()).collect();
    assert_eq!(sent, vec![true, false, false, false]);
    assert_eq!(caught.len(), 1, "one note, no count of errors: {caught:?}");
    assert!(caught[0].note && caught[0].label == NOTE_REFUSED && !caught[0].text.contains(MADE_UP));
    // Another token is tried again.
    activate(Some(Token::parse("vk-test-anderer-0000").unwrap()));
    assert!(!refused());
    let _ = fetch_through(UEX, &|u, b| wire.send(u, b));
    assert_eq!(wire.asked().last().map(|(_, b)| b.is_some()), Some(false));
    assert_eq!(wire.asked().iter().rev().nth(1).and_then(|(_, b)| b.clone()).as_deref(), Some("vk-test-anderer-0000"));
    activate(None);
}

/// Two requests on their way at once, both refused: the token is noted as refused once, not once per request.
#[test]
fn two_requests_refused_at_the_same_time_note_it_once() {
    let _turn = turn();
    activate(Some(token()));
    let wire = Wire::new(denied(401), Ok(br#"{"status":"ok","data":[]}"#.to_vec()));
    let inner_done = std::sync::atomic::AtomicBool::new(false);
    // The first answer comes only after a second request with the same token went out and was refused.
    let send = |url: &str, bearer: Option<&str>| {
        if bearer.is_some() && !inner_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let _ = fetch_through(url, &|u, b| wire.send(u, b));
        }
        wire.send(url, bearer)
    };
    let (result, caught) = crate::errors::captured(|| fetch_through(UEX, &send));
    assert!(result.is_ok() && refused());
    let with_token = wire.asked().iter().filter(|(_, b)| b.is_some()).count();
    assert_eq!(with_token, 2, "both went out with the token");
    assert_eq!(caught.len(), 1, "one note for both: {caught:?}");
    activate(None);
}

#[test]
fn a_403_counts_against_the_token_only_where_the_request_without_it_gets_through() {
    let _turn = turn();
    // Without the token it works: the token was the reason.
    activate(Some(token()));
    let wire = Wire::new(denied(403), Ok(br#"{"status":"ok","data":[]}"#.to_vec()));
    assert!(fetch_through(UEX, &|u, b| wire.send(u, b)).is_ok());
    assert!(refused());
    // Refused either way (a used-up limit, a block): not the token's fault, it stays in use.
    activate(Some(token()));
    let wire = Wire::new(denied(403), denied(403));
    let (result, caught) = crate::errors::captured(|| fetch_through(UEX, &|u, b| wire.send(u, b)));
    assert_eq!(result, Err(FetchError::Refused));
    assert!(!refused() && caught.is_empty(), "{caught:?}");
    // Another failure is no refusal: no second request.
    let wire = Wire::new(Err((500, FetchError::Failed("HTTP 500".into()))), Wire::ok().without_token);
    assert!(fetch_through(UEX, &|u, b| wire.send(u, b)).is_err());
    assert_eq!(wire.asked().len(), 1);
    assert!(!refused());
    activate(None);
}

#[test]
fn the_button_tells_accepted_refused_offline_and_failed() {
    let wire = Wire::ok();
    assert_eq!(check(&token(), &|u, b| wire.send(u, b)), Check::Accepted);
    assert_eq!(wire.asked(), vec![(CHECK_URL.to_string(), Some(MADE_UP.to_string()))]);
    let wire = Wire::new(denied(401), denied(401));
    assert_eq!(check(&token(), &|u, b| wire.send(u, b)), Check::Refused);
    let wire = Wire::new(denied(403), Wire::ok().without_token);
    assert_eq!(check(&token(), &|u, b| wire.send(u, b)), Check::Refused);
    let wire = Wire::new(denied(403), denied(403));
    assert!(matches!(check(&token(), &|u, b| wire.send(u, b)), Check::Failed(_)), "refused either way");
    let wire = Wire::new(Err((0, FetchError::Offline)), Err((0, FetchError::Offline)));
    assert_eq!(check(&token(), &|u, b| wire.send(u, b)), Check::Offline);
    let wire = Wire::new(Ok(b"<html>".to_vec()), Ok(Vec::new()));
    assert!(matches!(check(&token(), &|u, b| wire.send(u, b)), Check::Failed(_)), "no answer of UEX");
    let wire = Wire::new(Ok(br#"{"status":"requests_limit_reached"}"#.to_vec()), Ok(Vec::new()));
    assert!(matches!(check(&token(), &|u, b| wire.send(u, b)), Check::Failed(_)));
}

/// Data folder and store of secrets, each of its own below a scratch folder; the copy of the data if `copy`.
fn machine(name: &str, copy: bool) -> (Paths, std::path::PathBuf, std::path::PathBuf, crate::testdir::Dir) {
    let dir = crate::testdir::Dir::new(&format!("uex-token-{name}"));
    let data = dir.path().join("daten");
    let secrets = dir.path().join("geheim");
    std::fs::create_dir_all(&data).unwrap();
    let (data_text, secrets_text) = (data.to_string_lossy().into_owned(), secrets.to_string_lossy().into_owned());
    let root = dir.path().to_string_lossy().into_owned();
    let vars = [
        (crate::envvars::ENV_HOME, data_text.as_str()),
        (crate::envvars::ENV_SECRETS, secrets_text.as_str()),
        (crate::envvars::ENV_SECRETS_FILE, "1"),
        ("HOME", root.as_str()),
        ("USERPROFILE", root.as_str()),
    ];
    let host = Host::new(Flavor::native(), &vars, &root);
    let paths = if copy { Paths::for_copy(host, &data_text).unwrap() } else { Paths::new(host) };
    (paths, data, secrets, dir)
}

/// Every file below `folder` with its bytes.
fn all_files(folder: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(all_files(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push((path, bytes));
        }
    }
    out
}

fn holds(bytes: &[u8], text: &str) -> bool {
    bytes.windows(text.len()).any(|w| w == text.as_bytes())
}

#[test]
fn the_token_is_kept_in_the_store_of_secrets_never_in_the_data_folder_or_the_backup() {
    let _turn = turn();
    let (paths, data, secrets_folder, _dir) = machine("ablage", false);
    let store = SystemSecrets::from_host(&paths.host);
    assert!(!secrets_folder.starts_with(&data));
    assert!(paths.set_setting("sprache", crate::jsonio::Value::str("de")));
    save(&store, &token()).unwrap();
    assert!(is_set());
    assert_eq!(load(&paths, &store), Some(token()), "read back from the store");
    // Windows seals it: not even the store holds the plain text there.
    let kept = all_files(&secrets_folder);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept.iter().any(|(_, bytes)| holds(bytes, MADE_UP)), !cfg!(windows));
    // Nothing of the data folder holds it, and so nothing the backup takes along.
    for (file, bytes) in all_files(&data) {
        assert!(!holds(&bytes, MADE_UP), "{} holds the token", file.display());
    }
    let taken = crate::backup::files(&data.to_string_lossy()).unwrap();
    assert!(!taken.is_empty(), "the backup has something to take");
    for (full, _) in &taken {
        assert!(!std::path::Path::new(full).starts_with(&secrets_folder));
        assert!(!holds(&std::fs::read(full).unwrap(), MADE_UP), "{full}");
    }
    forget(&store).unwrap();
    assert!(!is_set());
    assert_eq!(load(&paths, &store), None);
    assert!(all_files(&secrets_folder).is_empty(), "deleted from the store");
}

#[test]
fn a_copy_of_the_data_never_loads_the_token() {
    let secrets = MemorySecrets::default();
    secrets.save(SECRET, MADE_UP).unwrap();
    let (own, ..) = machine("eigen", false);
    assert_eq!(load(&own, &secrets), Some(token()));
    let (copy, ..) = machine("kopie", true);
    assert_eq!(load(&copy, &secrets), None);
}

#[test]
fn a_store_that_cannot_keep_it_leaves_the_token_of_before() {
    struct Refusing;
    impl SecretStore for Refusing {
        fn save(&self, _: &str, _: &str) -> Result<(), SecretError> {
            Err(SecretError { code: "SECRET_REFUSED", message: "locked".into() })
        }
        fn load(&self, _: &str) -> Result<Option<String>, SecretError> {
            Ok(None)
        }
        fn delete(&self, _: &str) -> Result<(), SecretError> {
            Ok(())
        }
        fn uses_file(&self) -> bool {
            false
        }
    }
    let _turn = turn();
    activate(None);
    assert!(save(&Refusing, &token()).is_err());
    assert!(!is_set(), "not used when it is not kept");
}

#[test]
fn the_error_log_and_every_redacted_text_lose_the_token() {
    let _turn = turn();
    let (paths, data, _, _dir) = machine("bericht", false);
    activate(Some(token()));
    let redactor = crate::redact::Redactor::new(&paths);
    for text in [
        format!("request failed: {MADE_UP}"),
        format!("Authorization: Bearer {MADE_UP}"),
        format!("{{\"token\":\"{MADE_UP}\"}}"),
    ] {
        let said = redactor.redact(&text);
        assert!(!said.contains(MADE_UP) && said.contains("<geheim>"), "{said}");
    }
    let log = crate::error_log::ErrorLog::new(&paths, "5.0.0");
    let problem = crate::error_log::Problem { kind: "Failed", message: &format!("uex said no to {MADE_UP}"), trace: None };
    assert!(log.record("uex.fetch.test", Some(problem), &format!("note {MADE_UP}")));
    for (file, bytes) in all_files(&data) {
        assert!(!holds(&bytes, MADE_UP), "{} holds the token", file.display());
    }
    activate(None);
}

/// Only the redaction reads the token as text; nothing else can hand it on.
#[test]
fn only_the_redaction_reads_the_token_as_text() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for crate_dir in std::fs::read_dir(&src).unwrap().flatten() {
        for (file, bytes) in all_files(&crate_dir.path().join("src")) {
            let name = file.to_string_lossy().replace('\\', "/");
            let text = String::from_utf8_lossy(&bytes);
            let own = name.ends_with("/uex_token.rs") || name.contains("/uex_token/");
            if text.contains("secret_for_redaction()") && !own {
                found.push(name);
            }
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].ends_with("versekit-core/src/redact.rs"), "{found:?}");
}
