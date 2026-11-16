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

//! The way out to other servers, for the daily update check and for `versekit-orgcheck`: HTTPS with rustls (ring)
//! and the certificates of the system, plain HTTP only to the own computer, no redirects, a time limit.

use std::sync::Arc;
use std::time::Duration;

use crate::conformance::Exchange;
use crate::http::Response;

/// The longest a request may take.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// The largest body read.
pub const BODY_LIMIT: u64 = 4 * 1024 * 1024;

/// A client for requests out.
pub struct Client {
    agent: ureq::Agent,
    user_agent: String,
}

impl Client {
    /// A client that names itself `versekit-orgserver/<version>` and nothing else.
    pub fn new() -> Client {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .accept(ureq::config::AutoHeaderValue::None)
            .user_agent(ureq::config::AutoHeaderValue::None)
            .accept_encoding(ureq::config::AutoHeaderValue::None)
            .timeout_global(Some(TIMEOUT))
            .tls_config(tls)
            .build()
            .new_agent();
        Client { agent, user_agent: format!("versekit-orgserver/{}", env!("CARGO_PKG_VERSION")) }
    }
}

impl Default for Client {
    fn default() -> Client {
        Client::new()
    }
}

fn plain_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

impl Exchange for Client {
    fn exchange(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
        if !url.starts_with("https://") && !plain_allowed(url) {
            return Err(format!("only https (http only to the own computer): {url}"));
        }
        let mut builder = ureq::http::Request::builder().method(method).uri(url).header("User-Agent", self.user_agent.as_str());
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(body.to_vec()).map_err(|e| e.to_string())?;
        let mut answer = self.agent.run(request).map_err(|e| e.to_string())?;
        let status = answer.status().as_u16();
        let headers = answer
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_string(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let body = answer.body_mut().with_config().limit(BODY_LIMIT).read_to_vec().map_err(|e| e.to_string())?;
        Ok(Response { status, headers, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_goes_only_to_the_own_computer() {
        let client = Client::new();
        let refused = client.exchange("GET", "http://orga.example.org/", &[], b"").unwrap_err();
        assert!(refused.contains("only https"));
        assert!(plain_allowed("http://127.0.0.1:8110/x") && plain_allowed("http://[::1]:1/") && plain_allowed("http://localhost"));
        assert!(!plain_allowed("http://127.0.0.1.example.org/"));
        assert!(!plain_allowed("http://127.0.0.1:1@evil.example/"), "no user part before another host"); // privacy-ok: reserved example domain
    }
}
