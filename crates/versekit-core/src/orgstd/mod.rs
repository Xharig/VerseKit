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

//! The client of the open org standard `versekit-org` v1
//! (`tools/orga-standard/v1/openapi.yaml`): any org server that follows it
//! can be connected by its address, next to the fixed Basetool connection.
//!
//! | Module | Does |
//! |---|---|
//! | `discovery` | the address the member types, `/.well-known/versekit-org.json`, which addresses are allowed |
//! | `client` | device login, refreshing and revoking tokens, the API calls with paging |
//! | `data` | the rows of the lists (stock, blueprints, orders), checked one by one; only ever in memory |
//! | `share` | what of the own stock and blueprints goes out, as pure functions |
//! | `store` | the connections and their tokens in the user's own folder, never in the data folder |
//! | `hub` | the work in the background for the page "Connect org": login, check, disconnect |
//!
//! ⚠ No token or device code leaves these modules: not into a log, an error,
//! the report or the backup. `Debug` of every type that holds one hides it.

pub mod areas;
pub mod client;
pub mod data;
pub mod discovery;
pub mod hub;
pub mod share;
pub mod store;

/// The name of the standard in the discovery document.
pub const STANDARD: &str = "versekit-org";
/// The versions of the standard this client speaks, the newest first.
pub const VERSIONS: [&str; 1] = ["1"];
/// The path of the discovery document on the org's host.
pub const WELL_KNOWN: &str = "/.well-known/versekit-org.json";
/// The `client_id` VerseKit sends.
pub const CLIENT_ID: &str = "versekit";
/// The grant type of the device login (RFC 8628).
pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// The rights of version 1 that VerseKit asks for, when the server offers them.
pub const SCOPES: [&str; 6] =
    ["stock:read", "stock:write", "blueprints:read", "blueprints:write", "orders:read", "orders:write"];

/// Why something with an org server did not work. Carries a code and the
/// status only, never a header, a body or a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrgError {
    pub code: &'static str,
    /// HTTP status; 0 without an answer.
    pub status: u16,
    /// Seconds from `Retry-After`.
    pub retry_after: Option<u64>,
}

impl OrgError {
    pub const fn new(code: &'static str) -> OrgError {
        OrgError { code, status: 0, retry_after: None }
    }

    pub const fn with_status(mut self, status: u16) -> OrgError {
        self.status = status;
        self
    }
}

impl std::fmt::Display for OrgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.status == 0 { write!(f, "{}", self.code) } else { write!(f, "{} ({})", self.code, self.status) }
    }
}

impl std::error::Error for OrgError {}

/// The address cannot be read or is not allowed.
pub const E_ADDRESS: &str = "ADDRESS";
/// Plain `http` to another computer.
pub const E_INSECURE: &str = "INSECURE";
/// No answer: no network, no such host, a timeout, a certificate refused.
pub const E_NETWORK: &str = "NETWORK";
/// The host answers, but not as an org server of the standard.
pub const E_NO_STANDARD: &str = "NO_STANDARD";
/// The org server speaks no version this client knows.
pub const E_VERSION: &str = "VERSION";
/// The org server names an address on another host.
pub const E_FOREIGN: &str = "FOREIGN";
/// An answer of an unexpected shape.
pub const E_ANSWER: &str = "ANSWER";
/// The member refused the login in the panel.
pub const E_DENIED: &str = "DENIED";
/// The code ran out before the member confirmed it.
pub const E_EXPIRED: &str = "EXPIRED";
/// The connection is no longer valid; only a new login helps.
pub const E_RELOGIN: &str = "RELOGIN";
/// The member did not grant the right for this.
pub const E_SCOPE: &str = "SCOPE";
/// Too many requests; again after `retry_after`.
pub const E_RATE: &str = "RATE";
/// Any other refusal of the server.
pub const E_SERVER: &str = "SERVER";
/// The connection could not be stored.
pub const E_STORE: &str = "STORE";
/// The connection is not there (any more).
pub const E_GONE: &str = "GONE";
/// More lines than the standard takes (only found here, never an answer of a server).
pub const E_LIMIT: &str = "LIMIT";
/// A list without a line that would replace what the org holds (only found here).
pub const E_EMPTY: &str = "EMPTY";
/// Fetching is switched off (`VERSEKIT_NO_NET`) and the org lies on another computer (only found here).
pub const E_OFFLINE: &str = "OFFLINE";
