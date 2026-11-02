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

//! The Basetool connection against a made-up Basetool on 127.0.0.1: login,
//! nonce, renewal, refusals, revocation, the sync pass. Every request's
//! DPoP proof is checked here (signature, `htm`, `htu`, `ath`, nonce).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use versekit_core::basetool::config::Config;
use versekit_core::basetool::connection::{Connection, Poll, get_str, system_clock};
use versekit_core::basetool::error::Action;
use versekit_core::basetool::feed::{self, Sent};
use versekit_core::basetool::http::HttpTransport;
use versekit_core::basetool::jose::{self, Jwk};
use versekit_core::basetool::key::{KeyVault, StoredKey, verify};
use versekit_core::basetool::orgunits::OrgUnitsArea;
use versekit_core::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::jsonio::{self, Value};
use versekit_core::legacy;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

/// One request as the made-up Basetool saw it.
#[derive(Clone, Debug, Default)]
struct Seen {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
    body: String,
    /// The proof's claims, or `None` without one.
    claims: Option<Value>,
    proof_ok: bool,
}

impl Seen {
    fn form(&self) -> HashMap<String, String> {
        self.body
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (k.to_string(), v.replace('+', " ").replace("%3A", ":")))
            .collect()
    }

    fn claim(&self, name: &str) -> Option<String> {
        self.claims.as_ref()?.get(name).map(|v| match v {
            Value::Str(s) => s.clone(),
            other => jsonio::dumps(other, None, false),
        })
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn reply(status: u16, body: &str) -> Reply {
    Reply { status, headers: Vec::new(), body: body.to_string() }
}

impl Reply {
    fn header(mut self, name: &str, value: &str) -> Reply {
        self.headers.push((name.into(), value.into()));
        self
    }
}

type Handler = Box<dyn FnMut(&Seen) -> Reply + Send>;

/// A made-up Basetool; `base` is `http://127.0.0.1:<port>`.
struct Fake {
    base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn check_proof(seen: &mut Seen, base: &str) {
    let Some(proof) = seen.headers.get("dpop").cloned() else { return };
    let parts: Vec<&str> = proof.split('.').collect();
    if parts.len() != 3 {
        return;
    }
    let decode = |p: &str| jose::b64url_decode(p).ok().and_then(|b| String::from_utf8(b).ok());
    let (Some(header), Some(claims)) = (decode(parts[0]), decode(parts[1])) else { return };
    let (Ok(header), Ok(claims)) = (jsonio::loads(&header), jsonio::loads(&claims)) else { return };
    let jwk = header.get("jwk");
    let text =
        |v: Option<&Value>, k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_str).unwrap_or_default().to_string();
    let key = Jwk { x: text(jwk, "x"), y: text(jwk, "y") };
    let signature = jose::b64url_decode(parts[2]).unwrap_or_default();
    let signed = verify(&key, format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature);
    let htu_ok = text(Some(&claims), "htu") == format!("{base}{}", seen.path);
    let htm_ok = text(Some(&claims), "htm") == seen.method;
    let ath_ok = match seen.headers.get("authorization").and_then(|a| a.strip_prefix("DPoP ")) {
        Some(token) => text(Some(&claims), "ath") == jose::access_token_hash(token),
        None => claims.get("ath").is_none(),
    };
    seen.proof_ok = signed && htu_ok && htm_ok && ath_ok && text(Some(&header), "typ") == "dpop+jwt";
    seen.claims = Some(claims);
}

fn serve(handler: Handler) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().expect("address").port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (log, root) = (seen.clone(), base.clone());
    let handler = Arc::new(Mutex::new(handler));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let method = words.next().unwrap_or_default().to_string();
            let target = words.next().unwrap_or_default().to_string();
            let (path, query) =
                target.split_once('?').map_or((target.clone(), String::new()), |(p, q)| (p.into(), q.into()));
            let mut headers = HashMap::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                }
            }
            let length: usize = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let mut seen =
                Seen { method, path, query, headers, body: String::from_utf8_lossy(&body).into(), ..Seen::default() };
            check_proof(&mut seen, &root);
            let answer = (handler.lock().expect("handler"))(&seen);
            log.lock().expect("log").push(seen);
            let mut text = format!(
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                answer.status,
                answer.body.len()
            );
            for (k, v) in &answer.headers {
                text.push_str(&format!("{k}: {v}\r\n"));
            }
            text.push_str("\r\n");
            text.push_str(&answer.body);
            let _ = stream.write_all(text.as_bytes());
        }
    });
    Fake { base, seen }
}

impl Fake {
    fn issuer(&self) -> String {
        format!("{}/realms/iri", self.base)
    }

    fn api(&self) -> String {
        format!("{}/exchange/v1", self.base)
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().expect("log").clone()
    }

    fn host(&self) -> Host {
        Host::new(
            Flavor::native(),
            &[
                (legacy::ENV_BASETOOL_ISSUER, &self.issuer()),
                (legacy::ENV_BASETOOL_API, &self.api()),
                (legacy::ENV_BASETOOL_CLIENT, "sandbox-client"),
            ],
            "/",
        )
    }

    fn connection(&self, secrets: Arc<MemorySecrets>) -> Connection {
        let host = self.host();
        let vault: Arc<dyn KeyVault> = Arc::new(StoredKey { secrets: secrets.clone() });
        Connection::new(
            Config::from_host(&host),
            Arc::new(HttpTransport::new("", &host).expect("transport")),
            secrets,
            vault,
            "5.0.0",
            system_clock(),
        )
    }
}

fn discovery(issuer: &str) -> String {
    format!(
        r#"{{"issuer": "{issuer}", "device_authorization_endpoint": "{issuer}/device/auth",
            "token_endpoint": "{issuer}/token", "revocation_endpoint": "{issuer}/revoke",
            "scopes_supported": ["exchange.connect", "offline_access", "exchange.orgstock.read",
                                 "exchange.orgstock.details", "exchange.orgunits.read",
                                 "exchange.stock.read", "exchange.stock.write"]}}"#
    )
}

fn token(access: &str, refresh: &str, expires: u32) -> String {
    format!(
        r#"{{"token_type": "DPoP", "access_token": "{access}", "refresh_token": "{refresh}", "expires_in": {expires},
            "scope": "offline_access exchange.connect exchange.orgstock.read"}}"#
    )
}

#[test]
fn login_requests_renewal_and_disconnect() {
    let issuer_holder = Arc::new(Mutex::new(String::new()));
    let issuer_for = issuer_holder.clone();
    let mut polls = 0;
    let mut api_nonce_given = false;
    let mut unauthenticated_once = false;
    let fake = serve(Box::new(move |seen| {
        let issuer = issuer_for.lock().expect("issuer").clone();
        match (seen.method.as_str(), seen.path.as_str()) {
            ("GET", p) if p.ends_with("/.well-known/openid-configuration") => reply(200, &discovery(&issuer)),
            ("POST", p) if p.ends_with("/device/auth") => reply(
                200,
                r#"{"device_code": "dev-1", "user_code": "WDJB-MJHT", "verification_uri": "https://evil.example/device",
                    "verification_uri_complete": "https://evil.example/device?code=WDJB-MJHT", "interval": 1, "expires_in": 600}"#,
            )
            .header("Date", "Thu, 01 Jan 2099 00:00:00 GMT"),
            ("POST", p) if p.ends_with("/token") => {
                let form = seen.form();
                if seen.claim("nonce").is_none() {
                    return reply(400, r#"{"error": "use_dpop_nonce"}"#).header("DPoP-Nonce", "auth-n1");
                }
                match form.get("grant_type").map(String::as_str) {
                    Some("refresh_token") if form.get("refresh_token").map(String::as_str) == Some("R1") => {
                        reply(200, &token("A2", "R2", 300)).header("DPoP-Nonce", "auth-n2")
                    }
                    Some("refresh_token") => reply(400, r#"{"error": "invalid_grant"}"#),
                    _ => {
                        polls += 1;
                        match polls {
                            1 => reply(400, r#"{"error": "authorization_pending"}"#),
                            2 => reply(400, r#"{"error": "slow_down"}"#),
                            _ => reply(200, &token("A1", "R1", 300)),
                        }
                    }
                }
            }
            ("POST", p) if p.ends_with("/revoke") => reply(200, ""),
            (_, p) if p.starts_with("/exchange/v1") => {
                if !api_nonce_given {
                    api_nonce_given = true;
                    return reply(401, r#"{"code": "DPOP_INVALID"}"#)
                        .header("DPoP-Nonce", "api-n1")
                        .header("WWW-Authenticate", r#"DPoP error="use_dpop_nonce""#);
                }
                let route = &p["/exchange/v1".len()..];
                match route {
                    "" => reply(200, r#"{"installationId": "inst-1", "capabilities": ["exchange.orgstock.read"]}"#)
                        .header("DPoP-Nonce", "api-n2"),
                    "/org-stock" if !unauthenticated_once => {
                        unauthenticated_once = true;
                        reply(401, r#"{"code": "UNAUTHENTICATED"}"#)
                    }
                    "/org-stock" => reply(200, r#"{"items": [], "hasMore": false, "nextCursor": "c1"}"#),
                    "/org-stock/changes" => reply(
                        409,
                        r#"{"code": "MASS_CHANGE_CONFIRMATION_REQUIRED", "detail": "bitte bestätigen",
                            "confirmationUrl": "https://evil.example/confirm"}"#,
                    ),
                    _ => reply(429, r#"{"code": "RATE_LIMITED", "correlationId": "c-9"}"#).header("Retry-After", "30"),
                }
            }
            _ => reply(404, "{}"),
        }
    }));
    *issuer_holder.lock().expect("issuer") = fake.issuer();
    let secrets = Arc::new(MemorySecrets::default());
    let conn = fake.connection(secrets.clone());

    // Only offered rights are asked for; the untrusted page is replaced.
    let wanted = vec!["exchange.orgstock.read".to_string(), "exchange.unbekannt".to_string()];
    let mut login = conn.start_login(&wanted).expect("login");
    assert_eq!(login.user_code, "WDJB-MJHT");
    assert_eq!(login.verification_uri, format!("{}/device", fake.issuer()));
    assert_eq!(login.interval, 5);
    assert!(!format!("{login:?}").contains("dev-1"));
    let device_request = fake.seen().into_iter().find(|s| s.path.ends_with("/device/auth")).expect("device");
    assert_eq!(device_request.form()["scope"], "exchange.connect exchange.orgstock.read offline_access");
    assert!(!device_request.headers.contains_key("dpop"));

    assert_eq!(conn.poll_login(&mut login).expect("poll"), Poll::Pending);
    assert_eq!(conn.poll_login(&mut login).expect("poll"), Poll::SlowDown);
    assert_eq!(login.interval, 10);
    assert_eq!(conn.poll_login(&mut login).expect("poll"), Poll::Ok);
    assert!(conn.connected());
    assert_eq!(secrets.load(REFRESH_SECRET).expect("secret").as_deref(), Some("R1"));
    assert_eq!(conn.granted(), ["exchange.connect", "exchange.orgstock.read", "offline_access"]);

    // Every proof at the token endpoint carried the nonce after the first,
    // and its time follows the server clock (2099), not the own one.
    let token_requests: Vec<Seen> = fake.seen().into_iter().filter(|s| s.path.ends_with("/token")).collect();
    assert!(token_requests.iter().all(|s| s.proof_ok), "{token_requests:?}");
    assert_eq!(token_requests[1].claim("nonce").as_deref(), Some("auth-n1"));
    let iat: i64 = token_requests[0].claim("iat").and_then(|i| i.parse().ok()).expect("iat");
    assert!(iat >= 4_070_908_800, "iat {iat}");

    // API: nonce challenge once, then the answer.
    let doc = conn.service_document().expect("service document");
    assert_eq!(get_str(&doc, "installationId"), Some("inst-1"));
    // UNAUTHENTICATED: renewed once with R1, repeated with the new token.
    let fetched = feed::fetch_all(&conn, "/org-stock", None).expect("feed");
    assert_eq!(fetched.cursor.as_deref(), Some("c1"));
    assert_eq!(secrets.load(REFRESH_SECRET).expect("secret").as_deref(), Some("R2"));
    let last = fake.seen().last().cloned().expect("request");
    assert_eq!(last.headers["authorization"], "DPoP A2");
    assert_eq!(last.query, "limit=500");
    assert!(fake.seen().iter().filter(|s| s.path.starts_with("/exchange")).all(|s| s.proof_ok));

    // A mass change: held, and the confirmation page outside the Basetool
    // never reaches the browser.
    let body = Value::Object(vec![("ops".into(), Value::Array(vec![]))]);
    match feed::send_change_set(&conn, conn.config(), "/org-stock/changes", &body).expect("send") {
        Sent::Held { confirm_url, error } => {
            assert_eq!(confirm_url, None);
            assert_eq!(error.code, "MASS_CHANGE_CONFIRMATION_REQUIRED");
        }
        other => panic!("not held: {other:?}"),
    }
    let posted = fake.seen().last().cloned().expect("request");
    assert_eq!(posted.headers.get("idempotency-key").map(String::len), Some(36));
    assert_eq!(posted.headers["content-type"], "application/json");

    let limited = conn.request("GET", "/irgendwo", None, &[("cursor", "geheim")], None).expect_err("refused");
    assert_eq!((limited.code.as_str(), limited.status, limited.retry_after), ("RATE_LIMITED", 429, Some(30)));
    assert_eq!(limited.action(), Action::Retry);
    assert_eq!(limited.describe(), "RATE_LIMITED · HTTP 429 · GET /irgendwo · Retry-After 30s · correlationId=c-9");

    // Disconnect: revoked with the refresh token, then token and key gone.
    conn.disconnect();
    let revoke = fake.seen().into_iter().find(|s| s.path.ends_with("/revoke")).expect("revoke");
    assert_eq!(revoke.form()["token"], "R2");
    assert!(revoke.proof_ok);
    assert!(!conn.connected());
    assert_eq!(secrets.load(versekit_core::basetool::key::KEY_SECRET).expect("secret"), None);
}

#[test]
fn a_dead_refresh_token_and_a_revoked_installation() {
    let issuer_holder = Arc::new(Mutex::new(String::new()));
    let issuer_for = issuer_holder.clone();
    let fake = serve(Box::new(move |seen| {
        let issuer = issuer_for.lock().expect("issuer").clone();
        match seen.path.as_str() {
            p if p.ends_with("/.well-known/openid-configuration") => reply(200, &discovery(&issuer)),
            p if p.ends_with("/token") => match seen.form().get("refresh_token").map(String::as_str) {
                Some("tot") => reply(400, r#"{"error": "invalid_grant"}"#),
                _ => reply(200, &token("A9", "R9", 300)),
            },
            _ => reply(401, r#"{"code": "INSTALLATION_REVOKED"}"#),
        }
    }));
    *issuer_holder.lock().expect("issuer") = fake.issuer();
    let secrets = Arc::new(MemorySecrets::default());
    let conn = fake.connection(secrets.clone());

    secrets.save(REFRESH_SECRET, "tot").expect("save");
    let dead = conn.service_document().expect_err("dead token");
    assert_eq!((dead.code.as_str(), dead.action()), ("invalid_grant", Action::Relogin));
    assert!(!conn.connected());
    // The key stays: it is still this installation.
    assert!(secrets.load(versekit_core::basetool::key::KEY_SECRET).expect("secret").is_some());

    secrets.save(REFRESH_SECRET, "lebt").expect("save");
    let revoked = conn.service_document().expect_err("revoked");
    assert_eq!(revoked.action(), Action::ForgetKey);
    assert!(!conn.connected());
    assert_eq!(secrets.load(versekit_core::basetool::key::KEY_SECRET).expect("secret"), None);
    let no_token = conn.service_document().expect_err("not connected");
    assert_eq!(no_token.code, "NOT_CONNECTED");
}

#[test]
fn feed_pages_expired_cursor_and_a_stuck_feed() {
    let issuer_holder = Arc::new(Mutex::new(String::new()));
    let issuer_for = issuer_holder.clone();
    let fake = serve(Box::new(move |seen| {
        let issuer = issuer_for.lock().expect("issuer").clone();
        match seen.path.as_str() {
            p if p.ends_with("/.well-known/openid-configuration") => reply(200, &discovery(&issuer)),
            p if p.ends_with("/token") => reply(200, &token("A1", "R1", 300)),
            "/exchange/v1/feed" => match seen.query.as_str() {
                "limit=500&cursor=alt" => reply(410, r#"{"code": "CURSOR_EXPIRED"}"#),
                "limit=500" => reply(200, r#"{"items": [{"key": "a"}], "hasMore": true, "nextCursor": "p2"}"#),
                "limit=500&cursor=p2" => reply(
                    200,
                    r#"{"items": [{"key": "b"}], "removed": [{"key": "x"}], "hasMore": false, "nextCursor": "end"}"#,
                ),
                _ => reply(500, "{}"),
            },
            "/exchange/v1/stuck" => reply(200, r#"{"items": [], "hasMore": true}"#),
            _ => reply(404, "{}"),
        }
    }));
    *issuer_holder.lock().expect("issuer") = fake.issuer();
    let secrets = Arc::new(MemorySecrets::default());
    secrets.save(REFRESH_SECRET, "R0").expect("save");
    let conn = fake.connection(secrets);
    let (fetched, expired) = feed::fetch_resuming(&conn, "/feed", Some("alt")).expect("feed");
    assert!(expired);
    assert_eq!(fetched.items().len(), 2);
    assert_eq!(fetched.removed_keys(), ["x"]);
    assert_eq!(fetched.cursor.as_deref(), Some("end"));
    let stuck = feed::fetch_all(&conn, "/stuck", None).expect_err("stuck");
    assert_eq!((stuck.code.as_str(), stuck.action()), ("FEED_STUCK", Action::Retry));
}

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-basetool-{name}-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.to_string_lossy().into_owned()
}

#[test]
fn a_sync_pass_with_the_unit_list() {
    let issuer_holder = Arc::new(Mutex::new(String::new()));
    let issuer_for = issuer_holder.clone();
    let fake = serve(Box::new(move |seen| {
        let issuer = issuer_for.lock().expect("issuer").clone();
        match seen.path.as_str() {
            p if p.ends_with("/.well-known/openid-configuration") => reply(200, &discovery(&issuer)),
            p if p.ends_with("/token") => reply(200, &token("A1", "R1", 300)),
            "/exchange/v1" => reply(
                200,
                r#"{"installationId": "inst/7", "minClientVersion": "4.0.0",
                    "capabilities": ["exchange.stock.read", "exchange.stock.write", "exchange.orgunits.read"]}"#,
            ),
            "/exchange/v1/me/installation" => reply(200, "{}"),
            "/exchange/v1/me/account-check" => reply(200, r#"{"result": "match"}"#),
            "/exchange/v1/me/org-units" => {
                reply(200, r#"{"items": [{"id": "u1", "name": "Mamba", "shorthand": "MAM"}]}"#)
            }
            _ => reply(404, "{}"),
        }
    }));
    *issuer_holder.lock().expect("issuer") = fake.issuer();
    let dir = scratch("pass");
    // A made-up computer: nothing of the shell the check runs in counts.
    let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &dir)], &dir);
    let paths = Arc::new(Paths::new(host));
    assert!(paths.set_setting("basetool_lager", Value::Bool(true)));
    assert!(paths.set_setting("basetool_name", Value::str("Test-PC")));
    let secrets = Arc::new(MemorySecrets::default());
    secrets.save(REFRESH_SECRET, "R0").expect("save");
    let units = OrgUnitsArea::default();
    let wakes = Arc::new(Mutex::new(0u32));
    let counter = wakes.clone();
    let handle = SyncHandle::start(Setup {
        conn: Arc::new(fake.connection(secrets)),
        paths: paths.clone(),
        clock: system_clock(),
        version: "5.0.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(move || *counter.lock().expect("wakes") += 1),
        areas: vec![Box::new(units.clone())],
    });
    let status = handle.status();
    let mut waited = 0;
    while status.get().1.is_none_or(|s| s.phase != Phase::Ok) && waited < 100 {
        std::thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
    let done = status.get().1.expect("status");
    assert_eq!((done.phase, done.code.as_str()), (Phase::Ok, ""));
    assert_eq!(done.account.as_deref(), Some("match"));
    let (generation, list) = units.units.get();
    assert_eq!(generation, 1);
    assert_eq!(list.expect("units")[0].name, "Mamba");
    assert!(*wakes.lock().expect("wakes") > 0);

    let state = versekit_core::basetool::state::load_state(&paths, "inst/7");
    assert_eq!(get_str(&state, "label_sent"), Some("Test-PC"));
    assert!(std::path::Path::new(&paths.app_file("basetool-inst_7.json")).exists());
    let label = fake.seen().into_iter().find(|s| s.path.ends_with("/me/installation")).expect("label");
    assert_eq!(label.body, r#"{"label": "Test-PC"}"#);

    // A second pass on request: the account is known, no second check.
    let first = done.last_sync;
    handle.request_now();
    let mut waited = 0;
    while status.get().1.is_none_or(|s| s.phase != Phase::Ok || s.last_sync == first) && waited < 100 {
        std::thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
    assert_ne!(status.get().1.expect("status").last_sync, first, "no second pass");
    assert_eq!(fake.seen().iter().filter(|s| s.path == "/exchange/v1").count(), 2);
    let checks = fake.seen().iter().filter(|s| s.path.ends_with("/account-check")).count();
    assert_eq!(checks, 1);
    // The unit list did not change: the page sees no new generation.
    assert_eq!(units.units.generation(), 1);

    let job = handle.job(|conn| conn.account_check("Jemand").map_err(|e| e.code));
    assert_eq!(job.wait(Duration::from_secs(10)), Some(Ok("match".to_string())));
    assert_eq!(handle.login_state().get().1, None);

    handle.disconnect().wait(Duration::from_secs(10)).expect("disconnect");
    assert!(!std::path::Path::new(&paths.app_file("basetool-inst_7.json")).exists());
    assert!(units.units.get().1.is_none());
    handle.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

/// Counts how often the secret store is asked.
#[derive(Default)]
struct Counting {
    inner: MemorySecrets,
    loads: Mutex<u32>,
}

impl SecretStore for Counting {
    fn save(&self, name: &str, text: &str) -> Result<(), versekit_core::basetool::secrets::SecretError> {
        self.inner.save(name, text)
    }
    fn load(&self, name: &str) -> Result<Option<String>, versekit_core::basetool::secrets::SecretError> {
        *self.loads.lock().expect("loads") += 1;
        self.inner.load(name)
    }
    fn delete(&self, name: &str) -> Result<(), versekit_core::basetool::secrets::SecretError> {
        self.inner.delete(name)
    }
    fn uses_file(&self) -> bool {
        false
    }
}

#[test]
fn not_connected_the_worker_does_not_spin() {
    let dir = scratch("spin");
    let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &dir)], &dir);
    let paths = Arc::new(Paths::new(host));
    assert!(paths.set_setting("basetool_lager", Value::Bool(true)));
    let secrets = Arc::new(Counting::default());
    let net = Host::new(Flavor::native(), &[(legacy::ENV_NO_NET, "1")], "/");
    let conn = Connection::new(
        Config::from_host(&net),
        Arc::new(HttpTransport::new("", &net).expect("transport")),
        secrets.clone(),
        Arc::new(StoredKey { secrets: secrets.clone() }),
        "5.0.0",
        system_clock(),
    );
    // A clock that can jump ahead: the change must be older than 20 s.
    let ahead = Arc::new(Mutex::new(0.0f64));
    let real = system_clock();
    let shifted = ahead.clone();
    let handle = SyncHandle::start(Setup {
        conn: Arc::new(conn),
        paths,
        clock: Arc::new(move || real() + *shifted.lock().expect("clock")),
        version: "5.0.0".into(),
        own_account: Arc::new(|| None),
        waker: Arc::new(|| {}),
        areas: Vec::new(),
    });
    // An own change 30 s ago, then 1.5 s without a connection: a spinning
    // worker would ask the store thousands of times, the schedule once a
    // second.
    std::thread::sleep(Duration::from_millis(200));
    handle.local_changed();
    *ahead.lock().expect("clock") = 30.0;
    handle.request_now();
    *secrets.loads.lock().expect("loads") = 0;
    std::thread::sleep(Duration::from_millis(1500));
    let loads = *secrets.loads.lock().expect("loads");
    handle.shutdown();
    assert!((1..=4).contains(&loads), "{loads} questions to the store");
    let _ = std::fs::remove_dir_all(dir);
}

/// A find the overlay writes into the collection, in a process of its own, is an own change of the blueprints.
#[test]
fn a_collection_written_elsewhere_asks_for_a_pass_with_the_blueprints_switched_on() {
    let dir = scratch("bestand-fremd");
    let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &dir)], &dir);
    let paths = Arc::new(Paths::new(host));
    let secrets = Arc::new(MemorySecrets::default());
    let net = Host::new(Flavor::native(), &[(legacy::ENV_NO_NET, "1")], "/");
    let conn = Connection::new(
        Config::from_host(&net),
        Arc::new(HttpTransport::new("", &net).expect("transport")),
        secrets.clone(),
        Arc::new(StoredKey { secrets }),
        "5.0.0",
        system_clock(),
    );
    let handle = SyncHandle::without_worker(Setup {
        conn: Arc::new(conn),
        paths: Arc::clone(&paths),
        clock: system_clock(),
        version: "5.0.0".into(),
        own_account: Arc::new(|| None),
        waker: Arc::new(|| {}),
        areas: Vec::new(),
    });
    let file = paths.app_file(versekit_core::collection::FILE_NAME);
    let write = |text: &str| jsonio::write_text_atomic(&file, text, jsonio::Newline::Keep).expect("collection");
    // Switched off: the blueprints of the collection are not the sync's business.
    assert!(paths.set_setting("basetool_lager", Value::Bool(true)));
    handle.look_at_collection();
    write("{\"bauplaene\": {}}\n");
    handle.look_at_collection();
    assert!(!handle.change_waits(), "the blueprints are switched off");
    // Switched on: the first look only takes the state, a new write is a change.
    assert!(paths.set_setting("basetool_bauplaene", Value::Bool(true)));
    handle.look_at_collection();
    assert!(!handle.change_waits(), "nothing written since the last look");
    write("{\"bauplaene\": {\"arclight pistol\": {\"quelle\": \"log\"}}}\n");
    handle.look_at_collection();
    assert!(handle.change_waits(), "a find of the overlay goes out with the next pass");
    let _ = std::fs::remove_dir_all(dir);
}
