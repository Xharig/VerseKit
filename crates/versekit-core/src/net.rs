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

//! Fetching from the net behind one door, so that tests can put a stand-in
//! there and never touch the net.
//!
//! [`Web`] is the real one: HTTPS with the system's certificates, a time
//! limit and a size limit; with `SC_BP_NO_NET` set it fetches nothing.

use std::sync::Arc;
use std::time::Duration;

use crate::legacy::ENV_NO_NET;
pub use crate::scmdb::FetchError;

/// What the servers see of the program: `VerseKit/<version> (+<homepage>)`, the version of this build.
/// The same text as [`crate::basetool::config::user_agent`] gives for it.
pub const USER_AGENT: &str = concat!("VerseKit/", env!("CARGO_PKG_VERSION"), " (+https://versekit.xharig.com)");

/// Header lines of an answer, names in lower case.
pub type Headers = Vec<(String, String)>;

/// The longest pause a server's limit is believed (seconds).
const LIMIT_WAIT_MAX: u64 = 3600;
/// The pause when a server says its limit is used up but not for how long (seconds).
const LIMIT_WAIT_DEFAULT: u64 = 900;

/// How long to leave a server alone after `status` and `headers`, or `None` when the answer is no limit.
/// 429, or 403 with `Retry-After` or `X-RateLimit-Remaining: 0`; the pause is `Retry-After`, else the time to
/// `X-RateLimit-Reset` (Unix seconds, `now` is the same), else [`LIMIT_WAIT_DEFAULT`]; never more than an hour.
fn rate_limit_wait(status: u16, headers: &Headers, now: u64) -> Option<u64> {
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.trim());
    let retry = header("retry-after").and_then(|v| v.parse::<u64>().ok());
    let used_up = header("x-ratelimit-remaining") == Some("0");
    if !(status == 429 || (status == 403 && (retry.is_some() || used_up))) {
        return None;
    }
    let reset = header("x-ratelimit-reset").and_then(|v| v.parse::<u64>().ok()).map(|at| at.saturating_sub(now));
    Some(retry.or(reset).unwrap_or(LIMIT_WAIT_DEFAULT).clamp(1, LIMIT_WAIT_MAX))
}

/// What a conditional fetch brought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fresh {
    /// The body and the tag the server gave it (`ETag`).
    Body(Vec<u8>, Option<String>),
    /// The server still has the tag asked with (`304 Not Modified`): nothing was sent.
    Unchanged,
}

/// Something that fetches addresses.
pub trait Net: Send + Sync {
    /// The body of `url`.
    ///
    /// # Errors
    ///
    /// When the address cannot be fetched or fetching is switched off.
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError>;

    /// Only the header of `url` (`HEAD`).
    ///
    /// # Errors
    ///
    /// As [`Net::get`].
    fn head(&self, url: &str) -> Result<Headers, FetchError>;

    /// The answer of `url` to `body` sent as `content_type` (`POST`); a door that cannot send fails.
    ///
    /// # Errors
    ///
    /// As [`Net::get`].
    fn post(&self, url: &str, content_type: &str, body: &[u8]) -> Result<Vec<u8>, FetchError> {
        let _ = (url, content_type, body);
        Err(FetchError::Failed("POST".to_string()))
    }

    /// The body of `url` unless it still has the tag `etag` (`If-None-Match`); without a tag, or for a door
    /// that does not ask conditionally, the whole body.
    ///
    /// # Errors
    ///
    /// As [`Net::get`].
    fn get_if_changed(&self, url: &str, etag: Option<&str>) -> Result<Fresh, FetchError> {
        let _ = etag;
        self.get(url).map(|body| Fresh::Body(body, None))
    }

    /// The body of `url` written into `out` piece by piece, for files too large to hold at once.
    /// `progress` hears the bytes written so far and the whole length when the server names it.
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// As [`Net::get`], and when writing into `out` fails.
    fn download(
        &self,
        url: &str,
        out: &mut dyn std::io::Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u64, FetchError> {
        let body = self.get(url)?;
        out.write_all(&body).map_err(|e| FetchError::Failed(e.to_string()))?;
        let length = body.len() as u64;
        progress(length, Some(length));
        Ok(length)
    }
}

/// Whether fetching is switched off (`SC_BP_NO_NET`) for the running process. Only for the
/// door to the real net itself; logic decides by [`offline_in`] with its own environment.
pub fn offline() -> bool {
    std::env::var(ENV_NO_NET).is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Whether fetching is switched off (`SC_BP_NO_NET`) on `host`: the environment the program
/// was given, so that a check sets its own and does not depend on the shell it runs in.
pub fn offline_in(host: &crate::paths::Host) -> bool {
    host.var(ENV_NO_NET).is_some_and(|value| !value.is_empty() && value != "0")
}

/// The real net.
#[derive(Clone, Debug)]
pub struct Web {
    pub user_agent: String,
    pub timeout: Duration,
    /// The largest body read.
    pub limit: u64,
}

impl Web {
    pub fn new(user_agent: &str, timeout: Duration, limit: u64) -> Self {
        Self { user_agent: user_agent.to_string(), timeout, limit }
    }

    fn agent(&self) -> ureq::Agent {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .build();
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            // Every address is https; a redirect must not lead to plain http.
            .https_only(true)
            .timeout_global(Some(self.timeout))
            .user_agent(&self.user_agent)
            .tls_config(tls)
            .build()
            .new_agent()
    }
}

/// How much of a download is read and written at a time.
const DOWNLOAD_BLOCK: usize = 256 * 1024;

fn status_error(status: u16) -> FetchError {
    match status {
        403 => FetchError::Refused,
        404 => FetchError::Missing,
        status => FetchError::Failed(format!("HTTP {status}")),
    }
}

/// The error of an answer that is no success: a used-up limit with its pause, else by the status.
fn failure(answer: &ureq::http::Response<ureq::Body>) -> FetchError {
    let headers: Headers = answer
        .headers()
        .iter()
        .map(|(name, value)| (name.as_str().to_lowercase(), value.to_str().unwrap_or("").to_string()))
        .collect();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let status = answer.status().as_u16();
    rate_limit_wait(status, &headers, now).map_or_else(|| status_error(status), FetchError::RateLimited)
}

impl Net for Web {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if offline() {
            return Err(FetchError::Offline);
        }
        let mut answer = self.agent().get(url).call().map_err(|e| FetchError::Failed(e.to_string()))?;
        match answer.status().as_u16() {
            200 => answer
                .body_mut()
                .with_config()
                .limit(self.limit)
                .read_to_vec()
                .map_err(|e| FetchError::Failed(e.to_string())),
            _ => Err(failure(&answer)),
        }
    }

    fn post(&self, url: &str, content_type: &str, body: &[u8]) -> Result<Vec<u8>, FetchError> {
        if offline() {
            return Err(FetchError::Offline);
        }
        let mut answer = self
            .agent()
            .post(url)
            .content_type(content_type)
            .header("Accept", "application/json")
            .send(body)
            .map_err(|e| FetchError::Failed(e.to_string()))?;
        match answer.status().as_u16() {
            200 => answer
                .body_mut()
                .with_config()
                .limit(self.limit)
                .read_to_vec()
                .map_err(|e| FetchError::Failed(e.to_string())),
            status => Err(status_error(status)),
        }
    }

    fn get_if_changed(&self, url: &str, etag: Option<&str>) -> Result<Fresh, FetchError> {
        if offline() {
            return Err(FetchError::Offline);
        }
        let mut request = self.agent().get(url);
        if let Some(etag) = etag.filter(|tag| !tag.is_empty()) {
            request = request.header("If-None-Match", etag);
        }
        let mut answer = request.call().map_err(|e| FetchError::Failed(e.to_string()))?;
        match answer.status().as_u16() {
            304 => Ok(Fresh::Unchanged),
            200 => {
                let tag = answer.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
                let body = answer
                    .body_mut()
                    .with_config()
                    .limit(self.limit)
                    .read_to_vec()
                    .map_err(|e| FetchError::Failed(e.to_string()))?;
                Ok(Fresh::Body(body, tag))
            }
            _ => Err(failure(&answer)),
        }
    }

    fn download(
        &self,
        url: &str,
        out: &mut dyn std::io::Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u64, FetchError> {
        use std::io::Read;
        if offline() {
            return Err(FetchError::Offline);
        }
        let mut answer = self.agent().get(url).call().map_err(|e| FetchError::Failed(e.to_string()))?;
        let status = answer.status().as_u16();
        if status != 200 {
            return Err(status_error(status));
        }
        let total = answer
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok());
        let mut reader = answer.body_mut().with_config().limit(self.limit).reader();
        let mut block = vec![0u8; DOWNLOAD_BLOCK];
        let mut written = 0u64;
        loop {
            let count = reader.read(&mut block).map_err(|e| FetchError::Failed(e.to_string()))?;
            if count == 0 {
                break;
            }
            out.write_all(&block[..count]).map_err(|e| FetchError::Failed(e.to_string()))?;
            written += count as u64;
            progress(written, total);
        }
        Ok(written)
    }

    fn head(&self, url: &str) -> Result<Headers, FetchError> {
        if offline() {
            return Err(FetchError::Offline);
        }
        let answer = self.agent().head(url).call().map_err(|e| FetchError::Failed(e.to_string()))?;
        match answer.status().as_u16() {
            200 => Ok(answer
                .headers()
                .iter()
                .map(|(name, value)| (name.as_str().to_lowercase(), value.to_str().unwrap_or("").to_string()))
                .collect()),
            status => Err(status_error(status)),
        }
    }
}

/// A stand-in for tests: fixed answers per address, everything else is
/// missing. Records what was asked.
#[derive(Default)]
pub struct Fake {
    pub bodies: Vec<(String, Vec<u8>)>,
    pub heads: Vec<(String, Headers)>,
    /// The tag of an address: asked with it, the address answers „unchanged“.
    pub etags: Vec<(String, String)>,
    /// Fixed answers to what is sent: address, body sent, answer.
    pub posts: Vec<(String, Vec<u8>, Vec<u8>)>,
    /// Addresses whose server has used up its limit: the seconds it asks to wait.
    pub limited: Vec<(String, u64)>,
    pub asked: std::sync::Mutex<Vec<String>>,
}

impl Fake {
    pub fn with(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
        self.bodies.push((url.to_string(), body.into()));
        self
    }

    pub fn with_head(mut self, url: &str, headers: &[(&str, &str)]) -> Self {
        self.heads.push((url.to_string(), headers.iter().map(|(k, v)| (k.to_lowercase(), v.to_string())).collect()));
        self
    }

    pub fn with_etag(mut self, url: &str, etag: &str) -> Self {
        self.etags.push((url.to_string(), etag.to_string()));
        self
    }

    /// `url` answers that its limit is used up and asks to wait `seconds`.
    pub fn with_rate_limit(mut self, url: &str, seconds: u64) -> Self {
        self.limited.push((url.to_string(), seconds));
        self
    }

    /// `url` answers `answer` to exactly `sent`.
    pub fn with_post(mut self, url: &str, sent: impl Into<Vec<u8>>, answer: impl Into<Vec<u8>>) -> Self {
        self.posts.push((url.to_string(), sent.into(), answer.into()));
        self
    }

    /// The addresses asked so far, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

impl Net for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(url.to_string());
        }
        if let Some((_, seconds)) = self.limited.iter().find(|(u, _)| u == url) {
            return Err(FetchError::RateLimited(*seconds));
        }
        self.bodies.iter().find(|(u, _)| u == url).map(|(_, b)| b.clone()).ok_or(FetchError::Missing)
    }

    fn post(&self, url: &str, _content_type: &str, body: &[u8]) -> Result<Vec<u8>, FetchError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(format!("POST {url} {}", String::from_utf8_lossy(body)));
        }
        self.posts.iter().find(|(u, s, _)| u == url && s == body).map(|(_, _, a)| a.clone()).ok_or(FetchError::Missing)
    }

    fn get_if_changed(&self, url: &str, etag: Option<&str>) -> Result<Fresh, FetchError> {
        let tag = self.etags.iter().find(|(u, _)| u == url).map(|(_, t)| t.clone());
        if etag.is_some() && etag == tag.as_deref() && self.bodies.iter().any(|(u, _)| u == url) {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(format!("{url} (unchanged)"));
            }
            return Ok(Fresh::Unchanged);
        }
        self.get(url).map(|body| Fresh::Body(body, tag))
    }

    fn head(&self, url: &str) -> Result<Headers, FetchError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(format!("HEAD {url}"));
        }
        self.heads.iter().find(|(u, _)| u == url).map(|(_, h)| h.clone()).ok_or(FetchError::Missing)
    }
}

#[cfg(test)]
mod tests {
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn headers(list: &[(&str, &str)]) -> super::Headers {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn a_used_up_limit_is_told_from_an_ordinary_refusal() {
        use super::rate_limit_wait as wait;
        assert_eq!(wait(429, &headers(&[]), 1000), Some(900), "429 without a hint");
        assert_eq!(wait(429, &headers(&[("retry-after", "120")]), 1000), Some(120));
        assert_eq!(
            wait(403, &headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1300")]), 1000),
            Some(300)
        );
        assert_eq!(wait(403, &headers(&[("x-ratelimit-remaining", "0")]), 1000), Some(900), "no reset named");
        assert_eq!(wait(403, &headers(&[("retry-after", "60")]), 1000), Some(60));
        assert_eq!(wait(403, &headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "900")]), 1000), Some(1));
        assert_eq!(wait(429, &headers(&[("retry-after", "999999")]), 1000), Some(3600), "never more than an hour");
        assert_eq!(wait(403, &headers(&[("x-ratelimit-remaining", "12")]), 1000), None, "a refusal with limit left");
        assert_eq!(wait(403, &headers(&[]), 1000), None);
        assert_eq!(wait(404, &headers(&[("x-ratelimit-remaining", "0")]), 1000), None);
    }

    #[test]
    fn the_fake_tells_a_used_up_limit() {
        use super::{Fake, FetchError, Net};
        let net = Fake::default().with_rate_limit("https://x/a", 600);
        assert_eq!(net.get_if_changed("https://x/a", None), Err(FetchError::RateLimited(600)));
        assert_eq!(net.get("https://x/b"), Err(FetchError::Missing));
    }

    #[test]
    fn no_identification_sent_names_the_old_program() {
        use crate::basetool::config::user_agent;
        let sent = [
            super::USER_AGENT.to_string(),
            crate::translation::USER_AGENT.to_string(),
            crate::updater::USER_AGENT.to_string(),
            user_agent(env!("CARGO_PKG_VERSION")),
            user_agent(""),
        ];
        for agent in sent {
            let lower = agent.to_lowercase();
            for old in ["sc-bp-watcher", "sc bp watcher", "scbpwatcher", "sc_bp_watcher", "ehemals", "watcher"] {
                assert!(!lower.contains(old), "{agent:?} names the old program ({old})");
            }
            assert!(agent.starts_with("VerseKit/"), "{agent:?}");
        }
    }

    #[test]
    fn the_shared_identification_is_the_one_of_the_basetool() {
        assert_eq!(super::USER_AGENT, crate::basetool::config::user_agent(env!("CARGO_PKG_VERSION")));
        assert_eq!(crate::translation::USER_AGENT, super::USER_AGENT);
        assert_eq!(crate::updater::USER_AGENT, super::USER_AGENT);
    }

    #[test]
    fn offline_comes_from_the_environment_given() {
        let host = |value: &str| Host::new(Flavor::native(), &[(super::ENV_NO_NET, value)], "/");
        assert!(super::offline_in(&host("1")));
        assert!(super::offline_in(&host("ja")));
        assert!(!super::offline_in(&host("0")));
        assert!(!super::offline_in(&host("")));
        assert!(!super::offline_in(&Host::new(Flavor::native(), &[], "/")));
    }
}
