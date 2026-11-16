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

//! The address a member types and the discovery document behind it.
//!
//! ⚠ Every address the org server names (API, login, the page to confirm on)
//! must lie on the host the member typed or below it, and use `https`. Plain
//! `http` only on the own computer, where tests run. A server that names
//! another host is refused: it would send the member's code or token there.

use serde_json::{Value, json};

use crate::basetool::jose::UrlParts;
use crate::orgstd::{E_ADDRESS, E_FOREIGN, E_INSECURE, E_NO_STANDARD, E_VERSION, OrgError, STANDARD, VERSIONS};

/// The longest org name and tag shown.
const NAME_LIMIT: usize = 80;
const TAG_LIMIT: usize = 12;

fn loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// The scheme, host and port of an address: what the member typed, reduced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub scheme: String,
    /// In lower case, without brackets.
    pub host: String,
    pub port: Option<u16>,
}

impl Origin {
    /// `https://host[:port]`, IPv6 in brackets.
    pub fn url(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        match self.port {
            Some(port) => format!("{}://{host}:{port}", self.scheme),
            None => format!("{}://{host}", self.scheme),
        }
    }

    /// Reads what the member typed: `orga.example.org`, `https://orga.example.org/panel` and the like. A path is
    /// dropped; a user name, spaces or a backslash refuse it.
    pub fn parse(typed: &str) -> Result<Origin, OrgError> {
        let typed = typed.trim();
        if typed.is_empty() || typed.chars().any(|c| (c as u32) < 33 || c as u32 == 127 || c == '\\') {
            return Err(OrgError::new(E_ADDRESS));
        }
        let full = if typed.contains("://") { typed.to_string() } else { format!("https://{typed}") };
        let parts = UrlParts::split(&full).map_err(|_| OrgError::new(E_ADDRESS))?;
        if parts.netloc.contains('@') {
            return Err(OrgError::new(E_ADDRESS));
        }
        let host = parts.hostname().filter(|h| !h.is_empty() && !h.contains('%')).ok_or(OrgError::new(E_ADDRESS))?;
        let port = parts.port().map_err(|_| OrgError::new(E_ADDRESS))?;
        match parts.scheme.as_str() {
            "https" => {}
            "http" if loopback(&host) => {}
            "http" => return Err(OrgError::new(E_INSECURE)),
            _ => return Err(OrgError::new(E_ADDRESS)),
        }
        Ok(Origin { scheme: parts.scheme, host, port })
    }

    /// Whether `url` may be used for this org: `https` on the same host or below it, or on the own computer the
    /// same host with the same scheme.
    pub fn allows(&self, url: &str) -> bool {
        if url.chars().any(|c| (c as u32) < 33 || c as u32 == 127 || c == '\\') {
            return false;
        }
        let Ok(parts) = UrlParts::split(url) else { return false };
        if parts.netloc.contains('@') {
            return false;
        }
        let Some(host) = parts.hostname() else { return false };
        if parts.port().is_err() {
            return false;
        }
        if self.scheme == "http" {
            return parts.scheme == "http" && host == self.host;
        }
        parts.scheme == "https" && (host == self.host || host.ends_with(&format!(".{}", self.host)))
    }

    /// The address of the discovery document.
    pub fn well_known(&self) -> String {
        format!("{}{}", self.url(), crate::orgstd::WELL_KNOWN)
    }
}

/// What the discovery document says, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub origin: Origin,
    pub name: String,
    pub tag: String,
    pub homepage: Option<String>,
    /// The version of the standard both speak.
    pub version: String,
    pub api_base: String,
    pub device_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    /// The rights the server offers that this client knows, in the order of [`crate::orgstd::SCOPES`].
    pub scopes: Vec<String>,
}

/// A text for the page: control characters gone, at most `limit` characters.
fn shown(text: &str, limit: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(limit).collect::<String>().trim().to_string()
}

impl Discovery {
    /// Checks a discovery document of `origin`.
    pub fn parse(origin: &Origin, root: &Value) -> Result<Discovery, OrgError> {
        if root.get("standard").and_then(Value::as_str) != Some(STANDARD) {
            return Err(OrgError::new(E_NO_STANDARD));
        }
        let org = root.get("org").ok_or(OrgError::new(E_NO_STANDARD))?;
        let name = org.get("name").and_then(Value::as_str).map(|n| shown(n, NAME_LIMIT)).unwrap_or_default();
        if name.is_empty() {
            return Err(OrgError::new(E_NO_STANDARD));
        }
        let tag = org.get("tag").and_then(Value::as_str).map(|t| shown(t, TAG_LIMIT)).unwrap_or_default();
        let homepage = org.get("homepage").and_then(Value::as_str).filter(|h| origin.allows(h)).map(str::to_string);
        let versions = root.get("versions").and_then(Value::as_object).ok_or(OrgError::new(E_NO_STANDARD))?;
        let (version, entry) = VERSIONS
            .iter()
            .find_map(|v| versions.get(*v).map(|entry| (v.to_string(), entry)))
            .ok_or(OrgError::new(E_VERSION))?;
        let address = |key: &str| -> Result<String, OrgError> {
            let url = entry.get(key).and_then(Value::as_str).ok_or(OrgError::new(E_NO_STANDARD))?;
            if origin.allows(url) { Ok(url.trim_end_matches('/').to_string()) } else { Err(OrgError::new(E_FOREIGN)) }
        };
        let revocation_endpoint = match entry.get("revocation_endpoint") {
            None | Some(Value::Null) => None,
            Some(_) => Some(address("revocation_endpoint")?),
        };
        let offered: Vec<&str> =
            entry.get("scopes_supported").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let scopes: Vec<String> =
            crate::orgstd::SCOPES.iter().filter(|s| offered.contains(s)).map(|s| s.to_string()).collect();
        if scopes.is_empty() {
            return Err(OrgError::new(E_NO_STANDARD));
        }
        Ok(Discovery {
            origin: origin.clone(),
            name,
            tag,
            homepage,
            version,
            api_base: address("api_base")?,
            device_endpoint: address("device_authorization_endpoint")?,
            token_endpoint: address("token_endpoint")?,
            revocation_endpoint,
            scopes,
        })
    }

    /// The fields as they are stored with a connection.
    pub fn to_json(&self) -> Value {
        json!({
            "adresse": self.origin.url(),
            "name": self.name,
            "kuerzel": self.tag,
            "homepage": self.homepage,
            "version": self.version,
            "api": self.api_base,
            "anmeldung": self.device_endpoint,
            "token": self.token_endpoint,
            "widerruf": self.revocation_endpoint,
            "rechte_angeboten": self.scopes,
        })
    }

    /// The stored fields, checked again against their address.
    pub fn from_json(value: &Value) -> Option<Discovery> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let origin = Origin::parse(&text("adresse")?).ok()?;
        let discovery = Discovery {
            name: text("name")?,
            tag: text("kuerzel").unwrap_or_default(),
            homepage: text("homepage"),
            version: text("version")?,
            api_base: text("api")?,
            device_endpoint: text("anmeldung")?,
            token_endpoint: text("token")?,
            revocation_endpoint: text("widerruf"),
            scopes: value
                .get("rechte_angeboten")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            origin,
        };
        let urls = [&discovery.api_base, &discovery.device_endpoint, &discovery.token_endpoint];
        let all_allowed = urls.into_iter().chain(discovery.revocation_endpoint.as_ref()).all(|u| discovery.origin.allows(u));
        all_allowed.then_some(discovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_addresses_become_an_origin_and_unsafe_ones_are_refused() {
        let cases = [
            ("orga.example.org", "https://orga.example.org"),
            ("  https://Orga.Example.org/panel/x?y#z ", "https://orga.example.org"),
            ("orga.example.org:8443", "https://orga.example.org:8443"),
            ("http://127.0.0.1:9000", "http://127.0.0.1:9000"),
            ("http://localhost", "http://localhost"),
            ("https://[::1]:7000", "https://[::1]:7000"),
        ];
        for (typed, url) in cases {
            assert_eq!(Origin::parse(typed).map(|o| o.url()), Ok(url.to_string()), "{typed}");
        }
        assert_eq!(Origin::parse("http://orga.example.org").unwrap_err().code, E_INSECURE);
        for bad in ["", "   ", "ftp://orga.example.org", "https://user@orga.example.org", "orga example.org", "a\\b", "https://"] // privacy-ok: reserved example domain
        {
            assert_eq!(Origin::parse(bad).unwrap_err().code, E_ADDRESS, "{bad:?}");
        }
    }

    #[test]
    fn only_addresses_on_the_typed_host_or_below_are_allowed() {
        let origin = Origin::parse("orga.example.org").unwrap();
        for good in ["https://orga.example.org/v1", "https://api.orga.example.org/x", "https://ORGA.example.org:444/"] {
            assert!(origin.allows(good), "{good}");
        }
        for bad in [
            "http://orga.example.org/v1",
            "https://example.org/v1",
            "https://orga.example.org.evil.example/",
            "https://evilorga.example.org/",
            "https://orga.example.org@evil.example/", // privacy-ok: reserved example domain
            "https://orga.example.org /x",
            "javascript:alert(1)",
        ] {
            assert!(!origin.allows(bad), "{bad}");
        }
        let local = Origin::parse("http://127.0.0.1:9000").unwrap();
        assert!(local.allows("http://127.0.0.1:9000/versekit/v1"));
        assert!(!local.allows("http://localhost:9000/versekit/v1"));
        assert!(!local.allows("https://127.0.0.1:9000/x"));
    }

    fn document(api: &str) -> Value {
        json!({
            "standard": "versekit-org",
            "org": { "name": "  Test\u{7}orga ", "tag": "TST" },
            "versions": {
                "9": { "api_base": "https://elsewhere.example/v9" },
                "1": {
                    "api_base": api,
                    "device_authorization_endpoint": "https://orga.example.org/v1/device/code",
                    "token_endpoint": "https://orga.example.org/v1/token",
                    "scopes_supported": ["orders:read", "fly:ships", "stock:read"],
                }
            }
        })
    }

    #[test]
    fn the_discovery_document_is_checked() {
        let origin = Origin::parse("orga.example.org").unwrap();
        let found = Discovery::parse(&origin, &document("https://orga.example.org/v1/")).unwrap();
        assert_eq!((found.name.as_str(), found.version.as_str(), found.api_base.as_str()), ("Testorga", "1", "https://orga.example.org/v1"));
        assert_eq!(found.scopes, ["stock:read", "orders:read"]);
        assert_eq!(found.revocation_endpoint, None);
        assert_eq!(Discovery::from_json(&found.to_json()), Some(found.clone()));
        assert_eq!(Discovery::parse(&origin, &document("https://evil.example/v1")).unwrap_err().code, E_FOREIGN);
        let mut other = document("https://orga.example.org/v1");
        other["standard"] = json!("something-else");
        assert_eq!(Discovery::parse(&origin, &other).unwrap_err().code, E_NO_STANDARD);
        let mut newer = document("https://orga.example.org/v1");
        newer["versions"].as_object_mut().unwrap().remove("1");
        assert_eq!(Discovery::parse(&origin, &newer).unwrap_err().code, E_VERSION);
        let mut stored = found.to_json();
        stored["api"] = json!("https://evil.example/v1");
        assert_eq!(Discovery::from_json(&stored), None);
    }
}
