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

//! DPoP proofs (RFC 9449), the server nonce and the server's clock
//! (`dpop_reference/proof.py`, `basetool._Clock`).
//!
//! Every request gets a fresh proof signed with the key of this
//! installation. The first request is answered with `use_dpop_nonce`; that
//! is normal: repeat once with the nonce, every answer brings a fresh one.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::basetool::jose::{self, Jwk, b64url_encode, clock_origin, compact_json, nonce_origin, normalize_htu};
use crate::jsonio::Value;

pub const PROOF_TYPE: &str = "dpop+jwt";
pub const ALGORITHM: &str = "ES256";
pub const NONCE_HEADER: &str = "DPoP-Nonce";
pub const USE_DPOP_NONCE: &str = "use_dpop_nonce";

/// Why a key could not be opened, created or used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyError(pub String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeyError {}

/// The key of this installation; only its public part leaves it.
pub trait DpopKey: Send + Sync {
    fn public_jwk(&self) -> Jwk;
    /// ES256 over `data`: the SHA-256 signed, as `r || s` (64 bytes).
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, KeyError>;
}

/// The parts of a proof that are not given by the request.
#[derive(Clone, Debug, Default)]
pub struct ProofInput<'a> {
    /// The access token the request carries (adds `ath`); `None` at the
    /// token endpoint.
    pub access_token: Option<&'a str>,
    /// The latest `DPoP-Nonce` of the server, if one is known.
    pub nonce: Option<&'a str>,
    /// `iat` in seconds since 1970.
    pub issued_at: i64,
    /// The unique id; 144 random bits when absent.
    pub jti: Option<&'a str>,
}

/// 144 random bits in base64url (`secrets.token_urlsafe(18)`).
pub fn random_jti() -> Result<String, KeyError> {
    let mut bytes = [0u8; 18];
    getrandom::fill(&mut bytes).map_err(|e| KeyError(format!("no random numbers: {e}")))?;
    Ok(b64url_encode(&bytes))
}

/// Header and claims joined with a dot, before the signature.
pub fn signing_input(jwk: &Jwk, method: &str, url: &str, input: &ProofInput<'_>) -> Result<String, KeyError> {
    if method.is_empty() {
        return Err(KeyError("method is required".into()));
    }
    let header = Value::Object(vec![
        ("typ".into(), Value::str(PROOF_TYPE)),
        ("alg".into(), Value::str(ALGORITHM)),
        ("jwk".into(), jwk.to_value()),
    ]);
    let jti = match input.jti {
        Some(jti) if !jti.is_empty() => jti.to_string(),
        _ => random_jti()?,
    };
    let mut claims = vec![
        ("jti".into(), Value::str(jti)),
        ("htm".into(), Value::str(method.to_uppercase())),
        ("htu".into(), Value::str(normalize_htu(url).map_err(KeyError)?)),
        ("iat".into(), Value::int(input.issued_at)),
    ];
    if let Some(token) = input.access_token {
        claims.push(("ath".into(), Value::str(jose::access_token_hash(token))));
    }
    if let Some(nonce) = input.nonce.filter(|n| !n.is_empty()) {
        claims.push(("nonce".into(), Value::str(nonce)));
    }
    Ok(format!(
        "{}.{}",
        b64url_encode(compact_json(&header).as_bytes()),
        b64url_encode(compact_json(&Value::Object(claims)).as_bytes())
    ))
}

/// A signed proof for one request (compact JWS).
pub fn build_proof(key: &dyn DpopKey, method: &str, url: &str, input: &ProofInput<'_>) -> Result<String, KeyError> {
    let signing = signing_input(&key.public_jwk(), method, url, input)?;
    let signature = key.sign(signing.as_bytes())?;
    Ok(format!("{signing}.{}", b64url_encode(&signature)))
}

/// A header value, the name compared without case; the first match.
pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// Whether an answer is a nonce challenge that one retry resolves: `401`
/// with `WWW-Authenticate: DPoP …, error="use_dpop_nonce"` from the API,
/// `400` with the JSON error `use_dpop_nonce` from the token endpoint. Both
/// carry the nonce in `DPoP-Nonce`.
pub fn asks_for_nonce(status: u16, headers: &[(String, String)], body_error: Option<&str>) -> bool {
    if header(headers, NONCE_HEADER).is_none() {
        return false;
    }
    if status == 401 {
        let challenge = header(headers, "WWW-Authenticate").unwrap_or_default();
        return challenge.contains(&format!("error=\"{USE_DPOP_NONCE}\""));
    }
    status == 400 && body_error == Some(USE_DPOP_NONCE)
}

/// The latest nonce of each server origin.
#[derive(Default)]
pub struct NonceCache {
    nonces: Mutex<HashMap<String, String>>,
}

impl NonceCache {
    pub fn get(&self, url: &str) -> Option<String> {
        let origin = nonce_origin(url).ok()?;
        self.nonces.lock().ok()?.get(&origin).cloned()
    }

    /// Keeps the `DPoP-Nonce` of an answer, if it carries one.
    pub fn update(&self, url: &str, headers: &[(String, String)]) {
        if let Some(nonce) = header(headers, NONCE_HEADER).filter(|n| !n.is_empty())
            && let Ok(origin) = nonce_origin(url)
            && let Ok(mut nonces) = self.nonces.lock()
        {
            nonces.insert(origin, nonce.to_string());
        }
    }
}

/// The clock offset of each server, learned from the `Date` of its answers.
///
/// ⚠ The login server accepts an `iat` only from about 25 s before to 15 s
/// after its own time. A computer whose clock is 15 s fast would fail at the
/// login, so proofs use the time of the server they go to.
#[derive(Default)]
pub struct ServerClock {
    offsets: Mutex<HashMap<String, f64>>,
}

impl ServerClock {
    /// Learns the offset from the `Date` header; `now` is the own time.
    pub fn learn(&self, url: &str, headers: &[(String, String)], now: f64) {
        let Some(server) = header(headers, "Date").and_then(parse_http_date) else { return };
        if let Ok(mut offsets) = self.offsets.lock() {
            offsets.insert(clock_origin(url), server - now);
        }
    }

    /// The server's time for `url` in whole seconds.
    pub fn now(&self, url: &str, now: f64) -> i64 {
        let offset = self.offsets.lock().ok().and_then(|o| o.get(&clock_origin(url)).copied()).unwrap_or(0.0);
        (now + offset).trunc() as i64
    }
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// Days since 1970-01-01 for a date of the Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// An e-mail style date (`Wed, 21 Oct 2026 07:28:00 GMT`) as seconds since
/// 1970. Zones: `GMT`, `UT`, `UTC`, `Z` and `±hhmm`; anything else counts
/// as UTC. Two-digit years as in RFC 2822.
pub fn parse_http_date(text: &str) -> Option<f64> {
    let cleaned = text.replace(',', " ");
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    // The header may carry any bytes: cut only with `get`, never by index.
    let month_of = |word: &str| {
        let head = word.get(..3)?;
        MONTHS.iter().position(|m| head.eq_ignore_ascii_case(m))
    };
    let digits = |word: &str| -> Option<i64> {
        if word.is_empty() || word.len() > 4 || !word.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        word.parse().ok()
    };
    if words.first().is_some_and(|w| w.chars().all(char::is_alphabetic) && month_of(w).is_none()) {
        words.remove(0);
    }
    if words.len() < 4 {
        return None;
    }
    let (mut day, mut month) = (words[0], words[1]);
    if digits(day).is_none() {
        std::mem::swap(&mut day, &mut month);
    }
    let day = digits(day)?;
    let month = month_of(month)? as i64 + 1;
    let mut year = digits(words[2])?;
    if words[2].len() <= 2 {
        year += if year > 68 { 1900 } else { 2000 };
    } else if words[2].len() == 3 {
        year += 1900;
    }
    let mut clock = words[3].split(':');
    let hour = digits(clock.next()?)?;
    let minute = digits(clock.next()?)?;
    let second = match clock.next() {
        Some(s) => digits(s)?,
        None => 0,
    };
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 61 {
        return None;
    }
    let offset = match words.get(4) {
        Some(zone) if zone.len() == 5 && (zone.starts_with('+') || zone.starts_with('-')) => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let hh = digits(zone.get(1..3)?)?;
            let mm = digits(zone.get(3..5)?)?;
            sign * (hh * 3600 + mm * 60)
        }
        _ => 0,
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    Some(seconds as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    impl DpopKey for Fixed {
        fn public_jwk(&self) -> Jwk {
            Jwk { x: "X".into(), y: "Y".into() }
        }
        fn sign(&self, _data: &[u8]) -> Result<Vec<u8>, KeyError> {
            Ok(vec![1; 64])
        }
    }

    #[test]
    fn proof_parts_in_the_reference_order() {
        let input =
            ProofInput { access_token: Some("tok"), nonce: Some("n1"), issued_at: 1_760_000_000, jti: Some("j") };
        let proof = build_proof(&Fixed, "post", "https://H.example:443/a?b=c", &input).unwrap();
        let parts: Vec<&str> = proof.split('.').collect();
        let header = String::from_utf8(jose::b64url_decode(parts[0]).unwrap()).unwrap();
        let claims = String::from_utf8(jose::b64url_decode(parts[1]).unwrap()).unwrap();
        assert_eq!(header, r#"{"typ":"dpop+jwt","alg":"ES256","jwk":{"kty":"EC","crv":"P-256","x":"X","y":"Y"}}"#);
        assert_eq!(
            claims,
            format!(
                r#"{{"jti":"j","htm":"POST","htu":"https://h.example/a","iat":1760000000,"ath":"{}","nonce":"n1"}}"#,
                jose::access_token_hash("tok")
            )
        );
        assert_eq!(parts[2], b64url_encode(&[1; 64]));
        let fresh = build_proof(&Fixed, "GET", "https://h/", &ProofInput::default()).unwrap();
        let claims = String::from_utf8(jose::b64url_decode(fresh.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert!(!claims.contains("ath") && !claims.contains("nonce"));
        assert_eq!(random_jti().unwrap().len(), 24);
        assert!(build_proof(&Fixed, "", "https://h/", &ProofInput::default()).is_err());
    }

    #[test]
    fn nonce_challenges() {
        let h = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let api = h(&[("dpop-nonce", "n"), ("WWW-Authenticate", r#"DPoP error="use_dpop_nonce""#)]);
        assert!(asks_for_nonce(401, &api, None));
        assert!(!asks_for_nonce(401, &h(&[("WWW-Authenticate", r#"DPoP error="use_dpop_nonce""#)]), None));
        assert!(asks_for_nonce(400, &h(&[("DPoP-Nonce", "n")]), Some("use_dpop_nonce")));
        assert!(!asks_for_nonce(400, &h(&[("DPoP-Nonce", "n")]), Some("invalid_grant")));
        let cache = NonceCache::default();
        cache.update("https://h.example/token", &h(&[("DPoP-Nonce", "n2")]));
        assert_eq!(cache.get("https://H.example/other?x").as_deref(), Some("n2"));
        assert_eq!(cache.get("https://other.example/"), None);
    }

    #[test]
    fn server_clock_from_the_date_header() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:10 GMT"), Some(10.0));
        assert_eq!(parse_http_date("Wed, 21 Oct 2026 07:28:00 GMT"), Some(1_792_567_680.0));
        assert_eq!(parse_http_date("21 Oct 2026 09:28:00 +0200"), Some(1_792_567_680.0));
        assert_eq!(parse_http_date("29 Feb 2024 23:59:59 UTC"), Some(1_709_251_199.0));
        assert_eq!(parse_http_date("kaputt"), None);
        // Bytes above ASCII never panic.
        for odd in [
            "ää, 01 Jan 2026 00:00:00 GMT",
            "01 Jän 2026 00:00:00 GMT",
            "01 Jan 2026 00:00:00 +1ä1",
            "01 Jan 99999999999999999 00:00:00 GMT",
            "01 Jan 2026 -1:00:00 GMT",
        ] {
            let _ = parse_http_date(odd);
        }
        assert_eq!(parse_http_date("01 Jan 2026 -1:00:00 GMT"), None);
        let clock = ServerClock::default();
        let headers = vec![("date".to_string(), "Thu, 01 Jan 1970 00:01:40 GMT".to_string())];
        clock.learn("https://login.example/token", &headers, 90.5);
        assert_eq!(clock.now("https://LOGIN.example/other", 1000.0), 1009);
        assert_eq!(clock.now("https://api.example/", 1000.9), 1000);
    }
}
