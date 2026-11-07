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

//! Encoding helpers for DPoP proofs (`dpop_reference/jose.py`): base64url,
//! `htu`, `ath`, JWK thumbprints, PEM, and splitting addresses like
//! Python's `urlsplit`.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::digest;

use crate::jsonio::{self, Value};

/// Bytes as base64url without padding.
pub fn b64url_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

/// base64url with or without padding; `+`, `/` and inner `=` are refused.
pub fn b64url_decode(text: &str) -> Result<Vec<u8>, String> {
    let body = text.trim_end_matches('=');
    if body.contains(['+', '/', '=']) || body.len() % 4 == 1 {
        return Err("not base64url".into());
    }
    URL_SAFE_NO_PAD.decode(body).map_err(|_| "not base64url".to_string())
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> Vec<u8> {
    digest::digest(&digest::SHA256, data).as_ref().to_vec()
}

/// `ath`: base64url of the SHA-256 of the access token.
pub fn access_token_hash(access_token: &str) -> String {
    b64url_encode(&sha256(access_token.as_bytes()))
}

/// An address split like Python's `urllib.parse.urlsplit`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UrlParts {
    /// In lower case.
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

impl UrlParts {
    /// Tabs and line breaks are dropped, leading control characters and
    /// spaces too. An unbalanced `[` or `]` in the host part is an error.
    pub fn split(url: &str) -> Result<UrlParts, String> {
        let cleaned: String = url.chars().filter(|c| !matches!(c, '\t' | '\r' | '\n')).collect();
        let mut rest = cleaned.trim_start_matches(|c: char| c <= ' ');
        let mut parts = UrlParts::default();
        if let Some(i) = rest.find(':')
            && i > 0
            && rest.starts_with(|c: char| c.is_ascii_alphabetic())
            && rest[..i].chars().all(is_scheme_char)
        {
            parts.scheme = rest[..i].to_ascii_lowercase();
            rest = &rest[i + 1..];
        }
        if let Some(after) = rest.strip_prefix("//") {
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            parts.netloc = after[..end].to_string();
            rest = &after[end..];
            let (open, close) = (parts.netloc.contains('['), parts.netloc.contains(']'));
            if open != close {
                return Err("Invalid IPv6 URL".into());
            }
        }
        let (before_fragment, fragment) = rest.split_once('#').unwrap_or((rest, ""));
        let (path, query) = before_fragment.split_once('?').unwrap_or((before_fragment, ""));
        parts.path = path.to_string();
        parts.query = query.to_string();
        parts.fragment = fragment.to_string();
        Ok(parts)
    }

    fn host_info(&self) -> (&str, &str) {
        let info = self.netloc.rsplit_once('@').map_or(self.netloc.as_str(), |(_, h)| h);
        match info.split_once('[') {
            Some((_, bracketed)) => {
                let (host, after) = bracketed.split_once(']').unwrap_or((bracketed, ""));
                (host, after.split_once(':').map_or("", |(_, p)| p))
            }
            None => info.split_once(':').unwrap_or((info, "")),
        }
    }

    /// The host in lower case, `None` without one (`hostname`).
    pub fn hostname(&self) -> Option<String> {
        let (host, _) = self.host_info();
        if host.is_empty() {
            return None;
        }
        Some(match host.split_once('%') {
            Some((name, zone)) => format!("{}%{zone}", name.to_lowercase()),
            None => host.to_lowercase(),
        })
    }

    /// The port; an error if it is no number from 0 to 65535 (`port`).
    pub fn port(&self) -> Result<Option<u16>, String> {
        let (_, port) = self.host_info();
        if port.is_empty() {
            return Ok(None);
        }
        if !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("Port could not be cast to integer value as {port:?}"));
        }
        let value = port.bytes().try_fold(0u32, |n, b| n.checked_mul(10)?.checked_add(u32::from(b - b'0')));
        match value.and_then(|n| u16::try_from(n).ok()) {
            Some(number) => Ok(Some(number)),
            None => Err("Port out of range 0-65535".into()),
        }
    }

    /// Whether the address carries user information (`user@host`).
    pub fn has_userinfo(&self) -> bool {
        self.netloc.contains('@')
    }
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    }
}

/// `htu` of a request: scheme, host, a port other than the default, path;
/// no query and no fragment. The path stays exactly as sent.
pub fn normalize_htu(url: &str) -> Result<String, String> {
    let parts = UrlParts::split(url)?;
    let Some(default) = default_port(&parts.scheme) else {
        return Err("htu must be an http or https URL".into());
    };
    if parts.has_userinfo() {
        return Err("htu must not carry user info".into());
    }
    let Some(host) = parts.hostname() else { return Err("htu must name a host".into()) };
    let mut netloc = if host.contains(':') { format!("[{host}]") } else { host };
    if let Some(port) = parts.port()?
        && port != default
    {
        netloc = format!("{netloc}:{port}");
    }
    let path = if parts.path.is_empty() { "/" } else { parts.path.as_str() };
    Ok(format!("{}://{netloc}{path}", parts.scheme))
}

/// Scheme, host and port of a normalised address, the key of the nonce
/// cache.
pub fn nonce_origin(url: &str) -> Result<String, String> {
    let htu = normalize_htu(url)?;
    let start = htu.find("//").map_or(0, |i| i + 2);
    let end = htu[start..].find('/').map_or(htu.len(), |i| start + i);
    Ok(htu[..end].to_string())
}

/// Scheme and host part in lower case, the key of the clock offsets.
pub fn clock_origin(url: &str) -> String {
    match UrlParts::split(url) {
        Ok(parts) => format!("{}://{}", parts.scheme, parts.netloc.to_lowercase()),
        Err(_) => String::new(),
    }
}

/// The public part of a P-256 key, coordinates in base64url.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jwk {
    pub x: String,
    pub y: String,
}

impl Jwk {
    /// From the affine coordinates, 32 bytes each.
    pub fn from_coordinates(x: &[u8], y: &[u8]) -> Result<Jwk, String> {
        if x.len() != 32 || y.len() != 32 {
            return Err("P-256 coordinates are 32 bytes each".into());
        }
        Ok(Jwk { x: b64url_encode(x), y: b64url_encode(y) })
    }

    /// From an uncompressed point (`0x04 || x || y`).
    pub fn from_uncompressed(point: &[u8]) -> Result<Jwk, String> {
        match point.split_first() {
            Some((4, rest)) if rest.len() == 64 => Jwk::from_coordinates(&rest[..32], &rest[32..]),
            _ => Err("not an uncompressed P-256 point".into()),
        }
    }

    /// As in the proof header, in the order `kty`, `crv`, `x`, `y`.
    pub fn to_value(&self) -> Value {
        Value::Object(vec![
            ("kty".into(), Value::str("EC")),
            ("crv".into(), Value::str("P-256")),
            ("x".into(), Value::str(self.x.clone())),
            ("y".into(), Value::str(self.y.clone())),
        ])
    }

    /// RFC 7638 thumbprint: the required members, sorted, without spaces.
    pub fn thumbprint(&self) -> String {
        let text = format!(r#"{{"crv":"P-256","kty":"EC","x":"{}","y":"{}"}}"#, self.x, self.y);
        b64url_encode(&sha256(text.as_bytes()))
    }
}

/// JSON without spaces, keys in their order, other characters kept as they
/// are (`json.dumps(…, separators=(",", ":"), ensure_ascii=False)`).
pub fn compact_json(value: &Value) -> String {
    let mut out = String::new();
    push_compact(&mut out, value);
    out
}

fn push_compact(out: &mut String, value: &Value) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                jsonio::push_json_str(out, key);
                out.push(':');
                push_compact(out, item);
            }
            out.push('}');
        }
        other => out.push_str(&jsonio::dumps(other, None, false)),
    }
}

/// Every character above ASCII as `\uXXXX` (pairs above U+FFFF), as
/// `json.dumps(…, ensure_ascii=True)` writes it.
pub fn ascii_only(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// DER bytes as PEM with 64 characters per line, as OpenSSL writes it.
pub fn pem_encode(label: &str, der: &[u8]) -> String {
    let body = STANDARD.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in body.as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(chunk));
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

/// The DER bytes of the first PEM block with `label`.
pub fn pem_decode(label: &str, pem: &str) -> Result<Vec<u8>, String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = pem.find(&begin).ok_or("no PEM block")? + begin.len();
    let stop = pem[start..].find(&end).ok_or("PEM block without end")? + start;
    let body: String = pem[start..stop].chars().filter(|c| !c.is_whitespace()).collect();
    STANDARD.decode(body).map_err(|_| "PEM body is not base64".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trip_and_refusals() {
        assert_eq!(b64url_encode(&[0xfb, 0xff]), "-_8");
        assert_eq!(b64url_decode("-_8").unwrap(), vec![0xfb, 0xff]);
        assert_eq!(b64url_decode("-_8=").unwrap(), vec![0xfb, 0xff]);
        for bad in ["+/8", "a", "ab=c", "a=b"] {
            assert!(b64url_decode(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn htu_like_the_reference() {
        let cases = [
            (
                "https://Ingest.Profit-Base.online/exchange/v1/me?cursor=x#f",
                "https://ingest.profit-base.online/exchange/v1/me",
            ),
            ("HTTPS://host:443", "https://host/"),
            ("http://127.0.0.1:8080/a", "http://127.0.0.1:8080/a"),
            ("http://[::1]:80/x", "http://[::1]/x"),
            ("https://h/%7Euser/a%20b", "https://h/%7Euser/a%20b"),
        ];
        for (url, htu) in cases {
            assert_eq!(normalize_htu(url).as_deref(), Ok(htu), "{url}");
        }
        for bad in ["ftp://h/", "https://user@h/", "https:///x", "https://h:99999/", "https://h:x/", "https://[::1/"] {
            assert!(normalize_htu(bad).is_err(), "{bad}");
        }
        assert_eq!(nonce_origin("https://H:8443/a/b?c").unwrap(), "https://h:8443");
        assert_eq!(clock_origin("HTTPS://H.example/a"), "https://h.example");
    }

    #[test]
    fn thumbprint_of_the_rfc_example() {
        // The example key of RFC 7517, appendix A.1; the thumbprint is the
        // one the Python reference computes.
        let jwk = Jwk {
            x: "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU".into(),
            y: "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0".into(),
        };
        assert_eq!(jwk.thumbprint(), "oKIywvGUpTVTyxMQ3bwIIeQUudfr_CkLMjCE19ECD-U");
        assert_eq!(
            compact_json(&jwk.to_value()),
            r#"{"kty":"EC","crv":"P-256","x":"f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU","y":"x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"}"#
        );
    }

    #[test]
    fn ascii_escapes_like_python() {
        assert_eq!(ascii_only("ä€😀a"), "\\u00e4\\u20ac\\ud83d\\ude00a");
    }

    #[test]
    fn pem_round_trip() {
        let der: Vec<u8> = (0..100).collect();
        let pem = pem_encode("PRIVATE KEY", &der);
        assert!(pem.lines().all(|l| l.len() <= 64));
        assert_eq!(pem_decode("PRIVATE KEY", &pem).unwrap(), der);
    }
}
