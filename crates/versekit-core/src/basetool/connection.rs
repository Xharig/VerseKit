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

//! One connection to the Basetool for the whole program (`basetool.Connection`).
//!
//! **Login:** device login (RFC 8628). Verse-Kit fetches a code, the player
//! types it into the browser and agrees. ⚠⚠ Only the code and the plain
//! address are shown, never `verification_uri_complete`: the link with the
//! code built in skips the page on which the Basetool warns about code
//! fraud, and gets people used to clicking code links.
//!
//! **Tokens:** the access token lives 300 s and is renewed 30 s before; the
//! refresh token lies in the secret store. Both are bound to the key of this
//! installation (DPoP); a token without that binding is refused.

use std::sync::{Arc, Mutex};

use crate::basetool::config::{self, Config};
use crate::basetool::error::{Action, ApiError, parse_retry_after};
use crate::basetool::guard::{self, Entry, Journal, WriteBudget};
use crate::basetool::http::{Request, Response, Transport};
use crate::basetool::key::{KeyFailure, KeyVault};
use crate::basetool::proof::{DpopKey, NonceCache, ProofInput, ServerClock, asks_for_nonce, build_proof, header};
use crate::basetool::scopes::{SCOPE_CONNECT, SCOPE_OFFLINE};
use crate::basetool::secrets::{REFRESH_SECRET, SecretError, SecretStore};
use crate::jsonio::{self, Map, Value};

/// Renew this many seconds before the access token runs out.
pub const REFRESH_MARGIN: f64 = 30.0;

/// The time in seconds since 1970.
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// The system clock.
pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    })
}

/// A JSON answer as Python reads it: an object stays one, any other value
/// becomes `{"value": …}`, something unreadable `{}`.
pub fn parse_answer(raw: &[u8]) -> Map {
    if raw.is_empty() {
        return Map::new();
    }
    let Ok(text) = std::str::from_utf8(raw) else { return Map::new() };
    match jsonio::loads(text) {
        Ok(Value::Object(map)) => map,
        Ok(other) => vec![("value".into(), other)],
        Err(_) => Map::new(),
    }
}

/// A value of an answer.
pub fn get<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A text value of an answer; other kinds count as missing.
pub fn get_str<'a>(map: &'a Map, key: &str) -> Option<&'a str> {
    get(map, key).and_then(Value::as_str)
}

/// `int(value or default)` for counts and seconds from the server.
pub fn py_int(value: Option<&Value>, default: i64) -> i64 {
    match value {
        Some(Value::Int(n)) if n != "0" => n.parse().unwrap_or(if n.starts_with('-') { i64::MIN } else { i64::MAX }),
        Some(Value::Float(x)) if *x != 0.0 && x.is_finite() => x.trunc() as i64,
        Some(Value::Bool(true)) => 1,
        Some(Value::Str(s)) if !s.is_empty() => crate::text::py_strip(s).parse().unwrap_or(default),
        _ => default,
    }
}

/// `urllib.parse.urlencode`: space as `+`, only letters, digits and `_.-~`
/// stay as they are.
pub fn urlencode(pairs: &[(&str, &str)]) -> String {
    let quote = |text: &str| {
        let mut out = String::new();
        for byte in text.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => out.push(byte as char),
                b' ' => out.push('+'),
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    };
    pairs.iter().map(|(k, v)| format!("{}={}", quote(k), quote(v))).collect::<Vec<_>>().join("&")
}

/// A fresh `Idempotency-Key` (UUID version 4).
pub fn new_idempotency_key() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // Without the system's random numbers the time keeps keys apart.
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        b = nanos.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

fn secret_error(e: SecretError) -> ApiError {
    ApiError::internal(e.code, &e.message)
}

fn key_error(e: KeyFailure) -> ApiError {
    match e {
        KeyFailure::Secret(e) => secret_error(e),
        KeyFailure::Key(e) => ApiError::internal("INTERNAL", &e.0),
    }
}

/// A running device login. Code and address are all the player sees.
#[derive(Clone)]
pub struct DeviceLogin {
    device_code: String,
    pub user_code: String,
    /// The login page; an address outside the Basetool becomes the
    /// issuer's own page (`<issuer>/device`).
    pub verification_uri: String,
    /// Seconds between two questions; grows by 5 on `slow_down`.
    pub interval: u64,
    pub expires_at: f64,
    /// The rights asked for.
    pub scopes: Vec<String>,
}

impl std::fmt::Debug for DeviceLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceLogin")
            .field("verification_uri", &self.verification_uri)
            .field("interval", &self.interval)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

/// The answer to one question during the login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Ok,
}

impl Poll {
    pub fn name(self) -> &'static str {
        match self {
            Poll::Pending => "pending",
            Poll::SlowDown => "slow_down",
            Poll::Denied => "denied",
            Poll::Expired => "expired",
            Poll::Ok => "ok",
        }
    }
}

/// Addresses from the discovery document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Endpoints {
    pub device: String,
    pub token: String,
    pub revoke: Option<String>,
}

#[derive(Default)]
struct Inner {
    key: Option<Arc<dyn DpopKey>>,
    endpoints: Option<Endpoints>,
    /// The rights the issuer offers (`scopes_supported`); `None` while unknown.
    offered: Option<Vec<String>>,
    granted: Vec<String>,
}

#[derive(Default)]
struct Token {
    access: Option<String>,
    until: f64,
}

/// Records of what happened, for the start trace (rights granted).
pub type Trail = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Connection {
    config: Config,
    transport: Arc<dyn Transport>,
    secrets: Arc<dyn SecretStore>,
    vault: Arc<dyn KeyVault>,
    user_agent: String,
    clock: Clock,
    trail: Option<Trail>,
    nonces: NonceCache,
    server_clock: ServerClock,
    inner: Mutex<Inner>,
    token: Mutex<Token>,
    renewal: Mutex<()>,
    opening: Mutex<()>,
    /// The test mode (`testmode::basetool_locked`): no request leaves, the key is neither opened nor
    /// deleted.
    test_lock: bool,
    /// The key of the player's real installation on a copy of the data (`testmode::basetool_keeps_key`):
    /// never deleted, not by "Disconnect" and not by `INSTALLATION_REVOKED`.
    keep_key: bool,
    /// The write guard (`guard::WriteBudget`): the changes left for this run; `None` is no guard.
    write_budget: Option<Arc<WriteBudget>>,
    /// The request journal (`guard::Journal`); `None` writes nothing.
    journal: Option<Arc<Journal>>,
}

impl Connection {
    pub fn new(
        config: Config,
        transport: Arc<dyn Transport>,
        secrets: Arc<dyn SecretStore>,
        vault: Arc<dyn KeyVault>,
        version: &str,
        clock: Clock,
    ) -> Connection {
        Connection {
            config,
            transport,
            secrets,
            vault,
            user_agent: config::user_agent(version),
            clock,
            trail: None,
            nonces: NonceCache::default(),
            server_clock: ServerClock::default(),
            inner: Mutex::new(Inner::default()),
            token: Mutex::new(Token::default()),
            renewal: Mutex::new(()),
            opening: Mutex::new(()),
            test_lock: false,
            keep_key: false,
            write_budget: None,
            journal: None,
        }
    }

    pub fn with_trail(mut self, trail: Trail) -> Connection {
        self.trail = Some(trail);
        self
    }

    /// Locks the connection for the test mode (`true`): every request fails with
    /// [`crate::testmode::BASETOOL_LOCKED`] before it reaches the transport, the key of the
    /// installation is neither opened nor deleted, and disconnecting forgets the token of this
    /// data only.
    pub fn with_test_lock(mut self, locked: bool) -> Connection {
        self.test_lock = locked;
        self
    }

    /// Whether the connection is locked for the test mode.
    pub fn test_locked(&self) -> bool {
        self.test_lock
    }

    /// Keeps the key of the installation (`true`): "Disconnect" and `INSTALLATION_REVOKED` forget the token
    /// but never delete the key, because it is the key of the player's real installation.
    pub fn with_kept_key(mut self, keep: bool) -> Connection {
        self.keep_key = keep;
        self
    }

    /// Whether the key of the installation is never deleted by this connection.
    pub fn key_kept(&self) -> bool {
        self.keep_key
    }

    /// Puts the write guard on with a budget of this connection alone (`Some(ceiling)`): once the sends have
    /// used up the ceiling, and for any writing request outside the closed list of harmless ones, the request
    /// fails with [`crate::testmode::WRITE_GUARD`] before it is built. `0` only fetches. `None` is no guard.
    pub fn with_write_guard(self, max: Option<u32>) -> Connection {
        self.with_write_budget(max.map(|ceiling| Arc::new(WriteBudget::new(ceiling))))
    }

    /// The write guard with a budget shared with other connections (`guard::run_budget`: one per run).
    pub fn with_write_budget(mut self, budget: Option<Arc<WriteBudget>>) -> Connection {
        self.write_budget = budget;
        self
    }

    /// The ceiling of the write guard; `None` while there is none.
    pub fn write_guard(&self) -> Option<u32> {
        self.write_budget.as_ref().map(|budget| budget.ceiling())
    }

    /// The changes the write guard still lets through; `None` while there is none.
    pub fn write_left(&self) -> Option<u32> {
        self.write_budget.as_ref().map(|budget| budget.left())
    }

    /// Notes every request in the request journal (without content).
    pub fn with_journal(mut self, journal: Option<Journal>) -> Connection {
        self.journal = journal.map(Arc::new);
        self
    }

    fn refuse_in_test(&self) -> Result<(), ApiError> {
        if self.test_lock { Err(ApiError::new(crate::testmode::BASETOOL_LOCKED, 0)) } else { Ok(()) }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.secrets
    }

    fn now(&self) -> f64 {
        (self.clock)()
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn token(&self) -> std::sync::MutexGuard<'_, Token> {
        self.token.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // ------------------------------------------------------------ State

    /// Whether a refresh token is stored.
    pub fn connected(&self) -> bool {
        matches!(self.secrets.load(REFRESH_SECRET), Ok(Some(t)) if !t.is_empty())
    }

    /// The rights the Basetool granted with the last token, sorted.
    pub fn granted(&self) -> Vec<String> {
        self.inner().granted.clone()
    }

    fn key_ready(&self) -> Result<Arc<dyn DpopKey>, ApiError> {
        self.refuse_in_test()?;
        if let Some(key) = &self.inner().key {
            return Ok(key.clone());
        }
        // Opening may wait for the keyring: under a lock of its own, so the
        // other state stays readable, and never two keys created at once.
        let _opening = self.opening.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(key) = &self.inner().key {
            return Ok(key.clone());
        }
        let key = self.vault.open(true).map_err(key_error)?.ok_or_else(|| ApiError::internal("INTERNAL", "no key"))?;
        self.inner().key = Some(key.clone());
        Ok(key)
    }

    fn proof(&self, method: &str, url: &str, access_token: Option<&str>) -> Result<String, ApiError> {
        let key = self.key_ready()?;
        let nonce = self.nonces.get(url);
        let input = ProofInput {
            access_token,
            nonce: nonce.as_deref(),
            issued_at: self.server_clock.now(url, self.now()),
            jti: None,
        };
        build_proof(key.as_ref(), method, url, &input).map_err(|e| ApiError::internal("INTERNAL", &e.0))
    }

    fn learn(&self, url: &str, headers: &[(String, String)]) {
        self.server_clock.learn(url, headers, self.now());
        self.nonces.update(url, headers);
    }

    fn send(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(&str, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, ApiError> {
        self.send_counting(method, url, headers, body, None)
    }

    /// `send`, with the number of changes the request carries for the journal.
    fn send_counting(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(&str, String)>,
        body: Option<Vec<u8>>,
        changes: Option<usize>,
    ) -> Result<Response, ApiError> {
        // The one way to the transport: in the test mode nothing passes, reading included (a read needs a
        // token, and renewing it changes the session at the Basetool).
        self.refuse_in_test()?;
        let request = Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: headers.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            body,
        };
        let started = std::time::Instant::now();
        let result = self.transport.send(&request);
        if let Some(journal) = &self.journal {
            let (status, received) = result.as_ref().map_or((0, 0), |reply| (reply.status, reply.body.len()));
            journal.note(&Entry {
                time: self.now(),
                method,
                url,
                status,
                millis: started.elapsed().as_millis(),
                sent: request.body.as_ref().map_or(0, Vec::len),
                received,
                refused: false,
                changes,
                left: changes.and(self.write_left()),
            });
        }
        result
    }

    /// The write guard before a request is built: `WRITE_GUARD` for what it refuses, noted in the journal.
    /// A send it lets through takes its changes from the budget here, once, however often it is repeated.
    fn guard_request(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(), ApiError> {
        let Some(budget) = &self.write_budget else { return Ok(()) };
        if !budget.take(method, path, body) {
            return Ok(());
        }
        if let Some(journal) = &self.journal {
            let changes = guard::change_count(method, path, body);
            journal.note(&Entry {
                time: self.now(),
                method,
                url: path,
                status: 0,
                millis: 0,
                sent: 0,
                received: 0,
                refused: true,
                changes,
                left: changes.map(|_| budget.left()),
            });
        }
        let mut error = ApiError::new(crate::testmode::WRITE_GUARD, 0);
        error.path = format!("{method} {path}");
        Err(error)
    }

    // -------------------------------------------------------- Endpoints

    /// From the discovery document, but only if it names the expected issuer.
    pub fn endpoints(&self) -> Result<Endpoints, ApiError> {
        if let Some(found) = &self.inner().endpoints {
            return Ok(found.clone());
        }
        let url = format!("{}/.well-known/openid-configuration", self.config.issuer);
        let answer = self.send(
            "GET",
            &url,
            vec![("User-Agent", self.user_agent.clone()), ("Accept", "application/json".into())],
            None,
        )?;
        let doc = parse_answer(&answer.body);
        if answer.status != 200 || get_str(&doc, "issuer") != Some(self.config.issuer.as_str()) {
            let code = if answer.status == 200 { "ISSUER_MISMATCH" } else { "NETWORK" };
            return Err(ApiError::new(code, answer.status));
        }
        let text = |key: &str| get_str(&doc, key).filter(|s| !s.is_empty()).map(str::to_string);
        let (Some(device), Some(token)) = (text("device_authorization_endpoint"), text("token_endpoint")) else {
            return Err(ApiError::new("ISSUER_MISMATCH", answer.status));
        };
        let found = Endpoints { device, token, revoke: text("revocation_endpoint") };
        let offered = match get(&doc, "scopes_supported") {
            Some(Value::Array(items)) if !items.is_empty() => {
                Some(items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            }
            _ => None,
        };
        let mut inner = self.inner();
        inner.endpoints = Some(found.clone());
        inner.offered = offered;
        Ok(found)
    }

    /// The rights the Basetool offers, from the discovery document already
    /// read; `None` while unknown.
    pub fn offered_scopes(&self) -> Option<Vec<String>> {
        self.inner().offered.clone()
    }

    /// The rights of `scopes` the Basetool does not offer (yet); unknown
    /// means none.
    pub fn not_offered(&self, scopes: &[String]) -> Vec<String> {
        match self.offered_scopes() {
            Some(offered) => scopes.iter().filter(|s| !offered.contains(s)).cloned().collect(),
            None => Vec::new(),
        }
    }

    // ------------------------------------------------------------ Login

    /// Key ready, device code fetched; no token yet.
    ///
    /// ⚠⚠ A right the issuer does not offer makes the whole login fail with
    /// `invalid_scope`, so only offered rights are asked for. The others
    /// come by themselves once it offers them.
    pub fn start_login(&self, scopes: &[String]) -> Result<DeviceLogin, ApiError> {
        self.key_ready()?;
        let url = self.endpoints()?.device;
        let missing = self.not_offered(scopes);
        let mut wanted = vec![SCOPE_CONNECT.to_string()];
        wanted.extend(scopes.iter().filter(|s| s.as_str() != SCOPE_CONNECT && !missing.contains(s)).cloned());
        wanted.push(SCOPE_OFFLINE.to_string());
        let body = urlencode(&[("client_id", &self.config.client_id), ("scope", &wanted.join(" "))]);
        let answer = self.send(
            "POST",
            &url,
            vec![
                ("User-Agent", self.user_agent.clone()),
                ("Content-Type", "application/x-www-form-urlencoded".into()),
                ("Accept", "application/json".into()),
            ],
            Some(body.into_bytes()),
        )?;
        self.server_clock.learn(&url, &answer.headers, self.now());
        let doc = parse_answer(&answer.body);
        let device_code = get_str(&doc, "device_code").unwrap_or_default();
        if answer.status != 200 || device_code.is_empty() {
            let code = get_str(&doc, "error").filter(|e| !e.is_empty()).unwrap_or("DEVICE_LOGIN_FAILED");
            return Err(ApiError::new(code, answer.status)
                .with_retry_after(parse_retry_after(header(&answer.headers, "Retry-After"))));
        }
        let trusted = self.config.trusted_address(get_str(&doc, "verification_uri").unwrap_or_default());
        // The code that pairs this computer with the account: private for the streamer mode.
        crate::streamer::learn(get_str(&doc, "user_code").unwrap_or_default());
        crate::streamer::learn(device_code);
        Ok(DeviceLogin {
            device_code: device_code.to_string(),
            user_code: get_str(&doc, "user_code").unwrap_or_default().to_string(),
            verification_uri: if trusted.is_empty() {
                format!("{}/device", self.config.issuer.trim_end_matches('/'))
            } else {
                trusted
            },
            interval: py_int(get(&doc, "interval"), 5).max(5) as u64,
            expires_at: self.now() + py_int(get(&doc, "expires_in"), 600) as f64,
            scopes: wanted,
        })
    }

    /// Asks once; whoever asks waits `login.interval` seconds before.
    pub fn poll_login(&self, login: &mut DeviceLogin) -> Result<Poll, ApiError> {
        if self.now() >= login.expires_at {
            return Ok(Poll::Expired);
        }
        let (answer, status) = self.token_request(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &login.device_code),
            ("client_id", &self.config.client_id),
        ])?;
        if status == 200 {
            self.take_tokens(&answer)?;
            return Ok(Poll::Ok);
        }
        match get_str(&answer, "error") {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => {
                // For good, as RFC 8628 wants it.
                login.interval += 5;
                Ok(Poll::SlowDown)
            }
            Some("access_denied") => Ok(Poll::Denied),
            Some("expired_token") => Ok(Poll::Expired),
            other => Err(ApiError::new(other.filter(|e| !e.is_empty()).unwrap_or("TOKEN_FAILED"), status)),
        }
    }

    /// POST to the token endpoint with a proof; a nonce challenge and a
    /// refused proof (usually the clock) are each repeated once.
    fn token_request(&self, form: &[(&str, &str)]) -> Result<(Map, u16), ApiError> {
        let url = self.endpoints()?.token;
        let body = urlencode(form);
        let refreshing = form.iter().any(|(k, _)| *k == "refresh_token");
        let (mut answer, mut status) = (Map::new(), 0);
        for attempt in 0..3 {
            let reply = self.send(
                "POST",
                &url,
                vec![
                    ("User-Agent", self.user_agent.clone()),
                    ("Accept", "application/json".into()),
                    ("Content-Type", "application/x-www-form-urlencoded".into()),
                    ("DPoP", self.proof("POST", &url, None)?),
                ],
                Some(body.clone().into_bytes()),
            )?;
            status = reply.status;
            answer = parse_answer(&reply.body);
            self.learn(&url, &reply.headers);
            if attempt < 2 && asks_for_nonce(status, &reply.headers, get_str(&answer, "error")) {
                continue;
            }
            // A refused proof says nothing about the token; mostly it is the
            // clock, which is now set after the answer's `Date`.
            if attempt == 0
                && status == 400
                && matches!(get_str(&answer, "error"), Some("invalid_dpop_proof" | "invalid_request"))
                && refreshing
            {
                continue;
            }
            break;
        }
        Ok((answer, status))
    }

    /// ⚠ A token not bound to the key is useless for the API: refused, not
    /// used quietly.
    fn take_tokens(&self, answer: &Map) -> Result<(), ApiError> {
        if !get_str(answer, "token_type").unwrap_or_default().eq_ignore_ascii_case("dpop") {
            return Err(ApiError::new("DPOP_REQUIRED", 200));
        }
        {
            let mut token = self.token();
            token.access = get_str(answer, "access_token").map(str::to_string);
            token.until = self.now() + py_int(get(answer, "expires_in"), 300) as f64;
        }
        let mut granted: Vec<String> =
            get_str(answer, "scope").unwrap_or_default().split_whitespace().map(str::to_string).collect();
        granted.sort();
        let changed = {
            let mut inner = self.inner();
            let changed = inner.granted != granted;
            inner.granted = granted.clone();
            changed
        };
        if changed && let Some(trail) = &self.trail {
            let list = if granted.is_empty() { "(keine)".to_string() } else { granted.join(" ") };
            trail(&format!("Basetool: Rechte erteilt: {list}"));
        }
        if let Some(refresh) = get_str(answer, "refresh_token").filter(|r| !r.is_empty()) {
            self.secrets.save(REFRESH_SECRET, refresh).map_err(secret_error)?;
        }
        Ok(())
    }

    fn refresh(&self) -> Result<(), ApiError> {
        let refresh = self.secrets.load(REFRESH_SECRET).map_err(secret_error)?.filter(|r| !r.is_empty());
        let Some(refresh) = refresh else { return Err(ApiError::new("NOT_CONNECTED", 0)) };
        let (answer, status) = self.token_request(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &self.config.client_id),
        ])?;
        if status == 200 {
            return self.take_tokens(&answer);
        }
        match get_str(&answer, "error") {
            Some("invalid_grant") => {
                // Over (disconnected, 30 days unused, 90 days old). Token
                // gone, key stays: it is still this installation.
                self.forget_tokens();
                Err(ApiError::new("invalid_grant", status))
            }
            other => Err(ApiError::new(other.filter(|e| !e.is_empty()).unwrap_or("TOKEN_FAILED"), status)),
        }
    }

    fn valid_access(&self) -> Option<String> {
        let token = self.token();
        token.access.clone().filter(|_| self.now() < token.until - REFRESH_MARGIN)
    }

    fn access_token(&self) -> Result<String, ApiError> {
        if let Some(access) = self.valid_access() {
            return Ok(access);
        }
        // Parallel requests wait for one renewal instead of each renewing.
        let _renewing = self.renewal.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(access) = self.valid_access() {
            return Ok(access);
        }
        self.refresh()?;
        self.token().access.clone().ok_or_else(|| ApiError::new("TOKEN_FAILED", 200))
    }

    fn forget_tokens(&self) {
        *self.token() = Token::default();
        let _ = self.secrets.delete(REFRESH_SECRET);
    }

    // --------------------------------------------------------- Requests

    /// One request to the API; the answer of a 2xx as an object.
    ///
    /// A nonce challenge is repeated once. `UNAUTHENTICATED` renews once and
    /// repeats, then not again: no loop. A repeated write keeps its
    /// `Idempotency-Key`.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        query: &[(&str, &str)],
        idempotency_key: Option<&str>,
    ) -> Result<Map, ApiError> {
        self.refuse_in_test()?;
        self.guard_request(method, path, body)?;
        let changes = guard::change_count(method, path, body);
        let mut url = format!("{}{path}", self.config.api);
        if !query.is_empty() {
            url = format!("{url}?{}", urlencode(query));
        }
        let raw_body = body.map(|b| jsonio::dumps(b, None, false).into_bytes());
        let (mut nonce_retry, mut refreshed) = (false, false);
        loop {
            let token = self.access_token()?;
            let mut headers = vec![
                ("User-Agent", self.user_agent.clone()),
                ("Accept", "application/json, application/problem+json".into()),
                ("Authorization", format!("DPoP {token}")),
                ("DPoP", self.proof(method, &url, Some(&token))?),
            ];
            if raw_body.is_some() {
                headers.push(("Content-Type", "application/json".into()));
            }
            if let Some(key) = idempotency_key {
                headers.push(("Idempotency-Key", key.to_string()));
            }
            let reply = self.send_counting(method, &url, headers, raw_body.clone(), changes)?;
            self.learn(&url, &reply.headers);
            let answer = parse_answer(&reply.body);
            if (200..300).contains(&reply.status) {
                return Ok(answer);
            }
            let code = get_str(&answer, "code")
                .filter(|c| !c.is_empty())
                .map_or_else(|| format!("HTTP_{}", reply.status), str::to_string);
            if !nonce_retry && asks_for_nonce(reply.status, &reply.headers, None) {
                nonce_retry = true;
                continue;
            }
            if code == "UNAUTHENTICATED" && !refreshed {
                refreshed = true;
                self.token().access = None;
                continue;
            }
            let mut error = ApiError::new(&code, reply.status)
                .with_retry_after(parse_retry_after(header(&reply.headers, "Retry-After")))
                .with_problem(answer);
            error.path = format!("{method} {path}");
            if error.action() == Action::ForgetKey {
                self.forget_tokens();
                self.drop_key();
            } else if code == "CLIENT_REVOKED" {
                self.forget_tokens();
            }
            return Err(error);
        }
    }

    // ------------------------------------------------------- Disconnect

    /// ⚠ In the test mode, and on a copy that uses the key of the real installation, the key stays.
    fn drop_key(&self) {
        let _opening = self.opening.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.inner().key = None;
        if !self.test_lock && !self.keep_key {
            let _ = self.vault.delete();
        }
    }

    /// "Disconnect": revoke the token (as far as it works), delete the
    /// token, delete the key; in that order. In the test mode only the token
    /// of this data goes: nothing is revoked, the key stays. With the key
    /// kept (`with_kept_key`) the token is revoked and deleted, the key stays.
    pub fn disconnect(&self) {
        if !self.test_lock
            && let Ok(Some(refresh)) = self.secrets.load(REFRESH_SECRET)
            && !refresh.is_empty()
            && let Ok(endpoints) = self.endpoints()
            && let Some(url) = endpoints.revoke
            && let Ok(proof) = self.proof("POST", &url, None)
        {
            let body = urlencode(&[
                ("token", &refresh),
                ("token_type_hint", "refresh_token"),
                ("client_id", &self.config.client_id),
            ]);
            // Revoking is only an attempt.
            let _ = self.send(
                "POST",
                &url,
                vec![
                    ("User-Agent", self.user_agent.clone()),
                    ("Content-Type", "application/x-www-form-urlencoded".into()),
                    ("DPoP", proof),
                ],
                Some(body.into_bytes()),
            );
        }
        self.forget_tokens();
        self.drop_key();
        self.inner().granted.clear();
    }

    // ------------------------------------------------------- Shortcuts

    pub fn service_document(&self) -> Result<Map, ApiError> {
        self.request("GET", "", None, &[], None)
    }

    pub fn label_installation(&self, label: &str) -> Result<Map, ApiError> {
        let body = Value::Object(vec![("label".into(), Value::str(label))]);
        self.request("POST", "/me/installation", Some(&body), &[], None)
    }

    /// `match`, `mismatch`, `unknown` …; `unknown` without an answer.
    pub fn account_check(&self, handle: &str) -> Result<String, ApiError> {
        let body = Value::Object(vec![("handle".into(), Value::str(handle))]);
        let answer = self.request("POST", "/me/account-check", Some(&body), &[], None)?;
        Ok(get_str(&answer, "result").filter(|r| !r.is_empty()).unwrap_or("unknown").to_string())
    }
}
