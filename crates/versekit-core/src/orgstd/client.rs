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

//! The requests of the standard: discovery, the device login (RFC 8628),
//! refreshing and revoking tokens (RFC 6749, RFC 7009) and the API with a
//! bearer token and `next_cursor` paging.
//!
//! An access token that runs out within [`EARLY`] seconds is refreshed before
//! the request; a `401` refreshes once and tries again. A refresh token the
//! server no longer knows ends the connection (`RELOGIN`).

use std::sync::Arc;

use serde_json::{Value, json};

use crate::basetool::config::user_agent;
use crate::basetool::connection::urlencode;
use crate::basetool::http::{Request, Response, Transport};
use crate::orgstd::discovery::{Discovery, Origin};
use crate::orgstd::store::Connection;
use crate::orgstd::{
    CLIENT_ID, DEVICE_GRANT, E_ANSWER, E_DENIED, E_EXPIRED, E_FOREIGN, E_NETWORK, E_NO_STANDARD, E_RATE, E_RELOGIN,
    E_SCOPE, E_SERVER, OrgError,
};

/// Seconds before its end at which an access token is refreshed ahead of a request.
pub const EARLY: u64 = 30;
/// The most pages read of one list.
pub const PAGE_LIMIT: usize = 200;
/// Device codes and polling without numbers from the server.
const DEFAULT_EXPIRES: u64 = 600;
const DEFAULT_INTERVAL: u64 = 5;
const DEFAULT_ACCESS: u64 = 3600;

/// A login that waits for the member: the code to show and where to confirm it.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    /// The bare address of the page to confirm on; checked against the org's host.
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl std::fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCode")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

/// The tokens of a login or a refresh.
#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_in: u64,
    /// The granted rights; empty when the server did not name them.
    pub scopes: Vec<String>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens").field("expires_in", &self.expires_in).field("scopes", &self.scopes).finish_non_exhaustive()
    }
}

/// One poll of a waiting login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    Pending,
    /// Wait five seconds longer from now on.
    SlowDown,
    Done(Tokens),
}

/// `GET /me`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Me {
    pub member: String,
    pub scopes: Vec<String>,
}

/// The client of one VerseKit installation.
#[derive(Clone)]
pub struct OrgClient {
    transport: Arc<dyn Transport>,
    user_agent: String,
}

fn json_of(response: &Response) -> Option<Value> {
    serde_json::from_slice(&response.body).ok()
}

fn header(response: &Response, name: &str) -> Option<String> {
    response.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
}

/// The error of a refused API request.
fn refusal(response: &Response) -> OrgError {
    let retry_after = header(response, "Retry-After").and_then(|v| v.trim().parse::<u64>().ok()).map(|s| s.max(1));
    let code = match response.status {
        401 => E_RELOGIN,
        403 => E_SCOPE,
        429 => E_RATE,
        _ => E_SERVER,
    };
    OrgError { code, status: response.status, retry_after }
}

/// A user code as it may be shown: letters, digits and dashes, at most 20.
fn user_code_ok(code: &str) -> bool {
    !code.is_empty() && code.len() <= 20 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn tokens_of(body: &Value) -> Result<Tokens, OrgError> {
    let text = |key: &str| body.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let bearer = body.get("token_type").and_then(Value::as_str).is_some_and(|t| t.eq_ignore_ascii_case("bearer"));
    let (Some(access), Some(refresh), true) = (text("access_token"), text("refresh_token"), bearer) else {
        return Err(OrgError::new(E_ANSWER));
    };
    Ok(Tokens {
        access,
        refresh,
        expires_in: body.get("expires_in").and_then(Value::as_u64).unwrap_or(DEFAULT_ACCESS),
        scopes: text("scope").map(|s| s.split(' ').filter(|p| !p.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
    })
}

impl OrgClient {
    /// `version` goes into the `User-Agent`.
    pub fn new(transport: Arc<dyn Transport>, version: &str) -> OrgClient {
        OrgClient { transport, user_agent: user_agent(version) }
    }

    fn send(&self, method: &str, url: &str, extra: &[(&str, &str)], body: Option<(&str, Vec<u8>)>) -> Result<Response, OrgError> {
        let mut headers = vec![("User-Agent".to_string(), self.user_agent.clone()), ("Accept".into(), "application/json".into())];
        headers.extend(extra.iter().map(|(n, v)| (n.to_string(), v.to_string())));
        let body = body.map(|(kind, bytes)| {
            headers.push(("Content-Type".into(), kind.to_string()));
            bytes
        });
        let request = Request { method: method.into(), url: url.into(), headers, body };
        self.transport.send(&request).map_err(|_| OrgError::new(E_NETWORK))
    }

    fn form(&self, url: &str, pairs: &[(&str, &str)]) -> Result<Response, OrgError> {
        self.send("POST", url, &[], Some(("application/x-www-form-urlencoded", urlencode(pairs).into_bytes())))
    }

    /// Reads the discovery document behind what the member typed.
    pub fn discover(&self, typed: &str) -> Result<Discovery, OrgError> {
        let origin = Origin::parse(typed)?;
        let response = self.send("GET", &origin.well_known(), &[], None)?;
        match response.status {
            200 => Discovery::parse(&origin, &json_of(&response).ok_or(OrgError::new(E_NO_STANDARD))?),
            404 | 410 => Err(OrgError::new(E_NO_STANDARD).with_status(response.status)),
            300..=399 => Err(OrgError::new(E_FOREIGN).with_status(response.status)),
            _ => Err(refusal(&response)),
        }
    }

    /// Asks for a code for the member to confirm; asks for every right the server offers.
    pub fn start_login(&self, discovery: &Discovery) -> Result<DeviceCode, OrgError> {
        let scope = discovery.scopes.join(" ");
        let response = self.form(&discovery.device_endpoint, &[("client_id", CLIENT_ID), ("scope", &scope)])?;
        if response.status != 200 {
            return Err(refusal(&response));
        }
        let body = json_of(&response).ok_or(OrgError::new(E_ANSWER))?;
        let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_string);
        let (Some(device_code), Some(user_code), Some(verification_uri)) =
            (text("device_code"), text("user_code"), text("verification_uri"))
        else {
            return Err(OrgError::new(E_ANSWER));
        };
        if device_code.is_empty() || !user_code_ok(&user_code) {
            return Err(OrgError::new(E_ANSWER));
        }
        if !discovery.origin.allows(&verification_uri) {
            return Err(OrgError::new(E_FOREIGN));
        }
        Ok(DeviceCode {
            device_code,
            user_code,
            verification_uri,
            expires_in: body.get("expires_in").and_then(Value::as_u64).unwrap_or(DEFAULT_EXPIRES),
            interval: body.get("interval").and_then(Value::as_u64).unwrap_or(DEFAULT_INTERVAL),
        })
    }

    /// One poll of the login.
    pub fn poll_login(&self, discovery: &Discovery, code: &DeviceCode) -> Result<Poll, OrgError> {
        let pairs = [("grant_type", DEVICE_GRANT), ("device_code", code.device_code.as_str()), ("client_id", CLIENT_ID)];
        let response = self.form(&discovery.token_endpoint, &pairs)?;
        if response.status == 200 {
            return tokens_of(&json_of(&response).ok_or(OrgError::new(E_ANSWER))?).map(Poll::Done);
        }
        let error = json_of(&response).and_then(|b| b.get("error").and_then(Value::as_str).map(str::to_string));
        match error.as_deref() {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => Ok(Poll::SlowDown),
            Some("access_denied") => Err(OrgError::new(E_DENIED)),
            Some("expired_token") => Err(OrgError::new(E_EXPIRED)),
            _ => Err(refusal(&response)),
        }
    }

    /// New tokens for a refresh token; the old one is worthless afterwards.
    pub fn refresh(&self, discovery: &Discovery, refresh_token: &str) -> Result<Tokens, OrgError> {
        let pairs = [("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", CLIENT_ID)];
        let response = self.form(&discovery.token_endpoint, &pairs)?;
        match response.status {
            200 => tokens_of(&json_of(&response).ok_or(OrgError::new(E_ANSWER))?),
            400 | 401 => Err(OrgError::new(E_RELOGIN).with_status(response.status)),
            _ => Err(refusal(&response)),
        }
    }

    /// Tells the server to forget the connection; a server without a revocation endpoint has nothing to forget.
    pub fn revoke(&self, discovery: &Discovery, refresh_token: &str) -> Result<(), OrgError> {
        let Some(endpoint) = &discovery.revocation_endpoint else { return Ok(()) };
        let pairs = [("token", refresh_token), ("token_type_hint", "refresh_token"), ("client_id", CLIENT_ID)];
        let response = self.form(endpoint, &pairs)?;
        if response.status == 200 { Ok(()) } else { Err(refusal(&response)) }
    }

    /// Takes `tokens` into `connection`, the time `now` in seconds since 1970.
    pub fn take_tokens(connection: &mut Connection, tokens: Tokens, now: u64) {
        connection.access_token = tokens.access;
        connection.refresh_token = tokens.refresh;
        connection.access_expires = now + tokens.expires_in;
        if !tokens.scopes.is_empty() {
            connection.scopes = tokens.scopes;
        }
    }

    fn renew(&self, connection: &mut Connection, now: u64) -> Result<(), OrgError> {
        let tokens = self.refresh(&connection.discovery, &connection.refresh_token)?;
        OrgClient::take_tokens(connection, tokens, now);
        Ok(())
    }

    /// One API request at `path` below the API address (`/me`, `/stock?cursor=…`). The tokens of `connection` may
    /// change on the way: whoever stores the connection stores it afterwards, also after an error.
    pub fn call(
        &self,
        connection: &mut Connection,
        method: &str,
        path: &str,
        body: Option<&Value>,
        extra: &[(&str, &str)],
        now: u64,
    ) -> Result<Value, OrgError> {
        if connection.access_expires <= now + EARLY {
            self.renew(connection, now)?;
        }
        let url = format!("{}{path}", connection.discovery.api_base);
        let bytes = body.map(|b| ("application/json", b.to_string().into_bytes()));
        let attempt = |connection: &Connection| {
            let bearer = format!("Bearer {}", connection.access_token);
            let mut headers = vec![("Authorization", bearer.as_str())];
            headers.extend_from_slice(extra);
            self.send(method, &url, &headers, bytes.clone())
        };
        let mut response = attempt(connection)?;
        if response.status == 401 {
            self.renew(connection, now)?;
            response = attempt(connection)?;
        }
        if (200..300).contains(&response.status) {
            return Ok(json_of(&response).unwrap_or(Value::Null));
        }
        Err(refusal(&response))
    }

    /// Every entry of a list (`key` in the answer), following `next_cursor`.
    pub fn list(&self, connection: &mut Connection, path: &str, key: &str, now: u64) -> Result<Vec<Value>, OrgError> {
        let mut entries = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..PAGE_LIMIT {
            let page_path = match &cursor {
                Some(c) => format!("{path}?{}", urlencode(&[("cursor", c)])),
                None => path.to_string(),
            };
            let page = self.call(connection, "GET", &page_path, None, &[], now)?;
            let items = page.get(key).and_then(Value::as_array).ok_or(OrgError::new(E_ANSWER))?;
            entries.extend(items.iter().cloned());
            match page.get("next_cursor").and_then(Value::as_str) {
                Some(next) if Some(next) != cursor.as_deref() => cursor = Some(next.to_string()),
                _ => return Ok(entries),
            }
        }
        Err(OrgError::new(E_ANSWER))
    }

    /// Who the member is and which rights the connection has.
    pub fn me(&self, connection: &mut Connection, now: u64) -> Result<Me, OrgError> {
        let body = self.call(connection, "GET", "/me", None, &[], now)?;
        let member = body.pointer("/member/name").and_then(Value::as_str).ok_or(OrgError::new(E_ANSWER))?;
        let scopes = body.get("scopes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
        Ok(Me { member: member.chars().filter(|c| !c.is_control()).take(80).collect(), scopes: scopes.map(str::to_string).collect() })
    }

    /// The stock the members of the org share.
    pub fn org_stock(&self, connection: &mut Connection, now: u64) -> Result<Vec<Value>, OrgError> {
        self.list(connection, "/stock", "items", now)
    }

    /// Replaces the stock this member shares (`[{"item", "quantity", "unit"?, "quality"?, "location"?}]`).
    pub fn share_stock(&self, connection: &mut Connection, items: &[Value], now: u64) -> Result<Value, OrgError> {
        self.call(connection, "PUT", "/stock/mine", Some(&json!({ "items": items })), &[], now)
    }

    /// The blueprints of the org with who holds them.
    pub fn org_blueprints(&self, connection: &mut Connection, now: u64) -> Result<Vec<Value>, OrgError> {
        self.list(connection, "/blueprints", "blueprints", now)
    }

    /// Replaces the blueprints this member shares.
    pub fn share_blueprints(&self, connection: &mut Connection, names: &[String], now: u64) -> Result<Value, OrgError> {
        self.call(connection, "PUT", "/blueprints/mine", Some(&json!({ "blueprints": names })), &[], now)
    }

    /// The orders of the org.
    pub fn orders(&self, connection: &mut Connection, now: u64) -> Result<Vec<Value>, OrgError> {
        self.list(connection, "/orders", "orders", now)
    }

    /// Books a delivery to an order. `key` makes a repeat harmless: the server answers it as the first time.
    pub fn deliver(
        &self,
        connection: &mut Connection,
        order: &str,
        items: &[(String, f64)],
        key: &str,
        now: u64,
    ) -> Result<Value, OrgError> {
        let items: Vec<Value> = items.iter().map(|(item, quantity)| json!({ "item": item, "quantity": quantity })).collect();
        let path = format!("/orders/{}/deliveries", urlencode(&[("", order)]).trim_start_matches('='));
        self.call(connection, "POST", &path, Some(&json!({ "items": items })), &[("Idempotency-Key", key)], now)
    }
}
