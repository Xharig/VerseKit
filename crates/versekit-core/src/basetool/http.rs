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

//! Sending one request (`basetool._http`). Refusals come back as a status,
//! not as an error; only a network failure is `NETWORK`.
//!
//! TLS through rustls with the ring provider; the certificate is checked by
//! the system (Windows, macOS) or against the system's root store (Linux).
//! Plain `http` only to the own computer, where the test server runs.
//! Redirects are not followed: the proof names exactly one address, and a
//! redirect must not carry a token elsewhere.

use std::sync::Arc;
use std::time::Duration;

use crate::basetool::error::ApiError;
use crate::basetool::jose::UrlParts;
use crate::legacy;
use crate::paths::Host;

pub const TIMEOUT: Duration = Duration::from_secs(30);
/// Largest answer read; a snapshot page of 500 lots is far below.
pub const BODY_LIMIT: u64 = 64 * 1024 * 1024;

/// One request.
#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// One answer, whatever its status.
#[derive(Clone, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn header_names(headers: &[(String, String)]) -> Vec<&str> {
    headers.iter().map(|(name, _)| name.as_str()).collect()
}

/// Header values and bodies carry tokens and codes: only names and sizes.
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.url.split(['?', '#']).next().unwrap_or_default();
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("url", &path)
            .field("headers", &header_names(&self.headers))
            .field("body_bytes", &self.body.as_ref().map(Vec::len))
            .finish()
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &header_names(&self.headers))
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// Sends requests; the test server and recorded answers implement it too.
pub trait Transport: Send + Sync {
    fn send(&self, request: &Request) -> Result<Response, ApiError>;
}

fn loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// The real network.
pub struct HttpTransport {
    agent: ureq::Agent,
    /// For the own computer: no proxy of the environment.
    local_agent: ureq::Agent,
    /// The house switch "no network": only the own computer stays reachable.
    offline: bool,
}

impl HttpTransport {
    /// `ca`: certificate file of the test environment, otherwise empty.
    pub fn new(ca: &str, host: &Host) -> Result<HttpTransport, ApiError> {
        let roots = if ca.is_empty() {
            ureq::tls::RootCerts::PlatformVerifier
        } else {
            let pem = std::fs::read(ca).map_err(|e| ApiError::internal("CA_UNREADABLE", &e.to_string()))?;
            let certs: Vec<_> = ureq::tls::parse_pem(&pem)
                .filter_map(|item| match item {
                    Ok(ureq::tls::PemItem::Certificate(cert)) => Some(cert),
                    _ => None,
                })
                .collect();
            if certs.is_empty() {
                return Err(ApiError::internal("CA_UNREADABLE", "no certificate in the file"));
            }
            ureq::tls::RootCerts::new_with_certs(&certs)
        };
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(roots)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let agent = |proxy: bool| {
            let builder = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                // Only the headers the request names, none of ureq's own.
                .accept(ureq::config::AutoHeaderValue::None)
                .user_agent(ureq::config::AutoHeaderValue::None)
                .accept_encoding(ureq::config::AutoHeaderValue::None)
                .timeout_global(Some(TIMEOUT))
                .tls_config(tls.clone());
            // The own computer is never reached through a proxy of the
            // environment: a made-up server stays on this computer.
            let builder = if proxy { builder } else { builder.proxy(None) };
            builder.build().new_agent()
        };
        let offline = host.var(legacy::ENV_NO_NET).is_some_and(|v| !v.is_empty() && v != "0");
        Ok(HttpTransport { agent: agent(true), local_agent: agent(false), offline })
    }
}

impl Transport for HttpTransport {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let parts = UrlParts::split(&request.url).map_err(|_| ApiError::network("ValueError"))?;
        let host = parts.hostname().unwrap_or_default();
        if self.offline && !loopback(&host) {
            return Err(ApiError::network(legacy::ENV_NO_NET));
        }
        if parts.scheme != "https" && !(parts.scheme == "http" && loopback(&host)) {
            return Err(ApiError::network("InsecureAddress"));
        }
        let mut builder = ureq::http::Request::builder().method(request.method.as_str()).uri(request.url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let built =
            builder.body(request.body.clone().unwrap_or_default()).map_err(|_| ApiError::network("ValueError"))?;
        let agent = if loopback(&host) { &self.local_agent } else { &self.agent };
        let mut answer = agent.run(built).map_err(|e| ApiError::network(error_kind(&e)))?;
        let status = answer.status().as_u16();
        let headers = answer
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        // A refusal whose text breaks off counts as one without text. A
        // successful answer that breaks off is a network error: read as
        // empty, a snapshot would look like an empty Basetool.
        let body = match answer.body_mut().with_config().limit(BODY_LIMIT).read_to_vec() {
            Ok(body) => body,
            Err(_) if !(200..300).contains(&status) => Vec::new(),
            Err(ureq::Error::BodyExceedsLimit(_)) => return Err(ApiError::network("BodyTooLarge")),
            Err(_) => return Err(ApiError::network("IncompleteRead")),
        };
        Ok(Response { status, headers, body })
    }
}

/// The kind of a network failure, without the address (it may carry a
/// cursor).
fn error_kind(error: &ureq::Error) -> &'static str {
    match error {
        ureq::Error::Timeout(_) => "TimeoutError",
        ureq::Error::Io(_) => "OSError",
        ureq::Error::HostNotFound => "HostNotFound",
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "SSLError",
        ureq::Error::ConnectionFailed => "ConnectionError",
        ureq::Error::BodyExceedsLimit(_) => "BodyTooLarge",
        _ => "URLError",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    #[test]
    fn no_net_and_plain_http_stay_local() {
        let host = Host::new(Flavor::Posix, &[(legacy::ENV_NO_NET, "1")], "/");
        let transport = HttpTransport::new("", &host).unwrap();
        let request = |url: &str| Request { method: "GET".into(), url: url.into(), headers: vec![], body: None };
        let refused = transport.send(&request("https://ingest.profit-base.online/exchange/v1")).unwrap_err();
        assert_eq!((refused.code.as_str(), refused.problem_text("detail")), ("NETWORK", Some(legacy::ENV_NO_NET)));
        let online = HttpTransport::new("", &Host::new(Flavor::Posix, &[], "/")).unwrap();
        let insecure = online.send(&request("http://example.org/")).unwrap_err();
        assert_eq!(insecure.problem_text("detail"), Some("InsecureAddress"));
    }
}
