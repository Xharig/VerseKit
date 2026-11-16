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

//! Secure by default: without HTTPS only in the explicit test mode; the security headers on every answer; the
//! health check; the public status page says only "setup incomplete" while the administration names the fault; the
//! program makes its admin password on the first start and names in the log only where it lies.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use versekit_orgserver::http::{Request, Response};
use versekit_orgserver::{Config, Server};

const T0: u64 = 1_800_000_000;

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string())
}

fn header<'a>(answer: &'a Response, name: &str) -> Option<&'a str> {
    answer.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn text(answer: &Response) -> String {
    String::from_utf8_lossy(&answer.body).into_owned()
}

fn get(server: &Server, path: &str, headers: &[(&str, &str)]) -> Response {
    server.handle_at(&Request::new("GET", path, headers, b""), T0)
}

#[test]
fn without_https_the_server_runs_only_in_the_test_mode() {
    let plain = Config::new("http://127.0.0.1:8110", "Testorga", "TST");
    let why = plain.clone().with_env(env(&[])).unwrap_err();
    assert!(why.contains("VERSEKIT_ORG_TESTBETRIEB=1"), "the error names the way: {why}");
    let test = plain.with_env(env(&[("VERSEKIT_ORG_TESTBETRIEB", "1")])).unwrap();
    assert!(test.test_mode && !test.require_https);
    let server = Server::new(test);
    let page = text(&get(&server, "/", &[("Accept-Language", "de")]));
    assert!(page.contains("Testbetrieb"), "the status page says it: {page}");
    assert!(header(&get(&server, "/", &[]), "Strict-Transport-Security").is_none());

    let real = Config::new("https://orga.example.org", "Orga", "ORG").with_env(env(&[])).unwrap();
    assert!(real.require_https && !real.test_mode);
    let server = Server::new(real);
    let direct = get(&server, "/", &[]);
    assert_eq!(direct.status, 403, "a request that did not come over HTTPS");
    assert!(text(&direct).contains("HTTPS"));
    let api = server.handle_at(&Request::new("POST", "/versekit/v1/device/code", &[("X-Forwarded-Proto", "http")], b"scope=stock%3Aread"), T0);
    assert_eq!(api.status, 403);
    for headers in [&[("X-Forwarded-Proto", "https")][..], &[("Forwarded", "for=1.2.3.4;proto=https")][..]] {
        let answer = get(&server, "/", headers);
        assert_eq!(answer.status, 200);
        assert_eq!(header(&answer, "Strict-Transport-Security"), Some("max-age=15552000"));
    }
    assert_eq!(get(&server, "/healthz", &[]).status, 200, "the health check from inside the container");
}

#[test]
fn every_answer_carries_the_security_headers() {
    let server = Server::new(Config::new("https://orga.example.org", "Orga", "ORG").with_env(env(&[])).unwrap());
    let https = [("X-Forwarded-Proto", "https")];
    for path in ["/", "/device", "/admin/login", "/.well-known/versekit-org.json", "/versekit/v1/me", "/nirgends"] {
        let answer = get(&server, path, &https);
        assert_eq!(header(&answer, "X-Content-Type-Options"), Some("nosniff"), "{path}");
        assert_eq!(header(&answer, "Referrer-Policy"), Some("same-origin"), "{path}");
        assert_eq!(header(&answer, "X-Frame-Options"), Some("DENY"), "{path}");
        assert_eq!(header(&answer, "Strict-Transport-Security"), Some("max-age=15552000"), "{path}: six months");
        assert_eq!(header(&answer, "Permissions-Policy"), Some("camera=(), microphone=(), geolocation=()"), "{path}");
        let policy = header(&answer, "Content-Security-Policy").unwrap_or_default();
        assert!(policy.contains("default-src 'none'") && policy.contains("frame-ancestors 'none'"), "{path}: {policy}");
        assert!(!policy.contains("http") && !policy.contains('*'), "no other source: {policy}");
    }
}

#[test]
fn the_public_page_says_only_setup_incomplete_and_the_administration_names_the_fault() {
    let config = Config::new("https://orga.example.org", "Orga", "ORG")
        .with_env(env(&[("VERSEKIT_ORG_ADMIN_PASSWORD", "orga-leitung-passwort-42")]))
        .unwrap();
    let server = Server::new(config);
    let https = [("X-Forwarded-Proto", "https"), ("Accept-Language", "de")];
    assert!(!text(&get(&server, "/", &https)).contains("Einrichtung unvollständig"));
    assert_eq!(get(&server, "/device", &[]).status, 403, "a request past the proxy");
    let page = text(&get(&server, "/", &https));
    assert!(page.contains("Einrichtung unvollständig"), "{page}");
    assert!(!page.contains("X-Forwarded-Proto") && !page.contains("abgewiesen"), "no detail in public: {page}");

    let login = text(&get(&server, "/admin/login", &https));
    let token = login.split("name=\"csrf\" value=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let cookie = format!("vk_admin_login={token}");
    let body = format!("csrf={token}&password=orga-leitung-passwort-42");
    let headers = [("X-Forwarded-Proto", "https"), ("Cookie", cookie.as_str()), ("Accept-Language", "de")];
    let answer = server.handle_at(&Request::new("POST", "/admin/login", &headers, body.as_bytes()), T0);
    assert_eq!(answer.status, 303);
    let session = header(&answer, "Set-Cookie").unwrap().split(';').next().unwrap().to_string();
    let overview = text(&get(&server, "/admin", &[("X-Forwarded-Proto", "https"), ("Cookie", session.as_str()), ("Accept-Language", "de")]));
    assert!(overview.contains("1 Anfragen ohne HTTPS abgewiesen"), "{overview}");
    assert!(overview.contains("Uhr plausibel") && overview.contains("Admin-Passwort aus der Umgebung"), "{overview}");
}

const KEY: &str = "schluessel-des-testpiloten";

fn member_server() -> Server {
    Server::new(Config::new("https://orga.example.org", "Orga", "ORG").with_member("tester", "Testpilot", KEY))
}

fn post(server: &Server, path: &str, body: &str, now: u64) -> Response {
    server.handle_at(&Request::new("POST", path, &[("Accept-Language", "de")], body.as_bytes()), now)
}

/// A login of the test member at `now`: the refresh token.
fn logged_in(server: &Server, now: u64) -> String {
    let code = post(server, "/versekit/v1/device/code", "scope=stock%3Aread", now).json_body();
    assert!(server.approve(code["user_code"].as_str().unwrap(), "tester", None));
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&device_code={}",
        code["device_code"].as_str().unwrap()
    );
    post(server, "/versekit/v1/token", &form, now).json_body()["refresh_token"].as_str().unwrap().to_string()
}

fn refresh(server: &Server, token: &str, now: u64) -> serde_json::Value {
    post(server, "/versekit/v1/token", &format!("grant_type=refresh_token&refresh_token={token}"), now).json_body()
}

#[test]
fn a_refresh_token_used_again_ends_the_login_and_an_unused_one_runs_out() {
    let server = member_server();
    let first = logged_in(&server, T0);
    let second = refresh(&server, &first, T0 + 10)["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(refresh(&server, &first, T0 + 20)["error"], "invalid_grant", "the old one is worthless");
    assert_eq!(refresh(&server, &second, T0 + 30)["error"], "invalid_grant", "its second use ended the whole login");

    let idle = logged_in(&server, T0);
    let ninety_days = 90 * 86_400;
    let kept = refresh(&server, &idle, T0 + ninety_days - 60)["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(refresh(&server, &kept, T0 + 2 * ninety_days)["error"], "invalid_grant", "unused for 90 days it ran out");
}

#[test]
fn waiting_logins_and_wrong_keys_on_the_panel_are_braked() {
    let server = member_server();
    for _ in 0..versekit_orgserver::PENDING_MAX {
        assert_eq!(post(&server, "/versekit/v1/device/code", "scope=stock%3Aread", T0).status, 200);
    }
    let over = post(&server, "/versekit/v1/device/code", "scope=stock%3Aread", T0);
    assert_eq!((over.status, header(&over, "Retry-After")), (429, Some("60")));
    assert_eq!(post(&server, "/versekit/v1/device/code", "scope=stock%3Aread", T0 + 601).status, 200, "the old codes ran out");

    let server = member_server();
    let codes: Vec<String> = (0..8)
        .map(|_| post(&server, "/versekit/v1/device/code", "scope=stock%3Aread", T0).json_body()["user_code"].as_str().unwrap().to_string())
        .collect();
    let mut wrong = 0;
    'codes: for code in &codes {
        for _ in 0..4 {
            post(&server, "/device", &format!("user_code={code}&key=falsch-falsch-falsch&action=show"), T0);
            wrong += 1;
            if wrong == versekit_orgserver::PANEL_WRONG_PER_MINUTE {
                break 'codes;
            }
        }
    }
    let last = codes.last().unwrap();
    let held = String::from_utf8_lossy(&post(&server, "/device", &format!("user_code={last}&key={KEY}&action=show"), T0 + 1).body).into_owned();
    assert!(held.contains("zu viele falsche Schlüssel"), "even the right key waits for the rest of the minute: {held}");
    let later = String::from_utf8_lossy(&post(&server, "/device", &format!("user_code={last}&key={KEY}&action=show"), T0 + 60).body).into_owned();
    assert!(later.contains("Hallo Testpilot"), "the next minute the right key works: {later}");
}

#[test]
fn the_page_of_the_rights_carries_a_ticket_and_never_the_key() {
    let server = member_server();
    let code = post(&server, "/versekit/v1/device/code", "scope=stock%3Aread", T0).json_body()["user_code"].as_str().unwrap().to_string();
    let rights = String::from_utf8_lossy(&post(&server, "/device", &format!("user_code={code}&key={KEY}&action=show"), T0).body).into_owned();
    assert!(!rights.contains(KEY), "the key is not in the page");
    let ticket = rights.split("name=\"ticket\" value=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
    let forged = post(&server, "/device", &format!("user_code={code}&ticket=0000&action=approve&scope=stock%3Aread"), T0);
    assert!(String::from_utf8_lossy(&forged.body).contains("Der Schlüssel stimmt nicht."), "a made-up ticket is no key");
    let done = post(&server, "/device", &format!("user_code={code}&ticket={ticket}&action=approve&scope=stock%3Aread"), T0);
    assert!(String::from_utf8_lossy(&done.body).contains("Freigegeben"), "the ticket confirms");
}

#[test]
fn a_delivery_beyond_reason_is_refused() {
    let server = Server::new(
        Config::new("https://orga.example.org", "Orga", "ORG").with_member("tester", "Testpilot", KEY).with_order(
            serde_json::json!({"id": "a1", "title": "Erz", "items": [{"item": "Quantanium", "quantity": 4}]}),
        ),
    );
    let code = post(&server, "/versekit/v1/device/code", "scope=orders%3Awrite", T0).json_body();
    assert!(server.approve(code["user_code"].as_str().unwrap(), "tester", None));
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&device_code={}",
        code["device_code"].as_str().unwrap()
    );
    let access = post(&server, "/versekit/v1/token", &form, T0).json_body()["access_token"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {access}");
    let deliver = |quantity: &str| {
        let body = format!(r#"{{"items":[{{"item":"Quantanium","quantity":{quantity}}}]}}"#);
        server.handle_at(
            &Request::new("POST", "/versekit/v1/orders/a1/deliveries", &[("Authorization", bearer.as_str()), ("Idempotency-Key", quantity)], body.as_bytes()),
            T0,
        )
    };
    assert_eq!(deliver("1e300").status, 400, "a quantity beyond a billion");
    assert_eq!(deliver("2").status, 201);
}

/// The program, started in its own folder in the test mode on a free port of the own computer.
struct Program {
    child: Child,
    dir: PathBuf,
    port: u16,
    log: Vec<String>,
}

impl Program {
    fn start(dir: &PathBuf) -> Program {
        std::fs::write(
            dir.join("config.json"),
            r#"{"public_url": "http://127.0.0.1:1", "name": "Testorga", "tag": "TST",
                "members": [{"id": "tester", "name": "Testpilot", "key": "schluessel-des-testpiloten"}]}"#,
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_versekit-orgserver"))
            .args(["--config", "config.json", "--state", "state.json", "--listen", "127.0.0.1:0"])
            .current_dir(dir)
            .env("VERSEKIT_ORG_TESTBETRIEB", "1")
            .env("VERSEKIT_ORG_UPDATE_CHECK", "0")
            .env_remove("VERSEKIT_ORG_ADMIN_PASSWORD")
            .env_remove("VERSEKIT_ORG_ADMIN_PASSWORD_FILE")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut log = Vec::new();
        let port = loop {
            let line = lines.next().expect("the program ends before it listens").unwrap();
            log.push(line.clone());
            if let Some(address) = line.split("listening on ").nth(1) {
                break address.rsplit(':').next().unwrap().trim().parse().unwrap();
            }
        };
        Program { child, dir: dir.clone(), port, log }
    }

    fn get(&self, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let mut rest = String::new();
        if let Some(mut err) = self.child.stderr.take() {
            let _ = err.read_to_string(&mut rest);
        }
        let _ = self.child.wait();
        format!("{}\n{rest}", self.log.join("\n"))
    }
}

#[test]
fn the_program_makes_its_admin_password_and_names_only_where_it_lies() {
    let dir = std::env::temp_dir().join(format!("versekit-orgserver-program-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let program = Program::start(&dir);
    assert!(program.get("/healthz").starts_with("HTTP/1.1 200"));
    assert!(program.get("/admin/login").starts_with("HTTP/1.1 200"), "the administration is on without any setting");
    let file = program.dir.join("admin-password.txt");
    let log = program.stop();
    let password = std::fs::read_to_string(&file).expect("the password lies in the data folder").trim().to_string();
    assert!(password.len() >= 24);
    assert!(log.contains("admin-password.txt"), "the log names where it lies: {log}");
    assert!(!log.contains(&password), "the log never holds the password");

    let again = Program::start(&dir);
    let log = again.stop();
    assert!(!log.contains(&password) && !log.contains("created"), "the second start takes the same: {log}");
    assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), password);

    let shown = Command::new(env!("CARGO_BIN_EXE_versekit-orgserver"))
        .args(["--admin-password", "--state", "state.json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(shown.status.success());
    assert_eq!(String::from_utf8_lossy(&shown.stdout).trim(), password, "shown only when asked for in the terminal");
    let _ = std::fs::remove_dir_all(&dir);
}
