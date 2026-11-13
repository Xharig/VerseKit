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

//! Fetching a data file of scmdb.net at run time (`catalog.fetch_file` of
//! 4.12): first from scmdb's public mirror for programs, then from
//! scmdb.net itself.
//!
//! Nothing of it is shipped. scmdb is CC BY-NC-ND 4.0; its operator allows
//! fetching on the user's computer from the original address, not passing
//! the files on (the raw data belong to CIG). With `SC_BP_NO_NET` set,
//! nothing is fetched.

use std::sync::Arc;
use std::time::Duration;

use crate::legacy::ENV_NO_NET;

/// The public mirror, the first choice: no bot protection in front of it.
pub const MIRROR: &str = "https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data";

/// scmdb.net itself, when the mirror fails or lags behind.
pub const SCMDB: &str = "https://scmdb.net/data";

/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The largest file read.
const BODY_LIMIT: u64 = 64 * 1024 * 1024;

/// Why a file could not be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Fetching is switched off (`SC_BP_NO_NET`).
    Offline,
    /// The file is not there (HTTP 404) at either address.
    Missing,
    /// The server refused (HTTP 403): asking again does not help.
    Refused,
    /// The server's hourly limit is used up (HTTP 403 or 429 with a limit header): the seconds to wait.
    RateLimited(u64),
    /// Another answer or a network error, with what it was.
    Failed(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline => write!(f, "fetching is switched off ({ENV_NO_NET})"),
            Self::Missing => write!(f, "the file is not there"),
            Self::Refused => write!(f, "the server refused the request"),
            Self::RateLimited(wait) => write!(f, "rate limit reached, try again in {wait} s"),
            Self::Failed(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for FetchError {}

fn offline() -> bool {
    std::env::var(ENV_NO_NET).is_ok_and(|value| !value.is_empty() && value != "0")
}

pub(crate) fn agent(user_agent: &str) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Every address is https; a redirect must not lead to plain http.
        .https_only(true)
        .timeout_global(Some(TIMEOUT))
        .user_agent(user_agent)
        .tls_config(tls)
        .build()
        .new_agent()
}

pub(crate) fn fetch_from(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, FetchError> {
    let mut answer = agent.get(url).call().map_err(|error| FetchError::Failed(error.to_string()))?;
    match answer.status().as_u16() {
        200 => answer
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_vec()
            .map_err(|error| FetchError::Failed(error.to_string())),
        403 => Err(FetchError::Refused),
        404 => Err(FetchError::Missing),
        status => Err(FetchError::Failed(format!("HTTP {status}"))),
    }
}

/// The file `name` (e.g. `cig_data_issues-4.10.2-live.12913470.json`): from
/// the mirror, else from scmdb.net; `user_agent` names VerseKit
/// (`basetool::config::user_agent`).
///
/// # Errors
///
/// [`FetchError::Offline`] with `SC_BP_NO_NET` set; otherwise the error of
/// scmdb.net, or [`FetchError::Missing`] when neither address has the file.
pub fn fetch_file(name: &str, user_agent: &str) -> Result<Vec<u8>, FetchError> {
    // The guard stands before the switch, so it holds with the network off too.
    if name.is_empty() || name.contains(['/', '\\', '?', '#']) || name.contains("..") {
        return Err(FetchError::Failed(format!("not a file name: {name}")));
    }
    if offline() {
        return Err(FetchError::Offline);
    }
    let agent = agent(user_agent);
    let mut last = FetchError::Missing;
    for base in [MIRROR, SCMDB] {
        match fetch_from(&agent, &format!("{base}/{name}")) {
            // scmdb.net answers a file it does not have with the page of its app (HTTP 200): not there, as a 404.
            Ok(bytes) if is_page_for(name, &bytes) => last = FetchError::Missing,
            Ok(bytes) => return Ok(bytes),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Whether `bytes`, fetched for the data file `name`, are a web page instead: the file is a `.json` and the answer
/// starts (after a byte order mark and white space) with `<`, which no JSON does.
fn is_page_for(name: &str, bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    name.to_ascii_lowercase().ends_with(".json") && bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_is_refused_before_anything_is_asked() {
        // `https_only` holds for every hop, so a redirect to http dies the same way as a direct http address.
        let error = fetch_from(&agent("VerseKit/0"), "http://127.0.0.1:9/data.json").unwrap_err();
        let FetchError::Failed(what) = error else { panic!("{error:?}") };
        assert!(what.to_lowercase().contains("https"), "refused for the scheme, not for the connection: {what}");
    }

    /// scmdb.net answers `merged-4.10.0-live.13165946.json`, which it does not have, with the page of its app and
    /// HTTP 200: that is a file not there, not a broken one.
    #[test]
    fn a_web_page_for_a_json_file_counts_as_not_there() {
        let page = b"<!DOCTYPE html>\n<html lang=\"en\"><head><title>SCMDB</title></head></html>";
        assert!(is_page_for("merged-4.10.0-live.13165946.json", page));
        assert!(is_page_for("versions.json", b"\xEF\xBB\xBF \r\n <html></html>"), "after a mark and white space");
        assert!(!is_page_for("merged-4.10.3-live.13165946.json", b"{\n  \"version\": \"4.10.3-live.13165946\"}"));
        assert!(!is_page_for("versions.json", b"[{\"version\": \"4.10.3-live.13165946\"}]"));
        assert!(!is_page_for("versions.json", b""), "an empty answer is no page");
        assert!(!is_page_for("notes.txt", page), "only a JSON file is told from a page");
    }

    /// Against the network (run by hand with `--ignored`): a build scmdb does not name is not there at either
    /// address, the one it names is a JSON file.
    #[test]
    #[ignore = "asks the network"]
    fn a_build_scmdb_does_not_have_is_not_there_at_the_real_addresses() {
        assert_eq!(fetch_file("merged-4.10.0-live.13165946.json", "VerseKit/0"), Err(FetchError::Missing));
        let versions = fetch_file("versions.json", "VerseKit/0").expect("the list of builds");
        assert!(!is_page_for("versions.json", &versions));
    }

    #[test]
    fn a_path_is_no_file_name() {
        // Refused before anything is asked, whether the shell switched the network off or not.
        for name in ["", "../x.json", "a/b.json", "x.json?y", "a\\b"] {
            assert!(matches!(fetch_file(name, "VerseKit/0"), Err(FetchError::Failed(_))), "{name}");
        }
    }
}
