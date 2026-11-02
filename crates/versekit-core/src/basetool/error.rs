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

//! A refusal of the Basetool and what Verse-Kit does about it
//! (`basetool.ApiError`, `basetool.backoff`, `basetool_page.error_text`).
//!
//! ⚠ An error carries the server's code, never a header line or a token.

use std::fmt;

use crate::jsonio::{Map, Value};

/// What Verse-Kit does after an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Again later, with a growing pause and never before `Retry-After`.
    Retry,
    /// The connection is over; connecting again only on a click.
    Relogin,
    /// This installation was disconnected: throw the key away.
    ForgetKey,
    /// Stop and tell the player why.
    Stop,
    /// Verse-Kit is too old.
    Update,
}

impl Action {
    /// The name in the Python version.
    pub fn name(self) -> &'static str {
        match self {
            Action::Retry => "retry",
            Action::Relogin => "relogin",
            Action::ForgetKey => "forget_key",
            Action::Stop => "stop",
            Action::Update => "update",
        }
    }
}

/// Codes after which a later attempt can succeed.
pub const RETRY: [&str; 11] = [
    "RATE_LIMITED",
    "DPOP_PROOF_LIMIT",
    "QUOTA_EXCEEDED",
    "SERVICE_UNAVAILABLE",
    "EXCHANGE_DISABLED",
    "REGISTRY_UNAVAILABLE",
    "EXCHANGE_BUDGET_EXHAUSTED",
    "RELAY_BUSY",
    "BACKEND_RELAY_FAILED",
    "IDEMPOTENCY_IN_PROGRESS",
    "NETWORK",
];
/// Codes after which only a new login helps.
pub const RELOGIN: [&str; 3] = ["UNAUTHENTICATED", "CLIENT_REVOKED", "invalid_grant"];
/// Codes after which the key of this installation is worthless.
pub const FORGET_KEY: [&str; 1] = ["INSTALLATION_REVOKED"];

/// A refusal of the Basetool, or why no answer came.
#[derive(Clone, PartialEq)]
pub struct ApiError {
    pub code: String,
    /// HTTP status; 0 when no answer came.
    pub status: u16,
    /// Seconds from `Retry-After`, at least 1.
    pub retry_after: Option<u64>,
    /// The problem document of the answer (`detail`, `correlationId`,
    /// `confirmationUrl` …).
    pub problem: Map,
    /// Which request: method and path, without the query part (the cursor
    /// would stand there).
    pub path: String,
}

impl ApiError {
    /// An empty code becomes `UNKNOWN`.
    pub fn new(code: &str, status: u16) -> ApiError {
        let code = if code.is_empty() { "UNKNOWN" } else { code };
        ApiError { code: code.to_string(), status, retry_after: None, problem: Map::new(), path: String::new() }
    }

    pub fn with_retry_after(mut self, seconds: Option<u64>) -> ApiError {
        self.retry_after = seconds;
        self
    }

    pub fn with_problem(mut self, problem: Map) -> ApiError {
        self.problem = problem;
        self
    }

    /// No answer from the network; `detail` names the kind of failure.
    pub fn network(detail: &str) -> ApiError {
        ApiError::new("NETWORK", 0).with_problem(vec![("detail".into(), Value::str(detail))])
    }

    /// A failure inside Verse-Kit (key, store), not one of the Basetool.
    pub fn internal(code: &str, detail: &str) -> ApiError {
        ApiError::new(code, 0).with_problem(vec![("detail".into(), Value::str(detail))])
    }

    /// What Verse-Kit does after this error.
    pub fn action(&self) -> Action {
        let code = self.code.as_str();
        if FORGET_KEY.contains(&code) {
            Action::ForgetKey
        } else if RELOGIN.contains(&code) {
            Action::Relogin
        } else if code == "CLIENT_VERSION_UNSUPPORTED" {
            Action::Update
        } else if code == crate::testmode::BASETOOL_LOCKED || code == crate::testmode::WRITE_GUARD {
            // No answer came, but trying again would not change it.
            Action::Stop
        } else if RETRY.contains(&code) || self.status >= 500 || self.status == 0 {
            Action::Retry
        } else {
            Action::Stop
        }
    }

    /// A text value of the problem document, or `None`.
    pub fn problem_text(&self, key: &str) -> Option<&str> {
        self.problem.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str())
    }

    /// One line for the error log: code, status, request, wait, `detail`
    /// and `correlationId`. Never a header line, never a token.
    pub fn describe(&self) -> String {
        let mut parts = vec![self.code.clone(), format!("HTTP {}", self.status)];
        if !self.path.is_empty() {
            parts.push(self.path.clone());
        }
        if let Some(wait) = self.retry_after.filter(|w| *w > 0) {
            parts.push(format!("Retry-After {wait}s"));
        }
        for key in ["detail", "correlationId"] {
            if let Some((_, value)) = self.problem.iter().find(|(k, _)| k == key)
                && truthy(value)
            {
                parts.push(format!("{key}={}", py_str(value).chars().take(160).collect::<String>()));
            }
        }
        parts.join(" · ")
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

/// Like `describe`: the confirmation address in the problem stays out.
impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiError({})", self.describe())
    }
}

impl std::error::Error for ApiError {}

/// Python's truth value of a JSON value.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => n != "0",
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Python's `str()` for the simple values; containers as JSON.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Str(s) => s.clone(),
        other => crate::jsonio::dumps(other, None, false),
    }
}

/// `Retry-After` in seconds, at least 1; nothing for an absent or unreadable
/// value (a date is not read).
pub fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    let text = crate::text::py_strip(value?).replace('_', "");
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(&text)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if negative {
        return Some(1);
    }
    let number = digits.bytes().fold(0u64, |n, b| n.saturating_mul(10).saturating_add(u64::from(b - b'0')));
    Some(number.max(1))
}

/// Pause after the `attempt`-th failure (1, 2, …) in seconds: at least 5,
/// doubled up to 5 minutes, with a random part on top (`jitter` from 0 to
/// below 1), and never shorter than `Retry-After`.
pub fn backoff(attempt: u32, retry_after: Option<u64>, jitter: f64) -> f64 {
    let exponent = attempt.saturating_sub(1).min(16);
    let base = (5.0 * f64::from(1u32 << exponent)).min(300.0);
    let mut wait = (base + base * 0.2 * jitter).min(300.0);
    if let Some(seconds) = retry_after.filter(|s| *s > 0) {
        wait = wait.max(seconds as f64);
    }
    wait
}

/// A random number from 0 to below 1 for `backoff`.
pub fn jitter() -> f64 {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return 0.5;
    }
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// Error code to the text key of the user interface; `None` means the
/// general sentence with the code (`s_btv_f_allgemein`).
pub fn error_text_key(code: &str) -> Option<&'static str> {
    if code.starts_with("HTTP_5") {
        return Some("s_btv_f_netz");
    }
    Some(match code {
        // Maintenance and a busy relay are not a refusal (4.12 fell back on the general text here): the sync
        // tries again on its own, which is what the text says.
        "NETWORK"
        | "SERVICE_UNAVAILABLE"
        | "EXCHANGE_DISABLED"
        | "REGISTRY_UNAVAILABLE"
        | "RELAY_BUSY"
        | "BACKEND_RELAY_FAILED" => "s_btv_f_netz",
        "NOT_CONNECTED" | "CLIENT_REVOKED" | "INSTALLATION_REVOKED" => "s_btv_f_getrennt",
        "invalid_grant" | "UNAUTHENTICATED" => "s_btv_f_abgelaufen",
        "CLIENT_NOT_ALLOWED" | "unauthorized_client" | "invalid_client" => "s_btv_f_nicht_zugelassen",
        "CLIENT_SUSPENDED" => "s_btv_f_gesperrt",
        "CLIENT_VERSION_UNSUPPORTED" => "s_btv_f_version",
        "TERMS_NOT_ACCEPTED" => "s_btv_f_bedingungen",
        "PENDING_APPROVAL" => "s_btv_f_wartet",
        "NO_ROLE" | "ACTING_MEMBER_REFUSED" => "s_btv_f_rolle",
        "SCOPE_MISSING" => "s_btv_f_rechte",
        "ISSUER_MISMATCH" => "s_btv_f_aussteller",
        "QUOTA_EXCEEDED" => "s_btv_f_kontingent",
        "RATE_LIMITED" => "s_btv_f_bremse",
        "account_mismatch" => "s_btv_konto_falsch",
        "account_unknown" => "s_btv_konto_unbekannt",
        "account_all" => "s_btv_konto_alle",
        "account_none" => "s_btv_konto_keiner",
        "INSTALLATION_ID_MISSING" => "s_btv_f_kennung",
        "SECRET_REFUSED" | "SECRET_FAILED" => "s_btv_f_schluesselbund",
        "SECRET_UNSAFE" => "s_btv_f_datei_offen",
        crate::testmode::BASETOOL_LOCKED => "s_btv_testbetrieb",
        crate::testmode::WRITE_GUARD => "s_btv_f_schreibwache",
        _ => return None,
    })
}

/// The shown text for an error code.
pub fn error_text(code: &str) -> String {
    match error_text_key(code) {
        Some(key) => crate::language::t(key),
        None => crate::language::tf("s_btv_f_allgemein", &[("code", crate::language::Arg::from(code))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_like_python() {
        assert_eq!(ApiError::new("INSTALLATION_REVOKED", 401).action(), Action::ForgetKey);
        assert_eq!(ApiError::new("CLIENT_REVOKED", 401).action(), Action::Relogin);
        assert_eq!(ApiError::new("CLIENT_VERSION_UNSUPPORTED", 403).action(), Action::Update);
        assert_eq!(ApiError::new("RATE_LIMITED", 429).action(), Action::Retry);
        assert_eq!(ApiError::new("ANYTHING", 502).action(), Action::Retry);
        assert_eq!(ApiError::new("ANYTHING", 0).action(), Action::Retry);
        assert_eq!(ApiError::new("NOT_PERMITTED", 403).action(), Action::Stop);
        // The lock of the test mode comes without an answer and still halts: no retries in the background.
        assert_eq!(ApiError::new(crate::testmode::BASETOOL_LOCKED, 0).action(), Action::Stop);
        assert_eq!(ApiError::new(crate::testmode::WRITE_GUARD, 0).action(), Action::Stop, "no retries either");
        assert_eq!(ApiError::new("", 400).code, "UNKNOWN");
    }

    #[test]
    fn describe_names_no_header() {
        let mut error = ApiError::new("RATE_LIMITED", 429).with_retry_after(Some(12)).with_problem(vec![
            ("detail".into(), Value::str("x".repeat(200))),
            ("correlationId".into(), Value::str("c-1")),
            ("confirmationUrl".into(), Value::str("https://geheim")),
        ]);
        error.path = "GET /me/blueprints".into();
        let line = error.describe();
        assert!(line.starts_with("RATE_LIMITED · HTTP 429 · GET /me/blueprints · Retry-After 12s · detail="));
        assert!(line.ends_with(" · correlationId=c-1"));
        assert!(!line.contains("geheim"));
        assert_eq!(line.matches('x').count(), 160);
    }

    #[test]
    fn backoff_grows_and_respects_retry_after() {
        assert_eq!(backoff(1, None, 0.0), 5.0);
        assert_eq!(backoff(2, None, 0.0), 10.0);
        assert_eq!(backoff(1, None, 0.999), 5.0 + 0.999);
        assert_eq!(backoff(7, None, 0.0), 300.0);
        assert_eq!(backoff(40, None, 0.9), 300.0);
        assert_eq!(backoff(1, Some(3600), 0.0), 3600.0);
        assert_eq!(backoff(0, None, 0.0), 5.0);
        let j = jitter();
        assert!((0.0..1.0).contains(&j));
    }

    #[test]
    fn retry_after_like_int() {
        assert_eq!(parse_retry_after(Some(" 30 ")), Some(30));
        assert_eq!(parse_retry_after(Some("0")), Some(1));
        assert_eq!(parse_retry_after(Some("-5")), Some(1));
        assert_eq!(parse_retry_after(Some("1_0")), Some(10));
        assert_eq!(parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(parse_retry_after(Some("1.5")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn error_texts_use_keys_without_long_dash() {
        assert_eq!(error_text_key("HTTP_503"), Some("s_btv_f_netz"));
        assert_eq!(error_text_key("SCOPE_MISSING"), Some("s_btv_f_rechte"));
        assert_eq!(error_text_key("WHATEVER"), None);
    }

    #[test]
    fn every_error_text_exists_without_long_dash() {
        use crate::language::{Lang, text_in};
        let codes = [
            "HTTP_502",
            "NETWORK",
            "NOT_CONNECTED",
            "CLIENT_REVOKED",
            "INSTALLATION_REVOKED",
            "invalid_grant",
            "UNAUTHENTICATED",
            "CLIENT_NOT_ALLOWED",
            "unauthorized_client",
            "invalid_client",
            "CLIENT_SUSPENDED",
            "CLIENT_VERSION_UNSUPPORTED",
            "TERMS_NOT_ACCEPTED",
            "PENDING_APPROVAL",
            "NO_ROLE",
            "ACTING_MEMBER_REFUSED",
            "SCOPE_MISSING",
            "ISSUER_MISMATCH",
            "QUOTA_EXCEEDED",
            "RATE_LIMITED",
            "account_mismatch",
            "account_unknown",
            "account_all",
            "account_none",
            "INSTALLATION_ID_MISSING",
            "SECRET_REFUSED",
            "SECRET_FAILED",
            "SECRET_UNSAFE",
            crate::testmode::BASETOOL_LOCKED,
            crate::testmode::WRITE_GUARD,
        ];
        for code in codes {
            let key = error_text_key(code).unwrap_or_default();
            for lang in [Lang::De, Lang::En] {
                let text = text_in(lang, key).unwrap_or_default();
                assert!(!text.is_empty() && !text.contains('—'), "{code} → {key}: {text:?}");
            }
        }
    }
}
