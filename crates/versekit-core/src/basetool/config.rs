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

//! Where the Basetool is and who Verse-Kit is there
//! (`basetool.connection_config`, `trusted_address`, `user_agent`,
//! `default_label`, `label_valid`).
//!
//! ⚠ The issuer is fixed. The test environment is chosen only through
//! environment variables (`legacy::ENV_BASETOOL_*`), never in the user
//! interface: a switch there would be a lever for fraud.

use crate::basetool::jose::UrlParts;
use crate::legacy;
use crate::paths::Host;
use crate::text::py_strip;

pub const ISSUER: &str = "https://profit-base.online/auth/realms/basetool";
pub const API: &str = "https://ingest.profit-base.online/exchange/v1";
pub const CLIENT_ID: &str = "versekit";
pub const HOMEPAGE: &str = "https://versekit.xharig.com";

/// The variables that change the connection; any of them set shows a note.
pub const OVERRIDE_VARS: [&str; 4] =
    [legacy::ENV_BASETOOL_ISSUER, legacy::ENV_BASETOOL_API, legacy::ENV_BASETOOL_CLIENT, legacy::ENV_BASETOOL_CA];

/// Issuer, API address and client id; the real ones unless a test
/// environment is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub issuer: String,
    pub api: String,
    pub client_id: String,
    /// Certificate file of the test environment, otherwise empty.
    pub ca: String,
    /// Another issuer is set.
    pub sandbox: bool,
    /// Any of the variables is set.
    pub overridden: bool,
}

fn env<'a>(host: &'a Host, name: &str) -> Option<&'a str> {
    host.var(name).map(py_strip).filter(|v| !v.is_empty())
}

impl Config {
    /// API address, client id and certificate change only together with
    /// another issuer; without it they stay with the real Basetool.
    pub fn from_host(host: &Host) -> Config {
        let overridden = OVERRIDE_VARS.iter().any(|name| env(host, name).is_some());
        let Some(issuer) = env(host, legacy::ENV_BASETOOL_ISSUER) else {
            return Config {
                issuer: ISSUER.into(),
                api: API.into(),
                client_id: CLIENT_ID.into(),
                ca: String::new(),
                sandbox: false,
                overridden,
            };
        };
        Config {
            issuer: issuer.trim_end_matches('/').into(),
            api: env(host, legacy::ENV_BASETOOL_API).unwrap_or(API).trim_end_matches('/').into(),
            client_id: env(host, legacy::ENV_BASETOOL_CLIENT).unwrap_or(CLIENT_ID).into(),
            ca: env(host, legacy::ENV_BASETOOL_CA).unwrap_or_default().into(),
            sandbox: true,
            overridden: true,
        }
    }

    /// The configuration of this machine.
    pub fn current() -> Config {
        Config::from_host(&Host::current())
    }

    /// The hosts of issuer and API, the only ones the server may send the
    /// player to.
    pub fn trusted_hosts(&self) -> Vec<String> {
        [&self.issuer, &self.api].iter().filter_map(|base| UrlParts::split(base).ok()?.hostname()).collect()
    }

    /// `url` if it is an https address at the Basetool (issuer or API host,
    /// or a subdomain of them), otherwise empty. For addresses the server
    /// sends (login page, confirmation page) before they reach the browser.
    pub fn trusted_address(&self, url: &str) -> String {
        if web_address_on(url, &self.trusted_hosts()) { url.to_string() } else { String::new() }
    }
}

/// Whether `url` is an https address on one of `hosts` or a subdomain of
/// them (`paths.web_address` with a host list).
pub fn web_address_on(url: &str, hosts: &[String]) -> bool {
    if url.is_empty() || url.chars().any(|c| (c as u32) < 33 || c as u32 == 127 || c == '\\') {
        return false;
    }
    let Ok(parts) = UrlParts::split(url) else { return false };
    let Some(host) = parts.hostname() else { return false };
    if parts.scheme != "https" {
        return false;
    }
    hosts
        .iter()
        .map(|h| h.to_lowercase())
        .map(|h| h.trim_matches('.').to_string())
        .filter(|h| !h.is_empty())
        .any(|h| host == h || host.ends_with(&format!(".{h}")))
}

/// `VerseKit/<version> (+<homepage>)`.
pub fn user_agent(version: &str) -> String {
    format!("VerseKit/{} (+{HOMEPAGE})", if version.is_empty() { "0" } else { version })
}

/// The name of this installation in the Basetool: the system, never the
/// computer's name.
pub fn default_label() -> &'static str {
    if cfg!(windows) {
        "VerseKit Windows"
    } else if cfg!(target_os = "macos") {
        "VerseKit macOS"
    } else {
        "VerseKit Linux"
    }
}

/// Letters, digits, space, `-`, `_`, `.`; at most 40 characters; the first
/// one a letter, digit or `_`.
pub fn label_valid(label: &str) -> bool {
    let mut chars = label.chars();
    let Some(first) = chars.next() else { return false };
    let word = |c: char| c.is_alphanumeric() || c == '_';
    word(first) && label.chars().count() <= 40 && chars.all(|c| word(c) || matches!(c, ' ' | '.' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pypath::Flavor;

    fn host(vars: &[(&str, &str)]) -> Host {
        Host::new(Flavor::Posix, vars, "/")
    }

    #[test]
    fn real_basetool_unless_another_issuer() {
        let real = Config::from_host(&host(&[]));
        assert_eq!(
            (real.issuer.as_str(), real.api.as_str(), real.sandbox, real.overridden),
            (ISSUER, API, false, false)
        );
        // Without another issuer the API address stays, but the note shows.
        let api_only = Config::from_host(&host(&[(legacy::ENV_BASETOOL_API, "https://fremd.example/x")]));
        assert_eq!((api_only.api.as_str(), api_only.overridden), (API, true));
        let sandbox = Config::from_host(&host(&[
            (legacy::ENV_BASETOOL_ISSUER, " https://sandbox.example/realms/x/ "),
            (legacy::ENV_BASETOOL_CLIENT, "sandbox-client"),
        ]));
        assert_eq!(sandbox.issuer, "https://sandbox.example/realms/x");
        assert_eq!((sandbox.api.as_str(), sandbox.client_id.as_str(), sandbox.sandbox), (API, "sandbox-client", true));
    }

    #[test]
    fn only_basetool_addresses_are_trusted() {
        let config = Config::from_host(&host(&[]));
        for good in [
            "https://profit-base.online/device",
            "https://auth.profit-base.online/x",
            "https://INGEST.profit-base.online/c",
        ] {
            assert_eq!(config.trusted_address(good), good, "{good}");
        }
        for bad in [
            "http://profit-base.online/device",
            "https://profit-base.online.evil.example/",
            "https://evilprofit-base.online/",
            "https://profit-base.online\\@evil/",
            "https://profit-base.online /x",
            "javascript:alert(1)",
            "",
        ] {
            assert_eq!(config.trusted_address(bad), "", "{bad}");
        }
    }

    #[test]
    fn labels() {
        assert!(label_valid("VerseKit Windows"));
        assert!(label_valid("Rechner-2.ök"));
        assert!(label_valid("_x"));
        assert!(!label_valid(" Leer"));
        assert!(!label_valid("-strich"));
        assert!(!label_valid(""));
        assert!(!label_valid(&"x".repeat(41)));
        assert!(!label_valid("a/b"));
        assert_eq!(user_agent("5.0.0"), "VerseKit/5.0.0 (+https://versekit.xharig.com)");
    }
}
